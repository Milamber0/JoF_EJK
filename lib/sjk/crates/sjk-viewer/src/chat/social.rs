//! Local name bookmarks, not authenticated accounts. Disk work runs at first
//! configuration and explicit clicks only; the feed borrows the cached set.

use std::collections::BTreeSet;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const MAX_FRIENDS: usize = 1024;
const MAX_NAME_BYTES: usize = 256;
const MAX_FILE_BYTES: u64 = (MAX_FRIENDS * (MAX_NAME_BYTES + 1)) as u64;

#[derive(Default)]
pub(super) struct Friends {
    path: Option<PathBuf>,
    names: BTreeSet<String>,
    load_failed: bool,
}

impl Friends {
    pub(super) fn initialize(&mut self, directory: &Path) {
        if self.path.is_some() {
            return;
        }
        let path = directory.join("chat-friends.txt");
        match read(&path) {
            Ok(names) => self.names = names,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                self.load_failed = true;
                eprintln!("Could not load chat friends: {error}");
            }
        }
        self.path = Some(path);
    }

    pub(super) fn contains(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    /// Save before applying a change, so a failed write leaves the visible state intact.
    pub(super) fn toggle(&mut self, name: &str) -> Result<bool, &'static str> {
        let Some(path) = &self.path else {
            return Err("Friend storage is unavailable.");
        };
        if self.load_failed {
            return Err("Could not read friends; existing file left untouched.");
        }
        if !valid_name(name) {
            return Err("This name cannot be saved as a friend.");
        }
        let adding = !self.contains(name);
        if adding && self.names.len() >= MAX_FRIENDS {
            return Err("Friend list is full.");
        }
        let mut updated = self.names.clone();
        if adding {
            updated.insert(name.to_owned());
        } else {
            updated.remove(name);
        }
        if let Err(error) = save(path, &updated) {
            eprintln!("Could not save chat friends: {error}");
            return Err("Could not save friends. No change made.");
        }
        self.names = updated;
        Ok(adding)
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= MAX_NAME_BYTES && !name.chars().any(char::is_control)
}

fn read(path: &Path) -> io::Result<BTreeSet<String>> {
    let mut text = String::new();
    std::fs::File::open(path)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > MAX_FILE_BYTES
        || text.lines().count() > MAX_FRIENDS
        || !text.lines().all(valid_name)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid friend list",
        ));
    }
    Ok(text.lines().map(str::to_owned).collect())
}

fn save(path: &Path, names: &BTreeSet<String>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pending = path.with_extension("txt.pending");
    let result = (|| {
        let mut file = std::fs::File::create(&pending)?;
        for name in names {
            writeln!(file, "{name}")?;
        }
        file.sync_all()?;
        drop(file);
        std::fs::rename(&pending, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(pending);
    }
    result
}
