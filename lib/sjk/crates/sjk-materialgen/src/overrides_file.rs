//! Where the overrides file is read from, so a pack is never built without the player's
//! rules by accident.
//!
//! The file's home is the client's `generated` folder, next to the pack the client loads
//! ([`canonical`]: `GameData/SJK/generated` or the per-user SJK folder's). Until 13/10/2026
//! the tool read it only next to the output pk3, so when the default output moved from the
//! JKR-era `%APPDATA%\jkr\generated` to `GameData/SJK/generated`, Sol's rules stayed behind
//! and the 12/10 pack was built without them, silently. Now the tool looks, in order:
//!
//! 1. `--overrides FILE` (or none with `--no-overrides`);
//! 2. next to the output pk3 (a pack written elsewhere can carry its own rules);
//! 3. the canonical file in the client's `generated` folder;
//! 4. the folders earlier versions used ([`LEGACY_FOLDERS`]). A file found only there is
//!    copied to the canonical place (the old one is kept), so the next run finds it.
//!
//! The run prints which file it read, or every place it looked when it found none, and
//! the manifest records the same ([`Found::describe`]).

use crate::mount::{GENERATED_FOLDER, client_folder, user_data_root};
use std::path::{Path, PathBuf};

/// The overrides file's name, in the client's `generated` folder or next to an output pk3.
pub const FILE_NAME: &str = "sjk-materialgen-overrides.txt";

/// Folders earlier versions of the tool read the overrides from, relative to the
/// per-user data folder's parent (`%APPDATA%` on Windows), with why each is listed.
pub const LEGACY_FOLDERS: &[(&str, &str)] = &[(
    "jkr/generated",
    "the JKR-era per-user folder, the default output before 12/10/2026",
)];

/// How the overrides file was chosen.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// `--overrides FILE`.
    Argument,
    /// Next to the output pk3.
    BesideOutput,
    /// The client's `generated` folder.
    Canonical,
    /// A legacy folder; `copied_to` names the canonical copy made from it.
    Legacy { copied_to: Option<PathBuf> },
}

/// The outcome of the lookup.
#[derive(Clone, Debug, PartialEq)]
pub enum Found {
    /// The file read and how it was chosen.
    File(PathBuf, Source),
    /// `--no-overrides`.
    Disabled,
    /// No file anywhere; the places looked at, in order.
    Missing(Vec<PathBuf>),
}

