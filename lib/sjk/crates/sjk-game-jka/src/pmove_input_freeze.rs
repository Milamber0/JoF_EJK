//! Command filtering performed near the head of `PmoveSingle`.
//!
//! The branch order and timer boundaries mirror OpenJK
//! `codemp/game/bg_pmove.c:10224-10430`. Vehicle, held-client and
//! rocket-trooper branches require state the bounded predictor does not carry
//! and remain snapshot-authoritative. A JA+ server's medium flip over and taunts
//! follow [`JaPlusRules`].

use sjk_protocol::{ENTITY_NUMBER_NONE, UserCommand};

use crate::pmove::{MovementCollision, MovementState};
use crate::pmove_anim::{
    AnimationLengths, SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, set_animation,
};
use crate::pmove_japlus::JaPlusRules;
use crate::saber_move_data::movement::*;

const PM_FLOAT: u8 = 2;
const EF_DISINTEGRATION: u32 = 1 << 26;
const PMF_DUCKED: u16 = 1;
const SOLID_CONTENT_MASK: u32 = 1;
const BUTTON_ATTACK: u16 = 1;
const BUTTON_ALT_ATTACK: u16 = 128;
const BUTTON_FORCE_GRIP: u16 = 64;
const BUTTON_FORCE_POWER: u16 = 512;
const BUTTON_FORCE_LIGHTNING: u16 = 1_024;
const BUTTON_FORCE_DRAIN: u16 = 2_048;
const FORCE_INPUTS: u16 =
    BUTTON_FORCE_GRIP | BUTTON_FORCE_POWER | BUTTON_FORCE_LIGHTNING | BUTTON_FORCE_DRAIN;
const HAND_EXTEND_NONE: u8 = 0;
const HAND_EXTEND_TAUNT: u8 = 10;
const BOTH_MEDITATE_END: u16 = 1_190;

/// Rewrite one user command using codemp's authored full-body move rules.
pub(crate) fn apply(
    state: &mut MovementState,
    command: &mut UserCommand,
    command_millis: i32,
    collision: &impl MovementCollision,
    animation_lengths: Option<&dyn AnimationLengths>,
    ja_plus: JaPlusRules,
) {
    let had_movement = command.forward_move != 0 || command.right_move != 0 || command.up_move != 0;
    let mut stiffened =
        state.movement_type == PM_FLOAT || state.entity_flags & EF_DISINTEGRATION != 0;
    let mut lock_view = false;

    if !stiffened
        && (saber_lock_break_animation(state.legs_anim)
            || saber_lock_break_animation(state.torso_anim)
            || state.saber_lock_time >= command.server_time)
    {
        stiffened = true;
        lock_view = true;
    } else if !stiffened && freezes_for_back_attack(state) {
        if matches!(
            animation_name(state.legs_anim),
            Some("BOTH_JUMPFLIPSTABDOWN" | "BOTH_JUMPFLIPSLASHDOWN1")
        ) && (901..1_600).contains(&state.legs_timer)
            && ja_plus.flip_over_spins()
        {
            state.view_angles[1] += crate::pmove::frame_seconds(command_millis) * 240.0;
            lock_view = true;
        }
        stiffened = true;
    } else if !stiffened && freezes_for_legs_animation(state.legs_anim) {
        stiffened = true;
    } else if !stiffened && animation_is(state.legs_anim, "BOTH_ROLL_STAB") {
        // `bg_pmove.c:10267-10271`: the roll-stab legs clip also locks the view.
        stiffened = true;
        lock_view = true;
    } else if !stiffened && (kick_move(state.saber_move) || kicking_animation(state.legs_anim)) {
        stiffened = true;
    } else if !stiffened && grapple_animation(state.torso_anim) {
        stiffened = true;
        lock_view = true;
    } else if !stiffened && stab_down(state.saber_move) {
        if state.legs_timer < 800 {
            stiffened = true;
            lock_view = true;
        } else {
            command.right_move = 0;
            command.up_move = 0;
            command.forward_move = 64;
        }
    } else if !stiffened && pull_attack(state.saber_move) {
        stiffened = true;
    } else if !stiffened && in_kata(state) {
        move_for_kata(state, command);
    } else if !stiffened && full_body_taunt(state.legs_anim) && full_body_taunt(state.torso_anim) {
        lock_view = filter_taunt(state, command, &mut stiffened, animation_lengths, ja_plus);
    } else if !stiffened && animation_is(state.legs_anim, "BOTH_MEDITATE_END") {
        if state.legs_timer > 0 {
            command.buttons = 0;
            stiffened = true;
            lock_view = true;
        }
    } else if !stiffened && force_land_animation(state.legs_anim) {
        stiffened = true;
    }

    if state.saber_move == u32::from(LS_A_LUNGE) {
        command.right_move = 0;
        command.up_move = 0;
        command.forward_move = if state.legs_timer > 500 { 127 } else { 0 };
    }
    if state.saber_move == u32::from(LS_A_JUMP_T__B_) {
        if state.ground_entity_number != ENTITY_NUMBER_NONE {
            command.forward_move = 0;
        }
        command.right_move = 0;
        command.up_move = 0;
    }
    if ja_plus.enabled {
        japlus_animation_freeze(state, &mut stiffened, &mut lock_view);
    }
    filter_emplaced(state, command, collision, &mut stiffened);
    if lock_view {
        set_view_angle(state, command);
    }
    if stiffened {
        zero_movement(command);
    }
    // "don't let attack or alt attack if being gripped" (`bg_pmove.c:10471-10475`).
    if state.force_grip_cripple {
        command.buttons &= !(BUTTON_ATTACK | BUTTON_ALT_ATTACK);
    }
    state.input_freeze_active |= had_movement
        && command.forward_move == 0
        && command.right_move == 0
        && command.up_move == 0;
}

