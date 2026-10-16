//! A new medal's ceremony, shared by the pop-up's two looks (`sjk_view.rs`,
//! `classic_view.rs`): when it is, and how the medal and its light are drawn.
//!
//! The unlock pop-up's card (`unlock_toast.rs`) is small and brief, and only announces
//! a medal during a match; a medal is rarer and given by hand, so it takes the middle of
//! the screen. Over
//! [`ENTRANCE`] seconds the whole medal comes down into place on its ribbon, growing
//! from a third of its size past its own and settling (ease-out, a little overshoot);
//! as it lands, light bursts from the medallion (a glow, two gold rings running out,
//! a flash and sparks thrown out and falling), a gold arc sweeps round it and leaves a
//! ring of ticks (the main page's ring, turning as slowly), a band of light crosses
//! the medal, and the words fade up one after another: what it is, the name, what it
//! is for and when it was given, the team's note, then the button and its key. While
//! it waits, it breathes: the glow swells and fades, a softer band of light crosses
//! it every [`GLINT_PERIOD`] seconds and a few sparkles twinkle round it. On Next or
//! Close everything lifts away and fades in [`EXIT`] seconds.
//!
//! Everything is a draw-list primitive whose number is fixed (no allocation, a
//! bounded count of sparks and sparkles), sized from the medal's own size so either
//! look can place it. Every moment is a function of the seconds since the medal
//! arrived, so the pop-up can be held still at any of them for its tests and shots.

use crate::menu_widgets::MenuCanvas;
use sjk_ui::{Color, DrawCommand, Rect, TextureId};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Seconds from a medal's arrival until it stands still with all its words.
pub(crate) const ENTRANCE: f32 = 1.8;
/// Seconds a medal takes to lift away on Next or Close.
pub(crate) const EXIT: f32 = 0.32;
/// Seconds the darkness under the first medal of a sitting takes to come.
pub(crate) const SCRIM_IN: f32 = 0.3;
/// Seconds between two of the soft bands of light that cross a waiting medal.
pub(crate) const GLINT_PERIOD: f32 = 6.0;
/// Seconds one band of light takes to cross the medal.
const GLINT_TIME: f32 = 0.9;
/// When the entrance's band of light crosses, from and to (seconds).
const SWEEP: (f32, f32) = (0.95, 1.65);
/// Seconds of one breath of the glow behind a waiting medal (the emblem's core).
const BREATH: f32 = 4.2;
/// Seconds the ring of ticks takes to turn once: the main page's ring's four minutes.
const RING_TURN: f32 = 240.0;

/// Where the medallion sits in a medal's whole picture (`medals.rs`: the medal hanging
/// from its ribbon, 512 square), as shares of the picture's side: its middle and its
/// radius, measured from the bundled pictures' opaque pixels.
pub(crate) const MEDALLION: [f32; 2] = [0.5, 0.69];
pub(crate) const MEDALLION_RADIUS: f32 = 0.29;
/// The gold ring the sweep leaves round the medallion, and the ring of ticks just
/// outside it, against the medallion's radius; and its ticks.
pub(crate) const RING: f32 = 1.42;
pub(crate) const TICK_RING: f32 = 1.6;
const TICKS: usize = 24;
/// The medal's size as it begins its flight, against its own, and how far above its
/// place it begins, against its size.
const START_SCALE: f32 = 0.3;
const START_DROP: f32 = 0.3;

/// When each group of words fades up (seconds), by [`Words`].
const WORD_TIMES: [(f32, f32); Words::COUNT] = [
    (0.7, 1.05),
    (0.85, 1.25),
    (1.0, 1.4),
    (1.15, 1.6),
    (1.3, 1.7),
];

/// The pop-up's groups of words, which come one after another.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Words {
    /// "New medal", how many, and who it is from.
    Kicker,
    /// The medal's name.
    Name,
    /// What it is for and when it was given.
    Details,
    /// The team's note.
    Note,
    /// The button and its key.
    Actions,
}

