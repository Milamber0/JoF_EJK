//! GPU side of SJK's menu emblem ([`crate::menu::emblem`]): one texture
//! with its mip chain and one bind group per layer, outside the shared icon
//! atlas, uploaded once the worker has decoded the pictures. The glow and
//! ray layers draw with the shape renderer's additive pipeline.

use crate::menu::emblem::{Decoded, EmblemLayer, MipChain};

/// Bind groups of the emblem's layers, once installed.
pub(super) struct EmblemTextures {
    groups: [Option<wgpu::BindGroup>; EmblemLayer::ALL.len()],
    installed: bool,
}

impl EmblemTextures {
    pub(super) fn new() -> Self {
        Self {
            groups: std::array::from_fn(|_| None),
            installed: false,
        }
    }

    /// Whether [`Self::install`] has run (whatever it found).
    pub(super) fn installed(&self) -> bool {
        self.installed
    }

    /// The bind group sampling `layer`, if it was installed.
    pub(super) fn group(&self, layer: EmblemLayer) -> Option<&wgpu::BindGroup> {
        self.groups[layer.index()].as_ref()
    }

    /// Upload every decoded layer into its own texture.
    pub(super) fn install(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        layout: &wgpu::BindGroupLayout,
        decoded: &Decoded,
    ) {
        self.installed = true;
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SJK menu emblem sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        for layer in EmblemLayer::ALL {
            let Some(chain) = decoded.layer(layer) else {
                continue;
            };
            let view = upload(device, queue, chain);
            self.groups[layer.index()] =
                Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("SJK menu emblem bind group"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                }));
        }
    }
}

/// Create a texture for `chain`, write every level and return its view.
pub(super) fn upload(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    chain: &MipChain,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("SJK menu emblem"),
        size: wgpu::Extent3d {
            width: chain.size,
            height: chain.size,
            depth_or_array_layers: 1,
        },
        mip_level_count: chain.levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: crate::ui_target::TEXTURE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, pixels) in chain.levels.iter().enumerate() {
        let edge = (chain.size >> level).max(1);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(edge * 4),
                rows_per_image: Some(edge),
            },
            wgpu::Extent3d {
                width: edge,
                height: edge,
                depth_or_array_layers: 1,
            },
        );
    }
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}
