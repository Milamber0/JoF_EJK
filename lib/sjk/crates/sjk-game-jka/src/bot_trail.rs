//! A bot's way along the level's waypoint trail (OpenJK `codemp/game/ai_main.c`): which
//! points it may take (`PassWayCheck`), how far the trail runs between two
//! (`TotalTrailDistance`), a shorter way through a point's neighbours
//! (`CheckForShorterRoutes`), what the point it heads for asks of it
//! (`WPConstantRoutine`) and what happens on reaching it (`WPTouchRoutine`), whether it
//! runs from its enemy (`BotIsAChickenWuss`), and the goal it picks with nothing better
//! to do (`BotHasAssociated`, `GetBestIdleGoal`).
//!
//! Many of the reference's times here are floats (`wpSeenTime`, `beStill`, ...), set from
//! and compared with the integer level time; they are kept as floats so that a long
//! level rounds them as the reference does.

use crate::bot_routes::{BotRoutes, WPFLAG_GOALPOINT, WPFLAG_JUMP, Waypoint};
use crate::bot_think::BotMind;
use crate::bot_weapons::{WeaponRange, weapon_range};
use crate::crt_rand::CrtRand;
use crate::items::{Item, Kind};
use crate::player_death::Rng;

/// `WPFLAG_DUCK`.
const WPFLAG_DUCK: i32 = 0x20;
/// `WPFLAG_SNIPEORCAMPSTAND`.
const WPFLAG_SNIPEORCAMPSTAND: i32 = 0x800;
/// `WPFLAG_SNIPEORCAMP`.
const WPFLAG_SNIPEORCAMP: i32 = 0x2000;
/// `WPFLAG_ONEWAY_FWD`.
const WPFLAG_ONEWAY_FWD: i32 = 0x4000;
/// `WPFLAG_ONEWAY_BACK`.
const WPFLAG_ONEWAY_BACK: i32 = 0x8000;
/// `WPFLAG_NOMOVEFUNC`.
const WPFLAG_NOMOVEFUNC: i32 = 0x20_0000;
/// `LEVELFLAG_IMUSTNTRUNAWAY`.
const LEVELFLAG_IMUSTNTRUNAWAY: i32 = 4;
/// `MAX_CHICKENWUSS_TIME`.
const MAX_CHICKENWUSS_TIME: i32 = 10_000;
/// `BOT_RUN_HEALTH`.
const BOT_RUN_HEALTH: i32 = 40;
/// `forceJumpStrength` by levitation level.
const FORCE_JUMP_STRENGTH: [f32; 4] = [225.0, 420.0, 590.0, 840.0];
const FP_LEVITATION: i32 = 1;
const FP_RAGE: i32 = 8;
const WP_STUN_BATON: i32 = 1;
const WP_MELEE: i32 = 2;
const WP_SABER: i32 = 3;
const WP_BRYAR_PISTOL: i32 = 4;
const GT_JEDIMASTER: i32 = 2;
const GT_SINGLE_PLAYER: i32 = 5;
const GT_CTF: i32 = 8;
const GT_CTY: i32 = 9;
/// `CTFSTATE_DEFENDER`.
const CTFSTATE_DEFENDER: i32 = 2;

/// What the trail's choices read of the bot's own player (`cur_ps`, its health).
#[derive(Clone, Copy, Debug)]
pub struct BotBody<'a> {
    /// `fd.forcePowerLevel[FP_LEVITATION]`.
    pub levitation: i32,
    /// `fd.forcePowersKnown`, `fd.forcePowersActive`.
    pub powers_known: i32,
    pub powers_active: i32,
    /// `fd.forceJumpCharge`.
    pub force_jump_charge: f32,
    /// `groundEntityNum != ENTITYNUM_NONE`.
    pub on_ground: bool,
    pub weapon: i32,
    /// `stats[STAT_WEAPONS]`, `stats[STAT_HOLDABLE_ITEMS]`.
    pub weapons: i32,
    pub holdables: i32,
    pub powerups: &'a [i32],
    pub ammo: &'a [i32],
    pub is_jedi_master: bool,
    pub electrify_time: i32,
    pub health: i32,
}

/// A client enemy as the fear reads it.
#[derive(Clone, Copy, Debug)]
pub struct EnemyClient {
    pub is_jedi_master: bool,
    /// It carries a flag (`PW_REDFLAG` or `PW_BLUEFLAG`).
    pub red_flag: bool,
    pub blue_flag: bool,
    pub weapon: i32,
}

/// The bot's enemy: its health, and what it is as a client (`None` for an entity that
/// is no client).
#[derive(Clone, Copy, Debug)]
pub struct BotEnemy {
    pub health: i32,
    pub client: Option<EnemyClient>,
}

