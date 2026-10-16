//! `NET_StringToAdr` as the reference reads an operator's address: `localhost`, an
//! `inet_addr` number, or a name looked up through the host.
use super::{LegacyBanAddress, PORT_SERVER};
use std::net::{Ipv4Addr, SocketAddrV4};

/// `inet_addr` (glibc's `inet_aton` rules): one to four parts, each decimal, octal with
/// a leading `0` or hexadecimal with `0x`; the last part fills the remaining bytes.
/// `None` where the reference returns `INADDR_NONE`.
pub fn legacy_inet_addr(text: &str) -> Option<Ipv4Addr> {
    let bytes = text.as_bytes();
    let (mut parts, mut at) = (Vec::with_capacity(4), 0);
    loop {
        if !bytes.get(at).is_some_and(u8::is_ascii_digit) {
            return None;
        }
        let (base, start) = match (bytes[at], bytes.get(at + 1)) {
            (b'0', Some(b'x' | b'X')) => (16, at + 2),
            (b'0', _) => (8, at),
            _ => (10, at),
        };
        let mut value: u64 = 0;
        let mut end = start;
        while let Some(digit) = bytes
            .get(end)
            .and_then(|byte| (*byte as char).to_digit(base))
        {
            value = value * u64::from(base) + u64::from(digit);
            if value > u64::from(u32::MAX) {
                return None;
            }
            end += 1;
        }
        parts.push(value as u32);
        at = end;
        match bytes.get(at) {
            Some(b'.') if parts.len() < 4 => at += 1,
            None => break,
            Some(byte) if byte.is_ascii_whitespace() => break,
            _ => return None,
        }
    }
    let limits: &[u32] = match parts.len() {
        1 => &[u32::MAX],
        2 => &[0xff, 0xff_ffff],
        3 => &[0xff, 0xff, 0xffff],
        _ => &[0xff, 0xff, 0xff, 0xff],
    };
    if parts.iter().zip(limits).any(|(part, limit)| part > limit) {
        return None;
    }
    let value = match parts.as_slice() {
        [a] => *a,
        [a, b] => a << 24 | b,
        [a, b, c] => a << 24 | b << 16 | c,
        [a, b, c, d] => a << 24 | b << 16 | c << 8 | d,
        _ => return None,
    };
    Some(Ipv4Addr::from(value))
}

/// `NET_StringToAdr`: `localhost`, or an address with an optional `:port` (the
/// server's own port without one). A text starting with a digit is a number; any other
/// is a name, which `resolve` looks up. `255.255.255.255` is never an address.
pub fn legacy_string_to_address(
    text: &str,
    resolve: &mut dyn FnMut(&str) -> Option<Ipv4Addr>,
) -> LegacyBanAddress {
    if text == "localhost" {
        return LegacyBanAddress::Loopback;
    }
    let (base, port) = match text.split_once(':') {
        Some((base, port)) => (base, Some(port)),
        None => (text, None),
    };
    let address = if base.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        legacy_inet_addr(base).unwrap_or(Ipv4Addr::BROADCAST)
    } else {
        match resolve(base) {
            Some(address) => address,
            None => return LegacyBanAddress::Bad,
        }
    };
    if address == Ipv4Addr::BROADCAST {
        return LegacyBanAddress::Bad;
    }
    let port = port.map_or(PORT_SERVER, |port| super::atoi(port.as_bytes()) as u16);
    LegacyBanAddress::Ip(SocketAddrV4::new(address, port))
}