impl Words {
    pub(crate) const COUNT: usize = 5;
}

/// The sparks the burst throws: direction (degrees, 0 to the right, clockwise), how
/// far each flies against the medal's size, when it leaves (seconds) and its size
/// against the medal's.
const SPARKS: [(f32, f32, f32, f32); 20] = [
    (-90.0, 0.42, 0.50, 0.012),
    (-68.0, 0.30, 0.55, 0.009),
    (-47.0, 0.50, 0.52, 0.011),
    (-25.0, 0.36, 0.58, 0.008),
    (-6.0, 0.55, 0.51, 0.012),
    (14.0, 0.33, 0.56, 0.009),
    (33.0, 0.47, 0.53, 0.011),
    (52.0, 0.29, 0.60, 0.008),
    (71.0, 0.40, 0.54, 0.010),
    (90.0, 0.26, 0.57, 0.008),
    (109.0, 0.41, 0.52, 0.010),
    (128.0, 0.31, 0.59, 0.008),
    (147.0, 0.48, 0.53, 0.011),
    (166.0, 0.34, 0.56, 0.009),
    (186.0, 0.55, 0.50, 0.012),
    (205.0, 0.36, 0.58, 0.008),
    (227.0, 0.50, 0.52, 0.011),
    (248.0, 0.30, 0.55, 0.009),
    (266.0, 0.44, 0.61, 0.010),
    (300.0, 0.38, 0.54, 0.009),
];
/// The discs of the glow behind the medallion.
const GLOW_DISCS: usize = 8;
/// Seconds a spark flies.
const SPARK_LIFE: f32 = 1.0;
/// How far a spark falls by the end of its flight, against the medal's size.
const SPARK_FALL: f32 = 0.12;

/// The sparkles round a waiting medal: direction (degrees), distance from the
/// medallion's middle against the medal's size, and where in its cycle each begins
/// (seconds).
const TWINKLES: [(f32, f32, f32); 6] = [
    (-128.0, 0.47, 0.0),
    (-34.0, 0.52, 1.3),
    (18.0, 0.44, 2.4),
    (152.0, 0.50, 0.7),
    (200.0, 0.45, 1.9),
    (-62.0, 0.58, 2.9),
];
/// Seconds between two twinkles of one sparkle, and how long each lasts.
const TWINKLE_PERIOD: f32 = 3.4;
const TWINKLE_TIME: f32 = 0.8;

/// How the pop-up stands `t` seconds after the medal arrived, and `exit` seconds after
/// it began to leave.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Moment {
    /// The medal's opacity.
    pub(crate) medal: f32,
    /// The medal's size against its own.
    pub(crate) scale: f32,
    /// How far above its place the medal is, against its size.
    pub(crate) drop: f32,
    /// Each group of words' opacity, by [`Words`].
    pub(crate) words: [f32; Words::COUNT],
    /// Everything's opacity as the medal leaves (1 until then).
    pub(crate) leave: f32,
}

impl Moment {
    pub(crate) fn at(t: f32, exit: Option<f32>) -> Self {
        let flight = span(t, 0.05, 0.75);
        let gone = exit.map_or(0.0, |exit| span(exit, 0.0, EXIT));
        let words_gone = exit.map_or(0.0, |exit| span(exit, 0.0, EXIT * 0.6));
        Self {
            medal: ease_out_cubic(span(t, 0.05, 0.4)),
            scale: (START_SCALE + (1.0 - START_SCALE) * ease_out_back(flight))
                * (1.0 + 0.08 * gone),
            drop: START_DROP * (1.0 - ease_out_cubic(flight)) + 0.1 * gone * gone,
            words: WORD_TIMES
                .map(|(from, to)| ease_out_cubic(span(t, from, to)) * (1.0 - words_gone)),
            leave: 1.0 - gone * gone,
        }
    }

    /// The opacity of `words`.
    pub(crate) fn of(&self, words: Words) -> f32 {
        self.words[words as usize]
    }
}

