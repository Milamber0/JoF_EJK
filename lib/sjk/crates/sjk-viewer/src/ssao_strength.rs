//! Map-lifetime strength uniform shared by both SSAO paths.
use std::cell::Cell;
use wgpu::util::DeviceExt;

pub(super) struct Uniform {
    buffer: wgpu::Buffer,
    pub(super) group: wgpu::BindGroup,
    value: Cell<f32>,
}

pub(super) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SJK SSAO strength"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

impl Uniform {
    pub(super) fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> Self {
        let value = super::settings::DEFAULT_INTENSITY;
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SJK SSAO strength"),
            contents: bytemuck::cast_slice(&[value, 0., 0., 0.]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK SSAO strength"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        Self {
            buffer,
            group,
            value: Cell::new(value),
        }
    }

    pub(super) fn update(&self, queue: &crate::frame_queue::FrameQueue, value: f32) {
        if !value.is_finite() {
            return;
        }
        let value = value.clamp(0., 4.);
        if self.value.replace(value) != value {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[value, 0., 0., 0.]));
        }
    }
}
