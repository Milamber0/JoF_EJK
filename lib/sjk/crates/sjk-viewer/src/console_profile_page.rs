//! Console side of the Profile page (see `profile_panel.rs`): opening it on a tab,
//! saving the bio it asks to save, and routing keys and pointer events to it while it
//! is open, as for the Identity page. It is drawn in the SJK UI's look whatever the
//! menu style (`console_sjk_pages.rs`).

use super::profile_panel::{Inputs, Mode, PanelAction};
use super::*;
use crate::player_menu::ReturnTarget;
use crate::profile_hub::{Screen, Tab as HubTab};
use sjk_ui::InputEvent;

/// The `profile` command.
pub(crate) const PROFILE_COMMAND: &str = "profile";
pub(crate) const PROFILE_HELP: &str = "Open your SJK profile: picture, bio and record";

impl ViewerConsole {
    /// `profile` in the classic menus: show the page in front of the console, opening
    /// it if needed, or close it when it already shows.
    pub(crate) fn toggle_profile_panel(&mut self) {
        if self.open && self.profile_panel.is_open() {
            self.close_profile_panel();
            return;
        }
        self.open_profile_panel();
    }

    /// Show the page on its own (the `profile` command in the classic menus).
    pub(crate) fn open_profile_panel(&mut self) {
        let owns_console = !self.open;
        self.show_profile_panel(owns_console, Mode::Pages);
    }

    /// Show the page as `mode` says; closing it closes the console too when
    /// `owns_console`.
    fn show_profile_panel(&mut self, owns_console: bool, mode: Mode) {
        if !self.open {
            self.set_open(true);
        }
        self.browser.close();
        self.debug_panel.close();
        self.changelog.close();
        self.update_panel.close();
        self.config_import.close();
        self.credits.close();
        self.identity_panel.close();
        self.staff_panel.close();
        self.sjk_chat_panel.close();
        self.collection_panel.close();
        self.holocrons_panel.close();
        self.dead_key.settle();
        self.profile_panel.open_as(owns_console, mode);
    }

    /// The tab of the Profile or Collection screen the console shows
    /// ([`crate::profile_hub`]): its Profile page as the SJK Profile tab, or its
    /// Collection page as the Collection screen.
    pub(crate) fn profile_hub_tab(&self) -> Option<HubTab> {
        if !self.open {
            None
        } else if self.profile_panel.is_open() && self.profile_panel.mode() == Mode::Hub {
            Some(HubTab::Profile)
        } else if self.collection_panel.is_open() && self.collection_panel.is_hub() {
            Some(self.collection_panel.tab())
        } else if self.holocrons_panel.is_open() && self.holocrons_panel.is_hub() {
            Some(HubTab::Holocrons)
        } else {
            None
        }
    }

    /// Where the screen the console shows returns.
    pub(crate) fn profile_hub_return(&self) -> ReturnTarget {
        if self.collection_panel.is_open() && self.collection_panel.is_hub() {
            self.collection_panel.back()
        } else {
            self.profile_hub_return
        }
    }

    /// Show tab `tab` of the Profile or Collection screen (one the console shows) in
    /// place of the console, returning to `target`. When another of the screens' pages
    /// showed here, the console still closes with it if that one had opened it; between
    /// the Collection page's own tabs the page only turns. The Collection's Holocrons is
    /// the Holocrons page.
    pub(crate) fn open_profile_hub_page(&mut self, tab: HubTab, target: ReturnTarget) {
        if tab.player_page().is_some() {
            // The player screen's pages are not the console's.
            return;
        }
        let current = self.profile_hub_tab();
        let on_page = |tab: HubTab| tab.screen() == Screen::Collection && tab != HubTab::Holocrons;
        if on_page(tab) && current.is_some_and(on_page) {
            self.collection_panel.show(tab);
            self.holocrons_last = false;
            return;
        }
        if current == Some(tab) {
            return;
        }
        let owns_console = if current.is_some() {
            let profile = self.profile_panel.close();
            let collection = self.collection_panel.close();
            let holocrons = self.holocrons_panel.close();
            profile || collection || holocrons
        } else {
            !self.open
        };
        match tab {
            HubTab::Profile => {
                self.profile_hub_return = target;
                self.show_profile_panel(owns_console, Mode::Hub);
            }
            HubTab::Holocrons => {
                self.profile_hub_return = target;
                self.show_holocrons_panel(owns_console);
                self.holocrons_panel.set_hub(true);
                self.holocrons_last = true;
            }
            _ => {
                self.show_collection_panel(tab, owns_console, true, target);
                self.holocrons_last = false;
            }
        }
    }

