//! A bot's siege and Jedi Master priorities (OpenJK `codemp/game/ai_main.c`): in siege,
//! its side's objectives to complete or the attackers to hold off
//! (`SiegeTakesPriority`, `Siege_TargetClosestObjective`, `Siege_DefendFromAttackers`,
//! `Siege_CountDefenders`, `Siege_CountTeammates`, `EntityVisibleBox`); in Jedi Master,
//! the master or the saber lying free (`JMTakesPriority`).

use crate::bot_moves::{BotMoveWorld, MASK_SOLID, RouteView};
use crate::bot_routes::{BotRoutes, WPFLAG_SIEGE_IMPERIALOBJ, WPFLAG_SIEGE_REBELOBJ};
use crate::bot_think::BotMind;
use crate::bot_trail::{BotBody, get_best_idle_goal};
use crate::bot_weapons::{WeaponRange, weapon_range};
use crate::items::Item;
use crate::player_death::Rng;

/// `siegeState`.
pub const SIEGESTATE_ATTACKER: i32 = 1;
pub const SIEGESTATE_DEFENDER: i32 = 2;
/// `BOT_MIN_SIEGE_GOAL_SHOOT`, `BOT_MIN_SIEGE_GOAL_TRAVEL`.
const GOAL_SHOOT: f32 = 1024.0;
const GOAL_TRAVEL: f32 = 128.0;
/// `BOT_MAX_WEAPON_GATHER_TIME`, `BOT_MAX_WEAPON_CHASE_TIME`.
const WEAPON_GATHER_TIME: i32 = 1000;
const WEAPON_CHASE_TIME: i32 = 15_000;
const ENTITYNUM_NONE: i32 = 1023;
const SIEGETEAM_TEAM1: i32 = 1;
const TEAM_SPECTATOR: i32 = 3;
const GT_JEDIMASTER: i32 = 2;
const GT_SIEGE: i32 = 7;
const WP_BRYAR_PISTOL: i32 = 4;

/// An entity a siege point may be tied to (an objective).
#[derive(Clone, Copy, Debug)]
pub struct GoalThing {
    pub entity: i32,
    pub in_use: bool,
    /// It has a `use` function.
    pub usable: bool,
    pub takes_damage: bool,
    /// `health`: a target to shoot still stands while it is above 0.
    pub health: i32,
    pub absmin: [f32; 3],
    pub absmax: [f32; 3],
}

impl GoalThing {
    /// The centre of its bounds (a brush model's origin can be anywhere).
    fn centre(&self) -> [f32; 3] {
        std::array::from_fn(|axis| (self.absmax[axis] + self.absmin[axis]) / 2.0)
    }
}

/// A client as the objectives read it.
#[derive(Clone, Copy, Debug)]
pub struct ObjectiveClient {
    pub in_use: bool,
    pub team: i32,
    pub health: i32,
    pub origin: [f32; 3],
    pub is_jedi_master: bool,
    /// A bot's `siegeState`; `None` for a human.
    pub bot_role: Option<i32>,
}

/// The game's objectives around the bot.
#[derive(Clone, Copy, Debug)]
pub struct ObjectiveGame<'a> {
    pub gametype: i32,
    pub level_time: i32,
    pub clients: &'a [Option<ObjectiveClient>],
    /// The entities points may be tied to.
    pub things: &'a [GoalThing],
    /// `imperial_attackers`, `rebel_attackers`: which sides attack.
    pub attackers: (bool, bool),
    /// `gJMSaberEnt`: the Jedi Master's saber as an entity (number, in use, origin).
    pub jedi_master_saber: Option<(i32, bool, [f32; 3])>,
}

impl ObjectiveGame<'_> {
    fn thing(&self, entity: i32) -> Option<&GoalThing> {
        self.things.iter().find(|thing| thing.entity == entity)
    }
    fn team_of(&self, client: usize) -> i32 {
        self.clients
            .get(client)
            .and_then(|client| client.as_ref())
            .map_or(0, |client| client.team)
    }
}

/// What the priorities read of the bot beyond its mind.
#[derive(Clone, Copy, Debug)]
pub struct ObjectiveBot<'a> {
    pub client: usize,
    pub body: BotBody<'a>,
    pub last_dead_time: i32,
    pub forced_role: i32,
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// `EntityVisibleBox`: a clear box to `to`, or one stopped by `wanted` itself.
fn entity_visible_box(
    from: [f32; 3],
    to: [f32; 3],
    ignore: i32,
    wanted: i32,
    world: &mut dyn BotMoveWorld,
) -> bool {
    let trace = world.trace(from, [-1.0; 3], [1.0; 3], to, ignore, MASK_SOLID);
    if trace.fraction == 1.0 && !trace.start_solid && !trace.all_solid {
        return true;
    }
    let hit = i32::from(trace.entity_number);
    hit != ENTITYNUM_NONE && hit == wanted
}

