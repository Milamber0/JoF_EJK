//! Players' pictures on screen (`docs/identity.md`, "Pictures"): a small cache of the
//! pictures the UI shows, each in a cell of the UI's icon atlas.
//!
//! A screen asks for a player's picture with [`texture`], naming the key and the
//! version its profile or presence carries. The first time, a worker thread reads it
//! from the pictures kept on this PC ([`store`]) or downloads it from the hub, decodes
//! it and cuts it round ([`picture`]); [`service`], once a frame, takes what the worker
//! finished and uploads it into the current renderer's atlas. Until then (and when there
//! is none) the screen draws its own stand-in. Nothing here waits: the frame thread
//! only compares a few strings and copies finished pixels.
//!
//! The cache holds [`SLOTS`] pictures, the ones used least recently making room; one
//! more cell holds the preview of a picture the player is about to upload
//! ([`set_preview`]).

pub(crate) mod picture;
pub(crate) mod store;

use sjk_ui::TextureId;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Pictures kept in atlas cells at once (the last cell is the preview's).
pub(crate) const SLOTS: usize = crate::ui_renderer::AVATAR_ICON_CELLS - 1;
/// How long a picture that could not be had waits before it is asked for again.
const RETRY_AFTER: Duration = Duration::from_secs(120);
/// Most pictures uploaded into the atlas in one frame.
const UPLOADS_PER_FRAME: usize = 4;

/// Where a cache entry is.
#[derive(Clone, Debug)]
enum State {
    /// Asked of the worker.
    Loading,
    /// Decoded and cut round, [`sjk_identity::avatar::SIZE`] square RGBA.
    Ready(Arc<[u8]>),
    /// The worker could not get it (no hub, a refusal, a bad picture), at that time.
    Missing(Instant),
}

/// One picture in the cache.
#[derive(Clone, Debug)]
struct Slot {
    key_id: String,
    version: String,
    state: State,
    /// The frame it was last asked for in.
    used: u64,
    /// The renderer whose atlas holds it.
    uploaded_into: Option<u64>,
}

/// What the worker is asked.
struct Request {
    key_id: String,
    version: String,
    hub_url: String,
    dir: Option<PathBuf>,
}

/// What the worker answers.
struct Loaded {
    key_id: String,
    version: String,
    rgba: Option<Vec<u8>>,
}

/// The cache, its worker and the preview.
struct Avatars {
    slots: Vec<Slot>,
    /// Counts frames ([`service`]).
    frame: u64,
    /// The renderer the last frame drew with.
    renderer: Option<u64>,
    /// The hub's address and the folder pictures are kept in, from the settings.
    hub_url: String,
    dir: Option<PathBuf>,
    requests: Option<Sender<Request>>,
    answers: Option<Receiver<Loaded>>,
    /// The picture about to be uploaded, cut round, and the renderer that has it.
    preview: Option<Arc<[u8]>>,
    preview_uploaded_into: Option<u64>,
}

static AVATARS: Mutex<Avatars> = Mutex::new(Avatars {
    slots: Vec::new(),
    frame: 0,
    renderer: None,
    hub_url: String::new(),
    dir: None,
    requests: None,
    answers: None,
    preview: None,
    preview_uploaded_into: None,
});

fn lock() -> MutexGuard<'static, Avatars> {
    AVATARS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The atlas cell of cache slot `index`.
fn slot_texture(index: usize) -> TextureId {
    crate::ui_renderer::avatar_icon(index)
}

/// The atlas cell of the preview.
pub(crate) fn preview_texture() -> TextureId {
    crate::ui_renderer::avatar_icon(SLOTS)
}

impl Avatars {
    /// The texture of `key_id`'s picture at `version`, asking for it when the cache
    /// does not have it.
    fn texture(&mut self, key_id: &str, version: &str) -> Option<TextureId> {
        if !sjk_identity::avatar::valid_version(version)
            || !sjk_identity::avatar::valid_key_id(key_id)
        {
            return None;
        }
        let frame = self.frame;
        if let Some(index) = self
            .slots
            .iter()
            .position(|slot| slot.key_id.eq_ignore_ascii_case(key_id) && slot.version == version)
        {
            let renderer = self.renderer;
            let slot = &mut self.slots[index];
            slot.used = frame;
            let shown = matches!(slot.state, State::Ready(_))
                && slot.uploaded_into.is_some()
                && slot.uploaded_into == renderer;
            if matches!(slot.state, State::Missing(since) if since.elapsed() >= RETRY_AFTER) {
                slot.state = State::Loading;
                self.ask(index);
            }
            return shown.then(|| slot_texture(index));
        }
        let index = self.room()?;
        let slot = Slot {
            key_id: key_id.to_ascii_lowercase(),
            version: version.to_owned(),
            state: State::Loading,
            used: frame,
            uploaded_into: None,
        };
        if index == self.slots.len() {
            self.slots.push(slot);
        } else {
            self.slots[index] = slot;
        }
        self.ask(index);
        None
    }

    /// A slot to put a new picture in: a free one, else the one used longest ago, as
    /// long as it was not used this frame or the last (every slot on screen stays).
    fn room(&self) -> Option<usize> {
        if self.slots.len() < SLOTS {
            return Some(self.slots.len());
        }
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.used + 1 < self.frame)
            .min_by_key(|(_, slot)| slot.used)
            .map(|(index, _)| index)
    }

    /// Ask the worker for slot `index`'s picture, starting it the first time.
    fn ask(&mut self, index: usize) {
        if self.requests.is_none() {
            let (requests, inbox) = channel();
            let (outbox, answers) = channel();
            let spawned = std::thread::Builder::new()
                .name("sjk-avatars".to_owned())
                .spawn(move || work(&inbox, &outbox));
            if let Err(error) = spawned {
                crate::log::progress(format_args!("warning: players' pictures off: {error}"));
                self.slots[index].state = State::Missing(Instant::now());
                return;
            }
            self.requests = Some(requests);
            self.answers = Some(answers);
        }
        let slot = &self.slots[index];
        let request = Request {
            key_id: slot.key_id.clone(),
            version: slot.version.clone(),
            hub_url: self.hub_url.clone(),
            dir: self.dir.clone(),
        };
        if self
            .requests
            .as_ref()
            .is_none_or(|requests| requests.send(request).is_err())
        {
            self.slots[index].state = State::Missing(Instant::now());
        }
    }

    /// Show `rgba` as the preview ([`set_preview`]).
    fn set_preview(&mut self, rgba: Option<&[u8]>) {
        self.preview = rgba.map(|rgba| {
            let mut round = rgba.to_vec();
            picture::round(&mut round, sjk_identity::avatar::SIZE);
            round.into()
        });
        self.preview_uploaded_into = None;
    }

    /// Keep the preview as `key_id`'s picture at `version` ([`adopt_preview`]).
    fn adopt_preview(&mut self, key_id: &str, version: &str) {
        let Some(preview) = self.preview.take() else {
            return;
        };
        self.preview_uploaded_into = None;
        if !sjk_identity::avatar::valid_version(version)
            || !sjk_identity::avatar::valid_key_id(key_id)
        {
            return;
        }
        let key_id = key_id.to_ascii_lowercase();
        let index = self
            .slots
            .iter()
            .position(|slot| slot.key_id == key_id && slot.version == version)
            .or_else(|| self.room());
        let Some(index) = index else {
            return;
        };
        let slot = Slot {
            key_id,
            version: version.to_owned(),
            state: State::Ready(preview),
            used: self.frame,
            uploaded_into: None,
        };
        if index == self.slots.len() {
            self.slots.push(slot);
        } else {
            self.slots[index] = slot;
        }
    }

    /// Take the worker's answers into their slots.
    fn take_answers(&mut self) {
        let Some(answers) = &self.answers else {
            return;
        };
        while let Ok(loaded) = answers.try_recv() {
            if let Some(slot) = self.slots.iter_mut().find(|slot| {
                slot.key_id == loaded.key_id
                    && slot.version == loaded.version
                    && matches!(slot.state, State::Loading)
            }) {
                slot.state = match loaded.rgba {
                    Some(rgba) => State::Ready(rgba.into()),
                    None => State::Missing(Instant::now()),
                };
                slot.uploaded_into = None;
            }
        }
    }
}

