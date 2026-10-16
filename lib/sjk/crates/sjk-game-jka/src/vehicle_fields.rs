//! What a vehicle and a vehicle weapon are (`vehicleInfo_t`, `vehWeaponInfo_t`,
//! `codemp/game/bg_vehicles.h`), and the keys their definition files set: the
//! reference's field tables (`vehicleFields`, `vehWeaponFields`,
//! `bg_vehicleLoad.c:105-132`, `391-604`) as data, in the reference's order.
//!
//! A key's kind says what the game does with its value ([`FieldKind`]). The kinds the
//! game module leaves to the client (models, effects, shaders and sounds registered only
//! by cgame) are known keys that change nothing on the server: they have no storage here.

/// `MAX_VEHICLE_WEAPONS`: the fire and alternate fire of a vehicle.
pub const VEHICLE_WEAPONS: usize = 2;
/// `MAX_VEHICLE_MUZZLES`.
pub const VEHICLE_MUZZLES: usize = 12;
/// `MAX_VEHICLE_TURRETS`, `MAX_VEHICLE_TURRET_MUZZLES`.
pub const VEHICLE_TURRETS: usize = 2;
pub const TURRET_MUZZLES: usize = 2;
/// `VEH_MAX_PASSENGERS`: `maxPassengers` is clamped to it.
pub const MAX_PASSENGERS: i32 = 10;

/// `vehicleType_t`.
pub mod kind {
    pub const NONE: i32 = 0;
    pub const WALKER: i32 = 1;
    pub const FIGHTER: i32 = 2;
    pub const SPEEDER: i32 = 3;
    pub const ANIMAL: i32 = 4;
    pub const FLIER: i32 = 5;
    /// `VehicleTable`: the names the `type` key is read by.
    pub const NAMES: [&str; 6] = [
        "VH_NONE",
        "VH_WALKER",
        "VH_FIGHTER",
        "VH_SPEEDER",
        "VH_ANIMAL",
        "VH_FLIER",
    ];
}

/// `vehWeaponStats_t`: one of the vehicle's two weapons.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VehicleWeaponStats {
    /// `ID`: the weapon's index in the weapon table (`VEH_WEAPON_NONE`, -1, for none found).
    pub id: i32,
    pub delay: i32,
    pub linkable: i32,
    pub aim_correct: bool,
    pub ammo_max: i32,
    pub ammo_recharge_ms: i32,
}

/// `turretStats_t`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TurretStats {
    pub weapon: i32,
    pub delay: i32,
    pub ammo_max: i32,
    pub ammo_recharge_ms: i32,
    pub yaw_bone: Option<Vec<u8>>,
    pub pitch_bone: Option<Vec<u8>>,
    pub yaw_axis: i32,
    pub pitch_axis: i32,
    pub yaw_clamp_left: f32,
    pub yaw_clamp_right: f32,
    pub pitch_clamp_up: f32,
    pub pitch_clamp_down: f32,
    pub muzzles: [i32; TURRET_MUZZLES],
    pub gunner_view_tag: Option<Vec<u8>>,
    pub turn_speed: f32,
    pub ai: bool,
    pub ai_lead: bool,
    pub ai_range: f32,
    pub passenger_num: i32,
}

