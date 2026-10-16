//! Drawing of the classic profile pages in retail item order: backdrop art,
//! frames and titles, then the entries, lists and the description line.
//! Colours are the retail `forecolor`/`backcolor` values of the items. The
//! Force page and the cosmetics window draw in [`super::force_page`] and
//! [`super::cosmetics_page`] with the helpers here.

use super::layout::{
    self, GRID_CELL, GRID_COLUMNS, HILT_ROW, Item, PART_CELL, SWATCH, TINT_CELL, place, rect,
    swatch_step, window,
};
use super::{BLADE_SWATCHES, ClassicPage, Frame};
use crate::menu::art::ArtPiece;
use crate::menu::classic::layout::{CANVAS, HINT_Y, LOGO, Placement};
use crate::menu::classic::view::{FOCUS, GOLD, art, glow, ink};
use crate::player_menu::grid::{GRID_SCROLL_TOKEN, MAX_VISIBLE_TILES, TILE_BASE};
use crate::player_menu::part_icons::Part;
use crate::player_menu::saber::{SaberStyle, allowed};
use crate::player_menu::team_filter::TeamSkin;
use crate::player_menu::{PlayerMenu, catalog_of};
use crate::text::{TextVertex, UiFont};
use sjk_ui::{Color, DrawCommand, FontWeight, Gradient, Rect, TextAlign};

/// Retail section label colour (`forecolor .549 .854 1`).
pub(super) const LABEL: Color = Color::new(0.549, 0.854, 1.0, 1.0);
/// Retail field value colour (`forecolor .615 .615 .956`).
pub(super) const VALUE: Color = Color::new(0.615, 0.615, 0.956, 1.0);
/// Retail frame blue (`bordercolor`/`backcolor .298 .305 .690`).
pub(super) const FRAME: Color = Color::new(0.298, 0.305, 0.690, 1.0);
/// Saber type entries not chosen (`setitemcolor ... .65 .65 1`).
const UNCHOSEN: Color = Color::new(0.65, 0.65, 1.0, 1.0);
/// Retail description colour (`descColor 1 .682 0 .8`).
const HINT: Color = Color::new(1.0, 0.682, 0.0, 0.8);
/// List boxes of character creation (`backcolor .66 .66 1 .25`).
pub(super) const LIST_BACK: Color = Color::new(0.66, 0.66, 1.0, 0.25);
pub(super) const LIST_BORDER: Color = Color::new(0.66, 0.66, 1.0, 1.0);
/// Light and dark side tints of the profile's Force summary.
const LIGHT_SIDE: Color = Color::new(0.5, 0.5, 1.0, 1.0);
const DARK_SIDE: Color = Color::new(1.0, 0.35, 0.3, 1.0);
/// Hilt list box (`backcolor 0 0 .5 .25`, `bordercolor 0 0 .8 1`).
const HILT_BACK: Color = Color::new(0.0, 0.0, 0.5, 0.25);
const HILT_BORDER: Color = Color::new(0.0, 0.0, 0.8, 1.0);

/// First token of the part list's cells.
pub(super) const PART_BASE: u16 = 300;
/// First token of the tint list's swatches.
pub(super) const TINT_BASE: u16 = 360;
/// First token of each hilt list's rows.
pub(super) const HILT_BASE: [u16; 2] = [400, 500];
/// First token of each blade swatch row.
pub(super) const BLADE_BASE: [u16; 2] = [600, 610];
/// Wheel targets of the part, tint and hilt lists.
pub(super) const PARTS_SCROLL: u16 = 903;
pub(super) const TINTS_SCROLL: u16 = 904;
pub(super) const HILTS_SCROLL: [u16; 2] = [905, 906];
/// First token of the Force page's level stars (three per power).
pub(super) const STAR_BASE: u16 = 620;
/// First token of the cosmetics window's hat and cape rows, and the
/// lists' wheel targets.
pub(super) const HAT_BASE: u16 = 1000;
pub(super) const CAPE_BASE: u16 = 1400;
pub(super) const HATS_SCROLL: u16 = 907;
pub(super) const CAPES_SCROLL: u16 = 908;
/// First token of the Force page's template rows, and the list's wheel target.
pub(super) const TEMPLATE_BASE: u16 = 1700;
pub(super) const TEMPLATES_SCROLL: u16 = 909;

/// Blade swatch art, in [`BLADE_SWATCHES`] order.
const SWATCH_ART: [ArtPiece; 6] = [
    ArtPiece::SaberBlue,
    ArtPiece::SaberGreen,
    ArtPiece::SaberOrange,
    ArtPiece::SaberPurple,
    ArtPiece::SaberYellow,
    ArtPiece::SaberRed,
];

impl PlayerMenu {
    /// Draw the classic profile pages at `reveal` opacity; the SJK UI draws its
    /// own view (`append_sjk`).
    pub(crate) fn append(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        reveal: f32,
    ) {
        let place = Placement::new(viewport);
        let page = self.classic.page;
        let frame = self.frame();
        self.canvas.begin_transparent(viewport);
        self.canvas.push_opacity(reveal);
        if frame == Frame::Full {
            self.full_backdrop(viewport, &place);
        }
        match page {
            ClassicPage::Player => self.player_page(&place, frame),
            ClassicPage::Character => self.character_page(&place, frame),
            ClassicPage::Saber => self.saber_page(&place, frame),
            ClassicPage::Force => self.force_page(&place, frame),
            ClassicPage::Cosmetics => self.cosmetics_page(&place, frame),
        }
        let described = self.entries(&place, page, frame);
        if page == ClassicPage::Force {
            self.force_detail(&place, frame, described);
        }
        self.description(&place, page, frame, described);
        self.canvas.pop_opacity();
        self.canvas.finish(self.classic.focus as u16);
        self.canvas.append_text(vertices, font, viewport);
    }

    pub(super) fn piece(&mut self, place: &Placement, piece: ArtPiece, canvas: [f32; 4]) {
        if self.classic.art.has(piece) {
            art(&mut self.canvas, piece, place.rect(canvas));
        }
    }

    pub(super) fn fill(&mut self, place: &Placement, canvas: [f32; 4], color: Color) {
        let rect = place.rect(canvas);
        let _ = self
            .canvas
            .draw_list_mut()
            .push(DrawCommand::SolidRect { rect, color });
    }

