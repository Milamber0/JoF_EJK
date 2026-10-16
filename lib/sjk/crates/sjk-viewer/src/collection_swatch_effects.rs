//! The swatch's share of the later blade-skin sections (`blade_skin_effects.rs`), drawn
//! small with the UI's shapes as `saber.wgsl` animates them on the real blade: afterimages
//! trailing it, embers falling off it, veins along its core, a hologram's scan lines and
//! wireframe, glitched blocks with their colours split, and glyphs (the swatch spells
//! "SJK", having no wearer). The heartbeat, the sputtering cut and the team or
//! surroundings tint change the blade itself ([`Shaping`]).

use super::{BLADE_UNITS, capsule, disc, draw, fract, mix, stroke};
use crate::blade_skin_file::{BladeSkinDef, Rgb};
use crate::menu::sjk::Frame;
use sjk_ui::Color;
use sjk_ui::DrawList;

/// How the later sections change the swatch's blade at a moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Shaping {
    /// The heartbeat's brightening (0 at rest).
    pub(super) beat: f32,
    /// The share of the blade showing (1 unless it sputters).
    pub(super) shown: f32,
    /// A colour every colour is drawn toward, and how much.
    pub(super) tint: Option<(Rgb, f32)>,
    /// A chroma's turn to its wearer's colour (`saber_skins::Chroma`); 0 for none.
    pub(super) turn: f32,
}

impl Shaping {
    /// `def`'s shaping at `t` seconds.
    pub(super) fn of(def: &BladeSkinDef, t: f32) -> Self {
        let beat = def.pulse.map_or(0.0, |pulse| {
            crate::saber_skins::Beat {
                rate: pulse.rate,
                amount: pulse.amount,
                second: pulse.second,
                gap: pulse.gap,
                width: pulse.width,
            }
            .at(t)
        });
        let shown = def.sputter.map_or(1.0, |sputter| {
            let cycle = (t * sputter.rate).floor();
            if sputter.rate <= 0.0 || draw(cycle + 0.37) < sputter.threshold {
                1.0
            } else {
                1.0 - sputter.cut * draw(cycle * 1.7 + 4.1)
            }
        });
        // Outside a team (the swatch has no wearer) the team's `none`; the light of the
        // places a blade goes, slowly passing from warm to cool and back.
        let tint = def.team.map(|team| (team.none, team.amount)).or_else(|| {
            def.ambient.map(|ambient| {
                let light = crate::saber_skins::turn_hue([1.0, 0.55, 0.25], t * 0.05);
                let top = light.into_iter().fold(0.001, f32::max);
                (light.map(|c| (c / top).max(ambient.floor)), ambient.amount)
            })
        });
        Self {
            beat,
            shown,
            tint,
            turn: 0.0,
        }
    }

    /// `rgb` drawn toward the tint at its own strongest channel's level, as
    /// `saber.wgsl`'s `skin_apply_tint`.
    /// A chroma's colours are turned to its wearer's first.
    pub(super) fn tinted(self, rgb: Rgb) -> Rgb {
        let rgb = if self.turn == 0.0 {
            rgb
        } else {
            crate::saber_skins::turn_hue(rgb, self.turn)
        };
        match self.tint {
            Some((tint, amount)) => {
                let level = rgb.into_iter().fold(0.0, f32::max);
                mix(rgb, tint.map(|c| c * level), amount)
            }
            None => rgb,
        }
    }
}

/// The blade's afterimages, behind it: the corona drawn again above it, fainter each
/// time, as if it had just swung down.
pub(super) fn ghosts(
    canvas: &mut DrawList,
    frame: &Frame,
    def: &BladeSkinDef,
    at: [f32; 2],
    length: f32,
    corona: f32,
    ui: &dyn Fn(Rgb, f32) -> Color,
) {
    let Some(ghosts) = def.ghosts else {
        return;
    };
    let mut light = ghosts.fade;
    for index in 0..ghosts.count.min(3) {
        let lift = 13.0 * (index + 1) as f32;
        let half = corona * 0.8;
        capsule(
            canvas,
            frame,
            [at[0], at[1] - lift - half, length, half * 2.0],
            ui(def.corona.inner, 0.35 * light),
        );
        light *= ghosts.fade;
    }
}