/// The texture of `key_id`'s picture at `version` (as their profile or presence
/// carries it), once it is in the current renderer's atlas; `None` until then, for
/// no picture, and while every cell holds a picture on screen. The first call for a
/// picture asks the worker for it.
pub(crate) fn texture(key_id: &str, version: &str) -> Option<TextureId> {
    lock().texture(key_id, version)
}

impl Avatars {
    /// Forget the pictures that failed to load. A slot's place in the cache is its
    /// cell in the renderer's atlas, so the slots after a dropped one move to other
    /// cells: none is in its atlas any more, and each is uploaded again.
    fn drop_missing(&mut self) {
        self.slots
            .retain(|slot| !matches!(slot.state, State::Missing(_)));
        for slot in &mut self.slots {
            slot.uploaded_into = None;
        }
    }
}

/// Tell the cache where pictures come from: the hub's address (empty for none) and the
/// settings folder. A change lets pictures that could not be had be asked for again.
pub(crate) fn configure(config_directory: Option<&std::path::Path>, hub_url: &str) {
    let mut avatars = lock();
    let dir = config_directory.map(|dir| dir.join(store::DIR));
    if avatars.hub_url == hub_url && avatars.dir == dir {
        return;
    }
    avatars.hub_url = hub_url.to_owned();
    avatars.dir = dir;
    avatars.drop_missing();
}

