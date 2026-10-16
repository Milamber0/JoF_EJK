//! Vehicles made from their spawners: the map's `NPC_Vehicle` (`SP_NPC_Vehicle`,
//! `NPC_VehiclePrecache`, `G_VehicleSpawn`, `codemp/game/NPC_spawn.c:2085-2235`,
//! `g_vehicles.c:77-111`), the vehicle part of `NPC_Spawn_Do` (`NPC_spawn.c:1478-1570`:
//! `G_CreateSpeederNPC` and the other kinds, the vehicle's `Initialize`,
//! `g_vehicles.c:1049-1165`) and of `NPC_Begin`.
//!
//! The vehicle is the NPC's own record ([`Vehicle`]); its definition comes from the level's
//! vehicle table ([`VehicleTable`]), loaded on first use as the reference loads it.
//!
//! What is left for later: the droid unit a vehicle with a `*droidunit` bolt spawns at its
//! begin (a model's bolts are not read here), a walker's and a fighter's own set-up, and
//! `SHY` vehicle spawners.

use crate::npc_parms::NpcVehicle;
use crate::npc_spawn::{FL_NO_KNOCKBACK, FL_SHIELDED, NpcActor, NpcHost};
use crate::npc_spawners::NpcSpawner;
use crate::vehicle::{Vehicle, flags};
use crate::vehicle_fields::{VEHICLE_TURRETS, VEHICLE_WEAPONS, kind};
use crate::vehicle_parms::{VehicleRegistry, VehicleTable};

/// `FL_DMG_BY_HEAVY_WEAP_ONLY`.
const FL_DMG_BY_HEAVY_WEAP_ONLY: u32 = 0x200_0000;
/// `NPCAI_CUSTOM_GRAVITY`.
const NPCAI_CUSTOM_GRAVITY: u32 = 0x20_0000;
/// `STAT_HEALTH`, `STAT_WEAPONS`, `STAT_ARMOR`, `STAT_MAX_HEALTH`.
const STAT_HEALTH: usize = 0;
const STAT_WEAPONS: usize = 4;
const STAT_ARMOR: usize = 5;
const STAT_MAX_HEALTH: usize = 8;
/// `WP_BLASTER`; `WEAPON_READY`.
const WP_BLASTER: u32 = 5;
const WEAPON_READY: u32 = 0;
/// `BOTH_VS_IDLE`.
const BOTH_VS_IDLE: u32 = 1_036;
/// `BS_CINEMATIC`.
const BS_CINEMATIC: i32 = 9;
/// `ENTITYNUM_NONE`: a turret's enemy.
const ENTITYNUM_NONE: i32 = 1_023;

/// What an `NPC_Vehicle` spawner reads beyond an `NPC_spawner`'s keys (`SP_NPC_Vehicle`):
/// `dropTime` (as `fly_sound_debounce_time`, milliseconds), `dmg` (the no-pilot death's
/// delay), `speed` (its distance) and `model2` (the droid unit's NPC).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VehicleKeys {
    pub drop_delay: i32,
    pub no_pilot_delay: i32,
    pub no_pilot_distance: f32,
    pub droid_npc: Option<Vec<u8>>,
}

impl VehicleKeys {
    /// The keys of a map entity (`G_SpawnFloat("dropTime")` and the fields `dmg`, `speed`
    /// and `model2`).
    pub fn from_entity(entity: &sjk_entity::Entity) -> Self {
        let drop_time = entity
            .get("droptime")
            .map_or(0.0, |value| crate::text_parse::atof(value.as_bytes()));
        Self {
            drop_delay: if drop_time != 0.0 {
                (f64::from(drop_time) * 1000.0).ceil() as i32
            } else {
                0
            },
            no_pilot_delay: entity
                .get("dmg")
                .map_or(0, |value| crate::userinfo::atoi(value.as_bytes())),
            no_pilot_distance: entity
                .get("speed")
                .map_or(0.0, |value| crate::text_parse::atof(value.as_bytes())),
            droid_npc: entity
                .get("model2")
                .map(|value| crate::npc_parms::new_string(value.as_bytes())),
        }
    }
}

