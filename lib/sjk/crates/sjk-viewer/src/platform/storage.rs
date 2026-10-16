//! Writable client files: the installation's `GameData/SJK`, else the per-user folder.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// SJK's client folder in `GameData`.
const CLIENT_FOLDER: &str = "SJK";

/// The configuration path shared by the console, menus and storage consumers.
pub(super) struct Selection {
    pub(super) config: PathBuf,
    pub(super) fallback_reason: Option<io::Error>,
}

/// Prefer the installation's own client folder; probe actual file creation
/// rather than permission bits, which do not describe Windows ACLs reliably.
pub(super) fn select(game_data: &Path, per_user_config: Option<&Path>) -> io::Result<Selection> {
    let portable = game_data.join(CLIENT_FOLDER);
    match writable_directory(&portable) {
        Ok(()) => Ok(Selection {
            config: portable.join("config.cfg"),
            fallback_reason: None,
        }),
        Err(reason) => {
            let config = per_user_config.ok_or_else(|| {
                io::Error::other(format!(
                    "cannot write {}: {reason}; no user storage location available",
                    portable.display()
                ))
            })?;
            let directory = config
                .parent()
                .ok_or_else(|| io::Error::other("config has no parent"))?;
            writable_directory(directory)?;
            Ok(Selection {
                config: config.to_owned(),
                fallback_reason: Some(reason),
            })
        }
    }
}

fn writable_directory(directory: &Path) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    let mut probe = tempfile::NamedTempFile::new_in(directory)?;
    probe.write_all(b"SJK")?;
    probe.close()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_writable_sjk_folder_holds_the_config() {
        let root = tempfile::tempdir().unwrap();
        let game_data = root.path().join("GameData");
        let per_user = root.path().join("per-user");
        let selection = select(&game_data, Some(&per_user.join("config.cfg"))).unwrap();
        assert_eq!(
            selection.config,
            game_data.join(CLIENT_FOLDER).join("config.cfg")
        );
        assert!(selection.fallback_reason.is_none());
        assert!(!per_user.exists());
    }
}
