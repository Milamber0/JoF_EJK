//! MP startup search-path policy. Generic mount mechanics remain in sjk-vfs.
use crate::console::ViewerConsole;
use sjk_shell::{CvarDefinition, CvarFlags, CvarRegistry};
use sjk_vfs::VirtualFileSystem;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static STARTUP: OnceLock<Options> = OnceLock::new();

#[path = "asset_pack_policy.rs"]
pub(crate) mod pack_policy;
/// The cosmetics packs were named in the log (every world mounts them).
static COSMETICS_LOGGED: AtomicBool = AtomicBool::new(false);

/// The folder JoF EJK and EternalJK keep their own content in, where JoF's
/// launcher installs hat and cape packs.
const COSMETICS_GAME: &str = "EternalJK";
/// Where a pack's hats and capes are; a PK3 with models there is a
/// cosmetics pack.
const COSMETIC_FOLDERS: [&str; 2] = ["models/cosmetics/hats", "models/cosmetics/capes"];
/// JoF EJK's client pictures (`jofclient-assets.pk3`): the Force wheel's Repulse,
/// Dash and flamethrower icons. The pack also holds the sounds and effects JoF
/// servers play, such as `sound/jof/repulse.mp3`.
const JOF_CLIENT_FOLDER: &str = "gfx/jof";

/// Immutable startup settings shared by world-loading workers.
pub(crate) struct Options {
    game: String,
    basegame: String,
    home: Option<PathBuf>,
    portable: bool,
    directory_first: bool,
    debug: bool,
    /// Prefer base-game shader definitions; immutable after startup.
    pub(crate) protect_shaders: bool,
    disabled: pack_policy::Disabled,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            game: String::new(),
            basegame: String::new(),
            home: None,
            portable: true,
            directory_first: false,
            debug: false,
            protect_shaders: true,
            disabled: Default::default(),
        }
    }
}

/// Register startup preferences; edits apply on next process launch, not a map reload.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    for definition in [
        CvarDefinition::new(
            pack_policy::CVAR,
            "[]",
            CvarFlags::ARCHIVE,
            "Disabled installed PK3 packs (JSON list); edit in assetbrowser; client restart required",
        ),
        CvarDefinition::new(
            "fs_game",
            "",
            CvarFlags::ARCHIVE,
            "Mod directory; restart the client after changing",
        ),
        CvarDefinition::new(
            "fs_basegame",
            "",
            CvarFlags::ARCHIVE,
            "Intermediate game directory; client restart required",
        ),
        CvarDefinition::new(
            "fs_homepath",
            "",
            CvarFlags::ARCHIVE,
            "Optional explicit content home; empty disables it; restart required",
        ),
    ] {
        cvars.register(definition)?;
    }
    for (name, value, help) in [
        (
            "fs_portable",
            true,
            "Ignore optional content home; client restart required",
        ),
        (
            "fs_dirbeforepak",
            false,
            "Prefer loose files within each directory; restart required",
        ),
        (
            "fs_debug",
            false,
            "Log mounted sources and asset reads; client restart required",
        ),
        (
            "fs_protectShaders",
            true,
            "Prefer base-game shader definitions; client restart required",
        ),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    Ok(())
}

/// Freeze settings once before the first native world load; evidence defaults stay isolated.
pub(crate) fn initialize(console: &ViewerConsole) -> Result<(), Box<dyn Error>> {
    STARTUP
        .set(Options::from_console(console)?)
        .map_err(|_| "asset startup settings already initialized")?;
    Ok(())
}

/// Snapshot shared by all subsequent mounts without locks or config reads.
pub(crate) fn startup() -> &'static Options {
    STARTUP.get_or_init(Options::default)
}

impl Options {
    pub(crate) fn from_console(console: &ViewerConsole) -> Result<Self, Box<dyn Error>> {
        let game = console.text_value("fs_game").unwrap_or("").to_owned();
        let basegame = console.text_value("fs_basegame").unwrap_or("").to_owned();
        validate_directory(&game)?;
        validate_directory(&basegame)?;
        let home = console
            .text_value("fs_homepath")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        Ok(Self {
            disabled: pack_policy::Disabled::parse(
                console.text_value(pack_policy::CVAR).unwrap_or("[]"),
            )?,
            game,
            basegame,
            home,
            portable: console.bool_cvar("fs_portable").unwrap_or(true),
            directory_first: console.bool_cvar("fs_dirbeforepak").unwrap_or(false),
            debug: console.bool_cvar("fs_debug").unwrap_or(false),
            protect_shaders: console.bool_cvar("fs_protectShaders").unwrap_or(true),
        })
    }

