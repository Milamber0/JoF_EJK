//! Rig models: a player model drawn from a glTF file with its own skeleton, posed
//! every frame from the stock `_humanoid` skeleton that still drives the player.
//!
//! A model folder holding [`MANIFEST`] is a rig model. The player keeps a stock
//! driver model (Kyle unless the manifest names another): its GLM and skeleton
//! carry the server's animations, bolts, saber traces and dismemberment exactly as
//! before, but are not drawn. The glTF mesh is drawn in their place, skinned with a
//! palette computed here from the driver's evaluated bone matrices: each mapped
//! joint takes its JKA bone's model-space rotation from the base pose, after its own
//! bind pose is turned onto the JKA bone's direction; unmapped joints (twist bones,
//! face, cloth) keep their bind pose under their parent. Which joint follows which
//! JKA bone is a bone map: a named preset from [`PRESETS`] plus `map` lines.
//!
//! Each frame the rig's lower ankle is planted where the driver's is (its height
//! above the sole kept), so a rig whose legs are longer or shorter for its pelvis
//! than the driver's neither sinks nor floats ([`Feet`]).
//!
//! Joints under a manifest's `dynamic` joint (a tail) move on their own, springing
//! after their animated place and wagging a little ([`dynamics`]).
//!
//! The glTF's own animations become clips ([`clips`]) a rig plays on its skeleton
//! in place of the driver's pose, fading in and out (emotes). An emote made on
//! another rig model (its file's `rig`) is borrowed when the two skeletons share
//! joint names, as Unreal mannequin characters do ([`RigModel::borrowed`]). Stock
//! GLM models play those clips too, through the bone map in reverse ([`stock`]).
//!
//! See `docs/rig-models.md` for the file format.

pub(crate) use clips::{Clip, ClipPlay};
use clips::{Convert, Rest};
use glam::{Affine3A, Mat3, Mat3A, Vec3, Vec3A};
use sjk_model::gltf::{GltfModel, parse};
use sjk_model::{Gla, Glm};
use sjk_vfs::VirtualFileSystem;
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, LazyLock, Mutex};
pub(crate) use stock::{StockEmotes, StockScratch};

/// The file that makes a model folder a rig model.
pub(crate) const MANIFEST: &str = "sjk_rig.cfg";

/// The driver's ankle bones, whose lower one says where the ground is.
const ANKLES: [&str; 2] = ["ltalus", "rtalus"];

/// The driver model when the manifest does not name one.
const DEFAULT_DRIVER: &str = "models/players/kyle";

/// A surface's shader when its material has no base colour image.
const UNTEXTURED_SHADER: &str = "*white";

/// One bone-map line: the rig joint, the JKA bone it follows, and how its bind pose
/// is aligned onto the JKA base pose.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MapEntry {
    pub(crate) joint: String,
    pub(crate) bone: String,
    pub(crate) align: Align,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Align {
    /// Keep the bind pose as it is (the head, the feet: no child to aim along).
    Rest,
    /// Turn the joint so its direction to `joint_end` matches the JKA bone's
    /// direction to `bone_end`.
    Toward { joint_end: String, bone_end: String },
    /// Use the alignment computed for another mapped joint (finger chains).
    As(String),
}

/// A named bone map and the function that builds it.
pub(crate) type Preset = (&'static str, fn() -> Vec<MapEntry>);

/// Built-in bone maps, by the skeleton naming they read. Supporting another naming
/// is a new entry here (or `map` lines in a model's manifest), never code elsewhere.
pub(crate) const PRESETS: &[Preset] = &[
    // Unreal Engine mannequin names (pelvis, spine_01..05, upperarm_l, thumb_01_l...),
    // the skeleton most game and community character rigs export. Checked on
    // 14/10/2026 with a 177-joint mannequin character in Blender against posed Kyle.
    ("mannequin", mannequin),
];

fn entry(joint: &str, bone: &str, align: Align) -> MapEntry {
    MapEntry {
        joint: joint.to_owned(),
        bone: bone.to_owned(),
        align,
    }
}

fn toward(joint_end: &str, bone_end: &str) -> Align {
    Align::Toward {
        joint_end: joint_end.to_owned(),
        bone_end: bone_end.to_owned(),
    }
}

/// The Unreal mannequin onto `_humanoid`. Five spine joints share JKA's three; JKA's
/// two-joint finger groups (thumb d1, index/middle d2, ring/pinky d4) drive all
/// three joints of each finger.
fn mannequin() -> Vec<MapEntry> {
    let mut map = vec![
        entry("pelvis", "pelvis", toward("spine_01", "lower_lumbar")),
        entry(
            "spine_01",
            "lower_lumbar",
            toward("spine_03", "upper_lumbar"),
        ),
        entry(
            "spine_02",
            "lower_lumbar",
            toward("spine_03", "upper_lumbar"),
        ),
        entry("spine_03", "upper_lumbar", toward("spine_04", "thoracic")),
        entry("spine_04", "thoracic", toward("neck_01", "cervical")),
        entry("spine_05", "thoracic", toward("neck_01", "cervical")),
        entry("neck_01", "cervical", toward("head", "cranium")),
        entry("neck_02", "cervical", toward("head", "cranium")),
        entry("head", "cranium", Align::Rest),
    ];
    for side in ["l", "r"] {
        let j = side;
        map.extend([
            entry(
                &format!("upperarm_{side}"),
                &format!("{j}humerus"),
                toward(&format!("lowerarm_{side}"), &format!("{j}radius")),
            ),
            entry(
                &format!("lowerarm_{side}"),
                &format!("{j}radius"),
                toward(&format!("hand_{side}"), &format!("{j}hand")),
            ),
            entry(
                &format!("hand_{side}"),
                &format!("{j}hand"),
                toward(&format!("middle_01_{side}"), &format!("{j}_d2_j1")),
            ),
            entry(
                &format!("thigh_{side}"),
                &format!("{j}femurYZ"),
                toward(&format!("calf_{side}"), &format!("{j}tibia")),
            ),
            entry(
                &format!("calf_{side}"),
                &format!("{j}tibia"),
                toward(&format!("foot_{side}"), &format!("{j}talus")),
            ),
            entry(&format!("foot_{side}"), &format!("{j}talus"), Align::Rest),
        ]);
        for (finger, group) in [
            ("thumb", "d1"),
            ("index", "d2"),
            ("middle", "d2"),
            ("ring", "d4"),
            ("pinky", "d4"),
        ] {
            let first = format!("{finger}_01_{side}");
            map.push(entry(
                &first,
                &format!("{j}_{group}_j1"),
                toward(&format!("{finger}_02_{side}"), &format!("{j}_{group}_j2")),
            ));
            for joint in ["02", "03"] {
                map.push(entry(
                    &format!("{finger}_{joint}_{side}"),
                    &format!("{j}_{group}_j2"),
                    Align::As(first.clone()),
                ));
            }
        }
    }
    map
}

/// The parsed manifest.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Manifest {
    /// The glTF file, relative to the model folder.
    pub(crate) model: Option<String>,
    /// The stock model whose GLM and skeleton drive the player.
    pub(crate) driver: String,
    pub(crate) preset: String,
    /// glTF metres to game units; `None` fits the pelvis to the driver's.
    pub(crate) scale: Option<f32>,
    /// Game units the whole rig is raised by (negative lowers it), on top of the
    /// feet planting: a hand tweak for a model whose soles are not its lowest point.
    pub(crate) lift: f32,
    pub(crate) map: Vec<MapEntry>,
    /// Joints whose chains below move on their own (`dynamic <joint> [gravity]`
    /// lines), with their gravity when the line gives one.
    pub(crate) dynamic: Vec<(String, Option<f32>)>,
}

