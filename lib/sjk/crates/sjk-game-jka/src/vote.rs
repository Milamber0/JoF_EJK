//! Votes: `callvote`, `vote` and the frame that decides them.
//!
//! [`call_vote`] is `Cmd_CallVote_f` (`codemp/game/g_cmds.c:2139`) with the
//! `validVoteStrings` table (`:2096`) and its handlers (`G_Vote*`, `:1873-2085`);
//! [`cast_vote`] is `Cmd_Vote_f` (`:2288`); [`check_vote`] is `CheckVote`
//! (`g_main.c:2571`), which `G_RunFrame` runs once a frame. A vote is decided in a
//! frame, never in the command that cast it.
//!
//! The rules read a description of the clients and write only [`Vote`]; what they say
//! comes back as [`Said`] in the order the reference says it. What a passed vote *does*
//! is a console command — [`Said::Execute`] — and carrying it out is the server's
//! business, not this module's.

use crate::match_end::{GT_CTF, GT_DUEL, GT_FFA, GT_POWERDUEL, GT_SIEGE, TEAM_SPECTATOR};

/// `CS_VOTE_TIME` (`bg_public.h:112`): when the running vote began, or empty.
pub const CS_VOTE_TIME: usize = 8;
/// `CS_VOTE_STRING`: what the running vote would do, as players are shown it.
pub const CS_VOTE_STRING: usize = 9;
/// `CS_VOTE_YES`.
pub const CS_VOTE_YES: usize = 10;
/// `CS_VOTE_NO`.
pub const CS_VOTE_NO: usize = 11;
/// `VOTE_TIME` (`bg_public.h:72`): how long a vote stays open.
pub const VOTE_TIME: i32 = 30_000;
/// `g_voteDelay`'s default (`g_xcvar.h`): the pause between a vote passing and it
/// being carried out, for the votes that ask for one.
pub const DEFAULT_VOTE_DELAY: i32 = 3000;
/// `g_allowVote`'s default: every vote in the table.
pub const DEFAULT_ALLOW_VOTE: i32 = -1;
/// `GT_SINGLE_PLAYER`, which a gametype vote refuses.
const GT_SINGLE_PLAYER: i32 = 5;
/// `GT_MAX_GAME_TYPE`.
const GT_MAX_GAME_TYPE: i32 = 10;
/// `GTB_ALL` (`bg_public.h:264`). It stops short of `GTB_CTY`, so in the reference a
/// vote marked "all gametypes" is not applicable to capture the ysalamiri.
const GTB_ALL: u32 = 0x1FF;
/// `GTB_SIEGE`.
const GTB_SIEGE: u32 = 1 << GT_SIEGE;
/// `GTB_CTF`.
const GTB_CTF: u32 = 1 << GT_CTF;
/// `GTB_CTY`.
const GTB_CTY: u32 = 1 << 9;

/// What a vote asks for, one entry of `validVoteStrings`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoteKind {
    /// `capturelimit <num>`.
    Capturelimit,
    /// `clientkick <clientnum>`.
    Clientkick,
    /// `fraglimit <num>`.
    Fraglimit,
    /// `g_doWarmup <0-1>`.
    Warmup,
    /// `g_gametype <num or name>`.
    Gametype,
    /// `kick <client name>`.
    Kick,
    /// `map <name>`.
    Map,
    /// `map_restart <optional delay>`.
    MapRestart,
    /// `nextmap`.
    Nextmap,
    /// `timelimit <num>`.
    Timelimit,
}

/// One row of `validVoteStrings` (`g_cmds.c:2096`). Its position in [`VOTES`] is its bit
/// in `g_allowVote`.
#[derive(Clone, Copy, Debug)]
pub struct VoteEntry {
    /// The vote's own name, which is what is always shown.
    pub string: &'static str,
    /// The other names a caller may use for it.
    pub aliases: &'static [&'static str],
    /// Which vote it is.
    pub kind: VoteKind,
    /// The arguments it requires, not counting optional ones.
    pub arguments: usize,
    /// The gametypes it applies to, one bit each.
    pub gametypes: u32,
    /// Whether passing waits `g_voteDelay` before it is carried out.
    pub delayed: bool,
    /// The argument help, or `None` when it takes none.
    pub help: Option<&'static str>,
}

