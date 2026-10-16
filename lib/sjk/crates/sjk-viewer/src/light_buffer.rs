//! Half-resolution light buffer of the real-time mode. Depth/normal geometry selects
//! visible receivers; a depth-equal pass stores their full-precision attributes, keeping
//! the last coplanar draw and its derivative-dependent sun visibility. One fullscreen
//! pass evaluates lamps, probes and contact shadows per pixel. The stage shader
//! upsamples with a depth-aware filter (`sun_realtime_buffer.wgsl`), keeping the
//! world program small without changing its lighting resolution.
use super::*;
#[path = "light_buffer_preserve.rs"]
mod preserve;
#[path = "light_receivers.rs"]
mod receivers;
pub(in crate::world_materials) use receivers::sun_layout;

/// Colour and depth targets sized to the current scene; rebuilt on resize.
pub(in crate::world_materials) struct LightBuffer {
    /// Scene size and divisor the buffer was fitted to.
    pub(super) scene: [u32; 2],
    pub(super) divisor: u32,
    size: [u32; 2],
    /// The light pass also writes where the non-sun light comes from (`Images::direction`):
    /// the map has material maps that redistribute it.
    pub(super) directed: bool,
    /// The main view's images.
    main: Images,
    /// The images mirrors are lit into while floor reflections are enabled, so the main
    /// view's light stays intact around them (`preserve`).
    mirror: Option<Images>,
    /// A mirror is being lit or drawn: `mirror` holds the active images.
    mirroring: std::cell::Cell<bool>,
    /// The occlusion term, filtered, with the scene-to-texel scale: what the SSAO
    /// receivers pass samples in day mode instead of its own obscurance.
    pub(in crate::world_materials) sample_group: wgpu::BindGroup,
    receivers: receivers::Targets,
}

/// One set of light-buffer images: the main view's, or the set mirrors are lit into.
pub(in crate::world_materials) struct Images {
    pub(super) color: wgpu::TextureView,
    pub(super) depth: wgpu::TextureView,
    /// Dominant direction of the non-sun light and its directional share
    /// (`DirectedLight` in `sun_realtime.wgsl`); one texel when the buffer is not directed.
    pub(super) direction: wgpu::TextureView,
    /// `direction` covers the buffer: the light pass writes it.
    pub(super) directed: bool,
    /// Pre-pass normals and the occlusion term computed from them.
    normal: wgpu::TextureView,
    occlusion: wgpu::TextureView,
    /// Occlusion pass group: depth and normals in, occlusion out.
    occlusion_group: wgpu::BindGroup,
}

impl Images {
    fn new(device: &wgpu::Device, size: [u32; 2], directed: bool) -> Self {
        let (color, depth) = targets(device, size);
        let direction = target(
            device,
            if directed { size } else { [1, 1] },
            "SJK light buffer directions",
            DIRECTION_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::STORAGE_BINDING,
        );
        let normal = target(
            device,
            size,
            "SJK light buffer normals",
            NORMAL_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let occlusion = target(
            device,
            size,
            "SJK light buffer occlusion",
            OCCLUSION_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let occlusion_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK light occlusion"),
            layout: &occlusion_layout(device),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&depth),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&normal),
                },
            ],
        });
        Self {
            color,
            depth,
            direction,
            directed,
            normal,
            occlusion,
            occlusion_group,
        }
    }

    /// Bind group entries at `base` for `layout` (see `layout_entries`).
    pub(in crate::world_materials) fn entries(
        &self,
        base: u32,
        layout: Layout,
    ) -> Vec<wgpu::BindGroupEntry<'_>> {
        let entry = |binding, view| wgpu::BindGroupEntry {
            binding: base + binding,
            resource: wgpu::BindingResource::TextureView(view),
        };
        match layout {
            Layout::Absent => Vec::new(),
            Layout::Pass => vec![
                entry(1, &self.depth),
                entry(2, &self.occlusion),
                entry(3, &self.normal),
            ],
            Layout::Full => vec![
                entry(0, &self.color),
                entry(1, &self.depth),
                entry(3, &self.normal),
                entry(DIRECTION, &self.direction),
            ],
        }
    }
}

