//! Optional third-person ground HUD: health, armor and Force values placed
//! around the local player’s feet. Controlled by `cg_groundHud`.

mod gpu;
pub(crate) mod layout;
pub(crate) mod readout;

use crate::GpuState;
use crate::camera_uniform::CameraUniform;
use crate::hud::HudVisibility;
use glam::Vec3;
use layout::Layout;
use readout::{Conditions, Presentation, Readings};

/// Codemp `MINS_Z` (`bg_public.h`): the player box's floor below its origin.
const MINS_Z: f32 = -24.0;
/// The cvar that turns the ground HUD on.
pub(crate) const CVAR: &str = "cg_groundHud";

/// Retained ground-HUD state: GPU resources and this frame's decision.
pub(crate) struct GroundHud {
    renderer: gpu::Renderer,
    view: gpu::View,
    active: bool,
}

impl GroundHud {
    /// Build the pipeline for the scene colour target `format`, sampling the
    /// UI font atlas through bind groups of `text_layout`.
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        text_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        Self {
            renderer: gpu::Renderer::new(device, format, text_layout),
            view: gpu::View::default(),
            active: false,
        }
    }

    /// Retain the final main view; called with every scene-camera upload.
    pub(crate) fn set_view(&mut self, camera: &CameraUniform) {
        self.view = gpu::View {
            view_projection: glam::Mat4::from_cols_array_2d(&camera.view_projection),
            camera: Vec3::from_array(camera.camera_position),
            forward: Vec3::from_array(camera.view_forward),
        };
    }

    /// Draw in the overlay pass that holds scene depth read-only; `text` is
    /// the modern UI font atlas bind group (`GpuState::text_bind_group`).
    pub(crate) fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        depth: &'a wgpu::BindGroup,
        text: &'a wgpu::BindGroup,
    ) {
        if self.active {
            self.renderer.draw(pass, depth, text);
        }
    }
}

/// Decide this frame's HUD visibility and prepare the ground HUD.
///
/// Returns the normal HUD's visibility with its status widgets switched off
/// while the ground HUD stands in for them. `time_ms` picks the demo snapshot
/// when no live session runs.
pub(crate) fn frame(gpu: &mut GpuState, intermission: bool, time_ms: i32) -> HudVisibility {
    let mut visibility = if intermission
        || crate::menu_backdrop::hides_hud(gpu)
        || gpu.quick_wheel.hides_hud()
        || crate::scoreboard::hides_hud(gpu)
    {
        HudVisibility::HIDDEN
    } else {
        HudVisibility::from_console(gpu.console.as_ref())
    };
    let enabled = gpu
        .console
        .as_ref()
        .and_then(|c| c.bool_cvar(CVAR))
        .unwrap_or(false);
    let sample = enabled.then(|| sample(gpu, time_ms)).flatten();
    let presentation = sample.map_or(Presentation::Normal, |(ref conditions, ..)| {
        readout::select(Conditions {
            enabled,
            third_person: gpu.third_person && !gpu.detached_camera && gpu.scope.mode == 0,
            status_visible: visibility.status,
            intermission,
            ..*conditions
        })
    });
    match (presentation, sample) {
        (Presentation::Ground, Some((_, placement, readings))) => {
            visibility.ground_hud = true;
            let viewport = [
                gpu.configuration.width as f32,
                gpu.configuration.height as f32,
            ];
            let layout = Layout::build(&gpu.ui_font, readings);
            let hud = &mut gpu.ground_hud;
            hud.active = true;
            let placement = gpu::Placement {
                viewport,
                ..placement
            };
            hud.renderer
                .upload(&gpu.queue, hud.view, placement, &layout);
        }
        _ => gpu.ground_hud.active = false,
    }
    visibility
}

/// The local player's state this frame: live prediction where it runs, the
/// presented demo snapshot otherwise.
fn sample(gpu: &GpuState, time_ms: i32) -> Option<(Conditions, gpu::Placement, Readings)> {
    let player = match (&gpu.live_session, &gpu.demo_session) {
        (Some(session), _) => &session.latest_snapshot().player,
        (None, Some(session)) => &session.snapshot_at_or_before(time_ms).player,
        (None, None) => return None,
    };
    let predicted = gpu
        .live_session
        .as_ref()
        .and(gpu.local_prediction.predicted_state());
    let conditions = Conditions {
        in_game: true,
        movement_type: predicted.map_or(player.movement_type(), |p| p.movement_type),
        spectator: player.is_spectator(),
        following: !crate::local_prediction::predicts_local_view(player.movement_flags()),
        health: predicted.map_or(player.health(), |p| p.health),
        ..Conditions::default()
    };
    // The model root stands on the predicted origin; the camera's eye sits
    // `view_height` above it when nothing is predicted (demo playback).
    let (origin, yaw) = predicted.map_or_else(
        || {
            (
                gpu.camera_position - Vec3::Z * gpu.local_prediction.view_height(),
                gpu.camera_yaw,
            )
        },
        |p| (Vec3::from_array(p.origin), p.view_angles[1].to_radians()),
    );
    let placement = gpu::Placement {
        feet: origin + Vec3::Z * MINS_Z,
        yaw,
        viewport: [1.0; 2],
    };
    let readings = Readings::new(
        conditions.health,
        player.armor(),
        player.max_health(),
        i32::from(predicted.map_or(player.force_power(), |p| p.force_power)),
        predicted.map_or(player.weapon(), |p| p.weapon),
        player.saber_draw_style(),
    );
    Some((conditions, placement, readings))
}
