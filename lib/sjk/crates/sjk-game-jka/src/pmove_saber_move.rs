//! A saber move put into play (`PM_SetSaberMove`, `codemp/game/bg_saber.c:3741-4035`):
//! its pose — the style's copy of it, a saber's own draw and put-away, the stance for
//! the moves that stand still — on the torso or the whole body, the swing announced, and
//! the move's blocking; and the stance itself (`PM_GetSaberStance`, `bg_pmove.c:276-330`).

use crate::pmove_anim::{
    SETANIM_BOTH, SETANIM_FLAG_OVERRIDE, SETANIM_FLAG_RESTART, SETANIM_LEGS, SETANIM_TORSO,
};
use crate::pmove_lightsaber::*;
use crate::saber_move_data::{SABER_MOVES, movement::*};
use crate::saber_rules as rules;

/// `EV_SABER_ATTACK`, `EV_PAIN`.
const EV_SABER_ATTACK: u16 = 29;
const EV_PAIN: u16 = 89;
/// `SABER_ANIM_GROUP_SIZE`: a style's copy of the swings is this far on.
const SABER_ANIM_GROUP_SIZE: u16 = 77;
/// The poses `PM_SetSaberMove` reads by name.
const BOTH_STAND1: u16 = 915;
const BOTH_STAND2: u16 = 917;
const BOTH_STAND4TOATTACK2: u16 = 931;
const TORSO_DROPWEAP1: u16 = 1_396;
const TORSO_WEAPONIDLE10: u16 = 1_408;
const BOTH_WALK1: u16 = 1_102;
const BOTH_WALKBACK1: u16 = 1_134;
const BOTH_WALKBACK2: u16 = 1_135;
const BOTH_ARIAL_LEFT: u16 = 1_201;
const BOTH_ARIAL_RIGHT: u16 = 1_202;
const BOTH_S1_S6: u16 = 865;
const BOTH_S6_S1: u16 = 866;
const BOTH_S1_S7: u16 = 867;
const BOTH_S7_S1: u16 = 868;
const BOTH_P1_S1_T_: u16 = 665;
const BOTH_P6_S6_T_: u16 = 690;
const BOTH_P7_S7_T_: u16 = 715;
const BOTH_SABERFAST_STANCE: u16 = 850;
const BOTH_SABERSLOW_STANCE: u16 = 851;
const BOTH_SABERDUAL_STANCE: u16 = 852;
const BOTH_SABERSTAFF_STANCE: u16 = 853;
/// `brokenLimbs`: `BROKENLIMB_RARM`, `BROKENLIMB_LARM`.
const BROKENLIMB_RARM: u8 = 1 << 2;
const BROKENLIMB_LARM: u8 = 1 << 1;

/// The moves `PM_SetSaberMove` plays on the whole body (`bg_saber.c:3905-3931`).
const WHOLE_BODY: [u16; 25] = [
    LS_A_LUNGE,
    LS_A_JUMP_T__B_,
    LS_A_BACKSTAB,
    LS_A_BACK,
    LS_A_BACK_CR,
    LS_ROLL_STAB,
    LS_A_FLIP_STAB,
    LS_A_FLIP_SLASH,
    LS_JUMPATTACK_DUAL,
    LS_JUMPATTACK_ARIAL_LEFT,
    LS_JUMPATTACK_ARIAL_RIGHT,
    LS_JUMPATTACK_CART_LEFT,
    LS_JUMPATTACK_CART_RIGHT,
    LS_JUMPATTACK_STAFF_LEFT,
    LS_JUMPATTACK_STAFF_RIGHT,
    LS_A_BACKFLIP_ATK,
    LS_STABDOWN,
    LS_STABDOWN_STAFF,
    LS_STABDOWN_DUAL,
    LS_DUAL_SPIN_PROTECT,
    LS_STAFF_SOULCAL,
    LS_A1_SPECIAL,
    LS_A2_SPECIAL,
    LS_A3_SPECIAL,
    LS_UPSIDE_DOWN_ATTACK,
];

