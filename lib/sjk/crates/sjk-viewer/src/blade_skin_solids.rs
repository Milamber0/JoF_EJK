//! The blade-skin file's seventh set of sections (`docs/unlockables.md`, "Blade-skin
//! files"), added 15/10/2026 for the last of Creyon's Blade Lab page: solid blades. Every
//! other section adds light; these are drawn after the blades, blended over them, so they
//! can be dark or opaque: `ink` (an outline, a dark body, a glossy streak and a halftone
//! print of the glow: the Comic and Obsidian Edge blades) and `chrome` (a liquid mirror
//! rippling with beads rolling up it: the Quicksilver).
//!
//! A client of the sixth set refuses a field it does not know in its additions files, so
//! these live in a fifth one (`<id>.bladeextra5`, [`SOLIDS_ADDITIONS`]), laid over the
//! others: an older client draws the skin without them. As before, a section left out
//! draws nothing and, when present, every field of it is required.

use super::{Rgb, colour, within};
use serde::Deserialize;

/// The fifth additions file's extension (the seventh set's): `<id>.bladeextra5`.
pub(crate) const SOLIDS_ADDITIONS: &str = ".bladeextra5";

/// Ink over the blade: a body `width` blade radii wide (`body`: how opaque it is, in
/// `color`), an outline round it `edge` radii thick (`outline`: how opaque, in black), a
/// tip drawn to a point over its last `point` units (0: round), a glossy streak sliding
/// along the body (`gloss`), and the glow printed as halftone dots, `halftone` a unit
/// (0: none).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ink {
    pub(crate) color: Rgb,
    pub(crate) body: f32,
    pub(crate) outline: f32,
    pub(crate) width: f32,
    pub(crate) edge: f32,
    pub(crate) point: f32,
    pub(crate) gloss: f32,
    pub(crate) halftone: f32,
}

/// A liquid mirror `width` blade radii wide, tinted `color`, reflecting a bright sky above
/// a dark ground (and the light where the blade is): ripples running up it (`ripple`, how
/// far they swell it), beads swelling near the hilt and rolling to the tip (`beads` of
/// them at once), `speed` units a second.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Chrome {
    pub(crate) color: Rgb,
    pub(crate) width: f32,
    pub(crate) ripple: f32,
    pub(crate) beads: f32,
    pub(crate) speed: f32,
}

/// The seventh set's sections, borrowed.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Solids<'a> {
    pub(crate) ink: Option<&'a Ink>,
    pub(crate) chrome: Option<&'a Chrome>,
}

impl Solids<'_> {
    /// Each section within its ranges.
    pub(crate) fn check(&self) -> Result<(), String> {
        if let Some(i) = self.ink {
            colour("ink.color", i.color, 1.0)?;
            within("ink.body", i.body, 0.0, 1.0)?;
            within("ink.outline", i.outline, 0.0, 1.0)?;
            within("ink.width", i.width, 0.2, 2.0)?;
            within("ink.edge", i.edge, 0.02, 0.6)?;
            within("ink.point", i.point, 0.0, 20.0)?;
            within("ink.gloss", i.gloss, 0.0, 1.0)?;
            within("ink.halftone", i.halftone, 0.0, 4.0)?;
            if i.body <= 0.0 && i.outline <= 0.0 && i.halftone <= 0.0 {
                return Err("ink needs a body, an outline or a halftone".to_owned());
            }
        }
        if let Some(c) = self.chrome {
            colour("chrome.color", c.color, 1.0)?;
            within("chrome.width", c.width, 0.2, 2.0)?;
            within("chrome.ripple", c.ripple, 0.0, 0.5)?;
            within("chrome.beads", c.beads, 0.0, 8.0)?;
            within("chrome.speed", c.speed, 0.0, 40.0)?;
        }
        Ok(())
    }

    /// Whether the solid pass draws anything for this skin.
    pub(crate) fn any(&self) -> bool {
        self.ink.is_some_and(|i| i.body > 0.0 || i.outline > 0.0) || self.chrome.is_some()
    }
}
