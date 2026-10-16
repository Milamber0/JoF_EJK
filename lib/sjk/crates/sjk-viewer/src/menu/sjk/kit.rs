//! The SJK UI's controls, as the kit sheet of its design draws them: the focused
//! row's band, the switch, the tick box, the slider, segments, the field that
//! opens a list, the list itself, the reset arrow, the changed dot,
//! sub-headings, the search pill, the lit rail, and the pop-up card over its
//! scrim. Every position is in frame pixels ([`Frame`]); text
//! goes through [`text`] in the UI's families.

use super::{Frame, color, text};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};

/// Height of a framed control (a field, segments, the search pill).
pub(crate) const CONTROL_HEIGHT: f32 = 38.0;
/// A switch's track.
const SWITCH: [f32; 2] = [52.0, 28.0];
/// A slider's knob and track thickness.
const KNOB: f32 = 20.0;
const TRACK: f32 = 4.0;
/// A tick box's side.
pub(crate) const TICK_BOX: f32 = 26.0;

fn push(canvas: &mut MenuCanvas, command: DrawCommand) {
    let _ = canvas.draw_list_mut().push(command);
}

/// A filled pill (radius half its height) over frame rectangle `rect`.
fn pill(canvas: &mut MenuCanvas, frame: &Frame, rect: [f32; 4], colour: Color) {
    let [x, y, width, height] = rect;
    let rect = frame.rect(x, y, width, height);
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect,
            radius: rect.height * 0.5,
            color: colour,
        },
    );
}

/// A pill's outline over frame rectangle `rect`.
fn outline(canvas: &mut MenuCanvas, frame: &Frame, rect: [f32; 4], colour: Color) {
    let [x, y, width, height] = rect;
    let rect = frame.rect(x, y, width, height);
    push(
        canvas,
        DrawCommand::Border {
            rect,
            radius: rect.height * 0.5,
            width: 1.5 * frame.s,
            color: colour,
        },
    );
}

/// A control's outline colour: white while focused, holo otherwise.
fn edge(focused: bool) -> Color {
    if focused {
        Color::new(1.0, 1.0, 1.0, 0.7)
    } else {
        color::alpha(color::HOLO, 0.4)
    }
}

/// The focused row's band over `rect` (frame pixels), with its gold bar.
pub(crate) fn band(canvas: &mut MenuCanvas, frame: &Frame, rect: [f32; 4]) {
    let [x, y, width, height] = rect;
    let s = frame.s;
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect: frame.rect(x, y, width, height),
            radius: 10.0 * s,
            color: color::alpha(color::HOLO, 0.09),
        },
    );
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect: frame.rect(x, y + height * 0.2, 4.0, height * 0.6),
            radius: 2.0 * s,
            color: color::GOLD_BRIGHT,
        },
    );
}

/// Darkness over the whole window under a pop-up card, deep enough that the
/// card reads as the one thing on screen while the map still shows round it.
pub(crate) fn scrim(canvas: &mut MenuCanvas, viewport: [f32; 2]) {
    push(
        canvas,
        DrawCommand::SolidRect {
            rect: Rect::new(0.0, 0.0, viewport[0], viewport[1]),
            color: color::alpha(color::SPACE, 0.78),
        },
    );
}

/// A pop-up card over `rect` (frame pixels): its drop shadow, the dark glass
/// and a holo edge. Text draws over every shape, so whatever the card covers
/// must be left out by its screen.
pub(crate) fn card(canvas: &mut MenuCanvas, frame: &Frame, rect: [f32; 4]) {
    let [x, y, width, height] = rect;
    let s = frame.s;
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect: frame.rect(x + 4.0, y + 10.0, width, height),
            radius: 18.0 * s,
            color: color::alpha(color::SPACE, 0.6),
        },
    );
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect: frame.rect(x, y, width, height),
            radius: 18.0 * s,
            color: Color::new(0.04, 0.06, 0.12, 0.98),
        },
    );
    push(
        canvas,
        DrawCommand::Border {
            rect: frame.rect(x, y, width, height),
            radius: 18.0 * s,
            width: 1.5 * s,
            color: color::alpha(color::HOLO, 0.45),
        },
    );
}

