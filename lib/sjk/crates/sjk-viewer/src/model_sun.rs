//! Main-view model sunlight, sharing the world's current-frame shadow resources.
use super::*;

/// The extra model program exists only for explicitly enabled custom-sun maps.
pub(super) struct Runtime {
    layout: wgpu::PipelineLayout,
    /// Entity program: the real-time light evaluated per pixel.
    /// The stage program of the real-time mode, world and entities alike: the
    /// half-resolution light buffer upsampled per pixel.
    shader: wgpu::ShaderModule,
    active: wgpu::BindGroup,
    neutral: wgpu::BindGroup,
    /// `active` over the light buffer's mirror images, while floor reflections have them.
    mirror: Option<wgpu::BindGroup>,
    /// A mirror is being drawn: its images are the light buffer's active ones.
    pub(super) mirroring: std::cell::Cell<bool>,
    /// True only after the current main frame has produced a usable shadow fit.
    pub(super) ready: std::cell::Cell<bool>,
}

/// Identical layout for world receivers and model receivers; no copied shadow texture.
/// Materials read only the sun parameters and the already evaluated light buffer.
/// Keep light-evaluation resources out of every material draw’s resource tracking.
pub(super) fn receiver_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    receiver_layout_with(device, super::shadows::light_buffer::Layout::Full)
}

/// Full is the buffered material receiver; Pass and Absent evaluate lighting and
/// therefore retain cascades, probes and lamps. The light pass never binds its colour output.
pub(super) fn receiver_layout_with(
    device: &wgpu::Device,
    light: super::shadows::light_buffer::Layout,
) -> wgpu::BindGroupLayout {
    let depth = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Depth,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    let mut entries = vec![
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
        super::shadows::bounds::layout_entry(),
    ];
    // The GI sun test shares cascade binding 3. Avoid counting the same depth texture
    // twice against the fragment-stage texture limit (including cached lamp inputs).
    entries.extend(
        super::gi_probes::layout_entries(PROBE_BASE)
            .into_iter()
            .filter(|entry| entry.binding != PROBE_BASE + 4),
    );
    entries.extend(super::shadows::light_buffer::layout_entries(
        LIGHT_BASE, light,
    ));
    entries.push(crate::lamp_lights::Gpu::visibility_layout(LAMP_BASE - 5));
    entries.extend(crate::lamp_lights::Gpu::layout_entries(LAMP_BASE));
    // Static lamp visibility uses its atlas, not the obsolete depth placeholder.
    entries.extend(
        super::shadows::lamp_shadows::layout_entries(LAMP_BASE + 4)
            .into_iter()
            .filter(|entry| entry.binding != LAMP_BASE + 4),
    );
    if light == super::shadows::light_buffer::Layout::Full {
        entries.retain(|entry| material_binding(entry.binding));
        // The stage table program reads the point-light block here: its own group holds
        // binding arrays, which exclude uniform buffers, and storage reads are slower.
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: POINT_LIGHTS,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
    }
    if light == super::shadows::light_buffer::Layout::Pass {
        // The light pass's direct-lamp compute program evaluates the same light.
        for entry in &mut entries {
            entry.visibility |= wgpu::ShaderStages::COMPUTE;
        }
    }
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SJK shared sun receiver"),
        entries: &entries,
    })
}

/// Bindings consumed by `sun_realtime_buffer.wgsl`, shared by lit world and models, and
/// the light directions the material-map program reads (41).
fn material_binding(binding: u32) -> bool {
    matches!(binding, 2 | 20 | 21 | 23 | 41)
}

/// The point-light block in the material receiver group (stage table program only).
pub(super) const POINT_LIGHTS: u32 = 40;

/// First probe binding inside the receiver group.
pub(super) const PROBE_BASE: u32 = 8;

/// First light-buffer binding (colour, then depth) inside the receiver group.
pub(super) const LIGHT_BASE: u32 = super::shadows::light_buffer::BASE;

/// First lamp binding (grid, lamps, cells, lists) inside the receiver group.
pub(super) const LAMP_BASE: u32 = 24;

