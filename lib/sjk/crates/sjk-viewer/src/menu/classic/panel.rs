//! The classic Setup and Controls screens. Retail's `setup.menu` and
//! `controls.menu` (and the in-game pop-ups `ingame_setup.menu` and
//! `ingame_controls.menu`) keep the list of option groups on the left and
//! show the chosen group's items in a panel beside it. This module draws
//! everything around those items (backdrop, title, navigation, the group
//! list and the panel box) and gives the item rows their retail geometry.
//! The settings screen and the key-binding editor draw the items, so the
//! options behave the same in both menu styles.
//!
//! SJK makes the panels classic+ (`docs/classic-plus.md`): the item rows take
//! the panel's upper part and a detail box under them describes the focused
//! item (what it does, its value, default and range, when a change applies,
//! its console name), where retail's taller panel had room for more rows. A
//! row changed from its default carries a gold dot after its label, and a
//! setting that applies later a `*`. The rows' controls are classic+'s own
//! rather than retail art: switches, slim sliders, segments and choice fields
//! on retail's blue and gold, the focused row on a soft band.
//!
//! Retail's Controls and Setup are one Settings screen in SJK: the title band
//! carries its two tabs, KEY BINDINGS and OPTIONS ([`Page::settings_tab`]),
//! and the panel's first row is a search field over the whole tab.

use super::layout::{CANVAS, Entry, HINT_Y, Page, Placement, Size, Slot};
use super::view::{self, Caps, DISABLED, FOCUS, GOLD, HINT};
use crate::menu::art::{ArtPiece, ArtSet};
use crate::menu_widgets::MenuCanvas;
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign, TextureId};

/// Pointer tokens of the screen's own buttons (navigation row, group list,
/// Back and Exit): `CHROME_BASE` plus the button's index in its page's
/// [`Page::slots`]. Clear of the items' row tokens (row indices), the
/// settings tabs (500), the key-binding editor's secondary slots (600) and
/// the slider value targets (700).
pub(crate) const CHROME_BASE: u16 = 800;

/// Chrome slot indices of the Settings tabs in the title band, past every
/// page's own slots: KEY BINDINGS, then OPTIONS.
pub(crate) const TAB_SLOTS: [usize; 2] = [30, 31];

/// The panel's search field, clear of the other panel tokens (rows 0-499,
/// tabs 500, secondary slots 600, values 700, chrome 800, footer 900-912).
pub(crate) const SEARCH_TOKEN: u16 = 913;

/// The Settings tab a chrome slot names, if it names one.
pub(crate) fn settings_tab_slot(slot: usize) -> Option<usize> {
    TAB_SLOTS.iter().position(|tab| *tab == slot)
}

/// Page slot index of a chrome token.
pub(crate) fn chrome_slot(token: u16) -> Option<usize> {
    (CHROME_BASE..CHROME_BASE + 32)
        .contains(&token)
        .then(|| usize::from(token - CHROME_BASE))
}

/// Retail option item colour (`forecolor 0.65 0.65 1`).
pub(crate) const OPTION: Color = Color::new(0.65, 0.65, 1.0, 1.0);
/// The focused item's colour as retail paints it (`focusColor 1 1 1 1`),
/// pulsing ([`view::focus_pulse`]).
pub(crate) fn focus_text() -> Color {
    view::focus_pulse()
}
/// `color` at `alpha`.
pub(crate) const fn dim(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, alpha)
}
/// Retail colour of a key being rebound (`Item_Bind_Paint`'s red pulse).
pub(crate) const BINDING: Color = Color::new(1.0, 0.25, 0.25, 1.0);
/// Retail panel box (`setup_background`: `backcolor 0 0 .6 .5`, border
/// `0 0 .6 1`).
const PANEL_FILL: Color = Color::new(0.0, 0.0, 0.6, 0.5);
const PANEL_BORDER: Color = Color::new(0.0, 0.0, 0.6, 1.0);
/// Retail panel title colour (`forecolor .549 .854 1`).
const PANEL_TITLE: Color = Color::new(0.549, 0.854, 1.0, 1.0);
/// Classic+ detail box: retail's frame blue (`bordercolor .298 .305 .690`)
/// and field value colour (`forecolor .615 .615 .956`).
const DETAIL_BORDER: Color = Color::new(0.298, 0.305, 0.690, 1.0);
const DETAIL_TEXT: Color = Color::new(0.615, 0.615, 0.956, 1.0);
/// Width of the label column's end kept for a row's `*` mark.
const MARK_WIDTH: f32 = 7.0;
/// Classic+ controls: a value (brighter than the labels), an empty track and a
/// control's edge, in retail's option blue.
pub(crate) const VALUE: Color = Color::new(0.93, 0.93, 1.0, 1.0);
const TRACK: Color = Color::new(0.65, 0.65, 1.0, 0.22);
const EDGE: Color = Color::new(0.65, 0.65, 1.0, 0.4);
/// The focused control's outline.
const FOCUS_EDGE: Color = Color::new(1.0, 1.0, 1.0, 0.65);
/// The focused row's band.
const BAND: Color = Color::new(0.55, 0.62, 1.0, 0.16);
/// Width kept at a row's right end for its reset button.
const RESET_WIDTH: f32 = 14.0;
/// Retail slider art size (`SLIDER_WIDTH`, `SLIDER_HEIGHT`,
/// `SLIDER_THUMB_WIDTH`, `SLIDER_THUMB_HEIGHT` in `ui_shared.h`).
const SLIDER: [f32; 2] = [96.0, 16.0];
/// Retail gap between an item's label and its value (`textRect.w + 8`).
const VALUE_GAP: f32 = 8.0;
/// Space between a label and its row picture.
const ICON_GAP: f32 = 6.0;

/// Where a panel screen is shown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Frame {
    /// The main menu's full page (`setup.menu`, `controls.menu`).
    Main,
    /// The in-game pop-up under the top bar (`ingame_setup.menu`,
    /// `ingame_controls.menu`).
    InGame,
}

/// Canvas geometry of one frame, from the retail item rectangles.
struct Geometry {
    /// The filled panel box.
    panel: [f32; 4],
    /// Left edge and width of an item row.
    row_x: f32,
    row_width: f32,
    /// Top of the first row and the row pitch.
    first_row: f32,
    row_height: f32,
    /// Rows the panel holds.
    rows: usize,
    /// Right edge of item labels (`rect.x + textalignx`).
    label_end: f32,
    /// Item text height.
    text: f32,
    /// The title band and its vertical centre.
    title: [f32; 4],
    /// Centre of the description line.
    hint: [f32; 2],
    /// The classic+ detail box under the rows.
    detail: [f32; 4],
    /// The search field: the row above the first item row.
    search: [f32; 4],
}

/// `setup.menu`: panel `260 185 340 225`, items `260 188+14n 340 14`
/// labelled up to `textalignx 174`, title band `100 164 440 16`. Classic+
/// keeps the search field and ten rows in the panel's upper part and the
/// detail box in the rest of retail's panel.
const MAIN: Geometry = Geometry {
    panel: [260.0, 185.0, 340.0, 160.0],
    row_x: 260.0,
    row_width: 340.0,
    first_row: 202.0,
    row_height: 14.0,
    rows: 10,
    label_end: 434.0,
    text: 11.0,
    title: [100.0, 164.0, 440.0, 16.0],
    hint: [CANVAS[0] * 0.5, HINT_Y],
    detail: [260.0, 349.0, 340.0, 63.0],
    search: [260.0, 188.0, 340.0, 14.0],
};

