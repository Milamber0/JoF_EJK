//! Compact player dropdown beside the name, without moving the conversation.

use super::*;
use crate::chat::player_actions::{ACTIONS, COPY_NAME, FRIEND, IGNORE, MENU_BACK, WHISPER};

/// Prefer the free left margin. At an edge, stay inside the chat lane rather
/// than covering the scoreboard; both placements end above the composer.
pub(in crate::chat) fn placement(g: &Geometry, viewport: [f32; 2], anchor_y: f32) -> (Rect, f32) {
    let scale = crate::ui_scale::height_scale(viewport[1])
        .min(viewport[0] / 96.0)
        .min(viewport[1] / 104.0)
        .max(0.001);
    let width = 80.0 * scale;
    let height = 92.0 * scale;
    let gap = 6.0 * scale;
    let left = g.left - width - gap;
    let x = if left >= 0.0 {
        left
    } else {
        g.left + g.width - width
    };
    let x = x.clamp(0.0, (viewport[0] - width).max(0.0));
    let bottom = g.bottom.min(viewport[1] - gap);
    let y = anchor_y.clamp(0.0, (bottom - height).max(0.0));
    (Rect::new(x, y, width, height), scale)
}

impl ChatOverlay {
    pub(in crate::chat) fn build_player_menu(&mut self, g: &Geometry, viewport: [f32; 2]) {
        let Some(menu) = self.player_menu else { return };
        let Some(name) = self.roster.name(menu.target) else {
            self.player_menu = None;
            return;
        };
        let (panel, scale) = placement(g, viewport, menu.anchor_y);
        self.ui
            .accent_bar(panel, Color::new(0.015, 0.028, 0.043, 0.94));
        self.ui.hit_region(MENU_BACK, panel);
        for (row, token) in ACTIONS.into_iter().enumerate() {
            let (label, enabled) = match token {
                WHISPER => ("whisper", false),
                IGNORE => ("ignore", self.muted.contains(&menu.target)),
                FRIEND => ("friend", self.friends.contains(name)),
                COPY_NAME => ("copy", false),
                _ => unreachable!(),
            };
            let rect = Rect::new(
                panel.x,
                panel.y + (2.0 + row as f32 * 22.0) * scale,
                panel.width,
                22.0 * scale,
            );
            self.ui.hit_region(token, rect);
            let highlighted = menu.selected.map_or_else(
                || self.ui.token_hovered(token),
                |selected| selected == token,
            );
            if highlighted {
                let accent = self.ui.theme().accent;
                self.ui
                    .accent_bar(rect, Color::new(accent.r, accent.g, accent.b, 0.16));
            }
            if enabled {
                self.ui.accent_bar(
                    Rect::new(rect.x, rect.y, 2.0 * scale, rect.height),
                    self.ui.theme().accent,
                );
            }
            self.ui.text(
                label,
                Rect::new(
                    rect.x + 8.0 * scale,
                    rect.y + 3.0 * scale,
                    rect.width - 16.0 * scale,
                    16.0 * scale,
                ),
                13.0 * scale,
                if enabled || highlighted {
                    self.ui.theme().accent
                } else {
                    self.ui.theme().foreground
                },
                FontWeight::Regular,
                0.0,
            );
        }
    }
}
