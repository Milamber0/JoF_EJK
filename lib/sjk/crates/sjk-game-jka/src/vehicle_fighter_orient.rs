//! How a fighter turns and how it is animated (`codemp/game/FighterNPC.c`): its
//! orientation (`ProcessOrientCommands`: hyperspace, the drop, the death spiral and torn
//! wings (`FighterDamageRoutine`), the turn an impact left it (`m_vFullAngleVelocity`),
//! landing, yaw and pitch toward the pilot's view (`FighterYawAdjust`,
//! `FighterPitchAdjust`), the roll a turn and a strafe bank it into, damaged wings and
//! nose (`FighterWingMalfunctionCheck`, `FighterNoseMalfunctionCheck`), and an empty
//! fighter's dive) and its wings and gears (`AnimateVehicle`, the game's).
//!
//! `VEH_CONTROL_SCHEME_4` is not defined in the reference build; its branches are not here.

use crate::player_angle_math::{angle_mod, angle_subtract, normalized_angle};
use crate::pmove::MovementState;
use crate::pmove_anim::{AnimationLengths, SETANIM_BOTH};
use crate::vehicle::{Vehicle, flags};
use crate::vehicle_fighter::{
    FighterSteering, HYPERSPACE_TIME, MAX_STRAFE_TIME, MIN_LANDING_SLOPE, MIN_LANDING_SPEED,
    is_landed, is_landing, is_suspended, over_valid_landing_surface, predicted_angular_decrement,
};
use crate::vehicle_move::Steering;

/// `PITCH`, `YAW`, `ROLL`.
const PITCH: usize = 0;
const YAW: usize = 1;
const ROLL: usize = 2;
/// `SHIPSURF_DAMAGE_*` bits of `brokenLimbs`.
const DAMAGE_FRONT_LIGHT: u8 = 1 << 0;
const DAMAGE_RIGHT_LIGHT: u8 = 1 << 2;
const DAMAGE_LEFT_LIGHT: u8 = 1 << 3;
const DAMAGE_FRONT_HEAVY: u8 = 1 << 4;
const DAMAGE_RIGHT_HEAVY: u8 = 1 << 6;
const DAMAGE_LEFT_HEAVY: u8 = 1 << 7;
/// `SHIPSURF_BROKEN_C` .. `F`: the wings.
const BROKEN_C: i32 = 1 << 2;
const BROKEN_D: i32 = 1 << 3;
const BROKEN_E: i32 = 1 << 4;
const BROKEN_F: i32 = 1 << 5;
const ALL_WINGS: i32 = BROKEN_C | BROKEN_D | BROKEN_E | BROKEN_F;
/// `BOTH_GEARS_OPEN`, `BOTH_GEARS_CLOSE`, `BOTH_WINGS_OPEN`, `BOTH_WINGS_CLOSE`.
const BOTH_GEARS_OPEN: u16 = 1_093;
const BOTH_GEARS_CLOSE: u16 = 1_094;
const BOTH_WINGS_OPEN: u16 = 1_095;
const BOTH_WINGS_CLOSE: u16 = 1_096;

/// What a fighter's orientation asks of the game (`#ifdef _GAME` in `FighterDamageRoutine`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OrientCues {
    /// "if you land at all when pieces of your ship are missing, then die": `G_Damage(parent,
    /// killer, killer, vec3_origin, origin, 99999, DAMAGE_NO_ARMOR, MOD_SUICIDE)`.
    pub landed_broken: bool,
}

