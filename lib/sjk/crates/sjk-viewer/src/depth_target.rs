//! Single-sample depth shared by world drawing, flares and portal composition.

/// Retained depth attachment and its read-only flare binding.
pub(crate) struct DepthTarget {
    /// Attachment view, also readable by scene depth consumers.
    pub(crate) view: wgpu::TextureView,
    /// Existing flare depth-sampling binding.
    pub(crate) sample_bind_group: wgpu::BindGroup,
}

impl DepthTarget {
    /// Unchanged single-sample depth format.
    pub(crate) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    /// Allocate at construction or resize; AA does not change depth sampling.
    pub(crate) fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SJK depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            sample_bind_group: crate::world_materials::flares::depth_binding(device, &view),
            view,
        }
    }
}
