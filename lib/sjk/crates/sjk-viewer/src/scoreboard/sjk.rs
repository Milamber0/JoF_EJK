//! The SJK UI's scoreboard (`cg_scoreboardStyle sjk`, or `auto` with the SJK
//! UI's menus): the match's columns floating over the game dimmed behind them,
//! in the UI's families and colours, with no panel or box (`docs/sjk-ui.md`,
//! Scoreboard).
//!
//! It is laid out on the UI's 16:9 frame ([`Frame`]). A header line runs across
//! the top as the other screens' top bar does: the map, the mode with its
//! limits, the time and your place ("3rd of 14"), the screen's one memorable
//! thing, in gold. The columns stand right of the chat column, which the
//! scoreboard keeps for messages (`chat::view`): one list in free-for-all (two
//! side by side once rows would be too thin), the two teams side by side in
//! team games, each under its score with a thin rule in a muted team colour, and
//! in duels the duelists as two facing cards over the players waiting their
//! turn. Compact (`cg_compactScoreboard`, the default) packs the rows thinner
//! so a full server stays in one column, and sizes the lists from what they
//! show rather than the screen: the name column as wide as the longest name
//! (within bounds), the numbers right after it, the whole board centred on the
//! screen as far as the chat column allows ([`Board`]). Spectators share one
//! line at the bottom. Your row is on the UI's gold
//! band with gold accents; bots and spectators are quieter.
//!
//! Nothing here allocates: runs are formatted into the canvas's retained
//! slots, and measured text goes through a fixed buffer ([`Short`]).

use super::ScoreRow;
use super::classic::{FlagIcons, LocalStatus};
use super::motion::Motion;
use crate::menu::sjk::{Frame, color, fade, fade_across, kit, text};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use crate::text::{Plain, TextFace, UiFont};
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};
use std::fmt::{self, Write as _};

/// `GT_DUEL`, `GT_POWERDUEL`, `GT_TEAM`, `GT_SIEGE`, `GT_CTF`, `GT_CTY`.
const DUEL: i32 = 3;
const POWER_DUEL: i32 = 4;
const TEAM: i32 = 6;
const SIEGE: i32 = 7;
const CTF: i32 = 8;
const CTY: i32 = 9;
/// `TEAM_RED`, `TEAM_BLUE`, `TEAM_SPECTATOR`.
const RED: u8 = 1;
const BLUE: u8 = 2;
const SPECTATOR: u8 = 3;
/// `RANK_TIED_FLAG` in `persistant[PERS_RANK]`.
const RANK_TIED: u32 = 0x4000;

/// The header line's ends and middle line (frame pixels), as the other
/// screens' top bar.
const LEFT: f32 = 96.0;
const RIGHT: f32 = 1824.0;
const BAR_Y: f32 = 87.0;
/// The columns start right of the chat column the scoreboard keeps for
/// messages (40 + 600 pixels of a 1080-line window, `scoreboard::layout`).
const BOARD_LEFT: f32 = 680.0;
const BOARD_WIDTH: f32 = RIGHT - BOARD_LEFT;
/// Between two lists side by side.
const GUTTER: f32 = 40.0;
/// Free-for-all's first row; a team's, under its head.
const LIST_TOP: f32 = 206.0;
const TEAM_TOP: f32 = 316.0;
/// Rows end here; the spectators' line sits under them.
const BOTTOM: f32 = 962.0;
const SPECTATORS_Y: f32 = 1002.0;
/// Row heights: the most, the least before a list splits in two, the least
/// at all.
const ROW_MOST: f32 = 52.0;
const ROW_SPLIT: f32 = 34.0;
const ROW_FLOOR: f32 = 26.0;
/// Compact rows: the most, and the least before a list splits in two (only
/// a list longer than a full server's team, such as a duel's queue, does).
const COMPACT_MOST: f32 = 32.0;
const COMPACT_FLOOR: f32 = 20.0;
/// A compact list's name column: at least this wide, so a server of short
/// names still reads as a board, and at most this wide (a longer name ends in
/// an ellipsis); squeezed to [`COMPACT_NAME_SQUEEZED`] at the least when the
/// board would otherwise run past [`RIGHT`].
const COMPACT_NAME_LEAST: f32 = 140.0;
const COMPACT_NAME_MOST: f32 = 320.0;
const COMPACT_NAME_SQUEEZED: f32 = 100.0;
/// From a compact list's name column to the first number's right edge.
const COMPACT_NAME_GAP: f32 = 48.0;
/// A compact number column: its widest label and this gap before it, and
/// never narrower than [`COMPACT_STAT_LEAST`].
const COMPACT_LABEL_GAP: f32 = 14.0;
const COMPACT_STAT_LEAST: f32 = 48.0;
/// The frame's middle, which a compact board centres on.
const CENTRE: f32 = 960.0;
/// A compact board stays this far right of the chat column, as [`BOARD_LEFT`]
/// is of the column at 1080 lines.
const CHAT_CLEARANCE: f32 = 40.0;
/// From the last number column to the ping's right edge, in all but the
/// widest full list.
const PING_COLUMN: f32 = 100.0;
/// A duelist's card, and how far it stands from the middle of the pair.
const CARD_WIDTH: f32 = 470.0;
const CARD_OFFSET: f32 = 64.0;
/// Where the signal bars start before the ping's right edge: their 25 pixels,
/// a gap, then room for three digits.
const BARS_BEFORE_PING: f32 = 76.0;
/// How much of the UI's navy lies over the game behind the columns, over
/// the general dim.
const BOARD_DIM: f32 = 0.5;
/// A team's colour, muted: only its thin rule and its name wear it.
const RED_TEAM: Color = Color::new(0.88, 0.38, 0.38, 1.0);
const BLUE_TEAM: Color = Color::new(0.44, 0.64, 0.98, 1.0);

/// The match's limits, from the server's info.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Limits {
    pub(super) frags: i32,
    pub(super) captures: i32,
    pub(super) duel_wins: i32,
    pub(super) minutes: i32,
}

/// Who duels and what is known of their health.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Duelists {
    /// `CS_CLIENT_DUELISTS`: the duelists (power duel: the lone one, then the
    /// pair).
    pub(super) clients: [Option<u8>; 3],
    /// `CS_CLIENT_DUELHEALTHS` (the server's `g_showDuelHealths`): the first
    /// two duelists' health.
    pub(super) healths: [Option<i32>; 2],
    /// Your own health and shield, from your snapshot.
    pub(super) local: Option<(i32, i32)>,
}

/// Match facts the SJK look shows beyond the rows.
pub(super) struct SjkHeader<'a> {
    /// The map, `mp/` and all.
    pub(super) map: &'a str,
    pub(super) gametype: i32,
    pub(super) limits: Limits,
    /// Milliseconds since the level started, when known.
    pub(super) elapsed: Option<i32>,
    pub(super) team_scores: [i32; 2],
    pub(super) local: LocalStatus,
    /// Who last killed you, while you are dead.
    pub(super) killer: Option<&'a str>,
    pub(super) duel: Duelists,
    /// `cg_compactScoreboard`: thin rows, every player in one column.
    pub(super) compact: bool,
    /// The frame rate to show left of the clock (`cg_scoreboardFps`), when on and
    /// measured.
    pub(super) fps: Option<u32>,
}

/// Measures runs before they are drawn, so what follows them can be placed:
/// the families' own metrics once loaded, or Inter's, which then draws them.
/// Without a font (unit tests) widths are estimated from the letters.
#[derive(Clone, Copy, Default)]
pub(super) struct Measure<'a> {
    pub(super) display: Option<&'a UiFont>,
    pub(super) body: Option<&'a UiFont>,
}

impl Measure<'_> {
    /// Width in window pixels of `value` set at `size` (window pixels).
    fn width(&self, family: TextFamily, value: &str, size: f32, weight: FontWeight) -> f32 {
        let font = match family {
            TextFamily::Display => self.display,
            TextFamily::Body | TextFamily::Code => self.body,
        };
        let face = match weight {
            FontWeight::Regular => TextFace::Regular,
            FontWeight::Semibold => TextFace::Semibold,
        };
        match font {
            Some(font) => {
                crate::text::visible_text_width_face(font, value, size / font.height.max(1.0), face)
            }
            None => {
                let share = match family {
                    TextFamily::Display => 0.5,
                    TextFamily::Body | TextFamily::Code => 0.56,
                };
                visible_chars(value) as f32 * size * share
            }
        }
    }
}

/// Characters `value` shows, without its colour codes.
fn visible_chars(value: &str) -> usize {
    let mut count = 0;
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '^' && chars.peek().is_some_and(char::is_ascii_digit) {
            chars.next();
        } else {
            count += 1;
        }
    }
    count
}

/// A short run formatted on the stack, to measure and then draw it.
struct Short {
    bytes: [u8; 64],
    len: usize,
}

impl Short {
    fn of(value: fmt::Arguments<'_>) -> Self {
        let mut short = Self {
            bytes: [0; 64],
            len: 0,
        };
        let _ = short.write_fmt(value);
        short
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl fmt::Write for Short {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let end = self.len + value.len();
        if end > self.bytes.len() {
            return Err(fmt::Error);
        }
        self.bytes[self.len..end].copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}

/// The ordinal suffix of `place`: 1st, 2nd, 3rd, 4th, 11th, 22nd.
pub(super) fn ordinal(place: u32) -> &'static str {
    match (place % 100, place % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    }
}

/// The game type's name, as the menus name it.
pub(super) fn mode_name(gametype: i32) -> &'static str {
    match gametype {
        0 => "Free for all",
        1 => "Holocron FFA",
        2 => "Jedi Master",
        DUEL => "Duel",
        POWER_DUEL => "Power duel",
        TEAM => "Team FFA",
        SIEGE => "Siege",
        CTF => "Capture the flag",
        CTY => "Capture the ysalamiri",
        _ => "Match",
    }
}

/// A map's name without the `mp/` every multiplayer map starts with.
fn short_map(map: &str) -> &str {
    map.get(..3)
        .filter(|prefix| prefix.eq_ignore_ascii_case("mp/"))
        .map_or(map, |_| &map[3..])
}

/// The mode's limits in words after the mode, as the server browser words
/// them: " · 30 frags · 20 minutes".
struct LimitWords(Limits, i32);

