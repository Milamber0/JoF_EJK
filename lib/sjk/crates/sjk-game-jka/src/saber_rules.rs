//! The saber's small rules: which saber moves are attacks, parries, bounces and so on;
//! which animations are special attacks, spins or kicks; and how strong a swing is
//! (`G_PowerLevelForSaberAnim`, `codemp/game/w_saber.c:2966`).
//!
//! The predicates are the compiled game's own answers for every saber move and every
//! animation, held as tables (`saber_rules_table.rs`, generated from
//! compiled OpenJK output); the power level's switch is transliterated from
//! the OpenJK reference. What is written by hand
//! here is what reads a timer.

use crate::saber_rules_table::{
    ANIM_FLAGS, BOUNCE_FOR_ATTACK, BROKEN_PARRY, CLASH_FLAGS, DEFLECTION_FOR_QUAD,
    KNOCKAWAY_FOR_PARRY, MOVE_ANIMS, MOVE_FLAGS, MOVE_QUADS, PM_BROKEN_PARRY_FOR_PARRY,
    switch_level,
};

/// `saberType_t`'s `SABER_LANCE` and `SABER_TRIDENT`, the two that change a swing's
/// strength.
pub const SABER_LANCE: i32 = 9;
/// `SABER_TRIDENT`.
pub const SABER_TRIDENT: i32 = 11;
/// `SABER_SINGLE`, the stock saber.
pub const SABER_SINGLE: i32 = 1;
/// `LS_NONE`, and `LS_K1_TR`: `BG_KnockawayForParry`'s default.
const LS_NONE: u32 = 0;
const LS_K1_TR: u32 = 148;

fn move_flag(saber_move: u32, bit: u16) -> bool {
    MOVE_FLAGS
        .get(saber_move as usize)
        .is_some_and(|flags| flags & bit != 0)
}

fn clash_flag(saber_move: u32, bit: u8) -> bool {
    CLASH_FLAGS
        .get(saber_move as usize)
        .is_some_and(|flags| flags & bit != 0)
}

fn anim_flag(animation: u16, bit: u16) -> bool {
    ANIM_FLAGS
        .get(usize::from(animation))
        .is_some_and(|flags| flags & bit != 0)
}

