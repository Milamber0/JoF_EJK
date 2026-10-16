//! Camera flight between two backdrop shots along an authored route: a
//! Catmull-Rom spline through the waypoints, parametrised by arc length so
//! the speed stays even, with a smoothstep ease over the whole trip.

use super::Vantage;
use super::routes::{Route, Waypoint};
use glam::Vec3;

/// Longest route the fixed sampling buffers accept (vantage + waypoints).
const MAX_POINTS: usize = 16;

/// One spline control point in radians.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Knot {
    origin: Vec3,
    yaw: f32,
    pitch: f32,
}

/// Camera pose along the flight at some progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pose {
    pub(crate) origin: Vec3,
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
}

/// A fully resolved route: the departure vantage followed by the authored
/// waypoints, with yaws unwrapped so each turn takes the short way round.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Path {
    knots: [Knot; MAX_POINTS],
    count: usize,
    /// Cumulative chord length up to each knot.
    distance: [f32; MAX_POINTS],
}

impl Path {
    pub(crate) fn new(start: Vantage, route: &Route) -> Self {
        let mut knots = [Knot::default(); MAX_POINTS];
        let mut distance = [0.0; MAX_POINTS];
        knots[0] = Knot {
            origin: start.origin,
            yaw: start.yaw,
            pitch: start.pitch,
        };
        let count = (route.points.len() + 1).min(MAX_POINTS);
        for (index, waypoint) in route.points.iter().take(count - 1).enumerate() {
            let previous = knots[index];
            let knot = knot_from(waypoint, previous.yaw);
            distance[index + 1] = distance[index] + previous.origin.distance(knot.origin);
            knots[index + 1] = knot;
        }
        Self {
            knots,
            count,
            distance,
        }
    }

    /// Final knot of the route: the shot the flight arrives at.
    pub(crate) fn destination(&self) -> Vantage {
        let knot = self.knots[self.count - 1];
        Vantage {
            origin: knot.origin,
            yaw: knot.yaw,
            pitch: knot.pitch,
        }
    }

    /// Pose at arc-length fraction `progress` (0 = departure, 1 = arrival).
    pub(crate) fn sample(&self, progress: f32) -> Pose {
        let total = self.distance[self.count - 1];
        if self.count < 2 || total <= f32::EPSILON {
            return pose(self.knots[self.count - 1]);
        }
        let target = progress.clamp(0.0, 1.0) * total;
        let mut segment = 0;
        while segment + 2 < self.count && self.distance[segment + 1] < target {
            segment += 1;
        }
        let span = (self.distance[segment + 1] - self.distance[segment]).max(f32::EPSILON);
        let t = ((target - self.distance[segment]) / span).clamp(0.0, 1.0);
        let p0 = self.knots[segment.saturating_sub(1)];
        let p1 = self.knots[segment];
        let p2 = self.knots[segment + 1];
        let p3 = self.knots[(segment + 2).min(self.count - 1)];
        pose(catmull_rom(p0, p1, p2, p3, t))
    }
}

fn knot_from(waypoint: &Waypoint, previous_yaw: f32) -> Knot {
    let yaw = waypoint.yaw.to_radians();
    let turn = (yaw - previous_yaw).rem_euclid(std::f32::consts::TAU);
    let turn = if turn > std::f32::consts::PI {
        turn - std::f32::consts::TAU
    } else {
        turn
    };
    Knot {
        origin: Vec3::from_array(waypoint.origin),
        yaw: previous_yaw + turn,
        pitch: waypoint.pitch.to_radians(),
    }
}

fn pose(knot: Knot) -> Pose {
    Pose {
        origin: knot.origin,
        yaw: knot.yaw,
        pitch: knot.pitch,
    }
}

/// Uniform Catmull-Rom between `p1` and `p2`; the end knots are duplicated
/// by the caller so the curve starts and stops exactly on the shots.
fn catmull_rom(p0: Knot, p1: Knot, p2: Knot, p3: Knot, t: f32) -> Knot {
    let t2 = t * t;
    let t3 = t2 * t;
    let w0 = -0.5 * t3 + t2 - 0.5 * t;
    let w1 = 1.5 * t3 - 2.5 * t2 + 1.0;
    let w2 = -1.5 * t3 + 2.0 * t2 + 0.5 * t;
    let w3 = 0.5 * t3 - 0.5 * t2;
    Knot {
        origin: p0.origin * w0 + p1.origin * w1 + p2.origin * w2 + p3.origin * w3,
        yaw: p0.yaw * w0 + p1.yaw * w1 + p2.yaw * w2 + p3.yaw * w3,
        pitch: p0.pitch * w0 + p1.pitch * w1 + p2.pitch * w2 + p3.pitch * w3,
    }
}
