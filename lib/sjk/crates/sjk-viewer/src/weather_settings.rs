//! The weather and cloud settings, shared by the console and every installed world.

use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry, CvarValue};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// Weather on (1) or off (0).
pub(crate) const CVAR: &str = "r_weather";
/// Particle count scale.
pub(crate) const DENSITY_CVAR: &str = "r_weatherDensity";
/// How much is drawn: 0 low to 3 ultra ([`Quality`]).
pub(crate) const QUALITY_CVAR: &str = "r_weatherQuality";
/// Weather chosen by the player instead of the map's ([`forced_commands`]).
pub(crate) const FORCE_CVAR: &str = "r_weatherForce";
/// Ground fog: 0 none, 1 the map's, 2 on every map with sky.
pub(crate) const FOG_CVAR: &str = "r_weatherFog";
/// Volumetric clouds over open sky.
pub(crate) const CLOUDS_CVAR: &str = "r_clouds";

/// SJK's default density: twice the reference's particles.
const DEFAULT_DENSITY: f32 = 2.0;
const DEFAULT_QUALITY: u32 = 2;

const ENABLED: usize = 0;
const DENSITY: usize = 1;
const QUALITY: usize = 2;
const FORCE: usize = 3;
const FOG: usize = 4;
const CLOUDS: usize = 5;

/// The highest `r_weatherForce`.
pub(crate) const FORCE_MAX: i64 = 4;

/// What a quality level draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Quality {
    /// Rain splashes on the ground and water.
    pub(crate) splashes: bool,
    /// Splashes per near rain streak.
    pub(crate) splash_share: f32,
    /// Far rain streaks per near one; 0 draws no far layer.
    pub(crate) far_share: f32,
    /// Samples along each pixel's ray through the rain haze and ground fog; 0 keeps the
    /// original's drifting fog sprites instead.
    pub(crate) fog_steps: u32,
    /// Samples through the cloud layer.
    pub(crate) cloud_steps: u32,
    /// What rain does to the surfaces it falls on: 0 nothing, 1 they are wet, 2 water
    /// also runs down slopes and walls, 3 flat ground also gathers puddles.
    pub(crate) wet: u32,
}

impl Quality {
    /// Level 0 (low) to 3 (ultra).
    pub(crate) fn level(level: u32) -> Self {
        match level {
            0 => Self {
                splashes: false,
                splash_share: 0.0,
                far_share: 0.0,
                fog_steps: 0,
                cloud_steps: 10,
                wet: 0,
            },
            1 => Self {
                splashes: true,
                splash_share: 0.4,
                far_share: 0.0,
                fog_steps: 8,
                cloud_steps: 16,
                wet: 1,
            },
            2 => Self {
                splashes: true,
                splash_share: 0.6,
                far_share: 1.5,
                fog_steps: 12,
                cloud_steps: 24,
                wet: 2,
            },
            _ => Self {
                splashes: true,
                splash_share: 0.9,
                far_share: 2.5,
                fog_steps: 20,
                cloud_steps: 40,
                wet: 3,
            },
        }
    }
}

/// The world effect commands `r_weatherForce` stands for; empty for the map's own.
pub(crate) fn forced_commands(force: u32) -> &'static [&'static str] {
    match force {
        1 => &["lightrain"],
        2 => &["rain", "wind"],
        3 => &["heavyrain", "gustingwind"],
        4 => &["snow", "wind"],
        _ => &[],
    }
}

/// Live values; every read is one relaxed load.
#[derive(Clone)]
pub(crate) struct Settings(Arc<[AtomicU32; 6]>);

impl Default for Settings {
    fn default() -> Self {
        Self(Arc::new([
            AtomicU32::new(1),
            AtomicU32::new(DEFAULT_DENSITY.to_bits()),
            AtomicU32::new(DEFAULT_QUALITY),
            AtomicU32::new(0),
            AtomicU32::new(1),
            AtomicU32::new(1),
        ]))
    }
}

