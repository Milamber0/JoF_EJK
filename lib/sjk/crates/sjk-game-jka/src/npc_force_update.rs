//! `WP_ForcePowersUpdate` for an NPC (`g_main.c:3333-3346`, `w_force.c:4969-5480`): run at
//! its turn in `G_RunFrame` with its last command (`pers.cmd`), before its think — the
//! same update a player's is ([`crate::force_powers`], [`crate::knockdown`]), over the
//! NPC's own records: its player state, its Force data, its health and its knockdown
//! memory.
//!
//! What its powers reach beyond the NPCs — the players, the entity pool its sounds and
//! events go to — is the host's ([`NpcHost::player_force`], [`PlayerForce`]). The other
//! NPCs are reached as players are ([`NpcOthers`]): grip, lightning and drain treat a
//! client alike, whoever runs it — a player's powers too.

use crate::damage::DamageRequest;
use crate::entity_pool::EntityPool;
use crate::force_dark::{OtherPlayer, OtherPlayers};
use crate::force_powers::ForceFrame;
use crate::force_throw::{Candidate, ThrowPlayer};
use crate::npc_spawn::{NpcActor, NpcHost};
use crate::npc_world::NpcWorld;
use crate::player_death::Rng;
use crate::pmove::MovementTrace;

/// `g_forceRegenTime`'s default: a point every 200 ms.
const FORCE_REGEN_TIME: i32 = 200;
/// `ps.genericEnemyIndex`; `EF_SEEKERDRONE` (`ps.eFlags`).
const PS_GENERIC_ENEMY: usize = 26;
const PS_EFLAGS: usize = 17;
const EF_SEEKERDRONE: u32 = 1 << 21;
/// `ps.fd.saberAnimLevel`.
const PS_SABER_ANIM_LEVEL: usize = 23;
/// `TEAM_SPECTATOR`.
const TEAM_SPECTATOR: i32 = 3;

/// The players as a Force power in the NPCs' world reaches them: the host's, handed out by
/// [`NpcHost::player_force`]. The NPCs, the traces, the visibility and the generator are
/// the roster's and the [`NpcHost`]'s own.
pub trait PlayerForce {
    /// A player in the game (a corpse included), one at a time, as grip, lightning,
    /// drain and the mind trick reach it.
    fn force_player(&mut self, number: u16) -> Option<OtherPlayer<'_>>;
    /// A player as a push or pull reads and changes it.
    fn force_throw_player(&mut self, number: u16) -> Option<ThrowPlayer<'_>>;
    /// `G_Damage` on player `victim` by a power (lightning, drain, a grip's squeeze).
    /// Returns what the attacker's hit counter gains (`PERS_HITS`, `PERS_ATTACKEE_ARMOR`,
    /// `g_combat.c:4895-4905`).
    fn force_damage(&mut self, victim: u16, request: DamageRequest) -> (i32, Option<u32>);
    /// `trap->EntitiesInBox` for everything but the NPCs (the players, missiles and the
    /// rest), in entity number order, appended to `out`.
    fn force_entities_in_box(&mut self, mins: [f32; 3], maxs: [f32; 3], out: &mut Vec<Candidate>);
    /// `G_ReflectMissile(thrower, missile, forward)`: a missile pushed by a thrower at `origin`.
    fn force_reflect_missile(
        &mut self,
        missile: u16,
        thrower: u16,
        origin: [f32; 3],
        forward: [f32; 3],
    );
    /// `TossClientWeapon` on player `victim`, a pull's.
    fn force_toss_weapon(&mut self, victim: u16, direction: [f32; 3], speed: f32);
    /// The entities the powers' sounds, trackers and events become.
    fn force_pool(&mut self) -> &mut EntityPool;
    /// A sound tracker freed: every client is told (`kls <player> <tracker>`).
    fn force_loop_stopped(&mut self, player: u16, tracker: u16);
    /// `g_TimeSinceLastFrame`.
    fn force_since_last_frame(&self) -> i32;
    /// A power's use begins (an update, a push, a power the AI uses directly).
    fn force_begin(&mut self) {}
    /// It is over: what the host defers to then (the weapons a pull tore away, the
    /// movement of the players it changed) is done.
    fn force_end(&mut self) {}
}

