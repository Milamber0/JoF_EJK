//! An NPC begun: `NPC_Begin` (`codemp/game/NPC_spawn.c:862-1270`) and what it calls —
//! `NPC_SpotWouldTelefrag`, `NPC_SetWeapons`, `NPC_SetMiscDefaultData`, `G_KillBox`,
//! `G_CheckInSolid` — a frame after the spawn ([`crate::npc_spawn`]).
//!
//! In its order: an NPC that would stand in another body (a player's or an NPC's) waits
//! `wait` and tries again, or, with a `wait` below zero, fires its `target3` and frees
//! itself. Otherwise the player state is set up (spawn count, client number, health from
//! the map or the definition, the difficulty's tweaks, the weapons the team carries), the
//! entity made solid and shown, whoever it lands on telefragged, the default animation
//! set, the think handed to `NPC_Think`, the class and team defaults applied
//! (`NPC_SetMiscDefaultData`), the health bar filled, and the NPC lifted out of the floor.
//!
//! The reference ends `NPC_Begin` with a first think (`ClientThink` with an empty command:
//! `Pmove` over up to a second, and the entity converted from the player state); the
//! begin leaves the NPC ready for it — its eyes, its skeleton's animations, off the
//! ground — and the roster runs it ([`crate::npc_roster`]).
//!
//! Held to `tools/game-oracle/npcspawn.c` (`game-npcspawn.txt`).

use crate::npc_spawn::{
    EF_NODRAW, ENTITYNUM_NONE, FL_NO_KNOCKBACK, FL_NOTARGET, FL_SHIELDED, FRAMETIME, NpcActor,
    NpcHost, NpcThink, PERS_TEAM, es,
};
use crate::npc_spawners::{BOBA, Registration};

/// Wire fields of `playerState_t` the begin writes.
pub(crate) mod ps {
    pub const LEGS_ANIM: usize = 13;
    pub const TORSO_ANIM: usize = 15;
    pub const FORCE_POWER: usize = 18;
    pub const SABER_ANIM_LEVEL: usize = 23;
    pub const ROCKET_LOCK_INDEX: usize = 24;
    pub const WEAPON_STATE: usize = 33;
    pub const PM_FLAGS: usize = 38;
    pub const PM_TIME: usize = 41;
    pub const CLIENT_NUM: usize = 43;
    pub const GRAVITY: usize = 46;
    pub const WEAPON: usize = 47;
    pub const FORCE_KNOWN: usize = 51;
    pub const ROCKET_LOCK_TIME: usize = 79;
    pub const SABER_HOLSTERED: usize = 81;
    pub const SABER_ENTITY_NUM: usize = 31;
    pub const EFLAGS2: usize = 103;
    pub const CUSTOM_RGBA: [usize; 4] = [29, 42, 45, 32];
    pub const STAND_HEIGHT: usize = 35;
    pub const CROUCH_HEIGHT: usize = 36;
    pub const MODEL_SCALE: usize = 123;
}

