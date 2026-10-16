//! Loading of deformable player appearances from the mounted VFS.

use super::*;
use std::collections::HashMap;

/// Parsed `.gla` skeletons keyed by animation path. Every humanoid player
/// model shares `_humanoid.gla` (tens of MB), so the menu's stage keeps
/// one parsed copy across model switches instead of re-reading it.
#[derive(Clone, Default)]
pub(crate) struct GlaCache {
    animations: HashMap<String, Arc<Gla>>,
    audio: HashMap<(String, String), AnimationAudio>,
}

type AnimationAudio = (
    Arc<sjk_client::animation_events::Events>,
    Arc<crate::audio::SoundPrefetch>,
);

impl GlaCache {
    /// Keep what `other` parsed (a copy handed to a loading worker), entries this
    /// cache lacks only.
    pub(crate) fn absorb(&mut self, other: Self) {
        for (path, animation) in other.animations {
            self.animations.entry(path).or_insert(animation);
        }
        for (key, audio) in other.audio {
            self.audio.entry(key).or_insert(audio);
        }
    }

    fn get(
        &mut self,
        path: &str,
        bytes: impl FnOnce() -> Result<Vec<u8>, Box<dyn Error>>,
    ) -> Result<Arc<Gla>, Box<dyn Error>> {
        if let Some(animation) = self.animations.get(path) {
            return Ok(animation.clone());
        }
        let animation = Arc::new(Gla::parse(&bytes()?)?);
        self.animations.insert(path.to_owned(), animation.clone());
        Ok(animation)
    }
}

/// Split a legacy `model` cvar (`kyle/default`, `jedi_hm/head_a1|torso_a1|lower_a1`)
/// into the model directory under `models/players` and the skin variant.
pub(crate) fn split_model_cvar(value: &str) -> (String, &str) {
    let (directory, variant) = value.split_once('/').unwrap_or((value, "default"));
    let variant = if variant.is_empty() {
        "default"
    } else {
        variant
    };
    (format!("models/players/{directory}"), variant)
}

#[derive(Clone)]
pub(super) struct PlayerPreview {
    pub(super) mesh: Glm,
    pub(super) animation: Arc<Gla>,
    pub(super) skin: Skin,
    pub(super) config: Arc<AnimationConfig>,
    pub(super) sequence: AnimationSequence,
    pub(crate) events: Arc<sjk_client::animation_events::Events>,
    pub(crate) event_sounds: Option<Arc<crate::audio::SoundPrefetch>>,
    pub(super) origin: [f32; 3],
    pub(super) yaw: f32,
    /// Set when the appearance named a vehicle (`$<vehicle>`).
    pub(super) vehicle: Option<crate::vehicle_assets::VehicleKind>,
    pub(crate) vehicle_camera: Option<crate::camera::VehicleProfile>,
    /// The vehicle's HUD maxima and crosshair, read with its definition.
    pub(crate) vehicle_hud: Option<crate::hud::vehicle::Profile>,
    /// A rig model ([`crate::rig_model`]) drawn in place of `mesh`, which then
    /// only drives the pose, bolts and traces.
    pub(crate) rig: Option<Arc<crate::rig_model::RigModel>>,
    /// The rig's pose and palette from the latest frame, reused every frame; bolts
    /// read it so they follow a clip the rig plays.
    pub(crate) rig_pose: crate::rig_model::Scratch,
    /// The emote clips of rig models this stock model plays through the bone map in
    /// reverse ([`crate::rig_model::StockEmotes`]); `None` for a rig model or when
    /// no installed emote fits.
    pub(crate) stock_emotes: Option<Arc<crate::rig_model::StockEmotes>>,
    /// The stock emote's bone matrices from the latest frame, reused every frame.
    pub(crate) stock_pose: crate::rig_model::StockScratch,
}

