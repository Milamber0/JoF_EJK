//! Monotonic process logging and connect-timeline formatting.

use std::fmt;
use std::sync::OnceLock;
use std::time::Instant;
static LAUNCH: OnceLock<Instant> = OnceLock::new();
static TIMESTAMPS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Change diagnostic timestamp policy without allocation or locking.
pub(crate) fn set_timestamps(enabled: bool) {
    TIMESTAMPS.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

/// A milestone shown in the post-connect timeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TimelinePhase {
    Challenge,
    Connected,
    Gamestate,
    FirstSnapshot,
    MapLoaded,
    Models,
    Materials,
    Sounds,
    FirstFrame,
}
/// Fixed-storage timings for one connection attempt.
#[derive(Clone, Debug)]
pub(crate) struct ConnectTimeline {
    started: Instant,
    values_ms: [Option<f64>; 9],
    event_gap: GapReport,
}
impl ConnectTimeline {
    pub(crate) fn new() -> Self {
        Self {
            started: Instant::now(),
            values_ms: [None; 9],
            event_gap: GapReport::default(),
        }
    }
    pub(crate) fn mark(&mut self, phase: TimelinePhase) {
        self.values_ms[phase as usize] = Some(self.started.elapsed().as_secs_f64() * 1_000.0);
    }
    pub(crate) fn set_event_gap(&mut self, report: GapReport) {
        self.event_gap = report;
    }
    pub(crate) fn line(&self) -> String {
        let names = [
            "challenge",
            "connect",
            "gamestate",
            "snapshot",
            "map",
            "models",
            "materials",
            "sounds",
            "first-frame",
        ];
        let mut line = String::from("connect timeline:");
        for (name, value) in names.into_iter().zip(self.values_ms) {
            let value = value.map_or_else(|| "pending".to_owned(), |ms| format!("{ms:.1}ms"));
            line.push_str(&format!(" {name}={value}"));
        }
        line.push_str(&format!(" max-event-gap={}", self.event_gap));
        line
    }
}

/// A frame gap longer than this reads as a visible hitch at 60 Hz.
pub(crate) const HITCH_MS: f64 = 33.0;

/// What one measured interval saw: its longest gap, when that gap ended
/// relative to the interval start, and how many gaps counted as hitches.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct GapReport {
    pub(crate) maximum_ms: f64,
    pub(crate) maximum_at_ms: f64,
    pub(crate) hitches: u32,
}

impl fmt::Display for GapReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.1}ms", self.maximum_ms)?;
        if self.hitches > 0 {
            write!(
                f,
                " (worst at +{:.0}ms, {} frames over {HITCH_MS:.0}ms)",
                self.maximum_at_ms, self.hitches
            )?;
        }
        Ok(())
    }
}

/// Allocation-free event-loop gap measurement for a load interval.
#[derive(Clone, Debug, Default)]
pub(crate) struct EventGap {
    started: Option<Instant>,
    last: Option<Instant>,
    report: GapReport,
}

impl EventGap {
    pub(crate) fn start(&mut self, now: Instant) {
        self.started = Some(now);
        self.last = Some(now);
        self.report = GapReport::default();
    }

    pub(crate) fn observe(&mut self, now: Instant) {
        let (Some(started), Some(last)) = (self.started, self.last) else {
            return;
        };
        let gap_ms = now.duration_since(last).as_secs_f64() * 1_000.0;
        if gap_ms > HITCH_MS {
            self.report.hitches += 1;
        }
        if gap_ms > self.report.maximum_ms {
            self.report.maximum_ms = gap_ms;
            self.report.maximum_at_ms = now.duration_since(started).as_secs_f64() * 1_000.0;
        }
        self.last = Some(now);
    }

    pub(crate) fn stop(&mut self) -> GapReport {
        self.started = None;
        self.last = None;
        self.report
    }
}

pub(crate) fn init() {
    let _ = LAUNCH.set(Instant::now());
}

pub(crate) fn progress(arguments: fmt::Arguments<'_>) {
    let launch = LAUNCH.get_or_init(Instant::now);
    // Hosted (`rd-sjk`): the engine's console shows the line instead of stderr.
    if crate::host::forward_log(&arguments) {
        return;
    }
    crate::crash_report::trail(&arguments.to_string());
    if TIMESTAMPS.load(std::sync::atomic::Ordering::Relaxed) {
        eprintln!(
            "[+{:09.3} ms] {arguments}",
            launch.elapsed().as_secs_f64() * 1_000.0
        );
    } else {
        eprintln!("{arguments}");
    }
}
