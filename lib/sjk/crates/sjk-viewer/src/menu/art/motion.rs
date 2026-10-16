//! Retail menu motion: what the original menus animate, as functions of a
//! shared clock, so the classic views and the renderer agree.
//!
//! None of it is scripted in the `.menu` files, whose page swaps are instant
//! (`close all ; open ...`) and whose fades step 0.1 per millisecond
//! (`fadeCycle 1`, `fadeAmount 0.1`). It comes from the artwork's shaders in
//! `shaders/ui.shader` and from the menu code (`ui_shared.c`):
//!
//! - `main_ring` turns (`tcMod rotate 5`), see [`rotation`];
//! - the side glyph columns climb (`tcMod scroll 0 0.025`) and the logo's
//!   `env_logo` reflection drifts (`tcMod scroll 0.03 0.05`), see [`scroll`];
//! - the focus glows and the in-game bar flicker with four layers of
//!   `gfx/hud/static_menu` multiplied over them (`blendFunc GL_DST_COLOR
//!   GL_ONE`, `tcMod scroll ±1`, `±1.3` or `±1.7`), see [`flicker`] and
//!   [`compose_flicker`];
//! - the focused item's text breathes between its colour and 80% of it
//!   (`Item_TextColor`, `PULSE_DIVISOR` 75), see [`pulse`].
//!
//! SJK adds its own: the menu emblem ([`crate::menu::emblem`]) that stands
//! where retail played its logo video. Its orange core and ring breathe on a
//! slow sine ([`emblem_core_glow`]) and the blade's cyan lights shimmer
//! faster and less ([`emblem_lights_glow`]); both are the strengths of
//! additive glow layers over the still emblem.

use super::ArtPiece;
use image::RgbaImage;
use sjk_ui::Color;
use std::sync::OnceLock;
use std::time::Instant;

/// `main_ring`'s `tcMod rotate`, in degrees per second.
pub(crate) const RING_DEGREES_PER_SECOND: f32 = 5.0;
/// `menu_side_text`'s `tcMod scroll`, in textures per second.
pub(crate) const SIDE_SCROLL: [f32; 2] = [0.0, 0.025];
/// `jediacademy`'s `env_logo` stage: `tcMod scroll` and `alphaGen const`.
pub(crate) const LOGO_REFLECTION_SCROLL: [f32; 2] = [0.03, 0.05];
pub(crate) const LOGO_REFLECTION_ALPHA: f32 = 0.25;
/// `PULSE_DIVISOR` (`q_shared.h`): milliseconds per radian of the focus pulse.
const PULSE_DIVISOR: f64 = 75.0;
/// Seconds per breath of the emblem's core glow.
pub(crate) const EMBLEM_CORE_PERIOD: f64 = 4.2;
/// Weakest and strongest core glow; the cycle starts at the weakest, so the
/// glow swells as the menu appears.
pub(crate) const EMBLEM_CORE_RANGE: [f32; 2] = [0.08, 0.45];
/// Mean strength of the blade lights' glow and how far it strays from it.
pub(crate) const EMBLEM_LIGHTS_BASE: f32 = 0.30;
pub(crate) const EMBLEM_LIGHTS_DEPTH: f32 = 0.15;
/// Periods, in seconds, of the two sines the shimmer sums; their ratio is
/// not a simple fraction, so the pattern does not visibly repeat.
const EMBLEM_LIGHTS_PERIODS: [f64; 2] = [0.9, 0.37];

/// Seconds on the menu clock, which starts the first time it is read.
pub(crate) fn seconds() -> f64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// `color` as retail draws a focused item at `seconds`: `Item_TextColor`
/// lerps from the focus colour towards 80% of it (alpha included) by
/// `0.5 + 0.5 sin(ms / 75)`.
pub(crate) fn pulse(color: Color, seconds: f64) -> Color {
    let toward_low = 0.5 + 0.5 * (seconds * 1_000.0 / PULSE_DIVISOR).sin();
    let factor = 1.0 - 0.2 * toward_low as f32;
    Color::new(
        color.r * factor,
        color.g * factor,
        color.b * factor,
        color.a * factor,
    )
}

