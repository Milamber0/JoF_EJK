//! Feeds the vehicle HUD ([`super::vehicle`]) from the presented snapshot and the ride
//! prediction, and loads the vehicle's crosshair picture.

use super::vehicle::Sample;
use crate::GpuState;
use sjk_runtime::EntityId;
use sjk_ui::TextureId;

/// Refresh the vehicle HUD for `time` (the presentation time): the live session's newest
/// snapshot, or the demo snapshot being shown. Allocation-free except when the riding
/// vehicle changes and its crosshair picture is decoded.
pub(crate) fn sample(gpu: &mut GpuState, time: i32) {
    let snapshot = match (&gpu.live_session, &gpu.demo_session) {
        (Some(session), _) => session.latest_snapshot(),
        (None, Some(session)) => session.snapshot_at_or_before(time),
        (None, None) => {
            gpu.hud.vehicle.clear();
            return;
        }
    };
    let player = &snapshot.player;
    let number = player.vehicle_entity_num();
    // The vehicle's player state goes to its pilot alone.
    let vehicle = snapshot.vehicle_player.as_ref();
    let profile = (number != 0)
        .then(|| {
            let id = EntityId::new(u64::from(number) + 1);
            gpu.actor_meshes
                .iter()
                .find(|mesh| mesh.entity_id == Some(id))
                .and_then(|mesh| mesh.preview.vehicle_hud.as_ref())
        })
        .flatten();
    // Prediction runs only for a live session; a demo shows the snapshot's values.
    let ride = gpu
        .live_session
        .as_ref()
        .and(vehicle)
        .and_then(|_| gpu.local_prediction.vehicle_readout(number));
    let sample = Sample {
        number,
        piloted: vehicle.is_some(),
        profile,
        alive: player.health() > 0 && !player.is_spectator() && player.team() != 3,
        hull: vehicle.map_or(0, |state| state.health()),
        shield: vehicle.map_or(0, |state| state.armor()),
        speed: ride.map_or_else(
            || vehicle.map_or(0.0, |state| state.speed()),
            |(speed, _)| speed,
        ),
        ammo: [0, 1].map(|slot| {
            vehicle
                .and_then(|state| state.ammo_value(slot))
                .unwrap_or(0)
        }),
        linked: vehicle.is_some_and(|state| state.vehicle_fields().weapons_linked),
        turbo_time: ride.and_then(|(_, turbo)| turbo),
        time,
        dynamic_crosshair: gpu
            .console
            .as_ref()
            .and_then(|console| console.integer_cvar("cg_dynamiccrosshair"))
            .unwrap_or(1),
    };
    gpu.hud.vehicle.update(&sample);
    if let Some(vfs) = &gpu.vfs {
        let (shaders, renderer, queue) = (&gpu.shaders, &gpu.ui_shapes, &gpu.queue);
        gpu.hud.vehicle.ensure_picture(|name| {
            let pixels = super::icons::assets::decode(vfs, shaders, name)?;
            let texture = TextureId(crate::ui_renderer::VEHICLE_CROSSHAIR_ICON);
            renderer.upload_icon(queue, texture, pixels.as_raw());
            Some(texture)
        });
    }
}