/// Build a receiver group from the cascade entries plus the probe volume or its neutral
/// stand-ins, sharing the far depth view and comparison sampler for the probes' sun test.
pub(super) fn receiver_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    cascades: &[wgpu::BindGroupEntry<'_>],
    probes: Option<&super::gi_probes::Runtime>,
    comparison: &wgpu::Sampler,
    light: super::shadows::light_buffer::Binding<'_>,
    lamps: &crate::lamp_lights::Gpu,
    lamp_shadows: Option<&super::shadows::lamp_shadows::Runtime>,
    point_lights: &wgpu::Buffer,
) -> wgpu::BindGroup {
    let neutral_shadows = lamp_shadows
        .is_none()
        .then(|| super::shadows::lamp_shadows::neutral(device));
    use super::shadows::light_buffer::Binding;
    let neutral = super::gi_probes::Neutral::new(device);
    let neutral_light =
        matches!(light, Binding::Neutral).then(|| super::shadows::light_buffer::neutral(device));
    let (params, sh, depth, state) = probes.map_or(
        (
            &neutral.params,
            &neutral.storage,
            &neutral.storage,
            &neutral.storage,
        ),
        |p| (&p.params, &p.display, &p.depth, &p.state),
    );

    let mut entries = cascades.to_vec();

    entries.extend([
        wgpu::BindGroupEntry {
            binding: PROBE_BASE,
            resource: params.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: PROBE_BASE + 1,
            resource: sh.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: PROBE_BASE + 2,
            resource: depth.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: PROBE_BASE + 3,
            resource: state.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: PROBE_BASE + 5,
            resource: wgpu::BindingResource::Sampler(comparison),
        },
    ]);
    match (&light, &neutral_light) {
        (Binding::Buffer(buffer) | Binding::Pass(buffer), _) => {
            entries.extend(buffer.entries(LIGHT_BASE, light.layout()))
        }
        (Binding::Neutral, Some((color, depth, normal, direction))) => entries.extend([
            wgpu::BindGroupEntry {
                binding: LIGHT_BASE,
                resource: wgpu::BindingResource::TextureView(color),
            },
            wgpu::BindGroupEntry {
                binding: LIGHT_BASE + 1,
                resource: wgpu::BindingResource::TextureView(depth),
            },
            wgpu::BindGroupEntry {
                binding: LIGHT_BASE + 3,
                resource: wgpu::BindingResource::TextureView(normal),
            },
            wgpu::BindGroupEntry {
                binding: LIGHT_BASE + super::shadows::light_buffer::DIRECTION,
                resource: wgpu::BindingResource::TextureView(direction),
            },
        ]),
        _ => {}
    }
    entries.push(lamps.visibility_entry(LAMP_BASE - 5));
    entries.extend(lamps.entries(LAMP_BASE));
    match (lamp_shadows, &neutral_shadows) {
        (Some(shadows), _) => entries.extend(shadows.entries(LAMP_BASE + 4)),
        (None, Some((statics, dynamics, table))) => entries.extend([
            wgpu::BindGroupEntry {
                binding: LAMP_BASE + 4,
                resource: wgpu::BindingResource::TextureView(statics),
            },
            wgpu::BindGroupEntry {
                binding: LAMP_BASE + 5,
                resource: wgpu::BindingResource::TextureView(dynamics),
            },
            wgpu::BindGroupEntry {
                binding: LAMP_BASE + 6,
                resource: table.as_entire_binding(),
            },
        ]),
        _ => {}
    }
    entries.retain(|entry| entry.binding != LAMP_BASE + 4);
    if light.layout() == super::shadows::light_buffer::Layout::Full {
        entries.retain(|entry| material_binding(entry.binding));
        entries.push(wgpu::BindGroupEntry {
            binding: POINT_LIGHTS,
            resource: point_lights.as_entire_binding(),
        });
    }
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("SJK sun receiver"),
        layout,
        entries: &entries,
    })
}

