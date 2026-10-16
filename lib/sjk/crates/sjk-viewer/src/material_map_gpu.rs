//! GPU side of material maps: the extended stage layout, the linear map
//! textures, the world's vertex frames and the material program.
//!
//! A material-mapped stage gets a second bind group on [`Gpu::layout`]: the
//! ordinary stage entries plus the maps, the frames, its parameters and the
//! diffuse sampler. Its ordinary bind group stays, so every pass that binds stage
//! groups with its own pipelines (light pass cut-outs, flares) is unaffected.

use super::super::forge::Forge;
use super::{Params, StageMaps};
use image::RgbaImage;
use std::cell::OnceCell;
use std::collections::HashMap;
use std::error::Error;
use wgpu::util::DeviceExt;

/// Material bindings after the stage entries 0..=7 of `create_stage_layout`.
const NORMAL: u32 = 8;
const SPECULAR: u32 = 9;
const FRAMES: u32 = 10;
const PARAMS: u32 = 11;
const SAMPLER: u32 = 12;
/// After the reflection probes' 13..=16 (`material_maps_reflection.wgsl`).
const EMISSION: u32 = 17;

/// Map-lifetime material-map resources; exists only when material maps are enabled.
pub(in crate::world_materials) struct Gpu {
    layout: wgpu::BindGroupLayout,
    /// Packed tangent frame and light direction per flattened world vertex.
    frames: wgpu::Buffer,
    /// Bound where a stage has no map of a kind; the shader's flags skip it.
    neutral: wgpu::TextureView,
    /// Black: bound where a stage has no emission map.
    neutral_emission: wgpu::TextureView,
    /// Some stage has normal or specular maps: the real-time light pass also writes the
    /// light's direction for them (`light_buffer`), and the lamp cache bakes it.
    /// Emission maps alone need neither.
    pub(in crate::world_materials) directed: bool,
    /// The map's reflection probes, when it has any (`reflections`); stage groups then
    /// bind their cubes, else `neutral_reflections`.
    pub(in crate::world_materials) reflections: Option<super::reflections::gpu::Probes>,
    neutral_reflections: super::reflections::gpu::Shading,
    textures: HashMap<String, wgpu::TextureView>,
    program: OnceCell<(wgpu::PipelineLayout, wgpu::ShaderModule)>,
    /// The program's dynamic-glow variant (`program::glow_source`).
    glow_program: OnceCell<wgpu::ShaderModule>,
}

impl Gpu {
    pub(in crate::world_materials) fn new(
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
    ) -> Self {
        let mut entries = super::super::gpu::stage_layout_entries().to_vec();
        let texture = |binding| crate::texture_layout_entry(binding);
        entries.extend([
            texture(NORMAL),
            texture(SPECULAR),
            texture(EMISSION),
            wgpu::BindGroupLayoutEntry {
                binding: FRAMES,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: PARAMS,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Params>() as u64),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: SAMPLER,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ]);
        entries.extend(super::reflections::gpu::Shading::layout_entries());
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK material-mapped stage layout"),
            entries: &entries,
        });
        let neutral = upload(
            device,
            queue,
            &RgbaImage::from_pixel(1, 1, image::Rgba([128, 128, 255, 255])),
            false,
        );
        let neutral_emission = upload(
            device,
            queue,
            &RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 255])),
            true,
        );
        Self {
            layout,
            frames: frames_buffer(device, &[[0; 2]]),
            neutral,
            neutral_emission,
            directed: false,
            reflections: None,
            neutral_reflections: super::reflections::gpu::Shading::neutral(device),
            textures: HashMap::new(),
            program: OnceCell::new(),
            glow_program: OnceCell::new(),
        }
    }

    /// Install the flattened world's frames (`frames::pack`), once per map.
    pub(in crate::world_materials) fn set_frames(
        &mut self,
        device: &wgpu::Device,
        packed: &[[u32; 2]],
    ) {
        if !packed.is_empty() {
            self.frames = frames_buffer(device, packed);
        }
    }

    /// The uploaded views of a stage's normal, specular and emission maps (the neutral
    /// texture for a missing one), uploading each image once.
    fn views(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        maps: &StageMaps,
    ) -> [wgpu::TextureView; 3] {
        let mut view = |image: &Option<super::MapImage>, srgb: bool| -> wgpu::TextureView {
            image.as_ref().map_or_else(
                || {
                    if srgb {
                        self.neutral_emission.clone()
                    } else {
                        self.neutral.clone()
                    }
                },
                |image| {
                    self.textures
                        .entry(image.key.clone())
                        .or_insert_with(|| upload(device, queue, &image.pixels, srgb))
                        .clone()
                },
            )
        };
        [
            view(&maps.normal, false),
            view(&maps.specular, false),
            view(&maps.emission, true),
        ]
    }

    /// A stage's maps as the floor mirrors' finish reads them.
    pub(in crate::world_materials) fn floor_maps(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        maps: &StageMaps,
    ) -> super::FloorMaps {
        let [normal, specular, _] = self.views(device, queue, maps);
        super::FloorMaps {
            normal,
            specular,
            params: bytemuck::cast(maps.params),
            clamp: maps.clamp,
        }
    }

    /// The material group of one stage: `stage_entries` (the ordinary group's
    /// entries) plus its maps, the frames, its parameters and the diffuse sampler.
    pub(in crate::world_materials) fn bind(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        stage_entries: &[wgpu::BindGroupEntry<'_>],
        maps: &StageMaps,
        sampler: &wgpu::Sampler,
    ) -> Result<wgpu::BindGroup, Box<dyn Error>> {
        let [normal, specular, emission] = self.views(device, queue, maps);
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SJK material map parameters"),
            contents: bytemuck::bytes_of(&maps.params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let mut entries = stage_entries.to_vec();
        entries.extend([
            wgpu::BindGroupEntry {
                binding: NORMAL,
                resource: wgpu::BindingResource::TextureView(&normal),
            },
            wgpu::BindGroupEntry {
                binding: SPECULAR,
                resource: wgpu::BindingResource::TextureView(&specular),
            },
            wgpu::BindGroupEntry {
                binding: EMISSION,
                resource: wgpu::BindingResource::TextureView(&emission),
            },
            wgpu::BindGroupEntry {
                binding: FRAMES,
                resource: self.frames.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: PARAMS,
                resource: params.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: SAMPLER,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ]);
        let reflections = self
            .reflections
            .as_ref()
            .map_or(&self.neutral_reflections, |probes| &probes.shading);
        entries.extend(reflections.entries());
        Ok(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK material-mapped stage"),
            layout: &self.layout,
            entries: &entries,
        }))
    }

    /// The material program for the forge's current lighting mode, compiled once.
    pub(in crate::world_materials) fn program(
        &self,
        forge: &Forge,
    ) -> (&wgpu::PipelineLayout, &wgpu::ShaderModule) {
        let (layout, module) = self.program.get_or_init(|| {
            let device = &forge.device;
            let quads = crate::shared_geometry::quads::layout(device);
            let receiver = forge
                .model_sun
                .as_ref()
                .map(|_| super::super::model_sun::receiver_layout(device));
            let mut groups = vec![Some(&forge.camera_layout), Some(&self.layout), Some(&quads)];
            if let Some(receiver) = &receiver {
                groups.push(Some(receiver));
            }
            (
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("SJK material-mapped stage program"),
                    bind_group_layouts: &groups,
                    immediate_size: 0,
                }),
                device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("SJK material-mapped stage program"),
                    source: wgpu::ShaderSource::Wgsl(
                        super::program::source(receiver.is_some()).into(),
                    ),
                }),
            )
        });
        (layout, module)
    }

    /// The program's dynamic-glow variant: stages that glow only for their emission
    /// map write that emission, the rest their colour as before. Same layout.
    pub(in crate::world_materials) fn glow_program(
        &self,
        forge: &Forge,
    ) -> (&wgpu::PipelineLayout, &wgpu::ShaderModule) {
        let (layout, _) = self.program(forge);
        let module = self.glow_program.get_or_init(|| {
            forge
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("SJK material-mapped stage glow program"),
                    source: wgpu::ShaderSource::Wgsl(
                        super::program::glow_source(forge.model_sun.is_some()).into(),
                    ),
                })
        });
        (layout, module)
    }

    /// Forget the program: the lighting mode changed (real-time lighting installed).
    pub(in crate::world_materials) fn reset_program(&mut self) {
        self.program = OnceCell::new();
        self.glow_program = OnceCell::new();
    }
}

