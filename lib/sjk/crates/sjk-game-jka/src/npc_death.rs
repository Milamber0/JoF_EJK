//! An NPC dies: `player_die` for an NPC (`codemp/game/g_combat.c:2092-2918`, the branches
//! an `ET_NPC` takes) — its goal entity freed, its sabers going off, its `target` fired,
//! the killing blow's log line (`Kill:`, a named NPC's `targetname` in brackets; no
//! obituary event is sent for an NPC), the killer's score (`AddScore`, with the Jedi
//! Master's, the stun baton's and the excellent award's rules) and its victory
//! (`G_CheckVictoryScript`), the death animation (`G_PickDeathAnim`) and event, the
//! death's alert to its team (`G_DeathAlert`), its timers cleared, and its death effects
//! (`DeathFX`, `g_combat.c:1899-1996`). It keeps its box and its contents: the body goes
//! later ([`crate::npc_dead`]).
//!
//! Not here: vehicles and their riders (step 10), AI groups and combat points (step 5),
//! the rancor dropping what it holds (step 7), dismemberment
//! (the models'), scripts (`BSET_DEATH`, `BSET_FFDEATH`), and `Team_FragBonuses` (an NPC
//! carries no flag).
//!
//! Held to `tools/game-oracle/npccombat.c` (`game-npccombat.txt`).

use crate::event_entity::EventEntity;
use crate::npc_spawn::{NpcHost, es};
use crate::npc_world::NpcWorld;
use crate::player_death::DeathRequest;