/// Show `rgba` ([`sjk_identity::avatar::SIZE`] square, not yet round) as the preview of
/// the picture about to be uploaded, or none.
pub(crate) fn set_preview(rgba: Option<&[u8]>) {
    lock().set_preview(rgba);
}

/// The preview became `key_id`'s picture at `version` (the hub took it): keep its
/// pixels as that picture, so it shows at once without being downloaded, and drop the
/// preview.
pub(crate) fn adopt_preview(key_id: &str, version: &str) {
    lock().adopt_preview(key_id, version);
}

/// Keep `rgba` ([`sjk_identity::avatar::SIZE`] square, not yet round) as `key_id`'s
/// picture at `version`, as if the worker had loaded it, for a world shot.
#[cfg(test)]
pub(crate) fn insert_for_shot(key_id: &str, version: &str, rgba: &[u8]) {
    let mut avatars = lock();
    avatars.set_preview(Some(rgba));
    avatars.adopt_preview(key_id, version);
}

/// Whether the preview is in the current renderer's atlas.
pub(crate) fn preview_ready() -> bool {
    let avatars = lock();
    avatars.preview.is_some()
        && avatars.preview_uploaded_into.is_some()
        && avatars.preview_uploaded_into == avatars.renderer
}

/// Once a frame: take what the worker finished and upload the pictures `renderer`'s
/// atlas does not have yet (a few a frame).
pub(crate) fn service(
    renderer: &crate::ui_renderer::ShapeRenderer,
    queue: &crate::frame_queue::FrameQueue,
) {
    let mut avatars = lock();
    avatars.frame += 1;
    let id = renderer.id();
    avatars.renderer = Some(id);
    avatars.take_answers();
    let mut uploads = 0;
    for (index, slot) in avatars.slots.iter_mut().enumerate() {
        if uploads >= UPLOADS_PER_FRAME {
            break;
        }
        if let State::Ready(rgba) = &slot.state
            && slot.uploaded_into != Some(id)
        {
            renderer.upload_icon(queue, slot_texture(index), rgba);
            slot.uploaded_into = Some(id);
            uploads += 1;
        }
    }
    if avatars.preview_uploaded_into != Some(id)
        && let Some(preview) = &avatars.preview
    {
        renderer.upload_icon(queue, preview_texture(), preview);
        avatars.preview_uploaded_into = Some(id);
    }
}

