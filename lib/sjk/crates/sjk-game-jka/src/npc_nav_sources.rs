//! Navigation for levels that ship none: an SJK server option (`g_npcNav`), not the
//! reference's. The stock multiplayer maps carry no usable NPC navigation (their `.nav`
//! files are other maps' graphs or empty, and they place no waypoints), so the reference's NPCs mostly stand and shoot.
//! With the option on, a level without navigation of its own is given a graph made from
//! other data, and combat points at cover among its nodes; the ported navigator
//! (`NAVNEW_MoveToGoal`, the routes, `NPC_FindCombatPoint`) then runs on them unchanged.
//!
//! - [`NavSource::BotRoutes`]: the bots' route file (`botroutes/<map>.wnt`,
//!   [`crate::bot_routes`]) — its trail and its neighbour links, kept where an NPC's box
//!   walks them (a bot's Force jump is no NPC's way);
//! - [`NavSource::Collision`]: the route file where there is one, else the map's floor
//!   sampled against an NPC's box ([`sjk_nav::walk::flood_lattice`]) from places players
//!   are known to stand.
//!
//! The made graph goes where the map's waypoints would: its nodes given the clear radius
//! `SP_waypoint` measures, its links flagged blocked where a mover stands in them
//! (`HardConnect`), and — as the level loads, where `NAV_CheckCalcPaths` does it for
//! waypoints 400 ms in (a level with no NPC yet runs no NPC frame to do it in) — locked
//! doors and breakables failed, ranks calculated, combat points given their waypoints. Links run one way where the way down is a drop, so the graph is
//! directed ([`sjk_nav::Graph::set_directed`]).
//!
//! The numbers here are this game's: an NPC's standing box, `STEPSIZE`, `MIN_WALK_NORMAL`,
//! the view heights. `g_npcNav 0` changes nothing.

use crate::npc_combat_points::{CPF_DUCK, CombatPoint, MAX_COMBAT_POINTS};
use crate::npc_navigator::{CONTENTS_BOTCLIP, CONTENTS_MONSTERCLIP, MASK_SOLID, new_graph};
use crate::npc_spawn::{ENTITYNUM_NONE, NpcHost};
use crate::pmove::MovementCollision;
use sjk_nav::cover::{CoverRules, find_cover};
use sjk_nav::walk::{Lattice, Sweep, Walker, flood_lattice, walked_links};
pub use sjk_nav::walk::{SweepWorld, Walkways};

/// Where a level without navigation of its own gets it (`g_npcNav`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavSource {
    /// 0: the map's own navigation only, as the reference.
    Stock,
    /// 1: the bots' route file.
    BotRoutes,
    /// 2: the bots' route file, else the map's collision.
    Collision,
}

impl NavSource {
    /// The option's value: 1 and 2 as named, anything else stock.
    pub fn from_setting(value: i32) -> Self {
        match value {
            1 => Self::BotRoutes,
            2 => Self::Collision,
            _ => Self::Stock,
        }
    }
}

/// What made navigation walks against: `MASK_NPCSOLID` without bodies, and bot clip as
/// the reference's waypoints are connected against it.
pub const WALK_MASK: u32 = MASK_SOLID | CONTENTS_MONSTERCLIP | CONTENTS_BOTCLIP;

/// An NPC's standing box, `STEPSIZE`, `MIN_WALK_NORMAL`; a drop it lands from unhurt; no
/// jumps (the ported walkers never press jump on a route).
pub const WALKER: Walker = Walker {
    mins: [-15.0, -15.0, -24.0],
    maxs: [15.0, 15.0, 40.0],
    step_height: 18.0,
    max_drop: 96.0,
    jump_height: 0.0,
    min_floor_normal: 0.7,
    stride: 16.0,
};

/// The floor lattice: a node every 96 units, 1536 at most (ranks are nodes squared).
pub const LATTICE: Lattice = Lattice {
    spacing: 96.0,
    max_nodes: 1_536,
    merge_height: 48.0,
};
/// The coarser spacings a map too large for [`LATTICE`]'s nodes is sampled at instead.
const COARSER: [f32; 2] = [136.0, 192.0];

