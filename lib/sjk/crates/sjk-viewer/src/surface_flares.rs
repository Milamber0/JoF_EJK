//! BSP flare points represented as shared-buffer quads. The flare-only GPU
//! path expands them and samples the finished scene depth without readback.
use super::*;
pub(crate) const PREFIX: &str = "@sjk-flare/";

pub(super) fn append(scene: &mut FlattenedScene, bsp: &Bsp) -> Result<(), Box<dyn Error>> {
    let root = &bsp.render().models()[0].surfaces;
    let mut material_by_shader = std::collections::HashMap::new();
    for (index, surface) in bsp.render().surfaces().iter().enumerate() {
        if surface.kind != sjk_bsp::SurfaceKind::Flare
            || bsp.shaders()[surface.shader].surface_flags & 0x0020_0000 != 0
        {
            continue;
        }
        let material = *material_by_shader.entry(surface.shader).or_insert_with(|| {
            let material = scene.materials.len();
            scene.materials.push(ViewerMaterial {
                shader: format!("{PREFIX}{}", bsp.shaders()[surface.shader].name_lossy()),
                lightmap: -3,
            });
            material
        });
        let base = u32::try_from(scene.vertices.len())?;
        let start = u32::try_from(scene.indices.len())?;
        for uv in [[0., 0.], [1., 0.], [1., 1.], [0., 1.]] {
            scene.vertices.push(GpuVertex {
                position: surface.lightmap_origin,
                normal: surface.lightmap_vectors[2],
                color: [1.; 4],
                texture_coordinates: uv,
                lightmap_coordinates: uv,
            });
        }
        scene
            .indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        let mut clusters = Vec::new();
        for (leaf_index, leaf) in bsp.leaves().iter().enumerate() {
            if bsp
                .leaf_surface_indices(leaf_index)
                .is_some_and(|surfaces| surfaces.contains(&index))
            {
                if let Ok(cluster) = usize::try_from(leaf.cluster) {
                    if !clusters.contains(&cluster) {
                        clusters.push(cluster);
                    }
                }
            }
        }
        scene.draws.push(DrawBatch {
            indices: start..start + 6,
            material,
            clusters,
            surface_index: Some(index),
            world_surface: root.contains(&index),
            bounds: crate::scene_flatten::index_bounds(
                scene.indices[start as usize..start as usize + 6]
                    .iter()
                    .map(|&i| scene.vertices[i as usize].position),
            ),
        });
    }
    Ok(())
}