/// Strength of the emblem's orange core glow at `seconds`: a raised cosine
/// between [`EMBLEM_CORE_RANGE`] with period [`EMBLEM_CORE_PERIOD`].
pub(crate) fn emblem_core_glow(seconds: f64) -> f32 {
    let [low, high] = EMBLEM_CORE_RANGE;
    let swell = 0.5 - 0.5 * (std::f64::consts::TAU * seconds / EMBLEM_CORE_PERIOD).cos();
    low + (high - low) * swell as f32
}

/// Strength of the emblem's cyan blade-light glow at `seconds`: a quick,
/// shallow shimmer of [`EMBLEM_LIGHTS_DEPTH`] about [`EMBLEM_LIGHTS_BASE`].
pub(crate) fn emblem_lights_glow(seconds: f64) -> f32 {
    let [fast, faster] =
        EMBLEM_LIGHTS_PERIODS.map(|period| std::f64::consts::TAU * seconds / period);
    let wave = 0.6 * fast.sin() + 0.4 * (faster + 1.3).sin();
    EMBLEM_LIGHTS_BASE + EMBLEM_LIGHTS_DEPTH * wave as f32
}

/// Corner texture coordinates (top-left, top-right, bottom-right,
/// bottom-left) of a quad whose texture turns at `degrees_per_second`
/// about its centre, as `RB_CalcRotateTexCoords` (`tr_shade_calc.cpp`):
/// the coordinates turn by `-degrees_per_second * seconds`.
pub(crate) fn rotation(degrees_per_second: f32, seconds: f64) -> [[f32; 2]; 4] {
    let degrees = (-f64::from(degrees_per_second) * seconds).rem_euclid(360.0);
    let (sin, cos) = degrees.to_radians().sin_cos();
    let (sin, cos) = (sin as f32, cos as f32);
    let turn = |[s, t]: [f32; 2]| {
        [
            s * cos - t * sin + (0.5 - 0.5 * cos + 0.5 * sin),
            s * sin + t * cos + (0.5 - 0.5 * sin - 0.5 * cos),
        ]
    };
    [
        turn([0.0, 0.0]),
        turn([1.0, 0.0]),
        turn([1.0, 1.0]),
        turn([0.0, 1.0]),
    ]
}

/// Corner texture coordinates of a quad whose (wrapping) texture scrolls by
/// `speed` textures per second (`tcMod scroll`); the offset is kept in
/// `[0, 1)` so precision does not drift.
pub(crate) fn scroll(speed: [f32; 2], seconds: f64) -> [[f32; 2]; 4] {
    let offset = |speed: f32| (f64::from(speed) * seconds).rem_euclid(1.0) as f32;
    let [s, t] = [offset(speed[0]), offset(speed[1])];
    [[s, t], [s + 1.0, t], [s + 1.0, t + 1.0], [s, t + 1.0]]
}

/// Horizontal scroll speeds of the four `static_menu` layers a piece's
/// shader multiplies over it, for the pieces that flicker.
pub(crate) fn flicker(piece: ArtPiece) -> Option<[f32; 4]> {
    match piece {
        // `gfx/menus/menu_buttonback`.
        ArtPiece::ButtonBack => Some([-1.0, 1.0, -1.3, 1.3]),
        // `gfx/menus/menu_blendbox`, `menu_blendbox2` and `menu_top_mp`.
        ArtPiece::BlendBox | ArtPiece::BlendBox2 | ArtPiece::TopBar => Some([1.0, -1.0, 1.7, -1.7]),
        _ => None,
    }
}

