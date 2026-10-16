//! Headless scan of every installed player model through the client's real
//! appearance path, compared with what rd-vanilla and the retail cgame would do
//! with the same files. It never opens a window or touches the network.
//!
//! The scan is an ignored test because it reads the local game installation:
//!
//! ```sh
//! JKA_GAME_DATA="/path/to/GameData" cargo test --release -p sjk-viewer \
//!     player_model_scan -- --ignored --nocapture
//! ```
//!
//! `JKA_MODEL_SCAN_GAMES=EternalJK` (comma-separated) mounts further game folders
//! above `base`, as `fs_basegame`/`fs_game` would, and then reports only the model
//! directories that have files in them. Results go to
//! `target/parity-reports/player-models/<base[+game...]>.tsv` in the workspace,
//! one row per model and skin, with a summary on standard output. See
//! [client.md](../../../docs/client.md#player-models) for the rules the
//! reference column follows.

use crate::actor_load::build_actor_mesh;
use crate::player_assets::{GlaCache, load_player_appearance_with};
use crate::scene_flatten::FlattenedScene;
use sjk_runtime::Appearance;
use sjk_vfs::{MountId, VirtualFileSystem};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// One model directory under `models/players`.
#[derive(Default)]
struct ModelDirectory {
    has_mesh: bool,
    /// Lower-case `.skin` file names directly in the directory.
    skins: BTreeSet<String>,
    /// Some file of the directory comes from a scanned extra folder.
    in_extra_folder: bool,
}

struct Row {
    model: String,
    skin: String,
    sjk: String,
    reference: String,
}

#[test]
#[ignore = "reads the installed game data named by JKA_GAME_DATA"]
fn player_model_scan() {
    let Some(game_data) = std::env::var_os("JKA_GAME_DATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    else {
        panic!("set JKA_GAME_DATA to the GameData directory to scan");
    };
    let extra = std::env::var("JKA_MODEL_SCAN_GAMES").unwrap_or_default();
    let extra = extra
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    let (vfs, extra_mounts) = mount(&game_data, &extra);
    let directories = model_directories(&vfs, &extra_mounts);
    let mut rows = Vec::new();
    let mut cache = GlaCache::default();
    for (name, directory) in &directories {
        if !extra.is_empty() && !directory.in_extra_folder {
            continue;
        }
        scan_directory(&vfs, name, directory, &mut cache, &mut rows);
    }
    let label = std::iter::once("base")
        .chain(extra.iter().copied())
        .collect::<Vec<_>>()
        .join("+");
    let report = write_report(&label, &rows);
    println!("{}", summary(&label, &rows));
    println!("report: {}", report.display());
}

fn mount(game_data: &Path, extra: &[&str]) -> (VirtualFileSystem, Vec<MountId>) {
    let mut vfs = VirtualFileSystem::with_max_asset_bytes(1024 * 1024 * 1024);
    let mut extra_mounts = Vec::new();
    for game in std::iter::once("base").chain(extra.iter().copied()) {
        let directory = game_data.join(game);
        assert!(
            directory.is_dir(),
            "{} is not a directory",
            directory.display()
        );
        let before = vfs.mounts().count();
        vfs.mount_directory(&directory).expect("mount game folder");
        vfs.mount_pk3_directory_with_warnings(&directory, |path, error| {
            eprintln!("warning: skipping PK3 {}: {error}", path.display());
        })
        .expect("mount game folder archives");
        if game != "base" {
            extra_mounts.extend(vfs.mounts().skip(before).map(|mount| mount.id));
        }
    }
    (vfs, extra_mounts)
}

