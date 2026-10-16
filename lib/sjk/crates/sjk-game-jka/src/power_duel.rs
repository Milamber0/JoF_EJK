//! Power Duel (`g_gametype 4`): one lone duellist against a pair (OpenJK
//! `codemp/game/g_main.c:639-790, 2370-2482`, `g_combat.c:2032-2071, 2883-2918`,
//! `g_session.c:217-235`, `g_cmds.c:605-626, 1045-1124`, `g_client.c:3682-3695`,
//! `w_force.c:5460-5467`).
//!
//! The server keeps the line and runs the rounds; these are the rules it asks:
//! - **the duel teams** a newcomer is sorted onto ([`initial_duel_team`]) and counted by
//!   ([`count`]), and whether one more of a team fits ([`check_fail`]);
//! - **who comes in** from the line ([`next_in_line`], `AddPowerDuelPlayers`) and **who
//!   leaves** after a round ([`losers`], `RemovePowerDuelLosers`);
//! - **a round's end** at a death ([`death`]): the lone's death is the pair's round, the
//!   pair's last death the lone's;
//! - **the lone's handicaps**: its health falls from 150 towards 90 with its wins
//!   ([`lone_health`]) and its pool refills faster ([`lone_regen_time`]).

/// `GT_POWERDUEL`.
pub const GT_POWERDUEL: i32 = 4;
/// `duelTeam_t`.
pub const DUELTEAM_FREE: i32 = 0;
pub const DUELTEAM_LONE: i32 = 1;
pub const DUELTEAM_DOUBLE: i32 = 2;
/// `g_powerDuelStartHealth`, `g_powerDuelEndHealth`.
const START_HEALTH: i32 = 150;
const END_HEALTH: i32 = 90;

/// A client as the power duel counts it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counted {
    /// In the game (`inuse`).
    pub in_game: bool,
    pub spectator: bool,
    pub duel_team: i32,
}

/// `G_PowerDuelCount`: the lone and paired duellists among `clients` — the spectators too
/// where `with_spectators`.
pub fn count(clients: impl IntoIterator<Item = Counted>, with_spectators: bool) -> (i32, i32) {
    clients
        .into_iter()
        .filter(|client| client.in_game && (with_spectators || !client.spectator))
        .fold((0, 0), |(loners, doubles), client| match client.duel_team {
            DUELTEAM_LONE => (loners + 1, doubles),
            DUELTEAM_DOUBLE => (loners, doubles + 1),
            _ => (loners, doubles),
        })
}

/// `G_InitSessionData`'s duel team for a newcomer, from everyone counted (spectators
/// too): the pair while it has nobody or the lone outnumbers half of it, else the lone.
pub fn initial_duel_team(loners: i32, doubles: i32) -> i32 {
    if doubles == 0 || loners > doubles / 2 {
        DUELTEAM_DOUBLE
    } else {
        DUELTEAM_LONE
    }
}

/// `G_PowerDuelCheckFail`: whether a client of `duel_team` may not join the game, given
/// who plays (`loners`, `doubles`): no team, or its team full.
pub fn check_fail(duel_team: i32, loners: i32, doubles: i32) -> bool {
    duel_team == DUELTEAM_FREE
        || (duel_team == DUELTEAM_LONE && loners >= 1)
        || (duel_team == DUELTEAM_DOUBLE && doubles >= 2)
}

/// A spectator waiting in line, as `AddPowerDuelPlayers` reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Waiting {
    pub client: usize,
    pub connected: bool,
    pub spectator: bool,
    pub duel_team: i32,
    pub spectator_num: i32,
    /// A dedicated follow or scoreboard client, never brought in.
    pub scoreboard: bool,
}

/// `AddPowerDuelPlayers`' choice: nobody where one lone and two paired already play, or
/// where the line cannot make up the teams (`everyone`, spectators counted); else the
/// longest waiting spectator of a team still short (`playing` counts who plays).
pub fn next_in_line(
    waiting: &[Waiting],
    playing: (i32, i32),
    everyone: (i32, i32),
) -> Option<usize> {
    let (loners, doubles) = playing;
    if loners >= 1 && doubles >= 2 {
        return None;
    }
    if everyone.0 < 1 || everyone.1 < 2 {
        return None;
    }
    waiting
        .iter()
        .filter(|client| {
            client.connected
                && client.spectator
                && client.duel_team != DUELTEAM_FREE
                && !client.scoreboard
        })
        .filter(|client| {
            !(client.duel_team == DUELTEAM_LONE && loners >= 1)
                && !(client.duel_team == DUELTEAM_DOUBLE && doubles >= 2)
        })
        // The first of the longest waiting (`spectatorNum > next's`).
        .fold(None::<&Waiting>, |best, client| match best {
            Some(best) if client.spectator_num <= best.spectator_num => Some(best),
            _ => Some(client),
        })
        .map(|client| client.client)
}

