//! The models the `modelviewer` page lists: every player and NPC body with each of its
//! skins (`models/players/<model>/model.glm` and its `model_<skin>.skin` files), and
//! every rigid `.md3` or `.glm` under `models/` (items, weapons, map objects, vehicles,
//! hats). The retail game's own come first (told by the archive, like every list of
//! installed content, `AGENTS.md`), each group players before props, then by name.

use super::model_viewer::{Entry, Kind};
use sjk_vfs::VirtualFileSystem;
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// Every model `vfs` holds, in the page's order.
pub(crate) fn scan(vfs: &VirtualFileSystem) -> Vec<Entry> {
    let paths: Vec<String> = vfs
        .paths()
        .into_iter()
        .map(|path| path.as_str().to_owned())
        .collect();
    let retail: HashSet<String> = sjk_client::retail_view(vfs)
        .map(|view| {
            view.paths()
                .into_iter()
                .map(|path| path.as_str().to_owned())
                .collect()
        })
        .unwrap_or_default();
    entries(&paths, &retail)
}

/// The page's entries for the asset `paths`; `retail` holds the paths the retail archives
/// have.
fn entries(paths: &[String], retail: &HashSet<String>) -> Vec<Entry> {
    let mut bodies: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for path in paths {
        if let Some(model) = path
            .strip_prefix("models/players/")
            .and_then(|rest| rest.strip_suffix("/model.glm"))
            .filter(|model| !model.is_empty() && !model.contains('/'))
        {
            bodies.entry(model).or_default();
        }
    }
    let mut list = Vec::new();
    for path in paths {
        let Some(rest) = path.strip_prefix("models/players/") else {
            continue;
        };
        let Some((model, file)) = rest.split_once('/') else {
            continue;
        };
        if let (Some(skins), Some(skin)) = (
            bodies.get_mut(model),
            file.strip_prefix("model_")
                .and_then(|file| file.strip_suffix(".skin"))
                .filter(|skin| !skin.is_empty()),
        ) {
            skins.insert(skin);
            list.push(Entry {
                name: format!("{model}/{skin}"),
                retail: retail.contains(path),
                kind: Kind::Player,
            });
        }
    }
    // A body with no `model_<skin>.skin` (a species, built of parts) is listed by itself.
    for (model, skins) in &bodies {
        if skins.is_empty() {
            list.push(Entry {
                name: (*model).to_owned(),
                retail: retail.contains(&format!("models/players/{model}/model.glm")),
                kind: Kind::Player,
            });
        }
    }
    for path in paths {
        if !path.starts_with("models/") {
            continue;
        }
        let kind = if path.ends_with(".md3") {
            Kind::Md3
        } else if path.ends_with(".glm") && !path.starts_with("models/players/") {
            Kind::Glm
        } else {
            continue;
        };
        list.push(Entry {
            name: path.clone(),
            retail: retail.contains(path),
            kind,
        });
    }
    list.sort_by(|a, b| {
        (!a.retail, a.kind.is_prop(), &a.name).cmp(&(!b.retail, b.kind.is_prop(), &b.name))
    });
    list.dedup_by(|a, b| a.name == b.name);
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn bodies_with_each_skin_and_rigid_models_come_retail_first_players_before_props() {
        let all = paths(&[
            "models/players/kyle/model.glm",
            "models/players/kyle/model_default.skin",
            "models/players/kyle/model_hoth.skin",
            "models/players/kyle/head_default.skin",
            "models/players/rancor/model.glm",
            "models/players/rancor/model_default.skin",
            "models/players/jedi_hf/model.glm",
            "models/players/jedi_hf/head_a1.skin",
            "models/players/aaa_mod/model.glm",
            "models/players/aaa_mod/model_default.skin",
            "models/items/medpac.md3",
            "models/map_objects/crate/crate.glm",
            "models/players/hat/hat.md3",
            "models/players/odd/spare.glm",
            "textures/models/not_a_model.md3",
            "sound/x.wav",
        ]);
        let retail: HashSet<String> = [
            "models/players/kyle/model.glm",
            "models/players/kyle/model_default.skin",
            "models/players/kyle/model_hoth.skin",
            "models/players/rancor/model.glm",
            "models/players/rancor/model_default.skin",
            "models/players/jedi_hf/model.glm",
            "models/items/medpac.md3",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let list: Vec<(String, bool, Kind)> = entries(&all, &retail)
            .into_iter()
            .map(|e| (e.name, e.retail, e.kind))
            .collect();
        let expected: Vec<(String, bool, Kind)> = [
            ("jedi_hf", true, Kind::Player),
            ("kyle/default", true, Kind::Player),
            ("kyle/hoth", true, Kind::Player),
            ("rancor/default", true, Kind::Player),
            ("models/items/medpac.md3", true, Kind::Md3),
            ("aaa_mod/default", false, Kind::Player),
            ("models/map_objects/crate/crate.glm", false, Kind::Glm),
            ("models/players/hat/hat.md3", false, Kind::Md3),
        ]
        .into_iter()
        .map(|(name, retail, kind)| (name.to_owned(), retail, kind))
        .collect();
        assert_eq!(list, expected);
    }
}