/// The game around the bot.
#[derive(Clone, Copy, Debug)]
pub struct TrailGame {
    pub level_time: i32,
    pub gametype: i32,
    /// `gLevelFlags`.
    pub level_flags: i32,
    /// `bot_camp`.
    pub camp: bool,
}

/// `gWPArray[index]`, or none past the table (either end).
fn point(routes: &BotRoutes, index: i32) -> Option<&Waypoint> {
    usize::try_from(index)
        .ok()
        .and_then(|index| routes.waypoints.get(index))
}

/// An integer time as the float fields hold it.
fn at(time: i32) -> f32 {
    time as f32
}

/// `PassWayCheck`: whether the bot may take point `index` — none past the table, none
/// one-way against its direction, none a Force jump above its current point beyond its
/// levitation.
pub fn pass_way_check(mind: &BotMind, routes: &BotRoutes, index: i32, levitation: i32) -> bool {
    let Some(candidate) = point(routes, index) else {
        return false;
    };
    if mind.wp_direction != 0 && candidate.flags & WPFLAG_ONEWAY_FWD != 0 {
        return false;
    }
    if mind.wp_direction == 0 && candidate.flags & WPFLAG_ONEWAY_BACK != 0 {
        return false;
    }
    if let Some(current) = mind
        .wp_current
        .and_then(|current| routes.waypoints.get(current))
        && candidate.force_jump_to != 0
        && candidate.origin[2] > current.origin[2] + 64.0
        && levitation < candidate.force_jump_to
    {
        return false;
    }
    true
}

/// `TotalTrailDistance`: the trail's length from `start` to `end` in either direction, or
/// -1 past the table or through a point one-way against the way.
pub fn total_trail_distance(routes: &BotRoutes, start: i32, end: i32) -> f32 {
    let (from, to) = if start > end {
        (end, start)
    } else {
        (start, end)
    };
    let mut total = 0.0_f32;
    for index in from..to {
        let Some(waypoint) = point(routes, index) else {
            return -1.0;
        };
        if (end > start && waypoint.flags & WPFLAG_ONEWAY_BACK != 0)
            || (start > end && waypoint.flags & WPFLAG_ONEWAY_FWD != 0)
        {
            return -1.0;
        }
        total += waypoint.disttonext;
    }
    total
}

/// `CheckForShorterRoutes` on arriving at point `index`: the direction set towards the
/// destination, and (not within three seconds of the last switch) a neighbour whose trail
/// is 64 units shorter taken instead, charging a Force jump for one that needs it.
pub fn check_for_shorter_routes(
    mind: &mut BotMind,
    routes: &BotRoutes,
    index: usize,
    levitation: i32,
    level_time: i32,
) {
    let Some(destination) = mind.wp_destination else {
        return;
    };
    if index < destination {
        mind.wp_direction = 0;
    } else if index > destination {
        mind.wp_direction = 1;
    }
    if mind.wp_switch_time > at(level_time) {
        return;
    }
    let neighbours = &routes.waypoints[index].neighbours;
    if neighbours.is_empty() {
        return;
    }
    let (mut best_index, mut best_length) = (
        index as i32,
        total_trail_distance(routes, index as i32, destination as i32),
    );
    let mut force_jump = 0;
    for neighbour in neighbours {
        let length = total_trail_distance(routes, neighbour.num, destination as i32);
        if (length < best_length - 64.0 || best_length == -1.0)
            && levitation >= neighbour.force_jump_to
        {
            best_length = length;
            best_index = neighbour.num;
            force_jump = neighbour.force_jump_to;
        }
    }
    if best_index != index as i32 && best_index != -1 {
        mind.wp_current = usize::try_from(best_index)
            .ok()
            .filter(|&best| best < routes.waypoints.len());
        mind.wp_switch_time = at(level_time + 3000);
        if force_jump != 0 {
            mind.force_jump_charge_time = level_time + 1000;
            mind.be_still = at(level_time + 1000);
            mind.force_jumping = mind.force_jump_charge_time as f32;
        }
    }
}

