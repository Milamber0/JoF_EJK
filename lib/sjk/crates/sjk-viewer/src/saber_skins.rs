//! Blade skins: unlockable looks that replace a saber blade's colour (not its hilt).
//! Their ids, names and descriptions are the catalogue's ([`crate::unlockables`]); their
//! looks and sounds are data, blade-skin files ([`crate::blade_skin_file`]) in the packs
//! the SJK hub delivers ([`crate::sjk_packs`], `docs/unlockables.md`). The renderer is
//! generic: nothing here or in `saber.wgsl` knows any one skin.
//!
//! The loaded skins ([`LoadedSkins`], at most [`MAX_SKINS`]) are numbered in load order.
//! A skin is drawn as the saber material of its number after the retail pairs and the
//! neutral RGB pair ([`BladeColor::Skin`]), with its glow/core pair (the pack's images or
//! generated from the file's profiles) and its parameters in the skins' uniform array,
//! animated by time in `saber.wgsl`. It has its own trail and light colours and its own
//! sounds, named by the file.
//!
//! Who wears which skin is the per-client [`SaberSkins`] table on [`GpuState`]. Saber
//! submission (in the hand, thrown, first person) asks it for each entity's blades, and
//! the audio adapter gets its sound overrides from it every frame
//! ([`GpuState::sync_saber_skins`]). The local player's entry is its own skin, gated by
//! its hub profile ([`GpuState::local_saber_skin`]), in its own slot (the game state's,
//! also while it follows someone); every other client's is the look the hub relayed
//! for its slot (`GpuState::looks`), taken again only when it or the loaded skins
//! change. A skin whose pack is not loaded is the player's stock blade.

use crate::blade_skin_file::BladeSkinDef;
use crate::saber_rgb::BladeColor;
use crate::{GameAudio, GpuState};
use bytemuck::{Pod, Zeroable};
use sjk_vfs::VirtualFileSystem;
use std::sync::Arc;

/// Players a game server can hold (`MAX_CLIENTS`); entity ids 1..=32 are their bodies.
pub(crate) const MAX_CLIENTS: usize = sjk_client::SABER_SOUND_CLIENTS;
/// Most blade skins loaded at once: material slots and the storage array's length
/// (`MAX_SKINS` in `saber.wgsl`). 256 skins are 248 KiB, far inside a storage binding.
pub(crate) const MAX_SKINS: usize = 256;
/// Widest a skin's glow may reach past the stock capsule (`corona.reach`); the
/// instances' bounds allow for it ([`crate::saber::Instance::extent`]).
pub(crate) const MAX_GLOW_REACH: f32 = 2.0;
/// Largest glow or core image a pack may give, per side.
const IMAGE_MAX: u32 = 1_024;

/// A loaded skin's light flicker ([`crate::blade_skin_file::Flicker`]), copied for the
/// per-frame light.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct LightFlicker {
    pub(crate) amount: f32,
    /// Rate, weight and phase of each wave; unused waves weigh nothing.
    pub(crate) waves: [[f32; 3]; 2],
    /// The skin's heartbeat (`pulse`), which the light beats with; none when absent.
    pub(crate) pulse: Option<Beat>,
}

/// A heartbeat ([`crate::blade_skin_file::effects::Pulse`]): the CPU's copy of
/// `saber.wgsl`'s `skin_beat`, which beats on the presentation clock alone (no seed), so
/// a blade and its light beat together.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Beat {
    pub(crate) rate: f32,
    pub(crate) amount: f32,
    pub(crate) second: f32,
    pub(crate) gap: f32,
    pub(crate) width: f32,
}

impl Beat {
    /// Where the first beat peaks in a cycle, seconds (`BEAT_FIRST` in `saber.wgsl`).
    pub(crate) const FIRST: f32 = 0.1;

    /// How much the beat brightens at `seconds`: `amount × (first + second × next)`, each
    /// a Gaussian `width` seconds wide.
    pub(crate) fn at(self, seconds: f32) -> f32 {
        let into = (seconds * self.rate).rem_euclid(1.0) / self.rate;
        let bump = |at: f32| {
            let d = (into - at) / self.width;
            (-d * d).exp()
        };
        self.amount * (bump(Self::FIRST) + self.second * bump(Self::FIRST + self.gap))
    }
}

impl LightFlicker {
    /// The light's brightness at `time_millis` for a hilt's `seed` (so blades of one
    /// hilt do not pulse together with another's): `1 − amount + amount × waves`.
    pub(crate) fn at(self, time_millis: i64, seed: u64) -> f32 {
        if self.amount == 0.0 && self.pulse.is_none() {
            return 1.0;
        }
        let t = (time_millis.rem_euclid(1_024_000)) as f32 * 0.001;
        let phase = (seed % 1_000) as f32 * 0.618;
        let wave = self
            .waves
            .iter()
            .map(|[rate, weight, offset]| (t * rate + phase * offset).sin() * weight)
            .sum::<f32>();
        let beat = self.pulse.map_or(0.0, |beat| beat.at(t));
        ((1.0 - self.amount) + self.amount * wave) * (1.0 + beat)
    }
}

