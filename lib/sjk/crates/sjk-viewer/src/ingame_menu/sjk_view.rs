//! The SJK UI's in-game menu (`docs/sjk-ui.md`, In-game menu): Escape in a
//! match keeps the match drawing and lays a dark fade over its left side, with
//! a compact version of the main page's arc of entries on it (the chosen one
//! gold and larger, a line under it saying what it opens, a lit rail beside
//! them with a gold mark at the chosen one, SJK's emblem inside the curve), a
//! card of the match on the right with the player's side and the vote on it, a
//! row of small icon buttons under the emblem, the SJK chat docked under the match
//! between them (the main page's dock, `menu::sjk::chat_dock`), the player's profile
//! card bottom left (a click opens the Profile screen) and the keys at the bottom.
//!
//! The pages are the in-game menu's own ([`Page`]): the main one, Team (the J
//! key's, or Siege's classes), Players (a small scoreboard, drawn as a table with
//! the chosen player's card on the right) and its Report page, the call-vote lists
//! and Leave. The main page's row and card controls are `sjk_focus.rs`'s model,
//! drawn by [`dock`]. Their rows
//! are the shared ones where they are the same (Siege's, the call-vote
//! lists') and this module's where they differ ([`prepare`]); what a row does
//! is in `sjk_actions.rs` and, for the shared pages, `game_menu_actions.rs`.
//!
//! Positions are pixels of the SJK UI's 16:9 frame ([`Frame`]).

use super::players::{self, Player};
use super::sjk_focus::{Control, Controls, Focus, Icon};
use super::{Page, View};
use crate::game_font::GameFonts;
use crate::menu::emblem::{self, EmblemLayer};
use crate::menu::sjk::chat_dock::{self, ChatDock, Dock};
use crate::menu::sjk::{
    Frame, TextTarget, color, fade, fade_across, key_hint, key_hint_width, text,
};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use crate::text::{TextVertex, UiFont};
use sjk_protocol::{GameState, InfoString};
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};
use std::fmt::Write as _;

#[path = "sjk_dock.rs"]
mod dock;
pub(crate) use dock::{CONTROL_TOKEN, icon_of};

/// An entry of the main page, in the arc's order. Team and Vote live on the match
/// card, Camera control and SJK's pages in the row under the emblem
/// ([`super::sjk_focus`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Entry {
    Resume,
    /// The Profile screen: the character and the SJK profile as tabs
    /// ([`crate::profile_hub`]).
    Profile,
    /// The Collection screen: medals, achievements, shaders, toys and nameplates.
    Collection,
    Players,
    Settings,
    Servers,
    Leave,
}

impl Entry {
    /// The main page, top to bottom.
    pub(crate) const MAIN: [Self; 7] = [
        Self::Resume,
        Self::Profile,
        Self::Collection,
        Self::Players,
        Self::Settings,
        Self::Servers,
        Self::Leave,
    ];

    /// Its row on the main page.
    pub(crate) const fn index(self) -> usize {
        self as usize
    }

    /// The entry at row `row` of the main page.
    pub(crate) fn at(row: usize) -> Option<Self> {
        Self::MAIN.get(row).copied()
    }

    fn label(self) -> &'static str {
        match self {
            Self::Resume => "Resume",
            Self::Profile => "Profile",
            Self::Collection => "Collection",
            Self::Players => "Players",
            Self::Settings => "Settings",
            Self::Servers => "Servers",
            Self::Leave => "Leave",
        }
    }

    /// The line under the entry when chosen.
    fn hint(self) -> &'static str {
        match self {
            Self::Resume => "Back to the match",
            Self::Profile => "Character, saber and Force; your SJK profile",
            Self::Collection => "Your medals, achievements, shaders and toys",
            Self::Players => "Everyone here; report a cheater or a troll",
            Self::Settings => "Every option and key, with search",
            Self::Servers => "Find another server; joining leaves this one",
            Self::Leave => "Leave the server, or quit",
        }
    }
}

/// Rows of the Leave page; Stay follows them.
pub(crate) mod leave {
    pub(crate) const SERVER: usize = 0;
    pub(crate) const QUIT: usize = 1;
    pub(crate) const STAY: usize = 2;
}

/// How many rows `page` has where the SJK UI has rows of its own; `None` for
/// the pages it shares (Siege, the call-vote lists).
pub(crate) fn row_count(page: Page, team_game: bool) -> Option<usize> {
    match page {
        Page::Main => Some(Entry::MAIN.len()),
        Page::Team if team_game => Some(5),
        Page::Team => Some(3),
        Page::Leave => Some(3),
        Page::About => Some(1),
        Page::ConfirmLeave | Page::ConfirmQuit => Some(2),
        _ => None,
    }
}

/// The page and row Escape (or a page's Back) returns to from `page`: the
/// entry that opened it (or, for a page the match card opened, the main page
/// with [`return_focus`] on the card). `None` closes the menu.
pub(crate) fn parent(page: Page) -> Option<(Page, usize)> {
    Some(match page {
        Page::Main | Page::Shot => return None,
        Page::Team | Page::Siege | Page::CallVote => (Page::Main, Entry::Resume.index()),
        Page::Players => (Page::Main, Entry::Players.index()),
        // The chosen player's row: `sjk_back` takes it from the roster.
        Page::ReportPlayer => (Page::Players, 0),
        // Pages of the classic look only.
        Page::Vote | Page::Sjk => (Page::Main, Entry::Resume.index()),
        Page::Leave => (Page::Main, Entry::Leave.index()),
        Page::About => (Page::Main, Entry::Resume.index()),
        Page::ConfirmLeave => (Page::Leave, leave::SERVER),
        Page::ConfirmQuit => (Page::Leave, leave::QUIT),
        list => (
            Page::CallVote,
            super::callvote::opening_row(list).unwrap_or(0),
        ),
    })
}

/// Where the keyboard returns on the main page from `page`: the card control that
/// opened it (the side's buttons for Team and Siege, Call a vote for the call-vote
/// page and its lists), else the list.
pub(crate) fn return_focus(page: Page) -> Focus {
    match page {
        Page::Team => Focus::Card(Control::Team(sjk_client::LegacyTeamChoice::Spectator)),
        Page::Siege => Focus::Card(Control::SiegeClass),
        page if page.is_vote_page() => Focus::Card(Control::CallVote),
        _ => Focus::List,
    }
}

/// Write `label` and `hint` into row `row`.
fn put(rows: &mut [String], hints: &mut [String], row: usize, label: &str, hint: &str) {
    rows[row].push_str(label);
    hints[row].push_str(hint);
}

/// "1 player", "4 players".
struct Players(usize);

impl std::fmt::Display for Players {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            1 => formatter.write_str("1 player"),
            count => write!(formatter, "{count} players"),
        }
    }
}

/// Fill `rows` and their `hints` for `view`'s page where the SJK UI has rows of
/// its own; `None` leaves the page to the shared rows.
pub(super) fn prepare(view: &View<'_>, rows: &mut [String], hints: &mut [String]) -> Option<usize> {
    match view.page {
        Page::Main => {
            for (row, entry) in Entry::MAIN.iter().enumerate() {
                put(rows, hints, row, entry.label(), entry.hint());
            }
        }
        Page::Team if view.team_game => {
            let (red, blue) = (view.red_players, view.blue_players);
            put(rows, hints, 0, "Auto-join", "");
            let _ = write!(
                hints[0],
                "The side with fewer players: {red} red, {blue} blue"
            );
            for (row, label, players, team) in [(1, "Red team", red, 1), (2, "Blue team", blue, 2)]
            {
                rows[row].push_str(label);
                if view.team == team {
                    let _ = write!(hints[row], "Your team, {}", Players(players));
                } else {
                    let _ = write!(hints[row], "{}", Players(players));
                }
            }
            put(
                rows,
                hints,
                3,
                "Spectate",
                if view.team == 3 {
                    "You are watching"
                } else {
                    "Watch the match"
                },
            );
            put(rows, hints, 4, "Back", "");
        }
        Page::Team => {
            let watching = view.team == 3;
            put(
                rows,
                hints,
                0,
                "Join the game",
                if watching {
                    "Play in this match"
                } else {
                    "You are playing"
                },
            );
            put(
                rows,
                hints,
                1,
                "Spectate",
                if watching {
                    "You are watching"
                } else {
                    "Watch the match"
                },
            );
            put(rows, hints, 2, "Back", "");
        }
        Page::Leave => {
            put(
                rows,
                hints,
                leave::SERVER,
                "Leave the server",
                "Back to the main menu",
            );
            put(
                rows,
                hints,
                leave::QUIT,
                "Quit to desktop",
                "Your settings are saved",
            );
            put(rows, hints, leave::STAY, "Stay", "Back to the game menu");
        }
        Page::About => put(
            rows,
            hints,
            0,
            "Back",
            "The card on the right shows the server",
        ),
        Page::ConfirmLeave | Page::ConfirmQuit => {
            put(rows, hints, 0, "Yes", "");
            put(rows, hints, 1, "No", "");
        }
        _ => return None,
    }
    row_count(view.page, view.team_game)
}

/// Rows of the shared pages written "label  /  detail" (Siege's team row, a
/// client number to kick) keep the label; the detail becomes the hint. A game
/// type's number is left out.
pub(super) fn split_hints(page: Page, rows: &mut [String], hints: &mut [String]) {
    for (row, hint) in rows.iter_mut().zip(hints.iter_mut()) {
        let Some(at) = row.find("  /  ") else {
            continue;
        };
        if page != Page::VoteGameType {
            hint.clear();
            hint.push_str(&row[at + 5..]);
        }
        row.truncate(at);
    }
}

/// The page's name over its first entry; none on the main page.
fn page_title(page: Page, siege: bool) -> Option<&'static str> {
    Some(match page {
        Page::Main | Page::Shot => return None,
        Page::Team if siege => "Class",
        Page::Team => "Team",
        Page::Siege => "Choose your class",
        Page::Vote => "Vote",
        Page::CallVote => "Call a vote",
        Page::VoteMap => "Change map",
        Page::VoteGameType => "Change game type",
        Page::VoteKick => "Kick a player",
        Page::VoteClientKick => "Kick by client number",
        Page::VoteWarmup => "Warmup",
        Page::VoteTimeLimit => "Time limit, minutes",
        Page::VoteFragLimit => "Frag limit",
        Page::Sjk => "SJK",
        Page::Leave => "Leave",
        Page::About => "Server info",
        Page::ConfirmLeave => "Leave the server?",
        Page::ConfirmQuit => "Quit to desktop?",
        Page::Players => "Players",
        Page::ReportPlayer => "Report a player",
    })
}

/// How an entry reads when not chosen, and when chosen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Tone {
    Normal,
    /// A way back (Back, Stay) or out (the main page's Leave): quiet.
    Back,
    /// Leaving the match itself: ember when chosen.
    Leaving,
}