/// Layout of `LightBuffer::sample_group`.
pub(in crate::world_materials) fn sample_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SJK light occlusion sample"),
        entries: &[
            texture_entry(
                0,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::TextureSampleType::Float { filterable: true },
            ),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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
        ],
    })
}

/// What a receiver group carries for the light buffer.
pub(in crate::world_materials) enum Binding<'a> {
    /// No light-buffer bindings at all (the light pass group of the baked mode).
    Absent,
    /// The light pass's own group: it writes the colour, so it binds only the pre-pass
    /// depth, normals and the occlusion term.
    Pass(&'a Images),
    /// Stand-ins for the neutral model group and the baked mode.
    Neutral,
    Buffer(&'a Images),
}

/// Which light-buffer bindings a receiver layout declares.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::world_materials) enum Layout {
    Absent,
    Pass,
    Full,
}

impl Binding<'_> {
    /// The layout this binding fills.
    pub(in crate::world_materials) fn layout(&self) -> Layout {
        match self {
            Binding::Absent => Layout::Absent,
            Binding::Pass(_) => Layout::Pass,
            Binding::Neutral | Binding::Buffer(_) => Layout::Full,
        }
    }
}

/// First light-buffer binding inside the receiver group: colour, depth, then occlusion.
pub(in crate::world_materials) const BASE: u32 = 20;
/// The direction target's binding after [`BASE`] (41: past the point-light block, 40).
pub(in crate::world_materials) const DIRECTION: u32 = 21;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Octahedral direction, directional share and lamp share of the non-sun light.
const DIRECTION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgb10a2Unorm;
const OCCLUSION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

fn texture_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    sample_type: wgpu::TextureSampleType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

/// Layout entries for the light-buffer bindings at `base`: colour, depth, occlusion,
/// normal. The stage shader (Full) reads colour, depth and normal; the light pass (Pass)
/// reads depth, normals and occlusion.
pub(in crate::world_materials) fn layout_entries(
    base: u32,
    layout: Layout,
) -> Vec<wgpu::BindGroupLayoutEntry> {
    let texture = |binding, sample_type| {
        texture_entry(base + binding, wgpu::ShaderStages::FRAGMENT, sample_type)
    };
    let color = texture(0, wgpu::TextureSampleType::Float { filterable: false });
    let depth = texture(1, wgpu::TextureSampleType::Depth);
    let occlusion = texture(2, wgpu::TextureSampleType::Float { filterable: false });
    let normal = texture(3, wgpu::TextureSampleType::Float { filterable: false });
    let direction = texture(
        DIRECTION,
        wgpu::TextureSampleType::Float { filterable: false },
    );
    match layout {
        Layout::Absent => Vec::new(),
        Layout::Pass => vec![depth, occlusion, normal],
        Layout::Full => vec![color, depth, normal, direction],
    }
}

/// The occlusion pass group layout: pre-pass depth and normals in.
fn occlusion_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SJK light occlusion"),
        entries: &[
            texture_entry(
                0,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::TextureSampleType::Depth,
            ),
            texture_entry(
                1,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::TextureSampleType::Float { filterable: false },
            ),
        ],
    })
}

