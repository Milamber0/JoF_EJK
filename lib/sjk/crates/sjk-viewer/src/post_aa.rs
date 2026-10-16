//! Shared scene resolve and optional final display ramp. Neutral controls bypass their work.
use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry};
use wgpu::util::DeviceExt;

#[path = "post_color.rs"]
/// Cached scene-grade and display-ramp policy.
pub(crate) mod color;

#[path = "post_bloom.rs"]
mod bloom;

#[path = "effect_layer.rs"]
/// rd-vanilla's blended-effect framebuffer, merged by this resolve.
pub(crate) mod effects;

#[path = "post_hdr.rs"]
/// Scene precision and a colour-ratio-preserving display shoulder.
pub(crate) mod hdr;

#[path = "post_exposure.rs"]
/// Eye adaptation: the scene exposure, metered and smoothed on the GPU.
pub(crate) mod exposure;
#[path = "post_glow.rs"]
/// Dynamic glow (`r_DynamicGlow`): the glowing stages' image, its blur and settings.
pub(crate) mod glow;

/// Register an honestly named post-process control, on by default.
///
/// Measured at 0.026 ms per frame at 2560x1080 on an RX 9060 XT (interleaved A/B on a fixed
/// scene: 0.529/0.531 off against 0.558/0.554 on), so the visual gain is worth the cost by
/// default. `r_fxaa 0` restores the untouched base image exactly.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), CvarError> {
    hdr::register(cvars)?;
    cvars.register(CvarDefinition::new(
        "r_fxaa",
        1_i64,
        CvarFlags::ARCHIVE,
        "Scene FXAA (not MSAA); applies after a graphics reload (vid_restart)",
    ))?;
    cvars.on_change("r_fxaa", |_| {
        crate::log::progress(format_args!(
            "r_fxaa changed: {}",
            crate::graphics_reload::APPLY
        ));
        crate::graphics_reload::notice();
    })
}

/// Resolve startup policy; retained by the process context across console-free map installs.
pub(crate) fn enabled(console: Option<&crate::console::ViewerConsole>) -> bool {
    console.and_then(|c| c.integer_cvar("r_fxaa")).unwrap_or(1) != 0
}

/// A single-sample scene intermediate and an edge-directed resolve into the final output.
pub(crate) struct Runtime {
    /// Full scene destination, before the final AA resolve and HUD.
    pub(crate) scene: wgpu::TextureView,
    bind: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    parameters: wgpu::Buffer,
    fxaa: bool,
    hdr: hdr::Settings,
    /// Applied controls, allowing steady frames to skip updates.
    pub(super) policy: color::Policy,
    display: Option<Box<Runtime>>,
    bloom: Option<bloom::Bloom>,
    /// rd-vanilla's blended-effect framebuffer, merged by this resolve; production only.
    effects: Option<effects::Layer>,
    /// Dynamic glow image and blur, merged by this resolve; production only, when enabled.
    glow: Option<glow::Glow>,
    /// Eye adaptation of a scene resolve; none for a display-only pass.
    exposure: Option<exposure::Exposure>,
    /// Retained inputs to rebuild [`Self::bind`] when the effect layer is resized.
    inputs: Inputs,
}

/// Views and sampler bound by the resolve, kept to rebind a resized effect layer.
struct Inputs {
    sample: wgpu::TextureView,
    sampler: wgpu::Sampler,

    effect_encoding: effects::Encoding,
    /// The exposure state bound at binding 4 here and in the effect layer.
    exposure_state: wgpu::Buffer,

    /// The resolve pipeline was built to merge an effect layer.
    merge: bool,
    /// Window size, which sizes the glow blur.
    frame: [u32; 2],
}

impl Runtime {
    /// Construct from process policy, including a console-free world installation.
    pub(crate) fn for_context(
        context: &crate::gpu_context::Context,
        size: [u32; 2],
    ) -> Option<Self> {
        Self::configured(
            &context.device,
            context.format,
            context.ui_direct,
            size,
            context.fxaa,
            context.post_color.policy(),
            context.hdr,
            Some(size),
        )
    }