/// `validVoteStrings`, in the reference's order — which is also the order of the bits
/// in `g_allowVote` and of the list an unknown vote is answered with.
pub const VOTES: [VoteEntry; 10] = [
    VoteEntry {
        string: "capturelimit",
        aliases: &["caps"],
        kind: VoteKind::Capturelimit,
        arguments: 1,
        gametypes: GTB_CTF | GTB_CTY,
        delayed: true,
        help: Some("<num>"),
    },
    VoteEntry {
        string: "clientkick",
        aliases: &[],
        kind: VoteKind::Clientkick,
        arguments: 1,
        gametypes: GTB_ALL,
        delayed: false,
        help: Some("<clientnum>"),
    },
    VoteEntry {
        string: "fraglimit",
        aliases: &["frags"],
        kind: VoteKind::Fraglimit,
        arguments: 1,
        gametypes: GTB_ALL & !(GTB_SIEGE | GTB_CTF | GTB_CTY),
        delayed: true,
        help: Some("<num>"),
    },
    VoteEntry {
        string: "g_doWarmup",
        aliases: &["dowarmup", "warmup"],
        kind: VoteKind::Warmup,
        arguments: 1,
        gametypes: GTB_ALL,
        delayed: true,
        help: Some("<0-1>"),
    },
    VoteEntry {
        string: "g_gametype",
        aliases: &["gametype", "gt", "mode"],
        kind: VoteKind::Gametype,
        arguments: 1,
        gametypes: GTB_ALL,
        delayed: true,
        help: Some("<num or name>"),
    },
    VoteEntry {
        string: "kick",
        aliases: &[],
        kind: VoteKind::Kick,
        arguments: 1,
        gametypes: GTB_ALL,
        delayed: false,
        help: Some("<client name>"),
    },
    VoteEntry {
        string: "map",
        aliases: &[],
        kind: VoteKind::Map,
        arguments: 0,
        gametypes: GTB_ALL,
        delayed: true,
        help: Some("<name>"),
    },
    VoteEntry {
        string: "map_restart",
        aliases: &["restart"],
        kind: VoteKind::MapRestart,
        arguments: 0,
        gametypes: GTB_ALL,
        delayed: true,
        help: Some("<optional delay>"),
    },
    VoteEntry {
        string: "nextmap",
        aliases: &[],
        kind: VoteKind::Nextmap,
        arguments: 0,
        gametypes: GTB_ALL,
        delayed: true,
        help: None,
    },
    VoteEntry {
        string: "timelimit",
        aliases: &["time"],
        kind: VoteKind::Timelimit,
        arguments: 1,
        gametypes: GTB_ALL & !GTB_SIEGE,
        delayed: true,
        help: Some("<num>"),
    },
];

/// `gameNames` (`g_cmds.c:1858`): how a gametype vote is shown.
const GAME_NAMES: [&str; GT_MAX_GAME_TYPE as usize] = [
    "Free For All",
    "Holocron FFA",
    "Jedi Master",
    "Duel",
    "Power Duel",
    "Single Player",
    "Team FFA",
    "Siege",
    "Capture the Flag",
    "Capture the Ysalamiri",
];

/// The cvars and server state a vote is read against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoteSettings {
    /// `g_allowVote`: zero turns voting off; otherwise bit `i` allows [`VOTES`]`[i]`.
    pub allow: i32,
    /// `g_voteDelay`, in milliseconds.
    pub delay: i32,
    /// `level.gametype`.
    pub gametype: i32,
    /// Whether the `nextmap` cvar is set, which a `nextmap` vote needs.
    pub nextmap: bool,
    /// `level.maxclients` (`sv_maxclients`): the client numbers a `clientkick` may name.
    pub max_clients: usize,
}

impl Default for VoteSettings {
    fn default() -> Self {
        Self {
            allow: DEFAULT_ALLOW_VOTE,
            delay: DEFAULT_VOTE_DELAY,
            gametype: GT_FFA,
            nextmap: false,
            max_clients: 32,
        }
    }
}

