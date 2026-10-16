//! Settings > Quick wheel as the SJK UI draws it (`docs/sjk-ui.md`): the pages
//! in a column on the left, the shown page's choices in the middle, each
//! sub-headed, the focused row on the UI's band with its own small controls at
//! its end; on the right a live preview of the page's ring and what the focused
//! item does, the catalogue while a choice is picked, or a custom choice's form;
//! the keys of what has the keyboard bottom right. The Force page's middle column
//! says that its choices follow the player's powers, and its preview is an example
//! ring of a light-side build.
//!
//! Positions are pixels of the SJK UI's 16:9 frame ([`Frame`]), in the columns of
//! Settings' rows and detail; outside the SJK UI (classic+, tabbed) the same
//! layout is drawn on its own over the screen, moved left where the rail would
//! be.

use super::sjk_view::{backdrop, draw_rail};
use super::wheel_editor::*;
use super::*;
use crate::menu::sjk::{Frame, TextTarget, color, key_hint, key_hint_width, kit, text, wrap};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use crate::quick_wheel::catalog::ACTIONS;
use crate::quick_wheel::force_page;
use crate::quick_wheel::pages::{MAX_CHOICES, MAX_PAGES, Slot};
use crate::quick_wheel::ring;
use sjk_ui::{Color, DrawCommand, FontWeight, TextAlign};

/// The pages column, the choices column, their first line's top and a line.
const PAGES_X: f32 = 470.0;
const PAGES_WIDTH: f32 = 360.0;
const CHOICES_X: f32 = 870.0;
const CHOICES_WIDTH: f32 = 400.0;
const TOP: f32 = 176.0;
const LINE: f32 = 56.0;
/// The right column: the preview's middle, the facts, the catalogue's lines.
const DETAIL_X: f32 = 1360.0;
const DETAIL_WIDTH: f32 = 464.0;
const PREVIEW: [f32; 2] = [DETAIL_X + DETAIL_WIDTH * 0.5, 410.0];
const PREVIEW_SCALE: f32 = 0.78;
const PICK_TOP: f32 = 262.0;
const PICK_LINE: f32 = 40.0;
/// The keys' line.
const KEYS_Y: f32 = 992.0;
/// A row's small controls: their side and the gap between them.
const BUTTON: f32 = 30.0;
const BUTTON_GAP: f32 = 6.0;
/// How far left the layout moves without the rail (classic+, tabbed), so it
/// sits in the middle of the frame.
const OVERLAY_SHIFT: f32 = -190.0;

// The columns sit side by side within Settings' row and detail columns.
const _: () = assert!(PAGES_X + PAGES_WIDTH < CHOICES_X);
const _: () = assert!(CHOICES_X + CHOICES_WIDTH < DETAIL_X);
const _: () = assert!(TOP + (MAX_CHOICES + 2) as f32 * LINE < KEYS_Y - 20.0);
// Eight pages, Add a page, Add the Force page (while it is missing), Restore, then
// the Sound heading and its switch.
const _: () = assert!(TOP + (MAX_PAGES + 6) as f32 * LINE < KEYS_Y - 20.0);
const _: () = assert!(PICK_TOP + PICK_LINES as f32 * PICK_LINE < KEYS_Y - 20.0);

impl SettingsMenu {
    /// Draw the editor as the SJK UI's Settings category, beside `rail`.
    pub(super) fn append_wheel_sjk(
        &mut self,
        target: TextTarget<'_>,
        viewport: [f32; 2],
        reveal: f32,
        rail: &Rail<'_>,
    ) {
        let frame = Frame::new(viewport);
        let editor = &mut self.wheel;
        editor.ui.begin_transparent(viewport);
        editor.ui.push_opacity(reveal);
        backdrop(&mut editor.ui, viewport);
        let back = match editor.mode {
            WheelMode::Category => rail.back,
            WheelMode::Overlay => "Settings",
        };
        crate::menu::sjk::top_bar(&mut editor.ui, &frame, back, BACK, "Settings", None);
        draw_rail(&mut editor.ui, &frame, rail);
        editor.draw(&frame);
        editor.ui.pop_opacity();
        editor.ui.finish(0);
        target.append(&editor.ui, viewport);
    }

    /// Draw the editor on its own over the classic+ or tabbed settings, in the
    /// SJK UI's look, its text in the menus' `font`.
    pub(super) fn append_wheel_overlay(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        reveal: f32,
    ) {
        let frame = Frame::new(viewport);
        let editor = &mut self.wheel;
        editor.ui.begin_transparent(viewport);
        editor.ui.push_opacity(reveal);
        let _ = editor.ui.draw_list_mut().push(DrawCommand::SolidRect {
            rect: sjk_ui::Rect::new(0.0, 0.0, viewport[0], viewport[1]),
            color: color::alpha(color::SPACE, 0.6),
        });
        backdrop(&mut editor.ui, viewport);
        crate::menu::sjk::top_bar(
            &mut editor.ui,
            &frame,
            "Settings",
            BACK,
            "Quick wheel",
            None,
        );
        editor.draw(&frame.shifted(OVERLAY_SHIFT, 0.0));
        editor.ui.pop_opacity();
        editor.ui.finish(0);
        editor.ui.append_text(vertices, font, viewport);
    }
}

impl WheelEditor {
    /// The draw list the editor built last.
    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    /// The columns, the right column and the keys, in `frame`.
    fn draw(&mut self, frame: &Frame) {
        self.draw_pages(frame);
        self.draw_choices(frame);
        if matches!(self.typing, Some(Typing::Custom { .. })) {
            self.draw_form(frame);
        } else if self.picker.is_some() {
            self.draw_picker(frame);
        } else {
            self.draw_preview(frame);
        }
        self.draw_keys(frame);
    }

