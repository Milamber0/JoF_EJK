//! Flare-only stage pipelines: render after scene depth is complete, using
//! GPU texture loads instead of stock's synchronous glReadPixels per flare.
use super::*;
#[path = "frame_overlays.rs"]
mod overlays;

#[derive(Default)]
pub(super) struct Runtime {
    entries: Vec<Entry>,
}
struct Entry {
    material: usize,
    pipelines: Vec<(wgpu::RenderPipeline, wgpu::RenderPipeline)>,
}

pub(crate) fn depth_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SJK flare depth sample"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            // Froxel injection also reads scene depth to reject samples behind surfaces.
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT | wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    })
}

pub(crate) fn depth_binding(device: &wgpu::Device, view: &wgpu::TextureView) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("SJK flare scene depth"),
        layout: &depth_layout(device),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(view),
        }],
    })
}

/// Completed scene depth stays read-only while the vertex shader samples it.
pub(crate) fn begin_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    color: &wgpu::TextureView,
    depth: &wgpu::TextureView,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("SJK depth-sampled flares"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: color,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth,
            depth_ops: None,
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

impl Runtime {
    pub(super) fn new(device: &wgpu::Device, forge: &Forge, materials: &[Material]) -> Self {
        let entries = materials
            .iter()
            .enumerate()
            .filter(|(_, m)| m.flare)
            .map(|(material, m)| {
                let pipelines = m
                    .stages
                    .iter()
                    .map(|stage| {
                        let mut key = forge.pipeline_keys[stage.pipeline];
                        key.depth_write = false;
                        key.cull = None;
                        let create = |instanced, entry| {
                            gpu::create_pipeline_for_vertex(
                                device,
                                &forge.flare_layout,
                                &forge.shader,
                                forge.format,
                                key,
                                instanced,
                                entry,
                                "fragment_main",
                                "SJK BSP flare stage",
                                false,
                            )
                        };
                        (
                            create(false, "flare_vertex_main"),
                            create(true, "instanced_flare_vertex_main"),
                        )
                    })
                    .collect();
                Entry {
                    material,
                    pipelines,
                }
            })
            .collect();
        Self { entries }
    }
}

impl super::Runtime {
    pub(crate) fn has_flares(&self) -> bool {
        !self.flares.entries.is_empty()
    }

    pub(crate) fn draw_flares<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        input: &FrameDraw<'a>,
        depth: &'a wgpu::BindGroup,
    ) {
        if !self.has_flares() {
            return;
        }
        pass.set_bind_group(0, input.camera, &[]);
        pass.set_bind_group(2, &self.forge.geometry, &[]);
        pass.set_bind_group(3, depth, &[]);
        pass.set_vertex_buffer(0, input.vertices.slice(..));
        pass.set_vertex_buffer(1, input.instances.slice(..));
        pass.set_index_buffer(input.indices.slice(..), wgpu::IndexFormat::Uint32);
        for entry in &self.flares.entries {
            let material = &self.materials[entry.material];
            for (stage, (static_pipeline, instanced_pipeline)) in
                material.stages.iter().zip(&entry.pipelines)
            {
                pass.set_bind_group(1, &stage.bind_group, &[]);
                pass.set_pipeline(static_pipeline);
                let draws = if self.areas.active() {
                    &material.static_draws[..]
                } else {
                    material.static_draws_for(input.source_cluster, input.visibility)
                };
                for draw in draws {
                    if self
                        .areas
                        .visible(&draw.clusters, input.source_cluster, input.visibility)
                    {
                        pass.draw_indexed(draw.indices.clone(), 0, 0..1);
                    }
                }
                pass.set_pipeline(instanced_pipeline);
                for draw in &material.mover_draws {
                    if let Some(range) = input.mover_ranges.get(draw.mesh).filter(|r| !r.is_empty())
                    {
                        pass.draw_indexed(draw.indices.clone(), 0, range.clone());
                    }
                }
            }
        }
    }
}
