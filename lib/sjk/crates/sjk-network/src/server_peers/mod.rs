//! The legacy peer roster: admission, datagram routing and timeouts.
//!
//! Mirrors `SV_DirectConnect`, `SV_PacketEvent` and `SV_CheckTimeouts` in OpenJK
//! multiplayer. A roster slot is a protocol-26 *wire client number*, a scarce
//! adapter resource. It is not a native peer, world or entity identity and its
//! count is not the server's capacity: the authoritative core admits peers under
//! its own budgets through [`LegacyRosterHost::game_connect`], and an ordinary
//! client that cannot be given a representable wire number is refused explicitly.
use crate::LegacyClientPhase;
use std::net::SocketAddrV4;
mod connect;
pub use connect::{
    LEGACY_CONNECT_RESPONSE, LegacyConnectAttempt, LegacyConnectOutcome, LegacyConnectPolicy,
    LegacyConnectRefusal, connect_legacy_client,
};

/// Wire client numbers protocol 26 can represent (`MAX_CLIENTS`).
///
/// An upper bound for a roster, never a default and never a native capacity.
pub const LEGACY_WIRE_CLIENTS: usize = 32;

/// Complete out-of-band reply to a sequenced datagram from an unknown peer.
pub const LEGACY_UNKNOWN_PEER_REPLY: &[u8] = b"\xff\xff\xff\xffdisconnect";

/// Transport identity of a roster slot, as far as the legacy handshake sees it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyPeerAddress {
    /// In-process client of a listen server; exempt from the challenge.
    Loopback,
    /// Server-side bot. Never a datagram source.
    Bot,
    /// Remote IPv4 endpoint, the pinned reference's only network family.
    Ip(SocketAddrV4),
}

impl LegacyPeerAddress {
    /// `NET_CompareBaseAdr`: same family and host, ignoring the UDP port.
    fn same_host(self, other: Self) -> bool {
        match (self, other) {
            (Self::Loopback, Self::Loopback) => true,
            (Self::Ip(a), Self::Ip(b)) => a.ip() == b.ip(),
            _ => false,
        }
    }
    fn port(self) -> u16 {
        match self {
            Self::Ip(address) => address.port(),
            _ => 0,
        }
    }
}

/// Routing and timing record of one wire client number.
///
/// A slot keeps its last address and connect time after it becomes free, as the
/// reference does: the reconnect limit and the listen-server bot count read them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LegacyPeer {
    /// Last endpoint bound to this slot; `None` until first used.
    pub address: Option<LegacyPeerAddress>,
    /// Declared qport, not narrowed: a value outside 16 bits can never be routed.
    pub qport: i32,
    /// Challenge the client connected with; keys its channel.
    pub challenge: i32,
    /// Server time of the last accepted sequenced packet.
    pub last_packet_time: i32,
    /// Server time of the last accepted connection.
    pub last_connect_time: i32,
    /// Consecutive timeout checks that found this peer silent.
    pub timeout_count: i32,
    /// Outgoing sequence of the last gamestate; -1 forces the first one.
    pub gamestate_message: i32,
}

impl LegacyPeer {
    fn matches(&self, from: LegacyPeerAddress, qport: i32) -> bool {
        self.address.is_some_and(|address| {
            address.same_host(from) && (self.qport == qport || address.port() == from.port())
        })
    }
}

/// The session owner's roster plus the synchronous effects admission requires.
///
/// The production host also implements [`crate::LegacyDropHost`] over the same
/// slots. Effects run before the call returns; recording them is a test fixture.
pub trait LegacyRosterHost {
    /// Configured wire client numbers, at most [`LEGACY_WIRE_CLIENTS`].
    fn client_count(&self) -> usize;
    /// Current phase of a valid slot, including changes made by effects.
    fn phase(&self, slot: usize) -> LegacyClientPhase;
    /// Change a slot's phase without touching any native identity.
    fn set_phase(&mut self, slot: usize, phase: LegacyClientPhase);
    /// Routing record of a valid slot.
    fn peer(&self, slot: usize) -> &LegacyPeer;
    /// Mutable routing record of a valid slot.
    fn peer_mut(&mut self, slot: usize) -> &mut LegacyPeer;
    /// Check a remote client's challenge (see [`crate::LegacyChallenge`]).
    fn verify_challenge(&mut self, challenge: i32, from: SocketAddrV4) -> bool;
    /// Run game disconnect for a slot that is about to be reconnected.
    fn game_disconnect(&mut self, slot: usize);
    /// Discard the slot's whole previous session (channel, reliable and movement
    /// history, userinfo) and build a channel keyed by `challenge`.
    fn reset_session(&mut self, slot: usize, challenge: i32);
    /// Admit the native peer and run game connect; `Err` is the refusal text.
    ///
    /// This is where the authoritative core's own budgets apply. Stock adds an
    /// `ip` userinfo key first; that rewrite belongs to the userinfo step.
    fn game_connect(&mut self, slot: usize, userinfo: &[u8]) -> Result<(), Vec<u8>>;
    /// Apply the accepted userinfo (name, rate, snaps) to the session.
    fn userinfo_changed(&mut self, slot: usize);
    /// Schedule a master heartbeat.
    fn heartbeat(&mut self);
    /// Run the ordinary drop lifecycle ([`crate::drop_legacy_client`]).
    fn drop_client(&mut self, slot: usize, reason: &[u8]);
}

