//! How a walker moves (`codemp/game/WalkerNPC.c`): its speed gathered and lost
//! (`ProcessMoveCommands` — no strafing, walking at a quarter of its top speed), its yaw
//! turned toward its pilot's view and its pitch the pilot's (`WalkerYawAdjust`,
//! `ProcessOrientCommands`; an empty walker turns by its own command's strafe), and its
//! legs' animation (`AnimateVehicle`: walking, running, backing, standing — with its hatch
//! shut while somebody is inside).
//!
//! The move and orientation are shared (`bg`) and run in both the server's `Update` and a
//! client's prediction; the animation is the game's.

use crate::player_angle_math::{angle_subtract, normalized_angle};
use crate::pmove::MovementState;
use crate::pmove_anim::{
    AnimationLengths, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_FLAG_RESTART, SETANIM_LEGS,
};
use crate::vehicle::Vehicle;
use crate::vehicle_move::Steering;

/// `BUTTON_WALKING`; `ENTITYNUM_NONE`.
const BUTTON_WALKING: u16 = 16;
const ENTITYNUM_NONE: u16 = 1_023;
/// The walker's legs: `BOTH_STAND1` (occupied), `BOTH_STAND2` (empty), `BOTH_WALK1`,
/// `BOTH_RUN1`, `BOTH_WALKBACK1`.
const BOTH_STAND1: u16 = 915;
const BOTH_STAND2: u16 = 917;
const BOTH_WALK1: u16 = 1_102;
const BOTH_RUN1: u16 = 1_111;
const BOTH_WALKBACK1: u16 = 1_134;
/// The share of its top speed below which a walker walks.
const WALK_FRACTION: f32 = 0.275;

/// `ProcessMoveCommands` (`WalkerNPC.c:78-191`).
pub fn move_commands(
    vehicle: &mut Vehicle,
    state: &mut MovementState,
    steering: &Steering,
    move_dir: &mut [f32; 3],
) {
    let info = std::sync::Arc::clone(&vehicle.info);
    let modifier = vehicle.time_modifier;
    let speed_idle_dec = info.decel_idle * modifier;
    let (speed_idle, speed_min) = (info.speed_idle, info.speed_min);
    let mut speed_max = info.speed_max;
    let speed_inc = if state.vehicle_entity_num == 0 {
        // "drifts to a stop".
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
        crate::vehicle_move::coast(
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
        command.right_move = 0;
    }
    if steering.electrify_time > steering.command_time {
        speed_max *= 0.5;
    }
    let walk_max = speed_max * WALK_FRACTION;
    if vehicle.ucmd.buttons & BUTTON_WALKING != 0 && state.speed > walk_max {
        state.speed = walk_max;
    } else if state.speed > speed_max {
        state.speed = speed_max;
    } else if state.speed < speed_min {
        state.speed = speed_min;
    }
    if state.health <= 0 {
        // "don't keep moving while you're dying!"
        state.speed = 0.0;
    }
}

/// `ProcessOrientCommands` (`WalkerNPC.c:254-320`): a player pilot turns the walker by its
/// view (`WalkerYawAdjust`) and pitches it with its own; an empty walker, or an NPC's,
/// turns by its command's strafe — twice as fast for an NPC, and faster with speed.
pub fn orient_commands(vehicle: &mut Vehicle, state: &MovementState, steering: &Steering) {
    if steering.rider_player {
        yaw_adjust(vehicle, state, steering.rider_yaw);
        vehicle.orientation[0] = steering.rider_pitch;
        return;
    }
    let mut turn = vehicle.info.turning_speed;
    if !vehicle.info.turn_when_stopped && state.speed == 0.0 {
        turn = 0.0;
    }
    // The rider is the walker itself, an NPC's entity: "help NPCs out some".
    turn *= 2.0;
    if state.speed > 200.0 {
        turn += turn * state.speed / 200.0 * 0.05;
    }
    turn *= vehicle.time_modifier;
    if vehicle.ucmd.right_move < 0 {
        vehicle.orientation[1] += turn;
    } else if vehicle.ucmd.right_move > 0 {
        vehicle.orientation[1] -= turn;
    }
}

/// `WalkerYawAdjust` (`WalkerNPC.c:193-217`): toward the pilot's yaw, by as much as the
/// speed allows, no more than one and a half times the walker's turning speed.
fn yaw_adjust(vehicle: &mut Vehicle, state: &MovementState, rider_yaw: f32) {
    if state.speed == 0.0 {
        return;
    }
    let most = vehicle.info.turning_speed * 1.5;
    let difference = (angle_subtract(vehicle.orientation[1], rider_yaw)
        * (state.speed.abs() / vehicle.info.speed_max))
        .clamp(-most, most);
    vehicle.orientation[1] =
        normalized_angle(vehicle.orientation[1] - difference * (vehicle.time_modifier * 0.2));
}

/// `AnimateVehicle` (`WalkerNPC.c:324-441`) through `Vehicle_SetAnim` on the legs: none for
/// a dead walker.
pub fn animate(
    vehicle: &Vehicle,
    state: &mut MovementState,
    health: i32,
    lengths: &dyn AnimationLengths,
) {
    if health <= 0 {
        return;
    }
    let fraction = state.speed / vehicle.info.speed_max;
    let (animation, flags) = if fraction > 0.0 {
        let walking = vehicle.ucmd.buttons & BUTTON_WALKING != 0 || fraction < WALK_FRACTION;
        (
            if walking { BOTH_WALK1 } else { BOTH_RUN1 },
            SETANIM_FLAG_OVERRIDE,
        )
    } else if fraction < -0.018 {
        (BOTH_WALKBACK1, 0)
    } else {
        (
            if state.vehicle_entity_num != 0 {
                BOTH_STAND1
            } else {
                BOTH_STAND2
            },
            SETANIM_FLAG_RESTART | SETANIM_FLAG_HOLD,
        )
    };
    crate::pmove_anim::set_animation(state, SETANIM_LEGS, animation, flags, lengths);
}
