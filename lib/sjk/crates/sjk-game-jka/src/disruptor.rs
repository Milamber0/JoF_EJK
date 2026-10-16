//! The disruptor: the one instant-hit weapon so far (OpenJK `codemp/game/g_weapon.c`).
//!
//! - `WP_DisruptorMainFire` (`:557-680`): a trace from the eyes 8192 units along the
//!   view (`MASK_SHOT`), through anyone duelling somebody else or dodging, stopped by a
//!   level-3 saber that can block (`WP_SaberCanBlock`: the shot's temp entity and the
//!   block's flash, nothing else); then the shot's temp entity (`EV_DISRUPTOR_MAIN_SHOT`
//!   from the muzzle to the impact, the shooter in `eventParm`) and — unless the surface
//!   takes no impact — thirty points into what takes damage with `EV_DISRUPTOR_HIT`
//!   (weapon 1 for a client), or `EV_DISRUPTOR_SNIPER_MISS` on the world.
//! - `WP_DisruptorAltFire` (`:712-995`), the scoped shot: the charge's count (one unit
//!   every 50 ms, doubled, sixty at most — a full charge) added to seventy points; one,
//!   two or three traces (under ten, under twenty, else) each from where the last ended
//!   and passing what it struck, each with its `EV_DISRUPTOR_SNIPER_SHOT` (the previous
//!   end as its muzzle, `shouldtarget` for a full charge); a client struck is named on it,
//!   gets `EV_MISSILE_MISS` with `EF_ALT_FIRING` and counts for the accuracy, then takes
//!   the damage without knockback and `EV_DISRUPTOR_HIT`, and — killed by a full charge —
//!   is disintegrated: `EF_DISINTEGRATION`, `lastHitLoc`, its poses as before the death,
//!   its body passable, its velocity gone (the next think puts it in `PM_NOCLIP`, and no
//!   body is left for it). The world ends the traces with `EV_DISRUPTOR_SNIPER_MISS`; a
//!   surface that takes no impact ends them silently.
//! - `Jedi_DodgeEvasion` (`w_force.c:5490-5620`) is gated as the reference gates it
//!   (`g_forceDodge 1`: Force Sight active at level 3, on the ground, the weapon free):
//!   until the Force powers are ported nobody has Sight active, so nobody dodges; the
//!   dodge's own pose and speed burst are not ported.
//!
//! Held against `tools/game-oracle/disruptor.c`.

use crate::damage::{Attacker, DamageRequest};
use crate::event_entity::EventEntity;
use crate::pmove::MovementTrace;
use crate::pmove::flight::flight_axes;
use sjk_protocol::{PlayerState, legacy_direction_to_byte};

/// `EV_DISRUPTOR_MAIN_SHOT`, `EV_DISRUPTOR_SNIPER_SHOT`, `EV_DISRUPTOR_SNIPER_MISS`,
/// `EV_DISRUPTOR_HIT`, `EV_MISSILE_MISS`.
pub const EV_DISRUPTOR_MAIN_SHOT: u32 = 35;
pub const EV_DISRUPTOR_SNIPER_SHOT: u32 = 36;
pub const EV_DISRUPTOR_SNIPER_MISS: u32 = 37;
pub const EV_DISRUPTOR_HIT: u32 = 38;
const EV_MISSILE_MISS: u32 = 86;
/// `MOD_DISRUPTOR`, `MOD_DISRUPTOR_SNIPER`.
pub const MOD_DISRUPTOR: u32 = 8;
pub const MOD_DISRUPTOR_SNIPER: u32 = 10;
/// `DISRUPTOR_MAIN_DAMAGE`, `DISRUPTOR_ALT_DAMAGE - 30`, `DISRUPTOR_CHARGE_UNIT`.
const MAIN_DAMAGE: i32 = 30;
const ALT_BASE_DAMAGE: i32 = 70;
const CHARGE_UNIT: f32 = 50.0;
/// The charge's cap, and the traces a shot can go through.
const MAX_COUNT: i32 = 60;
const ALT_TRACES: usize = 3;
const SHOT_RANGE: f32 = 8_192.0;
/// `MASK_SHOT`.
const MASK_SHOT: u32 = 0x1 | 0x100 | 0x200 | 0x1000;
const SURF_NOIMPACT: u32 = 0x10;
const DAMAGE_NO_KNOCKBACK: u32 = 4;
const EF_ALT_FIRING: u32 = 1 << 10;
/// `EF_DISINTEGRATION`.
pub const EF_DISINTEGRATION: u32 = 1 << 26;
/// `FP_SEE` in `fd.forcePowersActive`.
const FP_SEE: u32 = 1 << 14;
const ENTITY_WORLD: u16 = 1_022;
const MAX_CLIENTS: u16 = 32;
/// `WP_MuzzlePoint` for the disruptor.
const MUZZLE_OFFSET: [f32; 3] = [12.0, 6.0, -6.0];

