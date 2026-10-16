//! Team votes: `callteamvote`, `teamvote` and the frame that decides them — OpenJK's
//! `Cmd_CallTeamVote_f` with `G_TeamVoteLeader` (`codemp/game/g_cmds.c:2330-2440`),
//! `Cmd_TeamVote_f` (`:2447-2490`) and `CheckTeamVote` (`g_main.c:2737-2783`). The only
//! team vote is `leader`: who leads the red or the blue side (`SetLeader`).
//!
//! The reference's `CheckTeamVote` never carries a passed team vote out: it sets the
//! *level's* `voteExecuteTime` (`g_main.c:2769`), so the last ordinary vote's command runs
//! again three seconds on and no leader is made. [`check_team_vote`] does that under the
//! stock rules; otherwise it waits the same three seconds and names the leader.

use crate::client_view::{ClientView, Connection, client_number_from_string};
use crate::match_end::TEAM_SPECTATOR;
use crate::vote::{Said, VOTE_TIME, Vote};

/// `CS_TEAMVOTE_TIME`, `CS_TEAMVOTE_STRING`, `CS_TEAMVOTE_YES`, `CS_TEAMVOTE_NO`: each
/// the red side's, the blue side's one above it.
pub const CS_TEAMVOTE_TIME: usize = 12;
pub const CS_TEAMVOTE_STRING: usize = 14;
pub const CS_TEAMVOTE_YES: usize = 16;
pub const CS_TEAMVOTE_NO: usize = 18;
/// `TEAM_RED`, `TEAM_BLUE`, `GT_TEAM`.
const TEAM_RED: i32 = 1;
const TEAM_BLUE: i32 = 2;
const GT_TEAM: i32 = 6;
/// A passed team vote waits this long (`level.time + 3000`).
const TEAM_VOTE_DELAY: i32 = 3_000;

/// One side's team vote: `level.teamVote*[cs_offset]` and its voters' `PSG_TEAMVOTED`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TeamVote {
    /// When it began; zero when none runs.
    pub time: i32,
    pub yes: i32,
    pub no: i32,
    /// When a passed one is carried out (not under the stock rules); zero when none waits.
    pub execute_time: i32,
    /// `leader <client>`.
    pub string: Vec<u8>,
    /// Who has voted in it, and whether yes (`pers.teamvote` 1) or no (2).
    ballots: Vec<(usize, bool)>,
}

/// `cs_offset`: 0 for red, 1 for blue, none for anyone else.
fn side(team: i32) -> Option<usize> {
    match team {
        TEAM_RED => Some(0),
        TEAM_BLUE => Some(1),
        _ => None,
    }
}

fn print(text: &str) -> Vec<u8> {
    format!("print \"{text}\n\"").into_bytes()
}

/// `Cmd_CallTeamVote_f`. `arguments` is the whole command, `callteamvote` first;
/// `allowed` is `g_allowTeamVote`, `gametype` `g_gametype`.
pub fn call_team_vote(
    votes: &mut [TeamVote; 2],
    gametype: i32,
    allowed: bool,
    voters: &[ClientView],
    caller: usize,
    arguments: &[&[u8]],
    now: i32,
) -> Vec<Said> {
    let Some(me) = voters.iter().find(|voter| voter.client == caller) else {
        return Vec::new();
    };
    if gametype < GT_TEAM {
        return vec![Said::Caller(print(
            "Cannot call a team vote in a non-team gametype!",
        ))];
    }
    let Some(offset) = side(me.team) else {
        return Vec::new();
    };
    if !allowed {
        return vec![Said::Caller(print("@@@NOVOTE"))];
    }
    if votes[offset].time != 0 {
        return vec![Said::Caller(print("@@@TEAMVOTEALREADY"))];
    }
    let arg1 = arguments.get(1).copied().unwrap_or_default();
    let arg2 = crate::chat::concat_args(arguments, 2);
    let invalid = |text: &[u8]| text.iter().any(|byte| matches!(byte, b';' | b'\r' | b'\n'));
    if invalid(arg1) || invalid(&arg2) {
        return vec![Said::Caller(print("Invalid team vote string."))];
    }
    if !arg1.eq_ignore_ascii_case(b"leader") {
        return vec![
            Said::Caller(print("Invalid team vote string.")),
            Said::Caller(print(
                "Allowed team vote strings are: ^2leader <optional client name or number>",
            )),
        ];
    }
    // `G_TeamVoteLeader`: no name is the caller itself.
    let target = if arguments.len() == 2 {
        Some(me)
    } else {
        match client_number_from_string(voters, &arg2, false) {
            Some(target) => Some(target),
            None => {
                return vec![Said::Caller(
                    [
                        b"print \"User ".as_slice(),
                        &arg2,
                        b" is not on the server\n\"",
                    ]
                    .concat(),
                )];
            }
        }
    };
    let Some(target) = target else {
        return Vec::new();
    };
    if target.team != me.team {
        return vec![Said::Caller(
            [
                b"print \"User ".as_slice(),
                &arg2,
                b" is not on your team\n\"",
            ]
            .concat(),
        )];
    }
    let vote = &mut votes[offset];
    vote.string = format!("leader {}", target.client).into_bytes();
    let mut said: Vec<Said> = voters
        .iter()
        .filter(|voter| voter.team == me.team)
        .map(|voter| {
            Said::One(
                voter.client,
                [
                    b"print \"".as_slice(),
                    &me.name,
                    b"^7 called a team vote (",
                    &vote.string,
                    b")\n\"",
                ]
                .concat(),
            )
        })
        .collect();
    vote.time = now;
    vote.yes = 1;
    vote.no = 0;
    vote.ballots = vec![(caller, true)];
    said.push(Said::ConfigString(
        CS_TEAMVOTE_TIME + offset,
        now.to_string().into_bytes(),
    ));
    said.push(Said::ConfigString(
        CS_TEAMVOTE_STRING + offset,
        vote.string.clone(),
    ));
    said.push(Said::ConfigString(CS_TEAMVOTE_YES + offset, b"1".to_vec()));
    said.push(Said::ConfigString(CS_TEAMVOTE_NO + offset, b"0".to_vec()));
    said
}

