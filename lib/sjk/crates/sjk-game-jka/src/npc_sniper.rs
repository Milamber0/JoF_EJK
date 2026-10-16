//! The sniper's AI (`codemp/game/NPC_AI_Sniper.c`), which `NPC_BehaviorSet_Sniper`
//! (`NPC.c:1084-1102`) runs for an enemy with the disruptor on its alternate fire (the retail
//! `rodian`), and Boba Fett while he snipes (`NPC_AI_Jedi.c:6306-6314`): its patrol and the
//! alerts it looks into (`NPC_BSSniper_Patrol`, `207-297`), its fight (`NPC_BSSniper_Attack`,
//! `663-875`) — primary fire up close, the alternate far off, the shots it means to miss at
//! first (`Sniper_FaceEnemy`, `531-626`) and later the lagged aim (`Sniper_UpdateEnemyPos`,
//! `628-646`), where it goes (`Sniper_CheckMoveState`, `Sniper_Move`), a combat point when it
//! cannot see its enemy (`Sniper_ResolveBlockedShot`), and ducking after a shot
//! (`Sniper_StartHide`).
//!
//! `NPC_ChangeWeapon` does nothing in multiplayer (`NPC_combat.c:860-889`): only the fire
//! mode changes. `NPC_Sniper_Pain`, `NPC_Sniper_PlayConfusionSound` and `Sniper_ClearTimers`
//! are never given to an NPC in multiplayer, and are not ported.
//!
//! Held to `tools/game-oracle/npcsniper.c` (`game-npcsoldier-sniper.txt`).

use crate::npc_combat_points::{PointSearch, cp};
use crate::npc_nav::NIF_COLLISION;
use crate::npc_senses::{
    AEL_DANGER, AEL_DISCOVERED, AEL_SUSPICIOUS, Spot, distance_squared, spot, subtract,
};
use crate::npc_spawn::NpcHost;
use crate::npc_st::{
    SCF_CHASE_ENEMIES, SCF_DONT_FIRE, SCF_IGNORE_ALERTS, SCF_LOOK_FOR_ENEMIES, SCF_USE_CP_NEAREST,
    squad,
};
use crate::npc_world::NpcWorld;
use crate::player_angle_math::vector_angles;
use sjk_protocol::UserCommand;

/// `ENEMY_POS_LAG_STEPS` (`b_public.h:132-134`): how many lagged places are kept.
pub const ENEMY_POS_LAG_STEPS: usize = 24;
/// `SPF_NO_HIDE`: a spawnflag that keeps a sniper up after its shot.
const SPF_NO_HIDE: i32 = 2;
/// `SCF_ALT_FIRE`.
pub(crate) const SCF_ALT_FIRE: u32 = 0x40;
/// `WP_DISRUPTOR`, `WP_EMPLACED_GUN`.
const WP_DISRUPTOR: u8 = 6;
const WP_EMPLACED_GUN: i32 = 17;
/// `BUTTON_ATTACK`, `BUTTON_WALKING`, `BUTTON_ALT_ATTACK`.
const BUTTON_ATTACK: u16 = 1;
const BUTTON_WALKING: u16 = 16;
const BUTTON_ALT_ATTACK: u16 = 128;
/// `CHAN_WEAPON`.
pub(crate) const CHAN_WEAPON: u32 = 2;
/// `MASK_PLAYERSOLID`: a player's clip mask.
const MASK_PLAYERSOLID: u32 = 0x1 | 0x10 | 0x100 | 0x1000;

/// `AngleVectors`: forward, right and up ([`crate::pmove::flight::angles_to_axis`]'s
/// precision; its left negated back).
pub(crate) fn angle_vectors(angles: [f32; 3]) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let [forward, left, up] = crate::pmove::flight::angles_to_axis(angles);
    (forward, left.map(|axis| -axis), up)
}

/// What a sniper keeps between thinks: `gNPC_t`'s `enemyLaggedPos`, and the entity's
/// `fly_sound_debounce_time` (which it sets and nothing reads for an NPC).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SniperMind {
    /// Where its enemy's head was, newest first, 100 ms of lag apart.
    pub enemy_lagged_pos: [[f32; 3]; ENEMY_POS_LAG_STEPS],
    pub fly_sound_debounce_time: i32,
}

