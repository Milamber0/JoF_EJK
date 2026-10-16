//! Log requests (`PROTOCOL.md`, "Log requests"): the SJK team asks a player for their
//! game's logs, to look into a problem the player has. The player's game reads the
//! open requests ([`LogRequest`]), the player chooses to send ([`LogUpload`]) or not,
//! and staff read what came ([`LogRequestRow`]). Staff also read players' crash
//! reports ([`CrashGroup`], [`CrashSummary`], [`CrashDetail`]) through the same page.
//!
//! Nothing is sent without the player choosing to: the service only lists requests.

use serde::Deserialize;

/// Longest log text the hub takes, in characters.
pub const LOGS_MAX: usize = 400_000;
/// Largest `POST /v1/logs` body the hub takes, in bytes.
pub const BODY_MAX: usize = 524_288;
/// What [`LogUpload::fitted`] keeps the body under, leaving room for the other fields.
const BODY_BUDGET: usize = BODY_MAX - 8_192;
/// Longest `build`, `os` and `gpu`, as for crash reports.
const BUILD_MAX: usize = 32;
const OS_MAX: usize = 64;
const GPU_MAX: usize = 160;

/// A request from the SJK team for the player's logs, as `GET /v1/log-requests` lists it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct LogRequest {
    /// The request's number at the hub.
    pub id: i64,
    /// The display name of the staff member who asked.
    #[serde(default)]
    pub from: String,
    /// What they wrote with it; may be empty.
    #[serde(default)]
    pub note: String,
    /// When they asked, unix seconds by the hub's clock.
    #[serde(default)]
    pub created: i64,
}

/// `GET /v1/log-requests`'s answer.
#[derive(Debug, Deserialize)]
pub(crate) struct LogRequests {
    pub(crate) requests: Vec<LogRequest>,
}

/// What a player sends for a request.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LogUpload {
    /// The logs, as plain text.
    pub logs: String,
    /// The game's build.
    pub build: String,
    /// The system, printable ASCII.
    pub os: String,
    /// The graphics adapter, printable ASCII.
    pub gpu: String,
}

impl LogUpload {
    /// The upload as the hub takes it: the logs [`clean`]ed and cut from the front (the
    /// oldest lines go) to [`LOGS_MAX`] characters and a body under [`BODY_MAX`] bytes,
    /// cutting at a line; `build` emptied unless it is the reports' alphabet, `os` and
    /// `gpu` cut to printable ASCII.
    pub fn fitted(&self) -> Self {
        let mut logs = clean(&self.logs);
        let chars = logs.chars().count();
        if chars > LOGS_MAX {
            let cut = logs
                .char_indices()
                .nth(chars - LOGS_MAX)
                .map_or(0, |(at, _)| at);
            logs = from_line(&logs, cut);
        }
        while body_len(&logs) > BODY_BUDGET {
            let over = body_len(&logs) - BODY_BUDGET;
            // Escapes take up to six bytes for one: cut at least what is over, by bytes.
            let mut cut = over.min(logs.len());
            while !logs.is_char_boundary(cut) {
                cut += 1;
            }
            logs = from_line(&logs, cut);
        }
        let build_ok = self.build.len() <= BUILD_MAX
            && self
                .build
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-./ ".contains(&b));
        Self {
            logs,
            build: if build_ok {
                self.build.clone()
            } else {
                String::new()
            },
            os: ascii(&self.os, OS_MAX),
            gpu: ascii(&self.gpu, GPU_MAX),
        }
    }

    /// `POST /v1/logs`'s body for request `id`.
    pub fn body(&self, id: i64) -> serde_json::Value {
        serde_json::json!({
            "request": id,
            "logs": self.logs,
            "build": self.build,
            "os": self.os,
            "gpu": self.gpu,
        })
    }
}

/// How many bytes `logs` takes in a JSON string.
fn body_len(logs: &str) -> usize {
    serde_json::to_string(logs).map_or(usize::MAX, |json| json.len())
}

/// `text` from byte `at` on, starting after the next line break when there is one, and
/// with a line saying earlier lines were left out.
fn from_line(text: &str, at: usize) -> String {
    let rest = &text[at..];
    let rest = match rest.find('\n') {
        Some(end) if at > 0 => &rest[end + 1..],
        _ => rest,
    };
    format!("[earlier lines left out]\n{rest}")
}

/// Printable ASCII of `raw`, its ends trimmed, at most `max` bytes.
fn ascii(raw: &str, max: usize) -> String {
    let kept: String = raw
        .trim()
        .chars()
        .filter(|c| (' '..='~').contains(c))
        .collect();
    kept[..kept.len().min(max)].trim_end().to_owned()
}

/// Bidi overrides and isolates, and the byte order mark, which the hub refuses.
fn hidden(c: char) -> bool {
    matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
}

