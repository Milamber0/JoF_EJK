//! The hosting engine's entities, drawn by SJK's own pipelines.
//!
//! Each frame the engine hands over its main view's scene ([`Scene`]): every model it
//! draws (each model of a Ghoul2 entity separately, posed by the engine's own bones),
//! its sprites, beams, quads and saber blades, and its dynamic lights. [`submit`] turns
//! them into what SJK's frame draws from: skinned actor meshes with this frame's
//! palette, static models, movers, saber instances, frame particles and point lights.
//! The pass runs inside SJK's frame (`render_inner`) where SJK submits its own
//! session's entities, so lighting, fog, materials and post-processing are SJK's.

use super::assets;
use crate::actor_instance::ActorInstance;
use crate::dynamic_lights::PointLight;
use crate::effect_runtime::Particle;
use crate::entity_materials::{OverrideInstance, OverrideMesh};
use crate::particle_types::PrimitiveShape;
use crate::saber::{Blade, Instance as SaberInstance};
use crate::saber_rgb::BladeColor;
use crate::{GpuState, scene_flatten::FlattenedScene};
use glam::{Mat3, Quat, Vec3};
use sjk_runtime::Appearance;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::{Duration, Instant};

/// `sjkHostEntity_t`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Entity {
    pub(crate) kind: i32,
    pub(crate) model: i32,
    pub(crate) shader: i32,
    pub(crate) skin: i32,
    pub(crate) renderfx: i32,
    pub(crate) key: i32,
    pub(crate) origin: [f32; 3],
    pub(crate) axis: [[f32; 3]; 3],
    pub(crate) scale: [f32; 3],
    pub(crate) end: [f32; 3],
    pub(crate) radius: f32,
    pub(crate) rotation: f32,
    pub(crate) length: f32,
    pub(crate) width2: f32,
    pub(crate) rgba: [u8; 4],
    pub(crate) frame: i32,
    pub(crate) oldframe: i32,
    pub(crate) backlerp: f32,
    pub(crate) lighting_origin: [f32; 3],
    pub(crate) first_bone: i32,
    pub(crate) num_bones: i32,
    pub(crate) first_surface: i32,
    pub(crate) num_surfaces: i32,
    pub(crate) root_surface: i32,
}

/// `sjkHostLight_t`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Light {
    pub(crate) origin: [f32; 3],
    pub(crate) radius: f32,
    pub(crate) color: [f32; 3],
    pub(crate) additive: i32,
}

pub(crate) const KIND_MODEL: i32 = 0;
pub(crate) const KIND_GHOUL2: i32 = 1;
pub(crate) const KIND_BRUSH: i32 = 2;
pub(crate) const KIND_SPRITE: i32 = 3;
pub(crate) const KIND_QUAD: i32 = 4;
pub(crate) const KIND_LINE: i32 = 5;
pub(crate) const KIND_SABER: i32 = 6;

const RF_THIRD_PERSON: i32 = 0x2;
const RF_DEPTHHACK: i32 = 0x8;
const RF_NODEPTH: i32 = 0x10;
const RF_FORCE_ENT_ALPHA: i32 = 0x400;
const RF_RGB_TINT: i32 = 0x800;

/// `sjkHostPolyVert_t`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct PolyVert {
    pub(crate) xyz: [f32; 3],
    pub(crate) st: [f32; 2],
    pub(crate) modulate: [u8; 4],
}

/// `sjkHostPoly_t`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Poly {
    pub(crate) shader: i32,
    pub(crate) first_vertex: i32,
    pub(crate) num_vertices: i32,
}

/// One frame's scene, copied from the engine.
#[derive(Default)]
pub(crate) struct Scene {
    pub(crate) entities: Vec<Entity>,
    pub(crate) bones: Vec<[[f32; 4]; 3]>,
    pub(crate) ints: Vec<i32>,
    pub(crate) lights: Vec<Light>,
    pub(crate) polys: Vec<Poly>,
    pub(crate) poly_verts: Vec<PolyVert>,
    pub(crate) decals: Vec<Decal>,
    /// Entities of the main view, the rest belonging to `previews`.
    pub(crate) main_entities: std::ops::Range<usize>,
    /// The model views without a world (`RDF_NOWORLDMODEL`) of this frame.
    pub(crate) previews: Vec<super::preview::PreviewView>,
}

