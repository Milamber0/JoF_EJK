//! A vehicle as the game keeps it (`Vehicle_t`, `codemp/game/bg_vehicles.h:394-560`): the
//! state of one vehicle NPC — its definition, its riders, its orientation and speed
//! control, its weapons' and turrets' ammunition, its boarding and dying.
//!
//! It belongs to its NPC ([`crate::npc_spawn::NpcActor::vehicle`]), a native actor of the
//! core: riders are named by their entity numbers, the capacity for passengers is the
//! definition's own. The vehicle's movement code ([`crate::vehicle_move`]) and the game's
//! vehicle functions ([`crate::vehicle_update`], [`crate::vehicle_spawn`]) share it, as the
//! reference's `bg` and game code share `Vehicle_t`.

use crate::vehicle_fields::{VEHICLE_MUZZLES, VEHICLE_TURRETS, VEHICLE_WEAPONS, VehicleInfo, kind};
use sjk_protocol::UserCommand;
use std::sync::Arc;

/// `MAX_VEHICLE_EXHAUSTS`.
pub const VEHICLE_EXHAUSTS: usize = 12;

/// `vehFlags_t` (`bg_vehicles.h:338-344`).
pub mod flags {
    pub const FLYING: u32 = 0x1;
    pub const CRASHING: u32 = 0x2;
    pub const LANDING: u32 = 0x4;
    pub const BUCKING: u32 = 0x10;
    pub const WINGS_OPEN: u32 = 0x20;
    pub const GEARS_OPEN: u32 = 0x40;
    pub const SLIDE_BREAKING: u32 = 0x80;
    pub const SPINNING: u32 = 0x100;
    pub const OUT_OF_CONTROL: u32 = 0x200;
    pub const SABER_IN_LEFT_HAND: u32 = 0x400;
}

/// `vehEject_t`: the way a rider is thrown off.
pub mod eject {
    pub const LEFT: i32 = 0;
    pub const RIGHT: i32 = 1;
    pub const FRONT: i32 = 2;
    pub const REAR: i32 = 3;
    pub const TOP: i32 = 4;
    pub const BOTTOM: i32 = 5;
}

/// `m_LandTrace`: what `BG_FighterUpdate` found under a fighter, its landing height
/// down (the parts of the `trace_t` the game reads).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LandTrace {
    /// `fraction`: 0 until the first trace, as the cleared `Vehicle_t` has it.
    pub fraction: f32,
    /// `plane.normal`.
    pub normal: [f32; 3],
}

/// `vehWeaponStatus_t`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WeaponStatus {
    pub linked: bool,
    pub ammo: i32,
    pub last_ammo_inc: i32,
    pub next_muzzle: i32,
}

/// `vehTurretStatus_t`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TurretStatus {
    pub ammo: i32,
    pub last_ammo_inc: i32,
    pub next_muzzle: i32,
    pub enemy: i32,
    pub enemy_hold_time: i32,
}

