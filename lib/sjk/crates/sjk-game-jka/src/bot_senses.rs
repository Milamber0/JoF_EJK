//! What a bot notices (OpenJK `codemp/game/ai_main.c`): the events each client made
//! (`UpdateEventTracker`), who may be its enemy (`PassStandardEnemyChecks`,
//! `PassLovedOneCheck`, `BotMindTricked`, `OnSameTeam`), what it hears (`BotCanHear`),
//! the enemy it picks (`ScanForEnemies`) and the danger it runs from
//! (`GetNearestBadThing`).
//!
//! The game is read through [`BotSenses`]: the clients by entity number, the other
//! entities, and the map's PVS and traces.

use crate::bot_personality::BotAttachment;
use crate::bot_think::BotMind;
use crate::player_angle_math::{angle_mod, vector_angles};

/// `LEVELFLAG_IGNOREINFALLBACK`.
const LEVELFLAG_IGNOREINFALLBACK: i32 = 2;
/// `ENEMY_FORGET_MS`.
const ENEMY_FORGET_MS: i32 = 10_000;
/// `MAX_CLIENTS`.
const MAX_CLIENTS: i32 = 32;
const PM_SPECTATOR: i32 = 4;
const PM_INTERMISSION: i32 = 7;
const TEAM_SPECTATOR: i32 = 3;
const CON_DISCONNECTED: i32 = 0;
const GT_JEDIMASTER: i32 = 2;
const GT_DUEL: i32 = 3;
const GT_POWERDUEL: i32 = 4;
const GT_SINGLE_PLAYER: i32 = 5;
const GT_TEAM: i32 = 6;
const WP_ROCKET_LAUNCHER: i32 = 11;
const WP_THERMAL: i32 = 12;
const WP_TRIP_MINE: i32 = 13;
const WP_DET_PACK: i32 = 14;
const WP_FLECHETTE: i32 = 10;
/// The events a bot hears (`entity_event_t`).
const EV_FOOTSTEP: i32 = 2;
const EV_FOOTSTEP_METAL: i32 = 3;
const EV_FOOTWADE: i32 = 5;
const EV_STEP_4: i32 = 7;
const EV_STEP_16: i32 = 10;
const EV_JUMP: i32 = 16;
const EV_ROLL: i32 = 17;
const EV_FIRE_WEAPON: i32 = 27;
const EV_ALT_FIRE: i32 = 28;
const EV_SABER_ATTACK: i32 = 29;
const EV_GLOBAL_SOUND: i32 = 77;

/// A client as the bots see it (its entity and `gclient_t`).
#[derive(Clone, Debug, Default)]
pub struct SensedClient {
    /// Its entity is in use (`inuse`).
    pub in_use: bool,
    /// A bot (`botstates[number]` in use, `SVF_BOT`).
    pub bot: bool,
    /// `pers.connected`.
    pub connected: i32,
    pub team: i32,
    pub duel_team: i32,
    pub health: i32,
    pub takes_damage: bool,
    pub pm_type: i32,
    /// `s.solid` non-zero.
    pub solid: bool,
    pub origin: [f32; 3],
    pub duel_in_progress: bool,
    pub duel_index: i32,
    pub is_jedi_master: bool,
    /// `fd.forceMindtrickTargetIndex` 1 to 4: the clients it has tricked, 16 a word.
    pub mind_tricked: [i32; 4],
    /// `dangerTime`: when it last attacked.
    pub danger_time: i32,
    /// `otherSoundTime`, `otherSoundLen`: a noise until then, heard that far.
    pub other_sound_time: i32,
    pub other_sound_len: f32,
    pub footstep_time: i32,
    pub netname: Vec<u8>,
    /// The weapon in hand and the flags carried, which a bot's fear weighs.
    pub weapon: i32,
    pub red_flag: bool,
    pub blue_flag: bool,
    /// What a bot fighting it reads: its eye height, its velocity, whether its entity
    /// moves (`s.pos.trDelta`), its Force (`fd.forcePower`, `fd.forceSide`,
    /// `fd.forcePowersActive`), its saber put away, and until when its duel challenge
    /// stands (`duelTime`).
    pub viewheight: i32,
    pub velocity: [f32; 3],
    pub moving: bool,
    pub force_power: i32,
    pub force_side: i32,
    pub force_powers_active: i32,
    pub saber_holstered: bool,
    pub duel_time: i32,
    /// On the ground, and its saber move, which a bot's saber defence reads.
    pub on_ground: bool,
    pub saber_move: u32,
}

