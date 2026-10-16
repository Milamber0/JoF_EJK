//! Clipping a move against the entities that stand in the world — other players — after
//! it has been clipped against the world: `SV_Trace` and `SV_ClipMoveToEntities` (OpenJK
//! `codemp/server/sv_world.cpp:640-840`). The sweep against one entity's box is the
//! collision model's (`sjk_bsp::trace_box_against_box`, held against the reference's
//! own); this module is the server's bookkeeping around it, which needs no map and so
//! takes the sweep as a function.

use crate::pmove::MovementTrace;

/// An entity a move can run into: a box standing at its origin, or a brush model's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxObstacle {
    /// The entity's number, which a trace that meets it reports.
    pub entity: u16,
    /// `r.currentOrigin`: where the entity was last linked.
    pub origin: [f32; 3],
    /// `r.mins` and `r.maxs`.
    pub bounds: ([f32; 3], [f32; 3]),
    /// `r.contents`: a living player is `CONTENTS_BODY`.
    pub contents: u32,
    /// The inline model a brush entity is made of (a door, a lift, a breakable), which a
    /// move is traced against (`CM_TransformedBoxTrace`) instead of the box, the box then
    /// being the model's own bounds; `None` for a box (a player, a missile).
    pub model: Option<usize>,
}

/// The move as `SV_Trace` is handed it.
#[derive(Clone, Copy, Debug)]
pub struct Move {
    /// Where the box starts and where it wants to go.
    pub ends: ([f32; 3], [f32; 3]),
    /// The moving box.
    pub bounds: ([f32; 3], [f32; 3]),
    /// What stops it.
    pub content_mask: u32,
}

/// `world` is the move clipped against the world. A move the world stops at once is not
/// looked at further; otherwise every obstacle whose box, grown by a unit, touches the
/// move's, grown by a unit, and is made of something the mask asks for is swept with
/// `sweep`, and the nearest stop wins — keeping what any of them knew about the start
/// being inside something.
///
/// The reference visits entities in the order of its sector tree; this takes them as
/// given. The order only decides between stops at exactly the same fraction.
pub fn clip_move_to_entities(
    world: MovementTrace,
    movement: Move,
    obstacles: impl IntoIterator<Item = BoxObstacle>,
    sweep: impl Fn(&BoxObstacle) -> MovementTrace,
) -> MovementTrace {
    let mut result = world;
    if result.fraction == 0.0 {
        return result;
    }
    let ((start, end), (mins, maxs)) = (movement.ends, movement.bounds);
    let (mut low, mut high) = ([0.0_f32; 3], [0.0_f32; 3]);
    for axis in 0..3 {
        let (from, to) = if end[axis] > start[axis] {
            (start[axis], end[axis])
        } else {
            (end[axis], start[axis])
        };
        (low[axis], high[axis]) = (from + mins[axis] - 1.0, to + maxs[axis] + 1.0);
    }
    for obstacle in obstacles {
        if result.all_solid {
            break;
        }
        // `SV_LinkEntity` grows an entity's box by a unit "because movement is clipped an
        // epsilon away from an actual edge"; `SV_AreaEntities` wants the two to overlap.
        let apart = (0..3).any(|axis| {
            obstacle.origin[axis] + obstacle.bounds.0[axis] - 1.0 > high[axis]
                || obstacle.origin[axis] + obstacle.bounds.1[axis] + 1.0 < low[axis]
        });
        if apart || movement.content_mask & obstacle.contents == 0 {
            continue;
        }
        let mut trace = sweep(&obstacle);
        trace.entity_number = obstacle.entity;
        if trace.all_solid {
            result.all_solid = true;
        } else if trace.start_solid {
            // "We want to get the number of an ent even if our trace starts inside it."
            (result.start_solid, result.entity_number) = (true, obstacle.entity);
        }
        if trace.fraction < result.fraction {
            let started_inside = result.start_solid;
            result = trace;
            result.start_solid |= started_inside;
        }
    }
    result
}

/// What a model says of a sweep that reached its entity (`G2API_CollisionDetect`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ModelAnswer {
    /// The entity has no model to test, or is not to be tested (a corpse, for a trace
    /// that does not ask for corpses): its box stands.
    NoModel,
    /// The model was missed: the sweep is as it was before this entity.
    Miss,
    /// The model was struck here, facing this way, on this surface.
    Hit {
        position: [f32; 3],
        normal: [f32; 3],
        surface: u32,
    },
}

/// [`clip_move_to_entities`] for a trace that asks for Ghoul2 (`G2TRFLAG_DOGHOULTRACE`,
/// `G2TRFLAG_GETSURFINDEX`), statement for statement as `SV_ClipMoveToEntities` has it
/// (`sv_world.cpp:538-796`). After each entity's box is merged, when that entity's own
/// trace names it, its model is asked along the whole sweep: a miss puts the sweep back
/// as it was before this entity; a hit moves the sweep's end and normal onto the model,
/// and — when the sweep names this entity — gives its surface as the surface flags.
///
/// An entity's own trace comes from `CM_Trace`, which zero-fills it: its entity number
/// is 0 unless the merge named it. So the entity numbered 0 — the first client — is asked
/// whenever its box is in the sweep's way at all, even when it was not the nearest, and
/// a hit on its model then moves the end of whatever the sweep holds.
pub fn clip_move_to_entities_ghoul2(
    world: MovementTrace,
    movement: Move,
    obstacles: impl IntoIterator<Item = BoxObstacle>,
    sweep: impl Fn(&BoxObstacle) -> MovementTrace,
    model: &mut dyn FnMut(&BoxObstacle) -> ModelAnswer,
) -> MovementTrace {
    let mut result = world;
    if result.fraction == 0.0 {
        return result;
    }
    let ((start, end), (mins, maxs)) = (movement.ends, movement.bounds);
    let (mut low, mut high) = ([0.0_f32; 3], [0.0_f32; 3]);
    for axis in 0..3 {
        let (from, to) = if end[axis] > start[axis] {
            (start[axis], end[axis])
        } else {
            (end[axis], start[axis])
        };
        (low[axis], high[axis]) = (from + mins[axis] - 1.0, to + maxs[axis] + 1.0);
    }
    for obstacle in obstacles {
        if result.all_solid {
            break;
        }
        let apart = (0..3).any(|axis| {
            obstacle.origin[axis] + obstacle.bounds.0[axis] - 1.0 > high[axis]
                || obstacle.origin[axis] + obstacle.bounds.1[axis] + 1.0 < low[axis]
        });
        if apart || movement.content_mask & obstacle.contents == 0 {
            continue;
        }
        let mut trace = sweep(&obstacle);
        // `CM_Trace`'s zeroed trace names entity 0 until the merge names this one.
        trace.entity_number = 0;
        let before = result;
        if trace.all_solid {
            result.all_solid = true;
            trace.entity_number = obstacle.entity;
        } else if trace.start_solid {
            (result.start_solid, result.entity_number) = (true, obstacle.entity);
            trace.entity_number = obstacle.entity;
        }
        if trace.fraction < result.fraction {
            let started_inside = result.start_solid;
            trace.entity_number = obstacle.entity;
            result = trace;
            result.start_solid |= started_inside;
        }
        if trace.entity_number != obstacle.entity {
            continue;
        }
        match model(&obstacle) {
            ModelAnswer::NoModel => {}
            ModelAnswer::Miss => result = before,
            ModelAnswer::Hit {
                position,
                normal,
                surface,
            } => {
                (result.end_position, result.plane_normal) = (position, normal);
                if result.entity_number == obstacle.entity {
                    result.surface_flags = surface;
                }
            }
        }
    }
    result
}