    /// Close the screens' page the console shows (for the player screen's tabs), and
    /// the console with it when the page had opened it.
    pub(crate) fn close_profile_hub_page(&mut self) {
        if self.profile_hub_tab().is_none() {
            return;
        }
        let profile = self.profile_panel.close();
        let collection = self.collection_panel.close();
        let holocrons = self.holocrons_panel.close();
        if profile || collection || holocrons {
            self.set_open(false);
        }
    }

    fn close_profile_panel(&mut self) {
        if self.profile_panel.close() {
            self.set_open(false);
        }
    }

    fn profile_panel_action(&mut self, action: PanelAction) {
        match action {
            PanelAction::None => {}
            PanelAction::Close => self.close_profile_panel(),
            PanelAction::Staff => {
                let owns_console = self.profile_panel.close();
                self.open_staff_panel();
                if owns_console {
                    self.staff_panel_owns_console();
                }
            }
            PanelAction::Hub(tab) if self.profile_panel.mode() == Mode::Hub => {
                let target = self.profile_hub_return;
                self.open_profile_hub_page(tab, target);
            }
            PanelAction::Hub(tab) => {
                // On its own the page hands over to the Collection page on its own.
                let owns_console = self.profile_panel.close();
                self.show_collection_panel(tab, owns_console, false, ReturnTarget::MainMenu);
            }
            PanelAction::Save { bio } => {
                // The page shows the hub's answer, or why nothing could be sent.
                if !crate::player_identity::set_bio(bio) {
                    self.push_log("^3profile: the identity is not running yet");
                }
            }
            PanelAction::SetAvatar { png } => {
                if !crate::player_identity::set_avatar(png) {
                    self.profile_panel
                        .picture_failed("Not sent: turn the SJK identity on first (cl_identity 1)");
                }
            }
            PanelAction::RemoveAvatar => {
                if !crate::player_identity::remove_avatar() {
                    self.profile_panel
                        .picture_failed("Not sent: turn the SJK identity on first (cl_identity 1)");
                }
            }
        }
    }

    /// Whether the Profile page shows.
    pub(crate) fn profile_panel_shown(&self) -> bool {
        self.open && self.profile_panel.is_open()
    }

    /// Open the Profile page if needed, on its picture panel.
    pub(crate) fn profile_show_picture(&mut self) {
        if !self.profile_panel_shown() {
            self.open_profile_panel();
        }
        self.profile_panel.show_picture();
    }

    /// Read the picture file at `path` and show it on the Profile page, ready to send
    /// (a file dropped on the window, `sjkavatar <file>`).
    pub(crate) fn profile_load_picture(&mut self, path: &std::path::Path) {
        if !self.profile_panel_shown() {
            self.open_profile_panel();
        }
        self.profile_panel.load_picture(path.to_owned());
    }

    /// Take the player's picture down (`sjkavatar clear`), on the Profile page.
    pub(crate) fn profile_remove_picture(&mut self) {
        if !self.profile_panel_shown() {
            self.open_profile_panel();
        }
        let action = self.profile_panel.remove_picture_now();
        self.profile_panel_action(action);
    }

    /// The identity service's last notice, to tell a save's answer.
    fn profile_notice() -> Option<String> {
        crate::player_identity::snapshot().and_then(|snapshot| snapshot.notice)
    }

    /// Give a pressed key to the open page; false when the page is closed.
    pub(super) fn profile_panel_key(&mut self, event: &KeyEvent) -> bool {
        if !self.profile_panel.is_open() {
            return false;
        }
        let action =
            self.profile_panel
                .handle_key(event, self.shift, self.control, Self::profile_notice());
        self.profile_panel_action(action);
        true
    }