/// `sjkHostDecal_t`: an `RE_AddDecalToScene` mark.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Decal {
    pub(crate) shader: i32,
    pub(crate) origin: [f32; 3],
    pub(crate) direction: [f32; 3],
    pub(crate) orientation: f32,
    pub(crate) rgba: [f32; 4],
    pub(crate) alpha_fade: i32,
    pub(crate) radius: f32,
    pub(crate) temporary: i32,
}

thread_local! {
    static SCENE: RefCell<Scene> = RefCell::new(Scene::default());
    static WORLD: RefCell<World> = RefCell::new(World::default());
}

/// Hand over the next frame's scene.
pub(crate) fn set_scene(scene: Scene) {
    SCENE.with(|cell| *cell.borrow_mut() = scene);
}

/// Forget the meshes built for the last world (a new [`GpuState`] has none of them).
pub(crate) fn reset() {
    WORLD.with(|cell| *cell.borrow_mut() = World::default());
}

/// A Ghoul2 mesh built for one model and skin, held by one model instance at a time
/// (its bone palette is the mesh's); a mesh whose instance went away is handed to the
/// next instance of the same model and skin.
struct Actor {
    model: i32,
    skin: i32,
    /// The instance (`sjkHostEntity_t::key`) drawing it.
    owner: i32,
    /// The last frame it was drawn.
    frame: u64,
}

/// What this world's [`GpuState`] holds for the engine's handles.
#[derive(Default)]
struct World {
    /// Ghoul2 meshes by actor mesh index.
    actors: HashMap<usize, Actor>,
    /// The mesh each instance, model and skin draws with.
    owners: HashMap<(i32, i32, i32), usize>,
    /// Frames submitted on this world.
    frame: u64,
    /// Ghoul2 models on the built-in `*default` skeleton (saber hilts, rigid props) by
    /// model handle: drawn as static models moved by their one bone.
    rigid: HashMap<i32, bool>,
    /// Static model meshes by model handle; `None` when the model would not load.
    objects: HashMap<i32, Option<usize>>,
    /// Ghoul2 models that failed to build, not tried again on this world.
    failed: HashSet<(i32, i32)>,
    gla: crate::player_assets::GlaCache,
    /// Effect shaders the particle atlas lacks, added on the next refresh.
    missing_effects: BTreeSet<String>,
    last_atlas_refresh: Option<Instant>,
}

/// A rotation and per-axis scale from Quake's `axis` (forward, left, up, possibly
/// scaled when `nonNormalizedAxes`).
fn transform(axis: &[[f32; 3]; 3]) -> (Quat, [f32; 3]) {
    let columns = axis.map(Vec3::from_array);
    let scale = columns.map(Vec3::length);
    let unit = |i: usize| {
        if scale[i] > 1e-6 {
            columns[i] / scale[i]
        } else {
            [Vec3::X, Vec3::Y, Vec3::Z][i]
        }
    };
    let rotation = Quat::from_mat3(&Mat3::from_cols(unit(0), unit(1), unit(2))).normalize();
    (
        rotation,
        scale.map(|value| if value > 1e-6 { value } else { 1.0 }),
    )
}

fn instance(entity: &Entity, rotation: Quat, scale: [f32; 3]) -> ActorInstance {
    let mut out = ActorInstance::new(entity.origin, rotation.to_array(), scale);
    out.entity_color = entity.rgba.map(|value| f32::from(value) / 255.0);
    out.depth_hack = if entity.renderfx & RF_DEPTHHACK != 0 {
        1.0
    } else {
        0.0
    };
    out.entity_control = [
        if entity.renderfx & RF_RGB_TINT != 0 {
            1.0
        } else {
            0.0
        },
        if entity.renderfx & RF_FORCE_ENT_ALPHA != 0 {
            1.0
        } else {
            0.0
        },
    ];
    if entity.renderfx & RF_THIRD_PERSON != 0 {
        out.view_flags |= 1;
    }
    out
}

