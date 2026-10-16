//! Preserve full-precision depth-equal receiver attributes, then shade each pixel once.
//! The attribute pass retains draw order, including coplanar receivers and entities.
use super::*;

/// Geometry needs only the cascades and sun parameters, not lamp/probe resources.
pub(in crate::world_materials) fn sun_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let depth = |binding| {
        texture_entry(
            binding,
            wgpu::ShaderStages::FRAGMENT,
            wgpu::TextureSampleType::Depth,
        )
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SJK receiver sun visibility"),
        entries: &[
            depth(0),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            depth(3),
            depth(4),
            depth(5),
            depth(6),
            super::bounds::layout_entry(),
        ],
    })
}

fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let stages = wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE;
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SJK receiver attributes"),
        entries: &[
            texture_entry(
                0,
                stages,
                wgpu::TextureSampleType::Float { filterable: false },
            ),
            texture_entry(
                1,
                stages,
                wgpu::TextureSampleType::Float { filterable: false },
            ),
        ],
    })
}

/// Indirect dispatch size (x, y, z) and entry count ahead of the packed pixels.
const DIRECT_HEADER: u64 = 16;

/// Receivers the lamp cache cannot serve. The deferred lighting pass lists them (packed
/// pixel coordinates) instead of walking their lamp lists itself: one receiver beside a
/// dense fixture reaches dozens of lamps, and that serial walk held the whole pass.
/// `direct_lamps` lights the listed receivers with each list spread over several lanes.
pub(in crate::world_materials) struct Direct {
    pub(in crate::world_materials) list: wgpu::Buffer,
    /// The header of an empty list: no workgroups yet, y = z = 1, no entries.
    reset: wgpu::Buffer,
    /// The direct-lamp program's group: the list and the light buffer as storage.
    group: wgpu::BindGroup,
    /// `group` over the light buffer's mirror images.
    mirror: Option<wgpu::BindGroup>,
}
impl Direct {
    fn new(device: &wgpu::Device, size: [u32; 2], images: &Images) -> Self {
        use wgpu::util::DeviceExt;
        let list = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK direct-lamp receivers"),
            size: DIRECT_HEADER + 4 * u64::from(size[0]) * u64::from(size[1]),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let reset = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SJK direct-lamp list reset"),
            contents: bytemuck::cast_slice(&[0u32, 1, 1, 0]),
            usage: wgpu::BufferUsages::COPY_SRC,
        });
        let group = direct_group(device, &list, images);
        Self {
            list,
            reset,
            group,
            mirror: None,
        }
    }

    /// The group lighting the active light-buffer images.
    fn group(&self, mirror: bool) -> &wgpu::BindGroup {
        match &self.mirror {
            Some(group) if mirror => group,
            _ => &self.group,
        }
    }
}

/// The direct-lamp program's group: `list` and the images it lights, as storage: the
/// colour, and the directions when the buffer is directed.
fn direct_group(device: &wgpu::Device, list: &wgpu::Buffer, images: &Images) -> wgpu::BindGroup {
    let directed = images.directed;
    let mut entries = vec![
        wgpu::BindGroupEntry {
            binding: 8,
            resource: list.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 9,
            resource: wgpu::BindingResource::TextureView(&images.color),
        },
    ];
    if directed {
        entries.push(wgpu::BindGroupEntry {
            binding: 10,
            resource: wgpu::BindingResource::TextureView(&images.direction),
        });
    }
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &direct_layout(device, directed),
        entries: &entries,
    })
}

/// Group 3 of the direct-lamp program: the receiver list and the light buffer it lights,
/// with the direction target when `directed`.
fn direct_layout(device: &wgpu::Device, directed: bool) -> wgpu::BindGroupLayout {
    let mut entries = vec![
        wgpu::BindGroupLayoutEntry {
            binding: 8,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 9,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format: FORMAT,
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            count: None,
        },
    ];
    if directed {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 10,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format: DIRECTION_FORMAT,
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            count: None,
        });
    }
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SJK direct-lamp receivers"),
        entries: &entries,
    })
}

