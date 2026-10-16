//! The resolution scale shared by 2D UI layouts.
//!
//! Menus, HUD, chat, scoreboard and console are authored in pixels of a
//! 1080-line screen and grow with the window height, as retail's 640x480
//! virtual screen did, so a layout looks the same at 1440 or 2160 lines as at
//! 1080. The operating system's display scale is deliberately not applied:
//! a fullscreen window already covers the display, so multiplying by it would
//! count the display density twice, and in a small window on a scaled desktop
//! it would push 1080-line layouts past the window edges. It only sets the
//! resolution the bundled font is rasterized at ([`crate::text::load_modern`]).

use crate::text::UiFont;

/// Window height a scale of 1 corresponds to.
pub(crate) const REFERENCE_HEIGHT: f32 = 1_080.0;
/// Smallest scale, for windows under 648 lines.
pub(crate) const MIN: f32 = 0.6;
/// Largest scale (2700 lines). There the largest menu text nears the bundled
/// font's raster resolution at 100% display scaling, so layouts stop growing.
pub(crate) const MAX: f32 = 2.5;

/// UI scale for a window `viewport_height` physical pixels tall.
pub(crate) fn height_scale(viewport_height: f32) -> f32 {
    (viewport_height / REFERENCE_HEIGHT).clamp(MIN, MAX)
}

/// Glyph scale that draws `font` with a `line`-pixel line box at 1080 lines,
/// grown by `scale`, for the raw [`crate::text::append_text`] calls.
///
/// Those take a multiple of the font's own units, and the bundled font's
/// units follow the display scale it was rasterized for; sizing by line
/// height keeps such text independent of the display scale, like the rest of
/// the UI.
pub(crate) fn glyph_scale(font: &UiFont, line: f32, scale: f32) -> f32 {
    line * scale / font.height.max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_proportionally_between_the_bounds() {
        assert_eq!(height_scale(1_080.0), 1.0);
        assert_eq!(height_scale(1_440.0), 1_440.0 / 1_080.0);
        assert_eq!(height_scale(2_160.0), 2.0);
    }

    #[test]
    fn clamps_tiny_and_huge_windows() {
        assert_eq!(height_scale(480.0), MIN);
        assert_eq!(height_scale(4_320.0), MAX);
    }

    #[test]
    fn raw_text_size_ignores_the_display_scale() {
        for display_scale in [1.0, 1.5, 2.0] {
            let font = crate::text::load_modern(display_scale, None).unwrap().font;
            let line = font.height * glyph_scale(&font, 42.6, 2.0);
            assert!((line - 85.2).abs() < 1e-3, "{display_scale}: {line}");
        }
    }
}
