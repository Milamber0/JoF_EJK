//! Multiplayer zoom presentation (`cg_view.c:1140-1280`), independent of the wire codec.
use crate::{GameAudio, GpuState};

#[path = "scope_gpu.rs"]
mod gpu;
pub(crate) use gpu::Mask;

#[derive(Clone, Copy)]
/// Render-clock zoom state; not weapon prediction or protocol storage.
pub(crate) struct Zoom {
    fov: f32,
    time: Option<i32>,
    sound_time: i32,
    /// Stock zoomFov/cgFov multiplier, or one outside zoom.
    pub(crate) sensitivity: f32,
    /// Current zoom mode for suppression of the ordinary crosshair.
    pub(crate) mode: u8,
}

impl Default for Zoom {
    fn default() -> Self {
        Self {
            fov: 80.0,
            time: None,
            sound_time: 0,
            sensitivity: 1.0,
            mode: 0,
        }
    }
}

impl Zoom {
    /// Return horizontal FOV and the stock 300 ms zoom sound pulse.
    pub(crate) fn frame(
        &mut self,
        time: i32,
        base: f32,
        mode: u8,
        locked: bool,
        zoom_time: i32,
        saved_fov: f32,
    ) -> (f32, bool) {
        let elapsed = self.time.map_or(0, |last| time.saturating_sub(last).max(0));
        if self.time.is_some_and(|last| time < last) {
            *self = Self::default();
        }
        self.time = Some(time);
        self.mode = mode;
        let base = base.clamp(1.0, 130.0);
        let mut sound = false;
        let horizontal = if mode == 2 {
            if self.fov > 40.0 {
                self.fov -= elapsed as f32 * 0.075;
                if self.fov < 40.0 {
                    self.fov = 40.0;
                } else if self.fov > base {
                    self.fov = base;
                }
            }
            self.fov
        } else if mode != 0 {
            if !locked {
                self.fov = self.fov.min(50.0) - elapsed as f32 * 0.035;
                if self.fov < 3.0 {
                    self.fov = 3.0;
                } else if self.fov > base {
                    self.fov = base;
                } else if self.sound_time < time || self.sound_time > time.saturating_add(10000) {
                    sound = true;
                    self.sound_time = time.saturating_add(300);
                }
            }
            if self.fov < 3.0 {
                self.fov = 50.0;
            }
            self.fov
        } else {
            self.fov = 80.0;
            let fraction = time.saturating_sub(zoom_time) as f32 / 100.0;
            if fraction <= 1.0 {
                saved_fov + fraction * (base - saved_fov)
            } else {
                base
            }
        };
        self.sensitivity = if mode != 0 { self.fov / base } else { 1.0 };
        (horizontal, sound)
    }
}

/// Widescreen horizontal FOV per `cg_fovAspectAdjust` (`cgame/cg_view.c:1241-1249`,
/// LordHavoc's Darkplaces formula with a 3:4 base aspect).
pub(crate) fn aspect_adjusted_fov(horizontal_degrees: f32, aspect: f32) -> f32 {
    let half = (horizontal_degrees.to_radians() * 0.5).tan() * 0.75 * aspect;
    half.atan().to_degrees() * 2.0
}

/// The player-state fields `CG_DrawActiveFrame` reads to decide whether a zoom
/// forces first person (`cg_view.c:3101-3131`, EternalJK).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ZoomView {
    pub(crate) health: i32,
    pub(crate) zoom_mode: u8,
    pub(crate) weapon: u8,
    pub(crate) emplaced_index: u16,
    pub(crate) vehicle_entity_num: u16,
}

impl ZoomView {
    const WP_MELEE: u8 = 2;
    const WP_SABER: u8 = 3;
    const WP_EMPLACED_GUN: u8 = 17;

    /// The view of a snapshot's (or a demo's recorded) player state.
    pub(crate) fn from_player(player: &sjk_protocol::PlayerState) -> Self {
        Self {
            health: player.health(),
            zoom_mode: player.zoom_mode(),
            weapon: player.weapon(),
            emplaced_index: player.emplaced_index(),
            vehicle_entity_num: player.vehicle_entity_num(),
        }
    }

    /// The view of the locally predicted player state (`cg.predictedPlayerState`).
    pub(crate) fn from_predicted(state: &sjk_client::pmove::MovementState) -> Self {
        Self {
            health: state.health,
            zoom_mode: state.zoom_mode,
            weapon: state.weapon,
            emplaced_index: state.emplaced_index,
            vehicle_entity_num: state.vehicle_entity_num,
        }
    }

    /// Whether the zoom forces first person, in EternalJK's order: a living player on
    /// an emplaced gun is third person whatever the zoom; a saber or melee zoom (the
    /// binoculars) is first person; riding a vehicle is third person; any other zoom
    /// (the disruptor) is first person. SJK does not force third person for the
    /// emplaced gun and vehicles (or for the knockdown, grapple and fall states that
    /// also force it in EternalJK), so there the zoom leaves the player's choice alone.
    pub(crate) fn forces_first_person(self) -> bool {
        if self.zoom_mode == 0 || self.health <= 0 {
            return false;
        }
        if self.weapon == Self::WP_EMPLACED_GUN && self.emplaced_index != 0 {
            return false;
        }
        if matches!(self.weapon, Self::WP_MELEE | Self::WP_SABER) {
            return true;
        }
        self.vehicle_entity_num == 0
    }
}

