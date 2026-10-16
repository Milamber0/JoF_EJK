//! How a bot moves and aims (OpenJK `codemp/game/ai_main.c`): the traces that make it
//! strafe round, jump over or duck under what is ahead (`BotTrace_Strafe`,
//! `BotTrace_Jump`, `BotTrace_Duck`), its way back when lost off the trail
//! (`BotFallbackNavigation`), lifts (`CheckForFunc`, `WaitingForNow`), what it can see of
//! a point (`WPOrgVisible`), and its aim: leading a moving enemy (`BotAimLeading`) and the
//! wobble of its skill (`BotAimOffsetGoalAngles`).

use crate::bot_routes::{BotRoutes, RouteWorld};
use crate::bot_think::{BotMind, angle_difference};
use crate::bot_weapons::{WeaponRange, weapon_range};
use crate::crt_rand::CrtRand;
use crate::player_angle_math::{normalize, vector_angles};
use crate::pmove::MovementTrace;
use crate::pmove::flight::flight_axes;

/// `MASK_SOLID`.
pub const MASK_SOLID: u32 = 0x1;
/// `MASK_PLAYERSOLID`: solid, player clip, bodies, terrain.
pub const MASK_PLAYERSOLID: u32 = 0x1111;
/// `STRAFEAROUND_RIGHT`, `STRAFEAROUND_LEFT`.
pub const STRAFE_RIGHT: i32 = 1;
pub const STRAFE_LEFT: i32 = 2;
/// `ENTITYNUM_NONE`.
const ENTITYNUM_NONE: i32 = 1023;
/// `MAX_CLIENTS`.
const MAX_CLIENTS: i32 = 32;

/// The game as a moving bot reads it.
pub trait BotMoveWorld {
    /// `trap->Trace` with a box, passing `pass`, against `mask`.
    fn trace(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        pass: i32,
        mask: u32,
    ) -> MovementTrace;
    /// `trap->InPVS`.
    fn in_pvs(&mut self, from: [f32; 3], to: [f32; 3]) -> bool;
    /// Whether the entity's classname has `func_` in it.
    fn is_func(&self, entity: i32) -> bool;
    /// For an `ET_SPECIAL` entity (a shield), the client that owns it, if its owner is a
    /// client; `None` for any other entity.
    fn special_owner(&self, entity: i32) -> Option<Option<i32>>;
    /// `OnSameTeam` of two clients.
    fn same_team(&self, one: i32, other: i32) -> bool;
    /// `botstates[client]->jumpTime`, for a client that is a bot.
    fn bot_jump_time(&self, client: i32) -> Option<f32>;
}

/// [`BotMoveWorld`] as [`RouteWorld`], for `GetNearestVisibleWP`.
pub struct RouteView<'a>(pub &'a mut dyn BotMoveWorld);

impl RouteWorld for RouteView<'_> {
    fn in_pvs(&mut self, from: [f32; 3], to: [f32; 3]) -> bool {
        self.0.in_pvs(from, to)
    }
    fn clear_box(
        &mut self,
        from: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        to: [f32; 3],
        ignore: i32,
    ) -> bool {
        let trace = self.0.trace(from, mins, maxs, to, ignore, MASK_SOLID);
        trace.fraction == 1.0 && !trace.start_solid && !trace.all_solid
    }
}

fn add(a: [f32; 3], b: [f32; 3], scale: f32) -> [f32; 3] {
    [
        a[0] + b[0] * scale,
        a[1] + b[1] * scale,
        a[2] + b[2] * scale,
    ]
}

fn toward(from: [f32; 3], to: [f32; 3]) -> [f32; 3] {
    [to[0] - from[0], to[1] - from[1], to[2] - from[2]]
}

