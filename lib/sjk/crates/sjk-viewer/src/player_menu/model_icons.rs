//! The character grid's model icons. A catalogue can list far more models
//! than the UI atlas has cells for them (an install with community packs
//! lists over a thousand), so the cells are a cache: a tile asks for its icon
//! when it is drawn, gets a free cell or the one drawn least recently, and a
//! worker thread decodes the picture into it. A model whose icon file cannot
//! be decoded is remembered, and its tile shows the model's name instead.

use super::icons::MODEL_ICONS;
use crate::ui_renderer::{ICON_SIZE, ShapeRenderer};
use sjk_client::LegacyAssetCatalog;
use sjk_ui::TextureId;
use sjk_vfs::VirtualFileSystem;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

/// [`Cell::entry`] of a cell holding nothing.
const NO_ENTRY: usize = usize::MAX;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Free,
    Loading,
    Ready,
}

/// One atlas cell: the catalogue entry it holds and when a tile last drew it.
#[derive(Clone, Copy, Debug)]
struct Cell {
    entry: usize,
    state: State,
    used: u64,
}

/// Decode `paths` (the first that decodes) for `entry` into `cell`.
struct Job {
    entry: usize,
    cell: usize,
    paths: Vec<String>,
}

struct Done {
    entry: usize,
    cell: usize,
    /// [`ICON_SIZE`]-square RGBA, or `None` when no path decoded.
    rgba: Option<Vec<u8>>,
}

struct Worker {
    jobs: Sender<Vec<Job>>,
    done: Receiver<Done>,
}

pub(super) struct ModelIcons {
    cells: Vec<Cell>,
    /// Cell of each entry holding or loading an icon.
    by_entry: HashMap<usize, usize>,
    /// Entries whose icon could not be decoded.
    failed: HashSet<usize>,
    /// Which entry each cell wants, shared with the worker so it skips jobs
    /// whose cell has since gone to another tile.
    wanted: Arc<Vec<AtomicUsize>>,
    /// Icon paths to try for each catalogue entry, in grid order.
    paths: Vec<Vec<String>>,
    /// The catalogue the paths were read from.
    catalog: Option<Arc<LegacyAssetCatalog>>,
    frame: u64,
    jobs: Vec<Job>,
    worker: Option<Worker>,
    /// Decoded icons waiting for the atlas.
    uploads: Vec<(usize, usize, Vec<u8>)>,
}

impl ModelIcons {
    pub(super) fn new() -> Self {
        Self {
            cells: vec![
                Cell {
                    entry: NO_ENTRY,
                    state: State::Free,
                    used: 0,
                };
                MODEL_ICONS
            ],
            by_entry: HashMap::with_capacity(MODEL_ICONS),
            failed: HashSet::new(),
            wanted: Arc::new(
                (0..MODEL_ICONS)
                    .map(|_| AtomicUsize::new(NO_ENTRY))
                    .collect(),
            ),
            paths: Vec::new(),
            catalog: None,
            frame: 1,
            jobs: Vec::new(),
            worker: None,
            uploads: Vec::new(),
        }
    }

    /// Atlas cell `cell`'s texture: cell 0 of the atlas stays the HUD's.
    fn texture(cell: usize) -> TextureId {
        TextureId(cell as u32 + 1)
    }

    /// Serve `catalog`'s icons from `vfs`. Nothing changes when it is the
    /// catalogue already served; another one empties the cache.
    pub(super) fn attach(
        &mut self,
        vfs: &Arc<VirtualFileSystem>,
        catalog: &Arc<LegacyAssetCatalog>,
    ) {
        if self
            .catalog
            .as_ref()
            .is_some_and(|served| Arc::ptr_eq(served, catalog))
        {
            return;
        }
        self.catalog = Some(Arc::clone(catalog));
        self.paths = icon_paths(catalog);
        self.failed.clear();
        self.forget_uploads();
        self.worker = spawn(Arc::clone(vfs), Arc::clone(&self.wanted));
    }

