//! CPU geometry support: unchanged trace and basis arithmetic.
use super::*;

/// Preserve the existing cylinder endpoint probe.
pub(super) fn trace_length(origin: Vec3, axis: Vec3, bsp: &Bsp, scratch: &mut TraceScratch) -> f32 {
    shortened_length(
        origin,
        trace_endpoint(
            origin,
            origin + axis.normalize_or(Vec3::X) * MAX_TRACE_DISTANCE,
            bsp,
            scratch,
        ),
    )
}

/// Distance to the CPU collision endpoint, unchanged by GPU expansion.
pub(crate) fn shortened_length(origin: Vec3, trace_end: Vec3) -> f32 {
    origin.distance(trace_end)
}

/// Preserve the existing point trace and mask without entering prediction.
pub(super) fn trace_endpoint(
    origin: Vec3,
    end: Vec3,
    bsp: &Bsp,
    scratch: &mut TraceScratch,
) -> Vec3 {
    Vec3::from_array(
        bsp.trace_box_with(
            scratch,
            origin.to_array(),
            end.to_array(),
            Aabb::POINT,
            0x0000_1001,
        )
        .end_position,
    )
}

/// Preserve the renderer's cylinder basis selection.
pub(super) fn normal_vectors(forward: Vec3) -> (Vec3, Vec3) {
    let reference = if forward.z.abs() > 0.9 {
        Vec3::Y
    } else {
        Vec3::Z
    };
    let right = forward.cross(reference).normalize_or(Vec3::Y);
    (right, right.cross(forward).normalize_or(Vec3::Z))
}
