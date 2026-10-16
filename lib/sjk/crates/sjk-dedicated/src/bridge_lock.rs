//! The server's saber locks: the lock checked when two blades clash
//! (`WP_SabersCheckLock` inside `CheckSaberDamage`), the lock's part of a think before
//! the move, the locked move with the opponent's movement moved too, and the duel's
//! loss after it. The rules are [`sjk_game_jka::saber_lock`]'s and
//! [`sjk_game_jka::pmove_saber_lock`]'s; this is where the server's players meet them.

use super::NativeGame;
use crate::collision::{Void, WithPlayers, WorldCollision};
use crate::map::LoadedMap;
use crate::peer::Peer;
use sjk_game_jka::damage::{Attacker, DamageRequest, MOD_SABER};
use sjk_game_jka::entity_clip::BoxObstacle;
use sjk_game_jka::player_death::Rng;
use sjk_game_jka::pmove::Predictor;
use sjk_game_jka::pmove::{MovementCollision, MovementTrace};
use sjk_game_jka::saber_lock::{LockFighter, LockMemory, LockWorld};
use sjk_protocol::{PlayerState, UserCommand};

/// `FP_RAGE`.
const FP_RAGE: usize = 8;
/// `ps.saberLockTime`, `saberLockFrame`, `saberLockEnemy`.
const PS_SABER_LOCK_TIME: usize = 107;
const PS_SABER_LOCK_FRAME: usize = 108;
const PS_SABER_LOCK_ENEMY: usize = 110;
/// `ps.forceHandExtend` and its knockdown.
const PS_FORCE_HAND_EXTEND: usize = 80;
const HANDEXTEND_KNOCKDOWN: u32 = 8;

/// The map and every other player but the two locking, for the lock's traces.
struct LockTraces<'a> {
    map: Option<&'a LoadedMap>,
    others: &'a mut Vec<BoxObstacle>,
}

impl LockWorld for LockTraces<'_> {
    fn trace(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        _skip: u16,
        mask: u32,
        other: BoxObstacle,
    ) -> MovementTrace {
        // The one not moving is where it stands now; it is taken out again after.
        self.others.push(other);
        let players = &*self.others;
        let trace = match self.map {
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
        };
        self.others.pop();
        trace
    }
}

/// Player `number` as a lock reads and changes it: its wire state and lock memory, with
/// its box and blocking from its movement. Taken field by field, so that the movement's
/// animation table can be read beside it.
fn fighter<'a>(
    number: usize,
    state: &'a mut PlayerState,
    lock: &'a mut LockMemory,
    movement: &Predictor,
    command: &UserCommand,
    lockable: bool,
) -> LockFighter<'a> {
    LockFighter {
        origin_linked: false,
        lockable,
        animation_scales: movement.state().saber_anim_speed_scales.0,
        number: number as u16,
        bounds: movement.box_bounds(),
        saber_blocking: movement.state().saber_blocking,
        command_angles: command.angles,
        hits: &mut lock.hits,
        // Only an NPC in the lock reads the teams (`NPCTEAM_PLAYER`).
        npc: false,
        player_team: 2,
        clip_mask: sjk_game_jka::saber_lock::MASK_PLAYERSOLID,
        state,
    }
}

/// Whether `peer` is in a lock, or has a lock's frame left over: its move is the
/// locked one.
pub(super) fn in_lock(peer: &Peer) -> bool {
    peer.state.raw_field(PS_SABER_LOCK_TIME).unwrap_or(0) != 0
        || peer.state.raw_field(PS_SABER_LOCK_FRAME).unwrap_or(0) != 0
}

