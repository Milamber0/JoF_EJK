//! `ForceThrow(user, pull)` (`w_force.c:2914-3677`) in the NPCs' world: an NPC's push or
//! pull — or a player's reaching the NPCs — the players' own
//! ([`crate::force_throw::throw`]) run over a world ([`NpcThrow`]) that holds the NPCs
//! beside the host's players, missiles and entities.
//!
//! An NPC is a client to the throw: it is listed where its linked box meets the thrower's
//! reach, and shoved, knocked down or disarmed as a player is. The reference never lists
//! a Galak mech, an AT-ST or a rancor (`w_force.c:3242-3248`), nor anything drawn with
//! `EF_NODRAW`.

use crate::event_entity::EventEntity;
use crate::force_throw::{Candidate, CandidateKind, ThrowPlayer, ThrowWorld};
use crate::npc_force_update::PlayerForce;
use crate::npc_spawn::{EF_NODRAW, NpcActor, NpcHost};
use crate::npc_world::NpcWorld;
use crate::player_death::Rng;
use crate::pmove::MovementTrace;

/// `CLASS_ATST`, `CLASS_GALAKMECH`, `CLASS_RANCOR`: never pushed or pulled.
const UNTHROWABLE: [i32; 3] = [1, 25, 54];
/// `s.eFlags`.
const ES_EFLAGS: usize = 19;

/// The world a push or pull in the NPCs' world reaches: every NPC and, through the host
/// ([`crate::npc_force_update::PlayerForce`]), everything else.
pub struct NpcThrow<'w, 'a, H: NpcHost> {
    /// The NPCs and the host.
    pub world: &'w mut NpcWorld<'a, H>,
    /// An NPC's `invulnerableTimer`, which nothing gives it: a throw's write lands here.
    pub invulnerable: i32,
}

impl<H: NpcHost> NpcThrow<'_, '_, H> {
    fn players(&mut self) -> &mut dyn PlayerForce {
        self.world
            .host
            .player_force()
            .expect("a host running the NPCs' Force")
    }
}

/// An NPC as `EntitiesInBox` lists it for a push or pull between `mins` and `maxs`: a
/// begun client whose linked box meets them — never an unthrowable class, nor one not drawn.
pub fn npc_candidate(npc: &NpcActor, mins: [f32; 3], maxs: [f32; 3]) -> Option<Candidate> {
    let (absmin, absmax) = npc.link;
    let overlaps = (0..3).all(|axis| absmin[axis] <= maxs[axis] && absmax[axis] >= mins[axis]);
    let drawn = npc.state.raw_field(ES_EFLAGS).unwrap_or(0) & EF_NODRAW == 0;
    (npc.begun() && overlaps && drawn && !UNTHROWABLE.contains(&npc.definition.client_class)).then(
        || Candidate {
            number: npc.number,
            kind: CandidateKind::Player,
            absmin,
            absmax,
            origin: npc.player.origin(),
        },
    )
}

/// An NPC as a push or pull reads and changes it; `invulnerable` stands for the
/// `invulnerableTimer` nothing gives an NPC.
pub fn npc_as_thrown<'a>(npc: &'a mut NpcActor, invulnerable: &'a mut i32) -> ThrowPlayer<'a> {
    ThrowPlayer {
        state: &mut npc.player,
        force: &mut npc.force,
        health: npc.health,
        knockdown: &mut npc.mind.knockdown,
        other_killer: &mut npc.mind.fight.other_killer,
        npc_class: Some(npc.definition.client_class),
        invulnerable_until: invulnerable,
        command: npc.mind.command,
        team: npc.session_team,
        push_effect_until: &mut npc.mind.push_effect_time,
        lock_hits: &mut npc.mind.saber_lock_hits,
    }
}

impl<H: NpcHost> ThrowWorld for NpcThrow<'_, '_, H> {
    fn entities_in_box(&mut self, mins: [f32; 3], maxs: [f32; 3], out: &mut Vec<Candidate>) {
        out.clear();
        self.players().force_entities_in_box(mins, maxs, out);
        out.extend(
            self.world
                .actors
                .iter()
                .filter_map(|npc| npc_candidate(npc, mins, maxs)),
        );
        out.sort_by_key(|candidate| candidate.number);
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

    fn player(&mut self, number: u16) -> Option<ThrowPlayer<'_>> {
        match self.world.actor_at(number) {
            Some(at) => Some(npc_as_thrown(
                &mut self.world.actors[at],
                &mut self.invulnerable,
            )),
            None => self.players().force_throw_player(number),
        }
    }

    fn reflect_missile(&mut self, missile: u16, thrower: u16, forward: [f32; 3]) {
        // The thrower where it stands: an NPC, or the player.
        let origin = match self.world.actor_at(thrower) {
            Some(at) => self.world.actors[at].player.origin(),
            None => match self.players().force_throw_player(thrower) {
                Some(player) => player.state.origin(),
                None => return,
            },
        };
        self.players()
            .force_reflect_missile(missile, thrower, origin, forward);
    }

    fn toss_weapon(&mut self, victim: u16, direction: [f32; 3], speed: f32) {
        // An NPC's own weapon is not tossed: its weapon's pickup is the combat step's.
        if self.world.actor_at(victim).is_none() {
            self.players().force_toss_weapon(victim, direction, speed);
        }
    }

    fn raise(&mut self, event: EventEntity) {
        let level_time = self.world.level_time;
        let _ = self
            .players()
            .force_pool()
            .spawn_temporary(event.state(), level_time, None);
    }

    fn sound_index(&mut self, name: &[u8]) -> u16 {
        self.world.host.sound_index(name)
    }

    fn rng(&mut self) -> &mut Rng {
        self.world.host.rng()
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `ForceThrow(user, pull)`: `thrower` (an NPC, or a player) pushes (or pulls)
    /// whatever its level reaches. The movement of every NPC it moved restarts from its
    /// player state at its next move.
    pub fn force_throw_by(&mut self, thrower: u16, pull: bool) {
        let context = crate::force_throw::Throw {
            level_time: self.level_time,
            gametype: self.host.gametype(),
            thrower,
            pull,
        };
        let Some(players) = self.host.player_force() else {
            return;
        };
        players.force_begin();
        crate::force_throw::throw(
            context,
            &mut NpcThrow {
                world: &mut *self,
                invulnerable: 0,
            },
        );
        if let Some(players) = self.host.player_force() {
            players.force_end();
        }
    }

    /// `ForceThrow(NPC, pull)` by the NPC at `me`.
    pub fn force_throw(&mut self, me: usize, pull: bool) {
        self.force_throw_by(self.actors[me].number, pull);
    }
}
