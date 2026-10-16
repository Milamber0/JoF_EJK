//! Wall-clock paced remote presentation with an interpolation bracket in hand.
//!
//! OpenJK codemp `cl_cgame.cpp:626-673,766-795` adjusts the realtime/server
//! delta per snapshot: reset above 500 ms, halve above 100 ms, otherwise drift
//! +1 ms or -2 ms if a frame approached the newest snapshot within 5 ms.
//! The feedback ignores time nudge; sampling subtracts it and never rewinds.
//! Unlike stock's first/reset frame, seed one interval behind the newest
//! snapshot: our runtime retains only the last two samples, not a snap queue.
//! Keep fractional milliseconds across arrivals; never clamp time to a packet.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Instant;

const DEFAULT_INTERVAL_MILLIS: i64 = 25;
const RESET_MILLIS: i64 = 500;
const FAST_ADJUST_MILLIS: i64 = 100;
const EXTRAPOLATION_MARGIN_MILLIS: i64 = 5;

/// Cached archived `cl_timenudge`, avoiding allocating cvar lookups each frame.
pub(crate) struct CvarSetting(Arc<AtomicI64>);

impl CvarSetting {
    /// Bind after registration, before cfg loading; changes include cfg/reset.
    pub(crate) fn bind(cvars: &mut sjk_shell::CvarRegistry) -> Result<Self, sjk_shell::CvarError> {
        let value = Arc::new(AtomicI64::new(0));
        let changed = Arc::clone(&value);
        cvars.on_change("cl_timenudge", move |change| {
            if let sjk_shell::CvarValue::Integer(value) = change.current {
                changed.store(value.clamp(-30, 30), Ordering::Relaxed);
            }
        })?;
        Ok(Self(value))
    }

    /// Effective stock-range nudge in milliseconds, without allocation or locks.
    pub(crate) fn millis(&self) -> i64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// The adjustment branch taken on receipt; observing it never changes the clock.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum TimingEvent {
    /// No new server time (a duplicate timestamp).
    #[default]
    None,
    /// A backwards server timestamp started a different timeline.
    TimelineRestart,
    /// Absolute delta error exceeded 500 ms; restore the interval buffer.
    Reset,
    /// Absolute delta error exceeded 100 ms; halve the difference.
    FastAdjust,
    /// Stock per-snapshot delta drift, in milliseconds (+1 or -2).
    Drift(i8),
}

/// Microsecond realtime/server delta with a monotonic presentation floor.
#[derive(Debug)]
pub(crate) struct SnapshotPresentationClock {
    anchor_server_micros: i64,
    upper_server_time: i64,
    anchor_instant: Instant,
    interval: i64,
    time_nudge_millis: i64,
    last_presented_micros: i64,
    extrapolated_snapshot: bool,
    local: bool,
}

impl SnapshotPresentationClock {
    /// Follow an in-process authority without network interpolation or drift.
    /// Its command clock already advances at wall-clock speed.
    pub(crate) fn follow_local(&mut self, server_time: i32, now: Instant) {
        self.upper_server_time = i64::from(server_time);
        if self.local {
            return;
        }
        self.anchor_server_micros = i64::from(server_time) * 1_000;
        self.anchor_instant = now;
        self.last_presented_micros = self.anchor_server_micros;
        self.local = true;
        self.extrapolated_snapshot = false;
    }

    /// Start one default snapshot interval behind the initial authoritative time.
    pub(crate) fn new(server_time: i32, now: Instant) -> Self {
        let server_time = i64::from(server_time);
        Self {
            anchor_server_micros: (server_time - DEFAULT_INTERVAL_MILLIS) * 1_000,
            upper_server_time: server_time,
            anchor_instant: now,
            interval: DEFAULT_INTERVAL_MILLIS,
            time_nudge_millis: 0,
            last_presented_micros: i64::MIN,
            extrapolated_snapshot: false,
            local: false,
        }
    }

    /// Apply live `cl_timenudge`; positive values add presentation latency.
    pub(crate) fn set_time_nudge_millis(&mut self, millis: i64) {
        self.time_nudge_millis = millis.clamp(-30, 30);
    }

    /// Adjust the delta once per accepted snapshot, independent of frame rate.
    pub(crate) fn receive_snapshot(&mut self, server_time: i32, now: Instant) -> TimingEvent {
        let server_time = i64::from(server_time);
        if server_time < self.upper_server_time {
            // A new map/time domain is the sole exception to monotonic time.
            let nudge = self.time_nudge_millis;
            *self = Self::new(server_time as i32, now);
            self.time_nudge_millis = nudge;
            return TimingEvent::TimelineRestart;
        }
        if server_time == self.upper_server_time {
            return TimingEvent::None;
        }
        self.interval = (server_time - self.upper_server_time).clamp(1, 200);
        self.upper_server_time = server_time;
        let mut micros = self.raw_micros(now);
        // CL_SetCGameTime checks the newly installed snap before adjusting.
        self.observe_extrapolation(micros);
        let error = server_time * 1_000 - micros;
        let event = if error.abs() > RESET_MILLIS * 1_000 {
            micros = (server_time - self.interval) * 1_000;
            TimingEvent::Reset
        } else if error.abs() > FAST_ADJUST_MILLIS * 1_000 {
            micros += error.div_euclid(2);
            TimingEvent::FastAdjust
        } else {
            let drift = if self.extrapolated_snapshot { -2 } else { 1 };
            self.extrapolated_snapshot = false;
            micros += i64::from(drift) * 1_000;
            TimingEvent::Drift(drift)
        };
        self.anchor_server_micros = micros;
        self.anchor_instant = now;
        event
    }

    /// Newest received server timestamp; sampling is allowed to pass it.
    pub(crate) fn upper_server_time(&self) -> i64 {
        self.upper_server_time
    }

    /// Monotonic integer-millisecond presentation time, including time nudge.
    pub(crate) fn sample(&mut self, now: Instant) -> i64 {
        self.sample_micros(now) / 1_000
    }

    fn sample_micros(&mut self, now: Instant) -> i64 {
        let raw = self.raw_micros(now);
        self.observe_extrapolation(raw);
        let nudge = if self.local {
            0
        } else {
            self.time_nudge_millis
        };
        self.last_presented_micros = raw
            .saturating_sub(nudge * 1_000)
            .max(self.last_presented_micros);
        self.last_presented_micros
    }

    fn observe_extrapolation(&mut self, raw_micros: i64) {
        self.extrapolated_snapshot |=
            raw_micros >= (self.upper_server_time - EXTRAPOLATION_MARGIN_MILLIS) * 1_000;
    }

    fn raw_micros(&self, now: Instant) -> i64 {
        let elapsed = now.saturating_duration_since(self.anchor_instant);
        self.anchor_server_micros
            .saturating_add(i64::try_from(elapsed.as_micros()).unwrap_or(i64::MAX))
    }
}
