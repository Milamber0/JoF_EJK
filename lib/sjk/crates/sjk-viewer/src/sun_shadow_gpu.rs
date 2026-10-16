//! Dedicated depth-only caster and diffuse correction resources; no colour shader mutations.
use super::*;

/// Create one map and pipelines at installation, with no changes to shared colour pipelines.
pub(super) fn new(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    forge: &Forge,
    sun: sjk_shader::SunParms,
    mut settings: settings::Settings,
    gi: Option<&super::super::gi::Runtime>,
    scene: [u32; 2],
    light_divisor: u32,
    lamp_set: &crate::lamp_lights::LampSet,
    occluders: &[super::super::mover_occlusion::Occluder],
    cache_pages: Option<&super::super::lamp_cache::Pages>,
    bounds: [Vec3; 2],
    domain: &super::super::gi_probe_domain::Domain,
) -> Runtime {
    // `SJK_MOVER_OCCLUSION=0` leaves movers out of lamp shadows, for comparisons; the far
    // sun cascade still follows them. Door tiles need the traced atlas.
    let fixtures = gi.and_then(|gi| gi.fixtures.as_ref());
    let door_occluders = if fixtures.is_none()
        || std::env::var_os("SJK_MOVER_OCCLUSION").is_some_and(|value| value == "0")
    {
        &[][..]
    } else {
        occluders
    };
    let mut lamps = crate::lamp_lights::Gpu::new(device, lamp_set, door_occluders);
    if let Some(geometry) = fixtures {
        lamps.prepare_visibility(device, queue, geometry);
    }
    // Every shadow-casting mover is tracked, door tiles or not: the far cascade redraws
    // when one moves.
    let doors = lamps.take_doors();
    let movers = super::super::mover_occlusion::gpu::Runtime::new(
        device,
        &lamps,
        doors,
        &lamp_set.lamps,
        occluders,
        cache_pages,
    );
    let lamp_shadows = settings
        .day
        .enabled
        .then(|| super::lamp_shadows::Runtime::new(device, &forge.camera_layout, lamp_set));
    settings.resolution = settings
        .resolution
        .min(device.limits().max_texture_dimension_2d);
    let depth_map =
        |label| {
            device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label), size: wgpu::Extent3d {
            width: settings.resolution, height: settings.resolution, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
        format: crate::DepthTarget::FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING
            // Static and moving casters remain separate through filtering.
            | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
        view_formats: &[] }).create_view(&Default::default())
        };
    let depth = depth_map("SJK dynamic sun shadow map");
    let buffer = |size| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    };
    let light_camera = |buffer: &wgpu::Buffer| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &forge.camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    };
    let camera_buffer = buffer(std::mem::size_of::<crate::CameraUniform>() as u64);
    let receiver_buffer = buffer(std::mem::size_of::<Parameters>() as u64);
    let camera = light_camera(&camera_buffer);
    let cascade = |label| {
        let camera_buffer = buffer(std::mem::size_of::<crate::CameraUniform>() as u64);
        let camera = light_camera(&camera_buffer);
        Cascade {
            depth: depth_map(label),
            camera_buffer,
            camera,
        }
    };
    let far = settings.world.then(|| FarCascade {
        cascade: cascade("SJK far sun cascade"),
        rendered: std::cell::Cell::new(None),
        movers: std::cell::Cell::new((0, None)),
    });
    let close = settings.world.then(|| cascade("SJK close sun cascade"));
    let held = settings.world.then(|| {
        [
            held::Held::new(depth_map("SJK held view casters")),
            held::Held::new(depth_map("SJK held close casters")),
        ]
    });
    let probes = match (gi, &far) {
        (Some(voxels), Some(far)) if settings.day.enabled => {
            Some(super::super::gi_probes::Runtime::new(
                device,
                voxels,
                &far.cascade.depth,
                bounds,
                |p| voxels.world.occupied(p) || domain.blocked(p),
                &lamps,
            ))
        }
        _ => None,
    };
    let light = settings.day.enabled.then(|| {
        super::light_buffer::LightBuffer::new(device, scene, light_divisor, forge.directed_light())
    });
    let receiver_layout = super::super::model_sun::receiver_layout(device);
    let pass_binding = light
        .as_ref()
        .map_or(super::light_buffer::Binding::Absent, |light| {
            super::light_buffer::Binding::Pass(light.main_images())
        });
    let pass_layout = super::super::model_sun::receiver_layout_with(device, pass_binding.layout());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        compare: Some(wgpu::CompareFunction::LessEqual),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let bounds = super::bounds::Bounds::new(
        device,
        settings.resolution,
        [
            held.as_ref().map_or(&depth, |h| h[0].depth()),
            held.as_ref().map_or(&depth, |h| h[1].depth()),
            far.as_ref().map_or(&depth, |f| &f.cascade.depth),
        ],
    );
    let receiver_entries = cascade_entries(
        &depth,
        &sampler,
        &bounds.view,
        &receiver_buffer,
        far.as_ref(),
        close.as_ref(),
        held.as_ref(),
    );
    let sun_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("SJK receiver sun visibility"),
        layout: &super::light_buffer::sun_layout(device),
        entries: &receiver_entries,
    });
    let receiver = super::super::model_sun::receiver_group(
        device,
        &receiver_layout,
        &receiver_entries,
        probes.as_ref(),
        &sampler,
        light
            .as_ref()
            .map_or(super::light_buffer::Binding::Neutral, |light| {
                super::light_buffer::Binding::Buffer(light.main_images())
            }),
        &lamps,
        lamp_shadows.as_ref(),
        &forge.point_lights,
    );
    let light_group = super::super::model_sun::receiver_group(
        device,
        &pass_layout,
        &receiver_entries,
        probes.as_ref(),
        &sampler,
        pass_binding,
        &lamps,
        lamp_shadows.as_ref(),
        &forge.point_lights,
    );
    let light_pipelines = light.as_ref().map(|light| {
        super::light_buffer::Pipelines::new(device, forge, &pass_layout, light.directed)
    });
    let empty_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[],
    });
    let empty = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &empty_layout,
        entries: &[],
    });
    let caster_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[
            Some(&forge.camera_layout),
            Some(&empty_layout),
            Some(&crate::shared_geometry::quads::layout(device)),
        ],
        immediate_size: 0,
    });
    let caster_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(
            concat!(
                include_str!("vertex_transform.wgsl"),
                include_str!("gpu_skinning.wgsl"),
                include_str!("sun_shadow_caster.wgsl")
            )
            .into(),
        ),
    });
    let caster = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("SJK sun opaque actor depth"),
        layout: Some(&caster_layout),
        vertex: wgpu::VertexState {
            module: &caster_shader,
            entry_point: Some("shadow_vertex"),
            compilation_options: Default::default(),
            buffers: &[
                Some(crate::GpuVertex::layout()),
                Some(crate::ActorInstance::layout()),
            ],
        },
        fragment: None,
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::DepthTarget::FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: wgpu::DepthBiasState {
                // The clamp caps the slope bias well under a unit: a face nearly parallel
                // to the sun must not push its depth along the light, or the shadow it
                // casts leaves a sunlit seam at its base (0.0005 gave a two unit seam,
                // step 211's 0.005 detached shadows by tens of units). The receiver-plane
                // bias in `sun_visibility.wgsl` covers the receiver's own slope.
                constant: 1,
                slope_scale: 1.0,
                clamp: 0.00002,
            },
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let world_caster = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("SJK opaque BSP sun depth"),
        layout: Some(&caster_layout),
        vertex: wgpu::VertexState {
            module: &caster_shader,
            entry_point: Some("world_shadow_vertex"),
            compilation_options: Default::default(),
            buffers: &[Some(crate::GpuVertex::layout())],
        },
        fragment: None,
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::DepthTarget::FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: wgpu::DepthBiasState {
                // The clamp caps the slope bias well under a unit: a face nearly parallel
                // to the sun must not push its depth along the light, or the shadow it
                // casts leaves a sunlit seam at its base (0.0005 gave a two unit seam,
                // step 211's 0.005 detached shadows by tens of units). The receiver-plane
                // bias in `sun_visibility.wgsl` covers the receiver's own slope.
                constant: 1,
                slope_scale: 1.0,
                clamp: 0.00002,
            },
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&forge.camera_layout), Some(&pass_layout)],
        immediate_size: 0,
    });

    let source = super::receiver_shader().into();

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source),
    });
    let receiver_pipelines =
        [Some(wgpu::Face::Front), Some(wgpu::Face::Back), None].map(|cull_mode| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("SJK sun diffuse receiver"),
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
                        format: forge.format,
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
        });
    Runtime {
        view: std::cell::Cell::new(None),
        time: std::cell::Cell::new(0.),
        director_sun: std::cell::Cell::new(None),
        filter_reference: std::cell::Cell::new(
            std::env::var("SJK_SHADOW_FILTER_REFERENCE").as_deref() == Ok("1"),
        ),
        day: std::cell::Cell::new(settings.day),
        clarity: std::cell::Cell::new(1.),
        light_scale: std::cell::Cell::new(1.),
        debug: std::cell::Cell::new(0),
        ambient_fill: std::cell::Cell::new(0.025),
        indirect_readability: std::cell::Cell::new([1., 1.]),
        gap_close: super::gap_close::Runtime::new(device, settings.resolution),
        gap_width: std::cell::Cell::new(4.),
        depth,
        camera_buffer,
        camera,
        far,
        close,
        held,
        probes,
        receiver_buffer,
        receiver,
        light_group,
        mirror_groups: None,
        sun_group,
        light,
        light_pipelines,
        cache_group: None,

        point_lights: forge.point_lights.clone(),
        sampler,
        bounds,
        lamps,
        movers,
        lamp_shadows,
        caster,
        receiver_pipelines,
        world_caster,

        volumetrics: None,
        receivers: Vec::new(),
        empty,
        sun,
        environment: true,
        suppress_sun: false,
        settings,
    }
}

