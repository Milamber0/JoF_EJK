//! The remote droid's AI (`codemp/game/NPC_AI_Remote.c`), which `NPC_BehaviorSet_Remote`
//! (`NPC.c:1031-1034`) runs: its behaviour (`NPC_BSRemote_Default`, `395-403`), its pain
//! (`NPC_Remote_Pain`, `46-54`: a strafe, then the pain), its hover (`Remote_MaintainHeight`,
//! `61-151`), its strafe (`162-190`), its hunt, closing in or backing off (`199-244`), its shot
//! (`Remote_Fire`, `252-280`), its fire (`Remote_Ranged`, `287-301`), its fight
//! (`Remote_Attack`, `314-352`), its idle (`359-364`) and its patrol (`371-388`).
//!
//! The retail remote (`remote_sp`) is an MD3 model, which multiplayer refuses to spawn
//! (`NPC_stats.c:3540`): a remote comes from an NPC file that gives the class a Ghoul2 model.

use crate::means_of_death::MOD_BRYAR_PISTOL;
use crate::npc_creature::CHAN_AUTO;
use crate::npc_jedi_patrol::distance_horizontal_squared;
use crate::npc_machine::{
    BUTTON_WALKING, MachineBolt, SCF_CHASE_ENEMIES, SCF_LOOK_FOR_ENEMIES, WP_BRYAR_PISTOL, damped,
};
use crate::npc_senses::{Spot, spot};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use sjk_protocol::UserCommand;

