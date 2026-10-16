//! The blade-skin file's third set of sections (`docs/unlockables.md`, "Blade-skin
//! files"), added 11/10/2026 for the Mythical shaders: a glint at the tip, wisps rising
//! off the blade, heat haze bending what is behind it and an echo of the blade that sways
//! beside it even when it is held still; and the options, the parts of a skin its wearer
//! may switch off ([`SkinOption`]), which every other player then draws the same way.
//!
//! An older client refuses a file holding a field it does not know, so these sections live
//! in an additions file beside the skin's (`<id>.bladeextra`, [`ADDITIONS`]): its fields
//! are laid over the skin file's before it is read, and an older client never opens it
//! (`blade_skin_file::load`). As before, a section left out draws nothing and, when
//! present, every field of it is required.
//!
//! The fourth set (11/10/2026) is the glint's `orbit`, lower on the blade and circling it,
//! and `holes`, small black holes drawn in the glints' places. A client of the third set
//! refuses a field it does not know there too, so it lives in a later additions file
//! (`<id>.bladeextra2`, [`LATER_ADDITIONS`]), laid over the first: a client of the third
//! set draws the glint at the tip as before, and no black hole.

use super::{BladeSkinDef, Rgb, colour, within};
use serde::Deserialize;

/// The additions file's extension: `skins/blades/<id>.bladeextra`.
pub(crate) const ADDITIONS: &str = ".bladeextra";
/// The later additions file's (the fourth set's), laid over the first: `<id>.bladeextra2`.
pub(crate) const LATER_ADDITIONS: &str = ".bladeextra2";
/// Most glints an orbit has.
pub(crate) const MAX_GLINTS: u32 = 4;
/// Most options a skin may offer, and most sections one switches.
pub(crate) const MAX_OPTIONS: usize = 6;
const MAX_OPTION_SECTIONS: usize = 4;
/// The longest option id and option name.
const OPTION_ID_MAX: usize = 24;
const OPTION_NAME_MAX: usize = 24;
/// Most rays a glint has.
pub(crate) const MAX_RAYS: u32 = 8;
/// Farthest (units) wisps rise or haze reaches past the glow: the quad's room
/// ([`super::BladeSkinDef::room`]) and the instances' bounds allow for it.
pub(crate) const MAX_ROOM: f32 = 16.0;

/// A glint at the tip: a star of rays turning slowly and twinkling, with a round glow at
/// its heart, facing the camera.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Star {
    pub(crate) color: Rgb,
    pub(crate) brightness: f32,
    /// Its rays (2 to [`MAX_RAYS`]), how far they reach from the tip (units) and how
    /// thick they are (a share of their length).
    pub(crate) rays: u32,
    pub(crate) length: f32,
    pub(crate) width: f32,
    /// Turns a second (negative: the other way).
    pub(crate) spin: f32,
    /// About how many times a second it twinkles, and how deep (0: steady).
    pub(crate) twinkle: f32,
    pub(crate) depth: f32,
    /// The round glow at its heart, a share of the rays' brightness.
    pub(crate) halo: f32,
}

/// The glint's (or the black holes') orbit (the fourth set): `count` of them `at` units
/// below the tip, circling
/// the blade `radius` units out, `rate` turns a second, their circle tilted by `tilt`
/// (radians: the near side higher) so it shows as a ring round the blade. Every other glint
/// is smaller, and one behind the blade dims. Without it the one stays at the tip.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Orbit {
    pub(crate) at: f32,
    /// 1 to [`MAX_GLINTS`], evenly round the circle.
    pub(crate) count: u32,
    pub(crate) radius: f32,
    pub(crate) rate: f32,
    pub(crate) tilt: f32,
}

/// Black holes (the fourth set), in the glints' places (one at the tip, or circling with
/// `orbit`): a black disc `size` units across its horizon, bending the scene behind it
/// round itself (`lens`, the ring the background is drawn into, in horizon radii), with a
/// thin bright photon ring and a fainter accretion swirl turning `spin` turns a second.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Holes {
    pub(crate) color: Rgb,
    pub(crate) brightness: f32,
    pub(crate) size: f32,
    pub(crate) lens: f32,
    pub(crate) spin: f32,
}

impl Holes {
    /// How far (units) a hole's quad reaches from its heart: its swirl and its lens.
    pub(crate) fn reach(&self) -> f32 {
        self.size * (2.5 + 2.0 * self.lens)
    }
}

