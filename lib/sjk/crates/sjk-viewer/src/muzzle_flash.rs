//! Codemp third-person muzzle event tracking and `*flash` socket conversion.
//!
//! `CG_AddPlayerWeapon` reads child model bolt zero through
//! `G2API_GetBoltMatrix`, then selects `ORIGIN` and `POSITIVE_X`
//! (`codemp/cgame/cg_weapons.c:716-745`). Child attachment itself remains in
//! raw renderer space; only the final cgame-visible bolt passes through
//! [`crate::bolt::game_facing`].

use crate::bolt::{self, BoltMatrix};
use glam::{Quat, Vec3};

pub(crate) const FLASH_BOLT: &str = "*flash";

/// World-space result returned to cgame by the renderer bolt API.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Socket {
    pub(crate) origin: [f32; 3],
    pub(crate) direction: [f32; 3],
}

/// Transform a weapon-local raw bolt through its raw child attachment, then
/// perform the one required renderer-to-game basis conversion.
pub(crate) fn world_socket(
    raw_local: BoltMatrix,
    weapon_origin: Vec3,
    weapon_rotation: Quat,
) -> Option<Socket> {
    let raw_world = std::array::from_fn(|row| {
        let axes = [0, 1, 2].map(|column| {
            weapon_rotation
                * Vec3::new(
                    raw_local[0][column],
                    raw_local[1][column],
                    raw_local[2][column],
                )
        });
        let origin = weapon_origin
            + weapon_rotation * Vec3::new(raw_local[0][3], raw_local[1][3], raw_local[2][3]);
        [axes[0][row], axes[1][row], axes[2][row], origin[row]]
    });
    socket_from_raw(raw_world)
}

/// Mirror the `BG_GiveMeVectorFromMatrix(ORIGIN/POSITIVE_X)` pair in
/// `CG_AddPlayerWeapon` after `G2API_GetBoltMatrix`'s 90-degree Z offset.
pub(crate) fn socket_from_raw(raw: BoltMatrix) -> Option<Socket> {
    let game = bolt::game_facing(raw);
    let direction = Vec3::from_array(bolt::column(&game, 0));
    (direction.length_squared() > 0.5).then_some(Socket {
        origin: bolt::column(&game, 3),
        direction: direction.normalize().to_array(),
    })
}
