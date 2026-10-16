//! The drop pop-up's look (`docs/holocrons.md`, "The pop-up"): the screen darkens and the
//! holocron floats in the middle of it in its tier's colour, in a ring of ticks, with the
//! words centred under it in the SJK UI's families (Inter until they load) and its button
//! from the kit (gold, Next or Close), the key bottom right as on the other screens. It is
//! drawn the same under the SJK UI and the classic menus. Laid out on the UI's 16:9 frame
//! in 1080-line pixels.
//!
//! The ceremony is the medal pop-up's ([`crate::medal_popup::award`]) in the tier's
//! colour: the holocron comes down into place, a burst of light leaves it (a glow, two
//! rings, sparks), a ring of ticks turns round it and a soft glow breathes while it waits;
//! the words fade up one group after another. Everything is a draw-list primitive of a fixed
//! number, so a frame allocates nothing.

use super::{HolocronPopup, NEXT_TOKEN, SCREEN_TOKEN, View};
use crate::holocrons::gem;
use crate::medal_popup::award::{self, Moment, Words};
use crate::menu::sjk::{Frame, TextTarget, color, key_hint, key_hint_width, kit, text};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use crate::sjk_chat_look::Measure;
use crate::text::TextFace;
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};
use std::f32::consts::TAU;
use std::time::Instant;

/// The column the words are centred in.
const CENTRE_X: f32 = 960.0;
const COLUMN: f32 = 880.0;
/// "New holocron" and where it came from (their middle lines).
const KICKER_Y: f32 = 100.0;
const FROM_Y: f32 = 134.0;
/// The holocron: its middle's height and its side.
const ICON_Y: f32 = 340.0;
const ICON_SIZE: f32 = 250.0;
/// The name's middle line, then the steps between the lines under it.
const NAME_Y: f32 = 580.0;
const DETAILS_GAP: f32 = 52.0;
const LINE: f32 = 30.0;
const NOTE_GAP: f32 = 16.0;
const NOTE_LINE: f32 = 28.0;
/// Lines the note may take.
const NOTE_LINES: usize = 4;
/// The button, centred under the words.
const BUTTON: [f32; 2] = [200.0, 46.0];
const BUTTON_GAP: f32 = 36.0;
/// The keys' line, as on the other screens.
const KEYS_Y: f32 = 992.0;
const KEYS_RIGHT: f32 = 1824.0;
/// Type sizes.
const KICKER_SIZE: f32 = 24.0;
const FROM_SIZE: f32 = 17.0;
const NAME_SIZE: f32 = 56.0;
const DETAILS_SIZE: f32 = 20.0;
const ODDS_SIZE: f32 = 16.0;
const NOTE_SIZE: f32 = 19.0;
/// How far a group of words rises as it fades up.
const RISE: f32 = 14.0;
/// The darkness's depth over the scene.
const SCRIM: f32 = 0.8;
/// The ring of ticks round the holocron, against its radius, and its ticks.
const TICK_RING: f32 = 1.3;
const TICKS: usize = 24;
/// The discs of the glow behind the holocron.
const GLOW_DISCS: usize = 8;
/// Seconds the ring of ticks takes to turn once, as the medal's does.
const RING_TURN: f32 = 240.0;
/// The sparks the burst throws: direction (degrees, 0 to the right, clockwise), how far
/// each flies against the holocron's size, when it leaves (seconds) and its size.
const SPARKS: [(f32, f32, f32, f32); 12] = [
    (-90.0, 0.50, 0.50, 0.016),
    (-60.0, 0.38, 0.55, 0.012),
    (-28.0, 0.56, 0.52, 0.015),
    (4.0, 0.42, 0.58, 0.012),
    (34.0, 0.54, 0.53, 0.015),
    (64.0, 0.36, 0.60, 0.012),
    (94.0, 0.46, 0.54, 0.013),
    (124.0, 0.40, 0.57, 0.012),
    (152.0, 0.55, 0.51, 0.015),
    (182.0, 0.38, 0.56, 0.012),
    (214.0, 0.52, 0.53, 0.014),
    (246.0, 0.42, 0.59, 0.012),
];
/// Seconds a spark flies, and how far it falls by the end against the holocron's size.
const SPARK_LIFE: f32 = 1.0;
const SPARK_FALL: f32 = 0.12;

