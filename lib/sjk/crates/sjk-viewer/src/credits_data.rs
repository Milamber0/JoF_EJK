//! Parsers for `assets/credits.txt` and `assets/credits_history.txt`, built
//! into the client and shown by the credits page (`credits.rs`). The files'
//! headers give their formats; the tests here check the real files.

use crate::medals::Medal;

/// The built-in credits.
pub(super) const EMBEDDED: &str = include_str!("../assets/credits.txt");
/// Everyone's features, pull requests and commits, written by
/// `scripts/credits_history.py`.
pub(super) const HISTORY: &str = include_str!("../assets/credits_history.txt");

/// Where the commits and pull requests open.
const SJK_COMMIT: &str = "https://github.com/Sol-Vulpes/SJK/commit/";
const SJK_PULL: &str = "https://github.com/Sol-Vulpes/SJK/pull/";
const JKR_PULL: &str = "https://github.com/Bishop-R/JKR/pull/";

#[derive(Debug)]
pub(super) struct Section {
    pub(super) title: String,
    pub(super) cards: Vec<Card>,
}

/// One person (or project) of a section.
#[derive(Debug)]
pub(super) struct Card {
    pub(super) name: String,
    /// "@handle", or empty.
    pub(super) github: String,
    pub(super) role: String,
    pub(super) did: Vec<String>,
    pub(super) links: Vec<Link>,
    /// The medals the SJK team gave them (`medal:` lines), in the catalogue's
    /// order, each with how often it was given (more than once only for a
    /// repeatable one).
    pub(super) medals: Vec<(Medal, u32)>,
    /// Their work from the history, newest first; empty for a card without.
    pub(super) work: Vec<Work>,
}

/// A feature or pull request, as merged into main.
#[derive(Debug)]
pub(super) struct Work {
    /// `dd/mm/yyyy` of its merge.
    pub(super) date: String,
    /// "SJK #2" or "JKR #48"; empty for a feature without a pull request.
    pub(super) reference: String,
    pub(super) title: String,
    /// Oldest first.
    pub(super) commits: Vec<Commit>,
}

#[derive(Debug)]
pub(super) struct Commit {
    /// The short hash.
    pub(super) hash: String,
    /// Who made it when that is not the work's owner, else empty.
    pub(super) by: String,
    pub(super) subject: String,
}

impl Work {
    /// The pull request's page, when it is one.
    pub(super) fn url(&self) -> Option<String> {
        let (repo, number) = self.reference.split_once(" #")?;
        let base = match repo {
            "SJK" => SJK_PULL,
            "JKR" => JKR_PULL,
            _ => return None,
        };
        Some(format!("{base}{number}"))
    }

    /// One commit, which says what the title says: nothing to unfold.
    pub(super) fn is_single(&self) -> bool {
        self.commits.len() == 1
    }
}

impl Commit {
    pub(super) fn url(&self) -> String {
        format!("{SJK_COMMIT}{}", self.hash)
    }
}

/// How much a card's history holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Tally {
    pub(super) work: usize,
    pub(super) pulls: usize,
    pub(super) commits: usize,
}

impl Card {
    pub(super) fn tally(&self) -> Tally {
        Tally {
            work: self.work.len(),
            pulls: self.work.iter().filter(|w| !w.reference.is_empty()).count(),
            commits: self.work.iter().map(|w| w.commits.len()).sum(),
        }
    }
}

/// A clickable address on a card.
#[derive(Debug)]
pub(super) struct Link {
    /// What the card shows: the label, or the address without `https://`.
    pub(super) label: String,
    pub(super) url: String,
}

impl Card {
    /// The GitHub profile behind `github`, if there is one.
    pub(super) fn github_url(&self) -> Option<String> {
        let handle = self.github.strip_prefix('@')?;
        Some(format!("https://github.com/{handle}"))
    }
}

/// `https://...` or `Label | https://...`.
fn parse_link(value: &str) -> Option<Link> {
    let (label, url) = match value.split_once('|') {
        Some((label, url)) => (label.trim(), url.trim()),
        None => {
            let url = value.trim();
            (
                url.trim_start_matches("https://").trim_end_matches('/'),
                url,
            )
        }
    };
    let host = url.strip_prefix("https://")?;
    if label.is_empty() || host.is_empty() || url.contains(char::is_whitespace) {
        return None;
    }
    Some(Link {
        label: label.to_owned(),
        url: url.to_owned(),
    })
}