fn tone(page: Page, row: usize, label: &str) -> Tone {
    match page {
        Page::Leave if row == leave::SERVER || row == leave::QUIT => Tone::Leaving,
        Page::ConfirmLeave | Page::ConfirmQuit if row == 0 => Tone::Leaving,
        Page::Main if row == Entry::Leave.index() => Tone::Back,
        _ if label == "Back" || label == "Stay" => Tone::Back,
        _ => Tone::Normal,
    }
}

/// The arc's circle, its centre off the frame's left edge, and the radius the
/// entries start on; the rail runs inside them.
const CENTRE: [f32; 2] = [-140.0, 540.0];
const TEXT_RADIUS: f32 = 580.0;
const RAIL_RADIUS: f32 = 548.0;
/// SJK's emblem inside the curve, its ring round it.
const EMBLEM: [f32; 2] = [196.0, 540.0];
const EMBLEM_SIZE: f32 = 150.0;
const EMBLEM_RING: f32 = 112.0;
/// An entry's pointer area: from a little before its start, this wide.
const ENTRY_REACH: f32 = 680.0;
/// The match card's column.
const CARD_X: f32 = 1360.0;
const CARD_WIDTH: f32 = 464.0;
const CARD_TOP: f32 = 218.0;
/// A line of medals on the Players page's card.
const MEDAL_PITCH: f32 = 44.0;
/// The keys' line.
const KEYS_Y: f32 = 1004.0;
/// Pointer token of the player's profile card, bottom left (`profile_card`).
pub(crate) const CARD_TOKEN: u16 = 900;
/// The docked SJK chat on the main page: in the match card's column, under its controls
/// (Sol's layout, 11/10/2026: the card moved up and the chat squeezed in under it, so
/// the middle of the screen shows the match), its last line near the frame's foot, right
/// of the keys; a sender's card shows on its left, over the clear match and above the
/// keys. Its pointer
/// tokens follow.
pub(crate) const CHAT_PLACE: chat_dock::Place = chat_dock::Place {
    x: CARD_X,
    top: CHAT_TOP,
    width: CARD_WIDTH,
    card_floor: KEYS_Y - 24.0,
};
/// The docked chat's top: under Call a vote, the card's last line.
const CHAT_TOP: f32 = 770.0;
pub(crate) const CHAT_TOKENS: chat_dock::Tokens = chat_dock::Tokens {
    field: 940,
    open: 941,
    card: 942,
    mute: 943,
    names: 944,
};

/// The arc's spacing and type for a page of some number of entries: the gap
/// between entries, their size and the chosen one's, and whether the chosen
/// one's line goes under it (else a row's detail follows its label).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Metrics {
    pitch: f32,
    size: f32,
    chosen: f32,
    hints: bool,
}

fn metrics(count: usize) -> Metrics {
    match count {
        0..=10 => Metrics {
            pitch: 68.0,
            size: 34.0,
            chosen: 48.0,
            hints: true,
        },
        11..=12 => Metrics {
            pitch: 50.0,
            size: 28.0,
            chosen: 38.0,
            hints: false,
        },
        _ => Metrics {
            pitch: 36.0,
            size: 21.0,
            chosen: 28.0,
            hints: false,
        },
    }
}

/// How far entry `index` of `count` sits below the arc's middle line.
fn entry_offset(index: usize, count: usize) -> f32 {
    (index as f32 - (count as f32 - 1.0) * 0.5) * metrics(count).pitch
}

/// The angle (radians, clockwise from the right) of a point `offset` below
/// the arc's middle line, on a circle of `radius`.
fn angle(offset: f32, radius: f32) -> f32 {
    (offset / radius).clamp(-1.0, 1.0).asin()
}

/// Where entry `index` of `count` starts, in frame pixels: on the arc, at its
/// vertical middle.
fn entry_point(index: usize, count: usize) -> [f32; 2] {
    let offset = entry_offset(index, count);
    let across = (TEXT_RADIUS * TEXT_RADIUS - offset * offset)
        .max(0.0)
        .sqrt();
    [CENTRE[0] + across, CENTRE[1] + offset]
}

/// The gold mark's motion along the rail: it eases towards the chosen entry.
#[derive(Debug, Default)]
pub(crate) struct Motion {
    /// Where the mark is (radians), `None` before the first frame.
    angle: Option<f32>,
    /// Menu time of the last frame.
    last: f64,
}

impl Motion {
    /// The mark's angle this frame, eased towards `target` since the last
    /// frame at menu time `seconds` (about 0.3 s, as the main page's arc).
    fn towards(&mut self, target: f32, seconds: f64) -> f32 {
        let elapsed = (seconds - self.last).clamp(0.0, 0.1) as f32;
        self.last = seconds;
        let current = self.angle.unwrap_or(target);
        let next = current + (target - current) * (1.0 - (-elapsed * 9.0).exp());
        self.angle = Some(next);
        next
    }
}

/// What the right side and the Players table read: the match card, and the roster
/// with the chosen player.
pub(super) struct Sides<'a> {
    pub(super) card: &'a Card,
    pub(super) players: &'a players::State,
}

/// The prepared rows of the page on show.
pub(super) struct Rows<'a> {
    pub(super) labels: &'a [String],
    pub(super) hints: &'a [String],
    pub(super) enabled: &'a [bool],
}

/// The main page beyond its list: the icons of the row under the emblem, the match
/// card's controls, the docked SJK chat while it is on and where the keyboard is
/// ([`super::sjk_focus`]).
pub(super) struct Extras<'a> {
    pub(super) focus: Focus,
    pub(super) icons: &'a [Icon],
    pub(super) controls: &'a Controls,
    pub(super) chat: Option<Chat<'a>>,
}

impl Extras<'_> {
    /// Nothing beyond the list (the other pages).
    pub(super) const NONE: Extras<'static> = Extras {
        focus: Focus::List,
        icons: &[],
        controls: &Controls::none(),
        chat: None,
    };
}

/// The docked SJK chat: what the menu keeps of it between frames and what it shows.
pub(super) struct Chat<'a> {
    pub(super) dock: &'a mut Dock,
    pub(super) view: ChatDock<'a>,
}

/// Draw `view`'s page and its right side (the match card, or a player's) into
/// `canvas`.
pub(super) fn build(
    canvas: &mut MenuCanvas,
    view: &View<'_>,
    rows: &Rows<'_>,
    sides: &Sides<'_>,
    extras: &mut Extras<'_>,
    motion: &mut Motion,
    viewport: [f32; 2],
) {
    let frame = Frame::new(viewport);
    let (card, roster) = (sides.card, sides.players);
    let table = view.page == Page::Players;
    // The player on show at the right: the one chosen in the table, or the one
    // being reported.
    let player = match view.page {
        Page::Players => roster.shown().get(view.selected_row),
        Page::ReportPlayer => roster.target(),
        _ => None,
    };
    canvas.begin_transparent(viewport);
    scrims(
        canvas,
        viewport,
        &frame,
        card.known || player.is_some(),
        table,
        extras.chat.is_some(),
    );
    let seconds = crate::menu::art::motion::seconds();
    if table {
        // The emblem stays; the table takes the arc's place.
        anchor(canvas, &frame, view, 0, motion, seconds);
        players_table(canvas, &frame, view, rows, roster);
    } else {
        anchor(canvas, &frame, view, rows.labels.len(), motion, seconds);
        entries(canvas, &frame, view, rows, extras.focus == Focus::List);
    }
    match player {
        Some(player) => {
            let blocked = roster.gate().reason().or_else(|| player.unreportable());
            let status = match (blocked, view.page) {
                (Some(reason), _) => reason,
                (None, Page::Players) => "Enter: report this player",
                (None, _) => "Choose why, then say what happened",
            };
            player_card(
                canvas,
                &frame,
                player,
                roster.team_game(),
                status,
                blocked.is_none(),
            );
        }
        None if card.known => {
            let main = view.page == Page::Main;
            draw_card(canvas, &frame, card, main);
            if main {
                dock::card_controls(canvas, &frame, card, view, extras);
            }
        }
        None => {}
    }
    if view.page == Page::Main {
        dock::icon_row(canvas, &frame, extras);
        if let Some(chat) = extras.chat.as_mut() {
            chat_dock::draw(
                canvas,
                &frame,
                CHAT_PLACE,
                chat.dock,
                &chat.view,
                extras.focus == Focus::Chat,
                CHAT_TOKENS,
            );
        }
    }
    if profile_card_room(view.page, if table { 0 } else { rows.labels.len() }) {
        let lit = canvas.token_hovered(CARD_TOKEN);
        crate::profile_card::with(|summary| {
            crate::profile_card::draw(
                canvas,
                &frame,
                &crate::profile_card::Card {
                    name: &summary.name,
                    detail: crate::profile_card::Detail::Line(&summary.detail),
                    summary,
                    lit,
                },
                CARD_TOKEN,
            );
        });
    }
    let chat = extras.chat.as_ref().map(|chat| chat.dock.is_typing());
    keys(canvas, &frame, view.page, extras.focus, chat);
    // The chat's sender card goes over everything, its targets last.
    if let Some(chat) = extras.chat.as_mut() {
        chat_dock::sender_card(
            canvas,
            &frame,
            CHAT_PLACE,
            chat.dock,
            &chat.view,
            CHAT_TOKENS,
        );
    }
    canvas.finish(dock::focus_token(view.selected_row, extras));
}

/// Whether the player's profile card has its corner on `page` with `count` entries on
/// the arc: not on the Players pages, whose table and player card take the screen, nor
/// when the arc's last entry (and the line under it) reaches down to it.
fn profile_card_room(page: Page, count: usize) -> bool {
    if matches!(page, Page::Players | Page::ReportPlayer) {
        return false;
    }
    if count == 0 {
        return true;
    }
    let m = metrics(count);
    let [_, last] = entry_point(count - 1, count);
    let under = if m.hints { 24.0 } else { 0.0 };
    last + m.pitch * 0.5 + under <= crate::profile_card::AREA[1] - 8.0
}

/// The fades that keep the menu readable over any part of the match: deep at
/// the left edge and clear by the middle, behind the card on the right, a soft pool
/// under the docked chat (`chat`), and along the bottom behind the keys.
fn scrims(
    canvas: &mut MenuCanvas,
    viewport: [f32; 2],
    frame: &Frame,
    card: bool,
    table: bool,
    chat: bool,
) {
    let [width, height] = viewport;
    let space = |alpha| color::alpha(color::SPACE, alpha);
    let x = |frame_x: f32| frame.point(frame_x, 0.0)[0];
    let mut stops = |points: &[(f32, f32)]| {
        for pair in points.windows(2) {
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
    };
    if table {
        // The Players table reaches the card: dark all the way across.
        stops(&[(0.0, 0.9), (x(1300.0), 0.74), (width, 0.8)]);
    } else {
        stops(&[(0.0, 0.9), (x(560.0), 0.78), (x(980.0), 0.0)]);
    }
    if card && !table {
        // Dark enough from the card's left edge (x 1360) for its small lines.
        stops(&[(x(1060.0), 0.0), (x(1340.0), 0.62), (width, 0.8)]);
    }
    if chat {
        // Rounded layers, each a little inside the last, so the pool has no edge.
        let place = CHAT_PLACE;
        let [x, y] = [place.x - 80.0, place.top - 56.0];
        let [w, h] = [place.width + 160.0, place.bottom() - place.top + 100.0];
        for layer in 0..5 {
            let inset = layer as f32 * 16.0;
            let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: frame.rect(x + inset, y + inset, w - inset * 2.0, h - inset * 2.0),
                radius: (90.0 - inset) * frame.s,
                color: space(0.14),
            });
        }
    }
    fade(
        canvas,
        Rect::new(0.0, height * 0.8, width, height * 0.2),
        space(0.0),
        space(0.72),
    );
}

