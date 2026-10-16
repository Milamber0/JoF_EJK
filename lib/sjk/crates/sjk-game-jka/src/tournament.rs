//! The duel's tournament (`g_gametype 3`): the line of spectators waiting for their turn
//! and what a round's end does to it — `AddTournamentQueue`, `AddTournamentPlayer`,
//! `RemoveTournamentLoser`, `RemoveDuelDrawLoser`, `AdjustTournamentScores`,
//! `DuelLimitHit` and `CheckTournament`'s strings (OpenJK `codemp/game/g_main.c:540-930,
//! 1387-1420, 2271-2320`). The game applies what these decide: the team changes go
//! through its own `SetTeam`, the strings through its configstrings.

use crate::client_begin::PlayerSession;

/// `TEAM_SPECTATOR`.
const TEAM_SPECTATOR: i32 = 3;

/// `AddTournamentQueue`: `client` goes to the back of the line (0) and every other
/// spectator of a slot in use one place forward — the longest waiting has the highest
/// number. `sessions` is every client that is not disconnected.
pub fn add_to_queue<'a>(
    sessions: impl IntoIterator<Item = (usize, &'a mut PlayerSession)>,
    client: usize,
) {
    for (number, session) in sessions {
        if number == client {
            session.spectator_num = 0;
        } else if session.team == TEAM_SPECTATOR {
            session.spectator_num += 1;
        }
    }
}

/// A client as `AddTournamentPlayer` weighs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Waiting {
    pub client: usize,
    /// `pers.connected == CON_CONNECTED`.
    pub connected: bool,
    /// `sess.sessionTeam == TEAM_SPECTATOR`.
    pub spectator: bool,
    /// `sess.spectatorNum`.
    pub spectator_num: i32,
    /// `ps.ping >= 999` with `g_allowHighPingDuelist` 0: lagging out, passed over.
    pub lagging: bool,
    /// A dedicated follower or scoreboard client (`SPECTATOR_SCOREBOARD`, or following a
    /// client below zero): never chosen.
    pub scoreboard: bool,
}

/// `AddTournamentPlayer`'s choice, when fewer than two play: the connected spectator
/// longest in line (the first such in slot order on a tie). The caller sets
/// `level.warmupTime` to -1 and puts it on the free team (`SetTeam(ent, "f")`).
pub fn next_in_line(clients: impl IntoIterator<Item = Waiting>) -> Option<usize> {
    let mut chosen: Option<Waiting> = None;
    for client in clients {
        if !client.connected || client.lagging || !client.spectator || client.scoreboard {
            continue;
        }
        if chosen.is_none_or(|chosen| client.spectator_num > chosen.spectator_num) {
            chosen = Some(client);
        }
    }
    chosen.map(|chosen| chosen.client)
}

/// One of `level.sortedClients[0]` and `[1]` as a round's end reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Duellist {
    pub client: usize,
    /// `pers.connected == CON_CONNECTED`.
    pub connected: bool,
    /// `ps.persistant[PERS_SCORE]`.
    pub score: i32,
    /// `ps.stats[STAT_HEALTH] + ps.stats[STAT_ARMOR]`: a tie's decider.
    pub vitality: i32,
}

/// Who won and who lost a round: `(winner, loser)`, each only where the reference counts
/// it. The winner is also `CS_CLIENT_DUELWINNER`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoundResult {
    pub winner: Option<usize>,
    pub loser: Option<usize>,
}

/// `AdjustTournamentScores`: the better score wins. On a tie between two connected
/// duellists the one with more health and armour left does, and on a tie of that too the
/// first sorted — the reference's last branch.
pub fn round_result(first: Duellist, second: Duellist) -> RoundResult {
    if first.score == second.score && first.connected && second.connected {
        let (winner, loser) = if second.vitality > first.vitality {
            (second, first)
        } else {
            (first, second)
        };
        return RoundResult {
            winner: Some(winner.client),
            loser: Some(loser.client),
        };
    }
    RoundResult {
        winner: first.connected.then_some(first.client),
        loser: second.connected.then_some(second.client),
    }
}

/// Who a round's end sends to the spectators: `RemoveDuelDrawLoser` on a tie between two
/// connected duellists (the one with less health and armour, the second sorted on a tie
/// of that), else `RemoveTournamentLoser` (the second sorted, with exactly two playing
/// and it connected). `playing` is `level.numPlayingClients`.
pub fn round_loser(first: Duellist, second: Duellist, playing: usize) -> Option<usize> {
    if first.score == second.score && first.connected && second.connected {
        return Some(if first.vitality > second.vitality {
            second.client
        } else if second.vitality > first.vitality {
            first.client
        } else {
            second.client
        });
    }
    (playing == 2 && second.connected).then_some(second.client)
}

/// `DuelLimitHit`: a connected client has won `duel_fraglimit` duels (never, at 0).
pub fn duel_limit_hit(wins: impl IntoIterator<Item = i32>, duel_fraglimit: i32) -> bool {
    duel_fraglimit != 0 && wins.into_iter().any(|wins| wins >= duel_fraglimit)
}

/// `CS_CLIENT_DUELISTS`: the two sorted first.
pub fn duelists_string(first: usize, second: usize) -> Vec<u8> {
    format!("{first}|{second}").into_bytes()
}

/// `CS_CLIENT_DUELHEALTHS` (`g_showDuelHealths` 1 or more): the two duellists' health.
pub fn healths_string(first: i32, second: i32) -> Vec<u8> {
    format!("{first}|{second}|!").into_bytes()
}
