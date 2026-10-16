//! Server-side demos (`sv_ccmds.cpp:1527-1705`, `sv_snapshot.cpp:151-179, 710-715`): a
//! client's messages, as they are sent, copied into a demo file.
//!
//! The file is the client demo format: for each message its sequence and length (little
//! endian) and its bytes before the channel encodes them, starting with a gamestate
//! written for the demo alone and ended by `-1 -1`. A recording waits for a message
//! whose snapshot has no base; from then on no snapshot is sent against a frame older
//! than that one, so that every delta in the file has its base in the file.
use super::{LegacyGameHost, Slot};
use sjk_protocol::{
    MAX_LEGACY_MESSAGE_BYTES, MessageWriter, ServiceCommand, write_gamestate_block,
};

/// A slot's demo state (`client->demo`).
#[derive(Clone, Debug, Default)]
pub(super) struct DemoState {
    /// The demo's name while one is recorded.
    pub recording: Option<Vec<u8>>,
    /// `demowaiting`: no message is saved until one's snapshot has no base.
    pub waiting: bool,
    /// `minDeltaFrame`: the oldest message a snapshot may be a delta from.
    pub min_delta_frame: i32,
    /// Bytes for the file, handed to the host after the datagram or frame.
    pub pending: Vec<u8>,
    /// The file ends after `pending`.
    pub closing: bool,
}

impl DemoState {
    /// Whether the snapshot of message `delta_message`'s request may not be a delta.
    pub fn forbids_delta(&self, delta_message: i32) -> bool {
        (self.recording.is_some() && self.waiting) || self.min_delta_frame > delta_message
    }

    /// After a snapshot of message `sequence`: one without a base ends the wait and is
    /// the oldest base from now on.
    pub fn snapshot_written(&mut self, sequence: i32, delta: bool) {
        if !delta {
            if self.waiting {
                self.min_delta_frame = sequence;
            }
            self.waiting = false;
        }
    }

    /// `SV_SendMessageToClient`'s copy into a recording demo that is not waiting.
    pub fn message_sent(&mut self, sequence: i32, message: &[u8]) {
        if self.recording.is_some() && !self.waiting {
            self.append(sequence, message);
        }
    }

    fn append(&mut self, sequence: i32, message: &[u8]) {
        self.pending.extend_from_slice(&sequence.to_le_bytes());
        self.pending
            .extend_from_slice(&(message.len() as i32).to_le_bytes());
        self.pending.extend_from_slice(message);
    }
}

impl Slot {
    /// `SV_RecordDemo` once the file is open: the demo waits for a whole snapshot, and
    /// starts with the gamestate this client would be sent now, numbered one before its
    /// next message. Nothing of the client's own state changes.
    pub(super) fn start_demo(&mut self, client: usize, name: &[u8], game: &impl LegacyGameHost) {
        let wire = &mut self.wire;
        let mut message = MessageWriter::new(MAX_LEGACY_MESSAGE_BYTES);
        let mut written = message
            .write_i32(wire.reliable.counters().client_sequence)
            .is_ok()
            && wire.reliable.write_pending_unsent(&mut message).is_ok();
        written = written
            && write_gamestate_block(
                &mut message,
                wire.reliable.counters().server_sequence,
                game.config_strings(),
                game.baselines(),
                client as i32,
                game.checksum_feed(),
            )
            .is_ok()
            && message.write_u8(ServiceCommand::End as u8).is_ok();
        let sequence = wire.channel.outgoing_sequence().wrapping_sub(1);
        self.demo.recording = Some(name.to_vec());
        self.demo.waiting = true;
        match message.finish() {
            Ok(payload) if written => self.demo.append(sequence, &payload),
            // A gamestate this client could not be sent either: the demo has none.
            _ => {}
        }
    }

    /// `SV_StopRecordDemo`'s end of the file.
    pub(super) fn stop_demo(&mut self) {
        self.demo.pending.extend_from_slice(&(-1_i32).to_le_bytes());
        self.demo.pending.extend_from_slice(&(-1_i32).to_le_bytes());
        self.demo.recording = None;
        self.demo.closing = true;
    }
}

/// Hand every slot's demo bytes to the host, and close the files that ended.
pub(super) fn flush_demos(slots: &mut [Slot], game: &mut impl LegacyGameHost) {
    for (client, slot) in slots.iter_mut().enumerate() {
        flush_demo(slot, client, game);
    }
}

/// Hand one slot's demo bytes to the host, and close its file if it ended. A new demo's
/// file must not open before the old one's end is written.
pub(super) fn flush_demo(slot: &mut Slot, client: usize, game: &mut impl LegacyGameHost) {
    if !slot.demo.pending.is_empty() {
        game.demo_data(client, &slot.demo.pending);
        slot.demo.pending.clear();
    }
    if std::mem::take(&mut slot.demo.closing) {
        game.demo_close(client);
    }
}
