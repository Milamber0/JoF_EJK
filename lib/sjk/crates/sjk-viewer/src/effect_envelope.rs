//! Allocation-free evaluation of Raven FX primitive lifetime curves.
//!
//! The weighting follows `CParticle::UpdateSize/UpdateRGB` and
//! `CLight::UpdateSize/UpdateRGB` in
//! `codemp/client/FxPrimitives.cpp:335-497,1529-1673`. Parameter conversion
//! follows `FX_AddLight` in `codemp/client/FxUtil.cpp:883-908`.

use sjk_effect::{Curve, CurveFlags, CurveModifier, Range};

/// One curve whose ranges were sampled when the primitive was spawned.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Envelope {
    start: f32,
    end: f32,
    parameter: f32,
    flags: CurveFlags,
}

impl Envelope {
    pub(crate) fn from_curve(curve: Curve, seed: u32) -> Self {
        Self {
            start: curve.start.sample(random_unit(seed)),
            end: curve.end.sample(random_unit(seed.wrapping_add(1))),
            parameter: curve.parameter.sample(random_unit(seed.wrapping_add(2))),
            flags: curve.flags,
        }
    }

    pub(crate) fn from_ranges(
        start: Range,
        end: Range,
        parameter: Range,
        flags: CurveFlags,
        seed: u32,
    ) -> Self {
        Self {
            start: start.sample(random_unit(seed)),
            end: end.sample(random_unit(seed.wrapping_add(1))),
            parameter: parameter.sample(random_unit(seed.wrapping_add(2))),
            flags,
        }
    }

    pub(crate) fn from_values(start: f32, end: f32, parameter: f32, flags: CurveFlags) -> Self {
        Self {
            start,
            end,
            parameter,
            flags,
        }
    }

    pub(crate) fn sample(self, elapsed_millis: f32, lifetime_millis: f32, seed: u32) -> f32 {
        let life = lifetime_millis.max(f32::EPSILON);
        let progress = (elapsed_millis / life).clamp(0.0, 1.0);
        let mut start_weight = if self.flags.linear {
            1.0 - progress
        } else {
            1.0
        };
        let parameter_progress = (self.parameter * 0.01).clamp(0.0, 1.0);
        let second_weight = match self.flags.modifier {
            CurveModifier::NonLinear if progress > parameter_progress => {
                1.0 - (progress - parameter_progress) / (1.0 - parameter_progress).max(f32::EPSILON)
            }
            CurveModifier::NonLinear => 1.0,
            CurveModifier::Clamp if progress < parameter_progress => {
                (parameter_progress - progress) / parameter_progress.max(f32::EPSILON)
            }
            CurveModifier::Clamp => 0.0,
            CurveModifier::Wave => {
                start_weight *=
                    (elapsed_millis * self.parameter * std::f32::consts::PI * 0.001).cos();
                start_weight
            }
            CurveModifier::None => start_weight,
        };
        if matches!(
            self.flags.modifier,
            CurveModifier::NonLinear | CurveModifier::Clamp
        ) {
            start_weight = if self.flags.linear {
                (start_weight + second_weight) * 0.5
            } else {
                second_weight
            };
        } else if self.flags.modifier == CurveModifier::Wave {
            start_weight = second_weight;
        }
        if self.flags.random {
            start_weight *= random_unit(seed ^ elapsed_millis.to_bits());
        }
        self.start * start_weight + self.end * (1.0 - start_weight)
    }
}

pub(crate) fn random_unit(mut seed: u32) -> f32 {
    seed ^= seed >> 16;
    seed = seed.wrapping_mul(0x7feb_352d);
    seed ^= seed >> 15;
    seed = seed.wrapping_mul(0x846c_a68b);
    seed ^= seed >> 16;
    (seed as f64 / u32::MAX as f64) as f32
}
