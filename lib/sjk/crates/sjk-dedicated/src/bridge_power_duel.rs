//! Power Duel on the native server: one lone duellist against a pair. The line and the
//! rounds are the tournament's (`bridge_tournament`), with the power duel's own choice of
//! who comes in (`AddPowerDuelPlayers`), its trio named and respawned together
//! (`CheckTournament`, `G_ResetDuelists`), a round decided by the lone's death or the
//! pair's (`player_die`), the dead sent to the line as losers (`ClientRespawn`) and away
//! after the round (`RemovePowerDuelLosers`), and the `duelteam` command. The rules are
//! [`sjk_game_jka::power_duel`]'s.

use super::{GAMETYPE_POWERDUEL, NativeGame, TEAM_SPECTATOR, Told};
use sjk_game_jka::event_entity::EventEntity;
use sjk_game_jka::player_death::DeathRequest;
use sjk_game_jka::power_duel::{
    self, Counted, DUELTEAM_DOUBLE, DUELTEAM_FREE, DUELTEAM_LONE, Duellist, Waiting,
};

/// `CS_CLIENT_DUELISTS`, `CS_CLIENT_DUELWINNER`.
const CS_CLIENT_DUELISTS: usize = 30;
const CS_CLIENT_DUELWINNER: usize = 29;
/// `EV_GLOBAL_DUEL`: every client is told who duels whom.
const EV_GLOBAL_DUEL: u32 = 23;
/// Entity fields the duel event carries its three in.
const ES_OTHER_ENTITY: usize = 59;
const ES_OTHER_ENTITY2: usize = 39;
const ES_GROUND_ENTITY: usize = 22;
/// How often the line is told what it lacks.
const PRINT_INTERVAL: i32 = 10_000;

/// What the server keeps of a power duel between frames; reset with the level, as the
/// module's globals are at a restart.
#[derive(Default)]
pub(super) struct PowerDuel {
    /// `g_dontFrickinCheck`: the trio is complete; nobody is brought in until a death.
    complete: bool,
    /// `g_duelPrintTimer`: when the line is next told what it lacks.
    print_at: i32,
    /// `g_noPDuelCheck`: the trio is being respawned, and its deaths decide nothing.
    resetting: bool,
}

impl NativeGame {
    pub(super) fn power_duel_on(&self) -> bool {
        self.gametype == GAMETYPE_POWERDUEL
    }

