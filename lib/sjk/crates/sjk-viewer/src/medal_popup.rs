//! The new medal pop-up: the first time the client sees a medal in the player's own
//! profile, or a repeatable one's count rise, it shows it once, large, with its name,
//! what it is for and the SJK team's note (`docs/identity.md`, "Medals"). Several
//! new medals show one after another.
//!
//! When: on the main menu, or when the game menu opens in a match, never over play.
//! A medal that arrives during a match is announced once by a card at the top of the
//! screen, as an achievement is ([`crate::unlock_toast`]), saying where to see it; the
//! pop-up waits for the game menu and takes the card back as it opens. While it shows,
//! the menu under it is not drawn and takes no input. What was shown is kept in
//! `medals_seen.txt` (`medals/seen.rs`), so each medal (and each new count) shows once
//! per identity.
//!
//! Each medal arrives in a short ceremony ([`award`]) with the multiplayer game's
//! fanfare (`audio/ui_cues.rs`): it comes down into place in a burst of gold light,
//! its words fade up, and it breathes gently while it waits. Enter, Space, Right,
//! Escape or a click on it finishes the ceremony at once; once it stands still they
//! act as its button, Next (or Close on the last), and the medal lifts away. The
//! pop-up has the SJK UI's look with its menus ([`sjk_view`]) and the classic+ look
//! with the classic ones ([`classic_view`]); the state, keys and pointer are shared.
//!
//! `debug_medal` ([`rehearsal`]) queues made-up medals through the same queue, so the
//! whole flow can be tried offline; those are never written to `medals_seen.txt`.

pub(crate) mod award;
mod classic_view;
pub(crate) mod rehearsal;
mod sjk_view;

use crate::audio::ui_cues::{self, Cue};
use crate::medals::seen::Seen;
use crate::medals::{self, Award};
use crate::menu::art::ArtSet;
use crate::menu::style::MenuStyle;
use crate::menu_widgets::MenuCanvas;
use crate::unlock_toast::{self, Unlock};
use award::{ENTRANCE, EXIT};
use sjk_ui::{InputEvent, UiEventKind};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// The button (Next, or Close on the last medal).
const NEXT_TOKEN: u16 = 970;
/// The rest of the screen: a click there acts as the button too.
const SCREEN_TOKEN: u16 = 971;
/// Longest wait for a medal's picture, decoded on a worker the first time one is
/// needed, before its ceremony begins without it.
const ART_WAIT: Duration = Duration::from_millis(1_500);

/// A medal waiting to be shown.
#[derive(Clone, Debug)]
struct Queued {
    award: Award,
    /// Made up by `debug_medal`: shown like any other, never counted as seen.
    rehearsal: bool,
    /// Its card was given in a match ([`MedalPopup::announce`]).
    announced: bool,
}

/// The medal on show and where its ceremony is.
struct Showing {
    award: Award,
    rehearsal: bool,
    /// "Given 07/10/2026", or nothing; made once, not every frame.
    given: String,
    /// When it was put on show.
    opened: Instant,
    /// When its ceremony began (with its fanfare); `None` while its picture loads.
    started: Option<Instant>,
    /// Seconds added to the ceremony's clock: a press during the entrance skips to
    /// its end.
    ahead: f32,
    /// When Next or Close was taken: it is lifting away.
    leaving: Option<Instant>,
}

impl Showing {
    fn new(queued: Queued, now: Instant) -> Self {
        Self {
            given: queued.award.given(),
            award: queued.award,
            rehearsal: queued.rehearsal,
            opened: now,
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

/// What a look draws this frame, borrowed from the pop-up.
struct View<'a> {
    award: &'a Award,
    given: &'a str,
    /// Which medal of how many this sitting shows, when more than one.
    place: Option<(usize, usize)>,
    /// Seconds into the ceremony, and since the medal began to leave.
    t: f32,
    exit: Option<f32>,
    /// The darkness's share of its full depth.
    scrim: f32,
    /// More medals wait: the button says Next, else Close.
    more: bool,
}

impl View<'_> {
    /// The button's word.
    fn action(&self) -> &'static str {
        if self.more { "Next" } else { "Close" }
    }
}

