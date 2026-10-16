//! Console ownership and persistence for the installed-pack browser.
use super::asset_browser::Action;
use super::*;

impl ViewerConsole {
    pub(crate) fn open_asset_browser(
        &mut self,
        game_data: &std::path::Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let owns_console = !self.open;
        let mut browser = std::mem::take(&mut self.asset_browser);
        let result = browser.open(game_data, self);
        if result.is_ok() {
            self.set_open(false);
            self.set_open(true);
            browser.owns_console = owns_console;
        }
        self.asset_browser = browser;
        result
    }

    fn asset_action(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::Close => {
                self.asset_browser.open = false;
                if self.asset_browser.owns_console {
                    self.set_open(false);
                }
            }
            Action::Changed => {
                let value = self.asset_browser.saved();
                self.set_cvar(crate::assets::search_paths::pack_policy::CVAR, &value);
            }
        }
    }

    pub(super) fn asset_browser_key(&mut self, event: &KeyEvent) -> bool {
        if !self.asset_browser.open {
            return false;
        }
        let action = self.asset_browser.key(event);
        self.asset_action(action);
        true
    }

    pub(super) fn asset_browser_pointer(&mut self, event: sjk_ui::InputEvent) -> bool {
        if !self.asset_browser.open {
            return false;
        }
        let action = self.asset_browser.pointer(event);
        self.asset_action(action);
        true
    }
}
