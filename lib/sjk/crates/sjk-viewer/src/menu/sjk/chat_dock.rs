//! The SJK chat docked on a page (`docs/hub-chat.md`, "In the menus"): under the
//! servers on the main page ([`super::home`]) and under the match on the in-game menu
//! (`ingame_menu/sjk_view.rs`), drawn by one piece of code for both. Its name and who
//! is online, the last messages in a box of fixed height (each wrapped onto as many
//! rows as it needs, newest at the bottom), a field to type in and Open chat for the
//! page. Resting the pointer on a name shows the sender's card with Mute.
//!
//! [`DockCache`] reads the identity service only when the chat or the mute list
//! changed: its last lines, who is online and why it cannot send or read. Muted
//! senders' lines are left out. [`Dock`] is what a page keeps between frames: what is
//! being typed, the sender card on show and which messages the last frame showed.

use crate::chat_gifs::draw as gif_draw;
use crate::menu::sjk::{Frame, color, text};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use crate::sjk_chat_look::{self, Measure};
use crate::text::TextFace;
use crate::{player_identity, player_mutes};
use sjk_ui::{DrawCommand, FontWeight, Rect, TextAlign};
use winit::keyboard::KeyCode;

/// Messages the dock keeps: as many as its box shows, each taking a row at least.
pub(crate) const LINES: usize = 5;

/// The messages' box under the dock's name, in frame pixels: its offset from the
/// dock's top, the height of a message on one row at the plain text style, the pitch
/// of a long message's further rows (closer, so a message reads as one), and the box's
/// height (five one-row messages). A larger text style (`ui_textScale`) makes the rows
/// taller, so fewer fit.
const BOX_TOP: f32 = 40.0;
const ROW: f32 = 26.0;
const WRAP: f32 = 20.0;
const BOX: f32 = LINES as f32 * ROW;
/// The messages' text size.
const TEXT_SIZE: f32 = 15.0;
/// How far the field reaches left of the dock's text.
const FIELD_OVERHANG: f32 = 14.0;
/// How the dock shows a message's GIF: smaller than the page.
const GIF: gif_draw::Look = gif_draw::DOCK;

/// The room a message's GIF takes under it (none without one).
fn gif_room(gif: Option<crate::chat_gifs::Shown>) -> f32 {
    gif.map_or(0.0, |shown| gif_draw::room(GIF, shown))
}

/// The SJK chat as the dock shows it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ChatDock<'a> {
    /// The last messages, oldest first; the dock shows the last that fit.
    pub(crate) lines: &'a [DockLine<'a>],
    /// Keys reading the chat lately.
    pub(crate) online: u32,
    /// The hub answered the last poll.
    pub(crate) live: bool,
    /// Why the chat cannot send or read, or what became of the last message.
    pub(crate) notice: &'a str,
    /// How the body family measures, to set a line's runs one after another and wrap
    /// its text; without it they are placed by an estimate.
    pub(crate) measure: Option<Measure<'a>>,
}

/// One message of the dock.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DockLine<'a> {
    /// The sender's name with its colour codes.
    pub(crate) name: &'a str,
    pub(crate) text: &'a str,
    pub(crate) verified: bool,
    pub(crate) staff: bool,
    /// The sender's SJK key, for their sender card.
    pub(crate) key_id: &'a str,
    /// The message is a holocron drop of this tier: no name, the tier's gem and colour.
    pub(crate) tier: Option<&'static crate::holocrons::Tier>,
    /// The GIF the message links to, shown under it ([`crate::chat_gifs`]).
    pub(crate) gif: Option<&'a crate::chat_gifs::GifId>,
}

/// A blank line for the dock's array.
pub(crate) const BLANK: DockLine<'static> = DockLine {
    name: "",
    text: "",
    verified: false,
    staff: false,
    key_id: "",
    tier: None,
    gif: None,
};

/// Where a page docks the chat, in frame pixels: the left edge of its text, its top
/// and its text's width (the field reaches a little past both sides); and the floor its
/// sender card's foot stays above (where the page's own text starts below it). The card
/// shows left of the dock, level with the name: shapes never cover text, so it goes
/// where it leaves the dock's lines and its field clear (to the name's right it covered
/// the message and the lines under it).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Place {
    pub(crate) x: f32,
    pub(crate) top: f32,
    pub(crate) width: f32,
    pub(crate) card_floor: f32,
}

impl Place {
    /// The field's top, under the messages' box.
    pub(crate) const fn field_top(self) -> f32 {
        self.top + BOX_TOP + BOX + 8.0
    }

    /// The bottom of the dock's last line (why a message could not go, Open chat).
    pub(crate) const fn bottom(self) -> f32 {
        self.field_top() + 42.0 + 22.0
    }
}

/// The pointer tokens a page gives the dock, apart from its own: the field, Open chat,
/// the first of the [`LINES`] names, the sender card and its Mute.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tokens {
    pub(crate) field: u16,
    pub(crate) open: u16,
    pub(crate) names: u16,
    pub(crate) card: u16,
    pub(crate) mute: u16,
}

impl Tokens {
    /// The name of the `index`th message shown.
    pub(crate) const fn name(self, index: usize) -> u16 {
        self.names + index as u16
    }

    fn is_name(self, token: u16) -> bool {
        (self.names..self.names + LINES as u16).contains(&token)
    }
}

/// What the dock asks its page to do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DockAction {
    /// Send what was typed ([`Dock::take_draft`], [`DockCache::send`]).
    Send,
    /// Open the SJK chat's page.
    Open,
    /// Mute, on this PC, the player whose sender card is on show ([`mute`]).
    Mute,
}

