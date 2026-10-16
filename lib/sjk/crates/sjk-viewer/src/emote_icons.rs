//! The emotes' pictures: an emote file's `icon` (`emotes.rs`), decoded when the
//! catalogue is loaded and put into the UI atlas's emote cells
//! ([`crate::ui_renderer::emote_icon`]), where the quick wheel and its editor show
//! it for `sjkemote <id>`. A picture is fitted into the square cell whole, on a
//! transparent ground; one that does not decode is reported to the log, and its
//! emote keeps the plain emote icon. The first [`CELLS`] pictures of the catalogue
//! are shown.

use crate::ui_renderer::{EMOTE_ICON_CELLS, ICON_SIZE, ShapeRenderer, emote_icon};
use sjk_ui::TextureId;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// Most pictures shown.
pub(crate) const CELLS: usize = EMOTE_ICON_CELLS;

#[derive(Default)]
struct Icons {
    /// The (id, picture path) pairs last put into the atlas, in cell order.
    served: Vec<(String, String)>,
    /// The cell of each emote whose picture is in the atlas.
    cells: HashMap<String, usize>,
}

static ICONS: LazyLock<Mutex<Icons>> = LazyLock::new(Mutex::default);

fn lock() -> std::sync::MutexGuard<'static, Icons> {
    ICONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The picture of emote `id`, when it has one in the atlas.
pub(crate) fn texture(id: &str) -> Option<TextureId> {
    lock().cells.get(id).copied().map(emote_icon)
}

/// Whether `texture` is one of the emote picture cells.
pub(crate) fn is_emote_texture(texture: TextureId) -> bool {
    (emote_icon(0).0..emote_icon(0).0 + CELLS as u32).contains(&texture.0)
}

/// The picture of the emote a console `command` plays (`sjkemote <id>`).
pub(crate) fn for_command(command: &str) -> Option<TextureId> {
    let mut words = command.split_whitespace();
    let verb = words.next()?;
    let id = words.next()?;
    if !verb.eq_ignore_ascii_case(crate::emotes::COMMAND) || words.next().is_some() {
        return None;
    }
    texture(id)
}

/// Put the loaded catalogue's pictures into the atlas; nothing happens while it is
/// the catalogue already served.
pub(crate) fn service(
    vfs: &sjk_vfs::VirtualFileSystem,
    renderer: &ShapeRenderer,
    queue: &crate::frame_queue::FrameQueue,
) {
    let wanted: Vec<(String, String)> = {
        let state = crate::emotes::lock();
        let Some(catalogue) = state.catalogue.as_ref() else {
            return;
        };
        catalogue
            .iter()
            .filter(|def| !def.icon.is_empty())
            .take(CELLS)
            .map(|def| (def.id.clone(), def.icon.clone()))
            .collect()
    };
    let mut icons = lock();
    if icons.served == wanted {
        return;
    }
    icons.cells.clear();
    for (cell, (id, path)) in wanted.iter().enumerate() {
        match decode(vfs, path) {
            Ok(rgba) => {
                renderer.upload_icon(queue, emote_icon(cell), &rgba);
                icons.cells.insert(id.clone(), cell);
            }
            Err(why) => crate::log::progress(format_args!("emotes: {id}: icon {path}: {why}")),
        }
    }
    icons.served = wanted;
}

/// The picture at `path`, fitted into one atlas cell.
fn decode(vfs: &sjk_vfs::VirtualFileSystem, path: &str) -> Result<Vec<u8>, String> {
    let asset = vfs
        .read(path)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "missing".to_owned())?;
    let image = crate::decode_image(&asset.bytes, path)
        .map_err(|error| error.to_string())?
        .into_rgba8();
    Ok(backed(&fit(&image)).into_raw())
}

/// `picture` (a fitted cell) a little smaller on a blue disc, so the white
/// silhouettes emote pictures often are show on any ground.
fn backed(picture: &image::RgbaImage) -> image::RgbaImage {
    let size = ICON_SIZE as f32;
    let middle = size / 2.0;
    let radius = middle - 1.0;
    let mut cell = image::RgbaImage::from_fn(ICON_SIZE, ICON_SIZE, |x, y| {
        let distance =
            ((x as f32 + 0.5 - middle).powi(2) + (y as f32 + 0.5 - middle).powi(2)).sqrt();
        let edge = (radius - distance + 0.5).clamp(0.0, 1.0);
        let t = (distance / radius).min(1.0);
        let mix = |inner: f32, outer: f32| (inner + (outer - inner) * t).round() as u8;
        image::Rgba([
            mix(42.0, 16.0),
            mix(140.0, 60.0),
            mix(240.0, 140.0),
            (edge * 255.0) as u8,
        ])
    });
    let inner = (size * 0.82) as u32;
    let small =
        image::imageops::resize(picture, inner, inner, image::imageops::FilterType::Triangle);
    let offset = i64::from((ICON_SIZE - inner) / 2);
    image::imageops::overlay(&mut cell, &small, offset, offset);
    cell
}

/// `image` scaled to fit an [`ICON_SIZE`] square whole, centred on transparency.
fn fit(image: &image::RgbaImage) -> image::RgbaImage {
    let (width, height) = image.dimensions();
    let longest = width.max(height).max(1) as f32;
    let scaled = |side: u32| ((side as f32 * ICON_SIZE as f32 / longest).round() as u32).max(1);
    let (fitted_width, fitted_height) = (scaled(width), scaled(height));
    let fitted = image::imageops::resize(
        image,
        fitted_width,
        fitted_height,
        image::imageops::FilterType::Triangle,
    );
    let mut cell = image::RgbaImage::new(ICON_SIZE, ICON_SIZE);
    image::imageops::overlay(
        &mut cell,
        &fitted,
        i64::from((ICON_SIZE - fitted_width) / 2),
        i64::from((ICON_SIZE - fitted_height) / 2),
    );
    cell
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_fills_the_cell_whole_and_keeps_its_shape() {
        let wide = image::RgbaImage::from_pixel(256, 128, image::Rgba([255, 0, 0, 255]));
        let cell = fit(&wide);
        assert_eq!(cell.dimensions(), (ICON_SIZE, ICON_SIZE));
        // Half as tall as wide: a band across the middle, clear above and below.
        assert_eq!(cell.get_pixel(64, 64)[3], 255);
        assert_eq!(cell.get_pixel(64, 10)[3], 0);
        assert_eq!(cell.get_pixel(64, 117)[3], 0);
        assert_eq!(cell.get_pixel(0, 64)[3], 255);
    }

    #[test]
    fn a_picture_stands_on_a_blue_disc() {
        let white = image::RgbaImage::from_pixel(128, 128, image::Rgba([255, 255, 255, 255]));
        let cell = backed(&image::RgbaImage::new(128, 128));
        assert_eq!(cell.get_pixel(0, 0)[3], 0);
        let middle = cell.get_pixel(64, 64);
        assert!(middle[3] == 255 && middle[2] > middle[0], "{middle:?}");
        assert_eq!(backed(&white).get_pixel(64, 64)[0], 255);
    }

    #[test]
    fn only_a_plain_sjkemote_command_names_an_emote() {
        lock().cells.insert("kane_dance".to_owned(), 3);
        assert_eq!(for_command("sjkemote kane_dance"), Some(emote_icon(3)));
        assert_eq!(for_command("SJKEMOTE  kane_dance "), Some(emote_icon(3)));
        assert_eq!(for_command("sjkemote kane_dance; say hi"), None);
        assert_eq!(for_command("sjkemote other"), None);
        assert_eq!(for_command("say kane_dance"), None);
        lock().cells.remove("kane_dance");
    }
}
