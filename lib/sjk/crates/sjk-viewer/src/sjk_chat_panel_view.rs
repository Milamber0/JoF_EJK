//! The SJK chat page's drawing, in the SJK UI's look: the messages down the left,
//! newest at the bottom over the field and Send, and the chosen message with what
//! can be done about it on the right. A message flows as one line, as SJK chat does
//! everywhere ([`crate::sjk_chat_look`]): the JoF emblem for the clan's tag
//! ([`crate::jof_tag`]), the name, the verified tick for a verified
//! sender, then the text in the SJK chat's gold, wrapping only when it is too long. Over
//! the chosen message, a small window shows who is online and who was seen last as
//! faces, their pictures or initials ([`super::people`]). A name in the chat or a face
//! in the window under the pointer shows that player's sender card over the page.
//! While the GIF picker is open it takes the messages' place
//! ([`super::gif_picker`]), and its key card takes the whole page's.

use super::*;
use crate::menu::sjk::recent::ago;
use crate::menu::sjk::{Frame, TextTarget, color, key_hint, key_hint_width, kit, text, top_bar};
use crate::menu_widgets::TextFamily;
use crate::sjk_chat_look::{self, Measure};
use crate::text::TextFace;
use sjk_identity::ChatMessage;
use sjk_identity::chat::name_for_display;
use sjk_ui::{DrawCommand, FontWeight, TextAlign};
use std::ops::Range;

/// The columns (frame pixels).
pub(super) const LIST_X: f32 = 96.0;
pub(super) const LIST_WIDTH: f32 = 1_180.0;
const SIDE_X: f32 = 1_340.0;
const SIDE_WIDTH: f32 = 484.0;
const TOP: f32 = 170.0;
/// The messages' area.
const LIST_TOP: f32 = 236.0;
const LIST_BOTTOM: f32 = 880.0;
/// A message: its first row (the name's), each row after it and the gap after it.
const NAME_ROW: f32 = 28.0;
const BODY_ROW: f32 = 24.0;
const GAP: f32 = 12.0;
/// The sizes of a message's name, its text and what is said of it (frame pixels).
const NAME_SIZE: f32 = 19.0;
const TEXT_SIZE: f32 = 17.0;
const META_SIZE: f32 = 15.0;
/// The field, GIF and Send.
const FIELD_Y: f32 = 900.0;
const FIELD_WIDTH: f32 = 936.0;
const GIF_X: f32 = LIST_X + FIELD_WIDTH + 16.0;
const SEND_X: f32 = LIST_X + 1_060.0;
/// The keys' line.
const KEYS_Y: f32 = 1_010.0;
/// The who-is-online window: its left, top and width (its height follows what it
/// shows), its first heading, from a heading to the faces under it, from the faces to
/// the next heading, a quiet line's row and the room under the last part (frame pixels).
const PEOPLE_X: f32 = SIDE_X - 18.0;
const PEOPLE_TOP: f32 = 204.0;
const PEOPLE_WIDTH: f32 = SIDE_WIDTH + 36.0;
const ONLINE_Y: f32 = 230.0;
const UNDER_HEADING: f32 = 22.0;
const PART_GAP: f32 = 34.0;
const NOTE_ROW: f32 = 22.0;
const PEOPLE_PAD: f32 = 18.0;
/// The chosen message's heading, this far under the window.
const CHOSEN_GAP: f32 = 34.0;

/// A name under the pointer: whose card it shows, who, and the name's rectangle.
type Shows = (CardSource, crate::sender_card::Person, sjk_ui::Rect);

/// A message's height on the page, its text in `rows` rows.
fn height(rows: usize) -> f32 {
    NAME_ROW + rows.saturating_sub(1) as f32 * BODY_ROW + GAP
}

/// A message laid out on the page, in frame pixels.
struct Laid {
    /// The name as shown, with its colour codes.
    name: String,
    /// The room the JoF emblem takes before the name ([`crate::jof_tag`]), or 0.
    jof: f32,
    name_width: f32,
    verified: bool,
    /// Where the text starts on the first row, after the name, the tick and the colon.
    indent: f32,
    /// Staff, and how long ago it came, on the right of the first row.
    meta: String,
    staff: bool,
    /// The text (or that its sender is muted here) and its rows.
    text: String,
    rows: Vec<Range<usize>>,
    muted: bool,
    /// The message is a holocron drop of this tier (or of one this client does not
    /// know, `None` with a text that says so): the tier's gem stands where the tick
    /// does, there is no colon, and the text is in the tier's colour.
    tier: Option<&'static crate::holocrons::Tier>,
    drop: bool,
    /// The message's GIF ([`crate::chat_gifs`]) and what it shows this frame.
    gif: Option<crate::chat_gifs::Shown>,
}

impl Laid {
    /// The message's height on the page, its GIF under its text included.
    fn height(&self) -> f32 {
        height(self.rows.len())
            + self
                .gif
                .map_or(0.0, |shown| crate::chat_gifs::draw::room(GIF, shown))
    }
}

/// How the page shows a message's GIF.
const GIF: crate::chat_gifs::draw::Look = crate::chat_gifs::draw::PAGE;

/// Lay `message` out in the page's column, measured as `measure` draws at the frame
/// scale `s`.
fn lay(message: &ChatMessage, inputs: &Inputs<'_>, measure: &Measure<'_>, s: f32) -> Laid {
    let s = s.max(0.001);
    let width = |value: &str, size: f32, face| measure.width(value, size * s, face) / s;
    let name = name_for_display(&message.name);
    let name = if name.is_empty() {
        "(no name)".to_owned()
    } else {
        name
    };
    let jof = if let Some(clan) = crate::jof_tag::clan(&message.name) {
        crate::jof_tag::room(clan, crate::jof_tag::side(NAME_SIZE))
    } else {
        0.0
    };
    let name_width = width(&name, NAME_SIZE, TextFace::Semibold).min(LIST_WIDTH * 0.4);
    // A holocron drop reads `Sol <gem> found a Rare Holocron.` on the page: the name, the
    // tier's gem for the tick, no colon.
    let drop = message.holocron.is_some();
    let words = crate::holocrons::line::words(message);
    let tick = if message.verified || drop {
        sjk_chat_look::tick_room(NAME_SIZE)
    } else {
        0.0
    };
    let colon = if drop {
        0.0
    } else {
        width(": ", TEXT_SIZE, TextFace::Regular)
    };
    let indent = jof + name_width + tick + colon;
    let staff = if message.staff { "Staff  ·  " } else { "" };
    let when = ago(u64::try_from(message.at).unwrap_or(0), inputs.now);
    let meta = format!("{staff}{when}");
    let meta_room = width(&meta, META_SIZE, TextFace::Regular) + 24.0;
    let muted = inputs.muted.contains(&message.key_id);
    let mut gif = None;
    let text = if muted {
        "Muted on this PC".to_owned()
    } else if let Some(words) = &words {
        words.rest.clone()
    } else if drop {
        "found a holocron.".to_owned()
    } else {
        // A muted sender's GIF is never asked for: the text above says so instead.
        let (text, id) =
            crate::chat_gifs::for_message(&sjk_chat_look::message_text(&message.text), muted);
        gif = id.as_ref().map(crate::chat_gifs::show);
        text
    };
    let rows = sjk_chat_look::flow(
        &text,
        (LIST_WIDTH - indent - meta_room).max(0.0),
        LIST_WIDTH,
        |value, room| measure.fitting(value, room * s, TEXT_SIZE * s, TextFace::Regular),
    );
    Laid {
        name,
        jof,
        name_width,
        verified: message.verified && !drop,
        indent,
        meta,
        staff: message.staff,
        text,
        rows,
        muted,
        tier: words.map(|words| words.tier),
        drop,
        gif,
    }
}

/// Leave out of `canvas` the text runs whose ink reaches `card` (measured with
/// `measure`): text draws over every shape, so a run under a card drawn later would
/// show through it. A run's rectangle can be far wider than its ink (a message's is
/// the whole list's width), so the ink is measured, else short lines beside the card
/// would vanish too.
fn leave_out_text_under(canvas: &mut MenuCanvas, measure: &Measure<'_>, card: sjk_ui::Rect) {
    let mut covered = Vec::new();
    for command in canvas.draw_list().commands() {
        let &DrawCommand::Text {
            rect,
            text,
            size,
            align,
            weight,
            ..
        } = command
        else {
            continue;
        };
        if !rect.overlaps(card) {
            continue;
        }
        let face = if matches!(weight, FontWeight::Regular) {
            TextFace::Regular
        } else {
            TextFace::Semibold
        };
        // A little room for the face's own spacing and weight.
        let ink =
            (measure.width(canvas.stored_text(text), size, face) * 1.05 + 2.0).min(rect.width);
        let x = match align {
            TextAlign::Start => rect.x,
            TextAlign::End => rect.right() - ink,
            _ => rect.x + (rect.width - ink) * 0.5,
        };
        if sjk_ui::Rect::new(x, rect.y, ink, rect.height).overlaps(card) {
            covered.push(text);
        }
    }
    if covered.is_empty() {
        return;
    }
    canvas.draw_list_mut().retain(
        |command| !matches!(command, DrawCommand::Text { text, .. } if covered.contains(text)),
    );
}

