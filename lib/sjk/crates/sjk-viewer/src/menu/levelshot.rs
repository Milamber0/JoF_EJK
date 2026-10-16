//! Map previews: `levelshots/<map>.jpg` (or `.tga`/`.png`) decoded on a
//! worker thread at the image's own resolution, with its mip chain, and
//! cached per map. The draw path only asks [`Levelshots::preview`]; decoding
//! and mip generation never run on it, and the levelshot texture is written
//! only when the wanted map changes.

use sjk_vfs::VirtualFileSystem;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

/// Decoded previews kept before the cache starts over.
const CACHE_LIMIT: usize = 32;
/// Decoded bytes kept before the cache starts over: about five 2048x1024 HD
/// levelshots with their mips, or every retail 512x512 one.
const CACHE_BYTES: usize = 64 << 20;
/// Longest edge kept; larger images are reduced to it before upload.
pub(crate) const MAX_EDGE: u32 = 4_096;
/// Extensions tried after `levelshots/<map>`, in the stock renderer's order.
const EXTENSIONS: [&str; 3] = ["jpg", "tga", "png"];

/// What the preview area shows for a map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Preview {
    /// The map's levelshot is in the levelshot texture.
    Image,
    /// Still decoding.
    Loading,
    /// The map ships no levelshot (or it did not decode).
    Missing,
}

/// One decoded levelshot: RGBA mip levels from the image's own size down to
/// one texel on its short side.
pub(crate) struct LevelshotImage {
    /// Width and height of level 0.
    pub(crate) size: [u32; 2],
    /// Tightly packed RGBA rows of each level, largest first.
    pub(crate) levels: Vec<Vec<u8>>,
}

impl LevelshotImage {
    /// Build the mip chain of `image`, reduced to [`MAX_EDGE`] first if larger.
    pub(crate) fn from_rgba(image: image::RgbaImage) -> Self {
        let (width, height) = image.dimensions();
        let longest = width.max(height).max(1);
        let image = if longest > MAX_EDGE {
            let scale = MAX_EDGE as f32 / longest as f32;
            image::imageops::resize(
                &image,
                ((width as f32 * scale).round() as u32).max(1),
                ((height as f32 * scale).round() as u32).max(1),
                image::imageops::FilterType::Triangle,
            )
        } else {
            image
        };
        let size = [image.width(), image.height()];
        let count = size[0].min(size[1]).max(1).ilog2() + 1;
        let mut levels = Vec::with_capacity(count as usize);
        let mut level = image;
        for index in 0..count {
            if index > 0 {
                let (w, h) = level.dimensions();
                level = image::imageops::resize(
                    &level,
                    (w / 2).max(1),
                    (h / 2).max(1),
                    image::imageops::FilterType::Triangle,
                );
            }
            levels.push(level.as_raw().clone());
        }
        Self { size, levels }
    }

    /// Bytes held by every level.
    pub(crate) fn bytes(&self) -> usize {
        self.levels.iter().map(Vec::len).sum()
    }
}

type Decoded = Option<Arc<LevelshotImage>>;

/// Per-map levelshot cache feeding the one levelshot texture.
pub(crate) struct Levelshots {
    vfs: Option<Arc<VirtualFileSystem>>,
    requests: Option<Sender<String>>,
    results: Option<Receiver<(String, Decoded)>>,
    cache: HashMap<String, Decoded>,
    /// Bytes of the decoded images in `cache`.
    cached_bytes: usize,
    /// The map whose preview is wanted now.
    wanted: String,
    /// The map the texture (or the placeholder) currently stands for.
    shown: String,
    shown_image: bool,
    /// The size of the image in the texture, while one is.
    shown_size: [u32; 2],
}

impl Levelshots {
    pub(crate) fn new() -> Self {
        Self {
            vfs: None,
            requests: None,
            results: None,
            cache: HashMap::with_capacity(CACHE_LIMIT),
            cached_bytes: 0,
            wanted: String::with_capacity(64),
            shown: String::with_capacity(64),
            shown_image: false,
            shown_size: [0; 2],
        }
    }