/// The third-person state the frame renders: the player's choice, unless a zoom
/// forces first person this frame. A pure function of this frame's inputs.
pub(crate) fn rendering_third_person(choice: bool, zoom_first_person: bool) -> bool {
    choice && !zoom_first_person
}
impl GpuState {
    /// Derives this frame's `third_person` from the player's camera choice
    /// (`third_person_choice`) and the zoom: nothing is remembered between frames, so
    /// the choice cannot be lost or left behind when the zoom ends, a demo or a
    /// session changes, or the player follows someone else.
    ///
    /// Call it once a frame before anything reads `third_person`.
    pub(crate) fn update_zoom_view(&mut self, time: i32) {
        self.zoom_first_person = self
            .zoom_view(time)
            .is_some_and(ZoomView::forces_first_person);
        self.third_person =
            rendering_third_person(self.third_person_choice, self.zoom_first_person)
                || self.free_camera_active();
        if self.live_session.is_some() {
            self.detached_camera = self.free_camera_active();
        }
    }

    /// The player state the zoom decision reads: the predicted one in a live session
    /// (the snapshot's while following another player, which bypasses prediction,
    /// `cg_predict.c:952`), the recorded one in a demo whose camera is the player's
    /// own. A demo's detached or director camera (spectate, look-at, orbit, free) is
    /// not the player's view, so the recorded player's zoom leaves it alone.
    fn zoom_view(&self, time: i32) -> Option<ZoomView> {
        if let Some(session) = &self.live_session {
            let player = &session.latest_snapshot().player;
            let predicted = crate::local_prediction::predicts_local_view(player.movement_flags())
                .then(|| self.local_prediction.predicted_state())
                .flatten();
            return Some(
                predicted.map_or_else(|| ZoomView::from_player(player), ZoomView::from_predicted),
            );
        }
        let session = self.demo_session.as_ref()?;
        session
            .camera()
            .follows_player_view()
            .then(|| ZoomView::from_player(&session.snapshot_at_or_before(time).player))
    }