/// How far the words still have to rise into place, as a share of their line, at
/// `opacity`.
pub(crate) fn rise(opacity: f32) -> f32 {
    1.0 - opacity
}

/// The darkness under the pop-up as a share of its full depth, `since` seconds after
/// the sitting's first medal arrived.
pub(crate) fn scrim(since: f32) -> f32 {
    ease_out_cubic(span(since, 0.0, SCRIM_IN))
}

/// How far `t` is from `from` to `to`, from 0 to 1.
pub(crate) fn span(t: f32, from: f32, to: f32) -> f32 {
    ((t - from) / (to - from)).clamp(0.0, 1.0)
}

pub(crate) fn ease_out_cubic(x: f32) -> f32 {
    1.0 - (1.0 - x).powi(3)
}

fn ease_in_out_cubic(x: f32) -> f32 {
    if x < 0.5 {
        4.0 * x * x * x
    } else {
        1.0 - (-2.0 * x + 2.0).powi(3) * 0.5
    }
}

/// Past 1 a little before settling, for a medal that lands.
fn ease_out_back(x: f32) -> f32 {
    const C1: f32 = 1.4;
    const C3: f32 = C1 + 1.0;
    1.0 + C3 * (x - 1.0).powi(3) + C1 * (x - 1.0).powi(2)
}

/// The colours a look draws the ceremony in.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Palette {
    /// The rings and the sweep.
    pub(crate) gold: Color,
    /// The glow, the flash and the sparks' tails.
    pub(crate) bright: Color,
    /// The ring of ticks.
    pub(crate) line: Color,
}

/// The burst's strength `t` seconds in: up quickly as the medal lands, then down to
/// nothing over a second and a half.
pub(crate) fn burst(t: f32) -> f32 {
    span(t, 0.45, 0.6) * (1.0 - ease_out_cubic(span(t, 0.6, 1.9)))
}

/// The glow behind the medallion: a steady light that comes with the medal, the
/// burst on top of it, and once the medal waits a slow breath.
pub(crate) fn glow(t: f32) -> f32 {
    let steady = 0.6 * span(t, 0.4, 1.0);
    let breath = if t > ENTRANCE {
        0.12 * (0.5 - 0.5 * (TAU * (t - ENTRANCE) / BREATH).cos())
    } else {
        0.0
    };
    steady + 1.4 * burst(t) + breath
}

/// Where a band of light is across the medal `t` seconds in (its middle, from left of
/// the medal to right of it, as a share of its width) and how strong it is: the
/// entrance's, then a softer one every [`GLINT_PERIOD`] seconds while it waits.
pub(crate) fn glint(t: f32) -> Option<(f32, f32)> {
    let (from, to) = SWEEP;
    let (progress, strength) = if t < to {
        (span(t, from, to), 1.0)
    } else {
        let waited = t - ENTRANCE - 1.5;
        if waited < 0.0 {
            return None;
        }
        (waited.rem_euclid(GLINT_PERIOD) / GLINT_TIME, 0.45)
    };
    if progress <= 0.0 || progress >= 1.0 {
        return None;
    }
    let eased = ease_in_out_cubic(progress);
    Some((-0.3 + 1.6 * eased, strength * (PI * progress).sin()))
}

/// Where the medal is drawn: its picture's settled square in window pixels.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Stage {
    pub(crate) rect: Rect,
    pub(crate) texture: TextureId,
    pub(crate) palette: Palette,
}

impl Stage {
    /// The medallion's middle and radius in the picture drawn over `rect`.
    fn medallion(rect: Rect) -> ([f32; 2], f32) {
        (
            [
                rect.x + rect.width * MEDALLION[0],
                rect.y + rect.height * MEDALLION[1],
            ],
            rect.width * MEDALLION_RADIUS,
        )
    }

