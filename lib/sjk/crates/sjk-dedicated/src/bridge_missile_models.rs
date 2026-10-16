//! A missile's run through the players' and NPCs' models and sabers
//! (`d_projectileGhoul2Collision` 1, [`sjk_game_jka::weapon_fire::MissileModels`]).
//!
//! The missile is swept through the map, the boxes `gather_obstacles` collects and the
//! other players' saber entities. When a player's or an NPC's box stops it, its posed
//! model decides (`SV_ClipMoveToEntities`' Ghoul2 half,
//! [`sjk_game_jka::entity_clip::clip_move_to_entities_ghoul2`]): a missile that misses
//! the model flies on, and one that hits it stamps the struck surface, which places the
//! damage. A missile that meets a saber entity is turned aside by the blade
//! ([`sjk_game_jka::saber_block::block_on_saber`]).

use super::bridge_saber::PlayerSkeleton;
use super::{HomingPeers, NativeGame, Told};
use crate::collision::{Void, WithPlayers, WorldCollision};
use sjk_game_jka::crt_rand::CrtRand;
use sjk_game_jka::entity_clip::{BoxObstacle, ModelAnswer, Move, clip_move_to_entities_ghoul2};
use sjk_game_jka::pmove::{MovementCollision, MovementTrace};
use sjk_game_jka::saber_block::{Defender, SaberBlock, block_missile, block_on_saber};
use sjk_game_jka::server_skeleton::CollisionQuery;
use sjk_game_jka::weapon_fire::{
    HomingTarget, Missile, MissileFrame, MissileModels, MissileRun, run_missile_against,
};
use sjk_server::{Server, WorldId};
use std::cell::RefCell;

/// `MAX_CLIENTS`: the entities that are players.
const MAX_CLIENTS: u16 = 32;
/// `g_g2TraceLod`.
const TRACE_LOD: usize = 3;
/// `FP_SABER_DEFENSE`.
const FP_SABER_DEFENSE: usize = 16;

/// The players as a missile's run reaches them, borrowed once for the run and lent to
/// the sweep, the impact and the saber blocks in turn.
struct Players<'a> {
    server: RefCell<&'a mut Server<(), crate::peer::Peer>>,
    world: WorldId,
    roster: &'a crate::players::PlayerRoster,
    work: RefCell<&'a mut super::bridge_saber_damage::SaberWork>,
    rand: RefCell<&'a mut CrtRand>,
    /// The NPCs and their posed models ([`super::bridge_npcs::Npcs::missile_collide`]).
    npcs: RefCell<&'a mut super::bridge_npcs::Npcs>,
    shooter_origin: [f32; 3],
    level_time: i32,
}

impl Players<'_> {
    /// `G2API_CollisionDetect` on a player's posed model, as the engine's Ghoul2 half asks
    /// it: at the frame's time, facing the view's yaw, at the trace LOD.
    fn collide(
        &self,
        obstacle: &BoxObstacle,
        start: [f32; 3],
        end: [f32; 3],
        radius: f32,
    ) -> ModelAnswer {
        if obstacle.entity >= MAX_CLIENTS {
            // An NPC's model at its current origin and yaw (`sv_world.cpp:737-765`).
            return self.npcs.borrow_mut().missile_collide(
                obstacle.entity,
                start,
                end,
                radius,
                self.level_time,
            );
        }
        let mut server = self.server.borrow_mut();
        let Some(handle) = self.roster.at(usize::from(obstacle.entity)) else {
            return ModelAnswer::NoModel;
        };
        let Some(peer) = server
            .world_mut(self.world)
            .and_then(|world| world.entity_mut(handle))
        else {
            return ModelAnswer::NoModel;
        };
        let (origin, yaw) = (peer.state.origin(), peer.state.view_angles()[1]);
        let Some(PlayerSkeleton {
            models, skeleton, ..
        }) = peer.skeleton.as_mut()
        else {
            return ModelAnswer::NoModel;
        };
        let query = CollisionQuery {
            origin,
            yaw,
            time: self.level_time,
            start,
            end,
            lod: TRACE_LOD,
            radius,
        };
        let mut work = self.work.borrow_mut();
        let (scratch, records) = work.buffers();
        match skeleton.collide(&**models, &query, scratch, records) {
            Ok(_) => records
                .first()
                .map_or(ModelAnswer::Miss, |record| ModelAnswer::Hit {
                    position: record.position,
                    normal: record.normal,
                    surface: record.surface as u32,
                }),
            Err(_) => ModelAnswer::NoModel,
        }
    }

    /// `G_MissileImpact`'s block by a player's saber (`WP_SaberCanBlock`), or by the
    /// saber entity numbered `saber` when that is what the missile met.
    fn defend(
        &self,
        client: u16,
        missile: &mut Missile,
        normal: [f32; 3],
        on_saber: bool,
    ) -> Option<SaberBlock> {
        {
            let mut npcs = self.npcs.borrow_mut();
            if let Some(npc) = npcs
                .roster
                .actors
                .iter_mut()
                .find(|npc| npc.number == client)
            {
                let mut rand = self.rand.borrow_mut();
                return sjk_game_jka::npc_missile_block::npc_missile_defence(
                    npc,
                    missile,
                    normal,
                    on_saber,
                    self.shooter_origin,
                    self.level_time,
                    &mut rand,
                );
            }
        }
        let mut server = self.server.borrow_mut();
        let handle = self.roster.at(usize::from(client))?;
        let defender = server.world_mut(self.world)?.entity_mut(handle)?;
        if defender.health <= 0 {
            return None;
        }
        let defense = defender
            .session
            .force
            .as_ref()
            .map_or(0, |force| force.levels[FP_SABER_DEFENSE]);
        let mut view = Defender {
            client,
            state: &mut defender.state,
            saber_blocking: defender.movement.state().saber_blocking,
            buttons: defender.last_command.buttons,
            forward_move: defender.last_command.forward_move,
            defense,
            block_time: &mut defender.block_time,
        };
        let mut rand = self.rand.borrow_mut();
        if on_saber {
            block_on_saber(
                &mut view,
                missile,
                normal,
                self.shooter_origin,
                self.level_time,
                &mut rand,
            )
        } else {
            block_missile(
                &mut view,
                missile,
                normal,
                self.shooter_origin,
                self.level_time,
                &mut rand,
            )
        }
    }
}