impl PlayerPreview {
    /// The bolt surface `name` posed with `matrices`, moved onto the rig model's
    /// joints when one is drawn (the saber in its hand, a hat on its head).
    pub(crate) fn bolt(&self, name: &str, matrices: &[[[f32; 4]; 3]]) -> Option<[[f32; 4]; 3]> {
        let bolt = self.mesh.surface_bolt_matrix(name, 0, matrices).ok()??;
        Some(match (&self.rig, self.mesh.surface_bolt_bone(name, 0)) {
            (Some(rig), Some(bone)) => {
                rig.retarget_bolt(bone, matrices, Some(&self.rig_pose), bolt)
            }
            _ => bolt,
        })
    }
}

pub(super) fn load_player_preview(
    vfs: &VirtualFileSystem,
    directory: &str,
    camera_origin: [f32; 3],
    camera_yaw: f32,
) -> Result<PlayerPreview, Box<dyn Error>> {
    load_player_appearance(vfs, directory, "default", camera_origin, camera_yaw)
}

pub(super) fn load_player_appearance(
    vfs: &VirtualFileSystem,
    directory: &str,
    variant: &str,
    camera_origin: [f32; 3],
    camera_yaw: f32,
) -> Result<PlayerPreview, Box<dyn Error>> {
    let mut cache = GlaCache::default();
    load_player_appearance_with(
        vfs,
        directory,
        variant,
        camera_origin,
        camera_yaw,
        &mut cache,
    )
}

/// [`load_player_appearance`] sharing parsed skeletons through `cache`.
pub(super) fn load_player_appearance_with(
    vfs: &VirtualFileSystem,
    directory: &str,
    variant: &str,
    camera_origin: [f32; 3],
    camera_yaw: f32,
    cache: &mut GlaCache,
) -> Result<PlayerPreview, Box<dyn Error>> {
    // A vehicle names itself, not a model (`$<vehicle>`): the vehicle table knows what it wears.
    let vehicle = match sjk_client::legacy_vehicle_name(directory) {
        Some(name) => Some(
            crate::vehicle_assets::look(vfs, name)
                .ok_or_else(|| format!("no mounted .veh file defines vehicle {name:?}"))?,
        ),
        None => None,
    };
    let vehicle_kind = vehicle.as_ref().map(|look| look.kind);
    let (directory, variant) = vehicle.as_ref().map_or((directory, variant), |look| {
        (look.directory.as_str(), look.variant.as_str())
    });
    let directory = directory.trim_end_matches('/');
    let read = |path: &str| -> Result<Vec<u8>, Box<dyn Error>> {
        Ok(vfs
            .read(path)?
            .ok_or_else(|| format!("player preview asset {path:?} was not found"))?
            .bytes)
    };
    // A rig model is driven by a stock model's mesh and skeleton, drawn as its glTF.
    let rig_manifest = if vehicle.is_none() {
        crate::rig_model::manifest(vfs, directory)?
    } else {
        None
    };
    let (mesh_directory, mesh_variant) = rig_manifest
        .as_ref()
        .map_or((directory, variant), |manifest| {
            (manifest.driver.as_str(), "default")
        });
    let mut mesh = Glm::parse(&read(&format!("{mesh_directory}/model.glm"))?)?;
    let animation_path = format!("{}.gla", mesh.animation_name);
    let animation = cache.get(&animation_path, || read(&animation_path))?;
    fit_skeleton(&mut mesh, &animation, mesh_directory, &animation_path)?;
    // A skin never costs the model: a skin that gives no handle falls back to
    // model_default.skin, then to the surfaces' own shaders (skin handle 0).
    let skin = crate::player_skin::resolve(vfs, mesh_directory, mesh_variant)?;
    let rig = rig_manifest
        .map(|manifest| {
            crate::rig_model::RigModel::load(vfs, directory, &manifest, &animation, &mesh)
        })
        .transpose()?
        .map(Arc::new);
    // A stock humanoid plays the rig models' emote clips on its own bones.
    let stock_emotes = if rig.is_none() && vehicle.is_none() {
        crate::rig_model::StockEmotes::load(vfs, &animation, &mesh).map(Arc::new)
    } else {
        None
    };
    let animation_directory = mesh
        .animation_name
        .rsplit_once('/')
        .map_or("", |(directory, _)| directory);
    let config = AnimationConfig::parse(&read(&format!("{animation_directory}/animation.cfg"))?)?;
    // A world loader shares the immutable sound table and encoded assets across
    // appearances using the same event file/skeleton, just like the GLA itself.
    let event_directory = if vfs.contains(&format!("{directory}/animevents.cfg"))? {
        directory
    } else {
        animation_directory
    };
    let audio_key = (event_directory.to_owned(), animation_directory.to_owned());
    let (events, event_sounds) = cache
        .audio
        .entry(audio_key)
        .or_insert_with(|| {
            let events = Arc::new(sjk_client::animation_events::Events::load(
                vfs,
                directory,
                animation_directory,
                &config,
            ));
            let sounds = Arc::new(crate::audio::SoundPrefetch::paths(vfs, events.paths()));
            (events, sounds)
        })
        .clone();
    let sequence = config
        .get("BOTH_STAND1IDLE1")
        .or_else(|| config.get("BOTH_STAND1"))
        // Vehicles and other machines have no standing animation: a swoop's whole table is
        // an attack, two cinematics and its root pose. Any frame of theirs is a rest pose.
        .or_else(|| config.get("ROOT"))
        .or_else(|| config.get_by_index(0))
        // A table naming no animation (a vehicle pack's nameless lines) holds frame 0.
        .unwrap_or(&AnimationSequence::REST)
        .clone();
    let forward = [camera_yaw.cos(), camera_yaw.sin()];
    let origin = [
        camera_origin[0] + forward[0] * 192.0,
        camera_origin[1] + forward[1] * 192.0,
        camera_origin[2] - 32.0,
    ];
    crate::log::progress(format_args!(
        concat!(
            "loaded player appearance {}/{}: {} mesh surfaces, ",
            "animation {} frames {}..{}"
        ),
        directory,
        variant,
        mesh.hierarchy.len(),
        sequence.name,
        sequence.first_frame,
        sequence.first_frame + sequence.frame_count
    ));
    Ok(PlayerPreview {
        mesh,
        animation,
        skin,
        config: Arc::new(config),
        events,
        event_sounds: Some(event_sounds),
        sequence,
        origin,
        yaw: camera_yaw + std::f32::consts::PI,
        vehicle: vehicle_kind,
        vehicle_camera: vehicle.as_ref().map(|look| look.camera),
        vehicle_hud: vehicle.as_ref().map(|look| look.hud.clone()),
        rig,
        rig_pose: Default::default(),
        stock_emotes,
        stock_pose: Default::default(),
    })
}

