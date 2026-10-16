//! Codemp-compatible saber attachment and blade presentation.
//!
//! `codemp/cgame/cg_players.c::CG_AddSaberBlade` evaluates the player model's
//! first Ghoul2 bolt (`*r_hand`), attaches the saber weapon model there with
//! the raw renderer bolt, then reads the saber model's `*blade1` bolt through
//! `G2API_GetBoltMatrix` and takes its origin and **negative Y column** as
//! blade forward. That API applies the multiplayer 90° basis offset (see
//! [`crate::bolt`]); in raw bolt terms the blade runs along the negated X
//! column, i.e. from the tag triangle's first vertex through its origin
//! vertex. Keeping those choices here prevents renderer call sites from
//! reintroducing the historical sideways blade.

use super::PlayerPreview;
use crate::saber_rgb::{BladeColor, MATERIAL_COUNT};
use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Quat, Vec3};
use sjk_shader::ShaderCatalog;
use sjk_vfs::VirtualFileSystem;
use std::error::Error;
use std::ops::Range;

pub(crate) use crate::saber_hilts::{HiltCatalog, load_hilts};

pub(crate) const HAND_BOLT: &str = "*r_hand";
pub(crate) const LEFT_HAND_BOLT: &str = "*l_hand";

pub(crate) const MAX_BLADE_INSTANCES: usize = 1_024 * 2 * 8 * 2;

/// Model-space transform encoded by the player's right-hand Ghoul2 surface bolt.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Attachment {
    pub(crate) grip: [f32; 3],
    pub(crate) rotation: [f32; 4],
}

/// Saber-model-space blade origin and codemp forward direction.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BladeSocket {
    pub(crate) origin: [f32; 3],
    pub(crate) direction: [f32; 3],
}

/// Fully transformed blade submitted to the saber renderer.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Blade {
    pub(crate) base: [f32; 3],
    pub(crate) direction: [f32; 3],
    pub(crate) length: f32,
    pub(crate) radius: f32,
}

/// Per-entity blade extension state mirroring
/// `codemp/game/bg_saberLoad.c::BG_SI_SetLengthGradual`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Extension {
    length: f32,
    desired: f32,
    maximum: f32,
    extend_debounce: i64,
    active: bool,
    initialized: bool,
}

impl Extension {
    /// Advance one cgame presentation frame and return the visible length.
    pub(crate) fn update(&mut self, active: bool, maximum: f32, time_millis: i64) -> f32 {
        if !self.initialized {
            self.length = if active { maximum } else { 0.0 };
            self.desired = self.length;
            self.maximum = maximum;
            self.extend_debounce = time_millis;
            self.active = active;
            self.initialized = true;
            return self.length;
        }
        self.maximum = maximum;
        if active != self.active {
            self.active = active;
            self.desired = if active { maximum } else { 0.0 };
        }
        if self.length == self.desired {
            return self.length;
        }
        if self.length == self.maximum || self.length == 0.0 {
            self.extend_debounce = time_millis;
            if self.length == 0.0 {
                self.length += 1.0;
            } else {
                self.length -= 1.0;
            }
        }
        let amount = ((time_millis - self.extend_debounce) as f32 * 0.01).max(0.2);
        if self.length < self.desired {
            self.length = (self.length + amount).min(self.desired).min(self.maximum);
        } else {
            self.length = (self.length - amount).max(self.desired).max(0.0);
        }
        self.length
    }
}

/// An instance's afterimage byte for the blade itself (fully bright).
const FULL_FADE: u32 = 0xff00_0000;
/// The light an instance carries until the grid is sampled: grey.
const GREY_LIGHT: u32 = 0x00ff_ffff;

/// One glow/core GPU instance. A glow carries its hilt sprite radius in colour alpha; a
/// core carries zero there.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct Instance {
    base: [f32; 3],
    length: f32,
    direction: [f32; 3],
    /// Drawn radius, after `CG_DoSaber`'s flicker and extension growth.
    radius: f32,
    color: [f32; 4],
    material: u32,
    /// Presentation seconds (wrapped, [`Self::with_animation`]) and a per-blade seed in
    /// [0, 1), for the skins' animation; unused by the retail and RGB pairs.
    animation: [f32; 2],
    /// Who wears a skinned blade ([`crate::saber_persona`]): lane 0 the light where it
    /// is (three bytes) and the afterimage's brightness (top byte, 255 for the blade);
    /// lanes 1 to 3 the name's letters and the team. Unused by the retail and RGB pairs.
    persona: [u32; 4],
    /// A chroma skin's turn to its wearer's colour, in turns
    /// ([`crate::saber_skins::Chroma`]); 0 for every other blade.
    chroma: f32,
    /// The sections of a skin its wearer switched off
    /// ([`crate::saber_skins::SkinColor::off`]); 0 for every other blade.
    off: u32,
    contact: u32,
    no_light: u32,
    /// The blade's configured radius, for contacts; not a GPU attribute.
    nominal_radius: f32,
    maximum_length: f32,
    /// How far (units) a skin's glow quad reaches past its glow, or a glint's rays' length;
    /// for the bounds, not a GPU attribute.
    room: f32,
    /// A glint's place ([`Instance::stars`]): how far below the tip (units) and how far
    /// out it circles; for the bounds, not a GPU attribute.
    glint: [f32; 2],
}

