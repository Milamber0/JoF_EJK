//! The wampa's attacks (`codemp/game/NPC_AI_Wampa.c`): `Wampa_Attack` (`288-362`) — the
//! double slash, the leap and the backhand, their blows landing on timers through each
//! animation — and `Wampa_Slash` (`198-285`), the blow itself: every client near the
//! swinging hand's bolt hurt, a backhand's thrown and maybe knocked down, a slain one maybe
//! torn apart, one in four knocked down.

use crate::npc_creature::{CHAN_WEAPON, DAMAGE_NO_ARMOR, DAMAGE_NO_KNOCKBACK, MOD_MELEE};
use crate::npc_dismember::part;
use crate::npc_senses::distance_squared;
use crate::npc_spawn::{ENTITYNUM_NONE, NpcHost};
use crate::npc_wampa::{
    BOTH_ATTACK1, BOTH_ATTACK2, BOTH_ATTACK3, BUTTON_WALKING, CLASS_WAMPA, MIN_DISTANCE,
};
use crate::npc_world::NpcWorld;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
use sjk_protocol::UserCommand;

/// The reach of a slash about its hand (`Wampa_Slash`'s `radius`).
const SLASH_RADIUS: f32 = 88.0;
/// `CLASS_RANCOR`, `CLASS_ATST`.
const CLASS_RANCOR: i32 = 54;
const CLASS_ATST: i32 = 1;
/// `BOTH_DEATH17`, `BOTH_DEATHBACKWARD2`.
const BOTH_DEATH17: u16 = 25;
const BOTH_DEATHBACKWARD2: u16 = 38;

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `Wampa_Attack(distance, doCharge)` (`NPC_AI_Wampa.c:288-362`).
    pub(crate) fn wampa_attack(
        &mut self,
        me: usize,
        distance: f32,
        charge: bool,
        command: &mut UserCommand,
    ) {
        let level_time = self.level_time;
        if !self.actors[me].mind.timers.exists("attacking") {
            self.wampa_start_attack(me, distance, charge);
        }
        let legs = self.actors[me].player.leg_animation();
        let (hand_r, hand_l) = (
            self.actors[me].mind.creature.render.hand_r,
            self.actors[me].mind.creature.render.hand_l,
        );
        if self.actors[me]
            .mind
            .timers
            .done2("attack_dmg", level_time, true)
        {
            match legs {
                BOTH_ATTACK1 | BOTH_ATTACK2 => {
                    self.wampa_slash(me, hand_r, false);
                    self.actors[me]
                        .mind
                        .timers
                        .set("attack_dmg2", level_time, 100);
                }
                BOTH_ATTACK3 => self.wampa_slash(me, hand_l, true),
                _ => {}
            }
        } else if self.actors[me]
            .mind
            .timers
            .done2("attack_dmg2", level_time, true)
            && matches!(legs, BOTH_ATTACK1 | BOTH_ATTACK2)
        {
            self.wampa_slash(me, hand_l, false);
        }
        // "Just using this to remove the attacking flag at the right time"
        self.actors[me]
            .mind
            .timers
            .done2("attacking", level_time, true);
        let npc = &self.actors[me];
        if npc.player.leg_animation() == BOTH_ATTACK1 && distance > npc.maxs[0] + MIN_DISTANCE {
            // "okay to keep moving"
            command.buttons |= BUTTON_WALKING;
            self.wampa_move(me, true, command);
        }
    }

    /// `Wampa_Attack`'s choice of an attack (`290-319`): the double slash two times in
    /// three (never for a charge), the leap for a charge or now and then from 270 to 430
    /// away, else the backhand; its damage's timer, and the attack held the animation's
    /// length and up to 200 ms more.
    fn wampa_start_attack(&mut self, me: usize, distance: f32, charge: bool) {
        let level_time = self.level_time;
        if self.host.irand(0, 2) != 0 && !charge {
            // "double slash"
            self.set_animation(
                me,
                SETANIM_BOTH,
                BOTH_ATTACK1,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            self.actors[me]
                .mind
                .timers
                .set("attack_dmg", level_time, 750);
        } else if charge || (distance > 270.0 && distance < 430.0 && self.host.irand(0, 1) == 0) {
            // "leap"
            let yaw = self.actors[me].player.view_angles()[1];
            self.set_animation(
                me,
                SETANIM_BOTH,
                BOTH_ATTACK2,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            self.actors[me]
                .mind
                .timers
                .set("attack_dmg", level_time, 500);
            let forward = Self::forward_of([0.0, yaw, 0.0]);
            let speed = distance * 1.5;
            let npc = &mut self.actors[me];
            npc.player
                .set_velocity([forward[0] * speed, forward[1] * speed, 150.0]);
            npc.player.set_ground_entity_num(ENTITYNUM_NONE);
        } else {
            // "backhand"
            self.set_animation(
                me,
                SETANIM_BOTH,
                BOTH_ATTACK3,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            self.actors[me]
                .mind
                .timers
                .set("attack_dmg", level_time, 250);
        }
        let legs_timer = self.actors[me].player.legs_timer();
        let hold = (legs_timer as f32 + self.host.rng().flrand(0.0, 1.0) * 200.0) as i32;
        let timers = &mut self.actors[me].mind.timers;
        timers.set("attacking", level_time, hold);
        // "allow us to re-evaluate our running speed/anim"
        for name in ["runfar", "runclose", "walk"] {
            timers.set(name, level_time, -1);
        }
    }

    /// `Wampa_Slash(boltIndex, backhand)` (`NPC_AI_Wampa.c:198-285`).
    fn wampa_slash(&mut self, me: usize, bolt: i32, backhand: bool) {
        let damage = if backhand {
            self.host.irand(10, 15)
        } else {
            self.host.irand(20, 30)
        };
        let mut near = Vec::new();
        let at = self.ents_near_bolt(me, SLASH_RADIUS, bolt, &mut near);
        let number = self.actors[me].number;
        for victim in near {
            if victim == number {
                continue;
            }
            let Some(before) = self.reached(victim) else {
                continue;
            };
            if distance_squared(before.origin, at) > SLASH_RADIUS * SLASH_RADIUS {
                continue;
            }
            let flags = if backhand {
                DAMAGE_NO_ARMOR
            } else {
                DAMAGE_NO_ARMOR | DAMAGE_NO_KNOCKBACK
            };
            self.creature_damage(
                me,
                victim,
                Some([0.0; 3]),
                Some(before.origin),
                damage,
                flags,
                MOD_MELEE,
            );
            let Some(after) = self.reached(victim) else {
                continue;
            };
            if backhand {
                // "actually push the enemy"
                let push = self.wampa_push_direction(me);
                if !matches!(after.class, CLASS_WAMPA | CLASS_RANCOR | CLASS_ATST) {
                    self.creature_throw(victim, push, 65.0);
                    let after = self.reached(victim).unwrap_or(after);
                    if after.knockdownable && after.health > 0 && self.host.irand(0, 1) != 0 {
                        self.creature_knockdown(victim);
                    }
                }
            } else if after.health <= 0 {
                // "killed them, chance of dismembering"
                if self.host.irand(0, 1) == 0 {
                    self.wampa_tear(number, victim, after.origin);
                }
            } else if self.host.irand(0, 3) == 0 && after.health > 0 {
                // "one out of every 4 normal hits does a knockdown, too": the push's
                // direction is drawn and never used.
                let _ = self.wampa_push_direction(me);
                self.creature_knockdown(victim);
            }
            self.creature_sound(victim, CHAN_WEAPON, b"sound/chars/rancor/swipehit.wav");
        }
    }

    /// A slash's push (`Wampa_Slash`'s `pushDir`): the wampa's view turned 25-50 degrees
    /// to the side and 15-25 up.
    fn wampa_push_direction(&mut self, me: usize) -> [f32; 3] {
        let mut angles = self.actors[me].player.view_angles();
        angles[1] += self.host.rng().flrand(25.0, 50.0);
        angles[0] = self.host.rng().flrand(-25.0, -15.0);
        Self::forward_of(angles)
    }

    /// A slain victim torn apart (`Wampa_Slash`, `258-270`): a part drawn, its death
    /// animation for the head or the waist, and the part cut off.
    fn wampa_tear(&mut self, wampa: u16, victim: u16, origin: [f32; 3]) {
        let limb = self.host.irand(part::HEAD, part::RLEG);
        if limb == part::HEAD {
            self.client_animation(
                victim,
                SETANIM_BOTH,
                BOTH_DEATH17,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
        } else if limb == part::WAIST {
            self.client_animation(
                victim,
                SETANIM_BOTH,
                BOTH_DEATHBACKWARD2,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
        }
        let torso = self
            .with_client(victim, |state, _| state.torso_animation())
            .unwrap_or(0);
        self.dismember(victim, wampa, origin, limb, 90.0, 0.0, torso, true);
    }
}
