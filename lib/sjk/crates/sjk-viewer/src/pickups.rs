//! BaseJKA `ET_ITEM` model, bob, rotation, and visibility presentation.
//!
//! The transform follows `CG_Item` in `codemp/cgame/cg_ents.c:1906-1916` and
//! `2019-2154`. In particular, weapon/powerup bob is
//! `4 + cos((cg.time + 1000) * (0.005 + entityNum * 0.00001)) * 4`
//! (`cg_ents.c:2025-2031`), while autorotation is the shared `cg.autoAngles`
//! yaw `(cg.time & 2047) * 360 / 2048` (`cg_ents.c:3437-3447`). Retail model
//! paths mirror `bg_itemlist` and `CG_RegisterItemVisuals`; their embedded MD3
//! or GLM surface shaders are resolved by the existing appearance loader.

use super::{ActorInstance, GameAudio, Particle, StaticModelMesh};
use crate::effect_runtime::{self, EffectLibrary};
use crate::entity_materials::OverrideInstance;
use crate::model_materials;
use glam::{Quat, Vec3};
use sjk_client::{legacy_angles_to_quaternion, legacy_evaluate_trajectory, legacy_item_appearance};
use sjk_protocol::Snapshot;
use sjk_runtime::Appearance;
use sjk_vfs::VirtualFileSystem;
use std::collections::BTreeSet;
use std::time::{Duration, Instant};
pub(crate) mod simple;

const ET_ITEM: u8 = 2; // codemp/game/bg_public.h:1247
const EF_DEAD: u32 = 1 << 1; // codemp/game/bg_public.h:638
const EF_NODRAW: u32 = 1 << 8; // codemp/game/bg_public.h:645
const EF_ITEMPLACEHOLDER: u32 = 1 << 23; // codemp/game/bg_public.h:667
const EF_DROPPEDWEAPON: u32 = 1 << 25; // codemp/game/bg_public.h:669
const ITEM_COUNT: usize = 51; // bg_itemlist entries 0..50 in bg_misc.c:710-1686
const HOLO_MODEL: &str = "models/map_objects/mp/holo.md3"; // cg_main.c:1217
const ITEM_CONE_EFFECT: &str = "mp/itemcone"; // cg_main.c:1175
const FORCE_LIGHTSIDE: u8 = 1; // codemp/qcommon/q_shared.h:1045
const FORCE_DARKSIDE: u8 = 2; // codemp/qcommon/q_shared.h:1046

#[path = "pickup_catalog.rs"]
mod catalog;
pub(crate) use catalog::{Catalog, extend_appearances};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MaterialOverride {
    Placeholder,
    LightDisabled,
    DarkDisabled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Holo {
    origin: [f32; 3],
    rotation: [f32; 4],
    color: [f32; 4],
    play_cone: bool,
}

/// One pickup evaluated at the current presentation time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Presented {
    simple: bool,
    simple_origin: [f32; 3],
    simple_color: [f32; 4],
    pub(crate) entity_number: u16,
    pub(crate) item_index: usize,
    pub(crate) origin: [f32; 3],
    pub(crate) rotation: [f32; 4],
    pub(crate) scale: [f32; 3],
    pub(crate) dropped: bool,
    pub(crate) placeholder: bool,
    material_override: Option<MaterialOverride>,
    entity_color: [f32; 4],
    holo: Option<Holo>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Armor,
    Health,
    Holdable,
    Powerup,
    Weapon,
    Other,
}

/// Collect visible item models into a reused fixed-capacity output vector.
pub(crate) fn collect(snapshot: &Snapshot, at_time: i32, output: &mut Vec<Presented>) {
    output.clear();
    for state in &snapshot.entities {
        if state.entity_type() != ET_ITEM {
            continue;
        }
        let Ok(item_index) = usize::try_from(state.model_index()) else {
            continue;
        };
        if !(1..ITEM_COUNT).contains(&item_index) {
            continue;
        }
        let flags = state.e_flags();
        let placeholder = flags & EF_ITEMPLACEHOLDER != 0;
        // CG_Item temporarily clears EF_NODRAW for respawn placeholders, but
        // dropped/dead placeholders remain hidden (`cg_ents.c:1906-1909`,
        // `2187-2191`).
        if flags & EF_NODRAW != 0 && !placeholder || placeholder && flags & EF_DEAD != 0 {
            continue;
        }
        let base = legacy_evaluate_trajectory(
            state.trajectory_base(),
            state.trajectory_delta(),
            state.trajectory_type(),
            state.trajectory_time(),
            state.trajectory_duration(),
            at_time,
        );
        output.push(present(
            state.number(),
            item_index,
            flags,
            base,
            state.angles(),
            at_time,
            snapshot.player.force_side(),
        ));
    }
}