/// The sender card a dock name shows under the pointer (`sender_card.rs`): who, the
/// name it is beside, and whether where they are has been asked.
#[derive(Debug)]
pub(super) struct SenderCard {
    pub(super) person: crate::sender_card::Person,
    anchor: Rect,
    located: bool,
    /// When the pointer left the name and the card, while it is off them.
    left_at: Option<std::time::Instant>,
}

/// How long a sender card stays once the pointer is off its name and itself: long enough
/// to cross from the name to a card shown left of the dock (Creyon's report, 11/10/2026:
/// it closed on the way).
const CARD_GRACE: std::time::Duration = std::time::Duration::from_millis(600);

/// What a page keeps of its dock between frames.
#[derive(Debug, Default)]
pub(crate) struct Dock {
    /// What is typed in the field, while typing.
    pub(super) draft: Option<String>,
    /// What Enter sent from the field, until the page takes it.
    sent: Option<String>,
    /// The sender card on show.
    pub(super) sender_card: Option<SenderCard>,
    /// The key and name of the player its Mute asked for, until the page takes them.
    muting: Option<(String, String)>,
    /// The view's messages the last frame showed: the first one's index and how many.
    shown: (usize, usize),
}

impl Dock {
    /// Whether the field takes the keys.
    pub(crate) fn is_typing(&self) -> bool {
        self.draft.is_some()
    }

    /// Start typing in the field (what was typed stays).
    pub(crate) fn start_typing(&mut self) {
        if self.draft.is_none() {
            self.draft = Some(String::new());
        }
    }

    /// Stop typing, what was typed dropped.
    pub(crate) fn stop_typing(&mut self) {
        self.draft = None;
    }

    /// End a sender card's grace at once, as if it had run out.
    #[cfg(test)]
    pub(crate) fn expire_card_for_test(&mut self) {
        if let Some(card) = &mut self.sender_card {
            card.left_at = Some(std::time::Instant::now() - CARD_GRACE);
        }
    }

    /// The page no longer shows the dock: no typing, no card.
    pub(crate) fn hide(&mut self) {
        self.draft = None;
        self.sender_card = None;
    }

    /// What Enter sent from the field.
    pub(crate) fn take_draft(&mut self) -> String {
        self.sent.take().unwrap_or_default()
    }

    /// The key and name of the player the card asked to mute ([`DockAction::Mute`]).
    pub(crate) fn take_mute(&mut self) -> Option<(String, String)> {
        self.muting.take()
    }

    /// Give the sender card on show its player's picture version (`picture`, from their
    /// key) until it is known. Called before the page is built, outside any lock.
    pub(crate) fn picture_sender_card(&mut self, picture: impl FnOnce(&str) -> Option<String>) {
        if let Some(card) = self
            .sender_card
            .as_mut()
            .filter(|card| card.person.avatar.is_none())
            && let Some(key_id) = &card.person.key_id
        {
            card.person.avatar = picture(key_id);
        }
    }

    /// Say on the sender card on show where its player is on the server being played
    /// (`place`, from their key and name: `player_mutes::place`), once. The main page is
    /// on no server and leaves it unknown. Called before the page is built, outside any
    /// lock.
    pub(crate) fn locate_sender_card(
        &mut self,
        place: impl FnOnce(Option<&str>, &str) -> crate::sender_card::Place,
    ) {
        if let Some(card) = self.sender_card.as_mut().filter(|card| !card.located) {
            card.located = true;
            card.person.place = place(card.person.key_id.as_deref(), &card.person.name);
        }
    }

    /// A key while typing in the field, with Shift and Ctrl held or not: text goes into
    /// it (Ctrl+V and Shift+Insert paste, [`crate::sjk_chat_field`]), Backspace takes a
    /// character back, Enter sends, Escape stops; every other key is swallowed, so no
    /// menu key or game binding acts.
    pub(crate) fn typing_key(
        &mut self,
        key: KeyCode,
        text: Option<&str>,
        shift: bool,
        control: bool,
    ) -> Option<DockAction> {
        let draft = self.draft.as_mut()?;
        if crate::text_entry::before_edit(draft, key, text, shift, control) {
            return None;
        }
        match key {
            KeyCode::Escape => self.draft = None,
            KeyCode::Enter | KeyCode::NumpadEnter => {
                if draft.trim().is_empty() {
                    self.draft = None;
                } else {
                    // The draft stays for `take_draft`.
                    let text = std::mem::take(draft);
                    self.draft = None;
                    self.sent = Some(text);
                    return Some(DockAction::Send);
                }
            }
            KeyCode::Backspace => {
                draft.pop();
            }
            _ => crate::sjk_chat_field::key_into(draft, key, text, shift, control),
        }
        None
    }

    /// The pointer over (or clicking) `token`: `None` when it is none of the dock's
    /// `tokens`, else what the dock asks. A click on the field starts typing, on Open
    /// chat opens the page (the page takes the typing); the names and the card only
    /// show the card.
    pub(crate) fn pointer(
        &mut self,
        tokens: Tokens,
        token: u16,
        activate: bool,
    ) -> Option<Option<DockAction>> {
        if token == tokens.field {
            if activate {
                self.start_typing();
            }
            return Some(None);
        }
        if token == tokens.open {
            if activate {
                self.draft = None;
                return Some(Some(DockAction::Open));
            }
            return Some(None);
        }
        if token == tokens.mute {
            // The dock shows only players not muted: its card offers Mute.
            let card = if activate {
                self.sender_card.take()
            } else {
                None
            };
            let Some(card) = card else {
                return Some(None);
            };
            self.muting = Some((card.person.key_id.unwrap_or_default(), card.person.name));
            return Some(Some(DockAction::Mute));
        }
        (token == tokens.card || tokens.is_name(token)).then_some(None)
    }

