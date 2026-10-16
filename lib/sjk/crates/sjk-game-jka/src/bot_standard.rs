//! The body of a bot's thinking (OpenJK `codemp/game/ai_main.c`'s `StandardBotAI`,
//! after its opening in [`crate::bot_think`]): mourning its death; each living frame its
//! squad, its Force power, its item and weapon, its enemy, its trail and destination,
//! its aim and fighting, its chat, its flags, targets and mines, its moving and jumping,
//! holding fire for a friend, and the buttons it presses.
//!
//! What it does to the game besides its input — a Force power selected, the saber
//! switched, a duel answered, a saber style cycled, the jump held, an item taken in
//! hand, a line said — comes back as [`BotActs`] for the server to carry out, as the
//! reference does at once. The random map generator's (`RMG`) paths are left out: no
//! RMG level is played. The fighting half is in [`crate::bot_standard_fight`].

use crate::bot_chat::{ChatGame, death_notify, do_chat, reply_greetings};
use crate::bot_ctf::CtfGame;
use crate::bot_destination::{Destination, get_ideal_destination};
use crate::bot_input::{ACTION_ATTACK, ACTION_USE};
use crate::bot_moves::{
    BotMoveWorld, MASK_SOLID, RouteView, check_for_func, fallback_navigation, wp_org_visible,
};
use crate::bot_objectives::ObjectiveGame;
use crate::bot_routes::BotRoutes;
use crate::bot_senses::{
    BotSenses, EventTracker, SensingBot, SensingRules, pass_loved_one_check,
    pass_standard_enemy_checks, scan_for_enemies,
};
use crate::bot_squad::{SquadGame, commander, do_teamplay, scan_for_leader};
use crate::bot_standard_fight as fight;
use crate::bot_think::BotMind;
use crate::bot_trail::{
    BotBody, BotEnemy, EnemyClient, TrailGame, pass_way_check, total_trail_distance,
    wp_constant_routine, wp_touch_routine,
};
use crate::bot_weapons::{Armoury, EnemySighting, wanted_item};
use crate::crt_rand::CrtRand;
use crate::items::Item;
use crate::player_angle_math::vector_angles;
use crate::player_death::Rng;

/// `ENEMY_FORGET_MS`.
pub(crate) const ENEMY_FORGET_MS: i32 = 10_000;
/// `ENTITYNUM_NONE`.
const ENTITYNUM_NONE: i32 = 1023;
/// `CON_CONNECTING`, `CON_CONNECTED`.
const CON_CONNECTING: i32 = 1;
const CON_CONNECTED: i32 = 2;
/// `BOT_WPTOUCH_DISTANCE`.
const BOT_WPTOUCH_DISTANCE: f32 = 32.0;
/// Waypoint flags: `WPFLAG_NOVIS`, `WPFLAG_WAITFORFUNC`, `WPFLAG_NOMOVEFUNC`.
const WPFLAG_NOVIS: i32 = 0x400;
const WPFLAG_WAITFORFUNC: i32 = 0x1000;
const WPFLAG_NOMOVEFUNC: i32 = 0x20_0000;
/// `LEVELFLAG_NOPOINTPREDICTION`.
const LEVELFLAG_NOPOINTPREDICTION: i32 = 1;
/// `FP_LEVITATION`.
const FP_LEVITATION: usize = 1;