fn present(
    entity_number: u16,
    item_index: usize,
    flags: u32,
    mut origin: [f32; 3],
    placed_angles: [f32; 3],
    at_time: i32,
    force_side: u8,
) -> Presented {
    let simple_origin = [origin[0], origin[1], origin[2] + 16.0];
    let kind = item_kind(item_index);
    let dropped = flags & EF_DROPPEDWEAPON != 0;
    let grey = grey_override(item_index, force_side);
    // BaseJKA MP draws this decoration before the ordinary item at
    // cg_ents.c:1918-1953 whenever simple-items is disabled (SJK's current
    // default) and the weapon/powerup was not dropped.
    let holo = (matches!(kind, Kind::Weapon | Kind::Powerup) && !dropped).then(|| Holo {
        origin,
        rotation: legacy_angles_to_quaternion(placed_angles),
        color: if grey.is_some() {
            [150.0 / 255.0, 150.0 / 255.0, 150.0 / 255.0, 1.0]
        } else {
            [1.0; 4]
        },
        play_cone: grey.is_none(),
    });
    let rotates =
        matches!(kind, Kind::Weapon | Kind::Powerup) && (!dropped || matches!(kind, Kind::Powerup));
    let angles = if rotates {
        if !dropped {
            origin[2] += 16.0;
        }
        let scale = 0.005 + f32::from(entity_number) * 0.000_01;
        origin[2] += 4.0 + ((at_time as f32 + 1_000.0) * scale).cos() * 4.0;
        [0.0, (at_time & 2_047) as f32 * 360.0 / 2_048.0, 0.0]
    } else {
        origin[2] += static_vertical_offset(item_index, kind, dropped);
        placed_angles
    };
    Presented {
        simple: false,
        simple_origin,
        simple_color: simple::color(flags, grey.is_some(), at_time),
        entity_number,
        item_index,
        origin,
        rotation: legacy_angles_to_quaternion(angles),
        scale: if matches!(kind, Kind::Weapon) {
            [1.5; 3]
        } else {
            [1.0; 3]
        },
        dropped,
        placeholder: flags & EF_ITEMPLACEHOLDER != 0,
        material_override: grey
            .or_else(|| (flags & EF_ITEMPLACEHOLDER != 0).then_some(MaterialOverride::Placeholder)),
        entity_color: if grey.is_some() {
            [150.0 / 255.0, 150.0 / 255.0, 150.0 / 255.0, 200.0 / 255.0]
        } else if flags & EF_ITEMPLACEHOLDER != 0 {
            [0.0, 200.0 / 255.0, 85.0 / 255.0, 1.0]
        } else {
            [1.0; 4]
        },
        holo,
    }
}

fn grey_override(item_index: usize, force_side: u8) -> Option<MaterialOverride> {
    match (item_index, force_side) {
        (15, FORCE_DARKSIDE) => Some(MaterialOverride::LightDisabled),
        (16, FORCE_LIGHTSIDE) => Some(MaterialOverride::DarkDisabled),
        _ => None,
    }
}

fn item_kind(index: usize) -> Kind {
    match index {
        1 | 2 => Kind::Armor,
        3 => Kind::Health,
        4..=14 => Kind::Holdable,
        15..=18 => Kind::Powerup,
        19..=31 | 35..=39 => Kind::Weapon,
        _ => Kind::Other,
    }
}

fn static_vertical_offset(index: usize, kind: Kind, dropped: bool) -> f32 {
    if dropped && matches!(kind, Kind::Weapon) {
        return match index {
            25 | 28 | 35 => -12.0,
            26 => -13.0,
            27 | 36 | 37 => -16.0,
            29 => -10.0,
            30 => -6.0,
            31 => -11.0,
            _ => -8.0,
        };
    }
    match index {
        2 => 7.0,
        3 | 5 | 8 => 2.0,
        4 => 5.0,
        _ => 0.0,
    }
}

