//! `debug_medal`: rehearse receiving SJK medals without the hub (`docs/client.md`,
//! "Identity"). It makes the list a hub profile would carry (`sjk_identity::Medal`:
//! id, count, today's date and a note), which the pop-up reads as it reads the hub's
//! (`medals::awards`) and queues like a real arrival, so the centre print in a match,
//! the pop-up on a menu, the ceremony, the fanfare and Next can all be tried offline.
//!
//! Nothing is sent: the command never reaches the identity service. Rehearsed medals
//! are not written to `medals_seen.txt`, so a medal the SJK team gives later still
//! shows.
//!
//! ```text
//! debug_medal                          the ids and how to use it
//! debug_medal early_tester             one medal
//! debug_medal bug_hunter x3 The fog    Bug Hunter given a third time, with a note
//! debug_medal all [note]               every medal, one after another
//! ```

use crate::medals::{Award, Medal};

/// The console command.
pub(crate) const COMMAND: &str = "debug_medal";
/// Its line in the command list and browser.
pub(crate) const HELP: &str =
    "Rehearse receiving SJK medals, nothing sent: debug_medal <id|all> [x<count>] [note]";
/// The word that names every medal.
const ALL: &str = "all";
/// The most a rehearsed count may be.
const MOST: u32 = 999;

/// The command's usage and the medals it knows.
pub(crate) fn listing() -> Vec<String> {
    let mut lines = vec![
        format!("usage: {COMMAND} <id|all> [x<count>] [note]"),
        "Shows medals as if the SJK team had just given them: the pop-up on the main menu or the game menu (a centre print during a match), with its ceremony and sound. Nothing is sent to the SJK hub and medals_seen.txt is not changed.".to_owned(),
    ];
    for medal in Medal::ALL {
        let again = if medal.repeatable() {
            ", given again with x<count>"
        } else {
            ""
        };
        lines.push(format!("  {:<18} {}{again}", medal.id(), medal.name()));
    }
    lines.push(format!("  {ALL:<18} every medal, one after another"));
    lines
}

/// The list a hub profile would carry for `args` (an id or `all`, an optional
/// `x<count>`, then the note's words), given at `now` (unix seconds).
pub(crate) fn parse(args: &[String], now: i64) -> Result<Vec<sjk_identity::Medal>, String> {
    let Some((first, rest)) = args.split_first() else {
        return Err(format!("usage: {COMMAND} <id|all> [x<count>] [note]"));
    };
    let first = first.to_ascii_lowercase();
    let medals: Vec<Medal> = if first == ALL {
        Medal::ALL.to_vec()
    } else {
        let medal = Medal::from_id(&first).ok_or_else(|| {
            let ids: Vec<&str> = Medal::ALL.iter().map(|medal| medal.id()).collect();
            format!("Unknown medal '{first}': {} or {ALL}", ids.join(", "))
        })?;
        vec![medal]
    };
    let (count, note) = match rest.split_first() {
        Some((word, note)) if count_word(word).is_some() => (count_word(word), note),
        _ => (None, rest),
    };
    let count = match count {
        Some(Ok(count)) => count,
        Some(Err(error)) => return Err(error),
        None => 1,
    };
    if count > 1 && medals.iter().all(|medal| !medal.repeatable()) {
        return Err(format!(
            "{} is given only once; only {} takes a count",
            medals[0].name(),
            Medal::BugHunter.name()
        ));
    }
    let note = note.join(" ");
    Ok(medals
        .into_iter()
        .map(|medal| sjk_identity::Medal {
            id: medal.id().to_owned(),
            count: if medal.repeatable() { count } else { 1 },
            awarded: now,
            note: note.clone(),
        })
        .collect())
}

/// `x<count>` read: `None` when `word` is not one, else the count or why it is
/// refused.
fn count_word(word: &str) -> Option<Result<u32, String>> {
    let digits = word.strip_prefix(['x', 'X'])?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(match digits.parse::<u32>() {
        Ok(count) if (1..=MOST).contains(&count) => Ok(count),
        _ => Err(format!("A count is x1 to x{MOST}")),
    })
}

/// What the console says once `awards` are queued.
pub(crate) fn queued(awards: &[Award]) -> Vec<String> {
    let names: Vec<String> = awards.iter().map(Award::label).collect();
    vec![
        format!("Rehearsing: {}.", names.join(", ")),
        "It shows on the main menu or when the game menu opens (a centre print says so during a match). Nothing is sent to the SJK hub.".to_owned(),
    ]
}

/// Seconds since 1970 now, for the date a rehearsed medal was given.
pub(crate) fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    const NOW: i64 = 1_791_456_000;

    #[test]
    fn one_medal_with_its_note_and_date() {
        let list = parse(&words("early_tester Thanks for testing ^3early"), NOW).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "early_tester");
        assert_eq!((list[0].count, list[0].awarded), (1, NOW));
        assert_eq!(list[0].note, "Thanks for testing ^3early");
        // Read as the hub's list is: the note plain, the date given.
        let awards = crate::medals::awards(&list);
        assert_eq!(awards[0].note, "Thanks for testing early");
        assert_eq!(awards[0].given(), "Given 08/10/2026");
        let ids = parse(&words("EARLY_CONTRIBUTOR"), NOW).unwrap();
        assert_eq!(ids[0].id, "early_contributor");
        assert!(ids[0].note.is_empty());
    }

    #[test]
    fn a_count_for_the_repeatable_medal_only() {
        let list = parse(&words("bug_hunter x3 The fog"), NOW).unwrap();
        assert_eq!((list[0].count, list[0].note.as_str()), (3, "The fog"));
        assert_eq!(crate::medals::awards(&list)[0].label(), "Bug Hunter x3");
        assert!(parse(&words("early_tester x2"), NOW).is_err());
        assert!(parse(&words("bug_hunter x0"), NOW).is_err());
        assert!(parse(&words("bug_hunter x1000"), NOW).is_err());
        // A word that only looks like one is part of the note.
        let list = parse(&words("bug_hunter xylophone"), NOW).unwrap();
        assert_eq!((list[0].count, list[0].note.as_str()), (1, "xylophone"));
    }

    #[test]
    fn all_gives_every_medal_in_the_catalogues_order() {
        let list = parse(&words("all x2 Well done"), NOW).unwrap();
        let ids: Vec<&str> = list.iter().map(|medal| medal.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "early_tester",
                "early_contributor",
                "bug_hunter",
                "supporter"
            ]
        );
        assert!(list.iter().all(|medal| medal.note == "Well done"));
        let counts: Vec<u32> = list.iter().map(|medal| medal.count).collect();
        assert_eq!(counts, [1, 1, 2, 1]);
    }

    #[test]
    fn an_unknown_id_names_the_known_ones_and_alone_it_lists_them() {
        let error = parse(&words("gold_star"), NOW).unwrap_err();
        assert!(error.contains("early_tester") && error.contains("bug_hunter"));
        assert!(error.contains(ALL));
        assert!(parse(&[], NOW).is_err());
        let listing = listing();
        for medal in Medal::ALL {
            assert!(listing.iter().any(|line| line.contains(medal.id())));
        }
        assert!(listing.iter().any(|line| line.contains("Nothing is sent")));
        assert!(COMMAND.starts_with("debug_") && COMMAND == COMMAND.to_ascii_lowercase());
    }
}
