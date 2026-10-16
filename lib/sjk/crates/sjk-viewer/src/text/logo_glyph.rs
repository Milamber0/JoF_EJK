//! The "WSI fonts" logo that Jedi Academy draws for `¬`.
//!
//! Byte 0xAC of the retail `ergoec` and `ocr_a` fonts is not a not-sign but
//! the boxed logo of the fonts' foundry, which players have long put in their
//! names. Inter draws a plain `¬` there, so the modern atlas takes this glyph
//! from the bundled SJK Menu font instead ([`super::retail_font::MENU`], where
//! it is a vector glyph like the rest of the font): [`LogoGlyph::bundled`]
//! rasterizes it once, large, and [`LogoGlyph::rasterize`] scales it to Inter's
//! cap height when the atlas is built.

use std::error::Error;

/// The byte (Latin-1 `¬`) the retail fonts draw as the logo.
pub(crate) const BYTE: u8 = 0xAC;
/// The font's own capital, whose height sets the logo's scale.
const CAP: char = 'H';
/// Raster size of the logo against SJK Menu's own raster em: large enough that
/// the biggest Inter atlas (a 3x raster at 3x DPI) only minifies it.
const OVERSAMPLE: f32 = 4.0;

/// The logo's coverage and metrics, in the pixels it was rasterized at.
#[derive(Clone, Debug)]
pub(crate) struct LogoGlyph {
    /// Ink width.
    width: f32,
    /// Ink height.
    height: f32,
    /// Pen advance.
    advance: f32,
    /// Ink left edge from the pen.
    offset_x: f32,
    /// Ink top above the baseline.
    baseline: f32,
    /// Height of the font's `H`, the reference the logo is scaled against.
    cap_height: f32,
    /// Alpha coverage, `coverage_width` by `coverage_height` samples.
    coverage: Vec<u8>,
    coverage_width: usize,
    coverage_height: usize,
}

/// The logo scaled for one rasterized face: fontdue-style metrics in raster
/// pixels plus coverage rows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScaledLogo {
    pub(crate) width: usize,
    pub(crate) height: usize,
    /// Ink left edge from the pen.
    pub(crate) xmin: i32,
    /// Ink bottom above the baseline (negative below), as fontdue's `ymin`.
    pub(crate) ymin: i32,
    pub(crate) advance: f32,
    pub(crate) pixels: Vec<u8>,
}

impl LogoGlyph {
    /// Rasterize the logo from the bundled SJK Menu font.
    pub(crate) fn bundled() -> Result<Self, Box<dyn Error>> {
        let (font, em) = super::retail_font::MENU.font()?;
        let size = em * OVERSAMPLE;
        let (metrics, coverage) = font.rasterize(char::from(BYTE), size);
        let cap_height = font.metrics(CAP, size).height as f32;
        if metrics.width == 0 || metrics.height == 0 || cap_height <= 0.0 {
            return Err("SJK Menu has no logo at 0xAC".into());
        }
        Ok(Self {
            width: metrics.width as f32,
            height: metrics.height as f32,
            advance: metrics.advance_width,
            offset_x: metrics.xmin as f32,
            baseline: (metrics.ymin + metrics.height as i32) as f32,
            cap_height,
            coverage,
            coverage_width: metrics.width,
            coverage_height: metrics.height,
        })
    }

    /// The logo for a face whose `H` is `cap_height` raster pixels tall, so it
    /// keeps the size it has beside the retail font's letters.
    pub(crate) fn rasterize(&self, cap_height: f32) -> ScaledLogo {
        let scale = cap_height / self.cap_height;
        let width = (self.width * scale).round().max(1.0) as usize;
        let height = (self.height * scale).round().max(1.0) as usize;
        let top = (self.baseline * scale).round() as i32;
        // Supersample enough to average every source sample when shrinking,
        // and to interpolate smoothly when enlarging.
        let steps_x = (self.coverage_width as f32 / width as f32).ceil().max(2.0) as usize;
        let steps_y = (self.coverage_height as f32 / height as f32)
            .ceil()
            .max(2.0) as usize;
        let mut pixels = Vec::with_capacity(width * height);
        for row in 0..height {
            for column in 0..width {
                let mut sum = 0.0;
                for sub_y in 0..steps_y {
                    for sub_x in 0..steps_x {
                        let u =
                            (column as f32 + (sub_x as f32 + 0.5) / steps_x as f32) / width as f32;
                        let v =
                            (row as f32 + (sub_y as f32 + 0.5) / steps_y as f32) / height as f32;
                        sum += self.sample(u, v);
                    }
                }
                pixels.push((sum / (steps_x * steps_y) as f32).round() as u8);
            }
        }
        ScaledLogo {
            width,
            height,
            xmin: (self.offset_x * scale).round() as i32,
            ymin: top - height as i32,
            advance: self.advance * scale,
            pixels,
        }
    }