    pub(super) fn border(&mut self, place: &Placement, canvas: [f32; 4], color: Color, width: f32) {
        let rect = place.rect(canvas);
        let _ = self.canvas.draw_list_mut().push(DrawCommand::Border {
            rect,
            radius: 0.0,
            width: width * place.scale,
            color,
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn label(
        &mut self,
        place: &Placement,
        canvas: [f32; 4],
        text: &str,
        size: f32,
        color: Color,
        weight: FontWeight,
        align: TextAlign,
    ) {
        let s = place.scale;
        let rect = line_rect(place, canvas, size);
        self.canvas
            .text_aligned(text, rect, size * s, color, weight, 0.6 * s, align);
    }

    /// [`Self::label`] for formatted text, without allocating.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn label_fmt(
        &mut self,
        place: &Placement,
        canvas: [f32; 4],
        text: std::fmt::Arguments<'_>,
        size: f32,
        color: Color,
        weight: FontWeight,
        align: TextAlign,
    ) {
        let s = place.scale;
        let rect = line_rect(place, canvas, size);
        self.canvas
            .text_fmt_aligned(text, rect, size * s, color, weight, 0.6 * s, align);
    }

    /// Title over a `menu_blendbox` band.
    pub(super) fn band_title(
        &mut self,
        place: &Placement,
        canvas: [f32; 4],
        text: &str,
        size: f32,
    ) {
        self.piece(place, ArtPiece::BlendBox, canvas);
        self.label(
            place,
            canvas,
            text,
            size,
            LABEL,
            FontWeight::Semibold,
            TextAlign::Center,
        );
    }

    /// The dimmed match or menu map behind full-screen pages, darker in the
    /// pillarbox, as the classic main menu does.
    fn full_backdrop(&mut self, viewport: [f32; 2], place: &Placement) {
        let [width, height] = viewport;
        let page = place.rect([0.0, 0.0, CANVAS[0], CANVAS[1]]);
        let draw = self.canvas.draw_list_mut();
        let _ = draw.push(DrawCommand::SolidRect {
            rect: Rect::new(0.0, 0.0, width, height),
            color: ink(0.86),
        });
        for side in [
            Rect::new(0.0, 0.0, page.x, height),
            Rect::new(page.right(), 0.0, width - page.right(), height),
        ] {
            let _ = draw.push(DrawCommand::SolidRect {
                rect: side,
                color: ink(0.9),
            });
        }
    }

    /// `player.menu` / `ingame_player.menu` decoration.
    fn player_page(&mut self, place: &Placement, frame: Frame) {
        let page = ClassicPage::Player;
        let at = |canvas| place_in(page, frame, canvas);
        match frame {
            Frame::Full => {
                for (piece, canvas) in [
                    (ArtPiece::CenterBlue, [156.0, 154.0, 320.0, 240.0]),
                    (ArtPiece::SideLeft, [0.0, 0.0, 160.0, 480.0]),
                    (ArtPiece::SideRight, [480.0, 0.0, 160.0, 480.0]),
                    (ArtPiece::Background, [0.0, 0.0, 640.0, 480.0]),
                    (ArtPiece::Logo, LOGO),
                    (ArtPiece::BoxesLeft, [0.0, 50.0, 320.0, 160.0]),
                    (ArtPiece::BoxesRight, [320.0, 50.0, 320.0, 160.0]),
                ] {
                    self.piece(place, piece, canvas);
                }
                // Retail's bar (`15 186 610 20`) overhung its frame (`13 186 610
                // 245`) by two units; it sits flush on the frame's top edge here.
                self.border(place, [13.0, 186.0, 610.0, 245.0], FRAME, 2.0);
                self.fill(place, [13.0, 186.0, 610.0, 20.0], FRAME);
                self.label(
                    place,
                    [13.0, 186.0, 610.0, 20.0],
                    "Character Model",
                    15.0,
                    LABEL,
                    FontWeight::Semibold,
                    TextAlign::Center,
                );
                for (text, canvas) in [
                    ("Custom", [434.0, 212.0, 95.0, 20.0]),
                    ("Force", [529.0, 212.0, 95.0, 20.0]),
                ] {
                    self.label(
                        place,
                        canvas,
                        text,
                        15.0,
                        LABEL,
                        FontWeight::Semibold,
                        TextAlign::Center,
                    );
                }
                self.force_glance(place, [434.0, 318.0, 190.0, 44.0]);
                self.worn_line(place, [434.0, 396.0, 190.0, 14.0]);
            }
            Frame::InGame => {
                self.window_box(place, page, frame);
                self.fill(place, at([0.0, 420.0, 600.0, 20.0]), FRAME);
                self.piece(place, ArtPiece::ButtonBack, at([20.0, 3.0, 560.0, 30.0]));
                self.label(
                    place,
                    at([20.0, 5.0, 560.0, 28.0]),
                    "Player Configuration",
                    16.0,
                    LABEL,
                    FontWeight::Semibold,
                    TextAlign::Center,
                );
                // Retail's bar (`17 57 570 20`) stood above its frame (`15 65 570
                // 230`) and past its right edge, with the title centred on `3 52
                // 570`; the frame here starts at the bar, the bar fills its top
                // edge and the title is centred on the bar.
                self.border(place, at([15.0, 57.0, 570.0, 238.0]), FRAME, 2.0);
                self.fill(place, at([15.0, 57.0, 570.0, 20.0]), FRAME);
                self.label(
                    place,
                    at([15.0, 57.0, 570.0, 20.0]),
                    "Character Model",
                    15.0,
                    LABEL,
                    FontWeight::Semibold,
                    TextAlign::Center,
                );
                self.label(
                    place,
                    at([425.0, 130.0, 150.0, 26.0]),
                    "Custom",
                    15.0,
                    LABEL,
                    FontWeight::Semibold,
                    TextAlign::Center,
                );
                // Retail's bars (`17 300 410 18`, `432 300 155 18`) stood two units
                // right of their boxes; flush with them here, titles centred on them.
                for (box_rect, bar) in [
                    ([15.0, 299.0, 410.0, 100.0], [15.0, 299.0, 410.0, 18.0]),
                    ([430.0, 299.0, 155.0, 100.0], [430.0, 299.0, 155.0, 18.0]),
                ] {
                    self.border(place, at(box_rect), FRAME, 2.0);
                    self.fill(place, at(bar), FRAME);
                }
                self.label(
                    place,
                    at([15.0, 299.0, 410.0, 18.0]),
                    "The Force",
                    15.0,
                    LABEL,
                    FontWeight::Semibold,
                    TextAlign::Center,
                );
                self.label(
                    place,
                    at([430.0, 299.0, 155.0, 18.0]),
                    "Saber",
                    15.0,
                    LABEL,
                    FontWeight::Semibold,
                    TextAlign::Center,
                );
                self.force_summary(place, frame);
            }
        }
    }

    /// The in-game profile's Force box: retail's mastery, side and points
    /// lines (`UI_FORCE_RANK`, `UI_FORCE_SIDE`, `UI_FORCE_POINTS`) beside the
    /// side's emblem, over a strip of the known powers' holocrons.
    fn force_summary(&mut self, place: &Placement, frame: Frame) {
        let page = ClassicPage::Player;
        let allocation = self.force.allocation();
        let side = allocation.side;
        let (side_text, side_color) = match side {
            sjk_client::ForceSide::Light => ("Light", LIGHT_SIDE),
            sjk_client::ForceSide::Dark => ("Dark", DARK_SIDE),
        };
        let rank = super::force_page::mastery(allocation.rank);
        let points = self.force.remaining_points();
        let pending = if self.force.is_dirty() {
            " (not applied)"
        } else {
            ""
        };
        let emblem = crate::player_menu::force_icons::side_texture(side);
        let mut x = 20.0;
        if self.force_icons.is_texture_ready(emblem) {
            let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: place.rect(place_in(page, frame, [20.0, 322.0, 40.0, 40.0])),
                texture: emblem,
                color: FOCUS,
            });
            x = 68.0;
        }
        let gold = Color::new(1.0, 0.682, 0.0, 0.8);
        let line = |row: f32| place_in(page, frame, [x, 320.0 + row * 16.0, 200.0, 15.0]);
        self.label_fmt(
            place,
            line(0.0),
            format_args!("Force Mastery: {rank}"),
            12.0,
            gold,
            FontWeight::Regular,
            TextAlign::Start,
        );
        self.label_fmt(
            place,
            line(1.0),
            format_args!("Force side: {side_text}"),
            12.0,
            side_color,
            FontWeight::Regular,
            TextAlign::Start,
        );
        self.label_fmt(
            place,
            line(2.0),
            format_args!("Points Remaining: {points}{pending}"),
            12.0,
            gold,
            FontWeight::Regular,
            TextAlign::Start,
        );
        self.holocron_strip(place, place_in(page, frame, [20.0, 372.0, 250.0, 16.0]));
    }

