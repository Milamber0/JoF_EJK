//! The SJK UI's main page: SJK's emblem in its turning holo ring over the live
//! map, the menu on an arc round the ring's right side, and the servers the
//! player joined last in a column on the right, each joined with one click.
//!
//! The ring's gold arc points at the chosen entry. Play, SJK and Quit open
//! pages of their own on the same ring (the arc swaps to their entries, as
//! retail's pages swapped inside one frame); Profile and Settings open their
//! screens. Escape leaves a page for the main one, and on the main one asks to
//! quit.
//!
//! Under the servers, the SJK chat is docked (`docs/hub-chat.md`): its last lines,
//! who is online, a field to type in and Open chat for its page. Top left, the donation
//! goal (`crate::donation_goal`): how much of the month's goal came in, a bar and Donate,
//! which opens SJK's Ko-fi page in the browser (`ui_donationGoal 0` hides it).
//!
//! Keys: Up and Down move along the arc, Enter takes the entry, Right (or Tab)
//! moves to the servers and Left (or Escape) back; Down past the last server
//! reaches the chat, where Enter starts typing (Enter sends, Escape stops). The
//! pointer chooses by hovering and acts with a click.
//!
//! The page is laid out on a 16:9 frame of 1080-line pixels centred in the
//! window: a wider window shows more map at the sides, a narrower one scales
//! the frame down to fit its width.

use super::chat_dock::{self, Dock, DockAction};
use super::recent::Ago;
use super::{Frame, color, fade, fade_across, key_hint, key_hint_width, text};
use crate::menu::MainDestination;
use crate::menu::emblem::{self, EmblemLayer};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use sjk_ui::{DrawCommand, FontWeight, Rect, TextAlign};
use winit::keyboard::KeyCode;

/// The JoF community's server, suggested while the player has joined none.
pub(crate) const JOF_SERVER: &str = "135.125.145.49:29070";

/// Pointer tokens: the arc's entries, then the servers, then the chat's field and
/// its Open chat.
const ENTRY_TOKEN: u16 = 0;
const SERVER_TOKEN: u16 = 20;
const CHAT_TOKEN: u16 = 40;
const OPEN_CHAT_TOKEN: u16 = 41;
/// The player's profile card, bottom left.
const CARD_TOKEN: u16 = 50;
/// The dock's names, one a row (a chat sender's card shows under the pointer), and
/// that sender card and its Mute: apart from every other target of the page.
const SENDER_NAME_TOKEN: u16 = 60;
const SENDER_CARD_TOKEN: u16 = 70;
const SENDER_MUTE_TOKEN: u16 = 71;
/// The donation goal's Donate button.
const DONATE_TOKEN: u16 = 80;
/// The dock's targets ([`chat_dock`]).
const DOCK_TOKENS: chat_dock::Tokens = chat_dock::Tokens {
    field: CHAT_TOKEN,
    open: OPEN_CHAT_TOKEN,
    names: SENDER_NAME_TOKEN,
    card: SENDER_CARD_TOKEN,
    mute: SENDER_MUTE_TOKEN,
};

/// The pages the ring shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Page {
    Main,
    Play,
    Sjk,
    Quit,
}

impl Page {
    fn entries(self) -> &'static [Entry] {
        match self {
            Self::Main => &MAIN,
            Self::Play => &PLAY,
            Self::Sjk => &SJK,
            Self::Quit => &QUIT,
        }
    }

    /// The main page's entry that opens this page.
    fn parent_entry(self) -> usize {
        match self {
            Self::Main | Self::Play => 0,
            Self::Sjk => 4,
            Self::Quit => 5,
        }
    }

    /// The page's name over its entries; none on the main page.
    fn title(self) -> Option<&'static str> {
        match self {
            Self::Main => None,
            Self::Play => Some("Play"),
            Self::Sjk => Some("SJK"),
            Self::Quit => Some("Quit"),
        }
    }
}

/// What an entry does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Step {
    /// Show another page.
    Page(Page),
    /// Leave the page for the main one.
    Back,
    /// Something the menu carries out ([`Action`]).
    Act(Action),
}

/// What the page asks the menu to do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    /// Open a screen the main menu leads to.
    Open(MainDestination),
    /// Join server `i` of the column.
    Join(usize),
    /// Send what was typed in the chat's field ([`Home::take_draft`]).
    SendChat,
    /// Open the SJK chat's page.
    OpenChat,
    /// Mute, on this PC, the player whose sender card the dock showed (its Mute,
    /// [`Home::take_mute`]).
    Mute,
    /// Open the donation page in the browser (the donation goal's Donate).
    Donate,
    /// Exit to the desktop.
    Quit,
}

/// One entry of the arc.
#[derive(Clone, Copy, Debug)]
struct Entry {
    label: &'static str,
    hint: &'static str,
    step: Step,
}

const fn entry(label: &'static str, hint: &'static str, step: Step) -> Entry {
    Entry { label, hint, step }
}

const fn open(destination: MainDestination) -> Step {
    Step::Act(Action::Open(destination))
}

const MAIN: [Entry; 6] = [
    entry(
        "Play",
        "Join a server, or start your own",
        Step::Page(Page::Play),
    ),
    entry(
        "Profile",
        "Character, Force and SJK profile",
        open(MainDestination::Player),
    ),
    entry(
        "Collection",
        "Your medals, achievements, shaders and toys",
        open(MainDestination::Collection),
    ),
    entry(
        "Settings",
        "Every option and key, with search",
        open(MainDestination::Settings { tab: 0 }),
    ),
    entry(
        "SJK",
        "What's new, updates and credits",
        Step::Page(Page::Sjk),
    ),
    entry("Quit", "Leave SJK", Step::Page(Page::Quit)),
];
const PLAY: [Entry; 3] = [
    entry(
        "Join a server",
        "Every server, favourites first",
        open(MainDestination::Browser),
    ),
    entry(
        "Create a game",
        "A match with bots on this machine",
        open(MainDestination::CreateGame),
    ),
    entry("Back", "", Step::Back),
];
const SJK: [Entry; 4] = [
    entry(
        "What's new",
        "Every release and who made it",
        open(MainDestination::Changelog),
    ),
    entry(
        "Update",
        "Check for a newer SJK",
        open(MainDestination::Update),
    ),
    entry(
        "Credits",
        "The people who make SJK",
        open(MainDestination::Credits),
    ),
    entry("Back", "", Step::Back),
];
const QUIT: [Entry; 2] = [
    entry(
        "Quit to desktop",
        "Your settings are saved",
        Step::Act(Action::Quit),
    ),
    entry("Stay", "Back to the menu", Step::Back),
];

/// A server in the column, as the page shows it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ServerItem<'a> {
    /// The server's name, with its colour codes; its address when unknown.
    pub(crate) name: &'a str,
    pub(crate) map: &'a str,
    /// Players, capacity and ping, while the server list has the server.
    pub(crate) live: Option<(u16, u16, u32)>,
    /// When the player last joined it; `None` for the suggested JoF server.
    pub(crate) played: Option<Ago>,
}

/// What the page shows of the player and the servers this frame.
pub(crate) struct HomeView<'a> {
    pub(crate) name: &'a str,
    pub(crate) model: &'a str,
    pub(crate) blade_name: &'static str,
    /// The servers joined last, newest first, or the suggested JoF server.
    pub(crate) servers: &'a [ServerItem<'a>],
    /// The build's version, `2026.1007.1`.
    pub(crate) version: &'a str,
    /// A newer release the update check found.
    pub(crate) update: Option<&'a str>,
    pub(crate) seconds: f64,
    /// The SJK chat, docked under the servers; `None` while it is off.
    pub(crate) chat: Option<ChatDock<'a>>,
    /// What the profile card says of the player's SJK profile.
    pub(crate) summary: &'a crate::profile_card::Summary,
    /// The donation goal, top left; `None` while it is hidden (`ui_donationGoal 0`).
    pub(crate) donation: Option<crate::donation_goal::Card>,
}

