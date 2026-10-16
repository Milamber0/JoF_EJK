//! A vehicle's guns (`codemp/game/g_weapon.c:3662-4482`): `FireVehicleWeapon` — which
//! muzzles of the weapon fire, one after another or linked all at once, their delays and
//! the ammunition they spend, `EV_NOAMMO` to a player pilot who runs dry — `WP_CalcVehMuzzle`
//! (where a muzzle is: a bolt of the vehicle's model, [`GunneryWorld::muzzle`]), the aim
//! corrected at what lies ahead (`weapNAim`, `WP_VehLeadCrosshairVeh`, a walker's
//! `WP_VehCheckTraceFromCamPos`), `WP_FireVehicleWeapon` (the projectile: its box, damage,
//! splash, owner, weapon, gravity, lifetime) and `G_VehMuzzleFireFX` (`EV_VEH_FIRE` on the
//! last projectile, naming every muzzle that fired).
//!
//! The pilot's trigger reaches here from the vehicle's move (`PM_Weapon`'s
//! `G_CheapWeaponFire`, [`crate::vehicle_update`]); the move asks for the volley
//! ([`crate::vehicle_update::VehicleRequest::Fire`]) and the game fires it once the move is
//! over, with the vehicle's state as the move left it at the trigger (nothing in the move
//! after `PM_Weapon` changes it).
//!
//! Not here yet: homing projectiles and the lock-on they need (`PM_RocketLock` for a
//! vehicle, `rocketThink` with the weapon's own turn and field of view — no vehicle locks
//! on without it, so no projectile homes); a mine's touch and its turn solid to its owner
//! (`WP_TouchVehMissile`, `WP_VehWeapSetSolidToOwner`: a projectile of no speed lies where
//! it was dropped until its life ends); projectiles that can be shot down (`iHealth`,
//! `RocketDie`); a fighter's camera trace (`BG_VehTraceFromCamPos`, only with a
//! `distanceCull` beyond 20000). Turrets fire through the same projectile and flash
//! ([`crate::vehicle_turrets`]).

use crate::event_entity::EventEntity;
use crate::pmove::MovementTrace;
use crate::vehicle::Vehicle;
use crate::vehicle_fields::{VEHICLE_MUZZLES, VehicleWeaponInfo, kind};
use crate::weapon_fire::{Missile, add_event, create_missile_by, snap_vector};

/// `EV_VEH_FIRE`, `EV_NOAMMO`.
const EV_VEH_FIRE: u32 = 24;
pub const EV_NOAMMO: u32 = 25;
/// `MOD_VEHICLE`.
const MOD_VEHICLE: u32 = 28;
/// `MASK_SHOT`, `CONTENTS_LIGHTSABER`; `MASK_SOLID | CONTENTS_SHOTCLIP`;
/// `CONTENTS_SOLID | CONTENTS_BODY`.
const MASK_SHOT: u32 = 0x1301;
const CONTENTS_LIGHTSABER: u32 = 0x4_0000;
const MASK_SOLID_SHOTCLIP: u32 = 0x81;
const MASK_CROSSHAIR: u32 = 0x101;
/// The projectile's `s.weapon` by kind.
const WP_BLASTER: u32 = 5;
const WP_DEMP2: u32 = 9;
const WP_ROCKET_LAUNCHER: u32 = 11;
const WP_THERMAL: u32 = 12;
const WP_TURRET: u32 = 18;
/// `EF_JETPACK_ACTIVE` (a vehicle's projectile), `EF_RADAROBJECT`.
const EF_JETPACK_ACTIVE: u32 = 1 << 11;
const EF_RADAROBJECT: u32 = 1 << 2;
/// `TR_GRAVITY`, `TR_STATIONARY`.
const TR_GRAVITY: u32 = 6;
/// Wire fields: `s.weapon`, `s.eFlags`, `s.pos.trType`, `s.genericenemyindex`,
/// `s.otherEntityNum2`, `s.owner`, `s.trickedentindex`.
const ES_WEAPON: usize = 14;
const ES_EFLAGS: usize = 19;
const ES_POS_TYPE: usize = 23;
const ES_GENERIC_ENEMY_INDEX: usize = 18;
const ES_OTHER_ENTITY_2: usize = 39;
const ES_OWNER: usize = 40;
const ES_TRICKED_ENTITY: usize = 58;
/// `MAX_GENTITIES`, which `s.genericenemyindex` adds to the vehicle's number (the wire's
/// convention for "an entity, not a client").
const GENTITY_OFFSET: u32 = 1_024;
/// `DEFAULT_MINS_2`.
const DEFAULT_MINS_2: f32 = -24.0;
/// `CreateMissile`'s life for a vehicle's projectile.
const PROJECTILE_LIFE_MS: i32 = 10_000;

