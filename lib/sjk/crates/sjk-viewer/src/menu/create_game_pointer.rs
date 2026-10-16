//! Pointer input on the Create game screen: hover selects a row, a click on
//! a cycler steps it the way its half points, a click on Start starts, the
//! wheel moves the selection and the top bar's Esc cap goes back.

use super::create_game::{BACK_TOKEN, CreateGameMenu, CreateGameResult, ROWS, Row};
use super::map_picker::PickerResult;
use crate::console::ViewerConsole;
use sjk_ui::{InputEvent, UiEventKind};

impl CreateGameMenu {
    /// Handle one pointer event on the screen.
    pub(crate) fn pointer(
        &mut self,
        event: InputEvent,
        console: &mut ViewerConsole,
    ) -> CreateGameResult {
        let Some(event) = self.ui.pointer(event) else {
            return CreateGameResult::None;
        };
        let Some(token) = event.token else {
            return CreateGameResult::None;
        };
        if self.picker.is_open() {
            return self.picker_pointer(
                event.kind,
                token,
                event.delta.map(|delta| delta.y),
                console,
            );
        }
        let row = usize::from(token);
        match event.kind {
            UiEventKind::Wheel => {
                let direction = event.delta.map_or(0, |delta| -delta.y.signum() as isize);
                let target = (self.selected as isize + direction).clamp(0, ROWS.len() as isize - 1);
                self.selected = target as usize;
                CreateGameResult::None
            }
            UiEventKind::HoverEnter | UiEventKind::Hover if row < ROWS.len() => {
                self.selected = row;
                CreateGameResult::None
            }
            UiEventKind::Activate if token == BACK_TOKEN => CreateGameResult::Back,
            UiEventKind::Activate if row < ROWS.len() => {
                self.selected = row;
                let rect = self.ui.rect_for(token);
                match (ROWS[row], rect, event.position) {
                    (Row::Start | Row::Hostname | Row::Lan | Row::Map, ..) => {
                        self.activate(console)
                    }
                    (_, Some(rect), Some(position)) => {
                        self.adjust(cycler_direction(rect, position.x), console);
                        CreateGameResult::None
                    }
                    _ => self.activate(console),
                }
            }
            _ => CreateGameResult::None,
        }
    }

    /// Pointer on the open map list: hover highlights, a click picks, the
    /// wheel scrolls and the top bar's Esc cap closes the list.
    fn picker_pointer(
        &mut self,
        kind: UiEventKind,
        token: u16,
        wheel: Option<f32>,
        console: &mut ViewerConsole,
    ) -> CreateGameResult {
        let slot = usize::from(token);
        let on_row = slot < self.picker.page();
        match kind {
            UiEventKind::Wheel => self
                .picker
                .scroll(wheel.map_or(0, |y| -y.signum() as isize)),
            UiEventKind::HoverEnter | UiEventKind::Hover if on_row => {
                self.picker.hover(self.picker.first() + slot)
            }
            UiEventKind::Activate if token == BACK_TOKEN => self.picker.close(),
            UiEventKind::Activate if on_row => {
                self.picker.hover(self.picker.first() + slot);
                if let PickerResult::Close(picked) = self.picker.pick(&self.catalogue) {
                    self.map_picked(picked, console);
                }
            }
            _ => {}
        }
        CreateGameResult::None
    }
}

/// Which way a click at `x` turns a cycler over `rect`: its left half steps
/// back, its right half on, as its carets point.
fn cycler_direction(rect: sjk_ui::Rect, x: f32) -> isize {
    if x < rect.x + rect.width * 0.5 { -1 } else { 1 }
}