fn model_directories(
    vfs: &VirtualFileSystem,
    extra_mounts: &[MountId],
) -> BTreeMap<String, ModelDirectory> {
    let mut directories = BTreeMap::<String, ModelDirectory>::new();
    for path in vfs.paths() {
        let Some(rest) = path.as_str().strip_prefix("models/players/") else {
            continue;
        };
        let Some((name, file)) = rest.split_once('/') else {
            continue;
        };
        if file.contains('/') || name.is_empty() {
            continue;
        }
        let directory = directories.entry(name.to_owned()).or_default();
        if file == "model.glm" {
            directory.has_mesh = true;
        } else if file.ends_with(".skin") {
            directory.skins.insert(file.to_owned());
        } else {
            continue;
        }
        if !directory.in_extra_folder {
            directory.in_extra_folder = extra_mounts.iter().any(|&mount| {
                vfs.read_from_mount(mount, path.as_str())
                    .is_ok_and(|asset| asset.is_some())
            });
        }
    }
    directories.retain(|_, directory| directory.has_mesh || !directory.skins.is_empty());
    directories
}

/// A skin no directory has, standing for a userinfo naming a skin that is not
/// installed (`zzzpiza/original`, `jedi/rgb` without EternalJK's assets).
const ABSENT: &str = "scan_absent";

/// The skins a player can ask for: every `model_<skin>.skin`, `default`, one
/// skin that is not installed and, when the directory has part files, one
/// three-part `head|torso|lower` combination and one with a missing lower part.
fn requested_skins(directory: &ModelDirectory) -> Vec<String> {
    let mut skins = directory
        .skins
        .iter()
        .filter_map(|file| file.strip_prefix("model_")?.strip_suffix(".skin"))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if !skins.iter().any(|skin| skin == "default") {
        skins.insert(0, "default".to_owned());
    }
    let part = |prefix: &str| {
        directory
            .skins
            .iter()
            .find(|file| file.starts_with(prefix))
            .and_then(|file| file.strip_suffix(".skin"))
    };
    skins.push(ABSENT.to_owned());
    if let (Some(head), Some(torso), Some(lower)) = (part("head_"), part("torso_"), part("lower_"))
    {
        skins.push(format!("{head}|{torso}|{lower}"));
        skins.push(format!("{head}|{torso}|lower_{ABSENT}"));
    }
    skins
}

fn scan_directory(
    vfs: &VirtualFileSystem,
    name: &str,
    directory: &ModelDirectory,
    cache: &mut GlaCache,
    rows: &mut Vec<Row>,
) {
    let model = format!("models/players/{name}");
    let mesh_reference = reference::mesh(vfs, &model);
    for skin in requested_skins(directory) {
        let sjk = sjk_result(vfs, &model, &skin, cache);
        let reference = match &mesh_reference {
            Ok(()) => reference::skin(vfs, &model, &skin),
            Err(rejection) => rejection.clone(),
        };
        rows.push(Row {
            model: name.to_owned(),
            skin,
            sjk,
            reference,
        });
    }
    // Part files are only ever read through a three-part skin; check each parses.
    for file in &directory.skins {
        if file.starts_with("model_") {
            continue;
        }
        let path = format!("{model}/{file}");
        let sjk = match vfs.read(&path) {
            Ok(Some(_)) => "ok".to_owned(),
            Ok(None) => "error: not found".to_owned(),
            Err(error) => format!("error: {error}"),
        };
        rows.push(Row {
            model: name.to_owned(),
            skin: format!("part {file}"),
            sjk,
            reference: reference::skin_file(vfs, &path).describe(),
        });
    }
}