/// `SP_NPC_Vehicle`'s own reading of the spawner (`NPC_spawn.c:2170-2200`), over what
/// `SP_NPC_spawner`'s would be: its count as the map gave it, no sound keys, no default
/// full name, `g_allowNPC` not asked, and its time to spawn the caller's
/// ([`place`]).
pub fn vehicle_spawner(spawner: &mut NpcSpawner, entity: &sjk_entity::Entity) {
    spawner.count = entity
        .get("count")
        .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()));
    spawner.sound_flags = 0;
    spawner.full_name = entity
        .get("fullname")
        .map(|value| crate::npc_parms::new_string(value.as_bytes()))
        .unwrap_or_default();
    spawner.spawn_at = None;
    spawner.vehicle_keys = VehicleKeys::from_entity(entity);
}

/// When an `NPC_Vehicle` spawner spawns (`SP_NPC_Vehicle`, `NPC_spawn.c:2208-2234`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Now, with the map (`G_VehicleSpawn`).
    Now,
    /// At this time (`think = G_VehicleSpawn`), precached.
    At(i32),
    /// When used by its name (`use = NPC_VehicleSpawnUse`), precached.
    WhenUsed,
    /// Never: its vehicle is not defined (`NPC_VehiclePrecache` failed); the spawner is freed.
    Refused,
}

/// Where a map's `NPC_Vehicle` spawner goes: precached (`NPC_VehiclePrecache`) unless it
/// spawns at once.
pub fn place(
    spawner: &NpcSpawner,
    level_time: i32,
    table: &mut VehicleTable,
    parms: &crate::npc_parms::NpcParms,
    host: &mut impl NpcHost,
) -> Placement {
    if spawner.targetname.is_none() && spawner.delay == 0 {
        return Placement::Now;
    }
    if !precache(spawner, table, parms, host) {
        return Placement::Refused;
    }
    if spawner.targetname.is_some() {
        Placement::WhenUsed
    } else {
        Placement::At(level_time + spawner.delay)
    }
}

/// `NPC_VehiclePrecache` (`NPC_spawn.c:2085-2155`): the vehicle looked up, `$<type>`
/// registered, and the droid unit's NPC precached (R2 and R5 both for `random` or
/// `default`). The model, skin and animation file it caches register nothing a client is
/// told. Whether the vehicle is defined.
pub fn precache(
    spawner: &NpcSpawner,
    table: &mut VehicleTable,
    parms: &crate::npc_parms::NpcParms,
    host: &mut impl NpcHost,
) -> bool {
    let name = spawner.npc_type.clone().unwrap_or_default();
    let Some(index) = table.index_for_name(&name, &mut Registry(host)) else {
        return false;
    };
    host.model_index(&[b"$".as_slice(), &name].concat());
    let info_droid = table
        .vehicle(index)
        .and_then(|info| info.droid_npc.clone())
        .filter(|droid| !droid.is_empty());
    let droid = spawner
        .vehicle_keys
        .droid_npc
        .clone()
        .filter(|droid| !droid.is_empty())
        .or(info_droid);
    if let Some(droid) = droid {
        let types: Vec<&[u8]> =
            if droid.eq_ignore_ascii_case(b"random") || droid.eq_ignore_ascii_case(b"default") {
                vec![b"r2d2", b"r5d2"]
            } else {
                vec![&droid]
            };
        for name in types {
            // `NPC_PrecacheType`: a spawner of no flags.
            let precached = crate::npc_precache::spawner_precache(parms, Some(name), 0, 0);
            for model in &precached.models {
                host.model_index(model);
            }
            for sound in &precached.sounds {
                host.sound_index(sound);
            }
            for weapon in &precached.weapons {
                if let Some(item) = crate::npc_spawners::Registration::Weapon(*weapon).item() {
                    host.register_item(item);
                }
            }
        }
    }
    true
}

