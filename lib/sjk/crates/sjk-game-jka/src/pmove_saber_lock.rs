//! Saber locks, the movement's half: the lock at the head of `PM_WeaponLightsaber`
//! (`codemp/game/bg_saber.c:2819-2862`), `PM_SaberLocked` (`bg_saber.c:1343-1520`: the
//! frames pushed by a player's weight, the break when one wins, when the two are pulled
//! apart or when the lock comes undone) and `PM_SaberLockBreak` with its poses
//! (`bg_saber.c:846-1280`). Like the reference, a player's move changes its opponent too,
//! so the caller hands the opponent's movement in; what the game keeps beside the
//! movement (the knockdown's time, who knocked it down, the duel's loss) comes back as a
//! [`LockOutcome`]. The lock's beginning and the presses are the game's
//! ([`crate::saber_lock`]).

use crate::player_death::Rng;
use crate::pmove::MovementState;
use crate::pmove_anim::{
    AnimationLengths, SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_TORSO,
    set_animation,
};
use crate::predicted_events::PredictedEvents;
use crate::saber_lock::*;
use crate::saber_move_data::movement::{LS_A_T2B, LS_K1_T_, LS_NONE, LS_V1_BL, LS_V1_BR};

/// Animations a break plays besides the lock tables'.
const BOTH_STAND1: u16 = 915;
const BOTH_KNOCKDOWN4: u16 = 1_222;
const BOTH_V1_BL_S1: u16 = 682;
const BOTH_V1_BR_S1: u16 = 676;
const BOTH_K1_S1_T_: u16 = 670;
const BOTH_A3_T__B_: u16 = 280;
/// The superbreaks of the old locks (`BOTH_LK_S_S_T_SB_1_W` and so on).
const BOTH_LK_S_S_S_SB_1_L: u16 = 763;
const BOTH_LK_S_S_S_SB_1_W: u16 = 764;
const BOTH_LK_S_S_T_SB_1_L: u16 = 768;
const BOTH_LK_S_S_T_SB_1_W: u16 = 769;
/// `BOTH_LK_*_L_1` of a pairing the other began (`L_2`).
const BOTH_LK_S_S_S_L_1: u16 = 762;
const BOTH_LK_S_S_T_L_1: u16 = 767;
const BOTH_LK_DL_DL_S_L_1: u16 = 772;
const BOTH_LK_DL_DL_T_L_1: u16 = 777;
const BOTH_LK_ST_ST_S_L_1: u16 = 812;
const BOTH_LK_ST_ST_T_L_1: u16 = 817;
/// `BLOCKED_NONE`, `BLOCKED_PARRY_BROKEN`.
const BLOCKED_NONE: u8 = 0;
const BLOCKED_PARRY_BROKEN: u8 = 2;
/// `weaponstate_t`'s ready and firing.
const WEAPON_READY: u8 = 0;
const WEAPON_FIRING: u8 = 3;
/// `forceHandExtend`'s knockdown and weapon-ready.
const HANDEXTEND_WEAPONREADY: u8 = 6;
const HANDEXTEND_KNOCKDOWN: u8 = 8;
/// `EV_JUMP`, `EV_PAIN`.
const EV_JUMP: u16 = 16;
const EV_PAIN: u16 = 89;
/// `ENTITYNUM_NONE`.
const ENTITY_NONE: u16 = 1_023;

/// The opponent's movement, as the locker's move reaches it, and what the locker's move
/// needs of the game: its generator (`Q_irand`), its `FP_SABER_OFFENSE` level and its
/// `ps.saberLockHits`, neither of which the wire carries.
pub struct LockOpponent<'a> {
    pub state: &'a mut MovementState,
    pub events: &'a mut PredictedEvents,
    pub rng: &'a mut Rng,
    pub offense: u8,
    pub hits: i32,
}