/// What the fight senses this think (the file's statics `enemyLOS2`, `enemyCS2`,
/// `faceEnemy2`, `move2`, `shoot2`, `enemyDist2`).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SniperSense {
    enemy_los: bool,
    enemy_cs: bool,
    face_enemy: bool,
    moving: bool,
    shoot: bool,
    enemy_dist: f32,
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSSniper_Default` (`877-887`).
    pub fn bs_sniper_default(&mut self, me: usize, command: &mut UserCommand) {
        if self.actors[me].mind.enemy.is_none() {
            self.bs_sniper_patrol(me, command);
        } else {
            self.bs_sniper_attack(me, command);
        }
    }

    /// `NPC_BSSniper_Patrol` (`207-297`): its shot count reset; unconfused, an enemy looked
    /// for, a danger fled, an alert taken (an enemy that gave itself away) or looked at;
    /// else its goal walked to.
    fn bs_sniper_patrol(&mut self, me: usize, command: &mut UserCommand) {
        self.actors[me].count = 0;
        let aim = self.actors[me].definition.stats.aim;
        if self.soldier_notices(me, (6 - aim) * 100, (6 - aim) * 500, command) {
            return;
        }
        if self.update_goal(me, command).is_some() {
            command.buttons |= BUTTON_WALKING;
            self.move_to_goal(me, true, command);
        }
        self.update_angles(me, true, true, command);
    }

    /// The sniper's and the grenadier's patrol before its walk (`NPC_AI_Sniper.c:211-287`,
    /// `NPC_AI_Grenadier.c:211-286`): unconfused, an enemy seen, a danger fled, or an alert
    /// looked into — an enemy that gave itself away taken, its attack delayed between
    /// `least` and `most`. Whether the think is done (the NPC turned).
    pub(crate) fn soldier_notices(
        &mut self,
        me: usize,
        least: i32,
        most: i32,
        command: &mut UserCommand,
    ) -> bool {
        let level_time = self.level_time;
        if self.actors[me].mind.confusion_time >= level_time {
            return false;
        }
        let flags = self.actors[me].script_flags;
        if flags & SCF_LOOK_FOR_ENEMIES != 0 && self.check_player_team_stealth(me) {
            self.update_angles(me, true, true, command);
            return true;
        }
        if flags & SCF_IGNORE_ALERTS != 0 {
            return false;
        }
        let alert = self.check_alerts(me, -1, false, AEL_SUSPICIOUS);
        if self.check_for_danger(me, alert) {
            self.update_angles(me, true, true, command);
            return true;
        }
        if let Some(event) = alert.map(|at| self.alerts.events()[at])
            && event.id != self.actors[me].mind.last_alert_id
        {
            self.actors[me].mind.last_alert_id = event.id;
            if event.level == AEL_DISCOVERED {
                let owner = event.owner.and_then(|owner| self.body(owner));
                if let Some(owner) = owner.filter(|owner| {
                    owner.health >= 0 && owner.player_team == self.actors[me].enemy_team
                }) {
                    self.set_enemy(me, owner.number);
                    let delay = self.host.irand(least, most);
                    self.actors[me]
                        .mind
                        .timers
                        .set("attackDelay", level_time, delay);
                }
            } else {
                let look = self.host.irand(500, 1_000);
                let tactics = &mut self.actors[me].mind.tactics;
                tactics.investigate_goal = event.position;
                tactics.investigate_debounce_time = level_time + look;
                if event.level == AEL_SUSPICIOUS {
                    let longer = self.host.irand(500, 2_500);
                    self.actors[me].mind.tactics.investigate_debounce_time += longer;
                }
            }
        }
        if self.actors[me].mind.tactics.investigate_debounce_time <= level_time {
            return false;
        }
        // A look at the alert from its eyes; its desired angles kept.
        let npc = &mut self.actors[me];
        let angles = vector_angles(subtract(
            npc.mind.tactics.investigate_goal,
            npc.mind.eye_point,
        ));
        let (yaw, pitch) = (npc.desired_yaw, npc.mind.desired_pitch);
        npc.desired_yaw = angles[1];
        npc.mind.desired_pitch = angles[0];
        self.update_angles(me, true, true, command);
        let npc = &mut self.actors[me];
        npc.desired_yaw = yaw;
        npc.mind.desired_pitch = pitch;
        true
    }

    /// `ST_HoldPosition` of the sniper and the grenadier (`Sniper_HoldPosition`,
    /// `128-138`): its combat point given up as failed, and no goal.
    pub(crate) fn soldier_hold_position(&mut self, me: usize) {
        let point = self.actors[me].mind.tactics.combat_point;
        self.free_combat_point(me, point, true);
        self.actors[me].mind.goal = None;
    }

    /// The combat point with a clear shot a chaser looks for (`Sniper_Move`, `174-192`;
    /// `Sniper_ResolveBlockedShot`, `416-437`; `Grenadier_Move`): near itself, else near
    /// its enemy; taken and gone to. Whether one was found.
    pub(crate) fn soldier_clear_point(&mut self, me: usize) -> bool {
        let npc = &self.actors[me];
        let (origin, flags) = (npc.current_origin, npc.script_flags);
        let Some(enemy) = npc.mind.enemy.and_then(|enemy| self.body(enemy)) else {
            return false;
        };
        let mut wanted = cp::CLEAR | cp::HAS_ROUTE;
        if flags & SCF_USE_CP_NEAREST != 0 {
            wanted &= !(cp::FLANK | cp::APPROACH_ENEMY | cp::CLOSEST);
            wanted |= cp::NEAREST;
        }
        let mut point = self.find_combat_point(
            me,
            PointSearch {
                position: origin,
                enemy: origin,
                flags: wanted,
                avoid_distance: 32.0,
                ignore: -1,
            },
        );
        if point == -1 && flags & SCF_USE_CP_NEAREST == 0 {
            let search = PointSearch {
                position: origin,
                enemy: enemy.origin,
                flags: cp::CLEAR | cp::HAS_ROUTE | cp::HORZ_DIST_COLL,
                avoid_distance: 32.0,
                ignore: -1,
            };
            point = self.find_combat_point(me, search);
        }
        if point == -1 {
            return false;
        }
        self.set_combat_point(me, point);
        let spot = self.level.combat_points[point as usize].origin;
        self.set_move_goal(me, spot, 8, true, point, None);
        true
    }

    /// `Sniper_Move` (`146-199`): a combat move toward the goal; running into the enemy
    /// holds; a failed chase looks for a point with a clear shot, else holds.
    fn sniper_move(&mut self, me: usize, command: &mut UserCommand) -> bool {
        self.actors[me].mind.combat_move = true;
        let moved = self.move_to_goal(me, true, command);
        let info = self.level.nav;
        if info.flags & NIF_COLLISION != 0 && info.blocker == self.actors[me].mind.enemy {
            self.soldier_hold_position(me);
        }
        if !moved {
            let npc = &self.actors[me];
            let chasing = npc.script_flags & SCF_CHASE_ENEMIES != 0
                && npc.mind.goal.is_some()
                && npc.mind.goal == npc.mind.enemy;
            if chasing && self.soldier_clear_point(me) {
                return moved;
            }
            self.soldier_hold_position(me);
        }
        moved
    }

    /// `Sniper_CheckMoveState` (`330-404`).
    fn sniper_check_move_state(
        &mut self,
        me: usize,
        sense: &mut SniperSense,
        command: &mut UserCommand,
    ) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        if npc.script_flags & SCF_CHASE_ENEMIES == 0 {
            if npc.mind.goal == npc.mind.enemy {
                sense.moving = false;
                return;
            }
        } else if npc.mind.tactics.squad_state == squad::RETREAT {
            if npc.mind.timers.done("flee", level_time) {
                self.actors[me].mind.tactics.squad_state = squad::IDLE;
            } else {
                sense.face_enemy = false;
            }
        } else if npc.mind.tactics.squad_state == squad::IDLE && npc.mind.goal.is_none() {
            sense.moving = false;
            return;
        }
        let _ = self.soldier_arrival(me, sense.enemy_los, sense.enemy_dist, command, |world| {
            let aim = world.actors[me].definition.stats.aim;
            world.host.irand((6 - aim) * 50, (6 - aim) * 100)
        });
    }

    /// `Sniper_CheckMoveState`'s and `Grenadier_CheckMoveState`'s arrival at a goal that is
    /// not the enemy (`364-403`): the timers by why it ran, the goal reached, the attack
    /// delayed (`delay`), the roaming held, the flight ended; still on the way, the roaming
    /// held longer. Whether it arrived.
    pub(crate) fn soldier_arrival(
        &mut self,
        me: usize,
        enemy_los: bool,
        enemy_dist: f32,
        command: &mut UserCommand,
        delay: impl FnOnce(&mut Self) -> i32,
    ) -> bool {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        if npc.mind.goal == npc.mind.enemy || npc.mind.goal.is_none() {
            return false;
        }
        let goal = self.goal_origin(me).unwrap_or([0.0; 3]);
        let npc = &self.actors[me];
        let state = npc.mind.tactics.squad_state;
        let arrived = crate::npc_nav::hit_nav_goal(
            npc.current_origin,
            npc.mins,
            npc.maxs,
            goal,
            16,
            self.flying(me),
        );
        if !(arrived || (state == squad::SCOUT && enemy_los && enemy_dist <= 10_000.0)) {
            let roam = self.host.irand(4_000, 8_000);
            self.actors[me]
                .mind
                .timers
                .set("roamTime", level_time, roam);
            return false;
        }
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
        self.reached_goal(me, command);
        let attack = delay(self);
        self.actors[me]
            .mind
            .timers
            .set("attackDelay", level_time, attack);
        let roam = self.host.irand(1_000, 4_000);
        self.actors[me]
            .mind
            .timers
            .set("roamTime", level_time, roam);
        if self.actors[me].mind.tactics.squad_state == squad::RETREAT {
            self.actors[me]
                .mind
                .timers
                .set("flee", level_time, -level_time);
            self.actors[me].mind.tactics.squad_state = squad::IDLE;
        }
        true
    }

    /// `Sniper_ResolveBlockedShot` (`406-457`): not ducking nor roaming, a chaser after its
    /// enemy looks for a combat point with a clear shot.
    fn sniper_resolve_blocked_shot(&mut self, me: usize) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        if !npc.mind.timers.done("duck", level_time)
            || !npc.mind.timers.done("roamTime", level_time)
        {
            return;
        }
        if npc.script_flags & SCF_CHASE_ENEMIES == 0
            || !(npc.mind.goal.is_none() || npc.mind.goal == npc.mind.enemy)
        {
            return;
        }
        if self.soldier_clear_point(me) {
            self.actors[me].mind.timers.set("duck", level_time, -1);
            let delay = self.host.irand(1_000, 3_000);
            self.actors[me]
                .mind
                .timers
                .set("attackDelay", level_time, delay);
        }
    }

    /// `Sniper_CheckFireState` (`465-509`): standing still with no clear shot, it now and
    /// then fires on where its enemy was lately seen; long unseen, its misses start over.
    fn sniper_check_fire_state(&mut self, me: usize, sense: &mut SniperSense) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        if sense.enemy_cs
            || matches!(
                npc.mind.tactics.squad_state,
                squad::RETREAT | squad::TRANSITION | squad::SCOUT
            )
            || npc.player.velocity() != [0.0; 3]
        {
            return;
        }
        let tactics = npc.mind.tactics;
        let aim = npc.definition.stats.aim;
        if self.host.irand(0, 1) == 0
            && tactics.enemy_last_seen_time != 0
            && level_time - tactics.enemy_last_seen_time < (5 - aim) * 1_000
        {
            if tactics.enemy_last_seen_location != [0.0; 3] {
                let muzzle = self.weapon_spot(me);
                let mut direction = subtract(tactics.enemy_last_seen_location, muzzle);
                crate::player_angle_math::normalize(&mut direction);
                let angles = vector_angles(direction);
                let npc = &mut self.actors[me];
                npc.desired_yaw = angles[1];
                npc.mind.desired_pitch = angles[0];
                sense.shoot = true;
            }
        } else if level_time - tactics.enemy_last_seen_time > 10_000 {
            self.actors[me].count = 0;
        }
    }

    /// `Sniper_EvaluateShot` (`511-529`): a shot that would strike entity `hit` will do — its
    /// enemy, one of its enemy's side, glass, or something breakable enough.
    pub(crate) fn sniper_evaluate_shot(&self, me: usize, hit: u16) -> bool {
        let Some(enemy) = self.actors[me].mind.enemy else {
            return false;
        };
        if hit == enemy {
            return true;
        }
        let struck = self.body(hit);
        if struck.is_some_and(|body| body.player_team == self.actors[me].enemy_team) {
            return true;
        }
        let glass = self.host.glass(hit);
        let (takes_damage, health) = match struck {
            Some(body) => (
                self.actor_at(hit)
                    .is_none_or(|at| self.actors[at].takes_damage),
                body.health,
            ),
            None => {
                let health = self.host.damageable_health(hit);
                (health.is_some() || glass, health.unwrap_or(0))
            }
        };
        (takes_damage && (glass || health < 40 || self.npc(me).weapon == WP_EMPLACED_GUN)) || glass
    }

    /// `CalcMuzzlePoint` along the NPC's view: the muzzle, and the view's right and up.
    fn view_muzzle(&self, me: usize) -> ([f32; 3], [f32; 3], [f32; 3]) {
        let npc = &self.actors[me];
        let (_, right, up) = angle_vectors(npc.player.view_angles());
        (self.weapon_spot(me), right, up)
    }

    /// `Sniper_FaceEnemy` (`531-626`): far off and no crack shot, it misses its first few
    /// shots on purpose — aimed beside and above or below its enemy, where the shot would
    /// strike nothing that counts — then aims at where its enemy was a little while ago
    /// (the worse its aim, the longer ago); close, at a height up its enemy's body.
    fn sniper_face_enemy(&mut self, me: usize, sense: &SniperSense, command: &mut UserCommand) {
        let level_time = self.level_time;
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        else {
            self.update_angles(me, true, true, command);
            return;
        };
        let (muzzle, _, _) = self.view_muzzle(me);
        let mut target = spot(&enemy, Spot::Origin);
        let aim = self.actors[me].definition.stats.aim;
        let angles;
        if sense.enemy_dist > 65_536.0 && aim < 5 {
            if self.actors[me].count < 5 - aim {
                let npc = &self.actors[me];
                if sense.shoot
                    && npc.mind.timers.done("attackDelay", level_time)
                    && level_time >= npc.mind.fight.shot_time
                {
                    self.aim_to_miss(me, muzzle, &mut target, &enemy);
                    self.actors[me].count += 1;
                } else if !sense.enemy_los {
                    self.update_angles(me, true, true, command);
                    return;
                }
            } else {
                let miss =
                    (8 - (aim + self.host.skill()) * 3).clamp(0, ENEMY_POS_LAG_STEPS as i32 - 1);
                target = self.actors[me].mind.sniper.enemy_lagged_pos[miss as usize];
            }
            angles = vector_angles(subtract(target, muzzle));
        } else {
            target[2] += self.host.rng().flrand(0.0, enemy.maxs[2]);
            angles = vector_angles(subtract(target, muzzle));
        }
        let npc = &mut self.actors[me];
        npc.desired_yaw = crate::npc_droid::angle_normalize360(angles[1]);
        npc.mind.desired_pitch = crate::npc_droid::angle_normalize360(angles[0]);
        self.update_angles(me, true, true, command);
    }

    /// `Sniper_FaceEnemy`'s miss (`556-587`): the target moved beside, above or below the
    /// enemy by a random share of its height until a shot there would strike nothing that
    /// counts, ten tries at most.
    fn aim_to_miss(
        &mut self,
        me: usize,
        muzzle: [f32; 3],
        target: &mut [f32; 3],
        enemy: &crate::npc_senses::Body,
    ) {
        let (_, right, up) = angle_vectors(vector_angles(subtract(*target, muzzle)));
        let number = self.actors[me].number;
        let (mut aim_error, mut hit, mut tries) = (false, true, 0);
        let shift = |target: &mut [f32; 3], scale: f32, axis: [f32; 3]| {
            *target = std::array::from_fn(|at| target[at] + scale * axis[at])
        };
        while hit && tries < 10 {
            tries += 1;
            if self.host.irand(0, 1) == 0 {
                aim_error = true;
                let height = if self.host.irand(0, 1) == 0 {
                    enemy.maxs[2]
                } else {
                    enemy.mins[2]
                };
                let scale = height * self.host.rng().flrand(1.5, 4.0);
                shift(target, scale, right);
            }
            if !aim_error || self.host.irand(0, 1) == 0 {
                let height = if self.host.irand(0, 1) == 0 {
                    enemy.maxs[2]
                } else {
                    enemy.mins[2]
                };
                let scale = height * self.host.rng().flrand(1.5, 4.0);
                shift(target, scale, up);
            }
            let trace = self.trace_bodies(
                muzzle,
                [0.0; 3],
                [0.0; 3],
                *target,
                number,
                crate::npc_aim::MASK_SHOT,
            );
            hit = self.sniper_evaluate_shot(me, trace.entity_number);
        }
    }

    /// `Sniper_UpdateEnemyPos` (`628-646`): the lagged places moved back a step, and its
    /// enemy's leaning head, a little lower, taken as the newest.
    fn sniper_update_enemy_pos(&mut self, me: usize, enemy: &crate::npc_senses::Body) {
        let lagged = &mut self.actors[me].mind.sniper.enemy_lagged_pos;
        lagged.copy_within(0..ENEMY_POS_LAG_STEPS - 1, 1);
        let mut head = spot(enemy, Spot::HeadLean);
        head[2] -= self.host.rng().flrand(2.0, 16.0);
        self.actors[me].mind.sniper.enemy_lagged_pos[0] = head;
    }

    /// `Sniper_StartHide` (`654-661`).
    fn sniper_start_hide(&mut self, me: usize) {
        let level_time = self.level_time;
        let duck = self.host.irand(2_000, 5_000);
        let timers = &mut self.actors[me].mind.timers;
        timers.set("duck", level_time, duck);
        timers.set("watch", level_time, 500);
        let after = self.host.irand(500, 2_000);
        self.actors[me]
            .mind
            .timers
            .set("attackDelay", level_time, duck + after);
    }

    /// `NPC_BSSniper_Attack` (`663-875`).
    fn bs_sniper_attack(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        if self.actors[me].mind.fight.pain_debounce_time > level_time {
            self.update_angles(me, true, true, command);
            return;
        }
        if !self.check_enemy_ext(me) {
            self.actors[me].mind.enemy = None;
            self.bs_sniper_patrol(me, command);
            return;
        }
        if self.actors[me].mind.timers.done("flee", level_time) {
            let alert = self.check_alerts(me, -1, false, AEL_DANGER);
            if self.check_for_danger(me, alert) {
                self.update_angles(me, true, true, command);
                return;
            }
        }
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        else {
            self.bs_sniper_patrol(me, command);
            return;
        };
        let mut sense = SniperSense {
            moving: true,
            enemy_dist: distance_squared(self.actors[me].current_origin, enemy.origin),
            ..SniperSense::default()
        };
        if self.sniper_changes_fire(me, &enemy, sense.enemy_dist) {
            self.update_angles(me, true, true, command);
            return;
        }
        self.sniper_update_enemy_pos(me, &enemy);
        self.sniper_sense(me, &enemy, &mut sense);
        self.sniper_check_move_state(me, &mut sense, command);
        self.sniper_check_fire_state(me, &mut sense);
        self.sniper_act(me, sense, command);
    }

    /// `NPC_BSSniper_Attack`'s fire mode (`698-731`): with the disruptor, the primary up
    /// close where its enemy could get straight to it, the alternate far off. Whether it
    /// changed (and waits this think).
    fn sniper_changes_fire(
        &mut self,
        me: usize,
        enemy: &crate::npc_senses::Body,
        enemy_dist: f32,
    ) -> bool {
        let npc = &self.actors[me];
        if npc.player.weapon() != WP_DISRUPTOR {
            return false;
        }
        let alternate = npc.script_flags & SCF_ALT_FIRE != 0;
        if enemy_dist < 16_384.0 {
            if !alternate {
                return false;
            }
            let clip = self
                .actor_at(enemy.number)
                .map_or(MASK_PLAYERSOLID, |at| self.actors[at].clip_mask);
            let (origin, number) = (npc.current_origin, npc.number);
            let trace = self.trace_bodies(
                enemy.origin,
                enemy.mins,
                enemy.maxs,
                origin,
                enemy.number,
                clip,
            );
            if trace.all_solid
                || trace.start_solid
                || !(trace.fraction == 1.0 || trace.entity_number == number)
            {
                return false;
            }
            self.actors[me].script_flags &= !SCF_ALT_FIRE;
            true
        } else if enemy_dist > 65_536.0 && !alternate {
            self.actors[me].script_flags |= SCF_ALT_FIRE;
            true
        } else {
            false
        }
    }

    /// `NPC_BSSniper_Attack`'s sense of its enemy (`733-781`): seen — and in reach, a clear
    /// shot along its view — or, long unseen, a way round found.
    fn sniper_sense(
        &mut self,
        me: usize,
        enemy: &crate::npc_senses::Body,
        sense: &mut SniperSense,
    ) {
        let level_time = self.level_time;
        if self.clear_los4(me, enemy) {
            let tactics = &mut self.actors[me].mind.tactics;
            tactics.enemy_last_seen_time = level_time;
            tactics.enemy_last_seen_location = enemy.origin;
            sense.enemy_los = true;
            if sense.enemy_dist < self.max_distance_squared(me) {
                let npc = &self.actors[me];
                let (forward, _, _) = angle_vectors(npc.player.view_angles());
                let muzzle = self.weapon_spot(me);
                let end: [f32; 3] =
                    std::array::from_fn(|axis| muzzle[axis] + 8_192.0 * forward[axis]);
                let number = npc.number;
                let hit = self
                    .trace_bodies(
                        muzzle,
                        [0.0; 3],
                        [0.0; 3],
                        end,
                        number,
                        crate::npc_aim::MASK_SHOT,
                    )
                    .entity_number;
                sense.enemy_cs = self.sniper_evaluate_shot(me, hit);
            }
        }
        sense.face_enemy = sense.enemy_los;
        if sense.enemy_cs {
            sense.shoot = true;
        } else if level_time - self.actors[me].mind.tactics.enemy_last_seen_time > 3_000 {
            self.sniper_resolve_blocked_shot(me);
        }
    }

    /// What `NPC_BSSniper_Attack` does with what it sensed (`789-874`): its goal gone to,
    /// ducking when still (not while it watches), facing its way or its enemy, and — its
    /// attack delay out — firing, then as often as not hiding.
    fn sniper_act(&mut self, me: usize, mut sense: SniperSense, command: &mut UserCommand) {
        let level_time = self.level_time;
        if sense.moving {
            sense.moving = self.actors[me].mind.goal.is_some() && self.sniper_move(me, command);
        }
        if !sense.moving {
            let timers = &self.actors[me].mind.timers;
            if !timers.done("duck", level_time) && timers.done("watch", level_time) {
                command.up_move = -127;
            }
        } else {
            self.actors[me].mind.timers.set("duck", level_time, -1);
        }
        let npc = &mut self.actors[me];
        let timers = &npc.mind.timers;
        if timers.done("duck", level_time)
            && timers.done("watch", level_time)
            && timers.get("attackDelay").unwrap_or(-1) - level_time > 1_000
            && npc.mind.attack_debounce_time < level_time
            && sense.enemy_los
            && npc.script_flags & SCF_ALT_FIRE != 0
            && npc.mind.sniper.fly_sound_debounce_time < level_time
        {
            npc.mind.sniper.fly_sound_debounce_time = level_time + 2_000;
        }
        if !sense.face_enemy {
            if sense.moving {
                let npc = &mut self.actors[me];
                npc.desired_yaw = npc.mind.tactics.last_path_angles[1];
                npc.mind.desired_pitch = 0.0;
                sense.shoot = false;
            }
            self.update_angles(me, true, true, command);
        } else {
            self.sniper_face_enemy(me, &sense, command);
        }
        if self.actors[me].script_flags & SCF_DONT_FIRE != 0 {
            sense.shoot = false;
        }
        if !sense.shoot || !self.actors[me].mind.timers.done("attackDelay", level_time) {
            return;
        }
        self.weapon_think(me, command);
        if command.buttons & (BUTTON_ATTACK | BUTTON_ALT_ATTACK) != 0 {
            self.sound_on_channel(me, CHAN_WEAPON, b"sound/null.wav");
        }
        if self.actors[me].spawnflags & SPF_NO_HIDE == 0 && self.host.irand(0, 1) == 0 {
            self.sniper_start_hide(me);
        } else {
            let shot = self.actors[me].mind.fight.shot_time;
            self.actors[me]
                .mind
                .timers
                .set("attackDelay", level_time, shot - level_time);
        }
    }

    /// `G_SoundOnEnt(NPC, channel, name)` (`g_utils.c:1409-1418`): an entity sound naming
    /// the NPC, on `channel`.
    pub(crate) fn sound_on_channel(&mut self, me: usize, channel: u32, name: &[u8]) {
        let sound = self.host.sound_index(name);
        let npc = &self.actors[me];
        let mut event = crate::knockdown::entity_sound(npc.current_origin, npc.number, channel);
        event.parameter = u32::from(sound);
        self.host.raise(event);
    }
}
