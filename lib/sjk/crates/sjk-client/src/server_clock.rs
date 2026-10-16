//! The client's running estimate of the server's clock.
//!
//! User commands carry the server time the client believes it is; the server
//! clamps that stamp to `[level.time - 1000, level.time + 200]` and drops a
//! command whose stamp does not advance `ps.commandTime` (`g_active.c`
//! `ClientThink_real`). A stamp the server rewrote never matches the one the
//! client predicted, so every pending command is discarded on the next
//! snapshot and the player sees raw, unpredicted snapshots. The stamp
//! therefore has to track real server time, not count issued commands.
//!
//! This mirrors `codemp/client/cl_cgame.cpp`: `CL_FirstSnapshot` anchors
//! `serverTimeDelta` on the first snapshot, `CL_AdjustTimeDelta` (lines
//! 626-673) re-anchors on a 500 ms jump, halves a 100 ms error and otherwise
//! drifts by one or two milliseconds per snapshot, and `CL_SetCGameTime`
//! (lines 779-792) derives a never-decreasing `cl.serverTime`.
//!
//! A clock is *anchored* only once it has seen a snapshot of the timeline it
//! stamps for. Until then it stamps 0, exactly as stock does: `CL_ClearState`
//! zeroes `cl.serverTime` with every gamestate and `CL_SetCGameTime` only
//! runs once the client is `CA_ACTIVE`, so the whole map load is stamped 0.
//! That matters because a server that has not yet seen the client
//! acknowledge the new `serverId` deliberately reports inflated snapshot
//! times (`sv.time + oldServerTime`, `sv_snapshot.cpp:190-198`) and drops to
//! raw `sv.time` afterwards (`sv_client.cpp:1517-1519`). A command stamped
//! from the inflated era is latched raw by `SV_ClientEnterWorld`
//! (`sv_client.cpp:576`) and then outranks every later command forever
//! (`:1436`), which strands the player at the spawn point.

use std::time::Instant;

/// `RESET_TIME`: a delta jump this large is a timeline change, not latency.
const RESET_MILLIS: i64 = 500;
/// Above this the delta is halved instead of nudged.
const FAST_ADJUST_MILLIS: i64 = 100;
/// `cl.extrapolatedSnapshot` margin before the newest snapshot.
const EXTRAPOLATION_MARGIN: i64 = 5;

/// Server-time estimate for stamping user commands (stock `cl.serverTime`).
#[derive(Clone, Debug)]
pub struct ServerClock {
    epoch: Instant,
    /// `cl.serverTimeDelta`: server time minus local milliseconds.
    delta: i64,
    /// Newest snapshot's server time (`cl.snap.serverTime`).
    latest_snapshot: i64,
    /// `cl.oldServerTime`: the monotonic floor.
    floor: i64,
    /// `cl.extrapolatedSnapshot`: a sample ran past the newest snapshot.
    extrapolated: bool,
    /// A snapshot of the timeline being stamped for has been seen
    /// (`CL_FirstSnapshot` has run). Until then every stamp is 0.
    anchored: bool,
    /// The client presents this timeline (stock `CA_ACTIVE`). While it is
    /// still loading the new map, snapshots must not anchor the clock: they
    /// can still carry the server's inflated transition times.
    active: bool,
}

impl ServerClock {
    /// Anchor on the first snapshot (`CL_FirstSnapshot`).
    pub fn new(snapshot_server_time: i32, now: Instant) -> Self {
        let server_time = i64::from(snapshot_server_time);
        Self {
            epoch: now,
            delta: server_time,
            latest_snapshot: server_time,
            floor: server_time,
            extrapolated: false,
            anchored: true,
            active: true,
        }
    }

    /// A clock for a world that is live but has not seen a snapshot yet: it
    /// stamps 0 until the next snapshot anchors it (`CL_FirstSnapshot`).
    pub fn unanchored(now: Instant) -> Self {
        Self {
            anchored: false,
            ..Self::new(0, now)
        }
    }

