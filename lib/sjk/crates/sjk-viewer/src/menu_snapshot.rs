//! Menu snapshots without a window or a GPU: a menu screen's draw list and
//! text drawn into a PNG on the CPU, with the player's retail menu art, to
//! look at a layout (classic+ pages, `docs/classic-plus.md`) before anyone
//! plays it. It never opens a window or touches the network.
//!
//! The snapshots are an ignored test because they read the local game
//! installation:
//!
//! ```sh
//! JKA_GAME_DATA="/path/to/GameData" cargo test --release -p sjk-viewer \
//!     menu_snapshot -- --ignored --nocapture
//! ```
//!
//! They are written to `target/menu-snapshots/` in the workspace at
//! 1440x1080 (the 640x480 canvas at 2.25 units per pixel). The drawing is an
//! approximation of the UI renderer: rounded rectangles anti-aliased over one
//! pixel, nearest-texel art without its motion and flicker, the Inter font only, and
//! only the atlas icons a screen's test supplies. Text is drawn over every shape,
//! as on screen. The in-game menus are drawn over a retail levelshot standing for
//! the match.

use crate::keybind_editor::{Category, KeybindEditor};
use crate::menu::art::{ArtPiece, ArtSet};
use crate::menu::classic::layout::{Entry, Page, Panel, Span};
use crate::menu::classic::panel::{Frame, PanelFrame};
use crate::settings::SettingsMenu;
use image::{Rgba, RgbaImage};
use sjk_ui::{DrawCommand, DrawList, Rect};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const VIEWPORT: [f32; 2] = [1440.0, 1080.0];
/// The emblem's layers by texture, decoded once for every snapshot.
static EMBLEM: std::sync::OnceLock<HashMap<u32, (RgbaImage, bool)>> = std::sync::OnceLock::new();

/// The retail menu art of the installation in `JKA_GAME_DATA`, decoded, and
/// the installation's files.
fn art() -> (ArtSet, Arc<sjk_vfs::VirtualFileSystem>) {
    let game = PathBuf::from(
        std::env::var_os("JKA_GAME_DATA").expect("set JKA_GAME_DATA to the GameData directory"),
    );
    let vfs = Arc::new(crate::assets::mount_game_data(&game).expect("mount the game data"));
    crate::menu::art::request(&vfs);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while crate::menu::art::decoded().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let decoded = crate::menu::art::decoded().expect("menu art decoded");
    let set = ArtPiece::ALL
        .into_iter()
        .filter(|piece| decoded.image(*piece).is_some())
        .fold(ArtSet::default(), |set, piece| set.with(piece));
    (set, vfs)
}

fn blend(image: &mut RgbaImage, x: i64, y: i64, color: [f32; 4]) {
    if x < 0 || y < 0 || x >= i64::from(image.width()) || y >= i64::from(image.height()) {
        return;
    }
    let pixel = image.get_pixel_mut(x as u32, y as u32);
    let alpha = color[3].clamp(0.0, 1.0);
    for (channel, source) in pixel.0.iter_mut().zip(color).take(3) {
        let below = f32::from(*channel) / 255.0;
        let value = source.clamp(0.0, 1.0) * alpha + below * (1.0 - alpha);
        *channel = (value * 255.0).round() as u8;
    }
}

/// Pixel bounds of `rect` inside `clip`.
fn span(rect: Rect, clip: Rect) -> (std::ops::Range<i64>, std::ops::Range<i64>) {
    let x0 = rect.x.max(clip.x).round() as i64;
    let y0 = rect.y.max(clip.y).round() as i64;
    let x1 = rect.right().min(clip.right()).round() as i64;
    let y1 = rect.bottom().min(clip.bottom()).round() as i64;
    (x0..x1, y0..y1)
}

/// `rect` filled with its corners rounded to `radius`, edges anti-aliased over a pixel.
fn fill_rounded(image: &mut RgbaImage, rect: Rect, radius: f32, clip: Rect, color: [f32; 4]) {
    shade_rounded(image, rect, radius, None, clip, color);
}

/// `rect` filled with its corners rounded to `radius`, or only its outline
/// `stroke` wide inside it, edges anti-aliased over a pixel.
fn shade_rounded(
    image: &mut RgbaImage,
    rect: Rect,
    radius: f32,
    stroke: Option<f32>,
    clip: Rect,
    color: [f32; 4],
) {
    let radius = radius.min(rect.width * 0.5).min(rect.height * 0.5);
    if radius < 0.5 {
        let Some(width) = stroke else {
            return fill(image, rect, clip, color);
        };
        for edge in [
            Rect::new(rect.x, rect.y, rect.width, width),
            Rect::new(rect.x, rect.bottom() - width, rect.width, width),
            Rect::new(rect.x, rect.y, width, rect.height),
            Rect::new(rect.right() - width, rect.y, width, rect.height),
        ] {
            fill(image, edge, clip, color);
        }
        return;
    }
    let grown = Rect::new(
        rect.x - 1.0,
        rect.y - 1.0,
        rect.width + 2.0,
        rect.height + 2.0,
    );
    let (xs, ys) = span(grown, clip);
    let half = [rect.width * 0.5 - radius, rect.height * 0.5 - radius];
    let middle = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
    for y in ys {
        for x in xs.clone() {
            let qx = (x as f32 + 0.5 - middle[0]).abs() - half[0];
            let qy = (y as f32 + 0.5 - middle[1]).abs() - half[1];
            let distance = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
            let inside = |distance: f32| (0.5 - distance).clamp(0.0, 1.0);
            let coverage = match stroke {
                Some(width) => inside(distance) - inside(distance + width),
                None => inside(distance),
            };
            if coverage > 0.0 {
                blend(
                    image,
                    x,
                    y,
                    [color[0], color[1], color[2], color[3] * coverage],
                );
            }
        }
    }
}

fn fill(image: &mut RgbaImage, rect: Rect, clip: Rect, color: [f32; 4]) {
    let (xs, ys) = span(rect, clip);
    for y in ys {
        for x in xs.clone() {
            blend(image, x, y, color);
        }
    }
}

/// Add `color`'s light at its alpha, as the renderer's additive pipeline.
fn add(image: &mut RgbaImage, x: i64, y: i64, color: [f32; 4]) {
    if x < 0 || y < 0 || x >= i64::from(image.width()) || y >= i64::from(image.height()) {
        return;
    }
    let pixel = image.get_pixel_mut(x as u32, y as u32);
    let alpha = color[3].clamp(0.0, 1.0);
    for (channel, source) in pixel.0.iter_mut().zip(color).take(3) {
        let value = f32::from(*channel) / 255.0 + source.clamp(0.0, 1.0) * alpha;
        *channel = (value.min(1.0) * 255.0).round() as u8;
    }
}

/// `source` mapped onto `rect` with texture coordinates `uv` at its corners
/// (top-left, top-right, bottom-right, bottom-left), tinted; added as light
/// when `additive`.
#[allow(clippy::too_many_arguments)]
fn textured(
    image: &mut RgbaImage,
    rect: Rect,
    clip: Rect,
    source: &RgbaImage,
    uv: [[f32; 2]; 4],
    tint: [f32; 4],
    wrap: bool,
    additive: bool,
) {
    let (xs, ys) = span(rect, clip);
    let lerp =
        |a: [f32; 2], b: [f32; 2], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    for y in ys {
        let t = ((y as f32 + 0.5) - rect.y) / rect.height;
        for x in xs.clone() {
            let s = ((x as f32 + 0.5) - rect.x) / rect.width;
            let [u, v] = lerp(lerp(uv[0], uv[1], s), lerp(uv[3], uv[2], s), t);
            let (u, v) = if wrap {
                (u.rem_euclid(1.0), v.rem_euclid(1.0))
            } else {
                (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0))
            };
            let tx = ((u * source.width() as f32) as u32).min(source.width() - 1);
            let ty = ((v * source.height() as f32) as u32).min(source.height() - 1);
            let texel = source.get_pixel(tx, ty).0.map(|c| f32::from(c) / 255.0);
            let color = [0, 1, 2, 3].map(|c| texel[c] * tint[c]);
            if additive {
                add(image, x, y, color);
            } else {
                blend(image, x, y, color);
            }
        }
    }
}

/// The emblem's decoded layers as pictures, once its worker has finished.
fn emblem_layers() -> HashMap<u32, (RgbaImage, bool)> {
    use crate::menu::emblem::EmblemLayer;
    crate::menu::emblem::request();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while crate::menu::emblem::decoded().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let Some(decoded) = crate::menu::emblem::decoded() else {
        return HashMap::new();
    };
    EmblemLayer::ALL
        .into_iter()
        .filter_map(|layer| {
            let chain = decoded.layer(layer)?;
            let image = RgbaImage::from_raw(chain.size, chain.size, chain.levels[0].clone())?;
            Some((layer.texture().0, (image, layer.additive())))
        })
        .collect()
}

