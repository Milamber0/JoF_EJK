//! The Collection's Nameplates tab: ornaments for the nameplate every player sees over
//! the player's head (`docs/client.md`, Nameplates). None exist yet, so the tab shows
//! where they will go: three empty places on the left (a crest before the name, a
//! frame round the plate, a trail under it) and, over the player's model beside the
//! page, their nameplate as a match draws it near, with dashed marks at those places.

use super::view::{LEFT_X, MODEL_AREA, STAGE_TEXT_WIDTH, STAGE_TEXT_X, left_lines};
use super::*;
use crate::menu::sjk::{Frame, color, text};
use crate::menu_widgets::TextFamily;
use sjk_ui::{Color, DrawCommand, FontWeight, TextAlign};
use std::f32::consts::{PI, TAU};

/// The empty places: their top, size and the gap between them.
const PLACES_TOP: f32 = 300.0;
const PLACE: f32 = 240.0;
const PLACE_GAP: f32 = 30.0;
/// The nameplate: its plate's width and the bars' colours (the Radial HUD's shield,
/// health and Force, as the nameplates take them).
const PLATE_WIDTH: f32 = 176.0;
const SHIELD: Color = Color::new(0.25, 0.82, 0.5, 1.0);
const HEALTH: Color = Color::new(0.9, 0.28, 0.3, 1.0);
const FORCE: Color = Color::new(0.29, 0.66, 1.0, 1.0);

/// `name` without its colour codes (`^1`).
fn plain(name: &str) -> String {
    let mut plain = String::with_capacity(name.len());
    let mut characters = name.chars();
    while let Some(character) = characters.next() {
        if character == '^' {
            let _ = characters.next();
        } else {
            plain.push(character);
        }
    }
    plain
}

/// A dashed outline round `rect` (frame pixels).
fn dashed_rect(
    canvas: &mut crate::menu_widgets::MenuCanvas,
    frame: &Frame,
    [x, y, width, height]: [f32; 4],
    colour: Color,
) {
    const DASH: f32 = 9.0;
    const STEP: f32 = 16.0;
    let mut along = 0.0;
    while along < width {
        let length = DASH.min(width - along);
        for top in [y, y + height - 1.2] {
            let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
                rect: frame.rect(x + along, top, length, 1.2),
                color: colour,
            });
        }
        along += STEP;
    }
    let mut down = 0.0;
    while down < height {
        let length = DASH.min(height - down);
        for left in [x, x + width - 1.2] {
            let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
                rect: frame.rect(left, y + down, 1.2, length),
                color: colour,
            });
        }
        down += STEP;
    }
}

/// A dashed arc round `centre` (frame pixels) from `start` over `sweep` radians.
fn dashed_arc(
    canvas: &mut crate::menu_widgets::MenuCanvas,
    frame: &Frame,
    centre: [f32; 2],
    radius: f32,
    (start, sweep): (f32, f32),
    colour: Color,
) {
    let dashes = ((radius * sweep.abs()) / 14.0).ceil().max(2.0) as usize;
    let each = sweep / dashes as f32;
    for dash in 0..dashes {
        let _ = canvas.draw_list_mut().push(DrawCommand::Arc {
            center: frame.point(centre[0], centre[1]),
            radius: radius * frame.s,
            width: 1.5 * frame.s,
            start: start + each * dash as f32,
            sweep: each * 0.55,
            color: colour,
            knockout: None,
        });
    }
}

impl Panel {
    /// The Nameplates tab.
    pub(super) fn nameplates(&mut self, frame: &Frame, inputs: &Inputs<'_>) {
        self.lead(
            frame,
            "No nameplates yet",
            "Ornaments for your nameplate, the name every player sees over your head.",
        );
        let places = [
            ("Crest", "Before your name"),
            ("Frame", "Round your plate"),
            ("Trail", "Under your plate"),
        ];
        for (index, (name, place)) in places.into_iter().enumerate() {
            let x = LEFT_X + index as f32 * (PLACE + PLACE_GAP);
            self.empty_place(frame, index, x, name, place);
        }
        super::toys::room_for_more(
            &mut self.ui,
            frame,
            716.0,
            "Nothing to collect here yet: the first ones come later.",
        );
        let at = self.plate_spot(frame);
        self.nameplate(frame, inputs, at);
        self.nameplate_words(frame);
    }