/// An entity that is no client, as `GetNearestBadThing` weighs it.
#[derive(Clone, Copy, Debug, Default)]
pub struct SensedThing {
    pub number: i32,
    pub in_use: bool,
    pub damage: i32,
    pub splash_damage: i32,
    /// `s.weapon`: a missile's.
    pub weapon: i32,
    /// `genericValue5 == 1000` marks a sentry; `genericValue3` is its owner.
    pub generic5: i32,
    pub generic3: i32,
    pub health: i32,
    pub owner: i32,
    pub current_origin: [f32; 3],
    /// `s.pos.trBase`.
    pub base: [f32; 3],
    /// `s.origin`, where a bot heading for it looks.
    pub origin: [f32; 3],
    /// `takedamage`: a danger that can be destroyed.
    pub takes_damage: bool,
}

/// A client's last events, as `UpdateEventTracker` keeps them (`boteventtracker_t`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EventTracker {
    pub event_sequence: i32,
    pub events: [i32; 2],
    pub event_time: f32,
}

/// The rules the senses read.
#[derive(Clone, Copy, Debug)]
pub struct SensingRules {
    pub level_time: i32,
    pub gametype: i32,
    /// `gLevelFlags`.
    pub level_flags: i32,
    /// `g_friendlyFire`, `bot_attachments`.
    pub friendly_fire: bool,
    pub attachments: bool,
}

/// The bot itself, as the senses read it.
#[derive(Clone, Copy, Debug)]
pub struct SensingBot {
    pub client: i32,
    pub duel_in_progress: bool,
    pub duel_index: i32,
    pub is_jedi_master: bool,
}

/// The game as a bot senses it.
pub trait BotSenses {
    /// The client with entity number `number`; none for an entity that is no client.
    fn client(&self, number: i32) -> Option<&SensedClient>;
    /// The entities that are no clients, in number order.
    fn things(&self) -> &[SensedThing];
    /// `trap->InPVS`.
    fn in_pvs(&mut self, from: [f32; 3], to: [f32; 3]) -> bool;
    /// `trap->Trace` of a point (`MASK_SOLID`, passing `pass`): its fraction and the
    /// entity it stopped at.
    fn trace(&mut self, from: [f32; 3], to: [f32; 3], pass: i32) -> (f32, i32);
}