/// The bot's own state as it thinks (`cur_ps`, its entity and its client).
#[derive(Clone, Copy, Debug)]
pub struct BotSelf<'a> {
    pub health: i32,
    pub weapon: i32,
    pub weapon_state: i32,
    pub weapon_charge_time: i32,
    pub rocket_lock_time: f32,
    pub rocket_last_valid_time: f32,
    pub ammo: &'a [i32],
    /// `stats[STAT_WEAPONS]`, `stats[STAT_HOLDABLE_ITEMS]`, `powerups`.
    pub weapons: i32,
    pub holdables: i32,
    pub powerups: &'a [i32],
    /// Its Force: points, side, the powers known and active, each power's level.
    pub force_power: i32,
    pub force_side: i32,
    pub powers_known: i32,
    pub powers_active: i32,
    pub force_levels: [i32; 18],
    /// `fd.forceGripBeingGripped`, `fd.forceGripCripple`, `electrifyTime`,
    /// `fd.forceJumpCharge`.
    pub grip_being_gripped: f32,
    pub grip_cripple: i32,
    pub electrify_time: i32,
    pub force_jump_charge: f32,
    /// Its saber: thrown, its entity (0 when knocked away), put away (`saberHolstered`),
    /// locked until, its style (`fd.saberAnimLevel`).
    pub saber_in_flight: bool,
    pub saber_entity_num: i32,
    pub saber_holstered: i32,
    pub saber_lock_time: i32,
    pub saber_anim_level: i32,
    pub has_det_pack_planted: bool,
    pub duel_in_progress: bool,
    pub duel_index: i32,
    pub is_jedi_master: bool,
    pub on_ground: bool,
    /// `pm_flags & PMF_JUMP_HELD`.
    pub jump_held: bool,
    /// `pers.cmd.upmove`: the last command's up move.
    pub last_upmove: i32,
    /// Its own entity moves (`s.pos.trDelta`).
    pub moving: bool,
    /// Where its planted det pack lies (`G_Find` of a `detpack` it owns).
    pub det_pack: Option<[f32; 3]>,
}

/// The game a bot's frame reads.
pub struct StandardGame<'a> {
    pub me: BotSelf<'a>,
    pub rules: SensingRules,
    pub trackers: &'a [EventTracker],
    pub trail: TrailGame,
    pub ctf: CtfGame<'a>,
    pub objectives: ObjectiveGame<'a>,
    pub squad: SquadGame<'a>,
    /// `se_language` is English (the bots chat).
    pub english: bool,
    /// `bot_honorableduelacceptance`, `g_privateDuel`.
    pub honorable_duels: bool,
    pub private_duels: bool,
    /// `bot_forcepowers` without `g_forcePowerDisable`.
    pub force_powers: bool,
    /// Its personality's weapon weights.
    pub weights: &'a [f32],
    pub item_of: &'a dyn Fn(i32) -> Option<&'static Item>,
    /// `ForcePowerUsableOn(bot, currentEnemy, power)`.
    pub usable_on_enemy: &'a dyn Fn(usize) -> bool,
}

/// What a bot's frame does to the game besides its input.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BotActs {
    /// `fd.forcePowerSelected`, as last set.
    pub force_selected: Option<usize>,
    /// `Cmd_ToggleSaber_f`, `Cmd_EngageDuel_f`, `Cmd_SaberAttackCycle_f`.
    pub toggle_saber: bool,
    pub engage_duel: bool,
    pub cycle_saber_style: bool,
    /// `PMF_JUMP_HELD` set on its player state.
    pub jump_held: bool,
    /// The holdable item (`HI_*`) made the one in hand.
    pub holdable: Option<i32>,
    /// A line said (`EA_Say`), to the team or not.
    pub said: Option<(Vec<u8>, bool)>,
}

/// The mind of client `me`, which the caller guarantees is a bot.
pub(crate) fn mind(minds: &mut [Option<BotMind>], me: usize) -> &mut BotMind {
    minds[me].as_mut().expect("the thinking client is a bot")
}

pub(crate) fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// `OrgVisible`: a clear point trace through the world (`MASK_SOLID`).
pub(crate) fn org_visible(
    world: &mut dyn BotMoveWorld,
    from: [f32; 3],
    to: [f32; 3],
    ignore: i32,
) -> bool {
    world
        .trace(from, [0.0; 3], [0.0; 3], to, ignore, MASK_SOLID)
        .fraction
        == 1.0
}

/// `EntityVisibleBox` of a point: clear, or stopped at `wanted`.
pub(crate) fn entity_visible(
    world: &mut dyn BotMoveWorld,
    from: [f32; 3],
    to: [f32; 3],
    ignore: i32,
    wanted: i32,
) -> bool {
    let trace = world.trace(from, [0.0; 3], [0.0; 3], to, ignore, MASK_SOLID);
    if trace.fraction == 1.0 && !trace.start_solid && !trace.all_solid {
        return true;
    }
    let hit = i32::from(trace.entity_number);
    hit != ENTITYNUM_NONE && hit == wanted
}