/// `ProcessOrientCommands` (`FighterNPC.c:1276-1680`).
pub fn orient_commands(
    vehicle: &mut Vehicle,
    state: &mut MovementState,
    steering: &Steering,
) -> OrientCues {
    let fighter = &steering.fighter;
    let time = steering.time;
    let rider = [steering.rider_pitch, steering.rider_yaw, fighter.rider_roll];
    let mut cues = OrientCues::default();
    if vehicle.hyperspace_time != 0 && time - vehicle.hyperspace_time < HYPERSPACE_TIME {
        // "Going to Hyperspace".
        vehicle.orientation = rider;
        state.view_angles = rider;
        return cues;
    }
    if vehicle.drop_time >= time {
        // "you can only YAW during this".
        vehicle.orientation[YAW] = rider[YAW];
        state.view_angles[YAW] = rider[YAW];
        return cues;
    }
    let modifier = vehicle.time_modifier;
    let removed = vehicle.removed_surfaces;
    let electrified = steering.electrify_time >= time;
    if fighter.dead
        || electrified
        || (vehicle.info.surf_destruction != 0 && removed != 0 && removed & ALL_WINGS == ALL_WINGS)
    {
        cues.landed_broken |= damage_routine(vehicle, state, fighter);
        vehicle.orientation[ROLL] = normalized_angle(vehicle.orientation[ROLL]);
        return cues;
    }
    if !fighter.unrestrained {
        vehicle.orientation[ROLL] =
            predicted_angular_decrement(0.95, modifier * 2.0, vehicle.orientation[ROLL]);
    }
    let landing_or_landed = is_landing(vehicle, state, fighter) || is_landed(vehicle, state);
    if !landing_or_landed {
        wing_malfunction(vehicle, state);
        // The turn an impact left it, worked off.
        for axis in 0..3 {
            let left = vehicle.full_angle_velocity[axis];
            if left == 0.0 {
                continue;
            }
            let step = (left * 0.1) * modifier;
            if step > 1.0 || step < -1.0 {
                vehicle.orientation[axis] = normalized_angle(vehicle.orientation[axis] + step);
                // "don't pitch downward into ground even more".
                if axis == PITCH
                    && vehicle.orientation[axis] > 90.0
                    && vehicle.orientation[axis] - step < 90.0
                {
                    vehicle.orientation[axis] = 90.0;
                    vehicle.full_angle_velocity[axis] = -vehicle.full_angle_velocity[axis];
                }
                vehicle.full_angle_velocity[axis] -= step;
            } else {
                vehicle.full_angle_velocity[axis] = 0.0;
            }
        }
    } else {
        vehicle.full_angle_velocity = [0.0; 3];
    }
    let mut roll = vehicle.orientation[ROLL];
    let number = fighter.parent_number;
    let no_yaw_control = (removed != 0 || electrified) && (number % 4 == 0 || number % 5 == 0);
    if landing_or_landed && removed == 0 && !electrified {
        if state.speed > 0.0 {
            vehicle.orientation[PITCH] = if vehicle.land_trace.fraction < 0.3 {
                0.0
            } else {
                predicted_angular_decrement(0.83, modifier * 10.0, vehicle.orientation[PITCH])
            };
        }
        if vehicle.land_trace.fraction > 0.1 || vehicle.land_trace.normal[2] < MIN_LANDING_SLOPE {
            yaw_adjust(vehicle, state, rider[YAW]);
        }
    } else if no_yaw_control {
        // Spiralling out of control: no yaw.
    } else if vehicle.pilot.is_some() && fighter.pilot_player && state.speed > 0.0 {
        if fighter.unrestrained {
            vehicle.orientation = rider;
            state.view_angles = rider;
            roll = vehicle.orientation[ROLL];
            nose_malfunction(vehicle, state);
        } else {
            yaw_adjust(vehicle, state, rider[YAW]);
            if !over_valid_landing_surface(vehicle) || state.speed > MIN_LANDING_SPEED {
                pitch_adjust(vehicle, state, rider[PITCH]);
                nose_malfunction(vehicle, state);
                // The roll a turn banks it into, eased.
                let yaw_delta =
                    angle_subtract(vehicle.orientation[YAW], vehicle.prev_orientation[YAW])
                        .clamp(-8.0, 8.0);
                roll -= yaw_delta;
                roll = predicted_angular_decrement(0.93, modifier * 2.0, roll);
                let limit = vehicle.info.roll_limit;
                if limit != -1.0 {
                    if roll > limit {
                        roll = limit;
                    } else if roll < -limit {
                        roll = -limit;
                    }
                }
            }
        }
    }
    if landing_or_landed
        && !fighter.dead
        && !electrified
        && (vehicle.info.surf_destruction == 0 || removed == 0)
    {
        // "even out your pitch".
        let pitch = vehicle.orientation[PITCH];
        vehicle.orientation[PITCH] = predicted_angular_decrement(
            if pitch > 0.0 { 0.2 } else { 0.75 },
            modifier * 10.0,
            pitch,
        );
    }
    if fighter.server
        && !vehicle.inhabited()
        && vehicle.land_trace.fraction >= 0.1
        && !fighter.in_space
        && !is_suspended(vehicle, state, fighter)
    {
        // "If no one is in this vehicle and it's up in the sky, pitch it forward as it comes
        // tumbling down."
        vehicle.ucmd.up_move = 0;
        vehicle.orientation[PITCH] += modifier;
        if !fighter.unrestrained && vehicle.orientation[PITCH] > 60.0 {
            vehicle.orientation[PITCH] = 60.0;
        }
    }
    if state.hacking_time == 0 {
        vehicle.orientation[ROLL] = roll;
        // "continually adjust the yaw based on the roll".
        if vehicle.orientation[ROLL] != 0.0 && !no_yaw_control && !fighter.unrestrained {
            vehicle.orientation[YAW] -= (vehicle.orientation[ROLL] * 0.05) * modifier;
        }
    } else {
        // The strafe's roll.
        let limit = vehicle.info.roll_limit;
        let strafe_roll = (state.hacking_time as f32 / MAX_STRAFE_TIME) * limit;
        let difference = angle_subtract(strafe_roll, vehicle.orientation[ROLL]);
        vehicle.orientation[ROLL] += (difference * 0.1) * modifier;
        if !fighter.unrestrained && limit != -1.0 && removed == 0 && !electrified {
            if vehicle.orientation[ROLL] > limit {
                vehicle.orientation[ROLL] = limit;
            } else if vehicle.orientation[ROLL] < -limit {
                vehicle.orientation[ROLL] = -limit;
            }
        }
    }
    if vehicle.info.surf_destruction != 0 {
        cues.landed_broken |= damage_routine(vehicle, state, fighter);
    }
    vehicle.orientation[ROLL] = normalized_angle(vehicle.orientation[ROLL]);
    cues
}