/// Colour alpha marking a skin's first glint ([`Instance::stars`]): neither a glow (a hilt
/// sprite's radius) nor a core (zero). Glint `n` of an orbit is `STAR - n`.
const STAR: f32 = -1.0;

/// `CG_DoSaber`'s per-frame random draws (`cg_players.c:5359-5469`): `crandom()` for the
/// glow and core radii and `Q_flrand(0, 1)` for the hilt sprite (tr_surface.cpp:489).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Flicker {
    /// Glow radius draw in [-1, 1].
    pub(crate) glow: f32,
    /// Core radius draw in [-1, 1].
    pub(crate) core: f32,
    /// Hilt sprite draw in [0, 1].
    pub(crate) hilt: f32,
}

impl Flicker {
    /// Deterministic draws for one blade in one presentation millisecond; no RNG state.
    pub(crate) fn sample(key: u64, time_millis: i64) -> Self {
        let mut state = key.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ (time_millis as u64);
        let mut next = || {
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
        };
        Self {
            glow: next() * 2.0 - 1.0,
            core: next() * 2.0 - 1.0,
            hilt: next(),
        }
    }
}

impl Instance {
    const ATTRIBUTES: [wgpu::VertexAttribute; 10] = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Float32, 2 => Float32x3, 3 => Float32, 4 => Float32x4, 5 => Uint32,
        6 => Float32x2, 7 => Uint32x4, 8 => Float32, 9 => Uint32];

    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBUTES,
        }
    }

    /// A fully extended blade without flicker; see [`Self::pair_flickering`].
    pub(crate) fn pair(blade: Blade, color: BladeColor) -> [Self; 2] {
        Self::pair_flickering(blade, color, blade.length, Flicker::default())
    }

    /// Match `CG_DoSaber` (`cg_players.c:5359-5469`): the glow radius is
    /// `(R - 0.075R + crandom·0.075R)·radiusmult`, the core's `(R/3 + crandom·0.075R)·radiusmult`,
    /// with `radiusmult = 1 + 2/length` while the blade is shorter than `max_length`; the
    /// hilt sprite is `5.5 + flrand·0.25` (tr_surface.cpp:489). Retail textures carry their
    /// own colour (vertex colour 0xff); the neutral pair is tinted by the instance colour.
    pub(crate) fn pair_flickering(
        blade: Blade,
        color: BladeColor,
        max_length: f32,
        flicker: Flicker,
    ) -> [Self; 2] {
        let [red, green, blue] = color.tint();
        let range = blade.radius * 0.075;
        let grow = if blade.length < max_length {
            1.0 + 2.0 / blade.length.max(0.5)
        } else {
            1.0
        };
        let persona = match color {
            BladeColor::Skin(skin) => {
                let [a, b, c] = skin.persona.lanes();
                [FULL_FADE | GREY_LIGHT, a, b, c]
            }
            _ => [0; 4],
        };
        let (chroma, off, room) = match color {
            BladeColor::Skin(skin) => (skin.chroma.turn, skin.off, skin.room),
            _ => (0.0, 0, 0.0),
        };
        let make = |radius: f32, hilt| Self {
            base: blade.base,
            length: blade.length,
            direction: blade.direction,
            radius: radius * grow,
            color: [red, green, blue, hilt],
            material: color.material(),
            animation: [0.0; 2],
            persona,
            chroma,
            off,
            contact: 0,
            no_light: 0,
            nominal_radius: blade.radius,
            maximum_length: max_length,
            room,
            glint: [0.0; 2],
        };
        [
            make(
                blade.radius - range + flicker.glow * range,
                5.5 + flicker.hilt * 0.25,
            ),
            make(blade.radius / 3.0 + flicker.core * range, 0.0),
        ]
    }

    /// An afterimage of a skinned blade ([`crate::saber_ghosts`]): its glow alone at
    /// `blade`'s pose, `fade` as bright.
    pub(crate) fn ghost(blade: Blade, color: BladeColor, fade: f32) -> Self {
        let [glow, _core] = Self::pair(blade, color);
        let byte = (fade.clamp(0.0, 1.0) * 255.0).round() as u32;
        Self {
            persona: [
                (glow.persona[0] & 0x00ff_ffff) | (byte << 24),
                glow.persona[1],
                glow.persona[2],
                glow.persona[3],
            ],
            ..glow
        }
    }

    /// A skin's glints, when its skin has them switched on: the one at `blade`'s tip, or
    /// each circling the blade below it (`saber.wgsl` moves them round); a quad facing the
    /// camera each, as wide as the rays are long, drawn by the glow pipelines.
    pub(crate) fn stars(blade: Blade, color: BladeColor) -> impl Iterator<Item = Self> {
        let glint = match color {
            BladeColor::Skin(skin) => skin.star,
            _ => None,
        };
        glint
            .map(|glint| (glint, Self::pair(blade, color)[0]))
            .into_iter()
            .flat_map(|(glint, glow)| {
                (0..glint.count).map(move |n| Self {
                    color: [
                        glow.color[0],
                        glow.color[1],
                        glow.color[2],
                        STAR - f32::from(n),
                    ],
                    room: glint.length,
                    glint: [glint.at, glint.reach],
                    ..glow
                })
            })
    }

    /// A skin's echo of `blade` at `seconds`, when its skin has one switched on: the glow
    /// alone, swaying beside the blade on two slow waves (the tip more than the hilt) and
    /// leaning a little, `seed` putting each blade's sway at its own place.
    pub(crate) fn echo(blade: Blade, color: BladeColor, seconds: f64, seed: u32) -> Option<Self> {
        let BladeColor::Skin(skin) = color else {
            return None;
        };
        let echo = skin.echo?;
        let direction = Vec3::from_array(blade.direction).normalize_or(Vec3::Z);
        let across = direction.cross(Vec3::Z);
        let across = if across.length_squared() > 1e-4 {
            across.normalize()
        } else {
            direction.cross(Vec3::X).normalize()
        };
        let other = direction.cross(across);
        let phase = (seconds * f64::from(echo.rate)).rem_euclid(1_024.0) as f32
            * std::f32::consts::TAU
            + (seed % 1_000) as f32 * 0.618;
        let sway = across * phase.sin() + other * (0.6 * (phase * 0.73 + 1.3).sin());
        let tip = Vec3::from_array(blade.base) + direction * blade.length;
        let base = Vec3::from_array(blade.base) + sway * (0.25 * echo.sway);
        let tip = tip
            + sway * echo.sway
            + across * (echo.lean * blade.length * (phase * 0.5 + 0.4).sin());
        let swayed = Blade {
            base: base.to_array(),
            direction: (tip - base).normalize_or(direction).to_array(),
            length: (tip - base).length(),
            radius: blade.radius,
        };
        Some(Self::ghost(swayed, color, echo.fade))
    }

    /// Whether this is a glint ([`Self::stars`]).
    #[cfg(test)]
    pub(crate) fn is_star(&self) -> bool {
        self.color[3] <= STAR
    }

    /// The persona lanes, for the tests.
    #[cfg(test)]
    pub(crate) fn persona_lanes(&self) -> [u32; 4] {
        self.persona
    }

    /// How bright an afterimage is (1 for a blade itself).
    #[cfg(test)]
    pub(crate) fn fade(&self) -> f32 {
        (self.persona[0] >> 24) as f32 / 255.0
    }

    /// The loaded skin's number this instance wears, if it wears one.
    pub(crate) fn skin_index(&self) -> Option<u8> {
        self.material
            .checked_sub(crate::saber_rgb::SKIN_MATERIAL)
            .filter(|index| (*index as usize) < crate::saber_skins::MAX_SKINS)
            .map(|index| index as u8)
    }

    /// Where the blade starts (its hilt end).
    pub(crate) fn base(&self) -> [f32; 3] {
        self.base
    }

    /// The light where the blade is (0 to 1 a channel), for a skin taking its colour.
    pub(crate) fn set_surroundings(&mut self, rgb: [f32; 3]) {
        self.persona[0] = (self.persona[0] & 0xff00_0000) | crate::saber_persona::pack_light(rgb);
    }

    /// Period the animation time wraps at, in seconds, keeping it precise in an `f32`
    /// (`saber.wgsl` sees a jump once in this long).
    pub(crate) const ANIMATION_PERIOD: f64 = 1_024.0;

    /// Stamp the skins' animation: presentation time `seconds` and a stable per-blade
    /// `seed` (the same for a blade's glow and core, and from frame to frame).
    pub(crate) fn with_animation(mut self, seconds: f64, seed: u32) -> Self {
        let mut hash = seed.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
        hash ^= hash >> 15;
        hash = hash.wrapping_mul(0x2c1b_3c6d);
        hash ^= hash >> 12;
        self.animation = [
            seconds.rem_euclid(Self::ANIMATION_PERIOD) as f32,
            (hash >> 8) as f32 / (1u32 << 24) as f32,
        ];
        self
    }

    /// Tag render instances with a stable presentation key; GPU attributes are unchanged.
    /// Lane 2 is reserved for a thrown primary, so catching cannot bridge a cut.
    pub(crate) fn with_contact(
        mut self,
        entity: u64,
        lane: usize,
        blade: usize,
        enabled: bool,
        no_light: bool,
    ) -> Self {
        if entity > 0 && entity <= 1024 && lane < 3 && blade < 8 {
            self.contact = ((entity - 1) as usize * 24 + lane * 8 + blade + 1) as u32;
            self.no_light = u32::from(no_light) | (u32::from(!enabled) << 1);
        }
        self
    }

    /// One contact source per glow/core pair, excluding menu blades.
    pub(crate) fn contact(self) -> Option<(usize, Blade, BladeColor, bool)> {
        if self.contact == 0 || self.color[3] <= 0.0 {
            return None;
        }
        let color =
            BladeColor::from_material(self.material, [self.color[0], self.color[1], self.color[2]]);
        Some((
            self.contact as usize - 1,
            Blade {
                base: self.base,
                length: self.length,
                direction: self.direction,
                radius: self.nominal_radius,
            },
            color,
            self.no_light & 1 != 0,
        ))
    }

    /// Wall-mark suppression does not suppress the stock blade cutoff.
    pub(crate) fn wall_marks(self) -> bool {
        self.no_light & 2 == 0
    }

    /// `CG_DoSaber` omits blades shorter than half a unit.
    pub(crate) fn visible(&self) -> bool {
        self.length >= 0.5
    }

    /// Shorten this frame's blade, retaining its extension state for the next frame.
    pub(crate) fn clip_length(&mut self, length: f32) {
        let length = length.max(0.0);
        let growth = |length: f32| {
            if length < self.maximum_length {
                1.0 + 2.0 / length.max(0.5)
            } else {
                1.0
            }
        };
        self.radius *= growth(length) / growth(self.length);
        self.length = length;
    }

    pub(crate) const fn material(self) -> usize {
        self.material as usize
    }

    /// A world segment and radius holding everything `saber.wgsl` draws for this instance:
    /// the glow capsule (grown chain and hilt sprite) or the core line from behind the hilt.
    pub(crate) fn extent(self) -> ([f32; 3], [f32; 3], f32) {
        let direction = Vec3::from_array(self.direction);
        let base = Vec3::from_array(self.base);
        let radius = self.radius.max(0.01);
        // A skin's glow reaches further, for its flares (its `corona.reach`, at most
        // `MAX_GLOW_REACH`).
        let widen = if self.material >= crate::saber_rgb::SKIN_MATERIAL {
            crate::saber_skins::MAX_GLOW_REACH
        } else {
            1.0
        };
        if self.color[3] <= STAR {
            // A quad as wide as the rays are long, round the tip or anywhere on the
            // glints' circle below it.
            let [at, out] = self.glint;
            let centre = base + direction * (self.length - at.min(self.length));
            let reach = self.room * std::f32::consts::SQRT_2 + out;
            return (centre.to_array(), centre.to_array(), reach);
        }
        let reach = if self.color[3] > 0.0 {
            // saber.wgsl `chain_radius` at the hilt, or the hilt sprite, and a skin's room
            // for its wisps and haze; quad corners reach √2.
            (((radius + 0.017 * self.length / (0.65 * radius)) * widen).max(self.color[3])
                + self.room)
                * std::f32::consts::SQRT_2
        } else {
            radius
        };
        (
            (base - direction * reach.max(1.0)).to_array(),
            (base + direction * (self.length + reach)).to_array(),
            reach,
        )
    }
}