    /// Calculate the legacy horizontal zoom before converting to the renderer's vertical FOV.
    pub(crate) fn scope_fov(&mut self, time: i32, audio: &mut Option<GameAudio>) -> f32 {
        let snapshot = self
            .live_session
            .as_ref()
            .map(|s| s.latest_snapshot())
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(|s| s.snapshot_at_or_before(time))
            });
        let Some(snapshot) = snapshot else {
            self.scope = Zoom::default();
            if let Some(mask) = &mut self.scope_mask {
                mask.prepare(&self.queue, false, 0, 80.0, time, 0.0, None);
            }
            return self.field_of_view;
        };
        let player = &snapshot.player;
        // Demo playback keeps the initial predictor, so only a live session may
        // read zoom mode from the predicted state (stock cg.predictedPlayerState),
        // and not while following another player (`cg_predict.c:952`), the same
        // choice `update_zoom_view` makes.
        let predicted = self
            .live_session
            .as_ref()
            .filter(|_| crate::local_prediction::predicts_local_view(player.movement_flags()))
            .and_then(|_| self.local_prediction.predicted_state());
        let mode = predicted.map_or(player.zoom_mode(), |state| state.zoom_mode);
        let base = self
            .console
            .as_ref()
            .and_then(|c| c.float_cvar("cg_fov"))
            .unwrap_or(90.0) as f32;
        // `cg_view.c:1241-1249`: widen the horizontal FOV for widescreen. Stock
        // clamps the cvar first and lets the adjusted value exceed that clamp.
        let base = if self
            .console
            .as_ref()
            .and_then(|c| c.bool_cvar("cg_fovAspectAdjust"))
            .unwrap_or(true)
        {
            let aspect = self.configuration.width as f32 / self.configuration.height as f32;
            aspect_adjusted_fov(base.clamp(1.0, 130.0), aspect)
        } else {
            base
        };
        let (horizontal, sound) = self.scope.frame(
            time,
            base,
            mode,
            predicted.map_or(player.zoom_locked(), |state| state.zoom_locked),
            predicted.map_or(player.zoom_time(), |state| state.zoom_time),
            predicted.map_or(player.zoom_fov(), |state| state.zoom_fov),
        );
        if let Some(mask) = &mut self.scope_mask {
            let visible = mode != 0
                && mode != 2
                && !self.third_person
                && player.movement_type() != 7
                && self
                    .console
                    .as_ref()
                    .and_then(|c| c.bool_cvar("cg_draw2D"))
                    .unwrap_or(true);
            let maximum = if player.entity_flags() & (1 << 20) != 0 {
                600.0
            } else {
                300.0
            };
            let state = predicted.map_or(player.weapon_state(), |state| state.weapon_state);
            let charge_time =
                predicted.map_or(player.weapon_charge_time(), |s| s.weapon_charge_time);
            let charge = (state == 5).then(|| (time - charge_time) as f32 / 1500.0);
            mask.prepare(
                &self.queue,
                visible,
                crate::cgame_options::scope_style(self.console.as_ref()),
                self.scope.fov,
                time,
                player.ammo_value(3).unwrap_or(0) as f32 / maximum,
                charge,
            );
        }
        if sound
            && player.movement_type() != 7
            && let Some(audio) = audio
        {
            audio.play_local(
                "sound/weapons/disruptor/zoomloop.wav",
                1.0,
                sjk_audio::SourceId(1022),
                sjk_audio::ChannelId(1),
            );
        }
        let aspect = self.configuration.width as f32 / self.configuration.height as f32;
        let horizontal = if player.movement_type() == 7 {
            80.0
        } else {
            horizontal
        };
        self.field_of_view =
            (2.0 * ((horizontal.to_radians() * 0.5).tan() / aspect).atan()).to_degrees();
        self.field_of_view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WP_BRYAR_PISTOL: u8 = 4;
    const WP_DISRUPTOR: u8 = 6;

    fn zoomed(weapon: u8) -> ZoomView {
        ZoomView {
            health: 100,
            zoom_mode: 1,
            weapon,
            emplaced_index: 0,
            vehicle_entity_num: 0,
        }
    }

    #[test]
    fn a_disruptor_zoom_forces_first_person_and_no_zoom_does_not() {
        assert!(zoomed(WP_DISRUPTOR).forces_first_person());
        let unzoomed = ZoomView {
            zoom_mode: 0,
            ..zoomed(WP_DISRUPTOR)
        };
        assert!(!unzoomed.forces_first_person());
    }

    #[test]
    fn the_binoculars_zoom_with_saber_or_melee_forces_first_person() {
        assert!(zoomed(ZoomView::WP_SABER).forces_first_person());
        assert!(zoomed(ZoomView::WP_MELEE).forces_first_person());
    }

    #[test]
    fn an_emplaced_gun_stays_third_person_under_zoom() {
        // `cg_view.c:3103-3107`: tested before the zoom.
        let gunner = ZoomView {
            emplaced_index: 7,
            ..zoomed(ZoomView::WP_EMPLACED_GUN)
        };
        assert!(!gunner.forces_first_person());
        // The emplaced weapon without a gun (`emplacedIndex` 0) is an ordinary zoom.
        assert!(zoomed(ZoomView::WP_EMPLACED_GUN).forces_first_person());
    }

    #[test]
    fn a_vehicle_stays_third_person_under_zoom_unless_the_weapon_is_saber_or_melee() {
        // `cg_view.c:3108-3126`: the vehicle test comes before the plain zoom test.
        let rider = ZoomView {
            vehicle_entity_num: 12,
            ..zoomed(WP_BRYAR_PISTOL)
        };
        assert!(!rider.forces_first_person());
        let saber_rider = ZoomView {
            weapon: ZoomView::WP_SABER,
            ..rider
        };
        assert!(saber_rider.forces_first_person());
    }

    #[test]
    fn a_dead_player_is_not_forced_into_first_person() {
        let dead = ZoomView {
            health: 0,
            ..zoomed(WP_DISRUPTOR)
        };
        assert!(!dead.forces_first_person());
    }

    #[test]
    fn the_choice_survives_a_zoom_without_a_flicker() {
        // Frame by frame: the zoom comes and goes, the player never touches the camera.
        let zoom = [false, true, true, true, false, false];
        let seen: Vec<bool> = zoom
            .iter()
            .map(|&zoomed| rendering_third_person(true, zoomed))
            .collect();
        assert_eq!(seen, [true, false, false, false, true, true]);
        // A first-person player stays first person throughout.
        assert!(zoom.iter().all(|&z| !rendering_third_person(false, z)));
        // A camera toggle during the zoom takes effect when the zoom ends.
        let choice_after_toggle = false;
        assert!(!rendering_third_person(choice_after_toggle, true));
        assert!(!rendering_third_person(choice_after_toggle, false));
    }

    #[test]
    fn only_the_players_own_demo_cameras_follow_the_recorded_zoom() {
        use crate::demo_playback::Camera;
        assert!(Camera::FirstPerson.follows_player_view());
        assert!(Camera::FirstPersonPitch(10).follows_player_view());
        assert!(Camera::FollowThirdPerson.follows_player_view());
        assert!(!Camera::Spectate(3).follows_player_view());
        assert!(!Camera::LookAt(3).follows_player_view());
        assert!(!Camera::Orbit(3).follows_player_view());
        assert!(
            !Camera::Free {
                origin: [0.0; 3],
                yaw: 0.0,
                pitch: 0.0,
            }
            .follows_player_view()
        );
    }
}