    /// The holocrons of every power with a level, in the Force page's
    /// order, each with a pip per level under it. A Jedi knowing more powers
    /// than fit at full size gets smaller holocrons rather than a missing
    /// one.
    fn holocron_strip(&mut self, place: &Placement, canvas: [f32; 4]) {
        let [x, y, w, h] = canvas;
        let levels = self.force.allocation().levels;
        let side_powers = match self.force.allocation().side {
            sjk_client::ForceSide::Light => layout::LIGHT_POWERS,
            sjk_client::ForceSide::Dark => layout::DARK_POWERS,
        };
        let order = layout::NEUTRAL_POWERS
            .iter()
            .chain(&side_powers)
            .chain(&layout::SABER_POWERS)
            .map(|power| usize::from(*power))
            .filter(|power| levels[*power] > 0);
        let [step, size] = strip_fit(order.clone().count(), w, h);
        for (slot, power) in order.enumerate() {
            let left = x + slot as f32 * step;
            let icon = crate::player_menu::force_icons::power_texture(power);
            if self.force_icons.is_texture_ready(icon) {
                let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                    rect: place.rect([left, y, size, size]),
                    texture: icon,
                    color: FOCUS,
                });
            }
            for pip in 0..levels[power] {
                let pip_x = left + 2.0 + f32::from(pip) * (size - 4.0) / 3.0;
                self.fill(
                    place,
                    [pip_x, y + size + 2.0, (size - 4.0) / 3.0 - 1.0, 2.0],
                    Color::new(1.0, 0.682, 0.0, 0.9),
                );
            }
        }
    }

    /// The full-screen profile's Force line under its button: mastery and
    /// side, the points left, and the known powers' holocrons.
    fn force_glance(&mut self, place: &Placement, canvas: [f32; 4]) {
        let [x, y, w, _] = canvas;
        let allocation = self.force.allocation();
        let rank = super::force_page::mastery(allocation.rank);
        let (side, color) = match allocation.side {
            sjk_client::ForceSide::Light => ("Light", LIGHT_SIDE),
            sjk_client::ForceSide::Dark => ("Dark", DARK_SIDE),
        };
        let points = self.force.remaining_points();
        self.label_fmt(
            place,
            [x, y, w, 14.0],
            format_args!("{rank} \u{b7} {side}"),
            12.0,
            color,
            FontWeight::Semibold,
            TextAlign::Center,
        );
        self.label_fmt(
            place,
            [x, y + 14.0, w, 13.0],
            format_args!("{points} points remaining"),
            11.0,
            VALUE,
            FontWeight::Regular,
            TextAlign::Center,
        );
        self.holocron_strip(place, [x + 4.0, y + 29.0, w - 8.0, 12.0]);
    }

    /// What the player wears, under the Cosmetics button.
    fn worn_line(&mut self, place: &Placement, canvas: [f32; 4]) {
        use sjk_client::CosmeticSlot;
        let hat = self.cosmetics.worn_label(CosmeticSlot::Hat);
        let cape = self.cosmetics.worn_label(CosmeticSlot::Cape);
        match (hat, cape) {
            (None, None) => self.label(
                place,
                canvas,
                "No hat or cape",
                11.0,
                VALUE,
                FontWeight::Regular,
                TextAlign::Center,
            ),
            (hat, cape) => self.label_fmt(
                place,
                canvas,
                format_args!(
                    "{}{}{}",
                    hat.as_deref().unwrap_or_default(),
                    if hat.is_some() && cape.is_some() {
                        " \u{b7} "
                    } else {
                        ""
                    },
                    cape.as_deref().unwrap_or_default()
                ),
                11.0,
                VALUE,
                FontWeight::Regular,
                TextAlign::Center,
            ),
        }
    }

    /// The retail in-game window (`menu_box_ingame`).
    pub(super) fn window_box(&mut self, place: &Placement, page: ClassicPage, frame: Frame) {
        let canvas = window(page, frame);
        if self.classic.art.has(ArtPiece::PopupBox) {
            self.piece(place, ArtPiece::PopupBox, canvas);
        } else {
            self.fill(place, canvas, ink(0.85));
            self.border(place, canvas, FRAME, 2.0);
        }
    }

    /// `player2.menu` / `ingame_player2.menu` decoration and the model.
    fn character_page(&mut self, place: &Placement, frame: Frame) {
        let page = ClassicPage::Character;
        let at = |canvas| place_in(page, frame, canvas);
        let portrait;
        match frame {
            Frame::Full => {
                self.piece(place, ArtPiece::CenterBlue, [156.0, 154.0, 320.0, 240.0]);
                self.piece(place, ArtPiece::CharMenu, [0.0, 0.0, 640.0, 480.0]);
                self.band_title(
                    place,
                    [100.0, 54.0, 440.0, 16.0],
                    "CHARACTER CREATION",
                    12.0,
                );
                portrait = [393.0, 104.0, 220.0, 220.0];
            }
            Frame::InGame => {
                self.window_box(place, page, frame);
                self.band_title(
                    place,
                    at([35.0, 5.0, 360.0, 28.0]),
                    "Custom Character",
                    15.0,
                );
                portrait = at([300.0, 84.0, 110.0, 110.0]);
            }
        }
        for (text, canvas) in match frame {
            Frame::Full => [
                ("SPECIES", [30.0, 88.0, 140.0, 24.0]),
                ("COLOR", [30.0, 144.0, 160.0, 24.0]),
                ("APPEARANCE", [30.0, 252.0, 180.0, 24.0]),
            ],
            Frame::InGame => [
                ("SPECIES", at([15.0, 48.0, 140.0, 24.0])),
                ("COLOR", at([15.0, 80.0, 160.0, 24.0])),
                ("APPEARANCE", at([15.0, 160.0, 180.0, 24.0])),
            ],
        } {
            self.label(
                place,
                canvas,
                text,
                16.0,
                LABEL,
                FontWeight::Semibold,
                TextAlign::Start,
            );
        }
        self.model_portrait(place, portrait);
        if frame == Frame::Full {
            self.piece(
                place,
                ArtPiece::CharMenuBottom,
                [320.0, 360.0, 320.0, 120.0],
            );
        }
    }

    /// Where retail drew the live model: the live preview once the renderer
    /// has drawn one, else the model's portrait; the name under it.
    pub(super) fn model_portrait(&mut self, place: &Placement, canvas: [f32; 4]) {
        let absolute = self.choice_index();
        if self.preview_ready {
            let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: place.rect(canvas),
                texture: crate::ui_renderer::PREVIEW_TEXTURE,
                color: FOCUS,
            });
        } else if let Some(texture) = self.icons.icon(absolute) {
            // The portrait is square: centred in a taller or wider spot.
            let [x, y, w, h] = canvas;
            let side = w.min(h);
            let square = [x + (w - side) * 0.5, y + (h - side) * 0.5, side, side];
            let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: place.rect(square),
                texture,
                color: FOCUS,
            });
        }
        let [x, y, w, h] = canvas;
        let model = self.draft.model.clone();
        self.label(
            place,
            [x - 10.0, y + h + 6.0, w + 20.0, 18.0],
            &model,
            12.0,
            VALUE,
            FontWeight::Regular,
            TextAlign::Center,
        );
    }

    /// `saber.menu` / `ingame_saber.menu` decoration and the drawn saber.
    fn saber_page(&mut self, place: &Placement, frame: Frame) {
        let page = ClassicPage::Saber;
        let at = |canvas| place_in(page, frame, canvas);
        let dual = self.saber.style() == SaberStyle::Dual;
        match frame {
            Frame::Full => {
                for (piece, canvas) in [
                    (ArtPiece::SaberBack, [0.0, 0.0, 640.0, 480.0]),
                    (ArtPiece::SaberBox, [4.0, 66.0, 219.0, 165.0]),
                    (ArtPiece::SaberBoxTop, [418.0, 66.0, 219.0, 60.0]),
                    // `UI_SetSaberBoxesandHilts` always stretches box 3.
                    (ArtPiece::SaberBoxMiddle, [418.0, 126.0, 219.0, 44.0]),
                    (ArtPiece::SaberBoxBottom, [418.0, 170.0, 219.0, 60.0]),
                ] {
                    self.piece(place, piece, canvas);
                }
                self.band_title(
                    place,
                    [100.0, 54.0, 440.0, 16.0],
                    "LIGHTSABER CREATION",
                    12.0,
                );
                self.label(
                    place,
                    [32.0, 96.0, 160.0, 24.0],
                    "SABER TYPE",
                    16.0,
                    LABEL,
                    FontWeight::Semibold,
                    TextAlign::Start,
                );
                self.label(
                    place,
                    [240.0, 80.0, 160.0, 15.0],
                    "HILT",
                    12.0,
                    LABEL,
                    FontWeight::Semibold,
                    TextAlign::Center,
                );
                if dual {
                    self.label(
                        place,
                        [240.0, 150.0, 160.0, 15.0],
                        "HILT 2",
                        12.0,
                        LABEL,
                        FontWeight::Semibold,
                        TextAlign::Center,
                    );
                }
                self.label(
                    place,
                    [446.0, 96.0, 160.0, 24.0],
                    "BLADE COLOR",
                    16.0,
                    LABEL,
                    FontWeight::Semibold,
                    TextAlign::Start,
                );
                if dual {
                    self.label(
                        place,
                        [446.0, 152.0, 160.0, 16.0],
                        "COLOR 2",
                        13.0,
                        LABEL,
                        FontWeight::Semibold,
                        TextAlign::Start,
                    );
                }
                self.channel_headings(place, dual);
                self.saber_model(place, frame, [44.0, 298.0, 380.0, 60.0], dual);
            }
            Frame::InGame => {
                self.window_box(place, page, frame);
                self.band_title(
                    place,
                    at([20.0, 5.0, 390.0, 28.0]),
                    "LIGHTSABER CREATION",
                    15.0,
                );
                self.label(
                    place,
                    at([15.0, 38.0, 160.0, 24.0]),
                    "SABER TYPE",
                    16.0,
                    LABEL,
                    FontWeight::Semibold,
                    TextAlign::Start,
                );
                let hilt_titles: &[(&str, [f32; 4])] = if dual {
                    &[
                        ("HILT", [200.0, 34.0, 160.0, 16.0]),
                        ("HILT 2", [200.0, 105.0, 160.0, 15.0]),
                    ]
                } else {
                    &[("HILT", [200.0, 38.0, 160.0, 16.0])]
                };
                for (text, canvas) in hilt_titles {
                    self.label(
                        place,
                        at(*canvas),
                        text,
                        12.0,
                        LABEL,
                        FontWeight::Semibold,
                        TextAlign::Center,
                    );
                }
                self.label(
                    place,
                    at([15.0, 181.0, 160.0, 16.0]),
                    "BLADE COLOR",
                    13.0,
                    LABEL,
                    FontWeight::Semibold,
                    TextAlign::Start,
                );
                if dual {
                    self.label(
                        place,
                        at([270.0, 181.0, 160.0, 16.0]),
                        "COLOR 2",
                        13.0,
                        LABEL,
                        FontWeight::Semibold,
                        TextAlign::Start,
                    );
                }
                self.saber_model(place, frame, at([40.0, 291.0, 350.0, 60.0]), dual);
            }
        }
    }

    /// Where retail spun the saber model: the live preview of the lit sabers
    /// alone, turning, once the renderer has drawn one, else a drawn hilt and
    /// blade in `fallback` in the draft's colours (two for Dual, a blade out
    /// of each end for Staff).
    fn saber_model(&mut self, place: &Placement, frame: Frame, fallback: [f32; 4], dual: bool) {
        match layout::preview_rect(ClassicPage::Saber, frame).filter(|_| self.preview_ready) {
            Some(rect) => {
                let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                    rect: place.rect(rect),
                    texture: crate::ui_renderer::PREVIEW_TEXTURE,
                    color: FOCUS,
                });
            }
            None => self.drawn_saber(place, fallback, dual),
        }
    }

    fn drawn_saber(&mut self, place: &Placement, canvas: [f32; 4], dual: bool) {
        let [x, y, w, h] = canvas;
        let saber = |menu: &mut Self, cy: f32, second: bool| {
            let [r, g, b] = menu.saber.rgb(second).map(|c| f32::from(c) / 255.0);
            let hilt_w = w * 0.16;
            let hilt_x = x + w * 0.5 - hilt_w * 0.5;
            let staff = menu.saber.style() == SaberStyle::Staff;
            let (blade_x, blade_w) = if staff {
                (x, w)
            } else {
                (hilt_x + hilt_w, w * 0.5 - hilt_w * 0.5)
            };
            let glow = Color::new(r, g, b, 0.55);
            let clear = Color::new(r, g, b, 0.0);
            for (top, start, end) in [(cy - 9.0, clear, glow), (cy, glow, clear)] {
                let rect = place.rect([blade_x, top, blade_w, 9.0]);
                let _ = menu.canvas.draw_list_mut().push(DrawCommand::GradientRect {
                    rect,
                    radius: 0.0,
                    gradient: Gradient {
                        start,
                        end,
                        vertical: true,
                    },
                });
            }
            menu.fill(
                place,
                [blade_x, cy - 3.0, blade_w, 6.0],
                Color::new(r, g, b, 0.9),
            );
            menu.fill(
                place,
                [blade_x, cy - 1.25, blade_w, 2.5],
                Color::new(1.0, 1.0, 1.0, 0.95),
            );
            menu.fill(
                place,
                [hilt_x, cy - 4.5, hilt_w, 9.0],
                Color::new(0.32, 0.33, 0.36, 1.0),
            );
            for ridge in 0..4 {
                let rx = hilt_x + hilt_w * (0.18 + ridge as f32 * 0.18);
                menu.fill(
                    place,
                    [rx, cy - 4.5, hilt_w * 0.05, 9.0],
                    Color::new(0.72, 0.73, 0.78, 1.0),
                );
            }
        };
        if dual {
            saber(self, y + h * 0.25, false);
            saber(self, y + h * 0.75, true);
        } else {
            saber(self, y + h * 0.5, false);
        }
    }

    /// Entries, lists and their pointer targets. Returns the entry whose
    /// description shows: the hovered one, else the focused one.
    fn entries(&mut self, place: &Placement, page: ClassicPage, frame: Frame) -> Option<Item> {
        let dual = self.saber.style() == SaberStyle::Dual;
        let items = self.classic_items();
        let mut described = items.get(self.classic.focus).copied();
        for (index, item) in items.iter().enumerate() {
            let token = index as u16;
            let canvas = rect(*item, page, frame, dual);
            let target = place.rect(canvas);
            let hovered = self.canvas.token_hovered(token) || self.sub_hovered(*item);
            if hovered {
                described = Some(*item);
            }
            let active = index == self.classic.focus || hovered;
            // The entry's own region first: the canvas gives the pointer to
            // the region registered last, so a list's cells, drawn next, win.
            self.canvas.hit_region(token, target);
            self.entry(place, page, frame, *item, canvas, active);
        }
        described
    }

    /// Whether the pointer is over one of `item`'s cells.
    fn sub_hovered(&self, item: Item) -> bool {
        let range = match item {
            Item::Models => TILE_BASE..TILE_BASE + MAX_VISIBLE_TILES,
            Item::Parts => PART_BASE..PART_BASE + 60,
            Item::Tints => TINT_BASE..TINT_BASE + 40,
            Item::Hilts => HILT_BASE[0]..HILT_BASE[0] + 100,
            Item::Hilts2 => HILT_BASE[1]..HILT_BASE[1] + 100,
            Item::Blades => BLADE_BASE[0]..BLADE_BASE[0] + 6,
            Item::Blades2 => BLADE_BASE[1]..BLADE_BASE[1] + 6,
            Item::Channel(index) => {
                let token = super::saber_rgb::RGB_BASE + u16::from(index);
                token..token + 1
            }
            Item::Power(power) => {
                let first = super::force_page::star_token(usize::from(power), 1);
                first..first + 3
            }
            Item::Templates => {
                TEMPLATE_BASE..TEMPLATE_BASE + super::force_page::MAX_TEMPLATE_ROWS as u16
            }
            Item::Hats => HAT_BASE..HAT_BASE + super::cosmetics_page::MAX_ROWS as u16,
            Item::Capes => CAPE_BASE..CAPE_BASE + super::cosmetics_page::MAX_ROWS as u16,
            _ => return false,
        };
        range
            .into_iter()
            .any(|token| self.canvas.token_hovered(token))
    }

    fn entry(
        &mut self,
        place: &Placement,
        page: ClassicPage,
        frame: Frame,
        item: Item,
        canvas: [f32; 4],
        active: bool,
    ) {
        let s = place.scale;
        let gold_text = if active { FOCUS } else { GOLD };
        match item {
            Item::NavProfile => {
                // The page's own button: white, never lit (retail).
                self.label(
                    place,
                    canvas,
                    item.label(),
                    17.0,
                    FOCUS,
                    FontWeight::Semibold,
                    TextAlign::Center,
                );
            }
            Item::SideLight
            | Item::SideDark
            | Item::Power(_)
            | Item::ForceReset
            | Item::ForceDiscard
            | Item::ForceApply
            | Item::Templates
            | Item::TemplateName
            | Item::TemplateSave => self.force_entry(place, item, canvas, active),
            Item::Hats | Item::Capes | Item::CosmeticsShow => {
                self.cosmetics_entry(place, item, canvas, active)
            }
            Item::NavPlay
            | Item::NavControls
            | Item::NavSetup
            | Item::Exit
            | Item::Back
            | Item::Apply
            | Item::ApplyMain
            | Item::CosmeticsButton
            | Item::CosmeticsClear => {
                if active {
                    glow(&mut self.canvas, place.rect(canvas), s, self.classic.art);
                }
                let size = match (item, frame) {
                    (Item::CosmeticsButton | Item::CosmeticsClear, _) => 15.0,
                    (_, Frame::InGame) => 15.0,
                    _ => 17.0,
                };
                // The saber page's one button reads "Apply" (`@MENUS_APPLY`, Item::ApplyMain);
                // the profile pages' APPLY is `@MENUS_APPLY_CAPS`.
                let text = match (item, page, frame) {
                    // `ingame_saber`'s button is `@MENUS_APPLY_CHANGES`.
                    (Item::Apply, ClassicPage::Saber, Frame::InGame) => "APPLY CHANGES",
                    (Item::Apply, _, Frame::InGame) => "Apply",
                    _ => item.label(),
                };
                self.label(
                    place,
                    canvas,
                    text,
                    size,
                    gold_text,
                    FontWeight::Semibold,
                    TextAlign::Center,
                );
            }
            Item::Name => {
                if active {
                    glow(&mut self.canvas, place.rect(canvas), s, self.classic.art);
                }
                let color = if frame == Frame::InGame {
                    gold_text
                } else if active {
                    FOCUS
                } else {
                    VALUE
                };
                let rect = place.rect(canvas);
                let text_rect = Rect::new(
                    rect.x,
                    rect.y + (rect.height - 15.0 * 1.25 * s) * 0.5,
                    rect.width,
                    15.0 * 1.25 * s,
                );
                let name = self.draft.name.clone();
                self.canvas.text_fmt_aligned(
                    format_args!("Name: {name}"),
                    text_rect,
                    15.0 * s,
                    color,
                    FontWeight::Regular,
                    0.6 * s,
                    TextAlign::Start,
                );
                if self.name_editing {
                    let accent = self.canvas.theme().accent;
                    self.canvas.edit_underline(rect, accent, s);
                }
            }
            Item::Team => {
                let team = match self.team {
                    TeamSkin::Default => "Default",
                    TeamSkin::Red => "Red",
                    TeamSkin::Blue => "Blue",
                };
                let color = if frame == Frame::InGame {
                    gold_text
                } else if active {
                    FOCUS
                } else {
                    VALUE
                };
                let rect = place.rect(canvas);
                let text_rect = Rect::new(rect.x, rect.y, rect.width + 120.0 * s, rect.height);
                self.canvas.text_fmt_aligned(
                    format_args!("Team Color: {team}"),
                    text_rect,
                    13.0 * s,
                    color,
                    FontWeight::Regular,
                    0.5 * s,
                    TextAlign::Start,
                );
            }
            Item::Search => self.search_field(place, canvas, active),
            Item::Models => self.head_grid(place, frame, canvas, active),
            Item::Custom | Item::SaberButton | Item::ForceButton => {
                let piece = match item {
                    Item::Custom => ArtPiece::CustomPlayer,
                    Item::SaberButton => ArtPiece::SaberOnly,
                    _ => ArtPiece::ConfigForce,
                };
                let rect = place.rect(canvas);
                if self.classic.art.has(piece) {
                    let shade = if active { 1.0 } else { 0.5 };
                    let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                        rect,
                        texture: piece.texture(),
                        color: Color::new(shade, shade, shade, 1.0),
                    });
                } else {
                    self.border(place, canvas, if active { FOCUS } else { FRAME }, 2.0);
                    let text = match item {
                        Item::Custom => "Custom",
                        Item::SaberButton => "Saber",
                        _ => "Force",
                    };
                    self.label(
                        place,
                        canvas,
                        text,
                        14.0,
                        gold_text,
                        FontWeight::Semibold,
                        TextAlign::Center,
                    );
                }
            }
            Item::Species => {
                if active {
                    glow(&mut self.canvas, place.rect(canvas), s, self.classic.art);
                }
                let catalog = catalog_of(&self.loader);
                let species = self
                    .current_species()
                    .and_then(|index| catalog.and_then(|catalog| catalog.species.get(index)))
                    .map_or("-", |species| species.model.as_str());
                let species = species.rsplit('/').next().unwrap_or(species).to_owned();
                self.label(
                    place,
                    canvas,
                    &species,
                    13.0,
                    if active { FOCUS } else { VALUE },
                    FontWeight::Regular,
                    TextAlign::Start,
                );
            }
            Item::Tints => self.tint_list(place, canvas, active),
            Item::PartHead | Item::PartTorso | Item::PartLegs => {
                let chosen = item.part_axis() == Some(self.classic.part_axis);
                if active {
                    glow(&mut self.canvas, place.rect(canvas), s, self.classic.art);
                }
                let color = if active || chosen { FOCUS } else { GOLD };
                self.label(
                    place,
                    canvas,
                    item.label(),
                    13.0,
                    color,
                    FontWeight::Semibold,
                    TextAlign::Start,
                );
            }
            Item::Parts => self.part_list(place, canvas, active),
            Item::Single | Item::Dual | Item::Staff => {
                let style = match item {
                    Item::Single => SaberStyle::Single,
                    Item::Dual => SaberStyle::Dual,
                    _ => SaberStyle::Staff,
                };
                if active {
                    glow(&mut self.canvas, place.rect(canvas), s, self.classic.art);
                }
                let chosen = self.saber.style() == style;
                let color = if chosen || active { FOCUS } else { UNCHOSEN };
                self.label(
                    place,
                    canvas,
                    item.label(),
                    12.0,
                    color,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
            }
            Item::Hilts | Item::Hilts2 => {
                self.hilt_list(place, canvas, item == Item::Hilts2, active)
            }
            Item::Blades | Item::Blades2 => {
                self.blade_swatches(place, frame, canvas, item == Item::Blades2, active)
            }
            Item::Channel(index) => self.channel_row(place, canvas, index, active),
        }
        let _ = page;
    }

    /// SJK's search over the head grid: a list box holding the typed words
    /// (a prompt while empty), underlined while typed, and how many models
    /// match at its right end.
    fn search_field(&mut self, place: &Placement, canvas: [f32; 4], active: bool) {
        let s = place.scale;
        let editing = self.search_editing;
        let lit = active || editing;
        self.fill(place, canvas, LIST_BACK);
        self.border(place, canvas, if lit { FOCUS } else { LIST_BORDER }, 1.0);
        let [x, y, w, h] = canvas;
        let size = (h - 4.0).clamp(9.0, 12.0);
        let count_width = 28.0;
        let search = std::mem::take(&mut self.search);
        if search.is_empty() && !editing {
            let prompt = Color::new(VALUE.r, VALUE.g, VALUE.b, 0.6);
            self.label(
                place,
                [x + 4.0, y, w - 8.0, h],
                "Search models",
                size,
                prompt,
                FontWeight::Regular,
                TextAlign::Start,
            );
        } else {
            let caret = if editing { "_" } else { "" };
            self.label_fmt(
                place,
                [x + 4.0, y, w - count_width - 8.0, h],
                format_args!("{search}{caret}"),
                size,
                if lit { FOCUS } else { VALUE },
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        if !search.is_empty() {
            let found = self.tiles.len();
            self.label_fmt(
                place,
                [x + w - count_width - 4.0, y, count_width, h],
                format_args!("{found}"),
                size,
                LABEL,
                FontWeight::Regular,
                TextAlign::End,
            );
        }
        self.search = search;
        if editing {
            let accent = self.canvas.theme().accent;
            self.canvas.edit_underline(place.rect(canvas), accent, s);
        }
    }

    /// The head grid: model portraits in 64-unit cells, scrolled by rows.
    fn head_grid(&mut self, place: &Placement, frame: Frame, canvas: [f32; 4], active: bool) {
        let s = place.scale;
        self.fill(place, canvas, Color::new(0.0, 0.0, 0.0, 1.0));
        let border = if frame == Frame::InGame {
            GOLD
        } else {
            Color::new(0.5, 0.5, 0.5, 1.0)
        };
        self.border(place, canvas, if active { FOCUS } else { border }, 1.0);
        let [x, y, w, h] = canvas;
        let rows = (h / GRID_CELL).floor().max(1.0) as usize;
        let tiles = self.tiles.len();
        let total_rows = tiles.div_ceil(GRID_COLUMNS);
        self.grid_max_scroll = total_rows.saturating_sub(rows);
        let current = self.tile_position();
        if self.grid_follow {
            self.grid_follow = false;
            let row = current.unwrap_or(0) / GRID_COLUMNS;
            if row < self.grid_scroll {
                self.grid_scroll = row;
            } else if row >= self.grid_scroll + rows {
                self.grid_scroll = row + 1 - rows;
            }
        }
        self.grid_scroll = self.grid_scroll.min(self.grid_max_scroll);
        self.canvas
            .scroll_region(GRID_SCROLL_TOKEN, place.rect(canvas));
        self.hovered_entry = None;
        let first = self.grid_scroll * GRID_COLUMNS;
        let last = (first + rows * GRID_COLUMNS)
            .min(tiles)
            .min(first + usize::from(MAX_VISIBLE_TILES));
        for slot in first..last {
            let local = slot - first;
            let cell = [
                x + 2.0 + (local % GRID_COLUMNS) as f32 * GRID_CELL,
                y + 1.0 + (local / GRID_COLUMNS) as f32 * GRID_CELL,
                GRID_CELL - 2.0,
                GRID_CELL - 2.0,
            ];
            let rect = place.rect(cell);
            let token = TILE_BASE + local as u16;
            let hovered = self.canvas.token_hovered(token);
            let absolute = self.tiles[slot];
            if hovered {
                self.hovered_entry = Some(absolute);
            }
            match self.icons.icon(absolute) {
                Some(texture) => {
                    let shade = if current == Some(slot) || hovered {
                        1.0
                    } else {
                        0.8
                    };
                    let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                        rect,
                        texture,
                        color: Color::new(shade, shade, shade, 1.0),
                    });
                }
                // An icon that cannot be decoded: the model's name instead.
                None if self.icons.failed(absolute) => {
                    let color = if current == Some(slot) || hovered {
                        FOCUS
                    } else {
                        VALUE
                    };
                    self.tile_name(absolute, rect, 9.0 * s, color, 0.3 * s);
                }
                None => {}
            }
            if current == Some(slot) {
                self.border(place, cell, GOLD, 2.0);
            } else if hovered {
                self.border(place, cell, Color::new(1.0, 1.0, 1.0, 0.5), 1.0);
            }
            self.canvas.hit_region(token, rect);
        }
        if total_rows > rows {
            let track = place.rect([x + w - 6.0, y + 2.0, 3.0, h - 4.0]);
            self.canvas
                .scrollbar(GRID_SCROLL_TOKEN, track, self.grid_scroll, rows, total_rows);
        }
        let _ = s;
    }

    /// Character creation's skin tint swatches.
    fn tint_list(&mut self, place: &Placement, canvas: [f32; 4], active: bool) {
        self.fill(place, canvas, LIST_BACK);
        self.border(place, canvas, if active { FOCUS } else { LIST_BORDER }, 1.0);
        self.canvas.scroll_region(TINTS_SCROLL, place.rect(canvas));
        let Some(colors) = self.species_field(|species| {
            species
                .colors
                .iter()
                .map(|color| color.rgb)
                .collect::<Vec<_>>()
        }) else {
            return;
        };
        let [x, y, w, h] = canvas;
        let visible = (w / TINT_CELL).floor() as usize;
        self.classic.tint_scroll = self
            .classic
            .tint_scroll
            .min(colors.len().saturating_sub(visible));
        let first = self.classic.tint_scroll;
        for (local, (index, rgb)) in colors
            .iter()
            .enumerate()
            .skip(first)
            .take(visible)
            .enumerate()
        {
            let cell = [
                x + 2.0 + local as f32 * TINT_CELL,
                y + (h - TINT_CELL) * 0.5 + 2.0,
                TINT_CELL - 4.0,
                TINT_CELL - 4.0,
            ];
            let [r, g, b] = rgb.map(|c| f32::from(c) / 255.0);
            // Retail's swatch: the species' tint base times the colour.
            match self
                .current_species()
                .and_then(|species| self.part_icons.tint_base(species))
            {
                Some(texture) => {
                    let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                        rect: place.rect(cell),
                        texture,
                        color: Color::new(r, g, b, 1.0),
                    });
                }
                None => self.fill(place, cell, Color::new(r, g, b, 1.0)),
            }
            let token = TINT_BASE + index as u16;
            if self.variants[3] == index {
                self.border(place, cell, FOCUS, 2.0);
            } else if self.canvas.token_hovered(token) {
                self.border(place, cell, Color::new(1.0, 1.0, 1.0, 0.5), 1.0);
            }
            self.canvas.hit_region(token, place.rect(cell));
        }
    }

    /// The selected part's variants in 72-unit cells: each variant's icon,
    /// as retail drew it, or its name while there is none.
    fn part_list(&mut self, place: &Placement, canvas: [f32; 4], active: bool) {
        let s = place.scale;
        self.fill(place, canvas, LIST_BACK);
        self.border(place, canvas, if active { FOCUS } else { LIST_BORDER }, 1.0);
        self.canvas.scroll_region(PARTS_SCROLL, place.rect(canvas));
        let axis = self.classic.part_axis;
        let Some(parts) = self.species_field(|species| match axis {
            0 => species.heads.clone(),
            1 => species.torsos.clone(),
            _ => species.legs.clone(),
        }) else {
            return;
        };
        let [x, y, w, h] = canvas;
        let visible = (w / PART_CELL).floor() as usize;
        let chosen = self.variants[axis];
        if self.classic.part_scroll + visible <= chosen || chosen < self.classic.part_scroll {
            self.classic.part_scroll = chosen.saturating_sub(visible.saturating_sub(1));
        }
        self.classic.part_scroll = self
            .classic
            .part_scroll
            .min(parts.len().saturating_sub(visible));
        let first = self.classic.part_scroll;
        for (local, (index, name)) in parts
            .iter()
            .enumerate()
            .skip(first)
            .take(visible)
            .enumerate()
        {
            let cell = [
                x + 2.0 + local as f32 * PART_CELL,
                y + (h - PART_CELL) * 0.5 + 1.0,
                PART_CELL - 4.0,
                PART_CELL - 4.0,
            ];
            self.fill(place, cell, Color::new(0.02, 0.02, 0.12, 0.85));
            let token = PART_BASE + index as u16;
            let hovered = self.canvas.token_hovered(token);
            let color = if index == chosen || hovered {
                FOCUS
            } else {
                VALUE
            };
            let rect = place.rect(cell);
            let part = [Part::Head, Part::Torso, Part::Legs][axis.min(2)];
            if let Some(texture) = self
                .current_species()
                .and_then(|species| self.part_icons.part(species, part, index))
            {
                let shade = if index == chosen || hovered {
                    1.0
                } else {
                    0.75
                };
                let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                    rect,
                    texture,
                    color: Color::new(shade, shade, shade, 1.0),
                });
                if index == chosen {
                    self.border(place, cell, FOCUS, 2.0);
                } else if hovered {
                    self.border(place, cell, Color::new(1.0, 1.0, 1.0, 0.5), 1.0);
                }
                self.canvas.hit_region(token, rect);
                continue;
            }
            self.canvas.text_aligned(
                name,
                Rect::new(
                    rect.x + 2.0 * s,
                    rect.y + rect.height * 0.5 - 8.0 * s,
                    rect.width - 4.0 * s,
                    16.0 * s,
                ),
                10.0 * s,
                color,
                FontWeight::Regular,
                0.2 * s,
                TextAlign::Center,
            );
            if index == chosen {
                self.border(place, cell, FOCUS, 2.0);
            }
            self.canvas.hit_region(token, rect);
        }
    }

    /// A value of the species being edited, if a species is.
    fn species_field<T>(&self, read: impl Fn(&sjk_client::LegacySpecies) -> T) -> Option<T> {
        let index = self.current_species()?;
        catalog_of(&self.loader)?.species.get(index).map(read)
    }

    /// One hilt list: the hilts the saber type allows, 16-unit rows.
    fn hilt_list(&mut self, place: &Placement, canvas: [f32; 4], second: bool, active: bool) {
        let s = place.scale;
        self.fill(place, canvas, HILT_BACK);
        self.border(place, canvas, if active { FOCUS } else { HILT_BORDER }, 1.0);
        let slot = usize::from(second);
        self.canvas
            .scroll_region(HILTS_SCROLL[slot], place.rect(canvas));
        let style = if second {
            SaberStyle::Single
        } else {
            self.saber.style()
        };
        let current = self.saber.hilt(second).to_ascii_lowercase();
        let Some(names): Option<Vec<(String, bool)>> = catalog_of(&self.loader).map(|catalog| {
            catalog
                .saber_hilts
                .iter()
                .filter(|hilt| allowed(hilt, style))
                .map(|hilt| {
                    (
                        hilt.display_name.clone(),
                        hilt.name.eq_ignore_ascii_case(&current),
                    )
                })
                .collect()
        }) else {
            return;
        };
        let [x, y, w, h] = canvas;
        let rows = (h / HILT_ROW).floor() as usize;
        let chosen = names.iter().position(|(_, chosen)| *chosen).unwrap_or(0);
        let scroll = &mut self.classic.hilt_scroll[slot];
        if chosen < *scroll {
            *scroll = chosen;
        } else if chosen >= *scroll + rows {
            *scroll = chosen + 1 - rows;
        }
        *scroll = (*scroll).min(names.len().saturating_sub(rows));
        let first = *scroll;
        for (local, (index, (name, is_chosen))) in
            names.iter().enumerate().skip(first).take(rows).enumerate()
        {
            let row = [x + 1.0, y + local as f32 * HILT_ROW, w - 2.0, HILT_ROW];
            let token = HILT_BASE[slot] + index as u16;
            let hovered = self.canvas.token_hovered(token);
            if *is_chosen {
                self.fill(place, row, Color::new(0.0, 0.0, 0.8, 0.45));
            }
            let color = if *is_chosen || hovered { FOCUS } else { VALUE };
            let rect = place.rect(row);
            self.canvas.text_aligned(
                name,
                Rect::new(rect.x + 4.0 * s, rect.y, rect.width - 8.0 * s, rect.height),
                12.0 * s,
                color,
                FontWeight::Regular,
                0.3 * s,
                TextAlign::Start,
            );
            self.canvas.hit_region(token, rect);
        }
    }

    /// Six blade colour swatches; the chosen one framed in white.
    fn blade_swatches(
        &mut self,
        place: &Placement,
        frame: Frame,
        canvas: [f32; 4],
        second: bool,
        active: bool,
    ) {
        let [x, y, _, _] = canvas;
        let step = swatch_step(frame);
        let chosen = self.saber.color(second);
        for (k, (index, piece)) in BLADE_SWATCHES.iter().zip(SWATCH_ART).enumerate() {
            let cell = [x + k as f32 * step, y, SWATCH, SWATCH];
            let token = BLADE_BASE[usize::from(second)] + k as u16;
            let hovered = self.canvas.token_hovered(token);
            let lit = *index == chosen || hovered;
            if self.classic.art.has(piece) {
                let shade = if lit { 1.0 } else { 0.75 };
                let rect = place.rect(cell);
                let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                    rect,
                    texture: piece.texture(),
                    color: Color::new(shade, shade, shade, 1.0),
                });
            } else {
                let [r, g, b] = crate::saber::Color::ALL[usize::from(*index)]
                    .blade_rgb()
                    .map(|c| f32::from(c) / 255.0);
                self.fill(place, cell, Color::new(r, g, b, 1.0));
            }
            let border = if *index == chosen {
                FOCUS
            } else if lit || active {
                Color::new(0.66, 0.66, 1.0, 1.0)
            } else {
                Color::new(0.33, 0.33, 0.5, 1.0)
            };
            self.border(
                place,
                cell,
                border,
                if *index == chosen { 2.0 } else { 1.0 },
            );
            self.canvas.hit_region(token, place.rect(cell));
        }
    }

    /// The description line: the hovered or focused entry's retail
    /// `descText`, or a note while the catalogue loads.
    fn description(
        &mut self,
        place: &Placement,
        page: ClassicPage,
        frame: Frame,
        item: Option<Item>,
    ) {
        let s = place.scale;
        let center = match frame {
            // The cosmetics window keeps JoF's line inside its bottom edge.
            _ if page == ClassicPage::Cosmetics => {
                let [x, y, w, h] = window(page, frame);
                [x + w * 0.5, y + h - 22.0]
            }
            // The profile and creation pages frame the screen down to y 431,
            // so their line sits between the bottom buttons; the saber page
            // has a button there and room above.
            Frame::Full if page == ClassicPage::Saber => [CANVAS[0] * 0.5, HINT_Y],
            Frame::Full => [CANVAS[0] * 0.5, 456.0],
            Frame::InGame => {
                let [x, y, w, h] = window(page, frame);
                let bottom = if page == ClassicPage::Player {
                    y + h - 10.0
                } else {
                    y + h - 18.0
                };
                [x + w * 0.5, bottom]
            }
        };
        let status = self.status();
        // SJK: a head under the pointer names its model.
        let hovered = self
            .hovered_entry
            .filter(|_| item == Some(Item::Models))
            .and_then(|entry| {
                catalog_of(&self.loader)
                    .and_then(|catalog| crate::player_menu::grid::entry_name(catalog, entry))
            });
        let text = match status {
            sjk_client::LegacyCatalogStatus::Idle | sjk_client::LegacyCatalogStatus::Loading => {
                "Reading the character catalogue..."
            }
            sjk_client::LegacyCatalogStatus::Failed => "The character catalogue is unavailable.",
            sjk_client::LegacyCatalogStatus::Ready => {
                hovered.unwrap_or_else(|| item.map_or("", Item::hint))
            }
        };
        self.canvas.text_aligned(
            text,
            place.centered(center, 560.0, 18.0),
            12.0 * s,
            HINT,
            FontWeight::Regular,
            0.3 * s,
            TextAlign::Center,
        );
    }
}

