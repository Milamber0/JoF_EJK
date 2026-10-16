//! The operator's console on this endpoint: `rcon` from the network and lines typed at
//! the server itself, run against the roster ([`crate::LegacyConsoleHost`]) with
//! whatever no engine command claims passed on to the game.
use super::{LegacyClock, LegacyGameHost, LegacyServerSession, hosts::View};
use crate::{
    LegacyClientPhase, LegacyConsoleHost, LegacyConsoleStatus, LegacyPeerAddress,
    execute_legacy_console,
};

/// The engine's own console settings, as the operator sets them.
#[derive(Clone, Debug)]
pub struct LegacyConsoleSettings {
    /// `rconpassword`: empty refuses every `rcon`.
    pub rcon_password: Vec<u8>,
    /// `fs_game`, or `base`.
    pub game_directory: Vec<u8>,
    /// `net_ip` and `net_port`, as `status` reports them.
    pub net_ip: Vec<u8>,
    pub net_port: i32,
    /// `dedicated`: 1 LAN, 2 public (the dedicated build's default).
    pub dedicated: i32,
}

impl Default for LegacyConsoleSettings {
    /// The reference's defaults.
    fn default() -> Self {
        Self {
            rcon_password: Vec::new(),
            game_directory: b"base".to_vec(),
            net_ip: b"localhost".to_vec(),
            net_port: 29070,
            dedicated: 2,
        }
    }
}

impl<G: LegacyGameHost> LegacyServerSession<G> {
    /// Run one line at the server's own console (`Cbuf_ExecuteText` from the terminal):
    /// what it prints goes to `print`, one `Com_Printf` message per call. What it tells
    /// the clients leaves with the next [`Self::frame`].
    pub fn console(&mut self, line: &[u8], clock: LegacyClock, print: &mut dyn FnMut(&[u8])) {
        let Self {
            game,
            settings,
            slots,
            challenge,
            heartbeat_due,
            lifecycle,
            output,
            server_bit,
            bans,
            auto_demo,
            whitelist,
            ..
        } = self;
        let mut view = View {
            slots,
            game,
            challenge,
            settings,
            clock,
            heartbeat_due,
            lifecycle,
            server_bit,
            bans,
            auto_demo,
            whitelist,
        };
        execute_legacy_console(&mut view, line, print);
        view.tell(output);
    }
}

impl<G: LegacyGameHost> LegacyConsoleHost for View<'_, G> {
    fn client_count(&self) -> usize {
        self.slots.len()
    }
    fn phase(&self, client: usize) -> LegacyClientPhase {
        self.slots[client].phase
    }
    fn address(&self, client: usize) -> LegacyPeerAddress {
        // A slot that was never used holds nobody; the commands never look at it.
        self.slots[client]
            .peer
            .address
            .unwrap_or(LegacyPeerAddress::Loopback)
    }
    fn name(&self, client: usize) -> &[u8] {
        &self.slots[client].name
    }
    fn userinfo(&self, client: usize) -> &[u8] {
        self.slots[client].userinfo.as_bytes()
    }
    fn rate(&self, client: usize) -> i32 {
        self.slots[client].rate
    }
    fn ping(&self, client: usize) -> i32 {
        self.slots[client].ping
    }
    fn score(&self, client: usize) -> i32 {
        self.game.client_score(client)
    }
    fn status(&self) -> LegacyConsoleStatus<'_> {
        let info = self.game.server_info();
        let console = &self.settings.console;
        LegacyConsoleStatus {
            hostname: info.hostname,
            game_directory: &console.game_directory,
            net_ip: &console.net_ip,
            net_port: console.net_port,
            dedicated: console.dedicated,
            mapname: info.mapname,
            gametype: info.gametype,
            private_clients: self.settings.private_clients,
            // `Sys_Milliseconds` counts from the server's start, as `svs.startTime` does.
            uptime_seconds: i64::from(self.clock.wall_time / 1000),
        }
    }
    fn server_info(&self) -> &[u8] {
        self.game.status_info()
    }
    fn kick(&mut self, client: usize, reason: &[u8]) {
        self.drop(client, reason, None);
        // "in case there is a funny zombie": its zombie time runs from the kick.
        self.slots[client].peer.last_packet_time = self.clock.server_time;
    }
    fn server_command(&mut self, client: Option<usize>, text: &[u8]) {
        self.command(client, text);
    }
    fn game_command(&mut self, line: &[u8], print: &mut dyn FnMut(&[u8])) -> bool {
        let mut bots = super::bots::BotSlots {
            slots: &mut *self.slots,
            ring: self.settings.snapshot_entity_ring,
        };
        self.game
            .console_command(line, self.clock.server_time, &mut bots, print)
    }
    fn log(&mut self, text: &[u8]) {
        self.game.console_log(text);
    }
    fn bans(&mut self) -> &mut crate::LegacyBanList {
        self.bans
    }
    fn ban_file(&mut self) -> Option<Vec<u8>> {
        self.game.ban_file()
    }
    fn save_ban_file(&mut self, text: &[u8]) {
        self.game.save_ban_file(text);
    }
    fn resolve(&mut self, name: &str) -> Option<std::net::Ipv4Addr> {
        self.game.resolve_host(name)
    }
    fn demo_recording(&self, client: usize) -> bool {
        self.slots[client].demo.recording.is_some()
    }
    fn start_demo(&mut self, client: usize, name: &[u8], path: &str) -> bool {
        super::demo::flush_demo(&mut self.slots[client], client, &mut *self.game);
        if !self.game.demo_open(client, path) {
            return false;
        }
        self.slots[client].start_demo(client, name, &*self.game);
        true
    }
    fn stop_demo(&mut self, client: usize) {
        self.slots[client].stop_demo();
    }
    fn file_exists(&mut self, path: &str) -> bool {
        self.game.file_exists(path)
    }
    fn timestamp(&self) -> String {
        self.game.timestamp()
    }
    fn heartbeat(&mut self) {
        *self.heartbeat_due = true;
    }
    fn quit(&mut self) {
        self.lifecycle.quit_due = true;
    }
    fn kill_server(&mut self) {
        self.lifecycle.kill_due = true;
    }
    fn running(&self) -> bool {
        self.lifecycle.running
    }
    fn whitelist(&mut self, address: std::net::Ipv4Addr, print: &mut dyn FnMut(&[u8])) {
        super::whitelist::whitelist_address(self.whitelist, &mut *self.game, address, print);
    }
}
