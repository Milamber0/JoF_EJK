//! `misc_turret` (OpenJK `codemp/game/g_turret.c`): the large two-piece turbolaser of
//! siege_hoth — a base on the ground and a top that turns on it.
//!
//! Only the base thinks (`turret_base_think`), every tenth of a second: switched off, it
//! stops the top and never thinks again; with no enemy it looks for the nearest client
//! its top sees in its radius (an AT-ST before anything else), and with one it fires from
//! the top along the top's aim — `WP_EMPLACED_GUN` bolts with a splash as big as their hit
//! — while it sees it, sleeping two seconds after it lost it. The top turns toward the
//! enemy's middle by its `apos` (`TR_LINEAR_STOP` over a tenth of a second), no faster
//! than the map's `speed` a think and within 40 degrees of pitch, or sweeps back and forth
//! when it has no one. The two halves share their health: a blow on either is the other's,
//! and the top's death leaves both wrecked and the base's think gone.

use crate::map_turret_world::{
    BlowAttacker, CONTENTS_BODY, CONTENTS_LIGHTSABER, EFFECT_EXPLOSION_TURRET, EFFECT_SPARKS,
    ET_GENERAL, FL_NOTARGET, FRAMETIME, MASK_SHOT, ObjectBlow, START_DIS, Shot, Sighted,
    TEAM_SPECTATOR, TR_LINEAR_STOP, TR_STATIONARY, TurretHost, WP_DEMP2, WP_EMPLACED_GUN, es, find,
    length_squared, object_take, place, play_effect, play_effect_id, publish_net_health, set_enemy,
    sub,
};
use crate::means_of_death::{MOD_TARGET_LASER, MOD_UNKNOWN};
use crate::player_angle_math::{angle_subtract, normalized_angle, vector_angles};
use sjk_protocol::{EntityState, LEGACY_ENTITY_FIELDS};

/// START_OFF: the one spawnflag.
pub const START_OFF: u32 = 1;
/// `pitchCap`: the top never pitches further than this.
const PITCH_CAP: f32 = 40.0;
const PITCH: usize = 0;
const YAW: usize = 1;

/// One half of the turret: its wire state and the fields of it the reference reads.
#[derive(Clone, Debug, PartialEq)]
pub struct Half {
    /// `s`.
    pub state: EntityState,
    /// `r.currentOrigin`, `r.currentAngles`, `r.mins`, `r.maxs`, `r.contents`, and
    /// `r.ownerNum` (each half's is the other's number).
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub contents: u32,
    pub owner: u16,
    /// `health`, `maxHealth`, `takedamage`, and whether `die` is still set.
    pub health: i32,
    pub max_health: i32,
    pub takes_damage: bool,
    pub dies: bool,
    /// `enemy`: the base's is what it fires at; the top's is what its pain took up.
    pub enemy: Option<u16>,
    /// `attackDebounceTime`, `painDebounceTime`.
    pub attack_debounce: i32,
    pub pain_debounce: i32,
    /// `speed` (the turn a think), `count` (the top's sweep offset), `mass` (shot speed),
    /// `wait`, `radius`.
    pub speed: f32,
    pub count: i32,
    pub mass: f32,
    pub wait: f32,
    pub radius: f32,
    /// `damage`, `splashDamage`, `splashRadius`.
    pub damage: i32,
    pub splash_damage: i32,
    pub splash_radius: i32,
    /// `teamnodmg`, `alliedTeam`.
    pub team_no_damage: i32,
    pub allied_team: i32,
}

impl Half {
    fn new(origin: [f32; 3], angles: [f32; 3]) -> Self {
        let mut state = EntityState::zero(0, &LEGACY_ENTITY_FIELDS);
        place(&mut state, origin, angles);
        Self {
            state,
            origin,
            angles,
            mins: [0.0; 3],
            maxs: [0.0; 3],
            contents: CONTENTS_BODY,
            owner: 0,
            health: 0,
            max_health: 0,
            takes_damage: false,
            dies: true,
            enemy: None,
            attack_debounce: 0,
            pain_debounce: 0,
            speed: 0.0,
            count: 0,
            mass: 0.0,
            wait: 0.0,
            radius: 0.0,
            damage: 0,
            splash_damage: 0,
            splash_radius: 0,
            team_no_damage: 0,
            allied_team: 0,
        }
    }

