//! Trip mines (laser traps) and det packs (OpenJK `codemp/game/g_weapon.c:2318-3078`,
//! with `g_object.c` and `g_items.c::G_RunItem`): general entities thrown under gravity
//! that stick to what they land on — a trip mine armed a second later with its beam
//! (`laserTrapThink`, a trace along the surface's normal every 100 ms that blows it 50
//! ms after a player breaks it), or two seconds later as a proximity mine
//! (`proxMineThink`, a player within half the splash radius); a det pack stuck to the
//! world (`charge_stick`) until its owner's alternate fire blows it (`BlowDetpacks`,
//! 100 to 300 ms on) or thirty seconds pass. Both are shootable and blow when hurt.

use crate::crt_rand::CrtRand;
use crate::damage::{Attacker, DamageRequest, vector_to_angles};
use crate::event_entity::EventEntity;
use crate::player_death::Rng;
use crate::pmove::{MovementCollision, MovementTrace};
use crate::weapon_fire::{Missile, add_event, run_object, set_origin, snap_vector, sound_event};
use sjk_protocol::{EntityState, LEGACY_ENTITY_FIELDS, PlayerState};

/// `LT_DAMAGE`, `LT_SPLASH_RAD`, `LT_SPLASH_DAM`, `LT_SIZE`, `LT_ALT_TIME`,
/// `LT_ACTIVATION_DELAY`, `LT_DELAY_TIME`; the det pack's hundred, two hundred over two
/// hundred, its box of two, its thirty seconds; `FRAMETIME`.
const LT_DAMAGE: i32 = 100;
const LT_SPLASH_RAD: f32 = 256.0;
const LT_SPLASH_DAM: i32 = 105;
const LT_SIZE: f32 = 1.5;
const LT_ALT_TIME: i32 = 2_000;
const LT_ACTIVATION_DELAY: i32 = 1_000;
const LT_DELAY_TIME: i32 = 50;
const LT_PROX_LIFE: i32 = 30_000;
const DET_DAMAGE: i32 = 100;
const DET_SPLASH: i32 = 200;
const DET_SPLASH_RADIUS: f32 = 200.0;
const DET_SIZE: f32 = 2.0;
const DET_LIFE: i32 = 30_000;
const FRAMETIME: i32 = 100;
const BEAM_RANGE: f32 = 1_024.0;
/// `MOD_TRIP_MINE_SPLASH`, `MOD_DET_PACK_SPLASH`; `WP_TRIP_MINE`, `WP_DET_PACK`.
pub const MOD_TRIP_MINE_SPLASH: u32 = 25;
pub const MOD_DET_PACK_SPLASH: u32 = 27;
const WP_TRIP_MINE: u32 = 13;
const WP_DET_PACK: u32 = 14;
/// `MASK_SHOT`, `ENTITYNUM_WORLD`, `ENTITYNUM_NONE`, `MAX_GENTITIES`, `MAX_CLIENTS`.
const MASK_SHOT: u32 = 0x1 | 0x100 | 0x200 | 0x1000;
const ENTITY_WORLD: u16 = 1_022;
const ENTITY_NONE: u16 = 1_023;
const MAX_GENTITIES: u32 = 1_024;
const MAX_CLIENTS: u16 = 32;
/// `EF_MISSILE_STICK`, `EF_FIRING`; `EV_MISSILE_MISS`, `EV_PLAY_EFFECT`,
/// `EFFECT_EXPLOSION_TRIPMINE`, `EFFECT_EXPLOSION_DETPACK`; `CHAN_WEAPON`, `CHAN_BODY`.
pub(crate) const EF_MISSILE_STICK: u32 = 1 << 22;
const EF_FIRING: u32 = 1 << 9;
const EV_MISSILE_MISS: u32 = 86;
const EV_PLAY_EFFECT: u32 = 68;
const EFFECT_EXPLOSION_TRIPMINE: u32 = 5;
const EFFECT_EXPLOSION_DETPACK: u32 = 6;
const CHAN_WEAPON: u32 = 2;
const CHAN_BODY: u32 = 6;
/// The wire fields.
const ES_POS_TIME: usize = 0;
const ES_POS_BASE: [usize; 3] = [2, 1, 4];
const ES_POS_DELTA: [usize; 3] = [6, 7, 10];
const ES_POS_TYPE: usize = 23;
const ES_APOS_TYPE: usize = 15;
const ES_APOS_TIME: usize = 34;
const ES_APOS_BASE: [usize; 3] = [5, 3, 33];
const ES_APOS_DELTA: [usize; 3] = [48, 44, 49];
const ES_WEAPON: usize = 14;
const ES_EFLAGS: usize = 19;
const ES_GENERIC_ENEMY: usize = 18;
const ES_GROUND_ENTITY: usize = 22;
const ES_SOLID: usize = 26;
const ES_G2_RADIUS: usize = 38;
const ES_MODEL: usize = 46;
const ES_MODEL_GHOUL2: usize = 54;
const ES_ORIGIN: [usize; 3] = [11, 12, 13];
const ES_ANGLES: [usize; 3] = [25, 9, 24];
const ES_BOLT2: usize = 63;
const ES_TIME: usize = 65;
const TR_STATIONARY: u32 = 0;
const TR_GRAVITY: u32 = 6;
const PS_HAS_DETPACK_PLANTED: usize = 87;