/// Add the engine's scene to this frame's submissions; `time` is the view's clock in
/// milliseconds. Runs where SJK submits its own session's entities.
pub(crate) fn submit(gpu: &mut GpuState, time: i64) {
    let mut scene = SCENE.with(|cell| std::mem::take(&mut *cell.borrow_mut()));
    let main = scene.main_entities.start.min(scene.entities.len())
        ..scene.main_entities.end.min(scene.entities.len());
    scene.entities = scene.entities[main].to_vec();
    WORLD.with(|cell| {
        let mut world = cell.borrow_mut();
        let now = Instant::now();
        world.frame += 1;
        let mut palettes_staged = false;
        for entity in &scene.entities {
            match entity.kind {
                KIND_GHOUL2 => {
                    palettes_staged |= submit_ghoul2(gpu, &mut world, &scene, entity);
                }
                KIND_MODEL => submit_model(gpu, &mut world, entity),
                KIND_BRUSH => submit_brush(gpu, entity),
                KIND_SABER => submit_saber(gpu, entity, time),
                KIND_SPRITE | KIND_QUAD | KIND_LINE => {
                    submit_effect(gpu, &mut world, entity, now, time);
                }
                _ => {}
            }
        }
        if palettes_staged {
            gpu.geometry.skinning.flush(&gpu.queue);
        }
        // SJK lights a blade with its own broader light (`saber_lights`), pushed with the
        // blade; cgame's light for the same blade, at its midpoint, is left out.
        let midpoints: Vec<Vec3> = scene
            .entities
            .iter()
            .filter(|entity| entity.kind == KIND_SABER)
            .map(|entity| {
                Vec3::from_array(entity.origin)
                    + Vec3::from_array(entity.axis[0]).normalize_or_zero() * (entity.length * 0.5)
            })
            .collect();
        for light in &scene.lights {
            let origin = Vec3::from_array(light.origin);
            if midpoints.iter().any(|mid| mid.distance(origin) < 12.0) {
                continue;
            }
            gpu.dynamic_lights.push(PointLight {
                origin: light.origin,
                radius: light.radius,
                color: light.color,
            });
        }
        submit_polys(gpu, &mut world, &scene, time);
        submit_decals(gpu, &mut world, &scene);
        refresh_effect_atlas(gpu, &mut world, now);
    });
    SCENE.with(|cell| {
        // Keep the allocations for the next frame.
        let mut spare = cell.borrow_mut();
        if spare.entities.capacity() < scene.entities.capacity() {
            *spare = Scene {
                entities: Vec::with_capacity(scene.entities.capacity()),
                ..Default::default()
            };
        }
    });
}

