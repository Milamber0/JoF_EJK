//! Small renderer construction helpers shared by the world resource builder.

use super::{EntityInstance, GpuVertex};
use glam::Vec3;
use std::ops::Range;

pub(super) fn append_instance_group(
    destination: &mut Vec<EntityInstance>,
    group: &[EntityInstance],
) -> Range<u32> {
    let start = u32::try_from(destination.len()).unwrap_or(1_024);
    destination.extend(group.iter().copied());
    let end = u32::try_from(destination.len()).unwrap_or(1_024);
    start..end
}

pub(super) fn angle_to_short(radians: f32) -> i32 {
    ((radians.to_degrees() * (65_536.0 / 360.0)) as i32) & 65_535
}

pub(super) fn mesh_center(vertices: &[GpuVertex]) -> [f32; 3] {
    let Some(first) = vertices.first() else {
        return [0.0; 3];
    };
    let mut minimum = Vec3::from_array(first.position);
    let mut maximum = minimum;
    for vertex in &vertices[1..] {
        let position = Vec3::from_array(vertex.position);
        minimum = minimum.min(position);
        maximum = maximum.max(position);
    }
    ((minimum + maximum) * 0.5).to_array()
}

pub(super) fn texture_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}
