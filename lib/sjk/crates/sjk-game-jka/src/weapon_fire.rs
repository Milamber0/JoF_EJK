//! A weapon fired and its missile run: `ClientEvents`' `EV_FIRE_WEAPON`/`EV_ALT_FIRE`
//! (OpenJK `codemp/game/g_active.c:890-915`) → `FireWeapon` (`g_weapon.c:4491-4600`:
//! the view's axes, `CalcMuzzlePoint` `:3639-3660`) → `WP_FireBryarPistol`
//! (`:254-310`) → `CreateMissile` (`g_missile.c:297-325`); then `G_RunMissile`
//! (`:815-1030`) every server frame, until `G_MissileImpact` (`:362-810`) ends it on
//! the world: the miss event on the missile itself, which the frames free
//! `EVENT_VALID_MSEC` later. Held against `tools/game-oracle/fire.c`, field for field
//! and frame for frame.
//!
//! The weapons so far: the pistol (`WP_FireBryarPistol` `:254-310`), the E11 blaster
//! (`WP_FireBlaster` `:430-538`: the alternate fire's spread from the game's own
//! generator, `Q_flrand`) and the bowcaster (`WP_FireBowcaster` `:1006-1125`: the
//! alternate bolt that bounces off the world three times — `G_BounceMissile`
//! `g_missile.c:154-210` and `EV_GRENADE_BOUNCE` — and the charged main fire fanning up
//! to five bolts of scaled damage at varied speeds) and the rocket launcher
//! (`WP_FireRocket` `:1909-1990`: a solid, damageable rocket of a hundred with a hundred
//! of splash over 160 units, the alternate at half the speed — its lock is not ported).
//! A missile that hits a player explodes on it as the reference has it
//! (`EV_MISSILE_HIT`, the player named) unless a saber blocks it
//! ([`crate::saber_block`]); the damage it does, direct and splash
//! ([`crate::damage::radius_damage`]), is the caller's.

use crate::event_entity::EventEntity;
use crate::player_death::Rng;
use crate::pmove::flight::flight_axes;
use crate::pmove::{MovementCollision, MovementTrace};
use crate::saber_block::{Blocked, SaberBlock};
use sjk_protocol::{EntityState, LEGACY_ENTITY_FIELDS, PlayerState, legacy_direction_to_byte};

/// `EV_FIRE_WEAPON`, `EV_ALT_FIRE`: the two events of `ClientEvents` that fire.
pub const EV_FIRE_WEAPON: u16 = 27;
pub const EV_ALT_FIRE: u16 = 28;
const EV_MISSILE_HIT: u32 = 85;
const EV_MISSILE_MISS: u32 = 86;
const EVENT_BITS: u32 = 0x300;
const EVENT_BIT1: u32 = 0x100;
const ET_GENERAL: u32 = 0;
const ET_MISSILE: u32 = 3;
/// `ET_NPC`: a shooter that is an NPC.
const ET_NPC: u32 = 13;
const TR_STATIONARY: u32 = 0;
const TR_LINEAR: u32 = 2;
const EF_ALT_FIRING: u32 = 1 << 10;
/// `WP_BRYAR_PISTOL`, `WP_BLASTER`, `WP_BOWCASTER`, `WP_ROCKET_LAUNCHER`.
const WP_BRYAR_PISTOL: u32 = 4;
const WP_BLASTER: u32 = 5;
const WP_BOWCASTER: u32 = 7;
const WP_ROCKET_LAUNCHER: u32 = 11;
/// `WP_REPEATER`, `WP_DEMP2`, `WP_FLECHETTE` and their numbers (`g_weapon.c:74-106`).
const WP_REPEATER: u32 = 8;
const WP_DEMP2: u32 = 9;
const WP_FLECHETTE: u32 = 10;
const REPEATER_SPREAD: f32 = 1.4;
const REPEATER_DAMAGE: i32 = 14;
const REPEATER_VELOCITY: f32 = 1_600.0;
const REPEATER_ALT_SIZE: f32 = 3.0;
const REPEATER_ALT_DAMAGE: i32 = 60;
const REPEATER_ALT_SPLASH_DAMAGE: i32 = 60;
const REPEATER_ALT_SPLASH_RADIUS: f32 = 128.0;
const REPEATER_ALT_VELOCITY: f32 = 1_100.0;
const DEMP2_DAMAGE: i32 = 35;
const DEMP2_VELOCITY: f32 = 1_800.0;
const DEMP2_SIZE: f32 = 2.0;
const FLECHETTE_SHOTS: usize = 5;
const FLECHETTE_SPREAD: f32 = 4.0;
const FLECHETTE_DAMAGE: i32 = 12;
const FLECHETTE_VEL: f32 = 3_500.0;
const FLECHETTE_SIZE: f32 = 1.0;
const FLECHETTE_ALT_DAMAGE: i32 = 60;
const FLECHETTE_ALT_SPLASH_DAM: i32 = 60;
const FLECHETTE_ALT_SPLASH_RAD: f32 = 128.0;
/// `DAMAGE_DEATH_KNOCKBACK`: the `dflags` of these bolts, which `G_MissileImpact` never
/// passes on (it strikes with nothing, or `DAMAGE_HALF_ABSORB` for the bowcaster, the
/// flechette and the rocket) and nothing else reads: masked to nothing here.
const DAMAGE_DEATH_KNOCKBACK: u32 = 0x80;
/// `EV_PLAY_EFFECT` and `EFFECT_EXPLOSION_FLECHETTE`.
const EV_PLAY_EFFECT: u32 = 68;
const EFFECT_EXPLOSION_FLECHETTE: u32 = 7;
/// `WP_CONCUSSION` and its primary's numbers (`g_weapon.c:117-125`).
const WP_CONCUSSION: u32 = 15;
const CONC_VELOCITY: f32 = 3_000.0;
const CONC_DAMAGE: i32 = 75;
const CONC_SPLASH_DAMAGE: i32 = 40;
const CONC_SPLASH_RADIUS: f32 = 200.0;
/// `MOD_CONC`.
const MOD_CONC: u32 = 29;
/// `WP_THERMAL` and its numbers (`g_weapon.c:2001-2014`): a direct hit of seventy, ninety
/// of splash over 128, thrown at up to 900 (the charge's share of it, 0.15 at least),
/// three seconds of fuse — the alternate's numbers are the same ones.
const WP_THERMAL: u32 = 12;
const TD_DAMAGE: i32 = 70;
const TD_SPLASH_RADIUS: f32 = 128.0;
const TD_SPLASH_DAMAGE: i32 = 90;
const TD_VELOCITY: f32 = 900.0;
const TD_MIN_CHARGE: f32 = 0.15;
const TD_TIME: i32 = 3_000;
/// `TR_GRAVITY`.
const TR_GRAVITY: u32 = 6;
/// `CHAN_WEAPON`, `CHAN_BODY`: the channels of the detonator's sounds.
const CHAN_WEAPON: u32 = 2;
const CHAN_BODY: u32 = 6;
/// `EV_GENERAL_SOUND`.
const EV_GENERAL_SOUND: u32 = 76;
const ROCKET_VELOCITY: f32 = 900.0;
const ROCKET_DAMAGE: i32 = 100;
const ROCKET_SPLASH_DAMAGE: i32 = 100;
const ROCKET_SPLASH_RADIUS: f32 = 160.0;
const ROCKET_SIZE: f32 = 3.0;
/// `ROCKET_ALT_THINK_TIME`: how often a homing rocket steers.
const ROCKET_ALT_THINK_TIME: i32 = 100;
/// How long a rocket lives unfired.
const ROCKET_LIFE_MS: i32 = 30_000;
const BRYAR_PISTOL_VEL: f32 = 1_600.0;
const BRYAR_PISTOL_DAMAGE: i32 = 10;
const BRYAR_CHARGE_UNIT: f32 = 200.0;
const BRYAR_ALT_SIZE: f32 = 1.0;
const BLASTER_SPREAD: f32 = 1.6;
const BLASTER_VELOCITY: f32 = 2_300.0;
const BLASTER_DAMAGE: i32 = 20;
/// An NPC's blaster bolt (`WP_FireBlasterMissile`'s "animent" damage).
const BLASTER_NPC_DAMAGE: i32 = 10;
const BOWCASTER_DAMAGE: i32 = 50;
const BOWCASTER_VELOCITY: f32 = 1_300.0;
const BOWCASTER_SIZE: f32 = 2.0;
const BOWCASTER_ALT_SPREAD: f32 = 5.0;
const BOWCASTER_VEL_RANGE: f32 = 0.3;
const BOWCASTER_CHARGE_UNIT: f32 = 200.0;
/// `MOD_BRYAR_PISTOL`, `MOD_BRYAR_PISTOL_ALT`, `MOD_BLASTER`, `MOD_BOWCASTER`.
const MOD_BRYAR_PISTOL: u32 = 4;
const MOD_BRYAR_PISTOL_ALT: u32 = 5;
const MOD_BLASTER: u32 = 6;
const MOD_BOWCASTER: u32 = 11;
/// `MOD_ROCKET`, `MOD_ROCKET_SPLASH`, `MOD_ROCKET_HOMING`, `MOD_ROCKET_HOMING_SPLASH`.
const MOD_ROCKET: u32 = 19;
const MOD_ROCKET_SPLASH: u32 = 20;
const MOD_ROCKET_HOMING: u32 = 21;
const MOD_ROCKET_HOMING_SPLASH: u32 = 22;
/// `MOD_THERMAL`, `MOD_THERMAL_SPLASH`, `MOD_TRIP_MINE_SPLASH`.
const MOD_THERMAL: u32 = 23;
const MOD_THERMAL_SPLASH: u32 = 24;
const MOD_TRIP_MINE_SPLASH: u32 = 25;
/// `MOD_REPEATER`, `MOD_REPEATER_ALT`, `MOD_REPEATER_ALT_SPLASH`, `MOD_DEMP2`,
/// `MOD_FLECHETTE`, `MOD_FLECHETTE_ALT_SPLASH`.
const MOD_REPEATER: u32 = 12;
const MOD_REPEATER_ALT: u32 = 13;
const MOD_REPEATER_ALT_SPLASH: u32 = 14;
const MOD_DEMP2: u32 = 15;
const MOD_FLECHETTE: u32 = 17;
const MOD_FLECHETTE_ALT_SPLASH: u32 = 18;
/// `MASK_SHOT`: a rocket's own contents, which it is solid and damageable with.
const MASK_SHOT: u32 = 0x1 | 0x100 | 0x200 | 0x1000;
/// `EV_GRENADE_BOUNCE`.
const EV_GRENADE_BOUNCE: u32 = 66;
/// `EVENT_VALID_MSEC`: how long an entity's event stays on it.
const EVENT_VALID_MS: i32 = 300;
/// `MASK_SHOT | CONTENTS_LIGHTSABER`.
const PISTOL_CLIP_MASK: u32 = 0x1 | 0x100 | 0x200 | 0x1000 | 0x40000;
/// `SURF_NOIMPACT`.
const SURF_NOIMPACT: u32 = 0x10;
/// `MAX_CLIENTS`: the entities that are players.
const MAX_CLIENTS: u16 = 32;
/// How long a missile lives unfired: `CreateMissile`'s `life` for these weapons.
const MISSILE_LIFE_MS: i32 = 10_000;

