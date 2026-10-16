//! JoF EJK's hats and capes: `.md3` models worn on a player's `*head_top`
//! and `*back` bolts, chosen by name after the saber colour digits of
//! `color1` and `color2` (see [`sjk_client::split_color_value`]).
//!
//! A piece is any model in `models/cosmetics/hats/` or
//! `models/cosmetics/capes/` (`models/players/hats/` and `capes/` when the
//! new folders are empty, as older packs used them), and JoF's catalogue
//! names are looked up in both. A piece may ship
//! `settings/cosmetics/<kind>/<name>.cosmetic`, JSON nudging it into place
//! per model and skin; the format is JoF EJK's and TaystJK's
//! (`CG_LoadCosmeticOffsets`). `cg_cosmetics` chooses whose pieces are
//! drawn. This module holds the data rules; the menu lists the
//! [`Catalog`], [`actors`] resolves and submits what players wear, and
//! [`command`] is JoF's `cosmetics` console command.

pub(crate) mod actors;
pub(crate) mod command;

use crate::actor_instance::ActorInstance;
use crate::bolt::{self, BoltMatrix};
use glam::{Mat3, Quat, Vec3};
use sjk_client::CosmeticSlot;
use sjk_vfs::VirtualFileSystem;

/// `cg_cosmetics` (JoF EJK, archived, default 1).
pub(crate) const VISIBILITY_CVAR: &str = "cg_cosmetics";

/// Whose cosmetics are drawn (`JAPRO_COSMETICS_*`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Visibility {
    Off,
    On,
    OnlyMe,
}

impl Visibility {
    /// Read `cg_cosmetics`; JoF treats anything out of range as on.
    pub(crate) fn from_cvar(value: i64) -> Self {
        match value {
            0 => Self::Off,
            2 => Self::OnlyMe,
            _ => Self::On,
        }
    }

    pub(crate) fn cvar_value(self) -> &'static str {
        match self {
            Self::Off => "0",
            Self::On => "1",
            Self::OnlyMe => "2",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::On => "On",
            Self::OnlyMe => "Only Me",
        }
    }

    /// Whether a piece on the local player (`local`) or another is drawn.
    pub(crate) fn shows(self, local: bool) -> bool {
        match self {
            Self::Off => false,
            Self::On => true,
            Self::OnlyMe => local,
        }
    }
}

/// Folders a slot's models are listed from, the newer first.
fn folders(slot: CosmeticSlot) -> [&'static str; 2] {
    match slot {
        CosmeticSlot::Hat => ["models/cosmetics/hats", "models/players/hats"],
        CosmeticSlot::Cape => ["models/cosmetics/capes", "models/players/capes"],
    }
}

/// Folder of a slot's `.cosmetic` fitting files.
fn settings_folder(slot: CosmeticSlot) -> &'static str {
    match slot {
        CosmeticSlot::Hat => "settings/cosmetics/hats",
        CosmeticSlot::Cape => "settings/cosmetics/capes",
    }
}

/// The JoF launcher's catalogue (`uiKnownHats`, `uiKnownCapes`), listed
/// even when only some are installed so the menu can say where the rest are.
const KNOWN_HATS: [&str; 24] = [
    "afro",
    "beard",
    "bucket",
    "cap",
    "cringe",
    "crown",
    "fedora",
    "fedora2",
    "fedora3",
    "fedora4",
    "glasses",
    "gradcap",
    "headcrab",
    "horns",
    "mario",
    "mask",
    "metalhelm",
    "plaguemask",
    "predatorhelm",
    "pumpkin",
    "santahat",
    "sombrero",
    "supersaiyan",
    "tophat",
];
const KNOWN_CAPES: [&str; 8] = [
    "ak47",
    "crowbar",
    "goose",
    "grogucape",
    "royalcape",
    "rpg",
    "vadercape",
    "yodacape",
];

fn known(slot: CosmeticSlot) -> &'static [&'static str] {
    match slot {
        CosmeticSlot::Hat => &KNOWN_HATS,
        CosmeticSlot::Cape => &KNOWN_CAPES,
    }
}

