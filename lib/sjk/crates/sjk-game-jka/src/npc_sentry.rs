//! The sentry droid's AI (`codemp/game/NPC_AI_Sentry.c`): its behaviour
//! (`NPC_BSSentry_Default`, `568-595`), its pain (`NPC_Sentry_Pain`, `97-127`: a DEMP2 shot
//! shuts it up behind its shields for nine seconds and more), its fire from its three muzzles
//! (`Sentry_Fire`, `134-231`: the shields opened, a quarter second's power-up, then a bolt
//! from each muzzle in turn), its hover (`Sentry_MaintainHeight`, `238-339`), its idle
//! (`346-367`), strafe and hunt (`374-446`), its bursts and the shields closed after them
//! (`Sentry_RangedAttack`, `453-482`), its fight (`Sentry_AttackDecision`, `489-534`) and its
//! patrol (`543-561`).
//!
//! `FL_SHIELDED` is the flag the missiles read (`G_MissileImpact` deflects a bolt off a
//! shielded entity, `g_missile.c:467-493`). `sentry_use` (`81-90`, the sentry woken by a
//! trigger) is [`NpcWorld::sentry_use`]: the reference installs it on a named sentry each
//! think; nothing uses an NPC by name in this port yet.

use crate::means_of_death::{MOD_BRYAR_PISTOL, MOD_DEMP2, MOD_DEMP2_ALT};
use crate::npc_creature::CHAN_AUTO;
use crate::npc_jedi_patrol::distance_horizontal_squared;
use crate::npc_machine::{
    BUTTON_WALKING, MachineBolt, PS_TORSO_TIMER, SCF_CHASE_ENEMIES, SCF_LOOK_FOR_ENEMIES,
    WP_BRYAR_PISTOL, damped,
};
use crate::npc_spawn::{FL_SHIELDED, NpcHost};
use crate::npc_world::NpcWorld;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
use sjk_protocol::UserCommand;

