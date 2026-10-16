//! `G2API_GetBoltMatrix` on a posed model, shared by the server's skeletons and the
//! client's actors: a bolt read from the bone matrices a pose evaluation produced, then
//! placed in the world the way the engine places it (`G2_API.cpp`'s `G2API_GetBoltMatrix`:
//! Ghoul2's quarter-turn root, the model scale on the translation, the rows normalized,
//! the world matrix of the angles and origin asked for, and the multiplayer 90° column
//! swap that `BG_GiveMeVectorFromMatrix` reads its axes from).
//!
//! The server reads a bolt on its own skeleton at the Ghoul2 clock
//! ([`crate::server_skeleton::ServerSkeleton::bolt_matrix`]); a client reads the same bolt
//! on the model it posed for drawing (`CG_Player`'s `BG_AttachToRancor`,
//! `cg_players.c:9220-9244`). Both go through [`model_bolt`] and [`world_bolt`].

use crate::server_skeleton::{GHOUL2_ROOT, multiply, normalize_rows, world_matrix};
use sjk_model::{Gla, Glm, ModelError};

/// A bolt of `body` posed as `matrices` (one per bone of `gla`), in model space: a surface
/// bolt (`*name`) through its surface, a bone bolt through the bone's matrix and its base
/// pose (`G2_GetBoltMatrixLow`, `tr_ghoul2.cpp:3132`). `None` where the model has no such
/// bolt (the engine then answers the model's origin).
pub fn model_bolt(
    body: &Glm,
    gla: &Gla,
    bolt: &str,
    matrices: &[[[f32; 4]; 3]],
) -> Result<Option<[[f32; 4]; 3]>, ModelError> {
    if bolt.starts_with('*') {
        return body.surface_bolt_matrix(bolt, 0, matrices);
    }
    let bone = gla
        .bones
        .iter()
        .position(|bone| bone.name.eq_ignore_ascii_case(bolt));
    Ok(bone.and_then(|bone| Some(multiply(*matrices.get(bone)?, gla.bones[bone].base_pose))))
}

/// A model-space bolt (`raw`, from [`model_bolt`]) as `G2API_GetBoltMatrix` returns it for
/// a model at `origin` turned by `angles` (most callers pass the yaw alone) and scaled by `scale` (`modelScale`; a zero
/// axis is unscaled): the world matrix, with the multiplayer column swap applied.
pub fn world_bolt(
    raw: [[f32; 4]; 3],
    angles: [f32; 3],
    origin: [f32; 3],
    scale: [f32; 3],
) -> [[f32; 4]; 3] {
    let mut placed = multiply(GHOUL2_ROOT, raw);
    for (row, scale) in placed.iter_mut().zip(scale) {
        if scale != 0.0 {
            row[3] *= scale;
        }
    }
    let world = multiply(world_matrix(angles, origin), normalize_rows(placed));
    // "this is horribly stupid and I hate it. But lots of game code is written to assume
    // this 90 degree offset thing." Column 0 becomes the negated column 1.
    std::array::from_fn(|row| [-world[row][1], world[row][0], world[row][2], world[row][3]])
}
