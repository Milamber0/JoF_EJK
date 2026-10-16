//! Pointer interaction for the debug panel: a click selects a row, a click on a tick
//! box (in a row or on the detail pane) ticks or unticks it, the wheel scrolls the
//! list wherever the pointer is, and the scrollbar drags. Hover only highlights, so
//! moving to the detail pane never changes the selection on the way.

use super::{Panel, PanelAction, Step, TABS, WHEEL_ROWS};
use sjk_ui::{InputEvent, UiEventKind};

/// The way back (the top bar's Esc cap).
pub(super) const BACK_TOKEN: u16 = 900;
/// Tab `i` answers to `TAB_BASE + i`.
pub(super) const TAB_BASE: u16 = 500;
/// Row `slot` on screen answers to `ROW_BASE + slot`.
pub(super) const ROW_BASE: u16 = 1_000;
/// The tick box of row `slot` answers to `TICK_BASE + slot`.
pub(super) const TICK_BASE: u16 = 1_100;
/// Most rows drawn at once, so row and tick tokens never overlap.
pub(super) const ROW_LIMIT: usize = (TICK_BASE - ROW_BASE) as usize;
/// Key hint that ticks or unticks the selected entry.
pub(super) const TOGGLE_TOKEN: u16 = 904;
/// Tick box on the detail pane.
pub(super) const DETAIL_TICK_TOKEN: u16 = 905;
/// Wheel target over the detail pane.
pub(super) const PANE_WHEEL_TOKEN: u16 = 906;
/// Draggable thumb beside the rows.
pub(super) const SCROLLBAR_TOKEN: u16 = 910;

/// Row and tick-box tokens of screen row `slot`.
pub(super) fn list_tokens(slot: usize) -> (u16, u16) {
    let slot = slot.min(ROW_LIMIT - 1) as u16;
    (ROW_BASE + slot, TICK_BASE + slot)
}

impl Panel {
    pub(crate) fn handle_pointer(&mut self, event: InputEvent) -> PanelAction {
        let Some(event) = self.ui.pointer(event) else {
            return PanelAction::None;
        };
        if event.kind == UiEventKind::Wheel {
            let direction = event.delta.map_or(0, |delta| -delta.y.signum() as isize);
            self.scroll_by(direction * WHEEL_ROWS);
            return PanelAction::None;
        }
        let Some(token) = event.token else {
            return PanelAction::None;
        };
        if event.kind == UiEventKind::Drag && token == SCROLLBAR_TOKEN {
            if let (Some(point), Some(track)) = (event.position, self.ui.rect_for(SCROLLBAR_TOKEN))
            {
                self.scroll_to_ratio((point.y - track.y) / track.height);
            }
            return PanelAction::None;
        }
        if event.kind != UiEventKind::Activate {
            return PanelAction::None;
        }
        // Position in `visible` of the row a row or tick-box token stands for.
        let row = |base: u16| {
            (token >= base)
                .then(|| self.first + usize::from(token - base))
                .filter(|&row| row < self.visible.len())
        };
        match token {
            BACK_TOKEN => PanelAction::Close,
            TOGGLE_TOKEN | DETAIL_TICK_TOKEN => self.apply(Step::Toggle),
            TICK_BASE.. => {
                if let Some(row) = row(TICK_BASE) {
                    self.selected = row;
                    self.apply(Step::Toggle);
                }
                PanelAction::None
            }
            ROW_BASE.. => {
                if let Some(row) = row(ROW_BASE) {
                    self.selected = row;
                }
                PanelAction::None
            }
            TAB_BASE.. if usize::from(token - TAB_BASE) < TABS.len() => {
                self.set_tab(usize::from(token - TAB_BASE));
                PanelAction::None
            }
            _ => PanelAction::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_and_tick_tokens_never_meet_the_fixed_ones() {
        let (first_row, first_tick) = list_tokens(0);
        let (last_row, last_tick) = list_tokens(usize::MAX);
        assert_eq!((first_row, first_tick), (ROW_BASE, TICK_BASE));
        assert!(last_row < TICK_BASE);
        assert_eq!(last_tick, TICK_BASE + ROW_LIMIT as u16 - 1);
        for fixed in [
            TOGGLE_TOKEN,
            DETAIL_TICK_TOKEN,
            PANE_WHEEL_TOKEN,
            SCROLLBAR_TOKEN,
        ] {
            assert!(fixed < ROW_BASE && fixed != BACK_TOKEN);
            assert!(fixed >= TAB_BASE + TABS.len() as u16);
        }
    }
}