const ES_POS_TIME: usize = 0;
const ES_POS_BASE: [usize; 3] = [2, 1, 4];
const ES_POS_DELTA: [usize; 3] = [6, 7, 10];
const ES_POS_DURATION: usize = 20;
const ES_TYPE: usize = 8;
const ES_WEAPON: usize = 14;
const ES_EFLAGS: usize = 19;
const ES_POS_TYPE: usize = 23;
const ES_EVENT: usize = 28;
const ES_EVENT_PARM: usize = 42;
const ES_OTHER_ENTITY: usize = 59;
const ES_ORIGIN: [usize; 3] = [11, 12, 13];
const ES_ORIGIN2: [usize; 3] = [56, 60, 53];
const ES_TRICKED_ENTITY: usize = 58;
const ES_GENERIC1: usize = 86;
/// `apos.trType`, `apos.trBase` (the angles) and `loopSound`.
const ES_APOS_TYPE: usize = 15;
/// `s.apos.trBase`, `s.apos.trDelta`, `s.apos.trTime`.
const ES_APOS_BASE: [usize; 3] = [5, 3, 33];
const ES_APOS_DELTA: [usize; 3] = [48, 44, 49];
const ES_APOS_TIME: usize = 34;
const ES_ANGLES: [usize; 3] = [25, 9, 24];
const ES_LOOP_SOUND: usize = 55;
/// `saberEntityNum`, which `G_SoundTempEntity` carries the channel in.
const ES_SABER_ENTITY: usize = 37;
const PS_WEAPON: usize = 47;
const PS_WEAPON_CHARGE_TIME: usize = 68;

/// `WP_MuzzlePoint` (`bg_weapons.c:30-48`): forward, right, up, by weapon.
const MUZZLE_POINT: [[f32; 3]; 17] = [
    [0.0, 0.0, 0.0],
    [0.0, 8.0, 0.0],
    [0.0, 8.0, 0.0],
    [8.0, 16.0, 0.0],
    [12.0, 6.0, -6.0],
    [12.0, 6.0, -6.0],
    [12.0, 6.0, -6.0],
    [12.0, 2.0, -6.0],
    [12.0, 4.5, -6.0],
    [12.0, 6.0, -6.0],
    [12.0, 6.0, -6.0],
    [12.0, 8.0, -4.0],
    [12.0, 0.0, -4.0],
    [12.0, 0.0, -10.0],
    [12.0, 0.0, -4.0],
    [12.0, 6.0, -6.0],
    [12.0, 6.0, -6.0],
];

/// A missile as the server keeps it beyond its wire state (`gentity_t`'s shared and
/// game fields), from the shot to the impact.
#[derive(Clone, Debug, PartialEq)]
pub struct Missile {
    /// Its wire state, unnumbered until the pool numbers it.
    pub state: EntityState,
    /// `r.currentOrigin`: where the last frame's run left it.
    pub current: [f32; 3],
    /// `r.mins`, `r.maxs`.
    pub bounds: ([f32; 3], [f32; 3]),
    /// `r.ownerNum`: the wire client that fired it, which its trace passes through.
    pub owner: u16,
    /// `clipmask`.
    pub clip_mask: u32,
    /// `damage`, `methodOfDeath`: for what it hits.
    pub damage: i32,
    pub method_of_death: u32,
    /// `nextthink` with `G_FreeEntity`: freed unfired at this time.
    pub free_at: i32,
    /// The velocity it struck with (`BG_EvaluateTrajectoryDelta` as `G_MissileImpact`
    /// read it, before the impact stopped it; straight up where it had none): the
    /// damage's direction.
    pub impact_velocity: [f32; 3],
    /// Where it struck (`tr.endpos`, which `G_RunMissile` makes `r.currentOrigin` before
    /// `G_MissileImpact`): the damage's point, before the impact snaps it for the event.
    pub impact_point: [f32; 3],
    /// `r.linked`: a missile is in the world's box lists only after its first run
    /// (`CreateMissile` sets its origin and no more; `G_RunMissile` links it).
    pub linked: bool,
    /// `FL_BOUNCE` and `bounceCount`: whether it bounces off the world, and how many
    /// times it still may.
    pub bounces: bool,
    pub bounce_count: i32,
    /// `eventTime`: when its last event was added, for the frame that clears it.
    pub event_time: i32,
    /// `dflags` of the direct hit (`G_MissileImpact`: `DAMAGE_HALF_ABSORB` for the
    /// bowcaster, the flechette and the rocket).
    pub damage_flags: u32,
    /// `splashDamage`, `splashRadius`, `splashMethodOfDeath`: the explosion around the
    /// impact, for the caller's `G_RadiusDamage`.
    pub splash_damage: i32,
    pub splash_radius: f32,
    pub splash_method_of_death: u32,
    /// `r.contents`: a rocket flies as a solid, damageable box and lies as one after.
    pub contents: u32,
    /// A rocket fired with a lock on somebody: `rocketThink` steers it.
    pub homing: Option<Homing>,
    /// `FL_BOUNCE_HALF`: a thermal detonator bounces at 0.65 and comes to rest.
    pub bounce_half: bool,
    /// `FL_BOUNCE_SHRAPNEL`: a flechette bolt bounces at a quarter into gravity, a few
    /// times, and comes to rest to be freed 100 ms on.
    pub bounce_shrapnel: bool,
    /// A flechette grenade (`WP_flechette_alt_blow` at `nextthink`): it blows through
    /// `laserTrapExplode` when its life is over instead of being freed.
    pub blows: bool,
    /// A dead saber (`MakeDeadSaber`): its own think instead of a freeing time.
    pub dead_saber: Option<DeadSaber>,
    /// A thermal detonator's fuse and physics (`thermalThinkStandard`).
    pub thermal: Option<Thermal>,
    /// `SVF_BROADCAST`: an armed detonator is sent to everyone, wherever they are.
    pub broadcast: bool,
    /// `think` `G_ExplodeMissile` at [`Self::free_at`] instead of `G_FreeEntity`: a
    /// vehicle's projectile with `explodeOnExpire` ([`crate::vehicle_weapons`]) bursts
    /// where its life ends ([`MissileRun::Burst`]).
    pub explodes: bool,
    /// `parent` where it is not [`Self::owner`]: the vehicle a pilot's shot came from,
    /// whose blast it is (`G_RadiusDamage(..., ent->parent, ...)`). `None`: the owner's.
    pub parent: Option<u16>,
    /// `passThroughNum`: the one it flies through (an emplaced gun's gunner), its sweep
    /// carried on past it (`g_missile.c:882-887`).
    pub pass_through: Option<u16>,
    /// `activator`: the player a thing's shot is credited to (an emplaced gun's gunner,
    /// `player_die`'s `WP_TURRET` rule).
    pub activator: Option<u16>,
}

impl Missile {
    /// Whom the missile's splash is credited to (`ent->parent`).
    pub fn splash_attacker(&self) -> u16 {
        self.parent.unwrap_or(self.owner)
    }
}

/// A thermal detonator's game memory: `genericValue5` (when it blows), `count` (the
/// warning given), whether its think is `thermalDetonatorExplode` now, and
/// `r.currentAngles` for the slope it came to rest on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thermal {
    pub fuse: i32,
    pub warned: bool,
    pub exploding: bool,
    pub angles: [f32; 3],
}

/// What a frame's missile run needs beyond the world: the players a homing rocket asks
/// after, the game's generator (`Q_irand`), the sound table (`G_SoundIndex`) and where a
/// sound's temp entity goes (`G_Sound`).
pub struct MissileFrame<'a> {
    pub homing: &'a dyn HomingTargets,
    pub rng: &'a mut Rng,
    pub sounds: &'a mut dyn FnMut(&[u8]) -> u16,
    pub raise: &'a mut dyn FnMut(EventEntity),
    /// The players' models and saber entities, where the world has them
    /// (`d_projectileGhoul2Collision`); `None` runs missiles against boxes alone.
    pub models: Option<&'a mut dyn MissileModels>,
    /// Whether the entity numbered so is an NPC: a client that is no player, struck as a
    /// player is — the hit event naming it, the mark's data (`g_missile.c:966-975`,
    /// `772-775`: `other->takedamage && other->client`, `ET_NPC`).
    pub npcs: &'a dyn Fn(u16) -> bool,
}

/// What a missile's run asks of the players' models and sabers, with
/// `d_projectileGhoul2Collision` 1: the world it is swept through tests each player's
/// posed model (`G2TRFLAG_DOGHOULTRACE`), and these answer the rest.
pub trait MissileModels {
    /// `G_RunMissile`'s record of a sweep that stopped on entity `number`, its surface
    /// flags holding the struck surface: a client with a model stamps it as its last
    /// struck surface (`g2LastSurfaceHit`, `g2LastSurfaceTime`). Whether the entity has
    /// a model, whose sweep's surface flags are then cleared.
    fn struck(&mut self, number: u16, surface: u32, level_time: i32) -> bool;
    /// The engine's plain trace (no model), which `G_RunMissile` re-traces a missile
    /// stuck where it starts with.
    fn plain_trace(
        &self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> MovementTrace;
    /// `G_MissileImpact` on entity `number` when it is a saber entity: its owner, and the
    /// block ([`crate::saber_block::block_on_saber`]). `None` for anything else.
    fn saber_block(
        &mut self,
        number: u16,
        missile: &mut Missile,
        normal: [f32; 3],
    ) -> Option<(u16, SaberBlock)>;
}

/// Nobody to home on.
pub struct NoTargets;

impl HomingTargets for NoTargets {
    fn target(&self, _: u16) -> Option<HomingTarget> {
        None
    }
}

/// A homing rocket's memory (`WP_FireRocket` with a lock: `enemy`, `angle` 0.5,
/// `movedir` and `random` as `G_Spawn` leaves them, `nextthink`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Homing {
    pub enemy: u16,
    pub move_dir: [f32; 3],
    pub random: f32,
    pub next_think: i32,
}

/// The one a homing rocket is after, as `rocketThink` reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HomingTarget {
    /// `r.currentOrigin`, and half the sum of the box's bottom and top to lift it by.
    pub origin: [f32; 3],
    pub middle_height: f32,
    /// `health > 0`.
    pub alive: bool,
    /// `ps.groundEntityNum != ENTITYNUM_NONE`.
    pub on_ground: bool,
}

/// Who a homing rocket asks about its enemy.
pub trait HomingTargets {
    /// The enemy as it stands now, `None` for one that is gone (or not a client).
    fn target(&self, number: u16) -> Option<HomingTarget>;
}

