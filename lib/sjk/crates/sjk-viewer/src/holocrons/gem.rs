//! A tier's gem, drawn from shapes: what stands for a holocron where its icon
//! (`gfx/sjk/holocron_<tier>`, [`super::icons`]) is missing, and the small mark in a
//! drop's chat line. A gem is a kite of horizontal bars: a flat top that widens to the
//! girdle, then narrows to a point, its left side darker than its right and its top
//! brighter than its foot, with a spark of light on the upper left.
//!
//! The number of commands is fixed by the rows ([`commands`]): nothing is allocated.

use sjk_ui::{Color, DrawCommand, DrawList, Rect};

/// Rows of a full-size gem (a pop-up's, a tab's).
pub(crate) const ROWS: usize = 16;
/// Rows of a small mark (a chat line's).
pub(crate) const MARK_ROWS: usize = 6;
/// Where the girdle, the widest row, is: 0 at the top, 1 at the point.
const GIRDLE: f32 = 0.35;
/// The flat top's width against the girdle's.
const TOP_WIDTH: f32 = 0.55;

/// How many draw commands a gem of `rows` rows takes.
#[cfg(test)]
pub(crate) const fn commands(rows: usize) -> usize {
    2 * rows + 1
}

/// The width of the gem at `at` (0 at the top, 1 at the point), against its full width.
fn width(at: f32) -> f32 {
    if at <= GIRDLE {
        TOP_WIDTH + (1.0 - TOP_WIDTH) * at / GIRDLE
    } else {
        ((1.0 - at) / (1.0 - GIRDLE)).max(0.0)
    }
}

/// `colour` scaled by `factor` (a brighter or darker facet), kept a colour.
fn shade(colour: Color, factor: f32, alpha: f32) -> Color {
    Color::new(
        (colour.r * factor).min(1.0),
        (colour.g * factor).min(1.0),
        (colour.b * factor).min(1.0),
        colour.a * alpha,
    )
}

/// Draw a gem of `colour` filling the square of side `2 * half` centred on `centre`, at
/// `alpha`, in `rows` rows.
pub(crate) fn draw(
    list: &mut DrawList,
    centre: [f32; 2],
    half: f32,
    colour: Color,
    alpha: f32,
    rows: usize,
) {
    let rows = rows.max(2);
    let height = 2.0 * half / rows as f32;
    for row in 0..rows {
        let at = (row as f32 + 0.5) / rows as f32;
        let half_width = half * width(at);
        // The foot is darker than the crown; a hair of overlap hides seams.
        let light = 1.2 - 0.5 * at;
        let top = centre[1] - half + row as f32 * height;
        for (left, factor) in [(true, 0.78), (false, 1.0)] {
            let x = if left {
                centre[0] - half_width
            } else {
                centre[0]
            };
            let _ = list.push(DrawCommand::SolidRect {
                rect: Rect::new(x, top, half_width, height + 0.5),
                color: shade(colour, light * factor, alpha),
            });
        }
    }
    // A spark of light on the crown's upper left.
    let spark = half * 0.16;
    let _ = list.push(DrawCommand::RoundedRect {
        rect: Rect::new(
            centre[0] - half * 0.36,
            centre[1] - half * 0.62,
            spark * 1.6,
            spark,
        ),
        radius: spark * 0.5,
        color: Color::new(1.0, 1.0, 1.0, 0.55 * alpha),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gem_is_a_fixed_number_of_commands_inside_its_square() {
        for rows in [MARK_ROWS, ROWS, 2, 40] {
            let mut list = DrawList::new(256);
            let centre = [100.0, 60.0];
            draw(
                &mut list,
                centre,
                20.0,
                Color::new(0.2, 0.5, 1.0, 1.0),
                1.0,
                rows,
            );
            assert_eq!(list.len(), commands(rows));
            for command in list.commands() {
                let rect = match command {
                    DrawCommand::SolidRect { rect, .. } | DrawCommand::RoundedRect { rect, .. } => {
                        rect
                    }
                    other => panic!("{other:?}"),
                };
                assert!(
                    rect.x >= centre[0] - 20.0 - 1e-3
                        && rect.right() <= centre[0] + 20.0 + 1e-3
                        && rect.y >= centre[1] - 20.0 - 1e-3
                        && rect.bottom() <= centre[1] + 20.0 + 0.5 + 1e-3,
                    "{rows} rows: {rect:?}"
                );
            }
        }
    }

    #[test]
    fn the_gem_is_widest_at_the_girdle_and_ends_in_a_point() {
        assert!((width(GIRDLE) - 1.0).abs() < 1e-6);
        assert!(width(0.0) < width(GIRDLE) && width(0.0) > 0.4);
        assert!(width(0.9) < width(0.6));
        assert_eq!(width(1.0), 0.0);
    }

    #[test]
    fn alpha_fades_every_part_and_the_foot_is_darker_than_the_crown() {
        let mut list = DrawList::new(64);
        draw(
            &mut list,
            [0.0, 0.0],
            10.0,
            Color::new(0.5, 0.5, 0.5, 1.0),
            0.5,
            4,
        );
        for command in list.commands() {
            let alpha = match command {
                DrawCommand::SolidRect { color, .. } | DrawCommand::RoundedRect { color, .. } => {
                    color.a
                }
                other => panic!("{other:?}"),
            };
            assert!(
                alpha <= 0.5 * 0.55 + 1e-6 || (alpha - 0.5).abs() < 1e-6,
                "{alpha}"
            );
        }
        let brightness = |index: usize| match &list.commands()[index] {
            DrawCommand::SolidRect { color, .. } => color.r,
            other => panic!("{other:?}"),
        };
        assert!(brightness(1) > brightness(7), "right side: crown over foot");
        assert!(brightness(1) > brightness(0), "right over left");
    }
}