/// SJK's emblem in its ring inside the curve, and the lit rail along the
/// entries with its gold mark easing to the chosen one.
fn anchor(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    view: &View<'_>,
    count: usize,
    motion: &mut Motion,
    seconds: f64,
) {
    let s = frame.s;
    let centre = frame.point(EMBLEM[0], EMBLEM[1]);
    let turn = (seconds * std::f64::consts::TAU / 240.0) as f32;
    emblem::rays(
        canvas,
        EmblemLayer::Ring,
        centre,
        EMBLEM_RING * s,
        turn,
        color::alpha(color::HOLO, 0.55),
    );
    let half = EMBLEM_SIZE * 0.5 * s;
    emblem::draw(
        canvas,
        Rect::new(centre[0] - half, centre[1] - half, half * 2.0, half * 2.0),
        seconds,
    );
    if count == 0 {
        return;
    }
    let pitch = metrics(count).pitch;
    let arc_centre = frame.point(CENTRE[0], CENTRE[1]);
    let first = angle(entry_offset(0, count) - pitch * 0.8, RAIL_RADIUS);
    let last = angle(entry_offset(count - 1, count) + pitch * 0.8, RAIL_RADIUS);
    // The rail: a holo arc, its ends fading out in steps.
    let line = |canvas: &mut MenuCanvas, start: f32, end: f32, alpha: f32, width: f32| {
        let _ = canvas.draw_list_mut().push(DrawCommand::Arc {
            center: arc_centre,
            radius: RAIL_RADIUS * s,
            width: width * s,
            start,
            sweep: end - start,
            color: color::alpha(color::HOLO, alpha),
            knockout: None,
        });
    };
    let span = last - first;
    let steps = 4;
    let end = span * 0.12;
    for step in 0..steps {
        let t = step as f32 / steps as f32;
        let alpha = 0.5 * (t + 0.5 / steps as f32);
        let piece = end / steps as f32;
        line(
            canvas,
            first + piece * step as f32,
            first + piece * (step + 1) as f32,
            alpha,
            2.0,
        );
        line(
            canvas,
            last - piece * (step + 1) as f32,
            last - piece * step as f32,
            alpha,
            2.0,
        );
    }
    line(canvas, first + end, last - end, 0.5, 2.0);
    // The gold mark beside the chosen entry.
    let chosen = view.selected_row.min(count - 1);
    let target = angle(entry_offset(chosen, count), RAIL_RADIUS);
    let mark = motion.towards(target, seconds);
    let sweep = pitch * 0.62 / RAIL_RADIUS;
    let _ = canvas.draw_list_mut().push(DrawCommand::Arc {
        center: arc_centre,
        radius: RAIL_RADIUS * s,
        width: 5.0 * s,
        start: mark - sweep * 0.5,
        sweep,
        color: color::GOLD_BRIGHT,
        knockout: None,
    });
}

/// A team's colour, for the team rows' marks and the card's team scores.
fn team_colour(team: u8) -> Color {
    if team == 1 {
        Color::new(1.0, 0.36, 0.33, 1.0)
    } else {
        Color::new(0.36, 0.6, 1.0, 1.0)
    }
}

/// The page's entries on the arc, its name over the first; the chosen one gold while
/// the list has the keyboard (`lit`), else only larger.
fn entries(canvas: &mut MenuCanvas, frame: &Frame, view: &View<'_>, rows: &Rows<'_>, lit: bool) {
    let s = frame.s;
    let count = rows.labels.len();
    if count == 0 {
        return;
    }
    let m = metrics(count);
    if let Some(title) = page_title(view.page, view.siege) {
        let offset = entry_offset(0, count) - m.pitch.max(56.0);
        let x = CENTRE[0]
            + (TEXT_RADIUS * TEXT_RADIUS - offset * offset)
                .max(0.0)
                .sqrt();
        text(
            canvas,
            TextFamily::Display,
            format_args!("{title}"),
            frame.rect(x, CENTRE[1] + offset - 15.0, 600.0, 30.0),
            24.0 * s,
            color::alpha(color::HOLO, 0.85),
            FontWeight::Semibold,
            TextAlign::Start,
        );
    }
    for (row, label) in rows.labels.iter().enumerate() {
        let chosen = row == view.selected_row;
        let enabled = rows.enabled.get(row).copied().unwrap_or(true);
        let hint = rows.hints.get(row).map_or("", String::as_str);
        let colour = match (chosen, enabled, tone(view.page, row, label)) {
            (true, true, _) if !lit => color::TEXT,
            (true, true, Tone::Leaving) => color::EMBER,
            (true, true, _) => color::GOLD_BRIGHT,
            (true, false, _) => color::MUTED,
            (false, false, _) => color::alpha(color::QUIET, 0.6),
            (false, true, Tone::Back) => color::QUIET,
            (false, true, _) => color::MUTED,
        };
        let size = if chosen { m.chosen } else { m.size };
        let [x, y] = entry_point(row, count);
        let rect = frame.rect(x, y - size * 0.6, 640.0, size * 1.2);
        if m.hints || hint.is_empty() {
            text(
                canvas,
                TextFamily::Display,
                format_args!("{label}"),
                rect,
                size * s,
                colour,
                FontWeight::Regular,
                TextAlign::Start,
            );
        } else {
            // A compact page has no room under the chosen entry: the row's
            // detail (a client number, Siege's other team) follows its label.
            text(
                canvas,
                TextFamily::Display,
                format_args!("{label}, {hint}"),
                rect,
                size * s,
                colour,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        if chosen && lit && m.hints && !hint.is_empty() {
            text(
                canvas,
                TextFamily::Body,
                format_args!("{hint}"),
                frame.rect(x + 2.0, y + 20.0, 600.0, 24.0),
                17.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        let team = match (view.page, view.team_game, row) {
            (Page::Team, true, 1) => Some(1),
            (Page::Team, true, 2) => Some(2),
            _ => None,
        };
        if let Some(team) = team {
            let height = size * 0.62;
            let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: frame.rect(x - 14.0, y - height * 0.5, 5.0, height),
                radius: 2.5 * s,
                color: team_colour(team),
            });
        }
        canvas.hit_region(
            row as u16,
            frame.rect(x - 24.0, y - m.pitch * 0.5, ENTRY_REACH, m.pitch),
        );
    }
}

/// The keys of the page, bottom centre: on the main page those of what has the
/// keyboard (the list, the row, the card or the chat). `chat` says whether the chat is
/// docked and typed in.
fn keys(canvas: &mut MenuCanvas, frame: &Frame, page: Page, focus: Focus, chat: Option<bool>) {
    let typing = chat == Some(true);
    let (list_tab, card_tab) = if chat.is_some() {
        ("icons, match, chat", "chat")
    } else {
        ("icons and match", "list")
    };
    let s = frame.s;
    let back = if page == Page::Main { "resume" } else { "back" };
    let enter = match page {
        Page::Players => "report",
        Page::ReportPlayer => "write",
        _ => "open",
    };
    let rows: [(&[&str], &str); 4] = match (page, focus) {
        (Page::Main, _) if typing => [
            (&["Enter"], "send"),
            (&["Esc"], "stop typing"),
            (&["Backspace"], "erase"),
            (&[], ""),
        ],
        (Page::Main, Focus::Chat) => [
            (&["Enter"], "type"),
            (&["Tab"], "list"),
            (&["Esc"], "list"),
            (&[], ""),
        ],
        (Page::Main, Focus::List) => [
            (&["Up", "Down"], "choose"),
            (&["Tab"], list_tab),
            (&["Enter"], enter),
            (&["Esc"], back),
        ],
        (Page::Main, Focus::Row(_)) => [
            (&["Left", "Right"], "choose"),
            (&["Tab"], "match"),
            (&["Enter"], "open"),
            (&["Esc"], "list"),
        ],
        (Page::Main, Focus::Card(control)) => [
            (&["Arrows"], "choose"),
            (&["Tab"], card_tab),
            (
                &["Enter"],
                match control {
                    Control::VoteYes | Control::VoteNo => "vote",
                    Control::Team(sjk_client::LegacyTeamChoice::Spectator) => "spectate",
                    Control::Team(_) => "join",
                    Control::SiegeClass | Control::CallVote => "open",
                },
            ),
            (&["Esc"], "list"),
        ],
        _ => [
            (&["Up", "Down"], "choose"),
            (&["Enter"], enter),
            (&["Esc"], back),
            (&[], ""),
        ],
    };
    let rows = rows.iter().filter(|(caps, _)| !caps.is_empty());
    let gap = 28.0 * s;
    let total: f32 = rows
        .clone()
        .map(|(caps, action)| key_hint_width(caps, action, s) + gap)
        .sum::<f32>()
        - gap;
    let [centre, y] = frame.point(960.0, KEYS_Y);
    let mut x = centre - total * 0.5;
    for (caps, action) in rows {
        x = key_hint(canvas, caps, action, x, y, s) + gap;
    }
}

/// The Players table's columns, in frame pixels: its left edge and width, the
/// name's start, the score's and ping's right ends, the hub column's start.
const TABLE_X: f32 = 340.0;
const TABLE_WIDTH: f32 = 990.0;
const TABLE_NAME: f32 = 30.0;
const TABLE_SCORE: f32 = 700.0;
const TABLE_PING: f32 = 800.0;
const TABLE_HUB: f32 = 840.0;
/// The table's title, its header line and first row, and the rows' pitch.
const TABLE_TITLE_Y: f32 = 164.0;
const TABLE_HEAD_Y: f32 = 232.0;
const TABLE_TOP: f32 = 262.0;
const TABLE_PITCH: f32 = 37.0;

/// The Players page: a small scoreboard of everyone on the server (name, score,
/// ping, what the SJK hub knows), More players... and Back under them, the chosen
/// row lit gold.
fn players_table(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    view: &View<'_>,
    rows: &Rows<'_>,
    roster: &players::State,
) {
    let s = frame.s;
    text(
        canvas,
        TextFamily::Display,
        format_args!("Players"),
        frame.rect(TABLE_X, TABLE_TITLE_Y, 600.0, 34.0),
        30.0 * s,
        color::alpha(color::HOLO, 0.9),
        FontWeight::Semibold,
        TextAlign::Start,
    );
    let (line, colour) = match roster.gate().reason() {
        Some(reason) => (reason, color::EMBER),
        None => (
            "Choose a player to report them to the SJK team",
            color::MUTED,
        ),
    };
    text(
        canvas,
        TextFamily::Body,
        format_args!("{} on the server.  {line}", Players(roster.count())),
        frame.rect(TABLE_X, TABLE_TITLE_Y + 38.0, TABLE_WIDTH, 24.0),
        17.0 * s,
        colour,
        FontWeight::Regular,
        TextAlign::Start,
    );
    let head = |canvas: &mut MenuCanvas, x: f32, width: f32, label: &str, align| {
        text(
            canvas,
            TextFamily::Body,
            format_args!("{label}"),
            frame.rect(TABLE_X + x, TABLE_HEAD_Y, width, 22.0),
            14.0 * s,
            color::QUIET,
            FontWeight::Semibold,
            align,
        );
    };
    head(canvas, TABLE_NAME, 400.0, "NAME", TextAlign::Start);
    head(canvas, TABLE_SCORE - 100.0, 100.0, "SCORE", TextAlign::End);
    head(canvas, TABLE_PING - 90.0, 90.0, "PING", TextAlign::End);
    head(canvas, TABLE_HUB, 150.0, "SJK HUB", TextAlign::Start);
    fade_across(
        canvas,
        frame.rect(TABLE_X, TABLE_TOP - 6.0, TABLE_WIDTH, 1.0),
        color::alpha(color::HOLO, 0.4),
        color::alpha(color::HOLO, 0.05),
    );
    let shown = roster.shown();
    for (row, label) in rows.labels.iter().enumerate() {
        let y = TABLE_TOP + row as f32 * TABLE_PITCH;
        let chosen = row == view.selected_row;
        let band = frame.rect(TABLE_X, y, TABLE_WIDTH, TABLE_PITCH - 3.0);
        if chosen {
            fade_across(
                canvas,
                band,
                color::alpha(color::GOLD, 0.2),
                color::alpha(color::GOLD, 0.03),
            );
            let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: frame.rect(TABLE_X, y + 4.0, 4.0, TABLE_PITCH - 11.0),
                radius: 2.0 * s,
                color: color::GOLD_BRIGHT,
            });
        }
        let cell = |x: f32, width: f32| frame.rect(TABLE_X + x, y + 2.0, width, TABLE_PITCH - 6.0);
        match shown.get(row) {
            Some(player) => {
                table_row(canvas, frame, roster.team_game(), player, chosen, y, &cell);
            }
            None => {
                let colour = match (chosen, label.as_str()) {
                    (true, _) => color::GOLD_BRIGHT,
                    (false, "Back") => color::QUIET,
                    (false, _) => color::MUTED,
                };
                text(
                    canvas,
                    TextFamily::Display,
                    format_args!("{label}"),
                    cell(TABLE_NAME, 500.0),
                    22.0 * s,
                    colour,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
            }
        }
        canvas.hit_region(row as u16, band);
    }
}

/// One player's row of the Players table.
fn table_row(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    team_game: bool,
    player: &Player,
    chosen: bool,
    y: f32,
    cell: &dyn Fn(f32, f32) -> Rect,
) {
    let s = frame.s;
    let watching = player.team == 3;
    if team_game && matches!(player.team, 1 | 2) {
        let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: frame.rect(TABLE_X + 14.0, y + 9.0, 4.0, TABLE_PITCH - 21.0),
            radius: 2.0 * s,
            color: team_colour(player.team),
        });
    }
    let name_colour = match (chosen, watching) {
        (true, _) => color::GOLD_BRIGHT,
        (false, true) => color::MUTED,
        (false, false) => color::TEXT,
    };
    text(
        canvas,
        TextFamily::Display,
        format_args!("{}", player.name),
        cell(TABLE_NAME, TABLE_SCORE - TABLE_NAME - 120.0),
        23.0 * s,
        name_colour,
        FontWeight::Regular,
        TextAlign::Start,
    );
    let number = |canvas: &mut MenuCanvas, x: f32, width: f32, value: std::fmt::Arguments<'_>| {
        text(
            canvas,
            TextFamily::Display,
            value,
            cell(x - width, width),
            22.0 * s,
            if chosen { color::TEXT } else { color::MUTED },
            FontWeight::Regular,
            TextAlign::End,
        );
    };
    match player.score {
        Some(score) if !watching => number(canvas, TABLE_SCORE, 100.0, format_args!("{score}")),
        _ => number(canvas, TABLE_SCORE, 100.0, format_args!("-")),
    }
    match (player.bot, player.ping) {
        (true, _) => number(canvas, TABLE_PING, 90.0, format_args!("bot")),
        (false, Some(ping)) => number(canvas, TABLE_PING, 90.0, format_args!("{ping}")),
        (false, None) => number(canvas, TABLE_PING, 90.0, format_args!("-")),
    }
    let (hub, colour) = match (&player.hub, player.you) {
        (Some(hub), true) if hub.verified => ("You, verified", color::GOLD_BRIGHT),
        (Some(_), true) => ("You, SJK", color::HOLO),
        (None, true) => ("You", color::QUIET),
        (Some(hub), false) if hub.verified => ("Verified", color::GOLD_BRIGHT),
        (Some(_), false) => ("SJK", color::HOLO),
        (None, false) => ("", color::QUIET),
    };
    if !hub.is_empty() {
        text(
            canvas,
            TextFamily::Body,
            format_args!("{hub}"),
            cell(TABLE_HUB, 150.0),
            16.0 * s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }
}