    /// Low-to-high priority directories, matching FS_Startup's nested ordering.
    pub(crate) fn directories(&self, install: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
        let home = if self.portable {
            None
        } else {
            // Downloads are session-selected, never an implicit global content home.
            self.home.as_deref()
        };
        let mut paths = Vec::new();
        for game in ["base", self.basegame.as_str(), self.game.as_str()] {
            if game.is_empty() {
                continue;
            }
            for root in std::iter::once(install).chain(home) {
                let path = root.join(game);
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
        Ok(paths)
    }

    /// The PK3s in `install/EternalJK` that carry hats or capes
    /// (`models/cosmetics/`) or JoF's client pictures (`gfx/jof/`), lowest
    /// priority first: JoF EJK's cosmetics and client assets, found without
    /// mounting the rest of that folder (its menus, HUD and strings). None
    /// when the folder is a game directory already.
    pub(crate) fn cosmetic_packs(&self, install: &Path) -> Vec<PathBuf> {
        let mounted = [self.basegame.as_str(), self.game.as_str()]
            .iter()
            .any(|game| game.eq_ignore_ascii_case(COSMETICS_GAME));
        let directory = install.join(COSMETICS_GAME);
        if mounted || !directory.is_dir() {
            return Vec::new();
        }
        sjk_vfs::pk3_search_order(&directory)
            .unwrap_or_default()
            .into_iter()
            .filter(|archive| self.disabled.allows(archive))
            .filter(|archive| carries_jof_content(archive))
            .collect()
    }

    /// The `install/EternalJK` folder when its PK3s are to be probed for single
    /// pictures: None when it is absent or already a game directory.
    fn eternaljk_directory(&self, install: &Path) -> Option<PathBuf> {
        let mounted = [self.basegame.as_str(), self.game.as_str()]
            .iter()
            .any(|game| game.eq_ignore_ascii_case(COSMETICS_GAME));
        let directory = install.join(COSMETICS_GAME);
        (!mounted && directory.is_dir()).then_some(directory)
    }

    /// EternalJK's own crosshair pictures, `gfx/2d/crosshaira` and `crosshairj`,
    /// from the highest-priority PK3 in `install/EternalJK` that has each (jaPRO's
    /// `japro-assets.pk3`), as `(archive, path, image bytes)`. EternalJK mounts that
    /// folder above `base`, so these replace the base pictures there. Empty when
    /// the folder is a game directory already.
    pub(crate) fn eternaljk_crosshairs(&self, install: &Path) -> Vec<(PathBuf, String, Vec<u8>)> {
        let Some(directory) = self.eternaljk_directory(install) else {
            return Vec::new();
        };
        let archives: Vec<(PathBuf, VirtualFileSystem)> = sjk_vfs::pk3_search_order(&directory)
            .unwrap_or_default()
            .into_iter()
            .rev()
            .filter(|archive| self.disabled.allows(archive))
            .filter_map(|archive| {
                let mut probe = VirtualFileSystem::new();
                probe.mount_pk3(&archive).ok()?;
                Some((archive, probe))
            })
            .collect();
        ["a", "j"]
            .iter()
            .filter_map(|letter| {
                archives.iter().find_map(|(archive, probe)| {
                    ["tga", "png", "jpg"].iter().find_map(|extension| {
                        let path = format!("gfx/2d/crosshair{letter}.{extension}");
                        let asset = probe.read(&path).ok()??;
                        Some((archive.clone(), path, asset.bytes))
                    })
                })
            })
            .collect()
    }

    /// JoF EternalJK's chat emoji pictures, every `gfx/emoji/*.png` in the PK3s of
    /// `install/EternalJK` (jaPRO's `japro-assets.pk3` ships 175), as `(path as the
    /// archive stores it, image bytes)`, the highest-priority archive's copy of each
    /// name. EternalJK mounts that folder whole; only these pictures are taken from
    /// it here. Empty when the folder is a game directory already.
    pub(crate) fn eternaljk_emojis(&self, install: &Path) -> Vec<(String, Vec<u8>)> {
        let Some(directory) = self.eternaljk_directory(install) else {
            return Vec::new();
        };
        let mut found: Vec<(String, Vec<u8>)> = Vec::new();
        for archive in sjk_vfs::pk3_search_order(&directory)
            .unwrap_or_default()
            .into_iter()
            .rev()
            .filter(|archive| self.disabled.allows(archive))
        {
            let mut probe = VirtualFileSystem::new();
            if probe.mount_pk3(&archive).is_err() {
                continue;
            }
            for name in probe.list_files(crate::chat::emoji::FOLDER, ".png") {
                let path = format!("{}/{name}", crate::chat::emoji::FOLDER);
                if found
                    .iter()
                    .any(|(known, _)| known.eq_ignore_ascii_case(&path))
                {
                    continue;
                }
                let Some(asset) = probe.read(&path).ok().flatten() else {
                    continue;
                };
                found.push((probe.original_name(&path).unwrap_or(path), asset.bytes));
            }
        }
        found
    }

    /// Mount existing directories; unreadable archives warn without discarding other packs.
    pub(crate) fn mount(&self, install: &Path) -> Result<VirtualFileSystem, Box<dyn Error>> {
        // Large offline imports may opt into a higher per-asset ceiling. Keep the
        // ordinary viewer default and generic VFS policy unchanged.
        let limit = std::env::var("SJK_MAX_ASSET_MIB")
            .ok()
            .map(|value| value.parse::<u64>())
            .transpose()?
            .unwrap_or(1024)
            .checked_mul(1024 * 1024)
            .filter(|&n| n > 0)
            .ok_or("SJK_MAX_ASSET_MIB must be positive and fit in u64")?;
        let mut vfs = VirtualFileSystem::with_max_asset_bytes(limit);
        vfs.set_read_diagnostics(self.debug);
        // SJK's own content (Illuminate's holocron, the card's outline, the hub's
        // packs with the unlockables' art), below everything so game data with the same paths
        // replaces it.
        crate::illuminate::mount(&mut vfs)?;
        crate::card_outline::mount(&mut vfs)?;
        crate::sjk_packs::mount_below_game_data(&mut vfs);
        // JoF EJK's hats and capes, below everything else so they never
        // replace other content.
        let log = !COSMETICS_LOGGED.swap(true, Ordering::Relaxed);
        for pack in self.cosmetic_packs(install) {
            match vfs.mount_pk3(&pack) {
                Ok(_) if log => crate::log::progress(format_args!(
                    "JoF EJK content (hats, capes, client assets) from {}",
                    pack.display(),
                )),
                Ok(_) => {}
                Err(error) => crate::log::progress(format_args!(
                    "warning: skipping cosmetics PK3 {}: {error}",
                    pack.display(),
                )),
            }
        }
        // Older generated packs in the game directories, left out when SJK's own
        // folder has one (`generated_packs.rs`).
        let generated = super::generated::present();
        let mut replaced = Vec::new();
        for directory in self.directories(install)? {
            if !directory.is_dir() {
                continue;
            }
            if !self.directory_first {
                vfs.mount_directory(&directory)?;
            }
            for archive in sjk_vfs::pk3_search_order(&directory)? {
                if self.disabled.allows(&archive) {
                    match vfs.mount_pk3(&archive) {
                        Ok(id) if generated && super::generated::is_generated(&vfs, id) => {
                            super::generated::log_replaced(&archive);
                            replaced.push(id);
                        }
                        Ok(_) => {}
                        Err(error) => crate::log::progress(format_args!(
                            "warning: skipping PK3 {}: {error}",
                            archive.display()
                        )),
                    }
                }
            }
            if self.directory_first {
                vfs.mount_directory(&directory)?;
            }
        }
        // EternalJK's crosshair pictures over the game's own, as EternalJK itself
        // mounts its folder above `base`; only those images, nothing else from
        // the pack.
        let crosshairs = self.eternaljk_crosshairs(install);
        if !crosshairs.is_empty() {
            if log {
                for (archive, path, _) in &crosshairs {
                    crate::log::progress(format_args!(
                        "crosshair picture {path} from {}",
                        archive.display(),
                    ));
                }
            }
            vfs.mount_memory(
                "EternalJK crosshairs",
                crosshairs.into_iter().map(|(_, path, bytes)| (path, bytes)),
            )?;
        }
        // JoF EternalJK's chat emojis, likewise only those pictures from the folder.
        let emojis = self.eternaljk_emojis(install);
        if !emojis.is_empty() {
            if log {
                crate::log::progress(format_args!(
                    "{} chat emoji pictures from {}",
                    emojis.len(),
                    install.join(COSMETICS_GAME).display(),
                ));
            }
            vfs.mount_memory("EternalJK chat emojis", emojis)?;
        }
        // `SJK_CONTENT=<dir>[:<dir>...]`: further content directories (loose files and
        // PK3s), above the installation: locally made content that has no place in it.
        for directory in std::env::var_os("SJK_CONTENT")
            .iter()
            .flat_map(std::env::split_paths)
            .filter(|directory| directory.is_dir())
        {
            vfs.mount_pk3_directory_with_warnings(&directory, |path, error| {
                crate::log::progress(format_args!(
                    "warning: skipping PK3 {}: {error}",
                    path.display()
                ));
            })?;
            vfs.mount_directory(&directory)?;
        }
        if !replaced.is_empty() {
            vfs = vfs.without_mounts(&replaced);
        }
        // The player's generated packs, above everything: no other pack replaces them.
        super::generated::mount_on_top(&mut vfs);
        if self.debug {
            for mount in vfs.mounts() {
                crate::log::progress(format_args!(
                    "fs mount: {} ({} entries)",
                    mount.name, mount.entries,
                ));
            }
        }
        Ok(vfs)
    }
}

/// Whether `archive` holds hat or cape models.
fn carries_jof_content(archive: &Path) -> bool {
    let mut probe = VirtualFileSystem::new();
    probe.mount_pk3(archive).is_ok()
        && (COSMETIC_FOLDERS
            .iter()
            .any(|folder| !probe.list_files(folder, ".md3").is_empty())
            || !probe.list_files(JOF_CLIENT_FOLDER, "").is_empty())
}

fn validate_directory(name: &str) -> Result<(), Box<dyn Error>> {
    if name == "."
        || name.contains("..")
        || name.contains(['/', '\\', ':'])
        || name.chars().any(char::is_control)
        || name.trim() != name
    {
        return Err(
            format!("invalid game directory {name:?}: expected a single directory name",).into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn pk3(path: &Path, entries: &[&str]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for entry in entries {
            zip.start_file(*entry, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"x").unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn only_eternaljk_packs_with_hats_capes_or_jof_pictures_are_found() {
        let install = tempfile::tempdir().unwrap();
        let folder = install.path().join("EternalJK");
        std::fs::create_dir_all(&folder).unwrap();
        pk3(
            &folder.join("zzz_jof_cosmetics.pk3"),
            &[
                "models/cosmetics/hats/santahat.md3",
                "shaders/japro_hats.shader",
            ],
        );
        pk3(
            &folder.join("capes_only.pk3"),
            &["models/cosmetics/capes/royalcape.md3"],
        );
        pk3(&folder.join("menus.pk3"), &["ui/jamp/main.menu"]);
        pk3(
            &folder.join("jofclient-assets.pk3"),
            &["gfx/jof/force_dash.tga", "sound/jof/repulse.mp3"],
        );
        let options = Options::default();
        let names: Vec<String> = options
            .cosmetic_packs(install.path())
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 3, "{names:?}");
        assert!(names.iter().all(|name| name != "menus.pk3"));
        // The hats reach the mounted file system, the menus do not.
        let vfs = options.mount(install.path()).unwrap();
        assert!(vfs.contains("models/cosmetics/hats/santahat.md3").unwrap());
        assert!(vfs.contains("gfx/jof/force_dash.tga").unwrap());
        assert!(!vfs.contains("ui/jamp/main.menu").unwrap());
        // With the folder as a game directory, it is mounted whole instead.
        let whole = Options {
            basegame: "EternalJK".to_owned(),
            ..Options::default()
        };
        assert!(whole.cosmetic_packs(install.path()).is_empty());
    }

    #[test]
    fn disabling_a_pack_restores_the_lower_priority_asset_and_can_be_undone() {
        let install = tempfile::tempdir().unwrap();
        let base = install.path().join("base");
        std::fs::create_dir_all(&base).unwrap();
        pk3(&base.join("a.pk3"), &["models/fixture.md3"]);
        pk3(&base.join("zzz.pk3"), &["models/fixture.md3"]);
        let mut options = Options::default();
        let source = |options: &Options| {
            options
                .mount(install.path())
                .unwrap()
                .read("models/fixture.md3")
                .unwrap()
                .unwrap()
                .source
                .mount_name
                .to_string()
        };
        assert!(source(&options).ends_with("zzz.pk3"));
        options.disabled.toggle("BASE/ZZZ.PK3");
        assert!(source(&options).ends_with("a.pk3"));
        options.disabled.toggle("base/zzz.pk3");
        assert!(source(&options).ends_with("zzz.pk3"));
    }

    #[test]
    fn disabled_eternaljk_packs_do_not_reenter_as_supplemental_assets() {
        let install = tempfile::tempdir().unwrap();
        let folder = install.path().join("EternalJK");
        std::fs::create_dir_all(&folder).unwrap();
        pk3(
            &folder.join("extras.pk3"),
            &[
                "models/cosmetics/hats/example.md3",
                "gfx/2d/crosshaira.png",
                "gfx/emoji/example.png",
            ],
        );
        let mut options = Options::default();
        assert_eq!(options.cosmetic_packs(install.path()).len(), 1);
        assert_eq!(options.eternaljk_crosshairs(install.path()).len(), 1);
        assert_eq!(options.eternaljk_emojis(install.path()).len(), 1);
        options.disabled.toggle("eternaljk/extras.pk3");
        assert!(options.cosmetic_packs(install.path()).is_empty());
        assert!(options.eternaljk_crosshairs(install.path()).is_empty());
        assert!(options.eternaljk_emojis(install.path()).is_empty());
    }

    #[test]
    fn eternaljk_emojis_are_mounted_without_the_rest_of_their_pack() {
        let install = tempfile::tempdir().unwrap();
        let folder = install.path().join("EternalJK");
        std::fs::create_dir_all(&folder).unwrap();
        pk3(
            &folder.join("japro-assets.pk3"),
            &[
                "gfx/emoji/`poop`.png",
                "gfx/emoji/#~`!D.png",
                "gfx/emoji/readme.txt",
                "ui/jamp/ingame.menu",
            ],
        );
        let options = Options::default();
        let vfs = options.mount(install.path()).unwrap();
        let mut listed = vfs.list_files("gfx/emoji", ".png");
        listed.sort();
        assert_eq!(listed, ["#~`!d.png", "`poop`.png"]);
        // The stored name keeps its case for the `!` rule.
        assert_eq!(
            vfs.original_name("gfx/emoji/#~`!d.png").as_deref(),
            Some("gfx/emoji/#~`!D.png")
        );
        assert!(!vfs.contains("ui/jamp/ingame.menu").unwrap());
        assert!(!vfs.contains("gfx/emoji/readme.txt").unwrap());
    }

    #[test]
    fn eternaljk_crosshairs_override_the_base_ones_alone() {
        let install = tempfile::tempdir().unwrap();
        let base = install.path().join("base");
        let folder = install.path().join("EternalJK");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&folder).unwrap();
        pk3(
            &base.join("hd_icons.pk3"),
            &[
                "gfx/2d/crosshaira.tga",
                "gfx/2d/crosshairb.tga",
                "gfx/2d/crosshairj.tga",
            ],
        );
        pk3(
            &folder.join("japro-assets.pk3"),
            &[
                "gfx/2d/crosshaira.tga",
                "gfx/2d/crosshairj.tga",
                "ui/jamp/ingame.menu",
            ],
        );
        let options = Options::default();
        let found: Vec<String> = options
            .eternaljk_crosshairs(install.path())
            .into_iter()
            .map(|(_, path, _)| path)
            .collect();
        assert_eq!(found, ["gfx/2d/crosshaira.tga", "gfx/2d/crosshairj.tga"]);
        let vfs = options.mount(install.path()).unwrap();
        let from = |path: &str| {
            let asset = vfs.read(path).unwrap().unwrap();
            vfs.mounts()
                .find(|mount| mount.id == asset.source.mount_id)
                .map(|mount| mount.name.to_string())
                .unwrap()
        };
        assert_eq!(from("gfx/2d/crosshaira.tga"), "EternalJK crosshairs");
        assert_eq!(from("gfx/2d/crosshairj.tga"), "EternalJK crosshairs");
        assert_ne!(from("gfx/2d/crosshairb.tga"), "EternalJK crosshairs");
        // Nothing else from that pack is mounted.
        assert!(!vfs.contains("ui/jamp/ingame.menu").unwrap());
    }
}