/// What a blade wearing a loaded skin is drawn with: the skin's number (its material
/// and its parameters in `saber.wgsl`) and its trail, light and flicker. Small and
/// `Copy`, so blades carry it without the skin table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SkinColor {
    /// The skin's number among the loaded skins, below [`MAX_SKINS`].
    pub(crate) index: u8,
    /// The blur trail's vertex colour.
    pub(crate) trail: [f32; 3],
    /// The dynamic light's colour (before `saber_lights`' gain).
    pub(crate) light: [f32; 3],
    pub(crate) flicker: LightFlicker,
    /// The skin's hue turning (`hue` in its file), for the light: turns a second, and the
    /// turn at the middle of a stock 40-unit blade. Zeros hold the light's colour.
    pub(crate) hue: [f32; 2],
    /// The skin's afterimages (`ghosts` in its file); none when absent.
    pub(crate) ghosts: Option<crate::saber_ghosts::GhostSpec>,
    /// Who wears it (team, name), for the skins drawn from that; the default with no
    /// wearer known.
    pub(crate) persona: crate::saber_persona::Persona,
    /// A chroma's base hue and the turn to its wearer's colour ([`Chroma`]); the default
    /// for the other skins.
    pub(crate) chroma: Chroma,
    /// The sections its wearer switched off ([`crate::blade_skin_file::extras::Section`]
    /// bits; `saber.wgsl` draws them as absent). The CPU's parts (afterimages, echo,
    /// glint, the light's beat and hue) are already left out of this colour.
    pub(crate) off: u32,
    /// How far (units) the glow's quad reaches past the glow, for wisps and haze.
    pub(crate) room: f32,
    /// The glint at the tip, or the glints circling the blade below it, or the black
    /// holes in their places; none when absent or off.
    pub(crate) star: Option<Glint>,
    /// The echo beside the blade; none when absent or off.
    pub(crate) echo: Option<crate::blade_skin_file::extras::Echo>,
}

/// Where a skin's glints (or black holes) are, for their quads
/// ([`crate::saber::Instance::stars`]): `saber.wgsl` moves them round their circle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Glint {
    /// Its rays' length, or how far a hole's quad reaches (units).
    pub(crate) length: f32,
    /// How far below the tip (units) they circle (0 at the tip), and how many.
    pub(crate) at: f32,
    pub(crate) count: u8,
    /// The farthest (units) a glint's heart gets from that point on the axis: the circle's
    /// radius, its tilt included.
    pub(crate) reach: f32,
}

/// A chroma skin ([`crate::unlockables::Unlockable::chroma`]) takes its wearer's saber
/// colour: every colour it draws is turned round the grey axis ([`turn_hue`]) from the
/// skin's own hue to the colour's, keeping brightness and saturation. A grey or white
/// colour (custom RGB) leaves the skin as its file draws it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Chroma {
    /// The skin's own hue, in turns ([`hue_of`] of its corona's rim); `None` for a skin
    /// that is not a chroma.
    pub(crate) base: Option<f32>,
    /// Turns from the skin's hue to its wearer's colour; 0 draws the file's colours.
    pub(crate) turn: f32,
}

/// Below this saturation (`(max − min) / max` of a colour) a colour has no hue to take.
const CHROMA_GREY: f32 = 0.2;

/// `rgb`'s hue in turns [0, 1): its angle round the grey axis from pure red, in the
/// direction [`turn_hue`] turns; `None` for a colour too grey to have one.
pub(crate) fn hue_of(rgb: [f32; 3]) -> Option<f32> {
    let high = rgb[0].max(rgb[1]).max(rgb[2]);
    let low = rgb[0].min(rgb[1]).min(rgb[2]);
    if high <= 0.0 || (high - low) / high < CHROMA_GREY {
        return None;
    }
    // Red's direction off the grey axis, and the axis crossed with it (turn_hue's sense).
    let x = (2.0 * rgb[0] - rgb[1] - rgb[2]) / 6.0_f32.sqrt();
    let y = (rgb[1] - rgb[2]) / 2.0_f32.sqrt();
    Some((y.atan2(x) / std::f32::consts::TAU).rem_euclid(1.0))
}

impl SkinColor {
    /// This skin on a blade whose stock colour is `stock`: a chroma turned to its hue,
    /// its light and trail with it; any other skin, or a grey `stock`, as it is.
    pub(crate) fn worn_with(self, stock: BladeColor) -> Self {
        let Some(base) = self.chroma.base else {
            return self;
        };
        let rgb = match stock {
            BladeColor::Retail(color) => color.blade_rgb(),
            BladeColor::Rgb(rgb) => rgb,
            BladeColor::Skin(_) => return self,
        };
        let turn = hue_of(rgb.map(|channel| f32::from(channel) / 255.0))
            .map_or(0.0, |hue| (hue - base).rem_euclid(1.0));
        Self {
            chroma: Chroma {
                base: Some(base),
                turn,
            },
            ..self
        }
    }

    /// The dynamic light's colour, a chroma's turned to its wearer's.
    pub(crate) fn light_color(&self) -> [f32; 3] {
        self.chromatic(self.light)
    }

    /// The blur trail's colour, a chroma's turned to its wearer's.
    pub(crate) fn trail_color(&self) -> [f32; 3] {
        self.chromatic(self.trail)
    }

    fn chromatic(&self, rgb: [f32; 3]) -> [f32; 3] {
        if self.chroma.turn == 0.0 {
            rgb
        } else {
            turn_hue(rgb, self.chroma.turn)
        }
    }
}

impl SkinColor {
    /// The light's colour at `time_millis`: the file's, turned with the hue of the
    /// blade's middle when the skin's hue turns.
    pub(crate) fn light_at(&self, time_millis: i64) -> [f32; 3] {
        let [rate, middle] = self.hue;
        if rate == 0.0 && middle == 0.0 {
            return self.light_color();
        }
        let t = (time_millis.rem_euclid(1_024_000)) as f32 * 0.001;
        turn_hue(self.light, t * rate + middle + self.chroma.turn)
    }
}

/// `rgb` turned `turns` round the grey axis, negative channels clipped: the CPU's copy of
/// `saber.wgsl`'s `skin_turn_hue` (a hue shift keeping brightness and saturation).
pub(crate) fn turn_hue(rgb: [f32; 3], turns: f32) -> [f32; 3] {
    let axis = 1.0 / 3.0_f32.sqrt();
    let (s, c) = (std::f32::consts::TAU * turns).sin_cos();
    let along_axis = axis * (rgb[0] + rgb[1] + rgb[2]) * axis * (1.0 - c);
    // axis × rgb, with every axis component equal.
    let cross = [
        axis * (rgb[2] - rgb[1]),
        axis * (rgb[0] - rgb[2]),
        axis * (rgb[1] - rgb[0]),
    ];
    std::array::from_fn(|i| (rgb[i] * c + cross[i] * s + along_axis).max(0.0))
}