/// `EF_INVULNERABLE`, which firing ends.
pub const EF_INVULNERABLE: u32 = 1 << 27;

/// `G_CheapWeaponFire`'s rule for a speeder's main weapon (`g_active.c:893-905`): its
/// pilot fires it only with melee in hand or its saber off (`BG_SabersOff`: both blades of
/// a staff or a pair).
pub fn pilot_may_fire(pilot: &crate::pmove::MovementState) -> bool {
    const WP_MELEE: u8 = 2;
    const WP_SABER: u8 = 3;
    const SS_DUAL: u8 = 6;
    const SS_STAFF: u8 = 7;
    let sabers_off = pilot.saber_holstered != 0
        && (!matches!(pilot.saber_anim_level_base, SS_DUAL | SS_STAFF)
            || pilot.saber_holstered >= 2);
    pilot.weapon == WP_MELEE || (pilot.weapon == WP_SABER && sabers_off)
}

/// A bolt of the vehicle's model as `G2API_GetBoltMatrix` gives it: `ORIGIN` and
/// `NEGATIVE_Y`, the way the muzzle points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MuzzleBolt {
    pub origin: [f32; 3],
    pub forward: [f32; 3],
}

impl MuzzleBolt {
    /// A model without a skeleton to pose (a Ghoul2 instance with no bones, the oracle's
    /// fake one): every bolt at the model's origin, unrotated, pointing down the world's
    /// negative y.
    pub fn unposed(origin: [f32; 3]) -> Self {
        Self {
            origin,
            forward: [-0.0, -1.0, -0.0],
        }
    }
}

/// What firing asks of the world.
pub trait GunneryWorld {
    /// `G2API_GetBoltMatrix_NoRecNoRot(ent->ghoul2, 0, tag, ..., angles, origin, level.time,
    /// NULL, modelScale)`: bolt `tag` of the vehicle's model placed at `origin` facing
    /// `angles`, as its skeleton is posed at `level_time`.
    fn muzzle(
        &mut self,
        tag: i32,
        angles: [f32; 3],
        origin: [f32; 3],
        level_time: i32,
    ) -> MuzzleBolt;
    /// `trap->Trace` past entity `pass`.
    fn trace(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        pass: u16,
        mask: u32,
    ) -> MovementTrace;
    /// `ps.velocity` of entity `number` where it is a vehicle NPC (`NPC_class ==
    /// CLASS_VEHICLE`), whose motion the aim leads.
    fn vehicle_velocity(&self, number: u16) -> Option<[f32; 3]>;
}

/// The vehicle as `FireVehicleWeapon` reads it at the trigger.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trigger {
    /// `level.time`.
    pub level_time: i32,
    /// The vehicle's entity number.
    pub number: u16,
    /// `m_pPilot`, and whether it is a player (`s.number < MAX_CLIENTS`).
    pub pilot: Option<u16>,
    pub pilot_is_player: bool,
    /// `ps.origin`, `ps.viewangles`, `ps.electrifyTime` of the vehicle.
    pub origin: [f32; 3],
    pub view_angles: [f32; 3],
    pub electrify_time: i32,
    /// `r.currentOrigin`, `r.mins`, `r.maxs`: where the vehicle was linked last.
    pub current_origin: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// `g_cullDistance` (the level's `distanceCull`), which a walker's crosshair reaches.
    pub cull_distance: f32,
    /// The alternate weapon (`BUTTON_ALT_ATTACK`).
    pub alternate: bool,
}

/// What a volley did beyond its projectiles.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Volley {
    /// `G_AddEvent(pilot, EV_NOAMMO, weaponNum)`: the player pilot is told a muzzle was
    /// ready but the weapon was dry.
    pub no_ammo: Option<u32>,
    /// The weapon whose ammunition changed: the parent's `ps.ammo[weaponNum]` takes it.
    pub spent: Option<usize>,
    /// A homing weapon fired: the parent's rocket lock is cleared.
    pub clear_lock: bool,
    /// Muzzles fired but no projectile carries the flash: `EV_VEH_FIRE` in a temp entity
    /// of its own where the vehicle is.
    pub flash: Option<EventEntity>,
}

