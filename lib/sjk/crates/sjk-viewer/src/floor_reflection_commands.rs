//! Suppress a hidden mirror's indirect geometry using this frame's exact depth
//! query. Each view owns a disjoint argument interval, including its light passes.
//! Uniforms are published after recording that interval; queue writes precede all
//! frame commands, so the earlier compute dispatch sees the final interval.
pub(in crate::scene_views::floor_reflections) struct Commands {
    pipeline: wgpu::ComputePipeline,
    slots: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
}
const CODE: &str = r#"
@group(0) @binding(0) var<storage,read> counts:array<vec2<u32>>;
@group(0) @binding(1) var<storage,read_write> arguments:array<u32>;
@group(0) @binding(2) var<uniform> region:vec4<u32>;
@compute @workgroup_size(64) fn suppress(@builtin(global_invocation_id) id:vec3<u32>) {
    if any(counts[region.x]!=vec2(0u)) {return;}
    for(var i=id.x;i<region.z;i+=4096u) {arguments[(region.y+i)*5u]=0u;}
}
"#;
impl Commands {
    pub(in crate::scene_views::floor_reflections) fn new(
        device: &wgpu::Device,
        counts: &wgpu::Buffer,
        arguments: &wgpu::Buffer,
        slots: usize,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hidden mirror commands"),
            source: wgpu::ShaderSource::Wgsl(CODE.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("hidden mirror commands"),
            layout: None,
            module: &shader,
            entry_point: Some("suppress"),
            compilation_options: Default::default(),
            cache: None,
        });
        let slots = (0..slots)
            .map(|_| {
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("mirror command interval"),
                    size: 16,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &pipeline.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: counts.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: arguments.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: buffer.as_entire_binding(),
                        },
                    ],
                });
                (buffer, group)
            })
            .collect();
        Self { pipeline, slots }
    }
    pub(in crate::scene_views::floor_reflections) fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        slot: usize,
    ) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("skip hidden mirror geometry"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.slots[slot].1, &[]);
        pass.dispatch_workgroups(64, 1, 1);
    }
    pub(in crate::scene_views::floor_reflections) fn publish(
        &self,
        queue: &crate::frame_queue::FrameQueue,
        slot: usize,
        plane: usize,
        bytes: std::ops::Range<u64>,
    ) {
        debug_assert!(bytes.start % 20 == 0 && bytes.end % 20 == 0);
        queue.write_buffer(
            &self.slots[slot].0,
            0,
            bytemuck::cast_slice(&[
                plane as u32,
                (bytes.start / 20) as u32,
                ((bytes.end - bytes.start) / 20) as u32,
                0,
            ]),
        );
    }
}
