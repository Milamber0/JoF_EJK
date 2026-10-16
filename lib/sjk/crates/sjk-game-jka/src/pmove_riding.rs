//! What `Pmove` does differently for a player riding a vehicle (`pm->ps->m_iVehicleNum` of
//! a client, OpenJK `codemp/game/bg_pmove.c`): the hyperspace point faced, the rider's box
//! on a speeder or an animal (`PM_CheckDuck`), no physics of its own — it goes where the
//! vehicle goes — only the weapons a rider may hold (`PM_WeaponOkOnVehicle`), and its
//! animations overridden by the ride (`PM_VehicleWeaponAnimate`).
//!
//! The vehicle rides into the move as a [`Riding`] ([`Predictor::set_riding`]): what the
//! rider's move reads of it, and what that move does back to it — its command pushed up and
//! its `hyperSpaceTime` kept current while it faces the hyperspace point — which the game
//! reads back ([`Predictor::riding`]) and applies to the vehicle once the move is done.
//! The server and a client's prediction build it the same way.

use super::*;
use crate::pmove_anim::{
    SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_FLAG_RESTART,
};
use crate::vehicle_fields::kind;

/// `CLASS_VEHICLE`.
const CLASS_VEHICLE: i32 = 53;

/// `MINS_Z`, `DEFAULT_VIEWHEIGHT`.
const MINS_Z: f32 = -24.0;
const DEFAULT_VIEWHEIGHT: i32 = 36;
/// `EF_NODRAW`.
const EF_NODRAW: u32 = 1 << 8;
/// `WP_MELEE`, `WP_SABER`, `WP_BLASTER`.
const WP_MELEE: u8 = 2;
const WP_SABER: u8 = 3;
const WP_BLASTER: u8 = 4;
/// `BUTTON_ATTACK`, `BUTTON_ALT_ATTACK`.
const BUTTON_ATTACK: u16 = 1;
const BUTTON_ALT_ATTACK: u16 = 128;
/// `EV_SABER_ATTACK`.
const EV_SABER_ATTACK: u16 = 29;
/// `LS_R_TL2BR`.
const LS_R_TL2BR: u32 = crate::saber_move_data::movement::LS_R_TL2BR as u32;

/// The animations `PM_VehicleWeaponAnimate` plays (`anims.h`).
mod anim {
    pub const BOTH_ATTACK3: u16 = 115;
    pub const BOTH_VS_REV: u16 = 1_027;
    pub const BOTH_VS_AIR_G: u16 = 1_029;
    pub const BOTH_VS_LAND_G: u16 = 1_033;
    pub const BOTH_VS_LAND_SL: u16 = 1_034;
    pub const BOTH_VS_LAND_SR: u16 = 1_035;
    pub const BOTH_VS_IDLE: u16 = 1_036;
    pub const BOTH_VS_IDLE_G: u16 = 1_037;
    pub const BOTH_VS_IDLE_SL: u16 = 1_038;
    pub const BOTH_VS_IDLE_SR: u16 = 1_039;
    pub const BOTH_VS_ATL_S: u16 = 1_048;
    pub const BOTH_VS_ATR_S: u16 = 1_049;
    pub const BOTH_VS_ATR_G: u16 = 1_052;
    pub const BOTH_VS_ATL_G: u16 = 1_053;
    pub const BOTH_VS_ATF_G: u16 = 1_054;
    pub const BOTH_VT_WALK_REV: u16 = 1_063;
    pub const BOTH_VT_RUN_FWD: u16 = 1_066;
    pub const BOTH_VT_TURBO: u16 = 1_078;
    pub const BOTH_VT_IDLE: u16 = 1_081;
    pub const BOTH_VT_IDLE_S: u16 = 1_083;
    pub const BOTH_VT_IDLE_G: u16 = 1_084;
    pub const BOTH_VT_ATL_S: u16 = 1_086;
    pub const BOTH_VT_ATR_S: u16 = 1_087;
    pub const BOTH_VT_ATR_G: u16 = 1_090;
    pub const BOTH_VT_ATL_G: u16 = 1_091;
    pub const BOTH_VT_ATF_G: u16 = 1_092;
}

