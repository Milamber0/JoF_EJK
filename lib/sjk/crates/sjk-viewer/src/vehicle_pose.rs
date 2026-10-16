//! Where a vehicle's model points and where its rider sits.
//!
//! `CG_G2PlayerAngles` (`cg_players.c:5120-5160`) gives a fighter its full angles, a
//! speeder and a walker yaw and roll, and everything else yaw alone. A rider takes the
//! vehicle's yaw and stands at its `*driver` bolt, read with the vehicle's yaw alone
//! (`cg_players.c:10425-10470`, `AttachRidersGeneric` in `bg_vehicleLoad.c`).
use crate::{actor_mesh::ActorMesh, vehicle_assets::VehicleKind, weapon_view};
use glam::Vec3;
use sjk_runtime::{EntityId, World};

/// `CLASS_VEHICLE` (`teams.h`).
const CLASS_VEHICLE: u8 = 53;

/// The bolt a vehicle's pilot is attached to.
const DRIVER_BOLT: &str = "*driver";

/// The locally piloted vehicle at the same predicted command as its pilot.
/// Remote vehicles and demo playback retain their interpolated world transforms.
#[derive(Clone, Copy)]
pub(crate) struct Predicted {
    pub(crate) number: u16,
    pub(crate) origin: [f32; 3],
    pub(crate) angles: [f32; 3],
}

impl Predicted {
    fn for_entity(self, id: EntityId) -> Option<Self> {
        (id.get() == u64::from(self.number) + 1).then_some(self)
    }
}

/// The driver bolt's origin in the space of a vehicle posed as `matrices`; `None` for
/// anything that is not a vehicle.
pub(crate) fn driver_seat(
    preview: &crate::PlayerPreview,
    matrices: &[[[f32; 4]; 3]],
) -> Option<[f32; 3]> {
    preview.vehicle?;
    let bolt = preview
        .mesh
        .surface_bolt_matrix(DRIVER_BOLT, 0, matrices)
        .ok()??;
    Some(crate::bolt::column(&bolt, 3))
}

/// The root rotation of a vehicle of `kind` whose entity angles are `angles` (pitch, yaw,
/// roll in degrees).
pub(crate) fn root_rotation(kind: VehicleKind, angles: [f32; 3]) -> [f32; 4] {
    sjk_client::legacy_angles_to_quaternion(match kind {
        VehicleKind::Fighter => angles,
        VehicleKind::Speeder | VehicleKind::Walker => [0.0, angles[1], angles[2]],
        VehicleKind::Upright => [0.0, angles[1], 0.0],
    })
}

/// A rider's place: the world position of the vehicle's driver bolt and the vehicle's yaw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Seat {
    pub(crate) translation: [f32; 3],
    pub(crate) yaw_degrees: f32,
}

/// The seat on vehicle entity `number`, drawn at `scale`, if that vehicle is posed this
/// frame and has one.
pub(crate) fn seat(
    world: &World,
    meshes: &[ActorMesh],
    number: u16,
    scale: f32,
    time: i64,
    predicted: Option<Predicted>,
) -> Option<Seat> {
    let id = EntityId::new(u64::from(number) + 1);
    let entity = world.entity(id)?;
    let mesh = meshes.iter().find(|mesh| mesh.entity_id == Some(id))?;
    let (origin, yaw_degrees) = match predicted.and_then(|p| p.for_entity(id)) {
        Some(p) => (p.origin, p.angles[1]),
        None => (
            entity.sample(time).translation,
            entity.sample_pose(time)?.view_angles_degrees[1],
        ),
    };
    let upright = sjk_client::legacy_angles_to_quaternion([0.0, yaw_degrees, 0.0]);
    let offset = Vec3::from_array(mesh.driver_seat?) * scale;
    Some(Seat {
        translation: (Vec3::from_array(origin)
            + weapon_view::actor_world_rotation(upright) * offset)
            .to_array(),
        yaw_degrees,
    })
}

/// Tilt `transform` if `entity` is a vehicle; seat it if it rides one.
///
/// `state` is the entity's snapshot state, absent for the local player, whose own player
/// state (`local`) rides for it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn place(
    world: &World,
    meshes: &[ActorMesh],
    mesh: Option<usize>,
    entity: &sjk_runtime::SceneEntity,
    transform: &mut sjk_runtime::Transform,
    snapshot: Option<&sjk_protocol::Snapshot>,
    state: Option<&sjk_protocol::EntityState>,
    local: bool,
    time: i64,
    predicted: Option<Predicted>,
) {
    if let Some(kind) = mesh.and_then(|index| meshes[index].preview.vehicle) {
        if let Some(p) = predicted.and_then(|p| p.for_entity(entity.id)) {
            // CG_AddPacketEntities + CG_CalcEntityLerpPositions bypass snapshot
            // interpolation for the vehicle this client predicts and pilots.
            transform.translation = p.origin;
            transform.rotation = root_rotation(kind, p.angles);
        } else if let Some(pose) = entity.sample_pose(time) {
            transform.rotation = root_rotation(kind, pose.view_angles_degrees);
        }
    }
    let Some(snapshot) = snapshot else { return };
    let riding = match state {
        _ if local && predicted.is_some() => predicted.unwrap().number,
        Some(state) if state.npc_class() != CLASS_VEHICLE => state.vehicle_entity_num(),
        None if local => snapshot.player.vehicle_entity_num(),
        _ => 0,
    };
    if riding == 0 {
        return;
    }
    // iModelScale: a percentage, zero meaning unscaled.
    let scale = snapshot
        .entities
        .iter()
        .find(|state| state.number() == riding)
        .and_then(|state| state.raw_field(123))
        .filter(|&percent| percent != 0)
        .map_or(1.0, |percent| percent as i32 as f32 / 100.0);
    if let Some(seat) = seat(world, meshes, riding, scale, time, predicted) {
        // The forced-angle root keeps the rider's own roll (`bg_pmove.c:12901-12910`).
        let roll = mesh.map_or(0.0, |index| {
            meshes[index].angle_controller.root_roll_degrees()
        });
        transform.translation = seat.translation;
        transform.rotation = sjk_client::legacy_angles_to_quaternion([0.0, seat.yaw_degrees, roll]);
    }
}
