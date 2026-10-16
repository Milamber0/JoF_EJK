//! GPU side of mover occlusion: occluder triangles and poses, and the trace of queued
//! door tiles into the lamp visibility atlas.
use super::{Doors, Occluder, Poses, Queue, Tracking};
use wgpu::util::DeviceExt;

/// Rays traced per frame: 31 tiles of 128² (about 17,000 rays each) or 120 of 64²,
/// each ray tested against the few movers near its lamp.
const RAYS_PER_FRAME: u32 = 1 << 19;
/// The most tiles one frame traces, whatever the resolution.
const MAX_TILES: usize = 512;
/// While movers keep moving, the lamp cache follows them this often; once every queued
/// tile is traced it follows at once.
const CACHE_REFRESH: std::time::Duration = std::time::Duration::from_millis(100);
/// Cache layers baked again in one frame; the rest wait for the next.
const LAYERS_PER_FRAME: usize = 4;

/// Pose tracking of one map's shadow-casting movers and the tracer of their door tiles.
pub(crate) struct Runtime {
    tracking: Tracking,
    /// `None` without door tiles: the poses then only tell the far sun cascade when
    /// movers have moved.
    tracer: Option<Tracer>,
}

/// The GPU trace of queued door tiles and the lamp cache regions it makes stale.
struct Tracer {
    batch: Vec<u32>,
    /// Tiles traced per frame at this atlas resolution.
    budget: usize,
    /// Tiles written to `work` and not yet traced.
    pending: std::cell::Cell<u32>,
    /// The poses `table` holds, by `Poses::generation`.
    uploaded: Option<u64>,
    table: Vec<[f32; 4]>,
    table_buffer: wgpu::Buffer,
    work: wgpu::Buffer,
    /// Per queued tile: the pixel rectangle its movers cover (`mover_occlusion.wgsl`).
    covered: wgpu::Buffer,
    /// `covered`'s starting value for each queued tile.
    uncovered: Vec<[u32; 4]>,
    pipeline: wgpu::ComputePipeline,
    publish: wgpu::ComputePipeline,
    group: wgpu::BindGroup,
    pitch: u32,
    /// Per door tile, the lamp cache texels its movers can shadow
    /// (`mover_occlusion::cache_regions`).
    regions: Vec<Vec<(u32, [u32; 4])>>,
    /// Cache texels of traced tiles waiting to be baked again, one rectangle per layer.
    stale: std::cell::RefCell<std::collections::BTreeMap<u32, [u32; 4]>>,
    refreshed: std::cell::Cell<Option<std::time::Instant>>,
}

impl Runtime {
    /// `None` without occluders. `doors` (`lamp_lights::Gpu::take_doors`) are traced when
    /// there are any and the atlas has a resolution; otherwise only poses are tracked.
    pub(crate) fn new(
        device: &wgpu::Device,
        lamps: &crate::lamp_lights::Gpu,
        doors: Option<Doors>,
        lamp_set: &[crate::lamp_lights::Lamp],
        occluders: &[Occluder],
        cache: Option<&crate::world_materials::lamp_cache::Pages>,
    ) -> Option<Self> {
        if occluders.is_empty() {
            return None;
        }
        let tracking = Tracking::new(
            occluders,
            doors.unwrap_or_else(|| Doors::none(occluders.len())),
        );
        let tracer = lamps
            .visibility_resolution()
            .filter(|_| !tracking.doors.tiles.is_empty())
            .map(|resolution| {
                Tracer::new(
                    device,
                    lamps,
                    &tracking.doors,
                    lamp_set,
                    occluders,
                    cache,
                    resolution,
                )
            });
        if tracer.is_none() {
            crate::log::progress(format_args!(
                "Mover occlusion: {} movers tracked for the far sun cascade, no door tiles",
                occluders.len()
            ));
        }
        Some(Self { tracking, tracer })
    }
}

