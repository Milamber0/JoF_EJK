//! The text dialog and its Report a bug button in the classic+ style
//! (`docs/classic-plus.md`), drawn while the menus are classic (`ui_menuStyle classic`).
//! Retail had no such screen, so the dialog takes the frame of the in-game pop-ups as the
//! Identity page does: the `menu_box_ingame` box over the dimmed screen, the `menu_blendbox`
//! title band, a retail list box holding the text, gold buttons with the `menu_buttonback`
//! glow, and the description line under the box. The button is a gold retail button on the
//! dark side's red band (`menu_blendboxr`), centred under the in-game bar's pop-ups.
//!
//! SJK adds the count of characters against the limit, the rules a report follows, why a
//! Send was refused, and a description for every control. Every control answers to the
//! dialog's tokens, so the keyboard and the pointer serve both looks.

use super::field::Face;
use super::sjk::Sentence;
use super::{
    CANCEL_TOKEN, CLEAR_MODEL_TOKEN, FIELD_TOKEN, Focus, Kind, LAUNCH_TOKEN, SEND_TOKEN, TextDialog,
};
use crate::console::browser::classic::{
    FRAME, LABEL, LIST_BACK, LIST_BORDER, VALUE, border, fill, text, with_alpha,
};
use crate::menu::art::{ArtPiece, ArtSet};
use crate::menu::classic::layout::Placement;
use crate::menu::classic::view::{self, Caps, FOCUS, GOLD, HINT};
use crate::menu_widgets::MenuCanvas;
use crate::text::{TextVertex, UiFont};
use sjk_ui::{Color, FontWeight, Rect, TextAlign};

/// The pop-up box and its title band (canvas units).
const BOX: [f32; 4] = [80.0, 100.0, 480.0, 240.0];
const TITLE: [f32; 4] = [100.0, 106.0, 440.0, 22.0];
/// What to write about: the question for a report, the selection for a note.
const PROMPT: [f32; 4] = [100.0, 132.0, 440.0, 14.0];
/// The list box holding the text, its rows and their height.
const FIELD: [f32; 4] = [100.0, 150.0, 440.0, 120.0];
const LINES: usize = 8;
const LINE: f32 = 14.0;
/// Canvas width of one character of the prompt, to cut it.
const CHAR_WIDTH: f32 = 6.0;
/// The rules and the count, then why a Send was refused.
const INFO: [f32; 4] = [100.0, 274.0, 440.0, 12.0];
const MESSAGE: [f32; 4] = [100.0, 290.0, 440.0, 12.0];
/// The gold buttons.
const SEND: [f32; 4] = [190.0, 312.0, 120.0, 20.0];
const CANCEL: [f32; 4] = [330.0, 312.0, 120.0, 20.0];
/// A note's Clear model, left of Send while the note has a model.
const CLEAR_MODEL: [f32; 4] = [96.0, 312.0, 84.0, 20.0];
/// The description line under the box.
const HINT_Y: f32 = 348.0;
/// The Report a bug button, centred at the bottom of the canvas.
const LAUNCH: [f32; 4] = [245.0, 448.0, 150.0, 20.0];
/// A refused Send, in retail's warning red.
const ERROR: Color = Color::new(1.0, 0.3, 0.3, 1.0);

// The text rows fit their box, the box and the description line fit the canvas.
const _: () = assert!(LINES as f32 * LINE + 8.0 <= FIELD[3]);
const _: () = assert!(BOX[1] + BOX[3] < HINT_Y && HINT_Y + 14.0 <= LAUNCH[1]);
const _: () = assert!(LAUNCH[1] + LAUNCH[3] <= 480.0);
// Clear model, Send and Cancel share a row, side by side, inside the box.
const _: () = assert!(SEND[0] + SEND[2] <= CANCEL[0] && SEND[1] == CANCEL[1]);
const _: () = assert!(CLEAR_MODEL[0] + CLEAR_MODEL[2] <= SEND[0] && CLEAR_MODEL[1] == SEND[1]);
const _: () = assert!(CLEAR_MODEL[0] >= BOX[0]);

