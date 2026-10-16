//! Legacy reliable command ownership; no sockets or game command implementation.
use crate::{LegacyClientHeader, LegacyClientPacket, ServerChannelError};
use sjk_protocol::{MessageError, MessageWriter, ServiceCommand};
use std::{error::Error, fmt};
mod client;
mod text;
pub use client::{ClientCommandDecision, ClientCommandPolicy};
use text::CommandText;
const HISTORY: usize = 128;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Lifetime {
    Open,
    Closing,
    Retained,
    Exhausted,
}

/// Observable protocol-26 reliable counters, copied without exposing mutation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LegacyReliableCounters {
    /// Last allocated reliable server command sequence.
    pub server_sequence: i32,
    /// Client's latest accepted reliable acknowledgement (stock permits future values).
    pub server_acknowledge: i32,
    /// Last reliable sequence written to an outbound compressed message.
    pub server_sent: i32,
    /// Client's latest message acknowledgement, including a rejected negative value.
    pub message_acknowledge: i32,
    /// Last dispatched reliable client command sequence.
    pub client_sequence: i32,
    /// Legacy flood-control timestamp, in server integer milliseconds.
    pub last_reliable_time: i32,
}

/// One client's reliable history and duplicate/flood decisions.
///
/// The command ring is a compatibility boundary, not the native server's command
/// capacity. Construction allocates the ring once. History, decoding and command
/// dispatch use fixed storage; the supplied writer/callback owns its own storage.
/// This is not a complete session: admission, map lifetime checks, client state,
/// pure/download policy, usercmd execution and command implementations are external.
pub struct LegacyReliableState {
    commands: Vec<CommandText>,
    last_client: CommandText,
    counters: LegacyReliableCounters,
    lifetime: Lifetime,
}
impl Default for LegacyReliableState {
    fn default() -> Self {
        Self::new()
    }
}
impl LegacyReliableState {
    /// Allocate an empty stock-sized reliable history for a newly admitted client.
    pub fn new() -> Self {
        Self {
            commands: vec![CommandText::default(); HISTORY],
            last_client: CommandText::default(),
            counters: LegacyReliableCounters::default(),
            lifetime: Lifetime::Open,
        }
    }
    /// Inspect counters; callers cannot advance them independently of history.
    pub fn counters(&self) -> LegacyReliableCounters {
        self.counters
    }
    /// Whether new client input is forbidden; closing peers may still queue final output.
    pub fn is_closed(&self) -> bool {
        self.lifetime != Lifetime::Open
    }
    pub(crate) fn begin_drop(&mut self) {
        if self.lifetime == Lifetime::Open {
            self.lifetime = Lifetime::Closing;
        }
    }
    pub(crate) fn finish_drop(&mut self) {
        if self.lifetime != Lifetime::Exhausted {
            self.lifetime = Lifetime::Retained;
        }
    }
    /// Last dispatched normalized client command, also the outbound channel key.
    pub fn last_client_command(&self) -> &[u8] {
        self.last_client.as_bytes()
    }
    /// Stock masked-ring key, including acknowledgement aliases.
    ///
    /// Selection happens before acknowledgement validation in the reference. This
    /// method does not assert that `sequence` was ever sent or may execute gameplay.
    pub fn server_command_key(&self, sequence: i32) -> &[u8] {
        self.commands[sequence as usize & (HISTORY - 1)].as_bytes()
    }
    /// Queue a server command after gamestate priming, returning its new sequence.
    ///
    /// Pass `primed=false` for pre-gamestate/disconnected clients to match the stock
    /// send gate. Text ends at NUL and is truncated to 1,023 bytes as in Q_strncpyz.
    /// Overflow forbids further input and requires a drop. Closing output remains
    /// writable for the reference's final broadcast/disconnect messages. The drop
    /// coordinator freezes new writes on entering zombie/free state, while keeping
    /// pending messages available for transmission.
    pub fn queue_server_command(
        &mut self,
        primed: bool,
        command: &[u8],
    ) -> Result<Option<i32>, ReliableError> {
        if matches!(self.lifetime, Lifetime::Retained | Lifetime::Exhausted) {
            return Err(ReliableError::Closed);
        }
        if !primed {
            return Ok(None);
        }
        let Some(next) = self.counters.server_sequence.checked_add(1) else {
            self.lifetime = Lifetime::Exhausted;
            return Err(ReliableError::SequenceExhausted);
        };
        self.counters.server_sequence = next;
        if i64::from(next) - i64::from(self.counters.server_acknowledge) == HISTORY as i64 + 1 {
            self.lifetime = Lifetime::Closing;
            return Err(ReliableError::ServerOverflow);
        }
        self.commands[next as usize & (HISTORY - 1)].set(command);
        Ok(Some(next))
    }
    /// A bot has read every command queued for it (`SV_BotGetConsoleMessage`, which
    /// the bot's thinking calls until none is left): all acknowledged.
    pub(crate) fn acknowledge_all(&mut self) {
        self.counters.server_acknowledge = self.counters.server_sequence;
    }
    /// Apply the initial acknowledgement checks from SV_ExecuteClientMessage.
    ///
    /// Returns false when the rest of the message must be ignored. Negative message
    /// acknowledgements leave the reliable acknowledgement untouched. Over-old
    /// reliable acknowledgements reset to the current sequence and ignore the body.
    /// Future/negative reliable values inside the stock window are preserved; map
    /// lifetime and gameplay admission checks must still run before body dispatch.
    pub fn acknowledge(&mut self, header: LegacyClientHeader) -> Result<bool, ReliableError> {
        if self.is_closed() {
            return Err(ReliableError::Closed);
        }
        self.counters.message_acknowledge = header.message_acknowledge;
        if header.message_acknowledge < 0 {
            return Ok(false);
        }
        self.counters.server_acknowledge = header.reliable_acknowledge;
        if i64::from(header.reliable_acknowledge)
            < i64::from(self.counters.server_sequence) - HISTORY as i64
        {
            self.counters.server_acknowledge = self.counters.server_sequence;
            return Ok(false);
        }
        Ok(true)
    }
    /// Decode a channel packet with its ring-selected key, then apply acknowledgement checks.
    ///
    /// On success, returns the header and whole decoded compressed payload. The
    /// caller must validate server lifetime/pure/session policy, then skip the three
    /// header longs before dispatching commands. `None` means ignore this body.
    pub fn decode<'a>(
        &mut self,
        packet: LegacyClientPacket<'a>,
    ) -> Result<Option<(LegacyClientHeader, &'a [u8])>, ReliableError> {
        if self.is_closed() {
            return Err(ReliableError::Closed);
        }
        let header = packet.header()?;
        let payload = packet.decode(self.server_command_key(header.reliable_acknowledge))?;
        Ok(self.acknowledge(header)?.then_some((header, payload)))
    }
    /// Borrow unacknowledged commands in resend order, including retained final output.
    ///
    /// During overflow closure the reference emits more than 128 sequence numbers
    /// through its masked ring. Preserve those aliases; only counter exhaustion
    /// makes serialization unavailable.
    pub fn pending_server_commands(&self) -> impl Iterator<Item = (i32, &[u8])> {
        let count = if self.lifetime == Lifetime::Exhausted {
            0
        } else {
            (i64::from(self.counters.server_sequence) - i64::from(self.counters.server_acknowledge))
                .max(0) as usize
        };
        (0..count).map(move |offset| {
            let sequence = (i64::from(self.counters.server_acknowledge) + 1 + offset as i64) as i32;
            (sequence, self.server_command_key(sequence))
        })
    }
    /// Append the reliable-command block, excluding the outer acknowledgement and EOF.
    ///
    /// A writer-capacity failure can leave partial output; discard that message.
    /// The sent counter changes only after the whole block is written successfully.
    pub fn write_pending(&mut self, writer: &mut MessageWriter) -> Result<(), ReliableError> {
        if self.lifetime == Lifetime::Exhausted {
            return Err(ReliableError::Closed);
        }
        for (sequence, command) in self.pending_server_commands() {
            writer.write_u8(ServiceCommand::ServerCommand as u8)?;
            writer.write_i32(sequence)?;
            writer.write_c_string(command)?;
        }
        self.counters.server_sent = self.counters.server_sequence;
        Ok(())
    }

    /// [`Self::write_pending`] for a copy of a message that is not sent (a server
    /// demo's gamestate): nothing is marked sent.
    pub fn write_pending_unsent(
        &mut self,
        writer: &mut MessageWriter,
    ) -> Result<(), ReliableError> {
        let sent = self.counters.server_sent;
        let written = self.write_pending(writer);
        self.counters.server_sent = sent;
        written
    }
}

