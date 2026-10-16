//! Where a bot wants to go (OpenJK `codemp/game/ai_main.c`'s `GetIdealDestination`):
//! away from the nearest danger, to its camping point, where its game mode's role sends
//! it, towards the one it hates, the leader it follows or its enemy (or away, afraid, to
//! a good item), and otherwise to the best item it lacks.

use crate::bot_ctf::{CtfBot, CtfGame, ctf_takes_priority};
use crate::bot_moves::{BotMoveWorld, RouteView};
use crate::bot_objectives::{ObjectiveBot, ObjectiveGame, jm_takes_priority, siege_takes_priority};
use crate::bot_routes::BotRoutes;
use crate::bot_senses::{BotSenses, SensingBot, SensingRules, get_nearest_bad_thing};
use crate::bot_think::BotMind;
use crate::bot_trail::{
    BotBody, BotEnemy, EnemyClient, TrailGame, get_best_idle_goal, is_a_chicken_wuss,
    total_trail_distance,
};
use crate::bot_weapons::{WeaponRange, weapon_range};
use crate::items::Item;
use crate::player_death::Rng;

const CON_CONNECTED: i32 = 2;
const GT_SINGLE_PLAYER: i32 = 5;

/// What `GetIdealDestination` reads beyond the bot's mind and the worlds.
pub struct Destination<'a> {
    pub bot: SensingBot,
    pub skill: f32,
    pub body: BotBody<'a>,
    pub last_dead_time: i32,
    pub sensing: SensingRules,
    pub trail: TrailGame,
    pub ctf: CtfGame<'a>,
    pub objectives: ObjectiveGame<'a>,
    pub item_of: &'a dyn Fn(i32) -> Option<&'static Item>,
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// The point nearest `origin` becomes the destination when the trail reaches it, held
/// for a draw of `hold` milliseconds (`Q_irand`, drawn only then).
fn head_near(
    mind: &mut BotMind,
    routes: &BotRoutes,
    origin: [f32; 3],
    world: &mut dyn BotMoveWorld,
    level_time: i32,
    hold: (i32, i32),
    rng: &mut Rng,
) {
    let Some(current) = mind.wp_current else {
        return;
    };
    if let Some(point) = routes.nearest_visible(origin, 0, &mut RouteView(world))
        && total_trail_distance(routes, current as i32, point as i32) != -1.0
    {
        mind.wp_destination = Some(point);
        mind.wp_dest_switch_time = (level_time + rng.irand(hold.0, hold.1)) as f32;
    }
}

/// The client `number` when it is alive and connected, and where it is.
fn living(senses: &dyn BotSenses, number: Option<i32>) -> Option<[f32; 3]> {
    let client = senses.client(number?)?;
    (client.health > 0 && client.connected == CON_CONNECTED).then_some(client.origin)
}

