//! Retained live soft-particle policy, shared across world installs.
use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry, CvarValue};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Zero restores the original single-pass particle path.
#[derive(Clone)]
pub(crate) struct Settings(Arc<AtomicBool>);

impl Default for Settings {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }
}

impl Settings {
    /// Seed from the registered value before subscribing; do not rely on a change event.
    pub(crate) fn bind(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        cvars.register(CvarDefinition::new(
            "r_softParticles",
            1_i64,
            CvarFlags::ARCHIVE,
            "Depth-softened world particle quads (16 world units); applies immediately",
        ))?;
        Self::from_registered(cvars)
    }

    /// Attach to the actual registry value, including a preconfigured value in tests.
    pub(super) fn from_registered(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        let settings = Self::default();
        settings.set(&cvars.get("r_softParticles").unwrap().value);
        let changed = settings.clone();
        cvars.on_change("r_softParticles", move |change| {
            changed.set(&change.current)
        })?;
        Ok(settings)
    }

    fn set(&self, value: &CvarValue) {
        if let CvarValue::Integer(value) = value {
            self.0.store(*value != 0, Ordering::Relaxed);
        }
    }

    /// One relaxed read per view, never per particle.
    pub(crate) fn enabled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
