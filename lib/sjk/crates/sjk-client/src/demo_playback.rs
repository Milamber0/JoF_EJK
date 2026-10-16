//! Deterministic protocol-26 demo snapshot stream shared by tools and viewers.

use sjk_protocol::{
    DemoReader, GameState, MessageError, MessageReader, ServiceCommand, Snapshot, SnapshotError,
    decode_initial_gamestate, decode_snapshot,
};
use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::io::Read;

// Demo records can reference older packet backup entries than the live
// client's immediate interpolation pair. The established parity harness keeps
// 64 decoded snapshots, matching PACKET_BACKUP in codemp/qcommon/q_shared.h.
const SNAPSHOT_HISTORY: usize = 64;
const MAX_BIG_CONFIG_STRING_BYTES: usize = 8_191;

/// Metadata returned after accepting one demo snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DemoAdvance {
    /// Previous accepted server time, if this is not the first snapshot.
    pub previous_server_time: Option<i32>,
    /// Newly accepted authoritative server time.
    pub server_time: i32,
    /// Whether a replacement gamestate preceded this snapshot.
    pub map_changed: bool,
}

/// Streaming protocol-26 demo decoder with stock configstring command handling.
///
/// This owns the same snapshot history required by delta decoding in a live
/// client. Consumers apply each accepted snapshot to `LegacyWorldAdapter`, so
/// tools and the viewer cannot grow separate interpolation implementations.
pub struct DemoPlayback<R> {
    reader: DemoReader<R>,
    game_state: GameState,
    shader_remaps: crate::ShaderRemaps,
    config_string_dirty: sjk_protocol::ConfigStringDirty,
    history: VecDeque<Snapshot>,
    latest_snapshot: Option<Snapshot>,
    commands: OfflineServerCommands,
    first_server_time: Option<i32>,
    pending_map_change: bool,
    record_count: u64,
    snapshot_count: u64,
    skipped_delta_snapshots: u64,
}

impl<R: Read> DemoPlayback<R> {
    /// Decode the leading gamestate record and prepare snapshot playback.
    pub fn new(input: R) -> Result<Self, DemoPlaybackError> {
        let mut reader = DemoReader::new(input);
        let first = reader.next_record()?.ok_or(DemoPlaybackError::Empty)?;
        let initial = decode_initial_gamestate(&first.payload)?;
        let sequence = initial.game_state.server_command_sequence;
        Ok(Self {
            reader,
            shader_remaps: crate::ShaderRemaps::from_game_state(&initial.game_state),
            game_state: initial.game_state,
            config_string_dirty: sjk_protocol::ConfigStringDirty::default(),
            history: VecDeque::with_capacity(SNAPSHOT_HISTORY),
            latest_snapshot: None,
            commands: OfflineServerCommands::new(sequence),
            first_server_time: None,
            pending_map_change: false,
            record_count: 1,
            snapshot_count: 0,
            skipped_delta_snapshots: 0,
        })
    }

    /// Decode and accept the next usable snapshot in the stream.
    pub fn next_snapshot(&mut self) -> Result<Option<DemoAdvance>, DemoPlaybackError> {
        while let Some(record) = self.reader.next_record()? {
            self.record_count += 1;
            let delta_sequence = snapshot_delta_sequence(&record.payload, record.sequence).ok();
            let delta_base = delta_sequence.and_then(|sequence| {
                self.latest_snapshot
                    .as_ref()
                    .filter(|snapshot| snapshot.message_sequence == sequence)
                    .or_else(|| {
                        self.history
                            .iter()
                            .find(|snapshot| snapshot.message_sequence == sequence)
                    })
            });
            let snapshot = match decode_snapshot(
                &record.payload,
                record.sequence,
                &self.game_state,
                delta_base,
            ) {
                Ok(snapshot) => snapshot,
                Err(SnapshotError::UnexpectedCommand(ServiceCommand::GameState)) => {
                    let initial = decode_initial_gamestate(&record.payload)?;
                    let sequence = initial.game_state.server_command_sequence;
                    self.shader_remaps.reset(&initial.game_state);
                    self.game_state = initial.game_state;
                    self.config_string_dirty.mark_all();
                    self.commands = OfflineServerCommands::new(sequence);
                    self.history.clear();
                    self.latest_snapshot = None;
                    self.pending_map_change = true;
                    continue;
                }
                Err(SnapshotError::DeltaBaseRequired { .. }) => {
                    self.skipped_delta_snapshots += 1;
                    continue;
                }
                Err(SnapshotError::UnexpectedCommand(_)) => continue,
                Err(error) => return Err(error.into()),
            };
            for command in &snapshot.server_commands {
                self.commands.apply(
                    command.sequence,
                    &command.command,
                    &mut self.game_state,
                    &mut self.config_string_dirty,
                    &mut self.shader_remaps,
                )?;
            }
            let previous_server_time = self
                .latest_snapshot
                .as_ref()
                .map(|previous| previous.server_time);
            if let Some(previous) = self.latest_snapshot.replace(snapshot) {
                if self.history.len() == SNAPSHOT_HISTORY {
                    self.history.pop_front();
                }
                self.history.push_back(previous);
            }
            let snapshot = self
                .latest_snapshot
                .as_ref()
                .expect("snapshot was installed");
            self.first_server_time.get_or_insert(snapshot.server_time);
            self.snapshot_count += 1;
            let map_changed = std::mem::take(&mut self.pending_map_change);
            return Ok(Some(DemoAdvance {
                previous_server_time,
                server_time: snapshot.server_time,
                map_changed,
            }));
        }
        Ok(None)
    }

