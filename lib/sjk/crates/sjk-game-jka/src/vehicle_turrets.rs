//! A vehicle's turrets (`codemp/game/g_vehicleTurret.c`, `VEH_TurretThink` from the
//! vehicle's `Update`, `g_vehicles.c:1502-1507`): a turret whose passenger is aboard aims
//! where that passenger looks and fires on its attack buttons; one that thinks for itself
//! (`turretNAI`) picks the nearest target it can see in range — a client before anything
//! else — holds on to it (3 s for a client, half a second for anything else), turns toward
//! it no faster than its turn speed, within its clamps, and fires while it is on target.
//!
//! A turret turns by its bones (`NPC_SetBoneAngles` on its yaw and pitch bones), which
//! moves its muzzles on the vehicle's model: the world answers both
//! ([`TurretWorld::set_bone_angles`], [`crate::vehicle_weapons::GunneryWorld::muzzle`]).
//! What the level offers as targets is the caller's to list ([`TurretTarget`]).

use crate::damage::vector_to_angles;
use crate::player_angle_math::{angle_subtract, normalized_angle};
use crate::vehicle::Vehicle;
use crate::vehicle_fields::{TurretStats, VehicleWeaponInfo};
use crate::vehicle_weapons::{GunneryWorld, Trigger, Volley};
use crate::weapon_fire::Missile;

/// `MASK_SHOT`.
const MASK_SHOT: u32 = 0x1301;
/// `ENTITYNUM_WORLD`, `ENTITYNUM_NONE`.
const ENTITYNUM_WORLD: i32 = 1_022;
const ENTITYNUM_NONE: i32 = 1_023;
/// `BUTTON_ATTACK | BUTTON_ALT_ATTACK`.
const ATTACK_BUTTONS: u16 = 1 | 128;
/// `PITCH`, `YAW`.
const PITCH: usize = 0;
const YAW: usize = 1;

/// Something in the level a turret may take aim at: a player, an NPC (the vehicle itself
/// among them), and whatever else takes damage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurretTarget {
    /// Its entity number.
    pub number: u16,
    /// A client: a player or an NPC (`ent->client`).
    pub client: bool,
    /// `takedamage`, `health`, `FL_NOTARGET`.
    pub takes_damage: bool,
    pub health: i32,
    pub no_target: bool,
    /// A breakable brush the vehicle may break (`FL_BBRUSH`, and no other NPC's to break),
    /// or a `misc_turret` (`WP_TURRET`): what a turret shoots that is no client.
    pub shootable_thing: bool,
    /// `sess.sessionTeam` (a spectator's is 3), `tempSpectate`; `teamnodmg` for a thing.
    pub session_team: i32,
    pub temp_spectate_until: i32,
    pub team_no_damage: i32,
    /// `r.ownerNum`: a rider of the vehicle is owned by it.
    pub owner: u16,
    /// `r.currentOrigin`, `r.absmin`, `r.absmax`.
    pub origin: [f32; 3],
    pub bounds: ([f32; 3], [f32; 3]),
    /// `ps.velocity` of a client, `s.pos.trDelta` of anything else: the lead.
    pub velocity: [f32; 3],
}

/// What the passenger who works a turret gives it: where it looks (`ps.viewangles`) and
/// the buttons of its command (`pers.cmd.buttons`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gunner {
    pub view_angles: [f32; 3],
    pub buttons: u16,
}

/// The vehicle a turret thinks for, beyond its [`Vehicle`].
#[derive(Clone, Copy, Debug)]
pub struct TurretParent<'t> {
    /// Its `sess.sessionTeam`: a team's turret spares its own side.
    pub session_team: i32,
    /// `parent->enemy`, which a turret that sees nobody takes on.
    pub enemy: Option<u16>,
    /// `level.time`.
    pub level_time: i32,
    /// `m_vOrientation` as the vehicle's `Update` had it when its turrets thought.
    pub orientation: [f32; 3],
    /// Everything a turret could take aim at, in entity order.
    pub targets: &'t [TurretTarget],
}