    /// Empty every cell, for an atlas that holds none of the icons uploaded
    /// so far (another world's): tiles ask for their icons again. Icons
    /// that failed to decode stay failed.
    pub(super) fn forget_uploads(&mut self) {
        self.by_entry.clear();
        self.jobs.clear();
        self.uploads.clear();
        for (cell, wanted) in self.cells.iter_mut().zip(self.wanted.iter()) {
            *cell = Cell {
                entry: NO_ENTRY,
                state: State::Free,
                used: 0,
            };
            wanted.store(NO_ENTRY, Ordering::Relaxed);
        }
    }

    /// The icon of catalogue entry `entry` if it is in the atlas, asking for
    /// it otherwise. Call it for every tile drawn: what is drawn stays cached.
    pub(super) fn icon(&mut self, entry: usize) -> Option<TextureId> {
        if let Some(&cell) = self.by_entry.get(&entry) {
            let slot = &mut self.cells[cell];
            slot.used = self.frame;
            return (slot.state == State::Ready).then(|| Self::texture(cell));
        }
        if entry < self.paths.len() && !self.failed.contains(&entry) {
            self.load(entry);
        }
        None
    }

    /// Whether `entry`'s icon file could not be decoded.
    pub(super) fn failed(&self, entry: usize) -> bool {
        self.failed.contains(&entry)
    }

    /// Give `entry` a cell, a free one or the one drawn least recently (not
    /// in this frame or the last), and queue its decode.
    fn load(&mut self, entry: usize) {
        let recent = self.frame.saturating_sub(1);
        let Some(cell) = self
            .cells
            .iter()
            .enumerate()
            .filter(|(_, cell)| cell.state == State::Free || cell.used < recent)
            .min_by_key(|(_, cell)| (cell.state != State::Free, cell.used))
            .map(|(index, _)| index)
        else {
            return;
        };
        let old = self.cells[cell].entry;
        if old != NO_ENTRY {
            self.by_entry.remove(&old);
        }
        self.cells[cell] = Cell {
            entry,
            state: State::Loading,
            used: self.frame,
        };
        self.wanted[cell].store(entry, Ordering::Relaxed);
        self.by_entry.insert(entry, cell);
        self.jobs.push(Job {
            entry,
            cell,
            paths: self.paths[entry].clone(),
        });
    }

    /// Take the worker's finished icons.
    pub(super) fn poll(&mut self) {
        let Some(worker) = &self.worker else {
            return;
        };
        while let Ok(done) = worker.done.try_recv() {
            let current = self.cells[done.cell].entry == done.entry;
            match done.rgba {
                Some(rgba) if current => self.uploads.push((done.entry, done.cell, rgba)),
                Some(_) => {}
                None => {
                    self.failed.insert(done.entry);
                    if current {
                        self.by_entry.remove(&done.entry);
                        self.cells[done.cell] = Cell {
                            entry: NO_ENTRY,
                            state: State::Free,
                            used: 0,
                        };
                        self.wanted[done.cell].store(NO_ENTRY, Ordering::Relaxed);
                    }
                }
            }
        }
    }

    /// Upload up to `limit` decoded icons, send this frame's requests to the
    /// worker and start the next frame.
    pub(super) fn end_frame(
        &mut self,
        renderer: &ShapeRenderer,
        queue: &crate::frame_queue::FrameQueue,
        limit: usize,
    ) {
        let count = limit.min(self.uploads.len());
        for (entry, cell, rgba) in self.uploads.drain(..count) {
            // Still this entry's: the cell may have moved on since.
            if self.cells[cell].entry == entry {
                renderer.upload_icon(queue, Self::texture(cell), &rgba);
                self.cells[cell].state = State::Ready;
            }
        }
        if !self.jobs.is_empty() {
            let jobs = std::mem::take(&mut self.jobs);
            if let Some(worker) = &self.worker
                && worker.jobs.send(jobs).is_err()
            {
                self.worker = None;
            }
        }
        self.frame += 1;
    }
}

