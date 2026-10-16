//! Convex brush clipping and its swept-bounds broad phase.
use super::*;

impl Bsp {
    /// Clips against a brush after rejecting disjoint swept bounds.
    pub(super) fn trace_brush(
        &self,
        brush_index: usize,
        work: &TraceWork,
        content_mask: u32,
        trace: &mut CollisionTrace,
    ) {
        let brush = &self.brushes[brush_index];
        if brush.content_flags & content_mask == 0 {
            return;
        }

        // CM_TraceThroughBrush (codemp/qcommon/cm_trace.cpp:553-563) rejects
        // disjoint swept bounds BEFORE the 1/8-unit plane clipping tolerance.
        // Without this, a nearly horizontal trace can hit a floor it never enters.
        for axis in 0..3 {
            let low = self.planes[self.brush_sides[brush.sides.start + axis * 2].plane];
            let high = self.planes[self.brush_sides[brush.sides.start + axis * 2 + 1].plane];
            let mut normal = [0.0; 3];
            normal[axis] = 1.0;
            if high.normal == normal && low.normal == normal.map(|v| -v) {
                let minimum = work.start[axis].min(work.end[axis]) - work.extents[axis];
                let maximum = work.start[axis].max(work.end[axis]) + work.extents[axis];
                if minimum > high.distance || maximum < -low.distance {
                    return;
                }
            }
        }
        let sides = brush.sides.clone().map(|side_index| {
            let side = &self.brush_sides[side_index];
            (self.planes[side.plane], side.shader)
        });
        match sweep_sides(work, sides) {
            Sweep::Miss => {}
            Sweep::Inside { all } => {
                trace.start_solid = true;
                if all {
                    trace.all_solid = true;
                    trace.fraction = 0.0;
                    trace.content_flags = brush.content_flags;
                }
            }
            Sweep::Through { enter, leave, lead } => {
                if enter < leave && enter > -1.0 && enter < trace.fraction {
                    trace.fraction = enter.max(0.0);
                    trace.plane = lead.map(|(plane, _)| plane);
                    trace.surface_flags = lead
                        .map(|(_, shader)| self.shaders[shader].surface_flags)
                        .unwrap_or_default();
                    trace.shader = lead.map(|(_, shader)| shader);
                    trace.content_flags = brush.content_flags;
                }
            }
        }
    }
}

/// What the side loop of `CM_TraceThroughBrush` (`cm_trace.cpp:565-640`) finds for one
/// convex set of sides.
pub(super) enum Sweep<S> {
    /// The move stays in front of one side: it cannot touch the solid.
    Miss,
    /// The move starts inside; `all` if it ends inside too.
    Inside { all: bool },
    /// Fractions at which the move enters and leaves, and the side it enters through.
    Through {
        enter: f32,
        leave: f32,
        lead: Option<(Plane, S)>,
    },
}

/// Sweep a box through the sides of one convex solid, each pushed out by the corner of
/// the box that reaches furthest into it, stopping 1/8 unit short of the surface.
pub(super) fn sweep_sides<S: Copy>(
    work: &TraceWork,
    sides: impl Iterator<Item = (Plane, S)>,
) -> Sweep<S> {
    let (mut enter_fraction, mut leave_fraction) = (-1.0_f32, 1.0_f32);
    let mut lead = None;
    let (mut start_out, mut get_out) = (false, false);
    for (plane, tag) in sides {
        let expanded_distance = plane.distance - dot(work.corner_for_plane(plane), plane.normal);
        let distance_start = dot(work.start, plane.normal) - expanded_distance;
        let distance_end = dot(work.end, plane.normal) - expanded_distance;

        get_out |= distance_end > 0.0;
        start_out |= distance_start > 0.0;
        if distance_start > 0.0
            && (distance_end >= SURFACE_CLIP_EPSILON || distance_end >= distance_start)
        {
            return Sweep::Miss;
        }
        if distance_start <= 0.0 && distance_end <= 0.0 {
            continue;
        }

        let denominator = distance_start - distance_end;
        if distance_start > distance_end {
            let candidate = ((distance_start - SURFACE_CLIP_EPSILON) / denominator).clamp(0.0, 1.0);
            if candidate > enter_fraction {
                enter_fraction = candidate;
                lead = Some((plane, tag));
            }
        } else {
            let candidate = ((distance_start + SURFACE_CLIP_EPSILON) / denominator).clamp(0.0, 1.0);
            leave_fraction = leave_fraction.min(candidate);
        }
    }
    if !start_out {
        return Sweep::Inside { all: !get_out };
    }
    Sweep::Through {
        enter: enter_fraction,
        leave: leave_fraction,
        lead,
    }
}