fn target(
    device: &wgpu::Device,
    size: [u32; 2],
    label: &str,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: usage
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

fn targets(device: &wgpu::Device, size: [u32; 2]) -> (wgpu::TextureView, wgpu::TextureView) {
    let attachment = wgpu::TextureUsages::RENDER_ATTACHMENT;
    (
        target(
            device,
            size,
            "SJK light buffer",
            FORMAT,
            attachment | wgpu::TextureUsages::STORAGE_BINDING,
        ),
        target(
            device,
            size,
            "SJK light buffer depth",
            crate::DepthTarget::FORMAT,
            attachment,
        ),
    )
}

/// One-texel stand-ins (colour, depth, normal, direction) for groups without a buffer.
pub(in crate::world_materials) fn neutral(
    device: &wgpu::Device,
) -> (
    wgpu::TextureView,
    wgpu::TextureView,
    wgpu::TextureView,
    wgpu::TextureView,
) {
    let (color, depth) = targets(device, [1, 1]);
    let normal = target(
        device,
        [1, 1],
        "SJK light buffer neutral normals",
        NORMAL_FORMAT,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    let direction = target(
        device,
        [1, 1],
        "SJK light buffer neutral directions",
        DIRECTION_FORMAT,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    (color, depth, normal, direction)
}

impl LightBuffer {
    /// `divisor` is the scene pixels per buffer texel per axis: two for a native scene,
    /// twice the supersampling factor when the scene is supersampled, so the lighting is
    /// always evaluated at half the display resolution (one for evidence only).
    pub(super) fn new(
        device: &wgpu::Device,
        scene: [u32; 2],
        divisor: u32,
        directed: bool,
    ) -> Self {
        let divisor = divisor.max(1);
        let size = [
            scene[0].div_ceil(divisor).max(1),
            scene[1].div_ceil(divisor).max(1),
        ];
        let main = Images::new(device, size, directed);
        let scale = [
            size[0] as f32 / scene[0].max(1) as f32,
            size[1] as f32 / scene[1].max(1) as f32,
            0.,
            0.,
        ];
        let scale_buffer = wgpu::util::DeviceExt::create_buffer_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("SJK light occlusion scale"),
                contents: bytemuck::cast_slice(&scale),
                usage: wgpu::BufferUsages::UNIFORM,
            },
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let sample_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK light occlusion sample"),
            layout: &sample_layout(device),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&main.occlusion),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: scale_buffer.as_entire_binding(),
                },
            ],
        });
        let receivers = receivers::Targets::new(device, size, &main);
        Self {
            scene,
            divisor,
            size,
            directed,
            main,
            mirror: None,
            mirroring: std::cell::Cell::new(false),
            sample_group,
            receivers,
        }
    }

    /// The images passes draw into and materials read: the mirror set while a mirror is
    /// lit or drawn, otherwise the main view's.
    pub(in crate::world_materials) fn images(&self) -> &Images {
        match &self.mirror {
            Some(mirror) if self.mirroring.get() => mirror,
            _ => &self.main,
        }
    }

    /// The main view's images.
    pub(in crate::world_materials) fn main_images(&self) -> &Images {
        &self.main
    }

    /// The mirror images, while floor reflections are enabled.
    pub(in crate::world_materials) fn mirror_images(&self) -> Option<&Images> {
        self.mirror.as_ref()
    }

    /// The mirror set is the active one.
    pub(in crate::world_materials) fn mirror_active(&self) -> bool {
        self.mirroring.get() && self.mirror.is_some()
    }

    /// The receivers' lamp cache coordinate target, when the device can attach it.
    pub(in crate::world_materials) fn receiver_cache(&self) -> Option<&wgpu::TextureView> {
        self.receivers.cache.as_ref()
    }

    /// The list of receivers the lamp cache cannot serve, beside the coordinate target.
    pub(in crate::world_materials) fn direct_list(&self) -> Option<&wgpu::Buffer> {
        self.receivers.direct.as_ref().map(|direct| &direct.list)
    }

    /// Scale from full-resolution pixel coordinates to buffer texel coordinates.
    pub(super) fn scale(&self) -> [f32; 2] {
        [
            self.size[0] as f32 / self.scene[0].max(1) as f32,
            self.size[1] as f32 / self.scene[1].max(1) as f32,
        ]
    }
}

/// Cull modes by the material's cull index (see `Material::light_buffered`).
const CULL: [Option<wgpu::Face>; 3] = [Some(wgpu::Face::Front), Some(wgpu::Face::Back), None];

