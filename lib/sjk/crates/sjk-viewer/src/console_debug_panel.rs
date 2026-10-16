//! Console side of the personal `debug_panel` test list (see `debug_panel.rs`): the
//! toggle, and routing keys and pointer events to the panel while it is open. The
//! shared console files only call into here, to keep merges with upstream small.

use super::debug_panel::{self, PanelAction};
use super::*;
use sjk_ui::InputEvent;

impl ViewerConsole {
    /// `debug_panel`: show the test list in front of the console, opening the console
    /// if needed, or close it (and the console, if the panel opened it).
    pub(crate) fn toggle_debug_panel(&mut self) {
        if self.open && self.debug_panel.is_open() {
            self.close_debug_panel();
            return;
        }
        let owns_console = !self.open;
        if !self.open {
            self.set_open(true);
        }
        self.browser.close();
        self.changelog.close();
        self.config_import.close();
        self.credits.close();
        self.dead_key.settle();
        self.debug_panel.open(owns_console);
    }

    fn close_debug_panel(&mut self) {
        if self.debug_panel.close() {
            self.set_open(false);
        }
    }

    /// Whether the key event's binds run `debug_panel`.
    pub(super) fn bound_to_debug_panel(&self, event: &KeyEvent) -> bool {
        let winit::keyboard::PhysicalKey::Code(key) = event.physical_key else {
            return false;
        };
        let Some(key_name) = crate::input::keys::name(crate::input::keys::Source::Key(key)) else {
            return false;
        };
        self.shell
            .binds
            .commands_for_event(key_name, true)
            .is_ok_and(|commands| {
                commands
                    .iter()
                    .any(|command| command.eq_ignore_ascii_case(debug_panel::COMMAND))
            })
    }

    /// Give a pressed key to the open panel; false when the panel is closed. A key
    /// bound to `debug_panel` closes it, so the same key toggles it.
    pub(super) fn debug_panel_key(&mut self, event: &KeyEvent) -> bool {
        if !self.debug_panel.is_open() {
            return false;
        }
        if !event.repeat && self.bound_to_debug_panel(event) {
            self.close_debug_panel();
        } else if self.debug_panel.handle_key(event, self.shift) == PanelAction::Close {
            self.close_debug_panel();
        }
        true
    }

    /// Give a pointer event to the open panel; false when the panel is closed.
    pub(super) fn debug_panel_pointer(&mut self, event: InputEvent) -> bool {
        if !self.debug_panel.is_open() {
            return false;
        }
        if self.debug_panel.handle_pointer(event) == PanelAction::Close {
            self.close_debug_panel();
        }
        true
    }

    /// Draw the panel in place of the console, in the SJK UI's look, its text to
    /// `target` (`console_sjk_pages.rs` routes it).
    pub(super) fn append_debug_panel(
        &mut self,
        target: crate::menu::sjk::TextTarget<'_>,
        viewport: [f32; 2],
    ) {
        self.debug_panel.append_sjk(target, viewport);
    }

    /// Open the test list on its first entry with notes, for a world shot.
    #[cfg(test)]
    pub(crate) fn debug_panel_for_shot(&mut self) {
        if !self.debug_panel.is_open() {
            self.toggle_debug_panel();
        }
        self.debug_panel.select_noted_for_shot();
    }

    /// The panel's draw list while it is shown.
    pub(super) fn debug_panel_draw_list(&self) -> Option<&sjk_ui::DrawList> {
        (self.open && self.debug_panel.is_open()).then(|| self.debug_panel.draw_list())
    }
}
