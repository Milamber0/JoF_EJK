//! The Collection's live swatches (its Shaders tab, and the Profile screen's Saber
//! tab): each blade skin drawn small with the UI's shapes, moving as it does in the
//! game. A blade skin lies across its swatch from a
//! steel hilt, drawn from its loaded blade-skin file (`blade_skin_file.rs`): a hot core
//! in a corona graded from the file's inside colour to its rim, breathing as its light
//! flickers in a soft haze, flame loops rising off it, granules drifting along it and,
//! when the skin has flares, now and then a bright flare running from the hilt to the
//! tip (as `saber.wgsl` animates the real blade), and, when the skin has them, lightning
//! arcs flashing off it, motes drifting round it and its colours turning through their
//! hues, and the later sections' looks ([`effects`]). A locked one is drawn grey and
//! still, under a padlock; an owned one whose pack has not come yet is drawn still in
//! neutral grey.

use crate::blade_skin_file::{Arcs, BladeSkinDef, Motes, Rgb};
use crate::menu::sjk::{Frame, color};

use crate::saber_skins::LoadedSkin;
use sjk_ui::{Color, DrawCommand, DrawList, Gradient};

#[path = "collection_swatch_effects.rs"]
mod effects;

/// The share of each flare cycle its knot runs.
const FLARE_RUN: f32 = 0.62;
/// Layers of the soft haze and of the flare's light; flame loops and granules along
/// the corona.
const HAZE_LAYERS: usize = 9;
const FLARE_LAYERS: usize = 8;
const PROMINENCES: usize = 6;
const GRANULES: usize = 12;
/// Motes drawn, and the straight runs of an arc's zigzag.
const MOTES: usize = 14;
const ARC_RUNS: usize = 7;
/// The swatch's blade stands for a stock one this long (units), for the skins' rates
/// along it.
const BLADE_UNITS: f32 = 40.0;

fn push(canvas: &mut DrawList, command: DrawCommand) {
    let _ = canvas.push(command);
}

/// A filled rounded rectangle, radius half its height (frame pixels).
fn capsule(canvas: &mut DrawList, frame: &Frame, rect: [f32; 4], colour: Color) {
    let [x, y, width, height] = rect;
    let rect = frame.rect(x, y, width, height);
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect,
            radius: rect.height.min(rect.width) * 0.5,
            color: colour,
        },
    );
}

/// A filled disc of `radius` round (`x`, `y`) (frame pixels).
fn disc(canvas: &mut DrawList, frame: &Frame, x: f32, y: f32, radius: f32, colour: Color) {
    capsule(
        canvas,
        frame,
        [x - radius, y - radius, radius * 2.0, radius * 2.0],
        colour,
    );
}

/// `value`'s fractional part.
fn fract(value: f32) -> f32 {
    value - value.floor()
}

/// A repeatable pseudo-random number in [0, 1) for `seed`.
fn draw(seed: f32) -> f32 {
    fract((seed * 12.9898).sin() * 43_758.547)
}

/// A straight stroke from `a` to `b` (frame pixels), `width` thick: an arc of a circle
/// many times its length, so it is straight to the eye, with round caps.
fn stroke(
    canvas: &mut DrawList,
    frame: &Frame,
    a: [f32; 2],
    b: [f32; 2],
    width: f32,
    colour: Color,
) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx.hypot(dy);
    if length < 0.5 {
        return;
    }
    let offset = length * 12.0;
    let centre = [
        (a[0] + b[0]) * 0.5 - dy / length * offset,
        (a[1] + b[1]) * 0.5 + dx / length * offset,
    ];
    let radius = offset.hypot(length * 0.5);
    let angle = |p: [f32; 2]| (p[1] - centre[1]).atan2(p[0] - centre[0]);
    let start = angle(a);
    let mut sweep = angle(b) - start;
    if sweep > std::f32::consts::PI {
        sweep -= std::f32::consts::TAU;
    } else if sweep < -std::f32::consts::PI {
        sweep += std::f32::consts::TAU;
    }
    push(
        canvas,
        DrawCommand::Arc {
            center: frame.point(centre[0], centre[1]),
            radius: radius * frame.s,
            width: width * frame.s,
            start,
            sweep,
            color: colour,
            knockout: None,
        },
    );
}

/// What a swatch shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Shown {
    /// The skin, alive.
    Skin,
    /// Locked: grey and still, under a padlock.
    Locked,
    /// Owned, but its pack has not come from the hub yet: neutral and still.
    Waiting,
}