/// The vehicle a player rides, as its move reads it and changes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Riding {
    /// The vehicle's entity number (`pm->ps->m_iVehicleNum`).
    pub vehicle: u16,
    /// `m_pVehicleInfo->type`.
    pub kind: i32,
    /// Whether this rider is the vehicle's pilot.
    pub pilot: bool,
    /// The vehicle's `ps.speed` and its definition's `speedMax`.
    pub speed: f32,
    pub speed_max: f32,
    /// The vehicle's `m_vOrientation`.
    pub orientation: [f32; 3],
    /// The vehicle's `ps.hyperSpaceTime` and `ps.hyperSpaceAngles`; the move keeps the time
    /// current while it faces them.
    pub hyperspace_time: i32,
    pub hyperspace_angles: [f32; 3],
    /// What the move forced on the vehicle's command (`m_ucmd`'s forward, right and up
    /// moves), for the game to apply.
    pub command_moves: Option<(i8, i8, i8)>,
    /// The vehicle faces the hyperspace point: `EF2_HYPERSPACE` for the game to set.
    pub ready_for_hyperspace: bool,
    /// The rider does not fit on it (`PM_CheckDuck`): no contents for 200 ms
    /// (`client->solidHack`), for the game to set.
    pub solid_hack: bool,
    /// A ship boundary turning the vehicle back (`vehTurnaroundIndex`,
    /// [`crate::vehicle_triggers`]), where the caller knows the point.
    pub turnaround: Option<Turnaround>,
}

/// What `PM_VehForcedTurning` reads of a vehicle a boundary turns back: until when
/// (`vehTurnaroundTime`), where its point stands (`s.origin`), where the vehicle is
/// (`playerState->origin`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Turnaround {
    pub until: i32,
    pub target: [f32; 3],
    pub vehicle_origin: [f32; 3],
}

impl Riding {
    /// The vehicle `number` of `vehicle`'s definition, its parent's speed, as a rider's move
    /// reads it.
    pub fn of(number: u16, vehicle: &crate::vehicle::Vehicle, rider: u16, speed: f32) -> Self {
        Self {
            vehicle: number,
            kind: vehicle.kind(),
            pilot: vehicle.pilot == Some(rider),
            speed,
            speed_max: vehicle.info.speed_max,
            orientation: vehicle.orientation,
            hyperspace_time: vehicle.hyperspace_time,
            hyperspace_angles: vehicle.hyperspace_angles,
            command_moves: None,
            ready_for_hyperspace: false,
            solid_hack: false,
            turnaround: None,
        }
    }

    /// What the rider's move did to the vehicle, applied (`m_ucmd`, `hyperSpaceTime`).
    pub fn apply_to(&self, vehicle: &mut crate::vehicle::Vehicle) -> bool {
        if let Some((forward, right, up)) = self.command_moves {
            vehicle.ucmd.forward_move = forward;
            vehicle.ucmd.right_move = right;
            vehicle.ucmd.up_move = up;
        }
        vehicle.hyperspace_time = self.hyperspace_time;
        self.ready_for_hyperspace
    }
}

/// `PM_WeaponOkOnVehicle` (`bg_pmove.c:9713-9727`).
pub fn weapon_ok_on_vehicle(weapon: u8) -> bool {
    matches!(weapon, WP_MELEE | WP_SABER | WP_BLASTER)
}

impl Predictor {
    /// The vehicle this player's move rides, or `None` on foot.
    pub fn set_riding(&mut self, riding: Option<Riding>) {
        self.riding = riding;
    }

    /// The vehicle as the last move left it.
    pub fn riding(&self) -> Option<Riding> {
        self.riding
    }

    /// Whether this is a rider's move (`m_iVehicleNum` of a client, with its vehicle).
    pub(super) fn riding_move(&self) -> bool {
        self.riding.is_some()
            && self.state.vehicle_entity_num != 0
            && self.state.client_num < MAX_CLIENTS
    }

