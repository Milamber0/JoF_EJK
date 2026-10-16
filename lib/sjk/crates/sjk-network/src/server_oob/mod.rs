//! Out-of-band requests: rate limiting, line reading and dispatch.
//!
//! Mirrors `SV_ConnectionlessPacket` in OpenJK multiplayer. The socket owner asks
//! the [`LegacyOobLimiter`] first, then parses the datagram and answers with the
//! reply builders of `server_query` or the admission of `server_peers`.
use crate::{ConnectPacketError, LegacyConnectRequest, server_connect::decode_connect_block};
mod limit;
mod tokens;
pub use limit::{LegacyOobAdmission, LegacyOobLimiter, LegacyOobRates};
pub use tokens::{LegacyOobLine, LegacyTokens};

const MARKER: &[u8] = &[0xff; 4];

/// What an out-of-band datagram asks for. Arguments borrow the caller's line.
#[derive(Debug, PartialEq, Eq)]
pub enum LegacyOobRequest<'a> {
    /// `getstatus [challenge]`.
    Status {
        /// First argument, echoed in the reply; empty when absent.
        challenge: &'a [u8],
    },
    /// `getinfo [challenge]`.
    Info {
        /// First argument, echoed in the reply; empty when absent.
        challenge: &'a [u8],
    },
    /// `getchallenge [clientChallenge]`.
    Challenge {
        /// First argument, whose integer value is echoed; empty when absent.
        client_challenge: &'a [u8],
    },
    /// `connect "<userinfo>"`, decompressed.
    Connect(LegacyConnectRequest),
    /// A datagram starting with `connect` whose compressed block is invalid. The
    /// reference would tokenize whatever its unchecked decoder produced.
    MalformedConnect(ConnectPacketError),
    /// `rcon <password> <command...>`.
    Rcon {
        /// The whole line, `rcon` included, as `Cmd_Cmd` keeps it: the command to run is
        /// found by walking it, not from its tokens.
        line: &'a [u8],
    },
    /// `disconnect`, `ipAuthorize`, anything unknown, or no marker: no reply.
    Ignored,
}

/// Read and classify one datagram, using `line` as the only scratch storage.
///
/// Command names match without regard to case. Any payload that *starts with*
/// `connect` is decompressed from its ninth byte before it is read, whatever
/// follows those seven letters, as in the reference.
pub fn parse_legacy_oob<'a>(datagram: &[u8], line: &'a mut LegacyOobLine) -> LegacyOobRequest<'a> {
    let Some(payload) = datagram.strip_prefix(MARKER) else {
        return LegacyOobRequest::Ignored;
    };
    if payload.starts_with(b"connect") {
        match decode_connect_block(datagram) {
            Ok(decoded) => line.read(&[&payload[..payload.len().min(8)], &decoded]),
            Err(error) => return LegacyOobRequest::MalformedConnect(error),
        }
    } else {
        line.read(&[payload]);
    }
    let line_bytes = <&LegacyOobLine>::from(line).as_bytes();
    let mut tokens = LegacyTokens::new(line_bytes);
    let Some(command) = tokens.next() else {
        return LegacyOobRequest::Ignored;
    };
    let is = |name: &[u8]| command.eq_ignore_ascii_case(name);
    if is(b"rcon") {
        return LegacyOobRequest::Rcon { line: line_bytes };
    }
    let argument = tokens.next().unwrap_or_default();
    if is(b"getstatus") {
        LegacyOobRequest::Status {
            challenge: argument,
        }
    } else if is(b"getinfo") {
        LegacyOobRequest::Info {
            challenge: argument,
        }
    } else if is(b"getchallenge") {
        LegacyOobRequest::Challenge {
            client_challenge: argument,
        }
    } else if is(b"connect") {
        LegacyOobRequest::Connect(LegacyConnectRequest::from_userinfo(argument))
    } else {
        LegacyOobRequest::Ignored
    }
}
