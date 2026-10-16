//! A picture of a game-data HUD for the settings' HUD picker. The HUD's menus
//! are laid out as `CG_DrawHUD` draws them for a sample player (health 74,
//! armor 46, Force 63, the saber in medium style) and composited on the CPU
//! at 1280x720 over a dimmed levelshot from the game's own archives (packs
//! replacing levelshots often caption them), with a crosshair to show where
//! the screen's centre is. The HUD's text (the score line, or the whole text
//! HUD) is returned beside the picture for the menu to draw in its own font.
//!
//! [`plan`] resolves the files on the calling thread (menus and shader names
//! only); [`render`] decodes the pictures and composites them, which is the
//! slow part, on a worker.

use super::choices::{HudChoice, is_retail_archive};
use super::frame::{Frame, Readout, Timers};
use super::layout::{Layout, Side};
use super::{DIGITS, Source, gpu::to_pixels, mount_file_name, pack_view, read_menus, source};
use image::{Rgba, RgbaImage};
use sjk_shader::{ShaderCatalog, StageBlend};
use sjk_vfs::VirtualFileSystem;

/// Size of the picture, a 16:9 screen.
pub(crate) const SIZE: [u32; 2] = [1_280, 720];
/// Levelshots of the retail archives tried as the backdrop, in order.
const BACKDROPS: [&str; 4] = [
    "levelshots/mp/ffa3.jpg",
    "levelshots/mp/ffa3.tga",
    "levelshots/mp/ffa5.jpg",
    "levelshots/mp/duel1.jpg",
];
/// How much of the backdrop's brightness is kept, so the HUD stands out.
const BACKDROP_LEVEL: f32 = 0.55;

/// What [`render`] needs, resolved from the files.
pub(crate) struct Plan {
    /// The files as the HUD reads them (its pack's view).
    vfs: VirtualFileSystem,
    kind: Kind,
    /// The backdrop levelshot's path and bytes.
    backdrop: Option<(&'static str, Vec<u8>)>,
}

enum Kind {
    /// The text-only HUD: nothing but text runs.
    Text,
    Menus {
        layout: Box<Layout>,
        /// Image path and additive blending of each picture of the layout,
        /// then the digits; `None` when the shader names no image.
        pictures: Vec<Option<(String, bool)>>,
        digits: u16,
    },
}

/// One run of the HUD's text, in the picture's pixels.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PreviewText {
    pub(crate) text: String,
    pub(crate) x: f32,
    /// Top of the line.
    pub(crate) y: f32,
    /// Line height.
    pub(crate) size: f32,
    pub(crate) color: [f32; 4],
    /// Centred on `x` rather than starting there.
    pub(crate) centred: bool,
}

/// A rendered preview.
pub(crate) struct Preview {
    pub(crate) image: RgbaImage,
    pub(crate) texts: Vec<PreviewText>,
}

/// Resolve `choice`'s menus and picture paths, or `None` when it is not a
/// game-data HUD or its files describe no HUD.
pub(crate) fn plan(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    choice: &HudChoice,
) -> Option<Plan> {
    if !choice.has_preview() {
        return None;
    }
    let view = pack_view(vfs, &choice.pack).unwrap_or_else(|| vfs.clone());
    let backdrop = retail_levelshot(vfs);
    let kind = match source(&choice.files) {
        Source::Text => Kind::Text,
        Source::Menus(list) => {
            let mut layout = Layout::resolve(&read_menus(&view, list)?);
            if !layout.is_usable() {
                return None;
            }
            let digits = layout.pictures.len() as u16;
            layout
                .pictures
                .extend(DIGITS.iter().map(|name| (*name).to_owned()));
            let pictures = layout
                .pictures
                .iter()
                .map(|name| {
                    let path = shaders.resolve_image(&view, name).ok().flatten()?;
                    let additive = shaders
                        .get(name)
                        .and_then(|definition| definition.stages.first())
                        .is_some_and(|stage| stage.blend == StageBlend::Add);
                    Some((path.as_str().to_owned(), additive))
                })
                .collect();
            Kind::Menus {
                layout: Box::new(layout),
                pictures,
                digits,
            }
        }
    };
    Some(Plan {
        vfs: view,
        kind,
        backdrop,
    })
}

/// The values the preview shows: a wounded saberist part-way into a match.
fn sample() -> Readout {
    let mut ammo = [0; 10];
    ammo[2] = 140;
    Readout {
        health: 74,
        max_health: 100,
        armor: 46,
        force: 63,
        weapon: 3,
        ammo,
        saber_style: 2,
        score: 12,
        time: 10_000,
        ..Readout::default()
    }
}