/// `FighterYawAdjust` (`FighterNPC.c:1196-1219`): the yaw toward the rider's, as far as
/// the speed allows.
fn yaw_adjust(vehicle: &mut Vehicle, state: &MovementState, rider_yaw: f32) {
    let difference = angle_subtract(vehicle.orientation[YAW], rider_yaw);
    if let Some(turn) = turn(vehicle, state, difference) {
        vehicle.orientation[YAW] = normalized_angle(vehicle.orientation[YAW] - turn);
    }
}

/// `FighterPitchAdjust` (`FighterNPC.c:1222-1246`): the pitch toward the rider's.
fn pitch_adjust(vehicle: &mut Vehicle, state: &MovementState, rider_pitch: f32) {
    let difference = angle_subtract(vehicle.orientation[PITCH], rider_pitch);
    if let Some(turn) = turn(vehicle, state, difference) {
        vehicle.orientation[PITCH] = angle_mod(vehicle.orientation[PITCH] - turn);
    }
}

/// The turn both adjustments make: `difference` scaled by the speed's share of the top
/// speed, at most `turningSpeed * 0.8`, times a fifth of the frame; none while stopped.
fn turn(vehicle: &Vehicle, state: &MovementState, difference: f32) -> Option<f32> {
    if state.speed == 0.0 {
        return None;
    }
    let speed = state.speed.abs();
    let most = vehicle.info.turning_speed * 0.8;
    let mut difference = difference * (speed / vehicle.info.speed_max);
    if difference > most {
        difference = most;
    } else if difference < -most {
        difference = -most;
    }
    Some(difference * (vehicle.time_modifier * 0.2))
}

