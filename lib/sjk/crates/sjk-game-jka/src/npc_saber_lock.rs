//! Saber locks with an NPC in them (OpenJK `codemp/game/w_saber.c:1201-1885`,
//! `g_active.c:2917-3001`): `WP_SabersCheckLock` between an NPC's blade and anyone's — the
//! players' rules ([`crate::saber_lock`]) with the NPC's side read from its actor
//! ([`npc_lock_fighter`]) — and the lock's part of an NPC's think.
//!
//! The reference lets an NPC lock with anyone not of its `playerTeam`, duelling or not; a
//! player it locks with is the host's ([`crate::npc_spawn::NpcHost::player_lock`]).

use crate::entity_clip::BoxObstacle;
use crate::npc_spawn::{NpcActor, NpcHost, es};
use crate::npc_world::NpcWorld;
use crate::pmove::MovementTrace;
use crate::saber_lock::{LockFighter, LockWorld};

/// `SFL_NOT_LOCKABLE`.
const SFL_NOT_LOCKABLE: u32 = 1 << 0;
/// `TR_STATIONARY`.
const TR_STATIONARY: u32 = 0;
/// `ps.saberLockEnemy`, `FP_RAGE`.
const PS_SABER_LOCK_ENEMY: usize = 110;
const FP_RAGE: usize = 8;

/// The NPC `npc` as a lock reads and changes it: its box, its move's blocking, its last
/// command's angles, its sabers' lock rules and animation speeds, its `playerTeam` and
/// `clipmask`.
pub fn npc_lock_fighter(npc: &mut NpcActor) -> LockFighter<'_> {
    let [first, second] = &npc.definition.sabers;
    let holstered = npc.player.saber_holstered();
    let lockable = first.flags & SFL_NOT_LOCKABLE == 0
        && !(second.is_held() && holstered == 0 && second.flags & SFL_NOT_LOCKABLE != 0);
    let held = |saber: &crate::saber_definition::SaberDefinition| {
        if saber.is_held() {
            saber.anim_speed_scale
        } else {
            1.0
        }
    };
    let animation_scales = [held(first), held(second)];
    LockFighter {
        origin_linked: false,
        number: npc.number,
        bounds: (npc.mins, npc.maxs),
        saber_blocking: npc.movement.state().saber_blocking,
        command_angles: npc.mind.command.angles,
        hits: &mut npc.saber.lock.hits,
        animation_scales,
        lockable,
        npc: true,
        player_team: npc.player_team,
        clip_mask: npc.clip_mask,
        state: &mut npc.player,
    }
}

/// What a lock begun leaves on an NPC beside its player state: `SetClientViewAngle`'s
/// `s.angles`, and — drawn to the lock's distance — `G_SetOrigin` and the link
/// (`w_saber.c:1418-1454`); its movement takes the state up. `origin_linked` is the
/// lock fighter's trace result: even an unchanged position resets the trajectory.
pub fn npc_locked(npc: &mut NpcActor, origin_linked: bool) {
    let angles = npc.player.view_angles();
    for (index, value) in es::ANGLES.into_iter().zip(angles) {
        npc.state.set_raw_field(index, value.to_bits());
    }
    let origin = npc.player.origin();
    if origin_linked {
        for (index, value) in es::POS_BASE.into_iter().zip(origin) {
            npc.state.set_raw_field(index, value.to_bits());
        }
        for index in es::POS_DELTA {
            npc.state.set_raw_field(index, 0);
        }
        npc.state.set_raw_field(es::POS_TYPE, TR_STATIONARY);
        npc.state.set_raw_field(es::POS_TIME, 0);
        npc.state.set_raw_field(es::POS_DURATION, 0);
        npc.current_origin = origin;
        npc.relink();
    }
    npc.movement = npc.movement.reseeded(&npc.player);
}

/// Whom an NPC in a lock pushes against in its move (`PM_SaberLocked`'s `genemy`).
pub enum LockPartner<'a> {
    /// Nobody in the game: the lock ends.
    Gone,
    /// Another NPC, its movement taken up from its player state.
    Npc(&'a mut crate::pmove::Predictor),
    /// A player, the host's.
    Player(u16),
}

