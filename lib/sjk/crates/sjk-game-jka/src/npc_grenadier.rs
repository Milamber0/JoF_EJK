//! The grenadier's AI (`codemp/game/NPC_AI_Grenadier.c`), which `NPC_BehaviorSet_Grenadier`
//! (`NPC.c:1062-1080`) runs for an enemy with a thermal detonator or a stun baton in hand:
//! its patrol (`NPC_BSGrenadier_Patrol`, `209-296`: the sniper's, [`crate::npc_sniper`]) and
//! its fight (`NPC_BSGrenadier_Attack`, `480-681`) — thrown from where it stands at an enemy in
//! front of it within 1024 units, or clubbed within 64 — where it goes
//! (`Grenadier_CheckMoveState`, `326-410`: after its enemy when it may chase;
//! `Grenadier_Move`, `148-201`), and ducking when still.
//!
//! Close to an enemy without a lit saber a thermal carrier would take up its baton, and far
//! off a baton carrier with a thermal would take that up: `NPC_ChangeWeapon` does nothing in
//! multiplayer (`NPC_combat.c:860-889`), so only the chase that comes with the first is
//! kept. `Grenadier_CheckFireState` (`418-458`) changes nothing (its firing on the last
//! place seen is commented out), nor is anything of `NPC_Grenadier_Pain`,
//! `NPC_Grenadier_PlayConfusionSound` or `Grenadier_ClearTimers` given to an NPC in
//! multiplayer.
//!
//! Held to `tools/game-oracle/npcsniper.c` (`game-npcsoldier-grenadier.txt`,
//! `game-npcsoldier-baton.txt`).

use crate::npc_jedi_patrol::distance_horizontal_squared;
use crate::npc_nav::NIF_COLLISION;
use crate::npc_senses::{AEL_DANGER, distance_squared, in_fov3};
use crate::npc_spawn::NpcHost;
use crate::npc_st::{SCF_CHASE_ENEMIES, SCF_DONT_FIRE, SCF_FIRE_WEAPON, squad};
use crate::npc_world::NpcWorld;
use sjk_protocol::UserCommand;

