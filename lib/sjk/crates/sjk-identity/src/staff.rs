//! Staff requests (`PROTOCOL.md`, "Staff"): what a key the hub's operator made staff
//! may do from inside the game: find players, verify them, give and take back medals and
//! unlockables, give and take back holocrons, clear achievements, moderate the SJK chat
//! and players' pictures, merge two players into one and unlink a key from a player. The
//! hub refuses every one from a key that is not staff; the client only offers them to
//! a player whose own profile says `staff`.

use crate::logs::{CrashDetail, CrashGroup, CrashSummary, LogRequestRow};
use crate::wire::Profile;

/// One staff request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StaffRequest {
    /// Find players: 16 hex digits for one key, a part of a name, or empty for the
    /// players seen most recently.
    Search(String),
    /// Give a medal.
    Award {
        /// The key that gets it.
        key_id: String,
        /// The medal's id.
        medal: String,
        /// Why, shown with the medal; may be empty.
        note: String,
    },
    /// Take one award of a medal back.
    Unaward {
        /// The key that holds it.
        key_id: String,
        /// The medal's id.
        medal: String,
    },
    /// Grant an unlockable (`PROTOCOL.md`, "Unlocks").
    Unlock {
        /// The key that gets it.
        key_id: String,
        /// The unlockable's id.
        unlock: String,
        /// Shown with it; may be empty.
        note: String,
    },
    /// Take an unlockable back.
    Relock {
        /// The key that holds it.
        key_id: String,
        /// The unlockable's id.
        unlock: String,
    },
    /// Clear achievements.
    ClearAchievements {
        /// The key whose achievements go.
        key_id: String,
        /// The achievement's id, or empty for every one.
        id: String,
    },
    /// Delete an SJK chat message for everyone.
    ChatDelete {
        /// The message's id.
        id: u64,
    },
    /// Mute or unmute a key in the SJK chat.
    ChatMute {
        /// The key.
        key_id: String,
        /// Mute (true) or unmute.
        muted: bool,
    },
    /// Take a key's picture down.
    AvatarRemove {
        /// The key.
        key_id: String,
    },
    /// Stop a key from uploading pictures (taking its picture down), or let it again.
    AvatarBlock {
        /// The key.
        key_id: String,
        /// Stop (true) or let again.
        blocked: bool,
    },
    /// Give a holocron of a tier (`PROTOCOL.md`, "Holocrons"). A gift counts toward no
    /// cap and is not announced.
    HolocronGive {
        /// The key that gets it.
        key_id: String,
        /// The tier's wire id (`uncommon`, `rare`, `legendary`, `mythical`).
        tier: String,
        /// Shown with it; may be empty.
        note: String,
    },
    /// Take one holocron back, by its number at the hub.
    HolocronRemove {
        /// The key that holds it.
        key_id: String,
        /// The holocron's number ([`crate::Holocron::id`]).
        id: u64,
    },
    /// Say whether the SJK team vouches for a player (the verified mark).
    Verify {
        /// The player's key.
        key_id: String,
        /// Vouch (true) or no longer.
        verified: bool,
    },
    /// Merge one player into another, for good: everything `from` holds goes to the
    /// player of `key_id`, and `from`'s keys become linked keys of that player. A staff
    /// player cannot be merged away.
    Merge {
        /// The player who is kept.
        key_id: String,
        /// The player merged away.
        from: String,
    },
    /// Detach a linked key from its player. The key starts afresh the next time it is
    /// used; nothing it brought moves back.
    Unlink {
        /// The linked key (never a player's main key).
        key_id: String,
    },
}

impl StaffRequest {
    /// The key the request changes, if it changes one.
    pub fn target(&self) -> Option<&str> {
        match self {
            Self::Search(_) | Self::ChatDelete { .. } => None,
            Self::Award { key_id, .. }
            | Self::Unaward { key_id, .. }
            | Self::Unlock { key_id, .. }
            | Self::Relock { key_id, .. }
            | Self::ClearAchievements { key_id, .. }
            | Self::ChatMute { key_id, .. }
            | Self::AvatarRemove { key_id }
            | Self::AvatarBlock { key_id, .. }
            | Self::HolocronGive { key_id, .. }
            | Self::HolocronRemove { key_id, .. }
            | Self::Verify { key_id, .. }
            | Self::Merge { key_id, .. }
            | Self::Unlink { key_id } => Some(key_id),
        }
    }
}

/// A staff request about players' logs and crash reports (`PROTOCOL.md`, "Log
/// requests"): asking a player for their logs, and reading what players sent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StaffQuery {
    /// Ask a player for their game's logs; their game asks them whether to send.
    RequestLogs {
        /// The player's key.
        key_id: String,
        /// Shown to the player with the request; may be empty.
        note: String,
    },
    /// The log requests, newest first.
    LogRequests,
    /// One request with the logs sent for it, by its number.
    Log(i64),
    /// The crash reports, grouped by the hub's signature.
    CrashGroups,
    /// The newest crash reports of one signature, or of all with an empty one.
    Crashes(String),
    /// One whole crash report, by its number.
    Crash(i64),
}

