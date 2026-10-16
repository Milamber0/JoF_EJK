//! `debug_holocron`: rehearse receiving holocrons without the hub (`docs/holocrons.md`).
//! It makes holocrons as a hub profile would list them, which the drop pop-up queues like
//! a real arrival, so the centre print in a match, the pop-up on a menu, its ceremony,
//! the fanfare, "1 of 3" and Next can all be tried offline; one chat line of the first
//! tier also goes into the game's chat feed.
//!
//! Nothing is sent: the command never reaches the identity service. Rehearsed holocrons
//! are not written to `holocrons_seen.txt`, so a holocron the hub drops later still shows.
//!
//! ```text
//! debug_holocron                 the tiers and how to use it
//! debug_holocron rare            one Rare Holocron
//! debug_holocron mythical x3     three Mythical Holocrons, one after another
//! debug_holocron all             one of every tier
//! ```

use super::{Entry, TIERS, Tier};

/// The console command.
pub(crate) const COMMAND: &str = "debug_holocron";
/// Its line in the command list and browser.
pub(crate) const HELP: &str =
    "Rehearse receiving holocrons, nothing sent: debug_holocron <tier|all> [x<count>]";
/// The word that names every tier.
const ALL: &str = "all";
/// The most one command rehearses.
pub(crate) const MOST: u32 = 20;
/// The number a rehearsed holocron starts at: far above any the hub has dropped, so a
/// rehearsal never looks older than a real one.
pub(crate) const FIRST_ID: u64 = 4_000_000_000;

/// The command's usage and the tiers it knows.
pub(crate) fn listing() -> Vec<String> {
    let mut lines = vec![
        format!("usage: {COMMAND} <tier|all> [x<count>]"),
        "Shows holocrons as if the hub had just dropped them: the pop-up on the main menu or the game menu (a centre print during a match), with its ceremony and sound. Nothing is sent to the SJK hub and holocrons_seen.txt is not changed.".to_owned(),
    ];
    for tier in &TIERS {
        lines.push(format!(
            "  {:<12} {} ({} of drops)",
            tier.id,
            tier.name,
            tier.odds()
        ));
    }
    lines.push(format!("  {ALL:<12} one of every tier"));
    lines
}

/// The holocrons `args` (a tier or `all`, an optional `x<count>`) make, dropped at `now`
/// (unix seconds), numbered from `first`.
pub(crate) fn parse(args: &[String], now: i64, first: u64) -> Result<Vec<Entry>, String> {
    let Some((tier, rest)) = args.split_first() else {
        return Err(format!("usage: {COMMAND} <tier|all> [x<count>]"));
    };
    let tier = tier.to_ascii_lowercase();
    let tiers: Vec<&'static Tier> = if tier == ALL {
        TIERS.iter().collect()
    } else {
        let found = Tier::from_id(&tier).ok_or_else(|| {
            let ids: Vec<&str> = TIERS.iter().map(|tier| tier.id).collect();
            format!("Unknown tier '{tier}': {} or {ALL}", ids.join(", "))
        })?;
        vec![found]
    };
    let count = match rest {
        [] => 1,
        [word] => count_word(word)?,
        _ => return Err(format!("usage: {COMMAND} <tier|all> [x<count>]")),
    };
    if count > 1 && tier == ALL {
        return Err(format!("{ALL} gives one of every tier: no count"));
    }
    Ok((0..count)
        .flat_map(|_| tiers.iter().copied())
        .zip(first..)
        .map(|(tier, id)| Entry {
            id,
            tier,
            dropped: now,
            gift: false,
            note: String::new(),
        })
        .collect())
}

/// `x<count>` read.
fn count_word(word: &str) -> Result<u32, String> {
    let wrong = || format!("A count is x1 to x{MOST}");
    let digits = word.strip_prefix(['x', 'X']).ok_or_else(wrong)?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(wrong());
    }
    match digits.parse::<u32>() {
        Ok(count) if (1..=MOST).contains(&count) => Ok(count),
        _ => Err(wrong()),
    }
}

/// What the console says once `entries` are queued.
pub(crate) fn queued(entries: &[Entry]) -> Vec<String> {
    let names: Vec<&str> = entries.iter().map(|entry| entry.tier.name).collect();
    vec![
        format!("Rehearsing: {}.", names.join(", ")),
        "It shows on the main menu or when the game menu opens (a centre print says so during a match). Nothing is sent to the SJK hub.".to_owned(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    const NOW: i64 = 1_791_641_100;

    fn tiers_of(entries: &[Entry]) -> Vec<&'static str> {
        entries.iter().map(|entry| entry.tier.id).collect()
    }

    #[test]
    fn one_tier_once_or_several_times() {
        let one = parse(&words("rare"), NOW, FIRST_ID).unwrap();
        assert_eq!(tiers_of(&one), ["rare"]);
        assert_eq!(
            (one[0].id, one[0].dropped, one[0].gift),
            (FIRST_ID, NOW, false)
        );
        let three = parse(&words("MYTHICAL x3"), NOW, FIRST_ID).unwrap();
        assert_eq!(tiers_of(&three), ["mythical"; 3]);
        let ids: Vec<u64> = three.iter().map(|entry| entry.id).collect();
        assert_eq!(
            ids,
            [FIRST_ID, FIRST_ID + 1, FIRST_ID + 2],
            "numbered, rising"
        );
        assert_eq!(parse(&words("rare X2"), NOW, 5).unwrap().len(), 2);
    }

    #[test]
    fn all_is_one_of_every_tier_in_the_catalogues_order() {
        let all = parse(&words("all"), NOW, 10).unwrap();
        assert_eq!(
            tiers_of(&all),
            ["uncommon", "rare", "legendary", "mythical"]
        );
        assert_eq!(
            all.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            [10, 11, 12, 13]
        );
        assert!(parse(&words("all x2"), NOW, 10).is_err());
    }

    #[test]
    fn bad_input_says_what_was_meant() {
        let error = parse(&words("gold"), NOW, 1).unwrap_err();
        assert!(error.contains("uncommon") && error.contains("mythical") && error.contains(ALL));
        assert!(parse(&[], NOW, 1).is_err());
        for bad in [
            "rare x0",
            "rare x21",
            "rare x",
            "rare 3",
            "rare x1x",
            "rare x2 x3",
        ] {
            assert!(parse(&words(bad), NOW, 1).is_err(), "{bad}");
        }
        assert_eq!(parse(&words("rare x20"), NOW, 1).unwrap().len(), 20);
    }

    #[test]
    fn the_listing_names_every_tier_and_says_nothing_is_sent() {
        let listing = listing();
        for tier in &TIERS {
            assert!(listing.iter().any(|line| line.contains(tier.id)));
        }
        assert!(listing.iter().any(|line| line.contains("Nothing is sent")));
        assert!(COMMAND.starts_with("debug_") && COMMAND == COMMAND.to_ascii_lowercase());
        let lines = queued(&parse(&words("rare x2"), NOW, 1).unwrap());
        assert_eq!(lines[0], "Rehearsing: Rare Holocron, Rare Holocron.");
    }
}