/// What a charge is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A trip mine (`count` 1): its beam along the surface's normal.
    TripMine,
    /// The alternate: a proximity mine.
    ProximityMine,
    /// A det pack.
    DetPack,
}

/// Where a charge is in its life: its `think` and what `touch` does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    /// Thrown: `TrapThink` (a mine, every 50 ms) or `G_RunObject` (a det pack, every
    /// 100 ms — `G_RunItem` moves it every frame besides).
    Flying,
    /// A mine stuck, its beam to be armed at the time (`laserTrapThink`).
    Set,
    /// A trip mine's beam, traced every 100 ms.
    Beam,
    /// A proximity mine, watching every frame; blown at `auto_at` at the latest.
    Watching { auto_at: i32 },
    /// A det pack stuck (`DetPackBlow` at the time), or primed to blow by its owner or
    /// by a hit.
    Planted,
    /// `laserTrapExplode` at the time.
    Exploding,
    /// `G_FreeEntity` at the time.
    Freeing,
}

/// A trip mine, a proximity mine or a det pack as the server keeps it.
#[derive(Clone, Debug, PartialEq)]
pub struct Charge {
    pub kind: Kind,
    /// The entity: its wire state, box, origin, owner, damage, splash.
    pub missile: Missile,
    /// `r.currentAngles`.
    pub angles: [f32; 3],
    pub phase: Phase,
    /// `nextthink`.
    pub next_think: i32,
    /// `setTime`: when it was placed, for the limit of ten.
    pub set_time: i32,
    /// `health`, `takedamage`.
    pub health: i32,
    pub takes_damage: bool,
    /// `pos1`/`movedir` (a mine's surface normal) or `pos2` (a det pack's).
    pub normal: [f32; 3],
    /// `count`: 1 for a trip mine, -1 for a stuck det pack.
    pub count: i32,
    /// `touch` still set.
    pub touches: bool,
    /// `r.linked`.
    pub linked: bool,
    /// Its entity number, once the pool has given it one.
    pub number: u16,
    /// `SVF_OWNERNOTSHARED`, set once stuck: its owner runs into it and can shoot it.
    pub owner_not_shared: bool,
}

/// The wire fields of a charge, as the transcript prints them (`s.time` and the rest
/// are on the state itself).
impl Charge {
    /// The state, numbered.
    pub fn state(&self, number: u16) -> EntityState {
        let mut state = self.missile.state.clone();
        let _ = state.set_number(number);
        state
    }

    /// The transcript's name for the think.
    pub fn think_name(&self) -> &'static str {
        match (self.kind, self.phase) {
            (_, Phase::Freeing) => "free",
            (_, Phase::Exploding) => "explode",
            (Kind::DetPack, Phase::Flying) => "object",
            (_, Phase::Flying) => "trap",
            (_, Phase::Set) | (_, Phase::Beam) => "beam",
            (_, Phase::Watching { .. }) => "prox",
            (_, Phase::Planted) => "blow",
        }
    }

    /// `r.absmin`, `r.absmax`: the linked box grown by a unit.
    pub fn linked_bounds(&self) -> ([f32; 3], [f32; 3]) {
        (
            std::array::from_fn(|axis| {
                self.missile.current[axis] + self.missile.bounds.0[axis] - 1.0
            }),
            std::array::from_fn(|axis| {
                self.missile.current[axis] + self.missile.bounds.1[axis] + 1.0
            }),
        )
    }
}

/// The apos of a thrown charge: `rand() % 360` for yaw, pitch and roll, the yaw negated
/// on `rand() % 10 < 5` — the CRT's generator, drawn in this order.
fn random_angles(rand: &mut CrtRand) -> [f32; 3] {
    let yaw = (rand.next() % 360) as f32;
    let pitch = (rand.next() % 360) as f32;
    let roll = (rand.next() % 360) as f32;
    let yaw = if rand.next() % 10 < 5 { -yaw } else { yaw };
    [pitch, yaw, roll]
}

fn write_vector(state: &mut EntityState, fields: [usize; 3], vector: [f32; 3]) {
    for axis in 0..3 {
        state.set_raw_field(fields[axis], vector[axis].to_bits());
    }
}

fn read_vector(state: &EntityState, fields: [usize; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| f32::from_bits(state.raw_field(fields[axis]).unwrap_or(0)))
}