/// Two map-sized scratch targets; original f32 attributes and sun visibility share channels.
pub(super) struct Targets {
    world: wgpu::TextureView,
    normal: wgpu::TextureView,
    group: wgpu::BindGroup,
    /// Lamp cache coordinates of each receiver; absent when the device cannot attach
    /// a third RGBA32F target.
    pub(in crate::world_materials) cache: Option<wgpu::TextureView>,
    /// Receivers the cache cannot serve; present with `cache`.
    pub(in crate::world_materials) direct: Option<Direct>,
}
impl Targets {
    pub(super) fn new(device: &wgpu::Device, size: [u32; 2], images: &Images) -> Self {
        let world = target(
            device,
            size,
            "SJK world attributes",
            wgpu::TextureFormat::Rgba32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let normal = target(
            device,
            size,
            "SJK normal attributes",
            wgpu::TextureFormat::Rgba32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout(device),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&world),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&normal),
                },
            ],
        });
        let cache = (device.limits().max_color_attachment_bytes_per_sample
            >= crate::world_materials::lamp_cache::ATTACHMENT_BYTES)
            .then(|| {
                target(
                    device,
                    size,
                    "SJK lamp cache attributes",
                    wgpu::TextureFormat::Rgba32Float,
                    wgpu::TextureUsages::RENDER_ATTACHMENT,
                )
            });
        let direct = cache.is_some().then(|| Direct::new(device, size, images));
        Self {
            world,
            normal,
            group,
            cache,
            direct,
        }
    }

    /// The direct-lamp group for the light buffer's mirror images, if any.
    pub(super) fn configure_mirror(&mut self, device: &wgpu::Device, images: Option<&Images>) {
        if let Some(direct) = &mut self.direct {
            direct.mirror = images.map(|images| direct_group(device, &direct.list, images));
        }
    }
}

/// Fixed receiver and fullscreen programs, independent of target size.
pub(super) struct Pipelines {
    attributes: [wgpu::RenderPipeline; 2],

    entity: wgpu::RenderPipeline,
    light: wgpu::RenderPipeline,
    /// Lamp cache variants: compiled at installation for a map whose light pass uses them
    /// (`prewarm_cached_receivers`), else on first use.
    cached: std::cell::OnceCell<Cached>,
    sources: Sources,
    /// The lighting passes also write the light buffer's direction target.
    directed: bool,
}
struct Cached {
    attributes: [wgpu::RenderPipeline; 2],

    entity: wgpu::RenderPipeline,
    light: wgpu::RenderPipeline,
    /// Lights the receivers `light` lists (`direct_lamps`).
    direct: wgpu::ComputePipeline,
}
/// What the cached variants are compiled from, kept for their first use.
struct Sources {
    world: wgpu::ShaderModule,
    entity: wgpu::ShaderModule,
    world_layout: wgpu::PipelineLayout,
    entity_layout: wgpu::PipelineLayout,
    light_layout: [wgpu::BindGroupLayout; 3],
}
impl Pipelines {
    pub(super) fn new(
        device: &wgpu::Device,
        forge: &Forge,
        receiver: &wgpu::BindGroupLayout,
        world: &wgpu::ShaderModule,
        entity: &wgpu::ShaderModule,
        directed: bool,
    ) -> Self {
        let sun = sun_layout(device);
        let world_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&forge.camera_layout), Some(&sun)],
            immediate_size: 0,
        });
        let entity_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                Some(&forge.camera_layout),
                Some(&forge.stage_layout),
                Some(&crate::shared_geometry::quads::layout(device)),
                Some(&sun),
            ],
            immediate_size: 0,
        });
        let attributes = [false, true].map(|mover| {
            attribute_pipeline(
                device,
                &world_layout,
                world,
                if mover {
                    "mover_vertex"
                } else {
                    "static_vertex"
                },
                mover,
                false,
                false,
            )
        });

        let entity_pipeline = attribute_pipeline(
            device,
            &entity_layout,
            entity,
            "entity_light_vertex",
            true,
            true,
            false,
        );
        let light_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                Some(&forge.camera_layout),
                Some(receiver),
                Some(&layout(device)),
            ],
            immediate_size: 0,
        });
        let light = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SJK once-per-pixel lighting"),
            layout: Some(&light_layout),
            vertex: wgpu::VertexState {
                module: world,
                entry_point: Some("receiver_vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: world,
                entry_point: Some(if directed {
                    "receiver_light_directed"
                } else {
                    "receiver_light"
                }),
                compilation_options: Default::default(),
                targets: light_targets(directed),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            attributes,

            entity: entity_pipeline,
            light,
            cached: std::cell::OnceCell::new(),
            sources: Sources {
                world: world.clone(),
                entity: entity.clone(),
                world_layout,
                entity_layout,
                light_layout: [
                    forge.camera_layout.clone(),
                    receiver.clone(),
                    layout(device),
                ],
            },
            directed,
        }
    }

    fn cached(&self, device: &wgpu::Device) -> &Cached {
        self.cached.get_or_init(|| {
            let from = &self.sources;
            let attributes = [false, true].map(|mover| {
                attribute_pipeline(
                    device,
                    &from.world_layout,
                    &from.world,
                    if mover {
                        "mover_vertex"
                    } else {
                        "static_vertex_cached"
                    },
                    mover,
                    false,
                    true,
                )
            });

            let entity = attribute_pipeline(
                device,
                &from.entity_layout,
                &from.entity,
                "entity_light_vertex",
                true,
                true,
                true,
            );
            let cache = crate::world_materials::lamp_cache::Cache::layout(device, self.directed);
            let light_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[
                    Some(&from.light_layout[0]),
                    Some(&from.light_layout[1]),
                    Some(&from.light_layout[2]),
                    Some(&cache),
                ],
                immediate_size: 0,
            });
            let light = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("SJK once-per-pixel lighting, cached lamps"),
                layout: Some(&light_layout),
                vertex: wgpu::VertexState {
                    module: &from.world,
                    entry_point: Some("receiver_vertex"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &from.world,
                    entry_point: Some(if self.directed {
                        "receiver_light_cached_directed"
                    } else {
                        "receiver_light_cached"
                    }),
                    compilation_options: Default::default(),
                    targets: light_targets(self.directed),
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
            let direct_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[
                    Some(&from.light_layout[0]),
                    Some(&from.light_layout[1]),
                    Some(&from.light_layout[2]),
                    Some(&direct_layout(device, self.directed)),
                ],
                immediate_size: 0,
            });
            let direct = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("SJK direct lamps"),
                layout: Some(&direct_layout),
                module: &from.world,
                entry_point: Some(if self.directed {
                    "direct_lamps_directed"
                } else {
                    "direct_lamps"
                }),
                compilation_options: Default::default(),
                cache: None,
            });
            Cached {
                attributes,

                entity,
                light,
                direct,
            }
        })
    }
}

