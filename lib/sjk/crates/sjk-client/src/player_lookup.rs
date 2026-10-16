//! Naming a player by name instead of slot, for client-side `tell` targets.
//!
//! EternalJK `codemp/cgame/cg_consolecmds.c:357` (`CG_ClientNumberFromString`)
//! resolves a typed name before the stock `tell <slot> <message>` command is
//! sent: colour codes are ignored, case does not matter, and a unique partial
//! name is accepted while several matches are refused. An exact name wins over
//! names that merely contain it, so `bob` still reaches `Bob` beside `Bobby`.

/// Outcome of resolving a typed player name against the connected players.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlayerLookup {
    /// Exactly one player is meant.
    Found(u16),
    /// No connected player's name contains the query.
    NotFound,
    /// Several players match; each candidate's slot and display name.
    Ambiguous(Vec<(u16, String)>),
}

/// Resolve `query` against `(slot, plain name, display name)` entries, where the
/// plain name has its colour codes removed and an empty plain name is a free slot.
pub fn lookup_player<'a>(
    players: impl IntoIterator<Item = (u16, &'a str, &'a str)>,
    query: &str,
) -> PlayerLookup {
    let wanted = crate::chat_name_key(query).trim().to_lowercase();
    if wanted.is_empty() {
        return PlayerLookup::NotFound;
    }
    let mut exact = Vec::new();
    let mut partial = Vec::new();
    for (slot, plain, display) in players {
        if plain.is_empty() {
            continue;
        }
        let name = plain.to_lowercase();
        if name == wanted {
            exact.push(slot);
        }
        if name.contains(&wanted) {
            partial.push((slot, display.to_owned()));
        }
    }
    if let [slot] = exact.as_slice() {
        return PlayerLookup::Found(*slot);
    }
    match partial.as_slice() {
        [] => PlayerLookup::NotFound,
        [(slot, _)] => PlayerLookup::Found(*slot),
        _ => PlayerLookup::Ambiguous(partial),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAYERS: [(u16, &str, &str); 4] = [
        (0, "Padawan", "^1Pada^7wan"),
        (3, "Bob", "^2Bob"),
        (7, "Bobby", "Bobby"),
        (9, "", ""),
    ];

    #[test]
    fn unique_partial_name_ignores_case_and_colours() {
        assert_eq!(lookup_player(PLAYERS, "PADA"), PlayerLookup::Found(0));
        assert_eq!(lookup_player(PLAYERS, "^3d^5aw"), PlayerLookup::Found(0));
        assert_eq!(lookup_player(PLAYERS, "bby"), PlayerLookup::Found(7));
    }

    #[test]
    fn exact_name_wins_over_longer_matches() {
        assert_eq!(lookup_player(PLAYERS, "bob"), PlayerLookup::Found(3));
    }

    #[test]
    fn several_partial_matches_are_listed() {
        assert_eq!(
            lookup_player(PLAYERS, "bo"),
            PlayerLookup::Ambiguous(vec![(3, "^2Bob".into()), (7, "Bobby".into())])
        );
    }

    #[test]
    fn missing_or_empty_names_match_nobody() {
        assert_eq!(lookup_player(PLAYERS, "kyle"), PlayerLookup::NotFound);
        assert_eq!(lookup_player(PLAYERS, "^1"), PlayerLookup::NotFound);
    }

    #[test]
    fn duplicate_exact_names_stay_ambiguous() {
        let players = [(1, "Jan", "Jan"), (2, "jan", "^4jan")];
        assert_eq!(
            lookup_player(players, "JAN"),
            PlayerLookup::Ambiguous(vec![(1, "Jan".into()), (2, "^4jan".into())])
        );
    }

    #[test]
    fn names_with_windows_1252_symbols_and_hidden_codes_resolve() {
        // Roster keys from bytes 0x92 and a vertical tab (see `chat_name_key`).
        let raw = ["^1Ja\u{92}son", "{JoF}\u{b}Toxiee\u{b}{C}.ak"];
        let keys = raw.map(crate::chat_name_key);
        let players = [(1, keys[0].as_str(), raw[0]), (2, keys[1].as_str(), raw[1])];
        assert_eq!(lookup_player(players, "Ja’son"), PlayerLookup::Found(1));
        assert_eq!(
            lookup_player(players, "{JoF}Toxiee{C}.ak"),
            PlayerLookup::Found(2)
        );
        assert_eq!(lookup_player(players, "toxiee"), PlayerLookup::Found(2));
    }
}
