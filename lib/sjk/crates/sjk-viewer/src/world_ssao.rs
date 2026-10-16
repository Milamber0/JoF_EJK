//! Conservative material-aware SSAO: only unambiguous static diffuse receivers.
use super::*;

#[path = "ssao_settings.rs"]
pub(crate) mod settings;
#[path = "ssao_strength.rs"]
mod strength;

/// Map-lifetime receiver selection and read-only-depth pipeline variants.
#[derive(Default)]
pub(super) struct AmbientOcclusion {
    pipelines: Vec<wgpu::RenderPipeline>,
    strength: Option<strength::Uniform>,
    /// The day-mode variants: the same receivers, sampling the light pass's occlusion.
    buffered: Vec<wgpu::RenderPipeline>,
    pub(super) receivers: Vec<(usize, usize)>,
}

/// A single opaque modulating base/lightmap pass has no emissive bundle.
pub(super) fn authored_diffuse(definition: Option<&sjk_shader::ShaderDefinition>) -> bool {
    definition.is_none_or(|d| !d.emits_light && d.emissive_images.is_empty() && d.sky.is_none())
}

/// Runtime blend/geometry eligibility, after authored emission has been excluded.
pub(super) fn eligible(key: PipelineKey, gpu: &GpuStage) -> bool {
    super::material_maps::without_maps(key.geometry) == 0
        && key.depth_write
        && key.source == wgpu::BlendFactor::One
        && key.destination == wgpu::BlendFactor::Zero
        && gpu.generators[2] == 0.0
        && gpu.secondary_control[2] > 0.5
        && gpu.secondary_control[1] == 1.0
        && (gpu.animation[3] == 1.0 || gpu.secondary_animation[3] == 1.0)
}

impl AmbientOcclusion {
    /// Compile the conservative receiver set once; frame drawing only traverses it.
    pub(super) fn new(
        device: &wgpu::Device,
        camera: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        materials: &[Material],
        keys: &[PipelineKey],
    ) -> Self {
        let receivers: Vec<_> = materials
            .iter()
            .enumerate()
            .filter_map(|(i, material)| {
                if material.flare
                    || material.sort != SORT_OPAQUE
                    || material.stages.len() != 1
                    || !material.fog_draws.is_empty()
                    || material.static_draws.is_empty()
                    || !material.stages[0].ao_receiver
                {
                    return None;
                }
                let cull = keys[material.stages[0].pipeline].cull;
                Some((
                    i,
                    match cull {
                        Some(wgpu::Face::Front) => 0,
                        Some(wgpu::Face::Back) => 1,
                        None => 2,
                    },
                ))
            })
            .collect();
        if receivers.is_empty() {
            return Self::default();
        }
        let depth = flares::depth_layout(device);
        let strength_layout = strength::layout(device);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK SSAO layout"),
            bind_group_layouts: &[Some(camera), Some(&depth), Some(&strength_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK depth-only SSAO"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("ssao_strength.wgsl"),
                    include_str!("ssao.wgsl")
                )
                .into(),
            ),
        });
        let sample = shadows::light_buffer::sample_layout(device);
        let buffered_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK buffered SSAO layout"),
            bind_group_layouts: &[Some(camera), Some(&sample), Some(&strength_layout)],
            immediate_size: 0,
        });
        let buffered_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK buffered SSAO"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("ssao_strength.wgsl"),
                    include_str!("ssao_buffered.wgsl")
                )
                .into(),
            ),
        });
        let variants = |layout: &wgpu::PipelineLayout, shader: &wgpu::ShaderModule| {
            [Some(wgpu::Face::Front), Some(wgpu::Face::Back), None]
                .map(|cull_mode| {
                    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some("SJK diffuse receiver SSAO"),
                        layout: Some(&layout),
                        vertex: wgpu::VertexState {
                            module: &shader,
                            entry_point: Some("vertex"),
                            compilation_options: Default::default(),
                            buffers: &[Some(crate::GpuVertex::layout())],
                        },
                        fragment: Some(wgpu::FragmentState {
                            module: &shader,
                            entry_point: Some("fragment"),
                            compilation_options: Default::default(),
                            targets: &[Some(wgpu::ColorTargetState {
                                format,
                                blend: Some(wgpu::BlendState {
                                    color: wgpu::BlendComponent {
                                        src_factor: wgpu::BlendFactor::Dst,
                                        dst_factor: wgpu::BlendFactor::Zero,
                                        operation: wgpu::BlendOperation::Add,
                                    },
                                    alpha: wgpu::BlendComponent::REPLACE,
                                }),
                                write_mask: wgpu::ColorWrites::RED
                                    | wgpu::ColorWrites::GREEN
                                    | wgpu::ColorWrites::BLUE,
                            })],
                        }),
                        primitive: wgpu::PrimitiveState {
                            cull_mode,
                            ..Default::default()
                        },
                        depth_stencil: Some(wgpu::DepthStencilState {
                            format: crate::DepthTarget::FORMAT,
                            depth_write_enabled: Some(false),
                            depth_compare: Some(wgpu::CompareFunction::Equal),
                            stencil: Default::default(),
                            bias: Default::default(),
                        }),
                        multisample: Default::default(),
                        multiview_mask: None,
                        cache: None,
                    })
                })
                .into()
        };
        let pipelines = variants(&layout, &shader);
        let buffered = variants(&buffered_layout, &buffered_shader);
        crate::log::progress(format_args!(
            "SSAO: {} static diffuse materials",
            receivers.len()
        ));
        Self {
            pipelines,
            buffered,
            receivers,
            strength: Some(strength::Uniform::new(device, &strength_layout)),
        }
    }
}

