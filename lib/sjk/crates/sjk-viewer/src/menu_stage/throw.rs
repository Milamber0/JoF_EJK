//! The Saber tab's throw: the stage model launches its primary saber the
//! way a thrown saber leaves the hand in game (`codemp/game/w_saber.c`
//! `saberFirstThrown` setup: 400 units/s along the view, spinning 800°/s
//! about the vertical, `BOTH_SABERPULL` held while it is out — `bg_saber.c`
//! `PM_WeaponLightsaber`), flies to the shot's focus point, and settles
//! there upright on a slow turntable so the hilt can be looked at. Leaving
//! the tab flies it back into the hand at the same speed.

use crate::audio::ui_cues::{self, Cue};
use glam::{Quat, Vec3};
use std::time::Instant;

/// Thrown saber speed, units per second (`VectorScale(dir, 400, trDelta)`).
const THROW_SPEED: f32 = 400.0;
/// Spin about the vertical while in flight, degrees per second
/// (`apos.trDelta[1] = 800`).
const THROW_SPIN: f32 = 800.0;
/// Seconds the spin takes to ease down and the hilt to tilt upright once
/// it reaches the focus.
const SETTLE_SECONDS: f32 = 0.6;
/// Turntable spin while floating, degrees per second.
const FLOAT_SPIN: f32 = 30.0;
/// Seconds a floating hilt takes to slide to a moved focus (a second hilt
/// joining it shifts both sideways).
const SLIDE_SECONDS: f32 = 0.4;

/// World pose of a hilt: its grip and orientation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Pose {
    pub(super) grip: Vec3,
    pub(super) rotation: Quat,
}

/// Where the primary saber is relative to the hand.
#[derive(Clone, Copy, Debug, Default)]
pub(super) enum Throw {
    #[default]
    Held,
    /// Flying to the focus, then floating there.
    Out {
        started: Instant,
        launch: Pose,
        target: Target,
    },
    /// Flying back into the hand.
    Back {
        started: Instant,
        from: Pose,
        seconds: f32,
    },
}

/// The spot a thrown hilt floats on, easing over when the focus moves.
#[derive(Clone, Copy, Debug)]
pub(super) struct Target {
    point: Vec3,
    from: Vec3,
    moved: Instant,
}

impl Target {
    fn new(point: Vec3, now: Instant) -> Self {
        Self {
            point,
            from: point,
            moved: now,
        }
    }

    fn at(&self, now: Instant) -> Vec3 {
        let elapsed = now.saturating_duration_since(self.moved).as_secs_f32();
        self.from
            .lerp(self.point, smoothstep(elapsed / SLIDE_SECONDS))
    }

    fn retarget(&mut self, point: Vec3, now: Instant) {
        if point != self.point {
            self.from = self.at(now);
            self.point = point;
            self.moved = now;
        }
    }
}

/// Ease-in/out over `t` in 0..=1.
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Degrees turned `after` seconds past arrival: the spin eases linearly
/// from the flight rate to the turntable rate over the settle, then holds.
fn settled_spin(after: f32) -> f32 {
    if after < SETTLE_SECONDS {
        THROW_SPIN * after - (THROW_SPIN - FLOAT_SPIN) * after * after / (2.0 * SETTLE_SECONDS)
    } else {
        (THROW_SPIN + FLOAT_SPIN) * SETTLE_SECONDS / 2.0 + FLOAT_SPIN * (after - SETTLE_SECONDS)
    }
}

impl Throw {
    pub(super) fn is_held(&self) -> bool {
        matches!(self, Self::Held)
    }

    /// Move between held, out and back for this frame. `hand` is the hilt
    /// pose in the hand, `focus` where the shot floats it, `blade` the
    /// hilt-local direction of its first blade; returns the hilt pose when
    /// it is not in the hand.
    pub(super) fn advance(
        &mut self,
        thrown: bool,
        hand: Option<Pose>,
        focus: Option<Vec3>,
        blade: Vec3,
        now: Instant,
    ) -> Option<Pose> {
        let (Some(hand), Some(focus)) = (hand, focus) else {
            *self = Self::Held;
            return None;
        };
        if let Self::Out { target, .. } = self {
            target.retarget(focus, now);
        }
        let current = self.pose(hand, blade, now);
        match (*self, thrown) {
            (Self::Held, true) => {
                ui_cues::post(Cue::Throw);
                *self = Self::Out {
                    started: now,
                    launch: hand,
                    target: Target::new(focus, now),
                };
            }
            (Self::Back { .. }, true) => {
                ui_cues::post(Cue::Throw);
                *self = Self::Out {
                    started: now,
                    launch: current.unwrap_or(hand),
                    target: Target::new(focus, now),
                };
            }
            (Self::Out { .. }, false) => {
                let from = current.unwrap_or(hand);
                *self = Self::Back {
                    started: now,
                    from,
                    seconds: from.grip.distance(hand.grip) / THROW_SPEED,
                };
            }
            (
                Self::Back {
                    started, seconds, ..
                },
                false,
            ) if now.saturating_duration_since(started).as_secs_f32() >= seconds => {
                ui_cues::post(Cue::Catch);
                *self = Self::Held;
            }
            _ => {}
        }
        self.pose(hand, blade, now)
    }

    fn pose(&self, hand: Pose, blade: Vec3, now: Instant) -> Option<Pose> {
        match *self {
            Self::Held => None,
            Self::Out {
                started,
                launch,
                target,
            } => {
                let focus = target.at(now);
                let elapsed = now.saturating_duration_since(started).as_secs_f32();
                let flight = launch.grip.distance(focus) / THROW_SPEED;
                // Spinning flat like the thrown blade, then tilting upright.
                let flat = Quat::from_rotation_arc(blade, Vec3::X);
                let upright = Quat::from_rotation_arc(blade, Vec3::Z);
                let (grip, spin, tilt) = if elapsed < flight {
                    (
                        launch.grip.lerp(focus, elapsed / flight),
                        THROW_SPIN * elapsed,
                        0.0,
                    )
                } else {
                    let after = elapsed - flight;
                    (
                        focus,
                        THROW_SPIN * flight + settled_spin(after),
                        smoothstep(after / SETTLE_SECONDS),
                    )
                };
                Some(Pose {
                    grip,
                    rotation: Quat::from_rotation_z(spin.to_radians()) * flat.slerp(upright, tilt),
                })
            }
            Self::Back {
                started,
                from,
                seconds,
            } => {
                let elapsed = now.saturating_duration_since(started).as_secs_f32();
                let t = if seconds > f32::EPSILON {
                    (elapsed / seconds).min(1.0)
                } else {
                    1.0
                };
                Some(Pose {
                    grip: from.grip.lerp(hand.grip, t),
                    rotation: from.rotation.slerp(hand.rotation, smoothstep(t)),
                })
            }
        }
    }
}
