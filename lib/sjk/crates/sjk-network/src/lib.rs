//! Portable socket transport and connectionless JKA discovery packets.

use sjk_protocol::{
    AdaptiveHuffmanError, InfoString, InfoStringError, MessageError, MessageReader, MessageWriter,
    UserCommand, compress_connect_block, legacy_command_hash, write_delta_user_command,
};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

mod server_configstring;
mod server_connect;
mod status;
pub use server_configstring::{
    LegacyConfigStringHost, LegacyConfigStringMarks, legacy_config_string_changed,
    legacy_config_string_commands, legacy_config_strings_catch_up,
};
pub use server_connect::{ConnectPacketError, decode_connect_packet};
mod connect_request;
mod server_challenge;
pub use connect_request::LegacyConnectRequest;
pub use server_challenge::LegacyChallenge;
mod server_channel;
pub use server_channel::{
    LegacyClientHeader, LegacyClientPacket, LegacyServerChannel, ServerChannelError,
};
mod server_reliable;
pub use server_reliable::{
    ClientCommandDecision, ClientCommandPolicy, LegacyReliableCounters, LegacyReliableState,
    ReliableError,
};
mod server_drop;
pub use server_drop::{
    LegacyClientPhase, LegacyDropEffect, LegacyDropError, LegacyDropHost, drop_legacy_client,
    send_legacy_server_command,
};
mod server_move;
mod server_session;
pub use server_session::{
    LEGACY_SNAPSHOT_ENTITIES, LegacyAutoDemoSettings, LegacyBotSlots, LegacyClock,
    LegacyConsoleSettings, LegacyDemoFolders, LegacyDownloadFile, LegacyGameHost, LegacyGameOutput,
    LegacyRateSettings, LegacyRosterTooLarge, LegacyServerSession, LegacySessionSettings,
    LegacySnapshotFrame, LegacySnapshotRefusal, LocalSnapshotBuffer, legacy_auto_demo_name,
    legacy_is_private_address, legacy_prune_auto_demos,
};
mod server_oob;
pub use server_oob::{
    LegacyOobAdmission, LegacyOobLimiter, LegacyOobLine, LegacyOobRates, LegacyOobRequest,
    LegacyTokens, parse_legacy_oob,
};
mod server_bans;
mod server_master;
mod server_whitelist;
pub use server_bans::{
    LEGACY_DEFAULT_MAX_BANS, LegacyBan, LegacyBanAddress, LegacyBanList, legacy_inet_addr,
    legacy_parse_cidr, legacy_string_to_address,
};
pub use server_master::{LEGACY_MASTER_SERVERS, LegacyMasterHeartbeat, LegacyMasterHost};
pub use server_whitelist::{
    LEGACY_DEFAULT_WHITELIST_CAPACITY, LegacyWhitelist, LegacyWhitelisting, WHITELIST_FILE,
};
mod server_console;
pub use server_console::{
    LEGACY_REDIRECT_BYTES, LegacyConsoleHost, LegacyConsoleStatus, LegacyRedirect,
    execute_legacy_console, run_legacy_rcon,
};
mod server_query;
pub use server_query::{
    LegacyInfoString, LegacyServerInfo, LegacyStatusPlayer, legacy_roster_population,
    write_legacy_challenge_response, write_legacy_info_response, write_legacy_status_response,
};
mod server_peers;
pub use server_peers::{
    LEGACY_CONNECT_RESPONSE, LEGACY_UNKNOWN_PEER_REPLY, LEGACY_WIRE_CLIENTS, LegacyConnectAttempt,
    LegacyConnectOutcome, LegacyConnectPolicy, LegacyConnectRefusal, LegacyPeer, LegacyPeerAddress,
    LegacyRosterHost, LegacyRoute, LegacyTimeoutPolicy, accept_legacy_packet,
    check_legacy_timeouts, connect_legacy_client, route_legacy_datagram,
};
mod server_message;
pub use server_message::{
    LegacyMessageContext, LegacyMessageEvent, LegacyMessageHost, LegacyMessageOutcome,
    execute_legacy_client_packet,
};
pub use server_move::{
    LegacyMoveAdmission, LegacyMoveEvent, LegacyMoveHost, LegacyMoveOutcome, LegacyMovePolicy,
    LegacyMoveState,
};
pub mod query;
mod receive_poll;

pub use status::{ServerStatus, StatusPlayer, parse_status_response, query_server_status};

pub const JKA_PROTOCOL: u32 = 26;
pub const DEFAULT_MASTER_PORT: u16 = 29_060;
pub const MAX_UDP_PACKET_BYTES: usize = 65_535;
/// Most usercmds one move packet may carry (`MAX_PACKET_USERCMDS`,
/// `codemp/qcommon/qcommon.h`; the server rejects larger counts).
pub const MAX_PACKET_USER_COMMANDS: usize = 32;
pub(crate) const OOB_PREFIX: [u8; 4] = [0xff; 4];
const FRAGMENT_BIT: u32 = 1 << 31;
const FRAGMENT_SIZE: usize = 1_300;

/// User-controlled fields in the legacy protocol-26 connect userinfo.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyUserInfo {
    /// Visible multiplayer player name.
    pub name: String,
    /// Legacy `model/skin` selection.
    pub model: String,
    /// Maximum requested network data rate in bytes per second.
    pub rate: u32,
    /// Requested server snapshot frequency.
    pub snaps: u16,
    /// Encoded rank, Force side, and power levels.
    pub forcepowers: String,
    /// Primary saber colour index.
    pub color1: u8,
    /// Secondary saber colour index.
    pub color2: u8,
    /// JoF EJK's worn hat and cape, written after the colour digits of
    /// `color1` and `color2` (`color1 "4santahat"`), where servers pass them
    /// through to `c1`/`c2` untouched and other clients' `atoi` stops before
    /// them. A name that is not 1 to 13 letters, digits, `_` or `-` with no
    /// leading digit is left out.
    pub cosmetics: [Option<String>; 2],
    /// Player starting-health percentage.
    pub handicap: u8,
    /// Player sex token used by legacy voice selection.
    pub sex: String,
    /// Whether the client predicts item pickups.
    pub predict_items: bool,
    /// Primary saber definition name.
    pub saber1: String,
    /// Secondary saber definition name or `none`.
    pub saber2: String,
    /// Per-player RGB tint.
    pub char_color: [u8; 3],
    /// Custom blade colours of the two sabers as the JA+/TaystJK
    /// `cp_sbRGB1`/`cp_sbRGB2` keys carry them (`r | g << 8 | b << 16`),
    /// sent only when set; the game adapter sets one for a saber whose
    /// colour index selects RGB.
    pub saber_rgb: [Option<u32>; 2],
    /// JA+/TaystJK client-plugin `cp_pluginDisable` bits (a set bit switches a
    /// plugin feature off for this client), sent only when set; the client's
    /// compatibility profile sets it for plugin servers and clears it elsewhere.
    pub plugin_disable: Option<u32>,
    /// Optional stock `CVAR_USERINFO` server password (`cl_main.cpp:2850`).
    pub password: Option<String>,
    /// Stock `ja_guid`: the client's identity for this server
    /// (`CL_UpdateGUID`, `cl_main.cpp:739-757`). Servers, mods and the
    /// protection layers in front of them use it to recognise a returning
    /// player; a client that sends none is logged as `NOGUID`.
    pub guid: Option<String>,
    /// The player's own values for game adapter extension keys (case ignored),
    /// each replacing the adapter's value where the adapter sends that key, and
    /// sent nowhere else; an invalid value leaves the adapter's.
    pub extension_overrides: Vec<(String, String)>,
}