/// The vehicle part of `NPC_Spawn_Do` before the parse (`NPC_spawn.c:1478-1520`): the
/// vehicle looked up and made (`G_Create*NPC`, which looks it up again). `None` where it is
/// not defined, or of no kind the game makes (the reference's `Couldn't spawn NPC`).
pub fn create(
    npc_type: &[u8],
    table: &mut VehicleTable,
    host: &mut impl NpcHost,
) -> Option<Vehicle> {
    let index = table.index_for_name(npc_type, &mut Registry(host))?;
    let kind_of = table.vehicle(index)?.kind;
    if !matches!(
        kind_of,
        kind::ANIMAL | kind::SPEEDER | kind::FIGHTER | kind::WALKER
    ) {
        host.print(&format!(
            "^1ERROR: Couldn't spawn NPC {}\n",
            String::from_utf8_lossy(npc_type)
        ));
        return None;
    }
    let index = table.index_for_name(npc_type, &mut Registry(host))?;
    let info = std::sync::Arc::new(table.vehicle(index)?.clone());
    Some(Vehicle::new(info, index))
}

/// How the parse treats the NPC spawned as `vehicle`.
pub fn parse_kind(vehicle: &Vehicle) -> NpcVehicle {
    if vehicle.kind() == kind::FIGHTER {
        NpcVehicle::Fighter
    } else {
        NpcVehicle::Other
    }
}

/// The vehicle's `Initialize` and the rest of `NPC_Spawn_Do`'s vehicle part
/// (`g_vehicles.c:1049-1165`, `NPC_spawn.c:1520-1570`), applied to the NPC the spawn made.
/// The reference runs `Initialize` before the definition is parsed, so where both write a
/// field the parse's value stands: the weapon a `weapon` key named, and the ammunition
/// it filled.
pub fn initialize(
    npc: &mut NpcActor,
    mut vehicle: Vehicle,
    spawner: &NpcSpawner,
    host: &mut impl NpcHost,
) {
    let gravity = host.gravity();
    let info = std::sync::Arc::clone(&vehicle.info);
    vehicle.armor = info.armor;
    if info.armor != 0 {
        npc.health = info.armor;
    }
    npc.max_health = info.armor;
    npc.player.stats[STAT_MAX_HEALTH] = info.armor as u32;
    npc.player.stats[STAT_HEALTH] = info.armor as u32;
    vehicle.shields = info.shields;
    crate::vehicle_update::update_shields(&vehicle, &mut npc.player);
    npc.player.stats[STAT_ARMOR] = info.shields as u32;
    let filled = npc.definition.ammo_filled;
    for slot in 0..VEHICLE_WEAPONS {
        vehicle.weapon_status[slot].ammo = info.weapons[slot].ammo_max;
        if filled & (1 << slot) == 0 {
            npc.player.ammo[slot] = info.weapons[slot].ammo_max as u32;
        }
    }
    for slot in 0..VEHICLE_TURRETS {
        let turret = &info.turrets[slot];
        // `turret[i].iMuzzle[i]`: each turret's own-numbered muzzle.
        vehicle.turret_status[slot].next_muzzle = turret.muzzles[slot] - 1;
        vehicle.turret_status[slot].ammo = turret.ammo_max;
        if filled & (1 << (VEHICLE_WEAPONS + slot)) == 0 {
            npc.player.ammo[VEHICLE_WEAPONS + slot] = turret.ammo_max as u32;
        }
        if turret.ai {
            vehicle.turret_status[slot].enemy = ENTITYNUM_NONE;
        }
    }
    npc.player.set_speed(0.0);
    if info.gravity != 0 && info.gravity as f32 != gravity {
        npc.ai_flags |= NPCAI_CUSTOM_GRAVITY;
        npc.player.set_gravity(info.gravity);
    }
    vehicle.flags = flags::GEARS_OPEN;
    vehicle.time_modifier = 1.0;
    vehicle.exhaust_tags = [-1; crate::vehicle::VEHICLE_EXHAUSTS];
    vehicle.muzzle_tags = [-1; crate::vehicle_fields::VEHICLE_MUZZLES];
    vehicle.droid_unit_tag = -1;
    set_bolts(npc, &mut vehicle, host);
    if npc.definition.weapon.is_none() {
        npc.player
            .set_raw_field(crate::npc_begin::ps::WEAPON, WP_BLASTER);
    }
    npc.player
        .set_raw_field(crate::npc_begin::ps::WEAPON_STATE, WEAPON_READY);
    npc.player.stats[STAT_WEAPONS] |= 1 << WP_BLASTER;
    // `BG_SetAnim(SETANIM_BOTH, BOTH_VS_IDLE, SETANIM_FLAG_NORMAL)` over a state with no
    // timers running.
    npc.player
        .set_raw_field(crate::npc_begin::ps::LEGS_ANIM, BOTH_VS_IDLE);
    npc.player
        .set_raw_field(crate::npc_begin::ps::TORSO_ANIM, BOTH_VS_IDLE);
    if info.kind == kind::FIGHTER {
        npc.flags |= FL_NO_KNOCKBACK | FL_SHIELDED | FL_DMG_BY_HEAVY_WEAP_ONLY;
    }
    // "Ships spawning in pointing straight down": landed, facing the spawner's yaw, the
    // view set to it (`SetClientViewAngle` against a command with no angles).
    vehicle.orientation = [0.0, spawner.angles[1], 0.0];
    npc.player
        .set_delta_angles(vehicle.orientation.map(crate::npc_think::angle_to_short));
    vehicle.drop_delay = spawner.vehicle_keys.drop_delay;
    vehicle.no_pilot_delay = spawner.vehicle_keys.no_pilot_delay;
    vehicle.no_pilot_distance = spawner.vehicle_keys.no_pilot_distance;
    vehicle.droid_npc = spawner.vehicle_keys.droid_npc.clone();
    npc.vehicle = Some(Box::new(vehicle));
}

