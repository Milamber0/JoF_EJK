//! The server's thrown sabers: the throw started from the blade the server read in the
//! hand, each saber entity's think in the frame's pass over the entities, and the wire
//! state of a flying saber sent to everyone. The rules are
//! [`sjk_game_jka::saber_throw`]'s; this is where the server's players, map and
//! breakables meet them.

use super::{EntityPool, NativeGame, Peer, SABER_ENTITY, Told};
use crate::collision::{Void, WithPlayers, WorldCollision};
use crate::visibility::Eye;
use sjk_game_jka::damage::DamageRequest;
use sjk_game_jka::entity_clip::BoxObstacle;
use sjk_game_jka::event_entity::EventEntity;
use sjk_game_jka::player_death::Rng;
use sjk_game_jka::pmove::{MovementCollision, MovementTrace};
use sjk_game_jka::saber_block::Defender;
use sjk_game_jka::saber_throw::{
    CONTENTS_LIGHTSABER, FlightTarget, Flown, Saber, SaberFlight, SaberOwner, SaberThink,
};

/// `FP_SABER_OFFENSE`, `FP_SABER_DEFENSE`, `FP_SABERTHROW`, `WP_SABER`, `CONTENTS_BODY`.
const FP_SABER_OFFENSE: usize = 15;
const FP_SABER_DEFENSE: usize = 16;
const FP_SABERTHROW: usize = 17;
const WP_SABER: u8 = 3;
const CONTENTS_BODY: u32 = 0x100;
/// `ENTITYNUM_NONE`.
const ENTITY_NONE: u16 = 1_023;

/// The server as `owner`'s thrown saber sees it.
struct ServerFlight<'a> {
    game: &'a mut NativeGame,
    owner: usize,
}

