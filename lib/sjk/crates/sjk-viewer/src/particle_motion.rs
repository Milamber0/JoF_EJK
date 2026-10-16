//! Allocation-free Raven `CParticle` motion and rotation state.
//!
//! `CParticle::UpdateOrigin` updates velocity before position
//! (`codemp/client/FxPrimitives.cpp:216-225`). Rotation follows
//! `CParticle::UpdateRotation` (`FxPrimitives.cpp:592-596`), including its
//! frame-time damping of angular velocity.

use super::*;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Motion {
    origin: Vec3,
    velocity: Vec3,
    acceleration: Vec3,
    rotation: f32,
    rotation_delta: f32,

    updated_at: Instant,
}

impl Motion {
    pub(crate) fn new(
        origin: Vec3,
        velocity: Vec3,
        acceleration: Vec3,
        rotation: f32,
        rotation_delta: f32,
        starts_at: Instant,
    ) -> Self {
        Self {
            origin,
            velocity,
            acceleration,
            rotation,
            rotation_delta,

            updated_at: starts_at,
        }
    }

    pub(crate) fn advance(&mut self, now: Instant) -> Sample {
        if let Some(step) = self.begin_step(now) {
            self.origin = step.predicted_origin;
        }
        self.sample()
    }

    pub(crate) fn begin_step(&mut self, now: Instant) -> Option<Step> {
        if now <= self.updated_at {
            return None;
        }
        let previous_origin = self.origin;
        let elapsed = now.duration_since(self.updated_at);
        let seconds = elapsed.as_secs_f32();
        let frame_millis = seconds * 1_000.0;
        self.velocity += self.acceleration * seconds;
        let predicted_origin = previous_origin + self.velocity * seconds;
        self.rotation += frame_millis * 0.01 * self.rotation_delta;
        self.rotation_delta *= 1.0 - frame_millis * 0.000_7;
        self.updated_at = now;
        Some(Step {
            previous_origin,
            predicted_origin,
            seconds,
        })
    }

    pub(crate) fn commit_origin(&mut self, origin: Vec3) {
        self.origin = origin;
    }

    pub(crate) fn add_acceleration_step(&mut self, seconds: f32) {
        self.velocity += self.acceleration * seconds;
    }

    pub(crate) fn reflect(&mut self, normal: Vec3, elasticity: f32) {
        self.velocity = (self.velocity - 2.0 * self.velocity.dot(normal) * normal) * elasticity;
    }

    pub(crate) fn stop_linear_motion(&mut self) {
        self.velocity = Vec3::ZERO;
        self.acceleration = Vec3::ZERO;
    }

    pub(crate) fn sample(&self) -> Sample {
        Sample {
            origin: self.origin,
            velocity: self.velocity,
            rotation_degrees: self.rotation,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Step {
    pub(crate) previous_origin: Vec3,
    pub(crate) predicted_origin: Vec3,
    pub(crate) seconds: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sample {
    pub(crate) origin: Vec3,
    pub(crate) velocity: Vec3,
    pub(crate) rotation_degrees: f32,
}
