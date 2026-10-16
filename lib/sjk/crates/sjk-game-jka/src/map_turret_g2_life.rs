//! A `misc_turretG2`'s life beside its thinks (`g_turret_G2.c`): the spawn and its
//! models, the bone it turns and the turbolaser's animation, the use, and `G_Damage`'s
//! pain, death and respawn.

use super::*;

/// `SP_misc_turretG2` with `finish_spawning_turretG2` (`g_turret_G2.c:1046-1314`) for the
/// map's entity, which is entity `me`, at `level_time` in `gametype`.
pub fn spawn(
    entity: &sjk_entity::Entity,
    me: u16,
    level_time: i32,
    gametype: i32,
    host: &mut dyn TurretHost,
) -> TurretG2 {
    let keys = crate::map_turret_world::Keys(entity);
    let spawnflags = keys.int("spawnflags", 0) as u32;
    let mut turret = TurretG2 {
        state: EntityState::zero(0, &LEGACY_ENTITY_FIELDS),
        origin: keys.origin(),
        angles: keys.angles(),
        mins: [0.0; 3],
        maxs: [0.0; 3],
        contents: 0,
        model_scale: [0.0; 3],
        health: keys.int("health", 0),
        max_health: 0,
        takes_damage: false,
        enemy: None,
        next_think: 0,
        usable: false,
        feels_pain: false,
        dies: false,
        flags: 0,
        spawnflags,
        speed: 0.0,
        pain_wait: 0,
        respawn_at: 0,
        full_health: 0,
        pain_ready_at: 0,
        bolts: [0; 2],
        effects: [0; 3],
        attack_debounce: 0,
        pain_debounce: 0,
        aim_debounce: 0,
        set_time: 0,
        bounce_count: 0,
        last_move_time: 0,
        last_shot: 0,
        wait: keys.float("wait", 0.0),
        random: keys.float("random", 0.0),
        mass: 0.0,
        radius: keys.float("radius", 0.0),
        damage: keys.int("dmg", 0),
        splash_damage: 0,
        splash_radius: 0,
        count: keys.int("count", 0),
        alt_fire: false,
        team_no_damage: keys.team_no_damage(),
        allied_team: keys.int("alliedteam", 0),
        targetname: keys.text("targetname"),
        target: keys.text("target"),
        target2: keys.text("target2"),
        pain_target: keys.text("paintarget"),
        freed: false,
    };
    turret.set(es::TEAM_OWNER, keys.int("teamowner", 0) as u32);
    set_models(&mut turret, me, false, host);
    turret.pain_wait = keys.int("painwait", 0);
    let scale = keys.int("customscale", 0).min(1_023);
    turret.set(es::MODEL_SCALE, scale as u32);
    if scale != 0 {
        turret.model_scale = [scale as f32 / 100.0; 3];
    }
    let icon = keys.text("icon");
    if !icon.is_empty() {
        let index = host.icon_index(icon.as_bytes());
        turret.set(es::GENERIC_ENEMY_INDEX, u32::from(index));
    }
    finish_spawning(&mut turret, me, &keys, level_time, gametype, host);
    turret.set(es::FRAME, u32::from(turret.spawnflags & START_OFF != 0));
    let mut eflags = turret.eflags();
    if !turret.turbo() {
        eflags |= EF_SHADER_ANIM;
    }
    if turret.spawnflags & SHOW_ON_RADAR != 0 {
        eflags |= EF_RADAROBJECT;
    }
    turret.set(es::EFLAGS, eflags);
    turret
}

