//! The Import page in the SJK UI (`docs/sjk-ui.md`, SJK's pages): a pop-up card
//! over the darkened map, as Update's and First setup's. Import and the file's
//! name over a headline; then the parts the file holds, one row each with its
//! switch, or the configs found from other clients (no file yet), what to do,
//! why nothing was imported, or what changed; then the actions along the card's
//! foot: Browse, and Import (gold) with Cancel. The gameplay FPS step is drawn
//! by `config_import_panel_fps.rs`. The state and its actions are the page's own.

use super::*;
use crate::menu::sjk::{Frame, TextTarget, color, key_hint, key_hint_width, kit, text, wrap};
use crate::menu_widgets::TextFamily;
use sjk_ui::TextAlign;

/// The card's left edge, top and width; it is as tall as what it holds.
const CARD_X: f32 = 540.0;
const CARD_TOP: f32 = 300.0;
const CARD_WIDTH: f32 = 840.0;
/// The card's inner margin, and its text's column.
const MARGIN: f32 = 44.0;
const TEXT_X: f32 = CARD_X + MARGIN;
const TEXT_WIDTH: f32 = CARD_WIDTH - MARGIN * 2.0;
/// The first line or row under the headline, and their steps.
const BODY_TOP: f32 = CARD_TOP + 138.0;
const LINE: f32 = 30.0;
const ROW: f32 = 56.0;
/// Most lines of text the card shows.
const LINES: usize = 5;
/// A part's value starts this far into its row.
const VALUE_X: f32 = TEXT_X + 250.0;
/// The buttons along the card's foot, this far under the body.
const BUTTONS_GAP: f32 = 36.0;
const BUTTON_HEIGHT: f32 = 46.0;
const BUTTON_GAP: f32 = 14.0;
/// The keys' line.
const KEYS_Y: f32 = 992.0;

