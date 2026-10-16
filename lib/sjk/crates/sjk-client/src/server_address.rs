//! BaseJKA server-address normalization for console and menu clients.

use std::fmt;
use std::net::Ipv4Addr;

const DEFAULT_SERVER_PORT: u16 = 29_070;

/// A validated host and port accepted by the legacy `connect` command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyServerAddress {
    normalized: String,
}

impl LegacyServerAddress {
    /// Parse an IPv4 literal or hostname and supply BaseJKA's default port.
    ///
    /// OpenJK's `CL_Connect_f` assigns `PORT_SERVER` when
    /// `NET_StringToAdr` returns an address with port zero
    /// (`codemp/client/cl_main.cpp:1008-1068`). Quotes are accepted here so
    /// the same parser can be tested independently of console tokenization.
    pub fn parse(input: &str) -> Result<Self, LegacyAddressError> {
        let input = unquote(input.trim())?;
        if input.is_empty() || input.chars().any(char::is_whitespace) {
            return Err(LegacyAddressError::InvalidHost);
        }
        let (host, port) = match input.split_once(':') {
            Some((host, port)) if !port.contains(':') => {
                let port = port
                    .parse::<u16>()
                    .ok()
                    .filter(|port| *port != 0)
                    .ok_or(LegacyAddressError::InvalidPort)?;
                (host, port)
            }
            Some(_) => return Err(LegacyAddressError::InvalidHost),
            None => (input, DEFAULT_SERVER_PORT),
        };
        if !valid_host(host) {
            return Err(LegacyAddressError::InvalidHost);
        }
        Ok(Self {
            normalized: format!("{host}:{port}"),
        })
    }

    /// Return the normalized `host:port` passed to the resolver.
    pub fn as_str(&self) -> &str {
        &self.normalized
    }

    /// Consume the address into its normalized connection string.
    pub fn into_string(self) -> String {
        self.normalized
    }
}

fn unquote(input: &str) -> Result<&str, LegacyAddressError> {
    if let Some(inner) = input
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    {
        return Ok(inner);
    }
    if input.starts_with('"') || input.ends_with('"') {
        return Err(LegacyAddressError::InvalidHost);
    }
    Ok(input)
}

fn valid_host(host: &str) -> bool {
    if host.parse::<Ipv4Addr>().is_ok() {
        return true;
    }
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

/// Validation failure for a legacy server address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyAddressError {
    /// The host is empty or is not an IPv4 literal/hostname.
    InvalidHost,
    /// The explicit port is missing, zero, or outside `u16`.
    InvalidPort,
}

impl fmt::Display for LegacyAddressError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidHost => "invalid server host",
            Self::InvalidPort => "invalid server port",
        })
    }
}

impl std::error::Error for LegacyAddressError {}