/// Recompose a flickering piece at `seconds` into `out` (RGBA, `base`'s
/// size), ready to upload. Retail adds the piece to the screen, then each
/// noise layer multiplies the screen by `1 + noise`; over SJK's
/// alpha-blended UI the product is taken over the piece alone and turned
/// into colour and coverage as the static pieces are
/// ([`super::additive_texel`]). Noise is sampled at the nearest texel.
pub(crate) fn compose_flicker(
    base: &RgbaImage,
    noise: &RgbaImage,
    speeds: [f32; 4],
    seconds: f64,
    out: &mut [u8],
) {
    let (width, height) = base.dimensions();
    let (noise_width, noise_height) = noise.dimensions();
    if out.len() != (width * height * 4) as usize || noise_width == 0 || noise_height == 0 {
        return;
    }
    let offsets = speeds.map(|speed| (f64::from(speed) * seconds).rem_euclid(1.0) as f32);
    for y in 0..height {
        let noise_y = (((y as f32 + 0.5) / height as f32) * noise_height as f32) as u32;
        let noise_y = noise_y.min(noise_height - 1);
        for x in 0..width {
            let s = (x as f32 + 0.5) / width as f32;
            let mut gain = [1.0_f32; 3];
            for offset in offsets {
                let noise_x =
                    (((s + offset).fract() * noise_width as f32) as u32).min(noise_width - 1);
                let texel = noise.get_pixel(noise_x, noise_y).0;
                for channel in 0..3 {
                    gain[channel] *= 1.0 + f32::from(texel[channel]) / 255.0;
                }
            }
            let [r, g, b, a] = base.get_pixel(x, y).0;
            let lit = |channel: u8, gain: f32| (f32::from(channel) * gain).min(255.0) as u8;
            let texel =
                super::additive_texel([lit(r, gain[0]), lit(g, gain[1]), lit(b, gain[2]), a]);
            let at = ((y * width + x) * 4) as usize;
            out[at..at + 4].copy_from_slice(&texel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4
    }

    #[test]
    fn rotation_starts_square_and_turns_about_the_centre() {
        let start = rotation(RING_DEGREES_PER_SECOND, 0.0);
        for (corner, expected) in start
            .iter()
            .zip([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
        {
            assert!(close(*corner, expected));
        }
        // 18 s at 5 degrees per second: a quarter turn, the coordinates
        // turned by -90 degrees, so the top-left corner samples the top-right.
        let quarter = rotation(RING_DEGREES_PER_SECOND, 18.0);
        assert!(close(quarter[0], [0.0, 1.0]), "{:?}", quarter[0]);
        assert!(close(quarter[1], [0.0, 0.0]), "{:?}", quarter[1]);
        // The centre never moves.
        let mid = |uv: [[f32; 2]; 4]| [(uv[0][0] + uv[2][0]) * 0.5, (uv[0][1] + uv[2][1]) * 0.5];
        assert!(close(mid(rotation(5.0, 7.3)), [0.5, 0.5]));
        // A full turn comes back.
        let full = rotation(RING_DEGREES_PER_SECOND, 72.0);
        assert!(close(full[2], [1.0, 1.0]));
    }

    #[test]
    fn scrolling_wraps_its_offset() {
        let uv = scroll(SIDE_SCROLL, 10.0);
        assert!(close(uv[0], [0.0, 0.25]));
        assert!(close(uv[2], [1.0, 1.25]));
        // 40 s is one full texture: back to the start.
        assert!(close(scroll(SIDE_SCROLL, 40.0)[0], [0.0, 0.0]));
        assert!(close(scroll([-1.0, 0.0], 0.25)[0], [0.75, 0.0]));
    }

    #[test]
    fn pulse_breathes_between_full_and_eighty_percent() {
        let white = Color::new(1.0, 1.0, 1.0, 1.0);
        // sin(0) = 0: halfway, 90%.
        assert!((pulse(white, 0.0).r - 0.9).abs() < 1e-6);
        let mut low = f32::MAX;
        let mut high = f32::MIN;
        for step in 0..1_000 {
            let color = pulse(white, f64::from(step) * 0.001);
            low = low.min(color.r);
            high = high.max(color.a);
        }
        assert!((low - 0.8).abs() < 1e-3 && (high - 1.0).abs() < 1e-3);
        // One cycle is 2 pi * 75 ms.
        let period = std::f64::consts::TAU * 0.075;
        assert!((pulse(white, 0.123).r - pulse(white, 0.123 + period).r).abs() < 1e-5);
    }

    #[test]
    fn emblem_core_breathes_slowly_within_its_range() {
        let [low, high] = EMBLEM_CORE_RANGE;
        assert!((emblem_core_glow(0.0) - low).abs() < 1e-6);
        assert!((emblem_core_glow(EMBLEM_CORE_PERIOD * 0.5) - high).abs() < 1e-5);
        let (mut least, mut most) = (f32::MAX, f32::MIN);
        for step in 0..10_000 {
            let glow = emblem_core_glow(f64::from(step) * 0.001);
            least = least.min(glow);
            most = most.max(glow);
        }
        assert!(least >= low - 1e-6 && most <= high + 1e-6);
        // One breath per period, even after an hour on the menu.
        let late = 3_600.0 + 1.234;
        assert!(
            (emblem_core_glow(late) - emblem_core_glow(late + EMBLEM_CORE_PERIOD)).abs() < 1e-4
        );
        // Gentle: no more than a 0.05 change in a 60 Hz frame's worth of time.
        for step in 0..1_000 {
            let at = f64::from(step) * 0.01;
            assert!((emblem_core_glow(at + 1.0 / 60.0) - emblem_core_glow(at)).abs() < 0.05);
        }
    }

    #[test]
    fn emblem_lights_shimmer_faster_and_shallower_than_the_core() {
        let (mut least, mut most) = (f32::MAX, f32::MIN);
        let (mut lights_travel, mut core_travel) = (0.0_f32, 0.0_f32);
        let mut previous = (emblem_lights_glow(0.0), emblem_core_glow(0.0));
        for step in 1..20_000 {
            let at = f64::from(step) * 0.001;
            let now = (emblem_lights_glow(at), emblem_core_glow(at));
            least = least.min(now.0);
            most = most.max(now.0);
            lights_travel += (now.0 - previous.0).abs();
            core_travel += (now.1 - previous.1).abs();
            previous = now;
        }
        let bound = EMBLEM_LIGHTS_DEPTH + 1e-6;
        assert!(least >= EMBLEM_LIGHTS_BASE - bound && most <= EMBLEM_LIGHTS_BASE + bound);
        assert!(least > 0.0 && most < 1.0);
        // It moves: most of its depth is used.
        assert!(most - least > EMBLEM_LIGHTS_DEPTH);
        // Shallower swing than the core, but more motion over the same time.
        let [low, high] = EMBLEM_CORE_RANGE;
        assert!(most - least < high - low);
        assert!(
            lights_travel > core_travel * 2.0,
            "{lights_travel} vs {core_travel}"
        );
    }

    #[test]
    fn only_the_static_shaders_flicker() {
        assert_eq!(flicker(ArtPiece::ButtonBack), Some([-1.0, 1.0, -1.3, 1.3]));
        assert!(flicker(ArtPiece::TopBar).is_some());
        assert!(flicker(ArtPiece::Ring).is_none());
        assert!(ArtPiece::ButtonBack.dynamic());
        assert!(!ArtPiece::Logo.dynamic());
    }

    #[test]
    fn flicker_brightens_the_glow_by_its_noise() {
        let base = RgbaImage::from_pixel(4, 2, image::Rgba([40, 80, 120, 255]));
        let mut out = vec![0_u8; 4 * 2 * 4];
        // No noise: the glow as the static piece converts it.
        let quiet = RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255]));
        compose_flicker(&base, &quiet, [1.0; 4], 3.7, &mut out);
        assert_eq!(&out[..4], &super::super::additive_texel([40, 80, 120, 255]));
        // Full noise doubles four times, clamped at white.
        let loud = RgbaImage::from_pixel(2, 2, image::Rgba([255, 255, 255, 255]));
        compose_flicker(&base, &loud, [1.0; 4], 0.0, &mut out);
        assert_eq!(&out[..4], &[255, 255, 255, 255]);
        // A wrong-sized output is left alone.
        let mut short = vec![7_u8; 3];
        compose_flicker(&base, &loud, [1.0; 4], 0.0, &mut short);
        assert_eq!(short, [7, 7, 7]);
    }

    #[test]
    fn flicker_noise_moves_with_time() {
        let base = RgbaImage::from_pixel(8, 1, image::Rgba([60, 60, 60, 255]));
        // Noise lit in its left half only.
        let mut noise = RgbaImage::from_pixel(8, 1, image::Rgba([0, 0, 0, 255]));
        for x in 0..4 {
            noise.put_pixel(x, 0, image::Rgba([255, 255, 255, 255]));
        }
        let speeds = [0.5; 4];
        let mut early = vec![0_u8; 8 * 4];
        let mut later = vec![0_u8; 8 * 4];
        compose_flicker(&base, &noise, speeds, 0.0, &mut early);
        compose_flicker(&base, &noise, speeds, 1.0, &mut later);
        // Half a texture later the lit half has moved to the other side.
        assert_eq!(&early[..4], &later[16..20]);
        assert_ne!(early, later);
    }
}