/// `BG_SaberInAttack`.
pub fn in_attack(saber_move: u32) -> bool {
    move_flag(saber_move, 1)
}
/// `BG_SaberInSpecial`.
pub fn in_special(saber_move: u32) -> bool {
    move_flag(saber_move, 1 << 1)
}
/// `PM_SaberInParry`.
pub fn in_parry(saber_move: u32) -> bool {
    move_flag(saber_move, 1 << 2)
}
/// `PM_SaberInBrokenParry`.
pub fn in_broken_parry(saber_move: u32) -> bool {
    move_flag(saber_move, 1 << 3)
}
/// `PM_SaberInDeflect`.
pub fn in_deflect(saber_move: u32) -> bool {
    move_flag(saber_move, 1 << 4)
}
/// `PM_SaberInBounce`.
pub fn in_bounce(saber_move: u32) -> bool {
    move_flag(saber_move, 1 << 5)
}
/// `PM_SaberInKnockaway`.
pub fn in_knockaway(saber_move: u32) -> bool {
    move_flag(saber_move, 1 << 6)
}
/// `BG_SaberInTransitionAny`: a start, a transition or a return.
pub fn in_transition_any(saber_move: u32) -> bool {
    move_flag(saber_move, 1 << 7)
}
/// `BG_SaberInReturn`.
pub fn in_return(saber_move: u32) -> bool {
    move_flag(saber_move, 1 << 8)
}
/// `BG_SaberInKata`.
pub fn in_kata(saber_move: u32) -> bool {
    move_flag(saber_move, 1 << 9)
}
/// `PM_SaberInTransition`.
pub fn in_transition(saber_move: u32) -> bool {
    move_flag(saber_move, 1 << 10)
}
/// `BG_KickMove`.
pub fn kick_move(saber_move: u32) -> bool {
    move_flag(saber_move, 1 << 11)
}
/// `BG_SaberInIdle`: none, ready, drawing or putting away.
pub fn in_idle(saber_move: u32) -> bool {
    move_flag(saber_move, 1 << 12)
}
/// `PM_SaberInStart`.
pub fn in_start(saber_move: u32) -> bool {
    move_flag(saber_move, 1 << 13)
}
/// The movement's `PM_BrokenParryForParry`: `LS_NONE` for a move that is no parry.
pub fn pm_broken_parry_for_parry(saber_move: u32) -> u32 {
    PM_BROKEN_PARRY_FOR_PARRY
        .get(saber_move as usize)
        .map_or(LS_NONE, |&to| u32::from(to))
}
/// `PM_SaberInReflect`.
pub fn in_reflect(saber_move: u32) -> bool {
    clash_flag(saber_move, 1)
}
/// `BG_InExtraDefenseSaberMove`.
pub fn in_extra_defense(saber_move: u32) -> bool {
    clash_flag(saber_move, 1 << 1)
}
/// `BG_SaberInAttackPure`.
pub fn in_attack_pure(saber_move: u32) -> bool {
    clash_flag(saber_move, 1 << 2)
}
/// `saberMoveData[move].startQuad` and `.endQuad`.
pub fn move_quads(saber_move: u32) -> (u8, u8) {
    MOVE_QUADS
        .get(saber_move as usize)
        .copied()
        .unwrap_or((0, 0))
}
/// `PM_SaberBounceForAttack`.
pub fn bounce_for_attack(saber_move: u32) -> u32 {
    BOUNCE_FOR_ATTACK
        .get(saber_move as usize)
        .map_or(LS_NONE, |&to| u32::from(to))
}
/// `BG_BrokenParryForAttack`.
pub fn broken_parry_for_attack(saber_move: u32) -> u32 {
    BROKEN_PARRY
        .get(saber_move as usize)
        .map_or(LS_NONE, |&(to, _)| u32::from(to))
}
/// `BG_BrokenParryForParry`, except for `LS_PARRY_UP`, which draws at random and is the
/// caller's (this answers `LS_NONE` for it).
pub fn broken_parry_for_parry(saber_move: u32) -> u32 {
    BROKEN_PARRY
        .get(saber_move as usize)
        .map_or(LS_NONE, |&(_, to)| u32::try_from(to).unwrap_or(LS_NONE))
}
/// `BG_KnockawayForParry`, which takes a block (`saberBlockedType_t`) despite its name.
pub fn knockaway_for_parry(block: u32) -> u32 {
    KNOCKAWAY_FOR_PARRY
        .get(block as usize)
        .map_or(LS_K1_TR, |&to| u32::from(to))
}
/// `PM_SaberDeflectionForQuad`: `LS_NONE` outside the eight quadrants.
pub fn deflection_for_quad(quad: usize) -> u32 {
    DEFLECTION_FOR_QUAD
        .get(quad)
        .map_or(LS_NONE, |&to| u32::from(to))
}
/// `saberMoveData[move].animToUse`.
pub fn move_animation(saber_move: u32) -> Option<u16> {
    MOVE_ANIMS.get(saber_move as usize).copied()
}

