//! The fighting half of a bot's frame (OpenJK `codemp/game/ai_main.c`'s
//! `StandardBotAI`; see [`crate::bot_standard`]): the Force power it picks, its weapon
//! changes, its reaction, answering a duel challenge, its aim, its saber and fists,
//! a target to shoot, planting mines, the Jedi Master's saber, moving and jumping,
//! shooting a danger, minding friends in its line of fire, and the buttons it presses.

use crate::bot_combat::{
    Arms, Foe, FoeClient, check_det_packs, combat, melee_handling, saber_handling, surface_near,
    weapon_blockable,
};
use crate::bot_input::{
    ACTION_ALT_ATTACK, ACTION_ATTACK, ACTION_CROUCH, ACTION_FORCEPOWER, ACTION_MOVEFORWARD,
    ACTION_MOVELEFT, ACTION_MOVERIGHT,
};
use crate::bot_moves::{
    AimTarget, BotMoveWorld, aim_leading, aim_offset_goal_angles, trace_duck, trace_jump,
    trace_strafe, waiting_for_now,
};
use crate::bot_routes::BotRoutes;
use crate::bot_senses::{BotSenses, in_field_of_vision, mind_tricked};
use crate::bot_standard::{
    BotActs, StandardGame, entity_visible, health_of, length, org_visible, sub,
};
use crate::bot_tactics::{
    alt_firing, friend_in_line_of_fire, keep_alt_from_firing, keep_prim_from_firing, prim_firing,
    strafe_tracing,
};
use crate::bot_think::BotMind;
use crate::bot_weapons::{
    WeaponRange, lead_factor, select_choice_weapon, select_ideal_weapon, try_another_weapon,
    weapon_range,
};
use crate::crt_rand::CrtRand;
use crate::force_powers::{
    FORCE_POWER_NEEDED, FP_ABSORB, FP_DRAIN, FP_GRIP, FP_HEAL, FP_LEVITATION, FP_LIGHTNING,
    FP_PROTECT, FP_PULL, FP_PUSH, FP_RAGE, FP_SABER_OFFENSE, FP_SABER_THROW, FP_SEE, FP_SPEED,
    FP_TEAM_FORCE, FP_TEAM_HEAL, FP_TELEPATHY,
};
use crate::player_angle_math::{normalize, vector_angles};
use crate::player_death::Rng;

const WP_SABER: i32 = 3;
const WP_TRIP_MINE: i32 = 13;
const WP_DET_PACK: i32 = 14;
const WEAPON_CHARGING: i32 = 4;
const WEAPON_CHARGING_ALT: i32 = 5;
const FORCE_LIGHTSIDE: i32 = 1;
const FORCE_DARKSIDE: i32 = 2;
/// Saber styles (`SS_*`).
const SS_FAST: i32 = 1;
const SS_MEDIUM: i32 = 2;
const SS_STRONG: i32 = 3;
const SS_DUAL: i32 = 6;
const SS_STAFF: i32 = 7;
const GT_JEDIMASTER: i32 = 2;
const GT_SINGLE_PLAYER: i32 = 5;
const GT_TEAM: i32 = 6;
/// `SABER_ATTACK_RANGE`, `MELEE_ATTACK_RANGE`, `BOT_SABER_THROW_RANGE`.
const SABER_ATTACK_RANGE: i32 = 128;
const MELEE_ATTACK_RANGE: f32 = 256.0;
const BOT_SABER_THROW_RANGE: f32 = 800.0;
/// `BOT_PLANT_DISTANCE`, `BOT_PLANT_INTERVAL`.
const BOT_PLANT_DISTANCE: f32 = 256.0;
const BOT_PLANT_INTERVAL: i32 = 15_000;
/// `FORCE_LIGHTNING_RADIUS`, `MAX_GRIP_DISTANCE`, `MAX_DRAIN_DISTANCE`, `MAX_TRICK_DISTANCE`.
const FORCE_LIGHTNING_RADIUS: f32 = 300.0;
const MAX_GRIP_DISTANCE: f32 = 256.0;
const MAX_DRAIN_DISTANCE: f32 = 512.0;
const MAX_TRICK_DISTANCE: f32 = 512.0;
const PITCH: usize = 0;
const YAW: usize = 1;

