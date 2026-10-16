//! How an NPC goes to its goal (`codemp/game/NPC_move.c`, `g_navnew.c`, `g_nav.c`,
//! `NPC_goal.c`): straight at it when the way is clear of the world
//! (`NPC_ClearPathToGoal`, `NAV_CheckAhead`), round the bodies in its way (sidestepping,
//! dancing aside, shoving another NPC out of it: `NAVNEW_AvoidCollision` and the rest), the
//! move made a command (`G_UcmdMoveForDir`, or facing the goal and running), the goal set
//! (`NPC_SetMoveGoal`) and reached (`UpdateGoal`, `NAV_HitNavGoal`).
//!
//! Where the world blocks the way, the NPC takes the route along the map's waypoints
//! (macro navigation, [`crate::npc_nav_route`]); with no route it turns to face its goal
//! and does not look again for 0.5–1.5 s (`noWaypointTime`).
//!
//! `d_altRoutes` and `d_patched` are 0, as the reference's defaults. There are no ladders
//! (`CONTENTS_LADDER` is never an NPC's `watertype` here) or scripts (`TID_MOVE_NAV`);
//! doors are the host's to name ([`NpcHost::door_center`], [`NpcHost::nav_obstacle`]).
//!
//! Held to `tools/game-oracle/npcst.c` (`game-npcst.txt`) and `npcnav.c`
//! (`game-npcnav.txt`).

use crate::npc_mind::WAYPOINT_NONE;
use crate::npc_senses::{Body, distance_squared, subtract};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use crate::player_angle_math::{angle_mod, normalize, vector_angles};
use sjk_protocol::UserCommand;

/// `NIF_*`: what a move met (`b_local.h:311-315`).
pub const NIF_MACRO_NAV: i32 = 0x2;
pub const NIF_COLLISION: i32 = 0x4;
pub const NIF_BLOCKED: i32 = 0x8;
/// `NPCAI_MOVING`, `NPCAI_TOUCHED_GOAL`, `NPCAI_BLOCKED`.
const NPCAI_MOVING: u32 = 0x4;
const NPCAI_TOUCHED_GOAL: u32 = 0x8;
pub(crate) const NPCAI_BLOCKED: u32 = 0x40;
/// `STEPSIZE`, `MAX_COLL_AVOID_DIST`, `MIN_STOP_DIST`, `MIN_BLOCKED_SPEECH_TIME`,
/// `NAVGOAL_USE_RADIUS`.
const STEPSIZE: f32 = 18.0;
const MAX_COLL_AVOID_DIST: f32 = 128.0;
const MIN_STOP_DIST: f32 = 64.0;
const MIN_BLOCKED_SPEECH_TIME: i32 = 4_000;
const NAVGOAL_USE_RADIUS: i32 = 16_384;
/// `MIN_DOOR_BLOCK_DIST_SQR`.
const MIN_DOOR_BLOCK_DIST_SQR: f32 = 16.0 * 16.0;
/// `CONTENTS_BODY`, `CONTENTS_BOTCLIP`.
const CONTENTS_BODY: u32 = 0x100;
const CONTENTS_BOTCLIP: u32 = 0x40;
/// `BUTTON_WALKING`; `EF2_FLYING`; `ps.eFlags2`, `ps.legsTimer`; `s.legsAnim`.
const BUTTON_WALKING: u16 = 16;
const EF2_FLYING: u32 = 1 << 4;
const PS_EFLAGS2: usize = 103;
const PS_LEGS_TIMER: usize = 21;
const ES_LEGS_ANIM: usize = 16;
/// `MAX_CLIENTS`.
const MAX_CLIENTS: u16 = 32;

/// `navInfo_t`: what the last move found (`frameNavInfo`, `NAV_GetLastMove`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NavInfo {
    pub direction: [f32; 3],
    pub path_direction: [f32; 3],
    pub distance: f32,
    pub flags: i32,
    /// The entity it ran into.
    pub blocker: Option<u16>,
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `VectorMA(start, scale, direction)`.
fn along(start: [f32; 3], scale: f32, direction: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| start[axis] + scale * direction[axis])
}

