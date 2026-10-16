//! The First setup screen: Settings' FIRST SETUP tab (a pop-up in the SJK UI),
//! offered at every start until its "Don't show at start" row is ticked, and
//! opened by the `firstsetup` command;
//! `firstsetup import` opens the Import page (`config_import.rs`). Its first row
//! picks the graphics quality, its second the menu style. The command's old name `quicksetup` is gone, so that `quit`
//! is the only command a `q` completes to.

use super::classic::layout::{Entry, Page};
use super::classic::panel::Frame;
use super::*;
use crate::settings::quick::HIDE_CVAR;

/// Console command that opens the screen.
pub(crate) const COMMAND: &str = "firstsetup";
/// Help text for completion and `cmdlist`.
pub(crate) const HELP: &str = "Open First setup; firstsetup import <file.cfg> brings name, model, FOV and keys from another client";
/// The command's argument that opens the Import page instead.
pub(crate) const IMPORT: &str = "import";

impl ClientMenu {
    /// Open the First setup screen, returning to `target` when it closes: the
    /// SJK UI's pop-up over its main page, the classic Setup panel's FIRST
    /// SETUP group under the classic style (and the SJK UI in a game), the
    /// SJK UI's Settings on First setup if that panel cannot open.
    pub(crate) fn open_first_setup(&mut self, console: &ViewerConsole, target: ReturnTarget) {
        if self.menu_style == MenuStyle::Sjk && target == ReturnTarget::MainMenu {
            self.open_sjk_first_setup(console, target);
            return;
        }
        let frame = match target {
            ReturnTarget::MainMenu => Frame::Main,
            ReturnTarget::InGame => Frame::InGame,
        };
        if self.open_classic_panel(console, Page::Setup, Entry::FirstSetup, frame, target) {
            return;
        }
        self.open_sjk_settings(console, super::sjk::settings::FIRST_SETUP, target);
    }