/// rd-vanilla never compares a mesh's bone count with its skeleton's
/// (`R_LoadMDXM`, `tr_ghoul2.cpp`): skinning looks each surface's bone references
/// up in the skeleton's bone cache. A mesh whose references all name bones of
/// its skeleton therefore loads and animates whatever count its header gives,
/// and is skinned with the skeleton's count here. A reference past the skeleton
/// reads outside rd-vanilla's bone cache; such a mesh is refused, and the
/// caller's fallback keeps one bad custom model from failing the map load.
fn fit_skeleton(
    mesh: &mut Glm,
    animation: &Gla,
    directory: &str,
    animation_path: &str,
) -> Result<(), Box<dyn Error>> {
    let skeleton_bones = animation.bones.len();
    if mesh.bone_count == skeleton_bones {
        return Ok(());
    }
    let highest = mesh
        .lods
        .iter()
        .flat_map(|lod| &lod.surfaces)
        .flat_map(|surface| surface.bone_references.iter().copied())
        .max();
    if highest.is_none_or(|bone| bone < skeleton_bones) {
        mesh.bone_count = skeleton_bones;
        return Ok(());
    }
    Err(format!(
        "{directory}/model.glm uses bone {} but its skeleton {animation_path} has {skeleton_bones}",
        highest.unwrap_or_default()
    )
    .into())
}

#[cfg(test)]
mod skeleton_tests {
    use super::fit_skeleton;
    use sjk_model::{Gla, GlaBone, Glm, GlmLod, GlmSurface};