/// `statIndex_t`: `STAT_HEALTH`, `STAT_WEAPONS`, `STAT_ARMOR`, `STAT_MAX_HEALTH`.
pub(crate) const STAT_HEALTH: usize = 0;
pub(crate) const STAT_WEAPONS: usize = 4;
const STAT_ARMOR: usize = 5;
pub(crate) const STAT_MAX_HEALTH: usize = 8;
/// `persistant[PERS_SPAWN_COUNT]`.
const PERS_SPAWN_COUNT: usize = 4;
/// `powerups[PW_CLOAKED]`, `Q3_INFINITE`.
const PW_CLOAKED: usize = 11;
const Q3_INFINITE: u32 = 16_777_216;
/// `CONTENTS_BODY`, `MASK_NPCSOLID`.
pub const CONTENTS_BODY: u32 = 0x100;
pub const MASK_NPCSOLID: u32 = 0x1 | 0x20 | 0x100 | 0x1000;
/// The spawn flags the begin reads: `SFB_CINEMATIC`, `SFB_NOTSOLID` (also the reference's
/// bare `64`), `SFB_STARTINSOLID`, `JSF_AMBUSH`.
const SFB_CINEMATIC: i32 = 32;
const SFB_NOTSOLID: i32 = 64;
const SFB_STARTINSOLID: i32 = 128;
const JSF_AMBUSH: i32 = 16;
/// `PMF_TIME_KNOCKBACK`, `PMF_RESPAWNED`; `WEAPON_IDLE`.
const PMF_TIME_KNOCKBACK: u32 = 64;
const PMF_RESPAWNED: u32 = 512;
const WEAPON_IDLE: u32 = 6;
/// `BOTH_STAND1`: the animation an NPC begins in.
pub const BOTH_STAND1: u32 = 915;
/// `bState_t`: `BS_DEFAULT`, `BS_CINEMATIC`.
const BS_DEFAULT: i32 = 0;
const BS_CINEMATIC: i32 = 9;
/// `npcteam_t`: `NPCTEAM_ENEMY`, `NPCTEAM_PLAYER`, `NPCTEAM_NEUTRAL`.
const NPCTEAM_ENEMY: i32 = 1;
const NPCTEAM_PLAYER: i32 = 2;
const NPCTEAM_NEUTRAL: i32 = 3;
/// `SCF_ALT_FIRE`, `SCF_IGNORE_ALERTS`, `SCF_DONT_FIRE`, `SCF_NO_GROUPS`, `SCF_NO_FORCE`.
const SCF_ALT_FIRE: u32 = 0x40;
const SCF_IGNORE_ALERTS: u32 = 0x2000;
const SCF_DONT_FIRE: u32 = 0x4000;
const SCF_NO_GROUPS: u32 = 0x2_0000;
const SCF_NO_FORCE: u32 = 0x20_0000;
/// `NPCAI_CUSTOM_GRAVITY`; `EF2_FLYING`; `SVF_PLAYER_USABLE`.
const NPCAI_CUSTOM_GRAVITY: u32 = 0x20_0000;
const EF2_FLYING: u32 = 1 << 4;
const SVF_PLAYER_USABLE: u32 = 0x10;
/// `FORCE_POWER_MAX`, `FP_LEVITATION`, `FORCE_LEVEL_3`.
const FORCE_POWER_MAX: u32 = 100;
const FP_LEVITATION: usize = 1;
/// `GT_HOLOCRON`, `GT_SIEGE`; `SIEGETEAM_TEAM1`, `SIEGETEAM_TEAM2`.
const GT_HOLOCRON: i32 = 1;
const GT_SIEGE: i32 = 7;
/// `EV_GENERAL_SOUND`; `saberEntityNum`, where `G_Sound` keeps the channel; `CHAN_ITEM`.
const EV_GENERAL_SOUND: u32 = 76;
const ES_SABER_ENTITY: usize = 37;
const CHAN_ITEM: u32 = 5;
/// `GALAK_SHIELD_HEALTH` (`NPC_AI_GalakMech.c:44`).
const GALAK_SHIELD_HEALTH: u32 = 500;
/// `weapon_t`s the begin reads, and `WP_NUM_WEAPONS`.
const WP_STUN_BATON: i32 = 1;
const WP_SABER: i32 = 3;
const WP_BRYAR_PISTOL: i32 = 4;
const WP_BLASTER: i32 = 5;
const WP_DISRUPTOR: i32 = 6;
const WP_BOWCASTER: i32 = 7;
const WP_REPEATER: i32 = 8;
const WP_DEMP2: i32 = 9;
const WP_FLECHETTE: i32 = 10;
const WP_ROCKET_LAUNCHER: i32 = 11;
const WP_THERMAL: i32 = 12;
const WP_NUM_WEAPONS: i32 = 19;

/// `class_t` values the begin tests (`teams.h:40-97`).
mod class {
    pub const ATST: i32 = 1;
    pub const GONK: i32 = 11;
    pub const INTERROGATOR: i32 = 16;
    pub const JEDI: i32 = 18;
    pub const KYLE: i32 = 19;
    pub const LUKE: i32 = 22;
    pub const MARK1: i32 = 23;
    pub const MOUSE: i32 = 29;
    pub const PROBE: i32 = 32;
    pub const PROTOCOL: i32 = 33;
    pub const R2D2: i32 = 34;
    pub const R5D2: i32 = 35;
    pub const REBORN: i32 = 37;
    pub const REMOTE: i32 = 39;
    pub const SEEKER: i32 = 41;
    pub const SENTRY: i32 = 42;
    pub const SHADOWTROOPER: i32 = 43;
    pub const STORMTROOPER: i32 = 44;
    pub const SWAMPTROOPER: i32 = 46;
    pub const IMPWORKER: i32 = 15;
    pub const TAVION: i32 = 47;
    pub const DESANN: i32 = 6;
    pub const BOBAFETT: i32 = 52;
    pub const VEHICLE: i32 = 53;
    pub const RANCOR: i32 = 54;
}

/// What a begin came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Begun {
    /// Begun: standing, shown, solid.
    Begun,
    /// Another body stands there: the begin is tried again at this time.
    Retry(i32),
    /// Another body stands there and it will not wait: its `target3` (if any) fired, the
    /// NPC freed a frame on.
    GaveUp(Option<Vec<u8>>),
}

