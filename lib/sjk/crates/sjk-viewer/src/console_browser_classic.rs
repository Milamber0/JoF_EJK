//! The console browser in the classic+ style (`docs/classic-plus.md`), drawn while
//! the console is the classic one (`con_style classic`). Retail had no such screen,
//! so it takes the frame of the in-game pop-ups (`ingame_setup.menu`): the
//! `menu_box_ingame` box over the dimmed screen, the `menu_blendbox` title band,
//! gold buttons with the `menu_buttonback` glow, a retail list box whose selected
//! row sits on `menu_blendbox2`, and the description line under the box.
//!
//! SJK adds the classic+ parts: a detail box for the selected entry (its kind,
//! value and default, and its whole description), the "shown of total" count,
//! and a description for every control. Every control answers to the
//! browser's tokens ([`super::pointer`]), so the keyboard, pointer and wheel
//! handling serve both looks.

use super::pointer::{
    ACTIVATE_TOKEN, FILTER_TOKEN, RESET_TOKEN, ROW_BASE, SCROLLBAR_TOKEN, SEARCH_TOKEN,
};
use super::{Browser, Entry, Kind, TABS};
use crate::menu::art::{ArtPiece, ArtSet};
use crate::menu::classic::layout::Placement;
use crate::menu::classic::view::{self, Caps, DISABLED, FOCUS, GOLD, HINT};
use crate::menu_widgets::{BACK_TOKEN, MenuCanvas, TAB_BASE};
use crate::text::{TextVertex, UiFont};
use sjk_shell::CommandSource;
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};

/// Retail title colour (`forecolor .549 .854 1`).
pub(crate) const LABEL: Color = Color::new(0.549, 0.854, 1.0, 1.0);
/// Retail field value colour (`forecolor .615 .615 .956`).
pub(crate) const VALUE: Color = Color::new(0.615, 0.615, 0.956, 1.0);
/// Retail option label colour (`setup.menu` items, `.65 .65 1`).
pub(crate) const OPTION: Color = Color::new(0.65, 0.65, 1.0, 1.0);
/// Retail frame blue (`bordercolor .298 .305 .690`).
pub(crate) const FRAME: Color = Color::new(0.298, 0.305, 0.690, 1.0);
/// Retail list box (`backcolor .66 .66 1 .25`, border `.66 .66 1`).
pub(crate) const LIST_BACK: Color = Color::new(0.66, 0.66, 1.0, 0.25);
pub(crate) const LIST_BORDER: Color = Color::new(0.66, 0.66, 1.0, 1.0);
/// A refused edit or unknown value, in retail's warning red.
const ERROR: Color = Color::new(1.0, 0.3, 0.3, 1.0);

/// The pop-up box, from `ingame_setup.menu`'s `570`-wide box, taller to hold the
/// list and the detail box (canvas units).
const BOX: [f32; 4] = [35.0, 26.0, 570.0, 414.0];
/// The title band.
const TITLE: [f32; 4] = [55.0, 32.0, 530.0, 22.0];
/// The filter buttons' row: x, y, width of one button, height.
const TAB_ROW: [f32; 4] = [55.0, 60.0, 92.0, 16.0];
/// The search field.
const SEARCH: [f32; 4] = [55.0, 82.0, 530.0, 18.0];
/// The list box and its rows.
const LIST: [f32; 4] = [55.0, 106.0, 530.0, 196.0];
const ROW_HEIGHT: f32 = 16.0;
/// Rows the list box holds.
pub(super) const LIST_ROWS: usize = 12;
/// The detail box for the selected entry.
const DETAIL: [f32; 4] = [55.0, 308.0, 530.0, 98.0];
/// Lines of description the detail box holds, and characters per line.
const DETAIL_LINES: usize = 4;
const DETAIL_LINE_CHARS: usize = 92;
/// The action buttons' row.
const BUTTONS: [f32; 4] = [55.0, 414.0, 530.0, 20.0];
/// The description line and the status line under the box.
const HINT_Y: f32 = 446.0;
const STATUS_Y: f32 = 462.0;

// The description and status lines sit under the box and on the canvas, and the
// list box holds its rows.
const _: () = assert!(BOX[1] + BOX[3] < HINT_Y && STATUS_Y + 13.0 <= 480.0);
const _: () = assert!(2.0 + LIST_ROWS as f32 * ROW_HEIGHT <= LIST[3]);

