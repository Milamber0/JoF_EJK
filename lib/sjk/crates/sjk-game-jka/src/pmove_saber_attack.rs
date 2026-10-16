//! Which saber move the controls ask for (`codemp/game/bg_saber.c`): the directional
//! attacks and their special forms (`PM_SaberAttackForMovement`, 2182-2560 — the lunge,
//! the jump attacks, the flip over, the backstabs, the cartwheels, the stab down), the
//! transition into the next attack (`PM_SaberAnimTransitionAnim`, 427-584) and when a
//! chain of attacks must end (`PM_SaberKataDone`, 747-838), the kata, the kicks and the
//! throw's conditions. The special moves' own movement (a lunge's shove, a flip's leap)
//! is set here as the reference sets it, in the move.

use crate::pmove::MovementTrace;
use crate::pmove_lightsaber::*;
use crate::saber_move_data::{SABER_MOVES, movement::*};
use crate::saber_rules as rules;
use sjk_protocol::UserCommand;

/// `MASK_PLAYERSOLID`, `MASK_SOLID`.
const MASK_PLAYERSOLID: u32 = 0x1 | 0x10 | 0x100 | 0x1000;
const MASK_SOLID: u32 = 0x1;
/// `FP_SABERTHROW`.
const FP_SABERTHROW: usize = 17;
/// `EV_JUMP`.
const EV_JUMP: u16 = 16;
/// `HANDEXTEND_NONE`.
const HANDEXTEND_NONE: u8 = 0;
/// `FORCE_LEVEL_1..3` as styles.
const FORCE_LEVEL_1: u8 = 1;
const FORCE_LEVEL_2: u8 = 2;
const FORCE_LEVEL_3: u8 = 3;

/// `AngleVectors` of a yaw alone: forward and right.
fn flat_axes(yaw: f32) -> ([f32; 3], [f32; 3]) {
    let (forward, right) = crate::pmove::flight::flight_axes([0.0, yaw, 0.0]);
    (forward.to_array(), right.to_array())
}

/// The saber's own trace (`pm->trace`), past the player.
fn trace(
    saber: &Lightsaber,
    start: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    end: [f32; 3],
    mask: u32,
) -> Option<MovementTrace> {
    saber
        .collision
        .map(|collision| collision.trace(start, mins, maxs, end, mask))
}

/// Whether what a trace met is a player or an NPC (`PM_BGEntForNum`'s `eType`), and its
/// legs.
fn body(saber: &Lightsaber, trace: &MovementTrace) -> Option<u16> {
    (trace.entity_number < ENTITY_NONE)
        .then(|| (saber.context.bodies)(trace.entity_number))
        .flatten()
}

/// `PM_SaberMoveQuadrantForMovement` (`bg_saber.c:653-701`).
pub(crate) fn quadrant_for_movement(command: &UserCommand) -> u8 {
    match (command.right_move.signum(), command.forward_move.signum()) {
        (1, 1) => Q_TL,
        (1, -1) => Q_BL,
        (1, _) => Q_L,
        (-1, 1) => Q_TR,
        (-1, -1) => Q_BR,
        (-1, _) => Q_R,
        (_, 0) => Q_R,
        _ => Q_T,
    }
}

/// `PM_GroundDistance` (`bg_saber.c:1936-1950`): how far below the player, box and all, the
/// solid ground is.
pub(crate) fn ground_distance(saber: &Lightsaber) -> f32 {
    let origin = saber.state.origin;
    let down = [origin[0], origin[1], origin[2] - 4_096.0];
    let end = trace(
        saber,
        origin,
        saber.bounds.0,
        saber.bounds.1,
        down,
        MASK_SOLID,
    )
    .map_or(down, |trace| trace.end_position);
    let apart = [origin[0] - end[0], origin[1] - end[1], origin[2] - end[2]];
    f64::from(apart[0] * apart[0] + apart[1] * apart[1] + apart[2] * apart[2]).sqrt() as f32
}

/// `PM_InSecondaryStyle`: a dual or staff wielder using a single style.
fn in_secondary_style(saber: &Lightsaber) -> bool {
    let state = &*saber.state;
    matches!(state.saber_anim_level_base, SS_STAFF | SS_DUAL)
        && state.saber_anim_level != state.saber_anim_level_base
}