/// `NPC_Begin(npc)` at `level_time`. `others` are the other NPCs, as the reference's
/// entity queries see them.
pub fn begin(
    npc: &mut NpcActor,
    others: &[&NpcActor],
    level_time: i32,
    host: &mut impl NpcHost,
    telefrags: &mut Vec<u16>,
) -> Begun {
    if npc.spawnflags & SFB_NOTSOLID == 0 && spot_would_telefrag(npc, others, host) {
        if npc.wait < 0.0 {
            npc.think = NpcThink::Free(level_time + FRAMETIME);
            return Begun::GaveUp(npc.target3.clone());
        }
        // `level.time + ent->wait`: a float, truncated to the think time.
        let at = (level_time as f32 + npc.wait) as i32;
        npc.think = NpcThink::Begin(at);
        return Begun::Retry(at);
    }
    let spawn_origin = npc.player.origin();
    let mut spawn_angles = angles_of(npc);
    spawn_angles[1] = npc.desired_yaw;
    npc.player.persistent[PERS_SPAWN_COUNT] =
        npc.player.persistent[PERS_SPAWN_COUNT].wrapping_add(1);
    npc.player
        .set_raw_field(ps::CLIENT_NUM, u32::from(npc.number));
    set_health(npc, host);
    tweak_stats(npc, host);
    npc.state
        .set_raw_field(es::GROUND_ENTITY, u32::from(ENTITYNUM_NONE));
    npc.takes_damage = true;
    (npc.contents, npc.clip_mask) = if npc.spawnflags & SFB_NOTSOLID == 0 {
        (CONTENTS_BODY, MASK_NPCSOLID)
    } else {
        (0, MASK_NPCSOLID & !CONTENTS_BODY)
    };
    npc.player
        .set_raw_field(ps::ROCKET_LOCK_INDEX, u32::from(ENTITYNUM_NONE));
    npc.player.set_raw_field(ps::ROCKET_LOCK_TIME, 0);
    let class = npc.definition.client_class;
    // Droids stay out of the targeting (`FL_NOTARGET`).
    if ![
        class::R2D2,
        class::R5D2,
        class::MOUSE,
        class::GONK,
        class::PROTOCOL,
    ]
    .contains(&class)
    {
        npc.flags &= !FL_NOTARGET;
    }
    let eflags = npc.state.raw_field(es::EFLAGS).unwrap_or(0);
    npc.state.set_raw_field(es::EFLAGS, eflags & !EF_NODRAW);
    // `NPC_SetFX_SpawnStates`: the world's gravity, unless the NPC flies by its own.
    if npc.ai_flags & NPCAI_CUSTOM_GRAVITY == 0 {
        npc.player
            .set_raw_field(ps::GRAVITY, host.gravity() as i32 as u32);
    }
    if npc.player.weapon() == 0 {
        set_weapons(npc);
    }
    npc.player.set_raw_field(ps::WEAPON_STATE, WEAPON_IDLE);
    let (weapon, skill) = (npc.player.weapon(), host.skill());
    crate::npc_combat::change_weapon(npc, weapon, skill);
    npc.player.set_origin(spawn_origin);
    or_player(npc, ps::PM_FLAGS, PMF_RESPAWNED);
    for axis in 0..3 {
        npc.state
            .set_raw_field(es::ORIGIN[axis], spawn_origin[axis].to_bits());
    }
    set_view_angle(npc, spawn_angles);
    npc.mind.look_target = ENTITYNUM_NONE;
    if npc.spawnflags & SFB_NOTSOLID == 0 {
        kill_box(npc, others, host, telefrags);
    }
    or_player(npc, ps::PM_FLAGS, PMF_TIME_KNOCKBACK);
    npc.player.set_raw_field(ps::PM_TIME, 100);
    // A seeker somebody made is theirs (`NPC_spawn.c:1085-1088`, `ItemUse_Seeker`).
    npc.state.set_raw_field(
        es::OWNER,
        u32::from(owner_of(npc).unwrap_or(ENTITYNUM_NONE)),
    );
    if class != class::VEHICLE {
        // `NPC_SetAnim(SETANIM_BOTH, BOTH_STAND1)` over a state with no timers running.
        npc.player.set_raw_field(ps::LEGS_ANIM, BOTH_STAND1);
        npc.player.set_raw_field(ps::TORSO_ANIM, BOTH_STAND1);
    }
    if npc.definition.entity_class != class::VEHICLE || host.gametype() != GT_SIEGE {
        npc.player.persistent[PERS_TEAM] = npc.player_team as u32;
    }
    // `NPC_Think` a frame on and up to 100 ms more.
    npc.think = NpcThink::Think(level_time + FRAMETIME + host.irand(0, 100));
    // `ent->pain = NPC_PainFunc(ent)`, which `NPC_SetMiscDefaultData` overrides for the
    // wampa and the rancor (`NPC_spawn.c:1119`, `256-271`).
    misc_default_data(npc, host, level_time);
    npc.mind.fight.pain = crate::npc_pain::pain_func(npc);
    if npc.health <= 0 {
        npc.health = npc.max_health;
    }
    npc.player.stats[STAT_HEALTH] = npc.health as u32;
    if npc.state.raw_field(es::SHOULD_TARGET).unwrap_or(0) != 0 {
        npc.bar_max_health = npc.health;
        scale_net_health(npc);
    }
    // "yes, again... sigh" (`NPC_spawn.c:1153`).
    let (weapon, skill) = (npc.player.weapon(), host.skill());
    crate::npc_combat::change_weapon(npc, weapon, skill);
    if npc.spawnflags & SFB_STARTINSOLID == 0 {
        check_in_solid(npc, others, host);
    }
    // No waypoint and no home one (`NPC_spawn.c:1199`).
    npc.mind.tactics.waypoint = crate::npc_mind::WAYPOINT_NONE;
    npc.mind.tactics.home_waypoint = crate::npc_mind::WAYPOINT_NONE;
    // No spawn script. The eyes are where it stands now, and stay there
    // (`renderInfo.eyePoint`); it is off the ground for the first think, which its caller
    // runs ([`crate::npc_world::NpcWorld::client_think`]).
    npc.mind.eye_point = npc.current_origin;
    npc.player.set_ground_entity_num(ENTITYNUM_NONE);
    // A vehicle animates with its definition's model (`BG_GetVehicleModelName`).
    let model = npc
        .vehicle
        .as_ref()
        .and_then(|vehicle| vehicle.info.model.clone())
        .unwrap_or_else(|| npc.definition.player_model.clone());
    let (lengths, humanoid) = host.npc_animations(&model);
    if let Some(lengths) = lengths {
        npc.movement.set_animation_lengths(lengths);
    }
    npc.humanoid = humanoid;
    crate::npc_creature::add_render_bolts(npc, host);
    host.publish(npc.number, &npc.state, npc.bounds(), npc.contents);
    Begun::Begun
}

