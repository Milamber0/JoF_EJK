//! Per-actor preparation and upload. Failures return to the caller's actor loop.
use super::{ActorMesh, gpu_skinning, server_bone_angles};
use crate::{GpuVertex, preview_gpu_vertex};
use sjk_client::pmove::MovementState;
use sjk_protocol::{GameState, Snapshot};
use sjk_runtime::{AnimationState, EntityId, World};
use std::error::Error;

/// Immutable inputs shared by independently prepared actors in one frame.
pub(super) struct Frame<'a> {
    pub world: &'a World,
    pub local_entity: Option<EntityId>,
    pub local_animation: Option<AnimationState>,
    pub predicted: Option<&'a MovementState>,
    pub snapshot: Option<&'a Snapshot>,
    pub game_state: Option<&'a GameState>,
    pub presentation_time: i64,
    /// The world, for the wall a player in a wall rebound holds.
    pub collision: &'a crate::movement_collision::BspMovementCollision<'a>,
}

/// Prepare one actor without allowing a bad attachment or angle query to stop its peers.
pub(super) fn prepare(mesh: &mut ActorMesh, frame: &Frame<'_>) -> Result<(), Box<dyn Error>> {
    let Frame {
        world,
        local_entity,
        local_animation,
        predicted,
        snapshot,
        game_state,
        presentation_time,
        collision,
    } = *frame;
    // A cut-off limb plays its copied animator; any request lets it evaluate.
    if let Some(limb) = &mesh.limb {
        if mesh.entity_id.is_some() {
            mesh.animator.requested = Some(limb.state);
        }
        return Ok(());
    }
    let frame_millis = mesh.angle_controller.begin_frame(presentation_time);
    let requested = mesh
        .entity_id
        .and_then(|entity_id| world.entity(entity_id))
        .map(|entity| {
            let pose = entity.sample_pose(presentation_time);
            if mesh.entity_id == local_entity {
                let animation = local_animation.or_else(|| entity.animation());
                (animation, crate::local_actor_state::pose(pose, predicted))
            } else {
                (entity.animation(), pose)
            }
        });
    let Some((Some(state), pose)) = requested else {
        return Ok(());
    };
    let mut state = state;
    if mesh.corpse_pool && !mesh.body_copied {
        state.lower.forced_frame =
            sjk_client::legacy_body_frame(&mesh.preview.config, state.lower.clip);
        state.upper.forced_frame =
            sjk_client::legacy_body_frame(&mesh.preview.config, state.upper.clip);
        state.lower.transition = None;
        state.upper.transition = None;
    }
    // Non-humanoids skip `CG_G2PlayerAngles` and face their entity yaw
    // (`cg_players.c:4274`, `:4366`).
    if !mesh.animator.humanoid() {
        mesh.render_yaw_degrees = None;
        if let (Some(snapshot), Some(game_state), Some(id)) = (snapshot, game_state, mesh.entity_id)
            && let Some(state) = snapshot
                .entities
                .iter()
                .find(|state| u64::from(state.number()) + 1 == id.get())
        {
            server_bone_angles::apply(mesh, state, game_state);
        }
    } else if let Some(pose) = pose {
        let origin = mesh
            .entity_id
            .and_then(|id| world.entity(id))
            .map_or([0.0; 3], |entity| {
                entity.sample(presentation_time).translation
            });
        let origin = if mesh.entity_id == local_entity {
            predicted.map_or(origin, |state| state.origin)
        } else {
            origin
        };
        let pose = mesh
            .wall_hold
            .apply(pose, mesh.entity_id, state.lower.clip, origin, collision);
        let mut inputs =
            sjk_client::LegacyPlayerAngleInputs::from_world(pose, origin, world, presentation_time);
        inputs.frame_millis = Some(frame_millis);
        if pose.angle.correct_animation_motion {
            inputs.motion_angles = mesh
                .animator
                .player_motion_angles(&mesh.preview.animation, presentation_time)?;
        }
        let angles = mesh
            .angle_controller
            .evaluate_with_inputs(pose, presentation_time, inputs);
        mesh.animator
            .set_player_angles(&mesh.preview.animation, angles)?;
        mesh.render_yaw_degrees = Some(angles.legs_yaw_degrees);
    } else {
        mesh.animator.clear_player_angles();
    }
    mesh.animator.requested = Some(state);
    Ok(())
}