/// `BotTrace_Strafe`: on the ground and facing within 60° of where it goes, with
/// something 32 units ahead — which side it can strafe round (1 right, 2 left), or 0.
pub fn trace_strafe(
    mind: &BotMind,
    client: i32,
    on_ground: bool,
    to_point: [f32; 3],
    world: &mut dyn BotMoveWorld,
) -> i32 {
    if !on_ground {
        return 0;
    }
    let (mins, maxs) = ([-15.0, -15.0, -8.0], [15.0, 15.0, 40.0]);
    let mut direction = toward(mind.origin, to_point);
    normalize(&mut direction);
    let heading = vector_angles(direction);
    let turn = angle_difference(mind.viewangles[1], heading[1]);
    if turn > 60.0 || turn < -60.0 {
        return 0;
    }
    let mut from = mind.origin;
    let mut difference = toward(from, to_point);
    normalize(&mut difference);
    let forward = flight_axes(vector_angles(difference)).0.to_array();
    let mut to = add(from, forward, 32.0);
    if world
        .trace(from, mins, maxs, to, client, MASK_PLAYERSOLID)
        .fraction
        == 1.0
    {
        return 0;
    }
    let right = flight_axes(heading).1.to_array();
    from = [
        from[0] + right[0] * 32.0,
        from[1] + right[1] * 32.0,
        from[2] + right[2] * 16.0,
    ];
    to = add(to, right, 32.0);
    if world
        .trace(from, mins, maxs, to, client, MASK_PLAYERSOLID)
        .fraction
        == 1.0
    {
        return STRAFE_RIGHT;
    }
    from = add(from, right, -64.0);
    to = add(to, right, -64.0);
    if world
        .trace(from, mins, maxs, to, client, MASK_PLAYERSOLID)
        .fraction
        == 1.0
    {
        return STRAFE_LEFT;
    }
    0
}

/// The point 4 units ahead towards `to_point`.
fn step_ahead(origin: [f32; 3], to_point: [f32; 3]) -> [f32; 3] {
    let forward = flight_axes(vector_angles(toward(origin, to_point)))
        .0
        .to_array();
    add(origin, forward, 4.0)
}

/// `BotTrace_Jump`: something just ahead that is clear 41 units up — worth jumping over,
/// but not a bot jumping itself, nor its enemy when it fights with the saber or melee.
pub fn trace_jump(
    mind: &BotMind,
    client: i32,
    weapon: i32,
    to_point: [f32; 3],
    level_time: i32,
    world: &mut dyn BotMoveWorld,
) -> bool {
    let mut ahead = step_ahead(mind.origin, to_point);
    let trace = world.trace(
        mind.origin,
        [-15.0, -15.0, -18.0],
        [15.0, 15.0, 32.0],
        ahead,
        client,
        MASK_PLAYERSOLID,
    );
    if trace.fraction == 1.0 {
        return false;
    }
    let blocker = i32::from(trace.entity_number);
    let mut from = mind.origin;
    from[2] += 41.0;
    ahead[2] += 41.0;
    if world
        .trace(
            from,
            [-15.0, -15.0, 0.0],
            [15.0, 15.0, 8.0],
            ahead,
            client,
            MASK_PLAYERSOLID,
        )
        .fraction
        != 1.0
    {
        return false;
    }
    if (0..MAX_CLIENTS).contains(&blocker)
        && world
            .bot_jump_time(blocker)
            .is_some_and(|time| time > level_time as f32)
    {
        return false;
    }
    let range = weapon_range(weapon);
    !(mind.current_enemy == Some(blocker)
        && (range == WeaponRange::Saber || range == WeaponRange::Melee))
}

/// `BotTrace_Duck`: clear just ahead low down but not 31 units up — worth ducking.
pub fn trace_duck(
    mind: &BotMind,
    client: i32,
    to_point: [f32; 3],
    world: &mut dyn BotMoveWorld,
) -> bool {
    let mut ahead = step_ahead(mind.origin, to_point);
    if world
        .trace(
            mind.origin,
            [-15.0, -15.0, -23.0],
            [15.0, 15.0, 8.0],
            ahead,
            client,
            MASK_PLAYERSOLID,
        )
        .fraction
        != 1.0
    {
        return false;
    }
    let mut from = mind.origin;
    from[2] += 31.0;
    ahead[2] += 31.0;
    world
        .trace(
            from,
            [-15.0, -15.0, 0.0],
            [15.0, 15.0, 32.0],
            ahead,
            client,
            MASK_PLAYERSOLID,
        )
        .fraction
        != 1.0
}

