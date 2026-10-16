//! The seeker drone a player deploys (`ItemUse_Seeker` outside siege: `EF_SEEKERDRONE`,
//! thirty seconds) and its think every command (`SeekerDroneUpdate` and
//! `FindGenericEnemyIndex`, OpenJK `codemp/game/w_force.c:4534-4731`, which
//! `WP_ForcePowersUpdate` runs): it circles its owner where the client draws it, beeps its
//! last five seconds, sparks out when they end or its owner dies, and fires at the nearest
//! enemy in front of its owner that it can see.

use crate::holdables::EF_SEEKERDRONE;
use crate::player_death::Rng;
use crate::weapon_fire::Missile;

/// `ENTITYNUM_NONE`: no enemy.
const NO_ENEMY: i32 = 1_023;
/// `WP_FireGenericBlasterMissile`'s bolt: 15 damage at 2000, `MOD_BLASTER`, the pistol's
/// look, `DAMAGE_DEATH_KNOCKBACK`, `MASK_SHOT | CONTENTS_LIGHTSABER`, eight bounces.
const BOLT_DAMAGE: i32 = 15;
const BOLT_VELOCITY: f32 = 2_000.0;
const MOD_BLASTER: u32 = 6;
const WP_BRYAR_PISTOL: u32 = 4;
const DAMAGE_DEATH_KNOCKBACK: u32 = 0x0080;
const BOLT_CLIP_MASK: u32 = 0x1301 | 0x0004_0000;
const ES_WEAPON: usize = 14;

/// The drone's owner as the think reads and writes it.
#[derive(Debug)]
pub struct Owner<'a> {
    pub number: u16,
    pub health: i32,
    pub origin: [f32; 3],
    pub view_angles: [f32; 3],
    pub entity_flags: &'a mut u32,
    /// `droneExistTime`, `droneFireTime`: floats in the game state (not transmitted).
    pub exist_time: &'a mut f32,
    pub fire_time: &'a mut f32,
    /// `genericEnemyIndex`.
    pub enemy: &'a mut i32,
}

/// Another client as `FindGenericEnemyIndex` reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub number: u16,
    pub origin: [f32; 3],
    pub health: i32,
    /// `OnSameTeam(self, ent)`.
    pub same_team: bool,
    /// `pm_type` neither `PM_INTERMISSION` nor `PM_SPECTATOR`.
    pub in_play: bool,
    /// A client at all (`en->client`).
    pub client: bool,
}

/// What the think does besides its owner's state.
#[derive(Clone, Debug, PartialEq)]
pub enum Deed {
    /// `G_PlayEffect(EFFECT_SPARK_EXPLOSION, org, (1, 0, 0))`: the drone gone.
    Spark([f32; 3]),
    /// `G_Sound(self, CHAN_BODY, "sound/weapons/laser_trap/warning.wav")`.
    Warning,
    /// The bolt, and `G_SoundAtLoc(org, CHAN_WEAPON, "sound/weapons/bryar/fire.wav")`.
    Fire(Missile),
}

/// What the think asks of the world.
pub trait DroneWorld {
    /// Number of client slots to inspect, in number order.
    fn slots(&self) -> usize;
    /// The client occupying a slot, or none for an empty slot.
    fn candidate(&self, slot: usize) -> Option<Candidate>;
    /// `OrgVisible(from, to, ignore)`: a point trace, `MASK_SOLID`, reaches `to`.
    fn visible(&self, from: [f32; 3], to: [f32; 3], ignore: u16) -> bool;
    /// The firing trace (`MASK_SOLID`, passing through nothing): clear, not started or
    /// ended in anything solid.
    fn clear(&self, from: [f32; 3], to: [f32; 3]) -> bool;
}

/// `InFront(spot, from, angles, threshold)` (`NPC_senses.c:103`): flat, pitch ignored.
pub fn in_front(spot: [f32; 3], from: [f32; 3], angles: [f32; 3], threshold: f32) -> bool {
    let mut dir = [spot[0] - from[0], spot[1] - from[1], 0.0];
    crate::player_angle_math::normalize(&mut dir);
    let forward = crate::pmove::flight::flight_axes([0.0, angles[1], angles[2]])
        .0
        .to_array();
    dir[0] * forward[0] + dir[1] * forward[1] + dir[2] * forward[2] > threshold
}

/// Where the drone is at `level_time`: circling 20 units out and bobbing 5 about
/// `elevated` (the client draws it by the same clock). The reference's arithmetic: the
/// angle in double, stored as a float; `cos`/`sin` of it in double, times 20 and 5,
/// stored as floats; added as floats.
fn circling(elevated: [f32; 3], level_time: i32) -> [f32; 3] {
    let angle = (f64::from((level_time / 12) & 255) * (std::f64::consts::PI * 2.0) / 255.0) as f32;
    let (sin, cos) = (f64::from(angle).sin(), f64::from(angle).cos());
    let dir = [(cos * 20.0) as f32, (sin * 20.0) as f32, (cos * 5.0) as f32];
    std::array::from_fn(|axis| elevated[axis] + dir[axis])
}

