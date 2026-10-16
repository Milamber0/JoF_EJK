//! The gameplay FPS step of the Import card in the SJK UI (`gameplay_fps.rs`): a
//! pop-up card as the Import page's, taller and higher up. "Gameplay FPS" and the
//! old config's name over a headline; what the import changed; why the frame
//! rate no longer moves you and what the old config's said; the choices as tiles
//! (Auto, 125, 142, 250, 333 and Other, with what each is to the player under it); the
//! typed value; the note on servers that fix the step and the FPS cap in use;
//! then Use (gold) with Cancel, or Skip after an import.

use super::*;
use crate::gameplay_fps::{EXPLANATION, FIXED_NOTE, OTHER};
use crate::menu::sjk::{Frame, TextTarget, color, key_hint, key_hint_width, kit, text, wrap};
use crate::menu_widgets::TextFamily;
use sjk_ui::TextAlign;

/// The card's left edge, top and width; it is as tall as what it holds.
const CARD_X: f32 = 540.0;
const CARD_TOP: f32 = 140.0;
const CARD_WIDTH: f32 = 840.0;
const MARGIN: f32 = 44.0;
const TEXT_X: f32 = CARD_X + MARGIN;
const TEXT_WIDTH: f32 = CARD_WIDTH - MARGIN * 2.0;
const BODY_TOP: f32 = CARD_TOP + 138.0;
const LINE: f32 = 30.0;
/// Most lines the import's summary and the old config's line take each.
const SHORT_LINES: usize = 2;
/// Most lines of the explanation.
const EXPLANATION_LINES: usize = 5;
/// The tiles: their height, the gap between them, and their captions' line.
const TILE_HEIGHT: f32 = 64.0;
const TILE_GAP: f32 = 12.0;
const CAPTION: f32 = 28.0;
/// The notes' lines.
const NOTE: f32 = 27.0;
const NOTES: usize = 3;
const BUTTONS_GAP: f32 = 30.0;
const BUTTON_HEIGHT: f32 = 46.0;
const BUTTON_GAP: f32 = 14.0;
const KEYS_Y: f32 = 992.0;
/// Characters a line holds at the body's size.
const WRAP: usize = 88;

