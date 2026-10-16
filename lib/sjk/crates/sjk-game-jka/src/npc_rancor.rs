//! The rancor's AI (`codemp/game/NPC_AI_Rancor.c`): its behaviour (`NPC_BSRancor_Default`,
//! `842-969`), its fight (`Rancor_Combat`, `634-712`), its moves (`Rancor_Idle`,
//! `Rancor_Patrol`, `Rancor_Move`, `71-148`), its roar (`Rancor_CheckRoar`, `84-95`), its
//! crushing tread (`Rancor_Crush`, `819-835`), letting go of a victim it cannot hold
//! (`Rancor_CheckDropVictim`, `800-816`) and its pain (`NPC_Rancor_Pain`, `719-798`). Its
//! attacks — the swipe and grab, the smash, the bite, the meal — are
//! [`crate::npc_rancor_attack`]'s.

use crate::npc_creature::{
    EF2_GENERIC_NPC_FLAG, EF2_HELD_BY_MONSTER, EF2_USE_ALT_ANIM, PS_EFLAGS2,
};
use crate::npc_senses::{AEL_DANGER, distance_squared, in_fov3};
use crate::npc_spawn::{ENTITYNUM_WORLD, FL_NOTARGET, NpcHost, es};
use crate::npc_world::NpcWorld;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
use sjk_protocol::UserCommand;

/// `MIN_DISTANCE`, `MAX_DISTANCE` (`NPC_AI_Rancor.c:28-31`).
const MIN_DISTANCE: f32 = 128.0;
const MAX_DISTANCE: i32 = 1_024;
/// `LSTATE_CLEAR`, `LSTATE_WAITING`.
const LSTATE_CLEAR: i32 = 0;
const LSTATE_WAITING: i32 = 1;
/// `AEL_DANGER_GREAT`.
pub(crate) const AEL_DANGER_GREAT: u32 = 4;
/// `EF2_ALERTED`.
const EF2_ALERTED: u32 = 1 << 2;
/// `BUTTON_WALKING`.
const BUTTON_WALKING: u16 = 16;
/// `SCF_LOOK_FOR_ENEMIES`.
const SCF_LOOK_FOR_ENEMIES: u32 = 0x800;
/// `MOD_CRUSH`.
const MOD_CRUSH: u32 = 36;
/// `CLASS_RANCOR`.
pub(crate) const CLASS_RANCOR: i32 = 54;
/// `TEAM_SPECTATOR`; `npcteam_t`: `NPCTEAM_FREE`, `NPCTEAM_ENEMY`, `NPCTEAM_PLAYER`,
/// `NPCTEAM_NEUTRAL`; `sess.sessionTeam`'s `TEAM_RED`, `TEAM_BLUE`.
const TEAM_SPECTATOR: i32 = 3;
const TEAM_RED: i32 = 1;
const TEAM_BLUE: i32 = 2;
const NPCTEAM_FREE: i32 = 0;
const NPCTEAM_ENEMY: i32 = 1;
const NPCTEAM_PLAYER: i32 = 2;
const NPCTEAM_NEUTRAL: i32 = 3;
/// `s.time`'s wire field.
const ES_TIME: usize = 65;

/// Animations the rancor plays or reads (`anims.h`).
pub(crate) mod anim {
    pub const BOTH_DEATH17: u16 = 25;
    pub const BOTH_DEATHBACKWARD2: u16 = 38;
    pub const BOTH_FALLDEATH1: u16 = 42;
    pub const BOTH_PAIN1: u16 = 95;
    pub const BOTH_PAIN2: u16 = 96;
    pub const BOTH_ATTACK1: u16 = 113;
    pub const BOTH_ATTACK2: u16 = 114;
    pub const BOTH_ATTACK3: u16 = 115;
    pub const BOTH_MELEE1: u16 = 122;
    pub const BOTH_MELEE2: u16 = 123;
    pub const BOTH_STAND1TO2: u16 = 927;
    pub const BOTH_SWIM_IDLE1: u16 = 1_310;
}

