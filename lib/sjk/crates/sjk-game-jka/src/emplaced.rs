//! The emplaced gun (OpenJK `codemp/game/g_weapon.c:4660-5076`): the heavy repeater on a
//! seat that ctf2 and the siege maps place (`emplaced_gun`). A player standing behind it,
//! facing its way, takes it with the use key ([`use_gun`], `emplaced_gun_use`): its weapon
//! becomes the gun's (`WP_EMPLACED_GUN`, the old one kept on the gun), it may no longer
//! move ([`crate::pmove_input_freeze`]) and every shot of its movement
//! ([`crate::pmove_emplaced`]) leaves one of the gun's two cannons in turn ([`fire`],
//! `WP_FireEmplaced`, `WP_FireEmplacedMissile`) along its view, held to the gun's arc
//! (`BG_EmplacedView`) and 40 degrees down. The use key again, backing off, walking more
//! than 64 units away or the gun's death give it back ([`update`], `emplaced_gun_update`,
//! every 50 ms) and keep the player off it for a second.
//!
//! Shot to death ([`die`]) it flashes for three seconds, blows up (`G_RadiusDamage` of 80
//! within 128 on everyone near it, the det pack's explosion), smokes for three seconds
//! and — with `CANRESPAWN` — comes back `4000 + count` ms after it died, with 40 % of its
//! health.
//!
//! Its think ([`update_dying`], [`update_gunner`]) draws on the game's generator and
//! raises its effects through [`GunThink`]; its blast is the caller's to deal between the
//! two halves.

use crate::event_entity::EventEntity;
use crate::player_angle_math::{angle_subtract, normalize, vector_angles};
use crate::pmove::MovementTrace;
use crate::pmove::flight::flight_axes;
use crate::weapon_fire::Missile;
use sjk_entity::Entity;
use sjk_protocol::{EntityState, LEGACY_ENTITY_FIELDS, PlayerState};

/// The gun's box (`SP_emplaced_gun`), as its QUAKED comment gives it.
pub const BOUNDS: ([f32; 3], [f32; 3]) = ([-30.0, -20.0, 8.0], [30.0, 20.0, 60.0]);
/// Its model (`G_ModelIndex`).
pub const MODEL: &[u8] = b"models/map_objects/mp/turret_chair.glm";
/// The item it registers (`RegisterItem(BG_FindItemForWeapon(WP_EMPLACED_GUN))`).
pub const ITEM_CLASSNAME: &str = "weapon_emplaced";
/// `EMPLACED_GUN_HEALTH`, and the share of it a gun that respawns has.
const EMPLACED_GUN_HEALTH: i32 = 800;
/// `EMPLACED_CANRESPAWN`, and the "deadsolid" bit it always sets.
const EMPLACED_CANRESPAWN: i32 = 1;
const DEAD_SOLID: i32 = 4;
/// "being caught in this thing when it blows would be really bad".
pub const SPLASH_DAMAGE: i32 = 80;
pub const SPLASH_RADIUS: i32 = 128;
/// `CONTENTS_SOLID`, `SOLID_BBOX`, `SVF_PLAYER_USABLE`, `MASK_SOLID`.
pub const CONTENTS_SOLID: u32 = 1;
const SOLID_BBOX: u32 = 2;
pub const SVF_PLAYER_USABLE: u32 = 0x10;
pub const MASK_SOLID: u32 = 1;
/// `WP_EMPLACED_GUN`, `WP_TURRET`; `NUM_FORCE_POWERS`; `MAX_CLIENTS`.
const WP_EMPLACED_GUN: u32 = 17;
const WP_TURRET: u32 = 18;
const NUM_FORCE_POWERS: u32 = 18;
const MAX_CLIENTS: u32 = 32;
/// `WEAPON_READY`; `STAT_WEAPONS`; `PMF_DUCKED`; `HANDEXTEND_NONE`.
const WEAPON_READY: u32 = 0;
const STAT_WEAPONS: usize = 4;
const PMF_DUCKED: u32 = 1;
const HANDEXTEND_NONE: u32 = 0;
/// `BUTTON_USE`.
const BUTTON_USE: u16 = 32;
/// `EV_FIRE_WEAPON`, `EV_PLAY_EFFECT`; `EFFECT_SMOKE`, `EFFECT_EXPLOSION_DETPACK`; the
/// event counter's bits.
const EV_FIRE_WEAPON: u32 = 27;
const EFFECT_SMOKE: u32 = 1;
const EFFECT_EXPLOSION_DETPACK: u32 = 6;
const EV_EVENT_BIT1: u32 = 0x100;
const EV_EVENT_BITS: u32 = 0x300;
/// `BLASTER_VELOCITY`, `BLASTER_DAMAGE`; `MOD_VEHICLE`; `MASK_SHOT | CONTENTS_LIGHTSABER`.
const BLASTER_VELOCITY: f32 = 2_300.0;
const BLASTER_DAMAGE: i32 = 20;
const MOD_VEHICLE: u32 = 28;
const SHOT_CLIP_MASK: u32 = 0x0004_1301;
/// `DAMAGE_DEATH_KNOCKBACK | DAMAGE_HEAVY_WEAP_CLASS`: the bolt's `dflags`, which only a
/// shield or a heavy-weapons-only target asks about (a `MOD_VEHICLE` passes both anyway).
pub const SHOT_DFLAGS: u32 = 0x0080 | 0x1000;
/// The bolt's life (`CreateMissile(..., 10000, ...)`).
const SHOT_LIFE_MS: i32 = 10_000;

