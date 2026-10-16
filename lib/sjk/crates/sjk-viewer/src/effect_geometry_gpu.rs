//! One-upload, fixed-capacity GPU submission for non-billboard FX geometry.
//!
//! World decals share the vertex/index upload but draw through a second
//! pipeline set with the polygon offset mark shaders declare
//! (`polygonOffset` → `qglPolygonOffset(-1, -2)`, `tr_shade.cpp:1811`), so
//! they win the depth test against the surface they were clipped from
//! without a depth-write.

use super::*;

#[path = "effect_expansion_gpu.rs"]
mod expansion;

/// `qglPolygonOffset(r_offsetFactor, r_offsetUnits)` with the -1 / -2
/// defaults (`tr_shade.cpp:1811`, `tr_init.cpp:1666-1667`).
const DECAL_BIAS: wgpu::DepthBiasState = wgpu::DepthBiasState {
    constant: -2,
    slope_scale: -1.0,
    clamp: 0.0,
};

pub(crate) struct Runtime {
    expansion: Option<expansion::Expansion>,
    mesh: crate::effect_geometry::Mesh,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    pipelines: [wgpu::RenderPipeline; crate::effect_blend::PIPELINE_COUNT],
    decal_pipelines: [wgpu::RenderPipeline; crate::effect_blend::PIPELINE_COUNT],
    stats: crate::effect_geometry::Stats,
}

impl Runtime {
    pub(crate) fn new(
        device: &wgpu::Device,
        camera_layout: &wgpu::BindGroupLayout,
        atlas_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK cylinder/electricity shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("effect_geometry.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK cylinder/electricity pipeline layout"),
            bind_group_layouts: &[Some(camera_layout), Some(atlas_layout)],
            immediate_size: 0,
        });
        let create_pipeline = |label, blend, bias| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[Some(crate::effect_geometry::Vertex::layout())],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fragment_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DepthTarget::FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias,
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipelines = crate::effect_blend::specifications()
            .map(|(label, blend)| create_pipeline(label, blend, Default::default()));
        let decal_pipelines = crate::effect_blend::specifications()
            .map(|(label, blend)| create_pipeline(label, blend, DECAL_BIAS));
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK fixed FX geometry vertices"),
            size: (crate::effect_geometry::MAX_VERTICES
                * std::mem::size_of::<crate::effect_geometry::Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK fixed FX geometry indices"),
            size: (crate::effect_geometry::MAX_INDICES * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let expansion = expansion::Expansion::new(device, &vertex_buffer, &index_buffer);
        let mut mesh = crate::effect_geometry::Mesh::default();
        if expansion.is_some() {
            mesh.enable_expansion();
        }
        Self {
            expansion,
            mesh,
            vertex_buffer,
            index_buffer,
            pipelines,
            decal_pipelines,
            stats: crate::effect_geometry::Stats::default(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare(
        &mut self,
        timing: &mut frame_pacing::budget::Timer,
        queue: &crate::frame_queue::FrameQueue,
        encoder: &mut wgpu::CommandEncoder,
        particles: &mut [Particle],
        decals: &mut crate::decal_store::DecalStore,
        atlas: &ParticleAtlas,
        now: Instant,
        global_seconds: f32,
        camera: Vec3,
        field_of_view: f32,
        bsp: &Bsp,
        scratch: &mut TraceScratch,
    ) {
        self.stats = self.mesh.build(
            particles,
            decals,
            atlas,
            now,
            global_seconds,
            camera,
            field_of_view,
            bsp,
            scratch,
        );
        timing.mark(frame_pacing::budget::Phase::EffectUploads);

        if let (Some(expansion), Some(batch)) = (&self.expansion, &self.mesh.expansion) {
            expansion.prepare(queue, encoder, batch);
            return;
        }
        if self.stats.vertices != 0 {
            queue.write_buffer(
                &self.vertex_buffer,
                0,
                bytemuck::cast_slice(self.mesh.vertices()),
            );
            queue.write_buffer(
                &self.index_buffer,
                0,
                bytemuck::cast_slice(self.mesh.indices()),
            );
        }
    }

    pub(crate) fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera: &'a wgpu::BindGroup,
        atlas: &'a wgpu::BindGroup,
    ) {
        if self.stats.indices == 0 {
            return;
        }
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(1, atlas, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        for (pipeline, range) in self.pipelines.iter().zip(self.mesh.ranges()) {
            if range.is_empty() {
                continue;
            }
            pass.set_pipeline(pipeline);
            pass.draw_indexed(range.clone(), 0, 0..1);
        }
    }

    /// Draw this frame's world marks. They go before every other effect, as rd-vanilla
    /// sorts mark shaders (`sort decal`) ahead of blended effects: an explosion's fire
    /// and smoke cover its own scorch mark instead of the mark showing through them.
    pub(crate) fn draw_decals<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera: &'a wgpu::BindGroup,
        atlas: &'a wgpu::BindGroup,
    ) {
        if self.stats.indices == 0
            || self
                .mesh
                .decal_ranges()
                .iter()
                .all(|range| range.is_empty())
        {
            return;
        }
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(1, atlas, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        for (pipeline, range) in self.decal_pipelines.iter().zip(self.mesh.decal_ranges()) {
            if range.is_empty() {
                continue;
            }
            pass.set_pipeline(pipeline);
            pass.draw_indexed(range.clone(), 0, 0..1);
        }
    }

    /// Add this frame's cylinders, lines, polygons and decals to the effect layer's screen
    /// bounds, from the CPU descriptions whichever backend tessellates them.
    pub(crate) fn bound(&self, bounds: &mut crate::frame_target::aa::effects::bounds::Bounds) {
        use glam::Vec3;
        if self.stats.indices == 0 {
            return;
        }
        let Some(batch) = &self.mesh.expansion else {
            for vertex in self.mesh.vertices() {
                bounds.sphere(Vec3::from_array(vertex.position), 0.0);
            }
            return;
        };
        for description in &batch.descriptions {
            let [a, b] = [description.a, description.b];
            match description.meta[0] {
                // Cylinder rings and widened lines: a segment and its larger radius.
                0 | 1 => bounds.segment(
                    Vec3::new(a[0], a[1], a[2]),
                    Vec3::new(b[0], b[1], b[2]),
                    a[3].abs().max(b[3].abs()),
                ),
                2 | 3 => {}
                _ => bounds.whole_screen(),
            }
        }
        // Clipped polygons (decals) and explicit quads carry their own points.
        for point in &batch.points {
            let [x, y, z, _] = point.position;
            bounds.sphere(Vec3::new(x, y, z), 0.0);
        }
    }

    /// Draw this frame's glowing cylinders, lines and electricity into the dynamic glow
    /// image with the ordinary effect pipelines (decals never glow here).
    pub(crate) fn draw_glow<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera: &'a wgpu::BindGroup,
        atlas: &'a wgpu::BindGroup,
    ) {
        if !self.has_glow() {
            return;
        }
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(1, atlas, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        for (pipeline, range) in self.pipelines.iter().zip(self.mesh.glow_ranges()) {
            if !range.is_empty() {
                pass.set_pipeline(pipeline);
                pass.draw_indexed(range.clone(), 0, 0..1);
            }
        }
    }

    /// Whether any glowing geometry was built this frame.
    pub(crate) fn has_glow(&self) -> bool {
        self.stats.indices != 0 && self.mesh.glow_ranges().iter().any(|r| !r.is_empty())
    }

    pub(crate) fn stats(&self) -> crate::effect_geometry::Stats {
        self.stats
    }
}