/// One posed Ghoul2 model: its own actor mesh, this frame's bones in its palette.
fn submit_ghoul2(gpu: &mut GpuState, world: &mut World, scene: &Scene, entity: &Entity) -> bool {
    // A shell over a model (force powers, powerups) draws the mesh again with one
    // shader; one SJK has no material for is left out.
    if entity.shader != 0 && override_material(gpu, entity.shader).is_none() {
        return false;
    }
    if is_rigid(gpu, world, entity.model) {
        submit_rigid_ghoul2(gpu, world, scene, entity);
        return false;
    }
    let Some(mesh) = actor_mesh(gpu, world, entity) else {
        return false;
    };
    let Some(actor) = gpu.actor_meshes.get_mut(mesh) else {
        return false;
    };
    let first = usize::try_from(entity.first_bone).unwrap_or(0);
    let count = usize::try_from(entity.num_bones).unwrap_or(0);
    let Some(bones) = scene.bones.get(first..first + count) else {
        return false;
    };
    // Surfaces the engine switched on or off, and its root surface.
    let first = usize::try_from(entity.first_surface).unwrap_or(0);
    let pairs = usize::try_from(entity.num_surfaces).unwrap_or(0);
    let overrides: Vec<(usize, u32)> = scene
        .ints
        .get(first..first + pairs * 2)
        .unwrap_or(&[])
        .chunks_exact(2)
        .filter_map(|pair| Some((usize::try_from(pair[0]).ok()?, pair[1] as u32)))
        .collect();
    let root = usize::try_from(entity.root_surface).ok();
    actor
        .surfaces
        .host_overrides(&overrides, root, &actor.preview.mesh.hierarchy);
    let staged = match &actor.gpu_palette {
        Some(palette) => {
            let wanted = palette.len();
            let result = if bones.len() == wanted {
                palette.stage(&mut gpu.geometry.skinning, bones)
            } else {
                let mut matched = bones.to_vec();
                matched.resize(wanted, [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]]);
                palette.stage(&mut gpu.geometry.skinning, &matched)
            };
            result.is_ok()
        }
        None => false,
    };
    let (rotation, _) = transform(&entity.axis);
    // Ghoul2 applies the model scale in its bones (`RootMatrix`).
    push(gpu, OverrideMesh::Actor(mesh), entity, instance(entity, rotation, [1.0; 3]));
    staged
}

/// The shared-table material of cgame's custom shader `handle`
/// (`model_materials`: force shells, item overrides, the disintegration burn).
fn override_material(gpu: &GpuState, handle: i32) -> Option<usize> {
    let name = assets::shader_name(handle)?;
    let overrides = &gpu.model_material_overrides;
    overrides.force(&name).or_else(|| {
        let name = name.to_ascii_lowercase();
        match name.as_str() {
            "powerups/placeholder" => Some(overrides.placeholder),
            "gfx/misc/mp_light_enlight_disable" => Some(overrides.light_disabled),
            "gfx/misc/mp_dark_enlight_disable" => Some(overrides.dark_disabled),
            crate::disintegration::BURN_SHADER => Some(overrides.disruptor_burn),
            _ => None,
        }
    })
}

/// Draw `instance` of `mesh`: in its group, or as an override draw for a custom
/// shader (`customShader`) or a forced alpha (`RF_FORCE_ENT_ALPHA`).
fn push(gpu: &mut GpuState, mesh: OverrideMesh, entity: &Entity, instance: ActorInstance) {
    let material = if entity.shader != 0 {
        match override_material(gpu, entity.shader) {
            Some(material) => Some(material),
            None => return,
        }
    } else {
        None
    };
    let forced_alpha = entity.renderfx & RF_FORCE_ENT_ALPHA != 0;
    if material.is_some() || forced_alpha {
        let overrides = &mut gpu.pickup_override_instances;
        if overrides.len() < overrides.capacity() {
            overrides.push(OverrideInstance {
                mesh,
                material,
                instance,
                no_depth: entity.renderfx & RF_NODEPTH != 0,
                forced_alpha,
            });
        }
        return;
    }
    let group = match mesh {
        OverrideMesh::Actor(index) => gpu.actor_groups.get_mut(index),
        OverrideMesh::Object(index) => gpu.object_groups.get_mut(index),
    };
    if let Some(group) = group {
        group.push(instance);
    }
}