impl Tracer {
    fn new(
        device: &wgpu::Device,
        lamps: &crate::lamp_lights::Gpu,
        doors: &Doors,
        lamp_set: &[crate::lamp_lights::Lamp],
        occluders: &[Occluder],
        cache: Option<&crate::world_materials::lamp_cache::Pages>,
        resolution: u32,
    ) -> Self {
        let mut triangles: Vec<[f32; 4]> = Vec::new();
        let mut table = Vec::with_capacity(occluders.len() * 4);
        for occluder in occluders {
            let first = (triangles.len() / 3) as u32;
            for [a, b, c] in &occluder.triangles {
                triangles.push(a.extend(0.).to_array());
                triangles.push((*b - *a).extend(0.).to_array());
                triangles.push((*c - *a).extend(0.).to_array());
            }
            let count = occluder.triangles.len() as u32;
            table.extend([
                [0., 0., 0., 1.],
                [0.; 4],
                occluder.lower.extend(f32::from_bits(first)).to_array(),
                occluder.upper.extend(f32::from_bits(count)).to_array(),
            ]);
        }
        let mut tiles: Vec<[u32; 4]> = Vec::with_capacity(doors.tiles.len());
        let mut lists: Vec<u32> = Vec::new();
        for tile in &doors.tiles {
            tiles.push([
                tile.lamp,
                lists.len() as u32,
                tile.occluders.len() as u32,
                0,
            ]);
            lists.extend(&tile.occluders);
        }
        let storage = |label, contents: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage: wgpu::BufferUsages::STORAGE | usage,
            })
        };
        let table_buffer = storage(
            "SJK mover occluder poses",
            bytemuck::cast_slice(&table),
            wgpu::BufferUsages::COPY_DST,
        );
        let triangle_buffer = storage(
            "SJK mover occluder triangles",
            bytemuck::cast_slice(&triangles),
            wgpu::BufferUsages::empty(),
        );
        let tile_buffer = storage(
            "SJK door tiles",
            bytemuck::cast_slice(&tiles),
            wgpu::BufferUsages::empty(),
        );
        let list_buffer = storage(
            "SJK door tile occluders",
            bytemuck::cast_slice(&lists),
            wgpu::BufferUsages::empty(),
        );
        let work = storage(
            "SJK door tiles to trace",
            bytemuck::cast_slice(&[0u32; MAX_TILES]),
            wgpu::BufferUsages::COPY_DST,
        );
        let covered = storage(
            "SJK door tile coverage",
            bytemuck::cast_slice(&[[0u32; 4]; MAX_TILES]),
            wgpu::BufferUsages::COPY_DST,
        );
        let read_only = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let mut entries = crate::lamp_lights::Gpu::layout_entries(0).to_vec();
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
        entries.extend((3..8).map(read_only));
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 8,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK door tile tracing"),
            entries: &entries,
        });
        let mut entries = lamps.entries(0).to_vec();
        entries.push(wgpu::BindGroupEntry {
            binding: 2,
            resource: wgpu::BindingResource::TextureView(lamps.visibility_view()),
        });
        for (binding, buffer) in [
            (3, &table_buffer),
            (4, &triangle_buffer),
            (5, &tile_buffer),
            (6, &list_buffer),
            (7, &work),
            (8, &covered),
        ] {
            entries.push(wgpu::BindGroupEntry {
                binding,
                resource: buffer.as_entire_binding(),
            });
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK door tile tracing"),
            layout: &layout,
            entries: &entries,
        });
        let source = format!(
            "{}{}",
            include_str!("lamp_visibility_common.wgsl"),
            include_str!("mover_occlusion.wgsl")
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK door tile tracing"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK door tile tracing"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let entry = |label, entry_point| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let pipeline = entry("SJK door tile tracing", "trace");
        let publish = entry("SJK door tile coverage", "publish");
        let started = std::time::Instant::now();
        let regions = cache.map_or_else(
            || vec![Vec::new(); doors.tiles.len()],
            |pages| {
                super::cache_regions(
                    lamp_set,
                    doors,
                    occluders,
                    pages.surfaces(),
                    pages.resolution(),
                )
            },
        );
        crate::log::progress(format_args!(
            "Mover occlusion: {} movers ({} triangles) shadow {} lamps; their cache \
             regions found in {:.0} ms",
            occluders.len(),
            triangles.len() / 3,
            doors.tiles.len(),
            started.elapsed().as_secs_f64() * 1e3
        ));
        Self {
            batch: Vec::with_capacity(MAX_TILES),
            budget: ((RAYS_PER_FRAME / ((resolution + 2) * (resolution + 2))) as usize)
                .clamp(8, MAX_TILES),
            pending: std::cell::Cell::new(0),
            uploaded: None,
            table,
            table_buffer,
            work,
            covered,
            uncovered: vec![[u32::MAX, u32::MAX, 0, 0]; MAX_TILES],
            pipeline,
            publish,
            group,
            pitch: resolution + 2,
            regions,
            stale: Default::default(),
            refreshed: std::cell::Cell::new(None),
        }
    }

    /// Upload the poses if they changed and, once the last batch is traced, the next tiles.
    fn prepare(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        poses: &Poses,
        tiles: &mut Queue,
    ) {
        if self.uploaded != Some(poses.generation) {
            for (index, pose) in poses.current.iter().enumerate() {
                self.table[index * 4] = pose.rotation.to_array();
                self.table[index * 4 + 1] = pose
                    .origin
                    .extend(f32::from(u8::from(pose.blocking)))
                    .to_array();
            }
            queue.write_buffer(&self.table_buffer, 0, bytemuck::cast_slice(&self.table));
            self.uploaded = Some(poses.generation);
        }
        if self.pending.get() == 0 {
            tiles.take(self.budget, &mut self.batch);
            if !self.batch.is_empty() {
                queue.write_buffer(&self.work, 0, bytemuck::cast_slice(&self.batch));
                queue.write_buffer(
                    &self.covered,
                    0,
                    bytemuck::cast_slice(&self.uncovered[..self.batch.len()]),
                );
                self.pending.set(self.batch.len() as u32);
            }
        }
    }

    fn encode(&self, encoder: &mut wgpu::CommandEncoder) {
        let tiles = self.pending.replace(0);
        if tiles == 0 {
            return;
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("SJK door tile tracing"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        let groups = self.pitch.div_ceil(8);
        pass.dispatch_workgroups(groups, groups, tiles);
        pass.set_pipeline(&self.publish);
        pass.dispatch_workgroups(tiles, 1, 1);
        let mut stale = self.stale.borrow_mut();
        for &tile in &self.batch[..tiles as usize] {
            for &(layer, rect) in &self.regions[tile as usize] {
                stale
                    .entry(layer)
                    .and_modify(|r| *r = super::merge(*r, rect))
                    .or_insert(rect);
            }
        }
    }
}

/// Lamp cache regions to bake again this frame (`Runtime::take_refresh`), without a heap.
#[derive(Default)]
pub(crate) struct Refresh {
    regions: [(u32, [u32; 4]); LAYERS_PER_FRAME],
    count: usize,
}

impl Refresh {
    pub(crate) fn regions(&self) -> &[(u32, [u32; 4])] {
        &self.regions[..self.count]
    }
}

impl Runtime {
    /// Place this frame's movers ([`Tracking::observe`]) and upload what changed and the
    /// next tiles to trace.
    pub(crate) fn observe(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        presented: &[crate::movers::Presented],
        baselines: Option<impl Iterator<Item = (crate::movers::Presented, bool)>>,
        mesh_of: impl Fn(usize) -> Option<usize>,
        in_view: Option<impl Fn(&super::Sight) -> bool>,
    ) {
        self.tracking
            .observe(presented, baselines, mesh_of, in_view);
        if let Some(tracer) = &mut self.tracer {
            tracer.prepare(queue, &self.tracking.poses, &mut self.tracking.queue);
        }
    }

    /// Trace the tiles `observe` queued, once.
    pub(crate) fn encode(&self, encoder: &mut wgpu::CommandEncoder) {
        if let Some(tracer) = &self.tracer {
            tracer.encode(encoder);
        }
    }

    /// The lamps `occluder` has door tiles for, and whether any tile is still queued.
    #[cfg(test)]
    pub(crate) fn door_lamps(&self, occluder: usize) -> (Vec<u32>, bool) {
        (
            self.tracking.doors.by_occluder[occluder]
                .iter()
                .map(|&tile| self.tracking.doors.tiles[tile as usize].lamp)
                .collect(),
            self.tracking.queue.pending_len() > 0
                || self.tracer.as_ref().is_some_and(|t| t.pending.get() > 0),
        )
    }

    /// Cache regions to bake again this frame: every [`CACHE_REFRESH`] while tiles are
    /// still queued, at once when none are; at most [`LAYERS_PER_FRAME`] layers.
    pub(crate) fn take_refresh(&self) -> Refresh {
        let mut refresh = Refresh::default();
        let Some(tracer) = &self.tracer else {
            return refresh;
        };
        let mut stale = tracer.stale.borrow_mut();
        let settled = self.tracking.queue.pending_len() == 0 && tracer.pending.get() == 0;
        let due = tracer
            .refreshed
            .get()
            .is_none_or(|at| at.elapsed() >= CACHE_REFRESH);
        if stale.is_empty() || !(settled || due) {
            return refresh;
        }
        tracer.refreshed.set(Some(std::time::Instant::now()));
        while refresh.count < LAYERS_PER_FRAME {
            let Some(region) = stale.pop_first() else {
                break;
            };
            refresh.regions[refresh.count] = region;
            refresh.count += 1;
        }
        refresh
    }

    /// Whether traced door tiles make lamp cache regions stale (`Cache::refresh`).
    pub(crate) fn rebakes_cache(&self) -> bool {
        self.tracer
            .as_ref()
            .is_some_and(|tracer| tracer.regions.iter().any(|regions| !regions.is_empty()))
    }

    /// Counts mover pose changes that affect light.
    pub(crate) fn generation(&self) -> u64 {
        self.tracking.poses.generation
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_door_tile_tracer_validates() {
        crate::wgsl_source::validate(&format!(
            "{}{}",
            include_str!("lamp_visibility_common.wgsl"),
            include_str!("mover_occlusion.wgsl")
        ));
    }
}