    /// Something typed in the field, for the world shots.
    #[cfg(test)]
    pub(crate) fn type_for_shot(&mut self, text: &str) {
        self.draft = Some(text.to_owned());
    }
}

/// Mute on this PC the player the dock's card asked for ([`DockAction::Mute`]): the
/// list on this PC (`player_mutes.rs`), nothing sent.
pub(crate) fn mute(dock: &mut Dock) {
    if let Some((key_id, name)) = dock.take_mute() {
        player_mutes::set_muted(Some(&key_id), &name, true);
    }
}

/// One message.
#[derive(Debug, Default)]
struct Line {
    name: String,
    text: String,
    verified: bool,
    staff: bool,
    key_id: String,
    tier: Option<&'static crate::holocrons::Tier>,
    gif: Option<crate::chat_gifs::GifId>,
}

impl Line {
    /// The message with its text `text` (as a line shows it), which may link to a GIF:
    /// the link worded "GIF" and the GIF kept, unless GIFs are off.
    fn with_gif(mut self, text: &str) -> Self {
        if self.tier.is_none() {
            (self.text, self.gif) = crate::chat_gifs::for_message(text, false);
        } else {
            text.clone_into(&mut self.text);
        }
        self
    }
}

/// The dock's copy of the chat.
#[derive(Default)]
pub(crate) struct DockCache {
    /// The chat's revision, outcome serial and mutes revision last read; `None` while
    /// the service is not running.
    mark: Option<Option<(u64, u64, u64)>>,
    /// Whether GIFs were on when the lines were read (`cl_sjkChatGifs`).
    gifs: bool,
    /// The last messages, oldest first.
    lines: Vec<Line>,
    online: u32,
    live: bool,
    /// Why the hub refused the last message.
    refused: String,
    /// Why the last message could not go at all ([`DockCache::send`]).
    pub(crate) local: &'static str,
    /// Made-up messages the world shots show: the service is not read.
    #[cfg(test)]
    frozen: bool,
}

impl DockCache {
    /// Read the chat again if it changed since the last frame.
    pub(crate) fn refresh(&mut self) {
        #[cfg(test)]
        if self.frozen {
            return;
        }
        let mutes = player_mutes::revision();
        let mark = player_identity::with_chat(|chat| {
            (
                chat.revision,
                chat.outcome.as_ref().map_or(0, |outcome| outcome.serial),
                mutes,
            )
        });
        let gifs = crate::chat_gifs::enabled();
        if self.mark == Some(mark) && self.gifs == gifs {
            return;
        }
        self.mark = Some(mark);
        self.gifs = gifs;
        self.lines.clear();
        self.refused.clear();
        let muted = player_mutes::muted_keys();
        let read = player_identity::with_chat(|chat| {
            let shown = chat
                .messages
                .iter()
                .filter(|message| !muted.contains(&message.key_id))
                .filter_map(|message| Some((message, sjk_chat_look::shown(message)?)));
            let skip = shown.clone().count().saturating_sub(LINES);
            // Muted senders are left out above, so their GIFs are never asked for.
            for (message, shown) in shown.skip(skip) {
                let line = Line {
                    name: shown.name,
                    text: String::new(),
                    verified: message.verified && shown.tier.is_none(),
                    staff: message.staff,
                    key_id: message.key_id.clone(),
                    tier: shown.tier,
                    gif: None,
                };
                self.lines.push(line.with_gif(&shown.text));
            }
            if let Some(outcome) = chat.outcome.as_ref().filter(|outcome| !outcome.sent) {
                self.refused.clone_from(&outcome.message);
            }
            (chat.online, chat.live)
        });
        (self.online, self.live) = read.unwrap_or((0, false));
        if read.is_some() {
            self.local = "";
        }
    }

    /// Hand `text`, typed in a dock, to the hub; `chat_on` is `cl_sjkChat`. Why it could
    /// not go shows under the field.
    pub(crate) fn send(&mut self, text: String, chat_on: bool) {
        self.local = if !chat_on {
            "SJK chat is off (Settings > Network)"
        } else if player_identity::chat(text) {
            ""
        } else {
            "Turn the SJK identity on to chat"
        };
    }

    /// The dock's view.
    pub(crate) fn view<'a>(&'a self, lines: &'a mut [DockLine<'a>; LINES]) -> ChatDock<'a> {
        for (slot, line) in lines.iter_mut().zip(&self.lines) {
            *slot = DockLine {
                name: &line.name,
                text: &line.text,
                verified: line.verified,
                staff: line.staff,
                key_id: &line.key_id,
                tier: line.tier,
                gif: line.gif.as_ref(),
            };
        }
        let notice = if !self.local.is_empty() {
            self.local
        } else if self.mark == Some(None) {
            "Turn the SJK identity on to chat"
        } else {
            self.refused.as_str()
        };
        ChatDock {
            lines: &lines[..self.lines.len()],
            online: self.online,
            live: self.live,
            notice,
            measure: None,
        }
    }

    /// Made-up messages (sender, text, verified) and who is online, for the world
    /// shots: the service is no longer read.
    #[cfg(test)]
    pub(crate) fn for_shot(&mut self, lines: &[(&str, &str, bool)], online: u32) {
        self.frozen = true;
        self.mark = Some(Some((0, 0, 0)));
        self.lines = lines
            .iter()
            .enumerate()
            .map(|(index, (name, text, verified))| {
                Line {
                    name: (*name).to_owned(),
                    text: String::new(),
                    verified: *verified,
                    staff: false,
                    key_id: format!("{:016x}", index + 1),
                    tier: None,
                    gif: None,
                }
                .with_gif(text)
            })
            .collect();
        self.online = online;
        self.live = true;
        self.refused.clear();
        self.local = "";
    }
}

