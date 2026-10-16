//! Satin floor finish and packed reflection raster coordinates.
use super::*;
use wgpu::util::DeviceExt;

pub(super) const SCALE: f32 = 0.5;
pub(super) const ROUGHNESS: f32 = 0.4;
/// Region margin, in roughness units, of a plane whose material has maps: the widest
/// blur the shader allows (0.6, `FLOOR_MAX_ROUGHNESS`) plus as much again for the
/// lookup a normal map bends.
pub(super) const MAPPED_MARGIN: f32 = 1.2;

/// Keep the shared light buffer's pixel coordinates valid while shading a smaller
/// raster. X/Y pack into the upper-left region; depth and the oblique clip stay intact.
pub(super) fn raster_projection(scale: f32) -> Mat4 {
    let mut matrix = Mat4::from_scale(Vec3::new(scale, scale, 1.));
    matrix.w_axis.x = scale - 1.;
    matrix.w_axis.y = 1. - scale;
    matrix
}

/// Expand by the filter footprint before packing, including one bilinear texel.
pub(super) fn region(rect: [f32; 4], size: [u32; 2], scale: f32, roughness: f32) -> [f32; 4] {
    let radius = roughness * 12. * size[1] as f32 / 1080.;
    let x = radius / size[0] as f32 + 1. / (size[0] as f32 * scale);
    let y = radius / size[1] as f32 + 1. / (size[1] as f32 * scale);
    [
        (rect[0] - x).max(0.) * scale,
        (rect[1] - y).max(0.) * scale,
        (rect[2] + x).min(1.) * scale,
        (rect[3] + y).min(1.) * scale,
    ]
}

pub(super) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SJK satin floor sample"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

pub(super) struct Finish {
    control: wgpu::Buffer,
    pub group: wgpu::BindGroup,
}
impl Finish {
    pub fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        color: &wgpu::TextureView,
    ) -> Self {
        let control = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SJK floor finish"),
            contents: bytemuck::cast_slice(&[0_f32; 4]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: control.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        Self { control, group }
    }
    pub fn update(
        &self,
        queue: &crate::frame_queue::FrameQueue,
        size: [u32; 2],
        scale: f32,
        roughness: f32,
    ) {
        queue.write_buffer(
            &self.control,
            0,
            bytemuck::cast_slice(&[1. / size[0] as f32, 1. / size[1] as f32, scale, roughness]),
        );
    }
}
