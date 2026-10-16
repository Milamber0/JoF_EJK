//! The pre-match warmup (`g_doWarmup`, `g_warmup`): `level.warmupTime` as OpenJK's
//! `codemp/game` keeps it.
//!
//! - [`at_level_start`] is `SP_worldspawn`'s (`g_spawn.c:1480-1488`): a level begun by the
//!   warmup's own restart (`g_restarted`) has none; otherwise `g_doWarmup` starts one in
//!   every game type but the duels and siege, which run their own.
//! - [`check`] is `CheckTournament`'s branch for the game types that are not duels
//!   (`g_main.c:2486-2544`): waiting while fewer than two play (in a team game, while a
//!   side is empty), then a countdown of `g_warmup - 1` seconds, then `map_restart 0`
//!   with `g_restarted` set.
//! - [`after_ranks`] is `CalculateRanks`' (`g_main.c:1122-1123`): `g_warmup 0` or siege
//!   ends any warmup.
//!
//! While `level.warmupTime` is not zero nobody scores (`AddScore`, `g_combat.c:469`) and
//! a time limit does not run (`CheckExitRules`).

use crate::match_end::{GT_DUEL, GT_POWERDUEL, GT_SIEGE};

/// `GT_TEAM`: from here on a warmup waits for both teams.
const GT_TEAM: i32 = 6;

/// What a frame's warmup check asks of the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Nothing changes.
    Unchanged,
    /// Back to waiting for players: `CS_WARMUP` is `-1`, and the log says `Warmup:`.
    Waiting,
    /// The countdown started: `CS_WARMUP` is when it ends (or `0`: `g_warmup` of a second
    /// or less plays at once).
    Countdown(i32),
    /// The countdown ran out: `g_restarted 1` and `map_restart 0`.
    Restart,
}

/// Who is playing, as `CheckTournament` counts them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Players {
    /// `level.numPlayingClients`.
    pub playing: i32,
    /// `TeamCount(-1, TEAM_RED)` and `TeamCount(-1, TEAM_BLUE)`.
    pub red: i32,
    pub blue: i32,
}

/// `SP_worldspawn`'s warmup: `Some(-1)` when the level begins waiting for players
/// (`CS_WARMUP` "-1", `Warmup:` in the log), `None` for no warmup. `restarted` is
/// `g_restarted`, which the caller clears.
pub fn at_level_start(do_warmup: bool, restarted: bool, gametype: i32) -> Option<i32> {
    (!restarted && do_warmup && !matches!(gametype, GT_DUEL | GT_POWERDUEL | GT_SIEGE))
        .then_some(-1)
}

/// `CalculateRanks`' last word on the warmup.
pub fn after_ranks(warmup_time: i32, g_warmup: i32, gametype: i32) -> i32 {
    if g_warmup == 0 || gametype == GT_SIEGE {
        0
    } else {
        warmup_time
    }
}

/// `CheckTournament` for a game type that is not a duel, once a frame. `warmup_time` is
/// `level.warmupTime`.
pub fn check(
    warmup_time: &mut i32,
    gametype: i32,
    players: Players,
    g_warmup: i32,
    level_time: i32,
) -> Step {
    if *warmup_time == 0 {
        return Step::Unchanged;
    }
    let not_enough = if gametype > GT_TEAM {
        players.red < 1 || players.blue < 1
    } else {
        players.playing < 2
    };
    if not_enough {
        if *warmup_time != -1 {
            *warmup_time = -1;
            return Step::Waiting;
        }
        return Step::Unchanged;
    }
    // (`g_warmup`'s modification count restarting the countdown is commented out in the
    // reference.)
    if *warmup_time < 0 {
        // "Fudge by -1 to account for extra delays."
        *warmup_time = if g_warmup > 1 {
            level_time + (g_warmup - 1) * 1_000
        } else {
            0
        };
        return Step::Countdown(*warmup_time);
    }
    if level_time > *warmup_time {
        *warmup_time += 10_000;
        return Step::Restart;
    }
    Step::Unchanged
}