/// `VELOCITY_DECAY`, `REMOTE_STRAFE_VEL`, `REMOTE_STRAFE_DIS`, `REMOTE_UPWARD_PUSH`,
/// `REMOTE_FORWARD_BASE_SPEED`, `REMOTE_FORWARD_MULTIPLIER`, `MIN_DISTANCE_SQR`.
const VELOCITY_DECAY: f32 = 0.85;
const STRAFE_VEL: i32 = 256;
const STRAFE_DIS: i32 = 200;
const UPWARD_PUSH: f32 = 32.0;
const FORWARD_BASE_SPEED: i32 = 10;
const FORWARD_MULTIPLIER: i32 = 5;
const MIN_DISTANCE_SQR: f32 = 80.0 * 80.0;
/// Its hiss.
const HISS: &[u8] = b"sound/chars/remote/misc/hiss.wav";

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSRemote_Default` (`395-403`).
    pub fn bs_remote_default(&mut self, me: usize, command: &mut UserCommand) {
        if self.actors[me].mind.enemy.is_some() {
            self.remote_attack(me, command);
        } else if self.actors[me].script_flags & SCF_LOOK_FOR_ENEMIES != 0 {
            self.remote_patrol(me, command);
        } else {
            self.remote_idle(me, command);
        }
    }

    /// `NPC_Remote_Pain` (`46-54`).
    pub(crate) fn remote_pain(
        &mut self,
        me: usize,
        attacker: Option<u16>,
        damage: i32,
        means: u32,
    ) {
        self.remote_strafe(me);
        self.npc_pain(me, attacker, damage, means);
    }

    /// `Remote_Strafe` (`162-190`).
    fn remote_strafe(&mut self, me: usize) {
        self.machine_strafe(me, STRAFE_DIS, STRAFE_VEL, UPWARD_PUSH, Some(HISS));
    }

    /// `Remote_MaintainHeight` (`61-151`): its angles, its climb damped; now and then — with a
    /// hiss — a hover to somewhere up its enemy's height; or its goal's height; its drift
    /// damped.
    fn remote_maintain_height(&mut self, me: usize, command: &mut UserCommand) {
        self.update_angles(me, true, true, command);
        let level_time = self.level_time;
        let origin = self.actors[me].current_origin;
        let mut velocity = self.actors[me].player.velocity();
        velocity[2] = damped(velocity[2], VELOCITY_DECAY, 2.0);
        if let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        {
            if self.actors[me].mind.timers.done("heightChange", level_time) {
                let delay = self.host.irand(1_000, 3_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("heightChange", level_time, delay);
                // `Q_irand(0, maxs[2] + 8)`: the float truncated for the int argument.
                let above = self.host.irand(0, (enemy.maxs[2] + 8.0) as i32);
                let mut dif = (enemy.origin[2] + above as f32) - origin[2];
                if dif.abs() > 2.0 {
                    if dif.abs() > 24.0 {
                        dif = if dif < 0.0 { -24.0 } else { 24.0 };
                    }
                    dif *= 10.0;
                    velocity[2] = (velocity[2] + dif) / 2.0;
                    let number = self.actors[me].number;
                    self.creature_sound(number, CHAN_AUTO, HISS);
                }
            }
        } else if let Some(goal) = self.goal_origin(me) {
            // No `lastGoalEntity` is kept: the goal alone.
            let dif = goal[2] - origin[2];
            if dif.abs() > 24.0 {
                let dif = if dif < 0.0 { -24.0 } else { 24.0 };
                velocity[2] = (velocity[2] + dif) / 2.0;
            }
        }
        velocity[0] = damped(velocity[0], VELOCITY_DECAY, 1.0);
        velocity[1] = damped(velocity[1], VELOCITY_DECAY, 1.0);
        self.actors[me].player.set_velocity(velocity);
    }

    /// `Remote_Hunt(visible, advance, retreat)` (`199-244`): a strafe while it may; else in
    /// toward its enemy, or back from it.
    fn remote_hunt(
        &mut self,
        me: usize,
        visible: bool,
        advance: bool,
        retreat: bool,
        command: &mut UserCommand,
    ) {
        if self.actors[me].mind.stand_time < self.level_time && visible {
            self.remote_strafe(me);
            return;
        }
        if !advance && visible {
            return;
        }
        if !visible {
            if let Some(forward) = self.machine_seek_unseen(me, 12, command) {
                let mut speed =
                    (FORWARD_BASE_SPEED + FORWARD_MULTIPLIER * self.host.skill()) as f32;
                if retreat {
                    speed *= -1.0;
                }
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
        let mut speed = (FORWARD_BASE_SPEED + FORWARD_MULTIPLIER * self.host.skill()) as f32;
        if retreat {
            speed *= -1.0;
        }
        self.machine_advance(me, enemy.origin, speed);
    }

    /// `Remote_Fire` (`252-280`): a bolt at its enemy's head from where it is — which
    /// `CreateMissile` snaps (`SnapVector` on the remote's own `r.currentOrigin`).
    fn remote_fire(&mut self, me: usize) {
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        else {
            return;
        };
        let origin = self.actors[me].current_origin;
        let angles = crate::player_angle_math::vector_angles(crate::npc_senses::subtract(
            spot(&enemy, Spot::Head),
            origin,
        ));
        let forward = Self::forward_of(angles);
        let number = self.actors[me].number;
        let snapped = self.machine_missile(
            me,
            origin,
            forward,
            MachineBolt {
                weapon: WP_BRYAR_PISTOL,
                damage: 10,
                means: MOD_BRYAR_PISTOL,
                speed: 1_000.0,
            },
            number,
        );
        self.actors[me].current_origin = snapped;
        self.play_effect_at(b"bryar/muzzle_flash", snapped, forward);
    }

    /// `Remote_Ranged(visible, advance, retreat)` (`287-301`).
    fn remote_ranged(
        &mut self,
        me: usize,
        visible: bool,
        advance: bool,
        retreat: bool,
        command: &mut UserCommand,
    ) {
        let level_time = self.level_time;
        if self.actors[me].mind.timers.done("attackDelay", level_time) {
            let delay = self.host.irand(500, 3_000);
            self.actors[me]
                .mind
                .timers
                .set("attackDelay", level_time, delay);
            self.remote_fire(me);
        }
        if self.actors[me].script_flags & SCF_CHASE_ENEMIES != 0 {
            self.remote_hunt(me, visible, advance, retreat, command);
        }
    }

    /// `Remote_Attack` (`314-352`): a spin now and then; somewhere between 80 and 113 units
    /// from its enemy, give or take.
    fn remote_attack(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        if self.actors[me].mind.timers.done("spin", level_time) {
            let delay = self.host.irand(250, 1_500);
            self.actors[me].mind.timers.set("spin", level_time, delay);
            let turn = self.host.irand(-200, 200);
            self.actors[me].desired_yaw += turn as f32;
        }
        self.remote_maintain_height(me, command);
        if !self.check_enemy_ext(me) {
            self.remote_idle(me, command);
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
        let ideal = MIN_DISTANCE_SQR + MIN_DISTANCE_SQR * self.host.rng().flrand(0.0, 1.0);
        let advance = f64::from(distance) > f64::from(ideal) * 1.25;
        let retreat = f64::from(distance) < f64::from(ideal) * 0.75;
        if !visible && self.actors[me].script_flags & SCF_CHASE_ENEMIES != 0 {
            self.remote_hunt(me, visible, advance, retreat, command);
            return;
        }
        self.remote_ranged(me, visible, advance, retreat, command);
    }

    /// `Remote_Idle` (`359-364`).
    fn remote_idle(&mut self, me: usize, command: &mut UserCommand) {
        self.remote_maintain_height(me, command);
        self.machine_idle(me, command);
    }

    /// `Remote_Patrol` (`371-388`).
    fn remote_patrol(&mut self, me: usize, command: &mut UserCommand) {
        self.remote_maintain_height(me, command);
        if self.actors[me].mind.enemy.is_none() && self.update_goal(me, command).is_some() {
            command.buttons |= BUTTON_WALKING;
            self.move_to_goal(me, true, command);
        }
        self.update_angles(me, true, true, command);
    }
}