/// What the game gives a locked player's move: its opponent's movement, where it has
/// one, the game's generator, and the player's `ps.saberLockHits`, which the wire does
/// not carry. The player's `FP_SABER_OFFENSE` level is its [`crate::pmove::MoveContext`]'s.
pub struct LockContext<'a> {
    pub opponent: Option<&'a mut crate::pmove::Predictor>,
    pub rng: &'a mut Rng,
    pub hits: i32,
}

impl LockContext<'_> {
    /// The opponent as one slice of the move reaches it.
    pub(crate) fn opponent(&mut self, offense: u8) -> Option<LockOpponent<'_>> {
        let hits = self.hits;
        let (state, events) = self.opponent.as_deref_mut()?.lock_parts();
        Some(LockOpponent {
            state,
            events,
            rng: &mut *self.rng,
            offense,
            hits,
        })
    }
}

/// What a lock's break asks of the game for the opponent beyond its movement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LockOutcome {
    /// Knocked down: its `forceHandExtendTime`.
    pub knocked_down_until: Option<i32>,
    /// `otherKiller`, `otherKillerTime`, `otherKillerDebounceTime`: who knocked it down.
    pub other_killer: Option<(u16, i32, i32)>,
    /// `pm->checkDuelLoss`: the loser, whom the game may finish or disarm.
    pub duel_loss: Option<u16>,
    /// `SEF_LOCK_WON` on the mover's `saberEventFlags` (`bg_saber.c:1237`): it won by
    /// over-power, which a Jedi NPC's AI presses home.
    pub lock_won: bool,
}

/// `PM_irand_timesync` (`bg_saber.c:38-52`): an integer in `low..=high` from `Q_random`,
/// the command's time as its seed, which it steps.
fn timesync(seed: &mut i32, low: i32, high: i32) -> i32 {
    *seed = 69_069_i32.wrapping_mul(*seed).wrapping_add(1);
    let random = ((*seed as u32) & 0xffff) as f32 / 65_536.0;
    let value = ((low - 1) as f32 + random * (high - low) as f32 + 1.0) as i32;
    value.clamp(low, high)
}

/// The head of `PM_WeaponLightsaber` after the knockdown's (`bg_saber.c:2819-2862`):
/// locked, the lock runs and nothing else of the saber does; a lock frame left over
/// breaks the lock with the opponent, or without one falls back to standing. Returns
/// whether the saber's code stops here.
pub(crate) fn head(
    state: &mut MovementState,
    server_time: i32,
    lengths: &dyn AnimationLengths,
    events: &mut PredictedEvents,
    opponent: Option<LockOpponent>,
    outcome: &mut LockOutcome,
) -> bool {
    let mut seed = server_time;
    if state.saber_lock_time > server_time {
        state.saber_move = u32::from(LS_NONE);
        if let Some(opponent) = opponent {
            locked(state, &mut seed, lengths, events, opponent, outcome);
        }
        return true;
    }
    if state.saber_lock_frame == 0 {
        return false;
    }
    if state.saber_lock_enemy < ENTITY_NONE {
        if let Some(opponent) = opponent {
            lock_break(
                state, &mut seed, lengths, events, opponent, false, 0, outcome,
            );
            return true;
        }
    }
    state.torso_timer = 0;
    set_animation(
        state,
        SETANIM_TORSO,
        BOTH_STAND1,
        SETANIM_FLAG_OVERRIDE,
        lengths,
    );
    state.saber_lock_frame = 0;
    false
}

