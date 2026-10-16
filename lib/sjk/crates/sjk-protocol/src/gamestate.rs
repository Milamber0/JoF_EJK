use crate::{
    EntityDeltaError, EntityField, EntityState, LEGACY_ENTITY_FIELDS, LEGACY_ENTITY_NUMBER_BITS,
    MAX_LEGACY_ENTITIES, MessageError, MessageReader, ReliableServerCommand, ServiceCommand,
    read_delta_entity,
};
use std::error::Error;
use std::fmt;

pub const MAX_CONFIGSTRINGS: usize = 1_700;
pub const MAX_GAMESTATE_CHARS: usize = 16_000;
pub const MAX_BIG_INFO_STRING_BYTES: usize = 8_191;

/// The state carried by JKA's initial `svc_gamestate` command.
///
/// Strings remain bytes until a subsystem chooses an encoding policy. This is
/// important for compatibility with old servers containing non-UTF-8 text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameState {
    pub server_command_sequence: i32,
    config_strings: Vec<Option<Vec<u8>>>,
    baselines: Vec<Option<EntityState>>,
    pub client_num: i32,
    pub checksum_feed: i32,
    pub rmg_marker: i16,
}

impl GameState {
    /// Empty state for a local game host using the compatibility presentation schema.
    /// This does not encode, decode or change any network message.
    pub fn empty_local(client_num: i32) -> Self {
        Self {
            server_command_sequence: 0,
            config_strings: vec![None; MAX_CONFIGSTRINGS],
            baselines: vec![None; MAX_LEGACY_ENTITIES],
            client_num,
            checksum_feed: 0,
            rmg_marker: 0,
        }
    }

    pub fn config_string(&self, index: usize) -> Option<&[u8]> {
        self.config_strings.get(index)?.as_deref()
    }

    pub fn config_strings(&self) -> impl Iterator<Item = (usize, &[u8])> {
        self.config_strings
            .iter()
            .enumerate()
            .filter_map(|(index, value)| value.as_deref().map(|value| (index, value)))
    }

    pub fn config_string_count(&self) -> usize {
        self.config_strings.iter().flatten().count()
    }

    /// Replace a validated string, returning whether its bytes changed.
    pub fn replace_config_string(
        &mut self,
        index: usize,
        value: Vec<u8>,
    ) -> Result<bool, GameStateError> {
        if index >= MAX_CONFIGSTRINGS {
            return Err(GameStateError::InvalidConfigStringUpdateIndex(index));
        }
        if value.len() > MAX_BIG_INFO_STRING_BYTES {
            return Err(GameStateError::ConfigStringTooLarge {
                actual_bytes: value.len(),
                maximum_bytes: MAX_BIG_INFO_STRING_BYTES,
            });
        }
        if self.config_string(index).unwrap_or_default() == value {
            return Ok(false);
        }
        let old_bytes = self.config_strings[index]
            .as_ref()
            .map_or(0, |old| old.len() + 1);
        let current_bytes = 1 + self
            .config_strings
            .iter()
            .flatten()
            .map(|string| string.len() + 1)
            .sum::<usize>();
        let next_bytes = current_bytes - old_bytes + value.len() + 1;
        if next_bytes > MAX_GAMESTATE_CHARS {
            return Err(GameStateError::ConfigStringStorageExceeded {
                maximum_bytes: MAX_GAMESTATE_CHARS,
            });
        }
        self.config_strings[index] = Some(value);
        Ok(true)
    }

    pub fn baseline(&self, number: usize) -> Option<&EntityState> {
        self.baselines.get(number)?.as_ref()
    }

    pub fn baselines(&self) -> impl Iterator<Item = &EntityState> {
        self.baselines.iter().flatten()
    }