/// Draw a blade skin across the swatch `rect` (frame pixels) at `seconds`: `skin` its
/// loaded file, if its pack is loaded; `owned` lights it; `turn` turns a chroma's colours
/// to its wearer's ([`chroma_turn`]). Says what it showed.
pub(crate) fn blade(
    canvas: &mut DrawList,
    frame: &Frame,
    rect: [f32; 4],
    skin: Option<&LoadedSkin>,
    owned: bool,
    seconds: f32,
    turn: f32,
) -> Shown {
    let [x, y, width, height] = rect;
    let s = frame.s;
    // The swatch's own dark glass, so the glow reads the same over any map.
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect: frame.rect(x, y, width, height),
            radius: 14.0 * s,
            color: Color::new(0.016, 0.024, 0.05, 0.94),
        },
    );
    let centre = y + height * 0.5;
    let hilt = 84.0;
    let start = x + 26.0 + hilt;
    let length = width - (start - x) - 34.0;
    let shown = match (owned, skin) {
        (true, Some(skin)) => {
            lit_blade(
                canvas,
                frame,
                &skin.def,
                [start, centre, length],
                seconds,
                turn,
            );
            Shown::Skin
        }
        (true, None) => {
            grey_blade(canvas, frame, start, centre, length, 1.6);
            Shown::Waiting
        }
        (false, _) => {
            grey_blade(canvas, frame, start, centre, length, 1.0);
            Shown::Locked
        }
    };
    hilt_at(canvas, frame, start, centre, hilt, owned);
    push(
        canvas,
        DrawCommand::Border {
            rect: frame.rect(x, y, width, height),
            radius: 14.0 * s,
            width: 1.2 * s,
            color: color::alpha(color::HOLO, if owned { 0.22 } else { 0.14 }),
        },
    );
    if !owned {
        padlock(canvas, frame, x + width * 0.5, centre);
    }
    shown
}

/// The height [`blade`] draws a swatch at (frame pixels); a small one is that swatch
/// shrunk.
const CARD_HEIGHT: f32 = 224.0;

/// A frame drawing a swatch `rect[3] / CARD_HEIGHT` times its card size into `rect`
/// (frame pixels of `frame`), and the swatch's own rectangle in it.
fn shrunk(frame: &Frame, rect: [f32; 4]) -> (Frame, [f32; 4]) {
    let [x, y, width, height] = rect;
    let k = height / CARD_HEIGHT;
    let small = Frame {
        s: frame.s * k,
        origin: frame.point(x, y),
    };
    (small, [0.0, 0.0, width / k, CARD_HEIGHT])
}

/// An owned blade skin as [`blade`] draws it on its card, shrunk into `rect` (frame
/// pixels): the Saber tab's blade choice.
pub(crate) fn small_blade(
    canvas: &mut DrawList,
    frame: &Frame,
    rect: [f32; 4],
    skin: Option<&LoadedSkin>,
    seconds: f32,
    turn: f32,
) -> Shown {
    let (small, rect) = shrunk(frame, rect);
    blade(canvas, &small, rect, skin, true, seconds, turn)
}

/// A blade skin as [`blade`] draws it, owned (alive) or not (grey under a padlock),
/// shrunk into `rect` (frame pixels): the Collection's rack.
pub(crate) fn small_swatch(
    canvas: &mut DrawList,
    frame: &Frame,
    rect: [f32; 4],
    skin: Option<&LoadedSkin>,
    owned: bool,
    seconds: f32,
    turn: f32,
) -> Shown {
    let (small, rect) = shrunk(frame, rect);
    blade(canvas, &small, rect, skin, owned, seconds, turn)
}

