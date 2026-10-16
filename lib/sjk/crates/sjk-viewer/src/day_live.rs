//! Live hour/rate/clarity snapshots. Resource-enabling controls remain startup policy.
use sjk_shell::{CvarError, CvarRegistry, CvarValue};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

/// Allocation-free presentation controls shared by console callbacks and render upload:
/// solar hour, minutes per day, volumetric clarity, real-time light scale, the
/// diagnostics bitmask, contact shadows, gap closure, ambient readability fill and
/// indirect-light controls.
#[derive(Clone)]
pub(crate) struct Clock(Arc<[AtomicU32; 10]>);

/// Names in slot order; every slot is a live cvar registered by `day::register`.
pub(crate) const LIVE_CVARS: [&str; 10] = [
    "r_dayHour",
    "r_dayMinutes",
    "r_volumetricClarity",
    "r_dayBrightness",
    "r_dayDebug",
    "r_contactShadows",
    "r_sunShadowGapClose",
    "r_ambientFill",
    "r_indirectBoost",
    "r_ambientFillOcclusion",
];

impl Default for Clock {
    fn default() -> Self {
        Self(Arc::new([
            AtomicU32::new(12_f32.to_bits()),
            AtomicU32::new(0),
            AtomicU32::new(1_f32.to_bits()),
            AtomicU32::new(1_f32.to_bits()),
            AtomicU32::new(0),
            AtomicU32::new(0),
            AtomicU32::new(0),
            AtomicU32::new(0.025_f32.to_bits()),
            AtomicU32::new(1_f32.to_bits()),
            AtomicU32::new(1_f32.to_bits()),
        ]))
    }
}

impl Clock {
    /// Seed archived values before subscribing; no callback is needed for the initial state.
    pub(crate) fn bind(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        if cvars.get("r_dayNight").is_none() {
            super::register(cvars)?;
        }
        let clock = Self::default();
        for (index, name) in LIVE_CVARS.into_iter().enumerate() {
            if let Some(cvar) = cvars.get(name) {
                clock.store(index, &cvar.value);
            }
            let changed = clock.clone();
            cvars.on_change(name, move |change| changed.store(index, &change.current))?;
        }
        Ok(clock)
    }

    fn store(&self, index: usize, value: &CvarValue) {
        let value = match value {
            CvarValue::Float(v) => *v as f32,
            CvarValue::Integer(v) => *v as f32,
            _ => return,
        };
        if !value.is_finite() {
            return;
        }
        let value = match index {
            0 => value.rem_euclid(24.),
            1 => value.clamp(0., 1440.),
            2 => value.clamp(0., 1.),
            3 => value.clamp(0.1, 10.),
            6 => value.clamp(0., 64.),
            7 => value.clamp(0., 0.2),
            8 => value.clamp(0., 4.),
            9 => value.clamp(0., 1.),
            _ => value.clamp(0., 4095.),
        };
        self.0[index].store(value.to_bits(), Ordering::Relaxed);
    }

    /// Read the bounded scalar controls without a registry lookup, allocation or lock.
    pub(crate) fn values(&self) -> [f32; 4] {
        [0, 1, 2, 3].map(|i| f32::from_bits(self.0[i].load(Ordering::Relaxed)))
    }

    /// `r_sunShadowGapClose`: holes in the shadow maps narrower than this many world units
    /// are closed.
    pub(crate) fn gap_close(&self) -> f32 {
        f32::from_bits(self.0[6].load(Ordering::Relaxed))
    }

    /// Live readability fill; zero preserves fully unlit rooms.
    pub(crate) fn ambient_fill(&self) -> f32 {
        f32::from_bits(self.0[7].load(Ordering::Relaxed))
    }

    /// Indirect-light gain and the fraction of AO applied to readability fill.
    pub(crate) fn indirect_readability(&self) -> [f32; 2] {
        [8, 9].map(|i| f32::from_bits(self.0[i].load(Ordering::Relaxed)))
    }

    /// `r_dayDebug` plus the contact shadow toggle folded into bit 2: which real-time
    /// terms to leave out (see `day::register`).
    pub(crate) fn debug(&self) -> u32 {
        let bits = f32::from_bits(self.0[4].load(Ordering::Relaxed)) as u32;
        let contact = f32::from_bits(self.0[5].load(Ordering::Relaxed)) != 0.;
        bits | if contact { 0 } else { 2 }
    }
}
