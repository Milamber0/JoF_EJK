//! `bot_minplayers` on this server (`g_bot.c`'s `G_CheckMinimumPlayers`): every ten
//! seconds of the level, while bots are enabled, a bot is added when the players are
//! fewer than the variable asks (`G_AddRandomBot`: a defined bot not yet playing, drawn
//! at random) and one removed when they are more (`G_RemoveRandomBot`: a spectating bot
//! first). Both go through the console, as the reference's `EXEC_INSERT` commands do:
//! `addbot` and `clientkick` run when the console is next drained.

use super::*;
use sjk_game_jka::client_begin::SPECTATOR_FOLLOW;

/// `Q_CleanStr`: colour codes (`^` and a digit) and unprintable bytes taken out.
fn clean_str(text: &[u8]) -> Vec<u8> {
    let mut clean = Vec::with_capacity(text.len());
    let mut at = 0;
    while let Some(&byte) = text.get(at) {
        if byte == 0 {
            break;
        }
        let next = text.get(at + 1).copied().unwrap_or(0);
        if byte == b'^' && next != 0 && next != b'^' && next.is_ascii_digit() {
            at += 1;
        } else if (0x20..=0x7E).contains(&byte) {
            clean.push(byte);
        }
        at += 1;
    }
    clean
}

impl NativeGame {
    /// `G_CheckMinimumPlayers`, which `G_CheckBotSpawn` runs first: not in siege nor at
    /// intermission, once in ten seconds of level time — a clock the reference keeps in a
    /// function's `static`, so a new level (its time starting over) waits for it.
    pub(super) fn check_minimum_players(&mut self, level_time: i32) {
        if self.gametype == GAMETYPE_SIEGE || self.match_end.intermission_time != 0 {
            return;
        }
        if self.bots.minimum_check_time > level_time.wrapping_sub(10_000) {
            return;
        }
        self.bots.minimum_check_time = level_time;
        let mut minimum = self.cvars.integer(b"bot_minplayers");
        if minimum <= 0 {
            return;
        }
        minimum = minimum.min(self.players.places() as i32);
        let (humans, bots) = (self.count_humans(-1), self.count_bots(-1, level_time));
        if humans + bots < minimum {
            self.add_random_bot(-1);
        } else if humans + bots > minimum
            && bots != 0
            && !self.remove_random_bot(i32::from(TEAM_SPECTATOR))
        {
            // No spectating bot: one that plays.
            self.remove_random_bot(-1);
        }
    }

    /// The begun clients on `team` (any for -1), bots or not.
    fn begun_on(&self, team: i32, bot: bool) -> impl Iterator<Item = (usize, &Peer)> {
        (0..self.players.places()).filter_map(move |client| {
            let peer = self.peer(client)?;
            (peer.begun && peer.bot == bot && (team < 0 || peer.session.team == team))
                .then_some((client, peer))
        })
    }

    /// `G_CountHumanPlayers`.
    fn count_humans(&self, team: i32) -> i32 {
        self.begun_on(team, false).count() as i32
    }

    /// `G_CountBotPlayers`: the bots playing, and the queued ones whose time has come.
    fn count_bots(&self, team: i32, level_time: i32) -> i32 {
        let queued = self
            .bots
            .queue
            .iter()
            .filter(|(time, _)| *time != 0 && *time <= level_time)
            .count();
        (self.begun_on(team, true).count() + queued) as i32
    }

    /// `G_AddRandomBot`: of the defined bots none playing on `team` goes by the name of,
    /// one drawn at random (`Q_flrand`) is added at `g_npcspskill`.
    fn add_random_bot(&mut self, team: i32) {
        let playing: Vec<Vec<u8>> = self
            .begun_on(team, true)
            .map(|(_, peer)| peer.name.clone())
            .collect();
        let names: Vec<Vec<u8>> = self
            .bots
            .roster
            .infos()
            .map(|info| {
                sjk_protocol::info_value(info, b"name")
                    .unwrap_or_default()
                    .to_vec()
            })
            .collect();
        let free = |name: &[u8]| {
            !playing
                .iter()
                .any(|playing| playing.eq_ignore_ascii_case(name))
        };
        let count = names.iter().filter(|name| free(name)).count() as i32;
        let mut left = (self.deaths.rng.flrand(0.0, 1.0) * count as f32) as i32;
        for name in names.iter().filter(|name| free(name)) {
            left -= 1;
            if left > 0 {
                continue;
            }
            let skill = self.cvars.integer(b"g_npcspskill") as f32;
            let side = match team {
                1 => "red",
                2 => "blue",
                _ => "",
            };
            // `Q_strncpyz` into 36 bytes, then `Q_CleanStr`.
            let name = clean_str(&name[..name.len().min(35)]);
            let command = format!(
                "addbot \"{}\" {skill:.2} {side} 0\n",
                String::from_utf8_lossy(&name)
            );
            self.commands.insert_text(command.as_bytes(), &mut console);
            return;
        }
    }

    /// `G_RemoveRandomBot`: the first bot on `team` (any for -1) not following someone as
    /// a spectator is kicked. Whether one was.
    fn remove_random_bot(&mut self, team: i32) -> bool {
        let found = self
            .begun_on(team, true)
            .find(|(_, peer)| {
                !(peer.session.team == i32::from(TEAM_SPECTATOR)
                    && peer.session.spectator_state == SPECTATOR_FOLLOW)
            })
            .map(|(client, _)| client);
        let Some(client) = found else { return false };
        self.commands
            .insert_text(format!("clientkick {client}\n").as_bytes(), &mut console);
        true
    }
}
