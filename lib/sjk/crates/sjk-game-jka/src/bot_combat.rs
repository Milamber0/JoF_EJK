//! A bot's fighting (OpenJK `codemp/game/ai_main.c`): when it fires and how
//! (`CombatBotAI`), where it stands with a saber (`SaberCombatHandling`) and with a melee
//! weapon (`MeleeCombatHandling`), setting off its det pack (`BotCheckDetPacks`), a wall
//! close ahead (`BotSurfaceNear`) and the weapons a saber can block
//! (`BotWeaponBlockable`).

use crate::bot_moves::{BotMoveWorld, MASK_SOLID};
use crate::bot_senses::in_field_of_vision;
use crate::bot_think::BotMind;
use crate::bot_weapons::{AltFireState, WeaponRange, should_secondary_fire, weapon_range};
use crate::player_angle_math::{normalize, vector_angles};
use crate::player_death::Rng;
use crate::pmove::flight::flight_axes;
use crate::saber_rules::{in_kata, in_special};

/// `SABER_ATTACK_RANGE`, `MELEE_ATTACK_RANGE`.
const SABER_ATTACK_RANGE: f32 = 128.0;
const MELEE_ATTACK_RANGE: f32 = 256.0;
/// `BOT_PLANT_BLOW_DISTANCE`.
const PLANT_BLOW_DISTANCE: f32 = 256.0;
const WEAPON_CHARGING: i32 = 4;
const WEAPON_CHARGING_ALT: i32 = 5;
const WP_STUN_BATON: i32 = 1;
const WP_MELEE: i32 = 2;
const WP_SABER: i32 = 3;
const WP_DISRUPTOR: i32 = 6;
const WP_DEMP2: i32 = 9;
const WP_ROCKET_LAUNCHER: i32 = 11;
const WP_THERMAL: i32 = 12;
const WP_TRIP_MINE: i32 = 13;
const WP_DET_PACK: i32 = 14;
/// `LS_SPINATTACK_DUAL`, `LS_SPINATTACK`.
const LS_SPINATTACK_DUAL: u32 = 29;
const LS_SPINATTACK: u32 = 30;

/// The bot's enemy as its fighting reads it: where it is, and as a client its ground,
/// weapon and saber move.
#[derive(Clone, Copy, Debug)]
pub struct Foe {
    pub origin: [f32; 3],
    pub client: Option<FoeClient>,
}

/// A client enemy's state.
#[derive(Clone, Copy, Debug)]
pub struct FoeClient {
    pub on_ground: bool,
    pub weapon: i32,
    pub saber_move: u32,
}

/// The bot's own weapon (`cur_ps`).
#[derive(Clone, Copy, Debug)]
pub struct Arms<'a> {
    pub weapon: i32,
    pub weapon_state: i32,
    pub weapon_charge_time: i32,
    pub rocket_lock_time: f32,
    pub rocket_last_valid_time: f32,
    pub ammo: &'a [i32],
}

fn toward(from: [f32; 3], to: [f32; 3]) -> [f32; 3] {
    [to[0] - from[0], to[1] - from[1], to[2] - from[2]]
}

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// `CombatBotAI`: whether to attack (`doAttack`) or fire the alternate way
/// (`doAltAttack`) — a saber within 128, a melee weapon within 256, a gun with the enemy
/// in its field of view (narrower for the explosive ones, wide while charging, doubled up
/// close); the thermal thrown or charged by distance; the alternate fire's charge redrawn
/// (500 to 1,000 ms) when not charging. True when a charge was let go.
pub fn combat(
    mind: &mut BotMind,
    arms: &Arms,
    foe: Option<&Foe>,
    level_time: i32,
    rng: &mut Rng,
) -> bool {
    let Some(foe) = foe else { return false };
    let angles = vector_angles(toward(mind.eye, foe.origin));
    let distance = mind.frame_enemy_len;
    match weapon_range(arms.weapon) {
        WeaponRange::Saber => {
            if distance <= SABER_ATTACK_RANGE {
                mind.do_attack = 1;
            }
            return false;
        }
        WeaponRange::Melee => {
            if distance <= MELEE_ATTACK_RANGE {
                mind.do_attack = 1;
            }
            return false;
        }
        _ => {}
    }
    let charging = arms.weapon_state == WEAPON_CHARGING || arms.weapon_state == WEAPON_CHARGING_ALT;
    let mut fov = if arms.weapon == WP_THERMAL || arms.weapon == WP_ROCKET_LAUNCHER {
        if arms.weapon_state == WEAPON_CHARGING_ALT && arms.weapon == WP_ROCKET_LAUNCHER {
            60.0
        } else {
            40.0
        }
    } else {
        60.0
    };
    if charging {
        fov = 160.0;
    }
    if distance < 128.0 {
        fov *= 2.0;
    }
    if !in_field_of_vision(mind.viewangles, fov, angles) {
        return false;
    }
    let held = level_time - arms.weapon_charge_time;
    if arms.weapon == WP_THERMAL {
        if ((held as f32) < distance * 2.0 && held < 4000 && distance > 64.0) || !charging {
            if !charging {
                if distance > 512.0 && distance < 800.0 {
                    mind.do_alt_attack = 1;
                } else {
                    mind.do_attack = 1;
                }
            }
            if arms.weapon_state == WEAPON_CHARGING {
                mind.do_attack = 1;
            } else if arms.weapon_state == WEAPON_CHARGING_ALT {
                mind.do_alt_attack = 1;
            }
        }
        return false;
    }
    let secondary = should_secondary_fire(&AltFireState {
        ammo: arms.ammo,
        weapon: arms.weapon,
        weapon_state: arms.weapon_state,
        weapon_charge_time: arms.weapon_charge_time,
        rocket_lock_time: arms.rocket_lock_time,
        rocket_last_valid_time: arms.rocket_last_valid_time,
        alt_charge_time: mind.alt_charge_time,
        enemy_distance: distance,
        level_time,
    });
    if !charging {
        mind.alt_charge_time = rng.irand(500, 1000);
    }
    if secondary == 1 {
        mind.do_alt_attack = 1;
    } else if secondary == 0
        && (arms.weapon_state != WEAPON_CHARGING || mind.alt_charge_time > held)
    {
        mind.do_attack = 1;
    }
    secondary == 2
}

