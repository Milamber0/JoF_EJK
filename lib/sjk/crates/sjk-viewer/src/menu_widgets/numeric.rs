//! Shared bounded numeric drafts for settings, saber colours and Camera control.

use super::MenuCanvas;
use crate::console::line_edit::{LineEdit, Motion};
use sjk_ui::{Color, FontWeight, Rect, TextAlign};
use winit::keyboard::KeyCode;

/// Separate targets keep clicks on a value from moving its slider.
pub(crate) const VALUE_BASE: u16 = 700;

pub(crate) fn value_row(token: u16) -> Option<usize> {
    (VALUE_BASE..VALUE_BASE + 100)
        .contains(&token)
        .then(|| usize::from(token - VALUE_BASE))
}

pub(crate) enum EditResult {
    Pending,
    Cancel,
    Commit(f64),
}

/// Longest draft SJK accepts, in characters.
const MAX_CHARS: usize = 10;

/// Owns the target row so pointer hover cannot redirect an unfinished edit.
pub(crate) struct NumericEdit {
    pub(crate) row: usize,
    text: String,
    caret: LineEdit,
    min: f64,
    max: f64,
    integer: bool,
    invalid: bool,
}

impl NumericEdit {
    pub(crate) fn new(row: usize, text: String, min: f64, max: f64, integer: bool) -> Self {
        let mut caret = LineEdit::default();
        caret.select_all(&text);
        Self {
            row,
            text,
            caret,
            min,
            max,
            integer,
            invalid: false,
        }
    }

    /// SJK: entry opened by typing on a selected slider, starting with `typed`
    /// instead of the current value. `None` when `typed` cannot start a number
    /// for this slider.
    pub(crate) fn typed(
        row: usize,
        typed: &str,
        min: f64,
        max: f64,
        integer: bool,
    ) -> Option<Self> {
        let mut edit = Self::new(row, String::new(), min, max, integer);
        edit.type_text(typed).then_some(edit)
    }

    /// The draft's value when it is valid, as Enter would apply it.
    pub(crate) fn committed(&self) -> Option<f64> {
        self.value()
    }

    /// SJK's stricter filter: digits, one decimal point (a comma counts as one),
    /// a minus sign only first and only when the slider goes below zero, and at
    /// most [`MAX_CHARS`] characters. True when anything was typed.
    fn type_text(&mut self, typed: &str) -> bool {
        let mut accepted = false;
        for character in typed.chars() {
            let character = if character == ',' { '.' } else { character };
            let (rest, at) = match self.caret.selection(&self.text) {
                Some(range) => {
                    let mut rest = self.text.clone();
                    rest.replace_range(range.clone(), "");
                    (rest, range.start)
                }
                None => (self.text.clone(), self.caret.cursor(&self.text)),
            };
            let fits = rest.chars().count() < MAX_CHARS && !(at == 0 && rest.starts_with('-'));
            let allowed = match character {
                '0'..='9' => true,
                '.' => !rest.contains('.'),
                '-' => self.min < 0.0 && at == 0,
                _ => false,
            };
            if fits && allowed {
                let mut buffer = [0; 4];
                self.caret.insert(
                    &mut self.text,
                    character.encode_utf8(&mut buffer),
                    MAX_CHARS,
                );
                accepted = true;
            }
        }
        accepted
    }

    /// Enter validates without step quantization; Escape discards the draft.
    /// A held key repeats editing keys but never types characters.
    pub(crate) fn key(&mut self, key: KeyCode, text: Option<&str>, repeat: bool) -> EditResult {
        match key {
            KeyCode::Escape => return EditResult::Cancel,
            KeyCode::Enter | KeyCode::NumpadEnter => {
                if let Some(value) = self.value() {
                    return EditResult::Commit(value);
                }
                self.invalid = true;
                return EditResult::Pending;
            }
            KeyCode::ArrowLeft => self.caret.motion(&self.text, Motion::Left, false),
            KeyCode::ArrowRight => self.caret.motion(&self.text, Motion::Right, false),
            KeyCode::Home => self.caret.motion(&self.text, Motion::Home, false),
            KeyCode::End => self.caret.motion(&self.text, Motion::End, false),
            KeyCode::Backspace => self.caret.delete(&mut self.text, Motion::Left),
            KeyCode::Delete => self.caret.delete(&mut self.text, Motion::Right),
            _ => {
                if let Some(text) = text {
                    if text == "\u{1}" {
                        self.caret.select_all(&self.text);
                    } else if !repeat {
                        self.type_text(text);
                    }
                }
            }
        }
        self.invalid = false;
        EditResult::Pending
    }

