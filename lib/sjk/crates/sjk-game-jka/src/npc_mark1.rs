//! The Mark I droid's AI (`codemp/game/NPC_AI_Mark1.c`): its behaviour
//! (`NPC_BSMark1_Default`, `765-782`), its choice between the blaster on its left arm (near)
//! and the rockets on its right (far), by the arms it still has (`Mark1_AttackDecision`,
//! `643-722`), its bursts from four flash bolts (`Mark1_BlasterAttack`, `Mark1_FireBlaster`,
//! `442-566`) and rockets (`Mark1_RocketAttack`, `Mark1_FireRocket`, `573-636`), its hunt,
//! patrol and idle; its pain — an arm or an ammo tube struck hard enough blown off, both
//! arms gone its death (`NPC_Mark1_Pain`, `338-414`); and its dying body's explosions and
//! last shots (`Mark1_dying`, `Mark1Dead_FireBlaster`, `Mark1Dead_FireRocket`, `142-330`),
//! which `NPC_RemoveBody` runs. `Mark1_die` is never installed in multiplayer
//! (`NPC_AI_Mark1.c:767`, commented out); its explosion at death is `DeathFX`'s.

use crate::npc_creature::CHAN_AUTO;
use crate::npc_jedi_patrol::distance_horizontal_squared;
use crate::npc_machine_parts::{
    HL_ARM_LT, HL_ARM_RT, HL_CHEST, HL_GENERIC1, MISSILE_CLIP, TURN_OFF, origin_and_back,
};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_TORSO};
use crate::weapon_fire::Missile;
use sjk_protocol::UserCommand;