/// `PM_SaberLocked`: both still in their lock poses, the lock holds — broken if they are
/// pulled out of 8 to 80 units — and a push moves the pusher's frame towards its win by
/// its offense and the opponent's back by as much, until one end is reached and the
/// pusher wins; anything else broke the lock, and it breaks with nobody winning.
fn locked(
    state: &mut MovementState,
    seed: &mut i32,
    lengths: &dyn AnimationLengths,
    events: &mut PredictedEvents,
    opponent: LockOpponent,
    outcome: &mut LockOutcome,
) {
    let enemy = &mut *opponent.state;
    if !(state.saber_lock_frame != 0
        && enemy.saber_lock_frame != 0
        && in_lock(state.torso_anim)
        && in_lock(enemy.torso_anim))
    {
        lock_break(state, seed, lengths, events, opponent, false, 0, outcome);
        return;
    }
    state.torso_timer = 0;
    state.weapon_time = 0;
    enemy.torso_timer = 0;
    enemy.weapon_time = 0;
    let distance: f32 = (0..3)
        .map(|axis| {
            (state.origin[axis] - enemy.origin[axis]) * (state.origin[axis] - enemy.origin[axis])
        })
        .sum();
    if !(64.0..=6_400.0).contains(&distance) {
        lock_break(state, seed, lengths, events, opponent, false, 0, outcome);
        return;
    }
    if !state.saber_lock_advance {
        return;
    }
    let strength = i32::from(opponent.offense) + 1;
    state.saber_lock_advance = false;
    let (first, frames) = frames_of(lengths, state.torso_anim);
    let current = f32::from(state.saber_lock_frame);
    let upward = if in_lock_old(state.torso_anim) {
        !matches!(state.torso_anim, BOTH_CCWCIRCLELOCK | BOTH_BF2LOCK)
    } else {
        increments(state.torso_anim, true)
    };
    let remaining = if upward {
        let frame = current.ceil() as i32 + strength;
        if frame >= first + frames {
            lock_break(
                state, seed, lengths, events, opponent, true, strength, outcome,
            );
            return;
        }
        state.saber_lock_frame = frame as u16;
        first + frames - frame
    } else {
        let frame = current.floor() as i32 - strength;
        if frame <= first {
            lock_break(
                state, seed, lengths, events, opponent, true, strength, outcome,
            );
            return;
        }
        state.saber_lock_frame = frame as u16;
        frame - first
    };
    if timesync(seed, 0, 2) == 0 {
        crate::pmove_weapon_charge::event(state, events, EV_JUMP, 0);
    }
    // The opponent's frame follows, from its own end, with a grunt now and then.
    let enemy = &mut *opponent.state;
    let (first, frames) = frames_of(lengths, enemy.torso_anim);
    let (grunts, from_start) = if in_lock_old(enemy.torso_anim) {
        let grunts = matches!(enemy.torso_anim, BOTH_CWCIRCLELOCK | BOTH_BF1LOCK);
        (grunts, grunts)
    } else {
        let grunts = increments(enemy.torso_anim, false);
        (grunts, !grunts)
    };
    if grunts && timesync(seed, 0, 2) == 0 {
        crate::pmove_weapon_charge::event(enemy, opponent.events, EV_PAIN, 80);
    }
    enemy.saber_lock_frame = if from_start {
        first + remaining
    } else {
        first + frames - remaining
    } as u16;
}

/// An animation's first frame and frame count.
fn frames_of(lengths: &dyn AnimationLengths, animation: u16) -> (i32, i32) {
    let first = lengths.first_frame(animation).unwrap_or(0) as i32;
    (
        first,
        lengths
            .timing(animation)
            .map_or(0, |timing| i32::from(timing.frame_count)),
    )
}