/// A player's card on the right, in the match card's place: their name, slot and
/// side, what the SJK hub knows of them, their score and ping, and whether they can
/// be reported (`status`, gold when `open`, else why not).
fn player_card(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    player: &Player,
    team_game: bool,
    status: &str,
    open: bool,
) {
    let s = frame.s;
    let rule = |canvas: &mut MenuCanvas, y: f32| {
        fade_across(
            canvas,
            frame.rect(CARD_X, y, CARD_WIDTH, 1.0),
            color::alpha(color::HOLO, 0.45),
            color::alpha(color::HOLO, 0.08),
        );
    };
    let body = |canvas: &mut MenuCanvas, y: f32, colour: Color, value: std::fmt::Arguments<'_>| {
        text(
            canvas,
            TextFamily::Body,
            value,
            frame.rect(CARD_X, y, CARD_WIDTH, 24.0),
            17.0 * s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
    };
    let mut y = CARD_TOP;
    // An SJK player's picture (or their initial) before their name.
    let name_x = match &player.hub {
        Some(hub) => {
            let radius = 34.0;
            crate::profile_card::avatar(
                canvas,
                frame.point(CARD_X + radius, CARD_TOP + radius + 4.0),
                radius * s,
                &crate::profile_card::Avatar {
                    key_id: &hub.key_id,
                    version: &hub.avatar,
                    name: &player.name,
                    verified: hub.verified,
                    preview: false,
                    lit: false,
                },
            );
            CARD_X + radius * 2.0 + 18.0
        }
        None => CARD_X,
    };
    let name_width = CARD_X + CARD_WIDTH - name_x;
    // The JoF emblem on the name's left ([`crate::jof_tag`]).
    let jof = if let Some(clan) = crate::jof_tag::clan(&player.name) {
        let side = crate::jof_tag::side(38.0);
        let emblem = frame.rect(name_x, y + 23.0 - side * 0.5, side, side);
        crate::jof_tag::draw(
            clan,
            emblem.x,
            emblem.y + emblem.height * 0.5,
            emblem.height,
            1.0,
            |command| {
                let _ = canvas.draw_list_mut().push(command);
            },
        );
        crate::jof_tag::room(clan, side)
    } else {
        0.0
    };
    text(
        canvas,
        TextFamily::Display,
        format_args!("{}", player.name),
        frame.rect(name_x + jof, y, name_width - jof, 46.0),
        38.0 * s,
        color::TEXT,
        FontWeight::Regular,
        TextAlign::Start,
    );
    y += 50.0;
    let side = player.side(team_game);
    let what = match (player.you, player.bot) {
        (true, _) => ", you",
        (_, true) => ", a bot",
        _ => "",
    };
    let slot_line = frame.rect(name_x, y, name_width, 24.0);
    if side.is_empty() {
        text(
            canvas,
            TextFamily::Body,
            format_args!("Slot {}{what}", player.slot),
            slot_line,
            17.0 * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::Start,
        );
    } else {
        text(
            canvas,
            TextFamily::Body,
            format_args!("Slot {}, {side}{what}", player.slot),
            slot_line,
            17.0 * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }
    y += 40.0;
    rule(canvas, y);
    y += 22.0;
    let (hub, colour) = match &player.hub {
        Some(hub) if hub.verified => ("Verified SJK player", color::GOLD_BRIGHT),
        Some(_) => ("SJK player", color::HOLO),
        None => ("Not known to the SJK hub", color::MUTED),
    };
    text(
        canvas,
        TextFamily::Display,
        format_args!("{hub}"),
        frame.rect(CARD_X, y, CARD_WIDTH, 34.0),
        28.0 * s,
        colour,
        FontWeight::Regular,
        TextAlign::Start,
    );
    y += 38.0;
    if let Some(hub) = &player.hub {
        // The player's own key stays off screen; "Known as" alone then.
        let key = crate::player_identity::printable_key_id(&hub.key_id).map_or_else(
            || "Known as".to_owned(),
            |key| format!("Key {key}, known as"),
        );
        body(canvas, y, color::QUIET, format_args!("{key} {}", hub.name));
    }
    y += 40.0;
    // The medals the SJK team gave them: medallions with their names, two a line.
    if let Some(hub) = player.hub.as_ref().filter(|hub| !hub.medals.is_empty()) {
        let column = CARD_WIDTH / 2.0;
        let mut lines = 0;
        for (index, (medal, count)) in hub.medals.iter().enumerate() {
            let x = CARD_X + (index % 2) as f32 * column;
            let row_y = y + (index / 2) as f32 * MEDAL_PITCH;
            let _ = canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: frame.rect(x, row_y, 36.0, 36.0),
                texture: medal.icon(),
                color: Color::new(1.0, 1.0, 1.0, 1.0),
            });
            text(
                canvas,
                TextFamily::Body,
                format_args!("{}", medal.label(count)),
                frame.rect(x + 46.0, row_y + 6.0, column - 52.0, 24.0),
                17.0 * s,
                color::TEXT,
                FontWeight::Regular,
                TextAlign::Start,
            );
            lines = index / 2 + 1;
        }
        y += lines as f32 * MEDAL_PITCH + 4.0;
    }
    let cells = [
        (
            "Score",
            player.score.filter(|_| player.team != 3),
            "Watching",
        ),
        ("Ping", player.ping.filter(|_| !player.bot), "-"),
    ];
    for (index, (caption, value, none)) in cells.into_iter().enumerate() {
        let x = CARD_X + index as f32 * CARD_WIDTH / 3.0;
        let rect = frame.rect(x, y, CARD_WIDTH / 3.0, 56.0);
        match value {
            Some(value) => text(
                canvas,
                TextFamily::Display,
                format_args!("{value}"),
                rect,
                50.0 * s,
                color::TEXT,
                FontWeight::Regular,
                TextAlign::Start,
            ),
            None => text(
                canvas,
                TextFamily::Display,
                format_args!("{none}"),
                rect,
                30.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            ),
        }
        text(
            canvas,
            TextFamily::Body,
            format_args!("{caption}"),
            frame.rect(x, y + 58.0, CARD_WIDTH / 3.0, 22.0),
            15.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }
    y += 104.0;
    rule(canvas, y);
    y += 22.0;
    if open {
        body(canvas, y, color::GOLD_BRIGHT, format_args!("{status}"));
    } else {
        let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: frame.rect(CARD_X, y + 4.0, 4.0, 16.0),
            radius: 2.0 * s,
            color: color::EMBER,
        });
        text(
            canvas,
            TextFamily::Body,
            format_args!("{status}"),
            frame.rect(CARD_X + 14.0, y, CARD_WIDTH - 14.0, 24.0),
            17.0 * s,
            color::TEXT,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }
}

