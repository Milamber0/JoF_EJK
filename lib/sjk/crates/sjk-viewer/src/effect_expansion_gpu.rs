//! Load-time GPU resources and allocation-free dispatch for effect expansion.
use crate::effect_geometry::{
    MAX_VERTICES,
    expansion::{Batch, Description, Point},
};

/// Fully built compute state; absence selects the unchanged CPU path for the whole runtime.
pub(crate) struct Expansion {
    descriptions: wgpu::Buffer,
    points: wgpu::Buffer,
    bind: wgpu::BindGroup,
    pipeline: wgpu::ComputePipeline,
}

impl Expansion {
    /// Construct transactionally before enabling descriptions on the mesh.
    pub(crate) fn new(
        device: &wgpu::Device,
        vertex: &wgpu::Buffer,
        index: &wgpu::Buffer,
    ) -> Option<Self> {
        if std::env::var("SJK_GPU_EFFECT_GEOMETRY").as_deref() == Ok("0") {
            return None;
        }
        let maximum = (MAX_VERTICES * std::mem::size_of::<crate::effect_geometry::Vertex>()) as u64;
        let limits = device.limits();
        if limits.max_storage_buffers_per_shader_stage < 4
            || u64::from(limits.max_storage_buffer_binding_size) < maximum
            || limits.max_compute_invocations_per_workgroup < 64
            || limits.max_compute_workgroup_size_x < 64
        {
            eprintln!("GPU effect expansion: storage limits require CPU fallback");
            return None;
        }
        let allocation_scope = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("effect geometry expansion"),
            source: wgpu::ShaderSource::Wgsl(include_str!("effect_expansion.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("effect geometry expansion"),
            layout: None,
            module: &shader,
            entry_point: Some("expand"),
            compilation_options: Default::default(),
            cache: None,
        });
        let buffer = |label, size| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let descriptions = buffer(
            "effect descriptions",
            (MAX_VERTICES / 3 * std::mem::size_of::<Description>()) as u64,
        );
        let points = buffer(
            "clipped decal points",
            (MAX_VERTICES * std::mem::size_of::<Point>()) as u64,
        );
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("effect expansion buffers"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[&descriptions, &points, vertex, index]
                .iter()
                .enumerate()
                .map(|(i, b)| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: b.as_entire_binding(),
                })
                .collect::<Vec<_>>(),
        });
        let validation = pollster::block_on(scope.pop());
        let allocation = pollster::block_on(allocation_scope.pop());
        if let Some(error) = validation.or(allocation) {
            eprintln!("GPU effect expansion: {error}; CPU fallback");
            return None;
        }
        Some(Self {
            descriptions,
            points,
            bind,
            pipeline,
        })
    }

    /// Upload descriptions only, then expand in the existing frame encoder before the draw pass.
    pub(crate) fn prepare(
        &self,
        queue: &crate::frame_queue::FrameQueue,
        encoder: &mut wgpu::CommandEncoder,
        batch: &Batch,
    ) {
        if batch.descriptions.is_empty() {
            return;
        }
        queue.write_buffer(
            &self.descriptions,
            0,
            bytemuck::cast_slice(&batch.descriptions),
        );
        if !batch.points.is_empty() {
            queue.write_buffer(&self.points, 0, bytemuck::cast_slice(&batch.points));
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("effect expansion"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.dispatch_workgroups(batch.descriptions.len() as u32, 1, 1);
    }
}
