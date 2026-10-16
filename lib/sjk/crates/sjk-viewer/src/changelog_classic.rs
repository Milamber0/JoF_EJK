//! The changelog page in the classic+ style (`docs/classic-plus.md`), drawn while
//! the menus are classic (`ui_menuStyle classic`). It takes the frame of the
//! classic+ command browser: the `menu_box_ingame` pop-up over the dimmed screen,
//! the `menu_blendbox` title band, a retail list box of releases whose selected
//! row sits on `menu_blendbox2`, a detail box with the selected release, a gold
//! Close button and the description line under the box. Every control answers
//! to the page's tokens, so the keyboard, pointer and wheel serve both looks.

use super::{BACK_TOKEN, LineKind, PANE_BAR_TOKEN, PANE_TOKEN, Panel, ROW_BASE, ROW_LIMIT};
use crate::console::browser::classic::{
    FRAME, LABEL, LIST_BACK, LIST_BORDER, OPTION, VALUE, border, fill, text, thumb, with_alpha,
};
use crate::menu::art::ArtPiece;
use crate::menu::classic::layout::Placement;
use crate::menu::classic::view::{self, Caps, FOCUS, GOLD, HINT};
use crate::text::{TextVertex, UiFont};
use sjk_ui::{FontWeight, Rect, TextAlign};

/// The pop-up box and its title band (canvas units), as the classic+ browser's.
const BOX: [f32; 4] = [35.0, 26.0, 570.0, 414.0];
const TITLE: [f32; 4] = [55.0, 32.0, 530.0, 22.0];
/// The release list box, and the height of one release row.
const LIST: [f32; 4] = [55.0, 62.0, 150.0, 344.0];
const ROW_HEIGHT: f32 = 30.0;
/// Release rows the list box holds.
const LIST_ROWS: usize = 11;
/// The detail box for the selected release.
const DETAIL: [f32; 4] = [213.0, 62.0, 372.0, 344.0];
/// Inner padding of the detail box, and the scrollbar's room at its right.
const PAD: f32 = 8.0;
const BAR_ROOM: f32 = 10.0;
/// The Close button.
const CLOSE: [f32; 4] = [270.0, 414.0, 100.0, 20.0];
/// The description line under the box.
const HINT_Y: f32 = 446.0;

/// Body text size and line heights in the detail box (canvas units).
const BODY: f32 = 11.0;
const BODY_LINE: f32 = 13.5;
const CREDIT: f32 = 8.5;
const CREDIT_LINE: f32 = 12.0;
const GAP: f32 = 5.0;
const INDENT: f32 = 10.0;

// Everything sits in the box, and the description line under it.
const _: () = assert!(BOX[1] + BOX[3] < HINT_Y && HINT_Y + 14.0 <= 480.0);
const _: () = assert!(LIST_ROWS as f32 * ROW_HEIGHT + 4.0 <= LIST[3]);
const _: () = assert!(LIST_ROWS <= ROW_LIMIT);
// The list, the detail box and Close follow each other without overlapping.
const _: () = assert!(LIST[0] + LIST[2] < DETAIL[0] && TITLE[1] + TITLE[3] < LIST[1]);
const _: () = assert!(LIST[1] + LIST[3] < CLOSE[1] && DETAIL[1] + DETAIL[3] < CLOSE[1]);

fn line_height(kind: LineKind) -> f32 {
    match kind {
        LineKind::Intro | LineKind::Change | LineKind::More => BODY_LINE,
        LineKind::Credit => CREDIT_LINE,
        LineKind::Gap => GAP,
    }
}

impl Panel {
    /// Draw the classic+ page over the whole frame. As with the debug panel,
    /// text other overlays appended earlier this frame is dropped rather than
    /// shown through. The SJK UI's look is drawn by `append_sjk` instead.
    pub(crate) fn append(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        vertices.clear();
        self.rows = LIST_ROWS;
        self.first = self
            .first
            .min(self.releases.len().saturating_sub(self.rows));
        let place = Placement::new(viewport);
        let mut hint: Option<&'static str> = None;

        self.ui.begin_transparent(viewport);
        self.classic_frame(viewport, &place);
        self.classic_list(&place, &mut hint);
        self.classic_detail(&place, font);
        self.classic_close(&place, &mut hint);
        text(
            &mut self.ui,
            &place,
            format_args!(
                "{}",
                hint.unwrap_or(
                    "Up and Down choose a release, Page Up and Page Down scroll, Esc closes."
                )
            ),
            [0.0, HINT_Y, 640.0, 14.0],
            12.0,
            HINT,
            FontWeight::Regular,
            TextAlign::Center,
        );

        let selected = self.selected.saturating_sub(self.first);
        self.ui
            .finish(ROW_BASE + selected.min(ROW_LIMIT - 1) as u16);
        self.ui.append_text(vertices, font, viewport);
    }