/// `FireVehicleWeapon` (`g_weapon.c:4185-4482`): the projectiles of this trigger pushed
/// onto `fired`, in the order the reference spawns them; `weapons` is the level's weapon
/// table (`g_vehWeaponInfo`).
pub fn fire(
    vehicle: &mut Vehicle,
    weapons: &[VehicleWeaponInfo],
    trigger: &Trigger,
    world: &mut dyn GunneryWorld,
    fired: &mut Vec<Missile>,
) -> Volley {
    let mut volley = Volley::default();
    if vehicle.removed_surfaces != 0 {
        return volley;
    }
    if vehicle.kind() == kind::WALKER && trigger.electrify_time > trigger.level_time {
        return volley;
    }
    // "fighters can only fire when wings are open" (which also means it has launched).
    if vehicle.kind() == kind::FIGHTER && vehicle.flags & crate::vehicle::flags::WINGS_OPEN == 0 {
        return volley;
    }
    let first = fired.len();
    let slot = usize::from(trigger.alternate);
    let info = std::sync::Arc::clone(&vehicle.info);
    let stats = &info.weapons[slot];
    let weapon_index = stats.id;
    let level_time = trigger.level_time;
    let player_pilot = trigger.pilot.is_some() && trigger.pilot_is_player;
    let mut muzzles_fired = 0i32;
    if vehicle.weapon_status[slot].ammo <= 0 {
        // Dry: told only where one of its muzzles would have fired.
        if player_pilot
            && (0..VEHICLE_MUZZLES).any(|muzzle| {
                info.muzzle_weapons[muzzle] == weapon_index && ready(vehicle, muzzle, level_time)
            })
        {
            volley.no_ammo = Some(slot as u32);
        }
        return volley;
    }
    let delay = stats.delay;
    let linked = stats.linkable == 2 || (stats.linkable == 1 && vehicle.weapon_status[slot].linked);
    if weapon_index <= 0 || weapon_index as usize >= weapons.len() {
        return volley;
    }
    let weapon = &weapons[weapon_index as usize];
    let mut cumulative_delay = if stats.linkable == 2 { delay } else { 0 };
    let mut cumulative_ammo = 0;
    let (mut muzzles, mut muzzles_ready) = (0, 0);
    for muzzle in 0..VEHICLE_MUZZLES {
        if info.muzzle_weapons[muzzle] != weapon_index {
            continue;
        }
        if ready(vehicle, muzzle, level_time) {
            muzzles_ready += 1;
        }
        let next = vehicle.weapon_status[slot].next_muzzle;
        // The designated next muzzle is not this weapon's (a first shot): this one.
        if usize::try_from(next)
            .ok()
            .and_then(|next| info.muzzle_weapons.get(next))
            != Some(&weapon_index)
        {
            vehicle.weapon_status[slot].next_muzzle = muzzle as i32;
        }
        if linked {
            cumulative_ammo += weapon.ammo_per_shot;
            if stats.linkable != 2 {
                cumulative_delay += delay;
            }
        }
        muzzles += 1;
    }
    if linked {
        if muzzles_ready != muzzles {
            return volley;
        }
        if vehicle.weapon_status[slot].ammo < cumulative_ammo {
            if player_pilot {
                volley.no_ammo = Some(slot as u32);
            }
            return volley;
        }
    }
    let mut warned = false;
    let mut single_fired = false;
    for muzzle in 0..VEHICLE_MUZZLES {
        if info.muzzle_weapons[muzzle] != weapon_index {
            continue;
        }
        if !linked && muzzle as i32 != vehicle.weapon_status[slot].next_muzzle {
            continue;
        }
        if !ready(vehicle, muzzle, level_time) {
            continue;
        }
        if vehicle.weapon_status[slot].ammo < weapon.ammo_per_shot {
            if !warned {
                warned = true;
                if player_pilot {
                    volley.no_ammo = Some(slot as u32);
                }
            }
        } else {
            calc_muzzle(vehicle, muzzle, trigger, world);
            let start = vehicle.muzzle_pos[muzzle];
            let mut direction = vehicle.muzzle_dir[muzzle];
            if walker_crosshair(vehicle, trigger, start, &mut direction, world) {
            } else if stats.aim_correct {
                aim_at_crosshair(vehicle, trigger, start, &mut direction, world);
            }
            muzzles_fired |= 1 << muzzle;
            if let Some(missile) = projectile(
                weapon,
                weapon_index as u32,
                start,
                direction,
                trigger,
                false,
                world,
            ) {
                fired.push(missile);
            }
            if weapon.homing != 0.0 {
                volley.clear_lock = true;
            }
        }
        if linked {
            continue;
        }
        // One muzzle: its ammunition, the next of this weapon's muzzles and its delay.
        let status = &mut vehicle.weapon_status[slot];
        if muzzles > 1 {
            let start = status.next_muzzle;
            let mut next = start;
            loop {
                next += 1;
                if next >= VEHICLE_MUZZLES as i32 {
                    next = 0;
                }
                if next == start {
                    break;
                }
                if info.muzzle_weapons[next as usize] == weapon_index {
                    status.next_muzzle = next;
                    break;
                }
            }
        }
        vehicle.muzzle_wait[status.next_muzzle as usize] = level_time + delay;
        status.ammo -= weapon.ammo_per_shot;
        volley.spent = Some(slot);
        single_fired = true;
        break;
    }
    if !single_fired {
        if cumulative_ammo != 0 {
            vehicle.weapon_status[slot].ammo -= cumulative_ammo;
            volley.spent = Some(slot);
        }
        if cumulative_delay != 0 {
            for muzzle in 0..VEHICLE_MUZZLES {
                if info.muzzle_weapons[muzzle] == weapon_index {
                    vehicle.muzzle_wait[muzzle] = level_time + cumulative_delay;
                }
            }
        }
    }
    if muzzles_fired > 0 {
        muzzle_flash(
            trigger,
            fired.get_mut(first..).and_then(<[Missile]>::last_mut),
            muzzles_fired as u32,
            &mut volley,
        );
    }
    volley
}

