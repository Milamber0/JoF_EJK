//! `SetTeam`'s execution, and the duel's tournament on the native server
//! (`sjk_game_jka::tournament`): the line of spectators, the next duellist brought in
//! each frame one is missing (`CheckTournament`), a round counted as its intermission
//! begins (`AdjustTournamentScores`), the loser sent to the back and the next one
//! brought in two seconds later, the next round four seconds in (`map_restart 0`), a
//! duellist who leaves losing the round, and the tournament ending at `duel_fraglimit`.

use super::{GAMETYPE_DUEL, NativeGame, TEAM_SPECTATOR, Told};
use sjk_game_jka::client_begin::{Sides, TeamCommand, set_team};
use sjk_game_jka::event_entity::EventEntity;
use sjk_game_jka::player_death::DeathRequest;
use sjk_game_jka::ranks::calculate_ranks_in;
use sjk_game_jka::tournament::{self, Duellist, Waiting};

/// `CS_CLIENT_DUELISTS`, `CS_CLIENT_DUELWINNER`.
const CS_CLIENT_DUELISTS: usize = 30;
const CS_CLIENT_DUELWINNER: usize = 29;
/// `persistant[PERS_SCORE]`, `stats[STAT_ARMOR]`.
const PERS_SCORE: usize = 0;
const STAT_ARMOR: usize = 5;

impl NativeGame {
    /// `level.numNonSpectatorClients`: every client in a slot, begun or not, on a team.
    pub(super) fn non_spectators(&self) -> i32 {
        (0..self.players.places())
            .filter(|client| {
                self.peer(*client)
                    .is_some_and(|peer| peer.session.team != i32::from(TEAM_SPECTATOR))
            })
            .count() as i32
    }

    /// `G_InitSessionData`'s team outside team games: a willing spectator (`team s…` in
    /// the userinfo) spectates; a duel already full puts the newcomer in line; anyone
    /// else plays. Team games keep the free team the begin sorts out.
    pub(super) fn initial_team(&self, userinfo: &[u8]) -> i32 {
        if self.gametype >= super::GAMETYPE_TEAM {
            return 0;
        }
        let willing = sjk_protocol::info_value(userinfo, b"team")
            .is_some_and(|value| value.first() == Some(&b's'));
        // A power duel's newcomers all wait in line (`G_InitSessionData`).
        if willing
            || (self.gametype == GAMETYPE_DUEL && self.non_spectators() >= 2)
            || self.gametype == super::GAMETYPE_POWERDUEL
        {
            i32::from(TEAM_SPECTATOR)
        } else {
            0
        }
    }

    /// `AddTournamentQueue`: `client` to the back of the line.
    pub(super) fn queue_at_back(&mut self, client: usize) {
        let handles: Vec<_> = self
            .players
            .holders()
            .enumerate()
            .filter_map(|(number, handle)| Some((number, handle?)))
            .collect();
        let Some(world) = self.server.world_mut(self.world) else {
            return;
        };
        for (number, handle) in handles {
            if let Some(peer) = world.entity_mut(handle) {
                tournament::add_to_queue([(number, &mut peer.session)], client);
            }
        }
    }

    /// What `PickTeam` weighs when a player asks for a team game without naming a side:
    /// how many are on each, the asking client excluded, and what each has scored.
    pub(super) fn sides(&mut self, ignore: usize) -> Sides {
        let mut sides = Sides {
            red_score: self.team_scores[0],
            blue_score: self.team_scores[1],
            non_spectators: self.non_spectators(),
            power_duel_full: self.power_duel_full(ignore),
            ..Sides::default()
        };
        for other in 0..self.players.places() {
            if other == ignore {
                continue;
            }
            let Some(peer) = self.peer_mut(other) else {
                continue;
            };
            match peer.session.team {
                1 => sides.red_players += 1,
                2 => sides.blue_players += 1,
                _ => {}
            }
        }
        sides
    }