/// `ClientEvents`' fire (`FireWeapon`): the muzzle, the direction, and the weapon's
/// missiles pushed onto `fired` — none for a weapon that is not ported. `entity` is the
/// player's as this command's think converted it (`s.pos.trBase` is the muzzle's base);
/// `rng` is the game's generator, which the spreads draw from; `rocket_last_valid_time`
/// is the movement's `rocketLastValidTime`, which is not on the wire; `lock_target_valid`
/// says whether the locked-on entity is a living client not on the shooter's team. A
/// rocket fired with a lock clears the lock from the state.
pub fn fire_weapon(
    state: &mut PlayerState,
    entity: &EntityState,
    level_time: i32,
    alternate: bool,
    rng: &mut Rng,
    rocket_last_valid_time: f32,
    lock_target_valid: &dyn Fn(u16) -> bool,
    sounds: &mut dyn FnMut(&[u8]) -> u16,
    fired: &mut Vec<Missile>,
) {
    let weapon = state.raw_field(PS_WEAPON).unwrap_or(0);
    if !matches!(
        weapon,
        WP_BRYAR_PISTOL
            | WP_BLASTER
            | WP_BOWCASTER
            | WP_REPEATER
            | WP_DEMP2
            | WP_FLECHETTE
            | WP_ROCKET_LAUNCHER
            | WP_THERMAL
            | WP_CONCUSSION
    ) {
        return;
    }
    let (muzzle, forward) = muzzle_point(state, entity);
    match weapon {
        WP_BRYAR_PISTOL => fired.push(fire_bryar_pistol(
            state, muzzle, forward, level_time, alternate,
        )),
        WP_BLASTER => fired.push(fire_blaster(
            state,
            muzzle,
            forward,
            level_time,
            alternate,
            rng,
            entity.raw_field(ES_TYPE) == Some(ET_NPC),
        )),
        WP_ROCKET_LAUNCHER => fired.push(fire_rocket(
            state,
            muzzle,
            forward,
            level_time,
            alternate,
            rocket_last_valid_time,
            lock_target_valid,
        )),
        WP_THERMAL => fired.push(fire_thermal(
            state, muzzle, forward, level_time, alternate, sounds,
        )),
        WP_REPEATER => fired.push(fire_repeater(
            state, muzzle, forward, level_time, alternate, rng,
        )),
        // The DEMP2's alternate is the shock sphere of `demp2::fire_alt`, no missile:
        // the caller fires it with a trace of its own.
        WP_DEMP2 if alternate => {}
        WP_DEMP2 => fired.push(fire_demp2(state, muzzle, forward, level_time)),
        WP_CONCUSSION if !alternate => {
            fired.push(fire_concussion(state, muzzle, forward, level_time))
        }
        // The concussion rifle's alternate beam knocks players down: with the knockdown.
        WP_CONCUSSION => {}
        WP_FLECHETTE if alternate => {
            fire_flechette_alt(state, muzzle, forward, level_time, rng, fired)
        }
        WP_FLECHETTE => fire_flechette_main(state, muzzle, forward, level_time, rng, fired),
        _ if alternate => fired.push(fire_bowcaster_alt(state, muzzle, forward, level_time)),
        _ => fire_bowcaster_main(state, muzzle, forward, level_time, rng, fired),
    }
}

/// `CalcMuzzlePoint` (`g_weapon.c:4420-4460`): the entity's base along the view by the
/// weapon's offsets (`WP_MuzzlePoint`), up by the view height, snapped; with the
/// view's forward.
pub fn muzzle_point(state: &PlayerState, entity: &EntityState) -> ([f32; 3], [f32; 3]) {
    let weapon = state.raw_field(PS_WEAPON).unwrap_or(0);
    let (forward, right) = flight_axes(state.view_angles());
    let (forward, right) = (forward.to_array(), right.to_array());
    let offset = MUZZLE_POINT[usize::from(weapon as u8).min(MUZZLE_POINT.len() - 1)];
    let base: [f32; 3] = std::array::from_fn(|axis| {
        f32::from_bits(entity.raw_field(ES_POS_BASE[axis]).unwrap_or(0))
    });
    let mut muzzle: [f32; 3] = std::array::from_fn(|axis| {
        base[axis] + offset[0] * forward[axis] + offset[1] * right[axis]
    });
    muzzle[2] += state.view_height() as f32 + offset[2];
    (snap_vector(muzzle), forward)
}

/// `CreateMissile`: a linear missile from `muzzle` (snapped already) along `direction`
/// at `velocity`, its delta snapped, alive `MISSILE_LIFE_MS`; the weapon, the damage,
/// the box and the bounce are the caller's.
fn create_missile(
    state: &PlayerState,
    muzzle: [f32; 3],
    direction: [f32; 3],
    velocity: f32,
    level_time: i32,
    alternate: bool,
) -> Missile {
    create_missile_by(
        state.client_num(),
        muzzle,
        direction,
        velocity,
        level_time,
        alternate,
    )
}

/// [`create_missile`] for entity `owner`, which need not be a player.
pub(crate) fn create_missile_by(
    owner: u16,
    muzzle: [f32; 3],
    direction: [f32; 3],
    velocity: f32,
    level_time: i32,
    alternate: bool,
) -> Missile {
    let mut entity = EntityState::zero(0, &LEGACY_ENTITY_FIELDS);
    entity.set_raw_field(ES_TYPE, ET_MISSILE);
    if alternate {
        entity.set_raw_field(ES_EFLAGS, EF_ALT_FIRING);
    }
    entity.set_raw_field(ES_POS_TYPE, TR_LINEAR);
    entity.set_raw_field(ES_POS_TIME, level_time as u32);
    let delta = snap_vector(direction.map(|axis| axis * velocity));
    for axis in 0..3 {
        entity.set_raw_field(ES_POS_BASE[axis], muzzle[axis].to_bits());
        entity.set_raw_field(ES_POS_DELTA[axis], delta[axis].to_bits());
    }
    Missile {
        state: entity,
        current: muzzle,
        bounds: ([0.0; 3], [0.0; 3]),
        owner,
        clip_mask: PISTOL_CLIP_MASK,
        damage: 0,
        method_of_death: 0,
        free_at: level_time + MISSILE_LIFE_MS,
        impact_velocity: [0.0; 3],
        impact_point: [0.0; 3],
        linked: false,
        bounces: false,
        bounce_count: 0,
        event_time: 0,
        damage_flags: 0,
        splash_damage: 0,
        splash_radius: 0.0,
        splash_method_of_death: 0,
        contents: 0,
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
    }
}

/// `WP_FireRepeater` (`g_weapon.c:1139-1213`): the primary bolt of fourteen at 1600,
/// with a spread of 1.4 in pitch and yaw (the generator's two draws), the saber in its
/// clip mask, bouncing up to eight times off what bounces it (nothing does: it has no
/// bounce flag); the alternate a ball of sixty at 1100 under gravity, 40 up, in a box of
/// three, with sixty of splash over 128.
fn fire_repeater(
    state: &PlayerState,
    muzzle: [f32; 3],
    forward: [f32; 3],
    level_time: i32,
    alternate: bool,
    rng: &mut Rng,
) -> Missile {
    if alternate {
        let mut missile = create_missile(
            state,
            muzzle,
            forward,
            REPEATER_ALT_VELOCITY,
            level_time,
            true,
        );
        missile.state.set_raw_field(ES_WEAPON, WP_REPEATER);
        missile.bounds = ([-REPEATER_ALT_SIZE; 3], [REPEATER_ALT_SIZE; 3]);
        missile.state.set_raw_field(ES_POS_TYPE, TR_GRAVITY);
        let up = f32::from_bits(missile.state.raw_field(ES_POS_DELTA[2]).unwrap_or(0)) + 40.0;
        missile.state.set_raw_field(ES_POS_DELTA[2], up.to_bits());
        missile.damage = REPEATER_ALT_DAMAGE;
        #[expect(
            clippy::erasing_op,
            reason = "masked to nothing, see DAMAGE_DEATH_KNOCKBACK"
        )]
        let damage_flags = DAMAGE_DEATH_KNOCKBACK & 0;
        missile.damage_flags = damage_flags;
        missile.method_of_death = MOD_REPEATER_ALT;
        missile.splash_method_of_death = MOD_REPEATER_ALT_SPLASH;
        missile.splash_damage = REPEATER_ALT_SPLASH_DAMAGE;
        missile.splash_radius = REPEATER_ALT_SPLASH_RADIUS;
        missile.bounce_count = 8;
        return missile;
    }
    let pitch = rng.flrand(-1.0, 1.0) * REPEATER_SPREAD;
    let yaw = rng.flrand(-1.0, 1.0) * REPEATER_SPREAD;
    let direction = slopped(forward, pitch, yaw);
    let mut missile = create_missile(
        state,
        muzzle,
        direction,
        REPEATER_VELOCITY,
        level_time,
        false,
    );
    missile.state.set_raw_field(ES_WEAPON, WP_REPEATER);
    missile.damage = REPEATER_DAMAGE;
    #[expect(
        clippy::erasing_op,
        reason = "masked to nothing, see DAMAGE_DEATH_KNOCKBACK"
    )]
    let damage_flags = DAMAGE_DEATH_KNOCKBACK & 0;
    missile.damage_flags = damage_flags;
    missile.method_of_death = MOD_REPEATER;
    missile.bounce_count = 8;
    missile
}

/// `WP_FireConcussion` (`g_weapon.c:3341-3380`): a fast rocket of seventy-five at 3000 in
/// a box of three, forty of splash over 200, `MOD_CONC` both ways, the saber in its clip
/// mask, never bouncing (`DAMAGE_EXTRA_KNOCKBACK` is its `dflags`, which nothing reads).
/// `W_TraceSetStart` with no box is a no-op for a player, whose muzzle is within its own
/// box.
fn fire_concussion(
    state: &PlayerState,
    muzzle: [f32; 3],
    forward: [f32; 3],
    level_time: i32,
) -> Missile {
    let mut missile = create_missile(state, muzzle, forward, CONC_VELOCITY, level_time, false);
    missile.state.set_raw_field(ES_WEAPON, WP_CONCUSSION);
    missile.bounds = ([-ROCKET_SIZE; 3], [ROCKET_SIZE; 3]);
    missile.damage = CONC_DAMAGE;
    missile.method_of_death = MOD_CONC;
    missile.splash_method_of_death = MOD_CONC;
    missile.splash_damage = CONC_SPLASH_DAMAGE;
    missile.splash_radius = CONC_SPLASH_RADIUS;
    missile.bounce_count = 0;
    missile
}

/// `WP_DEMP2_MainFire` (`g_weapon.c:1225-1243`): a bolt of thirty-five at 1800 in a box
/// of two, `MASK_SHOT` alone in its clip mask, never bouncing.
fn fire_demp2(
    state: &PlayerState,
    muzzle: [f32; 3],
    forward: [f32; 3],
    level_time: i32,
) -> Missile {
    let mut missile = create_missile(state, muzzle, forward, DEMP2_VELOCITY, level_time, false);
    missile.state.set_raw_field(ES_WEAPON, WP_DEMP2);
    missile.bounds = ([-DEMP2_SIZE; 3], [DEMP2_SIZE; 3]);
    missile.damage = DEMP2_DAMAGE;
    #[expect(
        clippy::erasing_op,
        reason = "masked to nothing, see DAMAGE_DEATH_KNOCKBACK"
    )]
    let damage_flags = DAMAGE_DEATH_KNOCKBACK & 0;
    missile.damage_flags = damage_flags;
    missile.method_of_death = MOD_DEMP2;
    missile.clip_mask = MASK_SHOT;
    missile.bounce_count = 0;
    missile
}

