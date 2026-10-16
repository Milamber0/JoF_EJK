//! Crash reports (`PROTOCOL.md` in Sol-Vulpes/SJK-hub, "Crash reports"): what the game
//! keeps when it crashes, the folder the reports wait in, and sending them to the hub.
//!
//! The game writes a report into the folder as it goes down (`crash_report.rs` in the
//! viewer), and the service sends what waits there once the player is registered
//! ([`crate::Service::send_crashes`]), usually at the next start. Text is cut and
//! cleaned to the hub's rules here, so the hub never refuses a report for its size or
//! a stray character.

use crate::hub::{Hub, HubError};
use crate::keys::Identity;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Longest message, in characters.
pub const MESSAGE_MAX: usize = 4_000;
/// Longest backtrace, in characters.
pub const BACKTRACE_MAX: usize = 16_000;
/// Longest log (the last console and diagnostic lines), in characters.
pub const LOG_MAX: usize = 16_000;
/// Longest panic location (`file:line:column`), in characters.
pub const LOCATION_MAX: usize = 256;
/// Longest thread name, in characters.
pub const THREAD_MAX: usize = 64;
/// Longest operating system and graphics adapter descriptions, in ASCII characters.
pub const OS_MAX: usize = 64;
/// Longest graphics adapter description.
pub const GPU_MAX: usize = 160;
/// Longest build and map names.
pub const BUILD_MAX: usize = 32;
/// Longest map name.
pub const MAP_MAX: usize = 64;
/// Most bytes a report's body may take; the hub reads at most 48 KiB.
const BODY_BUDGET: usize = 45_000;
/// Reports waiting to be sent; a newer one makes room by dropping the oldest.
pub const PENDING_MAX: usize = 20;
/// Reports already sent that stay in the folder for the player to read.
pub const SENT_KEPT: usize = 10;
/// A report this old (seconds) is dropped unsent.
pub const PENDING_AGE_MAX: u64 = 14 * 86_400;

/// What ended the game.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A Rust panic: a bug the code caught itself, with its message and place.
    #[default]
    Panic,
    /// A fatal error the game reported before closing (no graphics adapter, ...).
    Error,
    /// The game stopped without saying why (killed, an abort, a stack overflow, a
    /// driver taking the process down): found at the next start.
    Unclean,
}

impl Kind {
    /// The hub's code for it.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Panic => "panic",
            Self::Error => "error",
            Self::Unclean => "unclean",
        }
    }
}

/// One crash, as the game keeps it on disk and the hub takes it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct CrashReport {
    /// What ended the game.
    pub kind: Kind,
    /// The client's build.
    pub build: String,
    /// The operating system and architecture.
    pub os: String,
    /// The graphics adapter, its backend and driver.
    pub gpu: String,
    /// The thread that panicked.
    pub thread: String,
    /// Where it panicked: `file:line:column`.
    pub location: String,
    /// The panic's or the error's message.
    pub message: String,
    /// The backtrace, as far as the build can tell.
    pub backtrace: String,
    /// The last console and diagnostic lines before the crash (no chat).
    pub log: String,
    /// The map, `maps/<name>.bsp`, or empty.
    pub map: String,
    /// The game server's `ip:port`, or empty.
    pub server: String,
    /// The in-game name the player wore; empty takes the one the service knows.
    pub name: String,
    /// Seconds the game had run.
    pub uptime: u64,
    /// When it crashed, Unix seconds by this PC's clock.
    pub crashed: u64,
    /// The game's process id. Never sent: it tells the start after a crash that the
    /// unclean-exit marker it finds belongs to a process that already wrote a report.
    pub pid: u32,
}

