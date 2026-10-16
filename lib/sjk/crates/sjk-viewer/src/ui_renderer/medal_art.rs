//! GPU side of the medals' whole pictures ([`crate::medals::art`]): one texture with
//! its mip chain and one bind group per medal, outside the shared icon atlas (whose
//! cells are 128 pixels; the pictures are 512), uploaded once after a screen first
//! asked to draw one and the worker has decoded them.

use crate::medals::Medal;
use crate::medals::art::Decoded;

/// Bind groups of the medals' pictures, once installed.
pub(super) struct MedalTextures {
    groups: [Option<wgpu::BindGroup>; Medal::COUNT],
    installed: bool,
}

impl MedalTextures {
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

    /// The bind group sampling `medal`'s picture, if it was installed.
    pub(super) fn group(&self, medal: Medal) -> Option<&wgpu::BindGroup> {
        self.groups[medal.index()].as_ref()
    }

    /// Upload every decoded picture into its own texture.
    pub(super) fn install(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        layout: &wgpu::BindGroupLayout,
        decoded: &Decoded,
    ) {
        self.installed = true;
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SJK medal sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        for medal in Medal::ALL {
            let Some(chain) = decoded.picture(medal) else {
                continue;
            };
            let view = super::emblem::upload(device, queue, chain);
            self.groups[medal.index()] =
                Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("SJK medal bind group"),
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
