//! The player's generated packs (`docs/rendering.md`, "Generating material maps"):
//! what `sjk-materialgen` writes into the `generated/` folder of the client's files
//! (`GameData/SJK/generated`, or the per-user SJK folder's). They are mounted last,
//! above every game, server and downloaded pack, so no other PK3 replaces their maps.
//!
//! When the folder holds one, an older generated pack left in a game directory (a
//! `zzz_sjk_materials.pk3` or `zzz_jkr_materials.pk3` copied into `base` by hand, known
//! by its manifest) is left out, so its stale maps do not fill in for textures the new
//! pack decided against.

use sjk_vfs::{MountId, VirtualFileSystem};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// The folder beside `config.cfg` that `sjk-materialgen` writes into by default.
pub(crate) const FOLDER: &str = "generated";
/// The manifests a generated pack carries: today's tool, and its JKR-era name.
const MANIFESTS: [&str; 2] = [
    "sjk-materialgen/manifest.json",
    "jkr-materialgen/manifest.json",
];

/// The folders searched, lowest priority first, once known.
static DIRECTORIES: OnceLock<Vec<PathBuf>> = OnceLock::new();
/// The packs and the ones they replace were named in the log (every world mounts them).
static LOGGED: AtomicBool = AtomicBool::new(false);

/// Keep the folders to search: the per-user SJK folder's `generated/` (the tool's
/// default when `GameData/SJK` has no config) and `config_directory`'s. Called once at
/// start, after the settings are read; before that nothing is mounted.
pub(crate) fn locate_at_start(config_directory: &Path) {
    DIRECTORIES.get_or_init(|| {
        let per_user = crate::platform::per_user_config_file()
            .ok()
            .and_then(|config| config.parent().map(|folder| folder.join(FOLDER)));
        directories(per_user, config_directory.join(FOLDER))
    });
}

/// `per_user` then `client`, once each.
fn directories(per_user: Option<PathBuf>, client: PathBuf) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = per_user.into_iter().collect();
    if !folders.contains(&client) {
        folders.push(client);
    }
    folders
}

/// The generated packs, lowest priority first (each folder in Quake 3 order).
pub(crate) fn packs() -> Vec<PathBuf> {
    packs_in(DIRECTORIES.get().map(Vec::as_slice).unwrap_or_default())
}

fn packs_in(directories: &[PathBuf]) -> Vec<PathBuf> {
    directories
        .iter()
        .filter(|directory| directory.is_dir())
        .flat_map(|directory| sjk_vfs::pk3_search_order(directory).unwrap_or_default())
        .collect()
}

/// Whether the mount `id` of `vfs` is a generated pack (it has a manifest).
pub(crate) fn is_generated(vfs: &VirtualFileSystem, id: MountId) -> bool {
    MANIFESTS
        .iter()
        .any(|manifest| vfs.mount_contains(id, manifest).unwrap_or(false))
}

/// Whether this start mounts generated packs of its own (an older one in a game
/// directory is then left out).
pub(crate) fn present() -> bool {
    !packs().is_empty()
}

/// Mount the generated packs on top of `vfs`; a pack that cannot be read is named in
/// the log and left out. Called last, after the game directories and again after a
/// server's packs.
pub(crate) fn mount_on_top(vfs: &mut VirtualFileSystem) {
    mount_packs(vfs, &packs(), !LOGGED.swap(true, Ordering::Relaxed));
}

fn mount_packs(vfs: &mut VirtualFileSystem, packs: &[PathBuf], log: bool) {
    for pack in packs {
        match vfs.mount_pk3(pack) {
            Ok(_) if log => crate::log::progress(format_args!(
                "generated pack {} (mounted above all other packs)",
                pack.display()
            )),
            Ok(_) => {}
            Err(error) => crate::log::progress(format_args!(
                "warning: skipping generated pack {}: {error}",
                pack.display()
            )),
        }
    }
}

/// Log, once per start, that `archive` (an older generated pack in a game directory)
/// is left out.
pub(crate) fn log_replaced(archive: &Path) {
    if !LOGGED.load(Ordering::Relaxed) {
        crate::log::progress(format_args!(
            "generated pack {} left out: the one in {FOLDER}/ replaces it",
            archive.display()
        ));
    }
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
    fn folders_are_searched_once_each() {
        let client = PathBuf::from("GameData/SJK/generated");
        assert_eq!(
            directories(Some(client.clone()), client.clone()),
            vec![client.clone()]
        );
        let per_user = PathBuf::from("AppData/SJK/generated");
        assert_eq!(
            directories(Some(per_user.clone()), client.clone()),
            vec![per_user, client]
        );
    }

    #[test]
    fn generated_packs_are_known_by_their_manifest() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("zzz_jkr_materials.pk3");
        let new = root.path().join("zzz_sjk_materials.pk3");
        let map = root.path().join("map.pk3");
        pk3(&old, &["jkr-materialgen/manifest.json", "textures/a_n.png"]);
        pk3(&new, &["sjk-materialgen/manifest.json", "textures/a_n.png"]);
        pk3(&map, &["maps/a.bsp"]);
        let mut vfs = VirtualFileSystem::new();
        let ids: Vec<_> = [&old, &new, &map]
            .iter()
            .map(|pack| vfs.mount_pk3(pack).unwrap())
            .collect();
        assert!(is_generated(&vfs, ids[0]));
        assert!(is_generated(&vfs, ids[1]));
        assert!(!is_generated(&vfs, ids[2]));
    }

    #[test]
    fn generated_packs_go_on_top() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join(FOLDER);
        std::fs::create_dir(&folder).unwrap();
        let game = root.path().join("{JoF}textures.pk3");
        pk3(&game, &["textures/a_n.png"]);
        pk3(
            &folder.join("zzz_sjk_materials.pk3"),
            &["sjk-materialgen/manifest.json", "textures/a_n.png"],
        );
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_pk3(&game).unwrap();
        mount_packs(&mut vfs, &packs_in(&[folder]), false);
        let source = vfs.read("textures/a_n.png").unwrap().unwrap().source;
        assert!(source.mount_name.ends_with("zzz_sjk_materials.pk3"));
    }
}