/// An entity's `health`: a client's, or a thing's.
pub(crate) fn health_of(senses: &dyn BotSenses, number: i32) -> i32 {
    match senses.client(number) {
        Some(client) => client.health,
        None => senses
            .things()
            .iter()
            .find(|thing| thing.number == number)
            .map_or(0, |thing| thing.health),
    }
}

impl StandardGame<'_> {
    pub(crate) fn level_time(&self) -> i32 {
        self.rules.level_time
    }

    pub(crate) fn body(&self) -> BotBody<'_> {
        let me = &self.me;
        BotBody {
            levitation: me.force_levels[FP_LEVITATION],
            powers_known: me.powers_known,
            powers_active: me.powers_active,
            force_jump_charge: me.force_jump_charge,
            on_ground: me.on_ground,
            weapon: me.weapon,
            weapons: me.weapons,
            holdables: me.holdables,
            powerups: me.powerups,
            ammo: me.ammo,
            is_jedi_master: me.is_jedi_master,
            electrify_time: me.electrify_time,
            health: me.health,
        }
    }

    pub(crate) fn sensing_bot(&self, me: usize) -> SensingBot {
        SensingBot {
            client: me as i32,
            duel_in_progress: self.me.duel_in_progress,
            duel_index: self.me.duel_index,
            is_jedi_master: self.me.is_jedi_master,
        }
    }

    pub(crate) fn armoury(&self, mind: &BotMind, senses: &dyn BotSenses) -> Armoury<'_> {
        let enemy = mind.current_enemy.map(|enemy| EnemySighting {
            distance: mind.frame_enemy_len,
            visible: mind.frame_enemy_vis,
            weapon: senses.client(enemy).map(|client| client.weapon),
        });
        Armoury {
            ammo: self.me.ammo,
            weapons: self.me.weapons,
            weapon: self.me.weapon,
            enemy,
        }
    }

    fn chat<'s>(&self, senses: &'s dyn BotSenses) -> ChatGame<'s> {
        ChatGame {
            level_time: self.level_time(),
            english: self.english,
            senses,
        }
    }
}

/// A point's place and flags, read once.
struct Point {
    origin: [f32; 3],
    flags: i32,
}

/// The enemy as the trail's routines weigh it.
fn trail_enemy(senses: &dyn BotSenses, enemy: Option<i32>) -> Option<BotEnemy> {
    let enemy = enemy?;
    Some(match senses.client(enemy) {
        Some(client) => BotEnemy {
            health: client.health,
            client: Some(EnemyClient {
                is_jedi_master: client.is_jedi_master,
                red_flag: client.red_flag,
                blue_flag: client.blue_flag,
                weapon: client.weapon,
            }),
        },
        None => BotEnemy {
            health: health_of(senses, enemy),
            client: None,
        },
    })
}