/// How the dock's messages measure: with the body family's [`Measure`], else by an
/// estimate of its mean advance. Sizes and widths are window pixels.
struct Lay<'a> {
    measure: Option<Measure<'a>>,
    size: f32,
    /// The text column's width.
    width: f32,
}

impl Lay<'_> {
    fn width(&self, value: &str, face: TextFace) -> f32 {
        match self.measure {
            Some(measure) => measure.width(value, self.size, face),
            None => estimate(value) as f32 * self.size * 0.52,
        }
    }

    /// How many bytes of `value` fit `width` (at least one character).
    fn fits(&self, value: &str, width: f32) -> usize {
        match self.measure {
            Some(measure) => measure.fitting(value, width, self.size, TextFace::Regular),
            None => {
                let room = (width / (self.size * 0.52)).floor().max(1.0) as usize;
                value
                    .char_indices()
                    .nth(room)
                    .map_or(value.len(), |(at, _)| at)
            }
        }
    }

    /// The name's width on the message's first row: at most a little over half the row.
    fn name_width(&self, line: &DockLine<'_>) -> f32 {
        self.width(line.name, TextFace::Regular)
            .min(self.width * 0.55)
    }

    /// The room the JoF emblem takes before the name ([`crate::jof_tag`]), if any.
    fn jof_room(&self, line: &DockLine<'_>) -> f32 {
        if let Some(clan) = crate::jof_tag::clan(line.name) {
            crate::jof_tag::room(clan, crate::jof_tag::side(self.size))
        } else {
            0.0
        }
    }

    /// Where the text starts on the first row: after the JoF emblem, the name, the
    /// tick (or a drop's gem) and a colon (a drop has no name, so none).
    fn indent(&self, line: &DockLine<'_>) -> f32 {
        let tick = if line.verified || line.tier.is_some() {
            sjk_chat_look::tick_room(self.size)
        } else {
            0.0
        };
        let colon = if line.name.is_empty() {
            0.0
        } else {
            self.width(": ", TextFace::Regular)
        };
        self.jof_room(line) + self.name_width(line) + tick + colon
    }

    /// Hand each row of `line`'s text to `row`, as a chat line wraps after its name.
    fn flow(&self, line: &DockLine<'_>, row: impl FnMut(std::ops::Range<usize>)) {
        let indent = self.indent(line);
        sjk_chat_look::flow_each(
            line.text,
            (self.width - indent).max(0.0),
            self.width,
            |value, width| self.fits(value, width),
            row,
        );
    }

    /// How many rows `line` takes.
    fn rows(&self, line: &DockLine<'_>) -> usize {
        let mut rows = 0;
        self.flow(line, |_| rows += 1);
        rows
    }
}

/// Characters of `value` that take room (colour codes take none).
fn estimate(value: &str) -> usize {
    let codes = value
        .as_bytes()
        .windows(2)
        .filter(|pair| pair[0] == b'^' && pair[1].is_ascii_digit())
        .count();
    value.chars().count().saturating_sub(codes * 2)
}

/// Draw the dock at `place`: its name and who is online, the messages that fit its box
/// (newest at the bottom, each on the rows it needs; a newest message taller than the
/// whole box shows its first rows, its name with them, the last cut short), the field
/// (lit while the page's keyboard is on it, `focused`, or the pointer), and under it why
/// a message could not go and Open chat.
pub(crate) fn draw(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    place: Place,
    dock: &mut Dock,
    view: &ChatDock<'_>,
    focused: bool,
    tokens: Tokens,
) {
    let s = frame.s;
    let Place { x, top, width, .. } = place;
    text(
        canvas,
        TextFamily::Display,
        format_args!("SJK chat"),
        frame.rect(x, top, width, 30.0),
        24.0 * s,
        color::TEXT,
        FontWeight::Regular,
        TextAlign::Start,
    );
    let status = frame.rect(x, top + 4.0, width, 24.0);
    let body = |canvas: &mut MenuCanvas, rect, colour, align, value: std::fmt::Arguments<'_>| {
        text(
            canvas,
            TextFamily::Body,
            value,
            rect,
            TEXT_SIZE * s,
            colour,
            FontWeight::Regular,
            align,
        );
    };
    if view.live {
        body(
            canvas,
            status,
            color::MUTED,
            TextAlign::End,
            format_args!("{} online", view.online),
        );
    } else {
        body(
            canvas,
            status,
            color::QUIET,
            TextAlign::End,
            format_args!("Not connected"),
        );
    }
    messages(canvas, frame, place, dock, view, tokens);
    let field_top = place.field_top();
    let field = frame.rect(x - FIELD_OVERHANG, field_top, width + 24.0, 34.0);
    let lit = focused || dock.is_typing() || canvas.token_hovered(tokens.field);
    let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
        rect: field,
        radius: 8.0 * s,
        color: color::alpha(color::HOLO, if lit { 0.14 } else { 0.07 }),
    });
    if lit {
        let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: frame.rect(x - FIELD_OVERHANG, field_top + 6.0, 3.0, 22.0),
            radius: 1.5 * s,
            color: color::GOLD_BRIGHT,
        });
    }
    let inside = frame.rect(x, field_top + 5.0, width - 4.0, 24.0);
    match dock.draft.as_deref() {
        // The end of a long draft shows, as in a field that scrolls.
        Some(draft) => {
            let lay = Lay {
                measure: view.measure,
                size: TEXT_SIZE * s,
                width: inside.width,
            };
            let room = inside.width - lay.width("_", TextFace::Regular);
            let mut used = 0.0;
            let mut start = draft.len();
            for (at, c) in draft.char_indices().rev() {
                used += lay.width(&draft[at..at + c.len_utf8()], TextFace::Regular);
                if used > room {
                    break;
                }
                start = at;
            }
            body(
                canvas,
                inside,
                color::TEXT,
                TextAlign::Start,
                format_args!("{}_", &draft[start..]),
            );
        }
        None if lit => body(
            canvas,
            inside,
            color::MUTED,
            TextAlign::Start,
            format_args!("Enter to talk to every SJK player"),
        ),
        None => body(
            canvas,
            inside,
            color::QUIET,
            TextAlign::Start,
            format_args!("Say something to every SJK player"),
        ),
    }
    canvas.hit_region(tokens.field, field);
    let below = field_top + 42.0;
    if !view.notice.is_empty() {
        body(
            canvas,
            frame.rect(x, below, width - 104.0, 20.0),
            color::QUIET,
            TextAlign::Start,
            format_args!("{}", view.notice),
        );
    }
    let open = frame.rect(x + width - 96.0, below - 2.0, 96.0, 24.0);
    let lit = canvas.token_hovered(tokens.open);
    body(
        canvas,
        open,
        if lit { color::GOLD_BRIGHT } else { color::GOLD },
        TextAlign::End,
        format_args!("Open chat"),
    );
    canvas.hit_region(tokens.open, open);
}