    /// Read levelshots from `vfs` from now on; forgets everything decoded.
    pub(crate) fn attach_vfs(&mut self, vfs: Arc<VirtualFileSystem>) {
        self.vfs = Some(vfs);
        self.requests = None;
        self.results = None;
        self.cache.clear();
        self.cached_bytes = 0;
        self.shown.clear();
        self.shown_image = false;
        let wanted = std::mem::take(&mut self.wanted);
        self.want(&wanted);
    }

    /// Forget what the texture holds (it is another world's): the wanted
    /// preview is uploaded again from the cache.
    pub(crate) fn forget_upload(&mut self) {
        self.shown.clear();
        self.shown_image = false;
    }

    /// Ask for `map`'s preview (`mp/ffa3`); cheap when it is already wanted.
    pub(crate) fn want(&mut self, map: &str) {
        if self.wanted == map {
            return;
        }
        self.wanted.clear();
        self.wanted.push_str(map);
        if map.is_empty() || self.cache.contains_key(map) {
            return;
        }
        let Some(vfs) = &self.vfs else { return };
        if self.requests.is_none() {
            let (requests, results) = spawn_worker(Arc::clone(vfs));
            self.requests = Some(requests);
            self.results = Some(results);
        }
        if let Some(requests) = &self.requests {
            let _ = requests.send(map.to_owned());
        }
    }

    /// Collect finished decodes and, when the wanted map's preview is ready
    /// and not yet in the texture, hand it to `upload`.
    pub(crate) fn service(&mut self, mut upload: impl FnMut(&LevelshotImage)) {
        let results = self.results.take();
        if let Some(results) = &results {
            while let Ok((map, decoded)) = results.try_recv() {
                self.insert(map, decoded);
            }
        }
        self.results = results;
        if self.shown == self.wanted {
            return;
        }
        let Some(decoded) = self.cache.get(&self.wanted) else {
            return;
        };
        self.shown_image = match decoded {
            Some(image) => {
                upload(image);
                self.shown_size = image.size;
                true
            }
            None => false,
        };
        self.shown.clear();
        self.shown.push_str(&self.wanted);
    }

    fn insert(&mut self, map: String, decoded: Decoded) {
        let bytes = decoded.as_ref().map_or(0, |image| image.bytes());
        if self.cache.len() >= CACHE_LIMIT || self.cached_bytes + bytes > CACHE_BYTES {
            // Start over, but never drop the one still wanted.
            let wanted = &self.wanted;
            self.cache.retain(|name, _| name == wanted);
            self.cached_bytes = self
                .cache
                .values()
                .flatten()
                .map(|image| image.bytes())
                .sum();
        }
        if let Some(previous) = self.cache.insert(map, decoded) {
            self.cached_bytes -= previous.map_or(0, |image| image.bytes());
        }
        self.cached_bytes += bytes;
    }

    /// What to draw for `map` this frame.
    pub(crate) fn preview(&self, map: &str) -> Preview {
        if !self.shown.is_empty() && self.shown == map {
            return if self.shown_image {
                Preview::Image
            } else {
                Preview::Missing
            };
        }
        if self.vfs.is_none() || matches!(self.cache.get(map), Some(None)) {
            return Preview::Missing;
        }
        Preview::Loading
    }

    /// The pixel size of `map`'s levelshot while it is the one in the texture.
    pub(crate) fn size(&self, map: &str) -> Option<[u32; 2]> {
        (self.shown_image && !self.shown.is_empty() && self.shown == map).then_some(self.shown_size)
    }
}

