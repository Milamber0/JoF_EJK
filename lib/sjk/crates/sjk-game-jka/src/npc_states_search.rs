//! Searching and wandering along the waypoints (`NPC_BSSearch`, `NPC_BSWander`,
//! `NPC_behavior.c:960-1145`, `1215-1308`) from the home waypoint `NPC_BSSearchStart` set
//! ([`crate::npc_nav_route`]); and what a searcher that finds an enemy, or runs out of
//! waypoints, turns to: `NPC_BSRunAndShoot`, `NPC_StandTrackAndShoot` and
//! `NPC_BSStandGuard` (`NPC_AI_Default.c:104-234`, `404-497`).

use crate::npc_behavior::bstate;
use crate::npc_mind::WAYPOINT_NONE;
use crate::npc_navigator::NavHolder;
use crate::npc_spawn::NpcHost;
use crate::npc_states::BUTTON_ATTACK;
use crate::npc_world::NpcWorld;
use crate::pmove_anim::SETANIM_BOTH;
use sjk_protocol::UserCommand;

/// `NPCAI_ENROUTE_TO_HOMEWP`.
const NPCAI_ENROUTE_TO_HOMEWP: u32 = 0x2_0000;
/// `BOTH_GUARD_LOOKAROUND1`, `BOTH_GUARD_IDLE1` (`anims.h`); `SETANIM_FLAG_NORMAL`.
const BOTH_GUARD_LOOKAROUND1: u16 = 961;
const BOTH_GUARD_IDLE1: u16 = 962;
const SETANIM_FLAG_NORMAL: u8 = 0;

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSSearch` (`NPC_behavior.c:960-1145`): an enemy found ends the search (a
    /// temporary one, or it hunts); else to the goal entity at the waypoint in hand, then a
    /// look about for three to ten seconds, then off down a random branch of the home
    /// waypoint — or home again from a branch.
    pub fn bs_search(&mut self, me: usize, command: &mut UserCommand) {
        self.check_enemy(me, true, false, true);
        if self.actors[me].mind.enemy.is_some() {
            let npc = &mut self.actors[me];
            if npc.mind.temp_behavior == bstate::SEARCH {
                npc.mind.temp_behavior = bstate::DEFAULT;
            } else {
                npc.behavior_state = bstate::HUNT_AND_KILL;
                self.bs_run_and_shoot(me, command);
            }
            return;
        }
        let level_time = self.level_time;
        if self.actors[me].mind.tactics.investigate_debounce_time == 0 {
            // "On our way to a tempGoal"; the waypoint's radius is not read (32 either way).
            let npc = &mut self.actors[me];
            npc.mind.goal = npc.goal;
            let mut to_goal =
                crate::npc_senses::subtract(npc.mind.tactics.temp_goal.origin, npc.current_origin);
            if to_goal[2] < 24.0 {
                to_goal[2] = 0.0;
            }
            if length_squared(to_goal) < 32.0 * 32.0 {
                let waypoint = self.closest_waypoint_for(NavHolder::Actor(me), WAYPOINT_NONE);
                let npc = &mut self.actors[me];
                npc.mind.tactics.waypoint = waypoint;
                if npc.mind.tactics.home_waypoint == WAYPOINT_NONE || waypoint == WAYPOINT_NONE {
                    // "Heading for or at an invalid waypoint, get out of this bState".
                    if npc.mind.temp_behavior == bstate::SEARCH {
                        npc.mind.temp_behavior = bstate::DEFAULT;
                    } else {
                        npc.behavior_state = bstate::STAND_GUARD;
                        self.bs_run_and_shoot(me, command);
                    }
                    return;
                }
                if waypoint == npc.mind.tactics.home_waypoint
                    && npc.ai_flags & NPCAI_ENROUTE_TO_HOMEWP != 0
                {
                    // Home for the first time: the lost-enemy script would run (none does).
                    npc.ai_flags &= !NPCAI_ENROUTE_TO_HOMEWP;
                }
                self.look_about_animation(me);
            } else {
                self.move_to_goal(me, true, command);
            }
        } else if self.actors[me].mind.tactics.investigate_debounce_time > level_time {
            self.look_about(me);
        } else {
            // "Just finished waiting".
            let waypoint = self.closest_waypoint_for(NavHolder::Actor(me), WAYPOINT_NONE);
            self.actors[me].mind.tactics.waypoint = waypoint;
            let home = self.actors[me].mind.tactics.home_waypoint;
            if waypoint == home {
                // The branches are counted at the goal's waypoint and taken from home.
                let edges = self.node_edge_count(self.actors[me].mind.tactics.temp_goal.waypoint);
                if edges != WAYPOINT_NONE {
                    let branch = self.host.irand(0, edges - 1);
                    let next = self.level.navigator.graph.node_edge(home, branch);
                    self.put_temp_goal_at_node(me, next);
                }
            } else {
                self.put_temp_goal_at_node(me, home);
            }
            let npc = &mut self.actors[me];
            npc.mind.tactics.investigate_debounce_time = 0;
            npc.mind.goal = npc.goal;
            self.move_to_goal(me, true, command);
        }
        self.update_angles(me, true, true, command);
    }

    /// `NPC_BSWander` (`NPC_behavior.c:1215-1308`): to the goal entity (near enough within
    /// eight units), a look about for three to ten seconds, then down a random branch of the
    /// waypoint it stands at.
    pub fn bs_wander(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        if self.actors[me].mind.tactics.investigate_debounce_time == 0 {
            let npc = &mut self.actors[me];
            npc.mind.goal = npc.goal;
            let to_goal =
                crate::npc_senses::subtract(npc.mind.tactics.temp_goal.origin, npc.current_origin);
            if length_squared(to_goal) < 64.0 {
                let waypoint = self.closest_waypoint_for(NavHolder::Actor(me), WAYPOINT_NONE);
                self.actors[me].mind.tactics.waypoint = waypoint;
                self.look_about_animation(me);
            } else {
                self.move_to_goal(me, true, command);
            }
        } else if self.actors[me].mind.tactics.investigate_debounce_time > level_time {
            self.look_about(me);
        } else {
            let waypoint = self.closest_waypoint_for(NavHolder::Actor(me), WAYPOINT_NONE);
            self.actors[me].mind.tactics.waypoint = waypoint;
            if waypoint != WAYPOINT_NONE {
                let edges = self.node_edge_count(waypoint);
                if edges != WAYPOINT_NONE {
                    let branch = self.host.irand(0, edges - 1);
                    let next = self.level.navigator.graph.node_edge(waypoint, branch);
                    self.put_temp_goal_at_node(me, next);
                }
                let npc = &mut self.actors[me];
                npc.mind.tactics.investigate_debounce_time = 0;
                npc.mind.goal = npc.goal;
                self.move_to_goal(me, true, command);
            }
        }
        self.update_angles(me, true, true, command);
    }

    /// The search's arrival (`NPC_behavior.c:1045-1053`, `1237-1246`): a look round or a
    /// guard's idle, and the time it looks about for.
    fn look_about_animation(&mut self, me: usize) {
        let animation = if self.host.irand(0, 1) == 0 {
            BOTH_GUARD_LOOKAROUND1
        } else {
            BOTH_GUARD_IDLE1
        };
        self.set_animation(me, SETANIM_BOTH, animation, SETANIM_FLAG_NORMAL);
        let wait = self.host.irand(3_000, 10_000);
        self.actors[me].mind.tactics.investigate_debounce_time = self.level_time + wait;
    }

    /// "Turn angles every now and then to look around" (`NPC_behavior.c:1067-1096`,
    /// `1261-1280`): one think in thirty-one, toward a random branch of the goal's waypoint,
    /// give or take 45 degrees.
    fn look_about(&mut self, me: usize) {
        let goal = self.actors[me].mind.tactics.temp_goal;
        if goal.waypoint == WAYPOINT_NONE || self.host.irand(0, 30) != 0 {
            return;
        }
        let edges = self.node_edge_count(goal.waypoint);
        if edges == WAYPOINT_NONE {
            return;
        }
        let branch = self.host.irand(0, edges - 1);
        let next = self.level.navigator.graph.node_edge(goal.waypoint, branch);
        // `branchPos` is left as it was for a node without edges: here the origin.
        let position = self
            .level
            .navigator
            .graph
            .node(next)
            .map_or([0.0; 3], |node| node.position);
        let look = crate::npc_senses::subtract(position, goal.origin);
        let yaw = crate::saber_lock::vector_yaw(look) + self.host.rng().flrand(-45.0, 45.0);
        self.actors[me].desired_yaw = crate::npc_droid::angle_normalize360(yaw);
    }

    /// `Nav_GetNodeNumEdges` (`navigator.cpp:2472-2482`): -1 for no node.
    pub(crate) fn node_edge_count(&self, node: i32) -> i32 {
        self.level
            .navigator
            .graph
            .node(node)
            .map_or(WAYPOINT_NONE, |node| node.edges.len() as i32)
    }

    /// `Nav_GetNodePosition(node, tempGoal->r.currentOrigin)` and `tempGoal->waypoint =
    /// node`: the goal entity moved to the node (left where it was for none).
    fn put_temp_goal_at_node(&mut self, me: usize, node: i32) {
        let position = self
            .level
            .navigator
            .graph
            .node(node)
            .map(|node| node.position);
        let goal = &mut self.actors[me].mind.tactics.temp_goal;
        if let Some(position) = position {
            goal.origin = position;
        }
        goal.waypoint = node;
    }

    /// `NPC_BSRunAndShoot` (`NPC_AI_Default.c:404-497`): a duck held on; with an enemy, aim
    /// and fire standing, and — held back from hitting it — run at it; without one, a
    /// temporary hunt ends.
    pub fn bs_run_and_shoot(&mut self, me: usize, command: &mut UserCommand) {
        self.check_enemy(me, true, false, true);
        if self.actors[me].mind.states.duck_debounce_time > self.level_time {
            command.up_move = -127;
            if self.actors[me].mind.enemy.is_some() {
                self.check_can_attack(me, 1.0, false, command);
            }
            return;
        }
        let Some(enemy) = self.actors[me].mind.enemy else {
            let npc = &mut self.actors[me];
            if npc.mind.temp_behavior == bstate::HUNT_AND_KILL {
                npc.mind.temp_behavior = bstate::DEFAULT;
            }
            return;
        };
        let monitor = self.actors[me].mind.cant_hit_enemy_counter;
        self.stand_track_and_shoot(me, false, command);
        let npc = &self.actors[me];
        if command.buttons & BUTTON_ATTACK == 0
            && command.up_move >= 0
            && npc.mind.cant_hit_enemy_counter > monitor
        {
            let origin = self.body(enemy).map_or([0.0; 3], |body| body.origin);
            let mut to_enemy = crate::npc_senses::subtract(origin, npc.current_origin);
            to_enemy[2] = 0.0;
            if length_squared(to_enemy).sqrt() > 128.0 || npc.mind.cant_hit_enemy_counter >= 10 {
                let npc = &mut self.actors[me];
                npc.mind.cant_hit_enemy_counter = npc.mind.cant_hit_enemy_counter.min(60);
                if npc.mind.cant_hit_enemy_counter >= (npc.definition.stats.aggression + 1) * 10 {
                    self.lost_enemy_decide_chase(me);
                }
                command.angles[1] = 0;
                command.angles[0] = 0;
                let npc = &mut self.actors[me];
                npc.mind.goal = npc.mind.enemy;
                npc.mind.tactics.goal_radius = 12;
                self.move_to_goal(me, true, command);
                self.update_angles(me, true, true, command);
            }
        } else {
            self.actors[me].mind.cant_hit_enemy_counter = 0;
        }
    }

    /// `NPC_StandTrackAndShoot` (`NPC_AI_Default.c:104-168`): badly hurt, a ducker may
    /// duck; else it aims and fires if it can (`NPC_CheckCanAttack`), and ducks from an
    /// enemy firing at it. Whether the angles were set.
    pub(crate) fn stand_track_and_shoot(
        &mut self,
        me: usize,
        can_duck: bool,
        command: &mut UserCommand,
    ) -> bool {
        let mut duck = false;
        let (mut attack, mut faced) = (false, false);
        if can_duck && self.actors[me].health < 20 && self.host.rng().flrand(0.0, 1.0) != 0.0 {
            duck = true;
        }
        if !duck {
            attack = self.check_can_attack(me, 1.0, true, command);
            faced = true;
        }
        let weapon_time = self.actors[me]
            .player
            .raw_field(crate::npc_states_fight::PS_WEAPON_TIME)
            .unwrap_or(0) as i32;
        if can_duck && (duck || (!attack && weapon_time <= 0)) && command.up_move != -127 {
            if !duck
                && let Some(enemy) = self.actors[me]
                    .mind
                    .enemy
                    .and_then(|number| self.body(number))
                && enemy.enemy == Some(self.actors[me].number)
                && self.enemy_attacking(enemy.number)
                && self.check_defend(me, 1.0)
            {
                duck = true;
            }
            if duck {
                command.up_move = -127;
                self.actors[me].mind.states.duck_debounce_time = self.level_time + 1_000;
            }
        }
        faced
    }

    /// `NPC_BSStandGuard` (`NPC_AI_Default.c:201-234`): half the time an enemy is looked
    /// for (the player first against the player's side; in sight unless it could not hit
    /// the last one); with one, a temporary guard ends and a guard stands and shoots.
    pub fn bs_stand_guard(&mut self, me: usize, command: &mut UserCommand) {
        if self.actors[me].mind.enemy.is_none() && self.host.rng().flrand(0.0, 1.0) < 0.5 {
            let enemy_team = self.actors[me].enemy_team;
            if enemy_team != 0 {
                let check_vis = self.actors[me].mind.cant_hit_enemy_counter < 10;
                let players_first = enemy_team == crate::npc_enemy::NPCTEAM_PLAYER;
                if let Some(enemy) = self.pick_enemy_first(me, enemy_team, check_vis, players_first)
                {
                    self.set_enemy(me, enemy);
                }
            }
        }
        if self.actors[me].mind.enemy.is_some() {
            let npc = &mut self.actors[me];
            if npc.mind.temp_behavior == bstate::STAND_GUARD {
                npc.mind.temp_behavior = bstate::DEFAULT;
            }
            if npc.behavior_state == bstate::STAND_GUARD {
                npc.behavior_state = bstate::STAND_AND_SHOOT;
            }
        }
        self.update_angles(me, true, true, command);
    }

    /// `NPC_PickEnemy(NPC, enemyTeam, checkVis, findPlayersFirst, qtrue)`
    /// (`NPC_combat.c:1469-1762`): with `players_first`, client 0 is tried first (a living,
    /// valid, targetable enemy not the last, in the potentially visible set, not beyond the
    /// weapon's reach, and in sight where asked); else, or failing that, the closest of
    /// all ([`Self::pick_enemy`]).
    pub(crate) fn pick_enemy_first(
        &mut self,
        me: usize,
        enemy_team: i32,
        check_vis: bool,
        players_first: bool,
    ) -> Option<u16> {
        if enemy_team == crate::npc_enemy::NPCTEAM_NEUTRAL {
            return None;
        }
        if players_first
            && let Some(player) = self
                .host
                .players()
                .iter()
                .find(|body| body.number == 0)
                .copied()
        {
            let npc = self.npc(me);
            let behavior = self.actors[me].behavior_state;
            let usable = player.flags & 0x20 == 0
                && player.entity_flags & crate::npc_senses::EF_NODRAW == 0
                && player.health > 0
                && self.valid_for(me, &player)
                && self.actors[me].mind.last_enemy != Some(0)
                && self.host.in_pvs(player.origin, npc.origin);
            let watchful = (behavior == bstate::INVESTIGATE || behavior == bstate::PATROL)
                && self.actors[me].mind.enemy.is_none();
            let unseen = watchful
                && (!crate::npc_senses::in_visrange(&player, &npc, self.sight(me).visrange)
                    || self.visibility(
                        me,
                        &player,
                        crate::npc_senses::CHECK_360
                            | crate::npc_senses::CHECK_FOV
                            | crate::npc_senses::CHECK_VISRANGE,
                    ) != crate::npc_senses::Visibility::Fov);
            if usable && !unseen {
                let distance = crate::npc_senses::distance_squared(
                    self.actors[me].current_origin,
                    player.origin,
                );
                if distance < crate::npc_navigator::Q3_INFINITE as f32
                    && !self.enemy_too_far(me, &player, distance, false)
                {
                    let fighting =
                        behavior == bstate::STAND_AND_SHOOT || behavior == bstate::HUNT_AND_KILL;
                    let (checks, least) = if fighting {
                        (
                            crate::npc_senses::CHECK_360 | crate::npc_senses::CHECK_VISRANGE,
                            crate::npc_senses::Visibility::Full360,
                        )
                    } else {
                        (
                            crate::npc_senses::CHECK_360
                                | crate::npc_senses::CHECK_FOV
                                | crate::npc_senses::CHECK_VISRANGE,
                            crate::npc_senses::Visibility::Fov,
                        )
                    };
                    if !check_vis || self.visibility(me, &player, checks) == least {
                        return Some(0);
                    }
                }
            }
        }
        self.pick_enemy(me, me, enemy_team, check_vis)
    }
}

/// `VectorLengthSquared`: float products summed left to right.
pub(crate) fn length_squared(vector: [f32; 3]) -> f32 {
    vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]
}
