//! The Mark II droid's AI (`codemp/game/NPC_AI_Mark2.c`): its behaviour
//! (`NPC_BSMark2_Default`, `366-381`), its fight — up and running at its enemy, then down
//! and shielded to shoot (`Mark2_AttackDecision`, `231-314`), its blaster from its `*flash`
//! bolt (`Mark2_FireBlaster`, `156-198`), its hunt, patrol and idle, and its pain: an ammo
//! canister struck blows off and kills it (`NPC_Mark2_Pain`, `99-130`). Its death's
//! explosion is `DeathFX`'s ([`crate::npc_death`]).

use crate::npc_creature::CHAN_AUTO;
use crate::npc_jedi_patrol::distance_horizontal_squared;
use crate::npc_machine_parts::{HL_GENERIC1, MISSILE_CLIP, TURN_OFF};
use crate::npc_senses::{Spot, spot};
use crate::npc_spawn::{FL_SHIELDED, NpcHost};
use crate::npc_world::NpcWorld;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
use sjk_protocol::UserCommand;

/// `LSTATE_NONE`, `LSTATE_DROPPINGDOWN`, `LSTATE_DOWN`, `LSTATE_RISINGUP`.
const LSTATE_NONE: i32 = 0;
const LSTATE_DROPPINGDOWN: i32 = 1;
const LSTATE_DOWN: i32 = 2;
const LSTATE_RISINGUP: i32 = 3;
/// `AMMO_POD_HEALTH`; `MIN_DISTANCE_SQR`.
const AMMO_POD_HEALTH: i32 = 1;
const MIN_DISTANCE_SQR: f32 = 24.0 * 24.0;
/// `BOTH_RUN1START`, `BOTH_RUN1STOP`.
const BOTH_RUN1START: u16 = 1_112;
const BOTH_RUN1STOP: u16 = 1_113;
/// `SCF_LOOK_FOR_ENEMIES`; `BUTTON_WALKING`.
const SCF_LOOK_FOR_ENEMIES: u32 = 0x800;
const BUTTON_WALKING: u16 = 16;
/// `WP_BRYAR_PISTOL`, `MOD_BRYAR_PISTOL`; `ES_WEAPON`.
const WP_BRYAR_PISTOL: u32 = 4;
const MOD_BRYAR_PISTOL: u32 = 4;
const ES_WEAPON: usize = 14;
/// The canisters' surfaces, by part.
const CANISTERS: [&str; 3] = ["torso_canister1", "torso_canister2", "torso_canister3"];
/// `DAMAGE_NO_PROTECTION`; `MOD_UNKNOWN`.
const DAMAGE_NO_PROTECTION: u32 = 0x8;
const MOD_UNKNOWN: u32 = 0;

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSMark2_Default` (`NPC_AI_Mark2.c:366-381`).
    pub fn bs_mark2_default(&mut self, me: usize, command: &mut UserCommand) {
        if let Some(enemy) = self.actors[me].mind.enemy {
            self.actors[me].mind.goal = Some(enemy);
            self.mark2_attack_decision(me, command);
        } else if self.actors[me].script_flags & SCF_LOOK_FOR_ENEMIES != 0 {
            self.mark2_patrol(me, command);
        } else {
            self.machine_idle(me, command);
        }
    }

    /// `NPC_Mark2_Part_Explode` (`69-91`): the bolt's explosion and smoke along its back,
    /// and one more canister counted gone.
    fn mark2_part_explode(&mut self, me: usize, bolt: i32) {
        if bolt >= 0 {
            let (origin, back) =
                crate::npc_machine_parts::origin_and_back(self.bolt_matrix(me, bolt));
            self.play_effect_at(b"env/med_explode2", origin, back);
            self.play_effect_at(b"blaster/smoke_bolton", origin, back);
        }
        self.actors[me].count += 1;
    }

    /// `NPC_Mark2_Pain` (`99-130`): `NPC_Pain`; a canister struck (and hurt past its one
    /// point) blown off; the pain's sound; and, with a canister gone, death.
    pub(crate) fn mark2_pain(&mut self, me: usize, attacker: Option<u16>, damage: i32, means: u32) {
        let part = self.actors[me].mind.creature.walker.pain_hit_location;
        self.npc_pain(me, attacker, damage, means);
        for (index, canister) in CANISTERS.into_iter().enumerate() {
            let pod = HL_GENERIC1 + index as i32;
            let hurt = self.actors[me].mind.creature.walker.location_damage[pod as usize];
            if part == pod && hurt > AMMO_POD_HEALTH {
                if hurt >= AMMO_POD_HEALTH {
                    let bolt = self.add_bolt(me, canister);
                    if bolt != -1 {
                        self.mark2_part_explode(me, bolt);
                    }
                    self.machine_surface(me, canister, TURN_OFF);
                    break;
                }
            }
        }
        let number = self.actors[me].number;
        self.creature_sound(number, CHAN_AUTO, b"sound/chars/mark2/misc/mark2_pain");
        if self.actors[me].count > 0 {
            let health = self.actors[me].health;
            self.machine_self_damage(me, None, None, health, DAMAGE_NO_PROTECTION, MOD_UNKNOWN);
        }
    }

    /// `Mark2_Hunt` (`137-149`): its enemy made its goal, faced, and gone at.
    fn mark2_hunt(&mut self, me: usize, command: &mut UserCommand) {
        let npc = &mut self.actors[me];
        if npc.mind.goal.is_none() {
            npc.mind.goal = npc.mind.enemy;
        }
        self.face_enemy(me, true, command);
        self.actors[me].mind.combat_move = true;
        self.move_to_goal(me, true, command);
    }

    /// `Mark2_FireBlaster` (`156-198`): a bolt of one from `*flash` at the enemy's head (the
    /// dead one's straight ahead), with its flash and sound.
    fn mark2_fire_blaster(&mut self, me: usize) {
        let bolt = self.add_bolt(me, "*flash");
        let (muzzle, _) = crate::npc_machine_parts::origin_and_back(self.bolt_matrix(me, bolt));
        let forward = self.machine_aim(me, muzzle);
        self.play_effect_at(b"bryar/muzzle_flash", muzzle, forward);
        let number = self.actors[me].number;
        self.creature_sound(number, CHAN_AUTO, b"sound/chars/mark2/misc/mark2_fire");
        let mut missile = crate::npc_machine_parts::create_missile(
            number,
            muzzle,
            forward,
            1_600.0,
            10_000,
            self.level_time,
        );
        missile.state.set_raw_field(ES_WEAPON, WP_BRYAR_PISTOL);
        missile.damage = 1;
        missile.method_of_death = MOD_BRYAR_PISTOL;
        missile.clip_mask = MISSILE_CLIP;
        self.launch(me, missile);
    }

    /// The way a machine's blaster fires from `muzzle` (`Mark1_FireBlaster`,
    /// `Mark2_FireBlaster`): at its enemy's head while it lives, else along its own angles.
    pub(crate) fn machine_aim(&mut self, me: usize, muzzle: [f32; 3]) -> [f32; 3] {
        let npc = &self.actors[me];
        let enemy = npc.mind.enemy.and_then(|enemy| self.body(enemy));
        match enemy {
            Some(enemy) if self.actors[me].health != 0 => {
                let head = spot(&enemy, Spot::Head);
                let angles = crate::player_angle_math::vector_angles(crate::npc_senses::subtract(
                    head, muzzle,
                ));
                Self::forward_of(angles)
            }
            _ => Self::forward_of(self.actors[me].mind.current_angles),
        }
    }

    /// `Mark2_BlasterAttack(advance)` (`205-224`): a shot when its delay is over (more often
    /// down than up), else the hunt when advancing.
    fn mark2_blaster_attack(&mut self, me: usize, advance: bool, command: &mut UserCommand) {
        let level_time = self.level_time;
        if self.actors[me].mind.timers.done("attackDelay", level_time) {
            let delay = if self.actors[me].mind.fight.local_state == LSTATE_NONE {
                self.host.irand(500, 2_000)
            } else {
                self.host.irand(100, 500)
            };
            self.actors[me]
                .mind
                .timers
                .set("attackDelay", level_time, delay);
            self.mark2_fire_blaster(me);
        } else if advance {
            self.mark2_hunt(me, command);
        }
    }

    /// `Mark2_AttackDecision` (`231-314`): rising when told, hunting what it cannot see,
    /// dropping down to shoot once it has run a while, shooting down there shielded.
    fn mark2_attack_decision(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        self.face_enemy(me, true, command);
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        else {
            return;
        };
        // `(int)DistanceHorizontalSquared` into a float.
        let distance =
            distance_horizontal_squared(self.actors[me].current_origin, enemy.origin) as i32 as f32;
        let visible = self.clear_los4(me, &enemy);
        let advance = distance > MIN_DISTANCE_SQR;
        let local = self.actors[me].mind.fight.local_state;
        if local == LSTATE_RISINGUP {
            self.actors[me].flags &= !FL_SHIELDED;
            self.set_animation(
                me,
                SETANIM_BOTH,
                BOTH_RUN1START,
                SETANIM_FLAG_HOLD | SETANIM_FLAG_OVERRIDE,
            );
            let player = &self.actors[me].player;
            if player.legs_timer() <= 0 && player.torso_animation() == BOTH_RUN1START {
                self.actors[me].mind.fight.local_state = LSTATE_NONE;
            }
            return;
        }
        if !visible || !self.face_enemy(me, true, command) {
            if local == LSTATE_DOWN || local == LSTATE_DROPPINGDOWN {
                if self.actors[me].mind.timers.done("downTime", level_time) {
                    self.mark2_rise(me);
                }
            } else {
                self.mark2_hunt(me, command);
            }
            return;
        }
        if advance
            && self.actors[me].mind.timers.done("downTime", level_time)
            && local == LSTATE_DOWN
        {
            self.mark2_rise(me);
        }
        self.face_enemy(me, true, command);
        match self.actors[me].mind.fight.local_state {
            LSTATE_DROPPINGDOWN => {
                self.set_animation(
                    me,
                    SETANIM_BOTH,
                    BOTH_RUN1STOP,
                    SETANIM_FLAG_HOLD | SETANIM_FLAG_OVERRIDE,
                );
                let down = self.host.irand(3_000, 9_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("downTime", level_time, down);
                let npc = &mut self.actors[me];
                if npc.player.legs_timer() <= 0 && npc.player.torso_animation() == BOTH_RUN1STOP {
                    npc.flags |= FL_SHIELDED;
                    npc.mind.fight.local_state = LSTATE_DOWN;
                }
            }
            LSTATE_DOWN => {
                // "only damagable by lightsabers and missiles"
                self.actors[me].flags |= FL_SHIELDED;
                self.mark2_blaster_attack(me, false, command);
            }
            _ if self.actors[me].mind.timers.done("runTime", level_time) => {
                self.actors[me].mind.fight.local_state = LSTATE_DROPPINGDOWN
            }
            _ if advance => self.mark2_blaster_attack(me, advance, command),
            _ => {}
        }
    }

    /// Told to get up (`Mark2_AttackDecision`, `262-267`, `277-282`): its stop played, and a
    /// while to run before it drops again.
    fn mark2_rise(&mut self, me: usize) {
        self.actors[me].mind.fight.local_state = LSTATE_RISINGUP;
        self.set_animation(
            me,
            SETANIM_BOTH,
            BOTH_RUN1STOP,
            SETANIM_FLAG_HOLD | SETANIM_FLAG_OVERRIDE,
        );
        let run = self.host.irand(3_000, 8_000);
        let level_time = self.level_time;
        self.actors[me].mind.timers.set("runTime", level_time, run);
    }

    /// `Mark2_Patrol` (`322-349`): an enemy of its team noticed is faced; else to its goal
    /// walking, and its chatter's timer kept (the chatter itself is commented out).
    fn mark2_patrol(&mut self, me: usize, command: &mut UserCommand) {
        if self.check_player_team_stealth(me) {
            self.update_angles(me, true, true, command);
            return;
        }
        if self.actors[me].mind.enemy.is_none() {
            if self.update_goal(me, command).is_some() {
                command.buttons |= BUTTON_WALKING;
                self.move_to_goal(me, true, command);
                self.update_angles(me, true, true, command);
            }
            let level_time = self.level_time;
            if self.actors[me].mind.timers.done("patrolNoise", level_time) {
                let delay = self.host.irand(2_000, 4_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("patrolNoise", level_time, delay);
            }
        }
    }
}