/// Draw `list`'s shapes and art, then `vertices` (its text) from `atlas`.
fn raster(
    image: &mut RgbaImage,
    list: &DrawList,
    vertices: &[crate::text::TextVertex],
    atlas: &RgbaImage,
    icons: &HashMap<u32, RgbaImage>,
) {
    let decoded = crate::menu::art::decoded();
    let full = Rect::new(0.0, 0.0, image.width() as f32, image.height() as f32);
    let mut clips = vec![full];
    let mut opacity = vec![1.0_f32];
    let color = |c: sjk_ui::Color, o: f32| [c.r, c.g, c.b, c.a * o];
    let emblem = EMBLEM.get_or_init(emblem_layers);
    // The picture, whether it wraps and whether it adds light.
    let art = |texture: sjk_ui::TextureId| {
        let Some(piece) = ArtPiece::from_texture(texture) else {
            if let Some((layer, additive)) = emblem.get(&texture.0) {
                return Some((layer, false, *additive));
            }
            return icons.get(&texture.0).map(|icon| (icon, false, false));
        };
        Some((decoded?.image(piece)?, piece.wraps(), false))
    };
    for command in list.commands() {
        let clip = *clips.last().unwrap_or(&full);
        let o = *opacity.last().unwrap_or(&1.0);
        match command {
            DrawCommand::SolidRect { rect, color: c } => {
                fill(image, *rect, clip, color(*c, o));
            }
            DrawCommand::RoundedRect {
                rect,
                radius,
                color: c,
            } => {
                fill_rounded(image, *rect, *radius, clip, color(*c, o));
            }
            DrawCommand::GradientRect { rect, gradient, .. } => {
                const STEPS: usize = 32;
                for step in 0..STEPS {
                    let t = (step as f32 + 0.5) / STEPS as f32;
                    let (start, end) = (gradient.start, gradient.end);
                    let c = [
                        start.r + (end.r - start.r) * t,
                        start.g + (end.g - start.g) * t,
                        start.b + (end.b - start.b) * t,
                        (start.a + (end.a - start.a) * t) * o,
                    ];
                    let part = step as f32 / STEPS as f32;
                    let band = if gradient.vertical {
                        Rect::new(
                            rect.x,
                            rect.y + rect.height * part,
                            rect.width,
                            rect.height / STEPS as f32,
                        )
                    } else {
                        Rect::new(
                            rect.x + rect.width * part,
                            rect.y,
                            rect.width / STEPS as f32,
                            rect.height,
                        )
                    };
                    fill(image, band, clip, c);
                }
            }
            DrawCommand::Border {
                rect,
                radius,
                width,
                color: c,
            } => {
                shade_rounded(image, *rect, *radius, Some(*width), clip, color(*c, o));
            }
            DrawCommand::TexturedQuad {
                rect,
                texture,
                color: c,
            } => {
                if let Some((source, _, additive)) = art(*texture) {
                    let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
                    textured(
                        image,
                        *rect,
                        clip,
                        source,
                        uv,
                        color(*c, o),
                        false,
                        additive,
                    );
                }
            }
            // Only the in-game HUD turns pictures (the rocket lock-on); no menu does.
            DrawCommand::TexturedQuadTurned { .. } => {}
            DrawCommand::TexturedQuadUv {
                rect,
                texture,
                color: c,
                uv,
            } => {
                if let Some((source, wraps, additive)) = art(*texture) {
                    textured(
                        image,
                        *rect,
                        full,
                        source,
                        *uv,
                        color(*c, o),
                        wraps,
                        additive,
                    );
                }
            }
            DrawCommand::Arc {
                center,
                radius,
                width,
                start,
                sweep,
                color: c,
                knockout,
            } => {
                let extent = radius + width * 0.5 + 1.0;
                let area = Rect::new(
                    center[0] - extent,
                    center[1] - extent,
                    extent * 2.0,
                    extent * 2.0,
                );
                let (xs, ys) = span(area, clip);
                for y in ys {
                    for x in xs.clone() {
                        let point = [x as f32 + 0.5 - center[0], y as f32 + 0.5 - center[1]];
                        let distance = sjk_ui::arc_distance(point, *radius, *width, *start, *sweep);
                        let mut coverage = (0.5 - distance).clamp(0.0, 1.0);
                        if let Some(band) = knockout {
                            let knock = sjk_ui::knockout_coverage(
                                point,
                                [band.x - center[0], band.right() - center[0]],
                                band.height * 0.5,
                            );
                            coverage = sjk_ui::knockout_remainder(coverage, knock, c.a * o);
                        }
                        if coverage > 0.0 {
                            blend(image, x, y, color(*c, o * coverage));
                        }
                    }
                }
            }
            DrawCommand::Line {
                from,
                to,
                width,
                color: c,
            } => {
                // The renderer's line: full inside the stroke, fading across one pixel.
                let along = [to[0] - from[0], to[1] - from[1]];
                let length = along[0].hypot(along[1]);
                if length < 0.01 {
                    continue;
                }
                let half = width * 0.5 + 1.0;
                let area = Rect::new(
                    from[0].min(to[0]) - half,
                    from[1].min(to[1]) - half,
                    along[0].abs() + half * 2.0,
                    along[1].abs() + half * 2.0,
                );
                let (xs, ys) = span(area, clip);
                for y in ys {
                    for x in xs.clone() {
                        let point = [x as f32 + 0.5 - from[0], y as f32 + 0.5 - from[1]];
                        let t = (point[0] * along[0] + point[1] * along[1]) / length;
                        if !(0.0..=length).contains(&t) {
                            continue;
                        }
                        let across = (point[0] * along[1] - point[1] * along[0]).abs() / length;
                        let coverage = (0.5 - (across - width * 0.5)).clamp(0.0, 1.0);
                        if coverage > 0.0 {
                            blend(image, x, y, color(*c, o * coverage));
                        }
                    }
                }
            }
            DrawCommand::PushClip(rect) | DrawCommand::PushRoundedClip { rect, .. } => {
                let x = rect.x.max(clip.x);
                let y = rect.y.max(clip.y);
                clips.push(Rect::new(
                    x,
                    y,
                    (rect.right().min(clip.right()) - x).max(0.0),
                    (rect.bottom().min(clip.bottom()) - y).max(0.0),
                ));
            }
            DrawCommand::PopClip => {
                if clips.len() > 1 {
                    clips.pop();
                }
            }
            // Snapshots draw a tilted card flat: its parts as laid out.
            DrawCommand::PushTransform(_) | DrawCommand::PopTransform => {}
            DrawCommand::PushOpacity(value) => opacity.push(o * value),
            DrawCommand::PopOpacity => {
                if opacity.len() > 1 {
                    opacity.pop();
                }
            }
            DrawCommand::Text { .. } => {}
        }
    }
    raster_text(image, vertices, atlas);
}