/// `ingame_setup.menu` (menu rect `45 35 550 335`): box `0 0 570 335`,
/// group list `20 43+30n 170 30`, panel `210 41 350 250`, items
/// `220 41+20n 300 20` labelled up to `textalignx 165`, title band
/// `20 5 510 28`, description at `305 347`. Classic+ keeps nine rows and
/// the detail box under them, inside retail's panel; the search field takes
/// the first row.
const IN_GAME: Geometry = Geometry {
    panel: [45.0 + 210.0, 35.0 + 41.0, 350.0, 185.0],
    row_x: 45.0 + 220.0,
    row_width: 300.0,
    first_row: 35.0 + 63.0,
    row_height: 20.0,
    rows: 8,
    label_end: 45.0 + 385.0,
    text: 12.0,
    title: [45.0 + 20.0, 35.0 + 5.0, 510.0, 28.0],
    hint: [45.0 + 305.0, 35.0 + 347.0],
    detail: [45.0 + 210.0, 35.0 + 230.0, 350.0, 61.0],
    search: [45.0 + 220.0, 35.0 + 43.0, 300.0, 20.0],
};

/// What the classic+ detail box says about the focused item.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Detail<'a> {
    /// The item's name, drawn in capitals, and its value.
    pub(crate) title: &'a str,
    pub(crate) value: &'a str,
    /// What it does, in up to two lines.
    pub(crate) lines: [&'a str; 2],
    /// Default, range and when a change applies.
    pub(crate) facts: &'a str,
    /// The console name, at the facts line's end.
    pub(crate) name: &'a str,
    /// A picture of the item (an atlas cell), beside its lines.
    pub(crate) icon: Option<TextureId>,
    /// A small picture before the title: the icon of the item's group.
    pub(crate) badge: Option<TextureId>,
}

/// The in-game pop-up box (`background_pic` `0 0 570 335` of a menu at
/// `45 35`).
const IN_GAME_BOX: [f32; 4] = [45.0, 35.0, 570.0, 335.0];
/// The in-game group list: `20 43+30n 170 30`, labels set against the
/// right edge, glow `185` wide.
const IN_GAME_LIST: [f32; 4] = [45.0 + 20.0, 35.0 + 43.0, 170.0, 30.0];

impl Frame {
    fn geometry(self) -> &'static Geometry {
        match self {
            Self::Main => &MAIN,
            Self::InGame => &IN_GAME,
        }
    }

    /// Item rows the panel holds.
    pub(crate) fn capacity(self) -> usize {
        self.geometry().rows
    }

    /// Where a slider bar sits across its item row: the offset of its left
    /// edge and its width, as fractions of the row width.
    pub(crate) fn slider_span(self) -> (f32, f32) {
        let geometry = self.geometry();
        (
            (geometry.label_end + VALUE_GAP - geometry.row_x) / geometry.row_width,
            SLIDER[0] / geometry.row_width,
        )
    }
}

/// Position along a slider of the pointer at `x` over item row `row`
/// (window coordinates), for a frame with `slider_span` `(offset, width)`.
pub(crate) fn slider_ratio(row: Rect, x: f32, (offset, width): (f32, f32)) -> f32 {
    if row.width <= 0.0 || width <= 0.0 {
        return 0.0;
    }
    (((x - row.x) / row.width - offset) / width).clamp(0.0, 1.0)
}

/// One panel screen: which page's list it shows, which group is open.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PanelFrame {
    pub(crate) frame: Frame,
    pub(crate) page: Page,
    pub(crate) active: Entry,
    pub(crate) art: ArtSet,
}

/// A panel screen being drawn: its placement on the window and its
/// geometry, for the item rows.
pub(crate) struct PanelPlace {
    place: Placement,
    frame: Frame,
    /// Description of the chrome button under the pointer, if any.
    hovered_hint: Option<(&'static str, bool)>,
    /// Whether the rows keep a picture column between labels and values.
    icons: bool,
}

impl PanelFrame {
    /// Begin the screen in `canvas`: backdrop, title, the page's buttons and
    /// the empty panel box. Item rows go on top through the returned
    /// [`PanelPlace`]; [`PanelPlace::finish`] adds the description line.
    pub(crate) fn begin(
        &self,
        canvas: &mut MenuCanvas,
        viewport: [f32; 2],
        reveal: f32,
    ) -> PanelPlace {
        let place = Placement::new(viewport);
        canvas.begin_transparent(viewport);
        canvas.push_opacity(reveal);
        match self.frame {
            Frame::Main => view::page_backdrop(canvas, viewport, &place, self.page, self.art),
            Frame::InGame => self.in_game_box(canvas, viewport, &place),
        }
        let geometry = self.frame.geometry();
        let mut hovered_hint = None;
        match self.page.settings_tab() {
            Some(active) => {
                hovered_hint = self.tabs(canvas, &place, geometry, active);
            }
            None => self.title(canvas, &place, geometry, self.page.title().0),
        }
        for (index, slot) in self.page.slots().iter().enumerate() {
            let Some(target) = self.slot_target(index, slot) else {
                continue;
            };
            let token = CHROME_BASE + index as u16;
            let target = place.rect(target);
            let hovered = canvas.token_hovered(token);
            if hovered {
                hovered_hint = Some((slot.hint, slot.enabled()));
            }
            if hovered && slot.enabled() {
                self.glow(canvas, &place, slot, target);
            }
            // The open group's entry stays white, as retail recolours it;
            // the hovered one has the focus and pulses.
            let color = match (slot.enabled(), hovered, slot.entry == self.active) {
                (false, _, _) => DISABLED,
                (true, true, _) => view::focus_pulse(),
                (true, false, true) => FOCUS,
                (true, false, false) => GOLD,
            };
            match self.frame {
                Frame::Main => view::entry_label_colored(canvas, &place, slot, color),
                Frame::InGame => self.in_game_label(canvas, &place, index, slot, color),
            }
            let lit = hovered || slot.entry == self.active;
            self.group_icon(canvas, &place, index, slot, lit);
            canvas.hit_region(token, target);
        }
        panel_box(canvas, place.rect(geometry.panel), place.scale);
        PanelPlace {
            place,
            frame: self.frame,
            hovered_hint,
            icons: false,
        }
    }

    /// Canvas target of page slot `index`, if this frame shows it: every
    /// button on the main page, only the group list in the pop-up.
    fn slot_target(&self, index: usize, slot: &Slot) -> Option<[f32; 4]> {
        match self.frame {
            Frame::Main => Some(slot.target()),
            Frame::InGame => {
                let row = self.list_row(index)?;
                let [x, y, width, height] = self.in_game_list();
                Some([x, y + row as f32 * height, width, height])
            }
        }
    }

