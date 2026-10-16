//! Client-side pacing for reliable (string) commands.
//!
//! Stock servers silently discard any client command that arrives within
//! `sv_floodProtect` (1000 ms) of the previously accepted one once the client
//! is active: `SV_ClientCommand` (`codemp/server/sv_client.cpp:1274-1296`)
//! advances `lastClientCommand` but executes the string with `clientOk ==
//! qfalse`. Nothing tells the client. A stock player rarely types two commands
//! in a second; a shell that emits `userinfo`, `forcechanged` and `team` in
//! one burst loses everything after the first (EFF JA+ server,
//! `target/parity-reports/playtest-japlus5`). Commands still leave through the
//! ordinary reliable path — only their release time is spaced out.
//!
//! Commands sent while the server still considers the client `CS_PRIMED`
//! (the connect and map-restart bootstrap packets) are exempt and are not
//! routed through here.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// `sv_floodProtect` default window plus transport jitter margin.
pub const DEFAULT_COMMAND_INTERVAL: Duration = Duration::from_millis(1_050);
/// Outbox bound; more than this means a runaway caller, not a slow server.
const MAX_QUEUED: usize = 64;

/// Unsequenced outbox that releases one command per interval.
#[derive(Debug)]
pub(crate) struct ReliableCommandPacer {
    interval: Duration,
    last_sent: Option<Instant>,
    outbox: VecDeque<Vec<u8>>,
}

impl ReliableCommandPacer {
    pub(crate) fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_sent: None,
            outbox: VecDeque::new(),
        }
    }

    /// Queue a command; `false` when the outbox is full.
    pub(crate) fn push(&mut self, command: Vec<u8>) -> bool {
        if self.outbox.len() >= MAX_QUEUED {
            return false;
        }
        self.outbox.push_back(command);
        true
    }

    /// The next command whose release time has come, marking it sent.
    pub(crate) fn pop_due(&mut self, now: Instant) -> Option<Vec<u8>> {
        if self.outbox.is_empty() {
            return None;
        }
        if let Some(last) = self.last_sent {
            if now.saturating_duration_since(last) < self.interval {
                return None;
            }
        }
        self.last_sent = Some(now);
        self.outbox.pop_front()
    }

    /// Commands still waiting for their release time.
    pub(crate) fn queued(&self) -> usize {
        self.outbox.len()
    }

    /// Forget everything queued, e.g. across a gamestate replacement.
    pub(crate) fn clear(&mut self) {
        self.outbox.clear();
        self.last_sent = None;
    }
}