/// `s.angles`.
fn angles_of(npc: &NpcActor) -> [f32; 3] {
    es::ANGLES.map(|index| f32::from_bits(npc.state.raw_field(index).unwrap_or(0)))
}

/// A player-state field with bits added.
fn or_player(npc: &mut NpcActor, index: usize, bits: u32) {
    let value = npc.player.raw_field(index).unwrap_or(0);
    npc.player.set_raw_field(index, value | bits);
}

/// `SetClientViewAngle` (`g_client.c:1171-1183`): the delta angles against a command that
/// has none, `s.angles` and the view.
fn set_view_angle(npc: &mut NpcActor, angles: [f32; 3]) {
    // `ANGLE2SHORT`: `(int)(x * 65536 / 360) & 65535`, in floats.
    let delta = angles.map(|angle| ((angle * 65_536.0 / 360.0) as i32) & 65_535);
    npc.player.set_delta_angles(delta);
    for axis in 0..3 {
        npc.state
            .set_raw_field(es::ANGLES[axis], angles[axis].to_bits());
    }
    npc.player.set_view_angles(angles);
}

/// The health `NPC_Begin` gives (`NPC_spawn.c:918-941`): the map's, else the definition's
/// (raised by a quarter per difficulty level for everyone but Jedi), else 100.
fn set_health(npc: &mut NpcActor, host: &impl NpcHost) {
    let class = npc.definition.client_class;
    npc.max_health = if npc.health != 0 {
        npc.health
    } else if npc.definition.stats.health != 0 {
        if ![class::REBORN, class::SHADOWTROOPER, class::JEDI].contains(&class) {
            let stats = &mut npc.definition.stats;
            stats.health += stats.health / 4 * host.skill();
        }
        npc.definition.stats.health
    } else {
        100
    };
    npc.player.stats[STAT_MAX_HEALTH] = npc.max_health as u32;
}

/// The difficulty's tweaks to aim and turning (`NPC_spawn.c:943-1000`), with their draws.
fn tweak_stats(npc: &mut NpcActor, host: &mut impl NpcHost) {
    let class = npc.definition.client_class;
    let skill = host.skill();
    let stats = &mut npc.definition.stats;
    if npc.npc_type.eq_ignore_ascii_case(b"rodian") {
        match skill {
            0 => stats.aim = 1,
            1 => stats.aim = host.irand(2, 3),
            2 => stats.aim = host.irand(3, 4),
            _ => {}
        }
    } else if [class::STORMTROOPER, class::SWAMPTROOPER, class::IMPWORKER].contains(&class)
        || npc.npc_type.eq_ignore_ascii_case(b"rodian2")
    {
        let worker = class == class::IMPWORKER;
        match skill {
            0 => {
                stats.yaw_speed *= 0.75;
                if worker {
                    stats.aim -= host.irand(3, 6);
                }
            }
            1 if worker => stats.aim -= host.irand(2, 4),
            2 => {
                stats.yaw_speed *= 1.5;
                if worker {
                    stats.aim -= host.irand(0, 2);
                }
            }
            _ => {}
        }
    } else if class == class::REBORN || class == class::SHADOWTROOPER {
        match skill {
            1 => stats.yaw_speed *= 1.25,
            2 => stats.yaw_speed *= 1.5,
            _ => {}
        }
    }
}

/// `NPC_SetWeapons` (`NPC_spawn.c:759-801`): the weapons the team and name carry, each
/// with 100 rounds, and the best of them in hand — the saber over anything, the stun
/// baton only for want of anything else.
fn set_weapons(npc: &mut NpcActor) {
    let weapons =
        crate::npc_precache::weapons_for_team(npc.player_team, npc.spawnflags, &npc.npc_type);
    npc.player.stats[STAT_WEAPONS] = 0;
    let mut best = 0;
    for weapon in WP_SABER..WP_NUM_WEAPONS {
        if weapons & (1 << weapon) == 0 {
            continue;
        }
        npc.player.stats[STAT_WEAPONS] |= 1 << weapon;
        if let Some(data) = crate::weapon_data::legacy_weapon_data(weapon as u8) {
            npc.player.ammo[data.ammo_index] = 100;
        }
        if best == WP_SABER {
            continue;
        }
        if weapon == WP_STUN_BATON {
            if best == 0 {
                best = weapon;
            }
        } else if weapon > best || best == WP_STUN_BATON {
            best = weapon;
        }
    }
    npc.player.set_raw_field(ps::WEAPON, best as u32);
}

