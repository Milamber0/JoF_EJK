//! The Identity page in the classic+ style (`docs/classic-plus.md`), drawn while the menus
//! are classic (`ui_menuStyle classic`). It takes the frame of the classic+ command browser
//! and changelog: the `menu_box_ingame` pop-up over the dimmed screen, the `menu_blendbox`
//! title band, retail's option rows (a lilac label, a gold value, the highlight band behind
//! the focused row), list-box text fields, gold buttons that glow when focused, and the
//! description line under the box. Every control answers to the page's tokens, so the
//! keyboard and the pointer serve both looks.

use super::{
    BACK_TOKEN, BIO_TOKEN, COPY_TOKEN, Focus, HUB_TOKEN, Inputs, Panel, REVEAL_TOKEN, SAVE_TOKEN,
    TOGGLE_TOKEN, fit_tail, view as page_view,
};
use crate::console::browser::classic::{
    FRAME, LABEL, LIST_BACK, LIST_BORDER, OPTION, VALUE, border, fill, text, with_alpha,
};
use crate::menu::art::ArtPiece;
use crate::menu::classic::layout::Placement;
use crate::menu::classic::view::{self, Caps, FOCUS, GOLD, HINT};
use crate::text::{TextVertex, UiFont};
use sjk_ui::{Color, FontWeight, Rect, TextAlign};
use std::time::Instant;

/// The pop-up box and its title band (canvas units), as the classic+ browser's.
const BOX: [f32; 4] = [35.0, 26.0, 570.0, 414.0];
const TITLE: [f32; 4] = [55.0, 32.0, 530.0, 22.0];
/// Left edge and width of the content, and where it starts.
const LEFT: f32 = 55.0;
const WIDTH: f32 = 530.0;
const TOP: f32 = 60.0;
/// Where the content must end, above the Close button.
const BOTTOM: f32 = 408.0;
/// Height of a status line, of an option row and of a known-player row.
const LINE: f32 = 12.5;
const ROW: f32 = 18.0;
const PLAYER_ROW: f32 = 14.0;
/// Space between one block and the next.
const GAP: f32 = 6.0;
/// The text fields' left edge.
const FIELD_X: f32 = 130.0;
/// The Close button, and the description line under the box.
const CLOSE: [f32; 4] = [270.0, 414.0, 100.0, 20.0];
const HINT_Y: f32 = 446.0;
/// Longest status line, in characters, before it is cut.
const LINE_CHARS: usize = 100;

// The box and the description line under it fit the canvas; the title band is above the
// content and the content ends above Close.
const _: () = assert!(BOX[1] + BOX[3] < HINT_Y && HINT_Y + 14.0 <= 480.0);
const _: () = assert!(TITLE[1] + TITLE[3] <= TOP);
const _: () = assert!(BOTTOM < CLOSE[1]);

/// Where each part of the page sits, in canvas units.
#[derive(Debug, PartialEq)]
struct Layout {
    status: [f32; 4],
    toggle: [f32; 4],
    /// The bio field's row, the buttons' and the message line, with the fields on.
    bio: Option<[f32; 4]>,
    buttons: Option<[f32; 4]>,
    message: Option<[f32; 4]>,
    /// The known players' heading, and how many rows fit under it.
    players: Option<[f32; 4]>,
    player_rows: usize,
}

fn layout(lines: usize, fields: bool, players: usize) -> Layout {
    let status_height = 22.0 + lines as f32 * LINE + 4.0;
    let status = [LEFT, TOP, WIDTH, status_height];
    let mut y = TOP + status_height + GAP;
    let toggle = [LEFT, y, WIDTH, ROW];
    y += ROW + GAP;
    let (mut bio, mut buttons, mut message) = (None, None, None);
    if fields {
        bio = Some([LEFT, y, WIDTH, ROW]);
        y += ROW + GAP;
        buttons = Some([LEFT, y, WIDTH, ROW]);
        y += ROW + 4.0;
        message = Some([LEFT, y, WIDTH, 12.0]);
        y += 12.0 + GAP;
    }
    let first = y + 16.0;
    let player_rows = players.min(((BOTTOM - first) / PLAYER_ROW).max(0.0) as usize);
    let players_box = (player_rows > 0).then_some([LEFT, y, WIDTH, 12.0]);
    Layout {
        status,
        toggle,
        bio,
        buttons,
        message,
        players: players_box,
        player_rows,
    }
}

/// `text` cut to `max` characters with `...` at the end when it was longer.
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let head: String = text.chars().take(max.saturating_sub(3)).collect();
    format!("{head}...")
}