/// `BG_VehicleTurnRateForSpeed` (`bg_pmove.c:649-679`): the mouse's pitch and yaw rates
/// (1 where the definition has none), scaled by the speed where the turn depends on it.
fn turn_rate_for_speed(vehicle: &Vehicle, speed: f32) -> (f32, f32) {
    let info = &vehicle.info;
    let mut fraction = 1.0;
    if info.speed_dependant_turning
        && (vehicle.land_trace.fraction >= 1.0 || vehicle.land_trace.normal[2] < MIN_LANDING_SLOPE)
    {
        fraction = (speed / (info.speed_max * 0.75)).clamp(0.25, 1.0);
    }
    let pitch = if info.mouse_pitch != 0.0 {
        info.mouse_pitch * fraction
    } else {
        1.0
    };
    let yaw = if info.mouse_yaw != 0.0 {
        info.mouse_yaw * fraction
    } else {
        1.0
    };
    (pitch, yaw)
}

/// The sine of the vehicle command's time in seconds, as a double (`sin(serverTime*0.001)`).
fn wobble(vehicle: &Vehicle) -> f64 {
    (f64::from(vehicle.ucmd.server_time) * 0.001).sin()
}

/// `FighterWingMalfunctionCheck` (`FighterNPC.c:783-808`): a damaged wing rolls it.
fn wing_malfunction(vehicle: &mut Vehicle, state: &MovementState) {
    let (_, yaw_rate) = turn_rate_for_speed(vehicle, state.speed);
    let swing = (wobble(vehicle) + 1.0) * f64::from(vehicle.time_modifier) * f64::from(yaw_rate);
    let limbs = state.broken_limbs;
    let roll = &mut vehicle.orientation[ROLL];
    if limbs & DAMAGE_RIGHT_HEAVY != 0 {
        *roll = (f64::from(*roll) + swing * 50.0) as f32;
    } else if limbs & DAMAGE_RIGHT_LIGHT != 0 {
        *roll = (f64::from(*roll) + swing * 12.5) as f32;
    }
    if limbs & DAMAGE_LEFT_HEAVY != 0 {
        *roll = (f64::from(*roll) - swing * 50.0) as f32;
    } else if limbs & DAMAGE_LEFT_LIGHT != 0 {
        *roll = (f64::from(*roll) - swing * 12.5) as f32;
    }
}

/// `FighterNoseMalfunctionCheck` (`FighterNPC.c:810-826`): a damaged nose pitches it up
/// and down.
fn nose_malfunction(vehicle: &mut Vehicle, state: &MovementState) {
    let (pitch_rate, _) = turn_rate_for_speed(vehicle, state.speed);
    let swing = wobble(vehicle) * f64::from(vehicle.time_modifier) * f64::from(pitch_rate);
    let limbs = state.broken_limbs;
    let pitch = &mut vehicle.orientation[PITCH];
    if limbs & DAMAGE_FRONT_HEAVY != 0 {
        *pitch = (f64::from(*pitch) + swing * 50.0) as f32;
    } else if limbs & DAMAGE_FRONT_LIGHT != 0 {
        *pitch = (f64::from(*pitch) + swing * 20.0) as f32;
    }
}