/// The move of the NPC at `me` in a lock, or with its lock's frame left over (`Pmove` with
/// `PM_SaberLocked` reaching its opponent): the move through the host
/// ([`NpcHost::move_npc_locked`]), an NPC opponent's player state written from its
/// movement after. `None` for an NPC in no lock, whose move is the ordinary one.
pub(crate) fn locked_move<H: NpcHost>(
    actors: &mut [NpcActor],
    me: usize,
    host: &mut H,
    context: &crate::pmove::MoveContext,
    bodies: &[BoxObstacle],
) -> Option<crate::pmove_saber_lock::LockOutcome> {
    const PS_SABER_LOCK_FRAME: usize = 108;
    let npc = &actors[me];
    if npc.player.saber_lock_time() == 0
        && npc.player.raw_field(PS_SABER_LOCK_FRAME).unwrap_or(0) == 0
    {
        return None;
    }
    let (number, command, hits) = (npc.number, npc.mind.command, npc.saber.lock.hits);
    let enemy = npc.player.raw_field(PS_SABER_LOCK_ENEMY).unwrap_or(0) as u16;
    let Some(them) = actors
        .iter()
        .position(|other| other.number == enemy && other.number != number)
    else {
        let partner = if host.players().iter().any(|player| player.number == enemy) {
            LockPartner::Player(enemy)
        } else {
            LockPartner::Gone
        };
        return Some(host.move_npc_locked(
            &mut actors[me].movement,
            command,
            context,
            number,
            bodies,
            partner,
            hits,
        ));
    };
    let (low, high) = actors.split_at_mut(me.max(them));
    let (mine, theirs) = if me < them {
        (&mut low[me], &mut high[0])
    } else {
        (&mut high[0], &mut low[them])
    };
    theirs.movement = theirs.movement.reseeded(&theirs.player);
    let outcome = host.move_npc_locked(
        &mut mine.movement,
        command,
        context,
        number,
        bodies,
        LockPartner::Npc(&mut theirs.movement),
        hits,
    );
    write_movement(theirs);
    Some(outcome)
}

/// An NPC's player state taken from its movement, the health stat and the team kept whole
/// (the movement keeps them narrower than the game does).
fn write_movement(npc: &mut NpcActor) {
    const PERS_TEAM: usize = 3;
    let (health, team) = (npc.player.stats[0], npc.player.persistent[PERS_TEAM]);
    npc.movement.write_player_state(&mut npc.player);
    (npc.player.stats[0], npc.player.persistent[PERS_TEAM]) = (health, team);
}

/// An NPC a player's locked move pushed against (`PM_SaberLocked`'s `genemy`): its player
/// state taken from its movement (its health stat and team kept), and a break's knockdown
/// and credit for it.
pub fn pushed_by_player(npc: &mut NpcActor, outcome: &crate::pmove_saber_lock::LockOutcome) {
    write_movement(npc);
    if let Some(until) = outcome.knocked_down_until {
        npc.mind.knockdown.hand_extend_time = until;
    }
    if let Some((number, time, debounce_time)) = outcome.other_killer {
        npc.mind.fight.other_killer = crate::damage::OtherKiller {
            number,
            time,
            debounce_time,
        };
    }
}

/// The lock's traces through the host: its map and players, the NPCs but the two locking,
/// and `other` where it now stands.
pub struct HostLockWorld<'x, H: NpcHost> {
    pub host: &'x mut H,
    pub bodies: &'x mut Vec<BoxObstacle>,
    /// How many of `bodies` are the gathered NPCs (the rest is scratch for `other`).
    pub count: usize,
}