/// `PM_NORMAL`, `PM_DEAD`; `PMF_STUCK_TO_WALL`.
const PM_NORMAL: u8 = 0;
const PM_DEAD: u8 = 5;
const PMF_STUCK_TO_WALL: u32 = 16_384;
/// Player-state wire fields `player_die` writes.
const PS_PM_FLAGS: usize = 38;
const PS_ROCKET_LOCK_INDEX: usize = 24;
const PS_ROCKET_TARGET_TIME: usize = 71;
const PS_ROCKET_LOCK_TIME: usize = 79;
const PS_SABER_ENTITY: usize = 31;
const PS_SABER_HOLSTERED: usize = 81;
const PS_SABER_IN_FLIGHT: usize = 88;
const PS_ZOOM_MODE: usize = 90;
const PS_HACKING_TIME: usize = 91;
const PS_BROKEN_LIMBS: usize = 93;
const PS_EMPLACED_INDEX: usize = 112;
const PS_HELD_BY_CLIENT: usize = 121;
/// Entity wire fields it clears.
const ES_POWERUPS: usize = 77;
const ES_LOOP_SOUND: usize = 55;
const ES_LOOP_IS_SOUNDSET: usize = 70;
/// `STAT_HOLDABLE_ITEMS`, `STAT_HOLDABLE_ITEM`; persistants.
const STAT_HOLDABLE_ITEM: usize = 1;
const STAT_HOLDABLE_ITEMS: usize = 2;
const PERS_SCORE: usize = 0;
const PERS_PLAYEREVENTS: usize = 5;
const PERS_EXCELLENT_COUNT: usize = 10;
const PERS_KILLED: usize = 8;
const PERS_GAUNTLET_FRAG_COUNT: usize = 13;
const PLAYEREVENT_GAUNTLETREWARD: u32 = 0x2;
/// `EV_GENERAL_SOUND`, `EV_MUTE_SOUND`, `EV_PLAY_EFFECT_ID`, `EV_DEATH1`; `CHAN_WEAPON`.
const EV_GENERAL_SOUND: u32 = 76;
const EV_MUTE_SOUND: u32 = 74;
const EV_PLAY_EFFECT_ID: u32 = 69;
const EV_DEATH1: u32 = 90;
const CHAN_WEAPON: u32 = 2;
/// `GIB_HEALTH`.
const GIB_HEALTH: i32 = -40;
/// `WP_SABER`.
const WP_SABER: u8 = 3;
/// `MAX_CLIENTS`.
const MAX_CLIENTS: u16 = 32;
/// `MOD_STUN_BATON`; `CARNAGE_REWARD_TIME`.
const MOD_STUN_BATON: u32 = 1;
const CARNAGE_REWARD_TIME: i32 = 3_000;
/// `GT_JEDIMASTER`, `GT_TEAM`.
const GT_JEDIMASTER: i32 = 2;
const GT_TEAM: i32 = 6;
/// `SCF_FFDEATH`.
const SCF_FFDEATH: u32 = 0x100;
/// `class_t`s the death reads.
const CLASS_MARK1: i32 = 23;
const CLASS_GALAKMECH: i32 = 25;
const CLASS_VEHICLE: i32 = 53;
/// `DEATH_ALERT_RADIUS`, `DEATH_ALERT_SOUND_RADIUS`.
const DEATH_ALERT_RADIUS: f32 = 512.0;
const DEATH_ALERT_SOUND_RADIUS: f32 = 512.0;
/// `MOD_CRUSH`, `MOD_FALLING`, `MOD_UNKNOWN`, `MOD_TRIGGER_HURT`: deaths put down to whoever
/// last pushed.
const PUSHED_DEATHS: [u32; 4] = [36, 38, 0, 41];

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `player_die(npc, inflictor, attacker, damage, meansOfDeath)` for the NPC at `me`;
    /// `attacker` is `ENTITYNUM_WORLD` for none.
    pub fn die(&mut self, me: usize, attacker: u16, damage: i32, means: u32) {
        let level_time = self.level_time;
        if self.actors[me].player.movement_type() == PM_DEAD || self.host.intermission() {
            return;
        }
        // A vehicle with no death delay kills everyone aboard (`g_combat.c:2127-2233`).
        self.kill_everyone_aboard(me, attacker);
        let npc = &mut self.actors[me];
        let number = npc.number;
        npc.player.set_raw_field(PS_EMPLACED_INDEX, 0);
        // `G_BreakArm(self, 0)` mends a humanoid's arms (never a vehicle's: its bits are its
        // damaged surfaces).
        if npc.humanoid && npc.vehicle.is_none() {
            npc.player.set_raw_field(PS_BROKEN_LIMBS, 0);
        }
        npc.player
            .set_raw_field(PS_SABER_ENTITY, u32::from(npc.saber_entity.unwrap_or(0)));
        // Off the vehicle it rides (a droid unit's), `g_combat.c:2256-2298`.
        self.leave_ridden_vehicle(me);
        // Its combat point let go, its squad told and left (`g_combat.c:2309-2313`).
        let point = self.actors[me].mind.tactics.combat_point;
        self.free_combat_point(me, point, false);
        if self.actors[me].mind.tactics.group.is_some() {
            self.group_member_killed(me);
            self.delete_from_group(me);
        }
        let npc = &mut self.actors[me];
        if let Some(goal) = npc.goal.take() {
            self.host.free(goal);
        }
        // A flying Boba Fett's jets go out (`g_combat.c:2344-2345`).
        if self.actors[me].definition.client_class == crate::npc_boba::CLASS_BOBAFETT
            && self.boba_flying(me)
        {
            self.boba_fly_stop(me);
        }
        // A dying rancor lets go of its victim (`g_combat.c:2346-2347`).
        if self.actors[me].definition.entity_class == crate::npc_rancor::CLASS_RANCOR {
            self.rancor_drop_victim(me);
        }
        self.forget_victim(attacker, number);
        self.sabers_off(me);
        // `G_UseTargets(self, self)`: its `target`.
        if let Some(target) = self.actors[me]
            .target
            .clone()
            .filter(|target| !target.is_empty())
        {
            self.fired.push(target);
        }
        let npc = &mut self.actors[me];
        npc.player.set_raw_field(PS_HELD_BY_CLIENT, 0);
        // `BG_ClearRocketLock`.
        npc.player.set_raw_field(
            PS_ROCKET_LOCK_INDEX,
            u32::from(crate::npc_spawn::ENTITYNUM_NONE),
        );
        npc.player
            .set_raw_field(PS_ROCKET_LOCK_TIME, (-1.0_f32).to_bits());
        npc.player.set_raw_field(PS_ROCKET_TARGET_TIME, 0);
        npc.player.set_raw_field(PS_HACKING_TIME, 0);
        // `G_MuteSound(self->s.number, CHAN_WEAPON)`.
        let mute = EventEntity {
            event: EV_MUTE_SOUND,
            parameter: 0,
            origin: [0.0; 3],
            client: None,
            broadcast: true,
            extra: [(0, 0); 12],
        }
        .muting(number, CHAN_WEAPON);
        self.host.raise(mute);
        // A crush, a fall, a trigger or nothing, by itself or by no client: put down to
        // whoever last pushed it, while that counts.
        let mut attacker = attacker;
        let by_a_client = self.body(attacker).is_some();
        let other_killer = self.actors[me].mind.fight.other_killer;
        if (attacker == number || !by_a_client)
            && PUSHED_DEATHS.contains(&means)
            && other_killer.time > level_time
        {
            attacker = other_killer.number;
        }
        let npc = &mut self.actors[me];
        npc.player.set_movement_type(PM_DEAD);
        npc.player.set_raw_field(
            PS_PM_FLAGS,
            npc.player.raw_field(PS_PM_FLAGS).unwrap_or(0) & !PMF_STUCK_TO_WALL,
        );
        self.log_death(me, attacker, means);
        let npc = &mut self.actors[me];
        npc.mind.enemy = Some(attacker);
        npc.player.persistent[PERS_KILLED] = npc.player.persistent[PERS_KILLED].wrapping_add(1);
        self.score_death(me, attacker, means);
        let npc = &mut self.actors[me];
        npc.state.set_raw_field(es::WEAPON, 0);
        npc.state.set_raw_field(ES_POWERUPS, 0);
        npc.player.set_raw_field(PS_ZOOM_MODE, 0);
        npc.state.set_raw_field(ES_LOOP_SOUND, 0);
        npc.state.set_raw_field(ES_LOOP_IS_SOUNDSET, 0);
        npc.mind.fight.respawn_time = level_time + 1_700;
        npc.player.powerups = [0; 16];
        npc.player.stats[STAT_HOLDABLE_ITEMS] = 0;
        npc.player.stats[STAT_HOLDABLE_ITEM] = 0;
        self.death_animation(me, attacker, damage, means);
        // `G_AddEvent(self, EV_DEATH1 + deathAnim, wasJediMaster)`, the counter shared with
        // the players' deaths.
        let event = EV_DEATH1 + u32::from(*self.host.death_counter());
        self.add_event(me, event, 0);
        if attacker != number {
            let victim = self.npc(me);
            if let Some(attacker) = self.body(attacker) {
                self.alert_team(
                    &victim,
                    Some(me),
                    &attacker,
                    DEATH_ALERT_RADIUS,
                    DEATH_ALERT_SOUND_RADIUS,
                );
            }
        }
        let counter = self.host.death_counter();
        *counter = (*counter + 1) % 3;
        let npc = &mut self.actors[me];
        npc.mind.next_bstate_think = level_time;
        if npc.script_flags & SCF_FFDEATH != 0
            && let Some(target) = npc.target4.clone().filter(|target| !target.is_empty())
        {
            self.fired.push(target);
        }
        let npc = &mut self.actors[me];
        // `TIMER_Clear2`.
        npc.mind.timers = crate::npc_mind::NpcTimers::default();
        npc.mind.fight.time_of_death = level_time;
        self.death_effects(me);
    }

    /// `player_die`'s `attacker->NPC->group->enemy = NULL` (`g_combat.c:2349-2352`): an NPC
    /// killer's squad no longer after the dead.
    pub(crate) fn forget_victim(&mut self, attacker: u16, victim: u16) {
        let Some(group) = self
            .actor_at(attacker)
            .and_then(|at| self.actors[at].mind.tactics.group)
        else {
            return;
        };
        if self.level.groups[group].enemy == Some(victim) {
            self.level.groups[group].enemy = None;
        }
    }

    /// `player_die`'s sabers going off (`g_combat.c:2355-2371`): each lit blade's off sound,
    /// the first unless it is thrown.
    fn sabers_off(&mut self, me: usize) {
        let npc = &self.actors[me];
        let field = |index: usize| npc.player.raw_field(index).unwrap_or(0);
        if npc.player.weapon() != WP_SABER
            || field(PS_SABER_HOLSTERED) != 0
            || field(PS_SABER_ENTITY) == 0
        {
            return;
        }
        let [first, second] = &npc.definition.sabers;
        let origin = npc.current_origin;
        let mut sounds = [None, None];
        if field(PS_SABER_IN_FLIGHT) == 0 && first.sound_off != 0 {
            sounds[0] = Some(first.sound_off);
        }
        if second.sound_off != 0 && !second.model.is_empty() {
            sounds[1] = Some(second.sound_off);
        }
        for sound in sounds.into_iter().flatten() {
            self.host.raise(EventEntity {
                event: EV_GENERAL_SOUND,
                parameter: u32::from(sound),
                origin,
                client: None,
                broadcast: false,
                extra: [(0, 0); 12],
            });
        }
    }

    /// `player_die`'s log line (`g_combat.c:2445-2469`): the killer a client's number and
    /// name, `<world>` for anything else (an NPC too); the NPC by its type, and its
    /// `targetname` in brackets when it has one.
    fn log_death(&mut self, me: usize, attacker: u16, means: u32) {
        let npc = &self.actors[me];
        let victim = match &npc.targetname {
            Some(name) => [&npc.npc_type[..], b" (", name, b")"].concat(),
            None => npc.npc_type.clone(),
        };
        let killer = (attacker < MAX_CLIENTS && self.body(attacker).is_some())
            .then(|| (i32::from(attacker), self.host.client_name(attacker)));
        let line = crate::game_log::kill(
            killer
                .as_ref()
                .map(|(number, name)| (*number, name.as_slice())),
            usize::from(npc.number),
            &victim,
            means,
        );
        self.host.log(&line);
    }

    /// `player_die`'s scoring (`g_combat.c:2515-2640`) of an NPC's death.
    fn score_death(&mut self, me: usize, attacker: u16, means: u32) {
        let number = self.actors[me].number;
        let Some(killer) = self.body(attacker) else {
            // By the world: a point off the NPC's own score.
            self.add_score(number, -1);
            return;
        };
        self.victory(killer);
        if attacker == number || self.on_same_team(me, &killer) {
            self.add_score(attacker, -1);
            return;
        }
        if self.host.gametype() == GT_JEDIMASTER {
            // Only the master scores: the NPC is no master, so the killer if it is one,
            // else the master for a death between others.
            match self.host.jedi_master() {
                Some(master) if master == attacker => self.add_score(attacker, 1),
                Some(master) => self.add_score(master, 1),
                None => {}
            }
        } else {
            self.add_score(attacker, 1);
        }
        let level_time = self.level_time;
        if means == MOD_STUN_BATON {
            // The humiliation, on the killer (below) and on the NPC.
            let npc = &mut self.actors[me];
            npc.player.persistent[PERS_PLAYEREVENTS] ^= PLAYEREVENT_GAUNTLETREWARD;
        }
        match self.actor_at(attacker) {
            Some(at) => {
                let killer = &mut self.actors[at];
                if means == MOD_STUN_BATON {
                    killer.player.persistent[PERS_GAUNTLET_FRAG_COUNT] =
                        killer.player.persistent[PERS_GAUNTLET_FRAG_COUNT].wrapping_add(1);
                }
                if level_time - killer.mind.fight.last_kill_time < CARNAGE_REWARD_TIME {
                    killer.player.persistent[PERS_EXCELLENT_COUNT] =
                        killer.player.persistent[PERS_EXCELLENT_COUNT].wrapping_add(1);
                }
                killer.mind.fight.last_kill_time = level_time;
            }
            None => self.host.credit_kill(attacker, means),
        }
    }

    /// `OnSameTeam(npc, other)` (`g_team.c:206-288`): never outside team games (power duel
    /// by the duel team, which an NPC has none of); two NPCs of the same session team,
    /// neither free; never an NPC and a player.
    pub(crate) fn on_same_team(&self, me: usize, other: &crate::npc_senses::Body) -> bool {
        const GT_POWERDUEL: i32 = 4;
        let gametype = self.host.gametype();
        if gametype == GT_POWERDUEL {
            // An NPC's duel team is none, as is a player's who is not in the duel.
            return other.npc;
        }
        if gametype < GT_TEAM || !other.npc {
            return false;
        }
        let mine = self.actors[me].session_team;
        mine == other.session_team && !(mine == 0 && other.session_team == 0)
    }

    /// `AddScore(ent, origin, score)` (`g_combat.c:456-479`): for an NPC its own score (and
    /// its team's in a team game, by the player team it counts as), for a player the
    /// host's; nothing during the warm-up.
    pub(crate) fn add_score(&mut self, number: u16, points: i32) {
        if self.host.warmup() {
            return;
        }
        match self.actor_at(number) {
            Some(at) => {
                let npc = &mut self.actors[at];
                npc.player.persistent[PERS_SCORE] =
                    (npc.player.persistent[PERS_SCORE] as i32 + points) as u32;
                let team = npc.player.persistent[crate::npc_spawn::PERS_TEAM] as i32;
                self.host.npc_scored(team, points);
            }
            None => self.host.add_player_score(number, points),
        }
    }

    /// `G_CheckVictoryScript` (`g_combat.c:2000-2028`) for a killer NPC: a Jedi gets ready
    /// to taunt, Galak's mech to gloat, anyone else says something in a while.
    pub(crate) fn victory(&mut self, killer: crate::npc_senses::Body) {
        let Some(at) = self.actor_at(killer.number) else {
            return;
        };
        let level_time = self.level_time;
        if self.actors[at].player.weapon() == WP_SABER {
            self.actors[at].mind.blocked_speech_until = 0;
        } else if self.actors[at].definition.client_class == CLASS_GALAKMECH {
            let gloat = self.host.irand(5_000, 8_000);
            let npc = &mut self.actors[at];
            npc.wait = 1.0;
            npc.mind.timers.set("gloatTime", level_time, gloat);
            npc.mind.blocked_speech_until = 0;
        } else {
            // Sometimes the group's commander speaks instead (`g_combat.c:2019-2027`).
            let rank = self.actors[at].definition.rank;
            let commander = self.actors[at]
                .mind
                .tactics
                .group
                .and_then(|group| self.level.groups[group].commander)
                .and_then(|number| self.actor_at(number));
            let speaker = match commander {
                Some(commander)
                    if self.actors[commander].definition.rank > rank
                        && self.host.irand(0, 2) == 0 =>
                {
                    commander
                }
                _ => at,
            };
            let delay = self.host.irand(2_000, 5_000);
            self.actors[speaker].mind.greeting_debounce_time = level_time + delay;
        }
    }

    /// `G_PickDeathAnim` and its setting (`g_combat.c:2744-2790`): the pose by where the
    /// blow landed, held on both halves, and the respawn time a second on — or, where the
    /// skeleton has no death animation at all, the NPC freed now (not the Mark I).
    fn death_animation(&mut self, me: usize, attacker: u16, damage: i32, means: u32) {
        let level_time = self.level_time;
        let Self { actors, host, .. } = self;
        let npc = &actors[me];
        let request = DeathRequest {
            damage,
            means,
            point: npc.mind.fight.death_point,
            bounds: npc.link,
            ..DeathRequest::suicide(
                level_time,
                npc.number,
                npc.current_origin,
                [0, 0],
                npc.health,
            )
        };
        let yaw = npc.mind.current_angles[1];
        let Some(lengths) = npc.movement.animation_lengths() else {
            return;
        };
        let animation =
            crate::death_animation::pick(host.rng(), lengths, &npc.player, &request, yaw);
        match animation {
            Some(animation) if animation >= 1 => {
                let npc = &mut self.actors[me];
                if npc.health <= GIB_HEALTH {
                    npc.health = GIB_HEALTH + 1;
                }
                npc.mind.fight.respawn_time = level_time + 1_000;
                npc.player.set_movement_type(PM_NORMAL);
                use crate::pmove_anim::{
                    SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_FLAG_RESTART,
                };
                self.set_animation(
                    me,
                    SETANIM_BOTH,
                    animation,
                    SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD | SETANIM_FLAG_RESTART,
                );
                self.actors[me].player.set_movement_type(PM_DEAD);
                // A blade's (or a heavy melee's) kill: the death pose given to the model, then
                // a limb perhaps cut off (`g_combat.c:2788-2792`).
                if self.cuts_limbs(means, attacker) {
                    let NpcWorld { actors, host, .. } = &mut *self;
                    host.update_npc_anims(&actors[me], level_time);
                    let npc = &self.actors[me];
                    let check = crate::npc_dismember_check::DismemberCheck {
                        victim: npc.number,
                        enemy: attacker,
                        point: npc.mind.fight.death_point,
                        damage,
                        death_anim: animation,
                        post_death: false,
                        avoid: self.level.avoid_dismember,
                    };
                    self.check_for_dismemberment(check);
                }
            }
            // "Some droids don't have death anims": no animation, or animation 0.
            _ => {
                let npc = &mut self.actors[me];
                if npc.definition.client_class != CLASS_MARK1
                    && npc.definition.client_class != CLASS_VEHICLE
                {
                    npc.think = crate::npc_spawn::NpcThink::Free(level_time);
                }
            }
        }
    }

    /// `DeathFX` (`g_combat.c:1899-1996`): the droids' explosions and death sounds.
    fn death_effects(&mut self, me: usize) {
        let npc = &self.actors[me];
        let origin = npc.current_origin;
        let below = |depth: f32| [origin[0], origin[1], origin[2] - depth];
        let (_, right) = crate::pmove::flight::flight_axes(npc.mind.current_angles);
        let right = right.to_array();
        let along = |from: [f32; 3], scale: f32| -> [f32; 3] {
            std::array::from_fn(|axis| from[axis] + scale * right[axis])
        };
        match npc.definition.client_class {
            // CLASS_MOUSE.
            29 => {
                self.play_effect(b"env/small_explode", below(20.0));
                self.death_sound(me, b"sound/chars/mouse/misc/death1");
            }
            // CLASS_PROBE.
            32 => self.play_effect(
                b"explosions/probeexplosion1",
                [origin[0], origin[1], origin[2] + 50.0],
            ),
            // CLASS_ATST.
            1 => {
                let mut at = along(origin, 20.0);
                at[2] += 180.0;
                self.play_effect(b"explosions/droidexplosion1", at);
                self.play_effect(b"explosions/droidexplosion1", along(at, -40.0));
            }
            // CLASS_SEEKER, CLASS_REMOTE.
            41 | 39 => self.play_effect(b"env/small_explode", origin),
            // CLASS_GONK.
            11 => {
                let at = below(5.0);
                let which = self.host.irand(1, 3);
                self.death_sound(
                    me,
                    format!("sound/chars/gonk/misc/death{which}.wav").as_bytes(),
                );
                self.play_effect(b"env/med_explode", at);
            }
            // CLASS_R2D2, CLASS_PROTOCOL, CLASS_R5D2.
            34 | 33 | 35 => {
                self.play_effect(b"env/med_explode", below(10.0));
                self.death_sound(me, b"sound/chars/mark2/misc/mark2_explo");
            }
            // CLASS_MARK2.
            24 => {
                self.play_effect(b"explosions/droidexplosion1", below(15.0));
                self.death_sound(me, b"sound/chars/mark2/misc/mark2_explo");
            }
            // CLASS_INTERROGATOR.
            16 => {
                self.play_effect(b"explosions/droidexplosion1", below(15.0));
                self.death_sound(me, b"sound/chars/interrogator/misc/int_droid_explo");
            }
            CLASS_MARK1 => {
                let mut at = along(origin, 10.0);
                at[2] -= 15.0;
                self.play_effect(b"explosions/droidexplosion1", at);
                let at = along(at, -20.0);
                self.play_effect(b"explosions/droidexplosion1", at);
                self.play_effect(b"explosions/droidexplosion1", along(at, -20.0));
                self.death_sound(me, b"sound/chars/mark1/misc/mark1_explo");
            }
            // CLASS_SENTRY.
            42 => {
                self.death_sound(me, b"sound/chars/sentry/misc/sentry_explo");
                self.play_effect(b"env/med_explode", origin);
            }
            _ => {}
        }
    }

    /// `G_PlayEffectID(G_EffectIndex(name), at, (0, 0, 1))` (`g_utils.c:1271-1288`).
    fn play_effect(&mut self, name: &[u8], at: [f32; 3]) {
        let effect = self.host.effect_index(name);
        let mut event = EventEntity {
            event: EV_PLAY_EFFECT_ID,
            parameter: u32::from(effect),
            origin: at,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        for axis in 0..3 {
            event.extra[axis] = (es::ORIGIN[axis], at[axis].to_bits());
        }
        event.extra[3] = (es::ANGLES[2], 1.0_f32.to_bits());
        self.host.raise(event);
    }

    /// `G_Sound(npc, CHAN_AUTO, G_SoundIndex(name))`.
    fn death_sound(&mut self, me: usize, name: &[u8]) {
        let sound = self.host.sound_index(name);
        let origin = self.actors[me].current_origin;
        self.host.raise(EventEntity {
            event: EV_GENERAL_SOUND,
            parameter: u32::from(sound),
            origin,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        });
    }
}

