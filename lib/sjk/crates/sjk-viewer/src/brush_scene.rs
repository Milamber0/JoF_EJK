//! Inline BSP models use the world-material pipeline. Like codemp, which
//! draws an inline model only from `CG_Mover`, only `ET_MOVER`s draw one:
//! snapshot movers come from `movers::collect`, permanent BSP instances from
//! baselines rather than network snapshots.
use super::*;
use crate::{GpuState, first_person_view};

pub(crate) fn append_frame(gpu: &mut GpuState, time: i64) {
    let game = gpu
        .live_session
        .as_ref()
        .map(|s| s.game_state())
        .or_else(|| gpu.demo_session.as_ref().map(|s| s.game_state()))
        .or_else(|| gpu.resident.scenery.as_ref().map(|(game, _)| game));
    let snapshot = first_person_view::presented_snapshot(
        gpu.live_session.as_ref(),
        gpu.demo_session.as_ref(),
        time as i32,
    )
    .or_else(|| gpu.resident.scenery.as_ref().map(|(_, snapshot)| snapshot));
    append_instances_with_views(
        &gpu.movers,
        &gpu.mover_catalog,
        &mut gpu.mover_groups,
        |number| crate::actor_instance::scene_flags(game, snapshot, number),
    );
    /// `EF_PERMANENT` (`codemp/game/q_shared.h`): sent only in the baselines, never in a
    /// snapshot (`permanent_entities.rs`).
    const EF_PERMANENT: u32 = 1 << 7;
    let catalog = &gpu.mover_catalog;
    // `gpu.movers` holds the movers of the presented live or demo snapshot; the server
    // built it from the player's eye (`SV_BuildClientSnapshot`).
    let eye = first_person_view::presented_snapshot(
        gpu.live_session.as_ref(),
        gpu.demo_session.as_ref(),
        time as i32,
    )
    .map(|snapshot| {
        let mut eye = snapshot.player.origin();
        eye[2] += snapshot.player.view_height() as f32;
        crate::world_materials::mover_occlusion::Eye {
            cluster: usize::try_from(gpu.bsp.leaves()[gpu.bsp.leaf_at(eye)].cluster).ok(),
            visibility: gpu.bsp.render().visibility(),
        }
    });
    gpu.world_materials.observe_movers(
        &gpu.queue,
        &gpu.movers,
        game.map(|game| {
            game.baselines().filter_map(move |state| {
                legacy_present_mover(state, time as i32)
                    .map(|mover| (mover, state.e_flags() & EF_PERMANENT != 0))
            })
        }),
        |model_index| catalog.mesh_of(model_index),
        eye,
    );
    if let (Some(game), Some(snapshot)) = (game, snapshot) {
        for state in game.baselines().filter(|state| {
            sjk_client::legacy_permanent_visible(state, snapshot.player.origin())
                && snapshot
                    .entities
                    .binary_search_by_key(&state.number(), |e| e.number())
                    .is_err()
        }) {
            if let Some(mover) = legacy_present_mover(state, time as i32)
                && mover.visible
                && let Some(mesh) = gpu
                    .mover_catalog
                    .mesh_by_model
                    .get(mover.model_index)
                    .copied()
                    .flatten()
            {
                let mut instance = ActorInstance::new(mover.origin, mover.rotation, [1.0; 3]);
                instance.view_flags =
                    ActorInstance::WORLD | crate::actor_instance::legacy_render_flags(state);
                gpu.mover_groups[mesh].push(instance);
            }
        }
        crate::mover_models::append_frame(
            &gpu.object_meshes,
            &mut gpu.object_groups,
            game,
            snapshot,
            time as i32,
        );
    }
}