impl VoteSettings {
    /// Whether [`VOTES`]`[index]` may be called.
    fn allows(&self, index: usize) -> bool {
        self.allow & (1 << index) != 0
    }

    /// Whether spectators may vote: only in the two duel gametypes.
    fn spectators_vote(&self) -> bool {
        self.gametype == GT_DUEL || self.gametype == GT_POWERDUEL
    }
}

pub use crate::client_view::{ClientView as Voter, Connection};

/// How a client voted: `pers.vote`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ballot {
    /// Not voted, or voted before the running vote began.
    #[default]
    None,
    /// `1`.
    Yes,
    /// `2`.
    No,
}

/// Everything the level keeps about a vote: `level.voteTime`, `voteYes`, `voteNo`,
/// `voteExecuteTime`, `voteExecuteDelay`, the three strings, `votingGametype(To)`, and
/// each client's `PSG_VOTED` flag with its `pers.vote`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Vote {
    /// The millisecond the running vote began; zero when none is running.
    pub time: i32,
    /// Yes votes, the caller's included.
    pub yes: i32,
    /// No votes.
    pub no: i32,
    /// The millisecond a passed vote is carried out after; zero when none waits.
    pub execute_time: i32,
    /// The wait the running vote was called with.
    pub execute_delay: i32,
    /// The command a passed vote runs.
    pub string: Vec<u8>,
    /// What players are shown (`CS_VOTE_STRING`).
    pub display: Vec<u8>,
    /// What the prints quote: the command with quotes and line breaks removed.
    pub clean: Vec<u8>,
    /// The gametype a gametype vote asks for.
    pub gametype_to: Option<i32>,
    /// Who has voted in the running vote and how. A client number that is not here has
    /// not voted.
    ballots: Vec<(usize, Ballot)>,
    /// The red and the blue side's team votes (`level.teamVote*`).
    pub team: [crate::team_vote::TeamVote; 2],
}

impl Vote {
    /// Whether `client` has voted in the running vote, and which way.
    pub fn ballot(&self, client: usize) -> Ballot {
        self.ballots
            .iter()
            .find(|(voter, _)| *voter == client)
            .map_or(Ballot::None, |(_, ballot)| *ballot)
    }

    /// A slot's client left or a new one took it: `ClientConnect` clears `pers` and the
    /// game flags, so the slot has not voted.
    pub fn forget(&mut self, client: usize) {
        self.ballots.retain(|(voter, _)| *voter != client);
    }

    /// `G_ClearVote` (`g_client.c:3859-3873`): a client leaving the game or going to
    /// the spectators takes its ballot in the running vote back.
    pub fn clear(&mut self, client: usize) -> Vec<Said> {
        if self.time == 0 {
            return Vec::new();
        }
        let ballot = self.ballot(client);
        self.forget(client);
        match ballot {
            Ballot::Yes => {
                self.yes -= 1;
                vec![Said::ConfigString(
                    CS_VOTE_YES,
                    self.yes.to_string().into_bytes(),
                )]
            }
            Ballot::No => {
                self.no -= 1;
                vec![Said::ConfigString(
                    CS_VOTE_NO,
                    self.no.to_string().into_bytes(),
                )]
            }
            Ballot::None => Vec::new(),
        }
    }
}

/// Something the rules say, in the order they say it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Said {
    /// A server command for the client that sent the command.
    Caller(Vec<u8>),
    /// A server command for everyone.
    Everyone(Vec<u8>),
    /// A server command for one client (a team vote's prints go to its side).
    One(usize, Vec<u8>),
    /// `trap->SetConfigstring`.
    ConfigString(usize, Vec<u8>),
    /// `trap->SendConsoleCommand( EXEC_APPEND, ... )` without its newline: what a passed
    /// vote does.
    Execute(Vec<u8>),
    /// `trap->Cvar_Set(name, value)` at once (`G_RefreshNextMap`'s `nextmap`).
    SetCvar(&'static str, Vec<u8>),
    /// `G_KickAllBots`: a passed vote for siege kicks every bot (`clientkick`, inserted
    /// ahead of the map change).
    KickBots,
}

