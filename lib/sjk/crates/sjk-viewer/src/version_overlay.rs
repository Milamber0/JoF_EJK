//! The build label (`SJK <version> · <dd/mm/yyyy HH:MM> · <commit>`, see
//! [`crate::build_info`]) drawn small at the top centre of every frame, in menus
//! and in play, so a screenshot says which build it shows.
//!
//! The top centre is free in the stock and game-data HUDs: their gauges sit in
//! the bottom corners, the FPS counter and timers at the top right, notify and
//! vote lines at the top left, and warmup and centre prints lower down. While
//! the console is open it already shows the version in its own corner, so the
//! label is left out. `cg_drawVersion 0` hides it.

use crate::GpuState;
use crate::text::{TextFace, append_text_style, visible_text_width};
use crate::ui_scale;

/// The cvar that shows the label (1, the default) or hides it (0).
pub(crate) const CVAR: &str = "cg_drawVersion";

/// Line height at 1080 lines: half of Inter's 38.7-pixel menu line.
const LINE_AT_1080: f32 = 19.0;
/// Gap above the label at 1080 lines.
const TOP_AT_1080: f32 = 3.0;
/// White at a little over half opacity: readable, never louder than the HUD.
const COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.6];

impl GpuState {
    /// Append the label to this frame's UI text, above everything else.
    pub(crate) fn append_version_overlay(&mut self, viewport: [f32; 2], text_scale: f32) {
        let shown = self
            .console
            .as_ref()
            .is_none_or(|console| console.bool_cvar(CVAR).unwrap_or(true) && !console.is_open());
        if !shown {
            return;
        }
        let label = crate::build_info::label();
        let scale = ui_scale::glyph_scale(&self.ui_font, LINE_AT_1080, text_scale);
        let width = visible_text_width(&self.ui_font, label, scale);
        let top = TOP_AT_1080 * text_scale;
        append_text_style(
            &mut self.text_vertices,
            &self.ui_font,
            label,
            [((viewport[0] - width) * 0.5).round(), top.round()],
            scale,
            viewport,
            TextFace::Regular,
            COLOR,
            0.0,
        );
    }
}