/// The pass pipelines; independent of the buffer size.
pub(super) struct Pipelines {
    /// Depth and normal pre-pass per cull mode (front, back, none), static then mover.
    prepass: [wgpu::RenderPipeline; 6],

    /// Fullscreen occlusion over the pre-pass, before the light pass reads it.
    occlusion: wgpu::RenderPipeline,

    /// The entity pre-pass and light pass: the stage vertex path, one pipeline each.
    entity_prepass: wgpu::RenderPipeline,

    receivers: receivers::Pipelines,
}

impl Pipelines {
    pub(super) fn new(
        device: &wgpu::Device,
        forge: &Forge,
        receiver: &wgpu::BindGroupLayout,
        directed: bool,
    ) -> Self {
        // The pre-pass writes the depth the receiver group samples: camera only.
        let prepass_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK light pre-pass"),
            bind_group_layouts: &[Some(&forge.camera_layout)],
            immediate_size: 0,
        });
        let shade = include_str!("light_receivers_shade.wgsl").to_string();

        let program = format!(
            "{}{}{}",
            source(),
            include_str!("light_receivers.wgsl"),
            shade
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK light pass"),
            source: wgpu::ShaderSource::Wgsl(program.as_str().into()),
        });
        let static_buffers = [Some(crate::GpuVertex::layout())];
        let mover_buffers = [
            Some(crate::GpuVertex::layout()),
            Some(crate::ActorInstance::layout()),
        ];
        let vertex = |mover: bool| wgpu::VertexState {
            module: &shader,
            entry_point: Some(if mover {
                "mover_vertex"
            } else {
                "static_vertex"
            }),
            compilation_options: Default::default(),
            buffers: if mover {
                &mover_buffers[..]
            } else {
                &static_buffers[..]
            },
        };
        let depth = |write, compare| {
            Some(wgpu::DepthStencilState {
                format: crate::DepthTarget::FORMAT,
                depth_write_enabled: Some(write),
                depth_compare: Some(compare),
                stencil: Default::default(),
                bias: Default::default(),
            })
        };
        let prepass = [
            (0, false),
            (1, false),
            (2, false),
            (0, true),
            (1, true),
            (2, true),
        ]
        .map(|(cull, mover)| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("SJK light pre-pass"),
                layout: Some(&prepass_layout),
                vertex: vertex(mover),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("normal_fragment"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: NORMAL_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: CULL[cull],
                    ..Default::default()
                },
                depth_stencil: depth(true, wgpu::CompareFunction::Less),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        });

        let occlusion_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK light occlusion"),
            source: wgpu::ShaderSource::Wgsl(include_str!("light_occlusion.wgsl").into()),
        });
        let occlusion_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK light occlusion"),
            bind_group_layouts: &[Some(&forge.camera_layout), Some(&occlusion_layout(device))],
            immediate_size: 0,
        });
        let occlusion = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SJK light occlusion"),
            layout: Some(&occlusion_layout),
            vertex: wgpu::VertexState {
                module: &occlusion_shader,
                entry_point: Some("fullscreen"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &occlusion_shader,
                entry_point: Some("occlude"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: OCCLUSION_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let entity_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK entity light pass"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{}{}",
                    super::super::entity_light_shader(),
                    include_str!("light_receivers.wgsl")
                )
                .into(),
            ),
        });
        let quads = crate::shared_geometry::quads::layout(device);
        let entity_prepass_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("SJK entity light pre-pass"),
                bind_group_layouts: &[
                    Some(&forge.camera_layout),
                    Some(&forge.stage_layout),
                    Some(&quads),
                ],
                immediate_size: 0,
            });

        // Deforming materials deform here too, and surface sprites grow here too, so the
        // buffer's depth equals the scene's. Both paths are gated by the stage's own data.
        let deforms = [("geometry_deforms", 1.0), ("geometry_sprites", 1.0)];
        let entity_vertex = wgpu::VertexState {
            module: &entity_shader,
            entry_point: Some("entity_light_vertex"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &deforms,
                ..Default::default()
            },
            buffers: &mover_buffers,
        };
        let entity_prepass = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SJK entity light pre-pass"),
            layout: Some(&entity_prepass_layout),
            vertex: entity_vertex.clone(),
            fragment: Some(wgpu::FragmentState {
                module: &entity_shader,
                entry_point: Some("entity_normal_fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: NORMAL_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: depth(true, wgpu::CompareFunction::Less),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        Self {
            prepass,

            occlusion,
            entity_prepass,

            receivers: receivers::Pipelines::new(
                device,
                forge,
                receiver,
                &shader,
                &entity_shader,
                directed,
            ),
        }
    }
}