/// A tick box whose left edge is `x`, centred on `y`: an outlined square, gold
/// with a dark tick when `ticked`, ringed while `focused`. Returns its right
/// edge.
pub(crate) fn tick(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    x: f32,
    y: f32,
    ticked: bool,
    focused: bool,
) -> f32 {
    let size = TICK_BOX;
    let s = frame.s;
    let rect = frame.rect(x, y - size * 0.5, size, size);
    if ticked {
        push(
            canvas,
            DrawCommand::RoundedRect {
                rect,
                radius: 6.0 * s,
                color: color::GOLD,
            },
        );
        // The tick, a short stroke down then a long one up, laid as
        // overlapping dots along its two strokes.
        let ink = Color::new(0.078, 0.063, 0.02, 1.0);
        let corner = [x + size * 0.42, y + size * 0.24];
        let strokes = [
            ([x + size * 0.2, y - size * 0.02], corner),
            (corner, [x + size * 0.8, y - size * 0.26]),
        ];
        let dot = 3.6;
        for (from, to) in strokes {
            let steps = ((to[0] - from[0]).hypot(to[1] - from[1]) / 0.8).ceil() as usize;
            for step in 0..=steps {
                let t = step as f32 / steps as f32;
                let [cx, cy] = [
                    from[0] + (to[0] - from[0]) * t,
                    from[1] + (to[1] - from[1]) * t,
                ];
                pill(
                    canvas,
                    frame,
                    [cx - dot * 0.5, cy - dot * 0.5, dot, dot],
                    ink,
                );
            }
        }
    } else {
        push(
            canvas,
            DrawCommand::RoundedRect {
                rect,
                radius: 6.0 * s,
                color: color::alpha(color::SPACE, 0.85),
            },
        );
        push(
            canvas,
            DrawCommand::Border {
                rect,
                radius: 6.0 * s,
                width: 1.5 * s,
                color: color::alpha(color::HOLO, 0.7),
            },
        );
    }
    if focused {
        push(
            canvas,
            DrawCommand::Border {
                rect: frame.rect(x - 4.0, y - size * 0.5 - 4.0, size + 8.0, size + 8.0),
                radius: 9.0 * s,
                width: 1.5 * s,
                color: edge(true),
            },
        );
    }
    x + size
}

/// A switch ending at `right`, centred on `y`: a pill, gold with its knob right
/// when on, dark with its knob left when off, then On or Off after it. Returns
/// its left edge.
pub(crate) fn switch(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    right: f32,
    y: f32,
    on: bool,
    focused: bool,
) -> f32 {
    let label_width = 34.0;
    let x = right - label_width - 12.0 - SWITCH[0];
    let track = [x, y - SWITCH[1] * 0.5, SWITCH[0], SWITCH[1]];
    if on {
        pill(canvas, frame, track, color::GOLD);
    } else {
        pill(canvas, frame, track, color::alpha(color::SPACE, 0.85));
        outline(canvas, frame, track, color::alpha(color::HOLO, 0.45));
    }
    if focused {
        let [tx, ty, tw, th] = track;
        outline(
            canvas,
            frame,
            [tx - 3.0, ty - 3.0, tw + 6.0, th + 6.0],
            edge(true),
        );
    }
    let knob = if on {
        x + SWITCH[0] - 4.0 - KNOB
    } else {
        x + 4.0
    };
    pill(
        canvas,
        frame,
        [knob, y - KNOB * 0.5, KNOB, KNOB],
        if on {
            Color::new(1.0, 1.0, 1.0, 1.0)
        } else {
            color::MUTED
        },
    );
    text(
        canvas,
        TextFamily::Body,
        format_args!("{}", if on { "On" } else { "Off" }),
        frame.rect(right - label_width, y - 12.0, label_width, 24.0),
        17.0 * frame.s,
        if on || focused {
            color::TEXT
        } else {
            color::MUTED
        },
        FontWeight::Regular,
        TextAlign::End,
    );
    x
}

