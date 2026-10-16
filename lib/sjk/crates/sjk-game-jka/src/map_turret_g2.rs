//! `misc_turretG2` (OpenJK `codemp/game/g_turret_G2.c`): the Ghoul2 turret a map hangs from
//! a ceiling or stands on a floor — siege_hoth's hangar guns, siege_desert's — and the
//! TURBO turbolaser of the community maps.
//!
//! Every tenth of a second it thinks (`turretG2_base_think`): dead, it waits out its
//! respawn; switched off, it drops its enemy and stops aiming; else it looks for the
//! nearest target it sees in its radius — a client before anything else, a breakable brush
//! otherwise, never its allied team — and holds on to a client three seconds. It turns
//! toward the target's eye no faster than its turn speed (its yaw by its `apos`, its pitch
//! by a bone the clients are sent), and while it sees it, it fires from its muzzle bolt: a
//! blaster bolt of its own damage and speed, or the turbolaser's bolt from its two muzzles
//! in turn. Shot to death it explodes, fires what it targets, and — CANRESPAWN — comes back
//! whole after its `count`; the turbolaser is freed instead (`ObjectDie`).
//!
//! The level is the host's ([`TurretHost`]): what it offers as targets ([`Sighted`]), its
//! traces, the model's bolts, the indices, the events and the shots.

use crate::map_turret_world::{
    BlowAttacker, BoltIndex, CONTENTS_LIGHTSABER, DAMAGE_DEATH_KNOCKBACK, DAMAGE_HEAVY_WEAP_CLASS,
    DAMAGE_NO_KNOCKBACK, EFFECT_EXPLOSION_TURRET, ET_GENERAL, FL_NOTARGET, FRAMETIME, MASK_SHOT,
    ObjectBlow, START_DIS, Shot, Sighted, TEAM_SPECTATOR, TR_LINEAR, TurretHost, WP_BLASTER,
    WP_DEMP2, WP_TURRET, body_sound, es, find, length_squared, object_take, place, play_effect,
    play_effect_id, publish_net_health, set_enemy, sub,
};
use crate::means_of_death::{MOD_TARGET_LASER, MOD_TURBLAST, MOD_UNKNOWN};
use crate::player_angle_math::{angle_mod, angle_subtract, vector_angles};
use sjk_protocol::{EntityState, LEGACY_ENTITY_FIELDS};

/// The spawnflags (`g_turret_G2.c:33-36` and the QUAKED comment).
pub const START_OFF: u32 = 1;
pub const UPSIDE_DOWN: u32 = 2;
pub const CAN_RESPAWN: u32 = 4;
pub const TURBO: u32 = 8;
pub const LEAD_ENEMY: u32 = 16;
pub const SHOW_ON_RADAR: u32 = 32;

/// The models (`g_turret_G2.c:41-43`): the gun, its wreck, the turbolaser.
const CANNON: &[u8] = b"models/map_objects/imp_mine/turret_canon.glm";
const WRECK: &[u8] = b"models/map_objects/imp_mine/turret_damage.md3";
const TURBOLASER: &[u8] = b"models/map_objects/wedge/laser_cannon_model.glm";
/// `EF_G2ANIMATING`, `EF_RADAROBJECT`, `EF_SHADER_ANIM`; `EF2_BRACKET_ENTITY`.
const EF_G2ANIMATING: u32 = 1;
const EF_RADAROBJECT: u32 = 1 << 2;
const EF_SHADER_ANIM: u32 = 1 << 4;
const EF2_BRACKET_ENTITY: u32 = 1 << 6;
/// `(NEGATIVE_X) | (NEGATIVE_Z << 3) | (POSITIVE_Y << 6)`: `G2Tur_SetBoneAngles`' axes.
const BONE_ORIENT: u32 = 4 | (5 << 3) | (3 << 6);
/// `CONTENTS_BODY | CONTENTS_PLAYERCLIP | CONTENTS_MONSTERCLIP | CONTENTS_SHOTCLIP`.
const CONTENTS: u32 = 0x100 | 0x10 | 0x20 | 0x80;
/// `GT_SIEGE`.
const GT_SIEGE: i32 = 7;
const PITCH: usize = 0;
const YAW: usize = 1;

