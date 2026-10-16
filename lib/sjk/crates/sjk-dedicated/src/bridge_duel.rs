//! Private duels on the server: the duel key (`Cmd_EngageDuel_f`), the duel's part of a
//! think (`ClientThink_real`, `g_active.c:2405-2530`) and what a duel shields
//! (`G_Damage`'s duel rule, `G_MissileImpact`'s `killProj`). The rules themselves are
//! [`sjk_game_jka::duel`]'s.

use super::{NativeGame, PLAYER_SPEED, Told};
use crate::collision::{Void, WithPlayers, WorldCollision};
use crate::peer::Peer;
use sjk_game_jka::damage::{DamageRequest, Target};
use sjk_game_jka::duel::{self, Duellist};
use sjk_game_jka::event_entity::EventEntity;
use sjk_game_jka::pmove::MovementCollision;
use sjk_protocol::UserCommand;

/// `MASK_PLAYERSOLID`: what the challenge's trace stops at.
const MASK_PLAYERSOLID: u32 = 0x1 | 0x10 | 0x100 | 0x1000;
/// `EV_GENERAL_SOUND`, and the wire field its channel goes in (`saberEntityNum`).
const EV_GENERAL_SOUND: u32 = 76;
const ES_SABER_ENTITY: usize = 37;
/// `CHAN_AUTO`: the duel's saber sounds.
const CHAN_AUTO: u32 = 0;
/// `g_privateDuel`, on by default; there is no cvar system yet.
const PRIVATE_DUELS: bool = true;

/// `peer`, player `number`, as a duel reads and changes it.
fn duellist(number: usize, peer: &mut Peer) -> Duellist<'_> {
    Duellist {
        number: number as u16,
        state: &mut peer.state,
        health: &mut peer.health,
        name: &peer.name,
        hand_extend_time: &mut peer.knockdown.hand_extend_time,
        invulnerable_until: &mut peer.invulnerable_until,
        team: peer.session.team,
    }
}

impl NativeGame {
    /// `run` with `client` as a duellist and `other` as the other, when it is somebody
    /// else still connected. `client`'s state and name are taken out of its peer while it
    /// runs (into a slot kept for it, so nothing allocates), so the other can be reached
    /// through the world.
    fn with_duellists<T>(
        &mut self,
        client: usize,
        other: usize,
        run: impl FnOnce(&mut Duellist, Option<&mut Duellist>) -> T,
    ) -> Option<T> {
        let Self {
            server,
            world,
            players,
            duel_slot,
            ..
        } = self;
        let world = server.world_mut(*world)?;
        let handle = players.at(client)?;
        let peer = world.entity_mut(handle)?;
        std::mem::swap(&mut peer.state, duel_slot);
        let name = std::mem::take(&mut peer.name);
        let (mut health, mut hand, mut invulnerable, team) = (
            peer.health,
            peer.knockdown.hand_extend_time,
            peer.invulnerable_until,
            peer.session.team,
        );
        let result = {
            let mut me = Duellist {
                number: client as u16,
                state: duel_slot,
                health: &mut health,
                name: &name,
                hand_extend_time: &mut hand,
                invulnerable_until: &mut invulnerable,
                team,
            };
            let them = players
                .at(other)
                .filter(|_| other != client)
                .and_then(|handle| world.entity_mut(handle));
            let mut them = them.map(|peer| duellist(other, peer));
            run(&mut me, them.as_mut())
        };
        let peer = world.entity_mut(handle)?;
        std::mem::swap(&mut peer.state, duel_slot);
        peer.name = name;
        (
            peer.health,
            peer.knockdown.hand_extend_time,
            peer.invulnerable_until,
        ) = (health, hand, invulnerable);
        Some(result)
    }

    /// The duel's messages, saber sounds (`G_Sound` on `CHAN_AUTO` where each player
    /// stands) and events (`eventTime`), and both players' movement restarted from their
    /// states.
    fn apply_duel(&mut self, told: duel::Told, players: [usize; 2], level_time: i32) {
        for (to, text) in told.commands {
            self.told.push(match to {
                Some(client) => Told::One(usize::from(client), text),
                None => Told::Everyone(text),
            });
        }
        for (number, played) in told.sounds {
            // The duellist's sabers' own sounds (`saber[n].soundOn`, `soundOff`).
            let Some(peer) = self.peer(usize::from(number)) else {
                continue;
            };
            if !played.plays(peer.sabers.hands[1].is_held()) {
                continue;
            }
            let (origin, saber) = (
                peer.state.origin(),
                &peer.sabers.hands[usize::from(played.hand)],
            );
            let index = if played.on {
                saber.sound_on
            } else {
                saber.sound_off
            };
            if index == 0 {
                continue;
            }
            let mut sound = EventEntity {
                event: EV_GENERAL_SOUND,
                parameter: u32::from(index),
                origin,
                client: None,
                broadcast: false,
                extra: [(0, 0); 12],
            };
            sound.extra[0] = (ES_SABER_ENTITY, CHAN_AUTO);
            let _ = self.pool.spawn_temporary(sound.state(), level_time, None);
        }
        for number in told.raised {
            if let Some(peer) = self.peer_mut(usize::from(number)) {
                peer.entity.event_raised(level_time);
            }
        }
        for number in players {
            if let Some(peer) = self.peer_mut(number) {
                peer.movement = peer.movement.reseeded(&peer.state);
            }
        }
    }

