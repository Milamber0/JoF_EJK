//! How a fighter flies (`codemp/game/FighterNPC.c`): what it finds under it
//! (`BG_FighterUpdate`: its gravity, the landing trace), whether it is landed, landing,
//! launching or hanging suspended, and its speed gathered and lost (`ProcessMoveCommands`:
//! hyperspace, the drop after a suspension, taking off and setting down, the turbo, its
//! damaged engine, the throttle, strafing, and — on the server — its pitch's pull on its
//! speed and its gravity).
//!
//! Its orientation and animation are [`crate::vehicle_fighter_orient`]'s. Everything here
//! is `bg` code, which the server's `Update` and a client's prediction both run; what only
//! the game does (`#ifdef _GAME`) is gated by [`FighterSteering::server`], and the sounds
//! it starts are left as [`MoveCues`] for the game to play.

use crate::pmove::MovementState;
use crate::vehicle::{LandTrace, Vehicle};
use crate::vehicle_move::{MoveCues, Steering};

/// `MIN_LANDING_SPEED`, `MIN_LANDING_SLOPE`, `MAX_STRAFE_TIME` (`bg_vehicles.h:406-408`).
pub const MIN_LANDING_SPEED: f32 = 200.0;
pub const MIN_LANDING_SLOPE: f32 = 0.8;
pub const MAX_STRAFE_TIME: f32 = 2_000.0;
/// `HYPERSPACE_TIME`, `HYPERSPACE_TELEPORT_FRAC`, `HYPERSPACE_SPEED` (`bg_public.h:1808-1810`).
pub const HYPERSPACE_TIME: i32 = 4_000;
pub const HYPERSPACE_TELEPORT_FRAC: f32 = 0.75;
const HYPERSPACE_SPEED: f32 = 10_000.0;
/// `FIGHTER_MIN_TAKEOFF_FRACTION`.
const FIGHTER_MIN_TAKEOFF_FRACTION: f32 = 0.7;
/// `EF_JETPACK_ACTIVE`: the turbo's exhaust, for the client to draw.
const EF_JETPACK_ACTIVE: u32 = 1 << 11;
/// `ENTITYNUM_NONE`.
const ENTITYNUM_NONE: u16 = 1_023;
/// `SHIPSURF_DAMAGE_BACK_LIGHT`, `SHIPSURF_DAMAGE_BACK_HEAVY`: the engine's damage bits in
/// `brokenLimbs`.
const DAMAGE_BACK_LIGHT: u8 = 1 << 1;
const DAMAGE_BACK_HEAVY: u8 = 1 << 5;
/// `(MASK_NPCSOLID & ~CONTENTS_BODY)`: what the landing trace stops at.
pub const LANDING_MASK: u32 = 0x1 | 0x20 | 0x1000;

/// What a fighter's steering reads of its parent and of the game, beyond [`Steering`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FighterSteering {
    /// The game's side (`_GAME`): only the server checks who is aboard, the suspension and
    /// space, and pulls the speed and gravity by the pitch; a client's prediction does not.
    pub server: bool,
    /// `FighterIsInSpace`: the parent in a `trigger_space` (`client->inSpaceIndex`).
    pub in_space: bool,
    /// The parent's `SUSPENDED` spawnflag (2).
    pub suspended: bool,
    /// The parent's entity number, which picks how a damaged fighter spirals.
    pub parent_number: u16,
    /// `EF2_HYPERSPACE` on the parent: it faces its hyperspace point.
    pub hyperspace: bool,
    /// `EF_DEAD` on the parent.
    pub dead: bool,
    /// `BG_UnrestrainedPitchRoll` for the rider: `bg_fighterAltControl` and a player pilot.
    pub unrestrained: bool,
    /// The rider's roll, which an unrestrained fighter takes with the rest of its view.
    pub rider_roll: f32,
    /// `m_pPilot && m_pPilot->s.number < MAX_CLIENTS`: a player flies it.
    pub pilot_player: bool,
}