/// A `misc_turretG2` as the reference keeps it: its wire state whole, and the game's fields
/// its thinks read and write (named as `g_turret_G2.c` uses them).
#[derive(Clone, Debug, PartialEq)]
pub struct TurretG2 {
    /// `s`: what the clients are sent.
    pub state: EntityState,
    /// `r.currentOrigin`, `r.currentAngles`, `r.mins`, `r.maxs`, `r.contents`, `modelScale`.
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub contents: u32,
    pub model_scale: [f32; 3],
    /// `health`, `maxHealth` (non-zero: the bar is shown), `takedamage`.
    pub health: i32,
    pub max_health: i32,
    pub takes_damage: bool,
    /// `enemy`: what it is after.
    pub enemy: Option<u16>,
    /// `nextthink`; `think` is `turretG2_base_think` for as long as the turret is.
    pub next_think: i32,
    /// Whether `use`, `pain` and `die` are set (a death clears them, a respawn sets them).
    pub usable: bool,
    pub feels_pain: bool,
    pub dies: bool,
    /// `flags` (`FL_NOTARGET` while off), `spawnflags` (START_OFF toggled by a use).
    pub flags: u32,
    pub spawnflags: u32,
    /// `speed`: "really the pitch angle".
    pub speed: f32,
    /// `genericValue4` (`painwait`), `genericValue5` (when it respawns), `genericValue6`
    /// (its whole health), `genericValue8` (when its pain may fire again).
    pub pain_wait: i32,
    pub respawn_at: i32,
    pub full_health: i32,
    pub pain_ready_at: i32,
    /// `genericValue11`, `genericValue12`: its muzzle bolts.
    pub bolts: [BoltIndex; 2],
    /// `genericValue13..15`: the turbolaser's muzzle flash, shot and impact effects.
    pub effects: [i32; 3],
    /// `attackDebounceTime`, `painDebounceTime`, `aimDebounceTime`, `setTime`,
    /// `bounceCount`, `last_move_time`, `fly_sound_debounce_time` (the last shot).
    pub attack_debounce: i32,
    pub pain_debounce: i32,
    pub aim_debounce: i32,
    pub set_time: i32,
    pub bounce_count: i32,
    pub last_move_time: i32,
    pub last_shot: i32,
    /// `wait` between shots, `random` spread, `mass` (the shot's speed), `radius`.
    pub wait: f32,
    pub random: f32,
    pub mass: f32,
    pub radius: f32,
    /// `damage`, `splashDamage`, `splashRadius`, `count` (the respawn delay).
    pub damage: i32,
    pub splash_damage: i32,
    pub splash_radius: i32,
    pub count: i32,
    /// `alt_fire`: which of the turbolaser's muzzles is next.
    pub alt_fire: bool,
    /// `teamnodmg`, `alliedTeam`.
    pub team_no_damage: i32,
    pub allied_team: i32,
    /// `targetname`, `target`, `target2` (used on finding an enemy), `paintarget`.
    pub targetname: String,
    pub target: String,
    pub target2: String,
    pub pain_target: String,
    /// `ObjectDie` freed it: the host frees the entity.
    pub freed: bool,
}

impl TurretG2 {
    fn turbo(&self) -> bool {
        self.spawnflags & TURBO != 0
    }

    fn upside_down(&self) -> bool {
        self.spawnflags & UPSIDE_DOWN != 0
    }

    fn eflags(&self) -> u32 {
        self.state.raw_field(es::EFLAGS).unwrap_or(0)
    }

    fn set(&mut self, field: usize, value: u32) {
        self.state.set_raw_field(field, value);
    }
}