    /// An NPC that is no vehicle (`CLASS_VEHICLE`, 53) with an `m_iVehicleNum` (`bg_pmove.c:11027-11030`): a
    /// vehicle's droid unit, which goes where its vehicle puts it — and, once let go
    /// (`ENTITYNUM_NONE`), nowhere.
    pub(super) fn npc_aboard(&self) -> bool {
        self.npc
            .as_ref()
            .is_some_and(|body| body.class != CLASS_VEHICLE)
            && self.state.vehicle_entity_num != 0
    }

    /// `PmoveSingle`'s hyperspace (`bg_pmove.c:10579-10595`): a rider in the vehicle's
    /// hyperspace time faces its point; one a boundary turns back turns toward its point.
    pub(super) fn face_hyperspace(&mut self, command: &mut UserCommand, millis: i32) {
        if !self.riding_move() {
            return;
        }
        let Some(riding) = self.riding.as_mut() else {
            return;
        };
        if command.server_time.wrapping_sub(riding.hyperspace_time)
            < crate::vehicle_riders::HYPERSPACE_TIME
        {
            crate::vehicle_riders::face_hyperspace_point(
                &mut self.state,
                command,
                riding,
                frame_seconds(millis),
                millis,
            );
        } else if let Some(turnaround) = riding
            .turnaround
            .filter(|turnaround| turnaround.until > command.server_time)
        {
            forced_turning(
                &mut self.state,
                command,
                riding,
                turnaround,
                frame_seconds(millis),
            );
        }
    }

    /// `PM_CheckDuck` for a rider or a piloted vehicle (`bg_pmove.c:4414-4452`): no ducking
    /// or rolling; a vehicle keeps the box it was linked by; a rider of a speeder or an
    /// animal gets its own, and none where it does not fit. `None` for anyone else.
    pub(super) fn riding_check_duck(
        &mut self,
        collision: &impl MovementCollision,
    ) -> Option<Bounds> {
        let number = self.state.vehicle_entity_num;
        if number == 0 || number >= ENTITY_NUMBER_NONE {
            return None;
        }
        self.state.movement_flags &= !(PMF_DUCKED | crate::PMF_ROLLING);
        if self.state.client_num >= MAX_CLIENTS {
            let npc = self.npc?;
            return Some(Bounds {
                minimums: npc.mins,
                maximums: npc.maxs,
            });
        }
        let riding = self.riding?;
        if !matches!(riding.kind, kind::SPEEDER | kind::ANIMAL) {
            // A walker's or a fighter's rider: the cleared box of `ClientThink_real`, which
            // the function's common end stands up (neither ducked nor rolling).
            self.state.view_height = DEFAULT_VIEWHEIGHT;
            return Some(Bounds {
                minimums: [0.0; 3],
                maximums: [0.0, 0.0, self.state.standing_height],
            });
        }
        let bounds = Bounds {
            minimums: [-16.0, -16.0, MINS_Z],
            maximums: [16.0, 16.0, self.state.standing_height],
        };
        self.state.view_height = DEFAULT_VIEWHEIGHT;
        let origin = self.state.origin;
        let trace = collision.trace(
            origin,
            bounds.minimums,
            bounds.maximums,
            origin,
            PLAYER_CONTENT_MASK,
        );
        if trace.start_solid || trace.all_solid || trace.fraction != 1.0 {
            // "whoops, can't fit here. Down to 0!"
            if let Some(riding) = self.riding.as_mut() {
                riding.solid_hack = true;
            }
            // The function's common end still stands the box up (neither ducked nor
            // rolling): only its top comes back.
            return Some(Bounds {
                minimums: [0.0; 3],
                maximums: [0.0, 0.0, self.state.standing_height],
            });
        }
        Some(bounds)
    }

