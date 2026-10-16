//! Live material-lighting diagnostics; no texture replacement or pipeline rebuild.
use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry, CvarValue};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use super::material_maps::DEBUG_SHIFT;
/// The bits of the material-map view in the mode word.
const DEBUG_BITS: u32 = 7 << DEBUG_SHIFT;
/// `r_normalMapStrength` in the mode word: bits 16..24 hold `(64·strength + 192) mod
/// 256`, so a word without them (zero) means strength 1. Steps of 1/64 up to 3.98.
pub(crate) const STRENGTH_SHIFT: u32 = 16;
const STRENGTH_BITS: u32 = 255 << STRENGTH_SHIFT;

/// The mode word's bits for normal-map strength `value` (clamped to 0..3.98).
fn strength_bits(value: f64) -> u32 {
    let steps = (value.clamp(0., 255. / 64.) * 64.).round() as u32;
    ((steps + 192) & 255) << STRENGTH_SHIFT
}

/// `r_emissionStrength` in the mode word: bits 24..32 hold `(32·strength + 224) mod 256`,
/// so a word without them means strength 1. Steps of 1/32 up to 7.97.
pub(crate) const EMISSION_SHIFT: u32 = 24;
const EMISSION_BITS: u32 = 255 << EMISSION_SHIFT;
/// Set when `r_emissiveGlow` is 0: emission-mapped surfaces get no dynamic-glow halo.
pub(crate) const NO_EMISSIVE_GLOW: u32 = 1 << 11;

/// The mode word's bits for emission strength `value` (clamped to 0..7.97).
fn emission_bits(value: f64) -> u32 {
    let steps = (value.clamp(0., 255. / 32.) * 32.).round() as u32;
    ((steps + 224) & 255) << EMISSION_SHIFT
}

/// `r_parallaxStrength` in the mode word: bits 2..8 hold `(40·strength + 60) mod 64`, so
/// a word without them means the default 0.1. Steps of 1/40 up to 1.575.
pub(crate) const PARALLAX_SHIFT: u32 = 2;
const PARALLAX_BITS: u32 = 63 << PARALLAX_SHIFT;
/// `r_parallaxStrength` when nothing sets it: a tenth of the pack's depth (Sol's choice,
/// 07/10/2026; the full depth swam on sand and stone).
const PARALLAX_DEFAULT: f64 = 0.1;

/// The mode word's bits for parallax strength `value` (clamped to 0..1.575).
fn parallax_bits(value: f64) -> u32 {
    let steps = (value.clamp(0., 63. / 40.) * 40.).round() as u32;
    ((steps + 60) & 63) << PARALLAX_SHIFT
}

/// `r_parallaxNearDistance` in the mode word: bits 12..16 hold `(distance / 4 + 10) mod
/// 16`, so a word without them means the default 24 units. Steps of 4 units up to 60.
pub(crate) const PARALLAX_NEAR_SHIFT: u32 = 12;
const PARALLAX_NEAR_BITS: u32 = 15 << PARALLAX_NEAR_SHIFT;
/// `r_parallaxNearDistance` when nothing sets it: closer to a surface's plane than this
/// many units, its parallax stops growing on screen (`material_maps.wgsl`). A first-person
/// eye stays 15 units from a wall (the player's half width) and the third-person camera 4.
const PARALLAX_NEAR_DEFAULT: i64 = 24;

/// The mode word's bits for the parallax near distance `value` (clamped to 0..60 units).
fn parallax_near_bits(value: f64) -> u32 {
    let steps = (value.clamp(0., 60.) / 4.).round() as u32;
    ((steps + 10) & 15) << PARALLAX_NEAR_SHIFT
}

/// A number from a cvar value; `fallback` for text.
fn number(value: &CvarValue, fallback: f64) -> f64 {
    match value {
        CvarValue::Float(value) => *value,
        CvarValue::Integer(value) => *value as f64,
        CvarValue::Bool(on) => f64::from(u8::from(*on)),
        CvarValue::Text(_) => fallback,
    }
}

/// Two independent stock controls and the material-map view (`r_materialMapsDebug`,
/// bits [`DEBUG_SHIFT`]..+3) packed into the existing scene-light uniform.
#[derive(Clone, Default)]
pub(crate) struct Settings(Arc<AtomicU32>);