/// `BG_FighterUpdate` (`FighterNPC.c:47-116`): no gravity with a pilot aboard (the
/// definition's, or `gravity`, without one), and the landing trace — `trace` from the
/// parent's origin down its `landingHeight` with its box, through `LANDING_MASK`. The
/// server also ghosts the riders (the caller's).
pub fn update(
    vehicle: &mut Vehicle,
    state: &mut MovementState,
    gravity: f32,
    mins: [f32; 3],
    maxs: [f32; 3],
    trace: &mut dyn FnMut(
        [f32; 3],
        [f32; 3],
        [f32; 3],
        [f32; 3],
        u32,
    ) -> crate::pmove::MovementTrace,
) {
    state.gravity = if vehicle.pilot.is_some() {
        0.0
    } else if vehicle.info.gravity != 0 {
        vehicle.info.gravity as f32
    } else {
        gravity as i32 as f32
    };
    let mut bottom = state.origin;
    bottom[2] -= vehicle.info.landing_height;
    let found = trace(state.origin, mins, maxs, bottom, LANDING_MASK);
    vehicle.land_trace = LandTrace {
        fraction: found.fraction,
        normal: found.plane_normal,
    };
}

/// `PredictedAngularDecrement` (`FighterNPC.c:165-201`): `original` eased toward zero by a
/// step its size, `scale` and the frame's time set — never less than a tenth of the frame.
pub fn predicted_angular_decrement(scale: f32, time_modifier: f32, original: f32) -> f32 {
    let mut step = original * 0.05;
    if step < 0.0 {
        step = -step;
    }
    step *= 1.0 + (1.0 - scale);
    if step < 0.1 {
        step = 0.1;
    }
    step *= time_modifier * 0.1;
    if original > 0.0 {
        (original - step).max(0.0)
    } else if original < 0.0 {
        (original + step).min(0.0)
    } else {
        0.0
    }
}

/// `FighterOverValidLandingSurface`: ground within the landing height, flat enough.
pub fn over_valid_landing_surface(vehicle: &Vehicle) -> bool {
    vehicle.land_trace.fraction < 1.0 && vehicle.land_trace.normal[2] >= MIN_LANDING_SLOPE
}

/// `FighterIsLanded`: over ground it can land on, stopped.
pub fn is_landed(vehicle: &Vehicle, state: &MovementState) -> bool {
    over_valid_landing_surface(vehicle) && state.speed == 0.0
}

/// `FighterIsLanding`: over ground it can land on, slowing or crouching, slow enough — and,
/// on the server, with someone aboard.
pub fn is_landing(vehicle: &Vehicle, state: &MovementState, fighter: &FighterSteering) -> bool {
    over_valid_landing_surface(vehicle)
        && (!fighter.server || vehicle.inhabited())
        && (vehicle.ucmd.forward_move < 0 || vehicle.ucmd.up_move < 0)
        && state.speed <= MIN_LANDING_SPEED
}

/// `FighterIsLaunching`: over ground it can land on, climbing, under 200 — and, on the
/// server, with someone aboard.
pub fn is_launching(vehicle: &Vehicle, state: &MovementState, fighter: &FighterSteering) -> bool {
    over_valid_landing_surface(vehicle)
        && (!fighter.server || vehicle.inhabited())
        && vehicle.ucmd.up_move > 0
        && state.speed <= 200.0
}

/// `FighterSuspended`: an empty, stopped fighter with its `SUSPENDED` spawnflag, not asked
/// forward — only ever on the server.
pub fn is_suspended(vehicle: &Vehicle, state: &MovementState, fighter: &FighterSteering) -> bool {
    fighter.server
        && vehicle.pilot.is_none()
        && state.speed == 0.0
        && vehicle.ucmd.forward_move <= 0
        && fighter.suspended
}