    /// `health` into `s.health` where the bar is shown (`G_ScaleNetHealth`).
    fn show_health(&mut self) {
        if self.max_health != 0 {
            publish_net_health(&mut self.state, self.health, self.max_health);
        }
    }
}

/// Which half a blow struck.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Base,
    Top,
}

/// A `misc_turret`: the two halves, and the base's own thinking fields.
#[derive(Clone, Debug, PartialEq)]
pub struct MiscTurret {
    pub base: Half,
    pub top: Half,
    /// The base's `nextthink`, and whether its `think` is still set (the top's death
    /// clears it); its `use`.
    pub next_think: i32,
    pub thinking: bool,
    pub usable: bool,
    /// The base's `flags` and `spawnflags`.
    pub flags: u32,
    pub spawnflags: u32,
    /// The base's `aimDebounceTime`, `timestamp`, `setTime`, `bounceCount`,
    /// `fly_sound_debounce_time`.
    pub aim_debounce: i32,
    pub timestamp: i32,
    pub set_time: i32,
    pub bounce_count: i32,
    pub last_shot: i32,
    /// The top's `genericValue13..15`: its muzzle flash, shot and impact effects.
    pub effects: [u32; 3],
    /// The base's `targetname`, `target` and `target2`.
    pub targetname: String,
    pub target: String,
    pub target2: String,
    /// `healingclass`, `healingrate`, `healingsound`: siege's healers mend it with the use
    /// key (`TryHeal: see try_heal.rs`, not wired here).
    pub healing_class: String,
    pub healing_rate: i32,
    pub healing_sound: String,
}

/// `turret_base_think` (`g_turret.c:531-631`) for the base, entity `me`, at `level_time`.
pub fn think(
    turret: &mut MiscTurret,
    me: u16,
    targets: &[Sighted],
    level_time: i32,
    host: &mut dyn TurretHost,
) {
    if turret.spawnflags & START_OFF != 0 {
        turn_off(turret, level_time);
        turret.flags |= FL_NOTARGET;
        // "never think again"
        turret.next_think = -1;
        return;
    }
    turret.flags &= !FL_NOTARGET;
    turret.next_think = level_time + FRAMETIME;
    let mut turn_off_now = true;
    let enemy = find(targets, turret.base.enemy).copied();
    match enemy {
        _ if turret.base.enemy.is_none() => {
            if find_enemies(turret, me, targets, level_time, host) {
                turn_off_now = false;
            }
        }
        Some(enemy)
            if enemy.client
                && (enemy.session_team == TEAM_SPECTATOR
                    || enemy.temp_spectate_until >= level_time) =>
        {
            turret.base.enemy = None
        }
        _ => {
            if let Some(enemy) = enemy.filter(|enemy| enemy.health > 0) {
                let base = &turret.base;
                if length_squared(sub(enemy.origin, base.origin)) < base.radius * base.radius
                    && host.in_pvs(base.origin, enemy.origin)
                {
                    let at = if enemy.client {
                        enemy.eye
                    } else {
                        enemy.origin
                    };
                    let mut from = base.origin;
                    from[2] += if turret.spawnflags & 2 != 0 {
                        10.0
                    } else {
                        -10.0
                    };
                    let trace = host.trace(from, at, me, MASK_SHOT);
                    if !trace.all_solid && !trace.start_solid && trace.entity_number == enemy.number
                    {
                        turn_off_now = false;
                    }
                }
            }
            head_think(turret, level_time, host);
        }
    }
    if turn_off_now {
        if turret.bounce_count < level_time && turret.base.enemy.is_some() {
            // `turret_sleep`: "make turret play ping sound for 5 seconds".
            turret.aim_debounce = level_time + 5_000;
            turret.base.enemy = None;
        }
    } else {
        turret.bounce_count =
            (level_time as f32 + 2_000.0 + host.rng().flrand(0.0, 1.0) * 150.0) as i32;
    }
    aim(turret, targets, level_time, host);
}

/// `turret_turnoff` (`g_turret.c:377-396`): the top stopped where it stands, no turning
/// sound, no enemy.
fn turn_off(turret: &mut MiscTurret, level_time: i32) {
    let top = &mut turret.top;
    let duration = top.state.raw_field(es::APOS_DURATION).unwrap_or(0) as i32;
    crate::map_turret_world::set_apos(
        &mut top.state,
        top.angles,
        [0.0; 3],
        TR_STATIONARY,
        level_time,
        duration,
    );
    turret.base.state.set_raw_field(es::LOOP_SOUND, 0);
    turret.base.enemy = None;
}