pub(crate) use super::chat_dock::ChatDock;
#[cfg(test)]
pub(crate) use super::chat_dock::DockLine;

/// Where the keyboard is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Focus {
    Arc,
    Server(usize),
    /// The SJK chat's field.
    Chat,
    /// The player's profile card, bottom left.
    Card,
}

/// The page's state: which page of the ring, its chosen entry, the keyboard's
/// place, and the gold arc's motion.
#[derive(Debug)]
pub(crate) struct Home {
    page: Page,
    entry: usize,
    focus: Focus,
    /// Where the gold arc points (radians, clockwise from the right), easing
    /// towards its target; `None` until the first frame.
    arc: Option<f32>,
    /// Menu time of the last frame, for the arc's easing.
    last: f64,
    /// Whether the last frame docked the chat.
    dock: bool,
    /// The docked chat's field and sender card.
    chat: Dock,
}

impl Default for Home {
    fn default() -> Self {
        Self {
            page: Page::Main,
            entry: 0,
            focus: Focus::Arc,
            arc: None,
            last: 0.0,
            dock: false,
            chat: Dock::default(),
        }
    }
}

impl Home {
    /// The page `page` with `entry` chosen and the keyboard on server `server`
    /// (or the arc), for the menu snapshots.
    #[cfg(test)]
    pub(crate) fn for_snapshot(page: Page, entry: usize, server: Option<usize>) -> Self {
        Self {
            page,
            entry,
            focus: server.map_or(Focus::Arc, Focus::Server),
            ..Self::default()
        }
    }

    /// `draft` typed in the dock's field, the keyboard there; without one, the keyboard
    /// on the arc (world shots).
    #[cfg(test)]
    pub(crate) fn chat_for_shot(&mut self, draft: Option<&str>) {
        match draft {
            Some(draft) => {
                self.focus = Focus::Chat;
                self.chat.type_for_shot(draft);
            }
            None => {
                self.focus = Focus::Arc;
                self.chat.stop_typing();
            }
        }
    }

    /// Back on the main page's first entry, the keyboard on the arc.
    pub(crate) fn reset(&mut self) {
        self.page = Page::Main;
        self.entry = 0;
        self.focus = Focus::Arc;
        self.chat.stop_typing();
    }

    /// Whether the chat's field takes the keys.
    pub(crate) fn is_typing(&self) -> bool {
        self.chat.is_typing()
    }

    /// What Enter sent from the chat's field.
    pub(crate) fn take_draft(&mut self) -> String {
        self.chat.take_draft()
    }

    /// The key and name of the player the dock's card asked to mute ([`Action::Mute`]).
    pub(crate) fn take_mute(&mut self) -> Option<(String, String)> {
        self.chat.take_mute()
    }

    /// Give the dock's sender card on show its player's picture version (`picture`, from
    /// their key) until it is known. Called before the page is built, outside any lock.
    pub(crate) fn place_sender_card(&mut self, picture: impl FnOnce(&str) -> Option<String>) {
        self.chat.picture_sender_card(picture);
    }

    /// A key while typing in the chat's field: text goes into it, Backspace takes a
    /// character back, Enter sends, Escape stops; every other key is swallowed, so no
    /// menu key acts.
    pub(crate) fn typing_key(
        &mut self,
        key: KeyCode,
        text: Option<&str>,
        shift: bool,
        control: bool,
    ) -> Option<Action> {
        self.chat
            .typing_key(key, text, shift, control)
            .map(|_| Action::SendChat)
    }

    fn entries(&self) -> &'static [Entry] {
        self.page.entries()
    }

    /// Show `page`, its entry `entry` chosen.
    fn show(&mut self, page: Page, entry: usize) {
        self.page = page;
        self.entry = entry.min(page.entries().len() - 1);
        self.focus = Focus::Arc;
    }

    /// Take the chosen entry.
    fn take(&mut self) -> Option<Action> {
        match self.entries().get(self.entry)?.step {
            // Quit's page opens on Stay, so a stray Enter does not quit.
            Step::Page(Page::Quit) => self.show(Page::Quit, 1),
            Step::Page(page) => self.show(page, 0),
            Step::Back => self.back(),
            Step::Act(action) => return Some(action),
        }
        None
    }

    /// Leave a page for the main one, on the entry that opened it.
    fn back(&mut self) {
        let entry = self.page.parent_entry();
        self.show(Page::Main, entry);
    }

    /// A key on the page, with `servers` in the column; the action it takes.
    pub(crate) fn key(&mut self, key: KeyCode, servers: usize) -> Option<Action> {
        let count = self.entries().len();
        match (key, self.focus) {
            (KeyCode::ArrowUp | KeyCode::KeyW, Focus::Arc) => {
                self.entry = (self.entry + count - 1) % count;
            }
            (KeyCode::ArrowDown | KeyCode::KeyS, Focus::Arc) => {
                self.entry = (self.entry + 1) % count;
            }
            (KeyCode::ArrowRight | KeyCode::KeyD | KeyCode::Tab, Focus::Arc) if servers > 0 => {
                self.focus = Focus::Server(0);
            }
            (KeyCode::ArrowLeft | KeyCode::KeyA, Focus::Arc) => self.focus = Focus::Card,
            (KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space, Focus::Card) => {
                return Some(Action::Open(MainDestination::Profile));
            }
            (
                KeyCode::ArrowRight
                | KeyCode::KeyD
                | KeyCode::ArrowUp
                | KeyCode::KeyW
                | KeyCode::Tab
                | KeyCode::Escape,
                Focus::Card,
            ) => self.focus = Focus::Arc,
            (KeyCode::ArrowUp | KeyCode::KeyW, Focus::Server(index)) => {
                self.focus = Focus::Server((index + servers - 1) % servers);
            }
            (KeyCode::ArrowDown | KeyCode::KeyS, Focus::Server(index))
                if self.dock && index + 1 >= servers =>
            {
                self.focus = Focus::Chat;
            }
            (KeyCode::ArrowDown | KeyCode::KeyS, Focus::Server(index)) => {
                self.focus = Focus::Server((index + 1) % servers);
            }
            (KeyCode::ArrowUp | KeyCode::KeyW, Focus::Chat) => {
                self.focus = Focus::Server(servers.saturating_sub(1));
            }
            (KeyCode::ArrowLeft | KeyCode::KeyA | KeyCode::Tab | KeyCode::Escape, Focus::Chat) => {
                self.focus = Focus::Arc;
            }
            (KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space, Focus::Chat) => {
                self.chat.start_typing();
            }
            (
                KeyCode::ArrowLeft | KeyCode::KeyA | KeyCode::Tab | KeyCode::Escape,
                Focus::Server(_),
            ) => {
                self.focus = Focus::Arc;
            }
            (KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space, Focus::Server(index)) => {
                return (index < servers).then_some(Action::Join(index));
            }
            (KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space, Focus::Arc) => {
                return self.take();
            }
            (KeyCode::Escape | KeyCode::Backspace, Focus::Arc) if self.page != Page::Main => {
                self.back();
            }
            (KeyCode::Escape, Focus::Arc) => self.show(Page::Quit, 1),
            _ => {}
        }
        None
    }

    /// The pointer over (or clicking) `token`, with `servers` in the column:
    /// hovering chooses, a click acts.
    pub(crate) fn pointer(&mut self, token: u16, activate: bool, servers: usize) -> Option<Action> {
        let entries = ENTRY_TOKEN..ENTRY_TOKEN + self.entries().len() as u16;
        if entries.contains(&token) {
            self.entry = usize::from(token - ENTRY_TOKEN);
            self.focus = Focus::Arc;
            return if activate { self.take() } else { None };
        }
        if self.dock
            && let Some(reply) = self.chat.pointer(DOCK_TOKENS, token, activate)
        {
            if token == CHAT_TOKEN || token == OPEN_CHAT_TOKEN {
                self.focus = Focus::Chat;
            }
            return reply.map(|action| match action {
                DockAction::Send => Action::SendChat,
                DockAction::Open => Action::OpenChat,
                DockAction::Mute => Action::Mute,
            });
        }
        match token {
            CARD_TOKEN => {
                self.focus = Focus::Card;
                return activate.then_some(Action::Open(MainDestination::Profile));
            }
            DONATE_TOKEN => return activate.then_some(Action::Donate),
            _ => {}
        }
        let index = usize::from(token.checked_sub(SERVER_TOKEN)?);
        if index >= servers {
            return None;
        }
        self.focus = Focus::Server(index);
        activate.then_some(Action::Join(index))
    }

    /// Where the gold arc points this frame, eased towards `target` since the
    /// last frame at menu time `seconds`.
    fn arc_towards(&mut self, target: f32, seconds: f64) -> f32 {
        let elapsed = (seconds - self.last).clamp(0.0, 0.1) as f32;
        self.last = seconds;
        let current = self.arc.unwrap_or(target);
        let mut off = (target - current).rem_euclid(std::f32::consts::TAU);
        if off > std::f32::consts::PI {
            off -= std::f32::consts::TAU;
        }
        let next = current + off * (1.0 - (-elapsed * 9.0).exp());
        self.arc = Some(next);
        next
    }
}

