//! Static-world shadows for the scene's point lights: saber glow, bolts, explosions and
//! the other dynamic lights stop at walls instead of lighting the room behind them or
//! the corridor round a corner.
//!
//! Each frame, before anything draws, every light gets a 28×28 octahedral tile of how far
//! its light travels in each direction and the facing of what stops it, traced on the GPU
//! against the static triangles the lamps' visibility uses
//! ([`lamp_geometry`](super::lamp_geometry); glass, grates and sky let light through): 784
//! rays a light, 25,088 for a full list of 32. The tiles are copied into the point-light
//! uniform block after its CPU part ([`crate::dynamic_lights::BLOCK_BYTES`]), so every
//! program lit by point lights reads them without a new binding (`point_lights.wgsl`):
//! world surfaces, their material map highlights and models drawn with `r_modelPixelLight`.
//!
//! Only the real-time lighting mode builds the triangle tree; with baked lightmaps
//! (`r_dayNight 0`) nothing is traced and point lights shine through walls as in the
//! original game. `r_dynamicLightShadows 0` turns the tiles off for comparisons.
//! Movers are not in the tree, so a closed door does not stop a dynamic light.

use crate::dynamic_lights::{MAX_POINT_LIGHTS, SHADOW_HEADER_OFFSET, SHADOW_TILE_BYTES};

/// The header word group (lights with a tile) ahead of the tiles.
const HEADER_BYTES: u64 = 16;
/// Invocations per workgroup; each traces one word (two texels) of a tile.
const WORKGROUP: u32 = 64;

/// The frame's tile trace and the copy of its tiles into the point-light block.
pub(crate) struct Tracer {
    pipeline: wgpu::ComputePipeline,
    geometry: wgpu::BindGroup,
    group: wgpu::BindGroup,
    tiles: wgpu::Buffer,
    point_lights: wgpu::Buffer,
    /// Lights the next [`Self::encode`] traces, set when the frame's lights are uploaded.
    planned: std::cell::Cell<u32>,
}

impl Tracer {
    /// Build the tracer over `geometry` for the forge's point-light block.
    pub(crate) fn new(
        device: &wgpu::Device,
        geometry: &super::lamp_geometry::Runtime,
        point_lights: &wgpu::Buffer,
    ) -> Self {
        let tiles = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK point-light shadow tiles"),
            size: copy_bytes(MAX_POINT_LIGHTS as u32),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty,
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK point-light shadow tiles"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(
                    1,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
            ],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK point-light shadow tiles"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: point_lights.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: tiles.as_entire_binding(),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK point-light shadow tiles"),
            source: wgpu::ShaderSource::Wgsl(source().into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK point-light shadow tiles"),
            bind_group_layouts: &[Some(&geometry.layout), Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("SJK point-light shadow tiles"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("trace"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            pipeline,
            geometry: geometry.bind_group.clone(),
            group,
            tiles,
            point_lights: point_lights.clone(),
            planned: std::cell::Cell::new(0),
        }
    }

    /// Trace `lights` lights at the next [`Self::encode`] (none when `enabled` is off).
    /// Called with the frame's upload of the point-light block, whose CPU part resets the
    /// header: a frame that never encodes reads every light as unshadowed.
    pub(crate) fn plan(&self, lights: usize, enabled: bool) {
        self.planned.set(planned(lights, enabled));
    }

    /// Trace the planned tiles and copy them into the point-light block, once; before any
    /// pass reads the block this frame. Whether anything was traced.
    pub(crate) fn encode(&self, encoder: &mut wgpu::CommandEncoder) -> bool {
        let lights = self.planned.replace(0);
        if lights == 0 {
            return false;
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("SJK point-light shadow tiles"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.geometry, &[]);
            pass.set_bind_group(1, &self.group, &[]);
            let [x, y, z] = workgroups(lights);
            pass.dispatch_workgroups(x, y, z);
        }
        encoder.copy_buffer_to_buffer(
            &self.tiles,
            0,
            &self.point_lights,
            SHADOW_HEADER_OFFSET,
            copy_bytes(lights),
        );
        true
    }
}

/// The tracer's program: the static triangle tracer, the tile mapping and the trace.
fn source() -> String {
    format!(
        "{}{}{}",
        include_str!("lamp_geometry.wgsl"),
        include_str!("point_light_octa.wgsl"),
        include_str!("dynamic_light_shadows.wgsl")
    )
}

/// Lights traced in a frame with `lights` point lights.
fn planned(lights: usize, enabled: bool) -> u32 {
    if enabled {
        lights.min(MAX_POINT_LIGHTS) as u32
    } else {
        0
    }
}

/// Bytes the copy writes for `lights` traced lights: the header and their tiles.
fn copy_bytes(lights: u32) -> u64 {
    HEADER_BYTES + u64::from(lights) * SHADOW_TILE_BYTES
}

/// One invocation per tile word, one row of workgroups per light.
fn workgroups(lights: u32) -> [u32; 3] {
    let words = (SHADOW_TILE_BYTES / 4) as u32;
    [words.div_ceil(WORKGROUP), lights, 1]
}

#[cfg(test)]
#[path = "dynamic_light_shadows_tests.rs"]
mod tests;