fn filter_taunt(
    state: &mut MovementState,
    command: &mut UserCommand,
    stiffened: &mut bool,
    animation_lengths: Option<&dyn AnimationLengths>,
    ja_plus: JaPlusRules,
) -> bool {
    let cancel = command.buttons & (BUTTON_ATTACK | BUTTON_ALT_ATTACK | FORCE_INPUTS) != 0
        || command.up_move != 0;
    if cancel {
        if animation_is(state.legs_anim, "BOTH_MEDITATE")
            && animation_is(state.torso_anim, "BOTH_MEDITATE")
        {
            if let Some(lengths) = animation_lengths {
                set_animation(
                    state,
                    SETANIM_BOTH,
                    BOTH_MEDITATE_END,
                    SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                    lengths,
                );
            } else {
                state.legs_timer = 0;
                state.torso_timer = 0;
            }
        } else {
            state.legs_timer = 0;
            state.torso_timer = 0;
        }
        if state.force_hand_extend == HAND_EXTEND_TAUNT {
            state.force_hand_extend = HAND_EXTEND_NONE;
        }
        return false;
    }
    if animation_is(state.legs_anim, "BOTH_MEDITATE") {
        state.legs_timer = state.legs_timer.max(100);
    }
    if animation_is(state.torso_anim, "BOTH_MEDITATE") {
        if state.torso_timer < 100 {
            // Preserve codemp's authored assignment to the legs timer.
            state.legs_timer = 100;
        }
        state.force_hand_extend = HAND_EXTEND_TAUNT;
        state.force_hand_extend_time = command.server_time.wrapping_add(100);
        if ja_plus.free_taunts() {
            // A JA+ client keeps meditating in place but looks around freely, its
            // buttons kept (`bg_pmove.c:12228-12248` in EternalJK).
            *stiffened = true;
            return false;
        }
    }
    if ja_plus.free_taunts() {
        // Other taunts leave a JA+ client free to move and look (`bg_pmove.c:12249-12266`).
        return false;
    }
    if state.legs_timer > 0 || state.torso_timer > 0 {
        command.buttons = 0;
        *stiffened = true;
        return true;
    }
    false
}

/// Mirrors `PM_SetPMViewAngle(ps, ps->viewangles, ucmd)` (`bg_pmove.c:1311-1322`) so the
/// later `PM_UpdateViewAngles` evaluation preserves the authored locked angle: the view
/// stays as it is, whatever the command turns, `delta_angles` absorbing the difference.
/// `ANGLE2SHORT` multiplies before it divides (`q_shared.h`), as `angle_to_short` does.
pub(crate) fn set_view_angle(state: &mut MovementState, command: &UserCommand) {
    for axis in 0..3 {
        let short = crate::npc_think::angle_to_short(state.view_angles[axis]);
        state.delta_angles[axis] = short.wrapping_sub(command.angles[axis]);
    }
}

fn filter_emplaced(
    state: &mut MovementState,
    command: &UserCommand,
    collision: &impl MovementCollision,
    stiffened: &mut bool,
) {
    if state.emplaced_index == 0 {
        return;
    }
    if command.forward_move < 0 || ground_distance(state, collision) > 32.0 {
        state.emplaced_index = 0;
        state.saber_holstered = 0;
    } else {
        *stiffened = true;
    }
}

fn ground_distance(state: &MovementState, collision: &impl MovementCollision) -> f32 {
    let minimums = [-15.0, -15.0, -24.0];
    let height = if state.movement_flags & PMF_DUCKED != 0 {
        state.crouching_height
    } else {
        state.standing_height
    };
    let maximums = [15.0, 15.0, height];
    let mut end = state.origin;
    end[2] -= 4_096.0;
    let trace = collision.trace(state.origin, minimums, maximums, end, SOLID_CONTENT_MASK);
    glam::Vec3::from_array(state.origin).distance(glam::Vec3::from_array(trace.end_position))
}