/// `StandardBotAI` after its opening (not spectating, no `bot_forgimmick`), for bot
/// `me` among `minds` (by client). The C library's `rand` (`rand`) and the game's
/// `Q_irand` (`rng`) are drawn as the reference draws them.
#[allow(clippy::too_many_arguments)]
pub fn standard_bot_ai(
    minds: &mut [Option<BotMind>],
    me: usize,
    game: &StandardGame,
    routes: &mut BotRoutes,
    senses: &mut dyn BotSenses,
    world: &mut dyn BotMoveWorld,
    rng: &mut Rng,
    rand: &mut CrtRand,
) -> BotActs {
    let mut acts = BotActs::default();
    let time = game.level_time();
    if mind(minds, me).last_dead_time == 0 {
        mind(minds, me).last_dead_time = time;
    }
    if game.me.health < 1 {
        dead(minds, me, game, senses, rng, rand);
        return acts;
    }
    mind(minds, me).do_attack = 0;
    mind(minds, me).do_alt_attack = 0;
    if mind(minds, me).is_squad_leader != 0 {
        commander(minds, me, &game.squad, rng);
    } else {
        do_teamplay(mind(minds, me));
    }
    let mind_now = mind(minds, me);
    forget_the_departed(mind_now, senses);
    let mut force = fight::choose_force_power(mind_now, me, game, senses, &mut acts);
    mind_now.doing_fallback = false;
    mind_now.death_activities_done = false;
    let escaping = mind_now.running_to_escape_threat != 0;
    let sighting = game.armoury(mind_now, senses).enemy;
    if let Some(item) = wanted_item(game.me.holdables, game.me.health, sighting, escaping) {
        acts.holdable = Some(item);
        if rand.next() % 10 < 5 {
            mind_now.input.act(ACTION_USE);
        }
    }
    if fight::change_weapons(mind_now, game, senses) {
        return acts;
    }
    fight::react(mind_now, game);
    keep_track(minds, me, game, routes, senses, world, rng, &mut acts);
    if mind(minds, me).is_squad_leader == 0 && mind(minds, me).squad_leader.is_none() {
        scan_for_leader(minds, me, &game.squad);
    }
    let mind_now = mind(minds, me);
    if mind_now.squad_leader.is_none() && mind_now.squad_cannot_lead < time {
        mind_now.is_squad_leader = 1;
    }
    if mind_now.is_squad_leader != 0 && mind_now.squad_leader.is_some() {
        mind_now.squad_leader = None;
    }
    look_around(mind_now, me, game, routes, senses, world);
    let Some(doing_fallback) = follow_trail(mind_now, me, game, routes, senses, world, rng, rand)
    else {
        return acts;
    };
    mind_now.doing_fallback = doing_fallback != 0;
    fight::aim(mind_now, me, game, senses, world, rng, rand);
    let melee_strafe = fight::close_in(mind_now, me, game, senses, world, rng, &mut acts);
    if doing_fallback != 0 && mind_now.current_enemy.is_some() {
        mind_now.goal_position = mind_now.origin;
    }
    let mut halt = false;
    if mind_now.force_jumping > time as f32 {
        let mut level = mind_now.origin;
        level[2] = mind_now.goal_position[2];
        if length(sub(level, mind_now.goal_position)) < 32.0 {
            halt = true;
        }
    }
    if !talk(minds, me, game, senses, rng, &mut acts) {
        return acts;
    }
    let mind_now = mind(minds, me);
    crate::bot_tactics::ctf_flag_movement(mind_now, me as i32, routes, game.ctf.dropped, world);
    fight::shoot_goal(mind_now, me, game, world);
    if fight::plant_mines(mind_now, me, game, senses, world) {
        return acts;
    }
    fight::jedi_master_saber(mind_now, me, game, world);
    fight::move_on(
        mind_now,
        me,
        game,
        routes,
        world,
        rng,
        melee_strafe,
        halt,
        &mut acts,
    );
    fight::shoot_danger(mind_now, me, game, senses, world, rng);
    fight::mind_friends(mind_now, me, game, senses, world, &mut force, &mut acts);
    fight::press_buttons(mind_now, game, senses, rand, &mut force, &mut acts);
    // `MoveTowardIdealAngles`.
    mind_now.ideal_viewangles = mind_now.goal_angles;
    acts
}

/// The dead bot: its death mourned once (whom it loved told, a line for its killer),
/// its trail and enemy dropped, and now and then the attack pressed to respawn.
fn dead(
    minds: &mut [Option<BotMind>],
    me: usize,
    game: &StandardGame,
    senses: &mut dyn BotSenses,
    rng: &mut Rng,
    rand: &mut CrtRand,
) {
    let time = game.level_time();
    mind(minds, me).last_dead_time = time;
    let bot = game.sensing_bot(me);
    let killer = mind(minds, me)
        .last_hurt
        .filter(|&killer| senses.client(killer).is_some() && killer != me as i32);
    if !mind(minds, me).death_activities_done
        && let Some(killer) = killer
    {
        death_notify(minds, me, &game.chat(senses), &game.rules, rng);
        let loved = pass_loved_one_check(&mind(minds, me).loved, &bot, senses, killer, &game.rules);
        let section: Option<&[u8]> = if loved {
            Some(b"Died")
        } else {
            let killer_bot = senses.client(killer).map(|client| SensingBot {
                client: killer,
                duel_in_progress: client.duel_in_progress,
                duel_index: client.duel_index,
                is_jedi_master: client.is_jedi_master,
            });
            let loves_me = minds
                .get(killer as usize)
                .and_then(Option::as_ref)
                .zip(killer_bot)
                .is_some_and(|(other, killer_bot)| {
                    pass_loved_one_check(&other.loved, &killer_bot, senses, me as i32, &game.rules)
                });
            loves_me.then_some(b"KilledOnPurposeByLove")
        };
        let mind_now = mind(minds, me);
        if let Some(section) = section {
            mind_now.chat_object = Some(killer);
            mind_now.chat_alt_object = None;
            do_chat(mind_now, section, false, &game.chat(senses), rng);
        }
        mind_now.death_activities_done = true;
    }
    let mind_now = mind(minds, me);
    mind_now.wp_current = None;
    mind_now.current_enemy = None;
    mind_now.wp_destination = None;
    mind_now.wp_camping = None;
    mind_now.wp_camping_to = None;
    mind_now.wp_store_dest = None;
    mind_now.wp_dest_ignore_time = 0.0;
    mind_now.wp_dest_switch_time = 0.0;
    mind_now.wp_seen_time = 0.0;
    mind_now.wp_direction = 0;
    if rand.next() % 10 < 5 && (mind_now.do_chat == 0 || mind_now.chat_time < time as f32) {
        mind_now.input.act(ACTION_ATTACK);
    }
}