impl NativeGame {
    /// `WP_SabersCheckLock` between `first`, whose blade clashed, and `second`, with the
    /// game's generator the clash draws from: whether they locked. The two restart
    /// their movement from what the lock made of them.
    pub(super) fn check_lock(
        &mut self,
        first: usize,
        second: usize,
        rng: &mut Rng,
        level_time: i32,
    ) -> bool {
        // `g_saberLocking`, on by default.
        const LOCKING: bool = true;
        if second >= self.players.places() {
            return self.check_lock_with_npc(first, second as u16, rng, level_time);
        }
        let forced = self.debug_saber_locks();
        let Self {
            server,
            world,
            players,
            map,
            obstacles,
            lock_obstacles,
            gametype,
            ..
        } = self;
        // The obstacles were gathered for `first`; the lock passes `second` too.
        lock_obstacles.clear();
        lock_obstacles.extend(
            obstacles
                .iter()
                .filter(|obstacle| usize::from(obstacle.entity) != second)
                .copied(),
        );
        let handles = players.at(first).zip(players.at(second));
        let pair = handles
            .zip(server.world_mut(*world))
            .and_then(|((one, two), world)| world.entity_pair_mut(one, two));
        let Some((one, two)) = pair else { return false };
        // No lock without the poses' lengths to hold them by.
        let Some(lengths) = one.movement.animation_lengths() else {
            return false;
        };
        let mut traces = LockTraces {
            map: map.as_ref(),
            others: lock_obstacles,
        };
        let lockable =
            [&*one, &*two].map(|peer| peer.sabers.lockable(peer.state.saber_holstered()));
        let mut first_fighter = fighter(
            first,
            &mut one.state,
            &mut one.lock,
            &one.movement,
            &one.last_command,
            lockable[0],
        );
        let mut second_fighter = fighter(
            second,
            &mut two.state,
            &mut two.lock,
            &two.movement,
            &two.last_command,
            lockable[1],
        );
        let locked = if forced {
            sjk_game_jka::saber_lock::forced_lock(
                &mut first_fighter,
                &mut second_fighter,
                rng,
                lengths,
                &mut traces,
                level_time,
            )
        } else {
            sjk_game_jka::saber_lock::check_lock(
                &mut first_fighter,
                &mut second_fighter,
                *gametype,
                LOCKING,
                rng,
                lengths,
                &mut traces,
                level_time,
            )
        };
        if locked {
            for peer in [one, two] {
                peer.movement = peer.movement.reseeded(&peer.state);
            }
        }
        locked
    }

