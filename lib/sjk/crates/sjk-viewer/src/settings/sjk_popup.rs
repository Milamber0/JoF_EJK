//! First setup as the SJK UI's pop-up (`docs/sjk-ui.md`, First setup): a card
//! over the darkened map with its name and what it is for, First setup's rows
//! scrolling inside it as Settings draws them ([`super::sjk_view`]), the
//! focused row's help under them, and at its foot, always in view, the "Don't
//! show at start" tick beside All settings and Done.
//!
//! The tick is First setup's own last row, pinned out of the scrolling lines
//! ([`super::ClassicRows`]): the keyboard reaches it after the last row, a
//! click flips it, and Backspace returns it to its default, as on any row.
//! Positions are pixels of the SJK UI's 16:9 frame ([`Frame`]).

use super::classic_view::switch_text_on;
use super::help;
use super::sjk_view::{CONTROL_LEFT, KEYS_Y, LINE, ROWS_TOP, ROWS_WIDTH, ROWS_X};
use super::*;
use crate::menu::sjk::{Frame, TextTarget, color, kit, text, wrap};
use crate::menu_widgets::TextFamily;
use sjk_ui::{DrawCommand, FontWeight, TextAlign};

/// The card.
const CARD: [f32; 4] = [480.0, 56.0, 960.0, 948.0];
/// The card's inner margin, and where its text starts.
const MARGIN: f32 = 48.0;
const TEXT_X: f32 = CARD[0] + MARGIN;
const TEXT_WIDTH: f32 = CARD[2] - MARGIN * 2.0;
/// Where Settings' rows column moves to sit in the card, under its header.
const SHIFT: [f32; 2] = [CARD[0] + 80.0 - ROWS_X, CARD[1] + 164.0 - ROWS_TOP];
/// Lines the card shows before its rows scroll.
const VISIBLE: usize = 11;
/// The focused row's help, under the rows.
const HELP_TOP: f32 = ROWS_TOP + SHIFT[1] + VISIBLE as f32 * LINE + 12.0;
/// The foot: its rule, and the middle of its tick and buttons.
const FOOT_RULE: f32 = CARD[1] + CARD[3] - 94.0;
const FOOT_MIDDLE: f32 = CARD[1] + CARD[3] - 47.0;
const BUTTON_HEIGHT: f32 = 46.0;
/// An open list reaches no lower than the help's first line.
const LIST_BOTTOM: f32 = FOOT_RULE - 10.0 - SHIFT[1];
/// The keys' line, under the card.
const KEYS_SHIFT: f32 = CARD[1] + CARD[3] + 28.0 - KEYS_Y;

/// The All settings button's target; Done is the way back's (900).
pub(super) const ALL_SETTINGS_TOKEN: u16 = 914;
const DONE_TOKEN: u16 = 900;

impl SettingsMenu {
    /// Draw First setup's pop-up at `reveal` opacity.
    pub(crate) fn append_sjk_popup(
        &mut self,
        target: TextTarget<'_>,
        viewport: [f32; 2],
        reveal: f32,
    ) {
        let frame = Frame::new(viewport);
        let rows = frame.shifted(SHIFT[0], SHIFT[1]);
        self.ui.begin_transparent(viewport);
        self.ui.push_opacity(reveal);
        kit::scrim(&mut self.ui, viewport);
        kit::card(&mut self.ui, &frame, CARD);
        self.popup_header(&frame);
        let measure = target.body_measure();
        let slots = self.sjk_rows(&rows, &measure, VISIBLE, LIST_BOTTOM);
        if self.dropdown.is_none() {
            self.popup_help(&frame);
        }
        self.popup_foot(&frame);
        self.sjk_keys(&frame.shifted(0.0, KEYS_SHIFT));
        self.sjk_dropdown(&rows, &slots, VISIBLE, LIST_BOTTOM);
        self.ui.pop_opacity();
        self.ui.finish(self.selected as u16);
        target.append(&self.ui, viewport);
        self.sjk_controls = Some(rows.point(CONTROL_LEFT, 0.0)[0]);
    }

