//! The `identity` console command: `identity` opens the Identity page, and its
//! words set the player's bio and list who the hub knows on this server
//! (`docs/identity.md`). There is no name to set: the hub takes the name the
//! player plays under.

use crate::player_identity;
use sjk_identity::{Snapshot, Status};

/// Console command name.
pub(crate) const COMMAND: &str = "identity";
/// Help text for completion and `cmdlist`.
pub(crate) const HELP: &str = "Your SJK identity: open its page; bio, key and who subcommands";

const USAGE: &str = "identity [bio <text> | key | who [slot]]";
/// The answer to `identity name`, which no longer exists.
const NAME_FROM_GAME: &str =
    "Your SJK name is the name you play under: change it with /name <text>.";

/// What the player asked for.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Action {
    /// No words: open or close the page.
    Toggle,
    /// Show the key id and where the key file is.
    Key,
    /// List the players the hub knows on this server, or one of them.
    Who(Option<u8>),
    /// Set the bio at the hub.
    Bio(String),
}

/// Read the command's words.
pub(crate) fn parse(args: &[String]) -> Result<Action, String> {
    let Some(word) = args.first() else {
        return Ok(Action::Toggle);
    };
    let text = || args[1..].join(" ");
    match word.to_ascii_lowercase().as_str() {
        "key" => Ok(Action::Key),
        "who" => match args.get(1) {
            None => Ok(Action::Who(None)),
            Some(slot) => slot
                .parse()
                .map(|slot| Action::Who(Some(slot)))
                .map_err(|_| format!("who takes a client number, not \"{slot}\"")),
        },
        "bio" => Ok(Action::Bio(text())),
        "name" => Err(NAME_FROM_GAME.to_owned()),
        _ => Err(format!("usage: {USAGE}")),
    }
}

/// The lines `identity key` prints.
pub(crate) fn key_lines(snapshot: Option<&Snapshot>, file: &std::path::Path) -> Vec<String> {
    match snapshot {
        Some(snapshot) => vec![
            format!("Key id: {}", snapshot.local_key_id),
            format!(
                "The private key is in {}. Back it up: losing it loses this identity, and nobody else may have it.",
                file.display()
            ),
        ],
        None => vec!["No identity key yet: turn cl_identity on.".to_owned()],
    }
}

/// The lines `identity who` prints.
pub(crate) fn who_lines(snapshot: &Snapshot, slot: Option<u8>) -> Vec<String> {
    let mut lines = Vec::new();
    for player in &snapshot.players {
        if slot.is_some_and(|wanted| wanted != player.slot) {
            continue;
        }
        let name = if player.name.is_empty() {
            "(no name)"
        } else {
            &player.name
        };
        lines.push(format!(
            "{:>2}  {}  {}{}",
            player.slot,
            name,
            player.key_id,
            if player.verified { "  VERIFIED" } else { "" }
        ));
        if slot.is_some() {
            match snapshot.profiles.get(&player.key_id) {
                Some(profile) if !profile.bio.is_empty() => {
                    let bio = sjk_identity::bio::for_display(&profile.bio);
                    lines.extend(bio.lines().map(|line| format!("    {line}")));
                }
                Some(_) => lines.push("    (no bio)".to_owned()),
                None => {
                    player_identity::look_up(&player.key_id);
                    lines.push("    fetching the bio; run this again in a moment".to_owned());
                }
            }
        }
    }
    if lines.is_empty() {
        lines.push(match slot {
            Some(slot) => format!("The hub knows nobody in slot {slot} here."),
            None => "The hub knows no SJK players on this server.".to_owned(),
        });
    }
    lines
}

/// Why a profile change cannot be sent now, if it cannot.
pub(crate) fn profile_blocker(snapshot: Option<&Snapshot>) -> Option<&'static str> {
    match snapshot.map(|snapshot| &snapshot.status) {
        None | Some(Status::Disabled) => Some("Identity is off: turn cl_identity on."),
        Some(Status::NoHub) => Some("There is no hub: set cl_hubUrl."),
        Some(Status::Online) => None,
        Some(_) => Some("Not connected to the hub yet; try again in a moment."),
    }
}