/// `turretG2_base_think` (`g_turret_G2.c:842-973`) for turret `me` at `level_time`, the
/// level's targets being `targets` (in entity order).
pub fn think(
    turret: &mut TurretG2,
    me: u16,
    targets: &[Sighted],
    level_time: i32,
    host: &mut dyn TurretHost,
) {
    turret.next_think = level_time + FRAMETIME;
    if turret.health <= 0 {
        if turret.spawnflags & CAN_RESPAWN != 0
            && turret.respawn_at != 0
            && turret.respawn_at < level_time
        {
            respawn(turret, me, host);
        }
        return;
    }
    if turret.spawnflags & START_OFF != 0 {
        turn_off(turret, me, level_time, host);
        aim(turret, me, targets, level_time, host);
        turret.flags |= FL_NOTARGET;
        return;
    }
    turret.flags &= !FL_NOTARGET;
    if find(targets, turret.enemy).is_none_or(|enemy| enemy.health < 0) {
        turret.enemy = None;
    }
    let mut turn_off_now = true;
    if turret.last_move_time < level_time && find_enemies(turret, me, targets, level_time, host) {
        turn_off_now = false;
        // "hold on to clients for a min of 3 seconds", anything else less.
        let client = find(targets, turret.enemy).is_some_and(|enemy| enemy.client);
        turret.last_move_time = level_time + if client { 3_000 } else { 500 };
    }
    if let Some(enemy) = find(targets, turret.enemy).copied() {
        if enemy.client
            && (enemy.session_team == TEAM_SPECTATOR || enemy.temp_spectate_until >= level_time)
        {
            turret.enemy = None;
        } else if length_squared(sub(enemy.origin, turret.origin)) < turret.radius * turret.radius
            && host.in_pvs(turret.origin, enemy.origin)
        {
            // "Every now and again, check to see if we can even trace to the enemy".
            let at = if enemy.client {
                enemy.eye
            } else {
                enemy.origin
            };
            let mut from = turret.origin;
            from[2] += if turret.upside_down() { 10.0 } else { -10.0 };
            let trace = host.trace(from, at, me, MASK_SHOT);
            if !trace.all_solid && !trace.start_solid && trace.entity_number == enemy.number {
                turn_off_now = false;
            }
        }
    }
    if turn_off_now {
        // `bounceCount` keeps it from ping-ponging between on and off.
        if turret.bounce_count < level_time {
            turn_off(turret, me, level_time, host);
        }
    } else {
        turret.bounce_count =
            (level_time as f32 + 2_000.0 + host.rng().flrand(0.0, 1.0) * 150.0) as i32;
    }
    aim(turret, me, targets, level_time, host);
    if !turn_off_now {
        head_think(turret, me, level_time, host);
    }
}

/// `turretG2_turnoff` (`g_turret_G2.c:660-683`): with an enemy, it lets go — the
/// turbolaser to its "off" animation, the gun with its shut-down sound — and pings for
/// five seconds.
fn turn_off(turret: &mut TurretG2, me: u16, level_time: i32, host: &mut dyn TurretHost) {
    if turret.enemy.is_none() {
        return;
    }
    if turret.turbo() {
        bone_anim(turret, me, 4, 5, host);
    } else {
        body_sound(host, turret.origin, b"sound/chars/turret/shutdown.wav");
    }
    turret.aim_debounce = level_time + 5_000;
    turret.enemy = None;
}

