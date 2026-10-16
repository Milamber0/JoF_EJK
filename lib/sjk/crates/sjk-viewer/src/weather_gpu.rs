//! The weather's pipelines, uniform and images. Built the first time a map has weather;
//! a frame writes one uniform and records a few instanced draws without buffers.

use bytemuck::{Pod, Zeroable};
use std::ops::Range;

/// Mass buckets per cloud: particles share the flow of their bucket's mass.
pub(crate) const BUCKETS: usize = 8;
/// Quads in one splash: its ring, eight crown segments and six drops (`weather.wgsl`).
pub(crate) const SPLASH_PARTS: u32 = 15;
/// Edge of each weather image layer, in texels.
const IMAGE_SIZE: u32 = 64;

/// One cloud as `weather.wgsl` reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct GpuCloud {
    pub(crate) color: [f32; 4],
    pub(crate) shape: [f32; 4],
    pub(crate) box_min: [f32; 4],
    pub(crate) box_size: [f32; 4],
    pub(crate) layers: [f32; 4],
    pub(crate) velocity: [[f32; 4]; BUCKETS],
    pub(crate) offset: [[f32; 4]; BUCKETS],
}

/// The weather uniform.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct GpuWeather {
    pub(crate) window: [f32; 4],
    pub(crate) cover: [f32; 4],
    pub(crate) view: [f32; 4],
    pub(crate) light: [f32; 4],
    pub(crate) inverse_view_projection: [[f32; 4]; 4],
    pub(crate) haze: [f32; 4],
    pub(crate) fog_color: [f32; 4],
    pub(crate) fog_flow: [f32; 4],
    pub(crate) far: [f32; 4],
    pub(crate) wet: [f32; 4],
    pub(crate) wet_sky: [f32; 4],
    pub(crate) rain: [f32; 4],
    pub(crate) clouds: [GpuCloud; super::effects::MAX_CLOUDS],
}

/// Which pipeline a batch uses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    Streak,
    Splash,
    /// Added to the layer (`mBlendMode 1`).
    Sprite,
    /// Alpha-blended (`mBlendMode 0`).
    SpriteAlpha,
    /// The volumetric fog: one full-screen triangle.
    Volume,
}

/// One instanced draw: six vertices per particle (three for the fog).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Batch {
    pub(crate) kind: Kind,
    pub(crate) instances: Range<u32>,
}