/// What the player is in the match.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum You {
    /// Spectating, or not known.
    #[default]
    Watching,
    /// Playing, with `score` and `rank` (`persistant[PERS_RANK]`: 0 first,
    /// with `RANK_TIED_FLAG`).
    Playing { score: i32, rank: u32 },
}

/// The local player as the card reads them from the latest snapshot.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Local {
    pub(crate) team: u8,
    pub(crate) spectator: bool,
    /// `persistant[PERS_SCORE]`.
    pub(crate) score: i32,
    /// `persistant[PERS_RANK]`.
    pub(crate) rank: u32,
}

/// `RANK_TIED_FLAG` in `persistant[PERS_RANK]`.
const RANK_TIED: u32 = 0x4000;
/// `CS_LEVEL_START_TIME` and the first `CS_PLAYERS` config string.
const CS_LEVEL_START_TIME: usize = 21;
const CS_PLAYERS: usize = 1_131;

/// What the match card shows: the server, the map and mode with its limits,
/// the clock, the player's score and place (or the teams'), and who plays.
/// Kept between frames and refreshed from the live session ([`Self::refresh`]);
/// the server's info and the player counts are read again once a second.
#[derive(Debug, Default)]
pub(crate) struct Card {
    /// A match is on: the card shows.
    known: bool,
    hostname: String,
    address: String,
    /// The map without `mp/`.
    map: String,
    mode: &'static str,
    /// The limits in words ("30 frags, 20 minutes"), or "no limits".
    limits: String,
    team_game: bool,
    /// The time limit in milliseconds (0 for none).
    limit_ms: i32,
    /// Time left (`counting_down`) or played, in milliseconds.
    clock_ms: i32,
    counting_down: bool,
    you: You,
    team: u8,
    team_scores: [i32; 2],
    playing: usize,
    watching: usize,
    slots: i32,
    /// The vote on, with its counts; empty when none is.
    vote: String,
    vote_yes: i32,
    vote_no: i32,
    /// Server time of the last read of the server's info.
    read_at: Option<i32>,
    /// A made-up card the tests set stays as it is.
    #[cfg(test)]
    held: bool,
}

impl Card {
    /// Refresh from `game` at server time `time`: the `local` player, the
    /// teams' scores and the `address` the client joined.
    pub(crate) fn refresh(
        &mut self,
        game: &GameState,
        local: Local,
        time: i32,
        team_scores: [i32; 2],
        address: Option<&str>,
    ) {
        #[cfg(test)]
        if self.held {
            return;
        }
        let stale = self
            .read_at
            .is_none_or(|read| time < read || time - read >= 1_000);
        if !self.known || stale {
            self.read_at = Some(time);
            self.read_server(game, address);
        }
        self.known = true;
        self.team = local.team;
        self.team_scores = team_scores;
        self.you = if local.spectator {
            You::Watching
        } else {
            You::Playing {
                score: local.score,
                rank: local.rank,
            }
        };
        let start = game
            .config_string(CS_LEVEL_START_TIME)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .and_then(|text| text.trim().parse::<i32>().ok())
            .unwrap_or(0);
        let elapsed = time.saturating_sub(start).max(0);
        self.counting_down = self.limit_ms > 0 && elapsed <= self.limit_ms;
        self.clock_ms = if self.counting_down {
            self.limit_ms - elapsed
        } else {
            elapsed
        };
        let vote = sjk_client::legacy_team_vote(game, local.team, time)
            .filter(|vote| vote.active)
            .unwrap_or_else(|| sjk_client::legacy_global_vote(game, time));
        self.vote.clear();
        if vote.active {
            self.vote.push_str(vote.text);
            self.vote_yes = vote.yes;
            self.vote_no = vote.no;
        }
    }

    /// Whether a match is on: the card shows.
    pub(crate) fn is_known(&self) -> bool {
        self.known
    }

    /// No match is on (the menu opened without a server): the card hides.
    pub(crate) fn forget(&mut self) {
        #[cfg(test)]
        if self.held {
            return;
        }
        self.known = false;
        self.read_at = None;
    }

    /// Read the server's info and count its players.
    fn read_server(&mut self, game: &GameState, address: Option<&str>) {
        let info = game
            .config_string(0)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .and_then(|text| InfoString::parse(text).ok());
        let get = |key: &str| info.as_ref().and_then(|info| info.get(key)).unwrap_or("");
        let number = |key: &str| {
            info.as_ref()
                .and_then(|info| info.get_i32(key))
                .unwrap_or(0)
        };
        self.hostname.clear();
        self.hostname.push_str(get("sv_hostname"));
        self.address.clear();
        self.address.push_str(address.unwrap_or(""));
        let map = get("mapname");
        self.map.clear();
        self.map.push_str(map.strip_prefix("mp/").unwrap_or(map));
        let gametype = info.as_ref().and_then(|info| info.get_i32("g_gametype"));
        self.mode = crate::server_browser::gametype_name(gametype);
        self.team_game = gametype.is_some_and(|gametype| gametype >= 6);
        let minutes = number("timelimit").max(0);
        self.limit_ms = minutes.saturating_mul(60_000);
        // The one score limit the game type plays to, as the stock about
        // screen shows it (`ingame_about.menu`).
        let (key, words) = match gametype {
            Some(3 | 4) => ("duellimit", "duel wins"),
            Some(8 | 9) => ("capturelimit", "captures"),
            _ => ("fraglimit", "frags"),
        };
        let score = number(key);
        self.limits.clear();
        if score > 0 {
            let _ = write!(self.limits, "{score} {words}");
        }
        if minutes > 0 {
            if !self.limits.is_empty() {
                self.limits.push_str(", ");
            }
            let _ = write!(self.limits, "{minutes} minutes");
        }
        if self.limits.is_empty() {
            self.limits.push_str("no limits");
        }
        self.slots = number("sv_maxclients");
        self.playing = 0;
        self.watching = 0;
        for client in 0..32 {
            let Some(text) = game
                .config_string(CS_PLAYERS + client)
                .filter(|bytes| !bytes.is_empty())
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
            else {
                continue;
            };
            let team = InfoString::parse(text)
                .ok()
                .and_then(|info| info.get_i32("t"));
            if team == Some(3) {
                self.watching += 1;
            } else {
                self.playing += 1;
            }
        }
    }
}

/// "1st", "2nd", "3rd", "11th".
fn ordinal(place: u32) -> &'static str {
    match (place % 100, place % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    }
}

/// A clock of `millis` as minutes and seconds ("12:04").
struct Clock(i32);

impl std::fmt::Display for Clock {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let seconds = self.0.max(0) / 1_000;
        write!(formatter, "{}:{:02}", seconds / 60, seconds % 60)
    }
}

/// One of the card's numbers: what it is under it, a team's colour beside it.
enum Cell {
    Score(i32),
    Place { place: u32, tied: bool, of: usize },
    Team { team: u8, score: i32, yours: bool },
    Clock { millis: i32, left: bool },
}