/// Draw the planned HUD.
pub(crate) fn render(plan: &Plan) -> Preview {
    let [width, height] = SIZE;
    let viewport = [width as f32, height as f32];
    let mut canvas = backdrop(plan.backdrop.as_ref());
    crosshair(&mut canvas);
    let mut frame = Frame::default();
    let mut timers = Timers::default();
    let readout = sample();
    match &plan.kind {
        Kind::Text => frame.simple_hud(&readout, &mut timers),
        Kind::Menus {
            layout,
            pictures,
            digits,
        } => {
            frame.menu_hud(layout, &readout, &mut timers, *digits, "Score");
            let mut decoded: Vec<Option<Option<RgbaImage>>> =
                (0..pictures.len()).map(|_| None).collect();
            for picture in &frame.pictures[..frame.picture_count] {
                let index = usize::from(picture.picture);
                let Some(Some((path, additive))) = pictures.get(index) else {
                    continue;
                };
                let image = decoded[index].get_or_insert_with(|| decode(&plan.vfs, path));
                if let Some(image) = image {
                    let rect = to_pixels(picture.rect, picture.side, viewport, 1.0);
                    draw(&mut canvas, image, rect, picture.color, *additive);
                }
            }
        }
    }
    let texts = frame.texts[..frame.text_count]
        .iter()
        .map(|run| {
            let side = run.side.unwrap_or(Side::Left);
            let [x, y, _, size] = to_pixels([run.x, run.y, 0.0, run.size], side, viewport, 1.0);
            PreviewText {
                text: run.text.clone(),
                x,
                y,
                size,
                color: run.color,
                centred: run.centred,
            }
        })
        .collect();
    Preview {
        image: canvas,
        texts,
    }
}

fn decode(vfs: &VirtualFileSystem, path: &str) -> Option<RgbaImage> {
    let asset = vfs.read(path).ok().flatten()?;
    Some(crate::decode_image(&asset.bytes, path).ok()?.into_rgba8())
}

/// The first of [`BACKDROPS`] the game's own archives hold, read from them.
fn retail_levelshot(vfs: &VirtualFileSystem) -> Option<(&'static str, Vec<u8>)> {
    let retail: Vec<_> = vfs
        .mounts()
        .filter(|mount| is_retail_archive(mount_file_name(&mount.name)))
        .map(|mount| mount.id)
        .collect();
    BACKDROPS.into_iter().find_map(|path| {
        retail.iter().find_map(|mount| {
            let asset = vfs.read_from_mount(*mount, path).ok().flatten()?;
            Some((path, asset.bytes))
        })
    })
}