    /// The typed number clamped to the slider; integer sliders round a typed
    /// fraction (142.6 is 143) rather than refusing it.
    fn value(&self) -> Option<f64> {
        let value = self.text.trim().parse::<f64>().ok()?;
        let value = if self.integer { value.round() } else { value };
        value.is_finite().then(|| value.clamp(self.min, self.max))
    }

    /// Compact inline editing, using retained text storage instead of frame allocations.
    pub(crate) fn draw(&self, ui: &mut MenuCanvas, rect: Rect, scale: f32) {
        let color = if self.invalid {
            Color::new(1.0, 0.3, 0.25, 1.0)
        } else {
            ui.theme().accent
        };
        if self.caret.selection(&self.text).is_some() {
            ui.accent_bar(rect, Color::new(color.r, color.g, color.b, 0.2));
        }
        let cursor = self.caret.cursor(&self.text);
        let marker = if self.caret.selection(&self.text).is_some() {
            ""
        } else {
            "|"
        };
        let size = (15.0 * scale).min(rect.width / (self.text.len() + 1) as f32);
        ui.text_fmt_aligned(
            format_args!("{}{marker}{}", &self.text[..cursor], &self.text[cursor..]),
            rect,
            size,
            color,
            FontWeight::Semibold,
            0.0,
            TextAlign::End,
        );
        ui.edit_underline(rect, color, scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(edit: &mut NumericEdit) -> Option<f64> {
        match edit.key(KeyCode::Enter, None, false) {
            EditResult::Commit(value) => Some(value),
            _ => None,
        }
    }

    #[test]
    fn typing_opens_a_draft_with_that_character() {
        let mut edit = NumericEdit::typed(0, "1", 0.0, 2000.0, true).unwrap();
        edit.key(KeyCode::Digit4, Some("4"), false);
        edit.key(KeyCode::Digit2, Some("2"), false);
        assert_eq!(commit(&mut edit), Some(142.0));
        assert!(NumericEdit::typed(0, "x", 0.0, 2000.0, true).is_none());
        assert!(NumericEdit::typed(0, "-", 0.0, 2000.0, true).is_none());
        assert!(NumericEdit::typed(0, "-", -1.0, 2000.0, true).is_some());
    }

    #[test]
    fn integer_sliders_round_a_typed_fraction() {
        let mut edit = NumericEdit::typed(0, "142.6", 0.0, 2000.0, true).unwrap();
        assert_eq!(commit(&mut edit), Some(143.0));
    }

    #[test]
    fn auto_rows_take_minus_one() {
        let mut edit = NumericEdit::new(0, "60".to_owned(), -1.0, 2000.0, true);
        edit.key(KeyCode::Minus, Some("-"), false);
        edit.key(KeyCode::Digit1, Some("1"), false);
        assert_eq!(commit(&mut edit), Some(-1.0));
    }

    #[test]
    fn the_filter_takes_one_point_one_leading_minus_and_ten_characters() {
        let mut edit = NumericEdit::typed(0, "1", -10.0, 1e12, false).unwrap();
        edit.key(KeyCode::Period, Some("."), false);
        edit.key(KeyCode::Comma, Some(","), false);
        edit.key(KeyCode::Minus, Some("-"), false);
        edit.key(KeyCode::KeyE, Some("e"), false);
        assert_eq!(edit.text, "1.");
        for _ in 0..20 {
            edit.key(KeyCode::Digit5, Some("5"), false);
        }
        assert_eq!(edit.text.len(), MAX_CHARS);
        let mut comma = NumericEdit::typed(0, "0", 0.0, 1.0, false).unwrap();
        comma.key(KeyCode::Comma, Some(","), false);
        comma.key(KeyCode::Digit5, Some("5"), false);
        assert_eq!(commit(&mut comma), Some(0.5));
    }

    #[test]
    fn held_keys_do_not_type() {
        let mut edit = NumericEdit::typed(0, "1", 0.0, 2000.0, true).unwrap();
        edit.key(KeyCode::Digit1, Some("1"), true);
        assert_eq!(edit.text, "1");
    }

    #[test]
    fn committed_reports_only_valid_drafts() {
        let edit = NumericEdit::typed(0, "7", 0.0, 2000.0, true).unwrap();
        assert_eq!(edit.committed(), Some(7.0));
        let point = NumericEdit::typed(0, ".", 0.0, 2000.0, false).unwrap();
        assert_eq!(point.committed(), None);
    }
}