/// Add the medal `id` names to a card's `medals`, kept in the catalogue's order: a
/// repeatable medal given again counts up, any other one listed twice is an error,
/// as is an id the client does not know.
fn add_medal(medals: &mut Vec<(Medal, u32)>, id: &str) -> Result<(), String> {
    let Some(medal) = Medal::from_id(id) else {
        let known: Vec<_> = Medal::ALL.iter().map(|medal| medal.id()).collect();
        return Err(format!(
            "unknown medal {id:?}, expected one of {}",
            known.join(", ")
        ));
    };
    match medals.iter_mut().find(|(held, _)| *held == medal) {
        Some((_, count)) if medal.repeatable() => *count += 1,
        Some(_) => return Err(format!("medal {id} is listed twice")),
        None => {
            medals.push((medal, 1));
            medals.sort_by_key(|(medal, _)| medal.index());
        }
    }
    Ok(())
}

pub(super) fn parse(text: &str) -> Result<Vec<Section>, String> {
    let mut sections: Vec<Section> = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let number = number + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !line.is_ascii() {
            return Err(format!("credits.txt line {number}: not ASCII"));
        }
        if let Some(title) = line.strip_prefix("==") {
            sections.push(Section {
                title: title.trim().to_owned(),
                cards: Vec::new(),
            });
            continue;
        }
        let Some(section) = sections.last_mut() else {
            return Err(format!(
                "credits.txt line {number}: text before the first \"==\" section"
            ));
        };
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            section.cards.push(Card {
                name: name.trim().to_owned(),
                github: String::new(),
                role: String::new(),
                did: Vec::new(),
                links: Vec::new(),
                medals: Vec::new(),
                work: Vec::new(),
            });
            continue;
        }
        let Some(card) = section.cards.last_mut() else {
            return Err(format!(
                "credits.txt line {number}: a key before the first [name]"
            ));
        };
        let Some((key, value)) = line.split_once(':') else {
            return Err(format!(
                "credits.txt line {number}: expected \"key: value\""
            ));
        };
        let value = value.trim().to_owned();
        match key.trim() {
            "github" => card.github = format!("@{}", value.trim_start_matches('@')),
            "role" => card.role = value,
            "did" => card.did.push(value),
            "link" => match parse_link(&value) {
                Some(link) => card.links.push(link),
                None => {
                    return Err(format!(
                        "credits.txt line {number}: a link is \"https://...\" or \"Label | https://...\""
                    ));
                }
            },
            "medal" => add_medal(&mut card.medals, &value)
                .map_err(|why| format!("credits.txt line {number}: {why}"))?,
            other => return Err(format!("credits.txt line {number}: unknown key {other:?}")),
        }
    }
    for section in &sections {
        if section.cards.is_empty() {
            return Err(format!(
                "credits.txt: section {:?} has no card",
                section.title
            ));
        }
        if let Some(card) = section.cards.iter().find(|card| card.role.is_empty()) {
            return Err(format!("credits.txt: {} has no role", card.name));
        }
    }
    Ok(sections)
}

/// Parse the history: each person's name and work, in the file's order.
pub(super) fn parse_history(text: &str) -> Result<Vec<(String, Vec<Work>)>, String> {
    let mut people: Vec<(String, Vec<Work>)> = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let number = number + 1;
        let error = |what: &str| format!("credits_history.txt line {number}: {what}");
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        if !line.is_ascii() {
            return Err(error("not ASCII"));
        }
        if let Some(name) = line.strip_prefix("= ") {
            people.push((name.trim().to_owned(), Vec::new()));
            continue;
        }
        let Some((_, work)) = people.last_mut() else {
            return Err(error("work before the first \"= Name\""));
        };
        if let Some(rest) = line.strip_prefix("+ ") {
            let mut fields = rest.splitn(3, " | ");
            let (Some(date), Some(reference), Some(title)) =
                (fields.next(), fields.next(), fields.next())
            else {
                return Err(error("expected \"+ dd/mm/yyyy | ref | title\""));
            };
            if !is_date(date) {
                return Err(error("the date is not dd/mm/yyyy"));
            }
            let reference = if reference == "-" { "" } else { reference };
            work.push(Work {
                date: date.to_owned(),
                reference: reference.to_owned(),
                title: title.trim().to_owned(),
                commits: Vec::new(),
            });
        } else if let Some(rest) = line.strip_prefix("- ") {
            let Some(piece) = work.last_mut() else {
                return Err(error("a commit before the first \"+\" line"));
            };
            let mut fields = rest.splitn(3, " | ");
            let (Some(hash), Some(by), Some(subject)) =
                (fields.next(), fields.next(), fields.next())
            else {
                return Err(error("expected \"- hash | by | subject\""));
            };
            if hash.len() < 7 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(error("not a commit hash"));
            }
            piece.commits.push(Commit {
                hash: hash.to_owned(),
                by: by.trim().to_owned(),
                subject: subject.trim().to_owned(),
            });
        } else {
            return Err(error("expected \"= \", \"+ \" or \"- \""));
        }
    }
    if let Some((name, _)) = people
        .iter()
        .find(|(_, work)| work.iter().any(|w| w.commits.is_empty()))
    {
        return Err(format!(
            "credits_history.txt: work of {name} without a commit"
        ));
    }
    Ok(people)
}