/// The mesh `entity` draws with: its own from earlier frames, one a gone instance of
/// the same model and skin left, or a new one. A shell (`customShader`) draws over
/// the mesh its body drew this frame.
fn actor_mesh(gpu: &mut GpuState, world: &mut World, entity: &Entity) -> Option<usize> {
    let frame = world.frame;
    let owner = (entity.key, entity.model, entity.skin);
    if entity.shader != 0 {
        if let Some(&mesh) = world.owners.get(&owner) {
            return Some(mesh);
        }
        return world
            .actors
            .iter()
            .find(|(_, actor)| actor.owner == entity.key && actor.model == entity.model && actor.frame == frame)
            .map(|(&mesh, _)| mesh);
    }
    if let Some(&mesh) = world.owners.get(&owner) {
        if let Some(actor) = world.actors.get_mut(&mesh) {
            actor.frame = frame;
        }
        return Some(mesh);
    }
    let free = world
        .actors
        .iter()
        .find(|(_, actor)| {
            actor.model == entity.model && actor.skin == entity.skin && actor.frame + 1 < frame
        })
        .map(|(&mesh, actor)| (mesh, actor.owner));
    let mesh = match free {
        Some((mesh, previous)) => {
            world.owners.remove(&(previous, entity.model, entity.skin));
            mesh
        }
        None => build_actor(gpu, world, entity)?,
    };
    world.actors.insert(
        mesh,
        Actor {
            model: entity.model,
            skin: entity.skin,
            owner: entity.key,
            frame,
        },
    );
    world.owners.insert(owner, mesh);
    Some(mesh)
}

/// Whether GLM `model` uses the built-in one-bone skeleton (`*default`): its name
/// field at byte 72 of the header (`mdxmHeader_t::animName`).
fn is_rigid(gpu: &GpuState, world: &mut World, model: i32) -> bool {
    *world.rigid.entry(model).or_insert_with(|| {
        let Some(name) = assets::model_name(model) else {
            return false;
        };
        let Some(asset) = gpu.vfs.as_ref().and_then(|vfs| vfs.read(&name).ok().flatten()) else {
            return false;
        };
        asset
            .bytes
            .get(72..80)
            .is_some_and(|field| field.eq_ignore_ascii_case(b"*default"))
    })
}

/// A rigid Ghoul2 model (a saber hilt in a hand): its static mesh, placed by the
/// entity's transform times its one bone, which carries its bolt.
fn submit_rigid_ghoul2(gpu: &mut GpuState, world: &mut World, scene: &Scene, entity: &Entity) {
    let Some(bone) = usize::try_from(entity.first_bone)
        .ok()
        .and_then(|first| scene.bones.get(first))
    else {
        return;
    };
    let columns = [0, 1, 2].map(|c| Vec3::new(bone[0][c], bone[1][c], bone[2][c]));
    let local_scale = columns.map(Vec3::length);
    let local = Mat3::from_cols(
        columns[0] / local_scale[0].max(1e-6),
        columns[1] / local_scale[1].max(1e-6),
        columns[2] / local_scale[2].max(1e-6),
    );
    let translation = Vec3::new(bone[0][3], bone[1][3], bone[2][3]);
    let (rotation, _) = transform(&entity.axis);
    let mut placed = *entity;
    placed.origin = (Vec3::from_array(entity.origin) + rotation * translation).to_array();
    let rotation = (rotation * Quat::from_mat3(&local)).normalize();
    let Some(mesh) = object_mesh(gpu, world, entity.model) else {
        return;
    };
    push(gpu, OverrideMesh::Object(mesh), &placed, instance(&placed, rotation, local_scale));
}

fn build_actor(gpu: &mut GpuState, world: &mut World, entity: &Entity) -> Option<usize> {
    if world.failed.contains(&(entity.model, entity.skin)) {
        return None;
    }
    let built = (|| -> Result<usize, Box<dyn std::error::Error>> {
        let glm = assets::model_name(entity.model).ok_or("unnamed model")?;
        let skin = (entity.skin > 0)
            .then(|| assets::skin_lines(entity.skin))
            .flatten();
        let vfs = gpu.vfs.clone().ok_or("no VFS")?;
        let preview = crate::player_assets::load_host_appearance(
            &vfs,
            &glm,
            skin.as_deref(),
            &mut world.gla,
        )?;
        let appearance = Appearance {
            model: glm.trim_end_matches(".glm").to_owned(),
            variant: format!("host-{}", entity.skin),
        };
        let mut scene = FlattenedScene::default();
        let mesh = crate::actor_load::build_actor_mesh(
            &mut scene,
            preview,
            None,
            false,
            appearance.clone(),
            [None, None],
        )?;
        let mesh = gpu.upload_built_actor(mesh, scene, &appearance)?;
        gpu.actor_meshes.push(mesh);
        gpu.actor_groups.push(Vec::with_capacity(2));
        crate::log::progress(format_args!("rd-freedom: drawing {glm} (skin {})", entity.skin));
        Ok(gpu.actor_meshes.len() - 1)
    })();
    match built {
        Ok(mesh) => Some(mesh),
        Err(error) => {
            crate::log::progress(format_args!(
                "rd-freedom: could not draw model {}: {error}",
                assets::model_name(entity.model).unwrap_or_default()
            ));
            world.failed.insert((entity.model, entity.skin));
            None
        }
    }
}

