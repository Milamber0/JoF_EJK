//! Out-of-band query replies: `getinfo`, `getstatus` and `getchallenge`.
//!
//! Mirrors `SVC_Info`, `SVC_Status` and `SV_GetChallenge` in OpenJK multiplayer.
//! Population figures and the player list are inputs, not read from the legacy
//! roster: what a server with more native players than wire client numbers
//! advertises is the session owner's projection policy, and these replies are
//! text that can describe any count. [`legacy_roster_population`] supplies the
//! stock figures for a server that has nothing beyond its roster.
use crate::{JKA_PROTOCOL, LegacyClientPhase, LegacyPeerAddress, LegacyRosterHost};
mod info;
pub use info::LegacyInfoString;
use info::c_string;
pub(crate) use sjk_protocol::info_value;

/// Longest query argument the reference answers; longer requests get no reply.
const MAX_QUERY_CHALLENGE: usize = 128;
/// `MAX_MSGLEN`: the reference formats a reply into a buffer of this size.
const MAX_REPLY: usize = 49_152;
/// `GT_DUEL` and `GT_POWERDUEL` advertise the duel weapon mask instead.
const DUEL_GAMETYPES: [i32; 2] = [3, 4];

/// Everything `getinfo` reports, as the session owner chooses to present it.
#[derive(Clone, Copy, Debug)]
pub struct LegacyServerInfo<'a> {
    /// `sv_hostname`.
    pub hostname: &'a [u8],
    /// `mapname`.
    pub mapname: &'a [u8],
    /// `fs_game`; omitted from the reply when empty.
    pub game_directory: &'a [u8],
    /// Connected players advertised, public slots only in the reference.
    pub clients: i32,
    /// How many of [`Self::clients`] are not bots.
    pub humans: i32,
    /// Public capacity advertised (`sv_maxclients - sv_privateClients` in stock).
    pub max_clients: i32,
    /// `g_gametype`.
    pub gametype: i32,
    /// `g_needpass`.
    pub needpass: i32,
    /// `g_jediVmerc`.
    pub true_jedi: i32,
    /// `g_weaponDisable`.
    pub weapon_disable: i32,
    /// `g_duelWeaponDisable`, advertised instead in duel gametypes.
    pub duel_weapon_disable: i32,
    /// `g_forcePowerDisable`.
    pub force_disable: i32,
    /// `sv_autoDemo`.
    pub auto_demo: i32,
    /// `sv_minPing`; omitted when zero.
    pub min_ping: i32,
    /// `sv_maxPing`; omitted when zero.
    pub max_ping: i32,
}

/// One line of a status reply.
#[derive(Clone, Copy, Debug)]
pub struct LegacyStatusPlayer<'a> {
    /// `PERS_SCORE` of the player's state.
    pub score: i32,
    /// Measured ping in milliseconds.
    pub ping: i32,
    /// Display name; sent between quotes without escaping, as the reference does.
    pub name: &'a [u8],
}

/// Stock population figures: connected wire clients outside the private range.
pub fn legacy_roster_population(host: &impl LegacyRosterHost, private_clients: i32) -> (i32, i32) {
    let start = usize::try_from(private_clients).unwrap_or(0);
    let (mut clients, mut humans) = (0, 0);
    for slot in start..host.client_count() {
        if host.phase(slot) >= LegacyClientPhase::Connected {
            clients += 1;
            humans += i32::from(host.peer(slot).address != Some(LegacyPeerAddress::Bot));
        }
    }
    (clients, humans)
}

/// Append a complete `infoResponse` datagram; `false` means send nothing.
///
/// `challenge` is the query's first argument, echoed so the asker can match the
/// reply. Pairs that do not fit the 1,023-byte string are left out one by one.
pub fn write_legacy_info_response(
    out: &mut Vec<u8>,
    challenge: &[u8],
    server: &LegacyServerInfo<'_>,
) -> bool {
    if c_string(challenge).len() > MAX_QUERY_CHALLENGE {
        return false;
    }
    let mut info = LegacyInfoString::new();
    let number = |info: &mut LegacyInfoString, key: &[u8], value: i32| {
        info.set(key, value.to_string().as_bytes());
    };
    info.set(b"challenge", challenge);
    number(&mut info, b"protocol", JKA_PROTOCOL as i32);
    info.set(b"hostname", server.hostname);
    info.set(b"mapname", server.mapname);
    number(&mut info, b"clients", server.clients);
    number(&mut info, b"g_humanplayers", server.humans);
    number(&mut info, b"sv_maxclients", server.max_clients);
    number(&mut info, b"gametype", server.gametype);
    number(&mut info, b"needpass", server.needpass);
    number(&mut info, b"truejedi", server.true_jedi);
    let weapons = if DUEL_GAMETYPES.contains(&server.gametype) {
        server.duel_weapon_disable
    } else {
        server.weapon_disable
    };
    number(&mut info, b"wdisable", weapons);
    number(&mut info, b"fdisable", server.force_disable);
    number(&mut info, b"autodemo", server.auto_demo);
    if server.min_ping != 0 {
        number(&mut info, b"minPing", server.min_ping);
    }
    if server.max_ping != 0 {
        number(&mut info, b"maxPing", server.max_ping);
    }
    info.set(b"game", server.game_directory);
    out.extend_from_slice(b"\xff\xff\xff\xffinfoResponse\n");
    out.extend_from_slice(info.as_bytes());
    true
}

/// Append a complete `statusResponse` datagram; `false` means send nothing.
///
/// `serverinfo` is the server's `CVAR_SERVERINFO` string. Players are listed until
/// the reference's reply buffer would be full; the rest are left out, whole lines
/// only. A roster-sized server never reaches that limit.
pub fn write_legacy_status_response<'a>(
    out: &mut Vec<u8>,
    challenge: &[u8],
    serverinfo: &[u8],
    players: impl IntoIterator<Item = LegacyStatusPlayer<'a>>,
) -> bool {
    if c_string(challenge).len() > MAX_QUERY_CHALLENGE {
        return false;
    }
    let mut info = LegacyInfoString::from_truncated(serverinfo);
    info.set(b"challenge", challenge);
    let start = out.len();
    out.extend_from_slice(b"\xff\xff\xff\xffstatusResponse\n");
    out.extend_from_slice(info.as_bytes());
    out.push(b'\n');
    let (list, mut line) = (out.len(), Vec::new());
    for player in players {
        line.clear();
        line.extend_from_slice(format!("{} {} \"", player.score, player.ping).as_bytes());
        line.extend_from_slice(c_string(player.name));
        line.extend_from_slice(b"\"\n");
        if out.len() - list + line.len() >= MAX_REPLY {
            break;
        }
        out.extend_from_slice(&line);
    }
    // The reference formats the whole reply into one buffer of the same size.
    out.truncate(out.len().min(start + MAX_REPLY - 1));
    true
}

/// Append a complete `challengeResponse` datagram.
///
/// `client_argument` is the query's first argument; its integer value is echoed.
pub fn write_legacy_challenge_response(out: &mut Vec<u8>, challenge: i32, client_argument: &[u8]) {
    let echoed = crate::connect_request::decimal(client_argument);
    out.extend_from_slice(b"\xff\xff\xff\xff");
    out.extend_from_slice(format!("challengeResponse {challenge} {echoed}").as_bytes());
}
