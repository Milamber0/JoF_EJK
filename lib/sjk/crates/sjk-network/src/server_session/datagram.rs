//! Datagrams in, datagrams out: `SV_PacketEvent` and the per-frame housekeeping.
use super::{
    LegacyClock, LegacyGameHost, LegacyServerSession, Wire,
    hosts::{Executing, View},
    transmit,
};
use crate::{
    LEGACY_CONNECT_RESPONSE, LEGACY_UNKNOWN_PEER_REPLY, LegacyConnectAttempt, LegacyConnectOutcome,
    LegacyConnectPolicy, LegacyOobAdmission, LegacyOobRequest, LegacyPeerAddress, LegacyRoute,
    LegacyTimeoutPolicy, accept_legacy_packet, check_legacy_timeouts, connect_legacy_client,
    execute_legacy_client_packet, parse_legacy_oob, route_legacy_datagram,
    write_legacy_challenge_response, write_legacy_info_response, write_legacy_status_response,
};
use std::net::SocketAddrV4;

impl<G: LegacyGameHost> LegacyServerSession<G> {
    /// Handle one received datagram, sending any replies through `send`.
    ///
    /// Malformed input is dropped without a reply. Nothing is sent to anyone but
    /// `from` except the reliable output a drop queues for other clients, which
    /// leaves with the next [`Self::frame`].
    pub fn handle_datagram(
        &mut self,
        from: SocketAddrV4,
        datagram: &[u8],
        clock: LegacyClock,
        send: &mut impl FnMut(SocketAddrV4, &[u8]),
    ) {
        // `SV_PacketEvent`: a server that is not running hears nothing.
        if !self.lifecycle.running {
            return;
        }
        let Self {
            game,
            settings,
            slots,
            spare,
            challenge,
            limiter,
            line,
            reply,
            redirect,
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
        let address = LegacyPeerAddress::Ip(from);
        let client = match route_legacy_datagram(&mut view, address, datagram) {
            LegacyRoute::Peer { slot, .. } => slot,
            LegacyRoute::Unknown => return send(from, LEGACY_UNKNOWN_PEER_REPLY),
            LegacyRoute::Connectionless => {
                // Whitelisting of addresses that have played belongs to a later step.
                let admission = limiter.admit(
                    address,
                    view.whitelist.contains(address),
                    view.settings.oob_rates,
                    clock.wall_time,
                );
                if admission != LegacyOobAdmission::Accepted {
                    return;
                }
                reply.clear();
                match parse_legacy_oob(datagram, line) {
                    LegacyOobRequest::Info { challenge } => {
                        write_legacy_info_response(reply, challenge, &view.game.server_info());
                    }
                    LegacyOobRequest::Status { challenge } => {
                        let (info, players) = (view.game.status_info(), view.game.status_players());
                        write_legacy_status_response(reply, challenge, info, players);
                    }
                    LegacyOobRequest::Challenge { client_challenge } => {
                        let issued = view.challenge.issue(from, clock.server_time);
                        write_legacy_challenge_response(reply, issued, client_challenge);
                    }
                    LegacyOobRequest::Connect(request) => {
                        let (reconnect_limit_seconds, private_clients) = (
                            view.settings.reconnect_limit_seconds,
                            view.settings.private_clients,
                        );
                        let private_password = view.settings.private_password.clone();
                        // `SV_IsBanned`, before anything else of the connect is read.
                        let banned = view.bans.is_banned(from);
                        let outcome = connect_legacy_client(
                            &mut view,
                            LegacyConnectAttempt {
                                from: address,
                                request: &request,
                                banned,
                            },
                            LegacyConnectPolicy {
                                now: clock.server_time,
                                reconnect_limit_seconds,
                                private_clients,
                                private_password: &private_password,
                            },
                        );
                        match outcome {
                            LegacyConnectOutcome::Accepted { .. } => {
                                reply.extend_from_slice(LEGACY_CONNECT_RESPONSE)
                            }
                            LegacyConnectOutcome::Refused(refusal) => refusal.write_reply(reply),
                            LegacyConnectOutcome::LocalServerFull => {}
                        }
                    }
                    LegacyOobRequest::Rcon { line } => {
                        // The password is lent out of the settings the view also reaches.
                        let password = std::mem::take(&mut view.settings.console.rcon_password);
                        crate::run_legacy_rcon(
                            &mut view,
                            line,
                            &password,
                            from,
                            redirect,
                            &mut |datagram| send(from, datagram),
                        );
                        view.settings.console.rcon_password = password;
                    }
                    LegacyOobRequest::MalformedConnect(_) | LegacyOobRequest::Ignored => {}
                }
                if !reply.is_empty() {
                    send(from, reply);
                }
                // A connect may have made the game speak: a join print, a new player's string.
                view.tell(output);
                return;
            }
        };
        // The client's transport leaves its slot while its packet runs, so that the
        // packet can borrow it and a drop can still reach every slot of the roster.
        std::mem::swap(&mut view.slots[client].wire, spare);
        let Wire {
            channel,
            reliable,
            movement,
            ..
        } = spare;
        if let Ok(Some(packet)) = channel.receive(datagram)
            && accept_legacy_packet(&mut view, client, clock.server_time)
        {
            let mut executing = Executing {
                view: &mut view,
                client,
            };
            // A packet that fails to parse has no further effect; what it already
            // executed stands, as in the reference.
            let _ = execute_legacy_client_packet(packet, reliable, movement, &mut executing);
        }
        std::mem::swap(&mut view.slots[client].wire, spare);
        // What the game said while the packet ran, now that every client's history is
        // back in its slot.
        view.tell(output);
        // A requested gamestate leaves at once, as `SV_SendClientGameState` sends it:
        // fragments still under way are flushed first, then the scheduler takes over.
        let slot = &mut view.slots[client];
        if slot.gamestate_due {
            let mut datagram = [0; transmit::DATAGRAM_BYTES];
            while let Ok(Some(length)) = slot.wire.channel.next_datagram(&mut datagram) {
                send(from, &datagram[..length]);
            }
            // One that cannot be built leaves the client waiting; it asks again with
            // its next packet, and a drop is the map loader's decision.
            if let Ok((bytes, _)) = slot.queue_gamestate(client, &*view.game, clock.server_time) {
                slot.schedule_message(clock.server_time, bytes, &mut view.settings.rates);
                if let Ok(Some(length)) = slot.wire.channel.next_datagram(&mut datagram) {
                    send(from, &datagram[..length]);
                }
            }
        }
    }

    /// Per-frame housekeeping: timeouts, then pending reliable output.
    pub fn frame(&mut self, clock: LegacyClock, send: &mut impl FnMut(SocketAddrV4, &[u8])) {
        if !self.lifecycle.running {
            return;
        }
        let Self {
            game,
            settings,
            slots,
            challenge,
            heartbeat_due,
            lifecycle,
            message,
            output,
            server_bit,
            bans,
            auto_demo,
            whitelist,
            ..
        } = self;
        let (timeout_seconds, zombie_seconds) = (settings.timeout_seconds, settings.zombie_seconds);
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
        // `SV_BotFrame`, before the timeouts.
        super::bots::acknowledge_bot_commands(view.slots, clock.server_time);
        check_legacy_timeouts(
            &mut view,
            LegacyTimeoutPolicy {
                now: clock.server_time,
                timeout_seconds,
                zombie_seconds,
            },
        );
        // Timeouts make the game speak too, and so does its own frame between calls.
        view.tell(output);
        if !view.auto_demo.spawned {
            view.auto_demo.spawned = true;
            super::auto_demo::begin(&mut view);
        }
        let downloads = super::download::DownloadPolicy {
            allow: view.settings.allow_download,
            pure: view.settings.pure,
        };
        for client in 0..view.slots.len() {
            // `SV_SendClientSnapshot`: a client about to be sent a snapshot and not
            // recorded starts every automatic demo that is due.
            let slot = &view.slots[client];
            if view.settings.auto_demo.enabled
                && slot.demo.recording.is_none()
                && slot.snapshot_due(clock.server_time)
            {
                super::auto_demo::begin(&mut view);
            }
            let slot = &mut view.slots[client];
            slot.transmit(
                client,
                &*view.game,
                clock.server_time,
                *view.server_bit,
                &mut view.settings.rates,
                downloads,
                message,
                send,
            );
            slot.flush_download_log(|text| view.game.console_log(text));
        }
        super::demo::flush_demos(view.slots, view.game);
    }
}
