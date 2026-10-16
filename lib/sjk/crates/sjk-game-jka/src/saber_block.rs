//! A lit saber against missiles, as the reference's game module has it:
//!
//! - `WP_SaberStartMissileBlockCheck` (OpenJK `codemp/game/w_saber.c:5456-5846`), run for
//!   every playing client each server frame before the missiles move: the look target
//!   (`ClientBegin`'s `hasLookTarget`, whatever the weapon), and — with a saber in hand,
//!   idle — the nearest missile within 256 units coming at the player, which raises the
//!   saber towards it a frame early (`saberBlocked`, the quadrant of
//!   `WP_SaberBlockNonRandom` `:9133-9205`).
//! - `G_MissileImpact`'s block (`g_missile.c:498-563`) when a missile strikes the player:
//!   `WP_SaberCanBlock` (`w_saber.c:9282-9455`: the saber's state, the attack button,
//!   `InFront` `NPC_senses.c:103-119` with the defense level's cone), the `EV_SABER_BLOCK`
//!   flash, the defense counted one level lower while jumping or backpedalling, then the
//!   bolt reflected back at its shooter (level 3, `G_ReflectMissile` `:43-101`), deflected
//!   away (level 2, `G_DeflectMissile` `:103-150`) or killed on the blade (level 1), and
//!   `saberBlockTime` (nothing for level 3, 150 ms for 2, 250 ms for 1).
//! - The bounce's jitter is the C library's `rand()` through `RandFloat` (`w_saber.c:56-67`),
//!   ported in [`crate::crt_rand`]. On the reference's Linux build the fix `g_randFix` names
//!   is compiled out (`__GCC__` is no compiler's macro), so `rand()`'s 31 bits are divided
//!   by 32768: the jitter dwarfs the direction and a bounced bolt climbs into the octant
//!   of the three draws. The port follows the build the owner plays against.
//!
//! Held against `tools/game-oracle/saberblock.c`. What is not ported: the held saber's
//! own box (`CONTENTS_LIGHTSABER`, positioned from the model's bolts — SJK's server has no
//! skeleton yet), which the reference deflects from without a block check; NPCs; thrown
//! sabers; the shield of a siege class.

use crate::crt_rand::CrtRand;
use crate::event_entity::EventEntity;
use crate::pmove::MovementCollision;
use crate::pmove::flight::flight_axes;
use crate::pmove_weapon::{WEAPON_FIRING, WEAPON_RAISING};
use crate::weapon_fire::Missile;
use sjk_protocol::PlayerState;

