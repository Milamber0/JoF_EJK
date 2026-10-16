//! Fixed-storage, endpoint-matched prediction diagnostics; no wire or physics changes.

use sjk_client::pmove::MovementState;
use std::fmt;
use std::time::{Duration, Instant};

/// Replay work counts, not distinct network commands; a command may be replayed repeatedly.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Counts {
    /// Number of pending command replay attempts.
    pub replayed: u64,
    /// Replay commands entering at least one deferred saber slice.
    pub deferred: u64,
    /// Total increments of MovementState::saber_special_deferred during replay.
    pub deferred_slices: u64,
    /// Replay commands whose nonzero movement was entirely suppressed by the freeze table.
    pub frozen: u64,
    /// Endpoint-matched misses exceeding eight units, including throttled-out logs.
    pub large: u64,
    /// Large misses whose old or replayed endpoint command was deferred.
    pub large_deferred: u64,
    /// Large misses whose old or replayed endpoint command was movement-suppressed.
    pub large_frozen: u64,
}

impl Counts {
    /// Accumulate one snapshot's counters into a timing window.
    pub(crate) fn add(&mut self, other: Self) {
        self.replayed += other.replayed;
        self.deferred += other.deferred;
        self.deferred_slices += other.deferred_slices;
        self.frozen += other.frozen;
        self.large += other.large;
        self.large_deferred += other.large_deferred;
        self.large_frozen += other.large_frozen;
    }
}

impl fmt::Display for Counts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "replayed={} saber_deferred={} saber_special_deferred={} input_frozen={} \
            large_miss={} large_deferred={} large_frozen={}",
            self.replayed,
            self.deferred,
            self.deferred_slices,
            self.frozen,
            self.large,
            self.large_deferred,
            self.large_frozen
        )
    }
}

/// Small copy of an endpoint's state, excluding prediction buffers or owned data.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Endpoint {
    origin: [f32; 3],
    velocity: [f32; 3],
    flags: u16,
    kind: u8,
    ground: u16,
    saber: u32,
    legs: u16,
    torso: u16,
    legs_timer: i32,
    torso_timer: i32,
    saber_lock_time: i32,
    frozen: bool,
    deferred: bool,
    time: i32,
}

impl Endpoint {
    /// Capture only diagnostic scalar/vector fields from a predicted command endpoint.
    pub(crate) fn new(state: &MovementState) -> Self {
        Self {
            origin: state.origin,
            velocity: state.velocity,
            flags: state.movement_flags,
            kind: state.movement_type,
            ground: state.ground_entity_number,
            saber: state.saber_move,
            legs: state.legs_anim,
            torso: state.torso_anim,
            legs_timer: state.legs_timer,
            torso_timer: state.torso_timer,
            saber_lock_time: state.saber_lock_time,
            frozen: state.input_freeze_active,
            deferred: state.saber_deferred_active,
            time: state.command_time,
        }
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "time={} origin={:?} velocity={:?} pm_flags={} pm_type={} ground={} \
            saber={} legs={} torso={} frozen={} deferred={} \
            legsTimer={} torsoTimer={} saberLockTime={}",
            self.time,
            self.origin,
            self.velocity,
            self.flags,
            self.kind,
            self.ground,
            self.saber,
            self.legs,
            self.torso,
            self.frozen,
            self.deferred,
            self.legs_timer,
            self.torso_timer,
            self.saber_lock_time
        )
    }
}

/// Per-session limiter; independent of whether cl_showtimedelta is enabled.
#[derive(Default)]
pub(crate) struct MissLog {
    next: Option<Instant>,
}

impl MissLog {
    /// Count every large miss; print at most four lines per second without formatting allocation.
    pub(crate) fn observe(
        &mut self,
        magnitude: f32,
        old: Endpoint,
        replay: Endpoint,
        server: Endpoint,
        counts: &mut Counts,
    ) {
        if magnitude <= 8.0 || !magnitude.is_finite() {
            return;
        }
        counts.large += 1;
        counts.large_deferred += u64::from(old.deferred || replay.deferred);
        counts.large_frozen += u64::from(old.frozen || replay.frozen);
        if self.due(Instant::now()) {
            eprintln!(
                "prediction miss: magnitude={magnitude:.2} old=[{old}] \
                replay=[{replay}] server=[{server}]"
            );
        }
    }

    fn due(&mut self, now: Instant) -> bool {
        if self.next.is_some_and(|next| now < next) {
            return false;
        }
        self.next = Some(now + Duration::from_millis(250));
        true
    }
}