/// `turretG2_find_enemies` (`g_turret_G2.c:686-839`): the nearest target in its radius it
/// sees in the clear — a client before anything that came earlier in the list, a
/// breakable brush otherwise, never its allied team — becomes its enemy, with the wind-up
/// of a turret that had none for a while; its `target2` is used.
fn find_enemies(
    turret: &mut TurretG2,
    me: u16,
    targets: &[Sighted],
    level_time: i32,
    host: &mut dyn TurretHost,
) -> bool {
    if turret.aim_debounce > level_time && turret.pain_debounce < level_time {
        if !turret.turbo() {
            body_sound(host, turret.origin, b"sound/chars/turret/ping.wav");
        }
        turret.pain_debounce = level_time + 1_000;
    }
    let mut from = turret.origin;
    from[2] += if turret.upside_down() { 20.0 } else { -20.0 };
    let mut best_distance = turret.radius * turret.radius;
    let (mut best, mut found_client) = (None, false);
    let listed = targets.iter().filter(|target| {
        target.number != me
            && target.takes_damage
            && crate::vehicle_turrets::in_radius(target.bounds, from, turret.radius)
    });
    for target in listed {
        if !target.client && !target.breakable {
            continue;
        }
        if target.health <= 0 || target.no_target {
            continue;
        }
        if target.client
            && (target.session_team == TEAM_SPECTATOR || target.temp_spectate_until >= level_time)
        {
            continue;
        }
        if turret.allied_team != 0
            && (if target.client {
                target.session_team
            } else {
                target.team_no_damage
            }) == turret.allied_team
        {
            continue;
        }
        if !host.in_pvs(from, target.origin) {
            continue;
        }
        let mut at = if target.client {
            target.eye
        } else {
            target.origin
        };
        at[2] += if turret.upside_down() { -15.0 } else { 5.0 };
        let trace = host.trace(from, at, me, MASK_SHOT);
        if trace.all_solid
            || trace.start_solid
            || !(trace.fraction == 1.0 || trace.entity_number == target.number)
        {
            continue;
        }
        let distance = length_squared(sub(target.origin, turret.origin));
        if distance < best_distance || (target.client && !found_client) {
            if turret.attack_debounce < level_time {
                // "Wind up turrets for a bit"
                if !turret.turbo() {
                    body_sound(host, turret.origin, b"sound/chars/turret/startup.wav");
                }
                turret.attack_debounce = level_time + 1_400;
            }
            best = Some(*target);
            best_distance = distance;
            found_client |= target.client;
        }
    }
    let Some(best) = best else { return false };
    turret.enemy = set_enemy(turret.enemy, Some(&best));
    if !turret.target2.is_empty() {
        let name = turret.target2.clone();
        host.use_targets(&name, me, Some(me));
    }
    true
}

