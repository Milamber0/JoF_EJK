//! Combat points (`codemp/game/NPC_combat.c:2478-2944`): the places a map marks for NPCs
//! to fight from (`point_combat`, `SP_point_combat`), found by what an NPC wants of one —
//! cover from its enemy, a clear shot, nearer, farther, round the enemy's side
//! (`NPC_FindCombatPoint`, `NPC_CollectCombatPoints`) — and held while an NPC goes there
//! (`NPC_SetCombatPoint`, `NPC_ReserveCombatPoint`, `NPC_FreeCombatPoint`).
//!
//! The level keeps at most 512 (`MAX_COMBAT_POINTS`); a map's point past that is refused,
//! as the reference refuses it. A point an NPC must have a route to is one with a route
//! along the waypoints from the NPC's waypoint to the point's, or a straight clear path;
//! "nearest" is the cheapest route, where both have waypoints (else the first found).
//!
//! Held to `tools/game-oracle/npcst.c` (`game-npcst.txt`) and `npcnav.c`.

use crate::npc_senses::{Body, Spot, distance_squared, spot};
use crate::npc_spawn::{ENTITYNUM_NONE, NpcHost};
use crate::npc_world::NpcWorld;

/// `MAX_COMBAT_POINTS`.
pub const MAX_COMBAT_POINTS: usize = 512;
/// `CP_*`: what an NPC wants of a combat point (`b_local.h:252-270`).
pub mod cp {
    pub const ANY: i32 = 0;
    pub const COVER: i32 = 0x1;
    pub const CLEAR: i32 = 0x2;
    pub const FLEE: i32 = 0x4;
    pub const DUCK: i32 = 0x8;
    pub const NEAREST: i32 = 0x10;
    pub const AVOID_ENEMY: i32 = 0x20;
    pub const INVESTIGATE: i32 = 0x40;
    pub const SQUAD: i32 = 0x80;
    pub const AVOID: i32 = 0x100;
    pub const APPROACH_ENEMY: i32 = 0x200;
    pub const CLOSEST: i32 = 0x400;
    pub const FLANK: i32 = 0x800;
    pub const HAS_ROUTE: i32 = 0x1000;
    pub const SAFE: i32 = 0x4000;
    pub const HORZ_DIST_COLL: i32 = 0x8000;
    pub const NO_PVS: i32 = 0x1_0000;
    pub const RETREAT: i32 = 0x2_0000;
}
/// `CPF_*`: what a map says of a point (its `spawnflags`).
pub const CPF_DUCK: i32 = 0x1;
const CPF_FLEE: i32 = 0x2;
const CPF_INVESTIGATE: i32 = 0x4;
const CPF_SQUAD: i32 = 0x8;
/// `MIN_AVOID_DOT`, `MIN_AVOID_DISTANCE_SQUARED`, `CP_COLLECT_RADIUS`.
const MIN_AVOID_DOT: f32 = 0.75;
const MIN_AVOID_DISTANCE_SQUARED: f32 = 128.0 * 128.0;
const CP_COLLECT_RADIUS: f32 = 512.0;
/// `CONTENTS_MONSTERCLIP`, `CONTENTS_BOTCLIP`.
const CONTENTS_MONSTERCLIP: u32 = 0x20;
const CONTENTS_BOTCLIP: u32 = 0x40;
/// `STEPSIZE`.
const STEPSIZE: f32 = 18.0;
/// `WP_THERMAL`.
const WP_THERMAL: i32 = 12;

/// `combatPoint_t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CombatPoint {
    pub origin: [f32; 3],
    /// `CPF_*`.
    pub flags: i32,
    pub occupied: bool,
    /// Its nearest waypoint (`CP_FindCombatPointWaypoints`; 0 until looked for).
    pub waypoint: i32,
}

/// `SP_point_combat` (`NPC_combat.c:2478-2508`): the map's point, an eighth of a unit up,
/// or `None` past the level's 512.
pub fn point_combat(entity: &sjk_entity::Entity, points: &[CombatPoint]) -> Option<CombatPoint> {
    if points.len() >= MAX_COMBAT_POINTS {
        return None;
    }
    let mut origin = [0.0_f32; 3];
    if let Some(text) = entity.get("origin") {
        for (axis, word) in text.split_whitespace().take(3).enumerate() {
            origin[axis] = word.parse().unwrap_or(0.0);
        }
    }
    origin[2] += 0.125;
    let flags = entity
        .get("spawnflags")
        .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()));
    Some(CombatPoint {
        origin,
        flags,
        occupied: false,
        waypoint: 0,
    })
}

/// `DistanceHorizontalSquared`.
fn horizontal_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (x, y) = (a[0] - b[0], a[1] - b[1]);
    x * x + y * y
}

