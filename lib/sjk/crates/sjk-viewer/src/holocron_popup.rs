//! The holocron drop pop-up (`docs/holocrons.md`, "The pop-up"): the first time the client
//! sees a holocron in the player's own profile, it shows it once, large, in its tier's
//! colour with its name, when it was found and how rare it is. Several show one after
//! another ("1 of 3"). It follows the medal pop-up ([`crate::medal_popup`]), whose
//! ceremony it borrows:
//!
//! - When: on the main menu, or when the game menu opens in a match, never over play. A
//!   holocron that arrives during a match is announced once by a card at the top of the
//!   screen, as an achievement is ([`crate::unlock_toast`]), saying where to see it; the
//!   pop-up waits for the game menu and takes the card back as it opens. While it shows,
//!   the menu under it is not drawn and takes no input. A new medal goes first.
//! - What was shown is kept in `holocrons_seen.txt` ([`crate::holocrons::seen`]): the
//!   highest holocron number shown, so each shows once per identity, and an install that
//!   already held holocrons when it first read its profile shows the newest 20.
//! - Each arrives in a short ceremony with the multiplayer game's fanfare (the medals'
//!   cue): it comes down into place in a burst of light, its words fade up, and it
//!   breathes while it waits. Enter, Space, Right, Escape or a click finishes the ceremony
//!   at once; once it stands still they act as its button, Next (or Close on the last).
//! - One look serves every menu style ([`view`]): the SJK UI's, in its families once they
//!   load, else Inter.
//!
//! `debug_holocron` ([`crate::holocrons::rehearsal`]) queues made-up holocrons through the
//! same queue, so the whole flow can be tried offline; those are never written to
//! `holocrons_seen.txt`.

mod view;

use crate::audio::ui_cues::{self, Cue};
use crate::holocrons::seen::Seen;
use crate::holocrons::{self, COUNT, Entry, FIRST_READ_MAX};
use crate::medal_popup::award::{ENTRANCE, EXIT};
use crate::menu_widgets::MenuCanvas;
use crate::unlock_toast::{self, Unlock};
use sjk_identity::Holocron;
use sjk_ui::{InputEvent, TextureId, UiEventKind};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Instant;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// The button (Next, or Close on the last holocron).
const NEXT_TOKEN: u16 = 972;
/// The rest of the screen: a click there acts as the button too.
const SCREEN_TOKEN: u16 = 973;

/// A holocron waiting to be shown.
#[derive(Clone, Debug)]
struct Queued {
    entry: Entry,
    /// Made up by `debug_holocron`: shown like any other, never counted as seen.
    rehearsal: bool,
    /// Its card was given in a match ([`HolocronPopup::announce`]).
    announced: bool,
}

/// The holocron on show and where its ceremony is.
struct Showing {
    entry: Entry,
    rehearsal: bool,
    /// "Found 10/10/2026 14:05", or nothing; made once, not every frame.
    when: String,
    /// When its ceremony began (with its fanfare); `None` until the pop-up is seen.
    started: Option<Instant>,
    /// Seconds added to the ceremony's clock: a press during the entrance skips to its end.
    ahead: f32,
    /// When Next or Close was taken: it is lifting away.
    leaving: Option<Instant>,
}

impl Showing {
    fn new(queued: Queued) -> Self {
        Self {
            when: queued.entry.when(),
            entry: queued.entry,
            rehearsal: queued.rehearsal,
            started: None,
            ahead: 0.0,
            leaving: None,
        }
    }

    /// Seconds into its ceremony at `now`, and seconds since it began to leave.
    fn times(&self, now: Instant) -> (f32, Option<f32>) {
        let t = self.started.map_or(0.0, |started| {
            now.saturating_duration_since(started).as_secs_f32() + self.ahead
        });
        let exit = self
            .leaving
            .map(|leaving| now.saturating_duration_since(leaving).as_secs_f32());
        (t, exit)
    }
}

