//! A stormtrooper in a fight (`NPC_BSST_Attack`, `NPC_AI_Stormtrooper.c:2437-2762`): its
//! enemy kept or found (`NPC_CheckEnemyExt`), its squad commanded by whoever thinks first
//! ([`crate::npc_st_commander`]), then its own sense of the enemy — in view, in sight, a
//! clear shot (`NPC_ShotEntity`) or someone in the way (`ST_ResolveBlockedShot`) — where it
//! goes (`ST_CheckMoveState`, `ST_Move`), whether it fires on where the enemy was
//! (`ST_CheckFireState`), ducking, facing and, once its attack delay has run out, firing
//! (`WeaponThink`).
//!
//! Held to `tools/game-oracle/npcst.c` (`game-npcst.txt`).

use crate::npc_nav::NIF_COLLISION;
use crate::npc_senses::{Spot, distance_squared, spot};
use crate::npc_spawn::NpcHost;
use crate::npc_st::{SCF_CHASE_ENEMIES, SCF_DONT_FIRE, SCF_FIRE_WEAPON, speech, squad};
use crate::npc_world::NpcWorld;
use sjk_protocol::UserCommand;

/// `MIN_ROCKET_DIST_SQUARED`.
const MIN_ROCKET_DIST_SQUARED: f32 = 16_384.0;
/// `weapon_t`s the fight names.
const WP_NONE: i32 = 0;
const WP_SABER: i32 = 3;
const WP_DISRUPTOR: i32 = 6;
const WP_REPEATER: i32 = 8;
const WP_FLECHETTE: i32 = 10;
const WP_ROCKET_LAUNCHER: i32 = 11;
const WP_THERMAL: i32 = 12;
const WP_TRIP_MINE: i32 = 13;
const WP_DET_PACK: i32 = 14;
const WP_EMPLACED_GUN: i32 = 17;
/// `SCF_ALT_FIRE`.
const SCF_ALT_FIRE: u32 = 0x40;
/// `NPCTEAM_PLAYER`.
const NPCTEAM_PLAYER: i32 = 2;
/// `BUTTON_ATTACK`, `BUTTON_ALT_ATTACK`.
const BUTTON_ATTACK: u16 = 1;
const BUTTON_ALT_ATTACK: u16 = 128;
/// `ps.weaponTime`.
const PS_WEAPON_TIME: usize = 10;

/// What the fight senses this think (the file's statics: `enemyLOS`, `enemyCS`,
/// `enemyInFOV`, `hitAlly`, `faceEnemy`, `move`, `shoot`, `enemyDist`, `impactPos`).
#[derive(Clone, Copy, Debug, Default)]
pub struct AttackSense {
    pub enemy_los: bool,
    pub enemy_cs: bool,
    pub enemy_in_fov: bool,
    pub hit_ally: bool,
    pub face_enemy: bool,
    pub moving: bool,
    pub shoot: bool,
    pub enemy_dist: f32,
    pub impact: [f32; 3],
}

