//! Instance payload shared by simple entities and EFX primitives.

use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct EntityInstance {
    pub(crate) position: [f32; 3],
    pub(crate) kind: u32,
    pub(crate) size: f32,
    pub(crate) alpha: f32,
    pub(crate) uv_rect: [f32; 4],
    pub(crate) color: [f32; 4],
    pub(crate) direction: [f32; 3],
    pub(crate) rotation: f32,
    pub(crate) uv_transform: [f32; 4],
}

impl EntityInstance {
    const ATTRIBUTES: [wgpu::VertexAttribute; 9] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Uint32,
        2 => Float32,
        3 => Float32,
        4 => Float32x4,
        5 => Float32x4,
        6 => Float32x3,
        7 => Float32,
        8 => Float32x4
    ];

    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBUTES,
        }
    }
}