impl SaberFlight for ServerFlight<'_> {
    /// Past the owner, the obstacles gathered for it; past the saber alone, the owner
    /// too.
    fn trace(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        pass: u16,
        mask: u32,
    ) -> MovementTrace {
        let owner = self.owner;
        let own = self
            .game
            .peer(owner)
            .filter(|peer| peer.playing() && peer.corpse.is_none())
            .map(|peer| BoxObstacle {
                entity: owner as u16,
                origin: peer.state.origin(),
                bounds: peer.movement.box_bounds(),
                contents: CONTENTS_BODY,
                model: None,
            });
        let NativeGame {
            map,
            obstacles,
            lock_obstacles,
            ..
        } = &mut *self.game;
        let players: &[BoxObstacle] = if usize::from(pass) == owner {
            obstacles
        } else {
            lock_obstacles.clear();
            lock_obstacles.extend_from_slice(obstacles);
            lock_obstacles.extend(own);
            lock_obstacles
        };
        match map {
            Some(map) => WithPlayers {
                world: WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                },
                players,
            }
            .trace(start, mins, maxs, end, mask),
            None => WithPlayers {
                world: Void,
                players,
            }
            .trace(start, mins, maxs, end, mask),
        }
    }

    fn in_pvs(&mut self, from: [f32; 3], to: [f32; 3]) -> bool {
        self.game
            .map
            .as_ref()
            .is_none_or(|map| Eye::new(&map.bsp, &map.areas, from).sees_point(&map.bsp, to))
    }

    fn owner(&mut self) -> Option<SaberOwner<'_>> {
        let gametype = self.game.gametype;
        let owner = self.owner;
        let peer = self.game.peer_mut(owner)?;
        Some(SaberOwner {
            number: owner as u16,
            health: peer.health,
            spectator: !peer.playing(),
            offense: peer.force.levels[FP_SABER_OFFENSE],
            throw_level: peer.force.levels[FP_SABERTHROW],
            buttons: peer.lock.buttons,
            command_buttons: peer.last_command.buttons,
            storage: peer.saber_cut.storage,
            gametype,
            team: peer.session.team,
            memory: &mut peer.throw_memory,
            throw_delay: &mut peer.saber.throw_delay,
            attack_wound: &mut peer.saber_cut.attack_wound,
            danger_time: &mut peer.force.danger_time,
            invulnerable_until: &mut peer.invulnerable_until,
            state: &mut peer.state,
        })
    }

    fn entity_count(&self) -> u16 {
        let breakables = self
            .game
            .breakables
            .iter()
            .map(|(id, _)| id.legacy_number() + 1)
            .max()
            .unwrap_or(0);
        let sabers = (0..self.game.players.places())
            .filter_map(|client| self.game.peer(client))
            .map(|peer| peer.saber_number() + 1)
            .max()
            .unwrap_or(0);
        (self.game.players.places() as u16)
            .max(breakables)
            .max(sabers)
    }

    fn target(&mut self, number: u16) -> Option<FlightTarget> {
        let game = &*self.game;
        if usize::from(number) < game.players.places() {
            let peer = game.peer(usize::from(number)).filter(|peer| peer.begun)?;
            let state = &peer.state;
            let duel = state.duel_in_progress().then(|| state.duel_index());
            let contents = if peer.corpse.is_some() || !peer.playing() {
                0
            } else {
                CONTENTS_BODY
            };
            return Some(FlightTarget {
                client: true,
                origin: state.origin(),
                takes_damage: peer.playing(),
                health: peer.health,
                spectator: !peer.playing(),
                duel,
                contents,
                owner: ENTITY_NONE,
                ..FlightTarget::default()
            });
        }
        if let Some((_, brush)) = game
            .breakables
            .iter()
            .find(|(ours, _)| ours.legacy_number() == number)
        {
            // A brush entity's origin is where its model was built: the world's.
            return Some(FlightTarget {
                takes_damage: brush.contents != 0,
                health: brush.health,
                contents: brush.contents,
                owner: ENTITY_NONE,
                ..FlightTarget::default()
            });
        }
        let owner = (0..game.players.places()).find(|client| {
            game.peer(*client)
                .is_some_and(|peer| peer.saber_number() == number)
        })?;
        let peer = game.peer(owner)?;
        let contents = if peer.flight.think == SaberThink::InHand {
            if peer.saber_cut.entity_solid() {
                CONTENTS_LIGHTSABER
            } else {
                0
            }
        } else {
            peer.flight.contents
        };
        Some(FlightTarget {
            origin: peer.saber_cut.entity.0,
            contents,
            owner: owner as u16,
            ..FlightTarget::default()
        })
    }

    fn view_angles(&mut self, number: u16) -> Option<[f32; 3]> {
        self.game
            .peer(usize::from(number))
            .map(|peer| peer.state.view_angles())
    }

    fn saber_box(&mut self, current: [f32; 3]) -> ([f32; 3], [f32; 3]) {
        let level_time = self.game.last_frame_time;
        self.game
            .peer(self.owner)
            .map_or(([-16.0; 3], [16.0; 3]), |peer| {
                super::bridge_saber_damage::saber_box_of(self.owner, peer, current, level_time)
            })
    }

    fn run_missile(
        &mut self,
        missile: &mut sjk_game_jka::weapon_fire::Missile,
        level_time: i32,
        previous_time: i32,
    ) {
        let _ = self
            .game
            .run_one_missile(missile, level_time, previous_time);
    }

    fn defence(&mut self, number: u16) -> Option<u8> {
        self.game
            .peer(usize::from(number))
            .map(|peer| peer.force.levels[FP_SABER_DEFENSE])
    }

    fn block(&mut self, number: u16, point: [f32; 3]) -> bool {
        let level_time = self.game.last_frame_time;
        let Some(peer) = self.game.peer_mut(usize::from(number)) else {
            return false;
        };
        let mut defender = Defender {
            client: number,
            state: &mut peer.state,
            saber_blocking: peer.movement.state().saber_blocking,
            buttons: peer.last_command.buttons,
            forward_move: peer.last_command.forward_move,
            defense: peer.force.levels[FP_SABER_DEFENSE],
            block_time: &mut peer.block_time,
        };
        let blocked = defender.can_block_blow(point, level_time);
        if blocked {
            let raised = peer.state.saber_blocked();
            peer.movement.set_saber_blocked(raised);
        }
        blocked
    }

    /// Numbered by the pool and run with the other missiles: this frame where its slot
    /// comes after the saber's (`G_RunFrame` walks the entities in order), else from the
    /// next.
    fn spawn_dead_saber(&mut self, missile: sjk_game_jka::weapon_fire::Missile) {
        let level_time = self.game.last_frame_time;
        let Some(number) = self
            .game
            .pool
            .spawn_entity(missile.state.clone(), level_time)
        else {
            return;
        };
        self.game.pool.set_bounds(number, missile.bounds);
        let spawner = self
            .game
            .peer(self.owner)
            .and_then(|peer| peer.saber_entity);
        if spawner.is_none_or(|spawner| number.ordinal() > spawner.ordinal()) {
            self.game.missiles.push((number, missile));
        } else {
            self.game.dead_sabers_waiting.push((number, missile));
        }
    }

    fn hurt(&mut self, target: u16, request: DamageRequest) {
        let level_time = self.game.last_frame_time;
        if usize::from(target) < self.game.players.places() {
            let _ = self
                .game
                .strike(self.owner, usize::from(target), request, false);
        } else {
            let _ = self.game.hurt_brush(
                target,
                request.damage,
                request.means,
                self.owner as u16,
                level_time,
            );
        }
    }

    fn raise(&mut self, event: EventEntity) {
        let level_time = self.game.last_frame_time;
        let _ = self
            .game
            .pool
            .spawn_temporary(event.state(), level_time, None);
    }

    fn sound_index(&mut self, name: &[u8]) -> u16 {
        let NativeGame { sounds, told, .. } = &mut *self.game;
        sounds.index(name, &mut |index, value| {
            told.push(Told::ConfigString {
                index,
                previous: Vec::new(),
                value: value.to_vec(),
            })
        })
    }

    /// The owner's sabers as their definitions give them.
    fn owner_saber(&mut self) -> sjk_game_jka::saber_throw::SaberLook {
        let NativeGame {
            server,
            world,
            players,
            models,
            told,
            ..
        } = &mut *self.game;
        let peer = players
            .at(self.owner)
            .and_then(|handle| server.world(*world)?.entity(handle));
        let Some([first, second]) = peer.map(|peer| &peer.sabers.hands) else {
            return Default::default();
        };
        let model = if first.model.is_empty() {
            sjk_game_jka::saber_throw::SABER_MODEL
        } else {
            &first.model
        };
        let model = models.index(model, &mut |index, value| {
            told.push(Told::ConfigString {
                index,
                previous: Vec::new(),
                value: value.to_vec(),
            })
        });
        sjk_game_jka::saber_throw::SaberLook {
            model,
            on: [first.sound_on, second.sound_on],
            hum: first.sound_loop,
            off: first.sound_off,
            spin: first.spin_sound,
        }
    }

    fn owner_first_saber(&self) -> Option<&sjk_game_jka::saber_definition::SaberDefinition> {
        let NativeGame {
            server,
            world,
            players,
            ..
        } = &*self.game;
        let peer = players
            .at(self.owner)
            .and_then(|handle| server.world(*world)?.entity(handle))?;
        Some(&peer.sabers.hands[0])
    }

    fn model_index(&mut self, name: &[u8]) -> u16 {
        let NativeGame { models, told, .. } = &mut *self.game;
        models.index(name, &mut |index, value| {
            told.push(Told::ConfigString {
                index,
                previous: Vec::new(),
                value: value.to_vec(),
            })
        })
    }

    fn rng(&mut self) -> &mut Rng {
        &mut self.game.deaths.rng
    }
}