/// `EV_SABER_BLOCK`.
pub const EV_SABER_BLOCK: u32 = 31;
/// `saberBlockedType_t`: the quadrants a saber is raised to, for a blade or a missile.
pub const BLOCKED_NONE: u8 = 0;
pub const BLOCKED_UPPER_RIGHT: u8 = 4;
pub const BLOCKED_UPPER_LEFT: u8 = 5;
pub const BLOCKED_LOWER_RIGHT: u8 = 6;
pub const BLOCKED_LOWER_LEFT: u8 = 7;
pub const BLOCKED_TOP: u8 = 8;
pub const BLOCKED_UPPER_RIGHT_PROJ: u8 = 9;
pub const BLOCKED_TOP_PROJ: u8 = 13;
/// `SABER_REFLECT_MISSILE_CONE`: how far to the side a missile may come from.
const REFLECT_CONE: f32 = 0.2;
/// How far the check looks, and how far the second trace follows the missile.
const RADIUS: f32 = 256.0;
const WP_SABER: u8 = 3;
const WP_THERMAL: u8 = 12;
const BUTTON_ATTACK: u16 = 1;
/// `FP_LIGHTNING`, `FP_DRAIN`, `FP_PUSH`, `FP_GRIP`: powers that hold the hands.
const HANDS_BUSY: u32 = 1 << 7 | 1 << 13 | 1 << 3 | 1 << 6;
const MASK_PLAYERSOLID: u32 = 0x1 | 0x100 | 0x10000;
/// `BOTH_KNOCKDOWN1..=BOTH_KNOCKDOWN5`, then the get-ups through `BOTH_GETUP_FROLL_R`.
const KNOCKDOWN_LEGS: std::ops::RangeInclusive<u16> = 1_219..=1_223;
const GETUP_LEGS: std::ops::RangeInclusive<u16> = 1_224..=1_246;
/// `BOTH_A1_T__B_..=BOTH_H1_S1_BR` (`PM_InSaberAnim`).
const SABER_ANIMATIONS: std::ops::RangeInclusive<u16> = 126..=689;
/// `BG_SaberInSpecialAttack` (`bg_panimate.c:606-660`).
const SPECIAL_ATTACK_ANIMATIONS: [u16; 48] = [
    854, 855, 860, 914, 1_209, 1_210, 1_259, 1_258, 1_252, 1_253, 859, 858, 856, 857, 861, 862,
    863, 864, 870, 1_049, 1_048, 1_087, 1_086, 887, 888, 889, 890, 891, 892, 894, 895, 896, 897,
    898, 906, 907, 908, 909, 910, 911, 912, 913, 899, 902, 903, 1_273, 1_264, 1_265,
];
/// `saberMoveName_t` ranges: the ordinary attacks, the specials, the starts, returns and
/// transitions, the bounces and deflections, the broken parries, the knockaways, the
/// parries and the reflections.
const LS_READY: u32 = 1;
const ATTACKS: std::ops::RangeInclusive<u32> = 4..=10;
const SPECIALS: std::ops::RangeInclusive<u32> = 11..=61;
const STARTS_TO_TRANSITIONS: std::ops::RangeInclusive<u32> = 62..=117;
const BOUNCES: std::ops::RangeInclusive<u32> = 118..=132;
const BROKEN_PARRIES: std::ops::RangeInclusive<u32> = 133..=146;
const KNOCKAWAYS: std::ops::RangeInclusive<u32> = 147..=151;
const PARRIES: std::ops::RangeInclusive<u32> = 152..=156;
const REFLECTIONS: std::ops::RangeInclusive<u32> = 157..=161;
/// Weapons and means of death no saber blocks (`g_missile.c:498-507`).
const UNBLOCKABLE_WEAPONS: [u32; 5] = [11, 12, 13, 14, 9];
const UNBLOCKABLE_MEANS: [u32; 4] = [13, 18, 29, 30];

const PS_TORSO_ANIM: usize = 15;
const PS_LEGS_ANIM: usize = 13;
const PS_LEGS_TIMER: usize = 21;
const PS_SABER_BLOCKED: usize = 77;
const ES_POS_TIME: usize = 0;
const ES_POS_BASE: [usize; 3] = [2, 1, 4];
const ES_POS_DELTA: [usize; 3] = [6, 7, 10];
const ES_WEAPON: usize = 14;
const ES_ORIGIN: [usize; 3] = [11, 12, 13];
const ES_ANGLES: [usize; 3] = [25, 9, 24];

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

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `AngleVectors`' forward (the pitch and roll as given).
fn forward_of(angles: [f32; 3]) -> [f32; 3] {
    flight_axes(angles).0.to_array()
}

/// `RandFloat(min, max)` as the reference's Linux build computes it: `rand()`'s whole
/// value over 32768.
fn rand_float(rand: &mut CrtRand, minimum: f32, maximum: f32) -> f32 {
    ((rand.next() as f32) * (maximum - minimum)) / 32_768.0 + minimum
}

/// `InFront`: whether `spot` lies within the cone ahead of `from` facing `angles`' yaw,
/// `threshold` being the cosine it must beat.
pub fn in_front(spot: [f32; 3], from: [f32; 3], angles: [f32; 3], threshold: f32) -> bool {
    let mut direction = [spot[0] - from[0], spot[1] - from[1], 0.0];
    normalize(&mut direction);
    dot(direction, forward_of([0.0, angles[1], 0.0])) > threshold
}

/// `WP_SaberBlockNonRandom`: the quadrant a saber is raised to for a blow or a missile
/// at `hit`, from the eyes' height and the view's yaw: above the eyes by the side it is
/// on (`0.3` of the right vector), within 20 units below likewise (`0.1`), lower still by
/// the side alone; `for_missile` picks the projectile forms.
pub fn block_quadrant(state: &PlayerState, hit: [f32; 3], for_missile: bool) -> u8 {
    block_quadrant_at(
        state.origin(),
        state.view_height() as f32,
        state.view_angles()[1],
        hit,
        for_missile,
    )
}