/// The pop-up's state: the medals waiting, the one showing and what was shown.
pub(crate) struct MedalPopup {
    ui: MenuCanvas,
    queue: VecDeque<Queued>,
    current: Option<Showing>,
    /// How many were shown before `current` since the pop-up opened, for "2 of 3".
    step: usize,
    /// When the sitting's first medal was put on show, for the darkness coming in.
    sitting: Instant,
    seen: Option<Seen>,
    /// The settings folder `medals_seen.txt` is written to.
    directory: PathBuf,
    /// The key and the profile's list last offered, so an unchanged one is skipped.
    last: Option<(String, Vec<sjk_identity::Medal>)>,
    /// The look, following `ui_menuStyle`, and the retail art the classic+ one can
    /// draw.
    style: MenuStyle,
    art: ArtSet,
    /// A moment held still (seconds into the ceremony, and since it began to leave),
    /// for the tests and the off-screen shots.
    #[cfg(test)]
    held: Option<(f32, Option<f32>)>,
}

impl Default for MedalPopup {
    fn default() -> Self {
        Self {
            // About 20 text runs and 170 shapes at the busiest moment.
            ui: MenuCanvas::with_capacities(32, 256, 320),
            queue: VecDeque::new(),
            current: None,
            step: 0,
            sitting: Instant::now(),
            seen: None,
            directory: PathBuf::new(),
            last: None,
            style: MenuStyle::default(),
            art: ArtSet::default(),
            #[cfg(test)]
            held: None,
        }
    }
}

impl MedalPopup {
    /// The medals the player's profile lists now, for the key `key_id`, with the settings
    /// folder: any not shown yet wait for the pop-up.
    pub(crate) fn offer(&mut self, directory: &Path, key_id: &str, list: &[sjk_identity::Medal]) {
        if self
            .last
            .as_ref()
            .is_some_and(|(key, last)| key == key_id && last == list)
        {
            return;
        }
        self.last = Some((key_id.to_owned(), list.to_vec()));
        if self
            .seen
            .as_ref()
            .is_none_or(|seen| seen.key_id() != key_id)
        {
            self.seen = Some(Seen::load(directory, key_id));
            // Another identity's medals go; rehearsals stay.
            self.queue.retain(|waiting| waiting.rehearsal);
            if self.current.as_ref().is_some_and(|shown| !shown.rehearsal) {
                self.current = None;
            }
        }
        directory.clone_into(&mut self.directory);
        let Some(seen) = &self.seen else {
            return;
        };
        let mut added = false;
        for award in medals::awards(list) {
            if !seen.is_new(&award) {
                continue;
            }
            if self.current.as_ref().is_some_and(|shown| {
                !shown.rehearsal
                    && shown.award.medal == award.medal
                    && shown.award.count >= award.count
            }) {
                continue;
            }
            // A newer count replaces one still waiting, and the real medal a rehearsal.
            let queued = Queued {
                award,
                rehearsal: false,
                announced: false,
            };
            match self
                .queue
                .iter_mut()
                .find(|waiting| waiting.award.medal == queued.award.medal)
            {
                Some(waiting) => *waiting = queued,
                None => {
                    self.queue.push_back(queued);
                    added = true;
                }
            }
        }
        if added {
            // Decode the pictures now, so the ceremony need not wait for them.
            crate::medals::art::request();
        }
    }

    /// Queue `awards` as made-up arrivals (`debug_medal`): they go through the same
    /// queue, card, ceremony and sound as the hub's, but are never counted as seen. A
    /// rehearsal of a medal already waiting as one replaces it.
    pub(crate) fn rehearse(&mut self, awards: Vec<Award>) {
        for award in awards {
            let queued = Queued {
                award,
                rehearsal: true,
                announced: false,
            };
            match self
                .queue
                .iter_mut()
                .find(|waiting| waiting.rehearsal && waiting.award.medal == queued.award.medal)
            {
                Some(waiting) => *waiting = queued,
                None => self.queue.push_back(queued),
            }
        }
        crate::medals::art::request();
    }

    pub(crate) fn is_open(&self) -> bool {
        self.current.is_some()
    }

    /// A medal waits to be shown.
    pub(crate) fn pending(&self) -> bool {
        !self.queue.is_empty()
    }

    /// Show the first medal waiting; its ceremony begins once [`Self::update`] says
    /// the pop-up is seen.
    pub(crate) fn open_next(&mut self, now: Instant) {
        if self.current.is_none()
            && let Some(queued) = self.queue.pop_front()
        {
            self.step = 0;
            self.sitting = now;
            self.current = Some(Showing::new(queued, now));
        }
    }

