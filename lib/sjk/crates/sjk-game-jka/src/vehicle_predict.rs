//! A client's prediction of its own ride: `CG_PredictPlayerState`'s vehicle branch
//! (OpenJK `codemp/cgame/cg_predict.c:1283-1386`) and the `_CGAME` half of `PmoveSingle`'s
//! vehicle block (`codemp/game/bg_pmove.c:10966-11016`, `11138-11154`).
//!
//! The stock cgame predicts a ride through two moves per command: the pilot's own (it goes
//! nowhere of itself, [`crate::pmove::riding`]), then the vehicle's over the vehicle's
//! player state from the snapshot (`vps`), driven by the pilot's command. The game's
//! `Update` does not run on a client; in its place the move steers the vehicle itself —
//! `ProcessOrientCommands` and `ProcessMoveCommands`, the vehicle's and the pilot's view,
//! the move's direction — and `AttachRidersGeneric` carries the pilot to the driver tag.
//! Boarding is not predicted: while the vehicle boards it stands still.
//!
//! [`RidePrediction`] holds the vehicle's half; the caller keeps the pilot's `Predictor`
//! and hands both to [`RidePrediction::predict`] per command.
//!
//! A walker's move is the server's (`WalkerNPC.c`'s `ProcessMoveCommands`,
//! `ProcessOrientCommands`, through the same dispatch).
//!
//! Not here yet: a fighter's `BG_FighterUpdate` (a later slice's, as on the server), and
//! the hyperspace point (`PM_VehFaceHyperspacePoint` reads the vehicle's
//! `hyperSpaceTime`, which only a hyperspace trigger sets).

use std::sync::Arc;

use sjk_protocol::{PlayerState, UserCommand};

use crate::pmove::npc::NpcBody;
use crate::pmove::riding::{Riding, Turnaround};
use crate::pmove::vehicle::VehicleGame;
use crate::pmove::{MoveContext, MovementCollision, MovementConfig, MovementState, Predictor};
use crate::vehicle::Vehicle;
use crate::vehicle_fields::kind;
use crate::vehicle_move::{Steering, process_move_commands, process_orient_commands};

/// `BUTTON_TALK`.
const BUTTON_TALK: u16 = 2;
/// `CLASS_VEHICLE`.
const CLASS_VEHICLE: i32 = 53;
/// `PM_INTERMISSION`.
const PM_INTERMISSION: u8 = 7;
/// `EF_DEAD`.
const EF_DEAD: u32 = 1 << 1;
/// The player-state field of `eFlags2`.
const PS_EFLAGS2: usize = 103;
/// `PITCH`, `YAW`, `ROLL`.
const PITCH: usize = 0;
const YAW: usize = 1;
const ROLL: usize = 2;

/// The vehicle half of a client's ride prediction (`cg.predictedVehicleState` and the
/// vehicle's `m_pVehicle`).
pub struct RidePrediction {
    /// The vehicle's entity number and its pilot's client number.
    number: u16,
    pilot: u16,
    /// The vehicle's move; its [`Vehicle`] is in it only during a command.
    vehicle: Predictor,
    own: Option<Box<Vehicle>>,
    /// `parentPS->electrifyTime`, as the snapshot gave it.
    electrify_time: i32,
    /// The driver tag's place in the vehicle's model, along its yaw (forward, left, up).
    driver_offset: [f32; 3],
    /// The vehicle's `eFlags2` from the snapshot (`EF2_HYPERSPACE`).
    flags2: u32,
    /// `bg_fighterAltControl`, from the server's system info.
    fighter_alt_control: bool,
    /// Where the point a ship boundary turns the vehicle toward stands
    /// (`cg_entities[vehTurnaroundIndex].currentState.origin`), where the client has it.
    turnaround_target: Option<[f32; 3]>,
}

impl RidePrediction {
    /// The ride predicted from a snapshot: the vehicle's player state (`vps`), a fresh
    /// vehicle of its definition (`template`, [`Vehicle::new`] of the client's table), its
    /// entity's `solid` (the box `cg_predict.c:1330-1341` decodes), its pilot and the
    /// pilot's `ps.commandTime`, which the vehicle's prediction starts from
    /// (`cg_predict.c:1100`).
    pub fn new(
        vehicle_state: &PlayerState,
        template: &Vehicle,
        solid: u32,
        pilot: u16,
        command_time: i32,
        config: MovementConfig,
    ) -> Self {
        let mut ride = Self {
            number: 0,
            pilot,
            vehicle: Predictor::from_player_state(vehicle_state, config),
            own: Some(Box::new(template.clone())),
            electrify_time: 0,
            driver_offset: [0.0; 3],
            flags2: 0,
            fighter_alt_control: false,
            turnaround_target: None,
        };
        ride.reseed(vehicle_state, template, solid, pilot, command_time, config);
        ride
    }

