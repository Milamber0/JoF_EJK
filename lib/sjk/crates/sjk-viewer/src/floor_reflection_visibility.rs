//! Same-frame floor occlusion without CPU readback or previous-frame visibility.
//! Hidden floors get a degenerate reflected projection before their passes execute.
//! Draw submission remains conservative; this suppresses raster and lighting work.
use super::*;
use wgpu::util::DeviceExt;
#[path = "floor_reflection_commands.rs"]
mod commands;

pub(super) struct Visibility {
    pub(super) commands: Option<commands::Commands>,
    queries: wgpu::QuerySet,
    counts: wgpu::Buffer,
    depth: wgpu::RenderPipeline,
    suppress: wgpu::ComputePipeline,
    cameras: Vec<wgpu::BindGroup>,
}
impl Visibility {
    pub(super) fn new(
        device: &wgpu::Device,
        camera: &wgpu::BindGroupLayout,
        floors: &[Floor],
    ) -> Option<Self> {
        if floors.is_empty() || floors.len() > wgpu::QUERY_SET_MAX_QUERIES as usize {
            return None;
        }
        let count = floors.len() as u32;
        let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("SJK floor visibility"),
            ty: wgpu::QueryType::Occlusion,
            count,
        });
        let counts = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK floor visibility counts"),
            size: u64::from(count) * 8,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK floor visibility"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("vertex_transform.wgsl"),
                    include_str!("floor_reflection.wgsl")
                )
                .into(),
            ),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(camera)],
            immediate_size: 0,
        });
        let depth = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SJK floor visibility"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("floor_vertex"),
                compilation_options: Default::default(),
                buffers: &[Some(GpuVertex::layout())],
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::DepthTarget::FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Equal),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                entry(0, wgpu::BufferBindingType::Storage { read_only: true }),
                entry(1, wgpu::BufferBindingType::Storage { read_only: false }),
                entry(2, wgpu::BufferBindingType::Uniform),
            ],
        });
        let cameras = floors
            .iter()
            .enumerate()
            .map(|(index, floor)| {
                let index = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::bytes_of(&(index as u32)),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: counts.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: floor.camera.buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: index.as_entire_binding(),
                        },
                    ],
                })
            })
            .collect();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK hidden reflection suppression"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
@group(0) @binding(0) var<storage,read> counts: array<vec2<u32>>;
@group(0) @binding(1) var<storage,read_write> camera: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> index: u32;
@compute @workgroup_size(1) fn suppress() {
    if any(counts[index] != vec2(0u)) { return; }
    // Every vertex projects to the same point beyond the far plane. No matrix
    // inversion or history is involved, and the next prepare uploads a fresh camera.
    camera[0] = vec4(0.0);
    camera[1] = vec4(0.0);
    camera[2] = vec4(0.0);
    camera[3] = vec4(0.0, 0.0, 2.0, 1.0);
}
"#
                .into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let suppress = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("SJK hidden reflection suppression"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("suppress"),
            compilation_options: Default::default(),
            cache: None,
        });
        Some(Self {
            commands: None,
            queries,
            counts,
            depth,
            suppress,
            cameras,
        })
    }

    pub(super) fn configure_commands(&mut self, device: &wgpu::Device, arguments: &wgpu::Buffer) {
        self.commands = Some(commands::Commands::new(
            device,
            &self.counts,
            arguments,
            MAX_MIRRORS,
        ));
    }

    pub(super) fn encode(
        &self,
        gpu: &GpuState,
        encoder: &mut wgpu::CommandEncoder,
        main: &crate::world_materials::FrameDraw<'_>,
    ) {
        let floors = &gpu.scene_views.floors;
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SJK visible reflective floors"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &gpu.depth.view,
                    depth_ops: None,
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: Some(&self.queries),
                multiview_mask: None,
            });
            pass.set_pipeline(&self.depth);
            pass.set_bind_group(0, main.camera, &[]);
            pass.set_vertex_buffer(0, main.vertices.slice(..));
            pass.set_index_buffer(main.indices.slice(..), wgpu::IndexFormat::Uint32);
            for (index, floor) in floors.floors.iter().enumerate().filter(|(_, f)| f.visible) {
                pass.begin_occlusion_query(index as u32);
                for face in &floor.plane.faces {
                    if gpu.world_materials.areas.visible(
                        &face.clusters,
                        floors.cluster,
                        gpu.bsp.render().visibility(),
                    ) {
                        pass.draw_indexed(face.indices.clone(), 0, 0..1);
                    }
                }
                pass.end_occlusion_query();
            }
        }
        encoder.resolve_query_set(
            &self.queries,
            0..floors.floors.len() as u32,
            &self.counts,
            0,
        );
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("SJK suppress invisible floor views"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.suppress);
        for (index, _) in floors.floors.iter().enumerate().filter(|(_, f)| f.visible) {
            pass.set_bind_group(0, &self.cameras[index], &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
    }
}
