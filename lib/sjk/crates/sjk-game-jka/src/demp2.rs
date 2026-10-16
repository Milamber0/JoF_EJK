//! The DEMP2's alternate fire (OpenJK `codemp/game/g_weapon.c:1247-1487`): an instant
//! trace to where the shock sphere will stand, an entity that is no missile
//! (`ET_GENERAL`, never linked) which detonates on the next frame — the effect, its
//! count doubled on it — and then, every 50 ms for 800 ms, expands as the cube of its
//! age (`DEMP2_AltRadiusDamage`), hurting what its shockwave's edge reaches for the first
//! time, the owner spared.

use crate::damage::{Attacker, DamageRequest};
use crate::event_entity::EventEntity;
use crate::pmove::MovementTrace;
use sjk_protocol::{EntityState, LEGACY_ENTITY_FIELDS, PlayerState};

/// `DEMP2_ALT_DAMAGE`, `DEMP2_CHARGE_UNIT`, `DEMP2_ALT_RANGE`. (`DEMP2_ALT_SPLASHRADIUS`,
/// 256, is set on the sphere and never read: the shockwave has its own radius.)
const DEMP2_ALT_DAMAGE: i32 = 8;
const DEMP2_CHARGE_UNIT: f32 = 700.0;
const DEMP2_ALT_RANGE: f32 = 4_096.0;
/// `MOD_DEMP2`, `WP_DEMP2`, `MASK_SHOT`, `EV_PLAY_EFFECT`, `EFFECT_EXPLOSION_DEMP2ALT`.
const MOD_DEMP2: u32 = 15;
const WP_DEMP2: u32 = 9;
const MASK_SHOT: u32 = 0x1 | 0x100 | 0x200 | 0x1000;
const EV_PLAY_EFFECT: u32 = 68;
const EFFECT_EXPLOSION_DEMP2ALT: u32 = 9;
/// The wire fields the sphere's (unsent) state carries: the origin as `trBase`, the
/// weapon; the effect's angles and weapon.
const ES_POS_BASE: [usize; 3] = [2, 1, 4];
const ES_WEAPON: usize = 14;
const ES_ANGLES: [usize; 3] = [25, 9, 24];
const ES_ORIGIN: [usize; 3] = [11, 12, 13];
const PS_WEAPON_CHARGE_TIME: usize = 68;

/// The shock sphere as the server keeps it: `demp2_alt_proj`.
#[derive(Clone, Debug, PartialEq)]
pub struct Sphere {
    /// Its wire state, unnumbered until the pool numbers it; never linked, never sent.
    pub state: EntityState,
    /// `r.currentOrigin`: where the trace ended.
    pub origin: [f32; 3],
    /// `pos1`: the plane the trace ended on, the effect's direction.
    pub normal: [f32; 3],
    /// `count`: the charge's units, one to three.
    pub count: i32,
    /// `damage` (and `splashDamage`): eight times 0.8 per unit, one for a tap.
    pub damage: i32,
    /// `r.ownerNum`.
    pub owner: u16,
    /// `genericValue5`: when it detonated; `None` until it has.
    pub detonated_at: Option<i32>,
    /// `genericValue6`: the shockwave's last radius — an integer field, so truncated.
    pub last_radius: i32,
    /// `nextthink`.
    pub next_think: i32,
    /// Its think is `G_FreeEntity` now: gone at the next frame.
    pub freeing: bool,
}

/// `WP_DEMP2_AltFire` for a player: the charge's units (`level.time` less
/// `weaponChargeTime` over 700 ms, one to three; a tap — no whole unit — is a damage of
/// one), the trace from `muzzle` 4096 units along `forward` through the world and the
/// players (`MASK_SHOT`, the shooter passed), the sphere stood where it ended with its
/// think due now (the next frame).
pub fn fire_alt(
    state: &PlayerState,
    muzzle: [f32; 3],
    forward: [f32; 3],
    level_time: i32,
    trace: &mut dyn FnMut(u16, [f32; 3], [f32; 3], u32) -> MovementTrace,
) -> Sphere {
    let units = ((level_time - state.raw_field(PS_WEAPON_CHARGE_TIME).unwrap_or(0) as i32) as f32
        / DEMP2_CHARGE_UNIT) as i32;
    let count = units.clamp(1, 3);
    let factor = (count as f32 * 0.8).max(1.0);
    let mut damage = (DEMP2_ALT_DAMAGE as f32 * factor) as i32;
    if units == 0 {
        damage = 1;
    }
    let end: [f32; 3] = std::array::from_fn(|axis| muzzle[axis] + DEMP2_ALT_RANGE * forward[axis]);
    let found = trace(state.client_num(), muzzle, end, MASK_SHOT);
    let mut entity = EntityState::zero(0, &LEGACY_ENTITY_FIELDS);
    // `G_SetOrigin`: stationary at the trace's end.
    for axis in 0..3 {
        entity.set_raw_field(ES_POS_BASE[axis], found.end_position[axis].to_bits());
    }
    entity.set_raw_field(ES_WEAPON, WP_DEMP2);
    Sphere {
        state: entity,
        origin: found.end_position,
        normal: found.plane_normal,
        count,
        damage,
        owner: state.client_num(),
        detonated_at: None,
        last_radius: 0,
        next_think: level_time,
        freeing: false,
    }
}

