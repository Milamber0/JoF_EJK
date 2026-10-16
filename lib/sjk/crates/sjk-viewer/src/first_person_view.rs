//! First-person eye placement: `CG_OffsetFirstPersonView` applied to the
//! render camera only. The bob, duck, landing and stair timelines perturb
//! `cg.refdef` (`cg_view.c:923-1055`), never the angles the client sends, so
//! `camera_position`/`camera_yaw`/`camera_pitch` stay the unbobbed input
//! view and this module derives the presented eye from them each frame. The
//! policy lives in `sjk_client::view_bob`; this is the renderer glue.

use super::*;
use sjk_client::pmove::MovementState;
use sjk_client::{LegacyFirstPersonView, LegacyViewPlayer};
use sjk_protocol::Snapshot;

/// The timelines plus the snapshot they were last advanced by.
#[derive(Default)]
pub(crate) struct Tracker {
    view: LegacyFirstPersonView,
    observed_server_time: Option<i32>,
}

impl Tracker {
    /// Advance the timelines by a snapshot; repeats of the same server time
    /// (demo playback re-samples the current snapshot every frame) are
    /// ignored. `predicting` is false for demos and spectators.
    pub(crate) fn observe(&mut self, snapshot: &Snapshot, predicting: bool) {
        if self.observed_server_time == Some(snapshot.server_time) {
            return;
        }
        self.observed_server_time = Some(snapshot.server_time);
        self.view
            .observe(snapshot, snapshot.server_time, predicting);
    }
}

/// The presented first-person camera for one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Camera {
    pub(crate) position: Vec3,
    /// Radians; pitch positive looking up, roll positive tilting clockwise
    /// (`AngleVectors`: positive roll leans the up vector towards right).
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) roll: f32,
    /// `cg.landChange` at this frame, for the view weapon's quarter dip.
    pub(crate) landing: f32,
}

impl Camera {
    /// Look-at inputs: position, a target along the view and the rolled up.
    pub(crate) fn look(&self) -> (Vec3, Vec3, Vec3) {
        let forward = Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
        );
        let right = forward.cross(Vec3::Z).normalize_or(Vec3::NEG_Y);
        let up = Vec3::Z * self.roll.cos() + right * self.roll.sin();
        (self.position, self.position + forward * 256.0, up)
    }

    /// The view the hand rig is placed at (`cg.refdef.vieworg`).
    pub(crate) fn view(&self) -> first_person_weapon::View {
        first_person_weapon::View {
            origin: self.position,
            yaw: self.yaw,
            pitch: self.pitch,
        }
    }
}

/// Resolve this frame's first-person camera from the unbobbed camera fields,
/// or `None` outside sessions and during intermission.
pub(crate) fn camera(state: &GpuState, presentation_time: i32) -> Option<Camera> {
    if state.free_camera_active() {
        return None;
    }
    let snapshot = presented_snapshot(
        state.live_session.as_ref(),
        state.demo_session.as_ref(),
        presentation_time,
    )?;
    // Live play reads the predicted state like `cg.predictedPlayerState`;
    let player = match (
        state.local_prediction.predicted_state(),
        state.live_session.is_some(),
    ) {
        (Some(predicted), true) => from_movement_state(predicted),
        _ => LegacyViewPlayer::from_player_state(&snapshot.player),
    };
    let tracker = &state.first_person_view;
    let offset = tracker.view.offset(
        &player,
        presentation_time,
        crate::cgame_options::bob(state.console.as_ref()),
    )?;
    let [mut pitch, yaw, mut roll] = offset.angle_delta;
    // The damage view kick joins the bob angles (`cg_view.c:967-981`).
    if let Some([kick_pitch, kick_roll]) = state.damage_feedback.view_kick(presentation_time) {
        pitch += kick_pitch;
        roll += kick_roll;
    }
    Some(Camera {
        position: state.camera_position + Vec3::Z * (offset.height - player.view_height as f32),
        yaw: state.camera_yaw + yaw.to_radians(),
        pitch: state.camera_pitch - pitch.to_radians(),
        roll: roll.to_radians(),
        landing: tracker.view.landing_offset(presentation_time),
    })
}

/// The snapshot the live or demo session presents at `presentation_time`.
pub(crate) fn presented_snapshot<'a>(
    live: Option<&'a ClientSession>,
    demo: Option<&'a demo_playback::Session>,
    presentation_time: i32,
) -> Option<&'a Snapshot> {
    live.map(|session| session.snapshot_at_or_before(presentation_time))
        .or_else(|| demo.map(|session| session.snapshot_at_or_before(presentation_time)))
}

fn from_movement_state(state: &MovementState) -> LegacyViewPlayer {
    LegacyViewPlayer {
        movement_type: state.movement_type,
        movement_flags: state.movement_flags,
        velocity: state.velocity,
        view_angles: state.view_angles,
        view_height: state.view_height,
        bob_cycle: state.bob_cycle,
    }
}

/// CG_TransitionPlayerState after prediction (cg_predict.c:1429), once per frame.
pub(crate) fn deliver_events(state: &mut GpuState, audio: &mut Option<GameAudio>, time: i32) {
    let Some(session) = &state.live_session else {
        return;
    };
    let snapshot = session.latest_snapshot();
    if let Some(predicted) = state.local_prediction.predicted_state() {
        let view_height = predicted.view_height;
        state
            .first_person_view
            .view
            .observe_predicted_view_height(view_height, time);
    }
    let mode = state
        .console
        .as_ref()
        .and_then(|c| c.integer_cvar("cg_autoswitch"))
        .unwrap_or(1);
    let mut selected = None;
    state.local_prediction.drain_events(|event| {
        selected = state
            .auto_switch
            .predicted(event, mode, snapshot)
            .or(selected);
        state
            .first_person_view
            .view
            .observe_predicted_event(event, time, &snapshot.player);
        // The muzzle flash starts with the predicted shot, as its sound does.
        state
            .muzzle_effects
            .observe_predicted(event.client, event.sequence, event.event, time);
        if let Some(audio) = audio {
            audio.observe_predicted_event(event, snapshot);
        }
    });
    if let Some(weapon) = selected {
        state.choose_auto_weapon(weapon);
    }
}
