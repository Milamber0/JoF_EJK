//! The Jedi AI's choice at its distance from its enemy: `Jedi_CombatDistance`
//! (`codemp/game/NPC_AI_Jedi.c:1418-1853`) — hold while gripping or draining, keep a
//! thrown saber's distance, finish a taunt, press a won saber lock, back off from a locked
//! enemy, drain or push at arm's length, retreat when too close, heal or rage from afar and
//! give chase, and between striking range and 256 the tactical Force: rush a gripped enemy,
//! grip a saber thrower, or now and then pull, lightning, drain, grip or throw the saber;
//! last, rage when really mad.
//!
//! What the enemy is to these rules ([`JediFoe`]) is read from the NPC when it is one, and
//! from the host for a player.

use crate::force_powers::{
    FP_ABSORB, FP_DRAIN, FP_GRIP, FP_HEAL, FP_LIGHTNING, FP_PROTECT, FP_PULL, FP_RAGE,
    FP_SABER_THROW, FP_SPEED,
};
use crate::npc_jedi_moves::{CLASS_DESANN, EV_JCHASE1, EV_JCHASE3, EV_TAUNT1, EV_TAUNT3, rank};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use sjk_protocol::UserCommand;

/// `BUTTON_ATTACK`, `BUTTON_WALKING`, `BUTTON_ALT_ATTACK`.
const BUTTON_ATTACK: u16 = 1;
const BUTTON_WALKING: u16 = 16;
const BUTTON_ALT_ATTACK: u16 = 128;
/// `SCF_DONT_FIRE`.
const SCF_DONT_FIRE: u32 = 0x4000;
/// `SEF_INWATER`; `SEF_LOCK_WON`.
const SEF_INWATER: u32 = 0x80;
const SEF_LOCK_WON: u32 = crate::saber_clash::sef::LOCK_WON;
/// `SES_LEAVING`: a thrown saber on its way out.
const SES_LEAVING: i32 = 1;
/// `WP_SABER`.
const WP_SABER: i32 = 3;
/// `CLASS_BOBAFETT`.
const CLASS_BOBAFETT: i32 = 52;
/// `HANDEXTEND_JEDITAUNT`.
const HANDEXTEND_JEDITAUNT: u8 = 16;
/// `ENTITYNUM_NONE`.
const ENTITYNUM_NONE: u16 = crate::npc_spawn::ENTITYNUM_NONE;
/// `FORCE_LEVEL_1`.
const FORCE_LEVEL_1: i32 = 1;
/// `ps.weaponTime`, `ps.fd.forcePowersKnown`, `ps.stats[STAT_MAX_HEALTH]`.
const PS_WEAPON_TIME: usize = 10;
const PS_FORCE_KNOWN: usize = 51;
const STAT_MAX_HEALTH: usize = 8;

/// The enemy as `Jedi_CombatDistance` and `Jedi_Strafe` read it (`NPC->enemy`, its
/// `client->ps`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JediFoe {
    /// It has a client: a player or an NPC.
    pub client: bool,
    /// `r.currentOrigin`.
    pub origin: [f32; 3],
    /// `s.weapon`.
    pub weapon: i32,
    /// `ps.groundEntityNum`.
    pub ground: u16,
    /// `painDebounceTime`.
    pub pain_debounce_time: i32,
    /// `ps.saberLockTime`.
    pub saber_lock_time: i32,
    /// `ps.fd.forceGripBeingGripped` (a float).
    pub grip_being_gripped: f32,
    /// `ps.fd.forcePowersActive`.
    pub force_powers_active: u32,
    /// `ps.saberInFlight`, `ps.saberEntityNum`.
    pub saber_in_flight: bool,
    pub saber_entity_num: u16,
}

