//! The server going down (`SV_Shutdown`, `sv_init.cpp:1047-1124`): after `quit`, every
//! client is told why and disconnected at once.
use super::{LegacyClock, LegacyGameHost, LegacyServerSession, hosts::View};
use crate::{LegacyClientPhase, LegacyPeerAddress};
use std::net::SocketAddrV4;

/// Whether a server is running, and what the operator asked of it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Lifecycle {
    /// `sv_running`: a level is loaded and clients are served.
    pub running: bool,
    /// `quit` ran and the server has not shut down yet.
    pub quit_due: bool,
    /// `killserver` ran and the server has not stopped yet.
    pub kill_due: bool,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            running: true,
            quit_due: false,
            kill_due: false,
        }
    }
}

impl<G: LegacyGameHost> LegacyServerSession<G> {
    /// Whether `quit` ran; the server is then to send [`Self::final_message`], stop its
    /// game and exit, running nothing else.
    pub fn quit_due(&self) -> bool {
        self.lifecycle.quit_due
    }

    /// Whether `killserver` ran; the server is then to send [`Self::final_message`], stop
    /// its game and [`Self::stop`].
    pub fn kill_due(&self) -> bool {
        self.lifecycle.kill_due
    }

    /// Whether a server is running (`sv_running`). A stopped one answers no packet and
    /// runs no frame until a map starts one again ([`crate::LegacyGameOutput::MapChanged`]).
    pub fn running(&self) -> bool {
        self.lifecycle.running
    }

    /// The end of `SV_Shutdown` for `killserver`: every client gone, and the server
    /// stopped until the next map.
    pub fn stop(&mut self) {
        let ring = self.settings.snapshot_entity_ring;
        for slot in &mut self.slots {
            *slot = super::Slot::free(ring);
        }
        self.lifecycle = Lifecycle {
            running: false,
            quit_due: false,
            kill_due: false,
        };
    }

    /// `SV_FinalMessage`: twice over, every client from connected up (but a local one)
    /// is sent `print "<message>"` and `disconnect`, and every one a snapshot at once,
    /// whatever its rate.
    pub fn final_message(
        &mut self,
        message: &[u8],
        clock: LegacyClock,
        send: &mut impl FnMut(SocketAddrV4, &[u8]),
    ) {
        let Self {
            game,
            settings,
            slots,
            challenge,
            heartbeat_due,
            lifecycle,
            message: writer,
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
        let print = [&b"print \""[..], message, b"\""].concat();
        let downloads = super::download::DownloadPolicy {
            allow: view.settings.allow_download,
            pure: view.settings.pure,
        };
        for _ in 0..2 {
            for client in 0..view.slots.len() {
                let slot = &view.slots[client];
                if !matches!(
                    slot.phase,
                    LegacyClientPhase::Connected
                        | LegacyClientPhase::Primed
                        | LegacyClientPhase::Active
                ) {
                    continue;
                }
                if slot.peer.address != Some(LegacyPeerAddress::Loopback) {
                    view.command(Some(client), &print);
                    view.command(Some(client), b"disconnect");
                }
                let slot = &mut view.slots[client];
                slot.send_now(
                    client,
                    &*view.game,
                    clock.server_time,
                    *view.server_bit,
                    &mut view.settings.rates,
                    downloads,
                    writer,
                    send,
                );
            }
        }
        super::demo::flush_demos(view.slots, view.game);
    }
}