/// `WP_PlaceLaserTrap` (`:2616-2712`) less the limit of ten (the caller's, [`oldest`]):
/// `CreateLaserTrap` at the muzzle — a general entity bouncing by half and sticking,
/// `MASK_SHOT` both ways, a box of 1.5, the trap model, its owner as the generic enemy,
/// the trajectory under gravity at 256 along the view (512 for the alternate, which is
/// a proximity mine) from the snapped muzzle, the apos drawn from the CRT generator,
/// its think 50 ms on. `model` is the trap model's index (`G_ModelIndex`).
pub fn place_laser_trap(
    state: &PlayerState,
    muzzle: [f32; 3],
    forward: [f32; 3],
    alternate: bool,
    model: u16,
    level_time: i32,
    rand: &mut CrtRand,
) -> Charge {
    let mut entity = EntityState::zero(0, &LEGACY_ENTITY_FIELDS);
    entity.set_raw_field(ES_EFLAGS, EF_MISSILE_STICK);
    entity.set_raw_field(ES_WEAPON, WP_TRIP_MINE);
    entity.set_raw_field(ES_POS_TYPE, TR_GRAVITY);
    entity.set_raw_field(ES_SOLID, 2);
    entity.set_raw_field(ES_MODEL, u32::from(model));
    entity.set_raw_field(ES_MODEL_GHOUL2, 1);
    entity.set_raw_field(ES_G2_RADIUS, 40);
    entity.set_raw_field(
        ES_GENERIC_ENEMY,
        u32::from(state.client_num()) + MAX_GENTITIES,
    );
    entity.set_raw_field(ES_POS_TIME, level_time as u32);
    let start = snap_vector(muzzle);
    write_vector(&mut entity, ES_POS_BASE, start);
    entity.set_raw_field(ES_APOS_TYPE, TR_GRAVITY);
    entity.set_raw_field(ES_APOS_TIME, level_time as u32);
    write_vector(&mut entity, ES_APOS_BASE, random_angles(rand));
    let speed = if alternate { 512.0 } else { 256.0 };
    write_vector(&mut entity, ES_POS_DELTA, forward.map(|axis| axis * speed));
    let missile = Missile {
        state: entity,
        current: muzzle,
        bounds: ([-LT_SIZE; 3], [LT_SIZE; 3]),
        owner: state.client_num(),
        clip_mask: MASK_SHOT,
        damage: LT_DAMAGE,
        method_of_death: MOD_TRIP_MINE_SPLASH,
        free_at: i32::MAX,
        impact_velocity: [0.0; 3],
        impact_point: [0.0; 3],
        linked: true,
        bounces: false,
        bounce_count: 0,
        event_time: 0,
        damage_flags: 0,
        splash_damage: LT_SPLASH_DAM,
        splash_radius: LT_SPLASH_RAD,
        splash_method_of_death: MOD_TRIP_MINE_SPLASH,
        contents: MASK_SHOT,
        homing: None,
        bounce_half: true,
        bounce_shrapnel: false,
        blows: false,
        dead_saber: None,
        thermal: None,
        broadcast: false,
        explodes: false,
        parent: None,
        pass_through: None,
        activator: None,
    };
    Charge {
        kind: if alternate {
            Kind::ProximityMine
        } else {
            Kind::TripMine
        },
        missile,
        angles: [0.0; 3],
        phase: Phase::Flying,
        next_think: level_time + 50,
        set_time: level_time,
        health: 1,
        takes_damage: false,
        normal: [0.0; 3],
        count: i32::from(!alternate),
        touches: true,
        linked: true,
        number: 0,
        owner_not_shared: false,
    }
}