/// Whether the bot uses the Force this frame (`useTheForce`), against someone
/// (`forceHostile`).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Force {
    pub use_it: bool,
    pub hostile: bool,
}

impl Force {
    fn pick(&mut self, acts: &mut BotActs, power: usize, hostile: bool) {
        acts.force_selected = Some(power);
        self.use_it = true;
        self.hostile = hostile;
    }
}

fn knows(game: &StandardGame, power: usize) -> bool {
    game.me.powers_known & (1 << power) != 0
}

/// More Force points than `power` costs at its level (`forcePowerNeeded`).
fn affords(game: &StandardGame, power: usize) -> bool {
    let level = game.me.force_levels[power].clamp(0, 3) as usize;
    game.me.force_power > FORCE_POWER_NEEDED[level][power]
}

/// Where the enemy stands (`client->ps.origin`, or a thing's `s.origin`).
fn enemy_origin(senses: &dyn BotSenses, enemy: i32) -> [f32; 3] {
    match senses.client(enemy) {
        Some(client) => client.origin,
        None => senses
            .things()
            .iter()
            .find(|thing| thing.number == enemy)
            .map_or([0.0; 3], |thing| thing.origin),
    }
}

/// The enemy as the fighting routines read it.
fn foe(senses: &dyn BotSenses, enemy: Option<i32>) -> Option<Foe> {
    let enemy = enemy?;
    let client = senses.client(enemy).map(|client| FoeClient {
        on_ground: client.on_ground,
        weapon: client.weapon,
        saber_move: client.saber_move,
    });
    Some(Foe {
        origin: enemy_origin(senses, enemy),
        client,
    })
}

/// The Force power it picks: pushing off a missile or a grip first; the dark side's
/// grip, lightning, rage and drain, or the light side's absorb, mind trick and protect,
/// at an enemy in sight; then push, speed, sight and pull; healing, even with no enemy.
/// A hostile power the enemy cannot be touched by is not used.
pub(crate) fn choose_force_power(
    mind: &BotMind,
    me: usize,
    game: &StandardGame,
    senses: &dyn BotSenses,
    acts: &mut BotActs,
) -> Force {
    let time = game.level_time();
    let bot = me as i32;
    let me = &game.me;
    let mut force = Force::default();
    if mind.force_jump_charge_time > time {
        force = Force {
            use_it: true,
            hostile: false,
        };
    }
    if let Some(enemy) = mind.current_enemy.and_then(|enemy| senses.client(enemy))
        && mind.frame_enemy_vis
        && mind.force_jump_charge_time < time
    {
        let facing = in_field_of_vision(
            mind.viewangles,
            50.0,
            vector_angles(sub(enemy.origin, mind.eye)),
        );
        let distance = mind.frame_enemy_len;
        if knows(game, FP_PUSH)
            && (mind.do_force_push > time || me.grip_being_gripped > time as f32)
            && affords(game, FP_PUSH)
        {
            force.pick(acts, FP_PUSH, true);
        } else if me.force_side == FORCE_DARKSIDE {
            if knows(game, FP_GRIP) && me.powers_active & (1 << FP_GRIP) != 0 && facing {
                // Already gripping: hold it.
                force.pick(acts, FP_GRIP, true);
            } else if knows(game, FP_LIGHTNING)
                && distance < FORCE_LIGHTNING_RADIUS
                && me.force_power > 50
                && facing
            {
                force.pick(acts, FP_LIGHTNING, true);
            } else if knows(game, FP_GRIP)
                && distance < MAX_GRIP_DISTANCE
                && affords(game, FP_GRIP)
                && facing
            {
                force.pick(acts, FP_GRIP, true);
            } else if knows(game, FP_RAGE) && me.health < 25 && affords(game, FP_RAGE) {
                force.pick(acts, FP_RAGE, false);
            } else if knows(game, FP_DRAIN)
                && distance < MAX_DRAIN_DISTANCE
                && me.force_power > 50
                && facing
                && enemy.force_power > 10
                && enemy.force_side == FORCE_LIGHTSIDE
            {
                force.pick(acts, FP_DRAIN, true);
            }
        } else if me.force_side == FORCE_LIGHTSIDE {
            if knows(game, FP_ABSORB) && me.grip_cripple != 0 && affords(game, FP_ABSORB) {
                force.pick(acts, FP_ABSORB, false);
            } else if knows(game, FP_ABSORB)
                && me.electrify_time >= time
                && affords(game, FP_ABSORB)
            {
                force.pick(acts, FP_ABSORB, false);
            } else if knows(game, FP_TELEPATHY)
                && distance < MAX_TRICK_DISTANCE
                && affords(game, FP_TELEPATHY)
                && facing
                && enemy.force_powers_active & (1 << FP_SEE) == 0
            {
                force.pick(acts, FP_TELEPATHY, true);
            } else if knows(game, FP_ABSORB)
                && me.health < 75
                && enemy.force_side == FORCE_DARKSIDE
                && affords(game, FP_ABSORB)
            {
                force.pick(acts, FP_ABSORB, false);
            } else if knows(game, FP_PROTECT) && me.health < 35 && affords(game, FP_PROTECT) {
                force.pick(acts, FP_PROTECT, false);
            }
        }
        if !force.use_it {
            if knows(game, FP_PUSH)
                && me.grip_being_gripped > time as f32
                && affords(game, FP_PUSH)
                && facing
            {
                force.pick(acts, FP_PUSH, true);
            } else if knows(game, FP_SPEED) && me.health < 25 && affords(game, FP_SPEED) {
                force.pick(acts, FP_SPEED, false);
            } else if knows(game, FP_SEE) && mind_tricked(bot, enemy) && affords(game, FP_SEE) {
                force.pick(acts, FP_SEE, false);
            } else if knows(game, FP_PULL) && distance < 256.0 && me.force_power > 75 && facing {
                force.pick(acts, FP_PULL, true);
            }
        }
    }
    if !force.use_it && knows(game, FP_HEAL) && me.health < 50 && affords(game, FP_HEAL) {
        if me.force_levels[FP_HEAL] > 1
            || (mind.current_enemy.is_none() && mind.is_camping > time as f32)
        {
            // Meditating to heal only while camping, below the second level.
            force.pick(acts, FP_HEAL, false);
        }
    }
    if force.use_it
        && force.hostile
        && mind
            .current_enemy
            .and_then(|enemy| senses.client(enemy))
            .is_some()
        && let Some(power) = acts.force_selected
        && !(game.usable_on_enemy)(power)
    {
        force = Force::default();
    }
    force
}

