//! Who is in first place: `CalculateRanks` and `SortRanks` (`g_main.c:941-1190`) outside
//! duels, and the scoreboard a client is sent — `DeathmatchScoreboardMessage`
//! (`g_cmds.c`), the `scores` command. In team games every rank is the teams' order (red
//! ahead 0, blue 1, tied 2) and `CS_SCORES1`/`2` are the team scores. Held against the
//! transcripts' `scores` lines and `CS_SCORES1`/`CS_SCORES2`/`CS_CLIENT_DUELWINNER`
//! strings.

const CS_SCORES1: usize = 6;
const CS_SCORES2: usize = 7;
const CS_CLIENT_DUELWINNER: usize = 29;
const SCORE_NOT_PRESENT: i32 = -9_999;
const RANK_TIED_FLAG: u32 = 0x4000;
const TEAM_SPECTATOR: i32 = 3;
/// `MAX_CLIENT_SCORE_SEND`.
const SCORES_SENT: usize = 20;
/// `persistant[]` indices.
const PERS_SCORE: usize = 0;
const PERS_RANK: usize = 2;
const PERS_KILLED: usize = 8;

/// A connected client as the ranking sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contender {
    /// Wire client number.
    pub client: u16,
    /// `sess.sessionTeam`.
    pub team: i32,
    /// `pers.connected == CON_CONNECTING`: holds a gamestate but has not begun.
    pub connecting: bool,
    /// `sess.spectatorNum`: a spectator's place in the queue.
    pub spectator_number: i32,
    /// A dedicated scoreboard or `follow1`/`follow2` camera (`SPECTATOR_SCOREBOARD`, or a
    /// `spectatorClient` below zero): sorted after everyone.
    pub special: bool,
    /// In a power duel, the lone duellist in play: sorted before everyone.
    pub lone: bool,
    /// `iAmALoser`: a power duellist dead this round and waiting in line, still counted
    /// among the playing (`numPlayingClients`).
    pub loser: bool,
    /// `ps.persistant`.
    pub persistant: [i32; 16],
    /// `ps.ping`, `pers.enterTime`, `s.powerups`, and hits over shots — what the
    /// scoreboard shows besides the persistant counters.
    pub ping: i32,
    pub enter_time: i32,
    pub powerups: u32,
    pub accuracy: (i32, i32),
}

/// What `CalculateRanks` decides.
#[derive(Clone, Debug, PartialEq)]
pub struct Ranks {
    /// `level.sortedClients`: everyone connected, best first.
    pub sorted: Vec<u16>,
    /// `PERS_RANK` per playing client, in `sorted` order (the others keep theirs).
    pub ranks: Vec<(u16, u32)>,
    /// `CS_SCORES1`, `CS_SCORES2` and, outside team games, `CS_CLIENT_DUELWINNER`, to
    /// publish in this order.
    pub strings: Vec<(usize, Vec<u8>)>,
    /// `level.numPlayingClients`.
    pub playing: usize,
}

/// The game type and the team scores (`level.teamScores[TEAM_RED]`, `[TEAM_BLUE]`), which
/// team games rank by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Standings {
    pub gametype: i32,
    pub team_scores: [i32; 2],
}

/// `GT_TEAM`: from here on the game types are team games.
const GT_TEAM: i32 = 6;
/// `GT_DUEL`, `GT_POWERDUEL`: `CS_CLIENT_DUELWINNER` is the tournament's own.
const GT_DUEL: i32 = 3;
const GT_POWERDUEL: i32 = 4;

/// `CalculateRanks` for a free-for-all (see [`calculate_ranks_in`]).
pub fn calculate_ranks(clients: &[Contender]) -> Ranks {
    calculate_ranks_in(clients, Standings::default())
}

