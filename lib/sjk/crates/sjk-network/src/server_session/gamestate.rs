//! `SV_SendClientGameState`: the first message of a map, and of a reconnect.
use super::{LegacyGameHost, Slot};
use crate::{LegacyClientPhase, ReliableError, ServerChannelError};
use sjk_protocol::{
    GameStateWriteError, MAX_BIG_INFO_STRING_BYTES, MAX_CONFIGSTRINGS, MAX_LEGACY_MESSAGE_BYTES,
    MessageWriter, ServiceCommand, write_gamestate_block,
};
use std::{error::Error, fmt};

impl Slot {
    /// Build the gamestate for wire client number `client` and queue it.
    ///
    /// A connected client becomes primed; an active one stays active, which is how
    /// a map change reaches players already in the game. Any pure validation is
    /// forgotten, and the message's sequence is remembered so that a client still
    /// acknowledging older messages is sent the gamestate again. Reliable commands
    /// not yet acknowledged travel in front of it, as in every server message.
    ///
    /// The channel must be free. Returns the message's size as the rate calculation
    /// counts it, before the final `svc_EOF`, and the message as queued.
    pub(super) fn queue_gamestate(
        &mut self,
        client: usize,
        game: &impl LegacyGameHost,
        now: i32,
    ) -> Result<(usize, Vec<u8>), LegacyGamestateError> {
        let wire = &mut self.wire;
        if wire.channel.has_pending_message() {
            return Err(LegacyGamestateError::Channel(
                ServerChannelError::PendingMessage,
            ));
        }
        // The reference cannot hold such strings at all. A server whose own tables
        // are larger must decide what a legacy client is shown; it is never cut off
        // here without anyone being told.
        if let Some((index, _)) = game.config_strings().find(|(index, value)| {
            *index >= MAX_CONFIGSTRINGS || value.len() > MAX_BIG_INFO_STRING_BYTES
        }) {
            return Err(LegacyGamestateError::Unrepresentable {
                config_string: index,
            });
        }
        let mut message = MessageWriter::new(MAX_LEGACY_MESSAGE_BYTES);
        message
            .write_i32(wire.reliable.counters().client_sequence)
            .map_err(GameStateWriteError::from)?;
        wire.reliable.write_pending(&mut message)?;
        write_gamestate_block(
            &mut message,
            wire.reliable.counters().server_sequence,
            game.config_strings(),
            game.baselines(),
            client as i32,
            game.checksum_feed(),
        )?;
        let bytes = (message.bit_position() >> 3) + 1;
        message
            .write_u8(ServiceCommand::End as u8)
            .map_err(GameStateWriteError::from)?;
        let payload = message.finish().map_err(GameStateWriteError::from)?;
        self.peer.gamestate_message = wire.channel.outgoing_sequence();
        super::ping::record_sent(&mut self.timings, wire.channel.outgoing_sequence(), now);
        self.demo
            .message_sent(wire.channel.outgoing_sequence(), &payload);
        wire.channel
            .queue_message(&payload, wire.reliable.last_client_command())?;
        if self.phase == LegacyClientPhase::Connected {
            self.phase = LegacyClientPhase::Primed;
        }
        (self.gamestate_due, self.pure_authentic, self.got_cp) = (false, false, false);
        Ok((bytes, payload))
    }
}

/// A gamestate that could not be built or queued; the client keeps waiting for it.
#[derive(Debug)]
pub(super) enum LegacyGamestateError {
    /// A configstring index or length protocol 26 cannot carry.
    Unrepresentable {
        /// Index of the first offending string.
        config_string: usize,
    },
    /// The map's configstrings and baselines exceed one legacy message.
    Write(GameStateWriteError),
    /// The client's reliable history is closed or exhausted.
    Reliable(ReliableError),
    /// The channel is still sending, or its sequence numbers are exhausted.
    Channel(ServerChannelError),
}

impl fmt::Display for LegacyGamestateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unrepresentable { config_string } => {
                write!(
                    f,
                    "configstring {config_string} cannot be represented in protocol 26"
                )
            }
            Self::Write(error) => error.fmt(f),
            Self::Reliable(error) => error.fmt(f),
            Self::Channel(error) => error.fmt(f),
        }
    }
}
impl Error for LegacyGamestateError {}
impl From<GameStateWriteError> for LegacyGamestateError {
    fn from(error: GameStateWriteError) -> Self {
        Self::Write(error)
    }
}
impl From<ReliableError> for LegacyGamestateError {
    fn from(error: ReliableError) -> Self {
        Self::Reliable(error)
    }
}
impl From<ServerChannelError> for LegacyGamestateError {
    fn from(error: ServerChannelError) -> Self {
        Self::Channel(error)
    }
}