    /// The pages: each with its name (gold for the page shown) and how many
    /// choices it has, then Add a page and Restore the default pages; under
    /// them, the switch of the wheel's sounds.
    fn draw_pages(&mut self, frame: &Frame) {
        let s = frame.s;
        kit::heading(
            &mut self.ui,
            frame,
            PAGES_X,
            TOP + 36.0,
            PAGES_WIDTH,
            "Pages",
        );
        let focus = (self.column == Column::Pages && self.picker.is_none()).then_some(self.rows[0]);
        for index in 0..self.pages.len() {
            let top = TOP + LINE * (index + 1) as f32;
            let middle = top + LINE * 0.5;
            let focused = focus == Some(index);
            let shown = self.page == index;
            if focused {
                kit::band(&mut self.ui, frame, [PAGES_X, top, PAGES_WIDTH, LINE]);
            } else if shown {
                bar(&mut self.ui, frame, PAGES_X, top);
            }
            self.ui.hit_region(
                PAGE_BASE + index as u16,
                frame.rect(PAGES_X, top, PAGES_WIDTH, LINE),
            );
            let typing = match &self.typing {
                Some(Typing::Page { page, text, .. }) if *page == index => Some(text.as_str()),
                _ => None,
            };
            if let Some(typed) = typing {
                kit::field(
                    &mut self.ui,
                    frame,
                    [
                        PAGES_X + 14.0,
                        middle - kit::CONTROL_HEIGHT * 0.5,
                        PAGES_WIDTH - 28.0,
                        kit::CONTROL_HEIGHT,
                    ],
                    format_args!("{typed}_"),
                    true,
                    false,
                );
                continue;
            }
            let name = &self.pages[index].name;
            if self.confirm == Some(Confirm::RemovePage(index)) {
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("Remove {name}? Delete again"),
                    frame.rect(PAGES_X + 22.0, middle - 14.0, PAGES_WIDTH - 80.0, 28.0),
                    18.0 * s,
                    color::EMBER,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
            } else {
                let buttons = if focused { page_buttons_width() } else { 0.0 };
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{name}"),
                    frame.rect(
                        PAGES_X + 22.0,
                        middle - 14.0,
                        PAGES_WIDTH - 44.0 - buttons,
                        28.0,
                    ),
                    19.0 * s,
                    match (shown, focused) {
                        (true, _) => color::GOLD_BRIGHT,
                        (false, true) => color::TEXT,
                        (false, false) => color::alpha(color::TEXT, 0.88),
                    },
                    FontWeight::Regular,
                    TextAlign::Start,
                );
            }
            if focused {
                self.page_buttons(frame, middle, index);
            } else {
                let count = self.pages[index].choices.len();
                let force = self.pages[index].force;
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!(
                        "{}",
                        if force {
                            "Your powers".to_owned()
                        } else {
                            ChoiceCount(count).to_string()
                        }
                    ),
                    frame.rect(PAGES_X + PAGES_WIDTH - 150.0, middle - 12.0, 130.0, 24.0),
                    16.0 * s,
                    color::MUTED,
                    FontWeight::Regular,
                    TextAlign::End,
                );
            }
        }
        let add = self.pages.len();
        let full = add >= MAX_PAGES;
        self.action_row(
            frame,
            [PAGES_X, PAGES_WIDTH],
            add + 1,
            focus == Some(add),
            ADD_PAGE,
            if full {
                "8 pages at most"
            } else {
                "+  Add a page"
            },
            !full,
        );
        // The Force page, removed: a row puts it back.
        if let Some(row) = self.force_row() {
            self.action_row(
                frame,
                [PAGES_X, PAGES_WIDTH],
                row + 1,
                focus == Some(row),
                ADD_FORCE,
                if full {
                    "Force page: 8 pages at most"
                } else {
                    "+  Add the Force page"
                },
                !full,
            );
        }
        let restore = match (self.default, self.confirm == Some(Confirm::Restore)) {
            (true, _) => "These are the default pages",
            (false, true) => "Enter again to restore them",
            (false, false) => "Restore the default pages",
        };
        let confirming = self.confirm == Some(Confirm::Restore);
        let restore_row = self.restore_row();
        let top = TOP + LINE * (restore_row + 1) as f32;
        if focus == Some(restore_row) {
            kit::band(&mut self.ui, frame, [PAGES_X, top, PAGES_WIDTH, LINE]);
        }
        self.ui
            .hit_region(RESTORE, frame.rect(PAGES_X, top, PAGES_WIDTH, LINE));
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{restore}"),
            frame.rect(
                PAGES_X + 22.0,
                top + LINE * 0.5 - 14.0,
                PAGES_WIDTH - 44.0,
                28.0,
            ),
            18.0 * s,
            match (confirming, self.default) {
                (true, _) => color::EMBER,
                (false, true) => color::QUIET,
                (false, false) => color::MUTED,
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        // The wheel's sounds, under their own heading a line below.
        kit::heading(
            &mut self.ui,
            frame,
            PAGES_X,
            TOP + LINE * (restore_row + 2) as f32 + 36.0,
            PAGES_WIDTH,
            "Sound",
        );
        let top = TOP + LINE * (restore_row + 3) as f32;
        let focused = focus == Some(self.sounds_row());
        if focused {
            kit::band(&mut self.ui, frame, [PAGES_X, top, PAGES_WIDTH, LINE]);
        }
        self.ui
            .hit_region(SOUNDS, frame.rect(PAGES_X, top, PAGES_WIDTH, LINE));
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Wheel sounds"),
            frame.rect(
                PAGES_X + 22.0,
                top + LINE * 0.5 - 14.0,
                PAGES_WIDTH - 180.0,
                28.0,
            ),
            19.0 * s,
            if focused {
                color::TEXT
            } else {
                color::alpha(color::TEXT, 0.88)
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        kit::switch(
            &mut self.ui,
            frame,
            PAGES_X + PAGES_WIDTH - 16.0,
            top + LINE * 0.5,
            self.sounds,
            focused,
        );
    }

    /// The small controls at the end of the focused page's row: Rename, up,
    /// down and remove (not for the last page left).
    fn page_buttons(&mut self, frame: &Frame, middle: f32, index: usize) {
        let right = PAGES_X + PAGES_WIDTH - 12.0;
        let count = self.pages.len();
        let removable = count > 1;
        // Each control's middle, from the right.
        let mut x = right - BUTTON * 0.5;
        icon_button(
            &mut self.ui,
            frame,
            [x, middle],
            Glyph::Remove,
            removable,
            self.confirm == Some(Confirm::RemovePage(index)),
            PAGE_REMOVE,
        );
        x -= BUTTON + BUTTON_GAP;
        icon_button(
            &mut self.ui,
            frame,
            [x, middle],
            Glyph::Down,
            index + 1 < count,
            false,
            PAGE_DOWN,
        );
        x -= BUTTON + BUTTON_GAP;
        icon_button(
            &mut self.ui,
            frame,
            [x, middle],
            Glyph::Up,
            index > 0,
            false,
            PAGE_UP,
        );
        let left = x - BUTTON * 0.5 - BUTTON_GAP - 84.0;
        kit::button(
            &mut self.ui,
            frame,
            [left, middle - BUTTON * 0.5, 84.0, BUTTON],
            "Rename",
            false,
            true,
            false,
            PAGE_RENAME,
        );
    }

    /// The shown page's choices, each with its picture (or a ring) and name,
    /// what it is on the right, then Add a choice.
    fn draw_choices(&mut self, frame: &Frame) {
        let s = frame.s;
        let heading = format!(
            "On {}",
            self.pages
                .get(self.page)
                .map_or("", |page| page.name.as_str())
        );
        kit::heading(
            &mut self.ui,
            frame,
            CHOICES_X,
            TOP + 36.0,
            CHOICES_WIDTH,
            &heading,
        );
        if self.force_shown() {
            self.draw_force_note(frame);
            return;
        }
        let focus =
            (self.column == Column::Choices && self.picker.is_none()).then_some(self.rows[1]);
        let picking = self.picker.map(|picker| picker.at);
        let count = self.choices().len();
        for index in 0..count {
            let top = TOP + LINE * (index + 1) as f32;
            let middle = top + LINE * 0.5;
            let focused = focus == Some(index);
            let picked = picking == Some(Some(index));
            if focused {
                kit::band(&mut self.ui, frame, [CHOICES_X, top, CHOICES_WIDTH, LINE]);
            } else if picked {
                bar(&mut self.ui, frame, CHOICES_X, top);
            }
            self.ui.hit_region(
                CHOICE_BASE + index as u16,
                frame.rect(CHOICES_X, top, CHOICES_WIDTH, LINE),
            );
            let slot = self.choices()[index].clone();
            choice_icon(
                &mut self.ui,
                frame,
                CHOICES_X + 38.0,
                middle,
                38.0,
                slot.texture(),
            );
            let buttons = if focused {
                3.0 * BUTTON + 2.0 * BUTTON_GAP + 12.0
            } else {
                110.0
            };
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{}", slot.label()),
                frame.rect(
                    CHOICES_X + 70.0,
                    middle - 14.0,
                    CHOICES_WIDTH - 86.0 - buttons,
                    28.0,
                ),
                19.0 * s,
                if focused || picked {
                    color::TEXT
                } else {
                    color::alpha(color::TEXT, 0.88)
                },
                FontWeight::Regular,
                TextAlign::Start,
            );
            if focused {
                let right = CHOICES_X + CHOICES_WIDTH - 12.0;
                let mut x = right - BUTTON * 0.5;
                for (glyph, enabled, token) in [
                    (Glyph::Remove, true, CHOICE_REMOVE),
                    (Glyph::Down, index + 1 < count, CHOICE_DOWN),
                    (Glyph::Up, index > 0, CHOICE_UP),
                ] {
                    icon_button(
                        &mut self.ui,
                        frame,
                        [x, middle],
                        glyph,
                        enabled,
                        false,
                        token,
                    );
                    x -= BUTTON + BUTTON_GAP;
                }
            } else {
                let kind = match &slot {
                    Slot::Action(action) => ACTIONS[*action].group.label(),
                    Slot::Custom { .. } => "Custom",
                };
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{kind}"),
                    frame.rect(
                        CHOICES_X + CHOICES_WIDTH - 130.0,
                        middle - 12.0,
                        110.0,
                        24.0,
                    ),
                    15.0 * s,
                    color::MUTED,
                    FontWeight::Regular,
                    TextAlign::End,
                );
            }
        }
        let full = count >= MAX_CHOICES;
        self.action_row(
            frame,
            [CHOICES_X, CHOICES_WIDTH],
            count + 1,
            focus == Some(count) || picking == Some(None),
            ADD_CHOICE,
            if full {
                "10 choices at most: the ring is full"
            } else {
                "+  Add a choice"
            },
            !full,
        );
    }

    /// The Force page's middle column: no choices to edit, what fills it instead.
    fn draw_force_note(&mut self, frame: &Frame) {
        let s = frame.s;
        let top = TOP + LINE;
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Follows your Force powers"),
            frame.rect(
                CHOICES_X + 22.0,
                top + LINE * 0.5 - 14.0,
                CHOICES_WIDTH - 44.0,
                28.0,
            ),
            19.0 * s,
            color::GOLD_BRIGHT,
            FontWeight::Regular,
            TextAlign::Start,
        );
        let words = "Its choices are set by the game, not here: every power you can use when the wheel opens, Push first. An instant power is used at once and selected; Grip, Lightning, Drain and Stasis are only selected, for your Use Force key. Past 12 powers the rest go on a second page.";
        for (line, part) in wrap(words, 40).take(8).enumerate() {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{part}"),
                frame.rect(
                    CHOICES_X + 22.0,
                    top + LINE + line as f32 * 28.0,
                    CHOICES_WIDTH - 44.0,
                    26.0,
                ),
                17.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
    }

    /// A row that does one thing (add a page, add a choice) on line `line` of a
    /// column at `[x, width]`.
    #[allow(clippy::too_many_arguments)]
    fn action_row(
        &mut self,
        frame: &Frame,
        [x, width]: [f32; 2],
        line: usize,
        focused: bool,
        token: u16,
        label: &str,
        enabled: bool,
    ) {
        let top = TOP + LINE * line as f32;
        if focused {
            kit::band(&mut self.ui, frame, [x, top, width, LINE]);
        }
        self.ui.hit_region(token, frame.rect(x, top, width, LINE));
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{label}"),
            frame.rect(x + 22.0, top + LINE * 0.5 - 14.0, width - 44.0, 28.0),
            18.0 * frame.s,
            match (enabled, focused) {
                (false, _) => color::QUIET,
                (true, true) => color::GOLD_BRIGHT,
                (true, false) => color::alpha(color::GOLD_BRIGHT, 0.8),
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    /// The shown page's ring as the wheel draws it (the focused choice
    /// highlighted), then what the focused row does.
    fn draw_preview(&mut self, frame: &Frame) {
        let s = frame.s;
        let slots = self.choices().to_vec();
        let force = self.force_shown();
        let example = if force { force_example() } else { Vec::new() };
        let choices: Vec<ring::Choice<'_>> = if force {
            example
                .iter()
                .map(|choice| ring::Choice {
                    label: &choice.label,
                    icon: choice.icon,
                    size: None,
                    on: choice.on,
                })
                .collect()
        } else {
            slots
                .iter()
                .map(|slot| ring::Choice {
                    label: slot.label(),
                    icon: slot.texture(),
                    size: None,
                    on: false,
                })
                .collect()
        };
        let highlighted =
            (self.column == Column::Choices && self.rows[1] < slots.len()).then_some(self.rows[1]);
        let name = self
            .pages
            .get(self.page)
            .map_or(String::new(), |page| page.name.clone());
        ring::draw(
            &mut self.ui,
            &ring::Ring {
                centre: frame.point(PREVIEW[0], PREVIEW[1]),
                unit: PREVIEW_SCALE * s,
                page: &name,
                choices: &choices,
                highlighted,
                pointer: highlighted.map(|index| (ring::angle(index, slots.len()), 1.0)),
                pages: (self.page, self.pages.len()),
                neighbours: None,
                arrival: (1.0, 0.0),
                hint: false,
                empty: &ring::EMPTY,
            },
        );
        let mut y = PREVIEW[1] + ring::REACH * PREVIEW_SCALE + 30.0;
        let pages = self.pages.len();
        match (self.column, self.row()) {
            (Column::Pages, row) if row < pages => {
                let page = &self.pages[row];
                let keys = self.keys.get(row).cloned().unwrap_or_default();
                let bind = format!("{} {}", crate::quick_wheel::OPEN_COMMAND, page.id);
                if keys.is_empty() {
                    fact(&mut self.ui, frame, &mut y, "Key", "None", color::MUTED);
                } else {
                    fact(
                        &mut self.ui,
                        frame,
                        &mut y,
                        "Key",
                        &keys,
                        color::GOLD_BRIGHT,
                    );
                }
                fact(&mut self.ui, frame, &mut y, "Bind", &bind, color::HOLO);
                y += 10.0;
                note(
                    &mut self.ui,
                    frame,
                    &mut y,
                    if page.force {
                        "An example: in a game the page holds the Force powers you can use as the wheel opens, the one selected marked."
                    } else {
                        "Hold the key and point at a choice, let go to run it. While it is open, scroll or click to change page; a bare +wheel opens on the page shown last."
                    },
                );
            }
            (Column::Pages, row) if row == pages => note(
                &mut self.ui,
                frame,
                &mut y,
                "A new page starts empty; name it, then add its choices. Up to 8 pages.",
            ),
            (Column::Pages, row) if Some(row) == self.force_row() => note(
                &mut self.ui,
                frame,
                &mut y,
                "The page of your Force powers, back after General (or last), its choices set by the game.",
            ),
            (Column::Pages, row) if row == self.restore_row() => note(
                &mut self.ui,
                frame,
                &mut y,
                "Back to the wheel's own pages, Force, General, Toys and Weather. Your pages and choices are removed.",
            ),
            (Column::Pages, _) => note(
                &mut self.ui,
                frame,
                &mut y,
                "The game's menu sounds as the wheel changes page, moves to another choice and runs one, at the effects volume. Enter switches them.",
            ),
            (Column::Choices, row) => match slots.get(row) {
                Some(slot) => {
                    fact(
                        &mut self.ui,
                        frame,
                        &mut y,
                        "Runs",
                        slot.command(),
                        color::HOLO,
                    );
                    let kind = match slot {
                        Slot::Action(action) => ACTIONS[*action].group.label(),
                        Slot::Custom { .. } => "Custom command",
                    };
                    fact(&mut self.ui, frame, &mut y, "Kind", kind, color::TEXT);
                    y += 10.0;
                    note(
                        &mut self.ui,
                        frame,
                        &mut y,
                        "Enter changes it; Shift with Up or Down moves it round the ring; Delete takes it off.",
                    );
                }
                None => note(
                    &mut self.ui,
                    frame,
                    &mut y,
                    "Pick an action, or name a console command of your own. Up to 10 a page.",
                ),
            },
        }
    }

    /// The catalogue: what the choice will be, then every action under its
    /// group's sub-heading and the custom command last.
    fn draw_picker(&mut self, frame: &Frame) {
        let s = frame.s;
        let Some(picker) = self.picker else {
            return;
        };
        let title = match picker.at.and_then(|at| self.choices().get(at)) {
            Some(slot) => format!("Change {}", slot.label()),
            None => format!(
                "Add to {}",
                self.pages
                    .get(self.page)
                    .map_or("", |page| page.name.as_str())
            ),
        };
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{title}"),
            frame.rect(DETAIL_X, 186.0, DETAIL_WIDTH - 110.0, 40.0),
            30.0 * s,
            color::TEXT,
            FontWeight::Regular,
            TextAlign::Start,
        );
        kit::button(
            &mut self.ui,
            frame,
            [DETAIL_X + DETAIL_WIDTH - 96.0, 190.0, 96.0, 32.0],
            "Cancel",
            false,
            true,
            false,
            PICK_CLOSE,
        );
        let lines = pick_lines();
        let shown = picker.first..lines.len().min(picker.first + PICK_LINES);
        self.ui.scroll_region(
            PICK_SCROLL,
            frame.rect(
                DETAIL_X,
                PICK_TOP,
                DETAIL_WIDTH,
                PICK_LINES as f32 * PICK_LINE,
            ),
        );
        let on_page: Vec<usize> = self
            .choices()
            .iter()
            .filter_map(|slot| match slot {
                Slot::Action(action) => Some(*action),
                Slot::Custom { .. } => None,
            })
            .collect();
        for (slot, line) in shown.clone().enumerate() {
            let top = PICK_TOP + slot as f32 * PICK_LINE;
            let middle = top + PICK_LINE * 0.5;
            let label: std::borrow::Cow<'_, str> = match lines[line] {
                PickLine::Heading(heading) => {
                    kit::heading(
                        &mut self.ui,
                        frame,
                        DETAIL_X,
                        middle + 4.0,
                        DETAIL_WIDTH,
                        heading,
                    );
                    continue;
                }
                PickLine::Action(action) => ACTIONS[action].label.into(),
                PickLine::Emote(index) => {
                    emote_choice(index).map_or_else(|| "Emote".into(), |(name, _)| name.into())
                }
                PickLine::Custom => "Custom command...".into(),
            };
            let lit = line == picker.highlighted;
            if lit {
                kit::band(
                    &mut self.ui,
                    frame,
                    [DETAIL_X, top, DETAIL_WIDTH, PICK_LINE],
                );
            }
            self.ui.hit_region(
                PICK_BASE + slot as u16,
                frame.rect(DETAIL_X, top, DETAIL_WIDTH, PICK_LINE),
            );
            let wheel = |name| {
                crate::quick_wheel::catalog::icon_index(name).map(crate::ui_renderer::wheel_icon)
            };
            let icon = match lines[line] {
                PickLine::Action(action) => ACTIONS[action].icon.and_then(wheel),
                PickLine::Emote(index) => emote_choice(index)
                    .and_then(|(_, command)| crate::emote_icons::for_command(&command))
                    .or_else(|| wheel("taunt")),
                PickLine::Custom => wheel(crate::quick_wheel::catalog::CUSTOM_ICON),
                PickLine::Heading(_) => None,
            };
            choice_icon(&mut self.ui, frame, DETAIL_X + 30.0, middle, 30.0, icon);
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{label}"),
                frame.rect(DETAIL_X + 58.0, middle - 13.0, DETAIL_WIDTH - 190.0, 26.0),
                18.0 * s,
                if lit {
                    Color::new(1.0, 1.0, 1.0, 1.0)
                } else {
                    color::alpha(color::TEXT, 0.88)
                },
                FontWeight::Regular,
                TextAlign::Start,
            );
            if matches!(lines[line], PickLine::Action(action) if on_page.contains(&action)) {
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("On the page"),
                    frame.rect(DETAIL_X + DETAIL_WIDTH - 130.0, middle - 11.0, 116.0, 22.0),
                    14.0 * s,
                    color::GOLD,
                    FontWeight::Regular,
                    TextAlign::End,
                );
            }
        }
        if lines.len() > PICK_LINES {
            self.ui.list_scroll_mark(
                frame.rect(
                    DETAIL_X + DETAIL_WIDTH + 10.0,
                    PICK_TOP,
                    4.0,
                    PICK_LINES as f32 * PICK_LINE,
                ),
                picker.first,
                PICK_LINES,
                lines.len(),
                s,
            );
        }
    }

    /// A custom choice's form: its name and its console command, Save and
    /// Cancel.
    fn draw_form(&mut self, frame: &Frame) {
        let s = frame.s;
        let Some(Typing::Custom {
            at,
            label,
            command,
            field,
        }) = self.typing.clone()
        else {
            return;
        };
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!(
                "{}",
                if at.is_some() {
                    "Custom choice"
                } else {
                    "New custom choice"
                }
            ),
            frame.rect(DETAIL_X, 186.0, DETAIL_WIDTH, 40.0),
            30.0 * s,
            color::TEXT,
            FontWeight::Regular,
            TextAlign::Start,
        );
        let mut y = 240.0;
        note(
            &mut self.ui,
            frame,
            &mut y,
            "Any console command, as you would type it in the console or a bind; several go one after another with ;",
        );
        y += 14.0;
        for (name, value, which, token) in [
            ("Name", label.as_str(), Field::Label, FORM_LABEL),
            ("Command", command.as_str(), Field::Command, FORM_COMMAND),
        ] {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{name}"),
                frame.rect(DETAIL_X, y, DETAIL_WIDTH, 26.0),
                17.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
            y += 30.0;
            let active = field == which;
            kit::field(
                &mut self.ui,
                frame,
                [DETAIL_X, y, DETAIL_WIDTH, 44.0],
                format_args!("{value}{}", if active { "_" } else { "" }),
                active,
                false,
            );
            self.ui
                .hit_region(token, frame.rect(DETAIL_X, y, DETAIL_WIDTH, 44.0));
            y += 64.0;
        }
        let ready = !label.trim().is_empty() && !command.trim().is_empty();
        kit::button(
            &mut self.ui,
            frame,
            [DETAIL_X + DETAIL_WIDTH - 250.0, y, 112.0, 40.0],
            "Cancel",
            false,
            true,
            false,
            FORM_CANCEL,
        );
        kit::button(
            &mut self.ui,
            frame,
            [DETAIL_X + DETAIL_WIDTH - 126.0, y, 126.0, 40.0],
            "Save",
            true,
            ready,
            false,
            FORM_SAVE,
        );
    }

    /// The keys of what has the keyboard, right-aligned at the bottom.
    fn draw_keys(&mut self, frame: &Frame) {
        let s = frame.s;
        let category = self.mode == WheelMode::Category;
        let pages = self.pages.len();
        let mut keys: Vec<(&[&str], &str)> = Vec::with_capacity(6);
        match (&self.typing, self.picker, self.confirm) {
            (Some(Typing::Page { .. }), _, _) => {
                keys.extend([(&["Enter"][..], "save"), (&["Esc"][..], "cancel")]);
            }
            (Some(Typing::Custom { field, .. }), _, _) => {
                keys.push((&["Tab"][..], "other field"));
                keys.push((
                    &["Enter"][..],
                    if *field == Field::Label {
                        "next"
                    } else {
                        "save"
                    },
                ));
                keys.push((&["Esc"][..], "cancel"));
            }
            (None, Some(_), _) => {
                keys.extend([
                    (&["Up", "Down"][..], "choose"),
                    (&["Enter"][..], "pick"),
                    (&["Esc"][..], "cancel"),
                ]);
            }
            (None, None, Some(Confirm::RemovePage(_))) => {
                keys.extend([(&["Delete"][..], "remove"), (&["Esc"][..], "keep")]);
            }
            (None, None, Some(Confirm::Restore)) => {
                keys.extend([(&["Enter"][..], "restore"), (&["Esc"][..], "keep")]);
            }
            (None, None, None) => match (self.column, self.row()) {
                (Column::Pages, row) if row < pages => {
                    if !self.pages[row].force {
                        keys.push((&["Enter"][..], "choices"));
                    }
                    keys.extend([
                        (&["F2"][..], "rename"),
                        (&["Shift", "Up", "Down"][..], "move"),
                        (&["Delete"][..], "remove"),
                    ]);
                }
                (Column::Choices, row) if row < self.choices().len() => {
                    keys.extend([
                        (&["Enter"][..], "change"),
                        (&["Shift", "Up", "Down"][..], "move"),
                        (&["Delete"][..], "remove"),
                        (&["Left"][..], "pages"),
                    ]);
                }
                _ => keys.push((&["Enter"][..], "do it")),
            },
        }
        if self.typing.is_none() && self.picker.is_none() && self.confirm.is_none() {
            if category {
                keys.push((&["Tab"][..], "next group"));
            } else {
                keys.push((&["Esc"][..], "back"));
            }
        }
        let gap = 30.0 * s;
        let width: f32 = keys
            .iter()
            .map(|(caps, action)| key_hint_width(caps, action, s))
            .sum::<f32>()
            + gap * keys.len().saturating_sub(1) as f32;
        let [right, y] = frame.point(DETAIL_X + DETAIL_WIDTH, KEYS_Y);
        let mut x = right - width;
        for (caps, action) in keys {
            x = key_hint(&mut self.ui, caps, action, x, y, s) + gap;
        }
    }
}