/// The rows of a message in the box, in frame pixels at the text style's `scale`: a
/// message's first row is [`ROW`] high (the gap above it included), the others [`WRAP`].
#[derive(Clone, Copy)]
struct Pitch {
    first: f32,
    wrap: f32,
}

impl Pitch {
    fn new(scale: f32) -> Self {
        Self {
            first: ROW * scale,
            wrap: WRAP * scale,
        }
    }

    /// The height of a message of `rows` rows.
    fn height(self, rows: usize) -> f32 {
        self.first + rows.saturating_sub(1) as f32 * self.wrap
    }

    /// The most rows one message can show in the box.
    fn most(self) -> usize {
        1 + ((BOX - self.first) / self.wrap + 0.01).floor().max(0.0) as usize
    }
}

/// The messages that fit the box, newest at the bottom, from the newest upwards; each
/// name a pointer target for its sender card.
fn messages(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    place: Place,
    dock: &mut Dock,
    view: &ChatDock<'_>,
    tokens: Tokens,
) {
    let s = frame.s;
    let lay = Lay {
        measure: view.measure,
        size: TEXT_SIZE * s,
        width: place.width * s,
    };
    // A larger text style takes taller rows: the box keeps its height and holds fewer.
    let pitch = Pitch::new(view.measure.map_or(1.0, |measure| measure.scale()).max(1.0));
    // Rows of the shown messages and what their GIFs show, newest first.
    let mut taken = [0_usize; LINES];
    let mut gifs = [None; LINES];
    let mut count = 0;
    let mut used = 0.0;
    for line in view.lines.iter().rev().take(LINES) {
        let rows = lay.rows(line);
        let gif = line.gif.map(crate::chat_gifs::show);
        let height = pitch.height(rows) + gif_room(gif);
        if used + height > BOX + 0.5 {
            if count == 0 {
                // Taller than the box: its first rows, and its GIF only when all its
                // text fits with it.
                let most = pitch.most();
                taken[0] = rows.min(most);
                gifs[0] = gif.filter(|_| rows <= most && height <= BOX + 0.5);
                count = 1;
                used = pitch.height(taken[0]) + gif_room(gifs[0]);
            }
            break;
        }
        taken[count] = rows;
        gifs[count] = gif;
        count += 1;
        used += height;
    }
    let first = view.lines.len() - count;
    dock.shown = (first, count);
    let bottom = place.top + BOX_TOP + BOX;
    if count == 0 {
        text(
            canvas,
            TextFamily::Body,
            format_args!("Nobody has said anything yet"),
            frame.rect(place.x, bottom - WRAP, place.width, WRAP - 2.0),
            TEXT_SIZE * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::Start,
        );
        return;
    }
    let mut y = bottom - used;
    for (index, line) in view.lines[first..].iter().enumerate() {
        let rows = taken[count - 1 - index];
        let name = message(canvas, frame, &lay, line, [place.x, y], pitch, rows);
        // Resting the pointer on the name shows the sender's sender card.
        canvas.hit_region(tokens.name(index), name);
        y += pitch.height(rows);
        let gif = gifs[count - 1 - index];
        if let Some(shown) = gif {
            gif_draw::draw(canvas, frame, GIF, shown, [place.x, y], place.width);
        }
        y += gif_room(gif);
    }
}