/// The ring's centre, its radius and the arc's radius, in frame pixels.
const RING: [f32; 2] = [620.0, 540.0];
const RING_RADIUS: f32 = 312.0;
const ARC_RADIUS: f32 = 440.0;
/// Degrees between entries along the arc; closer for six, so the ends keep clear of
/// the ring ([`arc_step`]).
const ARC_STEP: f32 = 16.0;
const ARC_STEP_SIX: f32 = 12.5;
/// The servers' column: its rule's x, its text's x and width, its top and the
/// height of one server.
const COLUMN_RULE: f32 = 1470.0;
const COLUMN_X: f32 = 1500.0;
const COLUMN_WIDTH: f32 = 324.0;
const COLUMN_TOP: f32 = 236.0;
const SERVER_HEIGHT: f32 = 96.0;
/// The keys' line.
const KEYS_Y: f32 = 1004.0;
/// The docked chat, under the servers; a sender's card shows on its left, above the keys.
const DOCK: chat_dock::Place = chat_dock::Place {
    x: COLUMN_X,
    top: 700.0,
    width: COLUMN_WIDTH,
    card_floor: KEYS_Y - 24.0,
};

/// Degrees between entries along an arc of `count`.
fn arc_step(count: usize) -> f32 {
    if count > 5 { ARC_STEP_SIX } else { ARC_STEP }
}

/// Angle (radians) of entry `index` of `count` round the ring.
fn entry_angle(index: usize, count: usize) -> f32 {
    ((index as f32 - (count as f32 - 1.0) * 0.5) * arc_step(count)).to_radians()
}

/// Where entry `index` of `count` starts, in frame pixels: on the arc, at its
/// vertical middle.
fn entry_point(index: usize, count: usize) -> [f32; 2] {
    let angle = entry_angle(index, count);
    [
        RING[0] + ARC_RADIUS * angle.cos(),
        RING[1] + ARC_RADIUS * angle.sin(),
    ]
}