/// What the hub answered a [`StaffQuery`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StaffAnswer {
    /// The request made, or one request with its logs.
    LogRequest(Box<LogRequestRow>),
    /// The log requests.
    LogRequests(Vec<LogRequestRow>),
    /// The crash groups.
    CrashGroups(Vec<CrashGroup>),
    /// The crash reports of a signature.
    Crashes(Vec<CrashSummary>),
    /// One crash report.
    Crash(Box<CrashDetail>),
}

/// A text a staff member opened (logs or a crash report), for the page to save and show.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OpenedText {
    /// A file name for it, without extension: ASCII letters, digits, `-` and `_` only.
    pub name: String,
    /// The text.
    pub text: String,
}

/// What the staff requests brought back.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StaffState {
    /// Counts answers, so a reader can tell a new one.
    pub serial: u64,
    /// The players the last search found, updated by later answers about them.
    pub players: Vec<Profile>,
    /// What the last request came to: done, or why not.
    pub message: String,
    /// Whether the last request failed.
    pub failed: bool,
    /// A request is on its way.
    pub busy: bool,
    /// The log requests, as the hub last listed them (newest first).
    pub log_requests: Vec<LogRequestRow>,
    /// The crash groups, as the hub last listed them.
    pub crash_groups: Vec<CrashGroup>,
    /// The crash reports of the signature last asked about.
    pub crashes: Vec<CrashSummary>,
    /// That signature (empty: the newest of all).
    pub crashes_of: String,
    /// The logs or crash report opened last, until the viewer takes it
    /// (`Service::take_opened`).
    pub opened: Option<OpenedText>,
}

impl StaffState {
    /// Put `profile`, the hub's answer about a key, in place of that key's entry.
    pub fn replace(&mut self, profile: &Profile) {
        if let Some(entry) = self
            .players
            .iter_mut()
            .find(|entry| entry.key_id == profile.key_id)
        {
            *entry = profile.clone();
        } else {
            self.players.insert(0, profile.clone());
        }
    }

    /// Take the player who owns `key_id` (their main key or a linked one) out of the
    /// list, as a merge does with the player merged away; `kept` is never taken out.
    pub fn remove(&mut self, key_id: &str, kept: &str) {
        self.players
            .retain(|entry| entry.key_id == kept || !entry.has_key(key_id));
    }
}

impl StaffState {
    /// Take in the hub's answer to `query`.
    pub fn answered(&mut self, query: &StaffQuery, answer: StaffAnswer) {
        self.message = match answer {
            StaffAnswer::LogRequest(mut row) if matches!(query, StaffQuery::Log(_)) => {
                let message = format!("Opened the logs of {}", plain(&row.name));
                let name = format!("sjk-logs-{}-{}", row.id, file_part(&row.name));
                self.open(name, std::mem::take(&mut row.logs));
                self.put_request(*row);
                message
            }
            StaffAnswer::LogRequest(row) => {
                let message = format!("Asked {} for their logs", plain(&row.name));
                self.put_request(*row);
                message
            }
            StaffAnswer::LogRequests(rows) => {
                self.log_requests = rows;
                format!("{} log requests", self.log_requests.len())
            }
            StaffAnswer::CrashGroups(groups) => {
                self.crash_groups = groups;
                format!("{} kinds of crash", self.crash_groups.len())
            }
            StaffAnswer::Crashes(crashes) => {
                self.crashes = crashes;
                self.crashes_of = match query {
                    StaffQuery::Crashes(signature) => signature.clone(),
                    _ => String::new(),
                };
                format!("{} crash reports", self.crashes.len())
            }
            StaffAnswer::Crash(detail) => {
                let name = format!(
                    "sjk-crash-{}-{}",
                    detail.summary.id,
                    file_part(&detail.summary.name)
                );
                self.open(name, detail.readable());
                format!("Opened crash report #{}", detail.summary.id)
            }
        };
    }

    fn open(&mut self, name: String, text: String) {
        self.opened = Some(OpenedText { name, text });
    }

    /// Put `row` in place of the request with its number, or first.
    fn put_request(&mut self, row: LogRequestRow) {
        match self
            .log_requests
            .iter_mut()
            .find(|entry| entry.id == row.id)
        {
            Some(entry) => *entry = row,
            None => self.log_requests.insert(0, row),
        }
    }
}