/// `turret_find_enemies` (`g_turret.c:416-528`): the nearest client the top sees in the
/// base's radius — an AT-ST before one that is not — becomes the base's enemy, with a
/// wind-up for a turret that had none; its `target2` is used.
fn find_enemies(
    turret: &mut MiscTurret,
    me: u16,
    targets: &[Sighted],
    level_time: i32,
    host: &mut dyn TurretHost,
) -> bool {
    if turret.aim_debounce > level_time && turret.timestamp < level_time {
        turret.timestamp = level_time + 1_000;
    }
    let from = turret.top.origin;
    let radius = turret.base.radius;
    let mut best_distance = radius * radius;
    let mut best: Option<Sighted> = None;
    for target in targets.iter().filter(|target| {
        target.number != me
            && target.takes_damage
            && crate::vehicle_turrets::in_radius(target.bounds, from, radius)
    }) {
        if !target.client || target.health <= 0 || target.no_target {
            continue;
        }
        if target.session_team == TEAM_SPECTATOR || target.temp_spectate_until >= level_time {
            continue;
        }
        if turret.base.allied_team != 0 && target.session_team == turret.base.allied_team {
            continue;
        }
        if !host.in_pvs(from, target.origin) {
            continue;
        }
        let mut at = target.origin;
        at[2] += target.top * 0.5;
        let trace = host.trace(from, at, me, MASK_SHOT);
        if trace.all_solid
            || trace.start_solid
            || !(trace.fraction == 1.0 || trace.entity_number == target.number)
        {
            continue;
        }
        let distance = length_squared(sub(target.origin, turret.top.origin));
        // "target AT-STs over non-AT-STs".
        let walker_first = target.atst && best.is_some_and(|best| !best.atst);
        if distance < best_distance || walker_first {
            if turret.base.attack_debounce < level_time {
                turret.base.attack_debounce = level_time + 1_400;
            }
            best = Some(*target);
            best_distance = distance;
        }
    }
    let Some(best) = best else { return false };
    turret.base.enemy = set_enemy(turret.base.enemy, Some(&best));
    if !turret.target2.is_empty() {
        let name = turret.target2.clone();
        host.use_targets(&name, me, Some(me));
    }
    true
}

/// `turret_head_think` (`g_turret.c:201-247`): stunned, it sparks and three times in four
/// holds its fire; with an enemy, its wait over and its wind-up done, a shot from the top
/// eight units under its lid along its aim, fifteen units out.
fn head_think(turret: &mut MiscTurret, level_time: i32, host: &mut dyn TurretHost) {
    if turret.base.pain_debounce > level_time {
        play_effect(host, EFFECT_SPARKS, turret.base.origin, [0.0, 0.0, 1.0]);
        if host.rng().irand(0, 3) != 0 {
            return;
        }
    }
    if turret.base.enemy.is_none()
        || turret.set_time >= level_time
        || turret.base.attack_debounce >= level_time
    {
        return;
    }
    turret.set_time = (level_time as f32 + turret.base.wait) as i32;
    let top = &turret.top;
    let mut origin = top.origin;
    origin[2] += top.maxs[2] - 8.0;
    let forward = crate::pmove::flight::flight_axes(top.angles).0.to_array();
    origin = std::array::from_fn(|axis| origin[axis] + START_DIS * forward[axis]);
    fire(turret, origin, forward, level_time, host);
    turret.last_shot = level_time;
}

