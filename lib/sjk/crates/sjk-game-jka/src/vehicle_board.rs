//! Getting on and off a vehicle (`codemp/game/g_vehicles.c`): `ValidateBoard`, `Board`
//! with `SetPilot` and the passenger seats, `Ghost` and `UnGhost` for a vehicle that hides
//! its rider, `VEH_TryEject` and `Eject` — every side tried in turn — and `EjectAll`.
//!
//! The vehicle is an NPC's ([`NpcActor::vehicle`]); the functions reach it through a
//! [`Parent`] of borrows, which the vehicle's own move can build from its parts as well as
//! a caller between moves ([`Parent::of`]). Its rider is a player, borrowed for the work
//! ([`Rider`]), named by its entity number in the seats. The way off is traced through
//! whatever the caller hands in: the vehicle's move traces as the vehicle, which skips the
//! same entities the rider's trace would (the rider and the vehicle it owns).

use crate::event_entity::EventEntity;
use crate::npc_spawn::{NpcActor, NpcHost, es};
use crate::pmove::{MovementState, MovementTrace};
use crate::vehicle::{Vehicle, eject, flags};
use crate::vehicle_fields::kind;
use crate::vehicle_rider::{CONTENTS_BODY, ENTITYNUM_NONE, Rider, SVF_NOCLIENT};
use sjk_protocol::{EntityState, PlayerState, UserCommand};

/// `EV_GENERAL_SOUND`.
const EV_GENERAL_SOUND: u32 = 76;
/// `ps.loopSound`, `ps.m_iVehicleNum`; `s.loopSound`.
const PS_LOOP_SOUND: usize = 75;
const PS_VEHICLE: usize = 84;
const ES_LOOP_SOUND: usize = 55;
/// The vehicle spawner's `SUSPENDED` spawnflag: docked until boarded.
const SUSPENDED: i32 = 2;
/// `VEH_MOUNT_THROW_LEFT`, `VEH_MOUNT_THROW_RIGHT`: a speeder stolen from its pilot.
const VEH_MOUNT_THROW_LEFT: i32 = -5;
const VEH_MOUNT_THROW_RIGHT: i32 = -6;
/// `DEFAULT_MINS_2`, `DEFAULT_MAXS_2`.
pub(crate) const DEFAULT_MINS_2: f32 = -24.0;
const DEFAULT_MAXS_2: f32 = 40.0;
/// `MAX_CLIENTS`.
const MAX_CLIENTS: u16 = 32;
/// `YAW`.
const YAW: usize = 1;

/// A trace for the way off: start, mins, maxs, end, content mask.
pub type EjectTrace<'t> =
    dyn FnMut([f32; 3], [f32; 3], [f32; 3], [f32; 3], u32) -> MovementTrace + 't;

/// The vehicle NPC as the boarding functions reach it (`pVeh->m_pParentEntity`).
pub struct Parent<'a> {
    /// `s.number`.
    pub number: u16,
    /// `m_pVehicle`.
    pub vehicle: &'a mut Vehicle,
    /// Its movement: `ps.m_iVehicleNum` as a move reads it.
    pub state: &'a mut MovementState,
    /// `client->ps`: its loop sound and `m_iVehicleNum`.
    pub player: &'a mut PlayerState,
    /// `s`: its owner and loop sound.
    pub entity: &'a mut EntityState,
    /// `client->pers.cmd`.
    pub command: &'a mut UserCommand,
    /// `ent->health`, and `ent->spawnflags` where a boarding may clear `SUSPENDED`.
    pub health: i32,
    pub spawnflags: Option<&'a mut i32>,
    /// `r.currentAngles[YAW]`, `r.currentOrigin`, `r.maxs`, `clipmask`.
    pub yaw: f32,
    pub origin: [f32; 3],
    pub maxs: [f32; 3],
    pub clip_mask: u32,
}

impl<'a> Parent<'a> {
    /// The parent of the vehicle NPC `npc`, between moves; `None` for an NPC that is no
    /// vehicle.
    pub fn of(npc: &'a mut NpcActor) -> Option<Self> {
        let NpcActor {
            number,
            vehicle,
            movement,
            player,
            state,
            mind,
            health,
            spawnflags,
            current_origin,
            maxs,
            clip_mask,
            ..
        } = npc;
        Some(Self {
            number: *number,
            vehicle: vehicle.as_deref_mut()?,
            state: movement.state_mut(),
            player,
            entity: state,
            command: &mut mind.command,
            health: *health,
            spawnflags: Some(spawnflags),
            yaw: mind.current_angles[YAW],
            origin: *current_origin,
            maxs: *maxs,
            clip_mask: *clip_mask,
        })
    }