/// World-lifetime weather GPU state.
pub(crate) struct Gpu {
    streak: wgpu::RenderPipeline,
    splash: wgpu::RenderPipeline,
    sprite: wgpu::RenderPipeline,
    sprite_alpha: wgpu::RenderPipeline,
    volume: wgpu::RenderPipeline,
    /// Rain-wet surfaces, drawn into the scene itself before the players.
    wet: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl Gpu {
    /// Build the pipelines for the effect layer and the scene, the uniform and the image
    /// array.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        images: Option<(&sjk_vfs::VirtualFileSystem, &sjk_shader::ShaderCatalog)>,
        camera: &wgpu::BindGroupLayout,
        scene_format: wgpu::TextureFormat,
        cover: &wgpu::TextureView,
        far_cover: &wgpu::TextureView,
        noise: &super::noise::Texture,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK weather layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK weather uniform"),
            size: std::mem::size_of::<GpuWeather>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let image_view = image_array(device, queue, images);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SJK weather images"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK weather bind group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(cover),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&image_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&noise.view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&noise.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(far_cover),
                },
            ],
        });
        let depth = crate::world_materials::flares::depth_layout(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK weather pipeline layout"),
            bind_group_layouts: &[Some(camera), Some(&layout), Some(&depth)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK weather shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("weather.wgsl").into()),
        });
        // GL_ONE GL_ONE on the colour; the layer's alpha is left as it is.
        let added = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let blended = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: added.alpha,
        };
        let pipeline = |vertex, fragment, blend, additive: bool, depth_compare| {
            let constants = [("additive", f64::from(u8::from(additive)))];
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fragment),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vertex),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &constants,
                        ..Default::default()
                    },
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &constants,
                        ..Default::default()
                    },
                    targets: &[Some(wgpu::ColorTargetState {
                        format: crate::frame_target::aa::effects::FORMAT,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::DepthTarget::FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(depth_compare),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        // The scene times the alpha the shader writes, plus its colour: wet surfaces darken
        // and take a sheen in one blend, without reading the scene.
        let wet = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fragment_wet"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_volume"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_wet"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: scene_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::SrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: added.alpha,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        // Rain and its splashes are blended over the scene, a faint tinted streak, rather
        // than added to it: the reference's added grey turns a dense storm into white.
        Self {
            streak: pipeline(
                "vertex_streak",
                "fragment_streak",
                blended,
                false,
                wgpu::CompareFunction::LessEqual,
            ),
            splash: pipeline(
                "vertex_splash",
                "fragment_splash",
                blended,
                false,
                wgpu::CompareFunction::LessEqual,
            ),
            sprite: pipeline(
                "vertex_sprite",
                "fragment_sprite",
                added,
                true,
                wgpu::CompareFunction::LessEqual,
            ),
            sprite_alpha: pipeline(
                "vertex_sprite",
                "fragment_sprite",
                blended,
                false,
                wgpu::CompareFunction::LessEqual,
            ),
            // The fog reads the depth itself; its triangle covers everything.
            volume: pipeline(
                "vertex_volume",
                "fragment_volume",
                blended,
                false,
                wgpu::CompareFunction::Always,
            ),
            wet,
            uniform,
            bind_group,
        }
    }

    /// Wet the opaque world in `target`, reading the depth it left (`depth`, sampled).
    pub(crate) fn draw_wet(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        camera: &wgpu::BindGroup,
        depth: &wgpu::BindGroup,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SJK rain-wet surfaces"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.wet);
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_bind_group(2, depth, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Upload this frame's uniform.
    pub(crate) fn write(&self, queue: &crate::frame_queue::FrameQueue, weather: &GpuWeather) {
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(weather));
    }

    /// Record the batches into a pass on the effect layer, whose depth is read-only.
    pub(crate) fn draw<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        depth: &'pass wgpu::BindGroup,
        batches: &[Batch],
    ) {
        if batches.is_empty() {
            return;
        }
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_bind_group(2, depth, &[]);
        for batch in batches {
            pass.set_pipeline(match batch.kind {
                Kind::Streak => &self.streak,
                Kind::Splash => &self.splash,
                Kind::Sprite => &self.sprite,
                Kind::SpriteAlpha => &self.sprite_alpha,
                Kind::Volume => &self.volume,
            });
            let vertices = match batch.kind {
                Kind::Volume => 3,
                Kind::Splash => 6 * SPLASH_PARTS,
                _ => 6,
            };
            pass.draw(0..vertices, batch.instances.clone());
        }
    }
}

/// The weather images as one array, each resized to [`IMAGE_SIZE`] with mips. Their
/// texels stay display values, as every effect image's do. A missing image becomes a
/// soft round puff.
fn image_array(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    source: Option<(&sjk_vfs::VirtualFileSystem, &sjk_shader::ShaderCatalog)>,
) -> wgpu::TextureView {
    let layers = super::effects::Image::ALL.len() as u32;
    let levels = IMAGE_SIZE.ilog2() + 1;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("SJK weather images"),
        size: wgpu::Extent3d {
            width: IMAGE_SIZE,
            height: IMAGE_SIZE,
            depth_or_array_layers: layers,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, (_, path)) in super::effects::Image::ALL.iter().enumerate() {
        let loaded = source.and_then(|(vfs, shaders)| {
            crate::shader_image::load_shader_image(vfs, shaders, path)
                .map_err(|error| {
                    crate::log::progress(format_args!("weather: {path}: {error}"));
                })
                .ok()
        });
        let image = match loaded {
            Some(image) => image::imageops::resize(
                &image,
                IMAGE_SIZE,
                IMAGE_SIZE,
                image::imageops::FilterType::Triangle,
            ),
            None => puff(),
        };
        for (level, pixels) in crate::gpu_texture::box_mip_chain(&image).iter().enumerate() {
            let (width, height) = pixels.dimensions();
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                pixels.as_raw(),
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
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}

/// A white puff fading to its edge, in place of a missing image.
fn puff() -> image::RgbaImage {
    image::RgbaImage::from_fn(IMAGE_SIZE, IMAGE_SIZE, |x, y| {
        let centre = (IMAGE_SIZE as f32 - 1.0) * 0.5;
        let distance = ((x as f32 - centre).powi(2) + (y as f32 - centre).powi(2)).sqrt() / centre;
        let value = ((1.0 - distance).max(0.0).powi(2) * 255.0) as u8;
        image::Rgba([value, value, value, value])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_uniform_matches_the_shader_layout() {
        // Every member is a vec4 or an array of them: no padding rules can differ.
        assert_eq!(std::mem::size_of::<GpuCloud>(), (5 + 2 * BUCKETS) * 16);
        assert_eq!(
            std::mem::size_of::<GpuWeather>(),
            15 * 16 + super::super::effects::MAX_CLOUDS * std::mem::size_of::<GpuCloud>()
        );
        let shader = include_str!("weather.wgsl");
        assert!(shader.contains("array<vec4<f32>, 8>"));
        assert!(shader.contains("clouds: array<Cloud, 5>"));
        assert!(shader.contains(&format!("const SPLASH_PARTS: u32 = {SPLASH_PARTS}u;")));
    }

    /// Validate the shader, then specialise every entry point both ways and translate it
    /// for Vulkan (SPIR-V) and DX12 (HLSL), as wgpu does at pipeline creation.
    #[test]
    fn every_entry_point_translates_for_vulkan_and_dx12() {
        use wgpu::naga;
        use wgpu::naga::ShaderStage::{Fragment, Vertex};
        let source = include_str!("weather.wgsl");
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source)));
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("{error:?}"));
        let entries = [
            (Vertex, "vertex_streak"),
            (Fragment, "fragment_streak"),
            (Vertex, "vertex_sprite"),
            (Fragment, "fragment_sprite"),
            (Vertex, "vertex_splash"),
            (Fragment, "fragment_splash"),
            (Vertex, "vertex_volume"),
            (Fragment, "fragment_volume"),
            (Fragment, "fragment_wet"),
        ];
        for (stage, entry) in entries {
            for additive in [0.0, 1.0] {
                let mut values = naga::back::PipelineConstants::default();
                values.insert("additive".to_owned(), additive);
                let (module, info) = naga::back::pipeline_constants::process_overrides(
                    &module,
                    &info,
                    Some((stage, entry)),
                    &values,
                )
                .unwrap_or_else(|error| panic!("{entry}: {error:?}"));
                naga::back::spv::write_vec(
                    &module,
                    &info,
                    &naga::back::spv::Options::default(),
                    Some(&naga::back::spv::PipelineOptions {
                        shader_stage: stage,
                        entry_point: entry.to_owned(),
                    }),
                )
                .unwrap_or_else(|error| panic!("{entry} SPIR-V: {error:?}"));
                let pipeline = naga::back::hlsl::PipelineOptions {
                    entry_point: Some((stage, entry.to_owned())),
                };
                naga::back::hlsl::Writer::new(
                    String::new(),
                    &naga::back::hlsl::Options::default(),
                    &pipeline,
                )
                .write(&module, &info, None)
                .unwrap_or_else(|error| panic!("{entry} HLSL: {error:?}"));
            }
        }
    }
}