/// An MD3 or rigid GLM model, loaded as SJK loads a game model mid-match.
fn submit_model(gpu: &mut GpuState, world: &mut World, entity: &Entity) {
    let Some(mesh) = object_mesh(gpu, world, entity.model) else {
        return;
    };
    let (rotation, scale) = transform(&entity.axis);
    let scale = [
        scale[0] * entity.scale[0],
        scale[1] * entity.scale[1],
        scale[2] * entity.scale[2],
    ];
    push(gpu, OverrideMesh::Object(mesh), entity, instance(entity, rotation, scale));
}

/// The static mesh of model handle `model`, loaded the first time it is drawn.
fn object_mesh(gpu: &mut GpuState, world: &mut World, model: i32) -> Option<usize> {
    *world.objects.entry(model).or_insert_with(|| {
        let name = assets::model_name(model)?;
        let appearance = Appearance {
            model: name.clone(),
            variant: String::new(),
        };
        if let Err(error) = gpu.load_config_model(&appearance) {
            crate::log::progress(format_args!("rd-freedom: could not draw model {name}: {error}"));
            return None;
        }
        gpu.object_meshes
            .iter()
            .position(|mesh| mesh.appearance == appearance)
    })
}

/// An inline BSP model (a door, a lift) where the game put it.
fn submit_brush(gpu: &mut GpuState, entity: &Entity) {
    let Some(mesh) = usize::try_from(entity.model)
        .ok()
        .and_then(|model| gpu.mover_catalog.mesh_of(model))
    else {
        return;
    };
    let (rotation, _) = transform(&entity.axis);
    let mut mover = ActorInstance::new(entity.origin, rotation.to_array(), [1.0; 3]);
    mover.view_flags = ActorInstance::WORLD;
    if let Some(group) = gpu.mover_groups.get_mut(mesh) {
        group.push(mover);
    }
}

/// The colour a saber glow shader names (`gfx/effects/sabers/blue_glow2`), or the
/// entity colour for the RGB blades of JA+ and EternalJK.
fn blade_color(shader: &str, rgba: [u8; 4]) -> BladeColor {
    use crate::saber::Color;
    let shader = shader.to_ascii_lowercase();
    let named = [
        ("red", Color::Red),
        ("orange", Color::Orange),
        ("yellow", Color::Yellow),
        ("green", Color::Green),
        ("blue", Color::Blue),
        ("purple", Color::Purple),
    ];
    match named.iter().find(|(name, _)| shader.contains(name)) {
        Some((_, color)) => BladeColor::Retail(*color),
        None => BladeColor::from_rgb([rgba[0], rgba[1], rgba[2]]),
    }
}

fn submit_saber(gpu: &mut GpuState, entity: &Entity, time: i64) {
    let direction = Vec3::from_array(entity.axis[0]).normalize_or_zero();
    if direction == Vec3::ZERO || entity.length < 0.5 {
        return;
    }
    let blade = Blade {
        base: entity.origin,
        direction: direction.to_array(),
        length: entity.length,
        radius: entity.radius,
    };
    let shader = assets::shader_name(entity.shader).unwrap_or_default();
    let key = (entity.origin[0].to_bits() ^ entity.origin[1].to_bits().rotate_left(11)) as u32;
    let color = blade_color(&shader, entity.rgba);
    gpu.saber_instances.extend(
        SaberInstance::pair(blade, color)
            .map(|instance| instance.with_animation(time as f64 * 0.001, key)),
    );
    let mut blades = [None; 8];
    blades[0] = Some(blade);
    crate::saber_submission::lights::append(
        &mut gpu.dynamic_lights,
        &blades,
        color,
        1,
        false,
        time,
        u64::from(key),
    );
}

