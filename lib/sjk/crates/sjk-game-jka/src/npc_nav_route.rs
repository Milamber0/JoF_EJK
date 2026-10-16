//! An NPC's way along the waypoints (`codemp/game/g_navnew.c`, `g_nav.c`): the route to
//! its goal taken a node at a time (`NAVNEW_MoveToGoal`), each next node checked against
//! the world from where the NPC stands (`NAV_TestBestNode`), the step round bodies on the
//! way (`NAVNEW_AvoidCollision`); the waypoints of the players and of any entity found
//! (`NAV_FindPlayerWaypoint`, `NAV_FindClosestWaypointForEnt`); and what `G_RunFrame`
//! does for navigation every frame.
//!
//! `d_altRoutes` and `d_patched` are 0: an edge found blocked by a body is not failed,
//! the NPC stops instead.
//!
//! Held to `tools/game-oracle/npcnav.c` (`game-npcnav.txt`).

use crate::npc_mind::WAYPOINT_NONE;
use crate::npc_nav::NavInfo;
use crate::npc_nav_setup::NavObstacle;
use crate::npc_navigator::{CONTENTS_BODY, CONTENTS_BOTCLIP, NF_CLEAR_PATH, NavHolder};
use crate::npc_senses::{distance_squared, subtract};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use sjk_nav::NODE_NONE;

