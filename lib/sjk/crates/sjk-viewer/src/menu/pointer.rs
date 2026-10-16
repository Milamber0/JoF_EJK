//! Pointer dispatch for every client-shell phase.

use super::*;
use sjk_ui::UiEventKind;
use std::time::{Duration, Instant};

/// Rows one wheel notch scrolls the server table.
const WHEEL_ROWS: i32 = 3;
/// Two clicks on the same row this close together join it.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// Header cells left to right, indexed by `token - HEADER_TOKEN`.
pub(super) const SORT_COLUMNS: [SortColumn; 5] = [
    SortColumn::Name,
    SortColumn::Map,
    SortColumn::Players,
    SortColumn::Ping,
    SortColumn::Gametype,
];

impl ClientMenu {
    /// Route a normalized pointer event through the currently visible screen.
    pub(crate) fn handle_pointer(
        &mut self,
        event: InputEvent,
        console: &mut ViewerConsole,
    ) -> MenuAction {
        if !self.is_visible() {
            return MenuAction::None;
        }
        match self.state.phase() {
            ClientPhase::Settings => {
                let result = self.settings.handle_pointer(event, console);
                self.settings_result(result, console)
            }
            ClientPhase::Keybinds => {
                let result = self.keybinds.handle_pointer(event, console);
                self.keybinds_result(result, console)
            }
            ClientPhase::Player => {
                let result = self.player.handle_pointer(event, console);
                self.player_result(result, console)
            }
            ClientPhase::CreateGame => {
                let result = self.create_game.pointer(event, console);
                self.create_game_result(result)
            }
            _ => self.handle_shell_pointer(event, console),
        }
    }

    fn handle_shell_pointer(
        &mut self,
        event: InputEvent,
        console: &mut ViewerConsole,
    ) -> MenuAction {
        let Some(event) = self.ui.pointer(event) else {
            return MenuAction::None;
        };
        let Some(token) = event.token else {
            return MenuAction::None;
        };
        match self.state.phase() {
            ClientPhase::MainMenu if self.menu_style == MenuStyle::Sjk => {
                self.sjk_home_pointer(token, event.kind == UiEventKind::Activate, console)
            }
            ClientPhase::MainMenu => {
                self.classic.select(usize::from(token));
                if event.kind == UiEventKind::Activate {
                    self.activate_classic(console)
                } else {
                    MenuAction::None
                }
            }
            ClientPhase::Browser => {
                self.handle_browser_pointer(token, event.kind, event.position, event.delta, console)
            }
            ClientPhase::Connecting(_) if token == 0 && event.kind == UiEventKind::Activate => {
                self.cancel_join()
            }
            ClientPhase::ConnectionError if token == 0 && event.kind == UiEventKind::Activate => {
                self.state.open_browser();
                MenuAction::None
            }
            ClientPhase::Connecting(_) | ClientPhase::ConnectionError | ClientPhase::InGame => {
                MenuAction::None
            }
            ClientPhase::Settings | ClientPhase::Keybinds => unreachable!(),
            ClientPhase::Player | ClientPhase::CreateGame => unreachable!(),
        }
    }

