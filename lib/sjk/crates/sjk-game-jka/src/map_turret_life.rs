//! A `misc_turret`'s life beside its thinks (`g_turret.c`): the spawn of both halves, the
//! use, and `G_Damage`'s pain and death on either half.

use super::*;

/// `SP_misc_turret` with `turret_base_spawn_top` (`g_turret.c:693-891`) for the map's
/// entity, the base being entity `base` and the top the entity `top` the host spawned
/// for it, at `level_time`.
pub fn spawn(
    entity: &sjk_entity::Entity,
    base: u16,
    top: u16,
    level_time: i32,
    host: &mut dyn TurretHost,
) -> MiscTurret {
    let keys = crate::map_turret_world::Keys(entity);
    let (origin, angles) = (keys.origin(), keys.angles());
    let mut turret = MiscTurret {
        base: Half::new(origin, angles),
        top: Half::new([origin[0], origin[1], origin[2] + 128.0], angles),
        next_think: level_time + FRAMETIME * 5,
        thinking: true,
        usable: true,
        flags: 0,
        spawnflags: keys.int("spawnflags", 0) as u32,
        aim_debounce: 0,
        timestamp: 0,
        set_time: 0,
        bounce_count: 0,
        last_shot: 0,
        effects: [0; 3],
        targetname: keys.text("targetname"),
        target: keys.text("target"),
        target2: keys.text("target2"),
        healing_class: keys.text("healingclass"),
        healing_rate: keys.int("healingrate", 0),
        healing_sound: keys.text("healingsound"),
    };
    let (bottom, base_model) = (
        host.model_index(b"models/map_objects/hoth/turret_bottom.md3"),
        host.model_index(b"models/map_objects/hoth/turret_base.md3"),
    );
    let half = &mut turret.base;
    half.state
        .set_raw_field(es::MODEL_INDEX2, u32::from(bottom));
    half.state
        .set_raw_field(es::MODEL_INDEX, u32::from(base_model));
    for axis in 0..3 {
        half.state
            .set_raw_field(es::ORIGIN[axis], origin[axis].to_bits());
    }
    let icon = keys.text("icon");
    if !icon.is_empty() {
        let index = host.icon_index(icon.as_bytes());
        half.state
            .set_raw_field(es::GENERIC_ENEMY_INDEX, u32::from(index));
    }
    (half.maxs, half.mins) = ([32.0, 32.0, 128.0], [-32.0, -32.0, 0.0]);
    half.health = keys.int("health", 0);
    half.speed = keys.float("speed", 0.0);
    half.wait = keys.float("wait", 0.0);
    half.radius = keys.float("radius", 0.0);
    half.damage = keys.int("dmg", 0);
    half.team_no_damage = keys.team_no_damage();
    half.allied_team = keys.int("alliedteam", 0);
    spawn_top(&mut turret, &keys, base, top, host);
    turret
}

/// `turret_base_spawn_top` (`g_turret.c:736-891`): the top on the base, 128 units up; the
/// two linked; the defaults both share; the health bars; the effects and sounds.
pub(super) fn spawn_top(
    turret: &mut MiscTurret,
    keys: &crate::map_turret_world::Keys<'_>,
    base_number: u16,
    top_number: u16,
    host: &mut dyn TurretHost,
) {
    let (new_top, top_model) = (
        host.model_index(b"models/map_objects/hoth/turret_top_new.md3"),
        host.model_index(b"models/map_objects/hoth/turret_top.md3"),
    );
    let MiscTurret { base, top, .. } = turret;
    top.state.set_raw_field(es::MODEL_INDEX, u32::from(new_top));
    top.state
        .set_raw_field(es::MODEL_INDEX2, u32::from(top_model));
    base.owner = top_number;
    top.owner = base_number;
    top.team_no_damage = base.team_no_damage;
    top.allied_team = base.allied_team;
    base.state.set_raw_field(es::TYPE, ET_GENERAL);
    for name in [
        b"turret/explode" as &[u8],
        b"sparks/spark_exp_nosnd",
        b"turret/hoth_muzzle_flash",
    ] {
        host.effect_index(name);
    }
    top.speed = 0.0;
    // "a random time offset for the no-enemy-search-around-mode"
    top.count = (host.rng().flrand(0.0, 1.0) * 9_000.0) as i32;
    if base.health == 0 {
        base.health = 3_000;
    }
    top.health = base.health;
    if keys.int("showhealth", 0) != 0 {
        top.max_health = base.health;
        top.show_health();
        base.max_health = base.health;
        base.show_health();
    }
    base.takes_damage = true;
    base.mass = keys.float("shotspeed", 1_100.0);
    top.mass = base.mass;
    // "light the crosshair up properly over ourself": the allied team's colour, on both.
    top.state
        .set_raw_field(es::TEAM_OWNER, top.allied_team as u32);
    base.allied_team = top.allied_team;
    base.state
        .set_raw_field(es::TEAM_OWNER, top.allied_team as u32);
    base.state.set_raw_field(es::SHOULD_TARGET, 1);
    top.state.set_raw_field(es::SHOULD_TARGET, 1);
    if base.radius == 0.0 {
        base.radius = 1_024.0;
    }
    top.radius = base.radius;
    if base.wait == 0.0 {
        base.wait = 300.0 + host.rng().flrand(0.0, 1.0) * 55.0;
    }
    top.wait = base.wait;
    if base.splash_damage == 0 {
        base.splash_damage = 300;
    }
    top.splash_damage = base.splash_damage;
    if base.splash_radius == 0 {
        base.splash_radius = 128;
    }
    top.splash_radius = base.splash_radius;
    if base.damage == 0 {
        base.damage = 100;
    }
    top.damage = base.damage;
    if base.speed == 0.0 {
        base.speed = 20.0;
    }
    top.speed = base.speed;
    (top.maxs, top.mins) = ([48.0, 48.0, 16.0], [-48.0, -48.0, 0.0]);
    host.sound_index(b"sound/vehicles/weapons/hoth_turret/turn.wav");
    turret.effects = [
        b"turret/hoth_muzzle_flash" as &[u8],
        b"turret/hoth_shot",
        b"turret/hoth_impact",
    ]
    .map(|name| u32::from(host.effect_index(name)));
    top.takes_damage = true;
    host.register_weapon(WP_EMPLACED_GUN);
    top.state.set_raw_field(es::WEAPON, WP_EMPLACED_GUN);
}