impl<H: NpcHost> LockWorld for HostLockWorld<'_, H> {
    fn trace(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        skip: u16,
        mask: u32,
        other: BoxObstacle,
    ) -> MovementTrace {
        self.bodies.truncate(self.count);
        self.bodies.push(other);
        self.host
            .trace(start, mins, maxs, end, skip, mask, self.bodies)
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `WP_SabersCheckLock(me, other)` for the NPC at `me`, whose blade met `other`'s — an
    /// NPC's here, a player's through the host: whether they locked, the lock begun.
    pub(crate) fn npc_check_lock(&mut self, me: usize, other: u16) -> bool {
        let level_time = self.level_time;
        let gametype = self.host.gametype();
        let number = self.actors[me].number;
        let NpcWorld {
            actors,
            bodies,
            host,
            ..
        } = &mut *self;
        bodies.clear();
        bodies.extend(
            actors
                .iter()
                .filter(|npc| npc.contents != 0 && npc.number != number && npc.number != other)
                .map(NpcActor::body),
        );
        let count = bodies.len();
        let Some(them) = actors.iter().position(|npc| npc.number == other) else {
            return host.player_lock(&mut actors[me], other, true, bodies, level_time);
        };
        let Some(lengths) = actors[me].movement.shared_animation_lengths() else {
            return false;
        };
        let mut rng = *host.rng();
        let (locked, linked) = {
            let (low, high) = actors.split_at_mut(me.max(them));
            let (mine, theirs) = if me < them {
                (&mut low[me], &mut high[0])
            } else {
                (&mut high[0], &mut low[them])
            };
            let forced = host.debug_saber_locks();
            let mut world = HostLockWorld {
                host: &mut **host,
                bodies,
                count,
            };
            let (mut mine, mut theirs) = (npc_lock_fighter(mine), npc_lock_fighter(theirs));
            let locked = if forced {
                crate::saber_lock::forced_lock(
                    &mut mine,
                    &mut theirs,
                    &mut rng,
                    lengths.as_ref(),
                    &mut world,
                    level_time,
                )
            } else {
                crate::saber_lock::check_lock(
                    &mut mine,
                    &mut theirs,
                    gametype,
                    true,
                    &mut rng,
                    lengths.as_ref(),
                    &mut world,
                    level_time,
                )
            };
            (locked, [mine.origin_linked, theirs.origin_linked])
        };
        *host.rng() = rng;
        if locked {
            npc_locked(&mut actors[me], linked[0]);
            npc_locked(&mut actors[them], linked[1]);
        }
        locked
    }

    /// What a lock's move left for the game (`g_active.c:3040-3107`): the one knocked down,
    /// and a lost duel's loser.
    pub(crate) fn lock_outcome(
        &mut self,
        me: usize,
        enemy: u16,
        outcome: crate::pmove_saber_lock::LockOutcome,
    ) {
        if outcome.lock_won {
            self.actors[me].saber.event_flags |= crate::saber_clash::sef::LOCK_WON;
        }
        match self.actor_at(enemy) {
            Some(them) => {
                let mind = &mut self.actors[them].mind;
                if let Some(until) = outcome.knocked_down_until {
                    mind.knockdown.hand_extend_time = until;
                }
                if let Some((number, time, debounce_time)) = outcome.other_killer {
                    mind.fight.other_killer = crate::damage::OtherKiller {
                        number,
                        time,
                        debounce_time,
                    };
                }
            }
            None if outcome.knocked_down_until.is_some() || outcome.other_killer.is_some() => self
                .host
                .player_locked_down(enemy, outcome.knocked_down_until, outcome.other_killer),
            None => {}
        }
        if let Some(loser) = outcome.duel_loss {
            self.npc_duel_loss(me, loser);
        }
    }

    /// `pmove.checkDuelLoss` after the NPC at `me` won a lock (`g_active.c:3072-3107`): a
    /// loser as weak as a draw of 0 to 40 says is finished outright; one standing may lose
    /// its saber (`saberCheckKnockdown_DuelLoss`) — an NPC's here, a player's through the
    /// host.
    fn npc_duel_loss(&mut self, me: usize, loser: u16) {
        let level_time = self.level_time;
        let winner = &self.actors[me];
        let (origin, storage, attacker) = (
            winner.player.origin(),
            winner.saber.storage,
            crate::npc_saber_throw::npc_attacker(winner),
        );
        let [first, second] = &winner.definition.sabers;
        let chance = 1
            + first.disarm_bonus[0]
            + if second.is_held() && winner.player.saber_holstered() == 0 {
                second.disarm_bonus[0]
            } else {
                0
            };
        let Some(them) = self.actor_at(loser) else {
            self.host
                .player_lost_lock(loser, attacker, origin, &storage, chance, level_time);
            return;
        };
        self.npc_lost_lock(them, attacker, origin, &storage, chance);
    }

    /// `pmove.checkDuelLoss` on the NPC at `them`, beaten in a lock by `attacker` standing at
    /// `origin` (`g_active.c:3072-3107`): finished on a weak draw — its sword hand then off
    /// (`gGAvoidDismember` 2) — else, standing, its saber perhaps knocked away by the
    /// winner's blade (`saberCheckKnockdown_DuelLoss`, `storage`, `chance`).
    pub(crate) fn npc_lost_lock(
        &mut self,
        them: usize,
        attacker: crate::damage::Attacker,
        origin: [f32; 3],
        storage: &crate::saber_clash::SaberStorage,
        chance: i32,
    ) {
        const HANDEXTEND_KNOCKDOWN: u32 = 8;
        const PS_FORCE_HAND_EXTEND: usize = 80;
        let level_time = self.level_time;
        let loser = self.actors[them].number;
        let health = self.actors[them].health;
        if self.host.irand(0, 40) > health {
            let victim = &mut self.actors[them];
            let at = victim.player.origin();
            victim.player.set_velocity([0.0; 3]);
            victim.player.set_raw_field(PS_FORCE_HAND_EXTEND, 0);
            victim.mind.knockdown.hand_extend_time = 0;
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
            let request = crate::damage::DamageRequest {
                level_time,
                attacker: Some(attacker),
                direction: Some(direction),
                point: Some(at),
                damage: 9_999,
                flags: crate::triggers::DAMAGE_NO_PROTECTION,
                means: crate::damage::MOD_SABER,
            };
            // `gGAvoidDismember`: no limb as the blow kills, then the sword hand of a loser
            // it killed (`g_active.c:3087-3097`).
            use crate::npc_dismember_check::{AvoidDismember, DismemberCheck};
            let winner = attacker.client;
            self.level.avoid_dismember = AvoidDismember::Always;
            let _ = self.damage(
                them,
                crate::npc_damage::NpcBlow {
                    request,
                    spared_by_master: false,
                    surface: None,
                },
            );
            if let Some(them) = self
                .actor_at(loser)
                .filter(|at| self.actors[*at].health < 1)
            {
                let victim = &self.actors[them];
                let check = DismemberCheck {
                    victim: loser,
                    enemy: winner,
                    point: victim.player.origin(),
                    damage: 999,
                    death_anim: victim.player.leg_animation(),
                    post_death: false,
                    avoid: AvoidDismember::RightHand,
                };
                self.check_for_dismemberment(check);
            }
            self.level.avoid_dismember = AvoidDismember::No;
        } else if self.actors[them]
            .player
            .raw_field(PS_FORCE_HAND_EXTEND)
            .unwrap_or(0)
            != HANDEXTEND_KNOCKDOWN
            && self.actors[them].player.saber_entity_num() != 0
        {
            let _ = self.with_npc_saber(them, |saber, world| {
                crate::saber_drop::duel_loss(saber, world, storage, chance, level_time)
            });
        }
    }

    /// `ClientThink_real`'s lock before the move for the NPC at `me` (`g_active.c:2917-3001`,
    /// [`crate::saber_lock::think`]): facing its lock's opponent, its attack presses
    /// weighed.
    pub(crate) fn npc_lock_think(&mut self, me: usize) {
        let level_time = self.level_time;
        let enemy = self.actors[me]
            .player
            .raw_field(PS_SABER_LOCK_ENEMY)
            .unwrap_or(0) as u16;
        let opponent = self
            .body(enemy)
            .filter(|_| enemy != self.actors[me].number)
            .map(|body| body.origin);
        let npc = &mut self.actors[me];
        let (rage, recovery) = (
            npc.force_levels
                .get(FP_RAGE)
                .copied()
                .unwrap_or(0)
                .clamp(0, 255) as u8,
            npc.movement.state().force_rage_recovery_time,
        );
        let [first, second] = &npc.definition.sabers;
        let bonus = first.lock_bonus
            + if second.is_held() && npc.player.saber_holstered() == 0 {
                second.lock_bonus
            } else {
                0
            };
        let angles = npc.mind.command.angles;
        let mut rng = *self.host.rng();
        let npc = &mut self.actors[me];
        let locked = npc.player.saber_lock_time() > level_time;
        crate::saber_lock::think(
            &mut npc.player,
            &mut npc.saber.lock,
            opponent,
            angles,
            rage,
            recovery,
            bonus,
            level_time,
            &mut rng,
        );
        *self.host.rng() = rng;
        // `SetClientViewAngle` turns the entity too.
        if locked && opponent.is_some() {
            let view = npc.player.view_angles();
            for (index, value) in es::ANGLES.into_iter().zip(view) {
                npc.state.set_raw_field(index, value.to_bits());
            }
        }
    }
}
