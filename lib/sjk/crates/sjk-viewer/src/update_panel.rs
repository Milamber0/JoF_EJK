//! The Update page: whether a newer SJK release exists, and the buttons to check
//! again, install it or read its notes (state and work in `update.rs`).
//!
//! Opened by the main menu's Update entry or the `update` console command.
//! It is drawn in the SJK UI's look in every menu style (`update_panel_sjk.rs`).
//! Like the changelog page it lives in the console and is drawn in place of it,
//! so it opens over the menus and in a match.

use crate::menu_widgets::{BACK_TOKEN, MenuCanvas};
use crate::update::{self, State};
use sjk_ui::{InputEvent, UiEventKind};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

#[path = "update_panel_sjk.rs"]
mod sjk;

/// Console command that toggles the page.
pub(crate) const COMMAND: &str = "update";
/// Help text for completion and `cmdlist`.
pub(crate) const HELP: &str = "Check for a newer SJK release and install it";

/// Pointer targets of the footer's actions.
const PRIMARY_TOKEN: u16 = 920;
const CHECK_TOKEN: u16 = 921;
const NOTES_TOKEN: u16 = 922;

/// What the console does after the page handled an event.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PanelAction {
    None,
    Close,
    /// Look for a release again (the console knows the installed version).
    Check,
    /// Leave the client; the installed update starts as it exits.
    Quit,
}

pub(crate) struct Panel {
    open: bool,
    /// The page opened the console, so closing the page closes it too.
    owns_console: bool,
    ui: MenuCanvas,
}

impl Default for Panel {
    fn default() -> Self {
        Self::new()
    }
}

/// What the page says and offers in one state.
struct View {
    headline: String,
    detail: String,
    /// Download progress, 0 to 1.
    progress: Option<f32>,
    /// The Enter action's caption, if Enter does anything.
    primary: Option<&'static str>,
    /// Whether "check again" is offered.
    check: bool,
    /// Whether the release notes can be opened.
    notes: bool,
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

fn view(state: &State, installed: &str) -> View {
    let plain = |headline: &str, detail: String| View {
        headline: headline.to_owned(),
        detail,
        progress: None,
        primary: None,
        check: false,
        notes: false,
    };
    match state {
        State::Idle => View {
            primary: Some("Check"),
            ..plain("Not checked yet", format!("This is SJK {installed}."))
        },
        State::Checking => plain(
            "Checking for updates...",
            "Asking GitHub for the latest release.".to_owned(),
        ),
        State::UpToDate => View {
            primary: Some("Check again"),
            ..plain(
                "SJK is up to date",
                format!("Version {installed} is the latest release."),
            )
        },
        State::Available(release) => View {
            primary: Some("Install"),
            check: true,
            notes: true,
            ..plain(
                &format!("Update available: {}", release.version),
                format!(
                    "You have {installed}. Install downloads it, checks it and swaps the \
                     programs; your settings stay."
                ),
            )
        },
        State::Downloading { done, total } => View {
            progress: (*total > 0).then(|| (*done as f32 / *total as f32).clamp(0.0, 1.0)),
            ..plain(
                "Downloading the update...",
                if *total > 0 {
                    format!("{} of {}", megabytes(*done), megabytes(*total))
                } else {
                    megabytes(*done)
                },
            )
        },
        State::Installing => plain(
            "Installing...",
            "The download matched its checksum; replacing the programs.".to_owned(),
        ),
        State::Installed => View {
            primary: Some("Restart now"),
            ..plain(
                "Update installed",
                "SJK starts the new version when it exits. Restart now, or keep playing \
                 and restart later."
                    .to_owned(),
            )
        },
        State::Manual(release, reason) => View {
            primary: Some("Open release page"),
            check: true,
            notes: true,
            ..plain(
                &format!("Update available: {}", release.version),
                format!("SJK cannot install it here: {reason}. Download it from the release page."),
            )
        },
        State::Failed(message) => View {
            primary: Some("Try again"),
            ..plain("The update did not work", message.clone())
        },
        State::Unversioned => plain(
            "This is a development build",
            "Local builds have no release number to compare. Set cl_updateAs to a release \
             version to try the update check."
                .to_owned(),
        ),
    }
}

impl Panel {
    pub(crate) fn new() -> Self {
        Self {
            open: false,
            owns_console: false,
            ui: MenuCanvas::with_text_capacity(64),
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Show the page; `owns_console` when the console was closed before it.
    pub(crate) fn open(&mut self, owns_console: bool) {
        self.open = true;
        self.owns_console = owns_console;
    }

    /// Hide the page; returns whether it had opened the console.
    pub(crate) fn close(&mut self) -> bool {
        let owned = self.open && self.owns_console;
        self.open = false;
        self.owns_console = false;
        owned
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    /// What Enter does now.
    fn primary(&self) -> PanelAction {
        match update::state() {
            State::Available(_) => {
                update::install();
                PanelAction::None
            }
            State::Manual(release, _) => {
                update::open_page(&release.page);
                PanelAction::None
            }
            State::Installed => PanelAction::Quit,
            State::Idle | State::UpToDate | State::Failed(_) => PanelAction::Check,
            State::Checking
            | State::Downloading { .. }
            | State::Installing
            | State::Unversioned => PanelAction::None,
        }
    }

    fn notes(&self) {
        if let State::Available(release) | State::Manual(release, _) = update::state() {
            update::open_page(&release.page);
        }
    }

    pub(crate) fn handle_key(&mut self, event: &KeyEvent) -> PanelAction {
        if event.state != ElementState::Pressed {
            return PanelAction::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return PanelAction::None;
        };
        match key {
            KeyCode::Escape => PanelAction::Close,
            KeyCode::Enter | KeyCode::NumpadEnter => self.primary(),
            KeyCode::KeyC => PanelAction::Check,
            KeyCode::KeyN => {
                self.notes();
                PanelAction::None
            }
            _ => PanelAction::None,
        }
    }

    pub(crate) fn handle_pointer(&mut self, event: InputEvent) -> PanelAction {
        let Some(event) = self.ui.pointer(event) else {
            return PanelAction::None;
        };
        if event.kind != UiEventKind::Activate {
            return PanelAction::None;
        }
        match event.token {
            Some(BACK_TOKEN) => PanelAction::Close,
            Some(PRIMARY_TOKEN) => self.primary(),
            Some(CHECK_TOKEN) => PanelAction::Check,
            Some(NOTES_TOKEN) => {
                self.notes();
                PanelAction::None
            }
            _ => PanelAction::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_has_a_headline_and_only_idle_states_offer_a_check() {
        for state in [
            State::Idle,
            State::Checking,
            State::UpToDate,
            State::Downloading { done: 5, total: 10 },
            State::Installing,
            State::Installed,
            State::Failed("offline".to_owned()),
            State::Unversioned,
        ] {
            let view = view(&state, "2026.1005.1");
            assert!(
                !view.headline.is_empty() && !view.detail.is_empty(),
                "{state:?}"
            );
            assert!(!view.check && !view.notes, "{state:?}");
        }
        let busy = view(&State::Downloading { done: 5, total: 10 }, "x");
        assert_eq!(busy.progress, Some(0.5));
        assert!(busy.primary.is_none());
        assert_eq!(view(&State::Installed, "x").primary, Some("Restart now"));
    }
}