/// Its weapon: out of ammunition another; a weapon it must hold (a mine to plant, a det
/// pack to set off) or the best one. Whether a change was asked for (the frame ends).
pub(crate) fn change_weapons(
    mind: &mut BotMind,
    game: &StandardGame,
    senses: &dyn BotSenses,
) -> bool {
    let time = game.level_time();
    let armoury = game.armoury(mind, senses);
    if !armoury.loaded(game.me.weapon.max(0) as usize, false) {
        return try_another_weapon(mind, &armoury);
    }
    if mind
        .current_enemy
        .is_some_and(|enemy| mind.last_visible_enemy_index == enemy)
        && mind.frame_enemy_vis
        && mind.force_weapon_select != 0
    {
        mind.force_weapon_select = 0;
    }
    if mind.plant_continue > time {
        mind.do_attack = 1;
        mind.destination_grab_time = 0.0;
    }
    if mind.force_weapon_select == 0
        && game.me.has_det_pack_planted
        && mind.plant_kill_em_all > time
    {
        mind.force_weapon_select = WP_DET_PACK;
    }
    let chosen = if mind.force_weapon_select != 0 {
        select_choice_weapon(mind, mind.force_weapon_select, true, &armoury)
    } else {
        0
    };
    if chosen != 0 {
        return chosen == 2;
    }
    select_ideal_weapon(mind, game.weights, &armoury)
}

