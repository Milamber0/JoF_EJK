//! The level's navigator as Jedi Academy's engine keeps it (`codemp/server/NPCNav/
//! navigator.cpp`, the game's `trap->Nav_*`): the graph of the map's waypoints
//! ([`sjk_nav::Graph`], engine functionality with no game in it) with the reference's
//! limits, the nodes checked against an entity this frame (`CheckedNodes`), the failed
//! nodes an entity keeps, and the queries that need the world — the node nearest an
//! entity it can walk to (`GetNearestNode`, `TestBestFirst`), the best route between two
//! entities (`GetBestPathBetweenEnts`).
//!
//! The graph is loaded from the map's `.nav` file ([`crate::nav_file`]) when its checksum
//! is the map's, else made from the map's waypoint entities 400 ms into the level
//! ([`crate::npc_nav_setup`]). Nothing is written back to disk (`Nav_Save`):
//! [`crate::nav_file::save`] makes the file's bytes for whoever wants them.
//!
//! `d_altRoutes` and `d_patched` (cheat cvars, 0 by default) are 0: failed edges are
//! kept but never checked again (`CheckAllFailedEdges`, `CheckFailedNodes`) and ranks
//! never recalculated (`NF_RECALC`).
//!
//! Held to `tools/game-oracle/npcnav.c` (`game-npcnav.txt`, `game-npcnavload.txt`).

use crate::npc_mind::WAYPOINT_NONE;
use crate::npc_senses::distance_squared;
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use crate::player_death::Rng;
use sjk_nav::{Candidate, Graph, Limits, NODE_NONE};
use std::collections::HashMap;

/// `Q3_INFINITE`; `WORLD_SIZE`.
pub const Q3_INFINITE: i32 = 16_777_216;
const WORLD_SIZE: i32 = 131_072;
/// `NF_CLEAR_PATH`.
pub const NF_CLEAR_PATH: i32 = 0x2;
/// `MAX_FAILED_NODES`: the nodes an entity remembers failing to reach.
pub const MAX_FAILED_NODES: usize = 8;
/// `MAX_STORED_WAYPOINTS`: the waypoints kept for connecting, and the nodes whose checks
/// are cached (`CheckedNode`).
pub const MAX_STORED_WAYPOINTS: usize = 512;
/// `NODE_COLLECT_RADIUS`, `NODE_COLLECT_MAX`, `MAX_Z_DELTA`.
const NODE_COLLECT_RADIUS: i32 = 512;
const NODE_COLLECT_MAX: usize = 16;
const MAX_Z_DELTA: f64 = 18.0;
/// `CHECKED_NO`, `CHECKED_FAILED`, `CHECKED_PASSED`.
const CHECKED_NO: u8 = 0;
const CHECKED_FAILED: u8 = 1;
const CHECKED_PASSED: u8 = 2;
/// `CHECK_FAILED_EDGE_INTERVAL`.
const CHECK_FAILED_EDGE_INTERVAL: i32 = 1_000;
/// `MASK_SOLID`, `CONTENTS_BODY`, `CONTENTS_MONSTERCLIP`, `CONTENTS_BOTCLIP`.
pub(crate) const MASK_SOLID: u32 = 0x1 | 0x1000;
pub(crate) const CONTENTS_BODY: u32 = 0x100;
pub(crate) const CONTENTS_MONSTERCLIP: u32 = 0x20;
pub(crate) const CONTENTS_BOTCLIP: u32 = 0x40;
/// `STEPSIZE`.
const STEPSIZE: f32 = 18.0;

/// A graph with the reference's limits: its infinite cost, its world size, its 32 failed
/// edges.
pub fn new_graph() -> Graph {
    Graph::new(
        Limits {
            infinite_cost: Q3_INFINITE,
            rank_ceiling: WORLD_SIZE,
        },
        crate::nav_file::MAX_FAILED_EDGES,
    )
}

