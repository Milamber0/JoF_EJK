use crate::{
    ENTITY_NUMBER_NONE, EntityDeltaError, EntityState, GameState, LEGACY_ENTITY_FIELDS,
    LEGACY_ENTITY_NUMBER_BITS, MessageError, MessageReader, ServiceCommand, read_delta_entity,
};
use std::error::Error;
use std::fmt;
#[path = "player_vehicle_fields.rs"]
mod vehicle_fields;
#[path = "snapshot_write.rs"]
pub(crate) mod write;
use vehicle_fields::Slot::{self, Field, VehicleOnly};
pub use vehicle_fields::VehicleNetFields;
use vehicle_fields::{
    BOARDING, HYPERSPACE_ANGLES, HYPERSPACE_TIME, MOVE_DIR, ORIENTATION, SURFACES,
    TURNAROUND_INDEX, TURNAROUND_TIME, VEHICLE_ONLY_FIELDS, WEAPONS_LINKED,
};

const PLAYER_FIELD_WIDTHS: [i8; 137] = [
    32, 0, 0, 0, 0, 0, 0, 0, 0, 8, -16, 16, 0, 16, 16, 16, 10, 32, 8, 16, 16, 16, -8, 4, 10, 4, 32,
    10, 10, 8, 4, 10, 8, 4, 32, 10, 10, -16, 16, 8, 8, -16, 8, 10, 10, 8, 16, 8, 16, 1, 0, 32, 2,
    32, 8, 1, 10, 8, 8, 1, 8, 2, 4, 8, 8, -16, 10, 32, 32, 1, 8, 32, 6, 32, 0, 16, 1, 8, 2, 32, 8,
    2, 32, 8, 10, 8, 10, 1, 1, 16, 2, 32, 32, 8, 1, 0, 32, 32, 16, 16, 0, 16, 0, 10, 16, 0, 1, 32,
    16, 2, 10, 1, 10, 32, 1, 1, 1, 1, 32, 1, 1, 6, 10, 10, 16, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
];
const MAX_ARRAY_VALUES: usize = 16;
const STAT_WEAPONS: usize = 4;
const MAX_WEAPONS: u8 = 19;
const FLOAT_INT_BITS: u8 = 13;
const FLOAT_INT_BIAS: i32 = 1 << (FLOAT_INT_BITS - 1);
const MAX_AREA_MASK: usize = 32;
const PILOT_FIELD_SPECS: &[(Slot, i8)] = &[
    (Field(0), 32),                 // commandTime
    (Field(1), 0),                  // origin[1]
    (Field(2), 0),                  // origin[0]
    (Field(3), 0),                  // viewangles[1]
    (Field(4), 0),                  // viewangles[0]
    (Field(5), 0),                  // origin[2]
    (Field(10), -16),               // weaponTime
    (Field(11), 16),                // delta_angles[1]
    (Field(14), 16),                // delta_angles[0]
    (Field(17), 32),                // eFlags
    (Field(19), 16),                // eventSequence
    (Field(24), 10),                // rocketLockIndex
    (Field(27), 10),                // events[0]
    (Field(28), 10),                // events[1]
    (Field(33), 4),                 // weaponstate
    (Field(38), 16),                // pm_flags
    (Field(41), -16),               // pm_time
    (Field(43), 10),                // clientNum
    (Field(47), 8),                 // weapon
    (Field(48), 16),                // delta_angles[2]
    (Field(50), 0),                 // viewangles[2]
    (Field(56), 10),                // externalEvent
    (Field(60), 8),                 // eventParms[1]
    (Field(63), 8),                 // pm_type
    (Field(64), 8),                 // externalEventParm
    (Field(65), -16),               // eventParms[0]
    (Field(67), 32),                // weaponChargeSubtractTime
    (Field(68), 32),                // weaponChargeTime
    (Field(71), 32),                // rocketTargetTime
    (Field(74), 0),                 // fd.forceJumpZStart
    (Field(79), 32),                // rocketLockTime
    (Field(84), 10),                // m_iVehicleNum
    (Field(85), 8),                 // generic1
    (Field(103), 10),               // eFlags2
    (Field(13), 16),                // legsAnim
    (Field(15), 16),                // torsoAnim
    (Field(20), 16),                // torsoTimer
    (Field(21), 16),                // legsTimer
    (Field(39), 8),                 // jetpackFuel
    (Field(40), 8),                 // cloakFuel
    (Field(49), 1),                 // saberCanThrow
    (Field(53), 32),                // fd.forcePowerDebounce[FP_LEVITATION]
    (Field(55), 1),                 // torsoFlip
    (Field(69), 1),                 // legsFlip
    (Field(82), 32),                // fd.forcePowersActive
    (Field(87), 1),                 // hasDetPackPlanted
    (Field(96), 32),                // fd.forceRageRecoveryTime
    (Field(88), 1),                 // saberInFlight
    (Field(98), 16),                // fd.forceMindtrickTargetIndex
    (Field(99), 16),                // fd.forceMindtrickTargetIndex2
    (Field(101), 16),               // fd.forceMindtrickTargetIndex3
    (Field(104), 16),               // fd.forceMindtrickTargetIndex4
    (Field(106), 1),                // fd.sentryDeployed
    (Field(109), 2),                // fd.forcePowerLevel[FP_SEE]
    (Field(113), 32),               // holocronBits
    (Field(18), 8),                 // fd.forcePower
    (Field(6), 0),                  // velocity[0]
    (Field(7), 0),                  // velocity[1]
    (Field(8), 0),                  // velocity[2]
    (Field(9), 8),                  // bobCycle
    (Field(12), 0),                 // speed
    (Field(16), 10),                // groundEntityNum
    (Field(22), -8),                // viewheight
    (Field(23), 4),                 // fd.saberAnimLevel
    (Field(25), 4),                 // fd.saberDrawAnimLevel
    (Field(26), 32),                // genericEnemyIndex
    (Field(29), 8),                 // customRGBA[0]
    (Field(30), 4),                 // movementDir
    (Field(31), 10),                // saberEntityNum
    (Field(32), 8),                 // customRGBA[3]
    (Field(34), 32),                // saberMove
    (Field(35), 10),                // standheight
    (Field(36), 10),                // crouchheight
    (Field(37), -16),               // basespeed
    (Field(42), 8),                 // customRGBA[1]
    (Field(44), 10),                // duelIndex
    (Field(45), 8),                 // customRGBA[2]
    (Field(46), 16),                // gravity
    (Field(51), 32),                // fd.forcePowersKnown
    (Field(52), 2),                 // fd.forcePowerLevel[FP_LEVITATION]
    (Field(54), 8),                 // fd.forcePowerSelected
    (Field(57), 8),                 // damageYaw
    (Field(58), 8),                 // damageCount
    (Field(59), 1),                 // inAirAnim
    (Field(61), 2),                 // fd.forceSide
    (Field(62), 4),                 // saberAttackChainCount
    (Field(66), 10),                // lookTarget
    (VehicleOnly(MOVE_DIR + 1), 0), // moveDir[1]
    (VehicleOnly(MOVE_DIR), 0),     // moveDir[0]
    (Field(70), 8),                 // damageEvent
    (VehicleOnly(MOVE_DIR + 2), 0), // moveDir[2]
    (Field(72), 6),                 // activeForcePass
    (Field(73), 32),                // electrifyTime
    (Field(78), 2),                 // damageType
    (Field(75), 16),                // loopSound
    (Field(76), 1),                 // hasLookTarget
    (Field(77), 8),                 // saberBlocked
    (Field(80), 8),                 // forceHandExtend
    (Field(81), 2),                 // saberHolstered
    (Field(83), 8),                 // damagePitch
    (Field(86), 10),                // jumppad_ent
    (Field(89), 16),                // forceDodgeAnim
    (Field(90), 2),                 // zoomMode
    (Field(91), 32),                // hackingTime
    (Field(92), 32),                // zoomTime
    (Field(93), 8),                 // brokenLimbs
    (Field(94), 1),                 // zoomLocked
    (Field(95), 0),                 // zoomFov
    (Field(97), 32),                // fallingToDeath
    (Field(100), 0),                // lastHitLoc[2]
    (Field(102), 0),                // lastHitLoc[0]
    (Field(105), 0),                // lastHitLoc[1]
    (Field(107), 32),               // saberLockTime
    (Field(108), 16),               // saberLockFrame
    (Field(110), 10),               // saberLockEnemy
    (Field(111), 1),                // fd.forceGripCripple
    (Field(112), 10),               // emplacedIndex
    (Field(114), 1),                // isJediMaster
    (Field(115), 1),                // forceRestricted
    (Field(116), 1),                // trueJedi
    (Field(117), 1),                // trueNonJedi
    (Field(118), 32),               // duelTime
    (Field(119), 1),                // duelInProgress
    (Field(120), 1),                // saberLockAdvance
    (Field(121), 6),                // heldByClient
    (Field(122), 10),               // ragAttach
    (Field(123), 10),               // iModelScale
    (Field(124), 16),               // hackingBaseTime
    (Field(125), 1),                // userInt1
    (Field(126), 1),                // userInt2
    (Field(127), 1),                // userInt3
    (Field(128), 1),                // userFloat1
    (Field(129), 1),                // userFloat2
    (Field(130), 1),                // userFloat3
    (Field(131), 1),                // userVec1[0]
    (Field(132), 1),                // userVec1[1]
    (Field(133), 1),                // userVec1[2]
    (Field(134), 1),                // userVec2[0]
    (Field(135), 1),                // userVec2[1]
    (Field(136), 1),                // userVec2[2]
];
const VEHICLE_FIELD_SPECS: &[(Slot, i8)] = &[
    (Field(0), 32),                          // commandTime
    (Field(1), 0),                           // origin[1]
    (Field(2), 0),                           // origin[0]
    (Field(3), 0),                           // viewangles[1]
    (Field(4), 0),                           // viewangles[0]
    (Field(5), 0),                           // origin[2]
    (Field(6), 0),                           // velocity[0]
    (Field(7), 0),                           // velocity[1]
    (Field(8), 0),                           // velocity[2]
    (Field(10), -16),                        // weaponTime
    (Field(11), 16),                         // delta_angles[1]
    (Field(12), 0),                          // speed
    (Field(13), 16),                         // legsAnim
    (Field(14), 16),                         // delta_angles[0]
    (Field(16), 10),                         // groundEntityNum
    (Field(17), 32),                         // eFlags
    (Field(19), 16),                         // eventSequence
    (Field(21), 16),                         // legsTimer
    (Field(24), 10),                         // rocketLockIndex
    (Field(27), 10),                         // events[0]
    (Field(28), 10),                         // events[1]
    (Field(33), 4),                          // weaponstate
    (Field(38), 16),                         // pm_flags
    (Field(41), -16),                        // pm_time
    (Field(43), 10),                         // clientNum
    (Field(46), 16),                         // gravity
    (Field(47), 8),                          // weapon
    (Field(48), 16),                         // delta_angles[2]
    (Field(50), 0),                          // viewangles[2]
    (Field(56), 10),                         // externalEvent
    (Field(60), 8),                          // eventParms[1]
    (Field(63), 8),                          // pm_type
    (Field(64), 8),                          // externalEventParm
    (Field(65), -16),                        // eventParms[0]
    (VehicleOnly(ORIENTATION), 0),           // vehOrientation[0]
    (VehicleOnly(ORIENTATION + 1), 0),       // vehOrientation[1]
    (VehicleOnly(MOVE_DIR + 1), 0),          // moveDir[1]
    (VehicleOnly(MOVE_DIR), 0),              // moveDir[0]
    (VehicleOnly(ORIENTATION + 2), 0),       // vehOrientation[2]
    (VehicleOnly(MOVE_DIR + 2), 0),          // moveDir[2]
    (Field(71), 32),                         // rocketTargetTime
    (Field(73), 32),                         // electrifyTime
    (Field(75), 16),                         // loopSound
    (Field(79), 32),                         // rocketLockTime
    (Field(84), 10),                         // m_iVehicleNum
    (VehicleOnly(TURNAROUND_TIME), 32),      // vehTurnaroundTime
    (Field(91), 32),                         // hackingTime
    (Field(93), 8),                          // brokenLimbs
    (VehicleOnly(WEAPONS_LINKED), 1),        // vehWeaponsLinked
    (VehicleOnly(HYPERSPACE_TIME), 32),      // hyperSpaceTime
    (Field(103), 10),                        // eFlags2
    (VehicleOnly(HYPERSPACE_ANGLES + 1), 0), // hyperSpaceAngles[1]
    (VehicleOnly(BOARDING), 1),              // vehBoarding
    (VehicleOnly(TURNAROUND_INDEX), 10),     // vehTurnaroundIndex
    (VehicleOnly(SURFACES), 16),             // vehSurfaces
    (VehicleOnly(HYPERSPACE_ANGLES), 0),     // hyperSpaceAngles[0]
    (VehicleOnly(HYPERSPACE_ANGLES + 2), 0), // hyperSpaceAngles[2]
    (Field(125), 1),                         // userInt1
    (Field(126), 1),                         // userInt2
    (Field(127), 1),                         // userInt3
    (Field(128), 1),                         // userFloat1
    (Field(129), 1),                         // userFloat2
    (Field(130), 1),                         // userFloat3
    (Field(131), 1),                         // userVec1[0]
    (Field(132), 1),                         // userVec1[1]
    (Field(133), 1),                         // userVec1[2]
    (Field(134), 1),                         // userVec2[0]
    (Field(135), 1),                         // userVec2[1]
    (Field(136), 1),                         // userVec2[2]
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerState {
    fields: Box<[u32]>,
    pub stats: [u32; MAX_ARRAY_VALUES],
    pub persistent: [u32; MAX_ARRAY_VALUES],
    pub ammo: [u32; MAX_ARRAY_VALUES],
    pub powerups: [u32; MAX_ARRAY_VALUES],
    /// The vehicle-only netfields ([`VehicleNetFields`]), raw.
    vehicle_only: [u32; VEHICLE_ONLY_FIELDS],
}

impl PlayerState {
    /// Become a copy of `other` without allocating.
    pub fn copy_from(&mut self, other: &Self) {
        self.fields.copy_from_slice(&other.fields);
        (self.stats, self.persistent) = (other.stats, other.persistent);
        (self.ammo, self.powerups) = (other.ammo, other.powerups);
        self.vehicle_only = other.vehicle_only;
    }

    /// Reset every field to zero without allocating.
    pub fn clear(&mut self) {
        self.fields.fill(0);
        (self.stats, self.persistent) = ([0; MAX_ARRAY_VALUES], [0; MAX_ARRAY_VALUES]);
        (self.ammo, self.powerups) = ([0; MAX_ARRAY_VALUES], [0; MAX_ARRAY_VALUES]);
        self.vehicle_only = [0; VEHICLE_ONLY_FIELDS];
    }

    pub fn zero() -> Self {
        Self {
            fields: vec![0; PLAYER_FIELD_WIDTHS.len()].into_boxed_slice(),
            stats: [0; MAX_ARRAY_VALUES],
            persistent: [0; MAX_ARRAY_VALUES],
            ammo: [0; MAX_ARRAY_VALUES],
            powerups: [0; MAX_ARRAY_VALUES],
            vehicle_only: [0; VEHICLE_ONLY_FIELDS],
        }
    }

    pub fn command_time(&self) -> i32 {
        self.fields[0] as i32
    }

    /// Server side: the time of the last movement command this state reflects.
    pub fn set_command_time(&mut self, time: i32) {
        self.fields[0] = time as u32;
    }

    /// Server side: `pm_type`.
    pub fn set_movement_type(&mut self, movement_type: u8) {
        self.fields[63] = u32::from(movement_type);
    }

    /// Server side: world position.
    pub fn set_origin(&mut self, origin: [f32; 3]) {
        (self.fields[2], self.fields[1], self.fields[5]) = (
            origin[0].to_bits(),
            origin[1].to_bits(),
            origin[2].to_bits(),
        );
    }

    /// Server side: the wire client number this state belongs to.
    pub fn set_client_num(&mut self, client: u16) {
        self.fields[43] = u32::from(client);
    }

    /// Server side: velocity in units per second.
    pub fn set_velocity(&mut self, velocity: [f32; 3]) {
        (self.fields[6], self.fields[7], self.fields[8]) = (
            velocity[0].to_bits(),
            velocity[1].to_bits(),
            velocity[2].to_bits(),
        );
    }

    /// Server side: pitch, yaw, roll in degrees.
    pub fn set_view_angles(&mut self, angles: [f32; 3]) {
        (self.fields[4], self.fields[3], self.fields[50]) = (
            angles[0].to_bits(),
            angles[1].to_bits(),
            angles[2].to_bits(),
        );
    }

    /// Server side: what is added to a command's angles to give the view angles.
    pub fn set_delta_angles(&mut self, angles: [i32; 3]) {
        (self.fields[14], self.fields[11], self.fields[48]) =
            (angles[0] as u32, angles[1] as u32, angles[2] as u32);
    }

    /// Server side: eye height above the origin.
    pub fn set_view_height(&mut self, height: i32) {
        self.fields[22] = height as u32;
    }

    /// Server side: `pm_flags`.
    pub fn set_movement_flags(&mut self, flags: u16) {
        self.fields[38] = u32::from(flags);
    }

    /// Server side: `pm_time`.
    pub fn set_movement_time(&mut self, time: i16) {
        self.fields[41] = time as u32;
    }

    /// Server side: maximum speed.
    pub fn set_speed(&mut self, speed: f32) {
        self.fields[12] = speed.to_bits();
    }

    /// Server side: `basespeed`, which `BG_AdjustClientSpeed` resets `speed` from on
    /// every move; a client predicts with it, not with `speed`.
    pub fn set_base_speed(&mut self, speed: i32) {
        self.fields[37] = speed as u32;
    }

    /// Server side: gravity.
    pub fn set_gravity(&mut self, gravity: i32) {
        self.fields[46] = gravity as u32;
    }

    /// Server side: what the player stands on; 1,023 for nothing.
    pub fn set_ground_entity_num(&mut self, entity: u16) {
        self.fields[16] = u32::from(entity);
    }

    pub fn movement_type(&self) -> u8 {
        self.fields[63] as u8
    }

    pub fn movement_flags(&self) -> u16 {
        self.fields[38] as u16
    }

    /// Raw `playerState_t::eFlags` used by legacy presentation transitions.
    pub fn entity_flags(&self) -> u32 {
        self.fields[17]
    }

    pub fn movement_time(&self) -> i16 {
        self.fields[41] as i16
    }

    pub fn team(&self) -> u8 {
        self.persistent[3] as u8
    }

    pub fn is_spectator(&self) -> bool {
        self.movement_type() == 4 || self.team() == 3
    }

    /// Whether one `playerState_t::powerups` deadline is active at `time`.
    pub fn powerup_active(&self, index: usize, time: i32) -> bool {
        self.powerups
            .get(index)
            .is_some_and(|deadline| *deadline as i32 > time)
    }

    pub fn origin(&self) -> [f32; 3] {
        [
            f32::from_bits(self.fields[2]),
            f32::from_bits(self.fields[1]),
            f32::from_bits(self.fields[5]),
        ]
    }

    pub fn velocity(&self) -> [f32; 3] {
        [
            f32::from_bits(self.fields[6]),
            f32::from_bits(self.fields[7]),
            f32::from_bits(self.fields[8]),
        ]
    }

    pub fn speed(&self) -> f32 {
        f32::from_bits(self.fields[12])
    }

    pub fn ground_entity_num(&self) -> u16 {
        self.fields[16] as u16
    }

    pub fn movement_direction(&self) -> i8 {
        self.fields[30] as i8
    }

    pub fn gravity(&self) -> i32 {
        self.fields[46] as i32
    }

    pub fn standing_height(&self) -> u16 {
        self.fields[35] as u16
    }

    pub fn crouching_height(&self) -> u16 {
        self.fields[36] as u16
    }

    pub fn view_angles(&self) -> [f32; 3] {
        [
            f32::from_bits(self.fields[4]),
            f32::from_bits(self.fields[3]),
            f32::from_bits(self.fields[50]),
        ]
    }

    pub fn client_num(&self) -> u16 {
        self.fields[43] as u16
    }

    pub fn view_height(&self) -> i32 {
        self.fields[22] as i32
    }

    pub fn delta_angles(&self) -> [i32; 3] {
        [
            self.fields[14] as i32,
            self.fields[11] as i32,
            self.fields[48] as i32,
        ]
    }

    pub fn leg_animation(&self) -> u16 {
        self.fields[13] as u16
    }

    pub fn torso_animation(&self) -> u16 {
        self.fields[15] as u16
    }

    /// Protocol-26 `playerState_t::saberMove` (player-state field 34).
    pub fn saber_move(&self) -> u32 {
        self.fields[34]
    }

    /// Remaining upper-body animation time from protocol-26 netfield 20.
    pub fn torso_timer(&self) -> i32 {
        self.fields[20] as i32
    }

    /// Remaining lower-body animation time from protocol-26 netfield 21.
    pub fn legs_timer(&self) -> i32 {
        self.fields[21] as i32
    }

    /// Saber attack-chain count from protocol-26 netfield 62.
    pub fn saber_attack_chain_count(&self) -> u8 {
        self.fields[62] as u8
    }

    /// Saber block response state from protocol-26 netfield 77.
    pub fn saber_blocked(&self) -> u8 {
        self.fields[77] as u8
    }

    pub fn torso_flip(&self) -> bool {
        self.fields[55] != 0
    }

    pub fn leg_flip(&self) -> bool {
        self.fields[69] != 0
    }

    pub fn force_powers_active(&self) -> u32 {
        self.fields[82]
    }

    pub fn levitation_level(&self) -> u8 {
        self.fields[52] as u8
    }

    pub fn force_jump_start_height(&self) -> f32 {
        f32::from_bits(self.fields[74])
    }

    pub fn broken_limbs(&self) -> u8 {
        self.fields[93] as u8
    }

    pub fn vehicle_entity_num(&self) -> u16 {
        self.fields[84] as u16
    }

    pub fn weapon(&self) -> u8 {
        self.fields[47] as u8
    }

    /// Remaining `playerState_t::weaponTime` from protocol-26 netfield 10.
    pub fn weapon_time(&self) -> i32 {
        self.fields[10] as i32
    }

    /// Raw `weaponstate_t` from protocol-26 player netfield 33.
    pub fn weapon_state(&self) -> u8 {
        self.fields[33] as u8
    }

    /// `STAT_WEAPONS`, the owned-weapon bitset at stats index 4.
    pub fn owned_weapons(&self) -> u32 {
        self.stats[STAT_WEAPONS]
    }

    /// One signed ammo pool value; `-1` is codemp's infinite-ammo sentinel.
    pub fn ammo_value(&self, index: usize) -> Option<i32> {
        self.ammo.get(index).copied().map(|value| value as i32)
    }

    /// `fd.forceRageRecoveryTime` from protocol-26 player netfield 96.
    pub fn force_rage_recovery_time(&self) -> i32 {
        self.fields[96] as i32
    }

    /// `forceHandExtend` action gate from protocol-26 player netfield 80.
    pub fn force_hand_extend(&self) -> u8 {
        self.fields[80] as u8
    }

    /// Whether the server selected an airborne animation (netfield 59).
    pub fn in_air_animation(&self) -> bool {
        self.fields[59] != 0
    }

    /// Whether the server has forced Jedi Master weapon rules.
    pub fn is_jedi_master(&self) -> bool {
        self.fields[114] != 0
    }

    /// Whether the server has forced true-Jedi weapon rules.
    pub fn is_true_jedi(&self) -> bool {
        self.fields[116] != 0
    }

    /// Whether the player is in a private duel, which forces the saber.
    pub fn duel_in_progress(&self) -> bool {
        self.fields[119] != 0
    }

    /// Server time until which a fresh private duel freezes movement input
    /// (netfield 118, `msg.cpp` `playerStateFields`; `bg_pmove.c:6996-7000`).
    pub fn duel_time(&self) -> i32 {
        self.fields[118] as i32
    }

    /// Client entity selected by the private-duel state (netfield 44).
    pub fn duel_index(&self) -> u16 {
        self.fields[44] as u16
    }

    /// Local Force Seeing level (netfield 109).
    pub fn force_see_level(&self) -> u8 {
        self.fields[109] as u8
    }

    /// Local electrocution deadline (netfield 73).
    pub fn electrify_time(&self) -> i32 {
        self.fields[73] as i32
    }

    pub fn selected_force_power(&self) -> u8 {
        self.fields[54] as u8
    }

    /// `playerState_t::bobCycle` (protocol-26 netfield 9, 8 bits): bit 7 is
    /// the stride parity, bits 0-6 the phase.
    pub fn bob_cycle(&self) -> u8 {
        self.fields[9] as u8
    }

    /// `playerState_t::zoomMode` (protocol-26 netfield 90, 2 bits); non-zero
    /// while scoped.
    pub fn zoom_mode(&self) -> u8 {
        self.fields[90] as u8
    }

    /// `playerState_t::zoomLocked` (netfield 94): the disruptor zoom level is held.
    pub fn zoom_locked(&self) -> bool {
        self.fields[94] != 0
    }

    /// `playerState_t::zoomTime` (netfield 92): command time of the last unzoom.
    pub fn zoom_time(&self) -> i32 {
        self.fields[92] as i32
    }

    /// `playerState_t::zoomFov` (netfield 95, float): server-approximated locked zoom FOV.
    pub fn zoom_fov(&self) -> f32 {
        f32::from_bits(self.fields[95])
    }

    /// `playerState_t::weaponChargeTime` (netfield 68): start of the current charge.
    pub fn weapon_charge_time(&self) -> i32 {
        self.fields[68] as i32
    }

    /// Raw `playerState_t::fd.forceSide` from protocol-26 netfield 61.
    /// This is a read-only presentation accessor; the codec table is unchanged.
    pub fn force_side(&self) -> u8 {
        self.fields[61] as u8
    }

    pub fn event_sequence(&self) -> i32 {
        self.fields[19] as i32
    }

    pub fn event(&self, slot: usize) -> Option<u16> {
        match slot {
            0 => Some(self.fields[27] as u16),
            1 => Some(self.fields[28] as u16),
            _ => None,
        }
    }

    pub fn event_parameter(&self, slot: usize) -> Option<u16> {
        match slot {
            0 => Some(self.fields[65] as u16),
            1 => Some(self.fields[60] as u16),
            _ => None,
        }
    }

    pub fn external_event(&self) -> u16 {
        self.fields[56] as u16
    }

    pub fn external_event_parameter(&self) -> u8 {
        self.fields[64] as u8
    }

    pub fn force_power(&self) -> u8 {
        self.fields[18] as u8
    }

    pub fn saber_holstered(&self) -> u8 {
        self.fields[81] as u8
    }

    /// Protocol-26 `playerState_t::saberEntityNum` (netfield 31).
    pub fn saber_entity_num(&self) -> u16 {
        self.fields[31] as u16
    }

    /// Protocol-26 `playerState_t::saberInFlight` (netfield 88).
    pub fn saber_in_flight(&self) -> bool {
        self.fields[88] != 0
    }

    /// Protocol-26 `playerState_t::saberLockTime` (netfield 107).
    pub fn saber_lock_time(&self) -> i32 {
        self.fields[107] as i32
    }

    /// Protocol-26 `playerState_t::saberLockFrame` (netfield 108, 16 bits;
    /// 119 is `duelInProgress`).
    pub fn saber_lock_frame(&self) -> u16 {
        self.fields[108] as u16
    }

    /// Protocol-26 `playerState_t::forceRestricted` (netfield 128).
    pub fn force_restricted(&self) -> bool {
        self.fields[128] != 0
    }

    /// Protocol-26 `playerState_t::trueNonJedi` (netfield 130).
    pub fn true_non_jedi(&self) -> bool {
        self.fields[130] != 0
    }

    /// Protocol-26 `playerState_t::fallingToDeath` (netfield 97 — `msg.cpp`'s
    /// `playerStateFields`, where 104 is `fd.forceMindtrickTargetIndex4`).
    pub fn falling_to_death(&self) -> i32 {
        self.fields[97] as i32
    }

    /// Protocol-26 `playerState_t::hasDetPackPlanted` (netfield 87).
    pub fn has_detpack_planted(&self) -> bool {
        self.fields[87] != 0
    }

    /// Protocol-26 `playerState_t::emplacedIndex` (netfield 112).
    pub fn emplaced_index(&self) -> u16 {
        self.fields[112] as u16
    }

    /// Protocol-26 `playerState_t::customRGBA[0..3]` in RGBA order
    /// (netfields 29, 42, 45, 32).
    pub fn custom_rgba(&self) -> [u8; 4] {
        [29, 42, 45, 32].map(|field| self.fields[field] as u8)
    }

    /// Protocol-26 `playerState_t::fd.saberAnimLevel` (netfield 23): the style
    /// the swings are chosen from.
    pub fn saber_style(&self) -> u8 {
        self.fields[23] as u8
    }

    /// Protocol-26 `playerState_t::fd.saberDrawAnimLevel` (netfield 25): the
    /// style the HUD shows (`cg_draw.c:810`, `:1172`). The game sets it from
    /// the stance queue at once (`w_saber.c:8118-8127`), while `saberAnimLevel`
    /// changes only when the swing ends (`w_saber.c:8130-8138`).
    pub fn saber_draw_style(&self) -> u8 {
        self.fields[25] as u8
    }

    /// Stats travel as 16-bit signed shorts (`MSG_ReadDeltaPlayerstate`),
    /// so a dead player's negative health must be sign-extended.
    pub fn health(&self) -> i32 {
        i32::from(self.stats[0] as u16 as i16)
    }

    pub fn armor(&self) -> i32 {
        i32::from(self.stats[5] as u16 as i16)
    }

    pub fn max_health(&self) -> i32 {
        i32::from(self.stats[8] as u16 as i16)
    }

    /// Direction byte written by `P_DamageFeedback` and consumed by codemp's
    /// `CG_DamageFeedback` as the world-space yaw of the incoming damage.
    pub fn damage_yaw(&self) -> u8 {
        self.fields[57] as u8
    }

    /// Authoritative damage magnitude accumulated for this feedback event.
    pub fn damage_count(&self) -> u8 {
        self.fields[58] as u8
    }

    /// Sequence byte changed by the server whenever damage feedback is new.
    pub fn damage_event(&self) -> u8 {
        self.fields[70] as u8
    }

    /// Direction byte written by `P_DamageFeedback` and consumed by codemp's
    /// `CG_DamageFeedback` as the world-space pitch of the incoming damage.
    pub fn damage_pitch(&self) -> u8 {
        self.fields[83] as u8
    }

    pub fn raw_field(&self, index: usize) -> Option<u32> {
        self.fields.get(index).copied()
    }

    /// Write one wire field by its protocol-26 index: the raw 32 bits, a float's bit
    /// pattern or an integer. A server's game owns most fields and writes them this
    /// way; returns `false` for an index the protocol does not have.
    pub fn set_raw_field(&mut self, index: usize, value: u32) -> bool {
        self.fields
            .get_mut(index)
            .map(|field| *field = value)
            .is_some()
    }
}

impl Default for PlayerState {
    fn default() -> Self {
        Self::zero()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReliableServerCommand {
    pub sequence: i32,
    pub command: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub message_sequence: i32,
    pub reliable_acknowledge: i32,
    pub server_commands: Vec<ReliableServerCommand>,
    pub server_time: i32,
    pub delta_from: Option<i32>,
    pub flags: u8,
    pub area_mask: Vec<u8>,
    pub player: PlayerState,
    pub vehicle_player: Option<PlayerState>,
    pub entities: Vec<EntityState>,
    pub consumed_bits: usize,
}

/// Decodes an uncompressed (`deltaNum == 0`) live snapshot. Delta snapshots
/// are deliberately rejected until their referenced frame is supplied.
pub fn decode_base_snapshot(
    payload: &[u8],
    message_sequence: i32,
    gamestate: &GameState,
) -> Result<Snapshot, SnapshotError> {
    decode_snapshot(payload, message_sequence, gamestate, None)
}

pub fn decode_snapshot(
    payload: &[u8],
    message_sequence: i32,
    gamestate: &GameState,
    delta_base: Option<&Snapshot>,
) -> Result<Snapshot, SnapshotError> {
    let mut reader = MessageReader::new(payload);
    let reliable_acknowledge = reader.read_i32()?;
    let mut server_commands = Vec::new();
    loop {
        match reader.read_service_command()? {
            ServiceCommand::Nop => {}
            ServiceCommand::ServerCommand => {
                server_commands.push(ReliableServerCommand {
                    sequence: reader.read_i32()?,
                    command: reader.read_c_string(16_383)?,
                });
            }
            ServiceCommand::Snapshot => break,
            command => return Err(SnapshotError::UnexpectedCommand(command)),
        }
    }

    let server_time = reader.read_i32()?;
    let delta_distance = reader.read_u8()?;
    let delta_from = (delta_distance != 0).then(|| message_sequence - i32::from(delta_distance));
    let previous = if let Some(delta_from) = delta_from {
        Some(
            delta_base
                .filter(|snapshot| snapshot.message_sequence == delta_from)
                .ok_or(SnapshotError::DeltaBaseRequired {
                    message_sequence,
                    delta_from,
                })?,
        )
    } else {
        None
    };
    let flags = reader.read_u8()?;
    let area_length = usize::from(reader.read_u8()?);
    if area_length > MAX_AREA_MASK {
        return Err(SnapshotError::AreaMaskTooLarge(area_length));
    }
    let mut area_mask = Vec::with_capacity(area_length);
    for _ in 0..area_length {
        area_mask.push(reader.read_u8()?);
    }
    let zero_player = PlayerState::zero();
    let player = read_player_state(
        &mut reader,
        previous.map_or(&zero_player, |snapshot| &snapshot.player),
    )?;
    let vehicle_player = if player.vehicle_entity_num() != 0 {
        let zero_vehicle = PlayerState::zero();
        Some(read_vehicle_player_state(
            &mut reader,
            previous
                .and_then(|snapshot| snapshot.vehicle_player.as_ref())
                .unwrap_or(&zero_vehicle),
        )?)
    } else {
        None
    };

    let mut entities = Vec::new();
    let old_entities = previous.map_or(&[][..], |snapshot| snapshot.entities.as_slice());
    let mut old_index = 0;
    loop {
        let number = reader.read_bits(LEGACY_ENTITY_NUMBER_BITS)? as u16;
        if number == ENTITY_NUMBER_NONE {
            break;
        }
        while old_entities
            .get(old_index)
            .is_some_and(|entity| entity.number() < number)
        {
            entities.push(old_entities[old_index].clone());
            old_index += 1;
        }
        let from = if old_entities
            .get(old_index)
            .is_some_and(|entity| entity.number() == number)
        {
            let old = &old_entities[old_index];
            old_index += 1;
            old.clone()
        } else {
            gamestate
                .baseline(usize::from(number))
                .cloned()
                .unwrap_or_else(|| EntityState::zero(number, &LEGACY_ENTITY_FIELDS))
        };
        let entity = read_delta_entity(&mut reader, &from, number, &LEGACY_ENTITY_FIELDS)?;
        if entity.number() != ENTITY_NUMBER_NONE {
            entities.push(entity);
        }
    }
    entities.extend_from_slice(&old_entities[old_index..]);

    Ok(Snapshot {
        message_sequence,
        reliable_acknowledge,
        server_commands,
        server_time,
        delta_from,
        flags,
        area_mask,
        player,
        vehicle_player,
        entities,
        consumed_bits: reader.bit_position(),
    })
}

fn read_player_state(
    reader: &mut MessageReader<'_>,
    previous: &PlayerState,
) -> Result<PlayerState, SnapshotError> {
    let is_pilot = reader.read_bits(1)? != 0;
    read_player_state_fields(reader, previous, is_pilot.then_some(PILOT_FIELD_SPECS))
}

fn read_vehicle_player_state(
    reader: &mut MessageReader<'_>,
    previous: &PlayerState,
) -> Result<PlayerState, SnapshotError> {
    read_player_state_fields(reader, previous, Some(VEHICLE_FIELD_SPECS))
}

fn read_player_state_fields(
    reader: &mut MessageReader<'_>,
    previous: &PlayerState,
    mapped_fields: Option<&[(Slot, i8)]>,
) -> Result<PlayerState, SnapshotError> {
    let changed_count = usize::from(reader.read_u8()?);
    let field_count = mapped_fields.map_or(PLAYER_FIELD_WIDTHS.len(), <[_]>::len);
    if changed_count > field_count {
        return Err(SnapshotError::InvalidPlayerFieldCount(changed_count));
    }
    let mut state = previous.clone();
    for field_index in 0..changed_count {
        if reader.read_bits(1)? == 0 {
            continue;
        }
        let (slot, width) = mapped_fields.map_or_else(
            || (Field(field_index), PLAYER_FIELD_WIDTHS[field_index]),
            |fields| fields[field_index],
        );
        let value = if width == 0 {
            if reader.read_bits(1)? == 0 {
                let integer = reader.read_bits(FLOAT_INT_BITS)? as i32 - FLOAT_INT_BIAS;
                (integer as f32).to_bits()
            } else {
                reader.read_bits(32)?
            }
        } else if width < 0 {
            reader.read_signed_bits((-width) as u8)? as u32
        } else {
            reader.read_bits(width as u8)?
        };
        *state.slot_mut(slot) = value;
    }

    if reader.read_bits(1)? != 0 {
        read_array_delta(reader, &mut state.stats, Some(STAT_WEAPONS))?;
        read_array_delta(reader, &mut state.persistent, None)?;
        read_array_delta(reader, &mut state.ammo, None)?;
        read_array_delta_32(reader, &mut state.powerups)?;
    }
    Ok(state)
}

/// `stats`, `persistant` and `ammo`: codemp `MSG_ReadDeltaPlayerstate` reads each
/// entry with `MSG_ReadShort`, a signed 16-bit value sign-extended into the int
/// field, so a score, health or ammo of -1 stays -1 rather than 65535. Only
/// `STAT_WEAPONS` is an unsigned `MAX_WEAPONS`-bit field.
fn read_array_delta(
    reader: &mut MessageReader<'_>,
    values: &mut [u32; MAX_ARRAY_VALUES],
    wide_index: Option<usize>,
) -> Result<(), MessageError> {
    if reader.read_bits(1)? == 0 {
        return Ok(());
    }
    let mask = reader.read_bits(16)?;
    for (index, value) in values.iter_mut().enumerate() {
        if mask & (1 << index) != 0 {
            *value = if wide_index == Some(index) {
                reader.read_bits(MAX_WEAPONS)?
            } else {
                i32::from(reader.read_bits(16)? as u16 as i16) as u32
            };
        }
    }
    Ok(())
}

fn read_array_delta_32(
    reader: &mut MessageReader<'_>,
    values: &mut [u32; MAX_ARRAY_VALUES],
) -> Result<(), MessageError> {
    if reader.read_bits(1)? == 0 {
        return Ok(());
    }
    let mask = reader.read_bits(16)?;
    for (index, value) in values.iter_mut().enumerate() {
        if mask & (1 << index) != 0 {
            *value = reader.read_bits(32)?;
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapshotError {
    Message(MessageError),
    Entity(EntityDeltaError),
    UnexpectedCommand(ServiceCommand),
    DeltaBaseRequired {
        message_sequence: i32,
        delta_from: i32,
    },
    AreaMaskTooLarge(usize),
    InvalidPlayerFieldCount(usize),
    PilotPlayerStateNotYetSupported,
    VehiclePlayerStateNotYetSupported,
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(error) => error.fmt(formatter),
            Self::Entity(error) => error.fmt(formatter),
            Self::UnexpectedCommand(command) => {
                write!(formatter, "expected snapshot, received {command:?}")
            }
            Self::DeltaBaseRequired { delta_from, .. } => {
                write!(formatter, "snapshot requires delta base {delta_from}")
            }
            Self::AreaMaskTooLarge(length) => write!(formatter, "area mask is {length} bytes"),
            Self::InvalidPlayerFieldCount(count) => {
                write!(formatter, "invalid player-state field count {count}")
            }
            Self::PilotPlayerStateNotYetSupported => {
                formatter.write_str("optimized pilot player-state schema is not yet supported")
            }
            Self::VehiclePlayerStateNotYetSupported => {
                formatter.write_str("vehicle player-state payload is not yet supported")
            }
        }
    }
}

impl Error for SnapshotError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Message(error) => Some(error),
            Self::Entity(error) => Some(error),
            _ => None,
        }
    }
}

impl From<MessageError> for SnapshotError {
    fn from(value: MessageError) -> Self {
        Self::Message(value)
    }
}

impl From<EntityDeltaError> for SnapshotError {
    fn from(value: EntityDeltaError) -> Self {
        Self::Entity(value)
    }
}

#[cfg(test)]
mod array_tests {
    use super::*;

    #[test]
    fn short_arrays_are_signed_like_msg_read_short() {
        let mut state = PlayerState::zero();
        // PERS_SCORE -1 after a suicide; a C server holds it as int -1, which the
        // wire carries as the same 16 bits as 0xffff.
        state.persistent[0] = 0xffff;
        state.persistent[1] = (-3_i32) as u32;
        state.stats[0] = (-40_i32) as u32;
        state.ammo[3] = (-1_i32) as u32;
        state.stats[STAT_WEAPONS] = 1 << 18;
        let received = write::player_state_as_received(&state).unwrap();
        assert_eq!(received.persistent[0] as i32, -1);
        assert_eq!(received.persistent[1] as i32, -3);
        assert_eq!(received.stats[0] as i32, -40);
        assert_eq!(received.health(), -40);
        assert_eq!(received.ammo_value(3), Some(-1));
        // The weapon bitset is unsigned: its top bit does not spread.
        assert_eq!(received.stats[STAT_WEAPONS], 1 << 18);
    }

    #[test]
    fn positive_shorts_are_unchanged() {
        let mut state = PlayerState::zero();
        state.persistent[0] = 32_767;
        state.ammo[2] = 300;
        let received = write::player_state_as_received(&state).unwrap();
        assert_eq!(received.persistent[0], 32_767);
        assert_eq!(received.ammo[2], 300);
    }
}
