//! The second model of a mover (`CG_Mover`, `codemp/cgame/cg_ents.c:3469-3479`): the md3
//! a map names in a brush entity's `model2` key, which the game registers as
//! `modelindex2` and every client draws at the mover's origin and angles besides (or,
//! for a clip-only brush such as `t2_rancor`'s `func_usable` tables, instead of) its
//! inline model. The rule is [`sjk_client::legacy_mover_model2`]; this module loads the
//! models and submits them with the rigid entity models.

use crate::{ActorInstance, StaticModelMesh};
use sjk_client::{legacy_mover_model2, legacy_present_mover};
use sjk_protocol::{EntityState, GameState, Snapshot};
use sjk_runtime::Appearance;
use std::collections::BTreeSet;

/// `EF_PERMANENT`: sent only in the baselines, never in a snapshot.
const EF_PERMANENT: u32 = 1 << 7;

/// Add the second models of the movers in the baselines to the map's load set.
pub(crate) fn extend_appearances(game: Option<&GameState>, appearances: &mut BTreeSet<Appearance>) {
    let Some(game) = game else { return };
    appearances.extend(
        game.baselines()
            .filter_map(|state| legacy_mover_model2(game, state))
            .map(|model| model.appearance),
    );
}

/// A second model of a presented mover that is not loaded and was not tried yet: the
/// first-use loader (`model_demand`) prepares it.
pub(crate) fn wanted(
    game: &GameState,
    snapshot: &Snapshot,
    tried: impl Fn(&Appearance) -> bool,
) -> Option<Appearance> {
    snapshot
        .entities
        .iter()
        .filter_map(|state| legacy_mover_model2(game, state))
        .map(|model| model.appearance)
        .find(|appearance| !appearance.model.ends_with(".glm") && !tried(appearance))
}

/// Submit this frame's mover models: the snapshot's movers, then the permanent ones
/// the baselines hold (the same set `brush_scene` draws brushes for).
pub(crate) fn append_frame(
    meshes: &[StaticModelMesh],
    groups: &mut [Vec<ActorInstance>],
    game: &GameState,
    snapshot: &Snapshot,
    time: i32,
) {
    for state in &snapshot.entities {
        submit(meshes, groups, game, state, time, || {
            crate::actor_instance::legacy_render_flags(state)
        });
    }
    for state in game.baselines().filter(|state| {
        state.e_flags() & EF_PERMANENT != 0
            && sjk_client::legacy_permanent_visible(state, snapshot.player.origin())
            && snapshot
                .entities
                .binary_search_by_key(&state.number(), |e| e.number())
                .is_err()
    }) {
        submit(meshes, groups, game, state, time, || {
            crate::actor_instance::legacy_render_flags(state)
        });
    }
}

fn submit(
    meshes: &[StaticModelMesh],
    groups: &mut [Vec<ActorInstance>],
    game: &GameState,
    state: &EntityState,
    time: i32,
    view_flags: impl FnOnce() -> u32,
) {
    let Some(model) = legacy_mover_model2(game, state) else {
        return;
    };
    let Some(mover) = legacy_present_mover(state, time) else {
        return;
    };
    let Some(mesh) = meshes
        .iter()
        .position(|mesh| mesh.appearance == model.appearance)
    else {
        return;
    };
    let mut instance = ActorInstance::new(mover.origin, mover.rotation, [model.scale; 3]);
    instance.view_flags = view_flags();
    groups[mesh].push(instance);
}