/// `vehicleInfo_t`: a vehicle's definition. Everything starts at zero
/// (`BG_VehicleSetDefaults` is a `memset`); sound, effect and model fields hold the
/// configstring indices the game registered.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VehicleInfo {
    pub name: Option<Vec<u8>>,
    /// `type` (`vehicleType_t`, [`kind`]; -1 for a name not known).
    pub kind: i32,
    pub num_hands: i32,
    pub look_pitch: f32,
    pub look_yaw: f32,
    pub length: f32,
    pub width: f32,
    pub height: f32,
    pub center_of_gravity: [f32; 3],
    pub speed_max: f32,
    pub turbo_speed: f32,
    pub speed_min: f32,
    pub speed_idle: f32,
    pub accel_idle: f32,
    pub acceleration: f32,
    pub decel_idle: f32,
    /// `throttleSticks`: a float the table writes a `qboolean` into, so a true value is
    /// the float with the bits of 1.
    pub throttle_sticks: f32,
    pub strafe_perc: f32,
    pub banking_speed: f32,
    pub roll_limit: f32,
    pub pitch_limit: f32,
    pub braking: f32,
    pub mouse_yaw: f32,
    pub mouse_pitch: f32,
    pub turning_speed: f32,
    pub turn_when_stopped: bool,
    pub traction: f32,
    pub friction: f32,
    pub max_slope: f32,
    pub speed_dependant_turning: bool,
    pub mass: i32,
    pub armor: i32,
    pub shields: i32,
    pub shield_recharge_ms: i32,
    pub toughness: f32,
    pub malfunction_armor_level: i32,
    pub surf_destruction: i32,
    pub health_front: i32,
    pub health_back: i32,
    pub health_right: i32,
    pub health_left: i32,
    pub model: Option<Vec<u8>>,
    pub skin: Option<Vec<u8>>,
    pub g2radius: i32,
    /// `riderAnim` (an animation number, -1 for a name not known).
    pub rider_anim: i32,
    pub droid_npc: Option<Vec<u8>>,
    /// `crosshairShader`: the picture cgame draws as the crosshair while riding
    /// (`crosshairShaderHandle`); the game module never uses it.
    pub crosshair_shader: Option<Vec<u8>>,
    pub sound_on: i32,
    pub sound_take_off: i32,
    pub sound_loop: i32,
    pub sound_spin: i32,
    pub sound_turbo: i32,
    pub sound_land: i32,
    pub sound_off: i32,
    pub sound_shifts: [i32; 4],
    pub turbo_start_fx: i32,
    pub explode_fx: i32,
    pub weapons: [VehicleWeaponStats; VEHICLE_WEAPONS],
    /// `weapMuzzle`: the weapon table index each muzzle fires.
    pub muzzle_weapons: [i32; VEHICLE_MUZZLES],
    pub turrets: [TurretStats; VEHICLE_TURRETS],
    pub landing_height: f32,
    pub gravity: i32,
    pub hover_height: f32,
    pub hover_strength: f32,
    pub water_proof: bool,
    pub bouyancy: f32,
    pub fuel_max: i32,
    pub fuel_rate: i32,
    pub turbo_duration: i32,
    pub turbo_recharge: i32,
    pub visibility: i32,
    pub loudness: i32,
    pub explosion_radius: f32,
    pub explosion_damage: i32,
    pub max_passengers: i32,
    pub hide_rider: bool,
    pub kill_rider_on_death: bool,
    pub flammable: bool,
    pub explosion_delay: i32,
    pub camera_override: bool,
    pub camera_range: f32,
    pub camera_vert_offset: f32,
    pub camera_horz_offset: f32,
    pub camera_pitch_offset: f32,
    pub camera_fov: f32,
    pub camera_alpha: f32,
    pub camera_pitch_dependant_vert_offset: bool,
    /// `modelIndex`: `models/players/<model>/model.glm` registered after the parse.
    pub model_index: i32,
}

/// `vehWeaponInfo_t`: a vehicle weapon's definition.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VehicleWeaponInfo {
    pub name: Option<Vec<u8>>,
    pub is_projectile: bool,
    pub has_gravity: bool,
    pub ion_weapon: bool,
    pub saber_blockable: bool,
    /// `iModel`: the projectile's model, which the game registers too.
    pub model: i32,
    pub g2_mark_size: f32,
    pub speed: f32,
    pub homing: f32,
    pub homing_fov: f32,
    pub lock_on_time: i32,
    pub damage: i32,
    pub splash_damage: i32,
    pub splash_radius: f32,
    pub ammo_per_shot: i32,
    pub health: i32,
    pub width: f32,
    pub height: f32,
    pub life_time: i32,
    pub explode_on_expire: bool,
}