/// Its reaction time (its reflex over its skill, up to two seconds) runs from when it
/// has no enemy; a det pack it means to set off is set off.
pub(crate) fn react(mind: &mut BotMind, game: &StandardGame) {
    let time = game.level_time();
    let reaction = (mind.skills.reflex as f32 / mind.skill).clamp(0.0, 2000.0);
    if mind.current_enemy.is_none() {
        mind.time_to_react = time as f32 + reaction;
    }
    if game.me.weapon == WP_DET_PACK
        && game.me.has_det_pack_planted
        && mind.plant_kill_em_all > time
    {
        mind.do_alt_attack = 1;
    }
}

/// `bot_honorableduelacceptance`: an enemy close by with its saber put away, facing it,
/// is answered — its own saber put away, then the duel taken — and the bot holds still.
/// The reference tests the direction as if it were angles.
pub(crate) fn answer_challenge(
    mind: &mut BotMind,
    me: usize,
    game: &StandardGame,
    senses: &dyn BotSenses,
    acts: &mut BotActs,
) {
    let time = game.level_time();
    if !game.honorable_duels {
        return;
    }
    let Some(enemy) = mind.current_enemy.and_then(|enemy| senses.client(enemy)) else {
        return;
    };
    if !(game.me.weapon == WP_SABER
        && game.private_duels
        && mind.frame_enemy_vis
        && mind.frame_enemy_len < 400.0
        && enemy.weapon == WP_SABER
        && enemy.saber_holstered)
    {
        return;
    }
    if !in_field_of_vision(mind.viewangles, 100.0, sub(enemy.origin, mind.eye)) {
        return;
    }
    if game.me.saber_holstered == 0 {
        acts.toggle_saber = true;
    } else if enemy.duel_index == me as i32 && enemy.duel_time > time && !game.me.duel_in_progress {
        acts.engage_duel = true;
    }
    mind.do_attack = 0;
    mind.do_alt_attack = 0;
    mind.bot_challenging_time = time + 100;
    mind.be_still = (time + 100) as f32;
}

/// Once it has reacted and its enemy is fresh: it fights one in sight (or keeps a charge
/// going), stays near, and aims — at where it last saw a hidden one, else at the head,
/// leading it with a slow weapon, wobbled by its skill.
#[allow(clippy::too_many_arguments)]
pub(crate) fn aim(
    mind: &mut BotMind,
    me: usize,
    game: &StandardGame,
    senses: &dyn BotSenses,
    world: &mut dyn BotMoveWorld,
    rng: &mut Rng,
    rand: &mut CrtRand,
) {
    let time = game.level_time();
    let Some(enemy) = mind.current_enemy else {
        return;
    };
    // `level.time + (ENEMY_FORGET_MS - ENEMY_FORGET_MS * 0.2)` is a double.
    if !(mind.time_to_react < time as f32
        && f64::from(mind.enemy_seen_time) > f64::from(time) + 8000.0)
    {
        return;
    }
    let me_state = &game.me;
    if mind.frame_enemy_vis {
        let arms = Arms {
            weapon: me_state.weapon,
            weapon_state: me_state.weapon_state,
            weapon_charge_time: me_state.weapon_charge_time,
            rocket_lock_time: me_state.rocket_lock_time,
            rocket_last_valid_time: me_state.rocket_last_valid_time,
            ammo: me_state.ammo,
        };
        combat(mind, &arms, foe(senses, Some(enemy)).as_ref(), time, rng);
    } else if me_state.weapon_state == WEAPON_CHARGING_ALT {
        // Keep charging, in case it is seen again before it is forgotten.
        mind.do_alt_attack = 1;
    } else if me_state.weapon_state == WEAPON_CHARGING {
        mind.do_attack = 1;
    }
    if mind.destination_grab_time > (time + 100) as f32 {
        // Stay in the area it fights in.
        mind.destination_grab_time = (time + 100) as f32;
    }
    let client = senses.client(enemy);
    let mut head = enemy_origin(senses, enemy);
    if let Some(client) = client {
        head[2] += client.viewheight as f32;
    }
    if !mind.frame_enemy_vis {
        if org_visible(world, mind.eye, mind.last_enemy_spotted, -1) {
            mind.goal_angles = vector_angles(sub(mind.last_enemy_spotted, mind.eye));
            // The flechette's lob at a hidden enemy compares a 0-or-1 length with 128:
            // it never fires.
        }
        return;
    }
    let lead = lead_factor(me_state.weapon);
    if mind.skills.accuracy / mind.skill <= 8.0 && lead != 0.0 {
        aim_leading(mind, client.map(|client| client.velocity), head, lead);
    } else {
        mind.goal_angles = vector_angles(sub(head, mind.eye));
    }
    let target = AimTarget {
        tricked: client.is_some_and(|client| mind_tricked(me as i32, client)),
        revenge_level: if mind.revenge_enemy.is_some() && mind.current_enemy == mind.revenge_enemy {
            mind.revenge_hate_level
        } else {
            0
        },
        moving: client.is_some_and(|client| client.moving),
        self_moving: me_state.moving,
    };
    aim_offset_goal_angles(mind, Some(&target), time, rand);
}

