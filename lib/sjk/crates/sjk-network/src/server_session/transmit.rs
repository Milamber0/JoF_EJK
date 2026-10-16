//! Outgoing messages: every one acknowledges the client's commands, repeats the
//! server commands it has not acknowledged and carries a snapshot, as
//! `SV_SendClientSnapshot` builds them, and leaves when `SV_SendClientMessages`
//! would send it.
use super::{
    LegacyGameHost, LegacyRateSettings, LegacySnapshotFrame, Slot, download::DownloadPolicy,
    snapshot::SnapshotRequest,
};
use crate::{LegacyClientPhase, LegacyPeerAddress};
use sjk_protocol::{MessageWriter, ServiceCommand};
use std::net::SocketAddrV4;

/// Every datagram a server channel produces fits (step 333).
pub(super) const DATAGRAM_BYTES: usize = 1308;

impl Slot {
    /// One client's turn in a server frame: nothing before its next send time,
    /// then the next fragment of a message under way, else a new snapshot.
    ///
    /// Every client that is not free is served, zombies included: that is how a
    /// final `disconnect` and everything before world entry reaches them.
    pub(super) fn transmit(
        &mut self,
        client: usize,
        game: &impl LegacyGameHost,
        now: i32,
        server_bit: u8,
        rates: &mut LegacyRateSettings,
        downloads: DownloadPolicy,
        message: &mut MessageWriter,
        send: &mut impl FnMut(SocketAddrV4, &[u8]),
    ) {
        let Some(LegacyPeerAddress::Ip(to)) = self.peer.address else {
            return;
        };
        if !self.send_due(now) {
            return;
        }
        if self.wire.channel.has_pending_message() {
            self.schedule_fragment(now, rates);
        } else {
            // A message that does not fit is not sent; the next one is complete again.
            let Ok(bytes) =
                self.queue_snapshot(client, game, now, server_bit, (rates, downloads), message)
            else {
                return;
            };
            self.schedule_message(now, bytes, rates);
        }
        let mut datagram = [0; DATAGRAM_BYTES];
        if let Ok(Some(length)) = self.wire.channel.next_datagram(&mut datagram) {
            send(to, &datagram[..length]);
        }
    }

    /// `SV_SendClientSnapshot` at once, whatever the rate (`SV_FinalMessage` forces it):
    /// any message still in fragments is sent whole first, as `SV_SendMessageToClient`
    /// does, then a new one with every datagram it takes.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn send_now(
        &mut self,
        client: usize,
        game: &impl LegacyGameHost,
        now: i32,
        server_bit: u8,
        rates: &mut LegacyRateSettings,
        downloads: DownloadPolicy,
        message: &mut MessageWriter,
        send: &mut impl FnMut(SocketAddrV4, &[u8]),
    ) {
        let Some(LegacyPeerAddress::Ip(to)) = self.peer.address else {
            return;
        };
        let mut datagram = [0; DATAGRAM_BYTES];
        let mut drain = |slot: &mut Self| {
            while let Ok(Some(length)) = slot.wire.channel.next_datagram(&mut datagram) {
                send(to, &datagram[..length]);
            }
        };
        drain(self);
        if self
            .queue_snapshot(client, game, now, server_bit, (rates, downloads), message)
            .is_ok()
        {
            drain(self);
        }
    }

    /// Whether [`Self::transmit`] builds a new snapshot at `now`, rather than sending
    /// nothing or the next fragment of a message.
    pub(super) fn snapshot_due(&self, now: i32) -> bool {
        matches!(self.peer.address, Some(LegacyPeerAddress::Ip(_)))
            && self.send_due(now)
            && !self.wire.channel.has_pending_message()
    }

    /// Build and queue one snapshot message; the channel must be free. Returns the
    /// message's size as the rate calculation counts it, before the final `svc_EOF`.
    pub(super) fn queue_snapshot(
        &mut self,
        client: usize,
        game: &impl LegacyGameHost,
        server_time: i32,
        server_bit: u8,
        (rates, downloads): (&mut LegacyRateSettings, DownloadPolicy),
        message: &mut MessageWriter,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let wire = &mut self.wire;
        message.clear();
        message.write_i32(wire.reliable.counters().client_sequence)?;
        wire.reliable.write_pending(message)?;
        let (sequence, delta_message) = (
            wire.channel.outgoing_sequence(),
            wire.movement.delta_message(),
        );
        let delta = wire.history.write(
            message,
            SnapshotRequest {
                sequence: wire.channel.outgoing_sequence(),
                delta_message: wire.movement.delta_message(),
                server_time,
                server_bit,
                rate_delayed: self.rate_delayed,
                active: self.phase == LegacyClientPhase::Active,
                demo_forbids_delta: self.demo.forbids_delta(delta_message),
            },
            // A zombie's frame is not built (`SV_BuildClientSnapshot`); every other
            // client is shown the world, flagged not active until it has entered.
            (self.phase != LegacyClientPhase::Zombie).then_some(
                |frame: &mut LegacySnapshotFrame<'_>| {
                    game.build_snapshot(client, frame);
                },
            ),
            |number| game.baseline(number),
        )?;
        // "Add any download data if the client is downloading".
        self.write_download(client, message, server_time, downloads, rates, game)?;
        let bytes = (message.bit_position() >> 3) + 1;
        message.write_u8(ServiceCommand::End as u8)?;
        self.demo.snapshot_written(sequence, delta);
        super::ping::record_sent(&mut self.timings, sequence, server_time);
        let finished = message.finished()?;
        // "save the message to demo. this must happen before sending over network".
        self.demo.message_sent(sequence, finished);
        let wire = &mut self.wire;
        wire.channel
            .queue_message(finished, wire.reliable.last_client_command())?;
        Ok(bytes)
    }
}