    /// The dimmed screen and the pop-up box with its title band.
    fn classic_frame(&mut self, viewport: [f32; 2], place: &Placement) {
        let art = self.art;
        fill(
            &mut self.ui,
            Rect::new(0.0, 0.0, viewport[0], viewport[1]),
            view::ink(0.82),
        );
        let rect = place.rect(BOX);
        if art.has(ArtPiece::PopupBox) {
            view::art(&mut self.ui, ArtPiece::PopupBox, rect);
        } else {
            fill(&mut self.ui, rect, view::ink(0.9));
            border(&mut self.ui, rect, place.scale, FRAME);
        }
        let band = place.rect(TITLE);
        if art.has(ArtPiece::BlendBox) {
            view::art(&mut self.ui, ArtPiece::BlendBox, band);
        } else {
            view::soft_band(&mut self.ui, band, 0.16);
        }
        text(
            &mut self.ui,
            place,
            format_args!("SJK CHANGELOG"),
            [TITLE[0], TITLE[1] + 3.0, TITLE[2], TITLE[3] - 6.0],
            12.5,
            LABEL,
            FontWeight::Semibold,
            TextAlign::Center,
        );
    }

    /// The releases, newest first: title, then date and change count.
    fn classic_list(&mut self, place: &Placement, hint: &mut Option<&'static str>) {
        let s = place.scale;
        let art = self.art;
        let box_rect = place.rect(LIST);
        fill(&mut self.ui, box_rect, LIST_BACK);
        let hovered_list = (0..self.rows).any(|slot| self.ui.token_hovered(ROW_BASE + slot as u16));
        border(
            &mut self.ui,
            box_rect,
            s,
            if hovered_list { FOCUS } else { LIST_BORDER },
        );
        let shown = self.first..self.releases.len().min(self.first + self.rows);
        for (slot, index) in shown.enumerate() {
            let token = ROW_BASE + slot as u16;
            let row = [
                LIST[0] + 2.0,
                LIST[1] + 2.0 + slot as f32 * ROW_HEIGHT,
                LIST[2] - 4.0,
                ROW_HEIGHT,
            ];
            let selected = index == self.selected;
            if selected {
                if art.has(ArtPiece::BlendBox2) {
                    view::art(&mut self.ui, ArtPiece::BlendBox2, place.rect(row));
                } else {
                    view::soft_band(&mut self.ui, place.rect(row), 0.3);
                }
            }
            let hovered = self.ui.token_hovered(token);
            if hovered {
                *hint = Some("Show this release's changes.");
            }
            let release = &self.releases[index];
            text(
                &mut self.ui,
                place,
                format_args!("{}", release.title),
                [row[0] + 6.0, row[1] + 2.0, row[2] - 12.0, 14.0],
                12.0,
                if selected || hovered { FOCUS } else { GOLD },
                FontWeight::Semibold,
                TextAlign::Start,
            );
            text(
                &mut self.ui,
                place,
                format_args!("{}", release.meta),
                [row[0] + 6.0, row[1] + 16.0, row[2] - 12.0, 11.0],
                8.5,
                if release.date.is_empty() { GOLD } else { VALUE },
                FontWeight::Regular,
                TextAlign::Start,
            );
            self.ui.hit_region(token, place.rect(row));
        }
    }

