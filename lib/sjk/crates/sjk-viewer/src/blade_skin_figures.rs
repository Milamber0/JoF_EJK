//! The blade-skin file's sixth set of sections (`docs/unlockables.md`, "Blade-skin
//! files"), added 15/10/2026 for the rest of Creyon's Blade Lab page: figures drawn in
//! the blade's glow. A pencil sketch, an hourglass with its sand, a serpent coiled round
//! the blade, glass strands fanning from the hilt, butterflies resting on it and flying
//! off, wings of fire, and ghost copies of the blade.
//!
//! A client of the fifth set refuses a field it does not know in its additions files, so
//! these live in a fourth one (`<id>.bladeextra4`, [`FIGURES_ADDITIONS`]), laid over the
//! others: an older client draws the skin without them. As before, a section left out
//! draws nothing and, when present, every field of it is required.

use super::{Rgb, colour, within};
use serde::Deserialize;

/// The fourth additions file's extension (the sixth set's): `<id>.bladeextra4`.
pub(crate) const FIGURES_ADDITIONS: &str = ".bladeextra4";
/// Most strands, butterflies and copies (a side) a blade has, and feathers a wing.
pub(crate) const MAX_STRANDS: u32 = 32;
pub(crate) const MAX_BUTTERFLIES: u32 = 8;
pub(crate) const MAX_COPIES: u32 = 2;
pub(crate) const MAX_FEATHERS: u32 = 9;

/// A pencil sketch of a blade: two wobbly outlines `wobble` capsule radii unsteady and
/// diagonal hatching, `hatch` lines a unit, all redrawn `rate` times a second like a
/// flipbook; the blade's own glow is kept at `glow` of its light.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Sketch {
    pub(crate) color: Rgb,
    pub(crate) brightness: f32,
    pub(crate) rate: f32,
    pub(crate) wobble: f32,
    pub(crate) hatch: f32,
    pub(crate) glow: f32,
}

/// An hourglass: the blade pinched to `waist` of its width at its middle, a glass rim
/// `glass` bright in the corona's hot rim colour, and sand of its own colour pouring
/// from the upper chamber into the lower one over `period` seconds, then a flash and
/// again.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Hourglass {
    pub(crate) color: Rgb,
    pub(crate) brightness: f32,
    pub(crate) period: f32,
    pub(crate) waist: f32,
    pub(crate) glass: f32,
}

/// A serpent coiled round the blade, `coils` turns from hilt to tip, `amplitude` capsule
/// radii out, its body `thickness` radii thick, slithering `speed` turns a second; it
/// passes in front of the blade (hiding it) and behind, its head near the tip opening
/// its jaws now and then, two red eyes.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Serpent {
    pub(crate) color: Rgb,
    pub(crate) brightness: f32,
    pub(crate) coils: f32,
    pub(crate) amplitude: f32,
    pub(crate) speed: f32,
    pub(crate) thickness: f32,
}

/// Glass strands from the hilt: `count` of them fanning out to `spread` capsule radii at
/// the tip, swaying by `sway`, each a faint line ending in a bright point of its own
/// colour (hues from `hue` over a third of the wheel, slowly turning, at `saturation`).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Strands {
    pub(crate) count: u32,
    pub(crate) spread: f32,
    pub(crate) sway: f32,
    pub(crate) brightness: f32,
    pub(crate) hue: f32,
    pub(crate) saturation: f32,
}

/// Butterflies resting on the blade, `size` units across, slowly opening their wings;
/// each now and then (a `flight` share of its cycle) flies off up to `roam` units,
/// flapping `flap` times a second, and lands again.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Butterflies {
    pub(crate) color: Rgb,
    pub(crate) brightness: f32,
    pub(crate) count: u32,
    pub(crate) size: f32,
    pub(crate) flap: f32,
    pub(crate) flight: f32,
    pub(crate) roam: f32,
}

/// Wings of fire spreading from near the hilt, `span` units long, of `feathers` flaming
/// feathers a side, unfolding and folding `rate` times a second.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Wings {
    pub(crate) color: Rgb,
    pub(crate) brightness: f32,
    pub(crate) span: f32,
    pub(crate) rate: f32,
    pub(crate) feathers: u32,
}

/// Ghost copies of the blade's glow at other angles (`count` a side, each `angle`
/// radians further), `fade` of its light and flickering by `flicker`: they drift apart
/// over `period` seconds and collapse into the blade with a flash.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Copies {
    pub(crate) count: u32,
    pub(crate) angle: f32,
    pub(crate) fade: f32,
    pub(crate) period: f32,
    pub(crate) flicker: f32,
}

