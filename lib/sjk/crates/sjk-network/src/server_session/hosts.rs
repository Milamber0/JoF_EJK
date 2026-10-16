//! The session seen through the host traits of the adapter's coordinators.
use super::{LegacyClock, LegacyGameHost, LegacySessionSettings, Slot, schedule::UserinfoUpdate};
use crate::{
    ClientCommandPolicy, LegacyChallenge, LegacyClientPhase, LegacyDropEffect, LegacyDropHost,
    LegacyInfoString, LegacyMessageContext, LegacyMessageEvent, LegacyMessageHost,
    LegacyMoveAdmission, LegacyMoveEvent, LegacyPeer, LegacyPeerAddress, LegacyReliableState,
    LegacyRosterHost, LegacyTokens, drop_legacy_client,
};
use std::net::SocketAddrV4;

/// Everything a coordinator may touch while one datagram or frame is handled.
pub(super) struct View<'a, G> {
    pub slots: &'a mut [Slot],
    pub game: &'a mut G,
    pub challenge: &'a LegacyChallenge,
    pub settings: &'a mut LegacySessionSettings,
    pub clock: LegacyClock,
    pub heartbeat_due: &'a mut bool,
    /// Whether the server runs, and whether `quit` or `killserver` ran.
    pub lifecycle: &'a mut super::shutdown::Lifecycle,
    /// `svs.snapFlagServerBit`.
    pub server_bit: &'a mut u8,
    /// The ban list connections are admitted by.
    pub bans: &'a mut crate::LegacyBanList,
    /// The level's automatic demos.
    pub auto_demo: &'a mut super::auto_demo::AutoDemoLevel,
    /// The denial-of-service whitelist.
    pub whitelist: &'a mut crate::LegacyWhitelist,
}

impl<G: LegacyGameHost> View<'_, G> {
    /// Run the drop lifecycle. `executing` is the client whose packet is being
    /// handled right now: its reliable history is out of its slot, so the final
    /// messages of its own drop must be written to the history passed along.
    pub fn drop(
        &mut self,
        client: usize,
        reason: &[u8],
        executing: Option<(usize, &mut LegacyReliableState)>,
    ) {
        let mut drop = DropView {
            view: self,
            executing,
        };
        // The only failure left is an exhausted 31-bit command counter, after
        // local cleanup already ran; nothing further can be sent to that peer.
        let _ = drop_legacy_client(&mut drop, client, reason);
    }
}

impl<G: LegacyGameHost> LegacyRosterHost for View<'_, G> {
    fn client_count(&self) -> usize {
        self.slots.len()
    }
    fn phase(&self, slot: usize) -> LegacyClientPhase {
        self.slots[slot].phase
    }
    fn set_phase(&mut self, slot: usize, phase: LegacyClientPhase) {
        self.slots[slot].phase = phase;
    }
    fn peer(&self, slot: usize) -> &LegacyPeer {
        &self.slots[slot].peer
    }
    fn peer_mut(&mut self, slot: usize) -> &mut LegacyPeer {
        &mut self.slots[slot].peer
    }
    fn verify_challenge(&mut self, challenge: i32, from: SocketAddrV4) -> bool {
        self.challenge
            .verify(challenge, from, self.clock.server_time)
    }
    fn game_disconnect(&mut self, slot: usize) {
        self.game.client_disconnect(slot);
    }
    fn reset_session(&mut self, slot: usize, challenge: i32) {
        let slot = &mut self.slots[slot];
        slot.wire.reset(challenge);
        (slot.gamestate_due, slot.pure_authentic, slot.got_cp) = (false, false, false);
        slot.marks.clear();
        slot.name.clear();
        slot.userinfo.clear();
        // A new `client_t` is zeroed: nothing sent, nothing acknowledged.
        slot.timings = Default::default();
    }
    fn game_connect(&mut self, slot: usize, userinfo: &[u8]) -> Result<(), Vec<u8>> {
        // The game already sees the server's `ip` key (`SV_DirectConnect`); admission
        // has checked that it fits.
        let ip = match self.slots[slot].peer.address {
            Some(LegacyPeerAddress::Ip(address)) => address.to_string(),
            _ => "localhost".to_owned(),
        };
        let info = &mut self.slots[slot].userinfo;
        *info = LegacyInfoString::from_truncated(userinfo);
        info.set(b"ip", ip.as_bytes());
        self.game
            .client_connect(slot, self.slots[slot].userinfo.as_bytes())
    }
    fn userinfo_changed(&mut self, slot: usize) {
        if let Err(reason) = self.slots[slot].userinfo_changed(&self.settings.rates) {
            self.drop(slot, reason, None);
        }
    }
    fn heartbeat(&mut self) {
        *self.heartbeat_due = true;
    }
    fn drop_client(&mut self, slot: usize, reason: &[u8]) {
        self.drop(slot, reason, None);
    }
}