/// A hated one or an enemy who left the game is forgotten.
fn forget_the_departed(mind: &mut BotMind, senses: &dyn BotSenses) {
    let departed = |number: Option<i32>| {
        number
            .and_then(|number| senses.client(number))
            .is_some_and(|client| {
                client.connected != CON_CONNECTED && client.connected != CON_CONNECTING
            })
    };
    if mind.current_enemy.is_none() {
        mind.frame_enemy_vis = false;
    }
    if departed(mind.revenge_enemy) {
        mind.revenge_enemy = None;
        mind.revenge_hate_level = 0;
    }
    if departed(mind.current_enemy) {
        mind.current_enemy = None;
    }
}

/// The frame's bookkeeping before it looks around: camping ends, a stale point is
/// dropped, a lost or dead enemy forgotten (a line for a kill), a duel challenge
/// answered, the nearest point taken, and the nearest enemy sought.
#[allow(clippy::too_many_arguments)]
fn keep_track(
    minds: &mut [Option<BotMind>],
    me: usize,
    game: &StandardGame,
    routes: &BotRoutes,
    senses: &mut dyn BotSenses,
    world: &mut dyn BotMoveWorld,
    rng: &mut Rng,
    acts: &mut BotActs,
) {
    let time = game.level_time();
    let now = time as f32;
    let bot = game.sensing_bot(me);
    let mind_now = mind(minds, me);
    if mind_now.wp_camping.is_some() {
        if mind_now.is_camping < now {
            mind_now.wp_camping = None;
            mind_now.is_camping = 0.0;
        }
        if mind_now.current_enemy.is_some() && mind_now.frame_enemy_vis {
            mind_now.wp_camping = None;
            mind_now.is_camping = 0.0;
        }
    }
    if mind_now.wp_current.is_some()
        && (mind_now.wp_seen_time < now || mind_now.wp_travel_time < now)
    {
        mind_now.wp_current = None;
    }
    if let Some(enemy) = mind_now.current_enemy
        && (mind_now.enemy_seen_time < now
            || !pass_standard_enemy_checks(mind_now, &bot, senses, enemy, &game.rules))
    {
        let dead = health_of(senses, enemy) < 1;
        let attacked = mind_now.last_attacked == Some(enemy);
        if mind_now.revenge_enemy == Some(enemy) && dead && attacked {
            mind_now.chat_object = mind_now.revenge_enemy;
            mind_now.chat_alt_object = None;
            do_chat(mind_now, b"KilledHatedOne", true, &game.chat(senses), rng);
            mind_now.revenge_enemy = None;
            mind_now.revenge_hate_level = 0;
        } else if dead
            && pass_loved_one_check(&mind_now.loved, &bot, senses, enemy, &game.rules)
            && attacked
        {
            mind_now.chat_object = Some(enemy);
            mind_now.chat_alt_object = None;
            do_chat(mind_now, b"Killed", false, &game.chat(senses), rng);
        }
        mind_now.current_enemy = None;
    }
    fight::answer_challenge(mind_now, me, game, senses, acts);
    if mind_now.wp_current.is_none()
        && let Some(point) =
            routes.nearest_visible(mind_now.origin, me as i32, &mut RouteView(world))
    {
        mind_now.wp_current = Some(point);
        mind_now.wp_seen_time = (time + 1500) as f32;
        // Never more than ten seconds to reach a point.
        mind_now.wp_travel_time = (time + 10_000) as f32;
    }
    // The reference's test (`enemySeenTime < level.time || !frame_Enemy_Vis ||
    // !currentEnemy || currentEnemy`) always holds.
    if let Some(enemy) = scan_for_enemies(mind_now, &bot, senses, game.trackers, &game.rules) {
        mind_now.current_enemy = Some(enemy);
        mind_now.enemy_seen_time = (time + ENEMY_FORGET_MS) as f32;
    }
}

