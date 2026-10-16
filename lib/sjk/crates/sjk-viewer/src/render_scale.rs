//! Optional single-sample supersampling. Target ownership stays in the viewer.
use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry};

/// Register startup policy; changing attachment dimensions takes a graphics reload.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), CvarError> {
    cvars.register(CvarDefinition::new(
        "r_superSample",
        1_i64,
        CvarFlags::ARCHIVE,
        "Scene supersampling: 1 off, 2 or 3 per axis; applies after a graphics reload (vid_restart)",
    ))?;
    cvars.on_change("r_superSample", |_| {
        crate::log::progress(format_args!(
            "r_superSample changed: {}",
            crate::graphics_reload::APPLY
        ));
        crate::graphics_reload::notice();
    })
}

/// Sample the actual loaded config, not a parallel default-only settings object.
pub(crate) fn requested(console: Option<&crate::console::ViewerConsole>) -> u32 {
    // `SJK_RENDER_SCALE` overrides the archived cvar for one run (a launcher's choice).
    std::env::var("SJK_RENDER_SCALE")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .or_else(|| console.and_then(|c| c.integer_cvar("r_superSample")))
        .unwrap_or(1)
        .clamp(1, 3) as u32
}

/// Account for four scene colour/depth pairs; HDR RGBA16F uses 48 rather than 32 bytes.
pub(crate) fn supported_bytes(request: u32, output: [u32; 2], maximum: u32, bytes: u64) -> u32 {
    (2..=request.clamp(1, 3))
        .rev()
        .find(|&scale| {
            let [w, h] = output.map(|n| u64::from(n) * u64::from(scale));
            w > 0 && h > 0 && w <= u64::from(maximum) && h <= u64::from(maximum)
            // Reserve four colour/depth pairs, including secondary/preview headroom.
            && w.saturating_mul(h).saturating_mul(bytes) <= 512 * 1024 * 1024
        })
        .unwrap_or(1)
}

/// Allocate transactionally at installation/resize, never during steady frame drawing.
pub(crate) fn checked<T>(device: &wgpu::Device, build: impl FnOnce() -> T) -> Option<T> {
    let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let candidate = build();
    let invalid = pollster::block_on(validation.pop());
    let exhausted = pollster::block_on(memory.pop());
    if let Some(error) = invalid.or(exhausted) {
        crate::log::progress(format_args!(
            "supersampling allocation failed: {error}; reducing scale"
        ));
        None
    } else {
        Some(candidate)
    }
}

/// Retained scene colour and integer box resolve; HUD depth remains output-sized.
pub(crate) struct Runtime {
    /// Scene colour at the same dimensions as main depth and secondary views.
    pub(crate) scene: wgpu::TextureView,
    /// Native attachment for pipelines that depth-test neither HUD nor text.
    pub(crate) hud_depth: crate::DepthTarget,
    /// Effective scene size, after dimension and memory clamps.
    pub(crate) size: [u32; 2],
    bind: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
}

impl Runtime {
    /// Create inside an allocation error scope, before publishing any resized attachment.
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        output: [u32; 2],
        scale: u32,
    ) -> Self {
        let size = output.map(|n| n * scale);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SJK supersampled scene"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let scene = texture.create_view(&Default::default());
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK integer box resolve"),
            source: wgpu::ShaderSource::Wgsl(include_str!("render_scale.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SJK supersample resolve before scene postprocessing"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &[("SCALE", f64::from(scale))],
                    ..Default::default()
                },
                targets: &[Some(wgpu::ColorTargetState {
                    format,
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
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&scene),
            }],
        });
        Self {
            scene,
            size,
            bind,
            pipeline,
            hud_depth: crate::DepthTarget::new(device, output[0], output[1]),
        }
    }

    /// Average every covered linear-light texel, without filtering the later HUD.
    pub(crate) fn draw(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SJK supersample box resolve"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output,
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
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

impl crate::GpuState {
    /// Window resize retains native output dimensions, then rebuilds scene attachments.
    pub(crate) fn resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.size = size;
        self.configuration.width = size.width;
        self.configuration.height = size.height;
        // The frame in flight presents before its swapchain is reconfigured.
        self.frame_pacer.split.wait_previous();
        if let Some(surface) = &self.context.surface {
            surface.configure(&self.device, &self.configuration);
        }
        self.resize_scene_depth(size);
    }

    /// Output aspect is unchanged; only raster dimensions increase.
    pub(crate) fn scene_size(&self) -> [u32; 2] {
        self.render_scale
            .as_ref()
            .map_or([self.size.width, self.size.height], |s| s.size)
    }

    /// The offscreen texture the scene is drawn into, when the frame has one; without it
    /// the scene is drawn straight into the swapchain image.
    pub(crate) fn offscreen_scene_target(&self) -> Option<&wgpu::TextureView> {
        self.render_scale
            .as_ref()
            .map(|scale| &scale.scene)
            .or_else(|| self.post_aa.as_ref().map(|aa| &aa.scene))
    }

    /// Publish scaled attachments only after the entire candidate succeeded.
    pub(crate) fn with_render_scale(mut self) -> Self {
        self.install_render_scale();
        self
    }
}
