//! The imperial probe droid's AI (`codemp/game/NPC_AI_ImperialProbe.c`), which
//! `NPC_BehaviorSet_ImperialProbe` (`NPC.c:982-996`) runs: its behaviour
//! (`NPC_BSImperialProbe_Default`, `590-617`), its hover (`ImperialProbe_MaintainHeight`,
//! `71-160`), strafe and hunt (`167-247`), its blaster from its `*flash` bolt
//! (`ImperialProbe_FireBlaster`, `254-310`), its fire (`ImperialProbe_Ranged`, `317-353`), its
//! fight (`ImperialProbe_AttackDecision`, `364-405`), its pain (`NPC_Probe_Pain`, `412-486`: a
//! DEMP2 shot, or a low health with nothing below it, drops it spinning), its idle (`493-498`),
//! its patrol (`505-546`) and its fall (`ImperialProbe_Wait`, `553-574`: it blows where it
//! meets the floor).

use crate::means_of_death::{MOD_DEMP2, MOD_DEMP2_ALT, MOD_UNKNOWN};
use crate::npc_droid::angle_normalize360;
use crate::npc_jedi_patrol::{distance_horizontal_squared, normalized};
use crate::npc_machine::{
    BUTTON_WALKING, MASK_SOLID, MachineBolt, PS_ELECTRIFY_TIME, SCF_CHASE_ENEMIES,
    SCF_LOOK_FOR_ENEMIES, WP_BRYAR_PISTOL, damped,
};
use crate::npc_senses::{Spot, spot};
use crate::npc_spawn::{NpcHost, es};
use crate::npc_world::NpcWorld;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
use sjk_protocol::UserCommand;