impl CrashReport {
    /// The report within the hub's rules: every text cut to its length and cleaned
    /// of what the hub refuses. A message and a backtrace keep their start, the log
    /// its end (the newest lines).
    pub fn fitted(&self) -> Self {
        let mut fitted = Self {
            kind: self.kind,
            build: field(&self.build, BUILD_MAX),
            os: ascii(&self.os, OS_MAX),
            gpu: ascii(&self.gpu, GPU_MAX),
            thread: one_line(&self.thread, THREAD_MAX),
            location: one_line(&self.location, LOCATION_MAX),
            message: text(&self.message, MESSAGE_MAX, Keep::Start),
            backtrace: text(&self.backtrace, BACKTRACE_MAX, Keep::Start),
            log: text(&self.log, LOG_MAX, Keep::End),
            map: field(&self.map, MAP_MAX),
            server: self.server.trim().to_owned(),
            name: self.name.clone(),
            uptime: self.uptime.min(31_536_000),
            crashed: self.crashed,
            pid: self.pid,
        };
        // Characters outside ASCII and escaped newlines take more than a byte each:
        // halve the log, then the backtrace, until the body fits the hub's limit.
        while fitted.body().to_string().len() > BODY_BUDGET {
            if !fitted.log.is_empty() {
                let keep = fitted.log.chars().count() / 2;
                fitted.log = text(&fitted.log, keep, Keep::End);
            } else if !fitted.backtrace.is_empty() {
                let keep = fitted.backtrace.chars().count() / 2;
                fitted.backtrace = text(&fitted.backtrace, keep, Keep::Start);
            } else {
                fitted.message = text(&fitted.message, MESSAGE_MAX / 4, Keep::Start);
                break;
            }
        }
        fitted
    }

    /// The body `POST /v1/crash` takes, as is (call it on a [`CrashReport::fitted`]
    /// report). The process id stays on this PC; an empty name is left out.
    pub fn body(&self) -> serde_json::Value {
        let mut body = serde_json::json!({
            "kind": self.kind.code(),
            "build": self.build,
            "os": self.os,
            "gpu": self.gpu,
            "thread": self.thread,
            "location": self.location,
            "message": self.message,
            "backtrace": self.backtrace,
            "log": self.log,
            "map": self.map,
            "server": self.server,
            "uptime": self.uptime,
            "crashed": self.crashed,
        });
        if !self.name.is_empty() {
            body["name"] = serde_json::json!(self.name);
        }
        body
    }

    /// The report as plain text, for the player to read or pass on.
    pub fn readable(&self) -> String {
        let mut out = format!(
            "SJK crash report ({})\nBuild: {}\nSystem: {}\nGraphics: {}\nUptime: {} s\n",
            self.kind.code(),
            self.build,
            self.os,
            self.gpu,
            self.uptime
        );
        for (label, value) in [
            ("Map", &self.map),
            ("Server", &self.server),
            ("Thread", &self.thread),
            ("Location", &self.location),
        ] {
            if !value.is_empty() {
                out.push_str(&format!("{label}: {value}\n"));
            }
        }
        for (label, value) in [
            ("Message", &self.message),
            ("Backtrace", &self.backtrace),
            ("Last lines", &self.log),
        ] {
            if !value.is_empty() {
                out.push_str(&format!("\n{label}:\n{}\n", value.trim_end()));
            }
        }
        out
    }
}

/// Which end of a cut text stays.
#[derive(Clone, Copy)]
enum Keep {
    Start,
    End,
}

/// Bidirectional overrides and isolates and the byte order mark: the hub refuses them.
fn hidden(c: char) -> bool {
    matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
}

/// Multi-line text: `\r\n` as `\n`, no control character but `\n` and `\t`, at most
/// `max` characters. A text cut at its start drops the partial first line.
fn text(raw: &str, max: usize, keep: Keep) -> String {
    let clean: Vec<char> = raw
        .replace("\r\n", "\n")
        .chars()
        .filter(|&c| (!c.is_control() || c == '\n' || c == '\t') && !hidden(c))
        .collect();
    if clean.len() <= max {
        return clean.into_iter().collect();
    }
    match keep {
        Keep::Start => clean[..max].iter().collect(),
        Keep::End => {
            let tail: String = clean[clean.len() - max..].iter().collect();
            match tail.split_once('\n') {
                Some((_, rest)) if !rest.is_empty() => rest.to_owned(),
                _ => tail,
            }
        }
    }
}