    /// `parent->client->ps.loopSound = parent->s.loopSound = sound`.
    fn set_loop_sound(&mut self, sound: i32) {
        self.player.set_raw_field(PS_LOOP_SOUND, sound as u32);
        self.entity.set_raw_field(ES_LOOP_SOUND, sound as u32);
    }

    /// `parent->r.ownerNum`, and `s.owner` ("for prediction"): the pilot, or none.
    fn set_owner(&mut self, owner: u16) {
        self.entity.set_raw_field(es::OWNER, u32::from(owner));
    }

    /// `parent->client->ps.m_iVehicleNum`: its pilot's number plus one, or 0.
    fn set_vehicle_number(&mut self, number: u16) {
        self.player.set_raw_field(PS_VEHICLE, u32::from(number));
        self.state.vehicle_entity_num = number;
    }

    /// `G_Sound(parent, CHAN_AUTO, sound)`: a sound event where the vehicle is.
    fn sound(&self, sound: u16, host: &mut impl NpcHost) {
        host.raise(EventEntity {
            event: EV_GENERAL_SOUND,
            parameter: u32::from(sound),
            origin: self.origin,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        });
    }
}

/// The vehicle's `r.ownerNum`: its pilot, `ENTITYNUM_NONE` without one.
pub fn owner_of(npc: &NpcActor) -> u16 {
    npc.vehicle
        .as_deref()
        .and_then(|vehicle| vehicle.pilot)
        .unwrap_or(ENTITYNUM_NONE)
}

/// `ValidateBoard` (`g_vehicles.c:162-260`): whether the rider may get on, and from which
/// side (`m_iBoarding` set to -1 left, -2 right, -3 from behind).
pub fn validate_board(parent: &mut Parent<'_>, rider: &Rider<'_>) -> bool {
    let vehicle = &mut *parent.vehicle;
    if vehicle.die_time > 0 {
        return false;
    }
    if vehicle.pilot.is_some() {
        match vehicle.kind() {
            kind::FIGHTER => return vehicle.passenger_count < vehicle.info.max_passengers,
            kind::WALKER if rider.movement.ground_entity_number != parent.number => return false,
            kind::SPEEDER => {
                return matches!(
                    vehicle.boarding,
                    VEH_MOUNT_THROW_LEFT | VEH_MOUNT_THROW_RIGHT
                );
            }
            _ => {}
        }
    } else if vehicle.kind() == kind::FIGHTER {
        return true;
    }
    let mut to_rider: [f32; 3] =
        std::array::from_fn(|axis| rider.movement.origin[axis] - parent.origin[axis]);
    to_rider[2] = 0.0;
    crate::saber_clash::normalize(&mut to_rider);
    let mut right = crate::pmove::flight::flight_axes([0.0, parent.yaw, 0.0])
        .1
        .to_array();
    crate::saber_clash::normalize(&mut right);
    let dot = to_rider[0] * right[0] + to_rider[1] * right[1] + to_rider[2] * right[2];
    vehicle.boarding = if dot >= 0.5 {
        -2
    } else if dot <= -0.5 {
        -1
    } else {
        -3
    };
    vehicle.boarding <= -1
}

