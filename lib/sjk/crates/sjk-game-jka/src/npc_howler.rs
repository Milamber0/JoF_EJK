//! The howler's AI (`codemp/game/NPC_AI_Howler.c`): its behaviour
//! (`NPC_BSHowler_Default`, `224-234`), its patrol (`59-93`), its fight (`Howler_Combat`,
//! `156-193`), its bite (`Howler_Attack`, `Howler_TryDamage`, `111-153`) and its pain
//! (`NPC_Howler_Pain`, `200-216`). The retail howler is `TEAM_FREE`, which
//! `NPC_RunBehavior` sends to the default AI: only an enemy-team howler runs this.
//!
//! [`NpcWorld::creature_bite`] is the mine monster's too (`MineMonster_TryDamage`).

use crate::npc_creature::{CHAN_AUTO, DAMAGE_NO_KNOCKBACK, MOD_MELEE};
use crate::npc_jedi_patrol::distance_horizontal_squared;
use crate::npc_senses::distance_squared;
use crate::npc_spawn::{ENTITYNUM_NONE, ENTITYNUM_WORLD, NpcHost};
use crate::npc_wampa::{
    BOTH_ATTACK1, BOTH_PAIN1, BUTTON_WALKING, LSTATE_CLEAR, LSTATE_WAITING, SCF_LOOK_FOR_ENEMIES,
};
use crate::npc_world::NpcWorld;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
use sjk_protocol::UserCommand;

/// `MIN_DISTANCE`, `MAX_DISTANCE` (`NPC_AI_Howler.c:26-30`, the mine monster's alike).
pub(crate) const MIN_DISTANCE: f32 = 54.0;
pub(crate) const MAX_DISTANCE: i32 = 128;
/// `MASK_SHOT`: `CONTENTS_SOLID | CONTENTS_BODY | CONTENTS_CORPSE`.
const MASK_SHOT: u32 = 0x1 | 0x100 | 0x200;