    /// `PmoveSingle`'s weapon for a rider (`bg_pmove.c:11081-11113`): a weapon the vehicle
    /// allows is forced where the one held or asked for is not; whether `PM_Weapon` runs —
    /// not inside the vehicle, nor with a weapon it does not allow.
    pub(super) fn rider_weapon(&mut self, command: &mut UserCommand) -> bool {
        if !self.riding_move() {
            return true;
        }
        let hidden = self.state.entity_flags & EF_NODRAW != 0;
        if !hidden
            && (!weapon_ok_on_vehicle(command.weapon) || !weapon_ok_on_vehicle(self.state.weapon))
        {
            if !weapon_ok_on_vehicle(self.state.weapon) {
                if let Some(weapon) = (0..crate::LEGACY_WEAPON_COUNT as u8).find(|weapon| {
                    self.state.weapons & (1 << weapon) != 0 && weapon_ok_on_vehicle(*weapon)
                }) {
                    command.weapon = weapon;
                    self.state.weapon = weapon;
                }
            } else {
                command.weapon = self.state.weapon;
            }
        }
        !hidden && weapon_ok_on_vehicle(command.weapon)
    }

    /// `PM_VehicleWeaponAnimate` (`bg_pmove.c:6407-6645`): the pilot of a speeder or an
    /// animal rides in the pose of its weapon — idle, reversing, or attacking to a side.
    pub(super) fn vehicle_weapon_animate(&mut self, command: &UserCommand) {
        let Some(riding) = self
            .riding
            .filter(|riding| riding.pilot && self.riding_move())
        else {
            return;
        };
        if matches!(riding.kind, kind::WALKER | kind::FIGHTER) {
            return;
        }
        let Some(lengths) = self.animation_lengths.clone() else {
            return;
        };
        let mut buttons = command.buttons;
        let mut seed = command.server_time;
        let (animation, flags) = loop {
            if buttons & (BUTTON_ATTACK | BUTTON_ALT_ATTACK) != 0 {
                let flags = SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD;
                match self.state.weapon {
                    WP_SABER => {
                        if buttons & BUTTON_ALT_ATTACK != 0 {
                            // "don't do anything.. I guess."
                            buttons &= !BUTTON_ALT_ATTACK;
                            continue;
                        }
                        if self.state.torso_timer <= 0 {
                            self.add_event(EV_SABER_ATTACK, 0);
                        }
                        self.state.saber_move = LS_R_TL2BR;
                        if self.state.torso_timer > 0
                            && matches!(
                                self.state.torso_anim,
                                anim::BOTH_VS_ATR_S | anim::BOTH_VS_ATL_S
                            )
                        {
                            return;
                        }
                        let animation = if command.right_move > 0 {
                            anim::BOTH_VS_ATR_S
                        } else if command.right_move < 0 {
                            anim::BOTH_VS_ATL_S
                        } else {
                            // `PM_irand_timesync(0, 1)`.
                            seed = 69_069_i32.wrapping_mul(seed).wrapping_add(1);
                            let random = ((seed as u32) & 0xffff) as f32 / 65_536.0;
                            let pick = ((-1.0 + random + 1.0) as i32).clamp(0, 1);
                            if pick == 0 {
                                anim::BOTH_VS_ATR_S
                            } else {
                                anim::BOTH_VS_ATL_S
                            }
                        };
                        let restart = if self.state.torso_timer <= 0 {
                            SETANIM_FLAG_RESTART
                        } else {
                            0
                        };
                        break (Some(animation), flags | restart);
                    }
                    WP_BLASTER => {
                        let animation = (self.state.torso_anim == anim::BOTH_ATTACK3).then(|| {
                            if command.right_move > 0 {
                                anim::BOTH_VS_ATR_G
                            } else if command.right_move < 0 {
                                anim::BOTH_VS_ATL_G
                            } else {
                                anim::BOTH_VS_ATF_G
                            }
                        });
                        break (animation, flags);
                    }
                    _ => break (Some(anim::BOTH_VS_IDLE), flags),
                }
            } else if riding.speed < 0.0 && riding.kind == kind::ANIMAL {
                break (Some(anim::BOTH_VT_WALK_REV), 0);
            } else if riding.speed < 0.0 && riding.kind == kind::SPEEDER {
                break (Some(anim::BOTH_VS_REV), 0);
            } else {
                let animation = match self.state.weapon {
                    WP_SABER if crate::pmove_locomotion::sabers_off_state(&self.state) => {
                        anim::BOTH_VS_IDLE
                    }
                    WP_SABER => anim::BOTH_VS_IDLE_SR,
                    WP_BLASTER => anim::BOTH_VS_IDLE_G,
                    _ => anim::BOTH_VS_IDLE,
                };
                break (Some(animation), 0);
            }
        };
        let Some(mut animation) = animation else {
            return;
        };
        if riding.kind == kind::ANIMAL {
            // "agh.. remap anims for the tauntaun"
            animation = match animation {
                anim::BOTH_VS_IDLE if riding.speed > 0.0 => {
                    if riding.speed > riding.speed_max {
                        anim::BOTH_VT_TURBO
                    } else {
                        anim::BOTH_VT_RUN_FWD
                    }
                }
                anim::BOTH_VS_IDLE => anim::BOTH_VT_IDLE,
                anim::BOTH_VS_ATR_S => anim::BOTH_VT_ATR_S,
                anim::BOTH_VS_ATL_S => anim::BOTH_VT_ATL_S,
                anim::BOTH_VS_ATR_G => anim::BOTH_VT_ATR_G,
                anim::BOTH_VS_ATL_G => anim::BOTH_VT_ATL_G,
                anim::BOTH_VS_ATF_G => anim::BOTH_VT_ATF_G,
                anim::BOTH_VS_IDLE_SL | anim::BOTH_VS_IDLE_SR => anim::BOTH_VT_IDLE_S,
                anim::BOTH_VS_IDLE_G => anim::BOTH_VT_IDLE_G,
                // "should not happen for tauntaun".
                anim::BOTH_VS_AIR_G
                | anim::BOTH_VS_LAND_SL
                | anim::BOTH_VS_LAND_SR
                | anim::BOTH_VS_LAND_G => return,
                other => other,
            };
        }
        crate::pmove_anim::set_animation(
            &mut self.state,
            SETANIM_BOTH,
            animation,
            flags,
            lengths.as_ref(),
        );
    }
}