/// The buttons a state offers: the left one, and the right ones from the right.
struct Buttons {
    left: Option<(&'static str, u16)>,
    /// The gold one Enter takes, and whether it can be taken now.
    primary: Option<(&'static str, u16, bool)>,
    close: Option<&'static str>,
}

impl Panel {
    /// Draw the page in the SJK UI.
    pub(crate) fn append_sjk(&mut self, target: TextTarget<'_>, viewport: [f32; 2]) {
        if matches!(self.state, State::Fps { .. }) {
            self.append_sjk_fps(target, viewport);
            return;
        }
        let frame = Frame::new(viewport);
        let s = frame.s;
        let (headline, lines) = self.sjk_words();
        let detected = matches!(self.state, State::Waiting) && !self.detected.is_empty();
        let body_height = match &self.state {
            State::Loaded(found) => {
                self.rows.len() as f32 * ROW + if found.unknown_keys > 0 { LINE } else { 0.0 }
            }
            _ if detected => self.detected.len() as f32 * ROW + LINE,
            _ => lines.len() as f32 * LINE,
        };
        let buttons_y = BODY_TOP + body_height + BUTTONS_GAP;
        let height = buttons_y + BUTTON_HEIGHT + MARGIN - CARD_TOP;
        self.ui.begin_transparent(viewport);
        kit::scrim(&mut self.ui, viewport);
        kit::card(&mut self.ui, &frame, [CARD_X, CARD_TOP, CARD_WIDTH, height]);
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("Import"),
            frame.rect(TEXT_X, CARD_TOP + 30.0, TEXT_WIDTH, 30.0),
            22.0 * s,
            color::GOLD_BRIGHT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{}", self.file),
            frame.rect(TEXT_X + 120.0, CARD_TOP + 33.0, TEXT_WIDTH - 120.0, 26.0),
            16.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::End,
        );
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{headline}"),
            frame.rect(TEXT_X, CARD_TOP + 70.0, TEXT_WIDTH, 52.0),
            40.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        if matches!(self.state, State::Loaded(_)) {
            self.sjk_rows(&frame);
        } else if detected {
            self.sjk_detected(&frame);
        } else {
            let mut y = BODY_TOP;
            for line in &lines {
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{line}"),
                    frame.rect(TEXT_X, y, TEXT_WIDTH, 28.0),
                    18.0 * s,
                    color::alpha(color::TEXT, 0.86),
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                y += LINE;
            }
        }
        let buttons = self.sjk_buttons_shown();
        self.sjk_buttons(&frame, &buttons, buttons_y);
        self.sjk_keys(&frame);
        self.ui.finish(self.selected as u16);
        target.append(&self.ui, viewport);
    }

    /// The headline, and the lines under it when no parts are listed.
    fn sjk_words(&self) -> (&'static str, Vec<String>) {
        let wrapped = |text: &str| -> Vec<String> { wrap(text, 76).map(str::to_owned).collect() };
        let (headline, mut lines) = match &self.state {
            State::Waiting if self.browsing.is_some() => (
                "Choose your config in the file window",
                wrapped(
                    "Pick the .cfg of the client you come from, for example \
                     GameData/base/jampconfig.cfg, or the one in your mod's folder.",
                ),
            ),
            State::Waiting if !self.detected.is_empty() => {
                ("Bring your setup from another client", Vec::new())
            }
            State::Waiting => (
                "Bring your setup from another client",
                wrapped(
                    "Your name, model, field of view and keys, from its .cfg: browse for \
                     it, for example GameData/base/jampconfig.cfg, or drop it on this window.",
                ),
            ),
            State::Loaded(_) => ("Choose what to copy", Vec::new()),
            State::Failed(reason) => {
                let mut lines = wrapped(reason);
                lines.extend(wrapped(
                    "Browse for another .cfg file, or drop one on this window.",
                ));
                ("Nothing imported", lines)
            }
            State::Done(done) if done.is_empty() => (
                "Nothing changed",
                wrapped("What you ticked was already your setup."),
            ),
            State::Done(done) => ("Imported", done.clone()),
            State::Fps { .. } => ("", Vec::new()),
        };
        lines.truncate(LINES);
        (headline, lines)
    }

    /// The parts the file holds, one row each: its name, what the file gives, and
    /// its switch; the selected one banded. A click on a row ticks it.
    fn sjk_rows(&mut self, frame: &Frame) {
        let State::Loaded(found) = &self.state else {
            return;
        };
        let s = frame.s;
        let right = CARD_X + CARD_WIDTH - MARGIN;
        let unknown = found.unknown_keys;
        let rows: Vec<(Item, bool, String)> = self
            .rows
            .iter()
            .map(|(item, ticked)| (*item, *ticked, Self::value(found, *item)))
            .collect();
        let mut top = BODY_TOP;
        for (row, (item, ticked, value)) in rows.into_iter().enumerate() {
            let focused = row == self.selected;
            let middle = top + ROW * 0.5;
            if focused {
                kit::band(
                    &mut self.ui,
                    frame,
                    [TEXT_X - 16.0, top, TEXT_WIDTH + 32.0, ROW],
                );
            }
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{}", item.label()),
                frame.rect(TEXT_X + 8.0, middle - 13.0, VALUE_X - TEXT_X - 16.0, 26.0),
                19.0 * s,
                if focused {
                    color::TEXT
                } else {
                    color::alpha(color::TEXT, 0.9)
                },
                FontWeight::Regular,
                TextAlign::Start,
            );
            let switch_left = kit::switch(&mut self.ui, frame, right, middle, ticked, focused);
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{value}"),
                frame.rect(VALUE_X, middle - 12.0, switch_left - VALUE_X - 20.0, 24.0),
                17.0 * s,
                if ticked { color::MUTED } else { color::QUIET },
                FontWeight::Regular,
                TextAlign::Start,
            );
            self.ui.hit_region(
                row as u16,
                frame.rect(TEXT_X - 16.0, top, TEXT_WIDTH + 32.0, ROW),
            );
            top += ROW;
        }
        if unknown > 0 {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{}", unknown_note(unknown)),
                frame.rect(TEXT_X + 8.0, top + 4.0, TEXT_WIDTH, 24.0),
                16.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
    }

