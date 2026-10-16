//! The Collection's Toys tab: things to use in a match. The first is SJK's Illuminate
//! holocron, everyone's (`docs/client.md`, Illuminate): its picture on its plinth on
//! the left, the holocron lit by the player's model beside the page, and the words
//! beside the model with the switch that lights it or puts it out (`toy_illuminate`).

use super::view::{LEFT_X, STAGE_TEXT_WIDTH, STAGE_TEXT_X, glow, left_lines};
use super::*;
use crate::menu::sjk::{Frame, color, kit, text};
use crate::menu_widgets::TextFamily;
use sjk_ui::{Color, DrawCommand, FontWeight, TextAlign, TextureId};

/// The toy's niche: its corner and size.
const NICHE: [f32; 3] = [LEFT_X, 300.0, 260.0];

/// Illuminate's picture: the quick wheel's, uploaded into the atlas at start.
fn icon() -> TextureId {
    crate::ui_renderer::wheel_icon(
        crate::quick_wheel::catalog::icon_index("illuminate")
            .expect("the quick wheel's icons hold Illuminate's"),
    )
}

/// A row of dots across the left side at `y`: the room for more.
pub(super) fn room_for_more(
    canvas: &mut crate::menu_widgets::MenuCanvas,
    frame: &Frame,
    y: f32,
    words: &str,
) {
    let mut x = LEFT_X;
    while x < LEFT_X + 820.0 {
        let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
            rect: frame.rect(x, y, 8.0, 1.0),
            color: color::alpha(color::HOLO, 0.22),
        });
        x += 16.0;
    }
    text(
        canvas,
        TextFamily::Body,
        format_args!("{words}"),
        frame.rect(LEFT_X, y + 16.0, 820.0, 24.0),
        15.0 * frame.s,
        color::QUIET,
        FontWeight::Regular,
        TextAlign::Start,
    );
}

impl Panel {
    /// The Toys tab.
    pub(super) fn toys(&mut self, frame: &Frame, inputs: &Inputs<'_>) {
        let s = frame.s;
        self.lead(
            frame,
            "1 toy",
            "Things to use in a match, from the quick wheel or a key.",
        );
        let [x, y, size] = NICHE;
        glow(
            &mut self.ui,
            frame,
            [x + size * 0.5, y + size * 0.55],
            size * 0.62,
            color::GOLD,
            0.42,
        );
        let rect = frame.rect(x, y, size, size);
        let _ = self.ui.draw_list_mut().push(DrawCommand::Border {
            rect,
            radius: 14.0 * s,
            width: 2.0 * s,
            color: color::GOLD_BRIGHT,
        });
        let art = size * 0.72;
        let _ = self.ui.draw_list_mut().push(DrawCommand::TexturedQuad {
            rect: frame.rect(x + (size - art) * 0.5, y + (size - art) * 0.5, art, art),
            texture: icon(),
            color: Color::new(1.0, 1.0, 1.0, 1.0),
        });
        let _ = self.ui.draw_list_mut().push(DrawCommand::SolidRect {
            rect: frame.rect(x, y + size + 16.0, size, 2.0),
            color: color::GOLD,
        });
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("Illuminate"),
            frame.rect(x, y + size + 30.0, size, 30.0),
            24.0 * s,
            color::GOLD_BRIGHT,
            FontWeight::Semibold,
            TextAlign::Center,
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Everyone's"),
            frame.rect(x, y + size + 62.0, size, 22.0),
            15.0 * s,
            color::GOLD,
            FontWeight::Regular,
            TextAlign::Center,
        );
        self.ui.hit_region(TOY_TOKEN, rect);
        room_for_more(
            &mut self.ui,
            frame,
            716.0,
            "More toys will stand here as SJK adds them.",
        );
        self.toy_beside_model(frame, inputs);
    }

    /// What the toy is, beside the model, and its switch.
    fn toy_beside_model(&mut self, frame: &Frame, inputs: &Inputs<'_>) {
        let s = frame.s;
        let x = STAGE_TEXT_X;
        let width = STAGE_TEXT_WIDTH;
        if self.backstage == Backstage::None {
            glow(
                &mut self.ui,
                frame,
                [x + 110.0, 400.0],
                150.0,
                color::GOLD,
                0.3,
            );
            let _ = self.ui.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: frame.rect(x + 10.0, 300.0, 200.0, 200.0),
                texture: icon(),
                color: Color::new(1.0, 1.0, 1.0, 1.0),
            });
        }
        let mut y = 618.0;
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Toy"),
            frame.rect(x, y, width, 22.0),
            16.0 * s,
            color::HOLO,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        y += 26.0;
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("Illuminate holocron"),
            frame.rect(x, y, width, 62.0),
            52.0 * s,
            color::GOLD_BRIGHT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        y += 70.0;
        y = left_lines(
            &mut self.ui,
            frame,
            "A holocron floats by your left shoulder and lights the way in dark maps. Other SJK players on your server see it lit.",
            [x, y, width],
            (52, 3),
            18.0,
            color::TEXT,
            TextAlign::Start,
        );
        y += 4.0;
        y = left_lines(
            &mut self.ui,
            frame,
            "Bind a key to toy_illuminate, or use the quick wheel's Toys page.",
            [x, y, width],
            (60, 2),
            16.0,
            color::MUTED,
            TextAlign::Start,
        );
        y += 18.0;
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("Holocron lit"),
            frame.rect(x, y, 300.0, 30.0),
            22.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let right = x + 330.0;
        let left = kit::switch(
            &mut self.ui,
            frame,
            right,
            y + 15.0,
            inputs.illuminate,
            true,
        );
        self.ui.hit_region(
            TOY_SWITCH_TOKEN,
            frame.rect(left - 4.0, y - 4.0, right - left + 8.0, 38.0),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The toy, its words and its switch fit over the keys and answer the pointer,
    /// with and without a model, at 1080 lines, 4K, 4:3 and 21:9.
    #[test]
    fn the_toy_fits_over_the_keys() {
        let font = crate::text::load_modern(1.0, None).expect("Inter").font;
        for viewport in [
            [1_920.0, 1_080.0],
            [3_840.0, 2_160.0],
            [1_440.0, 1_080.0],
            [2_560.0, 1_080.0],
        ] {
            for backstage in [Backstage::None, Backstage::Preview { ready: true }] {
                let inputs = super::super::tests::inputs(unlockables::Holdings::Known(&[]), "");
                let mut panel = Panel::new();
                panel.open(Tab::Toys, true, true, ReturnTarget::InGame);
                panel.set_backstage(backstage);
                panel.build(&inputs, &font, viewport);
                assert!(!panel.ui.overflowed(), "{viewport:?}");
                let frame = Frame::new(viewport);
                let keys = frame.point(0.0, super::super::view::KEYS_Y)[1];
                for token in [TOY_TOKEN, TOY_SWITCH_TOKEN] {
                    let area = panel.ui.rect_for(token).expect("a target");
                    assert!(area.bottom() <= keys, "{viewport:?}");
                }
                for command in panel.ui.draw_list().commands() {
                    if let DrawCommand::Text { rect, .. } = command {
                        assert!(rect.bottom() <= keys || rect.y >= keys, "{rect:?}");
                    }
                }
            }
        }
    }
}