/// `Vehicle_t`.
#[derive(Clone, Debug)]
pub struct Vehicle {
    /// `m_pVehicleInfo`: the definition, and its index in the level's table.
    pub info: Arc<VehicleInfo>,
    pub info_index: usize,
    /// `m_pPilot`, `m_pOldPilot`: entity numbers.
    pub pilot: Option<u16>,
    pub old_pilot: Option<u16>,
    /// `m_iPilotTime`, `m_iPilotLastIndex`, `m_bHasHadPilot`: the no-pilot death timer.
    pub pilot_time: i32,
    pub pilot_last_index: i32,
    pub has_had_pilot: bool,
    /// `m_ppPassengers`, `m_iNumPassengers`: one slot per passenger the definition allows.
    pub passengers: Vec<Option<u16>>,
    pub passenger_count: i32,
    /// `m_pDroidUnit`.
    pub droid_unit: Option<u16>,
    /// `m_iBoarding`: 0, a negative boarding side, or the time boarding ends.
    pub boarding: i32,
    pub was_boarding: bool,
    pub boarding_velocity: [f32; 3],
    /// `m_fTimeModifier`: the move's frame time times 60.
    pub time_modifier: f32,
    /// Bolts the Ghoul2 setup found (`m_iExhaustTag`, `m_iMuzzleTag`, `m_iDroidUnitTag`,
    /// `m_iGunnerViewTag`), -1 for none.
    pub exhaust_tags: [i32; VEHICLE_EXHAUSTS],
    pub muzzle_tags: [i32; VEHICLE_MUZZLES],
    pub droid_unit_tag: i32,
    pub gunner_view_tags: [i32; VEHICLE_TURRETS],
    /// `m_iMuzzleWait`: when each muzzle may fire again; `m_iMuzzleTime`: the level time
    /// its place was last read (`WP_CalcVehMuzzle` reads it once a frame); `m_vMuzzlePos`,
    /// `m_vMuzzleDir`: that place and the way it points ([`crate::vehicle_weapons`]).
    pub muzzle_wait: [i32; VEHICLE_MUZZLES],
    pub muzzle_time: [i32; VEHICLE_MUZZLES],
    pub muzzle_pos: [[f32; 3]; VEHICLE_MUZZLES],
    pub muzzle_dir: [[f32; 3]; VEHICLE_MUZZLES],
    /// `m_ucmd`: the command the vehicle moves by.
    pub ucmd: UserCommand,
    /// `m_EjectDir`.
    pub eject_dir: i32,
    /// `m_ulFlags` ([`flags`]).
    pub flags: u32,
    /// `m_vOrientation` (the parent's `ps.vehOrientation`) and `m_vPrevOrientation`.
    pub orientation: [f32; 3],
    pub prev_orientation: [f32; 3],
    /// `m_vPrevRiderViewAngles`.
    pub prev_rider_view_angles: [f32; 3],
    /// `m_vAngularVelocity`.
    pub angular_velocity: f32,
    /// `m_vFullAngleVelocity`: the turn a fighter's impact left it to work off.
    pub full_angle_velocity: [f32; 3],
    /// `m_LandTrace` ([`LandTrace`]).
    pub land_trace: LandTrace,
    /// The parent's `ps.vehTurnaroundIndex` and `ps.vehTurnaroundTime`: the point a ship
    /// boundary turns it back toward, and until when ([`crate::vehicle_triggers`]).
    pub turnaround_index: u16,
    pub turnaround_time: i32,
    /// The parent's `locationDamage` for each side of a fighter (`SHIPSURF_FRONT` ..
    /// `SHIPSURF_LEFT`, [`crate::vehicle_surfaces`]).
    pub surface_damage: [i32; 4],
    /// The parent's `client->inSpaceIndex`: the `trigger_space` it is in (0, or
    /// `ENTITYNUM_NONE`, for none).
    pub in_space_index: u16,
    /// `m_iArmor`, `m_iShields`.
    pub armor: i32,
    pub shields: i32,
    /// `m_iHitDebounce`, `m_iDieTime`, `m_iTurboTime`, `m_iDropTime`,
    /// `m_iSoundDebounceTimer`, `lastShieldInc`.
    pub hit_debounce: i32,
    pub die_time: i32,
    pub turbo_time: i32,
    pub drop_time: i32,
    pub sound_debounce_timer: i32,
    pub last_shield_inc: i32,
    /// The parent's `ps.hyperSpaceTime` and `ps.hyperSpaceAngles`, which no field of the
    /// wire's player state keeps: a rider faces the angles while the time is under four
    /// seconds old ([`crate::vehicle_riders::face_hyperspace_point`]).
    pub hyperspace_time: i32,
    pub hyperspace_angles: [f32; 3],
    /// `m_iLastImpactDmg`, `m_iRemovedSurfaces`.
    pub last_impact_damage: i32,
    pub removed_surfaces: i32,
    /// `linkWeaponToggleHeld`.
    pub link_weapon_toggle_held: bool,
    pub weapon_status: [WeaponStatus; VEHICLE_WEAPONS],
    pub turret_status: [TurretStatus; VEHICLE_TURRETS],
    /// The parent's player-state fields no client is sent: `vehBoarding`,
    /// `vehWeaponsLinked`, `vehSurfaces`.
    pub ps_boarding: bool,
    pub ps_weapons_linked: bool,
    pub ps_surfaces: i32,
    /// What the spawner handed on (`NPC_Spawn_Do`, `NPC_spawn.c:1550-1565`): the drop after
    /// a suspension ends (`fly_sound_debounce_time`), the no-pilot death's delay and
    /// distance (`damage`, `speed`), and the droid unit's NPC (`model2`).
    pub drop_delay: i32,
    pub no_pilot_delay: i32,
    pub no_pilot_distance: f32,
    pub droid_npc: Option<Vec<u8>>,
}

