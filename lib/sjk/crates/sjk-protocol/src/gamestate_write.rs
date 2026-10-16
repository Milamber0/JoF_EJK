//! Synthesized gamestate messages used to begin legacy demo files.

use crate::{
    EntityState, EntityWriteError, GameState, LEGACY_ENTITY_FIELDS, MAX_LEGACY_MESSAGE_BYTES,
    MessageError, MessageWriter, ServiceCommand, write_delta_entity,
};
use std::error::Error;
use std::fmt;

/// Encode the synthetic first demo record produced by OpenJK `CL_Record_f`.
///
/// The payload contains the current reliable sequence, `svc_gamestate`, all
/// non-empty configstrings, entity baselines, client number, checksum feed,
/// the legacy RMG filler, and `svc_EOF` in the order at
/// `codemp/client/cl_main.cpp:341-394`.
pub fn write_initial_gamestate(
    reliable_sequence: i32,
    state: &GameState,
) -> Result<Vec<u8>, GameStateWriteError> {
    let mut message = MessageWriter::new(MAX_LEGACY_MESSAGE_BYTES);
    message.write_i32(reliable_sequence)?;
    // `CL_Record_f` synthesizes the obsolete RMG filler as zero rather than
    // copying any value from the source gamestate (`cl_main.cpp:382-383`).
    write_gamestate_block(
        &mut message,
        state.server_command_sequence,
        state.config_strings(),
        state.baselines(),
        state.client_num,
        state.checksum_feed,
    )?;
    message.write_u8(ServiceCommand::End as u8)?;
    Ok(message.finish()?)
}

/// Append `svc_gamestate` through the RMG filler to a message in progress.
///
/// This is the part a server's `SV_CreateClientGameStateMessage`
/// (`codemp/server/sv_client.cpp:438-469`) and a client's recorded demo share.
/// The caller writes what precedes it (the acknowledged client command, and for a
/// server any pending reliable commands) and the final `svc_EOF`. Configstrings
/// must be non-empty and in index order, baselines in entity-number order; the
/// reference produces both by scanning its arrays.
pub fn write_gamestate_block<'a>(
    message: &mut MessageWriter,
    server_command_sequence: i32,
    config_strings: impl IntoIterator<Item = (usize, &'a [u8])>,
    baselines: impl IntoIterator<Item = &'a EntityState>,
    client_num: i32,
    checksum_feed: i32,
) -> Result<(), GameStateWriteError> {
    message.write_u8(ServiceCommand::GameState as u8)?;
    message.write_i32(server_command_sequence)?;
    for (index, value) in config_strings {
        message.write_u8(ServiceCommand::ConfigString as u8)?;
        message.write_i16(index as i16)?;
        message.write_c_string(value)?;
    }
    let zero = EntityState::zero(0, &LEGACY_ENTITY_FIELDS);
    for baseline in baselines {
        message.write_u8(ServiceCommand::Baseline as u8)?;
        write_delta_entity(message, &zero, Some(baseline), true, &LEGACY_ENTITY_FIELDS)?;
    }
    message.write_u8(ServiceCommand::End as u8)?;
    message.write_i32(client_num)?;
    message.write_i32(checksum_feed)?;
    message.write_i16(0)?;
    Ok(())
}

/// Failure while synthesizing a demo gamestate payload.
#[derive(Debug)]
pub enum GameStateWriteError {
    /// Compressed-message serialization failed.
    Message(MessageError),
    /// An entity baseline could not be encoded.
    Entity(EntityWriteError),
}

impl fmt::Display for GameStateWriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(error) => error.fmt(formatter),
            Self::Entity(error) => error.fmt(formatter),
        }
    }
}

impl Error for GameStateWriteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Message(error) => Some(error),
            Self::Entity(error) => Some(error),
        }
    }
}

impl From<MessageError> for GameStateWriteError {
    fn from(value: MessageError) -> Self {
        Self::Message(value)
    }
}

impl From<EntityWriteError> for GameStateWriteError {
    fn from(value: EntityWriteError) -> Self {
        Self::Entity(value)
    }
}