/// The light pass program: shared visibility, probes and real-time light at group 1.
fn source() -> String {
    let (visibility, probes, realtime) = super::super::realtime_sources(1);
    format!(
        "{}{}{}{}{}{}{}{}",
        include_str!("vertex_transform.wgsl"),
        include_str!("surface_orientation.wgsl"),
        include_str!("sun_shadow_uniform.wgsl"),
        visibility,
        probes,
        realtime,
        include_str!("light_pass.wgsl"),
        include_str!("sun_contact.wgsl")
    )
}

impl Pipelines {}

impl super::super::Runtime {
    /// Main view, day mode: the opaque world into the half-resolution light buffer, after
    /// this frame's cascades and before the scene pass that samples it.
    pub(crate) fn draw_light_buffer(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        input: &FrameDraw<'_>,
        phases: Option<&crate::gpu_phases::Profiler>,
    ) {
        self.draw_light_buffer_region(encoder, input, phases, None, [1., 1.], None);
    }

    /// Restrict expensive shading to a reflected floor's normalized screen coverage.
    /// Depth stays complete for neighborhood samples. Occlusion output can be cropped
    /// independently; its inputs still cover the whole reflected view. `raster` is the
    /// fraction of the buffer (x, y) a packed view draws into, from its upper-left corner.
    pub(crate) fn draw_light_buffer_region(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        input: &FrameDraw<'_>,
        phases: Option<&crate::gpu_phases::Profiler>,
        region: Option<[f32; 4]>,
        raster: [f32; 2],
        occlusion_region: Option<[f32; 4]>,
    ) {
        let Some(shadow) = &self.shadows else {
            return;
        };
        let (Some(buffer), Some(pipelines)) = (&shadow.light, &shadow.light_pipelines) else {
            return;
        };
        let images = buffer.images();
        let depth = |ops| {
            Some(wgpu::RenderPassDepthStencilAttachment {
                view: &images.depth,
                depth_ops: ops,
                stencil_ops: None,
            })
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SJK light pre-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &images.normal,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: depth(Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.),
                    store: wgpu::StoreOp::Store,
                })),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_scissor_rect(
                0,
                0,
                ((buffer.size[0] as f32 * raster[0]).ceil() as u32).min(buffer.size[0]),
                ((buffer.size[1] as f32 * raster[1]).ceil() as u32).min(buffer.size[1]),
            );

            self.draw_light_geometry(
                &mut pass,
                input,
                None,
                None,
                |cull, mover| &pipelines.prepass[cull + 3 * usize::from(mover)],
                &pipelines.entity_prepass,
            );
        }
        if let Some(phases) = phases {
            phases.mark(encoder, "light-prepass");
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SJK light occlusion"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &images.occlusion,
                    resolve_target: None,
                    depth_slice: None,
                    // Not cleared: the pass writes every texel of its scissor, and a
                    // cropped mirror pass is read only inside that crop.
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
            if let Some(r) = occlusion_region {
                // Receiver rasterization already adds two texels. Its aligned 2x2 AO
                // gather can reach one further texel; four preserve both footprints.
                let [w, h] = buffer.size;
                let x = ((r[0] * w as f32).floor() as u32).saturating_sub(4);
                let y = ((r[1] * h as f32).floor() as u32).saturating_sub(4);
                let right = ((r[2] * w as f32).ceil() as u32 + 4).min(w);
                let bottom = ((r[3] * h as f32).ceil() as u32 + 4).min(h);
                pass.set_scissor_rect(x, y, right - x, bottom - y);
            } else {
                pass.set_scissor_rect(
                    0,
                    0,
                    ((buffer.size[0] as f32 * raster[0]).ceil() as u32).min(buffer.size[0]),
                    ((buffer.size[1] as f32 * raster[1]).ceil() as u32).min(buffer.size[1]),
                );
            }
            pass.set_pipeline(&pipelines.occlusion);
            pass.set_bind_group(0, input.camera, &[]);
            pass.set_bind_group(1, &images.occlusion_group, &[]);
            pass.draw(0..3, 0..1);
        }
        if let Some(phases) = phases {
            phases.mark(encoder, "light-occlusion");
        }
        if self.draw_receiver_lighting(encoder, input, region, phases) {
            return;
        }
    }

    /// Every light-buffered material's visible static and mover draws, then the frame's
    /// opaque entities with `entity` (stage group 1, geometry group 2, receiver group 3).
    /// `pages` is the lamp cache's vertex stream: static draws read it at slot 1, where
    /// movers and entities read their instances.
    fn draw_light_geometry<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        input: &FrameDraw<'a>,
        receiver: Option<&'a wgpu::BindGroup>,
        pages: Option<&'a wgpu::Buffer>,
        pipeline: impl Fn(usize, bool) -> &'a wgpu::RenderPipeline,
        entity: &'a wgpu::RenderPipeline,
    ) {
        pass.set_bind_group(0, input.camera, &[]);
        if let Some(receiver) = receiver {
            pass.set_bind_group(1, receiver, &[]);
        }
        pass.set_vertex_buffer(0, input.vertices.slice(..));
        pass.set_vertex_buffer(1, input.instances.slice(..));
        pass.set_index_buffer(input.indices.slice(..), wgpu::IndexFormat::Uint32);
        let mut last = None;
        let active = self.active_materials(input.source_cluster, input.visibility);
        // Static surfaces: one indirect list per view, replayed by each pass with a
        // multi-draw per cull mode. Order within a cull mode is the direct path's.
        let listed = self.indirect.as_ref().and_then(|indirect| {
            indirect
                .view(
                    &self.forge.queue,
                    super::super::indirect_draws::Key {
                        source: input.source_cluster,
                        camera: self.view_culling.active_generation(),
                        area: self.areas.revision(),

                        pvs: input.visibility.is_some(),
                    },
                    |push| {
                        for material in active.list.iter().map(|&index| &self.materials[index]) {
                            let Some(cull) = material.light_buffered else {
                                continue;
                            };
                            for range in self.visible_static_ranges(
                                material,
                                input.source_cluster,
                                input.visibility,
                            ) {
                                push(usize::from(cull), range);
                            }
                        }
                    },
                )
                .map(|view| (view, self.indirect.as_ref().unwrap()))
        });
        if let Some((view, indirect)) = listed {
            for cull in 0..super::super::indirect_draws::BUCKETS {
                if view.counts[cull] == 0 {
                    continue;
                }
                pass.set_pipeline(pipeline(cull, false));
                if let Some(pages) = pages {
                    pass.set_vertex_buffer(1, pages.slice(..));
                }
                pass.multi_draw_indexed_indirect(
                    &indirect.buffer,
                    view.offsets[cull],
                    view.counts[cull],
                );
            }
        }
        for material in active.list.iter().map(|&index| &self.materials[index]) {
            let Some(cull) = material.light_buffered else {
                continue;
            };
            let cull = usize::from(cull);
            if listed.is_none() {
                let mut ranges = self
                    .visible_static_ranges(material, input.source_cluster, input.visibility)
                    .peekable();
                if ranges.peek().is_some() {
                    if last != Some((cull, false)) {
                        pass.set_pipeline(pipeline(cull, false));
                        if let Some(pages) = pages {
                            pass.set_vertex_buffer(1, pages.slice(..));
                        }
                        last = Some((cull, false));
                    }
                    for range in ranges {
                        pass.draw_indexed(range, 0, 0..1);
                    }
                }
            }
            if !material.mover_draws.is_empty() {
                if last != Some((cull, true)) {
                    pass.set_pipeline(pipeline(cull, true));
                    if pages.is_some() {
                        pass.set_vertex_buffer(1, input.instances.slice(..));
                    }
                    last = Some((cull, true));
                }
                for draw in &material.mover_draws {
                    if let Some(range) = input
                        .mover_ranges
                        .get(draw.mesh)
                        .filter(|range| !range.is_empty())
                    {
                        pass.draw_indexed(draw.indices.clone(), 0, range.clone());
                    }
                }
            }
        }
        let cutouts = active
            .list
            .iter()
            .any(|&index| self.materials[index].light_cutout);
        if input.entities.is_empty() && !cutouts {
            return;
        }
        pass.set_pipeline(entity);
        pass.set_bind_group(2, &self.forge.geometry, &[]);
        if let Some(receiver) = receiver {
            pass.set_bind_group(3, receiver, &[]);
        }
        // Cut-out world surfaces take the entity pipeline for its alpha test, as the one
        // identity instance.
        if cutouts {
            pass.set_vertex_buffer(1, self.forge.identity_instance.slice(..));
            for material in active
                .list
                .iter()
                .map(|&index| &self.materials[index])
                .filter(|material| material.light_cutout)
            {
                let mut ranges = self
                    .visible_static_ranges(material, input.source_cluster, input.visibility)
                    .peekable();
                if ranges.peek().is_none() {
                    continue;
                }
                pass.set_bind_group(1, &material.stages[0].bind_group, &[]);
                for range in ranges {
                    pass.draw_indexed(range, 0, 0..1);
                }
            }
        }
        if input.entities.is_empty() {
            return;
        }
        pass.set_vertex_buffer(1, input.instances.slice(..));
        let mut last = None;
        for draw in input.entities {
            let Some(stage) = self
                .source_to_runtime
                .get(draw.material)
                .copied()
                .filter(|index| *index != usize::MAX)
                .and_then(|index| self.materials[index].stages.first().map(|s| (index, s)))
            else {
                continue;
            };
            if last != Some(stage.0) {
                pass.set_bind_group(1, &stage.1.bind_group, &[]);
                last = Some(stage.0);
            }
            pass.draw_indexed(draw.indices.clone(), 0, draw.instances.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The light pass program as `Pipelines::new` assembles it.
    fn program() -> String {
        format!(
            "{}{}{}",
            source(),
            include_str!("light_receivers.wgsl"),
            include_str!("light_receivers_shade.wgsl")
        )
    }

    #[test]
    fn light_pass_programs_validate_with_and_without_directions() {
        let program = program();
        crate::wgsl_source::validate(&program);
        for entry in [
            "receiver_light",
            "receiver_light_cached",
            "direct_lamps",
            "receiver_light_directed",
            "receiver_light_cached_directed",
            "direct_lamps_directed",
        ] {
            assert!(program.contains(&format!("fn {entry}(")), "{entry}");
        }
        crate::wgsl_source::validate(&super::super::super::entity_light_shader());
    }

    #[test]
    fn direction_binding_follows_the_point_light_block() {
        // The material receiver group keeps 40 for the point lights; the material-map
        // program reads the directions at 41.
        assert_eq!(BASE + DIRECTION, 41);
        assert!(
            include_str!("material_maps_realtime.wgsl")
                .contains("@group(3) @binding(41) var light_direction")
        );
        let full = layout_entries(BASE, Layout::Full);
        assert!(full.iter().any(|entry| entry.binding == 41));
        assert!(
            layout_entries(BASE, Layout::Pass)
                .iter()
                .all(|entry| entry.binding != 41)
        );
    }
}