/// `Distance` (`VectorLength` of the difference: a float sum, a double root).
pub(crate) fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    f64::from(distance_squared(a, b)).sqrt() as f32
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSRancor_Default` (`NPC_AI_Rancor.c:842-969`).
    pub fn bs_rancor_default(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        let (number, origin) = (self.actors[me].number, self.actors[me].current_origin);
        self.alerts.add_sight(
            Some(number),
            origin,
            1_024.0,
            AEL_DANGER_GREAT,
            50.0,
            level_time,
        );
        self.rancor_crush(me);
        let npc = &mut self.actors[me];
        let mut flags2 = npc.player.raw_field(PS_EFLAGS2).unwrap_or(0)
            & !(EF2_USE_ALT_ANIM | EF2_GENERIC_NPC_FLAG);
        if npc.count != 0 {
            flags2 |= EF2_USE_ALT_ANIM;
            if npc.count == 2 {
                flags2 |= EF2_GENERIC_NPC_FLAG;
            }
        }
        npc.player.set_raw_field(PS_EFLAGS2, flags2);
        if npc.mind.timers.done2("clearGrabbed", level_time, true) {
            self.rancor_drop_victim(me);
        } else if npc.player.leg_animation() == anim::BOTH_PAIN2
            && npc.count == 1
            && npc.mind.creature.activator.is_some()
        {
            if self.host.irand(0, 3) == 0 {
                self.rancor_check_drop_victim(me);
            }
        }
        if !self.actors[me].mind.timers.done("rageTime", level_time) {
            // "do nothing but roar first time we see an enemy"
            self.alerts.add_sound(
                Some(number),
                origin,
                1_024.0,
                AEL_DANGER_GREAT,
                false,
                level_time,
            );
            self.face_enemy(me, true, command);
            return;
        }
        match self.actors[me].mind.enemy {
            Some(enemy) => {
                if self.rancor_with_enemy(me, enemy, command) {
                    return;
                }
            }
            None => {
                if self.actors[me].mind.timers.done("idlenoise", level_time) {
                    let which = self.host.irand(1, 2);
                    self.creature_sound(
                        number,
                        crate::npc_creature::CHAN_AUTO,
                        format!("sound/chars/rancor/snort_{which}.wav").as_bytes(),
                    );
                    let delay = self.host.irand(2_000, 4_000);
                    self.actors[me]
                        .mind
                        .timers
                        .set("idlenoise", level_time, delay);
                    self.alerts.add_sound(
                        Some(number),
                        origin,
                        384.0,
                        AEL_DANGER,
                        false,
                        level_time,
                    );
                }
                if self.actors[me].script_flags & SCF_LOOK_FOR_ENEMIES != 0 {
                    self.rancor_patrol(me, command);
                } else {
                    self.rancor_idle(me, command);
                }
            }
        }
        self.update_angles(me, true, true, command);
    }

    /// `NPC_BSRancor_Default` with an enemy (`NPC_AI_Rancor.c:881-948`): its anger heard,
    /// its meal chewed, a dead or lost enemy forgotten, a better one looked for now and
    /// then, and the fight. Whether the behaviour is over for this think (it chews, or it
    /// got bored and patrolled).
    fn rancor_with_enemy(&mut self, me: usize, enemy: u16, command: &mut UserCommand) -> bool {
        let level_time = self.level_time;
        let (number, origin) = (self.actors[me].number, self.actors[me].current_origin);
        if self.actors[me].mind.timers.done("angrynoise", level_time) {
            let which = self.host.irand(1, 3);
            self.creature_sound(
                number,
                crate::npc_creature::CHAN_AUTO,
                format!("sound/chars/rancor/misc/anger{which}.wav").as_bytes(),
            );
            let delay = self.host.irand(5_000, 10_000);
            self.actors[me]
                .mind
                .timers
                .set("angrynoise", level_time, delay);
        } else {
            self.alerts.add_sound(
                Some(number),
                origin,
                512.0,
                AEL_DANGER_GREAT,
                false,
                level_time,
            );
        }
        let npc = &self.actors[me];
        if npc.count == 2 && npc.player.leg_animation() == anim::BOTH_ATTACK3 {
            // "we're still chewing our enemy up"
            self.update_angles(me, true, true, command);
            return true;
        }
        let enemy_class = self.body(enemy).map(|body| body.class);
        if enemy_class == Some(CLASS_RANCOR) {
            // "got mad at another Rancor, look for a valid enemy"
            if self.actors[me]
                .mind
                .timers
                .done("rancorInfight", level_time)
            {
                self.check_enemy_ext(me);
            }
        } else if self.actors[me].count == 0 {
            if !self.valid_enemy(me, enemy) {
                self.actors[me].mind.timers.remove("lookForNewEnemy");
                let gone = match self.body(enemy) {
                    None => true,
                    Some(_) => {
                        let died = self.enemy_time(enemy);
                        level_time - died > self.host.irand(10_000, 15_000)
                    }
                };
                if gone {
                    // "get bored with him"
                    self.actors[me].mind.enemy = None;
                    self.rancor_patrol(me, command);
                    self.update_angles(me, true, true, command);
                    return true;
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
        self.rancor_combat(me, command);
        false
    }

    /// `s.time` of entity `number` (an NPC's; a player's is never set).
    fn enemy_time(&self, number: u16) -> i32 {
        self.actor_at(number).map_or(0, |at| {
            self.actors[at].state.raw_field(ES_TIME).unwrap_or(0) as i32
        })
    }

    /// `ValidEnemy` (`NPC_combat.c:1360-1425`) of the NPC at `me` for `number`: alive,
    /// targetable, playing, of a team it may fight and not its own.
    pub(crate) fn valid_enemy(&self, me: usize, number: u16) -> bool {
        let Some(body) = self.body(number) else {
            return false;
        };
        if number == self.actors[me].number || body.flags & FL_NOTARGET != 0 || body.health <= 0 {
            return false;
        }
        if body.session_team == TEAM_SPECTATOR || body.spectating {
            return false;
        }
        let team = if body.npc {
            body.player_team
        } else {
            match body.session_team {
                TEAM_BLUE => NPCTEAM_PLAYER,
                TEAM_RED => NPCTEAM_ENEMY,
                _ => NPCTEAM_NEUTRAL,
            }
        };
        let npc = &self.actors[me];
        (team == NPCTEAM_FREE || npc.enemy_team == NPCTEAM_FREE || team == npc.enemy_team)
            && team != npc.player_team
    }

    /// `Rancor_Idle` (`NPC_AI_Rancor.c:71-81`).
    pub(crate) fn rancor_idle(&mut self, me: usize, command: &mut UserCommand) {
        self.actors[me].mind.fight.local_state = LSTATE_CLEAR;
        if self.update_goal(me, command).is_some() {
            command.buttons &= !BUTTON_WALKING;
            self.move_to_goal(me, true, command);
        }
    }

    /// `Rancor_CheckRoar` (`NPC_AI_Rancor.c:84-95`): the first time it is angered (`wait`
    /// still zero) it roars. Whether it did.
    pub(crate) fn rancor_check_roar(&mut self, me: usize) -> bool {
        if self.actors[me].wait != 0.0 {
            return false;
        }
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        npc.wait = 1.0;
        let flags2 = npc.player.raw_field(PS_EFLAGS2).unwrap_or(0);
        npc.player.set_raw_field(PS_EFLAGS2, flags2 | EF2_ALERTED);
        self.set_animation(
            me,
            SETANIM_BOTH,
            anim::BOTH_STAND1TO2,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        );
        let npc = &mut self.actors[me];
        let legs = npc.player.legs_timer();
        npc.mind.timers.set("rageTime", level_time, legs);
        true
    }

    /// `Rancor_Patrol` (`NPC_AI_Rancor.c:101-126`).
    fn rancor_patrol(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        self.actors[me].mind.fight.local_state = LSTATE_CLEAR;
        if self.update_goal(me, command).is_some() {
            command.buttons &= !BUTTON_WALKING;
            self.move_to_goal(me, true, command);
        } else if self.actors[me].mind.timers.done("patrolTime", level_time) {
            let wander = self.host.rng().flrand(-1.0, 1.0) * 5_000.0 + 5_000.0;
            self.actors[me]
                .mind
                .timers
                .set("patrolTime", level_time, wander as i32);
        }
        if !self.check_enemy_ext(me) {
            self.rancor_idle(me, command);
            return;
        }
        self.rancor_check_roar(me);
        let look = self.host.irand(5_000, 15_000);
        self.actors[me]
            .mind
            .timers
            .set("lookForNewEnemy", level_time, look);
    }

    /// `Rancor_Move` (`NPC_AI_Rancor.c:133-148`): after its enemy, unless it waits.
    pub(crate) fn rancor_move(&mut self, me: usize, command: &mut UserCommand) {
        if self.actors[me].mind.fight.local_state == LSTATE_WAITING {
            return;
        }
        self.actors[me].mind.goal = self.actors[me].mind.enemy;
        let moved = self.move_to_goal(me, true, command);
        let tactics = &mut self.actors[me].mind.tactics;
        if moved {
            tactics.consecutive_blocked_moves = 0;
        } else {
            tactics.consecutive_blocked_moves += 1;
        }
        tactics.goal_radius = MAX_DISTANCE;
    }

    /// `Rancor_Combat` (`NPC_AI_Rancor.c:634-712`): holding its victim it bites or eats; out
    /// of sight of its enemy it goes after it; otherwise it faces it and closes in, charges
    /// or attacks.
    fn rancor_combat(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        if self.actors[me].count != 0 {
            if self.actors[me]
                .mind
                .timers
                .done2("takingPain", level_time, true)
            {
                self.actors[me].mind.fight.local_state = LSTATE_CLEAR;
            } else {
                self.rancor_attack(me, 0.0, false, command);
            }
            self.update_angles(me, true, true, command);
            return;
        }
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        else {
            return;
        };
        if !self.clear_los4(me, &enemy) {
            let npc = &mut self.actors[me];
            npc.mind.combat_move = true;
            npc.mind.goal = Some(enemy.number);
            npc.mind.tactics.goal_radius = MIN_DISTANCE as i32;
            if !self.move_to_goal(me, true, command) {
                // "couldn't go after him? Look for a new one"
                let npc = &mut self.actors[me];
                npc.mind.timers.set("lookForNewEnemy", level_time, 0);
                npc.mind.tactics.consecutive_blocked_moves += 1;
            } else {
                self.actors[me].mind.tactics.consecutive_blocked_moves = 0;
            }
            return;
        }
        self.face_enemy(me, true, command);
        let npc = &self.actors[me];
        let distance = distance(npc.current_origin, enemy.origin);
        let mut advance = distance > npc.maxs[0] + MIN_DISTANCE;
        let mut charge = false;
        if advance {
            let yaw_only = [0.0, npc.mind.current_angles[1], 0.0];
            if enemy.health > 0
                && (distance - 250.0).abs() <= 80.0
                && in_fov3(enemy.origin, npc.current_origin, yaw_only, 30, 30)
                && self.host.irand(0, 9) == 0
            {
                // "go for the charge"
                charge = true;
                advance = false;
            }
        }
        if advance && self.actors[me].mind.timers.done("attacking", level_time) {
            if self.actors[me]
                .mind
                .timers
                .done2("takingPain", level_time, true)
            {
                self.actors[me].mind.fight.local_state = LSTATE_CLEAR;
            } else {
                self.rancor_move(me, command);
            }
        } else {
            self.rancor_attack(me, distance, charge, command);
        }
    }

    /// `Rancor_CheckDropVictim` (`NPC_AI_Rancor.c:800-816`): the victim let go where the box
    /// it stands in is clear.
    fn rancor_check_drop_victim(&mut self, me: usize) {
        let Some(victim) = self.actors[me].mind.creature.activator else {
            return;
        };
        let Some(body) = self.body(victim) else {
            return;
        };
        let (absmin_z, absmax_z, clip) = match self.actor_at(victim) {
            Some(at) => (
                self.actors[at].link.0[2],
                self.actors[at].link.1[2],
                self.actors[at].clip_mask,
            ),
            None => (
                body.origin[2] + body.mins[2] - 1.0,
                body.origin[2] + body.maxs[2] + 1.0,
                MASK_PLAYERSOLID,
            ),
        };
        let mins = [body.mins[0] - 1.0, body.mins[1] - 1.0, 0.0];
        let maxs = [body.maxs[0] + 1.0, body.maxs[1] + 1.0, 1.0];
        let start = [body.origin[0], body.origin[1], absmin_z];
        let end = [body.origin[0], body.origin[1], absmax_z - 1.0];
        let trace = self.trace_bodies(start, mins, maxs, end, victim, clip);
        if !trace.all_solid && !trace.start_solid && trace.fraction >= 1.0 {
            self.rancor_drop_victim(me);
        }
    }

    /// `Rancor_Crush` (`NPC_AI_Rancor.c:819-835`): a humanoid it stands on is crushed.
    fn rancor_crush(&mut self, me: usize) {
        let ground = self.actors[me].player.ground_entity_num();
        if ground >= ENTITYNUM_WORLD {
            return;
        }
        let Some(crushed) = self.reached(ground) else {
            return;
        };
        if crushed.humanoid {
            let origin = self.actors[me].current_origin;
            self.creature_damage(me, ground, None, Some(origin), 200, 0, MOD_CRUSH);
        }
    }

    /// `NPC_Rancor_Pain` (`NPC_AI_Rancor.c:719-798`): an attacker taken for its enemy — the
    /// player now and then, anyone when its enemy is dead or another rancor or it is stuck
    /// — and, when hurt enough, its pain played and its attack broken off.
    pub(crate) fn rancor_pain(&mut self, me: usize, attacker: Option<u16>, damage: i32) {
        let level_time = self.level_time;
        let attacker_body = attacker.and_then(|number| self.body(number));
        let hit_by_rancor = attacker_body.is_some_and(|body| body.class == CLASS_RANCOR);
        if let Some(body) = attacker_body
            && Some(body.number) != self.actors[me].mind.enemy
            && body.flags & FL_NOTARGET == 0
            && self.actors[me].count == 0
            && self.rancor_takes_attacker(me, &body)
        {
            self.set_enemy(me, body.number);
            let look = self.host.irand(5_000, 15_000);
            self.actors[me]
                .mind
                .timers
                .set("lookForNewEnemy", level_time, look);
            if hit_by_rancor {
                let infight = self.host.irand(2_000, 5_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("rancorInfight", level_time, infight);
            }
        }
        let npc = &self.actors[me];
        let holding = npc.count == 1 && npc.mind.creature.activator.is_some();
        let hurt = hit_by_rancor
            || (holding && self.host.irand(0, 4) == 0)
            || self.host.irand(0, 200) < damage;
        let npc = &self.actors[me];
        if !hurt
            || npc.player.leg_animation() == anim::BOTH_STAND1TO2
            || !npc.mind.timers.done("takingPain", level_time)
        {
            return;
        }
        if self.rancor_check_roar(me) {
            return;
        }
        let legs = self.actors[me].player.leg_animation();
        if legs == anim::BOTH_MELEE1 || legs == anim::BOTH_MELEE2 || legs == anim::BOTH_ATTACK2 {
            // "cant interrupt one of the big attack anims"
            return;
        }
        if self.actors[me].health > 100 || hit_by_rancor {
            let npc = &mut self.actors[me];
            npc.mind.timers.remove("attacking");
            let path = npc.mind.tactics.last_path_angles;
            for axis in 0..3 {
                npc.state
                    .set_raw_field(es::ANGLES[axis], path[axis].to_bits());
            }
            let pain = if npc.count == 1 {
                anim::BOTH_PAIN2
            } else {
                anim::BOTH_PAIN1
            };
            self.set_animation(
                me,
                SETANIM_BOTH,
                pain,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            let legs_timer = self.actors[me].player.legs_timer();
            let extra = self.host.irand(0, 500);
            let npc = &mut self.actors[me];
            npc.mind
                .timers
                .set("takingPain", level_time, legs_timer + extra);
            npc.mind.fight.local_state = LSTATE_WAITING;
        }
    }

    /// `NPC_Rancor_Pain`'s choice of the attacker (`NPC_AI_Rancor.c:733-737`), each draw
    /// made only where the C's `||` reaches it.
    fn rancor_takes_attacker(&mut self, me: usize, attacker: &crate::npc_senses::Body) -> bool {
        if attacker.number == 0 && self.host.irand(0, 3) == 0 {
            return true;
        }
        let npc = &self.actors[me];
        let Some(enemy) = npc.mind.enemy.and_then(|enemy| self.body(enemy)) else {
            return true;
        };
        if enemy.health == 0 || enemy.class == CLASS_RANCOR {
            return true;
        }
        npc.mind.tactics.consecutive_blocked_moves >= 10
            && distance_squared(attacker.origin, npc.current_origin)
                < distance_squared(enemy.origin, npc.current_origin)
    }
}

/// `MASK_PLAYERSOLID`: a player's clip mask.
const MASK_PLAYERSOLID: u32 = 0x1 | 0x10 | 0x100 | 0x1000;

/// Whether a client's `ps.eFlags2` says a monster holds it.
pub(crate) fn held(flags2: u32) -> bool {
    flags2 & EF2_HELD_BY_MONSTER != 0
}