/// Embers dripping off the blade and falling, accelerating, fading as they go.
pub(super) fn embers(
    canvas: &mut DrawList,
    frame: &Frame,
    def: &BladeSkinDef,
    at: [f32; 2],
    length: f32,
    t: f32,
    ui: &dyn Fn(Rgb, f32) -> Color,
) {
    let Some(embers) = def.embers else {
        return;
    };
    let count = (embers.density * 16.0).round() as usize;
    for index in 0..count {
        let i = index as f32;
        let phase = fract(t / embers.life + draw(i + 2.2));
        let x = at[0]
            + (0.06 + 0.88 * draw(i + 5.5)) * length
            + embers.spread * 10.0 * (draw(i + 8.1) - 0.5) * phase;
        let y = at[1] + 4.0 + 64.0 * phase * phase;
        let fade = (1.0 - phase).powf(1.5);
        disc(
            canvas,
            frame,
            x,
            y,
            1.2 + 1.8 * (1.0 - 0.5 * phase),
            ui(
                embers.color,
                (0.3 + 0.2 * embers.brightness).min(1.0) * fade,
            ),
        );
    }
}

/// Glowing veins along the core: short bright strokes wandering about it, running along.
pub(super) fn veins(
    canvas: &mut DrawList,
    frame: &Frame,
    def: &BladeSkinDef,
    at: [f32; 2],
    length: f32,
    t: f32,
    ui: &dyn Fn(Rgb, f32) -> Color,
) {
    let Some(veins) = def.veins else {
        return;
    };
    const RUNS: usize = 18;
    let shift = t * veins.speed / BLADE_UNITS;
    let point = |run: usize| {
        let k = run as f32 / RUNS as f32;
        let wander = draw((k * RUNS as f32 + (shift * RUNS as f32).floor()).floor() + 3.3) - 0.5;
        [at[0] + k * length, at[1] + wander * 7.0]
    };
    for run in 0..RUNS {
        stroke(
            canvas,
            frame,
            point(run),
            point(run + 1),
            1.6,
            ui(veins.color, (0.35 + 0.15 * veins.brightness).min(1.0)),
        );
    }
}

/// A hologram's scan lines running along the blade, its wireframe sides and rings.
#[allow(clippy::too_many_arguments)]
pub(super) fn scan(
    canvas: &mut DrawList,
    frame: &Frame,
    def: &BladeSkinDef,
    at: [f32; 2],
    length: f32,
    corona: f32,
    t: f32,
    ui: &dyn Fn(Rgb, f32) -> Color,
) {
    let Some(scan) = def.scan else {
        return;
    };
    let side = corona * scan.edge;
    let line = ui(scan.color, (0.3 + 0.12 * scan.brightness).min(0.9));
    for sign in [-1.0, 1.0] {
        stroke(
            canvas,
            frame,
            [at[0], at[1] + sign * side],
            [at[0] + length, at[1] + sign * side],
            1.2,
            line,
        );
    }
    // Rings across, sliding as the scan does.
    let rings = (scan.rings.max(0.15) * BLADE_UNITS * 0.5).round().max(2.0) as usize;
    let slide = fract(t * scan.speed / BLADE_UNITS * 0.5);
    for index in 0..rings {
        let k = (index as f32 + slide) / rings as f32;
        let x = at[0] + k * length;
        stroke(
            canvas,
            frame,
            [x, at[1] - side],
            [x, at[1] + side],
            1.0,
            line,
        );
    }
}

/// The glyphs of "SJK" running along the core toward the tip.
pub(super) fn glyphs(
    canvas: &mut DrawList,
    frame: &Frame,
    def: &BladeSkinDef,
    at: [f32; 2],
    length: f32,
    t: f32,
    ui: &dyn Fn(Rgb, f32) -> Color,
) {
    let Some(glyphs) = def.glyphs else {
        return;
    };
    const LETTERS: [u32; 3] = [19, 10, 11];
    let size = 16.0;
    let cell = size * 1.35;
    let scroll = (t * glyphs.speed / BLADE_UNITS * length).rem_euclid(cell * 4.0);
    let colour = ui(glyphs.color, (0.5 + 0.15 * glyphs.brightness).min(1.0));
    let mut slot = 0usize;
    let mut x = at[0] + 6.0 - cell * 4.0 + scroll;
    while x + size < at[0] + length - 4.0 {
        let letter = slot % 4;
        if x > at[0] + 4.0 && letter < 3 {
            for (a, b) in crate::saber_persona::glyph_strokes(LETTERS[letter]) {
                // Along the glyph is along the blade, across it is up the swatch.
                let point = |p: [f32; 2]| [x + p[1] * size, at[1] + (p[0] - 0.5) * size * 0.6];
                stroke(canvas, frame, point(a), point(b), 1.3, colour);
            }
        }
        x += cell;
        slot += 1;
    }
}