impl crate::GpuState {
    /// `identity <words>`: the subcommands that are not "open the page".
    pub(crate) fn identity_command(&mut self, args: &[String]) -> Result<Vec<String>, String> {
        let action = parse(args)?;
        let snapshot = player_identity::snapshot();
        match action {
            Action::Toggle => Ok(Vec::new()),
            Action::Key => {
                let file = self
                    .console
                    .as_ref()
                    .map(|console| console.config_directory().join("identity.key"))
                    .unwrap_or_default();
                Ok(key_lines(snapshot.as_ref(), &file))
            }
            Action::Who(slot) => match &snapshot {
                Some(snapshot) => Ok(who_lines(snapshot, slot)),
                None => Err("Identity is off: turn cl_identity on.".to_owned()),
            },
            Action::Bio(text) => {
                if let Some(reason) = profile_blocker(snapshot.as_ref()) {
                    return Err(reason.to_owned());
                }
                player_identity::set_bio(text);
                Ok(vec![
                    "Sent to the hub; the Identity page shows the result.".to_owned(),
                ])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_identity::{Presence, Profile};
    use std::collections::HashMap;

    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    fn snapshot() -> Snapshot {
        Snapshot {
            loot: Vec::new(),
            opening: None,
            reveal: None,
            local_key_id: "0123456789abcdef".to_owned(),
            status: Status::Online,
            key_id: "0123456789abcdef".to_owned(),
            me: Some(Profile {
                credits: 0,
                cards: Vec::new(),
                keys: Vec::new(),
                key_id: "0123456789abcdef".to_owned(),
                key: String::new(),
                name: "Sol".to_owned(),
                bio: "hi".to_owned(),
                verified: false,
                staff: false,
                created: 0,
                names: Vec::new(),
                medals: Vec::new(),
                achievements: Vec::new(),
                avatar: String::new(),
                unlocks: Vec::new(),
                holocron_counts: Default::default(),
                holocrons: Vec::new(),
            }),
            server: None,
            players: vec![Presence {
                slot: 3,
                claimed_name: "^1Fox".to_owned(),
                key_id: "fedcba9876543210".to_owned(),
                name: "Fox".to_owned(),
                verified: true,
                medals: Vec::new(),
                avatar: String::new(),
                look: None,
            }],
            profiles: HashMap::new(),
            model_checks: HashMap::new(),
            profiles_revision: 0,
            notice: None,
            revision: 0,
            report: None,
            note: None,
            player_report: None,
            avatar: None,
            look_outcome: None,
            packs_revision: 0,
            assets_note: None,
            crashes_sent: 0,
            crash_note: None,
            holocrons: None,
            log_requests: Vec::new(),
            log_answer: None,
        }
    }

    #[test]
    fn words_become_actions() {
        assert_eq!(parse(&words("")), Ok(Action::Toggle));
        assert_eq!(parse(&words("key")), Ok(Action::Key));
        assert_eq!(parse(&words("WHO")), Ok(Action::Who(None)));
        assert_eq!(parse(&words("who 3")), Ok(Action::Who(Some(3))));
        assert_eq!(
            parse(&words("bio Sol the Fox")),
            Ok(Action::Bio("Sol the Fox".to_owned()))
        );
        assert_eq!(parse(&words("bio")), Ok(Action::Bio(String::new())));
        // The name is the in-game one: the old word explains where it went.
        assert_eq!(parse(&words("name Sol")), Err(NAME_FROM_GAME.to_owned()));
        assert!(parse(&words("who x")).is_err());
        assert!(
            parse(&words("frobnicate"))
                .unwrap_err()
                .starts_with("usage")
        );
    }

    #[test]
    fn who_lists_slots_and_marks_verified_players() {
        let lines = who_lines(&snapshot(), None);
        assert_eq!(lines, [" 3  Fox  fedcba9876543210  VERIFIED"]);
        assert_eq!(
            who_lines(&snapshot(), Some(9)),
            ["The hub knows nobody in slot 9 here."]
        );
        let mut empty = snapshot();
        empty.players.clear();
        assert_eq!(
            who_lines(&empty, None),
            ["The hub knows no SJK players on this server."]
        );
    }

    #[test]
    fn who_with_a_slot_shows_a_fetched_bio() {
        let mut shown = snapshot();
        shown.profiles.insert(
            "fedcba9876543210".to_owned(),
            Profile {
                credits: 0,
                cards: Vec::new(),
                keys: Vec::new(),
                key_id: "fedcba9876543210".to_owned(),
                key: String::new(),
                name: "Fox".to_owned(),
                bio: "line one\nline two".to_owned(),
                verified: true,
                staff: false,
                created: 0,
                names: Vec::new(),
                medals: Vec::new(),
                achievements: Vec::new(),
                avatar: String::new(),
                unlocks: Vec::new(),
                holocron_counts: Default::default(),
                holocrons: Vec::new(),
            },
        );
        let lines = who_lines(&shown, Some(3));
        assert_eq!(lines[1..], ["    line one", "    line two"]);
    }

    #[test]
    fn profile_changes_wait_for_an_online_hub() {
        assert!(profile_blocker(None).is_some());
        let mut state = snapshot();
        assert_eq!(profile_blocker(Some(&state)), None);
        for status in [Status::Disabled, Status::NoHub, Status::Registering] {
            state.status = status;
            assert!(profile_blocker(Some(&state)).is_some());
        }
    }
}
