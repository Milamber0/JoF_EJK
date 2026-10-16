//! The pictures kept on this PC: `avatars/<key id>-<version>.png` in the settings
//! folder, each the hub's PNG as it was served, so a picture seen once is not downloaded
//! again until its player changes it. The folder is bounded ([`FILES_MAX`],
//! [`BYTES_MAX`]): past either, the pictures used longest ago go first. Only names of
//! that exact form are read, written or removed, so nothing else in the folder is
//! touched and no remote text becomes a path.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The folder's name, in the settings folder beside `identity.key`.
pub(crate) const DIR: &str = "avatars";
/// Most pictures kept.
pub(crate) const FILES_MAX: usize = 256;
/// Most bytes kept.
pub(crate) const BYTES_MAX: u64 = 8 * 1024 * 1024;

/// The file of `version` of `key_id`'s picture, when both are well formed.
pub(crate) fn file_name(key_id: &str, version: &str) -> Option<String> {
    (sjk_identity::avatar::valid_key_id(key_id) && sjk_identity::avatar::valid_version(version))
        .then(|| format!("{}-{version}.png", key_id.to_ascii_lowercase()))
}

/// Whether `name` is a file this store made.
fn ours(name: &str) -> bool {
    name.strip_suffix(".png")
        .and_then(|stem| stem.split_once('-'))
        .is_some_and(|(key, version)| file_name(key, version).as_deref() == Some(name))
}

/// The picture kept for `key_id` at `version`, if any; reading it counts as a use.
pub(crate) fn read(dir: &Path, key_id: &str, version: &str) -> Option<Vec<u8>> {
    let path = dir.join(file_name(key_id, version)?);
    let size = std::fs::metadata(&path).ok()?.len();
    if size > sjk_identity::avatar::ANSWER_MAX {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    touch(&path);
    Some(bytes)
}

/// Mark `path` used now, for the oldest-first trim.
fn touch(path: &Path) {
    if let Ok(file) = std::fs::File::options().write(true).open(path) {
        let _ = file.set_modified(SystemTime::now());
    }
}

/// Keep `png` as `version` of `key_id`'s picture (through a temporary file, so a reader
/// never sees half of one), drop the key's older versions, then trim the folder.
pub(crate) fn write(dir: &Path, key_id: &str, version: &str, png: &[u8]) -> std::io::Result<()> {
    let name = file_name(key_id, version)
        .ok_or_else(|| std::io::Error::other("not a picture's key id and version"))?;
    std::fs::create_dir_all(dir)?;
    let temporary = dir.join(format!("{name}.part"));
    std::fs::write(&temporary, png)?;
    std::fs::rename(&temporary, dir.join(&name))?;
    let prefix = format!("{}-", key_id.to_ascii_lowercase());
    for (path, _, _) in kept(dir) {
        let file = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if file.starts_with(&prefix) && file != name {
            let _ = std::fs::remove_file(&path);
        }
    }
    trim(dir, FILES_MAX, BYTES_MAX);
    Ok(())
}

/// The pictures in `dir`: path, size and when last used.
fn kept(dir: &Path) -> Vec<(PathBuf, u64, SystemTime)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_str().is_some_and(ours))
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            meta.is_file().then(|| {
                (
                    entry.path(),
                    meta.len(),
                    meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                )
            })
        })
        .collect()
}

/// Remove the pictures used longest ago until at most `files` are kept in at most
/// `bytes`; how many went.
pub(crate) fn trim(dir: &Path, files: usize, bytes: u64) -> usize {
    let mut pictures = kept(dir);
    pictures.sort_by_key(|(path, _, used)| (*used, path.clone()));
    let mut total: u64 = pictures.iter().map(|(_, size, _)| size).sum();
    let mut count = pictures.len();
    let mut removed = 0;
    for (path, size, _) in pictures {
        if count <= files && total <= bytes {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
        count -= 1;
        total = total.saturating_sub(size);
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "0123456789abcdef";

    fn version(n: u8) -> String {
        format!("{n:016x}")
    }

    fn set_used(dir: &Path, key: &str, version: &str, seconds: u64) {
        let file = std::fs::File::options()
            .write(true)
            .open(dir.join(file_name(key, version).unwrap()))
            .unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(seconds))
            .unwrap();
    }

    #[test]
    fn only_well_formed_names_are_used() {
        assert_eq!(
            file_name("0123456789ABCDEF", "fedcba9876543210").as_deref(),
            Some("0123456789abcdef-fedcba9876543210.png")
        );
        assert_eq!(file_name("../../etc", "fedcba9876543210"), None);
        assert_eq!(file_name(KEY, "../x"), None);
        assert!(ours("0123456789abcdef-fedcba9876543210.png"));
        assert!(!ours("identity.key"));
        assert!(!ours("0123456789abcdef-fedcba9876543210.png.part"));
        assert!(!ours("x-y.png"));
    }

    #[test]
    fn a_picture_is_kept_by_version_and_its_older_versions_go() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DIR);
        assert_eq!(read(&path, KEY, &version(1)), None);
        write(&path, KEY, &version(1), b"first").unwrap();
        assert_eq!(
            read(&path, KEY, &version(1)).as_deref(),
            Some(&b"first"[..])
        );
        write(&path, KEY, &version(2), b"second").unwrap();
        assert_eq!(read(&path, KEY, &version(1)), None, "the old version went");
        assert_eq!(
            read(&path, KEY, &version(2)).as_deref(),
            Some(&b"second"[..])
        );
        // Another key's picture stays, and so does anything that is not a picture.
        write(&path, "fedcba9876543210", &version(3), b"other").unwrap();
        std::fs::write(path.join("notes.txt"), b"mine").unwrap();
        write(&path, KEY, &version(4), b"third").unwrap();
        assert!(read(&path, "fedcba9876543210", &version(3)).is_some());
        assert!(path.join("notes.txt").exists());
        assert!(write(&path, "nope", &version(5), b"x").is_err());
    }

    #[test]
    fn the_folder_keeps_the_pictures_used_last_within_its_bounds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        let keys: Vec<String> = (0..6).map(|n| format!("{n:016x}")).collect();
        for (index, key) in keys.iter().enumerate() {
            write(path, key, &version(1), &[0; 100]).unwrap();
            set_used(path, key, &version(1), 1_000 + index as u64);
        }
        std::fs::write(path.join("keep.me"), [0; 10_000]).unwrap();
        // Reading the oldest makes it the newest.
        assert!(read(path, &keys[0], &version(1)).is_some());
        assert_eq!(trim(path, 4, BYTES_MAX), 2);
        let left = |key: &str| path.join(file_name(key, &version(1)).unwrap()).exists();
        assert!(left(&keys[0]), "used just now");
        assert!(!left(&keys[1]) && !left(&keys[2]));
        assert!(left(&keys[3]) && left(&keys[4]) && left(&keys[5]));
        // A byte bound too: 250 bytes keep two of the 100-byte pictures.
        assert_eq!(trim(path, FILES_MAX, 250), 2);
        assert_eq!(kept(path).len(), 2);
        assert!(path.join("keep.me").exists(), "not ours, not counted");
    }
}