/// A skinned blade's instances beyond its glow and core: its glints and its echo, each
/// when its skin has it switched on, at `seconds` with the blade's `seed`
/// (their animation stamped as the blade's). Never contact sources.
pub(crate) fn skin_extras(
    blade: Blade,
    color: BladeColor,
    seconds: f64,
    seed: u32,
) -> impl Iterator<Item = Instance> {
    Instance::stars(blade, color)
        .chain(Instance::echo(blade, color, seconds, seed))
        .map(move |instance| instance.with_animation(seconds, seed))
}

/// Group instances by shader pair and return their draw ranges.
pub(crate) fn material_ranges(instances: &mut [Instance]) -> [Range<u32>; MATERIAL_COUNT] {
    instances.sort_unstable_by_key(|instance| instance.material());
    std::array::from_fn(|material| {
        let start = instances.partition_point(|instance| instance.material() < material);
        let end = instances.partition_point(|instance| instance.material() <= material);
        u32::try_from(start).unwrap_or(u32::MAX)..u32::try_from(end).unwrap_or(u32::MAX)
    })
}

/// The six fixed retail saber shader pairs selected by codemp client color.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Color {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
}

impl Color {
    pub(crate) const ALL: [Self; 6] = [
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Blue,
        Self::Purple,
    ];