/// Icon paths of every catalogue entry in grid order: each character's
/// icon, then each species' first head icon in the retail JPG, PNG, TGA
/// order.
fn icon_paths(catalog: &LegacyAssetCatalog) -> Vec<Vec<String>> {
    let characters = catalog
        .characters
        .iter()
        .map(|character| vec![character.icon.clone()]);
    let species = catalog.species.iter().map(|species| {
        let stem = species.heads.first().map_or("default", String::as_str);
        let base = format!("models/players/{}/icon_{stem}", species.model);
        ["jpg", "png", "tga"]
            .map(|extension| format!("{base}.{extension}"))
            .to_vec()
    });
    characters.chain(species).collect()
}

fn spawn(vfs: Arc<VirtualFileSystem>, wanted: Arc<Vec<AtomicUsize>>) -> Option<Worker> {
    let (jobs, job_queue) = mpsc::channel::<Vec<Job>>();
    let (finished, done) = mpsc::channel();
    std::thread::Builder::new()
        .name("model icons".to_owned())
        .spawn(move || {
            while let Ok(mut batch) = job_queue.recv() {
                while let Ok(more) = job_queue.try_recv() {
                    batch.extend(more);
                }
                for job in batch {
                    // Scrolled past before its turn: the cell wants another.
                    if wanted[job.cell].load(Ordering::Relaxed) != job.entry {
                        continue;
                    }
                    let rgba = decode_first(&vfs, &job.paths);
                    let done = Done {
                        entry: job.entry,
                        cell: job.cell,
                        rgba,
                    };
                    if finished.send(done).is_err() {
                        return;
                    }
                }
            }
        })
        .ok()?;
    Some(Worker { jobs, done })
}