/// `BG_SuperBreakWinAnim`.
pub fn super_break_win(animation: u16) -> bool {
    anim_flag(animation, 1)
}
/// `BG_StabDownAnim`.
pub fn stab_down(animation: u16) -> bool {
    anim_flag(animation, 1 << 1)
}
/// `BG_SaberInSpecialAttack`.
pub fn special_attack(animation: u16) -> bool {
    anim_flag(animation, 1 << 2)
}
/// `BG_KickingAnim`.
pub fn kicking(animation: u16) -> bool {
    anim_flag(animation, 1 << 3)
}
/// `BG_SpinningSaberAnim`.
pub fn spinning(animation: u16) -> bool {
    anim_flag(animation, 1 << 4)
}
/// `BG_InSpecialJump`.
pub fn special_jump(animation: u16) -> bool {
    anim_flag(animation, 1 << 5)
}
/// `BG_SuperBreakLoseAnim`.
pub fn super_break_lose(animation: u16) -> bool {
    anim_flag(animation, 1 << 6)
}
/// `BG_FlippingAnim`.
pub fn flipping(animation: u16) -> bool {
    anim_flag(animation, 1 << 7)
}
/// `BG_InKataAnim`.
pub fn kata_animation(animation: u16) -> bool {
    anim_flag(animation, 1 << 8)
}
/// `BG_InSaberStandAnim`.
pub fn saber_stand(animation: u16) -> bool {
    anim_flag(animation, 1 << 9)
}
/// `PM_JumpingAnim`.
pub fn jumping(animation: u16) -> bool {
    anim_flag(animation, 1 << 10)
}
/// `PM_RunningAnim`.
pub fn running(animation: u16) -> bool {
    anim_flag(animation, 1 << 11)
}
/// `PM_WalkingAnim`.
pub fn walking(animation: u16) -> bool {
    anim_flag(animation, 1 << 12)
}
/// `PM_SwimmingAnim`.
pub fn swimming(animation: u16) -> bool {
    anim_flag(animation, 1 << 13)
}
/// `BG_InSlopeAnim`.
pub fn slope(animation: u16) -> bool {
    anim_flag(animation, 1 << 14)
}

/// `BG_InKnockDownOnGround` (`bg_panimate.c`): lying after a knockdown; the first half
/// second of getting up; the ends of two grapple and lock-break animations.
/// `length` is `BG_AnimLength` of the legs' animation.
pub fn knocked_down_on_ground(legs: u16, legs_timer: i32, length: i32) -> bool {
    match legs {
        // BOTH_KNOCKDOWN1..=5, BOTH_RELEASED
        1219..=1223 | 1301 => true,
        // The get-ups and the get-up rolls.
        1224..=1246 => length - legs_timer < 500,
        // BOTH_LK_DL_ST_T_SB_1_L
        788 => legs_timer < 1000,
        // BOTH_PLAYER_PA_3_FLY
        1291 => legs_timer < 300,
        _ => false,
    }
}

/// `G_PowerLevelForSaberAnim`: how strong the swing the torso plays is at this point of
/// it, from 0 (none) to 5. `elapsed` is the animation's length less its timer;
/// `my_saber_hit` asks for its strength in defence instead.
pub fn power_level(
    torso: u16,
    timer: i32,
    elapsed: i32,
    saber_type: i32,
    saber_num: usize,
    my_saber_hit: bool,
) -> i32 {
    match torso {
        // The attacks of each style: BOTH_A1_T__B_..BOTH_D1_B____ and on.
        126..=202 => match saber_type {
            SABER_LANCE => 4,
            SABER_TRIDENT => 3,
            _ => 1,
        },
        203..=279 => 2,
        280..=356 => 3,
        // Desann's.
        357..=433 => 4,
        // Tavion's, the dual and the staff styles.
        434..=664 => 2,
        // Parries, knockaways and broken parries: BOTH_P1_S1_T_..BOTH_H1_S1_BR.
        665..=689 => 1,
        _ => switch_level(torso, timer, elapsed, saber_num, my_saber_hit).unwrap_or(0),
    }
}

/// `SaberAttacking` (`w_saber.c:1050`): swinging, not parrying, bouncing or knocked
/// aside — an attack while the weapon fires unblocked, or any special.
pub fn attacking(saber_move: u32, weapon_state: u8, saber_blocked: u32) -> bool {
    const WEAPON_FIRING: u8 = 3;
    if in_parry(saber_move)
        || in_broken_parry(saber_move)
        || in_deflect(saber_move)
        || in_bounce(saber_move)
        || in_knockaway(saber_move)
    {
        return false;
    }
    if in_attack(saber_move) && weapon_state == WEAPON_FIRING && saber_blocked == 0 {
        return true;
    }
    in_special(saber_move)
}
