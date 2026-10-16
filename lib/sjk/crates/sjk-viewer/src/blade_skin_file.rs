//! Blade-skin files (`docs/unlockables.md`, "Blade-skin files"): the data a blade skin's
//! look and sounds are drawn from. The format is SJK's and open; the files themselves
//! are art, delivered in the SJK hub's packs (`sjk_packs.rs`), not in this repository.
//!
//! `skins/blades/<unlock id>.bladeskin` is a JSON object holding every parameter of the
//! generic blade-skin shading in `saber.wgsl` (colours, corona, granulation, flares,
//! shimmer, flame tongues, the core's rounded tip and, optionally, lightning arcs, drifting
//! motes and a turning hue), the trail and light colours, the light's flicker, the sounds'
//! game paths and, optionally, glow and core images (otherwise the engine generates the
//! grey glow/core pair from the two profiles). Parsing is strict: an unknown field, a
//! missing one or a value out of its range refuses the whole file, and the log names
//! the file and why. A section a skin leaves out is drawn as nothing (no arcs, no motes,
//! no hue turning), so files written before a section existed draw as they did. The
//! sections added after those ([`effects`]: sputter, glitch, scan, pulse, ghosts, embers,
//! veins, team, ambient, glyphs) follow the same rule, and so does the third set
//! ([`extras`]: a tip glint, wisps, heat haze, an echo, and the options a wearer may
//! switch off), read from an additions file older clients never open, and the fifth
//! ([`lab`]: bands, segments, a film, blobs and a front), from a third one, and the
//! sixth ([`figures`]: a sketch, an hourglass, a serpent, strands, butterflies, wings and
//! copies), from a fourth, and the seventh ([`solids`]: ink and chrome, drawn blended over
//! the blade), from a fifth.

use crate::saber_rgb::{CoreProfile, GlowProfile};
use serde::Deserialize;

#[path = "blade_skin_effects.rs"]
pub(crate) mod effects;
#[path = "blade_skin_extras.rs"]
pub(crate) mod extras;
#[path = "blade_skin_figures.rs"]
pub(crate) mod figures;
#[path = "blade_skin_lab.rs"]
pub(crate) mod lab;
#[path = "blade_skin_solids.rs"]
pub(crate) mod solids;

/// Where blade-skin files are.
pub(crate) const FOLDER: &str = "skins/blades";
/// Their extension.
pub(crate) const EXTENSION: &str = ".bladeskin";
/// The one version of the format.
const VERSION: u32 = 1;
/// The longest game path (`MAX_QPATH` less its terminator).
const PATH_MAX: usize = 63;
/// Most flare tracks, shimmer waves and light-flicker waves a skin may have.
pub(crate) const MAX_FLARES: u32 = 8;
const MAX_WAVES: usize = 2;
/// Granulation octaves a skin may have.
const MAX_OCTAVES: usize = 2;
/// Most lightning arcs a skin may have at once (`MAX_ARCS` in `saber.wgsl`).
pub(crate) const MAX_ARCS: u32 = 4;
/// The core's rounded tip when a file does not say (`core.tip`), in core half-widths.
pub(crate) const DEFAULT_TIP: f32 = 1.5;

/// A linear RGB colour.
pub(crate) type Rgb = [f32; 3];

