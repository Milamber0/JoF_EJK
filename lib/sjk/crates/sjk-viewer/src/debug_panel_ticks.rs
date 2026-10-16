//! The debug panel's "tested" ticks, saved beside `config.cfg` in
//! `debug_panel_tested.txt`: one entry id per line, so reordering or rewording the
//! test list keeps them. Ids of entries no longer in the list are kept as they are.

use std::io::{self, ErrorKind};
use std::path::Path;

/// File name of the ticks, in the configuration directory.
pub(super) const FILE_NAME: &str = "debug_panel_tested.txt";

const HEADER: &str = "# Entries ticked as tested in the debug_panel test list (Sol's build).\n\
                      # One entry id per line; ids come from assets/debug_panel.txt.\n";

/// Ids listed in a ticks file; comments and blank lines are skipped.
pub(super) fn parse(text: &str) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for line in text.lines().map(str::trim) {
        if !line.is_empty() && !line.starts_with('#') && !ids.iter().any(|id| id == line) {
            ids.push(line.to_owned());
        }
    }
    ids
}

/// The file contents for `ids`, in the order given.
pub(super) fn render<'a>(ids: impl IntoIterator<Item = &'a str>) -> String {
    let mut text = String::from(HEADER);
    for id in ids {
        text.push_str(id);
        text.push('\n');
    }
    text
}

/// Read the ticked ids; a missing file means nothing is ticked yet.
pub(super) fn load(path: &Path) -> io::Result<Vec<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(parse(&text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

/// Replace the ticks file through a temporary file, so a failed write keeps the old one.
pub(super) fn save(path: &Path, contents: &str) -> io::Result<()> {
    let temporary = path.with_extension("txt.tmp");
    std::fs::write(&temporary, contents)?;
    std::fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_round_trip_through_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);
        assert!(load(&path).unwrap().is_empty());
        save(&path, &render(["noclip", "dead-keys"])).unwrap();
        assert_eq!(load(&path).unwrap(), ["noclip", "dead-keys"]);
        save(&path, &render(["dead-keys"])).unwrap();
        assert_eq!(load(&path).unwrap(), ["dead-keys"]);
    }

    #[test]
    fn parse_skips_comments_blanks_and_repeats() {
        let text = "# comment\n\n  noclip  \r\nnoclip\ndead-keys\n";
        assert_eq!(parse(text), ["noclip", "dead-keys"]);
    }
}