/// What turning a turret asks of the world beyond its guns.
pub trait TurretWorld: GunneryWorld {
    /// `trap->InPVS`.
    fn in_pvs(&self, from: [f32; 3], to: [f32; 3]) -> bool;
    /// `NPC_SetBoneAngles(parent, bone, angles)`: the bone turned on the vehicle's model
    /// (the muzzles read from it follow) and named on its entity for the clients.
    fn set_bone_angles(&mut self, bone: &[u8], angles: [f32; 3]);
}

/// `VEH_TurretThink` for turret `turret` of `vehicle`. `trigger` is the vehicle as its
/// guns read it (its `ps.origin` and view, its pilot); `gunner` the passenger who works
/// the turret where it is aboard, alive and a client; `weapons` the level's weapon table.
/// The turret's projectiles are pushed onto `fired`; what else the volley did is returned.
pub fn think(
    vehicle: &mut Vehicle,
    weapons: &[VehicleWeaponInfo],
    trigger: &Trigger,
    turret: usize,
    parent: &TurretParent<'_>,
    gunner: Option<Gunner>,
    world: &mut dyn TurretWorld,
    fired: &mut Vec<Missile>,
) -> Volley {
    let info = std::sync::Arc::clone(&vehicle.info);
    let stats = &info.turrets[turret];
    let mut volley = Volley::default();
    if stats.ammo_max == 0 {
        return volley;
    }
    let Some(weapon) = usize::try_from(stats.weapon)
        .ok()
        .and_then(|index| weapons.get(index))
    else {
        return volley;
    };
    let trigger = Trigger {
        alternate: turret != 0,
        ..*trigger
    };
    // A turret whose next muzzle is none (`turretNMuzzleN` of 0, which the spawn makes -1):
    // the reference reads outside the muzzle arrays; this one does nothing.
    if !(0..crate::vehicle_fields::VEHICLE_MUZZLES as i32)
        .contains(&vehicle.turret_status[turret].next_muzzle)
    {
        return volley;
    }
    if stats.passenger_num != 0 && vehicle.passenger_count >= stats.passenger_num {
        // `VEH_TurretObeyPassengerControl`: a living client aboard in that seat.
        if let Some(gunner) = gunner {
            let muzzle = vehicle.turret_status[turret].next_muzzle;
            let mut aim = gunner.view_angles;
            aim_turret(
                vehicle,
                stats,
                parent.orientation,
                muzzle,
                None,
                weapon,
                &trigger,
                world,
                &mut aim,
            );
            if gunner.buttons & ATTACK_BUTTONS != 0 {
                check_fire(
                    vehicle,
                    stats,
                    weapon,
                    turret,
                    muzzle,
                    &trigger,
                    world,
                    fired,
                    &mut volley,
                );
            }
        }
        return volley;
    }
    if !stats.ai {
        return volley;
    }
    let range_squared = stats.ai_range * stats.ai_range;
    let muzzle = vehicle.turret_status[turret].next_muzzle;
    let find = |number: i32| {
        parent
            .targets
            .iter()
            .find(|target| i32::from(target.number) == number)
            .copied()
    };
    let mut enemy = None;
    let status = vehicle.turret_status[turret];
    if status.enemy < ENTITYNUM_WORLD {
        enemy = find(status.enemy);
        // "don't keep going after spectators, pilot, self, dead people, etc."
        let gone = enemy.is_none_or(|enemy| {
            enemy.health < 0
                || Some(enemy.number) == vehicle.pilot
                || enemy.number == trigger.number
                || enemy.owner == trigger.number
                || (enemy.client
                    && (enemy.session_team == TEAM_SPECTATOR
                        || enemy.temp_spectate_until >= parent.level_time))
        });
        if gone {
            enemy = None;
            vehicle.turret_status[turret].enemy = ENTITYNUM_NONE;
        }
    }
    let mut do_aim = false;
    if status.enemy_hold_time < parent.level_time {
        if let Some(found) = find_enemies(vehicle, stats, turret, muzzle, &trigger, parent, world) {
            enemy = Some(found);
            do_aim = true;
        } else if let Some(theirs) = parent
            .enemy
            .filter(|number| i32::from(*number) < ENTITYNUM_WORLD)
        {
            // `parent->enemy`, whatever it is: only a thing the level listed can be aimed at.
            if let Some(theirs) = find(i32::from(theirs)) {
                enemy = Some(theirs);
                do_aim = true;
            }
        }
        if let Some(enemy) = enemy {
            // "hold on to clients for a min of 3 seconds", anything else less.
            vehicle.turret_status[turret].enemy_hold_time =
                parent.level_time + if enemy.client { 3_000 } else { 500 };
        }
    }
    if let Some(target) = enemy.filter(|enemy| enemy.health > 0) {
        crate::vehicle_weapons::calc_muzzle(vehicle, muzzle as usize, &trigger, world);
        let start = vehicle.muzzle_pos[muzzle as usize];
        if length_squared(sub(target.origin, start)) < range_squared
            && world.in_pvs(start, target.origin)
        {
            // "Every now and again, check to see if we can even trace to the enemy".
            let trace = world.trace(
                start,
                [0.0; 3],
                [0.0; 3],
                target.origin,
                trigger.number,
                MASK_SHOT,
            );
            if trace.entity_number == target.number || (!trace.all_solid && !trace.start_solid) {
                do_aim = true;
            }
        }
    }
    if do_aim {
        let mut aim = [0.0; 3];
        if aim_turret(
            vehicle,
            stats,
            parent.orientation,
            muzzle,
            enemy,
            weapon,
            &trigger,
            world,
            &mut aim,
        ) {
            check_fire(
                vehicle,
                stats,
                weapon,
                turret,
                muzzle,
                &trigger,
                world,
                fired,
                &mut volley,
            );
        }
    }
    volley
}

