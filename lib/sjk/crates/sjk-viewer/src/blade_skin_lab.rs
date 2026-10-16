//! The blade-skin file's fifth set of sections (`docs/unlockables.md`, "Blade-skin
//! files"), added 15/10/2026 for the shaders of Creyon's Blade Lab page: bands (stripes
//! or rings round the blade, which may pinch it), segments (a tube in sections that
//! stutter out now and then), a soap film's colours, blobs of wax rising in a glass
//! column, and a front (a colour climbing the blade from the hilt and falling back).
//!
//! A client of the fourth set refuses a field it does not know in the later additions
//! file too, so these live in a third additions file (`<id>.bladeextra3`,
//! [`LAB_ADDITIONS`]), laid over the others: an older client draws the skin without them.
//! As before, a section left out draws nothing and, when present, every field of it is
//! required.

use super::{Rgb, colour, within};
use serde::Deserialize;

/// The third additions file's extension (the fifth set's): `<id>.bladeextra3`.
pub(crate) const LAB_ADDITIONS: &str = ".bladeextra3";
/// Most segments a blade is cut into, and most blobs it holds.
pub(crate) const MAX_SEGMENTS: u32 = 12;
pub(crate) const MAX_BLOBS: u32 = 8;

/// Bands round the blade: stripes of their own colour every `spacing` units, a `width`
/// share of it wide, slanting by `twist` (bands a capsule radius across, so they wind
/// round the blade like a barber's pole) and running `speed` units a second toward the
/// tip. In a band the glow and core take its colour by `mix`, its brightness is
/// `brightness` times theirs, and the glow narrows by `pinch` (rings squeezing it).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Bands {
    pub(crate) color: Rgb,
    pub(crate) brightness: f32,
    pub(crate) spacing: f32,
    pub(crate) width: f32,
    pub(crate) twist: f32,
    pub(crate) speed: f32,
    pub(crate) mix: f32,
    pub(crate) pinch: f32,
}

/// A tube in `count` sections, `gap` units dark between them: `rate` times a second each
/// section draws, and one whose draw is at least `threshold` drops to `1 − depth` of its
/// light for that draw (an old sign's stutter); `hum` is a faint unsteadiness all along.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Segments {
    pub(crate) count: u32,
    pub(crate) rate: f32,
    pub(crate) threshold: f32,
    pub(crate) depth: f32,
    pub(crate) gap: f32,
    pub(crate) hum: f32,
}

/// A soap film: the glow takes by `amount` the colours of a thin film (noise `scale` cells
/// a unit, sliding `speed` times as fast as a slow drift, at `saturation`), and its
/// inside clears by `clear` while its edge brightens, as a bubble is bright at its rim and
/// nearly clear in the middle.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Film {
    pub(crate) amount: f32,
    pub(crate) scale: f32,
    pub(crate) speed: f32,
    pub(crate) saturation: f32,
    pub(crate) clear: f32,
}

/// Blobs of wax in the blade's column: `count` of them, `size` capsule radii across,
/// rising about `speed` units a second (each at its own pace), swaying by `wobble`, and
/// merging where they touch (metaballs); `pools` is the size of the pools at the hilt
/// and the tip they leave and join (0: none).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Blobs {
    pub(crate) color: Rgb,
    pub(crate) brightness: f32,
    pub(crate) count: u32,
    pub(crate) size: f32,
    pub(crate) speed: f32,
    pub(crate) wobble: f32,
    pub(crate) pools: f32,
}

/// A front: once every `period` seconds, for a `duty` share of it, a colour climbs the
/// blade from the hilt up to a `reach` share of it, stays a `hold` share of that time and
/// falls back; below it the glow and core take its colour by `amount`. Its edge is `soft`
/// units wide and wanders by `jag`; `veins` keeps the skin's veins to the part it covers
/// (1: only there), as cracks the colour spreads through.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Front {
    pub(crate) color: Rgb,
    pub(crate) amount: f32,
    pub(crate) period: f32,
    pub(crate) duty: f32,
    pub(crate) hold: f32,
    pub(crate) reach: f32,
    pub(crate) soft: f32,
    pub(crate) jag: f32,
    pub(crate) veins: f32,
}

/// The fifth set's sections, borrowed.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Lab<'a> {
    pub(crate) bands: Option<&'a Bands>,
    pub(crate) segments: Option<&'a Segments>,
    pub(crate) film: Option<&'a Film>,
    pub(crate) blobs: Option<&'a Blobs>,
    pub(crate) front: Option<&'a Front>,
}

