use crate::{MessageError, MessageReader};
use std::error::Error;
use std::fmt;

pub const LEGACY_ENTITY_NUMBER_BITS: u8 = 10;
pub const MAX_LEGACY_ENTITIES: usize = 1 << LEGACY_ENTITY_NUMBER_BITS;
pub const ENTITY_NUMBER_NONE: u16 = (MAX_LEGACY_ENTITIES - 1) as u16;
const FLOAT_INT_BITS: u8 = 13;
const FLOAT_INT_BIAS: i32 = 1 << (FLOAT_INT_BITS - 1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityFieldEncoding {
    Float,
    Integer { width: u8, signed: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EntityField {
    pub name: &'static str,
    pub encoding: EntityFieldEncoding,
}

impl EntityField {
    pub const fn float(name: &'static str) -> Self {
        Self {
            name,
            encoding: EntityFieldEncoding::Float,
        }
    }

    pub const fn integer(name: &'static str, width: u8, signed: bool) -> Self {
        Self {
            name,
            encoding: EntityFieldEncoding::Integer { width, signed },
        }
    }
}

pub const LEGACY_ENTITY_FIELDS: [EntityField; 132] = [
    EntityField::integer("pos.trTime", 32, false),
    EntityField::float("pos.trBase[1]"),
    EntityField::float("pos.trBase[0]"),
    EntityField::float("apos.trBase[1]"),
    EntityField::float("pos.trBase[2]"),
    EntityField::float("apos.trBase[0]"),
    EntityField::float("pos.trDelta[0]"),
    EntityField::float("pos.trDelta[1]"),
    EntityField::integer("eType", 8, false),
    EntityField::float("angles[1]"),
    EntityField::float("pos.trDelta[2]"),
    EntityField::float("origin[0]"),
    EntityField::float("origin[1]"),
    EntityField::float("origin[2]"),
    EntityField::integer("weapon", 8, false),
    EntityField::integer("apos.trType", 8, false),
    EntityField::integer("legsAnim", 16, false),
    EntityField::integer("torsoAnim", 16, false),
    EntityField::integer("genericenemyindex", 32, false),
    EntityField::integer("eFlags", 32, false),
    EntityField::integer("pos.trDuration", 32, false),
    EntityField::integer("teamowner", 8, false),
    EntityField::integer("groundEntityNum", 10, false),
    EntityField::integer("pos.trType", 8, false),
    EntityField::float("angles[2]"),
    EntityField::float("angles[0]"),
    EntityField::integer("solid", 24, false),
    EntityField::integer("fireflag", 2, false),
    EntityField::integer("event", 10, false),
    EntityField::integer("customRGBA[3]", 8, false),
    EntityField::integer("customRGBA[0]", 8, false),
    EntityField::float("speed"),
    EntityField::integer("clientNum", 10, false),
    EntityField::float("apos.trBase[2]"),
    EntityField::integer("apos.trTime", 32, false),
    EntityField::integer("customRGBA[1]", 8, false),
    EntityField::integer("customRGBA[2]", 8, false),
    EntityField::integer("saberEntityNum", 10, false),
    EntityField::integer("g2radius", 8, false),
    EntityField::integer("otherEntityNum2", 10, false),
    EntityField::integer("owner", 10, false),
    EntityField::integer("modelindex2", 8, false),
    EntityField::integer("eventParm", 8, false),
    EntityField::integer("saberMove", 8, false),
    EntityField::float("apos.trDelta[1]"),
    EntityField::float("boneAngles1[1]"),
    EntityField::integer("modelindex", 16, true),
    EntityField::integer("emplacedOwner", 32, false),
    EntityField::float("apos.trDelta[0]"),
    EntityField::float("apos.trDelta[2]"),
    EntityField::integer("torsoFlip", 1, false),
    EntityField::float("angles2[1]"),
    EntityField::integer("lookTarget", 10, false),
    EntityField::float("origin2[2]"),
    EntityField::integer("modelGhoul2", 8, false),
    EntityField::integer("loopSound", 8, false),
    EntityField::float("origin2[0]"),
    EntityField::integer("shouldtarget", 1, false),
    EntityField::integer("trickedentindex", 16, false),
    EntityField::integer("otherEntityNum", 10, false),
    EntityField::float("origin2[1]"),
    EntityField::integer("time2", 32, false),
    EntityField::integer("legsFlip", 1, false),
    EntityField::integer("bolt2", 10, false),
    EntityField::integer("constantLight", 32, false),
    EntityField::integer("time", 32, false),
    EntityField::integer("hasLookTarget", 1, false),
    EntityField::float("boneAngles1[2]"),
    EntityField::integer("activeForcePass", 6, false),
    EntityField::integer("health", 10, false),
    EntityField::integer("loopIsSoundset", 1, false),
    EntityField::integer("saberHolstered", 2, false),
    EntityField::integer("npcSaber1", 9, false),
    EntityField::integer("maxhealth", 10, false),
    EntityField::integer("trickedentindex2", 16, false),
    EntityField::integer("forcePowersActive", 32, false),
    EntityField::integer("iModelScale", 10, false),
    EntityField::integer("powerups", 16, false),
    EntityField::integer("soundSetIndex", 8, false),
    EntityField::integer("brokenLimbs", 8, false),
    EntityField::integer("csSounds_Std", 8, false),
    EntityField::integer("saberInFlight", 1, false),
    EntityField::float("angles2[0]"),
    EntityField::integer("frame", 16, false),
    EntityField::float("angles2[2]"),
    EntityField::integer("forceFrame", 16, false),
    EntityField::integer("generic1", 8, false),
    EntityField::integer("boneIndex1", 6, false),
    EntityField::integer("NPC_class", 8, false),
    EntityField::integer("apos.trDuration", 32, false),
    EntityField::integer("boneOrient", 9, false),
    EntityField::integer("bolt1", 8, false),
    EntityField::integer("trickedentindex3", 16, false),
    EntityField::integer("m_iVehicleNum", 10, false),
    EntityField::integer("trickedentindex4", 16, false),
    EntityField::integer("surfacesOff", 32, false),
    EntityField::integer("eFlags2", 10, false),
    EntityField::integer("isJediMaster", 1, false),
    EntityField::integer("isPortalEnt", 1, false),
    EntityField::integer("heldByClient", 6, false),
    EntityField::integer("ragAttach", 10, false),
    EntityField::integer("boltToPlayer", 6, false),
    EntityField::integer("npcSaber2", 9, false),
    EntityField::integer("csSounds_Combat", 8, false),
    EntityField::integer("csSounds_Extra", 8, false),
    EntityField::integer("csSounds_Jedi", 8, false),
    EntityField::integer("surfacesOn", 32, false),
    EntityField::integer("boneIndex2", 6, false),
    EntityField::integer("boneIndex3", 6, false),
    EntityField::integer("boneIndex4", 6, false),
    EntityField::float("boneAngles1[0]"),
    EntityField::float("boneAngles2[0]"),
    EntityField::float("boneAngles2[1]"),
    EntityField::float("boneAngles2[2]"),
    EntityField::float("boneAngles3[0]"),
    EntityField::float("boneAngles3[1]"),
    EntityField::float("boneAngles3[2]"),
    EntityField::float("boneAngles4[0]"),
    EntityField::float("boneAngles4[1]"),
    EntityField::float("boneAngles4[2]"),
    EntityField::integer("userInt1", 1, false),
    EntityField::integer("userInt2", 1, false),
    EntityField::integer("userInt3", 1, false),
    EntityField::integer("userFloat1", 1, false),
    EntityField::integer("userFloat2", 1, false),
    EntityField::integer("userFloat3", 1, false),
    EntityField::integer("userVec1[0]", 1, false),
    EntityField::integer("userVec1[1]", 1, false),
    EntityField::integer("userVec1[2]", 1, false),
    EntityField::integer("userVec2[0]", 1, false),
    EntityField::integer("userVec2[1]", 1, false),
    EntityField::integer("userVec2[2]", 1, false),
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityState {
    number: u16,
    fields: Box<[u32]>,
}

impl EntityState {
    pub fn zero(number: u16, schema: &[EntityField]) -> Self {
        Self {
            number,
            fields: vec![0; schema.len()].into_boxed_slice(),
        }
    }

    /// Construct a state from schema-ordered raw 32-bit field values.
    ///
    /// This is primarily useful to protocol encoders and synthetic fixtures;
    /// float fields carry their IEEE-754 bit representation.
    pub fn from_raw_fields(
        number: u16,
        fields: impl Into<Box<[u32]>>,
    ) -> Result<Self, EntityDeltaError> {
        if usize::from(number) >= MAX_LEGACY_ENTITIES {
            return Err(EntityDeltaError::InvalidEntityNumber(number));
        }
        Ok(Self {
            number,
            fields: fields.into(),
        })
    }

    pub fn number(&self) -> u16 {
        self.number
    }

    /// Become a copy of `other` without allocating when the schemas match, as they
    /// do for every state a server keeps in its snapshot history.
    pub fn copy_from(&mut self, other: &Self) {
        self.number = other.number;
        if self.fields.len() == other.fields.len() {
            self.fields.copy_from_slice(&other.fields);
        } else {
            self.fields = other.fields.clone();
        }
    }

    pub fn raw_field(&self, index: usize) -> Option<u32> {
        self.fields.get(index).copied()
    }

    /// Write one wire field by its schema index: the raw 32 bits, a float's bit pattern
    /// or an integer. A server's game writes its entities this way; returns `false` for
    /// an index the schema does not have.
    pub fn set_raw_field(&mut self, index: usize, value: u32) -> bool {
        self.fields
            .get_mut(index)
            .map(|field| *field = value)
            .is_some()
    }

    /// Renumber the state: a player's entity carries its client's number.
    pub fn set_number(&mut self, number: u16) -> Result<(), EntityDeltaError> {
        if usize::from(number) >= MAX_LEGACY_ENTITIES {
            return Err(EntityDeltaError::InvalidEntityNumber(number));
        }
        self.number = number;
        Ok(())
    }

    pub fn integer_field(&self, index: usize) -> Option<i32> {
        self.raw_field(index).map(|value| value as i32)
    }

    pub fn float_field(&self, index: usize) -> Option<f32> {
        self.raw_field(index).map(f32::from_bits)
    }

    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    pub fn entity_type(&self) -> u8 {
        self.raw_field(8).unwrap_or(0) as u8
    }

    pub fn trajectory_base(&self) -> [f32; 3] {
        [
            f32::from_bits(self.raw_field(2).unwrap_or(0)),
            f32::from_bits(self.raw_field(1).unwrap_or(0)),
            f32::from_bits(self.raw_field(4).unwrap_or(0)),
        ]
    }

    pub fn trajectory_delta(&self) -> [f32; 3] {
        [
            f32::from_bits(self.raw_field(6).unwrap_or(0)),
            f32::from_bits(self.raw_field(7).unwrap_or(0)),
            f32::from_bits(self.raw_field(10).unwrap_or(0)),
        ]
    }

    pub fn trajectory_type(&self) -> u8 {
        self.raw_field(23).unwrap_or(0) as u8
    }

    pub fn trajectory_time(&self) -> i32 {
        self.raw_field(0).unwrap_or(0) as i32
    }

    pub fn trajectory_duration(&self) -> i32 {
        self.raw_field(20).unwrap_or(0) as i32
    }

    pub fn event_origin(&self) -> [f32; 3] {
        [
            f32::from_bits(self.raw_field(11).unwrap_or(0)),
            f32::from_bits(self.raw_field(12).unwrap_or(0)),
            f32::from_bits(self.raw_field(13).unwrap_or(0)),
        ]
    }

    /// `origin2`: the secondary point of trace-weapon events (the muzzle a
    /// disruptor beam starts from).
    pub fn origin2(&self) -> [f32; 3] {
        [
            f32::from_bits(self.raw_field(56).unwrap_or(0)),
            f32::from_bits(self.raw_field(60).unwrap_or(0)),
            f32::from_bits(self.raw_field(53).unwrap_or(0)),
        ]
    }

    /// `shouldtarget`: a one-bit flag events reuse (full charge of a sniper shot).
    pub fn should_target(&self) -> bool {
        self.raw_field(57).unwrap_or(0) != 0
    }

    pub fn angular_trajectory_base(&self) -> [f32; 3] {
        [
            f32::from_bits(self.raw_field(5).unwrap_or(0)),
            f32::from_bits(self.raw_field(3).unwrap_or(0)),
            f32::from_bits(self.raw_field(33).unwrap_or(0)),
        ]
    }

    /// `entityState_t::angles2` (netfields 82, 51, 84).
    pub fn angles2(&self) -> [f32; 3] {
        [
            f32::from_bits(self.raw_field(82).unwrap_or(0)),
            f32::from_bits(self.raw_field(51).unwrap_or(0)),
            f32::from_bits(self.raw_field(84).unwrap_or(0)),
        ]
    }

    pub fn angles(&self) -> [f32; 3] {
        [
            f32::from_bits(self.raw_field(25).unwrap_or(0)),
            f32::from_bits(self.raw_field(9).unwrap_or(0)),
            f32::from_bits(self.raw_field(24).unwrap_or(0)),
        ]
    }

    pub fn angular_trajectory_delta(&self) -> [f32; 3] {
        [
            f32::from_bits(self.raw_field(48).unwrap_or(0)),
            f32::from_bits(self.raw_field(44).unwrap_or(0)),
            f32::from_bits(self.raw_field(49).unwrap_or(0)),
        ]
    }

    pub fn angular_trajectory_type(&self) -> u8 {
        self.raw_field(15).unwrap_or(0) as u8
    }

    pub fn angular_trajectory_time(&self) -> i32 {
        self.raw_field(34).unwrap_or(0) as i32
    }

    pub fn angular_trajectory_duration(&self) -> i32 {
        self.raw_field(91).unwrap_or(0) as i32
    }

    pub fn client_num(&self) -> u16 {
        self.raw_field(32).unwrap_or(0) as u16
    }

    pub fn ground_entity_num(&self) -> u16 {
        self.raw_field(22).unwrap_or(0) as u16
    }

    pub fn movement_direction(&self) -> i8 {
        f32::from_bits(self.raw_field(51).unwrap_or(0)) as i8
    }

    pub fn model_index(&self) -> i16 {
        self.raw_field(46).unwrap_or(0) as i16
    }

    pub fn model_index2(&self) -> u8 {
        self.raw_field(41).unwrap_or(0) as u8
    }

    /// Generic secondary entity reference used by legacy gameplay adapters.
    ///
    /// Protocol 26 places `otherEntityNum2` at netfield 39; see
    /// `codemp/qcommon/msg.cpp`'s `entityStateFields` table. This is a
    /// read-only view and does not alter delta encoding.
    pub fn other_entity_num2(&self) -> u16 {
        self.raw_field(39).unwrap_or(0) as u16
    }

    /// Generic primary entity reference (`otherEntityNum`, netfield 59 in the
    /// same `entityStateFields` table). Read-only view; delta encoding is
    /// untouched.
    pub fn other_entity_num(&self) -> u16 {
        self.raw_field(59).unwrap_or(0) as u16
    }

    /// Generic `entityState_t::speed` value carried by protocol 26.
    pub fn speed(&self) -> f32 {
        f32::from_bits(self.raw_field(31).unwrap_or(0))
    }

    /// `entityState_t::constantLight` (netfield 64); a player's carries when its
    /// weapon charge began.
    pub fn constant_light(&self) -> u32 {
        self.raw_field(64).unwrap_or(0)
    }

    /// `entityState_t::bolt2` (netfield 63); a stuck trip mine sets it to 1 in
    /// proximity mode.
    pub fn bolt2(&self) -> u32 {
        self.raw_field(63).unwrap_or(0)
    }

    /// Generic `entityState_t::time` value carried by protocol 26.
    pub fn time(&self) -> i32 {
        self.raw_field(65).unwrap_or(0) as i32
    }

    /// Whether this state carries the generic portal-entity flag.
    pub fn is_portal_entity(&self) -> bool {
        self.raw_field(98).unwrap_or(0) != 0
    }

    pub fn solid(&self) -> u32 {
        self.raw_field(26).unwrap_or(0)
    }

    pub fn weapon(&self) -> u8 {
        self.raw_field(14).unwrap_or(0) as u8
    }

    pub fn e_flags(&self) -> u32 {
        self.raw_field(19).unwrap_or(0)
    }

    pub fn event(&self) -> u16 {
        self.raw_field(28).unwrap_or(0) as u16
    }

    pub fn event_parameter(&self) -> u8 {
        self.raw_field(42).unwrap_or(0) as u8
    }

    /// Overloaded protocol-26 `saberEntityNum`, used as the sound channel by
    /// `EV_GENERAL_SOUND` (netfield 37).
    pub fn event_sound_channel(&self) -> u16 {
        self.raw_field(37).unwrap_or(0) as u16
    }

    /// Protocol-26 `trickedentindex`, used by tracked sounds and mute events
    /// (netfield 58).
    pub fn tracked_entity_num(&self) -> u16 {
        self.raw_field(58).unwrap_or(0) as u16
    }

    /// Protocol-26 `entityState_t::owner` (netfield 40).
    pub fn owner(&self) -> u16 {
        self.raw_field(40).unwrap_or(0) as u16
    }

    /// Protocol-26 `entityState_t::emplacedOwner` (netfield 47).
    ///
    /// Player entities overload this as the electrocution effect deadline.
    pub fn emplaced_owner(&self) -> i32 {
        self.raw_field(47).unwrap_or(0) as i32
    }

    /// Protocol-26 `trickedentindex2`, the entity muted by `EV_MUTE_SOUND`
    /// (netfield 74).
    pub fn mute_entity_num(&self) -> u16 {
        self.raw_field(74).unwrap_or(0) as u16
    }

    pub fn loop_sound(&self) -> u8 {
        self.raw_field(55).unwrap_or(0) as u8
    }

    /// Protocol-26 `entityState_t::loopIsSoundset` (netfield 70).
    pub fn loop_is_soundset(&self) -> bool {
        self.raw_field(70).unwrap_or(0) != 0
    }

    /// Protocol-26 `entityState_t::soundSetIndex` (netfield 78).
    pub fn sound_set_index(&self) -> u8 {
        self.raw_field(78).unwrap_or(0) as u8
    }

    /// Protocol-26 `entityState_t::saberInFlight` (netfield 81).
    pub fn saber_in_flight(&self) -> bool {
        self.raw_field(81).unwrap_or(0) != 0
    }

    /// Protocol-26 `entityState_t::customRGBA[0..3]` in RGBA order
    /// (netfields 30, 35, 36, 29).
    pub fn custom_rgba(&self) -> [u8; 4] {
        [30, 35, 36, 29].map(|field| self.raw_field(field).unwrap_or(0) as u8)
    }

    pub fn leg_animation(&self) -> u16 {
        self.raw_field(16).unwrap_or(0) as u16
    }

    pub fn torso_animation(&self) -> u16 {
        self.raw_field(17).unwrap_or(0) as u16
    }

    /// Protocol-26 `entityState_t::saberMove` (netfield 43).
    pub fn saber_move(&self) -> u32 {
        self.raw_field(43).unwrap_or(0)
    }

    pub fn torso_flip(&self) -> bool {
        self.raw_field(50).unwrap_or(0) != 0
    }

    pub fn leg_flip(&self) -> bool {
        self.raw_field(62).unwrap_or(0) != 0
    }

    pub fn fire_flag(&self) -> u8 {
        self.raw_field(27).unwrap_or(0) as u8
    }

    /// Legacy general-purpose byte; impact events carry Bryar charge here.
    pub fn generic1(&self) -> u8 {
        self.raw_field(86).unwrap_or(0) as u8
    }

    pub fn force_powers_active(&self) -> u32 {
        self.raw_field(75).unwrap_or(0)
    }

    pub fn broken_limbs(&self) -> u8 {
        self.raw_field(79).unwrap_or(0) as u8
    }

    pub fn force_frame(&self) -> u16 {
        self.raw_field(85).unwrap_or(0) as u16
    }

    /// Protocol-26 `entityState_t::m_iVehicleNum` (netfield 93).
    pub fn vehicle_entity_num(&self) -> u16 {
        self.raw_field(93).unwrap_or(0) as u16
    }

    pub fn npc_class(&self) -> u8 {
        self.raw_field(88).unwrap_or(0) as u8
    }

    /// Protocol-26 `entityState_t::boneIndex1`..`boneIndex4` (netfields 87, 107-109): the
    /// `CS_G2BONES` index of the bone a server turns in `slot` (0-3), zero for none.
    pub fn bone_index(&self, slot: usize) -> u16 {
        [87, 107, 108, 109]
            .get(slot)
            .and_then(|&field| self.raw_field(field))
            .unwrap_or(0) as u16
    }

    /// Protocol-26 `entityState_t::boneAngles1`..`boneAngles4`: the angles (pitch, yaw,
    /// roll in degrees) of the bone in `slot` (0-3).
    pub fn bone_angles(&self, slot: usize) -> [f32; 3] {
        const FIELDS: [[usize; 3]; 4] = [
            [110, 45, 67],
            [111, 112, 113],
            [114, 115, 116],
            [117, 118, 119],
        ];
        FIELDS.get(slot).map_or([0.0; 3], |fields| {
            fields.map(|field| f32::from_bits(self.raw_field(field).unwrap_or(0)))
        })
    }

    /// Protocol-26 `entityState_t::boneOrient` (netfield 90): the up, right and forward axes
    /// the server's bone turns are about, three bits each (`cg_players.c:3978-3982`).
    pub fn bone_orient(&self) -> u32 {
        self.raw_field(90).unwrap_or(0)
    }

    /// Protocol-26 `entityState_t::iModelScale` (netfield 76): the model's scale as a
    /// percentage, zero meaning unscaled (`cg_players.c:8479-8493`).
    pub fn model_scale_percent(&self) -> i32 {
        self.raw_field(76).unwrap_or(0) as i32
    }

    /// Protocol-26 `entityState_t::npcSaber1` / `npcSaber2` (netfields 72 and
    /// 102): `CS_MODELS` indices of an NPC's saber definition names, zero when
    /// that slot carries no saber (`codemp/game/NPC_stats.c`, read back by
    /// `CG_G2AnimEntModelLoad` in `cg_players.c:7184-7203`).
    pub fn npc_saber_indices(&self) -> [u16; 2] {
        [72, 102].map(|field| self.raw_field(field).unwrap_or(0) as u16)
    }

    /// Protocol-26 `entityState_t::boltToPlayer` (netfield 101). NPCs reuse
    /// it as two packed 3-bit saber colours (`NPC_stats.c:2723-2724`,
    /// `:2833-2834`): bits 0-2 are saber 0's `saber_colors_t + 1`, bits 3-5
    /// saber 1's, zero meaning "not overridden".
    pub fn bolt_to_player(&self) -> u8 {
        self.raw_field(101).unwrap_or(0) as u8
    }

    /// Protocol-26 `entityState_t::modelGhoul2` (netfield 54): non-zero when
    /// the server built the entity's model as a Ghoul2 instance (NPCs and
    /// thrown sabers, `g_client.c:1750`).
    pub fn model_ghoul2(&self) -> u8 {
        self.raw_field(54).unwrap_or(0) as u8
    }

    /// Protocol-26 `entityState_t::powerups` bit mask (netfield 77).
    pub fn powerups(&self) -> u32 {
        self.raw_field(77).unwrap_or(0)
    }

    /// Protocol-26 `entityState_t::health` (netfield 69).
    pub fn health(&self) -> u32 {
        self.raw_field(69).unwrap_or(0)
    }

    /// Protocol-26 `entityState_t::maxhealth` (netfield 73); zero when unsent.
    pub fn max_health(&self) -> u32 {
        self.raw_field(73).unwrap_or(0)
    }

    /// Protocol-26 `entityState_t::bolt1` (netfield 91).
    pub fn bolt1(&self) -> bool {
        self.raw_field(91).unwrap_or(0) != 0
    }

    /// Protocol-26 `entityState_t::isJediMaster` (netfield 97).
    pub fn is_jedi_master(&self) -> bool {
        self.raw_field(97).unwrap_or(0) != 0
    }

    /// Test one client in the four protocol-26 `trickedentindex` bitsets.
    pub fn client_bitflag(&self, client: u16) -> bool {
        let (field, bit) = match client {
            0..=15 => (58, client),
            16..=31 => (74, client - 16),
            32..=47 => (92, client - 32),
            48..=63 => (94, client - 48),
            _ => return false,
        };
        self.raw_field(field).unwrap_or(0) & (1_u32 << bit) != 0
    }

    pub fn saber_holstered(&self) -> u8 {
        self.raw_field(71).unwrap_or(0) as u8
    }

    fn removed(schema: &[EntityField]) -> Self {
        Self::zero(ENTITY_NUMBER_NONE, schema)
    }
}

pub fn read_delta_entity(
    message: &mut MessageReader<'_>,
    from: &EntityState,
    number: u16,
    schema: &[EntityField],
) -> Result<EntityState, EntityDeltaError> {
    if usize::from(number) >= MAX_LEGACY_ENTITIES {
        return Err(EntityDeltaError::InvalidEntityNumber(number));
    }
    if from.fields.len() != schema.len() {
        return Err(EntityDeltaError::SchemaLengthMismatch {
            state_fields: from.fields.len(),
            schema_fields: schema.len(),
        });
    }

    if message.read_bits(1)? != 0 {
        return Ok(EntityState::removed(schema));
    }

    let mut state = from.clone();
    state.number = number;
    if message.read_bits(1)? == 0 {
        return Ok(state);
    }

    let changed_field_count = usize::from(message.read_u8()?);
    if changed_field_count > schema.len() {
        return Err(EntityDeltaError::InvalidChangedFieldCount {
            actual: changed_field_count,
            maximum: schema.len(),
        });
    }

    for (index, field) in schema.iter().take(changed_field_count).enumerate() {
        if message.read_bits(1)? == 0 {
            continue;
        }

        state.fields[index] = match field.encoding {
            EntityFieldEncoding::Float => read_float_field(message)?,
            EntityFieldEncoding::Integer { width, signed } => {
                if message.read_bits(1)? == 0 {
                    0
                } else if signed {
                    message.read_signed_bits(width)? as u32
                } else {
                    message.read_bits(width)?
                }
            }
        };
    }

    Ok(state)
}

fn read_float_field(message: &mut MessageReader<'_>) -> Result<u32, MessageError> {
    if message.read_bits(1)? == 0 {
        return Ok(0.0_f32.to_bits());
    }
    if message.read_bits(1)? == 0 {
        let integral = message.read_bits(FLOAT_INT_BITS)? as i32 - FLOAT_INT_BIAS;
        Ok((integral as f32).to_bits())
    } else {
        message.read_bits(32)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntityDeltaError {
    Message(MessageError),
    InvalidEntityNumber(u16),
    SchemaLengthMismatch {
        state_fields: usize,
        schema_fields: usize,
    },
    InvalidChangedFieldCount {
        actual: usize,
        maximum: usize,
    },
}

impl fmt::Display for EntityDeltaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(error) => error.fmt(formatter),
            Self::InvalidEntityNumber(number) => {
                write!(formatter, "invalid entity number {number}")
            }
            Self::SchemaLengthMismatch {
                state_fields,
                schema_fields,
            } => write!(
                formatter,
                "entity has {state_fields} fields but schema has {schema_fields}"
            ),
            Self::InvalidChangedFieldCount { actual, maximum } => write!(
                formatter,
                "entity delta contains {actual} fields, exceeding schema length {maximum}"
            ),
        }
    }
}

impl Error for EntityDeltaError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Message(error) => Some(error),
            _ => None,
        }
    }
}

impl From<MessageError> for EntityDeltaError {
    fn from(value: MessageError) -> Self {
        Self::Message(value)
    }
}