/// Wire fields (`entityState_t`, protocol 26).
mod es {
    pub const POS_BASE: [usize; 3] = [2, 1, 4];
    pub const APOS_BASE: [usize; 3] = [5, 3, 33];
    pub const ANGLES: [usize; 3] = [25, 9, 24];
    pub const ORIGIN: [usize; 3] = [11, 12, 13];
    pub const WEAPON: usize = 14;
    pub const SOLID: usize = 26;
    pub const EVENT: usize = 28;
    pub const G2_RADIUS: usize = 38;
    pub const OWNER: usize = 40;
    pub const EVENT_PARM: usize = 42;
    pub const MODEL_INDEX: usize = 46;
    pub const EMPLACED_OWNER: usize = 47;
    pub const MODEL_GHOUL2: usize = 54;
    pub const ORIGIN2: [usize; 3] = [56, 60, 53];
    pub const SHOULD_TARGET: usize = 57;
    pub const TIME: usize = 65;
    pub const ACTIVE_FORCE_PASS: usize = 68;
    pub const HEALTH: usize = 69;
    pub const MAX_HEALTH: usize = 73;
}

/// Player-state fields the gun changes on its gunner.
mod ps {
    pub const PM_FLAGS: usize = 38;
    pub const WEAPON: usize = 47;
    pub const WEAPON_STATE: usize = 33;
    pub const FORCE_HAND_EXTEND: usize = 80;
    pub const SABER_HOLSTERED: usize = 81;
    pub const EMPLACED_INDEX: usize = 112;
    pub const IS_JEDI_MASTER: usize = 114;
}

/// The teams a siege map gives a gun: whose it looks (`teamowner`), who may use it
/// (`alliedTeam`) and whose damage it takes none of (`teamnodmg`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GunTeams {
    pub owner: i32,
    pub allied: i32,
    pub no_damage: i32,
}