    /// Blade tint reported by the client adapter for this stock colour
    /// (the vertex colour `CG_DoSaber` hands the retail shader pair).
    pub(crate) const fn blade_rgb(self) -> [u8; 3] {
        match self {
            Self::Red => [255, 51, 51],
            Self::Orange => [255, 128, 26],
            Self::Yellow => [255, 255, 51],
            Self::Green => [51, 255, 51],
            Self::Blue => [51, 102, 255],
            Self::Purple => [230, 51, 255],
        }
    }

    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Red => 0,
            Self::Orange => 1,
            Self::Yellow => 2,
            Self::Green => 3,
            Self::Blue => 4,
            Self::Purple => 5,
        }
    }

    pub(crate) const fn shader_stem(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Orange => "orange",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Purple => "purple",
        }
    }

    /// Vertex RGB chosen by `CG_AddSaberBlade` for the blur primitive
    /// (`codemp/cgame/cg_players.c:6371-6394`).
    pub(crate) const fn trail_rgb(self) -> [f32; 3] {
        match self {
            Self::Red => [1.0, 0.0, 0.0],
            Self::Orange => [1.0, 64.0 / 255.0, 0.0],
            Self::Yellow => [1.0, 1.0, 0.0],
            Self::Green => [0.0, 1.0, 0.0],
            Self::Blue => [0.0, 64.0 / 255.0, 1.0],
            Self::Purple => [220.0 / 255.0, 0.0, 1.0],
        }
    }
}

