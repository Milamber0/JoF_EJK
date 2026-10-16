//! The wheel's ring in the SJK UI's look (`docs/sjk-ui.md`): the choices on a
//! dark navy band edged with holo line-work, the highlighted one grown inside a
//! gold ring over a lit sector, a gold arc on the middle disc pointing where the
//! mouse points, and in the middle the page's name, the highlighted choice's
//! name in gold and a dot for each page. On the wheel itself the previous and
//! next pages' names stand beside the ring and a line under it says how to
//! change page. The in-game wheel and the Settings editor's preview both draw it.
//!
//! Sizes are pixels of a 1080-line window times [`Ring::unit`]; the navy is deep
//! enough that every line reads over any scene.

use super::pages::MAX_CHOICES;
use crate::menu::sjk::{color, text, wrap};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign, TextureId};
use std::f32::consts::{FRAC_PI_2, TAU};

/// Where the choices sit from the middle, and the band they sit on.
pub(crate) const RADIUS: f32 = 196.0;
const BAND: f32 = 128.0;
/// A choice's picture, and the highlighted one's.
const ICON: f32 = 84.0;
const ICON_LIT: f32 = 102.0;
/// The middle disc.
const HUB: f32 = 120.0;
/// The band's outer edge, and the farthest anything of the ring reaches.
const OUTER: f32 = RADIUS + BAND * 0.5;
pub(crate) const REACH: f32 = OUTER + 18.0;
/// How far a page's choices turn as they arrive, in radians.
const ARRIVAL_TURN: f32 = 0.38;
/// What an empty page says, on two lines.
pub(crate) const EMPTY: [&str; 2] = ["Nothing here yet", "Add choices in Settings"];

/// One choice as the ring shows it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Choice<'a> {
    pub(crate) label: &'a str,
    /// Its picture in the UI atlas (a wheel icon, a Force power's); without one,
    /// a disc with its name.
    pub(crate) icon: Option<TextureId>,
    /// The picture's size in pixels when it is not square (a favourite GIF): it is
    /// drawn whole inside the choice's square, on a dark disc.
    pub(crate) size: Option<[u32; 2]>,
    /// In effect: a gold dot outside the band.
    pub(crate) on: bool,
}

/// What to draw and where.
pub(crate) struct Ring<'a> {
    /// The middle, in window pixels.
    pub(crate) centre: [f32; 2],
    /// Window pixels per pixel of a 1080-line window.
    pub(crate) unit: f32,
    pub(crate) page: &'a str,
    pub(crate) choices: &'a [Choice<'a>],
    pub(crate) highlighted: Option<usize>,
    /// Where the pointer points (radians clockwise from straight up) and how far
    /// out it is, 0 in the middle to 1 past the dead zone.
    pub(crate) pointer: Option<(f32, f32)>,
    /// This page's index and how many pages there are.
    pub(crate) pages: (usize, usize),
    /// The previous and next pages' names, beside the ring.
    pub(crate) neighbours: Option<(&'a str, &'a str)>,
    /// How far this page's arrival has come (1 once settled) and which way the
    /// switch went (-1 back, 1 on).
    pub(crate) arrival: (f32, f32),
    /// The line under the ring saying how to change page.
    pub(crate) hint: bool,
    /// What the middle says when the page has no choices ([`EMPTY`]).
    pub(crate) empty: &'a [&'a str],
}

/// Angle of choice `index` of `count`, clockwise from straight up.
pub(crate) fn angle(index: usize, count: usize) -> f32 {
    index as f32 * TAU / count.max(1) as f32
}

fn push(canvas: &mut MenuCanvas, command: DrawCommand) {
    let _ = canvas.draw_list_mut().push(command);
}