/// `finish_spawning_turretG2` (`g_turret_G2.c:1101-1314`): placed (turned over and lowered
/// when upside down), the defaults of its kind, its box, its health bar and its sounds.
pub(super) fn finish_spawning(
    turret: &mut TurretG2,
    me: u16,
    keys: &crate::map_turret_world::Keys<'_>,
    level_time: i32,
    gametype: i32,
    host: &mut dyn TurretHost,
) {
    if turret.upside_down() {
        turret.angles[2] += 180.0;
        turret.origin[2] -= 22.0;
    }
    for axis in 0..3 {
        turret.set(es::ORIGIN[axis], turret.origin[axis].to_bits());
    }
    place(&mut turret.state, turret.origin, turret.angles);
    turret.set(es::TYPE, ET_GENERAL);
    host.effect_index(b"turret/explode");
    host.effect_index(b"sparks/spark_exp_nosnd");
    turret.usable = true;
    turret.feels_pain = true;
    turret.next_think = level_time + FRAMETIME * 5;
    turret.speed = 0.0;
    if turret.spawnflags & CAN_RESPAWN != 0 && turret.count == 0 {
        turret.count = 20_000;
    }
    turret.mass = keys.float("shotspeed", 0.0);
    if turret.turbo() {
        let defaults = (2.0, 20_000.0, 2_000, 32_768.0, 1_000.0, 200, 500, 500);
        apply_defaults(turret, defaults);
        turret.maxs = [64.0, 64.0, 30.0];
        turret.mins = [-64.0, -64.0, -30.0];
        // "start in "off" anim"
        bone_anim(turret, me, 4, 5, host);
        if gametype == GT_SIEGE {
            let flags = turret.state.raw_field(es::EFLAGS2).unwrap_or(0);
            turret.set(es::EFLAGS2, flags | EF2_BRACKET_ENTITY);
        }
    } else {
        // `wait` draws its spread only where the map gave none.
        let wait = if turret.wait == 0.0 {
            150.0 + host.rng().flrand(0.0, 1.0) * 55.0
        } else {
            turret.wait
        };
        apply_defaults(turret, (2.0, 1_100.0, 100, 512.0, wait, 10, 25, 5));
        if turret.upside_down() {
            (turret.maxs, turret.mins) = ([10.0, 10.0, 30.0], [-10.0, -10.0, 0.0]);
        } else {
            (turret.maxs, turret.mins) = ([10.0, 10.0, 0.0], [-10.0, -10.0, -30.0]);
        }
    }
    turret.full_health = turret.health;
    if keys.int("showhealth", 0) != 0 {
        turret.max_health = turret.health;
        publish_net_health(&mut turret.state, turret.health, turret.max_health);
        turret.set(es::SHOULD_TARGET, 1);
    }
    let scale = turret.state.raw_field(es::MODEL_SCALE).unwrap_or(0);
    if scale != 0 {
        let factor = scale as f32 / 100.0;
        turret.mins = turret.mins.map(|value| value * factor);
        turret.maxs = turret.maxs.map(|value| value * factor);
    }
    if turret.turbo() {
        turret.effects = [
            b"turret/turb_muzzle_flash" as &[u8],
            b"turret/turb_shot",
            b"turret/turb_impact",
        ]
        .map(|name| i32::from(host.effect_index(name)));
        host.sound_index(b"sound/vehicles/weapons/turbolaser/turn.wav");
    } else {
        for name in [
            b"sound/chars/turret/startup.wav" as &[u8],
            b"sound/chars/turret/shutdown.wav",
            b"sound/chars/turret/ping.wav",
            b"sound/chars/turret/move.wav",
        ] {
            host.sound_index(name);
        }
    }
    turret.contents = CONTENTS;
    turret.takes_damage = true;
    turret.dies = true;
    host.register_weapon(WP_BLASTER);
    turret.set(es::WEAPON, WP_TURRET);
}

/// A kind's defaults for what the map left at zero: `random`, `mass`, `health`, `radius`,
/// `wait`, `splashDamage`, `splashRadius`, `damage`.
pub(super) fn apply_defaults(
    turret: &mut TurretG2,
    (random, mass, health, radius, wait, splash, splash_radius, damage): (
        f32,
        f32,
        i32,
        f32,
        f32,
        i32,
        i32,
        i32,
    ),
) {
    if turret.random == 0.0 {
        turret.random = random;
    }
    if turret.mass == 0.0 {
        turret.mass = mass;
    }
    if turret.health == 0 {
        turret.health = health;
    }
    if turret.radius == 0.0 {
        turret.radius = radius;
    }
    if turret.wait == 0.0 {
        turret.wait = wait;
    }
    if turret.splash_damage == 0 {
        turret.splash_damage = splash;
    }
    if turret.splash_radius == 0 {
        turret.splash_radius = splash_radius;
    }
    if turret.damage == 0 {
        turret.damage = damage;
    }
}