impl Settings {
    /// Follow EternalJK's archived, omit-default flags and seed before subscribing.
    pub(crate) fn bind(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        let flags = CvarFlags::ARCHIVE | CvarFlags::OMIT_DEFAULT;
        cvars.register(CvarDefinition::new(
            "r_fullbright",
            0_i64,
            flags,
            "White lightmaps, vertex bake and model diffuse; live",
        ))?;
        cvars.register(CvarDefinition::new(
            "r_lightmap",
            0_i64,
            flags,
            "Show lightmap bundles without diffuse texture; live",
        ))?;
        // A diagnostic view: never archived, so a restart always shows the real scene.
        cvars.register(CvarDefinition::new(
            "r_materialMapsDebug",
            0_i64,
            CvarFlags::NONE,
            "Material-map surfaces (r_normalMapping/r_specularMapping/r_emissiveMaps): \
             1 mapped normal as colour, 2 tint by maps found (green normal, blue specular, \
             red parallax), 3 normal-map relief x4 on grey, 4 reflection probes alone, \
             5 without reflection probes, 6 emission maps alone, 7 parallax reach (red far \
             limits, green near limit, blue steps); other surfaces unchanged; live",
        ))?;
        cvars.register(CvarDefinition::new(
            "r_normalMapStrength",
            1.0_f64,
            CvarFlags::ARCHIVE,
            "Multiplier on normal-map relief of material-mapped surfaces, 0 flat .. 3.98; \
             1 as authored; live",
        ))?;
        cvars.register(CvarDefinition::new(
            "r_parallaxStrength",
            PARALLAX_DEFAULT,
            CvarFlags::ARCHIVE,
            "Depth of parallax on material-mapped surfaces (r_parallaxMapping), 0 flat .. \
             1.575; 1 the pack's full depth, default 0.1; live",
        ))?;
        cvars.register(CvarDefinition::new(
            "r_parallaxNearDistance",
            PARALLAX_NEAR_DEFAULT,
            CvarFlags::ARCHIVE,
            "Units from a surface inside which its parallax stops growing on screen as the \
             camera closes in, 0 (off) .. 60 in steps of 4; default 24; live",
        ))?;
        cvars.register(CvarDefinition::new(
            "r_emissionStrength",
            1.0_f64,
            CvarFlags::ARCHIVE,
            "Brightness of emission maps (r_emissiveMaps), 0 off .. 7.97; 1 as authored; live",
        ))?;
        cvars.register(CvarDefinition::new(
            "r_emissiveGlow",
            1_i64,
            CvarFlags::ARCHIVE,
            "Dynamic-glow halo around emission-mapped texels (needs r_DynamicGlow 1); live",
        ))?;
        Self::from_registered(cvars)
    }

    fn from_registered(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        let settings = Self::default();
        for (name, bit) in [("r_fullbright", 1), ("r_lightmap", 2)] {
            settings.set(bit, &cvars.get(name).unwrap().value);
            let changed = settings.clone();
            cvars.on_change(name, move |change| changed.set(bit, &change.current))?;
        }
        settings.set_debug(&cvars.get("r_materialMapsDebug").unwrap().value);
        let changed = settings.clone();
        cvars.on_change("r_materialMapsDebug", move |change| {
            changed.set_debug(&change.current)
        })?;
        settings.set_strength(&cvars.get("r_normalMapStrength").unwrap().value);
        let changed = settings.clone();
        cvars.on_change("r_normalMapStrength", move |change| {
            changed.set_strength(&change.current)
        })?;
        settings.set_parallax(&cvars.get("r_parallaxStrength").unwrap().value);
        let changed = settings.clone();
        cvars.on_change("r_parallaxStrength", move |change| {
            changed.set_parallax(&change.current)
        })?;
        settings.set_parallax_near(&cvars.get("r_parallaxNearDistance").unwrap().value);
        let changed = settings.clone();
        cvars.on_change("r_parallaxNearDistance", move |change| {
            changed.set_parallax_near(&change.current)
        })?;
        settings.set_emission(&cvars.get("r_emissionStrength").unwrap().value);
        let changed = settings.clone();
        cvars.on_change("r_emissionStrength", move |change| {
            changed.set_emission(&change.current)
        })?;
        settings.set_emissive_glow(&cvars.get("r_emissiveGlow").unwrap().value);
        let changed = settings.clone();
        cvars.on_change("r_emissiveGlow", move |change| {
            changed.set_emissive_glow(&change.current)
        })?;
        Ok(settings)
    }

