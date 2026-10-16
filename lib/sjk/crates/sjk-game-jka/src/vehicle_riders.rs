//! What a speeder or an animal does to its rider inside its own move
//! (`codemp/game/g_vehicles.c`, `SpeederNPC.c`, `AnimalNPC.c`, `bg_pmove.c`,
//! `bg_vehicleLoad.c`): the pilot's view held to the vehicle's pitch limit
//! (`PM_VehicleViewAngles`), the rider getting off by the jump or roll keys or at the end of
//! its dismount (`UpdateRider`), the riders' animations (`AnimateRiders`) and the pilot kept
//! on the vehicle's driver tag (`AttachRiders`).
//!
//! Also the one thing the rider's own move does to the vehicle: facing the hyperspace point
//! (`PM_VehFaceHyperspacePoint`), which a vehicle's `hyperSpaceTime` of zero asks of every
//! rider in a level's first four seconds.

use crate::pmove::MovementState;
use crate::pmove_anim::{
    SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_HOLDLESS, SETANIM_FLAG_OVERRIDE,
    SETANIM_FLAG_RESTART,
};
use crate::vehicle::{Vehicle, eject, flags};
use crate::vehicle_board::{EjectTrace, Parent};
use crate::vehicle_fields::kind;
use crate::vehicle_rider::{FL_VEH_BOARDING, Rider};
use sjk_protocol::UserCommand;

/// `BUTTON_ATTACK`, `BUTTON_USE`, `BUTTON_WALKING`.
const BUTTON_ATTACK: u16 = 1;
const BUTTON_USE: u16 = 32;
const BUTTON_WALKING: u16 = 16;
/// `EV_JUMP`, `EV_ROLL`.
const EV_JUMP: u32 = 16;
const EV_ROLL: u32 = 17;
/// `JUMP_VELOCITY`.
const JUMP_VELOCITY: f32 = 225.0;
/// `WP_NONE`, `WP_MELEE`, `WP_SABER`, `WP_BLASTER`.
const WP_NONE: u8 = 0;
const WP_MELEE: u8 = 2;
const WP_SABER: u8 = 3;
const WP_BLASTER: u8 = 4;
/// `HYPERSPACE_TIME`, `HYPERSPACE_TELEPORT_FRAC`, `EF2_HYPERSPACE`.
pub const HYPERSPACE_TIME: i32 = 4_000;
const HYPERSPACE_TELEPORT_FRAC: f32 = 0.75;
pub const EF2_HYPERSPACE: u32 = 1 << 5;
/// `PITCH`, `YAW`.
const PITCH: usize = 0;
const YAW: usize = 1;

/// The animations the riders' functions play (`anims.h`).
mod anim {
    pub const BOTH_VS_MOUNT_L: u16 = 1_015;
    pub const BOTH_VS_DISMOUNT_L: u16 = 1_016;
    pub const BOTH_VS_MOUNT_R: u16 = 1_017;
    pub const BOTH_VS_DISMOUNT_R: u16 = 1_018;
    pub const BOTH_VS_MOUNTJUMP_L: u16 = 1_019;
    pub const BOTH_VS_MOUNTTHROW_L: u16 = 1_021;
    pub const BOTH_VS_MOUNTTHROW_R: u16 = 1_022;
    pub const BOTH_VT_WALK_FWD: u16 = 1_062;
    pub const BOTH_VT_RUN_FWD: u16 = 1_066;
    pub const BOTH_VT_TURBO: u16 = 1_078;
    pub const BOTH_VT_IDLE_SL: u16 = 1_079;
    pub const BOTH_VT_IDLE_SR: u16 = 1_080;
    pub const BOTH_VT_IDLE1: u16 = 1_082;
    pub const BOTH_VT_IDLE_G: u16 = 1_084;
    pub const BOTH_VT_ATL_S: u16 = 1_086;
    pub const BOTH_VT_ATR_S: u16 = 1_087;
    pub const BOTH_VT_ATR_TO_L_S: u16 = 1_088;
    pub const BOTH_VT_ATL_TO_R_S: u16 = 1_089;
    pub const BOTH_VT_ATR_G: u16 = 1_090;
    pub const BOTH_VT_ATL_G: u16 = 1_091;
    pub const BOTH_VT_ATF_G: u16 = 1_092;
    pub const BOTH_JUMP1: u16 = 1_138;
    pub const BOTH_ROLL_F: u16 = 1_167;
    pub const BOTH_ROLL_B: u16 = 1_168;
    pub const BOTH_ROLL_L: u16 = 1_169;
    pub const BOTH_ROLL_R: u16 = 1_170;
}

