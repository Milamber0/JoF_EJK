//! The players as an NPC's saber reaches them outside its blade's sweep, on this server:
//! the NPC's saber entity's wire state for its flight ([`NpcHost::take_entity_state`]), a
//! player blocking a thrown NPC saber, a player's saber knocked out of its hand or touched
//! where it lies, and a saber lock between an NPC and a player — its start, its move pushing
//! the player's pose, and what its end leaves ([`sjk_game_jka::npc_saber_lock`]).
//!
//! A knock-out, a knocked saber touched and a lost lock's end are the players' saber
//! flights' ([`super::NativeGame`]'s), so they are done once the NPC's turn is over
//! ([`NpcOutcome`]).

use super::*;
use sjk_game_jka::npc_saber_lock::{HostLockWorld, LockPartner, npc_lock_fighter, npc_locked};
use sjk_game_jka::npc_spawn::NpcActor;
use sjk_game_jka::pmove_saber_lock::{LockContext, LockOutcome};
use sjk_game_jka::saber_lock::{LockFighter, MASK_PLAYERSOLID, check_lock};

/// `FP_SABER_DEFENSE`; `ps.saberLockEnemy`.
const FP_SABER_DEFENSE: usize = 16;

impl NativeGame {
    /// A knocked saber of `client`'s touched by an NPC: stood upright (`SaberBounceSound`).
    pub(in super::super) fn bounce_saber(&mut self, client: usize) {
        let _ = self.with_saber(client, |saber, _| {
            sjk_game_jka::saber_drop::bounce_sound(saber)
        });
    }

    /// `pmove.checkDuelLoss` for `client`, who lost a lock to an NPC (`attacker`, standing
    /// at `origin`): finished outright on a draw of 0 to 40 above its health — still, its
    /// pose dropped — else, standing, its saber may fly (`saberCheckKnockdown_DuelLoss`)
    /// along the NPC's blades (`storage`) on its disarm chance (`g_active.c:3072-3107`).
    pub(in super::super) fn lost_lock_to_npc(
        &mut self,
        client: usize,
        attacker: sjk_game_jka::damage::Attacker,
        origin: [f32; 3],
        storage: &sjk_game_jka::saber_clash::SaberStorage,
        chance: i32,
        level_time: i32,
    ) {
        const HANDEXTEND_KNOCKDOWN: u32 = 8;
        const PS_FORCE_HAND_EXTEND: usize = 80;
        let Some(health) = self.peer(client).map(|peer| peer.health) else {
            return;
        };
        if self.deaths.rng.irand(0, 40) > health {
            let Some(victim) = self.peer_mut(client) else {
                return;
            };
            let at = victim.state.origin();
            victim.state.set_velocity([0.0; 3]);
            victim.state.set_raw_field(PS_FORCE_HAND_EXTEND, 0);
            victim.knockdown.hand_extend_time = 0;
            victim.movement = victim.movement.reseeded(&victim.state);
            let mut direction = [origin[0] - at[0], origin[1] - at[1], origin[2] - at[2]];
            let length = f64::from(
                direction[0] * direction[0]
                    + direction[1] * direction[1]
                    + direction[2] * direction[2],
            )
            .sqrt() as f32;
            if length != 0.0 {
                direction = direction.map(|value| value * (1.0 / length));
            }
            let request = DamageRequest {
                level_time,
                attacker: Some(attacker),
                direction: Some(direction),
                point: Some(at),
                damage: 9_999,
                flags: sjk_game_jka::triggers::DAMAGE_NO_PROTECTION,
                means: sjk_game_jka::damage::MOD_SABER,
            };
            self.avoid_dismember = true;
            let _ = self.strike(usize::from(attacker.client), client, request, false);
            self.avoid_dismember = false;
            self.dismember_lock_loser(client, attacker.client, level_time);
        } else if let Some(victim) = self.peer(client) {
            let knocked =
                victim.state.raw_field(PS_FORCE_HAND_EXTEND).unwrap_or(0) == HANDEXTEND_KNOCKDOWN;
            if !knocked && victim.state.saber_entity_num() != 0 {
                let _ = self.with_saber(client, |saber, world| {
                    sjk_game_jka::saber_drop::duel_loss(saber, world, storage, chance, level_time)
                });
                if let Some(victim) = self.peer_mut(client) {
                    victim.movement = victim.movement.reseeded(&victim.state);
                }
            }
        }
    }
}