/// What the look draws this frame, borrowed from the pop-up.
struct View<'a> {
    entry: &'a Entry,
    /// The tier's picture, when it has one; else it draws as a gem.
    icon: Option<TextureId>,
    when: &'a str,
    /// Which holocron of how many this sitting shows, when more than one.
    place: Option<(usize, usize)>,
    /// Seconds into the ceremony, and since the holocron began to leave.
    t: f32,
    exit: Option<f32>,
    /// The darkness's share of its full depth.
    scrim: f32,
    /// More holocrons wait: the button says Next, else Close.
    more: bool,
}

impl View<'_> {
    /// The button's word.
    fn action(&self) -> &'static str {
        if self.more { "Next" } else { "Close" }
    }
}

/// The pop-up's state: the holocrons waiting, the one showing and what was shown.
pub(crate) struct HolocronPopup {
    ui: MenuCanvas,
    queue: VecDeque<Queued>,
    current: Option<Showing>,
    /// How many were shown before `current` since the pop-up opened, for "2 of 3".
    step: usize,
    /// When the sitting's first holocron was put on show, for the darkness coming in.
    sitting: Instant,
    seen: Option<Seen>,
    /// The settings folder `holocrons_seen.txt` is written to.
    directory: PathBuf,
    /// The key, the number of holocrons and the highest number last offered, so an
    /// unchanged profile is skipped without reading its list.
    last: Option<(String, usize, u64)>,
    /// The tiers' pictures in the UI atlas, where they loaded.
    icons: [Option<TextureId>; COUNT],
    /// How many holocrons `debug_holocron` made, so each gets a number of its own.
    rehearsed: u64,
    /// A moment held still (seconds into the ceremony, and since it began to leave), for
    /// the tests and the off-screen shots.
    #[cfg(test)]
    held: Option<(f32, Option<f32>)>,
}

impl Default for HolocronPopup {
    fn default() -> Self {
        Self {
            // About 15 text runs and 150 shapes at the busiest moment.
            ui: MenuCanvas::with_capacities(32, 256, 320),
            queue: VecDeque::new(),
            current: None,
            step: 0,
            sitting: Instant::now(),
            seen: None,
            directory: PathBuf::new(),
            last: None,
            icons: [None; COUNT],
            rehearsed: 0,
            #[cfg(test)]
            held: None,
        }
    }
}

impl HolocronPopup {
    /// The holocrons the player's profile lists now (`list`, newest first), for the key
    /// `key_id`, with the settings folder: any not shown yet wait for the pop-up. Reading
    /// the list costs a comparison unless it changed.
    pub(crate) fn offer(&mut self, directory: &Path, key_id: &str, list: &[Holocron]) {
        let highest = list.iter().map(|holocron| holocron.id).max().unwrap_or(0);
        if self
            .last
            .as_ref()
            .is_some_and(|(key, len, top)| key == key_id && *len == list.len() && *top == highest)
        {
            return;
        }
        self.last = Some((key_id.to_owned(), list.len(), highest));
        if self
            .seen
            .as_ref()
            .is_none_or(|seen| seen.key_id() != key_id)
        {
            self.seen = Some(Seen::load(directory, key_id));
            // Another identity's holocrons go; rehearsals stay.
            self.queue.retain(|waiting| waiting.rehearsal);
            if self.current.as_ref().is_some_and(|shown| !shown.rehearsal) {
                self.current = None;
            }
        }
        directory.clone_into(&mut self.directory);
        let Some(seen) = &self.seen else {
            return;
        };
        let mut fresh: Vec<Entry> = list
            .iter()
            .filter_map(Entry::from_wire)
            .filter(|entry| {
                seen.is_new(entry.id)
                    && !self
                        .queue
                        .iter()
                        .any(|waiting| !waiting.rehearsal && waiting.entry.id == entry.id)
                    && !self
                        .current
                        .as_ref()
                        .is_some_and(|shown| !shown.rehearsal && shown.entry.id == entry.id)
            })
            .collect();
        // Oldest first; of many (a PC that reinstalled), the newest.
        fresh.sort_by_key(|entry| entry.id);
        if fresh.len() > FIRST_READ_MAX {
            fresh.drain(..fresh.len() - FIRST_READ_MAX);
        }
        self.queue.extend(fresh.into_iter().map(|entry| Queued {
            entry,
            rehearsal: false,
            announced: false,
        }));
    }

