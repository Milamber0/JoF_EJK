//! The `--model` preview actor: its GPU vertex ranges inside the flattened
//! scene and the per-frame re-skin that streams new vertices when the
//! animation frame advances.

use crate::animation_timing::animation_frame;
use crate::player_assets::PlayerPreview;
use crate::scene_flatten::{FlattenedScene, append_player_preview, preview_gpu_vertex};
use crate::{GpuState, GpuVertex};
use std::error::Error;
use std::ops::Range;
use std::time::Instant;

/// Where one skinned surface's vertices live in the shared vertex buffer.
pub(crate) struct PreviewVertexRange {
    pub(crate) surface_index: usize,
    pub(crate) vertices: Range<usize>,
}

/// A preview actor appended to the static scene, animated in place.
pub(crate) struct GpuPlayerAnimation {
    preview: PlayerPreview,
    vertex_ranges: Vec<PreviewVertexRange>,
    current_frame: usize,
    started: Instant,
}

impl GpuPlayerAnimation {
    /// Append the preview's first frame to `flattened` and remember where
    /// its surfaces landed.
    pub(crate) fn append(
        flattened: &mut FlattenedScene,
        preview: PlayerPreview,
    ) -> Result<Self, Box<dyn Error>> {
        let frame = preview.sequence.first_frame;
        let vertex_ranges = append_player_preview(flattened, &preview, frame)?;
        Ok(Self {
            preview,
            vertex_ranges,
            current_frame: frame,
            started: Instant::now(),
        })
    }
}

impl GpuState {
    /// Re-skin the preview actor when its animation frame changed.
    pub(crate) fn update_player_animation(&mut self) -> Result<(), Box<dyn Error>> {
        let Some(player) = &mut self.player_animation else {
            return Ok(());
        };
        let sequence = &player.preview.sequence;
        let frame = animation_frame(sequence, player.started.elapsed().as_secs_f32());
        if frame == player.current_frame {
            return Ok(());
        }
        let surfaces =
            player
                .preview
                .mesh
                .skin(&player.preview.animation, &player.preview.skin, frame, 0)?;
        for range in &player.vertex_ranges {
            let surface = surfaces
                .get(range.surface_index)
                .ok_or("animated player surface disappeared")?;
            if surface.vertices.len() != range.vertices.len() {
                return Err("animated player vertex count changed".into());
            }
            let vertices = surface
                .vertices
                .iter()
                .map(|vertex| preview_gpu_vertex(&player.preview, vertex))
                .collect::<Vec<_>>();
            let byte_offset = u64::try_from(range.vertices.start)?
                .checked_mul(u64::try_from(std::mem::size_of::<GpuVertex>())?)
                .ok_or("animated player buffer offset overflow")?;
            self.queue.write_buffer(
                &self.geometry.vertex_buffer,
                byte_offset,
                bytemuck::cast_slice(&vertices),
            );
        }
        player.current_frame = frame;
        Ok(())
    }
}