/// `PM_SaberAttackChainAngle` (`bg_saber.c:738-745`).
fn chain_angle(from: u16, to: u16) -> i32 {
    let (end, start) = (
        SABER_MOVES[usize::from(from)].end_quad,
        SABER_MOVES[usize::from(to)].start_quad,
    );
    i32::from(crate::pmove_saber::TRANSITION_ANGLE[usize::from(end)][usize::from(start)])
}

/// `PM_SaberKataDone` (`bg_saber.c:747-838`): whether an attack chain has run long enough
/// that the next attack must wait — never for Desann's, Tavion's, the dual or the staff
/// style; for the strong style by the chain's length and how it turns; for the others by
/// its length, drawn.
pub(crate) fn kata_done(saber: &mut Lightsaber, current: u16, next: u16) -> bool {
    let count = i32::from(saber.state.saber_attack_chain_count);
    if saber.state.vehicle_entity_num != 0 && count > 0 {
        return true;
    }
    let style = saber.state.saber_anim_level;
    if matches!(style, SS_DESANN | SS_TAVION | SS_STAFF | SS_DUAL) {
        return false;
    }
    if style == FORCE_LEVEL_3 {
        if current == LS_NONE || next == LS_NONE {
            return count > saber.timesync(0, 1);
        }
        if count > saber.timesync(2, 3) {
            return true;
        }
        if count > 0 {
            let angle = chain_angle(current, next);
            return if !(135..=215).contains(&angle) {
                true
            } else if angle == 180 {
                count > 1
            } else {
                count > 2
            };
        }
        return false;
    }
    if matches!(
        next,
        LS_A_TL2BR | LS_A_L2R | LS_A_BL2TR | LS_A_BR2TL | LS_A_R2L | LS_A_TR2BL
    ) {
        let tolerance = if style == FORCE_LEVEL_1 { 5 } else { 3 };
        if count >= tolerance && saber.timesync(1, count) > tolerance {
            return true;
        }
    }
    style == FORCE_LEVEL_2 && count > saber.timesync(2, 5)
}

/// `PM_SaberAnimTransitionAnim` (`bg_saber.c:427-584`): the move that leads from
/// `current` into `next` — a swing's start from the stance, the transition between
/// quadrants, or `next` itself. Its return to the stance keeps the reference's slip (it
/// counts from `next`, `LS_READY`, not from the attack).
pub(crate) fn transition(saber: &mut Lightsaber, current: u16, next: u16) -> u16 {
    let attack = |movement: u16| (LS_A_TL2BR..=LS_A_T2B).contains(&movement);
    let quads = |movement: u16| {
        (
            SABER_MOVES[usize::from(movement)].end_quad,
            SABER_MOVES[usize::from(movement)].start_quad,
        )
    };
    let between = |from: u16, to: u16| {
        crate::pmove_saber::TRANSITION_MOVE[usize::from(quads(from).0)][usize::from(quads(to).1)]
    };
    let result = if current == LS_READY {
        if attack(next) {
            LS_S_TL2BR + (next - LS_A_TL2BR)
        } else {
            next
        }
    } else if next == LS_READY {
        if attack(current) {
            (LS_R_TL2BR + next).wrapping_sub(LS_A_TL2BR)
        } else {
            next
        }
    } else if attack(next) {
        if next == current {
            if kata_done(saber, current, next) {
                LS_R_TL2BR + (next - LS_A_TL2BR)
            } else {
                between(current, next)
            }
        } else if quads(current).0 == quads(next).1 {
            next
        } else if attack(current)
            || (LS_D1_BR..=LS_D1_B_).contains(&current)
            || (LS_R_TL2BR..=LS_R_T2B).contains(&current)
            || (LS_PARRY_UP..=LS_REFLECT_LL).contains(&current)
            || (LS_K1_T_..=LS_K1_BL).contains(&current)
            || (LS_V1_BR..=LS_V1_B_).contains(&current)
            || matches!(
                current,
                LS_H1_T_ | LS_H1_TR | LS_H1_TL | LS_H1_BR | LS_H1_BL
            )
        {
            between(current, next)
        } else {
            next
        }
    } else {
        next
    };
    if result == LS_NONE { next } else { result }
}

