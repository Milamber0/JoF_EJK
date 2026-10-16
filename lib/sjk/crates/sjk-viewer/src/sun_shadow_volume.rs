//! Camera-volume fit with upstream map coverage; independent of visible actor bounds.
use glam::{Mat4, Vec3};

/// Fit a truncated main-view frustum in XY and all map occluders in light-space depth.
/// No camera PVS is used: offscreen architecture must still block sunlight.
pub(super) fn fit(
    camera: Mat4,
    sun: Vec3,
    bounds: [Vec3; 2],
    distance: f32,
    resolution: u32,
) -> Option<super::fit::Fit> {
    if !camera.is_finite()
        || camera.determinant().abs() < 1e-12
        || !sun.is_finite()
        || sun.length_squared() < 0.5
        || resolution == 0
        || !distance.is_finite()
        || distance <= 0.
        || bounds.iter().any(|p| !p.is_finite())
    {
        return None;
    }
    let inverse = camera.inverse();
    let near = inverse.project_point3(Vec3::ZERO);
    let far = inverse.project_point3(Vec3::Z);
    let forward = (far - near).normalize();
    if !forward.is_finite() {
        return None;
    }
    let view = light_view(sun);
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for x in [-1., 1.] {
        for y in [-1., 1.] {
            let a = inverse.project_point3(Vec3::new(x, y, 0.));
            let b = inverse.project_point3(Vec3::new(x, y, 1.));
            let ray = b - a;
            let axial = forward.dot(ray);
            if !a.is_finite() || !b.is_finite() || axial <= 0. {
                return None;
            }
            for point in [a, a + ray * (distance / axial).min(1.)] {
                let point = view.transform_point3(point);
                lo = lo.min(point);
                hi = hi.max(point);
            }
        }
    }
    Some(square(view, lo, hi, bounds, resolution))
}

/// The light's view: down the sun's direction at the origin.
pub(super) fn light_view(sun: Vec3) -> Mat4 {
    let sun = sun.normalize();
    let up = if sun.z.abs() > 0.95 { Vec3::Y } else { Vec3::Z };
    glam::camera::rh::view::look_at_mat4(sun * 4096., Vec3::ZERO, up)
}

/// The square fit over light-space `lo..hi` in XY, its centre on a texel, deep enough for
/// the whole map: sunward geometry can be far outside the square's own depth, but only its
/// XY projection overlapping the square matters.
pub(super) fn square(
    view: Mat4,
    lo: Vec3,
    hi: Vec3,
    bounds: [Vec3; 2],
    resolution: u32,
) -> super::fit::Fit {
    // What the square was fitted to is inside its depth, wherever the map's bounds are.
    let (mut near, mut far) = (-hi.z - 16., -lo.z + 16.);
    for x in [bounds[0].x, bounds[1].x] {
        for y in [bounds[0].y, bounds[1].y] {
            for z in [bounds[0].z, bounds[1].z] {
                let depth = -view.transform_point3(Vec3::new(x, y, z)).z;
                near = near.min(depth - 16.);
                far = far.max(depth + 16.);
            }
        }
    }
    let width = (((hi.x - lo.x).max(hi.y - lo.y) + 32.).max(256.) / 64.).ceil() * 64.;
    let texel = width / resolution as f32;
    let center = ((lo + hi) * 0.5 / texel).round() * texel;
    let projection = glam::camera::rh::proj::directx::orthographic(
        center.x - width * 0.5,
        center.x + width * 0.5,
        center.y - width * 0.5,
        center.y + width * 0.5,
        near,
        far,
    );
    super::fit::Fit {
        matrix: projection * view,
        texel,
        depth: far - near,
    }
}

/// Fit every map occluder in one light-space square: the far cascade behind the view fit.
/// Texels are coarse and world-only, but no surface is ever assumed sunlit for lack of a map.
pub(super) fn fit_map(sun: Vec3, bounds: [Vec3; 2], resolution: u32) -> Option<super::fit::Fit> {
    if !sun.is_finite()
        || sun.length_squared() < 0.5
        || resolution == 0
        || bounds.iter().any(|p| !p.is_finite())
        || bounds[0] == bounds[1]
    {
        return None;
    }
    let view = light_view(sun);
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for x in [bounds[0].x, bounds[1].x] {
        for y in [bounds[0].y, bounds[1].y] {
            for z in [bounds[0].z, bounds[1].z] {
                let point = view.transform_point3(Vec3::new(x, y, z));
                lo = lo.min(point);
                hi = hi.max(point);
            }
        }
    }
    Some(square(view, lo, hi, bounds, resolution))
}