    /// Current gamestate, including configstring updates seen during playback.
    pub fn game_state(&self) -> &GameState {
        &self.game_state
    }

    /// Pending notifications for snapshot consumers; the frame owner drains them later.
    pub fn config_string_changes(&self) -> &sjk_protocol::ConfigStringDirty {
        &self.config_string_dirty
    }

    /// Consume changed configstring indices once, without allocation.
    pub fn drain_config_string_changes(&mut self, visit: impl FnMut(usize)) {
        self.config_string_dirty.drain(visit);
    }

    /// Most recently accepted snapshot.
    pub fn latest_snapshot(&self) -> Option<&Snapshot> {
        self.latest_snapshot.as_ref()
    }

    /// Latest snapshot at or before `server_time`, matching live-session lookup.
    pub fn snapshot_at_or_before(&self, server_time: i32) -> Option<&Snapshot> {
        self.history
            .iter()
            .rev()
            .find(|snapshot| snapshot.server_time <= server_time)
            .or_else(|| self.latest_snapshot.as_ref())
    }

    /// Server time of the first accepted snapshot.
    pub fn first_server_time(&self) -> Option<i32> {
        self.first_server_time
    }

    /// Number of framed records consumed, including the initial gamestate.
    pub fn record_count(&self) -> u64 {
        self.record_count
    }

    /// Number of accepted snapshots.
    pub fn snapshot_count(&self) -> u64 {
        self.snapshot_count
    }

    /// Delta snapshots skipped because their base was unavailable.
    pub fn skipped_delta_snapshots(&self) -> u64 {
        self.skipped_delta_snapshots
    }
}

/// Fill a reused vector with fixed-step presentation times for one snapshot.
///
/// The current snapshot time is always included. Calling code can therefore
/// apply the snapshot once and sample interpolation at every returned time.
pub fn legacy_presentation_times(
    previous_server_time: Option<i32>,
    server_time: i32,
    step_millis: i32,
    output: &mut Vec<i64>,
) {
    output.clear();
    let step = step_millis.max(1);
    if let Some(previous) = previous_server_time {
        if server_time > previous {
            let mut time = previous.saturating_add(step);
            while time < server_time {
                output.push(i64::from(time));
                time = time.saturating_add(step);
            }
        }
    }
    output.push(i64::from(server_time));
}

fn snapshot_delta_sequence(payload: &[u8], sequence: i32) -> Result<i32, MessageError> {
    let mut message = MessageReader::new(payload);
    message.read_i32()?;
    loop {
        match message.read_service_command()? {
            ServiceCommand::End => return Ok(-1),
            ServiceCommand::Nop => {}
            ServiceCommand::ServerCommand => {
                message.read_i32()?;
                message.read_c_string(16_383)?;
            }
            ServiceCommand::Snapshot => {
                message.read_i32()?;
                let delta = message.read_u8()?;
                return Ok(if delta == 0 {
                    -1
                } else {
                    sequence - i32::from(delta)
                });
            }
            command => return Err(MessageError::UnknownServiceCommand(command as u8)),
        }
    }
}

