//! A card drawn from shapes (`docs/cards.md`, "In the client"): a sealed card shows its
//! back, a dark plate edged in its rarity's colour with the holocron's gem at its heart,
//! its rarity above, "Sealed" under the gem and its edition stamped at the foot. A
//! revealed card shows its face ([`super::faces`]), laid out as a trading card: its name
//! and a dot of its rarity's colour on top, its picture in a gold-edged 6:5 window, a
//! strip with its kind and set and its edition and finish, its lore in a box, and at the
//! foot its number in its set, its artist and its rarity. The finish lies over it all:
//! Holo runs a rainbow sheen across the card, Solar a warm wash, a turning corona and a
//! band of sunlight, and a gold edge. One drawing serves the Cards tab, the opening's
//! ceremony and the card on show. A small card shows a face's picture without its words
//! ([`WORDS`], [`TITLE`]).
//!
//! A card is a fixed number of commands for its finish ([`commands`]): nothing is
//! allocated, and the canvas's room is counted by the tests.

use super::faces::{ART_ASPECT, Face};
use super::{Finish, Rarity};
use crate::holocrons::gem;
use crate::menu::sjk::{Frame, color, text};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use sjk_ui::{Color, DrawCommand, FontWeight, Gradient, TextAlign};

/// A card's height against its width.
pub(crate) const ASPECT: f32 = 1.4;
/// Gem rows for a card this wide (frame pixels) or wider, and fewer below it.
const BIG: f32 = 200.0;
const BIG_ROWS: usize = gem::ROWS;
const SMALL_ROWS: usize = 9;
/// The Holo sheen's bands and their hues, red to violet.
const HOLO_BANDS: [[f32; 3]; 6] = [
    [1.0, 0.35, 0.45],
    [1.0, 0.75, 0.3],
    [0.55, 1.0, 0.45],
    [0.3, 0.9, 1.0],
    [0.45, 0.5, 1.0],
    [0.85, 0.45, 1.0],
];
/// A card's corner radius against its width.
const CORNER: f32 = 0.07;
/// How far a card turns towards the pointer at its edge, radians (about 9 degrees), and
/// the eye's distance for the perspective, in card heights.
const TILT_MAX: f32 = 0.16;
const EYE: f32 = 2.6;
/// The Holo sweep: seconds from one pass to the next, and the share of them it takes to
/// cross the card (the rest, it waits off the card).
const HOLO_PERIOD: f32 = 7.0;
const HOLO_CROSSING: f32 = 0.6;
/// The Solar corona's arcs.
const CORONA: usize = 3;
/// A face's name shows on a card this wide (frame pixels) or wider, and all its words
/// from [`WORDS`]: below, they would be too small to read.
const TITLE: f32 = 200.0;
const WORDS: f32 = 300.0;
/// The face's lore lines at most.
const LORE_LINES: usize = 7;
/// Solar's gold and its deeper orange.
const SUN: Color = Color::new(1.0, 0.8, 0.32, 1.0);
const EMBER: Color = Color::new(1.0, 0.48, 0.16, 1.0);

/// What a card shows.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Card<'a> {
    pub(crate) rarity: &'static Rarity,
    pub(crate) finish: &'static Finish,
    /// "Alpha Edition", or empty for no stamp.
    pub(crate) edition: &'a str,
    /// Its face, once revealed and in the packs; `None` draws the sealed back.
    pub(crate) face: Option<Shown<'a>>,
    /// Where the pointer rests on it, when it does: the card tilts towards it and its
    /// light follows it.
    pub(crate) tilt: Option<Tilt>,
}

/// Where the pointer rests on a card, from its middle: -1 at its left (top) edge, 1 at
/// its right (bottom) edge.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Tilt {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

/// A revealed card's face as a card draws it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shown<'a> {
    pub(crate) face: &'a Face,
    /// Its position among the faces, for its picture's texture.
    pub(crate) index: usize,
    /// How many cards its set has in the packs.
    pub(crate) of: usize,
}

/// How many shape commands a card of `finish` that wide (frame pixels) takes, its words
/// apart.
#[cfg(test)]
pub(crate) fn commands(finish: &Finish, width: f32) -> usize {
    let rows = if width >= BIG { BIG_ROWS } else { SMALL_ROWS };
    let finish = match finish.index {
        0 => 0,
        1 => HOLO_BANDS.len() + 2,
        _ => 1 + CORONA + 3 + 1,
    };
    // Opacity, plate, wash, edges, ring, gem, stamp, clip.
    2 + 1 + 1 + 2 + 1 + gem::commands(rows) + 1 + 2 + finish
}

