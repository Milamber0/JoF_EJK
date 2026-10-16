//! The wampa's AI (`codemp/game/NPC_AI_Wampa.c`): its behaviour (`NPC_BSWampa_Default`,
//! `526-675`), its patrol, idle and roar (`88-141`), its run and walk (`Wampa_Move`,
//! `148-191`), its fight (`Wampa_Combat`, `365-446`) and its pain (`NPC_Wampa_Pain`,
//! `453-519`). Its attacks and slashes are [`crate::npc_wampa_attack`]'s.
//!
//! The multiplayer wampa grabs nobody: it slashes, backhands and leaps. It advances on its
//! enemy only while it is hurt (`takingPain`): out of reach and out of pain it stands and
//! roars, as the reference's does.
//!
//! Also here, for the smaller creatures' AI ([`crate::npc_howler`],
//! [`crate::npc_mine_monster`]): `NPC_CheckEnemyExt` with the alerts (`NPC_FindEnemy`,
//! `NPC_utils.c:1422-1482`) and `ValidEnemy` (`NPC_combat.c:1360-1424`).

use crate::npc_senses::{AEL_DISCOVERED, distance_squared, in_fov3};
use crate::npc_spawn::{FL_NOTARGET, NpcHost};
use crate::npc_world::NpcWorld;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
use sjk_protocol::UserCommand;