/// `Siege_TargetClosestObjective`: the destination kept if it is still a live objective
/// of `flag`, else the nearest one; then what to do with its entity — shoot it (it takes
/// damage, is under 1,024 units and seen; never with a melee weapon or saber), touch it
/// (it is usable and under 128), or neither. Whether it had one.
fn target_closest_objective(
    mind: &mut BotMind,
    bot: &ObjectiveBot,
    routes: &BotRoutes,
    flag: i32,
    game: &ObjectiveGame,
    world: &mut dyn BotMoveWorld,
) -> bool {
    let live = |point: usize| {
        let waypoint = &routes.waypoints[point];
        waypoint.flags & flag != 0
            && waypoint.associated_entity != ENTITYNUM_NONE
            && game
                .thing(waypoint.associated_entity)
                .is_some_and(|thing| thing.in_use && thing.usable)
    };
    if !mind.wp_destination.is_some_and(live) {
        let mut best = None;
        let mut best_distance = 999_999_999.9_f32;
        for point in 0..routes.waypoints.len() {
            if live(point) {
                let length = distance(routes.waypoints[point].origin, mind.origin);
                if length < best_distance {
                    best_distance = length;
                    best = Some(point);
                }
            }
        }
        let Some(best) = best else { return false };
        mind.wp_destination = Some(best);
    }
    let destination = &routes.waypoints[mind.wp_destination.unwrap()];
    let Some(goal) = game.thing(destination.associated_entity).copied() else {
        return false;
    };
    let length = distance(mind.origin, destination.origin);
    let centre = goal.centre();
    (mind.shoot_goal, mind.touch_goal) = if goal.takes_damage
        && length < GOAL_SHOOT
        && entity_visible_box(mind.origin, centre, bot.client as i32, goal.entity, world)
    {
        (Some(goal.entity), None)
    } else if goal.usable && length < GOAL_TRAVEL {
        (None, Some(goal.entity))
    } else {
        (None, None)
    };
    if matches!(
        weapon_range(bot.body.weapon),
        WeaponRange::Melee | WeaponRange::Saber
    ) {
        mind.shoot_goal = None;
    }
    if mind.touch_goal.is_some() {
        mind.goal_position = centre;
    }
    true
}

/// `Siege_DefendFromAttackers`: the point nearest the nearest living enemy that is not a
/// spectator, held for ten seconds.
fn defend_from_attackers(
    mind: &mut BotMind,
    bot: &ObjectiveBot,
    routes: &BotRoutes,
    game: &ObjectiveGame,
    world: &mut dyn BotMoveWorld,
) {
    let team = game.team_of(bot.client);
    let mut best = None;
    let mut best_distance = 999_999.0_f32;
    for client in game.clients.iter().flatten() {
        if client.team != team && client.health > 0 && client.team != TEAM_SPECTATOR {
            let length = distance(client.origin, mind.origin);
            if length < best_distance {
                best = Some(client.origin);
                best_distance = length;
            }
        }
    }
    let Some(origin) = best else { return };
    if let Some(point) = routes.nearest_visible(origin, -1, &mut RouteView(world)) {
        mind.wp_destination = Some(point);
        mind.destination_grab_time = (game.level_time + 10_000) as f32;
    }
}

/// A shooting goal kept only in the PVS and in sight (or stopping the trace itself).
fn keep_visible_shoot_goal(
    mind: &mut BotMind,
    bot: &ObjectiveBot,
    game: &ObjectiveGame,
    world: &mut dyn BotMoveWorld,
) {
    let Some(goal) = mind.shoot_goal.and_then(|goal| game.thing(goal)).copied() else {
        return;
    };
    let centre = goal.centre();
    if !world.in_pvs(mind.origin, centre) {
        mind.shoot_goal = None;
        return;
    }
    let trace = world.trace(
        mind.origin,
        [0.0; 3],
        [0.0; 3],
        centre,
        bot.client as i32,
        MASK_SOLID,
    );
    if trace.fraction != 1.0 && i32::from(trace.entity_number) != goal.entity {
        mind.shoot_goal = None;
    }
}

