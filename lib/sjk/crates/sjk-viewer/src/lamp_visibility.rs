//! Map-lifetime static visibility for every fixture, independent of actor shadow slots.
//! Ray tracing happens once at installation. Receivers filter a fixed 5×5 footprint.
use super::Gpu;

const MAX_TEXELS: u64 = 32 * 1024 * 1024;
const RESOLUTION: u32 = 128;

fn shape(count: u32, limit: u32) -> Option<(u32, u32, [u32; 2])> {
    if count == 0 {
        return None;
    }
    let mut resolution = RESOLUTION;
    loop {
        let pitch = resolution + 2;
        let columns = (count as f64).sqrt().ceil() as u32;
        let columns = columns.min(limit / pitch).max(1);
        let rows = count.div_ceil(columns);
        // Candidate dimensions can exceed u32 before a lower-resolution retry.
        let size = [
            u64::from(columns) * u64::from(pitch),
            u64::from(rows) * u64::from(pitch),
        ];
        if size.iter().all(|&n| n <= u64::from(limit)) && size[0] * size[1] <= MAX_TEXELS {
            return Some((resolution, columns, size.map(|n| n as u32)));
        }
        if resolution == 1 {
            return None;
        }
        resolution = (resolution / 2).max(1);
    }
}

/// Door tiles the atlas can add to `lamps` lamp tiles at the resolution the lamps alone
/// would get: doors never coarsen the lamps' own shadows.
pub(super) fn door_capacity(lamps: u32, limit: u32) -> u32 {
    let Some((resolution, ..)) = shape(lamps, limit) else {
        return 0;
    };
    let fits = |doors: u32| {
        lamps
            .checked_add(doors)
            .and_then(|count| shape(count, limit))
            .is_some_and(|(r, ..)| r == resolution)
    };
    // The tile count a resolution fits only shrinks as the count grows.
    let (mut low, mut high) = (0u32, 1u32 << 20);
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if fits(middle) {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    low
}

pub(super) fn texture(device: &wgpu::Device, size: [u32; 2]) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("SJK all-fixture static visibility"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

impl Gpu {
    /// Trace static fixture visibility once. The atlas has a 128 MiB ceiling and no
    /// camera dependence. Resolution depends only on map size, never on the camera.
    pub(crate) fn prepare_visibility(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        geometry: &crate::world_materials::lamp_geometry::Runtime,
    ) {
        let Some((resolution, columns, size)) = shape(
            self.lamp_count + self.door_tiles,
            device.limits().max_texture_dimension_2d,
        ) else {
            return;
        };
        self.resolution = Some(resolution);
        self.visibility = texture(device, size);
        queue.write_buffer(&self.grid, 56, bytemuck::cast_slice(&[resolution, columns]));
        let mut entries = Self::layout_entries(0).to_vec();
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format: wgpu::TextureFormat::R32Float,
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            count: None,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &entries,
        });
        let mut entries = self.entries(0).to_vec();
        entries.push(wgpu::BindGroupEntry {
            binding: 2,
            resource: wgpu::BindingResource::TextureView(&self.visibility),
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &entries,
        });
        let source = format!(
            "{}{}{}",
            include_str!("lamp_geometry.wgsl"),
            include_str!("lamp_visibility_common.wgsl"),
            include_str!("lamp_visibility_build.wgsl")
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK fixture visibility tracing"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&geometry.layout), Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("SJK fixture visibility tracing"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("build"),
            compilation_options: Default::default(),
            cache: None,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &geometry.bind_group, &[]);
            pass.set_bind_group(1, &group, &[]);
            pass.dispatch_workgroups(size[0].div_ceil(8), size[1].div_ceil(8), 1);
        }
        queue.submit([encoder.finish()]);
        crate::log::progress(format_args!(
            "Fixture visibility: {} lamps and {} mover door tiles, {resolution} directional \
             samples per edge, {:.1} MiB",
            self.lamp_count,
            self.door_tiles,
            f64::from(size[0]) * f64::from(size[1]) * 4. / 1048576.
        ));
    }

    /// Fragment binding for the immutable map-lifetime distance atlas.
    pub(crate) fn visibility_layout(binding: u32) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }
    }
    /// Bind the atlas (one neutral texel when static geometry is unavailable).
    pub(crate) fn visibility_entry(&self, binding: u32) -> wgpu::BindGroupEntry<'_> {
        wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(&self.visibility),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn door_tiles_never_lower_the_lamps_resolution() {
        for lamps in [1, 100, 5_000, 20_000, 40_000] {
            let limit = 16_384;
            let alone = shape(lamps, limit).expect("a shape").0;
            let doors = door_capacity(lamps, limit);
            assert_eq!(shape(lamps + doors, limit).expect("a shape").0, alone);
            assert_ne!(shape(lamps + doors + 1, limit).map(|s| s.0), Some(alone));
        }
    }
}