/// A client as a round's end reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Duellist {
    pub client: usize,
    pub connected: bool,
    pub health: i32,
    pub spectator: bool,
    pub duel_team: i32,
    /// `iAmALoser`: it died this round and respawned into the line.
    pub loser: bool,
}

/// `RemovePowerDuelLosers`: up to three of the clients, in slot order, dead in play or
/// spectating as losers; the first sorted (`fallback`) where there is nobody.
pub fn losers(clients: &[Duellist], fallback: usize) -> Vec<usize> {
    let mut out: Vec<usize> = clients
        .iter()
        .filter(|client| {
            client.connected
                && (client.health <= 0 || client.loser)
                && (!client.spectator || client.loser)
        })
        .map(|client| client.client)
        .take(3)
        .collect();
    if out.is_empty() {
        out.push(fallback);
    }
    out
}

/// What a death in a power duel decides.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoundEnd {
    /// The living of the winning team, each a win (`G_AddPowerDuelScore`).
    pub winners: Vec<usize>,
    /// The losing team's dead or losers, each a loss (`G_AddPowerDuelLoserScore`).
    pub losers: Vec<usize>,
}

/// `player_die`'s power-duel checks for `dead` (whose health is already its own): the lone
/// dead is the pair's round; a paired duellist dead with no other of the pair alive in play
/// is the lone's. `None` where the round goes on.
pub fn death(clients: &[Duellist], dead: usize) -> Option<RoundEnd> {
    let team = clients
        .iter()
        .find(|client| client.client == dead)?
        .duel_team;
    let (winning, losing) = match team {
        DUELTEAM_LONE => (DUELTEAM_DOUBLE, DUELTEAM_LONE),
        DUELTEAM_DOUBLE => {
            let lives = clients.iter().any(|client| {
                client.client != dead
                    && client.connected
                    && !client.loser
                    && client.health > 0
                    && !client.spectator
                    && client.duel_team == DUELTEAM_DOUBLE
            });
            if lives {
                return None;
            }
            (DUELTEAM_LONE, DUELTEAM_DOUBLE)
        }
        _ => return None,
    };
    let winners = clients
        .iter()
        .filter(|client| {
            client.connected
                && !client.loser
                && client.health > 0
                && !client.spectator
                && client.duel_team == winning
        })
        .map(|client| client.client)
        .collect();
    let losers = clients
        .iter()
        .filter(|client| {
            client.connected
                && (client.loser || (client.health <= 0 && !client.spectator))
                && client.duel_team == losing
        })
        .map(|client| client.client)
        .collect();
    Some(RoundEnd { winners, losers })
}

/// The lone's health and maximum at a spawn (`ClientSpawn`): from 150 down towards 90 as
/// its wins near `duel_fraglimit`; 150 without a limit.
pub fn lone_health(wins: i32, duel_fraglimit: i32) -> i32 {
    if duel_fraglimit == 0 {
        return 150;
    }
    (START_HEALTH as f32
        - ((START_HEALTH - END_HEALTH) as f32 * wins as f32 / duel_fraglimit as f32)) as i32
}

/// `ClientSpawn`'s power-duel health: a lone — spectating or not — spawns with
/// [`lone_health`] as its health and maximum (the spawn gave everyone else a duel's 100).
pub fn spawned(
    state: &mut sjk_protocol::PlayerState,
    session: &crate::client_begin::PlayerSession,
    gametype: i32,
    duel_fraglimit: i32,
) {
    if gametype == GT_POWERDUEL && session.duel_team == DUELTEAM_LONE {
        let health = lone_health(session.wins, duel_fraglimit) as u32;
        (state.stats[0], state.stats[8]) = (health, health);
    }
}

/// The lone's regeneration pace (`WP_ForcePowersUpdate`): `g_forceRegenTime` scaled by
/// 0.6 and three tenths of its share of the win limit — 0.7 without a limit — at least 1,
/// in the double the reference adds to its integer debounce (`+=`, truncated after).
pub fn lone_regen_time(regen_time: i32, wins: i32, duel_fraglimit: i32) -> f64 {
    let scaled = if duel_fraglimit != 0 {
        f64::from(regen_time) * (0.6 + 0.3 * f64::from(wins) / f64::from(duel_fraglimit))
    } else {
        f64::from(regen_time) * 0.7
    };
    scaled.max(1.0)
}

/// `CS_CLIENT_DUELISTS` for three: the lone and the pair as sorted.
pub fn duelists_string(sorted: [usize; 3]) -> Vec<u8> {
    format!("{}|{}|{}", sorted[0], sorted[1], sorted[2]).into_bytes()
}