/// `STEPSIZE`, `MIN_DOOR_BLOCK_DIST_SQR`.
const STEPSIZE: f32 = 18.0;
const MIN_DOOR_BLOCK_DIST_SQR: f32 = 16.0 * 16.0;
/// `NPCAI_BLOCKED`, `NPCAI_ENROUTE_TO_HOMEWP`; `WP_SABER`.
const NPCAI_BLOCKED: u32 = crate::npc_nav::NPCAI_BLOCKED;
const NPCAI_ENROUTE_TO_HOMEWP: u32 = 0x2_0000;
const WP_SABER: i32 = 3;
/// `BS_SEARCH`, `BS_HUNT_AND_KILL`.
pub(crate) const BS_SEARCH: i32 = 5;
const BS_HUNT_AND_KILL: i32 = 15;

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NAV_FindClosestWaypointForEnt` (`g_nav.c:374-378`): the entity's nearest node it
    /// can walk to, starting from its own waypoint, the cheapest to `target` if given.
    pub fn closest_waypoint_for(&mut self, holder: NavHolder, target: i32) -> i32 {
        let last = self.nav_entity(holder).state.waypoint;
        self.nearest_node(holder, last, NF_CLEAR_PATH, target)
    }

    /// The start of `G_RunFrame` for navigation (`g_main.c:3022-3057`), after
    /// `AI_UpdateGroups`: the nodes checked last frame forgotten, and every waypoint held
    /// past its time made the last one — the NPCs', their goal entities', the players'.
    pub fn forget_waypoints(&mut self) {
        let level_time = self.level_time;
        self.level.navigator.clear_checked();
        for &at in self.order {
            let tactics = &mut self.actors[at].mind.tactics;
            let mut state = tactics.nav_state();
            state.forget(level_time);
            (tactics.waypoint, tactics.last_waypoint) = (state.waypoint, state.last_waypoint);
            let goal = &mut tactics.temp_goal;
            if goal.waypoint != WAYPOINT_NONE && goal.no_waypoint_time < level_time {
                (goal.last_waypoint, goal.waypoint) = (goal.waypoint, WAYPOINT_NONE);
            }
        }
        for (_, nav) in &mut self.level.player_navs {
            nav.forget(level_time);
        }
    }

    /// `NAV_FindPlayerWaypoint` (`g_nav.c:1936-1939`) for every player in the game, in
    /// client order, as `G_RunFrame` meets them (`g_main.c:3321-3326`).
    pub fn find_player_waypoints(&mut self) {
        for index in 0..self.host.players().len() {
            let number = self.host.players()[index].number;
            let last = self.level.player_nav(number).last_waypoint;
            let waypoint = self.nearest_node(
                NavHolder::Player(number),
                last,
                NF_CLEAR_PATH,
                WAYPOINT_NONE,
            );
            self.level.player_nav_mut(number).waypoint = waypoint;
        }
    }

    /// The goal entity's `noWaypointTime` set.
    pub(crate) fn set_goal_no_waypoint_time(&mut self, me: usize, time: i32) {
        match self.goal_holder(me) {
            Some(NavHolder::Goal(at)) => {
                self.actors[at].mind.tactics.temp_goal.no_waypoint_time = time
            }
            Some(NavHolder::Player(number)) => {
                self.level.player_nav_mut(number).no_waypoint_time = time
            }
            Some(NavHolder::Actor(at)) => self.actors[at].mind.tactics.no_waypoint_time = time,
            _ => {}
        }
    }

    /// Node `node`'s position, the world's origin for none (`Nav_GetNodePosition` leaves
    /// its output as it was; every caller here passes a fresh one).
    pub(crate) fn node_position(&self, node: i32) -> Option<[f32; 3]> {
        self.level
            .navigator
            .graph
            .node(node)
            .map(|node| node.position)
    }

    /// `NAVNEW_MoveToGoal` (`g_navnew.c:589-850`) with `d_altRoutes` 0: the next node on
    /// the route to the goal — the best path between the NPC and its goal found afresh,
    /// unless both hold waypoints they looked up in the last second — headed for if the
    /// world lets the NPC go straight to it (else back to its own waypoint) and bodies let
    /// it through ([`Self::avoid_collision`] turns `info` round them). `NODE_NONE` where
    /// there is no route or the way is blocked; where one of them has no waypoint it does
    /// not look again for 0.5–1.5 s.
    pub(crate) fn navnew_move_to_goal(
        &mut self,
        me: usize,
        info: &mut NavInfo,
        command: &mut sjk_protocol::UserCommand,
    ) -> i32 {
        let level_time = self.level_time;
        let Some(goal) = self.goal_holder(me) else {
            return NODE_NONE;
        };
        let mine = self.actors[me].mind.tactics.nav_state();
        let theirs = self.nav_entity(goal).state;
        if (mine.waypoint == WAYPOINT_NONE && mine.no_waypoint_time > level_time)
            || (theirs.waypoint == WAYPOINT_NONE && theirs.no_waypoint_time > level_time)
        {
            return NODE_NONE;
        }
        let mut best = if mine.no_waypoint_time > level_time && theirs.no_waypoint_time > level_time
        {
            let mut cost = 0;
            self.level.navigator.graph.best_node_alt_route(
                mine.waypoint,
                theirs.waypoint,
                &mut cost,
                NODE_NONE,
                false,
            )
        } else {
            let best = self.best_path_between(NavHolder::Actor(me), goal, NF_CLEAR_PATH);
            if best == NODE_NONE {
                if self.actors[me].mind.tactics.waypoint == NODE_NONE {
                    let delay = self.host.irand(500, 1_500);
                    self.actors[me].mind.tactics.no_waypoint_time = level_time + delay;
                }
                if self.nav_entity(goal).state.waypoint == NODE_NONE {
                    let delay = self.host.irand(500, 1_500);
                    self.set_goal_no_waypoint_time(me, level_time + delay);
                }
                return NODE_NONE;
            }
            if self.nav_entity(goal).state.no_waypoint_time < level_time {
                let delay = self.host.irand(500, 1_500);
                self.set_goal_no_waypoint_time(me, level_time + delay);
            }
            best
        };
        let Some(mut origin) = self.node_position(best) else {
            return NODE_NONE;
        };
        let waypoint = self.actors[me].mind.tactics.waypoint;
        if best != waypoint {
            // Heading for an edge off the waypoint confirmed clear: make sure it is.
            let old = best;
            best = self.test_best_node(me, waypoint, best, true);
            if best == waypoint {
                self.actors[me].ai_flags |= NPCAI_BLOCKED;
                self.actors[me].mind.tactics.blocked_dest =
                    self.node_position(old).unwrap_or_default();
                if let Some(position) = self.node_position(best) {
                    origin = position;
                }
            }
        }
        let mut attempt = *info;
        let mut direction = subtract(origin, self.actors[me].current_origin);
        crate::player_angle_math::normalize(&mut direction);
        attempt.direction = direction;
        let goal_number = self.actors[me].mind.goal;
        if !self.avoid_collision(me, goal_number, &mut attempt, true, 5, command) {
            // Blocked by a body: with `d_altRoutes` 0 the NPC stops, whichever node it
            // headed for.
            self.actors[me].ai_flags |= NPCAI_BLOCKED;
            self.actors[me].mind.tactics.blocked_dest =
                self.node_position(best).unwrap_or_default();
            return NODE_NONE;
        }
        // `NPC_ClearBlocked`.
        self.actors[me].mind.tactics.blocking_ent_num = i32::from(crate::npc_spawn::ENTITYNUM_NONE);
        *info = attempt;
        if self.npc(me).weapon == WP_SABER && info.direction[2] * info.distance > 64.0 {
            self.actors[me].ai_flags |= NPCAI_BLOCKED;
            self.actors[me].mind.tactics.blocked_dest = origin;
            return NODE_NONE;
        }
        let tactics = &mut self.actors[me].mind.tactics;
        tactics.shove_count = 0;
        if tactics.no_waypoint_time < level_time {
            let delay = self.host.irand(500, 1_500);
            self.actors[me].mind.tactics.no_waypoint_time = level_time + delay;
        }
        best
    }

    /// `NAV_TestBestNode` (`g_nav.c:993-1099`): `end` if the NPC's stepping box goes
    /// straight to it through the world (or all but its own radius of the way, unless it
    /// is over 48 units above or below and no saber carrier), or an unlocked door stands in
    /// the way and the NPC is not already at it; else `start` — the edge failed first
    /// (`fail_edge`) if a locked door, a breakable, a removable usable or a clip brush a
    /// script will remove is in the way.
    pub(crate) fn test_best_node(
        &mut self,
        me: usize,
        start: i32,
        end: i32,
        fail_edge: bool,
    ) -> i32 {
        let Some(target) = self.node_position(end) else {
            return start;
        };
        let npc = &self.actors[me];
        let (origin, maxs, number) = (npc.current_origin, npc.maxs, npc.number);
        let mins = [npc.mins[0], npc.mins[1], npc.mins[2] + STEPSIZE];
        let clip = (npc.clip_mask & !CONTENTS_BODY) | CONTENTS_BOTCLIP;
        let mut trace = self.trace_bodies(origin, mins, maxs, target, number, clip);
        if trace.start_solid {
            let again =
                self.trace_bodies(origin, mins, maxs, target, number, clip & !CONTENTS_BOTCLIP);
            if !again.start_solid {
                trace = again;
            }
        }
        if !trace.all_solid && !trace.start_solid && trace.fraction == 1.0 {
            return end;
        }
        let saber = self.npc(me).weapon == WP_SABER;
        let too_far = !saber && f64::from(origin[2] - target[2]).abs() > 48.0;
        if !too_far {
            let radius = maxs[0].max(maxs[1]);
            if trace.fraction >= 1.0 - radius / sjk_nav::distance(origin, target) {
                return end;
            }
        }
        if trace.entity_number >= crate::npc_spawn::ENTITYNUM_WORLD {
            return start;
        }
        match self.host.nav_obstacle(trace.entity_number) {
            NavObstacle::Door { unlocked: true } => {
                if distance_squared(origin, trace.end_position) < MIN_DOOR_BLOCK_DIST_SQR {
                    return start;
                }
                if !too_far {
                    return end;
                }
            }
            NavObstacle::Door { unlocked: false }
            | NavObstacle::Breakable
            | NavObstacle::RemovableUsable
            | NavObstacle::ClipBrush => {
                if fail_edge {
                    let time = self.level_time;
                    self.level
                        .navigator
                        .add_failed_edge(i32::from(number), start, end, time);
                }
            }
            NavObstacle::Other => {}
        }
        start
    }

    /// `NPC_BSSearchStart` (`NPC_behavior.c:1153-1171`): a search from `home` (the NPC's
    /// nearest waypoint where none is given, kept as its own if it holds none), the goal
    /// entity put at that node. (`NPC_BSSearch` itself is a later step's.)
    pub fn search_start(&mut self, me: usize, home: i32, state: i32) {
        let mut home = home;
        if home == WAYPOINT_NONE {
            home = self.closest_waypoint_for(NavHolder::Actor(me), WAYPOINT_NONE);
            if self.actors[me].mind.tactics.waypoint == WAYPOINT_NONE {
                self.actors[me].mind.tactics.waypoint = home;
            }
        }
        let position = self.node_position(home);
        let npc = &mut self.actors[me];
        npc.mind.tactics.home_waypoint = home;
        npc.mind.temp_behavior = state;
        npc.ai_flags |= NPCAI_ENROUTE_TO_HOMEWP;
        npc.mind.tactics.investigate_debounce_time = 0;
        if let Some(position) = position {
            npc.mind.tactics.temp_goal.origin = position;
        }
        npc.mind.tactics.temp_goal.waypoint = home;
    }

    /// `NPC_LostEnemyDecideChase` (`NPC_AI_Default.c:34-52`): an NPC hunting the enemy it
    /// went after searches from the enemy's last waypoint; the enemy forgotten.
    pub fn lost_enemy_decide_chase(&mut self, me: usize) {
        let npc = &self.actors[me];
        if npc.behavior_state == BS_HUNT_AND_KILL
            && npc.mind.enemy.is_some()
            && npc.mind.enemy == npc.mind.goal
        {
            let last = npc
                .mind
                .enemy
                .and_then(|enemy| self.nav_holder_of(enemy))
                .map_or(WAYPOINT_NONE, |holder| {
                    self.nav_entity(holder).state.last_waypoint
                });
            if last != WAYPOINT_NONE {
                self.search_start(me, last, BS_SEARCH);
            }
        }
        self.clear_enemy(me);
    }
}