/// One blade skin, as read and checked.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct BladeSkinDef {
    /// The format's version, 1.
    pub(crate) version: u32,
    /// The generated glow image's profile; the neutral RGB pair's when absent.
    #[serde(default = "neutral_glow")]
    pub(crate) glow_profile: GlowProfile,
    /// The generated core image's profile; the neutral RGB pair's when absent.
    #[serde(default = "neutral_core")]
    pub(crate) core_profile: CoreProfile,
    /// A glow image in the same pack (PNG, TGA or JPEG, grey) instead of the profile's.
    #[serde(default)]
    pub(crate) glow_image: Option<String>,
    /// A core image in the same pack (red: hot core, green: fringe) instead of the
    /// profile's.
    #[serde(default)]
    pub(crate) core_image: Option<String>,
    pub(crate) core: Core,
    pub(crate) corona: Corona,
    pub(crate) granulation: Granulation,
    pub(crate) flares: Flares,
    /// Waves of the corona's breathing, at most two.
    #[serde(default)]
    pub(crate) shimmer: Vec<ShimmerWave>,
    pub(crate) tongues: Tongues,
    /// Lightning arcs; none when absent.
    #[serde(default)]
    pub(crate) arcs: Option<Arcs>,
    /// Drifting motes (sparks, specks, shards); none when absent.
    #[serde(default)]
    pub(crate) motes: Option<Motes>,
    /// A hue turning with time, along the blade and outward; none when absent.
    #[serde(default)]
    pub(crate) hue: Option<Hue>,
    /// The later optional sections ([`effects`]); each none when absent.
    #[serde(default)]
    pub(crate) sputter: Option<effects::Sputter>,
    #[serde(default)]
    pub(crate) glitch: Option<effects::Glitch>,
    #[serde(default)]
    pub(crate) scan: Option<effects::Scan>,
    #[serde(default)]
    pub(crate) pulse: Option<effects::Pulse>,
    #[serde(default)]
    pub(crate) ghosts: Option<effects::Ghosts>,
    #[serde(default)]
    pub(crate) embers: Option<effects::Embers>,
    #[serde(default)]
    pub(crate) veins: Option<effects::Veins>,
    #[serde(default)]
    pub(crate) team: Option<effects::Team>,
    #[serde(default)]
    pub(crate) ambient: Option<effects::Ambient>,
    #[serde(default)]
    pub(crate) glyphs: Option<effects::Glyphs>,
    /// The third set ([`extras`]), from the additions file; each none when absent.
    #[serde(default)]
    pub(crate) star: Option<extras::Star>,
    /// The fourth set, from the later additions file: the glint's orbit and black holes;
    /// each none when absent.
    #[serde(default)]
    pub(crate) orbit: Option<extras::Orbit>,
    #[serde(default)]
    pub(crate) holes: Option<extras::Holes>,
    #[serde(default)]
    pub(crate) wisps: Option<extras::Wisps>,
    #[serde(default)]
    pub(crate) haze: Option<extras::Haze>,
    #[serde(default)]
    pub(crate) echo: Option<extras::Echo>,
    /// The fifth set ([`lab`]), from the third additions file; each none when absent.
    #[serde(default)]
    pub(crate) bands: Option<lab::Bands>,
    #[serde(default)]
    pub(crate) segments: Option<lab::Segments>,
    #[serde(default)]
    pub(crate) film: Option<lab::Film>,
    #[serde(default)]
    pub(crate) blobs: Option<lab::Blobs>,
    #[serde(default)]
    pub(crate) front: Option<lab::Front>,
    /// The sixth set ([`figures`]), from the fourth additions file; each none when absent.
    #[serde(default)]
    pub(crate) sketch: Option<figures::Sketch>,
    #[serde(default)]
    pub(crate) hourglass: Option<figures::Hourglass>,
    #[serde(default)]
    pub(crate) serpent: Option<figures::Serpent>,
    #[serde(default)]
    pub(crate) strands: Option<figures::Strands>,
    #[serde(default)]
    pub(crate) butterflies: Option<figures::Butterflies>,
    #[serde(default)]
    pub(crate) wings: Option<figures::Wings>,
    #[serde(default)]
    pub(crate) copies: Option<figures::Copies>,
    /// The seventh set ([`solids`]), from the fifth additions file; each none when absent.
    #[serde(default)]
    pub(crate) ink: Option<solids::Ink>,
    #[serde(default)]
    pub(crate) chrome: Option<solids::Chrome>,
    /// The parts its wearer may switch off; none when absent.
    #[serde(default)]
    pub(crate) options: Vec<extras::SkinOption>,
    /// The blur trail's vertex colour.
    pub(crate) trail: Rgb,
    pub(crate) light: Light,
    pub(crate) sounds: Sounds,
}

fn neutral_glow() -> GlowProfile {
    GlowProfile::NEUTRAL
}

fn neutral_core() -> CoreProfile {
    CoreProfile::NEUTRAL
}