/// The worker: answer each request from the pictures kept on this PC or from the hub,
/// until the cache is gone.
fn work(inbox: &Receiver<Request>, outbox: &Sender<Loaded>) {
    let mut hubs: Option<(String, sjk_identity::HttpHub)> = None;
    while let Ok(request) = inbox.recv() {
        let rgba = load(&request, &mut hubs);
        let answer = Loaded {
            key_id: request.key_id,
            version: request.version,
            rgba,
        };
        if outbox.send(answer).is_err() {
            return;
        }
    }
}

/// One picture: kept on this PC, else downloaded (and kept); decoded and cut round.
fn load(request: &Request, hubs: &mut Option<(String, sjk_identity::HttpHub)>) -> Option<Vec<u8>> {
    use sjk_identity::Hub;
    let kept = request
        .dir
        .as_deref()
        .and_then(|dir| store::read(dir, &request.key_id, &request.version))
        .and_then(|png| decode(&png));
    if kept.is_some() {
        return kept;
    }
    if request.hub_url.is_empty() {
        return None;
    }
    if hubs.as_ref().is_none_or(|(url, _)| *url != request.hub_url) {
        let agent = format!("SJK/{}", crate::build_info::VERSION);
        let hub = sjk_identity::HttpHub::new(&request.hub_url, &agent).ok()?;
        *hubs = Some((request.hub_url.clone(), hub));
    }
    let (_, hub) = hubs.as_mut()?;
    let png = match hub.avatar(&request.key_id, &request.version) {
        Ok(png) => png,
        Err(error) => {
            crate::log::progress(format_args!("picture of {}: {error}", request.key_id));
            return None;
        }
    };
    let rgba = decode(&png)?;
    if let Some(dir) = &request.dir {
        let _ = store::write(dir, &request.key_id, &request.version, &png);
    }
    Some(rgba)
}