/// `Board` (`g_vehicles.c:284-473`) for a player: the first to get on an empty vehicle
/// drives it, the next take the free seats. Returns whether it got on.
pub fn board(
    parent: &mut Parent<'_>,
    rider: &mut Rider<'_>,
    level_time: i32,
    host: &mut impl NpcHost,
) -> bool {
    if parent.health <= 0 || parent.vehicle.boarding > 0 || rider.vehicle() != 0 {
        return false;
    }
    if parent.vehicle.flags & flags::BUCKING != 0 {
        return false;
    }
    if !validate_board(parent, rider) {
        return false;
    }
    // "ALWAYS let the player be the pilot."
    let vehicle = &mut *parent.vehicle;
    vehicle.old_pilot = vehicle.pilot;
    if vehicle.pilot.is_none() {
        vehicle.pilot = Some(rider.number);
    } else if vehicle.passenger_count < vehicle.info.max_passengers {
        if let Some(slot) = vehicle.passengers.iter().position(Option::is_none) {
            vehicle.passengers[slot] = Some(rider.number);
            rider.set_passenger_slot(slot as u32 + 1);
        }
        vehicle.passenger_count += 1;
    } else {
        return false;
    }
    rider.set_vehicle(parent.number);
    let piloting = vehicle.pilot == Some(rider.number);
    let (sound_loop, sound_on, hide, drop_delay) = (
        vehicle.info.sound_loop,
        vehicle.info.sound_on,
        vehicle.info.hide_rider,
        vehicle.drop_delay,
    );
    if piloting {
        parent.set_owner(rider.number);
    }
    if let Some(spawnflags) = parent.spawnflags.as_deref_mut()
        && *spawnflags & SUSPENDED != 0
    {
        // Docked no longer: free to fall, after a moment's drop if it has one.
        *spawnflags &= !SUSPENDED;
        let release = host.sound_index(b"sound/vehicles/common/release.wav");
        parent.sound(release, host);
        if drop_delay != 0 {
            parent.vehicle.drop_time = level_time + drop_delay;
        }
    }
    if sound_loop != 0 {
        parent.set_loop_sound(sound_loop);
    }
    rider.set_owner(parent.number);
    if piloting {
        // "always gonna be under MAX_CLIENTS so no worries about 1 byte overflow"
        parent.set_vehicle_number(rider.number + 1);
    }
    if hide {
        ghost(rider);
    }
    if sound_on != 0 {
        parent.sound(sound_on as u16, host);
    }
    let orientation = parent.vehicle.orientation;
    rider.set_view_angle([orientation[0], orientation[1], 0.0]);
    if matches!(parent.vehicle.kind(), kind::WALKER | kind::FIGHTER) {
        // The walker's and the fighter's own `Board` (`WalkerNPC.c:58-67`,
        // `FighterNPC.c:140-150`): 1.5 s in which the rider does nothing, getting off
        // included.
        parent.vehicle.boarding = level_time + 1_500;
    }
    true
}

/// `Ghost` (`g_vehicles.c:1917-1935`): unseen and not solid.
pub(crate) fn ghost(rider: &mut Rider<'_>) {
    rider.body.server_flags |= SVF_NOCLIENT;
    rider.set_hidden(true);
    rider.body.contents = 0;
}

/// `UnGhost` (`g_vehicles.c:1938-1956`): seen and solid again.
fn unghost(rider: &mut Rider<'_>) {
    rider.body.server_flags &= !SVF_NOCLIENT;
    rider.set_hidden(false);
    rider.body.contents = CONTENTS_BODY;
}

/// `VEH_TryEject` (`g_vehicles.c:475-577`): where the rider would land getting off on side
/// `direction`, clear of the vehicle; `None` where it cannot.
fn try_eject(
    parent: &Parent<'_>,
    rider: &Rider<'_>,
    direction: i32,
    trace: &mut EjectTrace<'_>,
) -> Option<[f32; 3]> {
    let axis = crate::pmove::flight::angles_to_axis([0.0, parent.yaw, 0.0]);
    let (forward, left, up) = (axis[0], axis[1], axis[2]);
    let mut leave = match direction {
        eject::LEFT => left,
        eject::RIGHT => left.map(|value| -value),
        eject::FRONT => forward,
        eject::REAR => forward.map(|value| -value),
        eject::TOP => up,
        // "Bottom?": the direction is left as it was, which is nothing.
        _ => [0.0; 3],
    };
    crate::saber_clash::normalize(&mut leave);
    let bias = if parent.vehicle.kind() == kind::WALKER {
        1.0 + 0.2
    } else {
        1.0
    };
    let vehicle_diagonal =
        (parent.maxs[0] * parent.maxs[0] + parent.maxs[1] * parent.maxs[1]).sqrt();
    let mut maxs = rider.maxs;
    if rider.number < MAX_CLIENTS {
        // "player client mins and maxs are never stored permanently".
        maxs[0] = 15.0;
        maxs[1] = 15.0;
    }
    let rider_diagonal = (maxs[0] * maxs[0] + maxs[1] * maxs[1]).sqrt();
    let start = rider.movement.origin;
    let mut exit: [f32; 3] = std::array::from_fn(|index| {
        start[index] + leave[index] * ((vehicle_diagonal + rider_diagonal) * bias)
    });
    let found = trace(
        start,
        [-15.0, -15.0, DEFAULT_MINS_2],
        [15.0, 15.0, DEFAULT_MAXS_2],
        exit,
        rider.clip_mask,
    );
    if found.all_solid || found.start_solid {
        return None;
    }
    if found.fraction < 1.0 {
        if parent.clip_mask & rider.body.contents != 0 {
            // "the trace hit the vehicle, don't let them get out, just in case".
            return None;
        }
        exit = found.end_position;
    }
    Some(exit)
}