/// `ProcessMoveCommands` (`FighterNPC.c:293-779`); `move_dir` is the parent's `ps.moveDir`.
pub fn move_commands(
    vehicle: &mut Vehicle,
    state: &mut MovementState,
    steering: &Steering,
    move_dir: &[f32; 3],
) -> MoveCues {
    let info = std::sync::Arc::clone(&vehicle.info);
    let fighter = &steering.fighter;
    let modifier = vehicle.time_modifier;
    let time = steering.command_time;
    let mut cues = MoveCues::default();
    if vehicle.hyperspace_time != 0 && time - vehicle.hyperspace_time < HYPERSPACE_TIME {
        // "Going to Hyperspace": the movement overridden.
        let fraction = (time - vehicle.hyperspace_time) as f32 / HYPERSPACE_TIME as f32;
        if fraction < HYPERSPACE_TELEPORT_FRAC {
            // Waiting to face the point, then at once at hyperspace speed.
            state.speed = if fighter.hyperspace {
                HYPERSPACE_SPEED
            } else {
                0.0
            };
        } else {
            state.speed = 200.0
                + (1.0 - fraction) * (1.0 / HYPERSPACE_TELEPORT_FRAC) * (HYPERSPACE_SPEED - 200.0);
            let velocity = state.velocity;
            if (velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2])
                .sqrt()
                < state.speed
            {
                state.velocity = move_dir.map(|axis| axis * state.speed);
            }
        }
        return cues;
    }
    if vehicle.drop_time >= time {
        // "no speed, just drop".
        state.speed = 0.0;
        state.gravity = 800.0;
        return cues;
    }
    let landing_or_launching =
        is_landing(vehicle, state, fighter) || is_launching(vehicle, state, fighter);
    if landing_or_launching
        && (vehicle.ucmd.forward_move <= 0
            || vehicle.land_trace.fraction <= FIGHTER_MIN_TAKEOFF_FRACTION)
    {
        // Near the ground: only up and down.
        if vehicle.ucmd.up_move > 0 {
            if state.velocity[2] <= 0.0 && info.sound_take_off != 0 {
                cues.take_off = true;
            }
            state.velocity[2] += info.acceleration * modifier;
        } else if vehicle.ucmd.up_move < 0 {
            state.velocity[2] -= info.acceleration * modifier;
        } else if vehicle.ucmd.forward_move < 0 {
            if vehicle.land_trace.fraction != 0.0 {
                state.velocity[2] -= info.acceleration * modifier;
            }
            if vehicle.land_trace.fraction <= FIGHTER_MIN_TAKEOFF_FRACTION {
                state.velocity[2] = predicted_angular_decrement(
                    vehicle.land_trace.fraction,
                    modifier * 5.0,
                    state.velocity[2],
                );
                state.speed = 0.0;
            }
        }
        // "Make sure they don't pitch as they near the ground."
        vehicle.orientation[0] =
            predicted_angular_decrement(0.7, modifier * 10.0, vehicle.orientation[0]);
        return cues;
    }
    if vehicle.ucmd.up_move > 0
        && info.turbo_speed != 0.0
        && time - vehicle.turbo_time > info.turbo_recharge
    {
        vehicle.turbo_time = time + info.turbo_duration;
        cues.turbo = info.sound_turbo != 0;
    }
    let mut speed_inc = info.acceleration * modifier;
    let mut speed_max = if time < vehicle.turbo_time {
        // "no no no! this would el breako el predictiono!": twice the acceleration, by the
        // frame's time, the throttle forced open and the exhaust drawn.
        speed_inc = (info.acceleration * 2.0) * modifier;
        vehicle.ucmd.forward_move = 127;
        state.entity_flags |= EF_JETPACK_ACTIVE;
        info.turbo_speed
    } else {
        state.entity_flags &= !EF_JETPACK_ACTIVE;
        info.speed_max
    };
    let mut speed_idle_dec = info.decel_idle * modifier;
    let speed_idle = info.speed_idle;
    let speed_idle_accel = info.accel_idle * modifier;
    let speed_min = info.speed_min;
    if state.broken_limbs & DAMAGE_BACK_HEAVY != 0 {
        speed_max *= 0.8;
    } else if state.broken_limbs & DAMAGE_BACK_LIGHT != 0 {
        speed_max *= 0.6;
    }
    let inhabited = vehicle.inhabited();
    if vehicle.removed_surfaces != 0 || steering.electrify_time >= time {
        // Out of control.
        state.speed += speed_inc;
        vehicle.ucmd.forward_move = 127;
    } else if is_suspended(vehicle, state, fighter) {
        state.speed = 0.0;
        vehicle.ucmd.forward_move = 0;
    } else if fighter.server && !inhabited && state.speed > 0.0 {
        // "pilot jumped out while we were moving forward": the throttle stays locked.
        vehicle.ucmd.forward_move = 127;
    } else if (state.speed != 0.0
        || state.ground_entity_number == ENTITYNUM_NONE
        || vehicle.ucmd.forward_move != 0
        || vehicle.ucmd.up_move > 0)
        && vehicle.land_trace.fraction >= 0.05
    {
        if vehicle.ucmd.forward_move > 0 && speed_inc != 0.0 {
            state.speed += speed_inc;
            vehicle.ucmd.forward_move = 127;
        } else if vehicle.ucmd.forward_move < 0 || vehicle.ucmd.up_move < 0 {
            // Slowing, or braking (trying to land?), faster.
            if vehicle.ucmd.up_move < 0 {
                if vehicle.ucmd.forward_move != 0 {
                    speed_inc += info.braking;
                    speed_idle_dec += info.braking;
                } else {
                    speed_inc = info.braking;
                    speed_idle_dec = info.braking;
                }
            }
            if state.speed > speed_idle {
                state.speed -= speed_inc;
            } else if state.speed > speed_min {
                if over_valid_landing_surface(vehicle) {
                    state.speed -= speed_inc;
                } else {
                    state.speed -= speed_idle_dec;
                    if state.speed < MIN_LANDING_SPEED {
                        // No dead stop in mid-air.
                        state.speed = MIN_LANDING_SPEED;
                    }
                }
            }
            if info.kind == crate::vehicle_fields::kind::FIGHTER {
                vehicle.ucmd.forward_move = 127;
            } else if speed_min >= 0.0 {
                vehicle.ucmd.forward_move = 0;
            }
        } else if info.throttle_sticks != 0.0 {
            // A throttle that sticks where it is.
            if state.speed <= MIN_LANDING_SPEED {
                if over_valid_landing_surface(vehicle) {
                    if state.speed > 0.0 {
                        state.speed -= speed_idle_dec;
                    } else if state.speed < 0.0 {
                        state.speed += speed_idle_dec;
                    }
                } else if state.speed < speed_idle {
                    state.speed = (state.speed + speed_idle_accel).min(speed_idle);
                }
            }
        } else if (vehicle.land_trace.fraction >= 1.0
            || vehicle.land_trace.normal[2] < MIN_LANDING_SLOPE)
            && speed_idle > 0.0
        {
            // Launched: toward the idle speed.
            if state.speed < speed_idle {
                state.speed += speed_idle_accel;
                if state.speed > speed_idle {
                    state.speed = speed_idle;
                }
            } else if state.speed > 0.0 {
                state.speed -= speed_idle_dec;
                if state.speed < speed_idle {
                    state.speed = speed_idle;
                }
            }
        } else if state.speed > 0.0 {
            state.speed -= speed_idle_dec;
        } else if state.speed < 0.0 {
            state.speed += speed_idle_dec;
        }
    } else {
        vehicle.ucmd.forward_move = vehicle.ucmd.forward_move.max(0);
        vehicle.ucmd.up_move = vehicle.ucmd.up_move.max(0);
    }
    strafe(vehicle, state, steering, speed_max, inhabited);
    if state.speed > speed_max {
        state.speed = speed_max;
    } else if state.speed < speed_min {
        state.speed = speed_min;
    }
    if fighter.server {
        pull_of_pitch(vehicle, state, steering, speed_idle);
    } else {
        state.gravity = 0.0;
    }
    cues
}

