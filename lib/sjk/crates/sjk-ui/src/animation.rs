//! Deterministic, allocation-free scalar animation primitives.

/// Interpolation curve used by a [`Tween`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Easing {
    /// Constant-speed interpolation.
    Linear,
    /// A quick response followed by a gentle stop.
    EaseOutCubic,
    /// Symmetric smoothstep interpolation.
    #[default]
    SmoothStep,
}

/// A deterministic scalar transition driven solely by a caller-provided clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    from: f32,
    to: f32,
    start_ms: u64,
    duration_ms: u32,
    easing: Easing,
}

impl Tween {
    /// Create a completed transition at `value`.
    pub const fn settled(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            start_ms: 0,
            duration_ms: 0,
            easing: Easing::Linear,
        }
    }

    /// Create a transition between two values.
    pub const fn new(from: f32, to: f32, start_ms: u64, duration_ms: u32, easing: Easing) -> Self {
        Self {
            from,
            to,
            start_ms,
            duration_ms,
            easing,
        }
    }

    /// Sample the transition at an absolute time in milliseconds.
    pub fn sample(self, time_ms: u64) -> f32 {
        if self.duration_ms == 0 {
            return self.to;
        }
        let elapsed = time_ms.saturating_sub(self.start_ms) as f32;
        let t = (elapsed / self.duration_ms as f32).clamp(0.0, 1.0);
        let eased = match self.easing {
            Easing::Linear => t,
            Easing::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
            Easing::SmoothStep => t * t * (3.0 - 2.0 * t),
        };
        self.from + (self.to - self.from) * eased
    }

    /// Retarget from the value currently visible at `time_ms`.
    pub fn retarget(&mut self, to: f32, time_ms: u64, duration_ms: u32, easing: Easing) {
        let from = self.sample(time_ms);
        *self = Self::new(from, to, time_ms, duration_ms, easing);
    }

    /// Destination value.
    pub const fn target(self) -> f32 {
        self.to
    }

    /// Sample a deterministic repeating pulse whose half-cycle uses
    /// `duration_ms`. This does not retain state or allocate.
    pub fn pulse(low: f32, high: f32, time_ms: u64, duration_ms: u32) -> f32 {
        if duration_ms == 0 {
            return high;
        }
        let phase = (time_ms % (u64::from(duration_ms) * 2)) as f32 / duration_ms as f32;
        let linear = if phase <= 1.0 { phase } else { 2.0 - phase };
        let eased = linear * linear * (3.0 - 2.0 * linear);
        low + (high - low) * eased
    }
}