/// A UI colour (a saber colour as the menus show it) as the bytes a blade's colour is
/// made of, for [`chroma_turn`].
pub(crate) fn rgb_bytes(colour: Color) -> [u8; 3] {
    [colour.r, colour.g, colour.b].map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// Segments of the chroma mark's colour wheel.
const WHEEL_SEGMENTS: usize = 12;

/// The chroma mark: a small colour wheel (a ring running round the hues, a white dot in
/// its middle) of `size` frame pixels across, its top left at (`x`, `y`), at `alpha`.
/// Says the shader takes its wearer's saber colour; readable from about 12 pixels.
pub(crate) fn chroma_mark(
    canvas: &mut DrawList,
    frame: &Frame,
    x: f32,
    y: f32,
    size: f32,
    alpha: f32,
) {
    let radius = size * 0.5;
    let centre = frame.point(x + radius, y + radius);
    let width = size * 0.3 * frame.s;
    let ring = (radius * frame.s - width * 0.5).max(0.5);
    let sweep = std::f32::consts::TAU / WHEEL_SEGMENTS as f32;
    let cap = (width / ring).min(sweep * 0.7);
    // A dark backing so the wheel reads over a bright blade or a light panel.
    disc(
        canvas,
        frame,
        x + radius,
        y + radius,
        radius + 1.2,
        Color::new(0.02, 0.03, 0.06, 0.75 * alpha),
    );
    for segment in 0..WHEEL_SEGMENTS {
        let hue = segment as f32 / WHEEL_SEGMENTS as f32;
        // Red at the top, running clockwise through the colours.
        let rgb = crate::saber_skins::turn_hue([1.0, 0.0, 0.0], hue);
        let top = rgb.into_iter().fold(0.001, f32::max);
        push(
            canvas,
            DrawCommand::Arc {
                center: centre,
                radius: ring,
                width,
                // Short of the segment by its round caps, which close the seams.
                start: -std::f32::consts::FRAC_PI_2 + segment as f32 * sweep + cap * 0.5,
                sweep: sweep - cap,
                color: Color::new(rgb[0] / top, rgb[1] / top, rgb[2] / top, alpha),
                knockout: None,
            },
        );
    }
    disc(
        canvas,
        frame,
        x + radius,
        y + radius,
        radius * 0.28,
        Color::new(1.0, 1.0, 1.0, 0.95 * alpha),
    );
}

/// How far a swatch turns `skin`'s colours for a wearer whose saber is `rgb`: a chroma's
/// turn to that colour, 0 for any other skin, no skin or a grey colour.
pub(crate) fn chroma_turn(skin: Option<&LoadedSkin>, rgb: [u8; 3]) -> f32 {
    skin.map_or(0.0, |skin| {
        skin.chroma_turn(crate::saber_rgb::BladeColor::from_rgb(rgb))
    })
}

/// The stock blade in `colour` from the same hilt, still, in a swatch shrunk into
/// `rect` (frame pixels) as [`small_blade`]'s: the Saber tab's first blade choice.
pub(crate) fn small_stock(canvas: &mut DrawList, frame: &Frame, rect: [f32; 4], colour: Color) {
    let (small, [x, y, width, height]) = shrunk(frame, rect);
    let frame = &small;
    let s = frame.s;
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect: frame.rect(x, y, width, height),
            radius: 14.0 * s,
            color: Color::new(0.016, 0.024, 0.05, 0.94),
        },
    );
    let centre = y + height * 0.5;
    let hilt = 84.0;
    let start = x + 26.0 + hilt;
    let length = width - (start - x) - 34.0;
    let tint = |alpha: f32| Color::new(colour.r, colour.g, colour.b, alpha);
    // The stock blade: its colour's glow round a white core.
    for (half, alpha) in [(30.0, 0.12), (16.0, 0.4), (9.0, 0.85)] {
        capsule(
            canvas,
            frame,
            [
                start - 4.0,
                centre - half,
                length + 4.0 + half * 0.4,
                half * 2.0,
            ],
            tint(alpha),
        );
    }
    capsule(
        canvas,
        frame,
        [start, centre - 4.0, length, 8.0],
        Color::new(1.0, 1.0, 1.0, 1.0),
    );
    hilt_at(canvas, frame, start, centre, hilt, true);
    push(
        canvas,
        DrawCommand::Border {
            rect: frame.rect(x, y, width, height),
            radius: 14.0 * s,
            width: 1.2 * s,
            color: color::alpha(color::HOLO, 0.22),
        },
    );
}

/// `a` to `b` by `t`.
fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    std::array::from_fn(|channel| a[channel] + (b[channel] - a[channel]) * t)
}

/// A colour of the skin's own gradient, `u` from its rim (0) through its rim's heat and
/// its inside to its hot core (1).
fn gradient(def: &BladeSkinDef, u: f32) -> Rgb {
    let stops = [
        def.corona.rim_cool,
        def.corona.rim_hot,
        def.corona.inner,
        def.core.white,
    ];
    let at = u.clamp(0.0, 1.0) * (stops.len() - 1) as f32;
    let index = (at.floor() as usize).min(stops.len() - 2);
    mix(stops[index], stops[index + 1], at - index as f32)
}

/// A UI colour of `rgb` (clamped) at `alpha`.
fn ui(rgb: Rgb, alpha: f32) -> Color {
    Color::new(
        rgb[0].clamp(0.0, 1.0),
        rgb[1].clamp(0.0, 1.0),
        rgb[2].clamp(0.0, 1.0),
        alpha,
    )
}