pub(super) struct DropView<'v, 'a, 'r, G> {
    pub view: &'v mut View<'a, G>,
    pub executing: Option<(usize, &'r mut LegacyReliableState)>,
}

impl<G: LegacyGameHost> LegacyDropHost for DropView<'_, '_, '_, G> {
    fn client_count(&self) -> usize {
        self.view.slots.len()
    }
    fn phase(&self, client: usize) -> LegacyClientPhase {
        self.view.slots[client].phase
    }
    fn set_phase(&mut self, client: usize, phase: LegacyClientPhase) {
        self.view.slots[client].phase = phase;
    }
    fn reliable(&mut self, client: usize) -> &mut LegacyReliableState {
        match &mut self.executing {
            Some((executing, reliable)) if *executing == client => reliable,
            _ => &mut self.view.slots[client].wire.reliable,
        }
    }
    fn name(&self, client: usize) -> &[u8] {
        &self.view.slots[client].name
    }
    fn is_bot(&self, client: usize) -> bool {
        self.view.slots[client].peer.address == Some(LegacyPeerAddress::Bot)
    }
    fn is_recording(&self, client: usize) -> bool {
        self.view.slots[client].demo.recording.is_some()
    }
    fn effect(&mut self, effect: LegacyDropEffect) {
        match effect {
            LegacyDropEffect::GameDisconnect(client) => self.view.game.client_disconnect(client),
            LegacyDropEffect::FreeBot(client) => self.view.slots[client].name.clear(),
            LegacyDropEffect::ClearUserinfo(client) => {
                self.view.slots[client].userinfo.clear();
                self.view.slots[client].name.clear();
            }
            LegacyDropEffect::Heartbeat => *self.view.heartbeat_due = true,
            LegacyDropEffect::StopDemo(client) => {
                self.view.slots[client].stop_demo();
                self.view
                    .game
                    .console_log(format!("Stopped demo for client {client}.\n").as_bytes());
            }
            LegacyDropEffect::CloseDownload(client) => self.view.slots[client].download.close(),
        }
    }
}

/// The session while `client`'s packet executes.
pub(super) struct Executing<'v, 'a, G> {
    pub view: &'v mut View<'a, G>,
    pub client: usize,
}

impl<G: LegacyGameHost> LegacyMessageHost for Executing<'_, '_, G> {
    fn context(&self) -> LegacyMessageContext {
        let (slot, settings) = (&self.view.slots[self.client], &*self.view.settings);
        let (server_id, restarted_server_id) = self.view.game.server_ids();
        LegacyMessageContext {
            server_id,
            restarted_server_id,
            gamestate_message: slot.peer.gamestate_message,
            // The clock the last world stopped at, which this client carries across a
            // map change; zero for a client that has never ridden one. Pure servers do
            // not exist yet.
            old_server_time: slot.old_server_time,
            downloading: slot.download.active(),
            admission: LegacyMoveAdmission {
                phase: slot.phase,
                pure_required: false,
                pure_authentic: slot.pure_authentic,
                got_cp: slot.got_cp,
            },
            command_policy: ClientCommandPolicy {
                now: self.view.clock.server_time,
                active: false,
                local_client_running: false,
                flood_protect: settings.flood_protect,
                slow: false,
            },
            checksum_feed: self.view.game.checksum_feed(),
            legacy_fixes: settings.legacy_fixes,
        }
    }

