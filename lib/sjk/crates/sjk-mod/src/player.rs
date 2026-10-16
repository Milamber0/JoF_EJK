//! Players as a mod sees them, and finding one from what was typed.

/// A player in the roster.
#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub client: u16,
    /// The name with its colour codes.
    pub name: String,
    /// Where the player is drawn, when in the received view.
    pub drawn: Option<Drawn>,
}

/// A player as the client draws it this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drawn {
    /// The interpolated origin (`lerpOrigin`).
    pub origin: [f32; 3],
    /// The interpolated angles (`lerpAngles`), degrees.
    pub angles: [f32; 3],
    /// The height of the snapshot's position (`pos.trBase[2]`).
    pub base_z: f32,
    /// Milliseconds since that position was sent (`cg.time - pos.trTime`).
    pub age_ms: i32,
}

/// `text` without `^N` colour codes, in lower case.
pub fn strip_colours(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '^' && characters.peek().is_some_and(char::is_ascii_alphanumeric) {
            characters.next();
            continue;
        }
        out.extend(character.to_lowercase());
    }
    out
}

/// The client slot `query` names: a slot number in the roster, then a name
/// (colours and case ignored), then a fragment found in exactly one name.
pub fn resolve_player(players: &[Player], query: &str) -> Result<u16, String> {
    if let Ok(slot) = query.trim().parse::<u16>()
        && players.iter().any(|player| player.client == slot)
    {
        return Ok(slot);
    }
    let query = strip_colours(query.trim());
    if query.is_empty() {
        return Err("enter a player name or client number".into());
    }
    let exact: Vec<_> = players
        .iter()
        .filter(|player| strip_colours(&player.name) == query)
        .collect();
    let found = if exact.is_empty() {
        players
            .iter()
            .filter(|player| strip_colours(&player.name).contains(&query))
            .collect()
    } else {
        exact
    };
    match found.as_slice() {
        [player] => Ok(player.client),
        [] => Err(format!("no player matches \"{query}\"")),
        _ => Err(format!(
            "\"{query}\" matches several players; use a client number"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roster() -> Vec<Player> {
        ["^1Sol", "Solo^7Cup", "Kyle"]
            .iter()
            .zip([3, 7, 12])
            .map(|(name, client)| Player {
                client,
                name: (*name).into(),
                drawn: None,
            })
            .collect()
    }

    #[test]
    fn finds_slots_names_and_unique_fragments() {
        let players = roster();
        assert_eq!(resolve_player(&players, "12"), Ok(12));
        assert_eq!(resolve_player(&players, "sol"), Ok(3));
        assert_eq!(resolve_player(&players, "^5CUP"), Ok(7));
        assert_eq!(resolve_player(&players, "ky"), Ok(12));
        assert!(resolve_player(&players, "o").is_err());
        assert!(resolve_player(&players, "jan").is_err());
        // A number that is no one's slot is read as a name.
        assert!(resolve_player(&players, "5").is_err());
    }

    #[test]
    fn strips_colour_codes() {
        assert_eq!(strip_colours("^1R^7ed^"), "red^");
    }
}