/// The skin `def` from `start` along `length`, centred on `centre`, at `t` seconds, a
/// chroma's colours turned `turn`.
fn lit_blade(
    canvas: &mut DrawList,
    frame: &Frame,
    def: &BladeSkinDef,
    [start, centre, length]: [f32; 3],
    t: f32,
    turn: f32,
) {
    let s = frame.s;
    const WHITE: Rgb = [1.0; 3];
    let shaping = effects::Shaping {
        turn,
        ..effects::Shaping::of(def, t)
    };
    // A sputter cuts the blade short for a moment.
    let length = length * shaping.shown;
    // A skin whose hue turns: every colour turned as the blade is at `along` (0 at the
    // hilt, 1 at the tip) now; the others as they are. A tint draws them toward it.
    let hued = |rgb: Rgb, along: f32| {
        shaping.tinted(match def.hue {
            Some(hue) => {
                crate::saber_skins::turn_hue(rgb, t * hue.rate + along * BLADE_UNITS * hue.along)
            }
            None => rgb,
        })
    };
    let ui = |rgb: Rgb, alpha: f32| ui(hued(rgb, 0.5), alpha);
    // The corona breathes slowly and unevenly, as the blade's light flickers.
    let breath = 1.0
        + def
            .light
            .flicker
            .waves
            .iter()
            .enumerate()
            .map(|(index, wave)| 0.05 * (t * wave.rate + 1.7 * index as f32).sin() * wave.weight)
            .sum::<f32>()
        + 0.12 * shaping.beat;
    let end = start + length;
    let corona = 13.0 * breath;
    effects::ghosts(canvas, frame, def, [start, centre], length, corona, &ui);
    // The light it casts on the swatch, and its haze: many faint layers, so the glow
    // falls off softly instead of in bands.
    for step in 0..HAZE_LAYERS {
        let k = step as f32 / (HAZE_LAYERS - 1) as f32;
        let half = (64.0 - 46.0 * k) * breath;
        let warm = mix(def.corona.rim_cool, def.corona.inner, 0.2 + 0.5 * k);
        capsule(
            canvas,
            frame,
            [
                start - 6.0 - half * 0.25,
                centre - half,
                length + 6.0 + half * 0.6,
                half * 2.0,
            ],
            ui(warm, 0.026 + 0.03 * k),
        );
    }
    // Flame loops rising off the corona and sinking back, drifting tipward, above and
    // below in turn, when the skin has tongues.
    if def.tongues.range != 0.0 {
        for index in 0..PROMINENCES {
            let i = index as f32;
            let life = fract(t * (0.21 + 0.03 * i) + i * 0.37);
            let along = fract(i * 0.29 + 0.13 + t * 0.035);
            let rise = (life * std::f32::consts::PI).sin();
            let radius = 3.0 + 4.5 * rise;
            let px = start + 14.0 + along * (length - 28.0);
            let above = index % 2 == 0;
            let py = if above {
                centre - corona + 4.0
            } else {
                centre + corona - 4.0
            };
            push(
                canvas,
                DrawCommand::Arc {
                    center: frame.point(px, py),
                    radius: radius * s,
                    width: (1.4 + 1.2 * rise) * s,
                    start: if above { std::f32::consts::PI } else { 0.0 },
                    sweep: std::f32::consts::PI,
                    color: ui(
                        mix(def.corona.rim_hot, def.corona.inner, 0.45 * rise),
                        0.38 * rise,
                    ),
                    knockout: None,
                },
            );
        }
    }
    // The corona, rim into the inside and the glow by the core: stacked, so each band
    // is a step of a few pixels.
    for (scale, u, alpha) in [
        (1.0, 0.2, 0.55),
        (0.8, 0.4, 0.72),
        (0.62, 0.6, 0.86),
        (0.46, 0.8, 0.95),
    ] {
        let half = corona * scale;
        capsule(
            canvas,
            frame,
            [start, centre - half, length + half * 0.4, half * 2.0],
            ui(gradient(def, u), alpha),
        );
    }
    // Granulation: bright cells drifting along the glow.
    for index in 0..GRANULES {
        let i = index as f32;
        let along = fract(i * 0.0833 + 0.11 * (i * 1.7).sin() + t * 0.045);
        let wobble = (t * 1.9 + i * 2.3).sin() * corona * 0.5;
        let shine = 0.5 + 0.5 * (t * 3.3 + i * 1.3).sin();
        disc(
            canvas,
            frame,
            start + 6.0 + along * (length - 12.0),
            centre + wobble,
            1.4 + 1.1 * shine,
            ui(gradient(def, 0.9), 0.18 + 0.4 * shine),
        );
    }
    // The hot core, its fringe at its very edge.
    capsule(
        canvas,
        frame,
        [start, centre - 4.4, length, 8.8],
        ui(gradient(def, 0.9), 1.0),
    );
    capsule(
        canvas,
        frame,
        [start + 1.0, centre - 2.6, length - 2.0, 5.2],
        ui(mix(def.core.white, WHITE, 0.5), 1.0),
    );
    let at = [start, centre];
    effects::veins(canvas, frame, def, at, length, t, &ui);
    effects::scan(canvas, frame, def, at, length, corona, t, &ui);
    effects::glyphs(canvas, frame, def, at, length, t, &ui);
    effects::embers(canvas, frame, def, at, length, t, &ui);
    effects::lab(canvas, frame, def, at, length, corona, t, &ui);
    effects::figures(canvas, frame, def, at, length, corona, t, &ui);
    // A flare: a bright knot running from the hilt to the tip once a cycle of the skin's
    // first track, swelling and fading, its light soft round it.
    let phase = fract(t * def.flares.rate);
    if def.flares.count > 0 && def.flares.rate > 0.0 && phase < FLARE_RUN {
        let run = phase / FLARE_RUN;
        let eased = run * run * (3.0 - 2.0 * run);
        let fx = start + eased * length;
        let strength = (run * std::f32::consts::PI).sin();
        for step in 0..FLARE_LAYERS {
            let k = step as f32 / (FLARE_LAYERS - 1) as f32;
            let half = (5.0 + 25.0 * (1.0 - k) * (1.0 - k)) * (0.4 + 0.6 * strength);
            capsule(
                canvas,
                frame,
                [fx - half * 1.5, centre - half, half * 3.0, half * 2.0],
                ui(mix(gradient(def, 0.5), WHITE, 0.9 * k), 0.07 * strength),
            );
        }
        let streak = 10.0 + 30.0 * strength;
        capsule(
            canvas,
            frame,
            [fx - streak, centre - 4.0, streak * 2.0, 8.0],
            ui(mix(def.core.white, WHITE, 0.3), 0.85 * strength),
        );
        disc(
            canvas,
            frame,
            fx,
            centre,
            2.0 + 5.0 * strength,
            Color::new(1.0, 1.0, 1.0, strength),
        );
    }
    // A glint at the tip.
    disc(
        canvas,
        frame,
        end + corona * 0.2,
        centre,
        3.0 + 1.5 * breath,
        ui(gradient(def, 0.85), 0.55),
    );
    if let Some(motes) = &def.motes {
        swatch_motes(
            canvas,
            frame,
            motes,
            [start, centre],
            length,
            corona,
            t,
            &hued,
        );
    }
    if let Some(arcs) = &def.arcs {
        swatch_arcs(
            canvas,
            frame,
            arcs,
            [start, centre],
            length,
            corona,
            t,
            &hued,
        );
    }
    effects::glitch(canvas, frame, def, [start, centre], length, t);
    effects::solids(canvas, frame, def, [start, centre], length, corona, t, &ui);
}

