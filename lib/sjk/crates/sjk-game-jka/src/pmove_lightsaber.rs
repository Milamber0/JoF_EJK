//! `PM_WeaponLightsaber` (`codemp/game/bg_saber.c:2777-3740`) in full, for the single
//! saber styles and, as far as the stock saber reaches them, the dual and staff ones: a
//! knockdown's or a roll's wait (and the roll's stab), a lock, a kick or a superbreak
//! running out, the holstered saber, the alternate attack (the staff's kick; the throw
//! when the game follows thrown sabers), a thrown saber guided, the blade's block, bounce
//! or broken parry answered, the weapon change, the kata, a raise ending, and the attack
//! chosen from the move and the controls. Move selection is
//! [`crate::pmove_saber_attack`]'s, the move's pose `PM_SetSaberMove`
//! ([`crate::pmove_saber_move`]).

use crate::pmove::{MoveContext, MovementCollision, MovementState};
use crate::pmove_anim::{
    AnimationLengths, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_LEGS, SETANIM_TORSO,
    set_animation,
};
use crate::predicted_events::PredictedEvents;
use crate::saber_move_data::{SABER_MOVES, movement::*};
use crate::saber_rules as rules;
use sjk_protocol::UserCommand;

pub(crate) const BUTTON_ATTACK: u16 = 1;
pub(crate) const BUTTON_ALT_ATTACK: u16 = 128;
const WEAPON_READY: u8 = 0;
const WEAPON_RAISING: u8 = 1;
const WEAPON_DROPPING: u8 = 2;
const WEAPON_FIRING: u8 = 3;
const WEAPON_IDLE: u8 = 6;
/// `EV_SABER_UNHOLSTER`, `EV_NOAMMO`.
const EV_SABER_UNHOLSTER: u16 = 33;
const EV_NOAMMO: u16 = 25;
/// `saberStyle_t`.
pub(crate) const SS_FAST: u8 = 1;
pub(crate) const SS_MEDIUM: u8 = 2;
pub(crate) const SS_STRONG: u8 = 3;
pub(crate) const SS_DESANN: u8 = 4;
pub(crate) const SS_TAVION: u8 = 5;
pub(crate) const SS_DUAL: u8 = 6;
pub(crate) const SS_STAFF: u8 = 7;
/// `SABER_ALT_ATTACK_POWER`, `_LR`, `_FB`: what the specials cost.
pub(crate) const SABER_ALT_ATTACK_POWER: i32 = 50;
pub(crate) const SABER_ALT_ATTACK_POWER_LR: i32 = 10;
pub(crate) const SABER_ALT_ATTACK_POWER_FB: i32 = 25;
/// `bg_parryDebounce`: how long a parry is held, by `FP_SABER_DEFENSE`.
const PARRY_DEBOUNCE: [i32; 4] = [500, 300, 150, 50];
/// `saberBlockedType_t`.
const BLOCKED_BOUNCE_MOVE: u8 = 1;
const BLOCKED_PARRY_BROKEN: u8 = 2;
const BLOCKED_ATK_BOUNCE: u8 = 3;
const BLOCKED_UPPER_RIGHT: u8 = 4;
const BLOCKED_UPPER_RIGHT_PROJ: u8 = 9;
/// `saberQuadrant_t`: `Q_BR` through `Q_BL`, `Q_T`, `Q_TL`.
pub(crate) const Q_BR: u8 = 0;
pub(crate) const Q_R: u8 = 1;
pub(crate) const Q_TR: u8 = 2;
pub(crate) const Q_T: u8 = 3;
pub(crate) const Q_TL: u8 = 4;
pub(crate) const Q_L: u8 = 5;
pub(crate) const Q_BL: u8 = 6;
/// `BOTH_ROLL_F`, `BOTH_SABERPULL`, `BOTH_STAND1`, `BOTH_SABERDUAL_STANCE`,
/// `BOTH_FORCELEAP2_T__B_`, `BOTH_FORCELONGLEAP_START`, `_ATTACK`, `_LAND`, and the walks
/// and runs a finished raise's torso follows.
const BOTH_ROLL_F: u16 = 1_167;
const BOTH_SABERPULL: u16 = 1_342;
const BOTH_STAND1: u16 = 915;
const BOTH_SABERDUAL_STANCE: u16 = 852;
const BOTH_FORCELEAP2_T__B_: u16 = 858;
const BOTH_FORCELONGLEAP_START: u16 = 869;
const BOTH_FORCELONGLEAP_ATTACK: u16 = 870;
const BOTH_FORCELONGLEAP_LAND: u16 = 871;
/// `TORSO_RAISEWEAP1`.
const TORSO_RAISEWEAP1: u16 = 1_398;
const RAISE_FOLLOWS_LEGS: [u16; 9] = [
    1_102, 1_111, 1_114, 1_118, 1_120, 1_102, 1_103, 1_104, 1_106,
];
/// The walks and runs an attack's `-1` animation keeps the torso on.
const MOVING_LEGS: [u16; 15] = [
    1_102, 1_103, 1_104, 1_106, 1_134, 1_135, 1_105, 1_107, 1_111, 1_114, 1_118, 1_120, 1_136,
    1_137, 1_119,
];
/// `ENTITYNUM_NONE`.
pub(crate) const ENTITY_NONE: u16 = 1_023;
/// `PMF_DUCKED`, `PMF_JUMP_HELD`.
pub(crate) const PMF_DUCKED: u16 = 1;
pub(crate) const PMF_JUMP_HELD: u16 = 2;