/// [`block_quadrant`] for a player standing at `origin` with its eyes `view_height` up,
/// looking along `yaw`.
pub fn block_quadrant_at(
    origin: [f32; 3],
    view_height: f32,
    yaw: f32,
    hit: [f32; 3],
    for_missile: bool,
) -> u8 {
    let mut eyes = origin;
    eyes[2] += view_height;
    let mut difference = [hit[0] - eyes[0], hit[1] - eyes[1], 0.0];
    normalize(&mut difference);
    let right = flight_axes([0.0, yaw, 0.0]).1.to_array();
    // The reference compares the float against double literals: exact at the edges.
    let right_dot = f64::from(dot(right, difference));
    let z_difference = hit[2] - eyes[2];
    let quadrant = if z_difference > 0.0 {
        if right_dot > 0.3 {
            BLOCKED_UPPER_RIGHT
        } else if right_dot < -0.3 {
            BLOCKED_UPPER_LEFT
        } else {
            BLOCKED_TOP
        }
    } else if z_difference > -20.0 {
        if right_dot > 0.1 {
            BLOCKED_UPPER_RIGHT
        } else if right_dot < -0.1 {
            BLOCKED_UPPER_LEFT
        } else {
            BLOCKED_TOP
        }
    } else if right_dot >= 0.0 {
        BLOCKED_LOWER_RIGHT
    } else {
        BLOCKED_LOWER_LEFT
    };
    // `WP_MissileBlockForBlock`: the five quadrants have projectile forms five further on.
    if for_missile {
        quadrant + BLOCKED_UPPER_RIGHT_PROJ - BLOCKED_UPPER_RIGHT
    } else {
        quadrant
    }
}

/// `BG_SaberInAttack`: the ordinary attacks and every special.
fn in_attack(saber_move: u32) -> bool {
    ATTACKS.contains(&saber_move) || SPECIALS.contains(&saber_move)
}

/// `SaberAttacking` (`w_saber.c:1050-1075`).
fn attacking(state: &PlayerState) -> bool {
    let saber_move = state.saber_move();
    if PARRIES.contains(&saber_move)
        || BROKEN_PARRIES.contains(&saber_move)
        || BOUNCES.contains(&saber_move)
        || KNOCKAWAYS.contains(&saber_move)
    {
        return false;
    }
    if in_attack(saber_move)
        && state.weapon_state() == WEAPON_FIRING
        && state.saber_blocked() == BLOCKED_NONE
    {
        return true;
    }
    SPECIALS.contains(&saber_move)
}

/// The player a saber defends, as `WP_SaberCanBlock` and the impact read it.
pub struct Defender<'a> {
    pub client: u16,
    pub state: &'a mut PlayerState,
    /// `saberBlocking`, which is not on the wire: the mode of the move in progress.
    pub saber_blocking: u8,
    /// `pers.cmd`: the last command's buttons and forward input.
    pub buttons: u16,
    pub forward_move: i8,
    /// `fd.forcePowerLevel[FP_SABER_DEFENSE]`.
    pub defense: u8,
    /// `saberBlockTime`, which is not on the wire: no block before this time.
    pub block_time: &'a mut i32,
}