/// What an entity keeps for navigating: `waypoint`, `lastWaypoint`, `noWaypointTime`,
/// `failedWaypoints` (a node plus one; 0 for none), `failedWaypointCheckTime`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NavState {
    pub waypoint: i32,
    pub last_waypoint: i32,
    pub no_waypoint_time: i32,
    pub failed: [i32; MAX_FAILED_NODES],
    pub failed_check_time: i32,
}

impl NavState {
    /// `NodeFailed`, `NAV_CheckNodeFailedForEnt`.
    pub fn node_failed(&self, node: i32) -> bool {
        self.failed.iter().any(|failed| failed - 1 == node)
    }

    /// `G_RunFrame`'s forgetting (`g_main.c:3044-3050`): a waypoint held past its time
    /// becomes the last one.
    pub fn forget(&mut self, level_time: i32) {
        if self.waypoint != WAYPOINT_NONE && self.no_waypoint_time < level_time {
            self.last_waypoint = self.waypoint;
            self.waypoint = WAYPOINT_NONE;
        }
    }
}

/// A waypoint kept until the paths are calculated (`waypointData_t`): its names and its
/// node.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoredWaypoint {
    pub targetname: Vec<u8>,
    pub targets: [Vec<u8>; 4],
    pub node: i32,
}

/// A navigation goal a script names (`TAG_Add` with `RTF_NAVGOAL`): its name (lowered),
/// place, angles and radius.
#[derive(Clone, Debug, PartialEq)]
pub struct NavGoalTag {
    pub name: Vec<u8>,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub radius: i32,
}

/// The level's navigator.
#[derive(Clone, Debug)]
pub struct Navigator {
    /// The graph.
    pub graph: Graph,
    /// `CheckedNodes`: what a node's test against an entity gave this frame.
    checked: HashMap<(i32, u16), u8>,
    /// `navCalculatePaths`: no file was loaded, so the waypoint entities make the graph.
    pub calculating: bool,
    /// `navCalcPathTime`: when they do (0 once done).
    pub calc_at: i32,
    /// `tempWaypointList`.
    pub stored: Vec<StoredWaypoint>,
    /// The navigation goals (`waypoint_navgoal*`).
    pub tags: Vec<NavGoalTag>,
    /// The engine's own generator (`Q_irand` in the server, apart from the game's).
    rng: Rng,
    /// The candidates of the nearest-node queries, kept from query to query.
    near: Vec<Candidate>,
    near_goal: Vec<Candidate>,
    /// The waypoint the last query left a [`NavHolder::Marker`] (an item's, which
    /// `NPC_SearchForWeapons` reads).
    pub marker_waypoint: i32,
}

impl Default for Navigator {
    fn default() -> Self {
        Self {
            graph: new_graph(),
            checked: HashMap::new(),
            // No file loaded: the waypoint entities make the graph.
            calculating: true,
            calc_at: 0,
            stored: Vec::new(),
            tags: Vec::new(),
            rng: Rng(0x89ab_cdef),
            near: Vec::with_capacity(NODE_COLLECT_MAX + 1),
            near_goal: Vec::with_capacity(NODE_COLLECT_MAX + 1),
            marker_waypoint: WAYPOINT_NONE,
        }
    }
}

impl Navigator {
    /// `Nav_Load` as `G_InitGame` asks it (`g_main.c:325`): the map's file, if there is
    /// one and it is for this build of the map. Whether it was; if not, the waypoint
    /// entities will make the graph.
    pub fn load(&mut self, file: Option<&[u8]>, checksum: i32) -> bool {
        self.graph.free();
        let loaded = file
            .is_some_and(|bytes| crate::nav_file::load(&mut self.graph, bytes, checksum).is_ok());
        self.calculating = !loaded;
        if loaded {
            // `Nav_SetPathsCalculated(qtrue)` (`g_main.c:387`).
            self.graph.paths_calculated = true;
        }
        loaded
    }

    /// `ClearCheckedNodes`.
    pub fn clear_checked(&mut self) {
        self.checked.clear();
    }