/// A ring (or part of one) of `width` at `radius` round `centre`, centred on
/// `angle` (clockwise from up) and `sweep` long.
fn arc(
    canvas: &mut MenuCanvas,
    centre: [f32; 2],
    radius: f32,
    width: f32,
    angle: f32,
    sweep: f32,
    colour: Color,
) {
    push(
        canvas,
        DrawCommand::Arc {
            center: centre,
            radius,
            width,
            // The renderer's angles run clockwise from the right.
            start: angle - FRAC_PI_2 - sweep * 0.5,
            sweep,
            color: colour,
            knockout: None,
        },
    );
}

/// A disc `size` across centred on `at`.
fn disc(canvas: &mut MenuCanvas, at: [f32; 2], size: f32, colour: Color) {
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect: Rect::new(at[0] - size * 0.5, at[1] - size * 0.5, size, size),
            radius: size * 0.5,
            color: colour,
        },
    );
}

/// The point `radius` out from `centre` at `angle` (clockwise from up).
fn towards(centre: [f32; 2], angle: f32, radius: f32) -> [f32; 2] {
    [
        centre[0] + angle.sin() * radius,
        centre[1] - angle.cos() * radius,
    ]
}

/// A picture of `pixels` drawn whole in the middle of `square`, keeping its shape.
fn fit_inside(square: Rect, pixels: [u32; 2]) -> Rect {
    let [w, h] = [pixels[0].max(1) as f32, pixels[1].max(1) as f32];
    let scale = (square.width / w).min(square.height / h);
    let [w, h] = [w * scale, h * scale];
    Rect::new(
        square.x + (square.width - w) * 0.5,
        square.y + (square.height - h) * 0.5,
        w,
        h,
    )
}

/// The ease of a page arriving: quick, then settling.
fn ease(progress: f32) -> f32 {
    let rest = 1.0 - progress.clamp(0.0, 1.0);
    1.0 - rest * rest * rest
}

/// How much smaller the pictures are drawn for `count` choices: as large as for
/// ten up to ten, then shrunk with their share of the ring (the Force page's
/// twelve), so the gap between them stays.
fn icon_scale(count: usize) -> f32 {
    (MAX_CHOICES as f32 / count.max(1) as f32).min(1.0)
}