fn move_for_kata(state: &mut MovementState, command: &mut UserCommand) {
    if state.saber_move == u32::from(LS_STAFF_SOULCAL)
        && animation_is(state.legs_anim, "BOTH_A7_SOULCAL")
    {
        command.up_move = 0;
        if (251..700).contains(&state.legs_timer) {
            command.up_move = -127;
            command.right_move = 0;
            command.forward_move = command.forward_move.max(0);
        } else {
            command.right_move = 0;
            command.forward_move = if state.legs_timer >= 2_750 { 64 } else { 0 };
        }
        if (2_650..2_850).contains(&state.legs_timer)
            && state.ground_entity_number != ENTITY_NUMBER_NONE
        {
            state.velocity[2] = 250.0;
            state.force_jump_start_height = state.origin[2];
        }
    } else if animation_is(state.legs_anim, "BOTH_A2_SPECIAL") {
        command.right_move = 0;
        command.up_move = 0;
        command.forward_move = if (2_301..2_700).contains(&state.legs_timer)
            || (501..900).contains(&state.legs_timer)
        {
            127
        } else {
            0
        };
    } else if animation_is(state.legs_anim, "BOTH_A3_SPECIAL") {
        command.right_move = 0;
        command.up_move = 0;
        command.forward_move = if (1_001..1_700).contains(&state.legs_timer) {
            127
        } else {
            0
        };
    } else {
        zero_movement(command);
    }
}

fn zero_movement(command: &mut UserCommand) {
    command.forward_move = 0;
    command.right_move = 0;
    command.up_move = 0;
}

fn freezes_for_back_attack(state: &MovementState) -> bool {
    matches!(
        state.saber_move as u16,
        LS_A_BACK
            | LS_A_BACK_CR
            | LS_A_BACKSTAB
            | LS_A_FLIP_STAB
            | LS_A_FLIP_SLASH
            | LS_A_JUMP_T__B_
            | LS_DUAL_LR
            | LS_DUAL_FB
    )
}

fn freezes_for_legs_animation(animation: u16) -> bool {
    matches!(
        animation_name(animation),
        Some(
            "BOTH_A2_STABBACK1"
                | "BOTH_ATTACK_BACK"
                | "BOTH_CROUCHATTACKBACK1"
                | "BOTH_FORCELEAP2_T__B_"
                | "BOTH_JUMPFLIPSTABDOWN"
                | "BOTH_JUMPFLIPSLASHDOWN1"
        )
    )
}

fn stab_down(movement: u32) -> bool {
    matches!(
        movement as u16,
        LS_STABDOWN | LS_STABDOWN_STAFF | LS_STABDOWN_DUAL
    )
}

fn pull_attack(movement: u32) -> bool {
    matches!(movement as u16, LS_PULL_ATTACK_STAB | LS_PULL_ATTACK_SWING)
}

fn kick_move(movement: u32) -> bool {
    matches!(movement as u16, LS_KICK_F..=LS_KICK_L_AIR | LS_HILT_BASH)
}

fn in_kata(state: &MovementState) -> bool {
    matches!(
        state.saber_move as u16,
        LS_A1_SPECIAL | LS_A2_SPECIAL | LS_A3_SPECIAL | LS_DUAL_SPIN_PROTECT | LS_STAFF_SOULCAL
    ) || kata_animation(state.torso_anim)
        || kata_animation(state.legs_anim)
}

/// A JA+ server's own animations hold the player still (JoF EternalJK
/// `bg_pmove.c:12470-12498` `PmoveSingle`, `cgs.serverMod == SVMOD_JAPLUS`, at
/// bd5e202): the victim of a backflip kick, a get-up, a stab, a kiss or a ledge locks
/// the view too; the kicker's spin, back and jumping kicks, the backflip kick and the
/// flip stab only stop the movement. JA+ plays these server-side, so without this the
/// client kept walking through its own kick and was pulled back by every snapshot.
fn japlus_animation_freeze(state: &MovementState, stiffened: &mut bool, lock_view: &mut bool) {
    let legs = animation_name(state.legs_anim);
    let torso = animation_name(state.torso_anim);
    let index = |name: &str| crate::legacy_animation_index(name);
    let in_kiss_or_ledge = matches!(
        (index("BOTH_KISSEE"), index("BOTH_LEDGE_MERCPULL")),
        (Some(first), Some(last)) if (first..=last).contains(&usize::from(state.legs_anim))
    );
    if legs == Some("BOTH_JUMP_BACKFLIP_ATCKEE")
        || matches!(
            torso,
            Some("BOTH_JUMP_BACKFLIP_ATCKEE" | "BOTH_GETUP1" | "BOTH_NEW_STABEE")
        )
        || in_kiss_or_ledge
    {
        *lock_view = true;
        *stiffened = true;
    } else if matches!(
        legs,
        Some(
            "BOTH_MELEE_BACKKICK"
                | "BOTH_MELEE_SPINKICK"
                | "BOTH_JUMP_BACKKICK_SPIN"
                | "BOTH_JUMP_BACKFLIP_ATCK"
                | "BOTH_FLIP_STAB"
                | "BOTH_JUMP_BACKFLIP_ATCK_MISSED"
        )
    ) {
        *stiffened = true;
    }
}

