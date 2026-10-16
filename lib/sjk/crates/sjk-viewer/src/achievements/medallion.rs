//! How an achievement looks wherever it is drawn: its category's colour, its goal as
//! the medallion reads it (`1`, `100`, `1K`, `10h`) and the medallion itself, a ring
//! filling with the count round the goal, gold and lit once unlocked. The Profile
//! page's board (`profile_panel_view.rs`) and the unlock pop-up
//! (`unlock_toast.rs`) share them, so the two look alike.

use super::{Category, Counter, Kind, Source};
use crate::menu::sjk::{color, text};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};
use std::f32::consts::{FRAC_PI_2, TAU};
use std::fmt;

/// The board's medallion radius, which the other sizes are drawn for (1080-line pixels).
const BOARD_RADIUS: f32 = 31.0;

/// A category's colour.
pub(crate) fn tint(category: Category) -> Color {
    match category {
        Category::Combat => color::EMBER,
        Category::Duels => color::GOLD,
        Category::Journeys => color::HOLO,
        Category::Community => Color::new(0.45, 0.86, 0.62, 1.0),
    }
}

/// The goal as a medallion reads it, written without allocating.
#[derive(Clone, Copy)]
pub(crate) struct GoalLabel(&'static Kind);

/// `kind`'s goal as a medallion reads it: `1`, `100`, `1K`, `10h`.
pub(crate) fn goal_label(kind: &'static Kind) -> GoalLabel {
    GoalLabel(kind)
}

impl fmt::Display for GoalLabel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = self.0;
        if kind.source == Source::Client(Counter::Minutes) {
            return write!(formatter, "{}h", kind.goal / 60);
        }
        if kind.goal >= 1_000 && kind.goal.is_multiple_of(1_000) {
            return write!(formatter, "{}K", kind.goal / 1_000);
        }
        write!(formatter, "{}", kind.goal)
    }
}

/// What a medallion shows.
#[derive(Clone, Copy)]
pub(crate) struct Medallion {
    pub(crate) kind: &'static Kind,
    /// Where its middle is, in window pixels.
    pub(crate) centre: [f32; 2],
    /// Its ring's radius in window pixels.
    pub(crate) radius: f32,
    /// How much of the ring is filled, from 0 to 1.
    pub(crate) fraction: f32,
    /// Unlocked: the disc lit in the category's colour, the ring and goal in gold.
    pub(crate) done: bool,
}

/// Draw `medallion` on `canvas`.
pub(crate) fn draw(canvas: &mut MenuCanvas, medallion: Medallion) {
    let Medallion {
        kind,
        centre,
        radius,
        fraction,
        done,
    } = medallion;
    // The board's line widths and type, at this medallion's size.
    let k = radius / BOARD_RADIUS;
    let hue = tint(kind.category);
    let list = canvas.draw_list_mut();
    if done {
        let _ = list.push(DrawCommand::RoundedRect {
            rect: Rect::new(
                centre[0] - radius,
                centre[1] - radius,
                radius * 2.0,
                radius * 2.0,
            ),
            radius,
            color: color::alpha(hue, 0.28),
        });
    }
    let _ = list.push(DrawCommand::Arc {
        center: centre,
        radius,
        width: 3.0 * k,
        start: 0.0,
        sweep: TAU,
        color: color::alpha(color::HOLO, 0.16),
        knockout: None,
    });
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction > 0.0 {
        let _ = list.push(DrawCommand::Arc {
            center: centre,
            radius,
            width: 4.0 * k,
            start: -FRAC_PI_2,
            sweep: TAU * fraction,
            color: if done { color::GOLD_BRIGHT } else { hue },
            knockout: None,
        });
    }
    text(
        canvas,
        TextFamily::Display,
        format_args!("{}", goal_label(kind)),
        Rect::new(
            centre[0] - radius,
            centre[1] - 16.0 * k,
            radius * 2.0,
            32.0 * k,
        ),
        22.0 * k,
        if done {
            color::GOLD_BRIGHT
        } else {
            color::QUIET
        },
        FontWeight::Semibold,
        TextAlign::Center,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medallions_read_the_goal() {
        let label = |id| goal_label(super::super::find(id).unwrap()).to_string();
        assert_eq!(label("first_blood"), "1");
        assert_eq!(label("kills_1000"), "1K");
        assert_eq!(label("hours_100"), "100h");
        assert_eq!(label("arsenal"), "8");
    }
}
