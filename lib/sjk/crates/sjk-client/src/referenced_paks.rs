//! Server-advertised content references, shared by downloading and mounting.

/// One positionally paired server pak name and checksum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferencedPak<'a> {
    /// Server virtual path without the `.pk3` extension; validate before downloading.
    pub name: &'a str,
    /// Unseeded archive checksum, not the pure-server proof checksum.
    pub checksum: i32,
}

/// Pair the common prefix of the server's whitespace-separated reference lists.
///
/// OpenJK codemp `FS_PureServerSetReferencedPaks` uses the shorter list. Some
/// servers omit names while still advertising checksums; these unnamed entries
/// are not downloadable references. Never shift a name onto a later checksum.
/// Keeps a bounded inventory and strict validation of paired checksums;
/// callers remain responsible for path and downloaded-content validation.
///
/// Stock `FS_ReferencedPakNames` writes every name as `game/basename` and joins
/// them with spaces, so a pak called `{JoF}Sentinel Nexus.pk3` arrives as two
/// tokens and pushes every later name onto its neighbour's checksum (JoF's
/// servers sent 21 names for 20 checksums). A token without a `/` cannot start
/// a stock name: it is the rest of the previous one, rejoined here as the
/// original slice of `names`.
pub fn parse<'a>(checksums: &str, names: &'a str) -> Result<Vec<ReferencedPak<'a>>, &'static str> {
    let mut references = Vec::new();
    for (name, sum) in split_names(names).zip(checksums.split_whitespace()) {
        if references.len() == 1024 {
            return Err("too many referenced paks (limit 1024)");
        }
        references.push(ReferencedPak {
            name,
            checksum: sum.parse().map_err(|_| "invalid referenced checksum")?,
        });
    }
    Ok(references)
}

/// The names in `names`, a token without `/` joined to a previous `game/` name.
fn split_names(names: &str) -> impl Iterator<Item = &str> {
    let mut tokens = names
        .split_whitespace()
        .map(|token| {
            let start = token.as_ptr() as usize - names.as_ptr() as usize;
            (start, start + token.len())
        })
        .peekable();
    std::iter::from_fn(move || {
        let (start, mut end) = tokens.next()?;
        if names[start..end].contains('/') {
            while let Some(&(_, next_end)) = tokens
                .peek()
                .filter(|&&(next, next_end)| !names[next..next_end].contains('/'))
            {
                end = next_end;
                tokens.next();
            }
        }
        Some(&names[start..end])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_with_a_space_keeps_its_checksum_and_the_rest_theirs() {
        let references = parse(
            "1 2 3",
            "base/{JoF}Shuttle base/{JoF}Sentinel Nexus base/{JoF}Nebuchadnezzar",
        )
        .unwrap();
        let pairs: Vec<_> = references.iter().map(|r| (r.name, r.checksum)).collect();
        assert_eq!(
            pairs,
            [
                ("base/{JoF}Shuttle", 1),
                ("base/{JoF}Sentinel Nexus", 2),
                ("base/{JoF}Nebuchadnezzar", 3),
            ]
        );
    }

    #[test]
    fn several_spaces_stay_in_the_name() {
        let references = parse("7", "base/A  B C").unwrap();
        assert_eq!(references[0].name, "base/A  B C");
    }

    #[test]
    fn names_without_a_game_directory_are_not_joined() {
        let references = parse("1 2", "first second").unwrap();
        let names: Vec<_> = references.iter().map(|r| r.name).collect();
        assert_eq!(names, ["first", "second"]);
    }

    #[test]
    fn the_shorter_list_still_bounds_the_pairs() {
        assert_eq!(parse("1 2", "base/a").unwrap().len(), 1);
        assert_eq!(parse("1", "base/a base/b").unwrap().len(), 1);
        assert!(parse("1", "").unwrap().is_empty());
    }
}