/// `PM_SaberLockBreak`: the lock broken, `victory` for the one whose move this is. A
/// superbreak when its strength and weight beat two to four (`Q_irand`). The old locks
/// play their win and lose poses, the new ones their results. A win with weight left
/// and no superbreak knocks the loser down (who knocked it down remembered) and puts the
/// duel's loss to the game; a lock nobody won throws both apart. Both are free to swing,
/// out of the lock, and jump.
#[allow(clippy::too_many_arguments)]
fn lock_break(
    state: &mut MovementState,
    seed: &mut i32,
    lengths: &dyn AnimationLengths,
    events: &mut PredictedEvents,
    opponent: LockOpponent,
    victory: bool,
    strength: i32,
    outcome: &mut LockOutcome,
) {
    let LockOpponent {
        state: enemy,
        events: enemy_events,
        rng,
        hits,
        ..
    } = opponent;
    let super_break = strength + hits > rng.irand(2, 4);
    if win_animation(state, victory, super_break, lengths) {
        lose_animation(enemy, victory, super_break, lengths);
    } else {
        result_animation(state, super_break, true, lengths);
        state.weapon_state = WEAPON_FIRING;
        result_animation(enemy, super_break, false, lengths);
        enemy.weapon_state = WEAPON_READY;
    }
    if victory {
        if hits != 0 && !super_break {
            let away = direction(state.origin, enemy.origin);
            if enemy.vehicle_entity_num == 0 && enemy.emplaced_index == 0 {
                enemy.force_hand_extend = HANDEXTEND_KNOCKDOWN;
                outcome.knocked_down_until = Some(*seed + 1_100);
                enemy.force_dodge_anim = 0;
                outcome.other_killer = Some((state.client_num, *seed + 5_000, *seed + 100));
                enemy.velocity = [away[0] * 320.0, away[1] * 320.0, 100.0];
            }
            outcome.duel_loss = Some(enemy.client_num);
            outcome.lock_won = true;
        }
    } else {
        let away = direction(state.origin, enemy.origin);
        enemy.velocity = [away[0] * 160.0, away[1] * 160.0, 150.0];
        let back = direction(enemy.origin, state.origin);
        state.velocity = [back[0] * 160.0, back[1] * 160.0, 150.0];
        enemy.force_hand_extend = HANDEXTEND_WEAPONREADY;
    }
    state.weapon_time = 0;
    enemy.weapon_time = 0;
    for side in [&mut *state, &mut *enemy] {
        side.saber_lock_time = 0;
        side.saber_lock_frame = 0;
        side.saber_lock_enemy = 0;
    }
    state.force_hand_extend = HANDEXTEND_WEAPONREADY;
    crate::pmove_weapon_charge::event(state, events, EV_JUMP, 0);
    if !victory {
        crate::pmove_weapon_charge::event(enemy, enemy_events, EV_JUMP, 0);
    } else if timesync(seed, 0, 1) != 0 {
        let parameter = timesync(seed, 0, 75);
        crate::pmove_weapon_charge::event(enemy, enemy_events, EV_JUMP, parameter as u16);
    }
}

/// `VectorSubtract(to, from)` made a unit vector (`VectorNormalize`).
fn direction(from: [f32; 3], to: [f32; 3]) -> [f32; 3] {
    let mut vector = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let length = f64::from(vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2])
        .sqrt() as f32;
    if length != 0.0 {
        let inverse = 1.0 / length;
        for value in &mut vector {
            *value *= inverse;
        }
    }
    vector
}

/// `PM_SaberLockWinAnim`: the old locks' pose for the side whose move this is — a
/// superbreak, a break when nobody won, or its own win (the top lock's attack, the
/// bottom's kick) — held on both halves, the weapon busy as long as it and firing.
/// Whether it was an old lock.
fn win_animation(
    state: &mut MovementState,
    victory: bool,
    super_break: bool,
    lengths: &dyn AnimationLengths,
) -> bool {
    let animation = match state.torso_anim {
        BOTH_BF2LOCK if super_break => BOTH_LK_S_S_T_SB_1_W,
        BOTH_BF2LOCK if !victory => BOTH_BF1BREAK,
        BOTH_BF2LOCK => {
            state.saber_move = u32::from(LS_A_T2B);
            BOTH_A3_T__B_
        }
        BOTH_BF1LOCK if super_break => BOTH_LK_S_S_T_SB_1_W,
        BOTH_BF1LOCK if !victory => BOTH_KNOCKDOWN4,
        BOTH_BF1LOCK => {
            state.saber_move = u32::from(LS_K1_T_);
            BOTH_K1_S1_T_
        }
        BOTH_CWCIRCLELOCK | BOTH_CCWCIRCLELOCK if super_break => BOTH_LK_S_S_S_SB_1_W,
        BOTH_CWCIRCLELOCK if !victory => broken_parry(state, LS_V1_BL, BOTH_V1_BL_S1),
        BOTH_CWCIRCLELOCK => BOTH_CWCIRCLEBREAK,
        BOTH_CCWCIRCLELOCK if !victory => broken_parry(state, LS_V1_BR, BOTH_V1_BR_S1),
        BOTH_CCWCIRCLELOCK => BOTH_CCWCIRCLEBREAK,
        _ => return false,
    };
    set_animation(
        state,
        SETANIM_BOTH,
        animation,
        SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        lengths,
    );
    state.weapon_time = state.torso_timer;
    state.saber_blocked = BLOCKED_NONE;
    state.weapon_state = WEAPON_FIRING;
    true
}