/// One `PM_WeaponLightsaber`: the movement, its command as the saber's code changes it
/// (`pm->cmd`), and what else it reaches.
pub(crate) struct Lightsaber<'a> {
    pub state: &'a mut MovementState,
    pub command: UserCommand,
    /// `pml.msec`.
    pub millis: i32,
    pub lengths: &'a dyn AnimationLengths,
    pub events: &'a mut PredictedEvents,
    pub collision: Option<&'a dyn MovementCollision>,
    pub context: &'a MoveContext<'a>,
    /// The box of the move (`pm->mins`, `pm->maxs`).
    pub bounds: ([f32; 3], [f32; 3]),
    /// `pm->cmd.serverTime`, which `PM_irand_timesync` steps in place.
    pub seed: i32,
    /// `BG_FixSaberMoveData`'s fix in force (`LEGACYFIX_SABERMOVEDATA`).
    pub fixed_moves: bool,
    /// The JA+ server rules in force ([`crate::pmove_japlus`]).
    pub ja_plus: crate::pmove_japlus::JaPlusRules,
}

impl Lightsaber<'_> {
    /// `PM_irand_timesync` (`bg_saber.c:38-52`).
    pub(crate) fn timesync(&mut self, low: i32, high: i32) -> i32 {
        self.seed = 69_069_i32.wrapping_mul(self.seed).wrapping_add(1);
        let random = ((self.seed as u32) & 0xffff) as f32 / 65_536.0;
        (((low - 1) as f32 + random * (high - low) as f32 + 1.0) as i32).clamp(low, high)
    }

    /// `PM_AddEvent`.
    pub(crate) fn event(&mut self, event: u16, parameter: u16) {
        crate::pmove_weapon_charge::event(self.state, self.events, event, parameter);
    }

    /// `BG_EnoughForcePowerForMove`: the pool holds `cost`; if not, `EV_NOAMMO`.
    pub(crate) fn enough_force(&mut self, cost: i32) -> bool {
        if i32::from(self.state.force_power) < cost {
            self.event(EV_NOAMMO, 0);
            return false;
        }
        true
    }

    /// `BG_ForcePowerDrain` of a special move's `amount` (never levitation's).
    pub(crate) fn drain(&mut self, amount: i32) {
        self.state.force_power = (i32::from(self.state.force_power) - amount).max(0) as u8;
    }

    /// `PM_SetAnim`.
    pub(crate) fn animate(&mut self, parts: u8, animation: u16, flags: u8) {
        set_animation(self.state, parts, animation, flags, self.lengths);
    }

    /// `PM_SetSaberMove`.
    pub(crate) fn set_move(&mut self, new_move: u16) {
        crate::pmove_saber_move::set_saber_move(self, new_move);
    }

    /// `PM_WeaponLightsaber`.
    pub(crate) fn run(&mut self) {
        let mut check_only_weapon = false;
        let state = &mut *self.state;
        // A knockdown or a roll: the weapon waits, and a forward roll ends in a stab.
        if crate::pmove_hand_extend::in_knockdown(state.legs_anim, state.legs_timer)
            || crate::pmove_roll_anim::in_roll(state.legs_anim) && state.legs_timer > 0
        {
            if state.weapon_time > 0 {
                state.weapon_time = (state.weapon_time - self.millis).max(0);
            }
            if state.legs_anim == BOTH_ROLL_F
                && state.legs_timer <= 250
                && self.command.buttons & BUTTON_ATTACK != 0
                && self.enough_force(SABER_ALT_ATTACK_POWER_FB)
                && !self.state.saber_in_flight
                && !self
                    .context
                    .sabers
                    .any_flag(crate::saber_info::SFL_NO_ROLL_STAB)
            {
                if self.state.saber_holstered == 2 {
                    self.state.saber_holstered = 0;
                    self.event(EV_SABER_UNHOLSTER, 0);
                }
                self.set_move(LS_ROLL_STAB);
                self.drain(SABER_ALT_ATTACK_POWER_FB);
            }
            return;
        }
        // A saber lock (`bg_saber.c:2819-2862`) is `pmove_saber_lock::head`'s, which the
        // weapon's code runs before this, past the same wait.
        let state = &mut *self.state;
        if rules::kicking(state.legs_anim) || rules::kicking(state.torso_anim) {
            if state.legs_timer > 0 {
                return;
            }
            state.saber_move = u32::from(LS_READY);
            state.weapon_time = 0;
        }
        if (rules::super_break_lose(state.torso_anim) || rules::super_break_win(state.torso_anim))
            && state.torso_timer > 0
        {
            return;
        }
        if crate::pmove_locomotion::sabers_off_state(state) {
            if state.saber_move != u32::from(LS_READY) {
                self.set_move(LS_READY);
            }
            let state = &mut *self.state;
            let slope = rules::slope(state.legs_anim);
            if state.legs_anim != state.torso_anim && !slope && state.torso_timer <= 0 {
                let legs = state.legs_anim;
                self.animate(SETANIM_TORSO, legs, SETANIM_FLAG_OVERRIDE);
            } else if slope && self.state.torso_timer <= 0 {
                let stance = crate::pmove_saber_move::saber_stance(self);
                self.animate(SETANIM_TORSO, stance, SETANIM_FLAG_OVERRIDE);
            }
            if self.state.weapon_time < 1
                && self.command.buttons & (BUTTON_ALT_ATTACK | BUTTON_ATTACK) != 0
                && self.state.duel_time < self.seed
            {
                if self.state.vehicle_entity_num == 0 {
                    self.state.saber_holstered = 0;
                    self.event(EV_SABER_UNHOLSTER, 0);
                } else {
                    self.command.buttons &= !(BUTTON_ALT_ATTACK | BUTTON_ATTACK);
                }
            }
            if self.state.weapon_time > 0 {
                self.state.weapon_time -= self.millis;
            }
            check_only_weapon = true;
        } else {
            if self.state.saber_entity_num == 0 && self.state.saber_in_flight {
                // Knocked out of the hand: a dual wielder turns its other blade off; nobody
                // attacks with the one that is gone.
                if self.state.saber_anim_level == SS_DUAL {
                    if self.state.saber_holstered > 1 {
                        self.state.saber_holstered = 1;
                    }
                } else {
                    self.command.buttons &= !BUTTON_ATTACK;
                }
                self.command.buttons &= !BUTTON_ALT_ATTACK;
            }
            if self.command.buttons & BUTTON_ALT_ATTACK != 0 && self.alternate_attack() {
                return;
            }
            if self.guiding() {
                return;
            }
            if self.state.health <= 0 {
                return;
            }
            // The weapon's time runs (`PM_CheckPullAttack` is compiled out of the game).
            if self.state.weapon_time > 0 {
                self.state.weapon_time -= self.millis;
            } else {
                self.state.weapon_state = WEAPON_READY;
            }
            if self.state.saber_blocked != 0 {
                self.answer_block();
                return;
            }
        }
        // `weapChecks`.
        if self.state.saber_entity_num != 0
            && self.state.weapon_time <= 0
            && self.state.torso_timer <= 0
            && self.state.weapon != self.command.weapon
        {
            crate::pmove_weapon::begin_weapon_change(
                self.state,
                self.command.weapon,
                self.lengths,
                self.events,
            );
        }
        if crate::pmove_saber_attack::can_do_kata(self) {
            let sabers = self.context.sabers;
            let over_ride = sabers.special(|saber| saber.kata_move, LS_NONE);
            match over_ride {
                None => {
                    let kata = match self.state.saber_anim_level {
                        SS_FAST | SS_TAVION => Some(LS_A1_SPECIAL),
                        SS_MEDIUM => Some(LS_A2_SPECIAL),
                        SS_STRONG | SS_DESANN => Some(LS_A3_SPECIAL),
                        SS_DUAL => Some(LS_DUAL_SPIN_PROTECT),
                        SS_STAFF => Some(LS_STAFF_SOULCAL),
                        _ => None,
                    };
                    if let Some(kata) = kata {
                        self.set_move(kata);
                    }
                    self.state.weapon_state = WEAPON_FIRING;
                    self.drain(SABER_ALT_ATTACK_POWER);
                    return;
                }
                Some(LS_NONE) => {}
                Some(chosen) => {
                    self.set_move(chosen);
                    self.state.weapon_state = WEAPON_FIRING;
                    self.drain(SABER_ALT_ATTACK_POWER);
                    return;
                }
            }
        }
        if self.state.weapon_time > 0 {
            return;
        }
        if self.state.weapon_state == WEAPON_DROPPING {
            self.finish_weapon_change();
            return;
        }
        if self.state.weapon_state == WEAPON_RAISING {
            self.state.weapon_state = WEAPON_IDLE;
            let legs = self.state.legs_anim;
            let torso = if RAISE_FOLLOWS_LEGS.contains(&legs) {
                legs
            } else {
                crate::pmove_saber_move::saber_stance(self)
            };
            self.animate(SETANIM_TORSO, torso, 0);
            if self.state.weapon_state == WEAPON_RAISING {
                return;
            }
        }
        if check_only_weapon {
            return;
        }
        if self.state.saber_anim_level == SS_STAFF
            && self.command.buttons & BUTTON_ALT_ATTACK != 0
            && self.staff_kick()
        {
            return;
        }
        // Never a regular saber attack button.
        self.command.buttons &= !BUTTON_ALT_ATTACK;
        self.attack();
    }

    /// `PM_FinishWeaponChange` (`bg_pmove.c:5833-5857`): the weapon asked for (none that
    /// the player lacks) raised — the saber drawn.
    pub(crate) fn finish_weapon_change(&mut self) {
        let requested = self.command.weapon;
        let weapon = if usize::from(requested) < 19 && self.state.weapons & (1 << requested) != 0 {
            requested
        } else {
            0
        };
        if weapon == 3 {
            self.set_move(LS_DRAW);
        } else {
            self.animate(SETANIM_TORSO, TORSO_RAISEWEAP1, SETANIM_FLAG_OVERRIDE);
        }
        self.state.weapon = weapon;
        self.state.weapon_state = WEAPON_RAISING;
        self.state.weapon_time += 250;
    }

    /// The alternate attack (`bg_saber.c:2963-3027`): a staff's kick out of a returning
    /// swing, else a throw. Returns whether the saber's code stops (a kick began).
    fn alternate_attack(&mut self) -> bool {
        if self.state.saber_anim_level == SS_STAFF {
            let state = &*self.state;
            if state.weapon_time > 0
                && rules::in_return(state.saber_move)
                && state.saber_blocked == 0
                && self.command.buttons & BUTTON_ATTACK == 0
                && (self.command.forward_move != 0 || self.command.right_move != 0)
                && crate::pmove_saber_attack::check_alt_kick(self)
            {
                let kick = crate::pmove_saber_attack::kick_for_conditions(self);
                if let Some(kick) = kick {
                    self.state.weapon_time = 0;
                    self.set_move(kick);
                    return true;
                }
            }
            return false;
        }
        // The throw: the saber's flight is the game's to follow, which it does not yet.
        if self.context.saber_throws && self.state.weapon_time < 1 {
            crate::pmove_saber_attack::throw(self);
        }
        false
    }

    /// A thrown saber guided (`bg_saber.c:3030-3048`): the torso pulls at it, unless a
    /// dual wielder attacks with the other.
    fn guiding(&mut self) -> bool {
        let state = &*self.state;
        if !(state.saber_in_flight && state.saber_entity_num != 0) {
            return false;
        }
        let torso = state.torso_anim;
        let idle_torso = torso == BOTH_SABERDUAL_STANCE
            || torso == BOTH_SABERPULL
            || torso == BOTH_STAND1
            || rules::running(torso)
            || rules::walking(torso)
            || rules::jumping(torso)
            || rules::swimming(torso);
        if state.saber_anim_level != SS_DUAL
            || state.saber_holstered != 0
            || (self.command.buttons & BUTTON_ATTACK == 0 && idle_torso)
        {
            self.animate(
                SETANIM_TORSO,
                BOTH_SABERPULL,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            self.state.torso_timer = 1;
            return true;
        }
        false
    }

    /// The saber's answer to a block the game raised (`bg_saber.c:3110-3264`): a bounce
    /// replayed, a parry broken, an attack bounced back, a parry or a reflection held.
    fn answer_block(&mut self) {
        let blocked = self.state.saber_blocked;
        let parrying = (BLOCKED_UPPER_RIGHT..BLOCKED_UPPER_RIGHT_PROJ).contains(&blocked);
        if parrying {
            self.state.weapon_time =
                PARRY_DEBOUNCE[usize::from(self.context.saber_defense.min(3))] + 200;
        }
        let current = self.state.saber_move as u16;
        match blocked {
            BLOCKED_BOUNCE_MOVE => {
                self.state.torso_timer = 0;
                self.set_move(current);
                self.state.weapon_time = self.state.torso_timer;
                self.state.saber_blocked = 0;
            }
            BLOCKED_PARRY_BROKEN => {
                let next = if rules::in_broken_parry(u32::from(current)) {
                    current
                } else {
                    rules::pm_broken_parry_for_parry(u32::from(current)) as u16
                };
                if next != LS_NONE {
                    self.set_move(next);
                    self.state.weapon_time = self.state.torso_timer;
                }
            }
            BLOCKED_ATK_BOUNCE => {
                if current >= LS_T1_BR__R {
                    self.state.saber_blocked = 0;
                } else {
                    let start = SABER_MOVES[usize::from(current)].start_quad;
                    let bounce = if rules::in_bounce(u32::from(current))
                        || !rules::in_attack(u32::from(current))
                    {
                        if self.command.buttons & BUTTON_ATTACK != 0 {
                            let mut quadrant =
                                crate::pmove_saber_attack::quadrant_for_movement(&self.command);
                            while quadrant == start {
                                quadrant = self.timesync(i32::from(Q_BR), i32::from(Q_BL)) as u8;
                            }
                            crate::pmove_saber::TRANSITION_MOVE[usize::from(start)]
                                [usize::from(quadrant)]
                        } else if start == Q_T {
                            LS_R_BL2TR
                        } else if start < Q_T {
                            LS_R_TL2BR + u16::from(start - Q_BR)
                        } else {
                            LS_R_BR2TL + u16::from(start - Q_TL)
                        }
                    } else {
                        rules::bounce_for_attack(u32::from(current)) as u16
                    };
                    self.set_move(bounce);
                    self.state.weapon_time = self.state.torso_timer;
                }
            }
            4 => self.set_move(LS_PARRY_UR),
            9 => self.set_move(LS_REFLECT_UR),
            5 => self.set_move(LS_PARRY_UL),
            10 => self.set_move(LS_REFLECT_UL),
            6 => self.set_move(LS_PARRY_LR),
            11 => self.set_move(LS_REFLECT_LR),
            7 => self.set_move(LS_PARRY_LL),
            12 => self.set_move(LS_REFLECT_LL),
            8 => self.set_move(LS_PARRY_UP),
            13 => self.set_move(LS_REFLECT_UP),
            _ => self.state.saber_blocked = 0,
        }
        if parrying && self.state.torso_timer < self.state.weapon_time {
            self.state.torso_timer = self.state.weapon_time;
        }
        self.state.saber_blocked = 0;
        self.state.weapon_state = WEAPON_READY;
    }

    /// The staff's kick on the alternate button (`bg_saber.c:3428-3491`): on the ground or
    /// high enough in the air, from the stance, not ducking. Returns whether one began.
    fn staff_kick(&mut self) -> bool {
        let state = &*self.state;
        let rolling = crate::pmove_roll_anim::in_roll(state.legs_anim) && state.legs_timer > 0;
        if rules::kicking(state.torso_anim)
            || rules::kicking(state.legs_anim)
            || rolling
            || state.saber_move != u32::from(LS_READY)
            || state.movement_flags & PMF_DUCKED != 0
            || self.command.up_move < 0
        {
            return false;
        }
        let Some(mut kick) = crate::pmove_saber_attack::kick_for_conditions(self) else {
            return false;
        };
        if self.state.ground_entity_number == ENTITY_NONE {
            let ground = crate::pmove_saber_attack::ground_distance(self);
            let state = &*self.state;
            if (!rules::flipping(state.legs_anim) || state.legs_timer <= 0)
                && ground > 64.0
                && ground > -state.velocity[2] - 64.0
            {
                kick = match kick {
                    LS_KICK_F => LS_KICK_F_AIR,
                    LS_KICK_B => LS_KICK_B_AIR,
                    LS_KICK_R => LS_KICK_R_AIR,
                    LS_KICK_L => LS_KICK_L_AIR,
                    _ => return false,
                };
            } else if ground > 128.0 || state.velocity[2] >= 0.0 {
                return false;
            }
        }
        self.set_move(kick);
        true
    }

    /// The attack (`bg_saber.c:3496-3740`): the next move of a swing, a transition or a
    /// bounce; the stance when the button is up; else the move the controls ask for,
    /// through its transition, and the weapon busy for its length.
    fn attack(&mut self) {
        let saber_move = self.state.saber_move;
        let current =
            if saber_move > u32::from(LS_NONE) && (saber_move as usize) < SABER_MOVES.len() {
                saber_move as u16
            } else {
                LS_READY
            };
        let mut next = LS_NONE;
        let mut animation: i32 = -1;
        if current == LS_A_JUMP_T__B_ || self.state.torso_anim == BOTH_FORCELEAP2_T__B_ {
            next = LS_R_T2B;
        } else if self.command.buttons & (BUTTON_ATTACK | BUTTON_ALT_ATTACK) == 0 {
            self.state.weapon_time = 0;
            if self.state.weapon_state != WEAPON_READY {
                self.state.weapon_state = WEAPON_IDLE;
            }
            if (LS_S_TL2BR..=LS_S_T2B).contains(&current) {
                next = LS_A_TL2BR + (current - LS_S_TL2BR);
            } else if (LS_A_TL2BR..=LS_A_T2B).contains(&current) {
                next = LS_R_TL2BR + (current - LS_A_TL2BR);
            } else if rules::in_transition(u32::from(current)) {
                next = SABER_MOVES[usize::from(current)].chain_attack;
            } else if rules::in_bounce(u32::from(current)) {
                next = SABER_MOVES[usize::from(current)].chain_idle;
            } else {
                self.set_move(LS_READY);
                return;
            }
        }
        if self.state.weapon_time > 0 {
            self.state.weapon_state = WEAPON_FIRING;
            return;
        }
        let mut both = false;
        let torso = self.state.torso_anim;
        if torso == BOTH_FORCELONGLEAP_ATTACK || torso == BOTH_FORCELONGLEAP_LAND {
            return;
        }
        if torso == BOTH_FORCELONGLEAP_START {
            if self.state.torso_timer >= 200 {
                self.set_move(LS_LEAP_ATTACK);
            }
            return;
        }
        if (LS_PARRY_UP..=LS_REFLECT_LL).contains(&current) {
            // From a parry or a reflection, straight into the attack from where it ended.
            next = match SABER_MOVES[usize::from(current)].end_quad {
                Q_T => LS_A_T2B,
                Q_TR => LS_A_TR2BL,
                Q_TL => LS_A_TL2BR,
                Q_BR => LS_A_BR2TL,
                Q_BL => LS_A_BL2TR,
                _ => next,
            };
        }
        if next != LS_NONE {
            animation = i32::from(SABER_MOVES[usize::from(next)].animation);
        }
        if animation == -1 {
            if rules::in_transition(u32::from(current)) {
                next = SABER_MOVES[usize::from(current)].chain_attack;
            } else if (LS_S_TL2BR..=LS_S_T2B).contains(&current) {
                next = LS_A_TL2BR + (current - LS_S_TL2BR);
            } else if rules::in_broken_parry(u32::from(current)) {
                next = LS_READY;
            } else {
                next = crate::pmove_saber_attack::attack_for_movement(self, current);
                let blocked_before = rules::in_bounce(u32::from(current))
                    || rules::in_broken_parry(u32::from(current));
                if blocked_before
                    && SABER_MOVES[usize::from(next)].start_quad
                        == SABER_MOVES[usize::from(current)].end_quad
                {
                    next = SABER_MOVES[usize::from(current)].chain_attack;
                }
                if crate::pmove_saber_attack::kata_done(self, current, next) {
                    next = SABER_MOVES[usize::from(current)].chain_idle;
                }
            }
            if next != LS_NONE {
                next = crate::pmove_saber_attack::transition(self, current, next);
                animation = i32::from(SABER_MOVES[usize::from(next)].animation);
            }
        }
        if animation == -1 {
            next = SABER_MOVES[usize::from(current)].chain_attack;
            animation = i32::from(SABER_MOVES[usize::from(next)].animation);
            if self.command.forward_move == 0
                && self.command.right_move == 0
                && self.command.up_move >= 0
                && self.state.ground_entity_number != ENTITY_NONE
            {
                both = true;
            }
        }
        if animation == -1 {
            animation = if MOVING_LEGS.contains(&self.state.legs_anim) {
                i32::from(self.state.legs_anim)
            } else {
                i32::from(crate::pmove_saber_move::saber_stance(self))
            };
            next = LS_READY;
        }
        self.set_move(next);
        if both && i32::from(self.state.torso_anim) == animation {
            self.animate(
                SETANIM_LEGS,
                animation as u16,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
        }
        self.state.weapon_time = self.state.torso_timer;
        // `WEAPON_FIRING`: the weapon busy for the move, at least its fire time.
        self.state.weapon_state = WEAPON_FIRING;
        if self.state.weapon_time == 0 {
            self.state.weapon_time = crate::weapon_data::legacy_weapon_data(self.state.weapon)
                .map_or(0, |data| data.primary_time);
        }
    }
}