/// The owner's origin 40 units up (`elevated`).
fn above(origin: [f32; 3]) -> [f32; 3] {
    [origin[0], origin[1], origin[2] + 40.0]
}

/// `SeekerDroneUpdate` for `owner` at `level_time`.
pub fn update(
    owner: &mut Owner<'_>,
    world: &dyn DroneWorld,
    level_time: i32,
    rng: &mut Rng,
) -> Option<Deed> {
    if *owner.entity_flags & EF_SEEKERDRONE == 0 {
        *owner.enemy = -1;
        return None;
    }
    let now = level_time as f32;
    let gone = |owner: &mut Owner<'_>, elevated: [f32; 3]| {
        *owner.entity_flags &= !EF_SEEKERDRONE;
        *owner.enemy = -1;
        Some(Deed::Spark(circling(elevated, level_time)))
    };
    if owner.health < 1 {
        return gone(owner, above(owner.origin));
    }
    if *owner.exist_time >= now && *owner.exist_time < (level_time + 5_000) as f32 {
        *owner.enemy = (1_024.0 + *owner.exist_time) as i32;
        if *owner.fire_time < now {
            *owner.fire_time = (level_time + 100) as f32;
            return Some(Deed::Warning);
        }
        return None;
    } else if *owner.exist_time < now {
        // It sinks as its last moments pass: `prefig` clamped to 1..55.
        let prefig = ((*owner.exist_time - now) / 80.0).clamp(1.0, 55.0);
        let mut elevated = above(owner.origin);
        elevated[2] -= 55.0 - prefig;
        return gone(owner, elevated);
    }
    if *owner.enemy == -1 {
        *owner.enemy = NO_ENEMY;
    }
    if *owner.enemy != NO_ENEMY {
        let still = usize::try_from(*owner.enemy)
            .ok()
            .and_then(|slot| world.candidate(slot))
            .is_some_and(|enemy| {
                enemy.client
                    && enemy.number != owner.number
                    && enemy.health >= 1
                    && !enemy.same_team
                    && in_front(enemy.origin, owner.origin, owner.view_angles, 0.8)
                    && world.visible(owner.origin, enemy.origin, owner.number)
            });
        if !still {
            *owner.enemy = NO_ENEMY;
        }
    }
    if *owner.enemy == NO_ENEMY {
        // `FindGenericEnemyIndex`: the nearest threat in front and in sight.
        let mut best = (99_999_999.9_f32, None);
        for slot in 0..world.slots() {
            let Some(candidate) = world.candidate(slot) else {
                continue;
            };
            if !candidate.client
                || candidate.number == owner.number
                || candidate.health <= 0
                || candidate.same_team
                || !candidate.in_play
            {
                continue;
            }
            let a: [f32; 3] =
                std::array::from_fn(|axis| candidate.origin[axis] - owner.origin[axis]);
            let length = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
            if length < best.0
                && in_front(candidate.origin, owner.origin, owner.view_angles, 0.8)
                && world.visible(owner.origin, candidate.origin, owner.number)
            {
                best = (length, Some(candidate.number));
            }
        }
        if let Some(number) = best.1 {
            *owner.enemy = i32::from(number);
        }
    }
    if *owner.enemy == NO_ENEMY {
        return None;
    }
    let Some(enemy) = usize::try_from(*owner.enemy)
        .ok()
        .and_then(|slot| world.candidate(slot))
    else {
        return None;
    };
    let org = circling(above(owner.origin), level_time);
    if *owner.fire_time < now && world.clear(org, enemy.origin) {
        let mut direction: [f32; 3] = std::array::from_fn(|axis| enemy.origin[axis] - org[axis]);
        crate::player_angle_math::normalize(&mut direction);
        // `CreateMissile` snaps where it starts (`SnapVector(org)`), after the aim.
        let mut missile = crate::weapon_fire::create_missile_by(
            owner.number,
            crate::weapon_fire::snap_vector(org),
            direction,
            BOLT_VELOCITY,
            level_time,
            false,
        );
        missile.state.set_raw_field(ES_WEAPON, WP_BRYAR_PISTOL);
        missile.damage = BOLT_DAMAGE;
        missile.damage_flags = DAMAGE_DEATH_KNOCKBACK;
        missile.method_of_death = MOD_BLASTER;
        missile.clip_mask = BOLT_CLIP_MASK;
        missile.bounce_count = 8;
        *owner.fire_time = (level_time + rng.irand(400, 700)) as f32;
        return Some(Deed::Fire(missile));
    }
    None
}
