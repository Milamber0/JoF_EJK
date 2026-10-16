//! The player's own retail menu artwork for the classic menu style.
//!
//! The classic menus draw the original `gfx/menus` images (backgrounds,
//! window frames, the game logo, the in-game bar and boxes) when the mounted
//! game data has them. Nothing is bundled: a piece that is missing or fails
//! to decode is simply absent, and the classic views fall back to SJK's own
//! vector drawing for it.
//!
//! The images are decoded once per process on a worker thread, the first
//! time the classic style is in use ([`request`]), and kept as RGBA so each
//! world's UI renderer can upload them without decoding again
//! ([`decoded`]). The renderer gives every piece its own texture and bind
//! group (`ui_renderer/art.rs`) rather than a slot in the shared UI icon
//! atlas, which has no room for full-screen art.
//!
//! Retail animates some pieces in their shaders (`shaders/ui.shader`); the
//! classic views and the renderer reproduce that motion ([`motion`]): the
//! ring turns, the side glyphs and the logo's reflection scroll (their
//! textures wrap, [`ArtPiece::wraps`]) and the glows flicker with
//! `gfx/hud/static_menu`. The flickering pieces keep their raw image for that
//! ([`Decoded::flicker_base`]). Retail's ring played the `video/ja01` logo;
//! SJK shows its own emblem there instead ([`super::emblem`]), so the video
//! is not read.

pub(crate) mod motion;

use image::RgbaImage;
use sjk_ui::TextureId;
use sjk_vfs::VirtualFileSystem;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

/// First `TextureId` naming a piece; far from the atlas cell indices and
/// the atlas' reserved ids at the top of the range.
const TEXTURE_BASE: u32 = 0x4000_0000;
/// Longest side kept for a piece; larger HD replacements are scaled down.
const MAX_SIDE: u32 = 4_096;
/// Longest side of a flickering piece's raw image, recomposed on the CPU
/// every frame it is drawn.
const MAX_FLICKER_SIDE: u32 = 512;
/// The noise retail's glow shaders multiply the screen by.
const STATIC_PATH: &str = "gfx/hud/static_menu";

/// How the retail shader blends a piece over the screen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Blend {
    /// `GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA`, or an opaque image.
    Alpha,
    /// `GL_ONE GL_ONE` (or the near-identical screen blend of the in-game
    /// bar): black is transparent and light adds. Converted to alpha at
    /// decode time so the shared alpha-blended UI pipeline can draw it.
    Additive,
}

