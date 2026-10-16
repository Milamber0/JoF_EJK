//! The loaded map as the movement code collides with it.
use sjk_bsp::{Aabb, Bsp, TraceScratch};
use sjk_game_jka::entity_clip::{BoxObstacle, Move, clip_move_to_entities};
use sjk_game_jka::pmove::{ENTITY_NUMBER_WORLD, MovementCollision, MovementTrace};
use sjk_game_jka::saber_block::MissilePaths;
use std::cell::RefCell;

/// World brushes and patches only; movers and other players come with the entities
/// that are them. The scratch storage is the map's, reused for every trace.
pub struct WorldCollision<'a> {
    /// The map's brushes and patches.
    pub bsp: &'a Bsp,
    /// Trace storage sized for that map.
    pub scratch: &'a RefCell<TraceScratch>,
}

impl MovementCollision for WorldCollision<'_> {
    fn point_contents(&self, point: [f32; 3]) -> u32 {
        self.bsp.point_contents(point, u32::MAX)
    }

    fn trace(
        &self,
        start: [f32; 3],
        minimums: [f32; 3],
        maximums: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> MovementTrace {
        let Ok(bounds) = Aabb::new(minimums, maximums) else {
            // Movement only produces valid boxes; an invalid one goes nowhere.
            return MovementTrace {
                fraction: 0.0,
                all_solid: true,
                start_solid: true,
                ..MovementTrace::miss(start)
            };
        };
        let trace =
            self.bsp
                .trace_box_with(&mut self.scratch.borrow_mut(), start, end, bounds, mask);
        MovementTrace {
            fraction: trace.fraction,
            end_position: trace.end_position,
            plane_normal: trace.plane.map_or([0.0; 3], |plane| plane.normal),
            surface_flags: trace.surface_flags,
            start_solid: trace.start_solid,
            all_solid: trace.all_solid,
            // `SV_Trace`, sv_world.cpp:836.
            entity_number: if trace.fraction != 1.0 {
                ENTITY_NUMBER_WORLD
            } else {
                1_023
            },
        }
    }
}

/// No map loaded: nothing to run into.
pub struct Void;
impl MovementCollision for Void {
    fn trace(&self, _: [f32; 3], _: [f32; 3], _: [f32; 3], end: [f32; 3], _: u32) -> MovementTrace {
        MovementTrace::miss(end)
    }
}

/// A world that can sweep a move through one of its inline models: a door, a lift or a
/// breakable standing where the game has moved it (`CM_TransformedBoxTrace`).
pub trait BrushSweep {
    /// `obstacle`'s model at its origin against the box `bounds` swept from `start` to `end`.
    fn sweep_brush(
        &self,
        obstacle: &BoxObstacle,
        model: usize,
        start: [f32; 3],
        bounds: Aabb,
        end: [f32; 3],
        mask: u32,
    ) -> MovementTrace;
}

impl BrushSweep for WorldCollision<'_> {
    fn sweep_brush(
        &self,
        obstacle: &BoxObstacle,
        model: usize,
        start: [f32; 3],
        bounds: Aabb,
        end: [f32; 3],
        mask: u32,
    ) -> MovementTrace {
        let trace = self.bsp.trace_transformed_model(
            model,
            obstacle.origin,
            None,
            start,
            end,
            bounds,
            mask,
        );
        MovementTrace {
            fraction: trace.fraction,
            end_position: trace.end_position,
            plane_normal: trace.plane.map_or([0.0; 3], |plane| plane.normal),
            surface_flags: trace.surface_flags,
            start_solid: trace.start_solid,
            all_solid: trace.all_solid,
            entity_number: obstacle.entity,
        }
    }
}

impl BrushSweep for Void {
    /// No map, no models: nothing to run into.
    fn sweep_brush(
        &self,
        _: &BoxObstacle,
        _: usize,
        _: [f32; 3],
        _: Aabb,
        end: [f32; 3],
        _: u32,
    ) -> MovementTrace {
        MovementTrace::miss(end)
    }
}

/// One obstacle against a swept box: through its brush model where it is one, else its box.
pub fn sweep_obstacle(
    world: &impl BrushSweep,
    obstacle: &BoxObstacle,
    start: [f32; 3],
    bounds: Aabb,
    end: [f32; 3],
    mask: u32,
) -> MovementTrace {
    match obstacle.model {
        Some(model) => world.sweep_brush(obstacle, model, start, bounds, end, mask),
        None => sweep_box(obstacle, start, bounds, end, mask),
    }
}