    /// Replace networked state from the next snapshot, retaining local vehicle timers
    /// for the same ride, pilot and definition. The driver offset and animation lengths
    /// are kept as well.
    pub fn reseed(
        &mut self,
        vehicle_state: &PlayerState,
        template: &Vehicle,
        solid: u32,
        pilot: u16,
        command_time: i32,
        config: MovementConfig,
    ) {
        let fields = vehicle_state.vehicle_fields();
        let vehicle = match self.own.as_deref_mut() {
            Some(own) => {
                // cgame retains the entity's Vehicle_t across snapshot replay.
                // Turbo recharge and other local vehicle timers are not in vps;
                // resetting them makes an exhausted boost predict as available
                // again on every snapshot, then snap back to the server speed.
                if self.number != vehicle_state.client_num()
                    || self.pilot != pilot
                    || !Arc::ptr_eq(&own.info, &template.info)
                {
                    own.copy_from(template);
                }
                own
            }
            None => self.own.insert(Box::new(template.clone())),
        };
        vehicle.pilot = Some(pilot);
        vehicle.orientation = fields.orientation;
        vehicle.prev_orientation = fields.orientation;
        vehicle.boarding = i32::from(fields.boarding);
        vehicle.ps_boarding = fields.boarding;
        vehicle.removed_surfaces = fields.surfaces;
        vehicle.ps_surfaces = fields.surfaces;
        vehicle.hyperspace_time = fields.hyperspace_time;
        vehicle.hyperspace_angles = fields.hyperspace_angles;
        vehicle.turnaround_index = fields.turnaround_index as u16;
        vehicle.turnaround_time = fields.turnaround_time;
        let (mins, maxs) = solid_box(solid);
        let lengths = self.vehicle.shared_animation_lengths();
        self.vehicle = Predictor::from_player_state(vehicle_state, config);
        if let Some(lengths) = lengths {
            self.vehicle.set_animation_lengths(lengths);
        }
        self.vehicle.state_mut().command_time = command_time;
        self.vehicle.set_npc(Some(NpcBody {
            class: CLASS_VEHICLE,
            mins,
            maxs,
            humanoid: false,
            flags2: vehicle_state.raw_field(PS_EFLAGS2).unwrap_or(0),
        }));
        self.vehicle.set_move_dir(fields.move_dir);
        self.number = vehicle_state.client_num();
        self.pilot = pilot;
        self.electrify_time = vehicle_state.electrify_time();
        self.flags2 = vehicle_state.raw_field(PS_EFLAGS2).unwrap_or(0);
    }

    /// `bg_fighterAltControl` as the server's system info sets it: free pitch and roll in
    /// a fighter.
    pub fn set_fighter_alt_control(&mut self, on: bool) {
        self.fighter_alt_control = on;
    }

    /// Where the point the vehicle's `vehTurnaroundIndex` names stands, when the client has
    /// that entity (`PM_VehForcedTurning` turns the pilot toward it).
    pub fn set_turnaround_target(&mut self, target: Option<[f32; 3]>) {
        self.turnaround_target = target;
    }

    /// Where the pilot sits on the vehicle's model (`*driver`), when the client has it.
    pub fn set_driver_offset(&mut self, offset: [f32; 3]) {
        self.driver_offset = offset;
    }

    /// The vehicle skeleton's animation lengths, for its own animations.
    pub fn set_animation_lengths(&mut self, lengths: Arc<dyn crate::pmove_anim::AnimationLengths>) {
        self.vehicle.set_animation_lengths(lengths);
    }

    /// The vehicle's entity number.
    pub fn number(&self) -> u16 {
        self.number
    }

    /// The predicted vehicle state (`cg.predictedVehicleState`).
    pub fn state(&self) -> &MovementState {
        self.vehicle.state()
    }

