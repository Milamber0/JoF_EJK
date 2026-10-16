//! The blade-skin file's later optional sections (`docs/unlockables.md`, "Blade-skin
//! files"), added 10/10/2026 for the second set of saber shaders: a sputtering edge and
//! length, glitching, hologram scan lines, a heartbeat pulse, afterimages, falling embers,
//! glowing veins, a team or surroundings tint and scrolling glyphs. Each is drawn as
//! nothing when a file leaves it out, so earlier files draw as they did; when present,
//! every field of the section is required and checked as [`super::BladeSkinDef`]'s are.

use super::{Rgb, colour, within};
use serde::Deserialize;

/// Most afterimages a blade may leave (`ghosts.count`).
pub(crate) const MAX_GHOSTS: u32 = 4;

/// A ragged, unsteady blade: its edge frayed by noise and, now and then, its length cut
/// short for a moment.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Sputter {
    /// How far the noise frays the glow's edge, a share of its width.
    pub(crate) ragged: f32,
    /// Noise cells a unit along the blade, and how fast it runs (units a second).
    pub(crate) scale: f32,
    pub(crate) speed: f32,
    /// The most of the blade a sputter cuts off, a share of its length.
    pub(crate) cut: f32,
    /// Draws a second; a draw sputters when its random value is at least `threshold`.
    pub(crate) rate: f32,
    pub(crate) threshold: f32,
}

/// A glitching blade: its colours split apart and, on random draws, blocks of it jump
/// sideways and flash.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Glitch {
    /// How far red and blue part from green, in capsule radii (doubled in a glitch).
    pub(crate) split: f32,
    /// A block's length along the blade, units.
    pub(crate) blocks: f32,
    /// Draws a second; a block glitches when its draw is at least `threshold`.
    pub(crate) rate: f32,
    pub(crate) threshold: f32,
    /// How far a glitched block jumps, in capsule radii, and how much it brightens.
    pub(crate) shift: f32,
    pub(crate) flash: f32,
}

/// A hologram: scan lines running along the blade, a wireframe (two side lines and
/// rings round the blade), a hollow inside and a jittering projection.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Scan {
    /// The wireframe's colour and brightness.
    pub(crate) color: Rgb,
    pub(crate) brightness: f32,
    /// Scan lines a unit along, how fast they run (units a second) and how deep they
    /// darken the glow (0 to 1).
    pub(crate) lines: f32,
    pub(crate) speed: f32,
    pub(crate) depth: f32,
    /// Wireframe rings a unit along the blade (0: none).
    pub(crate) rings: f32,
    /// Where the wireframe's side lines run (capsule radii out) and their half-width.
    pub(crate) edge: f32,
    pub(crate) width: f32,
    /// How much the inside of the glow is dimmed (0 to 1).
    pub(crate) hollow: f32,
    /// How far a jitter throws the projection sideways (capsule radii), and draws a
    /// second.
    pub(crate) jitter: f32,
    pub(crate) jitter_rate: f32,
}

/// A heartbeat: a double beat brightening and widening the blade and its light.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Pulse {
    /// Beats a second, and how much a beat brightens (1: twice as bright).
    pub(crate) rate: f32,
    pub(crate) amount: f32,
    /// The second beat's share of the first, and seconds between them.
    pub(crate) second: f32,
    pub(crate) gap: f32,
    /// A beat's half-length, seconds, and how much it widens the glow.
    pub(crate) width: f32,
    pub(crate) swell: f32,
}

/// Afterimages: the blade's glow left behind where it was, fading.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ghosts {
    /// Afterimages, 1 to [`MAX_GHOSTS`].
    pub(crate) count: u32,
    /// Milliseconds between them.
    pub(crate) spacing: f32,
    /// The first one's brightness; each next is this much of the one before.
    pub(crate) fade: f32,
}

/// Embers dripping off the blade and falling, as gravity pulls them, whichever way the
/// blade is held.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Embers {
    pub(crate) color: Rgb,
    pub(crate) brightness: f32,
    /// The share of drip places that drip, and drip places a unit along the blade.
    pub(crate) density: f32,
    pub(crate) cells: f32,
    /// How fast they fall (units a second) and how long one lasts (seconds).
    pub(crate) fall: f32,
    pub(crate) life: f32,
    /// An ember's radius (units), and how much a drip's way strays sideways.
    pub(crate) size: f32,
    pub(crate) spread: f32,
}

/// Glowing veins (cracks) through the blade's inside and core: ridged noise along and
/// across it.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Veins {
    pub(crate) color: Rgb,
    pub(crate) brightness: f32,
    /// Noise cells a unit, how fast they run along (units a second), a vein's half-width
    /// (a share of a cell) and how bright they show in the core.
    pub(crate) scale: f32,
    pub(crate) speed: f32,
    pub(crate) width: f32,
    pub(crate) core: f32,
}

/// The blade takes its wearer's team colour: `red`, `blue`, or `none` outside a team.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Team {
    /// How much of the colour it takes (0 to 1).
    pub(crate) amount: f32,
    pub(crate) red: Rgb,
    pub(crate) blue: Rgb,
    pub(crate) none: Rgb,
}

/// The blade takes the colour of the light where it is (the map's light grid).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ambient {
    /// How much of the colour it takes (0 to 1), how much its saturation is raised (1
    /// keeps it) and the least of each channel (so a grey light still shows a colour).
    pub(crate) amount: f32,
    pub(crate) saturate: f32,
    pub(crate) floor: f32,
}

/// Glyphs scrolling along the blade spelling its wearer's name, each letter drawn as
/// SJK's own stroke glyph.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Glyphs {
    pub(crate) color: Rgb,
    pub(crate) brightness: f32,
    /// A glyph's height along the blade and the gap after it (units), how fast they
    /// scroll toward the tip (units a second) and a stroke's width (a share of a glyph).
    pub(crate) size: f32,
    pub(crate) spacing: f32,
    pub(crate) speed: f32,
    pub(crate) width: f32,
}

