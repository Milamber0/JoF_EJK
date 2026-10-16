//! Fixed model fixtures use the same texture-aware light extraction as BSP faces.
//! Only map-owned static placements qualify; pickups, movers and actors stay dynamic.
use super::*;
use crate::lamp_lights::{Emitter, Lamp};
use glam::{Quat, Vec3};
use std::collections::BTreeMap;

/// Resolve each model once and cook its emitting triangles after placement. Scaling
/// geometry before integration preserves source area under nonuniform model scales.
pub(super) fn extract(
    bsp: &Bsp,
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    lightmap: &wgpu::TextureView,
) -> Result<Vec<Lamp>, Box<dyn Error>> {
    let mut models = BTreeMap::<String, Vec<crate::static_models::Spawn>>::new();
    for spawn in crate::static_models::spawns(bsp) {
        if valid(&spawn) {
            models.entry(spawn.model.clone()).or_default().push(spawn);
        }
    }
    let mut cache = ImageCache::new();
    let mut lamps = Vec::new();
    for (model, placements) in models {
        let mut geometry = crate::scene_flatten::FlattenedScene::default();
        let appearance = sjk_runtime::Appearance {
            model,
            variant: String::new(),
        };
        let mesh = match crate::object_meshes::load_one(vfs, &appearance, &mut geometry) {
            Ok(mesh) => mesh,
            Err(error) => {
                eprintln!(
                    "could not load static light fixture {}: {error}",
                    appearance.model
                );
                continue;
            }
        };
        for draw in mesh.draws {
            let key = &geometry.materials[draw.material];
            let Some(definition) = shaders.get(&key.shader) else {
                continue;
            };
            if !definition.has_color_pass()
                || definition.sky.is_some()
                || !definition.deforms.is_empty()
            {
                continue;
            }
            let material = compile_material(
                vfs,
                shaders,
                key,
                lightmap,
                true,
                Default::default(),
                &mut cache,
                true,
            )?;
            if !material.emission.iter().any(|c| *c > 0.) {
                continue;
            }
            let source = &geometry.indices[draw.indices.start as usize..draw.indices.end as usize];
            let mut corners = Vec::new();
            let mut emitters = Vec::new();
            for placement in &placements {
                let rotation =
                    Quat::from_array(sjk_client::legacy_angles_to_quaternion(placement.angles));
                let scale = Vec3::from_array(placement.scale);
                let origin = Vec3::from_array(placement.origin);
                let start = u32::try_from(corners.len())?;
                corners.extend(source.iter().map(|&index| {
                    let vertex = &geometry.vertices[index as usize];
                    (
                        (origin + rotation * (Vec3::from_array(vertex.position) * scale))
                            .to_array(),
                        (rotation * (Vec3::from_array(vertex.normal) / scale))
                            .normalize_or_zero()
                            .to_array(),
                        vertex.texture_coordinates,
                    )
                }));
                emitters.push(Emitter {
                    shared_reach: true,
                    omnidirectional: material.sort != SORT_OPAQUE,
                    texture: &material.emission_texture,
                    radiance: material.emission,
                    ranges: vec![start..u32::try_from(corners.len())?],
                });
            }
            let count = u32::try_from(corners.len())?;
            let indices: Vec<u32> = (0..count).collect();
            lamps.extend(crate::lamp_lights::collect_patches(
                &corners, &indices, &emitters,
            ));
        }
    }
    crate::log::progress(format_args!("Static model fixture lamps: {}", lamps.len()));
    Ok(lamps)
}

fn valid(spawn: &crate::static_models::Spawn) -> bool {
    spawn
        .origin
        .iter()
        .chain(&spawn.angles)
        .chain(&spawn.scale)
        .all(|v| v.is_finite())
        && spawn.scale.iter().all(|v| v.abs() > 1e-6)
}
