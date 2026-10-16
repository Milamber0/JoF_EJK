//! Post-gamestate move-packet telemetry.
//!
//! A server silently ignores an entire move packet — before it clears the
//! map-transition time offset and before it runs any command — when the
//! packet's serverId is not the current one, when its message
//! acknowledgement is negative, or when its reliable acknowledgement has
//! fallen more than `MAX_RELIABLE_COMMANDS` behind
//! (`codemp/server/sv_client.cpp:1465-1512`). All three look identical from
//! the client: snapshots keep arriving, `ps.commandTime` never advances and
//! the player is stranded at the spawn point. The three values live in the
//! clear at the head of every move packet, so logging them for a few seconds
//! after each gamestate says which gate closed.

use std::time::{Duration, Instant};

/// How long to sample after a gamestate, and how often. The first seconds
/// are logged packet by packet: a stall is decided by one packet, and a
/// once-a-second sample cannot show which.
const WINDOW: Duration = Duration::from_secs(15);
const FULL_RATE: Duration = Duration::from_secs(3);
const INTERVAL: Duration = Duration::from_secs(1);
/// Snapshots this far below the stamps already sent mean the server restarted
/// its own timeline, the same `RESET_TIME` judgement `CL_AdjustTimeDelta`
/// makes. Stamping low again is then correct, not a step backwards.
const TIMELINE_RESTART_MILLIS: i32 = 500;

/// The head of a move packet, plus what the server has told us since.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MovePacketHead {
    pub(crate) server_id: i32,
    pub(crate) message_acknowledge: i32,
    pub(crate) server_command_sequence: i32,
    pub(crate) command_stamp: i32,
    pub(crate) snapshot_time: i32,
    pub(crate) command_time: i32,
    /// Highest reliable-command sequence the server has actually sent. When
    /// this runs away from the acknowledgement, the server stops reading our
    /// packets entirely once the gap passes `MAX_RELIABLE_COMMANDS`.
    pub(crate) highest_server_command: i32,
}

#[derive(Debug, Default)]
pub(crate) struct GamestateProbe {
    freeze: crate::freeze_probe::FreezeProbe,
    until: Option<Instant>,
    full_rate_until: Option<Instant>,
    next: Option<Instant>,
    /// Highest stamp handed to the server so far. The server keeps the
    /// newest usercmd it has seen and ignores anything not newer
    /// (`sv_client.cpp:1436`), so a stamp that goes backwards is stranded
    /// until real time catches up — permanently, if the step back is large.
    highest_stamp: i32,
}

impl GamestateProbe {
    /// Seed the diagnostic with the initial gamestate's server ID.
    pub(crate) fn new(server_id: i32) -> Self {
        let mut probe = Self::default();
        probe.freeze.gamestate(server_id);
        probe
    }

    /// Remember reliable command text in bounded diagnostic storage.
    pub(crate) fn command(&mut self, sequence: i32, bytes: &[u8]) {
        self.freeze.command(sequence, bytes);
    }

    /// Observe every decoded snapshot, independently of the temporary send probe window.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn snapshot(
        &mut self,
        now: Instant,
        snapshot_time: i32,
        command_time: i32,
        message_acknowledge: i32,
        server_id: i32,
        sequence: i32,
        highest: i32,
    ) {
        self.freeze.snapshot(
            now,
            MovePacketHead {
                server_id,
                message_acknowledge,
                snapshot_time,
                command_time,
                server_command_sequence: sequence,
                highest_server_command: highest,
                command_stamp: 0,
            },
        );
    }
    /// Report a gamestate and start sampling the move packets that follow.
    pub(crate) fn arm(
        &mut self,
        now: Instant,
        previous_server_id: i32,
        server_id: i32,
        system_info: &str,
    ) {
        self.freeze.gamestate(server_id);
        self.until = Some(now + WINDOW);
        self.full_rate_until = Some(now + FULL_RATE);
        self.next = Some(now);
        let raw = system_info
            .split('\\')
            .skip_while(|key| !key.eq_ignore_ascii_case("sv_serverid"))
            .nth(1)
            .unwrap_or("<absent>");
        eprintln!(
            "gamestate: serverId {previous_server_id} -> {server_id} (sv_serverid=\"{raw}\")"
        );
    }

    /// Log one move packet if it broke the stamp invariant, or if the window
    /// is open and a sample is due.
    pub(crate) fn sample(&mut self, now: Instant, head: MovePacketHead) {
        self.freeze.sent(head);
        // Only a step this client took alone matters. When the server
        // restarts its own timeline the snapshots drop with it, and stamping
        // low again is correct. A stamp of 0 is the map load, which carries
        // no timeline at all.
        let restarted =
            head.snapshot_time < self.highest_stamp.saturating_sub(TIMELINE_RESTART_MILLIS);
        let backwards =
            head.command_stamp != 0 && !restarted && head.command_stamp < self.highest_stamp;
        if backwards {
            eprintln!(
                "gamestate move: STAMP WENT BACKWARDS {} -> {} ({} ms), \
                 stranded until the server's clock passes the older one",
                self.highest_stamp,
                head.command_stamp,
                self.highest_stamp - head.command_stamp,
            );
        }
        if head.command_stamp != 0 {
            self.highest_stamp = if restarted {
                head.command_stamp
            } else {
                self.highest_stamp.max(head.command_stamp)
            };
        }
        if self.until.is_some_and(|until| now >= until) {
            self.until = None;
            self.full_rate_until = None;
            self.next = None;
            return;
        }
        if self.until.is_none() {
            return;
        }
        let full_rate = self.full_rate_until.is_some_and(|until| now < until);
        if !full_rate && self.next.is_some_and(|next| now < next) {
            return;
        }
        self.next = Some(now + INTERVAL);
        eprintln!(
            "gamestate move: serverId={} msgAck={} relAck={}/{} stamp={} snapTime={} \
             cmdTime={}",
            head.server_id,
            head.message_acknowledge,
            head.server_command_sequence,
            head.highest_server_command,
            head.command_stamp,
            head.snapshot_time,
            head.command_time,
        );
    }
}
