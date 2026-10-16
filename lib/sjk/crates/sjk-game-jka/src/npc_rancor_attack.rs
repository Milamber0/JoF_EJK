//! The rancor's attacks (`codemp/game/NPC_AI_Rancor.c`): `Rancor_Attack` (`447-631`) and
//! the blows it times — the swipe that knocks a victim flying or grabs it
//! (`Rancor_Swing`, `212-322`), the ground-shaking smash (`Rancor_Smash`, `324-383`), the
//! bite (`Rancor_Bite`, `385-444`) — and what it does with the victim it holds: the quick
//! bite, the meal that cuts it in half and swallows it, and letting it go
//! (`Rancor_DropVictim`, `156-210`).

use crate::npc_creature::{
    CHAN_AUTO, DAMAGE_NO_ARMOR, DAMAGE_NO_HIT_LOC, DAMAGE_NO_KNOCKBACK, DAMAGE_NO_PROTECTION,
    EF2_HELD_BY_MONSTER, MOD_MELEE, PS_EFLAGS2,
};
use crate::npc_dismember::part;
use crate::npc_rancor::{CLASS_RANCOR, anim, held};
use crate::npc_senses::{AEL_DANGER, distance_squared};
use crate::npc_spawn::{EF_NODRAW, ENTITYNUM_NONE, NpcHost, es};
use crate::npc_world::NpcWorld;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_TORSO};
use sjk_protocol::UserCommand;

/// Player-state wire fields: `eFlags`, `torsoTimer`, `legsTimer`, `lookTarget`,
/// `hasLookTarget`, `forceHandExtend`.
const PS_EFLAGS: usize = 17;
const PS_TORSO_TIMER: usize = 20;
const PS_LEGS_TIMER: usize = 21;
const PS_LOOK_TARGET: usize = 66;
const PS_HAS_LOOK_TARGET: usize = 76;
const PS_FORCE_HAND_EXTEND: usize = 80;
/// `HANDEXTEND_NONE`.
const HANDEXTEND_NONE: u32 = 0;
/// The classes a rancor never grabs (`NPC_AI_Rancor.c:250-264`): itself, Galak's mech, the
/// AT-ST, the droids, the seeker, the remote, the sentry, the interrogator, a vehicle.
const UNGRABBABLE: [i32; 15] = [
    CLASS_RANCOR,
    25,
    1,
    11,
    34,
    35,
    23,
    24,
    29,
    32,
    41,
    39,
    42,
    16,
    53,
];
/// `CLASS_ATST`.
const CLASS_ATST: i32 = 1;
/// `EV_JUMP`, `EV_DEATH1`, `EV_DEATH3`, `EV_SCREENSHAKE`.
const EV_JUMP: u32 = 16;
const EV_DEATH1: i32 = 90;
const EV_DEATH3: i32 = 92;
const EV_SCREENSHAKE: u32 = 42;
/// `Q3_INFINITE`.
const Q3_INFINITE: i32 = 16_777_216;
/// `s.bolt2`, `s.time` (`entityState_t` wire fields).
const ES_BOLT2: usize = 63;
const ES_TIME: usize = 65;