/// The first of `paths` that decodes, as [`ICON_SIZE`]-square RGBA. Icons
/// are decoded without the shared image cache: a thousand of them would
/// stay in memory at full size for the whole session.
fn decode_first(vfs: &VirtualFileSystem, paths: &[String]) -> Option<Vec<u8>> {
    paths.iter().find_map(|path| {
        let asset = vfs.read(path).ok().flatten()?;
        let image = crate::decode_image(&asset.bytes, path).ok()?.into_rgba8();
        Some(
            image::imageops::resize(
                &image,
                ICON_SIZE,
                ICON_SIZE,
                image::imageops::FilterType::Triangle,
            )
            .into_raw(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog(count: usize) -> Arc<LegacyAssetCatalog> {
        Arc::new(LegacyAssetCatalog {
            characters: (0..count)
                .map(|index| sjk_client::LegacyCharacter {
                    rig: false,
                    model: format!("m{index}"),
                    skin: "default".to_owned(),
                    cvar_value: format!("m{index}/default"),
                    icon: format!("models/players/m{index}/icon_default.png"),
                })
                .collect(),
            retail_characters: 0,
            species: Vec::new(),
            retail_species: 0,
            saber_hilts: Vec::new(),
            retail_saber_hilts: 0,
            excluded_saber_hilts: 0,
            saber_colors: sjk_client::LEGACY_SABER_COLORS,
        })
    }

    fn icon_png() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbaImage::from_pixel(4, 4, image::Rgba([200, 100, 50, 255]))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    /// A VFS holding a PNG icon for every even model and garbage for odd ones.
    fn vfs(count: usize) -> Arc<VirtualFileSystem> {
        let mut vfs = VirtualFileSystem::new();
        let png = icon_png();
        vfs.mount_memory(
            "icons",
            (0..count).map(|index| {
                let bytes = if index % 2 == 0 {
                    png.clone()
                } else {
                    b"not an image".to_vec()
                };
                (format!("models/players/m{index}/icon_default.png"), bytes)
            }),
        )
        .unwrap();
        Arc::new(vfs)
    }

    /// Run the worker until nothing is queued, the way frames would.
    fn settle(icons: &mut ModelIcons) {
        if !icons.jobs.is_empty() {
            let jobs = std::mem::take(&mut icons.jobs);
            icons.worker.as_ref().unwrap().jobs.send(jobs).unwrap();
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while icons.cells.iter().any(|cell| cell.state == State::Loading)
            && std::time::Instant::now() < deadline
        {
            icons.poll();
            for (entry, cell, _) in icons.uploads.drain(..) {
                if icons.cells[cell].entry == entry {
                    icons.cells[cell].state = State::Ready;
                }
            }
            std::thread::yield_now();
        }
        icons.frame += 1;
    }

    #[test]
    fn far_more_models_than_cells_all_get_icons_as_they_are_drawn() {
        let count = MODEL_ICONS * 4;
        let (vfs, catalog) = (vfs(count), catalog(count));
        let mut icons = ModelIcons::new();
        icons.attach(&vfs, &catalog);
        // Scroll through the whole grid a screenful (48 tiles) at a time.
        for first in (0..count).step_by(48) {
            let shown = first..(first + 48).min(count);
            for entry in shown.clone() {
                icons.icon(entry);
            }
            settle(&mut icons);
            for entry in shown {
                let drawn = icons.icon(entry);
                if entry % 2 == 0 {
                    assert!(drawn.is_some(), "entry {entry} has no icon");
                } else {
                    assert!(drawn.is_none() && icons.failed(entry), "entry {entry}");
                }
            }
            icons.frame += 1;
        }
        // Every texture is a model cell: never the HUD's cell 0 or past the range.
        for cell in 0..MODEL_ICONS {
            let texture = ModelIcons::texture(cell).0 as usize;
            assert!((1..=MODEL_ICONS).contains(&texture));
        }
    }

    #[test]
    fn tiles_on_screen_keep_their_cells() {
        let count = MODEL_ICONS * 3;
        let (vfs, catalog) = (vfs(count), catalog(count));
        let mut icons = ModelIcons::new();
        icons.attach(&vfs, &catalog);
        // Fill every cell, then ask for more while the first ten stay drawn.
        for entry in (0..MODEL_ICONS * 2).step_by(2) {
            icons.icon(entry);
        }
        settle(&mut icons);
        for _ in 0..3 {
            for entry in (0..20).step_by(2) {
                assert!(icons.icon(entry).is_some());
            }
            icons.frame += 1;
        }
        for entry in (0..20).step_by(2) {
            icons.icon(entry);
        }
        for entry in (MODEL_ICONS * 2..count).step_by(2) {
            icons.icon(entry);
        }
        for entry in (0..20).step_by(2) {
            assert!(icons.by_entry.contains_key(&entry), "entry {entry} lost");
        }
    }

    #[test]
    fn another_atlas_loads_the_icons_again() {
        let (vfs, catalog) = (vfs(8), catalog(8));
        let mut icons = ModelIcons::new();
        icons.attach(&vfs, &catalog);
        icons.icon(0);
        icons.icon(1);
        settle(&mut icons);
        assert!(icons.icon(0).is_some() && icons.failed(1));
        // A new world's atlas has none of the old one's pictures.
        icons.forget_uploads();
        assert!(icons.icon(0).is_none(), "an empty cell must not be drawn");
        assert_eq!(icons.jobs.len(), 1, "the icon is decoded again");
        settle(&mut icons);
        assert!(icons.icon(0).is_some());
        assert!(
            icons.icon(1).is_none() && icons.jobs.is_empty(),
            "a bad file stays failed"
        );
    }

    #[test]
    fn another_catalogue_empties_the_cache() {
        let (vfs, first) = (vfs(8), catalog(8));
        let mut icons = ModelIcons::new();
        icons.attach(&vfs, &first);
        icons.icon(0);
        settle(&mut icons);
        assert!(icons.icon(0).is_some());
        icons.attach(&vfs, &first);
        assert!(
            icons.icon(0).is_some(),
            "the same catalogue keeps its icons"
        );
        icons.attach(&vfs, &catalog(8));
        assert!(icons.icon(0).is_none());
        assert!(icons.by_entry.len() == 1 && icons.jobs.len() == 1);
    }
}
