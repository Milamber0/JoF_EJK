//! Importing a player's setup from another client: a `.cfg` file (Jedi Academy's
//! `jampconfig.cfg`, EternalJK's or JA+'s) dropped on the window, named to
//! `firstsetup import`, or one of the configs [`detect`] finds from the clients
//! installed beside SJK, gives the player's name, model, field of view, FPS cap
//! and key bindings, and what the frame rate meant for their movement
//! (`gameplay_fps.rs`). The Import page (`config_import_panel.rs`) lists what was
//! found and copies the ticked parts; nothing else in the file is read or run.

use std::path::{Path, PathBuf};

/// Files larger than this are not configs.
const MAX_BYTES: u64 = 1024 * 1024;

/// What the Import page can copy, in its order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Item {
    Name,
    /// `model`, with the `char_color_*` tint when the file sets it.
    Model,
    Fov,
    /// `com_maxfps`: the frames drawn. The movement it gave is asked after
    /// (`gameplay_fps.rs`).
    FpsCap,
    Binds,
}

impl Item {
    pub(crate) const ALL: [Self; 5] = [
        Self::Name,
        Self::Model,
        Self::Fov,
        Self::FpsCap,
        Self::Binds,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Model => "Model",
            Self::Fov => "Field of view",
            Self::FpsCap => "FPS cap",
            Self::Binds => "Key bindings",
        }
    }
}

/// The importable parts of one config file.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Found {
    pub(crate) name: Option<String>,
    pub(crate) model: Option<String>,
    /// `char_color_red`, `_green` and `_blue`, copied with the model.
    pub(crate) colors: [Option<String>; 3],
    pub(crate) fov: Option<String>,
    /// `com_maxfps`, 0 (uncapped) included.
    pub(crate) max_fps: Option<i64>,
    /// `cl_cmdratecap` (JoF EJK): on, its movement ran at 125 whatever its FPS.
    pub(crate) cmd_rate_cap: Option<bool>,
    /// Key and script of each binding, in file order, one per key.
    pub(crate) binds: Vec<(String, String)>,
    /// The file clears every binding first (`unbindall`), as a client's saved
    /// config does: its bindings are a whole table that replaces the player's.
    pub(crate) whole_table: bool,
    /// Bindings left out because SJK does not know their key.
    pub(crate) unknown_keys: usize,
}

/// The `char_color_*` cvars, in [`Found::colors`] order.
pub(crate) const COLOR_CVARS: [&str; 3] = ["char_color_red", "char_color_green", "char_color_blue"];