/// `VEH_MOUNT_THROW_LEFT`, `VEH_MOUNT_THROW_RIGHT`.
const VEH_MOUNT_THROW_LEFT: i32 = -5;
const VEH_MOUNT_THROW_RIGHT: i32 = -6;

/// `Vehicle_SetAnim` on a rider: its animation, and its entity's legs at once
/// (`ent->s.legsAnim = ent->client->ps.legsAnim`).
fn vehicle_set_animation(rider: &mut Rider<'_>, animation: u16, flags: u8) {
    rider.set_animation(SETANIM_BOTH, animation, flags);
    if let Some([_, torso]) = rider.movement.entity_animations {
        rider.movement.entity_animations = Some([rider.movement.legs_anim, torso]);
    }
    rider
        .entity
        .set_raw_field(16, u32::from(rider.movement.legs_anim));
}

/// `PM_VehicleViewAngles` (`bg_pmove.c:9640-9709`) for the pilot: its pitch held within the
/// vehicle's `lookPitch`, its view set against the vehicle's command.
pub fn vehicle_view_angles(vehicle: &Vehicle, rider: &mut Rider<'_>, fighter_alt_control: bool) {
    if vehicle.pilot != Some(rider.number) {
        // Only ever called for the pilot (`self = pm_entVeh`): the passengers' turret
        // clamp in its other branch is never reached.
        return;
    }
    if fighter_alt_control
        && rider.number < 32
        && rider.vehicle() != 0
        && vehicle.kind() == crate::vehicle_fields::kind::FIGHTER
    {
        // `BG_UnrestrainedPitchRoll`: free roll and pitch, no clamp.
        return;
    }
    let limit = vehicle.info.look_pitch;
    let (low, high) = (-limit, limit);
    let mut view = rider.movement.view_angles;
    // Pitch: clamped where the limit allows anything; yaw: "no allowance", left; roll: no
    // clamp.
    if low != 0.0 || high != 0.0 {
        if view[PITCH] > high {
            view[PITCH] = high;
        } else if view[PITCH] < low {
            view[PITCH] = low;
        }
    }
    let command = vehicle.ucmd;
    rider.movement.view_angles = view;
    rider.set_pm_view_angle(view, &command);
}

