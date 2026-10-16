//! Flying primary hilt and blade submission, `codemp/cgame/cg_players.c:10050-10305`.
//!
//! The hilt keeps the entity's evaluated position, spin (yaw) and roll; its pitch
//! is the owner's client-side tilt towards 90, which lays the saber flat
//! (`saber_trail::tilt`), and a saber being pulled back faces away from its owner.
//! Blade ROLL is zeroed. The sockets, world-blade transform and glow/core pair are
//! shared with held and menu-stage sabers (`saber_submission.rs`,
//! `menu_stage/sabers.rs`); the menu's own throw animation is separate
//! (`menu_stage/throw.rs`).

use crate::actor_instance::ActorInstance;
use crate::saber::{self, Instance};
use crate::saber_hilts::HiltCatalog;
use crate::saber_rgb::BladeColor;
use crate::saber_trail::tilt;
use glam::{EulerRot, Quat, Vec3};
use sjk_client::{LegacyThrownSaber, LegacyThrownSabers};
use sjk_runtime::{EntityId, SceneEntity, Transform};

/// Claim a flying saber before the ordinary rigid-object path can draw it twice.
pub(super) fn submit(
    sinks: &mut super::Sinks<'_>,
    thrown: Option<&LegacyThrownSabers<'_>>,
    entity: &SceneEntity,
    transform: Transform,
) -> bool {
    let Some(number) = entity
        .id
        .get()
        .checked_sub(1)
        .and_then(|id| u16::try_from(id).ok())
    else {
        return false;
    };
    let Some(mut owner) = thrown.and_then(|thrown| thrown.get(number)) else {
        return false;
    };
    // A player muted on this PC throws the default saber in the default colour.
    if crate::muted_players::entity_muted(sinks.muted, u64::from(owner.owner) + 1) {
        owner.name = crate::muted_players::SABER;
        owner.color = crate::muted_players::BLADE;
    }
    let Some(catalog) = sinks.saber_hilts else {
        return false;
    };
    let owner_entity = u64::from(owner.owner) + 1;
    let Some(owner_tilt) = sinks.saber_states.tilt_mut(owner_entity) else {
        return false;
    };
    if !owner.in_flight {
        // Caught: the lingering entity draws nothing and the tilt starts over
        // (`cg_ents.c:871`, `cg_players.c:10448-10449`).
        owner_tilt.reset();
        return true;
    }
    let presented = Quat::from_array(transform.rotation);
    let (_, server_pitch, _) = presented.to_euler(EulerRot::ZYX);
    let pitch = owner_tilt.advance(server_pitch.to_degrees(), sinks.presentation_time);
    let returning_from = owner
        .returning
        .then(|| {
            sinks
                .world
                .entity(EntityId::new(owner_entity))
                .map(|owner| {
                    (
                        Vec3::from_array(transform.translation),
                        Vec3::from_array(owner.sample(sinks.presentation_time).translation),
                    )
                })
        })
        .flatten();
    let (hilt_rotation, blade_rotation) = tilt::flight_rotations(presented, pitch, returning_from);
    let pose = Transform {
        rotation: hilt_rotation.to_array(),
        ..transform
    };
    let color = sinks
        .saber_skins
        .blade_color(owner_entity, BladeColor::from_rgb(owner.color));
    if let Some((hilt, blades)) = submit_hilt(
        owner,
        color,
        pose,
        blade_rotation,
        catalog,
        sinks.object_groups,
        sinks.saber_instances,
        sinks.presentation_time,
    ) {
        add_trails(sinks, owner_entity, hilt, &blades, color);
        crate::saber_submission::lights::append(
            sinks.lights,
            &blades,
            color,
            hilt.num_blades,
            hilt.no_dlight,
            sinks.presentation_time,
            entity.id.get(),
        );
    }
    true
}

/// A flying primary always trails (`saberInFlight && saberNum == 0` in
/// `CG_AddSaberBlade`'s draw test), for as long as the owner's current
/// `saberMove` authors, or the 40 ms `SABER_TRAIL_TIME` when it authors none.
/// It advances the same per-blade state the hilt used in the hand.
fn add_trails(
    sinks: &mut super::Sinks<'_>,
    owner_entity: u64,
    hilt: crate::saber_hilts::Hilt,
    blades: &[Option<saber::Blade>; 8],
    color: BladeColor,
) {
    let Some(edges) = sinks.trail_edges.as_mut() else {
        return;
    };
    let authored = sinks
        .world
        .entity(EntityId::new(owner_entity))
        .and_then(|owner| owner.equipment())
        .map_or(0, |equipment| equipment.trail_duration_millis);
    for (index, blade) in blades.iter().enumerate() {
        let (Some(blade), Some(hilt_blade)) = (blade, hilt.blade(index)) else {
            continue;
        };
        let Some(state) = sinks.saber_states.blade_mut(owner_entity, 0, index) else {
            continue;
        };
        if let Some(quad) = state.trail.update(
            edges.edge(*blade),
            sinks.presentation_time,
            authored,
            hilt_blade.trail_style,
            color,
            true,
        ) {
            sinks.saber_segments.insert(quad, sinks.presentation_time);
        }
    }
}

/// Emit one hilt at `transform` and each lit primary blade at `rotation` in `color`
/// (the owner's skin over its saber colour).
#[allow(clippy::too_many_arguments)]
pub(super) fn submit_hilt(
    owner: LegacyThrownSaber<'_>,
    color: BladeColor,
    transform: Transform,
    rotation: Quat,
    catalog: &HiltCatalog,
    objects: &mut [Vec<ActorInstance>],
    blades: &mut Vec<Instance>,
    presentation_time: i64,
) -> Option<(crate::saber_hilts::Hilt, [Option<saber::Blade>; 8])> {
    // CG_Player owns manual rendering only while saberInFlight (10050-10055);
    // CG_General ignores the lingering modelGhoul2=127 entity (cg_ents.c:871).
    if !owner.in_flight {
        return None;
    }
    let hilt = catalog.get(owner.name);
    objects[hilt.mesh_index].push(ActorInstance::new(
        transform.translation,
        transform.rotation,
        transform.scale,
    ));
    let origin = Vec3::from_array(transform.translation);
    let owner_entity = u64::from(owner.owner) + 1;
    let mut light_blades = [None; 8];
    for index in 0..usize::from(hilt.num_blades) {
        if index > 0 && !owner.extra_blades {
            break;
        }
        let Some(blade) = hilt.blade(index) else {
            continue;
        };
        let blade = saber::world_blade(origin, rotation, blade.socket, blade.length, blade.radius);
        // The seed of the same blade in the hand, so its animation carries on in flight.
        let key = owner_entity.wrapping_mul(24) + index as u64;
        blades.extend(Instance::pair(blade, color).map(|i| {
            i.with_contact(owner_entity, 2, index, !hilt.no_wall_marks, hilt.no_dlight)
                .with_animation(presentation_time as f64 * 0.001, key as u32)
        }));
        blades.extend(saber::skin_extras(
            blade,
            color,
            presentation_time as f64 * 0.001,
            key as u32,
        ));
        light_blades[index] = Some(blade);
    }
    Some((hilt, light_blades))
}
