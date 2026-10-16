//! Finding and mounting the player's game data the way the client does, and
//! the default output location.

use sjk_vfs::{VfsError, VirtualFileSystem, pk3_search_order};
use std::env;
use std::path::{Path, PathBuf};

/// File name of the generated archive: `zzz_` sorts after the retail
/// `assets*.pk3`, so the maps load after (and never replace) retail content when
/// the pk3 is copied into `base` by hand.
pub const DEFAULT_FILE_NAME: &str = "zzz_sjk_materials.pk3";
/// The client's folder in `GameData`, where it keeps `config.cfg` when it can write
/// there.
pub const CLIENT_FOLDER: &str = "SJK";
/// The folder in the client's folder (`GameData/SJK` or the per-user SJK folder) that
/// the client mounts generated packs from, above every other pack.
pub const GENERATED_FOLDER: &str = "generated";
/// The manifest every generated pack carries, by today's tool and its JKR-era name:
/// such a pack is never read as input.
pub const MANIFESTS: [&str; 2] = [
    "sjk-materialgen/manifest.json",
    "jkr-materialgen/manifest.json",
];

/// Whether `path` looks like a Jedi Academy `GameData` directory.
pub fn is_game_data(path: &Path) -> bool {
    path.join("base/assets0.pk3").is_file() && path.join("base/assets3.pk3").is_file()
}

/// The `GameData` directory: `explicit`, then `JKA_GAME_DATA`, then
/// `fs_gameData` from the per-user config file, then the usual install locations.
pub fn find_game_data(explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return if is_game_data(path) {
            Ok(path.to_owned())
        } else {
            Err(format!(
                "{} is not a Jedi Academy GameData directory (no base/assets0.pk3)",
                path.display()
            ))
        };
    }
    let mut candidates = Vec::new();
    candidates.extend(env::var_os("JKA_GAME_DATA").map(PathBuf::from));
    if let Some(config) = user_data_root().map(|root| root.join("config.cfg"))
        && let Ok(text) = std::fs::read_to_string(config)
    {
        candidates.extend(config_game_data(&text));
    }
    if let Ok(current) = env::current_dir() {
        candidates.push(current.join("GameData"));
    }
    if cfg!(windows) {
        for root in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(root) = env::var_os(root) {
                let mut path = PathBuf::from(root);
                path.extend(["Steam", "steamapps", "common", "Jedi Academy", "GameData"]);
                candidates.push(path);
            }
        }
    } else if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        candidates.push(home.join(".local/share/Steam/steamapps/common/Jedi Academy/GameData"));
        candidates.push(home.join(".steam/steam/steamapps/common/Jedi Academy/GameData"));
        candidates.push(home.join("Games/Jedi Academy/GameData"));
    }
    candidates
        .into_iter()
        .find(|path| is_game_data(path))
        .ok_or_else(|| {
            "Jedi Academy GameData was not found: pass --game-data, or set JKA_GAME_DATA".into()
        })
}

/// `fs_gameData` from `set`/`seta` lines of a config file.
fn config_game_data(text: &str) -> Option<PathBuf> {
    text.lines().rev().find_map(|line| {
        let mut words = line.split_whitespace();
        let command = words.next()?;
        if !command.eq_ignore_ascii_case("set") && !command.eq_ignore_ascii_case("seta") {
            return None;
        }
        if !words.next()?.eq_ignore_ascii_case("fs_gameData") {
            return None;
        }
        // The value is the rest of the line: quoted paths may contain spaces.
        let start = line.to_ascii_lowercase().find("fs_gamedata")? + "fs_gameData".len();
        let value = line[start..].trim().trim_matches('"');
        (!value.is_empty()).then(|| PathBuf::from(value))
    })
}