    /// The predicted vehicle's `m_iTurboTime` (`CG_DrawVehicleTurboRecharge` and
    /// `CG_DrawVehicleSpeed` read it): when the turbo in use ends, and so when the recharge
    /// starts. `None` while the vehicle is out of its holder, which is only during a command.
    pub fn turbo_time(&self) -> Option<i32> {
        self.own.as_ref().map(|vehicle| vehicle.turbo_time)
    }

    /// The predicted `vehOrientation`.
    pub fn orientation(&self) -> [f32; 3] {
        self.own
            .as_ref()
            .map_or([0.0; 3], |vehicle| vehicle.orientation)
    }

    /// One command of the ride (`cg_predict.c:1281-1386`): the pilot's move riding the
    /// vehicle, then — while it still pilots it, outside an intermission — the vehicle's
    /// move by the same command (the chat bubble's holding it still), which steers the
    /// vehicle and carries the pilot along.
    pub fn predict(
        &mut self,
        pilot: &mut Predictor,
        command: UserCommand,
        collision: &impl MovementCollision,
    ) {
        let Some(mut vehicle) = self.own.take() else {
            return;
        };
        let turnaround = self
            .turnaround_target
            .filter(|_| vehicle.turnaround_index != 0)
            .map(|target| Turnaround {
                until: vehicle.turnaround_time,
                target,
                vehicle_origin: self.vehicle.state().origin,
            });
        pilot.set_riding(Some(Riding {
            turnaround,
            ..Riding::of(
                self.number,
                &vehicle,
                self.pilot,
                self.vehicle.state().speed,
            )
        }));
        pilot.predict_command(command, collision);
        let mut own = command;
        if own.buttons & BUTTON_TALK != 0 {
            own.buttons = BUTTON_TALK;
            own.forward_move = 0;
            own.right_move = 0;
            own.up_move = 0;
        }
        vehicle.ucmd = own;
        let state = pilot.state();
        if state.vehicle_entity_num != self.number || state.movement_type == PM_INTERMISSION {
            self.own = Some(vehicle);
            return;
        }
        self.vehicle.set_vehicle(Some(vehicle));
        let mut game = ClientVehicle {
            pilot: pilot.state_mut(),
            pilot_number: self.pilot,
            electrify_time: self.electrify_time,
            driver_offset: self.driver_offset,
            number: self.number,
            flags2: self.flags2,
            fighter_alt_control: self.fighter_alt_control,
        };
        self.vehicle
            .predict_vehicle_command(own, collision, &MoveContext::CLIENT, &mut game);
        self.own = self.vehicle.take_vehicle();
    }
}

impl Clone for RidePrediction {
    fn clone(&self) -> Self {
        Self {
            number: self.number,
            pilot: self.pilot,
            vehicle: self.vehicle.clone(),
            own: self.own.clone(),
            electrify_time: self.electrify_time,
            driver_offset: self.driver_offset,
            flags2: self.flags2,
            fighter_alt_control: self.fighter_alt_control,
            turnaround_target: self.turnaround_target,
        }
    }

    /// Reuses the vehicle's storage: a client copies its committed ride into the frame's
    /// preview every frame.
    fn clone_from(&mut self, source: &Self) {
        self.number = source.number;
        self.pilot = source.pilot;
        self.vehicle.clone_from(&source.vehicle);
        match (self.own.as_deref_mut(), source.own.as_deref()) {
            (Some(own), Some(theirs)) => own.copy_from(theirs),
            _ => self.own.clone_from(&source.own),
        }
        self.electrify_time = source.electrify_time;
        self.driver_offset = source.driver_offset;
        self.flags2 = source.flags2;
        self.fighter_alt_control = source.fighter_alt_control;
        self.turnaround_target = source.turnaround_target;
    }
}

/// The box `cg_predict.c:1330-1341` makes of a vehicle's `s.solid`: its half width, how
/// far down, and (read from the wrong bits, 32 less) how far up.
pub fn solid_box(solid: u32) -> ([f32; 3], [f32; 3]) {
    let x = (solid & 255) as f32;
    let down = ((solid >> 8) & 255) as f32;
    let up = ((solid >> 15) & 255) as f32 - 32.0;
    ([-x, -x, -down], [x, x, up])
}

/// `PM_SetPMViewAngle` (`bg_pmove.c:1311-1323`): the delta angles against `command`, and
/// the view.
fn set_pm_view_angle(state: &mut MovementState, angles: [f32; 3], command: &UserCommand) {
    for axis in 0..3 {
        let short = crate::npc_think::angle_to_short(angles[axis]);
        state.delta_angles[axis] = short.wrapping_sub(i32::from(command.angles[axis]));
    }
    state.view_angles = angles;
}

