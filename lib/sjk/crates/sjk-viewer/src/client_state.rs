//! Explicit high-level client-shell state transitions.

/// Mutually exclusive screens and connection phases of the native client.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ClientPhase {
    MainMenu,
    Browser,
    Settings,
    Keybinds,
    Player,
    CreateGame,
    Connecting(String),
    ConnectionError,
    InGame,
}

/// Small state machine shared by menu input, rendering, and async operations.
pub(crate) struct ClientState {
    phase: ClientPhase,
    status: String,
}

impl ClientState {
    pub(crate) fn new(open_main_menu: bool) -> Self {
        Self {
            phase: if open_main_menu {
                ClientPhase::MainMenu
            } else {
                ClientPhase::InGame
            },
            status: String::new(),
        }
    }

    pub(crate) fn phase(&self) -> &ClientPhase {
        &self.phase
    }

    pub(crate) fn status(&self) -> &str {
        &self.status
    }

    pub(crate) fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    pub(crate) fn open_browser(&mut self) {
        self.phase = ClientPhase::Browser;
        self.status.clear();
    }

    pub(crate) fn open_settings(&mut self) {
        self.phase = ClientPhase::Settings;
        self.status.clear();
    }

    pub(crate) fn open_keybinds(&mut self) {
        self.phase = ClientPhase::Keybinds;
        self.status.clear();
    }

    pub(crate) fn open_player(&mut self) {
        self.phase = ClientPhase::Player;
        self.status.clear();
    }

    pub(crate) fn open_create_game(&mut self) {
        self.phase = ClientPhase::CreateGame;
        self.status.clear();
    }

    pub(crate) fn main_menu(&mut self) {
        self.phase = ClientPhase::MainMenu;
        self.status.clear();
    }

    pub(crate) fn connecting(&mut self, address: impl Into<String>) {
        let address = address.into();
        self.status = format!("Connecting to {address}...");
        self.phase = ClientPhase::Connecting(address);
    }

    pub(crate) fn connection_failed(&mut self, error: impl Into<String>) {
        self.phase = ClientPhase::ConnectionError;
        self.status = format!("Connection failed: {}", error.into());
    }

    pub(crate) fn entered_game(&mut self) {
        self.phase = ClientPhase::InGame;
        self.status.clear();
    }

    pub(crate) fn is_overlay_visible(&self) -> bool {
        !matches!(self.phase, ClientPhase::InGame)
    }
}
