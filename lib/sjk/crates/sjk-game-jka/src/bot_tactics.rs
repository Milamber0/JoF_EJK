//! A bot's small tactics within its frame (OpenJK `codemp/game/ai_main.c`): checking
//! the ground and walls it strafes along (`StrafeTracing`), whether it fires or holds
//! a charge (`PrimFiring`, `AltFiring`) and holding its fire (`KeepPrimFromFiring`,
//! `KeepAltFromFiring`) when a friend is in its line of fire (`CheckForFriendInLOF`),
//! closing on a dropped flag (`CTFFlagMovement`), and what it learns when it is hurt
//! (`BotDamageNotification`).

use crate::bot_ctf::LyingFlag;
use crate::bot_moves::{BotMoveWorld, MASK_SOLID};
use crate::bot_routes::BotRoutes;
use crate::bot_senses::{
    BotSenses, SensingBot, SensingRules, pass_loved_one_check, pass_standard_enemy_checks,
};
use crate::bot_squad::{SquadGame, love_level};
use crate::bot_think::BotMind;
use crate::player_death::Rng;
use crate::pmove::flight::flight_axes;

/// `MASK_PLAYERSOLID`.
const MASK_PLAYERSOLID: u32 = 0x1111;
/// `WEAPON_CHARGING`, `WEAPON_CHARGING_ALT`.
const WEAPON_CHARGING: i32 = 4;
const WEAPON_CHARGING_ALT: i32 = 5;
/// `BOT_FLAG_GET_DISTANCE`.
const BOT_FLAG_GET_DISTANCE: f32 = 256.0;
/// `ENEMY_FORGET_MS`.
const ENEMY_FORGET_MS: i32 = 10_000;
const GT_TEAM: i32 = 6;

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `StrafeTracing`: the side it strafes to (32 units right, or left for `meleeStrafeDir`)
/// is checked for a wall and for a ledge; either stops the strafe for 0.5 to 1.5
/// seconds (`meleeStrafeDisable`).
pub fn strafe_tracing(
    mind: &mut BotMind,
    client: i32,
    level_time: i32,
    world: &mut dyn BotMoveWorld,
    rng: &mut Rng,
) {
    let (mins, maxs) = ([-15.0, -15.0, -22.0], [15.0, 15.0, 32.0]);
    let right = flight_axes(mind.viewangles).1.to_array();
    let side = if mind.melee_strafe_dir != 0 {
        -32.0
    } else {
        32.0
    };
    let beside = [
        mind.origin[0] + right[0] * side,
        mind.origin[1] + right[1] * side,
        mind.origin[2] + right[2] * side,
    ];
    if world
        .trace(mind.origin, mins, maxs, beside, client, MASK_SOLID)
        .fraction
        != 1.0
    {
        mind.melee_strafe_disable = (level_time + rng.irand(500, 1500)) as f32;
    }
    let below = [beside[0], beside[1], beside[2] - 32.0];
    if world
        .trace(beside, [0.0; 3], [0.0; 3], below, client, MASK_SOLID)
        .fraction
        == 1.0
    {
        mind.melee_strafe_disable = (level_time + rng.irand(500, 1500)) as f32;
    }
}

/// `PrimFiring`: it fires, or lets a primary charge go.
pub fn prim_firing(mind: &BotMind, weapon_state: i32) -> bool {
    (weapon_state != WEAPON_CHARGING) == (mind.do_attack != 0)
}

/// `KeepPrimFromFiring`: the primary fire held — no shot, and a charge kept charging.
pub fn keep_prim_from_firing(mind: &mut BotMind, weapon_state: i32) {
    mind.do_attack = i32::from(weapon_state == WEAPON_CHARGING);
}

/// `AltFiring`: it alt-fires, or lets an alternate charge go.
pub fn alt_firing(mind: &BotMind, weapon_state: i32) -> bool {
    (weapon_state != WEAPON_CHARGING_ALT) == (mind.do_alt_attack != 0)
}

/// `KeepAltFromFiring`: the alternate fire held likewise.
pub fn keep_alt_from_firing(mind: &mut BotMind, weapon_state: i32) {
    mind.do_alt_attack = i32::from(weapon_state == WEAPON_CHARGING_ALT);
}