impl Found {
    pub(crate) fn has(&self, item: Item) -> bool {
        match item {
            Item::Name => self.name.is_some(),
            Item::Model => self.model.is_some(),
            Item::Fov => self.fov.is_some(),
            Item::FpsCap => self.max_fps.is_some(),
            Item::Binds => !self.binds.is_empty(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        !Item::ALL.into_iter().any(|item| self.has(item))
    }

    fn set_cvar(&mut self, name: &str, value: &str) {
        let value = value.to_owned();
        match name.to_ascii_lowercase().as_str() {
            "name" if !value.trim().is_empty() => self.name = Some(value),
            "model" if !value.trim().is_empty() => self.model = Some(value),
            "cg_fov" if value.trim().parse::<f64>().is_ok_and(|fov| fov > 0.0) => {
                self.fov = Some(value.trim().to_owned());
            }
            "com_maxfps" => {
                if let Ok(fps) = value.trim().parse::<i64>()
                    && fps >= 0
                {
                    self.max_fps = Some(fps);
                }
            }
            "cl_cmdratecap" => {
                if let Ok(on) = value.trim().parse::<i64>() {
                    self.cmd_rate_cap = Some(on != 0);
                }
            }
            other => {
                if let Some(slot) = COLOR_CVARS.iter().position(|cvar| *cvar == other)
                    && value.trim().parse::<i64>().is_ok()
                {
                    self.colors[slot] = Some(value.trim().to_owned());
                }
            }
        }
    }

    fn bind(&mut self, key: &str, script: String) {
        let Some(key) = sjk_shell::key_names::canonical_key(key) else {
            self.unknown_keys += 1;
            return;
        };
        self.binds.retain(|(old, _)| !old.eq_ignore_ascii_case(key));
        if !script.is_empty() {
            self.binds.push((key.to_owned(), script));
        }
    }
}

/// Why a file could not be imported, as the page says it.
pub(crate) fn read(path: &Path) -> Result<Found, String> {
    let is_cfg = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("cfg"));
    if !is_cfg {
        return Err("Only .cfg files can be imported.".to_owned());
    }
    let size = std::fs::metadata(path)
        .map_err(|error| format!("Could not read it: {error}."))?
        .len();
    if size > MAX_BYTES {
        return Err("It is too large to be a config.".to_owned());
    }
    let bytes = std::fs::read(path).map_err(|error| format!("Could not read it: {error}."))?;
    let found = parse(&sjk_shell::decode_config_text(bytes));
    if found.is_empty() {
        return Err(
            "No name, model, field of view, FPS cap or key bindings were found in it.".to_owned(),
        );
    }
    Ok(found)
}

/// What a config said about its frame rate, for the gameplay FPS step.
pub(crate) fn origin(file: &str, found: &Found) -> crate::gameplay_fps::Origin {
    crate::gameplay_fps::Origin {
        file: file.to_owned(),
        max_fps: found.max_fps.filter(|fps| *fps > 0),
        cmd_rate_cap: found.cmd_rate_cap == Some(true),
    }
}

/// The importable parts of a config's text. Only `seta`/`set` (and a bare
/// `name`, `model` or `cg_fov`), `bind`, `unbind` and `unbindall` are read.
pub(crate) fn parse(text: &str) -> Found {
    let mut found = Found::default();
    for line in text.lines() {
        let Ok(commands) = sjk_shell::split_commands(without_comment(line)) else {
            continue;
        };
        for command in commands {
            let Ok(tokens) = sjk_shell::tokenize(&command) else {
                continue;
            };
            let Some((verb, rest)) = tokens.split_first() else {
                continue;
            };
            match (verb.to_ascii_lowercase().as_str(), rest) {
                ("seta" | "set" | "sets" | "setu", [name, value, ..]) => {
                    found.set_cvar(name, value);
                }
                ("name" | "model" | "cg_fov", [value]) => found.set_cvar(verb, value),
                ("bind", [key, script @ ..]) if !script.is_empty() => {
                    found.bind(key, script.join(" "));
                }
                ("unbind", [key]) => found.bind(key, String::new()),
                ("unbindall", []) => {
                    found.binds.clear();
                    found.whole_table = true;
                }
                _ => {}
            }
        }
    }
    found
}

/// The configs other clients write, by file name, and the client each belongs to:
/// retail Jedi Academy and JA+ (`jampconfig.cfg` in the game folder), OpenJK
/// (`openjk.cfg`), EternalJK and JoF EJK (`eternaljk.cfg`) and TaystJK
/// (`taystjk.cfg`), as their `Q3CONFIG_CFG` names them.
const CLIENT_CONFIGS: [(&str, &str); 4] = [
    ("jampconfig.cfg", "Jedi Academy"),
    ("openjk.cfg", "OpenJK"),
    ("eternaljk.cfg", "EternalJK / JoF EJK"),
    ("taystjk.cfg", "TaystJK"),
];

/// The folders under `Documents/My Games` the OpenJK-based clients write to when
/// they are not portable (`HOMEPATH_NAME_WIN`).
const HOME_FOLDERS: [&str; 3] = ["OpenJK", "EternalJK", "TaystJK"];

/// A config folder that is the client's own rather than a mod's.
const OWN_FOLDERS: [&str; 4] = ["base", "eternaljk", "taystjk", "openjk"];

/// Most configs the Import page offers.
pub(crate) const MAX_DETECTED: usize = 4;

/// A config another client wrote, found on this computer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Detected {
    /// The client, and its mod folder when it is not the client's own.
    pub(crate) client: String,
    pub(crate) path: PathBuf,
    /// The player name it holds, without colour codes.
    pub(crate) name: Option<String>,
    /// When it was last written, `dd/mm/yyyy`.
    pub(crate) date: String,
    modified: std::time::SystemTime,
}

/// The configs other clients wrote, newest first, at most [`MAX_DETECTED`]: in each
/// folder of `game_data` (portable installs and retail) and of the OpenJK clients'
/// `My Games` folders under `my_games`. Configs holding nothing to import are
/// left out, and so is SJK's own folder.
pub(crate) fn detect(game_data: Option<&Path>, my_games: Option<&Path>) -> Vec<Detected> {
    let mut folders: Vec<PathBuf> = Vec::new();
    let mut subfolders = |root: &Path| {
        if let Ok(entries) = std::fs::read_dir(root) {
            folders.extend(
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.is_dir()),
            );
        }
    };
    if let Some(game_data) = game_data {
        subfolders(game_data);
    }
    if let Some(my_games) = my_games {
        for home in HOME_FOLDERS {
            subfolders(&my_games.join(home));
        }
    }
    let mut found: Vec<Detected> = folders
        .iter()
        .filter(|folder| {
            !folder
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("SJK"))
        })
        .flat_map(|folder| {
            CLIENT_CONFIGS
                .iter()
                .map(move |(file, client)| (folder.join(file), *client))
        })
        .filter_map(|(path, client)| detected(path, client))
        .collect();
    found.sort_by_key(|config| std::cmp::Reverse(config.modified));
    found.truncate(MAX_DETECTED);
    found
}

