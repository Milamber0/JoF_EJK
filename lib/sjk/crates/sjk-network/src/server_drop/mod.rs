//! Stock drop ordering and reliable closure, separate from sockets and game effects.
use crate::{LegacyReliableState, ReliableError};
use std::{error::Error, fmt};
mod commands;
mod drop;
pub use commands::send_legacy_server_command;
pub use drop::drop_legacy_client;

/// Stock connection lifecycle, confined to the legacy server adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum LegacyClientPhase {
    /// Unoccupied legacy session slot.
    Free,
    /// Disconnected peer retained temporarily for final output and stale input sinking.
    Zombie,
    /// Admitted, but gamestate has not yet primed the client.
    Connected,
    /// Gamestate sent; waiting to enter the world.
    Primed,
    /// Participating in the authoritative match.
    Active,
}

/// Synchronous effects required by the reference drop lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyDropEffect {
    /// Release this client's download resources.
    CloseDownload(usize),
    /// Run game disconnect, including removal of its authoritative actor.
    GameDisconnect(usize),
    /// Clear the bot's display name and game-side bot flag; phase is already free.
    FreeBot(usize),
    /// Clear userinfo and the display name through the session's normal update path.
    ClearUserinfo(usize),
    /// Finish demo recording and clear the recording flag before returning.
    StopDemo(usize),
    /// Schedule a master heartbeat because no connected clients remain.
    Heartbeat,
}

/// Access to the legacy session roster and synchronous game/resource effects.
///
/// Indices identify adapter slots, never native world entity IDs. Keep the roster
/// stable throughout a drop, including recursive drops triggered by broadcasts.
/// Effects must complete synchronously and maintain the documented name/recording
/// updates. Game callbacks may emit further server commands through this adapter.
/// Implementations own actual downloads, game entities, demos and heartbeat I/O;
/// this coordinator must not substitute event recording for those production effects.
pub trait LegacyDropHost {
    /// Number of stable legacy roster slots, including free slots.
    fn client_count(&self) -> usize;
    /// Current phase for a valid roster index.
    fn phase(&self, client: usize) -> LegacyClientPhase;
    /// Change the adapter phase, without changing native entity identity.
    fn set_phase(&mut self, client: usize, phase: LegacyClientPhase);
    /// Mutable reliable history for a valid roster index.
    fn reliable(&mut self, client: usize) -> &mut LegacyReliableState;
    /// Already validated legacy display name, interpreted as a NUL-terminated byte string.
    fn name(&self, client: usize) -> &[u8];
    /// Whether the transport peer is a bot (`NA_BOT`), independent of its game flags.
    fn is_bot(&self, client: usize) -> bool;
    /// Whether this client's server demo is currently being recorded.
    fn is_recording(&self, client: usize) -> bool;
    /// Apply an effect before the next drop operation runs.
    fn effect(&mut self, effect: LegacyDropEffect);
}

/// Invalid adapter slot or an unrecoverable reliable counter/lifecycle error.
#[derive(Debug)]
pub enum LegacyDropError {
    /// A directed operation referred to an index outside the host's roster.
    InvalidClient,
    /// A reliable state is inconsistent with its host phase or has exhausted counters.
    Reliable(ReliableError),
}
impl fmt::Display for LegacyDropError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidClient => f.write_str("invalid legacy client slot"),
            Self::Reliable(error) => error.fmt(f),
        }
    }
}
impl Error for LegacyDropError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Reliable(error) => Some(error),
            _ => None,
        }
    }
}
impl From<ReliableError> for LegacyDropError {
    fn from(error: ReliableError) -> Self {
        Self::Reliable(error)
    }
}