/// How many shape commands a revealed card's face takes, its words apart.
#[cfg(test)]
pub(crate) fn face_commands(finish: &Finish, width: f32) -> usize {
    let finish = match finish.index {
        0 => 0,
        1 => HOLO_BANDS.len() + 2,
        _ => 1 + CORONA + 3 + 1,
    };
    let boxes = if width >= WORDS { 2 } else { 0 };
    // Opacity, plate, wash, edge, the window's dark, picture and gold edge, the lore
    // box, clip.
    2 + 1 + 1 + 1 + 3 + boxes + 2 + finish
}

/// `colour` at `alpha` times its own.
fn faded(colour: Color, alpha: f32) -> Color {
    Color::new(colour.r, colour.g, colour.b, colour.a * alpha)
}

/// Draw `card` over `rect` (frame pixels: x, y, width; its height follows [`ASPECT`]) at
/// `alpha`, `seconds` into the page's clock for its finish's motion. `turn` (0 to 1) is
/// how far it has turned to face the player: it narrows about its middle as it turns,
/// and its words show only once it faces.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    [x, y, width]: [f32; 3],
    card: Card<'_>,
    seconds: f32,
    alpha: f32,
    turn: f32,
) {
    let Some(tilt) = card.tilt else {
        draw_flat(canvas, frame, [x, y, width], card, seconds, alpha, turn);
        return;
    };
    let height = width * ASPECT;
    let centre = frame.point(x + width * 0.5, y + height * 0.5);
    let _ = canvas
        .draw_list_mut()
        .push(DrawCommand::PushTransform(tilt_matrix(
            centre,
            height * frame.s,
            tilt,
        )));
    draw_flat(canvas, frame, [x, y, width], card, seconds, alpha, turn);
    let _ = canvas.draw_list_mut().push(DrawCommand::PopTransform);
}

/// The perspective that turns a card about `centre` (window pixels) towards the pointer
/// at `tilt`, the eye [`EYE`] times its `height` away: the side under the pointer
/// recedes, as if pressed.
pub(crate) fn tilt_matrix(centre: [f32; 2], height: f32, tilt: Tilt) -> [f32; 9] {
    let (yaw, pitch) = (
        tilt.x.clamp(-1.0, 1.0) * TILT_MAX,
        -tilt.y.clamp(-1.0, 1.0) * TILT_MAX,
    );
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let eye = height * EYE;
    let [ox, oy] = centre;
    // A point (u, v) of the card from its middle turns to x = u cos(yaw),
    // y = u sin(yaw) sin(pitch) + v cos(pitch), z = v sin(pitch) - u sin(yaw) cos(pitch),
    // and shows at the middle plus (x, y) times eye / (eye - z).
    let (zu, zv) = (-sy * cp, sp);
    let local = [
        eye * cy - ox * zu,
        -ox * zv,
        ox * eye,
        eye * sy * sp - oy * zu,
        eye * cp - oy * zv,
        oy * eye,
        -zu,
        -zv,
        eye,
    ];
    // Then from window points: (u, v) = (x - ox, y - oy).
    let mut out = [0.0; 9];
    for row in 0..3 {
        let [a, b, c] = [local[row * 3], local[row * 3 + 1], local[row * 3 + 2]];
        out[row * 3] = a;
        out[row * 3 + 1] = b;
        out[row * 3 + 2] = c - a * ox - b * oy;
    }
    out
}

