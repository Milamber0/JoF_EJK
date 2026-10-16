//! Always-on gameplay hitch recorder: one client-log line per stutter.
//!
//! Every rendered frame already carries its measured host phases
//! ([`budget::Sample`]); this keeps the last gameplay intervals and, when one is
//! far above their median, names the phases of the frame that ran long. Nothing
//! here allocates per frame: the median is sorted only for a candidate frame
//! (over [`FLOOR_MS`]) and a line is formatted only for a reported hitch.
use crate::frame_pacing::budget::{self, NAMES};
use std::fmt;
use std::time::{Duration, Instant};

/// Never a hitch at or under this: the same 33 ms as the load-gap report.
pub(crate) const FLOOR_MS: f64 = crate::log::HITCH_MS;
/// A hitch is also this many times the recent median frame time.
pub(crate) const MEDIAN_FACTOR: f64 = 3.0;
/// Frames ignored after a map change, focus change, resize or graphics reload.
pub(crate) const SETTLE: Duration = Duration::from_secs(2);
/// At most one hitch line this often; the rest only count towards the summary.
pub(crate) const LINE_SPACING: Duration = Duration::from_secs(1);
/// Gameplay time between summaries, in milliseconds.
pub(crate) const SUMMARY_MS: f64 = 60_000.0;
/// A longer interval is a pause (window drag, debugger, sleep), not a hitch.
pub(crate) const PAUSE_MS: f64 = 5_000.0;
/// Recent gameplay intervals behind the median.
const HISTORY: usize = 128;
/// Intervals needed before a median is trusted.
const MIN_HISTORY: usize = 16;
/// The pseudo-phase for time outside the previous frame's measured work.
const BETWEEN: &str = "between-frames";

/// What must stay the same for frames to compare: a change settles the recorder.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Context {
    pub(crate) map_changes: u32,
    pub(crate) size: [u32; 2],
    pub(crate) focused: bool,
}

/// One reported hitch: the delivered interval and its three largest parts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Hitch {
    pub(crate) ms: f64,
    pub(crate) median_ms: f64,
    pub(crate) top: [(&'static str, f64); 3],
}

impl fmt::Display for Hitch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "hitch: {:.1} ms (median {:.1} ms)",
            self.ms, self.median_ms
        )?;
        let mut first = true;
        for (name, ms) in self.top.iter().filter(|(name, _)| !name.is_empty()) {
            write!(f, "{} {name} {ms:.1}", if first { "" } else { "," })?;
            first = false;
        }
        Ok(())
    }
}

/// Hitches counted over [`SUMMARY_MS`] of gameplay, reported or not.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Summary {
    pub(crate) count: u32,
    pub(crate) worst_ms: f64,
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "hitches: {} in the last {:.0} s, worst {:.1} ms",
            self.count,
            SUMMARY_MS / 1_000.0,
            self.worst_ms
        )
    }
}

/// What one frame produced for the log.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Reports {
    pub(crate) hitch: Option<Hitch>,
    pub(crate) summary: Option<Summary>,
}

/// Fixed-size state; lives in the GPU state and is handed across world installs.
pub(crate) struct Recorder {
    history: [f64; HISTORY],
    scratch: [f64; HISTORY],
    next: usize,
    count: usize,
    settle_until: Option<Instant>,
    previous: Option<budget::Sample>,
    context: Option<Context>,
    last_line: Option<Instant>,
    period_ms: f64,
    period_count: u32,
    period_worst: f64,
}

impl Default for Recorder {
    fn default() -> Self {
        Self {
            history: [0.0; HISTORY],
            scratch: [0.0; HISTORY],
            next: 0,
            count: 0,
            settle_until: None,
            previous: None,
            context: None,
            last_line: None,
            period_ms: 0.0,
            period_count: 0,
            period_worst: 0.0,
        }
    }
}

impl Recorder {
    /// Ignore the next [`SETTLE`] and start a new median (vid_restart, a world install).
    pub(crate) fn settle(&mut self, now: Instant) {
        self.settle_until = Some(now + SETTLE);
        self.count = 0;
        self.next = 0;
    }

