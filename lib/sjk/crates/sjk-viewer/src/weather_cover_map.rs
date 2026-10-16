//! The rain cover around the camera, surveyed on a worker thread and kept in a
//! wrapping GPU texture.
//!
//! The world is cut into [`TILE`]-column tiles of [`CELL`]-unit columns. A worker
//! thread owns a [`Surveyor`] and surveys the tiles it is asked for, nearest first;
//! finished tiles stay cached for the map. The texture holds a window of
//! [`WINDOW_TILES`]² tiles centred on the camera, addressed by world cell modulo its
//! size, so moving the camera uploads only the tiles that enter the window. A column not
//! surveyed yet holds no weather ([`Column::OUTSIDE_MAP`]): weather appears around the
//! camera a moment after it starts rather than through a roof, and the fog reads the
//! far cover there. A second worker surveys that far cover once for the whole map
//! (`weather_cover_far.rs`) into a texture of its own. Nothing is surveyed on a map
//! without weather.

use super::cover::{Column, Marks, Surveyor};
use super::cover_far;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

/// World units per surveyed column.
pub(crate) const CELL: f32 = 16.0;
/// Columns along a tile's edge.
pub(crate) const TILE: usize = 32;
/// Tiles along the window's edge: 8 × 32 × 16 = 4096 units, at least 1536 on each side
/// of the camera.
pub(crate) const WINDOW_TILES: i32 = 8;
/// The texture's edge in texels.
pub(crate) const SIZE: u32 = WINDOW_TILES as u32 * TILE as u32;
/// Most cached tiles before the far ones are dropped (16 KiB each).
const CACHE_TILES: usize = 1024;

type Tile = [i32; 2];
type Texels = Box<[[f32; 4]]>;

/// What the shader needs to read the window.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Window {
    /// First cell of the window on each axis, and one past the last.
    pub(crate) cells: [f32; 4],
    /// The cover is in use: false on a map without sky, where weather is everywhere.
    pub(crate) enabled: bool,
    /// The far cover's first corner (x, y) and column width, and 1 once it is uploaded.
    pub(crate) far: [f32; 4],
}

struct Worker {
    requests: Sender<Tile>,
    results: Receiver<(Tile, Texels)>,
    /// The far cover over `far_grid`, sent once. Dropping `_stop_far` ends its survey
    /// early.
    far: Receiver<Texels>,
    far_grid: cover_far::Grid,
    _stop_far: Sender<()>,
}

/// The cover window and its tile cache.
pub(crate) struct CoverMap {
    texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    far_texture: wgpu::Texture,
    pub(crate) far_view: wgpu::TextureView,
    /// The far cover's grid, once its survey is uploaded.
    far: Option<cover_far::Grid>,
    worker: Option<Worker>,
    /// A sky was found; false leaves the cover off.
    has_sky: bool,
    cached: HashMap<Tile, Texels>,
    requested: HashSet<Tile>,
    /// Which tile each texture slot holds and whether it is surveyed there.
    slots: Vec<Option<(Tile, bool)>>,
    /// The window's first tile.
    origin: Option<Tile>,
    unsurveyed: Texels,
    /// Window tiles nearest first, relative to the origin; reused every move.
    order: Vec<Tile>,
}

