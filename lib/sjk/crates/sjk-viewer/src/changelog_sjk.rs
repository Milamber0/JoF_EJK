//! What's new in the SJK UI (`docs/sjk-ui.md`, SJK's pages): the releases
//! down a lit rail on the left, newest first, the chosen one's notes in a
//! reading column beside them (its date and size, its name, its introduction,
//! then each change after a gold dot with its credit under it), over the map.
//! The release list, scroll and keys are the page's own; only the drawing
//! differs.

use super::*;
use crate::inline_code::{self, Look, Measures, Place};
use crate::menu::sjk::{Frame, TextTarget, color, key_hint, key_hint_width, kit, text};
use crate::menu_widgets::TextFamily;

/// The releases' rail: its line, the first release's top and the step.
const RAIL_X: f32 = 96.0;
const RAIL_TOP: f32 = 200.0;
const RELEASE_STEP: f32 = 64.0;
const RELEASE_HEIGHT: f32 = 56.0;
const RAIL_WIDTH: f32 = 330.0;
/// The reading column, the notes' first line and where they stop.
const NOTES_X: f32 = 520.0;
const NOTES_WIDTH: f32 = 960.0;
const NOTES_TOP: f32 = 190.0;
const NOTES_BOTTOM: f32 = 950.0;
/// The notes' text and their chips (`inline_code`).
const NOTES_LOOK: Look = Look {
    size: 19.0,
    line: 30.0,
    chip_size: 17.5,
    chip_pad: 7.0,
    chip_height: 25.0,
    chip_radius: 6.0,
};
/// A change's text starts after its dot.
const INDENT: f32 = 26.0;
/// The keys' line.
const KEYS_Y: f32 = 992.0;

impl LineKind {
    /// A line's height in the SJK UI's notes.
    fn sjk_height(self) -> f32 {
        match self {
            Self::Intro | Self::Change | Self::More => 30.0,
            Self::Credit => 28.0,
            Self::Gap => 16.0,
        }
    }
}

impl Panel {
    /// Draw the page in the SJK UI.
    pub(crate) fn append_sjk(&mut self, target: TextTarget<'_>, viewport: [f32; 2]) {
        let measures = target.inline_measures();
        let frame = Frame::new(viewport);
        let s = frame.s;
        self.ui.begin_transparent(viewport);
        crate::settings::sjk_view::backdrop(&mut self.ui, viewport);
        let [x, y] = frame.point(RAIL_X, 75.0);
        let end = key_hint(&mut self.ui, &["Esc"], "Back", x, y, s);
        self.ui
            .hit_region(BACK_TOKEN, Rect::new(x, y, end - x, 24.0 * s));
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("What's new"),
            frame.rect((end - frame.origin[0]) / s + 22.0, 57.0, 600.0, 60.0),
            48.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        self.sjk_releases(&frame);
        self.sjk_notes(&frame, &measures);
        self.sjk_keys(&frame);
        let selected = self.selected.saturating_sub(self.first);
        self.ui
            .finish(ROW_BASE + selected.min(ROW_LIMIT - 1) as u16);
        target.append(&self.ui, viewport);
    }