/// `player_die` for a player an NPC killed (`g_combat.c:2515-2600`): the NPC's victory,
/// its point — in a Jedi Master game only for the master's death, else the master's
/// point — the stun baton's count and the excellent award on it. The player's own side
/// of the death is the host's.
pub fn npc_killed_player<H: NpcHost>(
    world: &mut NpcWorld<'_, H>,
    killer: u16,
    means: u32,
    victim_was_master: bool,
) {
    let Some(body) = world.body(killer) else {
        return;
    };
    world.victory(body);
    if world.host.gametype() == GT_JEDIMASTER && !victim_was_master {
        if let Some(master) = world.host.jedi_master() {
            world.add_score(master, 1);
        }
    } else {
        world.add_score(killer, 1);
    }
    let level_time = world.level_time;
    let Some(at) = world.actor_at(killer) else {
        return;
    };
    let npc = &mut world.actors[at];
    if means == MOD_STUN_BATON {
        npc.player.persistent[PERS_GAUNTLET_FRAG_COUNT] =
            npc.player.persistent[PERS_GAUNTLET_FRAG_COUNT].wrapping_add(1);
    }
    if level_time - npc.mind.fight.last_kill_time < CARNAGE_REWARD_TIME {
        npc.player.persistent[PERS_EXCELLENT_COUNT] =
            npc.player.persistent[PERS_EXCELLENT_COUNT].wrapping_add(1);
    }
    npc.mind.fight.last_kill_time = level_time;
}