/// [`draw`] without the tilt: the card as laid out, upright.
#[allow(clippy::too_many_arguments)]
fn draw_flat(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    [x, y, width]: [f32; 3],
    card: Card<'_>,
    seconds: f32,
    alpha: f32,
    turn: f32,
) {
    let s = frame.s;
    let height = width * ASPECT;
    let turn = turn.clamp(0.0, 1.0);
    let shown = (width * turn).max(1.0);
    let left = x + (width - shown) * 0.5;
    let rect = frame.rect(left, y, shown, height);
    let radius = width * CORNER * s;
    let unit = width / 220.0;
    let rarity = card.rarity;
    let edge = if card.finish.index == 2 {
        SUN
    } else {
        rarity.colour
    };
    let list = canvas.draw_list_mut();
    let _ = list.push(DrawCommand::PushOpacity(alpha.clamp(0.0, 1.0)));
    let _ = list.push(DrawCommand::RoundedRect {
        rect,
        radius,
        color: Color::new(0.035, 0.05, 0.1, 0.97),
    });
    let _ = list.push(DrawCommand::GradientRect {
        rect,
        radius,
        gradient: Gradient {
            start: rarity.colour_alpha(0.3),
            end: rarity.colour_alpha(0.04),
            vertical: true,
        },
    });
    let _ = list.push(DrawCommand::Border {
        rect,
        radius,
        width: (2.6 * unit * s).max(1.2),
        color: faded(edge, 0.95),
    });
    if let Some(shown_face) = card.face {
        face(
            canvas,
            frame,
            [x, y, width],
            [left, shown],
            card,
            shown_face,
            seconds,
            alpha,
            turn,
        );
        return;
    }
    let inset = 9.0 * unit * turn;
    let _ = list.push(DrawCommand::Border {
        rect: frame.rect(
            left + inset,
            y + 9.0 * unit,
            (shown - inset * 2.0).max(1.0),
            height - 18.0 * unit,
        ),
        radius: (radius - 6.0 * unit * s).max(1.0),
        width: (1.0 * s).max(1.0),
        color: Color::new(1.0, 1.0, 1.0, 0.12),
    });
    let [cx, cy] = frame.point(x + width * 0.5, y + height * 0.47);
    let _ = list.push(DrawCommand::Arc {
        center: [cx, cy],
        radius: width * 0.31 * s * turn.max(0.01),
        width: (1.6 * unit * s).max(1.0),
        start: 0.0,
        sweep: std::f32::consts::TAU,
        color: faded(edge, 0.4),
        knockout: None,
    });
    let rows = if width >= BIG { BIG_ROWS } else { SMALL_ROWS };
    gem::draw(
        list,
        [cx, cy],
        width * 0.2 * s * turn.max(0.01),
        rarity.colour,
        1.0,
        rows,
    );
    // The edition's stamp at the foot.
    let stamp = [
        x + width * 0.22,
        y + height - 40.0 * unit,
        width * 0.56,
        22.0 * unit,
    ];
    let _ = list.push(DrawCommand::RoundedRect {
        rect: frame.rect(
            stamp[0] + (stamp[2] - stamp[2] * turn) * 0.5,
            stamp[1],
            (stamp[2] * turn).max(1.0),
            stamp[3],
        ),
        radius: 11.0 * unit * s,
        color: if card.edition.is_empty() {
            Color::new(0.0, 0.0, 0.0, 0.0)
        } else {
            color::alpha(color::GOLD, 0.16)
        },
    });
    // The finish over everything, inside the card.
    finish_light(
        list,
        frame,
        card,
        [left, y, shown, height],
        width,
        [cx, cy],
        seconds,
    );
    let _ = list.push(DrawCommand::PopOpacity);
    if turn < 0.92 {
        return;
    }
    // The words, once it faces the player (text draws over every shape).
    let ink = alpha.clamp(0.0, 1.0);
    text(
        canvas,
        TextFamily::Display,
        format_args!(
            "{} {}",
            crate::symbols::card_rarity(rarity.index),
            rarity.name.to_uppercase()
        ),
        frame.rect(x, y + 16.0 * unit, width, 28.0 * unit),
        19.0 * unit * s,
        faded(rarity.colour, ink),
        FontWeight::Semibold,
        TextAlign::Center,
    );
    text(
        canvas,
        TextFamily::Body,
        format_args!("Sealed"),
        frame.rect(x, y + height * 0.47 + width * 0.33, width, 22.0 * unit),
        14.0 * unit * s,
        faded(color::MUTED, ink),
        FontWeight::Regular,
        TextAlign::Center,
    );
    if !card.edition.is_empty() {
        text(
            canvas,
            TextFamily::Body,
            format_args!("{}", card.edition),
            frame.rect(stamp[0], stamp[1] + 2.0 * unit, stamp[2], 18.0 * unit),
            12.0 * unit * s,
            faded(color::GOLD_BRIGHT, ink),
            FontWeight::Semibold,
            TextAlign::Center,
        );
    }
}

