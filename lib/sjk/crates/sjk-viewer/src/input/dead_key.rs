//! Dead-key composition for the console line and the chat field.
//!
//! On a layout with dead keys (`^` on French AZERTY and German QWERTZ, `'` on US
//! International), winit reports the dead key press as [`Key::Dead`] with no text,
//! and the next press carries the result: the composed character (`ê`), or on
//! Windows the dead character followed by the key's own when they do not combine
//! (`^1` in one event). Text fields that only read `KeyEvent::text` therefore show
//! nothing after the dead key, which hides the `^` of a colour code until the digit
//! arrives. [`DeadKey`] shows the dead character at the caret straight away, as a
//! layout without dead keys shows a typed `^`, and swaps it for the platform's text
//! once that arrives, so typing `^1` gives exactly `^1` on either kind of layout.
//!
//! Compose tables (X11 and Wayland through xkb) turn a dead `^` and a digit into a
//! superscript digit (`¹`). In Quake 3 text a caret before a digit is a colour code,
//! so that composition is turned back into `^` and the digit.
//!
//! IME composition (`WindowEvent::Ime`) is not involved: the window never enables
//! IME, so winit delivers dead keys as key events on every platform.

use std::ops::Range;
use winit::keyboard::Key;

/// A single-line text field that dead-key composition can type into.
pub(crate) trait TypingField {
    /// The current line.
    fn line(&self) -> &str;
    /// Caret byte offset in [`Self::line`].
    fn caret(&self) -> usize;
    /// Type `text` at the caret under the field's own rules (control characters,
    /// length limit, selection), leaving the caret after what was inserted.
    fn insert(&mut self, text: &str);
    /// Remove `range` of the line, keeping the caret on the same text.
    fn remove(&mut self, range: Range<usize>);
}

/// The dead key of a field shown at the caret while its composition is pending.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DeadKey {
    /// Byte offset and character of the shown dead key.
    shown: Option<(usize, char)>,
}

impl DeadKey {
    /// Type the character side of a pressed key: show a dead key, or insert the
    /// key's text in place of the shown dead key. Keys without text (modifiers,
    /// caret motion) leave a pending composition alone, as the platform does.
    pub(crate) fn type_key(
        &mut self,
        field: &mut impl TypingField,
        logical: &Key,
        text: Option<&str>,
    ) {
        match (text, logical) {
            (Some(text), _) => {
                let dead = self.withdraw(field);
                let mut code = [0; 8];
                field.insert(colour_code(dead, text, &mut code).unwrap_or(text));
            }
            (None, Key::Dead(Some(dead))) => {
                self.withdraw(field);
                let before = (field.line().len(), field.caret());
                field.insert(dead.encode_utf8(&mut [0; 4]));
                let end = field.caret();
                let start = end.saturating_sub(dead.len_utf8());
                // The field may refuse it (input limit); only track what it shows.
                let changed = before != (field.line().len(), end);
                if changed
                    && field.line().get(start..end).and_then(|s| s.chars().next()) == Some(*dead)
                {
                    self.shown = Some((start, *dead));
                }
            }
            _ => {}
        }
    }

    /// A key the field handles itself (Enter, Backspace, Tab, Escape). When the
    /// platform reported text for it, that key ended the composition, so the shown
    /// dead key stays typed: Backspace then erases it and Enter sends it, as both
    /// would with a `^` typed on a layout without dead keys.
    pub(crate) fn other_key(&mut self, text: Option<&str>) {
        if text.is_some() {
            self.settle();
        }
    }

    /// Keep the shown dead key as typed text, for example when the line is replaced.
    pub(crate) fn settle(&mut self) {
        self.shown = None;
    }

    /// Remove the shown dead key from the line, returning its character.
    fn withdraw(&mut self, field: &mut impl TypingField) -> Option<char> {
        let (start, dead) = self.shown.take()?;
        let end = start + dead.len_utf8();
        if field.line().get(start..end).and_then(|s| s.chars().next()) == Some(dead) {
            field.remove(start..end);
        }
        Some(dead)
    }
}

/// Where a caret at `caret` lands once `range` is removed from its line: fields use
/// it to implement [`TypingField::remove`].
pub(crate) fn keep_caret(caret: usize, range: &Range<usize>) -> usize {
    if caret >= range.end {
        caret - range.len()
    } else {
        caret.min(range.start)
    }
}