/// One skin's parameters as `saber.wgsl`'s `Skin` holds them: 95 `vec4`s (1520 bytes, so
/// the [`MAX_SKINS`] array is 389120 bytes, a read-only storage binding). The layout is the shader's; see there for what each lane is. A section
/// the skin's file leaves out (arcs, motes, hue and the later ones) is zeros, which the
/// shader draws as nothing.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct SkinUniform {
    core_white: [f32; 4],
    core_fringe_cool: [f32; 4],
    core_fringe_hot: [f32; 4],
    core_fringe: [f32; 4],
    core_breathe: [f32; 4],
    rim_cool: [f32; 4],
    rim_hot: [f32; 4],
    inner: [f32; 4],
    inner_mix: [f32; 4],
    brightness: [f32; 4],
    swell: [f32; 4],
    grain_coarse: [f32; 4],
    grain_fine: [f32; 4],
    grain_mix: [f32; 4],
    flare_rate: [f32; 4],
    flare_shape: [f32; 4],
    flare_size: [f32; 4],
    shimmer_a: [f32; 4],
    shimmer_b: [f32; 4],
    tongue_a: [f32; 4],
    tongue_b: [f32; 4],
    arc_color: [f32; 4],
    arc_shape: [f32; 4],
    arc_strike: [f32; 4],
    arc_place: [f32; 4],
    arc_motion: [f32; 4],
    mote_color: [f32; 4],
    mote_field: [f32; 4],
    mote_motion: [f32; 4],
    mote_band: [f32; 4],
    hue: [f32; 4],
    sputter_edge: [f32; 4],
    sputter_cut: [f32; 4],
    glitch_a: [f32; 4],
    glitch_b: [f32; 4],
    scan_color: [f32; 4],
    scan_lines: [f32; 4],
    scan_edge: [f32; 4],
    scan_jitter: [f32; 4],
    pulse_a: [f32; 4],
    pulse_b: [f32; 4],
    ember_color: [f32; 4],
    ember_field: [f32; 4],
    ember_shape: [f32; 4],
    vein_color: [f32; 4],
    vein_shape: [f32; 4],
    team_red: [f32; 4],
    team_blue: [f32; 4],
    team_none: [f32; 4],
    ambient: [f32; 4],
    glyph_color: [f32; 4],
    glyph_shape: [f32; 4],
    star_color: [f32; 4],
    star_shape: [f32; 4],
    star_motion: [f32; 4],
    star_orbit: [f32; 4],
    hole_color: [f32; 4],
    hole_shape: [f32; 4],
    wisp_color: [f32; 4],
    wisp_shape: [f32; 4],
    wisp_field: [f32; 4],
    haze: [f32; 4],
    band_color: [f32; 4],
    band_shape: [f32; 4],
    band_mix: [f32; 4],
    segment_a: [f32; 4],
    segment_b: [f32; 4],
    film: [f32; 4],
    film_b: [f32; 4],
    blob_color: [f32; 4],
    blob_shape: [f32; 4],
    blob_more: [f32; 4],
    front_color: [f32; 4],
    front_shape: [f32; 4],
    front_edge: [f32; 4],
    sketch_color: [f32; 4],
    sketch_shape: [f32; 4],
    hour_color: [f32; 4],
    hour_shape: [f32; 4],
    serpent_color: [f32; 4],
    serpent_shape: [f32; 4],
    strand_shape: [f32; 4],
    strand_hue: [f32; 4],
    fly_color: [f32; 4],
    fly_shape: [f32; 4],
    fly_roam: [f32; 4],
    wing_color: [f32; 4],
    wing_shape: [f32; 4],
    copy_shape: [f32; 4],
    copy_more: [f32; 4],
    ink_color: [f32; 4],
    ink_shape: [f32; 4],
    ink_more: [f32; 4],
    chrome_color: [f32; 4],
    chrome_shape: [f32; 4],
}

impl SkinUniform {
    /// The parameters of `def`, as the shader reads them.
    pub(crate) fn of(def: &BladeSkinDef) -> Self {
        let rgb = |c: [f32; 3], w: f32| [c[0], c[1], c[2], w];
        let wave = |w: &crate::blade_skin_file::ShimmerWave| [w.amount, w.rate, w.along, w.seed];
        let octave =
            |index: usize| {
                def.granulation.octaves.get(index).copied().unwrap_or(
                    crate::blade_skin_file::Octave {
                        scale: 0.0,
                        speed: 0.0,
                        evolve: 0.0,
                        offset: 0.0,
                        weight: 0.0,
                    },
                )
            };
        let (coarse, fine) = (octave(0), octave(1));
        let shimmer = |index: usize| def.shimmer.get(index).map_or([0.0; 4], wave);
        let (c, r, f, t) = (&def.core, &def.corona, &def.flares, &def.tongues);
        Self {
            core_white: rgb(c.white, c.white_flare),
            core_fringe_cool: rgb(c.fringe_cool, c.fringe_heat.base),
            core_fringe_hot: rgb(c.fringe_hot, c.fringe_heat.grain),
            core_fringe: [
                c.fringe_brightness.base,
                c.fringe_brightness.grain,
                c.fringe_brightness.flare,
                c.tip,
            ],
            core_breathe: wave(&c.breathe),
            rim_cool: rgb(r.rim_cool, r.inner_width),
            rim_hot: rgb(r.rim_hot, r.rim_heat.grain),
            inner: rgb(r.inner, r.rim_heat.flare),
            inner_mix: [r.inner_mix.base, r.inner_mix.grain, r.inner_mix.flare, 0.0],
            brightness: [
                r.brightness.base,
                r.brightness.grain,
                r.brightness.flare,
                0.0,
            ],
            swell: [r.reach, r.swell.grain, r.swell.flare, 0.0],
            grain_coarse: [coarse.scale, coarse.speed, coarse.evolve, coarse.offset],
            grain_fine: [fine.scale, fine.speed, fine.evolve, fine.offset],
            grain_mix: [
                coarse.weight,
                fine.weight,
                def.granulation.low,
                def.granulation.high,
            ],
            flare_rate: [f.rate, f.rate_step, f.rate_seed, f.count as f32],
            flare_shape: [f.phase_seed, f.phase_step, f.threshold, f.overshoot],
            flare_size: [f.size, f.size_jitter, 0.0, 0.0],
            shimmer_a: shimmer(0),
            shimmer_b: shimmer(1),
            tongue_a: [t.along, t.offset, t.out, t.speed],
            tongue_b: [t.edge_low, t.edge_high, t.low, t.range],
            ..Self::default()
        }
        .with_arcs(def.arcs.as_ref())
        .with_motes(def.motes.as_ref())
        .with_hue(def.hue.as_ref())
        .with_effects(def.effects())
        .with_extras(def)
        .with_lab(def.lab())
        .with_figures(def.figures())
        .with_solids(def.solids())
    }

