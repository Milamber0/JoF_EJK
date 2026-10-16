//! Caret and selection over the console's single input line.
//!
//! The text itself stays in [`super::ViewerConsole`]'s `input` string, which other
//! code replaces wholesale (history, completion, autoclear); this type only keeps a
//! byte caret and an optional selection anchor beside it. Every operation clamps
//! both to the text it is given first, so a replaced line can never leave them
//! pointing past its end or into the middle of a character.

use std::ops::Range;

/// A caret movement, also used as the extent of a deletion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Motion {
    /// One character left.
    Left,
    /// One character right.
    Right,
    /// To the start of the word before the caret.
    WordLeft,
    /// To the start of the next word.
    WordRight,
    /// To the start of the line.
    Home,
    /// To the end of the line.
    End,
}

/// Caret byte offset and selection anchor of the console input line.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct LineEdit {
    cursor: usize,
    /// The other end of the selection; the selection is empty when it equals the caret.
    anchor: Option<usize>,
}

impl LineEdit {
    /// Caret byte offset in `text`.
    pub(crate) fn cursor(&self, text: &str) -> usize {
        clamp(text, self.cursor)
    }

    /// Selected byte range in `text`, if it is not empty.
    pub(crate) fn selection(&self, text: &str) -> Option<Range<usize>> {
        let cursor = self.cursor(text);
        let anchor = clamp(text, self.anchor?);
        (anchor != cursor).then(|| anchor.min(cursor)..anchor.max(cursor))
    }

    /// The line was replaced: the caret goes to its end and nothing is selected.
    pub(crate) fn to_end(&mut self, text: &str) {
        self.cursor = text.len();
        self.anchor = None;
    }

    /// Drop the selection, keeping the caret.
    pub(crate) fn deselect(&mut self) {
        self.anchor = None;
    }

    /// Select the whole line, caret at its end.
    pub(crate) fn select_all(&mut self, text: &str) {
        self.anchor = Some(0);
        self.cursor = text.len();
    }

    /// Select `range` (from a double click), caret at its end.
    pub(crate) fn select(&mut self, text: &str, range: Range<usize>) {
        self.anchor = Some(clamp(text, range.start));
        self.cursor = clamp(text, range.end);
    }

    /// Put the caret at byte `target` (from the pointer); `extend` keeps or starts a
    /// selection from the current caret instead of dropping it.
    pub(crate) fn place(&mut self, text: &str, target: usize, extend: bool) {
        let cursor = self.cursor(text);
        if extend {
            self.anchor.get_or_insert(cursor);
        } else {
            self.anchor = None;
        }
        self.cursor = clamp(text, target);
    }

    /// Move the caret. With `extend` (Shift held) the selection grows or starts;
    /// without it, a left or right step over a selection lands on that side of it.
    pub(crate) fn motion(&mut self, text: &str, motion: Motion, extend: bool) {
        let cursor = self.cursor(text);
        if !extend && let Some(range) = self.selection(text) {
            self.anchor = None;
            match motion {
                Motion::Left => {
                    self.cursor = range.start;
                    return;
                }
                Motion::Right => {
                    self.cursor = range.end;
                    return;
                }
                _ => {}
            }
        }
        if extend {
            self.anchor.get_or_insert(cursor);
        } else {
            self.anchor = None;
        }
        self.cursor = target(text, cursor, motion);
    }

    /// Delete the selection, or else the text between the caret and `motion`'s
    /// target (Backspace is `Left`, Delete is `Right`, Ctrl adds the word forms).
    pub(crate) fn delete(&mut self, text: &mut String, motion: Motion) {
        let range = self.selection(text).unwrap_or_else(|| {
            let cursor = self.cursor(text);
            let other = target(text, cursor, motion);
            cursor.min(other)..cursor.max(other)
        });
        text.replace_range(range.clone(), "");
        self.cursor = range.start;
        self.anchor = None;
    }

    /// Replace the selection with `value`, or insert it at the caret, dropping control
    /// characters and whatever would take the line past `limit` bytes.
    pub(crate) fn insert(&mut self, text: &mut String, value: &str, limit: usize) {
        self.insert_counted(text, value, limit, char::len_utf8);
    }

    /// [`Self::insert`], keeping only what leaves the whole line `fits`: for a
    /// line whose length depends on all of it, such as chat, whose encoding the
    /// whole line picks.
    pub(crate) fn insert_fitting(
        &mut self,
        text: &mut String,
        value: &str,
        fits: impl Fn(&str) -> bool,
    ) {
        if let Some(range) = self.selection(text) {
            text.replace_range(range.clone(), "");
            self.cursor = range.start;
        }
        self.anchor = None;
        let mut cursor = self.cursor(text);
        for character in value.chars().filter(|character| !character.is_control()) {
            text.insert(cursor, character);
            if !fits(text) {
                text.remove(cursor);
                break;
            }
            cursor += character.len_utf8();
        }
        self.cursor = cursor;
    }

