//! Measured host frame work. Queue/present call durations are not GPU timestamps.
use std::time::Instant;

/// Mutually exclusive host phases, whose durations sum to the frame's elapsed work.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Phase {
    Other,
    Hud,
    Pose,
    World,
    Acquire,
    Effects,
    Encode,
    Submit,
    Present,
    Shell,
    Receive,
    Snapshot,
    Commands,
    Preview,
    Config,
    View,
    EncodeInstances,
    EncodeViews,
    EncodeWorld,
    EncodeOverlays,
    EncodeCopy,
    EffectUploads,
    EffectBillboards,
    EffectSort,
    QueueSubmit,
}

/// Stable ordering for reports and captured worst-frame attribution.
pub(crate) const NAMES: [&str; 25] = [
    "other",
    "hud",
    "pose+skin+upload",
    "world+actor-submit",
    "acquire-wait",
    "effects+lights",
    "encode-effect-geometry",
    "encoder-finish",
    "present-call",
    "shell+input+runtime",
    "socket-receive+decode",
    "snapshot+reconcile",
    "commands+prediction",
    "demo+prediction-preview",
    "config-refresh",
    "view+menu+audio",
    "encode-instance-pack+uploads",
    "encode-secondary-views",
    "encode-world-pass",
    "encode-overlays",
    "encode-capture-copy",
    "encode-effect-uploads",
    "encode-effect-billboards",
    "encode-effect-sort+pack",
    "queue-submit+post-submit",
];

/// Sum the disjoint subdivisions of the original encode+uploads region.
pub(crate) fn encode_total(phases: &[f64; NAMES.len()]) -> f64 {
    [
        Phase::Encode,
        Phase::EncodeInstances,
        Phase::EncodeViews,
        Phase::EncodeWorld,
        Phase::EncodeOverlays,
        Phase::EncodeCopy,
        Phase::EffectUploads,
        Phase::EffectBillboards,
        Phase::EffectSort,
    ]
    .iter()
    .map(|p| phases[*p as usize])
    .sum()
}

/// Sum effect preparation only, preserving the step-134 aggregate for comparison.
pub(crate) fn effect_total(phases: &[f64; NAMES.len()]) -> f64 {
    [
        Phase::Encode,
        Phase::EffectUploads,
        Phase::EffectBillboards,
        Phase::EffectSort,
    ]
    .iter()
    .map(|p| phases[*p as usize])
    .sum()
}

/// One actual frame, not a reciprocal of throughput.
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct Sample {
    /// Begin-to-end host work, including blocking driver calls, in milliseconds.
    pub(crate) work: f64,
    /// Begin-to-begin interval (zero for the first frame), including pacing/event delay.
    pub(crate) interval: f64,
    /// Disjoint host phase durations in milliseconds.
    pub(crate) phases: [f64; NAMES.len()],
}

/// Stack-only timer; no global state, heap allocation, GPU synchronization or locks.
pub(crate) struct Timer {
    start: Instant,
    last: Instant,
    phase: Phase,
    sample: Sample,
}

impl Timer {
    /// Start at actual entry into rendering, not at a redraw request.
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            start: now,
            last: now,
            phase: Phase::Other,
            sample: Sample::default(),
        }
    }

    /// Charge the preceding interval to its phase, then change phases.
    pub(crate) fn mark(&mut self, phase: Phase) {
        let now = Instant::now();
        self.sample.phases[self.phase as usize] +=
            now.duration_since(self.last).as_secs_f64() * 1000.0;
        self.last = now;
        self.phase = phase;
    }

    /// Close the final phase; pacing sleep and later GPU execution are not host work.
    pub(crate) fn finish(mut self) -> Sample {
        self.mark(Phase::Other);
        self.sample.work = self.last.duration_since(self.start).as_secs_f64() * 1000.0;
        self.sample
    }
}

/// Rolling population size, independent of framerate. Storage is allocated once at startup.
pub(crate) const CAPACITY: usize = 2048;

/// Summary of measured durations, with the complete record of the worst frame.
#[derive(Default, Debug)]
pub(crate) struct Stats {
    /// Completed frames retained in this population.
    pub(crate) count: usize,
    /// Arithmetic mean of the selected measured duration, milliseconds.
    pub(crate) mean: f64,
    /// Mean host phase durations over the same retained population, in milliseconds.
    pub(crate) mean_phases: [f64; NAMES.len()],
    /// Nearest-rank 99th percentile, milliseconds.
    pub(crate) p99: f64,
    /// Reciprocal of the mean duration of the slowest ceil(1%) samples.
    pub(crate) low: f64,
    /// Samples whose selected duration exceeds the explicit 3.00 ms budget.
    pub(crate) over_budget: usize,
    /// Worst selected sample, preserving its own host work and phase durations.
    pub(crate) worst: Sample,
}

/// Fixed-capacity samples and sort scratch; summarizing never reallocates.
pub(crate) struct Window {
    samples: Box<[Sample]>,
    scratch: Box<[f64]>,
    next: usize,
    count: usize,
}

impl Window {
    /// Allocate retained storage once, outside rendering.
    pub(crate) fn new() -> Self {
        Self {
            samples: vec![Sample::default(); CAPACITY].into_boxed_slice(),
            scratch: vec![0.0; CAPACITY].into_boxed_slice(),
            next: 0,
            count: 0,
        }
    }

    /// Retain one sample, overwriting the oldest when full.
    pub(crate) fn push(&mut self, sample: Sample) {
        self.samples[self.next] = sample;
        self.next = (self.next + 1) % CAPACITY;
        self.count = (self.count + 1).min(CAPACITY);
    }

    /// Nearest-rank p99 and slowest-1% mean, on measured work milliseconds.
    pub(crate) fn stats(&mut self) -> Stats {
        self.summarize(false)
    }

    /// Delivered cadence, including pacing/event delay; omit the initial unknown interval.
    pub(crate) fn cadence_stats(&mut self) -> Stats {
        self.summarize(true)
    }

    fn summarize(&mut self, cadence: bool) -> Stats {
        let duration = |sample: &Sample| {
            if cadence {
                sample.interval
            } else {
                sample.work
            }
        };
        let mut result = Stats::default();
        for sample in &self.samples[..self.count] {
            let value = duration(sample);
            if cadence && value <= 0.0 {
                continue;
            }
            self.scratch[result.count] = value;
            result.count += 1;
            result.mean += value;
            if !cadence {
                for (sum, phase) in result.mean_phases.iter_mut().zip(sample.phases) {
                    *sum += phase;
                }
            }
            result.over_budget += usize::from(value > 3.0);
            if value > duration(&result.worst) {
                result.worst = *sample;
            }
        }
        let count = result.count;
        if count == 0 {
            return result;
        }
        result.mean /= count as f64;
        for phase in &mut result.mean_phases {
            *phase /= count as f64;
        }
        let sorted = &mut self.scratch[..count];
        sorted.sort_unstable_by(f64::total_cmp);
        result.p99 = sorted[(99 * count).div_ceil(100) - 1];
        let tail = count.div_ceil(100);
        let slow_mean = sorted[count - tail..].iter().sum::<f64>() / tail as f64;
        result.low = if slow_mean > 0.0 {
            1000.0 / slow_mean
        } else {
            0.0
        };
        result
    }
}
