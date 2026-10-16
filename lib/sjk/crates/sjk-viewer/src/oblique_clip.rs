//! Oblique near-plane clipping (Lengyel, "Oblique View Frustum Depth
//! Projection and Clipping", JGT 2005).
//!
//! A mirrored or portal camera stands behind the surface it looks through, so
//! whatever lies behind that surface would block the view. Bending the
//! projection's near plane onto the surface's plane clips exactly that away
//! while keeping the far plane in place.

use glam::{Mat4, Vec3, Vec4};

/// `projection` with its near plane replaced by the world-space plane
/// through `point` with normal `normal` (pointing away from the camera,
/// into the kept half-space). `view` is the world-to-view matrix. Depth is
/// 0..1 as for Direct3D/wgpu.
pub(crate) fn oblique_projection(projection: Mat4, view: Mat4, point: Vec3, normal: Vec3) -> Mat4 {
    let world_plane = Vec4::new(normal.x, normal.y, normal.z, -normal.dot(point));
    let plane = view.inverse().transpose() * world_plane;
    let corner = projection.inverse() * Vec4::new(plane.x.signum(), plane.y.signum(), 1.0, 1.0);
    let scale = projection.row(3).dot(corner) / plane.dot(corner);
    let rows = [
        projection.row(0),
        projection.row(1),
        plane * scale,
        projection.row(3),
    ];
    Mat4::from_cols(rows[0], rows[1], rows[2], rows[3]).transpose()
}
