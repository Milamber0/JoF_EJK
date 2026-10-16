//! Prime opaque static depth before expensive material shading.
use super::*;
fn pipelines(forge: &Forge) -> [wgpu::RenderPipeline; 3] {
    let device = &forge.device;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("SJK opaque depth priming"),
        source: wgpu::ShaderSource::Wgsl(include_str!("depth_prime.wgsl").into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("depth priming"),
        bind_group_layouts: &[Some(&forge.camera_layout)],
        immediate_size: 0,
    });
    [Some(wgpu::Face::Front), Some(wgpu::Face::Back), None].map(|cull| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("depth priming"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[Some(crate::GpuVertex::layout())],
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                cull_mode: cull,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::DepthTarget::FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })
    })
}
impl Runtime {
    /// Compile the depth-priming pipelines at map installation, for a map whose frames
    /// will prime depth (real-time lighting), instead of in its first frame.
    pub(super) fn prewarm_depth_prime(&self) {
        self.depth_prime.get_or_init(|| pipelines(&self.forge));
    }

    /// Prime guaranteed opaque static depths for active real-time lighting.
    /// Return whether the caller should load the established depth instead of clearing it.
    pub(crate) fn prime_depth(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        depth: &wgpu::TextureView,
        input: &FrameDraw<'_>,
    ) -> bool {
        self.prime_depth_region(encoder, depth, input, None)
    }
    pub(crate) fn prime_depth_region(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        depth: &wgpu::TextureView,
        input: &FrameDraw<'_>,
        scissor: Option<[u32; 4]>,
    ) -> bool {
        if !self.realtime_materials_active() {
            return false;
        }
        let pipelines = self.depth_prime.get_or_init(|| pipelines(&self.forge));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("depth priming"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if let Some([x, y, w, h]) = scissor {
            pass.set_scissor_rect(x, y, w, h);
        }
        pass.set_bind_group(0, input.camera, &[]);
        pass.set_vertex_buffer(0, input.vertices.slice(..));
        pass.set_index_buffer(input.indices.slice(..), wgpu::IndexFormat::Uint32);
        let indirect = self.indirect.as_ref().and_then(|lists| {
            lists
                .runs(&self.forge.queue, |push| {
                    self.depth_ranges(input, |cull, range| push(cull, range, 0));
                })
                .map(|runs| (lists, runs))
        });
        if let Some((lists, runs)) = indirect {
            for run in runs.iter() {
                pass.set_pipeline(&pipelines[run.pipeline]);
                pass.multi_draw_indexed_indirect(&lists.buffer, run.offset, run.count);
            }
        } else {
            self.depth_ranges(input, |cull, range| {
                pass.set_pipeline(&pipelines[cull]);
                pass.draw_indexed(range, 0, 0..1);
            });
        }
        true
    }

    fn depth_ranges(&self, input: &FrameDraw<'_>, mut draw: impl FnMut(usize, Range<u32>)) {
        let active = self.active_materials(input.source_cluster, input.visibility);
        for &index in &active.list {
            let material = &self.materials[index];
            let Some(cull) = eligible_cull(material, &self.forge) else {
                continue;
            };
            for range in
                self.visible_static_ranges(material, input.source_cluster, input.visibility)
            {
                draw(cull, range);
            }
        }
    }
}

// Only the first, replacing stage may establish coverage. Deforms, sprites, depth
// bias, alpha tests and debug-discardable stages must retain the ordinary draw path.
fn eligible_cull(material: &Material, forge: &Forge) -> Option<usize> {
    if material.blended || material.flare {
        return None;
    }
    let stage = material.stages.first()?;
    let source = stage.table.as_ref()?;
    let key = forge.pipeline_keys[stage.pipeline];
    let changes_geometry_or_depth = key.geometry & (1 | 2 | 8) != 0;
    let alpha_test = source.gpu.generators[2] != 0.0;
    let debug_can_discard = (source.gpu.wave_functions[3] as u32) & 2 != 0;
    if !key.depth_write
        || key.depth != wgpu::CompareFunction::LessEqual
        || key.source != wgpu::BlendFactor::One
        || key.destination != wgpu::BlendFactor::Zero
        || changes_geometry_or_depth
        || alpha_test
        || debug_can_discard
    {
        return None;
    }
    Some(match key.cull {
        Some(wgpu::Face::Front) => 0,
        Some(wgpu::Face::Back) => 1,
        None => 2,
    })
}