impl fmt::Display for LimitWords {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self(limits, gametype) = *self;
        if matches!(gametype, CTF | CTY) {
            if limits.captures > 0 {
                write!(formatter, " · {} captures", limits.captures)?;
            }
        } else if gametype != SIEGE && limits.frags > 0 {
            write!(formatter, " · {} frags", limits.frags)?;
        }
        if matches!(gametype, DUEL | POWER_DUEL) && limits.duel_wins > 0 {
            write!(formatter, " · {} duel wins", limits.duel_wins)?;
        }
        if limits.minutes > 0 {
            write!(formatter, " · {} minutes", limits.minutes)?;
        }
        Ok(())
    }
}

/// The spectators' names without their colours, one after another.
struct Watching<'a>(&'a [ScoreRow]);

impl fmt::Display for Watching<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, row) in self.0.iter().filter(|row| watching(row)).enumerate() {
            if index > 0 {
                formatter.write_str("  ·  ")?;
            }
            write!(formatter, "{}", Plain(&row.name))?;
        }
        Ok(())
    }
}

fn watching(row: &ScoreRow) -> bool {
    row.team == SPECTATOR
}

/// A number column of a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stat {
    Score,
    Deaths,
    /// Duel wins and losses.
    Record,
    Captures,
    Assists,
    Defends,
    /// Minutes in the match.
    Time,
}

impl Stat {
    fn label(self, wide: bool) -> &'static str {
        match self {
            Self::Score => "Score",
            Self::Deaths => "Deaths",
            Self::Record => "Won/lost",
            Self::Captures if wide => "Captures",
            Self::Captures => "Caps",
            Self::Assists => "Assists",
            Self::Defends => "Defends",
            Self::Time => "Minutes",
        }
    }
}

/// A row's value in one column.
struct StatText<'a>(Stat, &'a ScoreRow);

impl fmt::Display for StatText<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self(stat, row) = self;
        match stat {
            Stat::Score => write!(formatter, "{}", row.score),
            Stat::Deaths => match row.deaths {
                Some(deaths) => write!(formatter, "{deaths}"),
                None => formatter.write_str("–"),
            },
            Stat::Record => match row.record {
                Some((wins, losses)) => write!(formatter, "{wins}/{losses}"),
                None => formatter.write_str("–"),
            },
            Stat::Captures => write!(formatter, "{}", row.captures),
            Stat::Assists => write!(formatter, "{}", row.assists),
            Stat::Defends => write!(formatter, "{}", row.defends),
            Stat::Time => write!(formatter, "{}", row.time),
        }
    }
}

/// What a list shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Free-for-all players: score, deaths when counted, time.
    Free { deaths: bool },
    /// A team: score and time, or in capture modes score, captures, assists
    /// and defends (and a carried flag before the name).
    Team { capture: bool },
    /// Players waiting their duel: their record and time.
    Waiting,
}

impl Kind {
    /// The list's number columns, left to right.
    fn stats(self) -> &'static [Stat] {
        match self {
            Self::Free { deaths: true } => &[Stat::Score, Stat::Deaths, Stat::Time],
            Self::Free { deaths: false } | Self::Team { capture: false } => {
                &[Stat::Score, Stat::Time]
            }
            Self::Team { capture: true } => {
                &[Stat::Score, Stat::Captures, Stat::Assists, Stat::Defends]
            }
            Self::Waiting => &[Stat::Record, Stat::Time],
        }
    }

    fn capture(self) -> bool {
        matches!(self, Self::Team { capture: true })
    }
}

/// Where a list's columns stand (frame pixels).
#[derive(Clone, Copy, Debug)]
struct Columns {
    x: f32,
    width: f32,
    wide: bool,
    /// The place's right edge, when the list numbers its rows.
    place: Option<f32>,
    /// A carried flag's icon, in capture modes.
    flag: Option<f32>,
    name: f32,
    name_end: f32,
    /// Each number column and its right edge.
    stats: [(Stat, f32); 4],
    count: usize,
    stat_width: f32,
    /// The signal bars' left and the ping's right edge.
    bars: f32,
    ping: f32,
}

impl Columns {
    /// A full board's list, `width` wide from `x`: the numbers packed at its
    /// right, the name taking what they leave.
    fn new(x: f32, width: f32, kind: Kind, numbered: bool) -> Self {
        let wide = width > 800.0;
        let stat_width = match kind {
            Kind::Team { capture: true } => 66.0,
            _ if wide => 124.0,
            _ => 74.0,
        };
        // Numbers are right-aligned, so the name may run into the start of
        // the first number column.
        let name_gap = if wide { 72.0 } else { 52.0 };
        Self::lay_out(x, width, kind, numbered, wide, stat_width, name_gap)
    }

    /// A compact list, `width` wide from `x`, its number columns
    /// `stat_width` apart; [`Self::compact_span`] gives the width for a name
    /// column.
    fn compact(x: f32, width: f32, kind: Kind, numbered: bool, stat_width: f32) -> Self {
        Self::lay_out(
            x,
            width,
            kind,
            numbered,
            false,
            stat_width,
            COMPACT_NAME_GAP,
        )
    }

    /// The width of a compact list whose name column is `room` wide.
    fn compact_span(kind: Kind, numbered: bool, room: f32, stat_width: f32) -> f32 {
        Self::lead(kind, numbered, false)
            + room
            + COMPACT_NAME_GAP
            + stat_width * (kind.stats().len() - 1) as f32
            + PING_COLUMN
    }

    /// From a list's left edge to its names: the place, a carried flag.
    fn lead(kind: Kind, numbered: bool, wide: bool) -> f32 {
        let place = match (numbered, wide) {
            (true, true) => 68.0,
            (true, false) => 52.0,
            (false, _) => 10.0,
        };
        place + if kind.capture() { 30.0 } else { 0.0 }
    }

    fn lay_out(
        x: f32,
        width: f32,
        kind: Kind,
        numbered: bool,
        wide: bool,
        stat_width: f32,
        name_gap: f32,
    ) -> Self {
        let right = x + width;
        let ping_width = if wide { 128.0 } else { PING_COLUMN };
        let order = kind.stats();
        let first = right - ping_width - stat_width * (order.len() - 1) as f32;
        let mut stats = [(Stat::Score, 0.0); 4];
        for (index, stat) in order.iter().enumerate() {
            stats[index] = (*stat, first + stat_width * index as f32);
        }
        let place = numbered.then_some(x + if wide { 46.0 } else { 34.0 });
        let name = x + Self::lead(kind, numbered, wide);
        let flag = kind.capture().then_some(name - 30.0);
        Self {
            x,
            width,
            wide,
            place,
            flag,
            name,
            name_end: first - name_gap,
            stats,
            count: order.len(),
            stat_width,
            // The bars just before the number, as the server browser has them.
            bars: right - BARS_BEFORE_PING,
            ping: right,
        }
    }
}

/// How a list's rows stand: in one column or two halves side by side, each
/// row `height` tall, `fits` to a half.
#[derive(Clone, Copy, Debug)]
struct Fit {
    halves: usize,
    height: f32,
    fits: usize,
}

/// Where the lists stand across the frame (frame pixels): the first from `x`,
/// each `width` wide (the teams' second a [`GUTTER`] after the first, a duel's
/// players waiting under the cards), the duelists' cards about `centre`.
/// `span` is a compact board's left and right edges, which the dim behind it
/// follows; the full board's dim runs to the window's right edge.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Board {
    x: f32,
    width: f32,
    centre: f32,
    span: Option<[f32; 2]>,
}

/// The least left edge a compact board takes (frame pixels): clear of the
/// chat column the scoreboard keeps (`scoreboard::layout`), wherever the
/// window puts it in the frame.
fn chat_clear(frame: &Frame, viewport: [f32; 2]) -> f32 {
    let chat = super::layout::Layout::new(viewport);
    (chat.chat_left + chat.chat_width - frame.origin[0]) / frame.s + CHAT_CLEARANCE
}

/// The name and number sizes of a row `height` tall (frame pixels).
fn text_sizes(compact: bool, height: f32) -> (f32, f32) {
    // Compact rows fill more of their height, to stay readable.
    if compact {
        (
            (height * 0.7).clamp(16.0, 20.0),
            (height * 0.75).clamp(17.0, 22.0),
        )
    } else {
        (
            (height * 0.44).clamp(16.0, 22.0),
            (height * 0.5).clamp(18.0, 26.0),
        )
    }
}

/// One frame's drawing state.
struct Painter<'a> {
    ui: &'a mut MenuCanvas,
    frame: Frame,
    header: &'a SjkHeader<'a>,
    measure: Measure<'a>,
    flags: FlagIcons,
    motion: &'a mut Motion,
    /// The least left edge a compact board takes ([`chat_clear`]).
    clear: f32,
}

/// Build the SJK UI's scoreboard into `ui`.
pub(super) fn build(
    ui: &mut MenuCanvas,
    rows: &[ScoreRow],
    header: &SjkHeader<'_>,
    flags: FlagIcons,
    measure: Measure<'_>,
    motion: &mut Motion,
    viewport: [f32; 2],
) {
    ui.begin_transparent(viewport);
    let opacity = motion.opacity();
    ui.push_opacity(opacity);
    let frame = Frame::new(viewport);
    let mut painter = Painter {
        ui,
        frame,
        header,
        measure,
        flags,
        motion,
        clear: chat_clear(&frame, viewport),
    };
    let board = painter.board(rows);
    dim(painter.ui, &frame, viewport, board.span);
    // It settles into place as it opens, as the classic board does.
    painter.frame.origin[1] -= (1.0 - opacity) * 12.0 * frame.s;
    painter.header_line(rows);
    if rows.is_empty() {
        painter.requesting();
    } else if matches!(header.gametype, DUEL | POWER_DUEL) {
        painter.duel(rows, &board);
    } else if header.gametype >= TEAM {
        painter.teams(rows, &board);
        painter.spectators(rows, &board);
    } else {
        painter.free_for_all(rows, &board);
        painter.spectators(rows, &board);
    }
    painter.ui.pop_opacity();
}