/// `WP_FlechetteMainFire` (`g_weapon.c:1513-1553`): five bolts of twelve at 3500 in a
/// box of one — the first straight, the rest with a spread of four in pitch and yaw —
/// each bouncing as shrapnel (`FL_BOUNCE_SHRAPNEL`) five to eight times, the generator's
/// draw after the spread's.
fn fire_flechette_main(
    state: &PlayerState,
    muzzle: [f32; 3],
    forward: [f32; 3],
    level_time: i32,
    rng: &mut Rng,
    fired: &mut Vec<Missile>,
) {
    for shot in 0..FLECHETTE_SHOTS {
        let (mut pitch, mut yaw) = (0.0, 0.0);
        if shot != 0 {
            pitch = rng.flrand(-1.0, 1.0) * FLECHETTE_SPREAD;
            yaw = rng.flrand(-1.0, 1.0) * FLECHETTE_SPREAD;
        }
        let direction = slopped(forward, pitch, yaw);
        let mut missile =
            create_missile(state, muzzle, direction, FLECHETTE_VEL, level_time, false);
        missile.state.set_raw_field(ES_WEAPON, WP_FLECHETTE);
        missile.bounds = ([-FLECHETTE_SIZE; 3], [FLECHETTE_SIZE; 3]);
        missile.damage = FLECHETTE_DAMAGE;
        // `G_MissileImpact` strikes with `DAMAGE_HALF_ABSORB` for this weapon, whatever
        // `dflags` says (`DAMAGE_DEATH_KNOCKBACK`, which nothing reads).
        missile.damage_flags = crate::damage::DAMAGE_HALF_ABSORB;
        missile.method_of_death = MOD_FLECHETTE;
        missile.bounce_count = rng.irand(5, 8);
        missile.bounce_shrapnel = true;
        fired.push(missile);
    }
}

/// `WP_FlechetteAltFire` (`g_weapon.c:1643-1707`): two grenades of sixty (as
/// `MOD_FLECHETTE_ALT_SPLASH` both ways) with sixty of splash over 128, in a box of
/// three, lobbed 8 to 12 degrees up and within 2 of the yaw (the generator's two draws
/// each), blowing (`WP_flechette_alt_blow`) 1.5 to 3.5 s on and thrown at 700 to 1400
/// a second (two draws more each, in that order) under gravity, bouncing by half up to
/// fifty times. Their start
/// is the muzzle: `W_TraceSetStart` with no box is a no-op for a player, whose muzzle
/// is within its own box.
fn fire_flechette_alt(
    state: &PlayerState,
    muzzle: [f32; 3],
    forward: [f32; 3],
    level_time: i32,
    rng: &mut Rng,
    fired: &mut Vec<Missile>,
) {
    let (pitch, yaw) = crate::damage::vector_to_angles(forward);
    for _ in 0..2 {
        let pitch = pitch - (rng.flrand(0.0, 1.0) * 4.0 + 8.0);
        let yaw = yaw + rng.flrand(-1.0, 1.0) * 2.0;
        let direction = crate::pmove::flight::flight_axes([pitch, yaw, 0.0])
            .0
            .to_array();
        // `CreateMissile( start, fwd, 700 + Q_flrand() * 700, 1500 + Q_flrand() * 2000, ...)`:
        // the reference's compiler evaluates the arguments right to left, so the life is
        // drawn before the speed (the fixture proves it).
        let life = 1_500.0 + rng.flrand(0.0, 1.0) * 2_000.0;
        let velocity = 700.0 + rng.flrand(0.0, 1.0) * 700.0;
        let mut missile = create_missile(state, muzzle, direction, velocity, level_time, true);
        missile.free_at = level_time + life as i32;
        missile.blows = true;
        missile.state.set_raw_field(ES_WEAPON, WP_FLECHETTE);
        missile.bounds = ([-3.0; 3], [3.0; 3]);
        missile.clip_mask = MASK_SHOT;
        missile.state.set_raw_field(ES_POS_TYPE, TR_GRAVITY);
        missile.bounce_half = true;
        missile.bounce_count = 50;
        missile.damage = FLECHETTE_ALT_DAMAGE;
        missile.splash_damage = FLECHETTE_ALT_SPLASH_DAM;
        missile.splash_radius = FLECHETTE_ALT_SPLASH_RAD;
        missile.method_of_death = MOD_FLECHETTE_ALT_SPLASH;
        missile.splash_method_of_death = MOD_FLECHETTE_ALT_SPLASH;
        fired.push(missile);
    }
}

/// `WP_FireThermalDetonator` (`g_weapon.c:2069-2151`): a detonator in a box of three
/// units under gravity, thrown at 900 times the charge's share (`level.time` less
/// `weaponChargeTime` over 900 ms, 0.15 at least, 1 at most) and 120 up for a living
/// thrower; the primary bounces at 0.65 (`FL_BOUNCE_HALF`, `bounceCount` -5: as often as
/// it likes), the alternate bursts on impact; seventy on a direct hit, ninety of splash
/// over 128; its fuse three seconds (`genericValue5`); its loop sound registered here.
/// `W_TraceSetStart` is a no-op for a player: the muzzle twelve ahead and four under the
/// eyes, in a box of three, is within the player's own box whatever its stance.
fn fire_thermal(
    state: &PlayerState,
    muzzle: [f32; 3],
    forward: [f32; 3],
    level_time: i32,
    alternate: bool,
    sounds: &mut dyn FnMut(&[u8]) -> u16,
) -> Missile {
    // Not `CreateMissile`: the alternate is not flagged `EF_ALT_FIRING`.
    let mut entity = EntityState::zero(0, &LEGACY_ENTITY_FIELDS);
    entity.set_raw_field(ES_TYPE, ET_MISSILE);
    let charge = ((level_time - state.raw_field(PS_WEAPON_CHARGE_TIME).unwrap_or(0) as i32) as f32
        / TD_VELOCITY)
        .clamp(TD_MIN_CHARGE, 1.0);
    let mut delta = forward.map(|axis| axis * (TD_VELOCITY * charge));
    if state.health() >= 0 {
        delta[2] += 120.0;
    }
    let delta = snap_vector(delta);
    entity.set_raw_field(ES_POS_TYPE, TR_GRAVITY);
    entity.set_raw_field(ES_POS_TIME, level_time as u32);
    for axis in 0..3 {
        entity.set_raw_field(ES_POS_BASE[axis], muzzle[axis].to_bits());
        entity.set_raw_field(ES_POS_DELTA[axis], delta[axis].to_bits());
    }
    entity.set_raw_field(
        ES_LOOP_SOUND,
        u32::from(sounds(b"sound/weapons/thermal/thermloop.wav")),
    );
    entity.set_raw_field(ES_WEAPON, WP_THERMAL);
    Missile {
        state: entity,
        current: muzzle,
        bounds: ([-ROCKET_SIZE; 3], [ROCKET_SIZE; 3]),
        owner: state.client_num(),
        clip_mask: MASK_SHOT,
        damage: TD_DAMAGE,
        method_of_death: MOD_THERMAL,
        // Its think is its own; nothing frees it unfired.
        free_at: i32::MAX,
        impact_velocity: [0.0; 3],
        impact_point: [0.0; 3],
        linked: false,
        bounces: false,
        bounce_count: -5,
        event_time: 0,
        damage_flags: 0,
        splash_damage: TD_SPLASH_DAMAGE,
        splash_radius: TD_SPLASH_RADIUS,
        splash_method_of_death: MOD_THERMAL_SPLASH,
        contents: 0,
        homing: None,
        bounce_half: !alternate,
        bounce_shrapnel: false,
        blows: false,
        dead_saber: None,
        thermal: Some(Thermal {
            fuse: level_time + TD_TIME,
            warned: false,
            exploding: false,
            angles: [0.0; 3],
        }),
        broadcast: false,
        explodes: false,
        parent: None,
        pass_through: None,
        activator: None,
    }
}

/// `WP_FireRocket`: a rocket of a hundred at 900 units a second (half that for the
/// alternate) in a box of three units, solid and damageable as it flies, with a hundred
/// of splash over 160 units, alive thirty seconds. With a lock (`rocketLockIndex`) held
/// ten intervals of 75 ms (the lock's time, or the last valid one where it went off),
/// the rocket homes on a living foe; the lock is cleared either way.
fn fire_rocket(
    state: &mut PlayerState,
    muzzle: [f32; 3],
    forward: [f32; 3],
    level_time: i32,
    alternate: bool,
    last_valid_time: f32,
    lock_target_valid: &dyn Fn(u16) -> bool,
) -> Missile {
    const ENTITY_NONE: u32 = 1_023;
    const LOCK_INTERVAL: f32 = 1_200.0 / 16.0;
    let velocity = if alternate {
        ROCKET_VELOCITY * 0.5
    } else {
        ROCKET_VELOCITY
    };
    let mut missile = create_missile(state, muzzle, forward, velocity, level_time, alternate);
    let lock_index = state.raw_field(24).unwrap_or(ENTITY_NONE);
    if lock_index != ENTITY_NONE {
        let mut lock_time = f32::from_bits(state.raw_field(79).unwrap_or(0));
        if lock_time == -1.0 {
            lock_time = last_valid_time;
        }
        let intervals = ((level_time as f32 - lock_time) / LOCK_INTERVAL) as i32;
        if intervals >= 10 && lock_time != -1.0 && lock_target_valid(lock_index as u16) {
            missile.homing = Some(Homing {
                enemy: lock_index as u16,
                move_dir: [0.0; 3],
                random: 0.0,
                next_think: level_time + ROCKET_ALT_THINK_TIME,
            });
        }
        state.set_raw_field(24, ENTITY_NONE);
        state.set_raw_field(79, 0f32.to_bits());
        state.set_raw_field(71, 0f32.to_bits());
    }
    missile.state.set_raw_field(ES_WEAPON, WP_ROCKET_LAUNCHER);
    missile.bounds = ([-ROCKET_SIZE; 3], [ROCKET_SIZE; 3]);
    missile.damage = ROCKET_DAMAGE;
    missile.damage_flags = crate::damage::DAMAGE_HALF_ABSORB;
    missile.method_of_death = if alternate {
        MOD_ROCKET_HOMING
    } else {
        MOD_ROCKET
    };
    missile.splash_method_of_death = if alternate {
        MOD_ROCKET_HOMING_SPLASH
    } else {
        MOD_ROCKET_SPLASH
    };
    missile.splash_damage = ROCKET_SPLASH_DAMAGE;
    missile.splash_radius = ROCKET_SPLASH_RADIUS;
    missile.clip_mask = MASK_SHOT;
    missile.contents = MASK_SHOT;
    missile.free_at = level_time + ROCKET_LIFE_MS;
    missile
}

/// `WP_FireBryarPistol`: 1600 units a second; the alternate fire's charge (one unit
/// every 200 ms, five at most) multiplies the damage and grows the missile's box.
fn fire_bryar_pistol(
    state: &PlayerState,
    muzzle: [f32; 3],
    forward: [f32; 3],
    level_time: i32,
    alternate: bool,
) -> Missile {
    let mut missile = create_missile(
        state,
        muzzle,
        forward,
        BRYAR_PISTOL_VEL,
        level_time,
        alternate,
    );
    missile.state.set_raw_field(ES_WEAPON, WP_BRYAR_PISTOL);
    let mut damage = BRYAR_PISTOL_DAMAGE;
    let mut half = 0.0;
    if alternate {
        let count = ((level_time - state.weapon_charge_time()) as f32 / BRYAR_CHARGE_UNIT) as i32;
        let count = count.clamp(1, 5);
        damage =
            (f64::from(damage) * (f64::from(count) * if count > 1 { 1.7 } else { 1.5 })) as i32;
        missile.state.set_raw_field(ES_GENERIC1, count as u32);
        half = BRYAR_ALT_SIZE * (count as f32 * 0.5);
    }
    missile.bounds = ([-half; 3], [half; 3]);
    missile.damage = damage;
    missile.method_of_death = if alternate {
        MOD_BRYAR_PISTOL_ALT
    } else {
        MOD_BRYAR_PISTOL
    };
    missile
}