/// Build the page into `canvas` at `reveal` opacity.
pub(crate) fn build(
    canvas: &mut MenuCanvas,
    viewport: [f32; 2],
    home: &mut Home,
    view: &HomeView<'_>,
    reveal: f32,
) {
    let frame = Frame::new(viewport);
    let s = frame.s;
    let seconds = view.seconds;
    canvas.begin_transparent(viewport);
    canvas.push_opacity(reveal);
    scrims(canvas, viewport, &frame);

    // The emblem in its ring, the sunburst's warmth behind it.
    let centre = frame.point(RING[0], RING[1]);
    let turn = (seconds * std::f64::consts::TAU / 240.0) as f32;
    emblem::rays(
        canvas,
        EmblemLayer::Sunburst,
        centre,
        420.0 * s,
        -turn * 0.6,
        color::alpha(color::GOLD, 0.18),
    );
    emblem::rays(
        canvas,
        EmblemLayer::Ring,
        centre,
        RING_RADIUS * s,
        turn,
        color::alpha(color::HOLO, 0.9),
    );
    emblem::draw(
        canvas,
        Rect::new(
            centre[0] - 190.0 * s,
            centre[1] - 190.0 * s,
            380.0 * s,
            380.0 * s,
        ),
        seconds,
    );

    // The gold arc points at the chosen entry, or at the servers.
    home.dock = view.chat.is_some();
    if !home.dock {
        home.chat.hide();
        if home.focus == Focus::Chat {
            home.focus = Focus::Arc;
        }
    }
    let entries = home.entries();
    let target = match home.focus {
        Focus::Arc => entry_angle(home.entry, entries.len()),
        Focus::Server(index) => {
            let y = COLUMN_TOP + 64.0 + (index as f32 + 0.5) * SERVER_HEIGHT;
            (y - RING[1]).atan2(COLUMN_X - RING[0])
        }
        Focus::Chat => (DOCK.field_top() + 16.0 - RING[1]).atan2(COLUMN_X - RING[0]),
        Focus::Card => {
            let centre = crate::profile_card::PICTURE[1] + crate::profile_card::PICTURE_SIZE * 0.5;
            (centre - RING[1]).atan2(crate::profile_card::PICTURE[0] + 200.0 - RING[0])
        }
    };
    let arc = home.arc_towards(target, seconds);
    let sweep = 0.42;
    let _ = canvas.draw_list_mut().push(DrawCommand::Arc {
        center: centre,
        radius: (RING_RADIUS + 6.0) * s,
        width: 6.0 * s,
        start: arc - sweep * 0.5,
        sweep,
        color: color::GOLD_BRIGHT,
        knockout: None,
    });
    // A short beam from the arc towards its entry.
    if home.focus == Focus::Arc {
        let inner = RING_RADIUS + 12.0;
        let outer = ARC_RADIUS - 18.0;
        let steps = 12;
        for step in 0..steps {
            let t = step as f32 / steps as f32;
            let r = inner + (outer - inner) * t;
            let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: Rect::new(
                    centre[0] + r * arc.cos() * s - 1.2 * s,
                    centre[1] + r * arc.sin() * s - 1.2 * s,
                    2.4 * s,
                    2.4 * s,
                ),
                radius: 1.2 * s,
                color: color::alpha(color::GOLD_BRIGHT, 0.85 * (1.0 - t)),
            });
        }
    }

    page_title(canvas, &frame, home);
    for (index, entry) in entries.iter().enumerate() {
        let chosen = index == home.entry;
        let lit = chosen && home.focus == Focus::Arc;
        let size = if chosen { 68.0 } else { 44.0 };
        let colour = match (lit, chosen, entry.step) {
            // Leaving is the one thing in ember.
            (true, _, Step::Act(Action::Quit)) => color::EMBER,
            (true, _, _) => color::GOLD_BRIGHT,
            (false, true, _) => color::TEXT,
            (false, false, Step::Back) => color::QUIET,
            (false, false, Step::Page(Page::Quit)) => color::QUIET,
            (false, false, _) => color::MUTED,
        };
        let [x, y] = entry_point(index, entries.len());
        text(
            canvas,
            TextFamily::Display,
            format_args!("{}", entry.label),
            frame.rect(x, y - size * 0.62, 640.0, size * 1.2),
            size * s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
        if chosen && !entry.hint.is_empty() {
            text(
                canvas,
                TextFamily::Body,
                format_args!("{}", entry.hint),
                frame.rect(x + 2.0, y + 32.0, 520.0, 24.0),
                18.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        let pitch = ARC_RADIUS * arc_step(entries.len()).to_radians();
        canvas.hit_region(
            ENTRY_TOKEN + index as u16,
            frame.rect(x - 16.0, y - pitch * 0.5, COLUMN_RULE - x - 40.0, pitch),
        );
    }

    servers(canvas, &frame, home, view);
    if let Some(card) = view.donation {
        donation(canvas, &frame, card);
    }
    if let Some(dock) = &view.chat {
        let focused = home.focus == Focus::Chat;
        chat_dock::draw(
            canvas,
            &frame,
            DOCK,
            &mut home.chat,
            dock,
            focused,
            DOCK_TOKENS,
        );
    }
    crate::profile_card::draw(
        canvas,
        &frame,
        &crate::profile_card::Card {
            name: view.name,
            detail: crate::profile_card::Detail::Model(
                view.model,
                view.blade_name,
                view.summary.skin,
            ),
            summary: view.summary,
            lit: home.focus == Focus::Card,
        },
        CARD_TOKEN,
    );
    hints(canvas, &frame, home, !view.servers.is_empty());
    version(canvas, &frame, view);
    // The card goes over everything, its targets last.
    match &view.chat {
        Some(dock) => {
            chat_dock::sender_card(canvas, &frame, DOCK, &mut home.chat, dock, DOCK_TOKENS);
        }
        None => home.chat.sender_card = None,
    }
    canvas.pop_opacity();
    let selected = match home.focus {
        Focus::Arc => ENTRY_TOKEN + home.entry as u16,
        Focus::Server(index) => SERVER_TOKEN + index as u16,
        Focus::Chat => CHAT_TOKEN,
        Focus::Card => CARD_TOKEN,
    };
    canvas.finish(selected);
}

/// Fades keeping text readable over any map: dark on the left behind the ring
/// and the arc, lighter in the middle, dark again behind the servers, and at
/// the top and bottom.
fn scrims(canvas: &mut MenuCanvas, viewport: [f32; 2], frame: &Frame) {
    let [width, height] = viewport;
    let space = |alpha| color::alpha(color::SPACE, alpha);
    // The frame's stops, stretched to the window's edges.
    let x = |frame_x: f32| frame.point(frame_x, 0.0)[0];
    let stops = [
        (0.0, 0.92),
        (x(730.0), 0.78),
        (x(1190.0), 0.35),
        (x(1400.0), 0.62),
        (width, 0.78),
    ];
    for pair in stops.windows(2) {
        let ((left, from), (right, to)) = (pair[0], pair[1]);
        if right > left {
            fade_across(
                canvas,
                Rect::new(left, 0.0, right - left, height),
                space(from),
                space(to),
            );
        }
    }
    fade(
        canvas,
        Rect::new(0.0, 0.0, width, height * 0.22),
        space(0.55),
        space(0.0),
    );
    fade(
        canvas,
        Rect::new(0.0, height * 0.72, width, height * 0.28),
        space(0.0),
        space(0.85),
    );
}

/// A sub-page's name over its first entry.
fn page_title(canvas: &mut MenuCanvas, frame: &Frame, home: &Home) {
    let Some(title) = home.page.title() else {
        return;
    };
    let count = home.entries().len();
    let angle = entry_angle(0, count) - (arc_step(count) * 1.1).to_radians();
    let x = RING[0] + ARC_RADIUS * angle.cos();
    let y = RING[1] + ARC_RADIUS * angle.sin();
    text(
        canvas,
        TextFamily::Display,
        format_args!("{title}"),
        frame.rect(x, y - 16.0, 400.0, 30.0),
        24.0 * frame.s,
        color::alpha(color::HOLO, 0.85),
        FontWeight::Semibold,
        TextAlign::Start,
    );
}

/// The servers' column: the ones joined last, or the suggested JoF server.
fn servers(canvas: &mut MenuCanvas, frame: &Frame, home: &Home, view: &HomeView<'_>) {
    let s = frame.s;
    let count = view.servers.len();
    let suggested = view
        .servers
        .first()
        .is_some_and(|server| server.played.is_none());
    let bottom = COLUMN_TOP + 64.0 + count.max(1) as f32 * SERVER_HEIGHT;
    let rule = color::alpha(color::HOLO, 0.6);
    let clear = color::alpha(color::HOLO, 0.0);
    let rule_top = COLUMN_TOP - 10.0;
    let height = bottom - rule_top;
    fade(
        canvas,
        frame.rect(COLUMN_RULE, rule_top, 2.0, height * 0.18),
        clear,
        rule,
    );
    let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
        rect: frame.rect(COLUMN_RULE, rule_top + height * 0.18, 2.0, height * 0.64),
        color: rule,
    });
    fade(
        canvas,
        frame.rect(COLUMN_RULE, rule_top + height * 0.82, 2.0, height * 0.18),
        rule,
        clear,
    );
    text(
        canvas,
        TextFamily::Display,
        format_args!(
            "{}",
            if suggested {
                "Start here"
            } else {
                "Recent servers"
            }
        ),
        frame.rect(COLUMN_X, COLUMN_TOP, COLUMN_WIDTH, 34.0),
        28.0 * s,
        color::TEXT,
        FontWeight::Regular,
        TextAlign::Start,
    );
    for (index, server) in view.servers.iter().enumerate() {
        let top = COLUMN_TOP + 64.0 + index as f32 * SERVER_HEIGHT;
        let token = SERVER_TOKEN + index as u16;
        let focused = home.focus == Focus::Server(index) || canvas.token_hovered(token);
        let target = frame.rect(
            COLUMN_X - 14.0,
            top - 8.0,
            COLUMN_WIDTH + 24.0,
            SERVER_HEIGHT - 8.0,
        );
        if focused {
            let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: target,
                radius: 10.0 * s,
                color: color::alpha(color::HOLO, 0.1),
            });
            let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: frame.rect(COLUMN_X - 14.0, top - 2.0, 3.0, SERVER_HEIGHT - 20.0),
                radius: 1.5 * s,
                color: color::GOLD_BRIGHT,
            });
        }
        text(
            canvas,
            TextFamily::Display,
            format_args!("{}", server.name),
            frame.rect(COLUMN_X, top, COLUMN_WIDTH, 32.0),
            27.0 * s,
            if focused {
                color::GOLD_BRIGHT
            } else {
                color::TEXT
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        let detail = frame.rect(COLUMN_X, top + 34.0, COLUMN_WIDTH, 22.0);
        let quiet = frame.rect(COLUMN_X, top + 56.0, COLUMN_WIDTH, 20.0);
        let body = |canvas: &mut MenuCanvas, rect, colour, value: std::fmt::Arguments<'_>| {
            text(
                canvas,
                TextFamily::Body,
                value,
                rect,
                16.0 * s,
                colour,
                FontWeight::Regular,
                TextAlign::Start,
            );
        };
        match server.live {
            Some((players, capacity, ping)) => body(
                canvas,
                detail,
                color::MUTED,
                format_args!("{players} of {capacity} on {}, {ping} ms", server.map),
            ),
            None if server.map.is_empty() => {
                body(
                    canvas,
                    detail,
                    color::MUTED,
                    format_args!("Not in the server list"),
                );
            }
            None => body(canvas, detail, color::MUTED, format_args!("{}", server.map)),
        }
        match server.played {
            Some(ago) => body(canvas, quiet, color::QUIET, format_args!("Played {ago}")),
            None => body(
                canvas,
                quiet,
                color::QUIET,
                format_args!("The JoF community's server"),
            ),
        }
        canvas.hit_region(token, target);
    }
}

/// The donation goal's place, top left: its left edge, its top and its width.
const DONATION: [f32; 3] = [96.0, 58.0, 420.0];