    /// `SetTeam(ent, wanted)` as the game calls it itself.
    pub(super) fn set_team_to(&mut self, client: usize, wanted: &[u8], server_time: i32) {
        let (gametype, sides) = (self.gametype, self.sides(client));
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let (before, name) = (peer.session.team, peer.name.clone());
        if let TeamCommand::Changed {
            announcement,
            queued,
        } = set_team(
            &mut peer.session,
            &name,
            wanted,
            gametype,
            sides,
            server_time,
        ) {
            self.team_changed(client, before, announcement, queued, server_time);
        }
    }

    /// `SetTeam`'s execution (`g_cmds.c:863-923`): a player leaving its team's world
    /// leaves its body if it was dead, is killed (a suicide at health 0) if it was not,
    /// and flashes out where it stood; one going to the spectators goes to the back of
    /// the line; everyone is told; `ClientUserinfoChanged`; then `ClientBegin` again.
    pub(super) fn team_changed(
        &mut self,
        client: usize,
        before: i32,
        announcement: Option<Vec<u8>>,
        queued: bool,
        server_time: i32,
    ) {
        self.team_changed_unbegun(client, before, announcement, queued, server_time);
        self.begin(client, server_time, None);
    }

    /// [`Self::team_changed`] without its closing `ClientBegin`, as `SetTeam` runs with
    /// `g_preventTeamBegin` set (`Cmd_SiegeClass_f` begins the player itself, or not).
    pub(super) fn team_changed_unbegun(
        &mut self,
        client: usize,
        before: i32,
        announcement: Option<Vec<u8>>,
        queued: bool,
        server_time: i32,
    ) {
        let playing = before != i32::from(TEAM_SPECTATOR);
        if playing && let Some(peer) = self.peer_mut(client) {
            if peer.health <= 0 {
                self.leave_body(client, server_time);
            } else {
                // `SetTeam` (`g_cmds.c:815`) sets the health and its stat first.
                let (origin, sounds) = (peer.state.origin(), peer.saber_off_sounds());
                peer.health = 0;
                peer.state.stats[0] = 0;
                self.die(
                    client,
                    DeathRequest::suicide(server_time, client as u16, origin, sounds, 0),
                );
            }
        }
        if queued {
            self.queue_at_back(client);
        }
        self.votes_follow_team_change(client, before);
        self.told.extend(announcement.map(Told::Everyone));
        // `BroadcastTeamChange`'s log line; a siege server announces nothing.
        if self.gametype != super::GAMETYPE_SIEGE {
            let after = self.peer(client).map_or(before, |peer| peer.session.team);
            self.log_about(client, |who| {
                sjk_game_jka::game_log::change_team(who, before, after)
            });
        }
        if playing && let Some(peer) = self.peer_mut(client) {
            let flash = EventEntity::teleport_out(peer.state.origin(), client as u16);
            let _ = self.pool.spawn_temporary(flash.state(), server_time, None);
        }
        // `ClientUserinfoChanged` before the begin, with its snaps advice.
        if let Some(advice) = self
            .peer(client)
            .and_then(|peer| self.snaps_advice(&peer.userinfo))
        {
            self.told.push(Told::One(client, advice));
        }
    }

    /// `level.sortedClients` and `level.numPlayingClients`, as `CalculateRanks` last
    /// left them.
    pub(super) fn sorted(&self) -> (Vec<u16>, usize) {
        let ranks = calculate_ranks_in(&self.contenders(), self.standings());
        (ranks.sorted, ranks.playing)
    }

    /// A configstring set as `SV_SetConfigstring` sets it: told only when it changes.
    pub(super) fn set_config_string(&mut self, index: usize, value: &[u8]) {
        let known = self
            .config_strings
            .iter()
            .find(|(known, _)| *known == index)
            .map(|(_, value)| value.as_slice());
        if known.unwrap_or_default() != value {
            self.publish_config_string(index, value);
        }
    }

