//! First-person view offsets: `CG_OffsetFirstPersonView` (`cg_view.c:923-1055`).
//!
//! The reference perturbs `cg.refdef.vieworg`/`viewangles` every frame from
//! the predicted player state: run pitch/roll from velocity, stride bob on
//! pitch, roll and height, and three short timelines — the duck height blend
//! (`cg_playerstate.c:539-542`), the landing dip (`cg_event.c:959-972`) and
//! the stair-step smoothing (`cg_event.c:1469-1502`, live prediction only).
//! [`LegacyViewBobSample`] is the shared `cg.bobcycle`/`bobfracsin`/`xyspeed`
//! triple of `CG_CalcViewValues` (`cg_view.c:1543-1551`).

use sjk_protocol::{PlayerState, Snapshot};

const PM_INTERMISSION: u8 = crate::intermission::PM_INTERMISSION;
const PMF_DUCKED: u16 = 1;
const PMF_FOLLOW: u16 = 4096;
const EV_STEP_4: u16 = 7;
const EV_STEP_16: u16 = 10;
const EV_FALL: u16 = 11;
const MAX_XY_SPEED: f32 = 270.0;
const DUCK_TIME: i32 = 100;
const LAND_DEFLECT_TIME: i32 = 150;
const LAND_RETURN_TIME: i32 = 300;
const STEP_TIME: i32 = 200;
const MAX_STEP_CHANGE: f32 = 32.0;

/// `cg.bobcycle`, `cg.bobfracsin` and `cg.xyspeed` for one player state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyViewBobSample {
    /// `(bobCycle & 128) >> 7`: which leg the stride is on.
    pub odd_leg: bool,
    /// `|sin((bobCycle & 127) / 127 * pi)|`.
    pub fraction_sin: f32,
    /// Horizontal speed capped at 270.
    pub xy_speed: f32,
}

impl LegacyViewBobSample {
    /// Sample from the networked `bobCycle` and velocity.
    pub fn new(bob_cycle: u8, velocity: [f32; 3]) -> Self {
        Self {
            odd_leg: bob_cycle & 128 != 0,
            fraction_sin: (f32::from(bob_cycle & 127) / 127.0 * std::f32::consts::PI)
                .sin()
                .abs(),
            xy_speed: (velocity[0] * velocity[0] + velocity[1] * velocity[1])
                .sqrt()
                .min(MAX_XY_SPEED),
        }
    }
}

/// The `cg_bob*` / `cg_run*` cvars (`cg_xcvar.h:42-44`, `:112-113`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyViewBobConfig {
    pub bob_pitch: f32,
    pub bob_roll: f32,
    pub bob_up: f32,
    pub run_pitch: f32,
    pub run_roll: f32,
}

impl Default for LegacyViewBobConfig {
    fn default() -> Self {
        Self {
            bob_pitch: 0.002,
            bob_roll: 0.002,
            bob_up: 0.005,
            run_pitch: 0.002,
            run_roll: 0.005,
        }
    }
}

/// The per-client timelines the offsets read (`cg.duckTime`, `cg.landTime`,
/// `cg.stepTime` and their deltas), advanced once per received snapshot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LegacyFirstPersonView {
    predicted_events: crate::predicted_events::PredictedEventLedger,
    duck: Option<(i32, f32)>,
    land: Option<(i32, f32)>,
    step: Option<(i32, f32)>,
    previous_view_height: Option<i32>,
    /// The predicted view height last seen, for [`Self::observe_predicted_view_height`].
    previous_predicted_view_height: Option<i32>,
    event_sequence: i32,
}