#[path = "demo_server_commands.rs"]
mod server_commands;
use server_commands::OfflineServerCommands;

fn parse_index(value: Option<&Vec<u8>>) -> Result<usize, DemoPlaybackError> {
    let value = value.ok_or(DemoPlaybackError::MissingConfigIndex)?;
    let text = std::str::from_utf8(value).map_err(|_| DemoPlaybackError::InvalidConfigIndex)?;
    text.parse()
        .map_err(|_| DemoPlaybackError::InvalidConfigIndex)
}

fn tokenize_command(command: &[u8]) -> Vec<Vec<u8>> {
    let mut output = Vec::new();
    let mut cursor = 0;
    while cursor < command.len() {
        while cursor < command.len() && command[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor == command.len() {
            break;
        }
        let quoted = command[cursor] == b'"';
        if quoted {
            cursor += 1;
        }
        let start = cursor;
        while cursor < command.len()
            && if quoted {
                command[cursor] != b'"'
            } else {
                !command[cursor].is_ascii_whitespace()
            }
        {
            cursor += 1;
        }
        output.push(command[start..cursor].to_vec());
        if quoted && cursor < command.len() {
            cursor += 1;
        }
    }
    output
}

/// Error produced while decoding a demo playback stream.
#[derive(Debug)]
pub enum DemoPlaybackError {
    /// Demo contains no leading gamestate record.
    Empty,
    /// Demo framing failed.
    Demo(sjk_protocol::DemoError),
    /// Initial or replacement gamestate decoding failed.
    GameState(sjk_protocol::GameStateError),
    /// Snapshot decoding failed.
    Snapshot(SnapshotError),
    /// Configstring storage rejected an update.
    ConfigString(sjk_protocol::GameStateError),
    /// A configstring command omitted its index.
    MissingConfigIndex,
    /// A configstring index was not valid decimal UTF-8.
    InvalidConfigIndex,
    /// A bcs continuation arrived without bcs0.
    UnexpectedBigConfigPart,
    /// A bcs continuation changed index mid-command.
    MismatchedBigConfigIndex { expected: usize, actual: usize },
    /// A bcs command exceeded the legacy reliable-command limit.
    BigConfigTooLarge,
}

impl fmt::Display for DemoPlaybackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("demo is empty"),
            Self::Demo(error) => write!(formatter, "{error}"),
            Self::GameState(error) => write!(formatter, "{error}"),
            Self::Snapshot(error) => write!(formatter, "{error}"),
            Self::ConfigString(error) => write!(formatter, "{error}"),
            Self::MissingConfigIndex => formatter.write_str("configstring index is missing"),
            Self::InvalidConfigIndex => formatter.write_str("configstring index is invalid"),
            Self::UnexpectedBigConfigPart => {
                formatter.write_str("big configstring continuation has no start")
            }
            Self::MismatchedBigConfigIndex { expected, actual } => write!(
                formatter,
                "big configstring changed index from {expected} to {actual}"
            ),
            Self::BigConfigTooLarge => formatter.write_str("big configstring is too large"),
        }
    }
}

impl Error for DemoPlaybackError {}

impl From<sjk_protocol::DemoError> for DemoPlaybackError {
    fn from(value: sjk_protocol::DemoError) -> Self {
        Self::Demo(value)
    }
}

impl From<sjk_protocol::GameStateError> for DemoPlaybackError {
    fn from(value: sjk_protocol::GameStateError) -> Self {
        Self::GameState(value)
    }
}

impl From<SnapshotError> for DemoPlaybackError {
    fn from(value: SnapshotError) -> Self {
        Self::Snapshot(value)
    }
}

impl From<MessageError> for DemoPlaybackError {
    fn from(value: MessageError) -> Self {
        Self::Snapshot(SnapshotError::Message(value))
    }
}

impl<R: Read> DemoPlayback<R> {
    /// Persistent shader aliases at the current playback position.
    pub fn shader_remaps(&self) -> &crate::ShaderRemaps {
        &self.shader_remaps
    }
    /// Local `clearRemaps`; a restart or later remap command applies again.
    pub fn clear_shader_remaps(&mut self) {
        self.shader_remaps.clear();
    }
}
