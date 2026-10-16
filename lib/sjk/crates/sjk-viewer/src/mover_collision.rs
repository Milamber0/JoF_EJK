//! `CM_TransformedBoxTrace` for a cached inline solid ([`sjk_bsp::Bsp::trace_transformed_model`],
//! which the server's movers share), and `CM_TempBoxModel` for a packed actor.

use super::*;

/// CG_PointContents (cg_predict.c:407-440): only inline models, never packed actors.
pub(super) fn contents(bsp: &Bsp, mover: &Collider, point: [f32; 3]) -> u32 {
    if mover.bounds.is_some() {
        return 0;
    }
    let Some(model) = bsp.render().models().get(mover.model) else {
        return 0;
    };
    let mut local = std::array::from_fn(|i| point[i] - mover.origin[i]);
    if mover.rotated {
        local = rotate(local, mover.axes);
    }
    let mut contents = 0;
    for brush in &bsp.brushes()[model.brushes.clone()] {
        if brush.sides.clone().all(|side| {
            let plane = bsp.planes()[bsp.brush_sides()[side].plane];
            plane.signed_distance(local) <= 0.0
        }) {
            contents |= brush.content_flags;
        }
    }
    contents
}

pub(super) fn trace(
    bsp: &Bsp,
    mover: &Collider,
    start: [f32; 3],
    end: [f32; 3],
    bounds: Aabb,
    mask: u32,
) -> MovementTrace {
    if let Some(solid) = mover.bounds {
        // A packed actor — another player: `CM_TempBoxModel` + `CM_TransformedBoxTrace`,
        // the sweep a server clips with, held bit for bit against the reference's
        // collision model (`tools/box-trace`). `CONTENTS_BODY` is what the hull is made of.
        let trace =
            sjk_bsp::trace_box_against_box(start, end, bounds, solid, mover.origin, 0x100, mask);
        return MovementTrace {
            fraction: trace.fraction,
            end_position: trace.end_position,
            plane_normal: trace.plane.map_or([0.0; 3], |plane| plane.normal),
            surface_flags: 0,
            start_solid: trace.start_solid,
            all_solid: trace.all_solid,
            entity_number: if trace.fraction < 1.0 || trace.start_solid {
                mover.entity
            } else {
                sjk_protocol::ENTITY_NUMBER_NONE
            },
        };
    }
    let trace = bsp.trace_transformed_model(
        mover.model,
        mover.origin,
        mover.rotated.then_some(mover.axes),
        start,
        end,
        bounds,
        mask,
    );
    let normal = trace.plane.map_or([0.0; 3], |plane| plane.normal);
    MovementTrace {
        fraction: trace.fraction,
        end_position: trace.end_position,
        plane_normal: normal,
        surface_flags: trace.surface_flags,
        start_solid: trace.start_solid,
        all_solid: trace.all_solid,
        entity_number: if trace.fraction != 1.0 {
            mover.entity
        } else {
            sjk_protocol::ENTITY_NUMBER_NONE
        },
    }
}

fn rotate(point: [f32; 3], axes: [[f32; 3]; 3]) -> [f32; 3] {
    axes.map(|axis| axis[0] * point[0] + axis[1] * point[1] + axis[2] * point[2])
}