/// `WPConstantRoutine`: while heading for its point — duck under a duck point; at a jump
/// point more than 40 units up, charge a Force jump (or, without levitation and more than
/// 64 up, give the point up and turn round); keep charging towards a Force-jump point.
pub fn wp_constant_routine(
    mind: &mut BotMind,
    routes: &BotRoutes,
    body: &BotBody,
    level_time: i32,
) {
    let Some(current) = mind
        .wp_current
        .and_then(|current| routes.waypoints.get(current))
    else {
        return;
    };
    let (flags, height, force_jump_to) = (current.flags, current.origin[2], current.force_jump_to);
    if flags & WPFLAG_DUCK != 0 {
        mind.duck_time = at(level_time + 100);
    }
    let strength = FORCE_JUMP_STRENGTH[body.levitation.clamp(0, 3) as usize];
    if flags & WPFLAG_JUMP != 0 {
        let mut rise = height - mind.origin[2] + 16.0;
        if mind.origin[2] + 16.0 >= height {
            rise = 0.0;
        }
        let levitates = body.powers_known & (1 << FP_LEVITATION) != 0;
        if rise > 40.0
            && levitates
            && (body.force_jump_charge < strength - 100.0 || !body.on_ground)
        {
            mind.force_jump_charge_time = level_time + 1000;
            if body.on_ground && mind.jump_prep < at(level_time - 300) {
                mind.jump_prep = at(level_time + 700);
            }
            mind.be_still = at(level_time + 300);
            mind.jump_time = 0.0;
            if mind.wp_seen_time < at(level_time + 600) {
                mind.wp_seen_time = at(level_time + 600);
            }
        } else if rise > 64.0 && !levitates {
            mind.wp_current = None;
            mind.wp_direction = i32::from(mind.wp_direction == 0);
            return;
        }
    }
    if force_jump_to != 0 && body.force_jump_charge < strength - 100.0 {
        mind.force_jump_charge_time = level_time + 200;
    }
}

/// `BotIsAChickenWuss`: 1 to run from the enemy, 2 while an earlier decision holds, 0 to
/// stand — by the level's rule, the game type (a Jedi Master's challenger, a flag
/// carrier), rage, health, the weapon in hand, a saber close by, lightning lately.
pub fn is_a_chicken_wuss(
    mind: &mut BotMind,
    body: &BotBody,
    enemy: Option<&BotEnemy>,
    game: &TrailGame,
) -> i32 {
    let level_time = game.level_time;
    if game.level_flags & LEVELFLAG_IMUSTNTRUNAWAY != 0 || game.gametype == GT_SINGLE_PLAYER {
        return 0;
    }
    let mut master_pass = false;
    if game.gametype == GT_JEDIMASTER && !body.is_jedi_master {
        let strong_master = enemy.is_some_and(|enemy| {
            enemy.client.is_some_and(|client| client.is_jedi_master) && enemy.health > 40
        });
        if strong_master && body.weapon < 11 {
            master_pass = true;
        } else {
            return 0;
        }
    }
    if !master_pass
        && game.gametype == GT_CTF
        && enemy
            .and_then(|enemy| enemy.client)
            .is_some_and(|client| client.red_flag || client.blue_flag)
    {
        return 0;
    }
    if mind.chicken_wuss_calculation_time > at(level_time) {
        return 2;
    }
    if body.powers_active & (1 << FP_RAGE) != 0 {
        return 0;
    }
    if game.gametype == GT_JEDIMASTER && !body.is_jedi_master {
        return 1;
    }
    mind.chicken_wuss_calculation_time = at(level_time + MAX_CHICKENWUSS_TIME);
    if body.health < BOT_RUN_HEALTH {
        return 1;
    }
    let range = weapon_range(body.weapon);
    if (range == WeaponRange::Melee || range == WeaponRange::Saber)
        && (range != WeaponRange::Saber || mind.saber_specialist == 0)
    {
        return 1;
    }
    if body.weapon == WP_BRYAR_PISTOL {
        return 1;
    }
    if enemy
        .and_then(|enemy| enemy.client)
        .is_some_and(|client| client.weapon == WP_SABER)
        && mind.frame_enemy_len < 512.0
        && body.weapon != WP_SABER
    {
        return 1;
    }
    if level_time - body.electrify_time < 16_000 {
        return 1;
    }
    mind.chicken_wuss_calculation_time = 0.0;
    0
}

/// `BotCTFGuardDuty`: a flag game's defender.
fn ctf_guard_duty(mind: &BotMind, gametype: i32) -> bool {
    (gametype == GT_CTF || gametype == GT_CTY) && mind.ctf_state == CTFSTATE_DEFENDER
}