/// Start the decoder thread; it always works on the newest request.
fn spawn_worker(vfs: Arc<VirtualFileSystem>) -> (Sender<String>, Receiver<(String, Decoded)>) {
    let (request_tx, request_rx) = mpsc::channel::<String>();
    let (result_tx, result_rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("sjk-levelshots".to_owned())
        .spawn(move || {
            while let Ok(mut map) = request_rx.recv() {
                // Scrolling queues many maps; only the latest is still wanted.
                while let Ok(newer) = request_rx.try_recv() {
                    map = newer;
                }
                let decoded = decode_levelshot(&vfs, &map).map(Arc::new);
                if result_tx.send((map, decoded)).is_err() {
                    return;
                }
            }
        });
    if let Err(error) = spawned {
        crate::log::progress(format_args!("levelshot decoder not started: {error}"));
    }
    (request_tx, result_rx)
}

/// Texture coordinates (top left, top right, bottom right, bottom left)
/// showing a levelshot of `size` over a rectangle `aspect` times as wide as
/// tall without stretching it: its middle, cut at its long sides. A square
/// levelshot is retail's, a 4:3 picture stored square.
pub(crate) fn cover_uv(size: [u32; 2], aspect: f32) -> [[f32; 2]; 4] {
    let [width, height] = size.map(|side| side.max(1) as f32);
    let image = if size[0] == size[1] {
        4.0 / 3.0
    } else {
        width / height
    };
    let (u, v) = if image > aspect {
        ((1.0 - aspect / image) * 0.5, 0.0)
    } else {
        (0.0, (1.0 - image / aspect) * 0.5)
    };
    [[u, v], [1.0 - u, v], [1.0 - u, 1.0 - v], [u, 1.0 - v]]
}

/// Where and how a levelshot of `size` fills `window` without stretching:
/// a picture as wide as the window or narrower covers it, cut at its top and
/// bottom ([`cover_uv`]); a wider one (the HD packs' 2:1 shots, which set the
/// map's title against their left edge) keeps its whole width, centred with
/// bands above and below, so nothing at its sides is lost.
pub(crate) fn screen_fit(size: [u32; 2], window: sjk_ui::Rect) -> (sjk_ui::Rect, [[f32; 2]; 4]) {
    let aspect = window.width / window.height.max(1.0);
    let [width, height] = size.map(|side| side.max(1) as f32);
    let image = if size[0] == size[1] {
        4.0 / 3.0
    } else {
        width / height
    };
    if image <= aspect {
        return (window, cover_uv(size, aspect));
    }
    let fitted = window.width / image;
    let rect = sjk_ui::Rect::new(
        window.x,
        window.y + (window.height - fitted) * 0.5,
        window.width,
        fitted,
    );
    (rect, [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
}

/// `map`'s levelshot at its own resolution, if it ships one.
pub(crate) fn decode_levelshot(vfs: &VirtualFileSystem, map: &str) -> Option<LevelshotImage> {
    EXTENSIONS.iter().find_map(|extension| {
        let path = format!("levelshots/{map}.{extension}");
        let asset = vfs.read(&path).ok().flatten()?;
        let image = crate::gpu_texture::decode_image(&asset.bytes, &path).ok()?;
        Some(LevelshotImage::from_rgba(image.into_rgba8()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32) -> image::RgbaImage {
        image::RgbaImage::from_pixel(width, height, image::Rgba([10, 20, 30, 255]))
    }

    #[test]
    fn keeps_the_source_resolution_with_a_full_mip_chain() {
        let hd = LevelshotImage::from_rgba(image(2048, 1024));
        assert_eq!(hd.size, [2048, 1024]);
        // 1024 on the short side: 11 levels, the last 2x1.
        assert_eq!(hd.levels.len(), 11);
        assert_eq!(hd.levels[0].len(), 2048 * 1024 * 4);
        assert_eq!(hd.levels[1].len(), 1024 * 512 * 4);
        assert_eq!(hd.levels[10].len(), 2 * 4);
        let retail = LevelshotImage::from_rgba(image(512, 512));
        assert_eq!(retail.size, [512, 512]);
        assert_eq!(retail.levels.len(), 10);
    }

    #[test]
    fn reduces_only_images_beyond_the_edge_limit() {
        let huge = LevelshotImage::from_rgba(image(8192, 4096));
        assert_eq!(huge.size, [MAX_EDGE, MAX_EDGE / 2]);
        let odd = LevelshotImage::from_rgba(image(1, 1));
        assert_eq!((odd.size, odd.levels.len()), ([1, 1], 1));
    }

    #[test]
    fn cache_starts_over_past_its_byte_budget_but_keeps_the_wanted_map() {
        let mut shots = Levelshots::new();
        shots.wanted.push_str("mp/keep");
        let hd = || Some(Arc::new(LevelshotImage::from_rgba(image(2048, 1024))));
        shots.insert("mp/keep".to_owned(), hd());
        for index in 0..8 {
            shots.insert(format!("mp/other{index}"), hd());
            assert!(shots.cached_bytes <= CACHE_BYTES);
        }
        assert!(shots.cache.contains_key("mp/keep"));
        let counted: usize = shots.cache.values().flatten().map(|i| i.bytes()).sum();
        assert_eq!(counted, shots.cached_bytes);
    }

    #[test]
    fn a_levelshot_covers_a_wide_screen_without_stretching() {
        // A retail square levelshot (a 4:3 picture) over a 16:9 screen: its
        // full width, the middle three quarters of its height.
        let uv = cover_uv([512, 512], 16.0 / 9.0);
        assert_eq!((uv[0][0], uv[1][0]), (0.0, 1.0));
        assert!((uv[0][1] - 0.125).abs() < 1e-5 && (uv[2][1] - 0.875).abs() < 1e-5);
        // A 2:1 HD one over 16:9 loses a little at its sides; over 4:3 more.
        let hd = cover_uv([2048, 1024], 16.0 / 9.0);
        assert!((hd[0][0] - 1.0 / 18.0).abs() < 1e-5 && hd[0][1] == 0.0);
        let narrow = cover_uv([2048, 1024], 4.0 / 3.0);
        assert!((narrow[0][0] - 1.0 / 6.0).abs() < 1e-5);
        // A 2:1 frame (the browser's) shows a 2:1 shot whole, a square one's
        // middle two thirds.
        assert_eq!(
            cover_uv([2048, 1024], 2.0),
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
        );
        let square = cover_uv([512, 512], 2.0);
        assert!((square[0][1] - 1.0 / 6.0).abs() < 1e-5 && square[0][0] == 0.0);
    }

    #[test]
    fn a_wider_levelshot_keeps_its_sides_on_screen() {
        let window = sjk_ui::Rect::new(0.0, 0.0, 1920.0, 1080.0);
        // A 2:1 HD shot: the whole picture, 960 tall, centred.
        let (rect, uv) = screen_fit([2048, 1024], window);
        assert_eq!(
            (rect.x, rect.y, rect.width, rect.height),
            (0.0, 60.0, 1920.0, 960.0)
        );
        assert_eq!(uv, [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        // A retail one covers the screen, cut at its top and bottom.
        let (rect, uv) = screen_fit([512, 512], window);
        assert_eq!(rect, window);
        assert!((uv[0][1] - 0.125).abs() < 1e-5);
    }

    #[test]
    fn service_uploads_the_wanted_image_once() {
        let mut shots = Levelshots::new();
        shots.wanted.push_str("mp/ffa3");
        shots.insert(
            "mp/ffa3".to_owned(),
            Some(Arc::new(LevelshotImage::from_rgba(image(512, 512)))),
        );
        let mut uploads = Vec::new();
        shots.service(|image| uploads.push(image.size));
        shots.service(|image| uploads.push(image.size));
        assert_eq!(uploads, [[512, 512]]);
        assert_eq!(shots.preview("mp/ffa3"), Preview::Image);
        assert_eq!(shots.size("mp/ffa3"), Some([512, 512]));
        assert_eq!(shots.size("mp/duel6"), None);
    }
}
