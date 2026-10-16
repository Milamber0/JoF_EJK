//! Radiant-style tiles for the invisible faces: each kind's label written in
//! the middle of a framed square, like the editor images of `textures/system`
//! (which retail does not ship). One layer per kind; red holds the label's
//! coverage, green the frame. The colour comes from the kind in the shader.

use super::kinds::Kind;
use fontdue::{Font, FontSettings};
use image::{Rgba, RgbaImage};

/// Tile edge in pixels.
pub(super) const SIZE: u32 = 128;
/// Frame width in pixels.
const FRAME: u32 = 3;
/// Widest a label line may be, in pixels.
const MAX_WIDTH: f32 = 112.0;
/// Label height for a one-line label; two lines share the tile.
const ONE_LINE_PX: f32 = 46.0;
const TWO_LINE_PX: f32 = 36.0;

/// One tile per [`Kind`], in [`Kind::ALL`] order; frames alone if the bundled
/// font failed to load.
pub(super) fn build() -> Vec<RgbaImage> {
    let font = Font::from_bytes(crate::text::RAJDHANI_BOLD, FontSettings::default()).ok();
    Kind::ALL
        .into_iter()
        .map(|kind| tile(font.as_ref(), kind.label()))
        .collect()
}

fn tile(font: Option<&Font>, lines: &[&str]) -> RgbaImage {
    let mut image = RgbaImage::from_fn(SIZE, SIZE, |x, y| {
        let edge = x.min(y).min(SIZE - 1 - x).min(SIZE - 1 - y);
        Rgba([0, if edge < FRAME { 255 } else { 0 }, 0, 255])
    });
    let Some(font) = font else {
        return image;
    };
    let mut px = if lines.len() > 1 {
        TWO_LINE_PX
    } else {
        ONE_LINE_PX
    };
    let widest = lines
        .iter()
        .map(|line| width(font, line, px))
        .fold(0.0, f32::max);
    if widest > MAX_WIDTH {
        px *= MAX_WIDTH / widest;
    }
    let line_height = px * 0.9;
    let top = (SIZE as f32 - line_height * lines.len() as f32) * 0.5;
    for (row, line) in lines.iter().enumerate() {
        // Rajdhani's capitals stand about 0.7 em above the baseline.
        let baseline = top + line_height * row as f32 + (line_height + px * 0.7) * 0.5;
        let mut pen = (SIZE as f32 - width(font, line, px)) * 0.5;
        for character in line.chars() {
            let (metrics, coverage) = font.rasterize(character, px);
            let left = (pen + metrics.xmin as f32).round() as i32;
            let glyph_top =
                (baseline - (metrics.height as i32 + metrics.ymin) as f32).round() as i32;
            for gy in 0..metrics.height {
                for gx in 0..metrics.width {
                    let (x, y) = (left + gx as i32, glyph_top + gy as i32);
                    if (0..SIZE as i32).contains(&x) && (0..SIZE as i32).contains(&y) {
                        let texel = image.get_pixel_mut(x as u32, y as u32);
                        texel[0] = texel[0].max(coverage[gy * metrics.width + gx]);
                    }
                }
            }
            pen += metrics.advance_width;
        }
    }
    image
}

fn width(font: &Font, line: &str, px: f32) -> f32 {
    line.chars()
        .map(|character| font.metrics(character, px).advance_width)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_framed_labelled_tile() {
        let tiles = build();
        assert_eq!(tiles.len(), Kind::ALL.len());
        for (kind, tile) in Kind::ALL.into_iter().zip(&tiles) {
            assert_eq!(tile.dimensions(), (SIZE, SIZE));
            assert_eq!(tile.get_pixel(0, 0)[1], 255, "{kind:?} frame");
            assert_eq!(tile.get_pixel(SIZE / 2, FRAME + 1)[1], 0, "{kind:?} inside");
            let inked = tile.pixels().filter(|texel| texel[0] > 128).count();
            assert!(inked > 200, "{kind:?} label has {inked} inked texels");
            // The label stays clear of the frame.
            for i in 0..SIZE {
                for (x, y) in [
                    (i, FRAME),
                    (FRAME, i),
                    (i, SIZE - 1 - FRAME),
                    (SIZE - 1 - FRAME, i),
                ] {
                    assert_eq!(tile.get_pixel(x, y)[0], 0, "{kind:?} at {x},{y}");
                }
            }
        }
    }
}
