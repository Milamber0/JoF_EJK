//! Drawing of the debug panel in the SJK UI's look, in every menu style (as the
//! Profile and SJK chat pages): the way back and "Test list" at the top with the
//! tabs under them and how many are tested at the top right; the entries down the
//! left, one row each (tick box, title, area), the chosen one on a band; the chosen
//! entry's details in a reading column on the right (area, title, its tick, what
//! changed, the steps to test numbered in gold, notes on a gold band); the last tick's outcome bottom left and the keys bottom right. Laid out
//! in frame pixels ([`Frame`]) and measured in the families with the player's text
//! style, so it holds at every window size and `ui_textScale`. Text goes into the
//! canvas' retained slots and the wrapped lines into a reused list, so a steady
//! frame allocates nothing.

use super::pointer::{BACK_TOKEN, DETAIL_TICK_TOKEN, PANE_WHEEL_TOKEN, ROW_BASE, ROW_LIMIT};
use super::pointer::{SCROLLBAR_TOKEN, TAB_BASE, TOGGLE_TOKEN, list_tokens};
use super::{Panel, TABS};
use crate::menu::sjk::{Frame, TextTarget, color, key_hint, key_hint_width, kit, text, top_bar};
use crate::menu_widgets::TextFamily;
use crate::sjk_chat_look::{Measure, flow_each};
use crate::text::TextFace;
use sjk_ui::{DrawCommand, FontWeight, Rect, TextAlign};
use std::ops::Range;

/// The list of entries (frame pixels): its left edge, width, first row and where
/// it stops, and the step from row to row.
const LIST_X: f32 = 96.0;
const LIST_WIDTH: f32 = 600.0;
const LIST_TOP: f32 = 214.0;
const LIST_BOTTOM: f32 = 950.0;
const ROW: f32 = 60.0;
/// A row's band, a little shorter than its step.
const ROW_BAND: f32 = 54.0;
/// Where a row's text starts, after its tick box.
const ROW_TEXT: f32 = 62.0;
/// The reading column.
const DETAIL_X: f32 = 790.0;
const DETAIL_WIDTH: f32 = 1_034.0;
const DETAIL_TOP: f32 = 210.0;
const DETAIL_BOTTOM: f32 = 950.0;
/// Sizes: a row's title and area, the entry's title, the column's text.
const ROW_TITLE_SIZE: f32 = 18.0;
const ROW_AREA_SIZE: f32 = 15.0;
const TITLE_SIZE: f32 = 36.0;
const BODY_SIZE: f32 = 18.0;
/// A line of the column's text, before the text style's scale.
const LINE: f32 = 28.0;
/// A step's text starts after its number.
const INDENT: f32 = 30.0;
/// The keys' line and the status's.
const KEYS_Y: f32 = 992.0;
/// The right edge of the frame's content.
const RIGHT: f32 = 1_824.0;

/// The detail column's sections and their headings.
const SECTIONS: [(&str, Section); 3] = [
    ("What changed", Section::Changes),
    ("To test", Section::Tests),
    ("Note", Section::Notes),
];

/// Which list of the entry a line comes from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Section {
    Changes,
    Tests,
    Notes,
}

/// One wrapped line of the chosen entry's column: `range` of item `item` of its
/// section; `first` for an item's first line (a step's gets its number).
#[derive(Clone, Debug)]
pub(super) struct Line {
    section: Section,
    item: usize,
    range: Range<usize>,
    first: bool,
}

impl Panel {
    /// Draw the panel over the whole frame, its text to `target`.
    pub(crate) fn append_sjk(&mut self, target: TextTarget<'_>, viewport: [f32; 2]) {
        let measure = target.body_measure();
        self.build(&measure, viewport);
        target.append(&self.ui, viewport);
    }

    /// Lay the page out on the canvas, its text measured as `measure` draws.
    fn build(&mut self, measure: &Measure<'_>, viewport: [f32; 2]) {
        let frame = Frame::new(viewport);
        self.rows = (((LIST_BOTTOM - LIST_TOP) / ROW).floor().max(1.0) as usize).min(ROW_LIMIT);
        self.first = self.first.min(self.visible.len().saturating_sub(self.rows));

        self.ui.begin_transparent(viewport);
        crate::settings::sjk_view::backdrop(&mut self.ui, viewport);
        top_bar(&mut self.ui, &frame, "Back", BACK_TOKEN, "Test list", None);
        crate::profile_hub::tabs(&mut self.ui, &frame, &TABS, self.tab, TAB_BASE);
        self.summary_view(&frame);
        self.list_view(&frame, measure);
        self.ui.scroll_region(
            PANE_WHEEL_TOKEN,
            frame.rect(
                DETAIL_X,
                DETAIL_TOP,
                DETAIL_WIDTH,
                DETAIL_BOTTOM - DETAIL_TOP,
            ),
        );
        if let Some(index) = self.selected_entry() {
            self.detail_view(&frame, measure, index);
        }
        let keys_x = self.keys_view(&frame);
        self.status_view(&frame, measure, keys_x);
        let selected = self.selected.saturating_sub(self.first);
        self.ui
            .finish(ROW_BASE + selected.min(ROW_LIMIT - 1) as u16);
    }