fn length(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// `UpdateEventTracker`: a client whose event sequence moved on is tracked afresh, heard
/// for half a millisecond past the level's time. `clients` gives each client's
/// `eventSequence` and `events`.
pub fn update_event_tracker(
    trackers: &mut [EventTracker],
    clients: &dyn Fn(usize) -> (i32, [i32; 2]),
    level_time: i32,
) {
    for (client, tracker) in trackers.iter_mut().enumerate() {
        let (sequence, events) = clients(client);
        if tracker.event_sequence != sequence {
            *tracker = EventTracker {
                event_sequence: sequence,
                events,
                event_time: (f64::from(level_time) + 0.5) as f32,
            };
        }
    }
}

/// `OnSameTeam` for two clients: the power duel's sides, the single-player game's bots
/// and humans, the team games' teams.
pub fn on_same_team(one: &SensedClient, other: &SensedClient, gametype: i32) -> bool {
    match gametype {
        GT_POWERDUEL => one.duel_team == other.duel_team,
        GT_SINGLE_PLAYER => one.bot == other.bot,
        _ if gametype < GT_TEAM => false,
        _ => one.team == other.team,
    }
}

/// `BotMindTricked`: whether `enemy` has tricked the bot in slot `bot`.
pub fn mind_tricked(bot: i32, enemy: &SensedClient) -> bool {
    let (word, bit) = ((bot / 16).clamp(0, 3) as usize, bot % 16);
    enemy.mind_tricked[word] & (1 << bit) != 0
}

/// `PassStandardEnemyChecks`: whether entity `number` may be the bot's enemy.
pub fn pass_standard_enemy_checks(
    mind: &BotMind,
    bot: &SensingBot,
    senses: &dyn BotSenses,
    number: i32,
    rules: &SensingRules,
) -> bool {
    let Some(enemy) = senses.client(number) else {
        return false;
    };
    if enemy.health < 1 || !enemy.takes_damage {
        return false;
    }
    if mind.doing_fallback && rules.level_flags & LEVELFLAG_IGNOREINFALLBACK != 0 {
        return false;
    }
    if enemy.pm_type == PM_INTERMISSION
        || enemy.pm_type == PM_SPECTATOR
        || enemy.team == TEAM_SPECTATOR
    {
        return false;
    }
    if enemy.connected == CON_DISCONNECTED || !enemy.solid || bot.client == number {
        return false;
    }
    if senses
        .client(bot.client)
        .is_some_and(|own| on_same_team(own, enemy, rules.gametype))
    {
        return false;
    }
    if mind_tricked(bot.client, enemy)
        && mind.current_enemy == Some(number)
        && length(mind.origin, enemy.origin) > 64.0
    {
        return false;
    }
    if enemy.duel_in_progress && enemy.duel_index != bot.client {
        return false;
    }
    if bot.duel_in_progress && number != bot.duel_index {
        return false;
    }
    if rules.gametype == GT_JEDIMASTER
        && !enemy.is_jedi_master
        && !bot.is_jedi_master
        && (!rules.friendly_fire || length(mind.origin, enemy.origin) > 350.0)
    {
        return false;
    }
    true
}

/// `BotCanHear`: whether the bot hears client `number` at `distance` — a noise it made,
/// a footstep, or its last event while the tracker still has it; a quarter as far from
/// one that tricked it.
pub fn can_hear(
    bot: &SensingBot,
    senses: &dyn BotSenses,
    trackers: &[EventTracker],
    number: i32,
    distance: f32,
    level_time: i32,
) -> bool {
    let Some(enemy) = senses.client(number) else {
        return false;
    };
    let reach = if enemy.other_sound_time > level_time {
        enemy.other_sound_len
    } else if enemy.footstep_time > level_time {
        256.0
    } else {
        let Some(tracker) = usize::try_from(number)
            .ok()
            .and_then(|number| trackers.get(number))
        else {
            return false;
        };
        if tracker.event_time < level_time as f32 {
            return false;
        }
        match tracker.events[(tracker.event_sequence & 1) as usize] {
            EV_GLOBAL_SOUND => 256.0,
            EV_FIRE_WEAPON | EV_ALT_FIRE | EV_SABER_ATTACK => 512.0,
            EV_STEP_4..=EV_STEP_16
            | EV_FOOTSTEP
            | EV_FOOTSTEP_METAL
            | EV_FOOTWADE
            | EV_JUMP
            | EV_ROLL => 256.0,
            _ => 999_999.0,
        }
    };
    let reach = if mind_tricked(bot.client, enemy) {
        reach / 4.0
    } else {
        reach
    };
    distance <= reach
}

/// `PassLovedOneCheck`: false only for a bot it loves (by name) enough — at 2 or more,
/// or any love for a teammate in a team game; never in a duel, never without
/// `bot_attachments`.
pub fn pass_loved_one_check(
    loved: &[BotAttachment],
    bot: &SensingBot,
    senses: &dyn BotSenses,
    number: i32,
    rules: &SensingRules,
) -> bool {
    if loved.is_empty() || rules.gametype == GT_DUEL || rules.gametype == GT_POWERDUEL {
        return true;
    }
    let Some(other) = senses.client(number).filter(|other| other.bot) else {
        return true;
    };
    if !rules.attachments {
        return true;
    }
    let teamplay = rules.gametype >= GT_TEAM;
    for love in loved {
        if other.netname == love.name {
            let same_team = senses
                .client(bot.client)
                .is_some_and(|own| on_same_team(own, other, rules.gametype));
            return !((teamplay && same_team) || love.level >= 2);
        }
    }
    true
}

/// `InFieldOfVision`: `angles` within half of `fov` of `view`, in pitch and yaw.
pub(crate) fn in_field_of_vision(view: [f32; 3], fov: f32, angles: [f32; 3]) -> bool {
    for axis in 0..2 {
        let angle = angle_mod(view[axis]);
        let target = angle_mod(angles[axis]);
        let mut difference = target - angle;
        if target > angle {
            if difference > 180.0 {
                difference = (f64::from(difference) - 360.0) as f32;
            }
        } else if difference < -180.0 {
            difference = (f64::from(difference) + 360.0) as f32;
        }
        let half = f64::from(fov) * 0.5;
        if difference > 0.0 {
            if f64::from(difference) > half {
                return false;
            }
        } else if f64::from(difference) < -half {
            return false;
        }
    }
    true
}

/// `ScanForEnemies`: the nearest client it may fight, can see (in view and not tricked
/// by it, or heard) and has a clear line to — a Jedi Master first; 128 units closer than
/// the enemy it has to switch; none while its enemy is the Jedi Master.
pub fn scan_for_enemies(
    mind: &BotMind,
    bot: &SensingBot,
    senses: &mut dyn BotSenses,
    trackers: &[EventTracker],
    rules: &SensingRules,
) -> Option<i32> {
    let has_enemy_distance = if mind.current_enemy.is_some() {
        mind.frame_enemy_len
    } else {
        0.0
    };
    if mind
        .current_enemy
        .and_then(|enemy| senses.client(enemy))
        .is_some_and(|enemy| enemy.is_jedi_master)
    {
        return None;
    }
    let mut closest = 999_999.0_f32;
    let mut no_attack_non_master = false;
    if rules.gametype == GT_JEDIMASTER {
        let there_is_a_master = (0..MAX_CLIENTS).any(|number| {
            senses
                .client(number)
                .is_some_and(|client| client.is_jedi_master)
        });
        if there_is_a_master && !bot.is_jedi_master {
            if rules.friendly_fire {
                closest = 128.0;
            } else {
                no_attack_non_master = true;
            }
        }
    }
    let mut best = None;
    for number in 0..=MAX_CLIENTS {
        if number == bot.client {
            continue;
        }
        let Some(enemy) = senses.client(number).cloned() else {
            continue;
        };
        if senses
            .client(bot.client)
            .is_some_and(|own| on_same_team(own, &enemy, rules.gametype))
        {
            continue;
        }
        if !pass_standard_enemy_checks(mind, bot, &*senses, number, rules)
            || !senses.in_pvs(enemy.origin, mind.eye)
            || !pass_loved_one_check(&mind.loved, bot, &*senses, number, rules)
        {
            continue;
        }
        let towards = [
            enemy.origin[0] - mind.eye[0],
            enemy.origin[1] - mind.eye[1],
            enemy.origin[2] - mind.eye[2],
        ];
        let mut distance =
            (towards[0] * towards[0] + towards[1] * towards[1] + towards[2] * towards[2]).sqrt();
        let angles = vector_angles(towards);
        if enemy.is_jedi_master {
            distance = 1.0;
        }
        let tricked = mind_tricked(bot.client, &enemy);
        let noticed = (in_field_of_vision(mind.viewangles, 90.0, angles) && !tricked)
            || can_hear(bot, &*senses, trackers, number, distance, rules.level_time);
        if !(distance < closest && noticed && senses.trace(mind.eye, enemy.origin, -1).0 == 1.0) {
            continue;
        }
        if tricked && !(distance < 256.0 || rules.level_time - enemy.danger_time < 100) {
            continue;
        }
        if (has_enemy_distance == 0.0 || distance < has_enemy_distance - 128.0)
            && (!no_attack_non_master || enemy.is_jedi_master)
        {
            closest = distance;
            best = Some(number);
        }
    }
    best
}

/// `GetNearestBadThing`: the nearest danger within 800 units it can see (a missile, at
/// half the reach unless an explosive; a sentry of someone not its teammate; not its own
/// or a teammate's rockets, packs, mines and thermals); a missile within 256 is pushed
/// away by a bot above skill 2. A missile's owner becomes its enemy if it has none and
/// the missile is within 512. It does not go back for 1.5 s after finding one.
pub fn get_nearest_bad_thing(
    mind: &mut BotMind,
    bot: &SensingBot,
    skill: f32,
    senses: &mut dyn BotSenses,
    rules: &SensingRules,
) -> Option<i32> {
    let level_time = rules.level_time;
    let own = senses.client(bot.client).cloned();
    let teammate = |senses: &dyn BotSenses, number: i32| {
        own.as_ref()
            .zip(senses.client(number))
            .is_some_and(|(own, other)| on_same_team(own, other, rules.gametype))
    };
    let (mut best, mut best_distance) = (None, 800.0_f32);
    for index in 0..senses.things().len() {
        let thing = senses.things()[index];
        let missile =
            thing.in_use && thing.damage != 0 && thing.weapon != 0 && thing.splash_damage != 0;
        let sentry = thing.generic5 == 1000
            && thing.in_use
            && thing.health > 0
            && thing.generic3 != bot.client
            && senses.client(thing.generic3).is_some()
            && !teammate(&*senses, thing.generic3);
        if missile || sentry {
            let distance = length(mind.origin, thing.current_origin);
            let mut factor = 1.0_f32;
            if !matches!(
                thing.weapon,
                WP_THERMAL | WP_FLECHETTE | WP_DET_PACK | WP_TRIP_MINE
            ) {
                factor = 0.5;
                if thing.weapon != 0 && distance <= 256.0 && skill > 2.0 {
                    mind.do_force_push = level_time + 700;
                }
            }
            let friendly_owner = thing.owner == bot.client
                || (thing.owner > 0
                    && thing.owner < MAX_CLIENTS
                    && senses.client(thing.owner).is_some()
                    && teammate(&*senses, thing.owner));
            if matches!(
                thing.weapon,
                WP_ROCKET_LAUNCHER | WP_DET_PACK | WP_TRIP_MINE | WP_THERMAL
            ) && friendly_owner
            {
                factor = 0.0;
            }
            if distance < best_distance * factor && senses.in_pvs(mind.origin, thing.base) {
                let (fraction, hit) = senses.trace(mind.origin, thing.base, bot.client);
                if fraction == 1.0 || hit == thing.number {
                    best = Some(thing.number);
                    best_distance = distance;
                }
            }
        }
        if thing.in_use
            && thing.damage != 0
            && thing.weapon != 0
            && (0..MAX_CLIENTS).contains(&thing.owner)
            && mind.current_enemy.is_none()
        {
            let owner = thing.owner;
            if senses.client(owner).is_some_and(|client| client.in_use)
                && pass_standard_enemy_checks(mind, bot, &*senses, owner, rules)
                && pass_loved_one_check(&mind.loved, bot, &*senses, owner, rules)
                && length(mind.origin, thing.current_origin) < 512.0
            {
                mind.current_enemy = Some(owner);
                mind.enemy_seen_time = (level_time + ENEMY_FORGET_MS) as f32;
            }
        }
    }
    if best.is_some() {
        mind.dont_go_back = (level_time + 1500) as f32;
    }
    best
}