/// `text` as the hub takes logs: `\r\n` made `\n`, and every control character but
/// `\n` and `\t`, and every [`hidden`] one, taken out.
pub fn clean(text: &str) -> String {
    text.replace("\r\n", "\n")
        .chars()
        .filter(|&c| (!c.is_control() || c == '\n' || c == '\t') && !hidden(c))
        .collect()
}

/// A log request as staff see it (`POST /v1/staff/log-requests`).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct LogRequestRow {
    /// The request's number.
    pub id: i64,
    /// The player asked (their person's id).
    pub key_id: String,
    /// The player's display name now.
    #[serde(default)]
    pub name: String,
    /// The staff member who asked.
    #[serde(default)]
    pub staff_key_id: String,
    /// Their display name.
    #[serde(default)]
    pub staff_name: String,
    /// What they wrote with it.
    #[serde(default)]
    pub note: String,
    /// When they asked, unix seconds.
    #[serde(default)]
    pub created: i64,
    /// `open`, `sent`, `declined` or `expired`.
    #[serde(default)]
    pub state: String,
    /// When the player answered, unix seconds, or 0.
    #[serde(default)]
    pub answered: i64,
    /// Bytes of logs sent, 0 unless sent.
    #[serde(default)]
    pub size: u64,
    /// The player's build, when sent.
    #[serde(default)]
    pub build: String,
    /// The player's system, when sent.
    #[serde(default)]
    pub os: String,
    /// The player's graphics adapter, when sent.
    #[serde(default)]
    pub gpu: String,
    /// The logs, only in the answer to [`crate::staff::StaffQuery::Log`].
    #[serde(default)]
    pub logs: String,
}

/// `POST /v1/staff/log-requests`'s answer.
#[derive(Debug, Deserialize)]
pub(crate) struct LogRequestRows {
    pub(crate) requests: Vec<LogRequestRow>,
}

/// The crash reports that share one signature (`POST /v1/staff/crash-groups`).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct CrashGroup {
    /// The hub's signature: kind, file and the message's first line, digits aside.
    pub signature: String,
    /// The newest report's kind.
    #[serde(default)]
    pub kind: String,
    /// The newest report's location.
    #[serde(default)]
    pub location: String,
    /// The first line of the newest report's message.
    #[serde(default)]
    pub message: String,
    /// How many reports.
    #[serde(default)]
    pub count: i64,
    /// How many players sent one.
    #[serde(default)]
    pub keys: i64,
    /// When the hub took the first, unix seconds.
    #[serde(default)]
    pub first: i64,
    /// When the hub took the last, unix seconds.
    #[serde(default)]
    pub last: i64,
    /// The builds the reports came from.
    #[serde(default)]
    pub builds: Vec<String>,
}

/// `POST /v1/staff/crash-groups`'s answer.
#[derive(Debug, Deserialize)]
pub(crate) struct CrashGroups {
    pub(crate) groups: Vec<CrashGroup>,
}

/// One crash report without its long texts (`POST /v1/staff/crashes`).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct CrashSummary {
    /// The report's number.
    pub id: i64,
    /// Who sent it.
    #[serde(default)]
    pub key_id: String,
    /// Their display name now.
    #[serde(default)]
    pub name: String,
    /// When the hub took it, unix seconds.
    #[serde(default)]
    pub created: i64,
    /// `panic`, `exception`, `error` or `unclean`.
    #[serde(default)]
    pub kind: String,
    /// The hub's signature, shared by one bug's reports.
    #[serde(default)]
    pub signature: String,
    /// The game's build.
    #[serde(default)]
    pub build: String,
    /// The map played.
    #[serde(default)]
    pub map: String,
    /// The system.
    #[serde(default)]
    pub os: String,
    /// The graphics adapter.
    #[serde(default)]
    pub gpu: String,
    /// Where in the code, for a panic.
    #[serde(default)]
    pub location: String,
    /// The message's first line.
    #[serde(default)]
    pub message: String,
    /// The server played on, or empty.
    #[serde(default)]
    pub server: String,
    /// Seconds the game ran.
    #[serde(default)]
    pub uptime: i64,
    /// The in-game name worn when it was sent.
    #[serde(default)]
    pub worn: String,
}

/// `POST /v1/staff/crashes`'s answer.
#[derive(Debug, Deserialize)]
pub(crate) struct CrashSummaries {
    pub(crate) crashes: Vec<CrashSummary>,
}

/// One whole crash report (`POST /v1/staff/crash`).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct CrashDetail {
    /// What [`CrashSummary`] holds, but the message whole.
    #[serde(flatten)]
    pub summary: CrashSummary,
    /// The thread that panicked.
    #[serde(default)]
    pub thread: String,
    /// The backtrace.
    #[serde(default)]
    pub backtrace: String,
    /// The end of the game's log.
    #[serde(default)]
    pub log: String,
    /// When it crashed, by the player's clock.
    #[serde(default)]
    pub crashed: i64,
}