/// One retail menu image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArtPiece {
    /// `main_background`: the full-screen backdrop.
    Background,
    /// `menu_side_text`: the glyph column down the left edge.
    SideLeft,
    /// `menu_side_text_right`: the glyph column down the right edge.
    SideRight,
    /// `main_ring`: the ring around the centre of the main menu.
    Ring,
    /// `main_centerwindow`: the centre window of the main menu.
    CenterWindow,
    /// `main_leftwindow`: the frame behind Play and Profile.
    LeftWindow,
    /// `main_rightwindow`: the frame behind Controls and Setup.
    RightWindow,
    /// `jediacademy`: the game logo across the top.
    Logo,
    /// `main_centerblue`: the panel behind the sub-pages' centre area.
    CenterBlue,
    /// `menu_boxes_left`: the sub-pages' top-left frame.
    BoxesLeft,
    /// `menu_boxes_right`: the sub-pages' top-right frame.
    BoxesRight,
    /// `menu_buttonback`: the glow behind a focused button.
    ButtonBack,
    /// `menu_blendbox`: the band behind page titles.
    BlendBox,
    /// `menu_top_mp`: the in-game menu's top bar.
    TopBar,
    /// `menu_box_dark` (shader `menu_box_ingame`): the in-game pop-up box.
    PopupBox,
    /// `menu/art/unknownmap_mp`: the connect screen's background
    /// (`ui/jamp/connect.menu`), also the loading screen's when a map has no
    /// levelshot.
    UnknownMap,
    /// `gfx/hud/mp_levelload`: the loading bar's surround (`CG_LoadBar`).
    LoadFrame,
    /// `gfx/hud/load_tick` (image `load_tick2`): the loading bar's fill.
    LoadTick,
    /// `gfx/hud/load_tick_cap`: the cap at the fill's right end.
    LoadCap,
    /// The same cap mirrored, for the fill's left end, which retail draws
    /// with a negative width.
    LoadCapLeft,
    /// `menu_buttonback2` (shader `menu_blendbox2`): the glow behind the
    /// focused entry of a Setup or Controls list.
    BlendBox2,
    /// `menu/new/slider`: the option panels' slider bar.
    Slider,
    /// `menu/new/sliderthumb`: the option panels' slider thumb.
    SliderThumb,
    /// `charmenu`: the full-screen backdrop of character creation.
    CharMenu,
    /// `charmenu_bottom`: the frame under character creation's model.
    CharMenuBottom,
    /// `sabermenu_back`: the full-screen backdrop of lightsaber creation.
    SaberBack,
    /// `sabermenu_box`: the saber type box.
    SaberBox,
    /// `sabermenu_box_top`: top of the stretchable blade colour box.
    SaberBoxTop,
    /// `sabermenu_box_middle`: middle of the stretchable blade colour box.
    SaberBoxMiddle,
    /// `sabermenu_box_bottom`: bottom of the stretchable blade colour box.
    SaberBoxBottom,
    /// `gfx/mp/custom_mp_default`: the profile's Custom character button.
    CustomPlayer,
    /// `saberonly`: the in-game profile's Saber button.
    SaberOnly,
    /// `saber_icon_blue`: blade colour swatch.
    SaberBlue,
    /// `saber_icon_green`: blade colour swatch.
    SaberGreen,
    /// `saber_icon_orange`: blade colour swatch.
    SaberOrange,
    /// `saber_icon_purple`: blade colour swatch.
    SaberPurple,
    /// `saber_icon_yellow`: blade colour swatch.
    SaberYellow,
    /// `saber_icon_red`: blade colour swatch.
    SaberRed,
    /// `menu_side_text_b`: the side columns' backdrop, under the scrolling
    /// glyphs of [`Self::SideLeft`] and [`Self::SideRight`].
    SideBase,
    /// `jediacademy` drawn opaque: the logo shader's first stage, which the
    /// reflection ([`Self::EnvLogo`]) and the logo itself go over.
    LogoBase,
    /// `env_logo`: the reflection scrolling through the logo's letters.
    EnvLogo,
    /// `menu_blendboxr`: the red band behind the dark side's Force powers.
    BlendBoxRed,
    /// `configforce`: the in-game profile's Force configuration button.
    ConfigForce,
    /// `forcestar0` to `forcestar8`: a bought Force level, numbered with
    /// what it cost (`uiForceStarShaders`, `ui_force.c`); `forcestar0` also
    /// stands for a free level not yet bought.
    ForceStar0,
    ForceStar1,
    ForceStar2,
    ForceStar3,
    ForceStar4,
    ForceStar5,
    ForceStar6,
    ForceStar7,
    ForceStar8,
    /// `forcecircle1` to `forcecircle8`: a Force level not bought yet,
    /// numbered with what it costs.
    ForceCircle1,
    ForceCircle2,
    ForceCircle3,
    ForceCircle4,
    ForceCircle5,
    ForceCircle6,
    ForceCircle7,
    ForceCircle8,
    /// `gfx/hud/mpi_rflag`: the red team's flag, beside Team Red in the
    /// in-game Join pop-up (classic+).
    RedFlag,
    /// `gfx/hud/mpi_bflag`: the blue team's flag, beside Team Blue.
    BlueFlag,
}

/// The stars of [`ArtPiece::force_level`], by cost.
const FORCE_STARS: [ArtPiece; 9] = [
    ArtPiece::ForceStar0,
    ArtPiece::ForceStar1,
    ArtPiece::ForceStar2,
    ArtPiece::ForceStar3,
    ArtPiece::ForceStar4,
    ArtPiece::ForceStar5,
    ArtPiece::ForceStar6,
    ArtPiece::ForceStar7,
    ArtPiece::ForceStar8,
];
/// The circles of [`ArtPiece::force_level`], by cost (`forcestar0` for 0).
const FORCE_CIRCLES: [ArtPiece; 9] = [
    ArtPiece::ForceStar0,
    ArtPiece::ForceCircle1,
    ArtPiece::ForceCircle2,
    ArtPiece::ForceCircle3,
    ArtPiece::ForceCircle4,
    ArtPiece::ForceCircle5,
    ArtPiece::ForceCircle6,
    ArtPiece::ForceCircle7,
    ArtPiece::ForceCircle8,
];