/// `Cmd_TeamVote_f`.
pub fn cast_team_vote(votes: &mut [TeamVote; 2], voter: &ClientView, choice: &[u8]) -> Vec<Said> {
    let Some(offset) = side(voter.team) else {
        return Vec::new();
    };
    let vote = &mut votes[offset];
    if vote.time == 0 {
        return vec![Said::Caller(print("@@@NOTEAMVOTEINPROG"))];
    }
    if vote
        .ballots
        .iter()
        .any(|(client, _)| *client == voter.client)
    {
        return vec![Said::Caller(print("@@@TEAMVOTEALREADYCAST"))];
    }
    let mut said = vec![Said::Caller(print("@@@PLTEAMVOTECAST"))];
    let yes = matches!(choice.first(), Some(b'y' | b'Y' | b'1'));
    vote.ballots.push((voter.client, yes));
    if yes {
        vote.yes += 1;
        said.push(Said::ConfigString(
            CS_TEAMVOTE_YES + offset,
            vote.yes.to_string().into_bytes(),
        ));
    } else {
        vote.no += 1;
        said.push(Said::ConfigString(
            CS_TEAMVOTE_NO + offset,
            vote.no.to_string().into_bytes(),
        ));
    }
    said
}

/// `numteamVotingClients`: connected, not a bot, on the side.
pub fn team_voting_clients(voters: &[ClientView], team: i32) -> i32 {
    voters
        .iter()
        .filter(|voter| {
            voter.connection == Connection::Connected
                && !voter.bot
                && voter.team == team
                && team != TEAM_SPECTATOR
        })
        .count() as i32
}

/// What a frame's `CheckTeamVote` for one side decided beyond what it says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeamVoteOutcome {
    /// Nothing more.
    Nothing,
    /// `SetLeader(team, client)`.
    Leader(i32, usize),
}

/// `CheckTeamVote(team)` once a frame. `voting` is [`team_voting_clients`]; `level_vote` is
/// the ordinary vote, whose execute time the stock rules set.
pub fn check_team_vote(
    votes: &mut [TeamVote; 2],
    team: i32,
    voting: i32,
    now: i32,
    stock_rules: bool,
    level_vote: &mut Vote,
) -> (Vec<Said>, TeamVoteOutcome) {
    let Some(offset) = side(team) else {
        return (Vec::new(), TeamVoteOutcome::Nothing);
    };
    let vote = &mut votes[offset];
    let mut outcome = TeamVoteOutcome::Nothing;
    if vote.execute_time != 0 && vote.execute_time < now {
        vote.execute_time = 0;
        if let Some(client) = vote.string.strip_prefix(b"leader ") {
            outcome = TeamVoteOutcome::Leader(team, crate::userinfo::atoi(client).max(0) as usize);
        }
    }
    if vote.time == 0 {
        return (Vec::new(), outcome);
    }
    let clean: Vec<u8> = vote
        .string
        .iter()
        .copied()
        .filter(|byte| !matches!(byte, b'"' | b'\n' | b'\r'))
        .collect();
    let said = |verdict: &str| {
        Said::Everyone(
            [
                format!("print \"@@@{verdict} (").as_bytes(),
                &clean,
                b")\n\"",
            ]
            .concat(),
        )
    };
    let mut out = Vec::new();
    if now - vote.time >= VOTE_TIME || vote.yes + vote.no == 0 {
        out.push(said("TEAMVOTEFAILED"));
    } else if vote.yes > voting / 2 {
        out.push(said("TEAMVOTEPASSED"));
        if stock_rules {
            level_vote.execute_time = now + TEAM_VOTE_DELAY;
        } else {
            vote.execute_time = now + TEAM_VOTE_DELAY;
        }
    } else if vote.no >= (voting + 1) / 2 {
        out.push(said("TEAMVOTEFAILED"));
    } else {
        return (out, outcome);
    }
    vote.time = 0;
    out.push(Said::ConfigString(CS_TEAMVOTE_TIME + offset, Vec::new()));
    (out, outcome)
}