/// `VectorNormalize` of `a - b`.
fn direction(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    let mut d = crate::npc_senses::subtract(a, b);
    crate::player_angle_math::normalize(&mut d);
    d
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// What `NPC_FindCombatPoint` is asked: where the NPC is, where the enemy is, the `CP_*`
/// it wants, how far to keep away and a point to leave out. (Its `avoidPosition` is never
/// read: `CP_AVOID` keeps away from `position`.)
#[derive(Clone, Copy, Debug)]
pub struct PointSearch {
    pub position: [f32; 3],
    pub enemy: [f32; 3],
    pub flags: i32,
    pub avoid_distance: f32,
    pub ignore: i32,
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_FindCombatPoint` (`NPC_combat.c:2616-2839`) for the NPC at `me`: the first point
    /// (in the level's order) within 512 units of the enemy position that is free and has
    /// everything asked, or -1.
    pub fn find_combat_point(&mut self, me: usize, search: PointSearch) -> i32 {
        let PointSearch {
            enemy,
            flags,
            avoid_distance,
            ignore,
            ..
        } = search;
        let avoid_squared = if avoid_distance <= 0.0 {
            MIN_AVOID_DISTANCE_SQUARED
        } else {
            avoid_distance * avoid_distance
        };
        // The NPC's waypoint, for the route tests (`NAV_GetNearestNode` from its last one
        // when it holds none).
        let mut waypoint = crate::npc_mind::WAYPOINT_NONE;
        if flags & (cp::HAS_ROUTE | cp::NEAREST) != 0 {
            let tactics = self.actors[me].mind.tactics;
            waypoint = if tactics.waypoint == crate::npc_mind::WAYPOINT_NONE {
                self.nearest_node(
                    crate::npc_navigator::NavHolder::Actor(me),
                    tactics.last_waypoint,
                    crate::npc_navigator::NF_CLEAR_PATH,
                    crate::npc_mind::WAYPOINT_NONE,
                )
            } else {
                tactics.waypoint
            };
        }
        let (mut best, mut best_cost) = (-1, crate::npc_navigator::Q3_INFINITE);
        let radius = if flags & cp::NO_PVS != 0 {
            CP_COLLECT_RADIUS * 4.0
        } else {
            CP_COLLECT_RADIUS
        };
        let radius_squared = radius * radius;
        let enemy_body = self.actors[me]
            .mind
            .enemy
            .and_then(|number| self.body(number));
        for index in 0..self.level.combat_points.len() {
            let point = self.level.combat_points[index];
            // `NPC_CollectCombatPoints` (`NPC_combat.c:2536-2610`).
            if point.occupied
                || (flags & cp::DUCK != 0 && point.flags & CPF_DUCK != 0)
                || (flags & cp::FLEE != 0 && point.flags & CPF_FLEE != 0)
                || (flags & cp::INVESTIGATE != 0 && point.flags & CPF_INVESTIGATE != 0)
                || (point.flags & CPF_SQUAD != 0 && flags & cp::SQUAD == 0)
                || (flags & cp::NO_PVS != 0 && self.host.in_pvs(enemy, point.origin))
            {
                continue;
            }
            let collected = if flags & cp::HORZ_DIST_COLL != 0 {
                horizontal_squared(enemy, point.origin)
            } else {
                distance_squared(enemy, point.origin)
            };
            if collected >= radius_squared || index as i32 == ignore {
                continue;
            }
            if !self.point_suits(
                me,
                point,
                search,
                collected,
                avoid_squared,
                enemy_body,
                waypoint,
            ) {
                continue;
            }
            // `CP_NEAREST`: the cheapest route from the NPC's waypoint, where both have one.
            if flags & cp::NEAREST != 0
                && waypoint != crate::npc_mind::WAYPOINT_NONE
                && point.waypoint != crate::npc_mind::WAYPOINT_NONE
            {
                let cost = self
                    .level
                    .navigator
                    .graph
                    .path_cost(waypoint, point.waypoint);
                if cost < best_cost {
                    (best_cost, best) = (cost, index as i32);
                }
                continue;
            }
            return index as i32;
        }
        best
    }

    /// `NPC_FindCombatPoint`'s tests of one collected point (`NPC_combat.c:2667-2828`).
    #[allow(clippy::too_many_arguments)]
    fn point_suits(
        &mut self,
        me: usize,
        point: CombatPoint,
        search: PointSearch,
        collected: f32,
        avoid_squared: f32,
        enemy_body: Option<Body>,
        waypoint: i32,
    ) -> bool {
        let PointSearch {
            position,
            enemy,
            flags,
            ..
        } = search;
        let origin = point.origin;
        if flags & cp::COVER != 0 && crate::npc_senses::clear_los(&mut self.senses(), origin, enemy)
        {
            return false;
        }
        if flags & cp::CLEAR != 0 {
            let Some(target) = enemy_body else {
                return false;
            };
            if !self.clear_los_to(origin, &target) {
                return false;
            }
            let distance = if self.npc(me).weapon == WP_THERMAL {
                horizontal_squared(origin, target.origin)
            } else {
                distance_squared(origin, target.origin)
            };
            let visrange = self.actors[me].definition.stats.visrange;
            if distance > visrange * visrange {
                return false;
            }
        }
        if flags & cp::AVOID != 0 && distance_squared(origin, position) < avoid_squared {
            return false;
        }
        let mine = if flags & cp::HORZ_DIST_COLL != 0 {
            horizontal_squared(position, enemy)
        } else {
            distance_squared(position, enemy)
        };
        if flags & cp::APPROACH_ENEMY != 0 && collected > mine {
            return false;
        }
        if flags & cp::RETREAT != 0 && collected < mine {
            return false;
        }
        if flags & cp::FLANK != 0
            && f64::from(dot(direction(position, enemy), direction(origin, enemy))) >= 0.4
        {
            return false;
        }
        if flags & cp::AVOID_ENEMY != 0 {
            let (enemy_dir, goal_dir) = (direction(position, enemy), direction(position, origin));
            if dot(goal_dir, enemy_dir) >= MIN_AVOID_DOT
                || distance_squared(origin, enemy) < avoid_squared
            {
                return false;
            }
        }
        let npc = &self.actors[me];
        let (mins, maxs, number, clip) = (npc.mins, npc.maxs, npc.number, npc.clip_mask);
        let trace = self.trace_bodies(origin, mins, maxs, origin, number, clip);
        if trace.all_solid || trace.start_solid {
            return false;
        }
        // `CP_HAS_ROUTE`: a route along the waypoints, else a straight clear path.
        if flags & cp::HAS_ROUTE == 0
            || (waypoint != crate::npc_mind::WAYPOINT_NONE
                && point.waypoint != crate::npc_mind::WAYPOINT_NONE
                && {
                    let mut cost = 0;
                    self.level.navigator.graph.best_node_alt_route(
                        waypoint,
                        point.waypoint,
                        &mut cost,
                        sjk_nav::NODE_NONE,
                        false,
                    ) != sjk_nav::NODE_NONE
                })
        {
            return true;
        }
        self.clear_path_to_point(me, mins, maxs, origin, clip, ENTITYNUM_NONE)
    }

    /// `G_ClearLOS3` (`NPC_senses.c:790-808`): a clear line from `start` to the body's
    /// origin, or to its leaning head.
    pub fn clear_los_to(&mut self, start: [f32; 3], target: &Body) -> bool {
        let mut senses = self.senses();
        crate::npc_senses::clear_los(&mut senses, start, spot(target, Spot::Origin))
            || crate::npc_senses::clear_los(&mut senses, start, spot(target, Spot::HeadLean))
    }

    /// `NAV_ClearPathToPoint` (`g_nav.c:238-367`) for the NPC at `me` (never a navigation
    /// goal): whether its box, stepping, goes straight to `point` — or hits only
    /// `ok_to_hit`.
    pub fn clear_path_to_point(
        &mut self,
        me: usize,
        mins: [f32; 3],
        maxs: [f32; 3],
        point: [f32; 3],
        clip: u32,
        ok_to_hit: u16,
    ) -> bool {
        let npc = &self.actors[me];
        let (origin, number) = (npc.current_origin, npc.number);
        if !self.host.in_pvs(origin, point) {
            return false;
        }
        let mut mins = mins;
        mins[2] = (mins[2] + STEPSIZE).min(maxs[2]);
        let mut trace = self.trace_bodies(
            origin,
            mins,
            maxs,
            point,
            number,
            clip | CONTENTS_MONSTERCLIP | CONTENTS_BOTCLIP,
        );
        if trace.start_solid {
            // `trace.contents & CONTENTS_BOTCLIP`: started inside a do-not-enter brush,
            // traced again without them.
            let again = self.trace_bodies(
                origin,
                mins,
                maxs,
                point,
                number,
                clip | CONTENTS_MONSTERCLIP,
            );
            if !again.start_solid {
                trace = again;
            }
        }
        if !trace.start_solid && !trace.all_solid && trace.fraction == 1.0 {
            return true;
        }
        ok_to_hit != ENTITYNUM_NONE && trace.entity_number == ok_to_hit
    }

    /// `NPC_ReserveCombatPoint` (`NPC_combat.c:2882-2899`).
    fn reserve_combat_point(&mut self, point: i32) -> bool {
        match usize::try_from(point)
            .ok()
            .and_then(|at| self.level.combat_points.get_mut(at))
        {
            Some(slot) if !slot.occupied => {
                slot.occupied = true;
                true
            }
            _ => false,
        }
    }

    /// `NPC_FreeCombatPoint` (`NPC_combat.c:2904-2925`) for the NPC at `me`: the point let
    /// go (and remembered as failed, if it did).
    pub fn free_combat_point(&mut self, me: usize, point: i32, failed: bool) -> bool {
        if failed {
            self.actors[me].mind.tactics.last_failed_combat_point = point;
        }
        match usize::try_from(point)
            .ok()
            .and_then(|at| self.level.combat_points.get_mut(at))
        {
            Some(slot) if slot.occupied => {
                slot.occupied = false;
                true
            }
            _ => false,
        }
    }

    /// `NPC_SetCombatPoint` (`NPC_combat.c:2930-2944`).
    pub fn set_combat_point(&mut self, me: usize, point: i32) -> bool {
        let held = self.actors[me].mind.tactics.combat_point;
        if held != -1 {
            self.free_combat_point(me, held, false);
        }
        if !self.reserve_combat_point(point) {
            return false;
        }
        self.actors[me].mind.tactics.combat_point = point;
        true
    }
}
