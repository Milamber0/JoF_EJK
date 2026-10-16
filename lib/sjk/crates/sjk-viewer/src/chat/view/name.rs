//! Tight name hit/hover geometry and a small five-point friend marker.

use super::*;

/// Follow the same atlas metrics and colour escapes as the rendered name.
pub(super) fn ink_bounds(font: &UiFont, name: &str, origin: [f32; 2], size: f32) -> Rect {
    let scale = size / font.height.max(1.0);
    let mut pen = origin[0];
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    let mut index = 0;
    while index < name.len() {
        if name.as_bytes()[index] == b'^'
            && name
                .as_bytes()
                .get(index + 1)
                .is_some_and(u8::is_ascii_digit)
        {
            index += 2;
            continue;
        }
        let (key, step) = crate::text::glyph_at(name, index);
        let glyph = font.glyph_of(TextFace::Semibold, key);
        if glyph.width > 0.0 && glyph.height > 0.0 {
            let x = pen + glyph.offset_x * scale;
            let y = origin[1] + glyph.offset_y * scale;
            bounds[0] = bounds[0].min(x);
            bounds[1] = bounds[1].min(y);
            bounds[2] = bounds[2].max(x + glyph.width * scale);
            bounds[3] = bounds[3].max(y + glyph.height * scale);
        }
        pen += glyph.advance * scale;
        index += step;
    }
    if bounds[0].is_finite() {
        Rect::new(
            bounds[0],
            bounds[1],
            bounds[2] - bounds[0],
            bounds[3] - bounds[1],
        )
    } else {
        Rect::new(origin[0], origin[1], 0.0, 0.0)
    }
}

/// Fixed scanline spans draw a five-point star without relying on a Unicode
/// glyph absent from the legacy Latin-1 atlas. No per-frame tessellation/storage.
pub(crate) fn star(ui: &mut crate::menu_widgets::MenuCanvas, rect: Rect, color: Color) {
    for &(row, left, right) in STAR {
        ui.accent_bar(
            Rect::new(
                rect.x + left * rect.width,
                rect.y + row as f32 * rect.height / 16.0,
                (right - left) * rect.width,
                rect.height / 16.0,
            ),
            color,
        );
    }
}

// Unit-square, upward-facing star sampled through sixteen horizontal bands.
const STAR: &[(u8, f32, f32)] = &[
    (0, 0.48831, 0.51169),
    (1, 0.46494, 0.53506),
    (2, 0.44157, 0.55843),
    (3, 0.41820, 0.58180),
    (4, 0.39483, 0.60517),
    (5, 0.06433, 0.93567),
    (6, 0.10085, 0.89915),
    (7, 0.17942, 0.82058),
    (8, 0.25799, 0.74201),
    (9, 0.29228, 0.70772),
    (10, 0.27495, 0.72505),
    (11, 0.25762, 0.48678),
    (11, 0.51322, 0.74238),
    (12, 0.24029, 0.39234),
    (12, 0.60766, 0.75971),
    (13, 0.22295, 0.29791),
    (13, 0.70209, 0.77705),
];