/// Load `model`/`skin` the way a connected player's appearance is loaded, build
/// its actor mesh, and pose one running frame.
fn sjk_result(vfs: &VirtualFileSystem, model: &str, skin: &str, cache: &mut GlaCache) -> String {
    let preview = match load_player_appearance_with(vfs, model, skin, [0.0; 3], 0.0, cache) {
        Ok(preview) => preview,
        Err(error) => return format!("error: {error}"),
    };
    let running = preview
        .config
        .get("BOTH_RUN1")
        .map_or(preview.sequence.first_frame, |run| run.first_frame);
    if let Err(error) = preview
        .mesh
        .skin(&preview.animation, &preview.skin, running, 0)
    {
        return format!("error: posing BOTH_RUN1: {error}");
    }
    let appearance = Appearance {
        model: model.to_owned(),
        variant: skin.to_owned(),
    };
    let mut scene = FlattenedScene::default();
    let names = [Some("single_1".to_owned()), None];
    match build_actor_mesh(&mut scene, preview, None, false, appearance, names) {
        Ok(_) => "ok".to_owned(),
        Err(error) => format!("error building actor mesh: {error}"),
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn write_report(label: &str, rows: &[Row]) -> PathBuf {
    let directory = workspace_root().join("target/parity-reports/player-models");
    std::fs::create_dir_all(&directory).expect("create report directory");
    let path = directory.join(format!("{label}.tsv"));
    let mut text = String::from("model\tskin\tsjk\treference\n");
    for row in rows {
        let _ = writeln!(
            text,
            "{}\t{}\t{}\t{}",
            row.model, row.skin, row.sjk, row.reference
        );
    }
    std::fs::write(&path, text).expect("write report");
    path
}

/// Counts, and every row where SJK falls back but the reference keeps the model.
fn summary(label: &str, rows: &[Row]) -> String {
    let models = rows.iter().map(|row| &row.model).collect::<BTreeSet<_>>();
    let failed = |row: &&Row| row.sjk != "ok";
    let reference_keeps = |row: &&Row| !row.reference.starts_with("rejects");
    let mut text = format!(
        "{label}: {} model directories, {} rows, {} SJK failures, {} where the reference keeps the model ({} directories)\n",
        models.len(),
        rows.len(),
        rows.iter().filter(failed).count(),
        rows.iter().filter(failed).filter(reference_keeps).count(),
        rows.iter()
            .filter(failed)
            .filter(reference_keeps)
            .map(|row| &row.model)
            .collect::<BTreeSet<_>>()
            .len(),
    );
    let mut classes = BTreeMap::<String, usize>::new();
    for row in rows.iter().filter(failed) {
        *classes.entry(error_class(&row.sjk)).or_default() += 1;
    }
    for (class, count) in classes {
        let _ = writeln!(text, "  {count:5}  {class}");
    }
    text
}

/// An error message without the parts that name one file.
fn error_class(error: &str) -> String {
    let mut class = String::with_capacity(error.len());
    let mut quoted = false;
    for character in error.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                if quoted {
                    class.push_str("\"..\"");
                }
            }
            _ if quoted => {}
            '0'..='9' => {
                if !class.ends_with('#') {
                    class.push('#');
                }
            }
            _ => class.push(character),
        }
    }
    class
}

/// What rd-vanilla (`codemp/rd-vanilla`) and the retail cgame
/// (`CG_RegisterClientModelname`, `codemp/cgame/cg_players.c`) do with the same
/// files, read with no validation beyond theirs.
mod reference {
    use sjk_vfs::VirtualFileSystem;

    const HUMANOID_BONES: [&str; 5] = [
        "model_root",
        "upper_lumbar",
        "cranium",
        "Motion",
        "lower_lumbar",
    ];

    fn i32_at(data: &[u8], offset: usize) -> Option<i32> {
        Some(i32::from_le_bytes(
            data.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
        ))
    }

    fn usize_at(data: &[u8], offset: usize) -> Option<usize> {
        usize::try_from(i32_at(data, offset)?).ok()
    }

    fn name_at(data: &[u8], offset: usize) -> Option<String> {
        let bytes = data.get(offset..offset.checked_add(64)?)?;
        let end = bytes.iter().position(|&byte| byte == 0).unwrap_or(64);
        Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
    }