/// Stage one successfully evaluated pose; the caller flushes even when another actor fails.
pub(super) fn apply(
    mesh: &mut ActorMesh,
    skinning: &mut gpu_skinning::Buffers,
    queue: &crate::frame_queue::FrameQueue,
    vertex_buffer: &wgpu::Buffer,
    presentation_time: i64,
) -> Result<(), Box<dyn Error>> {
    let Some(state) = mesh.animator.requested else {
        return Ok(());
    };
    mesh.animator.completed()?;
    let driver = mesh.animator.matrices();
    if let Some(limb) = &mut mesh.limb {
        limb.update_pivot(&mesh.preview.animation, driver);
    }
    // A stock model playing a rig model's emote clip is drawn, and carries its
    // bolts, in the clip's pose; saber traces keep the driver's.
    let emote = match mesh.preview.stock_emotes.clone() {
        Some(stock) if mesh.preview.rig.is_none() => mesh
            .entity_id
            .and_then(|id| u8::try_from(id.get().checked_sub(1)?).ok())
            .and_then(|slot| crate::emotes::playing(slot, std::time::Instant::now()))
            .and_then(|playing| {
                stock.play(
                    &playing.clip,
                    playing.seconds,
                    playing.remaining,
                    playing.looping,
                )
            })
            .is_some_and(|play| stock.pose(driver, play, &mut mesh.preview.stock_pose)),
        _ => false,
    };
    let matrices: &[[[f32; 4]; 3]] = if emote {
        &mesh.preview.stock_pose.matrices
    } else {
        driver
    };
    // A rig model is posed first, with the emote clip it plays: the bolts below
    // read its pose.
    if let Some(rig) = mesh.preview.rig.clone() {
        let play = mesh
            .entity_id
            .filter(|_| !rig.clips.is_empty())
            .and_then(|id| u8::try_from(id.get().checked_sub(1)?).ok())
            .and_then(|slot| crate::emotes::playing(slot, std::time::Instant::now()))
            .and_then(|playing| {
                rig.play(
                    &playing.clip,
                    playing.seconds,
                    playing.remaining,
                    playing.looping,
                )
            });
        rig.pose(matrices, play, &mut mesh.preview.rig_pose);
    }
    if let Some(palette) = &mut mesh.gpu_palette {
        match &mesh.preview.rig {
            Some(_) => palette.stage(skinning, &mesh.preview.rig_pose.palette)?,
            None => palette.stage(skinning, matrices)?,
        }
        mesh.force_bones.update(&mesh.preview, matrices);
        mesh.force_bones.update_head(&mesh.preview, matrices);
        mesh.weapon_attachments = crate::saber::attachments_from_matrices(&mesh.preview, matrices);
        mesh.driver_seat = crate::vehicle_pose::driver_seat(&mesh.preview, matrices);
        mesh.cosmetics.update_bolts(&mesh.preview, matrices);
        mesh.jetpack.update(&mesh.preview, matrices);
        mesh.current_frames = (
            state.lower.forced_frame.unwrap_or(state.lower.clip),
            state.upper.forced_frame.unwrap_or(state.upper.clip),
        );
        if !mesh.retained_pose.trace_required() {
            return Ok(());
        }
    }
    let surfaces = mesh
        .preview
        .mesh
        .skin_pose_matrices(&mesh.preview.skin, 0, matrices)?;
    mesh.retained_pose
        .update_trace_lod(&mesh.preview.mesh, driver)?;
    mesh.force_bones.update(&mesh.preview, matrices);
    mesh.force_bones.update_head(&mesh.preview, matrices);
    mesh.weapon_attachments = crate::saber::attachments_from_matrices(&mesh.preview, matrices);
    mesh.driver_seat = crate::vehicle_pose::driver_seat(&mesh.preview, matrices);
    mesh.cosmetics.update_bolts(&mesh.preview, matrices);
    mesh.jetpack.update(&mesh.preview, matrices);
    if let Some(rig) = &mesh.preview.rig
        && mesh.gpu_palette.is_none()
    {
        for range in &mesh.vertex_ranges {
            let surface = rig
                .surfaces
                .get(range.surface_index)
                .ok_or("rig surface disappeared")?;
            mesh.pose_vertices.clear();
            mesh.pose_vertices
                .extend(surface.vertices.iter().map(|vertex| {
                    crate::actor_load::rig_gpu_vertex(vertex, Some(&mesh.preview.rig_pose.palette))
                }));
            let byte_offset = u64::try_from(range.vertices.start)?
                .checked_mul(u64::try_from(std::mem::size_of::<GpuVertex>())?)
                .ok_or("animated actor buffer offset overflow")?;
            queue.write_buffer(
                vertex_buffer,
                byte_offset,
                bytemuck::cast_slice(&mesh.pose_vertices),
            );
        }
    }
    for range in &mesh.vertex_ranges {
        if mesh.gpu_palette.is_some() || mesh.preview.rig.is_some() {
            break;
        }
        let surface = surfaces
            .get(range.surface_index)
            .ok_or("animated actor surface disappeared")?;
        if surface.vertices.len() != range.vertices.len() {
            return Err("animated actor vertex count changed".into());
        }
        mesh.pose_vertices.clear();
        mesh.pose_vertices.extend(
            surface
                .vertices
                .iter()
                .map(|vertex| preview_gpu_vertex(&mesh.preview, vertex)),
        );
        let byte_offset = u64::try_from(range.vertices.start)?
            .checked_mul(u64::try_from(std::mem::size_of::<GpuVertex>())?)
            .ok_or("animated actor buffer offset overflow")?;
        queue.write_buffer(
            vertex_buffer,
            byte_offset,
            bytemuck::cast_slice(&mesh.pose_vertices),
        );
    }
    mesh.current_frames = (
        state.lower.forced_frame.unwrap_or(state.lower.clip),
        state.upper.forced_frame.unwrap_or(state.upper.clip),
    );
    if let Some(entity) = mesh.entity_id {
        mesh.retained_pose
            .publish(entity, presentation_time, surfaces);
    }
    Ok(())
}

/// Complete the upload batch even when one actor has an invalid pose.
pub(super) fn apply_all(
    meshes: &mut [ActorMesh],
    skinning: &mut gpu_skinning::Buffers,
    queue: &crate::frame_queue::FrameQueue,
    vertex_buffer: &wgpu::Buffer,
    presentation_time: i64,
) {
    for mesh in meshes {
        if mesh.animator.requested.is_none() {
            continue;
        }
        match apply(mesh, skinning, queue, vertex_buffer, presentation_time) {
            Ok(()) => mesh.animator.clear_error_report(),
            Err(error) => failed(mesh, error.as_ref()),
        }
    }
    // A bad actor must never strand healthy actors' staged palettes.
    skinning.flush(queue);
}

/// Report an actor's failure once until it successfully poses again.
pub(super) fn failed(mesh: &mut ActorMesh, error: &dyn Error) {
    mesh.retained_pose.invalidate();
    if mesh.animator.suppress_failed_frame() {
        crate::log::progress(format_args!(
            "actor {:?} ({}/{}): holding pose after animation error: {error}",
            mesh.entity_id.map(|id| id.get().saturating_sub(1)),
            mesh.appearance.model,
            mesh.appearance.variant,
        ));
    }
}
