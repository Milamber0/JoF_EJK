//! Protocol-26 server packet ownership. Admission, sockets and gameplay are separate.
use crate::xor_protocol_tail;
use sjk_protocol::{MessageError, MessageReader};
use std::{error::Error, fmt};
mod receive;
mod transmit;

/// One admitted client's legacy channel, with reusable send/reassembly storage.
///
/// The socket owner must route only packets from the admitted base address and
/// qport here, handle connectionless traffic separately, and own NAT port updates.
/// Creation allocates buffers; packet operations do not allocate scratch storage.
/// This is not a session: command history, acknowledgement validation, replay
/// policy, timeouts and rate scheduling belong to the caller.
pub struct LegacyServerChannel {
    challenge: i32,
    receive: receive::Receiver,
    transmit: transmit::Sender,
}

impl LegacyServerChannel {
    /// Create an empty channel after admission, using that client's challenge.
    pub fn new(challenge: i32) -> Self {
        Self {
            challenge,
            receive: receive::Receiver::new(),
            transmit: transmit::Sender::new(),
        }
    }

    /// Read the routing qport from a sequenced client datagram, without changing state.
    pub fn client_qport(packet: &[u8]) -> Result<u16, ServerChannelError> {
        receive::qport(packet)
    }

    /// Receive a client datagram; incomplete, duplicate or out-of-order packets return `None`.
    ///
    /// Preserves the reference's fragment sequencing, including its failure to
    /// advance `incomingSequence` on successful reassembly. Complete fragmented
    /// messages can therefore repeat; the session must suppress replayed commands.
    /// Malformed lengths return errors instead of reading beyond packet storage.
    pub fn receive(
        &mut self,
        packet: &[u8],
    ) -> Result<Option<LegacyClientPacket<'_>>, ServerChannelError> {
        let Some((sequence, qport)) = self.receive.process(packet)? else {
            return Ok(None);
        };
        Ok(Some(LegacyClientPacket {
            sequence,
            qport,
            challenge: self.challenge,
            payload: &mut self.receive.message,
        }))
    }

    /// Last accepted ordinary sequence; see [`Self::receive`] for fragmented messages.
    pub fn incoming_sequence(&self) -> i32 {
        self.receive.incoming
    }

    /// Reference `dropped` counter for the last newer candidate, including fragments.
    pub fn dropped_messages(&self) -> i32 {
        self.receive.dropped
    }

    /// Stage one complete Huffman payload and encode it using the acknowledged client command.
    ///
    /// The caller includes the final `svc_EOF` (as `SnapshotWriter` already does);
    /// this byte-oriented transport does not append another Huffman symbol.
    /// The command is the session's last executed client command, terminated at
    /// its first NUL. A pending message must be drained before another is queued.
    /// Returns the sequence used by every datagram belonging to this message.
    pub fn queue_message(
        &mut self,
        payload: &[u8],
        last_client_command: &[u8],
    ) -> Result<i32, ServerChannelError> {
        let command = command_bytes(last_client_command)?;
        self.transmit.queue(payload, self.challenge, command)
    }

    /// Emit one datagram into caller-owned storage, advancing the send cursor.
    ///
    /// Returns `None` when drained. A too-small destination leaves the cursor
    /// unchanged. The caller owns UDP send/error handling and fragment pacing.
    /// A destination of 1,308 bytes can hold every server datagram.
    pub fn next_datagram(
        &mut self,
        output: &mut [u8],
    ) -> Result<Option<usize>, ServerChannelError> {
        self.transmit.next(output)
    }

    /// Sequence the next queued message will carry (`netchan.outgoingSequence`).
    pub fn outgoing_sequence(&self) -> i32 {
        self.transmit.sequence
    }

    /// Bytes of the queued message not yet handed out (`unsentLength - unsentFragmentStart`).
    pub fn pending_bytes(&self) -> usize {
        self.transmit.pending_bytes()
    }

    /// Whether a queued message still has datagrams to emit.
    pub fn has_pending_message(&self) -> bool {
        self.transmit.pending
    }
}

/// The three Huffman-packed client fields needed before tail decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyClientHeader {
    /// Client's server lifetime identifier; the session validates it.
    pub server_id: i32,
    /// Server message acknowledged by this packet.
    pub message_acknowledge: i32,
    /// Reliable server command selecting the session's command-history key.
    pub reliable_acknowledge: i32,
}

/// A borrowed reassembled client payload, still encoded with the legacy XOR key.
pub struct LegacyClientPacket<'a> {
    /// Sequence from this datagram or fragment group.
    pub sequence: i32,
    /// Routing qport carried by the final received datagram.
    pub qport: u16,
    challenge: i32,
    payload: &'a mut [u8],
}

impl<'a> LegacyClientPacket<'a> {
    /// Read acknowledgement fields to select the correct server-command history entry.
    pub fn header(&self) -> Result<LegacyClientHeader, ServerChannelError> {
        let mut reader = MessageReader::new(self.payload);
        Ok(LegacyClientHeader {
            server_id: reader.read_i32()?,
            message_acknowledge: reader.read_i32()?,
            reliable_acknowledge: reader.read_i32()?,
        })
    }

    /// Consume the encoded view and decode it once, using the selected history key.
    ///
    /// Includes the three header fields in the returned compressed payload. The
    /// caller must still validate acknowledgements and parse commands/usercmds.
    /// Choosing the wrong history entry is a session error, not detectable here.
    pub fn decode(
        self,
        acknowledged_server_command: &[u8],
    ) -> Result<&'a [u8], ServerChannelError> {
        let command = command_bytes(acknowledged_server_command)?;
        let header = self.header()?;
        xor_protocol_tail(
            self.payload,
            12,
            self.challenge ^ header.server_id ^ header.message_acknowledge,
            command,
        );
        Ok(self.payload)
    }
}

fn command_bytes(bytes: &[u8]) -> Result<&[u8], ServerChannelError> {
    let bytes = bytes.split(|&byte| byte == 0).next().unwrap_or_default();
    if bytes.len() >= 1024 {
        return Err(ServerChannelError::CommandTooLong);
    }
    Ok(bytes)
}

/// A malformed packet or an explicit channel resource/lifecycle failure.
#[derive(Debug)]
pub enum ServerChannelError {
    /// Incomplete header, out-of-band prefix, invalid fragment length or oversized input.
    MalformedPacket,
    /// The complete payload exceeds the legacy message capacity.
    MessageTooLarge,
    /// Drain pending fragments before staging the next payload.
    PendingMessage,
    /// Caller-provided datagram storage is too small; the send cursor is unchanged.
    OutputTooSmall,
    /// Recreate the session before the sequence reaches the reserved OOB value.
    SequenceExhausted,
    /// Reliable command exceeds the legacy string capacity.
    CommandTooLong,
    /// A compressed client header is truncated or invalid.
    Message(MessageError),
}
impl fmt::Display for ServerChannelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedPacket => f.write_str("malformed legacy channel packet"),
            Self::MessageTooLarge => f.write_str("legacy channel message too large"),
            Self::PendingMessage => f.write_str("legacy channel still has pending datagrams"),
            Self::OutputTooSmall => f.write_str("datagram output storage too small"),
            Self::SequenceExhausted => f.write_str("legacy channel sequence exhausted"),
            Self::CommandTooLong => f.write_str("legacy channel command key too long"),
            Self::Message(error) => error.fmt(f),
        }
    }
}
impl Error for ServerChannelError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Message(error) => Some(error),
            _ => None,
        }
    }
}
impl From<MessageError> for ServerChannelError {
    fn from(error: MessageError) -> Self {
        Self::Message(error)
    }
}
