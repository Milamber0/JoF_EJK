//! `SV_DirectConnect`: from a decoded connect request to an occupied roster slot.
use super::{LegacyPeer, LegacyPeerAddress, LegacyRosterHost};
use crate::{JKA_PROTOCOL, LegacyClientPhase, LegacyConnectRequest};

/// Complete out-of-band reply that tells an admitted client to start its channel.
pub const LEGACY_CONNECT_RESPONSE: &[u8] = b"\xff\xff\xff\xffconnectResponse";

const MAX_INFO_STRING: usize = 1024;

/// One `connect` request and the transport facts the socket owner knows about it.
pub struct LegacyConnectAttempt<'a> {
    /// Datagram source; [`LegacyPeerAddress::Bot`] is never a valid source.
    pub from: LegacyPeerAddress,
    /// Decoded and normalized request (step 331).
    pub request: &'a LegacyConnectRequest,
    /// Verdict of the server's ban list, an administration capability.
    pub banned: bool,
}

/// Stock connection cvars at the current server time.
pub struct LegacyConnectPolicy<'a> {
    /// Current server time in integer milliseconds.
    pub now: i32,
    /// `sv_reconnectlimit`: seconds an endpoint must wait between connections.
    pub reconnect_limit_seconds: i32,
    /// `sv_privateClients`: leading slots reserved for the private password.
    pub private_clients: i32,
    /// `sv_privatePassword`; the empty default matches a client without one.
    pub private_password: &'a [u8],
}

/// Why a connection was refused. [`Self::write_reply`] is what the client is told.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacyConnectRefusal {
    /// The source address is banned.
    Banned,
    /// The client speaks another protocol; carries the version it declared.
    Protocol(i32),
    /// The same endpoint connected less than the reconnect limit ago.
    TooSoon,
    /// Userinfo leaves no room for the server's `ip` key.
    UserinfoLength,
    /// The challenge does not belong to this address.
    Challenge,
    /// No free wire client number in the range this client may use. The native
    /// server may still have capacity; legacy clients cannot be shown more.
    Full,
    /// The authoritative server or its game refused, with this text.
    Game(Vec<u8>),
}

impl LegacyConnectRefusal {
    /// Append the complete out-of-band `print` datagram the reference sends.
    pub fn write_reply(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(b"\xff\xff\xff\xffprint\n");
        match self {
            Self::Banned => out.extend_from_slice(b"You are banned from this server."),
            Self::Protocol(version) => out.extend_from_slice(
                format!("Server uses protocol version {JKA_PROTOCOL} (yours is {version}).")
                    .as_bytes(),
            ),
            Self::TooSoon => out.extend_from_slice(b"Reconnect rejected : too soon"),
            Self::UserinfoLength => out.extend_from_slice(
                b"Userinfo string length exceeded.  Try removing setu cvars from your config.",
            ),
            Self::Challenge => out.extend_from_slice(b"Incorrect challenge for your address."),
            // A string reference the client localizes itself.
            Self::Full => out.extend_from_slice(b"@@@SERVER_IS_FULL"),
            Self::Game(text) => out.extend_from_slice(text),
        }
        out.push(b'\n');
    }
}

/// Result of a connection attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacyConnectOutcome {
    /// The slot is connected; answer [`LEGACY_CONNECT_RESPONSE`].
    Accepted {
        /// Wire client number now bound to this endpoint.
        slot: usize,
        /// The endpoint already held this slot; its old session was discarded.
        reconnected: bool,
    },
    /// Answer with [`LegacyConnectRefusal::write_reply`].
    Refused(LegacyConnectRefusal),
    /// A listen server's own client found no slot and not every slot is a bot.
    /// The reference aborts the process here; this server reports it and sends nothing.
    LocalServerFull,
}

