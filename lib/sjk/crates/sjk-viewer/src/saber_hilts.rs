//! Load-time saber hilt meshes and their numbered blade sockets.

use crate::{Appearance, FlattenedScene, StaticModelMesh, append_static_glm_mesh};
use sjk_model::Glm;
use sjk_vfs::VirtualFileSystem;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;

use crate::saber::{BladeSocket, blade_socket_from_bolt};

const DEFAULT_HILT_MODEL: &str = "models/weapons2/saber_1/saber_1.glm";
const DEFAULT_BLADE_LENGTH: f32 = 40.0;
const DEFAULT_BLADE_RADIUS: f32 = 3.0;

/// One numbered blade socket and its `.sab` presentation parameters.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HiltBlade {
    pub(crate) socket: BladeSocket,
    pub(crate) length: f32,
    pub(crate) radius: f32,
    pub(crate) trail_style: u8,
}

/// Loaded hilt mesh and all valid `*blade1` through `*blade8` sockets.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Hilt {
    pub(crate) mesh_index: usize,
    blades: [Option<HiltBlade>; 8],
    pub(crate) num_blades: u8,
    /// Suppress scene lights for this hilt without suppressing its visible blade.
    pub(crate) no_dlight: bool,
    /// Authored contact-effect suppression.
    pub(crate) no_wall_marks: bool,
}

impl Hilt {
    pub(crate) fn blade(self, index: usize) -> Option<HiltBlade> {
        (index < usize::from(self.num_blades))
            .then(|| self.blades[index])
            .flatten()
    }
}

/// Hilts required by current clientinfo, with `single_1` as a safe fallback.
pub(crate) struct HiltCatalog {
    hilts: BTreeMap<String, Hilt>,
    fallback: Hilt,
    definitions: BTreeMap<String, crate::saber_defs::Definition>,
    /// Hilt models already in the object buffers, by model path.
    model_cache: BTreeMap<String, CachedModel>,
    /// Names whose load failed; not retried.
    failed: BTreeSet<String>,
}

impl HiltCatalog {
    /// Animation-event overrides from the same catalog as the visible hilt.
    pub(crate) fn animation_sound(&self, name: &str, path: &str, variant: usize) -> Option<&str> {
        let definition = self.definitions.get(name)?;
        if path.starts_with("sound/weapons/saber/saberspin") {
            definition.sound_spin.as_deref()
        } else if path.starts_with("sound/weapons/saber/saberhup")
            && definition.sound_swing[0].is_some()
        {
            definition.sound_swing[variant % 3].as_deref()
        } else {
            None
        }
    }

    pub(crate) fn get(&self, name: &str) -> Hilt {
        self.hilts.get(name).copied().unwrap_or(self.fallback)
    }

    /// Whether `name` is loaded, unknown to the `.sab` files, or failed —
    /// in every case [`Self::get`] has an answer without loading anything.
    pub(crate) fn is_resolved(&self, name: &str) -> bool {
        name.eq_ignore_ascii_case("none")
            || self.hilts.contains_key(name)
            || self.failed.contains(name)
            || !self.definitions.contains_key(name)
    }

    /// Load `name` (a `.sab` saber a player picked up mid-match) into `scene`
    /// and `object_meshes`. Returns whether a mesh was appended; the caller
    /// then uploads `scene` and registers the new object mesh.
    pub(crate) fn load(
        &mut self,
        vfs: &VirtualFileSystem,
        name: &str,
        scene: &mut FlattenedScene,
        object_meshes: &mut Vec<StaticModelMesh>,
    ) -> bool {
        if self.is_resolved(name) {
            return false;
        }
        let Some(definition) = self.definitions.get(name).cloned() else {
            return false;
        };
        let before = object_meshes.len();
        match append_hilt(
            vfs,
            &definition,
            scene,
            object_meshes,
            &mut self.model_cache,
        ) {
            Ok(hilt) => {
                self.hilts.insert(name.to_owned(), hilt);
            }
            Err(error) => {
                eprintln!("could not load saber definition {name}: {error}");
                self.failed.insert(name.to_owned());
            }
        }
        object_meshes.len() > before
    }
}

/// Append all client-advertised saber models to the shared object buffers.
pub(crate) fn load_hilts<'a>(
    vfs: &VirtualFileSystem,
    names: impl Iterator<Item = &'a str>,
    scene: &mut FlattenedScene,
    object_meshes: &mut Vec<StaticModelMesh>,
) -> Result<HiltCatalog, Box<dyn Error>> {
    let definitions = crate::saber_defs::load(vfs)?;
    let default = definitions
        .get("single_1")
        .cloned()
        .unwrap_or_else(default_definition);
    let mut required = names
        .filter(|name| !name.eq_ignore_ascii_case("none"))
        .map(str::to_ascii_lowercase)
        .collect::<BTreeSet<_>>();
    required.insert("single_1".to_owned());
    let mut model_cache = BTreeMap::new();
    let fallback = append_hilt(vfs, &default, scene, object_meshes, &mut model_cache)?;
    let mut hilts = BTreeMap::new();
    hilts.insert("single_1".to_owned(), fallback);
    for name in required {
        let Some(definition) = definitions.get(&name) else {
            continue;
        };
        match append_hilt(vfs, definition, scene, object_meshes, &mut model_cache) {
            Ok(hilt) => {
                hilts.insert(name, hilt);
            }
            Err(error) => eprintln!("could not load saber definition {name}: {error}"),
        }
    }
    Ok(HiltCatalog {
        hilts,
        fallback,
        definitions,
        model_cache,
        failed: BTreeSet::new(),
    })
}

