//! A box swept against an axis-aligned box — or a box cut to a wedge by one inclined plane —
//! with the collision model's arithmetic (`CM_TraceThroughBrush`: enter and leave fractions,
//! `SURFACE_CLIP_EPSILON`). The game's own traces go through its host; this is for what the
//! game traces against its own entities' boxes where no host trace passes the right ones (a
//! vehicle's passenger getting off, [`crate::vehicle_update`]), and for the oracle's room.

use crate::pmove::MovementTrace;

/// A solid: a box, and optionally a seventh, inclined plane `(normal, dist)` that cuts it
/// down to a wedge — the part with `normal . p <= dist`.
pub(crate) type Solid = ([f32; 3], [f32; 3], Option<([f32; 3], f32)>);

/// One solid against the swept box (`CM_TraceThroughBrush`): `best` is bettered where
/// it is hit sooner, and marked where the sweep starts inside it, naming `entity`.
pub(crate) fn sweep_solid(
    best: &mut MovementTrace,
    start: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    end: [f32; 3],
    solid: &Solid,
    entity: u16,
) {
    const EPSILON: f32 = 0.125;
    let &(solid_mins, solid_maxs, inclined) = solid;
    let (mut enter, mut leave, mut side) = (-1.0_f32, 1.0_f32, 0);
    let (mut starts_out, mut ends_out) = (false, false);
    for plane in 0..if inclined.is_some() { 7 } else { 6 } {
        let (axis, positive) = (plane >> 1, plane & 1 == 0);
        let (d1, d2) = match inclined {
            // The inclined plane, pushed out by the corner of the moving box that
            // reaches furthest into it (CM's `offsets[signbits]`).
            Some((n, plane_dist)) if plane == 6 => {
                let corner: [f32; 3] = std::array::from_fn(|axis| {
                    if n[axis] < 0.0 {
                        maxs[axis]
                    } else {
                        mins[axis]
                    }
                });
                let dist = plane_dist - (corner[0] * n[0] + corner[1] * n[1] + corner[2] * n[2]);
                (
                    (start[0] * n[0] + start[1] * n[1] + start[2] * n[2]) - dist,
                    (end[0] * n[0] + end[1] * n[1] + end[2] * n[2]) - dist,
                )
            }
            _ => {
                let dist = if positive {
                    solid_maxs[axis] - mins[axis]
                } else {
                    -(solid_mins[axis] - maxs[axis])
                };
                (
                    if positive { start[axis] } else { -start[axis] } - dist,
                    if positive { end[axis] } else { -end[axis] } - dist,
                )
            }
        };
        ends_out |= d2 > 0.0;
        starts_out |= d1 > 0.0;
        if d1 > 0.0 && (d2 >= EPSILON || d2 >= d1) {
            return;
        }
        if d1 <= 0.0 && d2 <= 0.0 {
            continue;
        }
        if d1 > d2 {
            let fraction = ((d1 - EPSILON) / (d1 - d2)).max(0.0);
            if fraction > enter {
                (enter, side) = (fraction, plane);
            }
        } else {
            leave = leave.min(((d1 + EPSILON) / (d1 - d2)).min(1.0));
        }
    }
    if !starts_out {
        best.start_solid = true;
        best.entity_number = entity;
        if !ends_out {
            (best.all_solid, best.fraction) = (true, 0.0);
        }
        return;
    }
    if enter < leave && enter > -1.0 && enter < best.fraction {
        best.fraction = enter.max(0.0);
        best.plane_normal = [0.0; 3];
        match inclined {
            Some((n, _)) if side == 6 => best.plane_normal = n,
            _ => best.plane_normal[side >> 1] = if side & 1 == 1 { -1.0 } else { 1.0 },
        }
        best.entity_number = entity;
    }
}

/// One box entity against a swept box (`CM_TransformedBoxTrace` on a box model): its
/// trace, its end from its fraction.
pub(crate) fn sweep_box(
    player: &crate::entity_clip::BoxObstacle,
    start: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    end: [f32; 3],
) -> MovementTrace {
    let mut best = MovementTrace::miss(end);
    let solid_mins: [f32; 3] =
        std::array::from_fn(|axis| player.origin[axis] + player.bounds.0[axis]);
    let solid_maxs: [f32; 3] =
        std::array::from_fn(|axis| player.origin[axis] + player.bounds.1[axis]);
    sweep_solid(
        &mut best,
        start,
        mins,
        maxs,
        end,
        &(solid_mins, solid_maxs, None),
        player.entity,
    );
    if best.fraction != 1.0 {
        for axis in 0..3 {
            best.end_position[axis] = start[axis] + best.fraction * (end[axis] - start[axis]);
        }
    }
    best
}