    /// The pop-up's group list rectangle of its first row: retail's 30-unit
    /// rows, tightened when the page has more groups than fit in the box
    /// (Setup gains SJK's RENDERER after retail's groups).
    fn in_game_list(&self) -> [f32; 4] {
        let [x, y, width, height] = IN_GAME_LIST;
        let [_, box_y, _, box_height] = IN_GAME_BOX;
        let groups = self
            .page
            .slots()
            .iter()
            .filter(|slot| slot.size == Size::List)
            .count()
            .max(1);
        let fit = (box_y + box_height - y) / groups as f32;
        [x, y, width, height.min(fit)]
    }

    /// Row of page slot `index` in the pop-up's group list.
    fn list_row(&self, index: usize) -> Option<usize> {
        let slots = self.page.slots();
        (slots.get(index)?.size == Size::List).then(|| {
            slots[..index]
                .iter()
                .filter(|slot| slot.size == Size::List)
                .count()
        })
    }

    /// Focus glow behind a hovered button: retail's `menu_blendbox2` behind
    /// group entries (10 units wider than the entry), `menu_buttonback`
    /// behind the others.
    fn glow(&self, canvas: &mut MenuCanvas, place: &Placement, slot: &Slot, target: Rect) {
        if slot.size != Size::List {
            view::glow(canvas, target, place.scale, self.art);
            return;
        }
        let extra = match self.frame {
            Frame::Main => 10.0,
            Frame::InGame => 15.0,
        };
        let wider = Rect::new(
            target.x,
            target.y,
            target.width + extra * place.scale,
            target.height,
        );
        if self.art.has(ArtPiece::BlendBox2) {
            view::art(canvas, ArtPiece::BlendBox2, wider);
        } else {
            view::glow(canvas, wider, place.scale, self.art);
        }
    }

    /// A pop-up group entry: retail font 3 at 0.9, set against the right
    /// edge of its row.
    fn in_game_label(
        &self,
        canvas: &mut MenuCanvas,
        place: &Placement,
        index: usize,
        slot: &Slot,
        color: Color,
    ) {
        let Some(row) = self.list_row(index) else {
            return;
        };
        let [x, y, width, height] = self.in_game_list();
        let size = Size::List.text();
        let top = y + row as f32 * height + (height - size * 1.2) * 0.5;
        canvas.text_fmt_aligned(
            format_args!("{}", Caps(slot.label)),
            place.rect([x, top, width, size * 1.2]),
            size * place.scale,
            color,
            FontWeight::Semibold,
            1.2 * place.scale,
            TextAlign::End,
        );
    }

    /// A group entry's icon (`settings_icons`) at the left end of its row, opposite
    /// the label: full while the group is open or hovered, dimmed otherwise.
    fn group_icon(
        &self,
        canvas: &mut MenuCanvas,
        place: &Placement,
        index: usize,
        slot: &Slot,
        lit: bool,
    ) {
        if slot.size != Size::List {
            return;
        }
        let Some(texture) = slot.entry.icon().and_then(crate::settings_icons::texture) else {
            return;
        };
        let Some([x, y, _, height]) = self.slot_target(index, slot) else {
            return;
        };
        let size = (height - 4.0).min(22.0);
        let alpha = match (slot.enabled(), lit) {
            (false, _) => 0.35,
            (true, true) => 1.0,
            (true, false) => 0.72,
        };
        let _ = canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
            rect: place.rect([x + 6.0, y + (height - size) * 0.5, size, size]),
            texture,
            color: Color::new(1.0, 1.0, 1.0, alpha),
        });
    }

    /// The pop-up's dark box over the dimmed match.
    fn in_game_box(&self, canvas: &mut MenuCanvas, viewport: [f32; 2], place: &Placement) {
        let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
            rect: Rect::new(0.0, 0.0, viewport[0], viewport[1]),
            color: view::ink(0.35),
        });
        let rect = place.rect(IN_GAME_BOX);
        if self.art.has(ArtPiece::PopupBox) {
            view::art(canvas, ArtPiece::PopupBox, rect);
        } else {
            let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
                rect,
                color: view::ink(0.86),
            });
        }
    }

    /// Settings' two tabs on the title band: the open one white over the
    /// band, the other gold, a hovered one pulsing with retail's glow.
    /// Returns the hovered tab's description.
    fn tabs(
        &self,
        canvas: &mut MenuCanvas,
        place: &Placement,
        geometry: &Geometry,
        active: usize,
    ) -> Option<(&'static str, bool)> {
        const LABELS: [&str; 2] = ["KEY BINDINGS", "OPTIONS"];
        const HINTS: [&str; 2] = [
            "Every key binding in one list, with search",
            "Video, sound, mouse, game, interface, HUD and renderer options",
        ];
        let [x, y, width, height] = geometry.title;
        let half = width * 0.5;
        let mut hint = None;
        for (index, label) in LABELS.iter().enumerate() {
            let target = [x + index as f32 * half + half * 0.15, y, half * 0.7, height];
            let rect = place.rect(target);
            let token = CHROME_BASE + TAB_SLOTS[index] as u16;
            let hovered = canvas.token_hovered(token);
            if index == active {
                if self.art.has(ArtPiece::BlendBox) {
                    view::art(canvas, ArtPiece::BlendBox, rect);
                } else {
                    view::soft_band(canvas, rect, 0.2);
                }
            } else if hovered {
                view::glow(canvas, rect, place.scale, self.art);
            }
            if hovered {
                hint = Some((HINTS[index], true));
            }
            let color = match (hovered, index == active) {
                (true, _) => view::focus_pulse(),
                (false, true) => FOCUS,
                (false, false) => GOLD,
            };
            canvas.text_aligned(
                label,
                place.rect([target[0], y + (height - 14.0) * 0.5 - 1.0, target[2], 14.0]),
                11.5 * place.scale,
                color,
                FontWeight::Semibold,
                3.0 * place.scale,
                TextAlign::Center,
            );
            canvas.hit_region(token, rect);
        }
        hint
    }

    /// The panel title over its `menu_blendbox` band.
    fn title(&self, canvas: &mut MenuCanvas, place: &Placement, geometry: &Geometry, text: &str) {
        let band = place.rect(geometry.title);
        if self.art.has(ArtPiece::BlendBox) {
            view::art(canvas, ArtPiece::BlendBox, band);
        } else {
            view::soft_band(canvas, band, 0.16);
        }
        let [x, y, width, height] = geometry.title;
        canvas.text_aligned(
            text,
            place.rect([x, y + (height - 14.0) * 0.5 - 1.0, width, 14.0]),
            11.5 * place.scale,
            PANEL_TITLE,
            FontWeight::Semibold,
            3.0 * place.scale,
            TextAlign::Center,
        );
    }
}

/// The filled panel box with its one-unit border.
fn panel_box(canvas: &mut MenuCanvas, rect: Rect, scale: f32) {
    let draw = canvas.draw_list_mut();
    let _ = draw.push(DrawCommand::SolidRect {
        rect,
        color: PANEL_FILL,
    });
    let line = scale.max(1.0);
    for edge in [
        Rect::new(rect.x, rect.y, rect.width, line),
        Rect::new(rect.x, rect.bottom() - line, rect.width, line),
        Rect::new(rect.x, rect.y, line, rect.height),
        Rect::new(rect.right() - line, rect.y, line, rect.height),
    ] {
        let _ = draw.push(DrawCommand::SolidRect {
            rect: edge,
            color: PANEL_BORDER,
        });
    }
}

