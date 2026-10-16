//! Bundled vector replacements for Jedi Academy's bitmap game fonts.
//!
//! SJK draws no bitmap fonts (docs/sjk.md "Fonts"), so the retail `ergoec`,
//! `ocr_a` and `arialnb` atlases give way to three bundled TrueType fonts built by
//! `scripts/game_fonts.py`: [`MENU`] (`ergoec`) and [`HUD`] (`arialnb`) are traced
//! from an HD replacement of the retail atlas, [`CHAT`] (`ocr_a`) is OCR-A set to
//! the retail widths. Each is rasterized once into a coverage atlas, like the
//! console font ([`super::console_font`]).
//!
//! The fonts keep the retail `.fontdat` layout contract, so every surface lays out
//! exactly as it did with the bitmaps: outlines are drawn at [`UNITS_PER_PIXEL`]
//! font units per retail pixel, a glyph advances its retail width, and the line
//! height and baseline are the ones the `.fontdat` header gave. Slots hold the
//! Windows-1252 character of their byte; bytes a font lacks draw `.`, as
//! `RE_Font_DrawString` drew for a glyph with no width.

use super::{
    FontAtlas, FontGlyph, GLYPH_COUNT, RasterizedGlyph, TextStyle, UiFont, paint_atlas,
    slot_character, wide,
};
use fontdue::{Font, FontSettings};
use std::error::Error;

/// Font units per retail pixel in the bundled fonts.
const UNITS_PER_PIXEL: f32 = 64.0;
/// Raster pixels per retail pixel: a 13-pixel retail capital rasterizes about
/// 80 pixels tall, enough for 4K menu text, and the mip chain covers 1080p.
const RASTER_SCALE: f32 = 6.0;

/// One bundled font and the retail layout it keeps.
pub(crate) struct RetailFace {
    /// Name used in the log.
    pub(crate) name: &'static str,
    bytes: &'static [u8],
    /// Line height in retail pixels.
    height: f32,
    /// Baseline below the line top, in retail pixels.
    baseline: f32,
    /// Fonts lending it the characters beyond its slots ([`super::wide`]), the
    /// closest in style that has Cyrillic, then kana and kanji.
    wide: &'static [wide::Source],
}

/// SJK Menu, in place of `ergoec` (`FONT_MEDIUM`): `mHeight` 22, `mAscender` 17.
pub(crate) const MENU: RetailFace = RetailFace {
    name: "SJK Menu",
    bytes: include_bytes!("../../assets/fonts/SJKMenu.ttf"),
    height: 22.0,
    baseline: 17.0,
    wide: wide::EXO2[0],
};

/// SJK Chat, in place of `ocr_a` (`FONT_SMALL`): `mHeight` 21, `mAscender` 17.
pub(crate) const CHAT: RetailFace = RetailFace {
    name: "SJK Chat",
    bytes: include_bytes!("../../assets/fonts/SJKChat.ttf"),
    height: 21.0,
    baseline: 17.0,
    wide: wide::MONO,
};

/// SJK HUD, in place of `arialnb`, the classic status HUD's font. Its header
/// is empty, so the line is the 14-point size with the baseline on its bottom.
pub(crate) const HUD: RetailFace = RetailFace {
    name: "SJK HUD",
    bytes: include_bytes!("../../assets/fonts/SJKHud.ttf"),
    height: 14.0,
    baseline: 14.0,
    wide: wide::INTER[1],
};

impl RetailFace {
    /// The parsed font and its raster em size in pixels.
    pub(crate) fn font(&self) -> Result<(Font, f32), Box<dyn Error>> {
        let font = Font::from_bytes(self.bytes, FontSettings::default())?;
        let em = font.units_per_em() / UNITS_PER_PIXEL * RASTER_SCALE;
        Ok((font, em))
    }
}

