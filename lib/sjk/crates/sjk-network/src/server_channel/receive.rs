use super::ServerChannelError;
use crate::{FRAGMENT_BIT, FRAGMENT_SIZE, OOB_PREFIX};
use sjk_protocol::MAX_LEGACY_MESSAGE_BYTES;

pub(super) struct Receiver {
    pub incoming: i32,
    pub dropped: i32,
    pub fragment_sequence: i32,
    pub fragments: Vec<u8>,
    pub message: Vec<u8>,
}

pub(super) fn qport(packet: &[u8]) -> Result<u16, ServerChannelError> {
    if packet.len() < 6
        || packet.len() > MAX_LEGACY_MESSAGE_BYTES
        || packet.starts_with(&OOB_PREFIX)
    {
        return Err(ServerChannelError::MalformedPacket);
    }
    Ok(u16::from_le_bytes([packet[4], packet[5]]))
}

impl Receiver {
    pub fn new() -> Self {
        Self {
            incoming: 0,
            dropped: 0,
            fragment_sequence: 0,
            fragments: Vec::with_capacity(MAX_LEGACY_MESSAGE_BYTES),
            message: Vec::with_capacity(MAX_LEGACY_MESSAGE_BYTES),
        }
    }

    pub fn process(&mut self, packet: &[u8]) -> Result<Option<(i32, u16)>, ServerChannelError> {
        let qport = qport(packet)?;
        let wire = u32::from_le_bytes(packet[..4].try_into().unwrap());
        let sequence = (wire & !FRAGMENT_BIT) as i32;
        let fragmented = wire & FRAGMENT_BIT != 0;
        if fragmented && packet.len() < 10 {
            return Err(ServerChannelError::MalformedPacket);
        }
        if sequence <= self.incoming {
            return Ok(None);
        }
        self.dropped = sequence - (self.incoming + 1);
        if !fragmented {
            self.message.clear();
            self.message.extend_from_slice(&packet[6..]);
            self.incoming = sequence;
            return Ok(Some((sequence, qport)));
        }
        let start = u16::from_le_bytes([packet[6], packet[7]]) as usize;
        let length = u16::from_le_bytes([packet[8], packet[9]]) as usize;
        if self.fragment_sequence != sequence {
            self.fragment_sequence = sequence;
            self.fragments.clear();
        }
        if start != self.fragments.len() {
            return Ok(None);
        }
        if 10 + length > packet.len() || self.fragments.len() + length > MAX_LEGACY_MESSAGE_BYTES {
            return Err(ServerChannelError::MalformedPacket);
        }
        self.fragments.extend_from_slice(&packet[10..10 + length]);
        if length == FRAGMENT_SIZE {
            return Ok(None);
        }
        // Stock reassembly reserves four bytes for the reconstructed sequence.
        if self.fragments.len() + 4 > MAX_LEGACY_MESSAGE_BYTES {
            return Err(ServerChannelError::MessageTooLarge);
        }
        self.message.clear();
        self.message.extend_from_slice(&self.fragments);
        self.fragments.clear();
        // Deliberately match codemp: the fragmented success branch does not
        // assign incomingSequence. Session command sequencing remains mandatory.
        Ok(Some((sequence, qport)))
    }
}
