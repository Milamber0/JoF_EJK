//! What an NPC's blade sweeps through ([`BladeWorld`], `CheckSaberDamage` for an NPC
//! swinger, `w_saber.c:3835-5250`): the map and the players (the host's), every other NPC's
//! body and its posed model, and every lit saber entity — a player's (the host's) or
//! another NPC's; and `WP_SaberApplyDamage`'s blows (`w_saber.c:3540-3586`).

use crate::damage::{Attacker, DamageRequest, MOD_SABER};
use crate::entity_clip::BoxObstacle;
use crate::npc_damage::NpcBlow;
use crate::npc_saber::{CONTENTS_LIGHTSABER, npc_fighter, set_npc_saber};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use crate::player_death::Rng;
use crate::pmove::MovementTrace;
use crate::saber_clash::{ClashWorld, Fighter};
use crate::saber_damage::{Ghoul2Answer, SaberHits, SaberTargets, SaberVictim, WallBounce};

/// `EF_DISINTEGRATION`'s wire field (`s.eFlags`).
const ES_EFLAGS: usize = 19;
/// `GT_POWERDUEL`, `GT_SINGLE_PLAYER`, `GT_TEAM`; `TEAM_FREE`.
const GT_POWERDUEL: i32 = 4;
const GT_SINGLE_PLAYER: i32 = 5;
const GT_TEAM: i32 = 6;
const TEAM_FREE: i32 = 0;
/// `PERS_HITS`, `PERS_ATTACKEE_ARMOR`.
const PERS_HITS: usize = 1;
const PERS_ATTACKEE_ARMOR: usize = 7;
/// `STAT_MAX_HEALTH` as the NPCs' attackers read it.
const STAT_MAX_HEALTH: usize = 8;

/// `OnSameTeam` between two NPCs (`g_team.c:206-288`): a power duel's duel teams (an
/// NPC's is none, so any two), no bot rule in a single-player game (neither is a bot), none
/// below the team games, and there the same session team unless both are free.
fn npcs_on_same_team(gametype: i32, one: i32, other: i32) -> bool {
    match gametype {
        GT_POWERDUEL | GT_SINGLE_PLAYER => true,
        _ if gametype < GT_TEAM => false,
        _ => one == other && !(one == TEAM_FREE && other == TEAM_FREE),
    }
}

/// The world an NPC's blade sweeps: its level, and the obstacles gathered for its sweep —
/// the other NPCs' bodies and every lit saber entity but its own.
pub(crate) struct BladeWorld<'w, 'a, H: NpcHost> {
    world: &'w mut NpcWorld<'a, H>,
    /// The swinger's index among the actors, and its entity number.
    swinger: usize,
    number: u16,
}

impl<'w, 'a, H: NpcHost> BladeWorld<'w, 'a, H> {
    /// The world for the NPC at `swinger`'s sweep, its obstacles gathered into the level's
    /// scratch.
    pub(crate) fn new(world: &'w mut NpcWorld<'a, H>, swinger: usize) -> Self {
        let number = world.actors[swinger].number;
        let NpcWorld {
            actors,
            bodies,
            host,
            ..
        } = &mut *world;
        bodies.clear();
        bodies.extend(
            actors
                .iter()
                .filter(|npc| npc.number != number && npc.contents != 0)
                .map(|npc| npc.body()),
        );
        for npc in actors
            .iter()
            .filter(|npc| npc.number != number && npc.saber.entity_solid())
        {
            let Some(saber) = npc.saber_entity else {
                continue;
            };
            let entity = &npc.saber.entity;
            bodies.push(BoxObstacle {
                entity: saber,
                origin: entity.origin,
                bounds: (entity.mins, entity.maxs),
                contents: CONTENTS_LIGHTSABER,
                model: None,
            });
        }
        host.player_saber_boxes(bodies);
        Self {
            world,
            swinger,
            number,
        }
    }

    /// Whether `number` is a player's entity.
    fn player(&self, number: u16) -> bool {
        self.world
            .host
            .players()
            .iter()
            .any(|player| player.number == number)
    }
}

impl<H: NpcHost> SaberTargets for BladeWorld<'_, '_, H> {
    fn trace(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> MovementTrace {
        let NpcWorld { host, bodies, .. } = &mut *self.world;
        host.trace(start, mins, maxs, end, self.number, mask, bodies)
    }

    fn collide(
        &mut self,
        number: u16,
        start: [f32; 3],
        end: [f32; 3],
        radius: f32,
    ) -> Ghoul2Answer {
        let level_time = self.world.level_time;
        if let Some(at) = self.world.actor_at(number) {
            let NpcWorld { host, actors, .. } = &mut *self.world;
            return host.collide_npc(&actors[at], start, end, radius, level_time);
        }
        if self.player(number) {
            return self
                .world
                .host
                .collide_player(number, start, end, radius, level_time);
        }
        Ghoul2Answer::NoModel
    }

    fn victim(&self, number: u16) -> Option<SaberVictim> {
        let swinger = &self.world.actors[self.swinger];
        if let Some(at) = self.world.actor_at(number) {
            let npc = &self.world.actors[at];
            let legs = npc.player.leg_animation();
            let length = npc
                .movement
                .animation_lengths()
                .and_then(|lengths| lengths.length_ms(legs))
                .unwrap_or(0);
            return Some(SaberVictim {
                client: true,
                takes_damage: npc.takes_damage,
                health: npc.health,
                disintegrated: npc.state.raw_field(ES_EFLAGS).unwrap_or(0)
                    & crate::disruptor::EF_DISINTEGRATION
                    != 0,
                spared_by_idle: npcs_on_same_team(
                    self.world.host.gametype(),
                    swinger.session_team,
                    npc.session_team,
                ),
                duel_elsewhere: false,
                knocked_down_on_ground: crate::saber_rules::knocked_down_on_ground(
                    legs,
                    npc.player.legs_timer(),
                    length,
                ),
                player_team: npc.player_team,
            });
        }
        let body = self.world.npc(self.swinger);
        self.world.host.player_saber_victim(number, &body)
    }

    fn saber_owner(&self, number: u16) -> Option<Fighter> {
        let owner = self.world.actors.iter().find(|npc| {
            npc.saber_entity == Some(number)
                && npc.number != self.number
                && npc.saber.entity_solid()
        });
        match owner {
            Some(npc) => Some(npc_fighter(npc)),
            None => self.world.host.player_saber(number),
        }
    }

    fn set_saber_owner(&mut self, owner: &Fighter) {
        match self.world.actor_at(owner.number) {
            Some(at) => set_npc_saber(&mut self.world.actors[at], owner),
            None => self.world.host.set_player_saber(owner),
        }
    }

    fn wall_bounce(&mut self, bounce: &WallBounce) {
        // `SFL_BOUNCE_ON_WALLS` on an NPC's saber: none of the stock NPCs' sabers has it,
        // installed custom ones do ([`crate::npc_saber_bounce`]).
        self.world.npc_saber_wall_bounce(self.swinger, bounce);
    }
}

impl<H: NpcHost> ClashWorld for BladeWorld<'_, '_, H> {
    fn debug_saber_locks(&self) -> bool {
        self.world.host.debug_saber_locks()
    }