impl HolocronPopup {
    /// Draw the pop-up over the whole frame at `now`, its text to `target`.
    pub(crate) fn append(&mut self, target: TextTarget<'_>, viewport: [f32; 2], now: Instant) {
        // The target's font outlives its vertex lists: measure first, then append.
        let measure = target.body_measure();
        self.build(measure, viewport, now);
        target.append(&self.ui, viewport);
    }

    /// Lay the pop-up out on its canvas, measuring its running text with `measure`.
    pub(super) fn build(&mut self, measure: Measure<'_>, viewport: [f32; 2], now: Instant) {
        let Some((ui, view)) = self.parts(now) else {
            return;
        };
        let frame = Frame::new(viewport);
        ui.begin_transparent(viewport);
        let screen = Rect::new(0.0, 0.0, viewport[0], viewport[1]);
        let _ = ui.draw_list_mut().push(DrawCommand::SolidRect {
            rect: screen,
            color: color::alpha(color::SPACE, SCRIM * view.scrim),
        });
        // A click anywhere acts as the button; the button itself is drawn over it.
        ui.hit_region(SCREEN_TOKEN, screen);
        holocron(ui, &frame, &view);
        let moment = Moment::at(view.t, view.exit);
        words(ui, &frame, measure, &view, &moment);
        ui.finish(NEXT_TOKEN);
    }
}

/// Draw the holocron and its light `view.t` seconds into the ceremony.
fn holocron(ui: &mut MenuCanvas, frame: &Frame, view: &View<'_>) {
    let moment = Moment::at(view.t, view.exit);
    let t = view.t;
    let tier = view.entry.tier;
    let [cx, cy] = frame.point(CENTRE_X, ICON_Y);
    let size = ICON_SIZE * frame.s;
    let radius = size * 0.5;
    let bright = Color::new(
        (tier.colour.r * 0.5 + 0.5).min(1.0),
        (tier.colour.g * 0.5 + 0.5).min(1.0),
        (tier.colour.b * 0.5 + 0.5).min(1.0),
        1.0,
    );
    ui.push_opacity(moment.leave);
    let list = ui.draw_list_mut();
    // The light behind it, in the tier's colour: discs of a little light each.
    let glow = award::glow(t);
    let grow = 0.9 + 0.12 * award::burst(t);
    for step in 0..GLOW_DISCS {
        let factor = 2.4 - 1.3 * step as f32 / (GLOW_DISCS - 1) as f32;
        award::disc(
            list,
            [cx, cy],
            radius * factor * grow,
            award::with_alpha(tier.colour, (0.034 * glow).min(0.13)),
        );
    }
    // The ring of ticks, turning as the main page's ring does.
    let ticks = 0.5 * award::ease_out_cubic(award::span(t, 0.6, 1.3));
    if ticks > 0.0 {
        let turn = TAU * t / RING_TURN;
        let step = TAU / TICKS as f32;
        for tick in 0..TICKS {
            let _ = list.push(DrawCommand::Arc {
                center: [cx, cy],
                radius: radius * TICK_RING,
                width: 2.0 * frame.s,
                start: turn + step * tick as f32,
                sweep: step * 0.3,
                color: award::with_alpha(tier.colour, ticks),
                knockout: None,
            });
        }
    }
    // Two rings of light running out as it lands.
    for (from, to) in [(0.5, 1.4), (0.62, 1.6)] {
        let wave = award::span(t, from, to);
        if wave > 0.0 && wave < 1.0 {
            let _ = list.push(DrawCommand::Arc {
                center: [cx, cy],
                radius: radius * (1.0 + 1.5 * award::ease_out_cubic(wave)),
                width: (6.0 * (1.0 - wave) + 1.0) * frame.s,
                start: 0.0,
                sweep: TAU,
                color: award::with_alpha(tier.colour, 0.85 * (1.0 - wave).powf(1.5)),
                knockout: None,
            });
        }
    }
    // The holocron: its picture, else its gem, coming down into place and growing.
    let lift = moment.drop * size;
    let drawn = size * moment.scale;
    match view.icon {
        Some(texture) => {
            let _ = list.push(DrawCommand::TexturedQuad {
                rect: Rect::new(cx - drawn * 0.5, cy - drawn * 0.5 - lift, drawn, drawn),
                texture,
                color: Color::new(1.0, 1.0, 1.0, moment.medal),
            });
        }
        None => gem::draw(
            list,
            [cx, cy - lift],
            drawn * 0.5,
            tier.colour,
            moment.medal,
            gem::ROWS,
        ),
    }
    // A flash over it as it lands.
    let flash = award::span(t, 0.5, 0.95);
    if flash > 0.0 && flash < 1.0 {
        award::disc(
            list,
            [cx, cy],
            radius * (1.0 + 0.2 * flash),
            award::with_alpha(bright, 0.45 * (1.0 - flash).powi(2)),
        );
    }
    // Sparks thrown out from it, each a bright head and a fading tail, falling a little.
    for (degrees, travel, delay, spark) in SPARKS {
        let life = award::span(t, delay, delay + SPARK_LIFE);
        if life <= 0.0 || life >= 1.0 {
            continue;
        }
        let (sin, cos) = degrees.to_radians().sin_cos();
        let out = radius * 0.95 + travel * size * award::ease_out_cubic(life);
        let fall = SPARK_FALL * size * life * life;
        let fade = (1.0 - life).powf(1.4);
        for (back, shrink, dim) in [(0.0, 1.0, 1.0), (0.05, 0.7, 0.55), (0.09, 0.45, 0.25)] {
            let reach = (out - back * size * (1.0 - life)).max(radius * 0.95);
            let colour = if back == 0.0 {
                Color::new(1.0, 0.97, 0.9, 1.0)
            } else {
                bright
            };
            award::disc(
                list,
                [cx + cos * reach, cy + sin * reach + fall],
                spark * size * shrink * (1.0 - 0.5 * life),
                award::with_alpha(colour, fade * dim),
            );
        }
    }
    ui.pop_opacity();
}