/// An emplaced gun as `SP_emplaced_gun` leaves it and its think keeps it.
#[derive(Clone, Debug, PartialEq)]
pub struct EmplacedGun {
    /// Its entity number, which its gunner's `emplacedIndex` names.
    pub number: u16,
    /// Its wire state (`s`), `solid` as the game sets it (`SOLID_BBOX`; the engine packs
    /// the box into it as it links the gun).
    pub state: EntityState,
    /// `s.origin` = `r.currentOrigin`: where it stands, dropped to the floor.
    pub origin: [f32; 3],
    /// `pos1`: the way it faces, which a taker must face too.
    pub base_angles: [f32; 3],
    pub health: i32,
    pub max_health: i32,
    pub takes_damage: bool,
    /// `count`: with `CANRESPAWN`, the extra milliseconds before it comes back.
    pub count: i32,
    pub spawnflags: i32,
    /// `genericValue1`: the use key that took it is still held (it does not let go).
    pub use_held: bool,
    /// `genericValue2`, `genericValue3`: the next smoke puff, and until when it smokes.
    pub next_smoke: i32,
    pub smoking_until: i32,
    /// `genericValue4`: 1 while its death's warning flashes, 2 once it blew up.
    pub dying: i32,
    /// `genericValue5`: when it comes back (0 until its think notices it died).
    pub respawn_at: i32,
    /// `genericValue10`: 1 when its last shot left the right cannon.
    pub side: i32,
    /// `activator`: its gunner.
    pub activator: Option<u16>,
    /// `nextthink` of its only think (`emplaced_gun_update`).
    pub next_think: i32,
    /// `eventTime`: when its last event was added.
    pub event_time: i32,
    pub teams: GunTeams,
    /// `healingclass`, `healingrate`, `healingsound`, `healingDebounce`.
    pub healing: crate::try_heal::Healable,
}

/// A player as the gun reaches it: its wire state, its `emplacedTime` (a server-only
/// field), `r.ownerNum`, `pers.cmd.buttons`, `s.weapon` and whether it is in use.
pub struct Gunner<'a> {
    pub number: u16,
    pub state: &'a mut PlayerState,
    pub emplaced_time: &'a mut i32,
    pub owner: &'a mut u16,
    pub buttons: u16,
    /// `s.weapon` of its entity (what its last think left there).
    pub entity_weapon: u32,
    pub in_use: bool,
}

/// What a gun's think draws on beside its gunner: the game's generator (`Q_irand`) and
/// where a temp entity goes (`G_PlayEffect`).
pub struct GunThink<'a> {
    pub irand: &'a mut dyn FnMut(i32, i32) -> i32,
    pub raise: &'a mut dyn FnMut(EventEntity),
}

/// The blast a gun's end asks for: `G_RadiusDamage(origin, gun, damage, radius, gun,
/// NULL, MOD_UNKNOWN)`, which the caller deals between the two halves of the think.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blast {
    pub origin: [f32; 3],
    pub damage: i32,
    pub radius: i32,
}

/// What a use of the gun came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Used {
    /// Nothing: dead, taken, too soon, busy, too far, from above, crouched or a Jedi Master.
    Refused,
    /// Taken.
    Taken,
    /// Not faced the right way: `TryHeal` instead.
    TryHeal,
}

