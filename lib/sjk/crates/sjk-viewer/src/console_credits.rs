//! Console side of the credits page (see `credits.rs`): opening it, and routing
//! keys and pointer events to it while it is open, as for the changelog page.

use super::*;
use sjk_ui::InputEvent;

impl ViewerConsole {
    /// `credits`: show the page in front of the console, opening the console if
    /// needed, or close it (and the console, if the page opened it).
    pub(crate) fn toggle_credits(&mut self) {
        if self.open && self.credits.is_open() {
            self.close_credits();
            return;
        }
        self.open_credits();
    }

    /// Show the page (the main menu's and the in-game SJK menu's Credits).
    pub(crate) fn open_credits(&mut self) {
        let owns_console = !self.open;
        if !self.open {
            self.set_open(true);
        }
        self.browser.close();
        self.debug_panel.close();
        self.changelog.close();
        self.identity_panel.close();
        self.profile_panel.close();
        self.collection_panel.close();
        self.holocrons_panel.close();
        self.staff_panel.close();
        self.sjk_chat_panel.close();
        self.config_import.close();
        self.dead_key.settle();
        self.credits.open(owns_console);
    }

    fn close_credits(&mut self) {
        if self.credits.close() {
            self.set_open(false);
        }
    }

    /// The page, for snapshots.
    #[cfg(test)]
    pub(crate) fn credits_mut(&mut self) -> &mut super::credits::Panel {
        &mut self.credits
    }

    /// Give a pressed key to the open page; false when the page is closed.
    pub(super) fn credits_key(&mut self, event: &KeyEvent) -> bool {
        if !self.credits.is_open() {
            return false;
        }
        if self.credits.handle_key(event, self.shift) {
            self.close_credits();
        }
        true
    }

    /// Give a pointer event to the open page; false when the page is closed.
    pub(super) fn credits_pointer(&mut self, event: InputEvent) -> bool {
        if !self.credits.is_open() {
            return false;
        }
        if self.credits.handle_pointer(event) {
            self.close_credits();
        }
        true
    }

    /// Draw the page in place of the console; false when it is not shown.
    pub(super) fn append_credits(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) -> bool {
        if !(self.open && self.credits.is_open()) {
            return false;
        }
        self.credits.append(vertices, font, viewport);
        true
    }

    /// The page's draw list while it is shown.
    pub(super) fn credits_draw_list(&self) -> Option<&sjk_ui::DrawList> {
        (self.open && self.credits.is_open()).then(|| self.credits.draw_list())
    }
}
