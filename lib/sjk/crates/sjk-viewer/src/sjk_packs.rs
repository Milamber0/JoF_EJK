//! SJK's own content packs (`docs/unlockables.md`, "Packs"): PK3s the SJK hub serves to
//! SJK clients, holding the art of unlockables (blade-skin files, their sounds and
//! images) under "all rights reserved", apart from SJK's open code. The identity
//! service downloads them into the `assets/` folder beside `identity.key`
//! (`sjk_identity::assets`); this module mounts what is there.
//!
//! At start every cached pack is mounted (whether the identity is on or not: the
//! content is SJK's own and already on the player's machine), and again whenever the
//! service writes a new one ([`follow_identity`]). Mounting reads the packs' blade-skin
//! files into the process-wide [`LoadedSkins`], which each world's `GpuState` takes
//! (uploading the skins' parameters and pairs and registering their sounds) the next
//! frame its generation differs ([`generation`]). The game's own file system also
//! mounts the cached packs, below all game data like the holocron
//! ([`mount_below_game_data`]), so a PK3 with the same paths replaces their files.

use crate::saber_skins::LoadedSkins;
use sjk_vfs::VirtualFileSystem;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

/// The cache folder, once known (the settings folder's `assets/`).
static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
/// The skins of the last mount; replaced whole, read rarely.
static SKINS: Mutex<Option<Arc<LoadedSkins>>> = Mutex::new(None);
/// The last mount's generation, read every frame without the lock.
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// The identity service's pack revision last mounted.
static SEEN_REVISION: AtomicU64 = AtomicU64::new(0);

fn lock() -> MutexGuard<'static, Option<Arc<LoadedSkins>>> {
    SKINS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The cache folder beside `identity.key` in `config_directory`.
pub(crate) fn directory_in(config_directory: &Path) -> PathBuf {
    config_directory.join(sjk_identity::assets::FOLDER)
}

/// Mount the packs cached in `config_directory`'s `assets/` and keep that folder for
/// later mounts. Called once at start, after the settings are read.
pub(crate) fn mount_at_start(config_directory: &Path) {
    let directory = DIRECTORY.get_or_init(|| directory_in(config_directory));
    mount(directory);
}

/// The generation of the skins mounted last; `GpuState` takes them when it differs.
pub(crate) fn generation() -> u64 {
    GENERATION.load(Ordering::Acquire)
}

/// The skins mounted last (none before any mount).
pub(crate) fn skins() -> Arc<LoadedSkins> {
    lock().clone().unwrap_or_default()
}

/// Every cached pack of `directory`, in name order: what [`mount_below_game_data`] and
/// [`mount`] mount.
fn cached(directory: &Path) -> Vec<PathBuf> {
    sjk_identity::assets::cached_packs(directory)
}

/// The packs of `directory` in a file system of their own; one that cannot be read is
/// named in the log and left out.
fn pack_file_system(directory: &Path) -> VirtualFileSystem {
    let mut packs = VirtualFileSystem::new();
    for pack in cached(directory) {
        if let Err(error) = packs.mount_pk3(&pack) {
            crate::log::progress(format_args!(
                "SJK packs: skipping {}: {error}",
                pack.display()
            ));
        }
    }
    packs
}

/// Mount the packs of `directory`: read their blade skins and make them the current
/// ones under a new generation. Returns the skins.
pub(crate) fn mount(directory: &Path) -> Arc<LoadedSkins> {
    let packs = pack_file_system(directory);
    let count = packs.mounts().count();
    let mut current = lock();
    let generation = GENERATION.load(Ordering::Acquire) + 1;
    crate::cards::faces::install(&packs, generation);
    let skins = Arc::new(LoadedSkins::load(packs, generation));
    if count > 0 {
        crate::log::progress(format_args!(
            "SJK packs: {count} from {}, blade skins: {}",
            directory.display(),
            skins.ids().collect::<Vec<_>>().join(", ")
        ));
    }
    *current = Some(Arc::clone(&skins));
    GENERATION.store(generation, Ordering::Release);
    skins
}

/// Mount the cached packs into a game file system being built, at its current (lowest)
/// position: called before any game data is mounted.
pub(crate) fn mount_below_game_data(vfs: &mut VirtualFileSystem) {
    let Some(directory) = DIRECTORY.get() else {
        return;
    };
    for pack in cached(directory) {
        if let Err(error) = vfs.mount_pk3(&pack) {
            crate::log::progress(format_args!(
                "warning: skipping SJK pack {}: {error}",
                pack.display()
            ));
        }
    }
}

/// Mount again when the identity service wrote a pack since the last look
/// (`revision`, its snapshot's `packs_revision`), and log what its last pack check did
/// (`note`) when that changes. Cheap when nothing changed.
pub(crate) fn follow_identity(revision: u64, note: Option<String>) {
    static NOTE: Mutex<Option<String>> = Mutex::new(None);
    if let Some(note) = note {
        let mut last = NOTE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if last.as_deref() != Some(note.as_str()) {
            crate::log::progress(format_args!("SJK packs: {note}"));
            *last = Some(note);
        }
    }
    if SEEN_REVISION.swap(revision, Ordering::AcqRel) == revision {
        return;
    }
    if let Some(directory) = DIRECTORY.get() {
        mount(directory);
    }
}

#[cfg(test)]
#[path = "sjk_packs_tests.rs"]
mod tests;