    /// `WP_SabersCheckLock(first, npc)` for a player whose blade met an NPC's saber: the
    /// player first, the NPC's side its actor's ([`sjk_game_jka::npc_saber_lock`]).
    fn check_lock_with_npc(
        &mut self,
        first: usize,
        npc: u16,
        rng: &mut Rng,
        level_time: i32,
    ) -> bool {
        let forced = self.debug_saber_locks();
        let Self {
            server,
            world,
            players,
            map,
            obstacles,
            lock_obstacles,
            gametype,
            npcs,
            ..
        } = self;
        let Some(actor) = npcs
            .roster
            .actors
            .iter_mut()
            .find(|actor| actor.number == npc)
        else {
            return false;
        };
        let Some(peer) = players
            .at(first)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle))
        else {
            return false;
        };
        let Some(lengths) = actor.movement.shared_animation_lengths() else {
            return false;
        };
        lock_obstacles.clear();
        lock_obstacles.extend(
            obstacles
                .iter()
                .filter(|obstacle| obstacle.entity != npc)
                .copied(),
        );
        let lockable = peer.sabers.lockable(peer.state.saber_holstered());
        // Its `playerTeam`: `NPCTEAM_ENEMY` on a siege's first side, else `NPCTEAM_PLAYER`.
        let team = if *gametype == super::GAMETYPE_SIEGE && peer.session.team == 1 {
            1
        } else {
            2
        };
        let mut player = LockFighter {
            player_team: team,
            ..fighter(
                first,
                &mut peer.state,
                &mut peer.lock,
                &peer.movement,
                &peer.last_command,
                lockable,
            )
        };
        let mut traces = LockTraces {
            map: map.as_ref(),
            others: lock_obstacles,
        };
        let mut npc_fighter = sjk_game_jka::npc_saber_lock::npc_lock_fighter(actor);
        let locked = if forced {
            sjk_game_jka::saber_lock::forced_lock(
                &mut player,
                &mut npc_fighter,
                rng,
                lengths.as_ref(),
                &mut traces,
                level_time,
            )
        } else {
            sjk_game_jka::saber_lock::check_lock(
                &mut player,
                &mut npc_fighter,
                *gametype,
                true,
                rng,
                lengths.as_ref(),
                &mut traces,
                level_time,
            )
        };
        let origin_linked = npc_fighter.origin_linked;
        if locked {
            peer.movement = peer.movement.reseeded(&peer.state);
            sjk_game_jka::npc_saber_lock::npc_locked(actor, origin_linked);
        }
        locked
    }

    /// `ClientThink_real`'s lock before the move (`g_active.c:2917-3001`): facing the
    /// opponent, an attack press counted, the weight pushing the lock.
    pub(super) fn lock_think(&mut self, client: usize, command: &UserCommand, level_time: i32) {
        let Some(peer) = self.peer(client) else {
            return;
        };
        let (time, frame) = (
            peer.state.raw_field(PS_SABER_LOCK_TIME).unwrap_or(0) as i32,
            peer.state.raw_field(PS_SABER_LOCK_FRAME).unwrap_or(0),
        );
        if time <= level_time && frame == 0 {
            return;
        }
        let enemy = peer.state.raw_field(PS_SABER_LOCK_ENEMY).unwrap_or(0) as usize;
        let opponent = (enemy != client)
            .then(|| self.peer(enemy))
            .flatten()
            .map(|other| other.state.origin());
        let Self {
            server,
            world,
            players,
            deaths,
            ..
        } = self;
        let Some(peer) = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle))
        else {
            return;
        };
        let (rage, recovery) = (
            peer.force.levels[FP_RAGE],
            peer.state.force_rage_recovery_time(),
        );
        let bonus = peer.sabers.lock_bonus(peer.state.saber_holstered());
        sjk_game_jka::saber_lock::think(
            &mut peer.state,
            &mut peer.lock,
            opponent,
            command.angles,
            rage,
            recovery,
            bonus,
            level_time,
            &mut deaths.rng,
        );
        peer.movement = peer.movement.reseeded(&peer.state);
    }

    /// The opponent a locked player's move reaches: its lock enemy, when that is another
    /// player in the world. `None` for anybody not in a lock.
    pub(super) fn lock_enemy(&self, client: usize) -> Option<Option<usize>> {
        let peer = self.peer(client)?;
        if !in_lock(peer) {
            return None;
        }
        let enemy = peer.state.raw_field(PS_SABER_LOCK_ENEMY).unwrap_or(0) as usize;
        Some(
            (enemy != client && enemy < self.players.places() && self.peer(enemy).is_some())
                .then_some(enemy),
        )
    }

    /// `pmove.checkDuelLoss` after the move (`g_active.c:3072-3107`): a loser as weak as
    /// a draw of 0 to 40 says is finished outright — still, the pose dropped — and dies
    /// holding its hand. One not knocked down (a superbreak's or a circle lock's loser)
    /// may lose its saber (`saberCheckKnockdown_DuelLoss`).
    pub(super) fn duel_loss(&mut self, client: usize, loser: u16, level_time: i32) {
        if self.npcs.roster.is_npc(loser) {
            self.npc_lost_lock(client, loser, level_time);
            return;
        }
        let loser = usize::from(loser);
        let Some(health) = self.peer(loser).map(|peer| peer.health) else {
            return;
        };
        if self.deaths.rng.irand(0, 40) > health {
            let Some(winner) = self.peer(client) else {
                return;
            };
            let (origin, max_health, team) = (
                winner.state.origin(),
                winner.state.max_health(),
                winner.session.team,
            );
            let Some(victim) = self.peer_mut(loser) else {
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
            let attacker = Attacker {
                npc: false,
                client: client as u16,
                max_health,
                team,
                saber_knockback: [0.0; 4],
            };
            let request = DamageRequest {
                level_time,
                attacker: Some(attacker),
                direction: Some(direction),
                point: Some(at),
                damage: 9_999,
                flags: sjk_game_jka::triggers::DAMAGE_NO_PROTECTION,
                means: MOD_SABER,
            };
            self.avoid_dismember = true;
            let _ = self.hurt(loser, request);
            self.avoid_dismember = false;
            self.dismember_lock_loser(loser, client as u16, level_time);
        } else if let Some(victim) = self.peer(loser) {
            // Standing (a superbreak's or a circle lock's loser): the saber may fly.
            let knocked =
                victim.state.raw_field(PS_FORCE_HAND_EXTEND).unwrap_or(0) == HANDEXTEND_KNOCKDOWN;
            if !knocked && victim.state.saber_entity_num() != 0 {
                let _ = self.disarm_duel_loss(loser, client, level_time);
                if let Some(victim) = self.peer_mut(loser) {
                    victim.movement = victim.movement.reseeded(&victim.state);
                }
            }
        }
    }

    /// `pmove.checkDuelLoss` for NPC `loser`, beaten by player `client`'s lock: finished or
    /// disarmed through the roster ([`sjk_game_jka::npc_roster::NpcRoster::lost_lock_to_player`]).
    fn npc_lost_lock(&mut self, client: usize, loser: u16, level_time: i32) {
        let Some(winner) = self.peer(client) else {
            return;
        };
        let (origin, max_health, team) = (
            winner.state.origin(),
            winner.state.max_health(),
            winner.session.team,
        );
        let (storage, chance) = (
            winner.saber_cut.storage,
            winner.sabers.disarm_chance(winner.state.saber_holstered()),
        );
        let attacker = Attacker {
            npc: false,
            client: client as u16,
            max_health,
            team,
            saber_knockback: [0.0; 4],
        };
        let fired = self
            .with_roster(|roster, _, host| {
                roster.lost_lock_to_player(
                    loser, attacker, origin, &storage, chance, level_time, host,
                )
            })
            .unwrap_or_default();
        self.fire_from_npcs(fired, level_time);
    }

    /// `g_debugSaberLocks` (a cheat): any two blades that meet lock.
    pub(super) fn debug_saber_locks(&self) -> bool {
        self.integer_cvar(b"g_debugSaberLocks", 0) != 0
    }
}