/// The core line: a hot centre (the core image's red) and a fringe (its green) heated
/// by the granulation and the flares.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Core {
    pub(crate) white: Rgb,
    /// How much a flare brightens the hot centre.
    pub(crate) white_flare: f32,
    pub(crate) fringe_cool: Rgb,
    pub(crate) fringe_hot: Rgb,
    /// Where the fringe sits between cool and hot: `base + grain × granulation`.
    pub(crate) fringe_heat: BaseGrain,
    /// The fringe's brightness: `base + grain × granulation + flare × flare`.
    pub(crate) fringe_brightness: BaseGrainFlare,
    /// The core's width breathing across.
    pub(crate) breathe: ShimmerWave,
    /// How long the core's rounded tip is, in core half-widths: over that length the
    /// line narrows on a quarter circle to a point ([`DEFAULT_TIP`] when absent).
    #[serde(default = "default_tip")]
    pub(crate) tip: f32,
}

fn default_tip() -> f32 {
    DEFAULT_TIP
}

/// The glow capsule: a gradient from the rim to the inside, widened by shimmer,
/// granulation and flares up to `reach` times the stock capsule.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Corona {
    /// The rim's colour, cool and heated.
    pub(crate) rim_cool: Rgb,
    pub(crate) rim_hot: Rgb,
    /// The colour inside, by the core.
    pub(crate) inner: Rgb,
    /// How far out (in capsule radii) the inside colour reaches.
    pub(crate) inner_width: f32,
    /// How the granulation and the flares heat the rim.
    pub(crate) rim_heat: GrainFlare,
    /// How much of the inside colour shows: `(base + grain × granulation) × inside² +
    /// flare × flare`.
    pub(crate) inner_mix: BaseGrainFlare,
    /// The glow's brightness: `base + grain × granulation + flare × flare`.
    pub(crate) brightness: BaseGrainFlare,
    /// Widest the glow may get, in stock capsule widths (1 to 2).
    pub(crate) reach: f32,
    /// How much the granulation and a flare widen the glow.
    pub(crate) swell: GrainFlare,
}

/// Value noise drifting along the blade.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Granulation {
    /// One or two octaves, summed by weight.
    pub(crate) octaves: Vec<Octave>,
    /// The sum's contrast: smoothstep from `low` to `high`.
    pub(crate) low: f32,
    pub(crate) high: f32,
}

/// One octave of granulation: noise at `(along × scale − time × speed, time × evolve +
/// seed × offset)`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Octave {
    pub(crate) scale: f32,
    pub(crate) speed: f32,
    pub(crate) evolve: f32,
    pub(crate) offset: f32,
    pub(crate) weight: f32,
}

/// Bright knots running from the hilt to the tip, one track each.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Flares {
    /// Tracks, 0 to 8.
    pub(crate) count: u32,
    /// Cycles a second of track `n`: `rate + rate_step × n + rate_seed × seed`.
    pub(crate) rate: f32,
    pub(crate) rate_step: f32,
    pub(crate) rate_seed: f32,
    /// Phase offsets per blade seed and per track.
    pub(crate) phase_seed: f32,
    pub(crate) phase_step: f32,
    /// A cycle is lit when its random draw in [0, 1] is at least this.
    pub(crate) threshold: f32,
    /// How far (units) a flare starts before the hilt and ends past the tip.
    pub(crate) overshoot: f32,
    /// Its length along the blade (units): `size + size_jitter × random`.
    pub(crate) size: f32,
    pub(crate) size_jitter: f32,
}

/// `amount × sin(time × rate + along × along + seed × seed)`.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShimmerWave {
    pub(crate) amount: f32,
    pub(crate) rate: f32,
    pub(crate) along: f32,
    pub(crate) seed: f32,
}

/// Flame tongues licking out of the corona's edge.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Tongues {
    /// Noise at `(along × along + seed × offset, out × out − time × speed)`.
    pub(crate) along: f32,
    pub(crate) offset: f32,
    pub(crate) out: f32,
    pub(crate) speed: f32,
    /// Where they start and are full, in capsule radii out.
    pub(crate) edge_low: f32,
    pub(crate) edge_high: f32,
    /// Their brightness: `low + range × noise`.
    pub(crate) low: f32,
    pub(crate) range: f32,
}

