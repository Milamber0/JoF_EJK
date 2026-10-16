//! A player a monster holds, drawn where the monster holds it, and the camera that watches.
//!
//! The server places a held player (`EF2_HELD_BY_MONSTER`, its holder its look target) at
//! the rancor's hand or jaw every command (`G_HeldByMonster`, `g_active.c:1552-1587`), but
//! the client does not draw it at that place: `CG_Player` reads the same bolt on the
//! monster *it* posed this frame and moves the player there, turned by the bolt
//! (`BG_AttachToRancor`, `cg_players.c:9220-9244`) — for every client entity, the local
//! predicted player among them. Only the rancor holds anyone in multiplayer: the wampa's
//! grab is single-player code (`NPC_AI_Wampa.c` never sets `EF2_HELD_BY_MONSTER`).
//!
//! The third-person camera of a held local player stands 120 units off and looks along
//! the rancor's facing turned about (`cg_view.c:371-378`, `652-664`).
use crate::actor_mesh::ActorMesh;
use sjk_protocol::{EntityState, PlayerState, Snapshot};
use sjk_runtime::{EntityId, World};

/// `EF2_HELD_BY_MONSTER`, `EF2_GENERIC_NPC_FLAG` (`bg_public.h:682-687`).
const EF2_HELD_BY_MONSTER: u32 = 1 << 0;
const EF2_GENERIC_NPC_FLAG: u32 = 1 << 3;
/// `CLASS_RANCOR` (`teams.h`).
const CLASS_RANCOR: u8 = 54;
/// The entity state's `eFlags2`, `hasLookTarget`, `lookTarget` (protocol-26 netfields).
const ES_EFLAGS2: usize = 96;
const ES_HAS_LOOK_TARGET: usize = 66;
const ES_LOOK_TARGET: usize = 52;
/// The player state's.
const PS_EFLAGS2: usize = 103;
const PS_HAS_LOOK_TARGET: usize = 76;
const PS_LOOK_TARGET: usize = 66;
/// `thirdPersonRange` of a held player (`cg_view.c:377`).
pub(crate) const HELD_CAMERA_RANGE: f32 = 120.0;

/// The monster entity holding the player `state` describes (the local player's own
/// state, `player`, when `state` is `None`), if it is held.
pub(crate) fn holder(player: &PlayerState, state: Option<&EntityState>) -> Option<u16> {
    let (flags, has, target) = match state {
        Some(state) => [ES_EFLAGS2, ES_HAS_LOOK_TARGET, ES_LOOK_TARGET]
            .map(|index| state.raw_field(index).unwrap_or(0))
            .into(),
        None => [PS_EFLAGS2, PS_HAS_LOOK_TARGET, PS_LOOK_TARGET]
            .map(|index| player.raw_field(index).unwrap_or(0))
            .into(),
    };
    (flags & EF2_HELD_BY_MONSTER != 0 && has != 0).then_some(target as u16)
}

/// Where `BG_AttachToRancor` puts a victim of the monster entity `number`: the world
/// place and the angles (pitch, yaw, roll) of the victim, read from the monster's mesh as
/// posed this frame — its jaw while its victim is in its mouth, its right hand otherwise.
/// `None` when the monster is not drawn or its model lacks the bolt.
pub(crate) fn attach(
    world: &World,
    meshes: &[ActorMesh],
    snapshot: &Snapshot,
    number: u16,
    time: i64,
) -> Option<([f32; 3], [f32; 3])> {
    let id = EntityId::new(u64::from(number) + 1);
    let entity = world.entity(id)?;
    let mesh = meshes.iter().find(|mesh| mesh.entity_id == Some(id))?;
    let state = snapshot
        .entities
        .iter()
        .find(|state| state.number() == number)?;
    let in_mouth = state.raw_field(ES_EFLAGS2).unwrap_or(0) & EF2_GENERIC_NPC_FLAG != 0;
    let bolt = sjk_game_jka::npc_creature::rancor_attach_bolt(in_mouth);
    let raw = sjk_game_jka::ghoul2_bolt::model_bolt(
        &mesh.preview.mesh,
        &mesh.preview.animation,
        bolt,
        mesh.animator.matrices(),
    )
    .ok()??;
    // `modelScale` from `iModelScale`, a percentage; zero leaves the model unscaled.
    let scale = state.model_scale_percent() as f32 / 100.0;
    let yaw = entity.sample_pose(time)?.view_angles_degrees[1];
    let matrix = sjk_game_jka::ghoul2_bolt::world_bolt(
        raw,
        [0.0, yaw, 0.0],
        entity.sample(time).translation,
        [scale; 3],
    );
    Some(sjk_game_jka::npc_creature::attach_to_rancor(
        matrix, in_mouth,
    ))
}

/// `CG_Player`'s placement of a held player: `transform` moved to its holder's bolt and
/// turned to its angles. `state` is the player's snapshot state, `None` for the local
/// player, whose own player state is read.
pub(crate) fn place(
    world: &World,
    meshes: &[ActorMesh],
    snapshot: &Snapshot,
    state: Option<&EntityState>,
    transform: &mut sjk_runtime::Transform,
    time: i64,
) {
    let Some(number) = holder(&snapshot.player, state) else {
        return;
    };
    if let Some((origin, angles)) = attach(world, meshes, snapshot, number, time) {
        transform.translation = origin;
        transform.rotation = sjk_client::legacy_angles_to_quaternion(angles);
    }
}

/// The third-person camera's yaw of a held local player: the rancor's facing turned about
/// (`AngleNormalize180(monster->lerpAngles[YAW] + 180)`), in degrees; `None` when the
/// player is not held by a rancor, and the camera is its own.
pub(crate) fn camera_yaw(world: &World, snapshot: &Snapshot, time: i64) -> Option<f32> {
    let number = holder(&snapshot.player, None)?;
    let state = snapshot
        .entities
        .iter()
        .find(|state| state.number() == number)?;
    if state.npc_class() != CLASS_RANCOR {
        return None;
    }
    let yaw = world
        .entity(EntityId::new(u64::from(number) + 1))?
        .sample_pose(time)?
        .view_angles_degrees[1];
    Some(angle_normalize_180(yaw + 180.0))
}

/// `AngleNormalize180`.
fn angle_normalize_180(angle: f32) -> f32 {
    let angle = (360.0 / 65_536.0) * (((angle * (65_536.0 / 360.0)) as i32) & 65_535) as f32;
    if angle > 180.0 { angle - 360.0 } else { angle }
}