    /// The cards for a match, once per new medal (or new count): each waiting medal
    /// not announced yet goes to `show`, at most [`unlock_toast::AT_ONCE`] at a time.
    pub(crate) fn announce(&mut self, mut show: impl FnMut(Unlock)) {
        let mut shown = 0;
        for waiting in self.queue.iter_mut().filter(|waiting| !waiting.announced) {
            waiting.announced = true;
            if shown < unlock_toast::AT_ONCE {
                shown += 1;
                show(Unlock::Medal {
                    medal: waiting.award.medal,
                    count: waiting.award.count,
                });
            }
        }
    }

    /// Move the ceremony on to `now`. A medal on show begins (with its fanfare) once
    /// the pop-up is `visible` and its picture is `art_ready`, or has waited
    /// [`ART_WAIT`] for it; one that has lifted away gives way to the next waiting, or
    /// closes the pop-up.
    pub(crate) fn update(&mut self, now: Instant, visible: bool, art_ready: bool) {
        let Some(showing) = &mut self.current else {
            return;
        };
        if showing.started.is_none() {
            if !visible {
                showing.opened = now;
            } else if art_ready || now.saturating_duration_since(showing.opened) >= ART_WAIT {
                showing.started = Some(now);
                ui_cues::post(Cue::Medal);
            }
        }
        if let (_, Some(exit)) = showing.times(now)
            && exit >= EXIT
        {
            self.current = self
                .queue
                .pop_front()
                .map(|queued| Showing::new(queued, now));
            self.step += 1;
        }
    }

    /// Enter, Space, Right or Escape, or a click: finish the entrance at once, or
    /// once the medal stands still, take its button (Next or Close): count it as
    /// seen and let it lift away. A key plays the menus' click, as a click does.
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
            seen.mark(&showing.award);
            if let Err(error) = seen.save(&self.directory) {
                crate::log::progress(format_args!(
                    "medals: cannot write {}: {error}",
                    crate::medals::seen::FILE
                ));
            }
        }
    }

    /// Enter, the keypad's Enter, Space, Right or Escape act ([`Self::press`]); every
    /// other key is swallowed while it shows.
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

    /// Follow `ui_menuStyle`, with the retail `art` the classic+ look can draw.
    pub(crate) fn set_style(&mut self, style: MenuStyle, art: ArtSet) {
        self.style = style;
        self.art = art;
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    /// The canvas and what to draw on it at `now`, while a medal shows.
    fn parts(&mut self, now: Instant) -> Option<(&mut MenuCanvas, View<'_>)> {
        let showing = self.current.as_ref()?;
        #[allow(unused_mut)]
        let (mut t, mut exit) = showing.times(now);
        #[allow(unused_mut)]
        let mut scrim = award::scrim(now.saturating_duration_since(self.sitting).as_secs_f32());
        #[cfg(test)]
        if let Some((held, leaving)) = self.held {
            (t, exit, scrim) = (held, leaving, 1.0);
        }
        let Self {
            ui,
            current: Some(showing),
            queue,
            step,
            ..
        } = self
        else {
            return None;
        };
        let total = *step + 1 + queue.len();
        Some((
            ui,
            View {
                award: &showing.award,
                given: &showing.given,
                place: (total > 1).then_some((*step + 1, total)),
                t,
                exit,
                scrim,
                more: !queue.is_empty(),
            },
        ))
    }
}

#[cfg(test)]
impl MedalPopup {
    /// Show `awards` as if the profile had just listed them, the first held `at`
    /// seconds into its ceremony (and `leaving` seconds into its exit), for the
    /// off-screen snapshots.
    pub(crate) fn preview(awards: Vec<Award>, at: f32, leaving: Option<f32>) -> Self {
        let mut popup = Self {
            queue: awards
                .into_iter()
                .map(|award| Queued {
                    award,
                    rehearsal: true,
                    announced: false,
                })
                .collect(),
            held: Some((at, leaving)),
            ..Self::default()
        };
        let now = Instant::now();
        popup.open_next(now);
        popup.update(now, true, true);
        popup
    }

    /// The same in `style`.
    pub(crate) fn preview_in(
        style: MenuStyle,
        awards: Vec<Award>,
        at: f32,
        leaving: Option<f32>,
    ) -> Self {
        let mut popup = Self::preview(awards, at, leaving);
        popup.style = style;
        popup
    }
}