impl NativeGame {
    /// `WP_SaberPositionUpdate` for `client`'s saber out of its hand, or lit in it: a
    /// throw the movement began starts from the blade read this frame (its base, its
    /// direction with the view's yaw); a flying saber learns where the hand is.
    pub(super) fn throw_frame(&mut self, client: usize, level_time: i32) {
        let Some(peer) = self.peer(client) else {
            return;
        };
        let state = &peer.state;
        if peer.saber_entity.is_none() || state.saber_entity_num() == 0 {
            return;
        }
        if !state.saber_in_flight() {
            // In hand: past the update's early return (a saber carrier, not raising or
            // lowering it, alive) and lit.
            let updated = state.weapon() == WP_SABER
                && !matches!(state.weapon_state(), 1 | 2)
                && peer.health >= 1;
            if updated && state.saber_holstered() == 0 {
                let _ = self.with_saber(client, |saber, _| {
                    sjk_game_jka::saber_throw::hand_update(saber)
                });
            }
            return;
        }
        // The hand: the blade read this frame, else (no model) the player's own place.
        let view_yaw = state.view_angles()[1];
        let (bolt_origin, bolt_angles) =
            match peer.blade.filter(|(_, read_at)| *read_at == level_time) {
                Some((blade, _)) => (
                    blade.base,
                    [blade.direction[0], view_yaw, blade.direction[2]],
                ),
                None => (state.origin(), [-0.0, view_yaw, -0.0]),
            };
        let _ = self.with_saber(client, |saber, world| {
            sjk_game_jka::saber_throw::owner_update(
                saber,
                world,
                bolt_origin,
                bolt_angles,
                level_time,
            )
        });
    }

