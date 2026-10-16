//! Client-side `misc_model_static` props — JKA's `SP_misc_model_static`
//! (`codemp/cgame/cg_spawn.c:170-233`) and `CG_DrawMiscStaticModels`
//! (`codemp/cgame/cg_draw.c:8348-8394`).
//!
//! The server frees these entities at spawn (`g_misc.c:302-305`), so they
//! never arrive over the network: the client rebuilds them from the BSP
//! entity lump. Their transform is `AnglesToAxis(angles)` with each axis row
//! scaled by `modelscale_vec` (or uniform `modelscale`), i.e. model-space
//! scale followed by rotation, which is exactly what the entity pipeline's
//! `rotate(rotation, position * scale)` computes. `zoffset` only moves the
//! stock cull point, not the model, so it is ignored here. Lighting is
//! sampled at the origin like `lightingOrigin` in the stock draw.

use super::{ActorInstance, StaticModelMesh};
use sjk_bsp::Bsp;
use sjk_client::legacy_angles_to_quaternion;
use sjk_entity::{Entity, parse_entity_lump};
use sjk_runtime::Appearance;
use std::collections::{BTreeSet, HashMap};

const CLASSNAME: &str = "misc_model_static";

/// One placed prop: the rigid mesh it draws and its constant instance.
#[derive(Clone, Copy, Debug)]
struct Placement {
    mesh: usize,
    instance: ActorInstance,
}

/// Constant prop placements resolved against the loaded rigid meshes.
#[derive(Debug, Default)]
pub(crate) struct StaticModels {
    placements: Vec<Placement>,
}

/// Parsed spawn fields of one `misc_model_static` entity.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Spawn {
    pub(crate) model: String,
    pub(crate) origin: [f32; 3],
    /// Pitch, yaw, roll in degrees.
    pub(crate) angles: [f32; 3],
    pub(crate) scale: [f32; 3],
}

impl Spawn {
    /// `SP_misc_model_static` field resolution; `None` when the entity is not
    /// a static model or names no model (stock drops the map for that).
    pub(crate) fn parse(entity: &Entity) -> Option<Self> {
        if entity.classname() != Some(CLASSNAME) {
            return None;
        }
        Self::placement(entity)
    }

    /// The same fields of any entity naming a model, whatever its class: where world
    /// notes look for the model under the crosshair (`world_notes`).
    pub(crate) fn placement(entity: &Entity) -> Option<Self> {
        let model = entity.get("model").filter(|model| !model.is_empty())?;
        let origin = entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]);
        let angles = match entity.vector("angles").ok().flatten() {
            Some(angles) => angles,
            None => [
                0.0,
                entity.number("angle").ok().flatten().unwrap_or(0.0),
                0.0,
            ],
        };
        let scale = match entity.vector("modelscale_vec").ok().flatten() {
            Some(scale) => scale,
            None => [entity.number("modelscale").ok().flatten().unwrap_or(1.0); 3],
        };
        Some(Self {
            model: model.to_owned(),
            origin,
            angles,
            scale,
        })
    }

    fn instance(&self) -> ActorInstance {
        ActorInstance::new(
            self.origin,
            legacy_angles_to_quaternion(self.angles),
            self.scale,
        )
    }
}

/// Every static prop the map places, in entity-lump order.
pub(crate) fn spawns(bsp: &Bsp) -> Vec<Spawn> {
    parse_entity_lump(bsp.entities())
        .map(|entities| entities.iter().filter_map(Spawn::parse).collect())
        .unwrap_or_default()
}

/// Add the prop models to the rigid-mesh load set.
pub(crate) fn extend_appearances(bsp: &Bsp, appearances: &mut BTreeSet<Appearance>) {
    appearances.extend(spawns(bsp).into_iter().map(|spawn| Appearance {
        model: spawn.model,
        variant: String::new(),
    }));
}

impl StaticModels {
    /// Resolve placements against the loaded meshes; props whose model failed
    /// to load are skipped (stock errors out of the map instead).
    pub(crate) fn build(bsp: &Bsp, meshes: &[StaticModelMesh]) -> Self {
        let by_model = meshes
            .iter()
            .enumerate()
            .filter(|(_, mesh)| mesh.appearance.variant.is_empty())
            .map(|(index, mesh)| (mesh.appearance.model.as_str(), index))
            .collect::<HashMap<_, _>>();
        let placements = spawns(bsp)
            .iter()
            .filter_map(|spawn| {
                let mesh = *by_model.get(spawn.model.as_str())?;
                Some(Placement {
                    mesh,
                    instance: spawn.instance(),
                })
            })
            .collect::<Vec<_>>();
        if !placements.is_empty() {
            crate::log::progress(format_args!(
                "{} misc_model_static props placed",
                placements.len()
            ));
        }
        Self { placements }
    }

    /// Submit every prop for this frame. Allocation-free once the groups have
    /// grown to their steady size.
    pub(crate) fn append_instances(&self, groups: &mut [Vec<ActorInstance>]) {
        for placement in &self.placements {
            groups[placement.mesh].push(placement.instance);
        }
    }
}