    /// Everyone in the game as `G_PowerDuelCount` counts them.
    fn counted(&self) -> impl Iterator<Item = Counted> + '_ {
        (0..self.players.places())
            .filter_map(|client| self.peer(client))
            .map(|peer| Counted {
                in_game: true,
                spectator: peer.session.team == i32::from(TEAM_SPECTATOR),
                duel_team: peer.session.duel_team,
            })
    }

    /// `G_InitSessionData`'s duel team for a newcomer in a power duel; none otherwise.
    pub(super) fn initial_duel_team(&self) -> i32 {
        if !self.power_duel_on() {
            return DUELTEAM_FREE;
        }
        let (loners, doubles) = power_duel::count(self.counted(), true);
        power_duel::initial_duel_team(loners, doubles)
    }

    /// `SetTeam`'s power-duel override: `client` may not play where three already do or
    /// its team is full (`G_PowerDuelCheckFail`).
    pub(super) fn power_duel_full(&self, client: usize) -> bool {
        if !self.power_duel_on() {
            return false;
        }
        let (loners, doubles) = power_duel::count(self.counted(), false);
        let team = self
            .peer(client)
            .map_or(DUELTEAM_FREE, |peer| peer.session.duel_team);
        self.sorted_playing() >= 3 || power_duel::check_fail(team, loners, doubles)
    }

    /// `level.numPlayingClients`.
    fn sorted_playing(&self) -> usize {
        sjk_game_jka::ranks::calculate_ranks_in(&self.contenders(), self.standings()).playing
    }

    /// `ClientBegin`'s power-duel check: a client that would play without a duel team is
    /// sent to the spectators instead (whose begin follows). Returns whether it was.
    pub(super) fn power_duel_begin(&mut self, client: usize, server_time: i32) -> bool {
        let unteamed = self.power_duel_on()
            && self.peer(client).is_some_and(|peer| {
                peer.session.team != i32::from(TEAM_SPECTATOR)
                    && peer.session.duel_team == DUELTEAM_FREE
            });
        if unteamed {
            self.set_team_to(client, b"s", server_time);
        }
        unteamed
    }

    /// `ClientRespawn` in a power duel: the dead does not come back but joins the line, a
    /// spectator at its back, marked as a loser of the round (`iAmALoser`). Returns
    /// whether it was.
    pub(super) fn power_duel_respawn(&mut self, client: usize, server_time: i32) -> bool {
        if !self.power_duel_on() {
            return false;
        }
        self.leave_body(client, server_time);
        let Some(peer) = self.peer_mut(client) else {
            return true;
        };
        peer.session.team = i32::from(TEAM_SPECTATOR);
        (peer.session.spectator_state, peer.session.spectator_client) =
            (sjk_game_jka::client_begin::SPECTATOR_FREE, 0);
        self.queue_at_back(client);
        self.begin(client, server_time, None);
        if let Some(peer) = self.peer_mut(client) {
            peer.loser = true;
        }
        true
    }

    /// `CheckTournament` in a power duel (`g_main.c:2370-2482`): a fourth in play sent
    /// away, the trio made up from the line, named, told to everyone and respawned
    /// together — or, while the line cannot make it up, told what it lacks every ten
    /// seconds.
    pub(super) fn check_power_duel(&mut self, server_time: i32) {
        if !self.power_duel_on() {
            return;
        }
        let (sorted, playing) = self.sorted();
        if playing >= 3 && self.non_spectators() >= 3 {
            self.set_config_string(
                CS_CLIENT_DUELISTS,
                &power_duel::duelists_string([sorted[0], sorted[1], sorted[2]].map(usize::from)),
            );
        }
        if playing < 2 {
            self.power_duel.complete = false;
        }
        let (loners, doubles) = power_duel::count(self.counted(), false);
        if playing > 3 {
            if loners > 1 {
                self.remove_duellists(DUELTEAM_LONE, server_time);
            } else if doubles > 2 {
                self.remove_duellists(DUELTEAM_DOUBLE, server_time);
            }
        } else if playing < 3 && (loners < 1 || doubles < 1) {
            self.power_duel.complete = false;
        }
        if self.sorted().1 >= 3 || self.power_duel.complete {
            self.power_duel.complete = true;
            return;
        }
        self.add_power_duel_players(server_time);
        let (sorted, playing) = self.sorted();
        if playing >= 3 && self.can_reset_duellists(&sorted) {
            self.announce_duellists(&sorted, server_time);
            self.reset_duellists(&sorted, server_time);
            self.power_duel.complete = true;
        } else if (playing > 0 || self.peers() > 0) && self.power_duel.print_at < server_time {
            let (loners, _) = power_duel::count(self.counted(), true);
            let lacking: &[u8] = if loners < 1 {
                b"cp \"@@@DUELMORESINGLE\n\""
            } else {
                b"cp \"@@@DUELMOREPAIRED\n\""
            };
            self.told.push(Told::Everyone(lacking.to_vec()));
            self.power_duel.print_at = server_time + PRINT_INTERVAL;
        }
        let (sorted, playing) = self.sorted();
        if playing >= 3 && self.non_spectators() >= 3 && self.can_reset_duellists(&sorted) {
            self.announce_duellists(&sorted, server_time);
        }
    }

    /// `EV_GLOBAL_DUEL` to everyone, and `CS_CLIENT_DUELISTS`: the three sorted first.
    fn announce_duellists(&mut self, sorted: &[u16], server_time: i32) {
        let mut event = EventEntity {
            event: EV_GLOBAL_DUEL,
            parameter: 0,
            origin: [0.0; 3],
            client: None,
            broadcast: true,
            extra: [(0, 0); 12],
        };
        event.extra[..3].copy_from_slice(&[
            (ES_OTHER_ENTITY, u32::from(sorted[0])),
            (ES_OTHER_ENTITY2, u32::from(sorted[1])),
            (ES_GROUND_ENTITY, u32::from(sorted[2])),
        ]);
        let _ = self.pool.spawn_temporary(event.state(), server_time, None);
        self.set_config_string(
            CS_CLIENT_DUELISTS,
            &power_duel::duelists_string([sorted[0], sorted[1], sorted[2]].map(usize::from)),
        );
    }

    /// `G_CanResetDuelists`: the three sorted first all alive and on a duel team in play.
    fn can_reset_duellists(&self, sorted: &[u16]) -> bool {
        sorted.iter().take(3).all(|client| {
            self.peer(usize::from(*client)).is_some_and(|peer| {
                peer.health > 0
                    && peer.session.team != i32::from(TEAM_SPECTATOR)
                    && peer.session.duel_team > DUELTEAM_FREE
            })
        }) && sorted.len() >= 3
    }

    /// `G_ResetDuelists`: each of the three killed (a suicide that decides nothing) and
    /// spawned again, so that the round starts from nothing.
    fn reset_duellists(&mut self, sorted: &[u16], server_time: i32) {
        for client in sorted.iter().take(3).map(|client| usize::from(*client)) {
            let Some(peer) = self.peer(client) else {
                continue;
            };
            let request = DeathRequest::suicide(
                server_time,
                client as u16,
                peer.state.origin(),
                peer.saber_off_sounds(),
                peer.health,
            );
            self.power_duel.resetting = true;
            self.die(client, request);
            self.power_duel.resetting = false;
            self.begin(client, server_time, None);
        }
    }

    /// `G_RemoveDuelist`: everyone in play of an over-full `team` sent to the line.
    fn remove_duellists(&mut self, team: i32, server_time: i32) {
        for client in 0..self.players.places() {
            if self.peer(client).is_some_and(|peer| {
                peer.session.team != i32::from(TEAM_SPECTATOR) && peer.session.duel_team == team
            }) {
                self.set_team_to(client, b"s", server_time);
            }
        }
    }

    /// `AddPowerDuelPlayers`: from the line, one at a time, whoever fills a place.
    fn add_power_duel_players(&mut self, server_time: i32) {
        loop {
            if self.sorted().1 >= 3 {
                return;
            }
            let waiting: Vec<Waiting> = (0..self.players.places())
                .filter_map(|client| {
                    let peer = self.peer(client)?;
                    Some(Waiting {
                        client,
                        connected: peer.begun,
                        spectator: peer.session.team == i32::from(TEAM_SPECTATOR),
                        duel_team: peer.session.duel_team,
                        spectator_num: peer.session.spectator_num,
                        scoreboard: peer.session.spectator_state
                            == sjk_game_jka::client_begin::SPECTATOR_SCOREBOARD
                            || peer.session.spectator_client < 0,
                    })
                })
                .collect();
            let playing = power_duel::count(self.counted(), false);
            let everyone = power_duel::count(self.counted(), true);
            let Some(next) = power_duel::next_in_line(&waiting, playing, everyone) else {
                return;
            };
            self.set_team_to(next, b"f", server_time);
        }
    }

    /// Everyone as a round's end reads them.
    fn duellists_now(&self) -> Vec<Duellist> {
        (0..self.players.places())
            .filter_map(|client| {
                let peer = self.peer(client)?;
                Some(Duellist {
                    client,
                    connected: peer.begun,
                    health: peer.state.stats[0] as i32,
                    spectator: peer.session.team == i32::from(TEAM_SPECTATOR),
                    duel_team: peer.session.duel_team,
                    loser: peer.loser,
                })
            })
            .collect()
    }

    /// `player_die`'s power-duel opening: the trio is no longer complete, and the rules are
    /// checked at once — a death at the intermission decides nothing. Returns whether the
    /// death goes on.
    pub(super) fn power_duel_death_begins(&mut self, server_time: i32) -> bool {
        if !self.power_duel_on() {
            return true;
        }
        self.power_duel.complete = false;
        self.run_match_end(server_time);
        self.match_end.intermission_time == 0
    }

    /// `player_die`'s power-duel end: the lone's death wins the pair the round, the pair's
    /// last death wins the lone it — a win for each living winner, a loss for each of the
    /// losers — and the round ends at the next check.
    pub(super) fn power_duel_death(&mut self, client: usize) {
        if !self.power_duel_on() || self.power_duel.resetting {
            return;
        }
        let Some(end) = power_duel::death(&self.duellists_now(), client) else {
            return;
        };
        for winner in end.winners {
            if let Some(peer) = self.peer_mut(winner) {
                peer.session.wins += 1;
            }
            self.record_changed(winner);
        }
        for loser in end.losers {
            if let Some(peer) = self.peer_mut(loser) {
                peer.session.losses += 1;
            }
            self.record_changed(loser);
        }
        self.match_end.end_power_duel = true;
    }

    /// Two seconds into a power duel's intermission (`CheckIntermissionExit`): the dead
    /// and the losers to the line, the places filled, the trio named.
    pub(super) fn power_duel_round_ends(&mut self, server_time: i32) {
        let (sorted, _) = self.sorted();
        let losers = power_duel::losers(
            &self.duellists_now(),
            sorted.first().map_or(0, |first| usize::from(*first)),
        );
        for loser in losers {
            self.set_team_to(loser, b"s", server_time);
        }
        self.power_duel.complete = false;
        self.add_power_duel_players(server_time);
        let (sorted, playing) = self.sorted();
        if playing >= 3 && self.non_spectators() >= 3 {
            self.set_config_string(
                CS_CLIENT_DUELISTS,
                &power_duel::duelists_string([sorted[0], sorted[1], sorted[2]].map(usize::from)),
            );
            self.set_config_string(CS_CLIENT_DUELWINNER, b"-1");
        }
    }

    /// `Cmd_DuelTeam_f`: with no word, the client is told its duel team; with one, it
    /// changes team — a duellist in play dies first, under its old team — its record
    /// cleared and published, not again for five seconds.
    pub(super) fn duel_team_command(
        &mut self,
        client: usize,
        word: Option<&[u8]>,
        server_time: i32,
    ) {
        if !self.power_duel_on() {
            return;
        }
        let Some(peer) = self.peer(client) else {
            return;
        };
        let (old, switch_time, playing) = (
            peer.session.duel_team,
            peer.switch_duel_team_time,
            peer.session.team != i32::from(TEAM_SPECTATOR),
        );
        let (max_health, team, origin) = (
            peer.state.max_health(),
            peer.session.team,
            peer.state.origin(),
        );
        let Some(word) = word else {
            let name: &[u8] = match old {
                DUELTEAM_FREE => b"None",
                DUELTEAM_LONE => b"Single",
                DUELTEAM_DOUBLE => b"Double",
                _ => return,
            };
            self.told
                .push(Told::One(client, [b"print \"", name, b"\n\""].concat()));
            return;
        };
        if switch_time > server_time {
            self.told
                .push(Told::One(client, b"print \"@@@NOSWITCH\n\"".to_vec()));
            return;
        }
        let wanted = if word.eq_ignore_ascii_case(b"free") {
            DUELTEAM_FREE
        } else if word.eq_ignore_ascii_case(b"single") {
            DUELTEAM_LONE
        } else if word.eq_ignore_ascii_case(b"double") {
            DUELTEAM_DOUBLE
        } else {
            self.told.push(Told::One(
                client,
                format!(
                    "print \"'{}' not a valid duel team.\n\"",
                    String::from_utf8_lossy(word)
                )
                .into_bytes(),
            ));
            old
        };
        if wanted == old {
            return;
        }
        if playing {
            // `G_Damage(ent, ent, ent, …, 99999, DAMAGE_NO_PROTECTION, MOD_SUICIDE)` under
            // the old team, which the round's end reads.
            let request = sjk_game_jka::damage::DamageRequest {
                level_time: server_time,
                attacker: Some(sjk_game_jka::damage::Attacker {
                    npc: false,
                    client: client as u16,
                    max_health,
                    team,
                    saber_knockback: [0.0; 4],
                }),
                direction: None,
                point: Some(origin),
                damage: 99_999,
                flags: sjk_game_jka::damage::DAMAGE_NO_PROTECTION,
                means: sjk_game_jka::means_of_death::MOD_SUICIDE,
            };
            let _ = self.hurt(client, request);
        }
        if let Some(peer) = self.peer_mut(client) {
            peer.session.duel_team = wanted;
            (peer.session.wins, peer.session.losses) = (0, 0);
            peer.switch_duel_team_time = server_time + 5_000;
        }
        self.record_changed(client);
    }
}