/// Draw `ring` into `canvas`.
pub(crate) fn draw(canvas: &mut MenuCanvas, ring: &Ring<'_>) {
    let u = ring.unit;
    let centre = ring.centre;
    let count = ring.choices.len();
    let step = TAU / count.max(1) as f32;
    let arrived = ease(ring.arrival.0);
    let turn = -ring.arrival.1 * (1.0 - arrived) * ARRIVAL_TURN;
    let fit = icon_scale(count);
    // The ground: a soft halo round the outside, the band from the middle
    // disc's rim to the outer edge, the middle disc darker still.
    arc(
        canvas,
        centre,
        (OUTER + 6.0) * u,
        16.0 * u,
        0.0,
        TAU,
        color::alpha(color::SPACE, 0.24),
    );
    arc(
        canvas,
        centre,
        (OUTER + HUB) * 0.5 * u,
        (OUTER - HUB) * u,
        0.0,
        TAU,
        color::alpha(color::SPACE, 0.66),
    );
    disc(
        canvas,
        centre,
        HUB * 2.0 * u,
        color::alpha(color::SPACE, 0.82),
    );
    // Holo line-work: the band's outer edge, the disc's rim, a tick between
    // choices.
    let hairline = (1.5 * u).max(1.0);
    arc(
        canvas,
        centre,
        OUTER * u,
        hairline,
        0.0,
        TAU,
        color::alpha(color::HOLO, 0.38),
    );
    arc(
        canvas,
        centre,
        HUB * u,
        hairline,
        0.0,
        TAU,
        color::alpha(color::HOLO, 0.34),
    );
    if count > 1 {
        for index in 0..count {
            arc(
                canvas,
                centre,
                (OUTER - 6.0) * u,
                12.0 * u,
                angle(index, count) + step * 0.5 + turn,
                2.0 / OUTER,
                color::alpha(color::HOLO, 0.5 * arrived),
            );
        }
    }
    // The highlighted choice: a gold glow behind it and a gold arc on the rim
    // of its share of the ring.
    if let Some(index) = ring.highlighted.filter(|index| *index < count) {
        let at = angle(index, count) + turn;
        let spot = towards(centre, at, RADIUS * u);
        disc(
            canvas,
            spot,
            (ICON_LIT + 34.0) * fit * u,
            color::alpha(color::GOLD, 0.07),
        );
        disc(
            canvas,
            spot,
            (ICON_LIT + 16.0) * fit * u,
            color::alpha(color::GOLD, 0.12),
        );
        // At most a choice's share of a full ring of eight, so a page of two
        // does not light half the ring.
        arc(
            canvas,
            centre,
            (OUTER + 3.0) * u,
            4.0 * u,
            at,
            (step * 0.72).min(TAU / 8.0 * 0.72),
            color::GOLD_BRIGHT,
        );
    }
    // The pointer: a gold arc on the disc's rim, brighter as it leaves the middle.
    if let Some((direction, strength)) = ring.pointer.filter(|(_, strength)| *strength > 0.05) {
        arc(
            canvas,
            centre,
            HUB * u,
            4.0 * u,
            direction,
            0.46,
            color::alpha(color::GOLD, 0.3 + 0.7 * strength.min(1.0)),
        );
    }
    canvas.push_opacity(0.2 + 0.8 * arrived);
    for (index, choice) in ring.choices.iter().enumerate() {
        let at = angle(index, count) + turn;
        let spot = towards(centre, at, RADIUS * u);
        let lit = ring.highlighted == Some(index);
        let size = if lit { ICON_LIT } else { ICON } * fit * u;
        if lit {
            arc(
                canvas,
                spot,
                size * 0.5 + 5.0 * u,
                2.5 * u,
                0.0,
                TAU,
                color::GOLD_BRIGHT,
            );
        }
        let rect = Rect::new(spot[0] - size * 0.5, spot[1] - size * 0.5, size, size);
        match choice.icon {
            Some(texture) => {
                let rect = match choice.size {
                    Some(pixels) => {
                        disc(canvas, spot, size, color::alpha(color::SPACE, 0.75));
                        fit_inside(rect, pixels)
                    }
                    // An emote's own picture has no disc of its own, as the wheel's do.
                    None if crate::emote_icons::is_emote_texture(texture) => {
                        disc(canvas, spot, size, color::alpha(color::SPACE, 0.75));
                        rect
                    }
                    None => rect,
                };
                push(
                    canvas,
                    DrawCommand::TexturedQuad {
                        rect,
                        texture,
                        color: Color::new(1.0, 1.0, 1.0, if lit { 1.0 } else { 0.86 }),
                    },
                );
            }
            None => name_disc(canvas, choice.label, rect, lit, u),
        }
        if choice.on {
            disc(
                canvas,
                towards(centre, at, (OUTER + 14.0) * u),
                9.0 * u,
                color::GOLD_BRIGHT,
            );
        }
    }
    canvas.pop_opacity();
    middle(canvas, ring, arrived);
    if let Some((previous, next)) = ring.neighbours.filter(|_| ring.pages.1 > 1) {
        neighbours(canvas, ring, previous, next);
    }
    if ring.hint && ring.pages.1 > 1 {
        let y = centre[1] + (REACH + 24.0) * u;
        let width = 230.0 * u;
        push(
            canvas,
            DrawCommand::RoundedRect {
                rect: Rect::new(centre[0] - width * 0.5, y - 15.0 * u, width, 30.0 * u),
                radius: 15.0 * u,
                color: color::alpha(color::SPACE, 0.6),
            },
        );
        text(
            canvas,
            TextFamily::Body,
            format_args!("Scroll to change page"),
            Rect::new(centre[0] - width * 0.5, y - 12.0 * u, width, 24.0 * u),
            15.0 * u,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Center,
        );
    }
}

