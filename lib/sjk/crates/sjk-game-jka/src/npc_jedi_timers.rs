//! A Jedi NPC's fight timers (`codemp/game/NPC_AI_Jedi.c:3821-4223`, `4889-5008`): the
//! debounced changes of direction (`Jedi_DebounceDirectionChanges`), the strafing, walking
//! and held Force buttons its timers apply (`Jedi_TimersApply`), the aggression and saber
//! style its fight's events change (`Jedi_CombatTimersUpdate`), and standing still for
//! the enemy's special moves (`Jedi_CheckEnemyMovement`).

use crate::npc_jedi_combat::{
    ENTITYNUM_NONE, FP_RAGE, PS_SABER_ANIM_LEVEL, PS_SABER_BLOCKED, PS_SABER_IN_FLIGHT,
    PS_WEAPON_TIME, WP_SABER, class, rank,
};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use crate::saber_clash::{normalize, sef};
use sjk_protocol::UserCommand;

/// `BUTTON_WALKING`, `BUTTON_FORCEGRIP`, `BUTTON_FORCE_LIGHTNING`, `BUTTON_FORCE_DRAIN`.
const BUTTON_WALKING: u16 = 16;
const BUTTON_FORCEGRIP: u16 = 64;
const BUTTON_FORCE_LIGHTNING: u16 = 1_024;
const BUTTON_FORCE_DRAIN: u16 = 2_048;
/// `WP_BRYAR_PISTOL` through `WP_ROCKET_LAUNCHER`: the ranged weapons whose wielder a Jedi
/// closes in on (`NPC_AI_Jedi.c:4048-4055`).
const RANGED_WEAPONS: std::ops::RangeInclusive<u8> = 4..=11;
/// `BLOCKED_PARRY_BROKEN`.
const BLOCKED_PARRY_BROKEN: u32 = 2;
/// `EV_GLOAT1` (`bg_public.h`: `EV_ANGER1` plus 71).
const EV_GLOAT1: i32 = crate::npc_enemy::EV_ANGER1 + 71;
/// The enemy's special moves the NPC stands still for.
const BOTH_A2_STABBACK1: u16 = 854;
const BOTH_JUMPFLIPSLASHDOWN1: u16 = 856;
const BOTH_JUMPFLIPSTABDOWN: u16 = 857;
const WALL_FLIPS: [u16; 5] = [1_247, 1_217, 1_218, 1_215, 1_212];

/// One axis of `Jedi_DebounceDirectionChanges`: the timers of its positive and negative
/// way and of standing still on it.
struct Axis {
    positive: &'static str,
    negative: &'static str,
    none: &'static str,
    /// `Q_irand` bounds of the positive and the negative way's hold.
    positive_hold: (i32, i32),
    negative_hold: (i32, i32),
}

const FORWARD_AXIS: Axis = Axis {
    positive: "moveforward",
    negative: "moveback",
    none: "movenone",
    positive_hold: (500, 2000),
    negative_hold: (250, 1000),
};
const RIGHT_AXIS: Axis = Axis {
    positive: "moveright",
    negative: "moveleft",
    none: "movecenter",
    positive_hold: (250, 1500),
    negative_hold: (250, 1500),
};