impl LegacyUserInfo {
    /// Construct the historical defaults while replacing the player name.
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            model: "kyle/default".to_owned(),
            rate: 25_000,
            snaps: 40,
            forcepowers: "7-1-032330000000001333".to_owned(),
            color1: 4,
            color2: 4,
            cosmetics: [None, None],
            handicap: 100,
            sex: "male".to_owned(),
            predict_items: true,
            saber1: "single_1".to_owned(),
            saber2: "none".to_owned(),
            char_color: [255; 3],
            saber_rgb: [None; 2],
            plugin_disable: None,
            password: None,
            guid: None,
            extension_overrides: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChallengeResponse {
    pub server_challenge: i32,
    pub echoed_client_challenge: Option<i32>,
}

/// `MAX_RELIABLE_COMMANDS` (`codemp/qcommon/qcommon.h:127`): how many of the
/// server's reliable commands both sides keep for retransmission, and the
/// ring the packet keys index.
pub const MAX_RELIABLE_COMMANDS: usize = 128;

#[derive(Debug)]
pub struct LegacyConnection {
    socket: UdpSocket,
    server: SocketAddr,
    qport: u16,
    server_challenge: i32,
    outgoing_sequence: i32,
    incoming_sequence: i32,
    fragment_sequence: Option<i32>,
    fragment_buffer: Vec<u8>,
    server_commands: Vec<Vec<u8>>,
    client_reliable_commands: BTreeMap<i32, Vec<u8>>,
    traffic: ConnectionTraffic,
    last_rejected: Option<(SocketAddr, [u8; REJECTED_SAMPLE_BYTES], usize)>,
    last_received: Option<Instant>,
    server_prints: query::PrintInbox,
}

/// How much of a discarded packet to keep for diagnosis.
const REJECTED_SAMPLE_BYTES: usize = 48;
/// `CL_DisconnectPacket`'s anti-spoof window: ignore an out-of-band
/// `disconnect` while the sequenced stream is still alive.
const DISCONNECT_SPOOF_GUARD: Duration = Duration::from_secs(3);

/// Socket-level counters, for telling "the server went quiet" apart from
/// "packets are arriving and something is dropping them".
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConnectionTraffic {
    /// Packets handed to the socket.
    pub sent: u64,
    /// Sequenced packets accepted from the server.
    pub received: u64,
    /// Packets discarded because their sequence was not newer than the last
    /// accepted one: duplicates, reordering, or a server whose sequence went
    /// backwards while ours did not.
    pub stale: u64,
    /// Packets discarded because they came from another address or port.
    /// A server behind a proxy or DDoS filter can answer from one.
    pub wrong_source: u64,
    /// Packets discarded because they are out-of-band, not part of the
    /// sequenced stream.
    pub out_of_band: u64,
}

impl LegacyConnection {
    pub fn server(&self) -> SocketAddr {
        self.server
    }

    pub fn local_address(&self) -> Result<SocketAddr, NetworkError> {
        Ok(self.socket.local_addr()?)
    }

    pub fn qport(&self) -> u16 {
        self.qport
    }

    pub fn server_challenge(&self) -> i32 {
        self.server_challenge
    }

    /// Socket counters since the connection was made.
    pub fn traffic(&self) -> ConnectionTraffic {
        self.traffic
    }

    /// Where the most recently discarded packet came from, and its opening
    /// bytes. Names what a server behind a proxy or filter is actually
    /// sending back when the sequenced stream never starts.
    pub fn last_rejected_packet(&self) -> Option<(SocketAddr, &[u8])> {
        let (source, bytes, length) = self.last_rejected.as_ref()?;
        Some((*source, &bytes[..*length]))
    }

    /// `CL_DisconnectPacket` (`codemp/client/cl_main.cpp:1640-1660`): the
    /// server drops clients it no longer knows with an out-of-band
    /// `disconnect` so they need not wait out the timeout. It is honoured
    /// only from the server's own address, and only when nothing sequenced
    /// has arrived for three seconds, because an unsequenced packet is
    /// trivial to spoof.
    fn is_disconnect_packet(&self, packet: &[u8]) -> bool {
        let Some(payload) = packet.strip_prefix(&OOB_PREFIX) else {
            return false;
        };
        if !payload.starts_with(b"disconnect") {
            return false;
        }
        self.last_received
            .is_none_or(|last| last.elapsed() >= DISCONNECT_SPOOF_GUARD)
    }

    fn note_rejected_packet(&mut self, source: SocketAddr, packet: &[u8]) {
        let mut bytes = [0_u8; REJECTED_SAMPLE_BYTES];
        let length = packet.len().min(REJECTED_SAMPLE_BYTES);
        bytes[..length].copy_from_slice(&packet[..length]);
        self.last_rejected = Some((source, bytes, length));
    }

    /// Store one of the server's reliable commands in the retransmit ring
    /// (`CL_ParseCommandString` keeps every parsed string in
    /// `clc.serverCommands`, `codemp/client/cl_parse.cpp`).
    pub fn record_server_command(&mut self, sequence: i32, command: &[u8]) {
        let slot = &mut self.server_commands[sequence as usize & (MAX_RELIABLE_COMMANDS - 1)];
        slot.clear();
        slot.extend_from_slice(command);
    }

    /// The command string both sides key a packet with: the one stored at the
    /// acknowledgement this packet carries. The client encodes with
    /// `clc.serverCommands[reliableAcknowledge & (MAX_RELIABLE_COMMANDS-1)]`
    /// (`cl_net_chan.cpp:64`, and `cl_input.cpp:1572` for the usercmd key) and
    /// the server decodes with its own copy of the same slot
    /// (`sv_net_chan.cpp:114`), so the newest command is the right key only
    /// when it happens to be the acknowledged one.
    fn key_command(&self, server_command_sequence: i32) -> &[u8] {
        &self.server_commands[server_command_sequence as usize & (MAX_RELIABLE_COMMANDS - 1)]
    }

    /// Sends the empty first client message that causes a connected JKA server
    /// to transmit its current gamestate.
    /// Ask for the gamestate, acknowledging the newest server message received.
    ///
    /// A stock server resends a gamestate only when the client acknowledges a
    /// message newer than it (`SV_ExecuteClientMessage`: `messageAcknowledge >
    /// gamestateMessageNum`), as a stock client does in every packet. Always
    /// acknowledging 0 left a lost gamestate (a dropped fragment of a large one)
    /// unrecoverable while the server kept sending its other messages.
    pub fn request_initial_gamestate(&mut self) -> Result<(), NetworkError> {
        let mut writer = MessageWriter::new(16_384);
        writer.write_i32(0)?; // unknown server id until the gamestate arrives
        writer.write_i32(self.incoming_sequence)?;
        writer.write_i32(0)?; // no reliable server command acknowledged yet
        writer.write_u8(5)?; // clc_EOF
        let mut payload = writer.finish()?;
        xor_protocol_tail(&mut payload, 12, self.server_challenge, self.key_command(0));

        self.send_payload(&payload)
    }

    /// Acknowledges the initial gamestate with the first neutral user command,
    /// transitioning a legacy server client from `CS_PRIMED` to `CS_ACTIVE`.
    pub fn enter_world(
        &mut self,
        server_id: i32,
        gamestate_sequence: i32,
        server_command_sequence: i32,
        checksum_feed: i32,
        command_time: i32,
        request_delta_snapshot: bool,
    ) -> Result<(), NetworkError> {
        self.send_user_command(
            server_id,
            gamestate_sequence,
            server_command_sequence,
            checksum_feed,
            request_delta_snapshot,
            &UserCommand {
                server_time: command_time,
                ..UserCommand::default()
            },
        )
    }

