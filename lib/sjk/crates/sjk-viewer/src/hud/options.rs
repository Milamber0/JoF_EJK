//! Retained optional readouts; no per-frame string allocation.
use super::*;
use sjk_ui::{DrawCommand, FontWeight, Rect, TextAlign, TextOverflow};
use std::fmt::Write;

/// Stock cg_draw.c:5194-5199 sizes the crosshair in virtual screen coordinates.
pub(crate) fn crosshair_size(console: Option<&ViewerConsole>, visible: bool) -> f32 {
    if !visible {
        return 0.0;
    }
    console
        .and_then(|c| c.float_cvar("cg_crosshairsize"))
        .filter(|v| v.is_finite())
        .unwrap_or(24.0)
        .clamp(0.0, 640.0) as f32
}

/// Retained speed label, updated from predicted velocity.
pub(super) struct Speed {
    /// Text storage reserved once when the HUD is constructed.
    pub(super) text: String,
    enabled: bool,

    position: [f32; 3],
}

impl Default for Speed {
    fn default() -> Self {
        Self {
            text: String::with_capacity(48),
            enabled: false,

            position: [132.0, 459.0, 0.75],
        }
    }
}

impl Speed {
    /// Apply supported speedometer flags to the current predicted velocity.
    pub(super) fn update(&mut self, velocity: [f32; 3], console: Option<&ViewerConsole>) {
        self.position = [
            ("cg_speedometerx", 132.0),
            ("cg_speedometery", 459.0),
            ("cg_speedometersize", 0.75),
        ]
        .map(|(name, value)| crate::cgame_options::scalar(console, name, value));
        let bits = console
            .and_then(|c| c.integer_cvar("cg_speedometer"))
            .unwrap_or(0);
        self.set(velocity, bits);
    }

    fn set(&mut self, velocity: [f32; 3], bits: i64) {
        self.enabled = bits & 1 != 0;
        self.text.clear();
        if !self.enabled {
            return;
        }
        let [x, y, z] = velocity;
        let speed = (x * x + y * y + if bits & 32768 != 0 { z * z } else { 0.0 }).sqrt();
        let (factor, unit) = if bits & 256 != 0 {
            (0.1028699967, "km/h")
        } else if bits & 512 != 0 {
            (0.06392043271, "mph")
        } else {
            (1.0, "ups")
        };
        let value = if speed.is_finite() {
            (speed * factor).min(999_999.0)
        } else {
            0.0
        };
        let _ = write!(self.text, "{:.0} {unit}", (value + 0.5).floor());
    }

    /// Append a text command referencing retained storage, without a backplate.
    pub(super) fn emit(&self, draw: &mut DrawList, theme: Theme, viewport: [f32; 2]) {
        if !self.enabled {
            return;
        }
        let s = crate::ui_scale::height_scale(viewport[1]);
        let _ = draw.push(DrawCommand::Text {
            rect: Rect::new(
                self.position[0] * viewport[0] / 640.0,
                self.position[1] * viewport[1] / 480.0 - 32.0 * s,
                240.0 * s,
                32.0 * s,
            ),
            text: TextId(314),
            size: 32.0 * s * self.position[2].clamp(0.0, 4.0),
            color: theme.foreground,
            align: TextAlign::Center,
            overflow: TextOverflow::Clip,
            weight: FontWeight::Semibold,
            letter_spacing: 0.0,
        });
    }
}

impl HudOverlay {}
