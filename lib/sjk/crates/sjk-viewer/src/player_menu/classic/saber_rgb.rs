//! Lightsaber creation's custom blade colour: a red, green and blue slider for
//! each saber, as JoF EternalJK's "RGB Color Creation" panel of
//! `ingame_saber.menu` has (`ui_sab1_r`..`ui_sab2_b`, written to the JA+
//! userinfo keys by `UI_UpdateSaberColor`, `codemp/ui/ui_main.c`). Retail had
//! only the six swatches. A slider always shows the blade's colour as it is
//! drawn now (a stock colour's own tint), and moving one makes the colour the
//! saber's own: `color1`/`color2` become `SABER_RGB` (6) and the tint goes to
//! `cp_sbRGB1`/`cp_sbRGB2` packed as `r | g << 8 | b << 16`
//! ([`crate::player_menu::saber`]); a swatch brings a stock colour back.
//!
//! Each row is the channel's letter, retail's slider bar and thumb (the option
//! panels' art) and the value; the bar takes clicks, drags and the wheel. On
//! the full page the rows stand in the right of the lower box under a heading
//! per saber with a chip of the colour, the live saber left of them; in the
//! in-game window they sit under their saber's swatches.

use super::Frame;
use super::view::{FRAME, LABEL, VALUE};
use crate::menu::art::ArtPiece;
use crate::menu::classic::layout::Placement;
use crate::menu::classic::view::FOCUS;
use crate::player_menu::PlayerMenu;
use crate::player_menu::rows::SaberRow;
use crate::player_menu::saber::RGB_COLOR_INDEX;
use sjk_ui::{Color, FontWeight, Rect, TextAlign};

/// First token of the slider bars: the first saber's red, green and blue,
/// then the second saber's.
pub(super) const RGB_BASE: u16 = 1960;
/// Width of the channel's letter before the bar.
const LETTER: f32 = 14.0;
/// The bar, at the option panels' slider art size, and the thumb.
const BAR: [f32; 2] = [96.0, 16.0];
const THUMB: [f32; 2] = [12.0, 20.0];
/// Gap between the bar and the value.
const GAP: f32 = 4.0;
/// Channel letters and their tints.
const CHANNELS: [(&str, Color); 3] = [
    ("R", Color::new(1.0, 0.42, 0.42, 1.0)),
    ("G", Color::new(0.42, 1.0, 0.48, 1.0)),
    ("B", Color::new(0.5, 0.64, 1.0, 1.0)),
];
/// The full page's headings, one per saber, over its rows.
const HEADING_FULL: [[f32; 4]; 2] = [[462.0, 248.0, 156.0, 16.0], [462.0, 332.0, 156.0, 16.0]];

/// Which saber (second or not) and channel slider `index` (0..6) edits.
pub(super) fn channel(index: u8) -> (bool, usize) {
    (index >= 3, usize::from(index % 3))
}

/// The saber draft's row for slider `index`, which the arrows and the wheel
/// step.
pub(super) fn row(index: u8) -> SaberRow {
    [
        SaberRow::Red,
        SaberRow::Green,
        SaberRow::Blue,
        SaberRow::SecondRed,
        SaberRow::SecondGreen,
        SaberRow::SecondBlue,
    ][usize::from(index.min(5))]
}

/// The slider whose bar is under pointer `token`.
pub(super) fn of_token(token: u16) -> Option<u8> {
    token
        .checked_sub(RGB_BASE)
        .and_then(|offset| u8::try_from(offset).ok())
        .filter(|offset| *offset < 6)
}

/// Slider `index`'s row, relative to the in-game window or on the full page.
pub(super) fn rect(frame: Frame, index: u8) -> [f32; 4] {
    let (second, step) = (index >= 3, f32::from(index % 3));
    match frame {
        // Right of the live saber in the lower box, a heading over each
        // saber's three rows.
        Frame::Full => [
            462.0,
            268.0 + step * 20.0 + if second { 84.0 } else { 0.0 },
            156.0,
            16.0,
        ],
        // Under the saber's own swatches (`BLADE COLOR` 15 197, `COLOR 2` 270 197).
        Frame::InGame => [
            if second { 270.0 } else { 15.0 },
            225.0 + step * 18.0,
            149.0,
            16.0,
        ],
    }
}

/// The bar of a slider row, on the canvas.
fn bar([x, y, _, h]: [f32; 4]) -> [f32; 4] {
    [x + LETTER, y + (h - BAR[1]) * 0.5, BAR[0], BAR[1]]
}

/// The value at pointer `x` over a bar spanning `bar` (window pixels).
pub(super) fn value_at(bar: Rect, x: f32) -> u8 {
    if bar.width <= 0.0 {
        return 0;
    }
    (((x - bar.x) / bar.width).clamp(0.0, 1.0) * 255.0).round() as u8
}

