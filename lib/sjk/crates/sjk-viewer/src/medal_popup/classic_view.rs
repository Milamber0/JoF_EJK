//! The new medal pop-up in the classic+ style (`docs/classic-plus.md`), drawn while the
//! menus are classic (`ui_menuStyle classic`). Retail had no such screen, so it takes
//! the frame of the in-game pop-ups as the report dialog does
//! (`text_dialog_classic.rs`): the `menu_box_ingame` box over the dimmed screen, the
//! `menu_blendbox` title band, the medal and its ceremony in retail's gold, its name
//! in gold capitals, a gold NEXT (or CLOSE) button with the `menu_buttonback` glow, and
//! the description line under the box, in the menus' font. It works without the
//! retail art.
//!
//! The state, keys and pointer tokens are the pop-up's ([`super`]); the ceremony is
//! [`super::award`]'s, the same as the SJK UI's.

use super::award::{self, Moment, Palette, Stage, Words};
use super::{MedalPopup, NEXT_TOKEN, SCREEN_TOKEN, View};
use crate::console::browser::classic::{FRAME, LABEL, VALUE, border, fill, text, with_alpha};
use crate::menu::art::{ArtPiece, ArtSet};
use crate::menu::classic::layout::Placement;
use crate::menu::classic::view::{self, Caps, GOLD, HINT};
use crate::menu_widgets::MenuCanvas;
use crate::text::{TextVertex, UiFont};
use sjk_ui::{Color, FontWeight, Rect, TextAlign};
use std::time::Instant;

/// The pop-up box's left and width (canvas units); it is as tall as what it holds and
/// centred, its parts placed from its top.
const BOX_X: f32 = 130.0;
const BOX_WIDTH: f32 = 380.0;
/// The title band, who it is from under it, and the medal's whole picture, from the
/// box's top.
const TITLE: [f32; 4] = [150.0, 6.0, 340.0, 22.0];
const FROM: [f32; 4] = [150.0, 32.0, 340.0, 12.0];
const MEDAL: [f32; 4] = [236.0, 48.0, 168.0, 168.0];
/// The words' column: its left, its width, the name's box and the lines under it,
/// from the box's top.
const COLUMN_X: f32 = 150.0;
const COLUMN: f32 = 340.0;
const NAME: [f32; 4] = [150.0, 246.0, 340.0, 20.0];
const DESCRIPTION_Y: f32 = 270.0;
const LINE: f32 = 14.0;
const NOTE_GAP: f32 = 6.0;
const NOTE_LINE: f32 = 13.0;
const DESCRIPTION_LINES: usize = 2;
const NOTE_LINES: usize = 5;
/// The button: its size, its gap under the words and the box's margin under it.
const BUTTON: [f32; 2] = [120.0, 20.0];
const BUTTON_GAP: f32 = 14.0;
const FOOT: f32 = 16.0;
/// The description line under the box: its gap and its height.
const HINT_GAP: f32 = 6.0;
const HINT_HEIGHT: f32 = 14.0;
/// How far a group of words rises as it fades up (canvas units).
const RISE: f32 = 6.0;
/// The menus' type sizes.
const NAME_SIZE: f32 = 17.0;
const TEXT_SIZE: f32 = 11.0;
const SMALL_SIZE: f32 = 10.0;

/// Retail's gold for the ceremony, and its blue for the ring of ticks.
const PALETTE: Palette = Palette {
    gold: GOLD,
    bright: Color::new(1.0, 0.86, 0.45, 1.0),
    line: LABEL,
};

// The parts follow each other down the box; the tallest box (two lines of what it is
// for, a date, a note of five lines) and its description line fit the canvas.
const _: () = assert!(TITLE[1] + TITLE[3] <= FROM[1] && FROM[1] + FROM[3] <= MEDAL[1]);
const _: () = assert!(MEDAL[1] + MEDAL[3] < NAME[1] && NAME[1] + NAME[3] <= DESCRIPTION_Y);
const TALLEST: f32 = DESCRIPTION_Y
    + DESCRIPTION_LINES as f32 * LINE
    + LINE
    + NOTE_GAP
    + NOTE_LINES as f32 * NOTE_LINE
    + BUTTON_GAP
    + BUTTON[1]
    + FOOT;