/// Its saber or fists at close quarters: the strong style against a healthy enemy now
/// and then, the medium one against a hurt one, the fast one against a dying one; saber
/// fighting within reach, else the saber thrown; fists within reach. Whether it strafes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn close_in(
    mind: &mut BotMind,
    me: usize,
    game: &StandardGame,
    senses: &dyn BotSenses,
    world: &mut dyn BotMoveWorld,
    rng: &mut Rng,
    acts: &mut BotActs,
) -> bool {
    let time = game.level_time();
    let me_state = &game.me;
    if me_state.saber_in_flight {
        mind.saber_throw_time = time + rng.irand(4000, 10_000);
    }
    let Some(enemy) = mind.current_enemy else {
        return false;
    };
    let target = foe(senses, Some(enemy));
    match weapon_range(me_state.weapon) {
        WeaponRange::Saber => {
            let towards = vector_angles(sub(enemy_origin(senses, enemy), mind.eye));
            if mind.saber_power_time < time {
                // Not the strong attacks all the time.
                mind.saber_power = rng.irand(1, 10) <= 5;
                mind.saber_power_time = time + rng.irand(3000, 15_000);
            }
            let style = me_state.saber_anim_level;
            if style != SS_STAFF && style != SS_DUAL {
                let health = health_of(senses, enemy);
                let offense = me_state.force_levels[FP_SABER_OFFENSE];
                let wanted = if health > 75 && offense > 2 {
                    (style != SS_STRONG && mind.saber_power).then_some(())
                } else if health > 40 && offense > 1 {
                    (style != SS_MEDIUM).then_some(())
                } else {
                    (style != SS_FAST).then_some(())
                };
                if wanted.is_some() {
                    acts.cycle_saber_style = true;
                }
            }
            let reach = if game.rules.gametype == GT_SINGLE_PLAYER {
                SABER_ATTACK_RANGE * 3
            } else {
                SABER_ATTACK_RANGE
            };
            let distance = mind.frame_enemy_len;
            if distance <= reach as f32 {
                saber_handling(mind, me as i32, target.as_ref(), time, world, rng);
                return mind.frame_enemy_len < 80.0;
            }
            if mind.saber_throw_time < time
                && !me_state.saber_in_flight
                && knows(game, FP_SABER_THROW)
                && in_field_of_vision(mind.viewangles, 30.0, towards)
                && distance < BOT_SABER_THROW_RANGE
                && style != SS_STAFF
            {
                mind.do_alt_attack = 1;
                mind.do_attack = 0;
            } else if me_state.saber_in_flight
                && distance > 300.0
                && distance < BOT_SABER_THROW_RANGE
            {
                mind.do_alt_attack = 1;
                mind.do_attack = 0;
            }
            false
        }
        WeaponRange::Melee if mind.frame_enemy_len <= MELEE_ATTACK_RANGE => {
            melee_handling(mind, target.as_ref(), time, world, rng);
            true
        }
        _ => false,
    }
}

/// An objective to shoot (`shootGoal`): with no close enemy it is aimed at, and shot when
/// in view and in sight.
pub(crate) fn shoot_goal(
    mind: &mut BotMind,
    me: usize,
    game: &StandardGame,
    world: &mut dyn BotMoveWorld,
) {
    let Some(goal) = mind.shoot_goal else { return };
    let Some(thing) = game
        .objectives
        .things
        .iter()
        .find(|thing| thing.entity == goal)
    else {
        return;
    };
    if thing.health <= 0 || !thing.takes_damage {
        return;
    }
    let middle = [0, 1, 2].map(|axis| (thing.absmax[axis] + thing.absmin[axis]) / 2.0);
    if mind.current_enemy.is_none() || mind.frame_enemy_len > 256.0 {
        let angles = vector_angles(sub(middle, mind.eye));
        mind.goal_angles = angles;
        if in_field_of_vision(mind.viewangles, 30.0, angles)
            && entity_visible(world, mind.origin, middle, me as i32, goal)
        {
            mind.do_attack = 1;
        }
    }
}