/// A revealed card's face over its plate (`[left, shown]` its turned span), then the
/// finish, then its words once it faces the player and is wide enough to read them.
#[allow(clippy::too_many_arguments)]
fn face(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    [x, y, width]: [f32; 3],
    [left, shown]: [f32; 2],
    card: Card<'_>,
    face: Shown<'_>,
    seconds: f32,
    alpha: f32,
    turn: f32,
) {
    let s = frame.s;
    let height = width * ASPECT;
    let rarity = card.rarity;
    // Across the card as it turns: a share of the width from its left.
    let across = |share: f32| left + share * shown;
    let list = canvas.draw_list_mut();
    // The picture's window: dark until its picture is uploaded, then the picture.
    let art = [
        across(0.0413),
        y + width * 0.119,
        (0.917 * shown).max(1.0),
        0.917 * width / ART_ASPECT,
    ];
    let window = frame.rect(art[0], art[1], art[2], art[3]);
    let _ = list.push(DrawCommand::RoundedRect {
        rect: window,
        radius: width * 0.012 * s,
        color: Color::new(0.02, 0.03, 0.06, 1.0),
    });
    let _ = list.push(DrawCommand::TexturedQuad {
        rect: window,
        texture: super::faces::texture(face.index),
        color: Color::new(1.0, 1.0, 1.0, 1.0),
    });
    let _ = list.push(DrawCommand::Border {
        rect: frame.rect(
            art[0] - width * 0.003,
            art[1] - width * 0.003,
            art[2] + width * 0.006,
            art[3] + width * 0.006,
        ),
        radius: width * 0.016 * s,
        width: (width * 0.0032 * s).max(1.0),
        color: color::GOLD_BRIGHT,
    });
    let lore_box = [
        across(0.0413),
        y + width * 0.945,
        (0.917 * shown).max(1.0),
        height - width * 1.065,
    ];
    if width >= WORDS {
        let rect = frame.rect(lore_box[0], lore_box[1], lore_box[2], lore_box[3]);
        let _ = list.push(DrawCommand::RoundedRect {
            rect,
            radius: width * 0.018 * s,
            color: Color::new(0.067, 0.1, 0.19, 0.92),
        });
        let _ = list.push(DrawCommand::Border {
            rect,
            radius: width * 0.018 * s,
            width: (width * 0.0016 * s).max(1.0),
            color: Color::new(0.23, 0.28, 0.43, 0.9),
        });
    }
    // The finish over everything, inside the card, its light about the picture.
    let centre = frame.point(x + width * 0.5, art[1] + art[3] * 0.5);
    finish_light(
        list,
        frame,
        card,
        [left, y, shown, height],
        width,
        centre,
        seconds,
    );
    let _ = list.push(DrawCommand::PopOpacity);
    if turn < 0.92 || width < TITLE {
        return;
    }
    let ink = alpha.clamp(0.0, 1.0);
    text(
        canvas,
        TextFamily::Display,
        format_args!("{}", face.face.name),
        frame.rect(
            x + width * 0.049,
            y + width * 0.03,
            width * 0.82,
            width * 0.08,
        ),
        width * 0.052 * s,
        faded(color::TEXT, ink),
        FontWeight::Semibold,
        TextAlign::Start,
    );
    // The rarity's symbol in the corner, in its colour.
    text(
        canvas,
        TextFamily::Display,
        format_args!("{}", crate::symbols::card_rarity(rarity.index)),
        frame.rect(
            x + width * 0.82,
            y + width * 0.03,
            width * 0.13,
            width * 0.08,
        ),
        width * 0.06 * s,
        faded(rarity.colour, ink),
        FontWeight::Semibold,
        TextAlign::End,
    );
    if width < WORDS {
        return;
    }
    let small = width * 0.025;
    let strip_y = art[1] + art[3] + width * 0.016;
    text(
        canvas,
        TextFamily::Body,
        format_args!("{}  ·  {}", face.face.kind.name(), face.face.set_name),
        frame.rect(x + width * 0.049, strip_y, width * 0.5, width * 0.036),
        small * s,
        faded(color::MUTED, ink),
        FontWeight::Semibold,
        TextAlign::Start,
    );
    let finish = if card.finish.index == 0 {
        String::new()
    } else {
        format!("  ·  {}", card.finish.name)
    };
    text(
        canvas,
        TextFamily::Body,
        format_args!("{}{finish}", card.edition),
        frame.rect(x + width * 0.45, strip_y, width * 0.501, width * 0.036),
        small * s,
        faded(color::GOLD_BRIGHT, ink),
        FontWeight::Semibold,
        TextAlign::End,
    );
    // The lore, cut into lines at spaces for the box's width at the player's text size.
    let lore_size = width * 0.033;
    let scale = crate::text::style::current().scale.max(0.5);
    let chars = (62.0 / scale).floor().max(16.0) as usize;
    let step = width * 0.046;
    for (index, line) in crate::menu::sjk::wrap(&face.face.lore, chars)
        .take(LORE_LINES)
        .enumerate()
    {
        text(
            canvas,
            TextFamily::Body,
            format_args!("{line}"),
            frame.rect(
                lore_box[0] + width * 0.028,
                lore_box[1] + width * 0.024 + index as f32 * step,
                lore_box[2] - width * 0.056,
                step,
            ),
            lore_size * s,
            faded(color::TEXT, ink),
            FontWeight::Regular,
            TextAlign::Start,
        );
    }
    let foot_y = y + height - width * 0.094;
    text(
        canvas,
        TextFamily::Code,
        format_args!("{:03} / {:03}", face.face.number, face.of.max(1)),
        frame.rect(x + width * 0.049, foot_y, width * 0.3, width * 0.04),
        small * s,
        faded(color::MUTED, ink),
        FontWeight::Regular,
        TextAlign::Start,
    );
    text(
        canvas,
        TextFamily::Body,
        format_args!("Art: {}", face.face.artist),
        frame.rect(x + width * 0.3, foot_y, width * 0.4, width * 0.04),
        small * s,
        faded(color::MUTED, ink),
        FontWeight::Semibold,
        TextAlign::Center,
    );
    text(
        canvas,
        TextFamily::Body,
        format_args!(
            "{} {}",
            crate::symbols::card_rarity(rarity.index),
            rarity.name
        ),
        frame.rect(x + width * 0.6, foot_y, width * 0.351, width * 0.04),
        small * s,
        faded(rarity.colour, ink),
        FontWeight::Semibold,
        TextAlign::End,
    );
}

