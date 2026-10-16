//! The player models this game has, for the hub (`PROTOCOL.md` in the hub, "Models"):
//! the folders under `models/players` holding a `model.glm` or a rig model's
//! `sjk_rig.cfg` ([`crate::rig_model::MANIFEST`]), in the game data mounted at start.
//! The player card asks the hub whether the inspected player's list holds the model the
//! local player wears (`player_identity::model_sight`).
//!
//! Listing reads every path of every mount, so it runs on a thread of its own, once
//! for each game data the viewer mounts. A server's downloads are not counted: they
//! change from server to server.

use sjk_vfs::VirtualFileSystem;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

/// The setting that sends the list (on by default).
pub(crate) const CVAR: &str = "cl_shareModels";
/// Names the hub takes at most, and characters a name may have.
const MODELS_MAX: usize = 4000;
const NAME_MAX: usize = 64;

/// What [`follow`] found in one game data.
#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct Models {
    /// Every model folder name, lower case, sorted, at most [`MODELS_MAX`].
    pub(crate) all: Vec<String>,
    /// The retail game's own, which every player has.
    pub(crate) retail: BTreeSet<String>,
}

#[derive(Default)]
struct State {
    /// The game data listed (or being listed), by address.
    vfs: usize,
    models: Option<Arc<Models>>,
}

static STATE: Mutex<State> = Mutex::new(State {
    vfs: 0,
    models: None,
});

fn lock() -> std::sync::MutexGuard<'static, State> {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// List the models of `vfs` on a thread, unless this game data was listed already.
pub(crate) fn follow(vfs: &Arc<VirtualFileSystem>) {
    let address = Arc::as_ptr(vfs) as usize;
    {
        let mut state = lock();
        if state.vfs == address {
            return;
        }
        state.vfs = address;
        state.models = None;
    }
    let vfs = Arc::clone(vfs);
    let spawned = std::thread::Builder::new()
        .name("sjk-installed-models".to_owned())
        .spawn(move || {
            let retail = sjk_client::retail_view(&vfs)
                .map(|retail| list(&retail).into_iter().collect())
                .unwrap_or_default();
            let models = Models {
                all: list(&vfs),
                retail,
            };
            let mut state = lock();
            if state.vfs == Arc::as_ptr(&vfs) as usize {
                state.models = Some(Arc::new(models));
            }
        });
    if spawned.is_err() {
        lock().vfs = 0;
    }
}

/// The models of the game data last given to [`follow`], once listed.
pub(crate) fn models() -> Option<Arc<Models>> {
    lock().models.clone()
}

/// The model folders of `vfs` the hub takes: lower case, sorted, without names it
/// would refuse (spaces, other than printable ASCII, too long).
fn list(vfs: &VirtualFileSystem) -> Vec<String> {
    let mut names = BTreeSet::new();
    for (extension, file) in [(".glm", "model.glm"), (".cfg", crate::rig_model::MANIFEST)] {
        for path in vfs.list_files("models/players", extension) {
            if let Some((name, found)) = path.split_once('/')
                && found.eq_ignore_ascii_case(file)
            {
                names.insert(name.to_ascii_lowercase());
            }
        }
    }
    names
        .into_iter()
        .filter(|name| valid(name))
        .take(MODELS_MAX)
        .collect()
}

fn valid(name: &str) -> bool {
    (1..=NAME_MAX).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b'/' && byte != b'\\')
}

/// The model folder of a `model` value (`kyle/red`, `*jedi`): what the hub lists, lower
/// case; `None` when it names none.
pub(crate) fn folder(model: &str) -> Option<String> {
    let model = model.trim().trim_start_matches('*');
    let folder = model.split('/').next()?.to_ascii_lowercase();
    valid(&folder).then_some(folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_with_a_mesh_or_a_rig_manifest_are_listed() {
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_memory(
            "test",
            [
                ("models/players/Kyle/model.glm", b"x".as_slice()),
                ("models/players/kyle/model_default.skin", b"x".as_slice()),
                ("models/players/wolfie/sjk_rig.cfg", b"x".as_slice()),
                ("models/players/skins_only/model_red.skin", b"x".as_slice()),
                ("models/players/two words/model.glm", b"x".as_slice()),
                ("models/players/deep/a/model.glm", b"x".as_slice()),
            ]
            .map(|(path, bytes)| (path.to_owned(), bytes.to_vec())),
        )
        .unwrap();
        assert_eq!(list(&vfs), ["kyle", "wolfie"]);
    }

    #[test]
    fn a_model_value_names_its_folder() {
        assert_eq!(folder("Kyle/red").as_deref(), Some("kyle"));
        assert_eq!(folder("*jedi").as_deref(), Some("jedi"));
        assert_eq!(folder("wolfie").as_deref(), Some("wolfie"));
        assert_eq!(folder(""), None);
        assert_eq!(folder("two words/x"), None);
    }
}
