//! Small controls the settings and pickers share: a text-edit underline and a
//! list's position mark.

use super::MenuCanvas;
use sjk_ui::{Color, DrawCommand, Rect};

impl MenuCanvas {
    /// Underline marking an active inline text edit.
    pub(crate) fn edit_underline(&mut self, rect: Rect, color: Color, scale: f32) {
        let _ = self.draw.push(DrawCommand::SolidRect {
            rect: Rect::new(rect.x, rect.bottom() - 2.0 * scale, rect.width, 2.0 * scale),
            color,
        });
    }

    /// Thin, display-only position mark in `track` for a list showing
    /// `page` of its `count` rows from `first`.
    pub(crate) fn list_scroll_mark(
        &mut self,
        track: Rect,
        first: usize,
        page: usize,
        count: usize,
        scale: f32,
    ) {
        let thumb_height = (track.height * page as f32 / count.max(1) as f32).max(24.0 * scale);
        let travel = track.height - thumb_height;
        let at = first as f32 / count.saturating_sub(page).max(1) as f32;
        let _ = self.draw.push(DrawCommand::RoundedRect {
            rect: track,
            radius: track.width * 0.5,
            color: Color::new(1.0, 1.0, 1.0, 0.07),
        });
        let _ = self.draw.push(DrawCommand::RoundedRect {
            rect: Rect::new(
                track.x,
                track.y + travel * at.clamp(0.0, 1.0),
                track.width,
                thumb_height,
            ),
            radius: track.width * 0.5,
            color: Color::new(0.809, 0.865, 0.906, 0.765),
        });
    }
}