/// `turretG2_set_models` (`g_turret_G2.c:142-224`): the wreck (non-turbo) and no model on the
/// server or the clients; or the gun whole again, its pitch bone at rest and its muzzles.
pub(super) fn set_models(turret: &mut TurretG2, me: u16, dying: bool, host: &mut dyn TurretHost) {
    let turbo = turret.turbo();
    if dying {
        if !turbo {
            let (wreck, gun) = (host.model_index(WRECK), host.model_index(CANNON));
            turret.set(es::MODEL_INDEX, u32::from(wreck));
            turret.set(es::MODEL_INDEX2, u32::from(gun));
        }
        host.remove_model(me);
        turret.set(es::MODEL_GHOUL2, 0);
        return;
    }
    let model = if turbo { TURBOLASER } else { CANNON };
    if turbo {
        let index = host.model_index(TURBOLASER);
        turret.set(es::MODEL_INDEX, u32::from(index));
    } else {
        let (gun, wreck) = (host.model_index(CANNON), host.model_index(WRECK));
        turret.set(es::MODEL_INDEX, u32::from(gun));
        turret.set(es::MODEL_INDEX2, u32::from(wreck));
    }
    host.init_model(me, model);
    turret.set(es::MODEL_GHOUL2, 1);
    turret.set(es::G2_RADIUS, if turbo { 128 } else { 80 });
    if turbo {
        set_bone_angles(turret, me, b"pitch", [0.0; 3], host);
        turret.bolts = [
            host.add_bolt(me, b"*muzzle1"),
            host.add_bolt(me, b"*muzzle2"),
        ];
    } else {
        set_bone_angles(turret, me, b"Bone_body", [0.0; 3], host);
        turret.bolts[0] = host.add_bolt(me, b"*flash03");
    }
}

/// `G2Tur_SetBoneAngles` (`g_turret_G2.c:46-140`): the bone's slot among the entity's four,
/// its angles there for the clients, and the bone turned on the server's instance.
pub(super) fn set_bone_angles(
    turret: &mut TurretG2,
    me: u16,
    bone: &[u8],
    angles: [f32; 3],
    host: &mut dyn TurretHost,
) {
    let index = u32::from(host.bone_index(bone));
    if !crate::npc_droid::write_bone_angles(&mut turret.state, index, angles, BONE_ORIENT) {
        return;
    }
    host.set_bone_angles(me, bone, angles);
}

/// `TurboLaser_SetBoneAnim` (`g_turret_G2.c:330-349`): the clients told to play frames
/// `start` to `end` (again, by the flip, when already playing them), and the server's
/// instance with them.
pub(super) fn bone_anim(
    turret: &mut TurretG2,
    me: u16,
    start: u32,
    end: u32,
    host: &mut dyn TurretHost,
) {
    let eflags = turret.eflags() | EF_G2ANIMATING;
    turret.set(es::EFLAGS, eflags);
    if turret.state.raw_field(es::TORSO_ANIM) == Some(start)
        && turret.state.raw_field(es::LEGS_ANIM) == Some(end)
    {
        let flip = turret.state.raw_field(es::TORSO_FLIP).unwrap_or(0);
        turret.set(es::TORSO_FLIP, u32::from(flip == 0));
    } else {
        turret.set(es::TORSO_ANIM, start);
        turret.set(es::LEGS_ANIM, end);
    }
    host.set_bone_anim(me, start as i32, end as i32);
}

/// `turretG2_respawn` (`g_turret_G2.c:423-443`): usable, hurt and killed again, its model
/// back, its whole health.
pub(super) fn respawn(turret: &mut TurretG2, me: u16, host: &mut dyn TurretHost) {
    turret.usable = true;
    turret.feels_pain = true;
    turret.dies = true;
    turret.takes_damage = true;
    turret.set(es::SHOULD_TARGET, 1);
    if turret.eflags() & EF_SHADER_ANIM != 0 {
        turret.set(es::FRAME, 0);
    }
    turret.set(es::WEAPON, WP_TURRET);
    set_models(turret, me, false, host);
    turret.health = turret.full_health;
    turret.set(es::HEALTH, turret.health as u32);
    if turret.max_health != 0 {
        publish_net_health(&mut turret.state, turret.health, turret.max_health);
    }
    turret.respawn_at = 0;
}

/// `turretG2_base_use` (`g_turret_G2.c:976-990`): switched on or off, its glow with it.
pub fn used(turret: &mut TurretG2) {
    if !turret.usable {
        return;
    }
    turret.spawnflags ^= START_OFF;
    let off = turret.eflags() & EF_SHADER_ANIM != 0 && turret.spawnflags & START_OFF != 0;
    turret.set(es::FRAME, u32::from(off));
}