/// Mines: its det pack set off when the enemy nears it; else, where a hidden enemy was
/// just seen close by, a trip mine or det pack taken out and planted on a surface ahead
/// (once each 15 seconds). Whether a weapon change was asked for (the frame ends).
pub(crate) fn plant_mines(
    mind: &mut BotMind,
    me: usize,
    game: &StandardGame,
    senses: &dyn BotSenses,
    world: &mut dyn BotMoveWorld,
) -> bool {
    let time = game.level_time();
    if game.me.has_det_pack_planted {
        let enemy = mind
            .current_enemy
            .and_then(|enemy| senses.client(enemy).map(|client| (enemy, client.origin)));
        check_det_packs(mind, game.me.det_pack, enemy, time, world);
        return false;
    }
    let lost_from_sight = mind
        .current_enemy
        .is_some_and(|enemy| mind.last_visible_enemy_index == enemy)
        && !mind.frame_enemy_vis;
    if !(lost_from_sight
        && mind.plant_time < time
        && mind.do_attack == 0
        && mind.do_alt_attack == 0)
    {
        if mind.plant_continue < time {
            mind.force_weapon_select = 0;
        }
        return false;
    }
    let moved = length(sub(mind.origin, mind.here_when_spotted));
    if !(mind.plant_decided > time
        || (mind.frame_enemy_len < BOT_PLANT_DISTANCE * 2.0 && moved < BOT_PLANT_DISTANCE))
    {
        return false;
    }
    let armoury = game.armoury(mind, senses);
    let mine = select_choice_weapon(mind, WP_TRIP_MINE, false, &armoury);
    let det = select_choice_weapon(mind, WP_DET_PACK, false, &armoury);
    if mind.plant_decided > time
        && mind.force_weapon_select != 0
        && game.me.weapon == mind.force_weapon_select
    {
        mind.do_attack = 1;
        mind.plant_decided = 0;
        mind.plant_time = time + BOT_PLANT_INTERVAL;
        mind.plant_continue = time + 500;
        mind.be_still = (time + 500) as f32;
    } else if (mine != 0 || det != 0) && surface_near(mind, me as i32, world) {
        // Mines first, then det packs.
        let weapon = if mine == 0 { WP_DET_PACK } else { WP_TRIP_MINE };
        let chosen = select_choice_weapon(mind, weapon, true, &armoury);
        if chosen != 0 && chosen != 2 {
            mind.plant_decided = time + 1000;
            mind.force_weapon_select = weapon;
            return true;
        } else if chosen == 2 {
            mind.force_weapon_select = weapon;
            return true;
        }
    }
    false
}

/// Jedi Master: the saber lying free within 256 units and in sight is gone for.
pub(crate) fn jedi_master_saber(
    mind: &mut BotMind,
    me: usize,
    game: &StandardGame,
    world: &mut dyn BotMoveWorld,
) {
    if game.rules.gametype != GT_JEDIMASTER || game.me.is_jedi_master || mind.jm_state != -1 {
        return;
    }
    let Some((_, true, saber)) = game.objectives.jedi_master_saber else {
        return;
    };
    if length(sub(mind.origin, saber)) < 256.0 && org_visible(world, mind.origin, saber, me as i32)
    {
        mind.goal_position = saber;
    }
}