/// `LSTATE_DROP`.
const LSTATE_DROP: i32 = 4;
/// `VELOCITY_DECAY`, `HUNTER_STRAFE_VEL`, `HUNTER_STRAFE_DIS`, `HUNTER_UPWARD_PUSH`,
/// `HUNTER_FORWARD_BASE_SPEED`, `HUNTER_FORWARD_MULTIPLIER`, `MIN_DISTANCE_SQR`.
const VELOCITY_DECAY: f32 = 0.85;
const STRAFE_VEL: i32 = 256;
const STRAFE_DIS: i32 = 200;
const UPWARD_PUSH: f32 = 32.0;
const FORWARD_BASE_SPEED: i32 = 10;
const FORWARD_MULTIPLIER: i32 = 5;
const MIN_DISTANCE_SQR: i32 = 128 * 128;
/// `BOTH_PAIN1`, `BOTH_RUN1`; `SETANIM_FLAG_NORMAL`.
const BOTH_PAIN1: u16 = 95;
const BOTH_RUN1: u16 = 1_111;
const NORMAL: u8 = 0;

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSImperialProbe_Default` (`590-617`).
    pub fn bs_imperial_probe_default(&mut self, me: usize, command: &mut UserCommand) {
        let npc = &mut self.actors[me];
        if npc.mind.enemy.is_some() {
            npc.mind.goal = npc.mind.enemy;
            self.probe_attack_decision(me, command);
        } else if npc.script_flags & SCF_LOOK_FOR_ENEMIES != 0 {
            self.probe_patrol(me, command);
        } else if npc.mind.fight.local_state == LSTATE_DROP {
            self.probe_wait(me, command);
        } else {
            self.probe_idle(me, command);
        }
    }

    /// `ImperialProbe_MaintainHeight` (`71-160`): its angles, a hover at its enemy's height
    /// (or its goal's), its drift damped.
    fn probe_maintain_height(&mut self, me: usize, command: &mut UserCommand) {
        self.update_angles(me, true, true, command);
        let origin = self.actors[me].current_origin;
        let mut velocity = self.actors[me].player.velocity();
        if let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        {
            let mut dif = enemy.origin[2] - origin[2];
            if dif.abs() > 8.0 {
                if dif.abs() > 16.0 {
                    dif = if dif < 0.0 { -16.0 } else { 16.0 };
                }
                velocity[2] = (velocity[2] + dif) / 2.0;
            }
        } else if let Some(goal) = self.goal_origin(me) {
            // No `lastGoalEntity` is kept: the goal alone.
            if (goal[2] - origin[2]).abs() > 24.0 {
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
    }

    /// `ImperialProbe_Hunt(visible, advance)` (`203-247`): running; a strafe while it may,
    /// else closing in when it should.
    fn probe_hunt(&mut self, me: usize, visible: bool, advance: bool, command: &mut UserCommand) {
        self.set_animation(
            me,
            SETANIM_BOTH,
            BOTH_RUN1,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        );
        if self.actors[me].mind.stand_time < self.level_time && visible {
            self.machine_strafe(me, STRAFE_DIS, STRAFE_VEL, UPWARD_PUSH, None);
            return;
        }
        if !advance {
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

    /// `ImperialProbe_FireBlaster` (`254-310`): a bolt from its `*flash` at its enemy's chest,
    /// a little off.
    fn probe_fire_blaster(&mut self, me: usize) {
        let muzzle = self.machine_muzzle(me, "*flash");
        self.play_effect_at(b"bryar/muzzle_flash", muzzle, [0.0; 3]);
        let number = self.actors[me].number;
        self.creature_sound(
            number,
            crate::npc_creature::CHAN_AUTO,
            b"sound/chars/probe/misc/fire",
        );
        let enemy = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy));
        let forward = match enemy.filter(|_| self.actors[me].health != 0) {
            Some(enemy) => {
                let mut target = spot(&enemy, Spot::Chest);
                target[0] += self.host.irand(0, 10) as f32;
                target[1] += self.host.irand(0, 10) as f32;
                let angles = crate::player_angle_math::vector_angles(crate::npc_senses::subtract(
                    target, muzzle,
                ));
                Self::forward_of(angles)
            }
            None => Self::forward_of(self.actors[me].mind.current_angles),
        };
        let damage = if self.host.skill() <= 1 { 5 } else { 10 };
        self.machine_missile(
            me,
            muzzle,
            forward,
            MachineBolt {
                weapon: WP_BRYAR_PISTOL,
                damage,
                means: MOD_UNKNOWN,
                speed: 1_600.0,
            },
            number,
        );
    }

    /// `ImperialProbe_Ranged(visible, advance)` (`317-353`): a shot now and then, sooner on
    /// harder skills; and the hunt, when it chases.
    fn probe_ranged(&mut self, me: usize, visible: bool, advance: bool, command: &mut UserCommand) {
        let level_time = self.level_time;
        if self.actors[me].mind.timers.done("attackDelay", level_time) {
            let (least, most) = match self.host.skill() {
                0 => (500, 3_000),
                1 => (300, 1_500),
                _ => (500, 2_000),
            };
            let delay = self.host.irand(least, most);
            self.actors[me]
                .mind
                .timers
                .set("attackDelay", level_time, delay);
            self.probe_fire_blaster(me);
        }
        if self.actors[me].script_flags & SCF_CHASE_ENEMIES != 0 {
            self.probe_hunt(me, visible, advance, command);
        }
    }

    /// `ImperialProbe_AttackDecision` (`364-405`).
    fn probe_attack_decision(&mut self, me: usize, command: &mut UserCommand) {
        self.probe_maintain_height(me, command);
        let level_time = self.level_time;
        let timers = &self.actors[me].mind.timers;
        if timers.done("patrolNoise", level_time) && timers.done("angerNoise", level_time) {
            self.probe_talk(me, 4_000, 10_000);
        }
        if !self.check_enemy_ext(me) {
            self.probe_idle(me, command);
            return;
        }
        self.set_animation(me, SETANIM_BOTH, BOTH_RUN1, NORMAL);
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
            self.probe_hunt(me, visible, advance, command);
            return;
        }
        self.face_enemy(me, true, command);
        self.probe_ranged(me, visible, advance, command);
    }

    /// `probetalk1..3`, and the next chatter `least` to `most` on.
    fn probe_talk(&mut self, me: usize, least: i32, most: i32) {
        let talk = self.host.irand(1, 3);
        self.sound_on_entity(
            me,
            format!("sound/chars/probe/misc/probetalk{talk}").as_bytes(),
        );
        let delay = self.host.irand(least, most);
        let level_time = self.level_time;
        self.actors[me]
            .mind
            .timers
            .set("patrolNoise", level_time, delay);
    }

    /// `NPC_Probe_Pain` (`412-486`).
    pub(crate) fn probe_pain(&mut self, me: usize, attacker: Option<u16>, damage: i32, means: u32) {
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        let path = npc.mind.tactics.last_path_angles;
        for (field, value) in es::ANGLES.into_iter().zip(path) {
            npc.state.set_raw_field(field, value.to_bits());
        }
        let demp2 = means == MOD_DEMP2 || means == MOD_DEMP2_ALT;
        if npc.health < 30 || demp2 {
            let (origin, number) = (npc.current_origin, npc.number);
            let end = [origin[0], origin[1], origin[2] - 128.0];
            let trace = self.trace_bodies(origin, [0.0; 3], [0.0; 3], end, number, MASK_SOLID);
            if trace.fraction == 1.0 || means == MOD_DEMP2 {
                // Nobody to blame is the world (`G_Damage`'s attacker), at the origin.
                let other = match attacker {
                    Some(other) => self.body(other).map(|body| body.origin),
                    None => Some([0.0; 3]),
                };
                if let Some(other) = other.filter(|_| demp2) {
                    self.set_animation(
                        me,
                        SETANIM_BOTH,
                        BOTH_PAIN1,
                        SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                    );
                    let npc = &mut self.actors[me];
                    let (away, _) =
                        normalized(crate::npc_senses::subtract(npc.current_origin, other));
                    let velocity = npc.player.velocity();
                    let mut velocity: [f32; 3] =
                        std::array::from_fn(|axis| velocity[axis] + 550.0 * away[axis]);
                    velocity[2] -= 127.0;
                    npc.player.set_velocity(velocity);
                }
                let npc = &mut self.actors[me];
                npc.player
                    .set_raw_field(PS_ELECTRIFY_TIME, (level_time + 3_000) as u32);
                npc.mind.fight.local_state = LSTATE_DROP;
            }
        } else {
            let chance = self.pain_chance(me, damage);
            if self.host.rng().flrand(0.0, 1.0) < chance {
                self.set_animation(me, SETANIM_BOTH, BOTH_PAIN1, SETANIM_FLAG_OVERRIDE);
            }
        }
        self.npc_pain(me, attacker, damage, means);
    }

    /// `ImperialProbe_Idle` (`493-498`).
    fn probe_idle(&mut self, me: usize, command: &mut UserCommand) {
        self.probe_maintain_height(me, command);
        self.machine_idle(me, command);
    }

    /// `ImperialProbe_Patrol` (`505-546`): an enemy of its team noticed is faced; else to its
    /// goal humming, chattering; with an enemy, an angry word.
    fn probe_patrol(&mut self, me: usize, command: &mut UserCommand) {
        self.probe_maintain_height(me, command);
        if self.check_player_team_stealth(me) {
            self.update_angles(me, true, true, command);
            return;
        }
        let level_time = self.level_time;
        if self.actors[me].mind.enemy.is_none() {
            self.set_animation(me, SETANIM_BOTH, BOTH_RUN1, NORMAL);
            if self.update_goal(me, command).is_some() {
                self.machine_loop_sound(me, b"sound/chars/probe/misc/probedroidloop");
                command.buttons |= BUTTON_WALKING;
                self.move_to_goal(me, true, command);
            }
            if self.actors[me].mind.timers.done("patrolNoise", level_time) {
                self.probe_talk(me, 2_000, 4_000);
            }
        } else {
            self.sound_on_entity(me, b"sound/chars/probe/misc/anger1");
            let delay = self.host.irand(2_000, 4_000);
            self.actors[me]
                .mind
                .timers
                .set("angerNoise", level_time, delay);
        }
        self.update_angles(me, true, true, command);
    }

    /// `ImperialProbe_Wait` (`553-574`): dropping, it spins, and blows where it meets the
    /// floor — its enemy to blame.
    fn probe_wait(&mut self, me: usize, command: &mut UserCommand) {
        if self.actors[me].mind.fight.local_state == LSTATE_DROP {
            let npc = &mut self.actors[me];
            npc.desired_yaw = angle_normalize360(npc.desired_yaw + 25.0);
            let (origin, number, enemy) = (npc.current_origin, npc.number, npc.mind.enemy);
            let end = [origin[0], origin[1], origin[2] - 32.0];
            let trace = self.trace_bodies(origin, [0.0; 3], [0.0; 3], end, number, MASK_SOLID);
            if trace.fraction != 1.0 {
                self.machine_self_damage(me, enemy, None, 2_000, 0, MOD_UNKNOWN);
            }
        }
        self.update_angles(me, true, true, command);
    }
}