/// Targets of the deferred lighting pass: the light, then the directions when `directed`.
fn light_targets(directed: bool) -> &'static [Option<wgpu::ColorTargetState>] {
    const TARGETS: [Option<wgpu::ColorTargetState>; 2] = [
        Some(wgpu::ColorTargetState {
            format: FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        }),
        Some(wgpu::ColorTargetState {
            format: DIRECTION_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        }),
    ];
    &TARGETS[..if directed { 2 } else { 1 }]
}

/// `cached` adds the lamp cache coordinate target; its static variant reads the page
/// stream at vertex slot 1, where movers and entities read their instances.
fn attribute_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    entry: &str,
    mover: bool,
    deforms: bool,
    cached: bool,
) -> wgpu::RenderPipeline {
    let slot = if mover {
        crate::ActorInstance::layout()
    } else {
        crate::world_materials::lamp_cache::Cache::page_layout()
    };
    let buffers = [Some(crate::GpuVertex::layout()), Some(slot)];
    let targets = [0, 1, 2].map(|_| {
        Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba32Float,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })
    });
    let constants = [("geometry_deforms", 1.0), ("geometry_sprites", 1.0)];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("SJK exact receiver attributes"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: if deforms { &constants } else { &[] },
                ..Default::default()
            },
            buffers: &buffers[..if mover || cached { 2 } else { 1 }],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(if cached {
                "attributes_cached"
            } else {
                "attributes"
            }),
            compilation_options: Default::default(),
            targets: &targets[..if cached { 3 } else { 2 }],
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
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
}

impl super::super::super::Runtime {
    /// Compile the lamp cache's receiver pipelines at map installation when this map's
    /// light pass will use them (the test [`Self::draw_receiver_lighting`] makes), instead
    /// of in its first frame. Returns whether they are used.
    pub(in crate::world_materials) fn prewarm_cached_receivers(&self) -> bool {
        let Some(shadow) = &self.shadows else {
            return false;
        };
        let (Some(buffer), Some(pipelines)) = (&shadow.light, &shadow.light_pipelines) else {
            return false;
        };
        let used = self.lamp_cache.is_some()
            && buffer.receivers.cache.is_some()
            && shadow.cache_group.is_some();
        if used {
            pipelines.receivers.cached(&self.forge.device);
        }
        used
    }

