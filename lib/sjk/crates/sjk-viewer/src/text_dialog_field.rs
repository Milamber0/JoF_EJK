//! The text dialog's box: how its text wraps and where every caret position sits.
//!
//! Both looks draw the text as retained text runs, one per wrapped line, which the renderer
//! sizes and spaces with the player's menu text style (`ui_textScale`, `ui_letterSpacing`,
//! [`TextStyle`]). The wrap, the caret and the pointer therefore measure with the same
//! [`Face`] the renderer draws with: the caret then sits exactly between two glyphs and a
//! click lands on the glyph under it, whatever the style.
//!
//! [`FieldLayout`] keeps the wrapped lines as byte ranges of the text (so a caret index maps
//! to a line and back), the x offset before every character, and the first line shown. The
//! dialog rebuilds it each frame it draws; the keys that move by lines (Up, Down, Home, End)
//! and the pointer read what the last frame left.

use crate::text::{TextFace, TextStyle, UiFont, glyph_at};
use std::ops::Range;

/// How the field's text is drawn: the font, the player's text style, the size and the extra
/// spacing the layout asks for, as the renderer resolves them (`TextStyle::place`).
pub(super) struct Face<'a> {
    font: &'a UiFont,
    /// Pixels of a glyph's advance per font unit.
    scale: f32,
    /// Pen advance added after every glyph.
    pen: f32,
    /// The styled text size, in pixels.
    size: f32,
}

impl<'a> Face<'a> {
    /// A run laid out `size` pixels high with `spacing` pixels between glyphs, in `style`.
    pub(super) fn new(font: &'a UiFont, style: TextStyle, size: f32, spacing: f32) -> Self {
        let styled = size * style.scale;
        Self {
            font,
            scale: styled / font.height.max(1.0),
            pen: spacing + style.tracking * styled,
            size: styled,
        }
    }

    /// How far drawing `character` moves the pen.
    pub(super) fn advance(&self, character: char) -> f32 {
        let mut buffer = [0; 4];
        let (key, _) = glyph_at(character.encode_utf8(&mut buffer), 0);
        self.font.glyph_of(TextFace::Regular, key).advance * self.scale + self.pen
    }

    /// The width of `text` drawn on one line.
    pub(super) fn width(&self, text: &str) -> f32 {
        // Glyph by glyph, so an emoji of several characters counts once.
        let mut width = 0.0;
        let mut index = 0;
        while index < text.len() {
            let (key, step) = glyph_at(text, index);
            width += self.font.glyph_of(TextFace::Regular, key).advance * self.scale + self.pen;
            index += step;
        }
        width
    }

    /// The styled text size, in pixels.
    pub(super) fn size(&self) -> f32 {
        self.size
    }
}

/// `text` broken into lines `fits` accepts, as byte ranges of `text`, at spaces where it can
/// be. A line break at a space swallows that one space; a word longer than a line is cut where
/// the line ends. Every byte of `text` but those spaces is in a range, in order, and the last
/// range ends at the end of `text`, so a caret at any index belongs to a line.
pub(super) fn wrap_ranges(text: &str, fits: impl Fn(&str) -> bool, lines: &mut Vec<Range<usize>>) {
    lines.clear();
    let mut start = 0;
    loop {
        // The widest run of whole words from `start` that fits (an empty one when the
        // text starts with a space).
        let mut best = None;
        let ends = text[start..]
            .match_indices(' ')
            .map(|(at, _)| start + at)
            .chain(std::iter::once(text.len()));
        for end in ends {
            if !fits(&text[start..end]) {
                break;
            }
            best = Some(end);
        }
        let Some(end) = best else {
            // Not even the first word fits: cut it where the line ends, one character at least.
            let mut end = start;
            for (at, character) in text[start..].char_indices() {
                let next = start + at + character.len_utf8();
                if end > start && !fits(&text[start..next]) {
                    break;
                }
                end = next;
            }
            lines.push(start..end);
            if end == text.len() {
                return;
            }
            start = end;
            continue;
        };
        lines.push(start..end);
        if end == text.len() {
            return;
        }
        // `end` is a space.
        start = end + 1;
    }
}

/// The wrapped lines of the dialog's text and where each caret position sits.
#[derive(Debug, Default)]
pub(super) struct FieldLayout {
    /// The byte range of each line.
    lines: Vec<Range<usize>>,
    /// The x offset before each character of each line (and after the last), flattened;
    /// line `i` owns `stops[starts[i]..]` for `chars + 1` entries.
    stops: Vec<f32>,
    starts: Vec<usize>,
    /// The first line shown, and how many lines the box shows.
    first: usize,
    rows: usize,
    /// The top left of the first row and the distance between rows, in pixels.
    origin: [f32; 2],
    pitch: f32,
    /// The text changed since the layout was built.
    stale: bool,
}

impl FieldLayout {
    /// The text changed: line-based moves wait for the next frame's layout.
    pub(super) fn touch(&mut self) {
        self.stale = true;
    }

    /// Back to the first line, for a new text.
    pub(super) fn reset(&mut self) {
        self.first = 0;
        self.stale = true;
    }

    /// Lay `text` out for a box `width` pixels wide drawn in `face`, leaving room for the
    /// caret at the end of a line.
    pub(super) fn lay_out(&mut self, text: &str, face: &Face<'_>, width: f32) {
        let room = width - face.size() * 0.5;
        wrap_ranges(text, |line| face.width(line) <= room, &mut self.lines);
        self.stops.clear();
        self.starts.clear();
        for line in &self.lines {
            self.starts.push(self.stops.len());
            let mut x = 0.0;
            self.stops.push(x);
            for character in text[line.clone()].chars() {
                x += face.advance(character);
                self.stops.push(x);
            }
        }
        self.stale = false;
    }