/// Draw text `vertices` from `atlas` over `image`.
fn raster_text(image: &mut RgbaImage, vertices: &[crate::text::TextVertex], atlas: &RgbaImage) {
    // Each glyph is six vertices `[x, y, u, v, r, g, b, a]` in clip space,
    // top-left first and bottom-right third; its coverage is the atlas
    // alpha averaged over each pixel's footprint. A wide glyph's coverage is the
    // wide atlas's (`text::wide`), and an emoji draws its own colours from the emoji
    // atlas (`text::emoji`; black for a shadow), as `text.wgsl` picks them.
    use crate::text::{emoji, wide};
    let floats: &[[f32; 8]] = bytemuck::cast_slice(vertices);
    let [width, height] = [image.width() as f32, image.height() as f32];
    let wide_pixels = wide::mirror(<[u8]>::to_vec);
    let emoji_pixels = emoji::mirror(<[u8]>::to_vec);
    for quad in floats.chunks_exact(6) {
        let pixel = |v: &[f32; 8]| [(v[0] + 1.0) * 0.5 * width, (1.0 - v[1]) * 0.5 * height];
        let [x0, y0] = pixel(&quad[0]);
        let [x1, y1] = pixel(&quad[2]);
        let (mut u0, v0, mut u1, v1) = (quad[0][2], quad[0][3], quad[2][2], quad[2][3]);
        let tint = [quad[0][4], quad[0][5], quad[0][6], quad[0][7]];
        let is_emoji = u0 >= emoji::U_OFFSET - 0.5;
        let is_wide = !is_emoji && u0 >= wide::U_OFFSET - 0.5;
        let (atlas_width, atlas_height) = if is_emoji {
            (u0, u1) = (u0 - emoji::U_OFFSET, u1 - emoji::U_OFFSET);
            (emoji::ATLAS_WIDTH, emoji::ATLAS_HEIGHT)
        } else if is_wide {
            (u0, u1) = (u0 - wide::U_OFFSET, u1 - wide::U_OFFSET);
            (wide::ATLAS_WIDTH, wide::ATLAS_HEIGHT)
        } else {
            (atlas.width(), atlas.height())
        };
        let shadow = tint[0].max(tint[1]).max(tint[2]) < 0.004;
        // A texel's colour and coverage.
        let texel = |tx: u32, ty: u32| -> ([f32; 3], f32) {
            if is_emoji {
                let at = ((ty * emoji::ATLAS_WIDTH + tx) * 4) as usize;
                let Some(rgba) = emoji_pixels.get(at..at + 4) else {
                    return ([0.0; 3], 0.0);
                };
                let colour = rgba[..3].iter().map(|&c| f32::from(c) / 255.0);
                let mut rgb = [0.0; 3];
                for (out, c) in rgb.iter_mut().zip(colour) {
                    *out = if shadow { 0.0 } else { c };
                }
                (rgb, f32::from(rgba[3]) / 255.0)
            } else if is_wide {
                let coverage = wide_pixels
                    .get((ty * wide::ATLAS_WIDTH + tx) as usize)
                    .copied()
                    .unwrap_or(0);
                ([tint[0], tint[1], tint[2]], f32::from(coverage) / 255.0)
            } else {
                let coverage = atlas.get_pixel(tx, ty).0[3];
                ([tint[0], tint[1], tint[2]], f32::from(coverage) / 255.0)
            }
        };
        let (atlas_width, atlas_height) = (atlas_width as f32, atlas_height as f32);
        for y in y0.floor() as i64..y1.ceil() as i64 {
            let texel_y = |py: f32| {
                let t = ((py - y0) / (y1 - y0)).clamp(0.0, 1.0);
                (v0 + (v1 - v0) * t) * atlas_height
            };
            let (ty0, ty1) = (
                texel_y(y as f32) as i64,
                texel_y(y as f32 + 1.0).ceil() as i64,
            );
            for x in x0.floor() as i64..x1.ceil() as i64 {
                let texel_x = |px: f32| {
                    let t = ((px - x0) / (x1 - x0)).clamp(0.0, 1.0);
                    (u0 + (u1 - u0) * t) * atlas_width
                };
                let (tx0, tx1) = (
                    texel_x(x as f32) as i64,
                    texel_x(x as f32 + 1.0).ceil() as i64,
                );
                let mut colour = [0.0_f32; 3];
                let mut sum = 0.0_f32;
                let mut count = 0.0_f32;
                for ty in ty0..ty1.max(ty0 + 1) {
                    for tx in tx0..tx1.max(tx0 + 1) {
                        if (0..atlas_width as i64).contains(&tx)
                            && (0..atlas_height as i64).contains(&ty)
                        {
                            let (rgb, coverage) = texel(tx as u32, ty as u32);
                            for (out, c) in colour.iter_mut().zip(rgb) {
                                *out += c * coverage;
                            }
                            sum += coverage;
                        }
                        count += 1.0;
                    }
                }
                if sum <= 0.0 {
                    continue;
                }
                let [r, g, b] = colour.map(|c| c / sum);
                blend(image, x, y, [r, g, b, tint[3] * sum / count.max(1.0)]);
            }
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// What a snapshot is drawn with: the menu font, the UI atlas icons the
/// screen uses (by texture id), and the backdrop.
struct Snapshot {
    font: crate::text::FontAtlas,
    icons: HashMap<u32, RgbaImage>,
    /// A match under the in-game menus: the first retail levelshot found.
    in_match: Option<RgbaImage>,
}

impl Snapshot {
    /// Draw one screen over the menu backdrop (or the match) and write it as
    /// `name`.png.
    fn save(
        &self,
        name: &str,
        list: &DrawList,
        vertices: &[crate::text::TextVertex],
        over_match: bool,
    ) {
        self.save_at(name, list, vertices, over_match, VIEWPORT);
    }

    /// [`Self::save`] at another `viewport` size, for screens that depend on the
    /// aspect ratio.
    fn save_at(
        &self,
        name: &str,
        list: &DrawList,
        vertices: &[crate::text::TextVertex],
        over_match: bool,
        viewport: [f32; 2],
    ) {
        let directory = workspace_root().join("target/menu-snapshots");
        std::fs::create_dir_all(&directory).expect("create the snapshot directory");
        let size = (viewport[0] as u32, viewport[1] as u32);
        let mut image = match (&self.in_match, over_match) {
            (Some(shot), true) => {
                image::imageops::resize(shot, size.0, size.1, image::imageops::FilterType::Triangle)
            }
            _ => RgbaImage::from_pixel(size.0, size.1, Rgba([18, 22, 30, 255])),
        };
        raster(&mut image, list, vertices, &self.font.image, &self.icons);
        let path = directory.join(format!("{name}.png"));
        image.save(&path).expect("write the snapshot");
        println!("{}", path.display());
    }
}

/// SJK's emblem in one atlas cell, as the renderer uploads it.
fn logo_icon() -> RgbaImage {
    let emblem =
        image::load_from_memory(include_bytes!("../../../assets/branding/sjk-logo-512.png"))
            .expect("the emblem decodes")
            .into_rgba8();
    image::imageops::resize(&emblem, 128, 128, image::imageops::FilterType::Lanczos3)
}

/// Image `path` of the game data, decoded.
fn decode(vfs: &sjk_vfs::VirtualFileSystem, path: &str) -> Option<RgbaImage> {
    let asset = vfs.read(path).ok().flatten()?;
    Some(crate::decode_image(&asset.bytes, path).ok()?.into_rgba8())
}

/// A retail levelshot to stand for the match behind the in-game menus.
fn match_backdrop(vfs: &sjk_vfs::VirtualFileSystem) -> Option<RgbaImage> {
    ["levelshots/mp/ffa3.jpg", "levelshots/mp/ffa5.jpg"]
        .into_iter()
        .find_map(|path| decode(vfs, path))
}

#[test]
#[ignore = "reads the installed game data named by JKA_GAME_DATA"]
fn menu_snapshot() {
    let (art, vfs) = art();
    let mut shots = Snapshot {
        font: crate::text::load_modern(1.0, None).expect("build the menu font"),
        icons: HashMap::from([(crate::ui_renderer::LOGO_TEXTURE.0, logo_icon())]),
        in_match: match_backdrop(&vfs),
    };
    for (index, (_, bytes)) in crate::settings_icons::ICONS.iter().enumerate() {
        let icon = image::load_from_memory(bytes).expect("a settings icon");
        let texture = crate::ui_renderer::settings_icon(index);
        shots.icons.insert(texture.0, icon.into_rgba8());
    }
    // The credits' cards wear medals.
    medal_icons(&mut shots.icons);
    let font = &shots.font;
    let directory = tempfile::tempdir().expect("scratch profile");
    let mut console =
        crate::console::ViewerConsole::new(directory.path().join("config.cfg")).expect("console");
    // A changed setting shows its mark and its default.
    console.set_cvar("r_hdrExposure", "1.5");
    // The classic option panels, focused on a row with something to say.
    let panels: [(&str, Page, Entry, Frame, &str); 9] = [
        (
            "setup-video-ingame",
            Page::Setup,
            Entry::Video,
            Frame::InGame,
            "cg_fov",
        ),
        (
            "setup-hud-ingame",
            Page::Setup,
            Entry::Hud,
            Frame::InGame,
            "cg_hudScale",
        ),
        (
            "setup-interface",
            Page::Setup,
            Entry::Interface,
            Frame::Main,
            "ui_menuStyle",
        ),
        (
            "setup-game-ingame",
            Page::Setup,
            Entry::GameOptions,
            Frame::InGame,
            "cg_saberTrail",
        ),
        (
            "setup-scoreboard",
            Page::Setup,
            Entry::Scoreboard,
            Frame::Main,
            "cg_showClientIDs",
        ),
        (
            "setup-sound-ingame",
            Page::Setup,
            Entry::Sound,
            Frame::InGame,
            "s_volume",
        ),
        (
            "renderer-image",
            Page::Graphics,
            Entry::RenderImage,
            Frame::Main,
            "r_hdrExposure",
        ),
        (
            "renderer-lighting-ingame",
            Page::Graphics,
            Entry::RenderLighting,
            Frame::InGame,
            "r_liveLighting",
        ),
        (
            "renderer-shadows",
            Page::Graphics,
            Entry::RenderShadows,
            Frame::Main,
            "r_sunShadowTaps",
        ),
    ];
    for (name, page, entry, frame, cvar) in panels {
        let mut menu = SettingsMenu::new();
        match entry.panel() {
            Some(Panel::Settings { caption, span }) => {
                let tab = SettingsMenu::tab_index(caption).expect("settings tab");
                menu.open_classic(&console, tab, span, frame);
            }
            Some(Panel::Renderer { tab }) => menu.open_classic_renderer(&console, tab, frame),
            Some(Panel::Group(group)) => menu.open_classic_group(&console, group, frame),
            other => panic!("{entry:?} has no settings panel: {other:?}"),
        }
        menu.select_cvar(cvar);
        let panel = PanelFrame {
            frame,
            page,
            active: entry,
            art,
        };
        let mut vertices = Vec::new();
        menu.append_classic(&mut vertices, &font.font, VIEWPORT, 1.0, &panel);
        shots.save(name, menu.draw_list(), &vertices, frame == Frame::InGame);
    }
    // Classic+ search over every option, and a dropdown in both frames.
    let searches: [(&str, Frame, Option<&str>, &str); 3] = [
        ("settings-search", Frame::Main, Some("shadow"), ""),
        ("settings-dropdown", Frame::Main, None, "r_fullscreen"),
        (
            "settings-dropdown-ingame",
            Frame::InGame,
            None,
            "r_fullscreen",
        ),
    ];
    for (name, frame, search, cvar) in searches {
        let mut menu = SettingsMenu::new();
        let tab = SettingsMenu::tab_index("VIDEO").expect("video tab");
        menu.open_classic(&console, tab, Span::ALL, frame);
        match search {
            Some(text) => menu.search_for_snapshot(&console, text),
            None => {
                menu.select_cvar(cvar);
                menu.dropdown_for_snapshot(&console);
            }
        }
        let panel = PanelFrame {
            frame,
            page: Page::Setup,
            active: Entry::Video,
            art,
        };
        let mut vertices = Vec::new();
        menu.append_classic(&mut vertices, &font.font, VIEWPORT, 1.0, &panel);
        shots.save(name, menu.draw_list(), &vertices, frame == Frame::InGame);
    }
    // Key-binding panels.
    let binds: [(&str, Entry, Category, Frame, &str); 5] = [
        (
            "controls-search",
            Entry::ForcePowers,
            Category::Force,
            Frame::Main,
            "force",
        ),
        (
            "controls-movement",
            Entry::Movement,
            Category::Movement,
            Frame::Main,
            "+moveup",
        ),
        (
            "controls-weapons-ingame",
            Entry::Weapons,
            Category::Weapons,
            Frame::InGame,
            "weapon 4",
        ),
        (
            "controls-force-ingame",
            Entry::ForcePowers,
            Category::Force,
            Frame::InGame,
            "+force_grip",
        ),
        (
            "controls-interaction",
            Entry::Interaction,
            Category::Interaction,
            Frame::Main,
            "use_bacta",
        ),
    ];
    for (name, entry, category, frame, command) in binds {
        let mut editor = KeybindEditor::new();
        for (texture, paths) in editor.snapshot_icons() {
            if let Some(image) = decode(&vfs, &paths[0]) {
                shots.icons.insert(texture.0, image);
            }
        }
        editor.open_classic(&console, category as usize, Span::ALL);
        if name == "controls-search" {
            editor.search_for_snapshot(command);
        } else {
            editor.select_command(command);
        }
        let panel = PanelFrame {
            frame,
            page: Page::Controls,
            active: entry,
            art,
        };
        let mut vertices = Vec::new();
        editor.append_classic(&mut vertices, &shots.font.font, VIEWPORT, 1.0, &panel);
        shots.save(name, editor.draw_list(), &vertices, frame == Frame::InGame);
    }
    in_game_menu(&shots, art);
    console_browser(&shots, art);
    changelog(&shots, art);
    weapon_select(&mut shots, &vfs);
    radial_hud(&mut shots);
    player_card(&mut shots);
    quick_wheels(&mut shots);
    identity_page(&shots, art);
    text_dialog(&shots, art);
    force_wheel(&mut shots, &vfs);
    profile_saber(&shots, art, &vfs, &mut console);
    classic_profile(&mut shots, &vfs, art);
    nameplates(&mut shots, &vfs);
}

/// Only the nameplates, for a quicker look than the whole set.
#[test]
#[ignore = "reads the installed game data named by JKA_GAME_DATA"]
fn nameplate_snapshot() {
    let (_, vfs) = art();
    let mut shots = Snapshot {
        font: crate::text::load_modern(1.0, None).expect("build the menu font"),
        icons: HashMap::from([(crate::ui_renderer::LOGO_TEXTURE.0, logo_icon())]),
        in_match: match_backdrop(&vfs),
    };
    nameplates(&mut shots, &vfs);
}

/// Nameplates over a match: a verified saber player in medium stance just spawned
/// (overheal, 125) with estimated Force (grey haze), an enemy with a gun the estimate
/// cannot place (yellow "?"), a staff player farther off with an empty shield (broken
/// grey), and the player's own plate (`cg_nameplateSelf`) over a large shield (199).
fn nameplates(shots: &mut Snapshot, vfs: &sjk_vfs::VirtualFileSystem) {
    use crate::hud::nameplate::{PreviewPlate, State};
    use sjk_ui::TextureId;
    let badge = RgbaImage::from_raw(128, 128, crate::ui_renderer::verified_badge_pixels())
        .expect("a full cell");
    shots
        .icons
        .insert(crate::ui_renderer::VERIFIED_TEXTURE.0, badge);
    let mut weapons = Vec::new();
    for (index, (weapon, path)) in [
        (3_u8, "gfx/hud/w_icon_lightsaber.tga"),
        (5, "gfx/hud/w_icon_blaster.tga"),
    ]
    .into_iter()
    .enumerate()
    {
        if let Some(image) = decode(vfs, path) {
            let texture = TextureId(900_000 + index as u32);
            let cell =
                image::imageops::resize(&image, 128, 128, image::imageops::FilterType::Triangle);
            shots.icons.insert(texture.0, cell);
            weapons.push((weapon, texture));
        }
    }
    let icons = crate::hud::icons::Icons::with_weapons(&weapons);
    let plates = [
        PreviewPlate {
            slot: 1,
            point: [360.0, 330.0],
            distance: 260.0,
            health: Some([1.25, 1.25, 1.25]),
            shield: Some([0.25, 0.25, 0.25]),
            force: Some([0.35, 0.8, 0.85]),
            weapon: 3,
            style: 2,
            verified: true,
            team: None,
            shared: true,
        },
        PreviewPlate {
            slot: 2,
            point: [800.0, 360.0],
            distance: 420.0,
            health: Some([0.01, 1.0, 1.25]),
            shield: Some([0.0, 0.25, 1.0]),
            force: Some([0.1, 0.45, 0.6]),
            weapon: 5,
            style: 0,
            verified: false,
            team: Some(true),
            shared: true,
        },
        PreviewPlate {
            slot: 3,
            point: [1170.0, 300.0],
            distance: 700.0,
            health: Some([0.45, 0.5, 0.55]),
            shield: Some([0.0, 0.0, 0.0]),
            force: Some([1.0, 1.0, 1.0]),
            weapon: 3,
            style: 7,
            verified: true,
            team: None,
            shared: false,
        },
        PreviewPlate {
            slot: 0,
            point: [720.0, 720.0],
            distance: 120.0,
            health: Some([1.0, 1.0, 1.0]),
            shield: Some([1.99, 1.99, 1.99]),
            force: Some([0.64, 0.64, 0.64]),
            weapon: 3,
            style: 3,
            verified: true,
            team: Some(false),
            shared: false,
        },
    ];
    let names = [
        "^1Sol^7 (you)",
        "^5Kit^7 Fisto",
        "^1Bounty",
        "^3Darth^7Staff",
    ];
    let mut state = State::default();
    let mut vertices = Vec::new();
    // The plates draw with the HUD font in game (`hud_runtime::append`).
    let hud = crate::text::load_classic().expect("build the HUD font");
    let menu = std::mem::replace(&mut shots.font, hud);
    state.preview(
        &plates,
        &names,
        &icons,
        &shots.font.font,
        &mut vertices,
        VIEWPORT,
    );
    shots.save("nameplates", &state.list, &vertices, true);
    shots.font = menu;
}

/// The classic profile's lightsaber creation page, full screen and in game,
/// single, dual (the first blade a custom RGB) and staff. The live 3D
/// preview needs the renderer, so the drawn saber that stands in for it
/// before its first frame is what shows here.
fn profile_saber(
    shots: &Snapshot,
    art: ArtSet,
    vfs: &Arc<sjk_vfs::VirtualFileSystem>,
    console: &mut crate::console::ViewerConsole,
) {
    let setups: [(&str, bool, &str, &str, &str, &str); 4] = [
        ("profile-saber-single", false, "single_1", "none", "4", "0"),
        (
            "profile-saber-dual-rgb",
            false,
            "single_1",
            "single_2",
            "6",
            "3",
        ),
        ("profile-saber-staff", false, "dual_1", "none", "0", "0"),
        (
            "profile-saber-dual-ingame",
            true,
            "single_1",
            "single_2",
            "6",
            "3",
        ),
    ];
    for (name, in_game, saber1, saber2, color1, color2) in setups {
        console.set_cvar("saber1", saber1);
        console.set_cvar("saber2", saber2);
        console.set_cvar("color1", color1);
        console.set_cvar("color2", color2);
        // Orange-pink: 255 96 160.
        let packed = sjk_client::pack_saber_rgb([255, 96, 160]);
        console.set_cvar("cp_sbRGB1", &packed.to_string());
        let mut menu = crate::player_menu::PlayerMenu::new();
        menu.open_classic_saber_for_snapshot(console, Arc::clone(vfs), art, in_game);
        let mut vertices = Vec::new();
        menu.append(&mut vertices, &shots.font.font, VIEWPORT, 1.0);
        shots.save(name, menu.draw_list(), &vertices, in_game);
    }
}

/// The classic profile pages (`player_menu::classic`) on both frames: the
/// profile page with its bars and Apply, character creation, cosmetics, and the
/// Force page on both sides focused on the side column's last powers, for a
/// dark-side Jedi Master who has spent every point with Dark Rage and Team
/// Energize at level 0 (an owner's profile).
fn classic_profile(shots: &mut Snapshot, vfs: &Arc<sjk_vfs::VirtualFileSystem>, art: ArtSet) {
    use crate::player_menu::{PlayerMenu, ReturnTarget};
    let profile = tempfile::tempdir().expect("scratch profile");
    let mut console =
        crate::console::ViewerConsole::new(profile.path().join("config.cfg")).expect("console");
    console.set_cvar("forcepowers", "7-2-031330310000030333");
    let pages: [(&str, &str, bool, Option<u8>, ReturnTarget); 10] = [
        (
            "profile-player",
            "player",
            false,
            None,
            ReturnTarget::MainMenu,
        ),
        (
            "profile-player-ingame",
            "player",
            false,
            None,
            ReturnTarget::InGame,
        ),
        (
            "profile-character",
            "character",
            false,
            None,
            ReturnTarget::MainMenu,
        ),
        (
            "profile-character-ingame",
            "character",
            false,
            None,
            ReturnTarget::InGame,
        ),
        (
            "profile-cosmetics",
            "cosmetics",
            false,
            None,
            ReturnTarget::MainMenu,
        ),
        (
            "profile-cosmetics-ingame",
            "cosmetics",
            false,
            None,
            ReturnTarget::InGame,
        ),
        (
            "profile-force-dark",
            "force",
            true,
            Some(8),
            ReturnTarget::MainMenu,
        ),
        (
            "profile-force-dark-ingame",
            "force",
            true,
            Some(12),
            ReturnTarget::InGame,
        ),
        (
            "profile-force-light",
            "force",
            false,
            Some(11),
            ReturnTarget::MainMenu,
        ),
        (
            "profile-force-light-ingame",
            "force",
            false,
            Some(5),
            ReturnTarget::InGame,
        ),
    ];
    for (name, page, dark, focus, target) in pages {
        let mut menu = PlayerMenu::new();
        menu.attach_catalogue(Arc::clone(vfs));
        menu.set_style(true, art);
        menu.open(&console, target);
        for (texture, paths) in menu.snapshot_page(page, dark) {
            if let Some(image) = paths.iter().find_map(|path| decode(vfs, path)) {
                shots.icons.insert(texture.0, image);
            }
        }
        if let Some(power) = focus {
            menu.snapshot_focus_power(power);
        }
        let mut vertices = Vec::new();
        menu.append(&mut vertices, &shots.font.font, VIEWPORT, 1.0);
        println!(
            "{name}: {} of {} draws, {} widgets",
            menu.draw_list().len(),
            menu.draw_list().limit(),
            menu.snapshot_widgets()
        );
        shots.save(
            name,
            menu.draw_list(),
            &vertices,
            target == ReturnTarget::InGame,
        );
    }
}

/// The weapon selection row (`crate::weapon_select`) over the match, before (SJK's
/// old name line) and after, at 4:3 and 16:9; the name is in the bundled font here.
fn weapon_select(shots: &mut Snapshot, vfs: &sjk_vfs::VirtualFileSystem) {
    use crate::weapon_select as row;
    let icon = |weapon: u8| match weapon {
        3 => "lightsaber",
        4 => "blaster_pistol",
        5 => "blaster",
        6 => "disruptor",
        7 => "bowcaster",
        8 => "repeater",
        9 => "demp2",
        10 => "flechette",
        11 => "merrsonn",
        12 => "thermal",
        _ => "c_rifle",
    };
    let owned = [3_u8, 4, 5, 6, 7, 8, 9, 10, 11, 12, 15];
    for &weapon in &owned {
        let path = format!("gfx/hud/w_icon_{}", icon(weapon));
        for (texture, suffix) in [(9_000, ""), (9_100, "_na")] {
            if let Some(image) = decode(vfs, &format!("{path}{suffix}.tga")) {
                shots.icons.insert(texture + u32::from(weapon), image);
            }
        }
    }
    let mut inventory = sjk_client::LegacyWeaponInventory {
        owned: owned.iter().fold(0, |bits, weapon| bits | 1 << weapon),
        ammo: [100; 16],
        detpack_planted: false,
        following: false,
        spectator: false,
        emplaced: false,
    };
    inventory.ammo[3] = 0; // Out of power cells: disruptor, bowcaster, DEMP2.
    let selected = 5;
    let font = &shots.font.font;
    let names = row::State::new();
    for (label, viewport) in [("4x3", VIEWPORT), ("16x9", [1920.0, 1080.0])] {
        let mut list = DrawList::new(32);
        let side = row::side_max(viewport, 0);
        let shown = row::row(&inventory, selected, side).expect("weapons owned");
        row::place_icons(&shown, selected, viewport, 1.0, |weapon, rect| {
            let empty = !row::has_ammo(&inventory, weapon);
            let _ = list.push(DrawCommand::TexturedQuad {
                rect,
                texture: sjk_ui::TextureId(if empty { 9_100 } else { 9_000 } + u32::from(weapon)),
                color: sjk_ui::Color::new(1.0, 1.0, 1.0, 1.0),
            });
        });
        let mut vertices = Vec::new();
        let (x, baseline, line) = row::name_placement(viewport, 1.0);
        let scale = crate::ui_scale::glyph_scale(font, line, 1.0);
        let name = "E11-Blaster Rifle";
        let width = crate::text::visible_text_width(font, name, scale);
        let ink = font.glyph(crate::text::TextFace::Regular, b'H');
        crate::text::append_text_style(
            &mut vertices,
            font,
            name,
            [
                x - width * 0.5,
                baseline - (ink.offset_y + ink.height) * scale,
            ],
            scale,
            viewport,
            crate::text::TextFace::Regular,
            row::NAME_COLOR,
            0.0,
        );
        shots.save_at(
            &format!("weapon-select-{label}"),
            &list,
            &vertices,
            true,
            viewport,
        );
        // Before: only the name, `^3`, 0.78 of the height down, 42.6 px at 1080.
        let mut before = Vec::new();
        let scale = crate::ui_scale::glyph_scale(font, 42.6, viewport[1] / 1_080.0);
        let old = format!("^3{}", names.name(selected));
        let width = crate::text::visible_text_width(font, &old, scale);
        crate::text::append_text(
            &mut before,
            font,
            &old,
            [(viewport[0] - width) * 0.5, viewport[1] * 0.78],
            scale,
            viewport,
        );
        shots.save_at(
            &format!("weapon-select-before-{label}"),
            &DrawList::new(1),
            &before,
            true,
            viewport,
        );
    }
}

/// The classic in-game bar and its pop-ups over the match.
/// The HUD's Force wheel (JoF EJK's retail icon bar) over the match: JoF JA+'s
/// Repulse selected among real powers and the other JoF entries, then JA+ merc
/// mode's flamethrower in Lightning's place. Names draw in the menu font here.
/// SJK's radial HUD with a full, a hurt and an empty-handed (saber) state.
/// The Identity page: switched off, online with a profile, earlier names and known
/// players, and a new player the hub knows by the name they wear.
fn identity_page(shots: &Snapshot, art: ArtSet) {
    use crate::console::identity_panel::{Inputs, Panel};
    use sjk_identity::{Presence, Profile, Snapshot as Hub, Status, WornName};
    let hub = |status: Status, name: &str, earlier: &[&str]| Hub {
        loot: Vec::new(),
        opening: None,
        reveal: None,
        local_key_id: "44f3d0b36c9b2510".to_owned(),
        status,
        key_id: "44f3d0b36c9b2510".to_owned(),
        me: Some(Profile {
            credits: 0,
            cards: Vec::new(),
            keys: Vec::new(),
            key_id: "44f3d0b36c9b2510".to_owned(),
            key: String::new(),
            name: name.to_owned(),
            bio: String::new(),
            verified: false,
            staff: false,
            created: 0,
            names: std::iter::once(name)
                .chain(earlier.iter().copied())
                .map(|name| WornName {
                    name: name.to_owned(),
                    first_seen: 0,
                    last_seen: 0,
                })
                .collect(),
            medals: Vec::new(),
            achievements: Vec::new(),
            avatar: String::new(),
            unlocks: Vec::new(),
            holocron_counts: Default::default(),
            holocrons: Vec::new(),
        }),
        server: None,
        players: vec![
            Presence {
                slot: 3,
                claimed_name: "^1Fox".to_owned(),
                key_id: "aaaaaaaaaaaaaaaa".to_owned(),
                name: "Fox".to_owned(),
                verified: true,
                medals: Vec::new(),
                avatar: String::new(),
                look: None,
            },
            Presence {
                slot: 7,
                claimed_name: "Kit".to_owned(),
                key_id: "bbbbbbbbbbbbbbbb".to_owned(),
                name: String::new(),
                verified: false,
                medals: Vec::new(),
                avatar: String::new(),
                look: None,
            },
        ],
        profiles: std::collections::HashMap::new(),
        model_checks: std::collections::HashMap::new(),
        profiles_revision: 0,
        notice: Some("saved".to_owned()),
        revision: 0,
        report: None,
        note: None,
        player_report: None,
        avatar: None,
        look_outcome: None,
        packs_revision: 0,
        assets_note: None,
        crashes_sent: 0,
        crash_note: None,
        holocrons: None,
        log_requests: Vec::new(),
        log_answer: None,
    };
    let online = hub(Status::Online, "^1Sol", &["^4Vulpes", "Padawan"]);
    let fresh = hub(Status::Online, "Padawan", &[]);
    let refused = hub(
        Status::Failed(
            "cannot reach the hub: io: No connection could be made because the target machine actively refused it. (os error 10061)"
                .to_owned(),
        ),
        "Sol",
        &[],
    );
    let key_file =
        "C:/Program Files (x86)/Steam/steamapps/common/Jedi Academy/GameData/SJK/identity.key";
    struct Case<'a> {
        name: &'a str,
        enabled: bool,
        snapshot: Option<&'a Hub>,
        bio: &'a str,
        focus: &'a str,
        message: &'a str,
        hub_url: &'a str,
    }
    let cases = [
        Case {
            name: "identity-off",
            enabled: false,
            snapshot: None,
            bio: "",
            focus: "toggle",
            message: "",
            hub_url: "https://sjk.dfox.app",
        },
        Case {
            name: "identity-online",
            enabled: true,
            snapshot: Some(&online),
            bio: "I make SJK and I play on JA+ servers. Come say hi, I am usually around in the evening and I love a good duel.",
            focus: "bio",
            message: "",
            hub_url: "https://sjk.dfox.app",
        },
        Case {
            name: "identity-new",
            enabled: true,
            snapshot: Some(&fresh),
            bio: "",
            focus: "toggle",
            message: "",
            hub_url: "https://sjk.dfox.app",
        },
        Case {
            name: "identity-hub",
            enabled: true,
            snapshot: Some(&refused),
            bio: "",
            focus: "hub",
            message: "",
            hub_url: "http://127.0.0.1:8787",
        },
    ];
    for case in &cases {
        let mut panel = Panel::new();
        panel.open(false);
        panel.set_art(art);
        panel.preview(case.bio, case.focus, case.message);
        let inputs = Inputs {
            enabled: case.enabled,
            hub_url: case.hub_url,
            key_error: None,
            snapshot: case.snapshot,
            key_file,
        };
        let mut vertices = Vec::new();
        panel.append(&inputs, &mut vertices, &shots.font.font, VIEWPORT);
        let name = format!("{}-classic", case.name);
        shots.save(&name, panel.draw_list(), &vertices, true);
    }
}

/// The Report a bug button and the text dialog over a match, for a report and a note,
/// in both looks.
fn text_dialog(shots: &Snapshot, art: ArtSet) {
    use crate::text_dialog::{Kind, TextDialog};
    let note = Kind::Note {
        subject: "Wall: textures/mp/ffa_wall2 (lightmapped, BSP surface 1432) on mp/ffa3".into(),
        model: None,
    };
    let cases = [
        (
            "report-dialog",
            Kind::Report,
            "The door near the red base flickers when I walk through it on ffa3, and the light behind it goes black. I expected it to open smoothly like it does in the original game.",
            false,
            "",
        ),
        (
            "report-dialog-refused",
            Kind::Report,
            "short",
            true,
            "write at least a few words",
        ),
        ("note-dialog", note, "", false, ""),
    ];
    let mut dialog = TextDialog::default();
    dialog.set_look(crate::text_dialog::Look::Classic, art);
    let mut vertices = Vec::new();
    dialog.append_launcher(&mut vertices, &shots.font.font, VIEWPORT);
    shots.save(
        "report-launcher-classic",
        dialog.launcher_draw_list(),
        &vertices,
        true,
    );
    for (name, kind, text, on_send, message) in &cases {
        dialog.open(kind.clone());
        dialog.preview(text, *on_send, message);
        let mut vertices = Vec::new();
        dialog.append(&mut vertices, &shots.font.font, VIEWPORT);
        shots.save(
            &format!("{name}-classic"),
            dialog.draw_list(),
            &vertices,
            true,
        );
    }
}

/// The player card beside three players over a match: one the hub vouches for,
/// one it only knows, and one it does not; and one at the screen's edge.
fn player_card(shots: &mut Snapshot) {
    use crate::hud::player_card::{HubInfo, State, card_from};
    let info = |text: &str| text.replace('|', "\\").into_bytes();
    let cards = [
        (
            "player-card-verified",
            card_from(
                &info("n|^1Sol^7 the Fox|t|1|model|kyle/default|st|single_1|st2|none|c1|4|c2|0|"),
                Some(HubInfo {
                    name: "Sol".to_owned(),
                    verified: true,
                    medals: crate::medals::Medals::default(),
                    ..HubInfo::default()
                }),
                None,
            ),
            [760.0, 470.0],
        ),
        (
            "player-card-registered",
            card_from(
                &info("n|Fox|t|2|model|jedi_hf/blue|st|dual_1|st2|dual_2|c1|0|c2|3|w|5|l|2|"),
                Some(HubInfo {
                    name: String::new(),
                    verified: false,
                    medals: crate::medals::Medals::default(),
                    ..HubInfo::default()
                }),
                None,
            ),
            [760.0, 470.0],
        ),
        (
            "player-card-plain",
            card_from(
                &info("n|Padawan|t|0|model|kyle|st|single_2|c1|2|"),
                None,
                None,
            ),
            [760.0, 470.0],
        ),
        (
            "player-card-edge",
            card_from(&info("n|Edge|t|0|model|kyle|st|single_1|c1|1|"), None, None),
            [1_380.0, 300.0],
        ),
    ];
    for (name, card, head) in cards {
        let state = State::preview(card, head, VIEWPORT, false);
        let mut vertices = Vec::new();
        crate::ui_renderer::append_text_commands(
            &state.list,
            |id| state.resolve_text(id),
            &mut vertices,
            &shots.font.font,
            VIEWPORT,
            crate::text::TextStyle::NEUTRAL,
        );
        shots.save(name, &state.list, &vertices, true);
    }
    player_card_profile(shots);
}

/// The player card of an SJK player with everything the card reads: the Glitch shader
/// worn (from the hub's packs `SJK_TEST_PACKS` names, else the test sample as the Sun),
/// the SJK TEAM mark, a Legendary holocron, a medal, and pinned, the achievements, bio
/// and since when.
fn player_card_profile(shots: &mut Snapshot) {
    use crate::hud::player_card::{HubInfo, ProfileFacts, State, card_from};
    let info = |text: &str| text.replace('|', "\\").into_bytes();
    let skins = match std::env::var_os("SJK_TEST_PACKS") {
        Some(directory) => {
            let _ = crate::sjk_packs::mount(std::path::Path::new(&directory));
            crate::sjk_packs::skins()
        }
        None => std::sync::Arc::new(crate::saber_skins::tests::loaded_sample(1)),
    };
    let shader = ["saber_glitch", "saber_sun"]
        .into_iter()
        .find(|id| skins.get(id).is_some());
    let achievement = |id: &str, unlocked| sjk_identity::Achievement {
        id: id.to_owned(),
        progress: 1,
        goal: 1,
        unlocked,
    };
    let profile = sjk_identity::Profile {
        credits: 0,
        cards: Vec::new(),
        keys: Vec::new(),
        key_id: "0123456789abcdef".to_owned(),
        key: String::new(),
        name: "Sol".to_owned(),
        bio: "Fox of the JoF and maker of SJK, the fast clean JKA client.".to_owned(),
        verified: true,
        staff: true,
        created: 1_790_812_800,
        names: Vec::new(),
        medals: Vec::new(),
        achievements: [
            "first_blood",
            "kills_100",
            "kills_1000",
            "duel_wins_10",
            "maps_10",
        ]
        .iter()
        .enumerate()
        .map(|(index, id)| achievement(id, 1_791_000_000 + index as i64))
        .collect(),
        unlocks: Vec::new(),
        avatar: String::new(),
        holocron_counts: sjk_identity::HolocronCounts {
            uncommon: 3,
            rare: 1,
            legendary: 1,
            mythical: 0,
        },
        holocrons: Vec::new(),
    };
    let medal = sjk_identity::Medal {
        id: "bug_hunter".to_owned(),
        count: 2,
        awarded: 0,
        note: String::new(),
    };
    for (name, pinned) in [
        ("player-card-profile", false),
        ("player-card-profile-pinned", true),
    ] {
        let card = card_from(
            &info("n|^1Sol^7 the Fox|t|1|model|kyle/default|st|single_1|st2|none|c1|4|c2|0|"),
            Some(HubInfo {
                name: "Sol".to_owned(),
                verified: true,
                medals: crate::medals::Medals::from_wire(std::slice::from_ref(&medal)),
                key_id: profile.key_id.clone(),
                avatar: String::new(),
                profile: Some(ProfileFacts::of(&profile)),
                vitals: crate::hud::player_card::Vitals::SeesAll,
                model: crate::hud::player_card::ModelSight::Missing,
            }),
            shader,
        );
        let skin = shader.and_then(|id| skins.get(id));
        let state = State::preview_with(card, [760.0, 470.0], VIEWPORT, pinned, skin);
        let mut vertices = Vec::new();
        crate::ui_renderer::append_text_commands(
            &state.list,
            |id| state.resolve_text(id),
            &mut vertices,
            &shots.font.font,
            VIEWPORT,
            crate::text::TextStyle::NEUTRAL,
        );
        shots.save(name, &state.list, &vertices, true);
    }
}

/// The SJK UI's main page over its map (the JoF HD wide levelshot of
/// mp/duel6 standing in for the live backdrop), with each family drawn from
/// its own atlas: the main page with three recent servers, the Play and SJK
/// pages, a server focused, the JoF suggestion of a first start, and at 4:3, each
/// with the SJK chat docked under the servers.
#[test]
#[ignore = "reads the installed game data named by JKA_GAME_DATA"]
fn sjk_home_snapshot() {
    use crate::game_font::SjkFonts;
    use crate::menu::sjk::home::{self, ChatDock, DockLine, Home, HomeView, Page, ServerItem};
    use crate::menu::sjk::recent::Ago;
    use crate::menu_widgets::MenuCanvas;
    let (_, vfs) = art();
    let display = crate::text::load_family(&crate::text::DISPLAY, 1.0, None).expect("Rajdhani");
    let body = crate::text::load_family(&crate::text::BODY, 1.0, None).expect("Exo 2");
    let backdrop = decode(&vfs, "levelshots/mp/duel6.jpg").expect("the duel6 levelshot");
    let recent = [
        ServerItem {
            name: "^5JoF^7 Jedi of Freedom",
            map: "mp/ffa3",
            live: Some((24, 32, 31)),
            played: Some(Ago::Hours(2)),
        },
        ServerItem {
            name: "Duel Arena",
            map: "mp/duel6",
            live: Some((14, 32, 28)),
            played: Some(Ago::Yesterday),
        },
        ServerItem {
            name: "Academy roleplay",
            map: "academy_v3",
            live: None,
            played: Some(Ago::Days(5)),
        },
    ];
    let suggestion = [ServerItem {
        name: "^5JoF^7 Jedi of Freedom",
        map: "mp/ffa3",
        live: Some((24, 32, 31)),
        played: None,
    }];
    let chat = [
        DockLine {
            name: "^5JoF^7 Jedi",
            text: "anyone up for duels on ffa3?",
            verified: true,
            staff: false,
            key_id: "0123456789abcdef",
            tier: None,
            gif: None,
        },
        DockLine {
            name: "^1Fox",
            text: "in 5 min, finishing a CTF",
            verified: false,
            staff: false,
            key_id: "fedcba9876543210",
            tier: None,
            gif: None,
        },
        DockLine {
            name: "Kyle",
            text: "the new HUD looks great, but the force bar in the corner feels a bit too small at 4K",
            verified: false,
            staff: false,
            key_id: "00000000000000ff",
            tier: None,
            gif: None,
        },
    ];
    // Name, window, page, chosen entry, focused server, servers.
    type Case<'a> = (
        &'a str,
        [f32; 2],
        Page,
        usize,
        Option<usize>,
        &'a [ServerItem<'a>],
    );
    let cases: [Case; 6] = [
        ("sjk-home", VIEWPORT_WIDE, Page::Main, 0, None, &recent),
        ("sjk-home-play", VIEWPORT_WIDE, Page::Play, 0, None, &recent),
        ("sjk-home-sjk", VIEWPORT_WIDE, Page::Sjk, 1, None, &recent),
        (
            "sjk-home-server",
            VIEWPORT_WIDE,
            Page::Main,
            2,
            Some(1),
            &recent,
        ),
        (
            "sjk-home-first",
            VIEWPORT_WIDE,
            Page::Main,
            1,
            None,
            &suggestion,
        ),
        ("sjk-home-4x3", VIEWPORT, Page::Main, 0, None, &recent),
    ];
    for (name, viewport, page, entry, server, servers) in cases {
        let mut canvas = MenuCanvas::new();
        let mut home = Home::for_snapshot(page, entry, server);
        let view = HomeView {
            name: "^5JoF^7 Jedi Gooner Solol",
            model: "kyle",
            blade_name: "blue",
            servers,
            version: "2026.1007.1",
            update: Some("2026.1008.1"),
            seconds: 12.0,
            chat: Some(ChatDock {
                lines: &chat,
                online: 12,
                live: true,
                notice: "",
                measure: Some(crate::sjk_chat_look::Measure::new(
                    &body.font,
                    crate::text::TextStyle::NEUTRAL,
                )),
            }),
            summary: &crate::profile_card::NOBODY,
            donation: Some(crate::donation_goal::Card {
                numbers: Some(crate::donation_goal::Numbers {
                    raised_cents: 8_000,
                    goal_cents: 20_000,
                    monthly: true,
                    euros: true,
                }),
            }),
        };
        home::build(&mut canvas, viewport, &mut home, &view, 1.0);
        let (mut display_text, mut body_text) = (Vec::new(), Vec::new());
        canvas.append_text_families(
            SjkFonts {
                display: (&mut display_text, &display.font),
                body: (&mut body_text, &body.font),
                code: None,
            },
            viewport,
            crate::text::TextStyle::NEUTRAL,
        );
        let fonts = SjkShotFonts {
            display: (&display_text, &display.image),
            body: (&body_text, &body.image),
        };
        save_sjk_shot(
            name,
            viewport,
            &backdrop,
            canvas.draw_list(),
            &HashMap::new(),
            fonts,
        );
    }
}