/// `PM_GetSaberStance`: a saber's own ready pose, else the style's; standing plainly
/// without the saber or with it off.
pub(crate) fn saber_stance(saber: &Lightsaber) -> u16 {
    let state = &*saber.state;
    if state.saber_entity_num == 0 || crate::pmove_locomotion::sabers_off_state(state) {
        return BOTH_STAND1;
    }
    let sabers = saber.context.sabers;
    for ready in [sabers.first, sabers.second]
        .into_iter()
        .flatten()
        .map(|saber| saber.ready_anim)
    {
        if ready != -1 {
            return ready as u16;
        }
    }
    if sabers.first.is_some() && sabers.second.is_some() && state.saber_holstered == 0 {
        return BOTH_SABERDUAL_STANCE;
    }
    match state.saber_anim_level {
        SS_DUAL => BOTH_SABERDUAL_STANCE,
        SS_STAFF => BOTH_SABERSTAFF_STANCE,
        SS_FAST | SS_TAVION => BOTH_SABERFAST_STANCE,
        SS_STRONG => BOTH_SABERSLOW_STANCE,
        // `SS_NONE`, `SS_MEDIUM`, `SS_DESANN` and the rest (`bg_pmove.c:311-332`).
        _ => BOTH_STAND2,
    }
}

/// `PM_SetSaberMove`.
pub(crate) fn set_saber_move(saber: &mut Lightsaber, new_move: u16) {
    let data = SABER_MOVES[usize::from(new_move)];
    let mut flags = data.animation_flags;
    let fixed = saber.fixed_moves;
    let mut animation = crate::saber_move_data::move_animation(new_move, fixed);
    let mut parts = SETANIM_TORSO;
    let state = &mut *saber.state;
    // A kata's count, for how long attacks may chain.
    if matches!(new_move, LS_READY | LS_A_FLIP_STAB | LS_A_FLIP_SLASH) {
        state.saber_attack_chain_count = 0;
    } else if rules::in_attack(u32::from(new_move)) {
        state.saber_attack_chain_count = state.saber_attack_chain_count.saturating_add(1);
    }
    state.saber_attack_chain_count = state.saber_attack_chain_count.min(16);
    let style = state.saber_anim_level;
    let sabers = saber.context.sabers;
    let own_pose = |pick: fn(&crate::saber_info::SaberInfo) -> i32| {
        [sabers.first, sabers.second]
            .into_iter()
            .flatten()
            .map(|saber| pick(&saber))
            .find(|pose| *pose != -1)
    };
    if new_move == LS_DRAW {
        animation = match own_pose(|saber| saber.draw_anim) {
            Some(pose) => pose as u16,
            None if style == SS_STAFF => BOTH_S1_S7,
            None if style == SS_DUAL => BOTH_S1_S6,
            None => animation,
        };
    } else if new_move == LS_PUTAWAY {
        animation = match own_pose(|saber| saber.putaway_anim) {
            Some(pose) => pose as u16,
            None if style == SS_STAFF => BOTH_S7_S1,
            None if style == SS_DUAL => BOTH_S6_S1,
            None => animation,
        };
    } else if matches!(style, SS_STAFF | SS_DUAL) && (LS_S_TL2BR..LS_REFLECT_LL).contains(&new_move)
    {
        // The staff and the dual sabers have their own sets; their bounces, parries and
        // the like come in one level only.
        if (LS_V1_BR..=LS_REFLECT_LL).contains(&new_move) {
            let set = if style == SS_STAFF {
                BOTH_P7_S7_T_
            } else {
                BOTH_P6_S6_T_
            };
            animation = set + (animation - BOTH_P1_S1_T_);
        } else {
            animation += u16::from(style - 1) * SABER_ANIM_GROUP_SIZE;
        }
    } else if style > 1 {
        let one_level = rules::in_idle(u32::from(new_move))
            || rules::in_parry(u32::from(new_move))
            || rules::in_knockaway(u32::from(new_move))
            || rules::in_broken_parry(u32::from(new_move))
            || rules::in_reflect(u32::from(new_move))
            || rules::in_special(u32::from(new_move));
        if !one_level {
            animation += u16::from(style - 1) * SABER_ANIM_GROUP_SIZE;
        }
    }
    // The same pose again is played again.
    let current = saber.state.saber_move;
    if (current as usize) < SABER_MOVES.len()
        && crate::saber_move_data::move_animation(current as u16, fixed) == animation
        && new_move > LS_PUTAWAY
    {
        flags |= SETANIM_FLAG_RESTART;
    }
    let riding = saber.state.vehicle_entity_num != 0;
    if !riding && rules::in_special(u32::from(new_move)) {
        flags |= SETANIM_FLAG_OVERRIDE;
    }
    if rules::saber_stand(animation) || animation == BOTH_STAND1 {
        // The stance follows the legs: standing, crouched, walking back or on a slope it
        // is the saber's.
        let state = &*saber.state;
        animation = state.legs_anim;
        let stance = saber_stance(saber);
        let state = &*saber.state;
        if (BOTH_STAND1..=BOTH_STAND4TOATTACK2).contains(&animation)
            || (TORSO_DROPWEAP1..=TORSO_WEAPONIDLE10).contains(&animation)
        {
            animation = stance;
        }
        if state.movement_flags & PMF_DUCKED != 0
            || matches!(animation, BOTH_WALKBACK1 | BOTH_WALKBACK2 | BOTH_WALK1)
            || rules::slope(animation)
        {
            animation = stance;
        }
        parts = SETANIM_TORSO;
    }
    if !riding {
        let command = saber.command;
        if matches!(
            new_move,
            LS_JUMPATTACK_ARIAL_RIGHT | LS_JUMPATTACK_ARIAL_LEFT
        ) {
            parts = SETANIM_LEGS;
        } else if WHOLE_BODY.contains(&new_move)
            || matches!(new_move, LS_PULL_ATTACK_STAB | LS_PULL_ATTACK_SWING)
            || rules::kick_move(u32::from(new_move))
        {
            parts = SETANIM_BOTH;
        } else if rules::spinning(animation) {
            parts = SETANIM_BOTH;
        } else if command.forward_move == 0 && command.right_move == 0 && command.up_move == 0 {
            let state = &*saber.state;
            let legs = state.legs_anim;
            let rolling = crate::pmove_roll_anim::in_roll(legs) && state.legs_timer > 0;
            let stance = saber_stance(saber);
            let state = &*saber.state;
            if !rules::flipping(legs)
                && !rolling
                && !crate::pmove_hand_extend::in_knockdown(legs, state.legs_timer)
                && !rules::jumping(legs)
                && !rules::special_jump(legs)
                && animation != stance
                && state.ground_entity_number != ENTITY_NONE
                && state.movement_flags & PMF_DUCKED == 0
            {
                parts = SETANIM_BOTH;
            } else if state.movement_flags & PMF_DUCKED == 0
                && matches!(new_move, LS_SPINATTACK_DUAL | LS_SPINATTACK)
            {
                parts = SETANIM_BOTH;
            }
        }
        saber.animate(parts, animation, flags);
        let state = &mut *saber.state;
        if parts != SETANIM_LEGS
            && matches!(state.legs_anim, BOTH_ARIAL_LEFT | BOTH_ARIAL_RIGHT)
            && state.legs_timer > state.torso_timer
        {
            state.legs_timer = state.torso_timer;
        }
    }
    if saber.state.torso_anim != animation {
        return;
    }
    // A swing that begins is announced (a kick is not); a broken arm hurts at it.
    if (rules::in_attack(u32::from(new_move)) || rules::special_attack(animation))
        && saber.state.saber_move != u32::from(new_move)
    {
        if !matches!(
            new_move,
            LS_KICK_F
                | LS_KICK_B
                | LS_KICK_R
                | LS_KICK_L
                | LS_KICK_F_AIR
                | LS_KICK_B_AIR
                | LS_KICK_R_AIR
                | LS_KICK_L_AIR
        ) {
            saber.event(EV_SABER_ATTACK, 0);
        }
        let broken = saber.state.broken_limbs;
        if broken != 0 {
            let factor = if broken & BROKENLIMB_RARM != 0 {
                Some(5)
            } else if broken & BROKENLIMB_LARM != 0 {
                Some(10)
            } else {
                None
            };
            if let Some(factor) = factor
                && saber.timesync(0, factor) == 0
            {
                let pain = saber.timesync(1, 100);
                saber.event(EV_PAIN, pain as u16);
            }
        }
    }
    let state = &mut *saber.state;
    if rules::in_special(u32::from(new_move)) && state.weapon_time < state.torso_timer {
        state.weapon_time = state.torso_timer;
    }
    state.saber_move = u32::from(new_move);
    state.saber_blocking = data.blocking;
    state.torso_anim = animation;
    if state.weapon_time <= 0 {
        state.saber_blocked = 0;
    }
}