/// `PM_CheckEnemyPresence` (`bg_saber.c:1999-2056`): a player or NPC within `radius` to the
/// right (0), left (1), front (2) or back (3), by one box trace.
fn enemy_present(saber: &Lightsaber, direction: u8, radius: f32) -> bool {
    let (forward, right) = flat_axes(saber.state.view_angles[1]);
    let towards = match direction {
        0 => right,
        1 => right.map(|value| -value),
        2 => forward,
        _ => forward.map(|value| -value),
    };
    let origin = saber.state.origin;
    let end = std::array::from_fn(|axis| origin[axis] + radius * towards[axis]);
    trace(saber, origin, [-12.0; 3], [12.0; 3], end, MASK_PLAYERSOLID).is_some_and(|hit| {
        hit.fraction != 1.0 && hit.entity_number < 1_022 && body(saber, &hit).is_some()
    })
}

/// `PM_CanDoDualDoubleAttacks` (`bg_saber.c:1974-1997`).
fn can_do_dual_double(saber: &Lightsaber) -> bool {
    let state = &*saber.state;
    !saber
        .context
        .sabers
        .any_flag(crate::saber_info::SFL_NO_MIRROR_ATTACKS)
        && !rules::special_attack(state.torso_anim)
        && !rules::special_attack(state.legs_anim)
}

/// `PM_CanBackstab` (`bg_saber.c:1567-1595`): a player or NPC right behind.
fn can_backstab(saber: &Lightsaber) -> bool {
    let (forward, _) = flat_axes(saber.state.view_angles[1]);
    let origin = saber.state.origin;
    let back = std::array::from_fn(|axis| origin[axis] - forward[axis] * 128.0);
    trace(
        saber,
        origin,
        [-15.0, -15.0, -8.0],
        [15.0, 15.0, 8.0],
        back,
        MASK_PLAYERSOLID,
    )
    .is_some_and(|hit| {
        hit.fraction != 1.0 && hit.entity_number < ENTITY_NONE && body(saber, &hit).is_some()
    })
}

/// `PM_CheckStabDown` (`bg_saber.c:587-651`): on the ground, facing a player or NPC lying
/// within 164 units, the stab down (the player's rise stopped).
fn stab_down(saber: &mut Lightsaber) -> u16 {
    if saber
        .context
        .sabers
        .any_flag(crate::saber_info::SFL_NO_STABDOWN)
        || saber.state.ground_entity_number == ENTITY_NONE
    {
        return LS_NONE;
    }
    if saber.state.client_num < 32 {
        saber.state.velocity[2] = 0.0;
        saber.command.up_move = 0;
    }
    let (forward, _) = flat_axes(saber.state.view_angles[1]);
    let origin = saber.state.origin;
    let end = std::array::from_fn(|axis| origin[axis] + 164.0 * forward[axis]);
    let lying = trace(saber, origin, [-15.0; 3], [15.0; 3], end, MASK_PLAYERSOLID)
        .filter(|hit| hit.entity_number < 1_022)
        .and_then(|hit| body(saber, &hit))
        .is_some_and(|legs| crate::knockdown::in_knockdown(legs));
    if !lying {
        return LS_NONE;
    }
    match saber.state.saber_anim_level {
        SS_DUAL => LS_STABDOWN_DUAL,
        SS_STAFF => LS_STABDOWN_STAFF,
        _ => LS_STABDOWN,
    }
}

/// `PM_SetForceJumpZStart`: a landing at the height the leap began hurts nobody.
fn jump_start(saber: &mut Lightsaber) {
    let height = saber.state.origin[2];
    saber.state.force_jump_start_height = if height == 0.0 { height - 0.1 } else { height };
}

/// The special forward jump attacks' leap: `speed` along the view, `rise` up.
fn leap(saber: &mut Lightsaber, speed: f32, rise: f32) {
    let (forward, _) = flat_axes(saber.state.view_angles[1]);
    saber.state.velocity = [forward[0] * speed, forward[1] * speed, forward[2] * speed];
    saber.state.velocity[2] = rise;
    jump_start(saber);
    saber.event(EV_JUMP, 0);
    saber.state.force_jump_sound = true;
    saber.command.up_move = 0;
}