    /// `CheckTournament` in a duel (`g_main.c:2271-2320`): the duellists named, and a
    /// spectator brought in while fewer than two play and no intermission is under way.
    pub(super) fn check_tournament(&mut self, server_time: i32) {
        if self.gametype != GAMETYPE_DUEL {
            // The duels have no warmup ("It seems we have decided there will be no
            // warmup in duel"); every other game type runs its own.
            if self.power_duel_on() {
                self.check_power_duel(server_time);
                self.limits.warmup_time = 0;
            } else {
                self.check_warmup(server_time);
            }
            return;
        }
        self.limits.warmup_time = 0;
        let (sorted, playing) = self.sorted();
        if playing >= 2 {
            self.set_config_string(
                CS_CLIENT_DUELISTS,
                &tournament::duelists_string(usize::from(sorted[0]), usize::from(sorted[1])),
            );
        }
        if playing < 2 && !self.match_end.ending() {
            self.add_tournament_player(server_time);
            let (sorted, playing) = self.sorted();
            if playing >= 2 {
                self.set_config_string(
                    CS_CLIENT_DUELISTS,
                    &tournament::duelists_string(usize::from(sorted[0]), usize::from(sorted[1])),
                );
            }
        }
    }

    /// `AddTournamentPlayer`: the longest waiting spectator plays, while fewer than two do.
    fn add_tournament_player(&mut self, server_time: i32) {
        if self.sorted().1 >= 2 {
            return;
        }
        let waiting: Vec<Waiting> = (0..self.players.places())
            .filter_map(|client| {
                let peer = self.peer(client)?;
                Some(Waiting {
                    client,
                    connected: peer.begun,
                    spectator: peer.session.team == i32::from(TEAM_SPECTATOR),
                    spectator_num: peer.session.spectator_num,
                    // `g_allowHighPingDuelist` is on by default.
                    lagging: false,
                    scoreboard: peer.session.spectator_state
                        == sjk_game_jka::client_begin::SPECTATOR_SCOREBOARD
                        || peer.session.spectator_client < 0,
                })
            })
            .collect();
        if let Some(next) = tournament::next_in_line(waiting) {
            self.set_team_to(next, b"f", server_time);
        }
    }

    /// The two sorted first, as a round's end reads them.
    fn duellists(&self) -> Option<(Duellist, Duellist, usize)> {
        let (sorted, playing) = self.sorted();
        let duellist = |client: u16| {
            let peer = self.peer(usize::from(client))?;
            Some(Duellist {
                client: usize::from(client),
                connected: peer.begun,
                score: peer.state.persistent[PERS_SCORE] as i32,
                vitality: peer.state.stats[0] as i32 + peer.state.stats[STAT_ARMOR] as i32,
            })
        };
        Some((
            duellist(*sorted.first()?)?,
            duellist(*sorted.get(1)?)?,
            playing,
        ))
    }

    /// In a duel, the other of the two sorted first, for `client`'s death.
    pub(super) fn duel_opponent(&self, client: usize) -> Option<u16> {
        if self.gametype != GAMETYPE_DUEL {
            return None;
        }
        let (sorted, _) = self.sorted();
        match (sorted.first().copied(), sorted.get(1).copied()) {
            (Some(first), Some(second)) if usize::from(first) == client => Some(second),
            (Some(first), Some(second)) if usize::from(second) == client => Some(first),
            _ => None,
        }
    }

    /// `DuelLimitHit`.
    fn duel_limit_hit(&self) -> bool {
        let wins = (0..self.players.places()).filter_map(|client| {
            self.peer(client)
                .filter(|peer| peer.begun)
                .map(|peer| peer.session.wins)
        });
        tournament::duel_limit_hit(wins, self.limits.duel_fraglimit)
    }

    /// `ClientUserinfoChanged` after a duel record changed: the scoreboard's `w`, `l`.
    pub(super) fn record_changed(&mut self, client: usize) {
        let Some(userinfo) = self.peer(client).map(|peer| peer.userinfo.clone()) else {
            return;
        };
        self.userinfo_changed(client, &userinfo);
    }