/// The separate static/moving cascade bindings shared by every receiver group of this runtime.
fn cascade_entries<'a>(
    depth: &'a wgpu::TextureView,
    sampler: &'a wgpu::Sampler,
    bounds: &'a wgpu::TextureView,
    buffer: &'a wgpu::Buffer,
    far: Option<&'a FarCascade>,
    close: Option<&'a Cascade>,
    world: Option<&'a [held::Held; 2]>,
) -> [wgpu::BindGroupEntry<'a>; 8] {
    [
        wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(depth),
        },
        wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::Sampler(sampler),
        },
        wgpu::BindGroupEntry {
            binding: 2,
            resource: buffer.as_entire_binding(),
        },
        // Actor-only mode has no far cascade; the flag in the uniform keeps it unsampled.
        wgpu::BindGroupEntry {
            binding: 3,
            resource: wgpu::BindingResource::TextureView(
                far.map_or(depth, |far| &far.cascade.depth),
            ),
        },
        wgpu::BindGroupEntry {
            binding: 4,
            resource: wgpu::BindingResource::TextureView(close.map_or(depth, |close| &close.depth)),
        },
        wgpu::BindGroupEntry {
            binding: 5,
            resource: wgpu::BindingResource::TextureView(world.map_or(depth, |w| w[0].depth())),
        },
        wgpu::BindGroupEntry {
            binding: 6,
            resource: wgpu::BindingResource::TextureView(world.map_or(depth, |w| w[1].depth())),
        },
        wgpu::BindGroupEntry {
            binding: 7,
            resource: wgpu::BindingResource::TextureView(bounds),
        },
    ]
}