impl Panel {
    /// Draw the classic+ page over the whole frame (see [`Panel::append`]).
    pub(super) fn append_classic(
        &mut self,
        inputs: &Inputs<'_>,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        let mut page = page_view(inputs, self.revealed);
        // The medals are the status's last line, their medallions at its right end.
        let medal_line = super::medals::classic_line(page.medals.as_deref()).map(|line| {
            page.lines.push(line);
            page.lines.len() - 1
        });
        let place = Placement::new(viewport);
        let rows = layout(page.lines.len(), self.fields, page.players.len());
        let mut hint: Option<&'static str> = None;

        self.ui.begin_transparent(viewport);
        self.classic_frame(viewport, &place);

        // What the page says: the status, in a framed box.
        let status = place.rect(rows.status);
        fill(&mut self.ui, status, view::ink(0.45));
        border(&mut self.ui, status, place.scale, FRAME);
        let [x, y, width, _] = rows.status;
        text(
            &mut self.ui,
            &place,
            format_args!("{}", clip(&page.headline, 60)),
            [x + 8.0, y + 3.0, width - 16.0, 16.0],
            13.0,
            GOLD,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        for (index, line) in page.lines.iter().enumerate() {
            text(
                &mut self.ui,
                &place,
                format_args!("{}", clip(line, LINE_CHARS)),
                [x + 8.0, y + 22.0 + index as f32 * LINE, width - 16.0, LINE],
                10.0,
                VALUE,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        if let (Some(index), Some(medals)) = (medal_line, page.medals.as_deref()) {
            let line_y = y + 22.0 + index as f32 * LINE;
            for (slot, award) in medals.iter().rev().enumerate() {
                let side = LINE - 1.0;
                let _ = self
                    .ui
                    .draw_list_mut()
                    .push(sjk_ui::DrawCommand::TexturedQuad {
                        rect: place.rect([
                            x + width - 8.0 - (slot + 1) as f32 * (side + 2.0),
                            line_y,
                            side,
                            side,
                        ]),
                        texture: award.medal.icon(),
                        color: Color::new(1.0, 1.0, 1.0, 1.0),
                    });
            }
        }

        self.classic_toggle(&place, rows.toggle, &mut hint);
        if let (Some(bio), Some(buttons)) = (rows.bio, rows.buttons) {
            self.classic_bio(&place, bio, &mut hint);
            self.classic_buttons(&place, buttons, &mut hint);
        }
        if let Some(message) = rows.message
            && !self.message.is_empty()
        {
            text(
                &mut self.ui,
                &place,
                format_args!("{}", self.message),
                message,
                10.0,
                Color::new(1.0, 0.45, 0.4, 1.0),
                FontWeight::Semibold,
                TextAlign::Start,
            );
        }
        if let Some(heading) = rows.players {
            self.classic_players(&place, heading, &page.players, rows.player_rows);
        }
        self.classic_close(&place, &mut hint);
        text(
            &mut self.ui,
            &place,
            format_args!(
                "{}",
                hint.unwrap_or(
                    "Tab moves between the controls, Enter works the one you are on, Esc closes."
                )
            ),
            [0.0, HINT_Y, 640.0, 14.0],
            12.0,
            HINT,
            FontWeight::Regular,
            TextAlign::Center,
        );
        self.ui.finish(self.focus.token());
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
            format_args!("SJK IDENTITY"),
            [TITLE[0], TITLE[1] + 3.0, TITLE[2], TITLE[3] - 6.0],
            12.5,
            LABEL,
            FontWeight::Semibold,
            TextAlign::Center,
        );
    }

    /// The highlight band behind a row that has the focus or the pointer.
    fn classic_row_band(&mut self, place: &Placement, rect: [f32; 4]) {
        if self.art.has(ArtPiece::BlendBox2) {
            view::art(&mut self.ui, ArtPiece::BlendBox2, place.rect(rect));
        } else {
            view::soft_band(&mut self.ui, place.rect(rect), 0.3);
        }
    }

    /// Retail's option row: a lilac label, the gold ON or OFF at the right.
    fn classic_toggle(
        &mut self,
        place: &Placement,
        row: [f32; 4],
        hint: &mut Option<&'static str>,
    ) {
        let hovered = self.ui.token_hovered(TOGGLE_TOKEN);
        let on = self.focus == Focus::Toggle || hovered;
        if on {
            self.classic_row_band(place, row);
        }
        if hovered {
            *hint = Some(
                "Share your in-game name and the server you play on with the SJK hub, or switch it all off.",
            );
        }
        let [x, y, width, height] = row;
        text(
            &mut self.ui,
            place,
            format_args!("{}", Caps("Share my identity with the SJK hub")),
            [x + 8.0, y, width * 0.7, height],
            11.0,
            if on { view::focus_pulse() } else { OPTION },
            FontWeight::Regular,
            TextAlign::Start,
        );
        text(
            &mut self.ui,
            place,
            format_args!("{}", if self.enabled { "ON" } else { "OFF" }),
            [x + width * 0.7, y, width * 0.3 - 8.0, height],
            11.0,
            if on { view::focus_pulse() } else { GOLD },
            FontWeight::Semibold,
            TextAlign::End,
        );
        self.ui.hit_region(TOGGLE_TOKEN, place.rect(row));
    }

    /// The optional bio field, a retail list box with its label at the left.
    fn classic_bio(&mut self, place: &Placement, bio: [f32; 4], hint: &mut Option<&'static str>) {
        let bio_text = std::mem::take(&mut self.bio);
        self.classic_field(
            place,
            "About you",
            bio,
            BIO_TOKEN,
            &bio_text,
            "Optional: a few words about you",
            Focus::Bio,
            "A few words about you (optional, 500 characters at most). Enter saves.",
            hint,
        );
        self.bio = bio_text;
    }

    #[allow(clippy::too_many_arguments)]
    fn classic_field(
        &mut self,
        place: &Placement,
        label: &str,
        row: [f32; 4],
        token: u16,
        content: &str,
        prompt: &str,
        focus: Focus,
        help: &'static str,
        hint: &mut Option<&'static str>,
    ) {
        let focused = self.focus == focus;
        let hovered = self.ui.token_hovered(token);
        if hovered {
            *hint = Some(help);
        }
        let [x, y, _, height] = row;
        text(
            &mut self.ui,
            place,
            format_args!("{}", Caps(label)),
            [x + 8.0, y, FIELD_X - x - 12.0, height],
            11.0,
            if focused { view::focus_pulse() } else { OPTION },
            FontWeight::Regular,
            TextAlign::Start,
        );
        let field = [FIELD_X, y, LEFT + WIDTH - FIELD_X, height];
        let rect = place.rect(field);
        fill(&mut self.ui, rect, LIST_BACK);
        border(
            &mut self.ui,
            rect,
            place.scale,
            if focused || hovered {
                FOCUS
            } else {
                LIST_BORDER
            },
        );
        let caret = focused
            && (Instant::now().duration_since(self.epoch).as_millis() / 500).is_multiple_of(2);
        let room = ((field[2] - 12.0) / 5.5) as usize;
        let (shown, color) = if content.is_empty() && !focused {
            (prompt.to_owned(), with_alpha(VALUE, 0.7))
        } else {
            let mut shown = fit_tail(content, room.saturating_sub(1));
            if caret {
                shown.push('|');
            }
            (shown, VALUE)
        };
        text(
            &mut self.ui,
            place,
            format_args!("{shown}"),
            [field[0] + 6.0, field[1], field[2] - 12.0, field[3]],
            11.0,
            color,
            FontWeight::Regular,
            TextAlign::Start,
        );
        self.ui.hit_region(token, rect);
    }

    /// One gold button: its glow and a pulsing white while focused or hovered.
    #[allow(clippy::too_many_arguments)]
    fn classic_button(
        &mut self,
        place: &Placement,
        token: u16,
        label: &str,
        rect: [f32; 4],
        focused: bool,
        help: &'static str,
        hint: &mut Option<&'static str>,
    ) {
        let area = place.rect(rect);
        let hovered = self.ui.token_hovered(token);
        if hovered {
            *hint = Some(help);
        }
        if hovered || focused {
            view::glow(&mut self.ui, area, place.scale, self.art);
        }
        text(
            &mut self.ui,
            place,
            format_args!("{}", Caps(label)),
            rect,
            12.0,
            if hovered || focused {
                view::focus_pulse()
            } else {
                GOLD
            },
            FontWeight::Semibold,
            TextAlign::Center,
        );
        self.ui.hit_region(token, area);
    }

    /// Save, copy the key id, show or hide the key and, when the hub is not SJK's own,
    /// the way back to it.
    fn classic_buttons(
        &mut self,
        place: &Placement,
        row: [f32; 4],
        hint: &mut Option<&'static str>,
    ) {
        let [x, y, _, height] = row;
        let copied = self
            .copied_until
            .is_some_and(|until| Instant::now() < until);
        let focus = self.focus;
        self.classic_button(
            place,
            SAVE_TOKEN,
            "Save",
            [x, y, 80.0, height],
            focus == Focus::Save,
            "Send your bio to the hub.",
            hint,
        );
        self.classic_button(
            place,
            COPY_TOKEN,
            if copied { "Copied" } else { "Copy my key id" },
            [x + 88.0, y, 140.0, height],
            focus == Focus::Copy,
            "Copy your key id, to give to whoever verifies you.",
            hint,
        );
        self.classic_button(
            place,
            REVEAL_TOKEN,
            if self.revealed {
                "Hide key"
            } else {
                "Show key"
            },
            [x + 236.0, y, 100.0, height],
            focus == Focus::Reveal,
            "Show your key id and key file, hidden while nobody asks (on a stream, say).",
            hint,
        );
        if self.offer_hub {
            self.classic_button(
                place,
                HUB_TOKEN,
                "Use the official hub",
                [x + 344.0, y, 180.0, height],
                focus == Focus::Hub,
                "Use SJK's own hub (sjk.dfox.app) again.",
                hint,
            );
        }
    }

    /// The players the hub knows here: a heading over a thin rule, then their rows.
    fn classic_players(
        &mut self,
        place: &Placement,
        heading: [f32; 4],
        players: &[super::PlayerLine],
        rows: usize,
    ) {
        let [x, y, width, height] = heading;
        text(
            &mut self.ui,
            place,
            format_args!("{}", Caps("Known players here")),
            [x, y, width, height],
            10.5,
            LABEL,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        fill(
            &mut self.ui,
            place.rect([x, y + 13.0, width, 1.0]),
            with_alpha(FRAME, 0.7),
        );
        for (index, player) in players.iter().take(rows).enumerate() {
            let row = [
                x + 4.0,
                y + 16.0 + index as f32 * PLAYER_ROW,
                width - 8.0,
                PLAYER_ROW,
            ];
            text(
                &mut self.ui,
                place,
                format_args!("{:>2}  {}", player.slot, clip(&player.name, 40)),
                row,
                11.0,
                if player.verified { GOLD } else { VALUE },
                FontWeight::Regular,
                TextAlign::Start,
            );
            if player.verified {
                text(
                    &mut self.ui,
                    place,
                    format_args!("VERIFIED"),
                    row,
                    9.0,
                    GOLD,
                    FontWeight::Semibold,
                    TextAlign::End,
                );
            }
        }
    }

    /// The gold Close button.
    fn classic_close(&mut self, place: &Placement, hint: &mut Option<&'static str>) {
        let rect = place.rect(CLOSE);
        let hovered = self.ui.token_hovered(BACK_TOKEN);
        if hovered {
            *hint = Some("Close the page (Esc).");
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

    fn parts(layout: &Layout) -> Vec<[f32; 4]> {
        [
            Some(layout.status),
            Some(layout.toggle),
            layout.bio,
            layout.buttons,
            layout.message,
            layout.players,
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    #[test]
    fn the_title_the_content_and_close_lie_inside_the_box_and_the_canvas() {
        assert!(inside(TITLE, BOX) && inside(CLOSE, BOX));
    }

    #[test]
    fn the_fullest_page_fits_above_close_and_its_parts_do_not_overlap() {
        for (lines, fields, players) in [(9, true, 5), (9, true, 0), (3, false, 5), (1, false, 0)] {
            let rows = layout(lines, fields, players);
            let mut parts = parts(&rows);
            for part in &parts {
                assert!(inside(*part, BOX), "{part:?} for {lines} lines");
                assert!(part[1] + part[3] <= BOTTOM + 0.01, "{part:?} reaches Close");
            }
            parts.sort_by(|a, b| a[1].total_cmp(&b[1]));
            for pair in parts.windows(2) {
                assert!(
                    pair[0][1] + pair[0][3] <= pair[1][1] + 0.01,
                    "{pair:?} overlap"
                );
            }
            let last = rows.players.map_or(0.0, |heading| {
                heading[1] + 16.0 + rows.player_rows as f32 * PLAYER_ROW
            });
            assert!(last <= BOTTOM + 0.01, "the player rows reach Close");
        }
    }

    #[test]
    fn as_many_players_are_listed_as_fit() {
        assert_eq!(layout(9, true, 5).player_rows, 5);
        assert_eq!(layout(9, true, 2).player_rows, 2);
        // Fewer fit when the status is taller and the fields are there.
        assert!(layout(30, true, 5).player_rows < 5);
        assert_eq!(layout(2, false, 0).players, None);
    }

    #[test]
    fn long_text_is_cut_with_an_ellipsis() {
        assert_eq!(clip("short", 10), "short");
        assert_eq!(clip("abcdefghij", 8), "abcde...");
        assert_eq!(clip("abcdefghij", 8).chars().count(), 8);
    }
}
