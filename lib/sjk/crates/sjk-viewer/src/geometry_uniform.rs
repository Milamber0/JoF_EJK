//! The geometry subset of a stage table, shared by colour and fog passes.
use super::GpuStage;
use wgpu::util::DeviceExt;

pub(crate) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SJK fog geometry stage"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 5,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: std::num::NonZeroU64::new(std::mem::size_of::<GpuStage>() as u64),
            },
            count: None,
        }],
    })
}
pub(crate) fn bind(device: &wgpu::Device, buffer: &wgpu::Buffer, offset: u64) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("SJK fog geometry stage"),
        layout: &layout(device),
        entries: &[wgpu::BindGroupEntry {
            binding: 5,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer,
                offset,
                size: std::num::NonZeroU64::new(std::mem::size_of::<GpuStage>() as u64),
            }),
        }],
    })
}
pub(crate) fn empty(device: &wgpu::Device) -> wgpu::BindGroup {
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("SJK undeformed fog geometry"),
        contents: bytemuck::bytes_of(&<GpuStage as bytemuck::Zeroable>::zeroed()),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    bind(device, &buffer, 0)
}
