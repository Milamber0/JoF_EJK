//! The Holocrons tab's loot table (`docs/holocrons.md`, "Loot table"): the odds of
//! everything a holocron brings, over the darkened page, from the tables the client keeps
//! of the hub's ([`crate::cards::loot`]).
//!
//! - How often holocrons drop (the hub's pace and daily limit, once it said).
//! - Per holocron: each tier's chance of dropping, its card's rarity and finish odds and
//!   its credits, then the same for any holocron found.
//! - Every kind of card (each rarity in each finish) per holocron found, as a share and as
//!   "1 in N", with notes on how the rolls are made and the rarest card.
//!
//! Loot table or L shows it; Escape, L, Enter or Back go back to the page.

use super::*;
use crate::audio::ui_cues::{self, Cue};
use crate::cards::{FINISHES, Finish, RARITIES, loot};
use crate::menu::sjk::{Frame, color, key_hint, key_hint_width, kit, text, wrap};
use crate::menu_widgets::TextFamily;
use sjk_ui::{Color, DrawCommand, FontWeight, TextAlign};

/// The page's edges.
const LEFT: f32 = 96.0;
const RIGHT: f32 = 1_824.0;
const WIDTH: f32 = RIGHT - LEFT;
/// The title and the two lines under it.
const TITLE_Y: f32 = 60.0;
const LINE_Y: f32 = 122.0;
const PACE_Y: f32 = 148.0;
/// The per-holocron table: its heading, the groups' labels and rule, the columns' names,
/// the tiers' rows and the row for any holocron.
const TABLE_Y: f32 = 206.0;
const GROUP_Y: f32 = 226.0;
const COLUMNS_Y: f32 = 262.0;
const ROWS_TOP: f32 = 296.0;
const ROW_HEIGHT: f32 = 52.0;
const ROW_GAP: f32 = 4.0;
const ANY_GAP: f32 = 10.0;
/// The per-holocron table's columns: the tier, its drop chance, the four rarities, the
/// three finishes and the credits.
const TIER_WIDTH: f32 = 330.0;
const DROP_X: f32 = LEFT + TIER_WIDTH;
const DROP_WIDTH: f32 = 130.0;
const CELL_WIDTH: f32 = 135.0;
const RARITY_X: f32 = DROP_X + DROP_WIDTH;
const FINISH_X: f32 = RARITY_X + CELL_WIDTH * 4.0 + 24.0;
const CREDITS_X: f32 = FINISH_X + CELL_WIDTH * 3.0 + 24.0;
const CREDITS_WIDTH: f32 = RIGHT - CREDITS_X;
/// The kinds' grid: its heading, the finishes' names and the rarities' rows.
const GRID_Y: f32 = 632.0;
const GRID_COLUMNS_Y: f32 = 652.0;
const GRID_TOP: f32 = 686.0;
const GRID_HEIGHT: f32 = 66.0;
const GRID_GAP: f32 = 6.0;
const GRID_CELL: f32 = 300.0;
/// The notes, right of the grid.
const NOTES_X: f32 = 1_390.0;
const NOTES_CHARS: usize = 68;
/// Back, and the keys' line.
const BACK_Y: f32 = 988.0;
const BACK_WIDTH: f32 = 150.0;

/// The notes on how the rolls are made, and the rarest card.
fn notes() -> [String; 5] {
    let (rarity, finish, chance) = loot::rarest();
    [
        "A card's rarity and its finish are rolled apart, so a kind of card is the two chances multiplied, weighed by how often each holocron drops."
            .to_owned(),
        "Credits are rolled evenly between the least and the most. A holocron that brings a saber shader brings no card and no credits; the averages and chances count it.".to_owned(),
        format!(
            "The rarest card is a {} {}: about {} holocrons.",
            RARITIES[rarity].name,
            FINISHES[finish].name,
            loot::one_in(chance)
        ),
        "The card is picked evenly among the cards of its rarity; a rarity with no card yet stays sealed.".to_owned(),
        "Gifts from the SJK team are given, not found; they open the same way.".to_owned(),
    ]
}

