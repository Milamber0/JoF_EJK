//! Crash reports (`docs/client.md`, "Crash reports"): what the game keeps when it
//! goes down, sent to the SJK hub at the next start while `cl_crashReports` is on.
//!
//! Three ways a session can end badly, each leaving a report in `crashes/` beside
//! the configuration (`sjk_identity::crash` owns the format and the sending):
//!
//! - a panic, on any thread: the hook [`install`] sets writes it with its message,
//!   place, backtrace and the last lines of the trail, as the panic happens;
//! - a fatal error `main` reports before closing ([`fatal_error`]);
//! - an exit that said nothing (killed, an abort, a stack overflow, a driver taking
//!   the process down): each session keeps a marker, `crashes/running-<pid>.json`,
//!   written again every 30 s with where the player was and the last lines; a clean
//!   exit removes it ([`clean_exit`]). The next start that finds one from a process
//!   that wrote no report turns it into an `unclean` report.
//!
//! The trail is the console's lines as they are kept (chat left out) and the
//! diagnostics `log::progress` prints. Every report is also written as plain text to
//! `crashes/last-crash.txt` for the player to read or pass on, sent or not.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock, TryLockError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sjk_identity::crash::{self, CrashReport, Kind};

/// The setting that sends reports to the hub.
pub(crate) const CVAR: &str = "cl_crashReports";
/// Lines the trail keeps, and the longest kept of each.
const TRAIL_LINES: usize = 150;
const TRAIL_LINE_MAX: usize = 400;
/// Lines of the trail the marker carries for an unclean exit.
const MARKER_LINES: usize = 60;
/// How often the marker is written again.
const MARKER_EVERY: Duration = Duration::from_secs(30);
/// Panic reports one session writes at most (a panic caught and repeated every
/// frame must not fill the folder).
const SESSION_MAX: usize = 3;
/// A marker written this recently may be a running game's (where its file cannot
/// tell, outside Windows).
#[cfg(not(windows))]
const MARKER_FRESH: Duration = Duration::from_secs(90);

static STARTED: OnceLock<Instant> = OnceLock::new();
static TRAIL: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
static STATE: Mutex<State> = Mutex::new(State::new());

