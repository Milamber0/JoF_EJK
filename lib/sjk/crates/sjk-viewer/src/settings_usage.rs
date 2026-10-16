//! Anonymous settings usage (`docs/client.md`, "Settings usage"): which settings
//! players change, from what to what, and what they search for in Settings, sent
//! to the SJK hub so the team can see which settings matter and what players
//! look for. Nothing names the player: no identity key, no signature, no name or
//! server, only the build and a random id drawn once per run
//! ([`sjk_identity::usage`]). A text setting's value (an address, a key) is never
//! sent, only that it changed. `cl_shareUsage` 0 sends nothing and keeps nothing.
//!
//! Settings records into a queue as the player acts; the queue goes to the hub
//! when Settings closes, on a thread of its own, and what the hub cannot take is
//! dropped: these are statistics, not reports.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sjk_identity::usage::{BATCH_MAX, Event, Mode};

/// The setting that shares settings usage (on by default).
pub(crate) const CVAR: &str = "cl_shareUsage";
/// Most events kept waiting; older ones go first.
const QUEUE_MAX: usize = 4 * BATCH_MAX;
/// A change of the same setting this soon after the last one is the same
/// change going on (a slider dragged, a key held): it updates that event.
const SAME_CHANGE: Duration = Duration::from_secs(4);

struct Queue {
    events: Vec<Event>,
    /// When the last change was recorded, to join a change going on to it.
    last_change: Option<Instant>,
    /// This run's random id, drawn at the first send.
    session: Option<String>,
}

impl Queue {
    const fn new() -> Self {
        Self {
            events: Vec::new(),
            last_change: None,
            session: None,
        }
    }
}

#[cfg(not(test))]
static QUEUE: Mutex<Queue> = Mutex::new(Queue::new());

fn lock() -> MutexGuard<'static, Queue> {
    #[cfg(not(test))]
    let queue = &QUEUE;
    // Each test thread has its own queue: tests that close Settings flush it.
    #[cfg(test)]
    let queue = TEST_QUEUE.with(|queue| *queue);
    queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
thread_local! {
    static TEST_QUEUE: &'static Mutex<Queue> = Box::leak(Box::new(Mutex::new(Queue::new())));
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_secs())
}

/// Whether the player shares settings usage.
pub(crate) fn enabled(console: &crate::console::ViewerConsole) -> bool {
    console.bool_cvar(CVAR).unwrap_or(true)
}

/// Record that `setting`, shown under `group`, changed from `from` to `to` in
/// `mode`, while `search` was typed if one was. A change of the same setting
/// just after the last one updates it (back where it started, it is dropped).
pub(crate) fn change(
    mode: Mode,
    group: &str,
    setting: &str,
    from: &str,
    to: &str,
    search: Option<&str>,
) {
    let mut queue = lock();
    let joined = queue
        .last_change
        .is_some_and(|last| last.elapsed() < SAME_CHANGE);
    queue.last_change = Some(Instant::now());
    if joined
        && let Some(Event::Change {
            setting: last,
            from: first,
            to: last_to,
            ..
        }) = queue.events.last_mut()
        && last == setting
    {
        if first == to {
            queue.events.pop();
        } else {
            to.clone_into(last_to);
        }
        return;
    }
    push(
        &mut queue,
        Event::Change {
            at: now(),
            mode,
            group: group.to_owned(),
            setting: setting.to_owned(),
            from: from.to_owned(),
            to: to.to_owned(),
            search: search.map(str::to_owned),
        },
    );
}

/// Record a search for `query` in `mode` that found `results` settings.
pub(crate) fn search(mode: Mode, query: &str, results: usize) {
    push(
        &mut lock(),
        Event::Search {
            at: now(),
            mode,
            query: query.to_owned(),
            results: u32::try_from(results).unwrap_or(u32::MAX),
        },
    );
}

fn push(queue: &mut Queue, event: Event) {
    if queue.events.len() >= QUEUE_MAX {
        queue.events.remove(0);
    }
    queue.events.push(event);
}

/// Send what was recorded to the hub, on a thread of its own, when the player
/// shares settings usage and a hub is set; otherwise forget it.
pub(crate) fn flush(console: &crate::console::ViewerConsole) {
    let url = console
        .text_value("cl_hubUrl")
        .unwrap_or_default()
        .trim()
        .to_owned();
    let mut queue = lock();
    if !enabled(console) || url.is_empty() {
        queue.events.clear();
        return;
    }
    if queue.events.is_empty() {
        return;
    }
    let events: Vec<Event> = queue.events.drain(..).collect();
    queue.last_change = None;
    let session = queue
        .session
        .get_or_insert_with(sjk_identity::usage::session_id)
        .clone();
    drop(queue);
    // Tests never reach the hub.
    if cfg!(test) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("settings-usage".to_owned())
        .spawn(move || send(&url, &session, &events));
    if let Err(error) = spawned {
        crate::log::progress(format_args!("settings usage: not sent ({error})"));
    }
}

fn send(url: &str, session: &str, events: &[Event]) {
    let agent = format!("SJK/{}", crate::build_info::VERSION);
    let mut hub = match sjk_identity::HttpHub::new(url, &agent) {
        Ok(hub) => hub,
        Err(error) => {
            crate::log::progress(format_args!("settings usage: not sent ({error})"));
            return;
        }
    };
    use sjk_identity::Hub as _;
    for batch in events.chunks(BATCH_MAX) {
        let body = sjk_identity::usage::body(crate::build_info::VERSION, session, batch);
        match hub.settings_usage(&body) {
            Ok(stored) => {
                crate::log::progress(format_args!("settings usage: sent {stored} events"));
            }
            Err(error) => {
                crate::log::progress(format_args!("settings usage: not sent ({error})"));
                return;
            }
        }
    }
}

/// The events waiting, for tests.
#[cfg(test)]
pub(crate) fn take_for_test() -> Vec<Event> {
    let mut queue = lock();
    queue.last_change = None;
    queue.events.drain(..).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_going_on_updates_its_event_and_one_undone_is_dropped() {
        change(Mode::Simple, "Display", "cg_fov", "95", "100", None);
        change(Mode::Simple, "Display", "cg_fov", "100", "105", None);
        search(Mode::Simple, "fov", 2);
        change(
            Mode::Simple,
            "Display",
            "r_gamma",
            "1",
            "1.1",
            Some("bright"),
        );
        change(
            Mode::Simple,
            "Display",
            "r_gamma",
            "1.1",
            "1",
            Some("bright"),
        );
        let events = take_for_test();
        assert_eq!(events.len(), 2, "{events:?}");
        assert!(matches!(
            &events[0],
            Event::Change { setting, from, to, .. } if setting == "cg_fov" && from == "95" && to == "105"
        ));
        assert!(matches!(&events[1], Event::Search { query, results: 2, .. } if query == "fov"));
    }

    #[test]
    fn sharing_off_keeps_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let mut console =
            crate::console::ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        assert!(enabled(&console), "on by default");
        console.set_cvar(CVAR, "0");
        search(Mode::Advanced, "glow", 3);
        flush(&console);
        assert!(take_for_test().is_empty());
    }
}