/// The colour a finish's name reads in.
fn finish_colour(finish: &Finish) -> Color {
    match finish.index {
        0 => color::TEXT,
        1 => color::HOLO,
        _ => color::GOLD_BRIGHT,
    }
}

/// `words` in the middle of `rect` (frame pixels) at `size`, in `colour`.
#[allow(clippy::too_many_arguments)]
fn centred(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    words: &str,
    [x, y, width, height]: [f32; 4],
    size: f32,
    colour: Color,
    family: TextFamily,
    weight: FontWeight,
) {
    text(
        canvas,
        family,
        format_args!("{words}"),
        frame.rect(x, y, width, height),
        size * frame.s,
        colour,
        weight,
        TextAlign::Center,
    );
}

/// A chance in a cell: its share, quiet when none.
fn chance_cell(canvas: &mut MenuCanvas, frame: &Frame, chance: f64, rect: [f32; 4]) {
    let colour = if chance > 0.0 {
        color::TEXT
    } else {
        color::alpha(color::QUIET, 0.6)
    };
    centred(
        canvas,
        frame,
        &loot::percent(chance),
        rect,
        21.0,
        colour,
        TextFamily::Display,
        FontWeight::Semibold,
    );
}

impl Panel {
    /// Whether the loot table shows.
    #[cfg(test)]
    pub(crate) fn showing_loot(&self) -> bool {
        self.loot
    }

    /// Show the loot table (`true`) or go back to the page.
    pub(crate) fn show_loot(&mut self, show: bool) {
        if self.loot != show {
            ui_cues::post(if show { Cue::Click } else { Cue::Back });
        }
        self.loot = show;
    }

    /// A key while the loot table shows.
    pub(super) fn loot_key(&mut self, key: KeyCode) -> PanelAction {
        if matches!(
            key,
            KeyCode::Escape | KeyCode::KeyL | KeyCode::Enter | KeyCode::NumpadEnter
        ) {
            self.show_loot(false);
        }
        PanelAction::None
    }

    /// A pointer event while the loot table shows: only Back answers.
    pub(super) fn loot_pointer(&mut self, event: InputEvent) -> PanelAction {
        if matches!(event, InputEvent::PointerWheel { .. }) {
            return PanelAction::None;
        }
        if let Some(event) = self.ui.pointer(event)
            && event.kind == UiEventKind::Activate
            && event.token == Some(LOOT_BACK_TOKEN)
        {
            self.show_loot(false);
        }
        PanelAction::None
    }

    /// Draw the loot table over the whole window.
    pub(super) fn draw_loot(&mut self, frame: &Frame, viewport: [f32; 2]) {
        let _ = self.ui.draw_list_mut().push(DrawCommand::SolidRect {
            rect: sjk_ui::Rect::new(0.0, 0.0, viewport[0], viewport[1]),
            color: color::alpha(color::SPACE, 0.93),
        });
        self.loot_head(frame);
        self.loot_table(frame);
        self.loot_grid(frame);
        self.loot_notes(frame);
        self.loot_keys(frame);
    }