    fn bones(count: usize) -> Vec<GlaBone> {
        (0..count)
            .map(|index| GlaBone {
                name: format!("bone{index}"),
                flags: 0,
                parent: index.checked_sub(1),
                base_pose: [[0.0; 4]; 3],
                inverse_base_pose: [[0.0; 4]; 3],
                children: Vec::new(),
            })
            .collect()
    }

    fn pair(mesh_bones: usize, references: Vec<usize>, skeleton_bones: usize) -> (Glm, Gla) {
        let mesh = Glm {
            name: "model".into(),
            animation_name: "models/players/_humanoid/_humanoid".into(),
            bone_count: mesh_bones,
            hierarchy: Vec::new(),
            lods: vec![GlmLod {
                surfaces: vec![GlmSurface {
                    hierarchy_index: 0,
                    vertices: Vec::new(),
                    triangles: Vec::new(),
                    bone_references: references,
                }],
            }],
        };
        let skeleton = Gla {
            name: "_humanoid".into(),
            scale: 1.0,
            bones: bones(skeleton_bones),
            frames: Vec::new(),
            compressed_bones: Vec::new(),
        };
        (mesh, skeleton)
    }

    #[test]
    fn a_mesh_matching_its_skeleton_loads() {
        let (mut mesh, skeleton) = pair(53, vec![0, 52], 53);
        assert!(fit_skeleton(&mut mesh, &skeleton, "models/players/kyle", "a.gla").is_ok());
        assert_eq!(mesh.bone_count, 53);
    }

    #[test]
    fn a_different_count_loads_when_every_reference_names_a_skeleton_bone() {
        for header_bones in [40, 60] {
            let (mut mesh, skeleton) = pair(header_bones, vec![0, 12, 39], 53);
            assert!(fit_skeleton(&mut mesh, &skeleton, "models/players/custom", "a.gla").is_ok());
            assert_eq!(mesh.bone_count, 53);
        }
    }

    #[test]
    fn a_reference_past_the_skeleton_is_a_load_error() {
        let (mut mesh, skeleton) = pair(72, vec![0, 60], 53);
        let error = fit_skeleton(&mut mesh, &skeleton, "models/players/custom", "a.gla")
            .unwrap_err()
            .to_string();
        assert!(error.contains("uses bone 60"), "{error}");
        assert!(error.contains("has 53"), "{error}");
    }
}

/// A Ghoul2 model another engine draws (`crate::host`): `glm` is any model path (a
/// player, an NPC, a saber hilt, a vehicle) and `skin` the surface-to-shader lines of
/// the skin that engine registered for it, or `None` for the surfaces' own shaders.
/// The engine poses it every frame, so it carries no animation table of its own.
pub(crate) fn load_host_appearance(
    vfs: &VirtualFileSystem,
    glm: &str,
    skin: Option<&str>,
    cache: &mut GlaCache,
) -> Result<PlayerPreview, Box<dyn Error>> {
    let read = |path: &str| -> Result<Vec<u8>, Box<dyn Error>> {
        Ok(vfs
            .read(path)?
            .ok_or_else(|| format!("model {path:?} was not found"))?
            .bytes)
    };
    let mut mesh = Glm::parse(&read(glm)?)?;
    let animation_path = format!("{}.gla", mesh.animation_name);
    let animation = cache.get(&animation_path, || read(&animation_path))?;
    let directory = glm.rsplit_once('/').map_or("", |(directory, _)| directory);
    fit_skeleton(&mut mesh, &animation, directory, &animation_path)?;
    let skin = skin.map_or_else(Skin::default, |lines| Skin::parse(lines.as_bytes()));
    Ok(PlayerPreview {
        mesh,
        animation,
        skin,
        config: Arc::new(AnimationConfig::default()),
        events: Arc::new(sjk_client::animation_events::Events::default()),
        event_sounds: None,
        sequence: AnimationSequence::REST.clone(),
        origin: [0.0; 3],
        yaw: 0.0,
        vehicle: None,
        vehicle_camera: None,
        vehicle_hud: None,
        rig: None,
        rig_pose: Default::default(),
        stock_emotes: None,
        stock_pose: Default::default(),
    })
}