impl Runtime {
    /// Publish a live strength change without recompiling or reallocating the pass.
    pub(crate) fn set_ssao_intensity(
        &self,
        queue: &crate::frame_queue::FrameQueue,
        intensity: f32,
    ) {
        if let Some(strength) = &self.ssao.strength {
            strength.update(queue, intensity);
        }
    }

    /// Whether this map has supported diffuse receivers; no pass is needed otherwise.
    pub(crate) fn has_ssao_receivers(&self) -> bool {
        !self.ssao.receivers.is_empty()
    }

    /// Ambient correction before fog, transparency, particles and emissive composites.
    /// With `light_buffered` (the light buffer was drawn this frame) the receivers take
    /// the light pass's occlusion term instead of computing their own.
    pub(crate) fn draw_ssao(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        depth: &crate::DepthTarget,
        input: &FrameDraw<'_>,
        light_buffered: bool,
    ) {
        let light = light_buffered
            .then(|| self.shadows.as_ref()?.light.as_ref())
            .flatten();
        // `r_dayDebug` bit 1 (no occlusion) covers this pass too.
        if light.is_some()
            && self
                .shadows
                .as_ref()
                .is_some_and(|s| s.debug.get() & 1 != 0)
        {
            return;
        }
        let Some(strength) = &self.ssao.strength else {
            return;
        };
        let mut pass = flares::begin_pass(encoder, color, &depth.view);
        pass.set_bind_group(0, input.camera, &[]);
        pass.set_bind_group(
            1,
            light.map_or(&depth.sample_bind_group, |l| &l.sample_group),
            &[],
        );
        pass.set_bind_group(2, &strength.group, &[]);
        pass.set_vertex_buffer(0, input.vertices.slice(..));
        pass.set_index_buffer(input.indices.slice(..), wgpu::IndexFormat::Uint32);
        let pipelines = if light.is_some() {
            &self.ssao.buffered
        } else {
            &self.ssao.pipelines
        };
        for &(material, pipeline) in &self.ssao.receivers {
            let material = &self.materials[material];
            pass.set_pipeline(&pipelines[pipeline]);
            let ranges =
                self.visible_static_ranges(material, input.source_cluster, input.visibility);
            for range in ranges {
                pass.draw_indexed(range, 0, 0..1);
            }
        }
    }
}
