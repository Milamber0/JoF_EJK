//! Inline BSP mover presentation through the world material pipeline.
//!
//! `collect` mirrors `CG_CalcEntityLerpPositions` in
//! `codemp/cgame/cg_ents.c:3128-3131`: both `pos` and `apos` are evaluated at
//! the presented `cg.time`. Model selection mirrors `CG_General` at
//! `cg_ents.c:2890-2898`, where `SOLID_BMODEL` selects `inlineDrawModel`.

use super::scene_flatten::FlattenedScene;
use super::{ActorDraw, ActorInstance};
use sjk_bsp::Bsp;

use sjk_client::{LegacyMoverPresentation, legacy_present_mover};
use sjk_protocol::Snapshot;

#[path = "brush_scene.rs"]
mod scene;
pub(crate) use scene::append_frame;

/// One renderable partition of the map's existing mesh: an inline model.
pub(crate) struct Mesh {
    pub(crate) model_index: Option<usize>,
    /// The world box the model can move through (`mover_occlusion::reach`).
    pub(crate) reach: [[f32; 3]; 2],
    /// The clusters and areas `reach` touches, to tell a mover the server removed from one
    /// it left out of the snapshot (`mover_occlusion::Sight`).
    pub(crate) sight: crate::world_materials::mover_occlusion::Sight,
    /// It never stops (`mover_occlusion::endless`): it blocks no lamp.
    pub(crate) endless: bool,
    pub(crate) draws: Vec<ActorDraw>,
}

/// Map-initialized mover meshes plus an O(1) inline-model lookup table.
pub(crate) struct Catalog {
    pub(crate) meshes: Vec<Mesh>,
    mesh_by_model: Vec<Option<usize>>,
}

/// An ET_MOVER evaluated at the current presentation time.
pub(crate) type Presented = LegacyMoverPresentation;

/// Build reusable draw ranges for all inline models. Geometry, material
/// indices, shader stages, and lightmaps remain owned by the world pipeline.
pub(crate) fn build_catalog(bsp: &Bsp, flattened: &FlattenedScene) -> Catalog {
    let spawns = spawn_entities(bsp);
    let mut mesh_by_model = vec![None; bsp.render().models().len()];
    let meshes = (1..bsp.render().models().len())
        .filter_map(|model_index| {
            let model = bsp.render().inline_model(model_index)?;
            let draws = flattened
                .draws
                .iter()
                .filter(|draw| {
                    draw.surface_index
                        .is_some_and(|surface| model.surfaces.contains(&surface))
                })
                .map(|draw| ActorDraw {
                    indices: draw.indices.clone(),
                    material: draw.material,
                })
                .collect::<Vec<_>>();
            let entity = spawns.get(&model_index);
            let bounds = &bsp.render().models()[model_index];
            let (lower, upper) = crate::world_materials::mover_occlusion::reach(
                entity,
                glam::Vec3::from_array(bounds.minimums),
                glam::Vec3::from_array(bounds.maximums),
            );
            (!draws.is_empty()).then_some(Mesh {
                model_index: Some(model_index),
                reach: [lower.to_array(), upper.to_array()],
                sight: crate::world_materials::mover_occlusion::Sight::new(bsp, lower, upper),
                endless: crate::world_materials::mover_occlusion::endless(entity),
                draws,
            })
        })
        .collect::<Vec<_>>();
    for (mesh_index, mesh) in meshes.iter().enumerate() {
        if let Some(model_index) = mesh.model_index {
            mesh_by_model[model_index] = Some(mesh_index);
        }
    }
    Catalog {
        meshes,
        mesh_by_model,
    }
}

/// The map's entities that spawn a brush model, by inline model index (`"model" "*N"`).
fn spawn_entities(bsp: &Bsp) -> std::collections::HashMap<usize, sjk_entity::Entity> {
    let Ok(entities) = sjk_entity::parse_entity_lump(bsp.entities()) else {
        return Default::default();
    };
    entities
        .into_iter()
        .filter_map(|entity| {
            let model = entity.get("model")?.strip_prefix('*')?.parse().ok()?;
            Some((model, entity))
        })
        .collect()
}

impl Catalog {
    /// The mesh drawing inline model `model_index`, if it has surfaces.
    pub(crate) fn mesh_of(&self, model_index: usize) -> Option<usize> {
        self.mesh_by_model.get(model_index).copied().flatten()
    }
}

/// Allocate per-mesh instance groups once during map initialization.
pub(crate) fn instance_groups(meshes: &[Mesh]) -> Vec<Vec<ActorInstance>> {
    meshes.iter().map(|_| Vec::with_capacity(4)).collect()
}

/// Collect every decoded mover into a caller-owned fixed-capacity pool.
pub(crate) fn collect(snapshot: &Snapshot, at_time: i32, output: &mut Vec<Presented>) {
    output.clear();
    for state in &snapshot.entities {
        if let Some(mover) = legacy_present_mover(state, at_time) {
            output.push(mover);
        }
    }
}

fn append_instances_with_views(
    presented: &[Presented],
    catalog: &Catalog,
    groups: &mut [Vec<ActorInstance>],
    view_flags: impl Fn(u16) -> u32,
) {
    for mover in presented.iter().filter(|mover| mover.visible) {
        let Some(mesh_index) = catalog
            .mesh_by_model
            .get(mover.model_index)
            .copied()
            .flatten()
        else {
            continue;
        };
        let mut instance = ActorInstance::new(mover.origin, mover.rotation, [1.0; 3]);
        instance.view_flags = view_flags(mover.entity_number);
        groups[mesh_index].push(instance);
    }
}

/// Flatten preallocated mover groups into the shared instance buffer and
/// remember each mesh's draw range.
pub(crate) fn append_groups(
    instances: &mut Vec<ActorInstance>,
    groups: &[Vec<ActorInstance>],
    ranges: &mut Vec<std::ops::Range<u32>>,
) {
    for group in groups {
        ranges.push(crate::actor_instance::append_group(instances, group));
    }
}