/// `WPTouchRoutine` on reaching the point it headed for: ten more seconds to the next,
/// no use for four where no func may move, a plain jump at a jump point that is no Force
/// jump, camping at a camping point (a camper that fears its enemy, guards its flag or
/// always camps, with a gun: 30 to 45 seconds by the C library's `rand`), no camping
/// with a melee weapon, and the destination reached or a shorter way looked for.
pub fn wp_touch_routine(
    mind: &mut BotMind,
    routes: &BotRoutes,
    body: &BotBody,
    enemy: Option<&BotEnemy>,
    game: &TrailGame,
    rand: &mut CrtRand,
) {
    let Some(current) = mind.wp_current else {
        return;
    };
    let Some(waypoint) = routes.waypoints.get(current) else {
        return;
    };
    let level_time = game.level_time;
    mind.wp_travel_time = at(level_time + 10_000);
    if waypoint.flags & WPFLAG_NOMOVEFUNC != 0 {
        mind.no_use_time = level_time + 4000;
    }
    if waypoint.flags & WPFLAG_JUMP != 0 && waypoint.force_jump_to == 0 {
        mind.jump_time = at(level_time + 100);
    }
    let melee = matches!(body.weapon, WP_SABER | WP_MELEE | WP_STUN_BATON);
    let camps = mind.is_camper != 0
        && game.camp
        && (is_a_chicken_wuss(mind, body, enemy, game) != 0
            || ctf_guard_duty(mind, game.gametype)
            || mind.is_camper == 2)
        && waypoint.flags & (WPFLAG_SNIPEORCAMP | WPFLAG_SNIPEORCAMPSTAND) != 0
        && !melee;
    if camps {
        let last = if mind.wp_direction != 0 {
            current as i32 + 1
        } else {
            current as i32 - 1
        };
        if point(routes, last).is_some() && last != 0 && mind.is_camping < at(level_time) {
            mind.is_camping = at(level_time + rand.next() % 15_000 + 30_000);
            mind.wp_camping = Some(current);
            mind.wp_camping_to = Some(last as usize);
            mind.camp_standing = waypoint.flags & WPFLAG_SNIPEORCAMPSTAND != 0;
        }
    } else if melee && mind.is_camping > at(level_time) {
        mind.is_camping = 0.0;
        mind.wp_camping_to = None;
        mind.wp_camping = None;
    }
    if let Some(destination) = mind.wp_destination {
        if current == destination {
            mind.wp_destination = None;
            mind.destination_grab_time = at(level_time
                + if mind.running_like_a_sissy != 0 {
                    500
                } else {
                    3500
                });
        } else {
            check_for_shorter_routes(mind, routes, current, body.levitation, level_time);
        }
    }
}

/// `BotHasAssociated`: whether the bot already has what point `waypoint` is the goal for
/// (`item` of its entity, `None` for one that is no item); a point tied to nothing counts
/// as had.
pub fn has_associated(body: &BotBody, waypoint: &Waypoint, item: Option<&Item>) -> bool {
    if waypoint.associated_entity == 1023 {
        return true;
    }
    let Some(item) = item else { return false };
    let tag = item.tag;
    let bit = |mask: i32| (0..32).contains(&tag) && mask & (1 << tag) != 0;
    match item.kind {
        Kind::Weapon => bit(body.weapons),
        Kind::Holdable => bit(body.holdables),
        Kind::Powerup => usize::try_from(tag)
            .ok()
            .and_then(|tag| body.powerups.get(tag))
            .is_some_and(|&until| until != 0),
        Kind::Ammo => usize::try_from(tag)
            .ok()
            .and_then(|tag| body.ammo.get(tag))
            .is_some_and(|&ammo| ammo > 10),
        _ => false,
    }
}

/// `GetBestIdleGoal`: with nothing better to do, the heaviest goal point it does not
/// already have, its weight less a point per 10,000 units of trail; none while it
/// wanders (`randomNav`, redrawn every 5 to 15 seconds unless it always camps).
/// `item_of` names each entity's item.
pub fn get_best_idle_goal(
    mind: &mut BotMind,
    routes: &BotRoutes,
    body: &BotBody,
    level_time: i32,
    rng: &mut Rng,
    item_of: &dyn Fn(i32) -> Option<&'static Item>,
) -> Option<usize> {
    let current = mind.wp_current?;
    if mind.is_camper != 2 && mind.random_nav_time < level_time {
        mind.random_nav = i32::from(rng.irand(1, 10) < 5);
        mind.random_nav_time = level_time + rng.irand(5000, 15_000);
    }
    if mind.random_nav != 0 {
        return None;
    }
    let (mut highest, mut desired) = (0_i32, None);
    for (index, waypoint) in routes.waypoints.iter().enumerate() {
        if waypoint.flags & WPFLAG_GOALPOINT != 0
            && waypoint.weight > highest as f32
            && !has_associated(body, waypoint, item_of(waypoint.associated_entity))
        {
            let trail = total_trail_distance(routes, current as i32, index as i32) as i32;
            if trail != -1 {
                let weighed = (waypoint.weight - (trail / 10_000) as f32) as i32;
                if weighed > highest {
                    highest = weighed;
                    desired = Some(index);
                }
            }
        }
    }
    desired
}