    /// The seventh set's lanes ([`crate::blade_skin_file::solids`]).
    fn with_solids(mut self, s: crate::blade_skin_file::solids::Solids<'_>) -> Self {
        let rgb = |c: [f32; 3], w: f32| [c[0], c[1], c[2], w];
        if let Some(i) = s.ink {
            self.ink_color = rgb(i.color, i.body);
            self.ink_shape = [i.outline, i.width, i.edge, i.point];
            self.ink_more = [i.gloss, i.halftone, 0.0, 0.0];
        }
        if let Some(c) = s.chrome {
            self.chrome_color = rgb(c.color, c.width);
            self.chrome_shape = [c.ripple, c.beads, c.speed, 0.0];
        }
        self
    }

    /// The sixth set's lanes ([`crate::blade_skin_file::figures`]).
    fn with_figures(mut self, f: crate::blade_skin_file::figures::Figures<'_>) -> Self {
        let rgb = |c: [f32; 3], w: f32| [c[0], c[1], c[2], w];
        if let Some(s) = f.sketch {
            self.sketch_color = rgb(s.color, s.brightness);
            self.sketch_shape = [s.rate, s.wobble, s.hatch, s.glow];
        }
        if let Some(h) = f.hourglass {
            self.hour_color = rgb(h.color, h.brightness);
            self.hour_shape = [h.period, h.waist, h.glass, 0.0];
        }
        if let Some(s) = f.serpent {
            self.serpent_color = rgb(s.color, s.brightness);
            self.serpent_shape = [s.coils, s.amplitude, s.speed, s.thickness];
        }
        if let Some(s) = f.strands {
            self.strand_shape = [s.count as f32, s.spread, s.sway, s.brightness];
            self.strand_hue = [s.hue, s.saturation, 0.0, 0.0];
        }
        if let Some(b) = f.butterflies {
            self.fly_color = rgb(b.color, b.brightness);
            self.fly_shape = [b.count as f32, b.size, b.flap, b.flight];
            self.fly_roam = [b.roam, 0.0, 0.0, 0.0];
        }
        if let Some(w) = f.wings {
            self.wing_color = rgb(w.color, w.brightness);
            self.wing_shape = [w.span, w.rate, w.feathers as f32, 0.0];
        }
        if let Some(c) = f.copies {
            self.copy_shape = [c.count as f32, c.angle, c.fade, c.period];
            self.copy_more = [c.flicker, 0.0, 0.0, 0.0];
        }
        self
    }

    /// The fifth set's lanes ([`crate::blade_skin_file::lab`]).
    fn with_lab(mut self, lab: crate::blade_skin_file::lab::Lab<'_>) -> Self {
        let rgb = |c: [f32; 3], w: f32| [c[0], c[1], c[2], w];
        if let Some(b) = lab.bands {
            self.band_color = rgb(b.color, b.brightness);
            self.band_shape = [b.spacing, b.width, b.twist, b.speed];
            self.band_mix = [b.mix, b.pinch, 0.0, 0.0];
        }
        if let Some(s) = lab.segments {
            self.segment_a = [s.count as f32, s.rate, s.threshold, s.depth];
            self.segment_b = [s.gap, s.hum, 0.0, 0.0];
        }
        if let Some(f) = lab.film {
            self.film = [f.amount, f.scale, f.speed, f.saturation];
            self.film_b = [f.clear, 0.0, 0.0, 0.0];
        }
        if let Some(b) = lab.blobs {
            self.blob_color = rgb(b.color, b.brightness);
            self.blob_shape = [b.count as f32, b.size, b.speed, b.wobble];
            self.blob_more = [b.pools, 0.0, 0.0, 0.0];
        }
        if let Some(f) = lab.front {
            self.front_color = rgb(f.color, f.amount);
            self.front_shape = [f.period, f.duty, f.hold, f.reach];
            self.front_edge = [f.soft, f.jag, f.veins, 0.0];
        }
        self
    }

    /// The third set's lanes ([`crate::blade_skin_file::extras`]), and the room the glow's
    /// quad keeps past the glow for them (`swell.w`, units, before any option is off).
    fn with_extras(mut self, def: &BladeSkinDef) -> Self {
        let rgb = |c: [f32; 3], w: f32| [c[0], c[1], c[2], w];
        if let Some(s) = &def.star {
            self.star_color = rgb(s.color, s.brightness);
            self.star_shape = [s.rays as f32, s.length, s.width, s.halo];
            self.star_motion = [s.spin, s.twinkle, s.depth, 0.0];
        }
        if let Some(h) = &def.holes {
            self.hole_color = rgb(h.color, h.brightness);
            self.hole_shape = [h.size, h.lens, h.spin, h.reach()];
        }
        if let Some(o) = def
            .orbit
            .filter(|_| def.star.is_some() || def.holes.is_some())
        {
            self.star_motion[3] = o.count as f32;
            self.star_orbit = [o.at, o.radius, o.rate, o.tilt];
        }
        if let Some(w) = &def.wisps {
            self.wisp_color = rgb(w.color, w.brightness);
            self.wisp_shape = [w.rise, w.speed, w.scale, w.curl];
            self.wisp_field = [w.density, 0.0, 0.0, 0.0];
        }
        if let Some(h) = &def.haze {
            self.haze = [h.strength, h.scale, h.speed, h.reach];
        }
        self
    }