/// "8 choices", "1 choice", "Empty".
struct ChoiceCount(usize);

impl std::fmt::Display for ChoiceCount {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            0 => formatter.write_str("Empty"),
            1 => formatter.write_str("1 choice"),
            count => write!(formatter, "{count} choices"),
        }
    }
}

/// The Force page's example ring: a light-side build, Protect selected, in the
/// Force bar's pictures.
fn force_example() -> Vec<crate::quick_wheel::ShownChoice> {
    const LIGHT: [u8; 9] = [3, 4, 2, 14, 0, 9, 10, 5, 11];
    let known = LIGHT.iter().fold(0, |known, slot| known | 1 << slot);
    force_page::choices(&force_page::Powers {
        known,
        selected: 9,
        flamethrower: false,
        icons: std::array::from_fn(|slot| {
            Some(sjk_ui::TextureId(
                crate::ui_renderer::FORCE_WHEEL_ICON_FIRST + slot as u32,
            ))
        }),
    })
}

/// Width the focused page row's controls take at its end.
fn page_buttons_width() -> f32 {
    3.0 * BUTTON + 84.0 + 3.0 * BUTTON_GAP + 12.0
}

/// The gold bar at a row's left that marks the page shown (or the choice being
/// changed) while the focus is elsewhere.
fn bar(canvas: &mut MenuCanvas, frame: &Frame, x: f32, top: f32) {
    let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
        rect: frame.rect(x, top + LINE * 0.25, 4.0, LINE * 0.5),
        radius: 2.0 * frame.s,
        color: color::alpha(color::GOLD, 0.8),
    });
}

