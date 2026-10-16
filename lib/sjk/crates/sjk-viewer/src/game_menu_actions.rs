//! In-game menu actions kept out of the renderer/event-loop module.

use super::*;

impl GpuState {
    pub(crate) fn select_team(&mut self, team: sjk_client::LegacyTeamChoice) {
        let command = sjk_client::legacy_team_command(team);
        if let Some(console) = &mut self.console {
            console.note_team_choice(team);
        }
        if let Some(session) = &mut self.live_session {
            match session.send_reliable_command(command) {
                Ok(()) => {
                    self.game_menu = false;
                    self.capture_pointer();
                }
                Err(error) => eprintln!("failed to select team: {error}"),
            }
        }
    }

    pub(crate) fn game_menu_row_count(&self) -> usize {
        self.in_game_menu
            .row_count(self.game_menu_page, self.is_team_game())
    }

    pub(crate) fn back_or_close_game_menu(&mut self) {
        if self.in_game_menu.is_sjk() {
            // Back to the entry that opened the page.
            self.sjk_back();
        } else if self.game_menu_page == GameMenuPage::ReportPlayer {
            self.back_to_players();
        } else if matches!(self.game_menu_page, GameMenuPage::Main | GameMenuPage::Shot) {
            self.game_menu = false;
            self.capture_pointer();
        } else if self.game_menu_page.is_vote_page()
            && self.game_menu_page != GameMenuPage::CallVote
        {
            self.game_menu_page = GameMenuPage::CallVote;
            self.game_menu_row = 0;
        } else {
            // The classic bar keeps focus on the button whose pop-up closed.
            let tab = ingame_menu::classic::Tab::of_page(self.game_menu_page)
                .filter(|_| self.in_game_menu.is_classic());
            self.game_menu_page = GameMenuPage::Main;
            self.game_menu_row = tab.map_or(0, ingame_menu::classic::Tab::index);
        }
    }

    pub(crate) fn activate_game_menu_row(&mut self) {
        if self.in_game_menu.is_classic() && self.activate_classic_row() {
            return;
        }
        if self.in_game_menu.is_sjk() && self.activate_sjk_ui_row() {
            return;
        }
        // Both looks take the main and Leave pages' rows themselves.
        match self.game_menu_page {
            GameMenuPage::Team => self.activate_team_row(),
            GameMenuPage::Siege => match self.in_game_menu.siege_action(self.game_menu_row) {
                ingame_menu::CallVoteAction::Send(command) => {
                    let unchanged = self.live_session.as_ref().is_some_and(|session| {
                        ingame_menu::siege_data::unchanged_choice(session.game_state(), &command)
                    });
                    if !unchanged {
                        self.send_menu_reliable(&command);
                    }
                    self.game_menu = false;
                    self.capture_pointer();
                }
                ingame_menu::CallVoteAction::Back => self.back_or_close_game_menu(),
                _ => self.game_menu_row = 0,
            },
            GameMenuPage::About => self.open_game_menu_page(GameMenuPage::Main),
            GameMenuPage::Sjk => self.activate_sjk_row(),
            GameMenuPage::Players => self.activate_players_row(self.game_menu_row),
            GameMenuPage::ReportPlayer => self.activate_report_player_row(self.game_menu_row),
            page if page.is_vote_page() => self.activate_callvote_row(page),
            _ => {}
        }
    }

    /// An entry of the SJK pop-up ([`ingame_menu::sjk`]); past them, Back.
    pub(crate) fn activate_sjk_row(&mut self) {
        match self.game_menu_row {
            ingame_menu::sjk::CHANGELOG => {
                // Drawn by the console over the game menu, which shows again
                // when the page closes.
                if let Some(console) = &mut self.console {
                    console.open_changelog();
                }
                self.sync_cursor_policy();
            }
            ingame_menu::sjk::CREDITS => {
                if let Some(console) = &mut self.console {
                    console.open_credits();
                }
                self.sync_cursor_policy();
            }
            ingame_menu::sjk::PROFILE => {
                if let Some(console) = &mut self.console {
                    console.open_profile_panel();
                }
                self.sync_cursor_policy();
            }
            ingame_menu::sjk::IDENTITY => {
                if let Some(console) = &mut self.console {
                    console.open_identity_panel();
                }
                self.sync_cursor_policy();
            }
            ingame_menu::sjk::REPORT => self.open_bug_report(),
            ingame_menu::sjk::REPORT_PLAYER => self.open_players_page(),
            ingame_menu::sjk::SJK_CHAT => {
                if let Some(console) = &mut self.console {
                    console.open_sjk_chat_panel();
                }
                self.sync_cursor_policy();
            }
            _ => self.back_or_close_game_menu(),
        }
    }

