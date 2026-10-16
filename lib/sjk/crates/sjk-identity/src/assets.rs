//! Asset packs (`PROTOCOL.md`, "Assets"): the art of SJK's unlockable cosmetics, which
//! the hub serves as PK3 files. The identity worker keeps a copy of each in a cache
//! folder beside `identity.key` ([`FOLDER`]), one `<name>.pk3` per pack, and the
//! viewer mounts what [`cached_packs`] finds there.
//!
//! A pack is written only after its length and SHA-256 match the hub's list, and
//! through a temporary file renamed over the old one, so the folder never holds a
//! partial or unchecked pack under a pack's name.

use crate::hub::{Hub, HubError};
use crate::keys::Identity;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// The cache folder's name, in the settings folder beside `identity.key`.
pub const FOLDER: &str = "assets";
/// Largest pack the hub serves, in bytes.
pub const PACK_MAX: u64 = 16 * 1024 * 1024;
/// Longest pack name.
const NAME_MAX: usize = 32;

/// One pack the hub lists (`GET /v1/assets`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Pack {
    /// The pack's name: 1 to 32 of `a` to `z`, `0` to `9` and `_`.
    pub name: String,
    /// Its length in bytes, at most [`PACK_MAX`].
    pub size: u64,
    /// The SHA-256 of its bytes, 64 lowercase hex digits.
    pub sha256: String,
}

/// The hub's answer to `GET /v1/assets`.
#[derive(Deserialize)]
struct Manifest {
    packs: Vec<Pack>,
}

/// Whether `name` may name a pack: 1 to 32 of `a` to `z`, `0` to `9` and `_`. Only
/// such a name becomes a file name or a request path.
pub fn valid_name(name: &str) -> bool {
    (1..=NAME_MAX).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// Whether `text` is a SHA-256 as the hub writes it: 64 lowercase hex digits.
fn valid_sha256(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// The packs of a `GET /v1/assets` answer, `{"packs":[{"name","size","sha256"}]}`. Any
/// pack with a bad name, a size over [`PACK_MAX`] or a malformed hash, or a name listed
/// twice, refuses the whole list.
pub fn manifest(answer: Value) -> Result<Vec<Pack>, HubError> {
    let manifest: Manifest = serde_json::from_value(answer)
        .map_err(|error| HubError::Protocol(format!("the asset list: {error}")))?;
    let mut seen = BTreeSet::new();
    for pack in &manifest.packs {
        let why = if !valid_name(&pack.name) {
            "a bad name"
        } else if pack.size > PACK_MAX {
            "a size over 16 MiB"
        } else if !valid_sha256(&pack.sha256) {
            "a SHA-256 that is not 64 lowercase hex digits"
        } else if !seen.insert(pack.name.as_str()) {
            "a name listed twice"
        } else {
            continue;
        };
        return Err(HubError::Protocol(format!(
            "the asset list has a pack with {why} ({:?})",
            pack.name
        )));
    }
    Ok(manifest.packs)
}

/// Where pack `name` is kept in the cache folder `dir`.
pub fn pack_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.pk3"))
}

/// The temporary file a pack is written to before it is renamed into place.
fn partial_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!(".{name}.pk3.part"))
}

/// The packs in the cache folder `dir`: its `<name>.pk3` files with a valid name,
/// sorted by name. Temporary files and anything else are left out; a missing folder
/// has none.
pub fn cached_packs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut named: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_name = entry.file_name();
            let name = file_name.to_str()?.strip_suffix(".pk3")?;
            let path = entry.path();
            (valid_name(name) && path.is_file()).then(|| (name.to_owned(), path))
        })
        .collect();
    named.sort();
    named.into_iter().map(|(_, path)| path).collect()
}

/// `digest` as lowercase hex.
fn hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The SHA-256 of `bytes`, 64 lowercase hex digits.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// The SHA-256 of the file at `path`, 64 lowercase hex digits, read in pieces.
pub fn file_sha256(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

/// Whether the cache folder `dir` already holds `pack` as listed: a file of its size
/// whose SHA-256 is the listed one.
pub fn is_cached(dir: &Path, pack: &Pack) -> bool {
    let path = pack_path(dir, &pack.name);
    let same_size =
        std::fs::metadata(&path).is_ok_and(|meta| meta.is_file() && meta.len() == pack.size);
    same_size && file_sha256(&path).is_ok_and(|sha256| sha256 == pack.sha256)
}

/// Whether downloaded `bytes` are `pack` as listed: at most [`PACK_MAX`] long, of its
/// size and with its SHA-256. `Err` says what differs.
pub fn check(pack: &Pack, bytes: &[u8]) -> Result<(), String> {
    let length = bytes.len() as u64;
    if length > PACK_MAX {
        return Err(format!("pack {} is over 16 MiB", pack.name));
    }
    if length != pack.size {
        return Err(format!(
            "pack {} is {length} bytes, the list says {}",
            pack.name, pack.size
        ));
    }
    if sha256_hex(bytes) != pack.sha256 {
        return Err(format!(
            "pack {}'s SHA-256 differs from the list",
            pack.name
        ));
    }
    Ok(())
}

/// Write pack `name` into the cache folder `dir` (made if missing) in one step: the
/// bytes go to a temporary file in the same folder, flushed to disk, which is then
/// renamed over `<name>.pk3`. On any error the temporary file is removed and the old
/// pack, if any, is left as it was. Answers the pack's path.
pub fn write_pack(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    if !valid_name(name) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{name:?} is not a pack name"),
        ));
    }
    std::fs::create_dir_all(dir)?;
    let partial = partial_path(dir, name);
    let target = pack_path(dir, name);
    let written = (|| {
        let mut file = File::create(&partial)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&partial, &target)
    })();
    if let Err(error) = written {
        let _ = std::fs::remove_file(&partial);
        return Err(error);
    }
    // The rename itself reaches the disk with the folder's entry; a failure here
    // leaves a whole pack in place either way.
    #[cfg(unix)]
    if let Ok(folder) = File::open(dir) {
        let _ = folder.sync_all();
    }
    Ok(target)
}