    /// The picture's square `t` seconds in, as [`Moment`] places it.
    pub(crate) fn drawn(&self, moment: &Moment) -> Rect {
        let size = self.rect.width * moment.scale;
        // It grows about its medallion, so the light stays where it bursts.
        let ([x, y], _) = Self::medallion(self.rect);
        Rect::new(
            x - size * MEDALLION[0],
            y - size * MEDALLION[1] - moment.drop * self.rect.width,
            size,
            size,
        )
    }

    /// Draw the medal and its light `t` seconds after it arrived (and `exit` seconds
    /// after it began to leave) on `canvas`.
    pub(crate) fn draw(&self, canvas: &mut MenuCanvas, t: f32, exit: Option<f32>) {
        let moment = Moment::at(t, exit);
        let size = self.rect.width;
        let unit = size / 340.0;
        let (centre, medallion) = Self::medallion(self.rect);
        let palette = self.palette;
        canvas.push_opacity(moment.leave);
        let list = canvas.draw_list_mut();

        // The light behind the medallion: discs of a little light each, so it fades
        // out smoothly rather than in rings.
        let glow = glow(t);
        let grow = 0.9 + 0.12 * burst(t);
        for step in 0..GLOW_DISCS {
            let factor = 2.8 - 1.6 * step as f32 / (GLOW_DISCS - 1) as f32;
            disc(
                list,
                centre,
                medallion * factor * grow,
                with_alpha(palette.bright, (0.032 * glow).min(0.12)),
            );
        }
        // The ring of ticks, turning as the main page's ring does.
        let ticks = 0.4 * ease_out_cubic(span(t, 0.6, 1.3));
        if ticks > 0.0 {
            let turn = TAU * t / RING_TURN;
            let step = TAU / TICKS as f32;
            for tick in 0..TICKS {
                let _ = list.push(DrawCommand::Arc {
                    center: centre,
                    radius: medallion * TICK_RING,
                    width: 2.0 * unit,
                    start: turn + step * tick as f32,
                    sweep: step * 0.3,
                    color: with_alpha(palette.line, ticks),
                    knockout: None,
                });
            }
        }
        // A gold arc sweeping round the ring, which stays as a thin circle.
        let sweep = ease_in_out_cubic(span(t, 0.35, 1.15));
        if sweep > 0.0 {
            let _ = list.push(DrawCommand::Arc {
                center: centre,
                radius: medallion * RING,
                width: 2.2 * unit,
                start: -FRAC_PI_2,
                sweep: TAU * sweep,
                color: with_alpha(palette.gold, 0.85 - 0.3 * span(t, 1.15, 1.6)),
                knockout: None,
            });
            if sweep < 1.0 {
                let angle = -FRAC_PI_2 + TAU * sweep;
                let point = [
                    centre[0] + medallion * RING * angle.cos(),
                    centre[1] + medallion * RING * angle.sin(),
                ];
                disc(list, point, 9.0 * unit, with_alpha(palette.bright, 0.3));
                disc(list, point, 3.5 * unit, Color::new(1.0, 0.97, 0.88, 1.0));
            }
        }
        // Two rings of light running out from the medallion as it lands.
        for (from, to) in [(0.5, 1.4), (0.62, 1.6)] {
            let wave = span(t, from, to);
            if wave > 0.0 && wave < 1.0 {
                let _ = list.push(DrawCommand::Arc {
                    center: centre,
                    radius: medallion * (1.0 + 1.6 * ease_out_cubic(wave)),
                    width: (6.0 * (1.0 - wave) + 1.0) * unit,
                    start: 0.0,
                    sweep: TAU,
                    color: with_alpha(palette.gold, 0.85 * (1.0 - wave).powf(1.5)),
                    knockout: None,
                });
            }
        }

        // The medal itself, then a band of light crossing it.
        let drawn = self.drawn(&moment);
        let _ = list.push(DrawCommand::TexturedQuad {
            rect: drawn,
            texture: self.texture,
            color: Color::new(1.0, 1.0, 1.0, moment.medal),
        });
        if let Some((at, strength)) = glint(t) {
            shine(list, drawn, self.texture, at, strength);
        }
        // A flash over the medallion as it lands.
        let flash = span(t, 0.5, 0.95);
        if flash > 0.0 && flash < 1.0 {
            disc(
                list,
                centre,
                medallion * (1.0 + 0.2 * flash),
                with_alpha(palette.bright, 0.5 * (1.0 - flash).powi(2)),
            );
        }
        // Sparks thrown out from it, each a bright head and a fading tail, falling a
        // little as they slow.
        for (degrees, travel, delay, spark) in SPARKS {
            let life = span(t, delay, delay + SPARK_LIFE);
            if life <= 0.0 || life >= 1.0 {
                continue;
            }
            let (sin, cos) = degrees.to_radians().sin_cos();
            let out = medallion * 0.95 + travel * size * ease_out_cubic(life);
            let fall = SPARK_FALL * size * life * life;
            let fade = (1.0 - life).powf(1.4);
            for (back, shrink, dim) in [(0.0, 1.0, 1.0), (0.05, 0.7, 0.55), (0.09, 0.45, 0.25)] {
                let reach = (out - back * size * (1.0 - life)).max(medallion * 0.95);
                let point = [centre[0] + cos * reach, centre[1] + sin * reach + fall];
                let colour = if back == 0.0 {
                    Color::new(1.0, 0.96, 0.85, 1.0)
                } else {
                    palette.bright
                };
                disc(
                    list,
                    point,
                    spark * size * shrink * (1.0 - 0.5 * life),
                    with_alpha(colour, fade * dim),
                );
            }
        }
        // Sparkles twinkling round it once it has landed.
        if t > 1.2 {
            for (degrees, distance, offset) in TWINKLES {
                let cycle = (t - 1.2 + offset).rem_euclid(TWINKLE_PERIOD);
                if cycle >= TWINKLE_TIME {
                    continue;
                }
                let light = (PI * cycle / TWINKLE_TIME).sin();
                let (sin, cos) = degrees.to_radians().sin_cos();
                let point = [
                    centre[0] + cos * distance * size,
                    centre[1] + sin * distance * size,
                ];
                sparkle(list, point, 0.04 * size * (0.6 + 0.4 * light), light * 0.8);
            }
        }
        canvas.pop_opacity();
    }
}