/// `GetIdealDestination`.
pub fn get_ideal_destination(
    mind: &mut BotMind,
    routes: &mut BotRoutes,
    context: &Destination,
    senses: &mut dyn BotSenses,
    world: &mut dyn BotMoveWorld,
    rng: &mut Rng,
) {
    let Some(current) = mind.wp_current else {
        return;
    };
    let level_time = context.sensing.level_time;
    let now = level_time as f32;
    let bad = if (level_time as f32 - mind.escape_dir_time) > 4000.0 {
        get_nearest_bad_thing(mind, &context.bot, context.skill, senses, &context.sensing)
    } else {
        None
    };
    let bad_thing = bad.and_then(|number| {
        senses
            .things()
            .iter()
            .find(|thing| thing.number == number)
            .copied()
    });
    mind.dangerous_object = bad_thing
        .filter(|thing| thing.in_use && thing.health > 0 && thing.takes_damage)
        .map(|thing| thing.number);
    if bad.is_none() && mind.wp_dest_ignore_time > now {
        return;
    }
    if bad.is_none() && mind.dont_go_back > now {
        if mind.wp_destination.is_some() {
            mind.wp_store_dest = mind.wp_destination;
        }
        mind.wp_destination = None;
        return;
    } else if bad.is_none() && mind.wp_store_dest.is_some() {
        mind.wp_destination = mind.wp_store_dest.take();
    }
    if bad.is_some() {
        mind.wp_camping = None;
    }
    if let Some(camp) = mind.wp_camping {
        mind.wp_destination = Some(camp);
        return;
    }
    if bad.is_none() {
        let ctf_bot = CtfBot {
            client: context.bot.client as usize,
            body: context.body,
            last_dead_time: context.last_dead_time,
            forced_role: mind.state_forced,
        };
        let objective_bot = ObjectiveBot {
            client: context.bot.client as usize,
            body: context.body,
            last_dead_time: context.last_dead_time,
            forced_role: mind.state_forced,
        };
        if ctf_takes_priority(
            mind,
            &ctf_bot,
            routes,
            &context.ctf,
            world,
            rng,
            context.item_of,
        ) {
            if mind.ctf_state != 0 {
                mind.running_to_escape_threat = 1;
            }
            return;
        } else if siege_takes_priority(
            mind,
            &objective_bot,
            routes,
            &context.objectives,
            world,
            rng,
            context.item_of,
        ) {
            if mind.siege_state != 0 {
                mind.running_to_escape_threat = 1;
            }
            return;
        } else if jm_takes_priority(
            mind,
            context.body.is_jedi_master,
            routes,
            &context.objectives,
            world,
        ) {
            mind.running_to_escape_threat = 1;
        }
    }
    if let Some(thing) = bad_thing {
        mind.running_like_a_sissy = level_time + 100;
        if mind.wp_destination.is_some() {
            mind.wp_store_dest = mind.wp_destination;
        }
        mind.wp_destination = None;
        let next = if mind.wp_direction != 0 {
            current as i32 + 1
        } else {
            current as i32 - 1
        };
        if let Some(other) = usize::try_from(next)
            .ok()
            .filter(|&next| next < routes.waypoints.len())
            && mind.escape_dir_time < now
        {
            let here = distance(thing.base, routes.waypoints[current].origin);
            let there = distance(thing.base, routes.waypoints[other].origin);
            if here < there {
                mind.wp_direction = i32::from(mind.wp_direction == 0);
                mind.wp_current = Some(other);
                mind.escape_dir_time = (level_time + rng.irand(500, 1000)) as f32;
            }
        }
        return;
    }
    let reach = match weapon_range(context.body.weapon) {
        WeaponRange::Melee | WeaponRange::Saber => 1.0,
        WeaponRange::Mid => 128.0,
        WeaponRange::Long => 300.0,
    };
    let switch_due = mind.wp_dest_switch_time < now;
    if let Some(origin) = living(senses, mind.revenge_enemy) {
        if switch_due {
            head_near(mind, routes, origin, world, level_time, (5000, 10_000), rng);
        }
    } else if let Some(origin) = living(senses, mind.squad_leader) {
        if switch_due {
            head_near(mind, routes, origin, world, level_time, (5000, 10_000), rng);
        }
    } else if let Some(enemy) = mind.current_enemy {
        let client = senses.client(enemy).cloned();
        let (origin, fear) = match &client {
            Some(client) => (
                client.origin,
                BotEnemy {
                    health: client.health,
                    client: Some(EnemyClient {
                        is_jedi_master: client.is_jedi_master,
                        red_flag: client.red_flag,
                        blue_flag: client.blue_flag,
                        weapon: client.weapon,
                    }),
                },
            ),
            None => {
                let thing = senses
                    .things()
                    .iter()
                    .find(|thing| thing.number == enemy)
                    .copied()
                    .unwrap_or_default();
                (
                    thing.origin,
                    BotEnemy {
                        health: thing.health,
                        client: None,
                    },
                )
            }
        };
        let chicken = is_a_chicken_wuss(mind, &context.body, Some(&fear), &context.trail);
        mind.running_to_escape_threat = chicken;
        if mind.frame_enemy_len < reach || (chicken != 0 && chicken != 2) {
            if mind.frame_enemy_len > 400.0 {
                if let Some(goal) = get_best_idle_goal(
                    mind,
                    routes,
                    &context.body,
                    level_time,
                    rng,
                    context.item_of,
                ) {
                    mind.wp_destination = Some(goal);
                }
            } else if current > 0 && current + 1 < routes.waypoints.len() {
                let ahead = distance(routes.waypoints[current + 1].origin, origin);
                let behind = distance(routes.waypoints[current - 1].origin, origin);
                mind.wp_destination = Some(if behind > ahead {
                    current - 1
                } else {
                    current + 1
                });
            }
        } else if chicken != 2 && switch_due {
            let hold = if context.sensing.gametype == GT_SINGLE_PLAYER {
                (300, 1000)
            } else {
                (1000, 5000)
            };
            head_near(mind, routes, origin, world, level_time, hold, rng);
        }
    }
    if mind.wp_destination.is_none()
        && mind.wp_dest_switch_time < now
        && let Some(goal) = get_best_idle_goal(
            mind,
            routes,
            &context.body,
            level_time,
            rng,
            context.item_of,
        )
    {
        mind.wp_destination = Some(goal);
    }
}
