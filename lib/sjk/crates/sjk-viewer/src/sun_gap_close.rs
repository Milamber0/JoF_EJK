//! Closing of slits in the sun shadow maps (`sun_gap_close.wgsl`): four separable
//! passes (minimum across, minimum down, maximum across, maximum down) through a scratch
//! depth texture, after a cascade is rendered. The radius in texels comes from the live
//! `r_sunShadowGapClose` width in world units and the cascade's texel size.

pub(super) struct Runtime {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    scratch: wgpu::TextureView,
    /// One uniform per pass: minimum across, minimum down, maximum across, maximum down.
    passes: [wgpu::Buffer; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Pass {
    direction: [i32; 2],
    radius: i32,
    minimum: u32,
}

impl Runtime {
    pub(super) fn new(device: &wgpu::Device, resolution: u32) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK shadow gap close"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK shadow gap close"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK shadow gap close"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sun_gap_close.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SJK shadow gap close"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("fullscreen"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("close"),
                compilation_options: Default::default(),
                targets: &[],
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::DepthTarget::FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let scratch = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("SJK shadow gap close scratch"),
                size: wgpu::Extent3d {
                    width: resolution,
                    height: resolution,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: crate::DepthTarget::FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let passes = std::array::from_fn(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("SJK shadow gap close pass"),
                size: std::mem::size_of::<Pass>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        Self {
            pipeline,
            layout,
            scratch,
            passes,
        }
    }

    /// Close holes narrower than `width` world units in `depth` (a map of `texel` units
    /// per texel), in place through the scratch texture. No-op for a width under a texel.
    pub(super) fn apply(
        &self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        encoder: &mut wgpu::CommandEncoder,
        depth: &wgpu::TextureView,
        texel: f32,
        width: f32,
    ) {
        let radius = (width / (2. * texel.max(1e-3))).ceil().min(8.) as i32;
        if radius < 1 {
            return;
        }
        let plan = [([1, 0], 1u32), ([0, 1], 1), ([1, 0], 0), ([0, 1], 0)];
        for (index, (direction, minimum)) in plan.into_iter().enumerate() {
            queue.write_buffer(
                &self.passes[index],
                0,
                bytemuck::bytes_of(&Pass {
                    direction,
                    radius,
                    minimum,
                }),
            );
            let (source, target) = if index % 2 == 0 {
                (depth, &self.scratch)
            } else {
                (&self.scratch, depth)
            };
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("SJK shadow gap close"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(source),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.passes[index].as_entire_binding(),
                    },
                ],
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SJK shadow gap close"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: target,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
