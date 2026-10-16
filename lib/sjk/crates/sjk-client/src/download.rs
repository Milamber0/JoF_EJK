//! Protocol-26 downloads before cgame activation (`cl_parse.cpp:634-719`).

use crate::ClientError;
use sjk_network::{LegacyConnection, NetworkError, ServerMessage};
use sjk_protocol::{GameState, InfoString, MessageReader, ServiceCommand};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// A validated remote request and its feed-independent referenced checksum.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PakRequest {
    /// Wire path, including the game-directory prefix.
    pub remote: String,
    /// Plain basename in the application's chosen write directory.
    pub filename: String,
    /// `sv_referencedPaks` checksum (not the pure checksum).
    pub checksum: i32,
}

/// Host-owned download capability. Called only on the connection worker.
pub trait DownloadStorage: Send {
    /// Reattach the worker's bounded progress channel and cancellation token on map changes.
    fn feedback(
        &mut self,
        progress: std::sync::mpsc::SyncSender<String>,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    );
    /// Check cancellation while waiting for server packets.
    fn check(&self) -> Result<(), String> {
        Ok(())
    }
    /// Compare references against a cached local checksum inventory; no disk I/O.
    fn missing(&self, game: &GameState) -> Result<Vec<PakRequest>, String>;
    /// Open a temporary file for an explicitly requested pak.
    fn begin(&mut self, request: &PakRequest) -> Result<(), String>;
    /// Accept the declared size before writing any payload.
    fn size(&mut self, bytes: u64) -> Result<(), String>;
    /// Append one in-order block and update progress.
    fn append(&mut self, bytes: &[u8]) -> Result<(), String>;
    /// Verify and publish a complete pak, then update the cached inventory.
    fn finish(&mut self) -> Result<(), String>;
    /// Remove temporary output on refusal, timeout, cancellation or malformed data.
    fn abort(&mut self);
    /// `request` was given up (refused by the server or failed verification)
    /// and the join goes on without it.
    fn skipped(&mut self, request: &PakRequest, reason: &str) {
        let _ = (request, reason);
    }
}

/// Parsed download block; the small payload is bounded by the stock message size.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Block {
    /// Low 16 bits of the monotonically increasing block index.
    pub(crate) number: u16,
    /// Present only on the initial block zero.
    pub(crate) size: Option<u64>,
    /// Empty payload denotes EOF, not a data-less keepalive.
    pub(crate) data: Vec<u8>,
}

/// Read precisely the fields written by `sv_client.cpp:859-870`.
pub(crate) fn block(reader: &mut MessageReader<'_>, first: bool) -> Result<Block, ClientError> {
    let number = reader.read_i16()? as u16;
    let size = if number == 0 && first {
        let size = reader.read_i32()?;
        if size < 0 {
            let reason = reader.read_c_string(1024)?;
            return Err(ClientError::Download(
                String::from_utf8_lossy(&reason).into_owned(),
            ));
        }
        Some(size as u64)
    } else {
        None
    };
    let length = reader.read_i16()?;
    // MSG_ReadShort is signed; every nonnegative i16 fits stock MAX_MSGLEN (49152).
    if length < 0 {
        return Err(ClientError::Download(
            "invalid download block length".into(),
        ));
    }
    let mut data = Vec::with_capacity(length as usize);
    for _ in 0..length {
        data.push(reader.read_u8()?);
    }
    Ok(Block { number, size, data })
}