/// `TEAM_SPECTATOR`.
const TEAM_SPECTATOR: i32 = 3;

/// `VEH_TurretFindEnemies` (`g_vehicleTurret.c:214-327`): the nearest target in range of
/// the muzzle — a client before anything else — that the turret sees in the clear; it
/// becomes the turret's enemy.
fn find_enemies(
    vehicle: &mut Vehicle,
    stats: &TurretStats,
    turret: usize,
    muzzle: i32,
    trigger: &Trigger,
    parent: &TurretParent<'_>,
    world: &mut dyn TurretWorld,
) -> Option<TurretTarget> {
    crate::vehicle_weapons::calc_muzzle(vehicle, muzzle as usize, trigger, world);
    let from = vehicle.muzzle_pos[muzzle as usize];
    let mut best_distance = stats.ai_range * stats.ai_range;
    let (mut best, mut found_client) = (None, false);
    for target in parent
        .targets
        .iter()
        .filter(|target| within(target, from, stats.ai_range))
    {
        if target.number == trigger.number
            || !target.takes_damage
            || target.health <= 0
            || target.no_target
        {
            continue;
        }
        if !target.client {
            if !target.shootable_thing {
                continue;
            }
        } else if target.session_team == TEAM_SPECTATOR
            || target.temp_spectate_until >= parent.level_time
        {
            continue;
        }
        // "don't get angry at my pilot or passengers".
        if Some(target.number) == vehicle.pilot || target.owner == trigger.number {
            continue;
        }
        if parent.session_team != 0 {
            let allied = if target.client {
                target.session_team == parent.session_team
            } else {
                target.team_no_damage == parent.session_team
            };
            if allied {
                continue;
            }
        }
        if !world.in_pvs(from, target.origin) {
            continue;
        }
        let trace = world.trace(
            from,
            [0.0; 3],
            [0.0; 3],
            target.origin,
            trigger.number,
            MASK_SHOT,
        );
        if trace.entity_number == target.number
            || (!trace.all_solid && !trace.start_solid && trace.fraction == 1.0)
        {
            let distance = length_squared(sub(target.origin, from));
            if distance < best_distance || (target.client && !found_client) {
                best = Some(*target);
                best_distance = distance;
                found_client |= target.client;
            }
        }
    }
    let best = best?;
    vehicle.turret_status[turret].enemy = i32::from(best.number);
    Some(best)
}