/// One message from `at` (frame pixels, the top of its room) on at most `rows` rows, as
/// SJK chat lines look everywhere ([`crate::sjk_chat_look`]): the JoF emblem for the
/// clan's tag ([`crate::jof_tag`]), the name in its colours,
/// the verified tick alone for a verified sender, a colon, then the message in the SJK
/// chat's gold going on after them and wrapping to the column's left edge. The last row
/// of a message cut short ends in an ellipsis. Returns the name's rectangle.
fn message(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    lay: &Lay<'_>,
    line: &DockLine<'_>,
    at: [f32; 2],
    pitch: Pitch,
    rows: usize,
) -> Rect {
    let s = frame.s;
    // Each row's text centred in a band of the further rows' pitch, under the gap.
    let first = frame.rect(
        at[0],
        at[1] + pitch.first - pitch.wrap,
        0.0,
        pitch.wrap - 2.0,
    );
    let run = |canvas: &mut MenuCanvas, rect, colour, value: std::fmt::Arguments<'_>| {
        text(
            canvas,
            TextFamily::Body,
            value,
            rect,
            lay.size,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
    };
    let jof = lay.jof_room(line);
    if let Some(clan) = crate::jof_tag::clan(line.name) {
        crate::jof_tag::draw(
            clan,
            first.x,
            first.y + first.height * 0.5,
            crate::jof_tag::side(lay.size),
            1.0,
            |command| {
                let _ = canvas.draw_list_mut().push(command);
            },
        );
    }
    let name_width = lay.name_width(line);
    let name = Rect::new(first.x + jof, first.y, name_width + 1.0, first.height);
    if !line.name.is_empty() {
        run(canvas, name, color::TEXT, format_args!("{}", line.name));
    }
    let mut x = first.x + jof + name_width;
    if let Some(tier) = line.tier {
        sjk_chat_look::gem_mark(
            canvas.draw_list_mut(),
            x,
            first.y + first.height * 0.5,
            lay.size,
            tier,
            1.0,
        );
        x += sjk_chat_look::tick_room(lay.size);
    } else if line.verified {
        sjk_chat_look::tick(
            canvas.draw_list_mut(),
            x,
            first.y + first.height * 0.5,
            lay.size,
            1.0,
        );
        x += sjk_chat_look::tick_room(lay.size);
    }
    if !line.name.is_empty() {
        let colon = lay.width(": ", TextFace::Regular);
        run(
            canvas,
            Rect::new(x, first.y, colon, first.height),
            color::TEXT,
            format_args!(":"),
        );
    }
    let indent = lay.indent(line);
    let cut = rows < lay.rows(line);
    let mut row = 0;
    lay.flow(line, |range| {
        let index = row;
        row += 1;
        if index >= rows || range.is_empty() {
            return;
        }
        let left = if index == 0 { indent } else { 0.0 };
        let rect = Rect::new(
            first.x + left,
            first.y + index as f32 * pitch.wrap * s,
            (lay.width - left).max(1.0),
            first.height,
        );
        // A message cut short: the rest of it on its last row, which the run's
        // ellipsis ends.
        let shown = if cut && index + 1 == rows {
            &line.text[range.start..]
        } else {
            &line.text[range]
        };
        run(
            canvas,
            rect,
            sjk_chat_look::text_colour(line.tier),
            format_args!("{shown}"),
        );
    });
    name
}

/// Follow the pointer over the dock's names (the last frame's): the sender card of the
/// sender under it, kept while the pointer is on the card, else none; then draw it
/// left of the dock ([`Place`]), over the page. Called after everything else the page
/// draws.
pub(crate) fn sender_card(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    place: Place,
    dock: &mut Dock,
    view: &ChatDock<'_>,
    tokens: Tokens,
) {
    let (first, shown) = dock.shown;
    let hovered = (0..shown).find(|index| canvas.token_hovered(tokens.name(*index)));
    match hovered.and_then(|index| Some((index, view.lines.get(first + index)?))) {
        Some((index, line)) => {
            let anchor = canvas.rect_for(tokens.name(index)).unwrap_or_default();
            match &mut dock.sender_card {
                Some(card)
                    if card.person.name == line.name
                        && card.person.key_id.as_deref() == Some(line.key_id) =>
                {
                    card.anchor = anchor;
                    card.left_at = None;
                }
                _ => {
                    dock.sender_card = Some(SenderCard {
                        person: person(line),
                        anchor,
                        located: false,
                        left_at: None,
                    });
                }
            }
        }
        None if canvas.token_hovered(tokens.card) || canvas.token_hovered(tokens.mute) => {
            if let Some(card) = &mut dock.sender_card {
                card.left_at = None;
            }
        }
        None => {
            let now = std::time::Instant::now();
            let gone = dock.sender_card.as_mut().is_some_and(|card| {
                let left = *card.left_at.get_or_insert(now);
                now.duration_since(left) >= CARD_GRACE
            });
            if gone {
                dock.sender_card = None;
            }
        }
    }
    let Some(card) = dock.sender_card.as_ref() else {
        return;
    };
    let size = crate::sender_card::size(&card.person, frame.s);
    let gap = 12.0 * frame.s;
    // Left of the field, which reaches a little past the dock's text.
    let [left, floor] = frame.point(place.x - FIELD_OVERHANG, place.card_floor);
    let origin = [
        (left - gap - size[0]).max(0.0),
        card.anchor.y.min(floor - gap - size[1]).max(0.0),
    ];
    crate::sender_card::draw(
        canvas,
        &crate::sender_card::Card {
            person: &card.person,
            muted: false,
            measure: view.measure.as_ref(),
        },
        origin,
        frame.s,
        crate::sender_card::Tokens {
            card: tokens.card,
            mute: tokens.mute,
        },
    );
}

