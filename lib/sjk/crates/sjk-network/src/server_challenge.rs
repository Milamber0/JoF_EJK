//! Stateless IPv4 challenge cookies, matching OpenJK codemp sv_challenge.cpp.
use md5::{Digest, Md5};
use std::net::SocketAddrV4;

/// Per-server challenge secret and precomputed HMAC pads.
///
/// The secret must be supplied from the platform's OS random source at server
/// startup. There is deliberately no default or Debug implementation. This
/// adapter implements the existing OpenJK handshake, not a general auth scheme.
pub struct LegacyChallenge {
    inner: Md5,
    outer: Md5,
}

impl LegacyChallenge {
    /// Initialize from a fresh 16-byte OS-generated server secret.
    pub fn new(secret: [u8; 16]) -> Self {
        let mut inner = [0x36; 64];
        let mut outer = [0x5c; 64];
        for (i, byte) in secret.into_iter().enumerate() {
            inner[i] ^= byte;
            outer[i] ^= byte;
        }
        Self {
            inner: Md5::new().chain_update(inner),
            outer: Md5::new().chain_update(outer),
        }
    }

    /// Issue an address/port-bound challenge for signed legacy server milliseconds.
    ///
    /// The timestamp changes every 16,384 ms. Its parity occupies the sign bit.
    /// IPv4 is the pinned reference's network family; engine-internal loopback
    /// bypass/admission and platform time ownership belong to the caller.
    pub fn issue(&self, address: SocketAddrV4, server_time: i32) -> i32 {
        self.at_timestamp(address, server_time >> 14)
    }

    /// Accept a challenge from the current or immediately preceding timestamp.
    /// The original address and UDP source port must both still match.
    pub fn verify(&self, challenge: i32, address: SocketAddrV4, server_time: i32) -> bool {
        let current = server_time >> 14;
        let period = ((challenge as u32) >> 31) as i32;
        self.at_timestamp(address, current - ((current & 1) ^ period)) == challenge
    }

    fn at_timestamp(&self, address: SocketAddrV4, timestamp: i32) -> i32 {
        // NET_AdrToString uses decimal a.b.c.d:port. Formatting is connection-time,
        // never a per-frame or usercmd operation.
        let inner = self
            .inner
            .clone()
            .chain_update(address.to_string().as_bytes())
            .chain_update(timestamp.to_le_bytes())
            .finalize();
        let digest = self.outer.clone().chain_update(inner).finalize();
        let value = u32::from_le_bytes(digest[..4].try_into().unwrap());
        ((value & 0x7fff_ffff) | (((timestamp as u32) & 1) << 31)) as i32
    }
}
