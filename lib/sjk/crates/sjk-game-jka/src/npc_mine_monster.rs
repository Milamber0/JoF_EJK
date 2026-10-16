//! The mine monster's AI (`codemp/game/NPC_AI_MineMonster.c`): its behaviour
//! (`NPC_BSMineMonster_Default`, `285-301`), its idle and patrol (`57-106`), its attacks
//! (`MineMonster_Attack`, `152-209`, its bites through [`NpcWorld::creature_bite`]) and its
//! pain (`NPC_MineMonster_Pain`, `257-277`). Its fight is the howler's
//! ([`NpcWorld::small_creature_combat`]). No mine monster model ships with multiplayer: it is
//! drawn as Kyle.

use crate::npc_howler::Bite;
use crate::npc_spawn::NpcHost;
use crate::npc_wampa::{
    BOTH_ATTACK1, BOTH_ATTACK2, BOTH_ATTACK3, BUTTON_WALKING, SCF_LOOK_FOR_ENEMIES,
};
use crate::npc_world::NpcWorld;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
use sjk_protocol::UserCommand;

/// `BOTH_ATTACK4`; `EV_PAIN`.
const BOTH_ATTACK4: u16 = 116;
const EV_PAIN: u32 = 89;

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSMineMonster_Default` (`NPC_AI_MineMonster.c:285-301`).
    pub fn bs_mine_monster_default(&mut self, me: usize, command: &mut UserCommand) {
        if self.actors[me].mind.enemy.is_some() {
            self.small_creature_combat(me, true, command);
        } else if self.actors[me].script_flags & SCF_LOOK_FOR_ENEMIES != 0 {
            self.creature_patrol(me, true, command);
        } else {
            self.mine_monster_idle(me, command);
        }
        self.update_angles(me, true, true, command);
    }

    /// `MineMonster_Idle` (`57-64`): to its goal, running.
    pub(crate) fn mine_monster_idle(&mut self, me: usize, command: &mut UserCommand) {
        if self.update_goal(me, command).is_some() {
            command.buttons &= !BUTTON_WALKING;
            self.move_to_goal(me, true, command);
        }
    }

    /// `MineMonster_Attack` (`152-209`): a new attack drawn — the leap at an enemy above it
    /// or now and then, the rare third, the first, or the second — or the one under way's
    /// bites on their timers.
    pub(crate) fn mine_monster_attack(&mut self, me: usize) {
        let level_time = self.level_time;
        if !self.actors[me].mind.timers.exists("attacking") {
            let above = self.actors[me]
                .mind
                .enemy
                .and_then(|enemy| self.body(enemy))
                .map(|enemy| enemy.origin[2] - self.actors[me].current_origin[2]);
            // `enemy && ((above > 10 && random > 0.1) || random > 0.8)`, each draw only
            // when reached.
            let leap = above.is_some_and(|above| {
                (above > 10.0 && self.host.rng().flrand(0.0, 1.0) > 0.1)
                    || self.host.rng().flrand(0.0, 1.0) > 0.8
            });
            let (animation, hold, damage_timer, delay) = if leap {
                let hold = (1_750.0 + self.host.rng().flrand(0.0, 1.0) * 200.0) as i32;
                (BOTH_ATTACK4, hold, "attack2_dmg", 950)
            } else if self.host.rng().flrand(0.0, 1.0) > 0.5 {
                if self.host.rng().flrand(0.0, 1.0) > 0.8 {
                    (BOTH_ATTACK3, 850, "attack2_dmg", 400)
                } else {
                    (BOTH_ATTACK1, 850, "attack1_dmg", 450)
                }
            } else {
                (BOTH_ATTACK2, 1_250, "attack1_dmg", 700)
            };
            self.actors[me]
                .mind
                .timers
                .set("attacking", level_time, hold);
            self.set_animation(
                me,
                SETANIM_BOTH,
                animation,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            self.actors[me]
                .mind
                .timers
                .set(damage_timer, level_time, delay);
        } else if self.actors[me]
            .mind
            .timers
            .done2("attack1_dmg", level_time, true)
        {
            self.mine_monster_bite(me, 5);
        } else if self.actors[me]
            .mind
            .timers
            .done2("attack2_dmg", level_time, true)
        {
            self.mine_monster_bite(me, 10);
        }
        self.actors[me]
            .mind
            .timers
            .done2("attacking", level_time, true);
    }

    /// `MineMonster_TryDamage(enemy, damage)` (`124-149`): the bite, and its sound — a bite's
    /// or a miss's, each through `G_EffectIndex` as the reference has it.
    fn mine_monster_bite(&mut self, me: usize, damage: i32) {
        match self.creature_bite(me, damage, false) {
            Bite::NoEnemy => {}
            Bite::Hit(_) => {
                let which = self.host.irand(1, 4);
                self.effect_as_sound(
                    me,
                    format!("sound/chars/mine/misc/bite{which}.wav").as_bytes(),
                );
            }
            Bite::Miss(_) => {
                let which = self.host.irand(1, 4);
                self.effect_as_sound(
                    me,
                    format!("sound/chars/mine/misc/miss{which}.wav").as_bytes(),
                );
            }
        }
    }

    /// `NPC_MineMonster_Pain` (`257-277`): its pain event, and a blow of 10 or more stops
    /// its attack and holds it in pain 1.35 s. (The reference removes `attacking1_dmg` and
    /// `attacking2_dmg`, which are never set: the bites' own timers stay.)
    pub(crate) fn mine_monster_pain(&mut self, me: usize, _attacker: Option<u16>, damage: i32) {
        let npc = &self.actors[me];
        let max = npc.player.stats[crate::npc_begin::STAT_MAX_HEALTH] as i32;
        // `floor((float)health / maxHealth * 100.0f)`.
        let parameter = (f64::from(npc.health as f32 / max as f32 * 100.0)).floor() as i32;
        self.add_event(me, EV_PAIN, parameter as u32);
        if damage >= 10 {
            self.small_creature_flinch(me, 1_350);
        }
    }
}