impl CoverMap {
    /// The textures, empty, with no worker yet.
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let cover_texture = |label, size| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let texture = cover_texture("SJK weather cover", SIZE);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let far_texture = cover_texture("SJK weather far cover", cover_far::SIZE as u32);
        let far_view = far_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let centre = (WINDOW_TILES / 2) as f32 - 0.5;
        let mut order: Vec<Tile> = (0..WINDOW_TILES)
            .flat_map(|y| (0..WINDOW_TILES).map(move |x| [x, y]))
            .collect();
        order.sort_by(|a, b| {
            let distance =
                |tile: &Tile| (tile[0] as f32 - centre).powi(2) + (tile[1] as f32 - centre).powi(2);
            distance(a).total_cmp(&distance(b))
        });
        Self {
            texture,
            view,
            far_texture,
            far_view,
            far: None,
            worker: None,
            has_sky: true,
            cached: HashMap::new(),
            requested: HashSet::new(),
            slots: vec![None; (WINDOW_TILES * WINDOW_TILES) as usize],
            origin: None,
            unsurveyed: vec![Column::OUTSIDE_MAP.texel(); TILE * TILE].into_boxed_slice(),
            order,
        }
    }

    /// Start surveying `bsp` with `marks`, dropping any previous survey (the zones
    /// changed). Without a sky no worker starts and the cover stays off.
    pub(crate) fn start(&mut self, bsp: Arc<sjk_bsp::Bsp>, marks: Marks) {
        let bounds = bsp
            .render()
            .models()
            .first()
            .map_or([[-65536.0; 3], [65536.0; 3]], |model| {
                [model.minimums, model.maximums]
            });
        let far_surveyor = Surveyor::new(bsp.clone(), marks.clone());
        let surveyor = Surveyor::new(bsp, marks);
        self.has_sky = surveyor.has_sky();
        self.cached.clear();
        self.requested.clear();
        self.slots.fill(None);
        self.origin = None;
        self.far = None;
        self.worker = None;
        if !self.has_sky {
            return;
        }
        let (requests, jobs) = channel::<Tile>();
        let (done, results) = channel();
        let (stop_far, far_stop) = channel();
        let (far_done, far) = channel();
        let far_grid = cover_far::Grid::around(bounds);
        let spawned = std::thread::Builder::new()
            .name("sjk-weather-cover".into())
            .spawn(move || survey(surveyor, &jobs, &done))
            .and_then(|_| {
                std::thread::Builder::new()
                    .name("sjk-weather-far-cover".into())
                    .spawn(move || survey_far(far_surveyor, far_grid, bounds, &far_stop, &far_done))
            });
        match spawned {
            Ok(_) => {
                self.worker = Some(Worker {
                    requests,
                    results,
                    far,
                    far_grid,
                    _stop_far: stop_far,
                })
            }
            Err(error) => {
                crate::log::progress(format_args!("weather: cover worker failed: {error}"));
                self.has_sky = false;
            }
        }
    }

    /// Follow the camera: move the window, ask for its missing tiles, upload what is new.
    /// Allocation-free while the camera stays within its tile and nothing arrives.
    pub(crate) fn update(&mut self, queue: &crate::frame_queue::FrameQueue, camera: [f32; 3]) {
        if self.worker.is_none() {
            return;
        }
        let tile_size = CELL * TILE as f32;
        let origin =
            [0, 1].map(|axis| (camera[axis] / tile_size).round() as i32 - WINDOW_TILES / 2);
        if self.origin != Some(origin) {
            self.origin = Some(origin);
            for index in 0..self.order.len() {
                let [x, y] = self.order[index];
                let tile = [origin[0] + x, origin[1] + y];
                self.place(queue, tile);
            }
            self.trim(origin);
        }
        if let Some(worker) = &self.worker
            && self.far.is_none()
            && let Ok(texels) = worker.far.try_recv()
        {
            let size = cover_far::SIZE as u32;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.far_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&texels),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size * 16),
                    rows_per_image: Some(size),
                },
                wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
            );
            self.far = Some(worker.far_grid);
        }
        loop {
            let received = match &self.worker {
                Some(worker) => worker.results.try_recv(),
                None => return,
            };
            match received {
                Ok((tile, texels)) => {
                    self.requested.remove(&tile);
                    self.cached.insert(tile, texels);
                    if self.in_window(tile) {
                        self.place(queue, tile);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    crate::log::progress(format_args!("weather: cover worker stopped"));
                    self.worker = None;
                    break;
                }
            }
        }
    }

    /// The window as the shader reads it.
    pub(crate) fn window(&self) -> Window {
        let far = self.far.map_or([0.0; 4], |grid| {
            [grid.origin[0], grid.origin[1], grid.cell, 1.0]
        });
        let Some(origin) = self.origin.filter(|_| self.has_sky) else {
            return Window {
                cells: [0.0; 4],
                enabled: self.has_sky,
                far,
            };
        };
        let first = origin.map(|tile| (tile * TILE as i32) as f32);
        let span = (WINDOW_TILES * TILE as i32) as f32;
        Window {
            cells: [first[0], first[1], first[0] + span, first[1] + span],
            enabled: true,
            far,
        }
    }

    fn in_window(&self, tile: Tile) -> bool {
        self.origin.is_some_and(|origin| {
            (0..2).all(|axis| (origin[axis]..origin[axis] + WINDOW_TILES).contains(&tile[axis]))
        })
    }

    /// Put `tile` in its slot: its survey if cached, else covered while it is asked for.
    fn place(&mut self, queue: &crate::frame_queue::FrameQueue, tile: Tile) {
        let slot = slot_of(tile);
        let ready = self.cached.contains_key(&tile);
        if self.slots[slot] == Some((tile, ready)) {
            return;
        }
        let texels = self.cached.get(&tile).unwrap_or(&self.unsurveyed);
        let [x, y] = [0, 1].map(|axis| tile[axis].rem_euclid(WINDOW_TILES) as u32 * TILE as u32);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(texels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TILE as u32 * 16),
                rows_per_image: Some(TILE as u32),
            },
            wgpu::Extent3d {
                width: TILE as u32,
                height: TILE as u32,
                depth_or_array_layers: 1,
            },
        );
        self.slots[slot] = Some((tile, ready));
        if !ready
            && self.requested.insert(tile)
            && let Some(worker) = &self.worker
        {
            let _ = worker.requests.send(tile);
        }
    }

    /// Forget far tiles once the cache is large.
    fn trim(&mut self, origin: Tile) {
        if self.cached.len() <= CACHE_TILES {
            return;
        }
        let centre = origin.map(|tile| tile + WINDOW_TILES / 2);
        self.cached.retain(|tile, _| {
            (0..2).all(|axis| (tile[axis] - centre[axis]).abs() <= 3 * WINDOW_TILES)
        });
    }
}