/// Moving on: towards its goal position unless it holds still, waits for a lift or has
/// reached a Force jump's spot — strafing about an enemy at close quarters (not into
/// walls or off ledges), jumping, crouching or sidestepping as the way ahead needs;
/// then its jumps held, charged and made, and crouching.
#[allow(clippy::too_many_arguments)]
pub(crate) fn move_on(
    mind: &mut BotMind,
    me: usize,
    game: &StandardGame,
    routes: &BotRoutes,
    world: &mut dyn BotMoveWorld,
    rng: &mut Rng,
    strafe: bool,
    halt: bool,
    acts: &mut BotActs,
) {
    let time = game.level_time();
    let now = time as f32;
    let me_state = &game.me;
    let goal = mind.goal_position;
    if mind.be_still < now && !waiting_for_now(mind, routes, me as i32, goal, time, world) && !halt
    {
        let mut direction = sub(goal, mind.origin);
        normalize(&mut direction);
        mind.goal_movedir = direction;
        if mind.jump_time > now && mind.j_delay < now && me_state.last_upmove > 0 {
            mind.be_still = (time + 200) as f32;
        } else {
            mind.input.move_towards(direction, 5000.0);
        }
        if strafe {
            strafe_tracing(mind, me as i32, time, world, rng);
        }
        if strafe && mind.melee_strafe_disable < now {
            mind.input.act(if mind.melee_strafe_dir != 0 {
                ACTION_MOVERIGHT
            } else {
                ACTION_MOVELEFT
            });
        }
        if trace_jump(mind, me as i32, me_state.weapon, goal, time, world) {
            mind.jump_time = (time + 100) as f32;
        } else if trace_duck(mind, me as i32, goal, world) {
            mind.duck_time = (time + 100) as f32;
        } else {
            match trace_strafe(mind, me as i32, me_state.on_ground, goal, world) {
                1 => mind.input.act(ACTION_MOVERIGHT),
                2 => mind.input.act(ACTION_MOVELEFT),
                _ => {}
            }
        }
    }
    if mind.force_jump_charge_time > time {
        mind.jump_time = 0.0;
    }
    if mind.jump_prep > now {
        mind.force_jump_charge_time = 0;
    }
    if mind.force_jump_charge_time > time {
        mind.jump_hold_time = ((mind.force_jump_charge_time - time) / 2 + time) as f32;
        mind.force_jump_charge_time = 0;
    }
    if mind.jump_hold_time > now {
        mind.jump_time = mind.jump_hold_time;
    }
    if mind.jump_time > now && mind.j_delay < now {
        if mind.jump_hold_time > now {
            mind.input.jump();
            let forward = match mind.wp_current {
                Some(current) => routes.waypoints[current].origin[2] - mind.origin[2] < 64.0,
                None => true,
            };
            if forward {
                mind.input.act(ACTION_MOVEFORWARD);
            }
            if !me_state.on_ground {
                acts.jump_held = true;
            }
        } else if !me_state.jump_held {
            mind.input.jump();
        }
    }
    if mind.duck_time > now {
        mind.input.act(ACTION_CROUCH);
    }
}

/// A danger it can destroy (`dangerousObject`), with no enemy in sight and a gun that
/// reaches: aimed at, a little off, and shot when in view and in sight.
pub(crate) fn shoot_danger(
    mind: &mut BotMind,
    me: usize,
    game: &StandardGame,
    senses: &dyn BotSenses,
    world: &mut dyn BotMoveWorld,
    rng: &mut Rng,
) {
    let Some(danger) = mind.dangerous_object else {
        return;
    };
    let Some(thing) = senses
        .things()
        .iter()
        .find(|thing| thing.number == danger)
        .copied()
    else {
        return;
    };
    let weapon = game.me.weapon;
    if !(thing.in_use
        && thing.health > 0
        && thing.takes_damage
        && (!mind.frame_enemy_vis || mind.current_enemy.is_none()))
    {
        return;
    }
    if !matches!(weapon_range(weapon), WeaponRange::Mid | WeaponRange::Long)
        || weapon == WP_DET_PACK
        || weapon == WP_TRIP_MINE
        || mind.shoot_goal.is_some()
    {
        return;
    }
    let towards = sub(thing.current_origin, mind.eye);
    if length(towards) <= 256.0 {
        return;
    }
    let angles = vector_angles(towards);
    mind.goal_angles = angles;
    if rng.irand(1, 10) < 5 {
        mind.goal_angles[YAW] += rng.irand(0, 3) as f32;
        mind.goal_angles[PITCH] += rng.irand(0, 3) as f32;
    } else {
        mind.goal_angles[YAW] -= rng.irand(0, 3) as f32;
        mind.goal_angles[PITCH] -= rng.irand(0, 3) as f32;
    }
    if in_field_of_vision(mind.viewangles, 30.0, angles)
        && entity_visible(world, mind.origin, thing.current_origin, me as i32, danger)
    {
        mind.do_attack = 1;
    }
}