/// One group of words faded to its moment and risen into place: `draw` gets the frame
/// pixels it still has to rise.
fn group(
    ui: &mut MenuCanvas,
    moment: &Moment,
    words: Words,
    draw: impl FnOnce(&mut MenuCanvas, f32),
) {
    let opacity = moment.of(words);
    if opacity <= 0.0 {
        return;
    }
    ui.push_opacity(opacity);
    draw(ui, RISE * award::rise(opacity));
    ui.pop_opacity();
}

/// A centred line of `family` text whose middle is at `y`.
#[allow(clippy::too_many_arguments)]
fn line(
    ui: &mut MenuCanvas,
    frame: &Frame,
    family: TextFamily,
    value: std::fmt::Arguments<'_>,
    y: f32,
    size: f32,
    colour: Color,
    weight: FontWeight,
) {
    text(
        ui,
        family,
        value,
        frame.rect(CENTRE_X - COLUMN * 0.5, y - size * 0.75, COLUMN, size * 1.5),
        size * frame.s,
        colour,
        weight,
        TextAlign::Center,
    );
}

/// Whether `value` fits the column at `size` (frame pixels), with room for the quotes
/// round a note.
fn fits(measure: Measure<'_>, value: &str, size: f32, s: f32) -> bool {
    measure.width(value, size * s, TextFace::Regular) <= (COLUMN - 24.0) * s
}

/// Where the words under the name go: the top of the note, or of the button when there is
/// none, as the layout stands (frame pixels), and how many lines the note takes.
fn below(view: &View<'_>, noted: usize) -> f32 {
    let details = NAME_Y + DETAILS_GAP + if view.when.is_empty() { 0.0 } else { LINE } + LINE;
    if noted > 0 {
        details + NOTE_GAP + noted as f32 * NOTE_LINE - NOTE_LINE * 0.5
    } else {
        details
    }
}