/// The texture slot of a tile.
fn slot_of(tile: Tile) -> usize {
    let [x, y] = tile.map(|value| value.rem_euclid(WINDOW_TILES) as usize);
    y * WINDOW_TILES as usize + x
}

/// The worker: survey requested tiles until the map's cover is dropped.
fn survey(mut surveyor: Surveyor, jobs: &Receiver<Tile>, done: &Sender<(Tile, Texels)>) {
    while let Ok(tile) = jobs.recv() {
        let texels = survey_tile(&mut surveyor, tile);
        if done.send((tile, texels)).is_err() {
            return;
        }
    }
}

/// The far worker: survey the whole map's far cover once, then end.
fn survey_far(
    mut surveyor: Surveyor,
    grid: cover_far::Grid,
    bounds: [[f32; 3]; 2],
    stop: &Receiver<()>,
    done: &Sender<Texels>,
) {
    let started = std::time::Instant::now();
    if let Some(texels) = cover_far::survey(&mut surveyor, grid, bounds, stop) {
        crate::log::progress(format_args!(
            "weather: far cover ({} units a column) surveyed in {:.0} ms",
            grid.cell,
            started.elapsed().as_secs_f64() * 1000.0
        ));
        let _ = done.send(texels);
    }
}

/// Every column of one tile, row by row, sampled at the column's centre.
fn survey_tile(surveyor: &mut Surveyor, tile: Tile) -> Texels {
    let mut texels = Vec::with_capacity(TILE * TILE);
    for row in 0..TILE {
        for column in 0..TILE {
            let cell = [
                tile[0] * TILE as i32 + column as i32,
                tile[1] * TILE as i32 + row as i32,
            ];
            let [x, y] = cell.map(|cell| (cell as f32 + 0.5) * CELL);
            texels.push(surveyor.column(x, y).texel());
        }
    }
    texels.into_boxed_slice()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_wrap_with_the_world_and_never_collide_inside_a_window() {
        for origin in [[0, 0], [-3, 7], [-1000, 4093]] {
            let mut seen = HashSet::new();
            for y in 0..WINDOW_TILES {
                for x in 0..WINDOW_TILES {
                    assert!(seen.insert(slot_of([origin[0] + x, origin[1] + y])));
                }
            }
        }
        assert_eq!(
            slot_of([-1, -1]),
            slot_of([WINDOW_TILES - 1, WINDOW_TILES - 1])
        );
    }

    #[test]
    fn surveyed_tiles_hold_their_columns_row_by_row() {
        use sjk_bsp::{Bsp, CollisionShader, box_brush, write_collision_map};
        let shaders = [
            CollisionShader {
                name: "textures/stone".into(),
                surface_flags: 0,
                content_flags: 1,
            },
            CollisionShader {
                name: "textures/skies/day".into(),
                surface_flags: super::super::cover::SURF_SKY,
                content_flags: 1,
            },
        ];
        // A floor that steps up by 64 at x = 256, under one sky.
        let brushes = [
            box_brush([-512.0, -512.0, -64.0], [256.0, 512.0, 0.0], 0),
            box_brush([256.0, -512.0, -64.0], [512.0, 512.0, 64.0], 0),
            box_brush([-512.0, -512.0, 512.0], [512.0, 512.0, 528.0], 1),
        ];
        let data = write_collision_map("", &shaders, &brushes);
        let bsp = Arc::new(Bsp::parse(&data).unwrap());
        let mut surveyor = Surveyor::new(bsp.clone(), Marks::read(&bsp, &[]));
        let texels = survey_tile(&mut surveyor, [0, 0]);
        assert_eq!(texels.len(), TILE * TILE);
        // Column 15 is centred at x = 248, column 16 at x = 264.
        assert!((texels[15][0] - 0.0).abs() < 0.5, "{:?}", texels[15]);
        assert!((texels[16][0] - 64.0).abs() < 0.5, "{:?}", texels[16]);
        assert!((texels[TILE * 3 + 16][1] - 512.0).abs() < 0.5);
    }
}