impl TextDialog {
    /// Draw the classic+ dialog over the whole frame (see [`TextDialog::append`]).
    pub(super) fn append_classic(
        &mut self,
        kind: &Kind,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        let place = Placement::new(viewport);
        let art = self.art;
        let report = *kind == Kind::Report;
        let mut hint: Option<&'static str> = None;
        self.card = Some(place.rect(BOX));
        let ui = &mut self.ui;
        ui.begin_transparent(viewport);

        // The dimmed screen, the pop-up box and its title band.
        fill(
            ui,
            Rect::new(0.0, 0.0, viewport[0], viewport[1]),
            view::ink(0.82),
        );
        let rect = place.rect(BOX);
        if art.has(ArtPiece::PopupBox) {
            view::art(ui, ArtPiece::PopupBox, rect);
        } else {
            fill(ui, rect, view::ink(0.9));
            border(ui, rect, place.scale, FRAME);
        }
        let band = place.rect(TITLE);
        if art.has(ArtPiece::BlendBox) {
            view::art(ui, ArtPiece::BlendBox, band);
        } else {
            view::soft_band(ui, band, 0.16);
        }
        let title = match kind {
            Kind::Report => "Report a bug",
            Kind::Note { .. } => "Note for Claude",
            Kind::PlayerReport { .. } => "Report a player",
        };
        text(
            ui,
            &place,
            format_args!("{}", Caps(title)),
            [TITLE[0], TITLE[1] + 3.0, TITLE[2], TITLE[3] - 6.0],
            12.5,
            LABEL,
            FontWeight::Semibold,
            TextAlign::Center,
        );
        let prompt = match kind {
            Kind::Report => "What went wrong? Where were you, and what did you expect?",
            Kind::Note { subject, .. } | Kind::PlayerReport { subject, .. } => subject.as_str(),
        };
        let room = (PROMPT[2] / CHAR_WIDTH) as usize;
        text(
            ui,
            &place,
            format_args!("{}", super::clip(prompt, room)),
            PROMPT,
            11.0,
            GOLD,
            FontWeight::Semibold,
            TextAlign::Start,
        );

        // The text, in a retail list box.
        let focused = self.focus == Focus::Field;
        let hovered = ui.token_hovered(FIELD_TOKEN);
        if hovered {
            hint = Some(match kind {
                Kind::Report => "Say what you did, what you saw and what you expected.",
                Kind::Note { .. } => {
                    "Write what to fix; the note keeps the surface, the position and a screenshot."
                }
                Kind::PlayerReport { .. } => {
                    "Say what the player did; the report keeps who, where and when."
                }
            });
        }
        let field = place.rect(FIELD);
        fill(ui, field, LIST_BACK);
        border(
            ui,
            field,
            place.scale,
            if focused || hovered {
                FOCUS
            } else {
                LIST_BORDER
            },
        );
        let caret = focused && (self.epoch.elapsed().as_millis() / 500).is_multiple_of(2);
        let [x, y, width, _] = FIELD;
        // The wrap, the caret and the clicks measure as the text is drawn: at this size and
        // spacing, in the player's text style.
        let face = Face::new(font, font.style(), 11.0 * place.scale, 0.3 * place.scale);
        self.layout
            .lay_out(&self.text, &face, (width - 12.0) * place.scale);
        let (caret_line, caret_x) = self.layout.locate(&self.text, self.edit.cursor(&self.text));
        let top = place.rect([x + 6.0, y + 4.0, width - 12.0, LINE]);
        self.layout
            .show(caret_line, LINES, [top.x, top.y], LINE * place.scale);
        let (first, shown) = self.layout.visible();
        if self.text.is_empty() && !focused {
            text(
                ui,
                &place,
                format_args!("Type here"),
                [x + 6.0, y + 4.0, width - 12.0, LINE],
                11.0,
                with_alpha(VALUE, 0.7),
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        for (row, range) in shown.iter().enumerate() {
            text(
                ui,
                &place,
                format_args!("{}", &self.text[range.clone()]),
                [x + 6.0, y + 4.0 + row as f32 * LINE, width - 12.0, LINE],
                11.0,
                VALUE,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        if caret && caret_line >= first {
            // A bar between two glyphs, level with the text.
            let bar = place.scale.max(1.0);
            let height = face.size() * 1.05;
            let row = top.y + (caret_line - first) as f32 * LINE * place.scale;
            fill(
                ui,
                Rect::new(
                    top.x + caret_x - bar * 0.5,
                    row + (LINE * place.scale - height) * 0.5,
                    bar,
                    height,
                ),
                GOLD,
            );
        }
        ui.hit_region(FIELD_TOKEN, field);

        // The rules, the count and why a Send was refused.
        let rules = match kind {
            Kind::Note { .. } => "Saved with a screenshot for Claude to read",
            _ => "Letters, digits, spaces and . , ! ? ' - : ( ) only",
        };
        text(
            ui,
            &place,
            format_args!("{rules}"),
            INFO,
            9.5,
            with_alpha(VALUE, 0.8),
            FontWeight::Regular,
            TextAlign::Start,
        );
        let count = self.text.chars().count();
        let limit = super::limit(kind);
        text(
            ui,
            &place,
            format_args!("{count} / {limit}"),
            INFO,
            10.0,
            if count >= limit { GOLD } else { VALUE },
            FontWeight::Regular,
            TextAlign::End,
        );
        // Why a Send was refused, or what it would be refused for while the pointer or the
        // keyboard is on Send.
        let previewing = self.focus == Focus::Send || ui.token_hovered(SEND_TOKEN);
        let reason = if !self.message.is_empty() {
            Some(self.message.as_str())
        } else if previewing {
            super::refusal(kind, &self.text)
        } else {
            None
        };
        if let Some(reason) = reason {
            fill(
                ui,
                place.rect([MESSAGE[0], MESSAGE[1] - 1.0, MESSAGE[2], MESSAGE[3] + 2.0]),
                with_alpha(ERROR, 0.16),
            );
            text(
                ui,
                &place,
                format_args!("{}", Sentence(reason)),
                [MESSAGE[0] + 4.0, MESSAGE[1], MESSAGE[2] - 8.0, MESSAGE[3]],
                10.0,
                ERROR,
                FontWeight::Semibold,
                TextAlign::Start,
            );
        }

        let send_help = if report {
            "Send the report to the SJK hub, signed with your SJK identity."
        } else {
            "Save the note and its screenshot."
        };
        button(
            ui,
            &place,
            art,
            SEND_TOKEN,
            "Send",
            SEND,
            self.focus == Focus::Send,
            send_help,
            &mut hint,
        );
        button(
            ui,
            &place,
            art,
            CANCEL_TOKEN,
            "Cancel",
            CANCEL,
            self.focus == Focus::Cancel,
            "Close without sending (Esc).",
            &mut hint,
        );
        if matches!(kind, Kind::Note { model: Some(_), .. }) {
            button(
                ui,
                &place,
                art,
                CLEAR_MODEL_TOKEN,
                "Clear",
                CLEAR_MODEL,
                self.focus == Focus::ClearModel,
                "Clear the model: the note is about what is behind it.",
                &mut hint,
            );
        }
        let idle = match kind {
            Kind::Note { .. } => {
                "Enter saves, Tab moves, Esc cancels; click the scene to note something else."
            }
            _ => "Enter sends, Tab moves between the controls, Esc cancels.",
        };
        text(
            ui,
            &place,
            format_args!("{}", hint.unwrap_or(idle)),
            [0.0, HINT_Y, 640.0, 14.0],
            12.0,
            HINT,
            FontWeight::Regular,
            TextAlign::Center,
        );
        ui.finish(match self.focus {
            Focus::Field => FIELD_TOKEN,
            Focus::Send => SEND_TOKEN,
            Focus::Cancel => CANCEL_TOKEN,
            Focus::ClearModel => CLEAR_MODEL_TOKEN,
        });
        ui.append_text(vertices, font, viewport);
    }

    /// Draw the classic+ Report a bug button (see [`TextDialog::append_launcher`]).
    pub(super) fn append_launcher_classic(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        let place = Placement::new(viewport);
        let art = self.art;
        let ui = &mut self.launcher;
        ui.begin_transparent(viewport);
        let rect = place.rect(LAUNCH);
        if art.has(ArtPiece::BlendBoxRed) {
            view::art(ui, ArtPiece::BlendBoxRed, rect);
        } else {
            fill(ui, rect, Color::new(0.6, 0.08, 0.06, 0.35));
        }
        let hovered = ui.token_hovered(LAUNCH_TOKEN);
        if hovered {
            view::glow(ui, rect, place.scale, art);
        }
        text(
            ui,
            &place,
            format_args!("REPORT A BUG"),
            LAUNCH,
            13.0,
            if hovered { view::focus_pulse() } else { GOLD },
            FontWeight::Semibold,
            TextAlign::Center,
        );
        ui.hit_region(LAUNCH_TOKEN, rect);
        ui.finish(0);
        ui.append_text(vertices, font, viewport);
    }
}

/// One gold button: its glow and a pulsing white while focused or hovered.
#[allow(clippy::too_many_arguments)]
fn button(
    ui: &mut MenuCanvas,
    place: &Placement,
    art: ArtSet,
    token: u16,
    label: &str,
    rect: [f32; 4],
    focused: bool,
    help: &'static str,
    hint: &mut Option<&'static str>,
) {
    let area = place.rect(rect);
    let hovered = ui.token_hovered(token);
    if hovered {
        *hint = Some(help);
    }
    if hovered || focused {
        view::glow(ui, area, place.scale, art);
    }
    text(
        ui,
        place,
        format_args!("{}", Caps(label)),
        rect,
        14.0,
        if hovered || focused {
            view::focus_pulse()
        } else {
            GOLD
        },
        FontWeight::Semibold,
        TextAlign::Center,
    );
    ui.hit_region(token, area);
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
        for part in [TITLE, PROMPT, FIELD, INFO, MESSAGE, SEND, CANCEL] {
            assert!(inside(part, BOX), "{part:?}");
        }
        assert!(inside(BOX, [0.0, 0.0, 640.0, 480.0]));
        assert!(inside(LAUNCH, [0.0, 0.0, 640.0, 480.0]));
    }

    #[test]
    fn parts_follow_each_other_without_overlapping() {
        let order = [TITLE, PROMPT, FIELD, INFO, MESSAGE, SEND];
        for pair in order.windows(2) {
            assert!(pair[0][1] + pair[0][3] <= pair[1][1], "{pair:?}");
        }
    }
}