/// One installed piece.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Piece {
    /// The name worn in `color1`/`color2`.
    pub(crate) name: String,
    /// What the menu lists it as.
    pub(crate) display: String,
}

/// The installed pieces of both slots, sorted by display name.
#[derive(Clone, Debug, Default)]
pub(crate) struct Catalog {
    pieces: [Vec<Piece>; 2],
    /// Whether JoF's catalogue has pieces of the slot that are not installed.
    missing: [bool; 2],
}

impl Catalog {
    /// List what `vfs` holds, as JoF EJK's `UI_LoadCosmetics` does.
    pub(crate) fn scan(vfs: &VirtualFileSystem) -> Self {
        let mut catalog = Self::default();
        for slot in CosmeticSlot::ALL {
            let [folder, legacy] = folders(slot);
            let mut names = models_in(vfs, folder);
            if names.is_empty() {
                names = models_in(vfs, legacy);
            }
            let mut missing = false;
            for name in known(slot) {
                if names.iter().any(|known| known.eq_ignore_ascii_case(name)) {
                    continue;
                }
                if model_path(vfs, slot, name).is_some() {
                    names.push((*name).to_owned());
                } else {
                    missing = true;
                }
            }
            let mut pieces: Vec<Piece> = names
                .into_iter()
                .map(|name| Piece {
                    display: display_name(&name),
                    name,
                })
                .collect();
            pieces.sort_by_cached_key(|piece| piece.display.to_ascii_lowercase());
            catalog.pieces[slot.index()] = pieces;
            catalog.missing[slot.index()] = missing;
        }
        catalog
    }

    pub(crate) fn pieces(&self, slot: CosmeticSlot) -> &[Piece] {
        &self.pieces[slot.index()]
    }

    /// Position of the piece called `name` (any case).
    pub(crate) fn position(&self, slot: CosmeticSlot, name: &str) -> Option<usize> {
        self.pieces(slot)
            .iter()
            .position(|piece| piece.name.eq_ignore_ascii_case(name))
    }

    /// Whether some of JoF's catalogue is not installed.
    pub(crate) fn has_missing(&self, slot: CosmeticSlot) -> bool {
        self.missing[slot.index()]
    }
}

/// Wearable model names in `folder`: `.md3` files directly in it whose name
/// can travel in `color1`/`color2`.
fn models_in(vfs: &VirtualFileSystem, folder: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for file in vfs.list_files(folder, ".md3") {
        if file.contains('/') {
            continue;
        }
        let name = &file[..file.len() - ".md3".len()];
        if sjk_client::valid_cosmetic_name(name)
            && !names.iter().any(|known| known.eq_ignore_ascii_case(name))
        {
            names.push(name.to_owned());
        }
    }
    names
}

/// The model a worn `name` draws, if installed.
pub(crate) fn model_path(
    vfs: &VirtualFileSystem,
    slot: CosmeticSlot,
    name: &str,
) -> Option<String> {
    if !sjk_client::valid_cosmetic_name(name) {
        return None;
    }
    folders(slot).into_iter().find_map(|folder| {
        let path = format!("{folder}/{}.md3", name.to_ascii_lowercase());
        vfs.contains(&path).ok()?.then_some(path)
    })
}

/// `UI_CosmeticDisplayName`: `_` and `-` become spaces, the first letter a
/// capital.
pub(crate) fn display_name(name: &str) -> String {
    let mut display: String = name
        .chars()
        .map(|c| if c == '_' || c == '-' { ' ' } else { c })
        .collect();
    if let Some(first) = display.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    display
}

/// The fitting offset of piece `name` on `model`/`skin`, from its
/// `.cosmetic` file; none when there is no file or nothing matches.
pub(crate) fn fitting_offset(
    vfs: &VirtualFileSystem,
    slot: CosmeticSlot,
    name: &str,
    model: &str,
    skin: &str,
) -> [f32; 3] {
    let path = format!("{}/{name}.cosmetic", settings_folder(slot));
    let Some(asset) = vfs.read(&path).ok().flatten() else {
        return [0.0; 3];
    };
    let text = String::from_utf8_lossy(&asset.bytes);
    match offset_from_json(&text, model, skin) {
        Ok(offset) => offset.unwrap_or_default(),
        Err(reason) => {
            crate::log::progress(format_args!(
                "warning: ignoring offsets for cosmetic ({name}): {reason}"
            ));
            [0.0; 3]
        }
    }
}