/// `path` as a [`Detected`] config of `client`, when it is one worth offering.
fn detected(path: PathBuf, client: &str) -> Option<Detected> {
    let metadata = std::fs::metadata(&path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let found = read(&path).ok()?;
    let modified = metadata.modified().ok()?;
    let folder = path
        .parent()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let own = OWN_FOLDERS
        .iter()
        .any(|own| folder.eq_ignore_ascii_case(own));
    let client = if folder.is_empty() || own {
        client.to_owned()
    } else {
        format!("{client} ({folder})")
    };
    let seconds = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64);
    Some(Detected {
        client,
        name: found
            .name
            .as_deref()
            .map(crate::console::config_import_panel::plain),
        date: crate::medals::date_text(seconds),
        path,
        modified,
    })
}

/// `Documents/My Games` of the player, where OpenJK-based clients write when they
/// are not portable.
pub(crate) fn my_games() -> Option<PathBuf> {
    let profile = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let folder = PathBuf::from(profile).join("Documents").join("My Games");
    folder.is_dir().then_some(folder)
}

/// `line` up to a `//` comment outside quotes.
fn without_comment(line: &str) -> &str {
    let mut quoted = false;
    let bytes = line.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            b'/' if !quoted && bytes.get(index + 1) == Some(&b'/') => return &line[..index],
            _ => {}
        }
    }
    line
}

/// Whether a file dropped on the window is the player's new picture
/// (`docs/identity.md`, "Pictures") rather than a setup to import: a picture file by its
/// extension at any time, and while the Profile page shows any file but a `.cfg`, so
/// the page says why a file is no picture.
fn dropped_picture(path: &Path, profile_shown: bool) -> bool {
    let config = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("cfg"));
    crate::avatars::picture::looks_like_picture(path) || (profile_shown && !config)
}

impl crate::GpuState {
    /// A file was dropped on the window: a picture goes to the Profile page, anything
    /// else to the Import page, with what it holds.
    pub(crate) fn file_dropped(&mut self, path: &Path) {
        let picture = self
            .console
            .as_ref()
            .is_some_and(|console| dropped_picture(path, console.profile_panel_shown()));
        // The SJK UI shows the picture on the Profile screen's SJK Profile tab.
        if picture && self.in_game_menu.is_sjk() {
            self.open_profile_hub(crate::profile_hub::Tab::Profile);
        }
        if let Some(console) = &mut self.console {
            if picture {
                console.profile_load_picture(path);
            } else {
                console.open_config_import(Some(path));
            }
        }
        self.sync_cursor_policy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dropped_picture_goes_to_the_profile_and_a_config_to_the_import() {
        let path = Path::new;
        assert!(dropped_picture(path("me.PNG"), false));
        assert!(dropped_picture(path("dir/me.jpeg"), false));
        assert!(!dropped_picture(path("jampconfig.cfg"), false));
        assert!(!dropped_picture(path("notes.txt"), false));
        // While the Profile page shows, any other file is tried as a picture (and
        // refused there with the reason); a config still goes to the import.
        assert!(dropped_picture(path("me.webp"), true));
        assert!(!dropped_picture(path("autoexec.CFG"), true));
    }

    #[test]
    fn a_saved_config_gives_name_model_fov_and_the_whole_bind_table() {
        let found = parse(
            "// generated by Jedi Academy, do not modify\n\
             unbindall\n\
             bind TAB \"+scores\"\n\
             bind w \"+forward\"\n\
             bind MOUSE1 \"+attack\"\n\
             seta name \"^1Sol^7Fox\"\n\
             seta model \"kyle/default\"\n\
             seta char_color_red \"200\"\n\
             seta cg_fov \"100\"\n\
             seta sensitivity \"3\"\n",
        );
        assert_eq!(found.name.as_deref(), Some("^1Sol^7Fox"));
        assert_eq!(found.model.as_deref(), Some("kyle/default"));
        assert_eq!(found.colors[0].as_deref(), Some("200"));
        assert_eq!(found.colors[1], None);
        assert_eq!(found.fov.as_deref(), Some("100"));
        assert!(found.whole_table);
        assert_eq!(found.binds.len(), 3);
        assert_eq!(found.binds[1].1, "+forward");
    }

    #[test]
    fn binds_keep_the_last_script_per_key_and_join_unquoted_words() {
        let found = parse(
            "bind x say hello there // greet\n\
             bind X \"say bye; wave\"\n\
             bind f \"+use\"\n\
             unbind f\n\
             bind NOSUCHKEY \"+attack\"\n",
        );
        assert!(!found.whole_table);
        assert_eq!(found.binds.len(), 1);
        assert_eq!(found.binds[0].1, "say bye; wave");
        assert_eq!(found.unknown_keys, 1);
    }

    #[test]
    fn comments_bare_cvars_and_bad_values() {
        let found = parse(
            "name \"a // b\" // not this\n\
             cg_fov wide\n\
             seta model \"\"\n\
             set cg_fov 110; seta snaps 40\n",
        );
        assert_eq!(found.name.as_deref(), Some("a // b"));
        assert_eq!(found.model, None);
        assert_eq!(found.fov.as_deref(), Some("110"));
    }

    #[test]
    fn a_latin1_bind_only_file_with_stock_backslash_and_tilde_keys() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("turin.cfg");
        let mut bytes = b"bind \"F3\" \"set name ^5".to_vec();
        bytes.push(0xB7);
        bytes.extend_from_slice(
            b"^7turin\"\nbind \"\\\" \"force_forcepowerother\"\nbind \"~\" \"toggleconsole\"\n\
              bind \"G\" \"force_absorb; wait 2; force_protect\"\n",
        );
        std::fs::write(&config, bytes).unwrap();
        let found = read(&config).unwrap();
        assert_eq!(found.unknown_keys, 0);
        assert_eq!(found.binds.len(), 4);
        assert_eq!(found.binds[0].1, "set name ^5\u{b7}^7turin");
        assert_eq!(found.binds[1].1, "force_forcepowerother");
        assert_eq!(found.binds[3].1, "force_absorb; wait 2; force_protect");
    }