/// `UpdateRider` (`g_vehicles.c:1635-1812`) for a rider that is no fighter's or walker's:
/// the rocket lock shown to it, and the ways off — the use key on an animal, the end of a
/// dismount, a jump, a roll. Returns whether it is still aboard.
pub fn update_rider(
    parent: &mut Parent<'_>,
    rider: &mut Rider<'_>,
    command: &UserCommand,
    level_time: i32,
    trace: &mut EjectTrace<'_>,
) -> bool {
    if parent.vehicle.boarding != 0 && parent.vehicle.die_time == 0 {
        return true;
    }
    rider.movement.rocket_lock_index = parent.state.rocket_lock_index;
    rider.movement.rocket_lock_time = parent.state.rocket_lock_time;
    rider.movement.rocket_target_time = parent.state.rocket_target_time;
    let vehicle_kind = parent.vehicle.kind();
    if command.buttons & BUTTON_USE != 0 && vehicle_kind != kind::SPEEDER {
        if vehicle_kind == kind::WALKER {
            parent.vehicle.eject_dir = eject::REAR;
            if crate::vehicle_board::eject(parent, rider, false, level_time, trace) {
                return false;
            }
        } else if parent.vehicle.flags & flags::FLYING == 0 {
            if parent.state.speed <= 600.0 && command.right_move != 0 {
                if crate::vehicle_board::eject(parent, rider, false, level_time, trace) {
                    let (animation, direction) = if command.right_move > 0 {
                        (anim::BOTH_ROLL_R, eject::RIGHT)
                    } else {
                        (anim::BOTH_ROLL_L, eject::LEFT)
                    };
                    parent.vehicle.eject_dir = direction;
                    rider.movement.velocity = parent.state.velocity.map(|value| value * 0.25);
                    vehicle_set_animation(
                        rider,
                        animation,
                        SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD | SETANIM_FLAG_HOLDLESS,
                    );
                    rider.movement.weapon_time = rider.movement.torso_timer - 200;
                    rider.add_event(EV_ROLL, 0);
                    return false;
                }
            } else {
                let (animation, direction) = if command.right_move > 0 {
                    (anim::BOTH_VS_DISMOUNT_R, eject::RIGHT)
                } else {
                    (anim::BOTH_VS_DISMOUNT_L, eject::LEFT)
                };
                parent.vehicle.eject_dir = direction;
                if parent.vehicle.boarding <= 1 {
                    // "I know I shouldn't reuse pVeh->m_iBoarding so many times".
                    let length = rider.animation_length(animation);
                    parent.vehicle.boarding = level_time + length;
                    rider.body.flags |= FL_VEH_BOARDING;
                    rider.movement.weapon_time = length;
                }
                rider.movement.velocity = parent.state.velocity.map(|value| value * 0.25);
                vehicle_set_animation(rider, animation, SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD);
            }
        } else {
            parent.vehicle.eject_dir = eject::LEFT;
            if crate::vehicle_board::eject(parent, rider, false, level_time, trace) {
                return false;
            }
        }
    }
    // A dismount's animation over: off now.
    if parent.vehicle.boarding < level_time && rider.body.flags & FL_VEH_BOARDING != 0 {
        rider.body.flags &= !FL_VEH_BOARDING;
        if crate::vehicle_board::eject(parent, rider, false, level_time, trace) {
            return false;
        }
    }
    if vehicle_kind == kind::FIGHTER || vehicle_kind == kind::WALKER {
        return true;
    }
    if command.up_move > 0 && crate::vehicle_board::eject(parent, rider, false, level_time, trace) {
        // "Allow them to force jump off."
        rider.movement.velocity = parent.state.velocity.map(|value| value * 0.5);
        rider.movement.velocity[2] += JUMP_VELOCITY;
        rider.movement.force_jump_start_height = rider.movement.origin[2];
        // No ICARUS task waits on a player's voice.
        rider.add_event(EV_JUMP, 0);
        vehicle_set_animation(
            rider,
            anim::BOTH_JUMP1,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        );
        return false;
    }
    if command.up_move < 0 {
        let (animation, direction) = if command.right_move > 0 {
            (anim::BOTH_ROLL_R, eject::RIGHT)
        } else if command.right_move < 0 {
            (anim::BOTH_ROLL_L, eject::LEFT)
        } else if command.forward_move < 0 {
            (anim::BOTH_ROLL_B, eject::REAR)
        } else if command.forward_move > 0 {
            (anim::BOTH_ROLL_F, eject::FRONT)
        } else {
            (anim::BOTH_ROLL_B, eject::REAR)
        };
        parent.vehicle.eject_dir = direction;
        if crate::vehicle_board::eject(parent, rider, false, level_time, trace) {
            if parent.vehicle.flags & flags::FLYING == 0 {
                rider.movement.velocity = parent.state.velocity.map(|value| value * 0.25);
                vehicle_set_animation(
                    rider,
                    animation,
                    SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD | SETANIM_FLAG_HOLDLESS,
                );
                rider.movement.weapon_time = rider.movement.torso_timer - 200;
                rider.add_event(EV_ROLL, 0);
            }
            return false;
        }
    }
    true
}

