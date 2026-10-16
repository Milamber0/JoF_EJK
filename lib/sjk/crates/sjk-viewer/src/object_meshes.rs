//! Load-time collection of the rigid network models a session can draw:
//! items, held and view weapons, projectiles, effect models and the
//! currently referenced rigid entity appearances, plus the map's `misc_model_static` props.

use super::*;

/// Reserve carrier instances in addition to ordinary pickups before the first frame.
pub(super) fn instance_groups(meshes: &[StaticModelMesh]) -> Vec<Vec<ActorInstance>> {
    meshes
        .iter()
        .map(|mesh| {
            let flag = actor_world_submission::flags::MODELS
                .iter()
                .flatten()
                .any(|name| *name == mesh.appearance.model);
            Vec::with_capacity(if flag {
                2 * sjk_protocol::MAX_LEGACY_ENTITIES
            } else {
                32
            })
        })
        .collect()
}

/// A rigid (non-skinned) model: its draws into the shared geometry buffers,
/// the frame-0 bounds centre and, for guns, the `tag_flash` bolt.
pub(super) struct StaticModelMesh {
    pub(super) appearance: Appearance,
    pub(super) draws: Vec<ActorDraw>,
    pub(super) center: [f32; 3],
    pub(super) flash_bolt: Option<bolt::BoltMatrix>,
}

/// Gather preload defaults and the initial world's rigid appearances and append
/// its frame-0 mesh to `flattened`. Models that fail to load are reported and
/// skipped so one broken asset cannot take the map down.
pub(super) fn load<'a>(
    vfs: &VirtualFileSystem,
    bsp: &Bsp,
    world: &World,
    game_state: Option<&GameState>,
    effect_names: impl Iterator<Item = &'a str>,
    vehicle_models: impl Iterator<Item = &'a str>,
    flattened: &mut FlattenedScene,
) -> Vec<StaticModelMesh> {
    let mut appearances = world
        .entities()
        .filter(|entity| !matches!(entity.kind, EntityKind::Actor | EntityKind::Corpse))
        .filter_map(|entity| entity.appearance().cloned())
        .collect::<BTreeSet<_>>();
    pickups::extend_appearances(&mut appearances);
    appearances.insert(Appearance {
        model: "models/weaphits/testboom.md3".to_owned(),
        variant: String::new(),
    });
    appearances.insert(Appearance {
        model: crate::illuminate::MODEL.to_owned(),
        variant: String::new(),
    });
    // The Profile screen's Holocrons tab: a model per tier and the locked look.
    appearances.extend(crate::holocrons::stage::models().map(|model| Appearance {
        model: model.to_owned(),
        variant: String::new(),
    }));
    static_models::extend_appearances(bsp, &mut appearances);
    crate::mover_models::extend_appearances(game_state, &mut appearances);
    first_person_weapon::Catalog::extend_appearances(&mut appearances);
    appearances.extend(
        (1..=16)
            .filter_map(weapon_view::held_model)
            .chain(projectiles::model_paths())
            .map(str::to_owned)
            .chain(vehicle_models.map(str::to_owned))
            .chain(effect_assets::required_models(vfs, effect_names))
            .map(|model| Appearance {
                model,
                variant: String::new(),
            }),
    );
    // CS_MODELS advertises possibilities, not the models in this snapshot.
    // Unused entries load through model_demand when an entity needs them.
    let mut meshes = Vec::with_capacity(appearances.len());
    for appearance in appearances {
        // Brush models, vehicle names, NPC saber names and NPC bodies are
        // not rigid models (`CG_RegisterGraphics`, cg_main.c; NPC bodies
        // load through `npc_refresh` as deformable actors).
        if appearance.model.starts_with(['*', '$', '@'])
            || sjk_client::legacy_npc_body_model(&appearance.model)
        {
            continue;
        }
        match load_one(vfs, &appearance, flattened) {
            Ok(mesh) => meshes.push(mesh),
            Err(error) => {
                eprintln!("could not load network model {}: {error}", appearance.model)
            }
        }
    }
    // The jetpack's file is a `model.glm`, which the loop above takes for an NPC body.
    let jetpack = Appearance {
        model: crate::jetpack::MODEL.to_owned(),
        variant: String::new(),
    };
    match load_one(vfs, &jetpack, flattened) {
        Ok(mesh) => meshes.push(mesh),
        Err(error) => eprintln!("could not load the jetpack {}: {error}", jetpack.model),
    }
    meshes
}

/// Parse one rigid model and append its bind mesh to CPU geometry.
pub(super) fn load_one(
    vfs: &VirtualFileSystem,
    appearance: &Appearance,
    flattened: &mut FlattenedScene,
) -> Result<StaticModelMesh, Box<dyn Error>> {
    let asset = vfs
        .read(&appearance.model)?
        .ok_or("network model asset was not found")?;
    let vertex_start = flattened.vertices.len();
    let (draws, flash_bolt) = if appearance.model.to_ascii_lowercase().ends_with(".glm") {
        let model = Glm::parse(&asset.bytes)?;
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ];
        let pose = vec![identity; model.bone_count];
        let flash = model.surface_bolt_matrix(muzzle_flash::FLASH_BOLT, 0, &pose)?;
        (append_static_glm_mesh(flattened, &model)?, flash)
    } else {
        let model = Md3::parse(&asset.bytes)?;
        (append_md3_mesh(flattened, &model)?, None)
    };
    Ok(StaticModelMesh {
        appearance: appearance.clone(),
        draws,
        center: mesh_center(&flattened.vertices[vertex_start..]),
        flash_bolt,
    })
}