/// A muzzle with a bolt whose wait is over (`m_iMuzzleTag[i] != -1 && m_iMuzzleWait[i] <
/// level.time`).
fn ready(vehicle: &Vehicle, muzzle: usize, level_time: i32) -> bool {
    vehicle.muzzle_tags[muzzle] != -1 && vehicle.muzzle_wait[muzzle] < level_time
}

/// `WP_CalcVehMuzzle` (`g_weapon.c:3674-3702`): the muzzle's bolt on the vehicle's model at
/// its `ps.origin`, facing its view (level for anything but a fighter), read once a frame.
pub(crate) fn calc_muzzle(
    vehicle: &mut Vehicle,
    muzzle: usize,
    trigger: &Trigger,
    world: &mut dyn GunneryWorld,
) {
    if vehicle.muzzle_time[muzzle] == trigger.level_time {
        return;
    }
    vehicle.muzzle_time[muzzle] = trigger.level_time;
    let mut angles = trigger.view_angles;
    if matches!(vehicle.kind(), kind::ANIMAL | kind::WALKER | kind::SPEEDER) {
        angles[0] = 0.0;
        angles[2] = 0.0;
    }
    let bolt = world.muzzle(
        vehicle.muzzle_tags[muzzle],
        angles,
        trigger.origin,
        trigger.level_time,
    );
    vehicle.muzzle_pos[muzzle] = bolt.origin;
    vehicle.muzzle_dir[muzzle] = bolt.forward;
}

/// `WP_VehCheckTraceFromCamPos` (`g_weapon.c:4122-4182`) for a walker driven by a player:
/// "the walker always draws the crosshair out from the first muzzle point" — straight
/// along its view from its head, `g_cullDistance` far, and the shot aimed at what that
/// meets. A fighter's (only with a `distanceCull` beyond 20000) is not ported.
fn walker_crosshair(
    vehicle: &Vehicle,
    trigger: &Trigger,
    start: [f32; 3],
    direction: &mut [f32; 3],
    world: &mut dyn GunneryWorld,
) -> bool {
    if trigger.pilot.is_none() || !trigger.pilot_is_player || vehicle.kind() != kind::WALKER {
        return false;
    }
    let forward = crate::pmove::flight::flight_axes(trigger.view_angles)
        .0
        .to_array();
    let mut from = trigger.current_origin;
    from[2] += vehicle.info.height - DEFAULT_MINS_2 - 48.0;
    let end: [f32; 3] =
        std::array::from_fn(|axis| from[axis] + trigger.cull_distance * forward[axis]);
    let trace = world.trace(
        from,
        [0.0; 3],
        [0.0; 3],
        end,
        trigger.number,
        MASK_CROSSHAIR,
    );
    let mut aim: [f32; 3] = std::array::from_fn(|axis| trace.end_position[axis] - start[axis]);
    crate::player_angle_math::normalize(&mut aim);
    *direction = aim;
    true
}