/// A choice's picture `size` across centred on (`x`, `y`), or a holo ring for
/// one without.
fn choice_icon(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    x: f32,
    y: f32,
    size: f32,
    icon: Option<sjk_ui::TextureId>,
) {
    let rect = frame.rect(x - size * 0.5, y - size * 0.5, size, size);
    let command = match icon {
        Some(texture) => DrawCommand::TexturedQuad {
            rect,
            texture,
            color: Color::new(1.0, 1.0, 1.0, 0.95),
        },
        None => DrawCommand::Border {
            rect,
            radius: rect.width * 0.5,
            width: (1.5 * frame.s).max(1.0),
            color: color::alpha(color::HOLO, 0.6),
        },
    };
    let _ = canvas.draw_list_mut().push(command);
}

/// What a small control shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Glyph {
    Up,
    Down,
    Remove,
}

/// A small round control centred on `at` (frame pixels) answering to `token`:
/// a caret up or down, or a cross; dimmed and inert when not `enabled`, ember
/// while a removal awaits its confirmation (`armed`).
fn icon_button(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    at: [f32; 2],
    glyph: Glyph,
    enabled: bool,
    armed: bool,
    token: u16,
) {
    let s = frame.s;
    let [x, y] = at;
    let rect = frame.rect(x - BUTTON * 0.5, y - BUTTON * 0.5, BUTTON, BUTTON);
    let hovered = enabled && canvas.token_hovered(token);
    let fill = match (armed, hovered) {
        (true, _) => color::alpha(color::EMBER, 0.85),
        (false, true) => color::alpha(color::HOLO, 0.2),
        (false, false) => color::alpha(color::HOLO, 0.06),
    };
    let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
        rect,
        radius: rect.width * 0.5,
        color: fill,
    });
    let _ = canvas.draw_list_mut().push(DrawCommand::Border {
        rect,
        radius: rect.width * 0.5,
        width: (1.2 * s).max(1.0),
        color: color::alpha(color::HOLO, if enabled { 0.45 } else { 0.15 }),
    });
    let ink = match (enabled, armed, hovered) {
        (false, _, _) => color::alpha(color::QUIET, 0.5),
        (true, true, _) => Color::new(0.1, 0.04, 0.02, 1.0),
        (true, false, true) => color::TEXT,
        (true, false, false) => color::MUTED,
    };
    let centre = frame.point(x, y);
    match glyph {
        Glyph::Up | Glyph::Down => {
            // A quarter ring bulging up or down, as the kit's carets.
            let towards = if glyph == Glyph::Up {
                -std::f32::consts::FRAC_PI_2
            } else {
                std::f32::consts::FRAC_PI_2
            };
            let radius = 7.0 * s;
            let back = radius * 0.75;
            let _ = canvas.draw_list_mut().push(DrawCommand::Arc {
                center: [
                    centre[0] - towards.cos() * back,
                    centre[1] - towards.sin() * back,
                ],
                radius,
                width: 2.2 * s,
                start: towards - std::f32::consts::FRAC_PI_4,
                sweep: std::f32::consts::FRAC_PI_2,
                color: ink,
                knockout: None,
            });
        }
        Glyph::Remove => {
            // Two strokes of overlapping dots across each other.
            let reach = 5.5 * s;
            let dot = 2.4 * s;
            for (dx, dy) in [(1.0, 1.0), (1.0, -1.0)] {
                for step in 0..=10 {
                    let t = step as f32 / 10.0 * 2.0 - 1.0;
                    let px = centre[0] + dx * t * reach;
                    let py = centre[1] + dy * t * reach;
                    let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                        rect: sjk_ui::Rect::new(px - dot * 0.5, py - dot * 0.5, dot, dot),
                        radius: dot * 0.5,
                        color: ink,
                    });
                }
            }
        }
    }
    if enabled {
        canvas.hit_region(token, rect);
    }
}