/// A reliable lifecycle transition or bounded message decoding/writing failure.
#[derive(Debug)]
pub enum ReliableError {
    /// This operation is unavailable in the current closing/retained lifecycle.
    Closed,
    /// An unacknowledged server command would be overwritten; drop this client.
    ServerOverflow,
    /// A client skipped a required reliable command; drop this client.
    ClientGap,
    /// A native checked counter cannot represent another legacy sequence.
    SequenceExhausted,
    /// Channel header or tail decoding failed.
    Channel(ServerChannelError),
    /// Compressed-message parsing or writing failed.
    Message(MessageError),
}
impl fmt::Display for ReliableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => f.write_str("legacy reliable state is closed"),
            Self::ServerOverflow => f.write_str("server command overflow"),
            Self::ClientGap => f.write_str("lost reliable client commands"),
            Self::SequenceExhausted => f.write_str("legacy reliable sequence exhausted"),
            Self::Channel(error) => error.fmt(f),
            Self::Message(error) => error.fmt(f),
        }
    }
}
impl Error for ReliableError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Channel(error) => Some(error),
            Self::Message(error) => Some(error),
            _ => None,
        }
    }
}
impl From<ServerChannelError> for ReliableError {
    fn from(error: ServerChannelError) -> Self {
        Self::Channel(error)
    }
}
impl From<MessageError> for ReliableError {
    fn from(error: MessageError) -> Self {
        Self::Message(error)
    }
}