/// Lightning arcs: jagged filaments struck again and again at random places, leaving the
/// blade, bulging out to one side and coming back, or leaping from the tip into the air.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Arcs {
    /// Arcs at once, 0 to [`MAX_ARCS`].
    pub(crate) count: u32,
    pub(crate) color: Rgb,
    pub(crate) brightness: f32,
    /// The filament's half-width (units) and its soft halo's share (four widths wide).
    pub(crate) width: f32,
    pub(crate) halo: f32,
    /// How far an arc bulges out, and how far its corners zigzag, in capsule radii.
    pub(crate) reach: f32,
    pub(crate) jag: f32,
    /// Zigzag corners a unit along the blade.
    pub(crate) kinks: f32,
    /// A strike's length, a share of the blade between `low` and `high`.
    pub(crate) span: Span,
    /// Strikes a second, per arc.
    pub(crate) rate: f32,
    /// Times a second a strike changes its shape (the crackle; 0 keeps it).
    pub(crate) jitter: f32,
    /// A strike shows when its random draw in [0, 1] is at least this.
    pub(crate) threshold: f32,
    /// The share of strikes that leap from the tip into the air.
    pub(crate) tip: f32,
    /// How fast a strike crawls along the blade, units a second (negative: hiltward).
    pub(crate) crawl: f32,
    /// How fast a strike fades: its light is `(1 − age)^decay` (0 holds it).
    pub(crate) decay: f32,
}

/// `low` to `high`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Span {
    pub(crate) low: f32,
    pub(crate) high: f32,
}

/// Motes: small specks in a field of cells round the blade, drifting with it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Motes {
    pub(crate) color: Rgb,
    pub(crate) brightness: f32,
    /// The share of cells holding a mote.
    pub(crate) density: f32,
    /// Cells a unit along the blade and a capsule radius out.
    pub(crate) cells: f32,
    pub(crate) rings: f32,
    /// A mote's half-width, a share of its cell, and how many times longer it is along
    /// the blade.
    pub(crate) size: f32,
    pub(crate) stretch: f32,
    pub(crate) drift: Drift,
    /// About how many times a second a mote twinkles (0: steady).
    pub(crate) twinkle: f32,
    /// Where (capsule radii out) motes appear and where they are gone.
    pub(crate) inner: f32,
    pub(crate) outer: f32,
    /// 0: all along the blade; 1: only toward and round the tip.
    pub(crate) focus: f32,
}

/// How the motes' field moves: `along` units a second toward the tip (negative: toward
/// the hilt), `out` capsule radii a second outward (negative: inward).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Drift {
    pub(crate) along: f32,
    pub(crate) out: f32,
}

/// The glow's and the core fringe's hue, turned round the grey axis by `rate` turns a
/// second, `along` turns a unit along the blade and `out` turns a capsule radius out.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Hue {
    pub(crate) rate: f32,
    pub(crate) along: f32,
    pub(crate) out: f32,
}

/// The blade's dynamic light.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Light {
    pub(crate) color: Rgb,
    pub(crate) flicker: Flicker,
}

/// The light's brightness: `1 − amount + amount × Σ weight × sin(t × rate + phase ×
/// the blade's phase)`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Flicker {
    pub(crate) amount: f32,
    #[serde(default)]
    pub(crate) waves: Vec<FlickerWave>,
}

/// One wave of the light's flicker.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct FlickerWave {
    pub(crate) rate: f32,
    pub(crate) weight: f32,
    pub(crate) phase: f32,
}

/// The game paths of the skin's sounds, replacing the stock saber sounds.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Sounds {
    pub(crate) on: String,
    pub(crate) off: String,
    pub(crate) hum: String,
    pub(crate) swings: [String; 3],
}

/// `base + grain × granulation`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct BaseGrain {
    pub(crate) base: f32,
    pub(crate) grain: f32,
}

/// `grain × granulation + flare × flare`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct GrainFlare {
    pub(crate) grain: f32,
    pub(crate) flare: f32,
}

/// `base + grain × granulation + flare × flare`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct BaseGrainFlare {
    pub(crate) base: f32,
    pub(crate) grain: f32,
    pub(crate) flare: f32,
}

