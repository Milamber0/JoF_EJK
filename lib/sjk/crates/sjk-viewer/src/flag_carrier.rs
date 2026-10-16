//! CG_PlayerFlag / CG_PlayerPowerups (codemp/cgame/cg_players.c:4412-4535).
use super::*;

/// CG_RegisterGraphics (cg_main.c:1231-1249): CTF, then CTY; red then blue.
pub(crate) const MODELS: [[&str; 2]; 2] = [
    ["models/flags/r_flag.md3", "models/flags/b_flag.md3"],
    [
        "models/flags/r_flag_ysal.md3",
        "models/flags/b_flag_ysal.md3",
    ],
];

/// GT_CTY is 9, distinct from GT_CTF=8 (bg_public.h:235-247).
pub(crate) fn model_set(gametype: i32) -> usize {
    usize::from(gametype == 9)
}

/// Stock tests independent PW_REDFLAG/PW_BLUEFLAG bits, not the carrier's team.
pub(crate) fn carried(powerups: u32) -> [bool; 2] {
    [powerups & (1 << 4) != 0, powerups & (1 << 5) != 0]
}

/// CG_PlayerFlag hides the local first-person model; corpse submission is separate.
pub(crate) fn eligible(draw_actor: bool, kind: EntityKind) -> bool {
    draw_actor && kind == EntityKind::Actor
}

/// Apply CG_PlayerFlag's asymmetric offset, lean and half-size model.
pub(crate) fn placement(
    mut raw: bolt::BoltMatrix,
    transform: sjk_runtime::Transform,
    angles: [f32; 3],
) -> ActorInstance {
    let rotation = weapon_view::actor_world_rotation(transform.rotation);
    // G2API_GetBoltMatrix normalizes the three basis rows before world multiplication.
    for row in &mut raw {
        let length = Vec3::new(row[0], row[1], row[2]).length();
        if length != 0.0 {
            for component in &mut row[..3] {
                *component /= length;
            }
        }
    }
    let game = bolt::game_facing(raw);
    let origin = Vec3::from_array(transform.translation);
    let grip = Vec3::from_array(bolt::column(&game, 3)) * Vec3::from_array(transform.scale);
    // Only bolt translation is model-scaled (G2_API.cpp:2047-2059).
    let forward = rotation * Vec3::from_array(bolt::column(&game, 0));
    let yaw = forward.y.atan2(forward.x).to_degrees() + 270.0;
    let (sy, cy) = angles[1].to_radians().sin_cos();
    let bolt_origin = origin + rotation * grip - Vec3::Z * 12.0 + Vec3::new(sy, -cy, 0.0) * 8.0;
    let mut flag_angles = [-angles[0] * 0.5 - 30.0, yaw, angles[2]];
    let axis = Quat::from_array(sjk_client::legacy_angles_to_quaternion(flag_angles));
    let flag_origin = bolt_origin + axis * Vec3::X * 24.0;
    flag_angles[2] += 20.0;
    ActorInstance::new(
        flag_origin.to_array(),
        sjk_client::legacy_angles_to_quaternion(flag_angles),
        [0.5; 3],
    )
}

/// Append to the existing rigid-model groups; no persistent carried state to go stale.
#[allow(clippy::too_many_arguments)]
pub(super) fn submit(
    sinks: &mut Sinks<'_>,
    mesh: usize,
    entity: &sjk_runtime::SceneEntity,
    transform: sjk_runtime::Transform,
    snapshot: &Snapshot,
    time: i64,
    draw_actor: bool,
) {
    if !eligible(draw_actor, entity.kind) {
        return;
    }
    let number = entity.id.get().saturating_sub(1) as u16;
    let Ok(index) = snapshot
        .entities
        .binary_search_by_key(&number, |state| state.number())
    else {
        return;
    };
    let state = &snapshot.entities[index];
    let Some(raw) = sinks.actor_meshes[mesh].force_bones.lumbar else {
        return;
    };
    let angles = entity
        .sample_pose(time)
        .map_or(state.angular_trajectory_base(), |pose| {
            pose.view_angles_degrees
        });
    for (active, model) in carried(state.powerups()).into_iter().zip(sinks.flag_meshes) {
        if let (true, Some(model)) = (active, model) {
            let mut instance = placement(raw, transform, angles);
            instance.view_flags = sinks.entity_view_flags;
            // Map-load reservation covers every network actor plus every pickup.
            sinks.object_groups[model].push(instance);
        }
    }
}
