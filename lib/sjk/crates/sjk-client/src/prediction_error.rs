//! Prediction-error smoothing of BaseJKA (`cg.predictedError`, `cg_errorDecay`).
//!
//! When a new snapshot arrives, cgame re-predicts the pending commands from
//! the authoritative player state. If the re-predicted origin at the command
//! time the previous frame had already predicted differs from that previous
//! prediction, the difference is not applied to the view as a snap: it is
//! accumulated into `cg.predictedError` and the view origin is offset by a
//! linearly decaying fraction of it over `cg_errorDecay` milliseconds
//! (default 100). Every rule cites `codemp/cgame`:
//!
//! - `cg_predict.c:1188-1237`: the miss is measured only when the re-predicted
//!   state reached the old `commandTime`; a teleport this frame clears the
//!   error instead; misses of at most 0.1 units are ignored; an outstanding
//!   error is first decayed to the current time (`cg_errorDecay 0` discards
//!   it) and the new delta added on top; the error clock is stamped with
//!   `cg.oldTime`, the previous frame's time.
//! - `cg_view.c:1597-1608`: while `0 < f < 1` the view origin gets
//!   `f * predictedError` with `f = (decay - (cg.time - errorTime)) / decay`;
//!   otherwise the clock is reset to zero.
//! - `cg_snapshot.c:205-207` and `cg_playerstate.c:507-509`: a frame is a
//!   teleport when `EF_TELEPORT_BIT` toggled between snapshots or the
//!   followed client changed.
//!
//! The model origin keeps the un-offset predicted position exactly as
//! `cent->lerpOrigin` does; only `refdef.vieworg` is shifted.

/// `EF_TELEPORT_BIT` (`codemp/game/bg_public.h:632`).
pub const EF_TELEPORT_BIT: u32 = 1 << 3;

/// Default `cg_errorDecay` (`cg_xcvar.h:84`), in milliseconds.
pub const DEFAULT_ERROR_DECAY_MILLIS: f32 = 100.0;

/// Misses of this size or below are not smoothed (`cg_predict.c:1214`).
const IGNORED_MISS_UNITS: f32 = 0.1;

/// Accumulated prediction error and its decay clock.
#[derive(Clone, Debug, PartialEq)]
pub struct PredictionErrorDecay {
    decay_millis: f32,
    error: [f32; 3],
    error_time: i32,
    teleport_pending: bool,
}

impl Default for PredictionErrorDecay {
    fn default() -> Self {
        Self::new(DEFAULT_ERROR_DECAY_MILLIS)
    }
}

impl PredictionErrorDecay {
    /// Smoother with the given `cg_errorDecay` value in milliseconds.
    pub fn new(decay_millis: f32) -> Self {
        Self {
            decay_millis,
            error: [0.0; 3],
            error_time: 0,
            teleport_pending: false,
        }
    }

    /// Update the `cg_errorDecay` value; zero disables smoothing.
    pub fn set_decay_millis(&mut self, decay_millis: f32) {
        self.decay_millis = decay_millis.max(0.0);
    }

    /// Current `cg_errorDecay` value in milliseconds.
    pub fn decay_millis(&self) -> f32 {
        self.decay_millis
    }

    /// Mark the coming re-prediction as a teleport (`cg.thisFrameTeleport`).
    pub fn mark_teleport(&mut self) {
        self.teleport_pending = true;
    }

    /// Whether a teleport is pending for the next re-prediction.
    pub fn teleport_pending(&self) -> bool {
        self.teleport_pending
    }

    /// Record a re-prediction result (`cg_predict.c:1188-1237`).
    ///
    /// `previous_origin` is what the previous frame had predicted for the
    /// command time the re-prediction just reached; `origin` is the
    /// re-predicted position at that same command time. `now_millis` is
    /// `cg.time`, `previous_frame_millis` is `cg.oldTime`.
    pub fn record_miss(
        &mut self,
        previous_origin: [f32; 3],
        origin: [f32; 3],
        now_millis: i32,
        previous_frame_millis: i32,
    ) {
        if self.teleport_pending {
            self.error = [0.0; 3];
            self.teleport_pending = false;
            return;
        }
        let delta = [
            previous_origin[0] - origin[0],
            previous_origin[1] - origin[1],
            previous_origin[2] - origin[2],
        ];
        let length = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
        if length <= IGNORED_MISS_UNITS {
            return;
        }
        let carried = if self.decay_millis > 0.0 {
            self.decay_fraction(now_millis).max(0.0)
        } else {
            0.0
        };
        for axis in 0..3 {
            self.error[axis] = delta[axis] + self.error[axis] * carried;
        }
        self.error_time = previous_frame_millis;
    }

    /// View-origin offset for this frame (`cg_view.c:1597-1608`).
    pub fn view_offset(&mut self, now_millis: i32) -> [f32; 3] {
        if self.decay_millis <= 0.0 {
            return [0.0; 3];
        }
        let fraction = self.decay_fraction(now_millis);
        if fraction > 0.0 && fraction < 1.0 {
            self.error.map(|axis| axis * fraction)
        } else {
            self.error_time = 0;
            [0.0; 3]
        }
    }

    /// Accumulated error vector (before decay).
    pub fn error(&self) -> [f32; 3] {
        self.error
    }

    fn decay_fraction(&self, now_millis: i32) -> f32 {
        let elapsed = now_millis.wrapping_sub(self.error_time) as f32;
        (self.decay_millis - elapsed) / self.decay_millis
    }
}
