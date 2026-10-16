//! The resolution list of the settings screen: Enter or a click on the
//! Resolution row opens it, in every menu style, as the SJK UI's pop-up card
//! over the darkened screen (`docs/sjk-ui.md`, Settings). Rows show each size
//! with its aspect-ratio group, the size in use in gold with its dot and
//! tagged, the desktop's size tagged; arrows, pages, Home/End or the wheel
//! move, Enter or a click picks and Esc (or its key cap under the card) goes
//! back to the rows. Positions are pixels of the SJK UI's 16:9 frame
//! ([`Frame`]).

use super::resolution::{self, parse_size};
use super::*;
use crate::menu::sjk::{Frame, TextTarget, color, key_hint, key_hint_width, kit, text};
use crate::menu_widgets::TextFamily;
use sjk_ui::{DrawCommand, FontWeight, InputEvent, TextAlign, UiEventKind};

/// The way back's target: the Esc cap under the card.
const BACK_TOKEN: u16 = 900;
/// The card's left edge, top and width; it is as tall as the rows it shows.
const CARD_X: f32 = 600.0;
const CARD_TOP: f32 = 80.0;
const CARD_WIDTH: f32 = 720.0;
/// The card's inner margin, and its text's column.
const MARGIN: f32 = 48.0;
const TEXT_X: f32 = CARD_X + MARGIN;
const TEXT_WIDTH: f32 = CARD_WIDTH - MARGIN * 2.0;
/// The first row's top, a row's height and most rows shown before the list
/// scrolls.
const ROWS_TOP: f32 = CARD_TOP + 140.0;
const ROW: f32 = 48.0;
const ROWS: usize = 14;
/// Room for a size ("3840 x 2160"), and for the in-use dot after the tags.
const SIZE_WIDTH: f32 = 220.0;
const DOT_ROOM: f32 = 26.0;
/// The keys' line under the card, and the room between its hints.
const KEYS: [(&[&str], &str); 4] = [
    (&["Up", "Down"], "Choose"),
    (&["PgUp", "PgDn"], "Page"),
    (&["Enter"], "Pick"),
    (&["Esc"], "Back"),
];
const KEY_GAP: f32 = 22.0;

// The card, with every row, and the keys under it inside the frame.
const _: () = assert!(ROWS_TOP + ROWS as f32 * ROW + MARGIN * 0.5 + 28.0 + 24.0 <= 1080.0);

impl SettingsMenu {
    /// The `r_resolution` size in use.
    fn current_resolution(console: &ViewerConsole) -> Option<[u32; 2]> {
        console.text_value("r_resolution").and_then(parse_size)
    }

    /// The display mode that is applied now.
    fn applied_display(&self, console: &ViewerConsole) -> DisplayMode {
        DisplayMode::requested(console).effective(self.exclusive_available())
    }

    /// Rebuild the sizes on offer for the current display mode.
    pub(super) fn build_choices(&mut self, console: &ViewerConsole) {
        let exclusive = self.applied_display(console) == DisplayMode::Exclusive;
        resolution::build_choices(
            self.monitor.as_ref(),
            Self::current_resolution(console),
            exclusive,
            &mut self.choices,
        );
    }

    /// Open the list on the size in use.
    pub(super) fn open_resolutions(&mut self, console: &ViewerConsole) {
        self.build_choices(console);
        let note = match self.applied_display(console) {
            DisplayMode::Windowed => "The window's size, grouped by aspect ratio.",
            DisplayMode::Borderless => "Used when windowed; borderless fills the desktop.",
            DisplayMode::Exclusive => "The monitor's video modes, by aspect ratio.",
        };
        // The card's page, so the size in use opens in mid view.
        self.picker.set_page(ROWS);
        self.picker
            .open(&self.choices, Self::current_resolution(console), note);
    }

    /// Step the Resolution row within its aspect-ratio group.
    pub(super) fn step_resolution(&mut self, console: &mut ViewerConsole, direction: i32) {
        self.build_choices(console);
        let current = Self::current_resolution(console);
        if let Some(size) = resolution::step(&self.choices, current, direction)
            && Some(size) != current
        {
            set_resolution(console, size);
        }
        self.refresh(console);
    }