const _: () = assert!(TALLEST + HINT_GAP + HINT_HEIGHT <= 470.0);

/// Where the box stands for the words it holds.
struct Layout {
    /// The box's top and height (canvas units).
    top: f32,
    height: f32,
    /// Lines of what it is for and of the note.
    described: usize,
    noted: usize,
}

impl Layout {
    /// The box for `view`'s words, measured in `font` at `scale`.
    fn new(view: &View<'_>, font: &UiFont, scale: f32) -> Self {
        let (_, described) =
            award::wrap::<DESCRIPTION_LINES>(view.award.medal.description(), |part| {
                fits(font, part, TEXT_SIZE, scale)
            });
        let (_, noted) =
            award::wrap::<NOTE_LINES>(&view.award.note, |part| fits(font, part, TEXT_SIZE, scale));
        let mut height = DESCRIPTION_Y + described as f32 * LINE;
        if !view.given.is_empty() {
            height += LINE;
        }
        if noted > 0 {
            height += NOTE_GAP + noted as f32 * NOTE_LINE;
        }
        height += BUTTON_GAP + BUTTON[1] + FOOT;
        Self {
            top: ((480.0 - height - HINT_GAP - HINT_HEIGHT) * 0.5).max(4.0),
            height,
            described,
            noted,
        }
    }

    /// A rectangle `[x, y, width, height]` whose `y` is from the box's top.
    fn at(&self, [x, y, width, height]: [f32; 4]) -> [f32; 4] {
        [x, self.top + y, width, height]
    }
}

impl MedalPopup {
    /// Draw the classic+ pop-up over the whole frame at `now`, its text in the menus'
    /// `font`.
    pub(crate) fn append_classic(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        now: Instant,
    ) {
        self.build_classic(font, viewport, now);
        self.ui.append_text(vertices, font, viewport);
    }

    /// Lay the pop-up out on its canvas, measuring with the menus' `font`.
    pub(super) fn build_classic(&mut self, font: &UiFont, viewport: [f32; 2], now: Instant) {
        let art = self.art;
        let Some((ui, view)) = self.parts(now) else {
            return;
        };
        let place = Placement::new(viewport);
        let layout = Layout::new(&view, font, place.scale);
        ui.begin_transparent(viewport);
        let screen = Rect::new(0.0, 0.0, viewport[0], viewport[1]);
        fill(ui, screen, view::ink(0.82 * view.scrim));
        ui.hit_region(SCREEN_TOKEN, screen);
        // The box and its title band.
        let rect = place.rect([BOX_X, layout.top, BOX_WIDTH, layout.height]);
        if art.has(ArtPiece::PopupBox) {
            view::art(ui, ArtPiece::PopupBox, rect);
        } else {
            fill(ui, rect, view::ink(0.9));
            border(ui, rect, place.scale, FRAME);
        }
        let band = place.rect(layout.at(TITLE));
        if art.has(ArtPiece::BlendBox) {
            view::art(ui, ArtPiece::BlendBox, band);
        } else {
            view::soft_band(ui, band, 0.16);
        }
        Stage {
            rect: place.rect(layout.at(MEDAL)),
            texture: view.award.medal.art(),
            palette: PALETTE,
        }
        .draw(ui, view.t, view.exit);
        let moment = Moment::at(view.t, view.exit);
        words(ui, &place, &layout, art, font, &view, &moment);
        ui.finish(NEXT_TOKEN);
    }
}

/// Whether `value` fits the column at canvas size `size` in the menus' `font`, with
/// its tracking and room for a note's quotes.
fn fits(font: &UiFont, value: &str, size: f32, scale: f32) -> bool {
    let measure = size * scale / font.height.max(1.0);
    let tracking = 0.3 * scale * value.chars().count() as f32;
    crate::text::visible_text_width(font, value, measure) + tracking <= (COLUMN - 16.0) * scale
}

/// One group of words faded to its moment: `draw` gets the canvas units it still has
/// to rise.
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

