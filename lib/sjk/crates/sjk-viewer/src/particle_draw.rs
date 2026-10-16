//! Particle pipelines and depth-read-only submission, shared by main and mirror views.
use crate::*;

#[path = "soft_particle_settings.rs"]
pub(crate) mod settings;

/// Legacy and softened pipelines share the same vertex/atlas/blend inputs.
pub(crate) struct Pipelines {
    plain: [wgpu::RenderPipeline; effect_blend::PIPELINE_COUNT],
    soft: [wgpu::RenderPipeline; effect_blend::PIPELINE_COUNT],
}

/// Build pipelines once per world; no per-frame pipeline or binding creation. `format` is
/// the scene's, for the opaque placeholder entity; blended effects target the legacy
/// effect layer ([`crate::frame_target::aa::effects::FORMAT`]).
pub(crate) fn create(
    device: &wgpu::Device,
    camera: &wgpu::BindGroupLayout,
    atlas: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> (wgpu::RenderPipeline, Pipelines) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("SJK particle shader"),
        source: wgpu::ShaderSource::Wgsl(
            concat!(
                include_str!("entity.wgsl"),
                include_str!("soft_particles.wgsl")
            )
            .into(),
        ),
    });
    let depth = crate::world_materials::flares::depth_layout(device);

    let plain = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(camera), Some(atlas)],
        immediate_size: 0,
    });
    let soft = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(camera), Some(atlas), Some(&depth)],
        immediate_size: 0,
    });
    let create = |blend, write, slot: Option<usize>| {
        let constants = [("soft_blend", slot.unwrap_or(0) as f64)];

        let selected = &shader;
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SJK particle pipeline"),
            layout: Some(if slot.is_some() { &soft } else { &plain }),
            vertex: wgpu::VertexState {
                module: selected,
                entry_point: Some("vertex_main"),
                compilation_options: Default::default(),
                buffers: &[Some(EntityInstance::layout())],
            },
            fragment: Some(wgpu::FragmentState {
                module: selected,
                entry_point: Some(if slot.is_some() {
                    "fragment_soft"
                } else {
                    "fragment_main"
                }),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: if slot.is_some() { &constants } else { &[] },
                    ..Default::default()
                },
                // Placeholder entities draw in the scene pass; blended effects in the layer.
                targets: &[Some(wgpu::ColorTargetState {
                    format: if write {
                        format
                    } else {
                        crate::frame_target::aa::effects::FORMAT
                    },
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: if write { Some(wgpu::Face::Back) } else { None },
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DepthTarget::FORMAT,
                depth_write_enabled: Some(write),
                depth_compare: Some(if write {
                    wgpu::CompareFunction::Less
                } else {
                    wgpu::CompareFunction::LessEqual
                }),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })
    };
    let entity = create(wgpu::BlendState::ALPHA_BLENDING, true, None);
    (
        entity,
        Pipelines {
            plain: effect_blend::specifications().map(|(_, blend)| create(blend, false, None)),
            soft: std::array::from_fn(|i| {
                create(effect_blend::specifications()[i].1, false, Some(i))
            }),
        },
    )
}

impl Pipelines {
    /// Draw each blend slot's billboard range, in slot order, with its plain or softened
    /// pipeline. Instance and bind-group state are the caller's.
    pub(crate) fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        ranges: impl Iterator<Item = std::ops::Range<u32>>,
        soft: bool,
    ) {
        let pipelines = if soft { &self.soft } else { &self.plain };
        for (pipeline, range) in pipelines.iter().zip(ranges) {
            if !range.is_empty() {
                pass.set_pipeline(pipeline);
                pass.draw(0..36, range);
            }
        }
    }
}

impl GpuState {
    /// Read-only depth is needed only when enabled and at least one billboard is submitted.
    pub(crate) fn soften_particles(&self, ranges: &effect_submission::Ranges) -> bool {
        self.context.soft_particles.enabled() && ranges.blended().any(|range| !range.is_empty())
    }