    /// Queue `entries` as made-up arrivals (`debug_holocron`): they go through the same
    /// queue, card, ceremony and sound as the hub's, but are never counted as seen.
    pub(crate) fn rehearse(&mut self, entries: Vec<Entry>) {
        self.rehearsed += entries.len() as u64;
        self.queue.extend(entries.into_iter().map(|entry| Queued {
            entry,
            rehearsal: true,
            announced: false,
        }));
    }

    /// The number to give the first of the next rehearsed holocrons: each has one of its
    /// own, rising.
    pub(crate) fn rehearsal_base(&self) -> u64 {
        holocrons::rehearsal::FIRST_ID + self.rehearsed
    }

    /// The tiers' pictures, as the HUD loaded them with the world.
    pub(crate) fn set_icons(&mut self, icons: [Option<TextureId>; COUNT]) {
        self.icons = icons;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.current.is_some()
    }

    /// A holocron waits to be shown.
    pub(crate) fn pending(&self) -> bool {
        !self.queue.is_empty()
    }

    /// Show the first holocron waiting; its ceremony begins once [`Self::update`] says
    /// the pop-up is seen.
    pub(crate) fn open_next(&mut self, now: Instant) {
        if self.current.is_none()
            && let Some(queued) = self.queue.pop_front()
        {
            self.step = 0;
            self.sitting = now;
            self.current = Some(Showing::new(queued));
        }
    }

    /// The cards for a match, once per new holocron: each waiting holocron not announced
    /// yet goes to `show`, at most [`unlock_toast::AT_ONCE`] of one arrival (the rest wait
    /// for the pop-up all the same).
    pub(crate) fn announce(&mut self, mut show: impl FnMut(Unlock)) {
        let mut shown = 0;
        for waiting in self.queue.iter_mut().filter(|waiting| !waiting.announced) {
            waiting.announced = true;
            if shown < unlock_toast::AT_ONCE {
                shown += 1;
                show(Unlock::Holocron {
                    tier: waiting.entry.tier,
                    id: waiting.entry.id,
                    gift: waiting.entry.gift,
                });
            }
        }
    }

    /// Move the ceremony on to `now`. A holocron on show begins (with its fanfare) once
    /// the pop-up is `visible`; one that has lifted away gives way to the next waiting, or
    /// closes the pop-up.
    pub(crate) fn update(&mut self, now: Instant, visible: bool) {
        let Some(showing) = &mut self.current else {
            return;
        };
        if showing.started.is_none() && visible {
            showing.started = Some(now);
            ui_cues::post(Cue::Medal);
        }
        if let (_, Some(exit)) = showing.times(now)
            && exit >= EXIT
        {
            self.current = self.queue.pop_front().map(Showing::new);
            self.step += 1;
        }
    }

    /// Enter, Space, Right or Escape, or a click: finish the entrance at once, or once the
    /// holocron stands still, take its button (Next or Close): count it as seen and let it
    /// lift away. A key plays the menus' click, as a click does.
    fn press(&mut self, now: Instant, key: bool) {
        let Some(showing) = &mut self.current else {
            return;
        };
        if showing.leaving.is_some() {
            return;
        }
        let (t, _) = showing.times(now);
        if showing.started.is_none() {
            showing.started = Some(now);
            showing.ahead = ENTRANCE;
            ui_cues::post(Cue::Medal);
            return;
        }
        if t < ENTRANCE {
            showing.ahead += ENTRANCE - t;
            return;
        }
        showing.leaving = Some(now);
        if key {
            ui_cues::post(Cue::Click);
        }
        if !showing.rehearsal
            && let Some(seen) = &mut self.seen
        {
            seen.mark(showing.entry.id);
            if let Err(error) = seen.save(&self.directory) {
                crate::log::progress(format_args!(
                    "holocrons: cannot write {}: {error}",
                    holocrons::seen::FILE
                ));
            }
        }
    }

