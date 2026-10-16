//! Screens the main menu opens, shared by both menu styles so the SJK UI
//! and the classic layout lead to the same places.

use super::*;

/// Where a main-menu entry leads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MainDestination {
    /// The server browser, refreshed when its rows are stale.
    Browser,
    /// The Create game screen.
    CreateGame,
    /// The Player screen (name, model, saber, Force).
    Player,
    /// The settings screen, on tab `tab`.
    Settings { tab: usize },
    /// The renderer settings on their own, closing straight back.
    Renderer,
    /// The key-binding editor on category tab `category`, closing straight
    /// back to the main menu.
    Keybinds { category: usize },
    /// The changelog page, drawn by the console over the menu.
    Changelog,
    /// The credits page, drawn by the console over the menu.
    Credits,
    /// The update page, drawn by the console over the menu.
    Update,
    /// The identity page, drawn by the console over the menu.
    Identity,
    /// The Profile page, drawn by the console over the menu.
    Profile,
    /// The Collection (medals, achievements, shaders, toys, nameplates), drawn by the
    /// console over the menu, on the tab it showed last.
    Collection,
    /// Exit to the desktop.
    Quit,
}

impl ClientMenu {
    /// Open `destination` from the main menu.
    pub(super) fn open_main_destination(
        &mut self,
        destination: MainDestination,
        console: &mut ViewerConsole,
    ) -> MenuAction {
        match destination {
            MainDestination::Browser => {
                if let Ok(master) = console.master_server() {
                    self.browser.set_master(master);
                }
                self.open_browser();
                // Rows fetched at start-up (or moments ago) show at once; a
                // fetch is only started when there is nothing fresh to show.
                if self.browser.is_stale() {
                    self.refresh();
                } else if self.browser.is_refreshing() {
                    self.state.set_status("Refreshing master server...");
                } else {
                    self.state.set_status(format!(
                        "{} responding servers. Enter joins; R refreshes.",
                        self.browser.entries().len()
                    ));
                }
                MenuAction::None
            }
            MainDestination::CreateGame => {
                self.open_create_game(console);
                MenuAction::None
            }
            MainDestination::Player => {
                // The SJK UI's Profile screen, on its Character tab.
                self.open_player_hub(console, ReturnTarget::MainMenu);
                MenuAction::None
            }
            // The SJK UI's Settings, on the category holding the tab's rows.
            MainDestination::Settings { tab } => {
                self.open_sjk_settings_on_tab(console, tab, ReturnTarget::MainMenu);
                MenuAction::None
            }
            // The SJK UI's Settings on Graphics, which gathers the renderer
            // rows.
            MainDestination::Renderer => {
                self.open_sjk_renderer(console, ReturnTarget::MainMenu);
                MenuAction::None
            }
            // The SJK UI's Key bindings: every binding in one list.
            MainDestination::Keybinds { .. } => {
                self.open_sjk_keys(console, ReturnTarget::MainMenu);
                MenuAction::None
            }
            MainDestination::Changelog => {
                console.open_changelog();
                MenuAction::None
            }
            MainDestination::Credits => {
                console.open_credits();
                MenuAction::None
            }
            MainDestination::Update => {
                console.open_update_panel();
                MenuAction::None
            }
            MainDestination::Identity => {
                console.open_identity_panel();
                MenuAction::None
            }
            MainDestination::Profile if self.menu_style == MenuStyle::Sjk => {
                // The SJK UI's Profile screen, on its SJK Profile tab.
                console.open_profile_hub_page(
                    crate::profile_hub::Tab::Profile,
                    ReturnTarget::MainMenu,
                );
                MenuAction::None
            }
            MainDestination::Profile => {
                console.open_profile_panel();
                MenuAction::None
            }
            MainDestination::Collection if self.menu_style == MenuStyle::Sjk => {
                // The SJK UI's Collection screen.
                let tab = console.collection_tab();
                console.open_profile_hub_page(tab, ReturnTarget::MainMenu);
                MenuAction::None
            }
            MainDestination::Collection => {
                console.open_collection_panel();
                MenuAction::None
            }
            MainDestination::Quit => MenuAction::Quit,
        }
    }

    /// Leave the key bindings, straight to wherever settings return.
    pub(super) fn close_keybinds(&mut self) -> MenuAction {
        self.close_settings()
    }
}