/// Whether `id` may name an unlock: 1 to 32 of `a` to `z`, `0` to `9` and `_`.
pub(crate) fn valid_id(id: &str) -> bool {
    crate::emotes::valid_id(id)
}

/// Read one blade-skin file: `id` from its name, `text` its contents.
pub(crate) fn parse(id: &str, text: &str) -> Result<BladeSkinDef, String> {
    if !valid_id(id) {
        return Err(format!(
            "{id:?} is not an unlock id (1 to 32 of a to z, 0 to 9 and _)"
        ));
    }
    let def: BladeSkinDef = serde_json::from_str(text).map_err(|error| error.to_string())?;
    def.check()?;
    Ok(def)
}

/// `value`, named `field`, within `low..=high`.
fn within(field: &str, value: f32, low: f32, high: f32) -> Result<(), String> {
    if value.is_finite() && (low..=high).contains(&value) {
        Ok(())
    } else {
        Err(format!("{field} must be {low} to {high}, not {value}"))
    }
}

fn colour(field: &str, rgb: Rgb, high: f32) -> Result<(), String> {
    for (channel, value) in ["red", "green", "blue"].into_iter().zip(rgb) {
        within(&format!("{field} {channel}"), value, 0.0, high)?;
    }
    Ok(())
}

/// A game path in the pack: relative, `/`-separated, no `..`, at most 63 characters.
fn game_path(field: &str, path: &str) -> Result<(), String> {
    let fine = !path.is_empty()
        && path.len() <= PATH_MAX
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path.split('/').all(|part| !part.is_empty() && part != "..");
    if fine {
        Ok(())
    } else {
        Err(format!(
            "{field} must be a game path of 1 to {PATH_MAX} characters, not {path:?}"
        ))
    }
}

/// A coefficient of the shading: any reasonable number.
fn factor(field: &str, value: f32) -> Result<(), String> {
    within(field, value, -1_000.0, 1_000.0)
}

fn wave(field: &str, wave: &ShimmerWave) -> Result<(), String> {
    within(&format!("{field}.amount"), wave.amount, 0.0, 0.5)?;
    factor(&format!("{field}.rate"), wave.rate)?;
    factor(&format!("{field}.along"), wave.along)?;
    factor(&format!("{field}.seed"), wave.seed)
}

