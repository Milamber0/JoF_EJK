//! Optional spatial fragment sampling of the existing map light grid.

use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry, CvarValue};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Retained spatial model diffuse policy; zero restores legacy entity-origin lighting.
#[derive(Clone, Default)]
pub(crate) struct Settings(Arc<AtomicBool>);

impl Settings {
    /// Register and seed from the actual registry before subscribing.
    pub(crate) fn bind(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        // Default on: spatial grid sampling makes lighting vary across a body, which single-point
        // sampling cannot. Measured 0.018 ms with three characters at 2560x1080 on an
        // RX 9060 XT (0.0427 off against 0.0610 on). `r_modelPixelLight 0` opts out.
        cvars.register(CvarDefinition::new("r_modelPixelLight", 1_i64, CvarFlags::ARCHIVE,
            "Sample model light grid at each pixel's world position (0 legacy); applies immediately"))?;
        Self::from_registered(cvars)
    }

    fn from_registered(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        let settings = Self::default();
        settings.set(&cvars.get("r_modelPixelLight").unwrap().value);
        let changed = settings.clone();
        cvars.on_change("r_modelPixelLight", move |change| {
            changed.set(&change.current)
        })?;
        Ok(settings)
    }

    fn set(&self, value: &CvarValue) {
        if let CvarValue::Integer(value) = value {
            self.0.store(*value != 0, Ordering::Relaxed);
        }
    }

    /// Read once at the existing scene-light upload, never once per pixel or actor.
    pub(crate) fn enabled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
