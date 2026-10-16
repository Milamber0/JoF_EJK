//! Quarter-resolution bloom, sharing the scene input and final color resolve.

/// Retained extraction and separable blur resources; absent when bloom is disabled.
pub(super) struct Bloom {
    /// Linear-light blurred brightness, sampled by the existing scene resolve.
    pub(super) output: wgpu::TextureView,
    extract: wgpu::RenderPipeline,
    horizontal: wgpu::RenderPipeline,
    vertical: wgpu::RenderPipeline,
    extract_bind: wgpu::BindGroup,
    horizontal_bind: wgpu::BindGroup,
    vertical_bind: wgpu::BindGroup,
    bright: wgpu::TextureView,
    scratch: wgpu::TextureView,
}

/// Round upward so odd-sized windows and single-pixel targets remain valid.
pub(super) fn extent(size: [u32; 2]) -> [u32; 2] {
    size.map(|n| n.div_ceil(4).max(1))
}

impl Bloom {
    /// Allocate on policy changes/resizes, never during steady frame submission.
    pub(super) fn new(device: &wgpu::Device, scene: &wgpu::TextureView, size: [u32; 2]) -> Self {
        let size = extent(size);
        let make = |label| {
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
                    format: wgpu::TextureFormat::Rgba16Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let bright = make("Bloom extracted brightness");
        let scratch = make("Bloom horizontal blur");
        let output = make("Bloom vertical blur");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Scene bloom"),
            source: wgpu::ShaderSource::Wgsl(include_str!("post_bloom.wgsl").into()),
        });
        let pipeline = |entry, horizontal| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[("HORIZONTAL", horizontal)],
                        ..Default::default()
                    },
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let extract = pipeline("extract", 0.0);
        let horizontal = pipeline("blur", 1.0);
        let vertical = pipeline("blur", 0.0);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Bloom linear clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind = |pipeline: &wgpu::RenderPipeline, view| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Bloom input"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            })
        };
        Self {
            extract_bind: bind(&extract, scene),
            horizontal_bind: bind(&horizontal, &bright),
            vertical_bind: bind(&vertical, &scratch),
            extract,
            horizontal,
            vertical,
            bright,
            scratch,
            output,
        }
    }

    /// Three bounded quarter-size passes; no CPU readback, allocation or synchronization.
    pub(super) fn draw(&self, encoder: &mut wgpu::CommandEncoder) {
        for (pipeline, bind, view) in [
            (&self.extract, &self.extract_bind, &self.bright),
            (&self.horizontal, &self.horizontal_bind, &self.scratch),
            (&self.vertical, &self.vertical_bind, &self.output),
        ] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Bloom quarter-resolution pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
