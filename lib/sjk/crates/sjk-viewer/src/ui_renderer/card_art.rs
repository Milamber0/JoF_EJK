//! The card faces' pictures (`crate::cards::faces`): one texture each, uploaded as the
//! worker decodes them, a couple a frame, and made again when the packs are mounted anew.

use crate::cards::faces;

/// Pictures uploaded in one frame at most, so a new pack does not stall a frame.
const PER_FRAME: usize = 2;

pub(super) struct CardTextures {
    /// The faces' generation the groups belong to.
    generation: u64,
    groups: Vec<Option<wgpu::BindGroup>>,
    sampler: Option<wgpu::Sampler>,
}

impl CardTextures {
    pub(super) fn new() -> Self {
        Self {
            generation: 0,
            groups: Vec::new(),
            sampler: None,
        }
    }

    /// The bind group sampling face `index`'s picture, once uploaded.
    pub(super) fn group(&self, index: usize) -> Option<&wgpu::BindGroup> {
        self.groups.get(index)?.as_ref()
    }

    /// Upload the pictures decoded since the last call (a few a frame); forget them all
    /// when the faces changed.
    pub(super) fn install(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        layout: &wgpu::BindGroupLayout,
    ) {
        let faces = faces::current();
        if faces.generation != self.generation || self.groups.len() != faces.list.len() {
            self.generation = faces.generation;
            self.groups = (0..faces.list.len()).map(|_| None).collect();
        }
        let mut uploaded = 0;
        for index in 0..self.groups.len() {
            if uploaded == PER_FRAME {
                break;
            }
            if self.groups[index].is_some() {
                continue;
            }
            let Some(chain) = faces.art(index) else {
                continue;
            };
            let sampler = self.sampler.get_or_insert_with(|| {
                device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("SJK card art sampler"),
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    mipmap_filter: wgpu::MipmapFilterMode::Linear,
                    ..Default::default()
                })
            });
            let view = super::emblem::upload(device, queue, chain);
            self.groups[index] = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("SJK card art bind group"),
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
            }));
            uploaded += 1;
        }
    }
}