/// The strafe (`FighterNPC.c:622-694`): a sideways push while the strafe key is held, for
/// at most two seconds each way (`hackingTime` counts them), and its count run back down
/// otherwise.
fn strafe(
    vehicle: &mut Vehicle,
    state: &mut MovementState,
    steering: &Steering,
    speed_max: f32,
    inhabited: bool,
) {
    let info = &vehicle.info;
    let modifier = vehicle.time_modifier;
    let fighter = &steering.fighter;
    let step = 50.0 * modifier;
    if info.strafe_perc != 0.0
        && (!fighter.server || inhabited)
        && vehicle.removed_surfaces == 0
        && steering.electrify_time < steering.command_time
        && (vehicle.land_trace.fraction >= 1.0
            || vehicle.land_trace.normal[2] < MIN_LANDING_SLOPE
            || state.speed > MIN_LANDING_SPEED)
        && vehicle.ucmd.right_move != 0
    {
        let mut strafe_speed = (info.strafe_perc * speed_max) * 5.0;
        let right = crate::pmove::flight::flight_axes([0.0, vehicle.orientation[1], 0.0])
            .1
            .to_array();
        let across = state.velocity[0] * right[0]
            + state.velocity[1] * right[1]
            + state.velocity[2] * right[2];
        if vehicle.ucmd.right_move > 0 {
            if state.hacking_time as f32 > -MAX_STRAFE_TIME {
                if across > 0.0 {
                    strafe_speed -= across;
                }
                if strafe_speed > 0.0 {
                    let push = strafe_speed * modifier;
                    state.velocity =
                        std::array::from_fn(|axis| state.velocity[axis] + right[axis] * push);
                }
                state.hacking_time = (state.hacking_time as f32 - step) as i32;
            }
        } else if (state.hacking_time as f32) < MAX_STRAFE_TIME {
            if across < 0.0 {
                strafe_speed += across;
            }
            if strafe_speed > 0.0 {
                let push = -strafe_speed * modifier;
                state.velocity =
                    std::array::from_fn(|axis| state.velocity[axis] + right[axis] * push);
            }
            state.hacking_time = (state.hacking_time as f32 + step) as i32;
        }
    } else if state.hacking_time > 0 {
        state.hacking_time = (state.hacking_time as f32 - step) as i32;
        if state.hacking_time < 0 {
            state.hacking_time = 0;
        }
    } else if state.hacking_time < 0 {
        state.hacking_time = (state.hacking_time as f32 + step) as i32;
        if state.hacking_time > 0 {
            state.hacking_time = 0;
        }
    }
}

