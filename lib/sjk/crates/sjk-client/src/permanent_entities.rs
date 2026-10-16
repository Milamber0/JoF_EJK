//! Baseline-only scenery: codemp CG_TransitionPermanent / CG_BuildSolidList.
//! SV_AddEntitiesVisibleFromPoint omits EF_PERMANENT from ordinary snapshots.
use sjk_protocol::{EntityState, GameState, Snapshot};

/// Whether a permanent baseline participates in the current local scene.
/// Stock uses a 5,500-unit origin radius, except terrain is always retained.
pub fn legacy_permanent_visible(entity: &EntityState, player_origin: [f32; 3]) -> bool {
    if entity.e_flags() & (1 << 7) == 0 {
        return false;
    }
    let origin = entity.trajectory_base();
    entity.entity_type() == 16
        || (0..3)
            .map(|i| {
                let distance = origin[i] - player_origin[i];
                distance * distance
            })
            .sum::<f32>()
            <= 5500.0 * 5500.0
}

/// Snapshot entities followed by visible baseline-only permanent entities.
/// No allocation; a snapshot update takes precedence if a server sends one.
pub fn legacy_scene_entities<'a>(
    game: &'a GameState,
    snapshot: &'a Snapshot,
) -> impl Iterator<Item = &'a EntityState> + 'a {
    snapshot
        .entities
        .iter()
        .chain(game.baselines().filter(move |entity| {
            legacy_permanent_visible(entity, snapshot.player.origin())
                && snapshot
                    .entities
                    .binary_search_by_key(&entity.number(), EntityState::number)
                    .is_err()
        }))
}