    /// Empty place `index` from `x`: a dashed niche with a drawing of where its kind
    /// goes, its name and place under it.
    fn empty_place(&mut self, frame: &Frame, index: usize, x: f32, name: &str, place: &str) {
        let s = frame.s;
        let line = color::alpha(color::HOLO, 0.55);
        let faint = color::alpha(color::HOLO, 0.28);
        dashed_rect(&mut self.ui, frame, [x, PLACES_TOP, PLACE, PLACE], faint);
        let middle = [x + PLACE * 0.5, PLACES_TOP + PLACE * 0.5];
        let bar = |canvas: &mut crate::menu_widgets::MenuCanvas, rect: [f32; 4]| {
            let [bx, by, bw, bh] = rect;
            let _ = canvas.draw_list_mut().push(DrawCommand::Border {
                rect: frame.rect(bx, by, bw, bh),
                radius: 3.0 * s,
                width: 1.5 * s,
                color: faint,
            });
        };
        match index {
            // A crest: a ring with a mark inside, before a name's bar.
            0 => {
                let _ = self.ui.draw_list_mut().push(DrawCommand::Arc {
                    center: frame.point(middle[0] - 52.0, middle[1]),
                    radius: 22.0 * s,
                    width: 2.0 * s,
                    start: 0.0,
                    sweep: TAU,
                    color: line,
                    knockout: None,
                });
                let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: frame.rect(middle[0] - 57.0, middle[1] - 5.0, 10.0, 10.0),
                    radius: 2.0 * s,
                    color: line,
                });
                bar(
                    &mut self.ui,
                    [middle[0] - 18.0, middle[1] - 8.0, 92.0, 16.0],
                );
            }
            // A frame round the plate's three bars, with corner marks.
            1 => {
                let _ = self.ui.draw_list_mut().push(DrawCommand::Border {
                    rect: frame.rect(middle[0] - 64.0, middle[1] - 30.0, 128.0, 60.0),
                    radius: 8.0 * s,
                    width: 2.0 * s,
                    color: line,
                });
                for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                    let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                        rect: frame.rect(
                            middle[0] + dx * 72.0 - 3.0,
                            middle[1] + dy * 38.0 - 3.0,
                            6.0,
                            6.0,
                        ),
                        radius: 3.0 * s,
                        color: line,
                    });
                }
                for row in 0..3 {
                    let width = if row == 2 { 72.0 } else { 96.0 };
                    bar(
                        &mut self.ui,
                        [
                            middle[0] - 48.0,
                            middle[1] - 18.0 + row as f32 * 13.0,
                            width,
                            7.0,
                        ],
                    );
                }
            }
            // A trail sweeping under the plate.
            _ => {
                for row in 0..2 {
                    bar(
                        &mut self.ui,
                        [
                            middle[0] - 50.0,
                            middle[1] - 34.0 + row as f32 * 12.0,
                            100.0,
                            7.0,
                        ],
                    );
                }
                for (radius, alpha) in [(64.0, 0.55), (48.0, 0.4)] {
                    let _ = self.ui.draw_list_mut().push(DrawCommand::Arc {
                        center: frame.point(middle[0], middle[1] - 40.0),
                        radius: radius * s,
                        width: 2.0 * s,
                        start: PI * 0.25,
                        sweep: PI * 0.5,
                        color: color::alpha(color::HOLO, alpha),
                        knockout: None,
                    });
                }
            }
        }
        let _ = self.ui.draw_list_mut().push(DrawCommand::SolidRect {
            rect: frame.rect(x, PLACES_TOP + PLACE + 16.0, PLACE, 2.0),
            color: color::alpha(color::HOLO, 0.22),
        });
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{name}"),
            frame.rect(x, PLACES_TOP + PLACE + 30.0, PLACE, 30.0),
            22.0 * s,
            color::MUTED,
            FontWeight::Semibold,
            TextAlign::Center,
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{place}"),
            frame.rect(x, PLACES_TOP + PLACE + 62.0, PLACE, 22.0),
            15.0 * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::Center,
        );
    }

    /// Where the nameplate stands, in frame pixels: over the model's head on the
    /// stage (the renderer says where), over the preview's, or high in the column.
    fn plate_spot(&self, frame: &Frame) -> [f32; 2] {
        match self.backstage {
            Backstage::World { head: Some(head) } => [
                (head[0] - frame.origin[0]) / frame.s,
                (head[1] - frame.origin[1]) / frame.s,
            ],
            Backstage::World { head: None } | Backstage::Preview { .. } => {
                let [x, y, width, _] = MODEL_AREA;
                [x + width * 0.5, y + 70.0]
            }
            Backstage::None => [STAGE_TEXT_X + STAGE_TEXT_WIDTH * 0.5, 380.0],
        }
    }

    /// The player's nameplate as a match draws it near, its bottom at `at`, with the
    /// dashed marks where ornaments will go.
    fn nameplate(&mut self, frame: &Frame, inputs: &Inputs<'_>, at: [f32; 2]) {
        let s = frame.s;
        let [x, bottom] = at;
        let mark = color::alpha(color::HOLO, 0.8);
        // Under the plate, the trail's place.
        dashed_arc(
            &mut self.ui,
            frame,
            [x, bottom - 52.0],
            60.0,
            (PI * 0.28, PI * 0.44),
            mark,
        );
        let plate_top = bottom - 46.0;
        let plate = [x - PLATE_WIDTH * 0.5, plate_top, PLATE_WIDTH, 30.0];
        let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: frame.rect(plate[0], plate[1], plate[2], plate[3]),
            radius: 4.0 * s,
            color: color::alpha(color::SPACE, 0.78),
        });
        for (row, (colour, share)) in [(SHIELD, 1.0), (HEALTH, 1.0), (FORCE, 0.78)]
            .into_iter()
            .enumerate()
        {
            let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: frame.rect(
                    plate[0] + 6.0,
                    plate[1] + 5.0 + row as f32 * 7.0,
                    (PLATE_WIDTH - 12.0) * share,
                    5.0,
                ),
                radius: 2.0 * s,
                color: colour,
            });
        }
        // Round the plate, the frame's place.
        dashed_rect(
            &mut self.ui,
            frame,
            [
                plate[0] - 9.0,
                plate[1] - 9.0,
                plate[2] + 18.0,
                plate[3] + 18.0,
            ],
            mark,
        );
        let name = crate::profile_hub::title(inputs.name);
        let name_top = plate_top - 44.0;
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{name}"),
            frame.rect(x - 200.0, name_top, 400.0, 34.0),
            26.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Center,
        );
        // Before the name, the crest's place.
        let half = crate::text::display_width(&plain(name), 27.0) * 0.5;
        dashed_arc(
            &mut self.ui,
            frame,
            [x - half - 26.0, name_top + 17.0],
            14.0,
            (0.0, TAU),
            mark,
        );
    }

    /// What the nameplate is, beside the model.
    fn nameplate_words(&mut self, frame: &Frame) {
        let s = frame.s;
        let x = STAGE_TEXT_X;
        let width = STAGE_TEXT_WIDTH;
        let mut y = 640.0;
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Nameplate"),
            frame.rect(x, y, width, 22.0),
            16.0 * s,
            color::HOLO,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        y += 26.0;
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("Your nameplate"),
            frame.rect(x, y, width, 62.0),
            52.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        y += 70.0;
        y = left_lines(
            &mut self.ui,
            frame,
            "What every SJK player sees over your head in a match: your name, then your shield, health and Force.",
            [x, y, width],
            (52, 3),
            18.0,
            color::TEXT,
            TextAlign::Start,
        );
        y += 4.0;
        left_lines(
            &mut self.ui,
            frame,
            "Ornaments will go before your name, round your plate and under it: the dashed marks.",
            [x, y, width],
            (60, 2),
            16.0,
            color::MUTED,
            TextAlign::Start,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The empty places, the nameplate and its words fit over the keys with the model
    /// on the stage, in a preview and without it, at 1080 lines, 4K, 4:3 and 21:9.
    #[test]
    fn the_places_and_the_plate_fit_over_the_keys() {
        let font = crate::text::load_modern(1.0, None).expect("Inter").font;
        for viewport in [
            [1_920.0, 1_080.0],
            [3_840.0, 2_160.0],
            [1_440.0, 1_080.0],
            [2_560.0, 1_080.0],
        ] {
            let head = Frame::new(viewport).point(1_150.0, 420.0);
            for backstage in [
                Backstage::None,
                Backstage::World { head: Some(head) },
                Backstage::Preview { ready: false },
            ] {
                let inputs = super::super::tests::inputs(unlockables::Holdings::Known(&[]), "");
                let mut panel = Panel::new();
                panel.open(Tab::Nameplates, true, true, ReturnTarget::MainMenu);
                panel.set_backstage(backstage);
                panel.build(&inputs, &font, viewport);
                assert!(!panel.ui.overflowed(), "{viewport:?}");
                let frame = Frame::new(viewport);
                let keys = frame.point(0.0, super::super::view::KEYS_Y)[1];
                for command in panel.ui.draw_list().commands() {
                    if let DrawCommand::Text { rect, .. } = command {
                        assert!(rect.bottom() <= keys || rect.y >= keys, "{rect:?}");
                        assert!(rect.right() <= viewport[0] + 1.0, "{rect:?}");
                    }
                }
            }
        }
    }
}
