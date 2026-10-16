//! Stock connection-line normalization and userinfo extraction.
use crate::{ConnectPacketError, LegacyTokens, decode_connect_packet};

const MAX_CONNECTION_LINE: usize = 1023;
const COMMAND_BYTES: usize = b"connect ".len();

/// A decoded connection request, before server or game admission policy.
///
/// Retains legacy byte-valued userinfo. Mirrors `MSG_ReadStringLine`, the first
/// `Cmd_TokenizeString` argument, and `Info_ValueForKey` in multiplayer codemp.
/// It does not authenticate a challenge, reserve a slot, or trust a supplied `ip`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyConnectRequest {
    userinfo: Vec<u8>,
}

impl LegacyConnectRequest {
    /// Decode a compressed stock connection packet and extract its userinfo.
    pub fn from_packet(packet: &[u8]) -> Result<Self, ConnectPacketError> {
        Ok(Self::from_arguments(&decode_connect_packet(packet)?))
    }

    /// Apply the stock server's connection-line handling to decompressed arguments.
    ///
    /// The `connect ` prefix consumes eight bytes of the 1,023-byte line limit.
    /// NUL/newline ends the line, percent becomes dot, and quotes do not use
    /// backslash escaping. Missing userinfo produces an empty request; admission
    /// must reject inappropriate protocol, challenge or profile values separately.
    pub fn from_arguments(arguments: &[u8]) -> Self {
        let mut line = [0_u8; MAX_CONNECTION_LINE - COMMAND_BYTES];
        let mut length = 0;
        for &byte in arguments.iter().take(line.len()) {
            if byte == 0 || byte == b'\n' {
                break;
            }
            line[length] = if byte == b'%' { b'.' } else { byte };
            length += 1;
        }
        Self::from_userinfo(
            LegacyTokens::new(&line[..length])
                .next()
                .unwrap_or_default(),
        )
    }

    /// Wrap a userinfo argument the out-of-band dispatcher already tokenized.
    pub fn from_userinfo(userinfo: &[u8]) -> Self {
        Self {
            userinfo: userinfo.to_vec(),
        }
    }

    /// Normalized raw userinfo; supplied address or authentication fields are untrusted.
    pub fn userinfo(&self) -> &[u8] {
        &self.userinfo
    }

    /// First case-insensitive matching value, matching legacy duplicate-key lookup.
    pub fn value(&self, key: &[u8]) -> Option<&[u8]> {
        crate::server_query::info_value(&self.userinfo, key)
    }

    /// Protocol integer as read by stock `SV_DirectConnect`.
    pub fn protocol(&self) -> i32 {
        decimal(self.value(b"protocol").unwrap_or_default())
    }

    /// Client's connection challenge; validation is a separate server operation.
    pub fn challenge(&self) -> i32 {
        decimal(self.value(b"challenge").unwrap_or_default())
    }

    /// Client's declared qport before the stock netchan's narrowing to 16 bits.
    pub fn qport(&self) -> i32 {
        decimal(self.value(b"qport").unwrap_or_default())
    }
}

// Linux x86-64 reference atoi: strtol-style prefix, then narrowing to int.
// Values outside the portable int range are pinned to that reference by fixtures.
pub(crate) fn decimal(mut bytes: &[u8]) -> i32 {
    bytes = bytes.trim_ascii_start();
    let negative = bytes.first() == Some(&b'-');
    if matches!(bytes.first(), Some(b'-' | b'+')) {
        bytes = &bytes[1..];
    }
    let limit = i64::MAX as u64 + u64::from(negative);
    let mut value = 0_u64;
    for &byte in bytes.iter().take_while(|byte| byte.is_ascii_digit()) {
        value = value
            .saturating_mul(10)
            .saturating_add(u64::from(byte - b'0'))
            .min(limit);
    }
    let bits = if negative {
        value.wrapping_neg()
    } else {
        value
    };
    bits as i32
}