/// A choice without a picture: a navy disc with a holo rim (gold when lit) and
/// its name on up to two lines.
fn name_disc(canvas: &mut MenuCanvas, label: &str, rect: Rect, lit: bool, u: f32) {
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect,
            radius: rect.width * 0.5,
            color: Color::new(0.05, 0.08, 0.15, 0.95),
        },
    );
    push(
        canvas,
        DrawCommand::Border {
            rect,
            radius: rect.width * 0.5,
            width: (1.5 * u).max(1.0),
            color: if lit {
                color::GOLD_BRIGHT
            } else {
                color::alpha(color::HOLO, 0.55)
            },
        },
    );
    let lines = wrap(label, 10).take(3).count();
    let shown = lines.min(2);
    let size = 15.0 * u;
    let line = size * 1.15;
    let top = rect.y + rect.height * 0.5 - line * shown as f32 * 0.5;
    for (index, part) in wrap(label, 10).take(shown).enumerate() {
        // A third line's words stay on the second, cut short: from where the
        // second line starts in the name (the lines are slices of it).
        let rest = if index + 1 == shown && lines > shown {
            let at = (part.as_ptr() as usize).saturating_sub(label.as_ptr() as usize);
            label.get(at..).unwrap_or(part)
        } else {
            part
        };
        text(
            canvas,
            TextFamily::Display,
            format_args!("{rest}"),
            Rect::new(
                rect.x + 6.0 * u,
                top + index as f32 * line,
                rect.width - 12.0 * u,
                line,
            ),
            size,
            if lit { color::GOLD_BRIGHT } else { color::TEXT },
            FontWeight::Semibold,
            TextAlign::Center,
        );
    }
}

/// The middle: the page's name, the highlighted choice's name and the page dots.
fn middle(canvas: &mut MenuCanvas, ring: &Ring<'_>, arrived: f32) {
    let u = ring.unit;
    let [x, y] = ring.centre;
    let width = HUB * 1.7 * u;
    let slide = ring.arrival.1 * (1.0 - arrived) * 26.0 * u;
    canvas.push_opacity(0.2 + 0.8 * arrived);
    text(
        canvas,
        TextFamily::Display,
        format_args!("{}", ring.page),
        Rect::new(x - width * 0.5 + slide, y - 58.0 * u, width, 28.0 * u),
        21.0 * u,
        color::MUTED,
        FontWeight::Semibold,
        TextAlign::Center,
    );
    canvas.pop_opacity();
    let label = ring
        .highlighted
        .and_then(|index| ring.choices.get(index))
        .map(|choice| choice.label);
    match label {
        Some(label) => text(
            canvas,
            TextFamily::Display,
            format_args!("{label}"),
            Rect::new(x - width * 0.5, y - 20.0 * u, width, 40.0 * u),
            30.0 * u,
            color::GOLD_BRIGHT,
            FontWeight::Semibold,
            TextAlign::Center,
        ),
        None if ring.choices.is_empty() => {
            for (line, words) in ring.empty.iter().enumerate() {
                text(
                    canvas,
                    TextFamily::Body,
                    format_args!("{words}"),
                    Rect::new(
                        x - width * 0.5,
                        y - 16.0 * u + line as f32 * 22.0 * u,
                        width,
                        22.0 * u,
                    ),
                    15.0 * u,
                    color::MUTED,
                    FontWeight::Regular,
                    TextAlign::Center,
                );
            }
        }
        None => {}
    }
    let (page, pages) = ring.pages;
    if pages > 1 {
        let gap = 15.0 * u;
        let lit = 20.0 * u;
        let dot = 7.0 * u;
        let total = lit + (pages - 1) as f32 * gap;
        let mut left = x - total * 0.5;
        let dots_y = y + 46.0 * u;
        for index in 0..pages {
            if index == page {
                push(
                    canvas,
                    DrawCommand::RoundedRect {
                        rect: Rect::new(left, dots_y - dot * 0.5, lit, dot),
                        radius: dot * 0.5,
                        color: color::GOLD_BRIGHT,
                    },
                );
                left += lit + gap - dot;
            } else {
                disc(
                    canvas,
                    [left + dot * 0.5, dots_y],
                    dot,
                    color::alpha(color::HOLO, 0.5),
                );
                left += gap;
            }
        }
    }
}

