//! Custom (RGB) blade colours.
//!
//! JA+/TaystJK let a player pick any blade tint (`saber_colors_t` index 6,
//! `SABER_RGB`).  Retail ships only the six coloured glow/core textures, and
//! TaystJK's own neutral `RGBglow1`/`RGBcore1` assets are not part of the
//! game data, so the engine generates a neutral pair itself — profiled from
//! the retail `blue_glow2` / `blue_line` images — and tints it per instance
//! in the shader.  The tint is applied like `CG_DoSaber` for `SABER_RGB`
//! (TaystJK `codemp/cgame/cg_players.c:6395-6460`): the glow takes the
//! colour, the hot core stays white with a coloured fringe.

use crate::saber::Color;
use crate::saber_skins::{MAX_SKINS, SkinColor};

/// Material slot of the engine-generated neutral pair, after the six retail
/// colours.
pub(crate) const RGB_MATERIAL: u32 = Color::ALL.len() as u32;
/// Material slot of the first loaded blade skin (`saber_skins.rs` `LoadedSkins` order),
/// after the neutral pair.
pub(crate) const SKIN_MATERIAL: u32 = RGB_MATERIAL + 1;
/// Number of saber material bind groups (six retail pairs, the neutral one and
/// one per blade-skin slot).
pub(crate) const MATERIAL_COUNT: usize = SKIN_MATERIAL as usize + MAX_SKINS;

/// Blade colour as the renderer needs it: a retail shader pair, or a tint
/// applied to the neutral pair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum BladeColor {
    /// One of the six retail colours, drawn with its own textures.
    Retail(Color),
    /// Any other colour, drawn with the neutral textures tinted by this RGB.
    Rgb([u8; 3]),
    /// A loaded blade skin, drawn with its own textures and animation (`saber_skins.rs`).
    Skin(SkinColor),
}

impl BladeColor {
    /// Pick the retail pair for an exact stock triplet, otherwise tint.
    pub(crate) fn from_rgb(rgb: [u8; 3]) -> Self {
        Color::ALL
            .into_iter()
            .find(|color| color.blade_rgb() == rgb)
            .map_or(Self::Rgb(rgb), Self::Retail)
    }

    /// Bind-group slot of this colour's glow/core pair.
    pub(crate) fn material(self) -> u32 {
        match self {
            Self::Retail(color) => color.index() as u32,
            Self::Rgb(_) => RGB_MATERIAL,
            Self::Skin(skin) => SKIN_MATERIAL + u32::from(skin.index),
        }
    }

    /// The colour an instance with `material` and per-instance `tint` was made from. A
    /// skin's instance carries its light colour as its tint ([`Self::tint`]), so the
    /// skin comes back with that light (and that as its trail), no flicker and no hue
    /// turning: enough for a contact's light.
    pub(crate) fn from_material(material: u32, tint: [f32; 3]) -> Self {
        if let Some(color) = Color::ALL.get(material as usize) {
            return Self::Retail(*color);
        }
        if let Some(index) = material
            .checked_sub(SKIN_MATERIAL)
            .filter(|index| (*index as usize) < MAX_SKINS)
        {
            return Self::Skin(SkinColor {
                index: index as u8,
                trail: tint,
                light: tint,
                flicker: Default::default(),
                hue: [0.0; 2],
                ghosts: None,
                persona: Default::default(),
                chroma: Default::default(),
                off: 0,
                room: 0.0,
                star: None,
                echo: None,
            });
        }
        Self::Rgb(tint.map(|c| (c * 255.).round().clamp(0., 255.) as u8))
    }

    /// Per-instance multiplier; retail textures already carry their colour. A skin's
    /// shading ignores it (`saber.wgsl`), so it carries the skin's light colour for
    /// [`Self::from_material`].
    pub(crate) fn tint(self) -> [f32; 3] {
        match self {
            Self::Retail(_) => [1.0; 3],
            Self::Rgb(rgb) => rgb.map(|channel| f32::from(channel) / 255.0),
            Self::Skin(skin) => skin.light_color(),
        }
    }

    /// Vertex RGB of the blur trail: the stock table for retail colours,
    /// the player's tint for RGB (`cg_players.c:7840-7855`), the skin's own for a skin.
    pub(crate) fn trail_rgb(self) -> [f32; 3] {
        match self {
            Self::Retail(color) => color.trail_rgb(),
            Self::Rgb(rgb) => rgb.map(|channel| f32::from(channel) / 255.0),
            Self::Skin(skin) => skin.trail_color(),
        }
    }
}

/// Side length of the generated glow image (`blue_glow2.jpg` is 128×128).
const GLOW_SIZE: u32 = 128;
/// Size of the generated core image (`blue_line.jpg` is 256×64).
const CORE_WIDTH: u32 = 64;
const CORE_HEIGHT: u32 = 256;