/// A friend in its line of fire: its shots held (and a hostile power); a hurt or drained
/// friend healed or given Force (in a team game, even when it does not shoot).
pub(crate) fn mind_friends(
    mind: &mut BotMind,
    me: usize,
    game: &StandardGame,
    senses: &dyn BotSenses,
    world: &mut dyn BotMoveWorld,
    force: &mut Force,
    acts: &mut BotActs,
) {
    let state = game.me.weapon_state;
    let help = |friend: usize, force: &mut Force, acts: &mut BotActs| {
        let Some(client) = senses.client(friend as i32) else {
            return;
        };
        if client.health <= 50 && affords(game, FP_TEAM_HEAL) {
            force.pick(acts, FP_TEAM_HEAL, false);
        } else if client.force_power <= 50 && affords(game, FP_TEAM_FORCE) {
            force.pick(acts, FP_TEAM_FORCE, false);
        }
    };
    if prim_firing(mind, state) || alt_firing(mind, state) {
        let Some(friend) = friend_in_line_of_fire(mind, me, &game.squad, world) else {
            return;
        };
        if prim_firing(mind, state) {
            keep_prim_from_firing(mind, state);
        }
        if alt_firing(mind, state) {
            keep_alt_from_firing(mind, state);
        }
        if force.use_it && force.hostile {
            force.use_it = false;
        }
        if !force.use_it {
            help(friend, force, acts);
        }
    } else if game.rules.gametype >= GT_TEAM
        && let Some(friend) = friend_in_line_of_fire(mind, me, &game.squad, world)
        && !force.use_it
    {
        help(friend, force, acts);
    }
}

/// The buttons: no second det pack, no attack into a blockable enemy while defending,
/// a saber lock mashed at random, still while answering a challenge, a knocked-away
/// saber called back; then attack or alternate attack, and the Force power (a charged
/// jump's levitation first), with `bot_forcepowers`.
pub(crate) fn press_buttons(
    mind: &mut BotMind,
    game: &StandardGame,
    senses: &dyn BotSenses,
    rand: &mut CrtRand,
    force: &mut Force,
    acts: &mut BotActs,
) {
    let time = game.level_time();
    let me_state = &game.me;
    if mind.do_attack != 0 && me_state.weapon == WP_DET_PACK && me_state.has_det_pack_planted {
        // One det pack at a time.
        mind.do_attack = 0;
    }
    if mind.do_attack != 0
        && me_state.weapon == WP_SABER
        && mind.saber_defending != 0
        && mind
            .current_enemy
            .and_then(|enemy| senses.client(enemy))
            .is_some_and(|client| weapon_blockable(client.weapon))
    {
        mind.do_attack = 0;
    }
    if me_state.saber_lock_time > time {
        mind.do_attack = i32::from(rand.next() % 10 < 5);
    }
    if mind.bot_challenging_time > time {
        mind.do_attack = 0;
        mind.do_alt_attack = 0;
    }
    if me_state.weapon == WP_SABER && me_state.saber_in_flight && me_state.saber_entity_num == 0 {
        // The saber knocked away: keep calling it back.
        mind.do_attack = 1;
        mind.do_alt_attack = 0;
    }
    if mind.do_attack != 0 {
        mind.input.act(ACTION_ATTACK);
    } else if mind.do_alt_attack != 0 {
        mind.input.act(ACTION_ALT_ATTACK);
    }
    if force.use_it && force.hostile && mind.bot_challenging_time > time {
        force.use_it = false;
    }
    if force.use_it {
        if mind.force_jump_charge_time > time {
            acts.force_selected = Some(FP_LEVITATION);
            mind.input.act(ACTION_FORCEPOWER);
        } else if game.force_powers {
            mind.input.act(ACTION_FORCEPOWER);
        }
    }
}
