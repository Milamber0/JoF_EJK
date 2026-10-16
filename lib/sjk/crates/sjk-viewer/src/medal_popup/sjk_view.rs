//! The new medal pop-up in the SJK UI (`docs/sjk-ui.md`, "New medal"): no card, the
//! medal floats in the middle of the darkened scene as the screen's one memorable
//! thing, in its ring of ticks, with the words centred under it in the UI's families
//! and its button from the kit (gold, Next or Close), the key bottom right as on the
//! other screens. Laid out on the UI's 16:9 frame ([`Frame`]) in 1080-line pixels.
//!
//! The state, keys and pointer tokens are the pop-up's ([`super`]); the ceremony is
//! [`super::award`]'s.

use super::award::{self, Moment, Palette, Stage, Words};
use super::{MedalPopup, NEXT_TOKEN, SCREEN_TOKEN, View};
use crate::menu::sjk::{Frame, TextTarget, color, key_hint, key_hint_width, kit, text};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use crate::text::UiFont;
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};
use std::time::Instant;

/// The column the words are centred in.
const CENTRE_X: f32 = 960.0;
const COLUMN: f32 = 880.0;
/// "New medal" and who it is from (their middle lines).
const KICKER_Y: f32 = 104.0;
const FROM_Y: f32 = 140.0;
/// The medal's whole picture: its top and its side.
const MEDAL_TOP: f32 = 172.0;
const MEDAL_SIZE: f32 = 340.0;
/// The name's middle line, then the steps between the lines under it.
const NAME_Y: f32 = 600.0;
const DESCRIPTION_GAP: f32 = 54.0;
const LINE: f32 = 30.0;
const NOTE_GAP: f32 = 16.0;
const NOTE_LINE: f32 = 28.0;
/// Lines the description and the note may take.
const DESCRIPTION_LINES: usize = 2;
const NOTE_LINES: usize = 5;
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
const DESCRIPTION_SIZE: f32 = 20.0;
const GIVEN_SIZE: f32 = 16.0;
const NOTE_SIZE: f32 = 19.0;
/// How far a group of words rises as it fades up.
const RISE: f32 = 14.0;
/// The darkness's depth over the scene.
const SCRIM: f32 = 0.8;

/// The SJK UI's colours for the ceremony.
const PALETTE: Palette = Palette {
    gold: color::GOLD,
    bright: color::GOLD_BRIGHT,
    line: color::HOLO,
};

impl MedalPopup {
    /// Draw the SJK UI's pop-up over the whole frame at `now`, its text to `target`.
    pub(crate) fn append_sjk(&mut self, target: TextTarget<'_>, viewport: [f32; 2], now: Instant) {
        // The font outlives the target's vertex lists: measure with it, then append.
        let body = match &target {
            TextTarget::Families(fonts, _) => fonts.body.1,
            TextTarget::Inter(_, font) => *font,
        };
        self.build_sjk(body, viewport, now);
        target.append(&self.ui, viewport);
    }

    /// Lay the pop-up out on its canvas, measuring its running text with the `body`
    /// family's metrics.
    pub(super) fn build_sjk(&mut self, body: &UiFont, viewport: [f32; 2], now: Instant) {
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
        stage(&frame, &view).draw(ui, view.t, view.exit);
        let moment = Moment::at(view.t, view.exit);
        words(ui, &frame, body, &view, &moment);
        ui.finish(NEXT_TOKEN);
    }
}

/// Where the medal stands on `frame`.
fn stage(frame: &Frame, view: &View<'_>) -> Stage {
    Stage {
        rect: frame.rect(
            CENTRE_X - MEDAL_SIZE * 0.5,
            MEDAL_TOP,
            MEDAL_SIZE,
            MEDAL_SIZE,
        ),
        texture: view.award.medal.art(),
        palette: PALETTE,
    }
}

/// One group of words faded to its moment and risen into place: `draw` gets the
/// frame pixels it still has to rise.
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