/// `Eject` (`g_vehicles.c:605-889`) for a player rider that is in the game: off on the
/// side the vehicle last used, or the next that is clear. A dead rider thrown off with
/// `force` gets off where it is when no side is clear. Returns whether it got off.
pub fn eject(
    parent: &mut Parent<'_>,
    rider: &mut Rider<'_>,
    force: bool,
    level_time: i32,
    trace: &mut EjectTrace<'_>,
) -> bool {
    let mut dead = rider.health < 1;
    let vehicle = &mut *parent.vehicle;
    if !force
        && !(vehicle.boarding == 0
            || vehicle.boarding == -999
            || (vehicle.boarding < -3 && vehicle.boarding >= -9))
    {
        dead = true;
        vehicle.boarding = 0;
        vehicle.was_boarding = false;
    }
    vehicle.eject_dir = vehicle.eject_dir.clamp(eject::LEFT, eject::BOTTOM);
    let first = vehicle.eject_dir;
    let exit = loop {
        if let Some(exit) = try_eject(parent, rider, parent.vehicle.eject_dir, trace) {
            break exit;
        }
        let vehicle = &mut *parent.vehicle;
        vehicle.eject_dir += 1;
        if vehicle.eject_dir > eject::BOTTOM {
            vehicle.eject_dir = eject::LEFT;
        }
        if vehicle.eject_dir == first {
            // "if he's dead.. just shove him in solid, who cares."
            if !dead || !force {
                return false;
            }
            break rider.movement.origin;
        }
    };
    // `G_SetOrigin`, and the state's origin.
    rider.movement.origin = exit;
    let passenger = parent.vehicle.passengers.contains(&Some(rider.number));
    if !leave_seat(parent, rider.number) {
        return false;
    }
    if passenger {
        // No longer anyone's passenger (`ps.generic1 = 0`, `g_vehicles.c:770-773`).
        rider.set_passenger_slot(0);
    }
    if parent.vehicle.info.hide_rider {
        unghost(rider);
    }
    after_seat_left(parent);
    rider.set_vehicle(0);
    rider.set_owner(ENTITYNUM_NONE);
    rider.movement.view_angles[0] = 0.0;
    rider.movement.view_angles[2] = 0.0;
    rider.movement.view_angles[YAW] = parent.vehicle.orientation[YAW];
    let view = rider.movement.view_angles;
    rider.set_view_angle(view);
    if rider.body.solid_hack != 0 {
        rider.body.solid_hack = 0;
        rider.body.contents = CONTENTS_BODY;
    }
    // `BG_SetLegsAnimTimer(0)`, `BG_SetTorsoAnimTimer(0)`.
    rider.movement.legs_timer = 0;
    rider.movement.torso_timer = 0;
    parent.vehicle.boarding = level_time + 1_000;
    true
}

/// `Eject` for a rider no longer in the game (`taintedRider`): its seat freed at once, the
/// vehicle boardable a second on.
pub fn eject_gone(parent: &mut Parent<'_>, number: u16, level_time: i32) {
    if !leave_seat(parent, number) {
        return;
    }
    after_seat_left(parent);
    parent.vehicle.boarding = level_time + 1_000;
}

/// Which seat of vehicle `vehicle` rider `number` has, as its `ps.generic1` says it: 0 for
/// the pilot's, the passenger seat's index plus one (`Board`, `g_vehicles.c:326-336`) —
/// after a pilot's leaving moved the first passenger to the controls and the rest up
/// (`g_vehicles.c:715-745`); `None` where it has none.
pub fn seat_of(vehicle: &crate::vehicle::Vehicle, number: u16) -> Option<u32> {
    if vehicle.pilot == Some(number) {
        return Some(0);
    }
    vehicle
        .passengers
        .iter()
        .position(|seat| *seat == Some(number))
        .map(|seat| seat as u32 + 1)
}