thread_local! {
    /// Set while a chat line is kept, so the trail leaves it out.
    static PRIVATE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// What a report says about the session, kept up to date as the game runs.
struct State {
    /// The folder beside the configuration, once known.
    dir: Option<PathBuf>,
    /// Whether reports are written to be sent (`cl_crashReports`).
    send: bool,
    gpu: String,
    map: String,
    server: String,
    /// Places of the panics this session reported.
    reported: Vec<String>,
    /// This session's marker, held open.
    marker: Option<File>,
    marker_written: Option<Instant>,
    /// Whether the markers of earlier sessions were looked at.
    looked_back: bool,
    /// The hub's count of reports taken, as last seen.
    sent_seen: u64,
}

impl State {
    const fn new() -> Self {
        Self {
            dir: None,
            send: true,
            gpu: String::new(),
            map: String::new(),
            server: String::new(),
            reported: Vec::new(),
            marker: None,
            marker_written: None,
            looked_back: false,
            sent_seen: 0,
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The lock if it can be had at once: a panic inside a holder of it must not hang.
fn try_lock<T>(mutex: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    match mutex.try_lock() {
        Ok(guard) => Some(guard),
        Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

/// Start the session's clock and add the panic hook (after the standard one, which
/// still prints the panic to stderr).
pub(crate) fn install() {
    let _ = STARTED.set(Instant::now());
    let standard = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        standard(info);
        on_panic(info);
    }));
}

/// Keep `line` in the trail (the console's line tap and `log::progress`).
pub(crate) fn trail(line: &str) {
    if PRIVATE.with(std::cell::Cell::get) {
        return;
    }
    let line = without_colours(line);
    let line = match line.char_indices().nth(TRAIL_LINE_MAX) {
        Some((cut, _)) => &line[..cut],
        None => &line,
    };
    let seconds = STARTED
        .get()
        .map_or(0.0, |started| started.elapsed().as_secs_f64());
    let line = format!("[{seconds:9.1}] {line}");
    crate::session_log::write(&line);
    if let Some(mut trail) = try_lock(&TRAIL) {
        if trail.len() == TRAIL_LINES {
            trail.pop_front();
        }
        trail.push_back(line);
    }
}

/// Run `keep` (which keeps a chat line) without the trail seeing what it keeps.
pub(crate) fn privately<R>(keep: impl FnOnce() -> R) -> R {
    PRIVATE.with(|private| private.set(true));
    let result = keep();
    PRIVATE.with(|private| private.set(false));
    result
}

/// Text without the game's `^N` colour codes.
fn without_colours(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^' && chars.peek().is_some_and(char::is_ascii_digit) {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

/// The last `lines` lines of the trail, oldest first.
fn trail_tail(lines: usize) -> String {
    try_lock(&TRAIL).map_or_else(String::new, |trail| {
        let skip = trail.len().saturating_sub(lines);
        trail
            .iter()
            .skip(skip)
            .fold(String::new(), |mut out, line| {
                out.push_str(line);
                out.push('\n');
                out
            })
    })
}

/// The folder reports go to: `crashes` beside the configuration. Starts this
/// session's marker there, and the session log beside it with the lines so far.
pub(crate) fn set_directory(config_directory: &Path) {
    // A hosting engine keeps its own crash reports; SJK's stay out of its profile.
    if crate::host::hosted() {
        return;
    }
    if cfg!(test) {
        return;
    }
    let earlier: Vec<String> =
        try_lock(&TRAIL).map_or_else(Vec::new, |trail| trail.iter().cloned().collect());
    crate::session_log::start(config_directory, &earlier);
    let dir = config_directory.join("crashes");
    let mut state = lock(&STATE);
    if state.dir.is_some() {
        return;
    }
    state.marker = open_marker(&dir);
    state.dir = Some(dir);
}

/// The graphics adapter, as the device was made on it.
pub(crate) fn set_gpu(info: &wgpu::AdapterInfo) {
    let mut gpu = format!("{} ({:?}", info.name, info.backend);
    for part in [&info.driver, &info.driver_info] {
        if !part.is_empty() {
            gpu.push_str(", ");
            gpu.push_str(part);
        }
    }
    gpu.push(')');
    lock(&STATE).gpu = gpu;
}

/// What a log request sends (`session_log::gather`), with what the session knows.
pub(crate) fn log_upload() -> sjk_identity::LogUpload {
    let context = {
        let state = lock(&STATE);
        crate::session_log::Context {
            gpu: state.gpu.clone(),
            os: os(),
            map: state.map.clone(),
            server: state.server.clone(),
        }
    };
    crate::session_log::gather(&context, &home())
}

/// The operating system, as far as the standard library tells.
fn os() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn uptime() -> u64 {
    STARTED
        .get()
        .map_or(0, |started| started.elapsed().as_secs())
}

/// The player's own folder, hidden from what a report carries.
fn home() -> String {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default()
}

/// A report of `kind` about this session, from what [`State`] knows.
fn session_report(state: Option<&State>, kind: Kind, log_lines: usize) -> CrashReport {
    CrashReport {
        kind,
        build: crate::build_info::VERSION.to_owned(),
        os: os(),
        gpu: state.map_or_else(String::new, |state| state.gpu.clone()),
        map: state.map_or_else(String::new, |state| state.map.clone()),
        server: state.map_or_else(String::new, |state| state.server.clone()),
        log: trail_tail(log_lines),
        uptime: uptime(),
        crashed: unix_now(),
        pid: std::process::id(),
        ..CrashReport::default()
    }
}

/// The folder to write into: the one set, or the per-user one when the game went
/// down before it knew its configuration.
fn report_dir(state: Option<&State>) -> Option<PathBuf> {
    state.and_then(|state| state.dir.clone()).or_else(|| {
        crate::platform::user_config_file()
            .ok()
            .and_then(|file| file.parent().map(|parent| parent.join("crashes")))
    })
}

/// Keep `report` in `dir`: as plain text for the player, and to be sent if `send`.
fn keep(dir: &Path, report: &CrashReport, send: bool) {
    let home = home();
    let mut report = report.clone();
    for text in [&mut report.message, &mut report.backtrace, &mut report.log] {
        *text = crash::without_home(text, &home);
    }
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(dir.join("last-crash.txt"), report.readable());
    if send {
        match crash::write_pending(dir, &report) {
            Ok(_) => crate::log::progress(format_args!(
                "crash report kept to send at the next start ({})",
                dir.display()
            )),
            Err(error) => crate::log::progress(format_args!("crash report not kept: {error}")),
        }
    }
}

fn on_panic(info: &std::panic::PanicHookInfo<'_>) {
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_owned());
    let location = info
        .location()
        .map(|place| format!("{}:{}:{}", place.file(), place.line(), place.column()))
        .unwrap_or_default();
    let mut state = try_lock(&STATE);
    if let Some(state) = state.as_mut() {
        if state.reported.len() >= SESSION_MAX || state.reported.contains(&location) {
            return;
        }
        state.reported.push(location.clone());
    }
    let mut report = session_report(state.as_deref(), Kind::Panic, TRAIL_LINES);
    report.message = message;
    report.location = location;
    report.thread = std::thread::current()
        .name()
        .unwrap_or("unnamed")
        .to_owned();
    report.backtrace = std::backtrace::Backtrace::force_capture().to_string();
    let send = state.as_ref().is_none_or(|state| state.send);
    if let Some(dir) = report_dir(state.as_deref()) {
        keep(&dir, &report, send);
    }
}

/// The fatal error `main` reports before closing: kept as a report, and the
/// session ends cleanly otherwise.
pub(crate) fn fatal_error(message: &str) {
    let state = lock(&STATE);
    let mut report = session_report(Some(&state), Kind::Error, TRAIL_LINES);
    report.message = message.to_owned();
    if let Some(dir) = report_dir(Some(&state)) {
        keep(&dir, &report, state.send);
    }
    drop(state);
    clean_exit();
}

/// The session ends as it should: its marker goes.
pub(crate) fn clean_exit() {
    let mut state = lock(&STATE);
    if state.marker.take().is_some()
        && let Some(dir) = &state.dir
    {
        let _ = std::fs::remove_file(marker_path(dir, std::process::id()));
    }
}

fn marker_path(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!("running-{pid}.json"))
}

/// Open this session's marker. On Windows it is shared for reading only, so a later
/// start cannot remove it while this game runs: that tells a live game's marker
/// from a dead one's.
fn open_marker(dir: &Path) -> Option<File> {
    std::fs::create_dir_all(dir).ok()?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 1;
        options.share_mode(FILE_SHARE_READ);
    }
    options.open(marker_path(dir, std::process::id())).ok()
}

/// Write the marker again: an unclean-exit report as it would read now.
fn write_marker(state: &mut State) {
    let report = session_report(Some(state), Kind::Unclean, MARKER_LINES);
    let Some(file) = state.marker.as_mut() else {
        return;
    };
    let Ok(json) = serde_json::to_vec(&report) else {
        return;
    };
    let written = file
        .seek(SeekFrom::Start(0))
        .and_then(|_| file.set_len(0))
        .and_then(|()| file.write_all(&json));
    if written.is_err() {
        state.marker = None;
    }
}

/// Whether the marker at `path` belongs to a game that still runs.
fn marker_alive(path: &Path) -> bool {
    #[cfg(not(windows))]
    {
        std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .is_ok_and(|modified| modified.elapsed().is_ok_and(|age| age < MARKER_FRESH))
    }
    // A running game holds its marker open without sharing deletion.
    #[cfg(windows)]
    {
        let _ = path;
        false
    }
}

/// Turn the markers earlier sessions left into reports, once: what this start found.
fn look_back(dir: &Path, send: bool) -> usize {
    let own = std::process::id();
    let mut found = 0;
    let markers = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("running-") && name.ends_with(".json"))
        });
    for path in markers {
        let pid = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.strip_prefix("running-"))
            .and_then(|pid| pid.parse::<u32>().ok());
        if pid == Some(own) || marker_alive(&path) {
            continue;
        }
        let report = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<CrashReport>(&bytes).ok());
        // Removal fails on a running game's marker (Windows): leave it be.
        if std::fs::remove_file(&path).is_err() {
            continue;
        }
        let Some(mut report) = report else {
            continue;
        };
        found += 1;
        if crash::written_by(dir, report.pid) {
            continue;
        }
        report.kind = Kind::Unclean;
        report.message = "SJK stopped without reporting an error (killed, an abort, a stack \
                          overflow or the graphics driver)"
            .to_owned();
        keep(dir, &report, send);
    }
    found
}