/// The levelshot covering the picture, dimmed; a dark gradient without one.
fn backdrop(levelshot: Option<&(&'static str, Vec<u8>)>) -> RgbaImage {
    let [width, height] = SIZE;
    let decoded = levelshot.and_then(|(path, bytes)| crate::decode_image(bytes, path).ok());
    if let Some(image) = decoded.map(image::DynamicImage::into_rgba8) {
        // Cover: scale to fill, then cut the middle.
        let scale =
            (width as f32 / image.width() as f32).max(height as f32 / image.height() as f32);
        let scaled = image::imageops::resize(
            &image,
            ((image.width() as f32 * scale).ceil() as u32).max(width),
            ((image.height() as f32 * scale).ceil() as u32).max(height),
            image::imageops::FilterType::Triangle,
        );
        let x = (scaled.width() - width) / 2;
        let y = (scaled.height() - height) / 2;
        let mut cut = image::imageops::crop_imm(&scaled, x, y, width, height).to_image();
        for pixel in cut.pixels_mut() {
            for channel in &mut pixel.0[..3] {
                *channel = (f32::from(*channel) * BACKDROP_LEVEL).round() as u8;
            }
            pixel.0[3] = 255;
        }
        return cut;
    }
    RgbaImage::from_fn(width, height, |_, y| {
        let t = y as f32 / height as f32;
        let channel = |top: f32, bottom: f32| (top + (bottom - top) * t).round() as u8;
        Rgba([
            channel(34.0, 10.0),
            channel(40.0, 12.0),
            channel(52.0, 18.0),
            255,
        ])
    })
}

/// A small light cross at the centre, where a HUD built around the
/// crosshair centres itself.
fn crosshair(canvas: &mut RgbaImage) {
    let [width, height] = SIZE;
    let (cx, cy) = (width as i32 / 2, height as i32 / 2);
    for offset in 4..12 {
        for (x, y) in [
            (cx + offset, cy),
            (cx - offset, cy),
            (cx, cy + offset),
            (cx, cy - offset),
        ] {
            canvas.put_pixel(x as u32, y as u32, Rgba([230, 230, 230, 255]));
        }
    }
}

/// Draw `image` into `rect` (pixels; a negative width or height mirrors it)
/// tinted by `color`, alpha-blended or added as the GPU HUD blends it.
fn draw(
    canvas: &mut RgbaImage,
    image: &RgbaImage,
    rect: [f32; 4],
    color: [f32; 4],
    additive: bool,
) {
    let [x, y, w, h] = rect;
    let (target_width, target_height) = (w.abs().round() as u32, h.abs().round() as u32);
    if target_width == 0 || target_height == 0 || image.width() == 0 || image.height() == 0 {
        return;
    }
    let resized;
    let scaled = if image.dimensions() == (target_width, target_height) {
        image
    } else {
        resized = image::imageops::resize(
            image,
            target_width,
            target_height,
            image::imageops::FilterType::Triangle,
        );
        &resized
    };
    let left = (if w < 0.0 { x + w } else { x }).round() as i64;
    let top = (if h < 0.0 { y + h } else { y }).round() as i64;
    for row in 0..target_height {
        let canvas_y = top + i64::from(row);
        if canvas_y < 0 || canvas_y >= i64::from(canvas.height()) {
            continue;
        }
        let source_y = if h < 0.0 {
            target_height - 1 - row
        } else {
            row
        };
        for column in 0..target_width {
            let canvas_x = left + i64::from(column);
            if canvas_x < 0 || canvas_x >= i64::from(canvas.width()) {
                continue;
            }
            let source_x = if w < 0.0 {
                target_width - 1 - column
            } else {
                column
            };
            let source = scaled.get_pixel(source_x, source_y).0;
            let alpha = f32::from(source[3]) / 255.0 * color[3];
            let target = canvas.get_pixel_mut(canvas_x as u32, canvas_y as u32);
            for channel in 0..3 {
                let value = f32::from(source[channel]) / 255.0 * color[channel];
                let below = f32::from(target.0[channel]) / 255.0;
                let blended = if additive {
                    below + value
                } else {
                    value * alpha + below * (1.0 - alpha)
                };
                target.0[channel] = (blended.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::choices::{list, tests::installed};
    use super::*;

    fn png(color: [u8; 4]) -> Vec<u8> {
        let mut bytes = Vec::new();
        RgbaImage::from_pixel(16, 16, Rgba(color))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    /// The installed HUDs of [`installed`] with their frame pictures: the
    /// retail frames red (left) and blue (right), the radial pack's green.
    fn files() -> VirtualFileSystem {
        let mut vfs = installed();
        vfs.mount_memory(
            "pictures",
            [
                ("gfx/hud/hudleft.png", png([255, 0, 0, 255])),
                ("gfx/hud/hudright.png", png([0, 0, 255, 255])),
                ("gfx/hud/radial.png", png([0, 255, 0, 128])),
            ],
        )
        .unwrap();
        vfs
    }

    fn pixel(preview: &Preview, x: u32, y: u32) -> [u8; 4] {
        preview.image.get_pixel(x, y).0
    }

    #[test]
    fn each_pack_is_drawn_from_its_own_files() {
        let vfs = files();
        let shaders = ShaderCatalog::default();
        let choices = list(&vfs);
        let retail = render(&plan(&vfs, &shaders, &choices[0]).unwrap());
        assert_eq!(retail.image.dimensions(), (SIZE[0], SIZE[1]));
        // 720 lines: 1.5 px per unit, so the 112-unit frames are 168 px in
        // the bottom corners, the right one kept to the right edge.
        assert_eq!(pixel(&retail, 80, 640), [255, 0, 0, 255]);
        assert_eq!(pixel(&retail, 1_200, 640), [0, 0, 255, 255]);
        // Outside the frames: the dark backdrop, the crosshair at the centre.
        assert!(
            pixel(&retail, 640, 300)[..3]
                .iter()
                .all(|channel| *channel < 60)
        );
        assert_eq!(pixel(&retail, 650, 360), [230, 230, 230, 255]);
        // The radial pack's half-transparent frames over the backdrop.
        let radial = render(&plan(&vfs, &shaders, &choices[2]).unwrap());
        let [red, green, blue, _] = pixel(&radial, 80, 640);
        assert!(green > 100 && red < 60 && blue < 60, "{red} {green} {blue}");
    }

    #[test]
    fn the_text_hud_is_text_only_and_sjk_layouts_have_no_preview() {
        let vfs = files();
        let shaders = ShaderCatalog::default();
        let choices = list(&vfs);
        let text = choices
            .iter()
            .find(|choice| choice.label == "Text only")
            .unwrap();
        let preview = render(&plan(&vfs, &shaders, text).unwrap());
        let strings: Vec<_> = preview.texts.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(strings, ["74", "46", "63", "MEDIUM"]);
        // Health at x 16 on the left; the style right of the centre.
        assert_eq!(preview.texts[0].x, 16.0 * 1.5);
        assert!(preview.texts[3].x > 640.0);
        for own in choices.iter().filter(|choice| !choice.has_preview()) {
            assert!(plan(&vfs, &shaders, own).is_none(), "{}", own.label);
        }
    }

    #[test]
    fn mirrored_pictures_flip() {
        let mut canvas = RgbaImage::from_pixel(4, 1, Rgba([0, 0, 0, 255]));
        let mut image = RgbaImage::from_pixel(4, 1, Rgba([0, 0, 0, 255]));
        image.put_pixel(0, 0, Rgba([255, 255, 255, 255]));
        draw(&mut canvas, &image, [4.0, 0.0, -4.0, 1.0], [1.0; 4], false);
        assert_eq!(canvas.get_pixel(3, 0).0[0], 255);
        assert_eq!(canvas.get_pixel(0, 0).0[0], 0);
    }
}