/// `drop_charge` (`:2876-2934`) from `WP_DropDetPack`: the muzzle four back along the
/// view, a general physics object with a box of two, `MASK_SHOT` both ways, a hundred
/// and two hundred of splash over two hundred, shootable at one health, under gravity
/// at 300 along the view with the apos drawn (and then overwritten with the view's
/// angles, spinning at 300 a second), its think `G_RunObject` 100 ms on.
pub fn drop_det_pack(
    state: &PlayerState,
    muzzle: [f32; 3],
    forward: [f32; 3],
    model: u16,
    level_time: i32,
    rand: &mut CrtRand,
) -> Charge {
    let start: [f32; 3] = std::array::from_fn(|axis| muzzle[axis] - 4.0 * forward[axis]);
    let mut entity = EntityState::zero(0, &LEGACY_ENTITY_FIELDS);
    entity.set_raw_field(ES_G2_RADIUS, 100);
    entity.set_raw_field(ES_MODEL_GHOUL2, 1);
    entity.set_raw_field(ES_MODEL, u32::from(model));
    entity.set_raw_field(ES_SOLID, 2);
    entity.set_raw_field(
        ES_GENERIC_ENEMY,
        u32::from(state.client_num()) + MAX_GENTITIES,
    );
    entity.set_raw_field(ES_WEAPON, WP_DET_PACK);
    // `G_SetOrigin`, then the trajectory.
    write_vector(&mut entity, ES_POS_BASE, start);
    entity.set_raw_field(ES_POS_TYPE, TR_GRAVITY);
    write_vector(&mut entity, ES_POS_DELTA, forward.map(|axis| axis * 300.0));
    entity.set_raw_field(ES_POS_TIME, level_time as u32);
    entity.set_raw_field(ES_APOS_TYPE, TR_GRAVITY);
    entity.set_raw_field(ES_APOS_TIME, level_time as u32);
    let _ = random_angles(rand);
    let (pitch, yaw) = vector_to_angles(forward);
    let angles = [pitch, yaw, 0.0];
    write_vector(&mut entity, ES_ANGLES, angles);
    write_vector(&mut entity, ES_APOS_BASE, angles);
    write_vector(&mut entity, ES_APOS_DELTA, [300.0, 0.0, 0.0]);
    let missile = Missile {
        state: entity,
        current: start,
        bounds: ([-DET_SIZE; 3], [DET_SIZE; 3]),
        owner: state.client_num(),
        clip_mask: MASK_SHOT,
        damage: DET_DAMAGE,
        method_of_death: MOD_DET_PACK_SPLASH,
        free_at: i32::MAX,
        impact_velocity: [0.0; 3],
        impact_point: [0.0; 3],
        linked: true,
        bounces: false,
        bounce_count: 0,
        event_time: 0,
        damage_flags: 0,
        splash_damage: DET_SPLASH,
        splash_radius: DET_SPLASH_RADIUS,
        splash_method_of_death: MOD_DET_PACK_SPLASH,
        contents: MASK_SHOT,
        homing: None,
        bounce_half: false,
        bounce_shrapnel: false,
        blows: false,
        dead_saber: None,
        thermal: None,
        broadcast: false,
        explodes: false,
        parent: None,
        pass_through: None,
        activator: None,
    };
    Charge {
        kind: Kind::DetPack,
        missile,
        angles: [0.0; 3],
        phase: Phase::Flying,
        next_think: level_time + FRAMETIME,
        set_time: level_time,
        health: 1,
        takes_damage: true,
        normal: [0.0; 3],
        count: 0,
        touches: true,
        linked: true,
        number: 0,
        owner_not_shared: false,
    }
}

/// The limit of ten a player may have out (`WP_PlaceLaserTrap`, `WP_DropDetPack`): with
/// ten already, the oldest is freed — the index in `charges` of the one to free, if any.
/// (`CheatsOn` spares det packs the limit.)
pub fn oldest<K>(
    charges: &[(K, Charge)],
    owner: u16,
    kind_is_det_pack: bool,
    level_time: i32,
) -> Option<usize> {
    let mine: Vec<usize> = charges
        .iter()
        .enumerate()
        .filter(|(_, (_, charge))| {
            charge.missile.owner == owner && (charge.kind == Kind::DetPack) == kind_is_det_pack
        })
        .map(|(index, _)| index)
        .collect();
    if mine.len() <= 9 {
        return None;
    }
    let mut lowest = level_time;
    let mut found = None;
    for index in mine {
        if charges[index].1.set_time < lowest {
            lowest = charges[index].1.set_time;
            found = Some(index);
        }
    }
    found
}

/// `BlowDetpacks`: with `hasDetPackPlanted`, every det pack of `owner`'s is primed to
/// blow 100 to 300 ms on (`Q_flrand`) with the warning on its body channel, and the flag
/// cleared. Returns whether any were.
pub fn blow_det_packs<K>(
    state: &mut PlayerState,
    charges: &mut [(K, Charge)],
    level_time: i32,
    rng: &mut Rng,
    frame: &mut ChargeFrame,
) -> bool {
    if state.raw_field(PS_HAS_DETPACK_PLANTED).unwrap_or(0) == 0 {
        return false;
    }
    for (_, charge) in charges.iter_mut() {
        if charge.kind != Kind::DetPack || charge.missile.owner != state.client_num() {
            continue;
        }
        write_vector(&mut charge.missile.state, ES_ORIGIN, charge.missile.current);
        charge.phase = Phase::Planted;
        charge.next_think = level_time + 100 + (rng.flrand(0.0, 1.0) * 200.0) as i32;
        let index = (frame.sounds)(b"sound/weapons/detpack/warning.wav");
        (frame.raise)(sound_event(charge.missile.current, CHAN_BODY, index));
    }
    state.set_raw_field(PS_HAS_DETPACK_PLANTED, 0);
    true
}

/// What a frame's run needs beyond the world: the sound table and where a temp entity
/// goes.
pub struct ChargeFrame<'a> {
    pub sounds: &'a mut dyn FnMut(&[u8]) -> u16,
    pub raise: &'a mut dyn FnMut(EventEntity),
}

/// A player as a proximity mine sees it (`proxMineThink`): connected, playing, alive,
/// an enemy (or friendly fire on).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Watched {
    pub number: u16,
    pub origin: [f32; 3],
}