    /// Enter, the keypad's Enter, Space, Right or Escape act ([`Self::press`]); every other
    /// key is swallowed while it shows.
    pub(crate) fn handle_key(&mut self, event: &KeyEvent) {
        if event.state != ElementState::Pressed || event.repeat {
            return;
        }
        if let PhysicalKey::Code(code) = event.physical_key {
            self.key(code, Instant::now());
        }
    }

    fn key(&mut self, code: KeyCode, now: Instant) {
        if matches!(
            code,
            KeyCode::Enter
                | KeyCode::NumpadEnter
                | KeyCode::Space
                | KeyCode::ArrowRight
                | KeyCode::Escape
        ) {
            self.press(now, true);
        }
    }

    /// A click on the button, or anywhere else, acts as Enter.
    pub(crate) fn handle_pointer(&mut self, event: InputEvent) {
        self.pointer(event, Instant::now());
    }

    fn pointer(&mut self, event: InputEvent, now: Instant) {
        if self.ui.pointer(event).is_some_and(|event| {
            event.kind == UiEventKind::Activate
                && matches!(event.token, Some(NEXT_TOKEN | SCREEN_TOKEN))
        }) {
            self.press(now, false);
        }
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    /// The canvas and what to draw on it at `now`, while a holocron shows.
    fn parts(&mut self, now: Instant) -> Option<(&mut MenuCanvas, View<'_>)> {
        let showing = self.current.as_ref()?;
        #[allow(unused_mut)]
        let (mut t, mut exit) = showing.times(now);
        #[allow(unused_mut)]
        let mut scrim = crate::medal_popup::award::scrim(
            now.saturating_duration_since(self.sitting).as_secs_f32(),
        );
        #[cfg(test)]
        if let Some((held, leaving)) = self.held {
            (t, exit, scrim) = (held, leaving, 1.0);
        }
        let Self {
            ui,
            current: Some(showing),
            queue,
            step,
            icons,
            ..
        } = self
        else {
            return None;
        };
        let total = *step + 1 + queue.len();
        Some((
            ui,
            View {
                entry: &showing.entry,
                icon: icons[showing.entry.tier.index],
                when: &showing.when,
                place: (total > 1).then_some((*step + 1, total)),
                t,
                exit,
                scrim,
                more: !queue.is_empty(),
            },
        ))
    }
}

impl crate::GpuState {
    /// Hand the pop-up the holocrons of the player's own profile (twice a second, with the
    /// identity's settings).
    pub(crate) fn offer_holocrons(&mut self) {
        let Some(console) = self.console.as_ref() else {
            return;
        };
        let directory = console.config_directory();
        let popup = &mut self.holocron_popup;
        let _ = crate::player_identity::with_own_holocrons(|key_id, _, list| {
            popup.offer(directory, key_id, list);
        });
    }

    /// Open the pop-up when a holocron waits and the main menu or the game menu is up (its
    /// cards go), or announce it once with a card during a match, and move its ceremony
    /// on (it begins once the window is not away). A medal on show or waiting goes first.
    /// Returns whether the pop-up draws this frame (not under the console).
    pub(crate) fn prepare_holocron_popup(&mut self, console_covers_frame: bool) -> bool {
        let now = Instant::now();
        let console_open = self
            .console
            .as_ref()
            .is_some_and(crate::console::ViewerConsole::is_open);
        let medals = self.medal_popup.is_open() || self.medal_popup.pending();
        if !self.holocron_popup.is_open() && self.holocron_popup.pending() {
            let main_menu = self.live_session.is_none()
                && self
                    .client_menu
                    .as_ref()
                    .is_some_and(crate::menu::ClientMenu::on_main_menu);
            let menu = self.game_menu || main_menu;
            // Only the pop-up waits for the medals: the cards queue with theirs.
            if menu && !medals && !console_open && !self.text_dialog.is_open() {
                self.holocron_popup.open_next(now);
                self.unlock_toast
                    .withdraw(now, |unlock| matches!(unlock, Unlock::Holocron { .. }));
            } else if !menu && self.live_session.is_some() {
                let toast = &mut self.unlock_toast;
                self.holocron_popup.announce(|unlock| toast.push(unlock));
            }
        }
        let visible = self.holocron_popup.is_open() && !console_open && !console_covers_frame;
        // The ceremony and its fanfare wait for the window to come back.
        let away = self
            .console
            .as_ref()
            .is_some_and(crate::console::ViewerConsole::window_away);
        self.holocron_popup.update(now, visible && !away);
        visible && self.holocron_popup.is_open()
    }