/// A move of 127 either way, as the reference renormalises the other axis to.
fn full(value: i8) -> i8 {
    match value {
        v if v > 0 => 127,
        v if v < 0 => -127,
        _ => 0,
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `Jedi_DebounceDirectionChanges` (`NPC_AI_Jedi.c:3821-3950`): a change of way on
    /// either axis waits for the other way's and the standstill's timers; with no move of
    /// its own, a way still held goes on.
    pub fn jedi_debounce_direction_changes(&mut self, me: usize, command: &mut UserCommand) {
        let (mut forward, mut right) = (command.forward_move, command.right_move);
        self.jedi_debounce_axis(me, &FORWARD_AXIS, &mut forward, &mut right);
        let (mut right_now, mut forward_now) = (right, forward);
        self.jedi_debounce_axis(me, &RIGHT_AXIS, &mut right_now, &mut forward_now);
        command.forward_move = forward_now;
        command.right_move = right_now;
    }

    /// One axis of [`Self::jedi_debounce_direction_changes`]: `value` the move on it,
    /// `other` the move on the other axis, renormalised when this one is cancelled.
    fn jedi_debounce_axis(&mut self, me: usize, axis: &Axis, value: &mut i8, other: &mut i8) {
        let level_time = self.level_time;
        let (way, against, hold) = match *value {
            v if v > 0 => (axis.positive, axis.negative, axis.positive_hold),
            v if v < 0 => (axis.negative, axis.positive, axis.negative_hold),
            _ => {
                if !self.jedi_timer_done(me, axis.positive) {
                    *value = 127;
                    self.actors[me].mind.move_dir = [0.0; 3];
                } else if !self.jedi_timer_done(me, axis.negative) {
                    *value = -127;
                    self.actors[me].mind.move_dir = [0.0; 3];
                }
                return;
            }
        };
        if !self.jedi_timer_done(me, against) || !self.jedi_timer_done(me, axis.none) {
            *value = 0;
            *other = full(*other);
            self.actors[me].mind.move_dir = [0.0; 3];
            self.jedi_timer_set(me, against, -level_time);
            if self.jedi_timer_done(me, axis.none) {
                let still = self.host.irand(1000, 2000);
                self.jedi_timer_set(me, axis.none, still);
            }
        } else if self.jedi_timer_done(me, way) {
            let duration = self.host.irand(hold.0, hold.1);
            self.jedi_timer_set(me, way, duration);
        }
    }

    /// `Jedi_TimersApply` (`NPC_AI_Jedi.c:3952-4010`): a strafe its timers hold, unless the
    /// NPC wants to turn more than 60 degrees against it; the debounced changes of way;
    /// walking while it stands or taunts; the grip, drain and lightning held down.
    pub fn jedi_timers_apply(&mut self, me: usize, command: &mut UserCommand) {
        if command.right_move == 0 {
            let npc = &self.actors[me];
            let (desired, yaw) = (npc.desired_yaw, npc.player.view_angles()[1]);
            if !self.jedi_timer_done(me, "strafeLeft") {
                if desired <= yaw + 60.0 {
                    command.right_move = -127;
                    self.actors[me].mind.move_dir = [0.0; 3];
                }
            } else if !self.jedi_timer_done(me, "strafeRight") && desired >= yaw - 60.0 {
                command.right_move = 127;
                self.actors[me].mind.move_dir = [0.0; 3];
            }
        }
        self.jedi_debounce_direction_changes(me, command);
        if command.forward_move == 0 && !self.jedi_timer_done(me, "walking") {
            command.buttons |= BUTTON_WALKING;
        }
        if !self.jedi_timer_done(me, "taunting") {
            command.buttons |= BUTTON_WALKING;
        }
        if !self.jedi_timer_done(me, "gripping") {
            command.buttons |= BUTTON_FORCEGRIP;
        }
        if !self.jedi_timer_done(me, "draining") {
            command.buttons |= BUTTON_FORCE_DRAIN;
        }
        if !self.jedi_timer_done(me, "holdLightning") {
            command.buttons |= BUTTON_FORCE_LIGHTNING;
        }
    }

    /// `Jedi_CombatTimersUpdate` (`NPC_AI_Jedi.c:4012-4223`): the cultist destroyer ever
    /// closing in; every two to five seconds, aggression changed by rage and by the enemy's
    /// weapon; a strafe begun or put off; and the saber events of its last swings
    /// answered with aggression, a faster or stronger style and a gloat. `command` is the
    /// frame's (`NPCS.ucmd`): `Jedi_Strafe` reads its forward move.
    pub fn jedi_combat_timers_update(
        &mut self,
        me: usize,
        enemy_dist: i32,
        command: &mut UserCommand,
    ) {
        let npc = &self.actors[me];
        if crate::npc_behavior::cultist_destroyer(
            npc.definition.client_class,
            npc.player.weapon(),
            &npc.npc_type,
        ) {
            self.jedi_aggression(me, 5);
            return;
        }
        if self.jedi_timer_done(me, "roamTime") {
            let roam = self.host.irand(2000, 5000);
            self.jedi_timer_set(me, "roamTime", roam);
            self.jedi_rage_aggression(me);
            self.jedi_weapon_aggression(me, enemy_dist);
        }
        if self.jedi_timer_done(me, "noStrafe")
            && self.jedi_timer_done(me, "strafeLeft")
            && self.jedi_timer_done(me, "strafeRight")
        {
            if self.host.irand(0, 4) == 0 {
                self.jedi_strafe(me, 1000, 3000, 0, 4000, true, command);
            } else {
                let postpone = self.host.irand(1000, 3000);
                self.jedi_timer_set(me, "noStrafe", postpone);
            }
        }
        if self.actors[me].saber.event_flags != 0 {
            self.jedi_saber_events(me);
        }
    }

    /// Rage raises aggression, recovering from it lowers it (`NPC_AI_Jedi.c:4023-4030`).
    fn jedi_rage_aggression(&mut self, me: usize) {
        let state = &self.actors[me].player;
        if state.force_powers_active() & (1 << FP_RAGE) != 0 {
            let change = self.host.irand(0, 3);
            self.jedi_aggression(me, change);
        } else if state.force_rage_recovery_time() > self.level_time {
            let change = self.host.irand(-2, 0);
            self.jedi_aggression(me, change);
        }
    }

    /// Aggression by the enemy's weapon (`NPC_AI_Jedi.c:4031-4073`): closer against a
    /// saber, closer still when it is off; closer against a gun not firing, or too near
    /// to deflect its shots.
    fn jedi_weapon_aggression(&mut self, me: usize, enemy_dist: i32) {
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.client_view(enemy))
        else {
            return;
        };
        if enemy.weapon == WP_SABER {
            self.jedi_aggression(me, if enemy.sabers_off { 2 } else { 1 });
        } else if RANGED_WEAPONS.contains(&enemy.weapon) {
            if enemy.attack_debounce_time < self.level_time {
                self.jedi_aggression(me, 1);
            }
            if enemy_dist < 256 {
                self.jedi_aggression(me, 1);
            }
        }
    }

    /// `ps.fd.saberAnimLevel` moved by `step` (`Jedi_AdjustSaberAnimLevel` clamps it).
    fn jedi_step_saber_style(&mut self, me: usize, step: i32) {
        let level = self.actors[me]
            .player
            .raw_field(PS_SABER_ANIM_LEVEL)
            .unwrap_or(0) as i32;
        self.jedi_adjust_saber_anim_level(me, level + step);
    }

    /// The saber events answered (`NPC_AI_Jedi.c:4095-4222`), each flag cleared once read.
    fn jedi_saber_events(&mut self, me: usize) {
        let flags = self.actors[me].saber.event_flags;
        let mut new_flags = flags;
        if flags & sef::PARRIED != 0 {
            self.jedi_timer_set(me, "parryTime", -1);
            let enemy_knocks_away = self.actors[me]
                .mind
                .enemy
                .and_then(|enemy| self.client_view(enemy))
                .is_some_and(|enemy| crate::saber_rules::in_knockaway(enemy.saber_move));
            if enemy_knocks_away {
                self.jedi_aggression(me, 1);
                self.jedi_step_saber_style(me, -1);
            } else {
                if self.host.irand(0, 1) == 0 {
                    self.jedi_aggression(me, -1);
                }
                if self.host.irand(0, 1) == 0 {
                    self.jedi_step_saber_style(me, -1);
                }
            }
            new_flags &= !sef::PARRIED;
        }
        if self.actors[me]
            .player
            .raw_field(PS_WEAPON_TIME)
            .unwrap_or(0)
            == 0
            && flags & sef::HIT_ENEMY != 0
        {
            if self.host.irand(0, 1) == 0 {
                self.jedi_aggression(me, -1);
                self.jedi_gloat(me);
            }
            if self.host.irand(0, 2) == 0 {
                self.jedi_step_saber_style(me, 1);
            }
            new_flags &= !sef::HIT_ENEMY;
        }
        if flags & sef::BLOCKED != 0 {
            self.jedi_blocked_event(me);
            new_flags &= !sef::BLOCKED;
        }
        if flags & sef::DEFLECTED != 0 {
            new_flags &= !sef::DEFLECTED;
            if self.host.irand(0, 3) == 0 {
                self.jedi_step_saber_style(me, -1);
            }
        }
        if flags & sef::HIT_WALL != 0 {
            new_flags &= !sef::HIT_WALL;
        }
        if flags & sef::HIT_OBJECT != 0 {
            if self.host.irand(0, 3) == 0 {
                self.jedi_step_saber_style(me, -1);
            }
            new_flags &= !sef::HIT_OBJECT;
        }
        self.actors[me].saber.event_flags = new_flags;
    }

    /// A gloat now and then after hitting the enemy (`NPC_AI_Jedi.c:4144-4151`).
    fn jedi_gloat(&mut self, me: usize) {
        if self.host.irand(0, 3) != 0 {
            return;
        }
        let level_time = self.level_time;
        let npc = &self.actors[me];
        if npc.mind.blocked_speech_until < level_time
            && self.jedi_speech_debounce(me) < level_time
            && npc.mind.fight.pain_debounce_time < level_time - 1000
        {
            let event = self.host.irand(EV_GLOAT1, EV_GLOAT1 + 2);
            self.add_voice(me, event, 3000);
            self.jedi_hush(me, level_time + 3000);
        }
    }

    /// Blocked while attacking (`NPC_AI_Jedi.c:4159-4200`): knocked back, much less
    /// aggressive — far less without its saber — and a stronger style; merely blocked, by
    /// chance a little less aggressive and stronger.
    fn jedi_blocked_event(&mut self, me: usize) {
        let state = &self.actors[me].player;
        if crate::saber_rules::in_broken_parry(state.saber_move())
            || state.raw_field(PS_SABER_BLOCKED).unwrap_or(0) == BLOCKED_PARRY_BROKEN
        {
            let in_flight = state.raw_field(PS_SABER_IN_FLIGHT).unwrap_or(0) != 0;
            self.jedi_aggression(me, if in_flight { -5 } else { -2 });
            self.jedi_step_saber_style(me, 1);
        } else {
            if self.host.irand(0, 2) == 0 {
                self.jedi_aggression(me, -1);
            }
            if self.host.irand(0, 1) == 0 {
                self.jedi_step_saber_style(me, 1);
            }
        }
    }

    /// `Jedi_CheckEnemyMovement` (`NPC_AI_Jedi.c:4889-5008`): all but Tavion, Desann, Luke
    /// and Yoda, when the enemy's special move is aimed at them, stand still for it by a
    /// roll against their rank — for a flip over them, a flip off a wall nearby (stepping
    /// to 64 units behind where it flies) or a backward stab (stepping to 32 behind it).
    pub fn jedi_check_enemy_movement(
        &mut self,
        me: usize,
        enemy_dist: f32,
        command: &mut UserCommand,
    ) {
        let npc = &self.actors[me];
        let class = npc.definition.client_class;
        if class == class::TAVION
            || class == class::DESANN
            || class == class::LUKE
            || npc.npc_type.eq_ignore_ascii_case(b"yoda")
        {
            return;
        }
        let Some(enemy) = npc.mind.enemy.and_then(|enemy| self.client_view(enemy)) else {
            return;
        };
        if enemy.enemy != Some(npc.number) {
            return;
        }
        let npc_rank = npc.definition.rank;
        if enemy.legs_anim == BOTH_JUMPFLIPSLASHDOWN1 || enemy.legs_anim == BOTH_JUMPFLIPSTABDOWN {
            if self.host.irand(0, npc_rank) < rank::LT {
                self.jedi_stand_still(me, command);
                self.jedi_hold_still_timers(me);
            }
        } else if WALL_FLIPS.contains(&enemy.legs_anim) {
            if enemy.ground_entity == ENTITYNUM_NONE
                && enemy_dist < 256.0
                && self.host.irand(0, npc_rank) < rank::LT
            {
                self.jedi_stand_still(me, command);
                let skill = self.host.skill();
                let noturn = self.host.irand(250, 500) * (3 - skill);
                self.jedi_timer_set(me, "noturn", noturn);
                let mut ahead = enemy.velocity;
                normalize(&mut ahead);
                self.jedi_step_behind(me, enemy.origin, -64.0, ahead, 32.0, command);
            }
        } else if enemy.legs_anim == BOTH_A2_STABBACK1
            && enemy_dist < 256.0
            && enemy_dist > 64.0
            && !crate::saber_block::in_front(
                self.actors[me].current_origin,
                enemy.origin,
                enemy.current_angles,
                0.0,
            )
            && self.host.irand(0, npc_rank) == 0
        {
            self.jedi_stand_still(me, command);
            let ahead = crate::pmove::flight::flight_axes(enemy.current_angles)
                .0
                .to_array();
            self.jedi_step_behind(me, enemy.origin, -32.0, ahead, 64.0, command);
        }
    }

    /// No move, no charged jump, no strafe for a while (`NPC_AI_Jedi.c:4908-4913`).
    fn jedi_stand_still(&mut self, me: usize, command: &mut UserCommand) {
        command.forward_move = 0;
        command.right_move = 0;
        command.up_move = 0;
        let npc = &mut self.actors[me];
        npc.mind.move_dir = [0.0; 3];
        npc.force.jump_charge = 0.0;
        self.jedi_timer_set(me, "strafeLeft", -1);
        self.jedi_timer_set(me, "strafeRight", -1);
        let hold = self.host.irand(500, 1000);
        self.jedi_timer_set(me, "noStrafe", hold);
    }

    /// `movenone` and `movecenter` held half a second to a second.
    fn jedi_hold_still_timers(&mut self, me: usize) {
        let none = self.host.irand(500, 1000);
        self.jedi_timer_set(me, "movenone", none);
        let center = self.host.irand(500, 1000);
        self.jedi_timer_set(me, "movecenter", center);
    }

    /// A step to `scale` units along `ahead` from the enemy's `origin`, when that is more
    /// than `near` away; else staying still (`NPC_AI_Jedi.c:4952-4964`, `4987-4998`).
    fn jedi_step_behind(
        &mut self,
        me: usize,
        origin: [f32; 3],
        scale: f32,
        ahead: [f32; 3],
        near: f32,
        command: &mut UserCommand,
    ) {
        let dest: [f32; 3] = std::array::from_fn(|axis| origin[axis] + scale * ahead[axis]);
        let mut dir = crate::npc_senses::subtract(dest, self.actors[me].current_origin);
        if normalize(&mut dir) > near {
            self.ucmd_move_for_dir(me, command, &mut dir);
        } else {
            self.jedi_hold_still_timers(me);
        }
    }
}