    /// Feed one completed frame. `sample.interval` is this frame's start minus the
    /// previous one's, so it is the previous frame's work plus the time between them:
    /// a hitch names the previous frame's phases.
    pub(crate) fn observe(
        &mut self,
        now: Instant,
        gameplay: bool,
        context: Context,
        sample: budget::Sample,
    ) -> Reports {
        let mut reports = Reports::default();
        if !gameplay {
            self.previous = None;
            self.settle(now);
            return reports;
        }
        if self.context != Some(context) {
            self.context = Some(context);
            self.previous = Some(sample);
            self.settle(now);
            return reports;
        }
        let Some(previous) = self.previous.replace(sample) else {
            self.settle(now);
            return reports;
        };
        let interval = sample.interval;
        if interval > PAUSE_MS {
            self.settle(now);
            return reports;
        }
        if self.settle_until.is_some_and(|until| now < until) {
            self.push(interval);
            return reports;
        }
        self.settle_until = None;
        self.period_ms += interval;
        if interval > FLOOR_MS && self.count >= MIN_HISTORY {
            let median = self.median();
            if interval > FLOOR_MS.max(MEDIAN_FACTOR * median) {
                self.period_count += 1;
                self.period_worst = self.period_worst.max(interval);
                if self
                    .last_line
                    .is_none_or(|last| now.duration_since(last) >= LINE_SPACING)
                {
                    self.last_line = Some(now);
                    reports.hitch = Some(Hitch {
                        ms: interval,
                        median_ms: median,
                        top: top_phases(&previous, interval),
                    });
                }
            }
        }
        self.push(interval);
        if self.period_ms >= SUMMARY_MS {
            if self.period_count > 0 {
                reports.summary = Some(Summary {
                    count: self.period_count,
                    worst_ms: self.period_worst,
                });
            }
            self.period_ms = 0.0;
            self.period_count = 0;
            self.period_worst = 0.0;
        }
        reports
    }

    fn push(&mut self, interval: f64) {
        self.history[self.next] = interval;
        self.next = (self.next + 1) % HISTORY;
        self.count = (self.count + 1).min(HISTORY);
    }

    /// Median of the retained intervals (upper middle for an even count).
    fn median(&mut self) -> f64 {
        let values = &mut self.scratch[..self.count];
        values.copy_from_slice(&self.history[..self.count]);
        let middle = self.count / 2;
        *values.select_nth_unstable_by(middle, f64::total_cmp).1
    }
}