/// SJK's per-user directory: `%APPDATA%\SJK` on Windows, `~/Library/Application
/// Support/SJK` on macOS, `$XDG_CONFIG_HOME/SJK` or `~/.config/SJK` elsewhere
/// (where the client keeps `config.cfg`).
pub fn user_data_root() -> Option<PathBuf> {
    let variable = |name: &str| {
        env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    let root = if cfg!(windows) {
        variable("APPDATA")?
    } else if cfg!(target_os = "macos") {
        variable("HOME")?.join("Library/Application Support")
    } else if let Some(path) = variable("XDG_CONFIG_HOME") {
        path
    } else {
        variable("HOME")?.join(".config")
    };
    Some(root.join("SJK"))
}

/// Where the client keeps its files: `GameData/SJK` when its `config.cfg` is there,
/// else the per-user SJK folder.
pub fn client_folder(game_data: &Path) -> Option<PathBuf> {
    let portable = game_data.join(CLIENT_FOLDER);
    if portable.join("config.cfg").is_file() {
        Some(portable)
    } else {
        user_data_root()
    }
}

/// `<client folder>/generated/zzz_sjk_materials.pk3`, which the client mounts above
/// every other pack.
pub fn default_output(game_data: &Path) -> Result<PathBuf, String> {
    client_folder(game_data)
        .map(|root| root.join(GENERATED_FOLDER).join(DEFAULT_FILE_NAME))
        .ok_or_else(|| "no per-user data directory (APPDATA/HOME unset): pass --out".into())
}

/// Whether the client mounts `out` by itself: a pk3 directly in `GameData/SJK/generated`
/// or in the per-user SJK folder's `generated`.
pub fn client_loads(out: &Path, game_data: &Path) -> bool {
    let Some(folder) = out.parent() else {
        return false;
    };
    std::iter::once(game_data.join(CLIENT_FOLDER))
        .chain(user_data_root())
        .any(|root| {
            let generated = root.join(GENERATED_FOLDER);
            is_inside(folder, &generated) && is_inside(&generated, folder)
        })
}

/// Whether `path` is `directory` or inside it, after resolving both as far as
/// they exist.
pub fn is_inside(path: &Path, directory: &Path) -> bool {
    let resolve = |path: &Path| {
        let mut existing = path.to_owned();
        let mut rest = Vec::new();
        while !existing.exists() {
            match (existing.parent(), existing.file_name()) {
                (Some(parent), Some(name)) => {
                    rest.push(name.to_owned());
                    existing = parent.to_owned();
                }
                _ => break,
            }
        }
        let mut resolved = existing.canonicalize().unwrap_or(existing);
        for name in rest.iter().rev() {
            resolved.push(name);
        }
        resolved
    };
    let lower = |path: PathBuf| {
        path.to_string_lossy()
            .to_ascii_lowercase()
            .replace('\\', "/")
    };
    let (path, directory) = (lower(resolve(path)), lower(resolve(directory)));
    let directory = directory.trim_end_matches('/');
    path == directory || path.starts_with(&format!("{directory}/"))
}

/// Mount `GameData/base`, then `GameData/<fs_game>`, then every `SJK_CONTENT`
/// directory, as the client does (loose files below each directory's pk3s,
/// pk3s in Quake 3 order; `SJK_CONTENT` pk3s below its loose files). Archives
/// named `exclude` (the tool's own earlier output) and every other generated pack
/// (one with a [manifest](MANIFESTS), whatever its name) are left out and returned,
/// so their maps never count as a texture's existing ones.
pub fn mount_game_data(
    game_data: &Path,
    fs_game: Option<&str>,
    exclude: &str,
) -> Result<(VirtualFileSystem, Vec<PathBuf>), VfsError> {
    let mut vfs = VirtualFileSystem::new();
    let mut excluded = Vec::new();
    let mut hidden = Vec::new();
    let mut mount_pk3s = |vfs: &mut VirtualFileSystem, directory: &Path| -> Result<(), VfsError> {
        for archive in pk3_search_order(directory)? {
            let name = archive
                .file_name()
                .map(|name| name.to_string_lossy().to_ascii_lowercase());
            if name.as_deref() == Some(exclude.to_ascii_lowercase().as_str()) {
                excluded.push(archive);
                continue;
            }
            match vfs.mount_pk3(&archive) {
                Ok(id) if is_generated(vfs, id) => {
                    hidden.push(id);
                    excluded.push(archive);
                }
                Ok(_) => {}
                Err(error) => eprintln!("warning: skipping {}: {error}", archive.display()),
            }
        }
        Ok(())
    };
    for game in std::iter::once("base").chain(fs_game.filter(|game| !game.is_empty())) {
        let directory = game_data.join(game);
        if !directory.is_dir() {
            continue;
        }
        vfs.mount_directory(&directory)?;
        mount_pk3s(&mut vfs, &directory)?;
    }
    for directory in env::var_os("SJK_CONTENT")
        .iter()
        .flat_map(env::split_paths)
        .filter(|directory| directory.is_dir())
    {
        mount_pk3s(&mut vfs, &directory)?;
        vfs.mount_directory(&directory)?;
    }
    Ok((vfs.without_mounts(&hidden), excluded))
}

/// Whether the mount `id` of `vfs` is a generated pack (it has a manifest).
fn is_generated(vfs: &VirtualFileSystem, id: sjk_vfs::MountId) -> bool {
    MANIFESTS
        .iter()
        .any(|manifest| vfs.mount_contains(id, manifest).unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_game_data_is_read() {
        let text = "seta r_fullscreen \"1\"\nseta fs_gameData \"C:\\Games\\JA\\GameData\"\n";
        assert_eq!(
            config_game_data(text),
            Some(PathBuf::from("C:\\Games\\JA\\GameData"))
        );
        assert_eq!(config_game_data("seta fs_game \"x\"\n"), None);
    }

    #[test]
    fn generated_packs_are_never_input_whatever_their_name() {
        use std::io::Write as _;
        let game_data = env::temp_dir().join(format!(
            "sjk-materialgen-generated-input-{}",
            std::process::id()
        ));
        let base = game_data.join("base");
        std::fs::create_dir_all(&base).unwrap();
        let pk3 = |name: &str, entries: &[&str]| {
            let file = std::fs::File::create(base.join(name)).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            for entry in entries {
                zip.start_file(*entry, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(b"x").unwrap();
            }
            zip.finish().unwrap();
        };
        pk3("assets0.pk3", &["textures/a.jpg"]);
        pk3(
            "zzz_jkr_materials.pk3",
            &["jkr-materialgen/manifest.json", "textures/a_n.png"],
        );
        pk3("zzz_sjk_materials.pk3", &["textures/b_n.png"]);
        let (vfs, excluded) = mount_game_data(&game_data, None, DEFAULT_FILE_NAME).unwrap();
        let names: Vec<_> = excluded
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["zzz_jkr_materials.pk3", "zzz_sjk_materials.pk3"]);
        assert!(vfs.contains("textures/a.jpg").unwrap());
        assert!(!vfs.contains("textures/a_n.png").unwrap());
        assert!(!vfs.contains("textures/b_n.png").unwrap());
        std::fs::remove_dir_all(&game_data).unwrap();
    }

    #[test]
    fn the_default_output_is_the_client_folder_when_it_holds_the_config() {
        let game_data = env::temp_dir().join(format!(
            "sjk-materialgen-client-folder-{}",
            std::process::id()
        ));
        let client = game_data.join(CLIENT_FOLDER);
        std::fs::create_dir_all(&client).unwrap();
        std::fs::write(client.join("config.cfg"), "").unwrap();
        let out = default_output(&game_data).unwrap();
        assert_eq!(out, client.join(GENERATED_FOLDER).join(DEFAULT_FILE_NAME));
        assert!(client_loads(&out, &game_data));
        assert!(!client_loads(&game_data.join("base/x.pk3"), &game_data));
        std::fs::remove_dir_all(&game_data).unwrap();
    }

    #[test]
    fn inside_checks_are_case_and_separator_blind() {
        let base = env::temp_dir().join("sjk-materialgen-inside");
        assert!(is_inside(&base.join("Base/zzz.pk3"), &base));
        assert!(is_inside(&base, &base));
        assert!(!is_inside(
            &env::temp_dir().join("sjk-materialgen-elsewhere"),
            &base
        ));
    }
}