/// `turret_fire` (`g_turret.c:150-198`) from the top: nothing from inside something solid;
/// the flash (turned by the direction itself, as the reference passes it), and the bolt.
fn fire(
    turret: &MiscTurret,
    start: [f32; 3],
    direction: [f32; 3],
    level_time: i32,
    host: &mut dyn TurretHost,
) {
    let top_number = turret.base.owner;
    if host.point_contents(start, top_number) & MASK_SHOT != 0 {
        return;
    }
    let flash_at = std::array::from_fn(|axis| start[axis] - START_DIS * direction[axis]);
    play_effect_id(host, turret.effects[0], flash_at, direction);
    let top = &turret.top;
    let mut missile = crate::weapon_fire::create_missile_by(
        top_number, start, direction, top.mass, level_time, false,
    );
    let state = &mut missile.state;
    state.set_raw_field(es::OTHER_ENTITY_2, turret.effects[1]);
    state.set_raw_field(es::EMPLACED_OWNER, turret.effects[2]);
    state.set_raw_field(es::WEAPON, WP_EMPLACED_GUN);
    missile.free_at = level_time + 10_000;
    missile.damage = top.damage;
    missile.splash_damage = top.damage;
    missile.splash_radius = 100.0;
    missile.method_of_death = MOD_TARGET_LASER;
    missile.splash_method_of_death = MOD_TARGET_LASER;
    missile.clip_mask = MASK_SHOT | CONTENTS_LIGHTSABER;
    missile.bounds = ([-1.5; 3], [1.5; 3]);
    missile.parent = Some(top_number);
    host.launch(Shot {
        missile,
        dflags: 0,
        parentless: false,
    });
}

/// `turret_aim` (`g_turret.c:250-374`): the top's angles where its `apos` has them; a turn
/// toward the enemy's middle (32 units higher for a walker), or a stunned twitch, or the
/// sweep of a turret with no one — never faster than its speed, never past 40 degrees of
/// pitch — over the next tenth of a second; the turning sound while it turns.
fn aim(turret: &mut MiscTurret, targets: &[Sighted], level_time: i32, host: &mut dyn TurretHost) {
    let top = &mut turret.top;
    top.angles = crate::map_turret_world::evaluate_angles(&top.state, level_time);
    top.angles[YAW] = normalized_angle(top.angles[YAW]);
    top.angles[PITCH] = normalized_angle(top.angles[PITCH]);
    let mut turn_speed = top.speed;
    let (mut yaw, mut pitch);
    if turret.base.pain_debounce > level_time {
        let rng = host.rng();
        let desired_yaw = top.angles[YAW] + rng.flrand(-45.0, 45.0);
        let desired_pitch =
            (top.angles[PITCH] + rng.flrand(-10.0, 10.0)).clamp(-PITCH_CAP, PITCH_CAP);
        yaw = angle_subtract(desired_yaw, top.angles[YAW]);
        pitch = angle_subtract(desired_pitch, top.angles[PITCH]);
        turn_speed = host.rng().flrand(-5.0, 5.0);
    } else if let Some(enemy) = find(targets, turret.base.enemy) {
        let mut at = enemy.origin;
        at[2] += enemy.top * 0.5;
        if enemy.walker {
            at[2] += 32.0;
        }
        let mut desired = vector_angles(sub(at, top.origin));
        desired[PITCH] = normalized_angle(desired[PITCH]).clamp(-PITCH_CAP, PITCH_CAP);
        yaw = angle_subtract(desired[YAW], top.angles[YAW]);
        pitch = angle_subtract(desired[PITCH], top.angles[PITCH]);
    } else {
        // "Pan back and forth in original facing": `sin` of a float promoted to double.
        let phase = f64::from(level_time as f32 * 0.0001 + top.count as f32).sin() as f32;
        let desired_yaw = normalized_angle(phase * 60.0 + turret.base.angles[YAW]);
        yaw = angle_subtract(desired_yaw, top.angles[YAW]);
        pitch = angle_subtract(0.0, top.angles[PITCH]);
        turn_speed = 1.0;
    }
    if yaw != 0.0 && yaw.abs() > turn_speed {
        yaw = if yaw >= 0.0 { turn_speed } else { -turn_speed };
    }
    if pitch != 0.0 && pitch.abs() > turn_speed {
        pitch = if pitch > 0.0 { turn_speed } else { -turn_speed };
    }
    // `VectorScale(setAngle, 1000 / FRAMETIME, trDelta)`.
    let delta = [pitch, yaw, 0.0].map(|value| value * (1_000 / FRAMETIME) as f32);
    crate::map_turret_world::set_apos(
        &mut top.state,
        top.angles,
        delta,
        TR_LINEAR_STOP,
        level_time,
        FRAMETIME,
    );
    let sound = if yaw != 0.0 || pitch != 0.0 {
        host.sound_index(b"sound/vehicles/weapons/hoth_turret/turn.wav")
    } else {
        0
    };
    top.state.set_raw_field(es::LOOP_SOUND, u32::from(sound));
}

#[path = "map_turret_life.rs"]
mod life;
pub use life::{damage, spawn, used};