/// `SP_emplaced_gun` (`g_weapon.c:4993-5076`) for the map's `entity` given entity number
/// `number` and the model's index: dropped down to what is under it (`drop`: the
/// engine's trace of its box, `MASK_SOLID`, from the origin 1024 units down), 800 of
/// health (320 if it may respawn), the item registered by the caller.
pub fn spawn(
    entity: &Entity,
    number: u16,
    model_index: u16,
    level_time: i32,
    drop: impl FnOnce([f32; 3], [f32; 3], [f32; 3], [f32; 3]) -> MovementTrace,
) -> EmplacedGun {
    let int = |key: &str, default: i32| {
        entity
            .get(key)
            .map_or(default, |text| crate::userinfo::atoi(text.as_bytes()))
    };
    let mut origin = crate::fx_runner::vector(entity, "origin");
    let angles = crate::fx_runner::spawn_angles(entity);
    let mut down = origin;
    down[2] -= 1_024.0;
    let trace = drop(origin, BOUNDS.0, BOUNDS.1, down);
    if trace.fraction != 1.0 && !trace.all_solid && !trace.start_solid {
        origin = trace.end_position;
    }
    let spawnflags = int("spawnflags", 0) | DEAD_SOLID;
    let mut health = EMPLACED_GUN_HEALTH;
    if spawnflags & EMPLACED_CANRESPAWN != 0 {
        // `ent->health *= 0.4`: an int times a double, truncated.
        health = (f64::from(health) * 0.4) as i32;
    }
    let constraint = entity
        .get("constraint")
        .map_or(60.0, |text| crate::text_parse::atof(text.as_bytes()));
    let mut state = EntityState::zero(0, &LEGACY_ENTITY_FIELDS);
    let _ = state.set_number(number);
    state.set_raw_field(es::SOLID, SOLID_BBOX);
    state.set_raw_field(es::ORIGIN2[0], constraint.to_bits());
    state.set_raw_field(es::MODEL_INDEX, u32::from(model_index));
    state.set_raw_field(es::MODEL_GHOUL2, 1);
    state.set_raw_field(es::G2_RADIUS, 110);
    state.set_raw_field(es::WEAPON, WP_EMPLACED_GUN);
    state.set_raw_field(es::OWNER, MAX_CLIENTS + 1);
    state.set_raw_field(es::SHOULD_TARGET, 1);
    for axis in 0..3 {
        // `G_SetOrigin` (base, origin), then the angles and their trajectory's base.
        state.set_raw_field(es::POS_BASE[axis], origin[axis].to_bits());
        state.set_raw_field(es::ORIGIN[axis], origin[axis].to_bits());
        state.set_raw_field(es::ANGLES[axis], angles[axis].to_bits());
        state.set_raw_field(es::APOS_BASE[axis], angles[axis].to_bits());
    }
    let teams = GunTeams {
        owner: int("teamowner", 0),
        allied: last_int(entity, &["teamuser", "alliedteam"]),
        no_damage: int("teamnodmg", 0),
    };
    let mut gun = EmplacedGun {
        number,
        state,
        origin,
        base_angles: angles,
        health,
        max_health: health,
        takes_damage: true,
        count: int("count", 600),
        spawnflags,
        use_held: false,
        next_smoke: 0,
        smoking_until: 0,
        dying: 0,
        respawn_at: 0,
        side: 0,
        activator: None,
        next_think: level_time + 50,
        event_time: 0,
        teams,
        healing: crate::try_heal::Healable::from_entity(entity),
    };
    scale_net_health(&mut gun);
    gun
}

/// The last of `keys` the lump gives, as an integer (`teamuser` and `alliedteam` both
/// write `alliedTeam`).
fn last_int(entity: &Entity, keys: &[&str]) -> i32 {
    entity
        .fields()
        .iter()
        .rev()
        .find(|(key, _)| keys.iter().any(|wanted| key.eq_ignore_ascii_case(wanted)))
        .map_or(0, |(_, value)| crate::userinfo::atoi(value.as_bytes()))
}

/// `G_ScaleNetHealth` (`g_utils.c:1116-1146`): the health bar the gun shows.
fn scale_net_health(gun: &mut EmplacedGun) {
    let (max, health) = if gun.max_health < 1_000 {
        (gun.max_health, gun.health.max(0))
    } else {
        let health = (gun.health / 100).max(0);
        (
            gun.max_health / 100,
            if gun.health > 0 && health <= 0 {
                1
            } else {
                health
            },
        )
    };
    gun.state.set_raw_field(es::MAX_HEALTH, max as u32);
    gun.state.set_raw_field(es::HEALTH, health as u32);
}

