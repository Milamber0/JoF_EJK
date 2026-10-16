//! GIFs in SJK chat (`docs/hub-chat.md`, "GIFs from GIPHY"): a message linking to a
//! GIPHY GIF shows the GIF, animated, under its line on the SJK chat page and the docks.
//! Each reader's client fetches it from GIPHY's media host itself; the hub carries only
//! the text.
//!
//! [`for_message`] finds the first GIPHY link of a message ([`link`]) and words the
//! line with "GIF" in its place, unless GIFs are off (`cl_sjkChatGifs 0`) or the sender
//! is muted, whose GIFs are never fetched. A screen asks for the GIF with [`show`]: the
//! first time, a worker thread fetches it ([`fetch`]) and decodes it ([`decode`]);
//! [`service`], once a frame, takes what the worker finished and uploads the frame each
//! GIF on screen is at into the UI renderer's GIF textures. Until then the screen draws
//! a placeholder, and a GIF that could not be had says so. Nothing here waits: the
//! frame thread compares ids and copies finished pixels.
//!
//! The decoded GIFs stay in memory only, by id, so a GIF in several messages loads once:
//! at most [`ENTRIES`] GIFs and [`BUDGET`] bytes of pixels, those used least recently
//! making room. [`GPU_SLOTS`] of them can be on screen at once. The GIF picker's tiles
//! ([`show_preview`]) are kept apart from the messages' GIFs, as GIPHY's smaller
//! preview file ([`link::Rendition::Preview`]); [`WORKERS`] threads fetch, so a page of
//! tiles fills together. [`favourites`] keeps the player's favourite GIFs and
//! [`search`] asks GIPHY's search for the picker.

pub(crate) mod decode;
pub(crate) mod draw;
pub(crate) mod favourites;
pub(crate) mod fetch;
pub(crate) mod link;
pub(crate) mod search;

pub(crate) use link::GifId;

use decode::Gif;
use sjk_ui::TextureId;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// `cl_sjkChatGifs`: whether GIPHY links in SJK chat show their GIF.
pub(crate) const CVAR: &str = "cl_sjkChatGifs";
/// GIFs kept decoded at once, the picker's tiles included.
pub(crate) const ENTRIES: usize = 48;
/// Bytes of pixels kept for every decoded GIF together (four GIFs at the largest
/// [`decode::MAX_DECODED`]).
pub(crate) const BUDGET: usize = 128 << 20;
/// GIFs drawn at once, each with a texture of its own: a page of the picker's tiles
/// and the messages' GIFs.
pub(crate) const GPU_SLOTS: usize = 24;
/// Threads fetching and decoding GIFs.
pub(crate) const WORKERS: usize = 3;
/// How long a GIF that could not be had waits before it is asked for again.
const RETRY_AFTER: Duration = Duration::from_secs(300);
/// Where the GIF textures' ids begin (see [`slot_texture`]).
const TEXTURE_BASE: u32 = 0x7000_0000;

static ENABLED: AtomicBool = AtomicBool::new(true);

/// Follow `cl_sjkChatGifs` (once a frame).
pub(crate) fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Whether GIPHY links show their GIF.
pub(crate) fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// A message's `text` (as a line shows it) and its GIF: the first GIPHY link made "GIF"
/// with the GIF under it, or the text as it is and none when GIFs are off, the sender
/// is muted (`muted`, whose GIFs are never fetched) or there is no such link.
pub(crate) fn for_message(text: &str, muted: bool) -> (String, Option<GifId>) {
    for_message_with(text, muted, enabled())
}

/// [`for_message`] with GIFs on or off (`on`).
fn for_message_with(text: &str, muted: bool, on: bool) -> (String, Option<GifId>) {
    if muted || !on {
        return (text.to_owned(), None);
    }
    link::with_label(text)
}

/// A GIF as the cache keeps it: a message's GIF, or a picker tile's (`preview`), the
/// smaller file.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Key {
    id: GifId,
    preview: bool,
}

impl Key {
    fn chat(id: &GifId) -> Self {
        Self {
            id: id.clone(),
            preview: false,
        }
    }
}

