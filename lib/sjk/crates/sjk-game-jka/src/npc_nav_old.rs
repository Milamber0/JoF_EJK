//! The older navigator's way to a goal (`NPC_GetMoveDirection`, `codemp/game/NPC_move.c:180-245`)
//! which the flying machines and Boba Fett in flight take toward an enemy they cannot see
//! (`Seeker_Hunt`, `ImperialProbe_Hunt`, `Sentry_Hunt`, `Remote_Hunt`, `Interrogator_Hunt`):
//! straight at the goal where the world lets it (`NPC_ClearPathToGoal`), else along the
//! waypoints (`NAV_MoveToGoal`, `g_nav.c:1135-1235`); round a body in the way
//! (`NAV_AvoidCollision`, `924-987`: `NAV_TestForBlocked`, `NAV_ResolveEntityCollision`,
//! `NAV_StackedCanyon`, `NAV_Bypass`, `NAV_TestBypass`, `NAV_ResolveBlock`, `577-918`).
//!
//! Unlike [`crate::npc_nav`]'s (`NAVNEW_*`), this one never shoves a blocker aside or keeps
//! count of blocked moves, and it finds its route afresh every time.

use crate::npc_mind::WAYPOINT_NONE;
use crate::npc_nav::{NIF_BLOCKED, NIF_COLLISION, NIF_MACRO_NAV, NavInfo};
use crate::npc_navigator::{NF_CLEAR_PATH, NavHolder};
use crate::npc_senses::{Body, distance_squared, subtract};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use sjk_protocol::UserCommand;

/// `MAX_COLL_AVOID_DIST`, `MIN_DOOR_BLOCK_DIST_SQR` (`g_nav.h:32-38`).
const MAX_COLL_AVOID_DIST: f32 = 128.0;
const MIN_DOOR_BLOCK_DIST_SQR: f32 = 16.0 * 16.0;
/// `NPCAI_BLOCKED`, `NPCAI_NO_COLL_AVOID`.
const NPCAI_BLOCKED: u32 = 0x40;
const NPCAI_NO_COLL_AVOID: u32 = 0x20;
/// `CONTENTS_BODY`, `CONTENTS_BOTCLIP`.
const CONTENTS_BODY: u32 = 0x100;
const CONTENTS_BOTCLIP: u32 = 0x40;
/// `MAX_CLIENTS`; `ENTITYNUM_WORLD`.
const MAX_CLIENTS: u16 = 32;
const ENTITYNUM_WORLD: u16 = 1_022;

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `VectorMA(start, scale, direction)`.
fn along(start: [f32; 3], scale: f32, direction: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| start[axis] + scale * direction[axis])
}

/// `sqrt(maxs[0]² + maxs[1]²)`: a box's reach on the flat, in double.
fn flat_reach(maxs: [f32; 3]) -> f64 {
    f64::from(maxs[0] * maxs[0] + maxs[1] * maxs[1]).sqrt()
}

/// `PerpendicularVector` (`q_math.c`): the unit axis `source` is least along, projected onto
/// the plane `source` is the normal of (`ProjectPointOnPlane`), normalised.
fn perpendicular_vector(source: [f32; 3]) -> [f32; 3] {
    let mut position = 0;
    let mut least = 1.0_f32;
    for (axis, value) in source.iter().enumerate() {
        if f64::from(*value).abs() < f64::from(least) {
            position = axis;
            least = value.abs();
        }
    }
    let mut unit = [0.0_f32; 3];
    unit[position] = 1.0;
    let inverse = 1.0 / dot(source, source);
    let d = dot(source, unit) * inverse;
    let normal = source.map(|axis| axis * inverse);
    let mut out: [f32; 3] = std::array::from_fn(|axis| unit[axis] - d * normal[axis]);
    crate::player_angle_math::normalize(&mut out);
    out
}

