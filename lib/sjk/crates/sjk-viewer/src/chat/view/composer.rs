//! Unboxed composer, channel choices, and a compact player-action popover.

use super::*;
use crate::chat::interaction::{GLOBAL, LATEST, SJK, TEAM};

impl ChatOverlay {
    pub(in crate::chat) fn build_composer(&mut self, font: &UiFont, g: &Geometry, ms: u64) {
        let Some(input) = &self.input else {
            return;
        };
        let alpha = Tween::new(0.0, 1.0, self.opened_ms, ENTER_MS, Easing::EaseOutCubic).sample(ms);
        let y = g.bottom + 18.0 * g.scale;
        self.ui.push_opacity(alpha);
        for (token, label, channel, offset) in [
            (GLOBAL, "All", Channel::Global, 0.0),
            (TEAM, "Team", Channel::Team, 58.0),
            (SJK, "SJK", Channel::Sjk, 116.0),
        ] {
            let rect = Rect::new(g.left + offset * g.scale, y, 50.0 * g.scale, 26.0 * g.scale);
            let selected = input.channel == channel;
            self.ui.hit_region(token, rect);
            self.ui.text(
                label,
                rect,
                14.0 * g.scale,
                if selected || self.ui.token_hovered(token) {
                    tint(channel, 1.0)
                } else {
                    Color::new(0.786, 0.827, 0.865, 1.0)
                },
                FontWeight::Semibold,
                0.0,
            );
            if selected {
                self.ui.accent_bar(
                    Rect::new(rect.x, rect.bottom(), 20.0 * g.scale, 2.0 * g.scale),
                    tint(channel, 1.0),
                );
            }
        }
        if input.channel == Channel::Whisper {
            let name = input
                .recipient
                .and_then(|target| self.roster.name(target))
                .unwrap_or("Player unavailable");
            let end = layout::fitting_end(name, font, g.width - 303.0 * g.scale, 14.0 * g.scale);
            self.ui.text_fmt_aligned(
                format_args!("To {}", &name[..end]),
                Rect::new(
                    g.left + 178.0 * g.scale,
                    y,
                    g.width - 298.0 * g.scale,
                    26.0 * g.scale,
                ),
                14.0 * g.scale,
                tint(Channel::Whisper, 1.0),
                FontWeight::Semibold,
                0.0,
                TextAlign::Start,
            );
        }
        if self.scroll > 0 {
            let rect = Rect::new(
                g.left + g.width - 116.0 * g.scale,
                y,
                116.0 * g.scale,
                26.0 * g.scale,
            );
            self.ui.hit_region(LATEST, rect);
            self.ui.text_fmt_aligned(
                format_args!("Latest +{}", self.unread),
                rect,
                12.0 * g.scale,
                tint(Channel::Global, 1.0),
                FontWeight::Semibold,
                0.0,
                TextAlign::End,
            );
        }
        self.build_draft(font, g, y + 42.0 * g.scale, ms);
        if !self.notice.is_empty() {
            self.ui.text(
                self.notice,
                Rect::new(g.left, y + 94.0 * g.scale, g.width, 20.0 * g.scale),
                11.0 * g.scale,
                Color::new(0.832, 0.87, 0.901, 0.93),
                FontWeight::Regular,
                0.0,
            );
        }
        self.ui.pop_opacity();
    }
}