/// Samplers of a saber material: the glow and its integral are sampled at level 0 (the
/// capsule integrates stock's sprite chain itself), the core line with the owner's stock
/// filtering, rd-vulkan's `GL_LINEAR_MIPMAP_LINEAR` with its default
/// `r_ext_max_anisotropy 2` (`rd-vulkan/tr_init.cpp:930`, `vk_image.cpp:320-352`).
pub(crate) struct Samplers {
    glow: wgpu::Sampler,
    core: wgpu::Sampler,
}

impl Samplers {
    /// Stock core anisotropy (`r_ext_max_anisotropy` default).
    const CORE_ANISOTROPY: u16 = 2;

    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let clamped = |label, mipmaps, anisotropy_clamp| wgpu::SamplerDescriptor {
            label: Some(label),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: mipmaps,
            anisotropy_clamp,
            ..Default::default()
        };
        Self {
            glow: device.create_sampler(&clamped(
                "SJK saber glow sampler",
                wgpu::MipmapFilterMode::Nearest,
                1,
            )),
            core: device.create_sampler(&clamped(
                "SJK saber core sampler",
                wgpu::MipmapFilterMode::Linear,
                Self::CORE_ANISOTROPY,
            )),
        }
    }
}

/// Load the six real retail glow/core shader pairs used by `CG_DoSaber`,
/// followed by the engine-generated neutral pair for custom RGB blades, which also
/// holds every blade-skin slot until a skin is loaded there
/// (`saber_gpu::Runtime::upload_skins`).
pub(crate) fn create_materials(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    layout: &wgpu::BindGroupLayout,
    samplers: &Samplers,
) -> Result<Vec<wgpu::BindGroup>, Box<dyn Error>> {
    let neutral = crate::saber_rgb::create_material(device, queue, layout, samplers);
    let mut materials = Vec::with_capacity(MATERIAL_COUNT);
    for color in Color::ALL {
        let stem = color.shader_stem();
        let load = |kind: &str| {
            crate::shader_image::load_shader_image(
                vfs,
                shaders,
                &format!("gfx/effects/sabers/{stem}_{kind}"),
            )
        };
        materials.push(material(
            device,
            queue,
            layout,
            samplers,
            &load("glow")?,
            &load("line")?,
        ));
    }
    materials.resize(MATERIAL_COUNT, neutral);
    Ok(materials)
}

/// Bind one glow/core pair: the glow as stored, its column integral, and the core with
/// rd-vulkan's box mip chain (stored display values throughout).
pub(crate) fn material(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    layout: &wgpu::BindGroupLayout,
    samplers: &Samplers,
    glow: &image::RgbaImage,
    core: &image::RgbaImage,
) -> wgpu::BindGroup {
    let integral = crate::saber_gpu::glow_integral::upload(device, queue, glow);
    let glow = crate::gpu_texture::upload_display_image(device, queue, "SJK saber glow", glow);
    let core =
        crate::gpu_texture::upload_display_image_mipmapped(device, queue, "SJK saber core", core);
    let view = |binding, view| wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    };
    let sampler = |binding, sampler| wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::Sampler(sampler),
    };
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("SJK saber shader pair"),
        layout,
        entries: &[
            view(0, &glow),
            view(1, &core),
            sampler(2, &samplers.glow),
            view(3, &integral),
            sampler(4, &samplers.core),
        ],
    })
}

/// Resolve the right and left hand from one already-evaluated pose.
pub(crate) fn attachments_from_matrices(
    preview: &PlayerPreview,
    matrices: &[[[f32; 4]; 3]],
) -> [Option<Attachment>; 2] {
    std::array::from_fn(|index| {
        attachment_from_matrices(preview, matrices, [HAND_BOLT, LEFT_HAND_BOLT][index])
    })
}

/// Build the right-hand attachment from an already evaluated actor pose.
pub(crate) fn attachment_from_matrices(
    preview: &PlayerPreview,
    matrices: &[[[f32; 4]; 3]],
    bolt_name: &str,
) -> Option<Attachment> {
    attachment_from_bolt(preview.bolt(bolt_name, matrices)?)
}

/// Convert a Ghoul2 3x4 bolt matrix without changing its handedness.
pub(crate) fn attachment_from_bolt(matrix: [[f32; 4]; 3]) -> Option<Attachment> {
    let origin = Vec3::new(matrix[0][3], matrix[1][3], matrix[2][3]);
    let x = Vec3::new(matrix[0][0], matrix[1][0], matrix[2][0]).normalize_or_zero();
    let raw_y = Vec3::new(matrix[0][1], matrix[1][1], matrix[2][1]);
    let y = (raw_y - x * raw_y.dot(x)).normalize_or_zero();
    let z = x.cross(y).normalize_or_zero();
    let encoded_z = Vec3::new(matrix[0][2], matrix[1][2], matrix[2][2]).normalize_or_zero();
    if [x, y, z, encoded_z]
        .into_iter()
        .any(|axis| axis.length_squared() < 0.5)
        || z.dot(encoded_z) < 0.99
    {
        return None;
    }
    Some(Attachment {
        grip: origin.to_array(),
        rotation: Quat::from_mat3(&Mat3::from_cols(x, y, z)).to_array(),
    })
}