    /// `R_LoadMDXM`/`R_LoadMDXA` (`tr_ghoul2.cpp`) and the cgame's bad-model
    /// checks for a player (`cg_players.c` `CG_RegisterClientModelname`).
    pub(super) fn mesh(vfs: &VirtualFileSystem, model: &str) -> Result<(), String> {
        let read = |path: &str| vfs.read(path).ok().flatten().map(|asset| asset.bytes);
        let Some(mesh) = read(&format!("{model}/model.glm")) else {
            return Err("rejects: no model.glm (cgame default model)".to_owned());
        };
        if mesh.get(..4) != Some(b"2LGM") {
            return Err("rejects: unknown file id (RE_RegisterModel)".to_owned());
        }
        if i32_at(&mesh, 4) != Some(6) {
            return Err("rejects: GLM version is not 6 (R_LoadMDXM)".to_owned());
        }
        let animation_name = name_at(&mesh, 72).unwrap_or_default();
        // FS_FOpenFileRead (files.cpp) drops one leading slash.
        let animation_path = animation_name
            .strip_prefix(['/', '\\'])
            .unwrap_or(&animation_name);
        let Some(animation) = read(&format!("{animation_path}.gla")) else {
            return Err("rejects: missing animation file (R_LoadMDXM)".to_owned());
        };
        if animation.get(..4) != Some(b"2LGA") || i32_at(&animation, 4) != Some(6) {
            return Err("rejects: GLA id or version (R_LoadMDXA)".to_owned());
        }
        oversized_surface(&mesh).map_or(Ok(()), Err)?;
        if !animation_name.contains("players/_humanoid/") {
            return Err("rejects: skeleton is not players/_humanoid (cgame badModel)".to_owned());
        }
        let bones = bone_names(&animation);
        let surfaces = surface_names(&mesh);
        let has = |name: &str| {
            bones.iter().any(|bone| bone.eq_ignore_ascii_case(name))
                || surfaces
                    .iter()
                    .any(|surface| surface.eq_ignore_ascii_case(name))
        };
        if let Some(missing) = HUMANOID_BONES.iter().find(|bone| !has(bone)) {
            return Err(format!("rejects: no {missing} bone (cgame badModel)"));
        }
        for bolt in ["*r_hand", "*l_hand"] {
            if !has(bolt) {
                return Err(format!("rejects: no {bolt} bolt (cgame badModel)"));
            }
        }
        if !has("*head_top") && !has("ceyebrow") {
            return Err("rejects: no *head_top/ceyebrow bolt (cgame badModel)".to_owned());
        }
        Ok(())
    }

    /// `R_LoadMDXM` drops the client (`ERR_DROP`) for a surface over the
    /// tessellator's limits (`SHADER_MAX_VERTEXES` 1000, 6000 indexes).
    fn oversized_surface(mesh: &[u8]) -> Option<String> {
        let lods = usize_at(mesh, 144)?;
        let surfaces = usize_at(mesh, 152)?;
        let mut lod = usize_at(mesh, 148)?;
        for _ in 0..lods.min(32) {
            let mut surface = lod.checked_add(4 + surfaces.checked_mul(4)?)?;
            for _ in 0..surfaces {
                let vertices = i32_at(mesh, surface + 12)?;
                let triangles = i32_at(mesh, surface + 20)?;
                if vertices > 1000 || triangles.saturating_mul(3) > 6000 {
                    return Some(format!(
                        "rejects: surface with {vertices} vertices/{triangles} triangles (R_LoadMDXM ERR_DROP)"
                    ));
                }
                surface = surface.checked_add(usize_at(mesh, surface + 36)?.max(1))?;
            }
            lod = lod.checked_add(usize_at(mesh, lod)?.max(1))?;
        }
        None
    }

    fn bone_names(animation: &[u8]) -> Vec<String> {
        let count = usize_at(animation, 84).unwrap_or(0).min(1024);
        (0..count)
            .filter_map(|bone| {
                let offset = usize_at(animation, 100 + bone * 4)?;
                name_at(animation, 100usize.checked_add(offset)?)
            })
            .collect()
    }