impl Vehicle {
    /// `G_CreateSpeederNPC`, `G_CreateAnimalNPC`, `G_CreateWalkerNPC`, `G_CreateFighterNPC`
    /// (`SpeederNPC.c:664-678`): a cleared vehicle of the definition.
    pub fn new(info: Arc<VehicleInfo>, info_index: usize) -> Self {
        let passengers = vec![None; info.max_passengers.max(0) as usize];
        Self {
            info,
            info_index,
            pilot: None,
            old_pilot: None,
            pilot_time: 0,
            pilot_last_index: 0,
            has_had_pilot: false,
            passengers,
            passenger_count: 0,
            droid_unit: None,
            boarding: 0,
            was_boarding: false,
            boarding_velocity: [0.0; 3],
            time_modifier: 0.0,
            exhaust_tags: [0; VEHICLE_EXHAUSTS],
            muzzle_tags: [0; VEHICLE_MUZZLES],
            droid_unit_tag: 0,
            gunner_view_tags: [0; VEHICLE_TURRETS],
            muzzle_wait: [0; VEHICLE_MUZZLES],
            muzzle_time: [0; VEHICLE_MUZZLES],
            muzzle_pos: [[0.0; 3]; VEHICLE_MUZZLES],
            muzzle_dir: [[0.0; 3]; VEHICLE_MUZZLES],
            ucmd: UserCommand::default(),
            eject_dir: 0,
            flags: 0,
            orientation: [0.0; 3],
            prev_orientation: [0.0; 3],
            prev_rider_view_angles: [0.0; 3],
            angular_velocity: 0.0,
            full_angle_velocity: [0.0; 3],
            land_trace: LandTrace::default(),
            surface_damage: [0; 4],
            turnaround_index: 0,
            turnaround_time: 0,
            in_space_index: 0,
            armor: 0,
            shields: 0,
            hit_debounce: 0,
            die_time: 0,
            turbo_time: 0,
            drop_time: 0,
            sound_debounce_timer: 0,
            last_shield_inc: 0,
            hyperspace_time: 0,
            hyperspace_angles: [0.0; 3],
            last_impact_damage: 0,
            removed_surfaces: 0,
            link_weapon_toggle_held: false,
            weapon_status: [WeaponStatus::default(); VEHICLE_WEAPONS],
            turret_status: [TurretStatus::default(); VEHICLE_TURRETS],
            ps_boarding: false,
            ps_weapons_linked: false,
            ps_surfaces: 0,
            drop_delay: 0,
            no_pilot_delay: 0,
            no_pilot_distance: 0.0,
            droid_npc: None,
        }
    }

    /// `self` made a copy of `source`, reusing its own storage (no allocation where the
    /// passenger slots and the droid's name fit): for a client copying its predicted ride
    /// every frame.
    pub fn copy_from(&mut self, source: &Self) {
        let mut passengers = std::mem::take(&mut self.passengers);
        let mut droid_npc = self.droid_npc.take();
        passengers.clone_from(&source.passengers);
        droid_npc.clone_from(&source.droid_npc);
        *self = Self {
            info: Arc::clone(&source.info),
            passengers,
            droid_npc,
            ..*source
        };
    }

    /// `m_pVehicleInfo->type`.
    pub fn kind(&self) -> i32 {
        self.info.kind
    }

    /// Whether the vehicle's movement is one this server moves: a speeder's, an animal's,
    /// a walker's or a fighter's.
    pub fn moves_here(&self) -> bool {
        matches!(
            self.info.kind,
            kind::SPEEDER | kind::ANIMAL | kind::WALKER | kind::FIGHTER
        )
    }

    /// Whether the parent is in a `trigger_space` (`inSpaceIndex && != ENTITYNUM_NONE`).
    pub fn in_space(&self) -> bool {
        self.in_space_index != 0 && self.in_space_index != 1_023
    }

    /// `Inhabited` (`g_vehicles.c:2459`): a pilot or a passenger aboard.
    pub fn inhabited(&self) -> bool {
        self.pilot.is_some() || self.passenger_count != 0
    }
}
