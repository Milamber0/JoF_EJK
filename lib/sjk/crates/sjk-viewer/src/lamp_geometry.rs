//! Map-lifetime GPU residency of exact static lighting geometry and its surface table. Installed
//! only for the real-time lighting mode; the 2004 baked mode allocates nothing here.
use wgpu::util::DeviceExt;
#[path = "gi_geometry.rs"]
pub(crate) mod geometry;

/// GPU acceleration counts and numerical tolerances, independent of map extent.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Header {
    counts: [u32; 4],
    settings: [f32; 4],
}

/// Immutable triangle buffers (kept alive by the bind group) plus the surface table.
pub(crate) struct Runtime {
    pub(crate) layout: wgpu::BindGroupLayout,
    pub(crate) bind_group: wgpu::BindGroup,
}

impl Runtime {
    /// Build and upload the static visibility BVH and surface table.
    pub(crate) fn new(
        device: &wgpu::Device,
        geometry: &geometry::Geometry,
        surfaces: &[crate::gi_voxels::Surface],
    ) -> Self {
        let header = Header {
            counts: [
                (geometry.nodes.len() / 2) as u32,
                (geometry.triangles.len() / 3) as u32,
                surfaces.len().max(1) as u32,
                0,
            ],
            settings: [2., 0., 0., 0.],
        };
        let storage = |label, contents: &[u8]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage: wgpu::BufferUsages::STORAGE,
            })
        };
        let header_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SJK fixture header"),
            contents: bytemuck::bytes_of(&header),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let nodes = storage(
            "SJK fixture triangle BVH",
            bytemuck::cast_slice(&geometry.nodes),
        );
        let triangles = if geometry.triangles.is_empty() {
            &[[0u32; 4]][..]
        } else {
            geometry.triangles.as_slice()
        };
        let triangle_buffer = storage("SJK fixture triangles", bytemuck::cast_slice(triangles));
        let table: Vec<[f32; 8]> = surfaces
            .iter()
            .map(|s| {
                [
                    s.albedo[0],
                    s.albedo[1],
                    s.albedo[2],
                    f32::from(u8::from(s.sky)),
                    s.emission[0],
                    s.emission[1],
                    s.emission[2],
                    0.,
                ]
            })
            .collect();
        let table = if table.is_empty() {
            vec![[0.; 8]]
        } else {
            table
        };
        let surfaces_buffer = storage("SJK fixture surface table", bytemuck::cast_slice(&table));
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE | wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let read_only = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK fixture static geometry"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(1, read_only),
                entry(2, read_only),
                entry(3, read_only),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK fixture static geometry"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: header_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: nodes.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: triangle_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: surfaces_buffer.as_entire_binding(),
                },
            ],
        });
        Self { layout, bind_group }
    }
}