/// `turretG2_aim` (`g_turret_G2.c:494-657`): the yaw where its `apos` has it now, and with
/// an enemy a turn toward where its muzzle sees the enemy's eye (led, where it leads) —
/// the yaw by its `apos`, at most 14 degrees a think (30 the turbolaser), the pitch by its
/// bone, at most 3 (15); its turning sound while it turns.
fn aim(
    turret: &mut TurretG2,
    me: u16,
    targets: &[Sighted],
    level_time: i32,
    host: &mut dyn TurretHost,
) {
    let (max_yaw, max_pitch) = if turret.turbo() {
        (30.0, 15.0)
    } else {
        (14.0, 3.0)
    };
    turret.angles = crate::map_turret_world::evaluate_angles(&turret.state, level_time);
    turret.angles[YAW] = angle_mod(turret.angles[YAW]);
    turret.speed = angle_mod(turret.speed);
    let (mut yaw, mut pitch) = (0.0f32, 0.0f32);
    if let Some(enemy) = find(targets, turret.enemy).copied() {
        let mut at = if enemy.client {
            enemy.eye
        } else {
            enemy.origin
        };
        at[2] -= if turret.upside_down() { 15.0 } else { 5.0 };
        let base = std::array::from_fn(|axis| {
            f32::from_bits(turret.state.raw_field(es::ORIGIN[axis]).unwrap_or(0))
        });
        if turret.spawnflags & LEAD_ENEMY != 0 {
            let mut direction = sub(at, base);
            let distance = normalize(&mut direction);
            at = std::array::from_fn(|axis| {
                at[axis] + (distance / turret.mass) * enemy.velocity[axis]
            });
        }
        let bolt = turret.bolts[usize::from(turret.alt_fire)];
        let eye = crate::map_turret_world::matrix_origin_and_x(host.bolt_matrix(
            me,
            bolt,
            turret.angles,
            base,
            turret.model_scale,
        ))
        .0;
        let desired = vector_angles(sub(at, eye));
        yaw = angle_subtract(turret.angles[YAW], desired[YAW]);
        pitch = angle_subtract(turret.speed, desired[PITCH]);
    }
    if yaw != 0.0 {
        if yaw.abs() > max_yaw {
            yaw = if yaw >= 0.0 { max_yaw } else { -max_yaw };
        }
        // `VectorScale(setAngle, -5, trDelta)`: the zeros scaled too, to -0.
        let delta = [0.0, yaw, 0.0].map(|value: f32| value * -5.0);
        crate::map_turret_world::set_apos(
            &mut turret.state,
            turret.angles,
            delta,
            TR_LINEAR,
            level_time,
            0,
        );
    }
    if pitch != 0.0 {
        if pitch.abs() > max_pitch {
            turret.speed += if pitch > 0.0 { -max_pitch } else { max_pitch };
        } else {
            turret.speed -= pitch;
        }
        let (bone, angles): (&[u8], [f32; 3]) = match (turret.turbo(), turret.upside_down()) {
            (true, true) => (b"pitch", [0.0, 0.0, -turret.speed]),
            (true, false) => (b"pitch", [0.0, 0.0, turret.speed]),
            (false, true) => (b"Bone_body", [turret.speed, 0.0, 0.0]),
            (false, false) => (b"Bone_body", [-turret.speed, 0.0, 0.0]),
        };
        set_bone_angles(turret, me, bone, angles, host);
    }
    let sound = if yaw != 0.0 || pitch != 0.0 {
        host.sound_index(if turret.turbo() {
            b"sound/vehicles/weapons/turbolaser/turn.wav"
        } else {
            b"sound/chars/turret/move.wav"
        })
    } else {
        0
    };
    turret.set(es::LOOP_SOUND, u32::from(sound));
}

/// `VectorNormalize`: the length, and the vector made a unit one.
fn normalize(vector: &mut [f32; 3]) -> f32 {
    let length = length_squared(*vector).sqrt();
    if length != 0.0 {
        let inverse = 1.0 / length;
        *vector = vector.map(|value| value * inverse);
    }
    length
}

/// `turretG2_head_think` (`g_turret_G2.c:446-491`): with an enemy, its wait over and its
/// wind-up done, a shot from its muzzle bolt (along its -X, the turbolaser's +X), fifteen
/// units out.
fn head_think(turret: &mut TurretG2, me: u16, level_time: i32, host: &mut dyn TurretHost) {
    if turret.enemy.is_none()
        || turret.set_time >= level_time
        || turret.attack_debounce >= level_time
    {
        return;
    }
    turret.set_time = (level_time as f32 + turret.wait) as i32;
    let bolt = turret.bolts[usize::from(turret.alt_fire)];
    let matrix = host.bolt_matrix(me, bolt, turret.angles, turret.origin, turret.model_scale);
    if turret.turbo() {
        turret.alt_fire = !turret.alt_fire;
    }
    let (mut origin, axis) = crate::map_turret_world::matrix_origin_and_x(matrix);
    let forward = if turret.turbo() {
        axis
    } else {
        axis.map(|value| -value)
    };
    origin = std::array::from_fn(|index| origin[index] + START_DIS * forward[index]);
    fire(turret, me, origin, forward, level_time, host);
    turret.last_shot = level_time;
}