impl Panel {
    /// Draw the page with what `inputs` says.
    pub(crate) fn append_sjk(
        &mut self,
        inputs: &Inputs<'_>,
        target: TextTarget<'_>,
        viewport: [f32; 2],
    ) {
        let measure = target.body_measure();
        self.build(inputs, &measure, viewport);
        target.append(&self.ui, viewport);
    }

    /// Lay the page out, its messages measured in the body family (`measure`).
    pub(super) fn build(&mut self, inputs: &Inputs<'_>, measure: &Measure<'_>, viewport: [f32; 2]) {
        let frame = Frame::new(viewport);
        self.order.clear();
        std::mem::swap(&mut self.shown.people, &mut self.previous_people);
        self.ui.begin_transparent(viewport);
        crate::settings::sjk_view::backdrop(&mut self.ui, viewport);
        top_bar(&mut self.ui, &frame, "Back", BACK_TOKEN, "SJK chat", None);
        inputs.giphy_key.clone_into(&mut self.giphy_key);
        let has_key = crate::chat_gifs::search::key(inputs.giphy_key).is_some();
        if self.gifs.key_card_due(has_key) {
            self.focus = super::gif_picker::KEY_FIELD_TOKEN;
        }
        if self.gifs.key_card.is_some() {
            // The card alone: text draws over every shape, so nothing goes under it.
            self.card = None;
            self.key_card(&frame);
            self.keys(&frame);
            if !self.order.contains(&self.focus) {
                self.focus = super::gif_picker::KEY_FIELD_TOKEN;
            }
            self.ui.finish(self.focus);
            return;
        }
        self.status(&frame, inputs);
        let named = if self.gifs.open {
            self.shown.messages.clear();
            self.shown.total = inputs.chat.map_or(0, |chat| chat.messages.len());
            self.gif_picker(&frame, inputs);
            None
        } else {
            self.messages(&frame, inputs, measure)
        };
        let (listed, bottom) = self.people(&frame, inputs);
        self.follow_card(named.or(listed));
        self.composer(&frame, inputs);
        self.chosen(&frame, inputs, measure, bottom + CHOSEN_GAP);
        self.keys(&frame);
        self.card(&frame, inputs, measure, viewport);
        if !self.order.contains(&self.focus) {
            self.focus = FIELD_TOKEN;
        }
        self.ui.finish(self.focus);
        // The faces changed under a resting pointer (the hub's list came, or changed):
        // the targets after them moved in the frame's list, so find what it rests on.
        if self.shown.people != self.previous_people
            && let Some(at) = self.pointer
        {
            let _ = self.ui.pointer(sjk_ui::InputEvent::PointerMove(at));
        }
    }

    /// Who reads the chat, or why it cannot be read, and what it is.
    fn status(&mut self, frame: &Frame, inputs: &Inputs<'_>) {
        let s = frame.s;
        let (line, colour) = match inputs.chat {
            _ if !inputs.enabled => (
                "SJK chat is off (Settings > Network > SJK chat)".to_owned(),
                color::EMBER,
            ),
            None => (
                "The SJK identity is off: turn it on to chat (Settings > Network)".to_owned(),
                color::EMBER,
            ),
            Some(chat) if !chat.live => ("Connecting to the SJK hub...".to_owned(), color::MUTED),
            Some(chat) if chat.online == 1 => {
                ("1 SJK player reading".to_owned(), color::GOLD_BRIGHT)
            }
            Some(chat) => (
                format!("{} SJK players reading", chat.online),
                color::GOLD_BRIGHT,
            ),
        };
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{line}"),
            frame.rect(LIST_X, TOP - 4.0, LIST_WIDTH, 28.0),
            22.0 * s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!(
                "Every SJK player reads this, in game and in the menus. The hub keeps the last 200 messages, in memory only."
            ),
            frame.rect(LIST_X, TOP + 26.0, LIST_WIDTH, 22.0),
            15.0 * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    /// The messages that fit, newest at the bottom, `scroll` messages up from it; returns
    /// the sender whose name is under the pointer, for the card.
    fn messages(
        &mut self,
        frame: &Frame,
        inputs: &Inputs<'_>,
        measure: &Measure<'_>,
    ) -> Option<Shows> {
        let s = frame.s;
        self.shown.messages.clear();
        let empty = std::collections::VecDeque::new();
        let messages = inputs.chat.map_or(&empty, |chat| &chat.messages);
        self.shown.total = messages.len();
        self.scroll = self.scroll.min(messages.len().saturating_sub(1));
        // From the newest on show upwards, while they fit.
        let mut room = LIST_BOTTOM - LIST_TOP;
        let mut first = messages.len().saturating_sub(self.scroll);
        let end = first;
        let mut laid = Vec::with_capacity(MESSAGES_SHOWN);
        while first > 0 && end - first < MESSAGES_SHOWN {
            let message = &messages[first - 1];
            let layout = lay(message, inputs, measure, s);
            let needed = layout.height();
            if needed > room {
                break;
            }
            room -= needed;
            first -= 1;
            laid.push(layout);
        }
        laid.reverse();
        if first == end {
            let line = if inputs.chat.is_some() && inputs.enabled {
                "Nobody has said anything yet. Say hello."
            } else {
                ""
            };
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{line}"),
                frame.rect(LIST_X, LIST_BOTTOM - 40.0, LIST_WIDTH, 24.0),
                17.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        let mut y = LIST_TOP + room;
        let mut hovered = None;
        for (index, (message, laid)) in messages.range(first..end).zip(&laid).enumerate() {
            let token = MESSAGE_BASE + index as u16;
            let tall = laid.height();
            let row = [LIST_X - 18.0, y - 4.0, LIST_WIDTH + 18.0, tall - 4.0];
            if self.focus == token || self.selected == Some(message.id) {
                kit::band(&mut self.ui, frame, row);
            }
            self.message(frame, laid, y);
            self.ui
                .hit_region(token, frame.rect(row[0], row[1], row[2], row[3]));
            // The name over its row: resting the pointer on it shows the card.
            let name = frame.rect(LIST_X + laid.jof, y, laid.name_width + 2.0, NAME_ROW - 2.0);
            self.ui.hit_region(NAME_BASE + index as u16, name);
            if self.ui.token_hovered(NAME_BASE + index as u16) {
                let person = crate::sender_card::Person {
                    name: laid.name.clone(),
                    key_id: Some(message.key_id.clone()),
                    hub_name: None,
                    verified: message.verified,
                    staff: message.staff,
                    medals: crate::medals::Medals::default(),
                    place: crate::sender_card::Place::Unknown,
                    avatar: None,
                    note: None,
                };
                hovered = Some((CardSource::Message(message.id), person, name));
            }
            self.order.push(token);
            self.shown
                .messages
                .push((message.id, message.key_id.clone()));
            y += tall;
        }
        if first > 0 {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{first} older: Page Up"),
                frame.rect(LIST_X, LIST_TOP - 26.0, LIST_WIDTH, 20.0),
                14.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::End,
            );
        }
        if self.scroll > 0 {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{} newer: Page Down", self.scroll),
                frame.rect(LIST_X, LIST_BOTTOM - 2.0, LIST_WIDTH, 20.0),
                14.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::End,
            );
        }
        hovered
    }

