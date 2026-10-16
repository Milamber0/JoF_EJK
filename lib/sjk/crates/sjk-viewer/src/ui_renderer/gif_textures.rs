//! GPU side of the SJK chat's GIFs ([`crate::chat_gifs`]): one texture and bind group
//! per GIF slot, outside the shared icon atlas (whose cells are 128 pixels; a GIF is up
//! to 480), each made at its GIF's size and given the frame the GIF is at.

use crate::chat_gifs::GPU_SLOTS;

/// One slot's texture.
struct SlotTexture {
    texture: wgpu::Texture,
    group: wgpu::BindGroup,
    size: [u32; 2],
}

/// The GIF slots' textures, made as GIFs are uploaded.
pub(super) struct GifTextures {
    slots: [Option<SlotTexture>; GPU_SLOTS],
    sampler: Option<wgpu::Sampler>,
}

impl GifTextures {
    pub(super) fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
            sampler: None,
        }
    }

    /// The bind group sampling slot `slot`, once it was given a frame.
    pub(super) fn group(&self, slot: usize) -> Option<&wgpu::BindGroup> {
        self.slots.get(slot)?.as_ref().map(|slot| &slot.group)
    }

    /// Give slot `slot` the frame `rgba` of a GIF of `size`, making its texture again
    /// when the size differs.
    pub(super) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        layout: &wgpu::BindGroupLayout,
        slot: usize,
        size: [u32; 2],
        rgba: &[u8],
    ) {
        let [width, height] = size;
        if slot >= GPU_SLOTS
            || width == 0
            || height == 0
            || rgba.len() != width as usize * height as usize * 4
        {
            return;
        }
        let sampler = self.sampler.get_or_insert_with(|| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("SJK chat GIF sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            })
        });
        if self.slots[slot]
            .as_ref()
            .is_none_or(|held| held.size != size)
        {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("SJK chat GIF"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: crate::ui_target::TEXTURE_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("SJK chat GIF bind group"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            });
            self.slots[slot] = Some(SlotTexture {
                texture,
                group,
                size,
            });
        }
        let Some(held) = &self.slots[slot] else {
            return;
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &held.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
}
