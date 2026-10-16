//! How a speeder and an animal steer and gather speed: the shared vehicle functions of
//! `codemp/game/SpeederNPC.c` and `AnimalNPC.c` that the game's `Update` and the client's
//! prediction both run (`ProcessMoveCommands`, `ProcessOrientCommands`), and
//! `BG_VehicleTurnRateForSpeed`.
//!
//! They work on the vehicle ([`Vehicle`]) and its parent's movement state; what else they
//! read of the world is handed in ([`Steering`]). A walker's are
//! [`crate::vehicle_walker`]'s; a fighter's [`crate::vehicle_fighter`]'s and
//! [`crate::vehicle_fighter_orient`]'s.

use crate::player_angle_math::{angle_subtract, normalized_angle};
use crate::pmove::MovementState;
use crate::vehicle::{Vehicle, flags};
use crate::vehicle_fields::kind;

/// `BUTTON_ALT_ATTACK`, `BUTTON_WALKING`.
const BUTTON_ALT_ATTACK: u16 = 128;
const BUTTON_WALKING: u16 = 16;
/// `EF_JETPACK_ACTIVE`.
pub const EF_JETPACK_ACTIVE: u32 = 1 << 11;
/// `ENTITYNUM_NONE`.
const ENTITYNUM_NONE: u16 = 1_023;
/// `WP_SABER`, `WP_MELEE`.
const WP_SABER: u8 = 3;
const WP_MELEE: u8 = 2;

/// What the steering reads beyond the vehicle and its movement state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steering {
    /// The time the vehicle's functions count by: `level.time` on the server,
    /// `pm->cmd.serverTime` on a client.
    pub time: i32,
    /// `pm->cmd.serverTime`: the speeder's electrified wobble reads it on either side.
    pub command_time: i32,
    /// The yaw the rider looks along (`riderPS->viewangles[YAW]`): the pilot's, or the
    /// vehicle's own without one; its pitch, which a walker takes; and whether the rider
    /// is a player pilot (`rider->s.number < MAX_CLIENTS`).
    pub rider_yaw: f32,
    pub rider_pitch: f32,
    pub rider_player: bool,
    /// `parentPS->electrifyTime`.
    pub electrify_time: i32,
    /// The pilot's weapon and whether its sabers are off, for the speeder's turbo: `None`
    /// without a pilot.
    pub pilot_weapon: Option<(u8, bool)>,
    /// What a fighter reads besides.
    pub fighter: crate::vehicle_fighter::FighterSteering,
}

/// What a vehicle's `ProcessMoveCommands` asks the game to play (`#ifdef _GAME`): a
/// fighter's take-off and turbo sounds (`G_EntitySound` on the parent), a speeder's turbo
/// effect at its exhausts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveCues {
    /// `soundTakeOff`.
    pub take_off: bool,
    /// `soundTurbo`.
    pub turbo: bool,
    /// `iTurboStartFX` from every exhaust.
    pub turbo_start: bool,
}

/// `ProcessMoveCommands` for the vehicle's kind: its speed gathered or lost, and what the
/// game is to play for it.
pub fn process_move_commands(
    vehicle: &mut Vehicle,
    state: &mut MovementState,
    steering: &Steering,
    move_dir: &mut [f32; 3],
) -> MoveCues {
    match vehicle.kind() {
        kind::SPEEDER => return speeder_move_commands(vehicle, state, steering),
        kind::ANIMAL => animal_move_commands(vehicle, state, steering, move_dir),
        kind::WALKER => crate::vehicle_walker::move_commands(vehicle, state, steering, move_dir),
        kind::FIGHTER => {
            return crate::vehicle_fighter::move_commands(vehicle, state, steering, move_dir);
        }
        _ => {}
    }
    MoveCues::default()
}