/// The skin's motes round the blade from `at` (its hilt end, frame pixels) along `length`,
/// `corona` pixels a capsule radius: a few specks drifting as the field does, twinkling,
/// showing between the motes' inner and outer radii and, by their focus, toward the tip.
#[allow(clippy::too_many_arguments)]
fn swatch_motes(
    canvas: &mut DrawList,
    frame: &Frame,
    motes: &Motes,
    at: [f32; 2],
    length: f32,
    corona: f32,
    t: f32,
    hued: &dyn Fn(Rgb, f32) -> Rgb,
) {
    let count = ((motes.density * 2.5).min(1.0) * MOTES as f32).round() as usize;
    let band = motes.outer - motes.inner;
    for index in 0..count {
        let i = index as f32;
        let along = fract(draw(i + 0.5) + t * motes.drift.along / BLADE_UNITS);
        // Toward the tip by the focus.
        let along = 1.0 - (1.0 - along) * (1.0 - 0.75 * motes.focus);
        let out = motes.inner + band * fract(draw(i + 7.3) + t * motes.drift.out / band.max(0.01));
        // Fading in after the inner radius and out before the outer one.
        let k = (out - motes.inner) / band.max(0.01);
        let shown = (k / 0.25).min(1.0) * ((1.0 - k) / 0.4).min(1.0);
        let twinkle = if motes.twinkle > 0.0 {
            0.5 + 0.5
                * (t * motes.twinkle * std::f32::consts::TAU * (0.5 + draw(i + 3.1))
                    + draw(i + 9.9) * 6.0)
                    .sin()
        } else {
            1.0
        };
        let side = if index % 2 == 0 { -1.0 } else { 1.0 };
        let (x, y) = (at[0] + along * length, at[1] + side * out * corona);
        let half = 1.6 + 1.2 * motes.size;
        capsule(
            canvas,
            frame,
            [
                x - half * motes.stretch,
                y - half,
                2.0 * half * motes.stretch,
                2.0 * half,
            ],
            ui(
                hued(motes.color, along),
                (0.25 + 0.75 * twinkle)
                    * shown.max(0.0)
                    * (0.45 + 0.25 * motes.brightness).min(1.0),
            ),
        );
    }
}

