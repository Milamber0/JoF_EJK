//! SJK's pages (What's new, Update, Identity, Credits, Import) in the SJK UI's look
//! (and the pages drawn in it in every style: Profile, Staff, Collection, Holocrons,
//! SJK chat and the debug panel's test list):
//! with `ui_menuStyle sjk` they are drawn by their SJK views, in the UI's
//! families ([`crate::menu::sjk::TextTarget`]), over the map. Opening, closing
//! and their keys and pointer stay the console's, as for the other looks.

use super::*;
use crate::menu::sjk::TextTarget;

impl ViewerConsole {
    /// Draw the pages in the SJK UI's look (`sjk`), or in the others.
    pub(crate) fn set_sjk_pages(&mut self, sjk: bool) {
        self.changelog.set_sjk(sjk);
        self.identity_panel.set_sjk(sjk);
        self.credits.set_sjk(sjk);
    }

    /// Whether the page drawn in place of the console is one of them in the
    /// SJK UI's look (the import page, which has only that look, comes first when
    /// open, then credits), or the command browser in it.
    pub(crate) fn sjk_page_open(&self) -> bool {
        if !self.open {
            return false;
        }
        if self.asset_browser.open || self.model_viewer.is_open() {
            return true;
        }
        // The Import and Update pages have the SJK UI's look in every menu style.
        if self.config_import.is_open() {
            return true;
        }
        if self.credits.is_open() {
            return self.credits.is_sjk();
        }
        // The Profile page has the SJK UI's look in every menu style.
        if self.profile_panel.is_open()
            || self.staff_panel.is_open()
            || self.collection_panel.is_open()
            || self.holocrons_panel.is_open()
            || self.sjk_chat_panel.is_open()
        {
            return true;
        }
        if !self.changelog.is_open()
            && !self.update_panel.is_open()
            && !self.identity_panel.is_open()
        {
            // The test list has the SJK UI's look in every menu style and draws
            // over the browser when both are open.
            return self.debug_panel.is_open() || (self.browser.is_open() && self.browser.is_sjk());
        }
        if self.changelog.is_open() {
            return self.changelog.is_sjk();
        }
        if self.update_panel.is_open() {
            return true;
        }
        self.identity_panel.is_open() && self.identity_panel.is_sjk()
    }

    /// Draw the open page ([`Self::sjk_page_open`]) with its text to `target`.
    pub(crate) fn append_sjk_page(&mut self, target: TextTarget<'_>, viewport: [f32; 2]) {
        if self.asset_browser.open {
            self.asset_browser.append(target, viewport);
        } else if self.model_viewer.is_open() {
            self.append_model_viewer(target, viewport);
        } else if self.config_import.is_open() {
            self.sync_config_browse();
            self.config_import.append_sjk(target, viewport);
        } else if self.credits.is_open() {
            self.credits.append_sjk(target, viewport);
        } else if self.profile_panel.is_open() {
            self.append_profile_panel(target, viewport);
        } else if self.staff_panel.is_open() {
            self.append_staff_panel(target, viewport);
        } else if self.collection_panel.is_open() {
            self.append_collection_panel(target, viewport);
        } else if self.holocrons_panel.is_open() {
            self.append_holocrons_panel(target, viewport);
        } else if self.sjk_chat_panel.is_open() {
            self.append_sjk_chat_panel(target, viewport);
        } else if self.changelog.is_open() {
            self.changelog.append_sjk(target, viewport);
        } else if self.update_panel.is_open() {
            self.update_panel.append_sjk(target, viewport);
        } else if self.identity_panel.is_open() {
            // Copied out so the page can borrow itself mutably while it draws.
            let snapshot = crate::player_identity::snapshot();
            let key_error = crate::player_identity::key_error();
            let hub_url = self.text_cvar("cl_hubUrl").unwrap_or_default().to_owned();
            let key_file = self
                .config_directory()
                .join("identity.key")
                .display()
                .to_string();
            let inputs = identity_panel::Inputs {
                enabled: self.bool_cvar("cl_identity") == Some(true),
                hub_url: &hub_url,
                key_error: key_error.as_deref(),
                snapshot: snapshot.as_ref(),
                key_file: &key_file,
            };
            self.identity_panel.append_sjk(&inputs, target, viewport);
        } else if self.debug_panel.is_open() {
            self.append_debug_panel(target, viewport);
        } else if self.browser.is_open() {
            self.browser.append_sjk(target, viewport);
        }
    }
}