/// `G_RadiusList`'s test (`g_utils.c:273-331`): the target's linked box within `radius` of
/// `origin`, measured from the box's nearest edge.
fn within(target: &TurretTarget, origin: [f32; 3], radius: f32) -> bool {
    in_radius(target.bounds, origin, radius)
}

/// `G_RadiusList`'s test (`g_utils.c:273-331`) on a linked box (`r.absmin`, `r.absmax`):
/// in `EntitiesInBox`' box about `origin`, and nearer than `radius` from its nearest edge.
pub(crate) fn in_radius(bounds: ([f32; 3], [f32; 3]), origin: [f32; 3], radius: f32) -> bool {
    let radius = radius.max(1.0);
    let (low, high) = bounds;
    let low_edge =
        |axis: usize| origin[axis] - radius <= high[axis] && origin[axis] + radius >= low[axis];
    if !(0..3).all(low_edge) {
        return false;
    }
    let gap: [f32; 3] = std::array::from_fn(|axis| {
        if origin[axis] < low[axis] {
            low[axis] - origin[axis]
        } else if origin[axis] > high[axis] {
            origin[axis] - high[axis]
        } else {
            0.0
        }
    });
    length_squared(gap).sqrt() < radius
}

/// `VEH_TurretAim` (`g_vehicleTurret.c:110-212`): the turret's bones turned from where its
/// muzzle points toward `desired` (the enemy's place, led where the turret leads, or the
/// gunner's view), within its clamps and its turn speed. Whether it is on target (an enemy,
/// and no clamp held it back).
#[allow(clippy::too_many_arguments)]
fn aim_turret(
    vehicle: &mut Vehicle,
    stats: &TurretStats,
    orientation: [f32; 3],
    muzzle: i32,
    enemy: Option<TurretTarget>,
    weapon: &VehicleWeaponInfo,
    trigger: &Trigger,
    world: &mut dyn TurretWorld,
    desired: &mut [f32; 3],
) -> bool {
    let muzzle = muzzle as usize;
    crate::vehicle_weapons::calc_muzzle(vehicle, muzzle, trigger, world);
    let current = angles_subtract(angles_of(vehicle.muzzle_dir[muzzle]), orientation);
    let mut on_target = false;
    if let Some(enemy) = enemy {
        on_target = true;
        *desired = angles_to_enemy(
            vehicle.muzzle_pos[muzzle],
            weapon.speed,
            &enemy,
            stats.ai_lead,
        );
    }
    *desired = angles_subtract(*desired, orientation);
    desired[YAW] = normalized_angle(desired[YAW]);
    if stats.yaw_clamp_left != 0.0 && desired[YAW] > stats.yaw_clamp_left {
        on_target = false;
        desired[YAW] = stats.yaw_clamp_left;
    }
    if stats.yaw_clamp_right != 0.0 && desired[YAW] < stats.yaw_clamp_right {
        on_target = false;
        desired[YAW] = stats.yaw_clamp_right;
    }
    desired[PITCH] = normalized_angle(desired[PITCH]);
    if stats.pitch_clamp_down != 0.0 && desired[PITCH] > stats.pitch_clamp_down {
        on_target = false;
        desired[PITCH] = stats.pitch_clamp_down;
    }
    if stats.pitch_clamp_up != 0.0 && desired[PITCH] < stats.pitch_clamp_up {
        on_target = false;
        desired[PITCH] = stats.pitch_clamp_up;
    }
    let mut add = angles_subtract(*desired, current);
    for axis in [PITCH, YAW] {
        if add[axis] > stats.turn_speed {
            add[axis] = stats.turn_speed;
        } else if add[axis] < -stats.turn_speed {
            add[axis] = -stats.turn_speed;
        }
    }
    let new_pitch = normalized_angle(current[PITCH] + add[PITCH]);
    let new_yaw = normalized_angle(current[YAW] + add[YAW]);
    if let Some(bone) = &stats.yaw_bone {
        let mut angles = [0.0; 3];
        angles[stats.yaw_axis as usize % 3] = new_yaw;
        world.set_bone_angles(bone, angles);
    }
    if let Some(bone) = &stats.pitch_bone {
        let mut angles = [0.0; 3];
        angles[stats.pitch_axis as usize % 3] = new_pitch;
        world.set_bone_angles(bone, angles);
    }
    // "force muzzle to recalc next check".
    vehicle.muzzle_time[muzzle] = 0;
    on_target
}