/// The finish's light over a card at `[x, y, width, height]` (frame pixels, `full` its
/// unturned width), inside its border and rounded corners, about `centre` (window
/// pixels): Holo's bands or Solar's sun, following the pointer while it rests on the
/// card ([`Card::tilt`]), and a soft glare under the pointer whatever the finish.
fn finish_light(
    list: &mut sjk_ui::DrawList,
    frame: &Frame,
    card: Card<'_>,
    [x, y, width, height]: [f32; 4],
    full: f32,
    centre: [f32; 2],
    seconds: f32,
) {
    let s = frame.s;
    let unit = full / 220.0;
    // Inside the rarity's edge, so the light never lies over it.
    let inset = 3.5 * unit;
    let _ = list.push(DrawCommand::PushRoundedClip {
        rect: frame.rect(
            x + inset,
            y + inset,
            (width - inset * 2.0).max(1.0),
            height - inset * 2.0,
        ),
        radius: ((full * CORNER - inset) * s).max(0.0),
    });
    // Where the pointer is on the card, 0 to 1 each way: the light passes through it.
    let light = card.tilt.map(|tilt| {
        [
            0.5 + 0.5 * tilt.x.clamp(-1.0, 1.0),
            0.5 + 0.5 * tilt.y.clamp(-1.0, 1.0),
        ]
    });
    match card.finish.index {
        0 => {}
        1 => holo(
            list,
            frame,
            [x, y, width, height],
            full * CORNER,
            seconds,
            light,
        ),
        _ => solar(
            list,
            frame,
            [x, y, width, height],
            centre,
            full,
            seconds,
            light,
        ),
    }
    if let Some(tilt) = card.tilt {
        // The glare: soft discs stacked under the pointer.
        let [gx, gy] = frame.point(
            x + width * (0.5 + 0.5 * tilt.x.clamp(-1.0, 1.0)),
            y + height * (0.5 + 0.5 * tilt.y.clamp(-1.0, 1.0)),
        );
        for step in 0..4 {
            let radius = full * (0.55 - step as f32 * 0.12) * s;
            let _ = list.push(DrawCommand::RoundedRect {
                rect: sjk_ui::Rect::new(gx - radius, gy - radius, radius * 2.0, radius * 2.0),
                radius,
                color: Color::new(1.0, 1.0, 1.0, 0.04),
            });
        }
    }
    let _ = list.push(DrawCommand::PopClip);
}