impl Panel {
    /// Draw the gameplay FPS step in the SJK UI.
    pub(super) fn append_sjk_fps(&mut self, target: TextTarget<'_>, viewport: [f32; 2]) {
        let State::Fps {
            choice,
            info,
            imported,
        } = &self.state
        else {
            return;
        };
        let choice = choice.clone();
        let info = info.clone();
        let imported = imported.clone();
        let frame = Frame::new(viewport);
        let s = frame.s;
        let wrapped = |text: &str, most: usize| -> Vec<String> {
            wrap(text, WRAP).take(most).map(str::to_owned).collect()
        };
        // Why the card opened by itself, or what the import changed.
        let summary = match (&info.alert, &imported) {
            (Some(alert), _) => wrapped(alert, SHORT_LINES),
            (None, Some(lines)) => wrapped(&imported_summary(lines), SHORT_LINES),
            (None, None) => Vec::new(),
        };
        let summary_colour = if info.alert.is_some() {
            color::EMBER
        } else {
            color::GOLD
        };
        let explanation = wrapped(EXPLANATION, EXPLANATION_LINES);
        let origin_line = choice
            .origin
            .as_ref()
            .and_then(crate::gameplay_fps::Origin::line)
            .map(|line| wrapped(&line, SHORT_LINES))
            .unwrap_or_default();
        let mut notes = wrapped(FIXED_NOTE, 2);
        match (info.server_fixed, info.cap_name) {
            (Some(step), Some(name)) => notes.extend(wrapped(
                &format!(
                    "{name} servers take {} a second at most; Ignore server caps lifts it.",
                    1000 / step.max(1)
                ),
                1,
            )),
            (Some(step), None) => notes.extend(wrapped(
                &format!(
                    "This server takes a command every {step} ms at most: here you move at {}.",
                    1000 / step.max(1)
                ),
                1,
            )),
            (None, _) => {}
        }
        notes.extend(wrapped(
            &format!(
                "Your FPS cap is {} (Settings > Video): the frames drawn, and Auto's movement.",
                info.cap
            ),
            1,
        ));
        notes.truncate(NOTES);
        let gap = |lines: &[String]| if lines.is_empty() { 0.0 } else { 10.0 };
        let tiles_top = BODY_TOP
            + summary.len() as f32 * LINE
            + gap(&summary)
            + explanation.len() as f32 * LINE
            + 10.0
            + origin_line.len() as f32 * LINE
            + 16.0;
        let typed_top = tiles_top + TILE_HEIGHT + CAPTION;
        let notes_top = typed_top + LINE + 12.0;
        let buttons_y = notes_top + notes.len() as f32 * NOTE + BUTTONS_GAP;
        let height = buttons_y + BUTTON_HEIGHT + MARGIN - CARD_TOP;

        self.ui.begin_transparent(viewport);
        kit::scrim(&mut self.ui, viewport);
        kit::card(&mut self.ui, &frame, [CARD_X, CARD_TOP, CARD_WIDTH, height]);
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("Gameplay FPS"),
            frame.rect(TEXT_X, CARD_TOP + 30.0, TEXT_WIDTH, 30.0),
            22.0 * s,
            color::GOLD_BRIGHT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        if let Some(origin) = &choice.origin {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{}", origin.file),
                frame.rect(TEXT_X + 200.0, CARD_TOP + 33.0, TEXT_WIDTH - 200.0, 26.0),
                16.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::End,
            );
        }
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("Pick the movement you are used to"),
            frame.rect(TEXT_X, CARD_TOP + 70.0, TEXT_WIDTH, 52.0),
            40.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let mut y = BODY_TOP;
        for line in &summary {
            self.fps_line(&frame, line, y, summary_colour);
            y += LINE;
        }
        y += gap(&summary);
        for line in &explanation {
            self.fps_line(&frame, line, y, color::alpha(color::TEXT, 0.86));
            y += LINE;
        }
        y += 10.0;
        for line in &origin_line {
            self.fps_line(&frame, line, y, color::TEXT);
            y += LINE;
        }
        self.fps_tiles(&frame, &choice, tiles_top);
        let typed = if choice.selected == OTHER {
            let draft = if choice.draft.is_empty() {
                "_".to_owned()
            } else {
                format!("{}_", choice.draft)
            };
            format!("Other: {draft}   {}", choice.draft_note())
        } else {
            "Or type any value from 20 to 1000 for Other.".to_owned()
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{typed}"),
            frame.rect(TEXT_X + 8.0, typed_top, TEXT_WIDTH - 16.0, 26.0),
            18.0 * s,
            if choice.selected == OTHER {
                color::TEXT
            } else {
                color::QUIET
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        let mut y = notes_top;
        for note in &notes {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{note}"),
                frame.rect(TEXT_X + 8.0, y, TEXT_WIDTH - 16.0, 24.0),
                16.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
            y += NOTE;
        }
        self.fps_buttons(&frame, &choice, imported.is_some(), buttons_y);
        self.fps_keys(&frame, imported.is_some());
        self.ui.finish(FPS_TOKEN + choice.selected as u16);
        target.append(&self.ui, viewport);
    }

    fn fps_line(&mut self, frame: &Frame, line: &str, y: f32, colour: sjk_ui::Color) {
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{line}"),
            frame.rect(TEXT_X, y, TEXT_WIDTH, 28.0),
            18.0 * frame.s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    /// The choices side by side, the chosen one gold, each with its caption.
    fn fps_tiles(&mut self, frame: &Frame, choice: &crate::gameplay_fps::Choice, top: f32) {
        let count = OTHER + 1;
        let width = (TEXT_WIDTH - TILE_GAP * (count - 1) as f32) / count as f32;
        for index in 0..count {
            let x = TEXT_X + index as f32 * (width + TILE_GAP);
            let label = crate::gameplay_fps::Choice::label(index);
            let chosen = index == choice.selected;
            kit::button(
                &mut self.ui,
                frame,
                [x, top, width, TILE_HEIGHT],
                &label,
                chosen,
                true,
                chosen,
                FPS_TOKEN + index as u16,
            );
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{}", choice.caption(index)),
                frame.rect(x, top + TILE_HEIGHT + 4.0, width, 22.0),
                15.0 * frame.s,
                if chosen { color::GOLD } else { color::MUTED },
                FontWeight::Regular,
                TextAlign::Center,
            );
        }
    }

    /// Use (gold, with the value) and Cancel, or Skip after an import.
    fn fps_buttons(
        &mut self,
        frame: &Frame,
        choice: &crate::gameplay_fps::Choice,
        imported: bool,
        y: f32,
    ) {
        let width = |label: &str| (48.0 + 9.5 * label.chars().count() as f32).max(140.0);
        let value = choice.value();
        let primary = match value {
            Some(0) => "Use Auto".to_owned(),
            Some(fps) => format!("Use {fps}"),
            None => "Use".to_owned(),
        };
        let mut right = CARD_X + CARD_WIDTH - MARGIN;
        let w = width(&primary);
        right -= w;
        kit::button(
            &mut self.ui,
            frame,
            [right, y, w, BUTTON_HEIGHT],
            &primary,
            true,
            value.is_some(),
            true,
            IMPORT_TOKEN,
        );
        right -= BUTTON_GAP;
        let close = if imported { "Skip" } else { "Cancel" };
        let w = width(close);
        kit::button(
            &mut self.ui,
            frame,
            [right - w, y, w, BUTTON_HEIGHT],
            close,
            false,
            true,
            false,
            BACK_TOKEN,
        );
    }

    /// The step's keys, right-aligned at the bottom.
    fn fps_keys(&mut self, frame: &Frame, imported: bool) {
        let s = frame.s;
        let keys: [(&[&str], &str); 4] = [
            (&["Left", "Right"], "choose"),
            (&["0-9"], "type"),
            (&["Enter"], "use"),
            (&["Esc"], if imported { "skip" } else { "cancel" }),
        ];
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

/// One line for what the import changed: the parts' names.
fn imported_summary(lines: &[String]) -> String {
    if lines.is_empty() {
        return "Nothing needed changing in your setup.".to_owned();
    }
    let parts: Vec<&str> = lines
        .iter()
        .map(|line| line.split(':').next().unwrap_or(line).trim())
        .collect();
    format!("Imported: {}.", parts.join(", "))
}

// At its tallest the card ends above the keys.
const _: () = assert!(
    BODY_TOP
        + (SHORT_LINES * 2 + EXPLANATION_LINES) as f32 * LINE
        + 36.0
        + TILE_HEIGHT
        + CAPTION
        + LINE
        + 12.0
        + NOTES as f32 * NOTE
        + BUTTONS_GAP
        + BUTTON_HEIGHT
        + MARGIN
        < KEYS_Y - 20.0
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_names_the_parts() {
        let lines = ["Name: Sol".to_owned(), "Key bindings: 3 keys".to_owned()];
        assert_eq!(imported_summary(&lines), "Imported: Name, Key bindings.");
        assert!(imported_summary(&[]).starts_with("Nothing"));
    }
}
