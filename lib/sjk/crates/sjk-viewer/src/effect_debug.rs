//! Effect diagnostics behind `fx_debug` and `cg_debugMissiles`: both cvars are off
//! by default and are not archived, so they change nothing until a player turns
//! one on in the console.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::console::ViewerConsole;

/// Mirror of the `fx_debug` cvar, refreshed by [`sync`] so code without console
/// access (effect spawning, the effect atlas) can test it with one atomic load.
static FX_DEBUG: AtomicBool = AtomicBool::new(false);

/// Each effect name is logged at most once per this interval.
const FX_REPORT_INTERVAL: Duration = Duration::from_secs(1);

/// Last time each effect name was logged by [`report_effect`].
static LAST_REPORT: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

/// Copies `fx_debug` into the flag [`enabled`] reads and returns whether
/// `cg_debugMissiles` is on.
///
/// Call it once per frame whether or not a snapshot is presented, so the flag
/// follows the cvar and goes back to off when the console is gone (or the cvar is
/// set to 0) rather than keeping the last snapshot's value. It reads two cvars by
/// name and allocates nothing.
pub(crate) fn sync(console: Option<&ViewerConsole>) -> bool {
    let on = |name: &str| console.and_then(|c| c.integer_cvar(name)).unwrap_or(0) != 0;
    FX_DEBUG.store(on("fx_debug"), Ordering::Relaxed);
    on("cg_debugMissiles")
}

/// Whether `fx_debug` was on at the last [`sync`].
pub(crate) fn enabled() -> bool {
    FX_DEBUG.load(Ordering::Relaxed)
}

/// `fx_debug 1`: log each effect as it plays (at most once a second per name) with
/// every component's kind, shaders, life and size, to trace a wrong-looking sprite
/// back to the effect and shader that drew it. Does nothing when the cvar is off.
pub(crate) fn report_effect(effect_name: &str, definition: &sjk_effect::EffectDefinition) {
    if !enabled() {
        return;
    }
    let now = Instant::now();
    {
        let Ok(mut last) = LAST_REPORT.lock() else {
            return;
        };
        let last = last.get_or_insert_with(HashMap::new);
        if last
            .get(effect_name)
            .is_some_and(|at| now.duration_since(*at) < FX_REPORT_INTERVAL)
        {
            return;
        }
        last.insert(effect_name.to_owned(), now);
    }
    let mut line = format!("fx {effect_name}:");
    for component in &definition.components {
        line.push_str(&format!(
            " [{:?} life {}-{} size {}-{} -> {}-{} {}]",
            component.kind,
            component.life.minimum,
            component.life.maximum,
            component.size.start.minimum,
            component.size.start.maximum,
            component.size.end.minimum,
            component.size.end.maximum,
            component.shaders.join(","),
        ));
    }
    crate::log::progress(format_args!("{line}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The flag is process-wide, so one test covers every transition.
    #[test]
    fn the_flag_follows_the_cvars_and_resets_without_a_console() {
        let directory = tempfile::tempdir().unwrap();
        let mut console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();

        assert!(!sync(Some(&console)));
        assert!(!enabled(), "off by default");

        assert!(console.set_cvar("fx_debug", "1"));
        assert!(!sync(Some(&console)), "cg_debugMissiles is a separate cvar");
        assert!(enabled());

        assert!(console.set_cvar("cg_debugMissiles", "1"));
        assert!(sync(Some(&console)));
        assert!(enabled());

        assert!(console.set_cvar("fx_debug", "0"));
        assert!(sync(Some(&console)));
        assert!(!enabled(), "setting the cvar to 0 turns the flag off");

        assert!(console.set_cvar("fx_debug", "1"));
        sync(Some(&console));
        assert!(enabled());
        assert!(!sync(None), "no console means both are off");
        assert!(!enabled(), "the flag does not outlive the console");
    }
}