/// `CG_LoadCosmeticOffsets` over the file's text: the skin's entry under
/// the model's wins; with no skin match, the model's own offsets apply when
/// it says `"modelFallback": true`. Keys match exactly, else the wildcard key
/// (`"kyle*"`) with the longest matching prefix.
fn offset_from_json(text: &str, model: &str, skin: &str) -> Result<Option<[f32; 3]>, String> {
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "not valid JSON".to_owned())?;
    let Some(model_entry) = json_match(&json, model) else {
        return Ok(None);
    };
    let fallback = model_entry
        .get("modelFallback")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if let Some(skin_entry) = json_match(model_entry, skin) {
        return offsets(skin_entry)
            .map(Some)
            .ok_or_else(|| format!("skin ({skin}) has no valid xOffset/yOffset/zOffset"));
    }
    if fallback {
        return offsets(model_entry)
            .map(Some)
            .ok_or_else(|| format!("model ({model}) has no valid xOffset/yOffset/zOffset"));
    }
    Ok(None)
}

/// `CG_CosmeticJSONMatch`.
fn json_match<'a>(object: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    let object = object.as_object()?;
    if let Some(exact) = object.get(name) {
        return Some(exact);
    }
    object
        .iter()
        .filter(|(_, entry)| entry.is_object())
        .filter_map(|(key, entry)| {
            let prefix = &key.as_bytes()[..key.find('*')?];
            // Compare bytes, as `Q_stricmpn` does: a player's model name may hold a
            // multi-byte character across the prefix length, and slicing the string
            // there panicked.
            name.as_bytes()
                .get(..prefix.len())
                .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
                .then_some((prefix.len(), entry))
        })
        .max_by_key(|(length, _)| *length)
        .map(|(_, entry)| entry)
}

/// `CG_CosmeticJSONOffsets`: all three numbers, or nothing.
fn offsets(node: &serde_json::Value) -> Option<[f32; 3]> {
    let read = |key: &str| node.get(key)?.as_f64().map(|value| value as f32);
    Some([read("xOffset")?, read("yOffset")?, read("zOffset")?])
}

/// Where a piece on raw bolt `raw` of an actor at `transform` is drawn:
/// `CG_DrawCosmeticOnPlayer`'s axes from `G2API_GetBoltMatrix`, its origin
/// two units down the bolt's up axis, then `offset` along the world axes.
pub(crate) fn placement(
    raw: BoltMatrix,
    transform: sjk_runtime::Transform,
    offset: [f32; 3],
) -> Option<ActorInstance> {
    placement_at(
        raw,
        Vec3::from_array(transform.translation),
        crate::weapon_view::actor_world_rotation(transform.rotation),
        Vec3::from_array(transform.scale),
        offset,
    )
}