    /// The selected release: its date line and title, then its wrapped
    /// introduction and changes, each change followed by its credit.
    fn classic_detail(&mut self, place: &Placement, font: &UiFont) {
        let s = place.scale;
        let [x, y, width, height] = DETAIL;
        let rect = place.rect(DETAIL);
        fill(&mut self.ui, rect, view::ink(0.45));
        border(&mut self.ui, rect, s, FRAME);
        self.ui.scroll_region(PANE_TOKEN, rect);
        if let Some(error) = &self.error {
            text(
                &mut self.ui,
                place,
                format_args!("{error}"),
                [x + PAD, y + PAD, width - PAD * 2.0, 14.0],
                11.0,
                GOLD,
                FontWeight::Regular,
                TextAlign::Start,
            );
            return;
        }
        let Some(release) = self.releases.get(self.selected) else {
            return;
        };
        text(
            &mut self.ui,
            place,
            format_args!("{}", release.title),
            [x + PAD, y + 4.0, width * 0.6, 16.0],
            14.0,
            GOLD,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        text(
            &mut self.ui,
            place,
            format_args!("{}", Caps(&release.meta)),
            [x + width * 0.5, y + 4.0, width * 0.5 - PAD, 16.0],
            9.0,
            GOLD,
            FontWeight::Semibold,
            TextAlign::End,
        );
        fill(
            &mut self.ui,
            place.rect([x + 6.0, y + 23.0, width - 12.0, 1.0]),
            with_alpha(FRAME, 0.7),
        );

        let top = y + 29.0;
        let bottom = y + height - PAD;
        let text_width = width - PAD * 2.0 - BAR_ROOM;
        self.wrap(font, text_width * s, BODY * s, 0.3 * s, INDENT * s);
        self.fit_scroll(bottom - top, line_height);

        let mut line_y = top;
        let mut shown = 0_usize;
        for line in &self.lines[self.scroll..] {
            let line_box = line_height(line.kind);
            if line_y + line_box > bottom + 0.01 {
                break;
            }
            shown += 1;
            let left = x + PAD;
            match line.kind {
                LineKind::Gap => {}
                LineKind::Intro => text(
                    &mut self.ui,
                    place,
                    format_args!("{}", line.text),
                    [left, line_y, text_width, BODY_LINE],
                    BODY,
                    VALUE,
                    FontWeight::Regular,
                    TextAlign::Start,
                ),
                LineKind::Change | LineKind::More => {
                    if line.kind == LineKind::Change {
                        fill(
                            &mut self.ui,
                            place.rect([left + 1.0, line_y + 5.5, 3.0, 3.0]),
                            GOLD,
                        );
                    }
                    text(
                        &mut self.ui,
                        place,
                        format_args!("{}", line.text),
                        [left + INDENT, line_y, text_width - INDENT, BODY_LINE],
                        BODY,
                        OPTION,
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                }
                LineKind::Credit => text(
                    &mut self.ui,
                    place,
                    format_args!("{}", line.text),
                    [left + INDENT, line_y, text_width - INDENT, CREDIT_LINE],
                    CREDIT,
                    GOLD,
                    FontWeight::Semibold,
                    TextAlign::Start,
                ),
            }
            line_y += line_box;
        }
        self.page_lines = shown.saturating_sub(2).max(1);

        if self.max_scroll > 0 {
            let track = [x + width - 8.0, top, 3.0, bottom - top];
            fill(&mut self.ui, place.rect(track), view::ink(0.6));
            let thumb = thumb(track, self.scroll, shown, self.lines.len());
            let active = self.ui.token_hovered(PANE_BAR_TOKEN);
            fill(
                &mut self.ui,
                place.rect(thumb),
                if active { FOCUS } else { view::gold(0.8) },
            );
            // Wider than the bar so it can be caught.
            self.ui.scroll_region(
                PANE_BAR_TOKEN,
                place.rect([track[0] - 3.0, track[1], track[2] + 6.0, track[3]]),
            );
        }
    }

    /// The gold Close button.
    fn classic_close(&mut self, place: &Placement, hint: &mut Option<&'static str>) {
        let rect = place.rect(CLOSE);
        let hovered = self.ui.token_hovered(BACK_TOKEN);
        if hovered {
            *hint = Some("Close the changelog (Esc).");
            view::glow(&mut self.ui, rect, place.scale, self.art);
        }
        text(
            &mut self.ui,
            place,
            format_args!("CLOSE"),
            CLOSE,
            14.0,
            if hovered { view::focus_pulse() } else { GOLD },
            FontWeight::Semibold,
            TextAlign::Center,
        );
        self.ui.hit_region(BACK_TOKEN, rect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside(inner: [f32; 4], outer: [f32; 4]) -> bool {
        inner[0] >= outer[0]
            && inner[1] >= outer[1]
            && inner[0] + inner[2] <= outer[0] + outer[2]
            && inner[1] + inner[3] <= outer[1] + outer[3]
    }

    #[test]
    fn every_part_lies_inside_the_box() {
        for part in [TITLE, LIST, DETAIL, CLOSE] {
            assert!(inside(part, BOX), "{part:?}");
        }
    }
}