/// What `Jedi_CombatDistance`'s first branches did (`NPC_AI_Jedi.c:1447-1576`).
enum Special {
    /// None applied: the plain distances decide.
    None,
    /// One applied; the rage check follows.
    Handled,
    /// A drain was begun and the reference returned.
    Returned,
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// The entity numbered `number` as the Jedi AI reads its enemy: an NPC's own state, a
    /// player's through the host, anything else by where it is.
    pub fn jedi_foe(&self, number: u16) -> Option<JediFoe> {
        if let Some(at) = self.actor_at(number) {
            let npc = &self.actors[at];
            let state = &npc.player;
            return Some(JediFoe {
                client: true,
                origin: npc.current_origin,
                weapon: i32::from(state.weapon()),
                ground: state.ground_entity_num(),
                pain_debounce_time: npc.mind.fight.pain_debounce_time,
                saber_lock_time: state.saber_lock_time(),
                grip_being_gripped: npc.force.grip_being_gripped,
                force_powers_active: state.force_powers_active(),
                saber_in_flight: state.saber_in_flight(),
                saber_entity_num: state.saber_entity_num(),
            });
        }
        if self
            .host
            .players()
            .iter()
            .any(|player| player.number == number)
        {
            return self.jedi_player_foe(number);
        }
        self.host.entity_box(number).map(|(origin, _, _)| JediFoe {
            origin,
            ground: ENTITYNUM_NONE,
            ..JediFoe::default()
        })
    }

    /// `jediSpeechDebounceTime` and the NPC's own speech both clear, and its chatter timer
    /// done: a taunt, and none again for three seconds (`NPC_AI_Jedi.c:1682-1687`).
    fn jedi_grip_taunt(&mut self, me: usize) {
        let level_time = self.level_time;
        if self.actors[me].mind.timers.done("chatter", level_time)
            && self.jedi_speech_debounce(me) < level_time
            && self.actors[me].mind.blocked_speech_until < level_time
        {
            let event = self.host.irand(EV_TAUNT1, EV_TAUNT3);
            self.add_voice(me, event, 3_000);
            self.jedi_hush(me, level_time + 3_000);
            self.actors[me]
                .mind
                .timers
                .set("chatter", level_time, 3_000);
        }
    }