    #[test]
    fn a_config_with_none_of_the_parts_is_empty() {
        assert!(parse("seta snaps 40\nexec other.cfg\n").is_empty());
    }

    #[test]
    fn only_cfg_files_are_read() {
        let directory = tempfile::tempdir().unwrap();
        let text = directory.path().join("notes.txt");
        std::fs::write(&text, "seta name x").unwrap();
        assert!(read(&text).unwrap_err().contains(".cfg"));
        let config = directory.path().join("jampconfig.CFG");
        std::fs::write(&config, "seta name x").unwrap();
        assert_eq!(read(&config).unwrap().name.as_deref(), Some("x"));
        let empty = directory.path().join("autoexec.cfg");
        std::fs::write(&empty, "seta snaps 40").unwrap();
        assert!(read(&empty).is_err());
    }

    #[test]
    fn the_frame_rate_and_jof_command_rate_are_read() {
        let found = parse("seta com_maxfps \"333\"\nseta cl_cmdratecap \"0\"\n");
        assert_eq!(found.max_fps, Some(333));
        assert_eq!(found.cmd_rate_cap, Some(false));
        assert!(found.has(Item::FpsCap));
        assert_eq!(origin("jampconfig.cfg", &found).max_fps, Some(333));
        assert_eq!(
            origin("jampconfig.cfg", &found).old_fps(),
            0,
            "Auto follows the cap"
        );
        // An uncapped client (0) imports its cap, but says nothing of its movement.
        let found = parse("seta com_maxfps 0\nseta cl_cmdratecap 1\n");
        assert_eq!(found.max_fps, Some(0));
        let jof = origin("eternaljk.cfg", &found);
        assert_eq!(jof.max_fps, None);
        assert_eq!(jof.old_fps(), 125, "cl_cmdratecap moved at 125");
        assert!(parse("seta com_maxfps fast").max_fps.is_none());
    }

    #[test]
    fn installed_clients_configs_are_found_newest_first() {
        let root = tempfile::tempdir().unwrap();
        let game_data = root.path().join("GameData");
        let my_games = root.path().join("My Games");
        let write = |path: std::path::PathBuf, text: &str| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(game_data.join("base/jampconfig.cfg"), "seta name ^1Old\n");
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(
            game_data.join("japlus/jampconfig.cfg"),
            "seta com_maxfps 333\n",
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(my_games.join("TaystJK/base/taystjk.cfg"), "seta name Sol\n");
        // Nothing to import, another file name, and SJK's own folder are left out.
        write(game_data.join("EternalJK/eternaljk.cfg"), "seta snaps 40\n");
        write(game_data.join("base/autoexec.cfg"), "seta name Nope\n");
        write(game_data.join("SJK/jampconfig.cfg"), "seta name Me\n");
        let found = detect(Some(&game_data), Some(&my_games));
        let clients: Vec<&str> = found.iter().map(|config| config.client.as_str()).collect();
        assert_eq!(
            clients,
            ["TaystJK", "Jedi Academy (japlus)", "Jedi Academy"]
        );
        assert_eq!(found[0].name.as_deref(), Some("Sol"));
        assert_eq!(found[2].name.as_deref(), Some("Old"));
        assert_eq!(found[0].date.len(), "dd/mm/yyyy".len());
        assert!(detect(None, None).is_empty());
    }
}