const ES_WEAPON: usize = 14;
const ES_EFLAGS: usize = 19;
const ES_ORIGIN2: [usize; 3] = [56, 60, 53];
const ES_SHOULD_TARGET: usize = 57;
const ES_OTHER_ENTITY: usize = 59;
const PS_EFLAGS: usize = 17;
/// `fd.forcePowerLevel[FP_SEE]`.
const PS_SEE_LEVEL: usize = 109;
const PS_LEGS_ANIM: usize = 13;
const PS_TORSO_ANIM: usize = 15;
/// `lastHitLoc` as `[x, y, z]`.
const PS_LAST_HIT_LOC: [usize; 3] = [102, 105, 100];

/// What a struck player is to the shot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StruckPlayer {
    /// `ps.duelInProgress` with somebody other than the shooter: the shot passes.
    pub duelling_another: bool,
    /// `fd.forcePowerLevel[FP_SABER_DEFENSE]`.
    pub saber_defense: u8,
    /// [`would_dodge`]: the shot passes as if it missed.
    pub dodges: bool,
}

/// What a blow came to, for the shot that goes on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blow {
    /// The target went from living to dead by this blow.
    pub killed: bool,
}

/// The world and the players as the shot traces and strikes them.
pub trait DisruptorTargets {
    /// `trap->Trace` from `start` to `end`, a point, passing `skip` (and what it owns).
    fn trace(&mut self, skip: u16, start: [f32; 3], end: [f32; 3], mask: u32) -> MovementTrace;
    /// A player entity's view, `None` for anything else (the world takes no damage).
    fn player(&self, number: u16) -> Option<StruckPlayer>;
    /// `WP_SaberCanBlock(other, point, 0, mod, qtrue, 0)`.
    fn can_block(&mut self, number: u16, point: [f32; 3]) -> bool;
    /// `G_Damage` on `number` (and, before it, `LogAccuracyHit` where the shot counts).
    fn hurt(&mut self, number: u16, request: DamageRequest) -> Blow;
    /// The disintegration of a player a full charge killed: the flag, the point, the
    /// poses restored, the body made passable, the velocity cleared.
    fn disintegrate(&mut self, number: u16, point: [f32; 3], poses: (u32, u32));
    /// The player's poses (`legsAnim`, `torsoAnim`) and view angles now, kept for a
    /// disintegration.
    fn poses(&self, number: u16) -> (u32, u32);
    /// `G_TempEntity`: the event entity spawned, in the order the shot makes them (a
    /// beam before the damage's own flashes, the hit after).
    fn raise(&mut self, event: EventEntity);
}

/// The shooter as the fire reads it.
pub struct Shooter<'a> {
    pub client: u16,
    pub state: &'a PlayerState,
    /// The muzzle (`CalcMuzzlePoint`, snapped), from the entity's base.
    pub muzzle: [f32; 3],
    /// The shooter as `G_Damage` reads it.
    pub attacker: Attacker,
}

impl Shooter<'_> {
    /// `CalcMuzzlePoint` for the disruptor from the entity's base `base`.
    pub fn muzzle_from(state: &PlayerState, base: [f32; 3]) -> [f32; 3] {
        let (forward, right) = flight_axes(state.view_angles());
        let (forward, right) = (forward.to_array(), right.to_array());
        let mut muzzle: [f32; 3] = std::array::from_fn(|axis| {
            base[axis] + MUZZLE_OFFSET[0] * forward[axis] + MUZZLE_OFFSET[1] * right[axis]
        });
        muzzle[2] += state.view_height() as f32 + MUZZLE_OFFSET[2];
        muzzle.map(|value| value as i32 as f32)
    }
}

/// `Jedi_DodgeEvasion`'s gate for a player: Force Sight active at level 3 (`g_forceDodge`
/// 1), alive, on the ground, the weapon free and the hand not extended. The dodge
/// itself is not ported: nobody has Sight active yet.
pub fn would_dodge(target: &PlayerState, alive: bool) -> bool {
    alive
        && target.force_powers_active() & FP_SEE != 0
        && target.ground_entity_num() != 1_023
        && target.weapon_time() <= 0
        && target.force_hand_extend() == 0
        && target.raw_field(PS_SEE_LEVEL).unwrap_or(0) >= 3
}

