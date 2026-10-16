//! Draft drawing and pointer metrics share the console's colour-aware glyph walk.

use super::*;
use crate::console::edit_view::{scroll_start, walk};

impl ChatOverlay {
    pub(in crate::chat) fn build_draft(&mut self, font: &UiFont, g: &Geometry, y: f32, ms: u64) {
        let Some(input) = &mut self.input else { return };
        let cursor = input.edit.cursor(&input.text);
        let size = 21.0 * g.scale;
        let width = (g.width - 20.0 * g.scale).max(1.0);
        let advance = |key| font.glyph_of(TextFace::Regular, key).advance * size / font.height;
        let start = scroll_start(&input.text, cursor, width, advance);
        let end = start + layout::fitting_end(&input.text[start..], font, width, size);
        let draft = &input.text[start..end];
        input.layout.rect = Rect::new(g.left, y, g.width, 34.0 * g.scale);
        input.layout.len = 0;
        walk(draft, advance, |index, x, _| {
            // Bounded by the stops array, whatever the window's width lets show.
            let Some(stop) = input.layout.stops.get_mut(input.layout.len) else {
                return true;
            };
            *stop = (start + index, g.left + x);
            input.layout.len += 1;
            false
        });
        let measure =
            |text| visible_text_width_face(font, text, size / font.height, TextFace::Regular);
        if let Some(range) = input.edit.selection(&input.text) {
            let left = range.start.clamp(start, end);
            let right = range.end.clamp(start, end);
            if left < right {
                let accent = self.ui.theme().accent;
                self.ui.accent_bar(
                    Rect::new(
                        g.left + measure(&input.text[start..left]),
                        y,
                        measure(&input.text[left..right]),
                        30.0 * g.scale,
                    ),
                    Color::new(accent.r, accent.g, accent.b, 0.30),
                );
            }
        }
        self.ui.text(
            if input.text.is_empty() {
                "Say something..."
            } else {
                draft
            },
            input.layout.rect,
            size,
            if input.text.is_empty() {
                Color::new(0.809, 0.843, 0.876, 0.905)
            } else {
                self.ui.theme().foreground
            },
            FontWeight::Regular,
            0.0,
        );
        let caret = measure(&input.text[start..cursor]);
        let blink = Tween::pulse(0.35, 1.0, ms.saturating_sub(self.opened_ms), 550);
        self.ui.accent_bar(
            Rect::new(
                g.left + caret + 2.0 * g.scale,
                y + 2.0 * g.scale,
                2.0 * g.scale,
                23.0 * g.scale,
            ),
            tint(input.channel, blink),
        );
        self.ui.accent_bar(
            Rect::new(g.left, y + 38.0 * g.scale, 32.0 * g.scale, g.scale),
            tint(input.channel, 0.55),
        );
    }
}
