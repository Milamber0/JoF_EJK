//! Identity in the SJK UI (`docs/sjk-ui.md`, SJK's pages): the identity's
//! state as a headline over its lines, the switch that shares it, the bio and
//! the page's buttons, then the known players here, over the map. The state,
//! focus, typing and actions are the page's own; only the drawing differs.

use super::*;
use crate::menu::sjk::{Frame, TextTarget, color, key_hint, key_hint_width, kit, text, wrap};
use crate::menu_widgets::TextFamily;

/// The page's column.
const COLUMN_X: f32 = 520.0;
const COLUMN_WIDTH: f32 = 900.0;
const TOP: f32 = 190.0;
/// Characters a line of the column holds (Exo 2 at 18).
const LINE_CHARS: usize = 90;
/// The keys' line.
const KEYS_Y: f32 = 992.0;

impl Panel {
    /// Draw the page in the SJK UI with what `inputs` says.
    pub(crate) fn append_sjk(
        &mut self,
        inputs: &Inputs<'_>,
        target: TextTarget<'_>,
        viewport: [f32; 2],
    ) {
        self.sync(inputs);
        let view = view(inputs, self.revealed);
        let frame = Frame::new(viewport);
        let s = frame.s;
        self.ui.begin_transparent(viewport);
        crate::settings::sjk_view::backdrop(&mut self.ui, viewport);
        let [x, y] = frame.point(96.0, 75.0);
        let end = key_hint(&mut self.ui, &["Esc"], "Back", x, y, s);
        self.ui
            .hit_region(BACK_TOKEN, Rect::new(x, y, end - x, 24.0 * s));
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("Identity"),
            frame.rect((end - frame.origin[0]) / s + 22.0, 57.0, 600.0, 60.0),
            48.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{}", view.headline),
            frame.rect(COLUMN_X, TOP, COLUMN_WIDTH, 64.0),
            52.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let mut y = TOP + 76.0;
        for line in &view.lines {
            for part in wrap(line, LINE_CHARS) {
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{part}"),
                    frame.rect(COLUMN_X, y, COLUMN_WIDTH, 28.0),
                    18.0 * s,
                    color::MUTED,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                y += 28.0;
            }
        }
        // The switch.
        y += 18.0;
        let row = [COLUMN_X, y, COLUMN_WIDTH, 56.0];
        let focused = self.focus == Focus::Toggle;
        if focused {
            kit::band(&mut self.ui, &frame, row);
        }
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Share my identity with the SJK hub"),
            frame.rect(COLUMN_X + 22.0, y + 14.0, 600.0, 28.0),
            19.0 * s,
            if focused {
                Color::new(1.0, 1.0, 1.0, 1.0)
            } else {
                color::alpha(color::TEXT, 0.88)
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        kit::switch(
            &mut self.ui,
            &frame,
            COLUMN_X + COLUMN_WIDTH - 10.0,
            y + 28.0,
            self.enabled,
            focused,
        );
        self.ui
            .hit_region(TOGGLE_TOKEN, frame.rect(row[0], row[1], row[2], row[3]));
        y += 72.0;
        if self.fields {
            y = self.sjk_bio(&frame, y);
        }
        self.sjk_players(&frame, &view, y);
        if let Some(medals) = &view.medals {
            self.sjk_medals(&frame, medals, TOP, KEYS_Y - 24.0);
        }
        self.sjk_keys(&frame);
        self.ui.finish(self.focus.token());
        target.append(&self.ui, viewport);
    }

    /// The bio's field and the page's buttons from `y`; returns the y after them.
    fn sjk_bio(&mut self, frame: &Frame, y: f32) -> f32 {
        let s = frame.s;
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("About you, optional, up to {BIO_LIMIT} characters"),
            frame.rect(COLUMN_X, y, COLUMN_WIDTH, 24.0),
            16.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        let field = [COLUMN_X, y + 30.0, COLUMN_WIDTH, 46.0];
        let focused = self.focus == Focus::Bio;
        let caret = focused && (self.epoch.elapsed().as_millis() / 500).is_multiple_of(2);
        if self.bio.is_empty() && !focused {
            kit::field(
                &mut self.ui,
                frame,
                field,
                format_args!("Say something about yourself"),
                false,
                false,
            );
        } else {
            let shown = fit_tail(&self.bio, 92);
            kit::field(
                &mut self.ui,
                frame,
                field,
                format_args!("{shown}{}", if caret { "|" } else { "" }),
                focused,
                false,
            );
        }
        self.ui.hit_region(
            BIO_TOKEN,
            frame.rect(field[0], field[1], field[2], field[3]),
        );
        let top = y + 96.0;
        let mut x = COLUMN_X;
        let copied = self
            .copied_until
            .is_some_and(|until| Instant::now() < until);
        let mut buttons: Vec<(&str, bool, u16, Focus)> = vec![
            ("Save", true, SAVE_TOKEN, Focus::Save),
            (
                if copied { "Copied" } else { "Copy my key id" },
                false,
                COPY_TOKEN,
                Focus::Copy,
            ),
            (
                if self.revealed {
                    "Hide key"
                } else {
                    "Show key"
                },
                false,
                REVEAL_TOKEN,
                Focus::Reveal,
            ),
        ];
        if self.offer_hub {
            buttons.push(("Use the official hub", false, HUB_TOKEN, Focus::Hub));
        }
        for (label, primary, token, focus) in buttons {
            // Rajdhani at 20 is about 9.5 pixels a character.
            let width = (48.0 + 9.5 * label.chars().count() as f32).max(140.0);
            kit::button(
                &mut self.ui,
                frame,
                [x, top, width, 48.0],
                label,
                primary,
                true,
                self.focus == focus,
                token,
            );
            x += width + 14.0;
        }
        if !self.message.is_empty() {
            let message = self.message.clone();
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{message}"),
                frame.rect(x + 8.0, top + 12.0, COLUMN_X + COLUMN_WIDTH - x - 8.0, 26.0),
                17.0 * s,
                color::EMBER,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        top + 72.0
    }

    /// The known players here, under a sub-heading, as many as fit.
    fn sjk_players(&mut self, frame: &Frame, view: &View, y: f32) {
        let s = frame.s;
        if view.players.is_empty() {
            return;
        }
        kit::heading(
            &mut self.ui,
            frame,
            COLUMN_X,
            y + 20.0,
            COLUMN_WIDTH,
            "Known players here",
        );
        let mut row_y = y + 42.0;
        for player in &view.players {
            if row_y + 34.0 > KEYS_Y - 24.0 {
                break;
            }
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}", player.slot),
                frame.rect(COLUMN_X, row_y, 40.0, 30.0),
                20.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::End,
            );
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{}", player.name),
                frame.rect(COLUMN_X + 60.0, row_y, COLUMN_WIDTH - 200.0, 30.0),
                19.0 * s,
                color::TEXT,
                FontWeight::Regular,
                TextAlign::Start,
            );
            if player.verified {
                text(
                    &mut self.ui,
                    TextFamily::Display,
                    format_args!("Verified"),
                    frame.rect(COLUMN_X + COLUMN_WIDTH - 140.0, row_y, 130.0, 30.0),
                    19.0 * s,
                    color::GOLD_BRIGHT,
                    FontWeight::Regular,
                    TextAlign::End,
                );
            }
            row_y += 34.0;
        }
    }

    /// The page's keys, right-aligned at the bottom.
    fn sjk_keys(&mut self, frame: &Frame) {
        let s = frame.s;
        let enter = match self.focus {
            Focus::Toggle => "switch",
            Focus::Bio | Focus::Save => "save",
            Focus::Copy => "copy",
            Focus::Hub => "use",
            Focus::Reveal if self.revealed => "hide the key",
            Focus::Reveal => "show the key",
        };
        let keys: [(&[&str], &str); 3] =
            [(&["Tab"], "next"), (&["Enter"], enter), (&["Esc"], "back")];
        let gap = 30.0 * s;
        let width: f32 = keys
            .iter()
            .map(|(caps, action)| key_hint_width(caps, action, s))
            .sum::<f32>()
            + gap * (keys.len() - 1) as f32;
        let [right, y] = frame.point(1_824.0, KEYS_Y);
        let mut x = right - width;
        for (caps, action) in keys {
            x = key_hint(&mut self.ui, caps, action, x, y, s) + gap;
        }
    }
}