impl crate::GpuState {
    /// Hand the pop-up the medals of the player's own profile (twice a second, with the
    /// identity's settings).
    pub(crate) fn offer_medals(&mut self) {
        let Some((key_id, list)) = crate::player_identity::own_medals() else {
            return;
        };
        let Some(console) = self.console.as_ref() else {
            return;
        };
        self.medal_popup
            .offer(console.config_directory(), &key_id, &list);
    }

    /// Open the pop-up when a medal waits and the main menu or the game menu is up (its
    /// cards go), or announce it once with a card during a match, and move its ceremony
    /// on (it begins once the window is not away). Returns whether the pop-up draws this
    /// frame (not under the console).
    pub(crate) fn prepare_medal_popup(&mut self, console_covers_frame: bool) -> bool {
        let now = Instant::now();
        let console_open = self
            .console
            .as_ref()
            .is_some_and(crate::console::ViewerConsole::is_open);
        if !self.medal_popup.is_open() && self.medal_popup.pending() {
            let main_menu = self.live_session.is_none()
                && self
                    .client_menu
                    .as_ref()
                    .is_some_and(crate::menu::ClientMenu::on_main_menu);
            let menu = self.game_menu || main_menu;
            if menu && !console_open && !self.text_dialog.is_open() {
                self.medal_popup.open_next(now);
                self.unlock_toast
                    .withdraw(now, |unlock| matches!(unlock, Unlock::Medal { .. }));
            } else if !menu && self.live_session.is_some() {
                let toast = &mut self.unlock_toast;
                self.medal_popup.announce(|unlock| toast.push(unlock));
            }
        }
        let visible = self.medal_popup.is_open() && !console_open && !console_covers_frame;
        // The ceremony and its fanfare wait for the window to come back.
        let away = self
            .console
            .as_ref()
            .is_some_and(crate::console::ViewerConsole::window_away);
        self.medal_popup.update(
            now,
            visible && !away,
            crate::medals::art::decoded().is_some(),
        );
        visible && self.medal_popup.is_open()
    }

    /// Draw the pop-up over the whole frame in its look. It darkens everything, so
    /// every font batch appended before it is dropped (text draws above all shapes);
    /// the SJK UI's look draws in its families once they are loaded.
    pub(crate) fn append_medal_popup(&mut self, viewport: [f32; 2]) {
        let now = Instant::now();
        self.text_vertices.clear();
        self.classic_text_vertices.clear();
        self.game_fonts.clear_text();
        match self.medal_popup.style {
            MenuStyle::Sjk => {
                let target = crate::ingame_menu::sjk_view::text_target(
                    &mut self.game_fonts,
                    &mut self.text_vertices,
                    &self.ui_font,
                );
                self.medal_popup.append_sjk(target, viewport, now);
            }
            MenuStyle::Classic => {
                let (vertices, font) = self.game_fonts.menu(&mut self.text_vertices, &self.ui_font);
                self.medal_popup
                    .append_classic(vertices, font, viewport, now);
            }
        }
    }

    /// `debug_medal`: queue made-up medals as if the hub had just given them
    /// ([`rehearsal`]); alone, list the ids. Nothing is sent and `medals_seen.txt` is
    /// left alone. The console closes so the ceremony shows at once over a menu.
    pub(crate) fn debug_medal_command(&mut self, args: &[String]) -> Result<Vec<String>, String> {
        if args.is_empty() {
            return Ok(rehearsal::listing());
        }
        let list = rehearsal::parse(args, rehearsal::unix_now())?;
        // The same reading as a hub profile's list: known ids, counts, plain notes.
        let awards = medals::awards(&list);
        let lines = rehearsal::queued(&awards);
        self.medal_popup.rehearse(awards);
        if let Some(console) = &mut self.console {
            console.set_open(false);
        }
        self.sync_cursor_policy();
        Ok(lines)
    }
}

/// What the looks' tests share: the fonts, the longest notes, the moments and the
/// window sizes.
#[cfg(test)]
mod fixtures {
    use super::award::{ENTRANCE, EXIT};
    use crate::medals::{Award, Medal};

    pub(super) struct Fonts {
        pub(super) display: crate::text::FontAtlas,
        pub(super) body: crate::text::FontAtlas,
    }