/// The height of the floor under `from` (a player-sized box dropped 4,096 units), whole
/// units, and whether the box started in a solid.
fn floor_under(from: [f32; 3], world: &mut dyn BotMoveWorld) -> (i32, bool) {
    let trace = world.trace(
        from,
        [-15.0, -15.0, -24.0],
        [15.0, 15.0, 32.0],
        [from[0], from[1], from[2] - 4096.0],
        -1,
        MASK_SOLID,
    );
    (
        trace.end_position[2] as i32,
        trace.start_solid || trace.all_solid,
    )
}

/// The strafe's direction flipped when its time is up (500 to 1,800 ms).
fn flip_strafe(mind: &mut BotMind, level_time: i32, rng: &mut Rng) {
    if mind.melee_strafe_time < level_time as f32 {
        mind.melee_strafe_dir = i32::from(mind.melee_strafe_dir == 0);
        mind.melee_strafe_time = (level_time + rng.irand(500, 1800)) as f32;
    }
}

/// The point halfway to the enemy along the line to it.
fn halfway(mind: &BotMind, foe: &Foe) -> [f32; 3] {
    let forward = flight_axes(vector_angles(toward(mind.origin, foe.origin)))
        .0
        .to_array();
    std::array::from_fn(|axis| mind.origin[axis] + forward[axis] * mind.frame_enemy_len / 2.0)
}

/// `MeleeCombatHandling`: the strafe flipped; on one floor with its enemy all the way,
/// go at it.
pub fn melee_handling(
    mind: &mut BotMind,
    foe: Option<&Foe>,
    level_time: i32,
    world: &mut dyn BotMoveWorld,
    rng: &mut Rng,
) {
    let Some(foe) = foe else { return };
    flip_strafe(mind, level_time, rng);
    let (enemy_floor, _) = floor_under(foe.origin, world);
    let (own_floor, _) = floor_under(mind.origin, world);
    let (middle_floor, _) = floor_under(halfway(mind, foe), world);
    if own_floor == enemy_floor && enemy_floor == middle_floor {
        mind.goal_position = foe.origin;
    }
}

