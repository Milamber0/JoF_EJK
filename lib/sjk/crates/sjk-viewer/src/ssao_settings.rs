//! Optional ambient occlusion policy; never inferred from a change callback alone.
use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry, CvarValue};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

/// Shared across world installs; zero restores the authored baked lighting.
#[derive(Clone)]
pub(crate) struct Settings(Arc<(AtomicBool, AtomicU32)>);

pub(super) const DEFAULT_INTENSITY: f32 = 4.0;

impl Default for Settings {
    fn default() -> Self {
        Self(Arc::new((
            AtomicBool::new(true),
            AtomicU32::new(DEFAULT_INTENSITY.to_bits()),
        )))
    }
}

impl Settings {
    /// Register SJK's ambient-occlusion defaults; archived values still take precedence.
    pub(crate) fn bind(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        cvars.register(CvarDefinition::new(
            "r_ssao",
            1_i64,
            CvarFlags::ARCHIVE,
            "Main-view static lightmapped surface SSAO; applies immediately",
        ))?;
        cvars.register(CvarDefinition::new(
            "ssao_intensity",
            f64::from(DEFAULT_INTENSITY),
            CvarFlags::ARCHIVE,
            "SSAO strength 0-4; 1 original, 4 default; live with r_ssao 1",
        ))?;
        Self::from_registered(cvars)
    }

    /// Subscribe after reading the registry, including pre-existing archived values.
    pub(super) fn from_registered(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        let result = Self::default();
        result.set(&cvars.get("r_ssao").unwrap().value);
        result.set_intensity(&cvars.get("ssao_intensity").unwrap().value);
        let intensity_changed = result.clone();
        cvars.on_change("ssao_intensity", move |change| {
            intensity_changed.set_intensity(&change.current)
        })?;
        let changed = result.clone();
        cvars.on_change("r_ssao", move |change| changed.set(&change.current))?;
        Ok(result)
    }

    fn set(&self, value: &CvarValue) {
        if let CvarValue::Integer(value) = value {
            self.0.0.store(*value != 0, Ordering::Relaxed);
        }
    }

    /// One policy read at the main-view boundary, not per surface or fragment.
    pub(crate) fn enabled(&self) -> bool {
        self.0.0.load(Ordering::Relaxed) && self.intensity() > 0.
    }

    fn set_intensity(&self, value: &CvarValue) {
        let value = match value {
            CvarValue::Float(v) => *v,
            CvarValue::Integer(v) => *v as f64,
            _ => return,
        };
        if value.is_finite() {
            self.0
                .1
                .store((value.clamp(0., 4.) as f32).to_bits(), Ordering::Relaxed);
        }
    }

    /// Live multiplier; zero skips SSAO, one restores its original strength.
    pub(crate) fn intensity(&self) -> f32 {
        f32::from_bits(self.0.1.load(Ordering::Relaxed))
    }
}