    /// Render the exact winning surface before evaluating its light, without repeated shading.
    pub(super) fn draw_receiver_lighting(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        input: &FrameDraw<'_>,
        region: Option<[f32; 4]>,
        phases: Option<&crate::gpu_phases::Profiler>,
    ) -> bool {
        // The main view's light pass in parts (`SJK_GPU_PHASES`): cache upkeep, receiver
        // attributes, shading, the direct-lamp list; the last part keeps `light-pass`.
        let mark = |encoder: &mut wgpu::CommandEncoder, name| {
            if let Some(phases) = phases {
                phases.mark(encoder, name);
            }
        };
        let shadow = self.shadows.as_ref().unwrap();
        let buffer = shadow.light.as_ref().unwrap();
        let target = &buffer.receivers;

        let pipelines = &shadow.light_pipelines.as_ref().unwrap().receivers;
        // Lamp light comes from the map's static cache when the map, the device and
        // this light buffer all provide for it.
        let cache = self
            .lamp_cache
            .as_ref()
            .zip(target.cache.as_ref())
            .zip(shadow.cache_group.as_ref());
        if let Some(((cache, _), _)) = cache {
            // Keep the bake's targets only for movers that will re-bake parts of it.
            cache.bake_once(
                &self.forge.device,
                encoder,
                input.vertices,
                input.indices,
                &shadow.lamps,
                self.shadow_bounds,
                shadow.movers.as_ref().is_some_and(|m| m.rebakes_cache()),
            );
            // Mover shadows that changed since (`mover_occlusion.rs`).
            if let Some(movers) = &shadow.movers {
                cache.refresh(
                    &self.forge.device,
                    encoder,
                    input.vertices,
                    input.indices,
                    &shadow.lamps,
                    self.shadow_bounds,
                    movers.take_refresh().regions(),
                );
            }
        }
        mark(encoder, "light-cache");
        // Not cleared: lighting reads only the texels the pre-pass covered, and the
        // depth-equal pass below writes every one of them (`receiver_texel`).
        let attachment = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        let scissor = |pass: &mut wgpu::RenderPass<'_>| {
            if let Some(r) = region {
                let [w, h] = buffer.size;
                let x = ((r[0] * w as f32).floor() as u32).saturating_sub(2);
                let y = ((r[1] * h as f32).floor() as u32).saturating_sub(2);
                let right = ((r[2] * w as f32).ceil() as u32 + 2).min(w);
                let bottom = ((r[3] * h as f32).ceil() as u32 + 2).min(h);
                pass.set_scissor_rect(x, y, right - x, bottom - y);
            }
        };
        if let Some(((cache, coordinates), _)) = cache {
            let cached = pipelines.cached(&self.forge.device);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SJK receiver pass"),
                color_attachments: &[
                    attachment(&target.world),
                    attachment(&target.normal),
                    attachment(coordinates),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &buffer.images().depth,
                    depth_ops: None,
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            scissor(&mut pass);

            self.draw_light_geometry(
                &mut pass,
                input,
                Some(&shadow.sun_group),
                Some(&cache.pages),
                |_, mover| &cached.attributes[usize::from(mover)],
                &cached.entity,
            );
        } else {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SJK receiver pass"),
                color_attachments: &[attachment(&target.world), attachment(&target.normal)],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &buffer.images().depth,
                    depth_ops: None,
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            scissor(&mut pass);

            self.draw_light_geometry(
                &mut pass,
                input,
                Some(&shadow.sun_group),
                None,
                |_, mover| &pipelines.attributes[usize::from(mover)],
                &pipelines.entity,
            );
        }
        mark(encoder, "light-receivers");
        let direct = cache.and(target.direct.as_ref());
        if let Some(direct) = direct {
            encoder.copy_buffer_to_buffer(&direct.reset, 0, &direct.list, 0, DIRECT_HEADER);
        }
        let images = buffer.images();
        let lit = [&images.color, &images.direction].map(|view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                // Not cleared: the pass writes every texel of its scissor (black where
                // no receiver is), and a mirror's light is read only inside its region.
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SJK deferred lighting"),
            color_attachments: &lit[..if images.directed { 2 } else { 1 }],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        scissor(&mut pass);
        pass.set_pipeline(cache.map_or(&pipelines.light, |_| {
            &pipelines.cached(&self.forge.device).light
        }));
        pass.set_bind_group(0, input.camera, &[]);
        pass.set_bind_group(1, shadow.light_group(), &[]);
        pass.set_bind_group(2, &target.group, &[]);
        if let Some((_, group)) = cache {
            pass.set_bind_group(3, group, &[]);
        }
        pass.draw(0..3, 0..1);
        drop(pass);
        mark(encoder, "light-shade");
        if let Some(direct) = direct {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("SJK direct lamps"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipelines.cached(&self.forge.device).direct);
            pass.set_bind_group(0, input.camera, &[]);
            pass.set_bind_group(1, shadow.light_group(), &[]);
            pass.set_bind_group(2, &target.group, &[]);
            pass.set_bind_group(3, direct.group(buffer.mirror_active()), &[]);
            pass.dispatch_workgroups_indirect(&direct.list, 0);
        }
        true
    }
}