impl ServerHost<'_> {
    fn peer_at(&mut self, client: u16) -> Option<&mut crate::peer::Peer> {
        let handle = self.roster.at(usize::from(client))?;
        self.server.world_mut(self.world)?.entity_mut(handle)
    }

    /// Entity `number`'s wire state, taken out of its pool slot.
    pub(super) fn take_state(&mut self, number: u16) -> Option<EntityState> {
        let id = self.pool.legacy_id(number)?;
        Some(std::mem::replace(
            self.pool.state_mut(id)?,
            EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS),
        ))
    }

    /// Entity `number`'s wire state given back, sent to the clients or not.
    pub(super) fn put_state(&mut self, number: u16, mut state: EntityState, shown: bool) {
        let Some(id) = self.pool.legacy_id(number) else {
            return;
        };
        let _ = state.set_number(number);
        if let Some(slot) = self.pool.state_mut(id) {
            *slot = state;
        }
        self.pool.show(id, shown);
    }

    /// Entity `number` raised an event of its own at `event_time`.
    pub(super) fn state_event(&mut self, number: u16, event_time: i32) {
        if let Some(id) = self.pool.legacy_id(number) {
            self.pool.event_raised(id, event_time);
        }
    }

    /// Player `player` blocking a thrown NPC saber at `point` (`WP_SaberCanBlock`,
    /// `WP_SaberBlockNonRandom`).
    pub(super) fn blocks_thrown(&mut self, player: u16, point: [f32; 3]) -> bool {
        let level_time = self.level_time;
        let Some(peer) = self.peer_at(player) else {
            return false;
        };
        let mut defender = sjk_game_jka::saber_block::Defender {
            client: player,
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

    /// `WP_SabersCheckLock` between `npc` and player `player` ([`NpcHost::player_lock`]):
    /// the player's side read from its state, both begun and restarted on a lock.
    pub(super) fn lock_with_player(
        &mut self,
        npc: &mut NpcActor,
        player: u16,
        npc_first: bool,
        bodies: &mut Vec<BoxObstacle>,
        level_time: i32,
    ) -> bool {
        let Some(lengths) = npc.movement.shared_animation_lengths() else {
            return false;
        };
        let team = self
            .players
            .iter()
            .find(|body| body.number == player)
            .map_or(2, |body| body.player_team);
        let Some(peer) = self.peer_at(player) else {
            return false;
        };
        let (mut state, mut hits) = (peer.state.clone(), peer.lock.hits);
        let (bounds, saber_blocking, command_angles) = (
            peer.movement.box_bounds(),
            peer.movement.state().saber_blocking,
            peer.last_command.angles,
        );
        let (lockable, animation_scales) = (
            peer.sabers.lockable(state.saber_holstered()),
            peer.movement.state().saber_anim_speed_scales.0,
        );
        let mut player_side = LockFighter {
            origin_linked: false,
            number: player,
            bounds,
            saber_blocking,
            command_angles,
            hits: &mut hits,
            animation_scales,
            lockable,
            npc: false,
            player_team: team,
            clip_mask: MASK_PLAYERSOLID,
            state: &mut state,
        };
        let (gametype, mut rng, count) = (self.gametype, self.deaths.rng, bodies.len());
        let forced = self.debug_saber_locks;
        let (locked, origin_linked) = {
            let mut world = HostLockWorld {
                host: &mut *self,
                bodies,
                count,
            };
            let mut mine = npc_lock_fighter(npc);
            let (one, two) = if npc_first {
                (&mut mine, &mut player_side)
            } else {
                (&mut player_side, &mut mine)
            };
            let locked = if forced {
                sjk_game_jka::saber_lock::forced_lock(
                    one,
                    two,
                    &mut rng,
                    lengths.as_ref(),
                    &mut world,
                    level_time,
                )
            } else {
                check_lock(
                    one,
                    two,
                    gametype,
                    true,
                    &mut rng,
                    lengths.as_ref(),
                    &mut world,
                    level_time,
                )
            };
            (locked, mine.origin_linked)
        };
        self.deaths.rng = rng;
        if locked {
            if let Some(peer) = self.peer_at(player) {
                (peer.state, peer.lock.hits) = (state, hits);
                peer.movement = peer.movement.reseeded(&peer.state);
            }
            npc_locked(npc, origin_linked);
        }
        locked
    }

    /// An NPC's locked move ([`NpcHost::move_npc_locked`]) through the map and everyone,
    /// a player opponent's movement taken up from its state and written back to it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn locked_move(
        &mut self,
        movement: &mut Predictor,
        command: UserCommand,
        context: &MoveContext,
        pass: u16,
        bodies: &[BoxObstacle],
        partner: LockPartner<'_>,
        hits: i32,
    ) -> LockOutcome {
        let (map, solids) = (self.map, self.solids);
        let mut rng = self.deaths.rng;
        // The lock changes the weapon move, not Pmove's foot-slope queries.
        let (npc_bodies, clock) = (std::cell::RefCell::new(&mut *self.bodies), self.ghoul2_time);
        let feet = |origin: [f32; 3], yaw: f32| {
            npc_bodies
                .borrow_mut()
                .foot_points(pass, yaw, origin, clock)
        };
        let context = &context.with_foot_bolts(&feet);
        let everyone = Everyone {
            solids,
            bodies,
            pass,
            riders: &[],
        };
        let mut run = |opponent: Option<&mut Predictor>| {
            let lock = LockContext {
                opponent,
                rng: &mut rng,
                hits,
            };
            everyone.with_obstacles(|players| match map {
                Some(map) => movement.predict_command_locked(
                    command,
                    &WithPlayers {
                        world: WorldCollision {
                            bsp: &map.bsp,
                            scratch: &map.scratch,
                        },
                        players,
                    },
                    context,
                    lock,
                ),
                None => movement.predict_command_locked(
                    command,
                    &WithPlayers {
                        world: Void,
                        players,
                    },
                    context,
                    lock,
                ),
            })
        };
        let outcome = match partner {
            LockPartner::Npc(other) => run(Some(other)),
            LockPartner::Player(player) => match self
                .roster
                .at(usize::from(player))
                .and_then(|handle| self.server.world_mut(self.world)?.entity_mut(handle))
            {
                Some(peer) => {
                    peer.movement = peer.movement.reseeded(&peer.state);
                    let outcome = run(Some(&mut peer.movement));
                    peer.movement.write_player_state(&mut peer.state);
                    outcome
                }
                None => run(None),
            },
            LockPartner::Gone => run(None),
        };
        self.deaths.rng = rng;
        outcome
    }

    /// A lock an NPC won knocked player `player` down, or credited the NPC with it.
    pub(super) fn locked_down(
        &mut self,
        player: u16,
        until: Option<i32>,
        other_killer: Option<(u16, i32, i32)>,
    ) {
        let Some(peer) = self.peer_at(player) else {
            return;
        };
        if let Some(until) = until {
            peer.knockdown.hand_extend_time = until;
        }
        if let Some((number, time, debounce_time)) = other_killer {
            peer.wounds.other_killer = sjk_game_jka::damage::OtherKiller {
                number,
                time,
                debounce_time,
            };
        }
    }
}