/// `SiegeTakesPriority`: in siege, always — first a weapon near after a pistol spawn;
/// then its role (its side attacking; otherwise defending, unless a third of its team
/// already defends), a forced role winning; then its side's nearest objective, or the
/// attackers held off.
pub fn siege_takes_priority(
    mind: &mut BotMind,
    bot: &ObjectiveBot,
    routes: &BotRoutes,
    game: &ObjectiveGame,
    world: &mut dyn BotMoveWorld,
    rng: &mut Rng,
    item_of: &dyn Fn(i32) -> Option<&'static Item>,
) -> bool {
    if game.gametype != GT_SIEGE || game.clients.get(bot.client).is_none_or(Option::is_none) {
        return false;
    }
    let level_time = game.level_time;
    let pistol = bot.body.weapon == WP_BRYAR_PISTOL;
    let mut keep_destination = None;
    if pistol && level_time - bot.last_dead_time < WEAPON_GATHER_TIME {
        if let Some(goal) = get_best_idle_goal(mind, routes, &bot.body, level_time, rng, item_of) {
            if mind.wp_dest_switch_time < level_time as f32 {
                mind.wp_destination = Some(goal);
            }
            return true;
        }
    } else if pistol
        && level_time - bot.last_dead_time < WEAPON_CHASE_TIME
        && mind
            .wp_destination
            .is_some_and(|point| routes.waypoints[point].weight != 0.0)
    {
        keep_destination = Some(mind.wp_destination);
    }
    let team = game.team_of(bot.client);
    let (attacking, flag) = if team == SIEGETEAM_TEAM1 {
        (game.attackers.0, WPFLAG_SIEGE_IMPERIALOBJ)
    } else {
        (game.attackers.1, WPFLAG_SIEGE_REBELOBJ)
    };
    if attacking {
        mind.siege_state = SIEGESTATE_ATTACKER;
    } else {
        mind.siege_state = SIEGESTATE_DEFENDER;
        // The bot's own slot counts with the role just set.
        let role = |index: usize, client: &ObjectiveClient| {
            if index == bot.client {
                client.bot_role.map(|_| mind.siege_state)
            } else {
                client.bot_role
            }
        };
        let defenders = game
            .clients
            .iter()
            .enumerate()
            .filter_map(|(index, client)| client.as_ref().map(|client| (index, client)))
            .filter(|(index, client)| {
                role(*index, client) == Some(SIEGESTATE_DEFENDER) && client.team == team
            })
            .count() as i32;
        let teammates = game
            .clients
            .iter()
            .flatten()
            .filter(|client| client.team == team)
            .count() as i32;
        if defenders > teammates / 3 && teammates > 1 {
            mind.siege_state = SIEGESTATE_ATTACKER;
        }
    }
    if bot.forced_role != 0 {
        mind.siege_state = bot.forced_role;
    }
    match mind.siege_state {
        SIEGESTATE_ATTACKER => {
            if !target_closest_objective(mind, bot, routes, flag, game, world) {
                defend_from_attackers(mind, bot, routes, game, world);
                keep_visible_shoot_goal(mind, bot, game, world);
            }
        }
        SIEGESTATE_DEFENDER => {
            defend_from_attackers(mind, bot, routes, game, world);
            keep_visible_shoot_goal(mind, bot, game, world);
        }
        _ => {
            target_closest_objective(mind, bot, routes, flag, game, world);
            keep_visible_shoot_goal(mind, bot, game, world);
        }
    }
    if let Some(destination) = keep_destination {
        mind.wp_destination = destination;
    }
    true
}

/// `JMTakesPriority`: in Jedi Master, unless the master itself — the point nearest the
/// master (or the saber lying free), grabbed every four seconds.
pub fn jm_takes_priority(
    mind: &mut BotMind,
    is_jedi_master: bool,
    routes: &BotRoutes,
    game: &ObjectiveGame,
    world: &mut dyn BotMoveWorld,
) -> bool {
    if game.gametype != GT_JEDIMASTER || is_jedi_master {
        return false;
    }
    let master = game
        .clients
        .iter()
        .position(|client| client.is_some_and(|client| client.in_use && client.is_jedi_master));
    mind.jm_state = master.map_or(-1, |master| master as i32);
    let important = match master {
        Some(master) => {
            let client = game.clients[master].unwrap();
            Some((master as i32, client.in_use, client.origin))
        }
        None => game.jedi_master_saber,
    };
    if let Some((entity, in_use, origin)) = important
        && in_use
        && mind.destination_grab_time < game.level_time as f32
        && let Some(point) = routes.nearest_visible(origin, entity, &mut RouteView(world))
    {
        mind.wp_destination = Some(point);
        mind.destination_grab_time = (game.level_time + 4000) as f32;
    }
    true
}
