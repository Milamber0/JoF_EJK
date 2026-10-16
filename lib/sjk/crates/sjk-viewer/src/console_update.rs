//! Console side of the Update page (see `update_panel.rs`): opening it, starting
//! the update checks, and routing keys and pointer events to it while it is open,
//! as for the changelog page.

use super::update_panel::PanelAction;
use super::*;
use sjk_ui::InputEvent;

impl ViewerConsole {
    /// `update`: show the page in front of the console, opening the console if
    /// needed, or close it (and the console, if the page opened it).
    pub(crate) fn toggle_update_panel(&mut self) {
        if self.open && self.update_panel.is_open() {
            self.close_update_panel();
            return;
        }
        self.open_update_panel();
    }

    /// Show the page (the main menu's Update entry). A page opened with nothing
    /// checked yet checks at once.
    pub(crate) fn open_update_panel(&mut self) {
        let owns_console = !self.open;
        if !self.open {
            self.set_open(true);
        }
        self.browser.close();
        self.debug_panel.close();
        self.changelog.close();
        self.identity_panel.close();
        self.config_import.close();
        self.dead_key.settle();
        self.update_panel.open(owns_console);
        if matches!(crate::update::state(), crate::update::State::Idle) {
            self.check_for_updates();
        }
    }

    /// Ask GitHub for a newer release. The installed version is the build's,
    /// or `cl_updateAs` when set, so a local build can try the check.
    pub(crate) fn check_for_updates(&self) {
        let pretend = self.text_cvar("cl_updateAs").unwrap_or_default().trim();
        crate::update::check(if pretend.is_empty() {
            crate::build_info::VERSION
        } else {
            pretend
        });
    }

    /// Once a frame: with `cl_autoUpdate` on, ask GitHub again every
    /// [`crate::update::RECHECK`] while nothing new was found.
    pub(crate) fn recheck_for_updates(&self) {
        if self.bool_cvar("cl_autoUpdate") != Some(true) {
            return;
        }
        let pretend = self.text_cvar("cl_updateAs").unwrap_or_default().trim();
        crate::update::recheck_if_due(
            if pretend.is_empty() {
                crate::build_info::VERSION
            } else {
                pretend
            },
            std::time::Instant::now(),
        );
    }

    /// The start-up check, when `cl_autoUpdate` is on. It also finds out that a
    /// local build has no version, so the Update page has an answer at once.
    pub(crate) fn check_for_updates_at_launch(&self) {
        crate::update::clean_up_old_files();
        if self.bool_cvar("cl_autoUpdate") == Some(true) {
            self.check_for_updates();
        }
    }

    /// Whether the Update page shows (in front of the console).
    pub(crate) fn update_page_open(&self) -> bool {
        self.open && self.update_panel.is_open()
    }

    fn close_update_panel(&mut self) {
        if self.update_panel.close() {
            self.set_open(false);
        }
    }

    fn update_panel_action(&mut self, action: PanelAction) {
        match action {
            PanelAction::None => {}
            PanelAction::Close => self.close_update_panel(),
            PanelAction::Check => self.check_for_updates(),
            PanelAction::Quit => self.pending_quit = true,
        }
    }

    /// Give a pressed key to the open page; false when the page is closed.
    pub(super) fn update_panel_key(&mut self, event: &KeyEvent) -> bool {
        if !self.update_panel.is_open() {
            return false;
        }
        let action = self.update_panel.handle_key(event);
        self.update_panel_action(action);
        true
    }

    /// Give a pointer event to the open page; false when the page is closed.
    pub(super) fn update_panel_pointer(&mut self, event: InputEvent) -> bool {
        if !self.update_panel.is_open() {
            return false;
        }
        let action = self.update_panel.handle_pointer(event);
        self.update_panel_action(action);
        true
    }

    /// The page's draw list while it is shown.
    pub(super) fn update_panel_draw_list(&self) -> Option<&sjk_ui::DrawList> {
        (self.open && self.update_panel.is_open()).then(|| self.update_panel.draw_list())
    }
}