/// Every other body the NPC's box meets (`EntitiesInBox` over the NPC's own box against
/// the others' linked boxes, grown by a unit): the other NPCs' by their bounds.
fn meets(npc: &NpcActor, other: &NpcActor, low: [f32; 3], high: [f32; 3]) -> bool {
    other.number != npc.number
        && (0..3).all(|axis| {
            other.current_origin[axis] + other.mins[axis] - 1.0 <= high[axis]
                && other.current_origin[axis] + other.maxs[axis] + 1.0 >= low[axis]
        })
}

/// `NPC_SpotWouldTelefrag` (`NPC_spawn.c:831-860`): a player or another NPC with a body
/// where this one would stand.
fn spot_would_telefrag(npc: &NpcActor, others: &[&NpcActor], host: &mut impl NpcHost) -> bool {
    let low: [f32; 3] = std::array::from_fn(|axis| npc.current_origin[axis] + npc.mins[axis]);
    let high: [f32; 3] = std::array::from_fn(|axis| npc.current_origin[axis] + npc.maxs[axis]);
    let owner = owner_of(npc);
    others.iter().any(|other| {
        other.contents & MASK_NPCSOLID != 0
            && Some(other.number) != owner
            && !crate::vehicle_droid::owned_by(other, npc.number)
            && meets(npc, other, low, high)
    }) || match owner {
        Some(owner) => host.player_in_box_but(low, high, owner),
        None => host.player_in_box(low, high),
    }
}

/// `r.ownerNum` of an NPC as it begins: a player's seeker's player (`ItemUse_Seeker`) or a
/// droid unit's vehicle ([`crate::vehicle_droid`]), whom its begin neither waits for nor
/// telefrags; nobody's for any other NPC.
fn owner_of(npc: &NpcActor) -> Option<u16> {
    npc.mind
        .creature
        .activator
        .filter(|_| npc.definition.client_class == class::SEEKER)
        .or_else(|| crate::vehicle_droid::riding_owner(npc))
}

/// `G_KillBox` (`g_utils.c:1166-1197`) from the player state's origin: the players there
/// telefragged by the host, and the other NPCs there — a body lying where it begins among
/// them — named in `telefrags` for the roster to strike (`G_Damage` for 100000,
/// `DAMAGE_NO_PROTECTION`, `MOD_TELEFRAG`) once the begin is over.
fn kill_box(
    npc: &NpcActor,
    others: &[&NpcActor],
    host: &mut impl NpcHost,
    telefrags: &mut Vec<u16>,
) {
    let origin = npc.player.origin();
    let low: [f32; 3] = std::array::from_fn(|axis| origin[axis] + npc.mins[axis]);
    let high: [f32; 3] = std::array::from_fn(|axis| origin[axis] + npc.maxs[axis]);
    let owner = owner_of(npc);
    match owner {
        Some(owner) => host.telefrag_players_but(low, high, npc.number, owner),
        None => host.telefrag_players(low, high, npc.number),
    }
    let mut struck: Vec<u16> = others
        .iter()
        .filter(|other| Some(other.number) != owner && meets(npc, other, low, high))
        .map(|other| other.number)
        .collect();
    struck.sort_unstable();
    telefrags.extend(struck);
}

/// `G_ScaleNetHealth` (`g_utils.c:1116-1146`): the health bar's health, scaled down by a
/// hundred from a maximum of a thousand, never shown as dead while alive.
pub(crate) fn scale_net_health(npc: &mut NpcActor) {
    let (max, health) = if npc.bar_max_health < 1000 {
        (npc.bar_max_health, npc.health.max(0))
    } else {
        let health = (npc.health / 100).max(0);
        (
            npc.bar_max_health / 100,
            if npc.health > 0 && health <= 0 {
                1
            } else {
                health
            },
        )
    };
    npc.state.set_raw_field(es::MAX_HEALTH, max as u32);
    npc.state.set_raw_field(es::HEALTH, health as u32);
}

/// `G_CheckInSolid(ent, qtrue)` (`g_utils.c:1941-1976`): the box, flat-bottomed at the
/// origin, swept down by its own depth; where it meets something short of that, the NPC
/// is set on it (`G_SetOrigin`: standing, no longer interpolated). The second check the
/// reference makes from there only answers, and nothing reads the answer.
fn check_in_solid(npc: &mut NpcActor, others: &[&NpcActor], host: &mut impl NpcHost) {
    let start = npc.current_origin;
    let mut end = start;
    end[2] += npc.mins[2];
    let mins = [npc.mins[0], npc.mins[1], 0.0];
    let bodies: Vec<crate::entity_clip::BoxObstacle> = others
        .iter()
        .filter(|other| other.number != npc.number)
        .map(|other| crate::entity_clip::BoxObstacle {
            entity: other.number,
            origin: other.current_origin,
            bounds: (other.mins, other.maxs),
            contents: other.contents,
            model: None,
        })
        .collect();
    let trace = host.trace(
        start,
        mins,
        npc.maxs,
        end,
        npc.number,
        npc.clip_mask,
        &bodies,
    );
    if trace.all_solid || trace.start_solid || trace.fraction >= 1.0 {
        return;
    }
    let mut origin = trace.end_position;
    origin[2] -= npc.mins[2];
    crate::npc_spawn::set_origin(npc, origin);
}