/// A slider's track from `x`, `width` long, centred on `y`, filled gold up to
/// `ratio`, its round knob ringed while `focused`.
pub(crate) fn slider(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    x: f32,
    y: f32,
    width: f32,
    ratio: f32,
    focused: bool,
) {
    let ratio = ratio.clamp(0.0, 1.0);
    pill(
        canvas,
        frame,
        [x, y - TRACK * 0.5, width, TRACK],
        color::alpha(color::HOLO, 0.22),
    );
    if ratio > 0.0 {
        pill(
            canvas,
            frame,
            [x, y - TRACK * 0.5, width * ratio, TRACK],
            color::GOLD,
        );
    }
    let knob = x + width * ratio;
    if focused {
        pill(
            canvas,
            frame,
            [
                knob - KNOB * 0.5 - 5.0,
                y - KNOB * 0.5 - 5.0,
                KNOB + 10.0,
                KNOB + 10.0,
            ],
            color::alpha(color::GOLD, 0.35),
        );
    }
    pill(
        canvas,
        frame,
        [knob - KNOB * 0.5, y - KNOB * 0.5, KNOB, KNOB],
        color::TEXT,
    );
}

/// The width a segment of `label` takes: Rajdhani SemiBold at 18 is about 8.6
/// pixels a character, with padding either side.
fn segment_width(label: &str) -> f32 {
    32.0 + 8.6 * label.chars().count() as f32
}

/// The width of [`segments`] showing `labels`.
pub(crate) fn segments_width(labels: &[&str]) -> f32 {
    labels.iter().map(|label| segment_width(label)).sum::<f32>() + 6.0
}

/// Segments ending at `right`, centred on `y`: every choice side by side in
/// one pill, the one in use filled gold; choice `i` answers to
/// `token_base + i`. `labels` are shown in sentence case.
#[allow(clippy::too_many_arguments)]
pub(crate) fn segments(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    right: f32,
    y: f32,
    labels: &[&str],
    current: usize,
    focused: bool,
    token_base: u16,
) {
    let inset = 3.0;
    let total: f32 = labels.iter().map(|label| segment_width(label)).sum::<f32>() + inset * 2.0;
    let mut x = right - total;
    let top = y - CONTROL_HEIGHT * 0.5;
    outline(
        canvas,
        frame,
        [x, top, total, CONTROL_HEIGHT],
        edge(focused),
    );
    x += inset;
    for (index, label) in labels.iter().enumerate() {
        let width = segment_width(label);
        let token = token_base + index as u16;
        let segment = [x, top + inset, width, CONTROL_HEIGHT - inset * 2.0];
        let hovered = canvas.token_hovered(token);
        if index == current {
            pill(canvas, frame, segment, color::GOLD);
        } else if hovered {
            pill(canvas, frame, segment, color::alpha(color::HOLO, 0.16));
        }
        let colour = match (index == current, hovered) {
            (true, _) => Color::new(0.078, 0.063, 0.02, 1.0),
            (false, true) => color::TEXT,
            (false, false) => color::MUTED,
        };
        text(
            canvas,
            TextFamily::Display,
            format_args!("{}", crate::menu::classic::view::Sentence(label)),
            frame.rect(x, y - 13.0, width, 26.0),
            20.0 * frame.s,
            colour,
            FontWeight::Regular,
            TextAlign::Center,
        );
        let [sx, sy, sw, sh] = segment;
        canvas.hit_region(token, frame.rect(sx, sy, sw, sh));
        x += width;
    }
}

/// A field over `rect` showing `value`, with a chevron when it opens a list
/// (`caret`), outlined white while focused.
pub(crate) fn field(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    rect: [f32; 4],
    value: std::fmt::Arguments<'_>,
    focused: bool,
    caret: bool,
) {
    let [x, y, width, height] = rect;
    pill(canvas, frame, rect, color::alpha(color::SPACE, 0.6));
    outline(canvas, frame, rect, edge(focused));
    if focused && !caret && crate::text_entry::selection_shown() {
        // Ctrl+A: the field's text is selected.
        let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
            rect: frame.rect(x + 10.0, y + 6.0, width - 20.0, height - 12.0),
            color: color::alpha(color::HOLO, 0.32),
        });
    }
    text(
        canvas,
        TextFamily::Body,
        value,
        frame.rect(
            x + 16.0,
            y + (height - 24.0) * 0.5,
            width - if caret { 44.0 } else { 28.0 },
            24.0,
        ),
        17.0 * frame.s,
        if focused {
            color::TEXT
        } else {
            color::alpha(color::TEXT, 0.9)
        },
        FontWeight::Regular,
        TextAlign::Start,
    );
    if caret {
        chevron(
            canvas,
            frame,
            x + width - 22.0,
            y + height * 0.5,
            if focused { color::TEXT } else { color::MUTED },
        );
    }
}