/// What the shockwave may reach: a thing that takes damage and has contents, with its
/// linked box (`r.absmin`, `r.absmax`) and origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShockTarget {
    pub number: u16,
    pub bounds: ([f32; 3], [f32; 3]),
    pub origin: [f32; 3],
}

/// A frame's outcome for a sphere.
#[derive(Clone, Debug, PartialEq)]
pub enum SphereRun {
    /// Its think is not due.
    Waiting,
    /// `DEMP2_AltDetonate`: the effect to raise (the count doubled as its weapon), and
    /// the expansion begins 50 ms on.
    Detonated(EventEntity),
    /// `DEMP2_AltRadiusDamage`: the shockwave moved out; `hurt` was called for what it
    /// reached.
    Expanded,
    /// Its 800 ms are up: its think is `G_FreeEntity` for the next frame.
    Done,
    /// `G_FreeEntity`: gone.
    Freed,
}

/// `G_RunThink` on the sphere at `level_time`: `DEMP2_AltDetonate` the frame after the
/// shot, then `DEMP2_AltRadiusDamage` every 50 ms — the radius the cube of the age over
/// 800 ms times 200 and the count's 0.6 (one at least); the targets at an ellipsoidal
/// distance from their box (the vertical halved) under the radius and not yet behind the
/// last edge (by sixteen per unit of count; the edge is kept as a whole number) are hurt as `MOD_DEMP2` by the owner with the
/// direction lifted twelve; the owner itself spared. `hurt` returns nothing: nothing is
/// counted for the accuracy.
pub fn run_sphere(
    sphere: &mut Sphere,
    level_time: i32,
    attacker: Attacker,
    targets: &[ShockTarget],
    hurt: &mut dyn FnMut(u16, DamageRequest),
) -> SphereRun {
    if sphere.next_think > level_time {
        return SphereRun::Waiting;
    }
    if sphere.freeing {
        return SphereRun::Freed;
    }
    let Some(started) = sphere.detonated_at else {
        // `DEMP2_AltDetonate`: a zeroed normal is made a unit along y for the effect.
        if sphere.normal == [0.0; 3] {
            sphere.normal[1] = 1.0;
        }
        let mut effect = EventEntity {
            event: EV_PLAY_EFFECT,
            parameter: EFFECT_EXPLOSION_DEMP2ALT,
            origin: sphere.origin,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        for axis in 0..3 {
            effect.extra[axis] = (ES_ANGLES[axis], sphere.normal[axis].to_bits());
            effect.extra[3 + axis] = (ES_ORIGIN[axis], sphere.origin[axis].to_bits());
        }
        // The count doubled as the effect's weapon.
        effect.extra[6] = (ES_WEAPON, (sphere.count * 2) as u32);
        sphere.detonated_at = Some(level_time);
        sphere.last_radius = 0;
        sphere.next_think = level_time + 50;
        return SphereRun::Detonated(effect);
    };
    let mut frac = (level_time - started) as f32 / 800.0;
    frac *= frac * frac;
    let mut radius = frac * 200.0;
    let factor = (sphere.count as f32 * 0.6).max(1.0);
    radius *= factor;
    for target in targets {
        if target.number == sphere.owner {
            continue;
        }
        let mut v: [f32; 3] = std::array::from_fn(|axis| {
            if sphere.origin[axis] < target.bounds.0[axis] {
                target.bounds.0[axis] - sphere.origin[axis]
            } else if sphere.origin[axis] > target.bounds.1[axis] {
                sphere.origin[axis] - target.bounds.1[axis]
            } else {
                0.0
            }
        });
        v[2] *= 0.5;
        let distance = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if distance >= radius {
            continue;
        }
        if (distance + (16 * sphere.count) as f32) < sphere.last_radius as f32 {
            continue;
        }
        let mut direction: [f32; 3] =
            std::array::from_fn(|axis| target.origin[axis] - sphere.origin[axis]);
        direction[2] += 12.0;
        hurt(
            target.number,
            DamageRequest {
                level_time,
                attacker: Some(attacker),
                direction: Some(direction),
                point: Some(sphere.origin),
                damage: sphere.damage,
                flags: 0,
                means: MOD_DEMP2,
            },
        );
    }
    sphere.last_radius = radius as i32;
    if frac < 1.0 {
        sphere.next_think = level_time + 50;
        SphereRun::Expanded
    } else {
        sphere.freeing = true;
        sphere.next_think = level_time;
        SphereRun::Done
    }
}