/// What a check of the packs fell short with.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Problem {
    /// Worth trying again soon: the hub out of reach or busy, too many downloads, a
    /// pack that did not match its listing, the disk.
    Soon(String),
    /// A refusal that will not change soon (an older hub without assets).
    Later(String),
}

impl Problem {
    /// A hub error, prefixed with what was being done. A 4xx refusal other than a
    /// signature (401) or too many (429) is not tried again soon.
    fn of(doing: &str, error: &HubError) -> Self {
        let text = format!("{doing}: {error}");
        match error {
            HubError::Rejected { status, .. }
                if (400..500).contains(status) && !matches!(status, 401 | 429) =>
            {
                Self::Later(text)
            }
            _ => Self::Soon(text),
        }
    }

    fn is_soon(&self) -> bool {
        matches!(self, Self::Soon(_))
    }

    /// The text.
    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Soon(text) | Self::Later(text) => text,
        }
    }

    /// Keep the problem to act on: one to try again soon over one that waits, else
    /// the first.
    fn keep(slot: &mut Option<Self>, problem: Self) {
        if slot
            .as_ref()
            .is_none_or(|kept| !kept.is_soon() && problem.is_soon())
        {
            *slot = Some(problem);
        }
    }
}

/// What one check of the hub's packs did.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Refresh {
    /// How many packs the hub listed; 0 when the list was not read.
    pub(crate) listed: usize,
    /// The packs written, by name.
    pub(crate) written: Vec<String>,
    /// What fell short, if anything.
    pub(crate) problem: Option<Problem>,
}

impl Refresh {
    /// A line for the log saying what happened.
    pub(crate) fn summary(&self) -> String {
        let mut text = if self.written.is_empty() {
            format!("asset packs checked ({} listed)", self.listed)
        } else {
            format!(
                "asset packs downloaded: {} ({} listed)",
                self.written.join(", "),
                self.listed
            )
        };
        if let Some(problem) = &self.problem {
            text.push_str("; ");
            text.push_str(problem.text());
        }
        text
    }
}