fn animation_is(animation: u16, expected: &str) -> bool {
    animation_name(animation) == Some(expected)
}

fn animation_name(animation: u16) -> Option<&'static str> {
    crate::legacy_animation_name(usize::from(animation))
}

fn saber_lock_break_animation(animation: u16) -> bool {
    let Some(name) = animation_name(animation) else {
        return false;
    };
    matches!(
        name,
        "BOTH_BF1BREAK" | "BOTH_BF2BREAK" | "BOTH_CWCIRCLEBREAK" | "BOTH_CCWCIRCLEBREAK"
    ) || name.starts_with("BOTH_LK_")
        && (name.ends_with("_B_1_L")
            || name.ends_with("_B_1_W")
            || name.ends_with("_SB_1_L")
            || name.ends_with("_SB_1_W"))
}

/// Exact `BG_KickingAnim` set (`bg_panimate.c:664-689`): the kicks, and the get-up rolls
/// forwards and back, which do the kick traces too.
pub(crate) fn kicking_animation(animation: u16) -> bool {
    matches!(
        animation_name(animation),
        Some(
            "BOTH_A7_KICK_F"
                | "BOTH_A7_KICK_B"
                | "BOTH_A7_KICK_R"
                | "BOTH_A7_KICK_L"
                | "BOTH_A7_KICK_S"
                | "BOTH_A7_KICK_BF"
                | "BOTH_A7_KICK_RL"
                | "BOTH_A7_KICK_F_AIR"
                | "BOTH_A7_KICK_B_AIR"
                | "BOTH_A7_KICK_R_AIR"
                | "BOTH_A7_KICK_L_AIR"
                | "BOTH_A7_HILT"
                | "BOTH_GETUP_BROLL_B"
                | "BOTH_GETUP_BROLL_F"
                | "BOTH_GETUP_FROLL_B"
                | "BOTH_GETUP_FROLL_F"
        )
    )
}

fn grapple_animation(animation: u16) -> bool {
    matches!(
        animation_name(animation),
        Some(
            "BOTH_KYLE_GRAB"
                | "BOTH_KYLE_MISS"
                | "BOTH_KYLE_PA_1"
                | "BOTH_KYLE_PA_2"
                | "BOTH_PLAYER_PA_1"
                | "BOTH_PLAYER_PA_2"
                | "BOTH_PLAYER_PA_FLY"
        )
    )
}

fn kata_animation(animation: u16) -> bool {
    matches!(
        animation_name(animation),
        Some(
            "BOTH_A6_SABERPROTECT"
                | "BOTH_A7_SOULCAL"
                | "BOTH_A1_SPECIAL"
                | "BOTH_A2_SPECIAL"
                | "BOTH_A3_SPECIAL"
        )
    )
}

pub(crate) fn full_body_taunt(animation: u16) -> bool {
    matches!(
        animation_name(animation),
        Some(
            "BOTH_GESTURE1"
                | "BOTH_DUAL_TAUNT"
                | "BOTH_STAFF_TAUNT"
                | "BOTH_BOW"
                | "BOTH_MEDITATE"
                | "BOTH_SHOWOFF_FAST"
                | "BOTH_SHOWOFF_MEDIUM"
                | "BOTH_SHOWOFF_STRONG"
                | "BOTH_SHOWOFF_DUAL"
                | "BOTH_SHOWOFF_STAFF"
                | "BOTH_VICTORY_FAST"
                | "BOTH_VICTORY_MEDIUM"
                | "BOTH_VICTORY_STRONG"
                | "BOTH_VICTORY_DUAL"
                | "BOTH_VICTORY_STAFF"
        )
    )
}

fn force_land_animation(animation: u16) -> bool {
    matches!(
        animation_name(animation),
        Some(
            "BOTH_FORCELAND1"
                | "BOTH_FORCELANDBACK1"
                | "BOTH_FORCELANDRIGHT1"
                | "BOTH_FORCELANDLEFT1"
        )
    )
}