/// `G_ClearTeamVote(ent, team)` (`g_client.c:3875-3897`): a client leaving `team`'s side
/// (or the game) takes its ballot in that side's running vote back. The reference always
/// republishes the red side's string (`CS_TEAMVOTE_YES`, not `+ voteteam`).
pub fn clear_team_vote(votes: &mut [TeamVote; 2], client: usize, team: i32) -> Vec<Said> {
    let Some(offset) = side(team) else {
        return Vec::new();
    };
    let vote = &mut votes[offset];
    if vote.time == 0 {
        return Vec::new();
    }
    let Some(at) = vote.ballots.iter().position(|(voter, _)| *voter == client) else {
        return Vec::new();
    };
    let (_, yes) = vote.ballots.remove(at);
    if yes {
        vote.yes -= 1;
        vec![Said::ConfigString(
            CS_TEAMVOTE_YES,
            vote.yes.to_string().into_bytes(),
        )]
    } else {
        vote.no -= 1;
        vec![Said::ConfigString(
            CS_TEAMVOTE_NO,
            vote.no.to_string().into_bytes(),
        )]
    }
}

/// A client as `SetLeader` and `CheckTeamLeader` read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeaderView {
    pub client: usize,
    /// `sess.sessionTeam`.
    pub team: i32,
    /// `pers.connected != CON_DISCONNECTED`.
    pub connected: bool,
    pub bot: bool,
    /// `pers.netname`.
    pub name: Vec<u8>,
    /// `sess.teamLeader`.
    pub leader: bool,
}

/// `PrintTeam`: `text` to everyone on `team`.
fn print_team<'a>(
    views: &'a [LeaderView],
    team: i32,
    text: &[u8],
) -> impl Iterator<Item = Said> + 'a {
    let text = text.to_vec();
    views
        .iter()
        .filter(move |view| view.team == team)
        .map(move |view| Said::One(view.client, text.clone()))
}

/// `SetLeader(team, client)` (`g_main.c:2674-2697`): the side's leaders cleared, `client`
/// made one, the side told. Returns what is said and whose userinfo string changed
/// (`ClientUserinfoChanged`), in order.
pub fn set_leader(views: &mut [LeaderView], team: i32, client: usize) -> (Vec<Said>, Vec<usize>) {
    let Some(at) = views.iter().position(|view| view.client == client) else {
        return (Vec::new(), Vec::new());
    };
    let name = views[at].name.clone();
    if !views[at].connected {
        return (
            print_team(
                views,
                team,
                &[b"print \"".as_slice(), &name, b" is not connected\n\""].concat(),
            )
            .collect(),
            Vec::new(),
        );
    }
    if views[at].team != team {
        return (
            print_team(
                views,
                team,
                &[
                    b"print \"".as_slice(),
                    &name,
                    b" is not on the team anymore\n\"",
                ]
                .concat(),
            )
            .collect(),
            Vec::new(),
        );
    }
    let mut changed = Vec::new();
    for view in views
        .iter_mut()
        .filter(|view| view.team == team && view.leader)
    {
        view.leader = false;
        changed.push(view.client);
    }
    views[at].leader = true;
    changed.push(client);
    let said = print_team(
        views,
        team,
        &[b"print \"".as_slice(), &name, b" @@@NEWTEAMLEADER\n\""].concat(),
    )
    .collect();
    (said, changed)
}

/// `CheckTeamLeader(team)` (`g_main.c:2704-2728`): a side left without a leader gets one —
/// its first player who is not a bot, else its first. Nobody's userinfo is told.
pub fn check_team_leader(views: &mut [LeaderView], team: i32) {
    if views.iter().any(|view| view.team == team && view.leader) {
        return;
    }
    let pick = views
        .iter()
        .position(|view| view.team == team && !view.bot)
        .or_else(|| views.iter().position(|view| view.team == team));
    if let Some(at) = pick {
        views[at].leader = true;
    }
}