impl ArtPiece {
    /// The image retail draws for a Force level costing `cost` points:
    /// the star once `bought`, the circle before (`UI_DrawForceStars`).
    pub(crate) fn force_level(cost: u8, bought: bool) -> Self {
        let table = if bought { &FORCE_STARS } else { &FORCE_CIRCLES };
        table[usize::from(cost).min(table.len() - 1)]
    }

    /// Every piece, in [`ArtPiece`] order.
    pub(crate) const ALL: [Self; 62] = [
        Self::Background,
        Self::SideLeft,
        Self::SideRight,
        Self::Ring,
        Self::CenterWindow,
        Self::LeftWindow,
        Self::RightWindow,
        Self::Logo,
        Self::CenterBlue,
        Self::BoxesLeft,
        Self::BoxesRight,
        Self::ButtonBack,
        Self::BlendBox,
        Self::TopBar,
        Self::PopupBox,
        Self::UnknownMap,
        Self::LoadFrame,
        Self::LoadTick,
        Self::LoadCap,
        Self::LoadCapLeft,
        Self::BlendBox2,
        Self::Slider,
        Self::SliderThumb,
        Self::CharMenu,
        Self::CharMenuBottom,
        Self::SaberBack,
        Self::SaberBox,
        Self::SaberBoxTop,
        Self::SaberBoxMiddle,
        Self::SaberBoxBottom,
        Self::CustomPlayer,
        Self::SaberOnly,
        Self::SaberBlue,
        Self::SaberGreen,
        Self::SaberOrange,
        Self::SaberPurple,
        Self::SaberYellow,
        Self::SaberRed,
        Self::SideBase,
        Self::LogoBase,
        Self::EnvLogo,
        Self::BlendBoxRed,
        Self::ConfigForce,
        Self::ForceStar0,
        Self::ForceStar1,
        Self::ForceStar2,
        Self::ForceStar3,
        Self::ForceStar4,
        Self::ForceStar5,
        Self::ForceStar6,
        Self::ForceStar7,
        Self::ForceStar8,
        Self::ForceCircle1,
        Self::ForceCircle2,
        Self::ForceCircle3,
        Self::ForceCircle4,
        Self::ForceCircle5,
        Self::ForceCircle6,
        Self::ForceCircle7,
        Self::ForceCircle8,
        Self::RedFlag,
        Self::BlueFlag,
    ];
    pub(crate) const COUNT: usize = Self::ALL.len();