impl Runtime {
    /// Without an `active` group (chosen at forge creation) the program binds its neutral
    /// group until the sun resources exist.
    pub(super) fn new(
        device: &wgpu::Device,
        forge: &Forge,
        active: Option<&wgpu::BindGroup>,
    ) -> Self {
        let receiver = receiver_layout(device);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK model sun program"),
            bind_group_layouts: &[
                Some(&forge.camera_layout),
                Some(&forge.stage_layout),
                Some(&crate::shared_geometry::quads::layout(device)),
                Some(&receiver),
            ],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK light buffer stage program"),
            source: wgpu::ShaderSource::Wgsl({
                let source = world_sun_shader();

                source.into()
            }),
        });
        // A separate inert binding prevents secondary views from sampling a main-view map.
        let depth = crate::DepthTarget::new(device, 1, 1);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK model sun disabled"),
            size: std::mem::size_of::<super::shadows::Parameters>() as u64,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });
        let neutral = receiver_group(
            device,
            &receiver,
            &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&depth.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&depth.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&depth.view),
                },
            ],
            None,
            &sampler,
            super::shadows::light_buffer::Binding::Neutral,
            &crate::lamp_lights::Gpu::new(device, &crate::lamp_lights::LampSet::default(), &[]),
            None,
            &forge.point_lights,
        );
        Self {
            layout,
            shader,
            active: active.cloned().unwrap_or_else(|| neutral.clone()),
            neutral,
            mirror: None,
            mirroring: std::cell::Cell::new(false),
            ready: std::cell::Cell::new(false),
        }
    }

    /// The material group over the light buffer's mirror images, if it has them.
    pub(super) fn set_mirror(&mut self, mirror: Option<&wgpu::BindGroup>) {
        self.mirror = mirror.cloned();
        self.mirroring.set(false);
    }

    /// A failed or not-yet-rendered fit cannot expose the previous frame's sun.
    pub(super) fn binding(&self, main_view: bool) -> &wgpu::BindGroup {
        if !main_view || !self.ready.get() {
            return &self.neutral;
        }
        match &self.mirror {
            Some(mirror) if self.mirroring.get() => mirror,
            _ => &self.active,
        }
    }
}

impl Forge {
    /// Select the current entity program without changing the world/flare layouts.
    pub(super) fn program(&self) -> (&wgpu::PipelineLayout, &wgpu::ShaderModule) {
        self.model_sun
            .as_ref()
            .map_or((&self.pipeline_layout, &self.shader), |sun| {
                (&sun.layout, &sun.shader)
            })
    }
}

impl super::Runtime {
    /// Rebuild world and entity variants at map installation, never on a day-clock tick.
    pub(super) fn configure_model_sun(
        &mut self,
        device: &wgpu::Device,
        active: Option<&wgpu::BindGroup>,
        mirror: Option<&wgpu::BindGroup>,
    ) {
        if let Some(sun) = &mut self.forge.model_sun {
            // Program already compiled into every pipeline: only the live group changes.
            sun.active = active.cloned().unwrap_or_else(|| sun.neutral.clone());
            sun.set_mirror(mirror);
            return;
        }
        let Some(active) = active else {
            return;
        };
        // Every slot starts empty again and compiles against the new program on first use.
        let mut sun = Runtime::new(device, &self.forge, Some(active));
        sun.set_mirror(mirror);
        self.forge.model_sun = Some(sun);
        if let Some(table) = &mut self.stage_table {
            table.reset_program();
        }
        if let Some(maps) = &mut self.forge.material_maps {
            maps.reset_program();
        }
        let count = self.forge.pipeline_keys.len();
        self.entity_pipelines = (0..count).map(|_| std::cell::OnceCell::new()).collect();
        self.entity_no_depth_pipelines = (0..count).map(|_| std::cell::OnceCell::new()).collect();
        self.glow_pipelines.reset(count);
        self.card_outline_mask = std::cell::OnceCell::new();
        self.pipeline_generation = self.pipeline_generation.wrapping_add(1);
    }
}