/// `MIN_DISTANCE`, `MAX_DISTANCE` (`NPC_AI_Wampa.c:27-31`).
pub(crate) const MIN_DISTANCE: f32 = 48.0;
const MAX_DISTANCE: i32 = 1_024;
/// `LSTATE_CLEAR`, `LSTATE_WAITING`.
pub(crate) const LSTATE_CLEAR: i32 = 0;
pub(crate) const LSTATE_WAITING: i32 = 1;
/// `BUTTON_WALKING`.
pub(crate) const BUTTON_WALKING: u16 = 16;
/// `EF2_USE_ALT_ANIM`; `ps.eFlags2`.
use crate::npc_creature::{EF2_USE_ALT_ANIM, PS_EFLAGS2};
/// Animations: `BOTH_ATTACK1..3`, `BOTH_GESTURE1..2`, `BOTH_PAIN1..2`.
pub(crate) const BOTH_ATTACK1: u16 = 113;
pub(crate) const BOTH_ATTACK2: u16 = 114;
pub(crate) const BOTH_ATTACK3: u16 = 115;
const BOTH_GESTURE1: u16 = 963;
const BOTH_GESTURE2: u16 = 964;
pub(crate) const BOTH_PAIN1: u16 = 95;
const BOTH_PAIN2: u16 = 96;
/// `CLASS_WAMPA`.
pub(crate) const CLASS_WAMPA: i32 = 55;
/// `SCF_LOOK_FOR_ENEMIES`.
pub(crate) const SCF_LOOK_FOR_ENEMIES: u32 = 0x800;
/// `s.time` (the entity's wire field) and `s.angles`.
const ES_TIME: usize = 65;
const ES_ANGLES: [usize; 3] = [25, 9, 24];

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSWampa_Default` (`NPC_AI_Wampa.c:526-675`).
    pub fn bs_wampa_default(&mut self, me: usize, command: &mut UserCommand) {
        let flags2 = self.actors[me].player.raw_field(PS_EFLAGS2).unwrap_or(0);
        self.actors[me]
            .player
            .set_raw_field(PS_EFLAGS2, flags2 & !EF2_USE_ALT_ANIM);
        let level_time = self.level_time;
        if !self.actors[me].mind.timers.done("rageTime", level_time) {
            // "do nothing but roar first time we see an enemy"
            self.face_enemy(me, true, command);
            return;
        }
        if let Some(enemy) = self.actors[me].mind.enemy {
            if !self.actors[me].mind.timers.done("attacking", level_time) {
                self.face_enemy(me, true, command);
                let distance = self.wampa_enemy_distance_to(me, enemy);
                self.level.wampa_enemy_distance = distance;
                self.wampa_attack(me, distance, false, command);
                return;
            }
            self.wampa_fight_upkeep(me, enemy, command);
            return;
        }
        if self.actors[me].mind.timers.done("idlenoise", level_time) {
            let number = self.actors[me].number;
            self.creature_sound(
                number,
                crate::npc_creature::CHAN_AUTO,
                b"sound/chars/wampa/misc/anger3.wav",
            );
            let time = self.host.irand(2_000, 4_000);
            self.actors[me]
                .mind
                .timers
                .set("idlenoise", level_time, time);
        }
        let spawnflags = self.actors[me].spawnflags;
        if spawnflags & 2 != 0 || spawnflags & 1 != 0 {
            // A searcher or a wanderer without an enemy (`629-660`, [`crate::npc_states_search`]).
            self.wampa_roams(me, spawnflags & 2 != 0, command);
        } else if self.actors[me].script_flags & SCF_LOOK_FOR_ENEMIES != 0 {
            self.wampa_patrol(me, command);
        } else {
            self.wampa_idle(me, command);
        }
        self.update_angles(me, true, true, command);
    }

    /// `NPC_BSWampa_Default`'s search (spawn flag 2) or wander (spawn flag 1) without an
    /// enemy (`629-660`): begun from its nearest waypoint the first time, walked; a
    /// wanderer that looks for enemies roars at one it finds, else idles.
    fn wampa_roams(&mut self, me: usize, searches: bool, command: &mut UserCommand) {
        use crate::npc_behavior::bstate;
        if self.actors[me].mind.tactics.home_waypoint == crate::npc_mind::WAYPOINT_NONE {
            self.search_start(
                me,
                crate::npc_mind::WAYPOINT_NONE,
                if searches {
                    bstate::SEARCH
                } else {
                    bstate::WANDER
                },
            );
            self.actors[me].mind.temp_behavior = bstate::DEFAULT;
        }
        command.buttons |= BUTTON_WALKING;
        let state = if searches {
            "NPC_BSSearch"
        } else {
            "NPC_BSWander"
        };
        if !self.run_state(me, state, command) {
            // The replay's driver stood the states in.
            self.host.stub(self.actors[me].number, state);
        }
        if searches || self.actors[me].script_flags & SCF_LOOK_FOR_ENEMIES == 0 {
            return;
        }
        if !self.creature_find_enemy(me, true) {
            self.wampa_idle(me, command);
        } else {
            self.wampa_check_roar(me);
            let time = self.host.irand(5_000, 15_000);
            self.actors[me]
                .mind
                .timers
                .set("lookForNewEnemy", self.level_time, time);
        }
    }

    /// `NPC_BSWampa_Default` with an enemy and no attack under way (`560-618`): its anger,
    /// its enemy dropped when long dead, a better one looked for, then the fight.
    fn wampa_fight_upkeep(&mut self, me: usize, enemy: u16, command: &mut UserCommand) {
        let level_time = self.level_time;
        let number = self.actors[me].number;
        if self.actors[me].mind.timers.done("angrynoise", level_time) {
            let which = self.host.irand(1, 2);
            self.creature_sound(
                number,
                crate::npc_creature::CHAN_VOICE,
                format!("sound/chars/wampa/misc/anger{which}.wav").as_bytes(),
            );
            let time = self.host.irand(5_000, 10_000);
            self.actors[me]
                .mind
                .timers
                .set("angrynoise", level_time, time);
        }
        let enemy_body = self.body(enemy);
        if enemy_body.is_some_and(|body| body.class == CLASS_WAMPA) {
            // "got mad at another Wampa, look for a valid enemy"
            if self.actors[me].mind.timers.done("wampaInfight", level_time) {
                self.creature_find_enemy(me, true);
            }
        } else {
            if !self.valid_enemy(me, enemy) {
                self.actors[me].mind.timers.remove("lookForNewEnemy");
                let in_use = enemy_body.is_some();
                if !in_use
                    || level_time - self.wampa_entity_time(enemy) > self.host.irand(10_000, 15_000)
                {
                    // "get bored with him"
                    self.actors[me].mind.enemy = None;
                    self.wampa_patrol(me, command);
                    self.update_angles(me, true, true, command);
                    // "just lost my enemy": a search or a wander from its waypoint (`585-594`).
                    let spawnflags = self.actors[me].spawnflags;
                    if spawnflags & 2 != 0 || spawnflags & 1 != 0 {
                        let (waypoint, state) = (
                            self.actors[me].mind.tactics.waypoint,
                            if spawnflags & 2 != 0 {
                                crate::npc_behavior::bstate::SEARCH
                            } else {
                                crate::npc_behavior::bstate::WANDER
                            },
                        );
                        self.search_start(me, waypoint, state);
                        self.actors[me].mind.temp_behavior = crate::npc_behavior::bstate::DEFAULT;
                    }
                    return;
                }
            }
            if self.actors[me]
                .mind
                .timers
                .done("lookForNewEnemy", level_time)
            {
                self.creature_look_for_new_enemy(me);
            }
        }
        self.wampa_combat(me, command);
    }

    /// The look for a better enemy of the wampa and the rancor (`NPC_AI_Wampa.c:598-615`,
    /// `NPC_AI_Rancor.c:928-945`): as if it had none, and a different one taken and held 5-15 s; otherwise another look in 2-5 s.
    pub(crate) fn creature_look_for_new_enemy(&mut self, me: usize) {
        let level_time = self.level_time;
        let saved = self.actors[me].mind.enemy.take();
        let find = self.actors[me].mind.confusion_time < level_time;
        let found = self.check_enemy(me, find, false, false);
        self.actors[me].mind.enemy = saved;
        match found {
            Some(new_enemy) if Some(new_enemy) != saved => {
                self.actors[me].mind.last_enemy = saved;
                self.set_enemy(me, new_enemy);
                let time = self.host.irand(5_000, 15_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("lookForNewEnemy", level_time, time);
            }
            _ => {
                let time = self.host.irand(2_000, 5_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("lookForNewEnemy", level_time, time);
            }
        }
    }

    /// `Wampa_Idle` (`88-98`).
    fn wampa_idle(&mut self, me: usize, command: &mut UserCommand) {
        self.actors[me].mind.fight.local_state = LSTATE_CLEAR;
        if self.update_goal(me, command).is_some() {
            command.buttons &= !BUTTON_WALKING;
            self.move_to_goal(me, true, command);
        }
    }

    /// `Wampa_CheckRoar` (`100-110`): once its roar's wait is up, a gesture it holds for its
    /// whole length (`rageTime`), and another roar in 5-20 s.
    pub(crate) fn wampa_check_roar(&mut self, me: usize) -> bool {
        let level_time = self.level_time;
        if self.actors[me].wait >= level_time as f32 {
            return false;
        }
        let wait = self.host.irand(5_000, 20_000);
        self.actors[me].wait = (level_time + wait) as f32;
        let gesture = self
            .host
            .irand(i32::from(BOTH_GESTURE1), i32::from(BOTH_GESTURE2)) as u16;
        self.set_animation(
            me,
            SETANIM_BOTH,
            gesture,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        );
        let legs_timer = self.actors[me].player.legs_timer();
        self.actors[me]
            .mind
            .timers
            .set("rageTime", level_time, legs_timer);
        true
    }

    /// `Wampa_Patrol` (`116-141`).
    fn wampa_patrol(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        self.actors[me].mind.fight.local_state = LSTATE_CLEAR;
        if self.update_goal(me, command).is_some() {
            command.buttons |= BUTTON_WALKING;
            self.move_to_goal(me, true, command);
        } else if self.actors[me].mind.timers.done("patrolTime", level_time) {
            let time = (self.host.rng().flrand(-1.0, 1.0) * 5_000.0 + 5_000.0) as i32;
            self.actors[me]
                .mind
                .timers
                .set("patrolTime", level_time, time);
        }
        if !self.creature_find_enemy(me, true) {
            self.wampa_idle(me, command);
            return;
        }
        self.wampa_check_roar(me);
        let time = self.host.irand(5_000, 15_000);
        self.actors[me]
            .mind
            .timers
            .set("lookForNewEnemy", level_time, time);
    }

    /// `Wampa_Move` (`148-191`): at its enemy, running upright, on all fours from afar, or
    /// walking close, each held a while.
    pub(crate) fn wampa_move(&mut self, me: usize, visible: bool, command: &mut UserCommand) {
        if self.actors[me].mind.fight.local_state == LSTATE_WAITING {
            return;
        }
        let level_time = self.level_time;
        let enemy = self.actors[me].mind.enemy;
        self.actors[me].mind.goal = enemy;
        if enemy.is_some() {
            let distance = self.level.wampa_enemy_distance;
            command.buttons &= !BUTTON_WALKING;
            let timers = &self.actors[me].mind.timers;
            let running =
                !timers.done("runfar", level_time) || !timers.done("runclose", level_time);
            let walking = !timers.done("walk", level_time);
            let run_speed = self.actors[me].definition.stats.run_speed;
            if running {
            } else if walking {
                command.buttons |= BUTTON_WALKING;
            } else if visible && distance > 384.0 && run_speed == 180 {
                self.actors[me].definition.stats.run_speed = 300;
                let time = self.host.irand(2_000, 4_000);
                self.actors[me].mind.timers.set("runfar", level_time, time);
            } else if distance > 256.0 && run_speed == 300 {
                self.actors[me].definition.stats.run_speed = 180;
                let time = self.host.irand(3_000, 5_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("runclose", level_time, time);
            } else if distance < 128.0 {
                self.actors[me].definition.stats.run_speed = 180;
                command.buttons |= BUTTON_WALKING;
                let time = self.host.irand(4_000, 6_000);
                self.actors[me].mind.timers.set("walk", level_time, time);
            }
        }
        if self.actors[me].definition.stats.run_speed == 300 {
            // "need to use the alternate run - hunched over on all fours"
            let flags2 = self.actors[me].player.raw_field(PS_EFLAGS2).unwrap_or(0);
            self.actors[me]
                .player
                .set_raw_field(PS_EFLAGS2, flags2 | EF2_USE_ALT_ANIM);
        }
        self.move_to_goal(me, true, command);
        self.actors[me].mind.tactics.goal_radius = MAX_DISTANCE;
    }

    /// `Wampa_Combat` (`365-446`).
    fn wampa_combat(&mut self, me: usize, command: &mut UserCommand) {
        let Some(enemy) = self.actors[me].mind.enemy else {
            return;
        };
        let enemy_origin = self.body(enemy).map_or([0.0; 3], |body| body.origin);
        let origin = self.actors[me].current_origin;
        if !crate::npc_senses::clear_los(&mut self.senses(), origin, enemy_origin) {
            if self.host.irand(0, 10) == 0 && self.wampa_check_roar(me) {
                return;
            }
            self.wampa_go_after(me, enemy, false, command);
            return;
        }
        if self.update_goal(me, command).is_some() {
            self.wampa_go_after(me, enemy, true, command);
            return;
        }
        let distance = self.wampa_enemy_distance_to(me, enemy);
        self.level.wampa_enemy_distance = distance;
        let mut advance = distance > self.actors[me].maxs[0] + MIN_DISTANCE;
        let mut charge = false;
        self.face_enemy(me, true, command);
        if advance {
            let yaw_only = [0.0, self.actors[me].mind.current_angles[1], 0.0];
            let enemy_health = self.body(enemy).map_or(0, |body| body.health);
            let origin = self.actors[me].current_origin;
            if enemy_health > 0
                && (distance - 350.0).abs() <= 80.0
                && in_fov3(enemy_origin, origin, yaw_only, 20, 20)
                && self.host.irand(0, 9) == 0
            {
                // "go for the charge"
                charge = true;
                advance = false;
            }
        }
        let level_time = self.level_time;
        let waiting = self.actors[me].mind.fight.local_state == LSTATE_WAITING;
        if (advance || waiting) && self.actors[me].mind.timers.done("attacking", level_time) {
            if self.actors[me]
                .mind
                .timers
                .done2("takingPain", level_time, true)
            {
                self.actors[me].mind.fight.local_state = LSTATE_CLEAR;
            } else {
                self.wampa_move(me, true, command);
            }
            return;
        }
        if self.host.irand(0, 20) == 0 && self.wampa_check_roar(me) {
            return;
        }
        if self.host.irand(0, 1) == 0 {
            self.wampa_attack(me, distance, charge, command);
        }
    }

    /// `Wampa_Combat`'s way to an enemy it cannot see or has a goal before (`368-392`).
    fn wampa_go_after(&mut self, me: usize, enemy: u16, visible: bool, command: &mut UserCommand) {
        let mind = &mut self.actors[me].mind;
        mind.combat_move = true;
        mind.goal = Some(enemy);
        mind.tactics.goal_radius = MAX_DISTANCE;
        self.wampa_move(me, visible, command);
    }

    /// `Distance(NPC->r.currentOrigin, NPC->enemy->r.currentOrigin)`.
    pub(crate) fn wampa_enemy_distance_to(&self, me: usize, enemy: u16) -> f32 {
        let enemy_origin = self.body(enemy).map_or([0.0; 3], |body| body.origin);
        f64::from(distance_squared(
            self.actors[me].current_origin,
            enemy_origin,
        ))
        .sqrt() as f32
    }

    /// `ent->s.time` of entity `number`: an NPC's; a player's is not kept here (0).
    fn wampa_entity_time(&self, number: u16) -> i32 {
        self.actor_at(number).map_or(0, |at| {
            self.actors[at].state.raw_field(ES_TIME).unwrap_or(0) as i32
        })
    }

    /// `NPC_Wampa_Pain` (`NPC_AI_Wampa.c:453-519`).
    pub(crate) fn wampa_pain(&mut self, me: usize, attacker: Option<u16>, damage: i32) {
        let level_time = self.level_time;
        let attacker_body = attacker.and_then(|number| self.body(number));
        let by_wampa = attacker_body.is_some_and(|body| body.class == CLASS_WAMPA);
        let enemy = self.actors[me].mind.enemy;
        if let Some(body) = attacker_body
            && Some(body.number) != enemy
            && body.flags & FL_NOTARGET == 0
        {
            let enemy_body = enemy.and_then(|number| self.body(number));
            let origin = self.actors[me].current_origin;
            // `||` stops at the first true, each draw only when reached.
            let turn = (body.number == 0 && self.host.irand(0, 3) == 0)
                || enemy.is_none()
                || enemy_body.is_none_or(|enemy| enemy.health == 0)
                || enemy_body.is_some_and(|enemy| enemy.class == CLASS_WAMPA)
                || (self.host.irand(0, 4) == 0
                    && distance_squared(body.origin, origin)
                        < distance_squared(
                            enemy_body.map_or([0.0; 3], |enemy| enemy.origin),
                            origin,
                        ));
            if turn {
                self.set_enemy(me, body.number);
                let time = self.host.irand(5_000, 15_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("lookForNewEnemy", level_time, time);
                if by_wampa {
                    let time = self.host.irand(2_000, 5_000);
                    self.actors[me]
                        .mind
                        .timers
                        .set("wampaInfight", level_time, time);
                }
            }
        }
        let legs = self.actors[me].player.leg_animation();
        let hurt = by_wampa || self.host.irand(0, 100) < damage;
        if !(hurt
            && legs != BOTH_GESTURE1
            && legs != BOTH_GESTURE2
            && self.actors[me].mind.timers.done("takingPain", level_time))
        {
            return;
        }
        if self.wampa_check_roar(me) {
            return;
        }
        let legs = self.actors[me].player.leg_animation();
        if matches!(legs, BOTH_ATTACK1 | BOTH_ATTACK2 | BOTH_ATTACK3)
            || !(self.actors[me].health > 100 || by_wampa)
        {
            return;
        }
        let animation = if self.host.irand(0, 1) == 0 {
            BOTH_PAIN2
        } else {
            BOTH_PAIN1
        };
        self.creature_flinch(me, animation, 500);
        let mind = &mut self.actors[me].mind;
        for name in ["runfar", "runclose", "walk"] {
            mind.timers.set(name, level_time, -1);
        }
    }

    /// A monster's flinch (`NPC_Wampa_Pain`'s, `NPC_Rancor_Pain`'s): its attack dropped, its
    /// angles back to its path's, `animation` played whole, `takingPain` for its length and
    /// up to `extra` more, and waiting.
    pub(crate) fn creature_flinch(&mut self, me: usize, animation: u16, extra: i32) {
        let level_time = self.level_time;
        self.actors[me].mind.timers.remove("attacking");
        let angles = self.actors[me].mind.tactics.last_path_angles;
        for (index, value) in ES_ANGLES.into_iter().zip(angles) {
            self.actors[me].state.set_raw_field(index, value.to_bits());
        }
        self.set_animation(
            me,
            SETANIM_BOTH,
            animation,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        );
        let legs_timer = self.actors[me].player.legs_timer();
        let time = legs_timer + self.host.irand(0, extra);
        self.actors[me]
            .mind
            .timers
            .set("takingPain", level_time, time);
        self.actors[me].mind.fight.local_state = LSTATE_WAITING;
    }

    /// `NPC_CheckEnemyExt(checkAlerts)` (`NPC_utils.c:1484-1498`, `NPC_FindEnemy`,
    /// `1422-1482`): the enemy kept while it is still one; else the nearest one it sees, or
    /// — with `alerts` — one an alert gives it (the player who made it, or a teammate's
    /// enemy) taken. Whether it has one.
    pub(crate) fn creature_find_enemy(&mut self, me: usize, alerts: bool) -> bool {
        if !alerts {
            return self.check_enemy_ext(me);
        }
        if self.actors[me].mind.confusion_time > self.level_time {
            return false;
        }
        if let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
            && self.valid_for(me, &enemy)
        {
            return true;
        }
        // `NPC_PickEnemyExt`: the nearest first, else the alerts.
        if self.check_enemy_ext(me) {
            return true;
        }
        let Some(at) = self.check_alerts(me, -1, true, AEL_DISCOVERED) else {
            return false;
        };
        let alert = self.alerts.events()[at];
        let number = self.actors[me].number;
        if alert.owner == Some(number) || alert.level < AEL_DISCOVERED {
            return false;
        }
        let owner = alert.owner.and_then(|owner| self.body(owner));
        let candidate = match owner {
            Some(owner) if owner.number == 0 => Some(owner.number),
            Some(owner) if owner.player_team == self.actors[me].player_team => owner.enemy,
            _ => None,
        };
        let Some(candidate) = candidate.and_then(|candidate| self.body(candidate)) else {
            return false;
        };
        if !self.valid_for(me, &candidate) {
            return false;
        }
        self.set_enemy(me, candidate.number);
        true
    }
}