    /// `Jedi_CombatDistance` (`NPC_AI_Jedi.c:1418-1853`) at `enemy_dist` from its enemy
    /// (0 is within striking range).
    pub fn jedi_combat_distance(&mut self, me: usize, enemy_dist: i32, command: &mut UserCommand) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        let active = npc.player.force_powers_active();
        if active & (1 << FP_GRIP) != 0 && npc.force_levels[FP_GRIP] > FORCE_LEVEL_1 {
            // "when gripping, don't move"
            return;
        } else if !npc.mind.timers.done("gripping", level_time) {
            self.actors[me]
                .mind
                .timers
                .set("gripping", level_time, -level_time);
            let delay = self.host.irand(0, 1_000);
            self.actors[me]
                .mind
                .timers
                .set("attackDelay", level_time, delay);
        }
        let npc = &self.actors[me];
        if crate::npc_behavior::cultist_destroyer(
            npc.definition.client_class,
            npc.player.weapon(),
            &npc.npc_type,
        ) {
            self.jedi_advance(me, command);
            let npc = &mut self.actors[me];
            npc.player.set_speed(npc.definition.stats.run_speed as f32);
            command.buttons &= !BUTTON_WALKING;
        }
        let npc = &self.actors[me];
        if npc.player.force_powers_active() & (1 << FP_DRAIN) != 0
            && npc.force_levels[FP_DRAIN] > FORCE_LEVEL_1
        {
            // "when draining, don't move"
            return;
        } else if !npc.mind.timers.done("draining", level_time) {
            self.actors[me]
                .mind
                .timers
                .set("draining", level_time, -level_time);
            let delay = self.host.irand(0, 1_000);
            self.actors[me]
                .mind
                .timers
                .set("attackDelay", level_time, delay);
        }
        match self.jedi_distance_special(me, enemy_dist, command) {
            Special::None => self.jedi_distance_general(me, enemy_dist, command),
            Special::Handled => {}
            // A drain begun: the reference returns, and never gets as far as its rage.
            Special::Returned => return,
        }
        self.jedi_rage_when_mad(me);
    }

    /// The branches of `Jedi_CombatDistance` that come before the plain distances
    /// (`NPC_AI_Jedi.c:1447-1564`): Boba Fett's range, a thrown saber, a taunt, a won
    /// saber lock, a locked enemy, a Jedi that may not use its saber, a drain up close.
    /// Whether one applied (the distances are not looked at), or the reference returned.
    fn jedi_distance_special(
        &mut self,
        me: usize,
        enemy_dist: i32,
        command: &mut UserCommand,
    ) -> Special {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        let state = &npc.player;
        let foe = npc
            .mind
            .enemy
            .and_then(|enemy| self.jedi_foe(enemy))
            .unwrap_or_default();
        if npc.definition.client_class == CLASS_BOBAFETT {
            if !npc.mind.timers.done("flameTime", level_time) {
                if enemy_dist > 50 {
                    self.jedi_advance(me, command);
                } else if enemy_dist <= 0 {
                    self.jedi_retreat(me, command);
                }
            } else if enemy_dist < 200 {
                self.jedi_retreat(me, command);
            } else if enemy_dist > 1024 {
                self.jedi_advance(me, command);
            }
        } else if state.saber_in_flight()
            && !crate::saber_rules::in_broken_parry(state.saber_move())
            && u32::from(state.saber_blocked()) != crate::saber_clash::BLOCKED_PARRY_BROKEN
        {
            self.jedi_keep_saber_distance(me, enemy_dist, command);
        } else if !npc.mind.timers.done("taunting", level_time) {
            if enemy_dist <= 64 {
                command.buttons |= BUTTON_ATTACK;
                if !self.actors[me].player.saber_in_flight() {
                    self.activate_saber(me);
                }
                self.actors[me]
                    .mind
                    .timers
                    .set("taunting", level_time, -level_time);
            } else if state.force_hand_extend() == HANDEXTEND_JEDITAUNT
                && npc.movement.state().force_hand_extend_time - level_time < 200
                && !state.saber_in_flight()
            {
                // "we're almost done with our special taunt"
                self.activate_saber(me);
            }
        } else if npc.saber.event_flags & SEF_LOCK_WON != 0 {
            // "we won a saber lock, press the advantage"
            if enemy_dist > 0 {
                self.jedi_advance(me, command);
            }
            let npc = &mut self.actors[me];
            if enemy_dist > 128 {
                npc.saber.event_flags &= !SEF_LOCK_WON;
            }
            if foe.pain_debounce_time + 2_000 < level_time {
                npc.saber.event_flags &= !SEF_LOCK_WON;
            }
            npc.mind.timers.set("strafeLeft", level_time, -1);
            npc.mind.timers.set("strafeRight", level_time, -1);
        } else if foe.client
            && foe.weapon == WP_SABER
            && foe.saber_lock_time > level_time
            && state.saber_lock_time() < level_time
        {
            // "enemy is in a saberLock and we are not"
            if enemy_dist < 64 {
                self.jedi_retreat(me, command);
            }
        } else if enemy_dist <= 64
            && (npc.script_flags & SCF_DONT_FIRE != 0
                || (npc.npc_type.eq_ignore_ascii_case(b"yoda") && self.host.irand(0, 10) == 0))
        {
            // "can't use saber and they're in striking range"
            return self.jedi_push_or_drain(me, foe, command);
        } else {
            return if self.jedi_drain_close(me, enemy_dist, foe, command) {
                Special::Returned
            } else {
                Special::None
            };
        }
        Special::Handled
    }

    /// A thrown saber's distance kept (`NPC_AI_Jedi.c:1470-1490`), and a saber on its way
    /// out held out there.
    fn jedi_keep_saber_distance(&mut self, me: usize, enemy_dist: i32, command: &mut UserCommand) {
        let saber_dist = self.actors[me].mind.saber_entity_dist;
        if enemy_dist < saber_dist {
            self.jedi_retreat(me, command);
        } else if enemy_dist > saber_dist && enemy_dist > 100 {
            self.jedi_advance(me, command);
        }
        let npc = &self.actors[me];
        if i32::from(npc.player.weapon()) == WP_SABER
            && npc.mind.saber_entity_state == SES_LEAVING
            && npc.force_levels[FP_SABER_THROW] > FORCE_LEVEL_1
            && npc.player.force_powers_active() & (1 << FP_SPEED) == 0
            && npc.saber.event_flags & SEF_INWATER == 0
        {
            command.buttons |= BUTTON_ALT_ATTACK;
        }
    }

    /// Within 64 and not to use its saber (`NPC_AI_Jedi.c:1536-1558`): now and then, the
    /// enemy in front, a drain when hurt or not firing — or a push — then back off. The
    /// reference returns after starting the drain, before backing off.
    fn jedi_push_or_drain(
        &mut self,
        me: usize,
        foe: JediFoe,
        command: &mut UserCommand,
    ) -> Special {
        let npc = &self.actors[me];
        let in_front = crate::saber_block::in_front(
            foe.origin,
            npc.current_origin,
            npc.player.view_angles(),
            0.2,
        );
        if self.host.irand(0, 5) == 0 && in_front {
            let npc = &self.actors[me];
            let hurt = npc.script_flags & SCF_DONT_FIRE != 0
                || (npc.max_health - npc.health) as f32 > npc.max_health as f32 * 0.25;
            if hurt
                && self.knows(me, FP_DRAIN)
                && self.force_power_available(me, FP_DRAIN, 20)
                && self.host.irand(0, 2) == 0
            {
                self.jedi_start_drain(me, command);
                return Special::Returned;
            }
            self.force_throw(me, false);
        }
        self.jedi_retreat(me, command);
        Special::Handled
    }

    /// The drain's start (`NPC_AI_Jedi.c:1545-1549`, `1568-1571`): the "draining" and
    /// "attackDelay" timers for three seconds, and at its enemy.
    fn jedi_start_drain(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        let timers = &mut self.actors[me].mind.timers;
        timers.set("draining", level_time, 3_000);
        timers.set("attackDelay", level_time, 3_000);
        self.jedi_advance(me, command);
    }

    /// Whether the NPC knows `power` (`ps.fd.forcePowersKnown`).
    fn knows(&self, me: usize, power: usize) -> bool {
        self.actors[me]
            .player
            .raw_field(PS_FORCE_KNOWN)
            .unwrap_or(0)
            & (1 << power)
            != 0
    }

    /// Whether the NPC knows `power` and is not running it.
    fn could_start(&self, me: usize, power: usize) -> bool {
        self.knows(me, power) && self.actors[me].player.force_powers_active() & (1 << power) == 0
    }

    /// Within 64 and hurt (`NPC_AI_Jedi.c:1559-1572`): a drain now and then, the enemy in
    /// front. Whether it drained (and the reference returned); otherwise the distances decide.
    fn jedi_drain_close(
        &mut self,
        me: usize,
        enemy_dist: i32,
        foe: JediFoe,
        command: &mut UserCommand,
    ) -> bool {
        let npc = &self.actors[me];
        if enemy_dist <= 64
            && (npc.max_health - npc.health) as f32 > npc.max_health as f32 * 0.25
            && self.knows(me, FP_DRAIN)
            && self.force_power_available(me, FP_DRAIN, 20)
            && self.host.irand(0, 10) == 0
            && {
                let npc = &self.actors[me];
                crate::saber_block::in_front(
                    foe.origin,
                    npc.current_origin,
                    npc.player.view_angles(),
                    0.2,
                )
            }
        {
            self.jedi_start_drain(me, command);
            return true;
        }
        false
    }

    /// The plain distances (`NPC_AI_Jedi.c:1573-1837`).
    fn jedi_distance_general(&mut self, me: usize, enemy_dist: i32, command: &mut UserCommand) {
        let aggression = self.actors[me].definition.stats.aggression;
        if enemy_dist <= -16 {
            // "we're too damn close!"
            self.jedi_retreat(me, command);
        } else if enemy_dist <= 0 {
            if aggression < 4 {
                self.jedi_retreat(me, command);
            }
        } else if enemy_dist > 256 {
            self.jedi_far_away(me, enemy_dist, command);
        } else if enemy_dist > 50 {
            self.jedi_tactical_force(me, enemy_dist, command);
        } else if aggression < 4 {
            // "not close enough to attack, but not far enough away to be safe"
            self.jedi_retreat(me, command);
        } else if aggression > 5
            && enemy_dist > 0
            && self.actors[me].script_flags & SCF_DONT_FIRE == 0
            && self.jedi_may_advance(me)
        {
            self.jedi_advance(me, command);
        }
    }

    /// Not parrying (or of a rank above lieutenant) and its enemy on the ground (or no
    /// client): may close in (`NPC_AI_Jedi.c:1805-1812`, `1823-1830`).
    fn jedi_may_advance(&self, me: usize) -> bool {
        let npc = &self.actors[me];
        let parrying =
            !npc.mind.timers.done("parryTime", self.level_time) && npc.definition.rank <= rank::LT;
        let foe = npc
            .mind
            .enemy
            .and_then(|enemy| self.jedi_foe(enemy))
            .unwrap_or_default();
        !parrying && (!foe.client || foe.ground != ENTITYNUM_NONE)
    }

    /// Way out of range (`NPC_AI_Jedi.c:1584-1650`): hurt and not so eager, a heal,
    /// protection, absorption or rage now and then; far off, a word as it gives chase; and
    /// at the enemy unless it used the Force.
    fn jedi_far_away(&mut self, me: usize, enemy_dist: i32, command: &mut UserCommand) {
        let level_time = self.level_time;
        let mut used_force = false;
        let aggression = self.actors[me].definition.stats.aggression;
        if aggression < self.host.irand(0, 20)
            && {
                let npc = &self.actors[me];
                (npc.health as f32) < npc.max_health as f32 * 0.75
            }
            && self.host.irand(0, 2) == 0
        {
            if self.could_start(me, FP_HEAL) && self.host.irand(0, 1) != 0 {
                self.force_heal(me);
                used_force = true;
            } else if self.could_start(me, FP_PROTECT) && self.host.irand(0, 1) != 0 {
                self.force_protect(me);
                used_force = true;
            } else if self.could_start(me, FP_ABSORB) && self.host.irand(0, 1) != 0 {
                self.force_absorb(me);
                used_force = true;
            } else if self.could_start(me, FP_RAGE) && self.host.irand(0, 1) != 0 {
                self.jedi_rage(me);
                used_force = true;
            }
        }
        if enemy_dist > 384
            && self.host.irand(0, 10) == 0
            && self.actors[me].mind.blocked_speech_until < level_time
            && self.jedi_speech_debounce(me) < level_time
        {
            let enemy = self.actors[me]
                .mind
                .enemy
                .and_then(|enemy| self.body(enemy));
            if enemy.is_some_and(|enemy| self.clear_los4(me, &enemy)) {
                let event = self.host.irand(EV_JCHASE1, EV_JCHASE3);
                self.add_voice(me, event, 3_000);
            }
            self.jedi_hush(me, level_time + 3_000);
        }
        if self.actors[me].definition.stats.aggression > 0 && !used_force {
            self.jedi_advance(me, command);
        }
    }

    /// Between 50 and 256 (`NPC_AI_Jedi.c:1652-1817`): rush a gripped enemy; grip one
    /// throwing its saber; or run at one choking someone and try a random Force attack.
    fn jedi_tactical_force(&mut self, me: usize, enemy_dist: i32, command: &mut UserCommand) {
        let level_time = self.level_time;
        let enemy = self.actors[me].mind.enemy;
        let foe = enemy
            .and_then(|enemy| self.jedi_foe(enemy))
            .unwrap_or_default();
        if foe.client && foe.grip_being_gripped > level_time as f32 {
            // "They're being gripped, rush them!"
            self.jedi_rush(me, enemy_dist, foe, command);
            let npc = &self.actors[me];
            if npc.definition.rank >= rank::LT_JG
                && self.host.irand(0, 5) == 0
                && npc.player.force_powers_active() & (1 << FP_SPEED) == 0
                && npc.saber.event_flags & SEF_INWATER == 0
            {
                command.buttons |= BUTTON_ALT_ATTACK;
            }
        } else if foe.client
            && foe.saber_in_flight
            && foe.saber_entity_num != 0
            && self.actors[me]
                .player
                .raw_field(PS_WEAPON_TIME)
                .unwrap_or(0) as i32
                <= 0
            && self.force_power_available(me, FP_GRIP, 0)
            && self.host.irand(0, 10) == 0
            && self.host.irand(0, 6) < self.host.skill()
            && self.host.irand(rank::CIVILIAN, rank::CAPTAIN) < self.actors[me].definition.rank
        {
            // "They're throwing their saber, grip them!"
            self.jedi_grip_taunt(me);
            self.jedi_start_grip(me);
        } else {
            if foe.client && foe.force_powers_active & (1 << FP_GRIP) != 0 {
                // "They're choking someone, probably an ally, run at them"
                self.jedi_rush(me, enemy_dist, foe, command);
            }
            self.jedi_random_force(me, enemy_dist, foe, command);
        }
    }

    /// The grip's start: the "gripping" and "attackDelay" timers for three seconds (the
    /// grip itself is the Force button the timers hold down).
    fn jedi_start_grip(&mut self, me: usize) {
        let level_time = self.level_time;
        let timers = &mut self.actors[me].mind.timers;
        timers.set("gripping", level_time, 3_000);
        timers.set("attackDelay", level_time, 3_000);
    }

    /// At an enemy on the ground, not parrying (or ranked above lieutenant), when far away
    /// or allowed to use its saber (`NPC_AI_Jedi.c:1656-1666`, `1697-1707`).
    fn jedi_rush(&mut self, me: usize, enemy_dist: i32, foe: JediFoe, command: &mut UserCommand) {
        let npc = &self.actors[me];
        let parrying =
            !npc.mind.timers.done("parryTime", self.level_time) && npc.definition.rank <= rank::LT;
        if foe.ground != ENTITYNUM_NONE
            && !parrying
            && (enemy_dist > 200 || npc.script_flags & SCF_DONT_FIRE == 0)
        {
            self.jedi_advance(me, command);
        }
    }

    /// Now and then a Force attack by rank (`NPC_AI_Jedi.c:1708-1817`): pull, lightning,
    /// drain, grip or a saber throw; or, eager enough, at the enemy.
    fn jedi_random_force(
        &mut self,
        me: usize,
        enemy_dist: i32,
        foe: JediFoe,
        command: &mut UserCommand,
    ) {
        let npc = &self.actors[me];
        let yoda = npc.npc_type.eq_ignore_ascii_case(b"yoda");
        let rank = npc.definition.rank;
        let chance_scale = if npc.definition.client_class == CLASS_DESANN || yoda {
            1
        } else if rank == rank::ENSIGN {
            2
        } else if rank >= rank::LT_JG {
            5
        } else {
            0
        };
        let dont_fire = npc.script_flags & SCF_DONT_FIRE != 0;
        let attacks = chance_scale != 0
            && (enemy_dist > self.host.irand(100, 200)
                || dont_fire
                || (yoda && self.host.irand(0, 3) == 0))
            && enemy_dist < 500
            && (self.host.irand(0, chance_scale * 10) < 5
                || (foe.client && foe.weapon != WP_SABER && self.host.irand(0, chance_scale) == 0));
        if attacks {
            let npc = &self.actors[me];
            let (rank, weapon) = (npc.definition.rank, i32::from(npc.player.weapon()));
            if ((rank == rank::ENSIGN || rank > rank::LT_JG) && self.host.irand(0, 1) == 0)
                || weapon != WP_SABER
            {
                self.jedi_force_attack(me, enemy_dist, command);
            } else if rank >= rank::LT_JG && self.may_throw_saber(me) {
                command.buttons |= BUTTON_ALT_ATTACK;
            }
        } else if self.actors[me].definition.stats.aggression > 5
            && self.jedi_may_advance(me)
            && (enemy_dist > 200 || !dont_fire)
        {
            // "see if we should advance now"
            self.jedi_advance(me, command);
        }
    }

    /// Not under Force speed, its saber not in water: a saber throw may be asked for.
    fn may_throw_saber(&self, me: usize) -> bool {
        let npc = &self.actors[me];
        npc.player.force_powers_active() & (1 << FP_SPEED) == 0
            && npc.saber.event_flags & SEF_INWATER == 0
    }

    /// The Force attack itself (`NPC_AI_Jedi.c:1718-1792`): pull the enemy close (and
    /// maybe swing), lightning, a drain, a grip with a taunt, or else a saber throw.
    fn jedi_force_attack(&mut self, me: usize, enemy_dist: i32, command: &mut UserCommand) {
        let level_time = self.level_time;
        let skill = self.host.skill();
        if self.force_power_usable(me, FP_PULL) && self.host.irand(0, 2) == 0 {
            // "force pull the guy to me!"
            self.force_throw(me, true);
            self.actors[me]
                .mind
                .timers
                .set("duck", level_time, enemy_dist * 3);
            if self.host.irand(0, 1) != 0 {
                command.buttons |= BUTTON_ATTACK;
            }
            return;
        }
        let lightning = (self.force_power_usable(me, FP_LIGHTNING) && {
            let npc = &self.actors[me];
            npc.script_flags & SCF_DONT_FIRE != 0
                && !npc.npc_type.eq_ignore_ascii_case(b"cultist_lightning")
        }) || self.host.irand(0, 1) != 0;
        if lightning {
            self.force_lightning(me);
            if self.actors[me].force_levels[FP_LIGHTNING] > FORCE_LEVEL_1 {
                let time = self.host.irand(1_000, 3_000 + skill * 500);
                self.actors[me]
                    .player
                    .set_raw_field(PS_WEAPON_TIME, time as u32);
                self.actors[me]
                    .mind
                    .timers
                    .set("holdLightning", level_time, time);
            }
            let time = self.actors[me]
                .player
                .raw_field(PS_WEAPON_TIME)
                .unwrap_or(0) as i32;
            self.actors[me]
                .mind
                .timers
                .set("attackDelay", level_time, time);
            return;
        }
        let npc = &self.actors[me];
        let drain = ((npc.health as f32) < npc.player.stats[STAT_MAX_HEALTH] as i32 as f32 * 0.75
            && self.host.irand(0, npc.force_levels[FP_DRAIN]) > FORCE_LEVEL_1
            && self.force_power_usable(me, FP_DRAIN)
            && {
                let npc = &self.actors[me];
                npc.script_flags & SCF_DONT_FIRE != 0
                    && !npc.npc_type.eq_ignore_ascii_case(b"cultist_drain")
            })
            || self.host.irand(0, 1) != 0;
        if drain {
            self.force_drain(me);
            let time = self.host.irand(1_000, 3_000 + skill * 500);
            self.actors[me]
                .player
                .set_raw_field(PS_WEAPON_TIME, time as u32);
            let timers = &mut self.actors[me].mind.timers;
            timers.set("draining", level_time, time);
            timers.set("attackDelay", level_time, time);
            return;
        }
        let in_view = self.force_power_usable(me, FP_GRIP) && {
            let enemy = self.actors[me]
                .mind
                .enemy
                .and_then(|enemy| self.body(enemy));
            enemy.is_some_and(|enemy| crate::npc_senses::in_fov(&enemy, &self.npc(me), 20, 30))
        };
        if in_view {
            self.jedi_grip_taunt(me);
            self.jedi_start_grip(me);
        } else if self.force_power_usable(me, FP_SABER_THROW) && self.may_throw_saber(me) {
            command.buttons |= BUTTON_ALT_ATTACK;
        }
    }

    /// "if really really mad, rage!" (`NPC_AI_Jedi.c:1839-1852`).
    fn jedi_rage_when_mad(&mut self, me: usize) {
        let aggression = self.actors[me].definition.stats.aggression;
        if aggression > self.host.irand(5, 15)
            && {
                let npc = &self.actors[me];
                (npc.health as f32) < npc.max_health as f32 * 0.75
            }
            && self.host.irand(0, 2) == 0
            && self.could_start(me, FP_RAGE)
        {
            self.jedi_rage(me);
        }
    }
}