/// A cartwheel's override (`bg_saber.c:2195-2248`): the first saber's move, or the
/// second's where the first cancels it and the second has one, or cancelled; the second's
/// alone when the first leaves it — except for the left, where the reference reads the
/// first saber's again (which leaves it to the style).
fn side_override(
    sabers: &crate::saber_info::Sabers,
    pick: fn(&crate::saber_info::SaberInfo) -> i32,
    left: bool,
) -> i32 {
    use crate::saber_info::LS_INVALID;
    match (sabers.first, sabers.second) {
        (Some(first), second) if pick(&first) != LS_INVALID => {
            if pick(&first) != 0 {
                pick(&first)
            } else {
                second
                    .map(|second| pick(&second))
                    .filter(|chosen| *chosen > 0)
                    .unwrap_or(0)
            }
        }
        (first, Some(second)) if pick(&second) != LS_INVALID => {
            if left {
                first.map_or(LS_INVALID, |first| pick(&first))
            } else {
                pick(&second)
            }
        }
        _ => LS_INVALID,
    }
}

/// `PM_SaberAttackForMovement` (`bg_saber.c:2182-2560`): the attack the controls ask for
/// from `current` — a slash by the direction held; with a jump or a crouch the style's
/// special (cartwheel, jump attacks, flip over, DFA, lunge, stab down, backflip,
/// backstab), each paid from the Force pool; a bounce's own chain attack; the overhead
/// from the stance; a dual wielder's mirror attacks.
pub(crate) fn attack_for_movement(saber: &mut Lightsaber, current: u16) -> u16 {
    let no_specials = in_secondary_style(saber);
    let sabers = saber.context.sabers;
    let jump_right = side_override(&sabers, |saber| saber.jump_right_move, false);
    let jump_left = side_override(&sabers, |saber| saber.jump_left_move, true);
    let cartwheels = !sabers.any_flag(crate::saber_info::SFL_NO_CARTWHEELS);
    let command = saber.command;
    let mut next = LS_NONE;
    if command.right_move != 0 {
        let right = command.right_move > 0;
        let jump_move = if right { jump_right } else { jump_left };
        let jumping = command.up_move > 0 || saber.state.movement_flags & PMF_JUMP_HELD != 0;
        if !no_specials
            && jump_move != 0
            && saber.state.velocity[2] > 20.0
            && command.buttons & BUTTON_ATTACK != 0
            && ground_distance(saber) < 70.0
            && jumping
            && saber.enough_force(SABER_ALT_ATTACK_POWER_LR)
        {
            saber.drain(SABER_ALT_ATTACK_POWER_LR);
            if jump_move != crate::saber_info::LS_INVALID {
                return jump_move as u16;
            }
            let (_, right_axis) = flat_axes(saber.state.view_angles[1]);
            let sideways = if right { 190.0 } else { -190.0 };
            saber.state.velocity[0] = 0.0;
            saber.state.velocity[1] = 0.0;
            for axis in 0..3 {
                saber.state.velocity[axis] += sideways * right_axis[axis];
            }
            if saber.state.saber_anim_level == SS_STAFF {
                next = if right {
                    LS_BUTTERFLY_RIGHT
                } else {
                    LS_BUTTERFLY_LEFT
                };
                saber.state.velocity[2] = if right { 350.0 } else { 250.0 };
            } else if cartwheels {
                saber.event(EV_JUMP, 0);
                saber.state.velocity[2] = if right { 300.0 } else { 350.0 };
                next = if right {
                    LS_JUMPATTACK_ARIAL_RIGHT
                } else {
                    LS_JUMPATTACK_ARIAL_LEFT
                };
            }
        } else if command.forward_move > 0 {
            next = if right { LS_A_TL2BR } else { LS_A_TR2BL };
        } else if command.forward_move < 0 {
            next = if right { LS_A_BL2TR } else { LS_A_BR2TL };
        } else {
            next = if right { LS_A_L2R } else { LS_A_R2L };
        }
    } else if command.forward_move > 0 {
        next = forward_attack(saber, no_specials);
    } else if command.forward_move < 0 {
        next = backward_attack(saber, no_specials);
    } else if rules::in_bounce(u32::from(current)) {
        next = SABER_MOVES[usize::from(current)].chain_attack;
        next = if kata_done(saber, current, next) {
            SABER_MOVES[usize::from(current)].chain_idle
        } else {
            SABER_MOVES[usize::from(current)].chain_attack
        };
    } else if current == LS_READY {
        next = LS_A_T2B;
    }
    if saber.state.saber_anim_level == SS_DUAL {
        if matches!(next, LS_A_R2L | LS_S_R2L | LS_A_L2R | LS_S_L2R)
            && can_do_dual_double(saber)
            && enemy_present(saber, 0, 100.0)
            && enemy_present(saber, 1, 100.0)
        {
            next = LS_DUAL_LR;
            saber.command.right_move = 0;
        } else if matches!(next, LS_A_T2B | LS_S_T2B | LS_A_BACK | LS_A_BACK_CR)
            && can_do_dual_double(saber)
            && enemy_present(saber, 2, 100.0)
            && enemy_present(saber, 3, 100.0)
        {
            next = LS_DUAL_FB;
            saber.command.forward_move = 0;
        }
    }
    next
}