impl LegacyFirstPersonView {
    /// Feed the newest snapshot at `time` (`cg.time` when it arrived).
    /// `predicting` is false for demo playback and following, where steps
    /// are interpolated instead of smoothed (`cg_event.c:1482-1485`).
    pub fn observe(&mut self, snapshot: &Snapshot, time: i32, predicting: bool) {
        let player = &snapshot.player;
        self.predicted_events
            .identity(player.client_num(), player.entity_flags());
        let view_height = player.view_height();
        // With prediction the duck smoothing follows the predicted state
        // ([`Self::observe_predicted_view_height`]); the snapshot's view height
        // arrives later, and smoothing from it too lifted the view back up for
        // a moment after every crouch, as a stutter.
        if !predicting
            && let Some(previous) = self.previous_view_height
            && previous != view_height
        {
            self.duck = Some((time, (view_height - previous) as f32));
        }
        self.previous_view_height = Some(view_height);
        let sequence = player.event_sequence();
        let previous = if self.event_sequence > 65533 && sequence < 2 {
            self.event_sequence - 65536
        } else {
            self.event_sequence
        };
        for event_sequence in previous.max(sequence - 2)..sequence {
            let slot = (event_sequence & 1) as usize;
            let (Some(event), Some(parameter)) = (player.event(slot), player.event_parameter(slot))
            else {
                continue;
            };
            if self
                .predicted_events
                .accept(event_sequence as u16, event & 0xff, parameter)
            {
                self.apply_event(event & 0xff, parameter, time, predicting, player);
            }
        }
        self.event_sequence = sequence;
    }

    /// `CG_TransitionPlayerState` after prediction (`cg_predict.c:1429`,
    /// `cg_playerstate.c:539-543`): a change of the predicted view height starts the
    /// duck smoothing, at the frame it happens.
    pub fn observe_predicted_view_height(&mut self, view_height: i32, time: i32) {
        if let Some(previous) = self.previous_predicted_view_height
            && previous != view_height
        {
            self.duck = Some((time, (view_height - previous) as f32));
        }
        self.previous_predicted_view_height = Some(view_height);
    }

    /// Apply a locally predicted movement event to the same landing/step timelines.
    pub fn observe_predicted_event(
        &mut self,
        event: crate::predicted_events::PredictedEvent,
        time: i32,
        player: &PlayerState,
    ) {
        self.predicted_events
            .identity(event.client, event.entity_flags);
        if self
            .predicted_events
            .accept(event.sequence, event.event, event.parameter)
        {
            self.apply_event(event.event, event.parameter, time, true, player);
        }
    }

    fn apply_event(
        &mut self,
        event: u16,
        parameter: u16,
        time: i32,
        predicting: bool,
        player: &PlayerState,
    ) {
        match event {
            EV_FALL => {
                let change = -(f32::from(parameter)).clamp(-32.0, 32.0);
                self.land = Some((time, change));
            }
            EV_STEP_4..=EV_STEP_16 => {
                if !predicting || player.movement_flags() & PMF_FOLLOW != 0 {
                    return;
                }
                let old_step = self.step.map_or(0.0, |(step_time, change)| {
                    let delta = time - step_time;
                    if delta < STEP_TIME {
                        change * (STEP_TIME - delta) as f32 / STEP_TIME as f32
                    } else {
                        0.0
                    }
                });
                let step = 4.0 * f32::from(event - EV_STEP_4 + 1);
                self.step = Some((time, (old_step + step).min(MAX_STEP_CHANGE)));
            }
            _ => {}
        }
    }

    /// `cg.landChange` scaled by the landing timeline at `time`; the view
    /// weapon applies a quarter of it (`cg_weapons.c:242-250`).
    pub fn landing_offset(&self, time: i32) -> f32 {
        let Some((land_time, change)) = self.land else {
            return 0.0;
        };
        let delta = time - land_time;
        if delta < 0 {
            0.0
        } else if delta < LAND_DEFLECT_TIME {
            change * delta as f32 / LAND_DEFLECT_TIME as f32
        } else if delta < LAND_DEFLECT_TIME + LAND_RETURN_TIME {
            change * (1.0 - (delta - LAND_DEFLECT_TIME) as f32 / LAND_RETURN_TIME as f32)
        } else {
            0.0
        }
    }

