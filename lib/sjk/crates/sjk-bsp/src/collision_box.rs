//! A box swept against a box that stands somewhere: how a move is clipped against an
//! entity that has bounds but no brush model — a player. The reference builds a one-brush
//! model from the bounds (`CM_TempBoxModel`, `cm_trace.cpp`/`cm_test.cpp`) and sweeps
//! through it in the entity's frame (`CM_TransformedBoxTrace`); its server
//! (`SV_ClipMoveToEntities`) and its clients' prediction (`CG_ClipMoveToEntities`) both do,
//! so the arithmetic here is theirs, operation for operation. Held against
//! `tools/box-trace`: 4,000 traces through the reference's own collision model.
use super::brush::{Sweep, sweep_sides};
use super::*;

/// Sweep `bounds` from `start` to `end` against the box `target` standing at `origin`
/// (unrotated). `target_contents` is what the box is made of (a player: the body flag);
/// a `content_mask` that does not ask for it sees nothing.
pub fn trace_box_against_box(
    start: [f32; 3],
    end: [f32; 3],
    bounds: Aabb,
    target: Aabb,
    origin: [f32; 3],
    target_contents: u32,
    content_mask: u32,
) -> CollisionTrace {
    let mut trace = CollisionTrace::miss(start, end);
    if target_contents & content_mask == 0
        || !start
            .into_iter()
            .chain(end)
            .chain(origin)
            .all(f32::is_finite)
    {
        return trace;
    }
    let work = TraceWork::relative(start, end, bounds, origin);
    // `CM_TraceThroughBrush` and `CM_TestBoxInBrush` both begin by comparing the swept
    // bounds with the brush's.
    let (low, high) = work.swept_bounds();
    if (0..3)
        .any(|axis| low[axis] > target.maximums()[axis] || high[axis] < target.minimums()[axis])
    {
        return interpolated(trace, start, end);
    }
    if work.start == work.end {
        // `CM_PositionTest`: a box brush has only its six axial sides, which the bounds
        // comparison has just covered.
        (trace.start_solid, trace.all_solid, trace.fraction) = (true, true, 0.0);
        return interpolated(trace, start, end);
    }
    // The box hull's sides in `CM_InitBoxHull`'s order: +x, -x, +y, -y, +z, -z.
    let sides = (0..6).map(|side| {
        let (axis, negative) = (side >> 1, side & 1 == 1);
        let mut normal = [0.0; 3];
        normal[axis] = if negative { -1.0 } else { 1.0 };
        (
            Plane {
                normal,
                distance: if negative {
                    -target.minimums()[axis]
                } else {
                    target.maximums()[axis]
                },
            },
            (),
        )
    });
    match sweep_sides(&work, sides) {
        Sweep::Miss => {}
        Sweep::Inside { all } => {
            trace.start_solid = true;
            // The reference names the contents only of what a move runs into, not of what
            // it is stuck in.
            if all {
                (trace.all_solid, trace.fraction) = (true, 0.0);
            }
        }
        Sweep::Through { enter, leave, lead } => {
            if enter < leave && enter > -1.0 && enter < trace.fraction {
                trace.fraction = enter.max(0.0);
                trace.plane = lead.map(|(plane, ())| plane);
                trace.content_flags = target_contents;
            }
        }
    }
    interpolated(trace, start, end)
}

/// `CM_TransformedBoxTrace` always recomputes the end from the fraction, a miss included.
fn interpolated(mut trace: CollisionTrace, start: [f32; 3], end: [f32; 3]) -> CollisionTrace {
    trace.end_position =
        std::array::from_fn(|axis| start[axis] + trace.fraction * (end[axis] - start[axis]));
    trace
}