/// Whether `shader` is the core line cgame draws inside a saber glow; SJK's blade
/// pair draws its own core.
fn saber_core(shader: &str) -> bool {
    let shader = shader.to_ascii_lowercase();
    shader.contains("sabers/") && shader.contains("_line")
}

/// A sprite, oriented quad or beam as one frame's particle: SJK's effect layers draw
/// it with the shader's own stages and blend.
fn submit_effect(gpu: &mut GpuState, world: &mut World, entity: &Entity, now: Instant, time: i64) {
    let Some(shader) = assets::shader_name(entity.shader) else {
        return;
    };
    if entity.kind == KIND_LINE && saber_core(&shader) {
        return;
    }
    if !crate::particle_types::frame_billboard_fits(gpu.particles.len()) {
        return;
    }
    let lower = shader.to_ascii_lowercase();
    if !gpu.particle_atlas.animations.contains_key(lower.as_str()) {
        world.missing_effects.insert(lower);
    }
    let constant = |value: f32| {
        crate::effect_envelope::Envelope::from_values(
            value,
            value,
            0.0,
            sjk_effect::CurveFlags::default(),
        )
    };
    let origin = Vec3::from_array(entity.origin);
    let (size, streak, normal) = match entity.kind {
        KIND_LINE => (
            entity.radius * 0.5,
            Some(Vec3::from_array(entity.end) - origin),
            None,
        ),
        KIND_QUAD => (
            entity.radius,
            None,
            Some(Vec3::from_array(entity.axis[0]).normalize_or_zero()),
        ),
        _ => (entity.radius, None, None),
    };
    let [r, g, b, a] = entity.rgba.map(|value| f32::from(value) / 255.0);
    let shader = gpu.effects.shader(&shader);
    let physics = crate::particle_physics::State::new(
        gpu.effects.code_primitive_definition(),
        0,
        0,
        0.0,
        sjk_effect::PrimitiveFlags::default(),
    );
    gpu.particles.push(Particle {
        motion: crate::particle_motion::Motion::new(origin, Vec3::ZERO, Vec3::ZERO, 0.0, 0.0, now),
        spawned_at: now,
        delay: Duration::ZERO,
        lifetime: Duration::from_secs(60),
        size: constant(size),
        start_length: 1.0,
        end_length: 1.0,
        streak,
        trace_streak: false,
        normal,
        alpha: constant(a),
        use_alpha: true,
        set_shader_time: false,
        rgb: [constant(r), constant(g), constant(b)],
        seed: time as u32 ^ entity.origin[0].to_bits(),
        shader,
        physics,
        shape: PrimitiveShape::FrameBillboard,
    });
}

/// The engine's polys (`RE_AddPolyToScene`: cgame's marks, effect polys, trails) through
/// SJK's decal layer, which draws a poly with its shader's stages and blend.
/// `RE_AddDecalToScene`: SJK projects the mark into the world (`decal_marks`).
fn submit_decals(gpu: &mut GpuState, world: &mut World, scene: &Scene) {
    for decal in &scene.decals {
        let Some(name) = assets::shader_name(decal.shader) else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        if !gpu.particle_atlas.animations.contains_key(lower.as_str()) {
            world.missing_effects.insert(lower.clone());
        }
        let request = crate::decal_marks::DecalRequest {
            origin: Vec3::from_array(decal.origin),
            direction: Vec3::from_array(decal.direction),
            orientation: decal.orientation,
            color: decal.rgba,
            radius: decal.radius,
            shader: gpu.effects.shader(&lower),
        };
        if decal.temporary != 0 {
            gpu.effect_aux.decals.request_temporary(request);
        } else {
            gpu.effect_aux.decals.request(request);
        }
    }
}