    /// `ui_direct` is [`crate::gpu_context::Context::ui_direct`]: without it the
    /// 2D layer always goes through an intermediate with a UNORM alias.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn configured(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        ui_direct: bool,
        size: [u32; 2],
        fxaa: bool,
        policy: color::Policy,
        hdr: hdr::Settings,
        effects: Option<[u32; 2]>,
    ) -> Option<Self> {
        if effects.is_none()
            && hdr.mode == 0
            && !fxaa
            && !policy.tonemap
            && !policy.bloom
            && policy.gamma == 1.0
            && ui_direct
        {
            return None;
        }
        let mut runtime = Self::pass_hdr(device, format, size, fxaa, policy, hdr, effects);
        if runtime.scene_effects() && needs_display_pass(policy.gamma, ui_direct) {
            runtime.display = Some(Box::new(Self::pass(
                device,
                format,
                size,
                false,
                color::Policy {
                    gamma: policy.gamma,
                    ..Default::default()
                },
            )));
        }
        Some(runtime)
    }

    fn pass(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size: [u32; 2],
        fxaa: bool,
        policy: color::Policy,
    ) -> Self {
        Self::pass_hdr(
            device,
            format,
            size,
            fxaa,
            policy,
            hdr::Settings::default(),
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn pass_hdr(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size: [u32; 2],
        fxaa: bool,
        policy: color::Policy,
        hdr: hdr::Settings,
        effects: Option<[u32; 2]>,
    ) -> Self {
        let scene_format = hdr.format(format);
        // As `scene_effects`: this pass resolves the scene before the HUD.
        let resolves_scene =
            hdr.mode != 0 || fxaa || policy.tonemap || policy.bloom || effects.is_some();
        let sample_format = scene_format.remove_srgb_suffix();
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SJK FXAA scene"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: scene_format,
            usage,
            view_formats: &[sample_format],
        });
        let scene = texture.create_view(&Default::default());
        // Only the production resolve, which merges the effect layer, draws glow.
        let glow = effects
            .filter(|_| policy.glow.enabled)
            .map(|scene| glow::Glow::new(device, scene, size, scene_format, policy.glow));
        let bloom = policy
            .bloom
            .then(|| bloom::Bloom::new(device, &scene, size));
        let sample = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(sample_format),
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK FXAA"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(include_str!("post_aa.wgsl"), include_str!("post_hdr.wgsl")).into(),
            ),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SJK scene FXAA before HUD"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &[
                        ("SRGB_OUTPUT", if format.is_srgb() { 1.0 } else { 0.0 }),
                        ("HDR_INPUT", if hdr.mode != 0 { 1.0 } else { 0.0 }),
                        ("SCENE_EXPOSURE", f64::from(u8::from(resolves_scene))),
                        ("EFFECTS", f64::from(u8::from(effects.is_some()))),
                        ("GLOW", f64::from(u8::from(glow.is_some()))),
                    ],
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SJK FXAA clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let mut values = parameters(fxaa, policy);
        if hdr.mode != 0 {
            values[2] = 1.0;
        }
        let parameters = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SJK color controls"),
            contents: bytemuck::cast_slice(&values),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let exposure = resolves_scene.then(|| {
            exposure::Exposure::new(device, &sample, &sampler, size, hdr.mode != 0, hdr.exposure)
        });
        let exposure_state = exposure
            .as_ref()
            .map_or_else(|| exposure::neutral(device), |e| e.state().clone());
        let inputs = Inputs {
            sample,
            sampler,

            effect_encoding: effects::Encoding {
                scene: scene_format,
            },
            exposure_state,

            merge: effects.is_some(),
            frame: size,
        };
        let layer = effects.map(|scene| {
            effects::Layer::new(
                device,
                scene,
                inputs.effect_encoding,
                &inputs.exposure_state,
            )
        });
        let bind = Self::bind(
            device,
            &pipeline,
            &parameters,
            bloom.as_ref(),
            &inputs,
            layer.as_ref(),
            glow.as_ref(),
        );
        Self {
            scene,
            bind,
            pipeline,
            parameters,
            fxaa,
            hdr,
            policy,
            display: None,
            bloom,
            effects: layer,
            glow,
            exposure,
            inputs,
        }
    }

    fn bind(
        device: &wgpu::Device,
        pipeline: &wgpu::RenderPipeline,
        parameters: &wgpu::Buffer,
        bloom: Option<&bloom::Bloom>,
        inputs: &Inputs,
        effects: Option<&effects::Layer>,
        glow: Option<&glow::Glow>,
    ) -> wgpu::BindGroup {
        let view = |binding, view| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(view),
        };
        // Without a layer the merge is compiled out; the scene stands in for its images.
        let (blended, original) = effects.map_or((&inputs.sample, &inputs.sample), |layer| {
            (layer.blended(), layer.original())
        });
        let entries = [
            view(0, &inputs.sample),
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&inputs.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: parameters.as_entire_binding(),
            },
            view(3, bloom.map_or(&inputs.sample, |b| &b.output)),
            wgpu::BindGroupEntry {
                binding: 4,
                resource: inputs.exposure_state.as_entire_binding(),
            },
            view(5, blended),
            view(6, original),
            wgpu::BindGroupEntry {
                binding: 7,
                resource: effects
                    .map_or(parameters, |layer| layer.region())
                    .as_entire_binding(),
            },
            // Without glow the composite is compiled out; the scene and the colour
            // controls stand in for its image and controls.
            view(8, glow.map_or(&inputs.sample, |glow| glow.output())),
            wgpu::BindGroupEntry {
                binding: 9,
                resource: glow
                    .map_or(parameters, |glow| glow.controls())
                    .as_entire_binding(),
            },
        ];
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK FXAA scene sample"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        })
    }

    /// (Re)size the legacy effect layer to the scene targets; startup and resize only.
    /// Only a resolve built with an effect layer (`effects` in [`Self::configured`])
    /// merges one.
    pub(crate) fn fit_effects(&mut self, device: &wgpu::Device, scene: [u32; 2]) {
        if !self.inputs.merge
            || self
                .effects
                .as_ref()
                .is_some_and(|layer| layer.size() == scene)
        {
            return;
        }
        let layer = effects::Layer::new(
            device,
            scene,
            self.inputs.effect_encoding,
            &self.inputs.exposure_state,
        );
        // The glow image shares the scene's depth, so it follows the scene's size too.
        if self.glow.is_some() {
            self.glow = Some(glow::Glow::new(
                device,
                scene,
                self.inputs.frame,
                self.inputs.effect_encoding.scene,
                self.policy.glow,
            ));
        }
        self.bind = Self::bind(
            device,
            &self.pipeline,
            &self.parameters,
            self.bloom.as_ref(),
            &self.inputs,
            Some(&layer),
            self.glow.as_ref(),
        );
        self.effects = Some(layer);
    }

    /// The legacy effect layer shared by every view this frame, when the resolve merges one.
    pub(crate) const fn effect_layer(&self) -> Option<&effects::Layer> {
        self.effects.as_ref()
    }

    /// The dynamic glow image the main view draws into, when glow is enabled.
    pub(crate) const fn glow(&self) -> Option<&glow::Glow> {
        self.glow.as_ref()
    }

    /// View the 2D layer draws into, always UNORM ([`crate::ui_target`]): the
    /// display intermediate's alias, which the display pass ramps without
    /// filtering any glyphs; this pass's own scene alias when it only applies the
    /// ramp; else `output_ui`, the swapchain image's 2D view.
    pub(crate) fn hud_target<'a>(
        &'a self,
        output_ui: &'a wgpu::TextureView,
    ) -> &'a wgpu::TextureView {
        if let Some(display) = &self.display {
            &display.inputs.sample
        } else if !self.scene_effects() {
            &self.inputs.sample
        } else {
            output_ui
        }
    }

    /// Resolve only scene effects here; display gamma is deliberately later.
    /// The resolve writes linear colour through the sRGB view of the texture
    /// the 2D layer then draws into.
    pub(crate) fn draw_scene(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
    ) {
        if self.scene_effects() {
            self.draw(
                encoder,
                self.display
                    .as_ref()
                    .map_or(output, |display| &display.scene),
            );
            // After every reader of this frame's exposure; the next frame shows the result.
            if let Some(exposure) = &self.exposure {
                exposure.record(encoder);
            }
        }
    }

    /// Apply stock display gamma after all overlays, with no resampling.
    pub(crate) fn draw_display(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
    ) {
        if let Some(display) = &self.display {
            display.draw(encoder, output);
        } else if !self.scene_effects() {
            self.draw(encoder, output);
        }
    }

    fn scene_effects(&self) -> bool {
        self.hdr.mode != 0
            || self.fxaa
            || self.policy.tonemap
            || self.policy.bloom
            || self.effects.is_some()
    }

    /// Slider changes upload 16 bytes, without rebuilding pipelines or textures.
    pub(super) fn set_gamma(&mut self, queue: &crate::frame_queue::FrameQueue, gamma: f32) {
        self.policy.gamma = gamma;
        if let Some(display) = &mut self.display {
            display.set_gamma(queue, gamma);
        } else {
            let mut values = parameters(self.fxaa, self.policy);
            if self.hdr.mode != 0 {
                values[2] = 1.0;
            }
            queue.write_buffer(&self.parameters, 0, bytemuck::cast_slice(&values));
        }
    }

    /// Resolve scene color after flares/portals, without sampling or changing depth.
    pub(crate) fn draw(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        if let Some(bloom) = &self.bloom {
            bloom.draw(encoder);
        }
        if let Some(glow) = &self.glow {
            glow.blur(encoder);
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SJK FXAA resolve"),
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

/// A scene resolve is followed by a separate display pass when the ramp is not
/// identity, or when the 2D layer cannot draw into the swapchain image directly.
fn needs_display_pass(gamma: f32, ui_direct: bool) -> bool {
    gamma != 1.0 || !ui_direct
}

fn parameters(fxaa: bool, policy: color::Policy) -> [f32; 4] {
    [
        f32::from(u8::from(fxaa)),
        f32::from(u8::from(policy.tonemap)),
        if fxaa || policy.tonemap || policy.bloom {
            1.0
        } else {
            policy.gamma
        },
        if policy.bloom { 0.35 } else { 0.0 },
    ]
}

#[cfg(test)]
mod tests {
    use super::needs_display_pass;

    #[test]
    fn display_pass_follows_gamma_and_surface_aliasing() {
        // Default path: the 2D layer draws into the swapchain image's UNORM view.
        assert!(!needs_display_pass(1.0, true));
        // r_gamma ramps world and 2D layer together after the overlays.
        assert!(needs_display_pass(1.2, true));
        // No UNORM view of the swapchain: the 2D layer uses the intermediate's alias.
        assert!(needs_display_pass(1.0, false));
        assert!(needs_display_pass(0.8, false));
    }
}