impl Manifest {
    /// Read `text`: one `key value...` per line, `//` comments.
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let mut manifest = Self {
            model: None,
            driver: DEFAULT_DRIVER.to_owned(),
            preset: "mannequin".to_owned(),
            scale: None,
            lift: 0.0,
            map: Vec::new(),
            dynamic: Vec::new(),
        };
        for (number, line) in text.lines().enumerate() {
            let line = line.split("//").next().unwrap_or("").trim();
            let words: Vec<&str> = line.split_whitespace().collect();
            let bad = || format!("{MANIFEST} line {}: {line:?}", number + 1);
            match words.as_slice() {
                [] => {}
                ["model", file] => manifest.model = Some((*file).to_owned()),
                ["driver", directory] => {
                    manifest.driver = directory.trim_end_matches('/').to_owned()
                }
                ["preset", name] => manifest.preset = (*name).to_owned(),
                ["scale", "auto"] => manifest.scale = None,
                ["scale", value] => {
                    manifest.scale = Some(
                        value
                            .parse()
                            .ok()
                            .filter(|s: &f32| *s > 0.0)
                            .ok_or_else(bad)?,
                    )
                }
                ["lift", value] => {
                    manifest.lift = value
                        .parse()
                        .ok()
                        .filter(|lift: &f32| lift.is_finite())
                        .ok_or_else(bad)?
                }
                ["dynamic", joint] => manifest.dynamic.push(((*joint).to_owned(), None)),
                ["dynamic", joint, gravity] => manifest.dynamic.push((
                    (*joint).to_owned(),
                    Some(
                        gravity
                            .parse()
                            .ok()
                            .filter(|gravity: &f32| gravity.is_finite())
                            .ok_or_else(bad)?,
                    ),
                )),
                ["map", joint, bone] => manifest.map.push(entry(joint, bone, Align::Rest)),
                ["map", joint, bone, "as", other] => {
                    manifest
                        .map
                        .push(entry(joint, bone, Align::As((*other).to_owned())))
                }
                ["map", joint, bone, joint_end, bone_end] => {
                    manifest
                        .map
                        .push(entry(joint, bone, toward(joint_end, bone_end)))
                }
                _ => return Err(bad()),
            }
        }
        Ok(manifest)
    }

    /// The preset's entries with this manifest's `map` lines replacing or adding.
    fn bone_map(&self) -> Result<Vec<MapEntry>, String> {
        let preset = PRESETS
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(&self.preset))
            .ok_or_else(|| format!("{MANIFEST}: unknown preset {:?}", self.preset))?;
        let mut map = (preset.1)();
        for line in &self.map {
            match map.iter_mut().find(|e| e.joint == line.joint) {
                Some(existing) => *existing = line.clone(),
                None => map.push(line.clone()),
            }
        }
        Ok(map)
    }
}

/// The manifest of the model in `directory`, when it is a rig model.
pub(crate) fn manifest(
    vfs: &VirtualFileSystem,
    directory: &str,
) -> Result<Option<Manifest>, Box<dyn Error>> {
    let Some(asset) = vfs.read(&format!("{directory}/{MANIFEST}"))? else {
        return Ok(None);
    };
    Ok(Some(Manifest::parse(&String::from_utf8_lossy(
        &asset.bytes,
    ))?))
}

/// One rig joint, in the driver's model space and units.
#[derive(Clone, Debug)]
pub(crate) struct Joint {
    /// The glTF node's name: borrowed clips find their joints by it.
    name: String,
    parent: Option<usize>,
    bind: Affine3A,
    inverse_bind: Affine3A,
    /// The bind pose relative to the parent's.
    local: Affine3A,
    /// The glTF node's rest transform relative to its parent node, for clips.
    rest: Rest,
    /// For a joint without a parent joint: what its parent nodes add (bind times the
    /// inverse of its rest), so a clip's root lands where the bind pose has it.
    above: Affine3A,
}

/// A joint that follows a driver bone.
#[derive(Clone, Copy, Debug)]
struct Driven {
    bone: usize,
    /// The alignment onto the JKA base pose, times the bind rotation.
    rotation: Mat3A,
    /// The pelvis also follows the bone's translation from its base pose.
    translate: Option<Vec3A>,
}

/// Where a driver bolt hanging on a bone lands on the rig: the joint mapped to that
/// bone (or to its nearest mapped ancestor) and how much longer the rig's limb is.
#[derive(Clone, Copy, Debug)]
struct Anchor {
    joint: usize,
    /// The mapped driver bone and its base position.
    bone: usize,
    base: Vec3A,
    /// The rig joint's length over the driver bone's, along their `Toward` pair.
    reach: f32,
}