/// The skin's lightning arcs from the blade at `at` (its hilt end, frame pixels) along
/// `length`: each strike a zigzag leaving the blade and coming back a span further on
/// (or leaping off the tip), bulging out to one side by the arcs' reach, flashing and
/// fading by their decay, re-shaped by their jitter.
#[allow(clippy::too_many_arguments)]
fn swatch_arcs(
    canvas: &mut DrawList,
    frame: &Frame,
    arcs: &Arcs,
    at: [f32; 2],
    length: f32,
    corona: f32,
    t: f32,
    hued: &dyn Fn(Rgb, f32) -> Rgb,
) {
    for arc in 0..arcs.count.min(crate::blade_skin_file::MAX_ARCS) {
        let a = arc as f32;
        let phase = t * arcs.rate + a * 0.618;
        let (cycle, age) = (phase.floor(), fract(phase));
        if draw(cycle + a * 31.0) < arcs.threshold {
            continue;
        }
        let light = (1.0 - age).powf(arcs.decay);
        let span = arcs.span.low + (arcs.span.high - arcs.span.low) * draw(cycle * 1.3 + a);
        let from_tip = draw(cycle * 0.7 + a * 5.0) < arcs.tip;
        let (first, last) = if from_tip {
            (1.0 - 0.15 * span, 1.0 + 0.35 * span)
        } else {
            let first = draw(cycle * 2.1 + a * 3.0) * (1.0 - span);
            (first, first + span)
        };
        let side = if draw(cycle * 3.7 + a) < 0.5 {
            -1.0
        } else {
            1.0
        };
        let shape = (t * arcs.jitter).floor() + a * 17.0;
        let reach = (arcs.reach * corona).min(70.0);
        let jag = arcs.jag * corona;
        let point = |run: usize| {
            let k = run as f32 / ARC_RUNS as f32;
            let bulge = if from_tip {
                (k * std::f32::consts::FRAC_PI_2).sin()
            } else {
                (k * std::f32::consts::PI).sin()
            };
            let zig = if run == 0 || (run == ARC_RUNS && !from_tip) {
                0.0
            } else {
                draw(shape + run as f32 * 1.7) * 2.0 - 1.0
            };
            let along = first + (last - first) * k;
            [
                at[0] + along.min(1.08) * length,
                at[1] + side * (bulge * reach + zig * jag * bulge.sqrt()),
            ]
        };
        for run in 0..ARC_RUNS {
            let fade = if from_tip {
                1.0 - run as f32 / ARC_RUNS as f32
            } else {
                1.0
            };
            let along = first + (last - first) * run as f32 / ARC_RUNS as f32;
            let colour = hued(arcs.color, along);
            let (a, b) = (point(run), point(run + 1));
            stroke(canvas, frame, a, b, 5.0, ui(colour, 0.22 * light * fade));
            stroke(
                canvas,
                frame,
                a,
                b,
                1.8,
                ui(mix(colour, [1.0; 3], 0.4), 0.95 * light * fade),
            );
        }
    }
}

/// A grey, still blade: locked (`light` 1), or owned with its look still to come
/// (brighter).
fn grey_blade(
    canvas: &mut DrawList,
    frame: &Frame,
    start: f32,
    centre: f32,
    length: f32,
    light: f32,
) {
    capsule(
        canvas,
        frame,
        [start - 4.0, centre - 15.0, length + 12.0, 30.0],
        color::alpha(color::QUIET, 0.08 * light),
    );
    capsule(
        canvas,
        frame,
        [start, centre - 6.0, length, 12.0],
        color::alpha(color::QUIET, 0.3 * light),
    );
    capsule(
        canvas,
        frame,
        [start + 1.0, centre - 2.5, length - 2.0, 5.0],
        color::alpha(color::MUTED, (0.45 * light).min(1.0)),
    );
}

/// The steel hilt ending at `start`, `length` long.
fn hilt_at(canvas: &mut DrawList, frame: &Frame, start: f32, centre: f32, length: f32, lit: bool) {
    let s = frame.s;
    let shade = if lit { 1.0 } else { 0.55 };
    let steel = |level: f32| {
        Color::new(
            level * shade,
            (level + 0.02) * shade,
            (level + 0.06) * shade,
            1.0,
        )
    };
    let x = start - length;
    // The pommel, the grip and the emitter's shroud.
    let rect = frame.rect(x, centre - 8.0, length - 12.0, 16.0);
    push(
        canvas,
        DrawCommand::GradientRect {
            rect,
            radius: 4.0 * s,
            gradient: Gradient {
                start: steel(0.78),
                end: steel(0.3),
                vertical: true,
            },
        },
    );
    for ridge in 0..5 {
        let rx = x + 22.0 + ridge as f32 * 8.0;
        push(
            canvas,
            DrawCommand::SolidRect {
                rect: frame.rect(rx, centre - 8.0, 3.0, 16.0),
                color: Color::new(0.06, 0.07, 0.1, 0.85),
            },
        );
    }
    let rect = frame.rect(start - 14.0, centre - 11.0, 14.0, 22.0);
    push(
        canvas,
        DrawCommand::GradientRect {
            rect,
            radius: 3.0 * s,
            gradient: Gradient {
                start: steel(0.86),
                end: steel(0.36),
                vertical: true,
            },
        },
    );
    // The activator, gold.
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect: frame.rect(x + 66.0, centre - 10.5, 7.0, 4.0),
            radius: 1.5 * s,
            color: if lit {
                color::GOLD
            } else {
                color::alpha(color::QUIET, 0.8)
            },
        },
    );
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect: frame.rect(x - 4.0, centre - 9.0, 6.0, 18.0),
            radius: 2.0 * s,
            color: steel(0.5),
        },
    );
}