/// The radial profile of a generated glow image: a Gaussian falloff of `width` (in
/// half-widths) and `peak`, plus an optional wider `tail`, zero at the border.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GlowProfile {
    pub(crate) width: f32,
    pub(crate) peak: f32,
    #[serde(default = "GlowProfile::no_tail_width")]
    pub(crate) tail_width: f32,
    #[serde(default)]
    pub(crate) tail_peak: f32,
}

impl GlowProfile {
    /// The neutral pair's: σ = half the half-width, peak 0.55, matching the measured
    /// retail glow profile.
    pub(crate) const NEUTRAL: Self = Self {
        width: 0.5,
        peak: 0.55,
        tail_width: 1.0,
        tail_peak: 0.0,
    };

    fn no_tail_width() -> f32 {
        1.0
    }
}

/// The profile of a generated core image: the red channel's white-hot core and the
/// green channel's broader fringe, `exp(-(d/width)²)` across.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CoreProfile {
    pub(crate) width: f32,
    pub(crate) fringe_width: f32,
}

impl CoreProfile {
    /// The neutral pair's.
    pub(crate) const NEUTRAL: Self = Self {
        width: 0.41,
        fringe_width: 0.7,
    };
}

/// A grey glow image of `profile`, [`GLOW_SIZE`] square.
pub(crate) fn generated_glow(profile: GlowProfile) -> Vec<u8> {
    let half = GLOW_SIZE as f32 / 2.0;
    let mut pixels = Vec::with_capacity((GLOW_SIZE * GLOW_SIZE * 4) as usize);
    for y in 0..GLOW_SIZE {
        for x in 0..GLOW_SIZE {
            let dx = (x as f32 + 0.5 - half) / half;
            let dy = (y as f32 + 0.5 - half) / half;
            let distance = (dx * dx + dy * dy).sqrt();
            let mut falloff = (-(distance / profile.width).powi(2)).exp() * profile.peak;
            if profile.tail_peak > 0.0 {
                falloff += (-(distance / profile.tail_width).powi(2)).exp() * profile.tail_peak;
            }
            let edge = (1.0 - distance).clamp(0.0, 0.1) * 10.0;
            let value = (falloff * edge * 255.0).round() as u8;
            pixels.extend_from_slice(&[value, value, value, 255]);
        }
    }
    pixels
}

/// A core image of `profile`, [`CORE_WIDTH`]×[`CORE_HEIGHT`]: over the last 16 rows at
/// either end `d` becomes the distance from the end's centre, so the tips round off and
/// fade like `blue_line.jpg`.
pub(crate) fn generated_core(profile: CoreProfile) -> Vec<u8> {
    const END_ROWS: f32 = 16.0;
    let half = CORE_WIDTH as f32 / 2.0;
    let mut pixels = Vec::with_capacity((CORE_WIDTH * CORE_HEIGHT * 4) as usize);
    for y in 0..CORE_HEIGHT {
        let from_end = (y as f32 + 0.5).min(CORE_HEIGHT as f32 - y as f32 - 0.5);
        let dy = (1.0 - from_end / END_ROWS).max(0.0);
        for x in 0..CORE_WIDTH {
            let dx = (x as f32 + 0.5 - half) / half;
            let distance = (dx * dx + dy * dy).sqrt();
            let core = (-(distance / profile.width).powi(2)).exp();
            let fringe = (-(distance / profile.fringe_width).powi(2)).exp();
            let channel = |value: f32| (value * 255.0).round() as u8;
            pixels.extend_from_slice(&[channel(core), channel(fringe), 0, 255]);
        }
    }
    pixels
}

/// Neutral glow: [`GlowProfile::NEUTRAL`].
pub(crate) fn neutral_glow() -> Vec<u8> {
    generated_glow(GlowProfile::NEUTRAL)
}

/// Neutral core: [`CoreProfile::NEUTRAL`].
pub(crate) fn neutral_core() -> Vec<u8> {
    generated_core(CoreProfile::NEUTRAL)
}

/// The generated glow image's size.
pub(crate) const fn glow_size() -> [u32; 2] {
    [GLOW_SIZE, GLOW_SIZE]
}

/// The generated core image's size.
pub(crate) const fn core_size() -> [u32; 2] {
    [CORE_WIDTH, CORE_HEIGHT]
}

/// Upload the neutral pair as the seventh saber material, as stored display values like
/// the retail pairs.
pub(crate) fn create_material(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    layout: &wgpu::BindGroupLayout,
    samplers: &crate::saber::Samplers,
) -> wgpu::BindGroup {
    let glow = image::RgbaImage::from_raw(GLOW_SIZE, GLOW_SIZE, neutral_glow())
        .expect("neutral glow has GLOW_SIZE² RGBA texels");
    let core = image::RgbaImage::from_raw(CORE_WIDTH, CORE_HEIGHT, neutral_core())
        .expect("neutral core has CORE_WIDTH×CORE_HEIGHT RGBA texels");
    crate::saber::material(device, queue, layout, samplers, &glow, &core)
}