/// `NPC_SetMiscDefaultData` (`NPC_spawn.c:228-523`): what the class, the name and the
/// team add — behaviour, flags, bounds, the saber and the Force set up, the siege side.
fn misc_default_data(npc: &mut NpcActor, host: &mut impl NpcHost, level_time: i32) {
    let class = npc.definition.client_class;
    if npc.spawnflags & SFB_CINEMATIC != 0 {
        npc.behavior_state = BS_CINEMATIC;
    }
    if class == class::BOBAFETT {
        register(BOBA, host);
        npc.force_levels[FP_LEVITATION] = 3;
        or_player(npc, ps::FORCE_KNOWN, 1 << FP_LEVITATION);
        npc.player.set_raw_field(ps::FORCE_POWER, 100);
        npc.script_flags |= SCF_ALT_FIRE | SCF_NO_GROUPS;
    }
    // A vehicle: the largest radius the wire holds (`NPC_spawn.c:243-253`); a walker is
    // shielded and never knocked back (its AT-ST pain is [`crate::npc_pain::pain_func`]'s;
    // its mass, which nothing knocks, and its Ghoul2 hatch send nothing).
    if npc.definition.entity_class == class::VEHICLE
        && let Some(vehicle) = npc.vehicle.as_deref()
    {
        npc.state.set_raw_field(es::G2_RADIUS, 255);
        if vehicle.kind() == crate::vehicle_fields::kind::WALKER {
            npc.flags |= FL_SHIELDED | FL_NO_KNOCKBACK;
        }
    }
    // Its instance's bolts: `SetupGameGhoul2Model`'s (at its spawn), then the wampa's and the
    // rancor's own (`Wampa_SetBolts`, `Rancor_SetBolts`).
    crate::npc_creature::setup_bolts(npc, host);
    let wampa = npc.npc_type.eq_ignore_ascii_case(b"wampa");
    crate::npc_creature::set_class_bolts(npc, host, class == class::RANCOR, wampa);
    if wampa {
        npc.state.set_raw_field(es::G2_RADIUS, 80);
        npc.flags |= FL_NO_KNOCKBACK;
    }
    if class == class::RANCOR {
        npc.state.set_raw_field(es::G2_RADIUS, 255);
        npc.flags |= FL_NO_KNOCKBACK;
        npc.health = npc.health.wrapping_mul(4);
    }
    if npc.npc_type.eq_ignore_ascii_case(b"yoda") {
        npc.script_flags |= SCF_NO_FORCE;
    }
    if [
        &b"emperor"[..],
        b"cultist_grip",
        b"cultist_drain",
        b"cultist_lightning",
    ]
    .iter()
    .any(|name| npc.npc_type.eq_ignore_ascii_case(name))
    {
        npc.script_flags |= SCF_DONT_FIRE;
    }
    if i32::from(npc.player.weapon()) == WP_SABER {
        // `WP_SaberInitBladeData`: a saber entity only the server knows, and its spin.
        npc.saber_entity = host.spawn_hidden();
        // `ps.saberEntityNum` names it (`w_saber.c`'s `WP_SaberInitBladeData`).
        npc.player.set_raw_field(
            ps::SABER_ENTITY_NUM,
            u32::from(npc.saber_entity.unwrap_or(0)),
        );
        npc.saber = crate::npc_saber::NpcSaber::new(&npc.definition.sabers, level_time);
        if let Some(number) = npc.saber_entity {
            crate::npc_saber_throw::init_saber_state(host, number);
        }
        host.sound_index(b"sound/weapons/saber/saberspin.wav");
        npc.player.set_raw_field(ps::SABER_HOLSTERED, 2);
        clear_timers(npc, &crate::npc_mind::JEDI_TIMERS, level_time);
    }
    if npc.player.raw_field(ps::FORCE_KNOWN).unwrap_or(0) != 0 {
        init_force_powers(npc, level_time, host);
    }
    if class == class::SEEKER {
        npc.default_behavior = BS_DEFAULT;
        fly(npc);
        npc.count = 30;
    }
    team_defaults(npc, host, level_time);
    // A siege side for the NPCs that are not vehicles; a seeker someone owns keeps its
    // teams, and a map-placed one has no owner.
    if host.gametype() == GT_SIEGE && npc.definition.entity_class != class::VEHICLE {
        npc.session_team = match npc.enemy_team {
            NPCTEAM_PLAYER => 1,
            NPCTEAM_ENEMY => 2,
            _ => 0,
        };
    }
    if class == class::ATST || class == class::MARK1 {
        npc.flags |= FL_SHIELDED | FL_NO_KNOCKBACK;
    }
}

