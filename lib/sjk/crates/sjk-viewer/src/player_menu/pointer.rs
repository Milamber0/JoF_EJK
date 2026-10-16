//! Pointer routing for the player form: hover selects, the wheel moves the
//! selection, tabs switch pages, a click on a cycler steps it toward the
//! half of the value zone that was hit, a click on the Force side picker
//! picks the card under it, a click or drag on a slider sets it from the
//! pointer, a click on a palette picks the chip under it, and the footer's
//! ESC cap goes back.
//!
//! The SJK UI's rows give their token the control's own rectangle, not a
//! row with a value zone at its right: there a ‹ › control turns by the half
//! of it that was hit and a chip row picks the chip drawn under the pointer
//! ([`half_direction`], [`chip_index`]).

use super::grid::{GRID_SCROLL_TOKEN, TILE_BASE};
use super::rows::FORCE_SIDE_ROW;
use super::saber::PALETTE;
use super::*;
use crate::menu_widgets::{BACK_TOKEN, TAB_BASE, cycler_direction, palette_index};
use sjk_ui::{InputEvent, UiEventKind};

impl PlayerMenu {
    pub(crate) fn handle_pointer(
        &mut self,
        event: InputEvent,
        console: &mut ViewerConsole,
    ) -> PlayerMenuResult {
        if self.classic_style {
            return self.classic_pointer(event, console);
        }
        let secondary = matches!(
            event,
            InputEvent::PointerRelease {
                button: sjk_ui::PointerButton::Secondary,
                ..
            }
        );
        let Some(event) = self.canvas.pointer(event) else {
            return PlayerMenuResult::None;
        };
        let Some(token) = event.token else {
            return PlayerMenuResult::None;
        };
        // SJK Force page: a right click on a level removes it.
        if secondary && event.kind == UiEventKind::Click && self.is_sjk() {
            self.sjk_remove_level(token);
            return PlayerMenuResult::None;
        }
        // SJK: pressing anything but the draft's own value field applies it.
        if event.kind == UiEventKind::Press
            && crate::menu_widgets::numeric::value_row(token)
                != self.numeric.as_ref().map(|edit| edit.row)
        {
            self.settle_numeric(console);
        }
        if event.kind == UiEventKind::Activate {
            if let Some(row) = crate::menu_widgets::numeric::value_row(token) {
                if self.numeric.as_ref().is_none_or(|edit| edit.row != row) {
                    self.begin_numeric(row);
                }
                return PlayerMenuResult::None;
            }
            self.numeric = None;
        } else if self.numeric.is_some() {
            return PlayerMenuResult::None;
        }
        if self.is_sjk()
            && let Some(result) = self.outfit_pointer(event.kind, token, event.delta)
        {
            return result;
        }
        if self.is_sjk()
            && !self.name_editing
            && !self.search_editing
            && let Some(result) = self.sjk_pointer(
                event.kind,
                token,
                event.delta.map_or(0.0, |delta| delta.y),
                console,
            )
        {
            return result;
        }
        let count = self.row_count();
        if event.kind == UiEventKind::Wheel {
            let direction = event.delta.map_or(0, |delta| -delta.y.signum() as i32);
            if token == GRID_SCROLL_TOKEN {
                self.scroll_grid(direction);
            } else if direction != 0 && count > 0 {
                self.selected = (self.selected as i32 + direction)
                    .clamp(0, count.saturating_sub(1) as i32)
                    as usize;
            }
            return PlayerMenuResult::None;
        }
        if let Some(tile) = (TILE_BASE..TAB_BASE)
            .contains(&token)
            .then(|| token - TILE_BASE)
        {
            return self.tile_event(event.kind, usize::from(tile), console);
        }
        let row = crate::menu_widgets::numeric::value_row(token).unwrap_or(usize::from(token));
        if matches!(event.kind, UiEventKind::HoverEnter | UiEventKind::Hover) {
            if row < count && !self.name_editing && !self.search_editing {
                self.selected = row;
            }
            return PlayerMenuResult::None;
        }
        if event.kind == UiEventKind::Drag {
            if crate::menu_widgets::numeric::value_row(token).is_some() {
                return PlayerMenuResult::None;
            }
            if let Some(position) = event.position.filter(|_| row < count) {
                self.selected = row;
                self.set_slider_from_pointer(console, token, position.x);
            }
            return PlayerMenuResult::None;
        }
        if event.kind != UiEventKind::Activate {
            return PlayerMenuResult::None;
        }
        if token == BACK_TOKEN {
            return PlayerMenuResult::Back(self.return_target);
        }
        if let Some(page) = (token >= TAB_BASE)
            .then(|| ProfilePage::ALL.get(usize::from(token - TAB_BASE)))
            .flatten()
        {
            self.set_page(*page);
            return PlayerMenuResult::None;
        }
        if row >= count {
            return PlayerMenuResult::None;
        }
        // SJK UI: a click on a row beside its control only chooses the row.
        if self.is_sjk()
            && event
                .position
                .is_some_and(|position| self.sjk_beside_control(token, position))
        {
            if self.name_editing {
                self.name_editing = false;
                self.apply(console);
            }
            self.search_editing = false;
            self.selected = row;
            return PlayerMenuResult::None;
        }
        if self.name_editing {
            // A click elsewhere commits the name like Enter would.
            self.name_editing = false;
            self.apply(console);
        }
        // A click elsewhere ends typing the search, keeping it.
        self.search_editing = false;
        self.selected = row;
        if let Some(position) = event.position {
            if self.set_slider_from_pointer(console, token, position.x) {
                return PlayerMenuResult::None;
            }
        }
        let side_picker = self.page == ProfilePage::Force && row == FORCE_SIDE_ROW;
        let direction = event
            .position
            .zip(self.canvas.rect_for(token))
            .filter(|_| self.selected_is_cycler())
            .map(|(position, rect)| {
                if side_picker || self.is_sjk() {
                    half_direction(rect, position.x)
                } else {
                    cycler_direction(rect, position.x)
                }
            });
        match direction {
            Some(direction) => self.adjust(console, direction),
            None => self.activate(console),
        }
        PlayerMenuResult::None
    }