    /// Draw a laid out message whose first row's top is `y`: the JoF emblem, the
    /// name, the tick, the
    /// colon and the text going on after them, then its other rows from the left
    /// edge, and what is said of it on the right of the first row.
    fn message(&mut self, frame: &Frame, laid: &Laid, y: f32) {
        let s = frame.s;
        let first = frame.rect(LIST_X, y, LIST_WIDTH, NAME_ROW - 2.0);
        if let Some(clan) = crate::jof_tag::clan(&laid.name) {
            crate::jof_tag::draw(
                clan,
                first.x,
                first.y + first.height * 0.5,
                crate::jof_tag::side(NAME_SIZE * s),
                1.0,
                |command| {
                    let _ = self.ui.draw_list_mut().push(command);
                },
            );
        }
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{}", laid.name),
            frame.rect(LIST_X + laid.jof, y, laid.name_width + 2.0, NAME_ROW - 2.0),
            NAME_SIZE * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let mut x = LIST_X + laid.jof + laid.name_width;
        if let Some(tier) = laid.tier {
            let [left, _] = frame.point(x, y);
            sjk_chat_look::gem_mark(
                self.ui.draw_list_mut(),
                left,
                first.y + first.height * 0.5,
                NAME_SIZE * s,
                tier,
                1.0,
            );
            x += sjk_chat_look::tick_room(NAME_SIZE);
        } else if laid.verified {
            let [left, _] = frame.point(x, y);
            sjk_chat_look::tick(
                self.ui.draw_list_mut(),
                left,
                first.y + first.height * 0.5,
                NAME_SIZE * s,
                1.0,
            );
            x += sjk_chat_look::tick_room(NAME_SIZE);
        }
        if !laid.drop {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!(":"),
                frame.rect(x, y, 12.0, NAME_ROW - 2.0),
                TEXT_SIZE * s,
                color::TEXT,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{}", laid.meta),
            frame.rect(LIST_X + LIST_WIDTH - 420.0, y + 2.0, 420.0, NAME_ROW - 6.0),
            META_SIZE * s,
            if laid.staff {
                color::GOLD_BRIGHT
            } else {
                color::QUIET
            },
            FontWeight::Regular,
            TextAlign::End,
        );
        let colour = if laid.muted {
            color::QUIET
        } else {
            sjk_chat_look::text_colour(laid.tier)
        };
        for (row, range) in laid.rows.iter().enumerate() {
            if range.is_empty() {
                continue;
            }
            let rect = if row == 0 {
                frame.rect(
                    LIST_X + laid.indent,
                    y,
                    (LIST_WIDTH - laid.indent).max(1.0),
                    NAME_ROW - 2.0,
                )
            } else {
                frame.rect(
                    LIST_X,
                    y + NAME_ROW + (row - 1) as f32 * BODY_ROW,
                    LIST_WIDTH,
                    BODY_ROW - 2.0,
                )
            };
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{}", &laid.text[range.clone()]),
                rect,
                TEXT_SIZE * s,
                colour,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        if let Some(shown) = laid.gif {
            let under = y + NAME_ROW + laid.rows.len().saturating_sub(1) as f32 * BODY_ROW;
            crate::chat_gifs::draw::draw(
                &mut self.ui,
                frame,
                GIF,
                shown,
                [LIST_X, under],
                LIST_WIDTH,
            );
        }
    }

    /// The who-is-online window over the chosen message: who reads the chat now
    /// (playing first) and who was seen most recently, as faces, or, from a hub that
    /// does not list them, the count and the chat's last senders. Returns the player
    /// whose face is under the pointer, for the card, and the window's bottom (frame
    /// pixels), which follows what it shows.
    fn people(&mut self, frame: &Frame, inputs: &Inputs<'_>) -> (Option<Shows>, f32) {
        use super::people::{
            FACES_WIDTH, ONLINE_RADIUS, RECENT_RADIUS, centre, columns, grid_height,
        };
        let s = frame.s;
        self.shown.people.clear();
        self.shown.unpictured.clear();
        let chat = inputs.chat.filter(|chat| inputs.enabled && chat.live);
        let listed = chat.map_or_else(Default::default, |chat| {
            super::people::derive(chat, inputs.muted)
        });
        // Where each part goes, so the window can be drawn round them first.
        let online_note = match chat {
            None => Some("Shows once the SJK hub answers."),
            Some(_) if !listed.listed => Some("This hub counts them but does not say who."),
            Some(_) if listed.online.is_empty() => Some("Nobody else."),
            Some(_) => None,
        };
        let online_columns = columns(FACES_WIDTH, ONLINE_RADIUS);
        let recent_columns = columns(FACES_WIDTH, RECENT_RADIUS);
        let online_faces = listed.online.len() + usize::from(listed.more > 0);
        let online_top = ONLINE_Y + UNDER_HEADING;
        let mut online_bottom =
            online_top + grid_height(online_faces, online_columns, ONLINE_RADIUS);
        if online_note.is_some() {
            online_bottom = online_bottom.max(online_top + NOTE_ROW);
        }
        // The second part, once the hub has answered.
        let recent_y = online_bottom + PART_GAP;
        let recent_top = recent_y + UNDER_HEADING;
        let bottom = match chat {
            None => online_bottom,
            Some(_) if listed.recent.is_empty() => recent_top + NOTE_ROW,
            Some(_) => recent_top + grid_height(listed.recent.len(), recent_columns, RECENT_RADIUS),
        } + PEOPLE_PAD;
        let rect = frame.rect(PEOPLE_X, PEOPLE_TOP, PEOPLE_WIDTH, bottom - PEOPLE_TOP);
        let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
            rect,
            radius: 14.0 * s,
            color: color::alpha(color::HOLO, 0.05),
        });
        let _ = self.ui.draw_list_mut().push(DrawCommand::Border {
            rect,
            radius: 14.0 * s,
            width: 1.0 * s,
            color: color::alpha(color::HOLO, 0.25),
        });
        let online = match chat {
            Some(_) => format!("Online  \u{b7}  {}", listed.count),
            None => "Online".to_owned(),
        };
        kit::heading(&mut self.ui, frame, SIDE_X, ONLINE_Y, SIDE_WIDTH, &online);
        let mut hovered = None;
        if let Some(note) = online_note {
            self.quiet_row(frame, online_top, note);
        }
        for (index, row) in listed.online.iter().enumerate() {
            let at = centre(index, online_columns, ONLINE_RADIUS, [SIDE_X, online_top]);
            let note = if row.playing {
                "Online now, in a match"
            } else {
                "Online now"
            };
            hovered = self
                .face(frame, row, at, ONLINE_RADIUS, true, note.to_owned())
                .or(hovered);
        }
        if listed.more > 0 {
            let at = centre(
                listed.online.len(),
                online_columns,
                ONLINE_RADIUS,
                [SIDE_X, online_top],
            );
            self.more_face(frame, at, ONLINE_RADIUS, listed.more);
        }
        if chat.is_none() {
            return (hovered, bottom);
        }
        let recent = if listed.listed {
            "Recently active"
        } else {
            "Recently in chat"
        };
        kit::heading(&mut self.ui, frame, SIDE_X, recent_y, SIDE_WIDTH, recent);
        if listed.recent.is_empty() {
            self.quiet_row(frame, recent_top, "Nobody yet.");
        }
        for (index, row) in listed.recent.iter().enumerate() {
            let at = centre(index, recent_columns, RECENT_RADIUS, [SIDE_X, recent_top]);
            let when = ago(row.seen, inputs.now);
            let note = if listed.listed {
                format!("Seen {when}")
            } else {
                format!("Last spoke {when}")
            };
            hovered = self
                .face(frame, row, at, RECENT_RADIUS, false, note)
                .or(hovered);
        }
        (hovered, bottom)
    }

    /// A quiet line in the who-is-online window, its top at `y`.
    fn quiet_row(&mut self, frame: &Frame, y: f32, line: &str) {
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{line}"),
            frame.rect(SIDE_X, y, SIDE_WIDTH, NOTE_ROW),
            15.0 * frame.s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    /// A player's face in the who-is-online window, a disc of `radius` round `at`
    /// (frame pixels): their picture once it is loaded, else their initial on their
    /// colour, as their card draws it; smaller and dimmed for someone not `online`,
    /// greyed for someone muted here, ringed in gold under the pointer, with a gold dot
    /// for someone in a match. Returns them, with `note` for their card, when the
    /// pointer is on it.
    fn face(
        &mut self,
        frame: &Frame,
        row: &super::people::Row,
        at: [f32; 2],
        radius: f32,
        online: bool,
        note: String,
    ) -> Option<Shows> {
        let s = frame.s;
        let token = PERSON_BASE + self.shown.people.len() as u16;
        self.shown.people.push(row.key_id.clone());
        let centre = frame.point(at[0], at[1]);
        let r = radius * s;
        let square = sjk_ui::Rect::new(centre[0] - r, centre[1] - r, r * 2.0, r * 2.0);
        let lit = self.ui.token_hovered(token);
        // The version the hub listed, else one found for a sender (asked after the
        // frame, outside the chat's lock).
        let version = match &row.avatar {
            Some(version) => Some(version.clone()),
            None => {
                let found = self.picture_of(&row.key_id).map(str::to_owned);
                if found.is_none() {
                    self.shown.unpictured.push(row.key_id.clone());
                }
                found
            }
        };
        let texture = version
            .as_deref()
            .and_then(|version| crate::avatars::texture(&row.key_id, version));
        let name = name_for_display(&row.name);
        match texture {
            Some(texture) => {
                let tint = if row.muted {
                    sjk_ui::Color::new(0.5, 0.52, 0.58, 0.45)
                } else if online {
                    sjk_ui::Color::new(1.0, 1.0, 1.0, 1.0)
                } else {
                    sjk_ui::Color::new(0.86, 0.88, 0.92, 0.82)
                };
                let _ = self.ui.draw_list_mut().push(DrawCommand::TexturedQuad {
                    rect: square,
                    texture,
                    color: tint,
                });
            }
            None => {
                let ground = if row.muted {
                    color::alpha(color::QUIET, 0.22)
                } else {
                    let own = crate::profile_card::stand_in_colour(&row.key_id, &name);
                    if online { own } else { color::alpha(own, 0.72) }
                };
                let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: square,
                    radius: r,
                    color: ground,
                });
                text(
                    &mut self.ui,
                    TextFamily::Display,
                    format_args!("{}", super::people::letter(&name)),
                    square,
                    r * 1.1,
                    if row.muted {
                        color::QUIET
                    } else if online {
                        color::TEXT
                    } else {
                        color::MUTED
                    },
                    FontWeight::Semibold,
                    TextAlign::Center,
                );
            }
        }
        let (ring, width) = if lit {
            (color::GOLD_BRIGHT, 2.0 * s)
        } else if row.muted {
            (color::alpha(color::QUIET, 0.35), 1.5 * s)
        } else if online {
            (color::alpha(color::HOLO, 0.5), 1.5 * s)
        } else {
            (color::alpha(color::HOLO, 0.25), 1.5 * s)
        };
        let _ = self.ui.draw_list_mut().push(DrawCommand::Arc {
            center: centre,
            radius: r + width,
            width,
            start: 0.0,
            sweep: std::f32::consts::TAU,
            color: ring,
            knockout: None,
        });
        if online && row.playing {
            // In a match: a gold dot at the face's foot, right, rimmed by the ground.
            let foot = [centre[0] + r * 0.72, centre[1] + r * 0.72];
            let dot = if row.muted {
                color::QUIET
            } else {
                color::GOLD_BRIGHT
            };
            for (size, colour) in [(7.0 * s, color::alpha(color::SPACE, 0.95)), (5.0 * s, dot)] {
                let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: sjk_ui::Rect::new(foot[0] - size, foot[1] - size, size * 2.0, size * 2.0),
                    radius: size,
                    color: colour,
                });
            }
        }
        self.ui.hit_region(token, square);
        lit.then(|| {
            let person = crate::sender_card::Person {
                name: if name.is_empty() {
                    "(no name)".to_owned()
                } else {
                    name
                },
                key_id: Some(row.key_id.clone()),
                hub_name: None,
                verified: row.verified,
                staff: row.staff,
                medals: crate::medals::Medals::default(),
                place: crate::sender_card::Place::Unknown,
                avatar: version,
                note: Some(note),
            };
            (CardSource::Person(row.key_id.clone()), person, square)
        })
    }

    /// The last online face of a full window: how many more players read the chat.
    fn more_face(&mut self, frame: &Frame, at: [f32; 2], radius: f32, more: usize) {
        let s = frame.s;
        let centre = frame.point(at[0], at[1]);
        let r = radius * s;
        let square = sjk_ui::Rect::new(centre[0] - r, centre[1] - r, r * 2.0, r * 2.0);
        let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: square,
            radius: r,
            color: color::alpha(color::HOLO, 0.12),
        });
        let width = 1.5 * s;
        let _ = self.ui.draw_list_mut().push(DrawCommand::Arc {
            center: centre,
            radius: r + width,
            width,
            start: 0.0,
            sweep: std::f32::consts::TAU,
            color: color::alpha(color::HOLO, 0.35),
            knockout: None,
        });
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("+{more}"),
            square,
            if more > 99 { 0.62 } else { 0.78 } * r,
            color::MUTED,
            FontWeight::Semibold,
            TextAlign::Center,
        );
    }

    /// The field, Send, and what became of the last message.
    fn composer(&mut self, frame: &Frame, inputs: &Inputs<'_>) {
        let s = frame.s;
        let focused = self.focus == FIELD_TOKEN;
        let caret = focused && (self.epoch.elapsed().as_millis() / 500).is_multiple_of(2);
        let count = self.draft.chars().count();
        let shown = if count > 96 {
            let tail: String = self.draft.chars().skip(count - 93).collect();
            format!("...{tail}")
        } else if self.draft.is_empty() && !focused {
            "Say something to every SJK player".to_owned()
        } else {
            self.draft.clone()
        };
        let field = [LIST_X, FIELD_Y, FIELD_WIDTH, 46.0];
        kit::field(
            &mut self.ui,
            frame,
            field,
            format_args!("{shown}{}", if caret { "|" } else { "" }),
            focused,
            false,
        );
        self.ui.hit_region(
            FIELD_TOKEN,
            frame.rect(field[0], field[1], field[2], field[3]),
        );
        self.order.push(FIELD_TOKEN);
        let can_send = !self.draft.trim().is_empty() && inputs.enabled && inputs.chat.is_some();
        // GIF opens the picker (closes it when open); off with the chat's GIFs.
        let gifs_on = crate::chat_gifs::enabled();
        kit::button(
            &mut self.ui,
            frame,
            [GIF_X, FIELD_Y + 2.0, 88.0, 42.0],
            "GIF",
            false,
            gifs_on,
            self.focus == GIF_TOKEN || self.gifs.open,
            GIF_TOKEN,
        );
        if gifs_on {
            self.order.push(GIF_TOKEN);
        }
        kit::button(
            &mut self.ui,
            frame,
            [SEND_X, FIELD_Y + 2.0, 120.0, 42.0],
            "Send",
            true,
            can_send,
            self.focus == SEND_TOKEN,
            SEND_TOKEN,
        );
        self.order.push(SEND_TOKEN);
        let notice = inputs
            .chat
            .and_then(|chat| chat.outcome.as_ref())
            .filter(|outcome| !outcome.sent)
            .map(|outcome| outcome.message.as_str());
        let line = match notice {
            Some(why) => format!("Not sent: {why}"),
            None => format!("{count} of {DRAFT_MAX}"),
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{line}"),
            frame.rect(LIST_X + 4.0, FIELD_Y + 54.0, FIELD_WIDTH, 20.0),
            14.0 * s,
            if notice.is_some() {
                color::EMBER
            } else {
                color::QUIET
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        // A refusal (the hub's too many requests, mostly) can be tried again.
        if notice.is_some() && !self.last_sent.is_empty() && inputs.enabled {
            kit::button(
                &mut self.ui,
                frame,
                [SEND_X, FIELD_Y + 52.0, 120.0, 34.0],
                "Retry",
                false,
                true,
                self.focus == RETRY_TOKEN,
                RETRY_TOKEN,
            );
            self.order.push(RETRY_TOKEN);
        }
    }

    /// The chosen message's sender and what can be done: Mute here, and for staff
    /// Delete and Mute at the hub.
    fn chosen(&mut self, frame: &Frame, inputs: &Inputs<'_>, measure: &Measure<'_>, top: f32) {
        let s = frame.s;
        kit::heading(
            &mut self.ui,
            frame,
            SIDE_X,
            top,
            SIDE_WIDTH,
            "Chosen message",
        );
        let message = self.selected.and_then(|id| {
            inputs
                .chat
                .and_then(|chat| chat.messages.iter().find(|message| message.id == id))
        });
        self.shown.chosen_gif = None;
        let Some(message) = message else {
            self.shown.chosen = None;
            for (row, line) in [
                "Choose a message (Up from the field, or a click)",
                "to mute its sender on this PC.",
            ]
            .into_iter()
            .enumerate()
            {
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{line}"),
                    frame.rect(SIDE_X, top + 30.0 + row as f32 * 24.0, SIDE_WIDTH, 22.0),
                    16.0 * s,
                    color::QUIET,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
            }
            return;
        };
        let muted = inputs.muted.contains(&message.key_id);
        // The name in the body family, which the page measures, and the tick alone
        // after it for a verified sender.
        let name = name_for_display(&message.name);
        self.shown.chosen = Some((message.id, message.key_id.clone(), name.clone(), muted));
        let size = 26.0;
        let room = SIDE_WIDTH - sjk_chat_look::tick_room(size);
        let width = (measure.width(&name, size * s, TextFace::Semibold) / s.max(0.001)).min(room);
        let rect = frame.rect(SIDE_X, top + 28.0, width + 2.0, 34.0);
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{name}"),
            rect,
            size * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        if message.verified {
            sjk_chat_look::tick(
                self.ui.draw_list_mut(),
                frame.point(SIDE_X + width, 0.0)[0],
                rect.y + rect.height * 0.5,
                size * s,
                1.0,
            );
        }
        // The player's own key stays off screen, on their own messages too.
        if let Some(key) = crate::player_identity::printable_key_id(&message.key_id) {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("Key id {key}"),
                frame.rect(SIDE_X, top + 66.0, SIDE_WIDTH, 22.0),
                15.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        let mut y = top + 110.0;
        kit::button(
            &mut self.ui,
            frame,
            [SIDE_X, y, 240.0, 42.0],
            if muted {
                "Unmute on this PC"
            } else {
                "Mute on this PC"
            },
            false,
            true,
            self.focus == MUTE_TOKEN,
            MUTE_TOKEN,
        );
        self.order.push(MUTE_TOKEN);
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Hides their chat, model, sabers and sounds on this PC."),
            frame.rect(SIDE_X, y + 50.0, SIDE_WIDTH, 22.0),
            14.0 * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::Start,
        );
        // The message's GIF, as its line shows it (none for a muted sender), can be made
        // a favourite for the GIF picker.
        if let (_, Some(gif)) = crate::chat_gifs::for_message(&message.text, muted) {
            let at = y + 84.0;
            let favourite = crate::chat_gifs::favourites::contains(&gif);
            kit::button(
                &mut self.ui,
                frame,
                [SIDE_X, at, 300.0, 42.0],
                if favourite {
                    "Remove GIF from favourites"
                } else {
                    "Add GIF to favourites"
                },
                false,
                true,
                self.focus == FAVOURITE_TOKEN,
                FAVOURITE_TOKEN,
            );
            self.order.push(FAVOURITE_TOKEN);
            self.shown.chosen_gif = Some(gif);
            // Staff's part goes under the button, as far below it as under Mute's line.
            y = at - 30.0;
        }
        if !inputs.staff {
            return;
        }
        y += 104.0;
        kit::heading(&mut self.ui, frame, SIDE_X, y, SIDE_WIDTH, "Staff");
        y += 24.0;
        for (index, (label, token)) in [
            ("Delete for everyone", DELETE_TOKEN),
            ("Mute at the hub", HUB_MUTE_TOKEN),
            ("Unmute at the hub", HUB_UNMUTE_TOKEN),
        ]
        .into_iter()
        .enumerate()
        {
            kit::button(
                &mut self.ui,
                frame,
                [SIDE_X, y + index as f32 * 54.0, 240.0, 42.0],
                label,
                false,
                true,
                self.focus == token,
                token,
            );
            self.order.push(token);
        }
        // What the page's last staff request came to, else what staff requests are.
        let answer = inputs
            .staff_state
            .zip(self.staff_after)
            .and_then(|(state, after)| {
                if state.serial > after {
                    Some(if state.failed {
                        (format!("Not done: {}", state.message), color::EMBER)
                    } else {
                        (format!("Done: {}", state.message), color::GOLD_BRIGHT)
                    })
                } else {
                    state
                        .busy
                        .then(|| ("Sending to the hub...".to_owned(), color::MUTED))
                }
            });
        let (line, colour) =
            answer.unwrap_or_else(|| ("The hub logs what staff do.".to_owned(), color::QUIET));
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{line}"),
            frame.rect(SIDE_X, y + 3.0 * 54.0, SIDE_WIDTH, 22.0),
            14.0 * s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
        let _ = self.ui.draw_list_mut().push(DrawCommand::SolidRect {
            rect: frame.rect(SIDE_X - 30.0, LIST_TOP - 20.0, 1.0, LIST_BOTTOM - LIST_TOP),
            color: color::alpha(color::HOLO, 0.25),
        });
    }

    /// Show the sender card of the player whose name or face is under the pointer
    /// (`hovered`), keep it while the pointer is on the card, else hide it.
    fn follow_card(&mut self, hovered: Option<Shows>) {
        match hovered {
            Some((source, person, anchor)) => match &mut self.card {
                Some(card) if card.source == source => card.anchor = anchor,
                _ => {
                    self.card = Some(Hovered {
                        source,
                        person,
                        anchor,
                        placed: false,
                    });
                }
            },
            None if self.ui.token_hovered(CARD_TOKEN) || self.ui.token_hovered(CARD_MUTE_TOKEN) => {
            }
            None => self.card = None,
        }
    }

    /// The sender card on show, beside its name and over the page; its targets last.
    fn card(
        &mut self,
        frame: &Frame,
        inputs: &Inputs<'_>,
        measure: &Measure<'_>,
        viewport: [f32; 2],
    ) {
        let Some(card) = &self.card else {
            self.shown.card_muted = false;
            return;
        };
        let muted = card
            .person
            .key_id
            .as_ref()
            .is_some_and(|key| inputs.muted.contains(key));
        self.shown.card_muted = muted;
        let s = frame.s;
        let size = crate::sender_card::size(&card.person, s);
        // A player of the who-is-online window has their card left of the window, over
        // the chat's quiet top, rather than over the window's other faces (text draws
        // over every shape, so a card over them would show their initials through it).
        let origin = match card.source {
            CardSource::Person(_) => [
                (frame.point(PEOPLE_X, 0.0)[0] - 12.0 * s - size[0]).max(0.0),
                card.anchor.y.min(viewport[1] - size[1]).max(0.0),
            ],
            CardSource::Message(_) => {
                crate::sender_card::beside(card.anchor, size, viewport, 12.0 * s)
            }
        };
        // Text draws over every shape: the page's text under the card is left out, or
        // the times and messages showed through it (Sol, 11/10/2026).
        leave_out_text_under(
            &mut self.ui,
            measure,
            sjk_ui::Rect::new(origin[0], origin[1], size[0], size[1]),
        );
        crate::sender_card::draw(
            &mut self.ui,
            &crate::sender_card::Card {
                person: &card.person,
                muted,
                measure: Some(measure),
            },
            origin,
            s,
            crate::sender_card::Tokens {
                card: CARD_TOKEN,
                mute: CARD_MUTE_TOKEN,
            },
        );
    }

    /// The keys of the page, bottom right: the picker's while it is open.
    fn keys(&mut self, frame: &Frame) {
        let s = frame.s;
        let keys: [(&[&str], &str); 4] = if self.gifs.key_card.is_some() {
            [
                (&["Ctrl", "V"], "paste"),
                (&["Enter"], "save"),
                (&["Tab"], "next"),
                (&["Esc"], "not now"),
            ]
        } else if self.gifs.open {
            [
                (&["Enter"], "pick"),
                (&["F"], "favourite"),
                (&["PgUp", "PgDn"], "scroll"),
                (&["Esc"], "close GIFs"),
            ]
        } else {
            [
                (&["Enter"], "send"),
                (&["Up", "Down"], "messages"),
                (&["PgUp", "PgDn"], "scroll"),
                (&["Esc"], "back"),
            ]
        };
        let gap = 30.0 * s;
        let width: f32 = keys
            .iter()
            .map(|(caps, action)| key_hint_width(caps, action, s))
            .sum::<f32>()
            + gap * (keys.len() - 1) as f32;
        let [right, y] = frame.point(1_824.0, KEYS_Y);
        let mut x = right - width;
        for (caps, action) in keys {
            x = key_hint(&mut self.ui, caps, action, x, y, s) + gap;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{chat, inputs, message};
    use super::*;
    use crate::text::{TextStyle, UiFont};
    use sjk_ui::{Color, Rect};

    impl Panel {
        /// [`Panel::build`] measuring in `font` without a text style.
        pub(in crate::console) fn build_with(
            &mut self,
            inputs: &Inputs<'_>,
            font: &UiFont,
            viewport: [f32; 2],
        ) {
            self.build(inputs, &Measure::new(font, TextStyle::NEUTRAL), viewport);
        }
    }

    /// Every text the page drew, with its rectangle and colour.
    fn texts(panel: &Panel) -> Vec<(String, Rect, Color)> {
        panel
            .ui
            .draw_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text {
                    rect, text, color, ..
                } => Some((panel.ui.stored_text(*text).to_owned(), *rect, *color)),
                _ => None,
            })
            .collect()
    }

    /// The verified ticks in the messages' column (at 1920 by 1080), the who-is-online
    /// window's left out.
    fn ticks(panel: &Panel) -> usize {
        panel
            .ui
            .draw_list()
            .commands()
            .iter()
            .filter(|command| {
                matches!(command, DrawCommand::TexturedQuad { texture, rect, .. }
                    if *texture == crate::ui_renderer::VERIFIED_TEXTURE && rect.x < SIDE_X - 30.0)
            })
            .count()
    }

    fn found<'a>(texts: &'a [(String, Rect, Color)], text: &str) -> &'a (String, Rect, Color) {
        texts
            .iter()
            .find(|(drawn, ..)| drawn == text)
            .unwrap_or_else(|| panic!("{text:?} not in {texts:?}"))
    }

    #[test]
    fn a_message_flows_after_its_name_in_gold() {
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        let mut state = chat(0);
        let mut short = message(1, "aaaaaaaaaaaaaaaa", "gg ^1all");
        short.name = "^2Sol".to_owned();
        short.verified = true;
        // As long as the hub takes, in wide letters: more than the column's row.
        let long = message(2, "bbbbbbbbbbbbbbbb", &"WMWMWMWM ".repeat(17)[..150]);
        state.messages.extend([short, long.clone()]);
        let mut panel = Panel::new();
        panel.open(true);
        panel.build_with(&inputs(&state, &[], false), &fonts.font, [1920.0, 1080.0]);
        let texts = texts(&panel);
        let (_, name, _) = found(&texts, "^2Sol");
        let (_, body, colour) = found(&texts, "gg all");
        // Centred on the same line (the name's text is larger): the name's row.
        assert!((name.y - body.y).abs() < 4.0, "on the name's row");
        assert!(body.x > name.right());
        assert_eq!(*colour, sjk_chat_look::GOLD);
        // The name keeps its codes and is not gold.
        assert_ne!(found(&texts, "^2Sol").2, sjk_chat_look::GOLD);
        // The verified sender has the tick, and no word says so.
        assert_eq!(ticks(&panel), 1);
        assert!(
            texts
                .iter()
                .all(|(text, ..)| !text.to_ascii_lowercase().contains("verified")),
            "{texts:?}"
        );
        // The long one wraps: its rows after the first start at the column's edge.
        let (_, long_name, _) = found(&texts, &long.name);
        let rows: Vec<_> = texts
            .iter()
            .filter(|(_, rect, colour)| *colour == sjk_chat_look::GOLD && rect.y >= long_name.y)
            .collect();
        assert!(rows.len() >= 2, "{rows:?}");
        assert!((rows[0].1.y - long_name.y).abs() < 4.0);
        assert!(rows[1].1.y > rows[0].1.y + 10.0);
        assert!(rows[1].1.x <= long_name.x + 0.5);
    }

    /// A holocron drop reads `name gem sentence` in its tier's colour: no colon, no tick.
    #[test]
    fn a_holocron_drop_is_the_name_a_gem_and_the_sentence_in_the_tiers_colour() {
        use crate::holocrons::TIERS;
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        let mut state = chat(0);
        let mut drop = message(1, "aaaaaaaaaaaaaaaa", "");
        drop.name = "^2Sol".to_owned();
        drop.verified = true;
        drop.holocron = Some(sjk_identity::DropMark {
            tier: "legendary".to_owned(),
            own: false,
        });
        let mut future = message(2, "bbbbbbbbbbbbbbbb", "");
        future.holocron = Some(sjk_identity::DropMark {
            tier: "from_the_future".to_owned(),
            own: false,
        });
        state.messages.extend([drop, future]);
        let mut panel = Panel::new();
        panel.open(true);
        panel.build_with(&inputs(&state, &[], false), &fonts.font, [1920.0, 1080.0]);
        let texts = texts(&panel);
        let (_, name, _) = found(&texts, "^2Sol");
        let (_, body, colour) = found(&texts, "found a Legendary Holocron!");
        assert_eq!(*colour, TIERS[2].colour);
        assert!((name.y - body.y).abs() < 4.0 && body.x > name.right());
        assert!(!texts.iter().any(|(text, ..)| text == ":"), "{texts:?}");
        assert_eq!(ticks(&panel), 0, "the gem stands for the tick");
        // A tier this client does not know is still a line, worded plainly.
        found(&texts, "found a holocron.");
    }

    /// A GIPHY link shows "GIF" in the line and the GIF's block under it, inside the
    /// messages' column and over the next message, at 1080p and 4K; a muted sender's
    /// GIF is never asked for; one that could not be had is a quiet line.
    #[test]
    fn a_gif_shows_under_its_line_and_fits_the_column() {
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        let shown = "https://giphy.com/gifs/wave-PageGif01Loading";
        let failed = "https://media.giphy.com/media/PageGif02Failed/giphy.gif";
        let muted_link = "https://i.giphy.com/PageGif03Muted.gif";
        crate::chat_gifs::fail_for_shot(&crate::chat_gifs::GifId::new("PageGif02Failed").unwrap());
        for viewport in [[1920.0, 1080.0], [3840.0, 2160.0]] {
            let s = Frame::new(viewport).s;
            let mut state = chat(0);
            state.messages.extend([
                message(1, "cccccccccccccccc", muted_link),
                message(2, "aaaaaaaaaaaaaaaa", &format!("hi {shown}")),
                message(3, "bbbbbbbbbbbbbbbb", failed),
                message(4, "aaaaaaaaaaaaaaaa", "gg"),
            ]);
            let muted = ["cccccccccccccccc".to_owned()];
            let mut panel = Panel::new();
            panel.open(true);
            panel.build_with(&inputs(&state, &muted, false), &fonts.font, viewport);
            assert!(!panel.ui.overflowed());
            let texts = texts(&panel);
            let (_, label, _) = found(&texts, "hi GIF");
            let (_, loading, _) = found(&texts, "Loading GIF");
            let (_, unavailable, _) = found(&texts, "GIF unavailable");
            let (_, gg, _) = found(&texts, "gg");
            // Under its line, in the column, above the next message.
            assert!(loading.y > label.bottom(), "{loading:?} {label:?}");
            assert!(loading.x >= LIST_X * s - 0.5);
            assert!(loading.right() <= (LIST_X + LIST_WIDTH) * s + 0.5);
            assert!(
                loading.bottom() < unavailable.y,
                "{loading:?} {unavailable:?}"
            );
            assert!(unavailable.bottom() < gg.y);
            assert!(gg.bottom() <= LIST_BOTTOM * s + 0.5);
            // The placeholder is the GIF's height.
            let boxes: Vec<Rect> = panel
                .ui
                .draw_list()
                .commands()
                .iter()
                .filter_map(|command| match command {
                    DrawCommand::RoundedRect { rect, .. }
                        if (rect.height - GIF.height * s).abs() < 0.5 =>
                    {
                        Some(*rect)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(boxes.len(), 1, "{boxes:?}");
            // The muted sender's line says so, and their GIF was never asked for.
            found(&texts, "Muted on this PC");
            assert!(!crate::chat_gifs::asked_for(
                &crate::chat_gifs::GifId::new("PageGif03Muted").unwrap()
            ));
            assert!(crate::chat_gifs::asked_for(
                &crate::chat_gifs::GifId::new("PageGif01Loading").unwrap()
            ));
        }
    }

    /// Every focus, long messages, a muted sender, a refusal and staff fit the canvas
    /// at 1080p, 4K, 4:3 and 21:9, in the families and in Inter.
    #[test]
    fn every_state_fits_the_canvas() {
        let load = |family| crate::text::load_family(family, 1.0, None).expect("a family");
        let body = load(&crate::text::BODY);
        let inter = crate::text::load_modern(1.0, None).expect("Inter");
        let mut crowded = chat(120);
        for id in 121..140 {
            crowded.messages.push_back(message(
                id,
                "aaaaaaaaaaaaaaaa",
                &"long words make many rows here ".repeat(5),
            ));
        }
        crowded.outcome = Some(sjk_identity::ReportOutcome {
            serial: 1,
            sent: false,
            message: "x".repeat(200),
        });
        let muted = ["aaaaaaaaaaaaaaaa".to_owned()];
        let empty = sjk_identity::ChatState::default();
        for (state, staff) in [(&crowded, true), (&empty, false)] {
            let mut panel = Panel::new();
            panel.open(true);
            panel.draft = "d".repeat(150);
            panel.selected = Some(130);
            let inputs = inputs(state, &muted, staff);
            panel.build_with(&inputs, &inter.font, [1920.0, 1080.0]);
            for focus in panel.order.clone() {
                for font in [&body.font, &inter.font] {
                    for viewport in [
                        [1920.0, 1080.0],
                        [3840.0, 2160.0],
                        [1440.0, 1080.0],
                        [2560.0, 1080.0],
                    ] {
                        panel.focus = focus;
                        panel.build_with(&inputs, font, viewport);
                        assert!(!panel.ui.overflowed(), "{focus} at {viewport:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_staff_request_says_what_it_came_to() {
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        let state = chat(3);
        let mut panel = Panel::new();
        panel.open(true);
        panel.selected = Some(3);
        let runs =
            |panel: &Panel| -> Vec<String> { panel.ui.text_runs().map(str::to_owned).collect() };
        // Nothing sent from the page: no line, whatever the staff state holds.
        let old = StaffShown {
            serial: 4,
            message: "Gave bug_hunter",
            failed: false,
            busy: false,
        };
        let quiet = Inputs {
            staff_state: Some(old),
            ..inputs(&state, &[], true)
        };
        panel.build_with(&quiet, &fonts.font, [1920.0, 1080.0]);
        assert!(!runs(&panel).iter().any(|run| run == "Gave bug_hunter"));
        // Sent: waiting, then the hub's answer.
        panel.staff_sent(4);
        let waiting = StaffShown { busy: true, ..old };
        panel.build_with(
            &Inputs {
                staff_state: Some(waiting),
                ..inputs(&state, &[], true)
            },
            &fonts.font,
            [1920.0, 1080.0],
        );
        assert!(
            runs(&panel)
                .iter()
                .any(|run| run == "Sending to the hub...")
        );
        let refused = StaffShown {
            serial: 5,
            message: "you sent as many staff requests as you may for now; try again later",
            failed: true,
            busy: false,
        };
        panel.build_with(
            &Inputs {
                staff_state: Some(refused),
                ..inputs(&state, &[], true)
            },
            &fonts.font,
            [1920.0, 1080.0],
        );
        assert!(
            runs(&panel)
                .iter()
                .any(|run| run.starts_with("Not done: you sent as many staff")),
            "{:?}",
            runs(&panel)
        );
    }

    #[test]
    fn the_status_says_why_the_chat_is_silent() {
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        let mut panel = Panel::new();
        panel.open(true);
        let state = chat(0);
        let mut off = inputs(&state, &[], false);
        off.chat = None;
        panel.build_with(&off, &fonts.font, [1920.0, 1080.0]);
        assert!(
            panel
                .ui
                .text_runs()
                .any(|run| run.starts_with("The SJK identity is off"))
        );
        off.enabled = false;
        panel.build_with(&off, &fonts.font, [1920.0, 1080.0]);
        assert!(
            panel
                .ui
                .text_runs()
                .any(|run| run.starts_with("SJK chat is off"))
        );
        panel.build_with(&inputs(&state, &[], false), &fonts.font, [1920.0, 1080.0]);
        assert!(
            panel
                .ui
                .text_runs()
                .any(|run| run == "9 SJK players reading")
        );
    }

    fn listed_person(key: &str, name: &str, seen: i64, playing: bool) -> sjk_identity::Person {
        sjk_identity::Person {
            key_id: key.to_owned(),
            name: name.to_owned(),
            seen,
            playing,
            ..sjk_identity::Person::default()
        }
    }

    /// The who-is-online window's rectangle as drawn (its border).
    fn window(panel: &Panel, s: f32) -> Rect {
        panel
            .ui
            .draw_list()
            .commands()
            .iter()
            .find_map(|command| match command {
                DrawCommand::Border {
                    rect,
                    radius,
                    width,
                    color,
                } if (*radius - 14.0 * s).abs() < 0.01
                    && (*width - s).abs() < 0.01
                    && *color == color::alpha(color::HOLO, 0.25)
                    && rect.y > 150.0 * s =>
                {
                    Some(*rect)
                }
                _ => None,
            })
            .expect("the window")
    }

    /// The faces' squares, by token, as the last frame laid them out.
    fn faces(panel: &Panel) -> Vec<Rect> {
        (0..panel.shown.people.len())
            .map(|index| {
                panel
                    .ui
                    .rect_for(PERSON_BASE + index as u16)
                    .expect("a face")
            })
            .collect()
    }

    fn inside(rect: Rect, of: Rect) -> bool {
        rect.x >= of.x
            && rect.right() <= of.right()
            && rect.y >= of.y
            && rect.bottom() <= of.bottom()
    }

    /// Point the pointer at `at` and lay the page out again.
    fn point_at(panel: &mut Panel, inputs: &Inputs<'_>, font: &UiFont, at: [f32; 2]) {
        let _ = panel.handle_pointer(sjk_ui::InputEvent::PointerMove(sjk_ui::Vec2::new(
            at[0], at[1],
        )));
        panel.build_with(inputs, font, [1920.0, 1080.0]);
    }

    /// The online players as a crowd of `count`, Sol first (verified, with a picture,
    /// playing), then Zed (playing) and the others; two seen recently.
    fn crowd(count: usize) -> sjk_identity::ChatState {
        let mut state = chat(3);
        let now = 1_791_400_000_i64;
        let mut online = vec![
            sjk_identity::Person {
                verified: true,
                avatar: "0123456789abcdef".to_owned(),
                ..listed_person(&format!("{:016x}", 2), "^2Sol", now, true)
            },
            listed_person("aaaaaaaaaaaaaaaa", "^1Zed", now, true),
        ];
        for n in 2..count {
            online.push(listed_person(
                &format!("{:016x}", 0x100 + n),
                &format!("Player {n:02}"),
                now,
                false,
            ));
        }
        online.truncate(count);
        state.people = Some(sjk_identity::People {
            online,
            recent: vec![
                listed_person("ffffffffffffffff", "Bastila", now - 180, false),
                listed_person("1111111111111111", "^5jan", now - 7_200, false),
            ],
        });
        state
    }

    /// The window shows who is online as faces in a grid (playing first, the last face
    /// "+N" for the rest) and who was seen last as smaller ones, inside the window and
    /// with no time written; each face shows its initial until its picture loads, and
    /// a gold dot for a player in a match.
    #[test]
    fn the_window_shows_faces_in_a_grid_with_a_last_one_for_the_rest() {
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        let state = crowd(20);
        let mut panel = Panel::new();
        panel.open(true);
        let inputs = inputs(&state, &[], false);
        panel.build_with(&inputs, &fonts.font, [1920.0, 1080.0]);
        let drawn = texts(&panel);
        found(&drawn, "Online  \u{b7}  9");
        found(&drawn, "Recently active");
        // Twenty online in eighteen faces: seventeen, then "+3".
        let online = super::people::ONLINE_FACES - 1;
        assert_eq!(panel.shown.people.len(), online + 2, "and two recent");
        let (_, more, _) = found(&drawn, "+3");
        let shown = faces(&panel);
        let window = window(&panel, 1.0);
        for face in shown.iter().chain([more]) {
            assert!(inside(*face, window), "{face:?} in {window:?}");
        }
        assert!(inside(*more, window));
        // Nine across, the tenth under the first; the "+3" in the last cell.
        let (first, tenth) = (shown[0], shown[9]);
        assert_eq!((first.width, first.height), (40.0, 40.0));
        assert!((tenth.x - first.x).abs() < 0.01 && (tenth.y - first.y - 50.0).abs() < 0.01);
        assert!((shown[1].x - first.x - 50.0).abs() < 0.01);
        assert!((more.x - shown[8].x).abs() < 0.01);
        assert!(
            more.y >= tenth.y - 1.0 && more.y < tenth.bottom(),
            "{more:?}"
        );
        // No two faces touch.
        for (index, a) in shown.iter().enumerate() {
            for b in &shown[index + 1..] {
                assert!(
                    a.right() < b.x || b.right() < a.x || a.bottom() < b.y || b.bottom() < a.y,
                    "{a:?} and {b:?}"
                );
            }
        }
        // Recent faces are smaller, under their heading, under the online ones.
        let recent = shown[online];
        assert_eq!((recent.width, recent.height), (30.0, 30.0));
        let (_, heading, _) = found(&drawn, "Recently active");
        assert!(heading.y > tenth.bottom() && recent.y > heading.bottom() - 8.0);
        // Their initials, colour codes aside, centred on their faces.
        let (_, s_letter, _) = drawn
            .iter()
            .find(|(text, rect, _)| text == "S" && inside(*rect, window))
            .expect("Sol's initial");
        assert!((s_letter.x + s_letter.width * 0.5 - (first.x + 20.0)).abs() < 0.5);
        assert!(
            drawn
                .iter()
                .any(|(text, rect, _)| text == "J" && inside(*rect, window))
        );
        // Gold dots for the two in a match, and nothing written of when anyone was seen.
        let dots = panel
            .ui
            .draw_list()
            .commands()
            .iter()
            .filter(|command| {
                matches!(command, DrawCommand::RoundedRect { color, rect, .. }
                    if *color == color::GOLD_BRIGHT && inside(*rect, window))
            })
            .count();
        assert_eq!(dots, 2);
        assert!(
            !drawn.iter().any(|(text, rect, _)| inside(*rect, window)
                && (text.contains("ago") || text == "playing")),
            "{drawn:?}"
        );
        // The chosen message's heading under the window.
        let (_, chosen, _) = found(&drawn, "Chosen message");
        assert!(chosen.y > window.bottom());
    }

    /// The window is as tall as what it shows, so Chosen message comes up under a short
    /// one; every face stays in the window and on screen at 1080p, 4K, 4:3 and 21:9.
    #[test]
    fn the_window_shrinks_to_its_faces_and_fits_every_screen() {
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        let chosen_at = |state: &sjk_identity::ChatState| {
            let mut panel = Panel::new();
            panel.open(true);
            panel.build_with(&inputs(state, &[], false), &fonts.font, [1920.0, 1080.0]);
            let (_, chosen, _) = found(&texts(&panel), "Chosen message").clone();
            (window(&panel, 1.0), chosen)
        };
        let (full, full_chosen) = chosen_at(&crowd(20));
        let (one_row, one_row_chosen) = chosen_at(&crowd(5));
        assert!(
            (full.height - one_row.height - 50.0).abs() < 0.01,
            "one row less"
        );
        assert!((full_chosen.y - one_row_chosen.y - 50.0).abs() < 0.01);
        // Shorter than the window of names it replaced (318 tall, Chosen at 556).
        assert!(full.bottom() < 522.0 && one_row_chosen.y < 556.0 - 15.0 - 100.0);
        let state = crowd(20);
        let mut panel = Panel::new();
        panel.open(true);
        panel.selected = Some(3);
        let staff = inputs(&state, &[], true);
        for viewport in [
            [1920.0, 1080.0],
            [3840.0, 2160.0],
            [1440.0, 1080.0],
            [2560.0, 1080.0],
            [2560.0, 1440.0],
            [1024.0, 768.0],
        ] {
            panel.build_with(&staff, &fonts.font, viewport);
            assert!(!panel.ui.overflowed(), "{viewport:?}");
            let s = Frame::new(viewport).s;
            let window = window(&panel, s);
            let screen = Rect::new(0.0, 0.0, viewport[0], viewport[1]);
            assert!(inside(window, screen), "{viewport:?}");
            for face in faces(&panel) {
                assert!(
                    inside(face, window),
                    "{face:?} in {window:?} at {viewport:?}"
                );
                // A whole number of pixels across at 1080p and 4K: crisp discs.
                if viewport == [3840.0, 2160.0] {
                    assert!(face.width == 80.0 || face.width == 60.0, "{face:?}");
                }
            }
            // The chosen message's tools under the window and above the keys.
            let mute = panel.ui.rect_for(MUTE_TOKEN).expect("Mute");
            let unmute = panel
                .ui
                .rect_for(HUB_UNMUTE_TOKEN)
                .expect("Unmute at the hub");
            let keys = Frame::new(viewport).point(0.0, KEYS_Y - 20.0)[1];
            assert!(
                mute.y > window.bottom() && unmute.bottom() < keys,
                "{viewport:?}"
            );
        }
    }

    /// A face under the pointer shows that player's card, left of the window, with when
    /// they were about; the gap between two faces shows none; a click chooses their
    /// newest message; a muted player's face is greyed.
    #[test]
    fn a_face_under_the_pointer_shows_its_card() {
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        let state = crowd(5);
        let muted = ["aaaaaaaaaaaaaaaa".to_owned()];
        let mut panel = Panel::new();
        panel.open(true);
        let inputs = inputs(&state, &muted, false);
        panel.build_with(&inputs, &fonts.font, [1920.0, 1080.0]);
        let shown = faces(&panel);
        let centre = |rect: Rect| [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
        // Sol's face: their card, with the picture the hub listed and that they play.
        point_at(&mut panel, &inputs, &fonts.font, centre(shown[0]));
        let card = panel.card.as_ref().expect("the card");
        assert_eq!(card.source, CardSource::Person(format!("{:016x}", 2)));
        assert_eq!(card.person.avatar.as_deref(), Some("0123456789abcdef"));
        assert_eq!(card.person.note.as_deref(), Some("Online now, in a match"));
        assert!(card.person.verified);
        let target = panel.ui.rect_for(CARD_TOKEN).expect("the card's target");
        assert!(
            target.right() <= window(&panel, 1.0).x,
            "left of the window: {target:?}"
        );
        assert!(found(&texts(&panel), "Online now, in a match").1.right() <= target.right());
        // Lit: a gold ring round it.
        assert!(panel.ui.draw_list().commands().iter().any(|command| matches!(
            command,
            DrawCommand::Arc { color, center, .. }
                if *color == color::GOLD_BRIGHT && (center[0] - centre(shown[0])[0]).abs() < 0.01
        )));
        // Between two faces: no card.
        point_at(
            &mut panel,
            &inputs,
            &fonts.font,
            [shown[0].right() + 5.0, centre(shown[0])[1]],
        );
        assert!(panel.card.is_none());
        // A recent face: when they were seen, on the card only.
        let recent = shown[5];
        point_at(&mut panel, &inputs, &fonts.font, centre(recent));
        let card = panel.card.as_ref().expect("the recent card");
        assert_eq!(card.person.name, "Bastila");
        assert_eq!(card.person.note.as_deref(), Some("Seen 3 minutes ago"));
        found(&texts(&panel), "Seen 3 minutes ago");
        // Zed is muted here: a grey disc, and Unmute on the card.
        let zed = shown[1];
        assert!(
            panel
                .ui
                .draw_list()
                .commands()
                .iter()
                .any(|command| matches!(
                    command,
                    DrawCommand::RoundedRect { color, rect, .. }
                        if *color == color::alpha(color::QUIET, 0.22) && *rect == zed
                ))
        );
        // A click chooses their newest message on show; someone who said nothing, none.
        assert_eq!(panel.activate(PERSON_BASE), PanelAction::None);
        assert_eq!(panel.selected, Some(2));
        panel.selected = None;
        let _ = panel.activate(PERSON_BASE + 1);
        assert_eq!(panel.selected, None);
    }

    /// A sender's card over the history leaves out the text it covers (text draws over
    /// every shape, so the times and messages showed through it, Sol 11/10/2026); the
    /// rest of the page keeps its text.
    #[test]
    fn a_senders_card_leaves_out_the_text_it_covers() {
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        let state = chat(12);
        let mut panel = Panel::new();
        panel.open(true);
        let inputs = inputs(&state, &[], false);
        panel.build_with(&inputs, &fonts.font, [1920.0, 1080.0]);
        let name = panel
            .ui
            .rect_for(NAME_BASE + 6)
            .expect("a name in the middle of the history");
        let before = texts(&panel).len();
        point_at(
            &mut panel,
            &inputs,
            &fonts.font,
            [name.x + 4.0, name.y + name.height * 0.5],
        );
        assert!(matches!(
            panel.card.as_ref().map(|card| &card.source),
            Some(CardSource::Message(_))
        ));
        let card = panel.ui.rect_for(CARD_TOKEN).expect("the card's target");
        let shown = texts(&panel);
        let history: Vec<_> = (1..=12).map(|id| format!("message {id}")).collect();
        let page_text = |text: &str| {
            history.iter().any(|line| line == text) || text.ends_with(" ago") || text == "just now"
        };
        let under: Vec<_> = shown
            .iter()
            .filter(|(text, rect, _)| rect.overlaps(card) && page_text(text))
            .collect();
        assert!(under.is_empty(), "page text under the card: {under:?}");
        assert!(
            shown.iter().any(|(text, ..)| history.contains(text)),
            "the history away from the card still shows"
        );
        assert!(shown.len() < before + 20, "nothing doubled");
    }

    /// When the faces change under a pointer that does not move, what it rests on is
    /// looked for again: the targets after the faces moved in the frame's list, and
    /// without it the field's place would light a face and show its card.
    #[test]
    fn faces_changing_under_a_resting_pointer_do_not_light_another_target() {
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        let (few, many) = (crowd(5), crowd(20));
        let mut panel = Panel::new();
        panel.open(true);
        panel.build_with(&inputs(&few, &[], false), &fonts.font, [1920.0, 1080.0]);
        let field = panel.ui.rect_for(FIELD_TOKEN).expect("the field");
        let at = [field.x + 10.0, field.y + field.height * 0.5];
        point_at(&mut panel, &inputs(&few, &[], false), &fonts.font, at);
        assert!(panel.card.is_none());
        for _ in 0..3 {
            panel.build_with(&inputs(&many, &[], false), &fonts.font, [1920.0, 1080.0]);
            assert!(
                panel.card.is_none(),
                "{:?}",
                panel.card.as_ref().map(|card| &card.source)
            );
        }
        // Resting on a face while the list grows: the face now under it.
        let first = faces(&panel)[0];
        point_at(
            &mut panel,
            &inputs(&many, &[], false),
            &fonts.font,
            [first.x + 20.0, first.y + 20.0],
        );
        assert!(panel.card.is_some());
        for _ in 0..2 {
            panel.build_with(&inputs(&few, &[], false), &fonts.font, [1920.0, 1080.0]);
        }
        let card = panel.card.as_ref().expect("still a card");
        assert_eq!(card.source, CardSource::Person(format!("{:016x}", 2)));
    }

    /// A hub that does not list who is online: the count, and the last senders' faces,
    /// newest first; their pictures are looked up after the frame, and their card says
    /// when they last spoke.
    #[test]
    fn without_a_list_the_window_shows_the_count_and_the_last_senders() {
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        let state = chat(3);
        let mut panel = Panel::new();
        panel.open(true);
        let inputs = inputs(&state, &[], false);
        panel.build_with(&inputs, &fonts.font, [1920.0, 1080.0]);
        let drawn = texts(&panel);
        found(&drawn, "Online  \u{b7}  9");
        found(&drawn, "This hub counts them but does not say who.");
        found(&drawn, "Recently in chat");
        let keys: Vec<String> = [3, 2, 1].iter().map(|id| format!("{id:016x}")).collect();
        assert_eq!(panel.shown.people, keys, "newest first");
        let shown = faces(&panel);
        assert!(shown.windows(2).all(|pair| pair[0].x < pair[1].x));
        assert_eq!(shown[0].width, 30.0, "recent faces");
        // Their pictures are not known: asked after the frame, kept once found.
        assert_eq!(panel.shown.unpictured, keys);
        let asked = std::cell::RefCell::new(Vec::new());
        panel.find_pictures(|key| {
            asked.borrow_mut().push(key.to_owned());
            (key == keys[1]).then(|| "0123456789abcdef".to_owned())
        });
        assert_eq!(*asked.borrow(), keys);
        panel.build_with(&inputs, &fonts.font, [1920.0, 1080.0]);
        assert_eq!(panel.shown.unpictured, [keys[0].clone(), keys[2].clone()]);
        let centre = |rect: Rect| [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
        point_at(&mut panel, &inputs, &fonts.font, centre(shown[1]));
        let card = panel.card.as_ref().expect("the card");
        assert_eq!(card.person.avatar.as_deref(), Some("0123456789abcdef"));
        // Player 2 spoke at 1_791_300_120, the page's now is 1_791_400_000.
        assert_eq!(card.person.note.as_deref(), Some("Last spoke yesterday"));
        // Not reading: the window says why it is empty, and is only that tall.
        let mut quiet = state.clone();
        quiet.live = false;
        panel.build_with(&inputs_for(&quiet), &fonts.font, [1920.0, 1080.0]);
        let drawn = texts(&panel);
        found(&drawn, "Shows once the SJK hub answers.");
        found(&drawn, "Online");
        assert!(!drawn.iter().any(|(text, ..)| text == "Recently active"));
        assert!(panel.shown.people.is_empty());
    }

    fn inputs_for(chat: &sjk_identity::ChatState) -> Inputs<'_> {
        inputs(chat, &[], false)
    }
}