impl Defender<'_> {
    /// `WP_SaberCanBlock` for a projectile: whether the saber is in a state to block, and
    /// whether `point` is within the defense level's cone (level 3 blocks 0.3 of the way
    /// round with the model's collision on, 2 0.6, 1 0.9, none never). On a block the
    /// saber is raised to the point's quadrant.
    pub fn can_block(&mut self, point: [f32; 3], level_time: i32) -> bool {
        if !self.ready_to_block(point, level_time) {
            return false;
        }
        let quadrant = block_quadrant(self.state, point, true);
        self.state
            .set_raw_field(PS_SABER_BLOCKED, u32::from(quadrant));
        true
    }

    /// `WP_SaberCanBlock` for a thrown saber (`qfalse, 999`: no strength, the same
    /// tests) and then `WP_SaberBlockNonRandom` in its blow form: the saber raised to
    /// the point's quadrant for a blade, not a bolt.
    pub fn can_block_blow(&mut self, point: [f32; 3], level_time: i32) -> bool {
        if !self.ready_to_block(point, level_time) {
            return false;
        }
        let quadrant = block_quadrant(self.state, point, false);
        self.state
            .set_raw_field(PS_SABER_BLOCKED, u32::from(quadrant));
        true
    }

    /// `WP_SaberCanBlock`'s tests: the saber in a state to block, and `point` within
    /// the defence level's cone.
    fn ready_to_block(&self, point: [f32; 3], level_time: i32) -> bool {
        let state = &*self.state;
        let saber_move = state.saber_move();
        if in_attack(saber_move) {
            return false;
        }
        if SABER_ANIMATIONS.contains(&state.torso_animation())
            && state.saber_blocked() == BLOCKED_NONE
            && saber_move != LS_READY
            && saber_move != 0
            && !(PARRIES.start() <= &saber_move && &saber_move <= REFLECTIONS.end())
        {
            return false;
        }
        if BROKEN_PARRIES.contains(&saber_move)
            || state.saber_entity_num() == 0
            || state.saber_holstered() != 0
            || state.weapon() != WP_SABER
            || state.weapon_state() == WEAPON_RAISING
            || state.saber_in_flight()
            || self.buttons & BUTTON_ATTACK != 0
            || attacking(state)
            || (saber_move != LS_READY && self.saber_blocking == 0)
            || *self.block_time >= level_time
            || state.force_hand_extend() != 0
        {
            return false;
        }
        let block_factor = match self.defense {
            3 => 0.3,
            2 => 0.6,
            1 => 0.9,
            _ => return false,
        };
        in_front(point, state.origin(), state.view_angles(), block_factor)
    }
}

/// What the block did with the missile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocked {
    /// Level 1: the bolt dies on the blade, striking the player without hurting it.
    Killed,
    /// Levels 2 and 3: the bolt flies on, the defender's now.
    Bounced,
}

/// A missile blocked: the flash where it struck, and its fate.
#[derive(Clone, Debug, PartialEq)]
pub struct SaberBlock {
    pub flash: EventEntity,
    pub outcome: Blocked,
}

/// `G_MissileImpact`'s saber block for a missile that struck the defender at
/// `missile.current` on a surface of normal `normal`, its shooter standing at
/// `shooter_origin`: `None` where the saber does not block and the impact goes on.
pub fn block_missile(
    defender: &mut Defender,
    missile: &mut Missile,
    normal: [f32; 3],
    shooter_origin: [f32; 3],
    level_time: i32,
    rand: &mut CrtRand,
) -> Option<SaberBlock> {
    let weapon = missile.state.raw_field(ES_WEAPON).unwrap_or(0);
    if UNBLOCKABLE_WEAPONS.contains(&weapon)
        || UNBLOCKABLE_MEANS.contains(&missile.method_of_death)
        || *defender.block_time >= level_time
    {
        return None;
    }
    if !defender.can_block(missile.current, level_time) {
        return None;
    }
    Some(turn_aside(
        defender,
        missile,
        normal,
        shooter_origin,
        level_time,
        rand,
    ))
}

/// `G_MissileImpact`'s block by a saber entity (`g_missile.c:566-640`): a missile that
/// struck the blade itself is turned aside whatever the saber's owner is doing — no
/// `WP_SaberCanBlock`, no `saberBlockTime` — but for the missiles no saber stops. An
/// owner whose weapon is at rest raises its saber to where the missile struck
/// (`WP_SaberBlockNonRandom`, the projectile forms). `None` where the missile is one no
/// saber stops.
pub fn block_on_saber(
    defender: &mut Defender,
    missile: &mut Missile,
    normal: [f32; 3],
    shooter_origin: [f32; 3],
    level_time: i32,
    rand: &mut CrtRand,
) -> Option<SaberBlock> {
    let weapon = missile.state.raw_field(ES_WEAPON).unwrap_or(0);
    if UNBLOCKABLE_WEAPONS.contains(&weapon) || UNBLOCKABLE_MEANS.contains(&missile.method_of_death)
    {
        return None;
    }
    if defender.state.weapon_time() <= 0 {
        let quadrant = block_quadrant(defender.state, missile.current, true);
        defender
            .state
            .set_raw_field(PS_SABER_BLOCKED, u32::from(quadrant));
    }
    Some(turn_aside(
        defender,
        missile,
        normal,
        shooter_origin,
        level_time,
        rand,
    ))
}

