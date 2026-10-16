//! Refine crowded candidate lists without changing the shared importance field.
//! A child excludes only sources with zero support or importance below a
//! conservative minimum throughout that child. Source summation order survives.
use super::{LampSet, selection};
use glam::{IVec3, Vec3};
#[path = "lamp_grid_lists.rs"]
mod lists;

const CROWDED: usize = 64;
const MAX_CELLS: usize = 4 * 1024 * 1024;
const MAX_REFERENCES: usize = 64 * 1024 * 1024;
pub(super) const BRANCH: u32 = 1 << 31;

pub(super) fn apply(set: &mut LampSet) {
    apply_with_lists(
        set,
        std::env::var("SJK_LAMP_LIST_DEDUP").as_deref() != Ok("0"),
    );
}
fn apply_with_lists(set: &mut LampSet, deduplicate: bool) {
    let counts = IVec3::from_array(set.counts.map(|v| v as i32));
    let roots = set
        .counts
        .into_iter()
        .map(|v| v as usize)
        .product::<usize>();
    if roots == 0
        || set.cells.len() != roots
        || set.thresholds.len()
            != set
                .counts
                .into_iter()
                .map(|v| v as usize + 1)
                .product::<usize>()
    {
        return;
    }
    if set.cells.iter().all(|entry| entry[1] as usize <= CROWDED) {
        return;
    }
    let side = if std::env::var("SJK_LAMP_REFINEMENT_SIDE").as_deref() == Ok("2") {
        2usize
    } else {
        4
    };
    let child_count = side * side * side;
    let old = std::mem::take(&mut set.cell_lamps);

    let mut references = lists::Lists::new(old.len().min(MAX_REFERENCES), deduplicate);
    let mut scratch = Vec::new();
    let mut children = vec![[0u32; 2]; child_count];
    let mut order: Vec<_> = (0..roots).collect();
    order.sort_unstable_by_key(|&index| std::cmp::Reverse(set.cells[index][1]));
    let mut remaining = old.len();
    let mut refined = 0;
    for index in order {
        let [first, count] = set.cells[index];
        let source = &old[first as usize..(first + count) as usize];
        remaining -= source.len();
        let mut use_children = false;
        if source.len() > CROWDED {
            let at = IVec3::new(
                index as i32 % counts.x,
                index as i32 / counts.x % counts.y,
                index as i32 / (counts.x * counts.y),
            );
            let lower = (at + set.origin).as_vec3() * set.cell;
            let spacing = set.cell / side as f32;
            scratch.clear();
            for (child, pair) in children.iter_mut().enumerate() {
                let c = Vec3::new(
                    (child % side) as f32,
                    (child / side % side) as f32,
                    (child / (side * side)) as f32,
                );
                let low = lower + c * spacing;
                let guard = low.abs().max_element() * f32::EPSILON * 32. + 0.01;
                let support_low = low - guard;
                let support_size = spacing + guard * 2.;
                let minimum = child_minimum(set, at, c / side as f32, (c + 1.) / side as f32);
                let start = scratch.len() as u32;
                for &id in source {
                    let lamp = &set.lamps[id as usize];
                    if !super::grid::touches(lamp, support_low, support_size) {
                        continue;
                    }
                    let distance = lamp.position.distance_squared(
                        lamp.position.clamp(support_low, support_low + support_size),
                    );
                    let window = (1. - distance / (lamp.radius * lamp.radius)).max(0.);
                    let upper =
                        selection::upper_bound(lamp, support_low, support_size) * window * window;
                    // Roundoff margins cover the CPU bound and shader arithmetic.
                    if upper * 1.00001 + 1e-12 >= minimum {
                        scratch.push(id);
                    }
                }
                *pair = [start, scratch.len() as u32 - start];
            }
            // Avoid increasing lookup/storage cost for lists that barely get shorter.
            // Reserve space for all remaining original lists before accepting a split.
            use_children = scratch.len() * 5 < source.len() * child_count * 4
                && references.len() + references.additional(&scratch, &children) + remaining
                    <= MAX_REFERENCES
                && set.cells.len() + child_count <= MAX_CELLS;
        }
        if use_children {
            let first_child = set.cells.len() as u32;
            set.cells[index] = [first_child, BRANCH | side as u32];
            for &[offset, count] in &children {
                let first = references.insert(&scratch[offset as usize..(offset + count) as usize]);
                set.cells.push([first, count]);
            }
            refined += 1;
        } else {
            set.cells[index] = [references.insert(source), count];
        }
    }
    crate::log::progress(format_args!(
        "Refined lamp grid: {refined} crowded cells, {} -> {} references",
        old.len(),
        references.len()
    ));
    // A/B builds retain a second root table and original lists in the same buffer.
    // Switching one uniform offset then compares identical shaders and settled GI.

    set.cell_lamps = references.finish();
}

// A smooth trilinear field is monotone on each axis inside a parent cell, so its
// minimum in a rectangular child is attained at one of the child's eight corners.
fn child_minimum(set: &LampSet, at: IVec3, low: Vec3, high: Vec3) -> f32 {
    let dims = IVec3::from_array(set.counts.map(|v| v as i32 + 1));
    let mut nodes = [0.; 8];
    for (i, node) in nodes.iter_mut().enumerate() {
        let p = at + IVec3::new((i & 1) as i32, ((i >> 1) & 1) as i32, ((i >> 2) & 1) as i32);
        *node = set.thresholds[(p.x + (p.y + p.z * dims.y) * dims.x) as usize];
    }
    let mut minimum = f32::INFINITY;
    for corner in 0..8 {
        let t = Vec3::new(
            if corner & 1 == 0 { low.x } else { high.x },
            if corner & 2 == 0 { low.y } else { high.y },
            if corner & 4 == 0 { low.z } else { high.z },
        );
        let t = t * t * (Vec3::splat(3.) - 2. * t);
        let mut value = 0.;
        for (i, &node) in nodes.iter().enumerate() {
            let weight = if i & 1 == 0 { 1. - t.x } else { t.x };
            let weight = weight * if i & 2 == 0 { 1. - t.y } else { t.y };
            let weight = weight * if i & 4 == 0 { 1. - t.z } else { t.z };
            value += node * weight;
        }
        minimum = minimum.min(value);
    }
    minimum * 0.99999
}