/// Whether `value` fits the column at `size` (frame pixels) in `font`, with room for
/// the quotes round a note.
fn fits(font: &UiFont, value: &str, size: f32, s: f32) -> bool {
    let scale = size * s / font.height.max(1.0);
    crate::text::visible_text_width(font, value, scale) <= (COLUMN - 24.0) * s
}

/// The words: what it is and who from, the name, what it is for and when it was
/// given, the note, then the button and its key.
fn words(ui: &mut MenuCanvas, frame: &Frame, body: &UiFont, view: &View<'_>, moment: &Moment) {
    let s = frame.s;
    group(ui, moment, Words::Kicker, |ui, rise| {
        match view.place {
            Some((at, of)) => line(
                ui,
                frame,
                TextFamily::Display,
                format_args!("New medal \u{b7} {at} of {of}"),
                KICKER_Y + rise,
                KICKER_SIZE,
                color::GOLD_BRIGHT,
                FontWeight::Semibold,
            ),
            None => line(
                ui,
                frame,
                TextFamily::Display,
                format_args!("New medal"),
                KICKER_Y + rise,
                KICKER_SIZE,
                color::GOLD_BRIGHT,
                FontWeight::Semibold,
            ),
        }
        line(
            ui,
            frame,
            TextFamily::Body,
            format_args!("From the SJK team"),
            FROM_Y + rise,
            FROM_SIZE,
            color::MUTED,
            FontWeight::Regular,
        );
    });
    let shown = view.award;
    group(ui, moment, Words::Name, |ui, rise| {
        let name = shown.medal.name();
        let y = NAME_Y + rise;
        if shown.count > 1 {
            let count = shown.count;
            line(
                ui,
                frame,
                TextFamily::Display,
                format_args!("{name} x{count}"),
                y,
                NAME_SIZE,
                color::GOLD_BRIGHT,
                FontWeight::Semibold,
            );
        } else {
            line(
                ui,
                frame,
                TextFamily::Display,
                format_args!("{name}"),
                y,
                NAME_SIZE,
                color::GOLD_BRIGHT,
                FontWeight::Semibold,
            );
        }
    });
    let (description, described) =
        award::wrap::<DESCRIPTION_LINES>(shown.medal.description(), |part| {
            fits(body, part, DESCRIPTION_SIZE, s)
        });
    let mut y = NAME_Y + DESCRIPTION_GAP;
    let given_y = y + described as f32 * LINE;
    group(ui, moment, Words::Details, |ui, rise| {
        for (index, part) in description[..described].iter().enumerate() {
            line(
                ui,
                frame,
                TextFamily::Body,
                format_args!("{part}"),
                y + index as f32 * LINE + rise,
                DESCRIPTION_SIZE,
                color::TEXT,
                FontWeight::Regular,
            );
        }
        if !view.given.is_empty() {
            line(
                ui,
                frame,
                TextFamily::Body,
                format_args!("{}", view.given),
                given_y + rise,
                GIVEN_SIZE,
                color::MUTED,
                FontWeight::Regular,
            );
        }
    });
    y = given_y + if view.given.is_empty() { 0.0 } else { LINE };
    let (note, noted) =
        award::wrap::<NOTE_LINES>(&shown.note, |part| fits(body, part, NOTE_SIZE, s));
    if noted > 0 {
        let top = y + NOTE_GAP;
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
        y = top + noted as f32 * NOTE_LINE - NOTE_LINE * 0.5;
    }
    let button_top = y + BUTTON_GAP;
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

/// The button's top for `view`'s words (frame pixels), as [`words`] places it once
/// they have risen into place.
#[cfg(test)]
fn button_top(body: &UiFont, view: &View<'_>, s: f32) -> f32 {
    let (_, described) = award::wrap::<DESCRIPTION_LINES>(view.award.medal.description(), |part| {
        fits(body, part, DESCRIPTION_SIZE, s)
    });
    let mut y = NAME_Y + DESCRIPTION_GAP + described as f32 * LINE;
    if !view.given.is_empty() {
        y += LINE;
    }
    let (_, noted) =
        award::wrap::<NOTE_LINES>(&view.award.note, |part| fits(body, part, NOTE_SIZE, s));
    if noted > 0 {
        y += NOTE_GAP + noted as f32 * NOTE_LINE - NOTE_LINE * 0.5;
    }
    y + BUTTON_GAP
}

#[cfg(test)]
mod tests {
    use super::super::award::{ENTRANCE, EXIT};
    use super::super::fixtures::{MOMENTS, VIEWPORTS, award, families, inter, notes};
    use super::*;
    use crate::medals::{Award, Medal};
    use crate::menu::style::MenuStyle;
    use sjk_ui::{InputEvent, PointerButton, Vec2};

    fn popup(awards: Vec<Award>, at: f32, leaving: Option<f32>) -> MedalPopup {
        MedalPopup::preview_in(MenuStyle::Sjk, awards, at, leaving)
    }

    /// Every medal, with no note and the longest ones, first of several or alone, at
    /// every moment, fits the canvas's draw and text budgets at 1080 lines, 4K, 4:3,
    /// 21:9 and 720 lines, in the families and in Inter; its words' lines fit the
    /// column, and the button and its key sit inside the window, the button above the
    /// key.
    #[test]
    fn every_medal_at_every_moment_fits() {
        let now = Instant::now();
        for fonts in [families(), inter()] {
            for note in notes() {
                for medal in Medal::ALL {
                    for others in [0, 3] {
                        let mut awards = vec![award(medal, &note)];
                        awards.extend((0..others).map(|_| award(Medal::EarlyContributor, "")));
                        for viewport in VIEWPORTS {
                            for (at, leaving) in MOMENTS {
                                let mut popup = popup(awards.clone(), at, leaving);
                                popup.build_sjk(&fonts.body.font, viewport, now);
                                let ui = &popup.ui;
                                assert!(!ui.overflowed(), "{medal:?} {at} {viewport:?}");
                                let (runs, slots) = ui.text_budget();
                                assert!(runs < slots, "{runs} text runs");
                                assert!(ui.draw_list().len() < 260, "{}", ui.draw_list().len());
                                let button = ui.rect_for(NEXT_TOKEN).expect("the button");
                                let frame = Frame::new(viewport);
                                let keys = frame.point(KEYS_RIGHT, KEYS_Y)[1];
                                assert!(
                                    button.x >= 0.0
                                        && button.right() <= viewport[0]
                                        && button.y > 0.0
                                        && button.bottom() < keys - 8.0 * frame.s,
                                    "{medal:?} {note:?} {viewport:?}: {button:?}"
                                );
                            }
                        }
                    }
                }
            }
            // Every line drawn fits the column.
            let s = 1.0;
            for note in notes() {
                let (lines, count) = award::wrap::<NOTE_LINES>(&note, |part| {
                    fits(&fonts.body.font, part, NOTE_SIZE, s)
                });
                for part in &lines[..count] {
                    let scale = NOTE_SIZE / fonts.body.font.height;
                    let width = crate::text::visible_text_width(&fonts.body.font, part, scale);
                    // A note at its longest takes every line, the last running on.
                    assert!(width <= COLUMN || count == NOTE_LINES, "{part}");
                }
            }
            for medal in Medal::ALL {
                let scale = NAME_SIZE / fonts.display.font.height;
                let name = crate::text::visible_text_width_face(
                    &fonts.display.font,
                    &medal.label(12),
                    scale,
                    crate::text::TextFace::Semibold,
                );
                assert!(name <= COLUMN, "{medal:?}: {name}");
            }
        }
    }

    /// The note never runs past its lines in the families: the hub's longest note of
    /// running text takes at most four.
    #[test]
    fn the_longest_running_note_takes_four_lines_at_most() {
        let fonts = families();
        let [_, running, _] = notes();
        let (_, count) = award::wrap::<NOTE_LINES>(&running, |part| {
            fits(&fonts.body.font, part, NOTE_SIZE, 1.0)
        });
        assert!((2..=4).contains(&count), "{count}");
    }

    /// The button is the kit's, centred under the words, its pointer area exactly the
    /// drawn pill; a click on it, or anywhere else, takes it once the medal stands
    /// still, and in the entrance finishes the entrance.
    #[test]
    fn the_button_is_the_kits_and_answers_the_pointer() {
        let fonts = families();
        let viewport = [1920.0, 1080.0];
        let mut popup = MedalPopup::default();
        popup.set_style(MenuStyle::Sjk, crate::menu::art::ArtSet::default());
        popup.rehearse(vec![
            award(Medal::EarlyTester, ""),
            award(Medal::BugHunter, ""),
        ]);
        let start = Instant::now();
        popup.open_next(start);
        popup.update(start, true, true);
        let settled = start + std::time::Duration::from_secs_f32(ENTRANCE + 0.2);
        popup.build_sjk(&fonts.body.font, viewport, settled);
        let button = popup.ui.rect_for(NEXT_TOKEN).expect("the button");
        let view_top = {
            let (_, view) = popup.parts(settled).unwrap();
            button_top(&fonts.body.font, &view, 1.0)
        };
        assert_eq!(
            [button.x, button.y, button.width, button.height],
            [960.0 - 100.0, view_top, 200.0, 46.0]
        );
        // The pill is gold: the screen's one main action.
        assert!(
            popup
                .ui
                .draw_list()
                .commands()
                .iter()
                .any(|command| matches!(
                    command,
                    DrawCommand::RoundedRect { rect, color, .. }
                        if *rect == button && color.r > 0.9 && color.b < 0.6
                ))
        );
        let click = |popup: &mut MedalPopup, at: Vec2, now: Instant| {
            let button = PointerButton::Primary;
            popup.pointer(InputEvent::PointerMove(at), now);
            popup.pointer(
                InputEvent::PointerPress {
                    position: at,
                    button,
                },
                now,
            );
            popup.pointer(
                InputEvent::PointerRelease {
                    position: at,
                    button,
                },
                now,
            );
        };
        let middle = Vec2::new(
            button.x + button.width * 0.5,
            button.y + button.height * 0.5,
        );
        click(&mut popup, middle, settled);
        assert!(popup.current.as_ref().unwrap().leaving.is_some(), "Next");
        // The second medal: a click elsewhere in its entrance finishes it.
        let next = settled + std::time::Duration::from_secs_f32(EXIT + 0.01);
        popup.update(next, true, true);
        popup.update(next, true, true);
        popup.build_sjk(&fonts.body.font, viewport, next);
        click(&mut popup, Vec2::new(40.0, 40.0), next);
        let showing = popup.current.as_ref().expect("Bug Hunter");
        assert_eq!(showing.award.medal, Medal::BugHunter);
        assert!(showing.leaving.is_none() && showing.ahead > 0.0);
        // Its button says Close: it is the last.
        popup.build_sjk(&fonts.body.font, viewport, next);
        let (_, view) = popup.parts(next).unwrap();
        assert_eq!(view.action(), "Close");
    }

    /// The medal floats in the frame's middle, its ring and light inside the window,
    /// at every size.
    #[test]
    fn the_medal_and_its_ring_stay_in_the_window() {
        for viewport in VIEWPORTS {
            let frame = Frame::new(viewport);
            let rect = frame.rect(
                CENTRE_X - MEDAL_SIZE * 0.5,
                MEDAL_TOP,
                MEDAL_SIZE,
                MEDAL_SIZE,
            );
            let centre_y = rect.y + rect.height * award::MEDALLION[1];
            let ring = rect.width * award::MEDALLION_RADIUS * award::TICK_RING;
            assert!(centre_y - ring > frame.point(0.0, FROM_Y)[1]);
            assert!(centre_y + ring < frame.point(0.0, NAME_Y - NAME_SIZE * 0.5)[1]);
            assert!((rect.x + rect.width * 0.5 - viewport[0] * 0.5).abs() < 1e-3);
        }
    }
}