/// `CrossProduct`.
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_GetMoveDirection` (`NPC_move.c:180-245`): the way toward the NPC's goal and how
    /// far it is — straight, round a body, or along the waypoints — or `None`, having turned
    /// to face the goal, where there is none. The move is kept as the last
    /// (`NAV_GetLastMove`).
    pub(crate) fn get_move_direction(
        &mut self,
        me: usize,
        command: &mut UserCommand,
    ) -> Option<([f32; 3], f32)> {
        self.level.nav = NavInfo::default();
        let goal = self.goal_origin(me)?;
        let mut direction = subtract(goal, self.actors[me].current_origin);
        let distance = crate::saber_clash::normalize(&mut direction);
        self.actors[me].mind.tactics.blocked_dest = goal;
        let mut info = NavInfo {
            direction,
            path_direction: direction,
            distance,
            flags: 0,
            blocker: None,
        };
        if !self.clear_path_to_goal(me, goal) {
            if self.nav_move_to_goal(me, &mut info) == WAYPOINT_NONE {
                return self.face_nav_direction(me, info);
            }
            info.flags |= NIF_MACRO_NAV;
        }
        if !self.nav_avoid_collision(me, &mut info, command) && info.flags & NIF_MACRO_NAV == 0 {
            if self.nav_move_to_goal(me, &mut info) == WAYPOINT_NONE {
                return self.face_nav_direction(me, info);
            }
            info.flags |= NIF_MACRO_NAV;
        }
        self.level.nav = info;
        Some((info.direction, info.distance))
    }

    /// "Can't reach goal, just face": the move kept, and its yaw desired.
    fn face_nav_direction(&mut self, me: usize, info: NavInfo) -> Option<([f32; 3], f32)> {
        self.level.nav = info;
        let angles = crate::player_angle_math::vector_angles(info.direction);
        self.actors[me].desired_yaw = crate::npc_droid::angle_normalize360(angles[1]);
        None
    }

    /// `NAV_MoveToGoal` (`g_nav.c:1135-1235`): the next node of the route from the NPC's
    /// waypoint to its goal's (a player's as the frame found it; anything else's looked up
    /// now), checked against the world; the move toward it — or, the world in the way, toward
    /// the nearest point of the edge back to its own waypoint, or that waypoint itself.
    /// `WAYPOINT_NONE` where either has no waypoint or there is no route.
    fn nav_move_to_goal(&mut self, me: usize, info: &mut NavInfo) -> i32 {
        let Some(goal) = self.goal_holder(me) else {
            return WAYPOINT_NONE;
        };
        let goal_waypoint = match goal {
            NavHolder::Player(_) => self.nav_entity(goal).state.waypoint,
            _ => {
                let last = self.nav_entity(goal).state.waypoint;
                let found = self.nearest_node(goal, last, NF_CLEAR_PATH, WAYPOINT_NONE);
                self.set_nav_waypoint(goal, found);
                found
            }
        };
        if goal_waypoint == WAYPOINT_NONE {
            return WAYPOINT_NONE;
        }
        let last = self.actors[me].mind.tactics.last_waypoint;
        let waypoint = self.nearest_node(NavHolder::Actor(me), last, NF_CLEAR_PATH, WAYPOINT_NONE);
        self.actors[me].mind.tactics.waypoint = waypoint;
        if waypoint == WAYPOINT_NONE {
            return WAYPOINT_NONE;
        }
        let mut best = self
            .level
            .navigator
            .graph
            .best_node(waypoint, goal_waypoint, WAYPOINT_NONE);
        if best == WAYPOINT_NONE {
            return WAYPOINT_NONE;
        }
        best = self.test_best_node(me, best, goal_waypoint, false);
        let mut origin = self.node_position(best).unwrap_or_default();
        let end = self.node_position(waypoint).unwrap_or_default();
        let clip = (self.actors[me].clip_mask & !CONTENTS_BODY) | CONTENTS_BOTCLIP;
        if !self.check_ahead(me, origin, clip).0 {
            origin = crate::npc_jedi_block::closest_point_on_segment(
                origin,
                end,
                self.actors[me].current_origin,
            );
            if !self.check_ahead(me, origin, clip).0 {
                best = waypoint;
                origin = self.node_position(best).unwrap_or_default();
            }
        }
        let mut direction = subtract(origin, self.actors[me].current_origin);
        info.distance = crate::saber_clash::normalize(&mut direction);
        info.direction = direction;
        let mut path = subtract(end, origin);
        crate::saber_clash::normalize(&mut path);
        info.path_direction = path;
        best
    }

    /// `NAV_AvoidCollision` (`g_nav.c:924-987`): the NPC's blocking forgotten; a body within
    /// 128 units along the move met — its goal may be walked into; one standing on the goal
    /// near stops it; else it goes round (`info.direction` turned). Whether it can go on.
    fn nav_avoid_collision(
        &mut self,
        me: usize,
        info: &mut NavInfo,
        command: &mut UserCommand,
    ) -> bool {
        let npc = &mut self.actors[me];
        npc.ai_flags &= !NPCAI_BLOCKED;
        npc.mind.tactics.blocking_ent_num = i32::from(ENTITYNUM_WORLD);
        info.distance = info.distance.min(MAX_COLL_AVOID_DIST);
        let end = along(npc.current_origin, info.distance, info.direction);
        let mut movedir = info.direction;
        if npc.ai_flags & NPCAI_NO_COLL_AVOID != 0 {
            return true;
        }
        let (clear, trace) = self.check_ahead(me, end, CONTENTS_BODY);
        if clear {
            return true;
        }
        info.blocker = Some(trace.entity_number);
        info.flags |= NIF_COLLISION;
        if self.actors[me].mind.goal == Some(trace.entity_number) {
            return true;
        }
        let blocker = self.blocker_body(trace.entity_number);
        if self.test_for_blocked(me, &blocker, info.distance, &mut info.flags, command) {
            return false;
        }
        if info.flags & NIF_BLOCKED != 0 {
            return true;
        }
        if !self.nav_resolve_entity_collision(
            me,
            &blocker,
            &mut movedir,
            info.path_direction,
            command,
        ) {
            return false;
        }
        info.direction = movedir;
        true
    }

    /// `NAV_ResolveEntityCollision` (`g_nav.c:846-889`): an unlocked door not right at hand
    /// passed; stuck between a player and the world, it stops (`NAV_StackedCanyon`); else
    /// round the blocker (`NAV_Bypass`), or it waits on one that waits on it
    /// (`NAV_ResolveBlock`).
    fn nav_resolve_entity_collision(
        &mut self,
        me: usize,
        blocker: &Body,
        movedir: &mut [f32; 3],
        path: [f32; 3],
        command: &mut UserCommand,
    ) -> bool {
        let origin = self.actors[me].current_origin;
        if self.host.door_center(blocker.number).is_some()
            && distance_squared(origin, blocker.origin) > MIN_DOOR_BLOCK_DIST_SQR
        {
            return true;
        }
        let mut blocked_dir = subtract(blocker.origin, origin);
        let blocked_dist = crate::saber_clash::normalize(&mut blocked_dir);
        if blocker.number < MAX_CLIENTS && self.nav_stacked_canyon(me, blocker, path) {
            self.npc_blocked(me, blocker);
            let spot = crate::npc_senses::spot(blocker, crate::npc_senses::Spot::HeadLean);
            self.face_position(me, spot, true, command);
            return false;
        }
        if self.nav_bypass(me, blocker, blocked_dir, blocked_dist, movedir) {
            return true;
        }
        // `NAV_ResolveBlock`: through a blocker that waits on this NPC, else a complaint.
        if self.actor_at(blocker.number).is_some_and(|at| {
            self.actors[at].mind.tactics.blocking_ent_num == i32::from(self.actors[me].number)
        }) {
            return true;
        }
        self.npc_blocked(me, blocker);
        let spot = crate::npc_senses::spot(blocker, crate::npc_senses::Spot::HeadLean);
        self.face_position(me, spot, true, command);
        false
    }

    /// `NAV_StackedCanyon` (`g_nav.c:780-840`): whether the NPC's box, set beside the blocker
    /// on either side across the path, is in the world both ways.
    fn nav_stacked_canyon(&mut self, me: usize, blocker: &Body, path: [f32; 3]) -> bool {
        let across = cross(path, perpendicular_vector(path));
        let npc = &self.actors[me];
        let reach = (flat_reach(blocker.maxs) + flat_reach(npc.maxs)) as f32;
        let (mins, maxs, number, clip) = (
            npc.mins,
            npc.maxs,
            npc.number,
            npc.clip_mask | CONTENTS_BOTCLIP,
        );
        for side in [reach, -reach] {
            let test = along(blocker.origin, side, across);
            let mut trace = self.trace_bodies(test, mins, maxs, test, number, clip);
            if trace.start_solid {
                // Inside a do-not-enter brush: tried again without them.
                let again =
                    self.trace_bodies(test, mins, maxs, test, number, clip & !CONTENTS_BOTCLIP);
                if !again.start_solid {
                    trace = again;
                }
            }
            if !trace.start_solid && !trace.all_solid {
                return false;
            }
        }
        true
    }

    /// `NAV_Bypass` (`g_nav.c:611-685`): opposite a blocker moving across its way, else round
    /// it by an arc (wider the nearer it is) on the side it faces, half the arc, the other
    /// side, half that: the first way clear of the world (`NAV_TestBypass`).
    fn nav_bypass(
        &mut self,
        me: usize,
        blocker: &Body,
        blocked_dir: [f32; 3],
        blocked_dist: f32,
        movedir: &mut [f32; 3],
    ) -> bool {
        let npc = &self.actors[me];
        let right = crate::pmove::flight::flight_axes(npc.mind.current_angles)
            .1
            .to_array();
        let yaw = crate::npc_nav::vector_to_yaw(blocked_dir);
        let avoid = (flat_reach(blocker.maxs) + flat_reach(npc.maxs)) as f32;
        let mut arc = if blocked_dist <= avoid {
            135.0
        } else {
            (avoid / blocked_dist) * 90.0
        };
        let clip = (npc.clip_mask & !CONTENTS_BODY) | CONTENTS_BOTCLIP;
        if blocker.velocity != [0.0; 3] {
            let mut moving = blocker.velocity;
            crate::saber_clash::normalize(&mut moving);
            let across = dot(moving, blocked_dir);
            if across < 0.35 && across > -0.35 {
                let away = moving.map(|axis| axis * -1.0);
                let position = along(self.actors[me].current_origin, blocked_dist, away);
                if self.check_ahead(me, position, clip).0 {
                    *movedir = away;
                    return true;
                }
            }
        }
        if dot(blocked_dir, right) < 0.0 {
            arc *= -1.0;
        }
        for turn in [arc, arc * 0.5, arc * -1.0, (arc * -1.0) * 0.5] {
            if self.nav_test_bypass(
                me,
                crate::npc_droid::angle_normalize360(yaw + turn),
                blocked_dist,
                clip,
                movedir,
            ) {
                return true;
            }
        }
        false
    }

    /// `NAV_TestBypass` (`g_nav.c:577-605`): the way along `yaw`, `distance` long, if the
    /// world leaves it clear.
    fn nav_test_bypass(
        &mut self,
        me: usize,
        yaw: f32,
        distance: f32,
        clip: u32,
        movedir: &mut [f32; 3],
    ) -> bool {
        let (forward, _) = crate::pmove::flight::flight_axes([0.0, yaw, 0.0]);
        let forward = forward.to_array();
        let position = along(self.actors[me].current_origin, distance, forward);
        if self.check_ahead(me, position, clip).0 {
            *movedir = forward;
            return true;
        }
        false
    }
}