/// A served picture as round RGBA pixels.
fn decode(png: &[u8]) -> Option<Vec<u8>> {
    let mut rgba = picture::decode_served(png)?;
    picture::round(&mut rgba, sjk_identity::avatar::SIZE);
    Some(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(n: u64) -> String {
        format!("{n:016x}")
    }

    fn key(n: u64) -> String {
        format!("{:016x}", 0xabc0_0000_0000_0000 | n)
    }

    /// A cache with no worker: requests are kept to answer by hand.
    fn cache() -> (Avatars, Receiver<Request>) {
        let (requests, inbox) = channel();
        let avatars = Avatars {
            slots: Vec::new(),
            frame: 1,
            renderer: Some(7),
            hub_url: "https://hub.example".to_owned(),
            dir: None,
            requests: Some(requests),
            answers: None,
            preview: None,
            preview_uploaded_into: None,
        };
        (avatars, inbox)
    }

    #[test]
    fn a_picture_is_asked_for_once_and_shown_once_uploaded() {
        let (mut avatars, inbox) = cache();
        let (outbox, answers) = channel();
        avatars.answers = Some(answers);
        assert_eq!(avatars.texture(&key(1), ""), None, "no picture");
        assert_eq!(avatars.texture("nope", &version(1)), None);
        assert!(inbox.try_recv().is_err(), "nothing is asked for those");
        assert_eq!(avatars.texture(&key(1), &version(1)), None);
        assert_eq!(avatars.texture(&key(1), &version(1)), None);
        // A key id in capitals is the same key.
        assert_eq!(
            avatars.texture(&key(1).to_ascii_uppercase(), &version(1)),
            None
        );
        assert_eq!(avatars.slots.len(), 1);
        let asked: Vec<Request> = inbox.try_iter().collect();
        assert_eq!(asked.len(), 1, "asked once");
        assert_eq!(
            (asked[0].key_id.as_str(), asked[0].version.as_str()),
            (key(1).as_str(), version(1).as_str())
        );
        outbox
            .send(Loaded {
                key_id: key(1),
                version: version(1),
                rgba: Some(vec![1; 16]),
            })
            .unwrap();
        avatars.take_answers();
        assert_eq!(
            avatars.texture(&key(1), &version(1)),
            None,
            "not uploaded yet"
        );
        avatars.slots[0].uploaded_into = Some(7);
        assert_eq!(avatars.texture(&key(1), &version(1)), Some(slot_texture(0)));
        // Another renderer (another world) does not have it.
        avatars.renderer = Some(8);
        assert_eq!(avatars.texture(&key(1), &version(1)), None);
        // A new version is a new picture.
        assert_eq!(avatars.texture(&key(1), &version(2)), None);
        assert_eq!(inbox.try_iter().count(), 1);
    }

    #[test]
    fn dropping_failed_pictures_uploads_the_ones_that_moved_again() {
        let (mut avatars, _inbox) = cache();
        for (index, ready) in [true, false, true].into_iter().enumerate() {
            avatars.slots.push(Slot {
                key_id: key(index as u64),
                version: version(1),
                state: if ready {
                    State::Ready(vec![1; 16].into())
                } else {
                    State::Missing(Instant::now())
                },
                used: 1,
                uploaded_into: Some(7),
            });
        }
        avatars.drop_missing();
        assert_eq!(avatars.slots.len(), 2);
        // The third picture now stands at index 1, whose atlas cell holds another.
        assert_eq!(avatars.slots[1].key_id, key(2));
        assert_eq!(avatars.texture(&key(2), &version(1)), None);
        assert!(
            avatars
                .slots
                .iter()
                .all(|slot| slot.uploaded_into.is_none())
        );
    }

    #[test]
    fn a_sent_preview_becomes_the_picture_of_its_new_version() {
        let size = sjk_identity::avatar::SIZE as usize;
        let (mut avatars, _inbox) = cache();
        avatars.set_preview(Some(&vec![200; size * size * 4]));
        assert!(avatars.preview.is_some());
        avatars.adopt_preview(&key(7), &version(9));
        assert!(avatars.preview.is_none());
        let slot = avatars
            .slots
            .iter()
            .find(|slot| slot.key_id == key(7) && slot.version == version(9))
            .expect("kept as that picture");
        let State::Ready(rgba) = &slot.state else {
            panic!("ready at once");
        };
        assert_eq!(rgba[3], 0, "cut round");
        assert_eq!(rgba.len(), size * size * 4);
    }

    #[test]
    fn the_cache_is_bounded_and_keeps_what_is_on_screen() {
        let (mut avatars, inbox) = cache();
        for n in 0..SLOTS as u64 {
            avatars.texture(&key(n), &version(1));
        }
        assert_eq!(avatars.slots.len(), SLOTS);
        // All on screen this frame: a new one waits for room.
        avatars.texture(&key(99), &version(1));
        assert_eq!(avatars.slots.len(), SLOTS);
        assert!(!avatars.slots.iter().any(|slot| slot.key_id == key(99)));
        // Two frames later, with only some still shown, the one used longest ago goes.
        avatars.frame += 2;
        for n in 1..SLOTS as u64 {
            avatars.texture(&key(n), &version(1));
        }
        avatars.texture(&key(99), &version(1));
        assert_eq!(avatars.slots[0].key_id, key(99));
        assert!(matches!(avatars.slots[0].state, State::Loading));
        assert_eq!(inbox.try_iter().count(), SLOTS + 1);
    }

    #[test]
    fn a_picture_that_could_not_be_had_is_asked_again_later_or_on_a_new_hub() {
        let (mut avatars, inbox) = cache();
        let (outbox, answers) = channel();
        avatars.answers = Some(answers);
        avatars.texture(&key(1), &version(1));
        outbox
            .send(Loaded {
                key_id: key(1),
                version: version(1),
                rgba: None,
            })
            .unwrap();
        avatars.take_answers();
        avatars.texture(&key(1), &version(1));
        assert_eq!(inbox.try_iter().count(), 1, "not asked again at once");
        let Some(long_ago) = Instant::now().checked_sub(RETRY_AFTER) else {
            return;
        };
        avatars.slots[0].state = State::Missing(long_ago);
        avatars.texture(&key(1), &version(1));
        assert_eq!(inbox.try_iter().count(), 1, "asked again later");
    }

    /// The whole path against a running hub: a player's picture made ready and sent as
    /// the Profile page sends it, then read by another client's worker and kept. Start
    /// `sjk-hub serve` and set `SJK_HUB_TEST_URL`, as for sjk-identity's `hub_e2e`.
    #[test]
    #[ignore = "needs a running hub (SJK_HUB_TEST_URL)"]
    fn the_worker_downloads_a_sent_picture_and_keeps_it() {
        use sjk_identity::Hub;
        let url = std::env::var("SJK_HUB_TEST_URL").expect("SJK_HUB_TEST_URL");
        let mut hub = sjk_identity::HttpHub::new(&url, "sjk-viewer-test").unwrap();
        let me = sjk_identity::Identity::generate().unwrap();
        hub.register(&me, Some("PictureWorker")).unwrap();
        // A photo as players have them: a JPEG, wider than tall.
        let photo = {
            let image = image::RgbImage::from_fn(300, 200, |x, _| {
                image::Rgb(if x < 150 {
                    [200, 30, 30]
                } else {
                    [30, 30, 200]
                })
            });
            let mut out = std::io::Cursor::new(Vec::new());
            image.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
            out.into_inner()
        };
        let prepared = picture::prepare(&photo).unwrap();
        let profile = hub.set_avatar(&me, &prepared.png).unwrap();
        assert!(sjk_identity::avatar::valid_version(&profile.avatar));
        let dir = tempfile::tempdir().unwrap();
        let request = Request {
            key_id: me.key_id(),
            version: profile.avatar.clone(),
            hub_url: url,
            dir: Some(dir.path().to_owned()),
        };
        let rgba = load(&request, &mut None).expect("downloaded");
        let size = sjk_identity::avatar::SIZE as usize;
        assert_eq!(rgba.len(), size * size * 4);
        assert!(
            store::read(dir.path(), &me.key_id(), &profile.avatar).is_some(),
            "kept"
        );
        // Without the hub, the kept picture still shows.
        let offline = Request {
            hub_url: String::new(),
            ..request
        };
        assert!(load(&offline, &mut None).is_some());
        hub.remove_avatar(&me).unwrap();
    }

    #[test]
    fn the_worker_reads_kept_pictures_without_a_hub() {
        let dir = tempfile::tempdir().unwrap();
        let png = {
            let image = image::RgbaImage::from_pixel(
                sjk_identity::avatar::SIZE,
                sjk_identity::avatar::SIZE,
                image::Rgba([10, 20, 30, 255]),
            );
            let mut out = std::io::Cursor::new(Vec::new());
            image.write_to(&mut out, image::ImageFormat::Png).unwrap();
            out.into_inner()
        };
        store::write(dir.path(), &key(1), &version(1), &png).unwrap();
        let mut hubs = None;
        let kept = Request {
            key_id: key(1),
            version: version(1),
            hub_url: String::new(),
            dir: Some(dir.path().to_owned()),
        };
        let rgba = load(&kept, &mut hubs).expect("read from the folder");
        let size = sjk_identity::avatar::SIZE as usize;
        assert_eq!(rgba.len(), size * size * 4);
        assert_eq!(rgba[3], 0, "cut round: the corner is clear");
        let middle = (size / 2 * size + size / 2) * 4;
        assert_eq!(&rgba[middle..middle + 4], &[10, 20, 30, 255]);
        let missing = Request {
            version: version(2),
            ..kept
        };
        assert_eq!(load(&missing, &mut hubs), None, "no hub, nothing kept");
    }
}
