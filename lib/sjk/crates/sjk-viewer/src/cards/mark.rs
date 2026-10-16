//! The credits' mark: the galactic credit's sign, a slanted seven crossed by two bars,
//! drawn from shapes so it is sharp at every size (no picture to load). Its outline is
//! the one Wikipedia draws (Sol's pick, 12/10/2026): a crossbar, a leg slanting down to
//! the left, and two short bars standing above it and two hanging below.
//!
//! The figures below are that outline's, in its own units (its top left at 0, 0; it is
//! [`WIDTH`] by [`HEIGHT`]). A mark is a fixed number of commands ([`COMMANDS`]).

use sjk_ui::{Color, DrawCommand, DrawList, Rect};

/// The outline's size in its own units.
const WIDTH: f32 = 258.8;
const HEIGHT: f32 = 476.6;
/// The crossbar's top and bottom.
const BAR_TOP: f32 = 92.6;
const BAR_BOTTOM: f32 = 198.5;
/// The two bars' left and right edges, above the crossbar and below it.
const BARS_ABOVE: [[f32; 2]; 2] = [[26.5, 46.7], [86.3, 105.9]];
const BARS_BELOW: [[f32; 2]; 2] = [[26.8, 46.4], [86.3, 105.9]];
/// How far the bars below reach.
const BELOW_END: f32 = 291.0;
/// The leg: the middle of its top (on the crossbar's top) and of its foot, and its width
/// across the stroke.
const LEG_TOP: [f32; 2] = [228.0, BAR_TOP];
const LEG_FOOT: [f32; 2] = [71.2, HEIGHT];
const LEG_WIDTH: f32 = 62.4;
/// Where the crossbar ends on the right: inside the leg.
const BAR_END: f32 = 200.0;

/// How many draw commands a mark takes.
#[cfg(test)]
pub(crate) const COMMANDS: usize = 8;

/// How wide a mark `height` tall is.
pub(crate) fn width(height: f32) -> f32 {
    height * WIDTH / HEIGHT
}

/// Draw a mark `height` pixels tall with its top left at `at` (window pixels), in
/// `colour`. Returns its width.
pub(crate) fn draw(list: &mut DrawList, at: [f32; 2], height: f32, colour: Color) -> f32 {
    let scale = height / HEIGHT;
    let [x, y] = at;
    let rect = |left: f32, top: f32, right: f32, bottom: f32| {
        Rect::new(
            x + left * scale,
            y + top * scale,
            (right - left) * scale,
            (bottom - top) * scale,
        )
    };
    let _ = list.push(DrawCommand::SolidRect {
        rect: rect(0.0, BAR_TOP, BAR_END, BAR_BOTTOM),
        color: colour,
    });
    for [left, right] in BARS_ABOVE {
        let _ = list.push(DrawCommand::SolidRect {
            rect: rect(left, 0.0, right, BAR_TOP),
            color: colour,
        });
    }
    for [left, right] in BARS_BELOW {
        let _ = list.push(DrawCommand::SolidRect {
            rect: rect(left, BAR_BOTTOM, right, BELOW_END),
            color: colour,
        });
    }
    // The leg runs past both ends and is cut level at the crossbar's top and the foot.
    let along = [LEG_FOOT[0] - LEG_TOP[0], LEG_FOOT[1] - LEG_TOP[1]];
    let reach = 0.2;
    let point = |t: f32| {
        [
            x + (LEG_TOP[0] + along[0] * t) * scale,
            y + (LEG_TOP[1] + along[1] * t) * scale,
        ]
    };
    let _ = list.push(DrawCommand::PushClip(rect(0.0, BAR_TOP, WIDTH, HEIGHT)));
    let _ = list.push(DrawCommand::Line {
        from: point(-reach),
        to: point(1.0 + reach),
        width: LEG_WIDTH * scale,
        color: colour,
    });
    let _ = list.push(DrawCommand::PopClip);
    WIDTH * scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mark_is_a_fixed_number_of_commands_within_its_box() {
        let mut list = DrawList::new(64);
        let colour = Color::new(1.0, 0.8, 0.2, 1.0);
        let wide = draw(&mut list, [10.0, 20.0], 47.66, colour);
        assert!((wide - 25.88).abs() < 0.01);
        assert!((width(47.66) - wide).abs() < 1e-4);
        assert_eq!(list.len(), COMMANDS);
        for command in list.commands() {
            if let DrawCommand::SolidRect { rect, .. } = command {
                assert!(rect.x >= 10.0 - 1e-3 && rect.right() <= 10.0 + wide + 1e-3);
                assert!(rect.y >= 20.0 - 1e-3 && rect.bottom() <= 20.0 + 47.66 + 1e-3);
            }
        }
    }
}
