//! A bot's capture-the-flag priorities (OpenJK `codemp/game/ai_main.c`): its role
//! (`ctfState`) and where that sends it — its own flag to defend (`BotDefendFlag`), the
//! enemy's to take (`BotGetEnemyFlag`), a carrier of its flag to chase
//! (`BotGetFlagBack`), a teammate carrying the enemy's to guard (`BotGuardFlagCarrier`),
//! home with the flag (`BotGetFlagHome`) — the flags' points moved to where a dropped
//! flag lies (`GetNewFlagPoint`), all decided by `CTFTakesPriority`.

use crate::bot_moves::{BotMoveWorld, MASK_SOLID, RouteView};
use crate::bot_routes::BotRoutes;
use crate::bot_think::BotMind;
use crate::bot_trail::{BotBody, get_best_idle_goal, total_trail_distance};
use crate::items::Item;
use crate::player_death::Rng;

/// `ctfState`.
pub const CTFSTATE_ATTACKER: i32 = 1;
pub const CTFSTATE_DEFENDER: i32 = 2;
pub const CTFSTATE_RETRIEVAL: i32 = 3;
pub const CTFSTATE_GUARDCARRIER: i32 = 4;
pub const CTFSTATE_GETFLAGHOME: i32 = 5;
/// `BASE_GUARD_DISTANCE`, `BASE_GETENEMYFLAG_DISTANCE`, `BASE_FLAGWAIT_DISTANCE`.
const BASE_DISTANCE: f32 = 256.0;
/// `WP_KEEP_FLAG_DIST`.
const WP_KEEP_FLAG_DIST: f32 = 128.0;
/// `BOT_MAX_WEAPON_GATHER_TIME`, `BOT_MAX_WEAPON_CHASE_CTF`.
const WEAPON_GATHER_TIME: i32 = 1000;
const WEAPON_CHASE_TIME: i32 = 5000;
const TEAM_RED: i32 = 1;
const TEAM_BLUE: i32 = 2;
const GT_CTF: i32 = 8;
const GT_CTY: i32 = 9;
const WP_BRYAR_PISTOL: i32 = 4;

/// A client as the flag game reads it.
#[derive(Clone, Copy, Debug)]
pub struct CtfClient {
    pub team: i32,
    /// It carries the red flag, the blue flag.
    pub red_flag: bool,
    pub blue_flag: bool,
    pub origin: [f32; 3],
    /// A bot's role (`botstates[number]->ctfState`); `None` for a human.
    pub bot_role: Option<i32>,
}

/// A flag lying in the game (`droppedRedFlag`, `droppedBlueFlag`): its entity, whether it
/// is dropped (`FL_DROPPED_ITEM`), where it is.
#[derive(Clone, Copy, Debug)]
pub struct LyingFlag {
    pub entity: i32,
    pub dropped: bool,
    pub base: [f32; 3],
}

/// The flag game around the bot.
#[derive(Clone, Copy, Debug)]
pub struct CtfGame<'a> {
    pub gametype: i32,
    pub level_time: i32,
    /// The clients by number (`None` for a slot with no client).
    pub clients: &'a [Option<CtfClient>],
    /// The red and blue flags last dropped.
    pub dropped: [Option<LyingFlag>; 2],
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// The point of a flag (`red` or blue) to head for, and whether one is there: the bot's
/// destination is the point when it is over 256 units away.
fn head_for(mind: &mut BotMind, routes: &BotRoutes, red: bool) -> bool {
    let point = if red {
        routes.current_flags.0
    } else {
        routes.current_flags.1
    };
    let Some(point) = point else { return false };
    if distance(mind.origin, routes.waypoints[point].origin) > BASE_DISTANCE {
        mind.wp_destination = Some(point);
    }
    true
}

/// `BotDefendFlag` (and `BotGetFlagHome`): its own flag's point.
fn defend_flag(mind: &mut BotMind, routes: &BotRoutes, team: i32) -> bool {
    match team {
        TEAM_RED => head_for(mind, routes, true),
        TEAM_BLUE => head_for(mind, routes, false),
        _ => false,
    }
}

/// `BotGetEnemyFlag`: the other team's flag's point.
fn get_enemy_flag(mind: &mut BotMind, routes: &BotRoutes, team: i32) -> bool {
    match team {
        TEAM_RED => head_for(mind, routes, false),
        TEAM_BLUE => head_for(mind, routes, true),
        _ => false,
    }
}

/// `BotGetFlagBack` (`ours`) and `BotGuardFlagCarrier`: the first client carrying the
/// flag of its own team not on its team (or the enemy's flag on its team); while its
/// destination may switch, the point nearest the carrier, when the trail reaches it.
fn follow_carrier(
    mind: &mut BotMind,
    routes: &BotRoutes,
    bot: usize,
    game: &CtfGame,
    ours: bool,
    world: &mut dyn BotMoveWorld,
    rng: &mut Rng,
) -> bool {
    let team = game
        .clients
        .get(bot)
        .and_then(|client| client.as_ref())
        .map_or(0, |client| client.team);
    // Red for a red bot, blue for any other — the reference's own `else`.
    let red_flag = (team == TEAM_RED) == ours;
    let carrier = game.clients.iter().flatten().find(|client| {
        (if red_flag {
            client.red_flag
        } else {
            client.blue_flag
        }) && (client.team == team) != ours
    });
    let Some(carrier) = carrier else { return false };
    if mind.wp_dest_switch_time < game.level_time as f32 {
        let origin = carrier.origin;
        if let Some(point) = routes.nearest_visible(origin, 0, &mut RouteView(world))
            && let Some(current) = mind.wp_current
            && total_trail_distance(routes, current as i32, point as i32) != -1.0
        {
            mind.wp_destination = Some(point);
            mind.wp_dest_switch_time = (game.level_time + rng.irand(1000, 5000)) as f32;
        }
    }
    true
}

