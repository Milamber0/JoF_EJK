//! Read-only BaseJKA vote projection and reliable call-vote construction.

use sjk_protocol::GameState;
use std::fmt::{self, Write as _};

const CS_VOTE_TIME: usize = 8;
const CS_VOTE_STRING: usize = 9;
const CS_VOTE_YES: usize = 10;
const CS_VOTE_NO: usize = 11;
const CS_TEAMVOTE_TIME: usize = 12;
const CS_TEAMVOTE_STRING: usize = 14;
const CS_TEAMVOTE_YES: usize = 16;
const CS_TEAMVOTE_NO: usize = 18;
const VOTE_TIME_MILLIS: i32 = 30_000;

/// The scope of one active BaseJKA vote.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyVoteScope {
    /// The vote is visible to every connected player.
    Global,
    /// The vote belongs to the red team.
    RedTeam,
    /// The vote belongs to the blue team.
    BlueTeam,
}

/// Allocation-free view of one vote in the current gamestate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VoteState<'a> {
    /// Whether the server's vote-time configstring is non-zero.
    pub active: bool,
    /// Global or team vote scope.
    pub scope: LegacyVoteScope,
    /// Server-provided display string.
    pub text: &'a str,
    /// Current affirmative count.
    pub yes: i32,
    /// Current negative count.
    pub no: i32,
    /// Time remaining at the supplied cgame/server presentation time.
    pub remaining_ms: i32,
}

impl VoteState<'_> {
    /// Return the rounded-down seconds displayed by `CG_DrawVote`.
    pub const fn remaining_seconds(self) -> i32 {
        self.remaining_ms / 1_000
    }
}

/// Project the global vote from configstrings 8 through 11.
///
/// OpenJK defines these slots in `codemp/game/bg_public.h:98-101`, updates
/// them from reliable `cs` commands in `cg_servercmds.c:804-814`, and uses a
/// 30-second countdown in `cg_draw.c:6513-6578`.
pub fn legacy_global_vote(game: &GameState, server_time: i32) -> VoteState<'_> {
    vote_at(game, LegacyVoteScope::Global, 0, server_time)
}

/// Project the local player's team vote, or `None` for a non-team player.
///
/// Team slots are paired red/blue ranges at 12..19
/// (`bg_public.h:103-106`); `CG_DrawTeamVote` selects offset 0 for red and 1
/// for blue (`cg_draw.c:6585-6622`).
pub fn legacy_team_vote(game: &GameState, team: u8, server_time: i32) -> Option<VoteState<'_>> {
    match team {
        1 => Some(vote_at(game, LegacyVoteScope::RedTeam, 0, server_time)),
        2 => Some(vote_at(game, LegacyVoteScope::BlueTeam, 1, server_time)),
        _ => None,
    }
}

fn vote_at(
    game: &GameState,
    scope: LegacyVoteScope,
    offset: usize,
    server_time: i32,
) -> VoteState<'_> {
    let (time_index, string_index, yes_index, no_index) = match scope {
        LegacyVoteScope::Global => (CS_VOTE_TIME, CS_VOTE_STRING, CS_VOTE_YES, CS_VOTE_NO),
        LegacyVoteScope::RedTeam | LegacyVoteScope::BlueTeam => (
            CS_TEAMVOTE_TIME + offset,
            CS_TEAMVOTE_STRING + offset,
            CS_TEAMVOTE_YES + offset,
            CS_TEAMVOTE_NO + offset,
        ),
    };
    let start = config_i32(game, time_index);
    VoteState {
        active: start != 0,
        scope,
        text: config_text(game, string_index),
        yes: config_i32(game, yes_index),
        no: config_i32(game, no_index),
        remaining_ms: (VOTE_TIME_MILLIS - server_time.saturating_sub(start)).max(0),
    }
}

fn config_text(game: &GameState, index: usize) -> &str {
    game.config_string(index)
        .and_then(|value| std::str::from_utf8(value).ok())
        .unwrap_or("")
}

fn config_i32(game: &GameState, index: usize) -> i32 {
    config_text(game, index).parse().unwrap_or(0)
}

/// A validated BaseJKA `callvote` choice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LegacyCallVote<'a> {
    /// Restart the current map.
    MapRestart,
    /// Advance through the server's `nextmap` chain.
    NextMap,
    /// Change to the named map.
    Map(&'a str),
    /// Change `g_gametype` to the numeric legacy value.
    GameType(u8),
    /// Kick the player whose clean/display name matches this value.
    Kick(&'a str),
    /// Kick the player occupying a numeric client slot.
    ClientKick(u8),
    /// Enable or disable warmup.
    Warmup(bool),
    /// Set the time limit.
    TimeLimit(u32),
    /// Set the frag limit.
    FragLimit(u32),
}

/// Reusable-buffer errors from [`write_legacy_callvote`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyCallVoteError {
    /// OpenJK rejects semicolons and line breaks in either argument.
    UnsafeArgument,
}

impl fmt::Display for LegacyCallVoteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("vote argument contains a command separator or line break")
    }
}

/// Write the byte-exact reliable payload accepted by `Cmd_CallVote_f`.
///
/// The allowlist and argument names come from OpenJK
/// `codemp/game/g_cmds.c:2096-2109`; its parser rejects `;`, CR and LF at
/// `g_cmds.c:2165-2170`. The caller retains `output`, so menu activation does
/// not require a second command-formatting path.
pub fn write_legacy_callvote(
    output: &mut String,
    vote: LegacyCallVote<'_>,
) -> Result<(), LegacyCallVoteError> {
    output.clear();
    match vote {
        LegacyCallVote::MapRestart => output.push_str("callvote map_restart"),
        LegacyCallVote::NextMap => output.push_str("callvote nextmap"),
        LegacyCallVote::Map(value) => write_argument(output, "map", value)?,
        LegacyCallVote::GameType(value) => {
            let _ = write!(output, "callvote g_gametype {value}");
        }
        LegacyCallVote::Kick(value) => write_argument(output, "kick", value)?,
        LegacyCallVote::ClientKick(value) => {
            let _ = write!(output, "callvote clientkick {value}");
        }
        LegacyCallVote::Warmup(value) => {
            let _ = write!(output, "callvote g_doWarmup {}", u8::from(value));
        }
        LegacyCallVote::TimeLimit(value) => {
            let _ = write!(output, "callvote timelimit {value}");
        }
        LegacyCallVote::FragLimit(value) => {
            let _ = write!(output, "callvote fraglimit {value}");
        }
    }
    Ok(())
}

fn write_argument(output: &mut String, name: &str, value: &str) -> Result<(), LegacyCallVoteError> {
    if value.contains([';', '\r', '\n']) {
        return Err(LegacyCallVoteError::UnsafeArgument);
    }
    let _ = write!(output, "callvote {name} {value}");
    Ok(())
}
