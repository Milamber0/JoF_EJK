//! Which of the player's own holocrons the drop pop-up has shown
//! (`holocron_popup.rs`): `holocrons_seen.txt` in the settings folder, beside
//! `identity.key`. Holocron numbers at the hub only rise, so the file keeps the highest
//! one shown; a holocron with a higher number is new. The file names the key it belongs
//! to, so a new identity sees its own. With no file for the key (a reinstall, a new PC),
//! everything the profile holds is new, as medals are: the pop-up shows the newest
//! [`super::FIRST_READ_MAX`] of them once.
//!
//! ```text
//! # Holocrons SJK has shown this identity: the highest holocron number shown.
//! key 0123456789abcdef
//! highest 41
//! ```

use std::path::Path;

/// The file, in the settings folder.
pub(crate) const FILE: &str = "holocrons_seen.txt";
const HEADER: &str = "# Holocrons SJK has shown this identity: the highest holocron number shown.";

/// What has been shown to one key.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Seen {
    key_id: String,
    /// The highest holocron number shown; 0 when none.
    highest: u64,
    /// A file for this key was read, or a holocron was marked since: the pop-up has
    /// been through this identity's holocrons before.
    known: bool,
}

impl Seen {
    /// What `text` says was shown to `key_id`; nothing, and not known, when it is
    /// another key's or unreadable.
    pub(crate) fn parse(text: &str, key_id: &str) -> Self {
        let mut seen = Self {
            key_id: key_id.to_owned(),
            ..Self::default()
        };
        let mut lines = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'));
        if lines.next().and_then(|line| line.strip_prefix("key ")) != Some(key_id) {
            return seen;
        }
        seen.known = true;
        for line in lines {
            if let Some(number) = line
                .strip_prefix("highest ")
                .and_then(|number| number.trim().parse::<u64>().ok())
            {
                seen.highest = number;
            }
        }
        seen
    }

    /// The file `directory` holds for `key_id`.
    pub(crate) fn load(directory: &Path, key_id: &str) -> Self {
        let text = std::fs::read_to_string(directory.join(FILE)).unwrap_or_default();
        Self::parse(&text, key_id)
    }

    /// The key these numbers belong to.
    pub(crate) fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Whether this identity's holocrons have been through the pop-up before.
    #[cfg(test)]
    pub(crate) fn known(&self) -> bool {
        self.known
    }

    /// The highest number shown.
    #[cfg(test)]
    pub(crate) fn highest(&self) -> u64 {
        self.highest
    }

    /// The file's text.
    pub(crate) fn text(&self) -> String {
        format!("{HEADER}\nkey {}\nhighest {}\n", self.key_id, self.highest)
    }

    /// Write the file into `directory`.
    pub(crate) fn save(&self, directory: &Path) -> std::io::Result<()> {
        std::fs::write(directory.join(FILE), self.text())
    }

    /// Whether holocron `id` is new: above everything shown.
    pub(crate) fn is_new(&self, id: u64) -> bool {
        id > self.highest
    }

    /// Count holocron `id`, and everything before it, as shown.
    pub(crate) fn mark(&mut self, id: u64) {
        self.highest = self.highest.max(id);
        self.known = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_holocron_is_new_until_it_or_a_later_one_is_marked() {
        let mut seen = Seen::parse("", "aa");
        assert!(!seen.known(), "no file: everything held is new");
        assert!(seen.is_new(1));
        seen.mark(7);
        assert!(seen.known());
        assert!(!seen.is_new(7) && !seen.is_new(3));
        assert!(seen.is_new(8));
        seen.mark(2);
        assert_eq!(seen.highest(), 7, "it never goes back");
    }

    #[test]
    fn the_file_round_trips_and_belongs_to_one_key() {
        let seen = Seen::parse("# x\nkey aa\nhighest 41\nfuture 9\n", "aa");
        assert!(seen.known());
        assert_eq!(seen.highest(), 41);
        assert_eq!(Seen::parse(&seen.text(), "aa"), seen);
        assert_eq!(seen.text().lines().count(), 3);
        let other = Seen::parse("key aa\nhighest 41\n", "bb");
        assert!(!other.known() && other.highest() == 0, "another key's file");
        assert_eq!(other.key_id(), "bb");
        // Damage reads as nothing shown, not a crash or a wrong number.
        for broken in ["highest 5\n", "key aa\nhighest many\n", "\u{0}\u{1}"] {
            let read = Seen::parse(broken, "aa");
            assert_eq!(read.highest(), 0, "{broken:?}");
        }
    }

    #[test]
    fn the_file_is_saved_in_the_settings_folder() {
        let directory = tempfile::tempdir().expect("a folder");
        let mut seen = Seen::load(directory.path(), "aa");
        assert!(!seen.known());
        seen.mark(12);
        seen.save(directory.path()).expect("saved");
        let loaded = Seen::load(directory.path(), "aa");
        assert!(loaded.known() && !loaded.is_new(12) && loaded.is_new(13));
        assert!(directory.path().join(FILE).exists());
    }
}
