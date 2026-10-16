//! Anonymous settings usage (`PROTOCOL.md`, "Settings usage"): which settings
//! players change, from what to what, and what they search for in Settings. A
//! batch carries no key, signature or name: only the build, a random id drawn
//! once per game run (so one run's events read together without linking runs)
//! and the events.

use serde::Serialize;
use serde_json::Value;

/// Most events one batch carries.
pub const BATCH_MAX: usize = 100;
/// Longest setting name, value and group, and the longest search, in characters.
const SETTING_MAX: usize = 64;
const VALUE_MAX: usize = 64;
const GROUP_MAX: usize = 40;
const QUERY_MAX: usize = 32;
const BUILD_MAX: usize = 40;

/// Which of Settings' two modes showed when the event happened.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// The settings that matter most, grouped simply (the default).
    Simple,
    /// Every setting.
    Advanced,
}

/// One thing a player did in Settings.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Event {
    /// A setting changed from `from` to `to`, as its row shows them.
    Change {
        /// Unix seconds.
        at: u64,
        /// The mode on show.
        mode: Mode,
        /// The category or search group the row was shown under.
        group: String,
        /// The setting's console name (or its row's stand-in, `renderer`).
        setting: String,
        /// The value before, as the row showed it.
        from: String,
        /// The value after.
        to: String,
        /// The search showing when the change was made, if one was.
        #[serde(skip_serializing_if = "Option::is_none")]
        search: Option<String>,
    },
    /// A search typed in Settings, and how many settings it found.
    Search {
        /// Unix seconds.
        at: u64,
        /// The mode on show.
        mode: Mode,
        /// What was typed.
        query: String,
        /// How many settings it found.
        results: u32,
    },
}

impl Event {
    /// The event cut to the hub's limits, control characters left out; `None`
    /// when nothing is left of what it must name.
    pub fn fitted(&self) -> Option<Self> {
        match self {
            Self::Change {
                at,
                mode,
                group,
                setting,
                from,
                to,
                search,
            } => {
                let setting: String = setting
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || "_.+- ".contains(*c))
                    .take(SETTING_MAX)
                    .collect();
                let setting = setting.trim().to_owned();
                (!setting.is_empty()).then(|| Self::Change {
                    at: *at,
                    mode: *mode,
                    group: printable(group, GROUP_MAX),
                    setting,
                    from: printable(from, VALUE_MAX),
                    to: printable(to, VALUE_MAX),
                    search: search
                        .as_deref()
                        .map(|search| printable(search, QUERY_MAX))
                        .filter(|search| !search.is_empty()),
                })
            }
            Self::Search {
                at,
                mode,
                query,
                results,
            } => {
                let query = printable(query, QUERY_MAX);
                (!query.is_empty()).then(|| Self::Search {
                    at: *at,
                    mode: *mode,
                    query,
                    results: (*results).min(10_000),
                })
            }
        }
    }
}

/// `text` without control characters, trimmed, at most `max` characters.
fn printable(text: &str, max: usize) -> String {
    let kept: String = text.chars().filter(|c| !c.is_control()).take(max).collect();
    kept.trim().to_owned()
}

/// A random id for one game run: 16 lowercase hex characters.
pub fn session_id() -> String {
    let bytes = crate::keys::random_bytes::<8>().unwrap_or_else(|_| {
        // No system randomness: the time is still a different id for each run.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |time| time.as_nanos() as u64);
        nanos.to_le_bytes()
    });
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `POST /v1/usage/settings`'s body: the build, the run's id and at most
/// [`BATCH_MAX`] events, each [`Event::fitted`].
pub fn body(build: &str, session: &str, events: &[Event]) -> Value {
    let events: Vec<Event> = events
        .iter()
        .filter_map(Event::fitted)
        .take(BATCH_MAX)
        .collect();
    let build: String = build
        .chars()
        .filter(|c| c.is_ascii_graphic())
        .take(BUILD_MAX)
        .collect();
    serde_json::json!({
        "build": build,
        "session": session,
        "events": events,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_batch_carries_the_protocols_fields_and_nothing_of_the_player() {
        let events = [
            Event::Change {
                at: 1_760_300_000,
                mode: Mode::Simple,
                group: "Display".to_owned(),
                setting: "cg_fov".to_owned(),
                from: "95".to_owned(),
                to: "100".to_owned(),
                search: Some("fov".to_owned()),
            },
            Event::Search {
                at: 1_760_300_010,
                mode: Mode::Advanced,
                query: "brightness\u{7}".to_owned(),
                results: 2,
            },
        ];
        let body = body("2026.1012.2", "9f3c0a1b2d4e5f60", &events);
        assert_eq!(
            body,
            serde_json::json!({
                "build": "2026.1012.2",
                "session": "9f3c0a1b2d4e5f60",
                "events": [
                    {"kind": "change", "at": 1_760_300_000, "mode": "simple", "group": "Display",
                     "setting": "cg_fov", "from": "95", "to": "100", "search": "fov"},
                    {"kind": "search", "at": 1_760_300_010, "mode": "advanced",
                     "query": "brightness", "results": 2},
                ],
            })
        );
    }

    #[test]
    fn events_are_cut_to_the_limits_and_empty_ones_dropped() {
        let long = "x".repeat(200);
        let change = Event::Change {
            at: 1,
            mode: Mode::Simple,
            group: long.clone(),
            setting: format!("cg_fov{long}"),
            from: long.clone(),
            to: String::new(),
            search: Some(String::new()),
        }
        .fitted()
        .unwrap();
        let Event::Change {
            group,
            setting,
            from,
            search,
            ..
        } = change
        else {
            panic!("a change");
        };
        assert_eq!(group.chars().count(), GROUP_MAX);
        assert_eq!(setting.chars().count(), SETTING_MAX);
        assert_eq!(from.chars().count(), VALUE_MAX);
        assert_eq!(search, None);
        let empty = Event::Search {
            at: 1,
            mode: Mode::Simple,
            query: " \u{1} ".to_owned(),
            results: 0,
        };
        assert_eq!(empty.fitted(), None);
        let many: Vec<Event> = (0..150)
            .map(|index| Event::Search {
                at: index,
                mode: Mode::Simple,
                query: "fov".to_owned(),
                results: 1,
            })
            .collect();
        let body = body("b", "0000000000000000", &many);
        assert_eq!(body["events"].as_array().unwrap().len(), BATCH_MAX);
    }

    #[test]
    fn a_session_id_is_sixteen_hex_characters() {
        let id = session_id();
        assert_eq!(id.len(), 16);
        assert!(
            id.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_ne!(id, session_id());
    }
}