/// One line of text, no control character at all, at most `max` characters.
fn one_line(raw: &str, max: usize) -> String {
    raw.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .filter(|&c| !hidden(c))
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Printable ASCII only, at most `max` characters (an adapter's `(TM)` sign goes).
fn ascii(raw: &str, max: usize) -> String {
    raw.chars()
        .filter(|c| (' '..='~').contains(c))
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// The bug reports' field alphabet: ASCII letters, digits, `_ - . /` and spaces.
fn field(raw: &str, max: usize) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric() || "_-./ ".contains(*c))
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// `home` (the player's own folder) replaced by `%USERPROFILE%` in `text`, so a
/// path in a message does not carry the Windows account name.
pub fn without_home(text: &str, home: &str) -> String {
    let home = home.trim_end_matches(['\\', '/']);
    if home.len() < 4 {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let lower_home = home.to_lowercase();
    while let Some(index) = find_ignoring_case(rest, &lower_home) {
        out.push_str(&rest[..index]);
        out.push_str("%USERPROFILE%");
        rest = &rest[index + home.len()..];
    }
    out.push_str(rest);
    out
}

/// Where `needle` (already lower case, ASCII case only) starts in `haystack`.
fn find_ignoring_case(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .char_indices()
        .map(|(index, _)| index)
        .find(|&index| {
            haystack
                .get(index..index + needle.len())
                .is_some_and(|part| part.to_lowercase() == needle)
        })
}

/// The folder reports wait in, inside the crash folder.
pub fn pending_dir(crash_dir: &Path) -> PathBuf {
    crash_dir.join("pending")
}

/// The folder sent reports stay in, inside the crash folder.
pub fn sent_dir(crash_dir: &Path) -> PathBuf {
    crash_dir.join("sent")
}

/// Write `report` to wait in `crash_dir` until it is sent: `pending/<time>-<pid>-<n>.json`,
/// through a temporary file so a half-written report never waits there. The oldest
/// make room past [`PENDING_MAX`].
pub fn write_pending(crash_dir: &Path, report: &CrashReport) -> std::io::Result<PathBuf> {
    let dir = pending_dir(crash_dir);
    std::fs::create_dir_all(&dir)?;
    let json = serde_json::to_vec_pretty(report).map_err(std::io::Error::other)?;
    let mut serial = 0;
    let path = loop {
        let path = dir.join(format!(
            "{:012}-{}-{serial}.json",
            report.crashed, report.pid
        ));
        if !path.exists() {
            break path;
        }
        serial += 1;
    };
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, json)?;
    std::fs::rename(&temporary, &path)?;
    trim(&dir, PENDING_MAX);
    Ok(path)
}

/// The reports waiting in `crash_dir`, oldest first.
pub fn pending(crash_dir: &Path) -> Vec<PathBuf> {
    reports_in(&pending_dir(crash_dir))
}

/// Whether a report from process `pid` waits or was sent already.
pub fn written_by(crash_dir: &Path, pid: u32) -> bool {
    let marker = format!("-{pid}-");
    [pending_dir(crash_dir), sent_dir(crash_dir)]
        .iter()
        .flat_map(|dir| reports_in(dir))
        .any(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(&marker))
        })
}

/// The `.json` files in `dir`, by name (oldest first: names start with the time).
fn reports_in(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    paths.sort();
    paths
}

/// Delete the oldest reports in `dir` past `keep`.
fn trim(dir: &Path, keep: usize) {
    let paths = reports_in(dir);
    for path in &paths[..paths.len().saturating_sub(keep)] {
        let _ = std::fs::remove_file(path);
    }
}

/// What one round of [`send_pending`] did.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Round {
    /// The hub's numbers for the reports it took.
    pub sent: Vec<i64>,
    /// Reports dropped: too old, unreadable, or refused for good.
    pub dropped: usize,
    /// Reports still waiting: the hub could not take them now.
    pub waiting: usize,
    /// Why the waiting ones wait.
    pub problem: Option<String>,
}