/// The text of an SJK UI snapshot, each family's vertices with its atlas.
struct SjkShotFonts<'a> {
    display: (&'a [crate::text::TextVertex], &'a RgbaImage),
    body: (&'a [crate::text::TextVertex], &'a RgbaImage),
}

/// Save SJK UI snapshot `name`: `backdrop` covering the window (cropped at
/// its sides), `list`'s shapes and `icons` over it, then each family's text.
fn save_sjk_shot(
    name: &str,
    viewport: [f32; 2],
    backdrop: &RgbaImage,
    list: &DrawList,
    icons: &HashMap<u32, RgbaImage>,
    fonts: SjkShotFonts<'_>,
) {
    let size = (viewport[0] as u32, viewport[1] as u32);
    let cover = size.1 as f32 / backdrop.height() as f32;
    let scaled_width = (backdrop.width() as f32 * cover).max(size.0 as f32) as u32;
    let scaled = image::imageops::resize(
        backdrop,
        scaled_width,
        size.1,
        image::imageops::FilterType::Triangle,
    );
    let mut image =
        image::imageops::crop_imm(&scaled, (scaled_width - size.0) / 2, 0, size.0, size.1)
            .to_image();
    raster(&mut image, list, &[], fonts.display.1, icons);
    raster_text(&mut image, fonts.display.0, fonts.display.1);
    raster_text(&mut image, fonts.body.0, fonts.body.1);
    let directory = workspace_root().join("target/menu-snapshots");
    std::fs::create_dir_all(&directory).expect("create the snapshot directory");
    let path = directory.join(format!("{name}.png"));
    image.save(&path).expect("write the snapshot");
    println!("{}", path.display());
}