/// A disc of `radius` round `centre`.
pub(crate) fn disc(list: &mut sjk_ui::DrawList, centre: [f32; 2], radius: f32, colour: Color) {
    let _ = list.push(DrawCommand::RoundedRect {
        rect: Rect::new(
            centre[0] - radius,
            centre[1] - radius,
            radius * 2.0,
            radius * 2.0,
        ),
        radius,
        color: colour,
    });
}

/// A four-pointed sparkle `reach` long each way from `centre`: two thin bars.
pub(crate) fn sparkle(list: &mut sjk_ui::DrawList, centre: [f32; 2], reach: f32, alpha: f32) {
    let thin = (reach * 0.16).max(1.0);
    let colour = Color::new(1.0, 0.97, 0.88, alpha);
    for (width, height) in [(reach * 2.0, thin), (thin, reach * 2.0)] {
        let _ = list.push(DrawCommand::RoundedRect {
            rect: Rect::new(
                centre[0] - width * 0.5,
                centre[1] - height * 0.5,
                width,
                height,
            ),
            radius: thin * 0.5,
            color: colour,
        });
    }
}

/// Strips of the band of light, which [`shine`] draws.
pub(crate) const SHINE_STRIPS: usize = 9;
/// The band's width against the medal's.
const SHINE_WIDTH: f32 = 0.26;
/// How much brighter the band makes the medal at its middle: the picture drawn again
/// over itself, tinted past white, so only the medal's own pixels light up.
pub(crate) const SHINE_TINT: [f32; 3] = [2.4, 2.2, 1.7];
/// The band's strongest opacity, at its middle.
const SHINE_ALPHA: f32 = 0.75;

