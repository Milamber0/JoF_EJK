use super::ServerChannelError;
use crate::{FRAGMENT_BIT, FRAGMENT_SIZE, xor_protocol_tail};
use sjk_protocol::MAX_LEGACY_MESSAGE_BYTES;

pub(super) struct Sender {
    pub sequence: i32,
    pub pending: bool,
    pub message: Vec<u8>,
    cursor: usize,
}

impl Sender {
    pub fn new() -> Self {
        Self {
            sequence: 1,
            pending: false,
            message: Vec::with_capacity(MAX_LEGACY_MESSAGE_BYTES),
            cursor: 0,
        }
    }

    pub fn queue(
        &mut self,
        payload: &[u8],
        challenge: i32,
        command: &[u8],
    ) -> Result<i32, ServerChannelError> {
        if self.pending {
            return Err(ServerChannelError::PendingMessage);
        }
        if payload.len() > MAX_LEGACY_MESSAGE_BYTES {
            return Err(ServerChannelError::MessageTooLarge);
        }
        // The high bit denotes fragments; all-ones is an OOB prefix. Never wrap.
        if self.sequence == i32::MAX {
            return Err(ServerChannelError::SequenceExhausted);
        }
        self.message.clear();
        self.message.extend_from_slice(payload);
        xor_protocol_tail(&mut self.message, 4, challenge ^ self.sequence, command);
        self.cursor = 0;
        self.pending = true;
        Ok(self.sequence)
    }

    pub fn pending_bytes(&self) -> usize {
        if self.pending {
            self.message.len() - self.cursor
        } else {
            0
        }
    }

    pub fn next(&mut self, output: &mut [u8]) -> Result<Option<usize>, ServerChannelError> {
        if !self.pending {
            return Ok(None);
        }
        let fragmented = self.message.len() >= FRAGMENT_SIZE;
        let length = if fragmented {
            (self.message.len() - self.cursor).min(FRAGMENT_SIZE)
        } else {
            self.message.len()
        };
        let header = if fragmented { 8 } else { 4 };
        if output.len() < header + length {
            return Err(ServerChannelError::OutputTooSmall);
        }
        let sequence = self.sequence as u32 | if fragmented { FRAGMENT_BIT } else { 0 };
        output[..4].copy_from_slice(&sequence.to_le_bytes());
        if fragmented {
            output[4..6].copy_from_slice(&(self.cursor as u16).to_le_bytes());
            output[6..8].copy_from_slice(&(length as u16).to_le_bytes());
        }
        output[header..header + length]
            .copy_from_slice(&self.message[self.cursor..self.cursor + length]);
        self.cursor += length;
        // An exact full fragment requires a subsequent zero-length terminator.
        if !fragmented || length != FRAGMENT_SIZE {
            self.pending = false;
            self.sequence += 1;
        }
        Ok(Some(header + length))
    }
}