/// The trail style of cgame's saber trail shaders (`saberBlur`, `swordTrail`).
fn trail_style(shader: &str) -> Option<u32> {
    match shader {
        "gfx/effects/sabers/saberblur" | "gfx/effects/sabers/sabertrail" => Some(0),
        "gfx/effects/sabers/swordtrail" => Some(1),
        _ => None,
    }
}

fn submit_polys(gpu: &mut GpuState, world: &mut World, scene: &Scene, time: i64) {
    gpu.effect_aux.decals.clear_host_polys();
    for poly in &scene.polys {
        let Some(name) = assets::shader_name(poly.shader) else {
            continue;
        };
        let first = usize::try_from(poly.first_vertex).unwrap_or(0);
        let count = usize::try_from(poly.num_vertices).unwrap_or(0);
        let Some(vertices) = scene.poly_verts.get(first..first + count) else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        if let Some(style) = trail_style(&lower)
            && let Ok(corners) = <&[PolyVert; 4]>::try_from(vertices)
        {
            // cgame's `CTrail` slice, redrawn each frame with its scrolled ST: SJK's
            // trail pass draws it this frame only, in its own vertex order.
            let quad = crate::saber_trail::Quad {
                vertices: corners.map(|vertex| crate::saber_trail::Vertex {
                    position: vertex.xyz,
                    uv: vertex.st,
                    color: [
                        f32::from(vertex.modulate[0]) / 255.0,
                        f32::from(vertex.modulate[1]) / 255.0,
                        f32::from(vertex.modulate[2]) / 255.0,
                        1.0,
                    ],
                    style,
                    _padding: 0,
                }),
                lifetime_millis: 1,
            };
            gpu.saber_trail_segments.insert(quad, time);
            continue;
        }
        if !gpu.particle_atlas.animations.contains_key(lower.as_str()) {
            world.missing_effects.insert(lower.clone());
        }
        let slot = crate::effect_blend::slot(
            gpu.particle_atlas
                .layers_for(&lower, 0.0)
                .iter()
                .next()
                .map_or(crate::particle_types::ParticleBlend::Add, |layer| layer.blend),
        ) as u8;
        let mut color = [0.0f32; 4];
        for vertex in vertices {
            for (channel, value) in color.iter_mut().zip(vertex.modulate) {
                *channel += f32::from(value) / 255.0;
            }
        }
        let color = color.map(|sum| sum / vertices.len() as f32);
        let points: Vec<crate::decal_marks::DecalVertex> = vertices
            .iter()
            .map(|vertex| crate::decal_marks::DecalVertex {
                position: vertex.xyz,
                st: vertex.st,
            })
            .collect();
        let shader = gpu.effects.shader(&lower);
        gpu.effect_aux
            .decals
            .push_host_poly(shader, slot, color, &points);
    }
}

/// Add the effect shaders the engine used that the particle atlas lacks, at most
/// twice a second (each refresh rebuilds the atlas).
fn refresh_effect_atlas(gpu: &mut GpuState, world: &mut World, now: Instant) {
    if world.missing_effects.is_empty()
        || world
            .last_atlas_refresh
            .is_some_and(|last| now.duration_since(last) < Duration::from_millis(500))
    {
        return;
    }
    world.last_atlas_refresh = Some(now);
    let mut required: BTreeSet<String> = std::mem::take(&mut world.missing_effects);
    gpu.effects.append_shader_paths(&mut required);
    required.extend(gpu.particle_atlas.animations.keys().cloned());
    let Some(vfs) = gpu.vfs.clone() else {
        return;
    };
    match crate::particle_atlas::create(
        &gpu.device,
        &gpu.queue,
        &crate::particle_atlas::layout(&gpu.device),
        &vfs,
        &gpu.shaders,
        &required,
    ) {
        Ok(atlas) => gpu.particle_atlas = atlas,
        Err(error) => crate::log::progress(format_args!("rd-freedom: effect atlas: {error}")),
    }
}
