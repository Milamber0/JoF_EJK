//! Drawing of the cosmetics window, JoF EJK's `ingame_cosmetics`: its frame,
//! title band, gold captions, list boxes with the worn row filled, Show
//! Cosmetics, Remove All and Apply. Where JoF drew a preview model, SJK
//! lists the capes beside the hats and says what is worn underneath; an
//! empty list says where cosmetics come from.

use super::layout::{self, Item};
use super::view::{CAPE_BASE, CAPES_SCROLL, HAT_BASE, HATS_SCROLL, LIST_BACK, LIST_BORDER, VALUE};
use super::{ClassicPage, Frame};
use crate::menu::art::ArtPiece;
use crate::menu::classic::layout::Placement;
use crate::menu::classic::view::{FOCUS, GOLD, glow};
use crate::player_menu::PlayerMenu;
use sjk_client::CosmeticSlot;
use sjk_ui::{Color, FontWeight, TextAlign};

/// Row height of the lists (`elementheight 16`).
pub(super) const ROW: f32 = 16.0;
/// Rows each list token range can name.
pub(super) const MAX_ROWS: usize = 256;
/// The worn row's fill (`outlinecolor .66 .66 1 .6`).
const WORN: Color = Color::new(0.66, 0.66, 1.0, 0.6);

/// The list a slot is shown in, its first row token and wheel target.
pub(super) fn list_of(slot: CosmeticSlot) -> (Item, u16, u16) {
    match slot {
        CosmeticSlot::Hat => (Item::Hats, HAT_BASE, HATS_SCROLL),
        CosmeticSlot::Cape => (Item::Capes, CAPE_BASE, CAPES_SCROLL),
    }
}

/// The slot and row a list token names.
pub(super) fn row_of(token: u16) -> Option<(CosmeticSlot, usize)> {
    CosmeticSlot::ALL.into_iter().find_map(|slot| {
        let (_, base, _) = list_of(slot);
        let row = usize::from(token.checked_sub(base)?);
        (row < MAX_ROWS).then_some((slot, row))
    })
}

impl PlayerMenu {
    /// Window, title and captions, and the line saying what is worn.
    pub(super) fn cosmetics_page(&mut self, place: &Placement, frame: Frame) {
        let page = ClassicPage::Cosmetics;
        let at = |canvas| layout::place(page, frame, canvas);
        if frame == Frame::Full {
            // JoF covers the profile behind with the main menu backdrop.
            self.piece(place, ArtPiece::Background, [0.0, 0.0, 640.0, 480.0]);
        }
        self.window_box(place, page, frame);
        self.band_title(place, at([35.0, 5.0, 360.0, 28.0]), "Cosmetics", 15.0);
        for (text, canvas) in [
            ("Hats", [15.0, 40.0, 130.0, 18.0]),
            ("Capes", [150.0, 40.0, 130.0, 18.0]),
        ] {
            self.label(
                place,
                at(canvas),
                text,
                15.0,
                GOLD,
                FontWeight::Semibold,
                TextAlign::Start,
            );
        }
        // Where JoF EJK drew the model wearing them: the live preview.
        let preview = at(layout::COSMETICS_PREVIEW);
        self.fill(place, preview, Color::new(0.0, 0.0, 0.0, 0.35));
        self.border(place, preview, LIST_BORDER, 1.0);
        self.model_portrait(place, at(layout::COSMETICS_MODEL));
        let line = at([15.0, 290.0, 400.0, 16.0]);
        let hat = self.cosmetics.worn_label(CosmeticSlot::Hat);
        let cape = self.cosmetics.worn_label(CosmeticSlot::Cape);
        match (hat, cape) {
            (None, None) => self.label(
                place,
                line,
                "Wearing no hat or cape",
                12.0,
                VALUE,
                FontWeight::Regular,
                TextAlign::Center,
            ),
            (hat, cape) => self.label_fmt(
                place,
                line,
                format_args!(
                    "Wearing  {}  \u{b7}  {}",
                    hat.as_deref().unwrap_or("no hat"),
                    cape.as_deref().unwrap_or("no cape")
                ),
                12.0,
                VALUE,
                FontWeight::Regular,
                TextAlign::Center,
            ),
        }
    }

    /// One entry of the cosmetics window.
    pub(super) fn cosmetics_entry(
        &mut self,
        place: &Placement,
        item: Item,
        canvas: [f32; 4],
        active: bool,
    ) {
        match item {
            Item::Hats => self.cosmetic_list(place, CosmeticSlot::Hat, canvas, active),
            Item::Capes => self.cosmetic_list(place, CosmeticSlot::Cape, canvas, active),
            Item::CosmeticsShow => {
                if active {
                    glow(
                        &mut self.canvas,
                        place.rect(canvas),
                        place.scale,
                        self.classic.art,
                    );
                }
                let visibility = self.cosmetics.visibility();
                self.label_fmt(
                    place,
                    canvas,
                    format_args!("Show Cosmetics: {}", visibility.label()),
                    13.0,
                    if active { FOCUS } else { GOLD },
                    FontWeight::Semibold,
                    TextAlign::Center,
                );
            }
            _ => {}
        }
    }

