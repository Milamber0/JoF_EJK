//! Crash-land roll selection from OpenJK `codemp/game/bg_pmove.c:3701-3894`.

use crate::pmove::{MovementCollision, MovementState};
use crate::pmove_anim::{
    AnimationLengths, SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, set_animation,
};
use crate::pmove_roll::{RollRules, in_roll, in_roll_complete, try_roll};
use sjk_protocol::UserCommand;

const BOTH_LAND1: u16 = 1_140;
const BOTH_A7_SOULCAL: u16 = 910;
const TIMER_LAND: i32 = 130;
const PMF_DUCKED: u16 = 1;

/// Apply only `PM_CrashLand`'s roll branch after an airborne-to-ground change.
///
/// Damage/events and force-jump landing clips remain server-authored; this
/// bounded port covers `bg_pmove.c:3866-3894`, including the completed-roll
/// fallback to `BOTH_LAND1` and `TIMER_LAND` from `bg_local.h:30`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn crash_land_roll(
    state: &mut MovementState,
    command: &UserCommand,
    collision: &impl MovementCollision,
    lengths: Option<&dyn AnimationLengths>,
    rules: RollRules,
    previous_origin: [f32; 3],
    previous_velocity: [f32; 3],
) {
    let Some(lengths) = lengths else {
        return;
    };
    let distance = state.origin[2] - previous_origin[2];
    let velocity = previous_velocity[2];
    let acceleration = -state.gravity;
    let a = acceleration * 0.5;
    let discriminant = velocity * velocity - 4.0 * a * -distance;
    if discriminant < 0.0 || a == 0.0 {
        state.in_air_animation = false;
        return;
    }
    let time = (-velocity - discriminant.sqrt()) / (2.0 * a);
    let impact_velocity = velocity + time * acceleration;
    let mut delta = impact_velocity * impact_velocity * 0.0001;
    if state.movement_flags & PMF_DUCKED != 0 {
        delta *= 2.0;
    }
    if state.movement_flags & PMF_DUCKED == 0
        || delta < 2.0
        || crate::pmove_roll_anim::on_ground(state.legs_anim)
        || in_roll(state)
        || state.force_hand_extend != 0
    {
        state.in_air_animation = false;
        return;
    }
    if in_roll_complete(state) {
        state.legs_timer = 0;
        state.legs_anim = 0;
        set_animation(
            state,
            SETANIM_BOTH,
            BOTH_LAND1,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            lengths,
        );
        state.legs_timer = TIMER_LAND;
    } else if let Some(animation) = try_roll(state, command, collision, rules) {
        state.legs_timer = 0;
        state.legs_anim = 0;
        if state.torso_anim == BOTH_A7_SOULCAL {
            state.torso_timer = 0;
        }
        set_animation(
            state,
            SETANIM_BOTH,
            animation,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            lengths,
        );
    }
    state.in_air_animation = false;
}
