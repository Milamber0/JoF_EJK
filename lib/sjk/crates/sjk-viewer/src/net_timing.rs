//! Opt-in, five-second snapshot timing windows (`cl_showtimedelta`).
//!
//! OpenJK codemp `CL_SetCGameTime` / `CL_AdjustTimeDelta` prints the delta on
//! each snapshot. This viewer reports aggregate observations without steering
//! either clock. Arrival times are when the existing receive call returns,
//! including any time spent queued in the socket. `gaps` counts server-time
//! intervals strictly larger than 1.5 times the window's median interval;
//! the receive API cannot peek for another snapshot without decoding it.

use crate::local_prediction::{CommandStatus, PredictionSample};
use crate::presentation_clock::TimingEvent;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(5);
// More than 13,000 snapshots/s in one window. Reserve only when enabled, never
// grow on a frame. If exhausted, report n/a for order statistics, not a biased
// subset. Counts, extrema, averages and frame/adjustment totals remain exact.
const SAMPLE_CAPACITY: usize = 65_536;

/// Cached cvar state: the shell's general name lookup allocates a lowercase key.
pub(crate) struct CvarSetting(Arc<AtomicBool>);

impl CvarSetting {
    /// Bind after cvar registration and before cfg loading so all changes apply.
    pub(crate) fn bind(cvars: &mut sjk_shell::CvarRegistry) -> Result<Self, sjk_shell::CvarError> {
        // Seed from the registered default: on_change only fires for later sets.
        let initial = cvars.get("cl_showtimedelta").is_some_and(
            |cvar| matches!(cvar.value, sjk_shell::CvarValue::Integer(value) if value != 0),
        );
        let enabled = Arc::new(AtomicBool::new(initial));
        let changed = Arc::clone(&enabled);
        cvars.on_change("cl_showtimedelta", move |change| {
            let enabled = matches!(
                change.current, sjk_shell::CvarValue::Integer(value) if value != 0
            );
            changed.store(enabled, Ordering::Relaxed);
        })?;
        Ok(Self(enabled))
    }

    /// Read the cvar's nonzero state without lookup, allocation, or locking.
    pub(crate) fn enabled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Default, PartialEq)]
struct Range {
    count: u64,
    sum: i128,
    min: i64,
    max: i64,
}

impl Range {
    fn add(&mut self, value: i64) {
        if self.count == 0 {
            self.min = value;
            self.max = value;
        } else {
            self.min = self.min.min(value);
            self.max = self.max.max(value);
        }
        self.count += 1;
        self.sum += i128::from(value);
    }

    fn average(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum as f64 / self.count as f64
        }
    }
}

#[derive(Debug)]
struct Samples {
    values: Vec<i64>,
    overflow: bool,
}

impl Samples {
    fn new() -> Self {
        Self {
            values: Vec::with_capacity(SAMPLE_CAPACITY),
            overflow: false,
        }
    }

    fn add(&mut self, value: i64) {
        if self.values.len() < SAMPLE_CAPACITY {
            self.values.push(value);
        } else {
            self.overflow = true;
        }
    }

    fn clear(&mut self) {
        self.values.clear();
        self.overflow = false;
    }

    fn percentile(&self, percent: usize) -> Option<i64> {
        if self.overflow {
            None
        } else if self.values.is_empty() {
            Some(0)
        } else {
            // Nearest-rank percentile, after sorting once per report.
            Some(self.values[(self.values.len() * percent).div_ceil(100) - 1])
        }
    }

    fn median_twice(&self) -> Option<i64> {
        if self.overflow {
            None
        } else if self.values.is_empty() {
            Some(0)
        } else {
            let len = self.values.len();
            Some(self.values[(len - 1) / 2] + self.values[len / 2])
        }
    }
}

struct Window {
    start: Option<Instant>,
    previous: Option<(i32, Instant)>,
    snaps: u64,
    interval: Range,
    intervals: Samples,
    jitter_micros: Samples,
    buffer: Range,
    frozen: u64,
    resets: u64,
    fast_adjusts: u64,
    /// Local-player prediction: `ps.commandTime` outcome per snapshot.
    acknowledged: u64,
    rewritten: u64,
    stalled: u64,
    pending: Range,
    /// Prediction miss in centi-units, so `Range` stays integer.
    miss_centi: Range,
    prediction_counts: crate::local_prediction::telemetry::Counts,
    /// Snapshots whose player state was a spectator, and pm_type extremes.
    spectator_snaps: u64,
    pm_type: Range,
}