/// Planting the rig's feet on the driver's: the driver's ankle bones and the rig's
/// ankle joints mapped to them, and how far above its sole each one's ankle stands.
#[derive(Clone, Debug)]
struct Feet {
    /// Driver ankle bones and their base positions.
    ankles: Vec<(usize, Vec3A)>,
    /// The rig's joints mapped to them.
    joints: Vec<usize>,
    /// The driver's lower ankle above its lowest point, in its base pose.
    driver_ankle: f32,
    /// The rig's lower ankle above its lowest point, in its bind pose.
    rig_ankle: f32,
    /// The rig's lowest point in its bind pose (where a clip's soles are).
    rig_sole: f32,
}

/// The least share of the joints a clip moves that a rig must have (by name) to
/// borrow it.
const BORROW_SHARE: f32 = 0.6;

/// Rig models loaded only to lend their clips, by folder; `None` for one that did not
/// load. Kept for the session: a model re-exported while the game runs is read again
/// after a restart.
static SOURCES: LazyLock<Mutex<HashMap<String, Option<Arc<RigModel>>>>> =
    LazyLock::new(Mutex::default);

/// The rig model in `directory`, without its surfaces, fitted to `driver`.
fn source_rig(vfs: &VirtualFileSystem, directory: &str, driver: &Gla) -> Option<Arc<RigModel>> {
    let key = directory.to_ascii_lowercase();
    let mut sources = SOURCES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(known) = sources.get(&key) {
        return known.clone();
    }
    let load = || -> Result<RigModel, Box<dyn Error>> {
        let manifest = manifest(vfs, directory)?.ok_or("not a rig model")?;
        let file = manifest
            .model
            .clone()
            .unwrap_or_else(|| "model.gltf".to_owned());
        let path = format!("{directory}/{file}");
        let base = path
            .rsplit_once('/')
            .map_or("", |(base, _)| base)
            .to_owned();
        let read = |path: &str| vfs.read(path).ok().flatten().map(|asset| asset.bytes);
        let bytes = read(&path).ok_or_else(|| format!("{path:?} was not found"))?;
        let model = parse(&bytes, |uri| read(&join(&base, uri)))?;
        // Its mesh is measured for the body fit of stock models, then dropped.
        let mut rig = RigModel::fit(model, &base, &manifest, driver)?;
        rig.body = Some(rig.measure_body(driver));
        rig.surfaces = Vec::new();
        Ok(rig)
    };
    let loaded = match load() {
        Ok(rig) => Some(Arc::new(rig)),
        Err(error) => {
            crate::log::progress(format_args!("rig model {directory}: {error}"));
            None
        }
    };
    sources.insert(key, loaded.clone());
    loaded
}

/// The lowest point of `mesh` in its base pose: its first LOD's drawn surfaces (no
/// bolt tags, hidden surfaces or caps), where the driver's soles are.
pub(crate) fn sole(mesh: &Glm) -> Option<f32> {
    let lowest = mesh
        .lods
        .first()?
        .surfaces
        .iter()
        .filter(|surface| {
            mesh.hierarchy
                .get(surface.hierarchy_index)
                .is_some_and(|h| {
                    h.flags & 0x3 == 0 && !h.name.starts_with('*') && !h.name.contains("_cap_")
                })
        })
        .flat_map(|surface| &surface.vertices)
        .map(|vertex| vertex.position[2])
        .fold(f32::MAX, f32::min);
    (lowest < f32::MAX).then_some(lowest)
}

/// One drawn surface, its vertices in the driver's model space.
#[derive(Clone, Debug)]
pub(crate) struct Surface {
    pub(crate) shader: String,
    pub(crate) vertices: Vec<Vertex>,
    pub(crate) indices: Vec<u32>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Vertex {
    pub(crate) position: [f32; 3],
    pub(crate) normal: [f32; 3],
    pub(crate) texture_coordinates: [f32; 2],
    pub(crate) joints: [u16; 4],
    pub(crate) weights: [f32; 4],
}

impl Vertex {
    /// Influences with weight, counted from the first (they are sorted strongest first).
    pub(crate) fn influences(&self) -> usize {
        self.weights.iter().take_while(|w| **w > 0.0).count().max(1)
    }
}

/// A loaded rig model: joints, bone map and surfaces, shared by every actor wearing it.
#[derive(Debug)]
pub(crate) struct RigModel {
    pub(crate) joints: Vec<Joint>,
    /// Joint indices, parents before children.
    order: Vec<usize>,
    driven: Vec<Option<Driven>>,
    /// One per driver bone, for [`RigModel::retarget_bolt`].
    anchors: Vec<Option<Anchor>>,
    /// The driver's pelvis bone and base position: a clip follows its height.
    pelvis: Option<(usize, Vec3A)>,
    /// Planting the feet, when the driver and the bone map have ankles.
    feet: Option<Feet>,
    /// The manifest's `lift`.
    lift: f32,
    /// The joints that move on their own.
    chains: dynamics::Chains,
    pub(crate) clips: Vec<Clip>,
    pub(crate) surfaces: Vec<Surface>,
    /// Its body measured on the driver's bones, for a rig loaded to lend its clips
    /// to stock models ([`body`]).
    body: Option<body::Body>,
}

/// glTF (+Y up, +Z front) to the GLA's model space (+Z up, -Y front): the axes a
/// Blender export writes, read back.
fn to_game_axes() -> Mat3A {
    Mat3A::from_cols(
        Vec3A::new(1.0, 0.0, 0.0),
        Vec3A::new(0.0, 0.0, 1.0),
        Vec3A::new(0.0, -1.0, 0.0),
    )
}

fn affine(rows: &[[f32; 4]; 3]) -> Affine3A {
    Affine3A::from_cols_array(&[
        rows[0][0], rows[1][0], rows[2][0], rows[0][1], rows[1][1], rows[2][1], rows[0][2],
        rows[1][2], rows[2][2], rows[0][3], rows[1][3], rows[2][3],
    ])
}

fn rows(matrix: &Affine3A) -> [[f32; 4]; 3] {
    let m = matrix.matrix3;
    let t = matrix.translation;
    [
        [m.x_axis.x, m.y_axis.x, m.z_axis.x, t.x],
        [m.x_axis.y, m.y_axis.y, m.z_axis.y, t.y],
        [m.x_axis.z, m.y_axis.z, m.z_axis.z, t.z],
    ]
}

/// An orthonormal frame whose X runs along `direction`, Z perpendicular to it and
/// the front (-Y), so two such frames differ only by how the bone is aimed.
fn frame(direction: Vec3A) -> Option<Mat3A> {
    let x = direction.try_normalize()?;
    let z = x.cross(Vec3A::NEG_Y).try_normalize()?;
    Some(Mat3A::from_cols(x, z.cross(x), z))
}

impl RigModel {
    /// Load the glTF named by `manifest` from `directory` and fit it to `driver`,
    /// its feet planted on those of `driver_mesh` (the driver's GLM).
    pub(crate) fn load(
        vfs: &VirtualFileSystem,
        directory: &str,
        manifest: &Manifest,
        driver: &Gla,
        driver_mesh: &Glm,
    ) -> Result<Self, Box<dyn Error>> {
        let read = |path: &str| vfs.read(path).ok().flatten().map(|asset| asset.bytes);
        let file = match &manifest.model {
            Some(file) => file.clone(),
            None if vfs.contains(&format!("{directory}/model.gltf"))? => "model.gltf".to_owned(),
            None => "model.glb".to_owned(),
        };
        let path = format!("{directory}/{file}");
        let base = path
            .rsplit_once('/')
            .map_or("", |(base, _)| base)
            .to_owned();
        let bytes = read(&path).ok_or_else(|| format!("rig model {path:?} was not found"))?;
        let model = parse(&bytes, |uri| read(&join(&base, uri)))?;
        let mut rig =
            Self::fit(model, &base, manifest, driver)?.with_feet(driver, sole(driver_mesh));
        rig.borrow_clips(vfs, directory, driver);
        Ok(rig)
    }

