//! How a GIF shows under its chat line, on the SJK chat page, the docks and the in-play
//! chat box: the GIF at a fixed height ([`Look::height`], smaller on the docks and in
//! the chat box), as wide as its shape makes it within the column (narrower and lower
//! when the column is too narrow); while it loads, a quiet box of that height saying
//! so; when it could not be had, one quiet line, "GIF unavailable".

use super::Shown;
use crate::menu::sjk::{Frame, color, text};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};

/// The GIF block's measures, in frame pixels.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Look {
    /// The room above the GIF, under the line's text.
    pub(crate) gap: f32,
    /// The GIF's height (its width follows its shape).
    pub(crate) height: f32,
    /// The height of the "GIF unavailable" line.
    pub(crate) note: f32,
    /// The size of the block's words.
    pub(crate) text_size: f32,
}

/// The SJK chat page's GIFs.
pub(crate) const PAGE: Look = Look {
    gap: 6.0,
    height: 160.0,
    note: 24.0,
    text_size: 15.0,
};

/// The docks' GIFs (main page, in-game menu), smaller.
pub(crate) const DOCK: Look = Look {
    gap: 4.0,
    height: 48.0,
    note: 20.0,
    text_size: 13.0,
};

/// The in-play chat box's GIFs (`chat/view.rs`), in 1080p pixels at chat font size 1,
/// grown with the chat box's scale: three of its 18-pixel rows high, so a GIF takes
/// the room of three lines and stays out of the way of play.
pub(crate) const FEED: Look = Look {
    gap: 4.0,
    height: 54.0,
    note: 18.0,
    text_size: 13.0,
};

/// The shape a loading GIF's box takes, its size not known yet.
const PLACEHOLDER_SHAPE: f32 = 1.5;

/// The room the block takes under its line (frame pixels).
pub(crate) fn room(look: Look, shown: Shown) -> f32 {
    match shown {
        Shown::Unavailable => look.note,
        Shown::Loading | Shown::Ready { .. } => look.gap + look.height,
    }
}

/// The size (frame pixels) a GIF of `size` (or a placeholder, `None`) is drawn at: the
/// look's height, its width from the GIF's shape, both made smaller to fit `width`.
pub(crate) fn fitted(look: Look, size: Option<[u32; 2]>, width: f32) -> [f32; 2] {
    let shape = size
        .filter(|[w, h]| *w > 0 && *h > 0)
        .map_or(PLACEHOLDER_SHAPE, |[w, h]| w as f32 / h as f32);
    let wide = look.height * shape;
    if wide <= width {
        [wide, look.height]
    } else {
        let width = width.max(1.0);
        [width, width / shape]
    }
}

/// Draw the block for `shown` with its top at `y` and its left at `x` (frame pixels)
/// in a column `width` wide.
pub(crate) fn draw(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    look: Look,
    shown: Shown,
    [x, y]: [f32; 2],
    width: f32,
) {
    paint(
        canvas,
        look,
        shown,
        frame.point(x, y),
        width * frame.s,
        frame.s,
        1.0,
    );
}

/// Draw the block for `shown` with its top-left at `origin` (window pixels) in a column
/// `width` window pixels wide, the look's measures grown by `s`, at `alpha` (the in-play
/// chat box fades a GIF with its line).
pub(crate) fn paint(
    canvas: &mut MenuCanvas,
    look: Look,
    shown: Shown,
    [x, y]: [f32; 2],
    width: f32,
    s: f32,
    alpha: f32,
) {
    let s = s.max(f32::EPSILON);
    let faded = |colour: Color, a: f32| color::alpha(colour, a * alpha);
    let top = y + look.gap * s;
    match shown {
        Shown::Ready { texture, size } => {
            let [w, h] = fitted(look, Some(size), width / s);
            let _ = canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: Rect::new(x, top, w * s, h * s),
                texture,
                color: Color::new(1.0, 1.0, 1.0, alpha),
            });
        }
        Shown::Loading => {
            let [w, h] = fitted(look, None, width / s);
            let rect = Rect::new(x, top, w * s, h * s);
            let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                rect,
                radius: 6.0 * s,
                color: faded(color::HOLO, 0.07),
            });
            text(
                canvas,
                TextFamily::Body,
                format_args!("Loading GIF"),
                rect,
                look.text_size * s,
                faded(color::QUIET, 1.0),
                FontWeight::Regular,
                TextAlign::Center,
            );
        }
        Shown::Unavailable => text(
            canvas,
            TextFamily::Body,
            format_args!("GIF unavailable"),
            Rect::new(x, y, width, (look.note - 2.0) * s),
            look.text_size * s,
            faded(color::QUIET, 1.0),
            FontWeight::Regular,
            TextAlign::Start,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gif_keeps_its_shape_within_the_column() {
        assert_eq!(fitted(PAGE, Some([200, 200]), 1_180.0), [160.0, 160.0]);
        assert_eq!(fitted(PAGE, Some([400, 200]), 1_180.0), [320.0, 160.0]);
        // Too wide for the column: as wide as it, lower.
        assert_eq!(fitted(DOCK, Some([480, 60]), 300.0), [300.0, 37.5]);
        assert_eq!(fitted(DOCK, None, 600.0), [72.0, 48.0]);
        assert_eq!(fitted(DOCK, Some([0, 0]), 600.0), [72.0, 48.0]);
    }

    #[test]
    fn the_block_takes_its_height_while_loading_and_a_line_when_unavailable() {
        let ready = Shown::Ready {
            texture: super::super::slot_texture(0),
            size: [200, 200],
        };
        assert_eq!(room(PAGE, Shown::Loading), room(PAGE, ready));
        assert_eq!(room(PAGE, ready), 166.0);
        // On the docks, a one-row message with its GIF and two more fill the box.
        assert_eq!(room(DOCK, ready), 52.0);
        assert_eq!(room(DOCK, Shown::Unavailable), 20.0);
    }
}