/// `^` and the digit for a superscript digit composed from a dead `^`.
fn colour_code<'a>(dead: Option<char>, text: &str, buffer: &'a mut [u8; 8]) -> Option<&'a str> {
    let mut characters = text.chars();
    let (Some('^'), Some(superscript), None) = (dead, characters.next(), characters.next()) else {
        return None;
    };
    let digit = match superscript {
        '\u{2070}' => 0,
        '\u{b9}' => 1,
        '\u{b2}' => 2,
        '\u{b3}' => 3,
        '\u{2074}'..='\u{2079}' => superscript as u32 - 0x2070,
        _ => return None,
    };
    buffer[0] = b'^';
    buffer[1] = b'0' + digit as u8;
    std::str::from_utf8(&buffer[..2]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::{NamedKey, SmolStr};

    /// A line with a caret and a byte limit, like the chat field.
    #[derive(Default)]
    struct Line {
        text: String,
        caret: usize,
        limit: usize,
    }

    impl TypingField for Line {
        fn line(&self) -> &str {
            &self.text
        }
        fn caret(&self) -> usize {
            self.caret
        }
        fn insert(&mut self, text: &str) {
            for character in text.chars().filter(|c| !c.is_control()) {
                if self.text.len() + character.len_utf8() > self.limit {
                    break;
                }
                self.text.insert(self.caret, character);
                self.caret += character.len_utf8();
            }
        }
        fn remove(&mut self, range: Range<usize>) {
            if self.caret >= range.end {
                self.caret -= range.len();
            } else if self.caret > range.start {
                self.caret = range.start;
            }
            self.text.replace_range(range, "");
        }
    }

    /// A pressed key event, as winit reports it on Windows.
    enum Press {
        /// A dead key: `Key::Dead`, no text.
        Dead(char),
        /// A modifier such as Shift: no text.
        Modifier,
        /// A character key with the text the platform composed for it.
        Text(&'static str),
    }

    fn line() -> Line {
        Line {
            limit: 64,
            ..Line::default()
        }
    }

    fn type_all(line: &mut Line, dead: &mut DeadKey, presses: &[Press]) {
        for press in presses {
            match press {
                Press::Dead(c) => dead.type_key(line, &Key::Dead(Some(*c)), None),
                Press::Modifier => dead.type_key(line, &Key::Named(NamedKey::Shift), None),
                Press::Text(text) => {
                    dead.type_key(line, &Key::Character(SmolStr::new(text)), Some(text));
                }
            }
        }
    }

    #[test]
    fn removing_a_range_keeps_the_caret_on_the_same_text() {
        assert_eq!(keep_caret(5, &(1..3)), 3);
        assert_eq!(keep_caret(2, &(1..3)), 1);
        assert_eq!(keep_caret(0, &(1..3)), 0);
        assert_eq!(keep_caret(3, &(1..3)), 1);
    }

    #[test]
    fn dead_caret_shows_at_once_like_a_typed_caret() {
        let (mut line, mut dead) = (line(), DeadKey::default());
        type_all(&mut line, &mut dead, &[Press::Text("a"), Press::Dead('^')]);
        assert_eq!((line.text.as_str(), line.caret), ("a^", 2));
    }

    #[test]
    fn azerty_colour_code_types_exactly_caret_digit() {
        // French AZERTY: dead `^`, then Shift and the `&`/`1` key; Windows delivers
        // the uncombined pair as the second key's text.
        let (mut line, mut dead) = (line(), DeadKey::default());
        type_all(
            &mut line,
            &mut dead,
            &[
                Press::Dead('^'),
                Press::Modifier,
                Press::Text("^1"),
                Press::Text("h"),
                Press::Text("i"),
            ],
        );
        assert_eq!(line.text, "^1hi");
        assert_eq!(line.caret, 4);
    }

    #[test]
    fn us_colour_code_is_unchanged() {
        let (mut line, mut dead) = (line(), DeadKey::default());
        type_all(
            &mut line,
            &mut dead,
            &[Press::Modifier, Press::Text("^"), Press::Text("1")],
        );
        assert_eq!(line.text, "^1");
    }

    #[test]
    fn composed_character_replaces_the_dead_key() {
        let (mut line, mut dead) = (line(), DeadKey::default());
        type_all(
            &mut line,
            &mut dead,
            &[
                Press::Text("f"),
                Press::Dead('^'),
                Press::Text("ê"),
                Press::Text("te"),
            ],
        );
        assert_eq!(line.text, "fête");
    }

    #[test]
    fn dead_key_then_space_gives_the_caret_alone() {
        let (mut line, mut dead) = (line(), DeadKey::default());
        type_all(&mut line, &mut dead, &[Press::Dead('^'), Press::Text("^")]);
        assert_eq!(line.text, "^");
    }

    #[test]
    fn dead_key_pressed_twice_types_both() {
        // Windows reports the second dead press with both characters as text.
        let (mut line, mut dead) = (line(), DeadKey::default());
        type_all(&mut line, &mut dead, &[Press::Dead('^'), Press::Text("^^")]);
        assert_eq!(line.text, "^^");
    }

    #[test]
    fn xkb_superscript_composition_becomes_a_colour_code() {
        let (mut line, mut dead) = (line(), DeadKey::default());
        for (superscript, code) in [
            ("¹", "^1"),
            ("²", "^2"),
            ("³", "^3"),
            ("⁰", "^0"),
            ("⁷", "^7"),
        ] {
            line.text.clear();
            line.caret = 0;
            type_all(
                &mut line,
                &mut dead,
                &[Press::Dead('^'), Press::Text(superscript)],
            );
            assert_eq!(line.text, code);
        }
    }

    #[test]
    fn superscript_typed_without_a_dead_caret_is_kept() {
        let (mut line, mut dead) = (line(), DeadKey::default());
        type_all(&mut line, &mut dead, &[Press::Text("²")]);
        assert_eq!(line.text, "²");
        type_all(&mut line, &mut dead, &[Press::Dead('¨'), Press::Text("¹")]);
        assert_eq!(line.text, "²¹");
    }

    #[test]
    fn dead_key_is_replaced_where_it_was_shown_after_caret_motion() {
        // Arrow keys carry no text and keep the platform's composition pending.
        let (mut line, mut dead) = (line(), DeadKey::default());
        type_all(&mut line, &mut dead, &[Press::Text("ab"), Press::Dead('^')]);
        line.caret = 1;
        dead.type_key(&mut line, &Key::Named(NamedKey::ArrowLeft), None);
        type_all(&mut line, &mut dead, &[Press::Text("^1")]);
        assert_eq!((line.text.as_str(), line.caret), ("a^1b", 3));
    }

    #[test]
    fn a_key_with_text_handled_by_the_field_keeps_the_shown_key() {
        // Enter or Backspace after a dead key: the composition ends, the `^` stays.
        let (mut line, mut dead) = (line(), DeadKey::default());
        type_all(&mut line, &mut dead, &[Press::Dead('^')]);
        dead.other_key(Some("^\r"));
        type_all(&mut line, &mut dead, &[Press::Text("1")]);
        assert_eq!(line.text, "^1");
    }

    #[test]
    fn a_key_without_text_handled_by_the_field_keeps_the_composition() {
        let (mut line, mut dead) = (line(), DeadKey::default());
        type_all(&mut line, &mut dead, &[Press::Dead('^')]);
        dead.other_key(None);
        type_all(&mut line, &mut dead, &[Press::Text("^1")]);
        assert_eq!(line.text, "^1");
    }

    #[test]
    fn a_replaced_line_is_not_edited_by_a_stale_dead_key() {
        let (mut line, mut dead) = (line(), DeadKey::default());
        type_all(&mut line, &mut dead, &[Press::Text("x"), Press::Dead('^')]);
        line.text = "q".to_owned();
        line.caret = 1;
        type_all(&mut line, &mut dead, &[Press::Text("^1")]);
        assert_eq!(line.text, "q^1");
    }

    #[test]
    fn a_full_line_does_not_track_a_refused_dead_key() {
        // The line already ends in a typed `^`, which must not be taken for the dead key.
        let mut line = Line {
            limit: 2,
            ..Line::default()
        };
        let mut dead = DeadKey::default();
        type_all(
            &mut line,
            &mut dead,
            &[Press::Text("a"), Press::Text("^"), Press::Dead('^')],
        );
        assert_eq!(dead, DeadKey::default());
        type_all(&mut line, &mut dead, &[Press::Text("^1")]);
        assert_eq!(line.text, "a^");
    }
}