/// A small downward caret centred on (`x`, `y`): the lower quarter of a ring,
/// one stroke with round ends.
fn chevron(canvas: &mut MenuCanvas, frame: &Frame, x: f32, y: f32, colour: Color) {
    caret(canvas, frame, x, y, std::f32::consts::FRAC_PI_2, colour);
}

/// A small caret centred on (`x`, `y`) pointing at angle `towards` (radians,
/// clockwise from right on screen: 0 right, π/2 down, π left): a quarter of a
/// ring bulging that way.
fn caret(canvas: &mut MenuCanvas, frame: &Frame, x: f32, y: f32, towards: f32, colour: Color) {
    let radius = 8.0;
    let back = radius * 0.75;
    push(
        canvas,
        DrawCommand::Arc {
            center: frame.point(x - towards.cos() * back, y - towards.sin() * back),
            radius: radius * frame.s,
            width: 2.4 * frame.s,
            start: towards - std::f32::consts::FRAC_PI_4,
            sweep: std::f32::consts::FRAC_PI_2,
            color: colour,
            knockout: None,
        },
    );
}

/// A sorted column's mark centred on (`x`, `y`): three short bars stacked
/// flush left, the longest on top while the column runs `descending` (the
/// most first) and at the bottom otherwise, as sort icons draw it.
pub(crate) fn sort_mark(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    x: f32,
    y: f32,
    descending: bool,
    colour: Color,
) {
    let widths = if descending {
        [13.0, 9.0, 5.0]
    } else {
        [5.0, 9.0, 13.0]
    };
    for (row, width) in widths.into_iter().enumerate() {
        pill(
            canvas,
            frame,
            [x - 6.5, y - 6.2 + row as f32 * 5.0, width, 2.4],
            colour,
        );
    }
}

/// A small caret pointing left (`left`) or right centred on (`x`, `y`), as a
/// cycler's ends are.
pub(crate) fn caret_mark(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    x: f32,
    y: f32,
    left: bool,
    colour: Color,
) {
    // The guillemet reads as an arrow where an arc reads as a bracket.
    text(
        canvas,
        TextFamily::Display,
        format_args!("{}", if left { '\u{2039}' } else { '\u{203a}' }),
        frame.rect(x - 14.0, y - 25.0, 28.0, 44.0),
        42.0 * frame.s,
        colour,
        FontWeight::Semibold,
        TextAlign::Center,
    );
}

/// A value stepped with Left and Right over `rect`: its outline, a caret at
/// each end and the value between them, after a colour `swatch` when given.
/// A click on its left half steps back, on its right half on.
pub(crate) fn cycler(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    rect: [f32; 4],
    value: std::fmt::Arguments<'_>,
    swatch: Option<Color>,
    focused: bool,
) {
    let [x, y, width, height] = rect;
    pill(canvas, frame, rect, color::alpha(color::SPACE, 0.6));
    outline(canvas, frame, rect, edge(focused));
    let middle = y + height * 0.5;
    let tint = if focused { color::TEXT } else { color::MUTED };
    caret_mark(canvas, frame, x + 20.0, middle, true, tint);
    caret_mark(canvas, frame, x + width - 20.0, middle, false, tint);
    let mut left = x + 38.0;
    if let Some(swatch) = swatch {
        pill(canvas, frame, [left, middle - 7.0, 14.0, 14.0], swatch);
        left += 20.0;
    }
    text(
        canvas,
        TextFamily::Body,
        value,
        frame.rect(left, middle - 12.0, x + width - 38.0 - left, 24.0),
        17.0 * frame.s,
        if focused {
            color::TEXT
        } else {
            color::alpha(color::TEXT, 0.9)
        },
        FontWeight::Regular,
        TextAlign::Center,
    );
}