/// Read the hub's list of packs and download, check and write into `dir` each one the
/// folder does not hold as listed. Packs the hub no longer lists are left in place.
/// Stops at a problem worth trying again soon (other than a pack that did not match
/// its listing, which is skipped).
pub(crate) fn refresh(hub: &mut dyn Hub, identity: &Identity, dir: &Path) -> Refresh {
    let mut refresh = Refresh::default();
    let packs = match hub.assets(identity) {
        Ok(packs) => packs,
        Err(error) => {
            refresh.problem = Some(Problem::of("cannot read the asset list", &error));
            return refresh;
        }
    };
    refresh.listed = packs.len();
    for pack in packs.iter().filter(|pack| !is_cached(dir, pack)) {
        let bytes = match hub.asset(identity, &pack.name) {
            Ok(bytes) => bytes,
            Err(error) => {
                let problem = Problem::of(&format!("cannot download pack {}", pack.name), &error);
                let stop = problem.is_soon();
                Problem::keep(&mut refresh.problem, problem);
                if stop {
                    break;
                }
                continue;
            }
        };
        if let Err(why) = check(pack, &bytes) {
            Problem::keep(
                &mut refresh.problem,
                Problem::Soon(format!("refused: {why}")),
            );
            continue;
        }
        match write_pack(dir, &pack.name, &bytes) {
            Ok(_) => refresh.written.push(pack.name.clone()),
            Err(error) => {
                Problem::keep(
                    &mut refresh.problem,
                    Problem::Soon(format!("cannot write pack {}: {error}", pack.name)),
                );
                break;
            }
        }
    }
    refresh
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    fn pack(name: &str, bytes: &[u8]) -> Pack {
        Pack {
            name: name.to_owned(),
            size: bytes.len() as u64,
            sha256: sha256_hex(bytes),
        }
    }

    #[test]
    fn hashes_are_lowercase_hex() {
        assert_eq!(sha256_hex(b"abc"), ABC);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("abc");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(file_sha256(&path).unwrap(), ABC);
    }

    #[test]
    fn pack_names_are_short_lowercase_words() {
        assert!(valid_name("sjk_skins"));
        assert!(valid_name("a"));
        assert!(valid_name(&"z".repeat(32)));
        assert!(!valid_name(""));
        assert!(!valid_name(&"z".repeat(33)));
        assert!(!valid_name("Skins"));
        assert!(!valid_name("../skins"));
        assert!(!valid_name("skins.pk3"));
        assert!(!valid_name("sk ins"));
    }

    #[test]
    fn a_good_list_parses() {
        let packs = manifest(json!({"packs": [
            {"name": "sjk_skins", "size": 241_234, "sha256": ABC},
            {"name": "sjk_sounds", "size": 0, "sha256": ABC, "future": true},
        ]}))
        .unwrap();
        assert_eq!(packs.len(), 2);
        assert_eq!(
            packs[0],
            Pack {
                name: "sjk_skins".to_owned(),
                size: 241_234,
                sha256: ABC.to_owned(),
            }
        );
        assert!(manifest(json!({"packs": []})).unwrap().is_empty());
    }

    #[test]
    fn a_bad_pack_refuses_the_whole_list() {
        let bad = |pack: Value| manifest(json!({ "packs": [pack] })).unwrap_err();
        for (pack, why) in [
            (
                json!({"name": "Skins", "size": 1, "sha256": ABC}),
                "a bad name",
            ),
            (
                json!({"name": "../x", "size": 1, "sha256": ABC}),
                "a bad name",
            ),
            (
                json!({"name": "skins", "size": PACK_MAX + 1, "sha256": ABC}),
                "over 16 MiB",
            ),
            (
                json!({"name": "skins", "size": 1, "sha256": ABC.to_uppercase()}),
                "SHA-256",
            ),
            (
                json!({"name": "skins", "size": 1, "sha256": &ABC[1..]}),
                "SHA-256",
            ),
        ] {
            let error = bad(pack);
            assert!(
                matches!(&error, HubError::Protocol(text) if text.contains(why)),
                "{error:?}"
            );
        }
        let twice = json!({"packs": [
            {"name": "skins", "size": 1, "sha256": ABC},
            {"name": "skins", "size": 1, "sha256": ABC},
        ]});
        assert!(matches!(manifest(twice), Err(HubError::Protocol(text)) if text.contains("twice")));
        for broken in [
            json!({}),
            json!({"packs": {}}),
            json!({"packs": [{"name": "skins", "size": -1, "sha256": ABC}]}),
            json!({"packs": [{"name": "skins", "sha256": ABC}]}),
        ] {
            assert!(matches!(manifest(broken), Err(HubError::Protocol(_))));
        }
    }

    #[test]
    fn downloaded_bytes_must_match_their_listing() {
        let listed = pack("skins", b"abc");
        assert_eq!(check(&listed, b"abc"), Ok(()));
        assert!(check(&listed, b"abd").unwrap_err().contains("SHA-256"));
        assert!(check(&listed, b"abcd").unwrap_err().contains("4 bytes"));
        let huge = vec![0; PACK_MAX as usize + 1];
        let over = Pack {
            size: huge.len() as u64,
            ..listed
        };
        assert!(check(&over, &huge).unwrap_err().contains("16 MiB"));
    }

    #[test]
    fn the_cache_lists_named_packs_only_in_name_order() {
        let dir = tempfile::tempdir().unwrap();
        assert!(cached_packs(&dir.path().join("missing")).is_empty());
        for file in [
            "sjk_skins.pk3",
            "a_first.pk3",
            ".sjk_skins.pk3.part",
            "Upper.pk3",
            "notes.txt",
            "skins.PK3",
        ] {
            std::fs::write(dir.path().join(file), b"x").unwrap();
        }
        std::fs::create_dir(dir.path().join("folder.pk3")).unwrap();
        assert_eq!(
            cached_packs(dir.path()),
            [
                dir.path().join("a_first.pk3"),
                dir.path().join("sjk_skins.pk3")
            ]
        );
    }

    #[test]
    fn a_pack_is_replaced_in_one_rename_and_leaves_no_partial_file() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(FOLDER);
        let path = write_pack(&dir, "skins", b"old").unwrap();
        assert_eq!(path, dir.join("skins.pk3"));
        write_pack(&dir, "skins", b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert!(is_cached(&dir, &pack("skins", b"new")));
        assert!(!is_cached(&dir, &pack("skins", b"old")));
        assert!(!is_cached(&dir, &pack("other", b"new")));
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["skins.pk3"]);
        // A rename that cannot happen (a folder in the way) leaves nothing behind.
        std::fs::create_dir(dir.join("blocked.pk3")).unwrap();
        assert!(write_pack(&dir, "blocked", b"x").is_err());
        assert!(!dir.join(".blocked.pk3.part").exists());
        assert!(write_pack(&dir, "../escape", b"x").is_err());
    }
}