/// The client's side of the vehicle's move: what `PmoveSingle` does under `_CGAME` where
/// the server runs the game's `Update`.
struct ClientVehicle<'p> {
    pilot: &'p mut MovementState,
    pilot_number: u16,
    electrify_time: i32,
    driver_offset: [f32; 3],
    /// The vehicle's number and its `eFlags2` as the snapshot gave them;
    /// `bg_fighterAltControl`.
    number: u16,
    flags2: u32,
    fighter_alt_control: bool,
}

impl VehicleGame for ClientVehicle<'_> {
    fn update(
        &mut self,
        vehicle: &mut Vehicle,
        state: &mut MovementState,
        command: &UserCommand,
        move_dir: &mut [f32; 3],
        move_box: ([f32; 3], [f32; 3]),
        collision: &dyn MovementCollision,
    ) {
        // `PM_VehicleViewAngles` (`bg_pmove.c:9730-9906`) for the pilot: its pitch within
        // the vehicle's `lookPitch`.
        let limit = vehicle.info.look_pitch;
        let mut view = self.pilot.view_angles;
        if limit != 0.0 {
            view[PITCH] = view[PITCH].clamp(-limit, limit);
        }
        set_pm_view_angle(self.pilot, view, &vehicle.ucmd);
        if vehicle.ps_boarding || vehicle.boarding != 0 {
            // "boarding is not predicted": still, the views the vehicle's.
            state.speed = 0.0;
            set_pm_view_angle(self.pilot, vehicle.orientation, command);
            set_pm_view_angle(state, vehicle.orientation, command);
            return;
        }
        if vehicle.kind() == kind::FIGHTER {
            // "client must explicitly call this for prediction": the pilot's gravity, the
            // move's box.
            let mut trace =
                |start, mins, maxs, end, mask| collision.trace(start, mins, maxs, end, mask);
            crate::vehicle_fighter::update(
                vehicle,
                state,
                self.pilot.gravity,
                move_box.0,
                move_box.1,
                &mut trace,
            );
        }
        vehicle.pilot = Some(self.pilot_number);
        vehicle.prev_orientation = vehicle.orientation;
        let steering = Steering {
            time: command.server_time,
            command_time: command.server_time,
            rider_yaw: self.pilot.view_angles[YAW],
            rider_pitch: self.pilot.view_angles[PITCH],
            rider_player: true,
            electrify_time: self.electrify_time,
            pilot_weapon: Some((
                self.pilot.weapon,
                crate::pmove_locomotion::sabers_off_state(self.pilot),
            )),
            fighter: crate::vehicle_fighter::FighterSteering {
                server: false,
                parent_number: self.number,
                hyperspace: self.flags2 & crate::vehicle_riders::EF2_HYPERSPACE != 0,
                dead: state.entity_flags & EF_DEAD != 0,
                unrestrained: self.fighter_alt_control
                    && vehicle.kind() == kind::FIGHTER
                    && self.pilot.vehicle_entity_num != 0,
                rider_roll: self.pilot.view_angles[ROLL],
                pilot_player: true,
                ..Default::default()
            },
        };
        process_orient_commands(vehicle, state, &steering);
        set_pm_view_angle(state, vehicle.orientation, command);
        process_move_commands(vehicle, state, &steering, move_dir);
        let view = self.pilot.view_angles;
        set_pm_view_angle(
            self.pilot,
            [view[PITCH], view[YAW], vehicle.orientation[ROLL]],
            command,
        );
        // The move's direction: a fighter's whole orientation, anyone else's yaw.
        let facing = if vehicle.kind() == kind::FIGHTER {
            vehicle.orientation
        } else {
            [0.0, vehicle.orientation[YAW], 0.0]
        };
        *move_dir = crate::pmove::flight::flight_axes(facing).0.to_array();
    }

    fn attach_riders(&mut self, vehicle: &mut Vehicle, state: &MovementState) {
        // `AttachRidersGeneric` (`bg_vehicleLoad.c:1455-1474`).
        if vehicle.pilot == Some(self.pilot_number) {
            self.pilot.origin = crate::vehicle_riders::driver_origin(
                state.origin,
                state.view_angles[YAW],
                self.driver_offset,
            );
        }
    }
}
