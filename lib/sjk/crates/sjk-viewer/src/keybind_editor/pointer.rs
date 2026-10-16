//! Pointer interaction for the visual key-binding form.

use super::*;
use sjk_ui::{InputEvent, UiEventKind};

impl KeybindEditor {
    pub(crate) fn handle_pointer(
        &mut self,
        event: InputEvent,
        console: &mut ViewerConsole,
    ) -> EditorResult {
        let Some(event) = self.ui.pointer(event) else {
            return EditorResult::None;
        };
        let Some(token) = event.token else {
            return EditorResult::None;
        };
        if let Some(slot) = crate::menu::classic::panel::chrome_slot(token) {
            return match event.kind {
                UiEventKind::Activate if self.classic.is_some() => EditorResult::Classic(slot),
                _ => EditorResult::None,
            };
        }
        if token == crate::menu::classic::panel::SEARCH_TOKEN {
            if event.kind == UiEventKind::Activate && !self.capture {
                self.begin_search();
            }
            return EditorResult::None;
        }
        if event.kind == UiEventKind::Activate {
            self.end_search();
        }
        let row = usize::from(token);
        if self.shows(row)
            && !self.capture
            && matches!(event.kind, UiEventKind::HoverEnter | UiEventKind::Hover)
        {
            self.selected = row;
        }
        if event.kind == UiEventKind::Wheel {
            let direction = event.delta.map_or(0, |delta| -delta.y.signum() as i32);
            self.scroll_by(direction * WHEEL_ROWS as i32);
            return EditorResult::None;
        }
        if event.kind == UiEventKind::Drag && token == SCROLLBAR_TOKEN {
            if let (Some(point), Some(track)) = (event.position, self.ui.rect_for(SCROLLBAR_TOKEN))
            {
                self.scroll_to_ratio((point.y - track.y) / track.height);
            }
            return EditorResult::None;
        }
        if event.kind != UiEventKind::Activate {
            return EditorResult::None;
        }
        match token {
            BACK_TOKEN => EditorResult::Back,
            RESET_TOKEN => {
                console.reset_default_binds();
                self.refresh(console);
                EditorResult::None
            }
            UNBIND_TOKEN => {
                self.clear_selected_slot(console);
                EditorResult::None
            }
            TAB_BASE.. if usize::from(token - TAB_BASE) < CATEGORIES.len() => {
                self.set_tab(usize::from(token - TAB_BASE));
                EditorResult::None
            }
            _ if self.shows(row) => {
                self.selected = row;
                self.binding_slot = 0;
                self.begin_capture();
                EditorResult::None
            }
            _ if token >= SECONDARY_BASE && self.shows(usize::from(token - SECONDARY_BASE)) => {
                self.selected = usize::from(token - SECONDARY_BASE);
                self.binding_slot = 1;
                self.begin_capture();
                EditorResult::None
            }
            _ => EditorResult::None,
        }
    }
}
