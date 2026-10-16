//! The game log on this server (`G_LogPrintf`): each line printed on the console as a
//! dedicated server prints it, and appended to `g_log` in the home directory with the
//! level's stamp before it — opened as a level starts, closed as it ends.

use super::NativeGame;
use sjk_game_jka::game_log::{self, Who};
use std::io::Write;

/// `pers.guid` for a userinfo: its `ja_guid`, else `NOGUID`.
pub(super) fn guid(userinfo: &[u8]) -> Vec<u8> {
    sjk_protocol::info_value(userinfo, b"ja_guid")
        .filter(|value| !value.is_empty())
        .map_or_else(|| b"NOGUID".to_vec(), <[u8]>::to_vec)
}

impl NativeGame {
    /// `G_LogPrintf`: `line` on the console, and stamped in the log file if one is open.
    pub(super) fn log(&mut self, line: &str) {
        let _ = std::io::stdout().write_all(line.as_bytes());
        let level_time = self.last_frame_time.max(self.level_start_time);
        if let Some(file) = &mut self.log_file {
            let stamped = game_log::stamp(level_time, self.level_start_time) + line;
            // A log that cannot be written is not the match's concern.
            let _ = file.write_all(stamped.as_bytes());
        }
    }

    /// `G_InitGame`'s log: `g_log` opened for appending in the home directory (nothing
    /// without a name or a home), then the level's first two lines.
    pub(super) fn open_log(&mut self) {
        let name = String::from_utf8_lossy(self.cvars.string(b"g_log")).into_owned();
        let print = |text: String| {
            let _ = std::io::stdout().write_all(text.as_bytes());
        };
        self.log_file = None;
        if name.is_empty() {
            print("Not logging game events to disk.\n".to_owned());
        } else {
            match self.config_files.append_home(&name) {
                Some(file) => {
                    self.log_file = Some(file);
                    print(format!("Logging to {name}\n"));
                }
                None => print(format!("WARNING: Couldn't open logfile: {name}\n")),
            }
        }
        for line in game_log::init_game(&self.status_info.clone()) {
            self.log(&line);
        }
    }

    /// `G_ShutdownGame` as the server quits: the log's last line, the file closed.
    pub fn shut_down(&mut self) {
        self.close_log();
    }

    /// `G_ShutdownGame`'s log: the last line, and the file closed.
    pub(super) fn close_log(&mut self) {
        self.log(&game_log::shutdown_game());
        self.log_file = None;
    }

    /// Who a player is to the log: slot, address, guid and name.
    pub(super) fn log_who(&self, client: usize) -> Option<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        let peer = self.peer(client)?;
        // `ClientConnect`: a bot's guid is `BOT`.
        let guid = if peer.bot {
            b"BOT".to_vec()
        } else {
            guid(&peer.userinfo)
        };
        Some((peer.address.clone(), guid, peer.name.clone()))
    }

    /// A line about one player, built from who it is.
    pub(super) fn log_about(&mut self, client: usize, line: impl FnOnce(Who) -> String) {
        let Some((address, guid, name)) = self.log_who(client) else {
            return;
        };
        let text = line(Who {
            client,
            address: &address,
            guid: &guid,
            name: &name,
        });
        self.log(&text);
    }

    /// `player_die`'s line.
    pub(super) fn log_kill(&mut self, victim: usize, attacker: Option<u16>, means: u32) {
        let victim_name = self
            .peer(victim)
            .map(|peer| peer.name.clone())
            .unwrap_or_default();
        let attacker = attacker.map(|number| {
            (
                i32::from(number),
                self.peer(usize::from(number))
                    .map(|peer| peer.name.clone())
                    .unwrap_or_default(),
            )
        });
        let line = game_log::kill(
            attacker
                .as_ref()
                .map(|(number, name)| (*number, name.as_slice())),
            victim,
            &victim_name,
            means,
        );
        self.log(&line);
    }

    /// `LogExit`: why the match ended, the team scores in a team game, and every
    /// connected player in the game by rank, with its score and ping.
    pub(super) fn log_exit(&mut self, reason: &str) {
        self.log(&game_log::exit(reason));
        let team_game = self.gametype >= sjk_game_jka::match_end::GT_TEAM;
        if team_game {
            self.log(&game_log::team_scores(
                self.team_scores[0],
                self.team_scores[1],
            ));
        }
        let (sorted, _) = self.sorted();
        for client in sorted.into_iter().map(usize::from) {
            let Some(peer) = self
                .peer(client)
                .filter(|peer| peer.begun && peer.session.team != i32::from(super::TEAM_SPECTATOR))
            else {
                continue;
            };
            let score = peer.state.persistent[super::PERS_SCORE] as i32;
            let line = game_log::score(
                team_game.then_some(peer.session.team),
                score,
                peer.ping,
                &guid(&peer.userinfo),
                client,
                &peer.name,
            );
            self.log(&line);
        }
    }
}
