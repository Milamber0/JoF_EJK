//! Main-view participating media. Map-owned resources; no temporal history or readback.
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;
#[path = "volumetric_range.rs"]
mod coverage;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Parameters {
    inverse: [[f32; 4]; 4],
    shadow: [[f32; 4]; 4],
    eye: [f32; 4],
    forward: [f32; 4],
    sun: [f32; 4],
    color: [f32; 4],
    grid: [u32; 4],
    range: [f32; 4],
    /// Close cascade transform; `close_range` = texel, axial depth, present flag, unused.
    close: [[f32; 4]; 4],
    close_range: [f32; 4],
    /// Map-wide shadow fit, then texel size, present flag, volume endpoint, near slice count.
    far: [[f32; 4]; 4],
    far_range: [f32; 4],
}

/// Fixed quality levels; dimensions are independent of the output framebuffer size.
pub(super) fn dimensions(level: u32) -> [u32; 3] {
    match level {
        1 => [80, 45, 48],
        2 => [128, 72, 64],
        _ => [192, 108, 96],
    }
}

/// Wide low-pass tiles per slice; bilinear across tiles is smooth, so no seams or halos.
pub(super) const TILES: u32 = 6;

/// Two half-float volumes and their immutable compute/composite bindings.
pub(super) struct Runtime {
    ready: std::cell::Cell<bool>,
    parameters: wgpu::Buffer,
    inject: wgpu::ComputePipeline,
    injection: wgpu::BindGroup,
    /// Per-column surface range ahead of injection; absent for the diagnostic shaders.
    columns: Option<(wgpu::ComputePipeline, wgpu::BindGroup)>,
    reduce: wgpu::ComputePipeline,
    reduction: wgpu::BindGroup,
    integrate: wgpu::ComputePipeline,
    integration: wgpu::BindGroup,
    composite: wgpu::RenderPipeline,
    composition: wgpu::BindGroup,
    dust_beams: wgpu::BindGroup,
    grid: [u32; 3],
    near_layers: u32,
    fog_count: u32,
    gain: f32,
    scattering: f32,
}

impl Runtime {
    /// Prevent a skipped or invalid shadow update from reusing a previous volume.
    pub(super) fn invalidate(&self) {
        self.ready.set(false);
    }