/// What a vote reads of the server beyond [`VoteSettings`]: the maps it has, the arena
/// files, the cvars a map vote keeps and a game-type vote corrects.
pub trait VoteWorld {
    /// `FS_Open("maps/<map>.bsp")` succeeds.
    fn map_exists(&self, map: &[u8]) -> bool;
    /// `level.arenas`.
    fn arenas(&self) -> &crate::arenas::Arenas;
    /// The `nextmap` cvar.
    fn nextmap(&self) -> &[u8];
    /// The `mapname` cvar.
    fn mapname(&self) -> &[u8];
    /// `g_autoMapCycle`.
    fn auto_map_cycle(&self) -> bool;
    /// `fraglimit` and `timelimit` (their integers) and `g_fraglimitVoteCorrection`.
    fn limits(&self) -> (i32, i32, bool);
}

/// A server with no maps and no arenas: a map vote finds nothing, a game-type vote's
/// follow-up has no map to go to.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoMaps;

impl VoteWorld for NoMaps {
    fn map_exists(&self, _: &[u8]) -> bool {
        false
    }
    fn arenas(&self) -> &crate::arenas::Arenas {
        static NONE: std::sync::OnceLock<crate::arenas::Arenas> = std::sync::OnceLock::new();
        NONE.get_or_init(Default::default)
    }
    fn nextmap(&self) -> &[u8] {
        b""
    }
    fn mapname(&self) -> &[u8] {
        b""
    }
    fn auto_map_cycle(&self) -> bool {
        false
    }
    fn limits(&self) -> (i32, i32, bool) {
        (20, 0, true)
    }
}

fn print(text: &str) -> Vec<u8> {
    format!("print \"{text}\n\"").into_bytes()
}

/// Byte strings back to back: what `Com_sprintf` builds, without assuming any of it is
/// UTF-8 — a player's name need not be.
fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

/// `MAX_CVAR_VALUE_STRING - 1`: `Cmd_CallVote_f` reads both arguments into buffers this
/// long, so anything past it is cut off before the vote is looked at.
const MAX_ARGUMENT: usize = 255;

/// `numVotingClients` as `CalculateRanks` counts it (`g_main.c:1089-1106`): connected,
/// not a bot, and playing — or watching a duel.
pub fn voting_clients(settings: &VoteSettings, voters: &[Voter]) -> i32 {
    voters
        .iter()
        .filter(|voter| {
            voter.connection == Connection::Connected
                && !voter.bot
                && (voter.team != TEAM_SPECTATOR || settings.spectators_vote())
        })
        .count() as i32
}

