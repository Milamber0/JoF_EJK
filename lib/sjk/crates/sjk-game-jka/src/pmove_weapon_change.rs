//! Ordinary switch lifecycle, shared by gun pmove and the bounded saber exit.
use super::*;
use crate::pmove_anim::{AnimationLengths, SETANIM_FLAG_OVERRIDE, SETANIM_TORSO, set_animation};
use crate::predicted_events::PredictedEvents;

/// codemp/animations.h ordinals, checked against the adapter's animation catalogue.
pub(super) const DROP: u16 = 1396;
/// Ordinary gun raise (the saber draw branch is separate).
pub(super) const RAISE: u16 = 1398;

/// PM_BeginWeaponChange, bg_pmove.c:5798-5825; ownership is never guessed.
pub(super) fn begin(
    state: &mut MovementState,
    weapon: u8,
    lengths: Option<&dyn AnimationLengths>,
    events: &mut PredictedEvents,
) {
    if weapon == WP_NONE
        || usize::from(weapon) >= LEGACY_WEAPON_COUNT
        || state.weapons & (1 << weapon) == 0
        || state.weapon_state == WEAPON_DROPPING
    {
        return;
    }
    if state.zoom_mode != 0 {
        state.zoom_mode = 0;
        state.zoom_time = state.command_time;
    }
    crate::pmove_weapon_charge::event(state, events, 26, u16::from(weapon));
    state.weapon_state = WEAPON_DROPPING;
    state.weapon_time += 200;
    torso(state, DROP, lengths);
    // `BG_ClearRocketLock` (`bg_pmove.c:5782-5790`).
    (
        state.rocket_lock_index,
        state.rocket_lock_time,
        state.rocket_target_time,
    ) = (sjk_protocol::ENTITY_NUMBER_NONE, -1.0, 0.0);
}

/// PM_FinishWeaponChange, bg_pmove.c:5833-5861, ordinary non-saber targets.
pub(super) fn finish(
    state: &mut MovementState,
    requested: u8,
    lengths: Option<&dyn AnimationLengths>,
) {
    let weapon =
        if usize::from(requested) < LEGACY_WEAPON_COUNT && state.weapons & (1 << requested) != 0 {
            requested
        } else {
            WP_NONE
        };
    // The caller's bounded gun gate excludes the distinct LS_DRAW saber branch.
    torso(state, RAISE, lengths);
    state.weapon = weapon;
    state.weapon_state = WEAPON_RAISING;
    state.weapon_time += 250;
}

fn torso(state: &mut MovementState, animation: u16, lengths: Option<&dyn AnimationLengths>) {
    if let Some(lengths) = lengths {
        set_animation(
            state,
            SETANIM_TORSO,
            animation,
            SETANIM_FLAG_OVERRIDE,
            lengths,
        );
    }
}

/// Return the torso to the ordinary ready pose after raising, bg_pmove.c:7248-7280:
/// the fists take the legs' pose, the scoped disruptor `TORSO_WEAPONREADY4`, every
/// other weapon its `WeaponReadyAnim` (the thrown weapons `TORSO_WEAPONREADY10`).
/// Vehicles and emplaced guns are excluded by the caller.
pub(super) fn raised(state: &mut MovementState, _lengths: Option<&dyn AnimationLengths>) {
    // `PM_CanSetWeaponAnims`: not on a vehicle.
    if state.vehicle_entity_num != 0 {
        return;
    }
    let animation = match state.weapon {
        2 => state.legs_anim,
        6 if state.zoom_mode == 1 => 1403,
        _ => crate::pmove_locomotion::weapon_ready_torso(state.weapon),
    };
    // `PM_StartTorsoAnim`: the pose set outright, restarted (flipped) where it is the one
    // already playing.
    crate::pmove_anim::start_torso(state, animation);
}