    pub fn send_user_command(
        &mut self,
        server_id: i32,
        message_acknowledge: i32,
        server_command_sequence: i32,
        checksum_feed: i32,
        request_delta_snapshot: bool,
        command: &UserCommand,
    ) -> Result<(), NetworkError> {
        self.send_user_command_with_reliables(
            server_id,
            message_acknowledge,
            server_command_sequence,
            checksum_feed,
            request_delta_snapshot,
            &[],
            command,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn send_user_command_with_reliables(
        &mut self,
        server_id: i32,
        message_acknowledge: i32,
        server_command_sequence: i32,
        checksum_feed: i32,
        request_delta_snapshot: bool,
        reliable_commands: &[(i32, &[u8])],
        command: &UserCommand,
    ) -> Result<(), NetworkError> {
        self.send_user_commands_with_reliables(
            server_id,
            message_acknowledge,
            server_command_sequence,
            checksum_feed,
            request_delta_snapshot,
            reliable_commands,
            std::slice::from_ref(command),
        )
    }

    /// Send one move packet carrying `commands` oldest first, the way
    /// `CL_WritePacket` resends the previous packets' usercmds under
    /// `cl_packetdup` (`codemp/client/cl_input.cpp:1536-1580`). Each command
    /// is delta-coded against the one before it, starting from the null
    /// command; the server skips any whose time it already executed
    /// (`codemp/server/sv_client.cpp:1425-1440`).
    #[allow(clippy::too_many_arguments)]
    pub fn send_user_commands_with_reliables(
        &mut self,
        server_id: i32,
        message_acknowledge: i32,
        server_command_sequence: i32,
        checksum_feed: i32,
        request_delta_snapshot: bool,
        reliable_commands: &[(i32, &[u8])],
        commands: &[UserCommand],
    ) -> Result<(), NetworkError> {
        if commands.is_empty() || commands.len() > MAX_PACKET_USER_COMMANDS {
            return Err(NetworkError::InvalidCommand);
        }
        let mut writer = MessageWriter::new(16_384);
        writer.write_i32(server_id)?;
        writer.write_i32(message_acknowledge)?;
        writer.write_i32(server_command_sequence)?;
        self.write_reliable_commands(&mut writer, reliable_commands)?;
        writer.write_u8(if request_delta_snapshot { 2 } else { 3 })?; // clc_move / clc_moveNoDelta
        writer.write_u8(commands.len() as u8)?;
        let acknowledged = self.key_command(server_command_sequence);
        let command_key = checksum_feed ^ message_acknowledge ^ legacy_command_hash(acknowledged);
        let mut previous = &UserCommand::default();
        for command in commands {
            write_delta_user_command(&mut writer, command_key, previous, command)?;
            previous = command;
        }
        writer.write_u8(5)?; // clc_EOF
        let mut payload = writer.finish()?;
        xor_protocol_tail(
            &mut payload,
            12,
            self.server_challenge ^ server_id ^ message_acknowledge,
            self.key_command(server_command_sequence),
        );
        self.send_payload(&payload)
    }

    fn write_reliable_commands(
        &mut self,
        writer: &mut MessageWriter,
        reliable_commands: &[(i32, &[u8])],
    ) -> Result<(), NetworkError> {
        for &(sequence, command) in reliable_commands {
            if command.contains(&0) {
                return Err(NetworkError::InvalidCommand);
            }
            writer.write_u8(4)?; // clc_clientCommand
            writer.write_i32(sequence)?;
            writer.write_c_string(command)?;
            self.client_reliable_commands
                .insert(sequence, command.to_vec());
        }
        while self.client_reliable_commands.len() > 128 {
            let Some(oldest) = self.client_reliable_commands.keys().next().copied() else {
                break;
            };
            self.client_reliable_commands.remove(&oldest);
        }
        Ok(())
    }

    pub fn send_reliable_commands(
        &mut self,
        server_id: i32,
        message_acknowledge: i32,
        server_command_sequence: i32,
        commands: &[(i32, &[u8])],
    ) -> Result<(), NetworkError> {
        let mut writer = MessageWriter::new(16_384);
        writer.write_i32(server_id)?;
        writer.write_i32(message_acknowledge)?;
        writer.write_i32(server_command_sequence)?;
        self.write_reliable_commands(&mut writer, commands)?;
        writer.write_u8(5)?; // clc_EOF
        let mut payload = writer.finish()?;
        xor_protocol_tail(
            &mut payload,
            12,
            self.server_challenge ^ server_id ^ message_acknowledge,
            self.key_command(server_command_sequence),
        );
        self.send_payload(&payload)
    }

    pub fn send_reliable_command(
        &mut self,
        server_id: i32,
        message_acknowledge: i32,
        server_command_sequence: i32,
        client_command_sequence: i32,
        command: &[u8],
    ) -> Result<(), NetworkError> {
        self.send_reliable_commands(
            server_id,
            message_acknowledge,
            server_command_sequence,
            &[(client_command_sequence, command)],
        )
    }

    /// Receives and reassembles one sequenced server message.
    /// A zero timeout polls queued packets without sleeping or retrying an empty socket.
    /// Nonzero timeouts retain the blocking deadline contract for bootstrap/download callers.
    pub fn receive_server_message(
        &mut self,
        timeout: Duration,
    ) -> Result<ServerMessage, NetworkError> {
        let deadline = Instant::now() + timeout;
        if !timeout.is_zero() {
            self.socket
                .set_read_timeout(Some(timeout.min(Duration::from_millis(250))))?;
        }
        let mut packet = [0_u8; MAX_UDP_PACKET_BYTES];
        loop {
            let received = if timeout.is_zero() {
                receive_poll::poll(&self.socket, &mut packet)?
            } else {
                receive_until(&self.socket, &mut packet, deadline)?
            };
            let Some((length, source)) = received else {
                return Err(NetworkError::TimedOut("sequenced server message"));
            };
            let wrong_source = source != self.server;
            if wrong_source || length < 4 || packet[..length].starts_with(&OOB_PREFIX) {
                if wrong_source {
                    self.traffic.wrong_source += 1;
                } else {
                    self.traffic.out_of_band += 1;
                }
                self.note_rejected_packet(source, &packet[..length.min(MAX_UDP_PACKET_BYTES)]);
                if !wrong_source && self.is_disconnect_packet(&packet[..length]) {
                    return Err(NetworkError::ServerDisconnected);
                }
                self.server_prints
                    .receive(self.server, source, &packet[..length]);
                continue;
            }
            self.last_received = Some(Instant::now());

            let wire_sequence = u32::from_le_bytes(packet[..4].try_into().expect("four bytes"));
            let fragmented = wire_sequence & FRAGMENT_BIT != 0;
            let sequence = (wire_sequence & !FRAGMENT_BIT) as i32;
            if sequence <= self.incoming_sequence {
                self.traffic.stale += 1;
                continue;
            }
            self.traffic.received += 1;

            let mut payload = if fragmented {
                if length < 8 {
                    return Err(NetworkError::MalformedNetchanPacket);
                }
                let start = usize::from(u16::from_le_bytes(
                    packet[4..6].try_into().expect("two bytes"),
                ));
                let fragment_length = usize::from(u16::from_le_bytes(
                    packet[6..8].try_into().expect("two bytes"),
                ));
                let fragment = packet
                    .get(8..8 + fragment_length)
                    .ok_or(NetworkError::MalformedNetchanPacket)?;
                if self.fragment_sequence != Some(sequence) {
                    self.fragment_sequence = Some(sequence);
                    self.fragment_buffer.clear();
                }
                if start != self.fragment_buffer.len() {
                    // Netchan fragmentation is carried over UDP and individual
                    // pieces are not retransmitted. A duplicate can be ignored;
                    // a forward gap invalidates only this message, not the
                    // connection. The next sequence can still be assembled.
                    if start > self.fragment_buffer.len() {
                        self.fragment_buffer.clear();
                    }
                    continue;
                }
                self.fragment_buffer.extend_from_slice(fragment);
                if fragment_length == FRAGMENT_SIZE {
                    continue;
                }
                self.fragment_sequence = None;
                std::mem::take(&mut self.fragment_buffer)
            } else {
                packet[4..length].to_vec()
            };

            self.incoming_sequence = sequence;
            // The first logical long is Huffman bit-packed, not a raw
            // little-endian integer. Its encoded bytes are left outside the
            // XOR tail specifically so the command key can be selected first.
            let reliable_acknowledge = MessageReader::new(&payload).read_i32().unwrap_or(0);
            let acknowledged_command = self
                .client_reliable_commands
                .get(&reliable_acknowledge)
                .cloned()
                .unwrap_or_default();
            xor_protocol_tail(
                &mut payload,
                4,
                self.server_challenge ^ sequence,
                &acknowledged_command,
            );
            return Ok(ServerMessage { sequence, payload });
        }
    }

    fn send_payload(&mut self, payload: &[u8]) -> Result<(), NetworkError> {
        let mut packet = Vec::with_capacity(6 + payload.len());
        packet.extend_from_slice(&self.outgoing_sequence.to_le_bytes());
        packet.extend_from_slice(&self.qport.to_le_bytes());
        packet.extend_from_slice(payload);
        self.socket.send_to(&packet, self.server)?;
        self.traffic.sent += 1;
        self.outgoing_sequence += 1;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerMessage {
    pub sequence: i32,
    pub payload: Vec<u8>,
}

/// Builds an out-of-band command datagram.
///
/// Text an `rcon` line carries leaves in Windows-1252 when it fits
/// ([`sjk_protocol::encode_legacy_text`]), as a retail client sends it.
pub fn connectionless_packet(command: &str) -> Result<Vec<u8>, NetworkError> {
    if command.as_bytes().contains(&0) || command.contains('\n') || command.contains('\r') {
        return Err(NetworkError::InvalidCommand);
    }
    let command = sjk_protocol::encode_legacy_text(command);
    let mut packet = Vec::with_capacity(4 + command.len());
    packet.extend_from_slice(&OOB_PREFIX);
    packet.extend_from_slice(&command);
    Ok(packet)
}

/// Builds JKA's unusual compressed out-of-band `connect` datagram.
///
/// Bytes through `connect ` remain plain text. OpenJK's `Huff_Compress` then
/// compresses the opening quote, userinfo, and closing quote as one block.
/// The userinfo (the player's name) leaves in Windows-1252 when it fits
/// ([`sjk_protocol::encode_legacy_text`]).
pub fn connect_packet(userinfo: &str) -> Result<Vec<u8>, NetworkError> {
    if userinfo.contains(['\0', '\n', '\r', '"']) {
        return Err(NetworkError::InvalidUserInfo);
    }
    let quoted = format!("\"{userinfo}\"");
    let compressed = compress_connect_block(&sjk_protocol::encode_legacy_text(&quoted))?;
    let mut packet = Vec::with_capacity(12 + compressed.len());
    packet.extend_from_slice(&OOB_PREFIX);
    packet.extend_from_slice(b"connect ");
    packet.extend_from_slice(&compressed);
    Ok(packet)
}

pub fn basic_userinfo(
    server_challenge: i32,
    qport: u16,
    name: &str,
) -> Result<String, NetworkError> {
    legacy_userinfo(server_challenge, qport, &LegacyUserInfo::with_name(name))
}

/// Assemble the legacy userinfo in the byte ordering used by the captured
/// protocol-26 connect path.
pub fn legacy_userinfo(
    server_challenge: i32,
    qport: u16,
    user: &LegacyUserInfo,
) -> Result<String, NetworkError> {
    legacy_userinfo_with_extensions(server_challenge, qport, user, &[])
}

/// Assemble legacy userinfo plus caller-owned compatibility-profile fields.
///
/// The transport deliberately assigns no meaning to extension names. Game
/// adapters own those names and values; this crate only validates and appends
/// them before the mandatory protocol connection keys.
pub fn legacy_userinfo_with_extensions(
    server_challenge: i32,
    qport: u16,
    user: &LegacyUserInfo,
    extensions: &[(&str, &str)],
) -> Result<String, NetworkError> {
    let mut result = legacy_userinfo_payload_with_extensions(user, extensions)?;
    result.push_str(&format!(
        "\\protocol\\{JKA_PROTOCOL}\\qport\\{qport}\\challenge\\{server_challenge}"
    ));
    Ok(result)
}

/// Assemble the `CVAR_USERINFO` payload used by an in-session `userinfo` command.
///
/// Unlike [`legacy_userinfo_with_extensions`], this deliberately excludes the
/// connection-only `protocol`, `qport`, and `challenge` keys. OpenJK starts a
/// connect packet from `Cvar_InfoString(CVAR_USERINFO)` and adds those three
/// keys afterwards (`codemp/client/cl_main.cpp:1611-1614`).
pub fn legacy_userinfo_payload_with_extensions(
    user: &LegacyUserInfo,
    extensions: &[(&str, &str)],
) -> Result<String, NetworkError> {
    if !valid_userinfo_value(&user.name)
        || !valid_userinfo_value(&user.model)
        || !valid_userinfo_value(&user.forcepowers)
        || !valid_userinfo_value(&user.sex)
        || !valid_userinfo_value(&user.saber1)
        || !valid_userinfo_value(&user.saber2)
        || user.rate == 0
        || user.snaps == 0
        || user
            .password
            .as_deref()
            .is_some_and(|value| !valid_userinfo_value(value))
        || user
            .guid
            .as_deref()
            .is_some_and(|value| !valid_userinfo_value(value))
        || extensions.iter().any(|(key, value)| {
            !valid_userinfo_value(key) || !valid_userinfo_value(value) || stock_userinfo_key(key)
        })
    {
        return Err(NetworkError::InvalidUserInfo);
    }
    let name = &user.name;
    let model = &user.model;
    let rate = user.rate;
    let snaps = user.snaps;
    let forcepowers = &user.forcepowers;
    let cosmetic = |slot: usize| {
        user.cosmetics[slot]
            .as_deref()
            .filter(|name| valid_cosmetic_name(name))
            .unwrap_or("")
    };
    let color1 = format!("{}{}", user.color1, cosmetic(0));
    let color2 = format!("{}{}", user.color2, cosmetic(1));
    let handicap = user.handicap;
    let sex = &user.sex;
    let predict_items = u8::from(user.predict_items);
    let saber1 = &user.saber1;
    let saber2 = &user.saber2;
    let [red, green, blue] = user.char_color;
    let mut result = format!(
        "\\name\\{name}\\rate\\{rate}\\snaps\\{snaps}\\model\\{model}\\forcepowers\\{forcepowers}\\color1\\{color1}\\color2\\{color2}\\handicap\\{handicap}\\sex\\{sex}\\cg_predictItems\\{predict_items}\\saber1\\{saber1}\\saber2\\{saber2}\\char_color_red\\{red}\\char_color_green\\{green}\\char_color_blue\\{blue}"
    );
    for (key, packed) in ["cp_sbRGB1", "cp_sbRGB2"].iter().zip(user.saber_rgb) {
        if let Some(packed) = packed {
            result.push_str(&format!("\\{key}\\{packed}"));
        }
    }
    if let Some(bits) = user.plugin_disable {
        result.push_str(&format!("\\cp_pluginDisable\\{bits}"));
    }
    for (key, value) in extensions {
        let value = user
            .extension_overrides
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
            .filter(|value| valid_userinfo_value(value))
            .unwrap_or(value);
        result.push('\\');
        result.push_str(key);
        result.push('\\');
        result.push_str(value);
    }
    if let Some(guid) = &user.guid {
        result.push_str("\\ja_guid\\");
        result.push_str(guid);
    }
    if let Some(password) = &user.password {
        result.push_str("\\password\\");
        result.push_str(password);
    }
    Ok(result)
}

/// JoF EJK's cosmetic name rule (`MAX_COSMETIC_LENGTH` 14 with the
/// terminator, no leading digit for the receiving `atoi` to swallow).
fn valid_cosmetic_name(name: &str) -> bool {
    (1..=13).contains(&name.len())
        && !name.as_bytes()[0].is_ascii_digit()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn valid_userinfo_value(value: &str) -> bool {
    !value.is_empty() && !value.contains(['\\', ';', '"', '\0', '\n', '\r'])
}

fn stock_userinfo_key(key: &str) -> bool {
    [
        "name",
        "rate",
        "snaps",
        "model",
        "forcepowers",
        "color1",
        "color2",
        "handicap",
        "sex",
        "cg_predictItems",
        "saber1",
        "saber2",
        "char_color_red",
        "char_color_green",
        "char_color_blue",
        "cp_sbRGB1",
        "cp_sbRGB2",
        "cp_pluginDisable",
        "protocol",
        "qport",
        "challenge",
        "ja_guid",
        "password",
    ]
    .iter()
    .any(|stock| key.eq_ignore_ascii_case(stock))
}

pub fn parse_master_response(packet: &[u8]) -> Result<Vec<SocketAddr>, NetworkError> {
    let payload = packet
        .strip_prefix(&OOB_PREFIX)
        .ok_or(NetworkError::MissingConnectionlessPrefix)?;
    let marker = b"getserversResponse";
    if !payload.starts_with(marker) {
        return Err(NetworkError::UnexpectedResponse);
    }
    let mut cursor = marker.len();
    while payload.get(cursor).is_some_and(u8::is_ascii_whitespace) {
        cursor += 1;
    }
    let mut servers = Vec::new();
    while cursor < payload.len() {
        if payload.get(cursor..cursor + 4) == Some(b"\\EOT") {
            break;
        }
        if payload.get(cursor) != Some(&b'\\') {
            return Err(NetworkError::MalformedMasterResponse { offset: cursor });
        }
        let entry = payload
            .get(cursor + 1..cursor + 7)
            .ok_or(NetworkError::MalformedMasterResponse { offset: cursor })?;
        servers.push(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(entry[0], entry[1], entry[2], entry[3])),
            u16::from_be_bytes([entry[4], entry[5]]),
        ));
        cursor += 7;
    }
    Ok(servers)
}

/// How long after the last master packet the list is taken as complete.
const MASTER_LULL: Duration = Duration::from_millis(300);

pub fn query_master(
    master: impl ToSocketAddrs,
    timeout: Duration,
) -> Result<Vec<SocketAddr>, NetworkError> {
    let master = master
        .to_socket_addrs()?
        .next()
        .ok_or(NetworkError::NoResolvedAddress)?;
    let bind_address = match master {
        SocketAddr::V4(_) => "0.0.0.0:0",
        SocketAddr::V6(_) => "[::]:0",
    };
    let socket = UdpSocket::bind(bind_address)?;
    socket.set_read_timeout(Some(timeout.min(Duration::from_millis(250))))?;
    socket.send_to(
        &connectionless_packet(&format!("getservers {JKA_PROTOCOL}"))?,
        master,
    )?;

    let mut deadline = Instant::now() + timeout;
    let mut packet = [0_u8; MAX_UDP_PACKET_BYTES];
    let mut servers = BTreeSet::new();
    loop {
        match socket.recv_from(&mut packet) {
            Ok((length, source)) if source.ip() == master.ip() => {
                servers.extend(parse_master_response(&packet[..length])?);
                // The list comes in one burst of packets (the end marker is
                // no help: some masters put it in every packet), so a lull
                // after it means the list is complete.
                deadline = deadline.min(Instant::now() + MASTER_LULL);
            }
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                if Instant::now() >= deadline {
                    break;
                }
            }
            Err(error) => return Err(error.into()),
        }
        if Instant::now() >= deadline {
            break;
        }
    }
    Ok(servers.into_iter().collect())
}

/// Ask every server for its info at once and hand each answer to `found` (with the
/// round-trip time) as it arrives. Returns once all have answered or the asking is over;
/// servers that never answer are simply absent.
///
/// Answers already waiting are read after every [`INFO_SEND_BATCH`] requests, so a long
/// list's early answers do not pile up in the socket's buffer (and overflow it) while the
/// rest are still being asked. A server that has not answered `timeout` after the last
/// request is asked again, [`INFO_RESENDS`] times, waiting two thirds of `timeout` each
/// time: a lost packet, or a busy server whose `getinfo` rate limit (`SVC_Info`) dropped
/// the request, would otherwise leave it out of this refresh, and a list that changed
/// from one refresh to the next. The round trip is timed from the latest request.
pub fn query_server_infos(
    servers: &[SocketAddr],
    timeout: Duration,
    mut found: impl FnMut(SocketAddr, InfoString, Duration),
) -> Result<(), NetworkError> {
    let mut sockets: [Option<UdpSocket>; 2] = [None, None];
    let mut asked = BTreeMap::new();
    let mut packet = [0_u8; MAX_UDP_PACKET_BYTES];
    let request = connectionless_packet("getinfo sjk")?;
    let mut wait = timeout;
    for round in 0..=INFO_RESENDS {
        let targets: Vec<SocketAddr> = if round == 0 {
            servers.to_vec()
        } else {
            asked.keys().copied().collect()
        };
        if targets.is_empty() {
            break;
        }
        for (sent, server) in targets.into_iter().enumerate() {
            let socket = info_socket(&mut sockets, server)?;
            socket.send_to(&request, server)?;
            asked.insert(server, Instant::now());
            if (sent + 1).is_multiple_of(INFO_SEND_BATCH) {
                for socket in sockets.iter().flatten() {
                    socket.set_nonblocking(true)?;
                    let read = read_info_answers(socket, &mut asked, &mut packet, &mut found);
                    socket.set_nonblocking(false)?;
                    read?;
                }
            }
        }
        let deadline = Instant::now() + wait;
        while !asked.is_empty() && Instant::now() < deadline {
            for socket in sockets.iter().flatten() {
                match socket.recv_from(&mut packet) {
                    Ok((length, source)) => {
                        let Some(sent) = asked.remove(&source) else {
                            continue;
                        };
                        if let Ok(info) = parse_info_response(&packet[..length]) {
                            found(source, info, sent.elapsed());
                        }
                    }
                    Err(error) if info_read_goes_on(&error) => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        wait = timeout * 2 / 3;
    }
    Ok(())
}

/// The socket that asks `server`, one for IPv4 and one for IPv6, bound on first use.
fn info_socket(
    sockets: &mut [Option<UdpSocket>; 2],
    server: SocketAddr,
) -> Result<&UdpSocket, NetworkError> {
    let (slot, bind_address) = match server {
        SocketAddr::V4(_) => (0, "0.0.0.0:0"),
        SocketAddr::V6(_) => (1, "[::]:0"),
    };
    match &mut sockets[slot] {
        Some(socket) => Ok(socket),
        slot => {
            let socket = UdpSocket::bind(bind_address)?;
            socket.set_read_timeout(Some(Duration::from_millis(50)))?;
            Ok(slot.insert(socket))
        }
    }
}

/// Whether a failed read of info answers leaves the others to come: nothing in yet
/// (`WouldBlock`, `TimedOut`), or a server that is gone (`ConnectionReset`). Windows
/// hands an unconnected UDP socket the "port unreachable" a dead server's address sent
/// back for an earlier request as a reset on its next read, as the engine's
/// `NET_GetPacket` knows (`WSAECONNRESET`, `net_ip.cpp:265-273`); taken as an error it ended the whole query,
/// and every server that had not answered yet was missing from the list.
fn info_read_goes_on(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::ConnectionReset
    )
}

/// Info requests [`query_server_infos`] sends before it reads the answers already in.
pub const INFO_SEND_BATCH: usize = 32;
/// How often [`query_server_infos`] asks a server again that has not answered.
pub const INFO_RESENDS: usize = 2;

/// Read the info answers waiting on a non-blocking `socket`, handing the ones from
/// servers in `asked` to `found` and taking them off it, until none is left.
fn read_info_answers(
    socket: &UdpSocket,
    asked: &mut BTreeMap<SocketAddr, Instant>,
    packet: &mut [u8; MAX_UDP_PACKET_BYTES],
    found: &mut impl FnMut(SocketAddr, InfoString, Duration),
) -> Result<(), NetworkError> {
    loop {
        match socket.recv_from(packet) {
            Ok((length, source)) => {
                let Some(sent) = asked.remove(&source) else {
                    continue;
                };
                if let Ok(info) = parse_info_response(&packet[..length]) {
                    found(source, info, sent.elapsed());
                }
            }
            Err(error) if error.kind() == io::ErrorKind::ConnectionReset => {}
            Err(error) if info_read_goes_on(&error) => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
}

pub fn query_server_info(
    server: SocketAddr,
    timeout: Duration,
) -> Result<InfoString, NetworkError> {
    let packet = query::first_response(server, &connectionless_packet("getinfo sjk")?, timeout)?;
    parse_info_response(&packet)
}

pub fn resolve_server(server: impl ToSocketAddrs) -> Result<SocketAddr, NetworkError> {
    server
        .to_socket_addrs()?
        .next()
        .ok_or(NetworkError::NoResolvedAddress)
}

/// Performs the stateless first half of JKA's connection handshake.
///
/// This does not consume a player slot: only the later compressed `connect`
/// request asks the server to create a client.
pub fn query_challenge(
    server: SocketAddr,
    client_challenge: i32,
    timeout: Duration,
) -> Result<ChallengeResponse, NetworkError> {
    let bind_address = match server {
        SocketAddr::V4(_) => "0.0.0.0:0",
        SocketAddr::V6(_) => "[::]:0",
    };
    let socket = UdpSocket::bind(bind_address)?;
    socket.set_read_timeout(Some(timeout))?;
    socket.send_to(
        &connectionless_packet(&format!("getchallenge {client_challenge}"))?,
        server,
    )?;

    let mut packet = [0_u8; MAX_UDP_PACKET_BYTES];
    let (length, source) = socket.recv_from(&mut packet)?;
    if source != server {
        return Err(NetworkError::UnexpectedSource {
            expected: server,
            source,
        });
    }
    let response = parse_challenge_response(&packet[..length])?;
    if response
        .echoed_client_challenge
        .is_some_and(|echo| echo != client_challenge)
    {
        return Err(NetworkError::ChallengeEchoMismatch {
            expected: client_challenge,
            received: response.echoed_client_challenge,
        });
    }
    Ok(response)
}

/// Completes the connectionless portion of the JKA client handshake and keeps
/// the UDP socket alive for the subsequent netchan exchange.
pub fn connect_legacy(
    server: SocketAddr,
    name: &str,
    timeout: Duration,
) -> Result<LegacyConnection, NetworkError> {
    connect_legacy_with_userinfo(server, &LegacyUserInfo::with_name(name), timeout)
}

/// Complete a legacy handshake using explicitly supplied userinfo cvars.
pub fn connect_legacy_with_userinfo(
    server: SocketAddr,
    userinfo: &LegacyUserInfo,
    timeout: Duration,
) -> Result<LegacyConnection, NetworkError> {
    connect_legacy_with_userinfo_extensions(server, userinfo, &[], timeout)
}

/// Complete a legacy handshake with adapter-owned userinfo extension fields.
pub fn connect_legacy_with_userinfo_extensions(
    server: SocketAddr,
    userinfo: &LegacyUserInfo,
    extensions: &[(&str, &str)],
    timeout: Duration,
) -> Result<LegacyConnection, NetworkError> {
    connect_legacy_with_userinfo_extensions_observed(server, userinfo, extensions, timeout, |_| {})
}

/// Stock resends the pending handshake packet until it is answered
/// (`CL_CheckForResend`), so a single lost or rate-limited datagram does not
/// fail the join: busy servers drop out-of-band packets past a global budget.
/// `getchallenge` is stateless and cheap to repeat.
const CHALLENGE_RESEND: Duration = Duration::from_secs(1);
/// A server ignores a repeated `connect` from the same address and port within
/// `sv_reconnectlimit` (3 s by default), so `connect` repeats at stock's
/// `RETRANSMIT_TIMEOUT` of 3 s.
const CONNECT_RESEND: Duration = Duration::from_secs(3);

/// Observable milestones and diagnostic replies in the connectionless handshake.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectPhase<'a> {
    /// A validated `challengeResponse` was received.
    Challenge,
    /// A validated `connectResponse` was received.
    Connected,
    /// An unrecognized OOB command from the expected server; bytes are borrowed.
    UnknownReply(&'a [u8]),
}

/// Connect while reporting handshake milestones and unexpected OOB command words.
pub fn connect_legacy_with_userinfo_extensions_observed(
    server: SocketAddr,
    userinfo: &LegacyUserInfo,
    extensions: &[(&str, &str)],
    timeout: Duration,
    mut observe: impl FnMut(ConnectPhase<'_>),
) -> Result<LegacyConnection, NetworkError> {
    let bind_address = match server {
        SocketAddr::V4(_) => "0.0.0.0:0",
        SocketAddr::V6(_) => "[::]:0",
    };
    let socket = UdpSocket::bind(bind_address)?;
    let receive_slice = timeout.min(Duration::from_millis(250));
    socket.set_read_timeout(Some(receive_slice))?;
    let client_challenge = process_challenge();
    let challenge_request = connectionless_packet(&format!("getchallenge {client_challenge}"))?;
    socket.send_to(&challenge_request, server)?;

    let deadline = Instant::now() + timeout;
    let mut resend = Instant::now() + CHALLENGE_RESEND;
    let mut packet = [0_u8; MAX_UDP_PACKET_BYTES];
    let challenge = loop {
        let Some((length, source)) = receive_until(&socket, &mut packet, resend.min(deadline))?
        else {
            if Instant::now() >= deadline {
                return Err(NetworkError::TimedOut("challenge response"));
            }
            socket.send_to(&challenge_request, server)?;
            resend = Instant::now() + CHALLENGE_RESEND;
            continue;
        };
        if source != server {
            continue;
        }
        if let Some(reason) = parse_print_response(&packet[..length]) {
            return Err(NetworkError::ConnectionRejected(reason));
        }
        let response = parse_challenge_response(&packet[..length]).inspect_err(|_| {
            if let Some(command) = oob_command(&packet[..length]) {
                observe(ConnectPhase::UnknownReply(command));
            }
        })?;
        if response
            .echoed_client_challenge
            .is_some_and(|echo| echo != client_challenge)
        {
            return Err(NetworkError::ChallengeEchoMismatch {
                expected: client_challenge,
                received: response.echoed_client_challenge,
            });
        }
        observe(ConnectPhase::Challenge);
        break response.server_challenge;
    };

    let qport = socket.local_addr()?.port();
    let userinfo = legacy_userinfo_with_extensions(challenge, qport, userinfo, extensions)?;
    let connect_request = connect_packet(&userinfo)?;
    socket.send_to(&connect_request, server)?;

    let deadline = Instant::now() + timeout;
    let mut resend = Instant::now() + CONNECT_RESEND;
    loop {
        let Some((length, source)) = receive_until(&socket, &mut packet, resend.min(deadline))?
        else {
            if Instant::now() >= deadline {
                return Err(NetworkError::TimedOut("connect response"));
            }
            socket.send_to(&connect_request, server)?;
            resend = Instant::now() + CONNECT_RESEND;
            continue;
        };
        if source != server {
            continue;
        }
        if let Some(reason) = parse_print_response(&packet[..length]) {
            return Err(NetworkError::ConnectionRejected(reason));
        }
        let payload = packet[..length]
            .strip_prefix(&OOB_PREFIX)
            .ok_or(NetworkError::UnexpectedResponse)?;
        let command = payload
            .split(|byte| byte.is_ascii_whitespace() || *byte == 0)
            .next()
            .unwrap_or_default();
        if command == b"connectResponse" {
            observe(ConnectPhase::Connected);
            return Ok(LegacyConnection {
                socket,
                server,
                qport,
                server_challenge: challenge,
                outgoing_sequence: 1,
                incoming_sequence: 0,
                fragment_sequence: None,
                fragment_buffer: Vec::new(),
                server_commands: vec![Vec::new(); MAX_RELIABLE_COMMANDS],
                traffic: ConnectionTraffic::default(),
                last_rejected: None,
                last_received: None,
                server_prints: query::PrintInbox::default(),
                client_reliable_commands: BTreeMap::new(),
            });
        }
        // A resent getchallenge can be answered after the first answer was used.
        if command != b"challengeResponse" {
            observe(ConnectPhase::UnknownReply(command));
        }
    }
}

fn oob_command(packet: &[u8]) -> Option<&[u8]> {
    Some(
        packet
            .strip_prefix(&OOB_PREFIX)?
            .split(|byte| byte.is_ascii_whitespace() || *byte == 0)
            .next()
            .unwrap_or_default(),
    )
}

fn xor_protocol_tail(payload: &mut [u8], start: usize, seed: i32, command: &[u8]) {
    let mut key = seed as u8;
    let mut command_index = 0;
    for (index, byte) in payload.iter_mut().enumerate().skip(start) {
        let command_byte = command.get(command_index).copied().unwrap_or(0);
        if !command.is_empty() {
            command_index += 1;
            if command_index == command.len() {
                command_index = 0;
            }
        }
        let command_byte = if command_byte == b'%' {
            b'.'
        } else {
            command_byte
        };
        key ^= command_byte.wrapping_shl((index & 1) as u32);
        *byte ^= key;
    }
}

fn receive_until(
    socket: &UdpSocket,
    packet: &mut [u8],
    deadline: Instant,
) -> Result<Option<(usize, SocketAddr)>, NetworkError> {
    loop {
        match socket.recv_from(packet) {
            Ok(received) => return Ok(Some(received)),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                if Instant::now() >= deadline {
                    return Ok(None);
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn parse_print_response(packet: &[u8]) -> Option<String> {
    let payload = packet.strip_prefix(&OOB_PREFIX)?;
    let message = payload.strip_prefix(b"print\n")?;
    Some(
        sjk_protocol::decode_legacy_text(message)
            .trim_matches(['\0', '\n', '\r'])
            .to_owned(),
    )
}

fn process_challenge() -> i32 {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let mixed = elapsed.as_nanos() ^ u128::from(std::process::id());
    (mixed as u32 ^ (mixed >> 32) as u32) as i32
}

pub fn parse_challenge_response(packet: &[u8]) -> Result<ChallengeResponse, NetworkError> {
    let payload = packet
        .strip_prefix(&OOB_PREFIX)
        .ok_or(NetworkError::MissingConnectionlessPrefix)?;
    let line = payload
        .split(|byte| *byte == b'\n' || *byte == b'\r' || *byte == 0)
        .next()
        .unwrap_or_default();
    let line = std::str::from_utf8(line).map_err(|_| NetworkError::NonUtf8Response)?;
    let mut fields = line.split_ascii_whitespace();
    if fields.next() != Some("challengeResponse") {
        return Err(NetworkError::UnexpectedResponse);
    }
    let server_challenge = fields
        .next()
        .ok_or(NetworkError::MalformedChallengeResponse)?
        .parse()
        .map_err(|_| NetworkError::MalformedChallengeResponse)?;
    let echoed_client_challenge = fields
        .next()
        .map(str::parse)
        .transpose()
        .map_err(|_| NetworkError::MalformedChallengeResponse)?;
    Ok(ChallengeResponse {
        server_challenge,
        echoed_client_challenge,
    })
}

fn parse_info_response(packet: &[u8]) -> Result<InfoString, NetworkError> {
    let payload = packet
        .strip_prefix(&OOB_PREFIX)
        .ok_or(NetworkError::MissingConnectionlessPrefix)?;
    let info = payload
        .strip_prefix(b"infoResponse\n")
        .or_else(|| payload.strip_prefix(b"infoResponse\r\n"))
        .ok_or(NetworkError::UnexpectedResponse)?;
    let info = info
        .split(|byte| *byte == b'\n' || *byte == 0)
        .next()
        .unwrap_or_default();
    // A host name in Windows-1252 (`»`, `§`, accented letters) is no UTF-8: read it as
    // the game does, byte by byte, rather than dropping the server from the list.
    let info = sjk_protocol::decode_legacy_text(info);
    Ok(InfoString::parse(&info)?)
}

#[derive(Debug)]
pub enum NetworkError {
    Io(io::Error),
    Info(InfoStringError),
    InvalidCommand,
    InvalidUserInfo,
    AdaptiveHuffman(AdaptiveHuffmanError),
    Message(MessageError),
    MissingConnectionlessPrefix,
    UnexpectedResponse,
    MalformedMasterResponse {
        offset: usize,
    },
    NoResolvedAddress,
    NonUtf8Response,
    MalformedChallengeResponse,
    ChallengeEchoMismatch {
        expected: i32,
        received: Option<i32>,
    },
    TimedOut(&'static str),
    ConnectionRejected(String),
    /// The server sent an out-of-band `disconnect`: it no longer recognises
    /// this client (`SV_PacketEvent`, `codemp/server/sv_main.cpp:838-840`).
    ServerDisconnected,
    MalformedNetchanPacket,
    UnexpectedFragmentOffset {
        expected: usize,
        received: usize,
    },
    UnexpectedSource {
        expected: SocketAddr,
        source: SocketAddr,
    },
}

impl fmt::Display for NetworkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::Info(error) => error.fmt(formatter),
            Self::InvalidCommand => write!(formatter, "invalid connectionless command"),
            Self::InvalidUserInfo => formatter.write_str("invalid connect userinfo"),
            Self::AdaptiveHuffman(error) => error.fmt(formatter),
            Self::Message(error) => error.fmt(formatter),
            Self::MissingConnectionlessPrefix => write!(formatter, "packet lacks the OOB prefix"),
            Self::UnexpectedResponse => write!(formatter, "unexpected connectionless response"),
            Self::MalformedMasterResponse { offset } => {
                write!(formatter, "malformed master response at byte {offset}")
            }
            Self::NoResolvedAddress => write!(formatter, "host resolved to no addresses"),
            Self::NonUtf8Response => write!(formatter, "connectionless response is not UTF-8"),
            Self::MalformedChallengeResponse => formatter.write_str("malformed challenge response"),
            Self::ChallengeEchoMismatch { expected, received } => write!(
                formatter,
                "challenge response echoed {received:?}, expected {expected}"
            ),
            Self::TimedOut(stage) => write!(formatter, "timed out waiting for {stage}"),
            Self::ConnectionRejected(reason) => write!(formatter, "connection rejected: {reason}"),
            Self::ServerDisconnected => write!(formatter, "Server disconnected"),
            Self::MalformedNetchanPacket => formatter.write_str("malformed netchan packet"),
            Self::UnexpectedFragmentOffset { expected, received } => write!(
                formatter,
                "received netchan fragment at {received}, expected {expected}"
            ),
            Self::UnexpectedSource { expected, source } => {
                write!(
                    formatter,
                    "expected response from {expected}, received {source}"
                )
            }
        }
    }
}

impl Error for NetworkError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Info(error) => Some(error),
            Self::AdaptiveHuffman(error) => Some(error),
            Self::Message(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for NetworkError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<InfoStringError> for NetworkError {
    fn from(value: InfoStringError) -> Self {
        Self::Info(value)
    }
}

impl From<AdaptiveHuffmanError> for NetworkError {
    fn from(value: AdaptiveHuffmanError) -> Self {
        Self::AdaptiveHuffman(value)
    }
}

impl From<MessageError> for NetworkError {
    fn from(value: MessageError) -> Self {
        Self::Message(value)
    }
}

#[cfg(test)]
mod info_charset_tests {
    use super::parse_info_response;

    /// A host name in Windows-1252 (`»`, `«`, `§`) no longer drops the server from the
    /// list: its info used to be refused as not UTF-8 (Sol's report of missing servers,
    /// 11/10/2026).
    #[test]
    fn a_windows_1252_host_name_keeps_the_server() {
        let packet =
            b"\xff\xff\xff\xffinfoResponse\n\\hostname\\\xbbJoF\xab \xa7Duels\\clients\\3\n";
        let info = parse_info_response(packet).expect("the server's info");
        assert_eq!(info.get("hostname"), Some("\u{bb}JoF\u{ab} \u{a7}Duels"));
        assert_eq!(info.get_i32("clients"), Some(3));
        let plain = b"\xff\xff\xff\xffinfoResponse\n\\hostname\\Temple\n";
        assert_eq!(
            parse_info_response(plain).unwrap().get("hostname"),
            Some("Temple")
        );
    }
}

#[cfg(test)]
mod handshake_resend_tests {
    use super::*;
    use std::thread;

    /// A loopback server that ignores the first `skip_challenges` getchallenge
    /// and `skip_connects` connect packets, as a lossy or rate-limited path would.
    fn lossy_server(
        skip_challenges: usize,
        skip_connects: usize,
    ) -> (SocketAddr, thread::JoinHandle<(usize, usize)>) {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let address = socket.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut challenges, mut connects) = (0, 0);
            let mut packet = [0_u8; MAX_UDP_PACKET_BYTES];
            while let Ok((length, client)) = socket.recv_from(&mut packet) {
                let payload = &packet[..length];
                if let Some(rest) =
                    payload.strip_prefix(b"\xff\xff\xff\xffgetchallenge ".as_slice())
                {
                    challenges += 1;
                    if challenges > skip_challenges {
                        let echo = String::from_utf8_lossy(rest)
                            .trim_end_matches('\0')
                            .to_owned();
                        let mut reply = OOB_PREFIX.to_vec();
                        reply
                            .extend_from_slice(format!("challengeResponse 1234 {echo}").as_bytes());
                        socket.send_to(&reply, client).unwrap();
                    }
                } else {
                    connects += 1;
                    if connects > skip_connects {
                        socket
                            .send_to(b"\xff\xff\xff\xffconnectResponse", client)
                            .unwrap();
                        return (challenges, connects);
                    }
                }
            }
            (challenges, connects)
        });
        (address, handle)
    }

    #[test]
    fn a_lost_getchallenge_is_resent() {
        let (server, handle) = lossy_server(1, 0);
        connect_legacy(server, "Padawan", Duration::from_secs(5)).unwrap();
        assert_eq!(handle.join().unwrap(), (2, 1));
    }

    #[test]
    fn a_lost_connect_is_resent() {
        let (server, handle) = lossy_server(0, 1);
        connect_legacy(server, "Padawan", Duration::from_secs(5)).unwrap();
        let (challenges, connects) = handle.join().unwrap();
        assert_eq!(connects, 2);
        // Getchallenge may have been repeated once while connect waited.
        assert!(challenges >= 1);
    }
}

#[cfg(test)]
mod cosmetic_userinfo_tests {
    use super::*;

    /// JoF EJK sends its `color1`/`color2` cvars verbatim, so a worn hat is
    /// the text after the colour digits (`UI_SetCosmetic`, `"%d%s"`).
    #[test]
    fn worn_cosmetics_follow_the_colour_digits() {
        let mut user = LegacyUserInfo::with_name("Sol");
        user.cosmetics = [Some("santahat".to_owned()), None];
        let info = legacy_userinfo(1, 2, &user).unwrap();
        assert!(info.contains(r"\color1\4santahat\color2\4\"), "{info}");
        user.cosmetics = [Some("bad\name".to_owned()), Some("2cape".to_owned())];
        let info = legacy_userinfo(1, 2, &user).unwrap();
        assert!(info.contains(r"\color1\4\color2\4\"), "{info}");
    }
}

#[cfg(test)]
mod legacy_text_packet_tests {
    use super::*;

    /// A name a retail or EternalJK player can read: `ø` is the single
    /// Windows-1252 byte 0xF8 inside the compressed block, not UTF-8's C3 B8.
    #[test]
    fn connect_userinfo_name_leaves_in_windows_1252() {
        let userinfo = legacy_userinfo(1234, 5678, &LegacyUserInfo::with_name("j^6ø^7f")).unwrap();
        let decoded = decode_connect_packet(&connect_packet(&userinfo).unwrap()).unwrap();
        let name = b"\\name\\j^6\xf8^7f\\";
        assert!(decoded.windows(name.len()).any(|window| window == name));
        assert!(!decoded.contains(&0xc3));
    }

    #[test]
    fn rcon_text_leaves_in_windows_1252() {
        let packet = connectionless_packet("rcon secret say ¤").unwrap();
        assert_eq!(packet, b"\xff\xff\xff\xffrcon secret say \xa4");
    }
}

#[cfg(test)]
mod info_query_tests {
    use super::{INFO_RESENDS, INFO_SEND_BATCH, info_read_goes_on, query_server_infos};
    use std::io;
    use std::net::{SocketAddr, UdpSocket};
    use std::time::Duration;

    /// A dead server's "port unreachable", which Windows hands over as a reset on the next
    /// read, does not end the query: the servers still to answer are waited for.
    #[test]
    fn a_reset_from_a_dead_server_leaves_the_others_to_come() {
        for kind in [
            io::ErrorKind::WouldBlock,
            io::ErrorKind::TimedOut,
            io::ErrorKind::ConnectionReset,
        ] {
            assert!(info_read_goes_on(&io::Error::from(kind)), "{kind:?}");
        }
        assert!(!info_read_goes_on(&io::Error::from(
            io::ErrorKind::PermissionDenied
        )));
    }

    /// A server that drops the first request (a lost packet, a rate limit) is asked
    /// again and listed; one that never answers is asked [`INFO_RESENDS`] more times.
    #[test]
    fn a_server_that_missed_the_first_request_is_asked_again() {
        let lossy = UdpSocket::bind("127.0.0.1:0").expect("a lossy server");
        let silent = UdpSocket::bind("127.0.0.1:0").expect("a silent server");
        let addresses = [lossy.local_addr().unwrap(), silent.local_addr().unwrap()];
        let answering = std::thread::spawn(move || {
            let mut packet = [0_u8; 1_400];
            lossy
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let _ = lossy.recv_from(&mut packet).expect("the first getinfo");
            let (_, client) = lossy.recv_from(&mut packet).expect("the second getinfo");
            let mut answer = vec![0xff; 4];
            answer.extend(b"infoResponse\n\\hostname\\lossy\n");
            lossy.send_to(&answer, client).unwrap();
            silent
                .set_read_timeout(Some(Duration::from_millis(600)))
                .unwrap();
            std::iter::from_fn(|| silent.recv_from(&mut packet).ok()).count()
        });
        let mut names = Vec::new();
        query_server_infos(&addresses, Duration::from_millis(300), |_, info, _| {
            names.push(info.get("hostname").unwrap_or_default().to_owned());
        })
        .expect("the query");
        assert_eq!(names, ["lossy"]);
        assert_eq!(answering.join().unwrap(), 1 + INFO_RESENDS);
    }

    /// Every one of several batches of servers answering at once is listed, with a
    /// silent address among them.
    #[test]
    fn every_server_of_a_long_list_answers() {
        let count = INFO_SEND_BATCH * 3 + 5;
        let servers: Vec<UdpSocket> = (0..count)
            .map(|_| UdpSocket::bind("127.0.0.1:0").expect("a local server"))
            .collect();
        let silent = UdpSocket::bind("127.0.0.1:0").expect("a silent server");
        let mut addresses: Vec<SocketAddr> = servers
            .iter()
            .map(|server| server.local_addr().unwrap())
            .collect();
        addresses.insert(count / 2, silent.local_addr().unwrap());
        let answering = std::thread::spawn(move || {
            let mut packet = [0_u8; 1_400];
            for (index, server) in servers.iter().enumerate() {
                server
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let (_, client) = server.recv_from(&mut packet).expect("a getinfo");
                let mut answer = vec![0xff; 4];
                answer.extend(
                    format!("infoResponse\n\\hostname\\server {index}\\clients\\0\n").bytes(),
                );
                server.send_to(&answer, client).unwrap();
            }
        });
        let mut names = Vec::new();
        query_server_infos(&addresses, Duration::from_millis(500), |_, info, _| {
            names.push(info.get("hostname").unwrap_or_default().to_owned());
        })
        .expect("the query");
        answering.join().unwrap();
        names.sort();
        let mut wanted: Vec<String> = (0..count).map(|index| format!("server {index}")).collect();
        wanted.sort();
        assert_eq!(names, wanted);
    }
}