/// `Cmd_CallVote_f`. `arguments` is the whole tokenized command, `callvote` first.
pub fn call_vote(
    settings: &VoteSettings,
    vote: &mut Vote,
    voters: &[Voter],
    caller: usize,
    arguments: &[&[u8]],
    now: i32,
    world: &dyn VoteWorld,
) -> Vec<Said> {
    let mut said = Vec::new();
    let Some(me) = voters.iter().find(|voter| voter.client == caller) else {
        return said;
    };
    if settings.allow == 0 {
        said.push(Said::Caller(print("@@@NOVOTE")));
        return said;
    }
    if vote.time != 0 {
        said.push(Said::Caller(print("@@@VOTEINPROGRESS")));
        return said;
    }
    if !settings.spectators_vote() && me.team == TEAM_SPECTATOR {
        said.push(Said::Caller(print("@@@NOSPECVOTE")));
        return said;
    }
    let count = arguments.len();
    let arg1 = arguments.get(1).copied().unwrap_or_default();
    let arg1 = &arg1[..arg1.len().min(MAX_ARGUMENT)];
    // `ConcatArgs(2)`: every argument after the vote's name, joined by single spaces.
    let mut arg2 = crate::chat::concat_args(arguments, 2);
    arg2.truncate(MAX_ARGUMENT);
    if [arg1, &arg2]
        .iter()
        .any(|text| text.iter().any(|byte| matches!(byte, b';' | b'\r' | b'\n')))
    {
        said.push(Said::Caller(print("Invalid vote string.")));
        return said;
    }
    let Some((index, alias)) = find_vote(settings, arg1) else {
        said.push(Said::Caller(print("Invalid vote string.")));
        said.push(Said::Caller(
            b"print \"Allowed vote strings are: \"".to_vec(),
        ));
        said.push(Said::Caller(print(&allowed_list(settings))));
        return said;
    };
    let entry = &VOTES[index];
    // An alias is replaced by the vote's own name; the name itself is kept as the
    // caller spelled it, case and all.
    let arg1 = if alias { entry.string.as_bytes() } else { arg1 };
    if entry.gametypes & (1 << settings.gametype) == 0 {
        said.push(Said::Caller(cat(&[
            b"print \"",
            arg1,
            b" is not applicable in this gametype.\n\"",
        ])));
        return said;
    }
    if count < entry.arguments + 2 {
        let help = entry.help.unwrap_or("(null)").as_bytes();
        said.push(Said::Caller(cat(&[
            b"print \"",
            arg1,
            b" requires more arguments: ",
            help,
            b"\n\"",
        ])));
        return said;
    }
    vote.gametype_to = None;
    vote.execute_delay = if entry.delayed { settings.delay } else { 0 };
    // A vote still waiting to be carried out is carried out now, and the new one stored.
    if vote.execute_time != 0 {
        vote.execute_time = 0;
        said.push(Said::Execute(vote.string.clone()));
    }
    let Some(strings) = describe(
        entry.kind, arg1, &arg2, count, settings, voters, world, &mut said,
    ) else {
        return said;
    };
    let (string, display, gametype_to) = strings;
    vote.clean = string
        .iter()
        .copied()
        .filter(|byte| !matches!(byte, b'"' | b'\n' | b'\r'))
        .collect();
    vote.string = string;
    vote.display = display;
    vote.gametype_to = gametype_to;
    said.push(Said::Everyone(cat(&[
        b"print \"",
        &me.name,
        b"^7 @@@PLCALLEDVOTE (",
        &vote.clean,
        b")\n\"",
    ])));
    // The vote starts, and its caller has voted yes.
    vote.time = now;
    vote.yes = 1;
    vote.no = 0;
    vote.ballots.clear();
    vote.ballots.push((caller, Ballot::Yes));
    said.push(Said::ConfigString(
        CS_VOTE_TIME,
        vote.time.to_string().into_bytes(),
    ));
    said.push(Said::ConfigString(CS_VOTE_STRING, vote.display.clone()));
    said.push(Said::ConfigString(
        CS_VOTE_YES,
        vote.yes.to_string().into_bytes(),
    ));
    said.push(Said::ConfigString(
        CS_VOTE_NO,
        vote.no.to_string().into_bytes(),
    ));
    said
}

/// The row a caller's name picks among the allowed ones, and whether it picked it by an
/// alias. The reference walks the table once, trying each allowed row's own name and
/// then its aliases.
fn find_vote(settings: &VoteSettings, name: &[u8]) -> Option<(usize, bool)> {
    VOTES
        .iter()
        .enumerate()
        .filter(|(index, _)| settings.allows(*index))
        .find_map(|(index, entry)| {
            if entry.string.as_bytes().eq_ignore_ascii_case(name) {
                Some((index, false))
            } else {
                entry
                    .aliases
                    .iter()
                    .any(|alias| alias.as_bytes().eq_ignore_ascii_case(name))
                    .then_some((index, true))
            }
        })
}

/// "Allowed vote strings are: " and the list, green and yellow in turn.
fn allowed_list(settings: &VoteSettings) -> String {
    let mut list = String::new();
    for (shown, (_, entry)) in VOTES
        .iter()
        .enumerate()
        .filter(|(index, _)| settings.allows(*index))
        .enumerate()
    {
        let colour = if shown % 2 == 0 { '2' } else { '3' };
        match entry.help {
            Some(help) => list.push_str(&format!("^{colour}{} {help} ", entry.string)),
            None => list.push_str(&format!("^{colour}{} ", entry.string)),
        }
    }
    list
}