/// The server's end of the move (`FighterNPC.c:708-772`): pitched down, faster and faster
/// over a planet; spiralling down with parts missing or electrified; no gravity suspended;
/// slow and not climbing, it sinks (not in space, unless there is ground to land on).
fn pull_of_pitch(
    vehicle: &mut Vehicle,
    state: &mut MovementState,
    steering: &Steering,
    speed_idle: f32,
) {
    let fighter = &steering.fighter;
    let modifier = vehicle.time_modifier;
    let pitch = vehicle.orientation[0];
    if pitch * 0.1 > 10.0 && !fighter.in_space {
        let multiplier = (pitch * 0.1).max(1.0);
        state.speed = predicted_angular_decrement(multiplier, modifier * 10.0, state.speed);
    }
    if vehicle.removed_surfaces != 0 || steering.electrify_time >= steering.command_time {
        // "going down".
        let number = fighter.parent_number;
        if fighter.in_space && number & 3 == 0 {
            state.gravity = 0.0;
        } else if fighter.in_space && number & 2 == 0 {
            state.gravity = -500.0;
            state.velocity[2] = 80.0;
        } else {
            state.gravity = 500.0;
            state.velocity[2] = -80.0;
        }
    } else if is_suspended(vehicle, state, fighter) {
        state.gravity = 0.0;
    } else if (state.speed == 0.0 || state.speed < speed_idle) && vehicle.ucmd.up_move <= 0 {
        if !fighter.in_space || over_valid_landing_surface(vehicle) {
            state.gravity = ((speed_idle - state.speed) / 4.0) as i32 as f32;
        }
    } else {
        state.gravity = 0.0;
    }
}