    /// The later sections' lanes ([`crate::blade_skin_file::effects`]).
    fn with_effects(mut self, e: crate::blade_skin_file::effects::Effects<'_>) -> Self {
        let rgb = |c: [f32; 3], w: f32| [c[0], c[1], c[2], w];
        if let Some(s) = e.sputter {
            self.sputter_edge = [s.ragged, s.scale, s.speed, 0.0];
            self.sputter_cut = [s.cut, s.rate, s.threshold, 0.0];
        }
        if let Some(g) = e.glitch {
            self.glitch_a = [g.split, g.blocks, g.rate, g.threshold];
            self.glitch_b = [g.shift, g.flash, 0.0, 0.0];
        }
        if let Some(s) = e.scan {
            self.scan_color = rgb(s.color, s.brightness);
            self.scan_lines = [s.lines, s.speed, s.depth, s.rings];
            self.scan_edge = [s.edge, s.width, s.hollow, 0.0];
            self.scan_jitter = [s.jitter, s.jitter_rate, 0.0, 0.0];
        }
        if let Some(p) = e.pulse {
            self.pulse_a = [p.rate, p.amount, p.second, p.gap];
            self.pulse_b = [p.width, p.swell, 0.0, 0.0];
        }
        if let Some(m) = e.embers {
            self.ember_color = rgb(m.color, m.brightness);
            self.ember_field = [m.density, m.cells, m.fall, m.life];
            self.ember_shape = [m.size, m.spread, 0.0, 0.0];
        }
        if let Some(v) = e.veins {
            self.vein_color = rgb(v.color, v.brightness);
            self.vein_shape = [v.scale, v.speed, v.width, v.core];
        }
        if let Some(t) = e.team {
            self.team_red = rgb(t.red, t.amount);
            self.team_blue = rgb(t.blue, 0.0);
            self.team_none = rgb(t.none, 0.0);
        }
        if let Some(a) = e.ambient {
            self.ambient = [a.amount, a.saturate, a.floor, 0.0];
        }
        if let Some(g) = e.glyphs {
            self.glyph_color = rgb(g.color, g.brightness);
            self.glyph_shape = [g.size, g.spacing, g.speed, g.width];
        }
        self
    }

    fn with_arcs(mut self, arcs: Option<&crate::blade_skin_file::Arcs>) -> Self {
        if let Some(a) = arcs {
            self.arc_color = [a.color[0], a.color[1], a.color[2], a.brightness];
            self.arc_shape = [a.width, a.halo, a.jag, a.kinks];
            self.arc_strike = [a.count as f32, a.rate, a.threshold, a.decay];
            self.arc_place = [a.reach, a.span.low, a.span.high, a.tip];
            self.arc_motion = [a.jitter, a.crawl, 0.0, 0.0];
        }
        self
    }

    fn with_motes(mut self, motes: Option<&crate::blade_skin_file::Motes>) -> Self {
        if let Some(m) = motes {
            self.mote_color = [m.color[0], m.color[1], m.color[2], m.brightness];
            self.mote_field = [m.density, m.cells, m.rings, m.size];
            self.mote_motion = [m.drift.along, m.drift.out, m.twinkle, m.stretch];
            self.mote_band = [m.inner, m.outer, m.focus, 0.0];
        }
        self
    }

    fn with_hue(mut self, hue: Option<&crate::blade_skin_file::Hue>) -> Self {
        if let Some(h) = hue {
            self.hue = [h.rate, h.along, h.out, 0.0];
        }
        self
    }
}

/// A loaded blade skin.
#[derive(Clone, Debug)]
pub(crate) struct LoadedSkin {
    /// The catalogue's id of the unlock it is.
    pub(crate) id: &'static str,
    pub(crate) def: BladeSkinDef,
    /// Its glow and core images, RGBA.
    pub(crate) glow: image::RgbaImage,
    pub(crate) core: image::RgbaImage,
}

impl LoadedSkin {
    /// Read `def` (the file of unlock `id`), taking its images from `packs`.
    pub(crate) fn new(
        id: &'static str,
        def: BladeSkinDef,
        packs: &VirtualFileSystem,
    ) -> Result<Self, String> {
        let image =
            |path: &Option<String>, size: [u32; 2], generated: fn(&BladeSkinDef) -> Vec<u8>| {
                match path {
                    Some(path) => pack_image(packs, path),
                    None => image::RgbaImage::from_raw(size[0], size[1], generated(&def))
                        .ok_or_else(|| "a generated image of the wrong size".to_owned()),
                }
            };
        let glow = image(&def.glow_image, crate::saber_rgb::glow_size(), |def| {
            crate::saber_rgb::generated_glow(def.glow_profile)
        })?;
        let core = image(&def.core_image, crate::saber_rgb::core_size(), |def| {
            crate::saber_rgb::generated_core(def.core_profile)
        })?;
        Ok(Self {
            id,
            def,
            glow,
            core,
        })
    }

    /// How far a blade whose stock colour is `stock` turns its colours: a chroma's turn
    /// to that colour ([`SkinColor::worn_with`]), 0 for any other skin.
    pub(crate) fn chroma_turn(&self, stock: BladeColor) -> f32 {
        self.color(0, 0).worn_with(stock).chroma.turn
    }