/// Where the bot and its enemy are this frame: the point's distance and sight (seen
/// through another's shield, the destination is dropped and the trail turned back), the
/// enemy's distance and sight (where and from where it was last seen).
fn look_around(
    mind: &mut BotMind,
    me: usize,
    game: &StandardGame,
    routes: &BotRoutes,
    senses: &dyn BotSenses,
    world: &mut dyn BotMoveWorld,
) {
    let time = game.level_time();
    if let Some(current) = mind.wp_current {
        let point = routes.waypoints[current].origin;
        mind.frame_waypoint_len = length(sub(point, mind.origin));
        match wp_org_visible(me as i32, mind.origin, point, me as i32, world) {
            2 => {
                mind.frame_waypoint_vis = false;
                mind.wp_seen_time = 0.0;
                mind.wp_destination = None;
                mind.wp_dest_ignore_time = (time + 5000) as f32;
                mind.wp_direction = i32::from(mind.wp_direction == 0);
            }
            0 => mind.frame_waypoint_vis = false,
            _ => mind.frame_waypoint_vis = true,
        }
    }
    if let Some(enemy) = mind.current_enemy {
        let seen_at = match senses.client(enemy) {
            Some(client) => {
                let mut head = client.origin;
                head[2] += client.viewheight as f32;
                head
            }
            None => senses
                .things()
                .iter()
                .find(|thing| thing.number == enemy)
                .map_or([0.0; 3], |thing| thing.origin),
        };
        mind.frame_enemy_len = length(sub(seen_at, mind.eye));
        if org_visible(world, mind.eye, seen_at, me as i32) {
            mind.frame_enemy_vis = true;
            mind.last_enemy_spotted = seen_at;
            mind.here_when_spotted = mind.origin;
            mind.last_visible_enemy_index = enemy;
            mind.hit_spotted = 0;
        } else {
            mind.frame_enemy_vis = false;
        }
    } else {
        mind.last_visible_enemy_index = ENTITYNUM_NONE;
    }
    if mind.frame_enemy_vis {
        mind.enemy_seen_time = (time + ENEMY_FORGET_MS) as f32;
    }
}