    pub fn baseline_count(&self) -> usize {
        self.baselines.iter().flatten().count()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialGameStateMessage {
    pub reliable_acknowledge: i32,
    pub game_state: GameState,
    pub consumed_bits: usize,
    /// The reliable commands the server flushed ahead of the gamestate.
    ///
    /// `SV_CreateClientGameStateMessage` sends everything still pending
    /// before `svc_gamestate` precisely because the gamestate then sets the
    /// client's command sequence (`codemp/server/sv_client.cpp:429-433`).
    /// Dropping them leaves the client acknowledging commands whose text it
    /// never saw, and that text is the key both sides encode packets with
    /// (`cl_net_chan.cpp:64`), so everything reliable it sends afterwards —
    /// `cp` included — reaches the server as garbage.
    pub server_commands: Vec<ReliableServerCommand>,
}

pub fn decode_initial_gamestate(payload: &[u8]) -> Result<InitialGameStateMessage, GameStateError> {
    decode_initial_gamestate_with_schema(payload, &LEGACY_ENTITY_FIELDS)
}

pub fn decode_initial_gamestate_with_schema(
    payload: &[u8],
    entity_schema: &[EntityField],
) -> Result<InitialGameStateMessage, GameStateError> {
    let mut message = MessageReader::new(payload);
    let reliable_acknowledge = message.read_i32()?;
    let mut server_commands = Vec::new();
    loop {
        match message.read_service_command()? {
            ServiceCommand::GameState => break,
            ServiceCommand::ServerCommand => {
                let sequence = message.read_i32()?;
                let command = message.read_c_string(MAX_BIG_INFO_STRING_BYTES)?;
                server_commands.push(ReliableServerCommand { sequence, command });
            }
            ServiceCommand::Nop | ServiceCommand::MapChange => {}
            actual => {
                return Err(GameStateError::ExpectedCommand {
                    expected: ServiceCommand::GameState,
                    actual,
                });
            }
        }
    }
    let game_state = decode_gamestate_command(&mut message, entity_schema)?;
    expect_command(&mut message, ServiceCommand::End)?;

    Ok(InitialGameStateMessage {
        reliable_acknowledge,
        game_state,
        consumed_bits: message.bit_position(),
        server_commands,
    })
}

fn decode_gamestate_command(
    message: &mut MessageReader<'_>,
    entity_schema: &[EntityField],
) -> Result<GameState, GameStateError> {
    let server_command_sequence = message.read_i32()?;
    let mut config_strings = vec![None; MAX_CONFIGSTRINGS];
    let mut baselines = vec![None; MAX_LEGACY_ENTITIES];
    // The legacy string table reserves byte zero for the empty string.
    let mut string_storage_bytes = 1_usize;

    loop {
        match message.read_service_command()? {
            ServiceCommand::ConfigString => {
                let wire_index = message.read_i16()?;
                let index = usize::try_from(wire_index)
                    .ok()
                    .filter(|index| *index < MAX_CONFIGSTRINGS)
                    .ok_or(GameStateError::InvalidConfigStringIndex(wire_index))?;
                let mut value = message.read_c_string(MAX_BIG_INFO_STRING_BYTES)?;
                // MSG_ReadBigString neutralizes printf format markers before
                // placing the value in the legacy gamestate table.
                value.iter_mut().for_each(|byte| {
                    if *byte == b'%' {
                        *byte = b'.';
                    }
                });
                string_storage_bytes = string_storage_bytes
                    .checked_add(value.len() + 1)
                    .filter(|bytes| *bytes <= MAX_GAMESTATE_CHARS)
                    .ok_or(GameStateError::ConfigStringStorageExceeded {
                        maximum_bytes: MAX_GAMESTATE_CHARS,
                    })?;
                config_strings[index] = Some(value);
            }
            ServiceCommand::Baseline => {
                let number = message.read_bits(LEGACY_ENTITY_NUMBER_BITS)? as u16;
                let zero = EntityState::zero(number, entity_schema);
                let baseline = read_delta_entity(message, &zero, number, entity_schema)?;
                baselines[usize::from(number)] = Some(baseline);
            }
            ServiceCommand::End => break,
            actual => {
                return Err(GameStateError::UnexpectedInnerCommand { actual });
            }
        }
    }

    Ok(GameState {
        server_command_sequence,
        config_strings,
        baselines,
        client_num: message.read_i32()?,
        checksum_feed: message.read_i32()?,
        rmg_marker: message.read_i16()?,
    })
}

fn expect_command(
    message: &mut MessageReader<'_>,
    expected: ServiceCommand,
) -> Result<(), GameStateError> {
    let actual = message.read_service_command()?;
    if actual != expected {
        return Err(GameStateError::ExpectedCommand { expected, actual });
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GameStateError {
    Message(MessageError),
    EntityDelta(EntityDeltaError),
    ExpectedCommand {
        expected: ServiceCommand,
        actual: ServiceCommand,
    },
    UnexpectedInnerCommand {
        actual: ServiceCommand,
    },
    InvalidConfigStringIndex(i16),
    InvalidConfigStringUpdateIndex(usize),
    ConfigStringTooLarge {
        actual_bytes: usize,
        maximum_bytes: usize,
    },
    ConfigStringStorageExceeded {
        maximum_bytes: usize,
    },
}

impl fmt::Display for GameStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(error) => error.fmt(formatter),
            Self::EntityDelta(error) => error.fmt(formatter),
            Self::ExpectedCommand { expected, actual } => {
                write!(formatter, "expected {expected:?}, received {actual:?}")
            }
            Self::UnexpectedInnerCommand { actual } => {
                write!(formatter, "unexpected {actual:?} inside gamestate")
            }
            Self::InvalidConfigStringIndex(index) => {
                write!(formatter, "invalid configstring index {index}")
            }
            Self::InvalidConfigStringUpdateIndex(index) => {
                write!(formatter, "invalid configstring update index {index}")
            }
            Self::ConfigStringTooLarge {
                actual_bytes,
                maximum_bytes,
            } => write!(
                formatter,
                "configstring has {actual_bytes} bytes; maximum is {maximum_bytes}"
            ),
            Self::ConfigStringStorageExceeded { maximum_bytes } => write!(
                formatter,
                "gamestate configstrings exceed the {maximum_bytes}-byte legacy limit"
            ),
        }
    }
}

impl Error for GameStateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Message(error) => Some(error),
            Self::EntityDelta(error) => Some(error),
            _ => None,
        }
    }
}

impl From<MessageError> for GameStateError {
    fn from(value: MessageError) -> Self {
        Self::Message(value)
    }
}

impl From<EntityDeltaError> for GameStateError {
    fn from(value: EntityDeltaError) -> Self {
        Self::EntityDelta(value)
    }
}