/// `turret_base_use` (`g_turret.c:634-650`): switched on or off (a turret switched off has
/// stopped thinking for good).
pub fn used(turret: &mut MiscTurret) {
    if turret.usable {
        turret.spawnflags ^= START_OFF;
    }
}

/// `G_Damage` on the half `part` (`me` its number): the health taken and shown, then the
/// half's death or pain — the base's (`bottom_die`, `TurretBasePain`) hands both to the
/// top, the top's (`auto_turret_die`, `TurretPain`) keeps the base's health with its own.
/// `attacker` is the level's entry for the attacker. The health taken, or `None`.
pub fn damage(
    turret: &mut MiscTurret,
    part: Part,
    me: u16,
    blow: &ObjectBlow,
    attacker: Option<&Sighted>,
    level_time: i32,
    host: &mut dyn TurretHost,
) -> Option<i32> {
    let half = match part {
        Part::Base => &mut turret.base,
        Part::Top => &mut turret.top,
    };
    let take = object_take(me, half.takes_damage, half.team_no_damage, blow)?;
    half.health -= take;
    half.show_health();
    let attacker_number = blow.attacker.map(|attacker| attacker.number);
    if half.health <= 0 {
        half.health = half.health.max(-999);
        half.enemy = attacker_number.or(Some(crate::npc_spawn::ENTITYNUM_WORLD));
        if half.dies {
            match part {
                // `bottom_die`: a top still standing dies with it, at the base's health.
                Part::Base if turret.top.health > 0 => {
                    turret.top.health = turret.base.health;
                    turret.top.show_health();
                    top_die(turret, attacker_number, host);
                }
                Part::Base => {}
                Part::Top => top_die(turret, attacker_number, host),
            }
        }
        return Some(take);
    }
    if part == Part::Base {
        // `TurretBasePain`: the top's health is the base's, and the top feels it.
        turret.top.health = turret.base.health;
        turret.top.show_health();
    }
    top_pain(turret, blow.attacker, attacker, level_time, host);
    Some(take)
}

/// `TurretPain` (`g_turret.c:32-53`) on the top: the base's health is the top's, a DEMP2
/// hit stuns the top, and the top takes the attacker for an enemy when it had none.
pub(super) fn top_pain(
    turret: &mut MiscTurret,
    attacker: Option<BlowAttacker>,
    sighted: Option<&Sighted>,
    level_time: i32,
    host: &mut dyn TurretHost,
) {
    turret.base.health = turret.top.health;
    turret.base.show_health();
    let top = &mut turret.top;
    if attacker.is_some_and(|attacker| attacker.client && attacker.weapon == WP_DEMP2) {
        top.attack_debounce =
            (level_time as f32 + 800.0 + host.rng().flrand(0.0, 1.0) * 500.0) as i32;
        top.pain_debounce = top.attack_debounce;
    }
    if top.enemy.is_none() {
        top.enemy = set_enemy(None, sighted);
    }
}

/// `auto_turret_die` (`g_turret.c:72-130`) on the top: the base's think and use gone; the
/// top dead, its explosion and its splash (sparing the attacker); both halves' wrecks.
pub(super) fn top_die(turret: &mut MiscTurret, attacker: Option<u16>, host: &mut dyn TurretHost) {
    turret.thinking = false;
    turret.usable = false;
    let top = &mut turret.top;
    top.dies = false;
    top.takes_damage = false;
    top.health = 0;
    top.state.set_raw_field(es::HEALTH, 0);
    top.state.set_raw_field(es::LOOP_SOUND, 0);
    top.state.set_raw_field(es::SHOULD_TARGET, 0);
    let mut at = top.origin;
    at[2] += top.maxs[2] * 0.5;
    let up = [0.0, 0.0, 1.0];
    play_effect(host, EFFECT_EXPLOSION_TURRET, at, up);
    let explode = host.effect_index(b"turret/explode");
    play_effect_id(host, u32::from(explode), at, up);
    let top = &mut turret.top;
    if top.splash_damage > 0 && top.splash_radius > 0 {
        let (origin, damage, radius) = (
            top.origin,
            top.splash_damage as f32,
            top.splash_radius as f32,
        );
        host.radius_damage(origin, attacker, damage, radius, attacker, MOD_UNKNOWN);
    }
    let top = &mut turret.top;
    top.state.set_raw_field(es::WEAPON, 0);
    let wreck = top.state.raw_field(es::MODEL_INDEX2).unwrap_or(0);
    if wreck != 0 {
        top.state.set_raw_field(es::MODEL_INDEX, wreck);
        let base_wreck = turret.base.state.raw_field(es::MODEL_INDEX2).unwrap_or(0);
        if base_wreck != 0 {
            turret.base.state.set_raw_field(es::MODEL_INDEX, base_wreck);
        }
        let top = &mut turret.top;
        crate::map_turret_world::stop_apos(&mut top.state, top.angles);
    }
}