/// The view's forward taken through `vectoangles` and back through `AngleVectors`, as
/// the blaster's and the bowcaster's fires do before adding their slop: `pitch` and
/// `yaw` are added to the angles between.
fn slopped(forward: [f32; 3], pitch: f32, yaw: f32) -> [f32; 3] {
    let (angle_pitch, angle_yaw) = crate::damage::vector_to_angles(forward);
    flight_axes([angle_pitch + pitch, angle_yaw + yaw, 0.0])
        .0
        .to_array()
}

/// `WP_FireBlaster`: 2300 units a second, twenty points — ten from an NPC
/// (`g_weapon.c:437-440`, `s.eType == ET_NPC`); the alternate fire's direction slopped by
/// 1.6 degrees of the generator's draw in pitch, then in yaw.
fn fire_blaster(
    state: &PlayerState,
    muzzle: [f32; 3],
    forward: [f32; 3],
    level_time: i32,
    alternate: bool,
    rng: &mut Rng,
    npc: bool,
) -> Missile {
    let (mut pitch, mut yaw) = (0.0, 0.0);
    if alternate {
        pitch = rng.flrand(-1.0, 1.0) * BLASTER_SPREAD;
        yaw = rng.flrand(-1.0, 1.0) * BLASTER_SPREAD;
    }
    let direction = slopped(forward, pitch, yaw);
    let mut missile = create_missile(
        state,
        muzzle,
        direction,
        BLASTER_VELOCITY,
        level_time,
        alternate,
    );
    missile.state.set_raw_field(ES_WEAPON, WP_BLASTER);
    missile.damage = if npc {
        BLASTER_NPC_DAMAGE
    } else {
        BLASTER_DAMAGE
    };
    missile.method_of_death = MOD_BLASTER;
    missile.bounce_count = 8;
    missile
}

/// `WP_BowcasterAltFire`: one bolt of fifty at 1300 units a second in a box of two
/// units, bouncing off the world three times; its direct hit is half absorbed.
fn fire_bowcaster_alt(
    state: &PlayerState,
    muzzle: [f32; 3],
    forward: [f32; 3],
    level_time: i32,
) -> Missile {
    let mut missile = create_missile(
        state,
        muzzle,
        forward,
        BOWCASTER_VELOCITY,
        level_time,
        false,
    );
    missile.state.set_raw_field(ES_WEAPON, WP_BOWCASTER);
    missile.bounds = ([-BOWCASTER_SIZE; 3], [BOWCASTER_SIZE; 3]);
    missile.damage = BOWCASTER_DAMAGE;
    missile.damage_flags = crate::damage::DAMAGE_HALF_ABSORB;
    missile.method_of_death = MOD_BOWCASTER;
    missile.bounces = true;
    missile.bounce_count = 3;
    missile
}

/// `WP_BowcasterMainFire`: the charge's count (one unit every 200 ms, one to five, an
/// even count knocked down to odd) of bolts, each of a damage scaled by the count
/// (50, 45, 40, 35, 30), at a speed varied by the generator (a third each way), slopped
/// a degree of the draw in pitch and fanned five degrees apart in yaw.
fn fire_bowcaster_main(
    state: &PlayerState,
    muzzle: [f32; 3],
    forward: [f32; 3],
    level_time: i32,
    rng: &mut Rng,
    fired: &mut Vec<Missile>,
) {
    let mut count =
        ((level_time - state.weapon_charge_time()) as f32 / BOWCASTER_CHARGE_UNIT) as i32;
    count = count.clamp(1, 5);
    if count & 1 == 0 {
        count -= 1;
    }
    let damage = match count {
        ..=1 => 50,
        2 => 45,
        3 => 40,
        4 => 35,
        _ => 30,
    };
    for index in 0..count {
        let velocity = BOWCASTER_VELOCITY * (rng.flrand(-1.0, 1.0) * BOWCASTER_VEL_RANGE + 1.0);
        let pitch = rng.flrand(-1.0, 1.0) * BOWCASTER_ALT_SPREAD * 0.2;
        let yaw =
            (index as f32 + 0.5) * BOWCASTER_ALT_SPREAD - count as f32 * 0.5 * BOWCASTER_ALT_SPREAD;
        let direction = slopped(forward, pitch, yaw);
        let mut missile = create_missile(state, muzzle, direction, velocity, level_time, true);
        missile.state.set_raw_field(ES_WEAPON, WP_BOWCASTER);
        missile.bounds = ([-BOWCASTER_SIZE; 3], [BOWCASTER_SIZE; 3]);
        missile.damage = damage;
        missile.damage_flags = crate::damage::DAMAGE_HALF_ABSORB;
        missile.method_of_death = MOD_BOWCASTER;
        fired.push(missile);
    }
}

/// What a frame's run of a missile came to.
#[derive(Clone, Debug, PartialEq)]
pub enum MissileRun {
    /// Still flying: its position moved on.
    Flying,
    /// It hit the world and became the miss event, freed `EVENT_VALID_MSEC` on.
    Missed,
    /// It hit a player or an NPC ([`MissileFrame::npcs`]), who is named in the hit event it
    /// became; the damage is the caller's.
    Hit(u16),
    /// It hit something that is not a player and not the world — a breakable brush, and
    /// in time a turret or a shootable door. The reference damages whatever it struck if
    /// that thing takes damage (`G_MissileImpact`'s `other->takedamage`), so the entity's
    /// number is reported and the damage is the caller's, exactly as for a player.
    HitEntity(u16),
    /// Its life ran out, or it struck a surface nothing impacts on: freed at once.
    Freed,
    /// It struck a player whose saber blocked it: the flash where it did, and — killed
    /// on the blade — the hit event it became, naming the player, or — bounced — its
    /// new flight, the player's own now.
    Blocked { struck: u16, block: SaberBlock },
    /// It bounced off the world (`G_BounceMissile`, the bounce event on it) and flies on.
    Bounced,
    /// A detonator's fuse ran out: it burst where it lay (`thermalDetonatorExplode`), the
    /// miss event straight up on it, freed `EVENT_VALID_MSEC` on; the splash from where
    /// it stands, sparing nobody, is the caller's.
    Burst,
    /// A flechette grenade blew where it lay (`laserTrapExplode`): the splash from where
    /// it stands, sparing nobody, as `MOD_TRIP_MINE_SPLASH`, is the caller's, then the
    /// burst's effect; it flies on, a unit along x, for the one frame until
    /// `G_FreeEntity` frees it.
    Blown { effect: EventEntity },
    /// A flechette grenade struck a player and blew on it (`G_MissileImpact` calling its
    /// think): the hit counted for the accuracy, the splash sparing nobody as
    /// `MOD_TRIP_MINE_SPLASH`, the effect, then — the hit event on it as for any hit —
    /// the splash once more as the grenade's own, sparing the one struck.
    HitBlown { struck: u16, effect: EventEntity },
}

/// A saber's answer to a missile striking a player: the impact's `normal` and the
/// struck player's number are given; a block changes the missile.
pub type Defence<'a> = dyn FnMut(u16, &mut Missile, [f32; 3]) -> Option<SaberBlock> + 'a;

/// `G_RunMissile` for one server frame at `level_time`: the missile is swept from where
/// it was to where its trajectory has it now, through `world` (which passes the owner
/// and everything the clip mask leaves out). On the world it ends with
/// `G_MissileImpact`'s miss: the event with the surface's normal, a general entity at
/// the point of impact snapped towards where it came from, at rest.
pub fn run_missile(
    missile: &mut Missile,
    level_time: i32,
    world: &dyn MovementCollision,
) -> MissileRun {
    let mut rng = Rng(1);
    let mut frame = MissileFrame {
        homing: &NoTargets,
        rng: &mut rng,
        sounds: &mut |_| 0,
        raise: &mut |_| {},
        models: None,
        npcs: &|_| false,
    };
    run_missile_against(
        missile,
        level_time,
        level_time - 50,
        world,
        &mut |_, _, _| None,
        &mut frame,
    )
}