    /// The releases down the rail, the chosen one lit.
    fn sjk_releases(&mut self, frame: &Frame) {
        let s = frame.s;
        self.rows =
            (((NOTES_BOTTOM - RAIL_TOP) / RELEASE_STEP).floor().max(1.0) as usize).min(ROW_LIMIT);
        self.first = self
            .first
            .min(self.releases.len().saturating_sub(self.rows));
        let shown = self.first..self.releases.len().min(self.first + self.rows);
        let bottom = RAIL_TOP + shown.len() as f32 * RELEASE_STEP;
        let lit = shown.contains(&self.selected).then(|| {
            RAIL_TOP + (self.selected - self.first) as f32 * RELEASE_STEP + RELEASE_HEIGHT * 0.5
        });
        kit::rail(
            &mut self.ui,
            frame,
            RAIL_X,
            RAIL_TOP - 16.0,
            bottom + 8.0,
            lit,
        );
        for (slot, index) in shown.enumerate() {
            let top = RAIL_TOP + slot as f32 * RELEASE_STEP;
            let token = ROW_BASE + slot as u16;
            let chosen = index == self.selected;
            let hovered = self.ui.token_hovered(token);
            let release = &self.releases[index];
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}", release.title),
                frame.rect(RAIL_X + 28.0, top + 2.0, RAIL_WIDTH - 28.0, 30.0),
                24.0 * s,
                match (chosen, hovered) {
                    (true, _) => color::GOLD_BRIGHT,
                    (false, true) => color::TEXT,
                    (false, false) => color::MUTED,
                },
                FontWeight::Regular,
                TextAlign::Start,
            );
            let unreleased = release.date.is_empty();
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!(
                    "{}",
                    if unreleased {
                        "Not released yet"
                    } else {
                        release.date.as_str()
                    }
                ),
                frame.rect(RAIL_X + 28.0, top + 32.0, RAIL_WIDTH - 28.0, 22.0),
                15.0 * s,
                if unreleased {
                    color::GOLD
                } else {
                    color::QUIET
                },
                FontWeight::Regular,
                TextAlign::Start,
            );
            self.ui
                .hit_region(token, frame.rect(RAIL_X, top, RAIL_WIDTH, RELEASE_HEIGHT));
        }
    }

    /// The chosen release's notes in the reading column, scrolled.
    fn sjk_notes(&mut self, frame: &Frame, measures: &Measures<'_>) {
        let s = frame.s;
        if let Some(error) = self.error.clone() {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{error}"),
                frame.rect(NOTES_X, NOTES_TOP, NOTES_WIDTH, 30.0),
                19.0 * s,
                color::EMBER,
                FontWeight::Regular,
                TextAlign::Start,
            );
            return;
        }
        let Some(release) = self.releases.get(self.selected) else {
            return;
        };
        let changes = release.changes.len();
        let when = if release.date.is_empty() {
            "Not released yet".to_owned()
        } else {
            release.date.clone()
        };
        let title = release.title.clone();
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!(
                "{when}, {changes} change{}",
                if changes == 1 { "" } else { "s" }
            ),
            frame.rect(NOTES_X, NOTES_TOP, NOTES_WIDTH, 24.0),
            17.0 * s,
            color::GOLD_BRIGHT,
            FontWeight::Regular,
            TextAlign::Start,
        );
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{title}"),
            frame.rect(NOTES_X, NOTES_TOP + 28.0, NOTES_WIDTH, 60.0),
            48.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let top = NOTES_TOP + 108.0;
        self.wrap_sjk(measures);
        let available = (NOTES_BOTTOM - top) * s;
        self.fit_scroll(available, |kind| kind.sjk_height() * s);
        self.ui.scroll_region(
            PANE_TOKEN,
            frame.rect(NOTES_X, top, NOTES_WIDTH, NOTES_BOTTOM - top),
        );
        let mut y = top;
        let mut shown = 0_usize;
        self.chips.clear();
        let mut chip_token = CHIP_BASE;
        for line in &self.lines[self.scroll..] {
            let height = line.kind.sjk_height();
            if y + height > NOTES_BOTTOM + 0.5 {
                break;
            }
            shown += 1;
            self.chips
                .extend(inline_code::chips(std::slice::from_ref(&line.row)).map(str::to_owned));
            match line.kind {
                LineKind::Gap => {}
                LineKind::Intro => inline_code::draw_rows(
                    &mut self.ui,
                    frame,
                    std::slice::from_ref(&line.row),
                    &NOTES_LOOK,
                    &Place {
                        x: NOTES_X,
                        y,
                        colour: color::MUTED,
                        code_family: TextFamily::Code,
                    },
                    Some(&mut chip_token),
                ),
                LineKind::Change | LineKind::More => {
                    if line.kind == LineKind::Change {
                        let dot = frame.rect(NOTES_X + 4.0, y + 11.0, 8.0, 8.0);
                        let _ = self
                            .ui
                            .draw_list_mut()
                            .push(sjk_ui::DrawCommand::RoundedRect {
                                rect: dot,
                                radius: dot.width * 0.5,
                                color: color::GOLD,
                            });
                    }
                    inline_code::draw_rows(
                        &mut self.ui,
                        frame,
                        std::slice::from_ref(&line.row),
                        &NOTES_LOOK,
                        &Place {
                            x: NOTES_X + INDENT,
                            y,
                            colour: color::alpha(color::TEXT, 0.92),
                            code_family: TextFamily::Code,
                        },
                        Some(&mut chip_token),
                    );
                }
                LineKind::Credit => text(
                    &mut self.ui,
                    TextFamily::Display,
                    format_args!("{}", line.text),
                    frame.rect(NOTES_X + INDENT, y + 1.0, NOTES_WIDTH - INDENT, height),
                    17.0 * s,
                    color::alpha(color::GOLD, 0.85),
                    FontWeight::Regular,
                    TextAlign::Start,
                ),
            }
            y += height;
        }
        self.page_lines = shown.saturating_sub(2).max(1);
        if self.max_scroll > 0 {
            self.ui.scrollbar(
                PANE_BAR_TOKEN,
                frame.rect(NOTES_X + NOTES_WIDTH + 24.0, top, 4.0, NOTES_BOTTOM - top),
                self.scroll,
                shown,
                self.lines.len(),
            );
        }
    }

    /// Wrap the chosen release for the reading column as `measures` draw it, its code
    /// in chips; credits keep their own case.
    fn wrap_sjk(&mut self, measures: &Measures<'_>) {
        // Keyed apart from the classic+ pane's measured wrap.
        let key = (self.selected, u32::MAX, measures.key(&NOTES_LOOK));
        if self.wrapped_for == Some(key) {
            return;
        }
        self.wrapped_for = Some(key);
        self.lines.clear();
        let Some(release) = self.releases.get(self.selected) else {
            return;
        };
        let line = |kind, row| Line {
            kind,
            text: String::new(),
            row,
        };
        for paragraph in &release.intro {
            for row in inline_code::flow(paragraph, NOTES_WIDTH, measures, &NOTES_LOOK) {
                self.lines.push(line(LineKind::Intro, row));
            }
            self.lines.push(line(LineKind::Gap, Vec::new()));
        }
        for change in &release.changes {
            let rows = inline_code::flow(&change.text, NOTES_WIDTH - INDENT, measures, &NOTES_LOOK);
            for (index, row) in rows.into_iter().enumerate() {
                let kind = if index == 0 {
                    LineKind::Change
                } else {
                    LineKind::More
                };
                self.lines.push(line(kind, row));
            }
            if !change.credit.is_empty() {
                self.lines.push(Line {
                    kind: LineKind::Credit,
                    text: change.credit.clone(),
                    row: Vec::new(),
                });
            }
            self.lines.push(line(LineKind::Gap, Vec::new()));
        }
        self.lines.pop();
    }

    /// The page's keys, right-aligned at the bottom.
    fn sjk_keys(&mut self, frame: &Frame) {
        let s = frame.s;
        let keys: [(&[&str], &str); 3] = [
            (&["Up", "Down"], "release"),
            (&["Page up", "Page down"], "scroll"),
            (&["Esc"], "back"),
        ];
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