    /// Bilinear coverage at normalized `(u, v)`, clamped to the raster's edges.
    fn sample(&self, u: f32, v: f32) -> f32 {
        let x = (u * self.coverage_width as f32 - 0.5).clamp(0.0, (self.coverage_width - 1) as f32);
        let y =
            (v * self.coverage_height as f32 - 0.5).clamp(0.0, (self.coverage_height - 1) as f32);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = (
            (x0 + 1).min(self.coverage_width - 1),
            (y0 + 1).min(self.coverage_height - 1),
        );
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let at = |x: usize, y: usize| f32::from(self.coverage[y * self.coverage_width + x]);
        let upper = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
        let lower = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
        upper * (1.0 - fy) + lower * fy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4x2 opaque logo with ink 2 above the baseline, advance 5 and offset 1,
    /// beside a capital 2 tall, sampled `fineness` times finer than that.
    fn logo(fineness: usize) -> LogoGlyph {
        LogoGlyph {
            width: 4.0,
            height: 2.0,
            advance: 5.0,
            offset_x: 1.0,
            baseline: 2.0,
            cap_height: 2.0,
            coverage: vec![255; 8 * fineness * fineness],
            coverage_width: 4 * fineness,
            coverage_height: 2 * fineness,
        }
    }

    #[test]
    fn scales_to_the_face_cap_height() {
        // The H is 2 tall; a 10 px cap height is a 5x scale.
        let scaled = logo(1).rasterize(10.0);
        assert_eq!([scaled.width, scaled.height], [20, 10]);
        assert_eq!([scaled.xmin, scaled.ymin], [5, 0]);
        assert_eq!(scaled.advance, 25.0);
        assert_eq!(scaled.pixels.len(), 200);
        assert!(scaled.pixels.iter().all(|&alpha| alpha == 255));
    }

    #[test]
    fn coverage_does_not_bleed_past_the_raster_edges() {
        let mut fine = logo(8);
        // Clear the right half: the left half must stay fully opaque.
        for row in 0..fine.coverage_height {
            let start = row * fine.coverage_width;
            fine.coverage[start + 16..start + 32].fill(0);
        }
        let scaled = fine.rasterize(10.0);
        for row in 0..scaled.height {
            assert!(
                scaled.pixels[row * scaled.width..][..9]
                    .iter()
                    .all(|&a| a == 255)
            );
            assert!(
                scaled.pixels[row * scaled.width + 11..][..9]
                    .iter()
                    .all(|&a| a == 0)
            );
        }
    }

    #[test]
    fn the_bundled_logo_is_a_box_about_a_capital_high() {
        let logo = LogoGlyph::bundled().unwrap();
        assert!(logo.width > logo.height);
        assert!(logo.height > logo.cap_height && logo.height < logo.cap_height * 1.5);
        // Mostly ink: the box is filled around the lettering.
        let ink = logo.coverage.iter().filter(|&&alpha| alpha > 128).count();
        assert!(ink * 2 > logo.coverage.len());
    }

    #[test]
    fn modern_atlas_draws_the_logo_in_both_faces() {
        use super::super::{TextFace, load_modern};
        let logo = logo(1);
        let plain = load_modern(1.0, None).unwrap().font;
        let spliced = load_modern(1.0, Some(&logo)).unwrap().font;
        for face in [TextFace::Regular, TextFace::Semibold] {
            let cap = spliced.glyph(face, b'H');
            let glyph = spliced.glyph(face, BYTE);
            // 4x2 units against a 2-unit H: twice as wide as the cap is tall.
            assert!((glyph.width - 2.0 * cap.height).abs() <= 1.0, "{face:?}");
            assert!((glyph.height - cap.height).abs() <= 1.0, "{face:?}");
            // Ink sits on the baseline, its top level with the capital's.
            assert!((glyph.offset_y - cap.offset_y).abs() <= 1.0, "{face:?}");
            assert_ne!(glyph.advance, plain.glyph(face, BYTE).advance);
            // Other glyphs are untouched.
            assert_eq!(cap.advance, plain.glyph(face, b'H').advance);
        }
    }
}
