//! `PM_Weapon`'s hand-extend block (OpenJK `codemp/game/bg_pmove.c:6759-6960`): while
//! `forceHandExtend` is set, the weapon code does nothing but hold the pose it names —
//! a knockdown's fall and its get-ups among them — and `HANDEXTEND_WEAPONREADY`, which
//! the game sets when the pose is over, raises the weapon again.

use crate::pmove::MovementState;
use crate::pmove_anim::{
    AnimationLengths, SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_LEGS,
    SETANIM_TORSO, set_animation, start_torso,
};

/// `forceHandExtend_t`.
pub const HANDEXTEND_NONE: u8 = 0;
const HANDEXTEND_FORCEPUSH: u8 = 1;
const HANDEXTEND_FORCEPULL: u8 = 2;
const HANDEXTEND_FORCE_HOLD: u8 = 3;
const HANDEXTEND_SABERPULL: u8 = 4;
const HANDEXTEND_CHOKE: u8 = 5;
pub const HANDEXTEND_WEAPONREADY: u8 = 6;
const HANDEXTEND_DODGE: u8 = 7;
pub const HANDEXTEND_KNOCKDOWN: u8 = 8;
const HANDEXTEND_DUELCHALLENGE: u8 = 9;
const HANDEXTEND_TAUNT: u8 = 10;
const HANDEXTEND_PRETHROW: u8 = 11;
const HANDEXTEND_POSTTHROW: u8 = 12;
const HANDEXTEND_PRETHROWN: u8 = 13;
const HANDEXTEND_POSTTHROWN: u8 = 14;
const HANDEXTEND_DRAGGING: u8 = 15;
const HANDEXTEND_JEDITAUNT: u8 = 16;
/// The poses.
const BOTH_FORCEPUSH: u16 = 1_331;
const BOTH_FORCEPULL: u16 = 1_332;
const BOTH_FORCEGRIP_HOLD: u16 = 1_346;
const BOTH_FORCELIGHTNING_HOLD: u16 = 1_337;
const BOTH_FORCE_2HANDEDLIGHTNING_HOLD: u16 = 1_353;
const BOTH_SABERPULL: u16 = 1_342;
const BOTH_CHOKE3: u16 = 1_322;
pub const BOTH_KNOCKDOWN1: u16 = 1_219;
pub const BOTH_KNOCKDOWN5: u16 = 1_223;
pub const BOTH_GETUP1: u16 = 1_224;
const BOTH_FORCE_GETUP_B1: u16 = 1_233;
const BOTH_FORCE_GETUP_B3: u16 = 1_235;
const BOTH_FORCE_GETUP_F2: u16 = 1_232;
const BOTH_ENGAGETAUNT: u16 = 1_187;
const BOTH_A3_TL_BR: u16 = 283;
const BOTH_D3_TL___: u16 = 353;
const BOTH_KNEES1: u16 = 1_099;
const BOTH_B1_BL___: u16 = 195;
const BOTH_GESTURE1: u16 = 963;
const TORSO_RAISEWEAP1: u16 = 1_398;
const BOTH_GUNSIT1: u16 = 1_014;
/// `WP_MELEE`, `WP_SABER`, `WP_EMPLACED_GUN`; `FP_GRIP`, `FP_LIGHTNING`,
/// `FP_DRAIN` bits; `WEAPON_RAISING`; `ENTITYNUM_NONE`.
const WP_MELEE: u8 = 2;
const WP_SABER: u8 = 3;
const WP_EMPLACED_GUN: u8 = 17;
const FP_GRIP: u32 = 1 << 6;
const FP_LIGHTNING: u32 = 1 << 7;
const FP_DRAIN: u32 = 1 << 13;
const WEAPON_RAISING: u8 = 1;
const ENTITY_NONE: u16 = 1_023;

/// `PM_InKnockDown` (`bg_panimate.c:1235-1273`): lying in a knockdown pose, or in one of
/// the get-ups it lists (not the crouched get-ups 1229–1230 nor `BOTH_FORCE_GETUP_B6`)
/// with the legs timer running.
pub(crate) fn in_knockdown(legs_anim: u16, legs_timer: i32) -> bool {
    (BOTH_KNOCKDOWN1..=BOTH_KNOCKDOWN5).contains(&legs_anim)
        || legs_timer != 0 && (matches!(legs_anim, 1_224..=1_228 | 1_231..=1_236 | 1_238..=1_246))
}

