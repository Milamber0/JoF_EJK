//! Following a leader (`NPC_BSFollowLeader`, `NPC_behavior.c:542-745`): the leader a
//! script or a Force mind trick gave (`client->leader`), kept within its follow distance —
//! closed on, backed away from — while the follower takes on the enemies it sees, hears or
//! its leader fights. Without a leader it stands guard
//! ([`NpcWorld::bs_stand_guard`](crate::npc_world::NpcWorld::bs_stand_guard)).

use crate::npc_behavior::bstate;
use crate::npc_senses::{
    AEL_MINOR, CHECK_360, CHECK_FOV, CHECK_PVS, CHECK_SHOOT, Spot, Visibility, spot, subtract,
};
use crate::npc_spawn::NpcHost;
use crate::npc_states::BUTTON_WALKING;
use crate::npc_states_fight::length;
use crate::npc_world::NpcWorld;
use crate::player_angle_math::vector_angles;
use sjk_protocol::UserCommand;

/// `SCF_IGNORE_ALERTS`, `SCF_LOOK_FOR_ENEMIES`.
const SCF_IGNORE_ALERTS: u32 = 0x2000;
const SCF_LOOK_FOR_ENEMIES: u32 = 0x800;
/// `AEL_SUSPICIOUS`.
const AEL_SUSPICIOUS: u32 = 2;
/// `FL_NOTARGET`.
const FL_NOTARGET: u32 = 0x20;
/// `WP_SABER`.
const WP_SABER: u8 = 3;
/// The full-body attacks a follower does not move out of (`BOTH_ATTACK1..3`,
/// `BOTH_MELEE1`, `BOTH_MELEE2`).
const FULL_BODY_ATTACKS: [u16; 5] = [113, 114, 115, 122, 123];
/// `ps.legsAnim`.
const PS_LEGS_ANIM: usize = 13;

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSFollowLeader` (`NPC_behavior.c:542-745`).
    pub fn bs_follow_leader(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        let Some(leader) = self.actors[me]
            .mind
            .leader
            .and_then(|number| self.body(number))
        else {
            // "ok, stand guard until we find an enemy".
            let npc = &mut self.actors[me];
            if npc.mind.temp_behavior == bstate::HUNT_AND_KILL {
                npc.mind.temp_behavior = bstate::DEFAULT;
            } else {
                npc.mind.temp_behavior = bstate::STAND_GUARD;
                self.bs_stand_guard(me, command);
            }
            return;
        };
        match self.actors[me]
            .mind
            .enemy
            .and_then(|number| self.body(number))
        {
            None => self.follower_finds_enemy(me, &leader),
            Some(enemy) if enemy.health <= 0 || enemy.flags & FL_NOTARGET != 0 => {
                self.clear_enemy(me);
                if self.actors[me].mind.tactics.enemy_check_debounce_time > level_time + 1_000 {
                    let time = self.host.irand(1_000, 2_000);
                    self.actors[me].mind.tactics.enemy_check_debounce_time = level_time + time;
                }
            }
            Some(_) => {
                let npc = &self.actors[me];
                if npc.player.weapon() != 0
                    && npc.mind.tactics.enemy_check_debounce_time < level_time
                {
                    let find_new = npc.mind.confusion_time < level_time
                        || npc.mind.temp_behavior != bstate::FOLLOW_LEADER;
                    self.check_enemy(me, find_new, false, true);
                }
            }
        }
        let weapon = self.actors[me].player.weapon();
        if let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|number| self.body(number))
            && weapon != 0
        {
            if weapon == WP_SABER && self.actors[me].mind.temp_behavior != bstate::FOLLOW_LEADER {
                // "go after the guy".
                self.actors[me].mind.temp_behavior = bstate::HUNT_AND_KILL;
                self.update_angles(me, true, true, command);
                return;
            }
            self.follower_fires(me, &enemy, command);
        } else {
            let (their_head, my_head) =
                (spot(&leader, Spot::Head), spot(&self.npc(me), Spot::Head));
            let angles = vector_angles(subtract(their_head, my_head));
            let npc = &mut self.actors[me];
            npc.desired_yaw = angles[1];
            npc.mind.desired_pitch = angles[0];
            self.update_angles(me, true, true, command);
        }
        let seen = self.visibility_with_shot(me, &leader, CHECK_PVS | CHECK_360 | CHECK_SHOOT);
        let legs = self.actors[me].player.raw_field(PS_LEGS_ANIM).unwrap_or(0) as u16;
        if !FULL_BODY_ATTACKS.contains(&legs) {
            self.keep_to_leader(me, &leader, seen, command);
        }
    }

    /// `NPC_BSFollowLeader` without an enemy (`NPC_behavior.c:563-610`): one it sees, one
    /// an alert tells of, or its leader's.
    fn follower_finds_enemy(&mut self, me: usize, leader: &crate::npc_senses::Body) {
        let level_time = self.level_time;
        let find_new = self.actors[me].mind.confusion_time < level_time;
        self.check_enemy(me, find_new, false, true);
        if self.actors[me].mind.enemy.is_some() {
            let time = self.host.irand(3_000, 10_000);
            self.actors[me].mind.tactics.enemy_check_debounce_time = level_time + time;
        } else if self.actors[me].script_flags & SCF_IGNORE_ALERTS == 0 {
            self.follower_hears(me);
        }
        if self.actors[me].mind.enemy.is_some() {
            return;
        }
        let Some(theirs) = leader.enemy.and_then(|number| self.body(number)) else {
            return;
        };
        let npc = &self.actors[me];
        if theirs.number != npc.number && theirs.player_team == npc.enemy_team && theirs.health > 0
        {
            self.set_enemy(me, theirs.number);
            let time = self.host.irand(3_000, 10_000);
            let tactics = &mut self.actors[me].mind.tactics;
            tactics.enemy_check_debounce_time = level_time + time;
            tactics.enemy_last_seen_time = level_time;
        }
    }

    /// The follower's alerts (`NPC_behavior.c:572-593`): a suspicious one's owner taken as
    /// the enemy when it is a living client of the NPC's enemy team. With no alert the
    /// reference reads the slot before the first (`level.alertEvents[-1]`): in its build
    /// that is past the body queue, a pointer's low half for the level (suspicious enough)
    /// and a body for the owner — the alert's ID taken is `level.portalSequence`, 0.
    fn follower_hears(&mut self, me: usize) {
        let level_time = self.level_time;
        let at = self.check_alerts(me, -1, false, AEL_MINOR);
        if self.actors[me].script_flags & SCF_LOOK_FOR_ENEMIES == 0 {
            return;
        }
        let Some(at) = at else {
            self.actors[me].mind.last_alert_id = 0;
            return;
        };
        let alert = self.alerts.events()[at];
        if alert.level < AEL_SUSPICIOUS {
            return;
        }
        self.actors[me].mind.last_alert_id = alert.id;
        let Some(owner) = alert.owner.and_then(|owner| self.body(owner)) else {
            return;
        };
        if owner.health <= 0 || owner.player_team != self.actors[me].enemy_team {
            return;
        }
        self.set_enemy(me, owner.number);
        let debounce = self.host.irand(3_000, 10_000);
        let tactics = &mut self.actors[me].mind.tactics;
        tactics.enemy_check_debounce_time = level_time + debounce;
        tactics.enemy_last_seen_time = level_time;
        let delay = self.host.irand(500, 1_000);
        self.actors[me]
            .mind
            .timers
            .set("attackDelay", level_time, delay);
    }

    /// The follower's fight (`NPC_behavior.c:640-676`): an enemy it sees turned toward (its
    /// wobbling head, the firing angles) and — a clear shot in its front cone — fired at;
    /// the aim bettered while it can shoot, worsened while it cannot see.
    fn follower_fires(
        &mut self,
        me: usize,
        enemy: &crate::npc_senses::Body,
        command: &mut UserCommand,
    ) {
        let seen = self.visibility_with_shot(me, enemy, CHECK_FOV | CHECK_SHOOT);
        if seen <= Visibility::Pvs {
            self.aim_adjust(me, -1);
            return;
        }
        let enemy_org = self.aim_wiggle(me, enemy, spot(enemy, Spot::Head));
        let muzzle = self.weapon_spot(me);
        let angles = vector_angles(subtract(enemy_org, muzzle));
        let npc = &mut self.actors[me];
        npc.desired_yaw = angles[1];
        npc.mind.desired_pitch = angles[0];
        self.update_firing_angles(me, true, true, command);
        if seen < Visibility::Shoot {
            self.aim_adjust(me, 1);
            return;
        }
        self.aim_adjust(me, 2);
        let npc = &self.actors[me];
        let (origin, facing, stats) = (
            npc.current_origin,
            npc.player.view_angles()[1],
            npc.definition.stats,
        );
        // Both `NPC_GetHFOVPercentage`: the second against the vertical field.
        if crate::npc_st::fov_percentage(enemy.origin, origin, facing, stats.hfov as f32, 1) > 0.6
            && crate::npc_st::fov_percentage(enemy.origin, origin, facing, stats.vfov as f32, 1)
                > 0.5
        {
            self.weapon_think(me, command);
        }
    }

    /// The follow (`NPC_behavior.c:697-744`): beyond half its follow distance and out of a
    /// clear shot or beyond five sixths of it, it closes (walking when it sees its leader
    /// within four thirds); within half it backs away; a move is then checked for walls and
    /// ledges (`NPC_MoveDirClear`).
    fn keep_to_leader(
        &mut self,
        me: usize,
        leader: &crate::npc_senses::Body,
        seen: Visibility,
        command: &mut UserCommand,
    ) {
        let npc = &self.actors[me];
        let follow = if npc.mind.states.follow_dist != 0.0 {
            npc.mind.states.follow_dist
        } else {
            96.0
        };
        let backup = follow / 2.0;
        let walk = (f64::from(follow) * 0.83) as f32;
        let run = (f64::from(follow) * 1.33) as f32;
        let mut to_leader = subtract(leader.origin, npc.current_origin);
        let distance = length(to_leader);
        to_leader[2] = 0.0;
        let flat = length(to_leader);
        if flat > backup && (seen != Visibility::Shoot || distance > walk) {
            self.actors[me].mind.goal = Some(leader.number);
            self.slide_move_to_goal(me, command);
            if seen == Visibility::Shoot && distance < run {
                command.buttons |= BUTTON_WALKING;
            }
        } else if distance < backup {
            self.actors[me].mind.goal = Some(leader.number);
            self.slide_move_to_goal(me, command);
            command.forward_move = command.forward_move.wrapping_neg();
            command.right_move = command.right_move.wrapping_neg();
            let dir = &mut self.actors[me].mind.move_dir;
            *dir = dir.map(|value| value * -1.0);
        }
        if command.forward_move != 0
            || command.right_move != 0
            || self.actors[me].mind.move_dir == [0.0; 3]
        {
            let (forward, right) = (
                i32::from(command.forward_move),
                i32::from(command.right_move),
            );
            self.npc_move_dir_clear(me, forward, right, true, command);
        }
    }

    /// `NPC_SlideMoveToGoal` (`NPC_move.c:496-508`): a combat move to the goal, its yaw
    /// kept as the view's.
    pub(crate) fn slide_move_to_goal(&mut self, me: usize, command: &mut UserCommand) -> bool {
        let yaw = self.actors[me].player.view_angles()[1];
        self.actors[me].mind.combat_move = true;
        let moved = self.move_to_goal(me, true, command);
        self.actors[me].desired_yaw = yaw;
        moved
    }
}