    /// What its blades are drawn with, as the loaded skin numbered `index`, worn with
    /// the options in `off` switched off.
    fn color(&self, index: u8, off: crate::saber_skin_options::OptionsOff) -> SkinColor {
        use crate::blade_skin_file::extras::Section;
        let off = self.def.off_mask(off);
        let on = |section: Section| off & section.bit() == 0;
        let flicker = &self.def.light.flicker;
        let mut waves = [[0.0; 3]; 2];
        for (slot, wave) in waves.iter_mut().zip(&flicker.waves) {
            *slot = [wave.rate, wave.weight, wave.phase];
        }
        SkinColor {
            index,
            trail: self.def.trail,
            light: self.def.light.color,
            flicker: LightFlicker {
                amount: flicker.amount,
                waves,
                pulse: self
                    .def
                    .pulse
                    .filter(|_| on(Section::Pulse))
                    .map(|pulse| Beat {
                        rate: pulse.rate,
                        amount: pulse.amount,
                        second: pulse.second,
                        gap: pulse.gap,
                        width: pulse.width,
                    }),
            },
            hue: self
                .def
                .hue
                .filter(|_| on(Section::Hue))
                .map_or([0.0; 2], |hue| [hue.rate, hue.along * 20.0]),
            ghosts: self
                .def
                .ghosts
                .filter(|_| on(Section::Ghosts))
                .map(|ghosts| crate::saber_ghosts::GhostSpec {
                    count: ghosts.count as u8,
                    spacing_millis: ghosts.spacing,
                    fade: ghosts.fade,
                }),
            persona: crate::saber_persona::Persona::default(),
            chroma: Chroma {
                base: crate::unlockables::is_chroma(self.id)
                    .then(|| {
                        hue_of(self.def.corona.rim_cool).or_else(|| hue_of(self.def.light.color))
                    })
                    .flatten(),
                turn: 0.0,
            },
            off,
            room: self.def.room(off),
            star: self
                .def
                .star
                .filter(|_| on(Section::Star))
                .map(|star| star.length)
                .or_else(|| {
                    self.def
                        .holes
                        .filter(|_| on(Section::Holes))
                        .map(|holes| holes.reach())
                })
                .map(|length| match self.def.orbit {
                    Some(orbit) => Glint {
                        length,
                        at: orbit.at,
                        count: orbit.count as u8,
                        reach: orbit.radius / orbit.tilt.cos(),
                    },
                    None => Glint {
                        length,
                        at: 0.0,
                        count: 1,
                        reach: 0.0,
                    },
                }),
            echo: self.def.echo.filter(|_| on(Section::Echo)),
        }
    }

    /// The options its file offers.
    pub(crate) fn options(&self) -> &[crate::blade_skin_file::extras::SkinOption] {
        &self.def.options
    }
}

/// An image file `path` of the packs, decoded.
fn pack_image(packs: &VirtualFileSystem, path: &str) -> Result<image::RgbaImage, String> {
    let bytes = packs
        .read(path)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("{path} is not in the pack"))?
        .bytes;
    let image = image::load_from_memory(&bytes)
        .map_err(|error| format!("{path}: {error}"))?
        .to_rgba8();
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 || width > IMAGE_MAX || height > IMAGE_MAX {
        return Err(format!(
            "{path} is {width}×{height}; images are 1 to {IMAGE_MAX} a side"
        ));
    }
    Ok(image)
}

/// The blade skins loaded from the packs, numbered in id order, and the packs
/// themselves (their sounds are read from there).
#[derive(Clone, Default)]
pub(crate) struct LoadedSkins {
    skins: Vec<LoadedSkin>,
    /// Every pack, mounted in name order.
    packs: Arc<VirtualFileSystem>,
    /// Counts loads; 0 is the empty table before any.
    generation: u64,
}

impl std::fmt::Debug for LoadedSkins {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedSkins")
            .field("ids", &self.ids().collect::<Vec<_>>())
            .field("generation", &self.generation)
            .finish()
    }
}

impl LoadedSkins {
    /// The skins of the blade-skin files in `packs` whose ids are blade skins of the
    /// catalogue, at most [`MAX_SKINS`]; the others are named in the log.
    pub(crate) fn load(packs: VirtualFileSystem, generation: u64) -> Self {
        let mut skins = Vec::new();
        for (id, def) in crate::blade_skin_file::load(&packs) {
            let path = format!(
                "{}/{id}{}",
                crate::blade_skin_file::FOLDER,
                crate::blade_skin_file::EXTENSION
            );
            let Some(known) = crate::unlockables::blade_skin(&id) else {
                crate::log::progress(format_args!(
                    "blade skins: {path}: not a blade skin this client knows; left out"
                ));
                continue;
            };
            if skins.len() == MAX_SKINS {
                crate::log::progress(format_args!(
                    "blade skins: {path}: more than {MAX_SKINS} skins; left out"
                ));
                continue;
            }
            match LoadedSkin::new(known.id, def, &packs) {
                Ok(skin) => skins.push(skin),
                Err(why) => crate::log::progress(format_args!("blade skins: {path}: {why}")),
            }
        }
        Self {
            skins,
            packs: Arc::new(packs),
            generation,
        }
    }

    /// Skins made in a test, with no pack behind them.
    #[cfg(test)]
    pub(crate) fn of(skins: Vec<LoadedSkin>, generation: u64) -> Self {
        Self {
            skins,
            packs: Arc::default(),
            generation,
        }
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// The ids of the loaded skins, in their numbers' order.
    pub(crate) fn ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.skins.iter().map(|skin| skin.id)
    }

    /// The packs every loaded file came from.
    pub(crate) fn packs(&self) -> &Arc<VirtualFileSystem> {
        &self.packs
    }

    /// The loaded skin of unlock `id`, if its pack is loaded.
    pub(crate) fn get(&self, id: &str) -> Option<&LoadedSkin> {
        self.skins.iter().find(|skin| skin.id == id)
    }

    /// What the blades of a player wearing unlock `id` with every option on are drawn
    /// with; `None` (the stock blade) when no loaded skin is that unlock.
    #[cfg(test)]
    pub(crate) fn color_of(&self, id: &str) -> Option<SkinColor> {
        self.color_with(id, 0)
    }

    /// [`Self::color_of`], worn with the options in `off` switched off.
    pub(crate) fn color_with(
        &self,
        id: &str,
        off: crate::saber_skin_options::OptionsOff,
    ) -> Option<SkinColor> {
        self.skins
            .iter()
            .position(|skin| skin.id == id)
            .map(|index| self.skins[index].color(index as u8, off))
    }