impl Lab<'_> {
    /// Each section within its ranges.
    pub(crate) fn check(&self) -> Result<(), String> {
        if let Some(b) = self.bands {
            colour("bands.color", b.color, 4.0)?;
            within("bands.brightness", b.brightness, 0.0, 10.0)?;
            within("bands.spacing", b.spacing, 0.2, 40.0)?;
            within("bands.width", b.width, 0.02, 0.98)?;
            within("bands.twist", b.twist, -4.0, 4.0)?;
            within("bands.speed", b.speed, -100.0, 100.0)?;
            within("bands.mix", b.mix, 0.0, 1.0)?;
            within("bands.pinch", b.pinch, 0.0, 0.8)?;
        }
        if let Some(s) = self.segments {
            if !(2..=MAX_SEGMENTS).contains(&s.count) {
                return Err(format!(
                    "segments.count must be 2 to {MAX_SEGMENTS}, not {}",
                    s.count
                ));
            }
            within("segments.rate", s.rate, 0.0, 60.0)?;
            within("segments.threshold", s.threshold, 0.0, 1.0)?;
            within("segments.depth", s.depth, 0.0, 1.0)?;
            within("segments.gap", s.gap, 0.0, 4.0)?;
            within("segments.hum", s.hum, 0.0, 1.0)?;
        }
        if let Some(f) = self.film {
            within("film.amount", f.amount, 0.0, 1.0)?;
            within("film.scale", f.scale, 0.02, 4.0)?;
            within("film.speed", f.speed, 0.0, 10.0)?;
            within("film.saturation", f.saturation, 0.0, 1.0)?;
            within("film.clear", f.clear, 0.0, 1.0)?;
        }
        if let Some(b) = self.blobs {
            colour("blobs.color", b.color, 4.0)?;
            within("blobs.brightness", b.brightness, 0.0, 10.0)?;
            if !(1..=MAX_BLOBS).contains(&b.count) {
                return Err(format!(
                    "blobs.count must be 1 to {MAX_BLOBS}, not {}",
                    b.count
                ));
            }
            within("blobs.size", b.size, 0.1, 1.5)?;
            within("blobs.speed", b.speed, 0.0, 40.0)?;
            within("blobs.wobble", b.wobble, 0.0, 1.0)?;
            within("blobs.pools", b.pools, 0.0, 2.0)?;
        }
        if let Some(f) = self.front {
            colour("front.color", f.color, 4.0)?;
            within("front.amount", f.amount, 0.0, 1.0)?;
            within("front.period", f.period, 0.5, 60.0)?;
            within("front.duty", f.duty, 0.05, 1.0)?;
            within("front.hold", f.hold, 0.0, 0.9)?;
            within("front.reach", f.reach, 0.05, 1.0)?;
            within("front.soft", f.soft, 0.1, 20.0)?;
            within("front.jag", f.jag, 0.0, 1.0)?;
            within("front.veins", f.veins, 0.0, 1.0)?;
        }
        Ok(())
    }
}

impl Front {
    /// How far up the blade (a share of it, 0 to `reach`) the front is `t` seconds into
    /// a blade's clock, with its `seed` (`saber.wgsl`'s `skin_front_height`).
    pub(crate) fn height(&self, t: f32, seed: f32) -> f32 {
        let p = (t / self.period + seed * 0.37).rem_euclid(1.0);
        if p >= self.duty {
            return 0.0;
        }
        let a = p / self.duty;
        let ramp = ((1.0 - self.hold) * 0.5).max(0.001);
        let rise = smoothstep(0.0, ramp, a);
        let fall = 1.0 - smoothstep(1.0 - ramp, 1.0, a);
        self.reach * rise * fall
    }
}

/// GLSL's smoothstep.
fn smoothstep(low: f32, high: f32, x: f32) -> f32 {
    let t = ((x - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_front_climbs_holds_and_falls_once_a_period() {
        let front = Front {
            color: [1.0, 0.2, 0.1],
            amount: 1.0,
            period: 10.0,
            duty: 0.5,
            hold: 0.4,
            reach: 0.8,
            soft: 2.0,
            jag: 0.0,
            veins: 0.0,
        };
        assert_eq!(front.height(0.0, 0.0), 0.0, "it starts at the hilt");
        assert!(
            (front.height(2.5, 0.0) - 0.8).abs() < 1e-6,
            "held at its reach"
        );
        assert!(front.height(0.75, 0.0) > 0.0 && front.height(0.75, 0.0) < 0.8);
        assert_eq!(front.height(6.0, 0.0), 0.0, "past its duty it is gone");
        assert_eq!(
            front.height(12.5, 0.0),
            front.height(2.5, 0.0),
            "every period"
        );
    }
}
