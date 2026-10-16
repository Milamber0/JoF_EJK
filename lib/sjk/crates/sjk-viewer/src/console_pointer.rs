//! Pointer-only console scrolling; command behavior remains keyboard driven. The
//! command and cvar browser takes every pointer event while it is open.

use super::ViewerConsole;
use sjk_ui::InputEvent;

impl ViewerConsole {
    pub(crate) fn handle_pointer(&mut self, event: InputEvent) {
        if self.asset_browser_pointer(event)
            || self.model_viewer_pointer(event)
            || self.config_import_pointer(event)
            || self.credits_pointer(event)
            || self.changelog_pointer(event)
            || self.update_panel_pointer(event)
            || self.identity_panel_pointer(event)
            || self.profile_panel_pointer(event)
            || self.staff_panel_pointer(event)
            || self.collection_panel_pointer(event)
            || self.holocrons_panel_pointer(event)
            || self.sjk_chat_panel_pointer(event)
            || self.debug_panel_pointer(event)
        {
            return;
        }
        if self.browser.is_open() {
            let action = self.browser.handle_pointer(event);
            self.browser_action(action);
            return;
        }
        // The SJK UI's console opens the command browser from its F3 hint.
        if let InputEvent::PointerPress {
            position,
            button: sjk_ui::PointerButton::Primary,
        } = event
            && self.classic.sjk.opens_browser(position)
        {
            self.selection.clear();
            self.browser.open(&self.shell);
            return;
        }
        // `Console_Key`: the wheel pages like Page Up/Down (Ctrl five times
        // as far), and with Shift walks the history.
        if let InputEvent::PointerWheel { delta, .. } = event {
            match (self.shift, delta.y > 0.0, delta.y < 0.0) {
                (true, true, _) => self.navigate_history(-1),
                (true, _, true) => self.navigate_history(1),
                (false, true, _) => {
                    self.scroll_rows(super::classic::page_rows(self.control) as isize);
                }
                (false, _, true) => {
                    self.scroll_rows(-(super::classic::page_rows(self.control) as isize));
                }
                _ => {}
            }
        } else {
            self.selection.pointer(event, self.shift);
        }
    }
}