/// `weapNAim` (`g_weapon.c:4364-4389`): what lies straight ahead of the vehicle (level for a
/// speeder) from where it was linked, 32768 out, and the shot aimed at it — leading it where
/// it is a vehicle.
fn aim_at_crosshair(
    vehicle: &Vehicle,
    trigger: &Trigger,
    start: [f32; 3],
    direction: &mut [f32; 3],
    world: &mut dyn GunneryWorld,
) {
    let angles = if vehicle.kind() == kind::SPEEDER {
        [0.0, vehicle.orientation[1], 0.0]
    } else {
        vehicle.orientation
    };
    let fixed = crate::pmove::flight::flight_axes(angles).0.to_array();
    let origin = trigger.current_origin;
    let end: [f32; 3] = std::array::from_fn(|axis| origin[axis] + 32_768.0 * fixed[axis]);
    let trace = world.trace(origin, [0.0; 3], [0.0; 3], end, trigger.number, MASK_SHOT);
    if trace.fraction < 1.0 && !trace.all_solid && !trace.start_solid {
        lead_crosshair(
            world.vehicle_velocity(trace.entity_number),
            trace.end_position,
            fixed,
            start,
            direction,
        );
    }
}

/// `WP_VehLeadCrosshairVeh` (`g_weapon.c:4106-4117`): the point aimed at moved along the
/// aim by the struck vehicle's speed along it, and the shot from `start` turned to it.
fn lead_crosshair(
    velocity: Option<[f32; 3]>,
    mut end: [f32; 3],
    aim: [f32; 3],
    start: [f32; 3],
    direction: &mut [f32; 3],
) {
    if let Some(velocity) = velocity {
        let along = velocity[0] * aim[0] + velocity[1] * aim[1] + velocity[2] * aim[2];
        for axis in 0..3 {
            end[axis] += along * aim[axis];
        }
    }
    let mut shot: [f32; 3] = std::array::from_fn(|axis| end[axis] - start[axis]);
    crate::player_angle_math::normalize(&mut shot);
    *direction = shot;
}

/// `WP_FireVehicleWeapon` (`g_weapon.c:3722-3932`): a projectile weapon's missile from
/// `start` along `direction` — none for a trace weapon, which the reference leaves
/// unimplemented. `index` is the weapon's in the table (`s.otherEntityNum2`, by which a
/// client draws it).
pub fn projectile(
    weapon: &VehicleWeaponInfo,
    index: u32,
    start: [f32; 3],
    direction: [f32; 3],
    trigger: &Trigger,
    turret: bool,
    world: &mut dyn GunneryWorld,
) -> Option<Missile> {
    if !weapon.is_projectile {
        return None;
    }
    let maxs = [weapon.width / 2.0, weapon.width / 2.0, weapon.height / 2.0];
    let mins = maxs.map(|value| value * -1.0);
    let start = snap_vector(trace_set_start(trigger, start, mins, maxs, world));
    let owner = trigger.pilot.unwrap_or(trigger.number);
    let mut missile = create_missile_by(
        owner,
        start,
        direction,
        weapon.speed,
        trigger.level_time,
        false,
    );
    missile.free_at = trigger.level_time + PROJECTILE_LIFE_MS;
    missile.parent = (owner != trigger.number).then_some(trigger.number);
    let state = &mut missile.state;
    state.set_raw_field(
        ES_GENERIC_ENEMY_INDEX,
        u32::from(trigger.number) + GENTITY_OFFSET,
    );
    missile.damage = weapon.damage;
    missile.splash_damage = weapon.splash_damage;
    missile.splash_radius = weapon.splash_radius;
    missile.clip_mask = if weapon.saber_blockable {
        MASK_SHOT | CONTENTS_LIGHTSABER
    } else {
        MASK_SHOT
    };
    missile.bounds = (mins, maxs);
    missile.method_of_death = MOD_VEHICLE;
    missile.splash_method_of_death = MOD_VEHICLE;
    // "we assume it's a rocket-like thing" with a box, else "a blaster-laser-like thing"
    // that bounces (it has no `FL_BOUNCE`, so it never does).
    let mut shown = if weapon.width != 0.0 || weapon.height != 0.0 {
        WP_ROCKET_LAUNCHER
    } else {
        WP_BLASTER
    };
    missile.bounce_count = if shown == WP_ROCKET_LAUNCHER { 0 } else { 8 };
    if weapon.has_gravity {
        shown = WP_THERMAL;
        missile.state.set_raw_field(ES_POS_TYPE, TR_GRAVITY);
    }
    if weapon.ion_weapon {
        shown = WP_DEMP2;
    }
    let state = &mut missile.state;
    state.set_raw_field(ES_OWNER, u32::from(trigger.number));
    let mut entity_flags = state.raw_field(ES_EFLAGS).unwrap_or(0);
    if trigger.alternate {
        entity_flags |= 1 << 10;
    }
    if turret {
        shown = WP_TURRET;
    }
    if weapon.life_time != 0 {
        missile.explodes = weapon.explode_on_expire;
        missile.free_at = trigger.level_time + weapon.life_time;
    }
    state.set_raw_field(ES_OTHER_ENTITY_2, index);
    entity_flags |= EF_JETPACK_ACTIVE;
    if weapon.speed == 0.0 {
        // "a mine or something": it lies where it was dropped, and 3 s on
        // (`WP_VehWeapSetSolidToOwner`) its life starts, to end as its weapon says — or
        // never, with none. Its touch and its turn solid to its owner are not ported.
        shown = WP_THERMAL;
        entity_flags |= EF_RADAROBJECT;
        crate::weapon_fire::set_origin(&mut missile, start);
        (missile.free_at, missile.explodes) = match weapon.life_time {
            0 => (i32::MAX, false),
            life => (trigger.level_time + 3_000 + life, weapon.explode_on_expire),
        };
    }
    let state = &mut missile.state;
    state.set_raw_field(ES_EFLAGS, entity_flags);
    state.set_raw_field(ES_WEAPON, shown);
    // `dflags` is `DAMAGE_DEATH_KNOCKBACK`, which only a shielded target reads; the direct
    // hit's own flags are `G_MissileImpact`'s by the weapon shown.
    missile.damage_flags = if shown == WP_ROCKET_LAUNCHER {
        crate::damage::DAMAGE_HALF_ABSORB
    } else {
        0
    };
    Some(missile)
}