/// A band of light across the medal drawn over `rect`, its middle at `at` of its
/// width, `strength` at most 1: the picture's own strips drawn again brighter, most
/// in the band's middle, so the light follows the medal's shape and not a box.
fn shine(list: &mut sjk_ui::DrawList, rect: Rect, texture: TextureId, at: f32, strength: f32) {
    let strip = SHINE_WIDTH / SHINE_STRIPS as f32;
    for index in 0..SHINE_STRIPS {
        let from = at - SHINE_WIDTH * 0.5 + strip * index as f32;
        let (left, right) = (from.max(0.0), (from + strip).min(1.0));
        if right <= left {
            continue;
        }
        let middle = (index as f32 + 0.5) / SHINE_STRIPS as f32;
        let profile = 1.0 - (2.0 * middle - 1.0).abs();
        let [r, g, b] = SHINE_TINT;
        let _ = list.push(DrawCommand::TexturedQuadUv {
            rect: Rect::new(
                rect.x + left * rect.width,
                rect.y,
                (right - left) * rect.width,
                rect.height,
            ),
            texture,
            color: Color::new(r, g, b, SHINE_ALPHA * strength * profile),
            uv: [[left, 0.0], [right, 0.0], [right, 1.0], [left, 1.0]],
        });
    }
}

pub(crate) fn with_alpha(colour: Color, alpha: f32) -> Color {
    Color::new(colour.r, colour.g, colour.b, colour.a * alpha)
}