/// Extract the saber socket exactly as multiplayer `CG_AddSaberBlade` does:
/// `*blade1` through `G2API_GetBoltMatrix`, origin from `ORIGIN`, forward
/// from `NEGATIVE_Y`. `raw` is the renderer bolt from
/// [`sjk_model::Glm::surface_bolt_matrix`].
pub(crate) fn blade_socket_from_bolt(raw: crate::bolt::BoltMatrix) -> Option<BladeSocket> {
    let game = crate::bolt::game_facing(raw);
    let direction = -Vec3::from_array(crate::bolt::column(&game, 1));
    (direction.length_squared() > 0.5).then_some(BladeSocket {
        origin: crate::bolt::column(&game, 3),
        direction: direction.normalize().to_array(),
    })
}

/// Apply the actor and hand-bolt transforms once, in parent-to-child order.
pub(crate) fn world_attachment(
    actor_origin: Vec3,
    actor_rotation: Quat,
    attachment: Attachment,
) -> (Vec3, Quat) {
    (
        actor_origin + actor_rotation * Vec3::from_array(attachment.grip),
        actor_rotation * Quat::from_array(attachment.rotation),
    )
}

/// Transform a saber-local blade socket into world space.
pub(crate) fn world_blade(
    grip: Vec3,
    weapon_rotation: Quat,
    socket: BladeSocket,
    length: f32,
    radius: f32,
) -> Blade {
    Blade {
        base: (grip + weapon_rotation * Vec3::from_array(socket.origin)).to_array(),
        direction: (weapon_rotation * Vec3::from_array(socket.direction))
            .normalize_or_zero()
            .to_array(),
        length,
        radius,
    }
}

#[cfg(test)]
mod cutoff_tests {
    use super::*;

    #[test]
    fn the_animation_attribute_sits_where_the_instance_keeps_it() {
        let offset = |index: usize| Instance::ATTRIBUTES[index].offset as usize;
        assert_eq!(offset(5), std::mem::offset_of!(Instance, material));
        assert_eq!(offset(6), std::mem::offset_of!(Instance, animation));
        assert_eq!(offset(8), std::mem::offset_of!(Instance, chroma));
        assert_eq!(
            Instance::ATTRIBUTES[6].format,
            wgpu::VertexFormat::Float32x2
        );
        // saber.wgsl's slots and skin array match the Rust ones.
        let shader = include_str!("saber.wgsl");
        assert!(shader.contains(&format!(
            "const NEUTRAL_MATERIAL: u32 = {}u;",
            crate::saber_rgb::RGB_MATERIAL
        )));
        assert!(shader.contains(&format!(
            "const SKIN_MATERIAL: u32 = {}u;",
            crate::saber_rgb::SKIN_MATERIAL
        )));
        assert!(shader.contains(&format!(
            "const MAX_SKINS: u32 = {}u;",
            crate::saber_skins::MAX_SKINS
        )));
        assert!(shader.contains("var<storage, read> skins: array<Skin, MAX_SKINS>;"));
        // `Skin` is 95 vec4s, as `SkinUniform`, and the array fits a storage binding.
        assert_eq!(
            std::mem::size_of::<crate::saber_skins::SkinUniform>(),
            95 * 16
        );
        const { assert!(95 * 16 * crate::saber_skins::MAX_SKINS <= 128 * 1024 * 1024) };
        let skin = &shader[shader.find("struct Skin {").unwrap()..];
        let skin = &skin[..skin.find('}').unwrap()];
        assert_eq!(skin.matches(": vec4<f32>,").count(), 95);
        // In `SkinUniform`'s order (its lanes are pinned by `saber_skins`' tests).
        let lanes: Vec<&str> = skin
            .lines()
            .filter_map(|line| line.trim().strip_suffix(": vec4<f32>,"))
            .collect();
        assert_eq!(
            lanes,
            [
                "core_white",
                "core_fringe_cool",
                "core_fringe_hot",
                "core_fringe",
                "core_breathe",
                "rim_cool",
                "rim_hot",
                "inner",
                "inner_mix",
                "brightness",
                "swell",
                "grain_coarse",
                "grain_fine",
                "grain_mix",
                "flare_rate",
                "flare_shape",
                "flare_size",
                "shimmer_a",
                "shimmer_b",
                "tongue_a",
                "tongue_b",
                "arc_color",
                "arc_shape",
                "arc_strike",
                "arc_place",
                "arc_motion",
                "mote_color",
                "mote_field",
                "mote_motion",
                "mote_band",
                "hue",
                "sputter_edge",
                "sputter_cut",
                "glitch_a",
                "glitch_b",
                "scan_color",
                "scan_lines",
                "scan_edge",
                "scan_jitter",
                "pulse_a",
                "pulse_b",
                "ember_color",
                "ember_field",
                "ember_shape",
                "vein_color",
                "vein_shape",
                "team_red",
                "team_blue",
                "team_none",
                "ambient",
                "glyph_color",
                "glyph_shape",
                "star_color",
                "star_shape",
                "star_motion",
                "star_orbit",
                "hole_color",
                "hole_shape",
                "wisp_color",
                "wisp_shape",
                "wisp_field",
                "haze",
                "band_color",
                "band_shape",
                "band_mix",
                "segment_a",
                "segment_b",
                "film",
                "film_b",
                "blob_color",
                "blob_shape",
                "blob_more",
                "front_color",
                "front_shape",
                "front_edge",
                "sketch_color",
                "sketch_shape",
                "hour_color",
                "hour_shape",
                "serpent_color",
                "serpent_shape",
                "strand_shape",
                "strand_hue",
                "fly_color",
                "fly_shape",
                "fly_roam",
                "wing_color",
                "wing_shape",
                "copy_shape",
                "copy_more",
                "ink_color",
                "ink_shape",
                "ink_more",
                "chrome_color",
                "chrome_shape",
            ]
        );
        assert!(shader.contains(&format!(
            "const MAX_ARCS: i32 = {};",
            crate::blade_skin_file::MAX_ARCS
        )));
        assert!(shader.contains("@location(6) animation: vec2<f32>"));
        // The persona lanes (`saber_persona.rs`), after the animation.
        assert_eq!(offset(7), std::mem::offset_of!(Instance, persona));
        assert_eq!(Instance::ATTRIBUTES[7].format, wgpu::VertexFormat::Uint32x4);
        assert!(shader.contains("@location(7) persona: vec4<u32>"));
        // The sections a wearer switched off, after the chroma.
        assert_eq!(offset(9), std::mem::offset_of!(Instance, off));
        assert_eq!(Instance::ATTRIBUTES[9].format, wgpu::VertexFormat::Uint32);
        assert!(shader.contains("@location(9) off: u32"));
    }