    /// Image path in game data, without extension.
    fn path(self) -> &'static str {
        match self {
            Self::Background => "gfx/menus/main_background",
            Self::SideLeft => "gfx/menus/menu_side_text",
            Self::SideRight => "gfx/menus/menu_side_text_right",
            Self::Ring => "gfx/menus/main_ring",
            Self::CenterWindow => "gfx/menus/main_centerwindow",
            Self::LeftWindow => "gfx/menus/main_leftwindow",
            Self::RightWindow => "gfx/menus/main_rightwindow",
            Self::Logo => "gfx/menus/jediacademy",
            Self::CenterBlue => "gfx/menus/main_centerblue",
            Self::BoxesLeft => "gfx/menus/menu_boxes_left",
            Self::BoxesRight => "gfx/menus/menu_boxes_right",
            Self::ButtonBack => "gfx/menus/menu_buttonback",
            Self::BlendBox => "gfx/menus/menu_blendbox",
            Self::TopBar => "gfx/menus/menu_top_mp",
            Self::PopupBox => "gfx/menus/menu_box_dark",
            Self::UnknownMap => "menu/art/unknownmap_mp",
            Self::LoadFrame => "gfx/hud/mp_levelload",
            Self::LoadTick => "gfx/hud/load_tick2",
            Self::LoadCap | Self::LoadCapLeft => "gfx/hud/load_tick_cap",
            Self::BlendBox2 => "gfx/menus/menu_buttonback2",
            Self::Slider => "menu/new/slider",
            Self::SliderThumb => "menu/new/sliderthumb",
            Self::CharMenu => "gfx/menus/charmenu",
            Self::CharMenuBottom => "gfx/menus/charmenu_bottom",
            Self::SaberBack => "gfx/menus/sabermenu_back",
            Self::SaberBox => "gfx/menus/sabermenu_box",
            Self::SaberBoxTop => "gfx/menus/sabermenu_box_top",
            Self::SaberBoxMiddle => "gfx/menus/sabermenu_box_middle",
            Self::SaberBoxBottom => "gfx/menus/sabermenu_box_bottom",
            Self::CustomPlayer => "gfx/mp/custom_mp_default",
            Self::SaberOnly => "gfx/menus/saberonly",
            Self::SaberBlue => "gfx/menus/saber_icon_blue",
            Self::SaberGreen => "gfx/menus/saber_icon_green",
            Self::SaberOrange => "gfx/menus/saber_icon_orange",
            Self::SaberPurple => "gfx/menus/saber_icon_purple",
            Self::SaberYellow => "gfx/menus/saber_icon_yellow",
            Self::SaberRed => "gfx/menus/saber_icon_red",
            Self::SideBase => "gfx/menus/menu_side_text_b",
            Self::LogoBase => "gfx/menus/jediacademy",
            Self::EnvLogo => "gfx/menus/env_logo",
            Self::BlendBoxRed => "gfx/menus/menu_blendboxr",
            Self::ConfigForce => "gfx/menus/configforce",
            Self::ForceStar0 => "gfx/menus/forcestar0",
            Self::ForceStar1 => "gfx/menus/forcestar1",
            Self::ForceStar2 => "gfx/menus/forcestar2",
            Self::ForceStar3 => "gfx/menus/forcestar3",
            Self::ForceStar4 => "gfx/menus/forcestar4",
            Self::ForceStar5 => "gfx/menus/forcestar5",
            Self::ForceStar6 => "gfx/menus/forcestar6",
            Self::ForceStar7 => "gfx/menus/forcestar7",
            Self::ForceStar8 => "gfx/menus/forcestar8",
            Self::ForceCircle1 => "gfx/menus/forcecircle1",
            Self::ForceCircle2 => "gfx/menus/forcecircle2",
            Self::ForceCircle3 => "gfx/menus/forcecircle3",
            Self::ForceCircle4 => "gfx/menus/forcecircle4",
            Self::ForceCircle5 => "gfx/menus/forcecircle5",
            Self::ForceCircle6 => "gfx/menus/forcecircle6",
            Self::ForceCircle7 => "gfx/menus/forcecircle7",
            Self::ForceCircle8 => "gfx/menus/forcecircle8",
            Self::RedFlag => "gfx/hud/mpi_rflag",
            Self::BlueFlag => "gfx/hud/mpi_bflag",
        }
    }

    /// Whether the piece's texture repeats: retail scrolls its texture
    /// coordinates (`tcMod scroll`), so the image wraps around.
    pub(crate) fn wraps(self) -> bool {
        matches!(self, Self::SideLeft | Self::SideRight | Self::EnvLogo)
    }

    /// Whether the renderer rewrites the piece's texture as it animates:
    /// the flickering glows.
    pub(crate) fn dynamic(self) -> bool {
        motion::flicker(self).is_some()
    }

    /// Blend of the piece's retail shader (`shaders/ui.shader`).
    fn blend(self) -> Blend {
        match self {
            Self::ButtonBack
            | Self::BlendBox
            | Self::TopBar
            | Self::LoadFrame
            | Self::LoadTick
            | Self::LoadCap
            | Self::LoadCapLeft
            | Self::BlendBox2
            | Self::Slider
            | Self::SliderThumb
            | Self::BlendBoxRed => Blend::Additive,
            piece if FORCE_STARS.contains(&piece) || FORCE_CIRCLES.contains(&piece) => {
                Blend::Additive
            }
            _ => Blend::Alpha,
        }
    }

    /// Position in [`Self::ALL`].
    pub(crate) fn index(self) -> usize {
        self as usize
    }

    /// The `TexturedQuad` texture that draws this piece.
    pub(crate) fn texture(self) -> TextureId {
        TextureId(TEXTURE_BASE + self as u32)
    }

    /// The piece a `TexturedQuad` texture names, if it names one.
    pub(crate) fn from_texture(texture: TextureId) -> Option<Self> {
        let index = texture.0.checked_sub(TEXTURE_BASE)?;
        Self::ALL.get(index as usize).copied()
    }
}

/// Which pieces are ready to draw.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ArtSet(u64);

impl ArtSet {
    /// Whether `piece` can be drawn.
    pub(crate) fn has(self, piece: ArtPiece) -> bool {
        self.0 & (1_u64 << piece.index()) != 0
    }