impl Window {
    fn new() -> Self {
        Self {
            start: None,
            previous: None,
            snaps: 0,
            interval: Range::default(),
            intervals: Samples::new(),
            jitter_micros: Samples::new(),
            buffer: Range::default(),
            frozen: 0,
            resets: 0,
            fast_adjusts: 0,
            acknowledged: 0,
            rewritten: 0,
            stalled: 0,
            pending: Range::default(),
            miss_centi: Range::default(),
            prediction_counts: Default::default(),
            spectator_snaps: 0,
            pm_type: Range::default(),
        }
    }

    fn clear(&mut self, start: Option<Instant>) {
        self.start = start;
        self.snaps = 0;
        self.interval = Range::default();
        self.intervals.clear();
        self.jitter_micros.clear();
        self.buffer = Range::default();
        self.frozen = 0;
        self.resets = 0;
        self.fast_adjusts = 0;
        self.acknowledged = 0;
        self.rewritten = 0;
        self.stalled = 0;
        self.pending = Range::default();
        self.miss_centi = Range::default();
        self.prediction_counts = Default::default();
        self.spectator_snaps = 0;
        self.pm_type = Range::default();
    }
}

/// Disabled by default; all sample storage is allocated only on cvar enable.
#[derive(Default)]
pub(crate) struct NetTiming {
    window: Option<Window>,
    /// Arrival of the newest snapshot, tracked whether or not the timing
    /// window is enabled: a session that stops receiving snapshots entirely
    /// reports nothing at all, which is exactly when it needs to speak up.
    last_snapshot: Option<Instant>,
    next_silence_report: Option<Instant>,
}

/// How long a live session may go without a snapshot before it is reported,
/// and how often to repeat while it stays silent.
const SILENCE: Duration = Duration::from_secs(3);
const SILENCE_REPEAT: Duration = Duration::from_secs(5);

impl NetTiming {
    /// Exclude loading-world samples without changing either clock or silence detection.
    pub(crate) fn active_frame(
        &mut self,
        ready: bool,
        now: Instant,
        newest: i64,
        presented: i64,
        upper: i64,
    ) -> Option<String> {
        if !ready {
            if let Some(window) = &mut self.window {
                window.clear(None);
                window.previous = None;
            }
            return None;
        }
        self.frame(now, newest, presented, upper)
    }