/// The game a little darker everywhere, deeper behind the header and the
/// columns: fades of the UI's navy, no panel. The full board's columns are
/// dimmed from the chat column's edge to the window's; a compact board's
/// (`span`) about the board, fading out on both sides.
fn dim(ui: &mut MenuCanvas, frame: &Frame, viewport: [f32; 2], span: Option<[f32; 2]>) {
    let [width, height] = viewport;
    let navy = |alpha| color::alpha(color::SPACE, alpha);
    ui.accent_bar(Rect::new(0.0, 0.0, width, height), navy(0.32));
    let header = frame.point(0.0, 230.0)[1];
    fade(ui, Rect::new(0.0, 0.0, width, header), navy(0.6), navy(0.0));
    let [left, right] = span.unwrap_or([BOARD_LEFT, f32::INFINITY]);
    let [start, _] = frame.point(left - 170.0, 0.0);
    let [full, _] = frame.point(left - 30.0, 0.0);
    fade_across(
        ui,
        Rect::new(start, 0.0, full - start, height),
        navy(0.0),
        navy(BOARD_DIM),
    );
    if right.is_finite() {
        let [end, _] = frame.point(right + 30.0, 0.0);
        let [stop, _] = frame.point(right + 170.0, 0.0);
        ui.accent_bar(Rect::new(full, 0.0, end - full, height), navy(BOARD_DIM));
        fade_across(
            ui,
            Rect::new(end, 0.0, stop - end, height),
            navy(BOARD_DIM),
            navy(0.0),
        );
    } else {
        ui.accent_bar(Rect::new(full, 0.0, width - full, height), navy(BOARD_DIM));
    }
}

