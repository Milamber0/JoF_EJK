//! Conservative depth bounds for exact static-shadow rejection.
use wgpu::util::DeviceExt;

// Level 0's tile, kept in step with `sun_shadow_bounds.wgsl` and `bounded_visibility`.
const TILE_SIZE: u32 = 8;

/// Min/max depths for the static view, close and far maps, in that layer order, as a
/// mip chain of ever wider tiles. Refresh a layer immediately after its source map,
/// before any receiver reads it.
pub(super) struct Bounds {
    pub(super) view: wgpu::TextureView,
    /// Per layer: level 0 from the depth map, then each coarser level from the last.
    passes: [Vec<(wgpu::BindGroup, u32)>; 3],
    build: wgpu::ComputePipeline,
    reduce: wgpu::ComputePipeline,
}
impl Bounds {
    /// Allocate one pair of bounds per tile at every level, including partial tiles at
    /// map edges; the coarsest level is a single tile over the whole map.
    pub(super) fn new(
        device: &wgpu::Device,
        resolution: u32,
        maps: [&wgpu::TextureView; 3],
    ) -> Self {
        // A power-of-two base keeps every coarser texel exactly 2x2 of the level below;
        // level 0's tiles past the map repeat its edge texels.
        let base = resolution.div_ceil(TILE_SIZE).next_power_of_two();
        let levels = base.ilog2() + 1;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SJK static shadow bounds"),
            size: wgpu::Extent3d {
                width: base,
                height: base,
                depth_or_array_layers: 3,
            },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let level = |mip| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                base_mip_level: mip,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        let levels_views: Vec<_> = (0..levels).map(level).collect();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK static shadow bounds"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sun_shadow_bounds.wgsl").into()),
        });
        let pipeline = |entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("SJK static shadow bounds"),
                layout: None,
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let (build, reduce) = (pipeline("build"), pipeline("reduce"));
        let passes = std::array::from_fn(|index| {
            let parameters = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&[index as u32, 0u32, 0u32, 0u32]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            (0..levels)
                .map(|mip| {
                    let mut entries = vec![
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(
                                &levels_views[mip as usize],
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: parameters.as_entire_binding(),
                        },
                    ];
                    entries.push(if mip == 0 {
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(maps[index]),
                        }
                    } else {
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(
                                &levels_views[mip as usize - 1],
                            ),
                        }
                    });
                    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("SJK static shadow bounds"),
                        layout: &if mip == 0 { &build } else { &reduce }.get_bind_group_layout(0),
                        entries: &entries,
                    });
                    (group, (base >> mip).max(1).div_ceil(8))
                })
                .collect()
        });
        Self {
            view,
            passes,
            build,
            reduce,
        }
    }
    /// Rebuild just the changed static cascade; moving casters never enter these bounds.
    pub(super) fn encode(&self, encoder: &mut wgpu::CommandEncoder, index: usize) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("SJK static shadow bounds"),
            timestamp_writes: None,
        });
        for (mip, (group, groups)) in self.passes[index].iter().enumerate() {
            pass.set_pipeline(if mip == 0 { &self.build } else { &self.reduce });
            pass.set_bind_group(0, group, &[]);
            pass.dispatch_workgroups(*groups, *groups, 1);
        }
    }
}
/// Shared receiver binding for conservative static-shadow rejection.
pub(in crate::world_materials) fn layout_entry() -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding: 7,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }
}