/// Everyone but a Force power's user in the NPCs' world: the NPCs (the roster's records;
/// an NPC user's own record stands as it was when the power began) and the players (the
/// host's [`PlayerForce`]). A blow on an NPC is dealt at once, as `G_Damage` is — its pain
/// draws from the generator before the power goes on.
pub struct NpcOthers<'w, 'a, H: NpcHost> {
    /// The NPCs and the host.
    pub world: &'w mut NpcWorld<'a, H>,
    /// The user's entity number.
    pub user: u16,
    /// What the user's hit counter gained from its powers' blows (`PERS_HITS`, and the
    /// last `PERS_ATTACKEE_ARMOR`), for its own state once they are done
    /// ([`NpcOthers::credit`]).
    pub hits: Option<(i32, Option<u32>)>,
}

impl<'w, 'a, H: NpcHost> NpcOthers<'w, 'a, H> {
    /// Everyone but `user`, in `world`.
    pub fn new(world: &'w mut NpcWorld<'a, H>, user: u16) -> Self {
        Self {
            world,
            user,
            hits: None,
        }
    }

    /// The hits the powers' blows counted, given to the user's `state`.
    pub fn credit(&self, state: &mut sjk_protocol::PlayerState) {
        const PERS_HITS: usize = 1;
        const PERS_ATTACKEE_ARMOR: usize = 7;
        if let Some((hits, armor)) = self.hits {
            state.persistent[PERS_HITS] = (state.persistent[PERS_HITS] as i32 + hits) as u32;
            state.persistent[PERS_ATTACKEE_ARMOR] = armor.unwrap_or(0);
        }
    }

    fn count(&mut self, (hits, armor): (i32, Option<u32>)) {
        if hits != 0 {
            let before = self.hits.map_or(0, |(before, _)| before);
            self.hits = Some((before + hits, armor));
        }
    }
}

/// An NPC as a Force power reaches it.
pub fn npc_as_other(npc: &mut NpcActor) -> OtherPlayer<'_> {
    OtherPlayer {
        state: &mut npc.player,
        force: &mut npc.force,
        health: &mut npc.health,
        knockdown: &mut npc.mind.knockdown,
        other_killer: &mut npc.mind.fight.other_killer,
        team: npc.session_team,
        npc_class: Some(npc.definition.client_class),
        absmin: npc.link.0,
        absmax: npc.link.1,
    }
}

impl<H: NpcHost> NpcOthers<'_, '_, H> {
    fn players(&mut self) -> &mut dyn PlayerForce {
        self.world
            .host
            .player_force()
            .expect("a host running the NPCs' Force")
    }
}

impl<H: NpcHost> OtherPlayers for NpcOthers<'_, '_, H> {
    fn slots(&self) -> u16 {
        let npcs = self
            .world
            .actors
            .iter()
            .map(|npc| npc.number + 1)
            .max()
            .unwrap_or(0);
        npcs.max(self.world.host.client_slots())
    }