    /// How many are tested, at the top right, over whose list it is.
    fn summary_view(&mut self, frame: &Frame) {
        let s = frame.s;
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{}", self.summary),
            frame.rect(RIGHT - 600.0, 62.0, 600.0, 30.0),
            24.0 * s,
            color::GOLD_BRIGHT,
            FontWeight::Regular,
            TextAlign::End,
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Sol's build only"),
            frame.rect(RIGHT - 600.0, 94.0, 600.0, 22.0),
            15.0 * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::End,
        );
    }

    /// The entries the tab keeps, one row each, with the scrollbar beside them;
    /// or what an empty tab means.
    fn list_view(&mut self, frame: &Frame, measure: &Measure<'_>) {
        let s = frame.s;
        if self.visible.is_empty() {
            let message = match self.tab {
                0 if !self.entries.is_empty() => "Everything in the list is ticked as tested.",
                1 => "Nothing is ticked yet.",
                _ => "The test list is empty.",
            };
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{message}"),
                frame.rect(LIST_X, LIST_TOP + 8.0, LIST_WIDTH, 28.0),
                BODY_SIZE * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
            return;
        }
        let shown = self.first..self.visible.len().min(self.first + self.rows);
        for (slot, position) in shown.enumerate() {
            self.row_view(frame, measure, slot, position);
        }
        if self.visible.len() > self.rows {
            self.ui.scrollbar(
                SCROLLBAR_TOKEN,
                frame.rect(
                    LIST_X + LIST_WIDTH + 16.0,
                    LIST_TOP,
                    4.0,
                    self.rows as f32 * ROW - (ROW - ROW_BAND),
                ),
                self.first,
                self.rows,
                self.visible.len(),
            );
        }
    }

    /// One entry: its tick box, its title (cut with an ellipsis when too long)
    /// and its area; the chosen one on a band, its title gold.
    fn row_view(&mut self, frame: &Frame, measure: &Measure<'_>, slot: usize, position: usize) {
        let s = frame.s;
        let top = LIST_TOP + slot as f32 * ROW;
        let index = self.visible[position];
        let chosen = position == self.selected;
        let ticked = self.tested[index];
        let (row_token, tick_token) = list_tokens(slot);
        let hovered = self.ui.token_hovered(row_token) || self.ui.token_hovered(tick_token);
        if chosen {
            kit::band(&mut self.ui, frame, [LIST_X, top, LIST_WIDTH, ROW_BAND]);
        } else if hovered {
            let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: frame.rect(LIST_X, top, LIST_WIDTH, ROW_BAND),
                radius: 10.0 * s,
                color: color::alpha(color::HOLO, 0.045),
            });
        }
        self.ui
            .hit_region(row_token, frame.rect(LIST_X, top, LIST_WIDTH, ROW_BAND));
        let middle = top + ROW_BAND * 0.5;
        let tick_x = LIST_X + 20.0;
        kit::tick(&mut self.ui, frame, tick_x, middle, ticked, false);
        self.ui.hit_region(
            tick_token,
            frame.rect(tick_x - 10.0, top, kit::TICK_BOX + 20.0, ROW_BAND),
        );

        let entry = &self.entries[index];
        let room = LIST_WIDTH - ROW_TEXT - 14.0;
        let title = entry.title.as_str();
        let cut = measure.fitting(title, room * s, ROW_TITLE_SIZE * s, TextFace::Regular);
        let colour = match (chosen, hovered, ticked) {
            (true, _, _) => color::GOLD_BRIGHT,
            (false, true, _) => color::TEXT,
            (false, false, true) => color::QUIET,
            (false, false, false) => color::alpha(color::TEXT, 0.9),
        };
        let title_rect = frame.rect(LIST_X + ROW_TEXT, top + 5.0, room, 26.0);
        if cut < title.len() {
            let ellipsis = measure.width("...", ROW_TITLE_SIZE * s, TextFace::Regular);
            let cut = measure.fitting(
                title,
                room * s - ellipsis,
                ROW_TITLE_SIZE * s,
                TextFace::Regular,
            );
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{}...", title[..cut].trim_end()),
                title_rect,
                ROW_TITLE_SIZE * s,
                colour,
                FontWeight::Regular,
                TextAlign::Start,
            );
        } else {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{title}"),
                title_rect,
                ROW_TITLE_SIZE * s,
                colour,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{}", entry.area),
            frame.rect(LIST_X + ROW_TEXT, top + 31.0, room, 20.0),
            ROW_AREA_SIZE * s,
            if chosen { color::GOLD } else { color::QUIET },
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    /// The chosen entry in the reading column: its area, its title (on two lines
    /// at most), its tick, then what changed, the steps to test and the notes,
    /// cut off at the column's bottom.
    fn detail_view(&mut self, frame: &Frame, measure: &Measure<'_>, index: usize) {
        let s = frame.s;
        let style = measure.scale().max(1.0);
        let line = LINE * style;
        let mut y = DETAIL_TOP;
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{}", self.entries[index].area),
            frame.rect(DETAIL_X, y, DETAIL_WIDTH, 24.0),
            19.0 * s,
            color::GOLD,
            FontWeight::Regular,
            TextAlign::Start,
        );
        y += 30.0;

        // The title on up to two lines, the second ending in an ellipsis when
        // the title is longer still.
        let title = self.entries[index].title.as_str();
        let title_line = 44.0 * style;
        let fits = |value: &str, width: f32| display_fitting(value, width, TITLE_SIZE * s);
        let mut lines: [Range<usize>; 2] = [0..0, 0..0];
        let mut count = 0;
        let mut more = false;
        flow_each(
            title,
            DETAIL_WIDTH * s,
            DETAIL_WIDTH * s,
            fits,
            |range| match lines.get_mut(count) {
                Some(slot) => {
                    *slot = range;
                    count += 1;
                }
                None => more = true,
            },
        );
        for (number, range) in lines.iter().take(count).enumerate() {
            let rect = frame.rect(DETAIL_X, y, DETAIL_WIDTH, title_line);
            let last = number + 1 == count && more;
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}{}", &title[range.clone()], if last { "..." } else { "" }),
                rect,
                TITLE_SIZE * s,
                color::TEXT,
                FontWeight::Semibold,
                TextAlign::Start,
            );
            y += title_line;
        }
        y += 14.0;

        let ticked = self.tested[index];
        let middle = y + 16.0;
        let end = kit::tick(&mut self.ui, frame, DETAIL_X, middle, ticked, false);
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{}", if ticked { "Tested" } else { "Not tested" }),
            frame.rect(end + 14.0, middle - 15.0, 140.0, 30.0),
            22.0 * s,
            if ticked {
                color::GOLD_BRIGHT
            } else {
                color::TEXT
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        let hint = if ticked {
            "Space or click to untick"
        } else {
            "Space or click to tick"
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{hint}"),
            frame.rect(end + 150.0, middle - 13.0, 420.0, 26.0),
            15.0 * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::Start,
        );
        self.ui.hit_region(
            DETAIL_TICK_TOKEN,
            frame.rect(DETAIL_X - 8.0, middle - 22.0, 290.0, 44.0),
        );
        y += 62.0;

        self.wrap_detail(measure, index, s);
        for (heading, section) in SECTIONS {
            let Some(start) = self.lines.iter().position(|l| l.section == section) else {
                continue;
            };
            if y + 30.0 + line > DETAIL_BOTTOM {
                break;
            }
            let note = section == Section::Notes;
            let in_section = self.lines[start..]
                .iter()
                .take_while(|l| l.section == section)
                .count();
            let room = (((DETAIL_BOTTOM - y - 30.0) / line).floor().max(0.0)) as usize;
            let shown = in_section.min(room);
            if note {
                // The notes on a soft gold band with a gold bar, so they stand out.
                let height = 30.0 + shown as f32 * line + 12.0;
                let band = frame.rect(DETAIL_X - 16.0, y - 20.0, DETAIL_WIDTH + 32.0, height);
                let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: band,
                    radius: 12.0 * s,
                    color: color::alpha(color::GOLD, 0.08),
                });
                let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: frame.rect(DETAIL_X - 16.0, y - 8.0, 4.0, height - 24.0),
                    radius: 2.0 * s,
                    color: color::GOLD_BRIGHT,
                });
                kit::heading_in(
                    &mut self.ui,
                    frame,
                    DETAIL_X,
                    y,
                    DETAIL_WIDTH,
                    heading,
                    color::GOLD_BRIGHT,
                );
            } else {
                kit::heading(&mut self.ui, frame, DETAIL_X, y, DETAIL_WIDTH, heading);
            }
            y += 26.0;
            for at in start..start + shown {
                let Line {
                    item,
                    ref range,
                    first,
                    ..
                } = self.lines[at];
                let range = range.clone();
                let source = match section {
                    Section::Changes => &self.entries[index].changes[item],
                    Section::Tests => &self.entries[index].tests[item],
                    Section::Notes => &self.entries[index].notes[item],
                };
                let indent = if section == Section::Tests {
                    INDENT
                } else {
                    0.0
                };
                let rect = frame.rect(DETAIL_X + indent, y, DETAIL_WIDTH - indent, line);
                let cut = at + 1 == start + shown && shown < in_section;
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{}{}", &source[range], if cut { " ..." } else { "" }),
                    rect,
                    BODY_SIZE * s,
                    match section {
                        Section::Changes => color::alpha(color::TEXT, 0.88),
                        Section::Tests => color::TEXT,
                        Section::Notes => color::GOLD_BRIGHT,
                    },
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                if first && section == Section::Tests {
                    text(
                        &mut self.ui,
                        TextFamily::Display,
                        format_args!("{}", item + 1),
                        frame.rect(DETAIL_X, y, INDENT - 6.0, line),
                        19.0 * s,
                        color::GOLD,
                        FontWeight::Semibold,
                        TextAlign::Start,
                    );
                }
                y += line;
            }
            y += 30.0;
        }
    }

    /// Wrap the chosen entry's sections to the column, measured as drawn, into
    /// the reused list of lines.
    fn wrap_detail(&mut self, measure: &Measure<'_>, index: usize, s: f32) {
        self.lines.clear();
        let room = |section| {
            (DETAIL_WIDTH
                - if section == Section::Tests {
                    INDENT
                } else {
                    0.0
                })
                * s
        };
        let fits = |value: &str, width: f32| {
            measure.fitting(value, width, BODY_SIZE * s, TextFace::Regular)
        };
        let entry = &self.entries[index];
        for (_, section) in SECTIONS {
            let items = match section {
                Section::Changes => &entry.changes,
                Section::Tests => &entry.tests,
                Section::Notes => &entry.notes,
            };
            for (item, value) in items.iter().enumerate() {
                let mut first = true;
                let room = room(section);
                flow_each(value, room, room, fits, |range| {
                    self.lines.push(Line {
                        section,
                        item,
                        range,
                        first,
                    });
                    first = false;
                });
            }
        }
    }

    /// The keys, right-aligned at the bottom; the tick key answers to the
    /// pointer as well. Returns where they start (window pixels).
    fn keys_view(&mut self, frame: &Frame) -> f32 {
        let s = frame.s;
        let keys: [(&[&str], &str); 4] = [
            (&["Space"], self.toggle_hint()),
            (&["Tab"], "filter"),
            (&["Up", "Down"], "entry"),
            (&["Esc"], "back"),
        ];
        let gap = 30.0 * s;
        let width: f32 = keys
            .iter()
            .map(|(caps, action)| key_hint_width(caps, action, s))
            .sum::<f32>()
            + gap * (keys.len() - 1) as f32;
        let [right, y] = frame.point(RIGHT, KEYS_Y);
        let start = right - width;
        let mut x = start;
        for (number, (caps, action)) in keys.into_iter().enumerate() {
            let end = key_hint(&mut self.ui, caps, action, x, y, s);
            if number == 0 {
                self.ui
                    .hit_region(TOGGLE_TOKEN, Rect::new(x, y, end - x, 24.0 * s));
            }
            x = end + gap;
        }
        start
    }

    /// The last tick's outcome, or why the list or the ticks could not be read
    /// or saved (ember), at the bottom left up to the keys at `keys_x`.
    fn status_view(&mut self, frame: &Frame, measure: &Measure<'_>, keys_x: f32) {
        if self.status.is_empty() {
            return;
        }
        let s = frame.s;
        let [x, y] = frame.point(LIST_X, KEYS_Y);
        let room = (keys_x - x - 30.0 * s).max(0.0);
        let status = self.status.as_str();
        let cut = measure.fitting(status, room, 15.0 * s, TextFace::Regular);
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!(
                "{}{}",
                &status[..cut],
                if cut < status.len() { "..." } else { "" }
            ),
            Rect::new(x, y, room, 24.0 * s),
            15.0 * s,
            if self.status_error {
                color::EMBER
            } else {
                color::MUTED
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    /// The key hint of the tick key for the selected entry.
    fn toggle_hint(&self) -> &'static str {
        match self.selected_entry() {
            Some(index) if self.tested[index] => "untick",
            _ => "tick as tested",
        }
    }
}

/// How many bytes of `value` fit `width` (window pixels) in the display family at
/// `size`, in the player's text style: at least one character.
fn display_fitting(value: &str, width: f32, size: f32) -> usize {
    let style = crate::text::style::current();
    let size = size * style.scale;
    let tracking = style.tracking * size;
    let mut used = 0.0;
    for (at, c) in value.char_indices() {
        let end = at + c.len_utf8();
        used += crate::text::display_width(&value[at..end], size) + tracking;
        if used > width && at > 0 {
            return at;
        }
    }
    value.len()
}