/// `turretG2_fire` (`g_turret_G2.c:353-421`): nothing from inside something solid; the
/// direction spread by `random`, the muzzle flash, and the shot — the gun's blaster bolt
/// or the turbolaser's (with its firing animation).
fn fire(
    turret: &mut TurretG2,
    me: u16,
    start: [f32; 3],
    direction: [f32; 3],
    level_time: i32,
    host: &mut dyn TurretHost,
) {
    if host.point_contents(start, me) & MASK_SHOT != 0 {
        return;
    }
    let flash_at: [f32; 3] = std::array::from_fn(|axis| start[axis] - START_DIS * direction[axis]);
    let mut direction = direction;
    if turret.random != 0.0 {
        let mut angles = vector_angles(direction);
        angles[PITCH] += host.rng().flrand(-turret.random, turret.random);
        angles[YAW] += host.rng().flrand(-turret.random, turret.random);
        direction = crate::pmove::flight::flight_axes(angles).0.to_array();
    }
    let angles = vector_angles(direction);
    if turret.turbo() {
        play_effect_id(host, turret.effects[0] as u32, flash_at, angles);
        host.launch(turbolaser_bolt(turret, me, start, direction, level_time));
        let (first, last) = if turret.alt_fire { (2, 3) } else { (0, 1) };
        bone_anim(turret, me, first, last, host);
        return;
    }
    let flash = host.effect_index(b"blaster/muzzle_flash");
    play_effect_id(host, u32::from(flash), flash_at, angles);
    let mut missile =
        crate::weapon_fire::create_missile_by(me, start, direction, turret.mass, level_time, false);
    missile.state.set_raw_field(es::WEAPON, WP_BLASTER);
    missile.free_at = level_time + 10_000;
    missile.damage = turret.damage;
    missile.splash_damage = turret.splash_damage;
    // "splashRadius = ent->splashDamage": the reference's own slip.
    missile.splash_radius = turret.splash_damage as f32;
    missile.method_of_death = MOD_TARGET_LASER;
    missile.splash_method_of_death = MOD_TARGET_LASER;
    missile.clip_mask = MASK_SHOT | CONTENTS_LIGHTSABER;
    missile.bounds = ([-1.5; 3], [1.5; 3]);
    host.launch(Shot {
        missile,
        dflags: DAMAGE_NO_KNOCKBACK | DAMAGE_HEAVY_WEAP_CLASS,
        parentless: true,
    });
}

/// `WP_FireTurboLaserMissile` (`g_weapon.c:457-490`): `CreateMissile` at the turret's shot
/// speed (as an int), its custom shot and impact effects, `WP_TURRET`, the turret's damage
/// and splash as `MOD_TURBLAST`, eight bounces it never makes, five seconds to live.
fn turbolaser_bolt(
    turret: &TurretG2,
    me: u16,
    start: [f32; 3],
    direction: [f32; 3],
    level_time: i32,
) -> Shot {
    let velocity = turret.mass as i32 as f32;
    let mut missile = crate::npc_machine_parts::create_missile(
        me, start, direction, velocity, 10_000, level_time,
    );
    let state = &mut missile.state;
    state.set_raw_field(es::OTHER_ENTITY_2, turret.effects[1] as u32);
    state.set_raw_field(es::EMPLACED_OWNER, turret.effects[2] as u32);
    state.set_raw_field(es::WEAPON, WP_TURRET);
    state.set_raw_field(es::OWNER, u32::from(me));
    missile.damage = turret.damage;
    missile.splash_damage = turret.splash_damage;
    missile.splash_radius = turret.splash_radius as f32;
    missile.method_of_death = MOD_TURBLAST;
    missile.splash_method_of_death = MOD_TURBLAST;
    missile.clip_mask = MASK_SHOT;
    missile.bounce_count = 8;
    missile.free_at = level_time + 5_000;
    missile.parent = Some(me);
    Shot {
        missile,
        dflags: DAMAGE_DEATH_KNOCKBACK,
        parentless: false,
    }
}

#[path = "map_turret_g2_life.rs"]
mod life;
use life::{bone_anim, respawn, set_bone_angles};
pub use life::{damage, spawn, used};