/// Admit, reconnect or refuse one connection request, in the reference's order.
///
/// Checks run as ban, protocol, reconnect limit, userinfo length, challenge, slot.
/// The reconnect limit reads free slots too and lets the first matching slot
/// decide. A refusal by the game leaves the chosen slot free but rebound to the
/// new endpoint with its previous session already discarded; both are stock.
pub fn connect_legacy_client(
    host: &mut impl LegacyRosterHost,
    attempt: LegacyConnectAttempt<'_>,
    policy: LegacyConnectPolicy<'_>,
) -> LegacyConnectOutcome {
    use LegacyConnectOutcome::Refused;
    let (from, request) = (attempt.from, attempt.request);
    if attempt.banned {
        return Refused(LegacyConnectRefusal::Banned);
    }
    if request.protocol() != JKA_PROTOCOL as i32 {
        return Refused(LegacyConnectRefusal::Protocol(request.protocol()));
    }
    let (challenge, qport) = (request.challenge(), request.qport());
    let count = host.client_count();
    if let Some(slot) = (0..count).find(|&slot| host.peer(slot).matches(from, qport)) {
        let waited = policy.now.wrapping_sub(host.peer(slot).last_connect_time);
        if waited < policy.reconnect_limit_seconds.wrapping_mul(1000) {
            return Refused(LegacyConnectRefusal::TooSoon);
        }
    }
    let userinfo = &request.userinfo()[..request.userinfo().len().min(MAX_INFO_STRING - 1)];
    let ip_length = match from {
        LegacyPeerAddress::Ip(address) => address.to_string().len(),
        _ => "localhost".len(),
    };
    if ip_length + userinfo.len() + 4 >= MAX_INFO_STRING {
        return Refused(LegacyConnectRefusal::UserinfoLength);
    }
    if let LegacyPeerAddress::Ip(address) = from
        && !host.verify_challenge(challenge, address)
    {
        return Refused(LegacyConnectRefusal::Challenge);
    }
    let occupied = (0..count).find(|&slot| {
        host.phase(slot) != LegacyClientPhase::Free && host.peer(slot).matches(from, qport)
    });
    let slot = match occupied {
        Some(slot) => {
            host.game_disconnect(slot);
            slot
        }
        None => match free_slot(host, from, request, &policy) {
            Ok(slot) => slot,
            Err(outcome) => return outcome,
        },
    };
    // `*newcl = temp`: nothing of the previous occupant survives, whatever follows.
    host.set_phase(slot, LegacyClientPhase::Free);
    *host.peer_mut(slot) = LegacyPeer {
        address: Some(from),
        qport,
        challenge,
        ..LegacyPeer::default()
    };
    host.reset_session(slot, challenge);
    if let Err(text) = host.game_connect(slot, userinfo) {
        return Refused(LegacyConnectRefusal::Game(text));
    }
    host.userinfo_changed(slot);
    host.set_phase(slot, LegacyClientPhase::Connected);
    let peer = host.peer_mut(slot);
    peer.last_packet_time = policy.now;
    peer.last_connect_time = policy.now;
    peer.gamestate_message = -1;
    // The first client and the one that fills the roster are announced at once.
    let connected = (0..count)
        .filter(|&slot| host.phase(slot) >= LegacyClientPhase::Connected)
        .count();
    if connected == 1 || connected == count {
        host.heartbeat();
    }
    LegacyConnectOutcome::Accepted {
        slot,
        reconnected: occupied.is_some(),
    }
}

/// First free slot outside the reserved range, or the listen server's bot eviction.
fn free_slot(
    host: &mut impl LegacyRosterHost,
    from: LegacyPeerAddress,
    request: &LegacyConnectRequest,
    policy: &LegacyConnectPolicy<'_>,
) -> Result<usize, LegacyConnectOutcome> {
    let count = host.client_count();
    let password = request.value(b"password").unwrap_or_default();
    let start = if password == policy.private_password {
        0
    } else {
        usize::try_from(policy.private_clients).unwrap_or(0)
    };
    if let Some(slot) = (start..count).find(|&slot| host.phase(slot) == LegacyClientPhase::Free) {
        return Ok(slot);
    }
    if from != LegacyPeerAddress::Loopback {
        return Err(LegacyConnectOutcome::Refused(LegacyConnectRefusal::Full));
    }
    let bots = (start..count)
        .filter(|&slot| host.peer(slot).address == Some(LegacyPeerAddress::Bot))
        .count();
    if count == 0 || bots < count.saturating_sub(start) {
        return Err(LegacyConnectOutcome::LocalServerFull);
    }
    host.drop_client(count - 1, b"only bots on server");
    Ok(count - 1)
}