/// The words: what it is and where from, the name, when it was found and the odds, the
/// team's note on a gift, then the button and its key.
fn words(
    ui: &mut MenuCanvas,
    frame: &Frame,
    measure: Measure<'_>,
    view: &View<'_>,
    moment: &Moment,
) {
    let s = frame.s;
    let tier = view.entry.tier;
    let hue = color::alpha(tier.colour, 1.0);
    group(ui, moment, Words::Kicker, |ui, rise| {
        match view.place {
            Some((at, of)) => line(
                ui,
                frame,
                TextFamily::Display,
                format_args!("New holocron \u{b7} {at} of {of}"),
                KICKER_Y + rise,
                KICKER_SIZE,
                hue,
                FontWeight::Semibold,
            ),
            None => line(
                ui,
                frame,
                TextFamily::Display,
                format_args!("New holocron"),
                KICKER_Y + rise,
                KICKER_SIZE,
                hue,
                FontWeight::Semibold,
            ),
        }
        let from = if view.entry.gift {
            "A gift from the SJK team"
        } else {
            "Dropped while you played"
        };
        line(
            ui,
            frame,
            TextFamily::Body,
            format_args!("{from}"),
            FROM_Y + rise,
            FROM_SIZE,
            color::MUTED,
            FontWeight::Regular,
        );
    });
    group(ui, moment, Words::Name, |ui, rise| {
        line(
            ui,
            frame,
            TextFamily::Display,
            format_args!("{}", tier.name),
            NAME_Y + rise,
            NAME_SIZE,
            hue,
            FontWeight::Semibold,
        );
    });
    group(ui, moment, Words::Details, |ui, rise| {
        let mut y = NAME_Y + DETAILS_GAP + rise;
        if !view.when.is_empty() {
            line(
                ui,
                frame,
                TextFamily::Body,
                format_args!("{}", view.when),
                y,
                DETAILS_SIZE,
                color::TEXT,
                FontWeight::Regular,
            );
            y += LINE;
        }
        line(
            ui,
            frame,
            TextFamily::Body,
            format_args!("{} of holocrons are this rare", tier.odds()),
            y,
            ODDS_SIZE,
            color::MUTED,
            FontWeight::Regular,
        );
    });
    let (note, noted) =
        award::wrap::<NOTE_LINES>(&view.entry.note, |part| fits(measure, part, NOTE_SIZE, s));
    if noted > 0 {
        let top =
            NAME_Y + DETAILS_GAP + if view.when.is_empty() { 0.0 } else { LINE } + LINE + NOTE_GAP;
        group(ui, moment, Words::Note, |ui, rise| {
            for (index, part) in note[..noted].iter().enumerate() {
                let open = if index == 0 { "\"" } else { "" };
                let close = if index + 1 == noted { "\"" } else { "" };
                line(
                    ui,
                    frame,
                    TextFamily::Body,
                    format_args!("{open}{part}{close}"),
                    top + index as f32 * NOTE_LINE + rise,
                    NOTE_SIZE,
                    color::alpha(color::TEXT, 0.92),
                    FontWeight::Regular,
                );
            }
        });
    }
    let button_top = below(view, noted) + BUTTON_GAP;
    let opacity = moment.of(Words::Actions);
    // The button's area is there from the start: a click in the entrance finishes it.
    ui.push_opacity(opacity);
    let rise = RISE * award::rise(opacity);
    kit::button(
        ui,
        frame,
        [
            CENTRE_X - BUTTON[0] * 0.5,
            button_top + rise,
            BUTTON[0],
            BUTTON[1],
        ],
        view.action(),
        true,
        true,
        true,
        NEXT_TOKEN,
    );
    let action = if view.more { "next" } else { "close" };
    let caps: &[&str] = &["Enter"];
    let [right, keys_y] = frame.point(KEYS_RIGHT, KEYS_Y);
    key_hint(
        ui,
        caps,
        action,
        right - key_hint_width(caps, action, s),
        keys_y,
        s,
    );
    ui.pop_opacity();
}