    /// Hierarchy names with `_off` removed, as `R_LoadMDXM` stores them.
    fn surface_names(mesh: &[u8]) -> Vec<String> {
        let count = usize_at(mesh, 152).unwrap_or(0).min(1024);
        let mut offset = usize_at(mesh, 156).unwrap_or(usize::MAX);
        let mut names = Vec::with_capacity(count);
        for _ in 0..count {
            let Some(mut name) = name_at(mesh, offset) else {
                break;
            };
            if let Some(stripped) = name.strip_suffix("_off") {
                name = stripped.to_owned();
            }
            names.push(name);
            let children = usize_at(mesh, offset + 140).unwrap_or(0);
            offset = offset.saturating_add(144 + children.saturating_mul(4));
        }
        names
    }

    /// What `RE_RegisterIndividualSkin` makes of one file (`tr_skin.cpp`).
    pub(super) enum SkinFile {
        Missing,
        Surfaces(usize),
    }

    impl SkinFile {
        pub(super) fn describe(&self) -> String {
            match self {
                Self::Missing => "missing".to_owned(),
                Self::Surfaces(0) => "no surfaces (default skin)".to_owned(),
                Self::Surfaces(count) => format!("{count} surfaces"),
            }
        }
    }

    pub(super) fn skin_file(vfs: &VirtualFileSystem, path: &str) -> SkinFile {
        match vfs.read(path).ok().flatten() {
            // `Skin::parse` follows RE_RegisterIndividualSkin's CommaParse tokenizer.
            Some(asset) => SkinFile::Surfaces(sjk_model::Skin::parse(&asset.bytes).len()),
            None => SkinFile::Missing,
        }
    }

    /// `CG_RegisterClientModelname` (`cg_players.c`): a three-part name when it has
    /// `|`, `head`, `torso` and `lower`, else `model_<skin>.skin`; when
    /// `RE_RegisterSkin` returns 0 (a missing part or no surfaces), `model_default.skin`,
    /// and when that fails too the model draws its own shaders.
    pub(super) fn skin(vfs: &VirtualFileSystem, model: &str, skin: &str) -> String {
        let three_part = skin.contains('|')
            && skin.contains("head")
            && skin.contains("torso")
            && skin.contains("lower");
        let requested = if three_part {
            let parts = skin.splitn(3, '|').collect::<Vec<_>>();
            if parts.len() == 3 {
                let mut total = 0;
                let mut missing = None;
                for (index, part) in parts.iter().enumerate() {
                    // RE_RegisterSkin skips a part named like an earlier one.
                    if parts[..index].contains(part) {
                        continue;
                    }
                    match skin_file(vfs, &format!("{model}/{part}.skin")) {
                        SkinFile::Missing => {
                            missing = Some(*part);
                            break;
                        }
                        SkinFile::Surfaces(count) => total += count,
                    }
                    if total == 0 {
                        break;
                    }
                }
                match missing {
                    Some(part) => Err(format!("{part}.skin missing")),
                    None if total == 0 => Err("no surfaces".to_owned()),
                    None => Ok(()),
                }
            } else {
                Err("not three parts".to_owned())
            }
        } else {
            match skin_file(vfs, &format!("{model}/model_{skin}.skin")) {
                SkinFile::Missing => Err(format!("model_{skin}.skin missing")),
                SkinFile::Surfaces(0) => Err("no surfaces".to_owned()),
                SkinFile::Surfaces(_) => Ok(()),
            }
        };
        let Err(reason) = requested else {
            return "keeps: uses the skin".to_owned();
        };
        match skin_file(vfs, &format!("{model}/model_default.skin")) {
            SkinFile::Surfaces(count) if count > 0 => {
                format!("keeps: {reason}, uses model_default.skin")
            }
            _ => format!("keeps: {reason}, no default skin, draws GLM shaders"),
        }
    }
}