/// The previous page's name left of the ring and the next one's right of it,
/// each with the mouse button that goes there, on a navy pill.
fn neighbours(canvas: &mut MenuCanvas, ring: &Ring<'_>, previous: &str, next: &str) {
    let u = ring.unit;
    let [x, y] = ring.centre;
    let gap = (REACH + 14.0) * u;
    for (name, button, left) in [(previous, "Left click", true), (next, "Right click", false)] {
        // Rajdhani SemiBold at 22 is about 10 pixels a character.
        let width = ((name.chars().count() as f32 * 10.5 + 44.0).clamp(130.0, 250.0)) * u;
        let rect_x = if left { x - gap - width } else { x + gap };
        push(
            canvas,
            DrawCommand::RoundedRect {
                rect: Rect::new(rect_x, y - 30.0 * u, width, 60.0 * u),
                radius: 14.0 * u,
                color: color::alpha(color::SPACE, 0.62),
            },
        );
        let inset = 16.0 * u;
        let (align, caret) = if left {
            (TextAlign::End, "\u{2039} ")
        } else {
            (TextAlign::Start, "")
        };
        let after = if left { "" } else { " \u{203a}" };
        text(
            canvas,
            TextFamily::Display,
            format_args!("{caret}{name}{after}"),
            Rect::new(rect_x + inset, y - 24.0 * u, width - inset * 2.0, 28.0 * u),
            22.0 * u,
            color::TEXT,
            FontWeight::Semibold,
            align,
        );
        text(
            canvas,
            TextFamily::Body,
            format_args!("{button}"),
            Rect::new(rect_x + inset, y + 3.0 * u, width - inset * 2.0, 22.0 * u),
            14.0 * u,
            color::MUTED,
            FontWeight::Regular,
            align,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::pages::MAX_FORCE_CHOICES;
    use super::*;

    #[test]
    fn choices_go_clockwise_from_the_top() {
        let centre = [100.0, 100.0];
        let top = towards(centre, angle(0, 4), 10.0);
        let right = towards(centre, angle(1, 4), 10.0);
        let bottom = towards(centre, angle(2, 4), 10.0);
        assert!((top[0] - 100.0).abs() < 1e-4 && (top[1] - 90.0).abs() < 1e-4);
        assert!((right[0] - 110.0).abs() < 1e-4 && (right[1] - 100.0).abs() < 1e-4);
        assert!((bottom[1] - 110.0).abs() < 1e-4);
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        assert!(ease(0.5) > 0.8, "quick, then settling");
    }

    #[test]
    fn ten_choices_fit_the_ring_with_room_between_them() {
        // Ten round icons round the ring leave a gap, the grown one included;
        // the Force page's twelve, drawn smaller, as much.
        for count in [MAX_CHOICES, MAX_FORCE_CHOICES] {
            let room = TAU * RADIUS / count as f32;
            let fit = icon_scale(count);
            assert!(
                room - (ICON + ICON_LIT) * 0.5 * fit > 12.0,
                "{count}: {room}"
            );
        }
        assert_eq!(icon_scale(MAX_CHOICES), 1.0);
        assert!(icon_scale(MAX_FORCE_CHOICES) > 0.8, "still readable");
    }

    // The middle disc stays inside the band's inner edge.
    const _: () = assert!(HUB < RADIUS - BAND * 0.5);
}
