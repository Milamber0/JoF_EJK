//! Caret stops of a line of console text, shared by the text inputs that draw
//! an editable line: where each glyph starts and where to start drawing so the
//! caret stays in view.

use crate::text::{GlyphKey, glyph_at};

/// Walk the caret stops of `text`: before each glyph (with any colour codes that lead
/// it) and at the end. `stop(index, x, advance)` gets each stop's byte offset, its
/// distance from the text's start and the advance of the glyph after it (zero at the
/// end); returning `true` ends the walk at that stop, whose offset is returned.
pub(crate) fn walk(
    text: &str,
    advance: impl Fn(GlyphKey) -> f32,
    mut stop: impl FnMut(usize, f32, f32) -> bool,
) -> usize {
    let bytes = text.as_bytes();
    let (mut index, mut unit, mut pen) = (0, 0, 0.0);
    while index < bytes.len() {
        if bytes[index] == b'^' && bytes.get(index + 1).is_some_and(u8::is_ascii_digit) {
            index += 2;
            continue;
        }
        let (glyph, step) = glyph_at(text, index);
        let width = advance(glyph);
        if stop(unit, pen, width) {
            return unit;
        }
        pen += width;
        index += step;
        unit = index;
    }
    stop(text.len(), pen, 0.0);
    text.len()
}

/// First caret stop from which `text[..cursor]` fits in `room`: where to start drawing
/// so the caret stays visible.
pub(crate) fn scroll_start(
    text: &str,
    cursor: usize,
    room: f32,
    advance: impl Fn(GlyphKey) -> f32 + Copy,
) -> usize {
    let mut caret = 0.0;
    walk(&text[..cursor], advance, |_, pen, _| {
        caret = pen;
        false
    });
    if caret <= room {
        return 0;
    }
    walk(&text[..cursor], advance, |_, pen, _| caret - pen <= room)
}
