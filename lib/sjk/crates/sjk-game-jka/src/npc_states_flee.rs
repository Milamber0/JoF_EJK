//! Fleeing (`NPC_BSFlee`, `NPC_Surrender`, `NPC_CheckSurrender`,
//! `NPC_behavior.c:1339-1579`): an NPC `NPC_StartFlee` sent running (a fright over, it
//! stops fleeing) takes a branch of its waypoint that does not lead toward the danger, runs
//! for it — or, unarmed and with nowhere to go, cowers (surrenders) — and looks for a weapon
//! lying about ([`crate::npc_weapon_pickup`]).
//!
//! `NPC_CheckSurrender` never surrenders in multiplayer (its only `return qtrue`s are
//! commented out) and reads nothing it changes: it is not run. No script sets a goal
//! (`NPC_SetGoal`), so `lastGoalEntity` is never set: the goal fled is the goal entity, else
//! the NPC's own ([`crate::npc_spawn::NpcActor::goal`]).

use crate::npc_behavior::bstate;
use crate::npc_mind::WAYPOINT_NONE;
use crate::npc_navigator::{NF_CLEAR_PATH, NavHolder};
use crate::npc_senses::subtract;
use crate::npc_spawn::NpcHost;
use crate::npc_states::BUTTON_WALKING;
use crate::npc_world::NpcWorld;
use sjk_protocol::UserCommand;

/// `SQUAD_IDLE`.
const SQUAD_IDLE: i32 = 0;
/// `EV_PUSHED1`, `EV_PUSHED3`.
const EV_PUSHED1: i32 = 125;
const EV_PUSHED3: i32 = 127;
/// `ps.weaponTime`, `ps.legsTimer`.
const PS_WEAPON_TIME: usize = 10;
const PS_LEGS_TIMER: usize = 21;

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSFlee` (`NPC_behavior.c:1466-1579`).
    pub fn bs_flee(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        if npc.mind.timers.done("flee", level_time) && npc.mind.temp_behavior == bstate::FLEE {
            npc.mind.temp_behavior = bstate::DEFAULT;
            npc.mind.tactics.squad_state = SQUAD_IDLE;
        }
        // `NPC_CheckSurrender`: never.
        let goal = npc.mind.goal.or(npc.goal);
        let mut reverse = true;
        if npc.mind.tactics.waypoint == WAYPOINT_NONE {
            let last = npc.mind.tactics.last_waypoint;
            let waypoint =
                self.nearest_node(NavHolder::Actor(me), last, NF_CLEAR_PATH, WAYPOINT_NONE);
            self.actors[me].mind.tactics.waypoint = waypoint;
        }
        let waypoint = self.actors[me].mind.tactics.waypoint;
        let edges = self.node_edge_count(waypoint);
        if waypoint != WAYPOINT_NONE && edges != WAYPOINT_NONE {
            let origin = self.actors[me].current_origin;
            let mut danger = subtract(self.actors[me].mind.tactics.investigate_goal, origin);
            crate::saber_clash::normalize(&mut danger);
            for branch in 0..edges {
                let next = self.level.navigator.graph.node_edge(waypoint, branch);
                let position = self
                    .level
                    .navigator
                    .graph
                    .node(next)
                    .map_or([0.0; 3], |node| node.position);
                let mut run = subtract(position, origin);
                crate::saber_clash::normalize(&mut run);
                if dot(run, danger) > self.host.rng().flrand(0.0, 0.5) {
                    // "don't run toward danger".
                    continue;
                }
                self.set_move_goal(me, position, 0, true, -1, None);
                reverse = false;
                break;
            }
        }
        let moved = self.move_to_goal(me, false, command);
        if self.npc(me).weapon == 0 && (!moved || reverse) {
            // "No weapon and no escape route... Just cower?"
            self.surrender(me);
            self.update_angles(me, true, true, command);
            return;
        }
        if !moved {
            let goal_origin = goal
                .and_then(|goal| self.flee_goal_origin(me, goal))
                .unwrap_or([0.0; 3]);
            let origin = self.actors[me].current_origin;
            let mut dir = if reverse {
                subtract(origin, goal_origin)
            } else {
                subtract(goal_origin, origin)
            };
            let npc = &mut self.actors[me];
            npc.mind.dist_to_goal = crate::saber_clash::normalize(&mut dir);
            npc.desired_yaw = crate::saber_lock::vector_yaw(dir);
            npc.mind.desired_pitch = 0.0;
            command.forward_move = 127;
        } else if reverse {
            self.actors[me].desired_yaw *= -1.0;
        }
        command.buttons &= !BUTTON_WALKING;
        self.update_angles(me, true, true, command);
        self.check_get_new_weapon(me);
    }

    /// Where the goal the flight started with stands now: the NPC's own goal entity (moved
    /// by the flight's own goal), or the body it went after.
    fn flee_goal_origin(&self, me: usize, goal: u16) -> Option<[f32; 3]> {
        if Some(goal) == self.actors[me].goal {
            return Some(self.actors[me].mind.tactics.temp_goal.origin);
        }
        self.body(goal).map(|body| body.origin)
    }

    /// `NPC_Surrender` (`NPC_behavior.c:1339-1361`): not while it fires or lies knocked
    /// down; a plea (`EV_PUSHED1..3`) when it has not surrendered for five seconds, and a
    /// second's surrender.
    pub fn surrender(&mut self, me: usize) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        let weapon_time = npc.player.raw_field(PS_WEAPON_TIME).unwrap_or(0);
        let legs_timer = npc.player.raw_field(PS_LEGS_TIMER).unwrap_or(0) as i32;
        if weapon_time != 0
            || crate::pmove_hand_extend::in_knockdown(npc.player.leg_animation(), legs_timer)
        {
            return;
        }
        if npc.mind.surrender_time < level_time - 5_000 {
            self.actors[me].mind.blocked_speech_until = 0;
            let event = self.host.irand(EV_PUSHED1, EV_PUSHED3);
            self.add_voice(me, event, 3_000);
        }
        self.actors[me].mind.surrender_time = level_time + 1_000;
    }
}
