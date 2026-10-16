//! Stable, conservative actor-region projection; never a simulation query.
use glam::{Mat4, Vec3};

/// Orthographic transform and texel/depth metrics for contact-hardening filtering.
#[derive(Clone, Copy)]
pub(super) struct Fit {
    /// World-to-light clip transform, with zero-to-one depth.
    pub(super) matrix: Mat4,
    /// World-space width of one square shadow texel.
    pub(super) texel: f32,
    /// World-space distance corresponding to the map's unit depth interval.
    pub(super) depth: f32,
}

/// Reject conservative actor spheres outside the exact main-view frustum.
pub(super) fn fit_visible(
    instances: &[crate::ActorInstance],
    sun: Vec3,
    resolution: u32,
    camera: Option<Mat4>,
) -> Option<Fit> {
    if !sun.is_finite() || sun.length_squared() < 0.5 {
        return None;
    }
    let sun = sun.normalize();
    let up = if sun.z.abs() > 0.95 { Vec3::Y } else { Vec3::Z };
    let view = glam::camera::rh::view::look_at_mat4(sun * 4096., Vec3::ZERO, up);
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for instance in instances {
        if instance.depth_hack != 0. || instance.view_flags & 1 != 0 {
            continue;
        }
        let center = view.transform_point3(Vec3::from_array(instance.position));
        let radius = 96. * Vec3::from_array(instance.scale).abs().max_element();
        if !center.is_finite() || !radius.is_finite() || radius <= 0. {
            continue;
        }
        if let Some(camera) = camera {
            let rows = camera.transpose();
            let planes = [
                rows.w_axis + rows.x_axis,
                rows.w_axis - rows.x_axis,
                rows.w_axis + rows.y_axis,
                rows.w_axis - rows.y_axis,
                rows.z_axis,
                rows.w_axis - rows.z_axis,
            ];
            let point = Vec3::from_array(instance.position).extend(1.);
            if planes
                .iter()
                .any(|p| p.dot(point) < -radius * p.truncate().length())
            {
                continue;
            }
        }
        lo = lo.min(center - Vec3::splat(radius));
        hi = hi.max(center + Vec3::splat(radius));
    }
    if !lo.is_finite() {
        return None;
    }
    // Quantized square extent avoids aspect-dependent texels; snap translation to texels.
    let width = (((hi.x - lo.x).max(hi.y - lo.y) + 32.).max(256.) / 64.).ceil() * 64.;
    let texel = width / resolution as f32;
    let center = ((lo + hi) * 0.5 / texel).round() * texel;
    let near = -hi.z - 16.;
    let far = -lo.z + 512.; // Receivers behind casters, not additional static casters.
    let projection = glam::camera::rh::proj::directx::orthographic(
        center.x - width * 0.5,
        center.x + width * 0.5,
        center.y - width * 0.5,
        center.y + width * 0.5,
        near,
        far,
    );
    Some(Fit {
        matrix: projection * view,
        texel,
        depth: far - near,
    })
}
