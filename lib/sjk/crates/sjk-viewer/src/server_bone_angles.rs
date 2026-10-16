//! Bones a server turns on a non-humanoid actor: a droid's head toward what it looks at
//! (`NPC_SetBoneAngles` on the server, `CG_G2ServerBoneAngles` on the client,
//! `cg_players.c:3960-4011`, called from `CG_G2PlayerAngles` for every skeleton that is
//! not the humanoid one, `:4376`). The entity carries up to four `CS_G2BONES` indices and
//! their angles; each named bone is turned after its animation.
use crate::actor_mesh::ActorMesh;
use sjk_protocol::{EntityState, GameState};

/// `CS_G2BONES` (`bg_public.h:141`).
const CS_G2BONES: usize = 1_163;
/// The entity's bone slots (`boneIndex1`..`boneIndex4`).
const SLOTS: usize = 4;

/// Turns the bones `state` names on `mesh`'s skeleton, the names read from `game_state`'s
/// `CS_G2BONES` strings. A slot of index zero, a name the configstrings lack or a bone the
/// skeleton lacks is skipped, as `CG_G2ServerBoneAngles` skips it; a turn once installed
/// stays until the server sends another, as it stays on the client's Ghoul2 instance.
pub(crate) fn apply(mesh: &mut ActorMesh, state: &EntityState, game_state: &GameState) {
    let orient = state.bone_orient();
    for slot in 0..SLOTS {
        let index = usize::from(state.bone_index(slot));
        if index == 0 {
            continue;
        }
        let Some(name) = game_state
            .config_string(CS_G2BONES + index)
            .filter(|name| !name.is_empty())
        else {
            continue;
        };
        let Some(bone) = mesh
            .preview
            .animation
            .bones
            .iter()
            .position(|bone| bone.name.as_bytes().eq_ignore_ascii_case(name))
        else {
            continue;
        };
        // A bone of the skeleton always takes a command; nothing to report otherwise.
        let _ = mesh.animator.set_server_bone_angles(
            &mesh.preview.animation,
            bone,
            state.bone_angles(slot),
            orient,
        );
    }
}