/// Cover: a wall within 48 units at crouching eyes (`CROUCH_VIEWHEIGHT` 12), low where
/// standing eyes (`DEFAULT_VIEWHEIGHT` 36) clear it; spots slid up to 128 units from a
/// node to its nearest wall, in a box a unit wider than the marker that finds a combat
/// point's waypoint (and lifted a step); threats 128 to 1024 away; a point every 128
/// units, 256 at most.
pub const COVER: CoverRules = CoverRules {
    wall_reach: 48.0,
    slide: 128.0,
    slide_mins: [-17.0, -17.0, -24.0 + 18.0],
    slide_maxs: [17.0, 17.0, 40.0],
    crouch_eye: 12.0,
    stand_eye: 36.0,
    near_threat: 128.0,
    far_threat: 1_024.0,
    threat_samples: 24,
    spacing: 128.0,
    max_points: 256,
};

/// `WPFLAG_ONEWAY_FWD`, `WPFLAG_ONEWAY_BACK` (`ai_main.h`).
const WPFLAG_ONEWAY_FWD: i32 = 0x4000;
const WPFLAG_ONEWAY_BACK: i32 = 0x8000;
/// How near in height a walk must end to the waypoint it went for.
const ARRIVAL_TOLERANCE: f32 = 24.0;
/// The share of a loaded graph's nodes that must stand on this map for it to be the map's.
const OWN_SHARE: f32 = 0.75;
/// `wpMins`, `wpMaxs` (`navigator.cpp:59-60`): the box an edge is traced with.
const WP_MINS: [f32; 3] = [-16.0, -16.0, -24.0 + 18.0];
const WP_MAXS: [f32; 3] = [16.0, 16.0, 32.0];

/// A world's collision as the made navigation sweeps it: `mask` (usually [`WALK_MASK`])
/// against whatever `collision` holds — for a server, the map without its movers, which
/// the made graph's links are flagged by instead ([`crate::npc_roster::NpcRoster::adopt_navigation`]).
pub struct CollisionSweeps<'a, C: MovementCollision> {
    pub collision: &'a C,
    pub mask: u32,
}

impl<C: MovementCollision> SweepWorld for CollisionSweeps<'_, C> {
    fn sweep(&mut self, start: [f32; 3], mins: [f32; 3], maxs: [f32; 3], end: [f32; 3]) -> Sweep {
        let trace = self.collision.trace(start, mins, maxs, end, self.mask);
        Sweep {
            fraction: trace.fraction,
            end: trace.end_position,
            normal: trace.plane_normal,
            start_solid: trace.start_solid || trace.all_solid,
        }
    }
}

/// The bots' waypoints as walkways: every waypoint a point; its trail to the next and
/// back (`TotalTrailDistance`'s one-way rules) and its neighbours that are not Force jumps
/// as candidate links, kept where an NPC walks them.
pub fn bot_route_walkways(
    routes: &crate::bot_routes::BotRoutes,
    world: &mut impl SweepWorld,
) -> Walkways {
    let points: Vec<[f32; 3]> = routes
        .waypoints
        .iter()
        .map(|waypoint| waypoint.origin)
        .collect();
    let mut candidates = Vec::new();
    for (index, waypoint) in routes.waypoints.iter().enumerate() {
        let here = index as u32;
        if index + 1 < routes.waypoints.len() {
            if waypoint.flags & WPFLAG_ONEWAY_BACK == 0 {
                candidates.push((here, here + 1));
            }
            if waypoint.flags & WPFLAG_ONEWAY_FWD == 0 {
                candidates.push((here + 1, here));
            }
        }
        for neighbour in &waypoint.neighbours {
            if neighbour.force_jump_to == 0
                && usize::try_from(neighbour.num).is_ok_and(|num| num < points.len())
            {
                candidates.push((here, neighbour.num as u32));
            }
        }
    }
    let links = walked_links(world, &WALKER, &points, &candidates, ARRIVAL_TOLERANCE);
    Walkways { points, links }
}

/// The map's floor as walkways, flooded from `seeds` (its spawn points, its items): at
/// [`LATTICE`]'s spacing, or — where that runs out of nodes before the floor does — at a
/// coarser one, the coarsest kept as it is.
pub fn collision_walkways(world: &mut impl SweepWorld, seeds: &[[f32; 3]]) -> Walkways {
    let mut ways = flood_lattice(world, &WALKER, &LATTICE, seeds);
    for spacing in COARSER {
        if ways.points.len() < LATTICE.max_nodes {
            break;
        }
        ways = flood_lattice(world, &WALKER, &Lattice { spacing, ..LATTICE }, seeds);
    }
    ways
}