    /// `Cmd_EngageDuel_f` for `client` (its duel key): the challenge line through the map
    /// and the others, and a challenge or an acceptance of whom it strikes.
    pub(super) fn duel_key(&mut self, client: usize, level_time: i32) {
        let Some(peer) = self.peer(client) else {
            return;
        };
        if !duel::may_challenge(&peer.state, self.gametype, PRIVATE_DUELS, level_time) {
            return;
        }
        let (start, end) = duel::challenge_line(&peer.state);
        self.gather_obstacles(client);
        let Self { map, obstacles, .. } = &*self;
        let hit = match map {
            Some(map) => WithPlayers {
                world: WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                },
                players: obstacles,
            }
            .trace(start, [0.0; 3], [0.0; 3], end, MASK_PLAYERSOLID),
            None => WithPlayers {
                world: Void,
                players: obstacles,
            }
            .trace(start, [0.0; 3], [0.0; 3], end, MASK_PLAYERSOLID),
        };
        let other = usize::from(hit.entity_number);
        if hit.fraction == 1.0 || other >= self.players.places() || other == client {
            return;
        }
        let gametype = self.gametype;
        let told = self
            .with_duellists(client, other, |me, them| {
                them.map(|them| duel::engage(me, them, gametype, level_time))
            })
            .flatten();
        if let Some(told) = told {
            self.apply_duel(told, [client, other], level_time);
        }
    }

    /// `ClientThink_real`'s duel for `client`, before its move: the sabers lit once the
    /// wait is over, the duel ended, or the command and the speed held still while it has
    /// not begun.
    pub(super) fn duel_think(&mut self, client: usize, command: &mut UserCommand, level_time: i32) {
        let Some(peer) = self.peer(client) else {
            return;
        };
        if !peer.state.duel_in_progress() {
            return;
        }
        let other = usize::from(peer.state.duel_index());
        let mut told = duel::Told::default();
        let Some(thought) = self.with_duellists(client, other, |me, them| {
            duel::think(me, them, level_time, &mut told)
        }) else {
            return;
        };
        let touched =
            !told.raised.is_empty() || !told.sounds.is_empty() || !told.commands.is_empty();
        if touched {
            self.apply_duel(told, [client, other], level_time);
        }
        // `speed`, `basespeed`: `g_speed` every think, 0 while held still.
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let base_speed = peer.state.raw_field(37).unwrap_or(0);
        if thought.frozen {
            (command.forward_move, command.right_move, command.up_move) = (0, 0, 0);
            if base_speed != 0 {
                peer.state.set_speed(0.0);
                peer.state.set_base_speed(0);
                peer.movement = peer.movement.reseeded(&peer.state);
            }
        } else if base_speed == 0 {
            peer.state.set_speed(PLAYER_SPEED);
            peer.state.set_base_speed(PLAYER_SPEED as i32);
            peer.movement = peer.movement.reseeded(&peer.state);
        }
    }

    /// `G_Damage`'s duel rule for a blow on `target` (`g_combat.c:4540-4556`): whether it
    /// is refused, in which case only a DEMP2's shock lands.
    pub(super) fn duel_refuses(&mut self, target: usize, request: &DamageRequest) -> bool {
        let Some(victim) = self.peer(target) else {
            return false;
        };
        // Only a client attacks as a client: a trigger's or a mover's number is past them.
        let attacker = request
            .attacker
            .filter(|attacker| usize::from(attacker.client) < self.players.places())
            .and_then(|attacker| {
                Some((
                    attacker.client,
                    &self.peer(usize::from(attacker.client))?.state,
                ))
            });
        if !duel::damage_refused(target as u16, &victim.state, attacker, request.means) {
            return false;
        }
        let mut rng = self.deaths.rng;
        if let Some(victim) = self.peer_mut(target) {
            let (bottom, top) = victim.movement.box_bounds();
            let origin = victim.state.origin();
            let bounds = (
                std::array::from_fn(|axis| origin[axis] + bottom[axis] - 1.0),
                std::array::from_fn(|axis| origin[axis] + top[axis] + 1.0),
            );
            let team = victim.session.team;
            let mut view = Target {
                client: target as u16,
                state: &mut victim.state,
                health: &mut victim.health,
                origin,
                bounds,
                invulnerable_until: &mut victim.invulnerable_until,
                team,
                wounds: &mut victim.wounds,
                force: None,
                spared_by_master: false,
            };
            sjk_game_jka::damage::shock(&mut view, request, &mut rng);
            victim.movement = victim.movement.reseeded(&victim.state);
        }
        self.deaths.rng = rng;
        true
    }

    /// `G_MissileImpact`'s `killProj`: a missile at a player duelling somebody other than
    /// its owner is spent harmlessly.
    pub(super) fn duel_spares(&self, target: usize, owner: u16) -> bool {
        target < self.players.places()
            && self
                .peer(target)
                .is_some_and(|peer| duel::elsewhere(&peer.state, owner))
    }
}