    /// The eye offset relative to `player.origin()` plus the angle deltas for
    /// `time`, or `None` during intermission where the view is verbatim.
    pub fn offset(
        &self,
        player: &LegacyViewPlayer,
        time: i32,
        config: LegacyViewBobConfig,
    ) -> Option<LegacyFirstPersonOffset> {
        if player.movement_type == PM_INTERMISSION {
            return None;
        }
        let bob = LegacyViewBobSample::new(player.bob_cycle, player.velocity);
        let (forward, left) = forward_left(player.view_angles);
        let mut pitch = dot(player.velocity, forward) * config.run_pitch;
        let mut roll = -dot(player.velocity, left) * config.run_roll;
        let speed = bob.xy_speed.max(200.0);
        let crouch = if player.movement_flags & PMF_DUCKED != 0 {
            3.0
        } else {
            1.0
        };
        pitch += bob.fraction_sin * config.bob_pitch * speed * crouch;
        let mut roll_bob = bob.fraction_sin * config.bob_roll * speed * crouch;
        if bob.odd_leg {
            roll_bob = -roll_bob;
        }
        roll += roll_bob;
        let mut height = player.view_height as f32;
        if let Some((duck_time, change)) = self.duck {
            let delta = time - duck_time;
            if (0..DUCK_TIME).contains(&delta) {
                height -= change * (DUCK_TIME - delta) as f32 / DUCK_TIME as f32;
            }
        }
        height += (bob.fraction_sin * bob.xy_speed * config.bob_up).min(6.0);
        height += self.landing_offset(time);
        if let Some((step_time, change)) = self.step {
            let delta = time - step_time;
            if (0..STEP_TIME).contains(&delta) {
                height -= change * (STEP_TIME - delta) as f32 / STEP_TIME as f32;
            }
        }
        Some(LegacyFirstPersonOffset {
            height,
            angle_delta: [pitch, 0.0, roll],
        })
    }
}

/// The predicted-player-state fields `CG_OffsetFirstPersonView` reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyViewPlayer {
    pub movement_type: u8,
    pub movement_flags: u16,
    pub velocity: [f32; 3],
    pub view_angles: [f32; 3],
    pub view_height: i32,
    pub bob_cycle: u8,
}

impl LegacyViewPlayer {
    /// Read the fields straight from a networked player state.
    pub fn from_player_state(player: &PlayerState) -> Self {
        Self {
            movement_type: player.movement_type(),
            movement_flags: player.movement_flags(),
            velocity: player.velocity(),
            view_angles: player.view_angles(),
            view_height: player.view_height(),
            bob_cycle: player.bob_cycle(),
        }
    }
}

/// Eye height above the player origin and `[pitch, yaw, roll]` degrees to
/// add to the view angles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyFirstPersonOffset {
    pub height: f32,
    pub angle_delta: [f32; 3],
}

/// `AngleVectors` forward and left rows for `[pitch, yaw, roll]` degrees.
fn forward_left(angles: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let (sp, cp) = angles[0].to_radians().sin_cos();
    let (sy, cy) = angles[1].to_radians().sin_cos();
    let (sr, cr) = angles[2].to_radians().sin_cos();
    let forward = [cp * cy, cp * sy, -sp];
    let left = [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp];
    // `AngleVectors` writes `right`; the view axis row 1 is `-right` = left.
    (forward, left.map(|component| -component))
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod predicted_duck_tests {
    use super::*;

    #[test]
    fn a_predicted_crouch_starts_the_duck_smoothing_once() {
        let mut view = LegacyFirstPersonView::default();
        view.observe_predicted_view_height(40, 1_000);
        assert_eq!(view.duck, None);
        view.observe_predicted_view_height(12, 1_050);
        assert_eq!(view.duck, Some((1_050, -28.0)));
        // The same height again changes nothing.
        view.observe_predicted_view_height(12, 1_100);
        assert_eq!(view.duck, Some((1_050, -28.0)));
    }
}