/// Append pickup instances using O(items) table lookup and preallocated groups.
pub(crate) fn append_instances(
    presented: &[Presented],
    catalog: &Catalog,
    meshes: &[StaticModelMesh],
    groups: &mut [Vec<ActorInstance>],
    overrides: model_materials::Overrides,
    override_instances: &mut Vec<OverrideInstance>,
) {
    for item in presented {
        if item.simple {
            continue;
        }
        let Some(mesh_index) = catalog.mesh(item.item_index) else {
            continue;
        };
        let rotation = Quat::from_array(item.rotation);
        let mut position = Vec3::from_array(item.origin);
        if matches!(item_kind(item.item_index), Kind::Weapon) && !item.dropped {
            // weaponMidpoint is derived from model bounds during
            // CG_RegisterWeapon (`cg_weapons.c:103-107`).
            position -= rotation * Vec3::from_array(meshes[mesh_index].center);
            position.z += 8.0;
        }
        let instance = if matches!(
            item.material_override,
            Some(MaterialOverride::LightDisabled | MaterialOverride::DarkDisabled)
        ) {
            ActorInstance::new(position.to_array(), item.rotation, item.scale)
                .with_rgba_tint(item.entity_color)
        } else if item.material_override == Some(MaterialOverride::Placeholder) {
            ActorInstance::new(position.to_array(), item.rotation, item.scale)
                .with_rgb_tint(item.entity_color)
        } else {
            ActorInstance::new(position.to_array(), item.rotation, item.scale)
        };
        let material = item.material_override.map(|material| match material {
            MaterialOverride::Placeholder => overrides.placeholder,
            MaterialOverride::LightDisabled => overrides.light_disabled,
            MaterialOverride::DarkDisabled => overrides.dark_disabled,
        });
        if let Some(material) = material {
            if override_instances.len() < override_instances.capacity() {
                override_instances.push(OverrideInstance {
                    mesh: crate::entity_materials::OverrideMesh::Object(mesh_index),
                    material: Some(material),
                    instance,
                    no_depth: false,
                    forced_alpha: false,
                });
            }
        } else {
            groups[mesh_index].push(instance);
        }
        if let (Some(holo), Some(holo_mesh)) = (item.holo, catalog.holo_mesh) {
            groups[holo_mesh].push(if holo.color == [1.0; 4] {
                ActorInstance::new(holo.origin, holo.rotation, [1.0; 3])
            } else {
                ActorInstance::new(holo.origin, holo.rotation, [1.0; 3]).with_rgb_tint(holo.color)
            });
        }
    }
}

/// The item cone's orientation: `CG_Item` plays it along `uNorm = (0, 0, 1)`
/// (`cg_ents.c:1922-1927,1952`), and `CFxScheduler::PlayEffect` makes that
/// direction the effect's forward axis (`FxScheduler.cpp:712-718`), so the
/// EFX's `origin 4..24 0 0` flares and its cylinder rise up to the hologram.
/// An identity rotation left forward on world +X and laid the cone sideways.
pub(crate) fn cone_rotation() -> Quat {
    crate::combat_effects::rotation_from_direction([0.0, 0.0, 1.0])
}

/// Replay BaseJKA's continuously submitted item-cone EFX for ordinary
/// non-dropped weapon/powerup holograms (`cg_ents.c:1950-1953`).
pub(crate) fn spawn_cones(
    presented: &[Presented],
    particles: &mut Vec<Particle>,
    auxiliary: &mut crate::effect_aux::Runtime,
    effects: &mut EffectLibrary,
    vfs: &VirtualFileSystem,
    audio: &mut Option<GameAudio>,
    at_time: i32,
    now: Instant,
) {
    // Re-played on the reference 8 ms cadence, not per rendered frame (`effect_cadence.rs`).
    if !auxiliary.continuous.due(now) {
        return;
    }
    for item in presented
        .iter()
        .filter(|item| item.holo.is_some_and(|holo| holo.play_cone))
    {
        effect_runtime::spawn_effect(
            particles,
            auxiliary,
            effects,
            vfs,
            ITEM_CONE_EFFECT,
            Vec3::from_array(item.holo.expect("filtered holo").origin),
            now,
            u32::from(item.entity_number) ^ (at_time as u32).rotate_left(9),
            0,
            audio,
            cone_rotation(),
        );
    }
}