/// One action button: its token and its label for the current state.
struct Button {
    token: u16,
    label: &'static str,
    enabled: bool,
    hint: &'static str,
}

impl Browser {
    /// Draw the classic+ browser over the whole frame. Overlay text draws
    /// above every overlay's shapes, so menus and chat build no text while it
    /// is open (see `Console::covers_frame`), and any other text appended
    /// earlier this frame is dropped rather than shown through the browser.
    /// The SJK UI's look is drawn by `append_sjk` instead.
    pub(crate) fn append(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        vertices.clear();
        self.rows = LIST_ROWS;
        self.first = self.first.min(self.visible.len().saturating_sub(self.rows));
        let place = Placement::new(viewport);
        let art = self.art;
        let mut hint: Option<&'static str> = None;

        self.ui.begin_transparent(viewport);
        frame(&mut self.ui, viewport, &place, art);
        self.tabs_view(&place, art, &mut hint);
        self.classic_search(&place, &mut hint);
        self.list_view(&place, art, &mut hint);
        self.detail_view(&place);
        self.buttons_view(&place, art, &mut hint);
        self.footer_view(&place, hint);

        let selected = self.selected.saturating_sub(self.first);
        self.ui
            .finish(ROW_BASE + selected.min(usize::from(u16::MAX - ROW_BASE)) as u16);
        self.ui.append_text(vertices, font, viewport);
    }

    /// The four filters as retail buttons, and the count at the row's end.
    fn tabs_view(&mut self, place: &Placement, art: ArtSet, hint: &mut Option<&'static str>) {
        let [x, y, width, height] = TAB_ROW;
        for (index, label) in TABS.iter().enumerate() {
            let token = TAB_BASE + index as u16;
            let rect = place.rect([x + index as f32 * (width + 4.0), y, width, height]);
            let hovered = self.ui.token_hovered(token);
            if hovered {
                view::glow(&mut self.ui, rect, place.scale, art);
                *hint = Some(TAB_HINTS[index]);
            }
            let color = match (hovered, index == self.tab) {
                (true, _) => view::focus_pulse(),
                (false, true) => FOCUS,
                (false, false) => GOLD,
            };
            text(
                &mut self.ui,
                place,
                format_args!("{}", Caps(label)),
                [x + index as f32 * (width + 4.0), y, width, height],
                12.0,
                color,
                FontWeight::Semibold,
                TextAlign::Center,
            );
            self.ui.hit_region(token, rect);
        }
        text(
            &mut self.ui,
            place,
            format_args!("{}", self.summary),
            [
                x + 4.0 * (width + 4.0),
                y,
                LIST[0] + LIST[2] - x - 4.0 * (width + 4.0),
                height,
            ],
            10.5,
            VALUE,
            FontWeight::Regular,
            TextAlign::End,
        );
    }