    fn client_command(
        &mut self,
        reliable: &mut LegacyReliableState,
        text: &[u8],
        game_allowed: bool,
    ) {
        // `SV_ExecuteClientCommand`: engine commands first, the game otherwise.
        // cp and vdr are not implemented yet and are swallowed here as the reference
        // keeps them from the game.
        let mut tokens = LegacyTokens::new(text);
        let (command, client) = (tokens.next().unwrap_or_default(), self.client);
        if command.eq_ignore_ascii_case(b"disconnect") {
            self.view
                .drop(client, b"@@@DISCONNECTED", Some((client, reliable)));
        } else if command.eq_ignore_ascii_case(b"userinfo") {
            let (argument, now) = (
                tokens.next().unwrap_or_default(),
                self.view.clock.server_time,
            );
            match self.view.slots[client].update_userinfo(argument, now, &self.view.settings.rates)
            {
                UserinfoUpdate::Changed => {
                    let View { slots, game, .. } = &mut *self.view;
                    game.client_userinfo_changed(client, slots[client].userinfo.as_bytes());
                }
                UserinfoUpdate::TooMany => {
                    let primed = self.view.slots[client].phase >= LegacyClientPhase::Primed;
                    // A full ring here ends in the ordinary overflow drop on the next send.
                    let _ =
                        reliable.queue_server_command(primed, b"print \"@@@TOO_MANY_INFO\n\"\n");
                }
                UserinfoUpdate::Drop(reason) => {
                    self.view.drop(client, reason, Some((client, reliable)))
                }
                UserinfoUpdate::Ignored => {}
            }
        } else if command.eq_ignore_ascii_case(b"download") {
            self.view.slots[client].begin_download(tokens.next().unwrap_or_default());
        } else if command.eq_ignore_ascii_case(b"nextdl") {
            let (block, now) = (
                crate::server_bans::atoi_text(tokens.next().unwrap_or_default()),
                self.view.clock.server_time,
            );
            let View { slots, game, .. } = &mut *self.view;
            let broken = slots[client].next_download(block, client, now).is_err();
            slots[client].flush_download_log(|text| game.console_log(text));
            if broken {
                // "the client will never parse the disconnect message because the cgame
                // isn't loaded yet".
                self.view
                    .drop(client, b"broken download", Some((client, reliable)));
            }
        } else if command.eq_ignore_ascii_case(b"stopdl") {
            self.view.slots[client].stop_download();
        } else if command.eq_ignore_ascii_case(b"donedl") {
            // "resend the game state to update any clients that entered during the
            // download".
            if self.view.slots[client].phase != LegacyClientPhase::Active {
                self.view.slots[client].gamestate_due = true;
            }
        } else if [&b"cp"[..], b"vdr"]
            .iter()
            .any(|engine| command.eq_ignore_ascii_case(engine))
        {
        } else if game_allowed {
            self.view
                .game
                .client_command(self.client, text, self.view.clock.server_time);
        }
    }

    fn message_event(&mut self, reliable: &mut LegacyReliableState, event: LegacyMessageEvent) {
        let client = self.client;
        match event {
            // The client acknowledged the new world's serverId, so its clock has
            // caught up and the old world's time is no longer owed to it
            // (`sv_client.cpp:1465-1512` reads it until exactly this point).
            LegacyMessageEvent::ClearOldServerTime => {
                self.view.slots[self.client].old_server_time = 0
            }
            LegacyMessageEvent::DropLostReliableCommands => {
                self.view
                    .drop(client, b"Lost reliable commands", Some((client, reliable)));
            }
            LegacyMessageEvent::Movement(LegacyMoveEvent::DropUnpure) => {
                self.view.drop(
                    client,
                    b"Cannot validate pure client!",
                    Some((client, reliable)),
                );
            }
            LegacyMessageEvent::Movement(LegacyMoveEvent::EnterWorld(command)) => {
                // `SV_ClientEnterWorld`: active, told what changed while it was primed,
                // and only then handed to the game.
                self.view.slots[client].phase = LegacyClientPhase::Active;
                // `sv_autoWhitelist`: listed as soon as it is in the game.
                if self.view.settings.auto_whitelist
                    && let Some(LegacyPeerAddress::Ip(address)) =
                        self.view.slots[client].peer.address
                {
                    let mut printed = Vec::new();
                    let View {
                        whitelist, game, ..
                    } = &mut *self.view;
                    super::whitelist::whitelist_address(
                        whitelist,
                        &mut **game,
                        *address.ip(),
                        &mut |text| printed.extend_from_slice(text),
                    );
                    if !printed.is_empty() {
                        self.view.game.console_log(&printed);
                    }
                }
                if super::output::catch_up(self.view, client, reliable) {
                    self.view
                        .drop(client, b"Server command overflow", Some((client, reliable)));
                } else {
                    self.view
                        .game
                        .enter_world(client, &command, self.view.clock.server_time);
                    super::auto_demo::begin(self.view);
                }
            }
            LegacyMessageEvent::Movement(LegacyMoveEvent::Think(command)) => {
                self.view
                    .game
                    .client_think(client, &command, self.view.clock.server_time);
            }
            // The executing packet still borrows the channel, so the gamestate is
            // queued as soon as the packet is done; nothing else runs in between,
            // because the reference stops reading a packet after this request too.
            LegacyMessageEvent::Movement(LegacyMoveEvent::SendGamestate) => {
                self.view.slots[client].gamestate_due = true;
            }
            // `SV_UserMove`: "save time for ping calculation".
            LegacyMessageEvent::Movement(LegacyMoveEvent::Acknowledge { message, at }) => {
                self.view.slots[client].timings
                    [(message & (super::ping::MESSAGE_BACKUP as i32 - 1)) as usize]
                    .acked = at;
            }
        }
    }
}
