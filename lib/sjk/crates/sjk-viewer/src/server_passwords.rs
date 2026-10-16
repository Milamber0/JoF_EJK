//! Passwords of locked servers the player chose to remember on this PC (Sol's
//! request, 11/10/2026: "save password option, like for Temple"). The browser's
//! password prompt opens with the remembered one already typed, so Enter joins.
//!
//! They are kept as plain text in `server-passwords.json` beside the profile's
//! `config.cfg`, which already saves the last one typed (the archived `password`
//! cvar) the same way; nothing leaves the PC.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The remembered passwords by server address, and where they are kept.
#[derive(Debug, Default)]
pub(crate) struct ServerPasswords {
    passwords: BTreeMap<String, String>,
    path: Option<PathBuf>,
}

impl ServerPasswords {
    /// The profile's remembered passwords, beside its `config.cfg`.
    pub(crate) fn load() -> Self {
        let path = crate::platform::user_config_file().ok().and_then(|path| {
            path.parent()
                .map(|parent| parent.join("server-passwords.json"))
        });
        Self::load_from(path)
    }

    /// The passwords kept at `path` (none when it is missing or unreadable).
    pub(crate) fn load_from(path: Option<PathBuf>) -> Self {
        let passwords = path
            .as_deref()
            .and_then(|path| fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { passwords, path }
    }

    /// The password remembered for `address`.
    pub(crate) fn get(&self, address: &str) -> Option<&str> {
        self.passwords.get(address).map(String::as_str)
    }

    /// Remember `password` for `address`, or (`remember` off) forget the one kept, and
    /// save the change.
    pub(crate) fn update(&mut self, address: &str, password: &str, remember: bool) {
        let changed = if remember && !password.is_empty() {
            self.passwords
                .insert(address.to_owned(), password.to_owned())
                != Some(password.to_owned())
        } else {
            self.passwords.remove(address).is_some()
        };
        if changed
            && let Some(path) = &self.path
            && let Err(error) = save(path, &self.passwords)
        {
            crate::log::progress(format_args!("server passwords: {error}"));
        }
    }
}

fn save(path: &Path, passwords: &BTreeMap<String, String>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(passwords).map_err(std::io::Error::other)?;
    fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::ServerPasswords;

    #[test]
    fn a_remembered_password_is_kept_on_disk_and_can_be_forgotten() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("server-passwords.json");
        let mut kept = ServerPasswords::load_from(Some(path.clone()));
        assert_eq!(kept.get("185.107.96.4:29071"), None);
        kept.update("185.107.96.4:29071", "temple", true);
        let again = ServerPasswords::load_from(Some(path.clone()));
        assert_eq!(again.get("185.107.96.4:29071"), Some("temple"));
        // Not remembered: forgotten.
        kept.update("185.107.96.4:29071", "temple", false);
        let again = ServerPasswords::load_from(Some(path.clone()));
        assert_eq!(again.get("185.107.96.4:29071"), None);
        // An empty password is never kept.
        kept.update("192.0.2.1:29070", "", true);
        assert_eq!(kept.get("192.0.2.1:29070"), None);
        // A file that cannot be read is no list.
        std::fs::write(&path, b"not json").unwrap();
        assert_eq!(ServerPasswords::load_from(Some(path)).get("x"), None);
    }
}
