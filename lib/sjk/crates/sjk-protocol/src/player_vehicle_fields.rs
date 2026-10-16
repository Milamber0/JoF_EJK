//! The vehicle-only protocol-26 player netfields: `playerState_t` members that only
//! `vehPlayerStateFields` (and, for `moveDir`, `pilotPlayerStateFields`) send in a
//! stock build with `_OPTIMIZED_VEHICLE_NETWORKING` (`codemp/qcommon/msg.cpp`); the
//! ordinary schema has none of them. A stock client predicts the vehicle it rides
//! from these, so [`PlayerState`] keeps them beside its ordinary fields.
use super::PlayerState;

/// `vehOrientation[0..3]`.
pub(super) const ORIENTATION: usize = 0;
/// `moveDir[0..3]`.
pub(super) const MOVE_DIR: usize = 3;
/// `vehTurnaroundTime`.
pub(super) const TURNAROUND_TIME: usize = 6;
/// `vehWeaponsLinked`.
pub(super) const WEAPONS_LINKED: usize = 7;
/// `hyperSpaceTime`.
pub(super) const HYPERSPACE_TIME: usize = 8;
/// `hyperSpaceAngles[0..3]`.
pub(super) const HYPERSPACE_ANGLES: usize = 9;
/// `vehBoarding`.
pub(super) const BOARDING: usize = 12;
/// `vehTurnaroundIndex`.
pub(super) const TURNAROUND_INDEX: usize = 13;
/// `vehSurfaces`.
pub(super) const SURFACES: usize = 14;
/// Raw storage slots for the vehicle-only netfields.
pub(super) const VEHICLE_ONLY_FIELDS: usize = 15;

/// Where a wire field of a player schema is kept in [`PlayerState`].
#[derive(Clone, Copy, Debug)]
pub(super) enum Slot {
    /// An ordinary-schema field, by its protocol-26 index.
    Field(usize),
    /// A vehicle-only field, by its slot above.
    VehicleOnly(usize),
}

/// The vehicle-only netfields of a player state, typed (`codemp/qcommon/q_shared.h`
/// `playerState_t`). Zero by default, as a state that never rode has them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VehicleNetFields {
    /// `vehOrientation`: the vehicle's pitch, yaw and roll.
    pub orientation: [f32; 3],
    /// `moveDir`: the direction the vehicle (or a pilot inside it) is moving.
    pub move_dir: [f32; 3],
    /// `vehTurnaroundTime`: until when an automatic turnaround steers.
    pub turnaround_time: i32,
    /// `vehWeaponsLinked`: the vehicle fires its linked weapons together.
    pub weapons_linked: bool,
    /// `hyperSpaceTime`: when the hyperspace jump started.
    pub hyperspace_time: i32,
    /// `hyperSpaceAngles`: the direction of the hyperspace jump.
    pub hyperspace_angles: [f32; 3],
    /// `vehBoarding`: a rider is boarding (a plain 1 or 0 on the wire).
    pub boarding: bool,
    /// `vehTurnaroundIndex`: the entity an automatic turnaround steers away from;
    /// ten bits on the wire.
    pub turnaround_index: i32,
    /// `vehSurfaces`: bits of broken-off surfaces; sixteen bits on the wire.
    pub surfaces: i32,
}

impl PlayerState {
    /// The vehicle-only netfields, which only a vehicle's own player state (and a
    /// pilot's `moveDir`) carries on the wire.
    pub fn vehicle_fields(&self) -> VehicleNetFields {
        let raw = &self.vehicle_only;
        let floats = |at: usize| std::array::from_fn(|i| f32::from_bits(raw[at + i]));
        VehicleNetFields {
            orientation: floats(ORIENTATION),
            move_dir: floats(MOVE_DIR),
            turnaround_time: raw[TURNAROUND_TIME] as i32,
            weapons_linked: raw[WEAPONS_LINKED] != 0,
            hyperspace_time: raw[HYPERSPACE_TIME] as i32,
            hyperspace_angles: floats(HYPERSPACE_ANGLES),
            boarding: raw[BOARDING] != 0,
            turnaround_index: raw[TURNAROUND_INDEX] as i32,
            surfaces: raw[SURFACES] as i32,
        }
    }

    /// Server side: set the vehicle-only netfields. Integers are sent cut to their
    /// wire widths, as `MSG_WriteBits` cuts them.
    pub fn set_vehicle_fields(&mut self, fields: &VehicleNetFields) {
        let raw = &mut self.vehicle_only;
        for (at, values) in [
            (ORIENTATION, fields.orientation),
            (MOVE_DIR, fields.move_dir),
            (HYPERSPACE_ANGLES, fields.hyperspace_angles),
        ] {
            for (i, value) in values.iter().enumerate() {
                raw[at + i] = value.to_bits();
            }
        }
        raw[TURNAROUND_TIME] = fields.turnaround_time as u32;
        raw[WEAPONS_LINKED] = u32::from(fields.weapons_linked);
        raw[HYPERSPACE_TIME] = fields.hyperspace_time as u32;
        raw[BOARDING] = u32::from(fields.boarding);
        raw[TURNAROUND_INDEX] = fields.turnaround_index as u32;
        raw[SURFACES] = fields.surfaces as u32;
    }

    /// The raw 32 bits a schema slot holds.
    pub(super) fn slot(&self, slot: Slot) -> u32 {
        match slot {
            Slot::Field(index) => self.fields[index],
            Slot::VehicleOnly(index) => self.vehicle_only[index],
        }
    }

    /// The raw 32 bits a schema slot holds, to overwrite.
    pub(super) fn slot_mut(&mut self, slot: Slot) -> &mut u32 {
        match slot {
            Slot::Field(index) => &mut self.fields[index],
            Slot::VehicleOnly(index) => &mut self.vehicle_only[index],
        }
    }
}