/// `CalculateRanks`: spectators last (later in the queue first), then by score. Outside
/// team games, ties are tied; in team games everyone's rank is the teams' order. `qsort`
/// is not stable; a stable sort keeps the connection order among equals, which is what
/// glibc's merge sort gives the reference on a handful of clients.
pub fn calculate_ranks_in(clients: &[Contender], standings: Standings) -> Ranks {
    let mut sorted: Vec<&Contender> = clients.iter().collect();
    sorted.sort_by(|a, b| sort_ranks(a, b, standings.gametype));
    let playing = sorted
        .iter()
        .filter(|client| !client.connecting && (client.team != TEAM_SPECTATOR || client.loser))
        .count();
    if standings.gametype >= GT_TEAM {
        let [red, blue] = standings.team_scores;
        let rank = match red.cmp(&blue) {
            std::cmp::Ordering::Equal => 2,
            std::cmp::Ordering::Greater => 0,
            std::cmp::Ordering::Less => 1,
        };
        return Ranks {
            sorted: sorted.iter().map(|client| client.client).collect(),
            ranks: sorted.iter().map(|client| (client.client, rank)).collect(),
            strings: vec![
                (CS_SCORES1, red.to_string().into_bytes()),
                (CS_SCORES2, blue.to_string().into_bytes()),
            ],
            playing,
        };
    }
    let mut ranks = Vec::with_capacity(playing);
    let (mut rank, mut score) = (0_u32, 0);
    for (place, client) in sorted.iter().take(playing).enumerate() {
        let new_score = client.persistant[PERS_SCORE];
        if place == 0 || new_score != score {
            rank = place as u32;
            ranks.push((client.client, rank));
        } else {
            let previous = ranks.len() - 1;
            ranks[previous].1 = rank | RANK_TIED_FLAG;
            ranks.push((client.client, rank | RANK_TIED_FLAG));
        }
        score = new_score;
    }
    let score_of = |place: usize| {
        sorted
            .get(place)
            .map_or(SCORE_NOT_PRESENT, |client| client.persistant[PERS_SCORE])
    };
    let (first, second) = match sorted.len() {
        0 => (SCORE_NOT_PRESENT, SCORE_NOT_PRESENT),
        1 => (score_of(0), SCORE_NOT_PRESENT),
        _ => (score_of(0), score_of(1)),
    };
    let winner = sorted
        .first()
        .map_or("-1".to_owned(), |client| client.client.to_string());
    let mut strings = vec![
        (CS_SCORES1, first.to_string().into_bytes()),
        (CS_SCORES2, second.to_string().into_bytes()),
    ];
    // "when not in duel, use this configstring to pass the index of the player currently
    // in first place"; a duel's is the round's winner.
    if standings.gametype != GT_DUEL && standings.gametype != GT_POWERDUEL {
        strings.push((CS_CLIENT_DUELWINNER, winner.into_bytes()));
    }
    Ranks {
        sorted: sorted.iter().map(|client| client.client).collect(),
        ranks,
        strings,
        playing,
    }
}

/// `SortRanks` outside power duels.
fn sort_ranks(a: &Contender, b: &Contender, gametype: i32) -> std::cmp::Ordering {
    use std::cmp::Ordering::{Equal, Greater, Less};
    // "sort single duelists first", above even the special clients.
    if gametype == crate::power_duel::GT_POWERDUEL {
        if a.lone {
            return Less;
        }
        if b.lone {
            return Greater;
        }
    }
    // "sort special clients last"
    if a.special {
        return Greater;
    }
    if b.special {
        return Less;
    }
    match (a.connecting, b.connecting) {
        (true, false) => return Greater,
        (false, true) => return Less,
        _ => {}
    }
    match (a.team == TEAM_SPECTATOR, b.team == TEAM_SPECTATOR) {
        (true, true) => return b.spectator_number.cmp(&a.spectator_number),
        (true, false) => return Greater,
        (false, true) => return Less,
        _ => {}
    }
    match a.persistant[PERS_SCORE].cmp(&b.persistant[PERS_SCORE]) {
        Greater => Less,
        Less => Greater,
        Equal => Equal,
    }
}

/// `DeathmatchScoreboardMessage` outside team games (see [`scoreboard_message_in`]).
pub fn scoreboard_message(clients: &[Contender], sorted: &[u16], level_time: i32) -> Vec<u8> {
    scoreboard_message_in(clients, sorted, level_time, [0; 2])
}

/// `DeathmatchScoreboardMessage`: the `scores` command for one client, from `clients` in
/// `sorted` order, at `level_time`, with the team scores (zero outside team games).
pub fn scoreboard_message_in(
    clients: &[Contender],
    sorted: &[u16],
    level_time: i32,
    team_scores: [i32; 2],
) -> Vec<u8> {
    let mut entries = String::new();
    for client in sorted
        .iter()
        .take(SCORES_SENT)
        .filter_map(|number| clients.iter().find(|client| client.client == *number))
    {
        let ping = if client.connecting {
            -1
        } else {
            client.ping.min(999)
        };
        let accuracy = if client.accuracy.1 != 0 {
            client.accuracy.0 * 100 / client.accuracy.1
        } else {
            0
        };
        let perfect =
            u8::from(client.persistant[PERS_RANK] == 0 && client.persistant[PERS_KILLED] == 0);
        let p = &client.persistant;
        let entry = format!(
            " {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
            client.client,
            p[PERS_SCORE],
            ping,
            (level_time - client.enter_time) / 60_000,
            0,
            client.powerups,
            accuracy,
            p[9],
            p[10],
            p[13],
            p[11],
            p[12],
            perfect,
            p[14]
        );
        // `MAX_STRING_CHARS - 1` for the entries, the prefix counted against it.
        if entries.len() + entry.len() + "scores 0 0 0".len() >= 1_023 {
            break;
        }
        entries.push_str(&entry);
    }
    format!(
        "scores {} {} {}{entries}",
        clients.len(),
        team_scores[0],
        team_scores[1]
    )
    .into_bytes()
}