    /// The leave page doubles as the confirmation: each entry is the action.
    fn activate_team_row(&mut self) {
        let team = if self.is_team_game() {
            [
                Some(sjk_client::LegacyTeamChoice::Auto),
                Some(sjk_client::LegacyTeamChoice::Red),
                Some(sjk_client::LegacyTeamChoice::Blue),
                Some(sjk_client::LegacyTeamChoice::Spectator),
                None,
            ]
            .get(self.game_menu_row)
            .copied()
            .flatten()
        } else {
            [
                Some(sjk_client::LegacyTeamChoice::Free),
                Some(sjk_client::LegacyTeamChoice::Spectator),
                None,
            ]
            .get(self.game_menu_row)
            .copied()
            .flatten()
        };
        if let Some(team) = team {
            self.select_team(team);
        } else {
            self.open_game_menu_page(GameMenuPage::Main);
        }
    }

    fn activate_callvote_row(&mut self, page: GameMenuPage) {
        match self.in_game_menu.callvote_action(page, self.game_menu_row) {
            ingame_menu::CallVoteAction::Open(page) => self.open_game_menu_page(page),
            ingame_menu::CallVoteAction::Back => self.back_or_close_game_menu(),
            ingame_menu::CallVoteAction::Send(command) => {
                self.send_menu_reliable(&command);
                self.open_game_menu_page(GameMenuPage::Main);
            }
            ingame_menu::CallVoteAction::None => {}
        }
    }

    /// Hand over to the classic Setup or Controls pop-up (`page`), on its
    /// first group; it returns to the game menu when closed.
    pub(crate) fn open_classic_panel_from_game(
        &mut self,
        page: crate::menu::classic::layout::Page,
    ) {
        if let (Some(menu), Some(console)) = (&mut self.client_menu, &self.console) {
            menu.open_classic_panel_from_game(console, page);
            self.game_menu = false;
        }
    }

    /// Hand over to the shell's server browser (japro's in-game entry); it
    /// returns to the game menu when closed, and a join leaves this server.
    pub(crate) fn open_browser_from_game(&mut self) {
        if let (Some(menu), Some(console)) = (&mut self.client_menu, &self.console) {
            menu.open_browser_from_game(console);
            menu.refresh_servers_if_stale();
            self.game_menu = false;
        }
    }

    pub(crate) fn is_team_game(&self) -> bool {
        self.live_session
            .as_ref()
            .and_then(|session| session.game_state().config_string(0))
            .and_then(|info| std::str::from_utf8(info).ok())
            .and_then(|info| InfoString::parse(info).ok())
            .and_then(|info| info.get_i32("g_gametype"))
            .is_some_and(|gametype| gametype >= 6)
    }

    pub(crate) fn vote_active(&self) -> bool {
        let Some(session) = &self.live_session else {
            return false;
        };
        let time = session.latest_snapshot().server_time;
        sjk_client::legacy_global_vote(session.game_state(), time).active
            || sjk_client::legacy_team_vote(
                session.game_state(),
                session.latest_snapshot().player.team(),
                time,
            )
            .is_some_and(|vote| vote.active)
    }

    pub(crate) fn send_vote(&mut self, yes: bool) {
        self.send_menu_reliable(if yes { "vote yes" } else { "vote no" });
    }

    pub(crate) fn send_menu_reliable(&mut self, command: &str) {
        if let Some(session) = &mut self.live_session
            && let Err(error) = session.send_reliable_command(command.as_bytes())
        {
            eprintln!("failed to send menu command: {error}");
        }
    }

    pub(crate) fn open_game_menu_page(&mut self, page: GameMenuPage) {
        self.game_menu_page = page;
        self.game_menu_row = 0;
    }

    /// Open the map's selector only in Siege; absent data is reported, never fabricated.
    pub(crate) fn open_siege_classes(&mut self) -> bool {
        let game = self.live_session.as_ref().map(ClientSession::game_state);
        if !ingame_menu::siege_data::is_siege(game) {
            return false;
        }
        let result = match (game, self.vfs.as_deref()) {
            (Some(game), Some(vfs)) => self.in_game_menu.refresh_siege(
                game,
                vfs,
                ingame_menu::siege_data::local_team(game),
            ),
            _ => Err("Siege assets are not available yet".into()),
        };
        match result {
            Ok(()) => {
                self.release_pointer();
                self.game_menu = true;
                self.open_game_menu_page(GameMenuPage::Siege);
            }
            Err(error) => {
                crate::log::progress(format_args!("Siege class selection: {error}"));
                if let Some(console) = &mut self.console {
                    console.push_log(error);
                }
            }
        }
        true
    }
}
