//! The server browser's pointer tokens, and the classic browser screen
//! ([`super::classic::browser`]) over the browser model.

use super::ClientMenu;
use crate::menu_widgets::TAB_BASE;
use crate::{TextVertex, UiFont};

pub(super) use super::browser_table::{HEADER_TOKEN, ROW_TOKEN, SCROLLBAR_TOKEN};

pub(super) const REFRESH_TOKEN: u16 = 10;
pub(super) const FAVOURITE_TOKEN: u16 = 11;
pub(super) const BACK_TOKEN: u16 = 12;
pub(super) const JOIN_TOKEN: u16 = 13;
pub(super) const ADDRESS_TOKEN: u16 = 15;
/// The SJK UI browser's Add a favourite (by its address).
pub(super) const ADD_FAVOURITE_TOKEN: u16 = 26;
pub(super) const FILTER_TOKEN: u16 = 20;
/// The server sources, all servers and favourites; source `i` answers to
/// `TAB_BASE + i`.
const SOURCES: u16 = 2;

impl ClientMenu {
    /// Retail's join-server screen ([`super::classic::browser`]) over the
    /// browser model and pointer tokens; the SJK UI draws its own
    /// ([`super::sjk::browser`]).
    pub(super) fn append_browser(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        reveal: f32,
    ) {
        use super::classic::browser::{Prompt, Screen, build};
        let status = if self.browser.is_refreshing() && self.browser.entries().is_empty() {
            "Querying the master server..."
        } else if self.browser.is_refreshing() {
            "Servers are answering..."
        } else {
            self.state.status()
        };
        let prompt = if self.address_editing {
            Prompt::Address {
                input: &self.address_input,
                error: &self.address_error,
            }
        } else if self.password_target.is_some() {
            Prompt::Password {
                length: self.password.chars().count(),
            }
        } else {
            Prompt::None
        };
        let screen = Screen {
            status,
            art: self.art,
            reveal,
            filter_editing: self.filter_editing,
            info_open: self.classic_info,
            from_game: self.browser_return == crate::player_menu::ReturnTarget::InGame,
            prompt,
        };
        build(&mut self.ui, viewport, &mut self.browser, &screen);
        self.ui.finish(self.browser_focus);
        self.ui.append_text(vertices, font, viewport);
    }
}

/// Which source (0 all servers, 1 favourites) a source token selects, if it is
/// one.
pub(super) fn tab_for_token(token: u16) -> Option<usize> {
    (TAB_BASE..TAB_BASE + SOURCES)
        .contains(&token)
        .then(|| usize::from(token - TAB_BASE))
}