    /// The configs other clients wrote, one row each: the client, then the name it
    /// holds and when it was saved; the selected one banded. A click reads it. A
    /// line under them says how to give another.
    fn sjk_detected(&mut self, frame: &Frame) {
        let s = frame.s;
        let rows: Vec<(String, String)> = self
            .detected
            .iter()
            .map(|config| {
                let saved = format!("saved {}", config.date);
                let value = match &config.name {
                    Some(name) => format!("{name}  -  {saved}"),
                    None => saved,
                };
                (config.client.clone(), value)
            })
            .collect();
        let mut top = BODY_TOP;
        for (row, (client, value)) in rows.into_iter().enumerate() {
            let focused = row == self.selected;
            let middle = top + ROW * 0.5;
            if focused {
                kit::band(
                    &mut self.ui,
                    frame,
                    [TEXT_X - 16.0, top, TEXT_WIDTH + 32.0, ROW],
                );
            }
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{client}"),
                frame.rect(TEXT_X + 8.0, middle - 13.0, 340.0, 26.0),
                19.0 * s,
                if focused {
                    color::TEXT
                } else {
                    color::alpha(color::TEXT, 0.9)
                },
                FontWeight::Regular,
                TextAlign::Start,
            );
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{value}"),
                frame.rect(TEXT_X + 360.0, middle - 12.0, TEXT_WIDTH - 368.0, 24.0),
                17.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::End,
            );
            self.ui.hit_region(
                row as u16,
                frame.rect(TEXT_X - 16.0, top, TEXT_WIDTH + 32.0, ROW),
            );
            top += ROW;
        }
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Not here? Browse for its .cfg, or drop it on this window."),
            frame.rect(TEXT_X + 8.0, top + 4.0, TEXT_WIDTH, 24.0),
            16.0 * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    fn sjk_buttons_shown(&self) -> Buttons {
        match &self.state {
            State::Waiting if !self.detected.is_empty() => Buttons {
                left: Some(("Browse...", BROWSE_TOKEN)),
                primary: Some(("Import", IMPORT_TOKEN, true)),
                close: Some("Close"),
            },
            State::Waiting | State::Failed(_) => Buttons {
                left: None,
                primary: Some(("Browse...", BROWSE_TOKEN, self.browsing.is_none())),
                close: Some("Close"),
            },
            State::Loaded(_) => Buttons {
                left: Some(("Other file...", BROWSE_TOKEN)),
                primary: Some(("Import", IMPORT_TOKEN, self.chosen().is_some())),
                close: Some("Cancel"),
            },
            State::Done(_) | State::Fps { .. } => Buttons {
                left: None,
                primary: Some(("Done", IMPORT_TOKEN, true)),
                close: None,
            },
        }
    }

    /// The actions along the card's foot at `y`: the left one from the left, the
    /// one Enter takes (gold) and Close from the right.
    fn sjk_buttons(&mut self, frame: &Frame, buttons: &Buttons, y: f32) {
        // Rajdhani at 20 is about 9.5 pixels a character.
        let width = |label: &str| (48.0 + 9.5 * label.chars().count() as f32).max(140.0);
        if let Some((label, token)) = buttons.left {
            let enabled = self.browsing.is_none();
            kit::button(
                &mut self.ui,
                frame,
                [TEXT_X, y, width(label), BUTTON_HEIGHT],
                label,
                false,
                enabled,
                false,
                token,
            );
        }
        let mut right = CARD_X + CARD_WIDTH - MARGIN;
        if let Some((label, token, enabled)) = buttons.primary {
            let w = width(label);
            right -= w;
            kit::button(
                &mut self.ui,
                frame,
                [right, y, w, BUTTON_HEIGHT],
                label,
                true,
                enabled,
                true,
                token,
            );
            right -= BUTTON_GAP;
        }
        if let Some(label) = buttons.close {
            let w = width(label);
            kit::button(
                &mut self.ui,
                frame,
                [right - w, y, w, BUTTON_HEIGHT],
                label,
                false,
                true,
                false,
                BACK_TOKEN,
            );
        }
    }

    /// The page's keys, right-aligned at the bottom.
    fn sjk_keys(&mut self, frame: &Frame) {
        let s = frame.s;
        let keys: &[(&[&str], &str)] = match &self.state {
            State::Waiting if !self.detected.is_empty() => &[
                (&["Enter"], "import"),
                (&["B"], "browse"),
                (&["Esc"], "close"),
            ],
            State::Waiting | State::Failed(_) => &[(&["Enter"], "browse"), (&["Esc"], "close")],
            State::Loaded(_) => &[
                (&["Enter"], "import"),
                (&["Space"], "tick"),
                (&["B"], "other file"),
                (&["Esc"], "cancel"),
            ],
            State::Done(_) | State::Fps { .. } => &[(&["Enter"], "done")],
        };
        let gap = 30.0 * s;
        let width: f32 = keys
            .iter()
            .map(|(caps, action)| key_hint_width(caps, action, s))
            .sum::<f32>()
            + gap * keys.len().saturating_sub(1) as f32;
        let [right, y] = frame.point(1_824.0, KEYS_Y);
        let mut x = right - width;
        for (caps, action) in keys {
            x = key_hint(&mut self.ui, caps, action, x, y, s) + gap;
        }
    }
}

// At its tallest (every part and the unknown keys' note, every config found and
// its note, or every line) the card ends above the keys.
const _: () =
    assert!(BODY_TOP + 5.0 * ROW + LINE + BUTTONS_GAP + BUTTON_HEIGHT + MARGIN < KEYS_Y - 20.0);
const _: () = assert!(crate::config_import::MAX_DETECTED <= 5);
const _: () =
    assert!(BODY_TOP + LINES as f32 * LINE + BUTTONS_GAP + BUTTON_HEIGHT + MARGIN < KEYS_Y - 20.0);