/// Send the reports waiting in `crash_dir`, oldest first, signed by `identity`; one
/// without a name takes `name`. A report the hub took (or already had: 409) moves
/// to the sent folder; one it refused for good is dropped; at the first that it
/// cannot take now (no answer, a limit, a hub without crash reports yet) the round
/// stops and the rest wait for the next.
pub fn send_pending(
    hub: &mut dyn Hub,
    identity: &Identity,
    crash_dir: &Path,
    name: &str,
    now: u64,
) -> Round {
    let mut round = Round::default();
    let paths = pending(crash_dir);
    for (index, path) in paths.iter().enumerate() {
        let report = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<CrashReport>(&bytes).ok());
        let Some(mut report) = report
            .filter(|report| report.crashed <= now && now - report.crashed <= PENDING_AGE_MAX)
        else {
            let _ = std::fs::remove_file(path);
            round.dropped += 1;
            continue;
        };
        if report.name.is_empty() {
            name.clone_into(&mut report.name);
        }
        let mut answer = hub.crash(identity, &report.fitted());
        // A name the hub will not take must not cost the report: send it without.
        if matches!(&answer, Err(HubError::Rejected { code, .. }) if code == "bad_name") {
            report.name.clear();
            answer = hub.crash(identity, &report.fitted());
        }
        match answer {
            Ok(id) => {
                round.sent.push(id);
                keep_sent(crash_dir, path);
            }
            Err(HubError::Rejected { status: 409, .. }) => keep_sent(crash_dir, path),
            Err(HubError::Rejected { status, code, .. })
                if matches!(status, 400 | 401 | 403 | 413) && code != "not_registered" =>
            {
                let _ = std::fs::remove_file(path);
                round.dropped += 1;
            }
            Err(error) => {
                round.waiting = paths.len() - index;
                round.problem = Some(error.to_string());
                break;
            }
        }
    }
    round
}