/// The SJK UI's Settings over the duel6 levelshot: Interface with a changed
/// slider focused (its dot and reset arrow), Display with a list open, a
/// search's results, Graphics scrolled to its weather, and at 4:3.
#[test]
#[ignore = "reads the installed game data named by JKA_GAME_DATA"]
fn sjk_settings_snapshot() {
    use crate::game_font::SjkFonts;
    use crate::menu::sjk::TextTarget;
    use crate::settings::mode::SettingsMode;
    let (_, vfs) = art();
    let display = crate::text::load_family(&crate::text::DISPLAY, 1.0, None).expect("Rajdhani");
    let body = crate::text::load_family(&crate::text::BODY, 1.0, None).expect("Exo 2");
    let backdrop = decode(&vfs, "levelshots/mp/duel6.jpg").expect("the duel6 levelshot");
    let mut icons = HashMap::new();
    for (index, (_, bytes)) in crate::settings_icons::ICONS.iter().enumerate() {
        let icon = image::load_from_memory(bytes).expect("a settings icon");
        icons.insert(
            crate::ui_renderer::settings_icon(index).0,
            icon.into_rgba8(),
        );
    }
    // The rail's Quick wheel wears the wheel's own icon.
    for (index, (_, bytes)) in crate::quick_wheel::ICONS.iter().enumerate() {
        let icon = image::load_from_memory(bytes).expect("a wheel icon");
        icons.insert(crate::ui_renderer::wheel_icon(index).0, icon.into_rgba8());
    }
    let directory = tempfile::tempdir().unwrap();
    let mut console =
        crate::console::ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
    console.set_cvar("con_scale", "1.3");
    // Name, window, category, focused setting, its list open, a search.
    type Case<'a> = (
        &'a str,
        [f32; 2],
        usize,
        Option<&'a str>,
        bool,
        Option<&'a str>,
    );
    let cases: [Case; 5] = [
        (
            "sjk-settings",
            VIEWPORT_WIDE,
            7,
            Some("con_scale"),
            false,
            None,
        ),
        (
            "sjk-settings-list",
            VIEWPORT_WIDE,
            1,
            Some("r_fullscreen"),
            true,
            None,
        ),
        (
            "sjk-settings-search",
            VIEWPORT_WIDE,
            1,
            None,
            false,
            Some("shadow"),
        ),
        (
            "sjk-settings-graphics",
            VIEWPORT_WIDE,
            2,
            Some(crate::weather::CVAR),
            false,
            None,
        ),
        (
            "sjk-settings-4x3",
            VIEWPORT,
            7,
            Some("con_scale"),
            false,
            None,
        ),
    ];
    // Advanced's categories, then Simple's: Display, and Graphics on Vanilla and
    // on the SJK renderer, whose alpha warning the detail column shows.
    SettingsMode::Advanced.store(&mut console);
    let simple: [Case; 4] = [
        (
            "sjk-settings-chat",
            VIEWPORT_WIDE,
            9,
            Some("cl_sjkChat"),
            false,
            None,
        ),
        (
            "sjk-settings-simple",
            VIEWPORT_WIDE,
            13,
            Some("cg_fov"),
            false,
            None,
        ),
        (
            "sjk-settings-simple-vanilla",
            VIEWPORT_WIDE,
            14,
            Some(crate::graphics_quality::RENDERER_ROW),
            false,
            None,
        ),
        (
            "sjk-settings-simple-alpha",
            VIEWPORT_WIDE,
            14,
            Some(crate::graphics_quality::RENDERER_ROW),
            false,
            None,
        ),
    ];
    for (name, viewport, category, cvar, list, search) in cases.into_iter().chain(simple) {
        if name.starts_with("sjk-settings-simple") {
            SettingsMode::Simple.store(&mut console);
        }
        let renderer = if name.ends_with("-vanilla") {
            crate::graphics_quality::VANILLA
        } else {
            crate::graphics_quality::SJK
        };
        crate::graphics_quality::set_renderer(&mut console, renderer);
        let mut menu = crate::menu::ClientMenu::new(true, String::new());
        menu.sjk_settings_for_snapshot(&console, category, cvar, list, search);
        let (mut display_text, mut body_text) = (Vec::new(), Vec::new());
        menu.append_sjk_settings(
            TextTarget::Families(
                SjkFonts {
                    display: (&mut display_text, &display.font),
                    body: (&mut body_text, &body.font),
                    code: None,
                },
                crate::text::TextStyle::NEUTRAL,
            ),
            viewport,
        );
        let fonts = SjkShotFonts {
            display: (&display_text, &display.image),
            body: (&body_text, &body.image),
        };
        let list = menu.draw_list().expect("the settings screen draws");
        save_sjk_shot(name, viewport, &backdrop, list, &icons, fonts);
    }
}