/// The button's top for `view`'s words (frame pixels), once they have risen into place.
#[cfg(test)]
pub(super) fn button_top(measure: Measure<'_>, view: &View<'_>, s: f32) -> f32 {
    let (_, noted) =
        award::wrap::<NOTE_LINES>(&view.entry.note, |part| fits(measure, part, NOTE_SIZE, s));
    below(view, noted) + BUTTON_GAP
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::holocron_popup::tests::{FONTS, popup};
    use crate::holocrons::{Entry, TIERS};
    use crate::medal_popup::award::{ENTRANCE, EXIT};

    const VIEWPORTS: [[f32; 2]; 5] = [
        [1920.0, 1080.0],
        [3840.0, 2160.0],
        [1440.0, 1080.0],
        [2560.0, 1080.0],
        [1280.0, 720.0],
    ];
    /// The flight, the landing, the burst, standing still, much later, and leaving.
    const MOMENTS: [(f32, Option<f32>); 8] = [
        (0.0, None),
        (0.3, None),
        (0.55, None),
        (0.8, None),
        (ENTRANCE, None),
        (ENTRANCE + 1.95, None),
        (30.0, None),
        (ENTRANCE + 2.0, Some(EXIT * 0.5)),
    ];

    fn entry(tier: usize, gift: bool, note: &str) -> Entry {
        Entry {
            id: 9,
            tier: &TIERS[tier],
            dropped: 1_791_641_100,
            gift,
            note: note.to_owned(),
        }
    }

    /// Every tier, with and without a note (a gift's longest, in the widest letters),
    /// alone or first of several, with and without its picture, at every moment, fits the
    /// canvas's draw and text budgets at 1080 lines, 4K, 4:3, 21:9 and 720 lines, in the
    /// families and in Inter; the button and its key sit inside the window, the button
    /// above the key.
    #[test]
    fn every_tier_at_every_moment_fits() {
        let now = Instant::now();
        let long = crate::medals::plain_note(&"WWWWWW MMMMMMM ".repeat(14));
        let running = "Thank you for the fog that followed the camera floor, the flickering door on ffa3 and every other bug you found and wrote up so clearly for us all.";
        let notes = [String::new(), running.to_owned(), long];
        FONTS.with(|fonts| {
            for measure in [fonts.families(), fonts.inter()] {
                for tier in 0..TIERS.len() {
                    for note in &notes {
                        for others in [0, 3] {
                            for icon in [None, Some(crate::holocrons::icons::texture(tier))] {
                                for viewport in VIEWPORTS {
                                    for (at, leaving) in MOMENTS {
                                        let mut entries = vec![entry(tier, !note.is_empty(), note)];
                                        entries.extend((0..others).map(|_| entry(0, false, "")));
                                        let mut popup = popup(entries, at, leaving);
                                        popup.set_icons([icon; crate::holocrons::COUNT]);
                                        popup.build(measure, viewport, now);
                                        let ui = &popup.ui;
                                        assert!(!ui.overflowed(), "{tier} {at} {viewport:?}");
                                        let (runs, slots) = ui.text_budget();
                                        assert!(runs < slots, "{runs} text runs");
                                        assert!(
                                            ui.draw_list().len() < 300,
                                            "{}",
                                            ui.draw_list().len()
                                        );
                                        let button = ui.rect_for(NEXT_TOKEN).expect("the button");
                                        let frame = Frame::new(viewport);
                                        let keys = frame.point(KEYS_RIGHT, KEYS_Y)[1];
                                        assert!(
                                            button.x >= 0.0
                                                && button.right() <= viewport[0]
                                                && button.y > 0.0
                                                && button.bottom() < keys - 8.0 * frame.s,
                                            "{tier} {note:?} {viewport:?}: {button:?}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    /// The button is the kit's, centred under the words, its pointer area exactly the drawn
    /// pill.
    #[test]
    fn the_button_sits_under_the_words() {
        let now = Instant::now();
        FONTS.with(|fonts| {
            let measure = fonts.families();
            let mut popup = popup(
                vec![entry(2, true, "Thanks for the bug")],
                ENTRANCE + 0.2,
                None,
            );
            popup.build(measure, [1920.0, 1080.0], now);
            let button = popup.ui.rect_for(NEXT_TOKEN).expect("the button");
            let top = {
                let (_, view) = popup.parts(now).unwrap();
                button_top(measure, &view, 1.0)
            };
            assert_eq!(
                [button.x, button.y, button.width, button.height],
                [960.0 - 100.0, top, 200.0, 46.0]
            );
        });
    }

    /// The holocron, its ring and its light stay in the window and clear of the words, at
    /// every size.
    #[test]
    fn the_holocron_and_its_ring_stay_in_the_window_and_clear_of_the_words() {
        for viewport in VIEWPORTS {
            let frame = Frame::new(viewport);
            let [cx, cy] = frame.point(CENTRE_X, ICON_Y);
            let ring = ICON_SIZE * 0.5 * TICK_RING * frame.s;
            assert!((cx - viewport[0] * 0.5).abs() < 1e-3);
            assert!(
                cy - ring > frame.point(0.0, FROM_Y + FROM_SIZE)[1],
                "{viewport:?}"
            );
            assert!(
                cy + ring < frame.point(0.0, NAME_Y - NAME_SIZE * 0.5)[1],
                "{viewport:?}"
            );
        }
    }
}