impl crate::GpuState {
    /// Twice a second, with the identity's turn: keep what a report says about the
    /// session, write the marker every [`MARKER_EVERY`], look at what earlier
    /// sessions left once, and have the identity service send the waiting reports.
    pub(crate) fn follow_crash_reports(&mut self) {
        let Some(console) = self.console.as_ref() else {
            return;
        };
        let send = console.bool_cvar(CVAR) != Some(false);
        let map = if self.resident.exploring() {
            self.resident.map.clone()
        } else {
            self.world_load_map.clone()
        };
        let server = self
            .live_session
            .as_ref()
            .filter(|session| !session.is_local())
            .map(|session| session.server().to_string())
            .unwrap_or_default();
        let mut state = lock(&STATE);
        state.send = send;
        state.map = map;
        state.server = server;
        let Some(dir) = state.dir.clone() else {
            return;
        };
        if state
            .marker_written
            .is_none_or(|written| written.elapsed() >= MARKER_EVERY)
        {
            state.marker_written = Some(Instant::now());
            write_marker(&mut state);
        }
        let mut lines = Vec::new();
        if !state.looked_back {
            state.looked_back = true;
            let waiting = !crash::pending(&dir).is_empty();
            if look_back(&dir, send) > 0 || (send && waiting) {
                lines.push(if send {
                    "^3SJK closed unexpectedly last time.^7 A crash report goes to the SJK \
                     team (cl_crashReports 0 turns this off)."
                        .to_owned()
                } else {
                    format!(
                        "^3SJK closed unexpectedly last time.^7 What is known is in {}",
                        dir.join("last-crash.txt").display()
                    )
                });
            }
        }
        crate::player_identity::send_crashes(send.then(|| dir.clone()));
        if let Some((sent, note)) = crate::player_identity::crash_outcome()
            && sent > state.sent_seen
        {
            state.sent_seen = sent;
            crate::log::progress(format_args!("{note}"));
            lines.push("Crash report sent to the SJK team. Thank you.".to_owned());
        }
        drop(state);
        if let Some(console) = self.console.as_mut() {
            for line in lines {
                console.push_log(line);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_codes_leave_the_trail() {
        assert_eq!(without_colours("^1Sol^7 joined ^^ x^"), "Sol joined ^^ x^");
    }

    /// Write a marker as a session that died a while ago left it.
    fn dead_marker(dir: &Path, report: &CrashReport) {
        let path = marker_path(dir, report.pid);
        std::fs::write(&path, serde_json::to_vec(report).unwrap()).unwrap();
        let old = SystemTime::now() - Duration::from_secs(600);
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();
    }

    #[test]
    fn a_dead_sessions_marker_becomes_one_unclean_report() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path();
        let marker = CrashReport {
            kind: Kind::Unclean,
            build: "2026.1011.1".to_owned(),
            map: "maps/mp/ffa3.bsp".to_owned(),
            pid: 4242,
            crashed: unix_now(),
            ..CrashReport::default()
        };
        dead_marker(dir, &marker);
        assert_eq!(look_back(dir, true), 1);
        assert!(!marker_path(dir, 4242).exists());
        let waiting = crash::pending(dir);
        assert_eq!(waiting.len(), 1);
        let report: CrashReport =
            serde_json::from_slice(&std::fs::read(&waiting[0]).unwrap()).unwrap();
        assert_eq!(report.kind, Kind::Unclean);
        assert_eq!(report.map, "maps/mp/ffa3.bsp");
        assert!(dir.join("last-crash.txt").exists());
        // A process that wrote its own report leaves no second one.
        dead_marker(dir, &marker);
        assert_eq!(look_back(dir, true), 1);
        assert_eq!(crash::pending(dir).len(), 1);
    }

    #[test]
    fn with_reports_off_only_the_text_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let report = CrashReport {
            message: format!("cannot open {}\\x", home()),
            ..CrashReport::default()
        };
        keep(dir.path(), &report, false);
        assert!(crash::pending(dir.path()).is_empty());
        let text = std::fs::read_to_string(dir.path().join("last-crash.txt")).unwrap();
        if home().len() >= 4 {
            assert!(text.contains("%USERPROFILE%"), "{text}");
        }
    }
}
