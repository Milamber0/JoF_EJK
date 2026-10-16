use super::{LegacyClientPhase, LegacyDropError, LegacyDropHost, drop_legacy_client};
use crate::ReliableError;

/// Send an already formatted stock command to one peer or broadcast it to the roster.
///
/// Like SV_SendServerCommand, stops at NUL and ignores strings over 1,022 bytes.
/// Pre-primed and zombie/free peers receive no new commands. Ring overflow invokes
/// the full drop coordinator synchronously before continuing a broadcast, preserving
/// nested drop order. Console echo is a host concern, not a network side effect.
pub fn send_legacy_server_command(
    host: &mut impl LegacyDropHost,
    client: Option<usize>,
    command: &[u8],
) -> Result<(), LegacyDropError> {
    if client.is_some_and(|client| client >= host.client_count()) {
        return Err(LegacyDropError::InvalidClient);
    }
    let command = c_string(command);
    if command.len() > 1022 {
        return Ok(());
    }
    match client {
        Some(client) => send_one(host, client, command),
        None => {
            for client in 0..host.client_count() {
                send_one(host, client, command)?;
            }
            Ok(())
        }
    }
}

fn send_one(
    host: &mut impl LegacyDropHost,
    client: usize,
    command: &[u8],
) -> Result<(), LegacyDropError> {
    if host.phase(client) < LegacyClientPhase::Primed {
        return Ok(());
    }
    match host.reliable(client).queue_server_command(true, command) {
        Ok(_) => Ok(()),
        Err(ReliableError::ServerOverflow) => {
            drop_legacy_client(host, client, b"Server command overflow")?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

pub(super) fn c_string(bytes: &[u8]) -> &[u8] {
    bytes.split(|&byte| byte == 0).next().unwrap_or_default()
}

pub(super) struct FormattedCommand {
    bytes: [u8; 1022],
    len: usize,
}
impl FormattedCommand {
    pub fn from_parts(parts: &[&[u8]]) -> Option<Self> {
        let mut value = Self {
            bytes: [0; 1022],
            len: 0,
        };
        for part in parts {
            let part = c_string(part);
            if part.len() > value.bytes.len() - value.len {
                return None;
            }
            value.bytes[value.len..value.len + part.len()].copy_from_slice(part);
            value.len += part.len();
        }
        Some(value)
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}