/// One fact of the right column: `name` then `value`, at `y`, which moves on.
fn fact(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    y: &mut f32,
    name: &str,
    value: &str,
    colour: Color,
) {
    let s = frame.s;
    text(
        canvas,
        TextFamily::Body,
        format_args!("{name}"),
        frame.rect(DETAIL_X, *y, 96.0, 28.0),
        17.0 * s,
        color::MUTED,
        FontWeight::Regular,
        TextAlign::Start,
    );
    for line in wrap(value, 38).take(3) {
        text(
            canvas,
            TextFamily::Body,
            format_args!("{line}"),
            frame.rect(DETAIL_X + 96.0, *y, DETAIL_WIDTH - 96.0, 28.0),
            17.0 * s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
        *y += 28.0;
    }
    *y += 4.0;
}

/// Running text in the right column at `y`, which moves on past it.
fn note(canvas: &mut MenuCanvas, frame: &Frame, y: &mut f32, words: &str) {
    for line in wrap(words, 48).take(5) {
        text(
            canvas,
            TextFamily::Body,
            format_args!("{line}"),
            frame.rect(DETAIL_X, *y, DETAIL_WIDTH, 26.0),
            16.0 * frame.s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        *y += 26.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quick_wheel::pages::Slot;

    fn console() -> (tempfile::TempDir, ViewerConsole) {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        (directory, console)
    }

    #[test]
    fn every_state_of_a_full_wheel_fits_the_canvas_at_any_size() {
        let (_directory, mut console) = console();
        while console.wheel_pages.add_page("").is_some() {}
        for page in 0..MAX_PAGES {
            while console
                .wheel_pages
                .set_choice(
                    page,
                    None,
                    Slot::Custom {
                        label: "Twenty-four characters!!".into(),
                        command: "say a long command; say another".into(),
                    },
                )
                .is_some()
            {}
        }
        let mut menu = SettingsMenu::new();
        let font = crate::text::load_modern(1.0, None).expect("Inter");
        let rail = Rail {
            categories: &[("Quick wheel", "options"); 12],
            current: Some(9),
            back: "Main menu",
            mode: crate::settings::mode::SettingsMode::Advanced,
        };
        for viewport in [[1920.0, 1080.0], [1440.0, 1080.0], [3840.0, 2160.0]] {
            for mode in [WheelMode::Category, WheelMode::Overlay] {
                for keys in [
                    &[][..],
                    &[KeyCode::ArrowRight][..],
                    &[KeyCode::ArrowRight, KeyCode::Enter][..],
                    &[KeyCode::ArrowRight, KeyCode::F2][..],
                    &[KeyCode::F2][..],
                    &[KeyCode::Delete][..],
                    // The pages after Force have choices to edit.
                    &[KeyCode::ArrowDown, KeyCode::ArrowRight, KeyCode::Enter][..],
                    &[KeyCode::ArrowDown, KeyCode::ArrowRight, KeyCode::F2][..],
                ] {
                    menu.open_wheel_editor(&console, mode);
                    for key in keys {
                        menu.wheel_key(*key, None, &mut console);
                    }
                    let mut vertices = Vec::new();
                    match mode {
                        WheelMode::Category => menu.append_wheel_sjk(
                            TextTarget::Inter(&mut vertices, &font.font),
                            viewport,
                            1.0,
                            &rail,
                        ),
                        WheelMode::Overlay => {
                            menu.append_wheel_overlay(&mut vertices, &font.font, viewport, 1.0)
                        }
                    }
                    assert!(
                        !menu.wheel.ui.overflowed(),
                        "{viewport:?} {mode:?} {keys:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_pointer_focuses_on_hover_and_acts_on_a_click() {
        use sjk_ui::{InputEvent, PointerButton, Vec2};
        let (_directory, mut console) = console();
        let font = crate::text::load_modern(1.0, None).expect("Inter");
        let mut menu = SettingsMenu::new();
        menu.open_wheel_editor(&console, WheelMode::Overlay);
        let draw = |menu: &mut SettingsMenu| {
            let mut vertices = Vec::new();
            menu.append_wheel_overlay(&mut vertices, &font.font, [1920.0, 1080.0], 1.0);
        };
        let centre = |menu: &SettingsMenu, token: u16| {
            let rect = menu.wheel.ui.rect_for(token).expect("drawn");
            Vec2::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5)
        };
        let hover = |menu: &mut SettingsMenu, console: &mut ViewerConsole, at: Vec2| {
            menu.handle_pointer(InputEvent::PointerMove(at), console);
        };
        let click = |menu: &mut SettingsMenu, console: &mut ViewerConsole, at: Vec2| {
            let button = PointerButton::Primary;
            menu.handle_pointer(InputEvent::PointerMove(at), console);
            menu.handle_pointer(
                InputEvent::PointerPress {
                    position: at,
                    button,
                },
                console,
            );
            menu.handle_pointer(
                InputEvent::PointerRelease {
                    position: at,
                    button,
                },
                console,
            );
        };
        draw(&mut menu);
        // The Force page is shown first: no choices, so no Add a choice to
        // point at.
        assert_eq!(menu.wheel.page, 0);
        assert!(menu.wheel.ui.rect_for(PAGE_BASE).is_some());
        assert!(menu.wheel.ui.rect_for(ADD_CHOICE).is_none());
        // Hovering General focuses it; a click shows its choices.
        let general = centre(&menu, PAGE_BASE + 1);
        hover(&mut menu, &mut console, general);
        assert_eq!((menu.wheel.rows[0], menu.wheel.page), (1, 0));
        click(&mut menu, &mut console, general);
        assert_eq!(menu.wheel.page, 1);
        draw(&mut menu);
        assert!(menu.wheel.ui.rect_for(ADD_CHOICE).is_some());
        // Hovering Weather focuses it without showing it; a click shows it.
        let weather = centre(&menu, PAGE_BASE + 3);
        hover(&mut menu, &mut console, weather);
        assert_eq!((menu.wheel.rows[0], menu.wheel.page), (3, 1));
        click(&mut menu, &mut console, weather);
        assert_eq!(menu.wheel.page, 3);
        // Add a choice: hovered, then clicked, opens the catalogue.
        draw(&mut menu);
        let add = centre(&menu, ADD_CHOICE);
        hover(&mut menu, &mut console, add);
        assert_eq!(
            (menu.wheel.column, menu.wheel.rows[1]),
            (Column::Choices, 8)
        );
        click(&mut menu, &mut console, add);
        assert!(menu.wheel.picker.is_some());
        // A click on a line of the catalogue puts it on the page.
        draw(&mut menu);
        let line = menu.wheel.picker.unwrap().first;
        let slot = (0..PICK_LINES as u16)
            .find(|slot| matches!(pick_lines()[line + usize::from(*slot)], PickLine::Action(_)))
            .unwrap();
        let at = centre(&menu, PICK_BASE + slot);
        click(&mut menu, &mut console, at);
        assert!(menu.wheel.picker.is_none());
        assert_eq!(console.wheel_pages.pages()[3].choices.len(), 9);
        // The focused choice's cross removes it.
        draw(&mut menu);
        let at = centre(&menu, CHOICE_REMOVE);
        click(&mut menu, &mut console, at);
        assert_eq!(console.wheel_pages.pages()[3].choices.len(), 8);
        // The way back closes the editor.
        draw(&mut menu);
        let at = centre(&menu, BACK);
        click(&mut menu, &mut console, at);
        assert!(!menu.wheel_editor_open());
    }

    #[test]
    fn choice_counts_read_as_words() {
        assert_eq!(ChoiceCount(0).to_string(), "Empty");
        assert_eq!(ChoiceCount(1).to_string(), "1 choice");
        assert_eq!(ChoiceCount(8).to_string(), "8 choices");
    }
}