/// Transfer missing paks, then request and return a fresh gamestate.
pub(crate) fn run(
    connection: &mut impl Transport,
    message: ServerMessage,
    storage: &mut dyn DownloadStorage,
    sequence: &mut i32,
    pending: &mut VecDeque<(i32, Vec<u8>)>,
) -> Result<ServerMessage, ClientError> {
    let initial = sjk_protocol::decode_initial_gamestate(&message.payload)?;
    let requests = storage
        .missing(&initial.game_state)
        .map_err(ClientError::Download)?;
    if requests.is_empty() {
        return Ok(message);
    }
    let info = initial
        .game_state
        .config_string(1)
        .ok_or(ClientError::MissingSystemInfo)?;
    let info =
        InfoString::parse(std::str::from_utf8(info).map_err(|_| ClientError::NonUtf8SystemInfo)?)?;
    let server_id = info
        .get_i32("sv_serverid")
        .ok_or(ClientError::MissingServerId)?;
    for command in &initial.server_commands {
        connection.record_server_command(command.sequence, &command.command);
    }
    let mut exchange = Exchange {
        connection,
        sequence,
        pending,
        server_id,
        message_ack: message.sequence,
        reliable_ack: initial.game_state.server_command_sequence,
    };
    exchange.acknowledge(initial.reliable_acknowledge);
    let result = (|| {
        for request in &requests {
            // One refused or unusable pak skips that pak, never the join: the
            // references are not a required install list.
            if let Err(reason) = transfer(&mut exchange, storage, request, &initial.game_state)? {
                storage.abort();
                storage.check().map_err(ClientError::Download)?;
                // Stop any blocks still coming for it before asking for the next.
                exchange.queue(b"stopdl".to_vec())?;
                exchange.send()?;
                storage.skipped(request, &reason);
            }
        }
        // cl_main.cpp:1342-1353: filesystem restart, donedl, wait for gamestate.
        exchange.queue(b"donedl".to_vec())?;
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            storage.check().map_err(ClientError::Download)?;
            exchange.send()?;
            let Some(message) = exchange.receive()? else {
                continue;
            };
            if let Ok(initial) = sjk_protocol::decode_initial_gamestate(&message.payload) {
                let attempted = |missing: &PakRequest| {
                    requests
                        .iter()
                        .any(|tried| tried.checksum == missing.checksum)
                };
                if !storage
                    .missing(&initial.game_state)
                    .map_err(ClientError::Download)?
                    .iter()
                    .all(attempted)
                {
                    return Err(ClientError::Download(
                        "server changed required paks during download; reconnect to retry".into(),
                    ));
                }
                exchange.acknowledge(initial.reliable_acknowledge);
                for command in &initial.server_commands {
                    exchange
                        .connection
                        .record_server_command(command.sequence, &command.command);
                }
                return Ok(message);
            }
            let mut reader = MessageReader::new(&message.payload);
            exchange.acknowledge(reader.read_i32()?);
            loop {
                match reader.read_service_command()? {
                    ServiceCommand::End => break,
                    ServiceCommand::Nop => {}
                    ServiceCommand::ServerCommand => exchange.server_command(&mut reader)?,
                    ServiceCommand::SetGame => {
                        reader.read_c_string(256)?;
                    }
                    ServiceCommand::Snapshot => {
                        skip_snapshot(&mut reader, &message, &initial.game_state)?;
                    }
                    // A late terminator is harmless; never acknowledge it twice.
                    ServiceCommand::Download => {
                        block(&mut reader, false)?;
                    }
                    command => return Err(ClientError::UnexpectedCommand(command)),
                }
            }
        }
        Err(ClientError::Download("no gamestate after donedl".into()))
    })();
    if result.is_err() {
        storage.abort();
        let _ = exchange.queue(b"stopdl".to_vec());
        let _ = exchange.send();
    }
    result
}

/// Request one pak and store it, verified. `Ok(Err(reason))` gives up this pak
/// alone; `Err` ends the connection (network failure, timeout, disconnect, cancel).
fn transfer(
    exchange: &mut Exchange<'_, impl Transport>,
    storage: &mut dyn DownloadStorage,
    request: &PakRequest,
    game: &GameState,
) -> Result<Result<(), String>, ClientError> {
    // Quoted: a stock server reads the name as Cmd_Argv(1), and pak names can
    // hold spaces. Storage validation keeps quotes and control bytes out.
    if request.remote.bytes().any(|b| b == b'"' || b < b' ') {
        return Ok(Err("unsafe pak name".into()));
    }
    if let Err(reason) = storage.begin(request) {
        return Ok(Err(reason));
    }
    exchange.queue(format!("download \"{}\"", request.remote).into_bytes())?;
    let mut expected = 0_u32;
    let mut progress = Instant::now();
    let deadline = progress + Duration::from_secs(1800);
    loop {
        storage.check().map_err(ClientError::Download)?;
        // Silence means the server, not this pak: end the join as before.
        if progress.elapsed() > Duration::from_secs(30) || Instant::now() > deadline {
            return Err(ClientError::Download("download timed out".into()));
        }
        exchange.send()?;
        let Some(message) = exchange.receive()? else {
            continue;
        };
        let mut reader = MessageReader::new(&message.payload);
        exchange.acknowledge(reader.read_i32()?);
        loop {
            match reader.read_service_command()? {
                ServiceCommand::End => break,
                ServiceCommand::Nop => {}
                ServiceCommand::ServerCommand => exchange.server_command(&mut reader)?,
                ServiceCommand::SetGame => {
                    reader.read_c_string(256)?;
                }
                ServiceCommand::Snapshot => {
                    skip_snapshot(&mut reader, &message, game)?;
                }
                ServiceCommand::Download => {
                    // A refusal (`sv_client.cpp` writes size -1 and a reason).
                    let block = match block(&mut reader, expected == 0) {
                        Ok(block) => block,
                        Err(ClientError::Download(reason)) => return Ok(Err(reason)),
                        Err(error) => return Err(error),
                    };
                    if block.number != expected as u16 {
                        continue; // Stock ignores repeats/out-of-order blocks.
                    }
                    if let Some(size) = block.size
                        && let Err(reason) = storage.size(size)
                    {
                        return Ok(Err(reason));
                    }
                    if let Err(reason) = storage.append(&block.data) {
                        return Ok(Err(reason));
                    }
                    exchange.queue(format!("nextdl {expected}").into_bytes())?;
                    expected += 1;
                    progress = Instant::now();
                    if block.data.is_empty() {
                        let stored = storage.finish();
                        exchange.send()?;
                        exchange.send()?;
                        return Ok(stored);
                    }
                }
                command => return Err(ClientError::UnexpectedCommand(command)),
            }
        }
    }
}