/// The block, before anything else `PM_Weapon` does. Returns whether the weapon code
/// is done for this command (a pose held), as the reference's `return` has it.
pub(crate) fn hand_extend(
    state: &mut MovementState,
    lengths: Option<&dyn AnimationLengths>,
) -> bool {
    if state.force_hand_extend == HANDEXTEND_WEAPONREADY && state.vehicle_entity_num == 0 {
        // Back into the weapon's stance: the raise, 250 ms more of weapon time.
        if state.weapon != WP_SABER && state.weapon != WP_MELEE {
            // The scoped disruptor's raise is the same pose (`TORSO_RAISEWEAP1`).
            let pose = if state.weapon == WP_EMPLACED_GUN {
                BOTH_GUNSIT1
            } else {
                TORSO_RAISEWEAP1
            };
            start_torso(state, pose);
        }
        state.weapon_state = WEAPON_RAISING;
        state.weapon_time += 250;
        state.force_hand_extend = HANDEXTEND_NONE;
        return false;
    }
    if state.force_hand_extend == HANDEXTEND_NONE {
        return false;
    }
    let mut separate_torso = None;
    let mut full_body = false;
    let pose = match state.force_hand_extend {
        HANDEXTEND_FORCEPUSH => BOTH_FORCEPUSH,
        HANDEXTEND_FORCEPULL => BOTH_FORCEPULL,
        HANDEXTEND_FORCE_HOLD => {
            if state.force_powers_active & FP_GRIP != 0 {
                BOTH_FORCEGRIP_HOLD
            } else if state.force_powers_active & FP_LIGHTNING != 0 {
                // Two-handed with the fists above level 2.
                if state.weapon == WP_MELEE && state.active_force_pass > 2 {
                    BOTH_FORCE_2HANDEDLIGHTNING_HOLD
                } else {
                    BOTH_FORCELIGHTNING_HOLD
                }
            } else if state.force_powers_active & FP_DRAIN != 0 {
                BOTH_FORCEGRIP_HOLD
            } else {
                BOTH_FORCEGRIP_HOLD
            }
        }
        HANDEXTEND_SABERPULL => BOTH_SABERPULL,
        HANDEXTEND_CHOKE => BOTH_CHOKE3,
        HANDEXTEND_DODGE => state.force_dodge_anim,
        HANDEXTEND_KNOCKDOWN => {
            // `forceDodgeAnim`: 0 lying, 1 the plain get-up, 2 and 3 the Force get-ups;
            // above 4, the legs' get-up (less eight) with a push on the torso.
            if state.force_dodge_anim != 0 {
                if state.force_dodge_anim > 4 {
                    separate_torso = Some(BOTH_FORCEPUSH);
                    match state.force_dodge_anim.wrapping_sub(8) {
                        2 => BOTH_FORCE_GETUP_B1,
                        3 => BOTH_FORCE_GETUP_B3,
                        _ => BOTH_GETUP1,
                    }
                } else {
                    match state.force_dodge_anim {
                        2 => BOTH_FORCE_GETUP_B1,
                        3 => BOTH_FORCE_GETUP_B3,
                        _ => BOTH_GETUP1,
                    }
                }
            } else {
                BOTH_KNOCKDOWN1
            }
        }
        HANDEXTEND_DUELCHALLENGE => BOTH_ENGAGETAUNT,
        HANDEXTEND_TAUNT => {
            let pose = state.force_dodge_anim;
            if pose != BOTH_ENGAGETAUNT
                && state.velocity == [0.0; 3]
                && state.ground_entity_number != ENTITY_NONE
            {
                full_body = true;
            }
            pose
        }
        HANDEXTEND_PRETHROW => {
            full_body = true;
            BOTH_A3_TL_BR
        }
        HANDEXTEND_POSTTHROW => {
            full_body = true;
            BOTH_D3_TL___
        }
        HANDEXTEND_PRETHROWN => {
            full_body = true;
            BOTH_KNEES1
        }
        HANDEXTEND_POSTTHROWN => {
            full_body = true;
            if state.force_dodge_anim != 0 {
                BOTH_FORCE_GETUP_F2
            } else {
                BOTH_KNOCKDOWN5
            }
        }
        HANDEXTEND_DRAGGING => BOTH_B1_BL___,
        HANDEXTEND_JEDITAUNT => BOTH_GESTURE1,
        _ => BOTH_FORCEPUSH,
    };
    let Some(lengths) = lengths else { return true };
    let flags = SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD;
    if separate_torso.is_none() {
        set_animation(state, SETANIM_TORSO, pose, flags, lengths);
        state.torso_timer = 1;
    }
    if full_body {
        set_animation(state, SETANIM_BOTH, pose, flags, lengths);
        state.legs_timer = 1;
        state.torso_timer = 1;
    } else if matches!(
        state.force_hand_extend,
        HANDEXTEND_DODGE | HANDEXTEND_KNOCKDOWN
    ) || (state.force_hand_extend == HANDEXTEND_CHOKE
        && state.ground_entity_number == ENTITY_NONE)
    {
        // The whole body for a dodge or a knockdown (and a choke off the ground).
        set_animation(state, SETANIM_LEGS, pose, flags, lengths);
        state.legs_timer = 1;
        if let Some(torso) = separate_torso {
            set_animation(state, SETANIM_TORSO, torso, flags, lengths);
            state.torso_timer = 1;
        }
    }
    true
}