    #[test]
    fn the_blade_shader_validates() {
        use wgpu::naga;
        let source = include_str!("saber.wgsl");
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source)));
    }

    #[test]
    fn animation_time_wraps_and_seeds_are_stable_and_spread() {
        let blade = Blade {
            base: [0.; 3],
            direction: [1., 0., 0.],
            length: 40.,
            radius: 3.,
        };
        let pair = Instance::pair(blade, BladeColor::from_rgb([0, 0, 255]));
        let stamped = pair.map(|i| i.with_animation(1_030.5, 7));
        assert_eq!(stamped[0].animation, stamped[1].animation);
        assert!((stamped[0].animation[0] - 6.5).abs() < 1e-4);
        assert_eq!(
            pair[0].with_animation(3.0, 7).animation[1],
            stamped[0].animation[1]
        );
        let seeds: Vec<f32> = (0..64)
            .map(|seed| pair[0].with_animation(0.0, seed).animation[1])
            .collect();
        assert!(seeds.iter().all(|seed| (0.0..1.0).contains(seed)));
        assert!(seeds.iter().any(|&seed| seed < 0.25) && seeds.iter().any(|&seed| seed > 0.75));
        assert_ne!(seeds[0], seeds[1]);
        // Unstamped (retail) instances carry zeros.
        assert_eq!(pair[0].animation, [0.0; 2]);
    }

    #[test]
    fn cutoff_matches_stock_render_pair_at_command_steps() {
        for step in [8, 7, 4, 3] {
            let mut extension = Extension::default();
            for time in (0..100).step_by(step) {
                let blade = Blade {
                    base: [0.; 3],
                    direction: [1., 0., 0.],
                    length: extension.update(true, 40., time),
                    radius: 3.,
                };
                let mut pair = Instance::pair(blade, BladeColor::from_rgb([0, 0, 255]));
                for instance in &mut pair {
                    instance.clip_length(12.);
                    assert_eq!(instance.length, 12.);
                }
                let expected = Instance::pair_flickering(
                    Blade {
                        length: 12.,
                        ..blade
                    },
                    BladeColor::from_rgb([0, 0, 255]),
                    40.,
                    Flicker::default(),
                );
                for (actual, expected) in pair.iter().zip(expected) {
                    assert!((actual.radius - expected.radius).abs() < 0.00001);
                }
                assert_eq!(extension.update(true, 40., time), 40.);
            }
        }
    }

    #[test]
    fn no_wall_marks_still_registers_a_cutoff_source() {
        let blade = Blade {
            base: [0.; 3],
            direction: [1., 0., 0.],
            length: 40.,
            radius: 3.,
        };
        let pair = Instance::pair(blade, BladeColor::from_rgb([0, 0, 255]))
            .map(|instance| instance.with_contact(1, 0, 0, false, false));
        assert!(pair[0].contact().is_some());
        assert!(!pair[0].wall_marks());
        assert!(!pair[0].contact().unwrap().3);
        assert!(pair[1].contact().is_none());
    }

    #[test]
    fn cutoff_keeps_extension_growth_and_the_one_unit_trace_margin() {
        let color = BladeColor::from_rgb([0, 0, 255]);
        for (initial, hit) in [(20., 10.), (40., 40.5), (40., 0.)] {
            let blade = Blade {
                base: [0.; 3],
                direction: [1., 0., 0.],
                length: initial,
                radius: 3.,
            };
            let mut pair = Instance::pair_flickering(blade, color, 40., Flicker::default());
            let expected = Instance::pair_flickering(
                Blade {
                    length: hit,
                    ..blade
                },
                color,
                40.,
                Flicker::default(),
            );
            for (actual, expected) in pair.iter_mut().zip(expected) {
                actual.clip_length(hit);
                assert!((actual.radius - expected.radius).abs() < 0.00001);
                assert_eq!(actual.visible(), hit >= 0.5);
            }
        }
    }
}