/// Move a sent report into the sent folder, which keeps the newest [`SENT_KEPT`].
fn keep_sent(crash_dir: &Path, path: &Path) {
    let dir = sent_dir(crash_dir);
    let moved = std::fs::create_dir_all(&dir).is_ok()
        && path
            .file_name()
            .is_some_and(|name| std::fs::rename(path, dir.join(name)).is_ok());
    if !moved {
        let _ = std::fs::remove_file(path);
    }
    trim(&dir, SENT_KEPT);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(crashed: u64, pid: u32) -> CrashReport {
        CrashReport {
            kind: Kind::Panic,
            build: "2026.1011.1".to_owned(),
            message: "index out of bounds: the len is 3 but the index is 7".to_owned(),
            location: "crates/sjk-viewer/src/main.rs:12:5".to_owned(),
            crashed,
            pid,
            ..CrashReport::default()
        }
    }

    #[test]
    fn text_keeps_lines_and_drops_what_the_hub_refuses() {
        assert_eq!(
            text("a\r\nb\tc\u{0}d\u{202e}e\u{1b}[0m", 100, Keep::Start),
            "a\nb\tcde[0m"
        );
        // A log cut at its start loses the partial first line.
        let log = "first line\nsecond line\nthird line";
        assert_eq!(text(log, 15, Keep::End), "third line");
        assert_eq!(text(log, 8, Keep::Start), "first li");
        assert_eq!(one_line("main\nthread\u{0}", 64), "main thread");
        assert_eq!(
            ascii("NVIDIA GeForce\u{2122} RTX", 160),
            "NVIDIA GeForce RTX"
        );
        assert_eq!(field("maps/mp/ffa3.bsp<", 64), "maps/mp/ffa3.bsp");
    }

    #[test]
    fn a_fitted_body_stays_under_the_hubs_limit() {
        let mut big = report(1, 1);
        big.log = "é\n".repeat(40_000);
        big.backtrace = "frame\n".repeat(10_000);
        big.message = "m".repeat(9_000);
        let fitted = big.fitted();
        assert!(fitted.message.chars().count() <= MESSAGE_MAX);
        assert!(fitted.backtrace.chars().count() <= BACKTRACE_MAX);
        assert!(fitted.body().to_string().len() <= BODY_BUDGET);
        assert!(fitted.log.ends_with("é\n"), "the newest lines stay");
        assert!(fitted.body().get("pid").is_none());
        assert!(fitted.body().get("name").is_none());
    }

    #[test]
    fn the_home_folder_is_hidden_whatever_its_case() {
        assert_eq!(
            without_home(
                r"cannot open C:\Users\Bob\AppData\SJK and c:\users\bob\x",
                r"C:\Users\Bob"
            ),
            r"cannot open %USERPROFILE%\AppData\SJK and %USERPROFILE%\x"
        );
        assert_eq!(without_home("nothing here", ""), "nothing here");
    }

    #[test]
    fn pending_reports_are_kept_oldest_first_and_capped() {
        let dir = tempfile::tempdir().unwrap();
        for crashed in 0..(PENDING_MAX as u64 + 3) {
            write_pending(dir.path(), &report(crashed + 100, 7)).unwrap();
        }
        let waiting = pending(dir.path());
        assert_eq!(waiting.len(), PENDING_MAX);
        assert!(waiting[0].to_string_lossy().contains("000000000103-7-0"));
        assert!(written_by(dir.path(), 7));
        assert!(!written_by(dir.path(), 8));
        let read: CrashReport =
            serde_json::from_slice(&std::fs::read(&waiting[0]).unwrap()).unwrap();
        assert_eq!(read, report(103, 7));
    }

    struct FakeHub {
        answers: Vec<Result<i64, HubError>>,
        names: Vec<String>,
    }

    impl Hub for FakeHub {
        fn register(&mut self, _: &Identity, _: Option<&str>) -> Result<crate::Profile, HubError> {
            Err(HubError::Network("unused".to_owned()))
        }
        fn set_bio(&mut self, _: &Identity, _: &str) -> Result<crate::Profile, HubError> {
            Err(HubError::Network("unused".to_owned()))
        }
        fn profile(&mut self, _: &str) -> Result<crate::Profile, HubError> {
            Err(HubError::Network("unused".to_owned()))
        }
        fn claim(
            &mut self,
            _: &Identity,
            _: &str,
            _: u8,
            _: &str,
            _: bool,
        ) -> Result<(), HubError> {
            Ok(())
        }
        fn release(&mut self, _: &Identity, _: &str) -> Result<(), HubError> {
            Ok(())
        }
        fn presence(&mut self, _: &str) -> Result<Vec<crate::Presence>, HubError> {
            Ok(Vec::new())
        }
        fn crash(&mut self, _: &Identity, report: &CrashReport) -> Result<i64, HubError> {
            self.names.push(report.name.clone());
            self.answers.remove(0)
        }
    }

    fn rejected(status: u16, code: &str) -> HubError {
        HubError::Rejected {
            status,
            code: code.to_owned(),
            message: code.to_owned(),
        }
    }

    #[test]
    fn a_round_sends_drops_and_stops_at_what_must_wait() {
        let dir = tempfile::tempdir().unwrap();
        let identity = Identity::generate().unwrap();
        let now = 2_000_000;
        // Too old: dropped unsent.
        write_pending(dir.path(), &report(now - PENDING_AGE_MAX - 1, 1)).unwrap();
        for pid in 2..=6 {
            write_pending(dir.path(), &report(now - 100 + u64::from(pid), pid)).unwrap();
        }
        std::fs::write(pending_dir(dir.path()).join("000000000001-9-0.json"), "{").unwrap();
        let mut hub = FakeHub {
            answers: vec![
                Err(rejected(400, "bad_name")),
                Ok(41),
                Err(rejected(409, "crash_duplicate")),
                Err(rejected(400, "crash_characters")),
                Err(rejected(429, "crash_quota")),
            ],
            names: Vec::new(),
        };
        let round = send_pending(&mut hub, &identity, dir.path(), "Sol", now);
        assert_eq!(round.sent, vec![41]);
        assert_eq!(
            round.dropped, 3,
            "the old one, the broken file, the refused one"
        );
        assert_eq!(round.waiting, 2);
        assert!(round.problem.unwrap().contains("crash_quota"));
        assert_eq!(hub.names, vec!["Sol", "", "Sol", "Sol", "Sol"]);
        assert_eq!(pending(dir.path()).len(), 2);
        assert_eq!(reports_in(&sent_dir(dir.path())).len(), 2);
    }
}
