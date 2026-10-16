//! The interrogator droid's AI (`codemp/game/NPC_AI_Interrogator.c`): its behaviour
//! (`NPC_BSInterrogator_Default`, `476-489`), its hover (`Interrogator_MaintainHeight`,
//! `159-251`), its parts (`Interrogator_PartsMove`, `86-149`), its strafe and hunt
//! (`260-358`), its injection (`Interrogator_Melee`, `367-396`), its fight
//! (`Interrogator_Attack`, `403-450`) and its idle (`457-469`). Its pain is the droids'
//! (`NPC_Droid_Pain`, [`crate::npc_droid`]); its death function (`Interrogator_die`) is
//! never installed in multiplayer (`NPC_AI_Interrogator.c:478`, commented out).

use crate::npc_creature::{CHAN_AUTO, DAMAGE_NO_KNOCKBACK, MOD_MELEE};
use crate::npc_droid::angle_normalize360;
use crate::npc_jedi_patrol::distance_horizontal_squared;
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use sjk_protocol::UserCommand;

/// `LSTATE_BLADEUP`, `LSTATE_BLADEDOWN` (the scalpel's way).
const LSTATE_BLADEUP: i32 = 1;
const LSTATE_BLADEDOWN: i32 = 2;
/// `VELOCITY_DECAY`, `HUNTER_UPWARD_PUSH`, `HUNTER_STRAFE_VEL`, `HUNTER_STRAFE_DIS`,
/// `HUNTER_FORWARD_BASE_SPEED`, `HUNTER_FORWARD_MULTIPLIER`, `MIN_DISTANCE`.
const VELOCITY_DECAY: f32 = 0.85;
const HUNTER_UPWARD_PUSH: f32 = 2.0;
const HUNTER_STRAFE_VEL: i32 = 32;
const HUNTER_STRAFE_DIS: i32 = 200;
const HUNTER_FORWARD_BASE_SPEED: i32 = 10;
const HUNTER_FORWARD_MULTIPLIER: i32 = 2;
const MIN_DISTANCE: i32 = 64;
/// `SCF_CHASE_ENEMIES`; `BUTTON_WALKING`; `MASK_SOLID`.
const SCF_CHASE_ENEMIES: u32 = 0x400;
const BUTTON_WALKING: u16 = 16;
const MASK_SOLID: u32 = 0x1 | 0x1000;
/// `s.loopSound`.
const ES_LOOP_SOUND: usize = 55;

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSInterrogator_Default` (`NPC_AI_Interrogator.c:476-489`).
    pub fn bs_interrogator_default(&mut self, me: usize, command: &mut UserCommand) {
        if self.actors[me].mind.enemy.is_some() {
            self.interrogator_attack(me, command);
        } else {
            self.interrogator_idle(me, command);
        }
    }

    /// `Interrogator_PartsMove` (`86-149`): the syringe swung now and then, the scalpel up
    /// and down, the claw turning — each on the entity for the clients.
    fn interrogator_parts_move(&mut self, me: usize) {
        let level_time = self.level_time;
        if self.actors[me].mind.timers.done("syringeDelay", level_time) {
            let mut syringe = self.actors[me].mind.creature.parts[0];
            syringe[1] = angle_normalize360(syringe[1]);
            if syringe[1] < 60.0 || syringe[1] > 300.0 {
                syringe[1] += self.host.irand(-20, 20) as f32;
            } else if syringe[1] > 180.0 {
                syringe[1] = self.host.irand(300, 360) as f32;
            } else {
                syringe[1] = self.host.irand(0, 60) as f32;
            }
            self.actors[me].mind.creature.parts[0] = syringe;
            self.npc_set_bone_angles(me, b"left_arm", syringe);
            let delay = self.host.irand(100, 1_000);
            self.actors[me]
                .mind
                .timers
                .set("syringeDelay", level_time, delay);
        }
        if self.actors[me].mind.timers.done("scalpelDelay", level_time) {
            let mut scalpel = self.actors[me].mind.creature.parts[1];
            if self.actors[me].mind.fight.local_state == LSTATE_BLADEDOWN {
                scalpel[0] -= 30.0;
                if scalpel[0] < 180.0 {
                    scalpel[0] = 180.0;
                    self.actors[me].mind.fight.local_state = LSTATE_BLADEUP;
                }
            } else {
                scalpel[0] += 30.0;
                if scalpel[0] >= 360.0 {
                    scalpel[0] = 360.0;
                    self.actors[me].mind.fight.local_state = LSTATE_BLADEDOWN;
                    let delay = self.host.irand(100, 1_000);
                    self.actors[me]
                        .mind
                        .timers
                        .set("scalpelDelay", level_time, delay);
                }
            }
            scalpel[0] = angle_normalize360(scalpel[0]);
            self.actors[me].mind.creature.parts[1] = scalpel;
            self.npc_set_bone_angles(me, b"right_arm", scalpel);
        }
        let mut claw = self.actors[me].mind.creature.parts[2];
        claw[1] += self.host.irand(10, 30) as f32;
        claw[1] = angle_normalize360(claw[1]);
        self.actors[me].mind.creature.parts[2] = claw;
        self.npc_set_bone_angles(me, b"claw", claw);
    }

    /// `Interrogator_MaintainHeight` (`159-251`): its hum, its angles, and a hover at its
    /// enemy's head height (or its goal's), its drift damped.
    fn interrogator_maintain_height(&mut self, me: usize, command: &mut UserCommand) {
        let sound = self
            .host
            .sound_index(b"sound/chars/interrogator/misc/torture_droid_lp");
        self.actors[me]
            .state
            .set_raw_field(ES_LOOP_SOUND, u32::from(sound));
        self.update_angles(me, true, true, command);
        let origin = self.actors[me].current_origin;
        let mut velocity = self.actors[me].player.velocity();
        if let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        {
            let mut dif = (enemy.origin[2] + enemy.maxs[2]) - origin[2];
            if dif.abs() > 2.0 {
                if dif.abs() > 16.0 {
                    dif = if dif < 0.0 { -16.0 } else { 16.0 };
                }
                velocity[2] = (velocity[2] + dif) / 2.0;
            }
        } else if let Some(goal) = self.goal_origin(me) {
            // No `lastGoalEntity` is kept: the goal alone.
            let dif = goal[2] - origin[2];
            if dif.abs() > 24.0 {
                command.up_move = if command.up_move < 0 { -4 } else { 4 };
            } else if velocity[2] != 0.0 {
                velocity[2] *= VELOCITY_DECAY;
                if velocity[2].abs() < 2.0 {
                    velocity[2] = 0.0;
                }
            }
        } else if velocity[2] != 0.0 {
            velocity[2] *= VELOCITY_DECAY;
            if velocity[2].abs() < 1.0 {
                velocity[2] = 0.0;
            }
        }
        for axis in 0..2 {
            if velocity[axis] != 0.0 {
                velocity[axis] *= VELOCITY_DECAY;
                if velocity[axis].abs() < 1.0 {
                    velocity[axis] = 0.0;
                }
            }
        }
        self.actors[me].player.set_velocity(velocity);
    }

    /// `Interrogator_Strafe` (`260-301`): a side drawn (the C library's `rand()`), and a
    /// strafe that way where the world leaves room, a little toward the enemy's height, held
    /// three seconds and more.
    fn interrogator_strafe(&mut self, me: usize) {
        let npc = &self.actors[me];
        let right = crate::pmove::flight::flight_axes(npc.mind.eye_angles)
            .1
            .to_array();
        let drawn = self.level.crt.next();
        let side = if drawn & 1 != 0 { -1 } else { 1 };
        let origin = npc.current_origin;
        let reach = (HUNTER_STRAFE_DIS * side) as f32;
        let end: [f32; 3] = std::array::from_fn(|axis| origin[axis] + reach * right[axis]);
        let number = npc.number;
        let trace = self
            .host
            .trace(origin, [0.0; 3], [0.0; 3], end, number, MASK_SOLID, &[]);
        if trace.fraction <= 0.9 {
            return;
        }
        let push = (HUNTER_STRAFE_VEL * side) as f32;
        let enemy = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy));
        let npc = &mut self.actors[me];
        let velocity = npc.player.velocity();
        let mut velocity: [f32; 3] =
            std::array::from_fn(|axis| velocity[axis] + push * right[axis]);
        if let Some(enemy) = enemy {
            let mut dif = (enemy.origin[2] + 32.0) - origin[2];
            if dif.abs() > 8.0 {
                dif = if dif < 0.0 {
                    -HUNTER_UPWARD_PUSH
                } else {
                    HUNTER_UPWARD_PUSH
                };
            }
            velocity[2] += dif;
        }
        npc.player.set_velocity(velocity);
        let level_time = self.level_time;
        let stand = ((level_time + 3_000) as f32 + self.host.rng().flrand(0.0, 1.0) * 500.0) as i32;
        self.actors[me].mind.stand_time = stand;
    }

    /// `Interrogator_Hunt(visible, advance)` (`312-358`): its parts, a look at its enemy, a
    /// strafe while it may, and — advancing — a push toward its enemy.
    fn interrogator_hunt(
        &mut self,
        me: usize,
        visible: bool,
        advance: bool,
        command: &mut UserCommand,
    ) {
        self.interrogator_parts_move(me);
        self.face_enemy(me, false, command);
        let level_time = self.level_time;
        if self.actors[me].mind.stand_time < level_time && visible {
            self.interrogator_strafe(me);
            if self.actors[me].mind.stand_time > level_time {
                // "successfully strafed"
                return;
            }
        }
        if !advance {
            return;
        }
        let Some(enemy) = self.actors[me].mind.enemy else {
            return;
        };
        if !visible {
            if let Some(forward) = self.machine_seek_unseen(me, 12, command) {
                let speed = (HUNTER_FORWARD_BASE_SPEED
                    + HUNTER_FORWARD_MULTIPLIER * self.host.skill())
                    as f32;
                self.machine_push(me, forward, speed);
            }
            return;
        }
        let Some(target) = self.body(enemy) else {
            return;
        };
        let npc = &mut self.actors[me];
        let mut forward = crate::npc_senses::subtract(target.origin, npc.current_origin);
        crate::player_angle_math::normalize(&mut forward);
        let speed =
            (HUNTER_FORWARD_BASE_SPEED + HUNTER_FORWARD_MULTIPLIER * self.host.skill()) as f32;
        let velocity = npc.player.velocity();
        npc.player.set_velocity(std::array::from_fn(|axis| {
            velocity[axis] + speed * forward[axis]
        }));
    }

    /// `Interrogator_Melee(visible, advance)` (`367-396`): within its enemy's height, an
    /// injection now and then; and the hunt, when it chases.
    fn interrogator_melee(
        &mut self,
        me: usize,
        visible: bool,
        advance: bool,
        command: &mut UserCommand,
    ) {
        let level_time = self.level_time;
        if self.actors[me].mind.timers.done("attackDelay", level_time)
            && let Some(enemy) = self.actors[me]
                .mind
                .enemy
                .and_then(|enemy| self.body(enemy))
        {
            let npc = &self.actors[me];
            let z = npc.current_origin[2];
            if z >= enemy.origin[2] + enemy.mins[2]
                && z + npc.mins[2] + 8.0 < enemy.origin[2] + enemy.maxs[2]
            {
                let delay = self.host.irand(500, 3_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("attackDelay", level_time, delay);
                self.creature_damage(
                    me,
                    enemy.number,
                    None,
                    None,
                    2,
                    DAMAGE_NO_KNOCKBACK,
                    MOD_MELEE,
                );
                let number = self.actors[me].number;
                self.creature_sound(
                    number,
                    CHAN_AUTO,
                    b"sound/chars/interrogator/misc/torture_droid_inject.mp3",
                );
            }
        }
        if self.actors[me].script_flags & SCF_CHASE_ENEMIES != 0 {
            self.interrogator_hunt(me, visible, advance, command);
        }
    }

    /// `Interrogator_Attack` (`403-450`).
    fn interrogator_attack(&mut self, me: usize, command: &mut UserCommand) {
        self.interrogator_maintain_height(me, command);
        let level_time = self.level_time;
        let timers = &self.actors[me].mind.timers;
        if timers.done("patrolNoise", level_time) && timers.done("angerNoise", level_time) {
            // The `va` has no `%d`: the draw is made, the name is plain.
            let _ = self.host.irand(1, 3);
            self.sound_on_entity(me, b"sound/chars/probe/misc/talk.wav");
            let delay = self.host.irand(4_000, 10_000);
            self.actors[me]
                .mind
                .timers
                .set("patrolNoise", level_time, delay);
        }
        if !self.check_enemy_ext(me) {
            self.interrogator_idle(me, command);
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
        let advance = !visible || distance > (MIN_DISTANCE * MIN_DISTANCE) as f32;
        if self.actors[me].script_flags & SCF_CHASE_ENEMIES != 0 {
            self.interrogator_hunt(me, visible, advance, command);
        }
        self.face_enemy(me, true, command);
        if !advance {
            self.interrogator_melee(me, visible, advance, command);
        }
    }

    /// `Interrogator_Idle` (`457-469`): an enemy of its team noticed is growled at; else it
    /// hovers and idles (`NPC_BSIdle`, `NPC_AI_Default.c:171-188`).
    fn interrogator_idle(&mut self, me: usize, command: &mut UserCommand) {
        if self.check_player_team_stealth(me) {
            self.sound_on_entity(me, b"sound/chars/mark1/misc/anger.wav");
            self.update_angles(me, true, true, command);
            return;
        }
        self.interrogator_maintain_height(me, command);
        if self.update_goal(me, command).is_some() {
            self.move_to_goal(me, true, command);
        }
        self.update_angles(me, true, true, command);
        command.buttons |= BUTTON_WALKING;
    }
}
