//! BaseJKA team-overlay command state.
//!
//! `CG_ParseTeamInfo` uses `TEAMINFO_OFFSET == 6`: after the row count each
//! row is client number, location, health, armor, current weapon, and powerup
//! bits (`codemp/cgame/cg_servercmds.c:90-118`). Invalid counts or client
//! numbers call `ERR_DROP`; SJK exposes the same failures to the session.

use sjk_protocol::GameState;
use std::error::Error;
use std::fmt;

/// Protocol-26 maximum used by `TEAM_MAXOVERLAY` and `MAX_CLIENTS`.
pub const MAX_TEAM_CLIENTS: usize = 32;
const TEAMINFO_OFFSET: usize = 6;
const CS_LOCATIONS: usize = 1_227;
const MAX_LOCATIONS: usize = 64;

/// One row of authoritative teammate state from a `tinfo` command.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TeamInfo {
    /// Client slot in the protocol-26 client array.
    pub client_num: u8,
    /// Index in the `CS_LOCATIONS` configstring range.
    pub location: i32,
    /// Authoritative health reported by the server.
    pub health: i32,
    /// Authoritative armor reported by the server.
    pub armor: i32,
    /// Current `WP_*` weapon index reported by the server.
    pub weapon: i32,
    /// Active powerup bitset reported by the server.
    pub powerups: i32,
}

/// Fixed-capacity equivalent of codemp's `sortedTeamPlayers` and client rows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TeamInfoTable {
    entries: [TeamInfo; MAX_TEAM_CLIENTS],
    len: usize,
    revision: u64,
}

impl Default for TeamInfoTable {
    fn default() -> Self {
        Self {
            entries: [TeamInfo::default(); MAX_TEAM_CLIENTS],
            len: 0,
            revision: 0,
        }
    }
}

impl TeamInfoTable {
    /// Apply tokenized `tinfo` arguments without allocating.
    pub fn apply_command(&mut self, arguments: &[Vec<u8>]) -> Result<(), TeamInfoError> {
        let count = legacy_atoi(argument(arguments, 1));
        let count = usize::try_from(count).map_err(|_| TeamInfoError::Count(count))?;
        if count > MAX_TEAM_CLIENTS {
            return Err(TeamInfoError::Count(count as i32));
        }

        let mut next = [TeamInfo::default(); MAX_TEAM_CLIENTS];
        for (row, entry) in next.iter_mut().take(count).enumerate() {
            let base = 2 + row * TEAMINFO_OFFSET;
            let client = legacy_atoi(argument(arguments, base));
            let client_num = u8::try_from(client)
                .ok()
                .filter(|client| usize::from(*client) < MAX_TEAM_CLIENTS)
                .ok_or(TeamInfoError::Client(client))?;
            *entry = TeamInfo {
                client_num,
                location: legacy_atoi(argument(arguments, base + 1)),
                health: legacy_atoi(argument(arguments, base + 2)),
                armor: legacy_atoi(argument(arguments, base + 3)),
                weapon: legacy_atoi(argument(arguments, base + 4)),
                powerups: legacy_atoi(argument(arguments, base + 5)),
            };
        }
        self.entries = next;
        self.len = count;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// Rows in the ordering supplied by the server.
    pub fn entries(&self) -> &[TeamInfo] {
        &self.entries[..self.len]
    }

    /// Monotonic cache key changed by every valid `tinfo` command.
    pub const fn revision(&self) -> u64 {
        self.revision
    }
}

/// Resolve a team-overlay location through `CS_LOCATIONS + location`.
///
/// Codemp falls back to `unknown` when the configstring is absent or empty
/// (`codemp/cgame/cg_draw.c`, `CG_DrawTeamOverlay`).
pub fn legacy_team_location(game_state: &GameState, location: i32) -> &str {
    usize::try_from(location)
        .ok()
        .filter(|location| *location < MAX_LOCATIONS)
        .and_then(|location| game_state.config_string(CS_LOCATIONS + location))
        .and_then(|value| std::str::from_utf8(value).ok())
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown")
}

/// Fatal validation errors matching `CG_ParseTeamInfo`'s `ERR_DROP` cases.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TeamInfoError {
    /// The declared row count exceeds the protocol client limit.
    Count(i32),
    /// A row names a client outside the protocol client limit.
    Client(i32),
}

impl fmt::Display for TeamInfoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Count(value) => write!(formatter, "team-info count out of range: {value}"),
            Self::Client(value) => write!(formatter, "team-info client out of range: {value}"),
        }
    }
}

impl Error for TeamInfoError {}

pub(crate) fn argument(arguments: &[Vec<u8>], index: usize) -> &[u8] {
    arguments.get(index).map_or(&[], Vec::as_slice)
}

/// C `atoi` behavior needed by cgame reliable commands.
pub(crate) fn legacy_atoi(bytes: &[u8]) -> i32 {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return 0;
    };
    let bytes = text.trim_start().as_bytes();
    let (negative, digits) = match bytes.first() {
        Some(b'-') => (true, &bytes[1..]),
        Some(b'+') => (false, &bytes[1..]),
        _ => (false, bytes),
    };
    let value =
        digits
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .fold(0_i32, |value, byte| {
                value
                    .saturating_mul(10)
                    .saturating_add(i32::from(*byte - b'0'))
            });
    if negative { -value } else { value }
}