/// The bolts the Ghoul2 setup finds on a vehicle's model (`SetupGameGhoul2Model`,
/// `g_client.c:1847-1890`) — the parse runs it after `Initialize`, for an NPC whose
/// definition names its class a vehicle: the droid unit, twelve exhausts, twelve muzzles
/// (`*muzzleN`, else `*flashN`), and the turrets' gunner views.
fn set_bolts(npc: &NpcActor, vehicle: &mut Vehicle, host: &mut impl NpcHost) {
    if npc.definition.entity_class != crate::npc_parms::CLASS_VEHICLE {
        return;
    }
    let model = vehicle.info.model.clone().unwrap_or_default();
    vehicle.droid_unit_tag = host.bolt(&model, "*droidunit");
    for (index, tag) in vehicle.exhaust_tags.iter_mut().enumerate() {
        *tag = host.bolt(&model, &format!("*exhaust{}", index + 1));
    }
    for (index, tag) in vehicle.muzzle_tags.iter_mut().enumerate() {
        *tag = host.bolt(&model, &format!("*muzzle{}", index + 1));
        if *tag == -1 {
            *tag = host.bolt(&model, &format!("*flash{}", index + 1));
        }
    }
    for (index, tag) in vehicle.gunner_view_tags.iter_mut().enumerate() {
        *tag = match &vehicle.info.turrets[index].gunner_view_tag {
            Some(name) => host.bolt(&model, &String::from_utf8_lossy(name)),
            None => -1,
        };
    }
}

/// `G_VehicleSpawn`'s part after the spawn (`g_vehicles.c:95-111`): a vehicle that is no
/// animal waits in the cinematic state, and one that dies without a pilot
/// (`NO_PILOT_DIE`) starts its timer — ten seconds and 512 units unless the map said.
pub fn after_spawn(npc: &mut NpcActor, level_time: i32) {
    let Some(vehicle) = npc.vehicle.as_deref_mut() else {
        return;
    };
    if vehicle.kind() != kind::ANIMAL {
        npc.behavior_state = BS_CINEMATIC;
    }
    if npc.spawnflags & 1 != 0 {
        if vehicle.no_pilot_delay == 0 {
            vehicle.no_pilot_delay = 10_000;
        }
        if vehicle.no_pilot_distance == 0.0 {
            vehicle.no_pilot_distance = 512.0;
        }
        vehicle.pilot_time = level_time + vehicle.no_pilot_delay;
    }
}

/// The host's registrations and console as the vehicle table asks for them.
pub struct Registry<'a, H: NpcHost>(pub &'a mut H);

impl<H: NpcHost> VehicleRegistry for Registry<'_, H> {
    fn model_index(&mut self, name: &[u8]) -> i32 {
        i32::from(self.0.model_index(name))
    }
    fn sound_index(&mut self, name: &[u8]) -> i32 {
        i32::from(self.0.sound_index(name))
    }
    fn effect_index(&mut self, name: &[u8]) -> i32 {
        i32::from(self.0.effect_index(name))
    }
    fn print(&mut self, text: &str) {
        self.0.print(text);
    }
}