    pub(super) fn handle_browser_pointer(
        &mut self,
        token: u16,
        kind: UiEventKind,
        position: Option<sjk_ui::Vec2>,
        delta: Option<sjk_ui::Vec2>,
        console: &mut ViewerConsole,
    ) -> MenuAction {
        use super::browser_view::{
            ADDRESS_TOKEN, BACK_TOKEN, FAVOURITE_TOKEN, FILTER_TOKEN, HEADER_TOKEN, JOIN_TOKEN,
            REFRESH_TOKEN, ROW_TOKEN, SCROLLBAR_TOKEN, tab_for_token,
        };
        // Hovering only highlights (the row frame reads the hover state);
        // the selection and the scroll window stay where the user put them.
        if kind == UiEventKind::Wheel {
            if self.classic_info {
                // The SERVER INFO pop-up covers the list.
                return MenuAction::None;
            }
            let direction = delta.map_or(0, |delta| -delta.y.signum() as i32);
            if self.menu_style == MenuStyle::Sjk && token == super::sjk::browser::PLAYERS_TOKEN {
                // The details panel's players, not the server table.
                self.browser.scroll_players(
                    direction * super::sjk::browser::PLAYER_WHEEL_ROWS,
                    super::sjk::browser::PLAYER_LINES,
                );
                return MenuAction::None;
            }
            self.browser.scroll_by(direction * WHEEL_ROWS);
            return MenuAction::None;
        }
        if kind == UiEventKind::Drag && token == SCROLLBAR_TOKEN {
            if let (Some(point), Some(track)) = (position, self.ui.rect_for(SCROLLBAR_TOKEN)) {
                let ratio = ((point.y - track.y) / track.height).clamp(0.0, 1.0);
                let span = self
                    .browser
                    .visible_len()
                    .saturating_sub(self.browser.page());
                self.browser
                    .scroll_to((ratio * span as f32).round() as usize);
            }
            return MenuAction::None;
        }
        if kind != UiEventKind::Activate {
            return MenuAction::None;
        }
        if self.menu_style == MenuStyle::Sjk
            && let Some(action) = self.sjk_browser_pointer(token, position, console)
        {
            return action;
        }
        if self.activate_filter(token, console) {
            return MenuAction::None;
        }
        if token != FILTER_TOKEN {
            self.filter_editing = false;
        }
        if (ROW_TOKEN..ROW_TOKEN + super::browser_table::ROW_TOKENS).contains(&token) {
            return self.click_browser_row(token);
        }
        if (HEADER_TOKEN..HEADER_TOKEN + 5).contains(&token) {
            self.browser
                .sort_by(SORT_COLUMNS[usize::from(token - HEADER_TOKEN)]);
            return MenuAction::None;
        }
        if let Some(tab) = tab_for_token(token) {
            self.browser.set_favorites_only(tab == 1);
            self.browser_focus = ROW_TOKEN + self.browser.selected() as u16;
            return MenuAction::None;
        }
        use super::classic::browser::{
            EXIT_TOKEN, INFO_CLOSE_TOKEN, INFO_TOKEN, REFRESH_LIST_TOKEN,
        };
        match token {
            ADDRESS_TOKEN => self.open_address_entry(),
            super::browser_view::ADD_FAVOURITE_TOKEN => self.open_favourite_entry(),
            REFRESH_TOKEN | REFRESH_LIST_TOKEN => self.refresh(),
            INFO_TOKEN => self.classic_info = true,
            INFO_CLOSE_TOKEN => self.classic_info = false,
            EXIT_TOKEN => {
                // Retail's EXIT closes the screen onto the quit page.
                let action = self.close_browser();
                if self.browser_return == ReturnTarget::MainMenu {
                    self.classic.show(super::classic::layout::Page::Quit);
                }
                return action;
            }
            FAVOURITE_TOKEN => self.browser.toggle_selected_favorite(),
            BACK_TOKEN => return self.close_browser(),
            JOIN_TOKEN => return self.join_selected(),
            FILTER_TOKEN => self.filter_editing = true,
            31 => {
                self.password_target = None;
                self.password.clear();
            }
            32 => return self.submit_password(console),
            34 => self.remember_password = !self.remember_password,
            41 => self.address_editing = true,
            42 => {
                self.address_editing = false;
                self.address_error.clear();
            }
            43 => return self.submit_address(),
            _ => {}
        }
        MenuAction::None
    }

    /// A click selects the row; a second click on the same row within
    /// [`DOUBLE_CLICK`] joins it.
    fn click_browser_row(&mut self, token: u16) -> MenuAction {
        let now = Instant::now();
        let repeat = self
            .browser_last_click
            .is_some_and(|(last, at)| last == token && now.duration_since(at) <= DOUBLE_CLICK);
        self.browser_last_click = Some((token, now));
        self.browser_focus = token;
        self.browser
            .set_selection(usize::from(token - super::browser_view::ROW_TOKEN));
        if repeat {
            self.browser_last_click = None;
            self.join_selected()
        } else {
            MenuAction::None
        }
    }

    fn submit_password(&mut self, console: &mut ViewerConsole) -> MenuAction {
        let Some(address) = self.password_target.clone() else {
            return MenuAction::None;
        };
        if self.password.is_empty() {
            return MenuAction::None;
        }
        console.set_cvar("password", &self.password);
        self.server_passwords
            .update(&address, &self.password, self.remember_password);
        self.password_target = None;
        self.state.connecting(address.clone());
        MenuAction::Connect(address)
    }
}
