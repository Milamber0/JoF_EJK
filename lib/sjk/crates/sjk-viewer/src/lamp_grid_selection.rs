//! Continuous world-space importance, shared by every cell meeting at a grid node.
use super::Lamp;

use glam::Vec3;

const STRONG: usize = 12;

/// Receiver-independent source importance at a world position.
pub(super) fn importance(lamp: &Lamp, point: Vec3) -> f32 {
    if lamp.normal != Vec3::ZERO && (point - lamp.position).dot(lamp.normal) <= 0.05 {
        return 0.;
    }
    let d2 = point.distance_squared(lamp.position);
    let window = (1. - d2 / (lamp.radius * lamp.radius)).max(0.);
    lamp.power * window * window
        / (d2 + lamp.axis_u.length_squared() + lamp.axis_v.length_squared() + 256.)
}

/// Generate shared node thresholds from the strongest local source scores.
pub(super) fn nodes(
    lamps: &[Lamp],
    buckets: &[Vec<u32>],
    origin: glam::IVec3,
    counts: glam::IVec3,
    cell: f32,
) -> Vec<f32> {
    let dims = counts + glam::IVec3::ONE;
    let mut values = Vec::with_capacity((dims.x * dims.y * dims.z) as usize);
    for z in 0..dims.z {
        for y in 0..dims.y {
            for x in 0..dims.x {
                let at = glam::IVec3::new(x, y, z).min(counts - glam::IVec3::ONE);
                let ids = &buckets[(at.x + (at.y + at.z * counts.y) * counts.x) as usize];
                let p = (glam::IVec3::new(x, y, z) + origin).as_vec3() * cell;
                let mut scores = [0.; STRONG];
                for &id in ids {
                    let score = importance(&lamps[id as usize], p);
                    if let Some(at) = scores.iter().position(|&s| score > s) {
                        for i in (at + 1..STRONG).rev() {
                            scores[i] = scores[i - 1];
                        }
                        scores[at] = score;
                    }
                }
                // The strongest sources are fully retained at each node; lower importance
                // fades over the last fifth of this rank's score. Sparse areas use 0.
                values.push(scores[STRONG - 1] * 0.8);
            }
        }
    }
    values
}

/// Minimum interpolated threshold possible anywhere in one cell.
pub(super) fn lower_bound(values: &[f32], counts: glam::IVec3, at: glam::IVec3) -> f32 {
    let dims = counts + glam::IVec3::ONE;
    let mut minimum = f32::INFINITY;
    for z in 0..2 {
        for y in 0..2 {
            for x in 0..2 {
                let p = at + glam::IVec3::new(x, y, z);
                minimum = minimum.min(values[(p.x + (p.y + p.z * dims.y) * dims.x) as usize]);
            }
        }
    }
    minimum
}

/// Conservative upper importance throughout a cell; ignores the <=1 range window.
pub(super) fn upper_bound(lamp: &Lamp, lower: Vec3, cell: f32) -> f32 {
    let d2 = lamp
        .position
        .distance_squared(lamp.position.clamp(lower, lower + cell));
    lamp.power / (d2 + lamp.axis_u.length_squared() + lamp.axis_v.length_squared() + 256.)
}