#[cfg(test)]
mod skin_extras_tests {
    use super::*;
    use crate::saber_skins::{Glint, SkinColor};

    /// A glint at the tip with rays `length` long.
    fn at_tip(length: f32) -> Option<Glint> {
        Some(Glint {
            length,
            at: 0.0,
            count: 1,
            reach: 0.0,
        })
    }

    fn skin(star: Option<Glint>, echo: Option<crate::blade_skin_file::extras::Echo>) -> BladeColor {
        let BladeColor::Skin(base) =
            BladeColor::from_material(crate::saber_rgb::SKIN_MATERIAL, [1.0; 3])
        else {
            unreachable!()
        };
        BladeColor::Skin(SkinColor {
            star,
            echo,
            room: 4.0,
            off: 0b101,
            ..base
        })
    }

    fn blade() -> Blade {
        Blade {
            base: [10.0, 20.0, 30.0],
            direction: [0.0, 0.0, 1.0],
            length: 40.0,
            radius: 3.0,
        }
    }

    #[test]
    fn the_glint_is_a_quad_round_the_tip_and_never_a_contact() {
        let stars: Vec<_> = Instance::stars(blade(), skin(at_tip(9.0), None)).collect();
        assert_eq!(stars.len(), 1);
        let star = stars[0];
        assert!(star.is_star());
        assert_eq!(star.off, 0b101, "the wearer's mask rides along");
        let (start, end, reach) = star.extent();
        assert_eq!(start, [10.0, 20.0, 70.0]);
        assert_eq!(end, start);
        assert!((reach - 9.0 * std::f32::consts::SQRT_2).abs() < 1e-4);
        assert!(star.with_contact(1, 0, 0, true, false).contact().is_none());
        assert_eq!(Instance::stars(blade(), skin(None, None)).count(), 0);
        assert_eq!(
            Instance::stars(blade(), BladeColor::from_rgb([0, 0, 255])).count(),
            0
        );
    }

    #[test]
    fn orbiting_glints_are_numbered_and_bounded_round_their_circle() {
        let orbit = Glint {
            length: 6.0,
            at: 10.0,
            count: 2,
            reach: 5.0,
        };
        let stars: Vec<_> = Instance::stars(blade(), skin(Some(orbit), None)).collect();
        assert_eq!(stars.len(), 2);
        assert!(stars.iter().all(Instance::is_star));
        // Glint n is `STAR - n`, which the shader reads back.
        assert_eq!([stars[0].color[3], stars[1].color[3]], [STAR, STAR - 1.0]);
        // Each one's bounds: round the point 10 units below the tip, wide enough for its
        // rays anywhere on the circle.
        let (start, end, reach) = stars[1].extent();
        assert_eq!(start, [10.0, 20.0, 60.0]);
        assert_eq!(end, start);
        assert!((reach - (6.0 * std::f32::consts::SQRT_2 + 5.0)).abs() < 1e-4);
        // Never below the hilt on a short blade.
        let short = Blade {
            length: 4.0,
            ..blade()
        };
        let (start, _, _) = Instance::stars(short, skin(Some(orbit), None))
            .next()
            .unwrap()
            .extent();
        assert_eq!(start, [10.0, 20.0, 30.0]);
    }

    #[test]
    fn a_skins_room_widens_its_glow_bounds() {
        let [glow, core] = Instance::pair(blade(), skin(None, None));
        let [plain, _] = Instance::pair(blade(), BladeColor::from_rgb([0, 0, 255]));
        assert!(glow.extent().2 > plain.extent().2 + 4.0);
        assert_eq!(core.extent().2, core.radius, "the core is the core");
    }

    #[test]
    fn the_echo_sways_beside_the_blade_as_a_dim_glow() {
        let echo = crate::blade_skin_file::extras::Echo {
            fade: 0.4,
            sway: 2.0,
            rate: 0.5,
            lean: 0.05,
        };
        let color = skin(None, Some(echo));
        let at = |seconds: f64| Instance::echo(blade(), color, seconds, 7).unwrap();
        let first = at(1.0);
        assert!((first.fade() - 0.4).abs() < 1.0 / 255.0);
        assert!(first.color[3] > 0.0, "a glow, not a core");
        // It moves with time, stays near the blade and the same at the same moment.
        let tip =
            |i: &Instance| Vec3::from_array(i.base) + Vec3::from_array(i.direction) * i.length;
        let blade_tip = Vec3::new(10.0, 20.0, 70.0);
        assert_ne!(tip(&first), tip(&at(1.6)));
        assert_eq!(tip(&first), tip(&at(1.0)));
        for step in 0..20 {
            let away = tip(&at(f64::from(step) * 0.3)).distance(blade_tip);
            assert!(away <= 2.0 * 1.2 + 0.05 * 40.0 + 0.01, "{away}");
        }
        assert!(Instance::echo(blade(), skin(None, None), 1.0, 7).is_none());
    }
}
