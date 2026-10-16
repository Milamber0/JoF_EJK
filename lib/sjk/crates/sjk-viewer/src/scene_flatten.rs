//! Flattens the static world and appended meshes into the combined GPU
//! vertex/index buffers, materials and draw batches the viewer uploads once.

use super::*;

#[path = "surface_flares.rs"]
pub(crate) mod flares;
#[path = "surface_sprites.rs"]
pub(crate) mod sprites;

/// Build the world mesh and stage-owned generated geometry before cataloging movers.
pub(super) fn prepare(
    scene: &StaticWorld,
    bsp: &Bsp,
    shaders: &ShaderCatalog,
) -> Result<FlattenedScene, Box<dyn Error>> {
    let mut flat = flatten_scene(scene, bsp)?;
    sprites::append(&mut flat, shaders)?;
    flares::append(&mut flat, bsp)?;
    Ok(flat)
}

#[derive(Default)]
pub(crate) struct FlattenedScene {
    pub(crate) vertices: Vec<GpuVertex>,
    pub(crate) indices: Vec<u32>,
    pub(crate) materials: Vec<ViewerMaterial>,
    pub(crate) draws: Vec<DrawBatch>,
}

/// One BSP face (or appended mesh piece) as an index range of the combined
/// buffers, with the material it draws with and the PVS clusters it is in.
pub(crate) struct DrawBatch {
    pub(crate) indices: Range<u32>,
    pub(crate) material: usize,
    pub(crate) clusters: Vec<usize>,
    pub(crate) surface_index: Option<usize>,
    /// Part of the static world (model 0) and not detached as a prop.
    pub(crate) world_surface: bool,
    /// World-space AABB of the referenced vertices, for light-frustum culling of casters.
    pub(crate) bounds: [[f32; 3]; 2],
}

/// AABB of the vertices an index range references; infinite when the range is empty.
pub(super) fn index_bounds(positions: impl Iterator<Item = [f32; 3]>) -> [[f32; 3]; 2] {
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for p in positions {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(p[axis]);
            hi[axis] = hi[axis].max(p[axis]);
        }
    }
    if lo[0] > hi[0] {
        return [[f32::NEG_INFINITY; 3], [f32::INFINITY; 3]];
    }
    [lo, hi]
}

