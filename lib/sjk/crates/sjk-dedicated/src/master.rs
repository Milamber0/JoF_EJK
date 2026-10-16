//! Master-server heartbeats on this server: `sv_master1`..`sv_master5` and `dedicated`
//! from the console variables, the datagrams on the server's socket, and every master
//! resolved whitelisted ([`sjk_network::LegacyMasterHeartbeat`]).

use crate::bridge::NativeGame;
use sjk_network::{
    LEGACY_MASTER_SERVERS, LegacyClock, LegacyGameHost, LegacyMasterHeartbeat, LegacyMasterHost,
    LegacyServerSession,
};
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};

/// The heartbeats, and the change each master variable had when last read.
#[derive(Default)]
pub struct Masters {
    heartbeat: LegacyMasterHeartbeat,
    /// `modified`: a variable counts as changed until its count is seen; a new one is.
    seen: [Option<i32>; LEGACY_MASTER_SERVERS],
}

impl Masters {
    /// `SV_MasterShutdown`: two heartbeats at once as the server goes down.
    pub fn shutdown(
        &mut self,
        session: &mut LegacyServerSession<NativeGame>,
        clock: LegacyClock,
        socket: &UdpSocket,
    ) {
        let dedicated = session.game().cvars().integer(b"dedicated");
        let mut host = Host {
            session,
            seen: &mut self.seen,
            socket,
        };
        self.heartbeat
            .shutdown(dedicated, clock.server_time, clock.wall_time, &mut host);
    }

    /// `SV_MasterHeartbeat` at the end of a server frame, first honouring any heartbeat
    /// the endpoint asked for (`SV_Heartbeat_f`).
    pub fn frame(
        &mut self,
        session: &mut LegacyServerSession<NativeGame>,
        clock: LegacyClock,
        socket: &UdpSocket,
    ) {
        if session.take_heartbeat_due() {
            self.heartbeat.force();
        }
        let dedicated = session.game().cvars().integer(b"dedicated");
        let mut host = Host {
            session,
            seen: &mut self.seen,
            socket,
        };
        self.heartbeat
            .beat(dedicated, clock.server_time, clock.wall_time, &mut host);
    }
}

struct Host<'a> {
    session: &'a mut LegacyServerSession<NativeGame>,
    seen: &'a mut [Option<i32>; LEGACY_MASTER_SERVERS],
    socket: &'a UdpSocket,
}

fn name(index: usize) -> String {
    format!("sv_master{}", index + 1)
}

impl Host<'_> {
    fn count(&self, index: usize) -> Option<i32> {
        self.session
            .game()
            .cvars()
            .var(name(index).as_bytes())
            .map(|var| var.modification_count)
    }
}

impl LegacyMasterHost for Host<'_> {
    fn master(&self, index: usize) -> String {
        String::from_utf8_lossy(self.session.game().cvars().string(name(index).as_bytes()))
            .into_owned()
    }
    fn master_modified(&mut self, index: usize) -> bool {
        let count = self.count(index);
        std::mem::replace(&mut self.seen[index], count) != count
    }
    fn clear_master(&mut self, index: usize) {
        self.session.game_mut().set_cvar(&name(index), "");
        self.seen[index] = self.count(index);
    }
    fn resolve(&mut self, name: &str) -> Option<Ipv4Addr> {
        crate::config_files::resolve_host(name)
    }
    fn send(&mut self, to: SocketAddrV4, bytes: &[u8]) {
        // A lost heartbeat is sent again in five minutes.
        let _ = self.socket.send_to(bytes, to);
    }
    fn print(&mut self, text: &[u8]) {
        self.session.game_mut().console_log(text);
    }
    fn whitelist(&mut self, address: Ipv4Addr) {
        self.session.whitelist_address(address);
    }
}
