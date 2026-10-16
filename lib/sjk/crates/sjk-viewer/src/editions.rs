//! Editions: the seasons of everything collectible (14/10/2026, Sol's request). Each
//! collectible belongs to one edition, the stamp of when it came: **Alpha** for the
//! whole alpha, and later editions as they open. Holocron cards already carry theirs
//! (the hub stamps a copy at opening, [`crate::cards`]); the shaders, medals,
//! achievements and toys take theirs from their catalogue entries here in the client.
//! One table says which editions exist and how each reads, so a new edition is one
//! line in [`ALL`] and the items put in it name its id. Old ones remain (an item keeps
//! its edition for good).

/// The alpha's edition, the first.
pub(crate) const ALPHA: &str = "alpha";

/// One edition: its id (the hub's, as a card carries it) and how players read it.
#[derive(Debug)]
pub(crate) struct Edition {
    pub(crate) id: &'static str,
    /// The full stamp: "Alpha Edition".
    pub(crate) name: &'static str,
}

/// Every edition the client knows, the oldest first.
pub(crate) const ALL: [Edition; 1] = [Edition {
    id: ALPHA,
    name: "Alpha Edition",
}];

/// The edition `id` names, if the client knows it.
pub(crate) fn find(id: &str) -> Option<&'static Edition> {
    ALL.iter().find(|edition| edition.id == id)
}

/// What players read for edition `id`: "Alpha Edition"; an id the client does not know
/// reads as itself with "Edition" after it, and none as nothing.
pub(crate) fn name(id: &str) -> String {
    match (id, find(id)) {
        ("", _) => String::new(),
        (_, Some(edition)) => edition.name.to_owned(),
        (other, None) => {
            let mut chars = other.chars();
            let first = chars.next().map(|c| c.to_ascii_uppercase());
            format!("{}{} Edition", first.unwrap_or(' '), chars.as_str())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_edition_reads_in_full() {
        assert_eq!(name("alpha"), "Alpha Edition");
        assert_eq!(name("zeta"), "Zeta Edition");
        assert_eq!(name(""), String::new());
    }

    #[test]
    fn every_item_names_an_edition_the_table_has() {
        for skin in crate::unlockables::ALL.iter() {
            assert!(find(skin.edition).is_some(), "{}", skin.id);
        }
        for medal in crate::medals::Medal::ALL {
            assert!(find(medal.edition()).is_some(), "{medal:?}");
        }
        for kind in &crate::achievements::ALL {
            assert!(find(kind.edition).is_some(), "{}", kind.id);
        }
        for (at, edition) in ALL.iter().enumerate() {
            assert!(
                ALL[..at].iter().all(|other| other.id != edition.id),
                "{}",
                edition.id
            );
        }
    }
}