    /// The SJK UI's families.
    pub(super) fn families() -> Fonts {
        let load = |family| crate::text::load_family(family, 1.0, None).expect("a bundled family");
        Fonts {
            display: load(&crate::text::DISPLAY),
            body: load(&crate::text::BODY),
        }
    }

    /// Inter, which draws until the families load (and the classic menus' text with
    /// `ui_gameFont 0`).
    pub(super) fn inter() -> Fonts {
        Fonts {
            display: crate::text::load_modern(1.0, None).expect("Inter"),
            body: crate::text::load_modern(1.0, None).expect("Inter"),
        }
    }

    /// No note, and the longest: the hub's 200 characters of running text, and of
    /// the widest letters.
    pub(super) fn notes() -> [String; 3] {
        let running = "Thank you for the fog that followed the camera floor, the flickering door on ffa3, the lost lightmaps and every other bug you found and wrote up so clearly for us all.";
        let wide = "WWWWWW MMMMMMM ".repeat(14);
        [
            String::new(),
            crate::medals::plain_note(running),
            crate::medals::plain_note(&wide),
        ]
    }

    /// `medal` with `note`, a repeatable one given a twelfth time.
    pub(super) fn award(medal: Medal, note: &str) -> Award {
        Award {
            medal,
            count: if medal.repeatable() { 12 } else { 1 },
            awarded: 1_791_336_225,
            note: note.to_owned(),
        }
    }

    /// The moments worth checking: the flight, the landing, the burst, the band of
    /// light, standing still, a soft glint while it waits, much later, and leaving.
    pub(super) const MOMENTS: [(f32, Option<f32>); 9] = [
        (0.0, None),
        (0.3, None),
        (0.55, None),
        (0.8, None),
        (1.3, None),
        (ENTRANCE, None),
        (ENTRANCE + 1.5 + 0.45, None),
        (30.0, None),
        (ENTRANCE + 2.0, Some(EXIT * 0.5)),
    ];

    /// 1080 lines, 4K, 4:3, 21:9 and 720 lines.
    pub(super) const VIEWPORTS: [[f32; 2]; 5] = [
        [1920.0, 1080.0],
        [3840.0, 2160.0],
        [1440.0, 1080.0],
        [2560.0, 1080.0],
        [1280.0, 720.0],
    ];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::medals::Medal;

    fn wire(id: &str, count: u32) -> sjk_identity::Medal {
        sjk_identity::Medal {
            id: id.to_owned(),
            count,
            awarded: 0,
            note: String::new(),
        }
    }

    fn award(medal: Medal, count: u32) -> Award {
        Award {
            medal,
            count,
            awarded: 0,
            note: String::new(),
        }
    }

    fn shown(popup: &MedalPopup) -> Option<Medal> {
        popup.current.as_ref().map(|showing| showing.award.medal)
    }

    fn after(start: Instant, seconds: f32) -> Instant {
        start + Duration::from_secs_f32(seconds)
    }

    /// Open the first waiting medal at `start`, picture ready, and stand it still.
    fn open_settled(popup: &mut MedalPopup, start: Instant) -> Instant {
        popup.open_next(start);
        popup.update(start, true, true);
        after(start, ENTRANCE + 0.1)
    }

    /// The names on the cards a match would show now.
    fn cards(popup: &mut MedalPopup) -> Vec<String> {
        let mut names = Vec::new();
        popup.announce(|unlock| names.push(unlock.name().to_string()));
        names
    }

    /// Take the button at `now` and let the medal lift away.
    fn next(popup: &mut MedalPopup, now: Instant) -> Instant {
        popup.press(now, true);
        let later = after(now, EXIT + 0.01);
        popup.update(later, true, true);
        popup.update(later, true, true);
        later
    }