    /// The first time the main menu is up in a run, once the menu style is known,
    /// open the screen over it, unless the player ticked "Don't show at start"
    /// (`ui_hideFirstSetup`). Escape leaves it for this run.
    pub(crate) fn offer_first_setup(&mut self, console: &mut ViewerConsole) -> bool {
        if self.first_setup_offered
            || console.bool_cvar(HIDE_CVAR) == Some(true)
            || *self.state.phase() != ClientPhase::MainMenu
        {
            return false;
        }
        self.first_setup_offered = true;
        self.open_first_setup(console, ReturnTarget::MainMenu);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn console() -> (tempfile::TempDir, ViewerConsole) {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        (directory, console)
    }

    #[test]
    fn every_start_offers_the_screen_once_until_it_is_hidden() {
        let (_directory, mut console) = console();
        let mut menu = ClientMenu::new(true, String::new());
        assert_eq!(console.bool_cvar(HIDE_CVAR), Some(false));
        assert!(menu.offer_first_setup(&mut console));
        assert_eq!(*menu.state.phase(), ClientPhase::Settings);
        // Not again in the same run.
        menu.state.main_menu();
        assert!(!menu.offer_first_setup(&mut console));
        // The next start offers it again, until the player hides it.
        let mut next = ClientMenu::new(true, String::new());
        assert!(next.offer_first_setup(&mut console));
        console.set_cvar(HIDE_CVAR, "1");
        let mut hidden = ClientMenu::new(true, String::new());
        assert!(!hidden.offer_first_setup(&mut console));
        assert_eq!(*hidden.state.phase(), ClientPhase::MainMenu);
    }

    #[test]
    fn the_classic_style_opens_the_setup_panel_group() {
        let (_directory, console) = console();
        let mut menu = ClientMenu::new(true, String::new());
        menu.menu_style = MenuStyle::Classic;
        menu.open_first_setup(&console, ReturnTarget::MainMenu);
        assert_eq!(*menu.state.phase(), ClientPhase::Settings);
        let panel = menu.classic_panel.expect("a classic panel");
        assert_eq!((panel.page, panel.entry), (Page::Setup, Entry::FirstSetup));
        assert_eq!(panel.frame, Frame::Main);
        // In a game it is the in-game frame.
        menu.open_first_setup(&console, ReturnTarget::InGame);
        assert_eq!(
            menu.classic_panel.map(|panel| panel.frame),
            Some(Frame::InGame)
        );
    }

    #[test]
    fn the_sjk_ui_opens_its_popup_on_first_setup_and_the_panel_in_a_game() {
        let (_directory, console) = console();
        let mut menu = ClientMenu::new(true, String::new());
        // The SJK UI is the default style.
        assert_eq!(menu.menu_style, MenuStyle::Sjk);
        menu.open_first_setup(&console, ReturnTarget::MainMenu);
        assert!(menu.sjk_settings_on_show() && menu.settings.popup());
        assert_eq!(
            menu.sjk_settings.category(),
            super::super::sjk::settings::FIRST_SETUP
        );
        // The in-game menu still opens its classic pop-up.
        menu.open_first_setup(&console, ReturnTarget::InGame);
        assert!(!menu.sjk_settings_on_show());
        assert_eq!(
            menu.classic_panel.map(|panel| panel.frame),
            Some(Frame::InGame)
        );
    }

    /// Picking another style on the screen's Menu style row keeps First setup
    /// on show in the new style.
    #[test]
    fn the_menu_style_is_picked_on_the_screen_and_the_screen_stays() {
        let (_directory, console) = console();
        let mut menu = ClientMenu::new(true, String::new());
        menu.open_first_setup(&console, ReturnTarget::MainMenu);
        assert!(menu.sjk_settings_on_show());
        // SJK UI to classic: the classic Setup page's First setup panel.
        menu.set_menu_style(MenuStyle::Classic, &console);
        let panel = menu.classic_panel.expect("a classic panel");
        assert_eq!((panel.page, panel.entry), (Page::Setup, Entry::FirstSetup));
        assert_eq!(*menu.state.phase(), ClientPhase::Settings);
    }

    /// All settings (or Tab) leaves the pop-up for the whole screen on First
    /// setup; Done (or Escape) goes back to the main page.
    #[test]
    fn all_settings_opens_the_screen_on_first_setup_and_done_closes() {
        use crate::settings::SettingsResult;
        let (_directory, mut console) = console();
        let mut menu = ClientMenu::new(true, String::new());
        menu.open_first_setup(&console, ReturnTarget::MainMenu);
        let action = menu.settings_result(SettingsResult::AllSettings, &mut console);
        assert_eq!(action, MenuAction::None);
        assert!(menu.sjk_settings_on_show() && !menu.settings.popup());
        assert_eq!(
            menu.sjk_settings.category(),
            super::super::sjk::settings::FIRST_SETUP
        );
        menu.open_first_setup(&console, ReturnTarget::MainMenu);
        menu.settings_result(SettingsResult::Back, &mut console);
        assert_eq!(*menu.state.phase(), ClientPhase::MainMenu);
        assert!(!menu.sjk_settings_on_show());
    }

    /// Picking the SJK UI on First setup's Menu style row, from the classic
    /// panel, goes on as the pop-up.
    #[test]
    fn first_setup_picked_to_the_sjk_ui_goes_on_as_its_popup() {
        let (_directory, console) = console();
        let mut menu = ClientMenu::new(true, String::new());
        menu.menu_style = MenuStyle::Classic;
        menu.open_first_setup(&console, ReturnTarget::MainMenu);
        assert!(!menu.settings.popup());
        menu.set_menu_style(MenuStyle::Sjk, &console);
        assert!(menu.sjk_settings_on_show() && menu.settings.popup());
        assert!(menu.classic_panel.is_none());
        // Another screen switched to the SJK UI stays itself.
        let mut menu = ClientMenu::new(true, String::new());
        menu.menu_style = MenuStyle::Classic;
        menu.set_menu_style(MenuStyle::Sjk, &console);
        assert_eq!(*menu.state.phase(), ClientPhase::MainMenu);
    }
}