    /// Show `rows` lines from `origin` (the top left of the first), `pitch` pixels apart, and
    /// scroll the least that keeps `line` among them.
    pub(super) fn show(&mut self, line: usize, rows: usize, origin: [f32; 2], pitch: f32) {
        self.rows = rows;
        self.origin = origin;
        self.pitch = pitch;
        if line < self.first {
            self.first = line;
        } else if line >= self.first + rows {
            self.first = line + 1 - rows;
        }
        self.first = self.first.min(self.lines.len().saturating_sub(rows));
    }

    /// The first line shown, and the lines shown.
    pub(super) fn visible(&self) -> (usize, &[Range<usize>]) {
        let end = (self.first + self.rows).min(self.lines.len());
        (self.first, &self.lines[self.first.min(end)..end])
    }

    /// The line a caret at byte `at` is on, and its x offset in the line. A caret between
    /// two lines cut inside a word is on the second.
    pub(super) fn locate(&self, text: &str, at: usize) -> (usize, f32) {
        let mut line = self.lines.len().saturating_sub(1);
        for (index, range) in self.lines.iter().enumerate() {
            let continues = self
                .lines
                .get(index + 1)
                .is_some_and(|next| next.start == at);
            if at < range.end || (at == range.end && !continues) {
                line = index;
                break;
            }
        }
        let Some(range) = self.lines.get(line) else {
            return (0, 0.0);
        };
        let at = at.clamp(range.start, range.end);
        let characters = text
            .get(range.start..at)
            .map_or(0, |part| part.chars().count());
        let stops = &self.stops[self.starts[line]..];
        (line, stops[characters.min(stops.len() - 1)])
    }

    /// The byte of the caret position on `line` nearest `x`.
    fn nearest(&self, text: &str, line: usize, x: f32) -> usize {
        let range = self.lines[line].clone();
        let count = text[range.clone()].chars().count();
        let stops = &self.stops[self.starts[line]..=self.starts[line] + count];
        let mut best = 0;
        for (index, stop) in stops.iter().enumerate() {
            if (stop - x).abs() < (stops[best] - x).abs() {
                best = index;
            }
        }
        text[range.clone()]
            .char_indices()
            .nth(best)
            .map_or(range.end, |(offset, _)| range.start + offset)
    }

    /// Whether the layout is of `text` as it stands.
    fn fresh(&self, text: &str) -> bool {
        !self.stale && self.lines.last().is_some_and(|last| last.end == text.len())
    }

    /// The caret position a click or tap at `point` (window pixels) lands on, if the box has
    /// been drawn.
    pub(super) fn at_point(&self, text: &str, point: [f32; 2]) -> Option<usize> {
        let (first, shown) = self.visible();
        if !self.fresh(text) || shown.is_empty() || self.pitch <= 0.0 {
            return None;
        }
        let row = ((point[1] - self.origin[1]) / self.pitch).floor().max(0.0) as usize;
        let line = first + row.min(shown.len() - 1);
        Some(self.nearest(text, line, point[0] - self.origin[0]))
    }

    /// Where Up or Down takes a caret at byte `at`: the same x one line away; the start of
    /// the text from the first line, its end from the last.
    pub(super) fn vertical(&self, text: &str, at: usize, down: bool) -> usize {
        if !self.fresh(text) {
            return at;
        }
        let (line, x) = self.locate(text, at);
        match (down, line) {
            (false, 0) => 0,
            (true, last) if last + 1 >= self.lines.len() => text.len(),
            (false, _) => self.nearest(text, line - 1, x),
            (true, _) => self.nearest(text, line + 1, x),
        }
    }

    /// The start (`home`) or end of the line a caret at byte `at` is on.
    pub(super) fn edge(&self, text: &str, at: usize, home: bool) -> usize {
        if !self.fresh(text) {
            return if home { 0 } else { text.len() };
        }
        let range = &self.lines[self.locate(text, at).0];
        if home { range.start } else { range.end }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrap(text: &str, width: usize) -> Vec<&str> {
        let mut lines = Vec::new();
        wrap_ranges(text, |line| line.chars().count() <= width, &mut lines);
        lines.into_iter().map(|range| &text[range]).collect()
    }

    #[test]
    fn lines_break_at_spaces_and_cut_long_words() {
        assert_eq!(wrap("one two three four", 9), ["one two", "three", "four"]);
        assert_eq!(wrap("abcdefghijklmnop", 8), ["abcdefgh", "ijklmnop"]);
        assert_eq!(wrap("", 10), [""]);
        // A space that would start a line is kept, one that ends a line is swallowed.
        assert_eq!(wrap(" a", 5), [" a"]);
        assert_eq!(wrap("abcd efgh", 4), ["abcd", "efgh"]);
        assert_eq!(wrap("abcd ", 4), ["abcd", ""]);
        assert_eq!(wrap("ab  cd", 4), ["ab ", "cd"]);
    }

    #[test]
    fn every_byte_but_a_swallowed_space_is_in_a_line() {
        let text = "wonderful  door   flickers on the way to the tower, again and again";
        for width in [3, 5, 8, 13, 40] {
            let mut lines = Vec::new();
            wrap_ranges(text, |line| line.chars().count() <= width, &mut lines);
            assert_eq!(lines.last().map(|line| line.end), Some(text.len()));
            for pair in lines.windows(2) {
                let gap = &text[pair[0].end..pair[1].start];
                assert!(gap.is_empty() || gap == " ", "{width}: {gap:?}");
            }
            assert_eq!(lines[0].start, 0);
        }
    }
}