/// `PM_VehForcedTurning` (`bg_pmove.c:9777-9824`, not `VEH_CONTROL_SCHEME_4`): the rider and
/// its vehicle's command pushed up and still, the rider's view turned 0.6 of the way per
/// second toward the boundary's point.
fn forced_turning(
    state: &mut MovementState,
    command: &mut UserCommand,
    riding: &mut Riding,
    turnaround: Turnaround,
    seconds: f32,
) {
    command.up_move = 127;
    command.forward_move = 0;
    command.right_move = 0;
    riding.command_moves = Some((0, 0, 127));
    const PITCH: usize = 0;
    const YAW: usize = 1;
    let toward: [f32; 3] =
        std::array::from_fn(|axis| turnaround.target[axis] - turnaround.vehicle_origin[axis]);
    let (pitch, yaw) = crate::damage::vector_to_angles(toward);
    let yaw_delta =
        crate::player_angle_math::angle_subtract(state.view_angles[YAW], yaw) * (0.6 * seconds);
    let pitch_delta =
        crate::player_angle_math::angle_subtract(state.view_angles[PITCH], pitch) * (0.6 * seconds);
    state.view_angles[YAW] =
        crate::player_angle_math::angle_subtract(state.view_angles[YAW], yaw_delta);
    state.view_angles[PITCH] =
        crate::player_angle_math::angle_subtract(state.view_angles[PITCH], pitch_delta);
    // `PM_SetPMViewAngle`.
    for axis in 0..3 {
        let short = crate::npc_think::angle_to_short(state.view_angles[axis]);
        state.delta_angles[axis] = short.wrapping_sub(command.angles[axis]);
    }
}