/// `WP_STUN_BATON`, `WP_SABER`, `WP_THERMAL`.
const WP_STUN_BATON: u8 = 1;
const WP_SABER: u8 = 3;
const WP_THERMAL: u8 = 12;
/// `BUTTON_WALKING`.
const BUTTON_WALKING: u16 = 16;
/// `MASK_PLAYERSOLID`: a player's clip mask.
const MASK_PLAYERSOLID: u32 = 0x1 | 0x10 | 0x100 | 0x1000;

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSGrenadier_Default` (`683-698`).
    pub fn bs_grenadier_default(&mut self, me: usize, command: &mut UserCommand) {
        if self.actors[me].script_flags & SCF_FIRE_WEAPON != 0 {
            self.weapon_think(me, command);
        }
        if self.actors[me].mind.enemy.is_none() {
            self.bs_grenadier_patrol(me, command);
        } else {
            self.bs_grenadier_attack(me, command);
        }
    }

    /// `NPC_BSGrenadier_Patrol` (`209-296`).
    fn bs_grenadier_patrol(&mut self, me: usize, command: &mut UserCommand) {
        if self.soldier_notices(me, 500, 2_500, command) {
            return;
        }
        if self.update_goal(me, command).is_some() {
            command.buttons |= BUTTON_WALKING;
            self.move_to_goal(me, true, command);
        }
        self.update_angles(me, true, true, command);
    }

    /// `Grenadier_Move` (`148-201`): [`crate::npc_sniper`]'s move, but only a thermal
    /// carrier looks for a combat point with a clear shot.
    fn grenadier_move(&mut self, me: usize, command: &mut UserCommand) -> bool {
        self.actors[me].mind.combat_move = true;
        let moved = self.move_to_goal(me, true, command);
        let info = self.level.nav;
        if info.flags & NIF_COLLISION != 0 && info.blocker == self.actors[me].mind.enemy {
            self.soldier_hold_position(me);
        }
        if !moved {
            let npc = &self.actors[me];
            let chasing = npc.script_flags & SCF_CHASE_ENEMIES != 0
                && npc.player.weapon() == WP_THERMAL
                && npc.mind.goal.is_some()
                && npc.mind.goal == npc.mind.enemy;
            if chasing && self.soldier_clear_point(me) {
                return moved;
            }
            self.soldier_hold_position(me);
        }
        moved
    }

    /// `Grenadier_CheckMoveState` (`326-410`): a grenadier that may not chase stays where it
    /// is; a fleeing one stops when its flight is over; one going somewhere may arrive (the
    /// sniper's arrival, its attack delayed 250–500 ms); with no goal, a chaser goes after
    /// its enemy.
    fn grenadier_check_move_state(
        &mut self,
        me: usize,
        enemy_los: bool,
        enemy_dist: f32,
        moving: &mut bool,
        face_enemy: &mut bool,
        command: &mut UserCommand,
    ) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        if npc.script_flags & SCF_CHASE_ENEMIES == 0 {
            if npc.mind.goal == npc.mind.enemy {
                *moving = false;
                return;
            }
        } else if npc.mind.tactics.squad_state == squad::RETREAT {
            if npc.mind.timers.done("flee", level_time) {
                self.actors[me].mind.tactics.squad_state = squad::IDLE;
            } else {
                *face_enemy = false;
            }
        }
        if self.soldier_arrival(me, enemy_los, enemy_dist, command, |world| {
            world.host.irand(250, 500)
        }) {
            return;
        }
        let npc = &mut self.actors[me];
        if npc.mind.goal.is_none() && npc.script_flags & SCF_CHASE_ENEMIES != 0 {
            npc.mind.goal = npc.mind.enemy;
        }
    }

    /// `NPC_BSGrenadier_Attack` (`480-681`).
    fn bs_grenadier_attack(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        if self.actors[me].mind.fight.pain_debounce_time > level_time {
            self.update_angles(me, true, true, command);
            return;
        }
        if !self.check_enemy_ext(me) {
            self.actors[me].mind.enemy = None;
            self.bs_grenadier_patrol(me, command);
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
            self.bs_grenadier_patrol(me, command);
            return;
        };
        let enemy_dist = distance_squared(enemy.origin, self.actors[me].current_origin);
        self.grenadier_weapon_choice(me, &enemy, enemy_dist);
        let (enemy_los, enemy_cs) = self.grenadier_sense(me, &enemy, enemy_dist);
        let (mut moving, mut face_enemy, mut shoot) = (true, enemy_los, false);
        if enemy_cs {
            shoot = true;
            let npc = &self.actors[me];
            let reach = npc.maxs[0] + enemy.maxs[0] + 16.0;
            match npc.player.weapon() {
                WP_THERMAL => moving = false,
                WP_STUN_BATON if enemy_dist < reach * reach => moving = false,
                _ => {}
            }
        }
        self.grenadier_check_move_state(
            me,
            enemy_los,
            enemy_dist,
            &mut moving,
            &mut face_enemy,
            command,
        );
        if moving {
            moving = self.actors[me].mind.goal.is_some() && self.grenadier_move(me, command);
        }
        if !moving {
            if !self.actors[me].mind.timers.done("duck", level_time) {
                command.up_move = -127;
            }
        } else {
            self.actors[me].mind.timers.set("duck", level_time, -1);
        }
        if !face_enemy {
            if moving {
                let npc = &mut self.actors[me];
                npc.desired_yaw = npc.mind.tactics.last_path_angles[1];
                npc.mind.desired_pitch = 0.0;
                shoot = false;
            }
            self.update_angles(me, true, true, command);
        } else {
            self.face_enemy(me, true, command);
        }
        if self.actors[me].script_flags & SCF_DONT_FIRE != 0 {
            shoot = false;
        }
        if shoot
            && self.actors[me].mind.timers.done("attackDelay", level_time)
            && self.actors[me].script_flags & SCF_FIRE_WEAPON == 0
        {
            self.weapon_think(me, command);
            let shot = self.actors[me].mind.fight.shot_time;
            self.actors[me]
                .mind
                .timers
                .set("attackDelay", level_time, shot - level_time);
        }
    }

    /// `NPC_BSGrenadier_Attack`'s choice of weapon (`516-546`): a thermal carrier close to an
    /// enemy without a lit saber that it could get straight to would club it — and chases it
    /// (`NPC_ChangeWeapon` itself does nothing in multiplayer).
    fn grenadier_weapon_choice(
        &mut self,
        me: usize,
        enemy: &crate::npc_senses::Body,
        enemy_dist: f32,
    ) {
        let foe = self.jedi_client(enemy.number);
        let unarmed_foe = foe.is_none_or(|foe| foe.weapon != WP_SABER || foe.sabers_off);
        if enemy_dist >= 16_384.0 || !unarmed_foe || self.actors[me].player.weapon() != WP_THERMAL {
            // The far case would take up a thermal, which changes nothing here.
            return;
        }
        let clip = self
            .actor_at(enemy.number)
            .map_or(MASK_PLAYERSOLID, |at| self.actors[at].clip_mask);
        let npc = &self.actors[me];
        let (origin, number) = (npc.current_origin, npc.number);
        let trace = self.trace_bodies(origin, enemy.mins, enemy.maxs, enemy.origin, number, clip);
        if !trace.all_solid
            && !trace.start_solid
            && (trace.fraction == 1.0 || trace.entity_number == enemy.number)
        {
            self.actors[me].script_flags |= SCF_CHASE_ENEMIES;
        }
    }

    /// `NPC_BSGrenadier_Attack`'s sense of its enemy (`548-590`): in sight, and in front — a
    /// baton's reach and a half-width cone, or a throw whose shot would strike the enemy or
    /// its side within 1024 units — the aim bettering as it has a clear shot, worsening out
    /// of sight. Whether it sees the enemy, and has a clear shot.
    fn grenadier_sense(
        &mut self,
        me: usize,
        enemy: &crate::npc_senses::Body,
        enemy_dist: f32,
    ) -> (bool, bool) {
        if !self.clear_los4(me, enemy) {
            self.aim_adjust(me, -1);
            return (false, false);
        }
        self.actors[me].mind.tactics.enemy_last_seen_time = self.level_time;
        let npc = &self.actors[me];
        let (origin, view) = (npc.current_origin, npc.player.view_angles());
        if npc.player.weapon() == WP_STUN_BATON {
            if enemy_dist <= 4_096.0 && in_fov3(enemy.origin, origin, view, 90, 45) {
                self.actors[me].mind.tactics.enemy_last_seen_location = enemy.origin;
                return (true, true);
            }
            return (true, false);
        }
        if !in_fov3(enemy.origin, origin, view, 45, 90) {
            return (true, false);
        }
        let (hit, _) = self.shot_entity(me, enemy);
        let enemy_team = self.actors[me].enemy_team;
        if hit != enemy.number
            && !self
                .body(hit)
                .is_some_and(|body| body.player_team == enemy_team)
        {
            return (true, false);
        }
        self.actors[me].mind.tactics.enemy_last_seen_location = enemy.origin;
        if distance_horizontal_squared(enemy.origin, origin) < 1_048_576.0 {
            self.aim_adjust(me, 2);
            (true, true)
        } else {
            self.aim_adjust(me, 1);
            (true, false)
        }
    }

    /// The sniper's and the grenadier's own behaviours as `NPC_RunBehavior` reaches them
    /// ([`crate::npc_behavior::Behavior::Stub`] by the reference's name): run, and `true`;
    /// `false` for another name, or while a replay's driver stands the class AI in.
    pub(crate) fn run_soldier(&mut self, me: usize, name: &str, command: &mut UserCommand) -> bool {
        if self.level.jedi_ai_stood_in() {
            return false;
        }
        match name {
            "NPC_BSSniper_Default" => self.bs_sniper_default(me, command),
            "NPC_BSGrenadier_Default" => self.bs_grenadier_default(me, command),
            _ => return false,
        }
        true
    }
}