    /// A key while the list is open.
    pub(super) fn resolution_key(
        &mut self,
        key: KeyCode,
        repeat: bool,
        console: &mut ViewerConsole,
    ) {
        let confirms = matches!(
            key,
            KeyCode::Enter
                | KeyCode::NumpadEnter
                | KeyCode::Space
                | KeyCode::Escape
                | KeyCode::Backspace
        );
        // A held Enter that opened the list must not pick from it too.
        if repeat && confirms {
            return;
        }
        if let PickResult::Close(Some(size)) = self.picker.key(key) {
            self.pick_resolution(console, size);
        }
    }

    /// Pointer while the list is open: hover highlights, a click picks, the
    /// wheel scrolls and the footer cap closes the list.
    pub(super) fn resolution_pointer(&mut self, event: InputEvent, console: &mut ViewerConsole) {
        let Some(event) = self.ui.pointer(event) else {
            return;
        };
        let Some(token) = event.token else {
            return;
        };
        let slot = usize::from(token);
        let on_row = slot < self.picker.page();
        match event.kind {
            UiEventKind::Wheel => {
                let notches = event.delta.map_or(0, |delta| -delta.y.signum() as isize);
                self.picker.scroll(notches);
            }
            UiEventKind::HoverEnter | UiEventKind::Hover if on_row => {
                self.picker.hover(self.picker.first() + slot);
            }
            UiEventKind::Activate if token == BACK_TOKEN => self.picker.close(),
            UiEventKind::Activate if on_row => {
                self.picker.hover(self.picker.first() + slot);
                if let PickResult::Close(Some(size)) = self.picker.pick() {
                    self.pick_resolution(console, size);
                }
            }
            _ => {}
        }
    }

    fn pick_resolution(&mut self, console: &mut ViewerConsole, size: [u32; 2]) {
        if Some(size) != Self::current_resolution(console) {
            set_resolution(console, size);
        }
        self.refresh(console);
    }

    /// Draw the open list at `reveal` opacity as the SJK UI's pop-up card
    /// over the darkened screen, whatever the menu style.
    pub(super) fn append_resolutions(
        &mut self,
        target: TextTarget<'_>,
        viewport: [f32; 2],
        reveal: f32,
    ) {
        let frame = Frame::new(viewport);
        let s = frame.s;
        self.picker.set_page(ROWS);
        let shown = self
            .picker
            .choices()
            .len()
            .saturating_sub(self.picker.first())
            .min(ROWS);
        let height = ROWS_TOP - CARD_TOP + shown.max(1) as f32 * ROW + MARGIN * 0.5;
        self.ui.begin_transparent(viewport);
        self.ui.push_opacity(reveal);
        kit::scrim(&mut self.ui, viewport);
        kit::card(&mut self.ui, &frame, [CARD_X, CARD_TOP, CARD_WIDTH, height]);
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("Resolution"),
            frame.rect(TEXT_X, CARD_TOP + 30.0, TEXT_WIDTH, 52.0),
            40.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{}", self.picker.note()),
            frame.rect(TEXT_X, CARD_TOP + 88.0, TEXT_WIDTH, 26.0),
            18.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        self.resolution_rows(&frame);
        self.resolution_keys(&frame, CARD_TOP + height + 28.0);
        self.ui.pop_opacity();
        let focus = self.picker.selected().saturating_sub(self.picker.first());
        self.ui.finish(focus as u16);
        target.append(&self.ui, viewport);
    }