impl Settings {
    /// Register the archived cvars and follow their changes.
    pub(crate) fn bind(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        let settings = Self::default();
        let integers: [(&str, i64, i64, i64, usize, &str); 5] = [
            (
                CVAR,
                1,
                0,
                1,
                ENABLED,
                "Rain, snow and mist on maps that have them",
            ),
            (
                QUALITY_CVAR,
                i64::from(DEFAULT_QUALITY),
                0,
                3,
                QUALITY,
                "Weather quality: 0 low, 1 medium, 2 high, 3 ultra",
            ),
            (
                FORCE_CVAR,
                0,
                0,
                FORCE_MAX,
                FORCE,
                "Weather on every map: 0 the map's, 1 drizzle, 2 rain, 3 storm, 4 snow",
            ),
            (
                FOG_CVAR,
                1,
                0,
                2,
                FOG,
                "Ground fog: 0 none, 1 the map's, 2 on every map with sky",
            ),
            (
                CLOUDS_CVAR,
                1,
                0,
                1,
                CLOUDS,
                "Volumetric clouds over open sky",
            ),
        ];
        for (name, default, low, high, slot, help) in integers {
            cvars.register(CvarDefinition::new(name, default, CvarFlags::ARCHIVE, help))?;
            if let Some(cvar) = cvars.get(name) {
                settings.set_integer(slot, &cvar.value, low, high);
            }
            let changed = settings.clone();
            cvars.on_change(name, move |change| {
                changed.set_integer(slot, &change.current, low, high)
            })?;
        }
        cvars.register(CvarDefinition::new(
            DENSITY_CVAR,
            f64::from(DEFAULT_DENSITY),
            CvarFlags::ARCHIVE,
            "Weather particle count, 1 as the original game, 0.25 to 4",
        ))?;
        if let Some(cvar) = cvars.get(DENSITY_CVAR) {
            settings.set_density(&cvar.value);
        }
        let changed = settings.clone();
        cvars.on_change(DENSITY_CVAR, move |change| {
            changed.set_density(&change.current)
        })?;
        Ok(settings)
    }

    fn set_integer(&self, slot: usize, value: &CvarValue, low: i64, high: i64) {
        let value = match value {
            CvarValue::Bool(value) => i64::from(*value),
            CvarValue::Integer(value) => *value,
            CvarValue::Float(value) if value.is_finite() => *value as i64,
            _ => return,
        };
        self.0[slot].store(value.clamp(low, high) as u32, Ordering::Relaxed);
    }

    fn set_density(&self, value: &CvarValue) {
        let value = match value {
            CvarValue::Float(value) => *value,
            CvarValue::Integer(value) => *value as f64,
            _ => return,
        };
        if value.is_finite() {
            let value = value.clamp(0.25, 4.0) as f32;
            self.0[DENSITY].store(value.to_bits(), Ordering::Relaxed);
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.0[ENABLED].load(Ordering::Relaxed) != 0
    }

    pub(crate) fn density(&self) -> f32 {
        f32::from_bits(self.0[DENSITY].load(Ordering::Relaxed))
    }

    pub(crate) fn quality(&self) -> Quality {
        Quality::level(self.0[QUALITY].load(Ordering::Relaxed))
    }

    /// `r_weatherForce`, 0 for the map's own weather.
    pub(crate) fn force(&self) -> u32 {
        self.0[FORCE].load(Ordering::Relaxed)
    }

    /// `r_weatherFog`: 0 none, 1 the map's, 2 always.
    pub(crate) fn fog(&self) -> u32 {
        self.0[FOG].load(Ordering::Relaxed)
    }

    pub(crate) fn clouds(&self) -> bool {
        self.0[CLOUDS].load(Ordering::Relaxed) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_clamped_and_follow_the_console() {
        let mut cvars = CvarRegistry::default();
        let settings = Settings::bind(&mut cvars).unwrap();
        assert!(settings.enabled() && settings.clouds());
        assert_eq!(settings.quality(), Quality::level(2));
        assert_eq!((settings.force(), settings.fog()), (0, 1));
        settings.set_integer(QUALITY, &CvarValue::Integer(9), 0, 3);
        assert_eq!(settings.quality(), Quality::level(3));
        settings.set_integer(FORCE, &CvarValue::Integer(-2), 0, FORCE_MAX);
        assert_eq!(settings.force(), 0);
        settings.set_density(&CvarValue::Float(100.0));
        assert_eq!(settings.density(), 4.0);
    }

    #[test]
    fn quality_adds_features_level_by_level() {
        let levels = [0, 1, 2, 3].map(Quality::level);
        assert!(!levels[0].splashes && levels[0].fog_steps == 0);
        for pair in levels.windows(2) {
            assert!(pair[1].cloud_steps > pair[0].cloud_steps);
            assert!(pair[1].fog_steps >= pair[0].fog_steps);
            assert!(pair[1].far_share >= pair[0].far_share);
            assert!(pair[1].wet > pair[0].wet);
        }
        assert!(forced_commands(0).is_empty());
        for force in 1..=FORCE_MAX as u32 {
            let effects = super::super::effects::Effects::from_commands(
                forced_commands(force).iter().copied(),
            );
            assert_eq!(effects.clouds.len(), 1, "force {force}");
        }
    }
}