    fn player(&mut self, number: u16) -> Option<OtherPlayer<'_>> {
        if number == self.user {
            return None;
        }
        match self.world.actor_at(number) {
            // A begun NPC, standing or a corpse (`inuse` and a client).
            Some(at) => {
                let npc = &mut self.world.actors[at];
                npc.begun().then(|| npc_as_other(npc))
            }
            None => self.players().force_player(number),
        }
    }

    fn trace(&mut self, start: [f32; 3], end: [f32; 3], pass: u16, mask: u32) -> MovementTrace {
        let NpcWorld {
            actors,
            bodies,
            host,
            ..
        } = &mut *self.world;
        bodies.clear();
        bodies.extend(
            actors
                .iter()
                .filter(|npc| npc.contents != 0 && npc.number != pass)
                .map(NpcActor::body),
        );
        host.trace(start, [0.0; 3], [0.0; 3], end, pass, mask, bodies)
    }

    fn in_pvs(&mut self, from: [f32; 3], to: [f32; 3]) -> bool {
        self.world.host.in_pvs(from, to)
    }

    fn damage(&mut self, victim: u16, request: DamageRequest) {
        let hits = if self.world.actor_at(victim).is_some() {
            let damaged = self.world.force_blow(victim, request);
            (damaged.attacker_hits, damaged.attackee_armor)
        } else {
            self.players().force_damage(victim, request)
        };
        // The user is the attacker of every blow its powers deal.
        self.count(hits);
    }

    fn rng(&mut self) -> &mut Rng {
        self.world.host.rng()
    }

    fn pool(&mut self) -> &mut EntityPool {
        self.players().force_pool()
    }

    fn sound_index(&mut self, name: &[u8]) -> u16 {
        self.world.host.sound_index(name)
    }

    fn loop_stopped(&mut self, player: u16, tracker: u16) {
        self.players().force_loop_stopped(player, tracker);
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `WP_ForcePowersUpdate` for the NPC at `me` (not a spectator): the knockdown's lying
    /// and get-up, `SeekerDroneUpdate` (no NPC has a seeker), then the powers' update with
    /// its last command.
    pub fn force_update(&mut self, me: usize) {
        if self.actors[me].session_team == TEAM_SPECTATOR {
            return;
        }
        self.knockdown_update(me);
        let npc = &mut self.actors[me];
        if npc.player.raw_field(PS_EFLAGS).unwrap_or(0) & EF_SEEKERDRONE == 0 {
            npc.player.set_raw_field(PS_GENERIC_ENEMY, u32::MAX);
        }
        let jump_sound = std::mem::take(&mut npc.mind.force_jump_sound);
        let command = npc.mind.command;
        // The movement restarts from the player state at its next move.
        let begun = self.with_force_frame(me, |state, force, frame| {
            crate::force_powers::begin(state, force, &command, jump_sound, frame)
        });
        let Some(begun) = begun else { return };
        // The reference keeps one table of levels (`ps.fd.forcePowerLevel`): what the update
        // gave it — "everyone gets 1st level jump" — is the NPC's level from now on, its
        // begin's `WP_SpawnInitForcePowers` and its moves included.
        let npc = &mut self.actors[me];
        for (level, updated) in npc.force_levels.iter_mut().zip(npc.force.levels) {
            *level = i32::from(updated);
        }
        // `sess.saberLevel` follows the style (`w_force.c:5027-5038`): an NPC is no bot, and
        // its `updateUITime` is never set.
        npc.mind.session_saber_level =
            npc.player.raw_field(PS_SABER_ANIM_LEVEL).unwrap_or(0) as i32;
        if let Some(pull) = begun.throw {
            self.force_throw(me, pull);
        }
        let _ = self.with_force_frame(me, |state, force, frame| {
            crate::force_powers::finish(state, force, begun, frame)
        });
    }

    /// `run` with the NPC at `me`'s player state, its Force and a [`ForceFrame`] over
    /// everyone else ([`NpcOthers`]) — its powers' own frame, in an update or used directly
    /// by its AI. The NPC's own record stands as it was for the others to read while the
    /// powers run on a copy, which then becomes it. `None` where the host runs no NPC's
    /// Force.
    pub(crate) fn with_force_frame<T>(
        &mut self,
        me: usize,
        run: impl FnOnce(
            &mut sjk_protocol::PlayerState,
            &mut crate::force_powers::ForcePowers,
            &mut ForceFrame,
        ) -> T,
    ) -> Option<T> {
        let since_last_frame = self.host.player_force()?.force_since_last_frame();
        let (level_time, gametype) = (self.level_time, self.host.gametype());
        let npc = &self.actors[me];
        let mut state = self
            .spare_state
            .take()
            .unwrap_or_else(sjk_protocol::PlayerState::zero);
        state.copy_from(&npc.player);
        let mut force = npc.force.clone();
        let (mut health, mut hand_extend_time, number, team) = (
            npc.health,
            npc.mind.knockdown.hand_extend_time,
            npc.number,
            npc.session_team,
        );
        let mut invulnerable = 0;
        let saber_off_sounds = crate::npc_jedi_glue::saber_off_sounds(npc);
        self.host.player_force()?.force_begin();
        let out = {
            let mut others = NpcOthers::new(&mut *self, number);
            let mut frame = ForceFrame {
                level_time,
                gametype,
                regen_time: FORCE_REGEN_TIME,
                since_last_frame,
                client: number,
                npc: true,
                team,
                saber_style: 1,
                saber_only: false,
                lone_regen: None,
                health: &mut health,
                hand_extend_time: &mut hand_extend_time,
                invulnerable_until: &mut invulnerable,
                others: &mut others,
                saber_off_sounds,
            };
            let out = run(&mut state, &mut force, &mut frame);
            others.credit(&mut state);
            out
        };
        if let Some(players) = self.host.player_force() {
            players.force_end();
        }
        let npc = &mut self.actors[me];
        std::mem::swap(&mut npc.player, &mut state);
        *self.spare_state = Some(state);
        npc.force = force;
        npc.health = health;
        npc.mind.knockdown.hand_extend_time = hand_extend_time;
        Some(out)
    }

    /// `run` with the NPC at `me` as a power's user outside its update (its AI's
    /// `ForceHeal`, `ForceSpeed`, `WP_ForcePowerStop`, ...).
    pub(crate) fn with_forcer<T>(
        &mut self,
        me: usize,
        run: impl FnOnce(&mut crate::force_powers::Forcer<'_, '_>) -> T,
    ) -> Option<T> {
        self.with_force_frame(me, |state, force, frame| {
            run(&mut crate::force_powers::Forcer::new(state, force, frame))
        })
    }

    /// `WP_ForcePowersUpdate`'s knockdown part (`w_force.c:5046-5135`), as a player's.
    fn knockdown_update(&mut self, me: usize) {
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        let updated = crate::knockdown::force_update(
            &mut npc.player,
            &mut npc.mind.knockdown,
            npc.health,
            &npc.mind.command,
            level_time,
        );
        if updated.blocking_cleared {
            npc.movement.set_saber_blocking(0);
        }
        if let Some(animation) = updated.animation {
            use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
            self.set_animation(
                me,
                SETANIM_BOTH,
                animation,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
        }
        let jump = updated
            .jump_sound
            .then(|| self.host.sound_index(b"*jump1.wav"));
        for mut event in updated.events {
            if event.event == crate::knockdown::EV_ENTITY_SOUND {
                event.parameter = u32::from(jump.unwrap_or(0));
            }
            self.host.raise(event);
        }
    }

    /// `G_Damage` on NPC `victim` by a Force power (lightning, drain, a grip's squeeze);
    /// the attacker's hit counter is the caller's to credit.
    pub(crate) fn force_blow(
        &mut self,
        victim: u16,
        request: DamageRequest,
    ) -> crate::npc_damage::NpcDamaged {
        let Some(at) = self.actor_at(victim) else {
            return crate::npc_damage::NpcDamaged::default();
        };
        self.host.noting_damage(
            victim,
            request
                .attacker
                .map_or(crate::npc_spawn::ENTITYNUM_WORLD, |attacker| {
                    attacker.client
                }),
            &request,
        );
        self.damage(
            at,
            crate::npc_damage::NpcBlow {
                request,
                spared_by_master: false,
                surface: None,
            },
        )
    }
}
