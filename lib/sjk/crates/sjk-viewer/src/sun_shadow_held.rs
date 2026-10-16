//! A view fit that does not turn with the camera, so the static casters drawn into it stay
//! valid while the eye moves a little (`r_liveLighting` 1 and below): the world is drawn into
//! a shadow map when the eye leaves the square or the sun turns, not every frame.
use super::fit::Fit;
use glam::{Mat4, Vec3};

/// How far beyond the view volume's reach a held fit extends, as a share of that reach:
/// the eye moves that far before the static casters are drawn again. Texels grow by as much.
const MARGIN: f32 = 0.25;

/// The eye (the middle of the near plane) and the distance from it to the farthest corner
/// of the view volume cut `distance` along the view axis.
fn reach(camera: Mat4, distance: f32) -> Option<(Vec3, f32)> {
    if !camera.is_finite()
        || camera.determinant().abs() < 1e-12
        || !distance.is_finite()
        || distance <= 0.
    {
        return None;
    }
    let inverse = camera.inverse();
    let eye = inverse.project_point3(Vec3::ZERO);
    let forward = (inverse.project_point3(Vec3::Z) - eye).normalize();
    let mut reach = 0_f32;
    for x in [-1., 1.] {
        for y in [-1., 1.] {
            let a = inverse.project_point3(Vec3::new(x, y, 0.));
            let ray = inverse.project_point3(Vec3::new(x, y, 1.)) - a;
            let axial = forward.dot(ray);
            if !a.is_finite() || !ray.is_finite() || axial.is_nan() || axial <= 0. {
                return None;
            }
            reach = reach.max((a + ray * (distance / axial).min(1.)).distance(eye));
        }
    }
    Some((eye, reach))
}

/// Whether `held`, a map `resolution` texels wide, still covers the view volume of `camera`
/// cut at `distance`, whichever way the camera turns where it stands.
pub(super) fn holds(held: &Fit, resolution: u32, camera: Mat4, distance: f32) -> bool {
    let Some((eye, reach)) = reach(camera, distance) else {
        return false;
    };
    let centre = held.matrix.project_point3(eye);
    let share = reach / (held.texel * resolution as f32 * 0.5);
    centre.x.abs() + share <= 1. && centre.y.abs() + share <= 1.
}

/// The square around the eye that holds the view volume cut at `distance` for every
/// direction of view, and a margin to move in; all map occluders in light-space depth.
pub(super) fn fit(
    camera: Mat4,
    sun: Vec3,
    bounds: [Vec3; 2],
    distance: f32,
    resolution: u32,
) -> Option<Fit> {
    if !sun.is_finite()
        || sun.length_squared() < 0.5
        || resolution == 0
        || bounds.iter().any(|p| !p.is_finite())
    {
        return None;
    }
    let (eye, reach) = reach(camera, distance)?;
    let view = super::volume::light_view(sun);
    let centre = view.transform_point3(eye);
    let half = Vec3::splat(reach * (1. + MARGIN));
    Some(super::volume::square(
        view,
        centre - half,
        centre + half,
        bounds,
        resolution,
    ))
}

/// The static casters of one shadow map, kept between frames.
pub(super) struct Held {
    /// Static-only depths, filtered independently from moving casters.
    depth: wgpu::TextureView,
    /// The sun direction and the fit its contents were drawn with.
    drawn: std::cell::Cell<Option<(Vec3, Fit)>>,
}

impl Held {
    pub(super) fn new(depth: wgpu::TextureView) -> Self {
        Self {
            depth,
            drawn: Default::default(),
        }
    }

    pub(super) fn depth(&self) -> &wgpu::TextureView {
        &self.depth
    }

    /// The fit to use this frame, and whether the static casters must be drawn into it:
    /// what was drawn stays while the sun stands and the view volume is inside it.
    pub(super) fn fit(
        &self,
        camera: Mat4,
        sun: Vec3,
        bounds: [Vec3; 2],
        distance: f32,
        resolution: u32,
    ) -> Option<(Fit, bool)> {
        if let Some((drawn, fit)) = self.drawn.get()
            && drawn.dot(sun) >= super::FAR_REFRESH_COS
            && holds(&fit, resolution, camera, distance)
        {
            return Some((fit, false));
        }
        let fit = fit(camera, sun, bounds, distance, resolution)?;
        self.drawn.set(Some((sun, fit)));
        Some((fit, true))
    }
}