impl Painter<'_> {
    fn s(&self) -> f32 {
        self.frame.s
    }

    /// Width in frame pixels of `value` set at `size` frame pixels.
    fn width(&self, family: TextFamily, value: &str, size: f32, weight: FontWeight) -> f32 {
        self.measure.width(family, value, size * self.s(), weight) / self.s()
    }

    /// A run of `family` at `size` frame pixels, its letters centred on `y`,
    /// from `x` `width` wide, aligned `align`.
    #[allow(clippy::too_many_arguments)]
    fn run(
        &mut self,
        family: TextFamily,
        value: fmt::Arguments<'_>,
        x: f32,
        y: f32,
        width: f32,
        size: f32,
        colour: Color,
        weight: FontWeight,
        align: TextAlign,
    ) {
        let rect = self.frame.rect(x, y - size * 0.75, width, size * 1.5);
        text(
            self.ui,
            family,
            value,
            rect,
            size * self.s(),
            colour,
            weight,
            align,
        );
    }

    fn push(&mut self, command: DrawCommand) {
        let _ = self.ui.draw_list_mut().push(command);
    }

    /// SJK's emblem `side` across from `x`, centred on `middle`, gold for a player the
    /// hub's operator vouches for, then `count` ribbon bars of their medals
    /// (`identity_mark`); in the board's units.
    fn marks(
        &mut self,
        x: f32,
        middle: f32,
        side: f32,
        tag: crate::player_identity::Tag,
        count: usize,
    ) {
        let emblem = self.frame.rect(x, middle - side * 0.5, side, side);
        let list = self.ui.draw_list_mut();
        super::identity_mark::draw_marks(
            emblem.x,
            emblem.y + emblem.height * 0.5,
            emblem.width,
            tag,
            count,
            |command| {
                let _ = list.push(command);
            },
        );
    }

    /// The JoF emblem ([`crate::jof_tag`]) `side` tall with its ink from `x`, centred
    /// on `middle`, in the board's units.
    fn jof(&mut self, name: &str, x: f32, middle: f32, side: f32) {
        let Some(clan) = crate::jof_tag::clan(name) else {
            return;
        };
        let emblem = self.frame.rect(x, middle - side * 0.5, side, side);
        let list = self.ui.draw_list_mut();
        crate::jof_tag::draw(
            clan,
            emblem.x,
            emblem.y + emblem.height * 0.5,
            emblem.height,
            1.0,
            |command| {
                let _ = list.push(command);
            },
        );
    }

    fn is_local(&self, row: &ScoreRow) -> bool {
        u16::from(row.client_num) == self.header.local.client
    }

    /// Whether `row`'s player is ready, at intermission.
    fn is_ready(&self, row: &ScoreRow) -> bool {
        let local = self.header.local;
        local.intermission && row.client_num < 32 && local.ready & (1 << row.client_num) != 0
    }

    /// Where the lists stand ([`Board`]). The full board fills the frame from
    /// the chat column's edge to [`RIGHT`]; a compact one is as wide as its
    /// names need, centred on the screen, or as near it as the chat column
    /// allows.
    fn board(&self, rows: &[ScoreRow]) -> Board {
        let gametype = self.header.gametype;
        let duel = matches!(gametype, DUEL | POWER_DUEL);
        let teams = !duel && gametype >= TEAM;
        if !self.header.compact || rows.is_empty() {
            return Board {
                x: BOARD_LEFT,
                width: if teams {
                    (BOARD_WIDTH - GUTTER) * 0.5
                } else {
                    BOARD_WIDTH
                },
                centre: BOARD_LEFT + BOARD_WIDTH * 0.5,
                span: None,
            };
        }
        if duel {
            // The cards keep their size, about a middle as near the screen's
            // as the chat column allows; the players waiting stand under it.
            let reach = CARD_OFFSET + CARD_WIDTH;
            let centre = CENTRE.max(self.clear + reach).min(RIGHT - reach);
            let duelists = self.duelists(rows);
            let chosen = |row: &ScoreRow| {
                duelists
                    .iter()
                    .flatten()
                    .any(|(index, _)| rows[*index].client_num == row.client_num)
            };
            let (x, width) = self.compact_list(rows, &List::waiting(), |row| !chosen(row), centre);
            return Board {
                x,
                width,
                centre,
                span: Some([(centre - reach).min(x), (centre + reach).max(x + width)]),
            };
        }
        if teams {
            let list = List::team(matches!(gametype, CTF | CTY));
            let stat = self.stat_width(list.kind);
            // Both teams' names get the same room, so the two lists match.
            let need = [RED, BLUE]
                .into_iter()
                .map(|team| {
                    let member = |row: &ScoreRow| row.team == team;
                    let fit = self.fit(&list, rows.iter().filter(|row| member(row)).count());
                    self.names_need(rows, member, fit.height)
                })
                .fold(0.0, f32::max);
            let room = self.compact_room(need, &list, stat, 2);
            let width = Columns::compact_span(list.kind, list.numbered, room, stat);
            let whole = width * 2.0 + GUTTER;
            let x = self.place(CENTRE, whole);
            return Board {
                x,
                width,
                centre: x + whole * 0.5,
                span: Some([x, x + whole]),
            };
        }
        let list = List::players(rows.iter().any(|row| row.deaths.is_some()));
        let (x, width) = self.compact_list(rows, &list, |row| !watching(row), CENTRE);
        Board {
            x,
            width,
            centre: x + width * 0.5,
            span: Some([x, x + width]),
        }
    }

    /// A compact `list` of the rows `member` takes: its left edge and width
    /// (both halves, when it splits), centred on `centre` as far as the chat
    /// column and [`RIGHT`] allow.
    fn compact_list(
        &self,
        rows: &[ScoreRow],
        list: &List,
        member: impl Fn(&ScoreRow) -> bool,
        centre: f32,
    ) -> (f32, f32) {
        let fit = self.fit(list, rows.iter().filter(|row| member(row)).count());
        let stat = self.stat_width(list.kind);
        let need = self.names_need(rows, &member, fit.height);
        let room = self.compact_room(need, list, stat, fit.halves);
        let half = Columns::compact_span(list.kind, list.numbered, room, stat);
        let width = half * fit.halves as f32 + GUTTER * (fit.halves - 1) as f32;
        (self.place(centre, width), width)
    }

    /// The left edge of a compact board `width` wide centred on `centre`:
    /// moved right to clear the chat column, and back to end by [`RIGHT`].
    fn place(&self, centre: f32, width: f32) -> f32 {
        (centre - width * 0.5).max(self.clear).min(RIGHT - width)
    }

    /// A compact name column for names that need `need`: within
    /// [`COMPACT_NAME_LEAST`] and [`COMPACT_NAME_MOST`], and narrower when
    /// `across` such lists side by side would not stand between the chat
    /// column and [`RIGHT`], or be wider than the full board (down to
    /// [`COMPACT_NAME_SQUEEZED`]).
    fn compact_room(&self, need: f32, list: &List, stat: f32, across: usize) -> f32 {
        let across = across.max(1) as f32;
        let fixed = Columns::compact_span(list.kind, list.numbered, 0.0, stat);
        // Never wider than the full board, however far the chat column lies.
        let room = (RIGHT - self.clear).min(BOARD_WIDTH);
        let fits = (room - GUTTER * (across - 1.0)) / across - fixed;
        need.clamp(COMPACT_NAME_LEAST, COMPACT_NAME_MOST)
            .min(fits)
            .max(COMPACT_NAME_SQUEEZED)
    }

    /// A compact list's number columns' width: its widest label, measured as
    /// [`Self::labels`] draws it, and a gap.
    fn stat_width(&self, kind: Kind) -> f32 {
        let label = kind
            .stats()
            .iter()
            .map(|stat| {
                self.width(
                    TextFamily::Body,
                    stat.label(false),
                    15.0,
                    FontWeight::Regular,
                )
            })
            .fold(0.0, f32::max);
        (label + COMPACT_LABEL_GAP).max(COMPACT_STAT_LEAST)
    }

    /// The name column the rows `member` takes need in a compact list of
    /// rows `height` tall: the widest name, measured as [`Self::row`] draws
    /// it, with what follows it (the emblem and every bar, "Ready").
    fn names_need(
        &self,
        rows: &[ScoreRow],
        member: impl Fn(&ScoreRow) -> bool,
        height: f32,
    ) -> f32 {
        let (size, _) = text_sizes(true, height);
        let side = size * 1.05;
        rows.iter()
            .filter(|row| member(row))
            .map(|row| {
                let name = self.width(TextFamily::Body, &row.name, size, FontWeight::Regular)
                    + jof_room(&row.name, size);
                let marks = row.identity.map_or(0.0, |tag| {
                    let bars = super::identity_mark::ribbon_count(tag.medals, side, f32::MAX);
                    super::identity_mark::marks_width(side, bars) + 10.0
                });
                name + marks + if self.is_ready(row) { 60.0 } else { 0.0 }
            })
            .fold(0.0, f32::max)
            // A pixel to spare, so rounding never costs a bar.
            + 1.0
    }

    /// How `total` rows stand in `list`: [`ROW_MOST`] tall at most, thinner
    /// as it fills, in two halves when it may split and rows would be thinner
    /// than [`ROW_SPLIT`], never thinner than [`ROW_FLOOR`] (compact:
    /// [`COMPACT_MOST`] to [`COMPACT_FLOOR`], splitting below it).
    fn fit(&self, list: &List, total: usize) -> Fit {
        let (most, split_below, floor) = if self.header.compact {
            (COMPACT_MOST, COMPACT_FLOOR, COMPACT_FLOOR)
        } else {
            (ROW_MOST, ROW_SPLIT, ROW_FLOOR)
        };
        let height = list.bottom - list.top;
        let halves = if list.split && total > 0 && height / (total as f32) < split_below {
            2
        } else {
            1
        };
        let per = total.div_ceil(halves).max(1);
        let row_height = (height / per as f32).clamp(floor, most);
        Fit {
            halves,
            height: row_height,
            fits: ((height / row_height) as usize).max(1),
        }
    }

    /// The duelists' rows and known health: those the server names
    /// (`CS_CLIENT_DUELISTS`, power duel: the lone one, then the pair), else
    /// the first players not spectating.
    fn duelists(&self, rows: &[ScoreRow]) -> [Option<(usize, Option<i32>)>; 3] {
        let wanted = if self.header.gametype == POWER_DUEL {
            3
        } else {
            2
        };
        // The server lists the health of the first two in the order it lists
        // them.
        let duel = self.header.duel;
        let mut duelists: [Option<(usize, Option<i32>)>; 3] = [None; 3];
        let mut found = 0;
        for (slot, client) in duel.clients.into_iter().enumerate() {
            if let Some(index) =
                client.and_then(|client| rows.iter().position(|row| row.client_num == client))
                && found < wanted
            {
                duelists[found] = Some((index, duel.healths.get(slot).copied().flatten()));
                found += 1;
            }
        }
        if found == 0 {
            // Without the server's word, the players not spectating.
            for (index, _) in rows.iter().enumerate().filter(|(_, row)| !watching(row)) {
                if found == wanted {
                    break;
                }
                duelists[found] = Some((index, None));
                found += 1;
            }
        }
        duelists
    }

    /// The map, the mode and its limits on the left; the time and your place,
    /// or the teams' standing, on the right.
    fn header_line(&mut self, rows: &[ScoreRow]) {
        let header = self.header;
        let map = short_map(header.map);
        self.run(
            TextFamily::Display,
            format_args!("{map}"),
            LEFT,
            BAR_Y,
            560.0,
            48.0,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let after = LEFT + self.width(TextFamily::Display, map, 48.0, FontWeight::Semibold) + 24.0;
        self.run(
            TextFamily::Body,
            format_args!(
                "{}{}",
                mode_name(header.gametype),
                LimitWords(header.limits, header.gametype)
            ),
            after,
            BAR_Y + 3.0,
            (BOARD_LEFT + 420.0 - after).max(120.0),
            18.0,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        if let Some(killer) = header.killer {
            self.run(
                TextFamily::Body,
                format_args!("Killed by {killer}"),
                LEFT,
                BAR_Y + 46.0,
                BOARD_LEFT - LEFT - 40.0,
                17.0,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        let standing_left = self.standing(rows);
        let clock_left = self.clock(standing_left - 44.0);
        self.frame_rate(clock_left - 44.0);
    }

    /// The frame rate ("240 fps") as the clock reads, ending at `right` (Sol's request,
    /// 11/10/2026).
    fn frame_rate(&mut self, right: f32) {
        let Some(fps) = self.header.fps else {
            return;
        };
        let word = "fps";
        let word_width = self.width(TextFamily::Body, word, 17.0, FontWeight::Regular);
        self.run(
            TextFamily::Body,
            format_args!("{word}"),
            right - word_width - 4.0,
            BAR_Y + 3.0,
            word_width + 4.0,
            17.0,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::End,
        );
        let value = Short::of(format_args!("{fps}"));
        self.right_run(value.as_str(), right - word_width - 10.0, 32.0, color::TEXT);
    }

    /// Your place ("3rd of 14"), the teams' standing, or that you watch,
    /// right-aligned on the header line; returns where it starts.
    fn standing(&mut self, rows: &[ScoreRow]) -> f32 {
        let header = self.header;
        let local = header.local;
        if matches!(header.gametype, DUEL | POWER_DUEL) {
            return RIGHT;
        }
        if header.gametype >= TEAM {
            if header.gametype == SIEGE {
                return RIGHT;
            }
            let [red, blue] = header.team_scores;
            let line = match local.team {
                RED | BLUE => {
                    let (mine, theirs) = if local.team == RED {
                        (red, blue)
                    } else {
                        (blue, red)
                    };
                    if mine > theirs {
                        Short::of(format_args!("Your team leads by {}", mine - theirs))
                    } else if mine < theirs {
                        Short::of(format_args!("Your team trails by {}", theirs - mine))
                    } else {
                        Short::of(format_args!("Tied at {mine}"))
                    }
                }
                _ if red > blue => Short::of(format_args!("Red leads by {}", red - blue)),
                _ if blue > red => Short::of(format_args!("Blue leads by {}", blue - red)),
                _ => Short::of(format_args!("Tied at {red}")),
            };
            return self.right_run(line.as_str(), RIGHT, 34.0, color::TEXT);
        }
        if local.team == SPECTATOR {
            return self.right_run("Spectating", RIGHT, 30.0, color::MUTED);
        }
        let players = rows
            .iter()
            .filter(|row| !watching(row) && row.has_score)
            .count();
        let place = (local.rank & !RANK_TIED) + 1;
        let tied = local.rank & RANK_TIED != 0;
        let of = Short::of(format_args!(
            "of {players}{}",
            if tied { ", tied" } else { "" }
        ));
        let of_left = self.right_run(of.as_str(), RIGHT, 28.0, color::MUTED);
        let placed = Short::of(format_args!("{place}{}", ordinal(place)));
        self.right_run(placed.as_str(), of_left - 10.0, 60.0, color::GOLD_BRIGHT)
    }

    /// `value` in the display family, ending at `right` on the header line;
    /// returns where it starts.
    fn right_run(&mut self, value: &str, right: f32, size: f32, colour: Color) -> f32 {
        let width = self.width(TextFamily::Display, value, size, FontWeight::Semibold);
        self.run(
            TextFamily::Display,
            format_args!("{value}"),
            right - width - 8.0,
            BAR_Y,
            width + 8.0,
            size,
            colour,
            FontWeight::Semibold,
            TextAlign::End,
        );
        right - width
    }

    /// The time left in a timed match ("14:32 left"), or the time played,
    /// ending at `right`; returns where it starts (`right` without a clock).
    fn clock(&mut self, right: f32) -> f32 {
        let header = self.header;
        let Some(elapsed) = header.elapsed.map(|elapsed| elapsed.max(0)) else {
            return right;
        };
        let limit = header.limits.minutes.saturating_mul(60_000);
        let (time, word) = if limit > 0 && elapsed <= limit {
            (limit - elapsed, "left")
        } else {
            (elapsed, "played")
        };
        let word_width = self.width(TextFamily::Body, word, 17.0, FontWeight::Regular);
        self.run(
            TextFamily::Body,
            format_args!("{word}"),
            right - word_width - 4.0,
            BAR_Y + 3.0,
            word_width + 4.0,
            17.0,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::End,
        );
        let clock = Short::of(format_args!("{}:{:02}", time / 60_000, time / 1_000 % 60));
        self.right_run(clock.as_str(), right - word_width - 10.0, 32.0, color::TEXT)
    }

    /// Before the server's first scores.
    fn requesting(&mut self) {
        self.run(
            TextFamily::Body,
            format_args!("Asking the server for the scores..."),
            BOARD_LEFT,
            LIST_TOP + 30.0,
            BOARD_WIDTH,
            20.0,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    /// Everyone playing in one list, by place; two side by side once rows
    /// would be thinner than [`ROW_SPLIT`].
    fn free_for_all(&mut self, rows: &[ScoreRow], board: &Board) {
        let deaths = rows.iter().any(|row| row.deaths.is_some());
        let list = List::players(deaths).at(board.x, board.width);
        self.list(rows, &list, |row| !watching(row));
    }

    /// The two teams side by side, red on the left, each under its score.
    fn teams(&mut self, rows: &[ScoreRow], board: &Board) {
        let capture = matches!(self.header.gametype, CTF | CTY);
        let width = board.width;
        let [red, blue] = self.header.team_scores;
        for (team, x) in [(RED, board.x), (BLUE, board.x + width + GUTTER)] {
            let (score, other) = if team == RED {
                (red, blue)
            } else {
                (blue, red)
            };
            let members = rows.iter().filter(|row| row.team == team).count();
            self.team_head(team, x, width, score, score < other, members);
            let list = List::team(capture).at(x, width);
            self.list(rows, &list, |row| row.team == team);
        }
    }

    /// A team's head: its score large (muted while it trails), its name in
    /// its colour with how many play, and a thin rule in that colour.
    fn team_head(
        &mut self,
        team: u8,
        x: f32,
        width: f32,
        score: i32,
        trails: bool,
        members: usize,
    ) {
        let (name, tint) = if team == RED {
            ("Red team", RED_TEAM)
        } else {
            ("Blue team", BLUE_TEAM)
        };
        let y = 200.0;
        let value = Short::of(format_args!("{score}"));
        self.run(
            TextFamily::Display,
            format_args!("{}", value.as_str()),
            x,
            y,
            240.0,
            76.0,
            if trails { color::MUTED } else { color::TEXT },
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let after =
            x + self.width(
                TextFamily::Display,
                value.as_str(),
                76.0,
                FontWeight::Semibold,
            ) + 22.0;
        self.run(
            TextFamily::Display,
            format_args!("{name}"),
            after,
            y - 10.0,
            width - (after - x),
            26.0,
            tint,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        self.run(
            TextFamily::Body,
            format_args!(
                "{members} {}",
                if members == 1 { "player" } else { "players" }
            ),
            after,
            y + 18.0,
            width - (after - x),
            16.0,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        let rule = self.frame.rect(x, 246.0, width, 2.0);
        self.push(DrawCommand::SolidRect {
            rect: rule,
            color: color::alpha(tint, 0.6),
        });
    }

    /// The duelists as two facing cards (power duel: the lone duelist facing
    /// the pair), then the players waiting their turn.
    fn duel(&mut self, rows: &[ScoreRow], board: &Board) {
        let power = self.header.gametype == POWER_DUEL;
        let duelists = self.duelists(rows);
        let centre = board.centre;
        if duelists.iter().flatten().count() >= 2 {
            self.run(
                TextFamily::Display,
                format_args!("vs"),
                centre - 40.0,
                300.0,
                80.0,
                30.0,
                color::MUTED,
                FontWeight::Semibold,
                TextAlign::Center,
            );
        }
        if let Some((index, health)) = duelists[0] {
            self.card(
                &rows[index],
                centre - CARD_OFFSET,
                false,
                150.0,
                true,
                health,
            );
        }
        if power {
            for (slot, top) in [(1, 150.0), (2, 300.0)] {
                if let Some((index, health)) = duelists[slot] {
                    self.card(&rows[index], centre + CARD_OFFSET, true, top, false, health);
                }
            }
        } else if let Some((index, health)) = duelists[1] {
            self.card(
                &rows[index],
                centre + CARD_OFFSET,
                true,
                150.0,
                true,
                health,
            );
        }
        let list = List::waiting().at(board.x, board.width);
        let chosen = |row: &ScoreRow| {
            duelists
                .iter()
                .flatten()
                .any(|(index, _)| rows[*index].client_num == row.client_num)
        };
        self.list(rows, &list, |row| !chosen(row));
    }

    /// One duelist's card from `anchor` outwards (`right`: to the right of
    /// it, else ending at it): the name, the score large, the record and the
    /// health and shield known, over a rule, gold for you. `big` is a duel's
    /// card; a power duel's pair stack two smaller ones.
    fn card(
        &mut self,
        row: &ScoreRow,
        anchor: f32,
        right: bool,
        top: f32,
        big: bool,
        health: Option<i32>,
    ) {
        let width = CARD_WIDTH;
        let x = if right { anchor } else { anchor - width };
        let align = if right {
            TextAlign::Start
        } else {
            TextAlign::End
        };
        let local = self.is_local(row);
        let (name_size, score_size) = if big { (44.0, 112.0) } else { (32.0, 60.0) };
        let name_y = top + name_size * 0.6;
        let name_colour = if row.bot { color::MUTED } else { color::TEXT };
        // The emblem of a player the hub knows sits on the name's outer side,
        // in the room the name leaves for it.
        // The JoF emblem comes before the name, on its left.
        let room = width - 50.0;
        let jof = jof_room(&row.name, name_size);
        let text_width = self
            .width(
                TextFamily::Display,
                &row.name,
                name_size,
                FontWeight::Semibold,
            )
            .min(room - jof);
        self.run(
            TextFamily::Display,
            format_args!("{}", row.name),
            if right {
                x + jof
            } else {
                x + width - room + jof
            },
            name_y,
            room - jof,
            name_size,
            name_colour,
            FontWeight::Semibold,
            align,
        );
        if jof > 0.0 {
            let start = if right {
                x
            } else {
                x + width - text_width - jof
            };
            self.jof(&row.name, start, name_y, crate::jof_tag::side(name_size));
        }
        let name_width = text_width + jof;
        if let Some(tag) = row.identity {
            let side = name_size * 0.62;
            // The bars go where the name leaves room on the card.
            let count = super::identity_mark::ribbon_count(
                tag.medals,
                side,
                (width - name_width - 12.0).max(0.0),
            );
            let marks = super::identity_mark::marks_width(side, count);
            let at = if right {
                x + name_width + 12.0
            } else {
                x + width - name_width - 12.0 - marks
            };
            self.marks(at, name_y, side, tag, count);
        }
        let score_y = if big { top + 150.0 } else { top + 78.0 };
        self.run(
            TextFamily::Display,
            format_args!("{}", row.score),
            x,
            score_y,
            width,
            score_size,
            if local {
                color::GOLD_BRIGHT
            } else {
                color::TEXT
            },
            FontWeight::Semibold,
            align,
        );
        let vitals = if local {
            self.header.duel.local
        } else {
            health
                .filter(|health| *health >= 0)
                .map(|health| (health, -1))
        };
        let facts = Facts {
            record: row.record,
            vitals,
        };
        let facts_y = if big { top + 230.0 } else { top + 116.0 };
        self.run(
            TextFamily::Body,
            format_args!("{facts}"),
            x,
            facts_y,
            width,
            if big { 18.0 } else { 16.0 },
            color::MUTED,
            FontWeight::Regular,
            align,
        );
        let rule_y = if big { top + 262.0 } else { top + 132.0 };
        let rule = self.frame.rect(x, rule_y, width, 2.0);
        self.push(DrawCommand::SolidRect {
            rect: rule,
            color: if local {
                color::GOLD
            } else {
                color::alpha(color::HOLO, 0.3)
            },
        });
    }

    /// Spectators on one line under the lists, from their left edge, their
    /// names without colours.
    fn spectators(&mut self, rows: &[ScoreRow], board: &Board) {
        if !rows.iter().any(watching) {
            return;
        }
        let label = "Watching";
        let label_width = self.width(TextFamily::Display, label, 20.0, FontWeight::Semibold);
        self.run(
            TextFamily::Display,
            format_args!("{label}"),
            board.x,
            SPECTATORS_Y,
            label_width + 8.0,
            20.0,
            color::HOLO,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let x = board.x + label_width + 18.0;
        self.run(
            TextFamily::Body,
            format_args!("{}", Watching(rows)),
            x,
            SPECTATORS_Y + 1.0,
            RIGHT - x,
            17.0,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    /// The rows `member` takes from `rows`, in their order, as `list` says:
    /// column labels over a rule, then a row each, split in two when allowed
    /// and needed, and cut with "and n more" (your row kept) when they cannot
    /// fit at [`ROW_FLOOR`] ([`COMPACT_FLOOR`] when compact).
    fn list(&mut self, rows: &[ScoreRow], list: &List, member: impl Fn(&ScoreRow) -> bool) {
        let total = rows.iter().filter(|row| member(row)).count();
        let Fit {
            halves,
            height: row_height,
            fits,
        } = self.fit(list, total);
        let width = if halves == 2 {
            (list.width - GUTTER) * 0.5
        } else {
            list.width
        };
        // A compact list's number columns are as wide as their labels.
        let stat_width = self.header.compact.then(|| self.stat_width(list.kind));
        let capacity = fits * halves;
        let cut = total > capacity;
        let shown = if cut { capacity - 1 } else { total };
        let local_hidden = cut
            && rows
                .iter()
                .filter(|row| member(row))
                .position(|row| self.is_local(row))
                .is_some_and(|index| index >= shown);
        let ordinary = if local_hidden { shown - 1 } else { shown };
        let columns = |half: usize| {
            let x = list.x + half as f32 * (width + GUTTER);
            match stat_width {
                Some(stat_width) => {
                    Columns::compact(x, width, list.kind, list.numbered, stat_width)
                }
                None => Columns::new(x, width, list.kind, list.numbered),
            }
        };
        for half in 0..halves {
            self.labels(&columns(half), list.top, list.title);
        }
        let mut place = 0;
        let mut previous = None;
        let mut slot = 0;
        for (index, row) in rows.iter().filter(|row| member(row)).enumerate() {
            // Players waiting are numbered in turn; others by score, ties
            // sharing a place.
            if list.kind == Kind::Waiting || previous != Some(row.score) {
                place = index + 1;
                previous = Some(row.score);
            }
            let local = self.is_local(row);
            if index >= ordinary && !(local_hidden && local) {
                continue;
            }
            let (half, line) = (slot / fits, slot % fits);
            let target = list.top + line as f32 * row_height;
            let y = if halves == 1 {
                self.motion.row(row.client_num, target)
            } else {
                target
            };
            let numbered = (list.numbered && row.has_score).then_some(place);
            self.row(&columns(half), row, numbered, y, row_height);
            slot += 1;
        }
        if cut {
            let (half, line) = (slot / fits, slot % fits);
            let at = columns(half);
            let y = list.top + (line as f32 + 0.5) * row_height;
            self.run(
                TextFamily::Body,
                format_args!("and {} more", total - slot),
                at.name,
                y,
                at.width,
                16.0,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
    }

    /// A list's column labels over a thin rule, `top` being its first row's.
    fn labels(&mut self, columns: &Columns, top: f32, title: &str) {
        let y = top - 22.0;
        let label = |painter: &mut Self, value: &str, x: f32, width: f32, align: TextAlign| {
            painter.run(
                TextFamily::Body,
                format_args!("{value}"),
                x,
                y,
                width,
                15.0,
                color::MUTED,
                FontWeight::Regular,
                align,
            );
        };
        label(
            self,
            title,
            columns.name,
            columns.name_end - columns.name,
            TextAlign::Start,
        );
        for &(stat, right) in &columns.stats[..columns.count] {
            label(
                self,
                stat.label(columns.wide),
                right - columns.stat_width,
                columns.stat_width,
                TextAlign::End,
            );
        }
        label(
            self,
            "Ping",
            columns.bars,
            columns.ping - columns.bars,
            TextAlign::End,
        );
        let rule = self.frame.rect(columns.x, top - 6.0, columns.width, 1.0);
        self.push(DrawCommand::SolidRect {
            rect: rule,
            color: color::alpha(color::HOLO, 0.22),
        });
    }

    /// One player's row with its top at `y`, `height` tall.
    fn row(
        &mut self,
        columns: &Columns,
        row: &ScoreRow,
        place: Option<usize>,
        y: f32,
        height: f32,
    ) {
        let local = self.is_local(row);
        let middle = y + height * 0.5;
        if local {
            kit::band(
                self.ui,
                &self.frame,
                [
                    columns.x - 12.0,
                    y + 2.0,
                    columns.width + 24.0,
                    height - 4.0,
                ],
            );
        } else {
            // A hairline under each row, to read across the wide list.
            let rule = self
                .frame
                .rect(columns.x, y + height - 1.0, columns.width, 1.0);
            self.push(DrawCommand::SolidRect {
                rect: rule,
                color: color::alpha(color::HOLO, 0.06),
            });
        }
        let (name_size, number_size) = text_sizes(self.header.compact, height);
        let quiet = row.bot || !row.has_score;
        if let (Some(right), Some(place)) = (columns.place, place) {
            self.run(
                TextFamily::Display,
                format_args!("{place}"),
                right - 60.0,
                middle,
                60.0,
                number_size,
                if local {
                    color::GOLD_BRIGHT
                } else {
                    color::MUTED
                },
                FontWeight::Semibold,
                TextAlign::End,
            );
        }
        if let Some(x) = columns.flag {
            let carried = if row.powerups & (1 << 6) != 0 {
                self.flags.neutral
            } else if row.powerups & (1 << 4) != 0 {
                self.flags.red
            } else if row.powerups & (1 << 5) != 0 {
                self.flags.blue
            } else {
                None
            };
            if let Some(texture) = carried {
                let side = (height - 12.0).min(24.0);
                let rect = self.frame.rect(x, middle - side * 0.5, side, side);
                self.push(DrawCommand::TexturedQuad {
                    rect,
                    texture,
                    color: Color::new(1.0, 1.0, 1.0, 1.0),
                });
            }
        }
        // The name, then the emblem of a player the hub knows and, at
        // intermission, whether they are ready.
        let room = columns.name_end - columns.name;
        let side = name_size * 1.05;
        let ready = self.is_ready(row);
        let ready_width = if ready { 60.0 } else { 0.0 };
        // The JoF emblem first, on the name's left.
        let jof = jof_room(&row.name, name_size);
        let full_name =
            self.width(TextFamily::Body, &row.name, name_size, FontWeight::Regular) + jof;
        // The bars take no more than a third of the name's room; a compact
        // list, sized for its names, gives them what the name leaves.
        let ribbons = row.identity.map_or(0, |tag| {
            let left = if self.header.compact {
                room - ready_width - full_name - 10.0
            } else {
                0.0
            };
            super::identity_mark::ribbon_count(tag.medals, side, (room / 3.0).max(left))
        });
        let marks = if row.identity.is_some() {
            super::identity_mark::marks_width(side, ribbons) + 10.0
        } else {
            0.0
        } + ready_width;
        let name_width = full_name.min((room - marks).max(0.0));
        if jof > 0.0 {
            self.jof(
                &row.name,
                columns.name,
                middle,
                crate::jof_tag::side(name_size),
            );
        }
        self.run(
            TextFamily::Body,
            format_args!("{}", row.name),
            columns.name + jof,
            middle,
            (room - marks - jof).max(0.0),
            name_size,
            if quiet {
                color::alpha(color::MUTED, 0.85)
            } else {
                color::TEXT
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        let mut after = columns.name + name_width + 10.0;
        if let Some(tag) = row.identity {
            self.marks(after, middle, side, tag, ribbons);
            after += super::identity_mark::marks_width(side, ribbons) + 10.0;
        }
        if ready {
            self.run(
                TextFamily::Body,
                format_args!("Ready"),
                after,
                middle,
                60.0,
                14.0,
                color::GOLD,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        if row.has_score && row.ping >= 0 {
            for &(stat, right) in &columns.stats[..columns.count] {
                let first = stat == Stat::Score || stat == Stat::Record;
                let colour = match (first, local) {
                    (true, true) => color::GOLD_BRIGHT,
                    (true, false) if !quiet => color::TEXT,
                    _ => color::MUTED,
                };
                self.run(
                    TextFamily::Display,
                    format_args!("{}", StatText(stat, row)),
                    right - columns.stat_width,
                    middle,
                    columns.stat_width,
                    if first {
                        number_size
                    } else {
                        number_size * 0.86
                    },
                    colour,
                    FontWeight::Semibold,
                    TextAlign::End,
                );
            }
        }
        let status = if !row.has_score {
            Some("Joining")
        } else if row.ping < 0 {
            Some("Connecting")
        } else if row.bot {
            Some("Bot")
        } else {
            None
        };
        match status {
            Some(status) => self.run(
                TextFamily::Body,
                format_args!("{status}"),
                columns.ping - 120.0,
                middle,
                120.0,
                15.0,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::End,
            ),
            None => {
                let ping = u32::try_from(row.ping).unwrap_or(0);
                crate::menu::sjk::browser::signal(
                    self.ui,
                    &self.frame,
                    columns.bars,
                    middle,
                    ping,
                    local,
                );
                self.run(
                    TextFamily::Display,
                    format_args!("{ping}"),
                    columns.bars + 30.0,
                    middle,
                    columns.ping - columns.bars - 30.0,
                    number_size * 0.86,
                    if local {
                        color::GOLD_BRIGHT
                    } else {
                        color::MUTED
                    },
                    FontWeight::Semibold,
                    TextAlign::End,
                );
            }
        }
    }
}

/// What a list shows and where.
struct List {
    x: f32,
    width: f32,
    top: f32,
    bottom: f32,
    kind: Kind,
    numbered: bool,
    /// The rows may stand in two lists side by side.
    split: bool,
    /// The name column's label.
    title: &'static str,
}

impl List {
    /// Free-for-all: everyone playing, by place.
    fn players(deaths: bool) -> Self {
        Self {
            x: BOARD_LEFT,
            width: BOARD_WIDTH,
            top: LIST_TOP,
            bottom: BOTTOM,
            kind: Kind::Free { deaths },
            numbered: true,
            split: true,
            title: "Player",
        }
    }

    /// A team, under its head.
    fn team(capture: bool) -> Self {
        Self {
            x: BOARD_LEFT,
            width: (BOARD_WIDTH - GUTTER) * 0.5,
            top: TEAM_TOP,
            bottom: BOTTOM,
            kind: Kind::Team { capture },
            numbered: false,
            split: false,
            title: "Player",
        }
    }

    /// The players waiting their duel, under the cards.
    fn waiting() -> Self {
        Self {
            x: BOARD_LEFT,
            width: BOARD_WIDTH,
            top: 530.0,
            bottom: BOTTOM,
            kind: Kind::Waiting,
            numbered: true,
            split: true,
            title: "Waiting to duel",
        }
    }

    /// The same list from `x`, `width` wide.
    fn at(self, x: f32, width: f32) -> Self {
        Self { x, width, ..self }
    }
}

/// A duelist's record and vitals in words: "3 wins · 1 loss · 87 health ·
/// 25 shield"; a shield of -1 is unknown.
struct Facts {
    record: Option<(i32, i32)>,
    vitals: Option<(i32, i32)>,
}

impl fmt::Display for Facts {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        let mut part = |formatter: &mut fmt::Formatter<'_>, args: fmt::Arguments<'_>| {
            if !std::mem::take(&mut first) {
                formatter.write_str("  ·  ")?;
            }
            formatter.write_fmt(args)
        };
        if let Some((wins, losses)) = self.record {
            part(
                formatter,
                format_args!("{wins} {}", if wins == 1 { "win" } else { "wins" }),
            )?;
            part(
                formatter,
                format_args!("{losses} {}", if losses == 1 { "loss" } else { "losses" }),
            )?;
        }
        if let Some((health, shield)) = self.vitals {
            part(formatter, format_args!("{health} health"))?;
            if shield >= 0 {
                part(formatter, format_args!("{shield} shield"))?;
            }
        }
        Ok(())
    }
}

/// The room the JoF emblem takes before `name` set at `size` ([`crate::jof_tag`]), or
/// 0 when the name has no JoF tag.
fn jof_room(name: &str, size: f32) -> f32 {
    crate::jof_tag::room_for(name, crate::jof_tag::side(size))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(client: u8, team: u8, score: i32) -> ScoreRow {
        ScoreRow {
            client_num: client,
            name: format!("^{}Player{client}", client % 8),
            team,
            score,
            deaths: None,
            ping: 20 + i32::from(client) * 9,
            time: 3 + i32::from(client),
            has_score: true,
            bot: client % 5 == 4,
            record: Some((i32::from(client % 4), i32::from(client % 3))),
            powerups: 0,
            defends: 1,
            assists: 2,
            captures: 3,
            identity: (client % 6 == 1).then_some(crate::player_identity::Tag {
                verified: client % 12 == 1,
                ..Default::default()
            }),
        }
    }

    fn header(gametype: i32, local: u16) -> SjkHeader<'static> {
        SjkHeader {
            map: "mp/duel6",
            gametype,
            limits: Limits {
                frags: 30,
                captures: 8,
                duel_wins: 5,
                minutes: 20,
            },
            elapsed: Some(5 * 60_000 + 28_000),
            team_scores: [5, 3],
            local: LocalStatus {
                client: local,
                team: 0,
                rank: 2,
                score: 18,
                ready: 0,
                intermission: false,
            },
            killer: None,
            duel: Duelists::default(),
            compact: false,
            fps: None,
        }
    }

    fn canvas() -> MenuCanvas {
        MenuCanvas::with_capacities(super::super::TEXT_SLOTS, 96, super::super::DRAWS)
    }

    fn draw(rows: &[ScoreRow], header: &SjkHeader<'_>, viewport: [f32; 2]) -> MenuCanvas {
        let mut ui = canvas();
        let mut motion = Motion::default();
        motion.present(true, true, std::time::Instant::now());
        build(
            &mut ui,
            rows,
            header,
            FlagIcons::default(),
            Measure::default(),
            &mut motion,
            viewport,
        );
        ui.finish(u16::MAX);
        ui
    }

    fn texts(ui: &MenuCanvas) -> usize {
        ui.draw_list()
            .commands()
            .iter()
            .filter(|command| matches!(command, DrawCommand::Text { .. }))
            .count()
    }

    /// Sol's request (11/10/2026): the frame rate stands on the header line left of the
    /// clock, its number in the clock's type with "fps" after it, clear of the mode on
    /// the left; without a reading (or with `cg_scoreboardFps 0`) nothing shows.
    #[test]
    fn the_header_shows_the_frame_rate_left_of_the_clock() {
        let rows: Vec<ScoreRow> = (0..8)
            .map(|client| row(client, 0, 30 - client as i32))
            .collect();
        let runs = |ui: &MenuCanvas| -> Vec<(String, Rect, f32)> {
            ui.draw_list()
                .commands()
                .iter()
                .filter_map(|command| match *command {
                    DrawCommand::Text {
                        rect, text, size, ..
                    } => Some((ui.stored_text(text).to_owned(), rect, size)),
                    _ => None,
                })
                .collect()
        };
        for viewport in VIEWPORTS {
            let mut shown = header(0, 0);
            shown.fps = Some(240);
            let ui = draw(&rows, &shown, viewport);
            let texts = runs(&ui);
            let find = |text: &str| {
                texts
                    .iter()
                    .find(|(drawn, ..)| drawn == text)
                    .unwrap_or_else(|| panic!("{text:?} not in {texts:?}"))
                    .clone()
            };
            let (_, number, number_size) = find("240");
            let (_, word, _) = find("fps");
            let (_, clock, clock_size) = find("14:32");
            let (_, mode, _) = find("Free for all · 30 frags · 20 minutes");
            assert_eq!(number_size, clock_size, "{viewport:?}: the clock's type");
            assert!(number.right() <= word.x + 1.0, "{viewport:?}");
            assert!(word.right() < clock.x, "{viewport:?}: {word:?} {clock:?}");
            assert!(mode.right() < number.x, "{viewport:?}: {mode:?} {number:?}");
            let without = draw(&rows, &header(0, 0), viewport);
            assert!(
                runs(&without).iter().all(|(text, ..)| text != "fps"),
                "{viewport:?}"
            );
        }
    }

    /// 16:9 at 1080 lines and 4K, 5:4, 4:3, 21:9 and 32:9.
    const VIEWPORTS: [[f32; 2]; 8] = [
        [1920.0, 1080.0],
        [3840.0, 2160.0],
        [1280.0, 1024.0],
        [1440.0, 1080.0],
        [2560.0, 1080.0],
        [3440.0, 1440.0],
        [3840.0, 1080.0],
        [5120.0, 1440.0],
    ];

    /// The test matches: name, rows, the match's facts.
    fn matches() -> Vec<(&'static str, Vec<ScoreRow>, SjkHeader<'static>)> {
        let free = |players: u8| -> Vec<ScoreRow> {
            (0..players)
                .map(|client| {
                    let mut row = row(client, 0, 40 - i32::from(client));
                    row.deaths = Some(3);
                    row
                })
                .collect()
        };
        let mut ready = header(CTF, 3);
        ready.local.intermission = true;
        ready.local.ready = u32::MAX;
        vec![
            ("free for all of 8", free(8), header(0, 3)),
            ("free for all of 32", free(32), header(0, 7)),
            (
                "team FFA of 8 a side",
                (0..16)
                    .map(|client| row(client, 1 + client % 2, 10 - i32::from(client / 2)))
                    .collect(),
                header(TEAM, 3),
            ),
            (
                "capture the flag of 16 a side, everyone ready",
                (0..32)
                    .map(|client| row(client, 1 + client % 2, 10 - i32::from(client / 2)))
                    .collect(),
                ready,
            ),
            (
                "duel with thirty waiting",
                (0..32)
                    .map(|client| row(client, if client < 2 { 0 } else { SPECTATOR }, 1))
                    .collect(),
                header(DUEL, 0),
            ),
        ]
    }

    /// Each list's thin rule under its labels, which spans its columns
    /// (window pixels).
    fn list_rules(ui: &MenuCanvas) -> Vec<Rect> {
        ui.draw_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::SolidRect { rect, color }
                    if *color == color::alpha(color::HOLO, 0.22) =>
                {
                    Some(*rect)
                }
                _ => None,
            })
            .collect()
    }

    /// The lists' left and right edges (window pixels).
    fn edges(ui: &MenuCanvas) -> [f32; 2] {
        list_rules(ui)
            .iter()
            .fold([f32::MAX, f32::MIN], |[left, right], rule| {
                [left.min(rule.x), right.max(rule.x + rule.width)]
            })
    }

    /// The rectangle of the first text run reading `value`.
    fn text_rect(ui: &MenuCanvas, value: &str) -> Option<Rect> {
        let rects = ui
            .draw_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { rect, .. } => Some(*rect),
                _ => None,
            });
        rects
            .zip(ui.text_runs())
            .find(|(_, run)| *run == value)
            .map(|(rect, _)| rect)
    }

    #[test]
    fn a_full_server_fits_the_canvas_in_every_mode() {
        for viewport in VIEWPORTS {
            for compact in [false, true] {
                let with = |mut header: SjkHeader<'static>| {
                    header.compact = compact;
                    header
                };
                // 32 playing, with deaths counted: two lists side by side
                // (compact: one).
                let mut free: Vec<_> = (0..32)
                    .map(|client| row(client, 0, 40 - i32::from(client)))
                    .collect();
                for row in &mut free {
                    row.deaths = Some(3);
                }
                let ui = draw(&free, &with(header(0, 7)), viewport);
                assert!(!ui.overflowed(), "free for all at {viewport:?}");
                assert!(texts(&ui) > 32 * 4, "every row drew");
                // Capture the flag, 16 a side, at intermission with everyone ready.
                let ctf: Vec<_> = (0..32)
                    .map(|client| row(client, 1 + client % 2, 10 - i32::from(client / 2)))
                    .collect();
                let mut ready = with(header(CTF, 3));
                ready.local.intermission = true;
                ready.local.ready = u32::MAX;
                let ui = draw(&ctf, &ready, viewport);
                assert!(!ui.overflowed(), "capture the flag at {viewport:?}");
                // Everyone on one team: cut, with "and n more" (compact: not).
                let one: Vec<_> = (0..32).map(|client| row(client, 2, 0)).collect();
                let ui = draw(&one, &with(header(TEAM, 31)), viewport);
                assert!(!ui.overflowed(), "one team at {viewport:?}");
                // A duel with thirty waiting.
                let duel: Vec<_> = (0..32)
                    .map(|client| row(client, if client < 2 { 0 } else { SPECTATOR }, 1))
                    .collect();
                let ui = draw(&duel, &with(header(DUEL, 0)), viewport);
                assert!(!ui.overflowed(), "duel at {viewport:?}");
                // Thirty-two watching.
                let watching: Vec<_> = (0..32).map(|client| row(client, SPECTATOR, 0)).collect();
                let ui = draw(&watching, &with(header(0, 0)), viewport);
                assert!(!ui.overflowed(), "spectators at {viewport:?}");
            }
        }
    }

    /// A compact board stands between the chat column and the header's right
    /// end at every aspect, centred on the screen when that clears the chat
    /// column and else just clear of it, as wide as its names need: much
    /// narrower than the full board, never absurdly narrow.
    #[test]
    fn a_compact_board_is_centred_clear_of_the_chat_column() {
        for viewport in VIEWPORTS {
            let frame = Frame::new(viewport);
            let chat = super::super::layout::Layout::new(viewport);
            let clear = chat.chat_left + chat.chat_width + CHAT_CLEARANCE * frame.s;
            let [right_end, _] = frame.point(RIGHT, 0.0);
            let middle = viewport[0] * 0.5;
            for (name, rows, mut header) in matches() {
                header.compact = true;
                let ui = draw(&rows, &header, viewport);
                assert!(!ui.overflowed(), "{name} at {viewport:?}");
                let [left, right] = edges(&ui);
                let width = (right - left) / frame.s;
                assert!(
                    left >= clear - 0.5,
                    "{name} at {viewport:?}: {left} < {clear}"
                );
                assert!(right <= right_end + 0.5, "{name} at {viewport:?}: {right}");
                assert!(right <= viewport[0], "{name} at {viewport:?}");
                assert!(
                    (400.0..=BOARD_WIDTH + 0.5).contains(&width),
                    "{name} at {viewport:?}: {width}"
                );
                if header.gametype == DUEL {
                    // The cards and the players waiting share a middle, the
                    // screen's when the cards clear the chat column.
                    let vs = text_rect(&ui, "vs").expect("vs");
                    let centre = vs.x + vs.width * 0.5;
                    let reach = (CARD_OFFSET + CARD_WIDTH) * frame.s;
                    let expected = middle.max(clear + reach);
                    assert!((centre - expected).abs() < 0.5, "{name} at {viewport:?}");
                    assert!(((left + right) * 0.5 - centre).abs() < 0.5, "{name}");
                    continue;
                }
                let centred = middle - (right - left) * 0.5;
                let expected = centred.max(clear);
                assert!(
                    (left - expected).abs() < 0.5,
                    "{name} at {viewport:?}: {left}, expected {expected}"
                );
            }
        }
        // A free-for-all is centred even at 1080 lines' 16:9, where the chat
        // column comes closest; a team game hugs the chat column there.
        let ffa_and_team = matches()
            .into_iter()
            .enumerate()
            .filter(|(index, _)| matches!(index, 0 | 2));
        for (_, (_, rows, mut header)) in ffa_and_team {
            header.compact = true;
            let ui = draw(&rows, &header, [1920.0, 1080.0]);
            let [left, right] = edges(&ui);
            if header.gametype == TEAM {
                assert!((left - BOARD_LEFT).abs() < 0.5, "{left}");
                assert!(right < RIGHT - 200.0, "{right}");
            } else {
                assert!(
                    ((left + right) * 0.5 - CENTRE).abs() < 0.5,
                    "{left} {right}"
                );
                assert!(right - left < 560.0, "{left} {right}");
            }
        }
    }

    /// Without compact rows the board keeps its place and width: from the
    /// chat column's edge to the header's right end.
    #[test]
    fn the_full_board_keeps_its_place() {
        for (name, rows, header) in matches() {
            let ui = draw(&rows, &header, [1920.0, 1080.0]);
            let [left, right] = edges(&ui);
            assert!((left - BOARD_LEFT).abs() < 0.5, "{name}: {left}");
            assert!((right - RIGHT).abs() < 0.5, "{name}: {right}");
            if header.gametype == DUEL {
                let vs = text_rect(&ui, "vs").expect("vs");
                let centre = vs.x + vs.width * 0.5;
                assert!((centre - (BOARD_LEFT + BOARD_WIDTH * 0.5)).abs() < 0.5);
            }
        }
    }

    /// A compact name column is as wide as the longest name with its marks,
    /// within its bounds: a long name ends in an ellipsis at the most, and
    /// a player with medals gets every bar.
    #[test]
    fn a_compact_name_column_follows_its_names() {
        let viewport = [1920.0, 1080.0];
        let mut ffa = header(0, 0);
        ffa.compact = true;
        let width_with = |rows: &[ScoreRow]| {
            let [left, right] = edges(&draw(rows, &ffa, viewport));
            right - left
        };
        let short: Vec<_> = (0..8).map(|client| row(client, 0, 9)).collect();
        let least = width_with(&short);
        let mut long = short.clone();
        long[4].name = "^1Mmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmm".to_owned();
        let most = width_with(&long);
        let mut middling = short.clone();
        // 20 letters at 20 pixels: wider than the least, short of the most.
        middling[4].name = "^5Padawan^7Rexxxxxxxxxxx".to_owned();
        let between = width_with(&middling);
        assert!(
            least < between && between < most,
            "{least} {between} {most}"
        );
        assert!(
            (most - least - (COMPACT_NAME_MOST - COMPACT_NAME_LEAST)).abs() < 0.5,
            "{least} {most}"
        );
        // The longest name ends at the column's end, in an ellipsis, short
        // of the first score's digits.
        let ui = draw(&long, &ffa, viewport);
        let name = text_rect(&ui, &long[4].name).expect("the long name");
        let score = text_rect(&ui, "9").expect("a score");
        assert!(name.x + name.width + 40.0 <= score.x + score.width);
        // Three medals: the emblem and three bars (the full board's rows of
        // the same room would show none), the column made wide enough.
        let mut decorated = short.clone();
        decorated[2].identity = Some(crate::player_identity::Tag {
            verified: true,
            medals: three_medals(),
        });
        let ui = draw(&decorated, &ffa, viewport);
        let side = 20.0 * 1.05;
        let (bar_width, bar_height, _) = super::super::identity_mark::ribbon_size(side);
        let bars = ui
            .draw_list()
            .commands()
            .iter()
            .filter(|command| {
                matches!(command, DrawCommand::SolidRect { rect, .. }
                    if (rect.width - bar_width).abs() < 0.01
                        && (rect.height - bar_height).abs() < 0.01)
            })
            .count();
        assert!(bars >= 3, "{bars} bars");
    }

    fn three_medals() -> crate::medals::Medals {
        let list: Vec<sjk_identity::Medal> = ["early_tester", "early_contributor", "bug_hunter"]
            .iter()
            .map(|id| sjk_identity::Medal {
                id: (*id).to_owned(),
                count: 1,
                awarded: 0,
                note: String::new(),
            })
            .collect();
        crate::medals::Medals::from_wire(&list)
    }

    /// Compact columns from [`Columns::compact_span`] give the names exactly
    /// their room, the numbers right after them and the ping at the end.
    #[test]
    fn compact_columns_pack_the_numbers_after_the_names() {
        for kind in [
            Kind::Free { deaths: true },
            Kind::Free { deaths: false },
            Kind::Team { capture: true },
            Kind::Team { capture: false },
            Kind::Waiting,
        ] {
            let numbered = !matches!(kind, Kind::Team { .. });
            for room in [COMPACT_NAME_SQUEEZED, COMPACT_NAME_LEAST, COMPACT_NAME_MOST] {
                for stat in [COMPACT_STAT_LEAST, 72.8] {
                    let width = Columns::compact_span(kind, numbered, room, stat);
                    let columns = Columns::compact(500.0, width, kind, numbered, stat);
                    assert!((columns.name_end - columns.name - room).abs() < 1e-3);
                    assert!(columns.name_end + 40.0 <= columns.stats[0].1, "{kind:?}");
                    assert!(columns.stats[columns.count - 1].1 <= columns.bars);
                    assert!((columns.ping - (500.0 + width)).abs() < 1e-3);
                    assert!(!columns.wide);
                }
            }
        }
    }

    /// Compact, a full free-for-all stays in one column and a team of 32
    /// shows everyone; without it the list splits and the team is cut.
    #[test]
    fn compact_keeps_a_full_server_in_one_column() {
        let free: Vec<_> = (0..32)
            .map(|client| row(client, 0, 40 - i32::from(client)))
            .collect();
        let one: Vec<_> = (0..32).map(|client| row(client, 2, 0)).collect();
        for viewport in VIEWPORTS {
            for compact in [true, false] {
                let mut ffa = header(0, 7);
                ffa.compact = compact;
                let ui = draw(&free, &ffa, viewport);
                assert!(!ui.overflowed(), "free for all at {viewport:?}");
                let columns = ui.text_runs().filter(|run| *run == "Player").count();
                assert_eq!(columns, if compact { 1 } else { 2 }, "{viewport:?}");
                let mut team = header(TEAM, 31);
                team.compact = compact;
                let ui = draw(&one, &team, viewport);
                assert!(!ui.overflowed(), "one team at {viewport:?}");
                let cut = ui.text_runs().any(|run| run.starts_with("and "));
                assert_eq!(cut, !compact, "{viewport:?}");
            }
        }
    }

    #[test]
    fn a_cut_list_keeps_your_row() {
        let rows: Vec<_> = (0..32).map(|client| row(client, 2, 0)).collect();
        let ui = draw(&rows, &header(TEAM, 31), [1920.0, 1080.0]);
        // Your row's band: the gold bar of the kit's band.
        let gold = ui.draw_list().commands().iter().any(|command| {
            matches!(command, DrawCommand::RoundedRect { color, .. } if *color == color::GOLD_BRIGHT)
        });
        assert!(gold, "your row is drawn on its band");
    }

    #[test]
    fn places_ordinals_and_words() {
        let suffixes: Vec<_> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 101, 111]
            .map(ordinal)
            .to_vec();
        assert_eq!(
            suffixes,
            [
                "st", "nd", "rd", "th", "th", "th", "th", "st", "nd", "rd", "st", "th"
            ]
        );
        assert_eq!(short_map("mp/duel6"), "duel6");
        assert_eq!(short_map("MP/ffa3"), "ffa3");
        assert_eq!(short_map("academy1"), "academy1");
        let limits = Limits {
            frags: 30,
            captures: 8,
            duel_wins: 3,
            minutes: 20,
        };
        assert_eq!(
            LimitWords(limits, 0).to_string(),
            " · 30 frags · 20 minutes"
        );
        assert_eq!(
            LimitWords(limits, CTF).to_string(),
            " · 8 captures · 20 minutes"
        );
        assert_eq!(
            LimitWords(limits, DUEL).to_string(),
            " · 30 frags · 3 duel wins · 20 minutes"
        );
        assert_eq!(LimitWords(Limits::default(), 0).to_string(), "");
        let facts = Facts {
            record: Some((1, 2)),
            vitals: Some((87, -1)),
        };
        assert_eq!(facts.to_string(), "1 win  ·  2 losses  ·  87 health");
        assert_eq!(visible_chars("^1Sol^7Fox"), 6);
    }

    #[test]
    fn names_end_before_the_numbers_in_every_list() {
        for kind in [
            Kind::Free { deaths: true },
            Kind::Free { deaths: false },
            Kind::Team { capture: true },
            Kind::Team { capture: false },
            Kind::Waiting,
        ] {
            let numbered = !matches!(kind, Kind::Team { .. });
            for width in [BOARD_WIDTH, (BOARD_WIDTH - GUTTER) * 0.5] {
                let columns = Columns::new(BOARD_LEFT, width, kind, numbered);
                assert!(columns.name_end - columns.name >= 160.0, "{kind:?} {width}");
                // The name stops before the first number's last digits.
                assert!(columns.name_end + 40.0 <= columns.stats[0].1, "{kind:?}");
                assert!(
                    columns.stats[columns.count - 1].1 <= columns.bars,
                    "{kind:?}"
                );
                assert!((columns.ping - (BOARD_LEFT + width)).abs() < 1e-3);
            }
        }
    }

    /// The duelists named by the server face each other with their health,
    /// matched by the server's order, and everyone else waits in turn.
    #[test]
    fn duelists_face_each_other_and_the_rest_wait() {
        let rows: Vec<_> = (0..6)
            .map(|client| row(client, if client < 2 { 0 } else { SPECTATOR }, 1))
            .collect();
        let mut duel = header(DUEL, 1);
        // The server lists 1 then 0; 0's health is known.
        duel.duel.clients = [Some(1), Some(0), None];
        duel.duel.healths = [None, Some(64)];
        duel.duel.local = Some((87, 25));
        let ui = draw(&rows, &duel, [1920.0, 1080.0]);
        let runs: Vec<_> = ui.text_runs().collect();
        assert!(runs.contains(&"vs"), "{runs:?}");
        assert!(
            runs.iter()
                .any(|run| run.ends_with("87 health  ·  25 shield")),
            "your card: {runs:?}"
        );
        assert!(
            runs.iter().any(|run| run.ends_with("64 health")),
            "the opponent's: {runs:?}"
        );
        assert!(runs.contains(&"Waiting to duel"), "{runs:?}");
        // The four waiting are numbered in turn though their scores tie.
        for place in ["1", "2", "3", "4"] {
            assert!(runs.contains(&place), "{place}: {runs:?}");
        }
    }

    /// Ties share a place: 1, 2, 2, 4.
    #[test]
    fn tied_scores_share_a_place() {
        let rows = [row(0, 0, 9), row(1, 0, 5), row(2, 0, 5), row(3, 0, 1)];
        let ui = draw(&rows, &header(0, 3), [1920.0, 1080.0]);
        // Each row starts with its place, then its name.
        let runs: Vec<_> = ui.text_runs().collect();
        let places: Vec<_> = runs
            .windows(2)
            .filter(|pair| pair[1].starts_with('^'))
            .map(|pair| pair[0])
            .collect();
        assert_eq!(places, ["1", "2", "2", "4"], "{runs:?}");
        // The header: the map without `mp/`, your place gold, of how many.
        assert!(runs.contains(&"duel6"), "{runs:?}");
        assert!(runs.contains(&"3rd") && runs.contains(&"of 4"), "{runs:?}");
        assert!(runs.contains(&"14:32"), "{runs:?}");
    }
}