/// [`run_missile`] with the sabers in the way: `defence` is asked for every player the
/// missile strikes, before the impact's hit and damage (`G_MissileImpact`,
/// `g_missile.c:498-563`). `G_RunFrame`'s clearing of an event shown long enough comes
/// first, as it does for every entity before it runs; `previous_time` is the last
/// frame's, which a bounce takes its moment from; `homing` is who a homing rocket asks
/// about its enemy, with the game's generator its steering draws from.
pub fn run_missile_against(
    missile: &mut Missile,
    level_time: i32,
    previous_time: i32,
    world: &dyn MovementCollision,
    defence: &mut Defence,
    frame: &mut MissileFrame,
) -> MissileRun {
    if level_time - missile.event_time > EVENT_VALID_MS
        && missile.state.raw_field(ES_EVENT).unwrap_or(0) != 0
    {
        missile.state.set_raw_field(ES_EVENT, 0);
    }
    let read = |index: usize| f32::from_bits(missile.state.raw_field(index).unwrap_or(0));
    let base: [f32; 3] = std::array::from_fn(|axis| read(ES_POS_BASE[axis]));
    let delta: [f32; 3] = std::array::from_fn(|axis| read(ES_POS_DELTA[axis]));
    let kind = missile.state.raw_field(ES_POS_TYPE).unwrap_or(0) as u8;
    let start_time = missile.state.raw_field(ES_POS_TIME).unwrap_or(0) as i32;
    let duration = missile.state.raw_field(ES_POS_DURATION).unwrap_or(0) as i32;
    let origin = crate::trajectory::legacy_evaluate_trajectory(
        base, delta, kind, start_time, duration, level_time,
    );
    let mut trace = world.trace(
        missile.current,
        missile.bounds.0,
        missile.bounds.1,
        origin,
        missile.clip_mask,
    );
    if trace.fraction != 1.0
        && trace.entity_number < crate::pmove::ENTITY_NUMBER_WORLD
        && let Some(models) = frame.models.as_deref_mut()
        && models.struck(trace.entity_number, trace.surface_flags, level_time)
    {
        // "clear the surface flags after, since we actually care about them in here"
        trace.surface_flags = 0;
    }
    if trace.start_solid || trace.all_solid {
        trace = match frame.models.as_deref() {
            Some(models) => models.plain_trace(
                missile.current,
                missile.bounds.0,
                missile.bounds.1,
                missile.current,
                missile.clip_mask,
            ),
            None => world.trace(
                missile.current,
                missile.bounds.0,
                missile.bounds.1,
                missile.current,
                missile.clip_mask,
            ),
        };
        trace.fraction = 0.0;
    } else {
        missile.current = trace.end_position;
    }
    missile.linked = true;
    // `g_missile.c:882-887`: through the one it passes (an emplaced gun's gunner), on to
    // where its trajectory is, and to its think.
    let passed = missile
        .pass_through
        .is_some_and(|number| number == trace.entity_number);
    if passed {
        missile.current = origin;
    }
    if trace.fraction != 1.0 && !passed {
        if trace.surface_flags & SURF_NOIMPACT != 0 {
            return MissileRun::Freed;
        }
        // `G_MissileImpact`: a player hit is named (`takedamage` and a client); anything
        // else the missile strikes is a miss with the surface's normal — or a bounce, for
        // a missile that bounces and still may. A player hit also carries the mark's
        // data (`g_missile.c:968-981`): where the missile is and where its trajectory had
        // it, two units apart at least, and that the two agree.
        let player = (trace.entity_number < MAX_CLIENTS || (frame.npcs)(trace.entity_number))
            .then_some(trace.entity_number);
        // Anything else the sweep named that is not the world: `G_MissileImpact` damages
        // it just the same, and a saber never blocks for it.
        let other_entity = (player.is_none()
            && trace.entity_number != sjk_protocol::ENTITY_NUMBER_NONE
            && trace.entity_number != crate::pmove::ENTITY_NUMBER_WORLD)
            .then_some(trace.entity_number);
        if player.is_none()
            && missile.bounce_shrapnel
            && (missile.bounce_count > 0 || missile.bounce_count == -5)
        {
            // `G_MissileImpact`'s shrapnel bounce (`g_missile.c:389-398`): no event, and
            // the flag dropped with the last bounce.
            bounce_missile(missile, &trace, level_time, previous_time, frame);
            if missile.bounce_count < 1 {
                missile.bounce_shrapnel = false;
            }
            return MissileRun::Bounced;
        }
        if player.is_none()
            && (missile.bounces || missile.bounce_half)
            && (missile.bounce_count > 0 || missile.bounce_count == -5)
        {
            bounce_missile(missile, &trace, level_time, previous_time, frame);
            add_event(missile, EV_GRENADE_BOUNCE, 0, level_time);
            // A detonator brought to rest still thinks this frame (`G_RunThink` after
            // the bounce): its fuse, and `G_RunObject`'s settling.
            if let Some(thermal) = missile.thermal {
                return thermal_think(missile, thermal, level_time, previous_time, world, frame);
            }
            // A dead saber thinks after its bounce too.
            if missile.dead_saber.is_some() {
                return match dead_saber_think(missile, level_time, previous_time, world) {
                    MissileRun::Flying => MissileRun::Bounced,
                    run => run,
                };
            }
            return MissileRun::Bounced;
        }
        missile.impact_velocity = crate::trajectory::legacy_evaluate_trajectory_delta(
            delta, kind, start_time, duration, level_time,
        );
        if missile.impact_velocity == [0.0; 3] {
            missile.impact_velocity[2] = 1.0;
        }
        if player.is_some() {
            let mut projected = origin;
            if projected == missile.current {
                projected[2] += 2.0;
            }
            for axis in 0..3 {
                missile
                    .state
                    .set_raw_field(ES_ORIGIN[axis], missile.current[axis].to_bits());
                missile
                    .state
                    .set_raw_field(ES_ORIGIN2[axis], projected[axis].to_bits());
            }
        }
        let block = player.and_then(|struck| defence(struck, missile, trace.plane_normal));
        // Or the blade itself: a saber entity turns the missile aside (`g_missile.c:566`).
        let saber = match (other_entity, frame.models.as_deref_mut()) {
            (Some(number), Some(models)) => models.saber_block(number, missile, trace.plane_normal),
            _ => None,
        };
        if let Some((owner, block)) = saber {
            if block.outcome == Blocked::Killed {
                // `killProj` on something that takes no damage: the miss.
                missile_impact(missile, &trace, None, level_time);
            }
            return MissileRun::Blocked {
                struck: owner,
                block,
            };
        }
        let bounced = block
            .as_ref()
            .is_some_and(|block| block.outcome == Blocked::Bounced);
        // A flechette grenade on a player blows there instead of striking it
        // (`g_missile.c:668-684`), before the hit event and the grenade's own splash.
        let blown = (player.is_some() && block.is_none() && missile.blows).then(|| blow(missile));
        if !bounced {
            missile_impact(missile, &trace, player, level_time);
        }
        // `G_RunMissile` after the impact: the mark is drawn where the hit event's other
        // entity is the one struck, which a bounce leaves unnamed.
        if missile.state.raw_field(ES_OTHER_ENTITY).unwrap_or(0) == u32::from(trace.entity_number) {
            missile.state.set_raw_field(ES_TRICKED_ENTITY, 1);
        }
        return match (player, block, blown) {
            (Some(struck), Some(block), _) => MissileRun::Blocked { struck, block },
            (Some(struck), None, Some(effect)) => MissileRun::HitBlown { struck, effect },
            (Some(other), None, None) => MissileRun::Hit(other),
            (None, _, _) if other_entity.is_some() => MissileRun::HitEntity(other_entity.unwrap()),
            (None, _, _) => MissileRun::Missed,
        };
    }
    // `G_RunThink`: the missile's own think — a homing rocket's steering or a
    // detonator's fuse and settling (which took the place of its freeing), else
    // `G_FreeEntity` once its life is over.
    if let Some(thermal) = missile.thermal {
        return thermal_think(missile, thermal, level_time, previous_time, world, frame);
    }
    if missile.dead_saber.is_some() {
        return dead_saber_think(missile, level_time, previous_time, world);
    }
    if let Some(steering) = missile.homing {
        if steering.next_think <= level_time {
            return rocket_think(missile, steering, level_time, frame.homing, frame.rng);
        }
        return MissileRun::Flying;
    }
    if missile.free_at > level_time {
        return MissileRun::Flying;
    }
    if missile.explodes {
        explode(missile, level_time);
        return MissileRun::Burst;
    }
    if missile.blows {
        // Its think at its life's end: the grenade — no miss event on it — is freed on
        // the next frame.
        let effect = blow(missile);
        missile.free_at = level_time + 1;
        return MissileRun::Blown { effect };
    }
    MissileRun::Freed
}

/// `WP_flechette_alt_blow` then `laserTrapExplode` (`g_weapon.c:1632-1640`,
/// `:2327-2363`): the velocity set to a unit along x, the splash from where the grenade
/// lies as `MOD_TRIP_MINE_SPLASH` (the reference's own mix-up) sparing nobody — the
/// caller's — and the flechette's burst effect there along that unit (`EV_PLAY_EFFECT`),
/// returned for the caller to raise after the splash's own events.
fn blow(missile: &mut Missile) -> EventEntity {
    missile.blows = false;
    for (axis, value) in ES_POS_DELTA.into_iter().zip([1.0f32, 0.0, 0.0]) {
        missile.state.set_raw_field(axis, value.to_bits());
    }
    let mut effect = EventEntity {
        event: EV_PLAY_EFFECT,
        parameter: EFFECT_EXPLOSION_FLECHETTE,
        origin: missile.current,
        client: None,
        broadcast: false,
        extra: [(0, 0); 12],
    };
    effect.extra[0] = (ES_ANGLES[0], 1.0f32.to_bits());
    for axis in 0..3 {
        effect.extra[1 + axis] = (ES_ORIGIN[axis], missile.current[axis].to_bits());
    }
    effect
}

/// `G_ExplodeMissile` (`g_missile.c:221-256`): where the trajectory has the missile now,
/// snapped (`G_SetOrigin`), a general entity with `EV_MISSILE_MISS` straight up, freed
/// with the event; the splash from there is the caller's.
fn explode(missile: &mut Missile, level_time: i32) {
    // `G_RunThink` clears `nextthink` before it thinks.
    missile.free_at = 0;
    let read = |index: usize| f32::from_bits(missile.state.raw_field(index).unwrap_or(0));
    let base: [f32; 3] = std::array::from_fn(|axis| read(ES_POS_BASE[axis]));
    let delta: [f32; 3] = std::array::from_fn(|axis| read(ES_POS_DELTA[axis]));
    let kind = missile.state.raw_field(ES_POS_TYPE).unwrap_or(0) as u8;
    let start_time = missile.state.raw_field(ES_POS_TIME).unwrap_or(0) as i32;
    let duration = missile.state.raw_field(ES_POS_DURATION).unwrap_or(0) as i32;
    let origin = snap_vector(crate::trajectory::legacy_evaluate_trajectory(
        base, delta, kind, start_time, duration, level_time,
    ));
    set_origin(missile, origin);
    missile.state.set_raw_field(ES_TYPE, ET_GENERAL);
    add_event(
        missile,
        EV_MISSILE_MISS,
        u32::from(legacy_direction_to_byte([0.0, 0.0, 1.0])),
        level_time,
    );
}

/// A dead saber's think (`DeadSaberThink`, `w_saber.c:6204-6214`) and what it keeps:
/// `speed`, the time it is freed after, `nextthink`, and `r.currentAngles`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DeadSaber {
    pub until: i32,
    pub next_think: i32,
    pub angles: [f32; 3],
    /// Its think is `G_FreeEntity` now: freed at the next.
    pub freeing: bool,
}

/// `G_RunThink` for a dead saber: freed once its time is past (the think after the one
/// that saw it); else `G_RunObject` every 100 ms, its touch (`SaberBounceSound`)
/// standing its angles upright.
fn dead_saber_think(
    missile: &mut Missile,
    level_time: i32,
    previous_time: i32,
    world: &dyn MovementCollision,
) -> MissileRun {
    let Some(mut dead) = missile.dead_saber else {
        return MissileRun::Flying;
    };
    if dead.next_think <= 0 || dead.next_think > level_time {
        return MissileRun::Flying;
    }
    dead.next_think = 0;
    if dead.freeing {
        return MissileRun::Freed;
    }
    if dead.until < level_time {
        dead.freeing = true;
        dead.next_think = level_time;
        missile.dead_saber = Some(dead);
        return MissileRun::Flying;
    }
    // `G_RunObject`: the angles from the spin first, then the move, 100 ms on.
    let read = |index: usize| f32::from_bits(missile.state.raw_field(index).unwrap_or(0));
    let base: [f32; 3] = std::array::from_fn(|axis| read(ES_APOS_BASE[axis]));
    let delta: [f32; 3] = std::array::from_fn(|axis| read(ES_APOS_DELTA[axis]));
    let kind = missile.state.raw_field(ES_APOS_TYPE).unwrap_or(0) as u8;
    let start = missile.state.raw_field(ES_APOS_TIME).unwrap_or(0) as i32;
    dead.angles =
        crate::trajectory::legacy_evaluate_trajectory(base, delta, kind, start, 0, level_time);
    dead.next_think = level_time + 100;
    if run_object(missile, &mut dead.angles, level_time, previous_time, world).is_some() {
        let mut upright = dead.angles;
        upright[0] = 90.0;
        for axis in 0..3 {
            missile
                .state
                .set_raw_field(ES_APOS_BASE[axis], upright[axis].to_bits());
        }
    }
    missile.dead_saber = Some(dead);
    MissileRun::Flying
}

/// `MOD_TRIP_MINE_SPLASH`, as `laserTrapExplode` names a flechette grenade's blow.
pub const MOD_LASER_TRAP_BLOW: u32 = MOD_TRIP_MINE_SPLASH;