/// `ProcessOrientCommands` for the vehicle's kind: its yaw turned toward the rider's (a
/// fighter's whole orientation, which may also set the parent's view).
pub fn process_orient_commands(
    vehicle: &mut Vehicle,
    state: &mut MovementState,
    steering: &Steering,
) -> crate::vehicle_fighter_orient::OrientCues {
    match vehicle.kind() {
        kind::SPEEDER => speeder_orient_commands(vehicle, state, steering),
        kind::ANIMAL => animal_orient_commands(vehicle, state, steering),
        kind::WALKER => crate::vehicle_walker::orient_commands(vehicle, state, steering),
        kind::FIGHTER => {
            return crate::vehicle_fighter_orient::orient_commands(vehicle, state, steering);
        }
        _ => {}
    }
    Default::default()
}

/// `ProcessMoveCommands` (`SpeederNPC.c:88-273`).
fn speeder_move_commands(
    vehicle: &mut Vehicle,
    state: &mut MovementState,
    steering: &Steering,
) -> MoveCues {
    let mut cues = MoveCues::default();
    let info = std::sync::Arc::clone(&vehicle.info);
    let modifier = vehicle.time_modifier;
    let speed_inc = if vehicle.flags & flags::FLYING != 0 {
        info.acceleration * modifier * 0.4
    } else if state.vehicle_entity_num == 0 {
        0.0
    } else {
        info.acceleration * modifier
    };
    let speed_idle_dec = info.decel_idle * modifier;
    let time = steering.time;
    if vehicle.pilot.is_some()
        && vehicle.ucmd.buttons & BUTTON_ALT_ATTACK != 0
        && info.turbo_speed != 0.0
    {
        let pilot_may = steering.pilot_weapon.is_some_and(|(weapon, sabers_off)| {
            weapon == WP_MELEE || (weapon == WP_SABER && sabers_off)
        });
        if (steering.electrify_time > time || pilot_may)
            && time - vehicle.turbo_time > info.turbo_recharge
        {
            vehicle.turbo_time = time + info.turbo_duration;
            cues.turbo_start = info.turbo_start_fx != 0;
            state.speed = info.turbo_speed;
        }
    }
    if vehicle.flags & flags::SLIDE_BREAKING != 0 {
        if vehicle.ucmd.forward_move >= 0 {
            vehicle.flags &= !flags::SLIDE_BREAKING;
        }
        state.speed = 0.0;
    } else if time > vehicle.turbo_time
        && vehicle.flags & flags::FLYING == 0
        && vehicle.ucmd.forward_move < 0
        && vehicle.orientation[2].abs() > 25.0
    {
        vehicle.flags |= flags::SLIDE_BREAKING;
    }
    let speed_max = if time < vehicle.turbo_time {
        state.entity_flags |= EF_JETPACK_ACTIVE;
        info.turbo_speed
    } else {
        state.entity_flags &= !EF_JETPACK_ACTIVE;
        info.speed_max
    };
    let (speed_idle, speed_min) = (info.speed_idle, info.speed_min);
    let command = vehicle.ucmd;
    if state.speed != 0.0
        || state.ground_entity_number == ENTITYNUM_NONE
        || command.forward_move != 0
        || command.up_move > 0
    {
        coast(
            state,
            command.forward_move,
            speed_inc,
            speed_idle,
            speed_min,
            speed_idle_dec,
        );
    }
    if state.speed > speed_max {
        state.speed = speed_max;
    } else if state.speed < speed_min {
        state.speed = speed_min;
    }
    if steering.electrify_time > time {
        state.speed *= modifier / 60.0;
    }
    cues
}

/// The speed a command gathers or loses (`SpeederNPC.c:207-240`, `AnimalNPC.c:163-196`):
/// forward adds, back brakes (faster above the idle speed), nothing coasts to a stop.
pub(crate) fn coast(
    state: &mut MovementState,
    forward: i8,
    speed_inc: f32,
    speed_idle: f32,
    speed_min: f32,
    speed_idle_dec: f32,
) {
    if forward > 0 && speed_inc != 0.0 {
        state.speed += speed_inc;
    } else if forward < 0 {
        if state.speed > speed_idle {
            state.speed -= speed_inc;
        } else if state.speed > speed_min {
            state.speed -= speed_idle_dec;
        }
    } else if state.speed > 0.0 {
        state.speed -= speed_idle_dec;
        if state.speed < 0.0 {
            state.speed = 0.0;
        }
    } else if state.speed < 0.0 {
        state.speed += speed_idle_dec;
        if state.speed > 0.0 {
            state.speed = 0.0;
        }
    }
}

