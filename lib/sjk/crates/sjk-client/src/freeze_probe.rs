//! Bounded, snapshot-driven diagnostics for server-side command starvation.

use crate::gamestate_probe::MovePacketHead;
use std::fmt::{self, Write};
use std::time::{Duration, Instant};

const STALLED_SNAPSHOTS: u32 = 40;

#[derive(Debug)]
/// Fixed-storage history of sent stamps and snapshot command-time progress.
pub(crate) struct FreezeProbe {
    gamestate_id: i32,
    previous_map_stamp: i32,
    maximum_sent: i32,
    sent: Option<MovePacketHead>,
    previous: Option<(i32, i32)>,
    stalled: u32,
    next_report: Option<Instant>,
    command_sequence: i32,
    command_bytes: [u8; 256],
    command_len: usize,
    truncated: bool,
}

impl Default for FreezeProbe {
    fn default() -> Self {
        Self {
            gamestate_id: 0,
            previous_map_stamp: 0,
            maximum_sent: 0,
            sent: None,
            previous: None,
            stalled: 0,
            next_report: None,
            command_sequence: 0,
            command_bytes: [0; 256],
            command_len: 0,
            truncated: false,
        }
    }
}

impl FreezeProbe {
    /// Start a new map's watermark while retaining the previous map's stamp.
    pub(super) fn gamestate(&mut self, id: i32) {
        self.gamestate_id = id;
        self.previous_map_stamp = self.maximum_sent;
        self.maximum_sent = 0;
        self.sent = None;
        self.previous = None;
        self.stalled = 0;
        self.next_report = None;
    }

    /// Store the latest reliable command without allocating or logging unsafe bytes.
    pub(super) fn command(&mut self, sequence: i32, bytes: &[u8]) {
        if sequence < self.command_sequence {
            return;
        }
        self.command_sequence = sequence;
        self.command_len = bytes.len().min(self.command_bytes.len());
        self.command_bytes[..self.command_len].copy_from_slice(&bytes[..self.command_len]);
        self.truncated = bytes.len() > self.command_len;
    }

    /// Preserve the actual sent watermark even when later stamps go backwards.
    pub(super) fn sent(&mut self, head: MovePacketHead) {
        self.maximum_sent = self.maximum_sent.max(head.command_stamp);
        self.sent = Some(head);
    }

    /// Report sustained command starvation at most once per second.
    pub(super) fn snapshot(&mut self, now: Instant, head: MovePacketHead) {
        if !self.observe(now, head.snapshot_time, head.command_time) {
            return;
        }
        let sent = self.sent.unwrap_or(head);
        eprintln!(
            "clientthink freeze: snapshots={} snapTime={} cmdTime={} sentServerId={} \
             gamestateServerId={} currentServerId={} oldServerTime=unknown(server-only) \
             previousMapStamp={} stamp={} maxSentSinceGamestate={} msgAck={} \
             sentRelAck={} relAck={}/{} lastServerCommand[{}]=\"{}{}\"",
            self.stalled,
            head.snapshot_time,
            head.command_time,
            sent.server_id,
            self.gamestate_id,
            head.server_id,
            self.previous_map_stamp,
            sent.command_stamp,
            self.maximum_sent,
            sent.message_acknowledge,
            sent.server_command_sequence,
            head.server_command_sequence,
            head.highest_server_command,
            self.command_sequence,
            Escaped(&self.command_bytes[..self.command_len]),
            if self.truncated { "..." } else { "" },
        );
    }

    fn observe(&mut self, now: Instant, snapshot_time: i32, command_time: i32) -> bool {
        self.stalled = match self.previous {
            Some((snap, cmd)) if snapshot_time > snap && command_time == cmd => {
                self.stalled.saturating_add(1)
            }
            _ => 0,
        };
        self.previous = Some((snapshot_time, command_time));
        if self.stalled == 0 {
            self.next_report = None;
        }
        if self.stalled < STALLED_SNAPSHOTS || self.next_report.is_some_and(|next| now < next) {
            return false;
        }
        self.next_report = Some(now + Duration::from_secs(1));
        true
    }
}

struct Escaped<'a>(&'a [u8]);

impl fmt::Display for Escaped<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for &byte in self.0 {
            for escaped in std::ascii::escape_default(byte) {
                f.write_char(char::from(escaped))?;
            }
        }
        Ok(())
    }
}