    /// The card's name, what it is for and how to bring another client's
    /// settings over.
    fn popup_header(&mut self, frame: &Frame) {
        let s = frame.s;
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("First setup"),
            frame.rect(TEXT_X, CARD[1] + 30.0, TEXT_WIDTH, 52.0),
            40.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("The few settings worth choosing first. Settings has them all, any time."),
            frame.rect(TEXT_X, CARD[1] + 88.0, TEXT_WIDTH, 26.0),
            18.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!(
                "Coming from another client? Start with the first row: your setup, and the FPS your movement felt like."
            ),
            frame.rect(TEXT_X, CARD[1] + 116.0, TEXT_WIDTH, 24.0),
            15.0 * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    /// What the focused row does, in up to two lines under the rows.
    fn popup_help(&mut self, frame: &Frame) {
        let s = frame.s;
        let Some(help) = self
            .rows()
            .get(self.selected)
            .filter(|_| self.shows(self.selected))
            .and_then(|setting| help::help(setting.cvar))
        else {
            return;
        };
        for (index, line) in wrap(help, 96).take(2).enumerate() {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{line}"),
                frame.rect(
                    ROWS_X + SHIFT[0],
                    HELP_TOP + index as f32 * 24.0,
                    ROWS_WIDTH,
                    24.0,
                ),
                16.0 * s,
                color::alpha(color::TEXT, 0.72),
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
    }

    /// The foot under its rule: the pinned "Don't show at start" tick on the
    /// left, focused with the row's band, then All settings and Done on the
    /// right.
    fn popup_foot(&mut self, frame: &Frame) {
        let s = frame.s;
        let _ = self.ui.draw_list_mut().push(DrawCommand::SolidRect {
            rect: frame.rect(CARD[0] + 24.0, FOOT_RULE, CARD[2] - 48.0, 1.0),
            color: color::alpha(color::HOLO, 0.2),
        });
        let pinned = self.classic.as_ref().and_then(|classic| classic.pinned);
        if let Some(row) = pinned {
            let focused = row == self.selected;
            let ticked = self
                .values
                .get(row)
                .is_some_and(|value| switch_text_on(value));
            let label = self.rows().get(row).map_or("", |setting| {
                help::classic_label(setting.cvar, setting.label).0
            });
            let band = [CARD[0] + 24.0, FOOT_MIDDLE - 26.0, 400.0, 52.0];
            if focused {
                kit::band(&mut self.ui, frame, band);
            }
            let after = kit::tick(&mut self.ui, frame, TEXT_X, FOOT_MIDDLE, ticked, focused);
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{label}"),
                frame.rect(after + 14.0, FOOT_MIDDLE - 14.0, 330.0, 28.0),
                19.0 * s,
                if focused || ticked {
                    color::TEXT
                } else {
                    color::alpha(color::TEXT, 0.88)
                },
                FontWeight::Regular,
                TextAlign::Start,
            );
            let [x, y, width, height] = band;
            self.ui
                .hit_region(row as u16, frame.rect(x, y, width, height));
        }
        let right = CARD[0] + CARD[2] - MARGIN;
        let top = FOOT_MIDDLE - BUTTON_HEIGHT * 0.5;
        kit::button(
            &mut self.ui,
            frame,
            [right - 160.0, top, 160.0, BUTTON_HEIGHT],
            "Done",
            true,
            true,
            false,
            DONE_TOKEN,
        );
        kit::button(
            &mut self.ui,
            frame,
            [right - 160.0 - 14.0 - 170.0, top, 170.0, BUTTON_HEIGHT],
            "All settings",
            false,
            true,
            false,
            ALL_SETTINGS_TOKEN,
        );
    }
}

// The rows, their scrollbar, the help and the foot inside the card, in order.
const _: () = assert!(ROWS_X + SHIFT[0] >= CARD[0] + MARGIN);
const _: () = assert!(ROWS_X + SHIFT[0] + ROWS_WIDTH + 18.0 <= CARD[0] + CARD[2] - 24.0);
const _: () = assert!(HELP_TOP + 2.0 * 24.0 <= FOOT_RULE - 8.0);
const _: () = assert!(FOOT_MIDDLE + BUTTON_HEIGHT * 0.5 < CARD[1] + CARD[3]);
// The card and the keys under it inside the frame.
const _: () = assert!(CARD[0] + CARD[2] <= 1824.0 && CARD[1] + CARD[3] + 48.0 <= 1080.0);

#[cfg(test)]
mod tests {
    use super::*;

    fn popup() -> (tempfile::TempDir, ViewerConsole, SettingsMenu) {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        let mut menu = SettingsMenu::new();
        menu.open_first_setup_popup(&console);
        (directory, console, menu)
    }

    fn hide_row(menu: &SettingsMenu) -> usize {
        menu.rows()
            .iter()
            .position(|setting| setting.cvar == quick::HIDE_CVAR)
            .unwrap()
    }

    /// "Don't show at start" leaves the scrolling lines for the foot, and the
    /// keyboard still reaches it: up from the first row, and back down.
    #[test]
    fn the_hide_row_is_pinned_at_the_foot_and_the_keyboard_reaches_it() {
        let (_directory, _console, mut menu) = popup();
        assert!(menu.popup() && menu.on_first_setup());
        let hide = hide_row(&menu);
        let classic = menu.classic.as_ref().unwrap();
        assert!(!classic.lines.contains(&Line::Row(hide)));
        assert_eq!(classic.rows().last(), Some(hide));
        assert!(menu.shows(hide));
        assert_eq!(menu.selected, 0);
        menu.step_selection(-1);
        assert_eq!(menu.selected, hide);
        menu.step_selection(1);
        assert_eq!(menu.selected, 0);
    }

    /// The tick is the row: stepping it hides First setup at start, and
    /// Backspace's reset shows it again.
    #[test]
    fn the_tick_hides_first_setup_at_start() {
        let (_directory, mut console, mut menu) = popup();
        let hide = hide_row(&menu);
        menu.step_selection(-1);
        menu.adjust(&mut console, 1);
        assert_eq!(console.bool_cvar(quick::HIDE_CVAR), Some(true));
        assert!(switch_text_on(&menu.values[hide]));
        menu.reset_to_default(&mut console, hide);
        assert_eq!(console.bool_cvar(quick::HIDE_CVAR), Some(false));
    }

    #[test]
    fn tab_leaves_for_all_settings_and_the_search_keys_do_nothing() {
        assert!(matches!(
            popup_key(KeyCode::Tab),
            Some(SettingsResult::AllSettings)
        ));
        for key in [KeyCode::Slash, KeyCode::BracketLeft, KeyCode::BracketRight] {
            assert!(matches!(popup_key(key), Some(SettingsResult::None)));
        }
        assert!(popup_key(KeyCode::ArrowUp).is_none());
        // First setup's other views pin nothing.
        let (_directory, console, mut menu) = popup();
        menu.open_classic_group(
            &console,
            Group::Quick,
            crate::menu::classic::panel::Frame::Main,
        );
        assert!(!menu.popup() && menu.on_first_setup());
    }
}