    /// Take the clips of the installed emotes made on other rig models (their file's
    /// `rig`) that this skeleton can play ([`Self::borrowed`]).
    fn borrow_clips(&mut self, vfs: &VirtualFileSystem, directory: &str, driver: &Gla) {
        let catalogue = crate::emotes::lock().catalogue.clone();
        let catalogue = catalogue.unwrap_or_else(|| crate::emotes::load(vfs));
        for emote in catalogue {
            if emote.clip.is_empty() || emote.rig.is_empty() {
                continue;
            }
            let source_directory = format!("models/players/{}", emote.rig);
            if source_directory.eq_ignore_ascii_case(directory)
                || self.clips.iter().any(|clip| clip.name == emote.clip)
            {
                continue;
            }
            let Some(source) = source_rig(vfs, &source_directory, driver) else {
                continue;
            };
            let Some(clip) = source.clips.iter().find(|clip| clip.name == emote.clip) else {
                crate::log::progress(format_args!(
                    "rig model {directory}: emote {}: {source_directory} has no clip {:?}",
                    emote.id, emote.clip
                ));
                continue;
            };
            match self.borrowed(&source, clip) {
                Some(clip) => {
                    crate::log::progress(format_args!(
                        "rig model {directory}: borrowed {} from {source_directory}",
                        clip.name
                    ));
                    self.clips.push(clip);
                }
                None => crate::log::progress(format_args!(
                    "rig model {directory}: emote {}: the skeleton of {source_directory} \
                     does not match",
                    emote.id
                )),
            }
        }
    }

    /// `clip` of rig `source` played on this rig: each joint named as one of the
    /// source's turns from its own bind pose as that one turns from the source's
    /// (model space, so bone rolls may differ), keeping this rig's bone lengths; the
    /// root and the joint under it (Unreal's root and pelvis) also move as the
    /// source's do. Joints the source lacks follow their parent. `None` when fewer
    /// than [`BORROW_SHARE`] of the joints the clip moves are found here.
    pub(crate) fn borrowed(&self, source: &RigModel, clip: &Clip) -> Option<Clip> {
        let names: HashMap<&str, usize> = source
            .joints
            .iter()
            .enumerate()
            .map(|(index, joint)| (joint.name.as_str(), index))
            .collect();
        let found: Vec<Option<usize>> = self
            .joints
            .iter()
            .map(|joint| names.get(joint.name.as_str()).copied())
            .collect();
        let moved = (0..source.joints.len())
            .filter(|&index| clip.animates(index))
            .count();
        let shared = found
            .iter()
            .flatten()
            .filter(|&&index| clip.animates(index))
            .count();
        if moved == 0 || (shared as f32) < moved as f32 * BORROW_SHARE {
            return None;
        }
        let depth = |joints: &[Joint], mut index: usize| {
            let mut depth = 0;
            while let Some(parent) = joints[index].parent {
                depth += 1;
                index = parent;
            }
            depth
        };
        let times = clip.key_times();
        let mut posed = Vec::new();
        let mut frames = Vec::with_capacity(times.len());
        for &time in &times {
            source.clip_pose(clip, time, &mut posed);
            let mut global = vec![Affine3A::IDENTITY; self.joints.len()];
            for &index in &self.order {
                let joint = &self.joints[index];
                let follow = match joint.parent {
                    Some(parent) => global[parent] * joint.local,
                    None => joint.bind,
                };
                global[index] = match found[index] {
                    Some(from) => {
                        let model = &source.joints[from];
                        let matrix3 =
                            posed[from].matrix3 * model.bind.matrix3.inverse() * joint.bind.matrix3;
                        let translation = if depth(&source.joints, from) <= 1 {
                            joint.bind.translation + posed[from].translation
                                - model.bind.translation
                        } else {
                            follow.translation
                        };
                        Affine3A {
                            matrix3,
                            translation,
                        }
                    }
                    None => follow,
                };
            }
            frames.push(
                self.joints
                    .iter()
                    .enumerate()
                    .map(|(index, joint)| match joint.parent {
                        Some(parent) => global[parent].inverse() * global[index],
                        None => joint.above.inverse() * global[index],
                    })
                    .collect(),
            );
        }
        Some(Clip::from_frames(&clip.name, &times, &frames))
    }