/// A frame's outcome for a charge.
#[derive(Clone, Debug, PartialEq)]
pub enum ChargeRun {
    /// Nothing observable.
    Quiet,
    /// It moved or stuck: its wire state is to be published.
    Moved,
    /// It blew: the splash is the caller's (`G_RadiusDamage` from `origin` by the owner,
    /// the charge itself ignored, with its damage, radius and means), then the effect
    /// here raised; the entity is freed next frame.
    Blown {
        origin: [f32; 3],
        effect: EventEntity,
    },
    /// `G_FreeEntity`: gone.
    Freed,
}

/// `laserTrapStick` (`:2485-2558`): stood where it landed (`G_SetOrigin`, then the
/// trajectory's delta made the normal and its time now), turned to face along the
/// normal, the stick sound; a trip mine's beam armed a second on, its box doubled, five
/// of health; a proximity mine watching two seconds on, armed at once with the warning.
fn laser_trap_stick(
    charge: &mut Charge,
    end: [f32; 3],
    normal: [f32; 3],
    level_time: i32,
    frame: &mut ChargeFrame,
) {
    set_origin(&mut charge.missile, end);
    charge.normal = normal;
    let state = &mut charge.missile.state;
    write_vector(state, ES_APOS_DELTA, [0.0; 3]);
    write_vector(state, ES_POS_DELTA, normal);
    state.set_raw_field(ES_POS_TIME, level_time as u32);
    let (pitch, yaw) = vector_to_angles(normal);
    let angles = [pitch, yaw, 0.0];
    write_vector(state, ES_APOS_BASE, angles);
    state.set_raw_field(ES_APOS_TYPE, TR_STATIONARY);
    write_vector(state, ES_ANGLES, angles);
    charge.angles = angles;
    let index = (frame.sounds)(b"sound/weapons/laser_trap/stick.wav");
    (frame.raise)(sound_event(charge.missile.current, CHAN_WEAPON, index));
    charge.takes_damage = true;
    charge.health = 5;
    charge.missile.bounds = ([-LT_SIZE * 2.0; 3], [LT_SIZE * 2.0; 3]);
    charge.owner_not_shared = true;
    charge.touches = false;
    if charge.count != 0 {
        charge.phase = Phase::Set;
        charge.next_think = level_time + LT_ACTIVATION_DELAY;
    } else {
        charge.phase = Phase::Watching {
            auto_at: level_time + LT_PROX_LIFE,
        };
        charge.next_think = level_time + LT_ALT_TIME;
        charge.touches = true;
        arm(charge, level_time, frame);
        charge.missile.state.set_raw_field(ES_BOLT2, 1);
    }
}

/// The warning, `EF_FIRING` and `s.time` -1 (the beam drawn), once.
fn arm(charge: &mut Charge, _level_time: i32, frame: &mut ChargeFrame) {
    let flags = charge.missile.state.raw_field(ES_EFLAGS).unwrap_or(0);
    if flags & EF_FIRING == 0 {
        let index = (frame.sounds)(b"sound/weapons/laser_trap/warning.wav");
        (frame.raise)(sound_event(charge.missile.current, CHAN_WEAPON, index));
        charge
            .missile
            .state
            .set_raw_field(ES_EFLAGS, flags | EF_FIRING);
        charge.missile.state.set_raw_field(ES_TIME, (-1_i32) as u32);
    }
}

/// `touchLaserTrap` (`:2379-2398`): an entity in the way (not the owner) blows the mine
/// 100 ms on, the normal kept as its delta; the world sticks it.
fn touch_laser_trap(
    charge: &mut Charge,
    trace: &MovementTrace,
    level_time: i32,
    frame: &mut ChargeFrame,
) {
    if trace.entity_number < ENTITY_WORLD {
        if trace.entity_number != charge.missile.owner {
            charge.touches = false;
            charge.phase = Phase::Exploding;
            charge.next_think = level_time + FRAMETIME;
            write_vector(&mut charge.missile.state, ES_POS_DELTA, trace.plane_normal);
        }
    } else {
        charge.touches = false;
        laser_trap_stick(
            charge,
            trace.end_position,
            trace.plane_normal,
            level_time,
            frame,
        );
    }
}

