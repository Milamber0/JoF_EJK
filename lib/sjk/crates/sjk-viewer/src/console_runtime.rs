//! Per-frame bridge from portable command buffering to viewer actions.

use super::*;

const MAX_ACTIONS_PER_FRAME: usize = 64;

impl GpuState {
    /// Execute cfg and bind scripts once per rendered frame.
    pub(crate) fn run_console_command_buffer(&mut self, audio: &mut Option<GameAudio>) {
        // A chip naming a setting (the changelog page's, the what's-new card's) opens
        // Settings on it.
        self.open_chip_settings();
        let Some(console) = &mut self.console else {
            return;
        };
        // The Collection's Toys tab lights or puts out the holocron through the console.
        if let Some(on) = console.take_illuminate_request() {
            self.illuminate.set_on(on);
        }
        let mut actions = [None; MAX_ACTIONS_PER_FRAME];
        console.advance_command_frame();
        console.pump_console_socket();
        let mut action_count = 0;
        let gameplay = &mut self.gameplay_input;
        for command in console.drain_input() {
            if let Some(action) = gameplay.apply(&command) {
                actions[action_count] = Some(action);
                action_count += 1;
            }
        }
        console.execute_buffered_frame(self.live_session.as_mut(), |command, _| {
            if !input::GameplayInput::recognizes(command) {
                return false;
            }
            if let Some(action) = gameplay.apply(command)
                && action_count < actions.len()
            {
                actions[action_count] = Some(action);
                action_count += 1;
            }
            true
        });
        if gameplay.finish_buffered_input() {
            action_count = 0;
        }
        if console.take_quit() {
            self.quit_requested = true;
        }
        for action in actions[..action_count].iter().copied().flatten() {
            self.apply_input_action(Some(action));
        }
        // `toy_illuminate` may have flipped it: tell the Toys tab.
        if let Some(console) = self.console.as_mut() {
            console.set_illuminate_lit(self.illuminate.lit());
        }
        let connection = self
            .console
            .as_mut()
            .and_then(console::ViewerConsole::take_connection_action);
        if let Some(action) = connection {
            self.apply_console_connection_action(action);
        }
        let screenshot = self
            .console
            .as_mut()
            .and_then(console::ViewerConsole::take_screenshot_request);
        if let Some(request) = screenshot {
            self.screenshots.request(request);
        }
        let demo = self
            .console
            .as_mut()
            .and_then(console::ViewerConsole::take_demo_action);
        if let Some(action) = demo {
            self.apply_console_demo_action(action);
        }
        self.run_client_commands(audio);
        // Save settings changed this frame or earlier, once they hold still.
        if let Some(console) = self.console.as_mut() {
            console.persist();
            // A release made while playing shows its update card within half an hour.
            console.recheck_for_updates();
        }
    }
}