    /// [`Self::insert`], with the line's length counted by `cost` per
    /// character instead of its UTF-8 bytes.
    pub(crate) fn insert_counted(
        &mut self,
        text: &mut String,
        value: &str,
        limit: usize,
        cost: impl Fn(char) -> usize,
    ) {
        if let Some(range) = self.selection(text) {
            text.replace_range(range.clone(), "");
            self.cursor = range.start;
        }
        self.anchor = None;
        let mut cursor = self.cursor(text);
        let mut length: usize = text.chars().map(&cost).sum();
        for character in value.chars().filter(|character| !character.is_control()) {
            if length + cost(character) > limit {
                break;
            }
            length += cost(character);
            text.insert(cursor, character);
            cursor += character.len_utf8();
        }
        self.cursor = cursor;
    }
}

impl LineEdit {
    /// Type `value` over the line (Insert's overstrike mode, `Field_CharEvent`):
    /// each character replaces the one after the caret, and past the end the line
    /// grows. A selection is replaced as [`Self::insert`] does; control characters
    /// are dropped and the line stays within `limit` bytes.
    pub(crate) fn overwrite(&mut self, text: &mut String, value: &str, limit: usize) {
        if self.selection(text).is_some() {
            self.insert(text, value, limit);
            return;
        }
        self.anchor = None;
        let mut cursor = self.cursor(text);
        for character in value.chars().filter(|character| !character.is_control()) {
            let replaced = text[cursor..].chars().next().map_or(0, char::len_utf8);
            if text.len() - replaced + character.len_utf8() > limit {
                break;
            }
            text.replace_range(
                cursor..cursor + replaced,
                character.encode_utf8(&mut [0; 4]),
            );
            cursor += character.len_utf8();
        }
        self.cursor = cursor;
    }
}

/// `index` moved down to a character boundary within `text`.
fn clamp(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// Word characters, as Ctrl+arrows and Ctrl+Backspace see them. Everything else,
/// including `_`, `.` and `/`, separates words, so `cg_drawFPS` and `127.0.0.1` are
/// edited a piece at a time, as EternalJK's Ctrl+Backspace does at `_` and `/`.
fn is_word(character: char) -> bool {
    character.is_alphanumeric()
}

/// Where `motion` takes a caret at `cursor` in `text`.
fn target(text: &str, cursor: usize, motion: Motion) -> usize {
    match motion {
        // By glyph, so the caret steps over an emoji of several characters (`👍🏽`, a
        // flag) at once and Backspace takes it whole.
        Motion::Left => {
            let (mut index, mut last) = (0, 0);
            while index < cursor {
                last = index;
                index += crate::text::glyph_at(text, index).1;
            }
            last
        }
        Motion::Right if cursor < text.len() => cursor + crate::text::glyph_at(text, cursor).1,
        Motion::Right => cursor,
        Motion::Home => 0,
        Motion::End => text.len(),
        Motion::WordLeft => {
            // Back over separators, then over the word before them.
            let mut start = cursor;
            let mut in_word = false;
            for (index, character) in text[..cursor].char_indices().rev() {
                if is_word(character) {
                    in_word = true;
                } else if in_word {
                    break;
                }
                start = index;
            }
            start
        }
        Motion::WordRight => {
            // Over the rest of this word and the separators after it.
            let mut past_word = false;
            for (index, character) in text[cursor..].char_indices() {
                if !is_word(character) {
                    past_word = true;
                } else if past_word {
                    return cursor + index;
                }
            }
            text.len()
        }
    }
}

/// The whitespace-separated token around byte `at` in `text`, for a double click:
/// a whole address such as `127.0.0.1:29070` rather than one number of it.
pub(crate) fn token_at(text: &str, at: usize) -> Range<usize> {
    let at = clamp(text, at);
    let start = text[..at]
        .char_indices()
        .rev()
        .find(|(_, character)| character.is_whitespace())
        .map_or(0, |(index, character)| index + character.len_utf8());
    let end = text[at..]
        .char_indices()
        .find(|(_, character)| character.is_whitespace())
        .map_or(text.len(), |(index, _)| at + index);
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_caret_steps_over_a_whole_emoji() {
        let thumb = "a\u{1F44D}\u{1F3FD}b";
        assert_eq!(target(thumb, 1, Motion::Right), 9);
        assert_eq!(target(thumb, 9, Motion::Left), 1);
        assert_eq!(target(thumb, 10, Motion::Left), 9);
        assert_eq!(target("\u{436}a", 2, Motion::Left), 0);
    }
}