/// `G_Damage` on turret `me` (`g_combat.c:4425-5516` for a thing that is no client): the
/// health taken and shown, and the death (`turretG2_die`) or the pain (`TurretG2Pain`) it
/// answers with. `attacker` is the level's entry for the attacker, for the enemy a pain
/// takes up. Returns the health taken, `None` where the blow was refused.
pub fn damage(
    turret: &mut TurretG2,
    me: u16,
    blow: &ObjectBlow,
    attacker: Option<&Sighted>,
    level_time: i32,
    host: &mut dyn TurretHost,
) -> Option<i32> {
    let take = object_take(me, turret.takes_damage, turret.team_no_damage, blow)?;
    turret.health -= take;
    if turret.max_health != 0 {
        publish_net_health(&mut turret.state, turret.health, turret.max_health);
    }
    let attacker_number = blow.attacker.map(|attacker| attacker.number);
    if turret.health <= 0 {
        turret.health = turret.health.max(-999);
        turret.enemy = attacker_number.or(Some(crate::npc_spawn::ENTITYNUM_WORLD));
        if turret.dies {
            die(turret, me, attacker_number, level_time, host);
        }
    } else if turret.feels_pain {
        pain(turret, me, blow.attacker, attacker, level_time, host);
    }
    Some(take)
}

/// `TurretG2Pain` (`g_turret_G2.c:227-250`): its `paintarget` used (once a `painwait`), a
/// DEMP2 hit's stun of two seconds and some, and the attacker taken for an enemy when it
/// had none.
pub(super) fn pain(
    turret: &mut TurretG2,
    me: u16,
    attacker: Option<BlowAttacker>,
    sighted: Option<&Sighted>,
    level_time: i32,
    host: &mut dyn TurretHost,
) {
    if !turret.pain_target.is_empty() && turret.pain_ready_at < level_time {
        let name = turret.pain_target.clone();
        host.use_targets(&name, me, Some(me));
        turret.pain_ready_at = level_time + turret.pain_wait;
    }
    if attacker.is_some_and(|attacker| attacker.client && attacker.weapon == WP_DEMP2) {
        turret.attack_debounce =
            (level_time as f32 + 2_000.0 + host.rng().flrand(0.0, 1.0) * 500.0) as i32;
        turret.pain_debounce = turret.attack_debounce;
    }
    if turret.enemy.is_none() {
        turret.enemy = set_enemy(None, sighted);
    }
}

/// `turretG2_die` (`g_turret_G2.c:253-325`): nothing more to use, hurt or kill; the
/// explosion (and its splash, sparing the attacker); the wreck — its targets used, its
/// respawn set — or, the turbolaser having none, `ObjectDie`: its targets used, freed.
pub(super) fn die(
    turret: &mut TurretG2,
    me: u16,
    attacker: Option<u16>,
    level_time: i32,
    host: &mut dyn TurretHost,
) {
    turret.usable = false;
    turret.dies = false;
    turret.feels_pain = false;
    turret.takes_damage = false;
    turret.health = 0;
    turret.set(es::HEALTH, 0);
    turret.set(es::LOOP_SOUND, 0);
    turret.set(es::SHOULD_TARGET, 0);
    let forward = if turret.upside_down() {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 0.0, -1.0]
    };
    let at = std::array::from_fn(|axis| turret.origin[axis] + 12.0 * forward[axis]);
    play_effect(host, EFFECT_EXPLOSION_TURRET, at, forward);
    if turret.splash_damage > 0 && turret.splash_radius > 0 {
        host.radius_damage(
            turret.origin,
            attacker,
            turret.splash_damage as f32,
            turret.splash_radius as f32,
            attacker,
            MOD_UNKNOWN,
        );
    }
    if turret.eflags() & EF_SHADER_ANIM != 0 {
        turret.set(es::FRAME, 1);
    }
    turret.set(es::WEAPON, 0);
    if turret.state.raw_field(es::MODEL_INDEX2).unwrap_or(0) == 0 {
        // `ObjectDie`.
        if !turret.target.is_empty() {
            let name = turret.target.clone();
            host.use_targets(&name, me, attacker);
        }
        turret.freed = true;
        return;
    }
    set_models(turret, me, true, host);
    crate::map_turret_world::stop_apos(&mut turret.state, turret.angles);
    if !turret.target.is_empty() {
        let name = turret.target.clone();
        host.use_targets(&name, me, attacker);
    }
    if turret.spawnflags & CAN_RESPAWN != 0 && turret.health < 1 && turret.respawn_at == 0 {
        turret.respawn_at = level_time + turret.count;
    }
}