/// `LSTATE_WAKEUP`, `LSTATE_ACTIVE`, `LSTATE_POWERING_UP`, `LSTATE_ATTACKING`.
const LSTATE_WAKEUP: i32 = 2;
const LSTATE_ACTIVE: i32 = 3;
const LSTATE_POWERING_UP: i32 = 4;
const LSTATE_ATTACKING: i32 = 5;
/// `MIN_DISTANCE_SQR`, `SENTRY_FORWARD_BASE_SPEED`, `SENTRY_FORWARD_MULTIPLIER`,
/// `SENTRY_VELOCITY_DECAY`, `SENTRY_STRAFE_VEL`, `SENTRY_STRAFE_DIS`, `SENTRY_UPWARD_PUSH`,
/// `SENTRY_HOVER_HEIGHT`.
const MIN_DISTANCE_SQR: i32 = 256 * 256;
const FORWARD_BASE_SPEED: i32 = 10;
const FORWARD_MULTIPLIER: i32 = 5;
const VELOCITY_DECAY: f32 = 0.85;
const STRAFE_VEL: i32 = 256;
const STRAFE_DIS: i32 = 200;
const UPWARD_PUSH: f32 = 32.0;
const HOVER_HEIGHT: f32 = 24.0;
/// `BOTH_ATTACK1`, `BOTH_FLY_SHIELDED`, `BOTH_SLEEP1`, `BOTH_POWERUP1`.
const BOTH_ATTACK1: u16 = 113;
const BOTH_FLY_SHIELDED: u16 = 1_309;
const BOTH_SLEEP1: u16 = 1_313;
const BOTH_POWERUP1: u16 = 1_323;
/// Its three muzzles, in the order it fires from them.
const MUZZLES: [&str; 3] = ["*flash1", "*flash2", "*flash03"];
/// `SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD`.
const HELD: u8 = SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD;

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSSentry_Default` (`568-595`).
    pub fn bs_sentry_default(&mut self, me: usize, command: &mut UserCommand) {
        let npc = &self.actors[me];
        if npc.mind.enemy.is_some() && npc.mind.fight.local_state != LSTATE_WAKEUP {
            self.sentry_attack_decision(me, command);
        } else if npc.script_flags & SCF_LOOK_FOR_ENEMIES != 0 {
            self.sentry_patrol(me, command);
        } else {
            self.sentry_idle(me, command);
        }
    }

    /// `sentry_use` (`81-90`): the shields dropped, the power-up played, and awake.
    pub fn sentry_use(&mut self, me: usize) {
        self.actors[me].flags &= !FL_SHIELDED;
        self.set_animation(me, SETANIM_BOTH, BOTH_POWERUP1, HELD);
        self.actors[me].mind.fight.local_state = LSTATE_ACTIVE;
    }

    /// `NPC_Sentry_Pain` (`97-127`).
    pub(crate) fn sentry_pain(
        &mut self,
        me: usize,
        attacker: Option<u16>,
        damage: i32,
        means: u32,
    ) {
        self.npc_pain(me, attacker, damage, means);
        if means == MOD_DEMP2 || means == MOD_DEMP2_ALT {
            self.actors[me].mind.fight.burst_count = 0;
            let delay = self.host.irand(9_000, 12_000);
            let level_time = self.level_time;
            self.actors[me]
                .mind
                .timers
                .set("attackDelay", level_time, delay);
            self.actors[me].flags |= FL_SHIELDED;
            self.set_animation(me, SETANIM_BOTH, BOTH_FLY_SHIELDED, HELD);
            let number = self.actors[me].number;
            self.creature_sound(number, CHAN_AUTO, b"sound/chars/sentry/misc/sentry_pain");
            self.actors[me].mind.fight.local_state = LSTATE_ACTIVE;
        }
    }

    /// `Sentry_Fire` (`134-231`).
    fn sentry_fire(&mut self, me: usize) {
        let level_time = self.level_time;
        self.actors[me].flags &= !FL_SHIELDED;
        match self.actors[me].mind.fight.local_state {
            LSTATE_POWERING_UP => {
                if !self.actors[me].mind.timers.done("powerup", level_time) {
                    return;
                }
                self.actors[me].mind.fight.local_state = LSTATE_ATTACKING;
                self.set_animation(me, SETANIM_BOTH, BOTH_ATTACK1, HELD);
            }
            LSTATE_ACTIVE => {
                self.actors[me].mind.fight.local_state = LSTATE_POWERING_UP;
                let number = self.actors[me].number;
                self.creature_sound(
                    number,
                    CHAN_AUTO,
                    b"sound/chars/sentry/misc/sentry_shield_open",
                );
                self.set_animation(me, SETANIM_BOTH, BOTH_POWERUP1, HELD);
                self.actors[me].mind.timers.set("powerup", level_time, 250);
                return;
            }
            LSTATE_ATTACKING => {}
            _ => {
                // "bad because we are uninitialized"
                self.actors[me].mind.fight.local_state = LSTATE_ACTIVE;
                return;
            }
        }
        let which = self.actors[me].mind.fight.burst_count.rem_euclid(3) as usize;
        let muzzle = self.machine_muzzle(me, MUZZLES[which]);
        let forward = Self::forward_of(self.actors[me].mind.current_angles);
        self.play_effect_at(b"bryar/muzzle_flash", muzzle, forward);
        let skill = self.host.skill();
        let damage = match skill {
            0 => 1,
            1 => 3,
            _ => 5,
        };
        let number = self.actors[me].number;
        self.machine_missile(
            me,
            muzzle,
            forward,
            MachineBolt {
                weapon: WP_BRYAR_PISTOL,
                damage,
                means: MOD_BRYAR_PISTOL,
                speed: 1_600.0,
            },
            number,
        );
        let fight = &mut self.actors[me].mind;
        fight.fight.burst_count += 1;
        fight.attack_debounce_time = level_time
            + 50
            + match skill {
                0 => 200,
                1 => 100,
                _ => 0,
            };
    }

    /// `Sentry_MaintainHeight` (`238-339`): its hum, its angles, a hover at its enemy's head
    /// height (or its goal's), its drift damped, and its face to its enemy.
    fn sentry_maintain_height(&mut self, me: usize, command: &mut UserCommand) {
        self.machine_loop_sound(me, b"sound/chars/sentry/misc/sentry_hover_1_lp");
        self.update_angles(me, true, true, command);
        let origin = self.actors[me].current_origin;
        let mut velocity = self.actors[me].player.velocity();
        if let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        {
            let mut dif = (enemy.origin[2] + enemy.maxs[2]) - origin[2];
            if dif.abs() > 8.0 {
                if dif.abs() > HOVER_HEIGHT {
                    dif = if dif < 0.0 { -24.0 } else { 24.0 };
                }
                velocity[2] = (velocity[2] + dif) / 2.0;
            }
        } else if let Some(goal) = self.goal_origin(me) {
            // No `lastGoalEntity` is kept: the goal alone.
            if (goal[2] - origin[2]).abs() > HOVER_HEIGHT {
                command.up_move = if command.up_move < 0 { -4 } else { 4 };
            } else {
                velocity[2] = damped(velocity[2], VELOCITY_DECAY, 2.0);
            }
        } else {
            velocity[2] = damped(velocity[2], VELOCITY_DECAY, 1.0);
        }
        velocity[0] = damped(velocity[0], VELOCITY_DECAY, 1.0);
        velocity[1] = damped(velocity[1], VELOCITY_DECAY, 1.0);
        self.actors[me].player.set_velocity(velocity);
        self.face_enemy(me, true, command);
    }

    /// `Sentry_Idle` (`346-367`): waking, it looks for enemies once its power-up is over;
    /// else asleep behind its shields, idling.
    fn sentry_idle(&mut self, me: usize, command: &mut UserCommand) {
        self.sentry_maintain_height(me, command);
        let npc = &mut self.actors[me];
        if npc.mind.fight.local_state == LSTATE_WAKEUP {
            if npc.player.raw_field(PS_TORSO_TIMER).unwrap_or(0) as i32 <= 0 {
                npc.script_flags |= SCF_LOOK_FOR_ENEMIES;
                npc.mind.fight.burst_count = 0;
            }
        } else {
            self.set_animation(me, SETANIM_BOTH, BOTH_SLEEP1, HELD);
            self.actors[me].flags |= FL_SHIELDED;
            self.machine_idle(me, command);
        }
    }

    /// `Sentry_Hunt(visible, advance)` (`391-446`): a strafe while it may; closing in when it
    /// should advance or cannot see its enemy.
    fn sentry_hunt(&mut self, me: usize, visible: bool, advance: bool, command: &mut UserCommand) {
        if self.actors[me].mind.stand_time < self.level_time && visible {
            self.machine_strafe(me, STRAFE_DIS, STRAFE_VEL, UPWARD_PUSH, None);
            return;
        }
        if !advance && visible {
            return;
        }
        if !visible {
            if let Some(forward) = self.machine_seek_unseen(me, 12, command) {
                let speed = (FORWARD_BASE_SPEED + FORWARD_MULTIPLIER * self.host.skill()) as f32;
                self.machine_push(me, forward, speed);
            }
            return;
        }
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        else {
            return;
        };
        let speed = (FORWARD_BASE_SPEED + FORWARD_MULTIPLIER * self.host.skill()) as f32;
        self.machine_advance(me, enemy.origin, speed);
    }

    /// `Sentry_RangedAttack(visible, advance)` (`453-482`): a burst of seven, a pause of half
    /// a second or more, then the shields closed for two to three and a half seconds.
    fn sentry_ranged_attack(
        &mut self,
        me: usize,
        visible: bool,
        advance: bool,
        command: &mut UserCommand,
    ) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        if npc.mind.timers.done("attackDelay", level_time)
            && npc.mind.attack_debounce_time < level_time
            && visible
        {
            if npc.mind.fight.burst_count > 6 {
                let closing = npc.mind.creature.machine.fly_sound_debounce_time;
                if closing == 0 {
                    // "delay closing down to give the player an opening"
                    let delay = self.host.irand(500, 2_000);
                    self.actors[me]
                        .mind
                        .creature
                        .machine
                        .fly_sound_debounce_time = level_time + delay;
                } else if closing < level_time {
                    let npc = &mut self.actors[me];
                    npc.mind.fight.local_state = LSTATE_ACTIVE;
                    npc.mind.creature.machine.fly_sound_debounce_time = 0;
                    npc.mind.fight.burst_count = 0;
                    let delay = self.host.irand(2_000, 3_500);
                    self.actors[me]
                        .mind
                        .timers
                        .set("attackDelay", level_time, delay);
                    self.actors[me].flags |= FL_SHIELDED;
                    self.set_animation(me, SETANIM_BOTH, BOTH_FLY_SHIELDED, HELD);
                    self.sound_on_entity(me, b"sound/chars/sentry/misc/sentry_shield_close");
                }
            } else {
                self.sentry_fire(me);
            }
        }
        if self.actors[me].script_flags & SCF_CHASE_ENEMIES != 0 {
            self.sentry_hunt(me, visible, advance, command);
        }
    }

    /// `Sentry_AttackDecision` (`489-534`).
    fn sentry_attack_decision(&mut self, me: usize, command: &mut UserCommand) {
        self.sentry_maintain_height(me, command);
        self.machine_loop_sound(me, b"sound/chars/sentry/misc/sentry_hover_2_lp");
        self.sentry_talk(me, 4_000, 10_000, true);
        let enemy = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy));
        if enemy.is_none_or(|enemy| enemy.health < 1) {
            self.actors[me].mind.enemy = None;
            self.sentry_idle(me, command);
            return;
        }
        if !self.check_enemy_ext(me) {
            self.sentry_idle(me, command);
            return;
        }
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        else {
            return;
        };
        // `(int)DistanceHorizontalSquared` into a float.
        let distance =
            distance_horizontal_squared(self.actors[me].current_origin, enemy.origin) as i32 as f32;
        let visible = self.clear_los4(me, &enemy);
        let advance = distance > MIN_DISTANCE_SQR as f32;
        if !visible && self.actors[me].script_flags & SCF_CHASE_ENEMIES != 0 {
            self.sentry_hunt(me, visible, advance, command);
            return;
        }
        self.face_enemy(me, true, command);
        self.sentry_ranged_attack(me, visible, advance, command);
    }

    /// Its chatter now and then (`talk1..3`): unless angry, when `angry_waits`.
    fn sentry_talk(&mut self, me: usize, least: i32, most: i32, angry_waits: bool) {
        let level_time = self.level_time;
        let timers = &self.actors[me].mind.timers;
        if timers.done("patrolNoise", level_time)
            && (!angry_waits || timers.done("angerNoise", level_time))
        {
            let talk = self.host.irand(1, 3);
            self.sound_on_entity(me, format!("sound/chars/sentry/misc/talk{talk}").as_bytes());
            let delay = self.host.irand(least, most);
            self.actors[me]
                .mind
                .timers
                .set("patrolNoise", level_time, delay);
        }
    }

    /// `NPC_Sentry_Patrol` (`543-561`): an enemy of its team noticed is faced; else to its
    /// goal walking, chattering.
    fn sentry_patrol(&mut self, me: usize, command: &mut UserCommand) {
        self.sentry_maintain_height(me, command);
        if self.actors[me].mind.enemy.is_none() {
            if self.check_player_team_stealth(me) {
                self.update_angles(me, true, true, command);
                return;
            }
            if self.update_goal(me, command).is_some() {
                command.buttons |= BUTTON_WALKING;
                self.move_to_goal(me, true, command);
            }
            self.sentry_talk(me, 2_000, 4_000, false);
        }
        self.update_angles(me, true, true, command);
    }
}
