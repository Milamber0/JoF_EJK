//! Per-instance GPU record shared by skinned actors, rigid MD3 objects and
//! instanced inline models.
//!
//! Besides the transform and entity colour, every instance carries the
//! entity light computed on the CPU (`entity_lighting`): rd-vanilla evaluates
//! `R_SetupEntityLighting` once per refEntity (`tr_light.cpp:304-412`) and
//! `RB_CalcDiffuseColor` (`tr_shade_calc.cpp:1141-1191`) then shades each
//! vertex from those three vectors. They remain the legacy diffuse result and
//! missing-grid fallback; optional spatial diffuse samples the map at each fragment.

use bytemuck::{Pod, Zeroable};
use std::ops::Range;

/// Instances the shared actor instance buffer holds: every actor, object and
/// mover draw plus the pickup and effect overrides of one frame. Up to 1024
/// entities can each draw several times, so a busy map exceeds the old 1024.
pub(crate) const CAPACITY: usize = 4_096;

/// Appends `group` to the frame's packed `instances`, stopping at [`CAPACITY`]
/// so a draw range can never reach past the GPU buffer (wgpu aborts the frame
/// on such a range). Returns the range the group drew.
pub(crate) fn append_group(
    instances: &mut Vec<ActorInstance>,
    group: &[ActorInstance],
) -> Range<u32> {
    let start = instances.len();
    let room = CAPACITY.saturating_sub(start);
    instances.extend_from_slice(&group[..group.len().min(room)]);
    start as u32..instances.len() as u32
}

/// Entity light on the 0..=1 colour scale, ready for the vertex stage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EntityLight {
    pub(crate) ambient: [f32; 3],
    pub(crate) directed: [f32; 3],
    /// Unit vector pointing towards the light.
    pub(crate) direction: [f32; 3],
}

impl EntityLight {
    /// Light used before the world grid is sampled: the reference's grid-less
    /// fallback (`tr_light.cpp:335-349`, 150 ambient + 32 minimum add, 150
    /// directed) lit from the default sun direction (`tr_bsp.cpp:2024-2028`).
    pub(crate) const FALLBACK: Self = Self {
        ambient: [182.0 / 255.0; 3],
        directed: [150.0 / 255.0; 3],
        direction: [0.428_57, 0.285_71, 0.857_14],
    };
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct ActorInstance {
    pub(crate) position: [f32; 3],
    /// 1.0 requests `RF_DEPTHHACK` (`tr_backend.cpp:906`: depth range 0..0.3
    /// so first-person models never clip into walls); 0.0 otherwise.
    pub(crate) depth_hack: f32,
    pub(crate) rotation: [f32; 4],
    pub(crate) scale: [f32; 3],
    /// One-based fog table slot; zero disables fog.
    pub(crate) fog_index: u32,
    pub(crate) entity_color: [f32; 4],
    pub(crate) shader_tex_coord: [f32; 2],
    pub(crate) entity_control: [f32; 2],
    pub(crate) light_ambient: [f32; 3],
    /// Bit 0: mirror/camera-portal only (RF_THIRD_PERSON). Bit 1: also
    /// submitted to the sky-portal scene (isPortalEnt).
    pub(crate) view_flags: u32,
    pub(crate) light_directed: [f32; 3],
    _padding4: f32,
    pub(crate) light_direction: [f32; 3],
    _padding5: f32,
}

impl ActorInstance {
    /// `view_flags` bit of world geometry (the identity instance, movers): the stage
    /// program gives such an instance no entity light.
    pub(crate) const WORLD: u32 = 4;
    const ATTRIBUTES: [wgpu::VertexAttribute; 11] = [
        attribute(wgpu::VertexFormat::Float32x4, 0, 5),
        attribute(wgpu::VertexFormat::Float32x4, 16, 6),
        attribute(wgpu::VertexFormat::Float32x3, 32, 7),
        attribute(wgpu::VertexFormat::Float32x4, 48, 8),
        attribute(wgpu::VertexFormat::Float32x2, 64, 9),
        attribute(wgpu::VertexFormat::Float32x2, 72, 10),
        attribute(wgpu::VertexFormat::Float32x3, 80, 11),
        attribute(wgpu::VertexFormat::Float32x3, 96, 12),
        attribute(wgpu::VertexFormat::Float32x3, 112, 13),
        attribute(wgpu::VertexFormat::Uint32, 44, 14),
        attribute(wgpu::VertexFormat::Uint32, 92, 15),
    ];