/// The vote's handler (`G_Vote*`): the command, what players are shown, and the
/// gametype a gametype vote asks for — or `None`, having said why, when the argument
/// is refused.
#[allow(clippy::too_many_arguments)]
fn describe(
    kind: VoteKind,
    arg1: &[u8],
    arg2: &[u8],
    count: usize,
    settings: &VoteSettings,
    voters: &[Voter],
    world: &dyn VoteWorld,
    said: &mut Vec<Said>,
) -> Option<(Vec<u8>, Vec<u8>, Option<i32>)> {
    let same = |string: Vec<u8>| Some((string.clone(), string, None));
    let with = |value: &dyn std::fmt::Display| cat(&[arg1, b" ", value.to_string().as_bytes()]);
    let arg2_text = String::from_utf8_lossy(arg2);
    match kind {
        VoteKind::Capturelimit | VoteKind::Fraglimit => same(with(&atoi(arg2).max(0))),
        VoteKind::Warmup => same(with(&atoi(arg2).clamp(0, 1))),
        VoteKind::MapRestart => same(with(&if count < 3 {
            5
        } else {
            atoi(arg2).clamp(0, 60)
        })),
        VoteKind::Timelimit => {
            let limit = atof(arg2).clamp(0.0, 35790.0);
            // `Q_isintegral`: `(int)f == f`.
            if (limit as i32) as f32 == limit {
                same(with(&(limit as i32)))
            } else {
                same(with(&format!("{limit:.3}")))
            }
        }
        VoteKind::Nextmap => {
            if !settings.nextmap {
                said.push(Said::Caller(print("nextmap not set.")));
                return None;
            }
            same(b"vstr nextmap".to_vec())
        }
        VoteKind::Clientkick => {
            let number = atoi(arg2);
            let Some(target) = usize::try_from(number)
                .ok()
                .filter(|&number| number < settings.max_clients)
            else {
                said.push(Said::Caller(print(&format!(
                    "invalid client number {number}."
                ))));
                return None;
            };
            let Some(target) = voters.iter().find(|voter| voter.client == target) else {
                said.push(Said::Caller(print(&format!(
                    "there is no client with the client number {number}."
                ))));
                return None;
            };
            Some((
                cat(&[arg1, b" ", arg2]),
                cat(&[arg1, b" ", &target.name]),
                None,
            ))
        }
        VoteKind::Kick => {
            let Some(target) = crate::client_view::client_number_from_string(voters, arg2, true)
            else {
                said.push(Said::Caller(cat(&[
                    b"print \"User ",
                    arg2,
                    b" is not on the server\n\"",
                ])));
                return None;
            };
            Some((
                format!("clientkick {}", target.client).into_bytes(),
                cat(&[b"kick ", &target.name]),
                None,
            ))
        }
        VoteKind::Gametype => {
            let mut gametype = atoi(arg2);
            if arg2.first().is_some_and(u8::is_ascii_alphabetic) {
                gametype = gametype_for_string(arg2).unwrap_or_else(|| {
                    said.push(Said::Caller(print(&format!(
                        "Gametype ({arg2_text}) unrecognised, defaulting to FFA/Deathmatch"
                    ))));
                    GT_FFA
                });
            } else if !(0..GT_MAX_GAME_TYPE).contains(&gametype) {
                said.push(Said::Caller(print(&format!(
                    "Gametype ({gametype}) is out of range, defaulting to FFA/Deathmatch"
                ))));
                gametype = GT_FFA;
            }
            if gametype == GT_SINGLE_PLAYER {
                said.push(Said::Caller(print(&format!(
                    "This gametype is not supported ({arg2_text})."
                ))));
                return None;
            }
            Some((
                with(&gametype),
                with(&GAME_NAMES[gametype as usize]),
                Some(gametype),
            ))
        }
        VoteKind::Map => describe_map(arg1, arg2, count, settings.gametype, world, said),
    }
}