/// The forward attack: the dual or staff jump attack, the medium style's flip over, the
/// strong style's DFA, the lunge from a crouch, the stab down at one lying, else the
/// overhead.
fn forward_attack(saber: &mut Lightsaber, no_specials: bool) -> u16 {
    let command = saber.command;
    let style = saber.state.saber_anim_level;
    let jumping = command.up_move > 0 || saber.state.movement_flags & PMF_JUMP_HELD != 0;
    let sabers = saber.context.sabers;
    if !no_specials
        && matches!(style, SS_DUAL | SS_STAFF)
        && saber.state.force_rage_recovery_time < saber.seed
        && (saber.state.ground_entity_number != ENTITY_NONE || ground_distance(saber) <= 40.0)
        && saber.state.velocity[2] >= 0.0
        && jumping
        && !rules::in_transition_any(saber.state.saber_move)
        && !rules::in_attack(saber.state.saber_move)
        && saber.state.weapon_time <= 0
        && saber.state.force_hand_extend == HANDEXTEND_NONE
        && command.buttons & BUTTON_ATTACK != 0
        && saber.enough_force(SABER_ALT_ATTACK_POWER_FB)
    {
        // `PM_SaberJumpAttackMove2`.
        let next = sabers
            .special(|saber| saber.jump_forward_move, LS_A_T2B)
            .unwrap_or_else(|| {
                if style == SS_DUAL {
                    saber.command.up_move = 0;
                    LS_JUMPATTACK_DUAL
                } else {
                    LS_JUMPATTACK_STAFF_RIGHT
                }
            });
        if next != LS_A_T2B && next != LS_NONE {
            saber.drain(SABER_ALT_ATTACK_POWER_FB);
        }
        return next;
    }
    let airborne_special = |saber: &mut Lightsaber, wanted: u8| -> bool {
        !no_specials
            && saber.state.saber_anim_level == wanted
            && saber.state.velocity[2] > 100.0
            && ground_distance(saber) < 32.0
            && !rules::special_jump(saber.state.legs_anim)
            && !rules::special_attack(saber.state.torso_anim)
            && saber.enough_force(SABER_ALT_ATTACK_POWER_FB)
    };
    if airborne_special(saber, SS_MEDIUM) {
        // `PM_SaberFlipOverAttackMove`; a JA+ server's improved yellow DFA leaps 60
        // forward rather than 150 (`bg_saber.c:1697-1703`).
        let speed = saber.ja_plus.flip_over_forward_speed();
        let next = sabers
            .special(|saber| saber.jump_forward_move, LS_A_T2B)
            .unwrap_or_else(|| {
                leap(saber, speed, 400.0);
                LS_A_FLIP_SLASH
            });
        if next != LS_A_T2B && next != LS_NONE {
            saber.drain(SABER_ALT_ATTACK_POWER_FB);
        }
        return next;
    }
    if airborne_special(saber, SS_STRONG) {
        // `PM_SaberJumpAttackMove`: the DFA.
        let next = sabers
            .special(|saber| saber.jump_forward_move, LS_A_T2B)
            .unwrap_or_else(|| {
                leap(saber, 300.0, 280.0);
                LS_A_JUMP_T__B_
            });
        if next != LS_A_T2B && next != LS_NONE {
            saber.drain(SABER_ALT_ATTACK_POWER_FB);
        }
        return next;
    }
    if matches!(style, SS_FAST | SS_DUAL | SS_STAFF)
        && saber.state.ground_entity_number != ENTITY_NONE
        && saber.state.movement_flags & PMF_DUCKED != 0
        && saber.state.weapon_time <= 0
        && !rules::special_attack(saber.state.torso_anim)
        && saber.enough_force(SABER_ALT_ATTACK_POWER_FB)
    {
        let next = lunge(saber, no_specials);
        if next != LS_A_T2B && next != LS_NONE {
            saber.drain(SABER_ALT_ATTACK_POWER_FB);
        }
        return next;
    }
    if no_specials {
        return LS_NONE;
    }
    let stab = stab_down(saber);
    if stab != LS_NONE && saber.enough_force(SABER_ALT_ATTACK_POWER_FB) {
        saber.drain(SABER_ALT_ATTACK_POWER_FB);
        stab
    } else {
        LS_A_T2B
    }
}