/// Where a received datagram belongs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyRoute {
    /// Four leading `0xff` bytes: handle as an out-of-band request.
    Connectionless,
    /// Sequenced packet for this slot's channel, zombies included.
    Peer {
        /// Roster slot whose channel must process the datagram.
        slot: usize,
        /// The slot's UDP port was rewritten to follow a translating router.
        port_translated: bool,
    },
    /// Sequenced packet from nobody: answer [`LEGACY_UNKNOWN_PEER_REPLY`].
    Unknown,
}

/// Find the owner of a datagram by host and qport, never by UDP port.
///
/// The first occupied slot wins. A datagram too short to hold a qport reads as
/// 65,535, as the reference's out-of-band reader returns -1 past the end.
pub fn route_legacy_datagram(
    host: &mut impl LegacyRosterHost,
    from: LegacyPeerAddress,
    datagram: &[u8],
) -> LegacyRoute {
    if datagram.starts_with(&[0xff; 4]) {
        return LegacyRoute::Connectionless;
    }
    let qport = datagram.get(4..6).map_or(0xffff, |bytes| {
        i32::from(u16::from_le_bytes([bytes[0], bytes[1]]))
    });
    for slot in 0..host.client_count() {
        let peer = *host.peer(slot);
        if host.phase(slot) == LegacyClientPhase::Free
            || peer.qport != qport
            || !peer.address.is_some_and(|address| address.same_host(from))
        {
            continue;
        }
        let port_translated = peer.address.map(LegacyPeerAddress::port) != Some(from.port());
        if port_translated {
            host.peer_mut(slot).address = Some(from);
        }
        return LegacyRoute::Peer {
            slot,
            port_translated,
        };
    }
    LegacyRoute::Unknown
}

/// Note a packet its channel accepted; returns whether its message may execute.
///
/// Zombies process the channel so their final reliable output is acknowledged, but
/// neither refresh their timeout nor execute anything.
pub fn accept_legacy_packet(host: &mut impl LegacyRosterHost, slot: usize, now: i32) -> bool {
    if host.phase(slot) == LegacyClientPhase::Zombie {
        return false;
    }
    host.peer_mut(slot).last_packet_time = now;
    true
}

/// Stock `sv_timeout` / `sv_zombietime`, in seconds, at the current server time.
#[derive(Clone, Copy, Debug)]
pub struct LegacyTimeoutPolicy {
    /// Current server time in integer milliseconds.
    pub now: i32,
    /// Silence after which a connected peer is dropped.
    pub timeout_seconds: i32,
    /// Time a zombie lingers to sink stale packets before its slot is free.
    pub zombie_seconds: i32,
}

/// One per-frame timeout pass over the roster.
///
/// A silent peer is dropped on the sixth consecutive pass and its slot freed at
/// once, skipping the zombie wait. Packet times ahead of the clock (a map change)
/// are pulled back first. Bots stay alive only because their commands refresh
/// their packet time, as in the reference.
pub fn check_legacy_timeouts(host: &mut impl LegacyRosterHost, policy: LegacyTimeoutPolicy) {
    let drop_point = policy
        .now
        .wrapping_sub(policy.timeout_seconds.wrapping_mul(1000));
    let zombie_point = policy
        .now
        .wrapping_sub(policy.zombie_seconds.wrapping_mul(1000));
    for slot in 0..host.client_count() {
        if host.peer(slot).last_packet_time > policy.now {
            host.peer_mut(slot).last_packet_time = policy.now;
        }
        let (phase, last) = (host.phase(slot), host.peer(slot).last_packet_time);
        if phase == LegacyClientPhase::Zombie && last < zombie_point {
            host.set_phase(slot, LegacyClientPhase::Free);
        } else if phase >= LegacyClientPhase::Connected && last < drop_point {
            host.peer_mut(slot).timeout_count += 1;
            if host.peer(slot).timeout_count > 5 {
                host.drop_client(slot, b"timed out");
                host.set_phase(slot, LegacyClientPhase::Free);
            }
        } else {
            host.peer_mut(slot).timeout_count = 0;
        }
    }
}