    /// The loaded skins with their numbers.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (u8, &LoadedSkin)> {
        self.skins
            .iter()
            .enumerate()
            .map(|(index, skin)| (index as u8, skin))
    }

    /// The uniform array `saber.wgsl` reads, zero past the loaded skins.
    pub(crate) fn uniforms(&self) -> Vec<SkinUniform> {
        let mut uniforms = vec![SkinUniform::default(); MAX_SKINS];
        for (uniform, skin) in uniforms.iter_mut().zip(&self.skins) {
            *uniform = SkinUniform::of(&skin.def);
        }
        uniforms
    }

    /// Whether the skin numbered `index` takes the colour of the light where it is
    /// (`ambient` in its file), so its instances want that light
    /// ([`crate::saber_persona::light_instances`]).
    pub(crate) fn takes_ambient(&self, index: u8) -> bool {
        self.skins
            .get(usize::from(index))
            .is_some_and(|skin| skin.def.ambient.is_some())
    }

    /// The skins' sound sets in their numbers' order, for the audio adapter.
    pub(crate) fn sound_sets(&self) -> Vec<sjk_client::SaberSoundSet<'_>> {
        self.skins
            .iter()
            .map(|skin| {
                let sounds = &skin.def.sounds;
                sjk_client::SaberSoundSet {
                    on: &sounds.on,
                    off: &sounds.off,
                    hum: &sounds.hum,
                    swings: [&sounds.swings[0], &sounds.swings[1], &sounds.swings[2]],
                }
            })
            .collect()
    }

    /// The swing skin `index` plays for a stock swing cue's `variant`.
    pub(crate) fn swing(&self, index: u8, variant: usize) -> Option<&str> {
        self.skins
            .get(usize::from(index))
            .map(|skin| skin.def.sounds.swings[variant % 3].as_str())
    }
}