    /// Allocate once with the world shadow map; absence of that map has no haze fallback.
    pub(super) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        shadow: &wgpu::TextureView,
        close: &wgpu::TextureView,
        far: Option<&wgpu::TextureView>,
        world: Option<[&wgpu::TextureView; 2]>,
        fog: &crate::fog_volumes::Table,
        level: u32,
    ) -> Self {
        let mut grid = dimensions(level);
        let near_layers = grid[2];
        let extended = far.is_some();

        if extended {
            grid[2] += near_layers / 2;
        }
        let volume = |label| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: grid[0],
                        height: grid[1],
                        depth_or_array_layers: grid[2],
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D3,
                    format: wgpu::TextureFormat::Rgba16Float,
                    usage: wgpu::TextureUsages::STORAGE_BINDING
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let injected = volume("SJK froxel sunlit source");
        let integrated = volume("SJK froxel additive scattering");
        let column_range = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("SJK froxel column surface range"),
                size: wgpu::Extent3d {
                    width: grid[0],
                    height: grid[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let tiles = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("SJK froxel slice tile means"),
                size: wgpu::Extent3d {
                    width: TILES,
                    height: TILES,
                    depth_or_array_layers: grid[2],
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D3,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        crate::log::progress(format_args!(
            "Froxel grid {grid:?}: {} bytes, no history",
            u64::from(grid[0]) * u64::from(grid[1]) * u64::from(grid[2]) * 16
        ));
        let parameters = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK medium parameters"),
            size: std::mem::size_of::<Parameters>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let fog_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SJK authored-fog transmission"),
            contents: bytemuck::cast_slice(&fog.entries),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let comparison = device.create_sampler(&wgpu::SamplerDescriptor {
            compare: Some(wgpu::CompareFunction::LessEqual),
            min_filter: wgpu::FilterMode::Linear,
            mag_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let linear = device.create_sampler(&wgpu::SamplerDescriptor {
            min_filter: wgpu::FilterMode::Linear,
            mag_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let source = concat!(
            include_str!("volumetric_coordinates.wgsl"),
            include_str!("volumetric_light.wgsl"),
        );

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK participating media"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let compute = |entry, layout| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout,
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        // Injection binds the main scene depth as group 1, so its group 0 must be explicit.
        let compute_entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty,
            count: None,
        };
        let uniform = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let inject_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK froxel injection"),
            entries: &[
                compute_entry(0, uniform),
                compute_entry(
                    1,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                compute_entry(
                    2,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                ),
                compute_entry(3, uniform),
                compute_entry(
                    5,
                    wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D3,
                    },
                ),
                compute_entry(
                    8,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                compute_entry(
                    11,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                compute_entry(
                    12,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                compute_entry(
                    13,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                compute_entry(
                    9,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
            ],
        });
        let inject_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[
                    Some(&inject_layout),
                    Some(&super::super::flares::depth_layout(device)),
                ],
                immediate_size: 0,
            });
        let inject = compute("inject", Some(&inject_pipeline_layout));
        let column_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK froxel columns"),
            entries: &[
                compute_entry(0, uniform),
                compute_entry(
                    10,
                    wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                ),
            ],
        });
        let column_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[
                    Some(&column_layout),
                    Some(&super::super::flares::depth_layout(device)),
                ],
                immediate_size: 0,
            });
        let columns = source.contains("fn columns(").then(|| {
            let pipeline = compute("columns", Some(&column_pipeline_layout));
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &column_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: parameters.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 10,
                        resource: wgpu::BindingResource::TextureView(&column_range),
                    },
                ],
            });
            (pipeline, group)
        });
        let reduce = compute("reduce", None);
        let integrate = compute("integrate", None);
        let binding = |binding, resource| wgpu::BindGroupEntry { binding, resource };
        let texture = wgpu::BindingResource::TextureView;
        let injection = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &inject_layout,
            entries: &[
                binding(0, parameters.as_entire_binding()),
                binding(1, texture(shadow)),
                binding(2, wgpu::BindingResource::Sampler(&comparison)),
                binding(3, fog_buffer.as_entire_binding()),
                binding(5, texture(&injected)),
                binding(8, texture(close)),
                binding(9, texture(&column_range)),
                binding(11, texture(far.unwrap_or(shadow))),
                binding(12, texture(world.map_or(shadow, |w| w[0]))),
                binding(13, texture(world.map_or(close, |w| w[1]))),
            ],
        });
        let reduction = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &reduce.get_bind_group_layout(0),
            entries: &[
                binding(0, parameters.as_entire_binding()),
                binding(4, texture(&injected)),
                binding(5, texture(&tiles)),
            ],
        });
        let integration = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &integrate.get_bind_group_layout(0),
            entries: &[
                binding(0, parameters.as_entire_binding()),
                binding(4, texture(&injected)),
                binding(5, texture(&integrated)),
                binding(6, wgpu::BindingResource::Sampler(&linear)),
                binding(7, texture(&tiles)),
            ],
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(
                    4,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                ),
                entry(
                    6,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                // Per-column surface depth from the `columns` pass, for the composite's
                // bilateral weights.
                entry(
                    9,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
            ],
        });
        let composition = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                binding(0, parameters.as_entire_binding()),
                binding(4, texture(&integrated)),
                binding(6, wgpu::BindingResource::Sampler(&linear)),
                binding(9, texture(&column_range)),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                Some(&layout),
                Some(&super::super::flares::depth_layout(device)),
            ],
            immediate_size: 0,
        });
        let composite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SJK medium composite"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("composite"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::SrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::RED
                        | wgpu::ColorWrites::GREEN
                        | wgpu::ColorWrites::BLUE,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let dust_beams =
            crate::dust_motes::beams::bind(device, &parameters, &injected, &tiles, &linear);
        let gain = 3.;
        let scattering = 0.00012;

        Self {
            ready: std::cell::Cell::new(false),
            parameters,
            inject,
            injection,
            columns,
            reduce,
            reduction,
            integrate,
            integration,
            composite,
            composition,
            dust_beams,
            grid,
            near_layers,
            fog_count: fog.count as u32,
            gain,
            scattering,
        }
    }

    /// Local scattering inputs, valid only after this frame's volume was prepared.
    pub(super) fn dust_beams(&self) -> Option<&wgpu::BindGroup> {
        self.ready.get().then_some(&self.dust_beams)
    }

    /// Upload only the camera/light uniforms; the renderer already evaluated the shadow fit.
    pub(super) fn update(
        &self,
        queue: &crate::frame_queue::FrameQueue,
        view: Mat4,
        shadow: Mat4,
        sun: sjk_shader::SunParms,
        distance: f32,
        clarity: f32,
        texel: f32,
        close: Option<(Mat4, f32, f32)>,
        far: Option<(Mat4, f32)>,
        bounds: [Vec3; 2],
        fog_mode: crate::fog_volumes::Mode,
    ) {
        self.ready.set(false);
        let inverse = view.inverse();
        if !inverse.is_finite() || inverse.z_axis.w.abs() < 1e-8 {
            return;
        }
        let eye = inverse.z_axis.truncate() / inverse.z_axis.w;
        let forward = (inverse.project_point3(Vec3::Z) - eye).normalize();
        if !eye.is_finite()
            || !forward.is_finite()
            || !sun.intensity.is_finite()
            || sun.color.iter().any(|c| !c.is_finite())
        {
            return;
        }
        let energy = sun.color.map(|c| c * sun.intensity / 60. * self.gain);
        // In-scattering only: zero incident energy produces exactly zero in every
        // froxel and an additive identity composite. Resume automatically with light.
        if energy.iter().all(|&c| c == 0.) {
            return;
        }
        let far = far.filter(|_| self.grid[2] > self.near_layers);
        let far_end = if far.is_some() {
            coverage::far_distance(inverse, eye, forward, bounds, distance)
        } else {
            distance
        };
        let parameters = Parameters {
            inverse: inverse.to_cols_array_2d(),
            shadow: shadow.to_cols_array_2d(),
            eye: eye
                .extend(match fog_mode {
                    crate::fog_volumes::Mode::Off => 0.,
                    crate::fog_volumes::Mode::Volume => 1.,
                    crate::fog_volumes::Mode::GlobalExp2 => 2.,
                })
                .to_array(),
            forward: forward.extend(0.).to_array(),
            sun: [sun.direction[0], sun.direction[1], sun.direction[2], 0.2],
            color: [energy[0], energy[1], energy[2], self.scattering],
            grid: [self.grid[0], self.grid[1], self.grid[2], self.fog_count],
            range: [2., distance, clarity.clamp(0., 1.), texel],
            close: close.map_or(Mat4::IDENTITY, |c| c.0).to_cols_array_2d(),
            close_range: close.map_or([0.; 4], |c| [c.1, c.2, 1., 0.]),
            far: far.map_or(Mat4::IDENTITY, |f| f.0).to_cols_array_2d(),
            far_range: [
                far.map_or(0., |f| f.1),
                f32::from(far.is_some()),
                far_end,
                self.near_layers as f32,
            ],
        };
        queue.write_buffer(&self.parameters, 0, bytemuck::bytes_of(&parameters));
        self.ready.set(true);
    }

    /// Inject, integrate and compose into the main scene, using its existing read-only depth.
    pub(super) fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::BindGroup,
    ) {
        if !self.ready.get() {
            return;
        }

        if let Some((pipeline, group)) = &self.columns {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("SJK froxel column ranges"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.set_bind_group(1, depth, &[]);
            pass.dispatch_workgroups(self.grid[0].div_ceil(8), self.grid[1].div_ceil(8), 1);
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("SJK froxel injection"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.inject);
            pass.set_bind_group(0, &self.injection, &[]);
            pass.set_bind_group(1, depth, &[]);
            pass.dispatch_workgroups(
                self.grid[0].div_ceil(4),
                self.grid[1].div_ceil(4),
                self.grid[2].div_ceil(4),
            );
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("SJK froxel slice means"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.reduce);
            pass.set_bind_group(0, &self.reduction, &[]);
            pass.dispatch_workgroups(TILES, TILES, self.grid[2]);
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("SJK medium prefix integration"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.integrate);
            pass.set_bind_group(0, &self.integration, &[]);
            pass.dispatch_workgroups(self.grid[0].div_ceil(8), self.grid[1].div_ceil(8), 1);
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SJK medium main-view composite"),
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
        pass.set_pipeline(&self.composite);
        pass.set_bind_group(0, &self.composition, &[]);
        pass.set_bind_group(1, depth, &[]);
        pass.draw(0..3, 0..1);
    }
}
