//! One character in a fixed cell, without a shadow: the classic console's
//! monospaced grid, as `SCR_DrawSmallChar` draws it.

use super::{GlyphKey, TextFace, TextVertex, UiFont, push_quad_within};

/// Append `key` drawn into the cell `[x, y, width, height]` (pixels) in
/// `color`, into a batch of at most `limit` vertices. The glyph is sized to the
/// cell's height and its advance centred in the cell's width, so the console
/// font ([`super::console_font`]), whose advance is the cell, keeps its own
/// side bearings. A glyph wider than the cell (a kanji, about two cells wide) is
/// shrunk to fit it, centred on the line, so the grid keeps one character per
/// cell. Nothing is drawn for a glyph without ink, such as a space.
pub(crate) fn append_cell(
    vertices: &mut Vec<TextVertex>,
    font: &UiFont,
    key: GlyphKey,
    cell: [f32; 4],
    color: [f32; 4],
    viewport: [f32; 2],
    limit: usize,
) {
    let glyph = font.glyph_of(TextFace::Regular, key);
    if glyph.width <= 0.0 || glyph.height <= 0.0 {
        return;
    }
    let [x, y, width, height] = cell;
    let line = height / font.height.max(1.0);
    let scale = line * (width / (glyph.advance * line)).min(1.0);
    let left = x + (width - glyph.advance * scale) * 0.5 + glyph.offset_x * scale;
    let top = y + (height - font.height * scale) * 0.5;
    push_quad_within(
        vertices,
        [
            left,
            top + glyph.offset_y * scale,
            glyph.width * scale,
            glyph.height * scale,
        ],
        glyph.uv,
        color,
        viewport,
        limit,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_glyphs_draw_in_their_cell_without_a_shadow() {
        let font = super::super::console_font::load().unwrap().font;
        let mut vertices = Vec::new();
        append_cell(
            &mut vertices,
            &font,
            GlyphKey::Slot(b'A'),
            [16.0, 32.0, 16.0, 32.0],
            [1.0; 4],
            [640.0, 480.0],
            usize::MAX,
        );
        // One quad: no shadow quad under it.
        assert_eq!(vertices.len(), 6);
        append_cell(
            &mut vertices,
            &font,
            GlyphKey::Slot(b' '),
            [32.0, 32.0, 16.0, 32.0],
            [1.0; 4],
            [640.0, 480.0],
            usize::MAX,
        );
        assert_eq!(vertices.len(), 6);
        // The limit is respected.
        append_cell(
            &mut vertices,
            &font,
            GlyphKey::Slot(b'B'),
            [32.0, 32.0, 16.0, 32.0],
            [1.0; 4],
            [640.0, 480.0],
            8,
        );
        assert_eq!(vertices.len(), 6);
    }

    #[test]
    fn a_kanji_shrinks_into_its_cell() {
        let font = super::super::console_font::load().unwrap().font;
        let kanji = GlyphKey::Wide('漢');
        font.glyph_of(TextFace::Regular, kanji);
        while super::super::wide::rasterize_pending(std::time::Duration::from_secs(1)) {}
        let mut vertices = Vec::new();
        let cell = [16.0, 32.0, 16.0, 32.0];
        append_cell(
            &mut vertices,
            &font,
            kanji,
            cell,
            [1.0; 4],
            [640.0, 480.0],
            usize::MAX,
        );
        assert_eq!(vertices.len(), 6);
        let [left, top] = vertices[0].window_position([640.0, 480.0]);
        let [right, bottom] = vertices[2].window_position([640.0, 480.0]);
        assert!(
            left >= cell[0] - 0.5 && right <= cell[0] + cell[2] + 0.5,
            "{left} {right}"
        );
        assert!(
            top >= cell[1] && bottom <= cell[1] + cell[3],
            "{top} {bottom}"
        );
    }
}