    /// "Search:" and the filter, underlined while it takes the keys.
    fn classic_search(&mut self, place: &Placement, hint: &mut Option<&'static str>) {
        let [x, y, width, height] = SEARCH;
        let typing = self.editing.is_none();
        let rect = place.rect(SEARCH);
        if self.ui.token_hovered(SEARCH_TOKEN) {
            *hint = Some("Type to search names and descriptions; Backspace erases.");
        }
        text(
            &mut self.ui,
            place,
            format_args!("{}", Caps("Search:")),
            [x, y, 70.0, height],
            12.0,
            OPTION,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let field = [x + 74.0, y, width - 74.0, height];
        if self.filter.is_empty() {
            let prompt = if typing {
                "type a name or a word_"
            } else {
                "type a name or a word"
            };
            text(
                &mut self.ui,
                place,
                format_args!("{prompt}"),
                field,
                11.5,
                with_alpha(VALUE, 0.7),
                FontWeight::Regular,
                TextAlign::Start,
            );
        } else {
            text(
                &mut self.ui,
                place,
                format_args!("{}{}", self.filter, if typing { "_" } else { "" }),
                field,
                11.5,
                if typing { FOCUS } else { VALUE },
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        if typing {
            fill(
                &mut self.ui,
                place.rect([field[0], y + height - 1.0, field[2], 1.0]),
                with_alpha(LIST_BORDER, 0.8),
            );
        }
        self.ui.hit_region(SEARCH_TOKEN, rect);
    }

    /// The list box: name, value and default, or a command's tag, per row.
    fn list_view(&mut self, place: &Placement, art: ArtSet, hint: &mut Option<&'static str>) {
        let s = place.scale;
        let box_rect = place.rect(LIST);
        fill(&mut self.ui, box_rect, LIST_BACK);
        let hovered_list = (0..self.rows).any(|slot| self.ui.token_hovered(ROW_BASE + slot as u16));
        border(
            &mut self.ui,
            box_rect,
            s,
            if hovered_list { FOCUS } else { LIST_BORDER },
        );
        let scrolls = self.visible.len() > self.rows;
        let row_width = LIST[2] - if scrolls { 14.0 } else { 4.0 };
        let shown = self.first..self.visible.len().min(self.first + self.rows);
        for (slot, position) in shown.enumerate() {
            let token = ROW_BASE + slot as u16;
            let row = [
                LIST[0] + 2.0,
                LIST[1] + 2.0 + slot as f32 * ROW_HEIGHT,
                row_width,
                ROW_HEIGHT,
            ];
            let selected = position == self.selected;
            if selected {
                if art.has(ArtPiece::BlendBox2) {
                    view::art(&mut self.ui, ArtPiece::BlendBox2, place.rect(row));
                } else {
                    view::soft_band(&mut self.ui, place.rect(row), 0.3);
                }
            }
            if self.ui.token_hovered(token) {
                *hint = Some(row_hint(&self.entries[self.visible[position]]));
            }
            self.classic_row(place, row, position, selected);
            self.ui.hit_region(token, place.rect(row));
        }
        if self.visible.is_empty() {
            text(
                &mut self.ui,
                place,
                format_args!("No command or cvar matches the search."),
                [LIST[0] + 8.0, LIST[1] + 6.0, LIST[2] - 16.0, ROW_HEIGHT],
                11.5,
                VALUE,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        if scrolls {
            let track = [LIST[0] + LIST[2] - 8.0, LIST[1] + 3.0, 3.0, LIST[3] - 6.0];
            let rect = place.rect(track);
            fill(&mut self.ui, rect, view::ink(0.6));
            let thumb = thumb(track, self.first, self.rows, self.visible.len());
            let active = self.ui.token_hovered(SCROLLBAR_TOKEN);
            fill(
                &mut self.ui,
                place.rect(thumb),
                if active { FOCUS } else { view::gold(0.8) },
            );
            // Wider than the bar so it can be caught.
            self.ui.scroll_region(
                SCROLLBAR_TOKEN,
                place.rect([track[0] - 3.0, track[1], track[2] + 6.0, track[3]]),
            );
        }
    }

    /// One row's cells.
    fn classic_row(&mut self, place: &Placement, row: [f32; 4], position: usize, selected: bool) {
        let [x, y, width, height] = row;
        let entry = &self.entries[self.visible[position]];
        text(
            &mut self.ui,
            place,
            format_args!("{}", entry.name),
            [x + 4.0, y, width * 0.42 - 4.0, height],
            11.5,
            if selected { view::focus_pulse() } else { VALUE },
            if selected {
                FontWeight::Semibold
            } else {
                FontWeight::Regular
            },
            TextAlign::Start,
        );
        let value = [x + width * 0.43, y, width * 0.30, height];
        match &entry.kind {
            Kind::Cvar {
                value: current,
                default,
                ..
            } => {
                if let Some(edit) = self.editing.as_deref().filter(|_| selected) {
                    text(
                        &mut self.ui,
                        place,
                        format_args!("{edit}_"),
                        value,
                        11.5,
                        FOCUS,
                        FontWeight::Semibold,
                        TextAlign::Start,
                    );
                    fill(
                        &mut self.ui,
                        place.rect([value[0], y + height - 2.0, value[2], 1.0]),
                        FOCUS,
                    );
                } else {
                    text(
                        &mut self.ui,
                        place,
                        format_args!("{current}"),
                        value,
                        11.5,
                        if entry.changed() { GOLD } else { VALUE },
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                }
                // The default only where it differs: the detail box always has it.
                if entry.changed() {
                    text(
                        &mut self.ui,
                        place,
                        format_args!("default {default}"),
                        [x + width * 0.74, y, width * 0.26 - 4.0, height],
                        10.5,
                        DISABLED,
                        FontWeight::Regular,
                        TextAlign::End,
                    );
                }
            }
            Kind::Command(source) => {
                text(
                    &mut self.ui,
                    place,
                    format_args!("{}", command_tag(*source)),
                    [x + width * 0.74, y, width * 0.26 - 4.0, height],
                    9.5,
                    GOLD,
                    FontWeight::Semibold,
                    TextAlign::End,
                );
            }
        }
    }

    /// The selected entry: name and kind, value and default, its description.
    fn detail_view(&mut self, place: &Placement) {
        let s = place.scale;
        let [x, y, width, height] = DETAIL;
        let rect = place.rect(DETAIL);
        fill(&mut self.ui, rect, view::ink(0.45));
        border(&mut self.ui, rect, s, FRAME);
        fill(
            &mut self.ui,
            place.rect([x + 6.0, y + 19.0, width - 12.0, 1.0]),
            with_alpha(FRAME, 0.7),
        );
        let Some(entry) = self.visible.get(self.selected).map(|&i| &self.entries[i]) else {
            text(
                &mut self.ui,
                place,
                format_args!("Nothing selected: search or change the filter."),
                [x + 6.0, y + 3.0, width - 12.0, 14.0],
                11.0,
                VALUE,
                FontWeight::Regular,
                TextAlign::Start,
            );
            return;
        };
        text(
            &mut self.ui,
            place,
            format_args!("{}", entry.name),
            [x + 6.0, y + 3.0, width * 0.6, 15.0],
            14.0,
            GOLD,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        text(
            &mut self.ui,
            place,
            format_args!("{}", Caps(kind_tag(entry))),
            [x + width * 0.6, y + 3.0, width * 0.4 - 6.0, 15.0],
            10.0,
            GOLD,
            FontWeight::Semibold,
            TextAlign::End,
        );
        let mut line_y = y + 23.0;
        if let Kind::Cvar { value, default, .. } = &entry.kind {
            text(
                &mut self.ui,
                place,
                format_args!("Value: {value}      Default: {default}"),
                [x + 6.0, line_y, width - 12.0, 13.0],
                11.5,
                if entry.changed() { GOLD } else { LABEL },
                FontWeight::Regular,
                TextAlign::Start,
            );
            line_y += 14.0;
        }
        let description = if entry.description.is_empty() {
            "No description."
        } else {
            entry.description.as_str()
        };
        for line in wrap::<DETAIL_LINES>(description, DETAIL_LINE_CHARS) {
            if line.is_empty() || line_y > y + height - 14.0 {
                break;
            }
            text(
                &mut self.ui,
                place,
                format_args!("{line}"),
                [x + 6.0, line_y, width - 12.0, 13.0],
                11.0,
                VALUE,
                FontWeight::Regular,
                TextAlign::Start,
            );
            line_y += 13.0;
        }
    }

    /// Edit or insert, default, filter, close: gold buttons, grey when they
    /// cannot act.
    fn buttons_view(&mut self, place: &Placement, art: ArtSet, hint: &mut Option<&'static str>) {
        let entry = self.visible.get(self.selected).map(|&i| &self.entries[i]);
        let editing = self.editing.is_some();
        let cvar = entry.map(|entry| &entry.kind);
        let resettable = matches!(
            cvar,
            Some(Kind::Cvar {
                read_only: false,
                ..
            })
        );
        let buttons = [
            Button {
                token: ACTIVATE_TOKEN,
                label: match (editing, cvar) {
                    (true, _) => "Apply",
                    (false, Some(Kind::Command(_))) => "Insert",
                    _ => "Edit",
                },
                enabled: entry.is_some(),
                hint: match (editing, cvar) {
                    (true, _) => "Apply the typed value (Enter).",
                    (false, Some(Kind::Command(_))) => {
                        "Back to the console with this command on the line (Enter)."
                    }
                    _ => "Type a new value for this cvar (Enter).",
                },
            },
            Button {
                token: RESET_TOKEN,
                label: "Default",
                enabled: resettable && !editing,
                hint: "Put this cvar back to its default (Delete).",
            },
            Button {
                token: FILTER_TOKEN,
                label: "Filter",
                enabled: !editing,
                hint: "Show the next filter (Tab; Shift+Tab goes back).",
            },
            Button {
                token: BACK_TOKEN,
                label: if editing { "Cancel" } else { "Close" },
                enabled: true,
                hint: if editing {
                    "Stop typing and keep the old value (Esc)."
                } else {
                    "Back to the console (Esc)."
                },
            },
        ];
        let [x, y, width, height] = BUTTONS;
        let cell = width / buttons.len() as f32;
        for (index, button) in buttons.iter().enumerate() {
            let target = [x + index as f32 * cell + 8.0, y, cell - 16.0, height];
            let rect = place.rect(target);
            let hovered = self.ui.token_hovered(button.token);
            if hovered {
                *hint = Some(button.hint);
                if button.enabled {
                    view::glow(&mut self.ui, rect, place.scale, art);
                }
            }
            let color = match (button.enabled, hovered) {
                (false, _) => DISABLED,
                (true, true) => view::focus_pulse(),
                (true, false) => GOLD,
            };
            text(
                &mut self.ui,
                place,
                format_args!("{}", Caps(button.label)),
                target,
                14.0,
                color,
                FontWeight::Semibold,
                TextAlign::Center,
            );
            self.ui.hit_region(button.token, rect);
        }
    }

    /// The description line, then the last action's outcome.
    fn footer_view(&mut self, place: &Placement, hint: Option<&'static str>) {
        text(
            &mut self.ui,
            place,
            format_args!(
                "{}",
                hint.unwrap_or(
                    "Up and Down choose, Enter edits or inserts, Tab filters, Esc closes."
                )
            ),
            [0.0, HINT_Y, 640.0, 14.0],
            12.0,
            HINT,
            FontWeight::Regular,
            TextAlign::Center,
        );
        if !self.status.is_empty() {
            text(
                &mut self.ui,
                place,
                format_args!("{}", self.status),
                [0.0, STATUS_Y, 640.0, 13.0],
                11.0,
                if self.status_error { ERROR } else { VALUE },
                FontWeight::Regular,
                TextAlign::Center,
            );
        }
    }
}

/// Descriptions of the filter buttons, in [`TABS`] order.
const TAB_HINTS: [&str; 4] = [
    "Show every command and cvar.",
    "Show commands only.",
    "Show cvars only.",
    "Show the cvars changed from their default.",
];

/// The dimmed screen and the pop-up box with its title band.
fn frame(canvas: &mut MenuCanvas, viewport: [f32; 2], place: &Placement, art: ArtSet) {
    fill(
        canvas,
        Rect::new(0.0, 0.0, viewport[0], viewport[1]),
        view::ink(0.82),
    );
    let rect = place.rect(BOX);
    if art.has(ArtPiece::PopupBox) {
        view::art(canvas, ArtPiece::PopupBox, rect);
    } else {
        fill(canvas, rect, view::ink(0.9));
        border(canvas, rect, place.scale, FRAME);
    }
    let band = place.rect(TITLE);
    if art.has(ArtPiece::BlendBox) {
        view::art(canvas, ArtPiece::BlendBox, band);
    } else {
        view::soft_band(canvas, band, 0.16);
    }
    text(
        canvas,
        place,
        format_args!("COMMANDS & CVARS"),
        [TITLE[0], TITLE[1] + 3.0, TITLE[2], TITLE[3] - 6.0],
        12.5,
        LABEL,
        FontWeight::Semibold,
        TextAlign::Center,
    );
}

/// What a row does, for the description line.
fn row_hint(entry: &Entry) -> &'static str {
    match entry.kind {
        Kind::Command(_) => "Enter puts this command on the console line.",
        Kind::Cvar {
            read_only: true, ..
        } => "Read-only: the game sets this cvar.",
        Kind::Cvar { .. } => "Enter edits the value; Delete restores the default.",
    }
}

/// The short tag a command's row shows where a cvar shows its default.
fn command_tag(source: CommandSource) -> &'static str {
    match source {
        CommandSource::External => "SERVER",
        _ => "COMMAND",
    }
}

/// The selected entry's kind, at the detail box's title line.
fn kind_tag(entry: &Entry) -> &'static str {
    match entry.kind {
        Kind::Command(CommandSource::External) => "Server command",
        Kind::Command(_) => "Command",
        Kind::Cvar {
            read_only: true, ..
        } => "Cvar, read-only",
        Kind::Cvar { archived: true, .. } => "Cvar, saved",
        Kind::Cvar { .. } => "Cvar",
    }
}

/// `text` in at most `N` lines of `chars` characters, cut at spaces; the last line
/// takes what is left.
fn wrap<const N: usize>(text: &str, chars: usize) -> [&str; N] {
    let mut out = [""; N];
    let mut rest = text.trim();
    for (index, line) in out.iter_mut().enumerate() {
        if rest.chars().count() <= chars || index + 1 == N {
            *line = rest;
            break;
        }
        let limit = rest
            .char_indices()
            .nth(chars)
            .map_or(rest.len(), |(at, _)| at);
        let cut = rest[..limit]
            .rfind(' ')
            .filter(|&cut| cut > 0)
            .unwrap_or(limit);
        *line = rest[..cut].trim_end();
        rest = rest[cut..].trim_start();
    }
    out
}

/// The scrollbar thumb inside `track` for a window of `visible` rows from `first`
/// over `total`, at least as tall as the bar is wide.
pub(in crate::console) fn thumb(
    track: [f32; 4],
    first: usize,
    visible: usize,
    total: usize,
) -> [f32; 4] {
    let [x, y, width, height] = track;
    let length = (height * visible as f32 / total.max(visible).max(1) as f32).max(width * 3.0);
    let travel = (height - length).max(0.0);
    let span = total.saturating_sub(visible).max(1) as f32;
    [
        x,
        y + travel * (first as f32 / span).clamp(0.0, 1.0),
        width,
        length,
    ]
}

pub(crate) fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha)
}

pub(crate) fn fill(canvas: &mut MenuCanvas, rect: Rect, color: Color) {
    let _ = canvas
        .draw_list_mut()
        .push(DrawCommand::SolidRect { rect, color });
}

pub(crate) fn border(canvas: &mut MenuCanvas, rect: Rect, scale: f32, color: Color) {
    let _ = canvas.draw_list_mut().push(DrawCommand::Border {
        rect,
        radius: 0.0,
        width: scale.max(1.0),
        color,
    });
}

/// Text at canvas size `size`, centred vertically in canvas box `box_`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn text(
    canvas: &mut MenuCanvas,
    place: &Placement,
    content: std::fmt::Arguments<'_>,
    box_: [f32; 4],
    size: f32,
    color: Color,
    weight: FontWeight,
    align: TextAlign,
) {
    let [x, y, width, height] = box_;
    let line = size * 1.25;
    canvas.text_fmt_aligned(
        content,
        place.rect([x, y + (height - line) * 0.5, width, line]),
        size * place.scale,
        color,
        weight,
        0.3 * place.scale,
        align,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside(inner: [f32; 4], outer: [f32; 4]) -> bool {
        inner[0] >= outer[0]
            && inner[1] >= outer[1]
            && inner[0] + inner[2] <= outer[0] + outer[2]
            && inner[1] + inner[3] <= outer[1] + outer[3]
    }

    #[test]
    fn every_part_lies_inside_the_box_and_the_canvas() {
        let tabs = [
            TAB_ROW[0],
            TAB_ROW[1],
            4.0 * (TAB_ROW[2] + 4.0) - 4.0,
            TAB_ROW[3],
        ];
        for part in [TITLE, tabs, SEARCH, LIST, DETAIL, BUTTONS] {
            assert!(inside(part, BOX), "{part:?}");
        }
        assert!(inside(BOX, [0.0, 0.0, 640.0, 480.0]));
    }

    #[test]
    fn parts_follow_each_other_without_overlapping() {
        let order = [TITLE, TAB_ROW, SEARCH, LIST, DETAIL, BUTTONS];
        for pair in order.windows(2) {
            assert!(pair[0][1] + pair[0][3] <= pair[1][1], "{pair:?}");
        }
    }

    #[test]
    fn descriptions_wrap_at_spaces_and_keep_the_rest() {
        let lines = wrap::<3>("one two three four five six", 9);
        assert_eq!(lines, ["one two", "three", "four five six"]);
        assert_eq!(wrap::<2>("short", 40), ["short", ""]);
        // A word longer than a line is cut where the line ends.
        assert_eq!(wrap::<2>("abcdefghij klm", 4), ["abcd", "efghij klm"]);
    }

    #[test]
    fn the_thumb_stays_on_its_track() {
        let track = [100.0, 10.0, 3.0, 190.0];
        for first in [0, 10, 88] {
            let [_, y, _, length] = thumb(track, first, 12, 100);
            assert!(y >= track[1] && y + length <= track[1] + track[3] + 0.001);
        }
        assert_eq!(thumb(track, 0, 12, 5)[3], track[3]);
    }
}
