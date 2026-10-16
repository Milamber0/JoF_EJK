//! Application routing for local console connection commands.

use super::*;

impl GpuState {
    /// Route console input, then apply any connection action after releasing
    /// the console/session borrow.
    pub(crate) fn route_console_key(&mut self, event: &KeyEvent) -> bool {
        let was_open = self
            .console
            .as_ref()
            .is_some_and(console::ViewerConsole::is_open);
        // The browser's search field takes a dead ^ as a literal colour-code
        // prefix. The input line composes dead keys itself (`input::dead_key`):
        // ^ then e types ê there, and ^ then a digit a colour code.
        let browsing = self
            .console
            .as_ref()
            .is_some_and(console::ViewerConsole::covers_frame);
        let literal_caret = was_open
            && browsing
            && event.state == ElementState::Pressed
            && matches!(event.logical_key, winit::keyboard::Key::Dead(Some('^')));
        let mut literal_event;
        let event = if literal_caret {
            literal_event = event.clone();
            literal_event.logical_key = winit::keyboard::Key::Character("^".into());
            literal_event.text = Some("^".into());
            &literal_event
        } else {
            event
        };
        let consumed = self
            .console
            .as_mut()
            .is_some_and(|console| console.handle_key(event, self.live_session.as_mut()));
        let is_open = self
            .console
            .as_ref()
            .is_some_and(console::ViewerConsole::is_open);
        if consumed && (was_open != is_open || literal_caret) {
            // The toggle can be a dead key (e.g. ^ on a German layout). It was
            // handled as an action, so its pending accent must not consume or
            // compose with the next typed letter. Leave ordinary text alone.
            if let Some(window) = &self.window {
                window.reset_dead_keys();
            }
        }
        let action = self
            .console
            .as_mut()
            .and_then(console::ViewerConsole::take_connection_action);
        if let Some(action) = action {
            self.apply_console_connection_action(action);
        }
        if consumed
            && self
                .console
                .as_ref()
                .is_some_and(console::ViewerConsole::is_open)
        {
            self.gameplay_input.release_keys();
            self.release_pointer();
        }
        consumed
    }

    pub(crate) fn apply_console_connection_action(&mut self, action: console::ConnectionAction) {
        match action {
            console::ConnectionAction::Connect(address) => self.begin_address_join(address),
            console::ConnectionAction::Disconnect => self.disconnect_to_menu(),
            console::ConnectionAction::Reconnect => self.reconnect_last(),
            console::ConnectionAction::DevMap(map) => self.start_development_map(map),
        }
    }
}

pub(crate) fn reconnect_target(last: Option<&str>) -> Result<String, &'static str> {
    last.map(str::to_owned)
        .ok_or("No previous server address to reconnect")
}