/// A side thrown into a broken parry: the move and the block, and its pose.
fn broken_parry(state: &mut MovementState, saber_move: u16, animation: u16) -> u16 {
    state.saber_move = u32::from(saber_move);
    state.saber_blocked = BLOCKED_PARRY_BROKEN;
    animation
}

/// `PM_SaberLockLoseAnim`: the old locks' pose for the other side — its superbreak, its
/// break, a knockdown from the top or a broken parry from the side — held on both halves
/// (`NPC_SetAnim`), the weapon busy as long as it, ready.
fn lose_animation(
    enemy: &mut MovementState,
    _victory: bool,
    super_break: bool,
    lengths: &dyn AnimationLengths,
) {
    let animation = match enemy.torso_anim {
        BOTH_BF2LOCK if super_break => BOTH_LK_S_S_T_SB_1_L,
        BOTH_BF2LOCK => BOTH_BF1BREAK,
        BOTH_BF1LOCK if super_break => BOTH_LK_S_S_T_SB_1_L,
        BOTH_BF1LOCK => BOTH_KNOCKDOWN4,
        BOTH_CWCIRCLELOCK | BOTH_CCWCIRCLELOCK if super_break => BOTH_LK_S_S_S_SB_1_L,
        BOTH_CWCIRCLELOCK => broken_parry(enemy, LS_V1_BL, BOTH_V1_BL_S1),
        BOTH_CCWCIRCLELOCK => broken_parry(enemy, LS_V1_BR, BOTH_V1_BR_S1),
        _ => return,
    };
    set_animation(
        enemy,
        SETANIM_BOTH,
        animation,
        SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        lengths,
    );
    enemy.weapon_time = enemy.torso_timer;
    enemy.saber_blocked = BLOCKED_NONE;
    enemy.weapon_state = WEAPON_READY;
}

/// `PM_SaberLockResultAnim`: a new lock's break or superbreak, won or lost, from the
/// lock pose (the other-began poses read as their own): held on both halves, the weapon
/// busy as long as it; the loser of a superbreak defenceless a little longer.
fn result_animation(
    side: &mut MovementState,
    super_break: bool,
    won: bool,
    lengths: &dyn AnimationLengths,
) {
    let mut animation = match side.torso_anim {
        BOTH_LK_S_S_S_L_2 => BOTH_LK_S_S_S_L_1,
        BOTH_LK_S_S_T_L_2 => BOTH_LK_S_S_T_L_1,
        BOTH_LK_DL_DL_S_L_2 => BOTH_LK_DL_DL_S_L_1,
        BOTH_LK_DL_DL_T_L_2 => BOTH_LK_DL_DL_T_L_1,
        BOTH_LK_ST_ST_S_L_2 => BOTH_LK_ST_ST_S_L_1,
        BOTH_LK_ST_ST_T_L_2 => BOTH_LK_ST_ST_T_L_1,
        other => other,
    };
    animation = if super_break {
        animation + 1
    } else {
        animation.wrapping_sub(2)
    };
    if won {
        animation += 1;
    }
    set_animation(
        side,
        SETANIM_BOTH,
        animation,
        SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        lengths,
    );
    if super_break && !won {
        side.saber_move = u32::from(LS_NONE);
        side.torso_timer += 250;
    }
    side.weapon_time = side.torso_timer;
    side.saber_blocked = BLOCKED_NONE;
}