impl crate::npc_roster::NpcRoster {
    /// Whether the level has navigation of its own: waypoints of the map's to connect, or
    /// a graph read from its `.nav` file most of whose nodes stand on this map's floor
    /// (the stock files are mostly other maps' graphs).
    pub fn has_own_navigation(&self, world: &mut impl SweepWorld) -> bool {
        let navigator = &self.level.navigator;
        if navigator.calculating {
            return !navigator.stored.is_empty();
        }
        let nodes = navigator.graph.nodes();
        if nodes.is_empty() {
            return false;
        }
        let standing = nodes
            .iter()
            .filter(|node| WALKER.floor_below(world, node.position, 64.0).is_some())
            .count();
        standing as f32 >= nodes.len() as f32 * OWN_SHARE
    }

    /// The level's navigation replaced by `ways`: each point a node with the clear radius
    /// `SP_waypoint` measures, each link a one-way connection flagged blocked where the
    /// host's world (its movers too) stands in it; combat points at cover among the nodes
    /// ([`COVER`], `CPF_DUCK` where the cover is low) for a map that places none, as many
    /// as the level's 512 leave room for; then, at once, what `NAV_CheckCalcPaths` does for
    /// waypoints 400 ms in — locked doors and breakables failed, ranks calculated, combat
    /// points given their waypoints. How many combat points were added.
    pub fn adopt_navigation(
        &mut self,
        ways: &Walkways,
        world: &mut impl SweepWorld,
        level_time: i32,
        host: &mut impl NpcHost,
    ) -> usize {
        let navigator = &mut self.level.navigator;
        navigator.graph = new_graph();
        navigator.graph.set_directed(true);
        navigator.stored.clear();
        for &point in &ways.points {
            let radius = crate::npc_nav_setup::waypoint_radius(host, point) as i32;
            navigator.graph.add_node(point, 0, radius);
        }
        for link in &ways.links {
            let (from, to) = (
                ways.points[link.from as usize],
                ways.points[link.to as usize],
            );
            let trace = host.trace(
                from,
                WP_MINS,
                WP_MAXS,
                to,
                ENTITYNUM_NONE,
                MASK_SOLID | CONTENTS_BOTCLIP | CONTENTS_MONSTERCLIP,
                &[],
            );
            let blocked = trace.fraction != 1.0 || trace.start_solid || trace.all_solid;
            let flags = link.flags | if blocked { sjk_nav::EDGE_BLOCKED } else { 0 };
            navigator
                .graph
                .link(link.from as i32, link.to as i32, flags);
        }
        navigator.calculating = true;
        let added = self.add_cover_points(world);
        let mut fired = crate::npc_roster::Fired::new();
        self.world(level_time, host, &mut fired).calc_paths();
        // A made point the navigator finds no waypoint for (no node it has a clear way from)
        // would be reached only by a straight way: dropped.
        if added > 0 {
            self.level
                .combat_points
                .retain(|point| point.waypoint != crate::npc_mind::WAYPOINT_NONE);
        }
        self.level.combat_points.len().min(added)
    }

    /// Combat points at cover among the level's nodes, for a map that places none of its
    /// own. How many were added.
    fn add_cover_points(&mut self, world: &mut impl SweepWorld) -> usize {
        if !self.level.combat_points.is_empty() {
            return 0;
        }
        let points: Vec<[f32; 3]> = self
            .level
            .navigator
            .graph
            .nodes()
            .iter()
            .map(|node| node.position)
            .collect();
        let rules = CoverRules {
            max_points: COVER.max_points.min(MAX_COMBAT_POINTS),
            ..COVER
        };
        let covers = find_cover(world, &points, &rules);
        for cover in &covers {
            let flags = if cover.low { CPF_DUCK } else { 0 };
            self.level.combat_points.push(CombatPoint {
                origin: cover.position,
                flags,
                occupied: false,
                waypoint: 0,
            });
        }
        covers.len()
    }
}