/// The words: the title band's, who from, the name, what it is for and when, the
/// note, then the button and the description line.
fn words(
    ui: &mut MenuCanvas,
    place: &Placement,
    layout: &Layout,
    art: ArtSet,
    font: &UiFont,
    view: &View<'_>,
    moment: &Moment,
) {
    let scale = place.scale;
    let row = |y: f32, height: f32| [COLUMN_X, layout.top + y, COLUMN, height];
    group(ui, moment, Words::Kicker, |ui, rise| {
        let [x, y, width, height] = layout.at(TITLE);
        let title = [x, y + 3.0, width, height - 6.0];
        match view.place {
            Some((at, of)) => text(
                ui,
                place,
                format_args!("NEW MEDAL  {at} OF {of}"),
                title,
                12.5,
                LABEL,
                FontWeight::Semibold,
                TextAlign::Center,
            ),
            None => text(
                ui,
                place,
                format_args!("NEW MEDAL"),
                title,
                12.5,
                LABEL,
                FontWeight::Semibold,
                TextAlign::Center,
            ),
        }
        let [x, y, width, height] = layout.at(FROM);
        text(
            ui,
            place,
            format_args!("From the SJK team"),
            [x, y + rise, width, height],
            SMALL_SIZE,
            with_alpha(VALUE, 0.85),
            FontWeight::Regular,
            TextAlign::Center,
        );
    });
    let shown = view.award;
    group(ui, moment, Words::Name, |ui, rise| {
        let [x, y, width, height] = layout.at(NAME);
        let name = Caps(shown.medal.name());
        if shown.count > 1 {
            let count = shown.count;
            text(
                ui,
                place,
                format_args!("{name} X{count}"),
                [x, y + rise, width, height],
                NAME_SIZE,
                GOLD,
                FontWeight::Semibold,
                TextAlign::Center,
            );
        } else {
            text(
                ui,
                place,
                format_args!("{name}"),
                [x, y + rise, width, height],
                NAME_SIZE,
                GOLD,
                FontWeight::Semibold,
                TextAlign::Center,
            );
        }
    });
    let (description, described) =
        award::wrap::<DESCRIPTION_LINES>(shown.medal.description(), |part| {
            fits(font, part, TEXT_SIZE, scale)
        });
    debug_assert_eq!(described, layout.described);
    let given_y = DESCRIPTION_Y + described as f32 * LINE;
    group(ui, moment, Words::Details, |ui, rise| {
        for (index, part) in description[..described].iter().enumerate() {
            text(
                ui,
                place,
                format_args!("{part}"),
                row(DESCRIPTION_Y + index as f32 * LINE + rise, LINE),
                TEXT_SIZE,
                LABEL,
                FontWeight::Regular,
                TextAlign::Center,
            );
        }
        if !view.given.is_empty() {
            text(
                ui,
                place,
                format_args!("{}", view.given),
                row(given_y + rise, LINE),
                SMALL_SIZE,
                with_alpha(VALUE, 0.85),
                FontWeight::Regular,
                TextAlign::Center,
            );
        }
    });
    let mut y = given_y + if view.given.is_empty() { 0.0 } else { LINE };
    let (note, noted) =
        award::wrap::<NOTE_LINES>(&shown.note, |part| fits(font, part, TEXT_SIZE, scale));
    debug_assert_eq!(noted, layout.noted);
    if noted > 0 {
        let top = y + NOTE_GAP;
        group(ui, moment, Words::Note, |ui, rise| {
            for (index, part) in note[..noted].iter().enumerate() {
                let open = if index == 0 { "\"" } else { "" };
                let close = if index + 1 == noted { "\"" } else { "" };
                text(
                    ui,
                    place,
                    format_args!("{open}{part}{close}"),
                    row(top + index as f32 * NOTE_LINE + rise, NOTE_LINE),
                    TEXT_SIZE,
                    VALUE,
                    FontWeight::Regular,
                    TextAlign::Center,
                );
            }
        });
        y = top + noted as f32 * NOTE_LINE;
    }
    // The button's area is there from the start: a click in the entrance finishes it.
    let opacity = moment.of(Words::Actions);
    ui.push_opacity(opacity);
    let rise = RISE * award::rise(opacity);
    let button = [
        320.0 - BUTTON[0] * 0.5,
        layout.top + y + BUTTON_GAP + rise,
        BUTTON[0],
        BUTTON[1],
    ];
    let area = place.rect(button);
    // The only control, so always focused: its glow and retail's pulsing white.
    view::glow(ui, area, scale, art);
    text(
        ui,
        place,
        format_args!("{}", Caps(view.action())),
        button,
        14.0,
        view::focus_pulse(),
        FontWeight::Semibold,
        TextAlign::Center,
    );
    ui.hit_region(NEXT_TOKEN, area);
    let hint = if view.more {
        "Enter or a click shows the next medal."
    } else {
        "Enter or a click closes the medal."
    };
    text(
        ui,
        place,
        format_args!("{hint}"),
        [
            0.0,
            layout.top + layout.height + HINT_GAP,
            640.0,
            HINT_HEIGHT,
        ],
        12.0,
        HINT,
        FontWeight::Regular,
        TextAlign::Center,
    );
    ui.pop_opacity();
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{MOMENTS, VIEWPORTS, award, families, inter, notes};
    use super::*;
    use crate::medals::Medal;
    use crate::menu::style::MenuStyle;

    fn inside(inner: [f32; 4], outer: [f32; 4]) -> bool {
        inner[0] >= outer[0]
            && inner[1] >= outer[1]
            && inner[0] + inner[2] <= outer[0] + outer[2]
            && inner[1] + inner[3] <= outer[1] + outer[3]
    }

    #[test]
    fn every_fixed_part_lies_inside_the_box_and_the_canvas() {
        let shortest = [
            BOX_X,
            0.0,
            BOX_WIDTH,
            DESCRIPTION_Y + BUTTON_GAP + BUTTON[1] + FOOT,
        ];
        for part in [TITLE, FROM, MEDAL, NAME] {
            assert!(inside(part, shortest), "{part:?}");
        }
        // The medal's ring stays clear of the title band and the name.
        let centre = MEDAL[1] + MEDAL[3] * award::MEDALLION[1];
        let ring = MEDAL[2] * award::MEDALLION_RADIUS * award::TICK_RING;
        assert!(centre - ring > TITLE[1] + TITLE[3] && centre + ring < NAME[1]);
    }

    /// Every medal, with no note and the longest ones, alone or first of several, at
    /// every moment, fits the canvas in the menus' fonts at every window size, and its
    /// button sits inside the box.
    #[test]
    fn every_medal_at_every_moment_fits_the_box() {
        let now = Instant::now();
        for fonts in [inter(), families()] {
            let font = &fonts.body.font;
            for note in notes() {
                for medal in Medal::ALL {
                    for others in [0, 2] {
                        let mut awards = vec![award(medal, &note)];
                        awards.extend((0..others).map(|_| award(Medal::EarlyTester, "")));
                        for viewport in VIEWPORTS {
                            for (at, leaving) in MOMENTS {
                                let mut popup = MedalPopup::preview_in(
                                    MenuStyle::Classic,
                                    awards.clone(),
                                    at,
                                    leaving,
                                );
                                popup.build_classic(font, viewport, now);
                                assert!(!popup.ui.overflowed(), "{medal:?} {at} {viewport:?}");
                                let place = Placement::new(viewport);
                                let button = popup.ui.rect_for(NEXT_TOKEN).expect("the button");
                                let (_, view) = popup.parts(now).expect("on show");
                                let layout = Layout::new(&view, font, place.scale);
                                let box_ = [BOX_X, layout.top, BOX_WIDTH, layout.height];
                                assert!(inside(box_, [0.0, 0.0, 640.0, 480.0 - 20.0]));
                                let frame = place.rect(box_);
                                assert!(
                                    button.x >= frame.x
                                        && button.right() <= frame.right()
                                        && button.y >= frame.y
                                        && button.bottom() <= frame.bottom() - 4.0 * place.scale,
                                    "{medal:?} {note:?} {viewport:?}: {button:?} in {frame:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// The name in capitals fits the column in the menus' fonts.
    #[test]
    fn every_name_fits_its_line() {
        for fonts in [inter(), families()] {
            let font = &fonts.display.font;
            for medal in Medal::ALL {
                let name = format!("{} X12", Caps(medal.name()));
                let scale = NAME_SIZE / font.height;
                let width = crate::text::visible_text_width(font, &name, scale);
                assert!(width + 0.3 * name.len() as f32 <= COLUMN, "{name}: {width}");
            }
        }
    }
}