/// `VEH_TurretAnglesToEnemy` (`g_vehicleTurret.c:82-108`): the world angles from the
/// muzzle to the enemy — led by its speed over the shot's flight where the turret leads.
fn angles_to_enemy(muzzle: [f32; 3], speed: f32, enemy: &TurretTarget, lead: bool) -> [f32; 3] {
    let mut at = enemy.origin;
    if lead {
        let distance = length_squared(sub(at, muzzle)).sqrt();
        let flight = distance / speed;
        at = std::array::from_fn(|axis| at[axis] + flight * enemy.velocity[axis]);
    }
    angles_of(sub(at, muzzle))
}

/// `VEH_TurretCheckFire` (`g_vehicleTurret.c:33-80`): with its muzzle's wait over and a
/// shot's ammunition, the turret fires from the muzzle (a `WP_TURRET` projectile, the
/// flash on it), spends the ammunition, and moves to its other muzzle, which waits the
/// turret's delay.
#[allow(clippy::too_many_arguments)]
fn check_fire(
    vehicle: &mut Vehicle,
    stats: &TurretStats,
    weapon: &VehicleWeaponInfo,
    turret: usize,
    muzzle: i32,
    trigger: &Trigger,
    world: &mut dyn TurretWorld,
    fired: &mut Vec<Missile>,
    volley: &mut Volley,
) {
    let at = muzzle as usize;
    if vehicle.muzzle_tags[at] == -1
        || vehicle.muzzle_wait[at] >= trigger.level_time
        || vehicle.turret_status[turret].ammo < weapon.ammo_per_shot
    {
        return;
    }
    crate::vehicle_weapons::calc_muzzle(vehicle, at, trigger, world);
    let first = fired.len();
    if let Some(missile) = crate::vehicle_weapons::projectile(
        weapon,
        stats.weapon as u32,
        vehicle.muzzle_pos[at],
        vehicle.muzzle_dir[at],
        trigger,
        true,
        world,
    ) {
        fired.push(missile);
    }
    crate::vehicle_weapons::muzzle_flash(
        trigger,
        fired.get_mut(first..).and_then(<[Missile]>::last_mut),
        1 << at,
        volley,
    );
    let status = &mut vehicle.turret_status[turret];
    status.ammo -= weapon.ammo_per_shot;
    let next = if muzzle + 1 == stats.muzzles[0] {
        stats.muzzles[1]
    } else {
        stats.muzzles[0]
    };
    if next != 0 {
        status.next_muzzle = next - 1;
    }
    let next = status.next_muzzle as usize;
    vehicle.muzzle_wait[next] = trigger.level_time + stats.delay;
}

/// `vectoangles` with its roll: pitch, yaw, 0.
fn angles_of(direction: [f32; 3]) -> [f32; 3] {
    let (pitch, yaw) = vector_to_angles(direction);
    [pitch, yaw, 0.0]
}

/// `AnglesSubtract`.
fn angles_subtract(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| angle_subtract(left[axis], right[axis]))
}

fn sub(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] - right[axis])
}

fn length_squared(vector: [f32; 3]) -> f32 {
    vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]
}