    fn rng(&mut self) -> &mut Rng {
        self.world.host.rng()
    }

    fn check_lock(&mut self, _me: u16, other: u16) -> bool {
        // `WP_SabersCheckLock(self, other)` with the swinging NPC first.
        self.world.npc_check_lock(self.swinger, other)
    }

    fn knock_out(&mut self, owner: u16, velocity: [f32; 3]) -> bool {
        // `saberKnockOutOfHand`: an NPC's saber through its flight, a player's through the host.
        let level_time = self.world.level_time;
        match self.world.actor_at(owner) {
            Some(at) => self
                .world
                .with_npc_saber(at, |saber, world| {
                    crate::saber_drop::knock_out_of_hand(saber, world, velocity, level_time)
                })
                .unwrap_or(false),
            None => self.world.host.knock_player_saber(owner, velocity),
        }
    }

    fn smash(&mut self, owner: u16, striker: u16, damage: i32) -> bool {
        // `saberCheckKnockdown_Smashed`: an NPC's thrown saber through its flight, a
        // player's through the host; the striker (this NPC) defending when its move is one
        // of the extra defence's.
        let level_time = self.world.level_time;
        let defending = crate::saber_rules::in_extra_defense(
            self.world.actors[self.swinger].player.saber_move(),
        );
        match self.world.actor_at(owner) {
            Some(at) => self
                .world
                .with_npc_saber(at, |saber, world| {
                    crate::saber_drop::smashed(saber, world, striker, defending, damage, level_time)
                })
                .unwrap_or(false),
            None => self
                .world
                .host
                .smash_player_saber(owner, striker, defending, damage),
        }
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `WP_SaberApplyDamage` for the NPC at `me`: each victim's total as one `G_Damage` —
    /// an NPC's placed by the surface its model was struck on, a player's through the host
    /// — the NPC's hit counter kept (`PERS_HITS`).
    pub(crate) fn deal_saber_blows(&mut self, me: usize, hits: &SaberHits) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        let number = npc.number;
        let sabers = &npc.definition.sabers;
        let attacker = Attacker {
            npc: true,
            client: number,
            max_health: npc.player.stats[STAT_MAX_HEALTH] as i32,
            team: npc.session_team,
            saber_knockback: [
                sabers[0].knockback_scale[0],
                sabers[0].knockback_scale[1],
                sabers[1].knockback_scale[0],
                sabers[1].knockback_scale[1],
            ],
        };
        let (actors, host) = (&*self.actors, &*self.host);
        let is_client = |victim: u16| {
            actors.iter().any(|npc| npc.number == victim)
                || host.players().iter().any(|player| player.number == victim)
        };
        let mut blows = [None; 16];
        for (slot, blow) in blows.iter_mut().zip(hits.blows(&is_client)) {
            *slot = Some(blow);
        }
        for blow in blows.into_iter().flatten() {
            let request = DamageRequest {
                level_time,
                attacker: Some(attacker),
                direction: Some(blow.direction),
                point: Some(blow.spot),
                damage: blow.damage,
                flags: blow.flags,
                means: MOD_SABER,
            };
            let (hits, armor) = if let Some(at) = self.actor_at(blow.victim) {
                self.host.noting_damage(blow.victim, number, &request);
                let surface = self.host.npc_surface_location(
                    &self.actors[at],
                    blow.flags,
                    blow.spot,
                    level_time,
                );
                let damaged = self.damage(
                    at,
                    NpcBlow {
                        request,
                        spared_by_master: false,
                        surface,
                    },
                );
                (damaged.attacker_hits, damaged.attackee_armor)
            } else if self
                .host
                .players()
                .iter()
                .any(|player| player.number == blow.victim)
            {
                self.host.saber_blow_on_player(blow.victim, request)
            } else {
                self.host.saber_blow_on_entity(blow.victim, request);
                (0, None)
            };
            if hits != 0
                && let Some(at) = self.actor_at(number)
            {
                let persistent = &mut self.actors[at].player.persistent;
                persistent[PERS_HITS] = (persistent[PERS_HITS] as i32 + hits) as u32;
                persistent[PERS_ATTACKEE_ARMOR] = armor.unwrap_or(0);
            }
        }
    }
}