/// What a screen draws for a GIF.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Shown {
    /// Being fetched or decoded, or not uploaded yet: a placeholder.
    Loading,
    /// The GIF's texture, its frame of the moment, and its size in pixels.
    Ready { texture: TextureId, size: [u32; 2] },
    /// It could not be had: "GIF unavailable".
    Unavailable,
}

/// The texture of GIF slot `slot`.
pub(crate) fn slot_texture(slot: usize) -> TextureId {
    debug_assert!(slot < GPU_SLOTS);
    TextureId(TEXTURE_BASE + slot as u32)
}

/// The GIF slot a `TexturedQuad` texture names, if it names one.
pub(crate) fn slot_of(texture: TextureId) -> Option<usize> {
    let slot = texture.0.checked_sub(TEXTURE_BASE)? as usize;
    (slot < GPU_SLOTS).then_some(slot)
}

/// Where a GIF is.
#[derive(Clone, Debug)]
enum State {
    /// Asked of the worker.
    Loading,
    Ready(Arc<Gif>),
    /// The worker could not get it, at that time.
    Failed(Instant),
}

/// One GIF in the cache.
#[derive(Debug)]
struct Entry {
    key: Key,
    state: State,
    /// The frame it was last asked for in.
    used: u64,
}

/// A GIF texture of the renderer and the GIF it holds.
#[derive(Clone, Debug)]
struct Slot {
    key: Key,
    used: u64,
    /// The renderer that has it, and the frame of the GIF it was given.
    uploaded: Option<(u64, usize)>,
}

/// The cache, its worker and the textures' use.
struct Gifs {
    entries: Vec<Entry>,
    slots: [Option<Slot>; GPU_SLOTS],
    /// Counts frames ([`service`]).
    frame: u64,
    /// The renderer the last frame drew with.
    renderer: Option<u64>,
    requests: Option<Sender<Key>>,
    answers: Option<Receiver<(Key, Option<Gif>)>>,
    /// When the animations began (the first [`service`]).
    started: Option<Instant>,
    /// The animations held at this time (the world shots).
    held_ms: Option<u64>,
}

static GIFS: Mutex<Gifs> = Mutex::new(Gifs::new());