    #[test]
    fn new_medals_wait_show_one_by_one_and_are_remembered() {
        let directory = tempfile::tempdir().expect("a folder");
        let mut popup = MedalPopup::default();
        popup.offer(
            directory.path(),
            "aa",
            &[wire("early_tester", 1), wire("bug_hunter", 1)],
        );
        assert!(popup.pending() && !popup.is_open());
        assert_eq!(cards(&mut popup), ["Early Tester", "Bug Hunter"]);
        assert!(cards(&mut popup).is_empty(), "said once");
        let start = Instant::now();
        let now = open_settled(&mut popup, start);
        assert_eq!(shown(&popup), Some(Medal::EarlyTester));
        let now = next(&mut popup, now);
        assert_eq!(shown(&popup), Some(Medal::BugHunter));
        assert_eq!(popup.step, 1);
        popup.update(now, true, true);
        let now = next(&mut popup, after(now, ENTRANCE + 0.1));
        assert!(!popup.is_open() && !popup.pending());
        popup.update(now, true, true);
        // The same list again, or after a restart, shows nothing.
        popup.offer(
            directory.path(),
            "aa",
            &[wire("early_tester", 1), wire("bug_hunter", 1)],
        );
        assert!(!popup.pending());
        let mut restarted = MedalPopup::default();
        restarted.offer(
            directory.path(),
            "aa",
            &[wire("early_tester", 1), wire("bug_hunter", 1)],
        );
        assert!(!restarted.pending());
        // A repeatable medal given again shows again; another identity sees its own.
        restarted.offer(
            directory.path(),
            "aa",
            &[wire("early_tester", 1), wire("bug_hunter", 2)],
        );
        assert!(restarted.pending());
        restarted.offer(directory.path(), "bb", &[wire("early_tester", 1)]);
        assert_eq!(restarted.queue.len(), 1);
        assert_eq!(restarted.queue[0].award.medal, Medal::EarlyTester);
    }

    #[test]
    fn unknown_medals_never_show() {
        let directory = tempfile::tempdir().expect("a folder");
        let mut popup = MedalPopup::default();
        popup.offer(directory.path(), "aa", &[wire("from_the_future", 1)]);
        assert!(!popup.pending());
    }

    /// The fanfare plays as each medal's ceremony begins, once; a press during the
    /// entrance finishes it without leaving; once still, a key takes Next with the
    /// menus' click, and the medal is gone only after its exit.
    #[test]
    fn each_medal_has_its_fanfare_and_a_press_first_finishes_its_entrance() {
        ui_cues::take_posted();
        let mut popup = MedalPopup::default();
        popup.rehearse(vec![
            award(Medal::EarlyTester, 1),
            award(Medal::EarlyContributor, 1),
        ]);
        let start = Instant::now();
        popup.open_next(start);
        // Not seen yet (the console over it): no ceremony, no sound.
        popup.update(start, false, true);
        assert!(popup.current.as_ref().unwrap().started.is_none());
        assert!(ui_cues::take_posted().is_empty());
        popup.update(start, true, true);
        assert_eq!(ui_cues::take_posted(), [Cue::Medal]);
        // A key in the entrance: it stands still at once, still the same medal.
        let early = after(start, 0.4);
        popup.key(KeyCode::Enter, early);
        let (t, exit) = popup.current.as_ref().unwrap().times(early);
        assert!((t - ENTRANCE).abs() < 1e-4 && exit.is_none());
        assert!(ui_cues::take_posted().is_empty(), "skipping is silent");
        // The next press takes Next: a click, then the medal lifts away.
        let press = after(start, 0.5);
        popup.key(KeyCode::Space, press);
        assert_eq!(ui_cues::take_posted(), [Cue::Click]);
        popup.update(after(start, 0.5 + EXIT * 0.5), true, true);
        assert_eq!(shown(&popup), Some(Medal::EarlyTester), "still leaving");
        popup.key(KeyCode::Enter, after(start, 0.5 + EXIT * 0.6));
        assert!(ui_cues::take_posted().is_empty(), "a key while it leaves");
        let gone = after(start, 0.5 + EXIT + 0.01);
        popup.update(gone, true, true);
        assert_eq!(shown(&popup), Some(Medal::EarlyContributor));
        assert!(
            ui_cues::take_posted().is_empty(),
            "the next begins next frame"
        );
        popup.update(after(start, 0.6 + EXIT), true, true);
        assert_eq!(ui_cues::take_posted(), [Cue::Medal]);
        // Escape and Right act as Enter; other keys and repeats do nothing.
        let now = after(start, 0.6 + EXIT + ENTRANCE + 0.1);
        popup.key(KeyCode::KeyA, now);
        popup.key(KeyCode::Tab, now);
        assert!(popup.current.as_ref().unwrap().leaving.is_none());
        popup.key(KeyCode::Escape, now);
        assert!(popup.current.as_ref().unwrap().leaving.is_some());
        popup.update(after(now, EXIT + 0.01), true, true);
        assert!(!popup.is_open(), "Close on the last");
        let mut right = MedalPopup::default();
        right.rehearse(vec![award(Medal::BugHunter, 2)]);
        let now = open_settled(&mut right, start);
        right.key(KeyCode::ArrowRight, now);
        assert!(right.current.as_ref().unwrap().leaving.is_some());
    }

