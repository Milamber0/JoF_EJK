//! Drop shadows under actors, as `CG_PlayerShadow` draws them.
//!
//! `cg_players.c:4649-4693` (cg_shadows 1): trace a 30x30x2 box down
//! `SHADOW_DISTANCE` from the actor origin through `MASK_PLAYERSOLID`; no
//! shadow when nothing is hit or the start is solid. The mark is a
//! temporary `CG_ImpactMark` of radius 24 on the contact plane, rotated by
//! the legs yaw, with rgb `1 - fraction` and alpha 1. `markShadow` uses
//! `rgbGen identity` / `alphaGen vertex`, so only the alpha reaches the
//! screen and the height fade never shows; the request carries white to
//! match.

use super::*;
use crate::decal_marks::DecalRequest;
use crate::decal_store::DecalStore;
use sjk_runtime::{EntityKind, World};

/// `SHADOW_DISTANCE` (`cg_players.c:4648`).
const SHADOW_DISTANCE: f32 = 128.0;
/// Radius passed to `CG_ImpactMark` for ordinary players.
const RADIUS: f32 = 24.0;
/// `cgs.media.shadowMarkShader`; the atlas keys shaders in lower case.
pub(crate) const SHADER: &str = "markshadow";
/// `MASK_PLAYERSOLID` (`bg_public.h:1226`): solid, player clip, body, terrain.
const MASK_PLAYERSOLID: u32 = 0x0000_0001 | 0x0000_0010 | 0x0000_0100 | 0x0000_1000;

/// Load-time state so per-frame requests stay allocation-free.
pub(crate) struct State {
    shader: Arc<str>,
    bounds: Aabb,
    /// `cg_shadows 0`: `CG_PlayerShadow` draws nothing
    /// (`codemp/cgame/cg_players.c:4657`).
    enabled: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            shader: Arc::from(SHADER),
            bounds: Aabb::new([-15.0, -15.0, 0.0], [15.0, 15.0, 2.0])
                .expect("player shadow bounds are well-formed"),
            enabled: true,
        }
    }
}

impl State {
    /// Latch `cg_shadows` (retail default 1).
    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

/// Per-frame inputs decoupled from the frame loop's other borrows.
pub(crate) struct Inputs<'a> {
    pub(crate) world: &'a World,
    pub(crate) presentation_time: i64,
    pub(crate) local_entity: Option<u64>,
    /// Predicted root of the local actor, whose transform the camera owns.
    pub(crate) local_root: [f32; 3],
    pub(crate) bsp: &'a Bsp,
    pub(crate) scratch: &'a mut TraceScratch,
    pub(crate) decals: &'a mut DecalStore,
}

/// Queue one temporary shadow mark per shadow-casting actor. `legs_yaw`
/// supplies the evaluated legs yaw for entities with a posed mesh; the
/// snapshot rotation stands in otherwise. Returns the number of marks queued.
pub(crate) fn request_all(
    state: &State,
    inputs: Inputs<'_>,
    legs_yaw: impl Fn(u64) -> Option<f32>,
) -> usize {
    let mut requested = 0;
    if !state.enabled {
        return requested;
    }
    for entity in inputs.world.entities() {
        if entity.kind != EntityKind::Actor || !entity.ground_shadow() {
            continue;
        }
        let transform = entity.sample(inputs.presentation_time);
        let id = entity.id.get();
        let origin = if Some(id) == inputs.local_entity {
            Vec3::from_array(inputs.local_root)
        } else {
            Vec3::from_array(transform.translation)
        };
        let yaw = legs_yaw(id).unwrap_or_else(|| yaw_degrees(transform.rotation));
        if request(
            state,
            inputs.bsp,
            inputs.scratch,
            inputs.decals,
            origin,
            yaw,
        ) {
            requested += 1;
        }
    }
    requested
}

/// Legs yaw of the posed mesh bound to an entity, for `request_all`.
pub(crate) fn mesh_legs_yaw(meshes: &[ActorMesh]) -> impl Fn(u64) -> Option<f32> + '_ {
    move |id| {
        meshes
            .iter()
            .find(|mesh| mesh.entity_id.map(|entity| entity.get()) == Some(id))
            .and_then(|mesh| mesh.render_yaw_degrees)
    }
}

/// Trace to the ground and queue the mark; `false` when there is no contact.
pub(crate) fn request(
    state: &State,
    bsp: &Bsp,
    scratch: &mut TraceScratch,
    decals: &mut DecalStore,
    origin: Vec3,
    legs_yaw_degrees: f32,
) -> bool {
    let trace = bsp.trace_box_with(
        scratch,
        origin.to_array(),
        (origin - Vec3::Z * SHADOW_DISTANCE).to_array(),
        state.bounds,
        MASK_PLAYERSOLID,
    );
    if trace.fraction >= 1.0 || trace.start_solid || trace.all_solid {
        return false;
    }
    let normal = Vec3::from_array(trace.plane.map_or([0.0, 0.0, 1.0], |plane| plane.normal));
    decals.request_temporary(DecalRequest {
        origin: Vec3::from_array(trace.end_position),
        direction: normal,
        orientation: legs_yaw_degrees,
        color: [1.0; 4],
        radius: RADIUS,
        shader: Arc::clone(&state.shader),
    });
    true
}

/// Yaw of a root rotation about +Z, in degrees.
fn yaw_degrees(rotation: [f32; 4]) -> f32 {
    let forward = Quat::from_array(rotation) * Vec3::X;
    forward.y.atan2(forward.x).to_degrees()
}