/// `BotFallbackNavigation`: lost off the trail, go 16 units along its goal's heading if
/// clear (1), else turn to a heading of the C library's `rand` (0); busy with a visible
/// enemy (2).
pub fn fallback_navigation(
    mind: &mut BotMind,
    world: &mut dyn BotMoveWorld,
    rand: &mut CrtRand,
) -> i32 {
    if mind.current_enemy.is_some() && mind.frame_enemy_vis {
        return 2;
    }
    mind.goal_angles[0] = 0.0;
    mind.goal_angles[2] = 0.0;
    let forward = flight_axes(mind.goal_angles).0.to_array();
    let ahead = add(mind.origin, forward, 16.0);
    if world
        .trace(
            mind.origin,
            [-15.0, -15.0, 0.0],
            [15.0, 15.0, 32.0],
            ahead,
            ENTITYNUM_NONE,
            MASK_SOLID,
        )
        .fraction
        == 1.0
    {
        mind.goal_position = ahead;
        return 1;
    }
    mind.goal_angles[1] = (rand.next() % 360) as f32;
    0
}

/// `CheckForFunc`: whether a `func_` brush lies within 64 units below `origin`.
pub fn check_for_func(origin: [f32; 3], ignore: i32, world: &mut dyn BotMoveWorld) -> bool {
    let under = [origin[0], origin[1], origin[2] - 64.0];
    let trace = world.trace(origin, [0.0; 3], [0.0; 3], under, ignore, MASK_SOLID);
    trace.fraction != 1.0 && world.is_func(i32::from(trace.entity_number))
}

/// `WaitingForNow`: heading for its point (`goal` its very origin, whole units) and
/// standing within 16 units of it on a `func_` brush while it is over 100 away — riding a
/// lift; within 64 on one while it is over 64 away, no use for two seconds.
pub fn waiting_for_now(
    mind: &mut BotMind,
    routes: &BotRoutes,
    client: i32,
    goal: [f32; 3],
    level_time: i32,
    world: &mut dyn BotMoveWorld,
) -> bool {
    let Some(point) = mind
        .wp_current
        .and_then(|current| routes.waypoints.get(current))
    else {
        return false;
    };
    let origin = point.origin;
    if (0..3).any(|axis| goal[axis] as i32 != origin[axis] as i32) {
        return false;
    }
    let flat = [mind.origin[0] - origin[0], mind.origin[1] - origin[1], 0.0];
    let distance = (flat[0] * flat[0] + flat[1] * flat[1] + flat[2] * flat[2]).sqrt();
    if distance < 16.0 && mind.frame_waypoint_len > 100.0 {
        if check_for_func(mind.origin, client, world) {
            return true;
        }
    } else if distance < 64.0
        && mind.frame_waypoint_len > 64.0
        && check_for_func(mind.origin, client, world)
    {
        mind.no_use_time = level_time + 2000;
    }
    false
}

/// `WPOrgVisible`: 0 blocked; 1 seen; 2 seen through a shield that is neither its own
/// nor a teammate's.
pub fn wp_org_visible(
    bot: i32,
    from: [f32; 3],
    to: [f32; 3],
    ignore: i32,
    world: &mut dyn BotMoveWorld,
) -> i32 {
    if world
        .trace(from, [0.0; 3], [0.0; 3], to, ignore, MASK_SOLID)
        .fraction
        != 1.0
    {
        return 0;
    }
    let trace = world.trace(from, [0.0; 3], [0.0; 3], to, ignore, MASK_PLAYERSOLID);
    let hit = i32::from(trace.entity_number);
    if trace.fraction != 1.0 && hit != ENTITYNUM_NONE {
        if let Some(owner) = world.special_owner(hit) {
            if let Some(owner) = owner
                && (world.same_team(bot, owner) || bot == owner)
            {
                return 1;
            }
            return 2;
        }
    }
    1
}

