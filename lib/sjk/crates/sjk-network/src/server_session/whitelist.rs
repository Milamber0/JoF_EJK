//! The session's side of the whitelist: the file read at startup, players listed as they
//! enter the world (`sv_autoWhitelist`), and the global rate doubled for listed senders.
use super::{LegacyGameHost, LegacyServerSession};
use crate::LegacyWhitelist;
use std::net::Ipv4Addr;

/// `SVC_WhitelistAdr` with the host's file.
pub(super) fn whitelist_address(
    whitelist: &mut LegacyWhitelist,
    game: &mut impl LegacyGameHost,
    address: Ipv4Addr,
    print: &mut dyn FnMut(&[u8]),
) {
    whitelist.whitelist(address, |record| game.append_whitelist(record), print);
}

impl<G: LegacyGameHost> LegacyServerSession<G> {
    /// `SVC_LoadWhitelist`: the list the host's file holds, added to this one. The
    /// reference reads it once, when the server starts.
    pub fn load_whitelist(&mut self) {
        if let Some(file) = self.game.whitelist_file() {
            self.whitelist.load(&file);
        }
    }

    /// `SVC_WhitelistAdr` from outside a packet or command (a master server resolved):
    /// what goes wrong is told on the server's console.
    pub fn whitelist_address(&mut self, address: Ipv4Addr) {
        let mut printed = Vec::new();
        whitelist_address(&mut self.whitelist, &mut self.game, address, &mut |text| {
            printed.extend_from_slice(text)
        });
        if !printed.is_empty() {
            self.game.console_log(&printed);
        }
    }

    /// The whitelist.
    pub fn whitelist(&self) -> &LegacyWhitelist {
        &self.whitelist
    }
}
