//! Opaque brush volumes for light views, including their hidden boundaries.
use sjk_bsp::Bsp;

/// Position-only triangles for static opaque solids. Visible faces alone form
/// open shells after BSP compilation and cannot represent their shadow volume.
#[derive(Default)]
pub struct ShadowHulls {
    /// Three consecutive positions make one triangle; no material or camera PVS.
    pub positions: Vec<[f32; 3]>,
}

impl ShadowHulls {
    /// Build world-model hulls that have a retained opaque render side. The caller
    /// classifies render surfaces using its resolved material policy. Sky brushes,
    /// collision-only volumes, and brushes with a translucent render side are excluded.
    /// Inline models remain the responsibility of their transformed scene instances.
    pub fn build(bsp: &Bsp, opaque_surface: impl Fn(usize) -> bool) -> Self {
        const CONTENTS_SOLID: u32 = 1;
        let mut result = Self::default();
        let Some(model) = bsp.render().models().first() else {
            return result;
        };
        for index in model.brushes.clone() {
            let brush = &bsp.brushes()[index];
            if brush.content_flags & CONTENTS_SOLID == 0 {
                continue;
            }
            let mut opaque = false;
            let mut excluded = false;
            for side in &bsp.brush_sides()[brush.sides.clone()] {
                let shader = &bsp.shaders()[side.shader];
                if shader.surface_flags & super::SURFACE_SKY != 0 {
                    excluded = true;
                    break;
                }
                // Unreferenced sides commonly store zero, not -1. Only accept a
                // matching retained render surface as evidence of a visible solid.
                let Some(surface) = usize::try_from(side.draw_surface)
                    .ok()
                    .and_then(|i| bsp.render().surfaces().get(i).map(|s| (i, s)))
                    .filter(|(_, s)| {
                        s.shader == side.shader && shader.surface_flags & super::SURFACE_NODRAW == 0
                    })
                else {
                    continue;
                };
                let vertices = &bsp.render().vertices()[surface.1.vertices.clone()];
                let plane = bsp.planes()[side.plane];
                if vertices.is_empty()
                    || vertices
                        .iter()
                        .any(|v| plane.signed_distance(v.position).abs() > 0.1)
                {
                    continue;
                }
                if !opaque_surface(surface.0) {
                    excluded = true;
                    break;
                }
                opaque = true;
            }
            if !opaque || excluded {
                continue;
            }
            for side in brush.sides.clone() {
                let points = bsp.brush_side_polygon(index, side);
                for i in 2..points.len() {
                    result
                        .positions
                        .extend([points[0], points[i - 1], points[i]]);
                }
            }
        }
        result
    }
}