/// `PM_SaberLungeAttackMove` (`bg_saber.c:1776-1830`).
fn lunge(saber: &mut Lightsaber, no_specials: bool) -> u16 {
    if let Some(chosen) = saber
        .context
        .sabers
        .special(|saber| saber.lunge_move, LS_A_T2B)
    {
        return chosen;
    }
    let style = saber.state.saber_anim_level;
    if style == SS_FAST {
        let (forward, _) = flat_axes(saber.state.view_angles[1]);
        saber.state.velocity = [forward[0] * 150.0, forward[1] * 150.0, forward[2] * 150.0];
        saber.event(EV_JUMP, 0);
        LS_A_LUNGE
    } else if !no_specials && style == SS_STAFF {
        LS_SPINATTACK
    } else if !no_specials {
        LS_SPINATTACK_DUAL
    } else {
        LS_A_T2B
    }
}

/// The backward attack: the staff's backflip, the backstab at one behind (crouched or
/// standing for the stronger styles), else the overhead.
fn backward_attack(saber: &mut Lightsaber, no_specials: bool) -> u16 {
    let command = saber.command;
    let jumping = command.up_move > 0 || saber.state.movement_flags & PMF_JUMP_HELD != 0;
    if !no_specials
        && saber.state.saber_anim_level == SS_STAFF
        && saber.state.force_rage_recovery_time < saber.seed
        && saber.state.levitation_level > FORCE_LEVEL_1
        && (saber.state.ground_entity_number != ENTITY_NONE || ground_distance(saber) <= 40.0)
        && saber.state.velocity[2] >= 0.0
        && jumping
        && !rules::in_transition_any(saber.state.saber_move)
        && !rules::in_attack(saber.state.saber_move)
        && saber.state.weapon_time <= 0
        && saber.state.force_hand_extend == HANDEXTEND_NONE
        && command.buttons & BUTTON_ATTACK != 0
    {
        // `PM_SaberBackflipAttackMove`.
        return saber
            .context
            .sabers
            .special(|saber| saber.jump_back_move, LS_A_T2B)
            .unwrap_or_else(|| {
                saber.command.up_move = 127;
                saber.state.velocity[2] = 500.0;
                LS_A_BACKFLIP_ATK
            });
    }
    if can_backstab(saber) && !rules::special_attack(saber.state.torso_anim) {
        let style = saber.state.saber_anim_level;
        if style >= FORCE_LEVEL_2 && style != SS_STAFF {
            if saber.state.movement_flags & PMF_DUCKED != 0 || command.up_move < 0 {
                LS_A_BACK_CR
            } else {
                LS_A_BACK
            }
        } else {
            LS_A_BACKSTAB
        }
    } else {
        LS_A_T2B
    }
}

/// `PM_KickMoveForConditions` (`bg_saber.c:2561-2635`): a kick the way the player moves,
/// the move then spent on it; `None` standing still.
pub(crate) fn kick_for_conditions(saber: &mut Lightsaber) -> Option<u16> {
    if saber.command.right_move != 0 {
        let kick = if saber.command.right_move > 0 {
            LS_KICK_R
        } else {
            LS_KICK_L
        };
        saber.command.right_move = 0;
        Some(kick)
    } else if saber.command.forward_move != 0 {
        let kick = if saber.command.forward_move > 0 {
            LS_KICK_F
        } else {
            LS_KICK_B
        };
        saber.command.forward_move = 0;
        Some(kick)
    } else if saber.ja_plus.standing_front_kick() {
        // A JA+ server kicks forward for a staff's alternate attack standing still
        // (`bg_saber.c:2857-2866`).
        Some(LS_KICK_F)
    } else {
        None
    }
}