    /// A list of the slot's installed pieces, the worn one filled.
    fn cosmetic_list(
        &mut self,
        place: &Placement,
        slot: CosmeticSlot,
        canvas: [f32; 4],
        active: bool,
    ) {
        let s = place.scale;
        let (_, base, wheel) = list_of(slot);
        self.fill(place, canvas, LIST_BACK);
        self.border(place, canvas, if active { FOCUS } else { LIST_BORDER }, 1.0);
        self.canvas.scroll_region(wheel, place.rect(canvas));
        let [x, y, w, h] = canvas;
        let rows = (h / ROW).floor() as usize;
        let count = self.cosmetics.count(slot).min(MAX_ROWS);
        if count == 0 {
            let (empty, folder) = match slot {
                CosmeticSlot::Hat => ("No hats installed.", "models/cosmetics/hats in base."),
                CosmeticSlot::Cape => ("No capes installed.", "models/cosmetics/capes in base."),
            };
            let lines: [(&str, f32); 5] = [
                (empty, 12.0),
                ("Install JoF EJK's cosmetics", 11.0),
                ("(GameData/EternalJK), or a pack", 11.0),
                ("with", 11.0),
                (folder, 11.0),
            ];
            for (row, (text, size)) in lines.into_iter().enumerate() {
                self.label(
                    place,
                    [x + 4.0, y + 8.0 + row as f32 * ROW, w - 8.0, ROW],
                    text,
                    size,
                    if row == 0 { GOLD } else { VALUE },
                    FontWeight::Regular,
                    TextAlign::Center,
                );
            }
            return;
        }
        let index = slot.index();
        // Room for JoF's note when some of its catalogue is missing.
        let missing = self
            .cosmetics
            .catalog()
            .is_some_and(|catalog| catalog.has_missing(slot));
        let lines = count + usize::from(missing);
        let cursor = self.cosmetics.cursor[index].min(count - 1);
        let scroll = &mut self.cosmetics.scroll[index];
        if cursor < *scroll {
            *scroll = cursor;
        } else if cursor >= *scroll + rows {
            *scroll = cursor + 1 - rows;
        }
        *scroll = (*scroll).min(lines.saturating_sub(rows));
        let first = *scroll;
        let worn = self.cosmetics.worn_position(slot);
        for row in first..(first + rows).min(lines) {
            let local = (row - first) as f32;
            let cell = [x + 1.0, y + 1.0 + local * ROW, w - 2.0, ROW];
            if row == count {
                self.label(
                    place,
                    cell,
                    "Get more from JoF Launcher or Cloud",
                    10.0,
                    GOLD,
                    FontWeight::Regular,
                    TextAlign::Center,
                );
                continue;
            }
            let token = base + row as u16;
            let hovered = self.canvas.token_hovered(token);
            let is_worn = worn == Some(row);
            if is_worn {
                self.fill(place, cell, WORN);
            }
            if (active && row == cursor) || hovered {
                self.piece(place, ArtPiece::BlendBox2, cell);
            }
            let Some(display) = self
                .cosmetics
                .catalog()
                .and_then(|catalog| catalog.pieces(slot).get(row))
                .map(|piece| piece.display.clone())
            else {
                continue;
            };
            let [cx, cy, cw, ch] = cell;
            let color = if is_worn || hovered || (active && row == cursor) {
                FOCUS
            } else {
                VALUE
            };
            self.label(
                place,
                [cx + 4.0, cy, cw - 44.0, ch],
                &display,
                12.0,
                color,
                FontWeight::Regular,
                TextAlign::Start,
            );
            if is_worn {
                self.label(
                    place,
                    [cx + cw - 42.0, cy, 38.0, ch],
                    "worn",
                    10.0,
                    GOLD,
                    FontWeight::Semibold,
                    TextAlign::End,
                );
            }
            self.canvas.hit_region(token, place.rect(cell));
        }
        if lines > rows {
            let track = place.rect([x + w - 5.0, y + 2.0, 3.0, h - 4.0]);
            self.canvas.scrollbar(wheel, track, first, rows, lines);
        }
        let _ = s;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_tokens_name_their_list() {
        for slot in CosmeticSlot::ALL {
            let (_, base, _) = list_of(slot);
            assert_eq!(row_of(base), Some((slot, 0)));
            assert_eq!(row_of(base + 9), Some((slot, 9)));
        }
        let (_, hat, _) = list_of(CosmeticSlot::Hat);
        let (_, cape, _) = list_of(CosmeticSlot::Cape);
        assert!(hat + MAX_ROWS as u16 <= cape);
        assert_eq!(row_of(hat - 1), None);
    }
}