    /// This set with `piece` added.
    pub(crate) fn with(self, piece: ArtPiece) -> Self {
        Self(self.0 | 1_u64 << piece.index())
    }
}

/// The decoded pieces, indexed by [`ArtPiece::index`].
pub(crate) struct Decoded {
    images: [Option<RgbaImage>; ArtPiece::COUNT],
    /// Raw (additive) images of the flickering pieces, by piece index.
    flicker_bases: [Option<RgbaImage>; ArtPiece::COUNT],
    /// `gfx/hud/static_menu`, the flicker noise.
    noise: Option<RgbaImage>,
}

impl Decoded {
    /// The RGBA image of `piece`, ready to upload, if game data has it.
    pub(crate) fn image(&self, piece: ArtPiece) -> Option<&RgbaImage> {
        self.images[piece.index()].as_ref()
    }

    /// The raw image and the noise a flickering piece is recomposed from,
    /// when game data has both.
    pub(crate) fn flicker_base(&self, piece: ArtPiece) -> Option<(&RgbaImage, &RgbaImage)> {
        Some((
            self.flicker_bases[piece.index()].as_ref()?,
            self.noise.as_ref()?,
        ))
    }
}

static DECODED: OnceLock<Decoded> = OnceLock::new();
static REQUESTED: AtomicBool = AtomicBool::new(false);

/// Start decoding the artwork from `vfs` on a worker thread, once per
/// process; later calls do nothing.
pub(crate) fn request(vfs: &Arc<VirtualFileSystem>) {
    if REQUESTED.swap(true, Ordering::AcqRel) {
        return;
    }
    let vfs = Arc::clone(vfs);
    let spawned = std::thread::Builder::new()
        .name("sjk-menu-art".into())
        .spawn(move || {
            let decoded = decode_all(&vfs);
            let found = decoded.images.iter().flatten().count();
            crate::log::progress(format_args!(
                "classic menu art: {found} of {} pieces found in game data",
                ArtPiece::COUNT
            ));
            let _ = DECODED.set(decoded);
        });
    if let Err(error) = spawned {
        crate::log::progress(format_args!(
            "warning: classic menu art not loaded, drawing without it: {error}"
        ));
    }
}

/// The decoded artwork, once the worker has finished.
pub(crate) fn decoded() -> Option<&'static Decoded> {
    DECODED.get()
}

fn decode_all(vfs: &VirtualFileSystem) -> Decoded {
    let raw = ArtPiece::ALL.map(|piece| read_image(vfs, piece.path()));
    let flicker_bases = std::array::from_fn(|index| {
        motion::flicker(ArtPiece::ALL[index])?;
        raw[index]
            .as_ref()
            .map(|image| limit(image.clone(), MAX_FLICKER_SIDE))
    });
    let mut index = 0;
    let images = raw.map(|image| {
        let piece = ArtPiece::ALL[index];
        index += 1;
        image.map(|image| prepare(piece, image))
    });
    Decoded {
        images,
        flicker_bases,
        noise: read_image(vfs, STATIC_PATH),
    }
}

/// Read and decode the image at `path`, trying the extensions retail
/// resolves an extensionless image name with.
fn read_image(vfs: &VirtualFileSystem, path: &str) -> Option<RgbaImage> {
    let image = ["tga", "jpg", "png"].iter().find_map(|extension| {
        let path = format!("{path}.{extension}");
        let asset = vfs.read(&path).ok()??;
        crate::decode_image(&asset.bytes, &path)
            .ok()
            .map(image::DynamicImage::into_rgba8)
    })?;
    let (width, height) = image.dimensions();
    (width != 0 && height != 0).then(|| limit(image, MAX_SIDE))
}

/// `image`, scaled down to at most `side` on its longest side.
fn limit(image: RgbaImage, side: u32) -> RgbaImage {
    let (width, height) = image.dimensions();
    if width.max(height) <= side {
        return image;
    }
    let scale = side as f32 / width.max(height) as f32;
    image::imageops::resize(
        &image,
        ((width as f32 * scale) as u32).max(1),
        ((height as f32 * scale) as u32).max(1),
        image::imageops::FilterType::Triangle,
    )
}