/// Up to `N` lines of `text`, cut at spaces where a line would no longer `fit`; a
/// word too long for a line stands alone (the renderer cuts it with an ellipsis), and
/// what is left past the last line goes on it. Returns the lines and how many there
/// are; nothing is allocated.
pub(crate) fn wrap<const N: usize>(text: &str, fits: impl Fn(&str) -> bool) -> ([&str; N], usize) {
    let mut lines = [""; N];
    let mut count = 0;
    let text = text.trim();
    let mut start = 0;
    while start < text.len() && count < N {
        if count == N - 1 {
            lines[count] = &text[start..];
            count += 1;
            break;
        }
        let rest = &text[start..];
        // The longest run of whole words from `start` that fits.
        let mut end = None;
        for (at, _) in rest
            .match_indices(' ')
            .chain(std::iter::once((rest.len(), "")))
        {
            if fits(rest[..at].trim_end()) {
                end = Some(at);
            } else {
                break;
            }
        }
        let end = end.unwrap_or_else(|| rest.find(' ').unwrap_or(rest.len()));
        lines[count] = rest[..end].trim_end();
        count += 1;
        start += end;
        start += text[start..].len() - text[start..].trim_start().len();
    }
    (lines, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_medal_comes_down_lands_and_settles() {
        let start = Moment::at(0.0, None);
        assert_eq!(start.medal, 0.0);
        assert!(start.scale <= START_SCALE + 1e-4 && start.drop >= START_DROP - 1e-4);
        assert!(start.words.iter().all(|words| *words == 0.0));
        // Past its size for a moment as it lands, then still.
        let landing = (0..=40)
            .map(|step| Moment::at(0.05 + 0.7 * step as f32 / 40.0, None).scale)
            .fold(0.0_f32, f32::max);
        assert!(landing > 1.02 && landing < 1.1, "{landing}");
        for t in [ENTRANCE, ENTRANCE + 3.0, ENTRANCE + 60.0] {
            let settled = Moment::at(t, None);
            assert!((settled.scale - 1.0).abs() < 1e-4 && settled.drop.abs() < 1e-4);
            assert!(settled.medal == 1.0 && settled.leave == 1.0);
            assert!(
                settled
                    .words
                    .iter()
                    .all(|words| (*words - 1.0).abs() < 1e-4)
            );
        }
    }

    #[test]
    fn the_words_come_one_after_another_and_all_by_the_end_of_the_entrance() {
        for pair in WORD_TIMES.windows(2) {
            assert!(pair[0].0 < pair[1].0 && pair[0].1 < pair[1].1, "{pair:?}");
        }
        assert!(WORD_TIMES.iter().all(|(_, to)| *to <= ENTRANCE));
        let early = Moment::at(1.0, None);
        assert!(early.of(Words::Kicker) > early.of(Words::Name));
        assert!(early.of(Words::Name) > early.of(Words::Details));
        assert_eq!(early.of(Words::Actions), 0.0);
    }

    #[test]
    fn the_burst_sparks_and_rings_end_before_the_medal_settles() {
        assert_eq!(burst(0.3), 0.0);
        assert!(burst(0.6) > 0.95);
        assert!(burst(ENTRANCE + 0.2) < 1e-3);
        for (_, _, delay, _) in SPARKS {
            assert!(
                delay + SPARK_LIFE < ENTRANCE,
                "a spark outlives the entrance"
            );
        }
        assert!(SWEEP.1 < ENTRANCE);
    }

    #[test]
    fn a_waiting_medal_breathes_and_glints_gently_and_periodically() {
        // The glow stays within its band while it waits.
        let waiting: Vec<f32> = (0..200)
            .map(|step| glow(ENTRANCE + step as f32 * 0.1))
            .collect();
        let (low, high) = waiting.iter().fold((f32::MAX, f32::MIN), |(low, high), g| {
            (low.min(*g), high.max(*g))
        });
        assert!(low >= 0.6 - 1e-4 && high <= 0.72 + 1e-4 && high - low > 0.1);
        // The entrance's band of light, then none for a while, then soft ones.
        assert!(glint(1.3).is_some_and(|(_, strength)| strength > 0.9));
        assert!(glint(ENTRANCE + 1.0).is_none());
        let first = ENTRANCE + 1.5 + GLINT_TIME * 0.5;
        let soft = glint(first).expect("a soft glint");
        assert!(soft.1 <= 0.45 + 1e-4 && soft.1 > 0.4);
        let again = glint(first + GLINT_PERIOD).expect("the next soft glint");
        assert!((again.0 - soft.0).abs() < 1e-3 && (again.1 - soft.1).abs() < 1e-3);
        assert!(glint(first + GLINT_TIME).is_none());
    }

    #[test]
    fn leaving_lifts_and_fades_everything() {
        let half = Moment::at(ENTRANCE + 2.0, Some(EXIT * 0.5));
        assert!(half.leave > 0.0 && half.leave < 1.0 && half.drop > 0.0);
        assert!(half.of(Words::Name) < half.leave, "the words go first");
        let gone = Moment::at(ENTRANCE + 2.0, Some(EXIT));
        assert!(gone.leave.abs() < 1e-5);
        assert!(gone.words.iter().all(|words| *words == 0.0));
        // Leaving during the entrance is the same lift.
        assert!(Moment::at(0.5, Some(EXIT)).leave.abs() < 1e-5);
    }

    #[test]
    fn the_scrim_comes_in_quickly() {
        assert_eq!(scrim(0.0), 0.0);
        assert!(scrim(SCRIM_IN * 0.5) > 0.5);
        assert_eq!(scrim(SCRIM_IN), 1.0);
    }

    /// Every moment draws a bounded number of commands; nothing is drawn by a spark,
    /// sparkle or band of light beyond the fixed tables.
    #[test]
    fn every_moment_draws_a_bounded_number_of_commands() {
        let stage = Stage {
            rect: Rect::new(790.0, 176.0, 340.0, 340.0),
            texture: crate::medals::Medal::BugHunter.art(),
            palette: Palette {
                gold: Color::new(1.0, 0.8, 0.3, 1.0),
                bright: Color::new(1.0, 0.9, 0.5, 1.0),
                line: Color::new(0.6, 0.8, 1.0, 1.0),
            },
        };
        let most = GLOW_DISCS + TICKS + 3 + 2 + 1 + SHINE_STRIPS + 1 + SPARKS.len() * 3 + 12 + 2;
        let mut canvas = MenuCanvas::with_capacities(4, 16, 512);
        let mut peak = 0;
        for step in 0..=400 {
            let t = step as f32 * 0.025;
            for exit in [None, Some(EXIT * 0.5)] {
                canvas.begin_transparent([1920.0, 1080.0]);
                stage.draw(&mut canvas, t, exit);
                peak = peak.max(canvas.draw_list().len());
                let shines = canvas
                    .draw_list()
                    .commands()
                    .iter()
                    .filter(|command| matches!(command, DrawCommand::TexturedQuadUv { .. }))
                    .count();
                assert!(shines <= SHINE_STRIPS);
            }
        }
        assert!(peak <= most && peak < 160, "{peak} of {most}");
    }

    /// The band of light lies on the medal, brighter than white in its middle.
    #[test]
    fn the_band_of_light_lies_on_the_medal() {
        let rect = Rect::new(100.0, 50.0, 300.0, 300.0);
        let texture = crate::medals::Medal::EarlyTester.art();
        for at in [-0.2, 0.0, 0.3, 0.5, 0.97, 1.2] {
            let mut list = sjk_ui::DrawList::new(16);
            shine(&mut list, rect, texture, at, 1.0);
            for command in list.commands() {
                let DrawCommand::TexturedQuadUv {
                    rect: strip,
                    color,
                    uv,
                    ..
                } = command
                else {
                    panic!("a strip");
                };
                assert!(strip.x >= rect.x - 1e-3 && strip.right() <= rect.right() + 1e-3);
                assert!((uv[0][0] - (strip.x - rect.x) / rect.width).abs() < 1e-4);
                assert!(color.r > 1.0 && color.a <= SHINE_ALPHA);
            }
        }
        let mut list = sjk_ui::DrawList::new(16);
        shine(&mut list, rect, texture, 0.5, 1.0);
        assert_eq!(list.len(), SHINE_STRIPS);
    }

    #[test]
    fn the_medal_grows_about_its_medallion() {
        let stage = Stage {
            rect: Rect::new(0.0, 0.0, 400.0, 400.0),
            texture: crate::medals::Medal::BugHunter.art(),
            palette: Palette {
                gold: Color::new(1.0, 0.8, 0.3, 1.0),
                bright: Color::new(1.0, 0.9, 0.5, 1.0),
                line: Color::new(0.6, 0.8, 1.0, 1.0),
            },
        };
        let settled = stage.drawn(&Moment::at(ENTRANCE, None));
        assert!((settled.x).abs() < 1e-3 && settled.y.abs() < 1e-3);
        assert!((settled.width - 400.0).abs() < 1e-3);
        let small = Moment {
            drop: 0.0,
            ..Moment::at(0.0, None)
        };
        let drawn = stage.drawn(&small);
        let middle = [
            drawn.x + drawn.width * MEDALLION[0],
            drawn.y + drawn.height * MEDALLION[1],
        ];
        assert!((middle[0] - 200.0).abs() < 1e-3 && (middle[1] - 276.0).abs() < 1e-3);
    }

    #[test]
    fn lines_wrap_at_spaces_within_their_room() {
        let fits = |line: &str| line.chars().count() <= 10;
        let (lines, count) = wrap::<4>("one two three four five six", fits);
        assert_eq!(&lines[..count], ["one two", "three four", "five six"]);
        // A word too long stands alone; the rest goes on the last line.
        let (lines, count) = wrap::<2>("abcdefghijklmnop qr st uv wx yz", fits);
        assert_eq!(&lines[..count], ["abcdefghijklmnop", "qr st uv wx yz"]);
        let (_, count) = wrap::<3>("   ", fits);
        assert_eq!(count, 0);
        let (lines, count) = wrap::<3>("Ça été  très   bien", fits);
        assert_eq!(&lines[..count], ["Ça été", "très", "bien"]);
    }
}