/// `rocketThink` (`g_weapon.c:1734-1895`) for a player's rocket: with the enemy gone,
/// dead or no client, the rocket is freed ten seconds on (its think replaced); else it
/// turns towards the enemy's middle — sharply when the enemy is behind, half its
/// speed; by half its direction under 0.7 of it; by nine tenths otherwise — with the
/// generator's three draws (of nothing: `random` is 0), dipping towards the floor
/// within 128 units of an enemy on the ground; normalised, at half the speed, snapped,
/// the trajectory restarted; and thinks again 100 ms on.
fn rocket_think(
    missile: &mut Missile,
    mut steering: Homing,
    level_time: i32,
    targets: &dyn HomingTargets,
    rng: &mut Rng,
) -> MissileRun {
    let mut velocity = ROCKET_VELOCITY;
    let Some(enemy) = targets.target(steering.enemy).filter(|enemy| enemy.alive) else {
        missile.homing = None;
        missile.free_at = level_time + 10_000;
        return MissileRun::Flying;
    };
    let (new_mult, old_mult) = (1.0, 1.0);
    let mut target_origin = enemy.origin;
    target_origin[2] += enemy.middle_height;
    let mut target_dir: [f32; 3] =
        std::array::from_fn(|axis| target_origin[axis] - missile.current[axis]);
    normalize(&mut target_dir);
    let move_dir = steering.move_dir;
    let dot =
        target_dir[0] * move_dir[0] + target_dir[1] * move_dir[1] + target_dir[2] * move_dir[2];
    let mut new_dir: [f32; 3];
    if dot < 0.0 {
        let right = [
            move_dir[1] * 1.0 - move_dir[2] * 0.0,
            move_dir[2] * 0.0 - move_dir[0] * 1.0,
            move_dir[0] * 0.0 - move_dir[1] * 0.0,
        ];
        let dot2 = target_dir[0] * right[0] + target_dir[1] * right[1] + target_dir[2] * right[2];
        let scale = if dot2 > 0.0 {
            0.4 * new_mult
        } else {
            -0.4 * new_mult
        };
        new_dir = std::array::from_fn(|axis| move_dir[axis] + scale * right[axis]);
        new_dir[2] = ((target_dir[2] * new_mult) + (move_dir[2] * old_mult)) * 0.5;
        velocity *= 0.5;
    } else if dot < 0.70 {
        new_dir = std::array::from_fn(|axis| move_dir[axis] + 0.5 * new_mult * target_dir[axis]);
    } else {
        new_dir = std::array::from_fn(|axis| move_dir[axis] + 0.9 * new_mult * target_dir[axis]);
    }
    for axis in new_dir.iter_mut() {
        *axis += rng.flrand(-1.0, 1.0) * steering.random * 0.25;
    }
    steering.random *= 0.9;
    if enemy.on_ground {
        let distance = (0..3)
            .map(|axis| (missile.current[axis] - target_origin[axis]).powi(2))
            .sum::<f32>()
            .sqrt();
        if distance < 128.0 {
            new_dir[2] -= (1.0 - (distance / 128.0)) * 0.6;
        }
    }
    normalize(&mut new_dir);
    let delta = snap_vector(new_dir.map(|axis| axis * (velocity * 0.5)));
    for axis in 0..3 {
        missile
            .state
            .set_raw_field(ES_POS_DELTA[axis], delta[axis].to_bits());
        missile
            .state
            .set_raw_field(ES_POS_BASE[axis], missile.current[axis].to_bits());
    }
    missile.state.set_raw_field(ES_POS_TIME, level_time as u32);
    steering.move_dir = new_dir;
    steering.next_think = level_time + ROCKET_ALT_THINK_TIME;
    missile.homing = Some(steering);
    MissileRun::Flying
}

/// `VectorNormalize`: the length, the vector scaled by its reciprocal when there is one.
fn normalize(vector: &mut [f32; 3]) -> f32 {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if length != 0.0 {
        let inverse = 1.0 / length;
        for axis in vector.iter_mut() {
            *axis *= inverse;
        }
    }
    length
}

/// `G_AddEvent` on a missile: the event with the next stepped sequence bits, its
/// parameter, and the time it was added.
pub(crate) fn add_event(missile: &mut Missile, event: u32, parameter: u32, level_time: i32) {
    let state = &mut missile.state;
    let bits =
        (state.raw_field(ES_EVENT).unwrap_or(0) & EVENT_BITS).wrapping_add(EVENT_BIT1) & EVENT_BITS;
    state.set_raw_field(ES_EVENT, event | bits);
    state.set_raw_field(ES_EVENT_PARM, parameter);
    missile.event_time = level_time;
}

/// `G_BounceMissile` (`g_missile.c:154-210`) for a missile that bounces whole or by
/// half: the velocity at the moment of the hit (the last frame's time plus the fraction
/// of this one, as an integer) reflected on the plane — scaled by 0.65 for
/// `FL_BOUNCE_HALF`, and brought to rest (`G_SetOrigin` where it struck) on a face that
/// is more floor than wall once under 40 a second — a detonator's bounce sound (one of
/// two, the generator's draw), then the missile moved a unit out along the normal and
/// its trajectory restarted there, one bounce fewer left.
fn bounce_missile(
    missile: &mut Missile,
    trace: &MovementTrace,
    level_time: i32,
    previous_time: i32,
    frame: &mut MissileFrame,
) {
    let read = |index: usize| f32::from_bits(missile.state.raw_field(index).unwrap_or(0));
    let delta: [f32; 3] = std::array::from_fn(|axis| read(ES_POS_DELTA[axis]));
    let kind = missile.state.raw_field(ES_POS_TYPE).unwrap_or(0) as u8;
    let start_time = missile.state.raw_field(ES_POS_TIME).unwrap_or(0) as i32;
    let duration = missile.state.raw_field(ES_POS_DURATION).unwrap_or(0) as i32;
    let hit_time = previous_time + ((level_time - previous_time) as f32 * trace.fraction) as i32;
    let velocity = crate::trajectory::legacy_evaluate_trajectory_delta(
        delta, kind, start_time, duration, hit_time,
    );
    let dot = velocity
        .iter()
        .zip(trace.plane_normal)
        .map(|(a, b)| a * b)
        .sum::<f32>();
    let normal = trace.plane_normal;
    let mut reflected: [f32; 3] =
        std::array::from_fn(|axis| velocity[axis] + -2.0 * dot * normal[axis]);
    if missile.bounce_shrapnel {
        // A quarter as fast, and under gravity from here; on a floor, under 40 a second
        // up, it comes to rest, freed 100 ms on.
        reflected = reflected.map(|axis| axis * 0.25);
        missile.state.set_raw_field(ES_POS_TYPE, TR_GRAVITY);
        if normal[2] > 0.7 && reflected[2] < 40.0 {
            set_origin(missile, trace.end_position);
            missile.free_at = level_time + 100;
            return;
        }
    } else if missile.bounce_half {
        reflected = reflected.map(|axis| axis * 0.65);
        let speed = (reflected[0] * reflected[0]
            + reflected[1] * reflected[1]
            + reflected[2] * reflected[2])
            .sqrt();
        if normal[2] > 0.2 && speed < 40.0 {
            set_origin(missile, trace.end_position);
            return;
        }
    }
    for axis in 0..3 {
        missile
            .state
            .set_raw_field(ES_POS_DELTA[axis], reflected[axis].to_bits());
    }
    if missile.state.raw_field(ES_WEAPON) == Some(WP_THERMAL) {
        let which = frame.rng.irand(1, 2);
        let name = if which == 1 {
            &b"sound/weapons/thermal/bounce1.wav"[..]
        } else {
            b"sound/weapons/thermal/bounce2.wav"
        };
        let index = (frame.sounds)(name);
        (frame.raise)(sound_event(missile.current, CHAN_BODY, index));
    } else if missile.state.raw_field(ES_WEAPON) == Some(3) {
        // A knocked saber, `WP_SABER` (`G_BounceMissile`: `bounce1` to `bounce3`).
        let name: &[u8] = match frame.rng.irand(1, 3) {
            1 => b"sound/weapons/saber/bounce1.wav",
            2 => b"sound/weapons/saber/bounce2.wav",
            _ => b"sound/weapons/saber/bounce3.wav",
        };
        let index = (frame.sounds)(name);
        (frame.raise)(sound_event(missile.current, CHAN_BODY, index));
    }
    for axis in 0..3 {
        missile.current[axis] += normal[axis];
        missile
            .state
            .set_raw_field(ES_POS_BASE[axis], missile.current[axis].to_bits());
    }
    missile.state.set_raw_field(ES_POS_TIME, level_time as u32);
    if missile.bounce_count != -5 {
        missile.bounce_count -= 1;
    }
}

/// `G_SetOrigin`: the entity stood at `origin`, its trajectory stationary there.
pub(crate) fn set_origin(missile: &mut Missile, origin: [f32; 3]) {
    let state = &mut missile.state;
    state.set_raw_field(ES_POS_TYPE, TR_STATIONARY);
    state.set_raw_field(ES_POS_TIME, 0);
    state.set_raw_field(ES_POS_DURATION, 0);
    for axis in 0..3 {
        state.set_raw_field(ES_POS_BASE[axis], origin[axis].to_bits());
        state.set_raw_field(ES_POS_DELTA[axis], 0);
    }
    missile.current = origin;
}

/// `G_Sound` (`G_SoundTempEntity`): the sound's temp entity where the missile is, the
/// channel in `saberEntityNum`.
pub(crate) fn sound_event(origin: [f32; 3], channel: u32, index: u16) -> EventEntity {
    EventEntity {
        event: EV_GENERAL_SOUND,
        parameter: u32::from(index),
        origin,
        client: None,
        broadcast: false,
        extra: [
            (ES_SABER_ENTITY, channel),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
        ],
    }
}

/// A detonator's think (`thermalThinkStandard`, `thermalDetonatorExplode`,
/// `g_weapon.c:2019-2066`), run every frame after its move: its fuse past, the next
/// frame's think is the burst — which first, once, gives the warning (`CHAN_WEAPON`,
/// `count`), pushes the fuse 500 ms on and makes the detonator a broadcast; and then
/// bursts where the trajectory has it, eight units up and snapped (`G_SetOrigin`), as a
/// general entity with `EV_MISSILE_MISS` straight up, freed with the event. Until then,
/// `G_RunObject` settles it every frame.
fn thermal_think(
    missile: &mut Missile,
    mut thermal: Thermal,
    level_time: i32,
    previous_time: i32,
    world: &dyn MovementCollision,
    frame: &mut MissileFrame,
) -> MissileRun {
    if thermal.exploding {
        if !thermal.warned {
            let index = (frame.sounds)(b"sound/weapons/thermal/warning.wav");
            (frame.raise)(sound_event(missile.current, CHAN_WEAPON, index));
            thermal.warned = true;
            thermal.fuse = level_time + 500;
            thermal.exploding = false;
            missile.broadcast = true;
            missile.thermal = Some(thermal);
            return MissileRun::Flying;
        }
        let read = |index: usize| f32::from_bits(missile.state.raw_field(index).unwrap_or(0));
        let base: [f32; 3] = std::array::from_fn(|axis| read(ES_POS_BASE[axis]));
        let delta: [f32; 3] = std::array::from_fn(|axis| read(ES_POS_DELTA[axis]));
        let kind = missile.state.raw_field(ES_POS_TYPE).unwrap_or(0) as u8;
        let start_time = missile.state.raw_field(ES_POS_TIME).unwrap_or(0) as i32;
        let duration = missile.state.raw_field(ES_POS_DURATION).unwrap_or(0) as i32;
        let mut origin = crate::trajectory::legacy_evaluate_trajectory(
            base, delta, kind, start_time, duration, level_time,
        );
        origin[2] += 8.0;
        let origin = snap_vector(origin);
        set_origin(missile, origin);
        missile.state.set_raw_field(ES_TYPE, ET_GENERAL);
        add_event(
            missile,
            EV_MISSILE_MISS,
            u32::from(legacy_direction_to_byte([0.0, 0.0, 1.0])),
            level_time,
        );
        missile.thermal = Some(thermal);
        return MissileRun::Burst;
    }
    if thermal.fuse < level_time {
        thermal.exploding = true;
        missile.thermal = Some(thermal);
        return MissileRun::Flying;
    }
    run_object(
        missile,
        &mut thermal.angles,
        level_time,
        previous_time,
        world,
    );
    missile.thermal = Some(thermal);
    MissileRun::Flying
}