    pub(crate) fn new(position: [f32; 3], rotation: [f32; 4], scale: [f32; 3]) -> Self {
        Self {
            position,
            depth_hack: 0.0,
            rotation,
            scale,
            fog_index: 0,
            entity_color: [1.0; 4],
            shader_tex_coord: [0.0; 2],
            entity_control: [0.0; 2],
            light_ambient: EntityLight::FALLBACK.ambient,
            view_flags: 0,
            light_directed: EntityLight::FALLBACK.directed,
            _padding4: 0.0,
            light_direction: EntityLight::FALLBACK.direction,
            _padding5: 0.0,
        }
    }

    /// Replace the entity light consumed by `rgbGen lightingDiffuse`.
    /// The identity instance world statics bind: flagged as world geometry.
    pub(crate) fn world_identity() -> Self {
        Self {
            // Static BSP geometry participates in sky views as well as the main view.
            view_flags: Self::WORLD | 2,
            ..Self::new([0.; 3], [0., 0., 0., 1.], [1.; 3])
        }
    }

    pub(crate) fn set_light(&mut self, light: EntityLight) {
        // A disintegrating actor carries its hit point and burn radius here instead
        // (`disintegration::State::mark`); rd-vanilla draws it unlit.
        if self.view_flags
            & (crate::disintegration::RF_DISINTEGRATE1 | crate::disintegration::RF_DISINTEGRATE2)
            != 0
        {
            return;
        }
        self.light_ambient = light.ambient;
        self.light_directed = light.directed;
        self.light_direction = light.direction;
    }

    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Rendering metadata only; snapshot state takes precedence over baselines.
pub(crate) fn scene_flags(
    game: Option<&sjk_protocol::GameState>,
    snapshot: Option<&sjk_protocol::Snapshot>,
    number: u16,
) -> u32 {
    snapshot
        .and_then(|s| {
            s.entities
                .binary_search_by_key(&number, |e| e.number())
                .ok()
                .map(|i| &s.entities[i])
        })
        .or_else(|| game.and_then(|g| g.baseline(usize::from(number))))
        .map_or(0, legacy_render_flags)
}

/// Stock `EF_SHADER_ANIM` / `RF_SETANIMINDEX`: retain the per-entity image frame
/// alongside view flags without enlarging the GPU instance or allocating per frame.
pub(crate) fn legacy_render_flags(state: &sjk_protocol::EntityState) -> u32 {
    let portal = if state.is_portal_entity() { 2 } else { 0 };
    portal
        | if state.e_flags() & 16 != 0 {
            8 | ((state.raw_field(83).unwrap_or(0) & 0xffff) << 8)
        } else {
            0
        }
}

const fn attribute(
    format: wgpu::VertexFormat,
    offset: u64,
    shader_location: u32,
) -> wgpu::VertexAttribute {
    wgpu::VertexAttribute {
        format,
        offset,
        shader_location,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(count: usize) -> Vec<ActorInstance> {
        vec![ActorInstance::new([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3]); count]
    }

    #[test]
    fn groups_pack_one_after_another() {
        let mut packed = Vec::new();
        assert_eq!(append_group(&mut packed, &group(3)), 0..3);
        assert_eq!(append_group(&mut packed, &group(2)), 3..5);
        assert_eq!(packed.len(), 5);
    }

    #[test]
    fn packing_stops_at_the_buffer_capacity() {
        // 1026 instances once overran the old 1024-entry buffer and wgpu aborted the frame.
        let mut packed = Vec::new();
        let first = append_group(&mut packed, &group(CAPACITY - 1));
        assert_eq!(first, 0..(CAPACITY as u32 - 1));
        let second = append_group(&mut packed, &group(5));
        assert_eq!(second, (CAPACITY as u32 - 1)..CAPACITY as u32);
        let third = append_group(&mut packed, &group(5));
        assert_eq!(third, CAPACITY as u32..CAPACITY as u32);
        assert_eq!(packed.len(), CAPACITY);
    }
}