/// The three largest of the frame's phases and the time between frames.
fn top_phases(previous: &budget::Sample, interval: f64) -> [(&'static str, f64); 3] {
    let mut top = [("", 0.0); 3];
    let between = (interval - previous.work).max(0.0);
    let parts = NAMES
        .iter()
        .copied()
        .zip(previous.phases)
        .chain(std::iter::once((BETWEEN, between)));
    for (name, ms) in parts {
        if ms <= 0.0 {
            continue;
        }
        if let Some(slot) = top.iter().position(|(_, kept)| ms > *kept) {
            top[slot..].rotate_right(1);
            top[slot] = (name, ms);
        }
    }
    top
}

impl crate::GpuState {
    /// Gameplay only: a live world on screen, past its first frame, window focused.
    fn hitch_gameplay(&self) -> bool {
        self.live_presentation_ready()
            && self.connect_timeline.is_none()
            && !self.transition_report_pending
            && self.gameplay_input.is_focused()
    }

    /// Check the frame just recorded and log a hitch or a summary.
    pub(crate) fn observe_hitch(&mut self, now: Instant) {
        let context = Context {
            map_changes: self.completed_map_changes,
            size: [self.size.width, self.size.height],
            focused: self.gameplay_input.is_focused(),
        };
        let gameplay = self.hitch_gameplay();
        let sample = self.frame_pacer.last;
        let reports = self.hitches.observe(now, gameplay, context, sample);
        for line in [
            reports.hitch.map(|hitch| hitch.to_string()),
            reports.summary.map(|summary| summary.to_string()),
        ]
        .into_iter()
        .flatten()
        {
            crate::log::progress(format_args!("{line}"));
            if let Some(console) = &mut self.console {
                console.push_log_quiet(line);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOCUSED: Context = Context {
        map_changes: 0,
        size: [1920, 1080],
        focused: true,
    };

    fn sample(interval: f64) -> budget::Sample {
        let mut sample = budget::Sample {
            interval,
            work: 3.0,
            ..budget::Sample::default()
        };
        sample.phases[budget::Phase::Hud as usize] = 1.0;
        sample.phases[budget::Phase::Config as usize] = 2.0;
        sample
    }

    /// A recorder past its settle time with `frames` steady frames of `ms`.
    fn steady(ms: f64, frames: u32) -> (Recorder, Instant) {
        let mut recorder = Recorder::default();
        let mut now = Instant::now();
        for _ in 0..frames {
            now += Duration::from_secs_f64(ms / 1_000.0);
            let reports = recorder.observe(now, true, FOCUSED, sample(ms));
            assert_eq!(reports, Reports::default());
        }
        (recorder, now)
    }

    fn hitch_at(recorder: &mut Recorder, now: Instant, ms: f64) -> Option<Hitch> {
        recorder.observe(now, true, FOCUSED, sample(ms)).hitch
    }

    #[test]
    fn median_of_recent_intervals() {
        let mut recorder = Recorder::default();
        for value in [9.0, 1.0, 5.0, 3.0, 7.0] {
            recorder.push(value);
        }
        assert_eq!(recorder.median(), 5.0);
        for _ in 0..HISTORY {
            recorder.push(4.0);
        }
        assert_eq!(recorder.count, HISTORY);
        assert_eq!(recorder.median(), 4.0);
    }

    #[test]
    fn threshold_is_floor_or_three_medians() {
        // Median 4 ms: the 33 ms floor rules.
        let (mut recorder, now) = steady(4.0, 600);
        assert!(hitch_at(&mut recorder, now, 33.0).is_none());
        let hitch = hitch_at(&mut recorder, now, 40.0).expect("over the floor");
        assert_eq!(hitch.ms, 40.0);
        assert_eq!(hitch.median_ms, 4.0);
        // Median 16 ms: three medians (48 ms) rules.
        let (mut recorder, now) = steady(16.0, 200);
        assert!(hitch_at(&mut recorder, now, 45.0).is_none());
        assert!(hitch_at(&mut recorder, now, 49.0).is_some());
    }

    #[test]
    fn names_the_previous_frames_largest_phases() {
        let (mut recorder, now) = steady(4.0, 600);
        let mut slow = sample(4.0);
        slow.work = 60.0;
        slow.phases[budget::Phase::Config as usize] = 55.0;
        recorder.observe(now, true, FOCUSED, slow);
        let hitch = hitch_at(&mut recorder, now, 70.0).expect("hitch");
        assert_eq!(
            hitch.top,
            [("config-refresh", 55.0), (BETWEEN, 10.0), ("hud", 1.0)]
        );
        assert_eq!(
            hitch.to_string(),
            "hitch: 70.0 ms (median 4.0 ms) config-refresh 55.0, between-frames 10.0, hud 1.0"
        );
    }

    #[test]
    fn one_line_a_second_and_a_summary_each_minute() {
        let (mut recorder, mut now) = steady(4.0, 600);
        assert!(hitch_at(&mut recorder, now, 50.0).is_some());
        now += Duration::from_millis(500);
        assert!(hitch_at(&mut recorder, now, 80.0).is_none());
        now += Duration::from_millis(500);
        assert!(hitch_at(&mut recorder, now, 40.0).is_some());
        let mut summary = None;
        for _ in 0..20_000 {
            now += Duration::from_millis(4);
            let reports = recorder.observe(now, true, FOCUSED, sample(4.0));
            if reports.summary.is_some() {
                assert!(summary.is_none(), "one summary per minute");
                summary = reports.summary;
            }
        }
        let summary = summary.expect("summary after 60 s of gameplay");
        assert_eq!(
            summary,
            Summary {
                count: 3,
                worst_ms: 80.0
            }
        );
        assert_eq!(
            summary.to_string(),
            "hitches: 3 in the last 60 s, worst 80.0 ms"
        );
    }

    #[test]
    fn a_quiet_minute_logs_nothing() {
        let (_, _) = steady(4.0, 16_000);
    }

    #[test]
    fn changes_and_menus_settle_for_two_seconds() {
        let (mut recorder, mut now) = steady(4.0, 600);
        let resized = Context {
            size: [1280, 720],
            ..FOCUSED
        };
        assert_eq!(
            recorder.observe(now, true, resized, sample(4.0)),
            Reports::default()
        );
        for _ in 0..400 {
            now += Duration::from_millis(4);
            let reports = recorder.observe(now, true, resized, sample(4.0));
            assert_eq!(reports, Reports::default());
        }
        let late = recorder.observe(now, true, resized, sample(90.0));
        assert!(late.hitch.is_none(), "still settling at 1.6 s");
        now += SETTLE;
        assert!(
            recorder
                .observe(now, true, resized, sample(90.0))
                .hitch
                .is_some()
        );
        // Out of gameplay (loading, menu world, unfocused): nothing, then settle.
        now += LINE_SPACING;
        recorder.observe(now, false, resized, sample(90.0));
        assert!(
            recorder
                .observe(now, true, resized, sample(90.0))
                .hitch
                .is_none()
        );
        // A long pause is not a hitch either.
        let (mut recorder, now) = steady(4.0, 600);
        assert!(hitch_at(&mut recorder, now, PAUSE_MS + 1.0).is_none());
    }
}