/// Who wears which skin: one entry per client slot, and the local player's own choice
/// for the menus' preview (no client slot there).
#[derive(Clone, Debug, Default)]
pub(crate) struct SaberSkins {
    /// What each slot is drawn wearing: `others`, with the local player's own in its slot.
    clients: [Option<SkinColor>; MAX_CLIENTS],
    /// What the hub's looks say each slot wears.
    others: [Option<SkinColor>; MAX_CLIENTS],
    /// The local player's skin, also shown with no session (the Character preview).
    local: Option<SkinColor>,
    /// The local player's client slot, in a session.
    local_client: Option<usize>,
    /// Who sits in each slot (team, name), for the skins drawn from that.
    personas: [crate::saber_persona::Persona; MAX_CLIENTS],
    /// The local player's own, also with no session (its `name`).
    local_persona: crate::saber_persona::Persona,
    /// The looks' revision and the loaded skins' generation `others` was last taken at.
    taken_at: Option<(u64, u64)>,
    /// Blades the world shots add every frame, without a session (blade, unlock id or
    /// a stock colour, seed).
    #[cfg(test)]
    pub(crate) shot_blades: Vec<(crate::saber::Blade, ShotColor, u32)>,
    /// The time the world shots' blades are drawn at, instead of the frame's.
    #[cfg(test)]
    pub(crate) shot_seconds: Option<f64>,
    /// Afterimages the world shots add every frame (pose, unlock id, brightness).
    #[cfg(test)]
    pub(crate) shot_ghosts: Vec<(crate::saber::Blade, &'static str, f32)>,
    /// Who the world shots' skinned blades are worn by (team, name).
    #[cfg(test)]
    pub(crate) shot_persona: crate::saber_persona::Persona,
}

/// A world shot's blade colour: stock, or the loaded skin of an unlock id.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum ShotColor {
    Stock(BladeColor),
    Skin(&'static str),
    /// A skin on a saber of that stock colour (a chroma takes it).
    Worn(&'static str, BladeColor),
    /// A skin with these options switched off (`crate::saber_skin_options`).
    Parts(&'static str, crate::saber_skin_options::OptionsOff),
}

impl SaberSkins {
    /// Set what the hub says client slot `client` wears (`None` for the stock blade);
    /// out-of-range slots are ignored, and the local player's own slot keeps its skin.
    #[cfg(test)]
    pub(crate) fn set(&mut self, client: usize, skin: Option<SkinColor>) {
        if let Some(slot) = self.others.get_mut(client) {
            *slot = skin;
            self.merge();
        }
    }

    /// Take every other slot's skin from the looks (`skin_of` a client slot) when their
    /// `revision` or the loaded skins' `generation` is not the one last taken; no
    /// allocation, 32 lookups when it changed.
    pub(crate) fn follow_looks(
        &mut self,
        revision: u64,
        generation: u64,
        skin_of: impl Fn(usize) -> Option<SkinColor>,
    ) {
        if self.taken_at == Some((revision, generation)) {
            return;
        }
        self.taken_at = Some((revision, generation));
        for (client, skin) in self.others.iter_mut().enumerate() {
            *skin = skin_of(client);
        }
        self.merge();
    }

    /// Draw `others`, with the local player's own skin in its slot.
    fn merge(&mut self) {
        self.clients = self.others;
        if let Some(slot) = self
            .local_client
            .and_then(|client| self.clients.get_mut(client))
        {
            *slot = self.local;
        }
    }

    /// The skin worn by the player whose body is entity `entity_id` (client + 1).
    pub(crate) fn get(&self, entity_id: u64) -> Option<SkinColor> {
        let client = usize::try_from(entity_id.checked_sub(1)?).ok()?;
        let persona = if Some(client) == self.local_client {
            self.local_persona.in_team(self.personas[client].team)
        } else {
            self.personas.get(client).copied()?
        };
        let skin = self.clients.get(client).copied().flatten()?;
        Some(SkinColor { persona, ..skin })
    }

    /// Who sits in each client slot, and the local player's own name (`name`), for
    /// the skins drawn from them; taken twice a second ([`GpuState::update_looks`]).
    pub(crate) fn set_personas(
        &mut self,
        personas: [crate::saber_persona::Persona; MAX_CLIENTS],
        local: crate::saber_persona::Persona,
    ) {
        self.personas = personas;
        self.local_persona = local;
    }

    /// The local player's own persona, for the menus' preview.
    pub(crate) fn local_persona(&self) -> crate::saber_persona::Persona {
        self.local_persona
    }

    /// The colour entity `entity_id`'s blades are drawn with: its skin over `color`, a
    /// chroma in `color`'s hue.
    pub(crate) fn blade_color(&self, entity_id: u64, color: BladeColor) -> BladeColor {
        self.get(entity_id)
            .map_or(color, |skin| BladeColor::Skin(skin.worn_with(color)))
    }

    /// The local player's skin, worn with its own persona.
    pub(crate) fn local(&self) -> Option<SkinColor> {
        self.local.map(|skin| SkinColor {
            persona: self.local_persona,
            ..skin
        })
    }

    /// The local player wears `skin` and, in a session, sits in slot `client`.
    pub(crate) fn set_local(&mut self, client: Option<usize>, skin: Option<SkinColor>) {
        if (client, skin) == (self.local_client, self.local) {
            return;
        }
        self.local = skin;
        self.local_client = client;
        self.merge();
    }

    /// Each client slot's sound set (its skin's number) for the audio adapter.
    pub(crate) fn sound_sets(&self) -> [Option<u8>; MAX_CLIENTS] {
        self.clients.map(|skin| skin.map(|skin| skin.index))
    }
}

impl GpuState {
    /// The skin the local player wears: `cg_saberSkin` only while the own hub profile
    /// lists that unlock (`Looks::own_saber_skin`, read twice a second) and its pack is
    /// loaded; with no identity, no hub, the unlock missing or no pack, the stock blade.
    pub(crate) fn local_saber_skin(&self) -> Option<SkinColor> {
        let own = self.looks.own();
        own.saber_skin
            .and_then(|id| self.blade_skins.color_with(id, own.options_off))
    }

    /// Once a frame, before the session's sabers are submitted: the loaded skins when a
    /// pack brought new ones (their parameters and pairs uploaded, their sounds
    /// registered again), every other client's entry from the hub's looks when they or
    /// the skins changed, the local player's from [`Self::local_saber_skin`] in its own
    /// slot (the game state's: following someone, the snapshot's player state is theirs,
    /// and they wear their own look), and every client's sound override to the audio
    /// adapter. No allocation unless skins were loaded: a few compares, and 32-entry
    /// copies on a change.
    pub(crate) fn sync_saber_skins(
        &mut self,
        game_audio: &mut Option<GameAudio>,
        presentation_time: i64,
    ) {
        if crate::sjk_packs::generation() != self.blade_skins.generation() {
            self.blade_skins = crate::sjk_packs::skins();
            self.saber_gpu
                .upload_skins(&self.device, &self.queue, &self.blade_skins);
        }
        let client = self
            .live_session
            .as_ref()
            .map(|session| (session.game_state(), session.latest_snapshot()))
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(|session| (session.game_state(), session.latest_snapshot()))
            })
            .and_then(|(game_state, snapshot)| {
                crate::looks::ViewSlots::of(game_state, &snapshot.player).own
            });
        let (looks, skins) = (&self.looks, &self.blade_skins);
        self.saber_skins
            .follow_looks(looks.revision(), skins.generation(), |client| {
                looks
                    .saber_skin_id(client)
                    .and_then(|id| skins.color_with(id, looks.options_off(client)))
            });
        let skin = self.local_saber_skin();
        self.saber_skins.set_local(client, skin);
        if let Some(audio) = game_audio {
            audio.follow_blade_skins(&self.blade_skins);
            audio.set_saber_sound_overrides(&self.saber_skins.sound_sets());
        }
        #[cfg(test)]
        self.submit_shot_blades(presentation_time);
        #[cfg(not(test))]
        let _ = presentation_time;
    }

    /// The world shots' blades, lit, at their chosen time; a skin whose pack is not
    /// loaded is the stock blue.
    #[cfg(test)]
    fn submit_shot_blades(&mut self, presentation_time: i64) {
        let millis = self
            .saber_skins
            .shot_seconds
            .map_or(presentation_time, |seconds| (seconds * 1_000.0) as i64);
        for (blade, color, seed) in self.saber_skins.shot_blades.clone() {
            let persona = self.saber_skins.shot_persona;
            let color = match color {
                ShotColor::Stock(color) => color,
                ShotColor::Skin(id) => self
                    .blade_skins
                    .color_of(id)
                    .map_or(BladeColor::Retail(crate::saber::Color::Blue), |skin| {
                        BladeColor::Skin(SkinColor { persona, ..skin })
                    }),
                ShotColor::Worn(id, stock) => self.blade_skins.color_of(id).map_or(stock, |skin| {
                    BladeColor::Skin(SkinColor { persona, ..skin }.worn_with(stock))
                }),
                ShotColor::Parts(id, off) => self
                    .blade_skins
                    .color_with(id, off)
                    .map_or(BladeColor::Retail(crate::saber::Color::Blue), |skin| {
                        BladeColor::Skin(SkinColor { persona, ..skin })
                    }),
            };
            self.saber_instances.extend(
                crate::saber::Instance::pair(blade, color)
                    .map(|i| i.with_animation(millis as f64 * 0.001, seed)),
            );
            self.saber_instances.extend(crate::saber::skin_extras(
                blade,
                color,
                millis as f64 * 0.001,
                seed,
            ));
            let mut lit = [None; 8];
            lit[0] = Some(blade);
            crate::saber_submission::lights::append(
                &mut self.dynamic_lights,
                &lit,
                color,
                1,
                false,
                millis,
                u64::from(seed),
            );
        }
        let persona = self.saber_skins.shot_persona;
        for (blade, id, fade) in self.saber_skins.shot_ghosts.clone() {
            if let Some(skin) = self.blade_skins.color_of(id) {
                let color = BladeColor::Skin(SkinColor { persona, ..skin });
                self.saber_instances.push(
                    crate::saber::Instance::ghost(blade, color, fade)
                        .with_animation(millis as f64 * 0.001, 2),
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "saber_skins_tests.rs"]
pub(crate) mod tests;