/// `EjectAll` (`g_vehicles.c:892-963`): everyone thrown off the top — the pilot, the old
/// pilot, then every passenger seat — and the passenger count cleared. The riders the
/// vehicle kills with it (`killRiderOnDeath`) are pushed onto `killed`, in that order, for
/// the caller's `G_Damage` (10000, `MOD_SUICIDE`). Only the riders handed in (the pilot and
/// the borrowed `passengers`) can be thrown off here.
pub fn eject_all(
    parent: &mut Parent<'_>,
    mut rider: Option<&mut Rider<'_>>,
    passengers: &mut [Rider<'_>],
    level_time: i32,
    trace: &mut EjectTrace<'_>,
    killed: &mut Vec<u16>,
) {
    let vehicle = &mut *parent.vehicle;
    vehicle.eject_dir = eject::TOP;
    vehicle.boarding = 0;
    vehicle.was_boarding = false;
    let kill = vehicle.info.kill_rider_on_death;
    for seat in [vehicle.pilot, vehicle.old_pilot] {
        let Some(number) = seat else { continue };
        if let Some(rider) = rider.as_deref_mut().filter(|rider| rider.number == number) {
            eject(parent, rider, true, level_time, trace);
            if kill {
                killed.push(number);
            }
        }
    }
    if parent.vehicle.passenger_count != 0 {
        for seat in 0..parent.vehicle.passengers.len() {
            let Some(number) = parent.vehicle.passengers[seat] else {
                continue;
            };
            if let Some(passenger) = passengers
                .iter_mut()
                .find(|passenger| passenger.number == number)
            {
                eject(parent, passenger, true, level_time, trace);
                if kill {
                    killed.push(number);
                }
            }
        }
        parent.vehicle.passenger_count = 0;
    }
}

/// The seat `number` had, left (`Eject`'s `getItOutOfMe`, `g_vehicles.c:697-786`): the
/// pilot's seat to the first passenger, or a passenger's freed. Whether `number` rode it.
fn leave_seat(parent: &mut Parent<'_>, number: u16) -> bool {
    let vehicle = &mut *parent.vehicle;
    if vehicle.pilot == Some(number) {
        vehicle.pilot = None;
        *parent.command = UserCommand::default();
        vehicle.ucmd = UserCommand::default();
        let mut owner = ENTITYNUM_NONE;
        let mut pilot_number = None;
        // The first passenger takes the controls. (Its `generic1`, and the moved-up
        // passengers', are their own states': the caller keeps them to [`seat_of`].)
        for index in 0..vehicle.passenger_count.max(0) as usize {
            let Some(passenger) = vehicle.passengers.get(index).copied().flatten() else {
                continue;
            };
            vehicle.pilot = Some(passenger);
            owner = passenger;
            pilot_number = Some(passenger + 1);
            vehicle.passengers[index] = None;
            for k in 1..vehicle.passenger_count as usize {
                if vehicle.passengers[k - 1].is_none() {
                    vehicle.passengers[k - 1] = vehicle.passengers[k];
                    vehicle.passengers[k] = None;
                }
            }
            vehicle.passenger_count -= 1;
            break;
        }
        parent.set_owner(owner);
        if let Some(number) = pilot_number {
            parent.set_vehicle_number(number);
        }
        return true;
    }
    if vehicle.old_pilot == Some(number) {
        vehicle.old_pilot = None;
        return true;
    }
    let Some(slot) = vehicle
        .passengers
        .iter()
        .position(|seat| *seat == Some(number))
    else {
        return false;
    };
    vehicle.passengers[slot] = None;
    vehicle.passenger_count -= 1;
    true
}

/// `Eject` after the seat is left (`g_vehicles.c:796-805`): an empty driver's seat turns
/// the engine off; an empty vehicle has no one aboard.
fn after_seat_left(parent: &mut Parent<'_>) {
    if parent.vehicle.pilot.is_none() {
        let empty = parent.vehicle.passenger_count == 0;
        parent.set_loop_sound(0);
        if empty {
            parent.set_vehicle_number(0);
        }
    }
}