/// The block's end, the same for both: the flash where the missile struck, the defence
/// one level lower for a player jumping or running backwards, the missile killed,
/// deflected or reflected by that level, and the next block's earliest time.
fn turn_aside(
    defender: &mut Defender,
    missile: &mut Missile,
    normal: [f32; 3],
    shooter_origin: [f32; 3],
    level_time: i32,
    rand: &mut CrtRand,
) -> SaberBlock {
    let flash = EventEntity {
        event: EV_SABER_BLOCK,
        parameter: 0,
        origin: missile.current,
        client: None,
        broadcast: false,
        extra: [
            (ES_ORIGIN[0], missile.current[0].to_bits()),
            (ES_ORIGIN[1], missile.current[1].to_bits()),
            (ES_ORIGIN[2], missile.current[2].to_bits()),
            (ES_ANGLES[0], normal[0].to_bits()),
            (ES_ANGLES[1], normal[1].to_bits()),
            (ES_ANGLES[2], normal[2].to_bits()),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
        ],
    };
    // Jumping or running backwards, the defense counts one level lower.
    let mut level = defender.defense;
    if defender.state.velocity()[2] > 0.0 || defender.forward_move < 0 {
        level = level.saturating_sub(1);
    }
    let forward = forward_of(defender.state.view_angles());
    match level {
        1 => {}
        2 => deflect(defender, missile, forward, level_time, rand),
        _ => reflect(defender, missile, forward, shooter_origin, level_time, rand),
    }
    *defender.block_time = if level == 3 {
        0
    } else {
        level_time + (350 - i32::from(level) * 100)
    };
    SaberBlock {
        flash,
        outcome: if level == 1 {
            Blocked::Killed
        } else {
            Blocked::Bounced
        },
    }
}

/// The missile's velocity as its trajectory has it, and its speed.
fn direction_and_speed(missile: &Missile) -> ([f32; 3], f32) {
    let mut delta: [f32; 3] = std::array::from_fn(|axis| {
        f32::from_bits(missile.state.raw_field(ES_POS_DELTA[axis]).unwrap_or(0))
    });
    let speed = normalize(&mut delta);
    (delta, speed)
}

/// The bounce's end, shared by both: the direction jittered, the speed restored, the
/// trajectory restarted from where the missile is, the defender now its owner.
fn bounce(
    defender: u16,
    missile: &mut Missile,
    mut direction: [f32; 3],
    speed: f32,
    jitter: f32,
    level_time: i32,
    rand: &mut CrtRand,
) {
    for axis in direction.iter_mut() {
        *axis += rand_float(rand, -jitter, jitter);
    }
    normalize(&mut direction);
    for axis in 0..3 {
        missile
            .state
            .set_raw_field(ES_POS_DELTA[axis], (direction[axis] * speed).to_bits());
        missile
            .state
            .set_raw_field(ES_POS_BASE[axis], missile.current[axis].to_bits());
    }
    missile.state.set_raw_field(ES_POS_TIME, level_time as u32);
    missile.owner = defender;
}

/// `G_ReflectMissile` by a defender's saber.
fn reflect(
    defender: &Defender,
    missile: &mut Missile,
    forward: [f32; 3],
    shooter_origin: [f32; 3],
    level_time: i32,
    rand: &mut CrtRand,
) {
    reflect_missile(
        defender.client,
        defender.state.origin(),
        missile,
        forward,
        shooter_origin,
        level_time,
        rand,
    );
}

/// `G_ReflectMissile(ent, missile, forward)` (`g_missile.c:43-100`): back at the shooter
/// (`shooter_origin`, a fifth of jitter each way), unless `ent` is the shooter, when it is
/// pushed away half as fast again; `ent` owns it now, and a rocket stops homing.
pub fn reflect_missile(
    ent: u16,
    ent_origin: [f32; 3],
    missile: &mut Missile,
    forward: [f32; 3],
    shooter_origin: [f32; 3],
    level_time: i32,
    rand: &mut CrtRand,
) {
    let (delta, mut speed) = direction_and_speed(missile);
    let mut direction;
    if missile.owner != ent {
        direction = std::array::from_fn(|axis| shooter_origin[axis] - missile.current[axis]);
        normalize(&mut direction);
    } else {
        speed *= 1.5;
        let origin = ent_origin;
        let towards: [f32; 3] = std::array::from_fn(|axis| missile.current[axis] - origin[axis]);
        let scale = dot(forward, towards);
        direction = delta.map(|axis| axis * scale);
        normalize(&mut direction);
    }
    bounce(ent, missile, direction, speed, 0.2, level_time, rand);
    if missile.state.raw_field(ES_WEAPON).unwrap_or(0) == 11 {
        // A rocket (`WP_ROCKET_LAUNCHER`): `think = 0`, `nextthink = 0` — no more homing,
        // and no end of its life either.
        missile.homing = None;
        missile.free_at = i32::MAX;
    }
}