    /// Draw the pop-up over the whole frame. It darkens everything, so every font batch
    /// appended before it is dropped (text draws above all shapes).
    pub(crate) fn append_holocron_popup(&mut self, viewport: [f32; 2]) {
        let now = Instant::now();
        self.text_vertices.clear();
        self.classic_text_vertices.clear();
        self.game_fonts.clear_text();
        self.holocron_popup.set_icons(self.hud.holocron_icons);
        let target = crate::ingame_menu::sjk_view::text_target(
            &mut self.game_fonts,
            &mut self.text_vertices,
            &self.ui_font,
        );
        self.holocron_popup.append(target, viewport, now);
    }

    /// `debug_holocron`: queue made-up holocrons as if the hub had just dropped them
    /// ([`holocrons::rehearsal`]); alone, list the tiers. Nothing is sent and
    /// `holocrons_seen.txt` is left alone. The console closes so the ceremony shows at
    /// once over a menu (or the cards over play in a match), and the first one's chat line
    /// shows in the game's feed.
    pub(crate) fn debug_holocron_command(
        &mut self,
        args: &[String],
    ) -> Result<Vec<String>, String> {
        if args.is_empty() {
            return Ok(holocrons::rehearsal::listing());
        }
        let entries = holocrons::rehearsal::parse(
            args,
            crate::medal_popup::rehearsal::unix_now(),
            self.holocron_popup.rehearsal_base(),
        )?;
        let lines = holocrons::rehearsal::queued(&entries);
        if let Some(entry) = entries.first() {
            self.chat.rehearse_holocron(entry.tier.id, Instant::now());
        }
        self.holocron_popup.rehearse(entries);
        if let Some(console) = &mut self.console {
            console.set_open(false);
        }
        self.sync_cursor_policy();
        Ok(lines)
    }

    /// `debug_open`: play the Holocrons tab's opening of made-up holocrons
    /// ([`crate::cards::rehearsal`]); alone, list the tiers. In the SJK UI the
    /// Collection's Holocrons tab shows it, in the classic menus the page on its own.
    /// Nothing is sent.
    pub(crate) fn debug_open_command(&mut self, args: &[String]) -> Result<Vec<String>, String> {
        if args.is_empty() {
            return Ok(crate::cards::rehearsal::listing());
        }
        // Check the words before any screen changes.
        crate::cards::rehearsal::parse(args)?;
        if self.in_game_menu.is_sjk() {
            self.open_profile_hub(crate::profile_hub::Tab::Holocrons);
        }
        let result = self.console.as_mut().map_or_else(
            || Err("the console is not ready".to_owned()),
            |console| console.debug_open(args),
        );
        self.sync_cursor_policy();
        result
    }
}

/// What the look's tests share: the fonts and a held pop-up.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::holocrons::Tier;
    use crate::sjk_chat_look::Measure;
    use crate::text::{FontAtlas, TextStyle};
    use std::time::Duration;

    pub(super) struct Fonts {
        body: FontAtlas,
        inter: FontAtlas,
    }

    impl Fonts {
        fn load() -> Self {
            Self {
                body: crate::text::load_family(&crate::text::BODY, 1.0, None)
                    .expect("a bundled family"),
                inter: crate::text::load_modern(1.0, None).expect("Inter"),
            }
        }

        /// The SJK UI's body family, as the pop-up measures its running text.
        pub(super) fn families(&self) -> Measure<'_> {
            Measure::new(&self.body.font, TextStyle::NEUTRAL)
        }