/// `BotAimLeading`: the goal angles at where an enemy moving at `velocity` will be —
/// ahead of `head` along its motion by the distance times `lead`, scaled by its speed.
pub fn aim_leading(mind: &mut BotMind, velocity: Option<[f32; 3]>, head: [f32; 3], lead: f32) {
    let Some(velocity) = velocity else { return };
    if mind.frame_enemy_len == 0.0 {
        return;
    }
    let mut total = velocity[0].abs() + velocity[1].abs() + velocity[2].abs();
    let mut motion = velocity;
    normalize(&mut motion);
    if total > 400.0 {
        total = 400.0;
    }
    let reach = f64::from(mind.frame_enemy_len) * 0.9 * f64::from(lead);
    let ahead = if total != 0.0 {
        (reach * (f64::from(total) * 0.0012)) as i32
    } else {
        reach as i32
    } as f32;
    let spot = add(head, motion, ahead);
    mind.goal_angles = vector_angles(toward(mind.eye, spot));
}

/// What the aim's wobble reads of the enemy.
#[derive(Clone, Copy, Debug, Default)]
pub struct AimTarget {
    /// The enemy tricked the bot (it aims by ear).
    pub tricked: bool,
    /// The enemy is the one it hates (`revengeEnemy`), at this level of hate.
    pub revenge_level: i32,
    /// The enemy moves (`s.pos.trDelta`), and so does the bot.
    pub moving: bool,
    pub self_moving: bool,
}

/// `BotAimOffsetGoalAngles`: while an offset holds, the goal angles moved by it; then a
/// new one of up to its accuracy over its skill (worse by ear, better in hatred, none at
/// a still enemy it sees, worse if either moves), drawn with the C library's `rand`, for
/// 200 to 700 ms. None for perfect aim.
pub fn aim_offset_goal_angles(
    mind: &mut BotMind,
    target: Option<&AimTarget>,
    level_time: i32,
    rand: &mut CrtRand,
) {
    if mind.skills.perfectaim != 0 {
        return;
    }
    if mind.aim_offset_time > level_time as f32 {
        if mind.aim_offset_amt_yaw != 0.0 {
            mind.goal_angles[1] += mind.aim_offset_amt_yaw;
        }
        if mind.aim_offset_amt_pitch != 0.0 {
            mind.goal_angles[0] += mind.aim_offset_amt_pitch;
        }
        for angle in &mut mind.goal_angles {
            if *angle > 360.0 {
                *angle -= 360.0;
            }
            if *angle < 0.0 {
                *angle += 360.0;
            }
        }
        return;
    }
    let mut accuracy = mind.skills.accuracy / mind.skill;
    if target.is_some_and(|target| target.tricked) {
        accuracy *= 7.0;
        if accuracy < 30.0 {
            accuracy = 30.0;
        }
    }
    if let Some(target) = target.filter(|target| target.revenge_level != 0) {
        accuracy /= target.revenge_level as f32;
    }
    if let Some(target) = target.filter(|_| mind.frame_enemy_vis) {
        accuracy = if target.moving {
            (f64::from(accuracy) + f64::from(accuracy) * 0.25) as f32
        } else {
            0.0
        };
        if target.self_moving {
            accuracy = (f64::from(accuracy) + f64::from(accuracy) * 0.15) as f32;
        }
    }
    if accuracy > 90.0 {
        accuracy = 90.0;
    }
    if accuracy < 1.0 {
        accuracy = 0.0;
    }
    if accuracy == 0.0 {
        mind.aim_offset_amt_yaw = 0.0;
        mind.aim_offset_amt_pitch = 0.0;
        return;
    }
    let reach = accuracy as i32;
    let draw = |rand: &mut CrtRand| {
        if rand.next() % 10 <= 5 {
            rand.next() % reach
        } else {
            -(rand.next() % reach)
        }
    };
    mind.aim_offset_amt_yaw = draw(rand) as f32;
    mind.aim_offset_amt_pitch = draw(rand) as f32;
    mind.aim_offset_time = (level_time + rand.next() % 500 + 200) as f32;
}
