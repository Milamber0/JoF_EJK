//! Create game's map list in the SJK UI's look: the top bar's way back to
//! Create game with the typed filter in its search pill, a header line over
//! fourteen rows of long names with their `mp/…` names (the chosen map marked
//! with a gold dot), and the highlighted map's levelshot large on the right
//! with its names under it. Every position is in frame pixels ([`Frame`]).

use super::create_game::{BACK_TOKEN, CreateGameMenu};
use super::create_game_catalog::MODES;
use super::create_game_view::{DETAIL_WIDTH, DETAIL_X, NAME_TOP, PICTURE, draw_keys, draw_picture};
use super::sjk::{Frame, SearchPill, color, kit, text, top_bar, wrap};
use crate::menu_widgets::TextFamily;
use sjk_ui::{Color, DrawCommand, FontWeight, TextAlign};

/// The list: its column, the header line, the rows and how many show.
const LIST_X: f32 = 96.0;
const LIST_WIDTH: f32 = 880.0;
const HEADER_TOP: f32 = 176.0;
const HEADER_HEIGHT: f32 = 34.0;
const ROWS_TOP: f32 = 216.0;
const ROW: f32 = 52.0;
pub(super) const VISIBLE: usize = 14;
/// The chosen map's gold dot, before its name, and where names start.
const MARK_X: f32 = LIST_X + 18.0;
const NAME_X: f32 = LIST_X + 34.0;
/// The file names' column, right-aligned to `FILE_RIGHT`.
const FILE_RIGHT: f32 = LIST_X + LIST_WIDTH - 22.0;
const FILE_WIDTH: f32 = 220.0;
/// The search pill's token (typing goes to the filter wherever the pointer
/// is) and the scrollbar's; neither is a row (`0..VISIBLE`).
pub(super) const SEARCH_TOKEN: u16 = 950;
pub(super) const SCROLLBAR_TOKEN: u16 = 951;
/// The highlighted map's game types, under its name.
const MODES_TOP: f32 = NAME_TOP + 60.0;