/// `CheckForFriendInLOF`: the client 2,048 units along its view (a 6-unit box from its
/// eye, `MASK_PLAYERSOLID`), when that is a teammate in a team game or a bot it loves
/// more than a little. `game` gives the clients by number; the world's trace answers
/// with the entity number, which counts only as one of them.
pub fn friend_in_line_of_fire(
    mind: &BotMind,
    client: usize,
    game: &SquadGame,
    world: &mut dyn BotMoveWorld,
) -> Option<usize> {
    let forward = flight_axes(mind.viewangles).0.to_array();
    let to = [
        mind.eye[0] + forward[0] * 2048.0,
        mind.eye[1] + forward[1] * 2048.0,
        mind.eye[2] + forward[2] * 2048.0,
    ];
    let trace = world.trace(
        mind.eye,
        [-3.0; 3],
        [3.0; 3],
        to,
        client as i32,
        MASK_PLAYERSOLID,
    );
    if trace.fraction == 1.0 {
        return None;
    }
    let hit = usize::from(trace.entity_number);
    let other = game.clients.get(hit)?.as_ref()?;
    if game.gametype >= GT_TEAM && game.same_team(client, hit) {
        return Some(hit);
    }
    if other.bot && love_level(mind, hit, game) > 1 {
        return Some(hit);
    }
    None
}

/// `CTFFlagMovement`: heading for a flag's point while that flag lies dropped within
/// reach (256 units) and in clear view (a 30×30×14 box), the bot goes for the flag
/// itself and remembers where it lay (`staticFlagSpot`). `wantFlag`, which the
/// reference only ever clears, is left out.
pub fn ctf_flag_movement(
    mind: &mut BotMind,
    client: i32,
    routes: &BotRoutes,
    dropped: [Option<LyingFlag>; 2],
    world: &mut dyn BotMoveWorld,
) {
    let (Some(red), Some(blue)) = routes.current_flags else {
        return;
    };
    let Some(destination) = mind
        .wp_destination
        .filter(|&point| point == red || point == blue)
    else {
        return;
    };
    // The blue flag's check comes second and wins when both points are the same.
    let mut desired = None;
    if destination == red
        && let Some(flag) = dropped[0].filter(|flag| flag.dropped)
    {
        desired = Some(flag);
    }
    if destination == blue
        && let Some(flag) = dropped[1].filter(|flag| flag.dropped)
    {
        desired = Some(flag);
    }
    let Some(flag) = desired else { return };
    if length(sub(mind.origin, flag.base)) <= BOT_FLAG_GET_DISTANCE {
        let trace = world.trace(
            mind.origin,
            [-15.0, -15.0, -7.0],
            [15.0, 15.0, 7.0],
            flag.base,
            client,
            MASK_SOLID,
        );
        if trace.fraction == 1.0 || i32::from(trace.entity_number) == flag.entity {
            mind.goal_position = flag.base;
            mind.static_flag_spot = flag.base;
        }
    }
}

/// `BotDamageNotification`: client `hurt` was hurt by client `attacker`. A bot attacker
/// alone has the right to it (`lastAttacked`, taken from every other bot); a human's
/// blow takes it from every bot. A hurt bot remembers who hurt it (`lastHurt`) and,
/// without an enemy, makes a fair enemy it does not love its enemy. Only clients count
/// (an NPC's slot is past `minds`).
pub fn damage_notification(
    minds: &mut [Option<BotMind>],
    hurt: usize,
    attacker: usize,
    senses: &dyn BotSenses,
    rules: &SensingRules,
) {
    if hurt >= minds.len() || attacker >= minds.len() || senses.client(attacker as i32).is_none() {
        return;
    }
    let bot_attacker = minds[attacker].is_some();
    for (slot, mind) in minds.iter_mut().enumerate() {
        if let Some(mind) = mind.as_mut()
            && !(bot_attacker && slot == attacker)
            && mind.last_attacked == Some(hurt as i32)
        {
            mind.last_attacked = None;
        }
    }
    if let Some(mind) = minds[attacker].as_mut() {
        mind.last_attacked = Some(hurt as i32);
    }
    let Some(mind) = minds[hurt].as_mut() else {
        return;
    };
    mind.last_hurt = Some(attacker as i32);
    if mind.current_enemy.is_some() {
        return;
    }
    let own = senses.client(hurt as i32);
    let bot = SensingBot {
        client: hurt as i32,
        duel_in_progress: own.is_some_and(|own| own.duel_in_progress),
        duel_index: own.map_or(0, |own| own.duel_index),
        is_jedi_master: own.is_some_and(|own| own.is_jedi_master),
    };
    if !pass_standard_enemy_checks(mind, &bot, senses, attacker as i32, rules) {
        return;
    }
    if pass_loved_one_check(&mind.loved, &bot, senses, attacker as i32, rules) {
        mind.current_enemy = Some(attacker as i32);
        mind.enemy_seen_time = (rules.level_time + ENEMY_FORGET_MS) as f32;
    }
}