/// Who a dock message's sender is. The dock leaves muted senders out; where they are
/// is asked once the card shows ([`Dock::locate_sender_card`]).
fn person(line: &DockLine<'_>) -> crate::sender_card::Person {
    crate::sender_card::Person {
        name: line.name.to_owned(),
        key_id: (!line.key_id.is_empty()).then(|| line.key_id.to_owned()),
        hub_name: None,
        verified: line.verified,
        staff: line.staff,
        medals: crate::medals::Medals::default(),
        place: crate::sender_card::Place::Unknown,
        avatar: None,
        note: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{TextStyle, test_font};

    const TOKENS: Tokens = Tokens {
        field: 1,
        open: 2,
        names: 10,
        card: 20,
        mute: 21,
    };
    const PLACE: Place = Place {
        x: 1500.0,
        top: 700.0,
        width: 324.0,
        card_floor: 1_080.0,
    };
    const VIEWPORT: [f32; 2] = [1920.0, 1080.0];

    fn line<'a>(name: &'a str, text: &'a str) -> DockLine<'a> {
        DockLine {
            name,
            text,
            verified: false,
            staff: false,
            key_id: "0123456789abcdef",
            tier: None,
            gif: None,
        }
    }

    /// The dock drawn with the test font in `style`: the body runs in gold (the
    /// messages' text) top to bottom, and the dock's state after it.
    fn gold_runs(lines: &[DockLine<'_>], style: TextStyle) -> (Vec<(String, Rect)>, Dock) {
        let font = test_font();
        let view = ChatDock {
            lines,
            online: 3,
            live: true,
            notice: "",
            measure: Some(Measure::new(&font, style)),
        };
        let mut canvas = MenuCanvas::new();
        canvas.begin_transparent(VIEWPORT);
        let mut dock = Dock::default();
        draw(
            &mut canvas,
            &Frame::new(VIEWPORT),
            PLACE,
            &mut dock,
            &view,
            false,
            TOKENS,
        );
        assert!(!canvas.overflowed());
        let runs = canvas
            .draw_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text {
                    rect,
                    text,
                    color,
                    align: TextAlign::Start,
                    ..
                } if *color == sjk_chat_look::GOLD => {
                    Some((canvas.stored_text(*text).to_owned(), *rect))
                }
                _ => None,
            })
            .collect();
        (runs, dock)
    }

    const LONG: &str = "the new HUD looks great, but the force bar in the corner feels a bit too \
                        small at 4K and the clock could move left";

    /// A holocron drop in the dock has no name or colon: the tier's gem, then its
    /// sentence in the tier's colour.
    #[test]
    fn a_holocron_drop_is_a_gem_and_one_sentence_in_the_tiers_colour() {
        let tier = &crate::holocrons::TIERS[3];
        let drop = DockLine {
            tier: Some(tier),
            ..line("", "Sol found a Mythical Holocron!")
        };
        let lines = [line("Fox", "gg"), drop];
        let font = crate::text::test_font();
        let mut canvas = MenuCanvas::with_capacities(32, 64, 128);
        let frame = Frame::new(VIEWPORT);
        canvas.begin_transparent(VIEWPORT);
        let mut dock = Dock::default();
        draw(
            &mut canvas,
            &frame,
            PLACE,
            &mut dock,
            &ChatDock {
                lines: &lines,
                online: 2,
                live: true,
                notice: "",
                measure: Some(Measure::new(&font, TextStyle::NEUTRAL)),
            },
            false,
            TOKENS,
        );
        let runs: Vec<(String, sjk_ui::Color)> = canvas
            .draw_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, color, .. } => {
                    Some((canvas.stored_text(*text).to_owned(), *color))
                }
                _ => None,
            })
            .collect();
        let sentence = runs
            .iter()
            .find(|(text, _)| text == "Sol found a Mythical Holocron!")
            .expect("the sentence");
        assert_eq!(sentence.1, tier.colour);
        assert_eq!(
            runs.iter().filter(|(text, _)| text == ":").count(),
            1,
            "Fox's colon only"
        );
        let gem = canvas
            .draw_list()
            .commands()
            .iter()
            .filter(|command| matches!(command, DrawCommand::SolidRect { .. }))
            .count();
        assert!(gem >= crate::holocrons::gem::MARK_ROWS);
    }

    #[test]
    fn a_long_message_wraps_onto_rows_of_its_own_whole() {
        let lines = [line("Fox", "gg"), line("^1Kyle", LONG)];
        let (runs, dock) = gold_runs(&lines, TextStyle::NEUTRAL);
        // Test glyphs are 8 wide at size 12, 10 at 15: 32 a row of 324.
        let texts: Vec<&str> = runs.iter().map(|(text, _)| text.as_str()).collect();
        assert_eq!(texts[0], "gg");
        let joined = texts[1..].join(" ");
        assert_eq!(joined, LONG, "nothing of it is lost");
        assert!(texts.len() >= 4, "{texts:?}");
        for pair in runs[1..].windows(2) {
            assert!(pair[1].1.y > pair[0].1.y, "row under row: {runs:?}");
        }
        // Rows after the first start at the column's left edge.
        assert!((runs[2].1.x - PLACE.x).abs() < 0.5, "{runs:?}");
        assert!(runs[1].1.x > PLACE.x);
        assert_eq!(dock.shown, (0, 2));
    }

    #[test]
    fn a_long_message_pushes_older_ones_out_of_the_box() {
        let lines = [
            line("A", "one"),
            line("B", "two"),
            line("C", "three"),
            line("D", LONG),
        ];
        let (runs, dock) = gold_runs(&lines, TextStyle::NEUTRAL);
        let texts: Vec<&str> = runs.iter().map(|(text, _)| text.as_str()).collect();
        assert!(!texts.contains(&"one"), "{texts:?}");
        assert!(texts.contains(&"three"), "{texts:?}");
        assert_eq!(dock.shown.0 + dock.shown.1, lines.len(), "the newest last");
        // The newest ends at the box's bottom, its rows closer than messages are.
        let last = runs.last().unwrap().1;
        let bottom = PLACE.top + BOX_TOP + BOX;
        assert!((last.y - (bottom - WRAP)).abs() < 4.0, "{last:?}");
        let rows = &runs[runs.len() - 2..];
        assert!((rows[1].1.y - rows[0].1.y - WRAP).abs() < 0.01, "{rows:?}");
    }

    #[test]
    fn a_larger_text_style_fits_fewer_rows_and_cuts_a_message_taller_than_the_box() {
        // As long as the hub takes: five rows of the plain style, six at 1.2.
        let tall = format!("{LONG} and the rest of what was said there");
        assert_eq!(tall.len(), sjk_identity::chat::TEXT_MAX);
        let lines = [line("A", "one"), line("Kyle", &tall)];
        let (runs, dock) = gold_runs(&lines, TextStyle::NEUTRAL);
        assert_eq!(dock.shown, (1, 1), "{runs:?}");
        assert_eq!(runs.len(), 5, "whole at the plain style: {runs:?}");
        let style = TextStyle {
            scale: 1.2,
            tracking: 0.0,
        };
        let (runs, dock) = gold_runs(&lines, style);
        assert_eq!(dock.shown, (1, 1), "only the newest: {runs:?}");
        assert_eq!(runs.len(), 5, "five rows fit at 1.2: {runs:?}");
        // Its first rows with its name, the last row holding the rest for the ellipsis.
        assert!(tall.starts_with(runs[0].0.as_str()));
        assert!(tall.ends_with(runs[4].0.as_str()));
        assert!(runs[4].0.len() > 27, "{runs:?}");
        let box_top = PLACE.top + BOX_TOP;
        assert!(runs[0].1.y >= box_top - 1.0, "{runs:?}");
    }

    #[test]
    fn typing_takes_letters_sends_and_stops() {
        let mut dock = Dock::default();
        assert_eq!(dock.pointer(TOKENS, TOKENS.field, false), Some(None));
        assert!(!dock.is_typing());
        assert_eq!(dock.pointer(TOKENS, TOKENS.field, true), Some(None));
        assert!(dock.is_typing());
        dock.typing_key(KeyCode::KeyH, Some("h"), false, false);
        dock.typing_key(KeyCode::KeyI, Some("i"), false, false);
        assert_eq!(
            dock.typing_key(KeyCode::Tab, Some("\t"), false, false),
            None
        );
        assert_eq!(
            dock.typing_key(KeyCode::Enter, Some("\r"), false, false),
            Some(DockAction::Send)
        );
        assert_eq!(dock.take_draft(), "hi");
        assert!(!dock.is_typing());
        assert_eq!(dock.pointer(TOKENS, 99, true), None, "not the dock's");
        assert_eq!(dock.pointer(TOKENS, TOKENS.name(4), true), Some(None));
        assert_eq!(dock.pointer(TOKENS, TOKENS.name(5), true), None);
        dock.start_typing();
        assert_eq!(
            dock.pointer(TOKENS, TOKENS.open, true),
            Some(Some(DockAction::Open))
        );
        assert!(!dock.is_typing(), "the page takes the typing");
    }

    /// A message linking to a GIF shows "GIF" and the GIF's block under it, inside the
    /// box and the column, at 1080p and 4K; older messages make room for it.
    #[test]
    fn a_gif_shows_under_its_message_inside_the_box() {
        let id = crate::chat_gifs::GifId::new("DockGif01Loading").unwrap();
        let font = test_font();
        for viewport in [[1920.0, 1080.0], [3840.0, 2160.0]] {
            let frame = Frame::new(viewport);
            let s = frame.s;
            let place = Place {
                x: 1500.0,
                top: 700.0,
                width: 324.0,
                card_floor: 1_080.0,
            };
            let lines = [
                line("Kyle", "first"),
                line("Kyle", "second"),
                line("Fox", "third"),
                DockLine {
                    gif: Some(&id),
                    ..line("Sol", "look GIF")
                },
            ];
            let view = ChatDock {
                lines: &lines,
                online: 3,
                live: true,
                notice: "",
                measure: Some(Measure::new(&font, TextStyle::NEUTRAL)),
            };
            let mut canvas = MenuCanvas::new();
            canvas.begin_transparent(viewport);
            let mut dock = Dock::default();
            draw(&mut canvas, &frame, place, &mut dock, &view, false, TOKENS);
            assert!(!canvas.overflowed());
            let texts: Vec<(String, Rect)> = canvas
                .draw_list()
                .commands()
                .iter()
                .filter_map(|command| match command {
                    DrawCommand::Text { rect, text, .. } => {
                        Some((canvas.stored_text(*text).to_owned(), *rect))
                    }
                    _ => None,
                })
                .collect();
            let find = |wanted: &str| {
                texts
                    .iter()
                    .find(|(text, _)| text == wanted)
                    .map(|(_, rect)| *rect)
            };
            let label = find("look GIF").expect("the line");
            let loading = find("Loading GIF").expect("the placeholder");
            assert!(loading.y >= label.bottom() - 1.0, "{loading:?} {label:?}");
            let box_top = (place.top + BOX_TOP) * s;
            let box_bottom = (place.top + BOX_TOP + BOX) * s;
            assert!(loading.bottom() <= box_bottom + 0.5, "{loading:?}");
            assert!(loading.x >= place.x * s - 0.5);
            assert!(loading.right() <= (place.x + place.width) * s + 0.5);
            // Room was made: the oldest is gone, the rest are in the box.
            assert!(find("first").is_none());
            for wanted in ["second", "third", "look GIF"] {
                let rect = find(wanted).expect(wanted);
                assert!(rect.y >= box_top - 0.5 && rect.bottom() <= box_bottom + 0.5);
            }
            assert_eq!(dock.shown, (1, 3));
        }
    }

    /// GIFs off: the link stays text and nothing is kept to show.
    #[test]
    fn a_line_keeps_its_link_without_a_gif_for_a_drop() {
        let text = "https://giphy.com/gifs/DockGif02Drop";
        let drop = Line {
            tier: Some(&crate::holocrons::TIERS[0]),
            ..Line::default()
        }
        .with_gif(text);
        assert_eq!((drop.text.as_str(), drop.gif), (text, None));
    }
}