impl PlayerMenu {
    /// One slider row: letter, bar and thumb, value; the bar's pointer
    /// target.
    pub(super) fn channel_row(
        &mut self,
        place: &Placement,
        canvas: [f32; 4],
        index: u8,
        active: bool,
    ) {
        let (second, channel) = channel(index);
        let value = self.saber.channel(second, channel);
        let [x, y, w, h] = canvas;
        let s = place.scale;
        let (letter, tint) = CHANNELS[channel];
        self.label(
            place,
            [x, y, LETTER, h],
            letter,
            12.0,
            if active { FOCUS } else { tint },
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let track = bar(canvas);
        let token = RGB_BASE + u16::from(index);
        let hovered = self.canvas.token_hovered(token);
        if self.classic.art.has(ArtPiece::Slider) {
            self.piece(place, ArtPiece::Slider, track);
        } else {
            let [bx, by, bw, bh] = track;
            self.fill(
                place,
                [bx, by + bh * 0.45, bw, bh * 0.1],
                Color::new(tint.r, tint.g, tint.b, 0.6),
            );
        }
        let ratio = f32::from(value) / 255.0;
        let thumb = [
            track[0] + track[2] * ratio - THUMB[0] * 0.5,
            track[1] - 2.0,
            THUMB[0],
            THUMB[1],
        ];
        if self.classic.art.has(ArtPiece::SliderThumb) {
            self.piece(place, ArtPiece::SliderThumb, thumb);
        } else {
            let [tx, ty, tw, th] = thumb;
            self.fill(place, [tx + tw * 0.3, ty, tw * 0.4, th], FOCUS);
        }
        let value_x = track[0] + track[2] + GAP;
        self.label_fmt(
            place,
            [value_x, y, x + w - value_x, h],
            format_args!("{value}"),
            12.0,
            if active || hovered { FOCUS } else { VALUE },
            FontWeight::Regular,
            TextAlign::End,
        );
        let target = place.rect(track);
        // The bar's target takes a little height either side, as the thumb does.
        let target = Rect::new(
            target.x,
            target.y - 2.0 * s,
            target.width,
            target.height + 4.0 * s,
        );
        self.canvas.hit_region(token, target);
    }

    /// The full page's headings over each saber's sliders, with a chip of
    /// the blade's colour as drawn now, framed white while it is the saber's
    /// own (RGB) colour.
    pub(super) fn channel_headings(&mut self, place: &Placement, dual: bool) {
        let sabers: &[bool] = if dual { &[false, true] } else { &[false] };
        for &second in sabers {
            let [x, y, w, h] = HEADING_FULL[usize::from(second)];
            let text = match (dual, second) {
                (false, _) => "CUSTOM COLOR",
                (true, false) => "CUSTOM COLOR 1",
                (true, true) => "CUSTOM COLOR 2",
            };
            self.label(
                place,
                [x, y, w - 24.0, h],
                text,
                12.0,
                LABEL,
                FontWeight::Semibold,
                TextAlign::Start,
            );
            let chip = [x + w - 18.0, y + 1.0, 18.0, h - 2.0];
            let [r, g, b] = self.saber.rgb(second).map(|c| f32::from(c) / 255.0);
            self.fill(place, chip, Color::new(r, g, b, 1.0));
            let own = self.saber.color(second) == RGB_COLOR_INDEX;
            self.border(
                place,
                chip,
                if own { FOCUS } else { FRAME },
                if own { 2.0 } else { 1.0 },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_name_their_slider_and_nothing_else() {
        for index in 0..6_u8 {
            assert_eq!(of_token(RGB_BASE + u16::from(index)), Some(index));
        }
        assert_eq!(of_token(RGB_BASE - 1), None);
        assert_eq!(of_token(RGB_BASE + 6), None);
    }

    #[test]
    fn sliders_edit_their_saber_s_channels_in_order() {
        assert_eq!(channel(0), (false, 0));
        assert_eq!(channel(2), (false, 2));
        assert_eq!(channel(3), (true, 0));
        assert_eq!(channel(5), (true, 2));
        assert_eq!(row(1), SaberRow::Green);
        assert_eq!(row(5), SaberRow::SecondBlue);
        for index in 0..6 {
            let (second, channel) = channel(index);
            assert_eq!(row(index).second(), second);
            assert_eq!(row(index).channel(), Some(channel));
        }
    }

    #[test]
    fn the_pointer_sets_the_value_along_the_bar() {
        let bar = Rect::new(100.0, 10.0, 200.0, 16.0);
        assert_eq!(value_at(bar, 100.0), 0);
        assert_eq!(value_at(bar, 300.0), 255);
        assert_eq!(value_at(bar, 200.0), 128);
        assert_eq!(value_at(bar, -50.0), 0);
        assert_eq!(value_at(bar, 900.0), 255);
    }

    #[test]
    fn rows_hold_letter_bar_and_value() {
        for frame in [Frame::Full, Frame::InGame] {
            for index in 0..6 {
                let row = rect(frame, index);
                let [bx, _, bw, _] = bar(row);
                // Room after the bar for "255".
                assert!(
                    row[0] + row[2] - (bx + bw + GAP) >= 30.0,
                    "{frame:?} {index}"
                );
            }
        }
        // The full page's headings sit above their saber's first row.
        for second in [false, true] {
            let heading = HEADING_FULL[usize::from(second)];
            let first = rect(Frame::Full, if second { 3 } else { 0 });
            assert!(heading[1] + heading[3] <= first[1]);
        }
    }
}