/// Colour chips across `rect`, one circle centred in each equal share of it
/// (so a click's share is its chip), the `active` one ringed gold.
pub(crate) fn chips(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    rect: [f32; 4],
    colours: &[Color],
    active: usize,
    focused: bool,
) {
    let [x, y, width, height] = rect;
    let share = width / colours.len().max(1) as f32;
    let middle = y + height * 0.5;
    let size = (share - 10.0).min(height - 8.0).min(26.0);
    for (index, colour) in colours.iter().enumerate() {
        let centre = x + share * (index as f32 + 0.5);
        if index == active {
            let ring = size + 10.0;
            pill(
                canvas,
                frame,
                [centre - ring * 0.5, middle - ring * 0.5, ring, ring],
                if focused {
                    color::GOLD_BRIGHT
                } else {
                    color::GOLD
                },
            );
            let gap = size + 4.0;
            pill(
                canvas,
                frame,
                [centre - gap * 0.5, middle - gap * 0.5, gap, gap],
                color::SPACE,
            );
        }
        pill(
            canvas,
            frame,
            [centre - size * 0.5, middle - size * 0.5, size, size],
            *colour,
        );
    }
}

/// A button over `rect` answering to `token`: gold with dark text when it is
/// the screen's one main action (`primary`), outlined otherwise; dimmed and
/// inert-looking when not `enabled`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn button(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    rect: [f32; 4],
    label: &str,
    primary: bool,
    enabled: bool,
    focused: bool,
    token: u16,
) {
    let [x, y, width, height] = rect;
    let hovered = canvas.token_hovered(token);
    if primary && enabled {
        pill(
            canvas,
            frame,
            rect,
            if focused || hovered {
                color::GOLD_BRIGHT
            } else {
                color::GOLD
            },
        );
    } else {
        pill(
            canvas,
            frame,
            rect,
            color::alpha(
                color::HOLO,
                if enabled && (focused || hovered) {
                    0.14
                } else {
                    0.05
                },
            ),
        );
        outline(
            canvas,
            frame,
            rect,
            if enabled {
                edge(focused || hovered)
            } else {
                color::alpha(color::HOLO, 0.18)
            },
        );
    }
    let ink = match (primary && enabled, enabled) {
        (true, _) => Color::new(0.078, 0.063, 0.02, 1.0),
        (false, true) => color::TEXT,
        (false, false) => color::QUIET,
    };
    text(
        canvas,
        TextFamily::Display,
        format_args!("{label}"),
        frame.rect(x, y + (height - 26.0) * 0.5, width, 26.0),
        20.0 * frame.s,
        ink,
        FontWeight::Regular,
        TextAlign::Center,
    );
    canvas.hit_region(token, frame.rect(x, y, width, height));
}

/// `level` of `most` rank pips from (`x`, `y`) (their centre line), gold
/// filled up to the level and outlined after; dimmed when not `available`.
pub(crate) fn pips(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    x: f32,
    y: f32,
    level: u8,
    most: u8,
    available: bool,
) {
    let size = 10.0;
    for pip in 0..most {
        let left = x + f32::from(pip) * (size + 7.0);
        let rect = [left, y - size * 0.5, size, size];
        if pip < level {
            pill(
                canvas,
                frame,
                rect,
                if available {
                    color::GOLD_BRIGHT
                } else {
                    color::alpha(color::GOLD, 0.35)
                },
            );
        } else {
            outline(
                canvas,
                frame,
                rect,
                color::alpha(color::HOLO, if available { 0.45 } else { 0.2 }),
            );
        }
    }
}