/// `ProcessOrientCommands` (`SpeederNPC.c:286-330`): the yaw turned toward the rider's,
/// by as much as the speed allows.
fn speeder_orient_commands(vehicle: &mut Vehicle, state: &MovementState, steering: &Steering) {
    turn_toward_rider(vehicle, state, steering.rider_yaw);
    if state.speed != 0.0 && steering.electrify_time > steering.command_time {
        // "do some crazy stuff": `sin` of a float's seconds, the rest a double's.
        let wobble = f64::from(steering.command_time as f32 / 1000.0).sin()
            * 3.0
            * f64::from(vehicle.time_modifier);
        vehicle.orientation[1] = (f64::from(vehicle.orientation[1]) + wobble) as f32;
    }
}

/// The turn both speeders and animals make (`SpeederNPC.c:305-322`, `AnimalNPC.c:246-266`).
fn turn_toward_rider(vehicle: &mut Vehicle, state: &MovementState, rider_yaw: f32) {
    let mut difference = angle_subtract(vehicle.orientation[1], rider_yaw);
    if state.speed == 0.0 {
        return;
    }
    let speed = state.speed.abs();
    let most = vehicle.info.turning_speed * 4.0;
    difference *= speed / vehicle.info.speed_max;
    if difference > most {
        difference = most;
    } else if difference < -most {
        difference = -most;
    }
    vehicle.orientation[1] =
        normalized_angle(vehicle.orientation[1] - difference * (vehicle.time_modifier * 0.2));
}

/// `ProcessMoveCommands` (`AnimalNPC.c:112-221`).
fn animal_move_commands(
    vehicle: &mut Vehicle,
    state: &mut MovementState,
    steering: &Steering,
    move_dir: &mut [f32; 3],
) {
    let info = std::sync::Arc::clone(&vehicle.info);
    let modifier = vehicle.time_modifier;
    let speed_idle_dec = info.decel_idle * modifier;
    let (speed_idle, speed_min) = (info.speed_idle, info.speed_min);
    let time = steering.time;
    if vehicle.pilot.is_some()
        && vehicle.ucmd.buttons & BUTTON_ALT_ATTACK != 0
        && info.turbo_speed != 0.0
        && time - vehicle.turbo_time > info.turbo_recharge
    {
        vehicle.turbo_time = time + info.turbo_duration;
        state.speed = info.turbo_speed;
    }
    let speed_max = if time < vehicle.turbo_time {
        info.turbo_speed
    } else {
        info.speed_max
    };
    let speed_inc = if state.vehicle_entity_num == 0 {
        // Drifts to a stop.
        *move_dir = [0.0; 3];
        state.speed = 0.0;
        speed_idle * modifier
    } else {
        info.acceleration * modifier
    };
    let command = &mut vehicle.ucmd;
    if state.speed != 0.0
        || state.ground_entity_number == ENTITYNUM_NONE
        || command.forward_move != 0
        || command.up_move > 0
    {
        coast(
            state,
            command.forward_move,
            speed_inc,
            speed_idle,
            speed_min,
            speed_idle_dec,
        );
    } else {
        command.forward_move = command.forward_move.max(0);
        command.up_move = command.up_move.max(0);
    }
    let walk_max = speed_max * 0.275;
    if time > vehicle.turbo_time
        && vehicle.ucmd.buttons & BUTTON_WALKING != 0
        && state.speed > walk_max
    {
        state.speed = walk_max;
    } else if state.speed > speed_max {
        state.speed = speed_max;
    } else if state.speed < speed_min {
        state.speed = speed_min;
    }
}

/// `ProcessOrientCommands` (`AnimalNPC.c:229-268`): the rider is the entity's owner, or
/// the animal itself.
fn animal_orient_commands(vehicle: &mut Vehicle, state: &MovementState, steering: &Steering) {
    turn_toward_rider(vehicle, state, steering.rider_yaw);
}