    /// Set the RGB channel slider under `token` from the pointer's x, or
    /// pick the palette chip there; false when the row is neither.
    fn set_slider_from_pointer(
        &mut self,
        console: &mut ViewerConsole,
        token: u16,
        pointer_x: f32,
    ) -> bool {
        if self.page != ProfilePage::Saber {
            return false;
        }
        let Some(row) = self.saber_rows().get(usize::from(token)).copied() else {
            return false;
        };
        let Some(rect) = self.canvas.rect_for(token) else {
            return false;
        };
        if let Some(channel) = row.channel() {
            let ratio = if self.is_sjk() {
                Self::sjk_slider_ratio(rect, pointer_x)
            } else {
                self.canvas.slider_ratio(rect, pointer_x)
            };
            let value = (ratio * 255.0).round() as u8;
            self.saber.set_channel(row.second(), channel, value);
        } else if row.is_blade() {
            let chip = if self.is_sjk() {
                chip_index(rect, pointer_x, PALETTE.len())
            } else {
                palette_index(rect, pointer_x, PALETTE.len())
            };
            self.saber.select_color(row.second(), PALETTE[chip]);
        } else {
            return false;
        }
        self.saber.apply(console);
        true
    }

    /// A tile of the creator's grid: hovering puts the cursor on it, a click chooses
    /// it ([`Self::creator_tile`]).
    fn tile_event(
        &mut self,
        kind: UiEventKind,
        tile: usize,
        console: &mut ViewerConsole,
    ) -> PlayerMenuResult {
        self.creator_tile(kind, tile, console);
        PlayerMenuResult::None
    }
}

/// Which half of `rect` a pointer at `x` is on, as an `adjust` direction:
/// on the side picker the left card is Light (-1), the right Dark (1); on
/// an SJK UI ‹ › control the left half steps back, the right forward.
fn half_direction(rect: sjk_ui::Rect, x: f32) -> isize {
    if x < rect.x + rect.width * 0.5 { -1 } else { 1 }
}

/// Which of `count` chips across `rect` a pointer at `x` is on: the SJK UI
/// centres each chip in an equal share of its control (`kit::chips`).
fn chip_index(rect: sjk_ui::Rect, x: f32, count: usize) -> usize {
    let ratio = ((x - rect.x) / rect.width.max(1.0)).clamp(0.0, 1.0);
    ((ratio * count as f32) as usize).min(count.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_cards_split_the_row_down_the_middle() {
        let rect = sjk_ui::Rect::new(100.0, 0.0, 400.0, 75.0);
        assert_eq!(half_direction(rect, 120.0), -1);
        assert_eq!(half_direction(rect, 299.0), -1);
        assert_eq!(half_direction(rect, 300.0), 1);
        assert_eq!(half_direction(rect, 480.0), 1);
    }

    /// The chip under the pointer is the one drawn there: each of the seven
    /// centred in its share of the control, as `kit::chips` lays them out.
    /// The value zone of the old form rows (the right 48% of a row) picked a chip
    /// left of the one clicked, and the first for any click on the left half.
    #[test]
    fn a_click_on_a_chip_picks_that_chip() {
        let rect = sjk_ui::Rect::new(1000.0, 0.0, 330.0, 38.0);
        let count = PALETTE.len();
        let share = rect.width / count as f32;
        for chip in 0..count {
            let centre = rect.x + share * (chip as f32 + 0.5);
            assert_eq!(chip_index(rect, centre, count), chip);
            assert_eq!(chip_index(rect, centre - share * 0.45, count), chip);
            assert_eq!(chip_index(rect, centre + share * 0.45, count), chip);
        }
        assert_ne!(palette_index(rect, rect.x + share * 1.5, count), 1);
        assert_eq!(chip_index(rect, rect.x - 5.0, count), 0);
        assert_eq!(chip_index(rect, rect.right() + 5.0, count), count - 1);
    }

    /// A ‹ › control turns by the half the click is on: just right of its
    /// middle steps forward (the value zone's middle, at 76% of the control,
    /// stepped back there).
    #[test]
    fn a_click_on_a_cycler_turns_by_its_half() {
        let rect = sjk_ui::Rect::new(1000.0, 0.0, 330.0, 38.0);
        assert_eq!(half_direction(rect, rect.x + 20.0), -1);
        assert_eq!(half_direction(rect, rect.x + rect.width * 0.55), 1);
        assert_eq!(cycler_direction(rect, rect.x + rect.width * 0.55), -1);
        assert_eq!(half_direction(rect, rect.right() - 20.0), 1);
    }
}