/// Wisps: thin smoke curling off the blade and rising, whichever way the blade is held.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Wisps {
    pub(crate) color: Rgb,
    pub(crate) brightness: f32,
    /// How high (units) the smoke rises off the blade before it is gone, and how fast
    /// (units a second).
    pub(crate) rise: f32,
    pub(crate) speed: f32,
    /// Noise cells a unit, how much the smoke curls (the noise's warp) and how much of
    /// it there is (0 to 1).
    pub(crate) scale: f32,
    pub(crate) curl: f32,
    pub(crate) density: f32,
}

/// Heat haze: what is behind the blade shimmers, the air bending as it rises.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Haze {
    /// How far (units) the picture is bent at most.
    pub(crate) strength: f32,
    /// Noise cells a unit, and how fast the shimmer rises (units a second).
    pub(crate) scale: f32,
    pub(crate) speed: f32,
    /// How far out it reaches, units past the glow's edge.
    pub(crate) reach: f32,
}

/// An echo: one faint copy of the blade's glow beside it, swaying slowly, there even while
/// the blade is still (afterimages, `ghosts`, only show while it moves).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Echo {
    /// Its brightness, how far (units) it sways from the blade, sways a second and how
    /// far it leans from the blade's direction (radians).
    pub(crate) fade: f32,
    pub(crate) sway: f32,
    pub(crate) rate: f32,
    pub(crate) lean: f32,
}

/// A part of a skin its wearer may switch off: one or more of its sections.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SkinOption {
    /// What the looks carry (`saber_off`): 1 to 24 of `a` to `z`, `0` to `9` and `_`.
    pub(crate) id: String,
    /// What the Collection shows: 1 to 24 printable ASCII characters.
    pub(crate) name: String,
    /// The sections it switches ([`Section::name`]), each in the file. Every option is
    /// on until its wearer switches it off: a wearer whose client knows no options sends
    /// none, and is drawn with every part.
    pub(crate) sections: Vec<String>,
}

/// The sections an option may switch, a bit each in an instance's mask (`saber.wgsl`
/// `OFF_*`, the same order).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Section {
    Arcs,
    Motes,
    Hue,
    Sputter,
    Glitch,
    Scan,
    Pulse,
    Ghosts,
    Embers,
    Veins,
    Team,
    Ambient,
    Glyphs,
    Star,
    Wisps,
    Haze,
    Echo,
    Holes,
    Bands,
    Segments,
    Film,
    Blobs,
    Front,
    Sketch,
    Hourglass,
    Serpent,
    Strands,
    Butterflies,
    Wings,
    Copies,
    Ink,
    Chrome,
}

impl Section {
    /// Thirty-two, the instance's mask's bits: a later section needs a wider mask.
    pub(crate) const ALL: [Self; 32] = [
        Self::Arcs,
        Self::Motes,
        Self::Hue,
        Self::Sputter,
        Self::Glitch,
        Self::Scan,
        Self::Pulse,
        Self::Ghosts,
        Self::Embers,
        Self::Veins,
        Self::Team,
        Self::Ambient,
        Self::Glyphs,
        Self::Star,
        Self::Wisps,
        Self::Haze,
        Self::Echo,
        Self::Holes,
        Self::Bands,
        Self::Segments,
        Self::Film,
        Self::Blobs,
        Self::Front,
        Self::Sketch,
        Self::Hourglass,
        Self::Serpent,
        Self::Strands,
        Self::Butterflies,
        Self::Wings,
        Self::Copies,
        Self::Ink,
        Self::Chrome,
    ];

    /// Its field's name in the file.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Arcs => "arcs",
            Self::Motes => "motes",
            Self::Hue => "hue",
            Self::Sputter => "sputter",
            Self::Glitch => "glitch",
            Self::Scan => "scan",
            Self::Pulse => "pulse",
            Self::Ghosts => "ghosts",
            Self::Embers => "embers",
            Self::Veins => "veins",
            Self::Team => "team",
            Self::Ambient => "ambient",
            Self::Glyphs => "glyphs",
            Self::Star => "star",
            Self::Wisps => "wisps",
            Self::Haze => "haze",
            Self::Echo => "echo",
            Self::Holes => "holes",
            Self::Bands => "bands",
            Self::Segments => "segments",
            Self::Film => "film",
            Self::Blobs => "blobs",
            Self::Front => "front",
            Self::Sketch => "sketch",
            Self::Hourglass => "hourglass",
            Self::Serpent => "serpent",
            Self::Strands => "strands",
            Self::Butterflies => "butterflies",
            Self::Wings => "wings",
            Self::Copies => "copies",
            Self::Ink => "ink",
            Self::Chrome => "chrome",
        }
    }

    /// Its bit in an instance's mask of sections switched off.
    pub(crate) const fn bit(self) -> u32 {
        1 << self as u32
    }

    pub(crate) fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|section| section.name() == name)
    }
}