/// [`placement`] for an actor whose world rotation already includes the
/// Ghoul2 facing turn ([`crate::weapon_view::actor_world_rotation`]), as the
/// menu stage keeps it.
pub(crate) fn placement_at(
    mut raw: BoltMatrix,
    origin: Vec3,
    rotation: Quat,
    scale: Vec3,
    offset: [f32; 3],
) -> Option<ActorInstance> {
    // G2API_GetBoltMatrix normalizes the three basis rows (as `flag_carrier`).
    for row in &mut raw {
        let length = Vec3::new(row[0], row[1], row[2]).length();
        if length != 0.0 {
            for component in &mut row[..3] {
                *component /= length;
            }
        }
    }
    let game = bolt::game_facing(raw);
    let axis = |index| rotation * Vec3::from_array(bolt::column(&game, index));
    let forward = axis(0).normalize_or_zero();
    let left = axis(1);
    let left = (left - forward * left.dot(forward)).normalize_or_zero();
    let up = forward.cross(left);
    if forward.length_squared() < 0.5 || left.length_squared() < 0.5 {
        return None;
    }
    let origin = origin + rotation * (Vec3::from_array(bolt::column(&game, 3)) * scale)
        - axis(2).normalize_or_zero() * 2.0
        + Vec3::from_array(offset);
    let quaternion = Quat::from_mat3(&Mat3::from_cols(forward, left, up));
    Some(ActorInstance::new(
        origin.to_array(),
        quaternion.to_array(),
        [1.0; 3],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_keys_match_model_names_with_symbols() {
        let json = serde_json::json!({ "abc*": { "x": 1 } });
        // `×` covers bytes 2..4, across the three-byte prefix: this panicked.
        assert!(json_match(&json, "ab×").is_none());
        assert!(json_match(&json, "abc×").is_some());
        assert!(json_match(&json, "ABcd").is_some());
    }

    #[test]
    fn display_names_read_as_jof_lists_them() {
        assert_eq!(display_name("santahat"), "Santahat");
        assert_eq!(display_name("royal_cape"), "Royal cape");
        assert_eq!(display_name("mc-fox"), "Mc fox");
    }

    #[test]
    fn visibility_reads_and_cycles_as_jof_does() {
        assert_eq!(Visibility::from_cvar(0), Visibility::Off);
        assert_eq!(Visibility::from_cvar(2), Visibility::OnlyMe);
        assert_eq!(Visibility::from_cvar(7), Visibility::On);
        assert!(Visibility::OnlyMe.shows(true) && !Visibility::OnlyMe.shows(false));
        assert!(!Visibility::Off.shows(true));
        for visibility in [Visibility::On, Visibility::OnlyMe, Visibility::Off] {
            let value: i64 = visibility.cvar_value().parse().unwrap();
            assert_eq!(Visibility::from_cvar(value), visibility);
        }
    }

    #[test]
    fn offsets_pick_the_skin_then_the_model_fallback() {
        let text = r#"{
            "kyle": { "modelFallback": true, "xOffset": 0, "yOffset": 0, "zOffset": 2,
                      "red": { "xOffset": 0, "yOffset": 1, "zOffset": 3 } },
            "de*": { "xOffset": 1, "yOffset": 0, "zOffset": 0,
                     "def*": { "xOffset": 4, "yOffset": 4, "zOffset": 4 } },
            "desann*": { "modelFallback": true, "xOffset": 9, "yOffset": 9, "zOffset": 9 }
        }"#;
        assert_eq!(
            offset_from_json(text, "kyle", "red"),
            Ok(Some([0.0, 1.0, 3.0]))
        );
        assert_eq!(
            offset_from_json(text, "kyle", "blue"),
            Ok(Some([0.0, 0.0, 2.0]))
        );
        // `de*` has no fallback, but its `def*` skin key matches.
        assert_eq!(
            offset_from_json(text, "dendo", "default"),
            Ok(Some([4.0; 3]))
        );
        assert_eq!(offset_from_json(text, "dendo", "blue"), Ok(None));
        // The longest wildcard prefix wins.
        assert_eq!(offset_from_json(text, "desann", "blue"), Ok(Some([9.0; 3])));
        assert_eq!(offset_from_json(text, "luke", "default"), Ok(None));
        assert!(offset_from_json("{ nope", "kyle", "default").is_err());
        let partial = r#"{ "kyle": { "red": { "xOffset": 1 } } }"#;
        assert!(offset_from_json(partial, "kyle", "red").is_err());
    }

    #[test]
    fn a_piece_sits_two_units_down_its_bolt() {
        // An identity bolt at (0, 0, 60) on an unrotated, unscaled actor.
        let raw = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 60.0],
        ];
        let transform = sjk_runtime::Transform {
            translation: [100.0, 0.0, 0.0],
            rotation: Quat::IDENTITY.to_array(),
            scale: [1.0; 3],
        };
        let instance = placement(raw, transform, [0.0, 0.0, 1.0]).expect("placed");
        let position = Vec3::from_array(instance.position);
        assert!((position - Vec3::new(100.0, 0.0, 59.0)).length() < 1e-4);
        let rotation = Quat::from_array(instance.rotation);
        assert!((rotation * Vec3::Z - Vec3::Z).length() < 1e-4);
    }
}