/// A list of `labels` over `rect` (frame pixels), each row `row` tall: the
/// `highlighted` one on a band, the one in use (`current`) in gold with a dot;
/// choice `i` answers to `token_base + i`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn list(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    rect: [f32; 4],
    row: f32,
    labels: &[&str],
    highlighted: usize,
    current: usize,
    token_base: u16,
) {
    let [x, y, width, height] = rect;
    let s = frame.s;
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect: frame.rect(x + 3.0, y + 6.0, width, height),
            radius: 14.0 * s,
            color: color::alpha(color::SPACE, 0.6),
        },
    );
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect: frame.rect(x, y, width, height),
            radius: 14.0 * s,
            color: Color::new(0.04, 0.06, 0.12, 0.98),
        },
    );
    push(
        canvas,
        DrawCommand::Border {
            rect: frame.rect(x, y, width, height),
            radius: 14.0 * s,
            width: 1.5 * s,
            color: color::alpha(color::HOLO, 0.45),
        },
    );
    for (index, label) in labels.iter().enumerate() {
        let top = y + 8.0 + index as f32 * row;
        let token = token_base + index as u16;
        let hovered = canvas.token_hovered(token);
        if index == highlighted || hovered {
            push(
                canvas,
                DrawCommand::RoundedRect {
                    rect: frame.rect(x + 6.0, top, width - 12.0, row),
                    radius: 8.0 * s,
                    color: color::alpha(color::HOLO, 0.12),
                },
            );
        }
        if index == current {
            pill(
                canvas,
                frame,
                [x + width - 26.0, top + row * 0.5 - 4.0, 8.0, 8.0],
                color::GOLD_BRIGHT,
            );
        }
        let colour = match (index == highlighted || hovered, index == current) {
            (true, _) => color::TEXT,
            (false, true) => color::GOLD_BRIGHT,
            (false, false) => color::MUTED,
        };
        text(
            canvas,
            TextFamily::Body,
            format_args!("{}", crate::menu::classic::view::Sentence(label)),
            frame.rect(x + 20.0, top + (row - 24.0) * 0.5, width - 56.0, 24.0),
            17.0 * s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
        canvas.hit_region(token, frame.rect(x + 6.0, top, width - 12.0, row));
    }
}

/// A gold reset arrow centred on (`x`, `y`) answering to `token`: a turning
/// stroke with a dot for its head, on a gold disc while hovered.
pub(crate) fn reset(canvas: &mut MenuCanvas, frame: &Frame, x: f32, y: f32, token: u16) {
    let s = frame.s;
    let hovered = canvas.token_hovered(token);
    let target = frame.rect(x - 16.0, y - 16.0, 32.0, 32.0);
    if hovered {
        pill(
            canvas,
            frame,
            [x - 16.0, y - 16.0, 32.0, 32.0],
            color::alpha(color::GOLD, 0.22),
        );
    }
    let centre = frame.point(x, y);
    let radius = 8.0 * s;
    let start = -std::f32::consts::FRAC_PI_2;
    let sweep = std::f32::consts::PI * 1.55;
    let colour = if hovered {
        color::TEXT
    } else {
        color::GOLD_BRIGHT
    };
    push(
        canvas,
        DrawCommand::Arc {
            center: centre,
            radius,
            width: 2.2 * s,
            start,
            sweep,
            color: colour,
            knockout: None,
        },
    );
    let end = start + sweep;
    let head = 5.0 * s;
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect: Rect::new(
                centre[0] + radius * end.cos() - head * 0.5,
                centre[1] + radius * end.sin() - head * 0.5,
                head,
                head,
            ),
            radius: head * 0.5,
            color: colour,
        },
    );
    canvas.hit_region(token, target);
}

/// [`changed_dot`]'s radius, in frame pixels.
pub(crate) const CHANGED_DOT_RADIUS: f32 = 3.5;

/// The gold dot after a label whose setting differs from its default, centred
/// on (`x`, `y`).
pub(crate) fn changed_dot(canvas: &mut MenuCanvas, frame: &Frame, x: f32, y: f32) {
    let r = CHANGED_DOT_RADIUS;
    pill(
        canvas,
        frame,
        [x - r, y - r, r * 2.0, r * 2.0],
        color::GOLD_BRIGHT,
    );
}

/// A sub-heading at (`x`, `y`) with a rule after it to `x + width`.
pub(crate) fn heading(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    x: f32,
    y: f32,
    width: f32,
    label: &str,
) {
    heading_in(canvas, frame, x, y, width, label, color::HOLO);
}

/// [`heading`] in `colour` (its rule too, fainter).
pub(crate) fn heading_in(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    x: f32,
    y: f32,
    width: f32,
    label: &str,
    colour: Color,
) {
    // Rajdhani SemiBold at 22 is about 8.5 pixels a character.
    let label_width = 8.5 * label.chars().count() as f32 + 22.0;
    text(
        canvas,
        TextFamily::Display,
        format_args!("{label}"),
        frame.rect(x, y - 15.0, label_width + 40.0, 30.0),
        22.0 * frame.s,
        colour,
        FontWeight::Regular,
        TextAlign::Start,
    );
    let rule = x + label_width;
    if rule < x + width {
        push(
            canvas,
            DrawCommand::SolidRect {
                rect: frame.rect(rule, y, x + width - rule, 1.0),
                color: color::alpha(colour, 0.22),
            },
        );
    }
}