/// `charge_stick` (`:2728-2833`): off a player (or anything without a weapon) it
/// bounces — its delta pushed along the normal by a random tenth to whole of its own
/// size on each axis — and stays touchable; off another projectile it blows at once
/// (not here: nothing else flies through a det pack); on the world it sticks: primed to
/// blow thirty seconds on (unless already primed), stationary where it is, turned along
/// the normal, `count` -1, the stick sound and an `EV_MISSILE_MISS` temp entity (its
/// owner off the wire).
fn charge_stick(
    charge: &mut Charge,
    trace: &MovementTrace,
    level_time: i32,
    rng: &mut Rng,
    frame: &mut ChargeFrame,
) {
    if trace.entity_number < ENTITY_WORLD {
        let normal = trace.plane_normal;
        let delta = read_vector(&charge.missile.state, ES_POS_DELTA);
        let magnitude = delta.map(f32::abs);
        let mut pushed = delta;
        for axis in 0..3 {
            pushed[axis] += normal[axis] * (magnitude[axis] * (rng.irand(1, 10) as f32 * 0.1));
        }
        write_vector(&mut charge.missile.state, ES_POS_DELTA, pushed);
        let (pitch, yaw) = vector_to_angles(normal);
        write_vector(&mut charge.missile.state, ES_ANGLES, [pitch, yaw, 0.0]);
        write_vector(&mut charge.missile.state, ES_APOS_BASE, [pitch, yaw, 0.0]);
        return;
    }
    if charge.phase == Phase::Flying {
        charge.touches = false;
        charge.phase = Phase::Planted;
        charge.next_think = level_time + DET_LIFE;
    }
    let current = charge.missile.current;
    let state = &mut charge.missile.state;
    write_vector(state, ES_APOS_DELTA, [0.0; 3]);
    state.set_raw_field(ES_APOS_TYPE, TR_STATIONARY);
    state.set_raw_field(ES_POS_TYPE, TR_STATIONARY);
    write_vector(state, ES_ORIGIN, current);
    write_vector(state, ES_POS_BASE, current);
    write_vector(state, ES_POS_DELTA, [0.0; 3]);
    let (pitch, yaw) = vector_to_angles(trace.plane_normal);
    let angles = [pitch, yaw, 0.0];
    write_vector(state, ES_ANGLES, angles);
    charge.angles = angles;
    write_vector(state, ES_APOS_BASE, angles);
    charge.normal = trace.plane_normal;
    charge.count = -1;
    let index = (frame.sounds)(b"sound/weapons/detpack/stick.wav");
    (frame.raise)(sound_event(current, CHAN_WEAPON, index));
    charge.owner_not_shared = true;
    // (`tent->r.ownerNum`, the det pack, is not on the wire.)
    (frame.raise)(EventEntity {
        event: EV_MISSILE_MISS,
        parameter: 0,
        origin: current,
        client: None,
        broadcast: false,
        extra: [(0, 0); 12],
    });
}

/// `G_PlayEffect`: the effect's temp entity at `origin` with `direction` as its angles.
pub(crate) fn effect(id: u32, origin: [f32; 3], direction: [f32; 3]) -> EventEntity {
    let mut event = EventEntity {
        event: EV_PLAY_EFFECT,
        parameter: id,
        origin,
        client: None,
        broadcast: false,
        extra: [(0, 0); 12],
    };
    for axis in 0..3 {
        event.extra[axis] = (ES_ANGLES[axis], direction[axis].to_bits());
        event.extra[3 + axis] = (ES_ORIGIN[axis], origin[axis].to_bits());
    }
    event
}

/// `laserTrapExplode` (`:2327-2363`): no more damage taken; the splash by the owner
/// (the caller's); `EV_MISSILE_MISS` on the mine itself; the trip mine effect with the
/// delta as its direction (none where `s.time` is -2); freed next frame.
fn laser_trap_explode(charge: &mut Charge, level_time: i32) -> ChargeRun {
    charge.takes_damage = false;
    add_event(&mut charge.missile, EV_MISSILE_MISS, 0, level_time);
    let mut direction = read_vector(&charge.missile.state, ES_POS_DELTA);
    if charge.missile.state.raw_field(ES_TIME) == Some((-2_i32) as u32) {
        direction = [0.0; 3];
    }
    charge.phase = Phase::Freeing;
    charge.next_think = level_time;
    ChargeRun::Blown {
        origin: charge.missile.current,
        effect: effect(EFFECT_EXPLOSION_TRIPMINE, charge.missile.current, direction),
    }
}

/// `DetPackBlow` (`:2835-2861`): no more damage taken; the splash by the owner (the
/// caller's; a breakable it was attached to is not here); the det pack effect with the
/// surface's normal as its direction once stuck, straight up before; freed next frame.
fn det_pack_blow(charge: &mut Charge, level_time: i32) -> ChargeRun {
    charge.takes_damage = false;
    let direction = if charge.count == -1 {
        charge.normal
    } else {
        [0.0, 0.0, 1.0]
    };
    charge.phase = Phase::Freeing;
    charge.next_think = level_time;
    ChargeRun::Blown {
        origin: charge.missile.current,
        effect: effect(EFFECT_EXPLOSION_DETPACK, charge.missile.current, direction),
    }
}

impl Charge {
    /// `G_RunThink`: whether the think is due.
    fn think_due(&self, level_time: i32) -> bool {
        self.next_think != 0 && self.next_think <= level_time
    }
}