    /// `CheckedNode`: never cached past the stored waypoints' count.
    fn checked(&self, node: i32, entity: u16) -> u8 {
        if !(0..MAX_STORED_WAYPOINTS as i32).contains(&node) {
            return CHECKED_NO;
        }
        self.checked
            .get(&(node, entity))
            .copied()
            .unwrap_or(CHECKED_NO)
    }

    /// `SetCheckedNode`.
    fn set_checked(&mut self, node: i32, entity: u16, value: u8) {
        if (0..MAX_STORED_WAYPOINTS as i32).contains(&node) {
            self.checked.insert((node, entity), value);
        }
    }

    /// `AddFailedEdge` (`navigator.cpp:1998-2090`) with `d_patched` 0: the edge from
    /// `start` to `end` failed for `entity`, checked again a second or two from now (with
    /// `d_altRoutes`), and — once the paths are known — costed infinite, every node marked
    /// to recalculate.
    pub fn add_failed_edge(&mut self, entity: i32, start: i32, end: i32, time: i32) {
        let count = self.graph.len() as i32;
        if count == 0
            || !(0..=crate::npc_spawn::ENTITYNUM_NONE as i32).contains(&entity)
            || !(0..count).contains(&start)
            || !(0..count).contains(&end)
        {
            return;
        }
        if self.graph.failed.edge_failed(start, end).is_some() {
            let _ = self.graph.failed.add(entity, start, end, 0);
            return;
        }
        if !self
            .graph
            .failed
            .slots()
            .iter()
            .any(|slot| slot.start == NODE_NONE)
        {
            return;
        }
        let check_time = time + CHECK_FAILED_EDGE_INTERVAL + self.rng.irand(0, 1_000);
        let _ = self.graph.failed.add(entity, start, end, check_time);
        if self.graph.paths_calculated {
            self.graph.set_edge_cost(start, end, Some(Q3_INFINITE));
            self.graph.flag_all(sjk_nav::NODE_RECALC);
        }
    }

    /// `ClearAllFailedEdges`: every slot cleared, its edge's cost made the distance again.
    pub fn clear_all_failed_edges(&mut self) {
        self.graph.failed.fill_none();
        for at in 0..self.graph.failed.slots().len() {
            let slot = self.graph.failed.slots()[at];
            self.graph.set_edge_cost(slot.start, slot.end, None);
            if let Some(slot) = self.graph.failed.slot_mut(at) {
                *slot = sjk_nav::FailedEdge {
                    start: NODE_NONE,
                    end: NODE_NONE,
                    check_time: 0,
                    entity: i32::from(crate::npc_spawn::ENTITYNUM_NONE),
                };
            }
        }
    }
}

/// An entity a navigation query is about: a player, an NPC, an NPC's goal entity, or a
/// marker put somewhere for a moment (`NAV_FindClosestWaypointForPoint2`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NavHolder {
    Player(u16),
    Actor(usize),
    Goal(usize),
    Marker {
        number: u16,
        origin: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
    },
}