/// The later sections of a file, each optional.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Effects<'a> {
    pub(crate) sputter: Option<&'a Sputter>,
    pub(crate) glitch: Option<&'a Glitch>,
    pub(crate) scan: Option<&'a Scan>,
    pub(crate) pulse: Option<&'a Pulse>,
    pub(crate) ghosts: Option<&'a Ghosts>,
    pub(crate) embers: Option<&'a Embers>,
    pub(crate) veins: Option<&'a Veins>,
    pub(crate) team: Option<&'a Team>,
    pub(crate) ambient: Option<&'a Ambient>,
    pub(crate) glyphs: Option<&'a Glyphs>,
}

impl Effects<'_> {
    /// Every present section within its ranges.
    pub(crate) fn check(&self) -> Result<(), String> {
        if let Some(s) = self.sputter {
            within("sputter.ragged", s.ragged, 0.0, 1.0)?;
            within("sputter.scale", s.scale, 0.05, 8.0)?;
            within("sputter.speed", s.speed, -100.0, 100.0)?;
            within("sputter.cut", s.cut, 0.0, 0.5)?;
            within("sputter.rate", s.rate, 0.0, 60.0)?;
            within("sputter.threshold", s.threshold, 0.0, 1.0)?;
        }
        if let Some(g) = self.glitch {
            within("glitch.split", g.split, 0.0, 1.0)?;
            within("glitch.blocks", g.blocks, 0.5, 40.0)?;
            within("glitch.rate", g.rate, 0.05, 60.0)?;
            within("glitch.threshold", g.threshold, 0.0, 1.0)?;
            within("glitch.shift", g.shift, 0.0, 1.0)?;
            within("glitch.flash", g.flash, 0.0, 4.0)?;
        }
        if let Some(s) = self.scan {
            colour("scan.color", s.color, 4.0)?;
            within("scan.brightness", s.brightness, 0.0, 10.0)?;
            within("scan.lines", s.lines, 0.05, 8.0)?;
            within("scan.speed", s.speed, -100.0, 100.0)?;
            within("scan.depth", s.depth, 0.0, 1.0)?;
            within("scan.rings", s.rings, 0.0, 2.0)?;
            within("scan.edge", s.edge, 0.1, 1.5)?;
            within("scan.width", s.width, 0.01, 0.5)?;
            within("scan.hollow", s.hollow, 0.0, 1.0)?;
            within("scan.jitter", s.jitter, 0.0, 0.5)?;
            within("scan.jitter_rate", s.jitter_rate, 0.0, 60.0)?;
        }
        if let Some(p) = self.pulse {
            within("pulse.rate", p.rate, 0.1, 4.0)?;
            within("pulse.amount", p.amount, 0.0, 2.0)?;
            within("pulse.second", p.second, 0.0, 1.0)?;
            within("pulse.gap", p.gap, 0.05, 1.0)?;
            within("pulse.width", p.width, 0.01, 0.5)?;
            within("pulse.swell", p.swell, 0.0, 0.5)?;
        }
        if let Some(g) = self.ghosts {
            if !(1..=MAX_GHOSTS).contains(&g.count) {
                return Err(format!(
                    "ghosts.count must be 1 to {MAX_GHOSTS}, not {}",
                    g.count
                ));
            }
            within("ghosts.spacing", g.spacing, 10.0, 250.0)?;
            within("ghosts.fade", g.fade, 0.0, 1.0)?;
        }
        if let Some(e) = self.embers {
            colour("embers.color", e.color, 4.0)?;
            within("embers.brightness", e.brightness, 0.0, 10.0)?;
            within("embers.density", e.density, 0.0, 1.0)?;
            within("embers.cells", e.cells, 0.05, 4.0)?;
            within("embers.fall", e.fall, 1.0, 400.0)?;
            within("embers.life", e.life, 0.1, 4.0)?;
            within("embers.size", e.size, 0.05, 2.0)?;
            within("embers.spread", e.spread, 0.0, 1.0)?;
        }
        if let Some(v) = self.veins {
            colour("veins.color", v.color, 4.0)?;
            within("veins.brightness", v.brightness, 0.0, 10.0)?;
            within("veins.scale", v.scale, 0.05, 8.0)?;
            within("veins.speed", v.speed, -100.0, 100.0)?;
            within("veins.width", v.width, 0.01, 0.5)?;
            within("veins.core", v.core, 0.0, 10.0)?;
        }
        if let Some(t) = self.team {
            within("team.amount", t.amount, 0.0, 1.0)?;
            colour("team.red", t.red, 4.0)?;
            colour("team.blue", t.blue, 4.0)?;
            colour("team.none", t.none, 4.0)?;
        }
        if let Some(a) = self.ambient {
            if self.team.is_some() {
                return Err("team and ambient cannot both tint a blade".to_owned());
            }
            within("ambient.amount", a.amount, 0.0, 1.0)?;
            within("ambient.saturate", a.saturate, 0.0, 4.0)?;
            within("ambient.floor", a.floor, 0.0, 1.0)?;
        }
        if let Some(g) = self.glyphs {
            colour("glyphs.color", g.color, 4.0)?;
            within("glyphs.brightness", g.brightness, 0.0, 10.0)?;
            within("glyphs.size", g.size, 1.0, 20.0)?;
            within("glyphs.spacing", g.spacing, 0.0, 10.0)?;
            within("glyphs.speed", g.speed, -100.0, 100.0)?;
            within("glyphs.width", g.width, 0.02, 0.3)?;
        }
        Ok(())
    }
}
