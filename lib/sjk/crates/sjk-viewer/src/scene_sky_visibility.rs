//! Conservative, map-retained scheduling of the existing sky-portal pass.
use super::*;

struct Surface {
    center: Vec3,
    radius: f32,
    clusters: Vec<usize>,
}

/// Retained sky candidates; no allocation or texture lookup during selection.
pub(super) struct SkyVisibility {
    surfaces: Vec<Surface>,
    areas: crate::world_materials::areas::Areas,
    enabled: bool,
}

impl SkyVisibility {
    /// Provision a target at installation even if a server enables its camera later.
    pub(super) fn has_candidates(&self) -> bool {
        self.enabled && !self.surfaces.is_empty()
    }

    /// Resolve shader policy and conservative geometry bounds once at map installation.
    pub(super) fn new(scene: &FlattenedScene, bsp: &Bsp, shaders: &ShaderCatalog) -> Self {
        let surfaces = scene
            .draws
            .iter()
            .filter(|draw| {
                draw.world_surface
                    && shaders
                        .get(&scene.materials[draw.material].shader)
                        .is_some_and(|shader| shader.sky.is_some())
            })
            .filter_map(|draw| {
                let surface = bsp.render().surfaces().get(draw.surface_index?)?;
                let vertices = &bsp.render().vertices()[surface.vertices.clone()];
                if vertices.is_empty() {
                    return None;
                }
                let center = vertices
                    .iter()
                    .map(|v| Vec3::from_array(v.position))
                    .sum::<Vec3>()
                    / vertices.len() as f32;
                let radius = vertices
                    .iter()
                    .map(|v| center.distance(Vec3::from_array(v.position)))
                    .fold(0.0, f32::max);
                Some(Surface {
                    center,
                    radius,
                    clusters: draw.clusters.clone(),
                })
            })
            .collect();
        Self {
            surfaces,
            areas: crate::world_materials::areas::Areas::new(bsp),
            enabled: std::env::var_os("SJK_SKY_PORTAL").is_none_or(|value| value != "0"),
        }
    }

    /// Skip the extra scene only when every candidate is provably outside visibility.
    pub(super) fn visible(
        &mut self,
        bsp: &Bsp,
        mask: &[u8],
        cluster: Option<usize>,
        matrix: Mat4,
    ) -> bool {
        if !self.enabled || self.surfaces.is_empty() {
            return false;
        }
        // The ordinary sky renderer draws all sky boxes without a valid source cluster.
        // Outside the BSP, do not replace a remote sky with the ordinary fallback by culling.
        if cluster.is_none() {
            return true;
        }
        self.areas.update(mask);
        self.surfaces.iter().any(|surface| {
            self.areas
                .visible(&surface.clusters, cluster, bsp.render().visibility())
                && (!surface.center.is_finite()
                    || !surface.radius.is_finite()
                    || super::sphere_visible(matrix, surface.center, surface.radius))
        })
    }
}