/// `GetNewFlagPoint`: the flag's point moved to the nearest one with a clear flat box to
/// where the dropped flag lies — unless its point is within 128 units and clear itself.
fn new_flag_point(
    routes: &mut BotRoutes,
    red: bool,
    flag: &LyingFlag,
    world: &mut dyn BotMoveWorld,
) {
    let Some(point) = (if red {
        routes.current_flags.0
    } else {
        routes.current_flags.1
    }) else {
        return;
    };
    let (mins, maxs) = ([-15.0, -15.0, -5.0], [15.0, 15.0, 5.0]);
    let clear = |world: &mut dyn BotMoveWorld, from: [f32; 3]| {
        world
            .trace(from, mins, maxs, flag.base, flag.entity, MASK_SOLID)
            .fraction
            == 1.0
    };
    let mut best_distance = distance(routes.waypoints[point].origin, flag.base);
    if best_distance <= WP_KEEP_FLAG_DIST && clear(world, routes.waypoints[point].origin) {
        return;
    }
    let mut best = None;
    for (index, waypoint) in routes.waypoints.iter().enumerate() {
        let length = distance(waypoint.origin, flag.base);
        if length < best_distance && clear(world, waypoint.origin) {
            best = Some(index);
            best_distance = length;
        }
    }
    if let Some(best) = best {
        if red {
            routes.current_flags.0 = Some(best);
        } else {
            routes.current_flags.1 = Some(best);
        }
    }
}

/// What `CTFTakesPriority` reads of the bot beyond its mind.
#[derive(Clone, Copy, Debug)]
pub struct CtfBot<'a> {
    /// Its slot.
    pub client: usize,
    pub body: BotBody<'a>,
    /// `lastDeadTime`, `state_Forced`.
    pub last_dead_time: i32,
    pub forced_role: i32,
}

/// `CTFTakesPriority`: in a flag game, whether its role decides where it goes — first
/// a weapon lying near after a pistol spawn; the flags' points moved to dropped flags
/// (or back); its role set by who carries what; then its role's destination.
pub fn ctf_takes_priority(
    mind: &mut BotMind,
    bot: &CtfBot,
    routes: &mut BotRoutes,
    game: &CtfGame,
    world: &mut dyn BotMoveWorld,
    rng: &mut Rng,
    item_of: &dyn Fn(i32) -> Option<&'static Item>,
) -> bool {
    if game.gametype != GT_CTF && game.gametype != GT_CTY {
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
    let team = game
        .clients
        .get(bot.client)
        .and_then(|client| client.as_ref())
        .map_or(0, |client| client.team);
    let (my_flag_red, enemy_flag_red) = (team == TEAM_RED, team != TEAM_RED);
    if routes.current_flags.0.is_none()
        || routes.current_flags.1.is_none()
        || routes.flag_entities.0.is_none()
        || routes.flag_entities.1.is_none()
    {
        return false;
    }
    for (index, red) in [(0, true), (1, false)] {
        match game.dropped[index].filter(|flag| flag.dropped) {
            Some(flag) => new_flag_point(routes, red, &flag, world),
            None => {
                if red {
                    routes.current_flags.0 = routes.red_flag;
                } else {
                    routes.current_flags.1 = routes.blue_flag;
                }
            }
        }
    }
    if mind.ctf_state == 0 {
        return false;
    }
    let (mut enemy_has_our_flag, mut on_my_team, mut attackers) = (false, 0, 0);
    for client in game.clients.iter().flatten() {
        let same_team = client.team == team;
        if (if my_flag_red {
            client.red_flag
        } else {
            client.blue_flag
        }) && !same_team
        {
            enemy_has_our_flag = true;
        }
        if same_team {
            on_my_team += 1;
        }
        match client.bot_role {
            Some(role) if role != CTFSTATE_ATTACKER && role != CTFSTATE_RETRIEVAL => {}
            _ => attackers += 1,
        }
    }
    let carrying_enemy_flag = bot
        .body
        .powerups
        .get(if enemy_flag_red { 4 } else { 5 })
        .is_some_and(|&until| until != 0);
    if carrying_enemy_flag {
        mind.ctf_state = if (on_my_team < 2 || attackers == 0) && enemy_has_our_flag {
            CTFSTATE_RETRIEVAL
        } else {
            CTFSTATE_GETFLAGHOME
        };
    } else if mind.ctf_state == CTFSTATE_GETFLAGHOME {
        mind.ctf_state = 0;
    }
    if bot.forced_role != 0 {
        mind.ctf_state = bot.forced_role;
    }
    let decided = match mind.ctf_state {
        CTFSTATE_DEFENDER | CTFSTATE_GETFLAGHOME => defend_flag(mind, routes, team),
        CTFSTATE_ATTACKER => get_enemy_flag(mind, routes, team),
        CTFSTATE_RETRIEVAL | CTFSTATE_GUARDCARRIER => {
            let found = follow_carrier(
                mind,
                routes,
                bot.client,
                game,
                mind.ctf_state == CTFSTATE_RETRIEVAL,
                world,
                rng,
            );
            if !found {
                mind.ctf_state = 0;
            }
            found
        }
        _ => false,
    };
    if decided && let Some(destination) = keep_destination {
        mind.wp_destination = destination;
    }
    decided
}
