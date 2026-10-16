//! The player outlines' mask ([`crate::card_outline`]): each outlined body (the
//! inspected player's, the team outlines') drawn again with the scene's vertex path,
//! bind groups and instance, into a one-channel image where it is in view, as its
//! group's value (the blend constant, so one program serves every group). Like the
//! glow pass it reads the scene's finished depth and never writes it; a slight bias
//! towards the eye keeps the body from losing against its own depth.
use super::*;

/// Pull the mask a little towards the eye: the body's own depth never rejects it, and
/// nothing thinner than the bias in front of the body is missed.
const BIAS: wgpu::DepthBiasState = wgpu::DepthBiasState {
    constant: -16,
    slope_scale: -1.0,
    clamp: 0.0,
};

/// The blend constant in place of the fragment's one: the group's mask value.
const GROUP_VALUE: wgpu::BlendComponent = wgpu::BlendComponent {
    src_factor: wgpu::BlendFactor::Constant,
    dst_factor: wgpu::BlendFactor::Zero,
    operation: wgpu::BlendOperation::Add,
};

impl Runtime {
    /// The mask pipeline for the mask material's stage key `index`, compiled on first use.
    fn card_outline_pipeline(&self, index: usize) -> &wgpu::RenderPipeline {
        self.card_outline_mask.get_or_init(|| {
            let key = self.forge.pipeline_keys[index];
            let (layout, shader) = self.forge.program_for(key);
            self.forge
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("SJK card outline mask"),
                    layout: Some(layout),
                    vertex: wgpu::VertexState {
                        module: shader,
                        entry_point: Some("entity_vertex_main"),
                        compilation_options: Default::default(),
                        buffers: &[
                            Some(crate::GpuVertex::layout()),
                            Some(crate::ActorInstance::layout()),
                        ],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: shader,
                        entry_point: Some("card_outline_mask_fragment"),
                        compilation_options: Default::default(),
                        // The fragment writes one; the blend gives the group's value.
                        targets: &[Some(wgpu::ColorTargetState {
                            format: crate::card_outline::MASK_FORMAT,
                            blend: Some(wgpu::BlendState {
                                color: GROUP_VALUE,
                                alpha: GROUP_VALUE,
                            }),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    // Every face: the mask is the shape the body covers, two-sided
                    // surfaces (hair, cloth) included.
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: crate::DepthTarget::FORMAT,
                        depth_write_enabled: Some(false),
                        depth_compare: Some(wgpu::CompareFunction::LessEqual),
                        stencil: Default::default(),
                        bias: BIAS,
                    }),
                    multisample: Default::default(),
                    multiview_mask: None,
                    cache: None,
                })
        })
    }

    /// Draw the outlines' `draws` (a group's mask material over a body's surfaces) into
    /// a pass whose colour target is the mask and whose depth is the scene's,
    /// read-only; `groups` gives each mask material's value.
    pub(crate) fn draw_card_outline_mask<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        frame: &FrameDraw<'pass>,
        draws: &'pass [crate::entity_materials::Draw],
        groups: &[(usize, f64)],
    ) {
        let mut value = None;
        pass.set_bind_group(0, frame.camera, &[]);
        pass.set_bind_group(2, &self.forge.geometry, &[]);
        if let Some(sun) = &self.forge.model_sun {
            pass.set_bind_group(3, sun.binding(true), &[]);
        }
        pass.set_vertex_buffer(0, frame.vertices.slice(..));
        pass.set_vertex_buffer(1, frame.instances.slice(..));
        pass.set_index_buffer(frame.indices.slice(..), wgpu::IndexFormat::Uint32);
        for draw in draws {
            let Some(material) = self
                .source_to_runtime
                .get(draw.material)
                .copied()
                .filter(|index| *index != usize::MAX)
                .and_then(|index| self.materials.get(index))
            else {
                continue;
            };
            let Some(stage) = material.stages.first() else {
                continue;
            };
            let Some(&(_, group)) = groups.iter().find(|(source, _)| *source == draw.material)
            else {
                continue;
            };
            if value != Some(group) {
                value = Some(group);
                pass.set_blend_constant(wgpu::Color {
                    r: group,
                    g: group,
                    b: group,
                    a: group,
                });
            }
            pass.set_pipeline(self.card_outline_pipeline(stage.pipeline));
            pass.set_bind_group(1, stage.color_group(), &[]);
            pass.draw_indexed(draw.indices.clone(), 0, draw.instances.clone());
        }
    }
}