/// `name` without colour codes.
fn plain(name: &str) -> String {
    let mut out = String::new();
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^' && chars.peek().is_some() {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

/// `name` as part of a file name: ASCII letters and digits, the rest `_`, at most 24.
fn file_part(name: &str) -> String {
    let part: String = plain(name)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .take(24)
        .collect();
    let part = part.trim_matches('_');
    if part.is_empty() {
        "player".to_owned()
    } else {
        part.to_owned()
    }
}

/// What a request's success is called on the page.
pub(crate) fn done(request: &StaffRequest, count: usize) -> String {
    match request {
        StaffRequest::Search(query) if query.is_empty() => {
            format!("{count} players seen lately")
        }
        StaffRequest::Search(_) => match count {
            0 => "Nobody found".to_owned(),
            1 => "1 player found".to_owned(),
            count => format!("{count} players found"),
        },
        StaffRequest::Award { medal, .. } => format!("Gave {medal}"),
        StaffRequest::Unaward { medal, .. } => format!("Took back {medal}"),
        StaffRequest::Unlock { unlock, .. } => format!("Unlocked {unlock}"),
        StaffRequest::Relock { unlock, .. } => format!("Took back {unlock}"),
        StaffRequest::ClearAchievements { id, .. } if id.is_empty() => {
            "Cleared every achievement".to_owned()
        }
        StaffRequest::ClearAchievements { id, .. } => format!("Cleared {id}"),
        StaffRequest::ChatDelete { .. } => "Deleted the message".to_owned(),
        StaffRequest::ChatMute { muted: true, .. } => "Muted in the chat".to_owned(),
        StaffRequest::ChatMute { muted: false, .. } => "Unmuted in the chat".to_owned(),
        StaffRequest::AvatarRemove { .. } => "Took the picture down".to_owned(),
        StaffRequest::AvatarBlock { blocked: true, .. } => {
            "Stopped their pictures and took theirs down".to_owned()
        }
        StaffRequest::AvatarBlock { blocked: false, .. } => {
            "They may upload pictures again".to_owned()
        }
        StaffRequest::HolocronGive { tier, .. } => format!("Gave a {tier} holocron"),
        StaffRequest::HolocronRemove { id, .. } => format!("Took back holocron #{id}"),
        StaffRequest::Verify { verified: true, .. } => "Verified".to_owned(),
        StaffRequest::Verify {
            verified: false, ..
        } => "No longer verified".to_owned(),
        StaffRequest::Merge { from, .. } => format!("Merged {from} into this player"),
        StaffRequest::Unlink { key_id } => format!("Unlinked {key_id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opened_logs_become_a_text_with_a_safe_file_name() {
        let mut state = StaffState::default();
        state.answered(
            &StaffQuery::LogRequests,
            StaffAnswer::LogRequests(vec![LogRequestRow {
                id: 4,
                name: "^1Kush^7ina".into(),
                state: "open".into(),
                ..LogRequestRow::default()
            }]),
        );
        assert_eq!(state.message, "1 log requests");
        state.answered(
            &StaffQuery::Log(4),
            StaffAnswer::LogRequest(Box::new(LogRequestRow {
                id: 4,
                name: "^1Kush^7ina/..".into(),
                state: "sent".into(),
                logs: "lines".into(),
                ..LogRequestRow::default()
            })),
        );
        let opened = state.opened.take().unwrap();
        assert_eq!(opened.name, "sjk-logs-4-Kushina");
        assert_eq!(opened.text, "lines");
        assert_eq!(state.message, "Opened the logs of Kushina/..");
        // The list's row is updated, without the text.
        assert_eq!(state.log_requests.len(), 1);
        assert_eq!(state.log_requests[0].state, "sent");
        assert!(state.log_requests[0].logs.is_empty());
    }

    #[test]
    fn a_new_request_goes_first_and_crash_reports_open_as_text() {
        let mut state = StaffState::default();
        state.answered(
            &StaffQuery::RequestLogs {
                key_id: "0123456789abcdef".into(),
                note: String::new(),
            },
            StaffAnswer::LogRequest(Box::new(LogRequestRow {
                id: 9,
                name: "***".into(),
                ..LogRequestRow::default()
            })),
        );
        assert_eq!(state.log_requests[0].id, 9);
        assert_eq!(state.message, "Asked *** for their logs");
        let mut detail = CrashDetail::default();
        detail.summary.id = 3;
        detail.summary.name = "***".into();
        state.answered(&StaffQuery::Crash(3), StaffAnswer::Crash(Box::new(detail)));
        let opened = state.opened.take().unwrap();
        assert_eq!(opened.name, "sjk-crash-3-player");
        assert!(opened.text.starts_with("SJK crash report #3"));
        state.answered(
            &StaffQuery::Crashes("00ff00ff00ff00ff".into()),
            StaffAnswer::Crashes(vec![CrashSummary::default()]),
        );
        assert_eq!(state.crashes_of, "00ff00ff00ff00ff");
    }
}