impl PanelPlace {
    /// Item rows the panel holds.
    pub(crate) fn capacity(&self) -> usize {
        self.frame.capacity()
    }

    /// Keep a picture column between the labels and the values
    /// ([`Self::row_icon`]), for panels whose items have pictures.
    pub(crate) fn with_icon_column(mut self) -> Self {
        self.icons = true;
        self
    }

    /// Side of a row picture, in canvas units.
    fn icon_side(&self) -> f32 {
        self.geometry().row_height - 2.0
    }

    /// Row `slot`'s picture, in the column after the label.
    pub(crate) fn row_icon(&self, canvas: &mut MenuCanvas, slot: usize, texture: TextureId) {
        let geometry = self.geometry();
        let side = self.icon_side();
        let top = geometry.first_row + slot as f32 * geometry.row_height + 1.0;
        let _ = canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
            rect: self
                .place
                .rect([geometry.label_end + ICON_GAP, top, side, side]),
            texture,
            color: Color::new(1.0, 1.0, 1.0, 1.0),
        });
    }

    /// The search field over the rows: "SEARCH" in the label column, then the
    /// typed text with a cursor while it has focus, or `prompt` while empty;
    /// `found` (matches, while a search is typed) at the row's end.
    pub(crate) fn search_field(
        &self,
        canvas: &mut MenuCanvas,
        text: &str,
        active: bool,
        prompt: &str,
        found: Option<usize>,
    ) {
        let geometry = self.geometry();
        let [x, y, width, height] = geometry.search;
        let rect = self.place.rect([x, y, width, height]);
        let hovered = canvas.token_hovered(SEARCH_TOKEN);
        if active {
            view::soft_band(canvas, rect, 0.22);
        }
        let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
            rect: self.place.rect([
                geometry.label_end + VALUE_GAP,
                y + height - 1.5,
                x + width - geometry.label_end - VALUE_GAP - 6.0,
                1.0,
            ]),
            color: if active || hovered {
                FOCUS
            } else {
                Color::new(OPTION.r, OPTION.g, OPTION.b, 0.5)
            },
        });
        let s = self.place.scale;
        let line = geometry.text * 1.25;
        let top = y + (height - line) * 0.5;
        canvas.text_aligned(
            "SEARCH",
            self.place.rect([x, top, geometry.label_end - x, line]),
            geometry.text * s,
            if active || hovered {
                focus_text()
            } else {
                GOLD
            },
            FontWeight::Semibold,
            1.2 * s,
            TextAlign::End,
        );
        let value = [
            geometry.label_end + VALUE_GAP,
            top,
            x + width - geometry.label_end - VALUE_GAP - 60.0,
            line,
        ];
        if text.is_empty() && !active {
            canvas.text_aligned(
                prompt,
                self.place.rect(value),
                geometry.text * s,
                Color::new(OPTION.r, OPTION.g, OPTION.b, 0.55),
                FontWeight::Regular,
                0.3 * s,
                TextAlign::Start,
            );
        } else {
            canvas.text_fmt_aligned(
                format_args!("{text}{}", if active { "_" } else { "" }),
                self.place.rect(value),
                geometry.text * s,
                FOCUS,
                FontWeight::Regular,
                0.3 * s,
                TextAlign::Start,
            );
        }
        if let Some(found) = found {
            canvas.text_fmt_aligned(
                format_args!("{found} FOUND"),
                self.place.rect([x + width - 62.0, top, 56.0, line]),
                (geometry.text - 2.0) * s,
                if found == 0 { DISABLED } else { GOLD },
                FontWeight::Semibold,
                0.8 * s,
                TextAlign::End,
            );
        }
        canvas.hit_region(SEARCH_TOKEN, rect);
    }

    /// Canvas box of a dropdown of `count` choices on row `slot`: under the
    /// row's value, or above it when the rows end first, within the rows.
    fn dropdown_box(&self, slot: usize, count: usize) -> [f32; 4] {
        let geometry = self.geometry();
        let row_height = geometry.row_height;
        let x = self.value_x() - 4.0;
        let width = (self.value_end() - x).min(150.0);
        let height = count as f32 * row_height + 4.0;
        let row_top = geometry.first_row + slot as f32 * row_height;
        let rows_end = geometry.first_row + geometry.rows as f32 * row_height;
        let top = if row_top + row_height + height <= rows_end {
            row_top + row_height
        } else {
            (row_top - height).max(geometry.first_row)
        };
        [x, top, width, height]
    }

    /// Whether a dropdown of `count` choices on row `slot` covers row
    /// `other`'s value: text draws over every shape, so the rows under it
    /// leave their values out while it is open.
    pub(crate) fn dropdown_covers(&self, slot: usize, count: usize, other: usize) -> bool {
        let geometry = self.geometry();
        let [_, top, _, height] = self.dropdown_box(slot, count);
        let row_top = geometry.first_row + other as f32 * geometry.row_height;
        row_top < top + height && top < row_top + geometry.row_height
    }

    /// A classic+ dropdown under row `slot`'s value: a rounded list of
    /// `labels` over a soft shadow, the `highlighted` one on the row band and
    /// the value in use (`current`) in gold with a dot, each answering to
    /// `token_base` plus its index. It opens upward when the panel's box has
    /// no room under the row.
    pub(crate) fn dropdown(
        &self,
        canvas: &mut MenuCanvas,
        slot: usize,
        labels: &[&str],
        highlighted: usize,
        current: usize,
        token_base: u16,
    ) {
        let geometry = self.geometry();
        let row_height = geometry.row_height;
        let [x, top, width, height] = self.dropdown_box(slot, labels.len());
        let s = self.place.scale;
        let rect = self.place.rect([x, top, width, height]);
        let draw = canvas.draw_list_mut();
        // A soft shadow under the list, then the list itself.
        let _ = draw.push(DrawCommand::RoundedRect {
            rect: Rect::new(rect.x + 1.5 * s, rect.y + 2.5 * s, rect.width, rect.height),
            radius: 3.0 * s,
            color: view::ink(0.5),
        });
        let _ = draw.push(DrawCommand::RoundedRect {
            rect,
            radius: 3.0 * s,
            color: Color::new(0.03, 0.04, 0.16, 0.97),
        });
        let _ = draw.push(DrawCommand::Border {
            rect,
            radius: 3.0 * s,
            width: s.max(1.0) * 0.6,
            color: FOCUS_EDGE,
        });
        for (index, label) in labels.iter().enumerate() {
            let row = [
                x + 2.0,
                top + 2.0 + index as f32 * row_height,
                width - 4.0,
                row_height,
            ];
            let target = self.place.rect(row);
            let hovered = canvas.token_hovered(token_base + index as u16);
            if index == highlighted || hovered {
                let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: target,
                    radius: 2.0 * s,
                    color: BAND,
                });
            }
            let color = if index == highlighted || hovered {
                FOCUS
            } else if index == current {
                GOLD
            } else {
                DETAIL_TEXT
            };
            let line = geometry.text * 1.25;
            canvas.text_fmt_aligned(
                format_args!("{}", view::Sentence(label)),
                self.place.rect([
                    row[0] + 6.0,
                    row[1] + (row_height - line) * 0.5,
                    row[2] - 12.0,
                    line,
                ]),
                geometry.text * s,
                color,
                FontWeight::Regular,
                0.4 * s,
                TextAlign::Start,
            );
            if index == current {
                // A gold dot marks the choice in use.
                let side = 3.2;
                let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: self.place.rect([
                        row[0] + row[2] - 8.0,
                        row[1] + (row_height - side) * 0.5,
                        side,
                        side,
                    ]),
                    radius: side * 0.5 * s,
                    color: view::gold(0.95),
                });
            }
            canvas.hit_region(token_base + index as u16, target);
        }
    }

    /// A heading row inside the list (a category of the key bindings, a
    /// group of search results): `LABEL` capitals over a thin rule.
    pub(crate) fn heading(&self, canvas: &mut MenuCanvas, slot: usize, text: &str) {
        let geometry = self.geometry();
        let top = geometry.first_row + slot as f32 * geometry.row_height;
        let s = self.place.scale;
        let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
            rect: self.place.rect([
                geometry.row_x + 6.0,
                top + geometry.row_height - 2.0,
                geometry.row_width - 18.0,
                1.0,
            ]),
            color: Color::new(DETAIL_BORDER.r, DETAIL_BORDER.g, DETAIL_BORDER.b, 0.9),
        });
        canvas.text_fmt_aligned(
            format_args!("{}", Caps(text)),
            self.text_rect(slot, geometry.row_x + 6.0, self.value_end()),
            (geometry.text - 1.0) * s,
            PANEL_TITLE,
            FontWeight::Semibold,
            2.4 * s,
            TextAlign::Start,
        );
    }

    /// Window scale of one canvas unit.
    pub(crate) fn scale(&self) -> f32 {
        self.place.scale
    }

    fn geometry(&self) -> &'static Geometry {
        self.frame.geometry()
    }

    /// Item row `slot` of the panel (window coordinates).
    pub(crate) fn row(&self, slot: usize) -> Rect {
        let geometry = self.geometry();
        self.place.rect([
            geometry.row_x,
            geometry.first_row + slot as f32 * geometry.row_height,
            geometry.row_width,
            geometry.row_height,
        ])
    }

    /// Classic+'s band behind the focused item: a soft rounded band across
    /// the row with a gold bar at its left end, in place of retail's
    /// `menu_blendbox` streak.
    pub(crate) fn highlight(&self, canvas: &mut MenuCanvas, slot: usize) {
        let geometry = self.geometry();
        let top = self.row_top(slot);
        let s = self.place.scale;
        let band = self.place.rect([
            geometry.row_x + 3.0,
            top + 0.5,
            geometry.row_width - 12.0,
            geometry.row_height - 1.0,
        ]);
        let draw = canvas.draw_list_mut();
        let _ = draw.push(DrawCommand::RoundedRect {
            rect: band,
            radius: 2.5 * s,
            color: BAND,
        });
        let _ = draw.push(DrawCommand::RoundedRect {
            rect: self.place.rect([
                geometry.row_x + 3.0,
                top + 2.0,
                1.5,
                geometry.row_height - 4.0,
            ]),
            radius: 0.75 * s,
            color: view::gold(0.95),
        });
    }

    /// Top of row `slot` on the canvas.
    fn row_top(&self, slot: usize) -> f32 {
        let geometry = self.geometry();
        geometry.first_row + slot as f32 * geometry.row_height
    }

    /// Canvas box `width` by `height` from `x`, centred on row `slot`.
    fn centred(&self, slot: usize, x: f32, width: f32, height: f32) -> [f32; 4] {
        let geometry = self.geometry();
        [
            x,
            self.row_top(slot) + (geometry.row_height - height) * 0.5,
            width,
            height,
        ]
    }

    /// Right edge of a row's controls, clear of its reset button.
    fn controls_end(&self) -> f32 {
        self.value_end() - RESET_WIDTH
    }

    /// Height of a framed control (a choice field, segments, a slider's
    /// number) on a row: a little above the text, with a gap to the next row.
    fn control_height(&self) -> f32 {
        let geometry = self.geometry();
        (geometry.text * 1.3).min(geometry.row_height - 2.5)
    }

    /// A framed control's background and edge: brighter while focused.
    fn frame_control(&self, canvas: &mut MenuCanvas, rect: Rect, focused: bool) {
        let s = self.place.scale;
        let draw = canvas.draw_list_mut();
        let _ = draw.push(DrawCommand::RoundedRect {
            rect,
            radius: 2.5 * s,
            color: view::ink(0.5),
        });
        let _ = draw.push(DrawCommand::Border {
            rect,
            radius: 2.5 * s,
            width: s.max(1.0) * 0.6,
            color: if focused { FOCUS_EDGE } else { EDGE },
        });
    }

    /// Classic+ switch for an on/off setting on row `slot`: a pill track,
    /// gold while on, its knob at the on (right) or off end, and On or Off
    /// after it. The row itself takes the click.
    pub(crate) fn switch(&self, canvas: &mut MenuCanvas, slot: usize, on: bool, focused: bool) {
        let geometry = self.geometry();
        let s = self.place.scale;
        let height = (geometry.text * 0.8).min(geometry.row_height - 5.0);
        let width = height * 2.0;
        let x = self.value_x();
        let track = self.place.rect(self.centred(slot, x, width, height));
        let radius = track.height * 0.5;
        let inset = height * 0.17;
        let knob = height - 2.0 * inset;
        let knob_x = if on {
            x + width - inset - knob
        } else {
            x + inset
        };
        let knob = self.place.rect(self.centred(slot, knob_x, knob, knob));
        let draw = canvas.draw_list_mut();
        let _ = draw.push(DrawCommand::RoundedRect {
            rect: track,
            radius,
            color: if on {
                view::gold(0.92)
            } else {
                view::ink(0.55)
            },
        });
        let _ = draw.push(DrawCommand::Border {
            rect: track,
            radius,
            width: s.max(1.0) * 0.6,
            color: match (focused, on) {
                (true, _) => FOCUS_EDGE,
                (false, true) => view::gold(1.0),
                (false, false) => EDGE,
            },
        });
        let _ = draw.push(DrawCommand::RoundedRect {
            rect: knob,
            radius: knob.height * 0.5,
            color: if on {
                FOCUS
            } else {
                Color::new(OPTION.r, OPTION.g, OPTION.b, 0.9)
            },
        });
        let text = if on { VALUE } else { dim(OPTION, 0.75) };
        self.value_from(
            canvas,
            slot,
            x + width + 5.0,
            if on { "On" } else { "Off" },
            if focused { FOCUS } else { text },
        );
    }

    /// Classic+ segmented choice on row `slot`: every choice side by side in
    /// one frame, the one in use filled gold; choice `i` answers to
    /// `token_base + i`.
    pub(crate) fn segments(
        &self,
        canvas: &mut MenuCanvas,
        slot: usize,
        labels: &[&str],
        current: usize,
        focused: bool,
        token_base: u16,
    ) {
        let geometry = self.geometry();
        let s = self.place.scale;
        let x = self.value_x() - 2.0;
        let count = labels.len().max(1);
        let total = (self.controls_end() - x).min(count as f32 * 52.0);
        let height = self.control_height();
        let outer = self.centred(slot, x, total, height);
        self.frame_control(canvas, self.place.rect(outer), focused);
        let width = total / count as f32;
        let line = geometry.text * 1.25;
        for (index, label) in labels.iter().enumerate() {
            let token = token_base + index as u16;
            let segment = [outer[0] + index as f32 * width, outer[1], width, height];
            let inner = self.place.rect([
                segment[0] + 1.0,
                segment[1] + 1.0,
                segment[2] - 2.0,
                segment[3] - 2.0,
            ]);
            let hovered = canvas.token_hovered(token);
            if index == current || hovered {
                let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: inner,
                    radius: 2.0 * s,
                    color: if index == current {
                        view::gold(0.92)
                    } else {
                        Color::new(OPTION.r, OPTION.g, OPTION.b, 0.2)
                    },
                });
            }
            let color = match (index == current, hovered) {
                (true, _) => view::ink(1.0),
                (false, true) => FOCUS,
                (false, false) => OPTION,
            };
            canvas.text_fmt_aligned(
                format_args!("{}", view::Sentence(label)),
                self.place.rect([
                    segment[0],
                    segment[1] + (height - line) * 0.5,
                    segment[2],
                    line,
                ]),
                geometry.text * 0.92 * s,
                color,
                FontWeight::Semibold,
                0.3 * s,
                TextAlign::Center,
            );
            canvas.hit_region(token, self.place.rect(segment));
        }
    }

    /// Classic+ field for a value chosen from a list (a long choice, the
    /// display mode, the resolution, the HUD): the value in a frame with a
    /// caret at its right end saying a click opens the list; without
    /// `caret`, a field being typed in.
    pub(crate) fn choice_field(
        &self,
        canvas: &mut MenuCanvas,
        slot: usize,
        text: std::fmt::Arguments<'_>,
        focused: bool,
        caret: bool,
    ) {
        let geometry = self.geometry();
        let s = self.place.scale;
        let x = self.value_x() - 3.0;
        let width = self.controls_end() - x;
        let field = self.centred(slot, x, width, self.control_height());
        self.frame_control(canvas, self.place.rect(field), focused);
        if focused && !caret && crate::text_entry::selection_shown() {
            // Ctrl+A: the field's text is selected.
            let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
                rect: self.place.rect([
                    field[0] + 2.0,
                    field[1] + 2.0,
                    field[2] - 4.0,
                    field[3] - 4.0,
                ]),
                color: Color::new(0.35, 0.55, 1.0, 0.4),
            });
        }
        let text_end = x + width - if caret { 10.0 } else { 3.0 };
        canvas.text_fmt_aligned(
            text,
            self.text_rect(slot, self.value_x(), text_end),
            geometry.text * s,
            if focused { FOCUS } else { VALUE },
            FontWeight::Regular,
            0.4 * s,
            TextAlign::Start,
        );
        if caret {
            // A small downward triangle in three steps.
            let centre = x + width - 5.5;
            let middle = field[1] + field[3] * 0.5;
            for (step, half) in [2.6_f32, 1.7, 0.8].into_iter().enumerate() {
                let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
                    rect: self.place.rect([
                        centre - half,
                        middle - 1.4 + step as f32 * 0.95,
                        half * 2.0,
                        0.95,
                    ]),
                    color: if focused { FOCUS } else { OPTION },
                });
            }
        }
    }

    /// Classic+ reset button at row `slot`'s right end, for a setting changed
    /// from its default: a gold turning arrow answering to `token`.
    pub(crate) fn reset_button(&self, canvas: &mut MenuCanvas, slot: usize, token: u16) {
        let geometry = self.geometry();
        let s = self.place.scale;
        let target = self.place.rect(self.centred(
            slot,
            self.value_end() - RESET_WIDTH,
            RESET_WIDTH,
            geometry.row_height,
        ));
        let hovered = canvas.token_hovered(token);
        let centre = [
            target.x + target.width * 0.5,
            target.y + target.height * 0.5,
        ];
        let radius = geometry.text * 0.32 * s;
        let color = if hovered { FOCUS } else { view::gold(0.9) };
        let draw = canvas.draw_list_mut();
        if hovered {
            let side = radius * 3.2;
            let _ = draw.push(DrawCommand::RoundedRect {
                rect: Rect::new(centre[0] - side * 0.5, centre[1] - side * 0.5, side, side),
                radius: side * 0.5,
                color: view::gold(0.25),
            });
        }
        let start = -std::f32::consts::FRAC_PI_2;
        let sweep = std::f32::consts::PI * 1.55;
        let _ = draw.push(DrawCommand::Arc {
            center: centre,
            radius,
            width: 1.2 * s,
            start,
            sweep,
            color,
            knockout: None,
        });
        // The arrow's head: a dot at the stroke's end.
        let end = start + sweep;
        let head = 2.4 * s;
        let _ = draw.push(DrawCommand::RoundedRect {
            rect: Rect::new(
                centre[0] + radius * end.cos() - head * 0.5,
                centre[1] + radius * end.sin() - head * 0.5,
                head,
                head,
            ),
            radius: head * 0.5,
            color,
        });
        canvas.hit_region(token, target);
    }

    /// Text box of row `slot` from canvas x `from` to `to`.
    fn text_rect(&self, slot: usize, from: f32, to: f32) -> Rect {
        let geometry = self.geometry();
        let line = geometry.text * 1.25;
        let top = geometry.first_row
            + slot as f32 * geometry.row_height
            + (geometry.row_height - line) * 0.5;
        self.place.rect([from, top, (to - from).max(0.0), line])
    }

    /// An item's label as written (classic+ sets labels in sentence case, not
    /// retail's capitals, so a long panel reads as words), set against the
    /// label column's right edge.
    pub(crate) fn label(&self, canvas: &mut MenuCanvas, slot: usize, text: &str, color: Color) {
        self.label_marked(canvas, slot, text, color, false);
    }

    /// [`Self::label`], with classic+'s gold `*` closing the label column
    /// when `later` (the setting applies after a restart, a graphics reload or on the next map).
    pub(crate) fn label_marked(
        &self,
        canvas: &mut MenuCanvas,
        slot: usize,
        text: &str,
        color: Color,
        later: bool,
    ) {
        let geometry = self.geometry();
        let end = if later {
            geometry.label_end - MARK_WIDTH
        } else {
            geometry.label_end
        };
        canvas.text_aligned(
            text,
            self.text_rect(slot, geometry.row_x, end),
            geometry.text * self.place.scale,
            color,
            FontWeight::Regular,
            0.3 * self.place.scale,
            TextAlign::End,
        );
        if later {
            canvas.text_aligned(
                "*",
                self.text_rect(slot, end + 1.0, geometry.label_end),
                geometry.text * self.place.scale,
                GOLD,
                FontWeight::Semibold,
                0.0,
                TextAlign::Start,
            );
        }
    }

    /// The scrollbar of a list showing `rows` rows: along the panel's right
    /// edge, clear of the values (in the in-game pop-up the rows end well
    /// inside the panel).
    pub(crate) fn scrollbar_track(&self, rows: usize) -> Rect {
        let geometry = self.geometry();
        let [x, _, width, _] = geometry.panel;
        self.place.rect([
            x + width - 5.0,
            geometry.first_row,
            3.0,
            rows as f32 * geometry.row_height,
        ])
    }

    /// Classic+'s mark of a row changed from its default: a gold dot just
    /// after its label.
    pub(crate) fn changed_mark(&self, canvas: &mut MenuCanvas, slot: usize) {
        let geometry = self.geometry();
        let side = 3.2;
        let rect = self
            .place
            .rect(self.centred(slot, geometry.label_end + 1.8, side, side));
        let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
            rect,
            radius: rect.height * 0.5,
            color: view::gold(0.95),
        });
    }

    /// The classic+ detail box: `detail`'s title and value over a rule,
    /// its two lines, and its facts line with the console name at the end.
    pub(crate) fn detail(&self, canvas: &mut MenuCanvas, detail: &Detail<'_>) {
        let [x, y, w, h] = self.geometry().detail;
        let s = self.place.scale;
        let rect = self.place.rect([x, y, w, h]);
        let draw = canvas.draw_list_mut();
        let _ = draw.push(DrawCommand::SolidRect {
            rect,
            color: view::ink(0.45),
        });
        let _ = draw.push(DrawCommand::Border {
            rect,
            radius: 0.0,
            width: s.max(1.0),
            color: DETAIL_BORDER,
        });
        let _ = draw.push(DrawCommand::SolidRect {
            rect: self.place.rect([x + 6.0, y + 18.0, w - 12.0, 1.0]),
            color: Color::new(DETAIL_BORDER.r, DETAIL_BORDER.g, DETAIL_BORDER.b, 0.7),
        });
        let line = |canvas: &mut MenuCanvas,
                    text: std::fmt::Arguments<'_>,
                    box_: [f32; 4],
                    size: f32,
                    color: Color,
                    weight: FontWeight,
                    align: TextAlign| {
            let [bx, by, bw, bh] = box_;
            let height = size * 1.25;
            canvas.text_fmt_aligned(
                text,
                self.place.rect([bx, by + (bh - height) * 0.5, bw, height]),
                size * s,
                color,
                weight,
                0.3 * s,
                align,
            );
        };
        let split = x + w * 0.6;
        let title_x = match detail.badge {
            Some(texture) => {
                let _ = canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                    rect: self.place.rect([x + 5.0, y + 2.0, 15.0, 15.0]),
                    texture,
                    color: Color::new(1.0, 1.0, 1.0, 1.0),
                });
                x + 24.0
            }
            None => x + 6.0,
        };
        line(
            canvas,
            format_args!("{}", Caps(detail.title)),
            [title_x, y + 3.0, split - title_x - 2.0, 14.0],
            11.5,
            GOLD,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        line(
            canvas,
            format_args!("{}", Caps(detail.value)),
            [split, y + 3.0, x + w - split - 6.0, 14.0],
            11.0,
            FOCUS,
            FontWeight::Regular,
            TextAlign::End,
        );
        // The picture fills the box under the rule; the lines follow it.
        let text_x = match detail.icon {
            Some(texture) => {
                let side = h - 24.0;
                let _ = canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                    rect: self.place.rect([x + 6.0, y + 21.0, side, side]),
                    texture,
                    color: Color::new(1.0, 1.0, 1.0, 1.0),
                });
                x + 12.0 + side
            }
            None => x + 6.0,
        };
        for (index, text) in detail.lines.iter().enumerate() {
            line(
                canvas,
                format_args!("{text}"),
                [
                    text_x,
                    y + 21.0 + index as f32 * 12.5,
                    x + w - 6.0 - text_x,
                    12.0,
                ],
                10.5,
                DETAIL_TEXT,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        let facts_y = y + h - 15.0;
        line(
            canvas,
            format_args!("{}", detail.facts),
            [text_x, facts_y, x + w * 0.72 - text_x, 12.0],
            10.0,
            PANEL_TITLE,
            FontWeight::Regular,
            TextAlign::Start,
        );
        line(
            canvas,
            format_args!("{}", detail.name),
            [x + w * 0.72, facts_y, w * 0.28 - 6.0, 12.0],
            9.5,
            Color::new(DETAIL_TEXT.r, DETAIL_TEXT.g, DETAIL_TEXT.b, 0.6),
            FontWeight::Regular,
            TextAlign::End,
        );
    }

    /// Canvas x where an item's value starts: after the picture column
    /// when the panel keeps one.
    fn value_x(&self) -> f32 {
        let column = if self.icons {
            ICON_GAP + self.icon_side()
        } else {
            0.0
        };
        self.geometry().label_end + VALUE_GAP + column
    }

    /// Right edge of the panel's text, inside its box.
    fn value_end(&self) -> f32 {
        let [x, _, width, _] = self.geometry().panel;
        x + width - 4.0
    }

    /// An item's value in capitals, from the value column to the panel's
    /// edge.
    pub(crate) fn value(&self, canvas: &mut MenuCanvas, slot: usize, text: &str, color: Color) {
        self.value_fmt(canvas, slot, format_args!("{}", Caps(text)), color);
    }

    /// An item's value as written, such as typed text (an address or a
    /// name), which capitals would misrepresent.
    pub(crate) fn value_plain(
        &self,
        canvas: &mut MenuCanvas,
        slot: usize,
        text: &str,
        color: Color,
    ) {
        self.value_from(canvas, slot, self.value_x(), text, color);
    }

    /// An item's value, formatted without allocating.
    pub(crate) fn value_fmt(
        &self,
        canvas: &mut MenuCanvas,
        slot: usize,
        text: std::fmt::Arguments<'_>,
        color: Color,
    ) {
        let geometry = self.geometry();
        canvas.text_fmt_aligned(
            text,
            self.text_rect(slot, self.value_x(), self.value_end()),
            geometry.text * self.place.scale,
            color,
            FontWeight::Regular,
            0.4 * self.place.scale,
            TextAlign::Start,
        );
    }

    fn value_from(
        &self,
        canvas: &mut MenuCanvas,
        slot: usize,
        from: f32,
        text: &str,
        color: Color,
    ) {
        let geometry = self.geometry();
        canvas.text_aligned(
            text,
            self.text_rect(slot, from, self.value_end()),
            geometry.text * self.place.scale,
            color,
            FontWeight::Regular,
            0.4 * self.place.scale,
            TextAlign::Start,
        );
    }

    /// The slider bar of row `slot` (window coordinates), where retail
    /// draws it: after the label, at the top of the item.
    pub(crate) fn slider_bar(&self, slot: usize) -> Rect {
        let geometry = self.geometry();
        let top = geometry.first_row
            + slot as f32 * geometry.row_height
            + (geometry.row_height - SLIDER[1]) * 0.5;
        self.place.rect([self.value_x(), top, SLIDER[0], SLIDER[1]])
    }

    /// A slider's rail across row `slot` (window coordinates): a thin line
    /// through the middle of retail's slider bar, which stays the pointer's
    /// span ([`Frame::slider_span`]).
    fn slider_rail(&self, slot: usize) -> Rect {
        let bar = self.slider_bar(slot);
        let height = 3.0 * self.place.scale;
        Rect::new(
            bar.x,
            bar.y + (bar.height - height) * 0.5,
            bar.width,
            height,
        )
    }

    /// Draw the slider rail of row `slot`: classic+'s slim rounded track in
    /// place of retail's slider art.
    pub(crate) fn draw_slider_bar(&self, canvas: &mut MenuCanvas, slot: usize) {
        let rail = self.slider_rail(slot);
        let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: rail,
            radius: rail.height * 0.5,
            color: TRACK,
        });
    }

    /// Draw the slider of row `slot` at `ratio` along its rail: the rail
    /// filled gold up to a round knob, ringed while `focused`; drawn after
    /// every rail.
    pub(crate) fn draw_slider_thumb(
        &self,
        canvas: &mut MenuCanvas,
        slot: usize,
        ratio: f32,
        focused: bool,
    ) {
        let rail = self.slider_rail(slot);
        let s = self.place.scale;
        let ratio = ratio.clamp(0.0, 1.0);
        let x = rail.x + rail.width * ratio;
        let draw = canvas.draw_list_mut();
        if ratio > 0.0 {
            let _ = draw.push(DrawCommand::RoundedRect {
                rect: Rect::new(rail.x, rail.y, rail.width * ratio, rail.height),
                radius: rail.height * 0.5,
                color: view::gold(0.92),
            });
        }
        let side = self.geometry().text * 0.8 * s;
        let middle = rail.y + rail.height * 0.5;
        let knob = Rect::new(x - side * 0.5, middle - side * 0.5, side, side);
        if focused {
            let ring = 1.6 * s;
            let _ = draw.push(DrawCommand::RoundedRect {
                rect: Rect::new(
                    knob.x - ring,
                    knob.y - ring,
                    knob.width + 2.0 * ring,
                    knob.height + 2.0 * ring,
                ),
                radius: side * 0.5 + ring,
                color: view::gold(0.35),
            });
        }
        let _ = draw.push(DrawCommand::RoundedRect {
            rect: knob,
            radius: side * 0.5,
            color: FOCUS,
        });
    }

    /// Canvas box of the number after a slider rail.
    fn slider_chip(&self, slot: usize) -> [f32; 4] {
        let from = self.value_x() + SLIDER[0] + VALUE_GAP - 2.0;
        let width = (self.controls_end() - from).min(40.0);
        self.centred(slot, from, width, self.control_height())
    }

    /// The number shown after a slider rail, and its click target for typed
    /// entry.
    pub(crate) fn slider_value_rect(&self, slot: usize) -> Rect {
        let [x, _, width, _] = self.slider_chip(slot);
        self.text_rect(slot, x, x + width)
    }

    /// The number after a slider rail, centred in a small frame.
    pub(crate) fn slider_value(
        &self,
        canvas: &mut MenuCanvas,
        slot: usize,
        text: &str,
        color: Color,
        focused: bool,
    ) {
        let chip = self.slider_chip(slot);
        self.frame_control(canvas, self.place.rect(chip), focused);
        let geometry = self.geometry();
        canvas.text_aligned(
            text,
            self.slider_value_rect(slot),
            geometry.text * self.place.scale,
            color,
            FontWeight::Regular,
            0.3 * self.place.scale,
            TextAlign::Center,
        );
    }

    /// End the screen: the description line shows the hovered button's
    /// description, else `item_hint` (the panel's own line), in retail's
    /// description colour.
    pub(crate) fn finish(&self, canvas: &mut MenuCanvas, item_hint: Option<&str>) {
        let geometry = self.geometry();
        let (text, color) = match (self.hovered_hint, item_hint) {
            (Some((hint, enabled)), _) => (hint, if enabled { HINT } else { DISABLED }),
            (None, Some(hint)) => (hint, HINT),
            (None, None) => ("", HINT),
        };
        if !text.is_empty() {
            let s = self.place.scale;
            canvas.text_aligned(
                text,
                self.place.centered(geometry.hint, 560.0, 18.0),
                13.0 * s,
                color,
                FontWeight::Regular,
                0.3 * s,
                TextAlign::Center,
            );
        }
        canvas.pop_opacity();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_fit_their_panel() {
        for frame in [Frame::Main, Frame::InGame] {
            let geometry = frame.geometry();
            let [_, top, _, height] = geometry.panel;
            let last = geometry.first_row + geometry.rows as f32 * geometry.row_height;
            assert!(geometry.first_row >= top, "{frame:?}");
            // The search field sits on the panel's first row, over the items.
            let [_, search_y, _, search_h] = geometry.search;
            assert!(search_y >= top && search_y + search_h <= geometry.first_row);
            assert!(last <= top + height, "{frame:?}: {last} > {}", top + height);
            let (offset, width) = frame.slider_span();
            assert!(offset > 0.0 && offset + width < 1.0, "{frame:?}");
            // The classic+ detail box sits under the rows, as wide as the
            // panel, within retail's panel box (`setup.menu` ends at 412,
            // `ingame_setup.menu` at 326) and above the description line.
            let [detail_x, detail_y, detail_w, detail_h] = geometry.detail;
            let [panel_x, _, panel_w, _] = geometry.panel;
            assert!(detail_y > top + height, "{frame:?}");
            assert_eq!((detail_x, detail_w), (panel_x, panel_w), "{frame:?}");
            let retail_bottom = match frame {
                Frame::Main => 412.0,
                Frame::InGame => 35.0 + 41.0 + 250.0,
            };
            assert!(detail_y + detail_h <= retail_bottom, "{frame:?}");
            assert!(detail_y + detail_h < geometry.hint[1], "{frame:?}");
            // Its four lines fit: title 3..17, two lines from 21, facts at h - 15.
            assert!(21.0 + 2.0 * 12.5 <= detail_h - 15.0 + 0.5, "{frame:?}");
        }
    }

    #[test]
    fn slider_ratio_follows_the_bar() {
        let row = Rect::new(100.0, 0.0, 340.0, 14.0);
        let span = Frame::Main.slider_span();
        let left = row.x + row.width * span.0;
        let right = left + row.width * span.1;
        let close = |a: f32, b: f32| (a - b).abs() < 1e-5;
        assert!(close(slider_ratio(row, left, span), 0.0));
        assert!(close(slider_ratio(row, right, span), 1.0));
        assert!(close(slider_ratio(row, (left + right) * 0.5, span), 0.5));
        assert_eq!(slider_ratio(row, 0.0, span), 0.0);
        assert_eq!(slider_ratio(row, 1000.0, span), 1.0);
    }

    #[test]
    fn chrome_tokens_round_trip() {
        assert_eq!(chrome_slot(CHROME_BASE), Some(0));
        assert_eq!(chrome_slot(CHROME_BASE + 14), Some(14));
        assert_eq!(chrome_slot(799), None);
        assert_eq!(chrome_slot(900), None);
    }

    #[test]
    fn in_game_list_holds_only_groups() {
        for page in [Page::Setup, Page::Controls] {
            let frame = PanelFrame {
                frame: Frame::InGame,
                page,
                active: page.opening_panel().unwrap(),
                art: ArtSet::default(),
            };
            let rows: Vec<_> = (0..page.slots().len())
                .filter_map(|index| frame.list_row(index))
                .collect();
            let groups = page
                .slots()
                .iter()
                .filter(|slot| slot.size == Size::List)
                .count();
            assert_eq!(rows, (0..groups).collect::<Vec<_>>());
            let [_, y, _, height] = frame.in_game_list();
            let [_, box_y, _, box_height] = IN_GAME_BOX;
            assert!(y + groups as f32 * height <= box_y + box_height, "{page:?}");
        }
    }
}