/// How a small creature's bite ended (`Howler_TryDamage`, `MineMonster_TryDamage`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Bite {
    /// Nobody to bite.
    NoEnemy,
    /// The trace hit entity `number`, which was bitten (`G_Damage`).
    Hit(u16),
    /// The trace hit nothing it may bite.
    Miss(u16),
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSHowler_Default` (`NPC_AI_Howler.c:224-234`).
    pub fn bs_howler_default(&mut self, me: usize, command: &mut UserCommand) {
        if self.actors[me].mind.enemy.is_some() {
            self.howler_combat(me, command);
        } else if self.actors[me].script_flags & SCF_LOOK_FOR_ENEMIES != 0 {
            self.creature_patrol(me, false, command);
        }
        // `Howler_Idle` does nothing.
        self.update_angles(me, true, true, command);
    }

    /// `Howler_Patrol` (`59-93`) and `MineMonster_Patrol` (`NPC_AI_MineMonster.c:72-106`):
    /// to its goal, running; a patrol's time kept; client 0 made its enemy within 256; an
    /// enemy looked for. The mine monster idles (`MineMonster_Idle`) without one.
    pub(crate) fn creature_patrol(
        &mut self,
        me: usize,
        idle_to_goal: bool,
        command: &mut UserCommand,
    ) {
        let level_time = self.level_time;
        self.actors[me].mind.fight.local_state = LSTATE_CLEAR;
        if self.update_goal(me, command).is_some() {
            command.buttons &= !BUTTON_WALKING;
            self.move_to_goal(me, true, command);
        } else if self.actors[me].mind.timers.done("patrolTime", level_time) {
            let time = (self.host.rng().flrand(-1.0, 1.0) * 5_000.0 + 5_000.0) as i32;
            self.actors[me]
                .mind
                .timers
                .set("patrolTime", level_time, time);
        }
        // "rwwFIXMEFIXME: Care about all clients, not just client 0"
        if let Some(player) = self
            .host
            .players()
            .iter()
            .find(|body| body.number == 0)
            .copied()
            && distance_squared(player.origin, self.actors[me].current_origin) < (256 * 256) as f32
        {
            self.set_enemy(me, 0);
        }
        if !self.creature_find_enemy(me, true) && idle_to_goal {
            self.mine_monster_idle(me, command);
        }
    }

    /// `Howler_Move` (`100-108`), `MineMonster_Move` alike: at its enemy, unless waiting.
    pub(crate) fn creature_close_in(&mut self, me: usize, command: &mut UserCommand) {
        if self.actors[me].mind.fight.local_state != LSTATE_WAITING {
            self.actors[me].mind.goal = self.actors[me].mind.enemy;
            self.move_to_goal(me, true, command);
            self.actors[me].mind.tactics.goal_radius = MAX_DISTANCE;
        }
    }

    /// `Howler_TryDamage(enemy, damage)` (`111-131`) and `MineMonster_TryDamage`: a shot's
    /// trace 54 units along its view from its origin, and whatever it met bitten —
    /// `anything` the howler's rule (all but the world), else the mine monster's (a real
    /// entity).
    pub(crate) fn creature_bite(&mut self, me: usize, damage: i32, anything: bool) -> Bite {
        if self.actors[me].mind.enemy.is_none() {
            return Bite::NoEnemy;
        }
        let npc = &self.actors[me];
        let direction = Self::forward_of(npc.player.view_angles());
        let origin = npc.current_origin;
        let end: [f32; 3] =
            std::array::from_fn(|axis| origin[axis] + MIN_DISTANCE * direction[axis]);
        let number = npc.number;
        let trace = self.trace_bodies(origin, [0.0; 3], [0.0; 3], end, number, MASK_SHOT);
        let hit = trace.entity_number;
        let bites = if anything {
            hit != ENTITYNUM_WORLD
        } else {
            hit < ENTITYNUM_NONE
        };
        if !bites {
            return Bite::Miss(hit);
        }
        if self.body(hit).is_some() {
            self.creature_damage(
                me,
                hit,
                Some(direction),
                Some(trace.end_position),
                damage,
                DAMAGE_NO_KNOCKBACK,
                MOD_MELEE,
            );
        } else {
            // Something that is no client: `G_Damage` is asked all the same.
            let npc = &self.actors[me];
            let attacker = crate::damage::Attacker {
                npc: true,
                client: number,
                max_health: npc.player.stats[crate::npc_begin::STAT_MAX_HEALTH] as i32,
                team: npc.session_team,
                saber_knockback: [0.0; 4],
            };
            let request = crate::damage::DamageRequest {
                level_time: self.level_time,
                attacker: Some(attacker),
                direction: Some(direction),
                point: Some(trace.end_position),
                damage,
                flags: DAMAGE_NO_KNOCKBACK,
                means: MOD_MELEE,
            };
            self.host.noting_damage(hit, number, &request);
        }
        Bite::Hit(hit)
    }

    /// `Howler_Attack` (`134-153`).
    fn howler_attack(&mut self, me: usize) {
        let level_time = self.level_time;
        if !self.actors[me].mind.timers.exists("attacking") {
            // "Going to do ATTACK1"
            let hold = (1_700.0 + self.host.rng().flrand(0.0, 1.0) * 200.0) as i32;
            self.actors[me]
                .mind
                .timers
                .set("attacking", level_time, hold);
            self.set_animation(
                me,
                SETANIM_BOTH,
                BOTH_ATTACK1,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            self.actors[me]
                .mind
                .timers
                .set("attack_dmg", level_time, 200);
        }
        if self.actors[me]
            .mind
            .timers
            .done2("attack_dmg", level_time, true)
        {
            self.creature_bite(me, 5, true);
        }
        self.actors[me]
            .mind
            .timers
            .done2("attacking", level_time, true);
    }

    /// `Howler_Combat` (`156-193`), and with `mine` `MineMonster_Combat`
    /// (`NPC_AI_MineMonster.c:212-250`): after an enemy it cannot see or while it has a
    /// goal; else, facing it, closing in from beyond 54 (or waiting in pain) and biting
    /// within.
    pub(crate) fn small_creature_combat(
        &mut self,
        me: usize,
        mine: bool,
        command: &mut UserCommand,
    ) {
        let Some(enemy) = self.actors[me].mind.enemy else {
            return;
        };
        let Some(target) = self.body(enemy) else {
            return;
        };
        if !self.clear_los4(me, &target) || self.update_goal(me, command).is_some() {
            let mind = &mut self.actors[me].mind;
            mind.combat_move = true;
            mind.goal = Some(enemy);
            mind.tactics.goal_radius = MAX_DISTANCE;
            self.move_to_goal(me, true, command);
            return;
        }
        self.face_enemy(me, true, command);
        let target = self.body(enemy).unwrap_or(target);
        let distance = distance_horizontal_squared(self.actors[me].current_origin, target.origin);
        let advance = distance > MIN_DISTANCE * MIN_DISTANCE;
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
                self.creature_close_in(me, command);
            }
        } else if mine {
            self.mine_monster_attack(me);
        } else {
            self.howler_attack(me);
        }
    }

    fn howler_combat(&mut self, me: usize, command: &mut UserCommand) {
        self.small_creature_combat(me, false, command);
    }

    /// `NPC_Howler_Pain` (`200-216`): a blow of 10 or more stops its attack and holds it in
    /// pain 2.9 s.
    pub(crate) fn howler_pain(&mut self, me: usize, _attacker: Option<u16>, damage: i32) {
        if damage >= 10 {
            self.small_creature_flinch(me, 2_900);
        }
    }

    /// The howler's and the mine monster's flinch: the attack dropped, `takingPain` for
    /// `time`, angles back to its path's, `BOTH_PAIN1`, waiting.
    pub(crate) fn small_creature_flinch(&mut self, me: usize, time: i32) {
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        npc.mind.timers.remove("attacking");
        npc.mind.timers.set("takingPain", level_time, time);
        let angles = npc.mind.tactics.last_path_angles;
        for (index, value) in [25, 9, 24].into_iter().zip(angles) {
            npc.state.set_raw_field(index, value.to_bits());
        }
        self.set_animation(
            me,
            SETANIM_BOTH,
            BOTH_PAIN1,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        );
        self.actors[me].mind.fight.local_state = LSTATE_WAITING;
    }

    /// `G_Sound(NPC, CHAN_AUTO, G_EffectIndex(name))` (`MineMonster_TryDamage`): the name
    /// registered as an effect, and that index sounded.
    pub(crate) fn effect_as_sound(&mut self, me: usize, name: &[u8]) {
        let index = self.host.effect_index(name);
        let origin = self.actors[me].current_origin;
        self.host
            .raise(crate::weapon_fire::sound_event(origin, CHAN_AUTO, index));
    }
}