/// `G_DeflectMissile`: away along the defender's view, a whole unit of jitter each way.
fn deflect(
    defender: &Defender,
    missile: &mut Missile,
    forward: [f32; 3],
    level_time: i32,
    rand: &mut CrtRand,
) {
    let (_, speed) = direction_and_speed(missile);
    let scale = dot(forward, forward);
    let mut direction = forward.map(|axis| axis * scale);
    normalize(&mut direction);
    bounce(
        defender.client,
        missile,
        direction,
        speed,
        1.0,
        level_time,
        rand,
    );
}

/// Another player as the look-target search sees it.
#[derive(Clone, Copy, Debug)]
pub struct LookCandidate {
    pub number: u16,
    /// `ps.origin`.
    pub origin: [f32; 3],
    /// The linked box grown by a unit (`r.absmin`, `r.absmax`).
    pub bounds: ([f32; 3], [f32; 3]),
    pub health: i32,
    pub team: i32,
    pub spectator: bool,
}

/// A missile in flight as the check sees it: linked, so with a box in the world.
#[derive(Clone, Copy, Debug)]
pub struct Incoming {
    pub number: u16,
    pub owner: u16,
    /// `r.currentOrigin`, `r.mins`, `r.maxs`.
    pub origin: [f32; 3],
    pub bounds: ([f32; 3], [f32; 3]),
    /// `s.pos.trDelta`, and whether the trajectory is at rest.
    pub delta: [f32; 3],
    pub stationary: bool,
    pub weapon: u8,
    /// `splashDamage && splashRadius`: a player leaves exploding missiles alone.
    pub explodes: bool,
    pub clip_mask: u32,
}

/// The world a missile's path is traced through: the room and the players, the
/// missile's owner left out as its trace's pass entity leaves it out
/// (`SV_ClipMoveToEntities`).
pub trait MissilePaths {
    fn trace_from(
        &self,
        owner: u16,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> crate::pmove::MovementTrace;
}

/// No missile paths at all: for a check with no missiles in the world.
pub struct NoPaths;

impl MissilePaths for NoPaths {
    fn trace_from(
        &self,
        _: u16,
        _: [f32; 3],
        _: [f32; 3],
        _: [f32; 3],
        end: [f32; 3],
        _: u32,
    ) -> crate::pmove::MovementTrace {
        crate::pmove::MovementTrace::miss(end)
    }
}

/// The player running the check.
pub struct Watcher<'a> {
    pub client: u16,
    pub state: &'a mut PlayerState,
    pub health: i32,
    pub team: i32,
    /// `pers.cmd.buttons`.
    pub buttons: u16,
    /// `r.absmax[2]`: the top of the linked box, grown by a unit.
    pub top: f32,
    /// Its first saber may block actively (no `SFL_NOT_ACTIVE_BLOCKING`).
    pub actively_blocks: bool,
}