impl CreateGameMenu {
    /// Lay the open map list out on `self.ui` at `reveal` opacity.
    pub(super) fn build_picker(&mut self, viewport: [f32; 2], reveal: f32) {
        let frame = Frame::new(viewport);
        let s = frame.s;
        self.picker.set_page(VISIBLE);
        let ui = &mut self.ui;
        ui.begin_transparent(viewport);
        ui.push_opacity(reveal);
        crate::settings::sjk_view::backdrop(ui, viewport);
        let filter = self.picker.filter();
        let shown = self.picker.match_count();
        let total = self.picker.total(&self.catalogue);
        top_bar(
            ui,
            &frame,
            "Create game",
            BACK_TOKEN,
            "Choose a map",
            Some(SearchPill {
                query: filter,
                active: true,
                prompt: "",
                found: (!filter.is_empty()).then_some(shown),
                token: SEARCH_TOKEN,
            }),
        );
        // The header: how many maps the mode has, and the file names' column.
        let mode = MODES[self.draft.mode].label;
        let header = if shown == total {
            format_args!("{total} maps for {mode}")
        } else {
            format_args!("{shown} of {total} maps for {mode}")
        };
        text(
            ui,
            TextFamily::Display,
            header,
            frame.rect(NAME_X, HEADER_TOP, 560.0, HEADER_HEIGHT),
            18.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        text(
            ui,
            TextFamily::Display,
            format_args!("File"),
            frame.rect(
                FILE_RIGHT - FILE_WIDTH,
                HEADER_TOP,
                FILE_WIDTH,
                HEADER_HEIGHT,
            ),
            18.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::End,
        );
        let _ = ui.draw_list_mut().push(DrawCommand::SolidRect {
            rect: frame.rect(LIST_X, HEADER_TOP + HEADER_HEIGHT, LIST_WIDTH, 1.0),
            color: color::alpha(color::HOLO, 0.22),
        });
        let (first, selected) = (self.picker.first(), self.picker.selected());
        for slot in 0..VISIBLE.min(shown.saturating_sub(first)) {
            let position = first + slot;
            let Some(entry) = self.picker.entry(&self.catalogue, position) else {
                break;
            };
            let top = ROWS_TOP + slot as f32 * ROW;
            let token = slot as u16;
            let chosen = position == selected;
            if chosen {
                kit::band(ui, &frame, [LIST_X, top, LIST_WIDTH, ROW]);
            } else if ui.token_hovered(token) {
                let _ = ui.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: frame.rect(LIST_X, top, LIST_WIDTH, ROW),
                    radius: 10.0 * s,
                    color: color::alpha(color::HOLO, 0.05),
                });
            }
            if entry.name == self.draft.map {
                kit::changed_dot(ui, &frame, MARK_X, top + ROW * 0.5);
            }
            let name_shown = entry.title != entry.name;
            let title_right = if name_shown {
                FILE_RIGHT - FILE_WIDTH - 16.0
            } else {
                FILE_RIGHT
            };
            text(
                ui,
                TextFamily::Body,
                format_args!("{}", entry.title),
                frame.rect(NAME_X, top, title_right - NAME_X, ROW),
                19.0 * s,
                if chosen {
                    Color::new(1.0, 1.0, 1.0, 1.0)
                } else {
                    color::alpha(color::TEXT, 0.88)
                },
                FontWeight::Regular,
                TextAlign::Start,
            );
            if name_shown {
                text(
                    ui,
                    TextFamily::Display,
                    format_args!("{}", entry.name),
                    frame.rect(FILE_RIGHT - FILE_WIDTH, top, FILE_WIDTH, ROW),
                    18.0 * s,
                    if chosen { color::TEXT } else { color::MUTED },
                    FontWeight::Regular,
                    TextAlign::End,
                );
            }
            ui.hit_region(token, frame.rect(LIST_X, top, LIST_WIDTH, ROW));
        }
        if shown == 0 {
            text(
                ui,
                TextFamily::Body,
                format_args!("No map matches \u{201c}{filter}\u{201d}."),
                frame.rect(NAME_X, ROWS_TOP + 8.0, LIST_WIDTH - 56.0, 36.0),
                19.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        if shown > VISIBLE {
            ui.scrollbar(
                SCROLLBAR_TOKEN,
                frame.rect(
                    LIST_X + LIST_WIDTH + 14.0,
                    ROWS_TOP,
                    4.0,
                    VISIBLE as f32 * ROW,
                ),
                first,
                VISIBLE,
                shown,
            );
        }
        // The highlighted map, large, right of the list.
        // Nothing there while no map matches.
        if let Some(entry) = self.picker.highlighted(&self.catalogue) {
            let map = entry.name.as_str();
            draw_picture(
                &mut self.ui,
                &frame,
                PICTURE,
                self.levelshots.preview(map),
                self.levelshots.size(map),
                map,
            );
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}", entry.title),
                frame.rect(DETAIL_X, NAME_TOP, DETAIL_WIDTH, 40.0),
                32.0 * s,
                color::TEXT,
                FontWeight::Semibold,
                TextAlign::Start,
            );
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("Game types"),
                frame.rect(DETAIL_X, MODES_TOP, DETAIL_WIDTH, 20.0),
                14.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
            let mut modes = String::with_capacity(160);
            for (index, mode) in MODES.iter().enumerate() {
                if entry.supports(index) {
                    if !modes.is_empty() {
                        modes.push_str(", ");
                    }
                    modes.push_str(mode.label);
                }
            }
            for (line, part) in wrap(&modes, 60).take(3).enumerate() {
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{part}"),
                    frame.rect(
                        DETAIL_X,
                        MODES_TOP + 24.0 + line as f32 * 28.0,
                        DETAIL_WIDTH,
                        26.0,
                    ),
                    18.0 * s,
                    color::TEXT,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
            }
        }
        draw_keys(
            &mut self.ui,
            &frame,
            &[
                (&["Enter"], "choose"),
                (&["Up", "Down"], "move"),
                (&["Backspace"], "erase"),
                (&["Esc"], "create game"),
            ],
        );
        self.ui.pop_opacity();
        let focus = self.picker.selected().saturating_sub(self.picker.first());
        self.ui.finish(focus as u16);
    }
}

const _: () = assert!(NAME_X + 360.0 < FILE_RIGHT - FILE_WIDTH);
const _: () = assert!(LIST_X + LIST_WIDTH + 40.0 < DETAIL_X);
const _: () = assert!(ROWS_TOP + VISIBLE as f32 * ROW < super::create_game_view::KEYS_Y - 20.0);
const _: () = assert!(HEADER_TOP + HEADER_HEIGHT < ROWS_TOP);