/// The trail: the point's routines, waiting for lifts, the goal position and angles
/// along it, the destination, and moving on when it is touched; without a point, the
/// fallback's wandering. `None` where the point's routine dropped it (the frame ends);
/// else whether the fallback ran.
#[allow(clippy::too_many_arguments)]
fn follow_trail(
    mind: &mut BotMind,
    me: usize,
    game: &StandardGame,
    routes: &mut BotRoutes,
    senses: &mut dyn BotSenses,
    world: &mut dyn BotMoveWorld,
    rng: &mut Rng,
    rand: &mut CrtRand,
) -> Option<i32> {
    let time = game.level_time();
    let now = time as f32;
    if mind.wp_current.is_none() {
        mind.jump_time = (time + 1500) as f32;
        mind.jump_hold_time = (time + 1500) as f32;
        mind.j_delay = 0.0;
        return Some(fallback_navigation(mind, world, rand));
    }
    let body = game.body();
    wp_constant_routine(mind, routes, &body, time);
    let current = mind.wp_current?;
    let point = (
        routes.waypoints[current].origin,
        routes.waypoints[current].flags,
    );
    let point = Point {
        origin: point.0,
        flags: point.1,
    };
    if point.flags & WPFLAG_WAITFORFUNC != 0 && !check_for_func(point.origin, -1, world) {
        // No lift under it yet: wait.
        mind.be_still = (time + 500) as f32;
    }
    if point.flags & WPFLAG_NOMOVEFUNC != 0 && check_for_func(point.origin, -1, world) {
        mind.be_still = (time + 500) as f32;
    }
    if mind.frame_waypoint_vis || point.flags & WPFLAG_NOVIS != 0 {
        // Out of sight, the point is dropped a second and a half later.
        mind.wp_seen_time = (time + 1500) as f32;
    }
    mind.goal_position = point.origin;
    let ahead = if mind.wp_direction != 0 {
        current as i32 - 1
    } else {
        current as i32 + 1
    };
    let ahead = usize::try_from(ahead)
        .ok()
        .filter(|&ahead| ahead < routes.waypoints.len());
    if let Some(camp) = mind.wp_camping {
        if let Some(to) = mind.wp_camping_to {
            mind.goal_angles = vector_angles(sub(routes.waypoints[to].origin, mind.origin));
        }
        let camp = routes.waypoints[camp].origin;
        if length(sub(mind.origin, camp)) < 64.0 {
            mind.goal_position = camp;
            mind.be_still = (time + 1000) as f32;
            if !mind.camp_standing {
                mind.duck_time = (time + 1000) as f32;
            }
        }
    } else if let Some(ahead) =
        ahead.filter(|_| game.rules.level_flags & LEVELFLAG_NOPOINTPREDICTION == 0)
    {
        mind.goal_angles = vector_angles(sub(routes.waypoints[ahead].origin, mind.origin));
    } else {
        mind.goal_angles = vector_angles(sub(point.origin, mind.origin));
    }
    if mind.destination_grab_time < now {
        let destination = Destination {
            bot: game.sensing_bot(me),
            skill: mind.skill,
            body,
            last_dead_time: mind.last_dead_time,
            sensing: game.rules,
            trail: game.trail,
            ctf: game.ctf,
            objectives: game.objectives,
            item_of: game.item_of,
        };
        get_ideal_destination(mind, routes, &destination, senses, world, rng);
    }
    if let (Some(current), Some(destination)) = (mind.wp_current, mind.wp_destination)
        && total_trail_distance(routes, current as i32, destination as i32) == -1.0
    {
        mind.wp_destination = None;
        mind.destination_grab_time = (time + 10_000) as f32;
    }
    if mind.frame_waypoint_len < BOT_WPTOUCH_DISTANCE {
        let enemy = trail_enemy(senses, mind.current_enemy);
        wp_touch_routine(mind, routes, &body, enemy.as_ref(), &game.trail, rand);
        let current = mind.wp_current?;
        let desired = if mind.wp_direction == 0 {
            current as i32 + 1
        } else {
            current as i32 - 1
        };
        if (0..routes.waypoints.len() as i32).contains(&desired)
            && pass_way_check(mind, routes, desired, body.levitation)
        {
            mind.wp_current = Some(desired as usize);
        } else {
            if mind.wp_destination.is_some() {
                mind.wp_destination = None;
                mind.destination_grab_time = (time + 10_000) as f32;
            }
            mind.wp_direction = i32::from(mind.wp_direction == 0);
        }
    }
    Some(0)
}

/// The chat waiting: while its time has not come and no enemy is in sight, the frame
/// ends (`false`); an enemy in sight cancels it; else it is said (a greeting answered).
fn talk(
    minds: &mut [Option<BotMind>],
    me: usize,
    game: &StandardGame,
    senses: &mut dyn BotSenses,
    rng: &mut Rng,
    acts: &mut BotActs,
) -> bool {
    let now = game.level_time() as f32;
    let mind_now = mind(minds, me);
    if mind_now.do_chat == 0 {
        return true;
    }
    let enemy_in_sight = mind_now.current_enemy.is_some() && mind_now.frame_enemy_vis;
    if mind_now.chat_time > now && !enemy_in_sight {
        return false;
    }
    if enemy_in_sight {
        mind_now.do_chat = 0;
        mind_now.chat_team = 0;
        return true;
    }
    if mind_now.chat_time <= now {
        let line_end = mind_now
            .current_chat
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(mind_now.current_chat.len());
        acts.said = Some((
            mind_now.current_chat[..line_end].to_vec(),
            mind_now.chat_team != 0,
        ));
        mind_now.chat_team = 0;
        let greeting = mind_now.do_chat == 2;
        if greeting {
            reply_greetings(minds, me, &game.chat(senses), rng);
        }
        mind(minds, me).do_chat = 0;
    }
    true
}