/// `PM_CanDoKata` (`bg_saber.c:2650-2689`): both buttons, standing still on the ground,
/// the saber in hand at rest or starting a swing, not in a kata already, the pool enough.
pub(crate) fn can_do_kata(saber: &mut Lightsaber) -> bool {
    if in_secondary_style(saber) {
        return false;
    }
    let state = &*saber.state;
    let command = saber.command;
    let okay_move = state.saber_move == u32::from(LS_READY) || rules::in_start(state.saber_move);
    if !(!state.saber_in_flight
        && okay_move
        && !rules::in_kata(state.saber_move)
        && !rules::kata_animation(state.legs_anim)
        && !rules::kata_animation(state.torso_anim)
        && state.ground_entity_number != ENTITY_NONE
        && command.buttons & BUTTON_ATTACK != 0
        && command.buttons & BUTTON_ALT_ATTACK != 0
        && command.forward_move == 0
        && command.right_move == 0
        && command.up_move <= 0)
    {
        return false;
    }
    if !saber.enough_force(SABER_ALT_ATTACK_POWER) {
        return false;
    }
    let sabers = saber.context.sabers;
    !(sabers.first.is_some_and(|first| first.kata_move == 0)
        || sabers.second.is_some_and(|second| second.kata_move == 0))
}

/// `PM_CheckAltKickAttack` (`bg_saber.c:2691-2724`): the staff, lit, not mid-flip.
pub(crate) fn check_alt_kick(saber: &Lightsaber) -> bool {
    let state = &*saber.state;
    !saber
        .context
        .sabers
        .any_flag(crate::saber_info::SFL_NO_KICKS)
        && saber.command.buttons & BUTTON_ALT_ATTACK != 0
        && (!rules::flipping(state.legs_anim) || state.legs_timer <= 250)
        && state.saber_anim_level == SS_STAFF
        && state.saber_holstered == 0
}

/// The throw's conditions and its start (`bg_saber.c:2992-3026`): allowed to throw, the
/// Force for it, and room ahead; the pool pays and the saber is in flight.
pub(crate) fn throw(saber: &mut Lightsaber) {
    let level = saber.context.saber_throw;
    let needed = crate::force_powers::FORCE_POWER_NEEDED[usize::from(level.min(3))][FP_SABERTHROW];
    let state = &*saber.state;
    if !(state.saber_can_throw
        && can_use_power_now(state, saber.seed, saber.context.gametype)
        && level > 0)
    {
        return;
    }
    // `PM_SaberPowerCheck`.
    let power = i32::from(state.force_power);
    let enough = if state.saber_in_flight {
        power > needed
    } else {
        saber.enough_force(needed)
    };
    if !enough {
        return;
    }
    let (forward, _) = crate::pmove::flight::flight_axes(saber.state.view_angles);
    let forward = forward.to_array();
    let origin = saber.state.origin;
    let end = std::array::from_fn(|axis| origin[axis] + 80.0 * forward[axis]);
    let blocked = trace(saber, origin, [-3.0; 3], [3.0; 3], end, MASK_PLAYERSOLID)
        .is_some_and(|hit| hit.all_solid || hit.start_solid || hit.fraction < 1.0);
    if blocked {
        return;
    }
    if !saber.state.saber_in_flight {
        saber.drain(needed);
    }
    saber.state.saber_in_flight = true;
}

/// `BG_HasYsalamiri` and `BG_CanUseFPNow` (`bg_misc.c:1709-1789`) for the saber throw and
/// the Force jump (neither is a push, nor saber offense or defense; levitation outlasts a
/// duel).
pub(crate) fn can_use_power_now(
    state: &crate::pmove::MovementState,
    time: i32,
    gametype: i32,
) -> bool {
    can_use_power(state, time, gametype, false)
}

/// [`can_use_power_now`] for the Force jump (`FP_LEVITATION`), which a duel allows.
pub(crate) fn can_levitate_now(
    state: &crate::pmove::MovementState,
    time: i32,
    gametype: i32,
) -> bool {
    can_use_power(state, time, gametype, true)
}

fn can_use_power(
    state: &crate::pmove::MovementState,
    time: i32,
    gametype: i32,
    levitation: bool,
) -> bool {
    let powerups = &state.powerup_deadlines;
    let flags = powerups[crate::ctf::PW_REDFLAG] != 0 || powerups[crate::ctf::PW_BLUEFLAG] != 0;
    if gametype == crate::ctf::GT_CTY && flags || powerups[15] != 0 {
        return false;
    }
    !(state.force_restricted
        || state.true_non_jedi
        || state.weapon == 17
        || state.vehicle_entity_num != 0
        || state.duel_in_progress && !levitation
        || state.saber_lock_frame != 0
        || state.saber_lock_time > time
        || state.falling_to_death != 0)
}