/// `VectorNormalize`, returning the length.
fn normalized(vector: &mut [f32; 3]) -> f32 {
    crate::saber_clash::normalize(vector)
}

/// `AngleVectors`' forward and right.
fn axes(angles: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let (forward, right) = crate::pmove::flight::flight_axes(angles);
    (forward.to_array(), right.to_array())
}

/// `NAV_HitNavGoal` (`g_nav.c:178-236`): whether a box at `point` reaches `dest` — within
/// `radius` of it (on the flat, a flier exactly), or, for a plain radius, overlapping a
/// cube of that half-size round it.
pub fn hit_nav_goal(
    point: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    dest: [f32; 3],
    radius: i32,
    flying: bool,
) -> bool {
    if radius & NAVGOAL_USE_RADIUS != 0 {
        let radius = (radius & !NAVGOAL_USE_RADIUS) as f32;
        let mut diff = subtract(point, dest);
        if flying {
            return distance_squared(dest, point) <= radius * radius;
        }
        if f64::from(diff[2]).abs() <= 24.0 {
            diff[2] = 0.0;
        }
        return dot(diff, diff) <= radius * radius;
    }
    let radius = radius as f32;
    (0..3).all(|axis| {
        let (low, high) = (point[axis] + mins[axis], point[axis] + maxs[axis]);
        low <= dest[axis] + radius && high >= dest[axis] - radius
    })
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `FlyingCreature` (`g_nav.c:68-75`): a gravity of zero or less.
    pub fn flying(&self, me: usize) -> bool {
        self.actors[me].player.gravity() <= 0
    }

    /// The origin of the NPC's goal entity (`goalEntity->r.currentOrigin`): its own goal
    /// entity's place, the body it goes after, an NPC's saber entity (its own knocked out of
    /// its hand, `Jedi_Attack`'s `saberStoredIndex`), or another entity as the host has it.
    pub fn goal_origin(&self, me: usize) -> Option<[f32; 3]> {
        let npc = &self.actors[me];
        let goal = npc.mind.goal?;
        if Some(goal) == npc.goal {
            return Some(npc.mind.tactics.temp_goal.origin);
        }
        if let Some(body) = self.body(goal) {
            return Some(body.origin);
        }
        if let Some(owner) = self
            .actors
            .iter()
            .find(|npc| npc.saber_entity == Some(goal))
        {
            return Some(owner.saber.entity.origin);
        }
        self.host.entity_box(goal).map(|(origin, _, _)| origin)
    }

    /// Whether the NPC's goal is its own goal entity (`goalEntity == tempGoal`).
    pub fn goal_is_temp(&self, me: usize) -> bool {
        let npc = &self.actors[me];
        npc.mind.goal.is_some() && npc.mind.goal == npc.goal
    }

    /// `NPC_MoveToGoal(tryStraight)` (`NPC_move.c:405-490`): a move toward the goal made the
    /// command's (`command`), unless the NPC is knocked down or in pain (a success that does
    /// nothing). Whether it could move.
    pub fn move_to_goal(
        &mut self,
        me: usize,
        try_straight: bool,
        command: &mut UserCommand,
    ) -> bool {
        let npc = &self.actors[me];
        let legs = npc.state.raw_field(ES_LEGS_ANIM).unwrap_or(0) as u16;
        let legs_timer = npc.player.raw_field(PS_LEGS_TIMER).unwrap_or(0) as i32;
        if crate::pmove_hand_extend::in_knockdown(npc.player.leg_animation(), legs_timer)
            || crate::pmove_locomotion::PAIN_ANIMATIONS.contains(&legs)
        {
            return true;
        }
        let Some((mut dir, distance)) = self.alt_route_direction(me, try_straight, command) else {
            return false;
        };
        let npc = &mut self.actors[me];
        npc.mind.dist_to_goal = distance;
        npc.mind.tactics.last_path_angles = vector_angles(dir);
        let stats = npc.definition.stats;
        npc.player
            .set_speed(if command.buttons & BUTTON_WALKING != 0 {
                stats.walk_speed
            } else {
                stats.run_speed
            } as f32);
        let combat =
            npc.mind.combat_move || (npc.mind.goal.is_some() && npc.mind.goal == npc.mind.enemy);
        if combat {
            self.ucmd_move_for_dir(me, command, &mut dir);
            return true;
        }
        let npc = &mut self.actors[me];
        let path = npc.mind.tactics.last_path_angles;
        npc.mind.desired_pitch = 0.0;
        npc.desired_yaw = angle_mod(path[1]);
        if npc.player.raw_field(PS_EFLAGS2).unwrap_or(0) & EF2_FLYING != 0 {
            npc.mind.desired_pitch = angle_mod(path[0]);
            if dir[2] != 0.0 {
                let scale = (dir[2] * distance).clamp(-64.0, 64.0);
                let mut velocity = npc.player.velocity();
                velocity[2] = scale;
                npc.player.set_velocity(velocity);
            }
        }
        command.forward_move = 127;
        true
    }

    /// `NPC_GetMoveDirectionAltRoute` (`NPC_move.c:247-336`): the direction to go in and the
    /// distance to the goal, or `None` (having turned to face the goal where it is blocked).
    fn alt_route_direction(
        &mut self,
        me: usize,
        try_straight: bool,
        command: &mut UserCommand,
    ) -> Option<([f32; 3], f32)> {
        self.actors[me].ai_flags &= !NPCAI_BLOCKED;
        self.level.nav = NavInfo::default();
        let goal = self.goal_origin(me)?;
        let mut direction = subtract(goal, self.actors[me].current_origin);
        let distance = normalized(&mut direction);
        self.actors[me].mind.tactics.blocked_dest = goal;
        self.level.nav = NavInfo {
            direction,
            path_direction: direction,
            distance,
            flags: 0,
            blocker: None,
        };
        if !try_straight || !self.clear_path_to_goal(me, goal) {
            // Macro navigation: the route along the waypoints, or — none — face the goal.
            let mut info = self.level.nav;
            if self.navnew_move_to_goal(me, &mut info, command) == sjk_nav::NODE_NONE {
                self.level.nav = info;
                let angles = vector_angles(self.level.nav.direction);
                self.actors[me].desired_yaw = angle_mod(angles[1]);
                return None;
            }
            info.flags |= NIF_MACRO_NAV;
            self.level.nav = info;
            return Some((info.direction, info.distance));
        }
        let mut info = self.level.nav;
        let goal_number = self.actors[me].mind.goal;
        let clear = self.avoid_collision(me, goal_number, &mut info, true, 30, command);
        self.level.nav = info;
        clear.then_some((info.direction, info.distance))
    }

    /// `NPC_ClearPathToGoal` (`NPC_move.c:45-90`): whether the world lets the NPC go
    /// straight at the goal at `goal`, or near enough.
    pub(crate) fn clear_path_to_goal(&mut self, me: usize, goal: [f32; 3]) -> bool {
        let clip = (self.actors[me].clip_mask & !CONTENTS_BODY) | CONTENTS_BOTCLIP;
        let (clear, trace) = self.check_ahead(me, goal, clip);
        if clear {
            return true;
        }
        let npc = &self.actors[me];
        let flying = self.flying(me);
        if !flying && f64::from(npc.current_origin[2] - goal[2]).abs() > 48.0 {
            return false;
        }
        let radius = npc.maxs[0].max(npc.maxs[1]);
        let distance = distance_squared(npc.current_origin, goal).sqrt();
        if trace.fraction >= 1.0 - radius / distance {
            return true;
        }
        self.goal_is_temp(me)
            && npc.mind.tactics.temp_goal.nav_goal
            && hit_nav_goal(
                trace.end_position,
                npc.mins,
                npc.maxs,
                goal,
                npc.mind.tactics.goal_radius,
                flying,
            )
    }

    /// `NAV_CheckAhead` (`g_nav.c:516-576`): whether the NPC's box, stepping, reaches `end`
    /// (or all but its own radius of it); with the trace.
    pub(crate) fn check_ahead(
        &mut self,
        me: usize,
        end: [f32; 3],
        clip: u32,
    ) -> (bool, crate::pmove::MovementTrace) {
        let npc = &self.actors[me];
        let (origin, maxs, number) = (npc.current_origin, npc.maxs, npc.number);
        let mins = [npc.mins[0], npc.mins[1], npc.mins[2] + STEPSIZE];
        let mut trace = self.trace_bodies(origin, mins, maxs, end, number, clip);
        if trace.start_solid && clip & CONTENTS_BOTCLIP != 0 {
            // Started inside a do-not-enter brush (`trace.contents & CONTENTS_BOTCLIP`):
            // traced again without them.
            let again =
                self.trace_bodies(origin, mins, maxs, end, number, clip & !CONTENTS_BOTCLIP);
            if !again.start_solid {
                trace = again;
            }
        }
        let clear = crate::npc_nav_ahead::clear_ahead(origin, maxs, end, &trace, |number| {
            self.host.nav_obstacle(number)
        });
        (clear, trace)
    }

    /// `NAVNEW_AvoidCollision` (`g_navnew.c:490-557`): the move's direction turned round a
    /// body in the way (`info.direction`). Whether the NPC can go on.
    pub(crate) fn avoid_collision(
        &mut self,
        me: usize,
        goal: Option<u16>,
        info: &mut NavInfo,
        set_blocked: bool,
        limit: i32,
        command: &mut UserCommand,
    ) -> bool {
        info.distance = info.distance.min(MAX_COLL_AVOID_DIST);
        let origin = self.actors[me].current_origin;
        let end = along(origin, info.distance, info.direction);
        let mut movedir = info.direction;
        let (clear, trace) = self.check_ahead(me, end, CONTENTS_BODY);
        if clear {
            if set_blocked {
                self.actors[me].mind.tactics.consecutive_blocked_moves = 0;
            }
            return true;
        }
        info.blocker = Some(trace.entity_number);
        info.flags |= NIF_COLLISION;
        if goal == Some(trace.entity_number) {
            return true;
        }
        let blocker = self.blocker_body(trace.entity_number);
        if set_blocked {
            if self.actors[me].mind.tactics.consecutive_blocked_moves > limit {
                self.set_blocked(me, &blocker);
                return false;
            }
            self.actors[me].mind.tactics.consecutive_blocked_moves += 1;
        }
        if self.test_for_blocked(me, &blocker, info.distance, &mut info.flags, command) {
            return false;
        }
        if !self.resolve_collision(me, &blocker, &mut movedir, set_blocked) {
            return false;
        }
        info.direction = movedir;
        true
    }

    /// The entity a body-only trace ran into (`&g_entities[trace.entityNum]`): a player or
    /// an NPC, else whatever else the host links there with no client — a model with a
    /// body's contents; in the fake engine, whose room ignores the trace's mask, the world
    /// itself (its box and origin zero).
    pub(crate) fn blocker_body(&self, number: u16) -> Body {
        if let Some(body) = self.body(number) {
            return body;
        }
        let (origin, mins, maxs) = self.host.entity_box(number).unwrap_or_default();
        Body {
            number,
            npc: false,
            origin,
            mins,
            maxs,
            view_height: 0,
            view_angles: [0.0; 3],
            eye_point: [0.0; 3],
            eye_angles: [0.0; 3],
            health: 0,
            flags: 0,
            entity_flags: 0,
            // No client: never anyone's team.
            player_team: -1,
            enemy_team: -1,
            session_team: 0,
            class: 0,
            weapon: 0,
            enemy: None,
            spectating: false,
            surrendering: false,
            velocity: [0.0; 3],
            ducked: false,
            saber_holstered: false,
            saber_in_flight: false,
        }
    }

    /// `NAV_TestForBlocked` (`g_nav.c:895-918`): a blocker standing on the goal, near,
    /// stops the NPC — facing it, and taking it on if it is an enemy.
    pub(crate) fn test_for_blocked(
        &mut self,
        me: usize,
        blocker: &Body,
        distance: f32,
        flags: &mut i32,
        command: &mut UserCommand,
    ) -> bool {
        let Some(goal) = self.goal_origin(me) else {
            return false;
        };
        if !hit_nav_goal(blocker.origin, blocker.mins, blocker.maxs, goal, 12, false) {
            return false;
        }
        *flags |= NIF_BLOCKED;
        if distance > MIN_STOP_DIST {
            return false;
        }
        self.npc_blocked(me, blocker);
        let spot = crate::npc_senses::spot(blocker, crate::npc_senses::Spot::HeadLean);
        self.face_position(me, spot, true, command);
        true
    }

    /// `NPC_Blocked` (`g_nav.c:87-125`): an enemy in the way taken on; anyone else
    /// complained of (no voice in multiplayer) at most every 4–8 s.
    pub(crate) fn npc_blocked(&mut self, me: usize, blocker: &Body) {
        if self.actors[me].mind.blocked_speech_until > self.level_time {
            return;
        }
        if blocker.player_team == self.actors[me].enemy_team {
            self.set_enemy(me, blocker.number);
            return;
        }
        self.set_blocked(me, blocker);
    }

    /// `NPC_SetBlocked` (`g_navnew.c:57-66`).
    fn set_blocked(&mut self, me: usize, blocker: &Body) {
        let chance = self.host.rng().flrand(0.0, 1.0);
        let npc = &mut self.actors[me];
        // `level.time + MIN_BLOCKED_SPEECH_TIME + (flrand * 4000)`: a float, truncated.
        npc.mind.blocked_speech_until =
            ((self.level_time + MIN_BLOCKED_SPEECH_TIME) as f32 + chance * 4_000.0) as i32;
        npc.mind.tactics.blocking_ent_num = i32::from(blocker.number);
    }

    /// `NAVNEW_ResolveEntityCollision` (`g_navnew.c:442-484`): round the blocker, or through
    /// if it waits on this NPC too.
    fn resolve_collision(
        &mut self,
        me: usize,
        blocker: &Body,
        movedir: &mut [f32; 3],
        set_blocked: bool,
    ) -> bool {
        // A door is passed unless the NPC is right at its middle (`CalcTeamDoorCenter`).
        if let Some(center) = self.host.door_center(blocker.number)
            && distance_squared(self.actors[me].current_origin, center) > MIN_DOOR_BLOCK_DIST_SQR
        {
            return true;
        }
        let mut blocked_dir = subtract(blocker.origin, self.actors[me].current_origin);
        let blocked_dist = normalized(&mut blocked_dir);
        if self.bypass(me, blocker, blocked_dir, blocked_dist, movedir, set_blocked) {
            return true;
        }
        let number = i32::from(self.actors[me].number);
        if self
            .actor_at(blocker.number)
            .is_some_and(|at| self.actors[at].mind.tactics.blocking_ent_num == number)
        {
            return true;
        }
        if set_blocked {
            self.set_blocked(me, blocker);
        }
        false
    }

    /// `NAVNEW_Bypass` (`g_navnew.c:397-425`).
    fn bypass(
        &mut self,
        me: usize,
        blocker: &Body,
        blocked_dir: [f32; 3],
        blocked_dist: f32,
        movedir: &mut [f32; 3],
        set_blocked: bool,
    ) -> bool {
        let mut angles = vector_angles(*movedir);
        angles[2] = 0.0;
        let (_, right) = axes(angles);
        if dance(blocker, movedir, right) {
            return true;
        }
        if self.sidestep(me, blocker, blocked_dir, blocked_dist, movedir) {
            return true;
        }
        self.push_blocker(me, blocker, right, set_blocked);
        false
    }

    /// `NAVNEW_SidestepBlocker` (`g_navnew.c:239-390`): a clear way past on the right, else
    /// the left, the side kept for 2 s.
    fn sidestep(
        &mut self,
        me: usize,
        blocker: &Body,
        blocked_dir: [f32; 3],
        blocked_dist: f32,
        movedir: &mut [f32; 3],
    ) -> bool {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        let (origin, maxs, number, clip) = (
            npc.current_origin,
            npc.maxs,
            npc.number,
            npc.clip_mask | CONTENTS_BOTCLIP,
        );
        let mins = [npc.mins[0], npc.mins[1], npc.mins[2] + STEPSIZE];
        let yaw = vector_to_yaw(blocked_dir);
        let reach = |maxs: [f32; 3]| f64::from(maxs[0] * maxs[0] + maxs[1] * maxs[1]).sqrt();
        let avoid_radius = (reach(blocker.maxs) + reach(maxs)) as f32;
        let mut arc = if blocked_dist <= avoid_radius {
            135.0
        } else {
            (avoid_radius / blocked_dist) * 90.0
        };
        let heading = |arc: f32| axes([0.0, angle_mod(yaw + arc), 0.0]).0;
        let tactics = npc.mind.tactics;
        if tactics.side_step_hold_time > level_time {
            if tactics.last_side_step_side == -1 {
                arc = -arc;
            }
            *movedir = heading(arc);
            let trace = self.trace_bodies(
                origin,
                mins,
                maxs,
                along(origin, blocked_dist, *movedir),
                number,
                clip,
            );
            return trace.fraction == 1.0 && !trace.all_solid && !trace.start_solid;
        }
        let try_side = |world: &mut Self, direction: [f32; 3]| {
            let trace = world.trace_bodies(
                origin,
                mins,
                maxs,
                along(origin, blocked_dist, direction),
                number,
                clip,
            );
            if trace.all_solid || trace.start_solid {
                0.0
            } else {
                trace.fraction
            }
        };
        let right = heading(arc);
        let right_success = try_side(self, right);
        if right_success >= 1.0 {
            return self.take_side(me, movedir, right, 1);
        }
        let left = heading(-arc);
        let left_success = try_side(self, left);
        if left_success >= 1.0 {
            return self.take_side(me, movedir, left, -1);
        }
        if left_success == 0.0 && right_success == 0.0 {
            return false;
        }
        if right_success * blocked_dist >= avoid_radius
            || left_success * blocked_dist >= avoid_radius
        {
            return if right_success >= left_success {
                self.take_side(me, movedir, right, 1)
            } else {
                self.take_side(me, movedir, left, -1)
            };
        }
        false
    }

    /// A sidestep taken: its direction, and its side held for 2 s.
    fn take_side(
        &mut self,
        me: usize,
        movedir: &mut [f32; 3],
        direction: [f32; 3],
        side: i32,
    ) -> bool {
        *movedir = direction;
        let tactics = &mut self.actors[me].mind.tactics;
        tactics.last_side_step_side = side;
        tactics.side_step_hold_time = self.level_time + 2_000;
        true
    }

    /// `NAVNEW_PushBlocker` (`g_navnew.c:98-180`): another NPC in the way asked to step
    /// aside (`pushVec` for 2 s), to whichever side is clear; never a player; not for more
    /// than 30 tries.
    fn push_blocker(&mut self, me: usize, blocker: &Body, right: [f32; 3], set_blocked: bool) {
        if self.actors[me].mind.tactics.shove_count > 30 || blocker.number < MAX_CLIENTS {
            return;
        }
        let Some(other) = self.actor_at(blocker.number) else {
            return;
        };
        if self.actors[other].mind.tactics.push_vec != [0.0; 3] {
            return;
        }
        let (origin, clip) = (
            blocker.origin,
            self.actors[other].clip_mask | CONTENTS_BOTCLIP,
        );
        let mins = [blocker.mins[0], blocker.mins[1], blocker.mins[2] + STEPSIZE];
        let amount = (f64::from(self.actors[me].maxs[1] + blocker.maxs[1]) * 1.2) as f32;
        let try_side = |world: &mut Self, scale: f32| {
            let trace = world.trace_bodies(
                origin,
                mins,
                blocker.maxs,
                along(origin, scale, right),
                blocker.number,
                clip,
            );
            if trace.start_solid || trace.all_solid {
                0.0
            } else {
                trace.fraction
            }
        };
        let left_success = try_side(self, -amount);
        let push = if left_success >= 1.0 {
            -amount
        } else {
            let right_success = try_side(self, amount);
            if left_success == 0.0 && right_success == 0.0 {
                return;
            }
            if right_success >= 1.0 || left_success < right_success {
                amount
            } else {
                -amount
            }
        };
        let level_time = self.level_time;
        let tactics = &mut self.actors[other].mind.tactics;
        tactics.push_vec = right.map(|axis| axis * push);
        tactics.push_vec_time = level_time + 2_000;
        if set_blocked {
            self.actors[me].mind.tactics.shove_count += 1;
        }
    }

    /// `G_UcmdMoveForDir` (`NPC_move.c:338-378`): the flat direction kept as the NPC's own
    /// (`ps.moveDir`) and made forward and right moves against the way it faces.
    pub fn ucmd_move_for_dir(&mut self, me: usize, command: &mut UserCommand, dir: &mut [f32; 3]) {
        let npc = &mut self.actors[me];
        let (forward, right) = axes(npc.mind.current_angles);
        dir[2] = 0.0;
        normalize(dir);
        npc.mind.move_dir = *dir;
        let forward_dot = (dot(forward, *dir) * 127.0).clamp(-127.0, 127.0);
        let right_dot = (dot(right, *dir) * 127.0).clamp(-127.0, 127.0);
        command.forward_move = f64::from(forward_dot).floor() as i8;
        command.right_move = f64::from(right_dot).floor() as i8;
    }

    /// `NPC_SetMoveGoal` (`g_nav.c:131-176`): the NPC's own goal entity put at `point` and
    /// made its goal.
    pub fn set_move_goal(
        &mut self,
        me: usize,
        point: [f32; 3],
        radius: i32,
        nav_goal: bool,
        combat_point: i32,
        target: Option<u16>,
    ) {
        let npc = &mut self.actors[me];
        if npc.goal.is_none() {
            return;
        }
        let mins = npc.mins;
        let last_waypoint = npc.mind.tactics.temp_goal.last_waypoint;
        // A target's own waypoint, where it has one (`targetEnt->waypoint >= 0`).
        let waypoint = target
            .and_then(|number| self.nav_holder_of(number))
            .map_or(WAYPOINT_NONE, |holder| {
                self.nav_entity(holder).state.waypoint
            })
            .max(WAYPOINT_NONE);
        let npc = &mut self.actors[me];
        npc.mind.tactics.temp_goal = crate::npc_mind::TempGoal {
            origin: point,
            mins,
            maxs: mins,
            nav_goal,
            combat_point,
            target,
            waypoint,
            last_waypoint,
            no_waypoint_time: 0,
        };
        npc.mind.goal = npc.goal;
        npc.mind.tactics.goal_radius = radius;
    }

    /// `NPC_ClearGoal` (`NPC_goal.c:73-98`): nothing pushed before it, so no goal.
    pub fn clear_goal(&mut self, me: usize) {
        let npc = &mut self.actors[me];
        npc.mind.goal = None;
        npc.mind.tactics.goal_time = self.level_time;
    }

    /// `NPC_ReachedGoal` (`NPC_goal.c:139-152`).
    pub fn reached_goal(&mut self, me: usize, command: &mut UserCommand) {
        self.clear_goal(me);
        let npc = &mut self.actors[me];
        npc.mind.tactics.goal_time = self.level_time;
        npc.ai_flags &= !NPCAI_MOVING;
        command.forward_move = 0;
    }

    /// `UpdateGoal` (`NPC_goal.c:265-289`): the goal, unless it is gone or reached.
    pub fn update_goal(&mut self, me: usize, command: &mut UserCommand) -> Option<u16> {
        let goal = self.actors[me].mind.goal?;
        let Some(origin) = self
            .goal_origin(me)
            .filter(|_| self.host.in_use(goal) || self.actor_at(goal).is_some())
        else {
            self.clear_goal(me);
            return None;
        };
        let npc = &mut self.actors[me];
        let touched = npc.ai_flags & NPCAI_TOUCHED_GOAL != 0;
        npc.ai_flags &= !NPCAI_TOUCHED_GOAL;
        if touched
            || hit_nav_goal(
                npc.current_origin,
                npc.mins,
                npc.maxs,
                origin,
                npc.mind.tactics.goal_radius,
                self.flying(me),
            )
        {
            self.reached_goal(me, command);
            return None;
        }
        Some(goal)
    }

    /// `G_ExpandPointToBBox` (`g_utils.c:2063-2104`): `point` moved off the walls until a
    /// box fits round it there. Whether it could be.
    pub fn expand_point_to_bbox(
        &mut self,
        point: &mut [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        ignore: u16,
        clip: u32,
    ) -> bool {
        let mut start = *point;
        for axis in 0..3 {
            let mut end = start;
            end[axis] += mins[axis];
            let trace = self.trace_bodies(start, [0.0; 3], [0.0; 3], end, ignore, clip);
            if trace.all_solid || trace.start_solid {
                return false;
            }
            if trace.fraction < 1.0 {
                let mut end = start;
                end[axis] += maxs[axis] - mins[axis] * trace.fraction;
                let trace = self.trace_bodies(start, [0.0; 3], [0.0; 3], end, ignore, clip);
                if trace.all_solid || trace.start_solid || trace.fraction < 1.0 {
                    return false;
                }
                start = end;
            }
        }
        let trace = self.trace_bodies(start, mins, maxs, start, ignore, clip);
        if trace.all_solid || trace.start_solid {
            return false;
        }
        *point = start;
        true
    }
}

/// `vectoyaw` (`bg_misc.c:1688-1707`): unlike `vectoangles`, in double.
pub(crate) fn vector_to_yaw(vector: [f32; 3]) -> f32 {
    if vector[1] == 0.0 && vector[0] == 0.0 {
        return 0.0;
    }
    let mut yaw = if vector[0] != 0.0 {
        (f64::from(vector[1]).atan2(f64::from(vector[0])) * 180.0 / std::f64::consts::PI) as f32
    } else if vector[1] > 0.0 {
        90.0
    } else {
        270.0
    };
    if yaw < 0.0 {
        yaw += 360.0;
    }
    yaw
}

/// `NAVNEW_DanceWithBlocker` (`g_navnew.c:186-233`): a blocker moving to one side passed on
/// the other — to the left when it goes right fast, else (even standing nearly still, as
/// the reference's second test has it) to the right.
fn dance(blocker: &Body, movedir: &mut [f32; 3], right: [f32; 3]) -> bool {
    if blocker.velocity == [0.0; 3] {
        return false;
    }
    let sideways = [blocker.velocity[0], blocker.velocity[1], 0.0];
    let speed = dot(sideways, right);
    if speed > 50.0 {
        *movedir = along(*movedir, -1.0, right);
    } else if speed > -50.0 {
        *movedir = std::array::from_fn(|axis| right[axis] + movedir[axis]);
    } else {
        return false;
    }
    normalize(movedir);
    true
}