fn frames_buffer(device: &wgpu::Device, packed: &[[u32; 2]]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("SJK material map vertex frames"),
        contents: bytemuck::cast_slice(packed),
        usage: wgpu::BufferUsages::STORAGE,
    })
}

/// Upload one map as RGBA8 with a full box-filtered mip chain, linear (normal and
/// specular maps are data, never colour) or `srgb` (emission maps are colour, encoded
/// like the diffuse image). Unfiltered normal maps shimmer at a distance.
fn upload(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    image: &RgbaImage,
    srgb: bool,
) -> wgpu::TextureView {
    let chain = mip_chain(image);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("SJK material map"),
        size: wgpu::Extent3d {
            width: image.width(),
            height: image.height(),
            depth_or_array_layers: 1,
        },
        mip_level_count: chain.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: if srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        },
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, pixels) in chain.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels.as_raw(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pixels.width() * 4),
                rows_per_image: Some(pixels.height()),
            },
            wgpu::Extent3d {
                width: pixels.width(),
                height: pixels.height(),
                depth_or_array_layers: 1,
            },
        );
    }
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Every level down to 1x1, each texel the mean of the 2x2 (or 2x1 at an odd edge)
/// texels above it.
pub(super) fn mip_chain(image: &RgbaImage) -> Vec<RgbaImage> {
    let mut chain = vec![image.clone()];
    while let Some(previous) = chain.last().filter(|p| p.width() > 1 || p.height() > 1) {
        let (width, height) = (
            (previous.width() / 2).max(1),
            (previous.height() / 2).max(1),
        );
        let next = RgbaImage::from_fn(width, height, |x, y| {
            let mut sum = [0_u32; 4];
            let mut count = 0;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (sx, sy) = (x * 2 + dx, y * 2 + dy);
                if sx < previous.width() && sy < previous.height() {
                    let texel = previous.get_pixel(sx, sy).0;
                    for channel in 0..4 {
                        sum[channel] += u32::from(texel[channel]);
                    }
                    count += 1;
                }
            }
            image::Rgba(sum.map(|value| ((value + count / 2) / count) as u8))
        });
        chain.push(next);
    }
    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_chain_reaches_one_texel_and_averages() {
        let mut image = RgbaImage::from_pixel(4, 2, image::Rgba([0, 0, 0, 0]));
        image.put_pixel(0, 0, image::Rgba([255, 255, 255, 255]));
        let chain = mip_chain(&image);
        let sizes: Vec<_> = chain.iter().map(RgbaImage::dimensions).collect();
        assert_eq!(sizes, vec![(4, 2), (2, 1), (1, 1)]);
        assert_eq!(chain[1].get_pixel(0, 0).0, [64; 4]);
        assert_eq!(chain[2].get_pixel(0, 0).0, [32; 4]);
    }
}
