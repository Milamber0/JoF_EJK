//! Console side of the Identity page (see `identity_panel.rs`): opening it, carrying out what
//! it asks (the switch, saving the profile, copying the key id) and routing keys and pointer
//! events to it while it is open, as for the Update page.

use super::identity_panel::{Inputs, PanelAction};
use super::*;
use sjk_ui::InputEvent;

impl ViewerConsole {
    /// `identity`: show the page in front of the console, opening the console if
    /// needed, or close it (and the console, if the page opened it).
    pub(crate) fn toggle_identity_panel(&mut self) {
        if self.open && self.identity_panel.is_open() {
            self.close_identity_panel();
            return;
        }
        self.open_identity_panel();
    }

    /// Show the page (Settings' SJK identity key row, the classic SJK menu's Identity).
    pub(crate) fn open_identity_panel(&mut self) {
        let owns_console = !self.open;
        self.open_identity_panel_owned(owns_console);
    }

    /// Show the page; closing it closes the console too when `owns_console`.
    pub(crate) fn open_identity_panel_owned(&mut self, owns_console: bool) {
        if !self.open {
            self.set_open(true);
        }
        self.browser.close();
        self.debug_panel.close();
        self.changelog.close();
        self.update_panel.close();
        self.profile_panel.close();
        self.collection_panel.close();
        self.holocrons_panel.close();
        self.staff_panel.close();
        self.sjk_chat_panel.close();
        self.config_import.close();
        self.credits.close();
        self.dead_key.settle();
        self.identity_panel.open(owns_console);
    }

    fn close_identity_panel(&mut self) {
        if self.identity_panel.close() {
            self.set_open(false);
        }
    }

    /// Do what the page asked.
    fn identity_panel_action(&mut self, action: PanelAction) {
        match action {
            PanelAction::None => {}
            PanelAction::Close => self.close_identity_panel(),
            PanelAction::SetEnabled(enabled) => {
                self.set_cvar("cl_identity", if enabled { "1" } else { "0" });
            }
            PanelAction::Save { bio } => {
                if !crate::player_identity::set_bio(bio) {
                    self.identity_panel.set_message(
                        "Not ready yet: the identity is starting, try again in a moment.",
                    );
                }
            }
            PanelAction::UseDefaultHub => {
                self.set_cvar("cl_hubUrl", crate::player_identity::DEFAULT_HUB_URL);
            }
            // The page shows or hides the key's id and file itself.
            PanelAction::Reveal => {}
            PanelAction::CopyKeyId => {
                if let Some(snapshot) = crate::player_identity::snapshot()
                    && crate::console::clipboard::copy(&snapshot.local_key_id)
                {
                    self.identity_panel.note_copied();
                }
            }
        }
    }

    /// Give a pressed key to the open page; false when the page is closed.
    pub(super) fn identity_panel_key(&mut self, event: &KeyEvent) -> bool {
        if !self.identity_panel.is_open() {
            return false;
        }
        let action = self
            .identity_panel
            .handle_key(event, self.shift, self.control);
        self.identity_panel_action(action);
        true
    }

    /// Give a pointer event to the open page; false when the page is closed.
    pub(super) fn identity_panel_pointer(&mut self, event: InputEvent) -> bool {
        if !self.identity_panel.is_open() {
            return false;
        }
        let action = self.identity_panel.handle_pointer(event);
        self.identity_panel_action(action);
        true
    }

    /// Draw the page in place of the console; false when it is not shown.
    pub(super) fn append_identity_panel(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) -> bool {
        if !(self.open && self.identity_panel.is_open()) {
            return false;
        }
        // Copied out so the page can borrow itself mutably while it draws.
        let snapshot = crate::player_identity::snapshot();
        let key_error = crate::player_identity::key_error();
        let hub_url = self.text_cvar("cl_hubUrl").unwrap_or_default().to_owned();
        let key_file = self
            .config_directory()
            .join("identity.key")
            .display()
            .to_string();
        let inputs = Inputs {
            enabled: self.bool_cvar("cl_identity") == Some(true),
            hub_url: &hub_url,
            key_error: key_error.as_deref(),
            snapshot: snapshot.as_ref(),
            key_file: &key_file,
        };
        self.identity_panel
            .append(&inputs, vertices, font, viewport);
        true
    }

    /// The page's draw list while it is shown.
    pub(super) fn identity_panel_draw_list(&self) -> Option<&sjk_ui::DrawList> {
        (self.open && self.identity_panel.is_open()).then(|| self.identity_panel.draw_list())
    }
}