/// A one-line box of text size `size` centred on the item's middle.
fn line_rect(place: &Placement, canvas: [f32; 4], size: f32) -> Rect {
    let line = size * 1.25;
    let [x, y, w, h] = canvas;
    place.rect([x, y + (h - line) * 0.5, w, line])
}

/// A window-relative retail rectangle on the canvas (full-screen pages are
/// already on it).
fn place_in(page: ClassicPage, frame: Frame, canvas: [f32; 4]) -> [f32; 4] {
    place(page, frame, canvas)
}

/// The step and side of `count` holocrons in a strip `w` wide and `h` high:
/// full size with a 3-unit gap, or the step that fills the width with the
/// gap shrunk in proportion.
fn strip_fit(count: usize, w: f32, h: f32) -> [f32; 2] {
    // The gap is the step's last 3/(h+3): `count` steps less one gap fill w.
    let gap_share = 3.0 / (h + 3.0);
    let step = (h + 3.0).min(w / (count.max(1) as f32 - gap_share));
    [step, step * (1.0 - gap_share)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_known_holocron_fits_its_strip() {
        // Thirteen powers (neutral, one side's, saber) in the full-screen
        // profile's strip shrink; a short list keeps full size.
        let [step, size] = strip_fit(13, 182.0, 12.0);
        assert!(12.0 * step + size <= 182.0 + 0.01);
        assert!(size > 10.0);
        assert_eq!(strip_fit(4, 182.0, 12.0), [15.0, 12.0]);
    }
}