/// `dd/mm/yyyy`.
fn is_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes[2] == b'/'
        && bytes[5] == b'/'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 2 | 5) || byte.is_ascii_digit())
}

/// The credits with everyone's history under their card. A person in the
/// history without a card is an error, so the script and credits.txt agree.
pub(super) fn parse_all(credits: &str, history: &str) -> Result<Vec<Section>, String> {
    let mut sections = parse(credits)?;
    for (name, work) in parse_history(history)? {
        let Some(card) = sections
            .iter_mut()
            .flat_map(|section| &mut section.cards)
            .find(|card| card.name == name)
        else {
            return Err(format!(
                "credits_history.txt: {name} has work but no card in credits.txt"
            ));
        };
        card.work = work;
    }
    Ok(sections)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_credits_parse_with_the_team_first() {
        let sections = parse(EMBEDDED).unwrap();
        let team = &sections[0];
        let names: Vec<_> = team.cards.iter().map(|card| card.name.as_str()).collect();
        assert_eq!(names, ["Sol"]);
        assert_eq!(team.cards[0].github, "@Sol-Vulpes");
        assert_eq!(
            team.cards[0].github_url().as_deref(),
            Some("https://github.com/Sol-Vulpes")
        );
        let cards: Vec<_> = sections.iter().flat_map(|section| &section.cards).collect();
        for name in ["Bishop", "Creyon", "Lumaya"] {
            assert!(cards.iter().any(|card| card.name == name), "{name} missing");
        }
        assert!(cards.iter().flat_map(|card| &card.links).count() > 0);
        // Both contributors with merged pull requests wear Early Contributor.
        let card = |name: &str| *cards.iter().find(|card| card.name == name).unwrap();
        for name in ["Creyon", "Lumaya"] {
            assert_eq!(card(name).medals, [(Medal::EarlyContributor, 1)], "{name}");
        }
        assert_eq!(card("Lumaya").github, "@lumayaa");
        assert!(card("Sol").medals.is_empty());
    }

    #[test]
    fn medals_are_known_ids_in_the_catalogues_order() {
        let text =
            "== A\n[X]\nrole: r\nmedal: bug_hunter\nmedal: early_contributor\nmedal: bug_hunter\n";
        let sections = parse(text).unwrap();
        assert_eq!(
            sections[0].cards[0].medals,
            [(Medal::EarlyContributor, 1), (Medal::BugHunter, 2)]
        );
        for bad in [
            "medal: from_the_future",
            "medal: Early_Contributor",
            "medal: early contributor",
            "medal:",
            "medal: early_tester\nmedal: early_tester",
        ] {
            let text = format!("== A\n[X]\nrole: r\n{bad}\n");
            let error = parse(&text).expect_err(bad);
            assert!(error.starts_with("credits.txt line "), "{error}");
        }
        assert!(
            parse("== A\n[X]\nrole: r\nmedal: nope\n")
                .unwrap_err()
                .contains("early_tester, early_contributor, bug_hunter, supporter")
        );
    }

    #[test]
    fn the_built_in_history_belongs_to_cards() {
        let sections = parse_all(EMBEDDED, HISTORY).unwrap();
        let cards: Vec<_> = sections.iter().flat_map(|section| &section.cards).collect();
        let card = |name: &str| *cards.iter().find(|card| card.name == name).unwrap();
        let sol = card("Sol").tally();
        assert!(
            sol.work > 100 && sol.commits > sol.work && sol.pulls > 10,
            "{sol:?}"
        );
        // Bishop's JKR pull requests, Creyon's SJK ones, each opening on GitHub.
        let bishop = &card("Bishop").work;
        assert!(bishop.iter().any(|w| w.reference.starts_with("JKR #")));
        let creyon = &card("Creyon").work;
        assert!(creyon.iter().all(|w| w.reference.starts_with("SJK #")));
        let first = creyon.last().unwrap();
        assert_eq!(first.reference, "SJK #2");
        assert_eq!(
            first.url().as_deref(),
            Some("https://github.com/Sol-Vulpes/SJK/pull/2")
        );
        // The history is newest first, by day.
        let day = |date: &str| {
            let (d, rest) = date.split_once('/').unwrap();
            let (m, y) = rest.split_once('/').unwrap();
            format!("{y}{m}{d}")
        };
        for person in &cards {
            for pair in person.work.windows(2) {
                assert!(day(&pair[0].date) >= day(&pair[1].date), "{}", person.name);
            }
        }
        assert!(card("Claude").work.is_empty());
        assert!(!card("Lumaya").work.is_empty());
    }

    #[test]
    fn history_lines_parse_and_errors_name_their_line() {
        let text = "# note\n= A\n+ 07/10/2026 | SJK #24 | Clip | blades\n- 8d21001 |  | Clip\n- abcdef0 | B | Fix\n+ 01/10/2026 | - | One\n- 0123456 |  | One\n";
        let people = parse_history(text).unwrap();
        let (name, work) = &people[0];
        assert_eq!(name, "A");
        assert_eq!(work[0].title, "Clip | blades");
        assert_eq!(work[0].reference, "SJK #24");
        assert_eq!(work[0].commits[1].by, "B");
        assert_eq!(work[0].commits[0].by, "");
        assert_eq!(
            work[0].commits[0].url(),
            "https://github.com/Sol-Vulpes/SJK/commit/8d21001"
        );
        assert!(!work[0].is_single());
        assert!(work[1].reference.is_empty() && work[1].url().is_none());
        assert!(work[1].is_single());
        for bad in [
            "+ 07/10/2026 | - | x\n",
            "= A\n+ 2026-10-07 | - | x\n- 0123456 |  | x\n",
            "= A\n+ 07/10/2026 | - x\n",
            "= A\n- 0123456 |  | x\n",
            "= A\n+ 07/10/2026 | - | x\n- xyz | | x\n",
            "= A\n+ 07/10/2026 | - | x\n",
            "= A\nstray\n",
        ] {
            assert!(parse_history(bad).is_err(), "{bad}");
        }
        assert!(
            parse_all(
                "== S\n[A]\nrole: r\n",
                "= B\n+ 07/10/2026 | - | x\n- 0123456 |  | x\n"
            )
            .is_err()
        );
    }

    #[test]
    fn format_errors_name_their_line() {
        let ok = "== A\n[X]\ngithub: @x\nrole: r\ndid: one\ndid: two\n";
        let sections = parse(ok).unwrap();
        assert_eq!(sections[0].cards[0].github, "@x");
        assert_eq!(sections[0].cards[0].did, ["one", "two"]);
        assert!(parse("[X]\nrole: r\n").is_err());
        assert!(parse("== A\nrole: r\n").is_err());
        assert!(parse("== A\n[X]\nrank: r\n").is_err());
        assert!(parse("== A\n[X]\ndid: no role\n").is_err());
        assert!(parse("== A\n").is_err());
    }

    #[test]
    fn links_are_https_with_an_optional_label() {
        let card =
            "== A\n[X]\nrole: r\nlink: https://a.example/b/\nlink: Pulls | https://c.example\n";
        let sections = parse(card).unwrap();
        let links = &sections[0].cards[0].links;
        assert_eq!(links[0].label, "a.example/b");
        assert_eq!(links[0].url, "https://a.example/b/");
        assert_eq!(links[1].label, "Pulls");
        assert_eq!(links[1].url, "https://c.example");
        assert!(sections[0].cards[0].github_url().is_none());
        for bad in [
            "http://a.example",
            "Label | ftp://a",
            "https://",
            " | https://a",
            "https://a b",
        ] {
            let text = format!("== A\n[X]\nrole: r\nlink: {bad}\n");
            assert!(parse(&text).is_err(), "{bad}");
        }
    }
}