    /// `G_RunThink` for every player's saber entity after the players' pass: in hand, the
    /// flag for a throw; out of it, the flight.
    pub(super) fn saber_thinks(&mut self, level_time: i32) {
        for client in 0..self.players.places() {
            let Some(peer) = self.peer(client).filter(|peer| peer.saber_entity.is_some()) else {
                continue;
            };
            // In hand the think only flags a throw: the world is gathered for a flight.
            if peer.flight.think != SaberThink::InHand {
                self.gather_obstacles(client);
                self.gather_saber_entities(client);
            }
            let previous_time = self.previous_frame_time;
            let flown = self.with_saber(client, |saber, world| {
                sjk_game_jka::saber_throw::run_think(saber, world, level_time, previous_time)
            });
            match flown.unwrap_or_default() {
                Flown::Nothing => {}
                Flown::Freed => {
                    // The owner left the game: its saber is back in its keeping, hidden.
                    if let Some(peer) = self.peer_mut(client) {
                        peer.flight.think = SaberThink::InHand;
                        peer.flight.shown = false;
                        peer.state.set_raw_field(88, 0);
                        peer.movement = peer.movement.reseeded(&peer.state);
                    }
                    if let Some(saber) = self.peer(client).and_then(|peer| peer.saber_entity) {
                        self.pool.show(saber, false);
                    }
                }
            }
        }
    }

    /// `run` on `client`'s saber with the server as its world; its wire state is the
    /// pool's, sent while it flies. The owner's movement takes up what changed on its
    /// wire state, and a flying saber is where other blades meet it.
    pub(super) fn with_saber<T>(
        &mut self,
        client: usize,
        run: impl FnOnce(&mut Saber, &mut dyn SaberFlight) -> T,
    ) -> Option<T> {
        let (id, mut entity, before) = {
            let peer = self.peer(client)?;
            (peer.saber_entity?, peer.flight, watched(&peer.state))
        };
        let number = id.legacy_number();
        let event_time = entity.event_time;
        // A saber entity of a map left behind is gone until the next spawn makes one.
        let mut state = std::mem::replace(
            self.pool.state_mut(id)?,
            sjk_protocol::EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS),
        );
        let result = run(
            &mut Saber {
                number,
                entity: &mut entity,
                state: &mut state,
            },
            &mut ServerFlight {
                game: self,
                owner: client,
            },
        );
        if let Some(slot) = self.pool.state_mut(id) {
            *slot = state;
        }
        self.pool.show(id, entity.shown);
        if entity.event_time != event_time {
            self.pool.event_raised(id, entity.event_time);
        }
        if let Some(peer) = self.peer_mut(client) {
            peer.flight = entity;
            if watched(&peer.state) != before {
                peer.movement = peer.movement.reseeded(&peer.state);
            }
            if entity.think != SaberThink::InHand {
                peer.saber_cut.entity = (
                    entity.current,
                    entity.mins,
                    entity.maxs,
                    entity.contents != 0,
                );
            }
        }
        Some(result)
    }
}

impl NativeGame {
    /// `Cmd_ToggleSaber_f` for a saber in flight: turned off in the air
    /// (`saberKnockDown` by its owner).
    pub(super) fn knock_down_in_flight(&mut self, client: usize, level_time: i32) {
        let _ = self.with_saber(client, |saber, world| {
            sjk_game_jka::saber_drop::knock_down(saber, world, client as u16, level_time)
        });
    }

    /// `saberKnockOutOfHand` for `client`'s saber, at `velocity`: whether it flew.
    pub(super) fn knock_out(&mut self, client: usize, velocity: [f32; 3], level_time: i32) -> bool {
        // An NPC's saber (a player's blade knocked it away) flies as the roster flies it.
        // The obstacles gathered for the sweep that knocked it are kept across.
        if self.npcs.roster.is_npc(client as u16) {
            let kept = (
                std::mem::take(&mut self.obstacles),
                std::mem::take(&mut self.body_legs),
            );
            let flew = self
                .with_roster(|roster, _, host| {
                    roster.knock_saber_out(client as u16, velocity, level_time, host)
                })
                .unwrap_or(false);
            (self.obstacles, self.body_legs) = kept;
            return flew;
        }
        self.with_saber(client, |saber, world| {
            sjk_game_jka::saber_drop::knock_out_of_hand(saber, world, velocity, level_time)
        })
        .unwrap_or(false)
    }