/// `ANGLE2SHORT`.
fn angle_to_short(angle: f32) -> i32 {
    ((angle * 65_536.0 / 360.0) as i32) & 65_535
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `Rancor_DropVictim` (`NPC_AI_Rancor.c:156-210`), which `player_die` runs for a dying
    /// rancor (`g_combat.c:2346-2347`): the victim let go — its view levelled, no longer held
    /// — and, if it is dead, dropped from the hand or hidden in the mouth; alive, it falls
    /// (an NPC thinks again at once).
    pub(crate) fn rancor_drop_victim(&mut self, me: usize) {
        let level_time = self.level_time;
        if let Some(victim) = self.actors[me].mind.creature.activator {
            self.release(victim);
            let count = self.actors[me].count;
            let dead = self
                .reached(victim)
                .is_none_or(|reached| reached.health <= 0);
            if dead {
                if count == 1 {
                    self.with_client(victim, |state, _| {
                        state.set_raw_field(PS_LEGS_TIMER, 0);
                        state.set_raw_field(PS_TORSO_TIMER, 0);
                    });
                } else {
                    self.with_client(victim, |state, _| {
                        let flags = state.raw_field(PS_EFLAGS).unwrap_or(0);
                        state.set_raw_field(PS_EFLAGS, flags | EF_NODRAW);
                    });
                }
            } else {
                if let Some(at) = self.actor_at(victim) {
                    self.actors[at].mind.next_bstate_think = level_time;
                }
                self.with_client(victim, |state, _| {
                    state.set_raw_field(PS_LEGS_TIMER, 0);
                    state.set_raw_field(PS_TORSO_TIMER, 0);
                });
            }
            let npc = &mut self.actors[me];
            if npc.mind.enemy == Some(victim) {
                npc.mind.enemy = None;
            }
            npc.mind.creature.activator = None;
        }
        self.actors[me].count = 0;
    }

    /// `Rancor_DropVictim`'s letting go of client `victim` (`NPC_AI_Rancor.c:162-171`): not
    /// held, no look target, its roll levelled (`SetClientViewAngle`), and an NPC's
    /// `r.currentAngles` levelled (`G_SetAngles`).
    fn release(&mut self, victim: u16) {
        if let Some(at) = self.actor_at(victim) {
            let npc = &mut self.actors[at];
            let flags2 = npc.player.raw_field(PS_EFLAGS2).unwrap_or(0);
            npc.player
                .set_raw_field(PS_EFLAGS2, flags2 & !EF2_HELD_BY_MONSTER);
            npc.player.set_raw_field(PS_HAS_LOOK_TARGET, 0);
            npc.player
                .set_raw_field(PS_LOOK_TARGET, u32::from(ENTITYNUM_NONE));
            let mut view = npc.player.view_angles();
            view[2] = 0.0;
            let command = npc.mind.command.angles;
            npc.player.set_delta_angles(std::array::from_fn(|axis| {
                angle_to_short(view[axis]) - command[axis]
            }));
            for axis in 0..3 {
                npc.state
                    .set_raw_field(es::ANGLES[axis], view[axis].to_bits());
            }
            npc.player.set_view_angles(view);
            let current = &mut npc.mind.current_angles;
            current[0] = 0.0;
            current[2] = 0.0;
            let current = *current;
            // `G_SetAngles`: `r.currentAngles`, `s.angles` and `s.apos.trBase`.
            for axis in 0..3 {
                npc.state
                    .set_raw_field(es::ANGLES[axis], current[axis].to_bits());
                npc.state
                    .set_raw_field(es::APOS_BASE[axis], current[axis].to_bits());
            }
            return;
        }
        // A player: its last command's angles are what its view less its deltas gave.
        self.with_client(victim, |state, _| {
            let flags2 = state.raw_field(PS_EFLAGS2).unwrap_or(0);
            state.set_raw_field(PS_EFLAGS2, flags2 & !EF2_HELD_BY_MONSTER);
            state.set_raw_field(PS_HAS_LOOK_TARGET, 0);
            state.set_raw_field(PS_LOOK_TARGET, u32::from(ENTITYNUM_NONE));
            let old = state.view_angles();
            let delta = state.delta_angles();
            let command: [i32; 3] =
                std::array::from_fn(|axis| angle_to_short(old[axis]) - delta[axis]);
            let mut view = old;
            view[2] = 0.0;
            state.set_delta_angles(std::array::from_fn(|axis| {
                angle_to_short(view[axis]) - command[axis]
            }));
            state.set_view_angles(view);
        });
    }

    /// The grab of `Rancor_Swing` (`NPC_AI_Rancor.c:266-290`): the rancor at `me` takes client
    /// `victim` in its hand — one already in its mouth let go first — makes it its enemy and
    /// holds off its next attack; the victim's pain runs, or (a player, or a dead NPC) its
    /// hand is lowered and it hangs.
    pub(crate) fn rancor_take(&mut self, me: usize, victim: u16) {
        let level_time = self.level_time;
        if self.actors[me].count == 2 {
            self.actors[me].mind.timers.remove("clearGrabbed");
            self.rancor_drop_victim(me);
        }
        let number = self.actors[me].number;
        self.actors[me].mind.enemy = Some(victim);
        self.with_client(victim, |state, _| {
            let flags2 = state.raw_field(PS_EFLAGS2).unwrap_or(0);
            state.set_raw_field(PS_EFLAGS2, flags2 | EF2_HELD_BY_MONSTER);
            state.set_raw_field(PS_HAS_LOOK_TARGET, 1);
            state.set_raw_field(PS_LOOK_TARGET, u32::from(number));
        });
        let npc = &mut self.actors[me];
        npc.mind.creature.activator = Some(victim);
        npc.count = 1;
        let legs = npc.player.legs_timer();
        let wait = self.host.irand(500, 2_500);
        self.actors[me]
            .mind
            .timers
            .set("attacking", level_time, legs + wait);
        match self.actor_at(victim) {
            Some(at) if self.actors[at].health > 0 => {
                let origin = self.actors[at].current_origin;
                // `gPainMOD` is whatever `G_Damage` last left it: the grab deals none.
                self.pain(at, Some(number), 100, 0, origin);
            }
            _ => {
                self.with_client(victim, |state, memory| {
                    state.set_raw_field(PS_FORCE_HAND_EXTEND, HANDEXTEND_NONE);
                    memory.hand_extend_time = 0;
                });
                self.client_animation(
                    victim,
                    SETANIM_BOTH,
                    anim::BOTH_SWIM_IDLE1,
                    SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                );
            }
        }
    }

    /// `Rancor_Swing` (`NPC_AI_Rancor.c:212-322`): every client in reach of its right hand
    /// grabbed (`try_grab`, the hand free, a class it can hold) or smacked — hurt, thrown
    /// and knocked down.
    fn rancor_swing(&mut self, me: usize, try_grab: bool) {
        let hand = self.actors[me].mind.creature.render.hand_r;
        let mut reached = Vec::new();
        let at = self.ents_near_bolt(me, 88.0, hand, &mut reached);
        for victim in reached {
            let Some(target) = self.reached(victim) else {
                continue;
            };
            if victim == self.actors[me].number || held(target.flags2) {
                continue;
            }
            if distance_squared(target.origin, at) > 88.0 * 88.0 {
                continue;
            }
            if try_grab && self.actors[me].count != 1 && !UNGRABBABLE.contains(&target.class) {
                self.rancor_take(me, victim);
                continue;
            }
            self.creature_sound(victim, CHAN_AUTO, b"sound/chars/rancor/swipehit.wav");
            let push = self.smack_direction(me);
            if target.class != CLASS_RANCOR && target.class != CLASS_ATST {
                let damage = self.host.irand(25, 40);
                self.creature_damage(
                    me,
                    victim,
                    Some([0.0; 3]),
                    Some(target.origin),
                    damage,
                    DAMAGE_NO_ARMOR | DAMAGE_NO_KNOCKBACK,
                    MOD_MELEE,
                );
                self.creature_throw(victim, push, 250.0);
                if self.reached(victim).is_some_and(|after| after.health > 0) {
                    self.creature_knockdown(victim);
                }
            }
        }
    }

    /// The way a smack sends its victim (`NPC_AI_Rancor.c:305-308`): the rancor's view turned
    /// 25–50° and raised 15–25°.
    fn smack_direction(&mut self, me: usize) -> [f32; 3] {
        let mut angles = self.actors[me].player.view_angles();
        angles[1] += self.host.rng().flrand(25.0, 50.0);
        angles[0] = self.host.rng().flrand(-25.0, -15.0);
        Self::forward_of(angles)
    }

    /// `Rancor_Smash` (`NPC_AI_Rancor.c:324-383`): its left fist on the ground — heard far,
    /// hurting whoever is under it, knocking down whoever stands near.
    fn rancor_smash(&mut self, me: usize) {
        const RADIUS: f32 = 128.0;
        let (number, origin) = (self.actors[me].number, self.actors[me].current_origin);
        self.alerts.add_sound(
            Some(number),
            origin,
            512.0,
            AEL_DANGER,
            false,
            self.level_time,
        );
        let hand = self.actors[me].mind.creature.render.hand_l;
        let mut reached = Vec::new();
        let at = self.ents_near_bolt(me, RADIUS, hand, &mut reached);
        let half = (RADIUS / 2.0) * (RADIUS / 2.0);
        for victim in reached {
            let Some(target) = self.reached(victim) else {
                continue;
            };
            if victim == number || held(target.flags2) {
                continue;
            }
            let distance = distance_squared(target.origin, at);
            if distance > RADIUS * RADIUS {
                continue;
            }
            self.creature_sound(victim, CHAN_AUTO, b"sound/chars/rancor/swipehit.wav");
            if distance < half {
                let damage = self.host.irand(10, 25);
                self.creature_damage(
                    me,
                    victim,
                    Some([0.0; 3]),
                    Some(target.origin),
                    damage,
                    DAMAGE_NO_ARMOR | DAMAGE_NO_KNOCKBACK,
                    MOD_MELEE,
                );
            }
            let Some(after) = self.reached(victim) else {
                continue;
            };
            if after.health > 0
                && after.class != CLASS_RANCOR
                && after.class != CLASS_ATST
                && (distance < half || after.ground != ENTITYNUM_NONE)
            {
                self.creature_knockdown(victim);
            }
        }
    }

    /// `Rancor_Bite` (`NPC_AI_Rancor.c:385-444`): whoever is near its crotch bolt (its first
    /// bolt, which it never set: the right hand) bitten, a limb maybe bitten off a victim
    /// it kills.
    fn rancor_bite(&mut self, me: usize) {
        let number = self.actors[me].number;
        let crotch = self.actors[me].mind.creature.render.crotch;
        let mut reached = Vec::new();
        let at = self.ents_near_bolt(me, 100.0, crotch, &mut reached);
        for victim in reached {
            let Some(target) = self.reached(victim) else {
                continue;
            };
            if victim == number
                || held(target.flags2)
                || distance_squared(target.origin, at) > 100.0 * 100.0
            {
                continue;
            }
            let damage = self.host.irand(15, 30);
            self.creature_damage(
                me,
                victim,
                Some([0.0; 3]),
                Some(target.origin),
                damage,
                DAMAGE_NO_ARMOR | DAMAGE_NO_KNOCKBACK,
                MOD_MELEE,
            );
            if self.reached(victim).is_some_and(|after| after.health <= 0)
                && self.host.irand(0, 1) == 0
            {
                // "bite something off"
                let limb = self.host.irand(part::HEAD, part::RLEG);
                if limb == part::HEAD {
                    self.client_animation(
                        victim,
                        SETANIM_BOTH,
                        anim::BOTH_DEATH17,
                        SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                    );
                } else if limb == part::WAIST {
                    self.client_animation(
                        victim,
                        SETANIM_BOTH,
                        anim::BOTH_DEATHBACKWARD2,
                        SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                    );
                }
                self.dismember_whole(victim, number, limb);
            }
            self.creature_sound(victim, CHAN_AUTO, b"sound/chars/rancor/chomp.wav");
        }
    }

    /// `G_Dismember(victim, rancor, victim->r.currentOrigin, limb, 90, 0,
    /// victim->client->ps.torsoAnim, qtrue)`, as every rancor's and wampa's cut is made.
    fn dismember_whole(&mut self, victim: u16, rancor: u16, limb: i32) {
        let Some(origin) = self.body(victim).map(|body| body.origin) else {
            return;
        };
        let torso = self
            .with_client(victim, |state, _| state.torso_animation())
            .unwrap_or(0);
        self.dismember(victim, rancor, origin, limb, 90.0, 0.0, torso, true);
    }

    /// `Rancor_Attack` (`NPC_AI_Rancor.c:447-631`): a new attack chosen when none is under
    /// way — the quick bite or the meal of the victim it holds, the charge, the smash or the
    /// grab — and the blows it times dealt (`attack_dmg`, `attack_dmg2`), the grab tried
    /// through its swing's reach.
    pub(crate) fn rancor_attack(
        &mut self,
        me: usize,
        distance: f32,
        charge: bool,
        _command: &mut UserCommand,
    ) {
        let level_time = self.level_time;
        if !self.actors[me].mind.timers.exists("attacking") {
            self.rancor_choose_attack(me, distance, charge);
            let legs = self.actors[me].player.legs_timer();
            let wait = legs as f32 + self.host.rng().flrand(0.0, 1.0) * 200.0;
            self.actors[me]
                .mind
                .timers
                .set("attacking", level_time, wait as i32);
        }
        let legs = self.actors[me].player.leg_animation();
        if self.actors[me]
            .mind
            .timers
            .done2("attack_dmg", level_time, true)
        {
            self.rancor_first_blow(me, legs);
        } else if self.actors[me]
            .mind
            .timers
            .done2("attack_dmg2", level_time, true)
        {
            self.rancor_second_blow(me, legs);
        } else if legs == anim::BOTH_ATTACK2 {
            let timer = self.actors[me].player.legs_timer();
            if (1_200..=1_350).contains(&timer) {
                let grab = self.host.irand(0, 2) == 0;
                self.rancor_swing(me, grab);
            } else if (1_100..=1_550).contains(&timer) {
                self.rancor_swing(me, true);
            }
        }
        // "Just using this to remove the attacking flag at the right time"
        self.actors[me]
            .mind
            .timers
            .done2("attacking", level_time, true);
    }

    /// `Rancor_Attack`'s choice (`NPC_AI_Rancor.c:452-500`).
    fn rancor_choose_attack(&mut self, me: usize, distance: f32, charge: bool) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        let (count, victim) = (npc.count, npc.mind.creature.activator);
        if count == 2 && victim.is_some() {
            return;
        }
        if let (1, Some(victim)) = (count, victim) {
            let alive = self.reached(victim).is_some_and(|target| target.health > 0);
            if alive && self.host.irand(0, 1) != 0 {
                // "quick bite"
                self.set_animation(
                    me,
                    SETANIM_BOTH,
                    anim::BOTH_ATTACK1,
                    SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                );
                self.actors[me]
                    .mind
                    .timers
                    .set("attack_dmg", level_time, 450);
                return;
            }
            // "full eat"
            self.set_animation(
                me,
                SETANIM_BOTH,
                anim::BOTH_ATTACK3,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            self.actors[me]
                .mind
                .timers
                .set("attack_dmg", level_time, 900);
            if self.reached(victim).is_some_and(|target| target.health > 0) {
                // "Make victim scream in fright"
                let scream = self.host.irand(EV_DEATH1, EV_DEATH3);
                self.client_event(victim, scream as u32, 0);
                self.client_animation(
                    victim,
                    SETANIM_TORSO,
                    anim::BOTH_FALLDEATH1,
                    SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                );
                if let Some(at) = self.actor_at(victim) {
                    // "no more thinking for you" — the items tossed are the rancor's own.
                    self.rancor_toss_items(me);
                    self.actors[at].mind.next_bstate_think = Q3_INFINITE;
                }
            }
            return;
        }
        let enemy_alive = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
            .is_some_and(|enemy| enemy.health > 0);
        if enemy_alive && charge {
            let npc = &mut self.actors[me];
            let forward = Self::forward_of([0.0, npc.player.view_angles()[1], 0.0]);
            let speed = distance * 1.5;
            npc.player
                .set_velocity([forward[0] * speed, forward[1] * speed, 150.0]);
            npc.player.set_ground_entity_num(ENTITYNUM_NONE);
            self.set_animation(
                me,
                SETANIM_BOTH,
                anim::BOTH_MELEE2,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            self.actors[me]
                .mind
                .timers
                .set("attack_dmg", level_time, 1_250);
        } else if self.host.irand(0, 1) == 0 {
            // "smash"
            self.set_animation(
                me,
                SETANIM_BOTH,
                anim::BOTH_MELEE1,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            self.actors[me]
                .mind
                .timers
                .set("attack_dmg", level_time, 1_000);
        } else {
            // "try to grab"
            self.set_animation(
                me,
                SETANIM_BOTH,
                anim::BOTH_ATTACK2,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            self.actors[me]
                .mind
                .timers
                .set("attack_dmg", level_time, 1_000);
        }
    }

    /// `Rancor_Attack`'s first blow (`NPC_AI_Rancor.c:507-567`).
    fn rancor_first_blow(&mut self, me: usize, legs: u16) {
        let level_time = self.level_time;
        let number = self.actors[me].number;
        let holding = if self.actors[me].count == 1 {
            self.actors[me].mind.creature.activator
        } else {
            None
        };
        match legs {
            anim::BOTH_MELEE1 => {
                self.rancor_smash(me);
                let hand = self.actors[me].mind.creature.render.hand_l;
                let at = self.bolt_position(me, hand);
                self.screen_shake(at, 4.0, 1_000);
            }
            anim::BOTH_MELEE2 => {
                self.rancor_bite(me);
                self.actors[me]
                    .mind
                    .timers
                    .set("attack_dmg2", level_time, 450);
            }
            anim::BOTH_ATTACK1 => {
                let Some(victim) = holding else { return };
                let Some(target) = self.reached(victim) else {
                    return;
                };
                let damage = self.host.irand(25, 40);
                self.creature_damage(
                    me,
                    victim,
                    Some([0.0; 3]),
                    Some(target.origin),
                    damage,
                    DAMAGE_NO_ARMOR | DAMAGE_NO_KNOCKBACK,
                    MOD_MELEE,
                );
                if self.reached(victim).is_some_and(|after| after.health <= 0) {
                    // "make it look like we bit his head off"
                    self.dismember_whole(victim, number, part::HEAD);
                    self.hang(victim);
                }
                self.creature_sound(victim, CHAN_AUTO, b"sound/chars/rancor/chomp.wav");
            }
            anim::BOTH_ATTACK2 => self.rancor_swing(me, true),
            anim::BOTH_ATTACK3 => {
                let Some(victim) = holding else { return };
                // "cut in half"
                self.dismember_whole(victim, number, part::WAIST);
                self.rancor_kill_held(me, victim);
                self.actors[me]
                    .mind
                    .timers
                    .set("attack_dmg2", level_time, 1_350);
                self.creature_sound(victim, CHAN_AUTO, b"sound/chars/rancor/swipehit.wav");
                let health = self.reached(victim).map_or(0, |after| after.health);
                self.client_event(victim, EV_JUMP, health as u32);
            }
            _ => {}
        }
    }

    /// `Rancor_Attack`'s second blow (`NPC_AI_Rancor.c:568-609`): the charge's bite again, or
    /// the meal swallowed — the victim killed if it still lives, hidden, and held in the
    /// mouth until `clearGrabbed`.
    fn rancor_second_blow(&mut self, me: usize, legs: u16) {
        let level_time = self.level_time;
        let number = self.actors[me].number;
        match legs {
            anim::BOTH_MELEE2 => self.rancor_bite(me),
            anim::BOTH_ATTACK3 => {
                let npc = &self.actors[me];
                let Some(victim) = npc.mind.creature.activator.filter(|_| npc.count == 1) else {
                    return;
                };
                self.creature_sound(victim, CHAN_AUTO, b"sound/chars/rancor/chomp.wav");
                if self.reached(victim).is_some_and(|target| target.health > 0) {
                    self.dismember_whole(victim, number, part::WAIST);
                    self.rancor_kill_held(me, victim);
                    let health = self.reached(victim).map_or(0, |after| after.health);
                    self.client_event(victim, EV_JUMP, health as u32);
                }
                // "*sigh*, can't get tags right, just remove them?"
                self.with_client(victim, |state, _| {
                    let flags = state.raw_field(PS_EFLAGS).unwrap_or(0);
                    state.set_raw_field(PS_EFLAGS, flags | EF_NODRAW);
                });
                self.actors[me].count = 2;
                self.actors[me]
                    .mind
                    .timers
                    .set("clearGrabbed", level_time, 2_600);
            }
            _ => {}
        }
    }

    /// The meal's killing blow (`NPC_AI_Rancor.c:553-560`, `593-597`): `G_Damage` for its
    /// enemy's health and ten more, nothing protecting, and the victim left hanging.
    fn rancor_kill_held(&mut self, me: usize, victim: u16) {
        let enemy_health = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
            .map_or(0, |enemy| enemy.health);
        let origin = self.body(victim).map_or([0.0; 3], |body| body.origin);
        let flags =
            DAMAGE_NO_PROTECTION | DAMAGE_NO_ARMOR | DAMAGE_NO_KNOCKBACK | DAMAGE_NO_HIT_LOC;
        self.creature_damage(
            me,
            victim,
            Some([0.0; 3]),
            Some(origin),
            enemy_health + 10,
            flags,
            MOD_MELEE,
        );
        self.hang(victim);
    }

    /// A dead victim left hanging in the hand: its hand lowered (`HANDEXTEND_NONE`) and
    /// `BOTH_SWIM_IDLE1`.
    fn hang(&mut self, victim: u16) {
        self.with_client(victim, |state, memory| {
            state.set_raw_field(PS_FORCE_HAND_EXTEND, HANDEXTEND_NONE);
            memory.hand_extend_time = 0;
        });
        self.client_animation(
            victim,
            SETANIM_BOTH,
            anim::BOTH_SWIM_IDLE1,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        );
    }

    /// `G_HeldByMonster` (`g_active.c:1552-1587`) for the NPC at `me`, which a monster holds,
    /// as its `ClientThink_real` begins: placed and turned where the rancor's hand (or its
    /// jaw, the victim in its mouth) is (`BG_AttachToRancor`), stopped, and its command's
    /// moves taken away.
    pub(crate) fn held_by_monster(&mut self, me: usize) {
        let npc = &self.actors[me];
        if npc.player.raw_field(PS_HAS_LOOK_TARGET).unwrap_or(0) != 0 {
            let monster = npc.player.raw_field(PS_LOOK_TARGET).unwrap_or(0) as u16;
            if let Some(at) = self.actor_at(monster) {
                let waypoint = self.actors[at].mind.tactics.waypoint;
                self.actors[me].mind.tactics.waypoint = waypoint;
                if self.actors[at].definition.client_class == CLASS_RANCOR {
                    let rancor = &self.actors[at];
                    let in_mouth = rancor.player.raw_field(PS_EFLAGS2).unwrap_or(0)
                        & crate::npc_creature::EF2_GENERIC_NPC_FLAG
                        != 0;
                    let (level_time, victim) = (self.level_time, self.actors[me].number);
                    if let Some((place, view)) =
                        self.host
                            .rancor_attach(&self.actors[at], victim, in_mouth, level_time)
                    {
                        self.actors[me].player.set_origin(place);
                        self.actors[me].player.set_view_angles(view);
                    }
                }
                let npc = &mut self.actors[me];
                npc.player.set_velocity([0.0; 3]);
                let origin = npc.player.origin();
                // `G_SetOrigin`.
                for axis in 0..3 {
                    npc.state
                        .set_raw_field(es::POS_BASE[axis], origin[axis].to_bits());
                    npc.state.set_raw_field(es::POS_DELTA[axis], 0);
                }
                npc.state.set_raw_field(es::POS_TYPE, 0);
                npc.state.set_raw_field(es::POS_TIME, 0);
                npc.state.set_raw_field(es::POS_DURATION, 0);
                npc.current_origin = origin;
                // `SetClientViewAngle`, `G_SetAngles`.
                let view = npc.player.view_angles();
                let command = npc.mind.command.angles;
                npc.player.set_delta_angles(std::array::from_fn(|axis| {
                    angle_to_short(view[axis]) - command[axis]
                }));
                for axis in 0..3 {
                    npc.state
                        .set_raw_field(es::ANGLES[axis], view[axis].to_bits());
                    npc.state
                        .set_raw_field(es::APOS_BASE[axis], view[axis].to_bits());
                }
                npc.mind.current_angles = view;
                npc.relink();
            }
        }
        // "don't allow movement, weapon switching, and most kinds of button presses"
        let command = &mut self.actors[me].mind.command;
        command.forward_move = 0;
        command.right_move = 0;
        command.up_move = 0;
    }

    /// `G_ScreenShake(origin, NULL, intensity, duration, qfalse)` (`g_utils.c:1295-1319`).
    fn screen_shake(&mut self, origin: [f32; 3], intensity: f32, duration: i32) {
        let mut event = crate::event_entity::EventEntity {
            event: EV_SCREENSHAKE,
            parameter: 0,
            origin,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        for axis in 0..3 {
            event.extra[axis] = (es::ORIGIN[axis], origin[axis].to_bits());
        }
        event.extra[3] = (es::ANGLES[0], intensity.to_bits());
        event.extra[4] = (ES_TIME, duration as u32);
        self.host.raise(event);
    }

    /// `TossClientItems(NPC)` (`g_combat.c:587-652`), which the meal runs on the rancor
    /// itself (the reference's slip for its victim): `s.bolt2` takes its weapon; a weapon
    /// with ammo it held (none but the fists' is a retail rancor's) is dropped with
    /// `EV_DESTROY_WEAPON_MODEL`, and every powerup still running after it
    /// ([`crate::dropped_items::toss_client_items`]); the items are the host's
    /// ([`NpcHost::drop_item`]). Nothing in siege.
    fn rancor_toss_items(&mut self, me: usize) {
        let gametype = self.host.gametype();
        let level_time = self.level_time;
        let npc = &self.actors[me];
        let (state, entity, weapon, number) =
            (&npc.player, &npc.state, npc.mind.command.weapon, npc.number);
        let tossed = crate::dropped_items::toss_client_items(
            state,
            entity,
            weapon,
            gametype,
            level_time,
            self.host.rng(),
        );
        if let Some(bolt2) = tossed.bolt2 {
            self.actors[me].state.set_raw_field(ES_BOLT2, bolt2);
        }
        if let Some(event) = tossed.event {
            self.host.raise(event);
        }
        for item in tossed.items {
            self.host.drop_item(number, item);
        }
    }
}