    /// Forget a departed connection, including its monotonic floor.
    ///
    /// Stock `CL_Disconnect` calls `CL_ClearState` (`codemp/client/cl_main.cpp`:
    /// 726,823). A new server must never receive the previous server's time,
    /// even if input is due before its next snapshot arrives. Stamps stay 0
    /// and snapshots are ignored until the new world calls [`Self::activate`].
    /// Use [`Self::restart`] instead for a gamestate on the same connection:
    /// that server may still remember the highest command already sent.
    pub fn reset_connection(&mut self, now: Instant) {
        *self = Self {
            active: false,
            ..Self::unanchored(now)
        };
    }

    /// Drop the anchor because the server restarted its timeline (a new
    /// gamestate: map change, same-map restart or `map_restart`). Stamps are
    /// 0 and snapshots are ignored until [`Self::activate`], so the inflated
    /// times the server sends during the transition never become an anchor.
    pub fn restart(&mut self) {
        self.anchored = false;
        self.active = false;
        self.extrapolated = false;
    }

    /// The new timeline's world is live (stock `CA_ACTIVE`): let the next
    /// snapshot anchor the clock.
    pub fn activate(&mut self) {
        self.active = true;
    }

    /// Whether stamps track the server (false while stamping 0).
    pub fn anchored(&self) -> bool {
        self.anchored
    }

    fn realtime(&self, now: Instant) -> i64 {
        i64::try_from(now.saturating_duration_since(self.epoch).as_millis()).unwrap_or(i64::MAX)
    }

    /// Drift the estimate toward a newly received snapshot
    /// (`CL_AdjustTimeDelta`).
    pub fn observe_snapshot(&mut self, snapshot_server_time: i32, now: Instant) {
        let server_time = i64::from(snapshot_server_time);
        if !self.active {
            return;
        }
        self.latest_snapshot = server_time;
        if !self.anchored {
            // `CL_FirstSnapshot`: sit exactly on this frame.
            self.anchored = true;
            self.delta = server_time - self.realtime(now);
            // Keep the connection's monotonic floor across a re-anchor. The
            // server remembers the newest usercmd it has seen and ignores
            // anything not newer (`sv_client.cpp:1436`), and it only forgets
            // that when it re-enters the client into the world — which a
            // gamestate sent to an already-active client does not do. A stamp
            // below one already sent is therefore dead on arrival, and a
            // clock that re-anchors a few hundred milliseconds behind the
            // stamps it was just issuing strands the player. The floor drops
            // only when this really is a different timeline, the same
            // `RESET_TIME` judgement `CL_AdjustTimeDelta` makes.
            let restarted = (server_time - self.floor).abs() > RESET_MILLIS;
            self.floor = if restarted {
                server_time
            } else {
                self.floor.max(server_time)
            };
            self.latest_snapshot = server_time;
            self.extrapolated = false;
            return;
        }
        let new_delta = server_time - self.realtime(now);
        let error = (new_delta - self.delta).abs();
        if error > RESET_MILLIS {
            self.delta = new_delta;
            self.floor = server_time;
        } else if error > FAST_ADJUST_MILLIS {
            self.delta = (self.delta + new_delta) >> 1;
        } else if self.extrapolated {
            self.extrapolated = false;
            self.delta -= 2;
        } else {
            self.delta += 1;
        }
    }

    /// The server time to stamp a command issued now (`CL_SetCGameTime`).
    pub fn server_time(&mut self, now: Instant) -> i32 {
        if !self.anchored {
            return 0;
        }
        let raw = self.realtime(now) + self.delta;
        let server_time = raw.max(self.floor);
        self.floor = server_time;
        if raw >= self.latest_snapshot - EXTRAPOLATION_MARGIN {
            self.extrapolated = true;
        }
        i32::try_from(server_time).unwrap_or(i32::MAX)
    }

    /// The most recent stamp handed out, for diagnostics.
    pub fn last_stamp(&self) -> i32 {
        if !self.anchored {
            return 0;
        }
        i32::try_from(self.floor).unwrap_or(i32::MAX)
    }

    /// Newest observed snapshot time, for diagnostics.
    pub fn latest_snapshot(&self) -> i32 {
        i32::try_from(self.latest_snapshot).unwrap_or(i32::MAX)
    }
}
