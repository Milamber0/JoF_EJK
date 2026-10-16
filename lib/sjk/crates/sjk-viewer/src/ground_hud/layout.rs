//! Where the ground HUD's pieces lie on the floor: two numbers beside the
//! feet and the stance dot behind the heels, as quads in the player's yaw
//! frame. Pure maths, shared by the renderer and the tests.
//!
//! The *text plane* is the floor seen from above with text reading upright
//! from behind the player: `x` points to the player's right, `y` along the
//! player's facing. Text "up" is therefore away from the default camera, so
//! the numbers read like writing painted on the floor.

use super::readout::{self, Digits, Readings};
use crate::text::{TextFace, UiFont};

/// Sizes and positions in world units. One place to tune.
pub(crate) mod look {
    /// Height of a digit: modest decals, a little under a boot's length.
    pub(crate) const CAP_HEIGHT: f32 = 5.5;
    /// Gap from the player's axis to each number's inner edge; the feet and
    /// a planted stance stay clear of it.
    pub(crate) const NUMBER_INNER: f32 = 13.0;
    /// Where the digits' vertical centre sits along the facing: level with
    /// the feet, where the default camera (80 behind, 76 above) sees floor.
    pub(crate) const NUMBER_FORWARD: f32 = 1.0;
    /// Stance dot centre, this far BEHIND the origin (against the facing):
    /// just behind the heels, the closest the spec allows.
    pub(crate) const DOT_BEHIND: f32 = 7.0;
    /// Stance dot radius.
    pub(crate) const DOT_RADIUS: f32 = 1.9;
    /// Soft dark contact halo around the dot, for bright floors.
    pub(crate) const DOT_HALO: f32 = 1.6;
    /// Dark outline around each digit, so it reads on any floor.
    pub(crate) const OUTLINE: f32 = 0.45;
}

/// Most quads one frame draws: two numbers of up to three digits, and the dot.
pub(crate) const MAX_QUADS: usize = 7;

/// What a quad draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A digit sampled from the UI font atlas, with a dark outline.
    #[default]
    Glyph,
    /// The stance dot, an antialiased disc with a contact halo.
    Dot,
}

/// One ground quad.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Quad {
    /// Text-plane rectangle `[x0, y0, x1, y1]`: left, near (towards the
    /// camera), right, far edge.
    pub(crate) rect: [f32; 4],
    /// Atlas coordinates at the corners: `[u at x0, v at y0, u at x1, v at y1]`.
    pub(crate) uv: [f32; 4],
    /// Atlas rectangle sampling is clamped to, so the outline never reads a
    /// neighbouring glyph.
    pub(crate) bounds: [f32; 4],
    /// Glyph: outline radius in atlas units `[du, dv]`. Dot: `[radius, halo]`
    /// in world units.
    pub(crate) outline: [f32; 2],
    /// Display (sRGB) colour.
    pub(crate) color: [f32; 3],
    /// Glyph or dot.
    pub(crate) kind: Kind,
}

/// A frame's quads in fixed storage; building one never allocates.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Layout {
    quads: [Quad; MAX_QUADS],
    len: usize,
}

/// Which side of the feet a number sits on.
#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

impl Layout {
    /// Lay out this frame: health + shield left of the feet, Force right of
    /// them, and the stance dot behind the heels while the saber is held.
    pub(crate) fn build(font: &UiFont, readings: Readings) -> Self {
        let mut layout = Self::default();
        let combined = readout::combined(readings);
        let health = readout::health_color(readout::combined_fraction(readings));
        layout.number(font, Digits::new(combined), Side::Left, health);
        let force = readout::force_value(readings);
        let force_color = readout::force_color(readout::force_fraction(readings));
        layout.number(font, Digits::new(force), Side::Right, force_color);
        if let Some(style) = readings.stance {
            let reach = look::DOT_RADIUS + look::DOT_HALO;
            let y = -look::DOT_BEHIND;
            layout.push(Quad {
                rect: [-reach, y - reach, reach, y + reach],
                outline: [look::DOT_RADIUS, look::DOT_HALO],
                color: readout::stance_color(style),
                kind: Kind::Dot,
                ..Quad::default()
            });
        }
        layout
    }

    /// The quads to draw, in order.
    pub(crate) fn quads(&self) -> &[Quad] {
        &self.quads[..self.len]
    }

    fn push(&mut self, quad: Quad) {
        if self.len < MAX_QUADS {
            self.quads[self.len] = quad;
            self.len += 1;
        }
    }

    /// One number, its inner ink edge `NUMBER_INNER` from the axis so it never
    /// crowds the feet, whatever its digit count.
    fn number(&mut self, font: &UiFont, digits: Digits, side: Side, color: [f32; 3]) {
        let reference = font.glyph(TextFace::Semibold, b'0');
        if reference.height <= 0.0 {
            return;
        }
        let scale = look::CAP_HEIGHT / reference.height;
        // Ink extent at pen origin 0.
        let mut pen = 0.0;
        let (mut left, mut right) = (f32::MAX, f32::MIN);
        for &byte in digits.as_bytes() {
            let glyph = font.glyph(TextFace::Semibold, byte);
            left = left.min(pen + glyph.offset_x * scale);
            right = right.max(pen + (glyph.offset_x + glyph.width) * scale);
            pen += glyph.advance * scale;
        }
        let shift = match side {
            Side::Left => -look::NUMBER_INNER - right,
            Side::Right => look::NUMBER_INNER - left,
        };
        let top = look::NUMBER_FORWARD + look::CAP_HEIGHT * 0.5;
        let pad = look::OUTLINE * 1.6;
        let mut pen = shift;
        for &byte in digits.as_bytes() {
            let glyph = font.glyph(TextFace::Semibold, byte);
            let (width, height) = (glyph.width * scale, glyph.height * scale);
            let x0 = pen + glyph.offset_x * scale;
            let far = top - (glyph.offset_y - reference.offset_y) * scale;
            pen += glyph.advance * scale;
            if width <= 0.0 || height <= 0.0 {
                continue;
            }
            let [u0, v0, u1, v1] = glyph.uv;
            let (du, dv) = ((u1 - u0) / width, (v1 - v0) / height);
            let reach = [(pad + look::OUTLINE) * du, (pad + look::OUTLINE) * dv];
            self.push(Quad {
                rect: [x0 - pad, far - height - pad, x0 + width + pad, far + pad],
                uv: [u0 - pad * du, v1 + pad * dv, u1 + pad * du, v0 - pad * dv],
                bounds: [u0 - reach[0], v0 - reach[1], u1 + reach[0], v1 + reach[1]],
                outline: [look::OUTLINE * du, look::OUTLINE * dv],
                color,
                kind: Kind::Glyph,
            });
        }
    }
}