/// A padlock centred on (`x`, `y`).
fn padlock(canvas: &mut DrawList, frame: &Frame, x: f32, y: f32) {
    let s = frame.s;
    disc(canvas, frame, x, y, 30.0, color::alpha(color::SPACE, 0.82));
    push(
        canvas,
        DrawCommand::Arc {
            center: frame.point(x, y - 5.0),
            radius: 7.0 * s,
            width: 3.0 * s,
            start: std::f32::consts::PI,
            sweep: std::f32::consts::PI,
            color: color::MUTED,
            knockout: None,
        },
    );
    for side in [-7.0, 7.0] {
        push(
            canvas,
            DrawCommand::SolidRect {
                rect: frame.rect(x + side - 1.5, y - 5.0, 3.0, 5.0),
                color: color::MUTED,
            },
        );
    }
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect: frame.rect(x - 11.0, y - 1.0, 22.0, 17.0),
            radius: 3.0 * s,
            color: color::MUTED,
        },
    );
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect: frame.rect(x - 2.0, y + 4.0, 4.0, 7.0),
            radius: 2.0 * s,
            color: color::SPACE,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawn(skin: Option<&LoadedSkin>, owned: bool, seconds: f32) -> (Shown, Vec<DrawCommand>) {
        let mut canvas = DrawList::new(400);
        let frame = Frame::new([1920.0, 1080.0]);
        let shown = blade(
            &mut canvas,
            &frame,
            [100.0, 100.0, 380.0, 200.0],
            skin,
            owned,
            seconds,
            0.0,
        );
        (shown, canvas.commands().to_vec())
    }

    #[test]
    fn the_chroma_mark_stays_in_its_square_and_runs_round_the_hues() {
        let mut canvas = DrawList::new(64);
        let frame = Frame::new([1920.0, 1080.0]);
        chroma_mark(&mut canvas, &frame, 40.0, 60.0, 14.0, 1.0);
        let [x, y] = frame.point(40.0, 60.0);
        let side = 14.0 * frame.s;
        let mut colours = Vec::new();
        for command in canvas.commands() {
            match *command {
                DrawCommand::Arc {
                    center,
                    radius,
                    width,
                    color,
                    ..
                } => {
                    let reach = radius + width * 0.5;
                    assert!(center[0] - reach >= x - 0.01 && center[0] + reach <= x + side + 0.01);
                    assert!(center[1] - reach >= y - 0.01 && center[1] + reach <= y + side + 0.01);
                    colours.push(color);
                }
                DrawCommand::RoundedRect { rect, .. } => {
                    // The backing reaches a little past it, by its 1.2 pixel rim.
                    let rim = 1.3 * frame.s;
                    assert!(rect.x >= x - rim && rect.x + rect.width <= x + side + rim);
                }
                _ => {}
            }
        }
        assert_eq!(colours.len(), WHEEL_SEGMENTS);
        // Every segment its own colour: red, green and blue each lead somewhere.
        for channel in 0..3 {
            assert!(colours.iter().any(|c| {
                let c = [c.r, c.g, c.b];
                (0..3).all(|other| other == channel || c[channel] > c[other] + 0.3)
            }));
        }
    }

    #[test]
    fn a_turned_swatch_draws_the_same_shapes_in_other_colours() {
        let def =
            crate::blade_skin_file::parse("saber_storm", crate::blade_skin_file::tests::SAMPLE)
                .unwrap();
        let skin = LoadedSkin::new("saber_storm", def, &sjk_vfs::VirtualFileSystem::new()).unwrap();
        let draw = |turn: f32| {
            let mut canvas = DrawList::new(400);
            let frame = Frame::new([1920.0, 1080.0]);
            let _ = blade(
                &mut canvas,
                &frame,
                [100.0, 100.0, 380.0, 200.0],
                Some(&skin),
                true,
                1.0,
                turn,
            );
            canvas.commands().to_vec()
        };
        let (plain, turned) = (draw(0.0), draw(0.4));
        assert_eq!(plain.len(), turned.len());
        assert_ne!(core_colour(&plain), core_colour(&turned));
        // The red skin turned to blue reads as a blue blade.
        let blue = crate::saber_rgb::BladeColor::Retail(crate::saber::Color::Blue);
        let turn = skin.chroma_turn(blue);
        assert!(turn != 0.0);
        assert_eq!(
            chroma_turn(Some(&skin), crate::saber::Color::Blue.blade_rgb()),
            turn
        );
        assert_eq!(chroma_turn(None, [0, 0, 255]), 0.0);
        assert_eq!(chroma_turn(Some(&skin), [255, 255, 255]), 0.0);
    }

    /// The colour of the first opaque shape, the core: it moves only with a turning hue.
    fn core_colour(commands: &[DrawCommand]) -> Color {
        commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::RoundedRect { color, .. } if color.a == 1.0 => Some(*color),
                _ => None,
            })
            .expect("a core")
    }

    #[test]
    fn arcs_motes_and_a_turning_hue_show_inside_the_swatch() {
        let def = crate::blade_skin_file::parse(
            "saber_sun",
            &crate::blade_skin_file::tests::sample_with_effects(),
        )
        .unwrap();
        let skin = LoadedSkin::new("saber_sun", def, &sjk_vfs::VirtualFileSystem::new()).unwrap();
        let plain = crate::saber_skins::tests::loaded_sample(1);
        let plain = plain.get("saber_sun");
        // Lightning strokes (long thin arcs of big circles) come and go.
        let strokes = |skin: Option<&LoadedSkin>, seconds: f32| {
            drawn(skin, true, seconds)
                .1
                .iter()
                .filter(
                    |command| matches!(command, DrawCommand::Arc { radius, .. } if *radius > 40.0),
                )
                .count()
        };
        let struck: Vec<usize> = (0..40)
            .map(|step| strokes(Some(&skin), step as f32 * 0.05))
            .collect();
        assert!(struck.iter().any(|count| *count > 0), "{struck:?}");
        assert!((0..40).all(|step| strokes(plain, step as f32 * 0.05) == 0));
        // Motes: more shapes than the plain sample at the same time.
        assert!(drawn(Some(&skin), true, 0.3).1.len() > drawn(plain, true, 0.3).1.len());
        // The hue turns the core's colour with time; without one it holds.
        assert_ne!(
            core_colour(&drawn(Some(&skin), true, 0.0).1),
            core_colour(&drawn(Some(&skin), true, 3.0).1)
        );
        assert_eq!(
            core_colour(&drawn(plain, true, 0.0).1),
            core_colour(&drawn(plain, true, 3.0).1)
        );
        // Everything stays inside the swatch, the strokes' ends too.
        let frame = Frame::new([1920.0, 1080.0]);
        let swatch = frame.rect(100.0, 100.0, 380.0, 200.0);
        for step in 0..60 {
            for command in drawn(Some(&skin), true, step as f32 * 0.17).1 {
                let rect = match command {
                    DrawCommand::RoundedRect { rect, .. } => rect,
                    DrawCommand::Arc {
                        center,
                        radius,
                        start,
                        sweep,
                        ..
                    } if radius > 40.0 => {
                        for angle in [start, start + sweep] {
                            let (x, y) = (
                                center[0] + radius * angle.cos(),
                                center[1] + radius * angle.sin(),
                            );
                            assert!(
                                x >= swatch.x
                                    && x <= swatch.right()
                                    && y >= swatch.y
                                    && y <= swatch.y + swatch.height,
                                "a stroke ends at ({x}, {y})"
                            );
                        }
                        continue;
                    }
                    _ => continue,
                };
                assert!(
                    rect.x >= swatch.x - 0.5
                        && rect.y >= swatch.y - 0.5
                        && rect.right() <= swatch.right() + 0.5
                        && rect.y + rect.height <= swatch.y + swatch.height + 0.5,
                    "{rect:?} at step {step}"
                );
            }
        }
    }

    #[test]
    fn a_loaded_skin_moves_and_a_locked_or_waiting_one_is_still() {
        let skins = crate::saber_skins::tests::loaded_sample(1);
        let skin = skins.get("saber_sun");
        assert!(skin.is_some());
        let (shown, a) = drawn(skin, true, 0.3);
        assert_eq!(shown, Shown::Skin);
        assert_ne!(a, drawn(skin, true, 1.1).1, "the owned swatch is alive");
        assert_eq!(
            drawn(skin, false, 0.3),
            drawn(skin, false, 1.1),
            "locked is still"
        );
        assert_eq!(drawn(skin, false, 0.3).0, Shown::Locked);
        let (waiting, still) = drawn(None, true, 0.3);
        assert_eq!(waiting, Shown::Waiting);
        assert_eq!(
            still,
            drawn(None, true, 1.1).1,
            "waiting for its pack is still"
        );
        // The flare runs part of each cycle: more shapes then.
        let period = 1.0 / skin.unwrap().def.flares.rate;
        let resting = drawn(skin, true, period * 0.9).1.len();
        assert!(drawn(skin, true, period * 0.3).1.len() > resting);
        // Its colours are the skin's: the sample is teal, never red.
        let teal = drawn(skin, true, 0.3).1.iter().any(|command| {
            matches!(command, DrawCommand::RoundedRect { color, .. }
                if color.g > color.r && color.b > color.r && color.a > 0.9)
        });
        assert!(teal);
        // Every shape stays inside the swatch.
        let frame = Frame::new([1920.0, 1080.0]);
        let swatch = frame.rect(100.0, 100.0, 380.0, 200.0);
        for seconds in [0.0, 0.7, 1.4, 2.2, 9.9] {
            for command in drawn(skin, true, seconds).1 {
                let rect = match command {
                    DrawCommand::RoundedRect { rect, .. }
                    | DrawCommand::GradientRect { rect, .. }
                    | DrawCommand::SolidRect { rect, .. }
                    | DrawCommand::Border { rect, .. } => rect,
                    _ => continue,
                };
                assert!(
                    rect.x >= swatch.x - 0.5
                        && rect.y >= swatch.y - 0.5
                        && rect.right() <= swatch.right() + 0.5
                        && rect.y + rect.height <= swatch.y + swatch.height + 0.5,
                    "{rect:?} at {seconds}"
                );
            }
        }
    }
}