/// The sixth set's sections, borrowed.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Figures<'a> {
    pub(crate) sketch: Option<&'a Sketch>,
    pub(crate) hourglass: Option<&'a Hourglass>,
    pub(crate) serpent: Option<&'a Serpent>,
    pub(crate) strands: Option<&'a Strands>,
    pub(crate) butterflies: Option<&'a Butterflies>,
    pub(crate) wings: Option<&'a Wings>,
    pub(crate) copies: Option<&'a Copies>,
}

impl Figures<'_> {
    /// Each section within its ranges.
    pub(crate) fn check(&self) -> Result<(), String> {
        if let Some(s) = self.sketch {
            colour("sketch.color", s.color, 4.0)?;
            within("sketch.brightness", s.brightness, 0.0, 10.0)?;
            within("sketch.rate", s.rate, 1.0, 30.0)?;
            within("sketch.wobble", s.wobble, 0.0, 1.0)?;
            within("sketch.hatch", s.hatch, 0.0, 4.0)?;
            within("sketch.glow", s.glow, 0.0, 1.0)?;
        }
        if let Some(h) = self.hourglass {
            colour("hourglass.color", h.color, 4.0)?;
            within("hourglass.brightness", h.brightness, 0.0, 10.0)?;
            within("hourglass.period", h.period, 2.0, 60.0)?;
            within("hourglass.waist", h.waist, 0.05, 1.0)?;
            within("hourglass.glass", h.glass, 0.0, 4.0)?;
        }
        if let Some(s) = self.serpent {
            colour("serpent.color", s.color, 4.0)?;
            within("serpent.brightness", s.brightness, 0.0, 10.0)?;
            within("serpent.coils", s.coils, 1.0, 12.0)?;
            within("serpent.amplitude", s.amplitude, 0.2, 1.5)?;
            within("serpent.speed", s.speed, -4.0, 4.0)?;
            within("serpent.thickness", s.thickness, 0.05, 0.6)?;
        }
        if let Some(s) = self.strands {
            if !(2..=MAX_STRANDS).contains(&s.count) {
                return Err(format!(
                    "strands.count must be 2 to {MAX_STRANDS}, not {}",
                    s.count
                ));
            }
            within("strands.spread", s.spread, 0.0, 1.6)?;
            within("strands.sway", s.sway, 0.0, 1.0)?;
            within("strands.brightness", s.brightness, 0.0, 10.0)?;
            within("strands.hue", s.hue, 0.0, 1.0)?;
            within("strands.saturation", s.saturation, 0.0, 1.0)?;
        }
        if let Some(b) = self.butterflies {
            colour("butterflies.color", b.color, 4.0)?;
            within("butterflies.brightness", b.brightness, 0.0, 10.0)?;
            if !(1..=MAX_BUTTERFLIES).contains(&b.count) {
                return Err(format!(
                    "butterflies.count must be 1 to {MAX_BUTTERFLIES}, not {}",
                    b.count
                ));
            }
            within("butterflies.size", b.size, 0.5, 8.0)?;
            within("butterflies.flap", b.flap, 0.0, 20.0)?;
            within("butterflies.flight", b.flight, 0.0, 1.0)?;
            within("butterflies.roam", b.roam, 0.0, super::extras::MAX_ROOM)?;
        }
        if let Some(w) = self.wings {
            colour("wings.color", w.color, 4.0)?;
            within("wings.brightness", w.brightness, 0.0, 10.0)?;
            within("wings.span", w.span, 2.0, super::extras::MAX_ROOM)?;
            within("wings.rate", w.rate, 0.0, 4.0)?;
            if !(3..=MAX_FEATHERS).contains(&w.feathers) {
                return Err(format!(
                    "wings.feathers must be 3 to {MAX_FEATHERS}, not {}",
                    w.feathers
                ));
            }
        }
        if let Some(c) = self.copies {
            if !(1..=MAX_COPIES).contains(&c.count) {
                return Err(format!(
                    "copies.count must be 1 to {MAX_COPIES}, not {}",
                    c.count
                ));
            }
            within("copies.angle", c.angle, 0.005, 0.1)?;
            within("copies.fade", c.fade, 0.0, 1.0)?;
            within("copies.period", c.period, 0.5, 30.0)?;
            within("copies.flicker", c.flicker, 0.0, 1.0)?;
        }
        Ok(())
    }

    /// How far (units) the figures reach past the glow: the butterflies' flights and the
    /// wings' span, for the sections not in `off`.
    pub(crate) fn room(&self, off: u32) -> f32 {
        use super::extras::Section;
        let butterflies = self
            .butterflies
            .filter(|_| off & Section::Butterflies.bit() == 0)
            .map_or(0.0, |b| b.roam + b.size);
        let wings = self
            .wings
            .filter(|_| off & Section::Wings.bit() == 0)
            .map_or(0.0, |w| w.span);
        butterflies.max(wings).min(super::extras::MAX_ROOM)
    }
}