/// The weapons whose splash makes a trooper keep its distance (`ST_CheckFireState`).
fn explosive(weapon: i32, flags: u32) -> bool {
    matches!(
        weapon,
        WP_ROCKET_LAUNCHER | WP_FLECHETTE | WP_THERMAL | WP_TRIP_MINE | WP_DET_PACK
    ) || (weapon == WP_REPEATER && flags & SCF_ALT_FIRE != 0)
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSST_Attack` (`NPC_AI_Stormtrooper.c:2437-2762`).
    pub fn bs_st_attack(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        if self.actors[me].mind.fight.pain_debounce_time > level_time {
            self.update_angles(me, true, true, command);
            return;
        }
        if !self.check_enemy_ext(me) {
            self.actors[me].mind.enemy = None;
            if self.actors[me].player_team == NPCTEAM_PLAYER {
                self.bs_patrol(me, command);
            } else {
                self.bs_st_patrol(me, command);
            }
            return;
        }
        if self.actors[me]
            .mind
            .timers
            .done("interrogating", level_time)
        {
            self.get_group(me);
        }
        match self.actors[me].mind.tactics.group {
            Some(group) => {
                if !self.level.groups[group].processed {
                    self.st_commander(me);
                }
            }
            None => {
                if self.actors[me].mind.timers.done("flee", level_time) {
                    let alert = self.check_alerts(me, -1, false, crate::npc_senses::AEL_DANGER);
                    if self.check_for_danger(me, alert) {
                        self.st_speech(me, speech::COVER, 0.0);
                        self.update_angles(me, true, true, command);
                        return;
                    }
                }
            }
        }
        if self.actors[me].mind.enemy.is_none() {
            self.bs_st_patrol(me, command);
            return;
        }
        let Some(mut sense) = self.sense_enemy(me, command) else {
            return;
        };
        self.check_move_state(me, &mut sense, command);
        self.check_fire_state(me, &mut sense);
        self.act(me, sense, command);
    }

    /// `NPC_BSST_Attack`'s sense of its enemy (`NPC_AI_Stormtrooper.c:2521-2640`): how far,
    /// in front or not, in sight, and whether a shot would hit it. `None` when a sniper
    /// takes up its alternate fire and waits.
    fn sense_enemy(&mut self, me: usize, command: &mut UserCommand) -> Option<AttackSense> {
        let level_time = self.level_time;
        let enemy = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))?;
        let npc = &self.actors[me];
        let mut sense = AttackSense {
            moving: true,
            enemy_dist: distance_squared(npc.current_origin, enemy.origin),
            ..AttackSense::default()
        };
        let mut to_enemy = crate::npc_senses::subtract(enemy.origin, npc.current_origin);
        crate::player_angle_math::normalize(&mut to_enemy);
        let (forward, _) = crate::pmove::flight::flight_axes(npc.player.view_angles());
        let forward = forward.to_array();
        let dot = to_enemy[0] * forward[0] + to_enemy[1] * forward[1] + to_enemy[2] * forward[2];
        sense.enemy_in_fov = dot > 0.5 || sense.enemy_dist * (1.0 - dot) < 10_000.0;
        let weapon = i32::from(npc.player.weapon());
        let flags = npc.script_flags;
        if sense.enemy_dist < MIN_ROCKET_DIST_SQUARED {
            if (weapon == WP_FLECHETTE || weapon == WP_REPEATER) && flags & SCF_ALT_FIRE != 0 {
                self.actors[me].script_flags &= !SCF_ALT_FIRE;
            }
        } else if sense.enemy_dist > 65_536.0 && weapon == WP_DISRUPTOR && flags & SCF_ALT_FIRE == 0
        {
            // A sniper takes up its alternate fire (`NPC_ChangeWeapon` does nothing in
            // multiplayer).
            self.actors[me].script_flags |= SCF_ALT_FIRE;
            self.update_angles(me, true, true, command);
            return None;
        }
        let group = self.actors[me].mind.tactics.group;
        if self.clear_los4(me, &enemy) {
            self.group_saw_enemy(group, enemy.origin);
            self.actors[me].mind.tactics.enemy_last_seen_time = level_time;
            sense.enemy_los = true;
            let flags = self.actors[me].script_flags;
            if weapon == WP_NONE {
                self.aim_adjust(me, -1);
            } else if (weapon == WP_ROCKET_LAUNCHER
                || (weapon == WP_FLECHETTE && flags & SCF_ALT_FIRE != 0))
                && sense.enemy_dist < MIN_ROCKET_DIST_SQUARED
            {
                sense.hit_ally = true;
            } else if sense.enemy_in_fov {
                self.aim_at(me, &enemy, &mut sense);
            }
        } else if self
            .host
            .in_pvs(enemy.origin, self.actors[me].current_origin)
        {
            self.actors[me].mind.tactics.enemy_last_seen_time = level_time;
            sense.face_enemy = true;
            self.aim_adjust(me, -1);
        }
        if weapon == WP_NONE {
            sense.face_enemy = false;
            sense.shoot = false;
        } else {
            sense.face_enemy |= sense.enemy_los;
            sense.shoot |= sense.enemy_cs;
        }
        Some(sense)
    }

    /// Whether a shot at the enemy would do (`NPC_AI_Stormtrooper.c:2584-2612`): it hits
    /// the enemy, one of the enemy's side, or something breakable enough — else the trooper
    /// sorts out who is in its way.
    fn aim_at(&mut self, me: usize, enemy: &crate::npc_senses::Body, sense: &mut AttackSense) {
        let (hit, impact) = self.shot_entity(me, enemy);
        sense.impact = impact;
        let npc = &self.actors[me];
        let (enemy_team, own_team, weapon) = (npc.enemy_team, npc.player_team, self.npc(me).weapon);
        let struck = self.body(hit);
        let (takes_damage, health) = match struck {
            Some(body) => (
                self.actor_at(hit)
                    .is_none_or(|at| self.actors[at].takes_damage),
                body.health,
            ),
            None => {
                let health = self.host.damageable_health(hit);
                (
                    health.is_some() || self.host.glass(hit),
                    health.unwrap_or(0),
                )
            }
        };
        let minor =
            takes_damage && (self.host.glass(hit) || health < 40 || weapon == WP_EMPLACED_GUN);
        if hit == enemy.number || struck.is_some_and(|body| body.player_team == enemy_team) || minor
        {
            let group = self.actors[me].mind.tactics.group;
            self.group_clear_shot(group);
            sense.enemy_cs = true;
            self.aim_adjust(me, 2);
            self.actors[me].mind.tactics.enemy_last_seen_location = enemy.origin;
        } else {
            self.aim_adjust(me, 1);
            self.resolve_blocked_shot(me, hit);
            sense.hit_ally = struck.is_some_and(|body| body.player_team == own_team);
        }
    }

    /// `ST_ResolveBlockedShot` (`NPC_AI_Stormtrooper.c:1450-1497`): a squadmate in the way
    /// told to duck, or this trooper stands up — or, neither possible, it moves on.
    fn resolve_blocked_shot(&mut self, me: usize, hit: u16) {
        let level_time = self.level_time;
        let timers = &self.actors[me].mind.timers;
        let (roam, stick) = (
            timers.get("roamTime").unwrap_or(-1),
            timers.get("stick").unwrap_or(-1),
        );
        let stuck = if roam > stick {
            roam - level_time
        } else {
            stick - level_time
        };
        if timers.done("duck", level_time) {
            let group = self.actors[me].mind.tactics.group;
            if group.is_some_and(|group| self.level.groups[group].contains(hit))
                && let Some(member) = self.actor_at(hit)
                && self.actors[member].mind.timers.done("duck", level_time)
                && self.actors[member].mind.timers.done("stand", level_time)
            {
                self.actors[member]
                    .mind
                    .timers
                    .set("duck", level_time, stuck);
                return;
            }
        } else if timers.done("stand", level_time) {
            self.actors[me].mind.timers.set("stand", level_time, stuck);
            return;
        }
        let timers = &mut self.actors[me].mind.timers;
        timers.set("roamTime", level_time, -1);
        timers.set("stick", level_time, -1);
        timers.set("duck", level_time, -1);
        let delay = self.host.irand(1_000, 3_000);
        // The reference's own spelling: a timer nothing reads.
        self.actors[me]
            .mind
            .timers
            .set("attakDelay", level_time, delay);
    }

    /// `ST_CheckMoveState` (`NPC_AI_Stormtrooper.c:1287-1448`): whether the trooper moves by
    /// its squad state, and its arrival at a goal (not its enemy).
    fn check_move_state(&mut self, me: usize, sense: &mut AttackSense, command: &mut UserCommand) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        let group = npc.mind.tactics.group;
        match npc.mind.tactics.squad_state {
            squad::SCOUT => {
                if !npc.mind.timers.done("stick", level_time) {
                    sense.moving = false;
                    return;
                }
                if sense.enemy_los {
                    if sense.enemy_cs && npc.mind.goal.is_some() && npc.mind.goal == npc.mind.enemy
                    {
                        self.update_squad_state(group, me, squad::STAND_AND_SHOOT);
                        sense.moving = false;
                        return;
                    }
                } else {
                    sense.face_enemy = false;
                }
            }
            squad::RETREAT => {
                if npc.mind.goal.is_some() {
                    sense.face_enemy = false;
                } else {
                    self.actors[me].mind.tactics.squad_state = squad::STAND_AND_SHOOT;
                }
            }
            squad::TRANSITION => {
                if npc.mind.goal.is_none() {
                    self.actors[me].mind.tactics.squad_state = squad::STAND_AND_SHOOT;
                }
            }
            squad::POINT => {
                if npc.mind.timers.done("stick", level_time) {
                    self.update_squad_state(group, me, squad::STAND_AND_SHOOT);
                    return;
                }
                sense.moving = false;
                return;
            }
            squad::STAND_AND_SHOOT | squad::COVER => {
                sense.moving = false;
                return;
            }
            squad::IDLE if npc.mind.goal.is_none() => {
                sense.moving = false;
                return;
            }
            _ => {}
        }
        let npc = &self.actors[me];
        if npc.mind.goal.is_none() || npc.mind.goal == npc.mind.enemy {
            return;
        }
        let goal = self.goal_origin(me).unwrap_or([0.0; 3]);
        let npc = &self.actors[me];
        let arrived = crate::npc_nav::hit_nav_goal(
            npc.current_origin,
            npc.mins,
            npc.maxs,
            goal,
            16,
            self.flying(me),
        );
        let state = npc.mind.tactics.squad_state;
        if !(arrived || (state == squad::SCOUT && sense.enemy_los && sense.enemy_dist <= 10_000.0))
        {
            let roam = self.host.irand(4_000, 8_000);
            self.actors[me]
                .mind
                .timers
                .set("roamTime", level_time, roam);
            return;
        }
        let mut next = squad::STAND_AND_SHOOT;
        match state {
            squad::RETREAT => {
                let npc = &self.actors[me];
                let duck = (npc.max_health - npc.health) * 100;
                self.actors[me].mind.timers.set("duck", level_time, duck);
                let hide = self.host.irand(3_000, 7_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("hideTime", level_time, hide);
                self.actors[me]
                    .mind
                    .timers
                    .set("flee", level_time, -level_time);
                next = squad::COVER;
            }
            squad::TRANSITION => {
                let hide = self.host.irand(2_000, 4_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("hideTime", level_time, hide);
            }
            _ => {}
        }
        self.update_squad_state(group, me, next);
        self.reached_goal(me, command);
        let delay = self.host.irand(250, 500);
        self.actors[me]
            .mind
            .timers
            .set("attackDelay", level_time, delay);
        let roam = self.host.irand(1_000, 4_000);
        self.actors[me]
            .mind
            .timers
            .set("roamTime", level_time, roam);
    }

    /// `ST_CheckFireState` (`NPC_AI_Stormtrooper.c:1505-1630`): a trooper standing with no
    /// clear shot, while its squad runs, now and then fires on where the enemy was last
    /// seen — not too near itself, nor, with the enemy long unseen, too far from it.
    fn check_fire_state(&mut self, me: usize, sense: &mut AttackSense) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        let state = npc.mind.tactics.squad_state;
        if sense.enemy_cs
            || matches!(state, squad::RETREAT | squad::TRANSITION | squad::SCOUT)
            || npc.player.velocity() != [0.0; 3]
        {
            return;
        }
        let tactics = npc.mind.tactics;
        let Some(group) = tactics.group else { return };
        let slot = &self.level.groups[group];
        let running = slot.num_state[squad::RETREAT as usize] > 0
            || slot.num_state[squad::TRANSITION as usize] > 0
            || slot.num_state[squad::SCOUT as usize] > 0;
        if sense.hit_ally || !sense.enemy_in_fov || tactics.enemy_last_seen_time <= 0 || !running {
            return;
        }
        if level_time - tactics.enemy_last_seen_time >= 10_000
            || level_time - slot.last_seen_enemy_time >= 10_000
        {
            return;
        }
        let group_unseen = level_time - slot.last_seen_enemy_time > 5_000;
        if self.host.irand(0, 10) != 0 {
            return;
        }
        let muzzle = spot(&self.npc(me), Spot::Head);
        if sense.impact == [0.0; 3] {
            let (forward, _) =
                crate::pmove::flight::flight_axes(self.actors[me].player.view_angles());
            let forward = forward.to_array();
            let end: [f32; 3] = std::array::from_fn(|axis| muzzle[axis] + 8_192.0 * forward[axis]);
            let number = self.actors[me].number;
            sense.impact = self
                .trace_bodies(
                    muzzle,
                    [0.0; 3],
                    [0.0; 3],
                    end,
                    number,
                    crate::npc_aim::MASK_SHOT,
                )
                .end_position;
        }
        let npc = &self.actors[me];
        let splash = explosive(
            npc.state
                .raw_field(crate::npc_spawn::es::WEAPON)
                .unwrap_or(0) as i32,
            npc.script_flags,
        );
        if distance_squared(sense.impact, muzzle) < if splash { 65_536.0 } else { 16_384.0 } {
            return;
        }
        if (level_time - tactics.enemy_last_seen_time > 5_000 || group_unseen)
            && distance_squared(sense.impact, tactics.enemy_last_seen_location)
                > if splash { 262_144.0 } else { 65_536.0 }
        {
            return;
        }
        let mut direction = crate::npc_senses::subtract(tactics.enemy_last_seen_location, muzzle);
        crate::player_angle_math::normalize(&mut direction);
        let angles = crate::player_angle_math::vector_angles(direction);
        let npc = &mut self.actors[me];
        npc.desired_yaw = angles[1];
        npc.mind.desired_pitch = angles[0];
        sense.shoot = true;
        sense.face_enemy = false;
    }

    /// What `NPC_BSST_Attack` does with what it sensed (`NPC_AI_Stormtrooper.c:2654-2761`):
    /// face the enemy, move toward the goal (`ST_Move`), duck when still, face its way
    /// when not facing the enemy, and fire when its attack delay allows.
    fn act(&mut self, me: usize, mut sense: AttackSense, command: &mut UserCommand) {
        let level_time = self.level_time;
        if sense.face_enemy {
            self.face_enemy(me, true, command);
        }
        let npc = &self.actors[me];
        if npc.script_flags & SCF_CHASE_ENEMIES == 0
            && npc.mind.goal.is_some()
            && npc.mind.goal == npc.mind.enemy
        {
            sense.moving = false;
        }
        let weapon = npc
            .state
            .raw_field(crate::npc_spawn::es::WEAPON)
            .unwrap_or(0) as i32;
        let weapon_time = npc.player.raw_field(PS_WEAPON_TIME).unwrap_or(0) as i32;
        if weapon_time > 0 && weapon == WP_ROCKET_LAUNCHER {
            sense.moving = false;
        }
        if sense.moving {
            sense.moving = self.actors[me].mind.goal.is_some() && self.st_move(me, command);
        }
        if !sense.moving {
            if !self.actors[me].mind.timers.done("duck", level_time) {
                command.up_move = -127;
            }
        } else {
            self.actors[me].mind.timers.set("duck", level_time, -1);
        }
        if !self.actors[me].mind.timers.done("flee", level_time) {
            sense.face_enemy = false;
        }
        if !sense.face_enemy {
            let npc = &mut self.actors[me];
            if !sense.moving {
                npc.mind.tactics.last_path_angles = npc.player.view_angles();
            }
            npc.desired_yaw = npc.mind.tactics.last_path_angles[1];
            npc.mind.desired_pitch = 0.0;
            self.update_angles(me, true, true, command);
            if sense.moving {
                sense.shoot = false;
            }
        }
        let npc = &self.actors[me];
        if npc.script_flags & SCF_DONT_FIRE != 0 {
            sense.shoot = false;
        }
        if let Some(enemy) = npc.mind.enemy.and_then(|enemy| self.body(enemy))
            && let Some(their) = enemy.enemy.and_then(|number| self.body(number))
            && enemy.weapon == WP_SABER
            && their.weapon == WP_SABER
        {
            sense.shoot = false;
        }
        if weapon_time > 0 {
            if weapon == WP_ROCKET_LAUNCHER {
                if !sense.enemy_los || !sense.enemy_cs {
                    self.actors[me].player.set_raw_field(PS_WEAPON_TIME, 0);
                } else {
                    let delay = self.host.irand(3_000, 5_000);
                    self.actors[me]
                        .mind
                        .timers
                        .set("attackDelay", level_time, delay);
                }
            }
        } else if sense.shoot && self.actors[me].mind.timers.done("attackDelay", level_time) {
            if self.actors[me].script_flags & SCF_FIRE_WEAPON == 0 {
                self.weapon_think(me, command);
            }
            if weapon == WP_ROCKET_LAUNCHER
                && command.buttons & BUTTON_ATTACK != 0
                && !sense.moving
                && self.host.skill() > 1
                && self.host.irand(0, 3) == 0
            {
                command.buttons &= !BUTTON_ATTACK;
                command.buttons |= BUTTON_ALT_ATTACK;
                let time = self.host.irand(1_000, 2_500);
                self.actors[me]
                    .player
                    .set_raw_field(PS_WEAPON_TIME, time as u32);
            }
        }
    }

    /// `ST_Move` (`NPC_AI_Stormtrooper.c:360-411`): a combat move toward the goal; running
    /// into the enemy holds the position, and a failed move hands the goal to a squadmate
    /// in the way (or gives it up); the first good move says what it is for.
    fn st_move(&mut self, me: usize, command: &mut UserCommand) -> bool {
        self.actors[me].mind.combat_move = true;
        let moved = self.move_to_goal(me, true, command);
        let info = self.level.nav;
        if info.flags & NIF_COLLISION != 0
            && info.blocker.is_some()
            && info.blocker == self.actors[me].mind.enemy
        {
            self.hold_position(me);
        }
        if moved {
            self.say_movement_speech(me);
            return true;
        }
        let group = self.actors[me].mind.tactics.group;
        let blocker_group = info
            .blocker
            .and_then(|blocker| self.actor_at(blocker))
            .map(|at| self.actors[at].mind.tactics.group);
        if let Some(group) = group
            && blocker_group == Some(Some(group))
        {
            let blocking = self.actors[me].mind.tactics.blocking_ent_num;
            let member = self.level.groups[group]
                .members()
                .iter()
                .find(|member| i32::from(member.number) == blocking)
                .map(|member| member.number);
            if let Some(other) = member.and_then(|number| self.actor_at(number)) {
                self.transfer_move_goal(me, other);
            }
        }
        self.hold_position(me);
        false
    }

    /// `ST_HoldPosition` (`NPC_AI_Stormtrooper.c:302-326`): its combat point given up as
    /// failed, and it stands and shoots.
    fn hold_position(&mut self, me: usize) {
        let level_time = self.level_time;
        if self.actors[me].mind.tactics.squad_state == squad::RETREAT {
            self.actors[me]
                .mind
                .timers
                .set("flee", level_time, -level_time);
        }
        let verify = self.host.irand(1_000, 3_000);
        self.actors[me]
            .mind
            .timers
            .set("verifyCP", level_time, verify);
        let point = self.actors[me].mind.tactics.combat_point;
        self.free_combat_point(me, point, true);
        let group = self.actors[me].mind.tactics.group;
        self.update_squad_state(group, me, squad::STAND_AND_SHOOT);
        self.actors[me].mind.goal = None;
    }

    /// `NPC_ST_SayMovementSpeech` (`NPC_AI_Stormtrooper.c:328-350`): what it stored to say
    /// on moving, said now — now and then by an imperial commander instead.
    fn say_movement_speech(&mut self, me: usize) {
        let tactics = self.actors[me].mind.tactics;
        if tactics.movement_speech == 0 {
            return;
        }
        let commander = self
            .imperial_commander(me)
            .filter(|_| self.host.irand(0, 3) == 0);
        self.st_speech(
            commander.unwrap_or(me),
            tactics.movement_speech,
            tactics.movement_speech_chance,
        );
        let tactics = &mut self.actors[me].mind.tactics;
        tactics.movement_speech = 0;
        tactics.movement_speech_chance = 0.0;
    }

    /// `NPC_BSPatrol` (`NPC_AI_Default.c:672-712`): the player's allies without an enemy —
    /// one looked for every `vigilance` seconds, a goal walked to.
    pub fn bs_patrol(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        if level_time > self.actors[me].mind.tactics.enemy_check_debounce_time {
            let vigilance = self.actors[me].definition.stats.vigilance;
            self.actors[me].mind.tactics.enemy_check_debounce_time =
                (level_time as f32 + vigilance * 1_000.0) as i32;
            self.check_enemy(me, true, false, true);
            if self.actors[me].mind.enemy.is_some() {
                self.actors[me].behavior_state = 15;
                return;
            }
        }
        self.actors[me].mind.tactics.investigate_sound_debounce_time = 0;
        if self.update_goal(me, command).is_some() {
            self.move_to_goal(me, true, command);
        }
        self.update_angles(me, true, true, command);
        command.buttons |= 16;
    }
}