/// `WP_TraceSetStart` (`g_weapon.c:1592-1624`): a start outside the vehicle's own box is
/// traced to from its `ps.origin` with the projectile's box, so that it does not begin on
/// the far side of a wall.
fn trace_set_start(
    trigger: &Trigger,
    start: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    world: &mut dyn GunneryWorld,
) -> [f32; 3] {
    let low: [f32; 3] =
        std::array::from_fn(|axis| trigger.current_origin[axis] + trigger.mins[axis]);
    let high: [f32; 3] =
        std::array::from_fn(|axis| trigger.current_origin[axis] + trigger.maxs[axis]);
    let inside = (0..3).all(|axis| {
        start[axis] + maxs[axis] <= high[axis] && start[axis] + mins[axis] >= low[axis]
    });
    if inside {
        return start;
    }
    let trace = world.trace(
        trigger.origin,
        mins,
        maxs,
        start,
        trigger.number,
        MASK_SOLID_SHOTCLIP,
    );
    if trace.start_solid || trace.all_solid || trace.fraction >= 1.0 {
        return start;
    }
    trace.end_position
}

/// `G_VehMuzzleFireFX` (`g_weapon.c:3935-3965`): the muzzles fired, named on the last
/// projectile as `EV_VEH_FIRE`, or — with none — in a temp entity where the vehicle is.
pub(crate) fn muzzle_flash(
    trigger: &Trigger,
    carrier: Option<&mut Missile>,
    muzzles: u32,
    volley: &mut Volley,
) {
    match carrier {
        Some(missile) => {
            missile
                .state
                .set_raw_field(ES_OWNER, u32::from(trigger.number));
            missile.state.set_raw_field(ES_TRICKED_ENTITY, muzzles);
            add_event(missile, EV_VEH_FIRE, 0, trigger.level_time);
        }
        None => {
            let mut flash = EventEntity {
                event: EV_VEH_FIRE,
                parameter: 0,
                origin: trigger.origin,
                client: None,
                broadcast: false,
                extra: [(0, 0); 12],
            };
            flash.extra[0] = (ES_OWNER, u32::from(trigger.number));
            flash.extra[1] = (ES_TRICKED_ENTITY, muzzles);
            volley.flash = Some(flash);
        }
    }
}