impl Runtime {
    /// The material receiver over the current sun parameters and evaluated light buffer.
    pub(super) fn build_receiver(&self, device: &wgpu::Device) -> wgpu::BindGroup {
        self.build_group(
            device,
            self.light
                .as_ref()
                .map_or(super::light_buffer::Binding::Neutral, |light| {
                    super::light_buffer::Binding::Buffer(light.main_images())
                }),
        )
    }

    /// The light pass group: the cascades and probes, plus the buffer depth it reads.
    pub(super) fn build_light_group(&self, device: &wgpu::Device) -> wgpu::BindGroup {
        self.build_group(
            device,
            self.light
                .as_ref()
                .map_or(super::light_buffer::Binding::Absent, |light| {
                    super::light_buffer::Binding::Pass(light.main_images())
                }),
        )
    }

    /// The material receiver and light pass groups over the mirror images, when the
    /// light buffer has them.
    pub(super) fn build_mirror_groups(&self, device: &wgpu::Device) -> Option<MirrorGroups> {
        let images = self.light.as_ref()?.mirror_images()?;
        Some(MirrorGroups {
            receiver: self.build_group(device, super::light_buffer::Binding::Buffer(images)),
            light: self.build_group(device, super::light_buffer::Binding::Pass(images)),
        })
    }

    fn build_group(
        &self,
        device: &wgpu::Device,
        light: super::light_buffer::Binding<'_>,
    ) -> wgpu::BindGroup {
        let layout = super::super::model_sun::receiver_layout_with(device, light.layout());
        let entries = cascade_entries(
            &self.depth,
            &self.sampler,
            &self.bounds.view,
            &self.receiver_buffer,
            self.far.as_ref(),
            self.close.as_ref(),
            self.held.as_ref(),
        );
        super::super::model_sun::receiver_group(
            device,
            &layout,
            &entries,
            self.probes.as_ref(),
            &self.sampler,
            light,
            &self.lamps,
            self.lamp_shadows.as_ref(),
            &self.point_lights,
        )
    }
}
