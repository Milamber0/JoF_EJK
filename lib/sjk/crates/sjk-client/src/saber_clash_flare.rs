//! Codemp saber-clash flare timing, visibility, and size selection.
//!
//! This mirrors `CG_SaberClashFlare` in `codemp/cgame/cg_draw.c:5337-5400`.
//! `EV_SABER_CLASHFLARE` backdates `cg_saberFlashTime` by 50 ms in
//! `codemp/cgame/cg_event.c:2372-2377`. Projection and drawing remain in the
//! viewer; the compatibility adapter owns the gameplay-authored envelope.

/// Why a currently latched clash does or does not produce a screen flare.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacySaberClashVisibility {
    NotActive,
    BehindView,
    Occluded,
    TooFar,
    Visible,
}

/// Exact world-space output of the codemp flare selector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacySaberClashSample {
    pub visibility: LegacySaberClashVisibility,
    pub position: [f32; 3],
    /// `v` used by `CG_DrawPic(x-v*300, y-v*300, v*600, v*600, ...)`.
    pub picture_scale: f32,
    pub elapsed_millis: i32,
}

/// Last-event latch corresponding to `cg_saberFlashTime/Pos`.
#[derive(Clone, Copy, Debug, Default)]
pub struct LegacySaberClashFlare {
    flash_time: i32,
    position: [f32; 3],
    latched: bool,
}

impl LegacySaberClashFlare {
    /// Latch one decoded clash event at the cgame time at which it was read.
    pub fn observe(&mut self, position: [f32; 3], cg_time: i32) {
        self.flash_time = cg_time.wrapping_sub(50);
        self.position = position;
        self.latched = true;
    }

    /// Evaluate the 150 ms codemp envelope and line-of-sight trace.
    ///
    /// The callback is the engine's existing point trace from the view origin
    /// to the clash position and returns the trace fraction. It is invoked
    /// only after the same time/behind-view tests used by cgame.
    pub fn sample(
        &self,
        cg_time: i32,
        view_origin: [f32; 3],
        view_forward: [f32; 3],
        mut trace_fraction: impl FnMut([f32; 3], [f32; 3]) -> f32,
    ) -> LegacySaberClashSample {
        const MAX_TIME: i32 = 150; // cg_draw.c:5341
        let elapsed = cg_time.wrapping_sub(self.flash_time);
        let mut result = LegacySaberClashSample {
            visibility: LegacySaberClashVisibility::NotActive,
            position: self.position,
            picture_scale: 0.0,
            elapsed_millis: elapsed,
        };
        if !self.latched || elapsed <= 0 || elapsed >= MAX_TIME {
            return result;
        }
        let difference = subtract(self.position, view_origin);
        if dot(difference, view_forward) < 0.2 {
            result.visibility = LegacySaberClashVisibility::BehindView;
            return result;
        }
        if trace_fraction(view_origin, self.position) < 1.0 {
            result.visibility = LegacySaberClashVisibility::Occluded;
            return result;
        }
        let length = dot(difference, difference).sqrt();
        if length > 1_200.0 {
            result.visibility = LegacySaberClashVisibility::TooFar;
            return result;
        }
        let mut scale =
            (1.0 - elapsed as f32 / MAX_TIME as f32) * ((1.0 - length / 800.0) * 2.0 + 0.35);
        if scale < 0.001 {
            scale = 0.001;
        }
        result.visibility = LegacySaberClashVisibility::Visible;
        result.picture_scale = scale;
        result
    }
}

fn subtract(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] - right[axis])
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left.iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
}