/// `SaberCombatHandling`: the strafe flipped; on one floor with its enemy all the way —
/// jump at one leaping above, attack from afar or turn defence on and off close, stand
/// its ground within 54 or go at it; pause now and then; back away (64 units, 256 from a
/// kata or spin, never off a ledge) from a saber close by; press a saber at a distance.
/// Otherwise within 56 it attacks.
pub fn saber_handling(
    mind: &mut BotMind,
    client: i32,
    foe: Option<&Foe>,
    level_time: i32,
    world: &mut dyn BotMoveWorld,
    rng: &mut Rng,
) {
    let Some(foe) = foe else { return };
    let now = level_time;
    flip_strafe(mind, now, rng);
    let (mut enemy_floor, enemy_stuck) = floor_under(foe.origin, world);
    let own_floor = if enemy_stuck {
        enemy_floor = 1;
        2
    } else {
        let (floor, stuck) = floor_under(mind.origin, world);
        if stuck {
            enemy_floor = 1;
            2
        } else {
            floor
        }
    };
    let (middle_floor, _) = floor_under(halfway(mind, foe), world);
    let distance = mind.frame_enemy_len;
    if !(own_floor == enemy_floor && enemy_floor == middle_floor) {
        if distance <= 56.0 {
            mind.do_attack = 1;
            mind.saber_defending = 0;
        }
        return;
    }
    if foe.origin[2] > mind.origin[2] + 32.0 && foe.client.is_some_and(|client| !client.on_ground) {
        mind.jump_time = (now + 100) as f32;
    }
    if distance > 128.0 {
        mind.saber_defending = 0;
        mind.saber_defend_decide_time = now + rng.irand(1000, 2000);
    } else if mind.saber_defend_decide_time < now {
        mind.saber_defending = i32::from(mind.saber_defending == 0);
        mind.saber_defend_decide_time = now + rng.irand(500, 2000);
    }
    if distance < 54.0 {
        mind.goal_position = mind.origin;
        mind.saber_bf_time = 0;
    } else {
        mind.goal_position = foe.origin;
    }
    let Some(enemy) = foe.client else { return };
    let spinning = |saber_move: u32| {
        in_kata(saber_move) || saber_move == LS_SPINATTACK || saber_move == LS_SPINATTACK_DUAL
    };
    if !in_special(enemy.saber_move)
        && distance > 90.0
        && mind.saber_bf_time > now
        && mind.saber_b_time > now
        && mind.be_still < now as f32
        && mind.saber_s_time < now
    {
        mind.be_still = (now + rng.irand(500, 1000)) as f32;
        mind.saber_s_time = now + rng.irand(1200, 1800);
    } else if enemy.weapon == WP_SABER
        && distance < 80.0
        && ((rng.irand(1, 10) < 8 && mind.saber_bf_time < now)
            || mind.saber_b_time > now
            || spinning(enemy.saber_move))
    {
        let mut away = toward(foe.origin, mind.origin);
        normalize(&mut away);
        let ideal = if spinning(enemy.saber_move) { 256 } else { 64 };
        let mut step = 0;
        while step < ideal {
            mind.goal_position =
                std::array::from_fn(|axis| mind.origin[axis] + away[axis] * step as f32);
            if mind.saber_b_time < now {
                mind.saber_bf_time = now + rng.irand(900, 1300);
                mind.saber_b_time = now + rng.irand(300, 700);
            }
            let below = [
                mind.goal_position[0],
                mind.goal_position[1],
                mind.goal_position[2] - 64.0,
            ];
            if world
                .trace(
                    mind.goal_position,
                    [0.0; 3],
                    [0.0; 3],
                    below,
                    client,
                    MASK_SOLID,
                )
                .fraction
                == 1.0
            {
                mind.goal_position = foe.origin;
                break;
            }
            step += 64;
        }
    } else if enemy.weapon == WP_SABER && distance >= 75.0 {
        mind.saber_bf_time = now + rng.irand(700, 1300);
        mind.saber_b_time = 0;
    }
}

/// `BotCheckDetPacks`: its det pack (`base`) set off (`plantKillEmAll`) when its client
/// enemy — seen, or freshly planted for — is nearer the pack than it is, within 256 and
/// in sight of it.
pub fn check_det_packs(
    mind: &mut BotMind,
    pack: Option<[f32; 3]>,
    enemy: Option<(i32, [f32; 3])>,
    level_time: i32,
    world: &mut dyn BotMoveWorld,
) {
    let Some(base) = pack else { return };
    let Some((number, origin)) = enemy else {
        return;
    };
    if !mind.frame_enemy_vis && level_time - mind.plant_continue >= 5000 {
        return;
    }
    let enemy_distance = length(toward(base, origin));
    if enemy_distance > length(toward(base, mind.origin)) {
        return;
    }
    if enemy_distance < PLANT_BLOW_DISTANCE
        && world
            .trace(origin, [0.0; 3], [0.0; 3], base, number, MASK_SOLID)
            .fraction
            == 1.0
    {
        mind.plant_kill_em_all = level_time + 500;
    }
}

/// `BotSurfaceNear`: something solid within 64 units ahead of its view.
pub fn surface_near(mind: &BotMind, client: i32, world: &mut dyn BotMoveWorld) -> bool {
    let forward = flight_axes(mind.viewangles).0.to_array();
    let ahead = std::array::from_fn(|axis| mind.origin[axis] + forward[axis] * 64.0);
    world
        .trace(mind.origin, [0.0; 3], [0.0; 3], ahead, client, MASK_SOLID)
        .fraction
        != 1.0
}

/// `BotWeaponBlockable`: whether a saber can block the weapon's shots.
pub fn weapon_blockable(weapon: i32) -> bool {
    !matches!(
        weapon,
        WP_STUN_BATON
            | WP_MELEE
            | WP_DISRUPTOR
            | WP_DEMP2
            | WP_ROCKET_LAUNCHER
            | WP_THERMAL
            | WP_TRIP_MINE
            | WP_DET_PACK
    )
}
