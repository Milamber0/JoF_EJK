//! The compressed connection-request boundary for a future native server.
use sjk_protocol::{AdaptiveHuffmanError, MAX_LEGACY_MESSAGE_BYTES, decompress_connect_block};
use std::{error::Error, fmt};

const CONNECT_HEADER: &[u8] = b"\xff\xff\xff\xffconnect ";

/// Decode a stock connection packet into its raw command arguments.
///
/// The result retains the surrounding userinfo quotes and any non-UTF-8 bytes.
/// Tokenization, userinfo validation, challenge verification, bans and admission
/// are separate session policies; successful decompression does not admit a client.
/// Matches `NET_OutOfBandData` / `SV_ConnectionlessPacket`'s offset of 12 bytes.
pub fn decode_connect_packet(packet: &[u8]) -> Result<Vec<u8>, ConnectPacketError> {
    if !packet.starts_with(CONNECT_HEADER) {
        return Err(ConnectPacketError::Header);
    }
    decode_connect_block(packet)
}

/// Decompress from the reference's fixed offset of 12 bytes, whatever precedes it:
/// the dispatcher decompresses every payload that starts with `connect`.
pub(crate) fn decode_connect_block(packet: &[u8]) -> Result<Vec<u8>, ConnectPacketError> {
    let block = packet.get(CONNECT_HEADER.len()..).unwrap_or_default();
    if block.len() < 2 {
        return Err(ConnectPacketError::Compression(
            AdaptiveHuffmanError::TruncatedInput,
        ));
    }
    if packet.len() > MAX_LEGACY_MESSAGE_BYTES {
        return Err(ConnectPacketError::PacketTooLarge);
    }
    decompress_connect_block(block, MAX_LEGACY_MESSAGE_BYTES - CONNECT_HEADER.len())
        .map_err(ConnectPacketError::Compression)
}

/// A malformed or oversized compressed connection packet.
#[derive(Debug, PartialEq, Eq)]
pub enum ConnectPacketError {
    /// The connectionless prefix and literal `connect ` header are required.
    Header,
    /// The datagram exceeds the legacy message capacity.
    PacketTooLarge,
    /// The adaptive Huffman block cannot be decoded within the capacity.
    Compression(AdaptiveHuffmanError),
}

impl fmt::Display for ConnectPacketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Header => f.write_str("invalid compressed connect header"),
            Self::PacketTooLarge => f.write_str("connect packet exceeds legacy capacity"),
            Self::Compression(error) => error.fmt(f),
        }
    }
}

impl Error for ConnectPacketError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Compression(error) => Some(error),
            _ => None,
        }
    }
}