/// `FighterDamageRoutine` (`FighterNPC.c:828-990`): a dead fighter's spiral, a torn one's
/// dive and roll. Whether it touched ground with pieces missing (the game kills it).
fn damage_routine(vehicle: &mut Vehicle, state: &MovementState, fighter: &FighterSteering) -> bool {
    let modifier = vehicle.time_modifier;
    let number = fighter.parent_number;
    let removed = vehicle.removed_surfaces;
    let orientation = &mut vehicle.orientation;
    if removed == 0 {
        if fighter.dead {
            // The death spiral.
            vehicle.ucmd.up_move = 0;
            if number % 3 == 0 {
                orientation[PITCH] += modifier;
                if !fighter.unrestrained && orientation[PITCH] > 60.0 {
                    orientation[PITCH] = 60.0;
                }
            } else if number % 2 == 0 {
                orientation[PITCH] -= modifier;
                // The reference's comparison, as it is.
                if !fighter.unrestrained && orientation[PITCH] > -60.0 {
                    orientation[PITCH] = -60.0;
                }
            }
            if number % 2 != 0 {
                orientation[YAW] += modifier;
                orientation[ROLL] += modifier * 4.0;
            } else {
                orientation[YAW] -= modifier;
                orientation[ROLL] -= modifier * 4.0;
            }
        }
        return false;
    }
    vehicle.ucmd.up_move = 0;
    if vehicle.land_trace.fraction >= 0.1 && !is_suspended(vehicle, state, fighter) {
        let orientation = &mut vehicle.orientation;
        if number % 2 == 0 {
            orientation[PITCH] += modifier;
            if !fighter.unrestrained && orientation[PITCH] > 60.0 {
                orientation[PITCH] = 60.0;
            }
        } else if number % 3 == 0 {
            orientation[PITCH] -= modifier;
            if !fighter.unrestrained && orientation[PITCH] > -60.0 {
                orientation[PITCH] = -60.0;
            }
        }
    }
    let landed_broken = fighter.server && vehicle.land_trace.fraction < 1.0;
    let (left, right) = (
        removed & (BROKEN_C | BROKEN_D),
        removed & (BROKEN_E | BROKEN_F),
    );
    let spins = number % 4 == 0 || number % 5 == 0;
    let roll = &mut vehicle.orientation[ROLL];
    if left != 0 && right != 0 {
        let mut factor: f32 = 2.0;
        if removed & ALL_WINGS == ALL_WINGS {
            factor *= 2.0;
        }
        if spins {
            factor *= 4.0;
        }
        *roll += modifier * factor;
    } else if left != 0 {
        let mut factor: f32 = 2.0;
        if left == BROKEN_C | BROKEN_D {
            factor *= 2.0;
        }
        if spins {
            factor *= 4.0;
        }
        *roll += factor * modifier;
    } else if right != 0 {
        let mut factor: f32 = 2.0;
        if right == BROKEN_E | BROKEN_F {
            factor *= 2.0;
        }
        if spins {
            factor *= 4.0;
        }
        *roll -= factor * modifier;
    }
    landed_broken
}

/// `AnimateVehicle` (`FighterNPC.c:1686-1763`), the game's: the wings shut going into
/// hyperspace; up in the air, the wings open and the gears up; near the ground, the gears
/// down to land (the landing sound where the definition has one) or up and the wings shut
/// to take off. Whether the landing sound plays.
pub fn animate(
    vehicle: &mut Vehicle,
    state: &mut MovementState,
    fighter: &FighterSteering,
    level_time: i32,
    lengths: &dyn AnimationLengths,
) -> bool {
    let mut animation = None;
    let mut land_sound = false;
    if vehicle.hyperspace_time != 0 && level_time - vehicle.hyperspace_time < HYPERSPACE_TIME {
        if vehicle.flags & flags::WINGS_OPEN != 0 {
            vehicle.flags &= !flags::WINGS_OPEN;
            animation = Some(BOTH_WINGS_CLOSE);
        }
    } else {
        let landing = is_landing(vehicle, state, fighter);
        let landed = is_landed(vehicle, state);
        if !landing && !landed {
            if vehicle.flags & flags::WINGS_OPEN == 0 {
                vehicle.flags |= flags::WINGS_OPEN;
                vehicle.flags &= !flags::GEARS_OPEN;
                animation = Some(BOTH_WINGS_OPEN);
            }
        } else if (vehicle.ucmd.forward_move < 0 || vehicle.ucmd.up_move < 0 || landed)
            && vehicle.land_trace.fraction <= 0.4
            && vehicle.land_trace.normal[2] >= MIN_LANDING_SLOPE
        {
            if vehicle.flags & flags::GEARS_OPEN == 0 {
                land_sound = vehicle.info.sound_land != 0;
                vehicle.flags |= flags::GEARS_OPEN;
                animation = Some(BOTH_GEARS_OPEN);
            }
        } else if vehicle.flags & flags::GEARS_OPEN != 0 {
            vehicle.flags &= !flags::GEARS_OPEN;
            animation = Some(BOTH_GEARS_CLOSE);
        } else if vehicle.flags & flags::WINGS_OPEN != 0 {
            vehicle.flags &= !flags::WINGS_OPEN;
            animation = Some(BOTH_WINGS_CLOSE);
        }
    }
    if let Some(animation) = animation {
        crate::pmove_anim::set_animation(state, SETANIM_BOTH, animation, 0, lengths);
    }
    land_sound
}