fn lock() -> MutexGuard<'static, Gifs> {
    GIFS.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Gifs {
    const fn new() -> Self {
        Self {
            entries: Vec::new(),
            slots: [const { None }; GPU_SLOTS],
            frame: 1,
            renderer: None,
            requests: None,
            answers: None,
            started: None,
            held_ms: None,
        }
    }

    /// What to draw for `id`, asking for it when the cache does not have it.
    fn show(&mut self, id: &Key) -> Shown {
        let frame = self.frame;
        let Some(index) = self.entries.iter().position(|entry| entry.key == *id) else {
            self.entries.push(Entry {
                key: id.clone(),
                state: State::Loading,
                used: frame,
            });
            self.ask(id);
            self.evict();
            return Shown::Loading;
        };
        let entry = &mut self.entries[index];
        entry.used = frame;
        match &entry.state {
            State::Loading => Shown::Loading,
            State::Failed(since) if since.elapsed() >= RETRY_AFTER => {
                entry.state = State::Loading;
                self.ask(id);
                Shown::Loading
            }
            State::Failed(_) => Shown::Unavailable,
            State::Ready(gif) => {
                let size = gif.size;
                let Some(slot) = self.slot_for(id) else {
                    return Shown::Loading;
                };
                let uploaded = self.slots[slot]
                    .as_ref()
                    .and_then(|slot| slot.uploaded)
                    .is_some_and(|(renderer, _)| Some(renderer) == self.renderer);
                if uploaded {
                    Shown::Ready {
                        texture: slot_texture(slot),
                        size,
                    }
                } else {
                    Shown::Loading
                }
            }
        }
    }

    /// The GPU slot holding `id`, given one if it has none: a free one, else the one
    /// used longest ago, as long as it was not used this frame or the last.
    fn slot_for(&mut self, id: &Key) -> Option<usize> {
        let frame = self.frame;
        if let Some(index) = self
            .slots
            .iter()
            .position(|slot| slot.as_ref().is_some_and(|slot| slot.key == *id))
        {
            if let Some(slot) = self.slots[index].as_mut() {
                slot.used = frame;
            }
            return Some(index);
        }
        let index = self.slots.iter().position(Option::is_none).or_else(|| {
            self.slots
                .iter()
                .enumerate()
                .filter_map(|(index, slot)| Some((index, slot.as_ref()?.used)))
                .filter(|(_, used)| used + 1 < frame)
                .min_by_key(|(_, used)| *used)
                .map(|(index, _)| index)
        })?;
        self.slots[index] = Some(Slot {
            key: id.clone(),
            used: frame,
            uploaded: None,
        });
        Some(index)
    }

    /// Ask the workers for `id`, starting them the first time.
    fn ask(&mut self, id: &Key) {
        // The tests never reach GIPHY: without a worker of their own, a GIF stays loading.
        if cfg!(test) && self.requests.is_none() {
            return;
        }
        if self.requests.is_none() {
            let (requests, inbox) = channel();
            let (outbox, answers) = channel();
            let inbox = Arc::new(Mutex::new(inbox));
            let mut started = 0;
            for worker in 0..WORKERS {
                let (inbox, outbox) = (Arc::clone(&inbox), outbox.clone());
                let spawned = std::thread::Builder::new()
                    .name(format!("sjk-chat-gifs-{worker}"))
                    .spawn(move || work(&inbox, &outbox));
                match spawned {
                    Ok(_) => started += 1,
                    Err(error) => crate::log::progress(format_args!(
                        "warning: an SJK chat GIF worker did not start: {error}"
                    )),
                }
            }
            if started == 0 {
                crate::log::progress(format_args!("warning: SJK chat GIFs off"));
                self.fail(id);
                return;
            }
            self.requests = Some(requests);
            self.answers = Some(answers);
        }
        if self
            .requests
            .as_ref()
            .is_none_or(|requests| requests.send(id.clone()).is_err())
        {
            self.fail(id);
        }
    }

    fn fail(&mut self, id: &Key) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.key == *id) {
            entry.state = State::Failed(Instant::now());
        }
    }

    /// Keep the cache within [`ENTRIES`] and [`BUDGET`]: drop the GIFs used longest ago,
    /// never one used this frame or the last.
    fn evict(&mut self) {
        loop {
            let bytes: usize = self
                .entries
                .iter()
                .map(|entry| match &entry.state {
                    State::Ready(gif) => gif.bytes(),
                    _ => 0,
                })
                .sum();
            if self.entries.len() <= ENTRIES && bytes <= BUDGET {
                return;
            }
            let frame = self.frame;
            let Some(index) = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.used + 1 < frame)
                .min_by_key(|(_, entry)| entry.used)
                .map(|(index, _)| index)
            else {
                return;
            };
            let gone = self.entries.swap_remove(index);
            for slot in &mut self.slots {
                if slot.as_ref().is_some_and(|slot| slot.key == gone.key) {
                    *slot = None;
                }
            }
        }
    }

    /// Take the worker's answers into their entries.
    fn take_answers(&mut self) {
        let Some(answers) = &self.answers else {
            return;
        };
        let mut any = false;
        while let Ok((id, gif)) = answers.try_recv() {
            if let Some(entry) = self
                .entries
                .iter_mut()
                .find(|entry| entry.key == id && matches!(entry.state, State::Loading))
            {
                entry.state = match gif {
                    Some(gif) => State::Ready(Arc::new(gif)),
                    None => State::Failed(Instant::now()),
                };
                any = true;
            }
        }
        if any {
            self.evict();
        }
    }

    /// Milliseconds since the animations began.
    fn elapsed_ms(&mut self) -> u64 {
        if let Some(held) = self.held_ms {
            return held;
        }
        let started = *self.started.get_or_insert_with(Instant::now);
        started.elapsed().as_millis() as u64
    }

    /// For each slot whose GIF is ready, the frame it is at, when `renderer` does not
    /// have that frame yet; the slot is marked as having it.
    fn due_uploads(&mut self, renderer: u64) -> Vec<(usize, Arc<Gif>, usize)> {
        let elapsed = self.elapsed_ms();
        let mut due = Vec::new();
        for (index, slot) in self.slots.iter_mut().enumerate() {
            let Some(held) = slot.as_mut() else {
                continue;
            };
            let gif = self.entries.iter().find_map(|entry| match &entry.state {
                State::Ready(gif) if entry.key == held.key => Some(gif),
                _ => None,
            });
            let Some(gif) = gif else {
                *slot = None;
                continue;
            };
            let frame = gif.frame_at(elapsed);
            if held.uploaded != Some((renderer, frame)) {
                held.uploaded = Some((renderer, frame));
                due.push((index, Arc::clone(gif), frame));
            }
        }
        due
    }
}

