//! What the classic in-game menu's bar buttons and pop-ups do. Pages the
//! classic menu shares with the SJK UI (about, join, Siege classes, call
//! vote) keep their actions in `game_menu_actions.rs`; this module covers
//! the bar and the retail pop-ups that differ.

use super::Page;
use super::classic::{self, NO, Tab, YES, leave};
use crate::GpuState;
use sjk_client::ClientSession;
use sjk_ui::UiEventKind;

impl GpuState {
    /// Whether the connected server runs Siege.
    pub(crate) fn is_siege_game(&self) -> bool {
        super::siege_data::is_siege(self.live_session.as_ref().map(ClientSession::game_state))
    }

    /// Activate the focused row of a classic page. Returns false for pages
    /// whose rows keep the shared actions.
    pub(crate) fn activate_classic_row(&mut self) -> bool {
        let row = self.game_menu_row;
        match self.game_menu_page {
            Page::Main => self.activate_classic_tab(row),
            Page::Leave => match row {
                leave::MAIN_MENU => self.open_confirmation(Page::ConfirmLeave),
                leave::QUIT => self.open_confirmation(Page::ConfirmQuit),
                _ => {}
            },
            Page::ConfirmLeave | Page::ConfirmQuit if row == NO => {
                self.open_game_menu_page(Page::Leave);
            }
            Page::ConfirmLeave => self.disconnect_to_menu(),
            Page::ConfirmQuit => self.quit_requested = true,
            Page::Sjk => self.activate_sjk_row(),
            Page::Vote => {
                // Retail votes and closes the menu (`uiScript closeingame`).
                self.send_vote(row == YES);
                self.game_menu = false;
                self.capture_pointer();
            }
            _ => return false,
        }
        true
    }

    /// Open a Yes/No confirmation with No focused, so a second Enter does
    /// not leave.
    fn open_confirmation(&mut self, page: Page) {
        self.open_game_menu_page(page);
        self.game_menu_row = NO;
    }

    /// Press bar button `index`: open its pop-up, or hand over to the screen
    /// it stands for.
    fn activate_classic_tab(&mut self, index: usize) {
        let Some(tab) = Tab::at(index) else {
            return;
        };
        if tab.unavailable(self.is_siege_game()).is_some() {
            return;
        }
        match tab {
            Tab::Sjk => self.open_game_menu_page(Page::Sjk),
            Tab::About => {
                let game_state = self.live_session.as_ref().map(ClientSession::game_state);
                self.in_game_menu
                    .refresh_about(game_state, self.last_connect_address.as_deref());
                self.open_game_menu_page(Page::About);
            }
            Tab::Join => {
                if !self.open_siege_classes() {
                    self.open_game_menu_page(Page::Team);
                }
            }
            Tab::Profile => self.open_player_menu_from_game(),
            // Unavailable outside Siege and inside it; kept for completeness.
            Tab::AddBot => {}
            Tab::Settings => {
                self.open_classic_panel_from_game(crate::menu::classic::layout::Page::Setup);
            }
            Tab::Vote => self.open_game_menu_page(Page::Vote),
            Tab::CallVote => {
                let game_state = self.live_session.as_ref().map(ClientSession::game_state);
                self.in_game_menu
                    .refresh_callvote(game_state, self.vfs.as_deref());
                self.open_game_menu_page(Page::CallVote);
            }
            Tab::Exit => self.open_game_menu_page(Page::Leave),
        }
    }

    /// A pointer event on bar button `index`. Hovering moves the bar's focus
    /// only while no pop-up is open; a click opens the button's pop-up from
    /// anywhere, as clicking another retail bar button does.
    pub(crate) fn classic_bar_pointer(&mut self, kind: UiEventKind, index: usize) {
        match kind {
            UiEventKind::HoverEnter | UiEventKind::Hover if self.game_menu_page == Page::Main => {
                self.game_menu_row = index;
            }
            UiEventKind::Activate => {
                self.game_menu_page = Page::Main;
                self.game_menu_row = index;
                self.activate_classic_tab(index);
            }
            _ => {}
        }
    }

    /// Left or Right in the classic menu: step along the bar, or from a
    /// pop-up to the neighbouring bar button (closing the pop-up).
    pub(crate) fn classic_menu_sideways(&mut self, forward: bool) {
        let count = Tab::ALL.len();
        let current = if self.game_menu_page == Page::Main {
            self.game_menu_row
        } else {
            classic::Tab::of_page(self.game_menu_page).map_or(0, Tab::index)
        };
        self.game_menu_page = Page::Main;
        self.game_menu_row = if forward {
            (current + 1) % count
        } else {
            current.checked_sub(1).unwrap_or(count - 1)
        };
    }
}