        /// Inter, which draws until the families load.
        pub(super) fn inter(&self) -> Measure<'_> {
            Measure::new(&self.inter.font, TextStyle::NEUTRAL)
        }
    }

    thread_local! {
        pub(super) static FONTS: Fonts = Fonts::load();
    }

    /// A pop-up showing `entries`, the first held `at` seconds into its ceremony (and
    /// `leaving` seconds into its exit).
    pub(super) fn popup(entries: Vec<Entry>, at: f32, leaving: Option<f32>) -> HolocronPopup {
        let mut popup = HolocronPopup {
            held: Some((at, leaving)),
            ..HolocronPopup::default()
        };
        popup.rehearse(entries);
        let now = Instant::now();
        popup.open_next(now);
        popup.update(now, true);
        popup
    }

    fn wire(id: u64, tier: &str) -> Holocron {
        Holocron {
            opened: None,
            id,
            tier: tier.to_owned(),
            dropped: 1_791_641_100,
            source: "play".to_owned(),
            note: String::new(),
        }
    }

    fn entry(id: u64, tier: usize) -> Entry {
        Entry {
            id,
            tier: &holocrons::TIERS[tier],
            dropped: 1_791_641_100,
            gift: false,
            note: String::new(),
        }
    }

    fn after(start: Instant, seconds: f32) -> Instant {
        start + Duration::from_secs_f32(seconds)
    }

    fn shown(popup: &HolocronPopup) -> Option<u64> {
        popup.current.as_ref().map(|showing| showing.entry.id)
    }

    /// Open the first waiting holocron at `start` and stand it still.
    fn open_settled(popup: &mut HolocronPopup, start: Instant) -> Instant {
        popup.open_next(start);
        popup.update(start, true);
        after(start, ENTRANCE + 0.1)
    }

    /// The names on the cards a match would show now.
    fn cards(popup: &mut HolocronPopup) -> Vec<String> {
        let mut names = Vec::new();
        popup.announce(|unlock| names.push(unlock.name().to_string()));
        names
    }

    /// Take the button at `now` and let the holocron lift away.
    fn next(popup: &mut HolocronPopup, now: Instant) -> Instant {
        popup.press(now, true);
        let later = after(now, EXIT + 0.01);
        popup.update(later, true);
        popup.update(later, true);
        later
    }

    #[test]
    fn new_holocrons_wait_show_one_by_one_oldest_first_and_are_remembered() {
        let directory = tempfile::tempdir().expect("a folder");
        let mut popup = HolocronPopup::default();
        // The hub lists them newest first.
        let list = [
            wire(9, "legendary"),
            wire(8, "from_the_future"),
            wire(5, "rare"),
        ];
        popup.offer(directory.path(), "aa", &list);
        assert!(popup.pending() && !popup.is_open());
        assert_eq!(popup.queue.len(), 2, "the unknown tier is left out");
        assert_eq!(cards(&mut popup), ["Rare Holocron", "Legendary Holocron"]);
        assert!(cards(&mut popup).is_empty(), "said once");
        let start = Instant::now();
        let now = open_settled(&mut popup, start);
        assert_eq!(shown(&popup), Some(5));
        let now = next(&mut popup, now);
        assert_eq!(shown(&popup), Some(9));
        assert_eq!(popup.step, 1);
        popup.update(now, true);
        let now = next(&mut popup, after(now, ENTRANCE + 0.1));
        assert!(!popup.is_open() && !popup.pending());
        popup.update(now, true);
        // The same list again, or after a restart, shows nothing.
        popup.offer(directory.path(), "aa", &list);
        assert!(!popup.pending());
        let mut restarted = HolocronPopup::default();
        restarted.offer(directory.path(), "aa", &list);
        assert!(!restarted.pending());
        // A higher number shows; another identity sees its own.
        let longer = [wire(11, "mythical"), wire(9, "legendary"), wire(5, "rare")];
        restarted.offer(directory.path(), "aa", &longer);
        assert_eq!(restarted.queue.len(), 1);
        assert_eq!(restarted.queue[0].entry.id, 11);
        restarted.offer(directory.path(), "bb", &list);
        assert_eq!(restarted.queue.len(), 2, "its own, the other's gone");
    }

    #[test]
    fn a_profile_read_for_the_first_time_shows_the_newest_twenty_oldest_first() {
        let directory = tempfile::tempdir().expect("a folder");
        let mut popup = HolocronPopup::default();
        let list: Vec<Holocron> = (1..=30).rev().map(|id| wire(id, "uncommon")).collect();
        popup.offer(directory.path(), "aa", &list);
        let ids: Vec<u64> = popup.queue.iter().map(|waiting| waiting.entry.id).collect();
        assert_eq!(ids, (11..=30).collect::<Vec<u64>>());
        // In a match, three cards say so; the pop-up shows all twenty.
        assert_eq!(cards(&mut popup).len(), unlock_toast::AT_ONCE);
        assert!(cards(&mut popup).is_empty());
        assert_eq!(popup.queue.len(), 20);
        // Once the last is taken, the older ten never come back.
        let start = Instant::now();
        let mut now = open_settled(&mut popup, start);
        for _ in 0..20 {
            now = next(&mut popup, now);
            popup.update(now, true);
            now = after(now, ENTRANCE + 0.1);
        }
        assert!(!popup.is_open());
        let mut again = HolocronPopup::default();
        again.offer(directory.path(), "aa", &list);
        assert!(!again.pending());
    }

    #[test]
    fn an_unchanged_profile_is_not_read_again_and_one_waiting_is_not_queued_twice() {
        let directory = tempfile::tempdir().expect("a folder");
        let mut popup = HolocronPopup::default();
        let list = [wire(3, "rare")];
        popup.offer(directory.path(), "aa", &list);
        popup.offer(directory.path(), "aa", &list);
        assert_eq!(popup.queue.len(), 1);
        // A new one arrives while the first waits: it comes after it, the first not queued
        // twice.
        popup.offer(
            directory.path(),
            "aa",
            &[wire(4, "legendary"), wire(3, "rare")],
        );
        let ids: Vec<u64> = popup.queue.iter().map(|waiting| waiting.entry.id).collect();
        assert_eq!(ids, [3, 4]);
        // The one on show is not queued again either.
        let mut popup = HolocronPopup::default();
        popup.offer(directory.path(), "cc", &list);
        popup.open_next(Instant::now());
        popup.offer(directory.path(), "cc", &[wire(4, "rare"), wire(3, "rare")]);
        assert_eq!(popup.queue.len(), 1);
        assert_eq!(popup.queue[0].entry.id, 4);
        assert_eq!(shown(&popup), Some(3));
    }

    /// The fanfare plays as each ceremony begins, once; a press during the entrance
    /// finishes it without leaving; once still, a key takes Next with the menus' click, and
    /// the holocron is gone only after its exit.
    #[test]
    fn each_holocron_has_its_fanfare_and_a_press_first_finishes_its_entrance() {
        ui_cues::take_posted();
        let mut popup = HolocronPopup::default();
        popup.rehearse(vec![entry(1, 1), entry(2, 2)]);
        let start = Instant::now();
        popup.open_next(start);
        // Not seen yet (the console over it): no ceremony, no sound.
        popup.update(start, false);
        assert!(popup.current.as_ref().unwrap().started.is_none());
        assert!(ui_cues::take_posted().is_empty());
        popup.update(start, true);
        assert_eq!(ui_cues::take_posted(), [Cue::Medal]);
        // A key in the entrance: it stands still at once, still the same holocron.
        let early = after(start, 0.4);
        popup.key(KeyCode::Enter, early);
        let (t, exit) = popup.current.as_ref().unwrap().times(early);
        assert!((t - ENTRANCE).abs() < 1e-4 && exit.is_none());
        assert!(ui_cues::take_posted().is_empty(), "skipping is silent");
        // The next press takes Next: a click, then the holocron lifts away.
        popup.key(KeyCode::Space, after(start, 0.5));
        assert_eq!(ui_cues::take_posted(), [Cue::Click]);
        popup.update(after(start, 0.5 + EXIT * 0.5), true);
        assert_eq!(shown(&popup), Some(1), "still leaving");
        popup.key(KeyCode::Enter, after(start, 0.5 + EXIT * 0.6));
        assert!(ui_cues::take_posted().is_empty(), "a key while it leaves");
        popup.update(after(start, 0.5 + EXIT + 0.01), true);
        assert_eq!(shown(&popup), Some(2));
        popup.update(after(start, 0.6 + EXIT), true);
        assert_eq!(ui_cues::take_posted(), [Cue::Medal]);
        // Escape acts as Enter; other keys and repeats do nothing.
        let now = after(start, 0.6 + EXIT + ENTRANCE + 0.1);
        popup.key(KeyCode::KeyA, now);
        popup.key(KeyCode::Tab, now);
        assert!(popup.current.as_ref().unwrap().leaving.is_none());
        popup.key(KeyCode::Escape, now);
        assert!(popup.current.as_ref().unwrap().leaving.is_some());
        popup.update(after(now, EXIT + 0.01), true);
        assert!(!popup.is_open(), "Close on the last");
    }

    /// A rehearsal goes through the same queue and ceremony but never into
    /// `holocrons_seen.txt`, so the real holocron still shows when the hub drops it; a
    /// new identity keeps the rehearsals.
    #[test]
    fn rehearsals_are_never_counted_as_seen() {
        let directory = tempfile::tempdir().expect("a folder");
        let file = directory.path().join(holocrons::seen::FILE);
        let mut popup = HolocronPopup::default();
        popup.offer(directory.path(), "aa", &[]);
        popup.rehearse(vec![entry(1, 0), entry(2, 3)]);
        let start = Instant::now();
        let now = open_settled(&mut popup, start);
        let now = next(&mut popup, now);
        popup.update(now, true);
        next(&mut popup, after(now, ENTRANCE + 0.1));
        assert!(!popup.is_open());
        assert!(!file.exists(), "nothing written");
        // The hub drops one now: it shows, and is remembered.
        popup.offer(directory.path(), "aa", &[wire(1, "uncommon")]);
        assert!(popup.pending() && !popup.queue[0].rehearsal);
        let mut early = HolocronPopup::default();
        early.rehearse(vec![entry(1, 0)]);
        early.offer(directory.path(), "bb", &[]);
        assert_eq!(
            early.queue.len(),
            1,
            "the identity arriving keeps rehearsals"
        );
        assert_eq!(cards(&mut early), ["Uncommon Holocron"]);
        // Each rehearsal gets a number of its own.
        assert_eq!(early.rehearsal_base(), holocrons::rehearsal::FIRST_ID + 1);
        early.rehearse(vec![entry(1, 0), entry(2, 0)]);
        assert_eq!(early.rehearsal_base(), holocrons::rehearsal::FIRST_ID + 3);
    }

    /// The real holocron is written as seen when its button is taken, before it has lifted
    /// away; a series interrupted resumes at the next one.
    #[test]
    fn a_real_holocron_is_remembered_when_its_button_is_taken() {
        let directory = tempfile::tempdir().expect("a folder");
        let mut popup = HolocronPopup::default();
        popup.offer(directory.path(), "aa", &[wire(8, "rare"), wire(7, "rare")]);
        let now = open_settled(&mut popup, Instant::now());
        popup.press(now, false);
        let seen = Seen::load(directory.path(), "aa");
        assert!(!seen.is_new(7) && seen.is_new(8), "7 was taken, 8 was not");
        // The game quit there: the next start shows only 8.
        let mut restarted = HolocronPopup::default();
        restarted.offer(directory.path(), "aa", &[wire(8, "rare"), wire(7, "rare")]);
        assert_eq!(restarted.queue.len(), 1);
        assert_eq!(restarted.queue[0].entry.id, 8);
    }

    #[test]
    fn nothing_is_drawn_while_idle() {
        let mut popup = HolocronPopup::default();
        assert!(popup.parts(Instant::now()).is_none());
        assert!(popup.draw_list().is_empty());
        let _ = Tier::from_id("rare");
    }
}