/// Rasterize `face` into a coverage atlas with mipmappable padding, its glyphs
/// in retail pixels.
pub(crate) fn load(face: &RetailFace) -> Result<FontAtlas, Box<dyn Error>> {
    let (font, em) = face.font()?;
    let rasterized: Vec<RasterizedGlyph> = (0..GLYPH_COUNT)
        .map(|byte| {
            let (metrics, pixels) = font.rasterize(slot_character(&font, byte as u8), em);
            RasterizedGlyph {
                face: 0,
                byte,
                metrics,
                pixels,
            }
        })
        .collect();
    let (image, rectangles) = paint_atlas(&rasterized);
    let mut glyphs = [[FontGlyph::default(); GLYPH_COUNT]; 2];
    for (glyph, uv) in rasterized.iter().zip(rectangles) {
        let metrics = glyph.metrics;
        let ink_top = (metrics.ymin as f32 + metrics.height as f32) / RASTER_SCALE;
        let entry = FontGlyph {
            width: metrics.width as f32 / RASTER_SCALE,
            height: metrics.height as f32 / RASTER_SCALE,
            // Retail advances are whole pixels; drop the raster's float error.
            advance: (metrics.advance_width / RASTER_SCALE).round(),
            offset_x: metrics.xmin as f32 / RASTER_SCALE,
            offset_y: face.baseline - ink_top,
            uv,
        };
        glyphs[0][glyph.byte] = entry;
        glyphs[1][glyph.byte] = entry;
    }
    Ok(FontAtlas {
        font: UiFont {
            glyphs,
            wide: [face.wide; 2],
            height: face.height,
            modern: false,
            style: TextStyle::NEUTRAL,
        },
        image,
        distance_field: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::TextFace;

    fn glyph(font: &UiFont, byte: u8) -> FontGlyph {
        font.glyph(TextFace::Regular, byte)
    }

    #[test]
    fn faces_keep_the_retail_line_and_whole_pixel_advances() {
        for face in [&MENU, &CHAT, &HUD] {
            let font = load(face).unwrap().font;
            assert_eq!(font.height, face.height, "{}", face.name);
            assert!(!font.is_modern());
            for byte in 0x20..=0xFF_u8 {
                let advance = glyph(&font, byte).advance;
                assert!(
                    advance > 0.0 && advance.fract() == 0.0,
                    "{} {byte}",
                    face.name
                );
            }
        }
    }

    #[test]
    fn capitals_stand_on_the_baseline() {
        for face in [&MENU, &CHAT, &HUD] {
            let font = load(face).unwrap().font;
            let cap = glyph(&font, b'H');
            let bottom = cap.offset_y + cap.height;
            // Within the anti-aliased pixel row the retail glyphs kept below it.
            assert!(
                (bottom - face.baseline).abs() <= 1.0,
                "{} {cap:?}",
                face.name
            );
            assert!(
                cap.height > 8.0 && cap.height < 15.0,
                "{} {cap:?}",
                face.name
            );
        }
    }

    #[test]
    fn chat_letters_sit_centred_in_their_advance() {
        let font = load(&CHAT).unwrap().font;
        for byte in (b'0'..=b'9').chain(b'A'..=b'Z').chain(b'a'..=b'z') {
            let g = glyph(&font, byte);
            let left = g.offset_x;
            let right = g.advance - g.offset_x - g.width;
            // Within the raster's rounding of the ink box to whole pixels.
            assert!(
                (left - right).abs() <= 0.5,
                "{:?} left {left} right {right}",
                byte as char
            );
        }
    }

    #[test]
    fn latin_1_is_drawn_and_c1_bytes_fall_back_to_a_dot() {
        for face in [&MENU, &CHAT, &HUD] {
            let font = load(face).unwrap().font;
            let dot = glyph(&font, b'.');
            for byte in (0x21..=0x7E_u8).chain(0xA1..=0xFF) {
                assert!(glyph(&font, byte).width > 0.0, "{} {byte:#x}", face.name);
            }
            // 0x81 has no Windows-1252 character.
            let missing = glyph(&font, 0x81);
            assert_eq!(
                [missing.width, missing.height, missing.advance],
                [dot.width, dot.height, dot.advance],
                "{}",
                face.name
            );
        }
    }

    #[test]
    fn the_menu_and_chat_fonts_draw_the_logo_at_0xac() {
        for (face, logo) in [(&MENU, true), (&CHAT, true), (&HUD, false)] {
            let font = load(face).unwrap().font;
            let glyph = glyph(&font, 0xAC);
            let cap = font.glyph(TextFace::Regular, b'H');
            // The boxed logo is wider than tall and about a capital high; the
            // not sign is a short bar.
            assert!(glyph.width > glyph.height, "{}", face.name);
            assert_eq!(glyph.height > cap.height * 0.8, logo, "{}", face.name);
        }
    }

    #[test]
    fn atlases_stay_coverage() {
        let atlas = load(&MENU).unwrap();
        assert!(atlas.image.width() >= crate::text::sdf::HD_LONG_SIDE);
        assert!(!atlas.distance_field);
    }
}