impl BladeSkinDef {
    /// Every value within its range.
    fn check(&self) -> Result<(), String> {
        if self.version != VERSION {
            return Err(format!("version must be {VERSION}, not {}", self.version));
        }
        let glow = &self.glow_profile;
        within("glow_profile.width", glow.width, 0.05, 1.0)?;
        within("glow_profile.peak", glow.peak, 0.0, 1.0)?;
        within("glow_profile.tail_width", glow.tail_width, 0.05, 2.0)?;
        within("glow_profile.tail_peak", glow.tail_peak, 0.0, 1.0)?;
        let core = &self.core_profile;
        within("core_profile.width", core.width, 0.05, 1.0)?;
        within("core_profile.fringe_width", core.fringe_width, 0.05, 1.5)?;
        for (field, path) in [
            ("glow_image", &self.glow_image),
            ("core_image", &self.core_image),
        ] {
            if let Some(path) = path {
                game_path(field, path)?;
            }
        }
        let c = &self.core;
        colour("core.white", c.white, 4.0)?;
        within("core.white_flare", c.white_flare, 0.0, 10.0)?;
        colour("core.fringe_cool", c.fringe_cool, 4.0)?;
        colour("core.fringe_hot", c.fringe_hot, 4.0)?;
        factor("core.fringe_heat.base", c.fringe_heat.base)?;
        factor("core.fringe_heat.grain", c.fringe_heat.grain)?;
        within(
            "core.fringe_brightness.base",
            c.fringe_brightness.base,
            0.0,
            10.0,
        )?;
        factor("core.fringe_brightness.grain", c.fringe_brightness.grain)?;
        factor("core.fringe_brightness.flare", c.fringe_brightness.flare)?;
        wave("core.breathe", &c.breathe)?;
        within("core.tip", c.tip, 0.5, 8.0)?;
        let r = &self.corona;
        colour("corona.rim_cool", r.rim_cool, 4.0)?;
        colour("corona.rim_hot", r.rim_hot, 4.0)?;
        colour("corona.inner", r.inner, 4.0)?;
        within("corona.inner_width", r.inner_width, 0.01, 1.0)?;
        factor("corona.rim_heat.grain", r.rim_heat.grain)?;
        factor("corona.rim_heat.flare", r.rim_heat.flare)?;
        factor("corona.inner_mix.base", r.inner_mix.base)?;
        factor("corona.inner_mix.grain", r.inner_mix.grain)?;
        factor("corona.inner_mix.flare", r.inner_mix.flare)?;
        within("corona.brightness.base", r.brightness.base, 0.0, 10.0)?;
        factor("corona.brightness.grain", r.brightness.grain)?;
        factor("corona.brightness.flare", r.brightness.flare)?;
        within(
            "corona.reach",
            r.reach,
            1.0,
            crate::saber_skins::MAX_GLOW_REACH,
        )?;
        within("corona.swell.grain", r.swell.grain, -2.0, 2.0)?;
        within("corona.swell.flare", r.swell.flare, -2.0, 2.0)?;
        let g = &self.granulation;
        if !(1..=MAX_OCTAVES).contains(&g.octaves.len()) {
            return Err(format!(
                "granulation.octaves must hold 1 to {MAX_OCTAVES} octaves, not {}",
                g.octaves.len()
            ));
        }
        for (index, octave) in g.octaves.iter().enumerate() {
            let field = |name: &str| format!("granulation.octaves[{index}].{name}");
            within(&field("scale"), octave.scale, 0.0, 100.0)?;
            factor(&field("speed"), octave.speed)?;
            factor(&field("evolve"), octave.evolve)?;
            factor(&field("offset"), octave.offset)?;
            within(&field("weight"), octave.weight, 0.0, 1.0)?;
        }
        within("granulation.low", g.low, 0.0, 1.0)?;
        within("granulation.high", g.high, 0.0, 1.0)?;
        if g.low >= g.high {
            return Err("granulation.low must be below granulation.high".to_owned());
        }
        let f = &self.flares;
        if f.count > MAX_FLARES {
            return Err(format!(
                "flares.count must be 0 to {MAX_FLARES}, not {}",
                f.count
            ));
        }
        within("flares.rate", f.rate, 0.0, 100.0)?;
        factor("flares.rate_step", f.rate_step)?;
        factor("flares.rate_seed", f.rate_seed)?;
        factor("flares.phase_seed", f.phase_seed)?;
        factor("flares.phase_step", f.phase_step)?;
        within("flares.threshold", f.threshold, 0.0, 1.0)?;
        within("flares.overshoot", f.overshoot, 0.0, 100.0)?;
        within("flares.size", f.size, 0.1, 100.0)?;
        within("flares.size_jitter", f.size_jitter, 0.0, 100.0)?;
        if self.shimmer.len() > MAX_WAVES {
            return Err(format!(
                "shimmer must hold at most {MAX_WAVES} waves, not {}",
                self.shimmer.len()
            ));
        }
        for (index, shimmer) in self.shimmer.iter().enumerate() {
            wave(&format!("shimmer[{index}]"), shimmer)?;
        }
        let t = &self.tongues;
        factor("tongues.along", t.along)?;
        factor("tongues.offset", t.offset)?;
        factor("tongues.out", t.out)?;
        factor("tongues.speed", t.speed)?;
        within("tongues.edge_low", t.edge_low, 0.0, 2.0)?;
        within("tongues.edge_high", t.edge_high, 0.0, 2.0)?;
        if t.edge_low >= t.edge_high {
            return Err("tongues.edge_low must be below tongues.edge_high".to_owned());
        }
        within("tongues.low", t.low, 0.0, 10.0)?;
        factor("tongues.range", t.range)?;
        if let Some(arcs) = &self.arcs {
            arcs.check()?;
        }
        if let Some(motes) = &self.motes {
            motes.check()?;
        }
        if let Some(hue) = &self.hue {
            within("hue.rate", hue.rate, -10.0, 10.0)?;
            within("hue.along", hue.along, -1.0, 1.0)?;
            within("hue.out", hue.out, -4.0, 4.0)?;
        }
        self.effects().check()?;
        self.check_extras()?;
        self.lab().check()?;
        self.figures().check()?;
        self.solids().check()?;
        colour("trail", self.trail, 1.0)?;
        colour("light.color", self.light.color, 4.0)?;
        within("light.flicker.amount", self.light.flicker.amount, 0.0, 1.0)?;
        if self.light.flicker.waves.len() > MAX_WAVES {
            return Err(format!(
                "light.flicker.waves must hold at most {MAX_WAVES} waves, not {}",
                self.light.flicker.waves.len()
            ));
        }
        for (index, wave) in self.light.flicker.waves.iter().enumerate() {
            let field = |name: &str| format!("light.flicker.waves[{index}].{name}");
            within(&field("rate"), wave.rate, 0.0, 100.0)?;
            within(&field("weight"), wave.weight, 0.0, 1.0)?;
            factor(&field("phase"), wave.phase)?;
        }
        let s = &self.sounds;
        game_path("sounds.on", &s.on)?;
        game_path("sounds.off", &s.off)?;
        game_path("sounds.hum", &s.hum)?;
        for (index, swing) in s.swings.iter().enumerate() {
            game_path(&format!("sounds.swings[{index}]"), swing)?;
        }
        Ok(())
    }
}