/// A 16:9 window, the shape most players have.
const VIEWPORT_WIDE: [f32; 2] = [1920.0, 1080.0];

/// Only the classic+ settings panels (switches, sliders, segments, choice
/// fields, sub-headings, a changed row's dot and reset arrow), in Inter and in
/// SJK Menu (`ui_gameFont 1`), for a quicker look than the whole set.
#[test]
#[ignore = "reads the installed game data named by JKA_GAME_DATA"]
fn settings_panel_snapshot() {
    let (art, vfs) = art();
    let mut icons = HashMap::from([(crate::ui_renderer::LOGO_TEXTURE.0, logo_icon())]);
    for (index, (_, bytes)) in crate::settings_icons::ICONS.iter().enumerate() {
        let icon = image::load_from_memory(bytes).expect("a settings icon");
        let texture = crate::ui_renderer::settings_icon(index);
        icons.insert(texture.0, icon.into_rgba8());
    }
    let directory = tempfile::tempdir().expect("scratch profile");
    let mut console =
        crate::console::ViewerConsole::new(directory.path().join("config.cfg")).expect("console");
    // Changed rows show their dot, and the focused one its reset arrow.
    console.set_cvar("r_hdrExposure", "1.5");
    console.set_cvar("con_style", "classic");
    console.set_cvar("cg_drawTimer", "1");
    let fonts = [
        (
            "",
            crate::text::load_modern(1.0, None).expect("build the menu font"),
        ),
        (
            "-gamefont",
            crate::text::retail_font::load(&crate::text::retail_font::MENU).expect("SJK Menu"),
        ),
    ];
    let cases: [(&str, Page, Entry, Frame, &str); 5] = [
        (
            "panel-interface",
            Page::Gameplay,
            Entry::Interface,
            Frame::Main,
            "con_style",
        ),
        (
            "panel-hud-ingame",
            Page::Gameplay,
            Entry::Hud,
            Frame::InGame,
            "cg_hudScale",
        ),
        (
            "panel-first-setup",
            Page::Setup,
            Entry::FirstSetup,
            Frame::Main,
            "r_fullscreen",
        ),
        (
            "panel-image",
            Page::Graphics,
            Entry::RenderImage,
            Frame::Main,
            "r_hdrExposure",
        ),
        (
            "panel-game-ingame",
            Page::Gameplay,
            Entry::GameOptions,
            Frame::InGame,
            "cg_auraShell",
        ),
    ];
    for (suffix, font) in fonts {
        let shots = Snapshot {
            font,
            icons: icons.clone(),
            in_match: match_backdrop(&vfs),
        };
        for (name, page, entry, frame, cvar) in cases {
            let mut menu = SettingsMenu::new();
            match entry.panel() {
                Some(Panel::Settings { caption, span }) => {
                    let tab = SettingsMenu::tab_index(caption).expect("settings tab");
                    menu.open_classic(&console, tab, span, frame);
                }
                Some(Panel::Renderer { tab }) => menu.open_classic_renderer(&console, tab, frame),
                Some(Panel::Group(group)) => menu.open_classic_group(&console, group, frame),
                other => panic!("{entry:?} has no settings panel: {other:?}"),
            }
            menu.select_cvar(cvar);
            let panel = PanelFrame {
                frame,
                page,
                active: entry,
                art,
            };
            let mut vertices = Vec::new();
            menu.append_classic(&mut vertices, &shots.font.font, VIEWPORT, 1.0, &panel);
            shots.save(
                &format!("{name}{suffix}"),
                menu.draw_list(),
                &vertices,
                frame == Frame::InGame,
            );
        }
    }
}