/// The world a missile is swept through: the map, the boxes, and the players' models.
struct WithModels<'a, W: MovementCollision + crate::collision::BrushSweep> {
    world: W,
    obstacles: &'a [BoxObstacle],
    players: &'a Players<'a>,
}

impl<W: MovementCollision + crate::collision::BrushSweep> MovementCollision for WithModels<'_, W> {
    fn point_contents(&self, point: [f32; 3]) -> u32 {
        self.world.point_contents(point)
    }

    /// `SV_Trace` with `G2TRFLAG_DOGHOULTRACE | G2TRFLAG_GETSURFINDEX | G2TRFLAG_THICK |
    /// G2TRFLAG_HITCORPSES`: a model is tested with half the box's width, at least one.
    fn trace(
        &self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> MovementTrace {
        let world = self.world.trace(start, mins, maxs, end, mask);
        let Ok(bounds) = sjk_bsp::Aabb::new(mins, maxs) else {
            return world;
        };
        let movement = Move {
            ends: (start, end),
            bounds: (mins, maxs),
            content_mask: mask,
        };
        let radius = if mins[0] != 0.0 || maxs[0] != 0.0 {
            (maxs[0] - mins[0]) / 2.0
        } else {
            0.0
        };
        let radius = radius.max(1.0);
        let sweep = |obstacle: &BoxObstacle| {
            crate::collision::sweep_obstacle(&self.world, obstacle, start, bounds, end, mask)
        };
        clip_move_to_entities_ghoul2(
            world,
            movement,
            self.obstacles.iter().copied(),
            sweep,
            &mut |obstacle| self.players.collide(obstacle, start, end, radius),
        )
    }
}

/// The run's answers for [`MissileModels`]: the struck surface, the plain re-trace and
/// the saber entities.
struct RunModels<'a, W: MovementCollision + crate::collision::BrushSweep> {
    players: &'a Players<'a>,
    boxes: WithPlayers<'a, W>,
}

impl<W: MovementCollision + crate::collision::BrushSweep> MissileModels for RunModels<'_, W> {
    fn struck(&mut self, number: u16, surface: u32, level_time: i32) -> bool {
        if number >= MAX_CLIENTS {
            return self
                .players
                .npcs
                .borrow_mut()
                .stamp_struck(number, surface, level_time);
        }
        let mut server = self.players.server.borrow_mut();
        let Some(handle) = self.players.roster.at(usize::from(number)) else {
            return false;
        };
        let Some(peer) = server
            .world_mut(self.players.world)
            .and_then(|world| world.entity_mut(handle))
        else {
            return false;
        };
        if peer.skeleton.is_none() {
            return false;
        }
        peer.saber_cut.stamp_surface(surface as usize, level_time);
        true
    }

    fn plain_trace(
        &self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> MovementTrace {
        self.boxes.trace(start, mins, maxs, end, mask)
    }

    fn saber_block(
        &mut self,
        number: u16,
        missile: &mut Missile,
        normal: [f32; 3],
    ) -> Option<(u16, SaberBlock)> {
        // An NPC's lit saber entity turns it aside as a player's does.
        let npc_owner = self
            .players
            .npcs
            .borrow()
            .roster
            .actors
            .iter()
            .find(|npc| npc.saber_entity == Some(number) && npc.saber.entity_solid())
            .map(|npc| npc.number);
        if let Some(owner) = npc_owner {
            return self
                .players
                .defend(owner, missile, normal, true)
                .map(|block| (owner, block));
        }
        let owner = {
            let server = self.players.server.borrow();
            let world = server.world(self.players.world)?;
            (0..self.players.roster.places() as u16).find(|client| {
                self.players
                    .roster
                    .at(usize::from(*client))
                    .and_then(|handle| world.entity(handle))
                    .is_some_and(|peer| {
                        peer.state.saber_entity_num() == number && peer.saber_cut.entity_solid()
                    })
            })?
        };
        self.players
            .defend(owner, missile, normal, true)
            .map(|block| (owner, block))
    }
}

