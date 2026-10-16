//! Master-server heartbeats (`SV_MasterHeartbeat`, `sv_main.cpp:199-305`): a server
//! run with `dedicated 2` tells each master server named in `sv_master1`..`sv_master5`
//! that it is up, every five minutes and at once when asked (`heartbeat`, a map change,
//! the last player leaving), and twice more when it shuts down. The master then queries
//! it like any browser.
//!
//! As in the reference, a master's port is always the master port: its code tests
//! `strstr(":", name)`, with the arguments the wrong way round, so a port an operator
//! gives is overwritten.

use crate::{LegacyBanAddress, legacy_string_to_address};
use std::net::{Ipv4Addr, SocketAddrV4};

/// `MAX_MASTER_SERVERS`.
pub const LEGACY_MASTER_SERVERS: usize = 5;
/// `HEARTBEAT_MSEC`.
const HEARTBEAT_MSEC: i32 = 300 * 1000;
/// `NEW_RESOLVE_DURATION`: a master's name is looked up again once a day.
const NEW_RESOLVE_DURATION: i32 = 86_400_000;
/// `PORT_MASTER`.
const PORT_MASTER: u16 = 29060;
/// What a heartbeat says (`HEARTBEAT_GAME`).
const HEARTBEAT: &[u8] = b"\xff\xff\xff\xffheartbeat QuakeArena-1\n";

/// What a heartbeat needs from the server.
pub trait LegacyMasterHost {
    /// `sv_master<index + 1>`: empty for none.
    fn master(&self, index: usize) -> String;
    /// Whether the variable changed since this was last asked (`modified`), which it
    /// then forgets.
    fn master_modified(&mut self, index: usize) -> bool;
    /// `Cvar_Set(sv_master<index + 1>, "")`, after a name that does not resolve; the
    /// change counts as seen.
    fn clear_master(&mut self, index: usize);
    /// Look a name up (`gethostbyname`).
    fn resolve(&mut self, name: &str) -> Option<Ipv4Addr>;
    /// Send one datagram.
    fn send(&mut self, to: SocketAddrV4, bytes: &[u8]);
    /// A line for the server's console.
    fn print(&mut self, text: &[u8]);
    /// `SVC_WhitelistAdr`: a master resolved is whitelisted.
    fn whitelist(&mut self, address: Ipv4Addr);
}

/// The heartbeats' state (`svs.nextHeartbeatTime`, the resolved addresses and when each
/// was looked up).
#[derive(Clone, Debug)]
pub struct LegacyMasterHeartbeat {
    next: i32,
    addresses: [LegacyBanAddress; LEGACY_MASTER_SERVERS],
    resolved_at: [i32; LEGACY_MASTER_SERVERS],
}

impl Default for LegacyMasterHeartbeat {
    fn default() -> Self {
        Self {
            next: 0,
            addresses: [LegacyBanAddress::Bad; LEGACY_MASTER_SERVERS],
            resolved_at: [0; LEGACY_MASTER_SERVERS],
        }
    }
}

impl LegacyMasterHeartbeat {
    /// `SV_Heartbeat_f`: the next heartbeat is due at once.
    pub fn force(&mut self) {
        self.next = -9_999_999;
    }

    /// When the next heartbeat is due, in server time.
    pub fn next(&self) -> i32 {
        self.next
    }

    /// `SV_MasterHeartbeat` in a server frame: `dedicated` is the variable (only 2 sends),
    /// `server_time` is `svs.time` and `wall` is `Com_Milliseconds`.
    pub fn beat(
        &mut self,
        dedicated: i32,
        server_time: i32,
        wall: i32,
        host: &mut impl LegacyMasterHost,
    ) {
        if dedicated != 2 || server_time < self.next {
            return;
        }
        self.next = server_time.wrapping_add(HEARTBEAT_MSEC);
        for index in 0..LEGACY_MASTER_SERVERS {
            let name = host.master(index);
            if name.is_empty() {
                continue;
            }
            // `SV_MasterNeedsResolving`: the clock went back, or a day has passed.
            let stale = self.resolved_at[index] > wall
                || wall.wrapping_sub(self.resolved_at[index]) > NEW_RESOLVE_DURATION;
            if host.master_modified(index) || stale {
                self.resolved_at[index] = wall;
                host.print(format!("Resolving {name}\n").as_bytes());
                let mut address = legacy_string_to_address(&name, &mut |name| host.resolve(name));
                if address == LegacyBanAddress::Bad {
                    host.print(format!("Couldn't resolve address: {name}\n").as_bytes());
                    host.clear_master(index);
                    continue;
                }
                if let LegacyBanAddress::Ip(ip) = &mut address {
                    ip.set_port(PORT_MASTER);
                }
                host.print(format!("{name} resolved to {}\n", address.to_text()).as_bytes());
                if let LegacyBanAddress::Ip(ip) = address {
                    host.whitelist(*ip.ip());
                }
                self.addresses[index] = address;
            }
            host.print(format!("Sending heartbeat to {name}\n").as_bytes());
            // A loopback master is this process's own client, which a dedicated server
            // does not have.
            if let LegacyBanAddress::Ip(to) = self.addresses[index] {
                host.send(to, HEARTBEAT);
            }
        }
    }

    /// `SV_MasterShutdown`: two heartbeats at once, so that a master polls the server,
    /// finds it gone and drops it.
    pub fn shutdown(
        &mut self,
        dedicated: i32,
        server_time: i32,
        wall: i32,
        host: &mut impl LegacyMasterHost,
    ) {
        for _ in 0..2 {
            self.next = -9999;
            self.beat(dedicated, server_time, wall, host);
        }
    }
}