/// `G_RunObject` (`g_object.c:94-260`) for a detonator, after `G_RunMissile` moved it:
/// one at rest is set falling again from where it lies (`TR_GRAVITY` from the last
/// frame's time); where the trajectory has it now is traced to (its owner passed) — a
/// clear way, or no way at all (a frame's fall into the floor it lies on, fraction 0):
/// on a face more floor than wall it is stood still (`G_StopObjectMoving`: stationary,
/// `s.origin` where it is, no velocity — the trajectory's time left as it was) with its
/// angles matched to the slope (`pitch_roll_for_slope`); on a wall a bouncing one that
/// did not move is stopped there, one that did is bounced (`G_BounceObject`). A
/// detonator never moves here in practice — `G_RunMissile` had it already — so
/// `DoImpact`, whose force a detonator's mass of one never reaches, and the push
/// triggers are not ported.
pub(crate) fn run_object(
    missile: &mut Missile,
    angles: &mut [f32; 3],
    level_time: i32,
    previous_time: i32,
    world: &dyn MovementCollision,
) -> Option<MovementTrace> {
    let state = &mut missile.state;
    if state.raw_field(ES_POS_TYPE).unwrap_or(0) == TR_STATIONARY {
        state.set_raw_field(ES_POS_TYPE, TR_GRAVITY);
        for axis in 0..3 {
            state.set_raw_field(ES_POS_BASE[axis], missile.current[axis].to_bits());
        }
        state.set_raw_field(ES_POS_TIME, previous_time as u32);
    }
    let read = |index: usize| f32::from_bits(missile.state.raw_field(index).unwrap_or(0));
    let base: [f32; 3] = std::array::from_fn(|axis| read(ES_POS_BASE[axis]));
    let delta: [f32; 3] = std::array::from_fn(|axis| read(ES_POS_DELTA[axis]));
    let kind = missile.state.raw_field(ES_POS_TYPE).unwrap_or(0) as u8;
    let start_time = missile.state.raw_field(ES_POS_TIME).unwrap_or(0) as i32;
    let duration = missile.state.raw_field(ES_POS_DURATION).unwrap_or(0) as i32;
    let origin = crate::trajectory::legacy_evaluate_trajectory(
        base, delta, kind, start_time, duration, level_time,
    );
    if origin == missile.current {
        return None;
    }
    let mut trace = world.trace(
        missile.current,
        missile.bounds.0,
        missile.bounds.1,
        origin,
        missile.clip_mask,
    );
    if !trace.start_solid && !trace.all_solid && trace.fraction != 0.0 {
        missile.current = trace.end_position;
    } else {
        trace.fraction = 0.0;
    }
    if trace.fraction == 1.0 {
        return None;
    }
    if missile.state.raw_field(ES_POS_TYPE).unwrap_or(0) == TR_GRAVITY {
        if trace.plane_normal[2] < 0.7 {
            if missile.bounces || missile.bounce_half {
                if trace.fraction <= 0.0 {
                    missile.current = trace.end_position;
                    for axis in 0..3 {
                        missile
                            .state
                            .set_raw_field(ES_POS_BASE[axis], trace.end_position[axis].to_bits());
                        missile.state.set_raw_field(ES_POS_DELTA[axis], 0);
                    }
                    missile.state.set_raw_field(ES_POS_TIME, level_time as u32);
                } else {
                    bounce_object(missile, angles, &trace, level_time, previous_time);
                }
            }
        } else {
            missile.state.set_raw_field(ES_APOS_TYPE, TR_STATIONARY);
            pitch_roll_for_slope(angles, trace.plane_normal);
            for axis in 0..3 {
                missile
                    .state
                    .set_raw_field(ES_APOS_BASE[axis], angles[axis].to_bits());
            }
            // `G_StopObjectMoving`.
            missile.state.set_raw_field(ES_POS_TYPE, TR_STATIONARY);
            for axis in 0..3 {
                missile
                    .state
                    .set_raw_field(ES_ORIGIN[axis], missile.current[axis].to_bits());
                missile
                    .state
                    .set_raw_field(ES_POS_BASE[axis], missile.current[axis].to_bits());
                missile.state.set_raw_field(ES_POS_DELTA[axis], 0);
            }
        }
    } else {
        missile.state.set_raw_field(ES_APOS_TYPE, TR_STATIONARY);
        pitch_roll_for_slope(angles, trace.plane_normal);
        for axis in 0..3 {
            missile
                .state
                .set_raw_field(ES_APOS_BASE[axis], angles[axis].to_bits());
        }
    }
    // The touch that follows is the caller's.
    Some(trace)
}

/// `G_BounceObject` (`g_object.c:34-82`): the velocity at the moment of the hit
/// reflected whole; by half (`FL_BOUNCE_HALF`) it is halved, and on a face more floor
/// than wall under 40 a second up it stops where it struck, its angles kept; otherwise
/// it flies on from the point of impact, at the time of it.
fn bounce_object(
    missile: &mut Missile,
    angles: &mut [f32; 3],
    trace: &MovementTrace,
    level_time: i32,
    previous_time: i32,
) {
    let read = |index: usize| f32::from_bits(missile.state.raw_field(index).unwrap_or(0));
    let delta: [f32; 3] = std::array::from_fn(|axis| read(ES_POS_DELTA[axis]));
    let kind = missile.state.raw_field(ES_POS_TYPE).unwrap_or(0) as u8;
    let start_time = missile.state.raw_field(ES_POS_TIME).unwrap_or(0) as i32;
    let duration = missile.state.raw_field(ES_POS_DURATION).unwrap_or(0) as i32;
    let hit_time = previous_time + ((level_time - previous_time) as f32 * trace.fraction) as i32;
    let velocity = crate::trajectory::legacy_evaluate_trajectory_delta(
        delta, kind, start_time, duration, hit_time,
    );
    let normal = trace.plane_normal;
    let dot = velocity.iter().zip(normal).map(|(a, b)| a * b).sum::<f32>();
    let mut reflected: [f32; 3] =
        std::array::from_fn(|axis| velocity[axis] + -2.0 * dot * normal[axis]);
    if missile.bounce_half {
        reflected = reflected.map(|axis| axis * 0.5);
        if normal[2] > 0.7 && reflected[2] < 40.0 {
            missile.state.set_raw_field(ES_APOS_TYPE, TR_STATIONARY);
            for axis in 0..3 {
                missile
                    .state
                    .set_raw_field(ES_APOS_BASE[axis], angles[axis].to_bits());
                missile
                    .state
                    .set_raw_field(ES_POS_DELTA[axis], reflected[axis].to_bits());
                missile
                    .state
                    .set_raw_field(ES_POS_BASE[axis], trace.end_position[axis].to_bits());
            }
            missile.current = trace.end_position;
            missile.state.set_raw_field(ES_POS_TIME, level_time as u32);
            return;
        }
    }
    missile.current = trace.end_position;
    missile.state.set_raw_field(ES_POS_TIME, hit_time as u32);
    for axis in 0..3 {
        missile
            .state
            .set_raw_field(ES_POS_DELTA[axis], reflected[axis].to_bits());
        missile
            .state
            .set_raw_field(ES_POS_BASE[axis], missile.current[axis].to_bits());
    }
}

/// `pitch_roll_for_slope` (`NPC.c:345-416`) for a thing without a client: its pitch and
/// roll matched to the slope's normal — `vectoangles` of the normal (pitch negated, so a
/// floor is 0), its yaw's forward against the thing's own forward and right.
pub(crate) fn pitch_roll_for_slope(angles: &mut [f32; 3], slope: [f32; 3]) {
    if slope == [0.0; 3] {
        return;
    }
    let (old_forward, old_right) = crate::pmove::flight::flight_axes(*angles);
    let (old_forward, old_right) = (old_forward.to_array(), old_right.to_array());
    let (slope_pitch, slope_yaw) = crate::damage::vector_to_angles(slope);
    let pitch = slope_pitch + 90.0;
    let (new_forward, _) = crate::pmove::flight::flight_axes([0.0, slope_yaw, 0.0]);
    let new_forward = new_forward.to_array();
    let modifier = if new_forward
        .iter()
        .zip(old_right)
        .map(|(a, b)| a * b)
        .sum::<f32>()
        < 0.0
    {
        -1.0
    } else {
        1.0
    };
    let dot = new_forward
        .iter()
        .zip(old_forward)
        .map(|(a, b)| a * b)
        .sum::<f32>();
    angles[0] = dot * pitch;
    angles[2] = (1.0 - dot.abs()) * pitch * modifier;
}

/// `G_MissileImpact`'s end (`g_missile.c:766-810`): the hit or the miss event.
fn missile_impact(
    missile: &mut Missile,
    trace: &MovementTrace,
    player: Option<u16>,
    level_time: i32,
) {
    // `G_AddEvent`'s `eventTime`.
    missile.event_time = level_time;
    let state = &mut missile.state;
    // `G_AddEvent`: the next stepped sequence bits.
    let bits =
        (state.raw_field(ES_EVENT).unwrap_or(0) & EVENT_BITS).wrapping_add(EVENT_BIT1) & EVENT_BITS;
    state.set_raw_field(
        ES_EVENT,
        if player.is_some() {
            EV_MISSILE_HIT
        } else {
            EV_MISSILE_MISS
        } | bits,
    );
    state.set_raw_field(
        ES_EVENT_PARM,
        u32::from(legacy_direction_to_byte(trace.plane_normal)),
    );
    if let Some(other) = player {
        state.set_raw_field(ES_OTHER_ENTITY, u32::from(other));
    }
    state.set_raw_field(ES_TYPE, ET_GENERAL);
    missile.impact_point = trace.end_position;
    // `SnapVectorTowards` the point of impact towards the base it flew from, then
    // `G_SetOrigin` there.
    let mut end = trace.end_position;
    for axis in 0..3 {
        let base = f32::from_bits(state.raw_field(ES_POS_BASE[axis]).unwrap_or(0));
        end[axis] = if base <= end[axis] {
            end[axis].floor()
        } else {
            end[axis].ceil()
        };
    }
    state.set_raw_field(ES_POS_TYPE, TR_STATIONARY);
    state.set_raw_field(ES_POS_TIME, 0);
    state.set_raw_field(ES_POS_DURATION, 0);
    for axis in 0..3 {
        state.set_raw_field(ES_POS_BASE[axis], end[axis].to_bits());
        state.set_raw_field(ES_POS_DELTA[axis], 0);
    }
    // `SnapVectorTowards(trace->endpos, ...)` snaps the trace's own end in place, and
    // `G_SetOrigin(ent, trace->endpos)` then stands the entity there: the splash starts
    // from the snapped point.
    missile.current = end;
}

/// The game module's own `SnapVector` (`q_math.c:1240`): an `(int)` cast on every
/// build but 32-bit MSVC, so it truncates where the engine's rounds.
pub(crate) fn snap_vector(vector: [f32; 3]) -> [f32; 3] {
    vector.map(|value| value as i32 as f32)
}
