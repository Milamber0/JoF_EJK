//! The AT-ST as an NPC's AI (`codemp/game/NPC_AI_Atst.c`, `CLASS_ATST`; the rideable walker
//! is the vehicles'): its behaviour (`NPC_BSATST_Default`, `332-350`), its fight — its enemy
//! faced and fired at through its move's attack buttons, its side weapons chosen far off by
//! those its model still shows (`ATST_Attack`, `ATST_Ranged`, `171-286`) — its hunt, patrol
//! and idle, and its pain: a groan (`G_ATSTCheckPain`, `88-135`, whose parts' breaking is
//! commented out) then `NPC_Pain` — `PainFunc::Atst` ([`crate::npc_pain`]), which the
//! rideable walker shares. Its death's explosions are `DeathFX`'s ([`crate::npc_death`]).

use crate::npc_jedi_patrol::distance_horizontal_squared;
use crate::npc_machine_parts::TURN_OFF;
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use crate::pmove_anim::SETANIM_BOTH;
use sjk_protocol::UserCommand;

/// `MIN_MELEE_RANGE_SQR`, `MIN_DISTANCE_SQR`.
const MIN_MELEE_RANGE_SQR: f32 = 640.0 * 640.0;
const MIN_DISTANCE_SQR: f32 = 128.0 * 128.0;
/// `BOTH_STAND1`.
const BOTH_STAND1: u16 = 915;
/// `SCF_CHASE_ENEMIES`, `SCF_LOOK_FOR_ENEMIES`; `BUTTON_ATTACK`, `BUTTON_ALT_ATTACK`,
/// `BUTTON_WALKING`.
const SCF_CHASE_ENEMIES: u32 = 0x400;
const SCF_LOOK_FOR_ENEMIES: u32 = 0x800;
const BUTTON_ATTACK: u16 = 1;
const BUTTON_ALT_ATTACK: u16 = 128;
const BUTTON_WALKING: u16 = 16;

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSATST_Default` (`NPC_AI_Atst.c:332-350`).
    pub fn bs_atst_default(&mut self, me: usize, command: &mut UserCommand) {
        if let Some(enemy) = self.actors[me].mind.enemy {
            if self.actors[me].script_flags & SCF_CHASE_ENEMIES != 0 {
                self.actors[me].mind.goal = Some(enemy);
            }
            self.atst_attack(me, command);
        } else if self.actors[me].script_flags & SCF_LOOK_FOR_ENEMIES != 0 {
            self.atst_patrol(me, command);
        } else {
            // `ATST_Idle`.
            self.machine_idle(me, command);
            self.set_animation(me, SETANIM_BOTH, BOTH_STAND1, 0);
        }
    }

    /// `ATST_Hunt` (`152-164`): its enemy made its goal and gone at.
    fn atst_hunt(&mut self, me: usize, command: &mut UserCommand) {
        let npc = &mut self.actors[me];
        if npc.mind.goal.is_none() {
            npc.mind.goal = npc.mind.enemy;
        }
        npc.mind.combat_move = true;
        self.move_to_goal(me, true, command);
    }

    /// `ATST_Ranged(visible, advance, altAttack)` (`171-192`): the attack (the alternate's
    /// too) pressed every half to three seconds while it sees its enemy; the hunt when it
    /// chases.
    fn atst_ranged(
        &mut self,
        me: usize,
        visible: bool,
        alternate: bool,
        command: &mut UserCommand,
    ) {
        let level_time = self.level_time;
        if self.actors[me].mind.timers.done("atkDelay", level_time) && visible {
            let delay = self.host.irand(500, 3_000);
            self.actors[me]
                .mind
                .timers
                .set("atkDelay", level_time, delay);
            command.buttons |= if alternate {
                BUTTON_ATTACK | BUTTON_ALT_ATTACK
            } else {
                BUTTON_ATTACK
            };
        }
        if self.actors[me].script_flags & SCF_CHASE_ENEMIES != 0 {
            self.atst_hunt(me, command);
        }
    }

    /// `ATST_Attack` (`199-286`): its enemy kept or dropped; faced; hunted when unseen and
    /// chased; far off, one of the side weapons its model still has (neither left: no
    /// weapon — `NPC_ChangeWeapon`, which does nothing in multiplayer); then the shot.
    fn atst_attack(&mut self, me: usize, command: &mut UserCommand) {
        if !self.check_enemy_ext(me) {
            self.actors[me].mind.enemy = None;
            return;
        }
        self.face_enemy(me, true, command);
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        else {
            return;
        };
        let distance =
            distance_horizontal_squared(self.actors[me].current_origin, enemy.origin) as i32 as f32;
        let long = distance > MIN_MELEE_RANGE_SQR;
        let visible = self.clear_los4(me, &enemy);
        let _advance = distance > MIN_DISTANCE_SQR;
        if !visible && self.actors[me].script_flags & SCF_CHASE_ENEMIES != 0 {
            self.atst_hunt(me, command);
            return;
        }
        let mut alternate = false;
        if long {
            let blaster = self.machine_surface_status(me, "head_light_blaster_cann");
            let charger = self.machine_surface_status(me, "head_concussion_charger");
            let there = |status: i32| status != -1 && status & TURN_OFF as i32 == 0;
            if there(blaster) && there(charger) {
                // "0 is blaster, 1 is charger (ALT SIDE)"
                alternate = self.host.irand(0, 1) != 0;
            } else if there(blaster) {
                alternate = false;
            } else if there(charger) {
                alternate = true;
            }
        }
        self.face_enemy(me, true, command);
        self.atst_ranged(me, visible, alternate, command);
    }

    /// `ATST_Patrol` (`293-312`): an enemy of its team noticed is faced; else to its goal
    /// walking.
    fn atst_patrol(&mut self, me: usize, command: &mut UserCommand) {
        if self.check_player_team_stealth(me) {
            self.update_angles(me, true, true, command);
            return;
        }
        if self.actors[me].mind.enemy.is_none() && self.update_goal(me, command).is_some() {
            command.buttons |= BUTTON_WALKING;
            self.move_to_goal(me, true, command);
            self.update_angles(me, true, true, command);
        }
    }
}