    /// Every joint's model-space pose while playing `clip` at `time`, into `out`.
    fn clip_pose(&self, clip: &Clip, time: f32, out: &mut Vec<Affine3A>) {
        out.clear();
        out.resize(self.joints.len(), Affine3A::IDENTITY);
        for &index in &self.order {
            let joint = &self.joints[index];
            let local = clip.local(index, time, &joint.rest);
            out[index] = match joint.parent {
                Some(parent) => out[parent] * local,
                None => joint.above * local,
            };
        }
    }

    /// Plant the feet ([`Feet`]) on a driver whose lowest base-pose point is
    /// `driver_sole`; nothing changes when the driver or the map has no ankles.
    fn with_feet(mut self, driver: &Gla, driver_sole: Option<f32>) -> Self {
        let lowest = |values: &mut dyn Iterator<Item = f32>| values.fold(f32::MAX, f32::min);
        self.feet = driver_sole.and_then(|sole| {
            let mut ankles = Vec::new();
            let mut joints = Vec::new();
            for name in ANKLES {
                let bone = driver.bones.iter().position(|bone| bone.name == name)?;
                let anchor = (*self.anchors.get(bone)?).filter(|a| a.bone == bone)?;
                ankles.push((bone, anchor.base));
                joints.push(anchor.joint);
            }
            let rig_sole = lowest(
                &mut self
                    .surfaces
                    .iter()
                    .flat_map(|surface| &surface.vertices)
                    .map(|vertex| vertex.position[2]),
            );
            if rig_sole == f32::MAX {
                return None;
            }
            Some(Feet {
                driver_ankle: lowest(&mut ankles.iter().map(|(_, base)| base.z)) - sole,
                rig_ankle: lowest(&mut joints.iter().map(|&j| self.joints[j].bind.translation.z))
                    - rig_sole,
                rig_sole,
                ankles,
                joints,
            })
        });
        self
    }