impl CrashDetail {
    /// The report as plain text, to read in an editor.
    pub fn readable(&self) -> String {
        let s = &self.summary;
        let mut out = format!(
            "SJK crash report #{} ({})\nSignature: {}\nPlayer: {} ({}){}\n",
            s.id,
            s.kind,
            s.signature,
            s.name,
            s.key_id,
            if s.worn.is_empty() {
                String::new()
            } else {
                format!(", wearing {}", s.worn)
            }
        );
        for (label, value) in [
            ("Build", s.build.as_str()),
            ("System", s.os.as_str()),
            ("Graphics", s.gpu.as_str()),
            ("Map", s.map.as_str()),
            ("Server", s.server.as_str()),
            ("Thread", self.thread.as_str()),
            ("Location", s.location.as_str()),
        ] {
            if !value.is_empty() {
                out.push_str(&format!("{label}: {value}\n"));
            }
        }
        out.push_str(&format!("Ran for: {} s\n", s.uptime));
        for (label, value) in [
            ("Message", s.message.as_str()),
            ("Backtrace", self.backtrace.as_str()),
            ("End of the log", self.log.as_str()),
        ] {
            if !value.is_empty() {
                out.push_str(&format!("\n{label}:\n{value}\n"));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_keeps_lines_and_tabs_and_drops_what_the_hub_refuses() {
        assert_eq!(
            clean("a\r\nb\tc\u{0}d\u{1b}[31me\u{7f}\u{202e}f\u{feff}g\rh"),
            "a\nb\tcd[31mefgh"
        );
        assert_eq!(clean("échec 🙂 <b>"), "échec 🙂 <b>");
    }

    #[test]
    fn a_fitted_upload_keeps_the_newest_lines_under_the_limits() {
        let line = "x".repeat(99);
        let logs: String = (0..6_000).map(|n| format!("{n:05} {line}\n")).collect();
        let fitted = LogUpload {
            logs: logs.clone(),
            ..LogUpload::default()
        }
        .fitted();
        assert!(fitted.logs.chars().count() <= LOGS_MAX + 30);
        assert!(fitted.logs.starts_with("[earlier lines left out]\n"));
        assert!(fitted.logs.ends_with(&format!("05999 {line}\n")));
        // Cut at a line: what follows the marker is a whole line.
        let first = fitted.logs.lines().nth(1).unwrap();
        assert_eq!(first.len(), 105, "{first}");
        assert!(fitted.body(1).to_string().len() <= BODY_MAX);
        // Small logs stay as they are.
        let small = LogUpload {
            logs: "one\ntwo\n".into(),
            ..LogUpload::default()
        };
        assert_eq!(small.fitted().logs, "one\ntwo\n");
    }

    #[test]
    fn a_body_full_of_escapes_still_fits() {
        // Each tab and each quote takes two bytes in JSON.
        let logs = "\t\"".repeat(200_000);
        let fitted = LogUpload {
            logs,
            ..LogUpload::default()
        }
        .fitted();
        assert!(fitted.body(1).to_string().len() <= BODY_MAX);
    }

    #[test]
    fn machine_fields_are_made_what_the_hub_takes() {
        let fitted = LogUpload {
            logs: "x".into(),
            build: "2026.1011.2".into(),
            os: " Windows 11 Pro™ ".into(),
            gpu: "a".repeat(200),
        }
        .fitted();
        assert_eq!(fitted.build, "2026.1011.2");
        assert_eq!(fitted.os, "Windows 11 Pro");
        assert_eq!(fitted.gpu.len(), GPU_MAX);
        let odd = LogUpload {
            build: "dev+abc".into(),
            ..LogUpload::default()
        };
        assert_eq!(odd.fitted().build, "");
    }

    #[test]
    fn a_crash_detail_reads_from_the_hubs_flat_object() {
        let detail: CrashDetail = serde_json::from_value(serde_json::json!({
            "id": 7, "key_id": "0123456789abcdef", "name": "Sol", "verified": true,
            "created": 10, "kind": "panic", "signature": "00ff00ff00ff00ff",
            "build": "2026.1011.2", "map": "maps/mp/ffa3.bsp", "os": "Windows 11",
            "gpu": "RTX", "thread": "main", "location": "src/a.rs:1:2",
            "message": "boom\nmore", "backtrace": "bt", "log": "lines",
            "server": "", "uptime": 5, "crashed": 9, "worn": "^1Sol", "names": []
        }))
        .unwrap();
        assert_eq!(detail.summary.id, 7);
        assert_eq!(detail.thread, "main");
        let text = detail.readable();
        assert!(text.contains("Message:\nboom\nmore"), "{text}");
        assert!(text.contains("Location: src/a.rs:1:2"), "{text}");
        assert!(!text.contains("Server:"), "{text}");
    }
}