/// `G_VoteMap` (`g_cmds.c:1988-2040`): no name lists the maps (`Cmd_MapList_f`); a name
/// with a backslash, a map the server lacks and one its arena does not give this game
/// type are refused; the rotation's `nextmap` is kept (`; set nextmap "…"`), and players
/// are shown the arena's long name.
fn describe_map(
    arg1: &[u8],
    arg2: &[u8],
    count: usize,
    gametype: i32,
    world: &dyn VoteWorld,
    said: &mut Vec<Said>,
) -> Option<(Vec<u8>, Vec<u8>, Option<i32>)> {
    if count < 3 {
        said.extend(
            world
                .arenas()
                .map_list(gametype)
                .into_iter()
                .map(Said::Caller),
        );
        return None;
    }
    if arg2.contains(&b'\\') {
        said.push(Said::Caller(
            b"print \"Can't have mapnames with a \\\n\"".to_vec(),
        ));
        return None;
    }
    if !world.map_exists(arg2) {
        said.push(Said::Caller(cat(&[
            b"print \"Can't find map maps/",
            arg2,
            b".bsp on server\n\"",
        ])));
        return None;
    }
    if !world.arenas().supports(arg2, gametype) {
        said.push(Said::Caller(print("@@@NOVOTE_MAPNOTSUPPORTEDBYGAME")));
        return None;
    }
    let nextmap = world.nextmap();
    let string = if nextmap.is_empty() {
        cat(&[arg1, b" ", arg2])
    } else {
        cat(&[arg1, b" ", arg2, b"; set nextmap \"", nextmap, b"\""])
    };
    let arena = world.arenas().info_for(arg2);
    let key = |key: &[u8]| {
        arena
            .and_then(|info| sjk_protocol::info_value(info, key))
            .filter(|value| !value.is_empty())
            .unwrap_or(b"ERROR")
    };
    let display = cat(&[b"map ", key(b"longname"), b" (", key(b"map"), b")"]);
    Some((string, display, None))
}

/// `BG_GetGametypeForString` (`bg_misc.c:3224`).
fn gametype_for_string(name: &[u8]) -> Option<i32> {
    const NAMES: [(&str, i32); 13] = [
        ("ffa", 0),
        ("dm", 0),
        ("holocron", 1),
        ("jm", 2),
        ("duel", 3),
        ("powerduel", 4),
        ("sp", 5),
        ("coop", 5),
        ("tdm", 6),
        ("tffa", 6),
        ("team", 6),
        ("siege", 7),
        ("ctf", 8),
    ];
    if name.eq_ignore_ascii_case(b"cty") {
        return Some(9);
    }
    NAMES
        .iter()
        .find(|(known, _)| known.as_bytes().eq_ignore_ascii_case(name))
        .map(|(_, gametype)| *gametype)
}

fn atoi(text: &[u8]) -> i32 {
    crate::userinfo::atoi(text)
}

/// `atof`: the longest prefix that reads as a decimal number, else zero.
fn atof(text: &[u8]) -> f32 {
    let bytes = text.trim_ascii_start();
    let mut end = 0;
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        end = 1;
    }
    let mut dot = false;
    while end < bytes.len() && (bytes[end].is_ascii_digit() || (!dot && bytes[end] == b'.')) {
        dot |= bytes[end] == b'.';
        end += 1;
    }
    std::str::from_utf8(&bytes[..end])
        .ok()
        .and_then(|number| number.parse::<f64>().ok())
        .map_or(0.0, |value| value as f32)
}

/// `Cmd_Vote_f`. `choice` is the command's first argument.
pub fn cast_vote(
    settings: &VoteSettings,
    vote: &mut Vote,
    voter: &Voter,
    choice: &[u8],
) -> Vec<Said> {
    if vote.time == 0 {
        return vec![Said::Caller(print("@@@NOVOTEINPROG"))];
    }
    if vote.ballot(voter.client) != Ballot::None {
        return vec![Said::Caller(print("@@@VOTEALREADY"))];
    }
    if !settings.spectators_vote() && voter.team == TEAM_SPECTATOR {
        return vec![Said::Caller(print("@@@NOVOTEASSPEC"))];
    }
    let mut said = vec![Said::Caller(print("@@@PLVOTECAST"))];
    // `tolower(msg[0]) == 'y' || msg[0] == '1'`; anything else is a no.
    if matches!(choice.first(), Some(b'y' | b'Y' | b'1')) {
        vote.yes += 1;
        vote.ballots.push((voter.client, Ballot::Yes));
        said.push(Said::ConfigString(
            CS_VOTE_YES,
            vote.yes.to_string().into_bytes(),
        ));
    } else {
        vote.no += 1;
        vote.ballots.push((voter.client, Ballot::No));
        said.push(Said::ConfigString(
            CS_VOTE_NO,
            vote.no.to_string().into_bytes(),
        ));
    }
    said
}