/// What a key's value becomes (`vehFieldType_t` as the game module reads it).
pub(crate) enum Slot<'a> {
    /// `VF_INT`: `atoi`.
    Int(&'a mut i32),
    /// `VF_FLOAT`: `atof`.
    Float(&'a mut f32),
    /// `VF_BOOL`: `atof != 0`.
    Bool(&'a mut bool),
    /// `VF_BOOL` written into a float (`throttleSticks`).
    BoolInFloat(&'a mut f32),
    /// `VF_STRING`: kept from the block's first such key.
    Text(&'a mut Option<Vec<u8>>),
    /// `VF_VECTOR`: `sscanf("%f %f %f")`, cleared unless all three are read.
    Vector(&'a mut [f32; 3]),
    /// `VF_VEHTYPE`: a `VehicleTable` name.
    VehicleKind(&'a mut i32),
    /// `VF_ANIM`: an animation name.
    Animation(&'a mut i32),
    /// `VF_WEAPON`: a vehicle weapon, loaded by name.
    Weapon(&'a mut i32),
    /// `VF_MODEL` (and a weapon's `VF_MODEL_CLIENT`): `G_ModelIndex`.
    Model(&'a mut i32),
    /// `VF_EFFECT`: `G_EffectIndex`.
    Effect(&'a mut i32),
    /// `VF_SOUND`: `G_SoundIndex`.
    Sound(&'a mut i32),
    /// A kind only the client registers: known, and nothing is written.
    ClientOnly,
}

/// One key of a field table.
pub(crate) struct Field<T> {
    pub name: &'static str,
    pub slot: fn(&mut T) -> Slot<'_>,
}

macro_rules! fields {
    ($record:ty; $($name:literal => $kind:ident $(($($path:tt)+))?),+ $(,)?) => {
        &[$(Field::<$record> { name: $name, slot: |_record: &mut $record| fields!(@slot _record, $kind $(, $($path)+)?) }),+]
    };
    (@slot $record:ident, ClientOnly) => { Slot::ClientOnly };
    (@slot $record:ident, $kind:ident, $($path:tt)+) => { Slot::$kind(&mut $record.$($path)+) };
}

/// `vehWeaponFields`.
pub(crate) static WEAPON_FIELDS: &[Field<VehicleWeaponInfo>] = fields![VehicleWeaponInfo;
    "name" => Text(name),
    "projectile" => Bool(is_projectile),
    "hasGravity" => Bool(has_gravity),
    "ionWeapon" => Bool(ion_weapon),
    "saberBlockable" => Bool(saber_blockable),
    "muzzleFX" => ClientOnly,
    "model" => Model(model),
    "shotFX" => ClientOnly,
    "impactFX" => ClientOnly,
    "g2MarkShader" => ClientOnly,
    "g2MarkSize" => Float(g2_mark_size),
    "loopSound" => ClientOnly,
    "speed" => Float(speed),
    "homing" => Float(homing),
    "homingFOV" => Float(homing_fov),
    "lockOnTime" => Int(lock_on_time),
    "damage" => Int(damage),
    "splashDamage" => Int(splash_damage),
    "splashRadius" => Float(splash_radius),
    "ammoPerShot" => Int(ammo_per_shot),
    "health" => Int(health),
    "width" => Float(width),
    "height" => Float(height),
    "lifetime" => Int(life_time),
    "explodeOnExpire" => Bool(explode_on_expire),
];

/// `vehicleFields`.
pub(crate) static VEHICLE_FIELDS: &[Field<VehicleInfo>] = fields![VehicleInfo;
    "name" => Text(name),
    "type" => VehicleKind(kind),
    "numHands" => Int(num_hands),
    "lookPitch" => Float(look_pitch),
    "lookYaw" => Float(look_yaw),
    "length" => Float(length),
    "width" => Float(width),
    "height" => Float(height),
    "centerOfGravity" => Vector(center_of_gravity),
    "speedMax" => Float(speed_max),
    "turboSpeed" => Float(turbo_speed),
    "speedMin" => Float(speed_min),
    "speedIdle" => Float(speed_idle),
    "accelIdle" => Float(accel_idle),
    "acceleration" => Float(acceleration),
    "decelIdle" => Float(decel_idle),
    "throttleSticks" => BoolInFloat(throttle_sticks),
    "strafePerc" => Float(strafe_perc),
    "bankingSpeed" => Float(banking_speed),
    "pitchLimit" => Float(pitch_limit),
    "rollLimit" => Float(roll_limit),
    "braking" => Float(braking),
    "mouseYaw" => Float(mouse_yaw),
    "mousePitch" => Float(mouse_pitch),
    "turningSpeed" => Float(turning_speed),
    "turnWhenStopped" => Bool(turn_when_stopped),
    "traction" => Float(traction),
    "friction" => Float(friction),
    "maxSlope" => Float(max_slope),
    "speedDependantTurning" => Bool(speed_dependant_turning),
    "mass" => Int(mass),
    "armor" => Int(armor),
    "shields" => Int(shields),
    "shieldRechargeMS" => Int(shield_recharge_ms),
    "toughness" => Float(toughness),
    "malfunctionArmorLevel" => Int(malfunction_armor_level),
    "surfDestruction" => Int(surf_destruction),
    "model" => Text(model),
    "skin" => Text(skin),
    "g2radius" => Int(g2radius),
    "riderAnim" => Animation(rider_anim),
    "droidNPC" => Text(droid_npc),
    "radarIcon" => ClientOnly,
    "dmgIndicFrame" => ClientOnly,
    "dmgIndicShield" => ClientOnly,
    "dmgIndicBackground" => ClientOnly,
    "icon_front" => ClientOnly,
    "icon_back" => ClientOnly,
    "icon_right" => ClientOnly,
    "icon_left" => ClientOnly,
    "crosshairShader" => Text(crosshair_shader),
    "shieldShader" => ClientOnly,
    "health_front" => Int(health_front),
    "health_back" => Int(health_back),
    "health_right" => Int(health_right),
    "health_left" => Int(health_left),
    "soundOn" => Sound(sound_on),
    "soundOff" => Sound(sound_off),
    "soundLoop" => Sound(sound_loop),
    "soundTakeOff" => Sound(sound_take_off),
    "soundEngineStart" => ClientOnly,
    "soundSpin" => Sound(sound_spin),
    "soundTurbo" => Sound(sound_turbo),
    "soundHyper" => ClientOnly,
    "soundLand" => Sound(sound_land),
    "soundFlyBy" => ClientOnly,
    "soundFlyBy2" => ClientOnly,
    "soundShift1" => Sound(sound_shifts[0]),
    "soundShift2" => Sound(sound_shifts[1]),
    "soundShift3" => Sound(sound_shifts[2]),
    "soundShift4" => Sound(sound_shifts[3]),
    "exhaustFX" => ClientOnly,
    "turboFX" => ClientOnly,
    "turboStartFX" => Effect(turbo_start_fx),
    "trailFX" => ClientOnly,
    "impactFX" => ClientOnly,
    "explodeFX" => Effect(explode_fx),
    "wakeFX" => ClientOnly,
    "dmgFX" => ClientOnly,
    "injureFX" => ClientOnly,
    "noseFX" => ClientOnly,
    "lwingFX" => ClientOnly,
    "rwingFX" => ClientOnly,
    "weap1" => Weapon(weapons[0].id),
    "weap2" => Weapon(weapons[1].id),
    "weap1Delay" => Int(weapons[0].delay),
    "weap2Delay" => Int(weapons[1].delay),
    "weap1Link" => Int(weapons[0].linkable),
    "weap2Link" => Int(weapons[1].linkable),
    "weap1Aim" => Bool(weapons[0].aim_correct),
    "weap2Aim" => Bool(weapons[1].aim_correct),
    "weap1AmmoMax" => Int(weapons[0].ammo_max),
    "weap2AmmoMax" => Int(weapons[1].ammo_max),
    "weap1AmmoRechargeMS" => Int(weapons[0].ammo_recharge_ms),
    "weap2AmmoRechargeMS" => Int(weapons[1].ammo_recharge_ms),
    "weap1SoundNoAmmo" => ClientOnly,
    "weap2SoundNoAmmo" => ClientOnly,
    "weapMuzzle1" => Weapon(muzzle_weapons[0]),
    "weapMuzzle2" => Weapon(muzzle_weapons[1]),
    "weapMuzzle3" => Weapon(muzzle_weapons[2]),
    "weapMuzzle4" => Weapon(muzzle_weapons[3]),
    "weapMuzzle5" => Weapon(muzzle_weapons[4]),
    "weapMuzzle6" => Weapon(muzzle_weapons[5]),
    "weapMuzzle7" => Weapon(muzzle_weapons[6]),
    "weapMuzzle8" => Weapon(muzzle_weapons[7]),
    "weapMuzzle9" => Weapon(muzzle_weapons[8]),
    "weapMuzzle10" => Weapon(muzzle_weapons[9]),
    "landingHeight" => Float(landing_height),
    "gravity" => Int(gravity),
    "hoverHeight" => Float(hover_height),
    "hoverStrength" => Float(hover_strength),
    "waterProof" => Bool(water_proof),
    "bouyancy" => Float(bouyancy),
    "fuelMax" => Int(fuel_max),
    "fuelRate" => Int(fuel_rate),
    "turboDuration" => Int(turbo_duration),
    "turboRecharge" => Int(turbo_recharge),
    "visibility" => Int(visibility),
    "loudness" => Int(loudness),
    "explosionRadius" => Float(explosion_radius),
    "explosionDamage" => Int(explosion_damage),
    "maxPassengers" => Int(max_passengers),
    "hideRider" => Bool(hide_rider),
    "killRiderOnDeath" => Bool(kill_rider_on_death),
    "flammable" => Bool(flammable),
    "explosionDelay" => Int(explosion_delay),
    "cameraOverride" => Bool(camera_override),
    "cameraRange" => Float(camera_range),
    "cameraVertOffset" => Float(camera_vert_offset),
    "cameraHorzOffset" => Float(camera_horz_offset),
    "cameraPitchOffset" => Float(camera_pitch_offset),
    "cameraFOV" => Float(camera_fov),
    "cameraAlpha" => Float(camera_alpha),
    "cameraPitchDependantVertOffset" => Bool(camera_pitch_dependant_vert_offset),
    "turret1Weap" => Weapon(turrets[0].weapon),
    "turret1Delay" => Int(turrets[0].delay),
    "turret1AmmoMax" => Int(turrets[0].ammo_max),
    "turret1AmmoRechargeMS" => Int(turrets[0].ammo_recharge_ms),
    "turret1YawBone" => Text(turrets[0].yaw_bone),
    "turret1PitchBone" => Text(turrets[0].pitch_bone),
    "turret1YawAxis" => Int(turrets[0].yaw_axis),
    "turret1PitchAxis" => Int(turrets[0].pitch_axis),
    "turret1ClampYawL" => Float(turrets[0].yaw_clamp_left),
    "turret1ClampYawR" => Float(turrets[0].yaw_clamp_right),
    "turret1ClampPitchU" => Float(turrets[0].pitch_clamp_up),
    "turret1ClampPitchD" => Float(turrets[0].pitch_clamp_down),
    "turret1Muzzle1" => Int(turrets[0].muzzles[0]),
    "turret1Muzzle2" => Int(turrets[0].muzzles[1]),
    "turret1TurnSpeed" => Float(turrets[0].turn_speed),
    "turret1AI" => Bool(turrets[0].ai),
    "turret1AILead" => Bool(turrets[0].ai_lead),
    "turret1AIRange" => Float(turrets[0].ai_range),
    "turret1PassengerNum" => Int(turrets[0].passenger_num),
    "turret1GunnerViewTag" => Text(turrets[0].gunner_view_tag),
    "turret2Weap" => Weapon(turrets[1].weapon),
    "turret2Delay" => Int(turrets[1].delay),
    "turret2AmmoMax" => Int(turrets[1].ammo_max),
    "turret2AmmoRechargeMS" => Int(turrets[1].ammo_recharge_ms),
    "turret2YawBone" => Text(turrets[1].yaw_bone),
    "turret2PitchBone" => Text(turrets[1].pitch_bone),
    "turret2YawAxis" => Int(turrets[1].yaw_axis),
    "turret2PitchAxis" => Int(turrets[1].pitch_axis),
    "turret2ClampYawL" => Float(turrets[1].yaw_clamp_left),
    "turret2ClampYawR" => Float(turrets[1].yaw_clamp_right),
    "turret2ClampPitchU" => Float(turrets[1].pitch_clamp_up),
    "turret2ClampPitchD" => Float(turrets[1].pitch_clamp_down),
    "turret2Muzzle1" => Int(turrets[1].muzzles[0]),
    "turret2Muzzle2" => Int(turrets[1].muzzles[1]),
    "turret2TurnSpeed" => Float(turrets[1].turn_speed),
    "turret2AI" => Bool(turrets[1].ai),
    "turret2AILead" => Bool(turrets[1].ai_lead),
    "turret2AIRange" => Float(turrets[1].ai_range),
    "turret2PassengerNum" => Int(turrets[1].passenger_num),
    "turret2GunnerViewTag" => Text(turrets[1].gunner_view_tag),
];

/// `Q_LinearSearch` with `vfieldcmp`: the first key of `fields` named `name`, in any case.
pub(crate) fn find<'a, T>(fields: &'a [Field<T>], name: &[u8]) -> Option<&'a Field<T>> {
    fields
        .iter()
        .find(|field| field.name.as_bytes().eq_ignore_ascii_case(name))
}
