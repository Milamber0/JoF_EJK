//! Game-facing Ghoul2 bolt matrices.
//!
//! `sjk_model::Glm::surface_bolt_matrix` reproduces the renderer-internal
//! transform (`G2_ProcessSurfaceBolt2` / `G2_GetBoltMatrixLow` in
//! `codemp/rd-vanilla/tr_ghoul2.cpp`). That raw matrix is what Ghoul2 itself
//! uses to attach child models (`G2_ConstructGhoulSkeleton`), so weapon hilts
//! are placed with it unchanged.
//!
//! Game code never sees that matrix directly. Multiplayer
//! `G2API_GetBoltMatrix` (`codemp/rd-vanilla/G2_API.cpp`, "lots of game code
//! is written to assume this 90 degree offset thing") rotates the basis 90°
//! about the bolt's Z axis before returning it, and every `codemp` consumer —
//! `CG_AddSaberBlade`'s `NEGATIVE_Y` blade forward, `CG_AddPlayerWeapon`'s
//! `POSITIVE_X` muzzle direction, `WP_SaberPositionUpdate` on the server — is
//! written against the rotated basis. Only the `_NoRecNoRot` entry point
//! (vehicle muzzles) skips it. Anything that mirrors a cgame/game bolt read
//! must go through [`game_facing`].

/// A Ghoul2 3x4 bolt matrix: rows are `x, y, z`, columns are the basis
/// vectors and the origin.
pub(crate) type BoltMatrix = [[f32; 4]; 3];

/// Convert a raw Ghoul2 bolt matrix into the basis multiplayer game code
/// receives from `G2API_GetBoltMatrix`.
///
/// Column 0 becomes the negated raw column 1 and column 1 becomes the raw
/// column 0; the Z column and the origin are untouched. This is exactly the
/// three `ftemp` swaps at the end of `G2API_GetBoltMatrix`.
pub(crate) fn game_facing(raw: BoltMatrix) -> BoltMatrix {
    std::array::from_fn(|row| [-raw[row][1], raw[row][0], raw[row][2], raw[row][3]])
}

/// Read one basis column of a game-facing bolt matrix, mirroring
/// `BG_GiveMeVectorFromMatrix` (`codemp/game/bg_misc.c`).
pub(crate) fn column(matrix: &BoltMatrix, index: usize) -> [f32; 3] {
    [matrix[0][index], matrix[1][index], matrix[2][index]]
}
