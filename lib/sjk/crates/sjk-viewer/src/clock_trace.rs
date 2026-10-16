//! Once-per-second stderr trace of the live-session clocks.
//!
//! Enabled by `SJK_TRACE_CLOCKS=1`. Relates the presented time, the command
//! stamp clock, the newest snapshot and the acknowledged command so a delayed
//! or frozen local animation can be attributed to the clock that drifted.

use super::GpuState;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(crate) struct ClockTrace {
    enabled: bool,
    next: Instant,
}

impl ClockTrace {
    pub(crate) fn new() -> Self {
        Self {
            enabled: std::env::var_os("SJK_TRACE_CLOCKS").is_some(),
            next: Instant::now(),
        }
    }
}

/// Print one trace line when due.
pub(crate) fn tick(gpu: &mut GpuState, presentation_time: i64, now: Instant) {
    if !gpu.clock_trace.enabled || now < gpu.clock_trace.next {
        return;
    }
    gpu.clock_trace.next = now + Duration::from_secs(1);
    let Some(session) = &gpu.live_session else {
        return;
    };
    let player = &session.latest_snapshot().player;
    let predicted = gpu.local_prediction.predicted_state();
    eprintln!(
        "clocks: present={presentation_time} stamp={} snap={} ack={} pending={} \
         predicted_cmd={} legs={} pm_type={} anim_local={}",
        gpu.server_clock.last_stamp(),
        gpu.server_clock.latest_snapshot(),
        player.command_time(),
        gpu.local_prediction.pending_len(),
        predicted.map_or(-1, |state| state.command_time),
        predicted.map_or(u16::MAX, |state| state.legs_anim),
        player.movement_type(),
        gpu.local_actor_state.is_active(),
    );
}
