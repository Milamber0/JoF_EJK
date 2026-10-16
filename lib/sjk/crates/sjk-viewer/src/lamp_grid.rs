//! Conservative map-lifetime lamp lists. A cell boundary cannot drop nonzero light.
use super::{Lamp, LampSet, selection};
use glam::{IVec3, Vec3};

const CELL: f32 = 128.;
const MAX_CELLS: u64 = 1_048_576;

/// Build conservative cell lists and their shared importance field at installation.
pub(super) fn build(lamps: Vec<Lamp>) -> LampSet {
    let mut set = unrefined(lamps);
    if std::env::var("SJK_LAMP_REFINEMENT").as_deref() != Ok("0") {
        super::refine::apply(&mut set);
    }
    set
}

/// The coarse reference, also used by refinement equivalence fixtures.
pub(super) fn unrefined(lamps: Vec<Lamp>) -> LampSet {
    if lamps.is_empty() {
        return LampSet::default();
    }
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for lamp in &lamps {
        lo = lo.min(lamp.position - lamp.radius);
        hi = hi.max(lamp.position + lamp.radius);
    }
    let mut cell = CELL;
    let (origin, counts) = loop {
        let origin = (lo / cell).floor().as_ivec3();
        let counts = (hi / cell).floor().as_ivec3() - origin + IVec3::ONE;
        if counts
            .to_array()
            .into_iter()
            .map(|n| n as u64)
            .product::<u64>()
            <= MAX_CELLS
        {
            break (origin, counts);
        }
        cell *= 2.;
    };
    let total = counts.x as usize * counts.y as usize * counts.z as usize;
    let mut buckets = vec![Vec::new(); total];
    for (id, lamp) in lamps.iter().enumerate() {
        let a = ((lamp.position - lamp.radius) / cell).floor().as_ivec3() - origin;
        let b = ((lamp.position + lamp.radius) / cell).floor().as_ivec3() - origin;
        for z in a.z..=b.z {
            for y in a.y..=b.y {
                for x in a.x..=b.x {
                    let lower = (IVec3::new(x, y, z) + origin).as_vec3() * cell;
                    if !touches(lamp, lower, cell) {
                        continue;
                    }
                    let index = (x + (y + z * counts.y) * counts.x) as usize;
                    buckets[index].push(id as u32);
                }
            }
        }
    }
    let thresholds = selection::nodes(&lamps, &buckets, origin, counts, cell);
    let mut cells = Vec::with_capacity(total);
    let mut cell_lamps = Vec::new();
    for (index, mut bucket) in buckets.into_iter().enumerate() {
        let at = IVec3::new(
            index as i32 % counts.x,
            index as i32 / counts.x % counts.y,
            index as i32 / (counts.x * counts.y),
        );
        let lower = (at + origin).as_vec3() * cell;
        let minimum = selection::lower_bound(&thresholds, counts, at);
        bucket.retain(|&id| selection::upper_bound(&lamps[id as usize], lower, cell) > minimum);
        cells.push([cell_lamps.len() as u32, bucket.len() as u32]);
        cell_lamps.extend(bucket);
    }
    crate::log::progress(format_args!(
        "Lamp grid: {} sources, {} cells, {} references, max {} per cell",
        lamps.len(),
        cells.len(),
        cell_lamps.len(),
        cells.iter().map(|c| c[1]).max().unwrap_or(0)
    ));
    LampSet {
        lamps,
        cell,
        origin,
        counts: counts.to_array().map(|n| n as u32),
        cells,
        cell_lamps,
        thresholds,
    }
}

// Reject only entire boxes outside the exact shader support (sphere and front plane).
// Stable source order also avoids a different floating-point sum at adjacent cells.
pub(super) fn touches(lamp: &Lamp, lower: Vec3, cell: f32) -> bool {
    let closest = lamp.position.clamp(lower, lower + cell);
    let front = (lower + cell * 0.5 - lamp.position).dot(lamp.normal)
        + lamp.normal.abs().element_sum() * cell * 0.5;
    closest.distance_squared(lamp.position) <= lamp.radius * lamp.radius
        && (lamp.normal == Vec3::ZERO || front >= 0.05)
}