    /// The visible rows, each the size, its aspect ratio and tags, the
    /// highlighted one on the band and the one in use in gold with its dot;
    /// row token `i` is list position `first + i`.
    fn resolution_rows(&mut self, frame: &Frame) {
        let s = frame.s;
        let (first, page) = (self.picker.first(), self.picker.page());
        let count = self.picker.choices().len();
        let current = self.picker.current();
        let right = CARD_X + CARD_WIDTH - MARGIN;
        for slot in 0..page.min(count.saturating_sub(first)) {
            let position = first + slot;
            let Some(&choice) = self.picker.choices().get(position) else {
                break;
            };
            let top = ROWS_TOP + slot as f32 * ROW;
            let middle = top + ROW * 0.5;
            let token = slot as u16;
            let selected = position == self.picker.selected();
            let in_use = Some(choice.size) == current;
            let band = [TEXT_X - 16.0, top, TEXT_WIDTH + 32.0, ROW];
            if selected {
                kit::band(&mut self.ui, frame, band);
            }
            let [w, h] = choice.size;
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{w} \u{d7} {h}"),
                frame.rect(TEXT_X + 8.0, middle - 15.0, SIZE_WIDTH, 30.0),
                23.0 * s,
                match (selected, in_use) {
                    (true, _) => color::TEXT,
                    (false, true) => color::GOLD_BRIGHT,
                    (false, false) => color::alpha(color::TEXT, 0.88),
                },
                FontWeight::Regular,
                TextAlign::Start,
            );
            let tag = match (in_use, choice.desktop) {
                (true, true) => "   \u{b7}   Desktop   \u{b7}   In use",
                (true, false) => "   \u{b7}   In use",
                (false, true) => "   \u{b7}   Desktop",
                (false, false) => "",
            };
            let tags_x = TEXT_X + 8.0 + SIZE_WIDTH;
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{}{tag}", choice.aspect),
                frame.rect(tags_x, middle - 12.0, right - DOT_ROOM - tags_x, 24.0),
                16.0 * s,
                if in_use {
                    color::GOLD_BRIGHT
                } else {
                    color::MUTED
                },
                FontWeight::Regular,
                TextAlign::End,
            );
            if in_use {
                self.resolution_shape(
                    [right - 10.0, middle - 4.0, 8.0, 8.0],
                    color::GOLD_BRIGHT,
                    frame,
                );
            }
            let [x, y, width, height] = band;
            self.ui.hit_region(token, frame.rect(x, y, width, height));
        }
        if count > page {
            // Where the view lies in the list: a track at the card's right
            // edge and its thumb.
            let track = [CARD_X + CARD_WIDTH - 20.0, ROWS_TOP, 4.0, page as f32 * ROW];
            let thumb = (page as f32 / count as f32).clamp(0.08, 1.0) * track[3];
            let travel = (track[3] - thumb) * first as f32 / (count - page) as f32;
            self.resolution_shape(track, color::alpha(color::HOLO, 0.12), frame);
            self.resolution_shape(
                [track[0], track[1] + travel, track[2], thumb],
                color::alpha(color::HOLO, 0.55),
                frame,
            );
        }
    }

    /// A small rounded shape over frame rectangle `rect`: the in-use dot, the
    /// scroll track and its thumb.
    fn resolution_shape(&mut self, rect: [f32; 4], colour: sjk_ui::Color, frame: &Frame) {
        let [x, y, width, height] = rect;
        let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: frame.rect(x, y, width, height),
            radius: width.min(height) * 0.5 * frame.s,
            color: colour,
        });
    }

    /// The keys' line under the card, centred, its top at `y`; Esc's cap is
    /// the pointer's way back.
    fn resolution_keys(&mut self, frame: &Frame, y: f32) {
        let s = frame.s;
        let width = KEYS
            .iter()
            .map(|(keys, action)| key_hint_width(keys, action, s) + KEY_GAP * s)
            .sum::<f32>()
            - KEY_GAP * s;
        let [centre, top] = frame.point(CARD_X + CARD_WIDTH * 0.5, y);
        let mut x = centre - width * 0.5;
        for (keys, action) in KEYS {
            let start = x;
            x = key_hint(&mut self.ui, keys, action, x, top, s);
            if keys == ["Esc"] {
                self.ui
                    .hit_region(BACK_TOKEN, Rect::new(start, top, x - start, 24.0 * s));
            }
            x += KEY_GAP * s;
        }
    }
}

#[cfg(test)]
impl SettingsMenu {
    /// Select the Resolution row and open its list, as Enter there does
    /// (world shots, tests).
    pub(crate) fn resolutions_for_shot(&mut self, console: &ViewerConsole) {
        self.select_cvar("r_resolution");
        self.open_resolutions(console);
    }
}

/// Write `size` to `r_resolution`.
fn set_resolution(console: &mut ViewerConsole, [width, height]: [u32; 2]) {
    console.set_cvar("r_resolution", &format!("{width}x{height}"));
}