    /// `saberCheckKnockdown_Smashed` for `client`'s thrown saber struck by `striker`.
    pub(super) fn smash(
        &mut self,
        client: usize,
        striker: u16,
        damage: i32,
        level_time: i32,
    ) -> bool {
        let defending = self.peer(usize::from(striker)).is_some_and(|peer| {
            sjk_game_jka::saber_rules::in_extra_defense(peer.state.saber_move())
        });
        // An NPC's thrown saber (a player's blade struck it) is smashed as the roster flies it.
        if self.npcs.roster.is_npc(client as u16) {
            let kept = (
                std::mem::take(&mut self.obstacles),
                std::mem::take(&mut self.body_legs),
            );
            let smashed = self
                .with_roster(|roster, _, host| {
                    roster.smash_saber(client as u16, striker, defending, damage, level_time, host)
                })
                .unwrap_or(false);
            (self.obstacles, self.body_legs) = kept;
            return smashed;
        }
        self.with_saber(client, |saber, world| {
            sjk_game_jka::saber_drop::smashed(saber, world, striker, defending, damage, level_time)
        })
        .unwrap_or(false)
    }

    /// `saberCheckKnockdown_DuelLoss` for `client`, beaten in a lock by `other`: whether
    /// its saber was knocked out of its hand.
    pub(super) fn disarm_duel_loss(
        &mut self,
        client: usize,
        other: usize,
        level_time: i32,
    ) -> bool {
        let (storage, chance) = self.peer(other).map_or((Default::default(), 1), |peer| {
            (
                peer.saber_cut.storage,
                peer.sabers.disarm_chance(peer.state.saber_holstered()),
            )
        });
        self.with_saber(client, |saber, world| {
            sjk_game_jka::saber_drop::duel_loss(saber, world, &storage, chance, level_time)
        })
        .unwrap_or(false)
    }

    /// `G_TouchTriggers` for `client` against the knocked sabers, in entity number order:
    /// each one it is in contact with stands its angles upright (`SaberBounceSound`).
    pub(super) fn touch_sabers(&mut self, client: usize) {
        let Some(peer) = self
            .peer(client)
            .filter(|peer| peer.health > 0 && peer.playing())
        else {
            return;
        };
        let (origin, bounds) = (peer.state.origin(), peer.movement.box_bounds());
        for owner in 0..self.players.places() {
            let touched = self.peer(owner).is_some_and(|other| {
                other.saber_entity.is_some()
                    && sjk_game_jka::saber_drop::touched_by(&other.flight, origin, bounds)
            });
            if touched {
                let _ = self.with_saber(owner, |saber, _| {
                    sjk_game_jka::saber_drop::bounce_sound(saber)
                });
            }
        }
    }
}

/// The owner's wire fields a flight changes that its movement keeps too: `saberEntityNum`,
/// `saberInFlight`, `saberCanThrow`.
fn watched(state: &sjk_protocol::PlayerState) -> [u32; 3] {
    [31, 88, 49].map(|index| state.raw_field(index).unwrap_or(0))
}

/// `WP_SaberInitBladeData` for `peer`'s saber entity (`saberStoredIndex`): named by the
/// state again, in hand, drawn by nobody, hidden, its think every frame from now
/// (`SaberUpdateSelf`) — the entity slot kept for the player's whole stay.
pub(super) fn init_saber_entity(peer: &mut Peer, pool: &mut EntityPool, server_time: i32) {
    let Some(id) = peer.saber_entity else { return };
    peer.state
        .set_raw_field(SABER_ENTITY, u32::from(id.legacy_number()));
    if let Some(state) = pool.state_mut(id) {
        *state = sjk_protocol::EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
        let _ = state.set_number(id.legacy_number());
        sjk_game_jka::saber_throw::in_hand_state(state);
    }
    pool.show(id, false);
    peer.flight = sjk_game_jka::saber_throw::SaberEntity {
        next_think: server_time,
        ..Default::default()
    };
}