/// The search pill over `rect` answering to `token`: a lens, the typed
/// `query` (with a cursor while `active`) or `prompt`, and at its end how many
/// were `found` while a search is typed, or else the `/` key that opens it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn search(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    rect: [f32; 4],
    query: &str,
    active: bool,
    prompt: &str,
    found: Option<usize>,
    token: u16,
) {
    let [x, y, width, height] = rect;
    let s = frame.s;
    let hovered = canvas.token_hovered(token);
    pill(canvas, frame, rect, color::alpha(color::SPACE, 0.6));
    outline(canvas, frame, rect, edge(active || hovered));
    if active && !query.is_empty() && crate::text_entry::selection_shown() {
        // Ctrl+A: the typed words are selected.
        push(
            canvas,
            DrawCommand::SolidRect {
                rect: frame.rect(x + 42.0, y + 6.0, width - 160.0, height - 12.0),
                color: color::alpha(color::HOLO, 0.32),
            },
        );
    }
    // The lens: a ring and a short handle of dots.
    let lens = frame.point(x + 26.0, y + height * 0.5 - 1.0);
    push(
        canvas,
        DrawCommand::Arc {
            center: lens,
            radius: 7.0 * s,
            width: 2.0 * s,
            start: 0.0,
            sweep: std::f32::consts::TAU,
            color: color::HOLO,
            knockout: None,
        },
    );
    for step in 0..4 {
        let at = 6.0 + step as f32 * 1.6;
        let dot = Rect::new(
            lens[0] + at * s - 1.1 * s,
            lens[1] + at * s - 1.1 * s,
            2.2 * s,
            2.2 * s,
        );
        push(
            canvas,
            DrawCommand::RoundedRect {
                rect: dot,
                radius: dot.width * 0.5,
                color: color::HOLO,
            },
        );
    }
    let text_rect = frame.rect(x + 48.0, y + (height - 24.0) * 0.5, width - 150.0, 24.0);
    if query.is_empty() && !active {
        text(
            canvas,
            TextFamily::Body,
            format_args!("{prompt}"),
            text_rect,
            17.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
    } else {
        text(
            canvas,
            TextFamily::Body,
            format_args!("{query}{}", if active { "_" } else { "" }),
            text_rect,
            17.0 * s,
            color::TEXT,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }
    if let Some(found) = found {
        text(
            canvas,
            TextFamily::Body,
            format_args!("{found} found"),
            frame.rect(x + width - 110.0, y + (height - 22.0) * 0.5, 92.0, 22.0),
            15.0 * s,
            if found == 0 {
                color::QUIET
            } else {
                color::GOLD_BRIGHT
            },
            FontWeight::Regular,
            TextAlign::End,
        );
    } else if !active {
        let [cap_x, cap_y] = frame.point(x + width - 46.0, y + (height - 24.0) * 0.5);
        super::key_hint(canvas, &["/"], "", cap_x, cap_y, s);
    }
    canvas.hit_region(token, frame.rect(x, y, width, height));
}

/// The lit rail down a list of entries: a holo line at `x` from `top` to
/// `bottom`, fading at its ends, with a gold segment beside the entry centred
/// on `lit` (frame pixels), when one is lit.
pub(crate) fn rail(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    x: f32,
    top: f32,
    bottom: f32,
    lit: Option<f32>,
) {
    let height = bottom - top;
    let line = color::alpha(color::HOLO, 0.5);
    let clear = color::alpha(color::HOLO, 0.0);
    super::fade(canvas, frame.rect(x, top, 2.0, height * 0.08), clear, line);
    push(
        canvas,
        DrawCommand::SolidRect {
            rect: frame.rect(x, top + height * 0.08, 2.0, height * 0.84),
            color: line,
        },
    );
    super::fade(
        canvas,
        frame.rect(x, top + height * 0.92, 2.0, height * 0.08),
        line,
        clear,
    );
    if let Some(y) = lit {
        pill(
            canvas,
            frame,
            [x - 3.0, y - 20.0, 8.0, 40.0],
            color::GOLD_BRIGHT,
        );
    }
}