/// Option `id`'s bit in a wearer's set of options switched off
/// ([`crate::saber_skin_options::OptionsOff`]): FNV-1a of the id, one of 64. A skin's
/// options never share one ([`BladeSkinDef::check_extras`]).
pub(crate) fn option_bit(id: &str) -> u64 {
    let hash = id.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    1 << (hash % 64)
}

/// Whether `id` may name an option.
pub(crate) fn valid_option_id(id: &str) -> bool {
    (1..=OPTION_ID_MAX).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

impl BladeSkinDef {
    /// Whether the file holds `section`.
    pub(crate) fn has(&self, section: Section) -> bool {
        match section {
            Section::Arcs => self.arcs.is_some(),
            Section::Motes => self.motes.is_some(),
            Section::Hue => self.hue.is_some(),
            Section::Sputter => self.sputter.is_some(),
            Section::Glitch => self.glitch.is_some(),
            Section::Scan => self.scan.is_some(),
            Section::Pulse => self.pulse.is_some(),
            Section::Ghosts => self.ghosts.is_some(),
            Section::Embers => self.embers.is_some(),
            Section::Veins => self.veins.is_some(),
            Section::Team => self.team.is_some(),
            Section::Ambient => self.ambient.is_some(),
            Section::Glyphs => self.glyphs.is_some(),
            Section::Star => self.star.is_some(),
            Section::Wisps => self.wisps.is_some(),
            Section::Haze => self.haze.is_some(),
            Section::Echo => self.echo.is_some(),
            Section::Holes => self.holes.is_some(),
            Section::Bands => self.bands.is_some(),
            Section::Segments => self.segments.is_some(),
            Section::Film => self.film.is_some(),
            Section::Blobs => self.blobs.is_some(),
            Section::Front => self.front.is_some(),
            Section::Sketch => self.sketch.is_some(),
            Section::Hourglass => self.hourglass.is_some(),
            Section::Serpent => self.serpent.is_some(),
            Section::Strands => self.strands.is_some(),
            Section::Butterflies => self.butterflies.is_some(),
            Section::Wings => self.wings.is_some(),
            Section::Copies => self.copies.is_some(),
            Section::Ink => self.ink.is_some(),
            Section::Chrome => self.chrome.is_some(),
        }
    }

    /// The mask of sections its wearer has switched off: those of every option in `off`
    /// (ids the file does not offer are ignored, so an older pack draws a newer wearer's
    /// choice as best it can).
    pub(crate) fn off_mask(&self, off: crate::saber_skin_options::OptionsOff) -> u32 {
        self.options
            .iter()
            .filter(|option| off & option_bit(&option.id) != 0)
            .flat_map(|option| &option.sections)
            .filter_map(|name| Section::named(name))
            .fold(0, |mask, section| mask | section.bit())
    }

    /// How far (units) the glow's quad reaches past the glow for the sections not in
    /// `off`: the wisps' rise, the haze's reach, the butterflies' flights and the wings'
    /// span.
    pub(crate) fn room(&self, off: u32) -> f32 {
        let wisps = self
            .wisps
            .filter(|_| off & Section::Wisps.bit() == 0)
            .map_or(0.0, |wisps| wisps.rise);
        let haze = self
            .haze
            .filter(|_| off & Section::Haze.bit() == 0)
            .map_or(0.0, |haze| haze.reach);
        wisps.max(haze).max(self.figures().room(off))
    }

    /// The third set's sections and the options within their ranges.
    pub(super) fn check_extras(&self) -> Result<(), String> {
        if let Some(s) = &self.star {
            colour("star.color", s.color, 4.0)?;
            within("star.brightness", s.brightness, 0.0, 10.0)?;
            if !(2..=MAX_RAYS).contains(&s.rays) {
                return Err(format!("star.rays must be 2 to {MAX_RAYS}, not {}", s.rays));
            }
            within("star.length", s.length, 1.0, 24.0)?;
            within("star.width", s.width, 0.01, 0.3)?;
            within("star.spin", s.spin, -4.0, 4.0)?;
            within("star.twinkle", s.twinkle, 0.0, 20.0)?;
            within("star.depth", s.depth, 0.0, 1.0)?;
            within("star.halo", s.halo, 0.0, 2.0)?;
        }
        if let Some(h) = &self.holes {
            if self.star.is_some() {
                return Err("star and holes may not both be given".to_owned());
            }
            colour("holes.color", h.color, 4.0)?;
            within("holes.brightness", h.brightness, 0.0, 10.0)?;
            within("holes.size", h.size, 0.2, 4.0)?;
            within("holes.lens", h.lens, 0.0, 3.0)?;
            within("holes.spin", h.spin, -4.0, 4.0)?;
        }
        if let Some(o) = &self.orbit {
            if self.star.is_none() && self.holes.is_none() {
                return Err("orbit needs a star or holes to circle the blade".to_owned());
            }
            within("orbit.at", o.at, 0.0, 40.0)?;
            if !(1..=MAX_GLINTS).contains(&o.count) {
                return Err(format!(
                    "orbit.count must be 1 to {MAX_GLINTS}, not {}",
                    o.count
                ));
            }
            within("orbit.radius", o.radius, 0.0, 16.0)?;
            within("orbit.rate", o.rate, -4.0, 4.0)?;
            within("orbit.tilt", o.tilt, 0.0, 1.0)?;
        }
        if let Some(w) = &self.wisps {
            colour("wisps.color", w.color, 4.0)?;
            within("wisps.brightness", w.brightness, 0.0, 10.0)?;
            within("wisps.rise", w.rise, 1.0, MAX_ROOM)?;
            within("wisps.speed", w.speed, 0.0, 60.0)?;
            within("wisps.scale", w.scale, 0.02, 2.0)?;
            within("wisps.curl", w.curl, 0.0, 2.0)?;
            within("wisps.density", w.density, 0.0, 1.0)?;
        }
        if let Some(h) = &self.haze {
            within("haze.strength", h.strength, 0.0, 2.0)?;
            within("haze.scale", h.scale, 0.05, 4.0)?;
            within("haze.speed", h.speed, 0.0, 60.0)?;
            within("haze.reach", h.reach, 0.5, MAX_ROOM)?;
        }
        if let Some(e) = &self.echo {
            within("echo.fade", e.fade, 0.0, 1.0)?;
            within("echo.sway", e.sway, 0.0, 6.0)?;
            within("echo.rate", e.rate, 0.02, 4.0)?;
            within("echo.lean", e.lean, 0.0, 0.3)?;
        }
        if self.options.len() > MAX_OPTIONS {
            return Err(format!(
                "options must hold at most {MAX_OPTIONS}, not {}",
                self.options.len()
            ));
        }
        let mut switched = 0_u32;
        for (index, option) in self.options.iter().enumerate() {
            let field = format!("options[{index}]");
            if !valid_option_id(&option.id) {
                return Err(format!(
                    "{field}.id must be 1 to {OPTION_ID_MAX} of a to z, 0 to 9 and _, not {:?}",
                    option.id
                ));
            }
            if self.options[..index].iter().any(|o| o.id == option.id) {
                return Err(format!("{field}.id {:?} is given twice", option.id));
            }
            if let Some(other) = self.options[..index]
                .iter()
                .find(|o| option_bit(&o.id) == option_bit(&option.id))
            {
                return Err(format!(
                    "{field}.id {:?} shares its bit with {:?}: rename one",
                    option.id, other.id
                ));
            }
            let printable = option.name.bytes().all(|b| (b' '..=b'~').contains(&b));
            if !(1..=OPTION_NAME_MAX).contains(&option.name.len()) || !printable {
                return Err(format!(
                    "{field}.name must be 1 to {OPTION_NAME_MAX} printable ASCII characters, not {:?}",
                    option.name
                ));
            }
            if !(1..=MAX_OPTION_SECTIONS).contains(&option.sections.len()) {
                return Err(format!(
                    "{field}.sections must name 1 to {MAX_OPTION_SECTIONS} sections"
                ));
            }
            for name in &option.sections {
                let Some(section) = Section::named(name) else {
                    return Err(format!("{field}.sections: {name:?} is not a section"));
                };
                if !self.has(section) {
                    return Err(format!("{field}.sections: the file has no {name}"));
                }
                if switched & section.bit() != 0 {
                    return Err(format!(
                        "{field}.sections: {name} is already another option's"
                    ));
                }
                switched |= section.bit();
            }
        }
        Ok(())
    }
}

/// Lay an additions file's fields (`additions`, a JSON object) over the skin file's
/// (`text`): a field in both is the addition's (a section replaced whole). Either one not
/// an object is refused.
pub(crate) fn merged(text: &str, additions: &str) -> Result<String, String> {
    let mut base: serde_json::Value =
        serde_json::from_str(text).map_err(|error| error.to_string())?;
    let more: serde_json::Value =
        serde_json::from_str(additions).map_err(|error| format!("additions file: {error}"))?;
    let (Some(base_fields), serde_json::Value::Object(more)) = (base.as_object_mut(), more) else {
        return Err("the skin and its additions file must both be objects".to_owned());
    };
    for (key, value) in more {
        base_fields.insert(key, value);
    }
    Ok(base.to_string())
}