/// `G_AddEvent` on the gun: the event with the counter's next bits, shown from now.
fn add_event(gun: &mut EmplacedGun, event: u32, parameter: u32, level_time: i32) {
    let bits = (gun.state.raw_field(es::EVENT).unwrap_or(0) & EV_EVENT_BITS)
        .wrapping_add(EV_EVENT_BIT1)
        & EV_EVENT_BITS;
    gun.state.set_raw_field(es::EVENT, event | bits);
    gun.state.set_raw_field(es::EVENT_PARM, parameter);
    gun.event_time = level_time;
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn length(vector: [f32; 3]) -> f32 {
    dot(vector, vector).sqrt()
}

/// `emplaced_gun_use` (`g_weapon.c:4740-4851`): `user` takes the gun — from within 64
/// units, below its top, standing, facing its way and standing behind it; its weapon
/// swapped for the gun's, its owner the gun.
pub fn use_gun(gun: &mut EmplacedGun, user: Gunner<'_>, level_time: i32) -> Used {
    let state = &*user.state;
    let player_origin = state.origin();
    if gun.health <= 0
        || gun.activator.is_some()
        || *user.emplaced_time > level_time
        || state.raw_field(ps::FORCE_HAND_EXTEND).unwrap_or(0) != HANDEXTEND_NONE
        || player_origin[2] > gun.origin[2] + 50.0 - 8.0
        || state.raw_field(ps::PM_FLAGS).unwrap_or(0) & PMF_DUCKED != 0
        || state.raw_field(ps::IS_JEDI_MASTER).unwrap_or(0) != 0
    {
        return Used::Refused;
    }
    let to_gun: [f32; 3] = std::array::from_fn(|axis| gun.origin[axis] - player_origin[axis]);
    if length(to_gun) > 64.0 {
        return Used::Refused;
    }
    let view = flight_axes(state.view_angles()).0.to_array();
    let facing = flight_axes(gun.base_angles).0.to_array();
    // "Must be reasonably facing the way the gun points (110 degrees or so)."
    if dot(view, facing) < -0.2 {
        return Used::TryHeal;
    }
    let mut toward = to_gun;
    normalize(&mut toward);
    if dot(toward, facing) < 0.6 {
        return Used::TryHeal;
    }
    gun.use_held = true;
    let old_weapon = user.entity_weapon;
    let state = user.state;
    state.set_raw_field(ps::WEAPON, gun.state.raw_field(es::WEAPON).unwrap_or(0));
    state.set_raw_field(ps::WEAPON_STATE, WEAPON_READY);
    state.stats[STAT_WEAPONS] |= 1 << WP_EMPLACED_GUN;
    state.set_raw_field(ps::EMPLACED_INDEX, u32::from(gun.number));
    gun.state
        .set_raw_field(es::EMPLACED_OWNER, u32::from(user.number));
    gun.state
        .set_raw_field(es::ACTIVE_FORCE_PASS, NUM_FORCE_POWERS + 1);
    gun.state.set_raw_field(es::WEAPON, old_weapon);
    *user.owner = gun.number;
    gun.activator = Some(user.number);
    Used::Taken
}

/// `emplaced_gun_pain` (`:4859-4875`): the health bar follows the health.
fn pain(gun: &mut EmplacedGun) {
    gun.state.set_raw_field(es::HEALTH, gun.health as u32);
}

/// `emplaced_gun_die` (`:4979-4991`): the warning begins — three seconds of flashing
/// (`s.time`) before it blows.
pub fn die(gun: &mut EmplacedGun, level_time: i32) {
    if gun.dying != 0 {
        return;
    }
    gun.dying = 1;
    gun.state
        .set_raw_field(es::TIME, (level_time + 3_000) as u32);
    gun.respawn_at = 0;
}

/// `G_Damage` on the gun (`g_combat.c:4400-5550` for a target with no client and no armour):
/// at least one point taken, the health bar scaled, then its pain or its death (a dead
/// gun still takes damage — it never stops taking it — and dies no second time).
/// The teams' rules are the caller's, who knows the attacker. Returns the damage taken.
pub fn damage(gun: &mut EmplacedGun, damage: i32, level_time: i32) -> i32 {
    if !gun.takes_damage {
        return 0;
    }
    let take = damage.max(1);
    gun.health -= take;
    scale_net_health(gun);
    if gun.health <= 0 {
        gun.health = gun.health.max(-999);
        die(gun, level_time);
    } else {
        pain(gun);
    }
    take
}

/// `emplaced_gun_update` (`:4877-4977`), when its `nextthink` has come, up to its blast:
/// its respawn timed and done, and at the end of its warning the det pack's explosion,
/// the smoke's length drawn — and the blast returned for the caller to deal before
/// [`update_gunner`] goes on.
pub fn update_dying(
    gun: &mut EmplacedGun,
    level_time: i32,
    think: &mut GunThink<'_>,
) -> Option<Blast> {
    if gun.health < 1 && gun.respawn_at == 0 {
        if gun.spawnflags & EMPLACED_CANRESPAWN != 0 {
            gun.respawn_at = level_time + 4_000 + gun.count;
        }
    } else if gun.health < 1 && gun.respawn_at < level_time {
        gun.state.set_raw_field(es::TIME, 0);
        gun.dying = 0;
        gun.smoking_until = 0;
        gun.health = (f64::from(EMPLACED_GUN_HEALTH) * 0.4) as i32;
        gun.state.set_raw_field(es::HEALTH, gun.health as u32);
    }
    let time = gun.state.raw_field(es::TIME).unwrap_or(0) as i32;
    if gun.dying == 0 || gun.dying >= 2 || time >= level_time {
        return None;
    }
    // "we have finished our warning (red flashing) effect, it's time to finish dying".
    let mut explosion = gun.origin;
    explosion[2] += 16.0;
    (think.raise)(crate::mines::effect(
        EFFECT_EXPLOSION_DETPACK,
        explosion,
        [0.0, 0.0, 1.0],
    ));
    gun.smoking_until = level_time + (think.irand)(2_500, 3_500);
    gun.state.set_raw_field(es::TIME, -1_i32 as u32);
    gun.dying = 2;
    Some(Blast {
        origin: gun.origin,
        damage: SPLASH_DAMAGE,
        radius: SPLASH_RADIUS,
    })
}

/// The rest of `emplaced_gun_update`: the smoke, and the gunner (`gunner`, the one the
/// gun's `activator` names, `None` when it is gone) — let go of when the use key is
/// pressed again, the gun taken from it, the gun dying or the gunner too far, else held
/// on the gun's weapon. The next think in 50 ms.
pub fn update_gunner(
    gun: &mut EmplacedGun,
    level_time: i32,
    think: &mut GunThink<'_>,
    gunner: Option<Gunner<'_>>,
) {
    if gun.smoking_until > level_time && gun.next_smoke < level_time {
        let mut smoke = gun.origin;
        smoke[2] += 60.0;
        (think.raise)(crate::mines::effect(EFFECT_SMOKE, smoke, [0.0, 0.0, 1.0]));
        gun.next_smoke = level_time + (think.irand)(250, 400);
    }
    gun.next_think = level_time + 50;
    if gun.activator.is_none() {
        return;
    }
    let gunner = gunner.filter(|gunner| gunner.in_use);
    let mut distance = 0.0;
    if let Some(gunner) = &gunner {
        let origin = gunner.state.origin();
        distance = length(std::array::from_fn(|axis| gun.origin[axis] - origin[axis]));
        if gunner.buttons & BUTTON_USE == 0 && gun.use_held {
            gun.use_held = false;
        }
    }
    if let Some(gunner) = gunner {
        if gunner.buttons & BUTTON_USE != 0 && !gun.use_held {
            gunner.state.set_raw_field(ps::EMPLACED_INDEX, 0);
            gunner.state.set_raw_field(ps::SABER_HOLSTERED, 0);
            return;
        }
        if gunner.state.raw_field(ps::EMPLACED_INDEX) != Some(u32::from(gun.number))
            || gun.dying != 0
            || distance > 64.0
        {
            // "get the user off of me then".
            gunner.state.stats[STAT_WEAPONS] &= !(1 << WP_EMPLACED_GUN);
            let held = gunner.state.raw_field(ps::WEAPON).unwrap_or(0);
            gunner
                .state
                .set_raw_field(ps::WEAPON, gun.state.raw_field(es::WEAPON).unwrap_or(0));
            gun.state.set_raw_field(es::WEAPON, held);
            *gunner.owner = sjk_protocol::ENTITY_NUMBER_NONE;
            *gunner.emplaced_time = level_time + 1_000;
            gunner.state.set_raw_field(ps::EMPLACED_INDEX, 0);
            gunner.state.set_raw_field(ps::SABER_HOLSTERED, 0);
            gun.activator = None;
            gun.state.set_raw_field(es::ACTIVE_FORCE_PASS, 0);
        } else {
            gunner.state.set_raw_field(ps::WEAPON, WP_EMPLACED_GUN);
            gunner.state.set_raw_field(ps::WEAPON_STATE, WEAPON_READY);
        }
    } else {
        // A gunner gone from the game lets go too (the reference writes to the client it
        // had; nothing of it is sent any more).
        gun.activator = None;
        gun.state.set_raw_field(es::ACTIVE_FORCE_PASS, 0);
    }
}

/// `BG_EmplacedView` (`bg_misc.c:2578-2618`): how far `view` turns past the gun's arc of
/// `constraint` either side of `angles`' yaw — 0 within it; otherwise the yaw held at the
/// arc's edge, and 2 when more than a degree past it (the client forces the view back),
/// 1 when just past.
pub fn emplaced_view(view: [f32; 3], angles: [f32; 3], constraint: f32) -> (i32, f32) {
    let mut difference = angle_subtract(view[1], angles[1]);
    if difference > constraint || difference < -constraint {
        let amount = if difference > constraint {
            let amount = difference - constraint;
            difference = constraint;
            amount
        } else {
            let amount = difference + constraint;
            difference = -constraint;
            amount
        };
        let yaw = angle_subtract(angles[1], -difference);
        return (
            if !(-1.0..=1.0).contains(&amount) {
                2
            } else {
                1
            },
            yaw,
        );
    }
    (0, 0.0)
}

/// `FireWeapon`'s aim for a gunner (`g_weapon.c:4509-4538`) and `WP_FireEmplaced`
/// (`:4660-4712`) with `WP_FireEmplacedMissile` (`:492-519`): from 46 units above the gun,
/// ten either side of it along the gunner's right (the cannons in turn, the gun's
/// `EV_FIRE_WEAPON` naming the side), along the view pitched no lower than 40 and turned
/// no further than the gun's arc; a blaster bolt of 20 at 2300 drawn as the turret's,
/// the gun's own, passing its gunner and credited to it (`activator`), `MOD_VEHICLE`.
/// `None` for a dead gun.
pub fn fire(
    gun: &mut EmplacedGun,
    gunner: u16,
    view_angles: [f32; 3],
    level_time: i32,
    alternate: bool,
) -> Option<Missile> {
    if gun.health <= 0 {
        return None;
    }
    let mut capped = view_angles;
    capped[0] = capped[0].min(40.0);
    let angles: [f32; 3] = std::array::from_fn(|axis| {
        f32::from_bits(gun.state.raw_field(es::ANGLES[axis]).unwrap_or(0))
    });
    let constraint = f32::from_bits(gun.state.raw_field(es::ORIGIN2[0]).unwrap_or(0));
    let (held, yaw) = emplaced_view(view_angles, angles, constraint);
    if held != 0 {
        capped[1] = yaw;
    }
    let forward = flight_axes(capped).0.to_array();
    let right = flight_axes(view_angles).1.to_array();
    let mut muzzle = gun.origin;
    muzzle[2] += 46.0;
    let (offset, side) = if gun.side != 0 { (10.0, 0) } else { (-10.0, 1) };
    muzzle = std::array::from_fn(|axis| muzzle[axis] + offset * right[axis]);
    gun.side = side;
    add_event(gun, EV_FIRE_WEAPON, side as u32, level_time);
    let direction = flight_axes(vector_angles(forward)).0.to_array();
    let muzzle = crate::weapon_fire::snap_vector(muzzle);
    let mut missile = crate::weapon_fire::create_missile_by(
        gun.number,
        muzzle,
        direction,
        BLASTER_VELOCITY,
        level_time,
        alternate,
    );
    missile.free_at = level_time + SHOT_LIFE_MS;
    missile.state.set_raw_field(es::WEAPON, WP_TURRET);
    missile.activator = Some(gunner);
    missile.pass_through = Some(gunner);
    missile.damage = BLASTER_DAMAGE;
    missile.method_of_death = MOD_VEHICLE;
    missile.clip_mask = SHOT_CLIP_MASK;
    missile.bounce_count = 8;
    Some(missile)
}