/// `ST_ClearTimers`, `Jedi_ClearTimers`: the class's timers, all run out now.
fn clear_timers(npc: &mut NpcActor, names: &[&'static str], level_time: i32) {
    for name in names {
        npc.mind.timers.set(name, level_time, 0);
    }
}

/// No gravity and flying (`NPCAI_CUSTOM_GRAVITY`, `EF2_FLYING`).
fn fly(npc: &mut NpcActor) {
    npc.player.set_raw_field(ps::GRAVITY, 0);
    npc.ai_flags |= NPCAI_CUSTOM_GRAVITY;
    or_player(npc, ps::EFLAGS2, EF2_FLYING);
}

/// The team's part of `NPC_SetMiscDefaultData` (`NPC_spawn.c:330-485`).
fn team_defaults(npc: &mut NpcActor, host: &mut impl NpcHost, level_time: i32) {
    let class = npc.definition.client_class;
    let ambush = npc.spawnflags & JSF_AMBUSH != 0;
    let weapon = i32::from(npc.player.weapon());
    match npc.player_team {
        NPCTEAM_PLAYER => {
            if class == class::JEDI || class == class::LUKE {
                npc.enemy_team = NPCTEAM_ENEMY;
                if ambush {
                    npc.script_flags |= SCF_IGNORE_ALERTS;
                }
            } else if weapon == WP_THERMAL || weapon == WP_BLASTER {
                clear_timers(npc, &crate::npc_mind::STORMTROOPER_TIMERS, level_time);
            }
            if class == class::KYLE
                || class == class::VEHICLE
                || npc.spawnflags & SFB_CINEMATIC != 0
            {
                npc.default_behavior = BS_CINEMATIC;
            }
        }
        NPCTEAM_NEUTRAL => {
            if npc.npc_type.eq_ignore_ascii_case(b"gonk") {
                npc.server_flags |= SVF_PLAYER_USABLE;
            }
        }
        NPCTEAM_ENEMY => {
            npc.default_behavior = BS_DEFAULT;
            if class == class::SHADOWTROOPER {
                cloak(npc, host);
            }
            if [
                class::TAVION,
                class::REBORN,
                class::DESANN,
                class::SHADOWTROOPER,
            ]
            .contains(&class)
            {
                npc.enemy_team = NPCTEAM_PLAYER;
                if ambush {
                    npc.script_flags |= SCF_IGNORE_ALERTS;
                }
            } else if [
                class::PROBE,
                class::REMOTE,
                class::INTERROGATOR,
                class::SENTRY,
            ]
            .contains(&class)
            {
                fly(npc);
            } else {
                // Every weapon but these is taken for a stormtrooper's (`NPC_spawn.c:440-470`).
                if ![
                    WP_BRYAR_PISTOL,
                    WP_DISRUPTOR,
                    WP_BOWCASTER,
                    WP_REPEATER,
                    WP_DEMP2,
                    WP_FLECHETTE,
                    WP_ROCKET_LAUNCHER,
                    WP_THERMAL,
                    WP_STUN_BATON,
                ]
                .contains(&weapon)
                {
                    clear_timers(npc, &crate::npc_mind::STORMTROOPER_TIMERS, level_time);
                }
                if npc.npc_type.eq_ignore_ascii_case(b"galak_mech") {
                    galak_mech_init(npc, host, level_time);
                }
            }
        }
        _ => {}
    }
}

/// `NPC_GalakMech_Init` (`NPC_AI_GalakMech.c:77-117`): outside a cinematic, the shield,
/// its attacks' timers, its box and no pushing about, the shield's surface shown and the face's hidden; in
/// one, the other way about.
fn galak_mech_init(npc: &mut NpcActor, host: &mut impl NpcHost, level_time: i32) {
    let shielded = npc.behavior_state != BS_CINEMATIC;
    if shielded {
        // Its attacks' timers, all run out.
        clear_timers(
            npc,
            &[
                "attackDelay",
                "flee",
                "smackTime",
                "beamDelay",
                "noLob",
                "noRapid",
                "talkDebounce",
            ],
            level_time,
        );
        npc.player.stats[STAT_ARMOR] = GALAK_SHIELD_HEALTH;
        npc.flags |= FL_SHIELDED | FL_NO_KNOCKBACK;
        (npc.mins, npc.maxs) = ([-60.0, -60.0, -24.0], [60.0, 60.0, 80.0]);
    }
    let _ = set_surface(npc, "torso_shield", shielded, host);
    for surface in [
        "torso_galakface",
        "torso_galakhead",
        "torso_eyes_mouth",
        "torso_collar",
        "torso_galaktorso",
    ] {
        let _ = set_surface(npc, surface, !shielded, host);
    }
}

/// `bgToggleableSurfaces` (`bg_misc.c:37-79`): the surfaces whose state the entity carries.
const TOGGLEABLE_SURFACES: [&str; 30] = [
    "l_arm_key",
    "torso_canister1",
    "torso_canister2",
    "torso_canister3",
    "torso_tube1",
    "torso_tube2",
    "torso_tube3",
    "torso_tube4",
    "torso_tube5",
    "torso_tube6",
    "r_arm",
    "l_arm",
    "torso_shield",
    "torso_galaktorso",
    "torso_collar",
    "r_wing1",
    "r_wing2",
    "l_wing1",
    "l_wing2",
    "r_gear",
    "l_gear",
    "nose",
    "blah4",
    "blah5",
    "l_hand",
    "r_hand",
    "helmet",
    "head",
    "head_concussion_charger",
    "head_light_blaster_cann",
];

/// `NPC_SetSurfaceOnOff` (`NPC_utils.c:1018-1056`): the surface's bit in `s.surfacesOn` or
/// `s.surfacesOff`; a surface not in the list is only warned of (`false`: the instance's
/// surface is then left alone too).
pub(crate) fn set_surface(
    npc: &mut NpcActor,
    name: &str,
    on: bool,
    host: &mut impl NpcHost,
) -> bool {
    let Some(bit) = TOGGLEABLE_SURFACES
        .iter()
        .position(|known| known.eq_ignore_ascii_case(name))
    else {
        host.print(&format!(
            "WARNING: Tried to toggle NPC surface that isn't in toggleable surface list ({name})\n"
        ));
        return false;
    };
    let (set, clear) = if on {
        (es::SURFACES_ON, es::SURFACES_OFF)
    } else {
        (es::SURFACES_OFF, es::SURFACES_ON)
    };
    let value = npc.state.raw_field(set).unwrap_or(0);
    npc.state.set_raw_field(set, value | 1 << bit);
    let value = npc.state.raw_field(clear).unwrap_or(0);
    npc.state.set_raw_field(clear, value & !(1 << bit));
    true
}

/// `Jedi_Cloak` (`NPC_AI_Jedi.c:804-821`): out of the targeting, and cloaked with a sound
/// if it was not.
pub(crate) fn cloak(npc: &mut NpcActor, host: &mut impl NpcHost) {
    npc.flags |= FL_NOTARGET;
    if npc.player.powerups[PW_CLOAKED] == 0 {
        npc.player.powerups[PW_CLOAKED] = Q3_INFINITE;
        item_sound(npc, host, b"sound/chars/shadowtrooper/cloak.wav");
    }
}

/// `G_Sound(npc, CHAN_ITEM, G_SoundIndex(name))` (`g_utils.c`): `G_SoundTempEntity` at its
/// origin, the channel in `saberEntityNum`.
pub(crate) fn item_sound(npc: &NpcActor, host: &mut impl NpcHost, name: &[u8]) {
    let sound = host.sound_index(name);
    let mut event = crate::event_entity::EventEntity {
        event: EV_GENERAL_SOUND,
        parameter: u32::from(sound),
        origin: npc.current_origin,
        client: None,
        broadcast: false,
        extra: [(0, 0); 12],
    };
    event.extra[0] = (ES_SABER_ENTITY, CHAN_ITEM);
    host.raise(event);
}

/// `WP_InitForcePowers` and `WP_SpawnInitForcePowers` for an NPC (`w_force.c:158-196`,
/// `423-531`): the style from its session (the style its Force updates before its begin
/// saw, level 1 where they saw none), the powers' loop sounds, a full pool,
/// and the known powers without a level forgotten. A holocron game takes every level away.
fn init_force_powers(npc: &mut NpcActor, level_time: i32, host: &mut impl NpcHost) {
    let level = npc.mind.session_saber_level;
    npc.player.set_raw_field(
        ps::SABER_ANIM_LEVEL,
        if (1..=3).contains(&level) {
            level as u32
        } else {
            1
        },
    );
    for name in [
        &b"sound/weapons/force/speedloop.wav"[..],
        b"sound/weapons/force/rageloop.wav",
        b"sound/weapons/force/absorbloop.wav",
        b"sound/weapons/force/protectloop.wav",
        b"sound/weapons/force/seeloop.wav",
        b"sound/player/nullifyloop.wav",
    ] {
        host.sound_index(name);
    }
    npc.player.set_raw_field(ps::FORCE_POWER, FORCE_POWER_MAX);
    crate::npc_force::init(&mut npc.force, &npc.force_levels, level_time);
    if host.gametype() == GT_HOLOCRON {
        // The saber-only exception (`HasSetSaberOnly`: saber offense and defense kept at
        // one) is not read here; a holocron game takes an NPC's every level.
        npc.force_levels = [0; crate::npc_parms::NPC_FORCE_POWERS];
    }
    let mut known = npc.player.raw_field(ps::FORCE_KNOWN).unwrap_or(0);
    for (power, level) in npc.force_levels.iter().enumerate() {
        if *level == 0 {
            known &= !(1 << power);
        }
    }
    npc.player.set_raw_field(ps::FORCE_KNOWN, known);
}

/// A class's precache list, registered.
pub(crate) fn register(list: &[Registration], host: &mut impl NpcHost) {
    for registration in list {
        match *registration {
            Registration::Sound(name) => {
                host.sound_index(name.as_bytes());
            }
            Registration::Effect(name) => {
                host.effect_index(name.as_bytes());
            }
            Registration::Weapon(_) | Registration::Ammo(_) => {
                if let Some(item) = registration.item() {
                    host.register_item(item);
                }
            }
        }
    }
}