/// `LSTATE_FIRED0` .. `LSTATE_FIRED4`: the flash the blaster fired from last.
const LSTATE_FIRED0: i32 = 3;
const LSTATE_FIRED1: i32 = 4;
const LSTATE_FIRED2: i32 = 5;
const LSTATE_FIRED3: i32 = 6;
const LSTATE_FIRED4: i32 = 7;
/// `MIN_MELEE_RANGE_SQR`, `MIN_DISTANCE_SQR`.
const MIN_MELEE_RANGE_SQR: f32 = 320.0 * 320.0;
const MIN_DISTANCE_SQR: f32 = 128.0 * 128.0;
/// `LEFT_ARM_HEALTH`, `RIGHT_ARM_HEALTH`, `AMMO_POD_HEALTH`.
const LEFT_ARM_HEALTH: i32 = 40;
const RIGHT_ARM_HEALTH: i32 = 40;
const AMMO_POD_HEALTH: i32 = 40;
/// `BOWCASTER_VELOCITY`, `BOWCASTER_SIZE`, and the rockets' damage.
const BOWCASTER_VELOCITY: f32 = 1_300.0;
const BOWCASTER_SIZE: f32 = 2.0;
const ROCKET_DAMAGE: i32 = 50;
/// `BOTH_SLEEP1`, `BOTH_ATTACK1`, `BOTH_ATTACK2`, `BOTH_PAIN1`.
const BOTH_SLEEP1: u16 = 1_313;
const BOTH_ATTACK1: u16 = 113;
const BOTH_ATTACK2: u16 = 114;
const BOTH_PAIN1: u16 = 95;
/// `SCF_LOOK_FOR_ENEMIES`; `BUTTON_WALKING`.
const SCF_LOOK_FOR_ENEMIES: u32 = 0x800;
const BUTTON_WALKING: u16 = 16;
/// `WP_BRYAR_PISTOL`, `WP_BOWCASTER`; `MOD_BRYAR_PISTOL`, `MOD_ROCKET`, `MOD_UNKNOWN`.
const WP_BRYAR_PISTOL: u32 = 4;
const WP_BOWCASTER: u32 = 7;
const MOD_BRYAR_PISTOL: u32 = 4;
const MOD_ROCKET: u32 = 19;
const MOD_UNKNOWN: u32 = 0;
const ES_WEAPON: usize = 14;
/// `ps.torsoTimer`.
const PS_TORSO_TIMER: usize = 20;
/// The tubes' surfaces and bolts, by part.
const TUBES: [&str; 6] = [
    "torso_tube1",
    "torso_tube2",
    "torso_tube3",
    "torso_tube4",
    "torso_tube5",
    "torso_tube6",
];
const TUBE_BOLTS: [&str; 6] = [
    "*torso_tube1",
    "*torso_tube2",
    "*torso_tube3",
    "*torso_tube4",
    "*torso_tube5",
    "*torso_tube6",
];
/// The flash bolts its dying body explodes at (`*flash8` .. `*flash10`).
const DYING_FLASHES: [&str; 3] = ["*flash8", "*flash9", "*flash10"];

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSMark1_Default` (`NPC_AI_Mark1.c:765-782`).
    pub fn bs_mark1_default(&mut self, me: usize, command: &mut UserCommand) {
        if let Some(enemy) = self.actors[me].mind.enemy {
            self.actors[me].mind.goal = Some(enemy);
            self.mark1_attack_decision(me, command);
        } else if self.actors[me].script_flags & SCF_LOOK_FOR_ENEMIES != 0 {
            self.mark1_patrol(me, command);
        } else {
            // `Mark1_Idle`.
            self.machine_idle(me, command);
            self.set_animation(me, SETANIM_BOTH, BOTH_SLEEP1, 0);
        }
    }

    /// `NPC_Mark1_Part_Explode` (`101-122`): the bolt's explosion and smoke along its back.
    fn mark1_part_explode(&mut self, me: usize, bolt: i32) {
        if bolt >= 0 {
            let (origin, back) = origin_and_back(self.bolt_matrix(me, bolt));
            self.play_effect_at(b"env/med_explode2", origin, back);
            self.play_effect_at(b"blaster/smoke_bolton", origin, back);
        }
    }

    /// A Mark I's rocket (`Mark1_FireRocket`, `Mark1Dead_FireRocket`): a bowcaster's bolt
    /// of fifty, a box of two, not bouncing.
    fn mark1_rocket(&self, me: usize, muzzle: [f32; 3], direction: [f32; 3]) -> Missile {
        let number = self.actors[me].number;
        let mut missile = crate::npc_machine_parts::create_missile(
            number,
            muzzle,
            direction,
            BOWCASTER_VELOCITY,
            10_000,
            self.level_time,
        );
        missile.state.set_raw_field(ES_WEAPON, WP_BOWCASTER);
        missile.bounds = ([-BOWCASTER_SIZE; 3], [BOWCASTER_SIZE; 3]);
        missile.damage = ROCKET_DAMAGE;
        missile.damage_flags = crate::npc_machine_parts::IMPACT_HALF_ABSORB;
        missile.method_of_death = MOD_ROCKET;
        missile.clip_mask = MISSILE_CLIP;
        missile.splash_damage = 0;
        missile.splash_radius = 0.0;
        missile.bounce_count = 0;
        missile
    }

    /// A Mark I's blaster bolt (`Mark1_FireBlaster`, `Mark1Dead_FireBlaster`): a pistol's
    /// bolt of one.
    fn mark1_blaster_bolt(&self, me: usize, muzzle: [f32; 3], direction: [f32; 3]) -> Missile {
        let number = self.actors[me].number;
        let mut missile = crate::npc_machine_parts::create_missile(
            number,
            muzzle,
            direction,
            1_600.0,
            10_000,
            self.level_time,
        );
        missile.state.set_raw_field(ES_WEAPON, WP_BRYAR_PISTOL);
        missile.damage = 1;
        missile.method_of_death = MOD_BRYAR_PISTOL;
        missile.clip_mask = MISSILE_CLIP;
        missile
    }

    /// `Mark1Dead_FireRocket` (`142-181`): a rocket from `*flash5` along its back.
    fn mark1_dead_fire_rocket(&mut self, me: usize) {
        let bolt = self.add_bolt(me, "*flash5");
        let (muzzle, back) = origin_and_back(self.bolt_matrix(me, bolt));
        self.play_effect_at(b"bryar/muzzle_flash", muzzle, back);
        let number = self.actors[me].number;
        self.creature_sound(number, CHAN_AUTO, b"sound/chars/mark1/misc/mark1_fire");
        let missile = self.mark1_rocket(me, muzzle, back);
        self.launch(me, missile);
    }

    /// `Mark1Dead_FireBlaster` (`189-220`): a bolt from `*flash1` along its back.
    fn mark1_dead_fire_blaster(&mut self, me: usize) {
        let bolt = self.add_bolt(me, "*flash1");
        let (muzzle, back) = origin_and_back(self.bolt_matrix(me, bolt));
        self.play_effect_at(b"bryar/muzzle_flash", muzzle, back);
        let missile = self.mark1_blaster_bolt(me, muzzle, back);
        self.launch(me, missile);
        let number = self.actors[me].number;
        self.creature_sound(number, CHAN_AUTO, b"sound/chars/mark1/misc/mark1_fire");
    }

    /// `Mark1_dying` (`268-330`), from `NPC_RemoveBody`: while its death plays, explosions
    /// now and then at a flash or a tube (the tube's surface gone), and now and then a last
    /// shot from each arm it still has.
    pub(crate) fn mark1_dying(&mut self, me: usize) {
        if self.actors[me].player.torso_timer() <= 0 {
            return;
        }
        let level_time = self.level_time;
        if self.actors[me]
            .mind
            .timers
            .done("dyingExplosion", level_time)
        {
            if self.host.irand(1, 3) == 1 {
                let flash = self.host.irand(8, 10);
                let bolt = self.add_bolt(me, DYING_FLASHES[(flash - 8) as usize]);
                self.mark1_part_explode(me, bolt);
            } else {
                let tube = self.host.irand(1, 6) as usize;
                let bolt = self.add_bolt(me, TUBE_BOLTS[tube - 1]);
                self.mark1_part_explode(me, bolt);
                self.machine_surface(me, TUBES[tube - 1], TURN_OFF);
            }
            let delay = self.host.irand(300, 1_000);
            self.actors[me]
                .mind
                .timers
                .set("dyingExplosion", level_time, delay);
        }
        // "Is the blaster still on the model?"
        if self.machine_surface_status(me, "l_arm") == 0 && self.host.irand(1, 5) == 1 {
            self.mark1_dead_fire_blaster(me);
        }
        // "Is the rocket still on the model?"
        if self.machine_surface_status(me, "r_arm") == 0 && self.host.irand(1, 10) == 1 {
            self.mark1_dead_fire_rocket(me);
        }
    }

    /// `NPC_Mark1_Pain` (`338-414`): `NPC_Pain` and its sound; struck in the chest, a flinch
    /// now and then; an arm or a tube struck past its health blown off; both arms gone, dead.
    pub(crate) fn mark1_pain(&mut self, me: usize, attacker: Option<u16>, damage: i32, means: u32) {
        let part = self.actors[me].mind.creature.walker.pain_hit_location;
        self.npc_pain(me, attacker, damage, means);
        let number = self.actors[me].number;
        self.creature_sound(number, CHAN_AUTO, b"sound/chars/mark1/misc/mark1_pain");
        let hurt = |world: &Self, part: i32| {
            world.actors[me].mind.creature.walker.location_damage[part as usize]
        };
        if part == HL_CHEST {
            let chance = self.host.irand(1, 4);
            if chance == 1 && damage > 5 {
                self.set_animation(
                    me,
                    SETANIM_BOTH,
                    BOTH_PAIN1,
                    SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                );
            }
        } else if part == HL_ARM_LT && hurt(self, HL_ARM_LT) > LEFT_ARM_HEALTH {
            if hurt(self, part) >= LEFT_ARM_HEALTH {
                let bolt = self.add_bolt(me, "*flash3");
                if bolt != -1 {
                    self.mark1_part_explode(me, bolt);
                }
                self.machine_surface(me, "l_arm", TURN_OFF);
            }
        } else if part == HL_ARM_RT && hurt(self, HL_ARM_RT) > RIGHT_ARM_HEALTH {
            if hurt(self, part) >= RIGHT_ARM_HEALTH {
                let bolt = self.add_bolt(me, "*flash4");
                if bolt != -1 {
                    self.mark1_part_explode(me, bolt);
                }
                self.machine_surface(me, "r_arm", TURN_OFF);
            }
        } else {
            for (index, tube) in TUBES.into_iter().enumerate() {
                let pod = HL_GENERIC1 + index as i32;
                if part == pod
                    && hurt(self, pod) > AMMO_POD_HEALTH
                    && hurt(self, part) >= AMMO_POD_HEALTH
                {
                    let bolt = self.add_bolt(me, TUBE_BOLTS[index]);
                    if bolt != -1 {
                        self.mark1_part_explode(me, bolt);
                    }
                    self.machine_surface(me, tube, TURN_OFF);
                    self.set_animation(
                        me,
                        SETANIM_BOTH,
                        BOTH_PAIN1,
                        SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                    );
                    break;
                }
            }
        }
        // "Are both guns shot off?"
        if self.machine_surface_status(me, "l_arm") > 0
            && self.machine_surface_status(me, "r_arm") > 0
        {
            let health = self.actors[me].health;
            self.machine_self_damage(me, None, None, health, 0, MOD_UNKNOWN);
        }
    }

    /// `Mark1_Hunt` (`422-434`): its enemy made its goal, faced, and gone at.
    fn mark1_hunt(&mut self, me: usize, command: &mut UserCommand) {
        let npc = &mut self.actors[me];
        if npc.mind.goal.is_none() {
            npc.mind.goal = npc.mind.enemy;
        }
        self.face_enemy(me, true, command);
        self.actors[me].mind.combat_move = true;
        self.move_to_goal(me, true, command);
    }

    /// `Mark1_FireBlaster` (`442-506`): a bolt from the next of its four flashes, at its
    /// enemy's head.
    fn mark1_fire_blaster(&mut self, me: usize) {
        let fight = &mut self.actors[me].mind.fight;
        let (next, flash) = match fight.local_state {
            state if state <= LSTATE_FIRED0 || state == LSTATE_FIRED4 => (LSTATE_FIRED1, "*flash1"),
            LSTATE_FIRED1 => (LSTATE_FIRED2, "*flash2"),
            LSTATE_FIRED2 => (LSTATE_FIRED3, "*flash3"),
            _ => (LSTATE_FIRED4, "*flash4"),
        };
        fight.local_state = next;
        let bolt = self.add_bolt(me, flash);
        let (muzzle, _) = origin_and_back(self.bolt_matrix(me, bolt));
        let forward = self.machine_aim(me, muzzle);
        self.play_effect_at(b"bryar/muzzle_flash", muzzle, forward);
        let number = self.actors[me].number;
        self.creature_sound(number, CHAN_AUTO, b"sound/chars/mark1/misc/mark1_fire");
        let missile = self.mark1_blaster_bolt(me, muzzle, forward);
        self.launch(me, missile);
    }

    /// `Mark1_BlasterAttack(advance)` (`513-566`): bursts of three to twelve shots, one a
    /// frame at most (`attackDelay2`), a pause of one to three seconds after; between bursts
    /// the hunt when advancing, and the firing stopped.
    fn mark1_blaster_attack(&mut self, me: usize, advance: bool, command: &mut UserCommand) {
        let level_time = self.level_time;
        if self.actors[me].mind.timers.done("attackDelay", level_time) {
            let mut chance = self.host.irand(1, 5);
            let fight = &mut self.actors[me].mind.fight;
            fight.burst_count += 1;
            if fight.burst_count < 3 {
                // "Force it to keep firing."
                chance = 2;
            } else if fight.burst_count > 12 {
                fight.burst_count = 0;
                chance = 1;
            }
            if chance == 1 {
                self.actors[me].mind.fight.burst_count = 0;
                let delay = self.host.irand(1_000, 3_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("attackDelay", level_time, delay);
                self.actors[me].player.set_raw_field(PS_TORSO_TIMER, 0);
            } else {
                if self.actors[me].mind.timers.done("attackDelay2", level_time) {
                    let delay = self.host.irand(50, 50);
                    self.actors[me]
                        .mind
                        .timers
                        .set("attackDelay2", level_time, delay);
                    self.mark1_fire_blaster(me);
                    self.set_animation(
                        me,
                        SETANIM_BOTH,
                        BOTH_ATTACK1,
                        SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                    );
                }
                return;
            }
        } else if advance {
            if self.actors[me].player.torso_animation() == BOTH_ATTACK1 {
                self.actors[me].player.set_raw_field(PS_TORSO_TIMER, 0);
            }
            self.mark1_hunt(me, command);
        } else if self.actors[me].player.torso_animation() == BOTH_ATTACK1 {
            self.actors[me].player.set_raw_field(PS_TORSO_TIMER, 0);
        }
    }

    /// `Mark1_FireRocket` (`573-617`): a rocket from `*flash5` at its enemy's head.
    fn mark1_fire_rocket(&mut self, me: usize) {
        let bolt = self.add_bolt(me, "*flash5");
        let (muzzle, _) = origin_and_back(self.bolt_matrix(me, bolt));
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        else {
            return;
        };
        let head = crate::npc_senses::spot(&enemy, crate::npc_senses::Spot::Head);
        let forward = Self::forward_of(crate::player_angle_math::vector_angles(
            crate::npc_senses::subtract(head, muzzle),
        ));
        let number = self.actors[me].number;
        self.creature_sound(number, CHAN_AUTO, b"sound/chars/mark1/misc/mark1_fire");
        let missile = self.mark1_rocket(me, muzzle, forward);
        self.launch(me, missile);
    }

    /// `Mark1_RocketAttack(advance)` (`624-636`): a rocket every one to three seconds, the
    /// hunt between them when advancing.
    fn mark1_rocket_attack(&mut self, me: usize, advance: bool, command: &mut UserCommand) {
        let level_time = self.level_time;
        if self.actors[me].mind.timers.done("attackDelay", level_time) {
            let delay = self.host.irand(1_000, 3_000);
            self.actors[me]
                .mind
                .timers
                .set("attackDelay", level_time, delay);
            self.set_animation(
                me,
                SETANIM_TORSO,
                BOTH_ATTACK2,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            self.mark1_fire_rocket(me);
        } else if advance {
            self.mark1_hunt(me, command);
        }
    }

    /// `Mark1_AttackDecision` (`643-722`): its enemy kept or dropped; hunting what it cannot
    /// see or face; else the blaster near and the rockets far — whichever arm it still has.
    fn mark1_attack_decision(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        let timers = &self.actors[me].mind.timers;
        if timers.done("patrolNoise", level_time) && timers.done("angerNoise", level_time) {
            let delay = self.host.irand(4_000, 10_000);
            self.actors[me]
                .mind
                .timers
                .set("patrolNoise", level_time, delay);
        }
        let enemy_health = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
            .map_or(0, |enemy| enemy.health);
        if enemy_health < 1 || !self.check_enemy_ext(me) {
            self.actors[me].mind.enemy = None;
            return;
        }
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        else {
            return;
        };
        let distance =
            distance_horizontal_squared(self.actors[me].current_origin, enemy.origin) as i32 as f32;
        let mut long = distance > MIN_MELEE_RANGE_SQR;
        let visible = self.clear_los4(me, &enemy);
        let advance = distance > MIN_DISTANCE_SQR;
        if !visible || !self.face_enemy(me, true, command) {
            self.mark1_hunt(me, command);
            return;
        }
        let blaster = self.machine_surface_status(me, "l_arm");
        let rocket = self.machine_surface_status(me, "r_arm");
        if blaster == 0 && rocket == 0 {
            // "It has both side weapons": so do nothing.
        } else if blaster != -1 && blaster != 0 {
            long = true;
        } else if rocket != -1 && rocket != 0 {
            long = false;
        } else {
            // "It should never get here, but just in case"
            let npc = &mut self.actors[me];
            npc.health = 0;
            npc.player.stats[0] = 0;
            let number = npc.number;
            self.die(me, number, 100, MOD_UNKNOWN);
        }
        self.face_enemy(me, true, command);
        if long {
            self.mark1_rocket_attack(me, advance, command);
        } else {
            self.mark1_blaster_attack(me, advance, command);
        }
    }

    /// `Mark1_Patrol` (`729-757`): an enemy of its team noticed wakes it up; else to its goal
    /// walking.
    fn mark1_patrol(&mut self, me: usize, command: &mut UserCommand) {
        if self.check_player_team_stealth(me) {
            let number = self.actors[me].number;
            self.creature_sound(number, CHAN_AUTO, b"sound/chars/mark1/misc/mark1_wakeup");
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
