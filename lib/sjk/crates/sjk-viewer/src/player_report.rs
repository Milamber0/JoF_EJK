//! Player reports for the SJK team (`docs/identity.md`, "Player reports"): the game
//! menu's Players page lists everyone on the server ([`crate::ingame_menu::players`]);
//! choosing one opens the Report page, a reason opens the text dialog, and Send hands
//! the report to the identity service, which signs it with the player's key and sends
//! it to the hub (`POST /v1/player-report`).
//!
//! Only a verified SJK player may report; the pages say so to the others. The report
//! names the player as they were when chosen (name, slot, the key the hub's presence
//! list shows for them), the server's address and name, the map and the match clock;
//! the hub checks everything again and limits how often a key may report and how often
//! one player may be reported. The outcome comes back on the SJK UI's card while it
//! waits for it, else as a centre print.

use crate::ingame_menu::Page;
use crate::ingame_menu::players::{self, Action, Gate};
use crate::text_dialog::Report;
use sjk_client::ServerEventKind;
use sjk_identity::Category;
use sjk_protocol::InfoString;
use std::time::Instant;

/// `CS_LEVEL_START_TIME`: when the map started, in server milliseconds.
const CS_LEVEL_START_TIME: usize = 21;

/// `name` without its Quake colour codes (`^` and the character after it), for the
/// dialog, which draws text as written.
fn plain(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^' && chars.peek().is_some_and(|next| *next != '^') {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

impl crate::GpuState {
    /// Open the Players page: read the roster now and ask the server for fresh scores.
    pub(crate) fn open_players_page(&mut self) {
        self.in_game_menu.players.reset();
        self.open_game_menu_page(Page::Players);
        self.refresh_game_menu_players();
    }

    /// While the Players or Report page shows: read the roster again once a second, and
    /// ask for scores every two.
    pub(crate) fn refresh_game_menu_players(&mut self) {
        if !matches!(self.game_menu_page, Page::Players | Page::ReportPlayer) {
            return;
        }
        let now = Instant::now();
        if !self.in_game_menu.players.due(now) {
            return;
        }
        let Some(session) = self.live_session.as_ref() else {
            self.in_game_menu.players.clear(Gate::Local);
            return;
        };
        let gate = if session.is_local() {
            Gate::Local
        } else {
            crate::player_identity::report_gate()
        };
        let game = session.game_state();
        let you = u8::try_from(game.client_num).ok();
        self.in_game_menu.players.refresh(
            game,
            session.scores(),
            you,
            gate,
            crate::player_identity::hub_mark,
        );
        if self.game_menu_page == Page::Players && self.in_game_menu.players.scores_due(now) {
            self.send_menu_reliable("score");
        }
    }

    /// Enter on row `row` of the Players page.
    pub(crate) fn activate_players_row(&mut self, row: usize) {
        match self.in_game_menu.players.action(row) {
            Action::Report => {
                if self.in_game_menu.players.choose(row) {
                    self.game_menu_page = Page::ReportPlayer;
                    self.game_menu_row = self.in_game_menu.players.first_report_row();
                }
            }
            Action::More => {
                self.in_game_menu.players.next_page();
                self.game_menu_row = 0;
            }
            Action::Back => self.back_or_close_game_menu(),
            Action::None => {}
        }
    }

    /// Enter on row `row` of the Report page: a reason opens the text dialog, Back
    /// returns to the chosen player's row.
    pub(crate) fn activate_report_player_row(&mut self, row: usize) {
        match players::State::category(row) {
            Some(category) if self.in_game_menu.players.blocked().is_none() => {
                self.open_player_report_dialog(category);
            }
            Some(_) => {}
            None => self.back_to_players(),
        }
    }

    /// From the Report page back to the Players page, on the chosen player's row.
    pub(crate) fn back_to_players(&mut self) {
        self.game_menu_page = Page::Players;
        self.game_menu_row = self.in_game_menu.players.target_row();
    }

    /// Close the game menu and ask for the report's few words.
    fn open_player_report_dialog(&mut self, category: Category) {
        let Some(target) = self.in_game_menu.players.target() else {
            return;
        };
        let subject = format!("{}: {}", plain(&target.name), category.label());
        self.game_menu = false;
        self.game_menu_page = Page::Main;
        self.game_menu_row = 0;
        self.gameplay_input.release_keys();
        self.text_dialog
            .open(crate::text_dialog::Kind::PlayerReport { subject, category });
        self.sync_cursor_policy();
    }

    /// The dialog's Send: check the text and hand the report to the identity service.
    /// The outcome shows on the SJK UI's card while it waits for it, else as a centre
    /// print.
    pub(crate) fn send_player_report(&mut self, category: Category, text: &str) {
        let refused = |why: String| (format!("Player report not sent: {why}"), Some(why));
        let (message, failure) = match self.player_report(category, text) {
            Err(why) => refused(why),
            Ok(report) => {
                if crate::player_identity::player_report(report) {
                    self.in_game_menu.players.waiting = true;
                    ("Sending the player report...".to_owned(), None)
                } else {
                    refused(crate::bug_report::IDENTITY_OFF.to_owned())
                }
            }
        };
        let shown = match failure {
            Some(why) => self.text_dialog.answer(Report::Player, Err(why)),
            None => self.text_dialog.sending(Report::Player),
        };
        if !shown {
            self.chat
                .receive(ServerEventKind::CenterPrint, message, None, Instant::now());
        }
    }

    /// The report for the chosen player, with where and when, or why it cannot go.
    fn player_report(
        &self,
        category: Category,
        text: &str,
    ) -> Result<sjk_identity::PlayerReport, String> {
        let text = sjk_identity::report::player_text(text).map_err(str::to_owned)?;
        let players = &self.in_game_menu.players;
        if let Some(why) = players.blocked() {
            return Err(why.to_owned());
        }
        let target = players.target().ok_or("no player chosen")?;
        let session = self
            .live_session
            .as_ref()
            .filter(|session| !session.is_local())
            .ok_or("not on an online server")?;
        let game = session.game_state();
        let config = |index: usize| {
            game.config_string(index)
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
        };
        let server_name = config(0)
            .and_then(|text| InfoString::parse(text).ok())
            .and_then(|info| info.get("sv_hostname").map(str::to_owned))
            .unwrap_or_default();
        let level_time = config(CS_LEVEL_START_TIME)
            .and_then(|start| start.trim().parse::<i32>().ok())
            .map(|start| session.latest_snapshot().server_time.saturating_sub(start))
            .and_then(|millis| u32::try_from(millis / 1_000).ok());
        Ok(sjk_identity::PlayerReport {
            category,
            text,
            server: session.server().to_string(),
            // The hub takes a name of at most 64 characters without control codes.
            server_name: server_name
                .chars()
                .filter(|c| !c.is_control())
                .take(64)
                .collect(),
            slot: target.slot,
            target_name: target.name.clone(),
            target_key_id: target
                .hub
                .as_ref()
                .map(|hub| hub.key_id.clone())
                .unwrap_or_default(),
            map: sjk_identity::report::field(&self.world_load_map, 64),
            build: sjk_identity::report::field(crate::build_info::VERSION, 32),
            level_time,
            // The identity service adds the name the player wears.
            name: String::new(),
        })
    }

    /// Once a frame: show the outcome of a report sent with [`Self::send_player_report`].
    pub(crate) fn poll_player_report(&mut self) {
        if !self.in_game_menu.players.waiting {
            return;
        }
        let serial = self.in_game_menu.players.serial;
        let Some(outcome) = crate::player_identity::player_report_outcome()
            .filter(|outcome| outcome.serial != serial)
        else {
            return;
        };
        self.in_game_menu.players.serial = outcome.serial;
        self.in_game_menu.players.waiting = false;
        let message = if outcome.sent {
            format!(
                "Player report sent ({}). The SJK team will look at it.",
                outcome.message
            )
        } else {
            format!("Player report not sent: {}", outcome.message)
        };
        crate::log::progress(format_args!("{message}"));
        let answer = if outcome.sent {
            Ok(outcome.message)
        } else {
            Err(outcome.message)
        };
        if !self.text_dialog.answer(Report::Player, answer) {
            self.chat
                .receive(ServerEventKind::CenterPrint, message, None, Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_dialog_names_the_player_without_colour_codes() {
        assert_eq!(super::plain("^1Darth ^7Vulpes"), "Darth Vulpes");
        // `^^` is a plain `^`, as `Q_IsColorString` reads it; the second starts a code.
        assert_eq!(super::plain("a^^b"), "a^");
        assert_eq!(super::plain("end^"), "end^");
    }
}