impl NativeGame {
    /// The players as a homing rocket asks after its enemy, as they stand now.
    pub(super) fn gather_homing_targets(&mut self) {
        let Self {
            server,
            world,
            players,
            homing,
            ..
        } = self;
        homing.clear();
        let Some(world) = server.world(*world) else {
            return;
        };
        for handle in players.holders() {
            let peer = handle
                .and_then(|handle| world.entity(handle))
                .filter(|peer| peer.begun && peer.playing());
            homing.push(peer.map(|peer| {
                let (bottom, top) = peer.movement.box_bounds();
                let top_z = peer.corpse.map_or(top[2], |corpse| corpse.1);
                HomingTarget {
                    origin: peer.state.origin(),
                    middle_height: (bottom[2] + top_z) * 0.5,
                    alive: peer.health > 0,
                    on_ground: peer.state.ground_entity_num() != 1_023,
                }
            }));
        }
    }

    /// `G_RunMissile` for one missile at `server_time`, through the map, the boxes, the
    /// saber entities and the players' models; a player it strikes may block it with the
    /// saber (`WP_SaberCanBlock`), a blade it meets turns it aside.
    pub(super) fn run_one_missile(
        &mut self,
        missile: &mut Missile,
        server_time: i32,
        previous_time: i32,
    ) -> MissileRun {
        let owner = usize::from(missile.owner);
        self.gather_obstacles(owner);
        self.gather_saber_entities(owner);
        // Where the shooter stands, a player or an NPC: a blocked bolt goes back at it.
        let shooter_origin = self.origin_of(missile.owner).unwrap_or([0.0; 3]);
        let Self {
            server,
            world,
            players,
            map,
            obstacles,
            rand,
            homing,
            deaths,
            sounds,
            told,
            pool,
            saber_work,
            npcs,
            ..
        } = self;
        let homing_targets = HomingPeers { targets: homing };
        let players = Players {
            server: RefCell::new(server),
            world: *world,
            roster: players,
            work: RefCell::new(saber_work),
            rand: RefCell::new(rand),
            npcs: RefCell::new(npcs),
            shooter_origin,
            level_time: server_time,
        };
        // An NPC struck is a client struck (`g_missile.c:966-975`).
        let is_npc = |number: u16| players.npcs.borrow().roster.is_npc(number);
        let mut defence = |struck: u16, missile: &mut Missile, normal: [f32; 3]| {
            players.defend(struck, missile, normal, false)
        };
        let mut sounds = |name: &[u8]| {
            sounds.index(name, &mut |index, value| {
                told.push(Told::ConfigString {
                    index,
                    previous: Vec::new(),
                    value: value.to_vec(),
                })
            })
        };
        let mut raise = |event: sjk_game_jka::event_entity::EventEntity| {
            let _ = pool.spawn_temporary(event.state(), server_time, None);
        };
        match map {
            Some(map) => {
                let bsp = || WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                };
                let mut models = RunModels {
                    players: &players,
                    boxes: WithPlayers {
                        world: bsp(),
                        players: obstacles,
                    },
                };
                let mut frame = MissileFrame {
                    homing: &homing_targets,
                    rng: &mut deaths.rng,
                    sounds: &mut sounds,
                    raise: &mut raise,
                    models: Some(&mut models),
                    npcs: &is_npc,
                };
                let sweep = WithModels {
                    world: bsp(),
                    obstacles,
                    players: &players,
                };
                run_missile_against(
                    missile,
                    server_time,
                    previous_time,
                    &sweep,
                    &mut defence,
                    &mut frame,
                )
            }
            None => {
                let mut models = RunModels {
                    players: &players,
                    boxes: WithPlayers {
                        world: Void,
                        players: obstacles,
                    },
                };
                let mut frame = MissileFrame {
                    homing: &homing_targets,
                    rng: &mut deaths.rng,
                    sounds: &mut sounds,
                    raise: &mut raise,
                    models: Some(&mut models),
                    npcs: &is_npc,
                };
                let sweep = WithModels {
                    world: Void,
                    obstacles,
                    players: &players,
                };
                run_missile_against(
                    missile,
                    server_time,
                    previous_time,
                    &sweep,
                    &mut defence,
                    &mut frame,
                )
            }
        }
    }
}