    /// Give a pointer event to the open page; false when the page is closed.
    pub(super) fn profile_panel_pointer(&mut self, event: InputEvent) -> bool {
        if !self.profile_panel.is_open() {
            return false;
        }
        let action = self
            .profile_panel
            .handle_pointer(event, Self::profile_notice());
        self.profile_panel_action(action);
        true
    }

    /// Draw the page with its text to `target`.
    pub(super) fn append_profile_panel(
        &mut self,
        target: crate::menu::sjk::TextTarget<'_>,
        viewport: [f32; 2],
    ) {
        if self.profile_panel.mode() == Mode::Hub {
            let back = self.profile_hub_return;
            let name = self.text_value("name").unwrap_or_default();
            if !self.profile_panel.hub_header().is(back, name) {
                let header = crate::profile_hub::Header {
                    back,
                    name: name.to_owned(),
                };
                self.profile_panel.set_hub_header(header);
            }
        }
        #[cfg(test)]
        if let Some(preview) = self.profile_panel.preview.take() {
            let inputs = Inputs {
                enabled: true,
                snapshot: Some(&preview.snapshot),
                standings: &preview.standings,
                record: &preview.record,
            };
            self.profile_panel.append_sjk(&inputs, target, viewport);
            self.profile_panel.preview = Some(preview);
            return;
        }
        // Copied out so the page can borrow itself mutably while it draws.
        let snapshot =
            crate::player_identity::collection_snapshot(crate::debug_collectibles::wanted(self));
        let held = snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.me.as_ref())
            .map(|me| me.achievements.clone())
            .unwrap_or_default();
        let standings = crate::achievements::standings(&held);
        let record = record();
        let inputs = Inputs {
            enabled: self.bool_cvar("cl_identity") == Some(true),
            snapshot: snapshot.as_ref(),
            standings: &standings,
            record: &record,
        };
        self.profile_panel.append_sjk(&inputs, target, viewport);
    }

    /// The world shot's made-up identity, for the Collection page too.
    #[cfg(test)]
    pub(super) fn profile_preview_snapshot(&self) -> Option<sjk_identity::Snapshot> {
        self.profile_panel
            .preview
            .as_ref()
            .map(|preview| preview.snapshot.clone())
    }

    /// The world shot's made-up achievements, for the Collection page too.
    #[cfg(test)]
    pub(super) fn profile_preview_standings(&self) -> Option<Vec<crate::achievements::Standing>> {
        self.profile_panel
            .preview
            .as_ref()
            .map(|preview| preview.standings.clone())
    }

    /// Show `preview` in place of the live identity and counts, for a world shot.
    #[cfg(test)]
    pub(crate) fn preview_profile(&mut self, preview: super::profile_panel::Preview) {
        self.profile_panel.preview = Some(preview);
    }

    /// Type `text` into the open page's bio, for a world shot.
    #[cfg(test)]
    pub(crate) fn profile_type_bio(&mut self, text: &str) {
        self.profile_panel.type_for_shot(text);
    }

    /// The page's draw list while it is shown.
    pub(super) fn profile_panel_draw_list(&self) -> Option<&sjk_ui::DrawList> {
        (self.open && self.profile_panel.is_open()).then(|| self.profile_panel.draw_list())
    }
}

/// The player's record as the Profile tab lists it.
fn record() -> Vec<(&'static str, String)> {
    use crate::achievements::{Counter, count};
    let minutes = count(Counter::Minutes);
    vec![
        ("Players defeated", count(Counter::Kills).to_string()),
        ("Saber kills", count(Counter::SaberKills).to_string()),
        ("Best streak", count(Counter::BestStreak).to_string()),
        ("Duels won", count(Counter::DuelWins).to_string()),
        ("Flags captured", count(Counter::Captures).to_string()),
        ("Maps played", count(Counter::Maps).to_string()),
        ("Servers played", count(Counter::Servers).to_string()),
        (
            "Time played",
            format!("{} h {:02} min", minutes / 60, minutes % 60),
        ),
    ]
}