    /// Apply the integer cvar's nonzero state; unchanged settings do no work.
    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        if enabled && self.window.is_none() {
            self.window = Some(Window::new());
        } else if !enabled && self.window.is_some() {
            self.window = None;
        }
    }

    /// Forget a departed session while retaining allocated sample storage.
    pub(crate) fn end_session(&mut self) {
        self.last_snapshot = None;
        self.next_silence_report = None;
        if let Some(window) = &mut self.window {
            window.clear(None);
            window.previous = None;
        }
    }

    /// Observe one accepted snapshot at the wall time its receive call returned.
    pub(crate) fn snapshot_received(&mut self, server_time: i32) {
        let now = Instant::now();
        self.last_snapshot = Some(now);
        self.next_silence_report = None;
        if self.window.is_some() {
            self.snapshot(server_time, now);
        }
    }

    /// Start watching a freshly joined session for snapshot silence.
    pub(crate) fn begin_session(&mut self, now: Instant) {
        self.last_snapshot = Some(now);
        self.next_silence_report = None;
    }

    /// How long this session has gone without a snapshot, when that has
    /// passed [`SILENCE`] and a report is due.
    pub(crate) fn silence_to_report(&mut self, now: Instant) -> Option<Duration> {
        let silence = now.saturating_duration_since(self.last_snapshot?);
        if silence < SILENCE || self.next_silence_report.is_some_and(|next| now < next) {
            return None;
        }
        self.next_silence_report = Some(now + SILENCE_REPEAT);
        Some(silence)
    }

    /// Record an arrival using an explicit wall time (also used by fixtures).
    pub(crate) fn snapshot(&mut self, server_time: i32, now: Instant) {
        let Some(window) = &mut self.window else {
            return;
        };
        window.start.get_or_insert(now);
        window.snaps += 1;
        if let Some((previous, arrival)) = window.previous {
            let interval = i64::from(server_time) - i64::from(previous);
            // Timeline restarts/duplicates establish a new arrival baseline;
            // their nonpositive gaps are not network interval/jitter samples.
            if interval > 0 {
                let elapsed = now.saturating_duration_since(arrival).as_micros();
                let elapsed = i64::try_from(elapsed).unwrap_or(i64::MAX);
                window.interval.add(interval);
                window.intervals.add(interval);
                window.jitter_micros.add((elapsed - interval * 1_000).abs());
            }
        }
        window.previous = Some((server_time, now));
    }

    /// Count only the two lag-adjustment branches, not timeline restarts.
    pub(crate) fn adjustment(&mut self, event: TimingEvent) {
        let Some(window) = &mut self.window else {
            return;
        };
        match event {
            TimingEvent::Reset => window.resets += 1,
            TimingEvent::FastAdjust => window.fast_adjusts += 1,
            TimingEvent::None | TimingEvent::TimelineRestart | TimingEvent::Drift(_) => {}
        }
    }

    /// Record what a predicted (non-spectator) snapshot told the local
    /// prediction: whether the server ran one of our commands, how many are
    /// still pending, and how far the re-prediction missed.
    pub(crate) fn prediction(&mut self, sample: PredictionSample) {
        let Some(window) = &mut self.window else {
            return;
        };
        match sample.status {
            CommandStatus::Acknowledged => window.acknowledged += 1,
            CommandStatus::Rewritten => window.rewritten += 1,
            CommandStatus::Stalled => window.stalled += 1,
        }
        window
            .pending
            .add(i64::try_from(sample.pending).unwrap_or(i64::MAX));
        window
            .miss_centi
            .add((f64::from(sample.miss_units) * 100.0) as i64);
        window.prediction_counts.add(sample.counts);
    }

    /// Record the local player's movement type and spectator state.
    pub(crate) fn player_state(&mut self, pm_type: u8, spectator: bool) {
        let Some(window) = &mut self.window else {
            return;
        };
        window.spectator_snaps += u64::from(spectator);
        window.pm_type.add(i64::from(pm_type));
    }

    /// Observe the actual rendered sample, returning at most one line per 5 s.
    pub(crate) fn frame(
        &mut self,
        now: Instant,
        newest: i64,
        presented: i64,
        upper: i64,
    ) -> Option<String> {
        let window = self.window.as_mut()?;
        let start = *window.start.get_or_insert(now);
        window.buffer.add(newest - presented);
        window.frozen += u64::from(presented >= upper);
        if now.saturating_duration_since(start) < WINDOW {
            return None;
        }
        window.intervals.values.sort_unstable();
        window.jitter_micros.values.sort_unstable();
        let gaps = window.intervals.median_twice().map(|median_twice| {
            window
                .intervals
                .values
                .iter()
                .filter(|gap| **gap * 4 > median_twice * 3)
                .count()
        });
        let jitter = |percent| {
            Millis(
                window
                    .jitter_micros
                    .percentile(percent)
                    .map(|us| us as f64 / 1_000.0),
            )
        };
        let frozen_pct = window.frozen as f64 * 100.0 / window.buffer.count as f64;
        let line = format!(
            concat!(
                "nettiming: snaps={} interval={}/{:.1}/{}ms arrival_jitter={}/{}ms ",
                "buffer={}/{:.1}/{}ms frozen_frames={}/{} ({:.1}%) ",
                "resets={} fast_adjusts={} gaps={} | cmds acked={} rewritten={} stalled={} ",
                "pending={}/{:.1}/{} miss={:.1}/{:.1}u pm_type={}..{} spectator_snaps={} {}"
            ),
            window.snaps,
            window.interval.min,
            window.interval.average(),
            window.interval.max,
            jitter(50),
            jitter(95),
            window.buffer.min,
            window.buffer.average(),
            window.buffer.max,
            window.frozen,
            window.buffer.count,
            frozen_pct,
            window.resets,
            window.fast_adjusts,
            Count(gaps),
            window.acknowledged,
            window.rewritten,
            window.stalled,
            window.pending.min,
            window.pending.average(),
            window.pending.max,
            window.miss_centi.average() / 100.0,
            window.miss_centi.max as f64 / 100.0,
            window.pm_type.min,
            window.pm_type.max,
            window.spectator_snaps,
            window.prediction_counts,
        );
        // Keep the previous snapshot so the first gap in the next window is
        // measured across the reporting boundary. No catch-up log burst.
        window.clear(Some(now));
        Some(line)
    }
}

struct Millis(Option<f64>);
impl fmt::Display for Millis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(value) => write!(f, "{value:.1}"),
            None => f.write_str("n/a"),
        }
    }
}

struct Count(Option<usize>);
impl fmt::Display for Count {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(value) => write!(f, "{value}"),
            None => f.write_str("n/a"),
        }
    }
}

/// Mirror the same aggregate line to stderr and scrollback after sampling.
pub(crate) fn frame(gpu: &mut crate::GpuState, presented: i64) {
    if gpu.net_timing.window.is_none() {
        return;
    }
    let Some(session) = &gpu.live_session else {
        return;
    };
    let ready = gpu.live_presentation_ready();
    if let Some(line) = gpu.net_timing.active_frame(
        ready,
        Instant::now(),
        i64::from(session.latest_snapshot().server_time),
        presented,
        gpu.presentation_clock.upper_server_time(),
    ) {
        crate::log::progress(format_args!("{line}"));
        if let Some(console) = &mut gpu.console {
            console.push_log(line);
        }
    }
}