/// Only the quick wheels, for a quicker look than the whole set.
#[test]
#[ignore = "reads the installed game data named by JKA_GAME_DATA"]
fn quick_wheel_snapshot() {
    let (_, vfs) = art();
    let mut shots = Snapshot {
        font: crate::text::load_modern(1.0, None).expect("build the menu font"),
        icons: HashMap::new(),
        in_match: match_backdrop(&vfs),
    };
    quick_wheels(&mut shots);
}

/// The default pages of the quick wheel over a match, the mouse pushed towards one
/// choice, the choices in effect marked (in Inter: these snapshots have no SJK UI
/// families; `world_shot::tests::duel6_quick_wheel` draws the real thing).
fn quick_wheels(shots: &mut Snapshot) {
    use crate::quick_wheel::{ICONS, QuickWheel, ShownPage, pages};
    for (index, (_, bytes)) in ICONS.iter().enumerate() {
        let icon = image::load_from_memory(bytes)
            .expect("a wheel icon")
            .into_rgba8();
        shots
            .icons
            .insert(crate::ui_renderer::wheel_icon(index).0, icon);
    }
    let shown = |marked: &[usize]| -> Vec<ShownPage> {
        // The Force page is left out: it needs a game's powers.
        pages::defaults()
            .iter()
            .filter(|page| !page.force)
            .map(|page| ShownPage {
                id: page.id.clone(),
                name: page.name.clone(),
                choices: page
                    .choices
                    .iter()
                    .enumerate()
                    .map(|(index, slot)| crate::quick_wheel::ShownChoice {
                        label: slot.label().to_owned(),
                        command: slot.command().to_owned(),
                        icon: slot.texture(),
                        on: marked.contains(&index),
                        gif: None,
                    })
                    .collect(),
                force: false,
                gifs: false,
            })
            .collect()
    };
    let cases: [(&str, usize, [f32; 2], &[usize]); 3] = [
        ("quick-wheel-general", 0, [60.0, -60.0], &[1, 2]),
        ("quick-wheel-weather", 2, [80.0, 20.0], &[0, 6, 7]),
        ("quick-wheel-weather-middle", 2, [6.0, -4.0], &[3, 6, 7]),
    ];
    for (name, page, pointer, marked) in cases {
        let mut state = QuickWheel::default();
        let now = std::time::Instant::now();
        state.open(shown(marked), page, now);
        state.moved(pointer);
        state.build(VIEWPORT, now);
        let mut vertices = Vec::new();
        state.append_text(None, &mut vertices, &shots.font.font, VIEWPORT);
        shots.save(name, state.draw_list(), &vertices, true);
    }
}
/// Only the radial HUD, for a quicker look than the whole set.
#[test]
#[ignore = "reads the installed game data named by JKA_GAME_DATA"]
fn radial_hud_snapshot() {
    let (_, vfs) = art();
    let mut shots = Snapshot {
        font: crate::text::load_modern(1.0, None).expect("build the menu font"),
        icons: HashMap::from([(crate::ui_renderer::LOGO_TEXTURE.0, logo_icon())]),
        in_match: match_backdrop(&vfs),
    };
    radial_hud(&mut shots);
}

fn radial_hud(shots: &mut Snapshot) {
    use crate::hud::{HudLook, HudOverlay, HudVisibility};
    let visibility = HudVisibility {
        hud: true,
        status: true,
        weapon: true,
        crosshair: true,
        crosshair_names: false,
        timer: false,
        lagometer: false,
        ground_hud: false,
        menu_hud: false,
    };
    let states = [
        ("hud-radial-full", 100, 100, 100, 5, Some(300), None, 1.0),
        ("hud-radial-hurt", 38, 20, 62, 5, Some(120), None, 0.4),
        ("hud-radial-low", 18, 0, 8, 5, Some(4), None, 0.013),
        ("hud-radial-saber", 83, 40, 100, 3, None, Some(2), 0.0),
        ("hud-radial-strong", 61, 15, 45, 3, None, Some(3), 0.0),
        ("hud-radial-fast", 100, 100, 100, 3, None, Some(1), 0.0),
        // Just spawned (125) over a large shield picked up at 99 (199).
        (
            "hud-radial-overheal",
            125,
            199,
            100,
            5,
            Some(300),
            None,
            1.0,
        ),
    ];
    // The 4:3 frame, then the same states on a 16:9 screen: the rings keep their place
    // relative to the screen's height.
    let screens = [("", VIEWPORT), ("-wide", [1920.0, 1080.0])];
    for ((name, health, armor, force, weapon, ammo, style, ammo_ratio), (suffix, viewport)) in
        states
            .into_iter()
            .flat_map(|state| screens.map(|screen| (state, screen)))
    {
        let name = format!("{name}{suffix}");
        let mut hud = HudOverlay::new();
        let values = sjk_client::HudDataSource {
            health,
            armor,
            force,
            weapon,
            ammo,
            saber_style: style,
        };
        hud.preview_values(
            values,
            [
                health as f32 / 100.0,
                armor as f32 / 100.0,
                force as f32 / 100.0,
                ammo_ratio,
            ],
        );
        let _ = hud.layout(
            &shots.font.font,
            HudLook::Radial,
            viewport,
            1.0,
            visibility,
            0,
        );
        let mut vertices = Vec::new();
        crate::ui_renderer::append_text_commands(
            hud.draw_list(),
            |id| hud.resolve_text(id),
            &mut vertices,
            &shots.font.font,
            viewport,
            crate::text::TextStyle::NEUTRAL,
        );
        shots.save_at(&name, hud.draw_list(), &vertices, true, viewport);
    }
}

fn force_wheel(shots: &mut Snapshot, vfs: &sjk_vfs::VirtualFileSystem) {
    use crate::hud::force_wheel::{ICONS, picture_names, snapshot};
    use sjk_client::force_wheel::{DASH, REPULSE, STASIS};
    let mut icons = [None; ICONS];
    for (slot, name) in picture_names() {
        let image = ["tga", "png", "jpg"]
            .into_iter()
            .find_map(|extension| decode(vfs, &format!("{name}.{extension}")));
        if let Some(image) = image {
            let id = sjk_ui::TextureId(crate::ui_renderer::FORCE_WHEEL_ICON_FIRST + slot as u32);
            shots.icons.insert(id.0, image);
            icons[slot] = Some(id);
        }
    }
    // Heal, Speed, Push, Pull, Mind Trick, Sense, Lightning, Grip, Drain.
    let powers = [0, 2, 3, 4, 5, 14, 7, 6, 13]
        .iter()
        .fold(0_u32, |bits, p| bits | 1 << p);
    let jof = (1 << STASIS) | (1 << REPULSE) | (1 << DASH);
    for (name, selected, flamethrower) in [
        ("hud-force-wheel-jof", REPULSE, false),
        ("hud-force-wheel-merc", 7, true),
    ] {
        let view = sjk_client::selection::SelectionView {
            inventory: false,
            available: powers | jof,
            selected,
            alpha: 1.0,
        };
        let (list, names) = snapshot(view, &icons, flamethrower, VIEWPORT);
        let mut vertices = Vec::new();
        crate::ui_renderer::append_text_commands(
            &list,
            names,
            &mut vertices,
            &shots.font.font,
            VIEWPORT,
            crate::text::TextStyle::NEUTRAL,
        );
        shots.save(name, &list, &vertices, true);
    }
}

/// The console's command browser in the classic+ look the classic console uses
/// (`console_browser_classic.rs`).
fn console_browser(shots: &Snapshot, art: ArtSet) {
    let directory = tempfile::tempdir().expect("scratch profile");
    let mut console =
        crate::console::ViewerConsole::new(directory.path().join("config.cfg")).expect("console");
    console.set_cvar("con_style", "classic");
    // A changed cvar shows its default beside it.
    console.set_cvar("con_height", "0.75");
    console.set_browser_art(art);
    console.open_browser_on("con_");
    let mut vertices = Vec::new();
    console.append_overlay(&mut vertices, &shots.font.font, VIEWPORT, 1.0);
    shots.save(
        "console-browser-classic",
        console.draw_list(),
        &vertices,
        true,
    );
}

/// The changelog page on two releases, the credits page, and the classic main
/// menu with its Changelog entry.
fn changelog(shots: &Snapshot, art: ArtSet) {
    let directory = tempfile::tempdir().expect("scratch profile");
    let mut console =
        crate::console::ViewerConsole::new(directory.path().join("config.cfg")).expect("console");
    console.open_changelog();
    console.set_page_art(art);
    for (name, release) in [("changelog-classic", 1), ("changelog-classic-alpha1", 3)] {
        console.changelog_mut().select(release);
        let mut vertices = Vec::new();
        console.append_overlay(&mut vertices, &shots.font.font, VIEWPORT, 1.0);
        shots.save(name, console.draw_list(), &vertices, true);
    }
    // Scrolled by pixels, or to a person's panel with their folds open.
    enum At {
        Pixels(f32),
        Person(u16),
    }
    for (name, at, viewport) in [
        ("credits", At::Pixels(0.0), VIEWPORT),
        ("credits-scrolled", At::Pixels(600.0), VIEWPORT),
        ("credits-wide", At::Pixels(0.0), [2560.0, 1080.0]),
        ("credits-end", At::Pixels(100_000.0), VIEWPORT),
        ("credits-unfolded", At::Person(0), VIEWPORT),
        ("credits-unfolded-bishop", At::Person(1), [2560.0, 1080.0]),
    ] {
        console.open_credits();
        let mut vertices = Vec::new();
        match at {
            At::Pixels(pixels) => {
                console.credits_mut().settle();
                console.credits_mut().scroll_to(pixels);
            }
            At::Person(person) => {
                console.credits_mut().unfold(person);
                console.credits_mut().settle();
                console.append_overlay(&mut vertices, &shots.font.font, viewport, 1.0);
                console.credits_mut().scroll_to_person(person);
            }
        }
        console.append_overlay(&mut vertices, &shots.font.font, viewport, 1.0);
        shots.save_at(name, console.draw_list(), &vertices, false, viewport);
    }
    let mut canvas = crate::menu_widgets::MenuCanvas::new();
    let mut classic = crate::menu::classic::ClassicMain::new();
    classic.select(2);
    crate::menu::classic::view::build(&mut canvas, VIEWPORT, &classic, 1.0, art);
    let mut vertices = Vec::new();
    canvas.append_text(&mut vertices, &shots.font.font, VIEWPORT);
    shots.save("main-classic", canvas.draw_list(), &vertices, false);
    let mut canvas = crate::menu_widgets::MenuCanvas::new();
    let mut sjk = crate::menu::classic::ClassicMain::new();
    sjk.show(Page::Sjk);
    crate::menu::classic::view::build(&mut canvas, VIEWPORT, &sjk, 1.0, art);
    let mut vertices = Vec::new();
    canvas.append_text(&mut vertices, &shots.font.font, VIEWPORT);
    shots.save("sjk-page", canvas.draw_list(), &vertices, false);
}