/// What to draw for GIF `id` this frame ([`Shown`]). The first call for a GIF asks the
/// worker for it.
pub(crate) fn show(id: &GifId) -> Shown {
    lock().show(&Key::chat(id))
}

/// What to draw for GIF `id` as a picker's tile this frame: its preview, the smaller
/// file GIPHY makes ([`link::Rendition::Preview`]).
pub(crate) fn show_preview(id: &GifId) -> Shown {
    lock().show(&Key {
        id: id.clone(),
        preview: true,
    })
}

/// Whether the cache still waits for `key`: a GIF dropped while it waited in the queue
/// (the picker scrolled past it) is not fetched.
fn wanted(key: &Key) -> bool {
    lock()
        .entries
        .iter()
        .any(|entry| entry.key == *key && matches!(entry.state, State::Loading))
}

/// Once a frame: take what the worker finished and give `renderer` the frame each GIF
/// on screen is at.
pub(crate) fn service(
    renderer: &mut crate::ui_renderer::ShapeRenderer,
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
) {
    let due = {
        let mut gifs = lock();
        gifs.frame += 1;
        let id = renderer.id();
        gifs.renderer = Some(id);
        gifs.take_answers();
        gifs.due_uploads(id)
    };
    for (slot, gif, frame) in due {
        renderer.upload_gif(device, queue, slot, gif.size, &gif.frames[frame].rgba);
    }
}

/// A worker: fetch and decode each GIF asked for, until the cache is gone.
fn work(inbox: &Mutex<Receiver<Key>>, outbox: &Sender<(Key, Option<Gif>)>) {
    let mut http = fetch::Http::new();
    loop {
        let next = inbox
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .recv();
        let Ok(key) = next else {
            return;
        };
        if !wanted(&key) {
            continue;
        }
        let id = &key.id;
        let gif = match fetch::fetch(&mut http, id, key.preview) {
            Ok(bytes) => {
                let gif = decode::decode(&bytes);
                if gif.is_none() {
                    crate::log::progress(format_args!(
                        "GIF {}: not a GIF SJK can show",
                        id.as_str()
                    ));
                }
                gif
            }
            Err(error) => {
                crate::log::progress(format_args!("GIF {}: {error}", id.as_str()));
                None
            }
        };
        if outbox.send((key, gif)).is_err() {
            return;
        }
    }
}

/// Keep `bytes` (a GIF file) decoded as `id`'s GIF, as if the worker had fetched it, and
/// hold the animations at `held_ms`, for a world shot.
#[cfg(test)]
pub(crate) fn insert_for_shot(id: &GifId, bytes: &[u8], held_ms: u64) {
    insert_key_for_shot(Key::chat(id), bytes, held_ms);
}

/// [`insert_for_shot`] for a picker's tile.
#[cfg(test)]
pub(crate) fn insert_preview_for_shot(id: &GifId, bytes: &[u8], held_ms: u64) {
    let key = Key {
        id: id.clone(),
        preview: true,
    };
    insert_key_for_shot(key, bytes, held_ms);
}

#[cfg(test)]
fn insert_key_for_shot(key: Key, bytes: &[u8], held_ms: u64) {
    let gif = decode::decode(bytes).expect("a GIF for the shot");
    let mut gifs = lock();
    gifs.held_ms = Some(held_ms);
    gifs.entries.retain(|entry| entry.key != key);
    let used = gifs.frame;
    gifs.entries.push(Entry {
        key,
        state: State::Ready(Arc::new(gif)),
        used,
    });
}

/// Whether the cache holds or asked for `id`'s GIF, for the tests.
#[cfg(test)]
pub(crate) fn asked_for(id: &GifId) -> bool {
    lock().entries.iter().any(|entry| entry.key.id == *id)
}

