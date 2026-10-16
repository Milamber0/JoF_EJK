//! Per-frame extrapolation of the local prediction between network packets.
//!
//! The viewer makes a user command every gameplay FPS step, 8 ms at its
//! default 125 (`command_rate`); the stock
//! client predicts through a fresh usercmd every rendered frame
//! (`cl_input.cpp` `CL_CreateNewCommands`, `cg_predict.c:1124-1237`). Without
//! this the presented origin holds still for up to a command interval and then
//! steps, which reads as movement jitter at frame rates above the gameplay FPS.

#[path = "interpolated_view.rs"]
pub(crate) mod interpolated;

use super::GpuState;
use std::time::Instant;

/// Present the committed prediction advanced through this frame's input.
pub(crate) fn tick(
    gpu: &mut GpuState,
    audio: &mut Option<super::GameAudio>,
    now: Instant,
    presentation_time: i32,
) {
    if !gpu.live_presentation_ready() {
        return;
    }
    let Some(session) = gpu.live_session.as_ref() else {
        return;
    };
    if gpu.local_prediction.predicted_state().is_none() {
        interpolated::present(gpu, presentation_time);
        return;
    }
    let snapshot = session.latest_snapshot();
    gpu.gameplay_input
        .selection
        .sync(&snapshot.player, presentation_time);
    let mut command = gpu.gameplay_input.user_command(
        gpu.server_clock.server_time(now),
        gpu.camera_pitch,
        gpu.camera_yaw,
        gpu.gameplay_input
            .view_authority
            .command_delta()
            .unwrap_or(snapshot.player.delta_angles()),
        gpu.selected_weapon
            .unwrap_or_else(|| snapshot.player.weapon()),
        snapshot.player.selected_force_power(),
        0,
    );
    command.buttons = sjk_game_jka::pmove_talk::command_buttons(command.buttons, gpu.talk_button());
    gpu.local_prediction
        .preview_command(command, &gpu.bsp, &mut gpu.trace_scratch);
    let presented_snapshot = session.snapshot_at_or_before(presentation_time);
    if let Some(position) = gpu
        .local_prediction
        .present_frame(presented_snapshot, presentation_time)
    {
        gpu.camera_position = position;
    }
    if let Some(predicted) = gpu.local_prediction.predicted_state() {
        gpu.gameplay_input.view_authority.observe_prediction(
            predicted,
            &mut gpu.camera_pitch,
            &mut gpu.camera_yaw,
        );
    }
    super::first_person_view::deliver_events(gpu, audio, presentation_time);
}

/// Fold the view's authoritative `delta_angles` into the camera once a snapshot is
/// presented: the predicted state's while the local view is predicted (stock renders
/// `cg.predictedPlayerState.viewangles`, `cg_view.c:1520,1586`, whose delta
/// `CG_PredictPlayerState` replays with the pending commands, `cg_predict.c:1281`), else the
/// snapshot's. `local_view` false (following, intermission) drops the baseline.
pub(crate) fn observe_view(
    gpu: &mut GpuState,
    player: &sjk_protocol::PlayerState,
    local_view: bool,
) {
    let (client, delta, angles) = match gpu
        .local_prediction
        .predicted_state()
        .filter(|_| local_view)
    {
        Some(predicted) => (
            i32::from(predicted.client_num),
            predicted.delta_angles,
            predicted.view_angles,
        ),
        None => (
            i32::from(player.client_num()),
            player.delta_angles(),
            player.view_angles(),
        ),
    };
    gpu.gameplay_input.view_authority.observe(
        client,
        delta,
        angles,
        local_view,
        &mut gpu.camera_pitch,
        &mut gpu.camera_yaw,
    );
}