/// `G_RunItem` (`g_items.c:3218-3271`) for a det pack, every frame: one with no ground
/// under it set falling; one at rest only thinks; else moved to where the trajectory
/// has it (the trace passing its owner), its think, and — stopped short — `G_BounceItem`:
/// the velocity reflected and scaled by its `physicsBounce` (none), then `charge_stick`
/// while it touches, else stood a unit up and snapped with the ground remembered.
fn run_item(
    charge: &mut Charge,
    level_time: i32,
    previous_time: i32,
    world: &dyn MovementCollision,
    everyone: &dyn MovementCollision,
    watched: &[Watched],
    rng: &mut Rng,
    frame: &mut ChargeFrame,
) -> Option<ChargeRun> {
    let state = &mut charge.missile.state;
    if state.raw_field(ES_GROUND_ENTITY).unwrap_or(0) == u32::from(ENTITY_NONE)
        && state.raw_field(ES_POS_TYPE) != Some(TR_GRAVITY)
    {
        state.set_raw_field(ES_POS_TYPE, TR_GRAVITY);
        state.set_raw_field(ES_POS_TIME, level_time as u32);
    }
    if state.raw_field(ES_POS_TYPE) == Some(TR_STATIONARY) {
        return charge.think_due(level_time).then(|| {
            run_think(
                charge,
                level_time,
                previous_time,
                world,
                everyone,
                watched,
                rng,
                frame,
            )
        });
    }
    let base = read_vector(state, ES_POS_BASE);
    let delta = read_vector(state, ES_POS_DELTA);
    let kind = state.raw_field(ES_POS_TYPE).unwrap_or(0) as u8;
    let start_time = state.raw_field(ES_POS_TIME).unwrap_or(0) as i32;
    let origin =
        crate::trajectory::legacy_evaluate_trajectory(base, delta, kind, start_time, 0, level_time);
    let mut trace = world.trace(
        charge.missile.current,
        charge.missile.bounds.0,
        charge.missile.bounds.1,
        origin,
        charge.missile.clip_mask,
    );
    charge.missile.current = trace.end_position;
    if trace.start_solid {
        trace.fraction = 0.0;
    }
    let mut run = ChargeRun::Moved;
    if charge.think_due(level_time) {
        run = run_think(
            charge,
            level_time,
            previous_time,
            world,
            everyone,
            watched,
            rng,
            frame,
        );
    }
    if trace.fraction == 1.0 {
        return Some(run);
    }
    // `G_BounceItem`.
    let hit_time = previous_time + ((level_time - previous_time) as f32 * trace.fraction) as i32;
    let state = &mut charge.missile.state;
    let delta = read_vector(state, ES_POS_DELTA);
    let kind = state.raw_field(ES_POS_TYPE).unwrap_or(0) as u8;
    let start_time = state.raw_field(ES_POS_TIME).unwrap_or(0) as i32;
    let velocity =
        crate::trajectory::legacy_evaluate_trajectory_delta(delta, kind, start_time, 0, hit_time);
    let normal = trace.plane_normal;
    let dot = velocity.iter().zip(normal).map(|(a, b)| a * b).sum::<f32>();
    let reflected: [f32; 3] =
        std::array::from_fn(|axis| (velocity[axis] + -2.0 * dot * normal[axis]) * 0.0);
    write_vector(state, ES_POS_DELTA, reflected);
    if charge.touches {
        charge_stick(charge, &trace, level_time, rng, frame);
        return Some(if matches!(run, ChargeRun::Blown { .. }) {
            run
        } else {
            ChargeRun::Moved
        });
    }
    if normal[2] > 0.0 && reflected[2] < 40.0 {
        let mut end = trace.end_position;
        end[2] += 1.0;
        let end = snap_vector(end);
        set_origin(&mut charge.missile, end);
        charge
            .missile
            .state
            .set_raw_field(ES_GROUND_ENTITY, u32::from(trace.entity_number));
        return Some(if matches!(run, ChargeRun::Blown { .. }) {
            run
        } else {
            ChargeRun::Moved
        });
    }
    let moved: [f32; 3] = std::array::from_fn(|axis| charge.missile.current[axis] + normal[axis]);
    charge.missile.current = moved;
    write_vector(&mut charge.missile.state, ES_POS_BASE, moved);
    charge
        .missile
        .state
        .set_raw_field(ES_POS_TIME, level_time as u32);
    Some(run)
}