/// Mark `id`'s GIF as one that could not be had, for a world shot.
#[cfg(test)]
pub(crate) fn fail_for_shot(id: &GifId) {
    let key = Key::chat(id);
    let mut gifs = lock();
    gifs.entries.retain(|entry| entry.key != key);
    let used = gifs.frame;
    gifs.entries.push(Entry {
        key,
        state: State::Failed(Instant::now()),
        used,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where the tests answer for the worker.
    type Outbox = Sender<(Key, Option<Gif>)>;

    /// A cache with no worker: requests are kept to answer by hand.
    fn cache() -> (Gifs, Receiver<Key>, Outbox) {
        let (requests, inbox) = channel();
        let (outbox, answers) = channel();
        let mut gifs = Gifs::new();
        gifs.requests = Some(requests);
        gifs.answers = Some(answers);
        gifs.renderer = Some(7);
        gifs.held_ms = Some(0);
        (gifs, inbox, outbox)
    }

    fn id(n: usize) -> Key {
        Key::chat(&GifId::new(&format!("Gif{n:04}abc")).unwrap())
    }

    fn small_gif() -> Gif {
        decode::decode(&decode::test_animation([8, 4], 2, 50)).unwrap()
    }

    /// A GIF whose pixels count `bytes`, without decoding one.
    fn gif_of(bytes: usize) -> Gif {
        Gif {
            size: [1, 1],
            frames: vec![decode::Frame {
                rgba: vec![0; bytes].into_boxed_slice(),
                delay_ms: 100,
            }],
            length_ms: 0,
        }
    }

    /// One frame: the frame thread's turn, with renderer 7.
    fn frame(gifs: &mut Gifs) -> Vec<(usize, usize)> {
        gifs.frame += 1;
        gifs.take_answers();
        gifs.due_uploads(7)
            .into_iter()
            .map(|(slot, _, frame)| (slot, frame))
            .collect()
    }

    #[test]
    fn a_gif_is_asked_for_once_and_drawn_once_uploaded() {
        let (mut gifs, inbox, outbox) = cache();
        assert_eq!(gifs.show(&id(1)), Shown::Loading);
        assert_eq!(gifs.show(&id(1)), Shown::Loading);
        assert_eq!(inbox.try_recv().ok(), Some(id(1)));
        assert!(inbox.try_recv().is_err(), "asked once");
        outbox.send((id(1), Some(small_gif()))).unwrap();
        assert!(frame(&mut gifs).is_empty(), "no slot until it is drawn");
        // Ready, given a slot; uploaded by the next frame, drawn after it.
        assert_eq!(gifs.show(&id(1)), Shown::Loading);
        assert_eq!(frame(&mut gifs), [(0, 0)]);
        assert_eq!(
            gifs.show(&id(1)),
            Shown::Ready {
                texture: slot_texture(0),
                size: [8, 4],
            }
        );
        assert!(
            frame(&mut gifs).is_empty(),
            "the same frame is not sent again"
        );
        // The animation moves on: the next frame is uploaded.
        gifs.held_ms = Some(60);
        assert_eq!(frame(&mut gifs), [(0, 1)]);
        // Another renderer (a new world) gets it again.
        gifs.renderer = Some(8);
        assert_eq!(gifs.show(&id(1)), Shown::Loading);
        assert_eq!(gifs.due_uploads(8).len(), 1);
    }

    #[test]
    fn a_gif_that_could_not_be_had_says_so_and_is_not_asked_again_at_once() {
        let (mut gifs, inbox, outbox) = cache();
        let _ = gifs.show(&id(2));
        let _ = inbox.try_recv();
        outbox.send((id(2), None)).unwrap();
        let _ = frame(&mut gifs);
        assert_eq!(gifs.show(&id(2)), Shown::Unavailable);
        assert!(inbox.try_recv().is_err());
        // Long after, asked again.
        gifs.entries[0].state = State::Failed(Instant::now() - RETRY_AFTER);
        assert_eq!(gifs.show(&id(2)), Shown::Loading);
        assert_eq!(inbox.try_recv().ok(), Some(id(2)));
    }

    #[test]
    fn the_cache_drops_the_gifs_used_longest_ago() {
        let (mut gifs, _inbox, outbox) = cache();
        for n in 0..ENTRIES + 4 {
            let _ = gifs.show(&id(n));
            outbox.send((id(n), Some(gif_of(16)))).unwrap();
            let _ = frame(&mut gifs);
            let _ = frame(&mut gifs);
        }
        assert_eq!(gifs.entries.len(), ENTRIES);
        for n in 0..4 {
            assert!(gifs.entries.iter().all(|entry| entry.key != id(n)), "{n}");
        }
        // By bytes: two GIFs of 50 MiB and a third make 150, over 128; the oldest goes.
        let (mut gifs, _inbox, outbox) = cache();
        for n in 0..3 {
            let _ = gifs.show(&id(n));
            outbox.send((id(n), Some(gif_of(50 << 20)))).unwrap();
            let _ = frame(&mut gifs);
            let _ = frame(&mut gifs);
        }
        let kept: Vec<Key> = gifs.entries.iter().map(|entry| entry.key.clone()).collect();
        assert_eq!(kept.len(), 2);
        assert!(!kept.contains(&id(0)));
        // A GIF on screen (used this frame or the last) is never dropped.
        let (mut gifs, _inbox, outbox) = cache();
        for n in 0..3 {
            let _ = gifs.show(&id(n));
            outbox.send((id(n), Some(gif_of(50 << 20)))).unwrap();
        }
        let _ = frame(&mut gifs);
        assert_eq!(gifs.entries.len(), 3, "all three on screen");
    }

    #[test]
    fn a_dropped_gif_frees_its_slot_and_slots_go_to_the_oldest_off_screen() {
        let (mut gifs, _inbox, outbox) = cache();
        for n in 0..GPU_SLOTS + 1 {
            let _ = gifs.show(&id(n));
            outbox.send((id(n), Some(small_gif()))).unwrap();
        }
        let _ = frame(&mut gifs);
        // All nine on screen this frame: eight get a slot, the ninth waits.
        let shown: Vec<Shown> = (0..GPU_SLOTS + 1).map(|n| gifs.show(&id(n))).collect();
        assert!(gifs.slots.iter().all(Option::is_some));
        assert_eq!(shown[GPU_SLOTS], Shown::Loading);
        let _ = frame(&mut gifs);
        let _ = frame(&mut gifs);
        let _ = frame(&mut gifs);
        // Later only the ninth is on screen: it takes the slot used longest ago.
        let _ = gifs.show(&id(GPU_SLOTS));
        assert!(
            gifs.slots
                .iter()
                .flatten()
                .any(|slot| slot.key == id(GPU_SLOTS))
        );
    }

    #[test]
    fn a_tile_and_a_message_keep_the_same_gif_apart() {
        let (mut gifs, inbox, _outbox) = cache();
        let chat = id(1);
        let tile = Key {
            id: chat.id.clone(),
            preview: true,
        };
        let _ = gifs.show(&chat);
        let _ = gifs.show(&tile);
        assert_eq!(inbox.try_recv().ok(), Some(chat));
        assert_eq!(inbox.try_recv().ok(), Some(tile));
        assert_eq!(gifs.entries.len(), 2);
    }

    #[test]
    fn muted_senders_and_gifs_off_fetch_nothing() {
        let text = "look https://giphy.com/gifs/cat-3o7TKSjRrfIPjeiVyM";
        let (shown, gif) = for_message_with(text, true, true);
        assert_eq!((shown.as_str(), gif), (text, None), "muted");
        let (shown, gif) = for_message_with(text, false, true);
        assert_eq!(shown, "look GIF");
        assert_eq!(gif, GifId::new("3o7TKSjRrfIPjeiVyM"));
        let (shown, gif) = for_message_with(text, false, false);
        assert_eq!((shown.as_str(), gif), (text, None), "off");
    }

    #[test]
    fn gif_textures_have_ids_of_their_own() {
        for slot in 0..GPU_SLOTS {
            assert_eq!(slot_of(slot_texture(slot)), Some(slot));
        }
        assert_eq!(slot_of(TextureId(TEXTURE_BASE + GPU_SLOTS as u32)), None);
        assert_eq!(slot_of(TextureId(0)), None);
        assert_eq!(slot_of(crate::medals::Medal::ALL[0].art()), None);
        assert_eq!(crate::medals::Medal::from_art(slot_texture(0)), None);
        assert_eq!(
            crate::menu::art::ArtPiece::from_texture(slot_texture(0)),
            None
        );
    }
}
