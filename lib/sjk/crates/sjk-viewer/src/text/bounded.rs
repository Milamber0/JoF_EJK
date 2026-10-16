//! Retained text stays inside its layout rectangle, including glyph shadows.
use super::*;
use sjk_ui::{Rect, TextOverflow};

/// Submit bounded text without letting the legacy viewport wrapping escape a row.
#[allow(clippy::too_many_arguments)]
pub(crate) fn append_bounded(
    vertices: &mut Vec<TextVertex>,
    font: &UiFont,
    value: &str,
    origin: [f32; 2],
    rect: Rect,
    clip: Rect,
    scale: f32,
    viewport: [f32; 2],
    face: TextFace,
    base_color: [f32; 4],
    spacing: f32,
    overflow: TextOverflow,
    palette: CodePalette,
) {
    let mut cursor = origin;
    let mut color = base_color;
    let bytes = value.as_bytes();
    let mut index = 0;
    let ellipsis_width = 3.0 * (font.glyph(face, b'.').advance * scale + spacing);
    let truncate = overflow == TextOverflow::Ellipsis
        && visible_text_width_style(font, value, scale, face, spacing) > rect.width;
    let right = rect.right() - if truncate { ellipsis_width } else { 0.0 };
    while index < bytes.len() && cursor[1] < clip.bottom() {
        if bytes[index] == b'^' && index + 1 < bytes.len() && bytes[index + 1].is_ascii_digit() {
            color = palette.colour(bytes[index + 1] - b'0');
            color[3] *= base_color[3];
            index += 2;
            continue;
        }
        let (key, step) = super::glyph_at(value, index);
        index += step;
        let glyph = font.glyph_of(face, key);
        let advance = glyph.advance * scale + spacing;
        let newline = key == GlyphKey::Slot(b'\n');
        if newline || cursor[0] + advance > right {
            if overflow == TextOverflow::Wrap {
                cursor = [rect.x, cursor[1] + font.height * scale * 1.18];
                if newline {
                    continue;
                }
            } else if newline || truncate {
                if truncate {
                    for _ in 0..3 {
                        glyph_quad(
                            vertices,
                            font.glyph(face, b'.'),
                            cursor,
                            scale,
                            color,
                            clip,
                            viewport,
                        );
                        cursor[0] += ellipsis_width / 3.0;
                    }
                }
                break;
            }
        }
        glyph_quad(vertices, glyph, cursor, scale, color, clip, viewport);
        cursor[0] += advance;
        if overflow == TextOverflow::Clip && cursor[0] >= clip.right() {
            break;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn glyph_quad(
    vertices: &mut Vec<TextVertex>,
    glyph: FontGlyph,
    cursor: [f32; 2],
    scale: f32,
    color: [f32; 4],
    clip: Rect,
    viewport: [f32; 2],
) {
    let rect = Rect::new(
        cursor[0] + glyph.offset_x * scale,
        cursor[1] + glyph.offset_y * scale,
        glyph.width * scale,
        glyph.height * scale,
    );
    let shadow = Rect::new(
        rect.x + scale.max(1.0),
        rect.y + scale.max(1.0),
        rect.width,
        rect.height,
    );
    clipped_quad(
        vertices,
        shadow,
        glyph.uv,
        [0.0, 0.0, 0.0, 0.55 * color[3]],
        clip,
        viewport,
    );
    clipped_quad(vertices, rect, glyph.uv, color, clip, viewport);
}

fn clipped_quad(
    vertices: &mut Vec<TextVertex>,
    rect: Rect,
    uv: [f32; 4],
    color: [f32; 4],
    clip: Rect,
    viewport: [f32; 2],
) {
    let left = rect.x.max(clip.x);
    let top = rect.y.max(clip.y);
    let right = rect.right().min(clip.right());
    let bottom = rect.bottom().min(clip.bottom());
    if right <= left || bottom <= top {
        return;
    }
    let u = |x| uv[0] + (uv[2] - uv[0]) * (x - rect.x) / rect.width;
    let v = |y| uv[1] + (uv[3] - uv[1]) * (y - rect.y) / rect.height;
    push_quad(
        vertices,
        [left, top, right - left, bottom - top],
        [u(left), v(top), u(right), v(bottom)],
        color,
        viewport,
    );
}