/// `AnimateRiders` for a speeder (`SpeederNPC.c:351-616`): only the mount's animation — the
/// rest of it is switched off in the reference (`if (1) return;`).
pub fn animate_speeder_riders(vehicle: &mut Vehicle, rider: &mut Rider<'_>, level_time: i32) {
    if vehicle.boarding >= 0 {
        return;
    }
    let animation = match vehicle.boarding {
        -1 => anim::BOTH_VS_MOUNT_L,
        -2 => anim::BOTH_VS_MOUNT_R,
        -3 => anim::BOTH_VS_MOUNTJUMP_L,
        VEH_MOUNT_THROW_LEFT => anim::BOTH_VS_MOUNTTHROW_R,
        VEH_MOUNT_THROW_RIGHT => anim::BOTH_VS_MOUNTTHROW_L,
        // The reference's default, `BOTH_VS_IDLE`.
        _ => 1_036,
    };
    // "the delay is actually 40% (0.4f) of the animation time".
    let length = (rider.animation_length(animation) as f32 * 0.4) as i32;
    vehicle.boarding = level_time + length;
    rider.set_animation(
        SETANIM_BOTH,
        animation,
        SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
    );
}

/// The weapon pose an animal's rider holds (`EWeaponPose`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pose {
    None,
    Blaster,
    SaberLeft,
    SaberRight,
}

/// `AnimateRiders` for an animal (`AnimalNPC.c:485-650`): the rider rides as the animal
/// goes — walking, running, its turbo, its weapon's idle and attacks.
pub fn animate_animal_riders(
    vehicle: &mut Vehicle,
    parent_speed: f32,
    rider: &mut Rider<'_>,
    level_time: i32,
) {
    if vehicle.boarding != 0 {
        return;
    }
    let fraction = parent_speed / vehicle.info.speed_max;
    let command = vehicle.ucmd;
    let weapon = rider.movement.weapon;
    let has_weapon = weapon != WP_NONE && weapon != WP_MELEE;
    let attacking = has_weapon && command.buttons & BUTTON_ATTACK != 0;
    let mut right = command.right_move > 0;
    let mut left = command.right_move < 0;
    let turbo = fraction > 0.0 && level_time < vehicle.turbo_time;
    let walking = fraction > 0.0 && (command.buttons & BUTTON_WALKING != 0 || fraction <= 0.275);
    let running = fraction > 0.275;
    vehicle.flags &= !flags::CRASHING;
    if rider.movement.weapon_time > 0 {
        return;
    }
    let pose = if weapon == WP_BLASTER {
        Pose::Blaster
    } else if weapon == WP_SABER {
        let torso = rider.movement.torso_anim;
        if vehicle.flags & flags::SABER_IN_LEFT_HAND != 0 && torso == anim::BOTH_VT_ATL_TO_R_S {
            vehicle.flags &= !flags::SABER_IN_LEFT_HAND;
        }
        if vehicle.flags & flags::SABER_IN_LEFT_HAND == 0 && torso == anim::BOTH_VT_ATR_TO_L_S {
            vehicle.flags |= flags::SABER_IN_LEFT_HAND;
        }
        if vehicle.flags & flags::SABER_IN_LEFT_HAND != 0 {
            Pose::SaberLeft
        } else {
            Pose::SaberRight
        }
    } else {
        Pose::None
    };
    let (animation, flags) = if attacking && pose != Pose::None {
        if turbo {
            right = true;
            left = false;
        }
        if !left && !right && weapon == WP_SABER {
            left = pose == Pose::SaberLeft;
            right = !left;
        }
        let animation = match (left, right, pose) {
            (true, _, Pose::Blaster) => anim::BOTH_VT_ATL_G,
            (true, _, Pose::SaberLeft) => anim::BOTH_VT_ATL_S,
            (true, _, Pose::SaberRight) => anim::BOTH_VT_ATR_TO_L_S,
            (false, true, Pose::Blaster) => anim::BOTH_VT_ATR_G,
            (false, true, Pose::SaberLeft) => anim::BOTH_VT_ATL_TO_R_S,
            (false, true, Pose::SaberRight) => anim::BOTH_VT_ATR_S,
            // "Attack Ahead": only a blaster has one.
            _ => anim::BOTH_VT_ATF_G,
        };
        (
            animation,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD | SETANIM_FLAG_RESTART,
        )
    } else if turbo {
        (anim::BOTH_VT_TURBO, SETANIM_FLAG_OVERRIDE)
    } else {
        let animation = match pose {
            Pose::None if walking => anim::BOTH_VT_WALK_FWD,
            Pose::None if running => anim::BOTH_VT_RUN_FWD,
            Pose::None => anim::BOTH_VT_IDLE1,
            Pose::Blaster => anim::BOTH_VT_IDLE_G,
            Pose::SaberLeft => anim::BOTH_VT_IDLE_SL,
            Pose::SaberRight => anim::BOTH_VT_IDLE_SR,
        };
        (animation, SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLDLESS)
    };
    vehicle_set_animation(rider, animation, flags);
}