/// The donation goal, top left, clear of the ring: "Support SJK" and Donate, then how
/// much of the goal came in and its bar, then what a donation over 1 euro brings.
fn donation(canvas: &mut MenuCanvas, frame: &Frame, card: crate::donation_goal::Card) {
    use crate::donation_goal::Amount;
    let s = frame.s;
    let [x, top, width] = DONATION;
    let button = 128.0;
    text(
        canvas,
        TextFamily::Display,
        format_args!("Support SJK"),
        frame.rect(x, top + 4.0, width - button - 12.0, 32.0),
        26.0 * s,
        color::TEXT,
        FontWeight::Regular,
        TextAlign::Start,
    );
    super::kit::button(
        canvas,
        frame,
        [x + width - button, top, button, 40.0],
        "Donate",
        true,
        true,
        false,
        DONATE_TOKEN,
    );
    let body =
        |canvas: &mut MenuCanvas, y: f32, size: f32, colour, value: std::fmt::Arguments<'_>| {
            text(
                canvas,
                TextFamily::Body,
                value,
                frame.rect(x, y, width, size + 6.0),
                size * s,
                colour,
                FontWeight::Regular,
                TextAlign::Start,
            );
        };
    let mut y = top + 48.0;
    match card.numbers {
        Some(numbers) => {
            let raised = Amount {
                cents: numbers.raised_cents,
                euros: numbers.euros,
            };
            let goal = Amount {
                cents: numbers.goal_cents,
                euros: numbers.euros,
            };
            let period = if numbers.monthly { " this month" } else { "" };
            body(
                canvas,
                y,
                17.0,
                color::MUTED,
                format_args!("{raised} of {goal}{period}"),
            );
            y += 30.0;
            let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: frame.rect(x, y, width, 8.0),
                radius: 4.0 * s,
                color: color::alpha(color::HOLO, 0.16),
            });
            let fill = numbers.fill();
            if fill > 0.0 {
                let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: frame.rect(x, y, (width * fill).max(8.0), 8.0),
                    radius: 4.0 * s,
                    color: color::GOLD_BRIGHT,
                });
            }
            y += 18.0;
        }
        None => {
            body(
                canvas,
                y,
                17.0,
                color::MUTED,
                format_args!("Help fund SJK's development on Ko-fi"),
            );
            y += 28.0;
        }
    }
    body(
        canvas,
        y,
        14.0,
        color::QUIET,
        format_args!("More than 1 \u{20ac} brings the Supporter medal"),
    );
}

/// The keys of the page, bottom centre.
fn hints(canvas: &mut MenuCanvas, frame: &Frame, home: &Home, servers: bool) {
    let s = frame.s;
    let arc_back = if home.page == Page::Main {
        "quit"
    } else {
        "back"
    };
    let rows: [(&[&str], &str); 3] = match home.focus {
        Focus::Arc if servers => [
            (&["Up", "Down"], "turn the ring"),
            (&["Enter"], "open"),
            (&["Right"], "servers"),
        ],
        Focus::Arc => [
            (&["Up", "Down"], "turn the ring"),
            (&["Enter"], "open"),
            (&["Esc"], arc_back),
        ],
        Focus::Server(_) => [
            (&["Up", "Down"], "choose"),
            (&["Enter"], "join"),
            (&["Left"], "back"),
        ],
        Focus::Chat if home.is_typing() => [
            (&["Enter"], "send"),
            (&["Esc"], "stop typing"),
            (&["Backspace"], "erase"),
        ],
        Focus::Chat => [
            (&["Enter"], "type"),
            (&["Up"], "servers"),
            (&["Left"], "back"),
        ],
        Focus::Card => [
            (&["Enter"], "open your profile"),
            (&["Right"], "back"),
            (&["Esc"], "back"),
        ],
    };
    let gap = 28.0 * s;
    let total: f32 = rows
        .iter()
        .map(|(keys, action)| key_hint_width(keys, action, s))
        .sum::<f32>()
        + gap * (rows.len() - 1) as f32;
    let [centre, y] = frame.point(960.0, KEYS_Y);
    let mut x = centre - total * 0.5;
    for (keys, action) in rows {
        x = key_hint(canvas, keys, action, x, y, s) + gap;
    }
}