impl Found {
    /// The file to read, if any.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::File(path, _) => Some(path),
            _ => None,
        }
    }

    /// One line for the run's output and the manifest.
    pub fn describe(&self) -> String {
        match self {
            Self::File(path, source) => {
                let how = match source {
                    Source::Argument => "--overrides".to_owned(),
                    Source::BesideOutput => "next to the output".to_owned(),
                    Source::Canonical => "the client's generated folder".to_owned(),
                    Source::Legacy { copied_to: None } => {
                        "an earlier version's folder, not copied".to_owned()
                    }
                    Source::Legacy {
                        copied_to: Some(copy),
                    } => format!("an earlier version's folder, copied to {}", copy.display()),
                };
                format!("{} ({how})", path.display())
            }
            Self::Disabled => "none (--no-overrides)".to_owned(),
            Self::Missing(looked) => format!(
                "NONE FOUND, the heuristics alone decide; looked in: {}",
                looked
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

/// The canonical overrides file: in the client's `generated` folder.
pub fn canonical(game_data: &Path) -> Option<PathBuf> {
    client_folder(game_data).map(|root| root.join(GENERATED_FOLDER).join(FILE_NAME))
}

/// The legacy overrides files, from [`LEGACY_FOLDERS`].
pub fn legacy() -> Vec<PathBuf> {
    let Some(parent) = user_data_root().and_then(|root| root.parent().map(Path::to_owned)) else {
        return Vec::new();
    };
    LEGACY_FOLDERS
        .iter()
        .map(|(folder, _)| parent.join(folder).join(FILE_NAME))
        .collect()
}

/// Find the overrides file for a run writing `out` (see the module documentation).
/// `migrate` copies a file found only in a legacy folder to `canonical`; a dry run
/// passes false and writes nothing.
pub fn find(
    explicit: Option<&Path>,
    disabled: bool,
    out: &Path,
    canonical: Option<&Path>,
    legacy: &[PathBuf],
    migrate: bool,
) -> Result<Found, String> {
    if disabled {
        return Ok(Found::Disabled);
    }
    if let Some(path) = explicit {
        return if path.is_file() {
            Ok(Found::File(path.to_owned(), Source::Argument))
        } else {
            Err(format!("--overrides {}: no such file", path.display()))
        };
    }
    let mut looked = Vec::new();
    let beside = out.with_file_name(FILE_NAME);
    if beside.is_file() {
        let source = if canonical.is_some_and(|canonical| same_file(canonical, &beside)) {
            Source::Canonical
        } else {
            Source::BesideOutput
        };
        return Ok(Found::File(beside, source));
    }
    looked.push(beside);
    if let Some(canonical) = canonical {
        if canonical.is_file() {
            return Ok(Found::File(canonical.to_owned(), Source::Canonical));
        }
        if !looked.iter().any(|path| same_file(path, canonical)) {
            looked.push(canonical.to_owned());
        }
    }
    for old in legacy {
        if old.is_file() {
            let copied_to = match canonical.filter(|_| migrate) {
                Some(canonical) => {
                    if let Some(folder) = canonical.parent() {
                        std::fs::create_dir_all(folder).map_err(|error| {
                            format!("cannot create {}: {error}", folder.display())
                        })?;
                    }
                    std::fs::copy(old, canonical).map_err(|error| {
                        format!(
                            "cannot copy {} to {}: {error}",
                            old.display(),
                            canonical.display()
                        )
                    })?;
                    Some(canonical.to_owned())
                }
                None => None,
            };
            return Ok(Found::File(old.clone(), Source::Legacy { copied_to }));
        }
        looked.push(old.clone());
    }
    Ok(Found::Missing(looked))
}

/// Whether two paths name the same file, ignoring case and separators (Windows paths).
fn same_file(a: &Path, b: &Path) -> bool {
    let normal = |path: &Path| {
        path.to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase()
    };
    normal(a) == normal(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Folders {
        root: PathBuf,
        out: PathBuf,
        canonical: PathBuf,
        legacy: PathBuf,
    }

    fn folders(name: &str) -> Folders {
        let root = std::env::temp_dir().join(format!(
            "sjk-materialgen-overrides-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        for folder in ["tmp", "SJK/generated", "jkr/generated"] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
        }
        Folders {
            out: root.join("tmp/zzz_sjk_materials.pk3"),
            canonical: root.join("SJK/generated").join(FILE_NAME),
            legacy: root.join("jkr/generated").join(FILE_NAME),
            root,
        }
    }

    #[test]
    fn a_file_left_in_the_old_folder_is_found_and_copied() {
        // 12/10/2026: the rules stayed in %APPDATA%\jkr\generated when the pack moved.
        let f = folders("legacy");
        std::fs::write(&f.legacy, "textures/a/b class=stone\n").unwrap();
        let legacy = [f.legacy.clone()];
        let found = find(None, false, &f.out, Some(&f.canonical), &legacy, true).unwrap();
        assert_eq!(
            found,
            Found::File(
                f.legacy.clone(),
                Source::Legacy {
                    copied_to: Some(f.canonical.clone())
                }
            )
        );
        assert!(found.describe().contains("copied to"));
        assert_eq!(
            std::fs::read_to_string(&f.canonical).unwrap(),
            "textures/a/b class=stone\n"
        );
        assert!(f.legacy.is_file(), "the old file is kept");
        // The next run reads the canonical copy, also for an output elsewhere.
        let found = find(None, false, &f.out, Some(&f.canonical), &legacy, true).unwrap();
        assert_eq!(found, Found::File(f.canonical.clone(), Source::Canonical));
        std::fs::remove_dir_all(&f.root).unwrap();
    }

    #[test]
    fn a_dry_run_reads_the_old_file_without_copying() {
        let f = folders("dry");
        std::fs::write(&f.legacy, "x class=stone\n").unwrap();
        let legacy = [f.legacy.clone()];
        let found = find(None, false, &f.out, Some(&f.canonical), &legacy, false).unwrap();
        assert_eq!(
            found,
            Found::File(f.legacy.clone(), Source::Legacy { copied_to: None })
        );
        assert!(!f.canonical.exists());
        std::fs::remove_dir_all(&f.root).unwrap();
    }

    #[test]
    fn order_argument_beside_canonical_and_none() {
        let f = folders("order");
        let legacy = [f.legacy.clone()];
        // Nothing anywhere: every place looked at is named, loudly.
        let found = find(None, false, &f.out, Some(&f.canonical), &legacy, true).unwrap();
        let Found::Missing(looked) = &found else {
            panic!("{found:?}");
        };
        assert_eq!(looked.len(), 3);
        assert!(found.describe().starts_with("NONE FOUND"));
        std::fs::write(&f.canonical, "x class=stone\n").unwrap();
        std::fs::write(&f.legacy, "y class=stone\n").unwrap();
        assert_eq!(
            find(None, false, &f.out, Some(&f.canonical), &legacy, true).unwrap(),
            Found::File(f.canonical.clone(), Source::Canonical)
        );
        let beside = f.out.with_file_name(FILE_NAME);
        std::fs::write(&beside, "z class=stone\n").unwrap();
        assert_eq!(
            find(None, false, &f.out, Some(&f.canonical), &legacy, true).unwrap(),
            Found::File(beside.clone(), Source::BesideOutput)
        );
        assert_eq!(
            find(
                Some(&f.legacy),
                false,
                &f.out,
                Some(&f.canonical),
                &legacy,
                true
            )
            .unwrap(),
            Found::File(f.legacy.clone(), Source::Argument)
        );
        assert!(
            find(
                Some(&f.root.join("nope.txt")),
                false,
                &f.out,
                None,
                &[],
                true
            )
            .is_err()
        );
        assert_eq!(
            find(None, true, &f.out, Some(&f.canonical), &legacy, true).unwrap(),
            Found::Disabled
        );
        // The default output's own folder is the canonical one.
        let default_out = f.canonical.with_file_name("zzz_sjk_materials.pk3");
        assert_eq!(
            find(None, false, &default_out, Some(&f.canonical), &legacy, true).unwrap(),
            Found::File(f.canonical.clone(), Source::Canonical)
        );
        std::fs::remove_dir_all(&f.root).unwrap();
    }
}