/// Where a sweeping light's middle is at `seconds` (`period` seconds a pass): from
/// `from` to `to` over the first [`HOLO_CROSSING`] of the period, then off the card
/// until the next pass. `light` (0 to 1 across the card, the pointer's) holds it there
/// instead, at `x` plus that share of `width`.
fn sweep(
    seconds: f32,
    period: f32,
    [from, to]: [f32; 2],
    light: Option<f32>,
    [x, width]: [f32; 2],
) -> f32 {
    if let Some(light) = light {
        return x + width * light;
    }
    let phase = (seconds / period).fract() / HOLO_CROSSING;
    if phase >= 1.0 {
        return to;
    }
    // Eased in and out, so it does not start or stop all at once.
    let eased = phase * phase * (3.0 - 2.0 * phase);
    from + (to - from) * eased
}

/// A slanting stroke through `middle` (frame pixels, its point at the card's middle
/// height), long enough that its square ends lie far off a card `height` tall.
fn slanted(frame: &Frame, [mx, my]: [f32; 2], slope: f32, height: f32) -> ([f32; 2], [f32; 2]) {
    let reach = height * 1.2;
    (
        frame.point(mx + slope * reach, my - reach),
        frame.point(mx - slope * reach, my + reach),
    )
}

/// Holo: bands of the rainbow slanting across the card, drifting over it, and a bright
/// line where the light catches; `corner` is the card's corner radius (frame pixels).
/// The caller's rounded clip keeps the bands inside the card's corners.
#[allow(clippy::too_many_arguments)]
fn holo(
    list: &mut sjk_ui::DrawList,
    frame: &Frame,
    [x, y, width, height]: [f32; 4],
    corner: f32,
    seconds: f32,
    light: Option<[f32; 2]>,
) {
    let s = frame.s;
    let band = width * 0.16;
    // How far the bands lean across the card's height.
    let slope = 0.42 * 2.0;
    let lean = height * 0.42;
    let spread = band * 0.85 * (HOLO_BANDS.len() - 1) as f32;
    // The glint's place: from where every band is off the card on the left to where
    // every one is off it on the right.
    let glint = sweep(
        seconds,
        HOLO_PERIOD,
        [
            x - lean - band - spread * 0.5,
            x + width + lean + band + spread * 0.5,
        ],
        light.map(|[across, _]| across),
        [x, width],
    );
    // The bands pass through the pointer's height, or the card's middle.
    let middle = y + height * light.map_or(0.5, |[_, down]| down);
    let start = glint - spread * 0.5;
    for (index, [r, g, b]) in HOLO_BANDS.iter().enumerate() {
        let (from, to) = slanted(
            frame,
            [start + index as f32 * band * 0.85, middle],
            slope,
            height,
        );
        let _ = list.push(DrawCommand::Line {
            from,
            to,
            width: band * s,
            color: Color::new(*r, *g, *b, 0.13),
        });
    }
    // The glint: a thin white line at the bands' middle.
    let (from, to) = slanted(frame, [glint, middle], slope, height);
    let _ = list.push(DrawCommand::Line {
        from,
        to,
        width: band * 0.18 * s,
        color: Color::new(1.0, 1.0, 1.0, 0.28),
    });
    // A faint pearl over the whole card, rounded as the card is.
    let _ = list.push(DrawCommand::GradientRect {
        rect: frame.rect(x, y, width, height),
        radius: corner * s,
        gradient: Gradient {
            start: Color::new(0.75, 0.85, 1.0, 0.05),
            end: Color::new(1.0, 0.7, 0.95, 0.08),
            vertical: false,
        },
    });
}