/// Glitched blocks over the blade: on a draw, a block shown again aside, red one way and
/// blue the other.
pub(super) fn glitch(
    canvas: &mut DrawList,
    frame: &Frame,
    def: &BladeSkinDef,
    at: [f32; 2],
    length: f32,
    t: f32,
) {
    let Some(glitch) = def.glitch else {
        return;
    };
    let frame_draw = (t * glitch.rate).floor();
    let blocks = (BLADE_UNITS / glitch.blocks).round().clamp(2.0, 12.0) as usize;
    let width = length / blocks as f32;
    for block in 0..blocks {
        if draw(frame_draw * 1.3 + block as f32 * 7.7) < glitch.threshold {
            continue;
        }
        let jump = (draw(frame_draw + block as f32) - 0.5) * 18.0 * glitch.shift;
        let x = at[0] + block as f32 * width;
        for (offset, colour) in [
            (-2.0 - 6.0 * glitch.split, Color::new(1.0, 0.2, 0.3, 0.55)),
            (2.0 + 6.0 * glitch.split, Color::new(0.2, 0.5, 1.0, 0.55)),
        ] {
            capsule(
                canvas,
                frame,
                [x, at[1] + jump + offset - 3.0, width, 6.0],
                colour,
            );
        }
    }
}

/// The fifth set (`blade_skin_lab.rs`) over the swatch's blade from `at` (its hilt end)
/// along `length`, `corona` pixels a capsule radius: a front's colour climbing from the
/// hilt, blobs rising in the column, a film's colours sliding, bands winding round it,
/// and a segmented tube's gaps and stuttering sections.
#[allow(clippy::too_many_arguments)]
pub(super) fn lab(
    canvas: &mut DrawList,
    frame: &Frame,
    def: &BladeSkinDef,
    at: [f32; 2],
    length: f32,
    corona: f32,
    t: f32,
    ui: &dyn Fn(Rgb, f32) -> Color,
) {
    let units = length / BLADE_UNITS;
    if let Some(front) = def.front {
        let height = front.height(t, 0.0) * length;
        if height > 1.0 {
            capsule(
                canvas,
                frame,
                [at[0], at[1] - corona * 0.7, height, corona * 1.4],
                ui(front.color, 0.55 * front.amount),
            );
        }
    }
    if let Some(blobs) = def.blobs {
        for index in 0..blobs.count {
            let i = index as f32;
            let pace = blobs.speed * (0.6 + 0.8 * draw(i + 1.3));
            let place = fract(draw(i + 4.7) + t * pace / BLADE_UNITS);
            let sway = (t * 0.6 + i * 2.0).sin() * blobs.wobble * corona * 0.3;
            disc(
                canvas,
                frame,
                at[0] + place * length,
                at[1] + sway,
                corona * blobs.size * (0.45 + 0.3 * draw(i + 7.9)),
                ui(blobs.color, (0.4 + 0.12 * blobs.brightness).min(1.0)),
            );
        }
        if blobs.pools > 0.0 {
            for x in [at[0] + 2.0, at[0] + length - 2.0] {
                disc(
                    canvas,
                    frame,
                    x,
                    at[1],
                    corona * 0.4 * blobs.pools,
                    ui(blobs.color, 0.7),
                );
            }
        }
    }
    if let Some(film) = def.film {
        const PATCHES: usize = 7;
        for index in 0..PATCHES {
            let k = (index as f32 + 0.5) / PATCHES as f32;
            let hue = k * 1.8 + t * (0.03 + 0.05 * film.speed);
            let rgb = crate::saber_skins::turn_hue([1.0, 0.35, 0.6], hue);
            let rgb = mix([1.0; 3], rgb, film.saturation);
            for sign in [-1.0, 1.0] {
                capsule(
                    canvas,
                    frame,
                    [
                        at[0] + (k - 0.5 / PATCHES as f32) * length,
                        at[1] + sign * corona * 0.62 - 2.0,
                        length / PATCHES as f32,
                        4.0,
                    ],
                    ui(rgb, 0.55 * film.amount),
                );
            }
        }
    }
    if let Some(bands) = def.bands {
        let spacing = (bands.spacing * units).max(3.0);
        let shift = (t * bands.speed * units).rem_euclid(spacing);
        let slant = bands.twist * spacing * 0.6;
        let colour = ui(bands.color, (0.45 + 0.4 * bands.mix).min(1.0));
        let mut x = at[0] - spacing + shift;
        while x < at[0] + length {
            if x > at[0] + 1.0 {
                stroke(
                    canvas,
                    frame,
                    [x - slant * 0.5, at[1] + corona * 0.6],
                    [x + slant * 0.5, at[1] - corona * 0.6],
                    (spacing * bands.width).clamp(1.0, 6.0),
                    colour,
                );
            }
            x += spacing;
        }
    }
    if let Some(segments) = def.segments {
        let width = length / segments.count as f32;
        let frame_draw = (t * segments.rate).floor();
        for section in 0..segments.count {
            let x = at[0] + section as f32 * width;
            if section > 0 && segments.gap > 0.0 {
                disc(
                    canvas,
                    frame,
                    x,
                    at[1],
                    2.2,
                    Color::new(0.02, 0.02, 0.04, 0.85),
                );
            }
            if segments.rate > 0.0
                && draw(frame_draw * 1.7 + section as f32 * 5.3) >= segments.threshold
            {
                capsule(
                    canvas,
                    frame,
                    [x, at[1] - corona, width, corona * 2.0],
                    Color::new(0.02, 0.02, 0.04, 0.8 * segments.depth),
                );
            }
        }
    }
}