fn temp(event: u32, origin: [f32; 3], parameter: u32) -> EventEntity {
    EventEntity {
        event,
        parameter,
        origin,
        client: None,
        broadcast: false,
        extra: [(0, 0); 12],
    }
}

/// The shot's beam entity: from `muzzle` (its `origin2`) to `end`, the shooter named.
fn beam(event: u32, end: [f32; 3], muzzle: [f32; 3], shooter: u16) -> EventEntity {
    let mut entity = temp(event, end, u32::from(shooter));
    for axis in 0..3 {
        entity.extra[axis] = (ES_ORIGIN2[axis], muzzle[axis].to_bits());
    }
    entity
}

/// `WP_DisruptorMainFire`. `targets` is asked to trace, to block, to take the damage
/// and to spawn the temp entities.
pub fn fire_main(shooter: &Shooter, level_time: i32, targets: &mut dyn DisruptorTargets) {
    let forward = flight_axes(shooter.state.view_angles()).0.to_array();
    let mut start = shooter.state.origin();
    start[2] += shooter.state.view_height() as f32;
    let end: [f32; 3] = std::array::from_fn(|axis| start[axis] + SHOT_RANGE * forward[axis]);
    let mut ignore = shooter.client;
    let mut trace = MovementTrace::miss(end);
    for _ in 0..10 {
        trace = targets.trace(ignore, start, end, MASK_SHOT);
        let struck = trace.entity_number;
        let Some(player) = targets.player(struck) else {
            break;
        };
        if player.duelling_another || player.dodges {
            start = trace.end_position;
            ignore = struck;
            continue;
        }
        if player.saber_defense >= 3 && targets.can_block(struck, trace.end_position) {
            targets.raise(beam(
                EV_DISRUPTOR_MAIN_SHOT,
                trace.end_position,
                shooter.muzzle,
                shooter.client,
            ));
            targets.raise(block_flash(trace.end_position, trace.plane_normal));
            return;
        }
        break;
    }
    let render_impact = trace.surface_flags & SURF_NOIMPACT == 0;
    targets.raise(beam(
        EV_DISRUPTOR_MAIN_SHOT,
        trace.end_position,
        shooter.muzzle,
        shooter.client,
    ));
    if !render_impact {
        return;
    }
    let struck = trace.entity_number;
    if struck < ENTITY_WORLD && targets.player(struck).is_some() {
        targets.hurt(
            struck,
            DamageRequest {
                level_time,
                attacker: Some(shooter.attacker),
                direction: Some(forward),
                point: Some(trace.end_position),
                damage: MAIN_DAMAGE,
                flags: 0,
                means: MOD_DISRUPTOR,
            },
        );
        let mut hit = temp(
            EV_DISRUPTOR_HIT,
            trace.end_position,
            u32::from(legacy_direction_to_byte(trace.plane_normal)),
        );
        if struck < MAX_CLIENTS {
            hit.extra[0] = (ES_WEAPON, 1);
        }
        targets.raise(hit);
    } else {
        let mut miss = temp(
            EV_DISRUPTOR_SNIPER_MISS,
            trace.end_position,
            u32::from(legacy_direction_to_byte(trace.plane_normal)),
        );
        miss.extra[0] = (ES_WEAPON, 1);
        targets.raise(miss);
    }
}

/// `EV_SABER_BLOCK` as the disruptor raises it: the point as its origin, the normal as
/// its angles — a unit of yaw where there is no normal.
fn block_flash(point: [f32; 3], normal: [f32; 3]) -> EventEntity {
    let normal = if normal == [0.0; 3] {
        [0.0, 1.0, 0.0]
    } else {
        normal
    };
    let mut flash = temp(crate::saber_block::EV_SABER_BLOCK, point, 0);
    flash.extra = [
        (11, point[0].to_bits()),
        (12, point[1].to_bits()),
        (13, point[2].to_bits()),
        (25, normal[0].to_bits()),
        (9, normal[1].to_bits()),
        (24, normal[2].to_bits()),
        (0, 0),
        (0, 0),
        (0, 0),
        (0, 0),
        (0, 0),
        (0, 0),
    ];
    flash
}

