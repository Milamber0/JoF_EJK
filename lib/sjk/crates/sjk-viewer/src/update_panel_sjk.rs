//! Update in the SJK UI (`docs/sjk-ui.md`, SJK's pages): a pop-up card
//! over the darkened map, as First setup's. Update and the version this is
//! over what the update check found as a headline and its detail, the
//! download's gold progress, then the actions along the card's foot: Release
//! notes and Check again on the left, Close and the one Enter takes, gold, on
//! the right. The state and its actions are the page's own.

use super::*;
use crate::menu::sjk::{Frame, TextTarget, color, key_hint, key_hint_width, kit, text, wrap};
use crate::menu_widgets::TextFamily;
use sjk_ui::{FontWeight, TextAlign};

/// The card's left edge, top and width; it is as tall as what it says.
const CARD_X: f32 = 540.0;
const CARD_TOP: f32 = 340.0;
const CARD_WIDTH: f32 = 840.0;
/// The card's inner margin, and its text's column.
const MARGIN: f32 = 44.0;
const TEXT_X: f32 = CARD_X + MARGIN;
const TEXT_WIDTH: f32 = CARD_WIDTH - MARGIN * 2.0;
/// The detail's first line, its line step and most lines.
const DETAIL_TOP: f32 = CARD_TOP + 138.0;
const DETAIL_LINE: f32 = 30.0;
const DETAIL_LINES: usize = 4;
/// The progress bar's track, and its place under the detail.
const TRACK_WIDTH: f32 = 620.0;
const TRACK_GAP: f32 = 20.0;
/// The buttons along the card's foot, this far under the text.
const BUTTONS_GAP: f32 = 40.0;
const BUTTON_HEIGHT: f32 = 46.0;
const BUTTON_GAP: f32 = 14.0;
/// The keys' line.
const KEYS_Y: f32 = 992.0;

impl Panel {
    /// Draw the page in the SJK UI.
    pub(crate) fn append_sjk(&mut self, target: TextTarget<'_>, viewport: [f32; 2]) {
        let frame = Frame::new(viewport);
        let s = frame.s;
        let installed = crate::build_info::VERSION;
        let view = view(&update::state(), installed);
        let detail: Vec<&str> = wrap(&view.detail, 76).take(DETAIL_LINES).collect();
        let text_end = DETAIL_TOP
            + detail.len() as f32 * DETAIL_LINE
            + view.progress.map_or(0.0, |_| TRACK_GAP + 6.0);
        let buttons_y = text_end + BUTTONS_GAP;
        let height = buttons_y + BUTTON_HEIGHT + MARGIN - CARD_TOP;
        self.ui.begin_transparent(viewport);
        kit::scrim(&mut self.ui, viewport);
        kit::card(&mut self.ui, &frame, [CARD_X, CARD_TOP, CARD_WIDTH, height]);
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("Update"),
            frame.rect(TEXT_X, CARD_TOP + 30.0, TEXT_WIDTH, 30.0),
            22.0 * s,
            color::GOLD_BRIGHT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("This is SJK {installed}"),
            frame.rect(TEXT_X, CARD_TOP + 33.0, TEXT_WIDTH, 26.0),
            16.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::End,
        );
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{}", view.headline),
            frame.rect(TEXT_X, CARD_TOP + 70.0, TEXT_WIDTH, 52.0),
            40.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let mut y = DETAIL_TOP;
        for line in detail {
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
            y += DETAIL_LINE;
        }
        if let Some(progress) = view.progress {
            y += TRACK_GAP;
            let track = frame.rect(TEXT_X, y, TRACK_WIDTH, 6.0);
            let _ = self
                .ui
                .draw_list_mut()
                .push(sjk_ui::DrawCommand::RoundedRect {
                    rect: track,
                    radius: track.height * 0.5,
                    color: color::alpha(color::HOLO, 0.2),
                });
            let filled = frame.rect(TEXT_X, y, TRACK_WIDTH * progress, 6.0);
            let _ = self
                .ui
                .draw_list_mut()
                .push(sjk_ui::DrawCommand::RoundedRect {
                    rect: filled,
                    radius: filled.height * 0.5,
                    color: color::GOLD,
                });
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{:.0} %", progress * 100.0),
                frame.rect(TEXT_X + TRACK_WIDTH + 20.0, y - 13.0, 100.0, 30.0),
                22.0 * s,
                color::GOLD_BRIGHT,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        self.sjk_buttons(&frame, &view, buttons_y);
        self.sjk_keys(&frame, &view);
        self.ui.finish(PRIMARY_TOKEN);
        target.append(&self.ui, viewport);
    }

    /// The actions along the card's foot at `y`: Release notes and Check
    /// again from the left, the one Enter takes (gold) and Close from the right.
    fn sjk_buttons(&mut self, frame: &Frame, view: &View, y: f32) {
        // Rajdhani at 20 is about 9.5 pixels a character.
        let width = |label: &str| (48.0 + 9.5 * label.chars().count() as f32).max(140.0);
        let mut left = TEXT_X;
        for (shown, label, token) in [
            (view.notes, "Release notes", NOTES_TOKEN),
            (view.check, "Check again", CHECK_TOKEN),
        ] {
            if shown {
                let w = width(label);
                kit::button(
                    &mut self.ui,
                    frame,
                    [left, y, w, BUTTON_HEIGHT],
                    label,
                    false,
                    true,
                    false,
                    token,
                );
                left += w + BUTTON_GAP;
            }
        }
        let mut right = CARD_X + CARD_WIDTH - MARGIN;
        if let Some(primary) = view.primary {
            let w = width(primary);
            right -= w;
            kit::button(
                &mut self.ui,
                frame,
                [right, y, w, BUTTON_HEIGHT],
                primary,
                true,
                true,
                true,
                PRIMARY_TOKEN,
            );
            right -= BUTTON_GAP;
        }
        let w = width("Close");
        kit::button(
            &mut self.ui,
            frame,
            [right - w, y, w, BUTTON_HEIGHT],
            "Close",
            false,
            true,
            false,
            BACK_TOKEN,
        );
    }

    /// The page's keys, right-aligned at the bottom.
    fn sjk_keys(&mut self, frame: &Frame, view: &View) {
        let s = frame.s;
        let mut keys: Vec<(&[&str], &str)> = Vec::with_capacity(4);
        if let Some(primary) = view.primary {
            keys.push((&["Enter"], primary));
        }
        if view.check {
            keys.push((&["C"], "check again"));
        }
        if view.notes {
            keys.push((&["N"], "release notes"));
        }
        keys.push((&["Esc"], "close"));
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

// At its tallest (every line of detail and the progress bar) the card ends
// above the keys; the bar's percentage fits beside it.
const _: () = assert!(
    DETAIL_TOP
        + DETAIL_LINES as f32 * DETAIL_LINE
        + TRACK_GAP
        + 6.0
        + BUTTONS_GAP
        + BUTTON_HEIGHT
        + MARGIN
        < KEYS_Y - 20.0
);
const _: () = assert!(TEXT_X + TRACK_WIDTH + 20.0 + 60.0 <= CARD_X + CARD_WIDTH - MARGIN);