/// The match card, on the right: the server and its address, the map with its
/// mode and limits, the numbers that matter (score, place or the teams', the
/// clock) and who plays. On the `main` page the side's line says whether the player
/// watches, and the controls follow ([`dock::card_controls`]).
fn draw_card(canvas: &mut MenuCanvas, frame: &Frame, card: &Card, main: bool) {
    let s = frame.s;
    let rule = |canvas: &mut MenuCanvas, y: f32| {
        fade_across(
            canvas,
            frame.rect(CARD_X, y, CARD_WIDTH, 1.0),
            color::alpha(color::HOLO, 0.45),
            color::alpha(color::HOLO, 0.08),
        );
    };
    let body = |canvas: &mut MenuCanvas, y: f32, colour: Color, value: std::fmt::Arguments<'_>| {
        text(
            canvas,
            TextFamily::Body,
            value,
            frame.rect(CARD_X, y, CARD_WIDTH, 24.0),
            17.0 * s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
    };
    let mut y = CARD_TOP;
    let name = if card.hostname.is_empty() {
        "This server"
    } else {
        card.hostname.as_str()
    };
    text(
        canvas,
        TextFamily::Display,
        format_args!("{name}"),
        frame.rect(CARD_X, y, CARD_WIDTH, 46.0),
        38.0 * s,
        color::TEXT,
        FontWeight::Regular,
        TextAlign::Start,
    );
    y += 50.0;
    if !card.address.is_empty() {
        body(canvas, y, color::QUIET, format_args!("{}", card.address));
    }
    y += 40.0;
    rule(canvas, y);
    y += 22.0;
    text(
        canvas,
        TextFamily::Display,
        format_args!("{}", card.map),
        frame.rect(CARD_X, y, CARD_WIDTH, 40.0),
        34.0 * s,
        color::TEXT,
        FontWeight::Regular,
        TextAlign::Start,
    );
    y += 44.0;
    body(
        canvas,
        y,
        color::MUTED,
        format_args!("{}, {}", card.mode, card.limits),
    );
    y += 50.0;
    // The numbers.
    let mut cells: [Option<Cell>; 4] = [None, None, None, None];
    let mut count = 0;
    let mut push = |cell| {
        cells[count] = Some(cell);
        count += 1;
    };
    if card.team_game {
        for team in [1_u8, 2] {
            push(Cell::Team {
                team,
                score: card.team_scores[usize::from(team - 1)],
                yours: card.team == team,
            });
        }
    }
    if let You::Playing { score, rank } = card.you {
        push(Cell::Score(score));
        if !card.team_game {
            push(Cell::Place {
                place: (rank & !RANK_TIED) + 1,
                tied: rank & RANK_TIED != 0,
                of: card.playing,
            });
        }
    }
    push(Cell::Clock {
        millis: card.clock_ms,
        left: card.counting_down,
    });
    let width = CARD_WIDTH / count.max(3) as f32;
    let size = if count > 3 { 42.0 } else { 50.0 };
    for (index, cell) in cells.iter().flatten().enumerate() {
        let x = CARD_X + index as f32 * width;
        let value = frame.rect(x, y, width, 56.0);
        let label = frame.rect(x, y + 58.0, width, 22.0);
        let number = |canvas: &mut MenuCanvas, value_text: std::fmt::Arguments<'_>| {
            text(
                canvas,
                TextFamily::Display,
                value_text,
                value,
                size * s,
                color::TEXT,
                FontWeight::Regular,
                TextAlign::Start,
            );
        };
        let caption = |canvas: &mut MenuCanvas, caption_text: std::fmt::Arguments<'_>| {
            text(
                canvas,
                TextFamily::Body,
                caption_text,
                label,
                15.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        };
        match *cell {
            Cell::Score(score) => {
                number(canvas, format_args!("{score}"));
                caption(canvas, format_args!("Your score"));
            }
            Cell::Place { place, tied, of } => {
                number(canvas, format_args!("{place}{}", ordinal(place)));
                if tied {
                    caption(canvas, format_args!("Tied, of {of}"));
                } else {
                    caption(canvas, format_args!("Of {of}"));
                }
            }
            Cell::Team { team, score, yours } => {
                let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: frame.rect(x, y + 62.0, 4.0, 14.0),
                    radius: 2.0 * s,
                    color: team_colour(team),
                });
                number(canvas, format_args!("{score}"));
                let caption_rect = frame.rect(x + 10.0, y + 58.0, width - 10.0, 22.0);
                text(
                    canvas,
                    TextFamily::Body,
                    format_args!(
                        "{}",
                        match (yours, team) {
                            (true, _) => "Your team",
                            (false, 1) => "Red team",
                            (false, _) => "Blue team",
                        }
                    ),
                    caption_rect,
                    15.0 * s,
                    color::MUTED,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
            }
            Cell::Clock { millis, left } => {
                number(canvas, format_args!("{}", Clock(millis)));
                caption(
                    canvas,
                    format_args!("{}", if left { "Time left" } else { "Played" }),
                );
            }
        }
    }
    y += 104.0;
    rule(canvas, y);
    y += 22.0;
    body(
        canvas,
        y,
        color::MUTED,
        format_args!(
            "{} playing, {} watching, {} slots",
            card.playing, card.watching, card.slots
        ),
    );
    if card.you == You::Watching && !main {
        body(
            canvas,
            y + 28.0,
            color::QUIET,
            format_args!("You are spectating"),
        );
    }
}

/// Where this frame's SJK UI text goes: the UI's families once they are
/// loaded, Inter (in `vertices`) until then.
pub(crate) fn text_target<'a>(
    fonts: &'a mut GameFonts,
    vertices: &'a mut Vec<TextVertex>,
    font: &'a UiFont,
) -> TextTarget<'a> {
    if fonts.has_sjk() {
        let style = font.style();
        match fonts.sjk() {
            Some(families) => TextTarget::Families(families, style),
            None => unreachable!("checked above"),
        }
    } else {
        let (vertices, font) = fonts.menu(vertices, font);
        TextTarget::Inter(vertices, font)
    }
}

#[cfg(test)]
impl Card {
    /// A made-up match for the world shots and tests: an FFA on duel6, or a
    /// CTF (`team_game`) with the player on blue; the player spectating when
    /// `watching`. It stays as set ([`Self::refresh`] leaves it).
    pub(crate) fn for_shot(team_game: bool, watching: bool) -> Self {
        let mut card = Self {
            known: true,
            hostname: "^5Yavin ^7Saber Club".to_owned(),
            address: "192.0.2.11:29070".to_owned(),
            map: "duel6".to_owned(),
            mode: if team_game { "CTF" } else { "FFA" },
            limits: if team_game {
                "8 captures, 20 minutes".to_owned()
            } else {
                "30 frags, 20 minutes".to_owned()
            },
            team_game,
            limit_ms: 20 * 60_000,
            clock_ms: 12 * 60_000 + 4_000,
            counting_down: true,
            you: if watching {
                You::Watching
            } else {
                You::Playing { score: 17, rank: 2 }
            },
            team: if watching {
                3
            } else if team_game {
                2
            } else {
                0
            },
            team_scores: [3, 5],
            playing: 14,
            watching: 2,
            slots: 24,
            vote: String::new(),
            vote_yes: 0,
            vote_no: 0,
            read_at: None,
            held: true,
        };
        card.vote.push_str("Change map to mp/ffa3");
        card.vote_yes = 4;
        card.vote_no = 1;
        card
    }

    /// A made-up Siege on duel6 with the player on red, for the world shots.
    pub(crate) fn for_siege_shot() -> Self {
        let mut card = Self::for_shot(true, false);
        card.mode = "Siege";
        card.limits = "20 minutes".to_owned();
        card.team = 1;
        card
    }
}

#[cfg(test)]
mod tests {
    use super::dock::icon_token;
    use super::*;

    const VIEWPORTS: [[f32; 2]; 5] = [
        [1_920.0, 1_080.0],
        [3_840.0, 2_160.0],
        [2_560.0, 1_080.0],
        [1_440.0, 1_080.0],
        [1_024.0, 768.0],
    ];