/// The `func_usable` brushes that are there (`SP_func_usable` links them solid unless
/// they start off), each its own model standing at its origin.
pub fn usable_obstacles(
    usables: &[(
        sjk_game_jka::entity_id::EntityId,
        sjk_game_jka::use_key::Usable,
    )],
) -> impl Iterator<Item = BoxObstacle> + '_ {
    usables
        .iter()
        .filter(|(_, usable)| usable.contents != 0)
        .map(|(number, usable)| BoxObstacle {
            entity: number.legacy_number(),
            origin: usable.origin,
            bounds: usable.bounds,
            contents: usable.contents,
            model: Some(usable.model),
        })
}

/// The map's brush entities as obstacles, each traced through its own model: the
/// breakables still standing (solid to everybody, what a bolt strikes) and the doors and
/// lifts where their last frame left them (`G_RunMover` at `level_time`).
pub fn brush_obstacles<'a>(
    breakables: &'a [(
        sjk_game_jka::entity_id::EntityId,
        sjk_game_jka::breakables::Breakable,
    )],
    doors: &'a [(
        sjk_game_jka::entity_id::EntityId,
        sjk_game_jka::movers::Door,
    )],
    level_time: i32,
) -> impl Iterator<Item = BoxObstacle> + 'a {
    let breakables = breakables
        .iter()
        .filter(|(_, brush)| brush.contents != 0)
        .map(|(number, brush)| BoxObstacle {
            entity: number.legacy_number(),
            origin: [0.0; 3],
            bounds: brush.bounds,
            contents: brush.contents,
            model: Some(brush.model),
        });
    let doors = doors
        .iter()
        .filter(|(_, door)| door.contents != 0)
        .map(move |(number, door)| BoxObstacle {
            entity: number.legacy_number(),
            origin: sjk_game_jka::movers::origin_at(door, level_time),
            bounds: door.bounds,
            contents: door.contents,
            model: Some(door.model),
        });
    breakables.chain(doors)
}

/// A world and the players standing in it: `SV_Trace`. The obstacles are everyone but the
/// player who moves, where each was when it last thought.
pub struct WithPlayers<'a, W: MovementCollision> {
    /// The map, or nothing.
    pub world: W,
    /// The other players' boxes.
    pub players: &'a [BoxObstacle],
}

impl<W: MovementCollision + BrushSweep> WithPlayers<'_, W> {
    /// The map's trace clipped by `players`' boxes, as `SV_Trace` clips it.
    pub(crate) fn trace_through(
        &self,
        players: impl Iterator<Item = BoxObstacle>,
        start: [f32; 3],
        minimums: [f32; 3],
        maximums: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> MovementTrace {
        let world = self.world.trace(start, minimums, maximums, end, mask);
        let Ok(bounds) = Aabb::new(minimums, maximums) else {
            return world;
        };
        let movement = Move {
            ends: (start, end),
            bounds: (minimums, maximums),
            content_mask: mask,
        };
        clip_move_to_entities(world, movement, players, |player| {
            sweep_obstacle(&self.world, player, start, bounds, end, mask)
        })
    }
}

/// One box obstacle against a swept box (`CM_TransformedBoxTrace` on a box model).
pub fn sweep_box(
    player: &BoxObstacle,
    start: [f32; 3],
    bounds: Aabb,
    end: [f32; 3],
    mask: u32,
) -> MovementTrace {
    let Ok(target) = Aabb::new(player.bounds.0, player.bounds.1) else {
        return MovementTrace::miss(end);
    };
    let trace = sjk_bsp::trace_box_against_box(
        start,
        end,
        bounds,
        target,
        player.origin,
        player.contents,
        mask,
    );
    MovementTrace {
        fraction: trace.fraction,
        end_position: trace.end_position,
        plane_normal: trace.plane.map_or([0.0; 3], |plane| plane.normal),
        surface_flags: 0,
        start_solid: trace.start_solid,
        all_solid: trace.all_solid,
        entity_number: player.entity,
    }
}

impl<W: MovementCollision + BrushSweep> MovementCollision for WithPlayers<'_, W> {
    fn point_contents(&self, point: [f32; 3]) -> u32 {
        self.world.point_contents(point)
    }

    fn trace(
        &self,
        start: [f32; 3],
        minimums: [f32; 3],
        maximums: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> MovementTrace {
        self.trace_through(
            self.players.iter().copied(),
            start,
            minimums,
            maximums,
            end,
            mask,
        )
    }
}

impl<W: MovementCollision + BrushSweep> MissilePaths for WithPlayers<'_, W> {
    /// The players here are everyone; the missile's owner is left out as its trace's
    /// pass entity leaves it out.
    fn trace_from(
        &self,
        owner: u16,
        start: [f32; 3],
        minimums: [f32; 3],
        maximums: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> MovementTrace {
        self.trace_through(
            self.players
                .iter()
                .copied()
                .filter(|player| player.entity != owner),
            start,
            minimums,
            maximums,
            end,
            mask,
        )
    }
}