    fn fit(
        model: GltfModel,
        base: &str,
        manifest: &Manifest,
        driver: &Gla,
    ) -> Result<Self, Box<dyn Error>> {
        let map = manifest.bone_map()?;
        let bone_index = |name: &str| driver.bones.iter().position(|b| b.name == name);
        let joint_index = |name: &str| model.joints.iter().position(|j| j.name == name);
        let bone_base = |bone: usize| affine(&driver.bones[bone].base_pose);
        let axes = to_game_axes();
        // Scale: the pelvis joint's bind height onto the driver's pelvis base height.
        let pelvis = map.iter().find(|e| e.bone == "pelvis");
        let scale = match manifest.scale {
            Some(scale) => scale,
            None => {
                let joint = pelvis
                    .and_then(|e| joint_index(&e.joint))
                    .ok_or("scale auto needs a joint mapped to the pelvis")?;
                let bone = bone_index("pelvis").ok_or("the driver skeleton has no pelvis")?;
                let height = model.joints[joint].bind[1][3];
                if height <= 0.0 {
                    return Err("the pelvis joint is not above the model's origin".into());
                }
                bone_base(bone).translation.z / height
            }
        };
        let convert = |m: &Affine3A| Affine3A {
            matrix3: axes * m.matrix3 * axes.transpose(),
            translation: axes * m.translation * scale,
        };
        let mut joints = Vec::with_capacity(model.joints.len());
        for joint in &model.joints {
            let bind = convert(&affine(&joint.bind));
            let rest = convert(&affine(&joint.rest));
            joints.push(Joint {
                name: joint.name.clone(),
                parent: joint.parent,
                bind,
                inverse_bind: bind.inverse(),
                local: bind,
                rest: Rest::of(&rest),
                above: bind * rest.inverse(),
            });
        }
        let clips: Vec<Clip> = model
            .animations
            .iter()
            .map(|animation| Clip::new(animation, joints.len(), Convert::new(axes, scale)))
            .filter(|clip| clip.duration > 0.0)
            .collect();
        for index in 0..joints.len() {
            if let Some(parent) = joints[index].parent {
                joints[index].local = joints[parent].inverse_bind * joints[index].bind;
            }
        }
        let order = topological(&joints)?;
        let dynamic_anchors: Vec<(usize, f32)> = manifest
            .dynamic
            .iter()
            .filter_map(|(name, gravity)| {
                let found = joint_index(name);
                if found.is_none() {
                    crate::log::progress(format_args!(
                        "rig model {base}: {MANIFEST}: no joint {name:?} for dynamic"
                    ));
                }
                Some((found?, gravity.unwrap_or(dynamics::GRAVITY)))
            })
            .collect();
        let chains = dynamics::Chains::under(&joints, &order, &dynamic_anchors);
        // Alignments first (an `As` entry reads another's), then the driven joints.
        let mut aligned: Vec<Option<Mat3A>> = vec![None; joints.len()];
        for entry in &map {
            let (Some(joint), Some(bone)) = (joint_index(&entry.joint), bone_index(&entry.bone))
            else {
                continue;
            };
            aligned[joint] = Some(match &entry.align {
                Align::Rest | Align::As(_) => Mat3A::IDENTITY,
                Align::Toward {
                    joint_end,
                    bone_end,
                } => {
                    let (Some(joint_end), Some(bone_end)) =
                        (joint_index(joint_end), bone_index(bone_end))
                    else {
                        continue;
                    };
                    let jka = bone_base(bone_end).translation - bone_base(bone).translation;
                    let rig = joints[joint_end].bind.translation - joints[joint].bind.translation;
                    match (frame(jka), frame(rig)) {
                        (Some(jka), Some(rig)) => jka * rig.transpose(),
                        _ => Mat3A::IDENTITY,
                    }
                }
            });
        }
        for entry in &map {
            if let Align::As(other) = &entry.align
                && let (Some(joint), Some(other)) = (joint_index(&entry.joint), joint_index(other))
            {
                aligned[joint] = aligned[other].or(Some(Mat3A::IDENTITY));
            }
        }
        let mut driven = vec![None; joints.len()];
        let mut mapped = 0;
        for entry in &map {
            let (Some(joint), Some(bone)) = (joint_index(&entry.joint), bone_index(&entry.bone))
            else {
                continue;
            };
            mapped += 1;
            let align = aligned[joint].unwrap_or(Mat3A::IDENTITY);
            driven[joint] = Some(Driven {
                bone,
                rotation: align * joints[joint].bind.matrix3,
                translate: (entry.bone == "pelvis").then(|| bone_base(bone).translation),
            });
        }
        let anchors = anchors(&map, driver, &joints, joint_index, bone_index);
        if mapped == 0 {
            return Err(format!(
                "no joint of the rig matches the {:?} bone map",
                manifest.preset
            )
            .into());
        }
        let normals = Mat3::from(axes);
        let surfaces = model
            .primitives
            .into_iter()
            .map(|primitive| {
                let vertices = primitive
                    .positions
                    .iter()
                    .enumerate()
                    .map(|(index, position)| {
                        let position = axes * Vec3A::from_array(*position) * scale;
                        let normal = normals
                            * Vec3::from_array(
                                primitive
                                    .normals
                                    .get(index)
                                    .copied()
                                    .unwrap_or([0.0, 1.0, 0.0]),
                            );
                        Vertex {
                            position: position.to_array(),
                            normal: normal.normalize_or_zero().to_array(),
                            texture_coordinates: primitive
                                .texture_coordinates
                                .get(index)
                                .copied()
                                .unwrap_or_default(),
                            joints: primitive.joints.get(index).copied().unwrap_or_default(),
                            weights: primitive
                                .weights
                                .get(index)
                                .copied()
                                .unwrap_or([1.0, 0.0, 0.0, 0.0]),
                        }
                    })
                    .collect::<Vec<_>>();
                if vertices
                    .iter()
                    .flat_map(|v| v.joints)
                    .any(|j| usize::from(j) >= joints.len())
                {
                    return Err("a vertex names a joint the skin does not have".into());
                }
                let shader = primitive
                    .base_color_image
                    .as_deref()
                    .map_or_else(|| UNTEXTURED_SHADER.to_owned(), |uri| join(base, uri));
                // glTF fronts wind counter-clockwise, the game's clockwise (its
                // default `cull front` hides the other side), so each triangle turns.
                let mut indices = primitive.indices;
                for triangle in indices.chunks_exact_mut(3) {
                    triangle.swap(1, 2);
                }
                Ok(Surface {
                    shader,
                    vertices,
                    indices,
                })
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        crate::log::progress(format_args!(
            "rig model {base}: {} joints ({mapped} driven by {:?}), {} surfaces, {} clips{}, scale {scale:.2}",
            joints.len(),
            manifest.preset,
            surfaces.len(),
            clips.len(),
            if clips.is_empty() {
                String::new()
            } else {
                format!(
                    " ({})",
                    clips
                        .iter()
                        .map(|clip| clip.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            },
        ));
        let pelvis = driven
            .iter()
            .flatten()
            .find_map(|driven| driven.translate.map(|base| (driven.bone, base)));
        Ok(Self {
            joints,
            order,
            driven,
            anchors,
            pelvis,
            feet: None,
            lift: manifest.lift,
            chains,
            clips,
            surfaces,
            body: None,
        })
    }

    /// This rig's body ([`body::Body`]) from its surfaces in the bind pose: each vertex
    /// on the JKA bone its strongest joint follows (or its nearest mapped ancestor's).
    fn measure_body(&self, driver: &Gla) -> body::Body {
        let bone_of = |mut joint: usize| {
            for _ in 0..self.joints.len() {
                if let Some(driven) = self.driven.get(joint).copied().flatten() {
                    return Some(driven.bone);
                }
                joint = self.joints.get(joint)?.parent?;
            }
            None
        };
        let samples: Vec<(usize, Vec3A)> = self
            .surfaces
            .iter()
            .flat_map(|surface| &surface.vertices)
            .filter_map(|vertex| {
                let bone = bone_of(usize::from(vertex.joints[0]))?;
                Some((bone, Vec3A::from_array(vertex.position)))
            })
            .collect();
        let joint_of = |bone: usize| {
            (*self.anchors.get(bone)?)
                .filter(|anchor| anchor.bone == bone)
                .map(|anchor| self.joints[anchor.joint].bind.translation)
        };
        body::Body::measure(driver, &samples, |lower, end| {
            Some((joint_of(lower)?, joint_of(end)?))
        })
    }

    /// The palette size: one matrix per joint.
    pub(crate) fn joint_count(&self) -> usize {
        self.joints.len()
    }

    /// The clip named `name` `seconds` after it started, if the rig has it (see
    /// [`ClipPlay::new`]).
    pub(crate) fn play(
        &self,
        name: &str,
        seconds: f32,
        remaining: Option<f32>,
        looping: bool,
    ) -> Option<ClipPlay> {
        let index = self.clips.iter().position(|clip| clip.name == name)?;
        Some(ClipPlay::new(
            index,
            self.clips[index].duration,
            seconds,
            remaining,
            looping,
        ))
    }

    /// Pose every joint from the driver's evaluated `bones` (one model-space matrix
    /// per GLA bone, as the GLM is skinned with), with `play`'s clip over it, into
    /// `scratch.palette`.
    pub(crate) fn pose(
        &self,
        bones: &[[[f32; 4]; 3]],
        play: Option<ClipPlay>,
        scratch: &mut Scratch,
    ) {
        let now = std::time::Instant::now();
        let step = scratch
            .last
            .replace(now)
            .map(|last| now.duration_since(last).as_secs_f32());
        self.pose_after(bones, play, scratch, step);
    }

    /// [`Self::pose`], `step` seconds after the actor's last pose (`None`: the first)
    /// for the dynamic joints.
    pub(crate) fn pose_after(
        &self,
        bones: &[[[f32; 4]; 3]],
        play: Option<ClipPlay>,
        scratch: &mut Scratch,
        step: Option<f32>,
    ) {
        let Scratch {
            pose,
            retarget,
            clip,
            palette,
            blended,
            dynamics: chain_state,
            last: _,
        } = scratch;
        pose.clear();
        pose.resize(self.joints.len(), Affine3A::IDENTITY);
        for &index in &self.order {
            let parent = self.joints[index].parent.map(|parent| pose[parent]);
            pose[index] = self.place(index, parent, bones);
        }
        // Where the driver's soles are now, from its lower ankle.
        let ground = self.feet.as_ref().and_then(|feet| {
            let ankle = feet
                .ankles
                .iter()
                .filter_map(|(bone, base)| {
                    bones
                        .get(*bone)
                        .map(|matrix| affine(matrix).transform_point3a(*base).z)
                })
                .fold(f32::MAX, f32::min);
            (ankle < f32::MAX).then_some(ankle - feet.driver_ankle)
        });
        if let (Some(feet), Some(ground)) = (&self.feet, ground) {
            let ankle = feet
                .joints
                .iter()
                .map(|&joint| pose[joint].translation.z)
                .fold(f32::MAX, f32::min);
            let shift = ground + feet.rig_ankle - ankle;
            for joint in pose.iter_mut() {
                joint.translation.z += shift;
            }
        }
        *blended = false;
        if let Some(play) = play.filter(|play| play.weight > 0.0)
            && let Some(source) = self.clips.get(play.clip)
        {
            // A clip's pose is the rig's own, standing where its bind pose stands;
            // the driver's pelvis says how far its animations sink the body.
            let mut offset = self.pelvis.and_then(|(bone, base)| {
                bones
                    .get(bone)
                    .map(|matrix| affine(matrix).transform_point3a(base) - base)
            });
            // With planted feet, the clip's soles stand on the driver's ground.
            if let (Some(feet), Some(ground)) = (&self.feet, ground) {
                let offset = offset.get_or_insert(Vec3A::ZERO);
                offset.z = ground - feet.rig_sole;
            }
            self.clip_pose(source, play.time, clip);
            retarget.clone_from(pose);
            for ((pose, retarget), clip) in pose.iter_mut().zip(retarget.iter()).zip(clip.iter()) {
                let mut clip = *clip;
                clip.translation += offset.unwrap_or(Vec3A::ZERO);
                *pose = if play.weight >= 1.0 {
                    clip
                } else {
                    clips::blend(retarget, &clip, play.weight)
                };
            }
            *blended = true;
        }
        if !self.chains.is_empty() {
            dynamics::simulate(
                &self.joints,
                &self.chains,
                pose,
                chain_state,
                step,
                dynamics::WAG_UNITS,
            );
        }
        if self.lift != 0.0 {
            for joint in pose.iter_mut() {
                joint.translation.z += self.lift;
            }
        }
        palette.clear();
        palette.extend(
            pose.iter()
                .zip(&self.joints)
                .map(|(pose, joint)| rows(&(*pose * joint.inverse_bind))),
        );
    }

    /// Joint `index` posed under its parent's `parent` pose.
    fn place(&self, index: usize, parent: Option<Affine3A>, bones: &[[[f32; 4]; 3]]) -> Affine3A {
        let joint = &self.joints[index];
        let mut pose = match parent {
            Some(parent) => parent * joint.local,
            None => joint.bind,
        };
        if let Some(driven) = self.driven[index]
            && let Some(bone) = bones.get(driven.bone)
        {
            let bone = affine(bone);
            pose.matrix3 = bone.matrix3 * driven.rotation;
            if let Some(base) = driven.translate {
                pose.translation = joint.bind.translation + bone.transform_point3a(base) - base;
            }
        }
        pose
    }

    /// Joint `index` alone posed from `bones`, walking up its parents.
    fn joint_pose(&self, index: usize, bones: &[[[f32; 4]; 3]]) -> Affine3A {
        let parent = self.joints[index]
            .parent
            .map(|parent| self.joint_pose(parent, bones));
        self.place(index, parent, bones)
    }

    /// Move `bolt`, a driver bolt hanging on `bone` and evaluated from `bones`, onto
    /// the rig. It keeps the driver's orientation (mapped joints turn as the driver's
    /// bones do) and its offset from the bone, stretched by the limb's reach, but
    /// starts from the rig's joint: a saber sits in the rig's hand, a hat on its head.
    ///
    /// `posed` is this frame's pose of the actor when it was made from the same
    /// `bones` (it saves walking the joints, and carries a clip playing on the rig:
    /// the bolt then turns and moves with the joint, a saber staying in a dancing
    /// hand); `None` poses the joint afresh from the driver alone.
    pub(crate) fn retarget_bolt(
        &self,
        bone: usize,
        bones: &[[[f32; 4]; 3]],
        posed: Option<&Scratch>,
        mut bolt: [[f32; 4]; 3],
    ) -> [[f32; 4]; 3] {
        let origin = Vec3A::new(bolt[0][3], bolt[1][3], bolt[2][3]);
        let Some((moved, clip)) = self.retargeted(bone, bones, posed, origin) else {
            return bolt;
        };
        let rotation = match clip {
            Some(clip) => {
                let axes = affine(&bolt).matrix3;
                clip.matrix3 * axes
            }
            None => affine(&bolt).matrix3,
        };
        let turned = rows(&Affine3A {
            matrix3: rotation,
            translation: moved,
        });
        bolt.copy_from_slice(&turned);
        bolt
    }

    /// [`Self::retarget_bolt`] for a point: a driver bone's own origin lands on its
    /// mapped joint.
    pub(crate) fn retarget_point(
        &self,
        bone: usize,
        bones: &[[[f32; 4]; 3]],
        posed: Option<&Scratch>,
        point: Vec3A,
    ) -> Vec3A {
        self.retargeted(bone, bones, posed, point)
            .map_or(point, |(moved, _)| moved)
    }

    /// `point` on driver bone `bone` moved onto the rig, and the transform a playing
    /// clip adds to the joint it hangs on.
    fn retargeted(
        &self,
        bone: usize,
        bones: &[[[f32; 4]; 3]],
        posed: Option<&Scratch>,
        point: Vec3A,
    ) -> Option<(Vec3A, Option<Affine3A>)> {
        let anchor = (*self.anchors.get(bone)?)?;
        let matrix = bones.get(anchor.bone)?;
        let driver_joint = affine(matrix).transform_point3a(anchor.base);
        let posed = posed.filter(|posed| posed.pose.len() == self.joints.len());
        let (rig_joint, clip) = match posed {
            Some(posed) if posed.blended => {
                let retarget = posed.retarget[anchor.joint];
                let clip = posed.pose[anchor.joint] * retarget.inverse();
                (retarget.translation, Some(clip))
            }
            Some(posed) => (posed.pose[anchor.joint].translation, None),
            None => (self.joint_pose(anchor.joint, bones).translation, None),
        };
        let moved = rig_joint + (point - driver_joint) * anchor.reach;
        Some((
            clip.map_or(moved, |clip| clip.transform_point3a(moved)),
            clip,
        ))
    }
}

/// [`Anchor`]s for every driver bone: the first joint mapped to each bone, inherited
/// by unmapped bones from their nearest mapped ancestor.
fn anchors(
    map: &[MapEntry],
    driver: &Gla,
    joints: &[Joint],
    joint_index: impl Fn(&str) -> Option<usize>,
    bone_index: impl Fn(&str) -> Option<usize>,
) -> Vec<Option<Anchor>> {
    let base = |bone: usize| affine(&driver.bones[bone].base_pose).translation;
    let mut own: Vec<Option<Anchor>> = vec![None; driver.bones.len()];
    for entry in map {
        let (Some(joint), Some(bone)) = (joint_index(&entry.joint), bone_index(&entry.bone)) else {
            continue;
        };
        if own[bone].is_some() {
            continue;
        }
        let reach = match &entry.align {
            Align::Toward {
                joint_end,
                bone_end,
            } => joint_index(joint_end).zip(bone_index(bone_end)).and_then(
                |(joint_end, bone_end)| {
                    let rig = (joints[joint_end].bind.translation - joints[joint].bind.translation)
                        .length();
                    let jka = (base(bone_end) - base(bone)).length();
                    (rig > f32::EPSILON && jka > f32::EPSILON).then(|| rig / jka)
                },
            ),
            Align::Rest | Align::As(_) => None,
        };
        own[bone] = Some(Anchor {
            joint,
            bone,
            base: base(bone),
            reach: reach.unwrap_or(1.0),
        });
    }
    (0..driver.bones.len())
        .map(|mut bone| {
            loop {
                if let Some(anchor) = own[bone] {
                    break Some(anchor);
                }
                bone = driver.bones[bone].parent?;
            }
        })
        .collect()
}

/// One actor's rig pose storage, kept between frames so posing allocates nothing.
#[derive(Clone, Debug, Default)]
pub(crate) struct Scratch {
    /// This frame's model-space joints.
    pose: Vec<Affine3A>,
    /// While a clip blends in: the joints retargeted from the driver alone, and the
    /// clip's own.
    retarget: Vec<Affine3A>,
    clip: Vec<Affine3A>,
    /// Whether `pose` holds a clip (`retarget` is then valid).
    blended: bool,
    /// This frame's skinning matrices, one per joint.
    pub(crate) palette: Vec<[[f32; 4]; 3]>,
    /// The actor's dynamic joints, and when it was last posed.
    dynamics: dynamics::State,
    last: Option<std::time::Instant>,
}

/// `vertex` skinned with `palette` (the CPU path): position and unit normal.
pub(crate) fn skin(vertex: &Vertex, palette: &[[[f32; 4]; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut position = [0.0f32; 3];
    let mut normal = [0.0f32; 3];
    for slot in 0..vertex.influences() {
        let Some(m) = palette.get(usize::from(vertex.joints[slot])) else {
            continue;
        };
        let weight = vertex.weights[slot];
        let (p, n) = (vertex.position, vertex.normal);
        for row in 0..3 {
            position[row] +=
                weight * (m[row][0] * p[0] + m[row][1] * p[1] + m[row][2] * p[2] + m[row][3]);
            normal[row] += weight * (m[row][0] * n[0] + m[row][1] * n[1] + m[row][2] * n[2]);
        }
    }
    let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if length > f32::EPSILON {
        normal = normal.map(|value| value / length);
    }
    (position, normal)
}

/// `uri` relative to the folder `base`, with `./` and backslashes normalised.
fn join(base: &str, uri: &str) -> String {
    let uri = uri.replace('\\', "/");
    let uri = uri.trim_start_matches("./");
    if base.is_empty() {
        uri.to_owned()
    } else {
        format!("{base}/{uri}")
    }
}

/// Joint indices with every parent before its children.
fn topological(joints: &[Joint]) -> Result<Vec<usize>, Box<dyn Error>> {
    let mut order = Vec::with_capacity(joints.len());
    let mut state = vec![0u8; joints.len()];
    fn visit(
        index: usize,
        joints: &[Joint],
        state: &mut [u8],
        order: &mut Vec<usize>,
    ) -> Result<(), Box<dyn Error>> {
        match state[index] {
            2 => return Ok(()),
            1 => return Err("the rig's joint hierarchy has a cycle".into()),
            _ => {}
        }
        state[index] = 1;
        if let Some(parent) = joints[index].parent {
            visit(parent, joints, state, order)?;
        }
        state[index] = 2;
        order.push(index);
        Ok(())
    }
    for index in 0..joints.len() {
        visit(index, joints, &mut state, &mut order)?;
    }
    Ok(order)
}

#[path = "rig_body.rs"]
mod body;
#[path = "rig_clips.rs"]
mod clips;
#[path = "rig_dynamics.rs"]
mod dynamics;
#[path = "rig_stock.rs"]
mod stock;

#[cfg(test)]
#[path = "rig_model_tests.rs"]
mod tests;