/// `WP_SaberStartMissileBlockCheck` for a player, every frame after the saber update:
/// `hasLookTarget` cleared and — unless the weapon is busy, the player dead or knocked
/// down, which return at once — set to the nearest living enemy within 256 units in a
/// clear line of sight from the eyes; then, with a lit saber in hand and no attack
/// under way, the nearest missile within 256 units ahead (its direction from the player
/// beating `SABER_REFLECT_MISSILE_CONE`), heading in, with a clear path to the player's
/// top or along its own way, raises the saber to its quadrant. `sight` is the world the
/// line of sight is traced through (the other players and the room), `paths` the one
/// the missiles' ways are.
pub fn missile_block_check(
    watcher: &mut Watcher,
    candidates: &[LookCandidate],
    missiles: &[Incoming],
    sight: &dyn MovementCollision,
    paths: &dyn MissilePaths,
) {
    let state = &mut *watcher.state;
    state.set_raw_field(76, 0);
    let mut full_routine = state.weapon() == WP_SABER
        && !state.saber_in_flight()
        && state.force_powers_active() & HANDS_BUSY == 0;
    if state.weapon_time() > 0 || !watcher.actively_blocks || watcher.health <= 0 {
        return;
    }
    let legs = state.raw_field(PS_LEGS_ANIM).unwrap_or(0) as u16;
    if KNOCKDOWN_LEGS.contains(&legs)
        || (GETUP_LEGS.contains(&legs) && state.raw_field(PS_LEGS_TIMER).unwrap_or(0) != 0)
    {
        return;
    }
    let saber_move = state.saber_move();
    if state.saber_holstered() != 0
        || watcher.buttons & BUTTON_ATTACK != 0
        || in_attack(saber_move)
        || SPECIAL_ATTACK_ANIMATIONS.contains(&(state.raw_field(PS_TORSO_ANIM).unwrap_or(0) as u16))
        || STARTS_TO_TRANSITIONS.contains(&saber_move)
    {
        full_routine = false;
    }
    let origin = state.origin();
    let mut eyes = origin;
    eyes[2] += state.view_height() as f32;
    let within = |bounds: ([f32; 3], [f32; 3])| {
        (0..3).all(|axis| {
            bounds.0[axis] <= origin[axis] + RADIUS && bounds.1[axis] >= origin[axis] - RADIUS
        })
    };
    let mut nearest: Option<(u16, f32)> = None;
    for candidate in candidates {
        if !within(candidate.bounds)
            || candidate.spectator
            || candidate.health <= 0
            || (watcher.team != 0 && candidate.team == watcher.team)
        {
            continue;
        }
        let distance = (0..3)
            .map(|axis| (origin[axis] - candidate.origin[axis]).powi(2))
            .sum::<f32>()
            .sqrt();
        if nearest.is_some_and(|(_, best)| distance >= best) {
            continue;
        }
        let line = sight.trace(eyes, [0.0; 3], [0.0; 3], candidate.origin, MASK_PLAYERSOLID);
        if line.fraction == 1.0 || line.entity_number == candidate.number {
            nearest = Some((candidate.number, distance));
        }
    }
    if let Some((number, _)) = nearest {
        state.set_raw_field(76, 1);
        state.set_raw_field(66, u32::from(number));
    }
    if !full_routine {
        return;
    }
    let forward = forward_of([0.0, state.view_angles()[1], 0.0]);
    let mut closest = RADIUS;
    let mut incoming = None;
    for missile in missiles {
        // Its linked box, grown by a unit as `SV_LinkEntity` grows every one.
        let linked = (
            std::array::from_fn(|axis| missile.origin[axis] + missile.bounds.0[axis] - 1.0),
            std::array::from_fn(|axis| missile.origin[axis] + missile.bounds.1[axis] + 1.0),
        );
        if !within(linked)
            || missile.owner == watcher.client
            || missile.stationary
            || missile.weapon == WP_THERMAL
            || missile.explodes
            || missile.weapon == WP_SABER
        {
            continue;
        }
        let mut direction: [f32; 3] =
            std::array::from_fn(|axis| missile.origin[axis] - origin[axis]);
        let distance = normalize(&mut direction);
        if dot(direction, forward) < REFLECT_CONE {
            continue;
        }
        let mut heading = missile.delta;
        normalize(&mut heading);
        if dot(direction, heading) > 0.0 {
            continue;
        }
        if distance >= closest {
            continue;
        }
        let blocked = |trace: &crate::pmove::MovementTrace| {
            trace.all_solid
                || trace.start_solid
                || (trace.fraction < 1.0
                    && trace.entity_number != watcher.client
                    && trace.entity_number != state.saber_entity_num())
        };
        let to_top = [origin[0], origin[1], watcher.top - 4.0];
        if blocked(&paths.trace_from(
            missile.owner,
            missile.origin,
            missile.bounds.0,
            missile.bounds.1,
            to_top,
            missile.clip_mask,
        )) {
            let along: [f32; 3] =
                std::array::from_fn(|axis| missile.origin[axis] + RADIUS * heading[axis]);
            if blocked(&paths.trace_from(
                missile.owner,
                missile.origin,
                missile.bounds.0,
                missile.bounds.1,
                along,
                missile.clip_mask,
            )) {
                continue;
            }
        }
        closest = distance;
        incoming = Some(missile.origin);
    }
    if let Some(point) = incoming {
        let quadrant = block_quadrant(state, point, true);
        state.set_raw_field(PS_SABER_BLOCKED, u32::from(quadrant));
    }
}