/// The sixth set (`blade_skin_figures.rs`) over the swatch's blade from `at` (its hilt
/// end) along `length`, `corona` pixels a capsule radius: ghost copies, a sketch's
/// outlines and hatching, an hourglass's sand, a serpent and its head, strands with their
/// bright ends, butterflies, and wings of fire by the hilt.
#[allow(clippy::too_many_arguments)]
pub(super) fn figures(
    canvas: &mut DrawList,
    frame: &Frame,
    def: &BladeSkinDef,
    at: [f32; 2],
    length: f32,
    corona: f32,
    t: f32,
    ui: &dyn Fn(Rgb, f32) -> Color,
) {
    let tau = std::f32::consts::TAU;
    if let Some(copies) = def.copies {
        let apart = (fract(t / copies.period) * 1.2).min(1.0);
        for sign in [-1.0, 1.0] {
            let lift = sign * corona * 0.9 * apart;
            stroke(
                canvas,
                frame,
                at,
                [at[0] + length, at[1] + lift],
                corona * 0.7,
                ui(def.corona.inner, 0.18 * copies.fade),
            );
        }
    }
    if let Some(sketch) = def.sketch {
        let ink = ui(sketch.color, (0.5 + 0.1 * sketch.brightness).min(1.0));
        let redraw = (t * sketch.rate).floor();
        for side in [-1.0, 1.0] {
            let mut last = [at[0], at[1] + side * corona * 0.45];
            for step in 1..=10 {
                let k = step as f32 / 10.0;
                let wobble = (draw(redraw * 3.1 + k * 17.0 + side) - 0.5) * 6.0 * sketch.wobble;
                let next = [
                    at[0] + k * length,
                    at[1] + side * corona * 0.45 * (1.0 - k * k * 0.6) + wobble,
                ];
                stroke(canvas, frame, last, next, 1.3, ink);
                last = next;
            }
        }
        let lines = (sketch.hatch * BLADE_UNITS * 0.4).round().clamp(4.0, 30.0) as usize;
        for line in 0..lines {
            let x = at[0] + (line as f32 + 0.5) / lines as f32 * length;
            stroke(
                canvas,
                frame,
                [x - 3.0, at[1] + corona * 0.35],
                [x + 3.0, at[1] - corona * 0.35],
                0.9,
                ui(sketch.color, 0.4),
            );
        }
    }
    if let Some(hourglass) = def.hourglass {
        let p = fract(t / hourglass.period);
        let sand = ui(hourglass.color, (0.5 + 0.1 * hourglass.brightness).min(1.0));
        let middle = at[0] + length * 0.5;
        let top = (1.0 - p) * length * 0.36;
        let bottom = p * length * 0.36;
        capsule(
            canvas,
            frame,
            [middle, at[1] - 3.0, top.max(1.0), 6.0],
            sand,
        );
        capsule(
            canvas,
            frame,
            [at[0], at[1] - 3.0, bottom.max(1.0), 6.0],
            sand,
        );
        stroke(
            canvas,
            frame,
            [at[0] + bottom, at[1]],
            [middle, at[1]],
            1.0,
            sand,
        );
        for side in [-1.0, 1.0] {
            stroke(
                canvas,
                frame,
                [at[0], at[1] + side * corona * 0.6],
                [middle, at[1] + side * corona * 0.6 * hourglass.waist],
                1.0,
                ui(def.corona.rim_hot, 0.6),
            );
            stroke(
                canvas,
                frame,
                [middle, at[1] + side * corona * 0.6 * hourglass.waist],
                [at[0] + length, at[1] + side * corona * 0.6],
                1.0,
                ui(def.corona.rim_hot, 0.6),
            );
        }
    }
    if let Some(serpent) = def.serpent {
        let gold = ui(serpent.color, (0.55 + 0.1 * serpent.brightness).min(1.0));
        let point = |k: f32| {
            let phase = tau * (serpent.coils * k - t * serpent.speed);
            [
                at[0] + k * length,
                at[1] + phase.sin() * corona * 0.6 * serpent.amplitude,
            ]
        };
        for step in 0..30 {
            let k = step as f32 / 30.0 * 0.9;
            stroke(canvas, frame, point(k), point(k + 0.03), 2.4, gold);
        }
        let head = point(0.9);
        disc(canvas, frame, head[0], head[1], 4.0, gold);
        disc(
            canvas,
            frame,
            head[0] + 1.5,
            head[1] - 1.5,
            1.0,
            Color::new(1.0, 0.1, 0.05, 1.0),
        );
    }
    if let Some(strands) = def.strands {
        for index in 0..strands.count.min(14) {
            let i = index as f32;
            let spread = draw(i * 1.3 + 0.2) * 2.0 - 1.0;
            let tip = length * (0.72 + 0.28 * draw(i * 7.7 + 0.5));
            let sway = (t * 1.2 + i).sin() * 2.0 * strands.sway;
            let end = [at[0] + tip, at[1] + spread * corona * strands.spread + sway];
            let colour = crate::saber_skins::turn_hue(
                [1.0, 0.35, 0.35],
                strands.hue + draw(i * 4.1) * 0.35 + t * 0.04,
            );
            stroke(canvas, frame, at, end, 0.8, ui(colour, 0.35));
            disc(canvas, frame, end[0], end[1], 2.2, ui(colour, 0.95));
        }
    }
    if let Some(butterflies) = def.butterflies {
        let wing = ui(
            butterflies.color,
            (0.6 + 0.1 * butterflies.brightness).min(1.0),
        );
        for index in 0..butterflies.count.min(6) {
            let i = index as f32;
            let flying =
                fract(t / (6.0 + 4.0 * draw(i + 0.3)) + draw(i + 1.1) * 3.7) < butterflies.flight;
            let lift = if flying {
                -12.0 - 6.0 * (t * 2.0 + i).sin()
            } else {
                0.0
            };
            let x = at[0] + (0.12 + 0.78 * draw(i + 2.9)) * length;
            let y = at[1] - corona * 0.5 + lift;
            let open = 0.5 + 0.5 * (t * if flying { 9.0 } else { 0.8 } + i).sin().abs();
            for side in [-1.0, 1.0] {
                disc(canvas, frame, x + side * 2.6 * open, y - 1.0, 2.6, wing);
                disc(canvas, frame, x + side * 2.0 * open, y + 2.0, 1.8, wing);
            }
        }
    }
    if let Some(wings) = def.wings {
        let unfold = 0.35 + 0.65 * ((tau * t * wings.rate).sin() * 0.5 + 0.5);
        let origin = [at[0] + length * 0.15, at[1]];
        for side in [-1.0, 1.0] {
            for feather in 0..wings.feathers {
                let k = feather as f32 / (wings.feathers - 1).max(1) as f32;
                let angle = (0.25 + 1.05 * k) * unfold;
                let span = 30.0 * (0.55 + 0.45 * (1.0 - (k - 0.6).abs()));
                stroke(
                    canvas,
                    frame,
                    origin,
                    [
                        origin[0] + angle.cos() * span,
                        origin[1] - side * angle.sin() * span,
                    ],
                    2.2,
                    ui(mix(wings.color, [1.0, 0.9, 0.6], 0.3 * (1.0 - k)), 0.55),
                );
            }
        }
    }
}