    fn view(page: Page, selected_row: usize, team_game: bool, team: u8) -> View<'static> {
        View {
            page,
            selected_row,
            team,
            team_game,
            siege: false,
            red_players: 4,
            blue_players: 3,
            vote_active: true,
            staff: false,
            _frame: std::marker::PhantomData,
        }
    }

    /// A card with every line: a vote on, both sides in a CTF on blue.
    fn full_card() -> Controls {
        Controls::for_match(super::super::sjk_focus::Match {
            known: true,
            team_game: true,
            siege: false,
            team: 2,
            vote: true,
        })
    }

    /// Messages for the docked chat: a short one, a long one and one in between.
    const CHAT: [chat_dock::DockLine<'static>; 3] = [
        chat_dock::DockLine {
            name: "^5JoF^7 Jedi",
            text: "anyone up for duels on ffa3?",
            verified: true,
            staff: false,
            key_id: "0123456789abcdef",
            tier: None,
            gif: None,
        },
        chat_dock::DockLine {
            name: "Kyle",
            text: "the new HUD looks great, but the force bar in the corner feels a bit \
                   too small at 4K and the clock could move a little to the left",
            verified: false,
            staff: false,
            key_id: "00000000000000ff",
            tier: None,
            gif: None,
        },
        chat_dock::DockLine {
            name: "^1Fox",
            text: "in 5 min",
            verified: false,
            staff: false,
            key_id: "fedcba9876543210",
            tier: None,
            gif: None,
        },
    ];

    /// The main page's row (Staff tools shown), `controls` and the chat docked in
    /// `dock`, the keyboard on `focus`.
    fn main_extras<'a>(controls: &'a Controls, focus: Focus, dock: &'a mut Dock) -> Extras<'a> {
        Extras {
            focus,
            icons: Icon::shown(true),
            controls,
            chat: Some(Chat {
                dock,
                view: ChatDock {
                    lines: &CHAT,
                    online: 12,
                    live: true,
                    notice: "Turn the SJK identity on to chat",
                    measure: None,
                },
            }),
        }
    }

    fn rows<const N: usize>(labels: [&str; N]) -> (Vec<String>, Vec<String>, Vec<bool>) {
        (
            labels.iter().map(|label| (*label).to_owned()).collect(),
            vec![String::new(); N],
            vec![true; N],
        )
    }

    #[test]
    fn every_page_fits_between_the_title_and_the_keys() {
        for count in 1..=18 {
            let m = metrics(count);
            let [_, top] = entry_point(0, count);
            let [_, bottom] = entry_point(count - 1, count);
            // The page's name over the first entry, the chosen one's line
            // under the last, the keys below.
            assert!(top - m.pitch.max(56.0) - 15.0 > 150.0, "{count}");
            assert!(bottom + 48.0 < KEYS_Y - 40.0, "{count}");
            for index in 0..count {
                let [x, _] = entry_point(index, count);
                // On the left half, clear of the emblem, short of the card.
                assert!(x > EMBLEM[0] + EMBLEM_SIZE * 0.5 + 40.0, "{count} {index}");
                assert!(x + ENTRY_REACH < CARD_X, "{count} {index}");
            }
            // The chosen entry and its neighbours do not overlap.
            assert!(m.pitch >= m.chosen * 0.5 + m.size * 0.5 + 8.0, "{count}");
        }
    }

    #[test]
    fn pages_draw_within_the_canvas_with_every_row_reachable() {
        let card = Card::for_shot(false, false);
        let mut roster = players::State::for_shot(32, true, players::Gate::Open);
        roster.choose(3);
        let mut table = vec!["Player".to_owned(); players::PAGE_ITEMS];
        table.extend(["More players...".to_owned(), "Back".to_owned()]);
        let maps: Vec<String> = (0..16).map(|index| format!("mp/ffa{index}")).collect();
        let mut long: Vec<&str> = maps.iter().map(String::as_str).collect();
        long.extend(["More maps...", "Back"]);
        for viewport in VIEWPORTS {
            for (page, labels) in [
                (Page::Main, Entry::MAIN.map(Entry::label).to_vec()),
                (Page::VoteMap, long.clone()),
                (
                    Page::Leave,
                    vec!["Leave the server", "Quit to desktop", "Stay"],
                ),
                (Page::Players, table.iter().map(String::as_str).collect()),
                (
                    Page::ReportPlayer,
                    sjk_identity::Category::ALL
                        .iter()
                        .map(|category| category.label())
                        .chain(["Back"])
                        .collect(),
                ),
            ] {
                let labels: Vec<String> = labels.iter().map(|label| (*label).to_owned()).collect();
                let hints = vec!["A line".to_owned(); labels.len()];
                let enabled = vec![true; labels.len()];
                let mut canvas = MenuCanvas::new();
                let mut motion = Motion::default();
                let controls = full_card();
                let mut dock = Dock::default();
                let mut extras = if page == Page::Main {
                    main_extras(&controls, Focus::Row(Icon::Staff), &mut dock)
                } else {
                    Extras::NONE
                };
                build(
                    &mut canvas,
                    &view(page, 1, false, 0),
                    &Rows {
                        labels: &labels,
                        hints: &hints,
                        enabled: &enabled,
                    },
                    &Sides {
                        card: &card,
                        players: &roster,
                    },
                    &mut extras,
                    &mut motion,
                    viewport,
                );
                assert!(!canvas.overflowed(), "{page:?} {viewport:?}");
                for row in 0..labels.len() {
                    let rect = canvas.rect_for(row as u16).expect("the row's area");
                    assert!(
                        rect.x >= 0.0 && rect.right() <= viewport[0],
                        "{page:?} {row}"
                    );
                    assert!(
                        rect.y >= 0.0 && rect.bottom() <= viewport[1],
                        "{page:?} {row}"
                    );
                }
            }
        }
    }

    /// The profile card shows bottom left wherever the arc leaves it room, answers the
    /// pointer there, and no entry's area or text reaches into it.
    #[test]
    fn the_profile_card_sits_under_the_arc_when_there_is_room() {
        let card = Card::for_shot(false, false);
        let roster = players::State::for_shot(8, true, players::Gate::Open);
        for viewport in VIEWPORTS {
            for count in 1..=24 {
                let labels: Vec<String> = (0..count).map(|row| format!("Entry {row}")).collect();
                let hints = vec!["What it opens".to_owned(); count];
                let enabled = vec![true; count];
                let page = if count == Entry::MAIN.len() {
                    Page::Main
                } else {
                    Page::VoteMap
                };
                let controls = full_card();
                let mut dock = Dock::default();
                // The row's label shows over it while an icon has the keyboard.
                let mut extras = if page == Page::Main {
                    main_extras(&controls, Focus::Row(Icon::Camera), &mut dock)
                } else {
                    Extras::NONE
                };
                let mut canvas = MenuCanvas::new();
                build(
                    &mut canvas,
                    &view(page, count - 1, false, 0),
                    &Rows {
                        labels: &labels,
                        hints: &hints,
                        enabled: &enabled,
                    },
                    &Sides {
                        card: &card,
                        players: &roster,
                    },
                    &mut extras,
                    &mut Motion::default(),
                    viewport,
                );
                let Some(area) = canvas.rect_for(CARD_TOKEN) else {
                    assert!(count > 12, "{count}: every short page has the card");
                    continue;
                };
                assert!(profile_card_room(page, count));
                assert!(area.right() < viewport[0] * 0.5 && area.bottom() <= viewport[1]);
                let overlaps = |rect: Rect| {
                    rect.x < area.right()
                        && rect.right() > area.x
                        && rect.y < area.bottom()
                        && rect.bottom() > area.y
                };
                for row in 0..count {
                    let rect = canvas.rect_for(row as u16).unwrap();
                    assert!(!overlaps(rect), "{viewport:?} {count}: row {row}");
                }
                // The row of icons sits between the arc and the profile card.
                if page == Page::Main {
                    for icon in Icon::ALL {
                        let rect = canvas.rect_for(icon_token(icon)).expect("an icon's area");
                        assert!(!overlaps(rect), "{viewport:?}: {icon:?}");
                        for row in 0..count {
                            let entry = canvas.rect_for(row as u16).unwrap();
                            assert!(
                                rect.bottom() <= entry.y
                                    || rect.y >= entry.bottom()
                                    || rect.right() <= entry.x
                                    || rect.x >= entry.right(),
                                "{viewport:?}: {icon:?} over row {row}"
                            );
                        }
                    }
                }
                let inside = |rect: Rect| {
                    rect.x >= area.x - 0.5
                        && rect.right() <= area.right() + 0.5
                        && rect.y >= area.y - 0.5
                        && rect.bottom() <= area.bottom() + 0.5
                };
                for command in canvas.draw_list().commands() {
                    if let DrawCommand::Text { rect, .. } = command {
                        assert!(inside(*rect) || !overlaps(*rect), "{viewport:?} {count}");
                    }
                }
            }
        }
        // Not on the Players pages, whose table and player card take the screen.
        assert!(!profile_card_room(Page::Players, 0));
        assert!(!profile_card_room(Page::ReportPlayer, 8));
        assert!(profile_card_room(Page::Main, Entry::MAIN.len()));
        assert!(profile_card_room(Page::Leave, 3));
    }

    /// The main page's icons and card controls answer the pointer where they are
    /// drawn, inside the window, apart from each other and from the card's text above
    /// them; the keyboard's place is the canvas's focus.
    #[test]
    fn the_row_and_the_card_controls_have_their_own_places() {
        let card = Card::for_shot(true, false);
        let roster = players::State::for_shot(8, true, players::Gate::Open);
        let labels: Vec<String> = Entry::MAIN.map(Entry::label).map(str::to_owned).to_vec();
        let hints = vec!["What it opens".to_owned(); labels.len()];
        let enabled = vec![true; labels.len()];
        for viewport in VIEWPORTS {
            for (controls, focus) in [
                (full_card(), Focus::Card(Control::VoteNo)),
                (
                    Controls::for_match(super::super::sjk_focus::Match {
                        known: true,
                        team_game: false,
                        siege: false,
                        team: 3,
                        vote: false,
                    }),
                    Focus::Card(Control::CallVote),
                ),
            ] {
                let mut canvas = MenuCanvas::new();
                let mut dock = Dock::default();
                let mut extras = main_extras(&controls, focus, &mut dock);
                let mut team_view = view(Page::Main, 2, true, 2);
                team_view.staff = true;
                build(
                    &mut canvas,
                    &team_view,
                    &Rows {
                        labels: &labels,
                        hints: &hints,
                        enabled: &enabled,
                    },
                    &Sides {
                        card: &card,
                        players: &roster,
                    },
                    &mut extras,
                    &mut Motion::default(),
                    viewport,
                );
                assert!(!canvas.overflowed(), "{viewport:?}");
                let mut areas: Vec<Rect> = Vec::new();
                for (index, placed) in controls.as_slice().iter().enumerate() {
                    let token = CONTROL_TOKEN + index as u16;
                    let rect = canvas.rect_for(token).expect("a control's area");
                    assert!(rect.x >= 0.0 && rect.right() <= viewport[0], "{placed:?}");
                    assert!(rect.y >= 0.0 && rect.bottom() <= viewport[1], "{placed:?}");
                    areas.push(rect);
                }
                for icon in Icon::ALL {
                    areas.push(canvas.rect_for(icon_token(icon)).expect("an icon"));
                }
                // The docked chat's field, Open chat and names, and the profile card.
                let tokens = CHAT_TOKENS;
                for token in [tokens.field, tokens.open, tokens.name(0), CARD_TOKEN] {
                    areas.push(canvas.rect_for(token).expect("a chat or card area"));
                }
                // Clear of the list's entries.
                for row in 0..labels.len() {
                    let entry = canvas.rect_for(row as u16).expect("an entry");
                    for token in [tokens.field, tokens.open, tokens.name(0)] {
                        let rect = canvas.rect_for(token).unwrap();
                        assert!(
                            rect.bottom() <= entry.y
                                || rect.y >= entry.bottom()
                                || rect.right() <= entry.x
                                || rect.x >= entry.right(),
                            "{viewport:?}: chat {token} over row {row}"
                        );
                    }
                }
                for (index, rect) in areas.iter().enumerate() {
                    for other in &areas[index + 1..] {
                        let apart = rect.bottom() <= other.y
                            || rect.y >= other.bottom()
                            || rect.right() <= other.x
                            || rect.x >= other.right();
                        assert!(apart, "{viewport:?}: {rect:?} and {other:?}");
                    }
                }
                let focused = super::dock::focus_token(2, &extras);
                assert!(focused >= CONTROL_TOKEN, "{focus:?}");
            }
        }
    }

    /// Resting the pointer on a name in the docked chat shows the sender's card left of
    /// the dock (over none of its lines, nor the match card's), above the keys, clear of
    /// the profile card and the list, inside the window; it stays while the pointer is on
    /// it.
    #[test]
    fn the_chat_sender_card_shows_left_of_the_dock() {
        use sjk_ui::{InputEvent, Vec2};
        let card = Card::for_shot(false, false);
        let roster = players::State::for_shot(8, true, players::Gate::Open);
        let labels: Vec<String> = Entry::MAIN.map(Entry::label).map(str::to_owned).to_vec();
        let hints = vec!["What it opens".to_owned(); labels.len()];
        let enabled = vec![true; labels.len()];
        let controls = full_card();
        let centre = |rect: Rect| Vec2::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5);
        let overlaps = |a: Rect, b: Rect| {
            a.x < b.right() && a.right() > b.x && a.y < b.bottom() && a.bottom() > b.y
        };
        let mut hovered = 0;
        for viewport in VIEWPORTS {
            let mut canvas = MenuCanvas::new();
            let mut dock = Dock::default();
            let draw = |canvas: &mut MenuCanvas, dock: &mut Dock| {
                build(
                    canvas,
                    &view(Page::Main, 0, false, 0),
                    &Rows {
                        labels: &labels,
                        hints: &hints,
                        enabled: &enabled,
                    },
                    &Sides {
                        card: &card,
                        players: &roster,
                    },
                    &mut main_extras(&controls, Focus::List, dock),
                    &mut Motion::default(),
                    viewport,
                );
            };
            draw(&mut canvas, &mut dock);
            assert!(canvas.rect_for(CHAT_TOKENS.card).is_none());
            for index in 0..CHAT.len() {
                let Some(name) = canvas.rect_for(CHAT_TOKENS.name(index)) else {
                    continue;
                };
                canvas.pointer(InputEvent::PointerMove(centre(name)));
                draw(&mut canvas, &mut dock);
                hovered += 1;
                assert!(!canvas.overflowed(), "{viewport:?}");
                let sender = canvas.rect_for(CHAT_TOKENS.card).expect("the sender card");
                let frame = Frame::new(viewport);
                let [dock_left, keys_top] = frame.point(CHAT_PLACE.x - 14.0, KEYS_Y - 24.0);
                assert!(sender.right() <= dock_left, "{viewport:?}: {sender:?}");
                assert!(sender.bottom() <= keys_top, "{viewport:?}: {sender:?}");
                assert!(
                    sender.x >= 0.0 && sender.y >= 0.0,
                    "{viewport:?}: {sender:?}"
                );
                for row in 0..labels.len() {
                    let entry = canvas.rect_for(row as u16).expect("an entry");
                    assert!(!overlaps(sender, entry), "{viewport:?}: over entry {row}");
                }
                let profile = canvas.rect_for(CARD_TOKEN).expect("the profile card");
                assert!(!overlaps(sender, profile), "{viewport:?}");
                // Onto the card: it stays.
                let mute = canvas.rect_for(CHAT_TOKENS.mute).expect("Mute");
                canvas.pointer(InputEvent::PointerMove(centre(mute)));
                draw(&mut canvas, &mut dock);
                assert!(canvas.rect_for(CHAT_TOKENS.card).is_some(), "{viewport:?}");
                // Off the name and the card: it stays a moment, for the way across,
                // then goes.
                canvas.pointer(InputEvent::PointerMove(Vec2::new(1.0, 1.0)));
                draw(&mut canvas, &mut dock);
                assert!(canvas.rect_for(CHAT_TOKENS.card).is_some(), "{viewport:?}");
                dock.expire_card_for_test();
                draw(&mut canvas, &mut dock);
                assert!(canvas.rect_for(CHAT_TOKENS.card).is_none(), "{viewport:?}");
            }
        }
        assert!(hovered >= VIEWPORTS.len() * 2, "{hovered} names hovered");
    }

    /// Sol's layout (11/10/2026): the chat docks in the match card's column, under its
    /// last control (Call a vote, or the side's buttons while a vote is on), inside the
    /// window and right of the keys.
    #[test]
    fn the_chat_docks_under_the_match_card() {
        let card = Card::for_shot(false, false);
        let roster = players::State::for_shot(8, true, players::Gate::Open);
        let labels: Vec<String> = Entry::MAIN.map(Entry::label).map(str::to_owned).to_vec();
        let hints = vec!["What it opens".to_owned(); labels.len()];
        let enabled = vec![true; labels.len()];
        for vote in [false, true] {
            let controls = Controls::for_match(super::super::sjk_focus::Match {
                known: true,
                team_game: true,
                siege: false,
                team: 2,
                vote,
            });
            for viewport in VIEWPORTS {
                let mut canvas = MenuCanvas::new();
                let mut dock = Dock::default();
                let mut shown = view(Page::Main, 0, true, 2);
                shown.vote_active = vote;
                build(
                    &mut canvas,
                    &shown,
                    &Rows {
                        labels: &labels,
                        hints: &hints,
                        enabled: &enabled,
                    },
                    &Sides {
                        card: &card,
                        players: &roster,
                    },
                    &mut main_extras(&controls, Focus::List, &mut dock),
                    &mut Motion::default(),
                    viewport,
                );
                let frame = Frame::new(viewport);
                let lowest = (0..controls.as_slice().len())
                    .filter_map(|index| canvas.rect_for(CONTROL_TOKEN + index as u16))
                    .map(Rect::bottom)
                    .fold(0.0, f32::max);
                let field = canvas.rect_for(CHAT_TOKENS.field).expect("the field");
                let open = canvas.rect_for(CHAT_TOKENS.open).expect("Open chat");
                let first = canvas.rect_for(CHAT_TOKENS.name(0)).expect("a name");
                let [column, _] = frame.point(CARD_X - 20.0, 0.0);
                let [keys_right, _] = frame.point(1_290.0, 0.0);
                assert!(first.y > lowest, "{vote} {viewport:?}: {first:?} {lowest}");
                assert!(field.x >= column && field.x >= keys_right, "{viewport:?}");
                assert!(open.bottom() <= viewport[1], "{viewport:?}: {open:?}");
                assert!(field.right() <= viewport[0], "{viewport:?}: {field:?}");
            }
        }
    }

    #[test]
    fn the_main_page_lists_every_function_of_the_menu() {
        let (mut labels, mut hints, _) = rows([""; 24]);
        let count = prepare(&view(Page::Main, 0, false, 0), &mut labels, &mut hints);
        assert_eq!(count, Some(Entry::MAIN.len()));
        // Sol's six (09/10/2026): Team and Vote are on the match card, Camera
        // control and SJK's pages in the row of icons; then Collection (10/10/2026).
        assert_eq!(
            labels[..Entry::MAIN.len()],
            [
                "Resume",
                "Profile",
                "Collection",
                "Players",
                "Settings",
                "Servers",
                "Leave"
            ]
        );
        assert!(
            hints[..Entry::MAIN.len()]
                .iter()
                .all(|hint| !hint.is_empty())
        );
        for (row, entry) in Entry::MAIN.iter().enumerate() {
            assert_eq!(Entry::at(row), Some(*entry));
            assert_eq!(entry.index(), row);
        }
        assert_eq!(Entry::at(Entry::MAIN.len()), None);
    }

    #[test]
    fn team_rows_say_who_is_where() {
        let (mut labels, mut hints, _) = rows([""; 24]);
        let count = prepare(&view(Page::Team, 0, true, 2), &mut labels, &mut hints);
        assert_eq!(count, Some(5));
        assert_eq!(
            labels[..5],
            ["Auto-join", "Red team", "Blue team", "Spectate", "Back"]
        );
        assert_eq!(hints[0], "The side with fewer players: 4 red, 3 blue");
        assert_eq!(hints[1], "4 players");
        assert_eq!(hints[2], "Your team, 3 players");
        let (mut labels, mut hints, _) = rows([""; 24]);
        prepare(&view(Page::Team, 0, false, 3), &mut labels, &mut hints);
        assert_eq!(labels[..3], ["Join the game", "Spectate", "Back"]);
        assert_eq!(hints[1], "You are watching");
    }

    #[test]
    fn shared_rows_keep_their_label_and_move_the_detail_to_the_hint() {
        let (mut labels, mut hints, _) = rows([
            "Red team  /  switch to Blue",
            "Padawan  /  client 3",
            "Back",
        ]);
        split_hints(Page::VoteClientKick, &mut labels, &mut hints);
        assert_eq!(labels, ["Red team", "Padawan", "Back"]);
        assert_eq!(hints, ["switch to Blue", "client 3", ""]);
        let (mut labels, mut hints, _) = rows(["Duel  /  3"]);
        split_hints(Page::VoteGameType, &mut labels, &mut hints);
        assert_eq!((labels[0].as_str(), hints[0].as_str()), ("Duel", ""));
    }

    #[test]
    fn escape_returns_to_the_entry_that_opened_the_page() {
        assert_eq!(parent(Page::Main), None);
        assert_eq!(parent(Page::Shot), None);
        // Pages the match card opens return to the main page, the keyboard on the
        // card's control that opened them.
        for (page, focus) in [
            (
                Page::Team,
                Focus::Card(Control::Team(sjk_client::LegacyTeamChoice::Spectator)),
            ),
            (Page::Siege, Focus::Card(Control::SiegeClass)),
            (Page::CallVote, Focus::Card(Control::CallVote)),
            (Page::VoteMap, Focus::Card(Control::CallVote)),
        ] {
            assert_eq!(return_focus(page), focus, "{page:?}");
        }
        assert_eq!(parent(Page::Team).map(|(page, _)| page), Some(Page::Main));
        assert_eq!(
            parent(Page::CallVote).map(|(page, _)| page),
            Some(Page::Main)
        );
        assert_eq!(return_focus(Page::Players), Focus::List);
        assert_eq!(
            parent(Page::Players),
            Some((Page::Main, Entry::Players.index()))
        );
        assert_eq!(
            parent(Page::ReportPlayer).map(|(page, _)| page),
            Some(Page::Players)
        );
        assert_eq!(
            parent(Page::Leave),
            Some((Page::Main, Entry::Leave.index()))
        );
        for list in [
            Page::VoteMap,
            Page::VoteGameType,
            Page::VoteKick,
            Page::VoteClientKick,
            Page::VoteWarmup,
            Page::VoteTimeLimit,
            Page::VoteFragLimit,
        ] {
            let (page, row) = parent(list).expect("a call-vote list's parent");
            assert_eq!(page, Page::CallVote);
            assert_eq!(super::super::callvote::opening_row(list), Some(row));
        }
        for page in [Page::Leave, Page::Main, Page::Team] {
            assert!(row_count(page, false).is_some(), "{page:?}");
        }
    }

    #[test]
    fn only_leaving_turns_ember_and_ways_back_are_quiet() {
        assert_eq!(tone(Page::Leave, 0, "Leave the server"), Tone::Leaving);
        assert_eq!(tone(Page::Leave, 1, "Quit to desktop"), Tone::Leaving);
        assert_eq!(tone(Page::Leave, 2, "Stay"), Tone::Back);
        assert_eq!(tone(Page::Main, Entry::Leave.index(), "Leave"), Tone::Back);
        assert_eq!(tone(Page::Main, 0, "Resume"), Tone::Normal);
        assert_eq!(tone(Page::CallVote, 9, "Back"), Tone::Back);
    }

    #[test]
    fn the_card_reads_the_match_from_the_game_state() {
        let mut game = GameState::empty_local(0);
        let mut set = |index: usize, value: &str| {
            game.replace_config_string(index, value.as_bytes().to_vec())
                .expect("a config string");
        };
        set(
            0,
            "\\sv_hostname\\^4JoF ^7duels\\mapname\\mp/duel6\\g_gametype\\0\\timelimit\\20\\fraglimit\\30\\sv_maxclients\\24",
        );
        set(CS_LEVEL_START_TIME, "1000");
        for client in 0..5 {
            set(CS_PLAYERS + client, "\\n\\Player\\t\\0");
        }
        set(CS_PLAYERS + 7, "\\n\\Watcher\\t\\3");
        let mut card = Card::default();
        let local = Local {
            team: 0,
            spectator: false,
            score: 17,
            rank: 2 | RANK_TIED,
        };
        let time = 1_000 + 7 * 60_000 + 56_000;
        card.refresh(&game, local, time, [0, 0], Some("192.0.2.11:29070"));
        assert!(card.known);
        assert_eq!(card.hostname, "^4JoF ^7duels");
        assert_eq!(card.map, "duel6");
        assert_eq!(card.mode, "FFA");
        assert_eq!(card.limits, "30 frags, 20 minutes");
        assert_eq!((card.playing, card.watching, card.slots), (5, 1, 24));
        assert!(card.counting_down);
        assert_eq!(Clock(card.clock_ms).to_string(), "12:04");
        assert_eq!(
            card.you,
            You::Playing {
                score: 17,
                rank: 2 | RANK_TIED
            }
        );
        assert!(card.vote.is_empty());
        // Past the limit (overtime), the clock counts up.
        card.refresh(&game, local, 1_000 + 21 * 60_000, [0, 0], None);
        assert!(!card.counting_down);
        // A spectator, then no match at all.
        card.refresh(
            &game,
            Local {
                spectator: true,
                ..local
            },
            time,
            [0, 0],
            None,
        );
        assert_eq!(card.you, You::Watching);
        card.forget();
        assert!(!card.known);
    }

    #[test]
    fn places_and_clocks_read_as_words() {
        let words: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 101]
            .iter()
            .map(|place| format!("{place}{}", ordinal(*place)))
            .collect();
        assert_eq!(
            words,
            [
                "1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "101st"
            ]
        );
        assert_eq!(Clock(0).to_string(), "0:00");
        assert_eq!(Clock(65_999).to_string(), "1:05");
        assert_eq!(Clock(-5).to_string(), "0:00");
        assert_eq!(Players(1).to_string(), "1 player");
        assert_eq!(Players(0).to_string(), "0 players");
    }

    #[test]
    fn the_mark_eases_towards_the_chosen_entry() {
        let mut motion = Motion::default();
        assert_eq!(motion.towards(0.5, 10.0), 0.5);
        let next = motion.towards(-0.2, 10.05);
        assert!(next < 0.5 && next > -0.2);
    }
}