/// The charge's think, due: the object physics with its touch (`TrapThink`,
/// `G_RunObject`), the beam, the watch, the blast, the freeing.
fn run_think(
    charge: &mut Charge,
    level_time: i32,
    previous_time: i32,
    world: &dyn MovementCollision,
    everyone: &dyn MovementCollision,
    watched: &[Watched],
    rng: &mut Rng,
    frame: &mut ChargeFrame,
) -> ChargeRun {
    charge.next_think = 0;
    match charge.phase {
        Phase::Flying => {
            // `G_RunObject` with `nextthink` 100 on (`TrapThink`'s 50 is overwritten by
            // it: a mine's first think alone comes 50 on).
            charge.next_think = level_time + FRAMETIME;
            let mut angles = charge.angles;
            let hit = run_object(
                &mut charge.missile,
                &mut angles,
                level_time,
                previous_time,
                world,
            );
            charge.angles = angles;
            if let Some(trace) = hit
                && charge.touches
            {
                if charge.kind == Kind::DetPack {
                    charge_stick(charge, &trace, level_time, rng, frame);
                } else {
                    touch_laser_trap(charge, &trace, level_time, frame);
                }
            }
            ChargeRun::Moved
        }
        Phase::Set | Phase::Beam => {
            // `laserTrapThink`: relinked, armed once, the beam traced along the normal.
            arm(charge, level_time, frame);
            charge.phase = Phase::Beam;
            charge.next_think = level_time + FRAMETIME;
            let base = read_vector(&charge.missile.state, ES_POS_BASE);
            let end: [f32; 3] =
                std::array::from_fn(|axis| base[axis] + BEAM_RANGE * charge.normal[axis]);
            // The beam passes the mine alone: its owner breaks it too.
            let trace = everyone.trace(charge.missile.current, [0.0; 3], [0.0; 3], end, MASK_SHOT);
            charge.missile.state.set_raw_field(ES_TIME, (-1_i32) as u32);
            if trace.entity_number < MAX_CLIENTS || trace.start_solid {
                charge.touches = false;
                charge.phase = Phase::Exploding;
                charge.next_think = level_time + LT_DELAY_TIME;
            }
            ChargeRun::Moved
        }
        Phase::Watching { auto_at } => {
            // `proxMineThink`: every frame; past its time, or a player it watches for
            // within half the splash radius, it blows next frame.
            charge.next_think = level_time;
            if auto_at < level_time {
                charge.phase = Phase::Exploding;
                return ChargeRun::Quiet;
            }
            for player in watched {
                if player.number == charge.missile.owner {
                    continue;
                }
                let apart: [f32; 3] =
                    std::array::from_fn(|axis| charge.missile.current[axis] - player.origin[axis]);
                if (apart[0] * apart[0] + apart[1] * apart[1] + apart[2] * apart[2]).sqrt()
                    < charge.missile.splash_radius / 2.0
                {
                    charge.phase = Phase::Exploding;
                    return ChargeRun::Quiet;
                }
            }
            ChargeRun::Quiet
        }
        Phase::Planted => det_pack_blow(charge, level_time),
        Phase::Exploding => laser_trap_explode(charge, level_time),
        Phase::Freeing => ChargeRun::Freed,
    }
}

/// A frame for a charge at `level_time`: a det pack through `G_RunItem` (its think
/// inside), a mine through its think when due. `world` is the room and everything but
/// the charge's owner (what its move passes), `everyone` the room and every player (what
/// its beam passes); `watched` are the players a proximity mine may see (connected,
/// playing, alive, enemies or friendly fire on).
pub fn run_charge(
    charge: &mut Charge,
    level_time: i32,
    previous_time: i32,
    world: &dyn MovementCollision,
    everyone: &dyn MovementCollision,
    watched: &[Watched],
    rng: &mut Rng,
    frame: &mut ChargeFrame,
) -> ChargeRun {
    if charge.kind == Kind::DetPack {
        return run_item(
            charge,
            level_time,
            previous_time,
            world,
            everyone,
            watched,
            rng,
            frame,
        )
        .unwrap_or(ChargeRun::Quiet);
    }
    if !charge.think_due(level_time) {
        return ChargeRun::Quiet;
    }
    run_think(
        charge,
        level_time,
        previous_time,
        world,
        everyone,
        watched,
        rng,
        frame,
    )
}

/// `G_Damage` on a charge (a blast reaching it, a bolt): its health down by the damage;
/// dead, a trip mine blows 100 ms on with a third of its splash when a player did it
/// (`laserTrapDelayedExplode`), a det pack 50 to 100 ms on (`DetPackDie`); hurt but not
/// dead, a det pack blows the same (`DetPackPain`). Returns whether it took the blow.
pub fn hurt_charge(
    charge: &mut Charge,
    damage: i32,
    by_player: bool,
    level_time: i32,
    rng: &mut Rng,
) -> bool {
    if !charge.takes_damage {
        return false;
    }
    charge.health -= damage;
    match charge.kind {
        Kind::DetPack => {
            charge.phase = Phase::Planted;
            charge.next_think = level_time + rng.irand(50, 100);
            charge.takes_damage = false;
        }
        _ if charge.health <= 0 => {
            charge.phase = Phase::Exploding;
            charge.next_think = level_time + FRAMETIME;
            charge.takes_damage = false;
            if by_player {
                charge.missile.splash_damage /= 3;
                charge.missile.splash_radius /= 3.0;
            }
        }
        _ => {}
    }
    true
}

/// The splash a blast makes: by the owner, from where the charge stands, sparing the
/// charge itself.
pub fn blast_request(charge: &Charge, level_time: i32, attacker: Attacker) -> (DamageRequest, f32) {
    (
        DamageRequest {
            level_time,
            attacker: Some(attacker),
            direction: None,
            point: Some(charge.missile.current),
            damage: charge.missile.splash_damage,
            flags: 0,
            means: charge.missile.splash_method_of_death,
        },
        charge.missile.splash_radius,
    )
}
