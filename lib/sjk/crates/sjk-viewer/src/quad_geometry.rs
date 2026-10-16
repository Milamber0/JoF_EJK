//! Load-time vertex-to-quad references for autosprite geometry. Vertices remain
//! in the live shared buffer, so skeletal pose uploads and buffer growth are
//! visible to the deformation shader without CPU work per rendered quad.
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct QuadRef {
    vertices: [u32; 4],
    directed_edges: u32,
    valid: u32,
    padding: [u32; 2],
}

const EDGES: [[usize; 2]; 6] = [[0, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]];

pub(crate) fn references(count: usize, indices: &[u32], base: u32) -> Vec<QuadRef> {
    let mut result = vec![QuadRef::zeroed(); count];
    for triangles in indices.windows(6).step_by(3) {
        let mut sorted: [u32; 6] = triangles.try_into().unwrap();
        sorted.sort_unstable();
        let mut vertices = [0; 4];
        let mut unique = 0;
        for (i, &vertex) in sorted.iter().enumerate() {
            if i != 0 && vertex == sorted[i - 1] {
                continue;
            }
            if unique < 4 {
                vertices[unique] = vertex;
            }
            unique += 1;
        }
        if unique != 4 || vertices[3] as usize >= count {
            continue;
        }
        let mut directed_edges = 0;
        for (i, [a, b]) in EDGES.into_iter().enumerate() {
            if triangles
                .windows(2)
                .any(|pair| pair == [vertices[a], vertices[b]])
            {
                directed_edges |= 1 << i;
            }
        }
        let entry = QuadRef {
            vertices: vertices.map(|i| i + base),
            directed_edges,
            valid: 1,
            padding: [0; 2],
        };
        for index in vertices {
            result[index as usize] = entry;
        }
    }
    result
}

pub(crate) fn upload(
    device: &wgpu::Device,
    count: usize,
    indices: &[u32],
    base: u32,
) -> wgpu::Buffer {
    let refs = references(count.max(1), indices, base);
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("SJK immutable quad lookup"),
        contents: bytemuck::cast_slice(&refs),
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
    })
}

pub(crate) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SJK geometry deformation inputs"),
        entries: &[
            entry(0),
            entry(1),
            entry(3),
            entry(4),
            entry(5),
            // The stage path's lookup tables (`surface_tables.rs`), read by both stages.
            wgpu::BindGroupLayoutEntry {
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ..entry(6)
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

fn entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

pub(crate) fn bind(
    device: &wgpu::Device,
    vertices: &wgpu::Buffer,
    quads: &wgpu::Buffer,
) -> wgpu::BindGroup {
    let environment = super::environment::buffer(device, &Default::default());
    bind_with_environment(device, vertices, quads, &environment)
}

pub(crate) fn bind_with_environment(
    device: &wgpu::Device,
    vertices: &wgpu::Buffer,
    quads: &wgpu::Buffer,
    environment: &wgpu::Buffer,
) -> wgpu::BindGroup {
    let skin = crate::actor_pose::gpu_skinning::Buffers::empty(device);
    bind_with_skin(device, vertices, quads, environment, &skin)
}

/// Bind optional skin data alongside the existing shared deformation inputs.
pub(crate) fn bind_with_skin(
    device: &wgpu::Device,
    vertices: &wgpu::Buffer,
    quads: &wgpu::Buffer,
    environment: &wgpu::Buffer,
    skin: &crate::actor_pose::gpu_skinning::Buffers,
) -> wgpu::BindGroup {
    let tables = crate::surface_tables::buffer(device);
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("SJK geometry deformation inputs"),
        layout: &layout(device),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 6,
                resource: tables.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: skin.lookup.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: skin.vertices.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: skin.joints.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 0,
                resource: vertices.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: quads.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: environment.as_entire_binding(),
            },
        ],
    })
}

pub(crate) fn empty_binding(device: &wgpu::Device) -> wgpu::BindGroup {
    let zero = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("SJK geometry fallback"),
        contents: &[0; 256],
        usage: wgpu::BufferUsages::STORAGE,
    });
    bind(device, &zero, &zero)
}