/// The version, bottom right, and the newer release the update check found.
fn version(canvas: &mut MenuCanvas, frame: &Frame, view: &HomeView<'_>) {
    let s = frame.s;
    text(
        canvas,
        TextFamily::Display,
        format_args!("SJK {}", view.version),
        frame.rect(1224.0, 962.0, 600.0, 28.0),
        22.0 * s,
        color::TEXT,
        FontWeight::Regular,
        TextAlign::End,
    );
    if let Some(update) = view.update {
        text(
            canvas,
            TextFamily::Body,
            format_args!("SJK {update} is ready to install"),
            frame.rect(1224.0, 994.0, 600.0, 22.0),
            16.0 * s,
            color::GOLD_BRIGHT,
            FontWeight::Regular,
            TextAlign::End,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORTS: [[f32; 2]; 6] = [
        [1_920.0, 1_080.0],
        [3_840.0, 2_160.0],
        [2_560.0, 1_080.0],
        [1_440.0, 1_080.0],
        [1_280.0, 1_024.0],
        [1_024.0, 768.0],
    ];

    #[test]
    fn the_frame_fits_every_window_and_keeps_its_proportions() {
        for viewport in VIEWPORTS {
            let frame = Frame::new(viewport);
            let corner = frame.point(1920.0, 1080.0);
            assert!(
                frame.origin[0] >= -0.01 && frame.origin[1] >= -0.01,
                "{viewport:?}"
            );
            assert!(corner[0] <= viewport[0] + 0.01 && corner[1] <= viewport[1] + 0.01);
            // Centred both ways.
            assert!((frame.origin[0] - (viewport[0] - corner[0])).abs() < 0.01);
        }
        // 16:9 fills the window; 4:3 scales the frame to its width.
        assert_eq!(Frame::new([3_840.0, 2_160.0]).s, 2.0);
        assert_eq!(Frame::new([1_440.0, 1_080.0]).s, 0.75);
    }

    #[test]
    fn the_arc_clears_the_ring_and_the_column() {
        for page in [Page::Main, Page::Play, Page::Sjk, Page::Quit] {
            let count = page.entries().len();
            for index in 0..count {
                let [x, y] = entry_point(index, count);
                assert!(x > RING[0] + RING_RADIUS + 60.0, "{page:?} {index}");
                // The widest label at the chosen size ("Quit to desktop", about
                // 400 pixels of Rajdhani at 68) stays short of the column.
                assert!(x + 400.0 < COLUMN_RULE, "{page:?} {index}");
                assert!(y > 120.0 && y < 900.0, "{page:?} {index}");
            }
        }
    }

    // Four servers end above the corners' text, and the dock between them.
    const _: () = assert!(COLUMN_TOP + 64.0 + 4.0 * SERVER_HEIGHT < DOCK.top);
    const _: () = assert!(DOCK.bottom() < 962.0);

    #[test]
    fn pages_open_and_close_on_the_ring() {
        let mut home = Home::default();
        // Play opens its page; its Back returns to Play.
        assert_eq!(home.key(KeyCode::Enter, 0), None);
        assert_eq!(home.page, Page::Play);
        assert_eq!(
            home.key(KeyCode::Enter, 0),
            Some(Action::Open(MainDestination::Browser))
        );
        home.key(KeyCode::ArrowUp, 0);
        assert_eq!(home.key(KeyCode::Enter, 0), None, "Back");
        assert_eq!((home.page, home.entry), (Page::Main, 0));
        // Escape on the main page asks to quit, on Stay; Escape again leaves.
        home.key(KeyCode::Escape, 0);
        assert_eq!((home.page, home.entry), (Page::Quit, 1));
        home.key(KeyCode::Escape, 0);
        assert_eq!((home.page, home.entry), (Page::Main, 5));
        // Quit to desktop.
        home.key(KeyCode::Enter, 0);
        home.key(KeyCode::ArrowUp, 0);
        assert_eq!(home.key(KeyCode::Enter, 0), Some(Action::Quit));
        // Collection opens its screen, Settings its own, straight away.
        home.reset();
        home.key(KeyCode::ArrowDown, 0);
        home.key(KeyCode::ArrowDown, 0);
        assert_eq!(
            home.key(KeyCode::Enter, 0),
            Some(Action::Open(MainDestination::Collection))
        );
        home.reset();
        home.key(KeyCode::ArrowDown, 0);
        home.key(KeyCode::ArrowDown, 0);
        home.key(KeyCode::ArrowDown, 0);
        assert_eq!(
            home.key(KeyCode::Enter, 0),
            Some(Action::Open(MainDestination::Settings { tab: 0 }))
        );
    }

    #[test]
    fn the_keyboard_and_pointer_reach_the_servers() {
        let mut home = Home::default();
        // No servers: Right stays on the arc.
        home.key(KeyCode::ArrowRight, 0);
        assert_eq!(home.focus, Focus::Arc);
        home.key(KeyCode::ArrowRight, 3);
        assert_eq!(home.focus, Focus::Server(0));
        home.key(KeyCode::ArrowUp, 3);
        assert_eq!(home.key(KeyCode::Enter, 3), Some(Action::Join(2)));
        home.key(KeyCode::Escape, 3);
        assert_eq!(home.focus, Focus::Arc);
        assert_eq!(
            home.page,
            Page::Main,
            "Escape left the column, not the page"
        );
        // Hovering a server chooses it, a click joins it.
        assert_eq!(home.pointer(SERVER_TOKEN + 1, false, 3), None);
        assert_eq!(home.focus, Focus::Server(1));
        assert_eq!(
            home.pointer(SERVER_TOKEN + 1, true, 3),
            Some(Action::Join(1))
        );
        assert_eq!(home.pointer(SERVER_TOKEN + 3, true, 3), None);
        // Hovering an entry chooses it; a click on SJK opens its page.
        home.pointer(ENTRY_TOKEN + 4, false, 3);
        assert_eq!((home.entry, home.focus), (4, Focus::Arc));
        assert_eq!(home.pointer(ENTRY_TOKEN + 4, true, 3), None);
        assert_eq!(home.page, Page::Sjk);
        assert_eq!(
            home.pointer(ENTRY_TOKEN + 2, true, 3),
            Some(Action::Open(MainDestination::Credits))
        );
    }

    fn server(name: &'static str) -> ServerItem<'static> {
        ServerItem {
            name,
            map: "mp/ffa3",
            live: Some((5, 16, 40)),
            played: Some(Ago::Minutes(5)),
        }
    }

    const LINES: [DockLine<'static>; 2] = [
        DockLine {
            name: "^2Sol",
            text: "gg all",
            verified: true,
            staff: true,
            key_id: "0123456789abcdef",
            tier: None,
            gif: None,
        },
        DockLine {
            name: "Fox",
            text: "a long message that will not fit on one row of the dock at all, cut",
            verified: false,
            staff: false,
            key_id: "fedcba9876543210",
            tier: None,
            gif: None,
        },
    ];

    fn view<'a>(servers: &'a [ServerItem<'a>], chat: Option<ChatDock<'a>>) -> HomeView<'a> {
        HomeView {
            name: "Sol",
            model: "kyle",
            blade_name: "blue",
            servers,
            version: "2026.1008.1",
            update: Some("2026.1009.1"),
            seconds: 1.0,
            chat,
            summary: &crate::profile_card::NOBODY,
            donation: None,
        }
    }

    // The donation goal's lowest line stays above the ring's top.
    const _: () = assert!(DONATION[1] + 48.0 + 30.0 + 18.0 + 20.0 < RING[1] - RING_RADIUS - 40.0);

    #[test]
    fn the_donation_goal_clears_the_ring_and_its_button_donates() {
        let mut home = Home::default();
        let servers = [server("JoF")];
        let mut with = view(&servers, None);
        with.donation = Some(crate::donation_goal::Card {
            numbers: Some(crate::donation_goal::Numbers {
                raised_cents: 8_000,
                goal_cents: 20_000,
                monthly: true,
                euros: true,
            }),
        });
        let mut canvas = MenuCanvas::new();
        build(&mut canvas, [1920.0, 1080.0], &mut home, &with, 1.0);
        assert!(canvas.rect_for(DONATE_TOKEN).is_some());
        assert_eq!(home.pointer(DONATE_TOKEN, false, 1), None);
        assert_eq!(home.pointer(DONATE_TOKEN, true, 1), Some(Action::Donate));
        let mut canvas = MenuCanvas::new();
        build(
            &mut canvas,
            [1920.0, 1080.0],
            &mut home,
            &view(&servers, None),
            1.0,
        );
        assert!(canvas.rect_for(DONATE_TOKEN).is_none());
    }

    #[test]
    fn the_keys_and_the_pointer_reach_the_profile_card() {
        let mut home = Home::default();
        home.key(KeyCode::ArrowLeft, 1);
        assert_eq!(home.focus, Focus::Card);
        assert_eq!(
            home.key(KeyCode::Enter, 1),
            Some(Action::Open(MainDestination::Profile))
        );
        // Escape on the card goes back to the arc; it does not ask to quit.
        home.key(KeyCode::Escape, 1);
        assert_eq!((home.focus, home.page), (Focus::Arc, Page::Main));
        home.key(KeyCode::ArrowLeft, 1);
        home.key(KeyCode::ArrowRight, 1);
        assert_eq!(home.focus, Focus::Arc);
        // Hovering chooses it, a click opens the profile.
        assert_eq!(home.pointer(CARD_TOKEN, false, 1), None);
        assert_eq!(home.focus, Focus::Card);
        assert_eq!(
            home.pointer(CARD_TOKEN, true, 1),
            Some(Action::Open(MainDestination::Profile))
        );
    }

    /// The card answers the pointer in the bottom-left corner of every window, and no
    /// other text of the page reaches into it.
    #[test]
    fn the_profile_card_has_its_corner_to_itself() {
        let servers = [server("JoF")];
        for viewport in VIEWPORTS {
            for focus in [Focus::Arc, Focus::Card, Focus::Server(0), Focus::Chat] {
                let mut canvas = MenuCanvas::new();
                let mut home = Home {
                    focus,
                    ..Home::default()
                };
                build(
                    &mut canvas,
                    viewport,
                    &mut home,
                    &view(&servers, Some(dock())),
                    1.0,
                );
                let area = canvas.rect_for(CARD_TOKEN).expect("the card");
                assert!(
                    area.x >= 0.0 && area.bottom() <= viewport[1],
                    "{viewport:?}"
                );
                assert!(area.right() < viewport[0] * 0.5, "{viewport:?}");
                let inside = |rect: Rect| {
                    rect.x >= area.x - 0.5
                        && rect.right() <= area.right() + 0.5
                        && rect.y >= area.y - 0.5
                        && rect.bottom() <= area.bottom() + 0.5
                };
                let overlaps = |rect: Rect| {
                    rect.x < area.right()
                        && rect.right() > area.x
                        && rect.y < area.bottom()
                        && rect.bottom() > area.y
                };
                for command in canvas.draw_list().commands() {
                    if let DrawCommand::Text { rect, .. } = command {
                        assert!(
                            inside(*rect) || !overlaps(*rect),
                            "{viewport:?} {focus:?}: {rect:?} reaches into the card"
                        );
                    }
                }
            }
        }
    }

    fn dock() -> ChatDock<'static> {
        ChatDock {
            lines: &LINES,
            online: 7,
            live: true,
            notice: "",
            measure: None,
        }
    }

    #[test]
    fn the_dock_fits_under_the_servers_in_every_window() {
        let servers = [server("a"), server("b"), server("c"), server("d")];
        for viewport in VIEWPORTS {
            let mut canvas = MenuCanvas::new();
            let mut home = Home::default();
            build(
                &mut canvas,
                viewport,
                &mut home,
                &view(&servers, Some(dock())),
                1.0,
            );
            let frame = Frame::new(viewport);
            let field = canvas.rect_for(CHAT_TOKEN).expect("the dock's field");
            let open = canvas.rect_for(OPEN_CHAT_TOKEN).expect("Open chat");
            let last = canvas.rect_for(SERVER_TOKEN + 3).unwrap();
            let version_top = frame.point(0.0, 962.0)[1];
            for rect in [field, open] {
                assert!(rect.y > last.bottom(), "{viewport:?}: under the servers");
                assert!(
                    rect.bottom() < version_top,
                    "{viewport:?}: above the version"
                );
                assert!(rect.x >= 0.0 && rect.right() <= viewport[0], "{viewport:?}");
            }
        }
        // Without the chat there is no dock.
        let mut canvas = MenuCanvas::new();
        build(
            &mut canvas,
            VIEWPORTS[0],
            &mut Home::default(),
            &view(&servers, None),
            1.0,
        );
        assert!(canvas.rect_for(CHAT_TOKEN).is_none());
    }

    #[test]
    fn the_dock_shows_the_last_lines_and_who_is_online() {
        let servers = [server("a")];
        let mut canvas = MenuCanvas::new();
        let mut home = Home::default();
        build(
            &mut canvas,
            VIEWPORTS[0],
            &mut home,
            &view(&servers, Some(dock())),
            1.0,
        );
        let runs: Vec<&str> = canvas.text_runs().collect();
        assert!(runs.contains(&"SJK chat"), "{runs:?}");
        assert!(runs.contains(&"7 online"), "{runs:?}");
        assert!(runs.contains(&"^2Sol"), "{runs:?}");
        assert!(runs.contains(&"gg all"), "{runs:?}");
        assert!(runs.contains(&"Open chat"), "{runs:?}");
    }

    #[test]
    fn resting_on_a_dock_name_shows_the_sender_card_with_mute() {
        use sjk_ui::{InputEvent, Vec2};
        let servers = [server("a")];
        let mut canvas = MenuCanvas::new();
        let mut home = Home {
            dock: true,
            ..Home::default()
        };
        let draw = |canvas: &mut MenuCanvas, home: &mut Home| {
            build(
                canvas,
                VIEWPORTS[0],
                home,
                &view(&servers, Some(dock())),
                1.0,
            );
        };
        let centre = |rect: Rect| Vec2::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5);
        draw(&mut canvas, &mut home);
        assert!(home.chat.sender_card.is_none());
        let name = canvas.rect_for(SENDER_NAME_TOKEN).expect("Sol's name");
        canvas.pointer(InputEvent::PointerMove(centre(name)));
        draw(&mut canvas, &mut home);
        let runs: Vec<&str> = canvas.text_runs().collect();
        for wanted in ["SJK player, SJK staff", "Key 0123456789abcdef", "Mute"] {
            assert!(runs.contains(&wanted), "{wanted:?} in {runs:?}");
        }
        let card = canvas.rect_for(SENDER_CARD_TOKEN).expect("the card");
        assert!(
            card.x >= name.right() || card.right() <= name.x,
            "beside the name: {card:?} {name:?}"
        );
        let [width, height] = VIEWPORTS[0];
        assert!(card.x >= 0.0 && card.right() <= width && card.bottom() <= height);
        // Onto the card: it stays; Mute mutes Sol, the dock's first line.
        let mute = canvas.rect_for(SENDER_MUTE_TOKEN).expect("Mute");
        canvas.pointer(InputEvent::PointerMove(centre(mute)));
        draw(&mut canvas, &mut home);
        assert!(home.chat.sender_card.is_some());
        // Away from both: it stays a moment for the way across, then goes.
        canvas.pointer(InputEvent::PointerMove(Vec2::new(4.0, 4.0)));
        draw(&mut canvas, &mut home);
        assert!(home.chat.sender_card.is_some());
        home.chat.expire_card_for_test();
        draw(&mut canvas, &mut home);
        assert!(home.chat.sender_card.is_none());
        canvas.pointer(InputEvent::PointerMove(centre(name)));
        draw(&mut canvas, &mut home);
        assert_eq!(home.pointer(SENDER_MUTE_TOKEN, true, 1), Some(Action::Mute));
        assert_eq!(
            home.take_mute(),
            Some(("0123456789abcdef".to_owned(), "^2Sol".to_owned()))
        );
        assert_eq!(home.pointer(SENDER_MUTE_TOKEN, true, 1), None, "once");
        assert_eq!(home.take_mute(), None);
    }

    /// The dock's sender card and the player's own profile card, bottom left, keep
    /// apart in every window, and hovering a dock name leaves the keyboard where it is.
    #[test]
    fn the_sender_card_keeps_clear_of_the_profile_card() {
        use sjk_ui::{InputEvent, Vec2};
        let servers = [server("JoF")];
        let overlaps = |a: Rect, b: Rect| {
            a.x < b.right() && a.right() > b.x && a.y < b.bottom() && a.bottom() > b.y
        };
        for viewport in VIEWPORTS {
            for row in 0..dock().lines.len() {
                let mut canvas = MenuCanvas::new();
                let mut home = Home::default();
                let draw = |canvas: &mut MenuCanvas, home: &mut Home| {
                    build(canvas, viewport, home, &view(&servers, Some(dock())), 1.0);
                };
                draw(&mut canvas, &mut home);
                let name = canvas
                    .rect_for(SENDER_NAME_TOKEN + row as u16)
                    .expect("a dock name");
                let centre = Vec2::new(name.x + name.width * 0.5, name.y + name.height * 0.5);
                canvas.pointer(InputEvent::PointerMove(centre));
                draw(&mut canvas, &mut home);
                assert_eq!(
                    home.focus,
                    Focus::Arc,
                    "{viewport:?}: hovering moves no focus"
                );
                let sender = canvas.rect_for(SENDER_CARD_TOKEN).expect("the sender card");
                let profile = canvas.rect_for(CARD_TOKEN).expect("the profile card");
                assert!(
                    !overlaps(sender, profile),
                    "{viewport:?} row {row}: {sender:?} over {profile:?}"
                );
                // Left of the dock and its field, above the keys, over no text drawn
                // before it (text shows through shapes).
                let frame = Frame::new(viewport);
                let [dock_left, keys_top] = frame.point(DOCK.x - 14.0, KEYS_Y - 24.0);
                assert!(
                    sender.right() <= dock_left && sender.bottom() <= keys_top,
                    "{viewport:?} row {row}: {sender:?} over the dock or the keys"
                );
                let commands = canvas.draw_list().commands();
                let card_at = commands
                    .iter()
                    .position(|command| {
                        matches!(command, DrawCommand::RoundedRect { rect, .. } if *rect == sender)
                    })
                    .expect("the card's shape");
                for command in &commands[..card_at] {
                    if let DrawCommand::Text {
                        rect,
                        text,
                        size,
                        align,
                        ..
                    } = command
                    {
                        // Its ink, at a generous width a character, in its box.
                        let text = canvas.stored_text(*text);
                        let ink = (text.chars().count() as f32 * size * 0.62).min(rect.width);
                        let x = match align {
                            TextAlign::Start => rect.x,
                            TextAlign::Center => rect.x + (rect.width - ink) * 0.5,
                            TextAlign::End => rect.right() - ink,
                        };
                        let ink = Rect::new(x, rect.y, ink, rect.height);
                        assert!(
                            !overlaps(ink, sender),
                            "{viewport:?} row {row}: {text:?} {ink:?} under the card {sender:?}"
                        );
                    }
                }
                assert!(
                    sender.x >= 0.0
                        && sender.y >= 0.0
                        && sender.right() <= viewport[0] + 0.5
                        && sender.bottom() <= viewport[1] + 0.5,
                    "{viewport:?}: {sender:?}"
                );
            }
        }
    }

    #[test]
    fn the_sender_cards_picture_is_asked_until_it_is_known() {
        use sjk_ui::{InputEvent, Vec2};
        let servers = [server("a")];
        let mut canvas = MenuCanvas::new();
        let mut home = Home::default();
        let draw = |canvas: &mut MenuCanvas, home: &mut Home| {
            build(
                canvas,
                VIEWPORTS[0],
                home,
                &view(&servers, Some(dock())),
                1.0,
            );
        };
        draw(&mut canvas, &mut home);
        // No card: nothing to ask.
        home.place_sender_card(|_| panic!("no card on show"));
        let name = canvas.rect_for(SENDER_NAME_TOKEN).expect("Sol's name");
        canvas.pointer(InputEvent::PointerMove(Vec2::new(
            name.x + name.width * 0.5,
            name.y + name.height * 0.5,
        )));
        draw(&mut canvas, &mut home);
        home.place_sender_card(|key| {
            assert_eq!(key, "0123456789abcdef");
            None
        });
        home.place_sender_card(|_| Some("00000000000000aa".to_owned()));
        home.place_sender_card(|_| panic!("known now"));
        let card = home.chat.sender_card.as_ref().expect("the card");
        assert_eq!(card.person.avatar.as_deref(), Some("00000000000000aa"));
    }

    #[test]
    fn a_dock_line_flows_with_the_tick_alone_and_its_text_in_gold() {
        let font = crate::text::test_font();
        let servers = [server("a")];
        let mut canvas = MenuCanvas::new();
        let chat = ChatDock {
            measure: Some(crate::sjk_chat_look::Measure::new(
                &font,
                crate::text::TextStyle::NEUTRAL,
            )),
            ..dock()
        };
        build(
            &mut canvas,
            VIEWPORTS[0],
            &mut Home::default(),
            &view(&servers, Some(chat)),
            1.0,
        );
        let texts: Vec<(String, Rect, sjk_ui::Color)> = canvas
            .draw_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text {
                    rect, text, color, ..
                } => Some((canvas.stored_text(*text).to_owned(), *rect, *color)),
                _ => None,
            })
            .collect();
        let find = |wanted: &str| {
            texts
                .iter()
                .find(|(text, ..)| text == wanted)
                .unwrap_or_else(|| panic!("{wanted:?} not in {texts:?}"))
        };
        let (_, name, name_colour) = find("^2Sol");
        let (_, body, body_colour) = find("gg all");
        assert!((name.y - body.y).abs() < 0.01 && body.x > name.right());
        assert_eq!(*body_colour, crate::sjk_chat_look::GOLD);
        assert_ne!(*name_colour, crate::sjk_chat_look::GOLD);
        let ticks: Vec<Rect> = canvas
            .draw_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::TexturedQuad { rect, texture, .. }
                    if *texture == crate::ui_renderer::VERIFIED_TEXTURE =>
                {
                    Some(*rect)
                }
                _ => None,
            })
            .collect();
        assert_eq!(ticks.len(), 1, "Sol is verified, Fox is not");
        assert!(ticks[0].x >= name.right() - 1.0 && ticks[0].right() <= body.x);
    }

    #[test]
    fn the_keyboard_reaches_the_dock_and_types() {
        let mut home = Home {
            dock: true,
            ..Home::default()
        };
        home.key(KeyCode::ArrowRight, 3);
        home.key(KeyCode::ArrowDown, 3);
        home.key(KeyCode::ArrowDown, 3);
        assert_eq!(home.focus, Focus::Server(2));
        home.key(KeyCode::ArrowDown, 3);
        assert_eq!(home.focus, Focus::Chat);
        home.key(KeyCode::ArrowUp, 3);
        assert_eq!(home.focus, Focus::Server(2));
        home.key(KeyCode::ArrowDown, 3);
        assert_eq!(home.key(KeyCode::Enter, 3), None);
        assert!(home.is_typing());
        // Without the dock, Down on the last server goes round to the first.
        let mut home = Home::default();
        home.key(KeyCode::ArrowRight, 3);
        for _ in 0..3 {
            home.key(KeyCode::ArrowDown, 3);
        }
        assert_eq!(home.focus, Focus::Server(0));
    }

    #[test]
    fn dock_typing_takes_letters_not_menu_keys() {
        let mut home = Home {
            dock: true,
            ..Home::default()
        };
        home.focus = Focus::Chat;
        home.key(KeyCode::Enter, 1);
        for (key, text) in [
            (KeyCode::KeyW, "w"),
            (KeyCode::KeyS, "s"),
            (KeyCode::Space, " "),
        ] {
            assert_eq!(home.typing_key(key, Some(text), false, false), None);
        }
        assert_eq!(home.typing_key(KeyCode::ArrowUp, None, false, false), None);
        assert_eq!(
            home.typing_key(KeyCode::Tab, Some("\t"), false, false),
            None
        );
        assert_eq!(home.focus, Focus::Chat);
        assert_eq!(home.chat.draft.as_deref(), Some("ws "));
        home.typing_key(KeyCode::Backspace, Some("\u{8}"), false, false);
        assert_eq!(home.chat.draft.as_deref(), Some("ws"));
        for _ in 0..200 {
            home.typing_key(KeyCode::KeyA, Some("a"), false, false);
        }
        assert_eq!(home.chat.draft.as_ref().unwrap().chars().count(), 150);
    }

    #[test]
    fn enter_sends_and_escape_cancels() {
        let mut home = Home {
            dock: true,
            ..Home::default()
        };
        home.focus = Focus::Chat;
        home.key(KeyCode::Enter, 1);
        home.typing_key(KeyCode::KeyG, Some("g"), false, false);
        home.typing_key(KeyCode::KeyG, Some("g"), false, false);
        assert_eq!(
            home.typing_key(KeyCode::Enter, Some("\r"), false, false),
            Some(Action::SendChat)
        );
        assert_eq!(home.take_draft(), "gg");
        assert!(!home.is_typing());
        home.key(KeyCode::Enter, 1);
        home.typing_key(KeyCode::KeyX, Some("x"), false, false);
        assert_eq!(home.typing_key(KeyCode::Escape, None, false, false), None);
        assert!(!home.is_typing());
        assert_eq!(home.focus, Focus::Chat);
        // Enter on nothing typed only stops typing.
        home.key(KeyCode::Enter, 1);
        assert_eq!(
            home.typing_key(KeyCode::Enter, Some("\r"), false, false),
            None
        );
        assert!(!home.is_typing());
    }

    #[test]
    fn the_pointer_types_in_the_dock_and_opens_the_chat() {
        let mut home = Home {
            dock: true,
            ..Home::default()
        };
        assert_eq!(home.pointer(CHAT_TOKEN, false, 1), None);
        assert_eq!(home.focus, Focus::Chat);
        assert!(!home.is_typing());
        assert_eq!(home.pointer(CHAT_TOKEN, true, 1), None);
        assert!(home.is_typing());
        assert_eq!(
            home.pointer(OPEN_CHAT_TOKEN, true, 1),
            Some(Action::OpenChat)
        );
        assert!(!home.is_typing(), "the page takes the typing");
    }

    #[test]
    fn the_arc_eases_the_short_way_round() {
        let mut home = Home::default();
        assert_eq!(home.arc_towards(1.0, 10.0), 1.0);
        let next = home.arc_towards(2.0, 10.05);
        assert!(next > 1.0 && next < 2.0);
        home.arc = Some(3.0);
        assert!(home.arc_towards(-3.0, 10.1) > 3.0);
    }
}