    /// The ceremony waits for the medal's picture, then begins without it.
    #[test]
    fn the_ceremony_waits_a_moment_for_the_picture() {
        ui_cues::take_posted();
        let mut popup = MedalPopup::default();
        popup.rehearse(vec![award(Medal::EarlyContributor, 1)]);
        let start = Instant::now();
        popup.open_next(start);
        popup.update(after(start, 0.5), true, false);
        assert!(popup.current.as_ref().unwrap().started.is_none());
        popup.update(after(start, 1.6), true, false);
        assert!(popup.current.as_ref().unwrap().started.is_some());
        assert_eq!(ui_cues::take_posted(), [Cue::Medal]);
    }

    /// A rehearsal goes through the same queue and ceremony but never into
    /// `medals_seen.txt`, so the real medal still shows when the hub gives it; the
    /// real one replaces a rehearsal of it still waiting, and a new identity keeps
    /// the rehearsals.
    #[test]
    fn rehearsals_are_never_counted_as_seen() {
        let directory = tempfile::tempdir().expect("a folder");
        let file = directory.path().join(crate::medals::seen::FILE);
        let mut popup = MedalPopup::default();
        popup.offer(directory.path(), "aa", &[]);
        popup.rehearse(vec![
            award(Medal::EarlyTester, 1),
            award(Medal::BugHunter, 3),
        ]);
        popup.rehearse(vec![award(Medal::BugHunter, 4)]);
        assert_eq!(popup.queue.len(), 2, "a rehearsal replaces its own kind");
        assert_eq!(popup.queue[1].award.count, 4);
        let start = Instant::now();
        let now = open_settled(&mut popup, start);
        let now = next(&mut popup, now);
        popup.update(now, true, true);
        next(&mut popup, after(now, ENTRANCE + 0.1));
        assert!(!popup.is_open());
        assert!(!file.exists(), "nothing written");
        // The hub gives the real one now: it shows, and is remembered.
        popup.offer(directory.path(), "aa", &[wire("early_tester", 1)]);
        assert!(popup.pending() && !popup.queue[0].rehearsal);
        let mut fresh = MedalPopup::default();
        fresh.rehearse(vec![award(Medal::EarlyContributor, 1)]);
        fresh.offer(directory.path(), "aa", &[wire("early_contributor", 1)]);
        assert_eq!(fresh.queue.len(), 1);
        assert!(
            !fresh.queue[0].rehearsal,
            "the real one replaces the rehearsal"
        );
        let mut early = MedalPopup::default();
        early.rehearse(vec![award(Medal::EarlyContributor, 1)]);
        early.offer(directory.path(), "bb", &[]);
        assert_eq!(
            early.queue.len(),
            1,
            "the identity arriving keeps rehearsals"
        );
        // In a match the rehearsal's card is the hub's.
        assert_eq!(cards(&mut early), ["Early Contributor"]);
    }

    /// A medal given again while its first count still waits has a card of its own,
    /// with the new count.
    #[test]
    fn a_new_count_waiting_has_its_own_card() {
        let directory = tempfile::tempdir().expect("a folder");
        let mut popup = MedalPopup::default();
        popup.offer(directory.path(), "aa", &[wire("bug_hunter", 1)]);
        assert_eq!(cards(&mut popup), ["Bug Hunter"]);
        popup.offer(directory.path(), "aa", &[wire("bug_hunter", 2)]);
        assert_eq!(popup.queue.len(), 1);
        assert_eq!(cards(&mut popup), ["Bug Hunter x2"]);
    }

    /// The real medal is written as seen when its button is taken, before it has
    /// lifted away.
    #[test]
    fn a_real_medal_is_remembered_when_its_button_is_taken() {
        let directory = tempfile::tempdir().expect("a folder");
        let mut popup = MedalPopup::default();
        popup.offer(directory.path(), "aa", &[wire("early_contributor", 1)]);
        let now = open_settled(&mut popup, Instant::now());
        popup.press(now, false);
        let seen = Seen::load(directory.path(), "aa");
        assert!(!seen.is_new(&award(Medal::EarlyContributor, 1)));
    }
}
