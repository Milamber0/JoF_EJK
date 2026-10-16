//! Match timer and warmup projections from configstring-backed cgame state.

use sjk_protocol::GameState;

const CS_WARMUP: usize = 5;
const CS_LEVEL_START_TIME: usize = 21;

/// Elapsed match clock projected exactly as `CG_DrawTimer`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MatchClock {
    /// Whole elapsed minutes.
    pub minutes: i32,
    /// Seconds within the current minute.
    pub seconds: i32,
}

/// Warmup text state from `CG_DrawWarmup`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WarmupText {
    /// No warmup message is active.
    Hidden,
    /// The server is waiting for enough players.
    WaitingForPlayers,
    /// Match start countdown in displayed seconds.
    StartsIn(i32),
}

/// Read `CS_LEVEL_START_TIME` on every projection, so `cs` updates take effect.
pub fn legacy_match_clock(game: &GameState, server_time: i32) -> MatchClock {
    let start = config_i32(game, CS_LEVEL_START_TIME);
    let total = server_time.wrapping_sub(start) / 1_000;
    MatchClock {
        minutes: total / 60,
        seconds: total % 60,
    }
}

/// Read `CS_WARMUP` and reproduce the displayed `sec + 1` countdown.
pub fn legacy_warmup_text(game: &GameState, server_time: i32) -> WarmupText {
    let warmup = config_i32(game, CS_WARMUP);
    if warmup == 0 {
        WarmupText::Hidden
    } else if warmup < 0 {
        WarmupText::WaitingForPlayers
    } else {
        let seconds = warmup.wrapping_sub(server_time) / 1_000;
        if seconds < 0 {
            WarmupText::Hidden
        } else {
            WarmupText::StartsIn(seconds + 1)
        }
    }
}

fn config_i32(game: &GameState, index: usize) -> i32 {
    game.config_string(index)
        .and_then(|value| std::str::from_utf8(value).ok())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}
