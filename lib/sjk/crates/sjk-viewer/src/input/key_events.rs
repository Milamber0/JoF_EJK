//! Native keyboard routing into UI and the console command buffer.
use super::alt_code;
use crate::*;

impl GpuState {
    pub(crate) fn keyboard(&mut self, event: KeyEvent) {
        // Whoever takes the release, a held push or pull key stops repeating.
        if event.state == ElementState::Released
            && let PhysicalKey::Code(code) = event.physical_key
            && let Some(console) = &mut self.console
        {
            console.force_repeat_key_up(code, crate::input::keys::key_name(&event));
        }
        let typing = self.text_has_keyboard();
        match self.alt_code.key(&event, typing) {
            alt_code::Step::Pass => self.route_keyboard(event),
            alt_code::Step::Withhold => {}
            alt_code::Step::Type(character) => {
                let typed = alt_code::typed_event(&event, character);
                self.route_keyboard(event);
                self.route_keyboard(typed);
            }
        }
    }

    /// A text field has the keyboard: the console, the chat composer or a menu,
    /// rather than the gameplay bindings.
    pub(crate) fn text_has_keyboard(&self) -> bool {
        self.console
            .as_ref()
            .is_some_and(console::ViewerConsole::is_open)
            || self.chat.is_typing()
            || self.text_dialog.is_open()
            || self
                .client_menu
                .as_ref()
                .is_some_and(|menu| menu.is_visible())
            || self.game_menu
    }

    fn route_keyboard(&mut self, event: KeyEvent) {
        // Escape closes an open quick wheel without choosing, and goes no further.
        if self.quick_wheel.is_open()
            && event.state == winit::event::ElementState::Pressed
            && event.physical_key == PhysicalKey::Code(winit::keyboard::KeyCode::Escape)
        {
            self.quick_wheel.cancel();
            return;
        }
        // A modern composer must be able to type `~`, unlike stock, where the
        // console key precedes the message catcher (`cl_keys.cpp:1318`).
        let console_open = self
            .console
            .as_ref()
            .is_some_and(console::ViewerConsole::is_open);
        let typed = matches!(event.physical_key, PhysicalKey::Code(_));
        if !console_open && self.text_dialog.is_open() {
            let action = self.text_dialog.handle_key(&event);
            self.apply_dialog_action(action);
            return;
        }
        // A new medal on show takes every key until it is closed.
        if !console_open && self.medal_popup.is_open() {
            self.medal_popup.handle_key(&event);
            return;
        }
        // So does a holocron drop.
        if !console_open && self.holocron_popup.is_open() {
            self.holocron_popup.handle_key(&event);
            return;
        }
        // So does the graphics reload card.
        if !console_open && self.graphics_reload.card.is_open() {
            let choice = self.graphics_reload.card.handle_key(&event);
            self.reload_card_choice(choice);
            return;
        }
        // And the low-FPS card.
        if !console_open && self.fps_card.is_open() {
            let choice = self.fps_card.handle_key(&event);
            self.fps_card_choice(choice);
            return;
        }
        // And the log request card.
        if !console_open && self.log_request_card.is_open() {
            let choice = self.log_request_card.handle_key(&event);
            self.log_request_card_choice(choice);
            return;
        }
        // And the what's-new card.
        if !console_open && self.whats_new_card.is_open() {
            let choice = self.whats_new_card.handle_key(&event);
            self.whats_new_card_choice(choice);
            return;
        }
        // Ctrl+Tab changes the Profile screen's tab, whichever screen shows it.
        if self.profile_hub_key(&event) {
            return;
        }
        if !console_open && typed && self.chat.is_typing() {
            self.chat_key(&event);
            return;
        }
        // Typing in the SJK UI game menu's docked chat: the console key and the
        // bindings type there too.
        if !console_open && typed && self.sjk_chat_typing(&event) {
            return;
        }
        if self.route_console_key(&event) {
            return;
        }
        if self
            .client_menu
            .as_ref()
            .is_some_and(|menu| menu.is_visible())
        {
            let action = match (&mut self.client_menu, &mut self.console) {
                (Some(menu), Some(console)) => menu.handle_key(&event, console),
                _ => menu::MenuAction::None,
            };
            self.apply_client_menu_action(action);
            return;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return;
        };
        if self.chat.is_typing() {
            self.chat_key(&event);
            return;
        }
        if self.shot_key(&event) {
            return;
        }
        if self.game_menu {
            if event.state != ElementState::Pressed || event.repeat {
                return;
            }
            // The SJK UI's main page: its list, row of icons and match card.
            if self.sjk_main_key(key) {
                return;
            }
            match key {
                // The SJK UI's arc passes over rows that cannot be taken.
                KeyCode::ArrowUp
                | KeyCode::KeyW
                | KeyCode::ArrowDown
                | KeyCode::KeyS
                | KeyCode::Tab
                    if self.in_game_menu.is_sjk() =>
                {
                    let forward = !matches!(key, KeyCode::ArrowUp | KeyCode::KeyW);
                    self.game_menu_row = self.in_game_menu.sjk_step(
                        self.game_menu_page,
                        self.game_menu_row,
                        self.game_menu_row_count(),
                        forward,
                    );
                }
                KeyCode::ArrowUp | KeyCode::KeyW => {
                    let count = self.game_menu_row_count();
                    self.game_menu_row = self
                        .in_game_menu
                        .navigate(sjk_ui::AbstractAction::Previous)
                        .filter(|row| *row < count)
                        .unwrap_or_else(|| self.game_menu_row.checked_sub(1).unwrap_or(count - 1));
                }
                KeyCode::ArrowDown | KeyCode::KeyS | KeyCode::Tab => {
                    let count = self.game_menu_row_count();
                    self.game_menu_row = self
                        .in_game_menu
                        .navigate(sjk_ui::AbstractAction::Next)
                        .filter(|row| *row < count)
                        .unwrap_or((self.game_menu_row + 1) % count);
                }
                KeyCode::ArrowLeft | KeyCode::ArrowRight | KeyCode::KeyA | KeyCode::KeyD
                    if self.in_game_menu.is_classic() =>
                {
                    self.classic_menu_sideways(matches!(key, KeyCode::ArrowRight | KeyCode::KeyD));
                }
                KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                    if self.in_game_menu.activation_allowed(self.game_menu_row) {
                        self.activate_game_menu_row();
                    }
                }
                KeyCode::Escape => self.back_or_close_game_menu(),
                _ => {}
            }
            return;
        }
        // Escape unpins a pinned player card, or drops a waiting `worldnote` selection,
        // before it opens the game menu.
        if key == KeyCode::Escape
            && event.state == ElementState::Pressed
            && (self.hud.card.cancel() || self.world_notes.cancel_selection())
        {
            return;
        }
        if key == KeyCode::Escape
            && event.state == ElementState::Pressed
            && !self
                .console
                .as_ref()
                .is_some_and(|c| c.has_key_binding("ESCAPE"))
        {
            self.release_pointer();
            if self.live_session.is_some() || self.resident.exploring() {
                self.game_menu = true;
                self.game_menu_page = GameMenuPage::Main;
                self.game_menu_row = 0;
                self.in_game_menu.focus = crate::ingame_menu::sjk_focus::Focus::List;
                self.in_game_menu.stop_chat_typing();
            }
            return;
        }
        if event.repeat {
            return;
        }
        if let Some(console) = &mut self.console {
            let name = crate::input::keys::key_name(&event);
            console.queue_bound_key(key, name, event.state == ElementState::Pressed);
        }
    }
}