    /// Preserve billboard, geometry and saber ordering in either pass; the decals are
    /// drawn before, under the weather.
    fn draw_particle_tail<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera: &'a wgpu::BindGroup,
        ranges: &effect_submission::Ranges,
        depth: Option<&'a wgpu::BindGroup>,
    ) {
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(1, &self.particle_atlas.bind_group, &[]);
        if let Some(depth) = depth {
            pass.set_bind_group(2, depth, &[]);
        }
        pass.set_vertex_buffer(0, self.entity_instance_buffer.slice(..));
        self.particle_pipelines
            .draw(pass, ranges.blended(), depth.is_some());
        self.effect_geometry
            .draw(pass, camera, &self.particle_atlas.bind_group);
        self.saber_gpu.draw(pass, camera);
    }

    /// Composite the frame's blended effects over `color` in rd-vanilla's 8-bit, per-channel
    /// clamped display space (`effect_layer.rs`), within `region` when given. The main view
    /// leaves them in the layer for the final resolve; a secondary view takes them back.
    /// Depth is finished by then and is read, never written, by the effect pass.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn composite_effects(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        depth: &DepthTarget,
        camera: &wgpu::BindGroup,
        ranges: &effect_submission::Ranges,
        resolve: EffectResolve,
        region: Option<[u32; 4]>,
    ) {
        let Some(layer) = self.post_aa.as_ref().and_then(|aa| aa.effect_layer()) else {
            return;
        };
        let size = color.texture().size();
        let size = [size.width, size.height];
        let merge = resolve == EffectResolve::Merge;
        // Weather belongs to the main view only and reaches the whole screen.
        let weather = merge && self.weather.visible();
        let present = ranges.blended().any(|range| !range.is_empty())
            || self.effect_geometry.stats().indices != 0
            || self.saber_gpu.has_draws()
            || weather;
        // The main view encodes and merges only the rectangle its effects can reach.
        let region = if weather {
            None
        } else if merge && present {
            self.effect_region(layer.view(), ranges, size)
        } else {
            region
        };
        let empty = region.is_some_and(|[_, _, w, h]| w == 0 || h == 0);
        if !present || empty || layer.size() != size {
            // An empty merge rectangle: the resolve never reads the layer this frame.
            if merge {
                layer.set_merge_region(&self.queue, Some([0; 4]));
            }
            return;
        }
        if merge {
            layer.set_merge_region(&self.queue, region);
        }
        let phases = self.gpu_phases.as_ref().filter(|_| merge);
        if let Some(phases) = phases {
            phases.mark(encoder, "scene-world");
        }
        layer.encode(&self.device, encoder, color, region);
        if let Some(phases) = phases {
            phases.mark(encoder, "effect-encode");
        }
        let mut pass = layer.begin_effects(encoder, &depth.view);
        if let Some([x, y, w, h]) = region {
            pass.set_scissor_rect(x, y, w, h);
        }
        // First of all: a blade skin's heat haze replaces the pixels it bends with the
        // scene's, which the layer still holds there only before anything else draws.
        self.saber_gpu
            .draw_haze(&self.device, &mut pass, camera, layer.original());
        // Impact marks lie on the world, so the haze covers them as it covers the wall.
        self.effect_geometry
            .draw_decals(&mut pass, camera, &self.particle_atlas.bind_group);
        // Weather before the other effects, though `RB_RenderWorldEffects` runs after
        // every surface. Effects write no depth, so weather drawn after them lay over every
        // blade and puff in front of it: the haze, measured to the wall behind, greyed
        // sabers and smoke as if they stood at the wall, and rain streaked across them
        // (Sol, 11/10/2026). Drawn first, it is covered by the effects even where a drop
        // is nearer: rain is faint, a blade is not.
        if weather {
            self.weather
                .draw(&mut pass, camera, &depth.sample_bind_group);
        }
        let soft = self
            .soften_particles(ranges)
            .then_some(&depth.sample_bind_group);
        self.draw_particle_tail(&mut pass, camera, ranges, soft);
        drop(pass);
        if resolve == EffectResolve::WriteBack {
            layer.write_back(encoder, color, region);
        }
    }
}

/// Where a view's composited effects end up.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EffectResolve {
    /// The main view: the final resolve adds them after the world's own post-processing.
    Merge,
    /// A portal, sky or mirror view: written back into its own target.
    WriteBack,
}