/// `CheckVote` (`g_main.c:2571`), once a frame: a passed vote carried out once its delay
/// is over, then the running vote passed, failed, timed out or left running.
/// `voting_clients` is [`voting_clients`] as the frame finds it.
///
/// Returns nothing, and allocates nothing, on a frame where nothing happens.
pub fn check_vote(
    vote: &mut Vote,
    voting_clients: i32,
    now: i32,
    gametype: i32,
    world: &dyn VoteWorld,
) -> Vec<Said> {
    let mut said = Vec::new();
    if vote.execute_time != 0 && vote.execute_time < now {
        vote.execute_time = 0;
        said.push(Said::Execute(vote.string.clone()));
        if let Some(to) = vote.gametype_to.take() {
            gametype_follow_up(gametype, to, world, &mut said);
        }
    }
    if vote.time == 0 {
        return said;
    }
    if now - vote.time >= VOTE_TIME || vote.yes + vote.no == 0 {
        said.push(Said::Everyone(cat(&[
            b"print \"@@@VOTEFAILED (",
            &vote.clean,
            b")\n\"",
        ])));
    } else if vote.yes > voting_clients / 2 {
        said.push(Said::Everyone(cat(&[
            b"print \"@@@VOTEPASSED (",
            &vote.clean,
            b")\n\"",
        ])));
        vote.execute_time = now + vote.execute_delay;
    } else if vote.no >= (voting_clients + 1) / 2 {
        // The same as running out of time.
        said.push(Said::Everyone(cat(&[
            b"print \"@@@VOTEFAILED (",
            &vote.clean,
            b")\n\"",
        ])));
    } else {
        return said;
    }
    vote.time = 0;
    said.push(Said::ConfigString(CS_VOTE_TIME, Vec::new()));
    said
}

/// What `CheckVote` does after a passed game-type vote's command (`g_main.c:2576-2628`):
/// to another game type, `nextmap` is refreshed for it and its map is loaded (siege kicks
/// the bots first); to the same one, `nextmap` is refreshed only with
/// `g_autoMapCycle`; and `g_fraglimitVoteCorrection` brings the limits into a duel's
/// range, or out of it.
fn gametype_follow_up(gametype: i32, to: i32, world: &dyn VoteWorld, said: &mut Vec<Said>) {
    let refreshed = world.arenas().refresh_next_map(
        world.mapname(),
        to,
        to != gametype,
        world.auto_map_cycle(),
    );
    if let Some((nextmap, _)) = &refreshed {
        said.push(Said::SetCvar("nextmap", nextmap.clone()));
    }
    if to != gametype {
        if to == GT_SIEGE {
            said.push(Said::KickBots);
        }
        if let Some((_, map)) = refreshed.filter(|(_, map)| !map.is_empty()) {
            said.push(Said::Execute(cat(&[b"map ", &map])));
        }
    }
    let (fraglimit, timelimit, correction) = world.limits();
    if correction {
        let duel = |gametype: i32| gametype == GT_DUEL || gametype == GT_POWERDUEL;
        if duel(to) && !duel(gametype) {
            if fraglimit > 3 || fraglimit == 0 {
                said.push(Said::Execute(b"fraglimit 3".to_vec()));
            }
            if timelimit != 0 {
                said.push(Said::Execute(b"timelimit 0".to_vec()));
            }
        } else if !duel(to) && duel(gametype) && fraglimit != 0 && fraglimit < 20 {
            said.push(Said::Execute(b"fraglimit 20".to_vec()));
        }
    }
}