/// One material's index range of an instanced (mover / actor) mesh.
pub(super) struct ActorDraw {
    pub(super) indices: Range<u32>,
    pub(super) material: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ViewerMaterial {
    pub(super) shader: String,
    pub(super) lightmap: i32,
}

pub(super) fn flatten_scene(
    scene: &StaticWorld,
    bsp: &Bsp,
) -> Result<FlattenedScene, Box<dyn Error>> {
    let mut vertices = Vec::with_capacity(scene.vertex_count());
    let mut indices = Vec::with_capacity(scene.triangle_count() * 3);
    let mut materials = Vec::with_capacity(scene.batches().len());
    let mut draws = Vec::new();
    for (material_index, batch) in scene.batches().iter().enumerate() {
        let base: u32 = vertices.len().try_into()?;
        let index_start: u32 = indices.len().try_into()?;
        vertices.extend(batch.vertices.iter().map(|vertex| GpuVertex {
            position: vertex.position,
            normal: vertex.normal,
            color: vertex.color.map(|channel| f32::from(channel) / 255.0),
            texture_coordinates: vertex.texture_coordinates,
            lightmap_coordinates: vertex.lightmap_coordinates,
        }));
        for index in &batch.indices {
            indices.push(
                base.checked_add(*index)
                    .ok_or("combined mesh index overflow")?,
            );
        }
        materials.push(ViewerMaterial {
            shader: bsp.shaders()[batch.material.shader]
                .name_lossy()
                .into_owned(),
            lightmap: batch.material.lightmap,
        });
        for draw in &batch.draws {
            draws.push(DrawBatch {
                indices: index_start
                    .checked_add(draw.indices.start)
                    .ok_or("draw range overflow")?
                    ..index_start
                        .checked_add(draw.indices.end)
                        .ok_or("draw range overflow")?,
                material: material_index,
                clusters: draw.clusters.clone(),
                surface_index: Some(draw.surface_index),
                world_surface: bsp.render().models()[0]
                    .surfaces
                    .contains(&draw.surface_index),
                bounds: index_bounds(
                    batch.indices[draw.indices.start as usize..draw.indices.end as usize]
                        .iter()
                        .map(|&i| batch.vertices[i as usize].position),
                ),
            });
        }
    }
    Ok(FlattenedScene {
        vertices,
        indices,
        materials,
        draws,
    })
}

pub(super) fn append_player_preview(
    scene: &mut FlattenedScene,
    preview: &PlayerPreview,
    frame: usize,
) -> Result<Vec<PreviewVertexRange>, Box<dyn Error>> {
    let surfaces = preview
        .mesh
        .skin(&preview.animation, &preview.skin, frame, 0)?;
    let mut vertex_ranges = Vec::new();
    for (surface_index, surface) in surfaces.into_iter().enumerate() {
        let Some(shader) = surface.shader.as_ref() else {
            continue;
        };
        let vertex_base: u32 = scene.vertices.len().try_into()?;
        let vertex_start = scene.vertices.len();
        let index_start: u32 = scene.indices.len().try_into()?;
        scene.vertices.extend(
            surface
                .vertices
                .iter()
                .map(|vertex| preview_gpu_vertex(preview, vertex)),
        );
        for triangle in surface.triangles {
            for index in triangle {
                scene.indices.push(
                    vertex_base
                        .checked_add(index)
                        .ok_or("player preview index overflow")?,
                );
            }
        }
        let material = scene.materials.len();
        scene.materials.push(ViewerMaterial {
            shader: shader.clone(),
            lightmap: -1,
        });
        scene.draws.push(DrawBatch {
            indices: index_start..scene.indices.len().try_into()?,
            material,
            clusters: Vec::new(),
            surface_index: None,
            world_surface: true,
            bounds: index_bounds(
                scene.indices[index_start as usize..]
                    .iter()
                    .map(|&i| scene.vertices[i as usize].position),
            ),
        });
        vertex_ranges.push(PreviewVertexRange {
            surface_index,
            vertices: vertex_start..scene.vertices.len(),
        });
    }
    Ok(vertex_ranges)
}

pub(super) fn append_actor_mesh(
    scene: &mut FlattenedScene,
    actor: &PlayerPreview,
    frame: usize,
) -> Result<(Vec<ActorDraw>, Vec<PreviewVertexRange>), Box<dyn Error>> {
    let surfaces = actor.mesh.skin(&actor.animation, &actor.skin, frame, 0)?;
    let mut draws = Vec::new();
    let mut vertex_ranges = Vec::new();
    for (surface_index, surface) in surfaces.into_iter().enumerate() {
        let Some(shader) = surface.shader.as_ref() else {
            continue;
        };
        let vertex_base: u32 = scene.vertices.len().try_into()?;
        let vertex_start = scene.vertices.len();
        let index_start: u32 = scene.indices.len().try_into()?;
        scene.vertices.extend(
            surface
                .vertices
                .iter()
                .map(|vertex| preview_gpu_vertex(actor, vertex)),
        );
        for triangle in surface.triangles {
            for index in triangle {
                scene.indices.push(
                    vertex_base
                        .checked_add(index)
                        .ok_or("actor mesh index overflow")?,
                );
            }
        }
        let material = scene.materials.len();
        scene.materials.push(ViewerMaterial {
            shader: shader.clone(),
            lightmap: -1,
        });
        draws.push(ActorDraw {
            indices: index_start..scene.indices.len().try_into()?,
            material,
        });
        vertex_ranges.push(PreviewVertexRange {
            surface_index,
            vertices: vertex_start..scene.vertices.len(),
        });
    }
    Ok((draws, vertex_ranges))
}

pub(super) fn append_md3_mesh(
    scene: &mut FlattenedScene,
    model: &Md3,
) -> Result<Vec<ActorDraw>, Box<dyn Error>> {
    let mut draws = Vec::new();
    for surface in &model.surfaces {
        let Some(vertices) = surface.frames.first() else {
            continue;
        };
        let Some(shader) = surface.shaders.first() else {
            continue;
        };
        if vertices.len() != surface.texture_coordinates.len() {
            return Err("MD3 position/texture-coordinate count differs".into());
        }
        let vertex_base: u32 = scene.vertices.len().try_into()?;
        let index_start: u32 = scene.indices.len().try_into()?;
        scene
            .vertices
            .extend(vertices.iter().zip(&surface.texture_coordinates).map(
                |(vertex, texture_coordinates)| GpuVertex {
                    position: vertex.position,
                    normal: vertex.normal,
                    color: [1.0; 4],
                    texture_coordinates: *texture_coordinates,
                    lightmap_coordinates: [0.0; 2],
                },
            ));
        for triangle in &surface.triangles {
            for index in triangle {
                scene.indices.push(
                    vertex_base
                        .checked_add(*index)
                        .ok_or("network model index overflow")?,
                );
            }
        }
        let material = scene.materials.len();
        scene.materials.push(ViewerMaterial {
            shader: shader.clone(),
            lightmap: -1,
        });
        draws.push(ActorDraw {
            indices: index_start..scene.indices.len().try_into()?,
            material,
        });
    }
    Ok(draws)
}

pub(super) fn append_static_glm_mesh(
    scene: &mut FlattenedScene,
    model: &Glm,
) -> Result<Vec<ActorDraw>, Box<dyn Error>> {
    let lod = model.lods.first().ok_or("static GLM has no LOD")?;
    let mut draws = Vec::new();
    for (surface, hierarchy) in lod.surfaces.iter().zip(&model.hierarchy) {
        if hierarchy.name.starts_with('*') || hierarchy.flags & 0x2 != 0 {
            continue;
        }
        let vertex_base: u32 = scene.vertices.len().try_into()?;
        let index_start: u32 = scene.indices.len().try_into()?;
        scene
            .vertices
            .extend(surface.vertices.iter().map(|vertex| GpuVertex {
                position: vertex.position,
                normal: vertex.normal,
                color: [1.0; 4],
                texture_coordinates: vertex.texture_coordinates,
                lightmap_coordinates: [0.0; 2],
            }));
        for triangle in &surface.triangles {
            for index in triangle {
                scene.indices.push(
                    vertex_base
                        .checked_add(*index)
                        .ok_or("static GLM index overflow")?,
                );
            }
        }
        let material = scene.materials.len();
        scene.materials.push(ViewerMaterial {
            shader: hierarchy.shader.clone(),
            lightmap: -1,
        });
        draws.push(ActorDraw {
            indices: index_start..scene.indices.len().try_into()?,
            material,
        });
    }
    Ok(draws)
}

pub(super) fn preview_gpu_vertex(preview: &PlayerPreview, vertex: &SkinnedVertex) -> GpuVertex {
    let sine = preview.yaw.sin();
    let cosine = preview.yaw.cos();
    let rotate = |value: [f32; 3]| {
        [
            cosine * value[0] - sine * value[1],
            sine * value[0] + cosine * value[1],
            value[2],
        ]
    };
    let position = rotate(vertex.position);
    GpuVertex {
        position: [
            position[0] + preview.origin[0],
            position[1] + preview.origin[1],
            position[2] + preview.origin[2],
        ],
        normal: rotate(vertex.normal),
        color: [1.0; 4],
        texture_coordinates: vertex.texture_coordinates,
        lightmap_coordinates: [0.0; 2],
    }
}
