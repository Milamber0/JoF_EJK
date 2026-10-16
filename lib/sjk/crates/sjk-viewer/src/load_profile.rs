//! One-shot world-install profiling with monotonic sub-phase timestamps.

use std::time::Instant;

/// Prints individual and cumulative load durations without affecting frame hot paths.
pub(crate) struct LoadProfile<'a> {
    cancelled: Option<&'a std::sync::atomic::AtomicBool>,
    started: Instant,
    previous: Instant,
}

impl<'a> LoadProfile<'a> {
    pub(crate) fn start(cancelled: Option<&'a std::sync::atomic::AtomicBool>) -> Self {
        let now = Instant::now();
        Self {
            cancelled,
            started: now,
            previous: now,
        }
    }

    pub(crate) fn mark(&mut self, phase: &str) -> Result<(), String> {
        if self
            .cancelled
            .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
        {
            return Err("world install cancelled".into());
        }
        let now = Instant::now();
        crate::log::progress(format_args!(
            "world load phase {phase}: phase={:.1}ms cumulative={:.1}ms",
            now.duration_since(self.previous).as_secs_f64() * 1_000.0,
            now.duration_since(self.started).as_secs_f64() * 1_000.0,
        ));
        self.previous = now;
        Ok(())
    }
}