impl BladeSkinDef {
    /// The later optional sections, borrowed.
    pub(crate) fn effects(&self) -> effects::Effects<'_> {
        effects::Effects {
            sputter: self.sputter.as_ref(),
            glitch: self.glitch.as_ref(),
            scan: self.scan.as_ref(),
            pulse: self.pulse.as_ref(),
            ghosts: self.ghosts.as_ref(),
            embers: self.embers.as_ref(),
            veins: self.veins.as_ref(),
            team: self.team.as_ref(),
            ambient: self.ambient.as_ref(),
            glyphs: self.glyphs.as_ref(),
        }
    }
}

impl BladeSkinDef {
    /// The seventh set's sections, borrowed.
    pub(crate) fn solids(&self) -> solids::Solids<'_> {
        solids::Solids {
            ink: self.ink.as_ref(),
            chrome: self.chrome.as_ref(),
        }
    }

    /// The sixth set's sections, borrowed.
    pub(crate) fn figures(&self) -> figures::Figures<'_> {
        figures::Figures {
            sketch: self.sketch.as_ref(),
            hourglass: self.hourglass.as_ref(),
            serpent: self.serpent.as_ref(),
            strands: self.strands.as_ref(),
            butterflies: self.butterflies.as_ref(),
            wings: self.wings.as_ref(),
            copies: self.copies.as_ref(),
        }
    }

    /// The fifth set's sections, borrowed.
    pub(crate) fn lab(&self) -> lab::Lab<'_> {
        lab::Lab {
            bands: self.bands.as_ref(),
            segments: self.segments.as_ref(),
            film: self.film.as_ref(),
            blobs: self.blobs.as_ref(),
            front: self.front.as_ref(),
        }
    }
}

impl Arcs {
    fn check(&self) -> Result<(), String> {
        if self.count > MAX_ARCS {
            return Err(format!(
                "arcs.count must be 0 to {MAX_ARCS}, not {}",
                self.count
            ));
        }
        colour("arcs.color", self.color, 4.0)?;
        within("arcs.brightness", self.brightness, 0.0, 10.0)?;
        within("arcs.width", self.width, 0.02, 1.0)?;
        within("arcs.halo", self.halo, 0.0, 2.0)?;
        within("arcs.reach", self.reach, 0.0, 2.0)?;
        within("arcs.jag", self.jag, 0.0, 1.0)?;
        within("arcs.kinks", self.kinks, 0.05, 4.0)?;
        within("arcs.span.low", self.span.low, 0.05, 1.0)?;
        within("arcs.span.high", self.span.high, 0.05, 1.0)?;
        if self.span.low > self.span.high {
            return Err("arcs.span.low must be at most arcs.span.high".to_owned());
        }
        within("arcs.rate", self.rate, 0.05, 60.0)?;
        within("arcs.jitter", self.jitter, 0.0, 120.0)?;
        within("arcs.threshold", self.threshold, 0.0, 1.0)?;
        within("arcs.tip", self.tip, 0.0, 1.0)?;
        within("arcs.crawl", self.crawl, -100.0, 100.0)?;
        within("arcs.decay", self.decay, 0.0, 8.0)
    }
}