/// `WP_DisruptorAltFire`.
pub fn fire_alt(shooter: &Shooter, level_time: i32, targets: &mut dyn DisruptorTargets) {
    let forward = flight_axes(shooter.state.view_angles()).0.to_array();
    let mut start = shooter.state.origin();
    start[2] += shooter.state.view_height() as f32;
    let mut count = (level_time - shooter.state.weapon_charge_time()) / CHARGE_UNIT as i32;
    count *= 2;
    let mut full_charge = false;
    if count < 1 {
        count = 1;
    } else if count >= MAX_COUNT {
        count = MAX_COUNT;
        full_charge = true;
    }
    let traces = if count < 10 {
        1
    } else if count < 20 {
        2
    } else {
        ALT_TRACES
    };
    let damage = ALT_BASE_DAMAGE + count;
    let mut skip = shooter.client;
    let mut muzzle = shooter.muzzle;
    for _ in 0..traces {
        let end: [f32; 3] = std::array::from_fn(|axis| start[axis] + SHOT_RANGE * forward[axis]);
        let trace = targets.trace(skip, start, end, MASK_SHOT);
        let struck = trace.entity_number;
        if struck == shooter.client {
            start = trace.end_position;
            skip = struck;
            continue;
        }
        let render_impact = trace.surface_flags & SURF_NOIMPACT == 0;
        let player = targets.player(struck);
        if let Some(player) = player {
            if player.duelling_another || player.dodges {
                skip = struck;
                start = trace.end_position;
                continue;
            }
            if player.saber_defense >= 3 && targets.can_block(struck, trace.end_position) {
                let mut shot = beam(
                    EV_DISRUPTOR_SNIPER_SHOT,
                    trace.end_position,
                    muzzle,
                    shooter.client,
                );
                shot.extra[3] = (ES_SHOULD_TARGET, u32::from(full_charge));
                targets.raise(shot);
                targets.raise(block_flash(trace.end_position, trace.plane_normal));
                return;
            }
        }
        let mut shot = beam(
            EV_DISRUPTOR_SNIPER_SHOT,
            trace.end_position,
            muzzle,
            shooter.client,
        );
        shot.extra[3] = (ES_SHOULD_TARGET, u32::from(full_charge));
        if !render_impact {
            targets.raise(shot);
            break;
        }
        let Some(_) = player else {
            // The world (or a mover, a glass brush: not here yet).
            targets.raise(shot);
            targets.raise(temp(
                EV_DISRUPTOR_SNIPER_MISS,
                trace.end_position,
                u32::from(legacy_direction_to_byte(trace.plane_normal)),
            ));
            break;
        };
        if struck < MAX_CLIENTS {
            shot.extra[4] = (ES_OTHER_ENTITY, u32::from(struck));
        }
        targets.raise(shot);
        if struck < MAX_CLIENTS {
            let mut miss = temp(
                EV_MISSILE_MISS,
                trace.end_position,
                u32::from(legacy_direction_to_byte(trace.plane_normal)),
            );
            miss.extra[0] = (ES_EFLAGS, EF_ALT_FIRING);
            targets.raise(miss);
        }
        let poses = targets.poses(struck);
        let blow = targets.hurt(
            struck,
            DamageRequest {
                level_time,
                attacker: Some(shooter.attacker),
                direction: Some(forward),
                point: Some(trace.end_position),
                damage,
                flags: DAMAGE_NO_KNOCKBACK,
                means: MOD_DISRUPTOR_SNIPER,
            },
        );
        if blow.killed && full_charge {
            targets.disintegrate(struck, trace.end_position, poses);
        }
        let mut hit = temp(
            EV_DISRUPTOR_HIT,
            trace.end_position,
            u32::from(legacy_direction_to_byte(trace.plane_normal)),
        );
        if struck < MAX_CLIENTS {
            hit.extra[0] = (ES_WEAPON, 1);
        }
        targets.raise(hit);
        muzzle = trace.end_position;
        start = trace.end_position;
        skip = struck;
    }
}

/// The disintegration's marks on the victim's state (`g_weapon.c:936-949`): the flag,
/// where the blow landed, the poses from before the death, the velocity gone. The
/// body's contents (`r.contents = 0`) are the caller's.
pub fn disintegrated(state: &mut PlayerState, point: [f32; 3], poses: (u32, u32)) {
    state.set_raw_field(
        PS_EFLAGS,
        state.raw_field(PS_EFLAGS).unwrap_or(0) | EF_DISINTEGRATION,
    );
    for axis in 0..3 {
        state.set_raw_field(PS_LAST_HIT_LOC[axis], point[axis].to_bits());
    }
    state.set_raw_field(PS_LEGS_ANIM, poses.0);
    state.set_raw_field(PS_TORSO_ANIM, poses.1);
    state.set_velocity([0.0; 3]);
}
