//! The players as an NPC's blade meets them on this server ([`sjk_game_jka::npc_saber`]):
//! their posed models (`G_G2TraceCollide`), what each is to the blade (`CheckSaberDamage`'s
//! victim rules), their lit saber entities in the blade's way and their sabers in a clash
//! (`CheckSaberDamage`'s clash branch, whose change to a player's saber is made at once).
//! An NPC's blows on players are dealt as its turn ends ([`NpcOutcome::SaberBlow`]).

use super::super::bridge_saber::PlayerSkeleton;
use super::host::ServerHost;
use super::*;
use sjk_game_jka::npc_senses::Body;
use sjk_game_jka::saber_clash::Fighter;
use sjk_game_jka::saber_damage::{Ghoul2Answer, SaberVictim};
use sjk_game_jka::server_skeleton::CollisionQuery;

/// `g_g2TraceLod`.
const TRACE_LOD: usize = 3;
/// `CONTENTS_LIGHTSABER`.
const CONTENTS_LIGHTSABER: u32 = 0x4_0000;
/// `NPCTEAM_ENEMY`, `NPCTEAM_PLAYER`: a player's `playerTeam` (`g_client.c:3794-3811`).
const NPCTEAM_ENEMY: i32 = 1;
const NPCTEAM_PLAYER: i32 = 2;
/// `EF_DISINTEGRATION`.
const EF_DISINTEGRATION: u32 = sjk_game_jka::disruptor::EF_DISINTEGRATION;

impl ServerHost<'_> {
    /// Player `client` in the world, for a change.
    pub(super) fn peer_mut(&mut self, client: u16) -> Option<&mut crate::peer::Peer> {
        let handle = self.roster.at(usize::from(client))?;
        self.server.world_mut(self.world)?.entity_mut(handle)
    }

    /// Player `client` in the world, begun and playing.
    fn playing(&self, client: u16) -> Option<&crate::peer::Peer> {
        let handle = self.roster.at(usize::from(client))?;
        self.server
            .world(self.world)?
            .entity(handle)
            .filter(|peer| peer.begun && peer.playing())
    }

    /// A player's `playerTeam`: an enemy for the NPCs only on a siege's first team.
    fn player_team(&self, peer: &crate::peer::Peer) -> i32 {
        if self.gametype == GAMETYPE_SIEGE && peer.session.team == 1 {
            NPCTEAM_ENEMY
        } else {
            NPCTEAM_PLAYER
        }
    }

    /// `G_G2TraceCollide` on player `player`'s posed model: the first record struck, its
    /// surface stamped as the one struck (`g2LastSurfaceHit`).
    pub(super) fn collide_player_body(
        &mut self,
        player: u16,
        start: [f32; 3],
        end: [f32; 3],
        radius: f32,
        level_time: i32,
    ) -> Ghoul2Answer {
        let Some(handle) = self.roster.at(usize::from(player)) else {
            return Ghoul2Answer::NoModel;
        };
        let Some(peer) = self
            .server
            .world_mut(self.world)
            .and_then(|world| world.entity_mut(handle))
        else {
            return Ghoul2Answer::NoModel;
        };
        let (origin, yaw) = (peer.state.origin(), peer.state.view_angles()[1]);
        let Some(PlayerSkeleton {
            models, skeleton, ..
        }) = peer.skeleton.as_mut()
        else {
            return Ghoul2Answer::NoModel;
        };
        let query = CollisionQuery {
            origin,
            yaw,
            time: level_time,
            start,
            end,
            lod: TRACE_LOD,
            radius,
        };
        let super::bodies::NpcBodies {
            scratch, records, ..
        } = &mut *self.bodies;
        match skeleton.collide(models, &query, scratch, records) {
            Ok(_) => match records.first() {
                Some(record) => {
                    peer.saber_cut.stamp_surface(record.surface, level_time);
                    Ghoul2Answer::Hit {
                        position: record.position,
                        normal: record.normal,
                    }
                }
                None => Ghoul2Answer::Miss,
            },
            Err(error) => {
                eprintln!("client {player}'s collision: {error}");
                Ghoul2Answer::NoModel
            }
        }
    }

    /// What player `player` is to the NPC `swinger`'s blade: a client that can be hurt while
    /// it plays; spared an idle touch only by a power duel's teammate (`OnSameTeam` with an
    /// NPC, whose duel team is none); untouchable while it duels someone else.
    pub(super) fn player_victim(&self, player: u16, swinger: &Body) -> Option<SaberVictim> {
        // A breakable brush; a saber entity is not a victim but a saber owner.
        if self
            .breakables
            .iter()
            .any(|(ours, _)| ours.legacy_number() == player)
        {
            return Some(SaberVictim {
                takes_damage: true,
                health: 1,
                ..SaberVictim::default()
            });
        }
        let handle = self.roster.at(usize::from(player))?;
        let peer = self.server.world(self.world)?.entity(handle)?;
        let state = &peer.state;
        let legs = state.leg_animation();
        let length = peer
            .skeleton
            .as_ref()
            .map_or(0, |skeleton| skeleton.models.animation_length(legs));
        Some(SaberVictim {
            client: true,
            takes_damage: peer.begun && peer.playing(),
            health: peer.health,
            disintegrated: state.entity_flags() & EF_DISINTEGRATION != 0,
            spared_by_idle: self.gametype == GAMETYPE_POWERDUEL && peer.session.duel_team == 0,
            duel_elsewhere: state.duel_in_progress() && state.duel_index() != swinger.number,
            knocked_down_on_ground: sjk_game_jka::saber_rules::knocked_down_on_ground(
                legs,
                state.legs_timer(),
                length,
            ),
            player_team: self.player_team(peer),
        })
    }

    /// The player whose linked, lit saber entity is `saber_entity`, as a clash reads it.
    pub(super) fn player_fighter(&self, saber_entity: u16) -> Option<Fighter> {
        (0..self.roster.places()).find_map(|client| {
            let peer = self.playing(client as u16)?;
            (peer.state.saber_entity_num() == saber_entity && peer.saber_cut.entity_solid()).then(
                || Fighter {
                    player_team: self.player_team(peer),
                    ..super::super::bridge_saber_damage::fighter(client, peer)
                },
            )
        })
    }

    /// A clash's change to a player's saber, made by an NPC's blade.
    pub(super) fn set_player_fighter(&mut self, owner: &Fighter) {
        if let Some(peer) = self.peer_mut(owner.number) {
            super::super::bridge_saber_damage::set_saber_state(peer, owner);
        }
    }

    /// Every player's linked, lit saber entity, as an NPC's blade meets it.
    pub(super) fn saber_boxes(&self, out: &mut Vec<BoxObstacle>) {
        for client in 0..self.roster.places() {
            let Some(peer) = self.playing(client as u16) else {
                continue;
            };
            let entity = peer.state.saber_entity_num();
            if peer.saber_cut.entity_solid() && entity != 0 {
                let (origin, mins, maxs, _) = peer.saber_cut.entity;
                out.push(BoxObstacle {
                    entity,
                    origin,
                    bounds: (mins, maxs),
                    contents: CONTENTS_LIGHTSABER,
                    model: None,
                });
            }
        }
    }
}