/// Turn a piece's raw image into the one uploaded for it.
fn prepare(piece: ArtPiece, mut image: RgbaImage) -> RgbaImage {
    if piece == ArtPiece::LoadCapLeft {
        image::imageops::flip_horizontal_in_place(&mut image);
    }
    if piece == ArtPiece::LogoBase {
        for pixel in image.pixels_mut() {
            pixel.0[3] = 255;
        }
    }
    if piece.blend() == Blend::Additive {
        additive_to_alpha(&mut image);
    }
    image
}

/// Re-express an additively blended image for alpha blending: the
/// brightest channel becomes the alpha and the colour is divided by it, so
/// `colour * alpha + under * (1 - alpha)` adds the original colour and only
/// slightly darkens what lies under bright texels.
fn additive_to_alpha(image: &mut RgbaImage) {
    for pixel in image.pixels_mut() {
        pixel.0 = additive_texel(pixel.0);
    }
}

/// One texel of [`additive_to_alpha`].
pub(crate) fn additive_texel([r, g, b, a]: [u8; 4]) -> [u8; 4] {
    let peak = r.max(g).max(b);
    if peak == 0 {
        return [0, 0, 0, 0];
    }
    let scale =
        |channel: u8| ((u16::from(channel) * 255 + u16::from(peak) / 2) / u16::from(peak)) as u8;
    let alpha = (u16::from(peak) * u16::from(a) / 255) as u8;
    [scale(r), scale(g), scale(b), alpha]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textures_round_trip_and_stay_clear_of_the_atlas() {
        for piece in ArtPiece::ALL {
            assert_eq!(ArtPiece::from_texture(piece.texture()), Some(piece));
            assert_eq!(ArtPiece::ALL[piece.index()], piece);
        }
        assert_eq!(ArtPiece::from_texture(TextureId(0)), None);
        assert_eq!(ArtPiece::from_texture(TextureId(u32::MAX)), None);
        assert_eq!(ArtPiece::from_texture(TextureId(u32::MAX - 1)), None);
    }

    #[test]
    fn every_piece_fits_the_set() {
        assert!(ArtPiece::COUNT <= u64::BITS as usize);
        let all = ArtPiece::ALL
            .iter()
            .fold(ArtSet::default(), |set, piece| set.with(*piece));
        assert!(ArtPiece::ALL.iter().all(|piece| all.has(*piece)));
    }

    #[test]
    fn force_levels_draw_their_cost_as_retail_does() {
        assert_eq!(ArtPiece::force_level(3, true), ArtPiece::ForceStar3);
        assert_eq!(ArtPiece::force_level(3, false), ArtPiece::ForceCircle3);
        // `uiForceStarShaders[0]` is `forcestar0` both ways.
        assert_eq!(ArtPiece::force_level(0, false), ArtPiece::ForceStar0);
        assert_eq!(ArtPiece::force_level(0, true), ArtPiece::ForceStar0);
        assert_eq!(ArtPiece::force_level(9, false), ArtPiece::ForceCircle8);
        assert_eq!(ArtPiece::ForceCircle8.blend(), Blend::Additive);
        assert_eq!(ArtPiece::ConfigForce.blend(), Blend::Alpha);
    }

    #[test]
    fn art_set_tracks_pieces() {
        let set = ArtSet::default().with(ArtPiece::Ring).with(ArtPiece::Logo);
        assert!(set.has(ArtPiece::Ring) && set.has(ArtPiece::Logo));
        assert!(!set.has(ArtPiece::Background));
    }

    #[test]
    fn additive_black_is_transparent_and_light_keeps_its_colour() {
        let mut image = RgbaImage::from_raw(
            3,
            1,
            vec![0, 0, 0, 255, 200, 100, 0, 255, 255, 255, 255, 255],
        )
        .expect("3x1 image");
        additive_to_alpha(&mut image);
        assert_eq!(image.get_pixel(0, 0).0, [0, 0, 0, 0]);
        // Alpha-blending the converted texel over black gives the original.
        let [r, g, b, a] = image.get_pixel(1, 0).0;
        let over_black = |c: u8| (u16::from(c) * u16::from(a) / 255) as u8;
        assert_eq!(a, 200);
        assert!(over_black(r).abs_diff(200) <= 1);
        assert!(over_black(g).abs_diff(100) <= 1);
        assert_eq!(over_black(b), 0);
        assert_eq!(image.get_pixel(2, 0).0, [255, 255, 255, 255]);
    }
}
