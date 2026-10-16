//! The menu's side of a hosted game: opening Create game, handing its
//! server to the menu, following the start-up, and ending the server with
//! the game it hosts.

use super::create_game::CreateGameResult;
use super::*;
use crate::connection::local_server::LocalServer;

impl ClientMenu {
    /// Open the Create game screen from the main menu.
    pub(crate) fn open_create_game(&mut self, console: &ViewerConsole) {
        self.create_game.open(console);
        self.state.open_create_game();
    }

    /// Open Create game on row `selected`, with its map list open and `filter`
    /// typed when it is given (world shots).
    #[cfg(test)]
    pub(crate) fn create_game_for_shot(
        &mut self,
        console: &ViewerConsole,
        selected: usize,
        filter: Option<&str>,
    ) {
        if !matches!(self.state.phase(), ClientPhase::CreateGame) {
            self.open_create_game(console);
        }
        self.create_game.for_shot(selected, filter);
    }

    /// Show Create game's server name being typed, `typed` so far (world shots).
    #[cfg(test)]
    pub(crate) fn create_game_name_for_shot(&mut self, typed: &str) {
        self.create_game.editing = Some(typed.to_owned());
    }

    /// Whether Create game's levelshot has decoded and its canvas had room
    /// (world shots): (settled, overflowed).
    #[cfg(test)]
    pub(crate) fn create_game_shot_state(&self) -> (bool, bool) {
        (
            self.create_game.picture_settled(),
            self.create_game.overflowed(),
        )
    }

    /// Carry out what the Create game screen asked for.
    pub(super) fn create_game_result(&mut self, result: CreateGameResult) -> MenuAction {
        match result {
            CreateGameResult::None => MenuAction::None,
            CreateGameResult::Back => {
                // Leaving while it starts abandons the game.
                self.create_game.stop_server();
                self.state.main_menu();
                MenuAction::None
            }
            CreateGameResult::Start(settings) => MenuAction::HostGame(settings),
        }
    }

    /// Escape on the connecting notice: back to where the join came from —
    /// Create game (stopping its server) for a hosted game, else the browser.
    pub(super) fn cancel_join(&mut self) -> MenuAction {
        if self.create_game.stop_server() {
            self.state.open_create_game();
        } else {
            self.state.open_browser();
        }
        MenuAction::CancelJoin
    }

    /// The application started the server `settings` asked for.
    pub(crate) fn local_server_started(&mut self, server: LocalServer, map: &str) {
        self.create_game.server_started(server);
        // The gate at the browser vantage opens onto the hosted map.
        self.destination_map = Some(map.to_owned());
    }

    /// The server could not be started; the reason stays on the screen.
    pub(crate) fn local_server_failed(&mut self, reason: &str) {
        self.create_game.server_failed(reason);
    }

    /// Follow a starting server (each frame, from [`Self::poll`]).
    pub(super) fn poll_local_server(&mut self) {
        if self.create_game.poll_server().is_some()
            && !matches!(self.state.phase(), ClientPhase::CreateGame)
        {
            self.state.open_create_game();
        }
    }

    /// The hosted server's address, once, when it is ready to be joined.
    pub(crate) fn take_local_join(&mut self) -> Option<String> {
        self.create_game
            .take_join()
            .map(|address| address.to_string())
    }

    /// End the hosted game's server, if one runs (leaving the game, a lost
    /// connection, a failed join).
    pub(crate) fn stop_local_server(&mut self) {
        if self.create_game.stop_server() {
            crate::log::progress(format_args!("local server stopped: the hosted game ended"));
        }
    }

    /// A join to `address` is starting: any hosted server elsewhere ends.
    pub(crate) fn release_local_server_unless(&mut self, address: &str) {
        self.create_game.release_unless(address);
    }
}