/// Solar: a warm wash from the top, a corona of arcs turning round the gem, a band of
/// sunlight passing over, and embers' glow at the foot.
#[allow(clippy::too_many_arguments)]
fn solar(
    list: &mut sjk_ui::DrawList,
    frame: &Frame,
    [x, y, width, height]: [f32; 4],
    centre: [f32; 2],
    full: f32,
    seconds: f32,
    light: Option<[f32; 2]>,
) {
    let s = frame.s;
    let corner = full * CORNER;
    let _ = list.push(DrawCommand::GradientRect {
        rect: frame.rect(x, y, width, height),
        radius: corner * s,
        gradient: Gradient {
            start: faded(SUN, 0.24),
            end: faded(EMBER, 0.06),
            vertical: true,
        },
    });
    let squeeze = width / full.max(1.0);
    for index in 0..CORONA {
        let phase = index as f32 / CORONA as f32;
        let pulse = 0.5 + 0.5 * (seconds * 1.7 + phase * 6.0).sin();
        let _ = list.push(DrawCommand::Arc {
            center: centre,
            radius: full * (0.36 + 0.04 * index as f32) * s * squeeze.max(0.01),
            width: (2.0 + index as f32) * s * full / 220.0,
            start: seconds * (0.5 + 0.2 * index as f32) + phase * std::f32::consts::TAU,
            sweep: 1.1 + 0.5 * pulse,
            color: faded(if index == 1 { EMBER } else { SUN }, 0.35 + 0.35 * pulse),
            knockout: None,
        });
    }
    let lean = height * 0.35;
    let bx = sweep(
        seconds,
        HOLO_PERIOD * 1.4,
        [x - lean - width * 0.12, x + width + lean + width * 0.12],
        light.map(|[across, _]| across),
        [x, width],
    );
    let middle = y + height * light.map_or(0.5, |[_, down]| down);
    let (from, to) = slanted(frame, [bx, middle], 0.7, height);
    let _ = list.push(DrawCommand::Line {
        from,
        to,
        width: width * 0.22 * s,
        color: faded(SUN, 0.16),
    });
    let _ = list.push(DrawCommand::Line {
        from,
        to,
        width: width * 0.04 * s,
        color: Color::new(1.0, 0.97, 0.85, 0.35),
    });
    let _ = list.push(DrawCommand::GradientRect {
        rect: frame.rect(x, y + height * 0.72, width, height * 0.28),
        radius: corner * s,
        gradient: Gradient {
            start: faded(EMBER, 0.0),
            end: faded(EMBER, 0.2),
            vertical: true,
        },
    });
    let glow = 0.5 + 0.5 * (seconds * 2.3).sin();
    let _ = list.push(DrawCommand::Border {
        rect: frame.rect(x, y, width, height),
        radius: corner * s,
        width: (3.5 * s * full / 220.0).max(1.5),
        color: faded(SUN, 0.35 + 0.4 * glow),
    });
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Each finish takes the commands it says, at either gem size, turned or not.
    #[test]
    fn a_card_takes_a_fixed_number_of_commands() {
        let frame = Frame::new([1920.0, 1080.0]);
        for finish in &super::super::FINISHES {
            for width in [120.0, 320.0] {
                for turn in [0.3, 1.0] {
                    let mut canvas = MenuCanvas::with_capacities(16, 16, 512);
                    canvas.begin_transparent([1920.0, 1080.0]);
                    draw(
                        &mut canvas,
                        &frame,
                        [100.0, 100.0, width],
                        Card {
                            rarity: &super::super::RARITIES[2],
                            finish,
                            edition: "Alpha Edition",
                            face: None,
                            tilt: None,
                        },
                        1.5,
                        1.0,
                        turn,
                    );
                    let shapes = canvas
                        .draw_list()
                        .commands()
                        .iter()
                        .filter(|command| !matches!(command, DrawCommand::Text { .. }))
                        .count();
                    assert_eq!(shapes, commands(finish, width), "{} {width}", finish.id);
                }
            }
        }
    }

    fn map(m: [f32; 9], [x, y]: [f32; 2]) -> [f32; 2] {
        let w = m[6] * x + m[7] * y + m[8];
        [
            (m[0] * x + m[1] * y + m[2]) / w,
            (m[3] * x + m[4] * y + m[5]) / w,
        ]
    }

    /// Upright, a card maps to itself; tilted towards the pointer at its right edge, the
    /// right side recedes (narrower and nearer the middle) and the left comes forward,
    /// the middle staying where it is.
    #[test]
    fn a_tilt_turns_the_side_under_the_pointer_away() {
        let centre = [960.0, 540.0];
        let flat = tilt_matrix(centre, 600.0, Tilt::default());
        for point in [[700.0, 300.0], [1_200.0, 800.0], centre] {
            let [x, y] = map(flat, point);
            assert!((x - point[0]).abs() < 1e-3 && (y - point[1]).abs() < 1e-3);
        }
        let right = tilt_matrix(centre, 600.0, Tilt { x: 1.0, y: 0.0 });
        let middle = map(right, centre);
        assert!((middle[0] - centre[0]).abs() < 1e-3 && (middle[1] - centre[1]).abs() < 1e-3);
        let near_right = map(right, [1_160.0, 540.0]);
        let near_left = map(right, [760.0, 540.0]);
        assert!(near_right[0] - centre[0] < 200.0, "the right side recedes");
        assert!(
            centre[0] - near_left[0] > near_right[0] - centre[0],
            "the left comes forward"
        );
        // The receding side is shorter: its corners come together.
        let top = map(right, [1_160.0, 240.0]);
        let bottom = map(right, [1_160.0, 840.0]);
        assert!(bottom[1] - top[1] < 600.0);
        // Pointer at the foot: the foot recedes.
        let foot = tilt_matrix(centre, 600.0, Tilt { x: 0.0, y: 1.0 });
        let low = map(foot, [960.0, 840.0]);
        assert!(low[1] - centre[1] < 300.0);
    }

    /// The sweep crosses from one end to the other, eased, then waits off the card; the
    /// pointer holds it where it is.
    #[test]
    fn the_sweep_crosses_then_waits_and_the_pointer_holds_it() {
        let ends = [-100.0, 500.0];
        assert_eq!(sweep(0.0, 10.0, ends, None, [0.0, 400.0]), -100.0);
        let crossing = HOLO_CROSSING * 10.0;
        let halfway = sweep(crossing * 0.5, 10.0, ends, None, [0.0, 400.0]);
        assert!((halfway - 200.0).abs() < 1e-3);
        assert_eq!(
            sweep(crossing + 0.5, 10.0, ends, None, [0.0, 400.0]),
            500.0,
            "waiting"
        );
        assert_eq!(sweep(9.9, 10.0, ends, None, [0.0, 400.0]), 500.0);
        assert_eq!(
            sweep(10.0, 10.0, ends, None, [0.0, 400.0]),
            -100.0,
            "the next pass"
        );
        assert_eq!(sweep(3.0, 10.0, ends, Some(0.25), [0.0, 400.0]), 100.0);
    }

    /// A slanting stroke's ends lie far off the card, so its square ends never show.
    #[test]
    fn slanted_strokes_end_far_off_the_card() {
        let frame = Frame::new([1920.0, 1080.0]);
        let (from, to) = slanted(&frame, [500.0, 400.0], 0.84, 300.0);
        let [_, top] = frame.point(0.0, 400.0 - 150.0);
        let [_, bottom] = frame.point(0.0, 400.0 + 150.0);
        assert!(from[1] < top - 100.0 && to[1] > bottom + 100.0);
    }

    pub(crate) fn face() -> Face {
        Face {
            id: "sol_lore_001".to_owned(),
            name: "Toxiee and Sol on JoF Memorial".to_owned(),
            set: "sol_lore".to_owned(),
            set_name: "Sol lore".to_owned(),
            number: 1,
            kind: super::super::faces::Kind::Community,
            rarity: &super::super::RARITIES[3],
            lore: "Tox was simply chilling when a pestering fox started taking pictures and                    turning them into art for its card collection, who knows why."
                .to_owned(),
            artist: "Sol".to_owned(),
            art: "cards/sol_lore_001.jpg".to_owned(),
        }
    }

    /// A face takes the commands it says in each finish, with its words from the width
    /// that reads them: none on a small card, all of them on a big one.
    #[test]
    fn a_face_takes_a_fixed_number_of_commands_and_its_words_by_width() {
        let frame = Frame::new([1920.0, 1080.0]);
        let face = face();
        for finish in &super::super::FINISHES {
            for (width, words) in [(128.0, 0), (240.0, 1), (500.0, 7)] {
                let mut canvas = MenuCanvas::with_capacities(32, 128, 512);
                canvas.begin_transparent([1920.0, 1080.0]);
                draw(
                    &mut canvas,
                    &frame,
                    [100.0, 100.0, width],
                    Card {
                        rarity: face.rarity,
                        finish,
                        edition: "Alpha Edition",
                        face: Some(Shown {
                            face: &face,
                            index: 0,
                            of: 1,
                        }),
                        tilt: None,
                    },
                    1.5,
                    1.0,
                    1.0,
                );
                let commands = canvas.draw_list().commands();
                let shapes = commands
                    .iter()
                    .filter(|command| !matches!(command, DrawCommand::Text { .. }))
                    .count();
                assert_eq!(
                    shapes,
                    face_commands(finish, width),
                    "{} {width}",
                    finish.id
                );
                let runs: Vec<&str> = canvas.text_runs().collect();
                assert!(runs.len() >= words, "{width}: {runs:?}");
                if words == 0 {
                    assert!(runs.is_empty(), "{runs:?}");
                } else {
                    assert_eq!(runs[0], "Toxiee and Sol on JoF Memorial");
                }
                if width >= WORDS {
                    assert!(runs.contains(&"001 / 001"), "{runs:?}");
                    assert!(runs.contains(&"Art: Sol"), "{runs:?}");
                }
            }
        }
    }
}