impl Motes {
    fn check(&self) -> Result<(), String> {
        colour("motes.color", self.color, 4.0)?;
        within("motes.brightness", self.brightness, 0.0, 10.0)?;
        within("motes.density", self.density, 0.0, 1.0)?;
        within("motes.cells", self.cells, 0.05, 8.0)?;
        within("motes.rings", self.rings, 0.5, 16.0)?;
        within("motes.size", self.size, 0.02, 0.5)?;
        within("motes.stretch", self.stretch, 1.0, 8.0)?;
        if self.size * self.stretch > 0.5 {
            return Err(format!(
                "motes.size × motes.stretch must be at most 0.5 (a mote stays in its cell), not {}",
                self.size * self.stretch
            ));
        }
        within("motes.drift.along", self.drift.along, -100.0, 100.0)?;
        within("motes.drift.out", self.drift.out, -10.0, 10.0)?;
        within("motes.twinkle", self.twinkle, 0.0, 60.0)?;
        within("motes.inner", self.inner, 0.0, 2.0)?;
        within("motes.outer", self.outer, 0.0, 2.5)?;
        if self.inner >= self.outer {
            return Err("motes.inner must be below motes.outer".to_owned());
        }
        within("motes.focus", self.focus, 0.0, 1.0)
    }
}

/// Every `skins/blades/<id>.bladeskin` in `vfs` that reads, by id, with its additions
/// file (`<id>.bladeextra`, [`extras::ADDITIONS`]), its later one (`<id>.bladeextra2`,
/// [`extras::LATER_ADDITIONS`]), its third (`<id>.bladeextra3`, [`lab::LAB_ADDITIONS`])
/// its fourth (`<id>.bladeextra4`, [`figures::FIGURES_ADDITIONS`]) and its fifth
/// (`<id>.bladeextra5`, [`solids::SOLIDS_ADDITIONS`]) laid over it in turn when there are
/// some; the ones that do not read are reported to
/// the log by name.
pub(crate) fn load(vfs: &sjk_vfs::VirtualFileSystem) -> Vec<(String, BladeSkinDef)> {
    let mut skins = Vec::new();
    let text_of = |path: &str| {
        vfs.read(path)
            .map_err(|error| error.to_string())
            .and_then(|asset| asset.ok_or_else(|| "missing".to_owned()))
            .and_then(|asset| String::from_utf8(asset.bytes).map_err(|_| "not UTF-8".to_owned()))
    };
    for listed in vfs.list_files(FOLDER, EXTENSION) {
        let path = format!("{FOLDER}/{listed}");
        let id = listed.trim_end_matches(EXTENSION);
        // The additions files, laid over the skin file in turn when there are some.
        let lay_over = |text: String, extension: &str| {
            let additions = format!("{FOLDER}/{id}{extension}");
            match vfs.read(&additions) {
                Ok(Some(_)) => extras::merged(&text, &text_of(&additions)?),
                _ => Ok(text),
            }
        };
        let read = text_of(&path)
            .and_then(|text| lay_over(text, extras::ADDITIONS))
            .and_then(|text| lay_over(text, extras::LATER_ADDITIONS))
            .and_then(|text| lay_over(text, lab::LAB_ADDITIONS))
            .and_then(|text| lay_over(text, figures::FIGURES_ADDITIONS))
            .and_then(|text| lay_over(text, solids::SOLIDS_ADDITIONS))
            .and_then(|text| parse(id, &text));
        match read {
            Ok(def) => skins.push((id.to_owned(), def)),
            Err(why) => crate::log::progress(format_args!("blade skins: {path}: {why}")),
        }
    }
    skins.sort_by(|a, b| a.0.cmp(&b.0));
    skins.dedup_by(|a, b| a.0 == b.0);
    skins
}

#[cfg(test)]
#[path = "blade_skin_file_tests.rs"]
pub(crate) mod tests;