fn default_definition() -> crate::saber_defs::Definition {
    crate::saber_defs::Definition {
        name: "single_1".to_owned(),
        sound_spin: None,
        sound_swing: [None, None, None],
        model: DEFAULT_HILT_MODEL.to_owned(),
        num_blades: 1,
        blade_lengths: [DEFAULT_BLADE_LENGTH; 8],
        blade_radii: [DEFAULT_BLADE_RADIUS; 8],
        blade_style2_start: 0,
        trail_style: 0,
        trail_style2: 0,
        no_dlight: false,
        no_wall_marks: false,
    }
}

type CachedModel = (usize, [Option<BladeSocket>; 8]);

fn append_hilt(
    vfs: &VirtualFileSystem,
    definition: &crate::saber_defs::Definition,
    scene: &mut FlattenedScene,
    object_meshes: &mut Vec<StaticModelMesh>,
    model_cache: &mut BTreeMap<String, CachedModel>,
) -> Result<Hilt, Box<dyn Error>> {
    let (mesh_index, sockets) = if let Some(cached) = model_cache.get(&definition.model) {
        *cached
    } else {
        load_hilt_model(vfs, definition, scene, object_meshes, model_cache)?
    };
    Ok(Hilt {
        mesh_index,
        blades: hilt_blades(definition, &sockets),
        num_blades: definition.num_blades,
        no_dlight: definition.no_dlight,
        no_wall_marks: definition.no_wall_marks,
    })
}

/// Pair every socket the model has with the `.sab` blade parameters.
pub(crate) fn hilt_blades(
    definition: &crate::saber_defs::Definition,
    sockets: &[Option<BladeSocket>; 8],
) -> [Option<HiltBlade>; 8] {
    std::array::from_fn(|index| {
        let socket = sockets[index]?;
        let secondary = definition.blade_style2_start > 0
            && index >= usize::from(definition.blade_style2_start);
        Some(HiltBlade {
            socket,
            length: definition.blade_lengths[index],
            radius: definition.blade_radii[index],
            trail_style: if secondary {
                definition.trail_style2
            } else {
                definition.trail_style
            },
        })
    })
}

/// Read the `*blade1`..`*blade8` sockets of a hilt model in its bind pose.
/// `CG_InitG2SaberData` adds the numbered tags in order and only blade 1
/// falls back to `*flash` (`codemp/cgame/cg_players.c:1212-1237`).
pub(crate) fn hilt_sockets(model: &Glm) -> Result<[Option<BladeSocket>; 8], Box<dyn Error>> {
    let identity = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
    ];
    let pose = vec![identity; model.bone_count];
    let sockets = std::array::from_fn(|index| {
        let name = format!("*blade{}", index + 1);
        model
            .surface_bolt_matrix(&name, 0, &pose)
            .ok()
            .flatten()
            .or_else(|| {
                (index == 0)
                    .then(|| model.surface_bolt_matrix("*flash", 0, &pose).ok().flatten())
                    .flatten()
            })
            .and_then(blade_socket_from_bolt)
    });
    if sockets[0].is_none() {
        return Err("saber hilt has neither *blade1 nor *flash".into());
    }
    Ok(sockets)
}

fn load_hilt_model(
    vfs: &VirtualFileSystem,
    definition: &crate::saber_defs::Definition,
    scene: &mut FlattenedScene,
    object_meshes: &mut Vec<StaticModelMesh>,
    model_cache: &mut BTreeMap<String, CachedModel>,
) -> Result<CachedModel, Box<dyn Error>> {
    let asset = vfs
        .read(&definition.model)?
        .ok_or_else(|| format!("hilt model {} is missing", definition.model))?;
    let model = Glm::parse(&asset.bytes)?;
    let sockets = hilt_sockets(&model)?;
    let mesh_index = object_meshes.len();
    let draws = append_static_glm_mesh(scene, &model)?;
    object_meshes.push(StaticModelMesh {
        appearance: Appearance {
            model: definition.model.clone(),
            variant: String::new(),
        },
        draws,
        center: [0.0; 3],
        flash_bolt: None,
    });
    let cached = (mesh_index, sockets);
    model_cache.insert(definition.model.clone(), cached);
    Ok(cached)
}