    /// The title, what the page is, and how often holocrons drop.
    fn loot_head(&mut self, frame: &Frame) {
        let s = frame.s;
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("Loot table"),
            frame.rect(LEFT, TITLE_Y, WIDTH, 54.0),
            44.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!(
                "What a holocron is and what opening one gives. The SJK hub rolls each one with these odds."
            ),
            frame.rect(LEFT, LINE_Y, WIDTH, 26.0),
            17.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        let pace = match (self.data.every, self.data.today) {
            (Some(every), Some((_, cap))) => format!(
                "One holocron about every {} minutes of active play on a server, at most {cap} a day.",
                every.div_ceil(60)
            ),
            _ => "Holocrons are found by playing on a server.".to_owned(),
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{pace}"),
            frame.rect(LEFT, PACE_Y, WIDTH, 24.0),
            16.0 * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    /// Per holocron: each tier's chance of dropping, its card's odds and its credits, then
    /// any holocron found.
    fn loot_table(&mut self, frame: &Frame) {
        let s = frame.s;
        kit::heading(&mut self.ui, frame, LEFT, TABLE_Y, WIDTH, "Per holocron");
        // The groups' labels over a rule each.
        for (label, x, columns) in [
            ("Card rarity", RARITY_X, 4.0),
            ("Card finish", FINISH_X, 3.0),
        ] {
            let width = CELL_WIDTH * columns;
            centred(
                &mut self.ui,
                frame,
                label,
                [x, GROUP_Y, width, 22.0],
                15.0,
                color::QUIET,
                TextFamily::Body,
                FontWeight::Regular,
            );
            let _ = self.ui.draw_list_mut().push(DrawCommand::SolidRect {
                rect: frame.rect(x + 12.0, GROUP_Y + 26.0, width - 24.0, 1.0),
                color: color::alpha(color::HOLO, 0.22),
            });
        }
        // The columns' names.
        let column = [COLUMNS_Y, 24.0];
        let name = |ui: &mut MenuCanvas, words: &str, x: f32, width: f32, colour: Color| {
            centred(
                ui,
                frame,
                words,
                [x, column[0], width, column[1]],
                16.0,
                colour,
                TextFamily::Body,
                FontWeight::Semibold,
            );
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Holocron"),
            frame.rect(LEFT + 16.0, COLUMNS_Y, TIER_WIDTH, 24.0),
            16.0 * s,
            color::MUTED,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        name(
            &mut self.ui,
            "Drop chance",
            DROP_X,
            DROP_WIDTH,
            color::MUTED,
        );
        for rarity in &RARITIES {
            let x = RARITY_X + rarity.index as f32 * CELL_WIDTH;
            let words = format!(
                "{} {}",
                crate::symbols::card_rarity(rarity.index),
                rarity.name
            );
            name(&mut self.ui, &words, x, CELL_WIDTH, rarity.colour);
        }
        for finish in &FINISHES {
            let x = FINISH_X + finish.index as f32 * CELL_WIDTH;
            let words = format!("{} {}", crate::symbols::finish(finish.index), finish.name);
            name(&mut self.ui, &words, x, CELL_WIDTH, finish_colour(finish));
        }
        name(
            &mut self.ui,
            "Credits",
            CREDITS_X,
            CREDITS_WIDTH,
            color::GOLD,
        );
        // A row per tier.
        for (index, tier) in holocrons::TIERS.iter().enumerate() {
            let y = ROWS_TOP + index as f32 * (ROW_HEIGHT + ROW_GAP);
            self.loot_row_back(frame, y, tier.colour_alpha(0.06), tier.colour);
            super::view::picture(
                &mut self.ui,
                frame,
                index,
                LEFT + 30.0,
                y + ROW_HEIGHT * 0.5,
                16.0,
                1.0,
            );
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}", tier.name),
                frame.rect(LEFT + 58.0, y + 12.0, TIER_WIDTH - 60.0, 28.0),
                22.0 * s,
                tier.colour,
                FontWeight::Semibold,
                TextAlign::Start,
            );
            self.loot_cells(
                frame,
                y,
                loot::drop(index),
                |rarity| loot::rarity_in(index, rarity),
                |finish| loot::finish_in(index, finish),
            );
            let (least, most) = crate::loot_odds::current().credits[index];
            self.loot_credits(
                frame,
                y,
                &format!("{least} to {most}"),
                loot::credits_mean(index),
            );
        }
        // Any holocron found.
        let y = ROWS_TOP + holocrons::COUNT as f32 * (ROW_HEIGHT + ROW_GAP) + ANY_GAP;
        let _ = self.ui.draw_list_mut().push(DrawCommand::SolidRect {
            rect: frame.rect(LEFT, y - ANY_GAP * 0.5 - 1.0, WIDTH, 1.0),
            color: color::alpha(color::GOLD, 0.35),
        });
        self.loot_row_back(
            frame,
            y,
            color::alpha(color::GOLD, 0.07),
            color::GOLD_BRIGHT,
        );
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("Any holocron found"),
            frame.rect(LEFT + 20.0, y + 12.0, TIER_WIDTH - 20.0, 28.0),
            22.0 * s,
            color::GOLD_BRIGHT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        self.loot_cells(frame, y, 1.0, loot::rarity_any, loot::finish_any);
        let credits = crate::loot_odds::current().credits;
        let least = credits.iter().map(|range| range.0).min().unwrap_or(0);
        let most = credits.iter().map(|range| range.1).max().unwrap_or(0);
        self.loot_credits(frame, y, &format!("{least} to {most}"), loot::credits_any());
    }

    /// A row's faint ground over the table's width, with a bar in `bar` at its left.
    fn loot_row_back(&mut self, frame: &Frame, y: f32, ground: Color, bar: Color) {
        let s = frame.s;
        let list = self.ui.draw_list_mut();
        let _ = list.push(DrawCommand::RoundedRect {
            rect: frame.rect(LEFT, y, WIDTH, ROW_HEIGHT),
            radius: 6.0 * s,
            color: ground,
        });
        let _ = list.push(DrawCommand::RoundedRect {
            rect: frame.rect(LEFT, y + 8.0, 4.0, ROW_HEIGHT - 16.0),
            radius: 2.0 * s,
            color: bar,
        });
    }

    /// A row's drop chance and its card's odds by rarity and by finish.
    fn loot_cells(
        &mut self,
        frame: &Frame,
        y: f32,
        drop: f64,
        rarity: impl Fn(usize) -> f64,
        finish: impl Fn(usize) -> f64,
    ) {
        let cell = |x: f32, width: f32| [x, y + 13.0, width, 28.0];
        chance_cell(&mut self.ui, frame, drop, cell(DROP_X, DROP_WIDTH));
        for index in 0..RARITIES.len() {
            let x = RARITY_X + index as f32 * CELL_WIDTH;
            chance_cell(&mut self.ui, frame, rarity(index), cell(x, CELL_WIDTH));
        }
        for index in 0..FINISHES.len() {
            let x = FINISH_X + index as f32 * CELL_WIDTH;
            chance_cell(&mut self.ui, frame, finish(index), cell(x, CELL_WIDTH));
        }
    }

    /// A row's credits: the range, and the average under it.
    fn loot_credits(&mut self, frame: &Frame, y: f32, range: &str, mean: f64) {
        centred(
            &mut self.ui,
            frame,
            range,
            [CREDITS_X, y + 5.0, CREDITS_WIDTH, 26.0],
            20.0,
            color::GOLD_BRIGHT,
            TextFamily::Display,
            FontWeight::Semibold,
        );
        centred(
            &mut self.ui,
            frame,
            &format!("{} on average", mean.round()),
            [CREDITS_X, y + 30.0, CREDITS_WIDTH, 18.0],
            13.0,
            color::QUIET,
            TextFamily::Body,
            FontWeight::Regular,
        );
    }

    /// Every kind of card, per holocron found: a row per rarity, a column per finish.
    fn loot_grid(&mut self, frame: &Frame) {
        let s = frame.s;
        kit::heading(
            &mut self.ui,
            frame,
            LEFT,
            GRID_Y,
            TIER_WIDTH + GRID_CELL * 3.0,
            "Every card, per holocron found",
        );
        for finish in &FINISHES {
            let words = format!("{} {}", crate::symbols::finish(finish.index), finish.name);
            centred(
                &mut self.ui,
                frame,
                &words,
                [
                    DROP_X + finish.index as f32 * GRID_CELL,
                    GRID_COLUMNS_Y + 10.0,
                    GRID_CELL,
                    24.0,
                ],
                16.0,
                finish_colour(finish),
                TextFamily::Body,
                FontWeight::Semibold,
            );
        }
        for rarity in &RARITIES {
            let y = GRID_TOP + rarity.index as f32 * (GRID_HEIGHT + GRID_GAP);
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!(
                    "{} {} card",
                    crate::symbols::card_rarity(rarity.index),
                    rarity.name
                ),
                frame.rect(LEFT + 16.0, y + 8.0, TIER_WIDTH - 16.0, 28.0),
                22.0 * s,
                rarity.colour,
                FontWeight::Semibold,
                TextAlign::Start,
            );
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{} of cards", loot::percent(loot::rarity_any(rarity.index))),
                frame.rect(LEFT + 16.0, y + 38.0, TIER_WIDTH - 16.0, 20.0),
                14.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
            for finish in &FINISHES {
                let x = DROP_X + finish.index as f32 * GRID_CELL;
                let chance = loot::kind_any(rarity.index, finish.index);
                let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: frame.rect(x + 8.0, y, GRID_CELL - 16.0, GRID_HEIGHT),
                    radius: 8.0 * s,
                    color: rarity.colour_alpha(0.05 + 0.04 * finish.index as f32),
                });
                centred(
                    &mut self.ui,
                    frame,
                    &loot::percent(chance),
                    [x, y + 7.0, GRID_CELL, 30.0],
                    24.0,
                    color::TEXT,
                    TextFamily::Display,
                    FontWeight::Semibold,
                );
                centred(
                    &mut self.ui,
                    frame,
                    &loot::one_in(chance),
                    [x, y + 38.0, GRID_CELL, 20.0],
                    14.0,
                    color::MUTED,
                    TextFamily::Body,
                    FontWeight::Regular,
                );
            }
        }
    }

    /// How the rolls are made, and the rarest card, right of the grid.
    fn loot_notes(&mut self, frame: &Frame) {
        let s = frame.s;
        kit::heading(
            &mut self.ui,
            frame,
            NOTES_X,
            GRID_Y,
            RIGHT - NOTES_X,
            "How it rolls",
        );
        let mut y = GRID_TOP - 6.0;
        for note in &notes() {
            for (line, part) in wrap(note, NOTES_CHARS).enumerate() {
                if line == 0 {
                    let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                        rect: frame.rect(NOTES_X, y + 9.0, 5.0, 5.0),
                        radius: 2.5 * s,
                        color: color::alpha(color::HOLO, 0.7),
                    });
                }
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{part}"),
                    frame.rect(NOTES_X + 16.0, y, RIGHT - NOTES_X - 16.0, 22.0),
                    15.0 * s,
                    color::MUTED,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                y += 21.0;
            }
            y += 9.0;
        }
    }

    /// Back, and the keys.
    fn loot_keys(&mut self, frame: &Frame) {
        let s = frame.s;
        kit::button(
            &mut self.ui,
            frame,
            [LEFT, BACK_Y, BACK_WIDTH, 44.0],
            "Back",
            false,
            true,
            false,
            LOOT_BACK_TOKEN,
        );
        let keys: [(&[&str], &str); 2] = [(&["L"], "holocrons"), (&["Esc"], "back")];
        let gap = 30.0 * s;
        let width: f32 = keys
            .iter()
            .map(|(caps, action)| key_hint_width(caps, action, s))
            .sum::<f32>()
            + gap;
        let [right, y] = frame.point(RIGHT, super::view::KEYS_Y);
        let mut x = right - width;
        for (caps, action) in keys {
            x = key_hint(&mut self.ui, caps, action, x, y, s) + gap;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// L and the button show the table; Escape, L, Enter and Back go back, and the page's
    /// keys wait meanwhile.
    #[test]
    fn the_table_comes_and_goes() {
        let mut panel = Panel::new();
        panel.open(true);
        assert!(!panel.showing_loot());
        assert_eq!(panel.key(KeyCode::KeyL, false), PanelAction::None);
        assert!(panel.showing_loot());
        let _ = panel.key(KeyCode::ArrowDown, false);
        assert_eq!(panel.selected, 0, "the page's keys wait");
        assert_eq!(panel.key(KeyCode::Escape, false), PanelAction::None);
        assert!(
            !panel.showing_loot(),
            "Escape goes back to the page, not out"
        );
        for key in [KeyCode::KeyL, KeyCode::Enter] {
            panel.show_loot(true);
            let _ = panel.key(key, false);
            assert!(!panel.showing_loot(), "{key:?}");
        }
        panel.show_loot(true);
        panel.open(true);
        assert!(!panel.showing_loot(), "a page opened afresh shows itself");
    }

    fn click(panel: &mut Panel, token: u16) {
        let rect = panel.ui.rect_for(token).expect("a pointer area");
        let at = sjk_ui::Vec2::new(rect.x + 4.0, rect.y + 4.0);
        for event in [
            InputEvent::PointerMove(at),
            InputEvent::PointerPress {
                position: at,
                button: sjk_ui::PointerButton::Primary,
            },
            InputEvent::PointerRelease {
                position: at,
                button: sjk_ui::PointerButton::Primary,
            },
        ] {
            let _ = panel.handle_pointer(event);
        }
    }

    #[test]
    fn the_buttons_show_and_close_it() {
        let mut panel = Panel::new();
        panel.open(true);
        panel.set_hub(true);
        panel.build([1920.0, 1080.0]);
        click(&mut panel, LOOT_TOKEN);
        assert!(panel.showing_loot());
        panel.build([1920.0, 1080.0]);
        click(&mut panel, LOOT_BACK_TOKEN);
        assert!(!panel.showing_loot());
    }

    /// The table fits the canvas and the window in every shape, the pace known or not:
    /// the grid and the notes end above Back and the keys.
    #[test]
    fn the_table_fits() {
        let known = Data {
            today: Some((3, 8)),
            every: Some(1_800),
            ..Data::nothing(State::Known)
        };
        for data in [Data::nothing(State::IdentityOff), known] {
            for viewport in [
                [1920.0, 1080.0],
                [3840.0, 2160.0],
                [1440.0, 1080.0],
                [2560.0, 1080.0],
            ] {
                let mut panel = Panel::new();
                panel.open(true);
                panel.data = data.clone();
                panel.show_loot(true);
                panel.build(viewport);
                assert!(!panel.overflowed(), "{viewport:?}");
                let frame = Frame::new(viewport);
                let back = panel.ui.rect_for(LOOT_BACK_TOKEN).expect("Back");
                assert!(back.bottom() <= viewport[1] && back.x >= 0.0);
                let grid_end = frame.point(0.0, GRID_TOP + 4.0 * (GRID_HEIGHT + GRID_GAP))[1];
                assert!(grid_end < back.y, "{viewport:?}");
                let [right, _] = frame.point(RIGHT, 0.0);
                assert!(right <= viewport[0]);
            }
        }
        // The notes' lines end above the keys.
        let notes = notes();
        assert_eq!(
            notes[2],
            "The rarest card is a Legendary Solar: about 1 in 11 001 holocrons."
        );
        let lines: usize = notes
            .iter()
            .map(|note| wrap(note, NOTES_CHARS).count())
            .sum();
        let end = GRID_TOP - 6.0 + lines as f32 * 21.0 + notes.len() as f32 * 9.0;
        assert!(end < super::view::KEYS_Y - 10.0, "the notes reach {end}");
        // The table's columns stay inside the page.
        const { assert!(CREDITS_X + 200.0 <= RIGHT && FINISH_X + CELL_WIDTH * 3.0 < CREDITS_X) };
        const { assert!(DROP_X + GRID_CELL * 3.0 < NOTES_X) };
    }
}