fn in_game_menu(shots: &Snapshot, art: ArtSet) {
    use crate::ingame_menu::{InGameMenu, Page as Popup, View};
    let pages: [(&str, Popup, usize, bool); 6] = [
        ("ingame-bar", Popup::Main, 2, true),
        ("ingame-sjk", Popup::Sjk, 0, true),
        ("ingame-join", Popup::Team, 1, true),
        ("ingame-vote", Popup::Vote, 0, true),
        ("ingame-exit", Popup::Leave, 0, false),
        ("ingame-callvote", Popup::CallVote, 0, true),
    ];
    for (name, page, selected_row, team_game) in pages {
        let mut menu = InGameMenu::new();
        menu.set_style(crate::menu::style::MenuStyle::Classic, art);
        let view = View {
            page,
            selected_row,
            team: 1,
            team_game,
            siege: false,
            red_players: 3,
            blue_players: 2,
            vote_active: page == Popup::Vote,
            staff: false,
            _frame: std::marker::PhantomData,
        };
        let mut vertices = Vec::new();
        menu.append(view, &mut vertices, &shots.font.font, VIEWPORT);
        shots.save(name, menu.draw_list(), &vertices, true);
    }
}

/// The medals' pictures as the renderer has them: each small medallion in its atlas
/// cell and each whole medal under its own texture (scaled down once, as the sampled
/// mip level would be, since these snapshots sample the nearest texel).
fn medal_icons(icons: &mut HashMap<u32, RgbaImage>) {
    for medal in crate::medals::Medal::ALL {
        let small = image::load_from_memory(medal.small_png()).expect("a medallion");
        icons.insert(medal.icon().0, small.into_rgba8());
        let art = image::load_from_memory(medal.art_png()).expect("a medal");
        let art = image::imageops::resize(
            &art.into_rgba8(),
            256,
            256,
            image::imageops::FilterType::Lanczos3,
        );
        icons.insert(medal.art().0, art);
    }
}

/// Made-up medals as the hub sends them in a profile.
fn shot_medals() -> Vec<sjk_identity::Medal> {
    let medal = |id: &str, count, awarded, note: &str| sjk_identity::Medal {
        id: id.to_owned(),
        count,
        awarded,
        note: note.to_owned(),
    };
    vec![
        medal("early_tester", 1, 1_790_208_000, ""),
        medal(
            "early_contributor",
            1,
            1_790_640_000,
            "Thank you for the ^3remaps^7 and the HUD work in the first weeks.",
        ),
        medal(
            "bug_hunter",
            3,
            1_791_336_225,
            "The fog that followed the camera floor, the flickering door on ffa3 and the lost lightmaps.",
        ),
    ]
}

/// The medals everywhere they show, on made-up hub data: the player card glanced at and
/// pinned, the Identity page in every look with medals and without, the SJK UI's at
/// 16:9 and 4:3, and the new medal pop-up alone and as the first of three.
#[test]
#[ignore = "reads the installed game data named by JKA_GAME_DATA"]
fn medals_snapshot() {
    use crate::console::identity_panel::{Inputs, Panel};
    use crate::game_font::SjkFonts;
    use crate::hud::player_card::{HubInfo, State, card_from};
    use crate::menu::sjk::TextTarget;
    use sjk_identity::{Profile, Snapshot as Hub, Status};
    let (art, vfs) = art();
    let mut shots = Snapshot {
        font: crate::text::load_modern(1.0, None).expect("build the menu font"),
        icons: HashMap::from([(crate::ui_renderer::LOGO_TEXTURE.0, logo_icon())]),
        in_match: match_backdrop(&vfs),
    };
    medal_icons(&mut shots.icons);
    // The player card, glanced at and pinned.
    let info = |text: &str| text.replace('|', "\\").into_bytes();
    let medals = crate::medals::Medals::from_wire(&shot_medals());
    for (name, pinned) in [("medals-card", false), ("medals-card-pinned", true)] {
        let card = card_from(
            &info("n|^1Sol^7 the Fox|t|1|model|kyle/default|st|single_1|st2|none|c1|4|c2|0|"),
            Some(HubInfo {
                name: "Sol".to_owned(),
                verified: true,
                medals,
                ..HubInfo::default()
            }),
            None,
        );
        let state = State::preview(card, [760.0, 470.0], VIEWPORT, pinned);
        let mut vertices = Vec::new();
        crate::ui_renderer::append_text_commands(
            &state.list,
            |id| state.resolve_text(id),
            &mut vertices,
            &shots.font.font,
            VIEWPORT,
            crate::text::TextStyle::NEUTRAL,
        );
        shots.save(name, &state.list, &vertices, true);
    }
    // The Identity page.
    let hub = |medals: Vec<sjk_identity::Medal>| Hub {
        loot: Vec::new(),
        opening: None,
        reveal: None,
        local_key_id: "44f3d0b36c9b2510".to_owned(),
        status: Status::Online,
        key_id: "44f3d0b36c9b2510".to_owned(),
        me: Some(Profile {
            credits: 0,
            cards: Vec::new(),
            keys: Vec::new(),
            key_id: "44f3d0b36c9b2510".to_owned(),
            key: String::new(),
            name: "^1Sol".to_owned(),
            bio: String::new(),
            verified: true,
            staff: false,
            created: 0,
            names: Vec::new(),
            medals,
            achievements: Vec::new(),
            avatar: String::new(),
            unlocks: Vec::new(),
            holocron_counts: Default::default(),
            holocrons: Vec::new(),
        }),
        server: None,
        players: Vec::new(),
        profiles: HashMap::new(),
        model_checks: HashMap::new(),
        profiles_revision: 0,
        notice: None,
        revision: 0,
        report: None,
        note: None,
        player_report: None,
        avatar: None,
        look_outcome: None,
        packs_revision: 0,
        assets_note: None,
        crashes_sent: 0,
        crash_note: None,
        holocrons: None,
        log_requests: Vec::new(),
        log_answer: None,
    };
    let with = hub(shot_medals());
    let without = hub(Vec::new());
    let key_file =
        "C:/Program Files (x86)/Steam/steamapps/common/Jedi Academy/GameData/SJK/identity.key";
    let inputs = |snapshot| Inputs {
        enabled: true,
        hub_url: "https://sjk.dfox.app",
        key_error: None,
        snapshot: Some(snapshot),
        key_file,
    };
    for (name, snapshot) in [
        ("medals-identity", &with),
        ("medals-identity-none", &without),
    ] {
        let mut panel = Panel::new();
        panel.open(false);
        panel.set_art(art);
        let mut vertices = Vec::new();
        panel.append(&inputs(snapshot), &mut vertices, &shots.font.font, VIEWPORT);
        shots.save(
            &format!("{name}-classic"),
            panel.draw_list(),
            &vertices,
            true,
        );
    }
    let display = crate::text::load_family(&crate::text::DISPLAY, 1.0, None).expect("Rajdhani");
    let body = crate::text::load_family(&crate::text::BODY, 1.0, None).expect("Exo 2");
    let backdrop = decode(&vfs, "levelshots/mp/duel6.jpg").expect("the duel6 levelshot");
    // The key's id and file stay hidden but in the last, after Show.
    for (name, snapshot, viewport, revealed) in [
        ("medals-identity-sjk", &with, VIEWPORT_WIDE, false),
        ("medals-identity-sjk-none", &without, VIEWPORT_WIDE, false),
        ("medals-identity-sjk-4x3", &with, VIEWPORT, false),
        ("medals-identity-sjk-shown", &with, VIEWPORT_WIDE, true),
    ] {
        let mut panel = Panel::new();
        panel.open(false);
        panel.set_sjk(true);
        if revealed {
            panel.reveal_for_shot();
        }
        let (mut display_text, mut body_text) = (Vec::new(), Vec::new());
        panel.append_sjk(
            &inputs(snapshot),
            TextTarget::Families(
                SjkFonts {
                    display: (&mut display_text, &display.font),
                    body: (&mut body_text, &body.font),
                    code: None,
                },
                crate::text::TextStyle::NEUTRAL,
            ),
            viewport,
        );
        let fonts = SjkShotFonts {
            display: (&display_text, &display.image),
            body: (&body_text, &body.image),
        };
        save_sjk_shot(
            name,
            viewport,
            &backdrop,
            panel.draw_list(),
            &shots.icons,
            fonts,
        );
    }
    // The new medal pop-up: the SJK UI's over the duel6 levelshot at moments of its
    // ceremony (held still), then the classic+ one.
    use crate::medal_popup::MedalPopup;
    use crate::medal_popup::award::{ENTRANCE, EXIT};
    use crate::menu::style::MenuStyle;
    let awards = crate::medals::awards(&shot_medals());
    let hunter = || vec![awards[2].clone()];
    for (name, shown, viewport, at, leaving) in [
        ("medals-popup", hunter(), VIEWPORT_WIDE, ENTRANCE, None),
        ("medals-popup-flight", hunter(), VIEWPORT_WIDE, 0.3, None),
        ("medals-popup-landing", hunter(), VIEWPORT_WIDE, 0.55, None),
        ("medals-popup-burst", hunter(), VIEWPORT_WIDE, 0.8, None),
        ("medals-popup-sweep", hunter(), VIEWPORT_WIDE, 1.3, None),
        (
            "medals-popup-glint",
            hunter(),
            VIEWPORT_WIDE,
            ENTRANCE + 1.95,
            None,
        ),
        (
            "medals-popup-leaving",
            hunter(),
            VIEWPORT_WIDE,
            ENTRANCE + 2.0,
            Some(EXIT * 0.5),
        ),
        (
            "medals-popup-first-of-three",
            awards[..3].to_vec(),
            VIEWPORT_WIDE,
            ENTRANCE,
            None,
        ),
        (
            "medals-popup-4x3",
            vec![awards[1].clone()],
            VIEWPORT,
            ENTRANCE,
            None,
        ),
    ] {
        let mut popup = MedalPopup::preview_in(MenuStyle::Sjk, shown, at, leaving);
        let (mut display_text, mut body_text) = (Vec::new(), Vec::new());
        popup.append_sjk(
            TextTarget::Families(
                SjkFonts {
                    display: (&mut display_text, &display.font),
                    body: (&mut body_text, &body.font),
                    code: None,
                },
                crate::text::TextStyle::NEUTRAL,
            ),
            viewport,
            std::time::Instant::now(),
        );
        let fonts = SjkShotFonts {
            display: (&display_text, &display.image),
            body: (&body_text, &body.image),
        };
        save_sjk_shot(
            name,
            viewport,
            &backdrop,
            popup.draw_list(),
            &shots.icons,
            fonts,
        );
    }
    for (name, shown, at) in [
        ("medals-popup-classic", hunter(), ENTRANCE),
        ("medals-popup-classic-burst", hunter(), 0.8),
        (
            "medals-popup-classic-first-of-three",
            awards[..3].to_vec(),
            ENTRANCE,
        ),
    ] {
        let mut popup = MedalPopup::preview_in(MenuStyle::Classic, shown, at, None);
        popup.set_style(MenuStyle::Classic, art);
        let mut vertices = Vec::new();
        popup.append_classic(
            &mut vertices,
            &shots.font.font,
            VIEWPORT,
            std::time::Instant::now(),
        );
        shots.save_at(name, popup.draw_list(), &vertices, true, VIEWPORT);
    }
}
