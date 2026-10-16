//! Map-bounded distant coverage, appended after the existing near-volume slices.
use glam::{Mat4, Vec3};

/// Cover visible map bounds, limited by the camera far plane. Keep the near endpoint fixed.
pub(super) fn far_distance(
    inverse: Mat4,
    eye: Vec3,
    forward: Vec3,
    bounds: [Vec3; 2],
    near_end: f32,
) -> f32 {
    let camera_end = (inverse.project_point3(Vec3::Z) - eye).dot(forward);
    let mut map_end = near_end;
    for corner in 0..8 {
        let point = Vec3::new(
            bounds[(corner & 1) as usize].x,
            bounds[((corner >> 1) & 1) as usize].y,
            bounds[((corner >> 2) & 1) as usize].z,
        );
        map_end = map_end.max((point - eye).dot(forward));
    }
    // Padding puts the endpoint behind map surfaces; never compress existing near slices.
    (map_end + near_end * 0.1)
        .min(camera_end)
        .max(near_end * 1.001)
}