/// The seventh set (`blade_skin_solids.rs`) over the swatch's blade, drawn last as the
/// solid pass is: a liquid mirror (dark below, bright above, a highlight, beads rolling
/// along), and ink (halftone dots along the glow's edge, a dark body with a glossy streak,
/// a black outline, a pointed tip).
#[allow(clippy::too_many_arguments)]
pub(super) fn solids(
    canvas: &mut DrawList,
    frame: &Frame,
    def: &BladeSkinDef,
    at: [f32; 2],
    length: f32,
    corona: f32,
    t: f32,
    ui: &dyn Fn(Rgb, f32) -> Color,
) {
    if let Some(chrome) = def.chrome {
        let half = 4.5 * chrome.width;
        capsule(
            canvas,
            frame,
            [at[0], at[1] - half, length, half * 2.0],
            ui(mix(chrome.color, [0.1; 3], 0.6), 1.0),
        );
        capsule(
            canvas,
            frame,
            [at[0], at[1] - half, length, half],
            ui(chrome.color, 0.95),
        );
        stroke(
            canvas,
            frame,
            [at[0] + 2.0, at[1] - half * 0.45],
            [at[0] + length - 2.0, at[1] - half * 0.45],
            1.2,
            Color::new(1.0, 1.0, 1.0, 0.9),
        );
        for bead in 0..(chrome.beads.round() as usize).min(4) {
            let along = fract(draw(bead as f32 + 0.7) + t * chrome.speed / BLADE_UNITS);
            disc(
                canvas,
                frame,
                at[0] + along * length,
                at[1],
                half * 1.5,
                ui(chrome.color, 0.9),
            );
        }
    }
    let Some(ink) = def.ink else {
        return;
    };
    if ink.halftone > 0.0 {
        let step = (6.0 / ink.halftone.max(0.5)).clamp(3.0, 8.0);
        let mut x = at[0] + step;
        while x < at[0] + length {
            for side in [-1.0, 1.0] {
                disc(
                    canvas,
                    frame,
                    x,
                    at[1] + side * corona * 0.75,
                    1.3,
                    ui(def.corona.rim_hot, 0.85),
                );
            }
            x += step;
        }
    }
    let half = 4.0 * ink.width;
    let point = (ink.point / BLADE_UNITS * length).min(length * 0.5);
    if ink.body > 0.0 {
        let colour = Color::new(ink.color[0], ink.color[1], ink.color[2], ink.body);
        capsule(
            canvas,
            frame,
            [at[0], at[1] - half, length - point, half * 2.0],
            colour,
        );
        for k in 0..6 {
            let f = k as f32 / 6.0;
            let w = half * (1.0 - f);
            stroke(
                canvas,
                frame,
                [at[0] + length - point + f * point, at[1] - w],
                [at[0] + length - point + f * point, at[1] + w],
                point / 6.0 + 1.0,
                colour,
            );
        }
        if ink.gloss > 0.0 {
            let slide = fract(t * 0.13);
            stroke(
                canvas,
                frame,
                [at[0] + slide * length * 0.6, at[1] - half * 0.35],
                [
                    at[0] + slide * length * 0.6 + length * 0.25,
                    at[1] - half * 0.35,
                ],
                1.2,
                Color::new(1.0, 1.0, 1.0, 0.5 * ink.gloss),
            );
        }
    }
    if ink.outline > 0.0 {
        let black = Color::new(0.0, 0.0, 0.0, ink.outline);
        let edge = (ink.edge * 6.0).clamp(1.0, 3.0);
        for side in [-1.0, 1.0] {
            stroke(
                canvas,
                frame,
                [at[0], at[1] + side * half],
                [at[0] + length - point, at[1] + side * half],
                edge,
                black,
            );
            stroke(
                canvas,
                frame,
                [at[0] + length - point, at[1] + side * half],
                [at[0] + length + if point > 0.0 { 0.0 } else { half }, at[1]],
                edge,
                black,
            );
        }
    }
}