    /// `BeginIntermission`'s duel part: the round counted (`AdjustTournamentScores`), and
    /// whether it ends the tournament.
    pub(super) fn duel_intermission_begins(&mut self) {
        if self.gametype != GAMETYPE_DUEL && self.gametype != super::GAMETYPE_POWERDUEL {
            return;
        }
        self.set_config_string(CS_CLIENT_DUELWINNER, b"-1");
        // A power duel's records were kept at each death.
        if self.gametype == super::GAMETYPE_POWERDUEL {
            self.match_end.duel_exit = self.duel_limit_hit();
            return;
        }
        if let Some((first, second, _)) = self.duellists() {
            let result = tournament::round_result(first, second);
            if let Some(winner) = result.winner.filter(|winner| self.peer(*winner).is_some()) {
                if let Some(peer) = self.peer_mut(winner) {
                    peer.session.wins += 1;
                }
                self.record_changed(winner);
                self.set_config_string(CS_CLIENT_DUELWINNER, winner.to_string().as_bytes());
            }
            if let Some(loser) = result.loser {
                if let Some(peer) = self.peer_mut(loser) {
                    peer.session.losses += 1;
                }
                self.record_changed(loser);
            }
        }
        self.match_end.duel_exit = self.duel_limit_hit();
    }

    /// Two seconds into a duel's intermission (`CheckIntermissionExit`,
    /// `g_main.c:1673-1811`): the loser to the back of the line, the next one in, the
    /// duellists named and no winner shown.
    pub(super) fn duel_round_ends(&mut self, server_time: i32) {
        if self.gametype == super::GAMETYPE_POWERDUEL {
            self.power_duel_round_ends(server_time);
            return;
        }
        if self.gametype != GAMETYPE_DUEL {
            return;
        }
        if let Some((first, second, playing)) = self.duellists()
            && let Some(loser) = tournament::round_loser(first, second, playing)
        {
            self.set_team_to(loser, b"s", server_time);
        }
        self.add_tournament_player(server_time);
        let (sorted, playing) = self.sorted();
        if playing >= 2 {
            self.set_config_string(
                CS_CLIENT_DUELISTS,
                &tournament::duelists_string(usize::from(sorted[0]), usize::from(sorted[1])),
            );
            self.set_config_string(CS_CLIENT_DUELWINNER, b"-1");
        }
    }

    /// `ExitLevel` in a duel: short of the duel limit, the next round on this level
    /// (`map_restart 0`, which the engine runs before its next frame); at it, everyone's
    /// record is cleared and the level ends as any other. Returns whether it restarted.
    pub(super) fn duel_exit_level(&mut self, server_time: i32) -> bool {
        if self.gametype != GAMETYPE_DUEL && self.gametype != super::GAMETYPE_POWERDUEL {
            return false;
        }
        if !self.duel_limit_hit() {
            self.match_end.intermission_time = 0;
            self.queue_restart(server_time);
            return true;
        }
        for client in 0..self.players.places() {
            if let Some(peer) = self.peer_mut(client)
                && peer.begun
            {
                (peer.session.wins, peer.session.losses) = (0, 0);
            }
        }
        false
    }

    /// `ClientDisconnect` in a duel (`g_client.c:3969-3988`), before the client is
    /// gone: a duellist leaving mid-round gives the other the round — its record a win,
    /// its score cleared — and one leaving during the intermission starts the next round.
    pub(super) fn duel_disconnect(&mut self, client: usize, server_time: i32) {
        if self.gametype != GAMETYPE_DUEL {
            return;
        }
        if self.match_end.intermission_time == 0 {
            let (sorted, _) = self.sorted();
            let other = match (sorted.first().copied(), sorted.get(1).copied()) {
                (Some(first), Some(second)) if usize::from(second) == client => {
                    Some(usize::from(first))
                }
                (Some(first), Some(second)) if usize::from(first) == client => {
                    Some(usize::from(second))
                }
                _ => None,
            };
            if let Some(other) = other {
                if let Some(peer) = self.peer_mut(other) {
                    peer.state.persistent[PERS_SCORE] = 0;
                    peer.session.wins += 1;
                }
                self.record_changed(other);
            }
        }
        let playing = self.peer(client).is_some_and(|peer| peer.session.team == 0);
        if playing && self.match_end.intermission_time != 0 {
            self.match_end.intermission_time = 0;
            self.queue_restart(server_time);
        }
    }
}