    /// `r_emissionStrength`; a non-number leaves strength 1.
    fn set_emission(&self, value: &CvarValue) {
        let bits = emission_bits(number(value, 1.));
        let _ = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |word| {
                Some(word & !EMISSION_BITS | bits)
            });
    }

    /// `r_parallaxStrength`; a non-number leaves the default.
    fn set_parallax(&self, value: &CvarValue) {
        let bits = parallax_bits(number(value, PARALLAX_DEFAULT));
        let _ = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |word| {
                Some(word & !PARALLAX_BITS | bits)
            });
    }

    /// `r_parallaxNearDistance`; a non-number leaves the default.
    fn set_parallax_near(&self, value: &CvarValue) {
        let bits = parallax_near_bits(number(value, PARALLAX_NEAR_DEFAULT as f64));
        let _ = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |word| {
                Some(word & !PARALLAX_NEAR_BITS | bits)
            });
    }

    /// `r_emissiveGlow`: 0 sets [`NO_EMISSIVE_GLOW`].
    fn set_emissive_glow(&self, value: &CvarValue) {
        if number(value, 1.) != 0. {
            self.0.fetch_and(!NO_EMISSIVE_GLOW, Ordering::Relaxed);
        } else {
            self.0.fetch_or(NO_EMISSIVE_GLOW, Ordering::Relaxed);
        }
    }

    /// `r_materialMapsDebug` 0..7; other values show the scene unchanged.
    fn set_debug(&self, value: &CvarValue) {
        let view = match value {
            CvarValue::Integer(value @ 0..=7) => *value as u32,
            _ => 0,
        };
        let _ = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |bits| {
                Some(bits & !DEBUG_BITS | view << DEBUG_SHIFT)
            });
    }

    /// `r_normalMapStrength`; a non-number leaves strength 1.
    fn set_strength(&self, value: &CvarValue) {
        let strength = match value {
            CvarValue::Float(value) => *value,
            CvarValue::Integer(value) => *value as f64,
            _ => 1.,
        };
        let bits = strength_bits(strength);
        let _ = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |word| {
                Some(word & !STRENGTH_BITS | bits)
            });
    }

    fn set(&self, bit: u32, value: &CvarValue) {
        if let CvarValue::Integer(value) = value {
            if *value != 0 {
                self.0.fetch_or(bit, Ordering::Relaxed);
            } else {
                self.0.fetch_and(!bit, Ordering::Relaxed);
            }
            if bit == 2 && *value == 2 {
                crate::log::progress(format_args!(
                    "r_lightmap 2: intensity heatmap unavailable; showing ordinary lightmap"
                ));
            }
        }
    }

    /// Sample once alongside the existing scene-light upload, not per material or fragment.
    pub(crate) fn bits(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_map_view_has_its_own_bits() {
        let mut cvars = CvarRegistry::new();
        let settings = Settings::bind(&mut cvars).expect("registers");
        assert_eq!(settings.bits(), 0);
        cvars.set_text("r_fullbright", "1").expect("set");
        cvars.set_text("r_materialMapsDebug", "2").expect("set");
        assert_eq!(settings.bits(), 1 | 2 << DEBUG_SHIFT);
        // The stock bits stay below the view's: `mode & 3` tests are unaffected.
        assert_eq!(settings.bits() & 3, 1);
        cvars.set_text("r_materialMapsDebug", "3").expect("set");
        assert_eq!(settings.bits() >> DEBUG_SHIFT, 3);
        cvars.set_text("r_materialMapsDebug", "5").expect("set");
        assert_eq!(settings.bits() >> DEBUG_SHIFT, 5);
        // 7, the parallax reach, is the last view the three bits hold.
        cvars.set_text("r_materialMapsDebug", "7").expect("set");
        assert_eq!(settings.bits() >> DEBUG_SHIFT, 7);
        // Out of range shows the scene unchanged rather than another view.
        cvars.set_text("r_materialMapsDebug", "8").expect("set");
        assert_eq!(settings.bits(), 1);
        cvars.set_text("r_fullbright", "0").expect("set");
        cvars.set_text("r_materialMapsDebug", "1").expect("set");
        assert_eq!(settings.bits(), 1 << DEBUG_SHIFT);
    }

    #[test]
    fn normal_strength_one_is_the_empty_word() {
        let mut cvars = CvarRegistry::new();
        let settings = Settings::bind(&mut cvars).expect("registers");
        // The default leaves the word as it was: zero bits decode to strength 1.
        assert_eq!(settings.bits(), 0);
        let decode = |bits: u32| ((((bits >> STRENGTH_SHIFT) + 64) & 255) as f32) / 64.;
        cvars.set_text("r_normalMapStrength", "2").expect("set");
        assert_eq!(decode(settings.bits()), 2.0);
        cvars.set_text("r_normalMapStrength", "0").expect("set");
        assert_eq!(decode(settings.bits()), 0.0);
        cvars.set_text("r_normalMapStrength", "9").expect("set");
        assert_eq!(decode(settings.bits()), 255. / 64.);
        cvars.set_text("r_normalMapStrength", "0.5").expect("set");
        cvars.set_text("r_materialMapsDebug", "2").expect("set");
        assert_eq!(decode(settings.bits()), 0.5);
        assert_eq!(settings.bits() >> DEBUG_SHIFT & 7, 2);
        // The shader decodes the same bits.
        assert!(
            include_str!("material_maps.wgsl")
                .contains("(((point_lights.metadata.z >> 16u) + 64u) & 255u)")
        );
    }

    #[test]
    fn parallax_strength_tenth_is_the_empty_word() {
        let mut cvars = CvarRegistry::new();
        let settings = Settings::bind(&mut cvars).expect("registers");
        assert_eq!(
            cvars.get("r_parallaxStrength").expect("registered").value,
            CvarValue::Float(0.1)
        );
        // The default 0.1 leaves the word empty.
        assert_eq!(settings.bits(), 0);
        let decode = |bits: u32| ((((bits >> PARALLAX_SHIFT) + 4) & 63) as f32) / 40.;
        assert_eq!(decode(0), 0.1);
        cvars.set_text("r_parallaxStrength", "1").expect("set");
        assert_eq!(decode(settings.bits()), 1.0);
        cvars.set_text("r_parallaxStrength", "0").expect("set");
        assert_eq!(decode(settings.bits()), 0.0);
        cvars.set_text("r_parallaxStrength", "5").expect("set");
        assert_eq!(decode(settings.bits()), 63. / 40.);
        cvars.set_text("r_parallaxStrength", "0.25").expect("set");
        // Its bits sit between the stock bits and the debug view, apart from the others.
        cvars.set_text("r_fullbright", "1").expect("set");
        cvars.set_text("r_materialMapsDebug", "2").expect("set");
        cvars.set_text("r_normalMapStrength", "2").expect("set");
        assert_eq!(decode(settings.bits()), 0.25);
        assert_eq!(settings.bits() & 3, 1);
        assert_eq!(settings.bits() >> DEBUG_SHIFT & 7, 2);
        assert_eq!(
            PARALLAX_BITS & (3 | DEBUG_BITS | NO_EMISSIVE_GLOW | STRENGTH_BITS | EMISSION_BITS),
            0
        );
        // The shader decodes the same bits.
        assert!(
            include_str!("material_maps.wgsl").contains("f32((((mode >> 2u) + 4u) & 63u))/40.0")
        );
    }

    #[test]
    fn parallax_near_distance_default_is_the_empty_word() {
        let mut cvars = CvarRegistry::new();
        let settings = Settings::bind(&mut cvars).expect("registers");
        assert_eq!(
            cvars
                .get("r_parallaxNearDistance")
                .expect("registered")
                .value,
            CvarValue::Integer(24)
        );
        // The default 24 units leaves the word empty.
        assert_eq!(settings.bits(), 0);
        let decode = |bits: u32| (((bits >> PARALLAX_NEAR_SHIFT) + 6) & 15) * 4;
        assert_eq!(decode(0), 24);
        cvars.set_text("r_parallaxNearDistance", "0").expect("set");
        assert_eq!(decode(settings.bits()), 0);
        cvars.set_text("r_parallaxNearDistance", "60").expect("set");
        assert_eq!(decode(settings.bits()), 60);
        // Past the range it clamps; between steps it takes the nearest.
        cvars
            .set_text("r_parallaxNearDistance", "500")
            .expect("set");
        assert_eq!(decode(settings.bits()), 60);
        cvars.set_text("r_parallaxNearDistance", "-8").expect("set");
        assert_eq!(decode(settings.bits()), 0);
        cvars.set_text("r_parallaxNearDistance", "17").expect("set");
        assert_eq!(decode(settings.bits()), 16);
        // Its bits sit between the emission-glow bit and the normal strength, apart from
        // every other field.
        cvars.set_text("r_parallaxStrength", "0.5").expect("set");
        cvars.set_text("r_materialMapsDebug", "7").expect("set");
        cvars.set_text("r_normalMapStrength", "2").expect("set");
        cvars.set_text("r_emissionStrength", "3").expect("set");
        cvars.set_text("r_emissiveGlow", "0").expect("set");
        cvars.set_text("r_fullbright", "1").expect("set");
        assert_eq!(decode(settings.bits()), 16);
        assert_eq!(settings.bits() >> DEBUG_SHIFT & 7, 7);
        assert_eq!(
            PARALLAX_NEAR_BITS
                & (3 | PARALLAX_BITS
                    | DEBUG_BITS
                    | NO_EMISSIVE_GLOW
                    | STRENGTH_BITS
                    | EMISSION_BITS),
            0
        );
        cvars.set_text("r_parallaxNearDistance", "24").expect("set");
        assert_eq!(settings.bits() & PARALLAX_NEAR_BITS, 0);
        // The shader decodes the same bits.
        assert!(
            include_str!("material_maps.wgsl").contains("f32((((mode >> 12u) + 6u) & 15u))*4.0")
        );
    }

    #[test]
    fn emission_strength_and_glow_have_their_own_bits() {
        let mut cvars = CvarRegistry::new();
        let settings = Settings::bind(&mut cvars).expect("registers");
        // Defaults (strength 1, halo on) leave the word empty.
        assert_eq!(settings.bits(), 0);
        let decode = |bits: u32| ((((bits >> EMISSION_SHIFT) + 32) & 255) as f32) / 32.;
        cvars.set_text("r_emissionStrength", "2.5").expect("set");
        assert_eq!(decode(settings.bits()), 2.5);
        cvars.set_text("r_emissionStrength", "0").expect("set");
        assert_eq!(decode(settings.bits()), 0.0);
        cvars.set_text("r_emissionStrength", "100").expect("set");
        assert_eq!(decode(settings.bits()), 255. / 32.);
        // Neighbouring fields keep their values.
        cvars.set_text("r_normalMapStrength", "2").expect("set");
        cvars.set_text("r_materialMapsDebug", "6").expect("set");
        assert_eq!(settings.bits() >> DEBUG_SHIFT & 7, 6);
        assert_eq!(decode(settings.bits()), 255. / 32.);
        assert_eq!(settings.bits() & NO_EMISSIVE_GLOW, 0);
        cvars.set_text("r_emissiveGlow", "0").expect("set");
        assert_ne!(settings.bits() & NO_EMISSIVE_GLOW, 0);
        assert_eq!(settings.bits() >> DEBUG_SHIFT & 7, 6);
        cvars.set_text("r_emissiveGlow", "1").expect("set");
        assert_eq!(settings.bits() & NO_EMISSIVE_GLOW, 0);
        // The glow bit sits between the debug view and the normal strength.
        assert_eq!(
            NO_EMISSIVE_GLOW & (DEBUG_BITS | STRENGTH_BITS | EMISSION_BITS),
            0
        );
        // The shader decodes the same bits.
        assert!(
            include_str!("material_maps.wgsl").contains("f32((((mode >> 24u) + 32u) & 255u))/32.0")
        );
    }
}