/// `Vehicle_SetAnim` on the pilot as an animal's mount begins (`AnimalNPC.c:431-434`).
pub fn animal_mount(rider: &mut Rider<'_>, animation: u16) {
    vehicle_set_animation(rider, animation, SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD);
}

/// `AttachRiders` (`g_vehicles.c:1818-1914`, `AttachRidersGeneric`): the pilot on the
/// driver tag — `offset` from the vehicle's origin along its yaw (forward, left, up), the
/// tag's place in the vehicle's model; none where the host has no model for it, as the
/// fake engine's Ghoul2 of the oracle has none.
pub fn attach_riders(
    vehicle: &Vehicle,
    origin: [f32; 3],
    yaw: f32,
    offset: [f32; 3],
    rider: &mut Rider<'_>,
) {
    if vehicle.pilot != Some(rider.number) {
        return;
    }
    rider.movement.origin = driver_origin(origin, yaw, offset);
}

/// Where the driver tag of a vehicle at `origin` facing `yaw` is: `offset` along its yaw
/// (forward, left, up), the vehicle's origin itself for no offset.
pub fn driver_origin(origin: [f32; 3], yaw: f32, offset: [f32; 3]) -> [f32; 3] {
    if offset == [0.0; 3] {
        return origin;
    }
    let axis = crate::pmove::flight::angles_to_axis([0.0, yaw, 0.0]);
    std::array::from_fn(|index| {
        origin[index]
            + axis[0][index] * offset[0]
            + axis[1][index] * offset[1]
            + axis[2][index] * offset[2]
    })
}

/// `PM_VehFaceHyperspacePoint` (`bg_pmove.c:9911-9982`), from the rider's move: the rider
/// and its vehicle's command pushed up and still, the view turned toward the hyperspace
/// angles, the vehicle's `hyperSpaceTime` kept current until it faces them. `seconds` and
/// `millis` are the rider's slice's.
pub fn face_hyperspace_point(
    state: &mut MovementState,
    command: &mut UserCommand,
    riding: &mut crate::pmove::riding::Riding,
    seconds: f32,
    millis: i32,
) {
    let fraction = (command.server_time - riding.hyperspace_time) as f32 / HYPERSPACE_TIME as f32;
    command.up_move = 127;
    command.forward_move = 0;
    command.right_move = 0;
    riding.command_moves = Some((0, 0, 127));
    let turn = 90.0 * seconds;
    let mut matched = 0;
    for axis in 0..3 {
        let target = riding.hyperspace_angles[axis];
        let delta = crate::player_angle_math::angle_subtract(target, riding.orientation[axis]);
        if delta.abs() < turn {
            state.view_angles[axis] = target;
            matched += 1;
        } else {
            let delta = crate::player_angle_math::angle_subtract(target, state.view_angles[axis]);
            let view = state.view_angles[axis];
            state.view_angles[axis] = if delta.abs() < turn {
                target
            } else if delta > 0.0 {
                if axis == YAW {
                    crate::player_angle_math::angle_mod(view + turn)
                } else {
                    crate::player_angle_math::normalized_angle(view + turn)
                }
            } else if axis == YAW {
                crate::player_angle_math::angle_mod(view - turn)
            } else {
                crate::player_angle_math::normalized_angle(view - turn)
            };
        }
    }
    for axis in 0..3 {
        let short = crate::npc_think::angle_to_short(state.view_angles[axis]);
        state.delta_angles[axis] = short.wrapping_sub(command.angles[axis]);
    }
    if fraction < HYPERSPACE_TELEPORT_FRAC {
        if matched < 3 {
            riding.hyperspace_time += millis;
        } else {
            riding.ready_for_hyperspace = true;
        }
    }
}