// SV_SendClientSnapshot writes a full, NOT_ACTIVE snapshot before download blocks
// (`sv_snapshot.cpp:142-148,838-848`). Reuse the production decoder, not a second codec.
fn skip_snapshot(
    reader: &mut MessageReader<'_>,
    message: &ServerMessage,
    game: &GameState,
) -> Result<(), ClientError> {
    let snapshot = sjk_protocol::decode_base_snapshot(&message.payload, message.sequence, game)?;
    while reader.bit_position() < snapshot.consumed_bits {
        reader.read_bits(1)?;
    }
    Ok(())
}

struct Exchange<'a, T> {
    connection: &'a mut T,
    sequence: &'a mut i32,
    pending: &'a mut VecDeque<(i32, Vec<u8>)>,
    server_id: i32,
    message_ack: i32,
    reliable_ack: i32,
}

impl<T: Transport> Exchange<'_, T> {
    fn queue(&mut self, command: Vec<u8>) -> Result<(), ClientError> {
        if self.pending.len() >= 64 {
            return Err(ClientError::ReliableCommandOverflow);
        }
        *self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(ClientError::ReliableCommandOverflow)?;
        self.pending.push_back((*self.sequence, command));
        Ok(())
    }

    fn acknowledge(&mut self, ack: i32) {
        self.pending
            .retain(|(sequence, _)| *sequence > ack.min(*self.sequence));
    }

    fn send(&mut self) -> Result<(), ClientError> {
        let commands: Vec<_> = self
            .pending
            .iter()
            .map(|(n, s)| (*n, s.as_slice()))
            .collect();
        self.connection.send_reliable_commands(
            self.server_id,
            self.message_ack,
            self.reliable_ack,
            &commands,
        )?;
        Ok(())
    }

    fn receive(&mut self) -> Result<Option<ServerMessage>, ClientError> {
        match self
            .connection
            .receive_server_message(Duration::from_millis(100))
        {
            Ok(message) => {
                self.message_ack = message.sequence;
                Ok(Some(message))
            }
            Err(NetworkError::TimedOut(_)) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn server_command(&mut self, reader: &mut MessageReader<'_>) -> Result<(), ClientError> {
        let sequence = reader.read_i32()?;
        let text = reader.read_c_string(8192)?;
        self.connection.record_server_command(sequence, &text);
        self.reliable_ack = self.reliable_ack.max(sequence);
        if text.starts_with(b"disconnect") {
            return Err(ClientError::Download(
                String::from_utf8_lossy(&text).into_owned(),
            ));
        }
        if text.starts_with(b"print ") {
            eprintln!("download: {}", String::from_utf8_lossy(&text));
        }
        Ok(())
    }
}

/// Narrow transport seam for deterministic stock-format transcripts without sockets.
pub(crate) trait Transport {
    /// Retain reliable server text before acknowledging its sequence (packet XOR key).
    fn record_server_command(&mut self, sequence: i32, text: &[u8]);
    /// Send stock reliable commands without movement while CS_PRIMED.
    fn send_reliable_commands(
        &mut self,
        server: i32,
        message: i32,
        reliable: i32,
        commands: &[(i32, &[u8])],
    ) -> Result<(), NetworkError>;
    /// Receive a decoded netchan payload or a bounded timeout.
    fn receive_server_message(&mut self, timeout: Duration) -> Result<ServerMessage, NetworkError>;
}

impl Transport for LegacyConnection {
    fn record_server_command(&mut self, sequence: i32, text: &[u8]) {
        LegacyConnection::record_server_command(self, sequence, text);
    }

    fn send_reliable_commands(
        &mut self,
        server: i32,
        message: i32,
        reliable: i32,
        commands: &[(i32, &[u8])],
    ) -> Result<(), NetworkError> {
        LegacyConnection::send_reliable_commands(self, server, message, reliable, commands)
    }

    fn receive_server_message(&mut self, timeout: Duration) -> Result<ServerMessage, NetworkError> {
        LegacyConnection::receive_server_message(self, timeout)
    }
}