/// What a query reads of an entity.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NavEntity {
    pub(crate) number: u16,
    pub(crate) origin: [f32; 3],
    pub(crate) mins: [f32; 3],
    pub(crate) maxs: [f32; 3],
    /// `self->client`: it steps.
    pub(crate) client: bool,
    /// `FL_NAVGOAL`: a goal entity tested from its owner's place (the NPC's index).
    pub(crate) nav_goal_of: Option<usize>,
    pub(crate) state: NavState,
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// The entity `holder` names, as a query reads it.
    pub(crate) fn nav_entity(&self, holder: NavHolder) -> NavEntity {
        let blank = NavEntity {
            number: crate::npc_spawn::ENTITYNUM_NONE,
            origin: [0.0; 3],
            mins: [0.0; 3],
            maxs: [0.0; 3],
            client: false,
            nav_goal_of: None,
            state: NavState::default(),
        };
        match holder {
            NavHolder::Player(number) => {
                let Some(body) = self
                    .host
                    .players()
                    .iter()
                    .find(|body| body.number == number)
                else {
                    return blank;
                };
                NavEntity {
                    number,
                    origin: body.origin,
                    mins: body.mins,
                    maxs: body.maxs,
                    client: true,
                    nav_goal_of: None,
                    state: self.level.player_nav(number),
                }
            }
            NavHolder::Actor(at) => {
                let npc = &self.actors[at];
                NavEntity {
                    number: npc.number,
                    origin: npc.current_origin,
                    mins: npc.mins,
                    maxs: npc.maxs,
                    client: true,
                    nav_goal_of: None,
                    state: npc.mind.tactics.nav_state(),
                }
            }
            NavHolder::Goal(at) => {
                let npc = &self.actors[at];
                let goal = npc.mind.tactics.temp_goal;
                let state = NavState {
                    waypoint: goal.waypoint,
                    last_waypoint: goal.last_waypoint,
                    no_waypoint_time: goal.no_waypoint_time,
                    ..NavState::default()
                };
                NavEntity {
                    number: npc.goal.unwrap_or(crate::npc_spawn::ENTITYNUM_NONE),
                    origin: goal.origin,
                    mins: goal.mins,
                    maxs: goal.maxs,
                    client: false,
                    nav_goal_of: goal.nav_goal.then_some(at),
                    state,
                }
            }
            NavHolder::Marker {
                number,
                origin,
                mins,
                maxs,
            } => NavEntity {
                number,
                origin,
                mins,
                maxs,
                client: false,
                nav_goal_of: None,
                state: NavState {
                    waypoint: WAYPOINT_NONE,
                    ..NavState::default()
                },
            },
        }
    }

    /// The entity's `waypoint` set.
    pub(crate) fn set_nav_waypoint(&mut self, holder: NavHolder, waypoint: i32) {
        match holder {
            NavHolder::Player(number) => self.level.player_nav_mut(number).waypoint = waypoint,
            NavHolder::Actor(at) => self.actors[at].mind.tactics.waypoint = waypoint,
            NavHolder::Goal(at) => self.actors[at].mind.tactics.temp_goal.waypoint = waypoint,
            NavHolder::Marker { .. } => self.level.navigator.marker_waypoint = waypoint,
        }
    }

    /// `GetEdgeCost` (`navigator.cpp:740-763`, `2640-2656`): the distance between two
    /// nodes where a 16-unit cube goes from one to the other through the world, else the
    /// infinite cost.
    pub fn edge_cost(&mut self, first: i32, second: i32) -> i32 {
        let graph = &self.level.navigator.graph;
        let (Some(start), Some(end)) = (graph.node(first), graph.node(second)) else {
            return Q3_INFINITE;
        };
        let (start, end) = (start.position, end.position);
        let trace = self.trace_bodies(
            start,
            [-8.0; 3],
            [8.0; 3],
            end,
            crate::npc_spawn::ENTITYNUM_NONE,
            MASK_SOLID,
        );
        if trace.fraction < 1.0 || trace.all_solid || trace.start_solid {
            return Q3_INFINITE;
        }
        sjk_nav::distance(start, end) as i32
    }

    /// The player or NPC numbered `number`, as a query holds it.
    pub(crate) fn nav_holder_of(&self, number: u16) -> Option<NavHolder> {
        if self.host.players().iter().any(|body| body.number == number) {
            return Some(NavHolder::Player(number));
        }
        self.actor_at(number).map(NavHolder::Actor)
    }

    /// The entity `NPCInfo->goalEntity` of the NPC at `me` is, as a query holds it.
    pub(crate) fn goal_holder(&self, me: usize) -> Option<NavHolder> {
        if self.goal_is_temp(me) {
            return Some(NavHolder::Goal(me));
        }
        self.nav_holder_of(self.actors[me].mind.goal?)
    }

    /// `NAV_ClearPathToPoint` (`g_nav.c:244-365`) for any entity: whether its box goes
    /// straight to `point` — a client stepping; a navigation goal traced from `point` back
    /// to itself with its owner's box, near enough when the owner would reach it there —
    /// or hits only `ok_to_hit`.
    pub(crate) fn nav_clear_path(
        &mut self,
        entity: &NavEntity,
        mins: [f32; 3],
        maxs: [f32; 3],
        point: [f32; 3],
        clip: u32,
        ok_to_hit: u16,
    ) -> bool {
        if !self.host.in_pvs(entity.origin, point) {
            return false;
        }
        let (mut mins, mut maxs) = (mins, maxs);
        if let Some(owner) = entity.nav_goal_of {
            (mins, maxs) = (self.actors[owner].mins, self.actors[owner].maxs);
        }
        if entity.client || entity.nav_goal_of.is_some() {
            mins[2] = (mins[2] + STEPSIZE).min(maxs[2]);
        }
        let Some(owner) = entity.nav_goal_of else {
            let trace = self.trace_retrying(
                entity.origin,
                mins,
                maxs,
                point,
                entity.number,
                clip | CONTENTS_MONSTERCLIP | CONTENTS_BOTCLIP,
            );
            return (!trace.start_solid && !trace.all_solid && trace.fraction == 1.0)
                || (ok_to_hit != crate::npc_spawn::ENTITYNUM_NONE
                    && trace.entity_number == ok_to_hit);
        };
        let pass = self.actors[owner].number;
        let trace = self.trace_retrying(
            point,
            mins,
            maxs,
            entity.origin,
            pass,
            (clip | CONTENTS_MONSTERCLIP | CONTENTS_BOTCLIP) & !CONTENTS_BODY,
        );
        if trace.start_solid || trace.all_solid {
            return false;
        }
        if trace.fraction == 1.0
            || (ok_to_hit != crate::npc_spawn::ENTITYNUM_NONE && trace.entity_number == ok_to_hit)
        {
            return true;
        }
        let npc = &self.actors[owner];
        // `NPCS.NPCInfo->goalRadius`: the owner's, which is the NPC thinking.
        crate::npc_nav::hit_nav_goal(
            entity.origin,
            npc.mins,
            npc.maxs,
            trace.end_position,
            npc.mind.tactics.goal_radius,
            self.flying(owner),
        )
    }

    /// A trace that, started inside a do-not-enter brush (`trace.contents &
    /// CONTENTS_BOTCLIP`), is made again without them — taken where that one does not
    /// start solid (the traces carry no contents).
    fn trace_retrying(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        pass: u16,
        clip: u32,
    ) -> crate::pmove::MovementTrace {
        let trace = self.trace_bodies(start, mins, maxs, end, pass, clip);
        if trace.start_solid && clip & CONTENTS_BOTCLIP != 0 {
            let again = self.trace_bodies(start, mins, maxs, end, pass, clip & !CONTENTS_BOTCLIP);
            if !again.start_solid {
                return again;
            }
        }
        trace
    }

    /// `TestNodePath`: the entity's own box to `position` through the world (`MASK_SOLID`,
    /// bodies never), hitting `ok_to_hit` allowed.
    fn test_node_path(
        &mut self,
        entity: &NavEntity,
        ok_to_hit: u16,
        position: [f32; 3],
        include_entities: bool,
    ) -> bool {
        let clip = if include_entities {
            MASK_SOLID
        } else {
            MASK_SOLID & !CONTENTS_BODY
        };
        self.nav_clear_path(entity, entity.mins, entity.maxs, position, clip, ok_to_hit)
    }

    /// `TestBestFirst`: the last node, if the entity can still walk to it, or a neighbour
    /// of it nearer the entity that it can walk to.
    fn test_best_first(&mut self, entity: &NavEntity, last: i32) -> i32 {
        let Some(node) = self.level.navigator.graph.node(last) else {
            return NODE_NONE;
        };
        let (position, edges) = (node.position, node.edges.len());
        let mut best =
            if self.test_node_path(entity, crate::npc_spawn::ENTITYNUM_NONE, position, true) {
                last
            } else {
                NODE_NONE
            };
        let mut best_distance = if best == NODE_NONE {
            Q3_INFINITE as f32
        } else {
            distance_squared(entity.origin, position)
        };
        for index in 0..edges {
            let graph = &self.level.navigator.graph;
            let id = graph.nodes()[last as usize].edges[index].node;
            if entity.state.node_failed(id) {
                continue;
            }
            let position = graph.nodes()[id as usize].position;
            let distance = distance_squared(entity.origin, position);
            if distance >= best_distance {
                continue;
            }
            let passed = self.level.navigator.checked(id, entity.number) == CHECKED_PASSED
                || self.test_node_path(entity, crate::npc_spawn::ENTITYNUM_NONE, position, true);
            if passed {
                (best_distance, best) = (distance, id);
            }
            self.level.navigator.set_checked(
                id,
                entity.number,
                if passed {
                    CHECKED_PASSED
                } else {
                    CHECKED_FAILED
                },
            );
        }
        best
    }

    /// `GetNearestNode` (`navigator.cpp:1497-1605`): for `target` none, the last node or a
    /// neighbour of it ([`Self::test_best_first`]); else, among the 16 nodes nearest within
    /// 512, the first the entity stands within (and at its height), or the first it can
    /// walk to — or, with `target`, the one with the cheapest route to it.
    pub(crate) fn nearest_node(
        &mut self,
        holder: NavHolder,
        last: i32,
        flags: i32,
        target: i32,
    ) -> i32 {
        if self.level.navigator.graph.is_empty() {
            return NODE_NONE;
        }
        let entity = self.nav_entity(holder);
        if target == NODE_NONE {
            let best = self.test_best_first(&entity, last);
            if best != NODE_NONE {
                return best;
            }
        }
        let mut near = std::mem::take(&mut self.level.navigator.near);
        self.level.navigator.graph.collect_nearest(
            entity.origin,
            NODE_COLLECT_RADIUS,
            NODE_COLLECT_MAX,
            &mut near,
        );
        let (mut best, mut best_cost) = (NODE_NONE, Q3_INFINITE);
        for candidate in &near {
            let node = &self.level.navigator.graph.nodes()[candidate.node as usize];
            let (position, radius) = (node.position, node.radius);
            if entity.state.node_failed(candidate.node) {
                continue;
            }
            if (candidate.distance as i32) < radius.wrapping_mul(radius)
                && f64::from(position[2] - entity.origin[2]).abs() < MAX_Z_DELTA
            {
                best = candidate.node;
                break;
            }
            if self.level.navigator.checked(candidate.node, entity.number) == CHECKED_FAILED {
                continue;
            }
            if flags & NF_CLEAR_PATH != 0
                && !self.test_node_path(&entity, crate::npc_spawn::ENTITYNUM_NONE, position, false)
            {
                self.level
                    .navigator
                    .set_checked(candidate.node, entity.number, CHECKED_FAILED);
                continue;
            }
            self.level
                .navigator
                .set_checked(candidate.node, entity.number, CHECKED_PASSED);
            if target == NODE_NONE {
                best = candidate.node;
                break;
            }
            let cost = self.level.navigator.graph.path_cost(candidate.node, target);
            if cost < best_cost {
                (best_cost, best) = (cost, candidate.node);
            }
        }
        self.level.navigator.near = near;
        best
    }

    /// Whether node `id` passes for `entity` in `GetBestPathBetweenEnts` (`navigator.cpp:
    /// 1349-1391`, `1452-1488`), caching the answer: not failed for it, and — where the
    /// entity is not within the node's radius at its height — in its PVS and reachable
    /// in a straight line (with `NF_CLEAR_PATH`), hitting `ok_to_hit` allowed.
    fn node_usable(
        &mut self,
        entity: &NavEntity,
        candidate: Candidate,
        flags: i32,
        ok_to_hit: u16,
        include_entities: bool,
    ) -> bool {
        match self.level.navigator.checked(candidate.node, entity.number) {
            CHECKED_FAILED => return false,
            CHECKED_PASSED => return true,
            _ => {}
        }
        if entity.state.node_failed(candidate.node) {
            self.level
                .navigator
                .set_checked(candidate.node, entity.number, CHECKED_FAILED);
            return false;
        }
        let node = &self.level.navigator.graph.nodes()[candidate.node as usize];
        let (position, radius) = (node.position, node.radius);
        let outside = candidate.distance as i32 >= radius.wrapping_mul(radius)
            || f64::from(position[2] - entity.origin[2]).abs() >= MAX_Z_DELTA;
        if outside
            && flags & NF_CLEAR_PATH != 0
            && (!self.host.in_pvs(entity.origin, position)
                || !self.test_node_path(entity, ok_to_hit, position, include_entities))
        {
            self.level
                .navigator
                .set_checked(candidate.node, entity.number, CHECKED_FAILED);
            return false;
        }
        self.level
            .navigator
            .set_checked(candidate.node, entity.number, CHECKED_PASSED);
        true
    }

    /// `GetBestPathBetweenEnts` (`navigator.cpp:1299-1495`) with `d_altRoutes` 0: of the 16
    /// nodes nearest each entity, the pair whose whole way — to the first node, along the
    /// graph, from the last — is cheapest, each node usable by its entity; the entities'
    /// waypoints set to that pair (both none if there is none), and the next node from the
    /// entity's toward the goal's returned.
    pub(crate) fn best_path_between(
        &mut self,
        holder: NavHolder,
        goal_holder: NavHolder,
        flags: i32,
    ) -> i32 {
        if self.level.navigator.graph.is_empty() {
            return NODE_NONE;
        }
        let (entity, goal) = (self.nav_entity(holder), self.nav_entity(goal_holder));
        let mut near = std::mem::take(&mut self.level.navigator.near);
        let mut near_goal = std::mem::take(&mut self.level.navigator.near_goal);
        self.level.navigator.graph.collect_nearest(
            entity.origin,
            NODE_COLLECT_RADIUS,
            NODE_COLLECT_MAX,
            &mut near,
        );
        self.level.navigator.graph.collect_nearest(
            goal.origin,
            NODE_COLLECT_RADIUS,
            NODE_COLLECT_MAX,
            &mut near_goal,
        );
        self.set_nav_waypoint(holder, NODE_NONE);
        self.set_nav_waypoint(goal_holder, NODE_NONE);
        let (mut best_cost, mut ours, mut theirs) = (Q3_INFINITE, NODE_NONE, NODE_NONE);
        for &candidate in &near {
            if !self.node_usable(&entity, candidate, flags, goal.number, true) {
                continue;
            }
            let position = self.level.navigator.graph.nodes()[candidate.node as usize].position;
            for &other in &near_goal {
                let graph = &self.level.navigator.graph;
                let position2 = graph.nodes()[other.node as usize].position;
                let cost = (f64::from(
                    sjk_nav::distance(entity.origin, position)
                        + sjk_nav::distance(goal.origin, position2),
                )
                .floor() as i32)
                    .wrapping_add(graph.path_cost(candidate.node, other.node));
                if cost >= best_cost || !self.node_usable(&goal, other, flags, entity.number, false)
                {
                    continue;
                }
                (best_cost, ours, theirs) = (cost, candidate.node, other.node);
            }
        }
        self.level.navigator.near = near;
        self.level.navigator.near_goal = near_goal;
        self.set_nav_waypoint(holder, ours);
        self.set_nav_waypoint(goal_holder, theirs);
        if ours == NODE_NONE || theirs == NODE_NONE {
            return NODE_NONE;
        }
        self.level.navigator.graph.best_node_alt_route(
            ours,
            theirs,
            &mut best_cost,
            NODE_NONE,
            false,
        )
    }
}
