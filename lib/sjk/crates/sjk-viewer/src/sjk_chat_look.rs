//! How an SJK chat line looks wherever it shows (`docs/hub-chat.md`): in the game's
//! chat, on the main page's dock and on the SJK chat page. A line flows as one line
//! and wraps only when it is too long, as a game chat line does: the sender's name in
//! its own colour codes, the verified tick after it for a verified sender (no word),
//! then the message in the SJK chat's gold, a colour no `^` code gives, so the line
//! stands apart from every game chat line.

use crate::text::{TextFace, TextStyle, UiFont, visible_text_width_style};
use sjk_ui::{Color, DrawCommand, DrawList, Rect};
use std::ops::Range;

/// The SJK chat's gold (`#F5C756`), its messages' colour. It is none of the game's
/// colour codes (`^0` to `^9`, in the game's palette and in the SJK UI's lifted one):
/// brighter than the SJK UI's accent gold (`#E8B84A`), far from `^3` yellow and `^8`
/// orange.
pub(crate) const GOLD: Color = Color::new(0.961, 0.78, 0.337, 1.0);

/// [`GOLD`] at `alpha`.
pub(crate) const fn gold(alpha: f32) -> Color {
    Color::new(GOLD.r, GOLD.g, GOLD.b, alpha)
}

/// A message's text as SJK chat shows it: what the chat rules let through
/// ([`sjk_identity::chat::for_display`]), its colour codes dropped so all of it is in
/// [`GOLD`]. Names keep theirs.
pub(crate) fn message_text(raw: &str) -> String {
    crate::text::Plain(&sjk_identity::chat::for_display(raw)).to_string()
}

/// The verified tick's side next to a name drawn at `size`.
pub(crate) fn tick_side(size: f32) -> f32 {
    size * 0.9
}

/// The room the tick takes after a name drawn at `size`: a gap, the tick and a hair
/// after it.
pub(crate) fn tick_room(size: f32) -> f32 {
    size * 0.2 + tick_side(size) + size * 0.05
}

/// Draw the verified tick (the nameplates' gold seal with its white tick,
/// [`crate::ui_renderer::VERIFIED_TEXTURE`]) after a name drawn at `size` that ends at
/// `x`, centred on the height `middle`, at `alpha`. Returns where it was drawn.
pub(crate) fn tick(list: &mut DrawList, x: f32, middle: f32, size: f32, alpha: f32) -> Rect {
    let side = tick_side(size);
    let rect = Rect::new(x + size * 0.2, middle - side * 0.5, side, side);
    let _ = list.push(DrawCommand::TexturedQuad {
        rect,
        texture: crate::ui_renderer::VERIFIED_TEXTURE,
        color: Color::new(1.0, 1.0, 1.0, alpha),
    });
    rect
}

/// What an SJK chat message shows as, in the docked chat and on the chat page: a
/// message's name and text, or a holocron drop's one sentence with no name, in its tier's
/// colour ([`crate::holocrons::line`]).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Shown {
    /// The sender's name with its colour codes; empty for a drop.
    pub(crate) name: String,
    pub(crate) text: String,
    /// The tier of a holocron drop.
    pub(crate) tier: Option<&'static crate::holocrons::Tier>,
}

/// `message` as it shows; `None` for a drop of a tier this client does not know.
pub(crate) fn shown(message: &sjk_identity::ChatMessage) -> Option<Shown> {
    if message.holocron.is_none() {
        return Some(Shown {
            name: sjk_identity::chat::name_for_display(&message.name),
            text: message_text(&message.text),
            tier: None,
        });
    }
    let words = crate::holocrons::line::words(message)?;
    Some(Shown {
        name: String::new(),
        text: words.text,
        tier: Some(words.tier),
    })
}

/// The colour a line's text is drawn in: its tier's for a holocron drop, else the SJK
/// chat's gold.
pub(crate) fn text_colour(tier: Option<&crate::holocrons::Tier>) -> Color {
    tier.map_or(GOLD, |tier| tier.colour)
}

/// Draw a holocron drop's mark (the tier's gem, in place of the verified tick) after a
/// name drawn at `size` that ends at `x`, centred on the height `middle`, at `alpha`.
/// Returns where it was drawn; the room it takes is [`tick_room`]'s.
pub(crate) fn gem_mark(
    list: &mut DrawList,
    x: f32,
    middle: f32,
    size: f32,
    tier: &crate::holocrons::Tier,
    alpha: f32,
) -> Rect {
    let side = tick_side(size);
    let rect = Rect::new(x + size * 0.2, middle - side * 0.5, side, side);
    crate::holocrons::gem::draw(
        list,
        [rect.x + side * 0.5, middle],
        side * 0.5,
        tier.colour,
        alpha,
        crate::holocrons::gem::MARK_ROWS,
    );
    rect
}

/// Text widths as a font draws them in a text style: the menus' sizes and letter
/// spacing (`ui_textScale`, `ui_letterSpacing`) apply when their text is drawn, so a
/// line laid out run after run is measured with them.
#[derive(Clone, Copy)]
pub(crate) struct Measure<'a> {
    font: &'a UiFont,
    style: TextStyle,
}

impl std::fmt::Debug for Measure<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Measure")
            .field("height", &self.font.height)
            .field("style", &self.style)
            .finish()
    }
}

impl<'a> Measure<'a> {
    pub(crate) fn new(font: &'a UiFont, style: TextStyle) -> Self {
        Self { font, style }
    }

    /// How much larger than their size the text style draws runs (`ui_textScale`).
    pub(crate) fn scale(&self) -> f32 {
        self.style.scale
    }

    /// The width of `text` drawn at `size` in `face`, colour codes taking no room.
    pub(crate) fn width(&self, text: &str, size: f32, face: TextFace) -> f32 {
        let size = size * self.style.scale;
        visible_text_width_style(
            self.font,
            text,
            size / self.font.height.max(1.0),
            face,
            self.style.tracking * size,
        )
    }

    /// How many bytes of `text` fit `width` at `size` in `face`: at least one
    /// character, never between a colour code's `^` and its digit.
    pub(crate) fn fitting(&self, text: &str, width: f32, size: f32, face: TextFace) -> usize {
        let mut used = 0.0;
        let mut chars = text.char_indices().peekable();
        while let Some((at, c)) = chars.next() {
            if c == '^' && chars.peek().is_some_and(|(_, next)| next.is_ascii_digit()) {
                chars.next();
                continue;
            }
            let end = at + c.len_utf8();
            used += self.width(&text[at..end], size, face);
            if used > width && at > 0 {
                return at;
            }
        }
        text.len()
    }

    /// [`Measure::width`] to the right edge of the last visible letter's ink: the
    /// room after it (its side bearing, the letter spacing) left out, so a mark set
    /// a gap after a word sits that gap from its last letter whatever the letter.
    pub(crate) fn ink_width(&self, text: &str, size: f32, face: TextFace) -> f32 {
        let width = self.width(text, size, face);
        let mut last = None;
        let mut chars = text.char_indices().peekable();
        while let Some((at, c)) = chars.next() {
            if c == '^' && chars.peek().is_some_and(|(_, next)| next.is_ascii_digit()) {
                chars.next();
                continue;
            }
            last = Some(at);
        }
        let Some(at) = last else {
            return width;
        };
        let size = size * self.style.scale;
        let scale = size / self.font.height.max(1.0);
        let glyph = self.font.glyph_of(face, crate::text::glyph_at(text, at).0);
        let after =
            (glyph.advance - glyph.offset_x - glyph.width) * scale + self.style.tracking * size;
        (width - after).max(0.0)
    }
}

/// `text` cut into rows, the first `first` wide (what the name before it leaves) and
/// the others `rest` wide, at spaces, as a game chat line wraps. A word that does not
/// fit after the name but fits a whole row starts the next row (the first row is then
/// empty); a word longer than a row is broken. `fits(text, width)` is how many bytes
/// of `text` fit `width` ([`Measure::fitting`]).
pub(crate) fn flow(
    text: &str,
    first: f32,
    rest: f32,
    fits: impl Fn(&str, f32) -> usize,
) -> Vec<Range<usize>> {
    let mut rows = Vec::with_capacity(2);
    flow_each(text, first, rest, fits, |row| rows.push(row));
    rows
}

/// [`flow`] handing each row to `row` instead of collecting them, so a layout redone
/// every frame (the docked chat's) allocates nothing. Always at least one row.
pub(crate) fn flow_each(
    text: &str,
    first: f32,
    rest: f32,
    fits: impl Fn(&str, f32) -> usize,
    mut row: impl FnMut(Range<usize>),
) {
    let mut count = 0;
    let mut start = 0;
    while start < text.len() {
        let width = if count == 0 { first } else { rest };
        let mut end = start + fits(&text[start..], width.max(0.0));
        if end < text.len() {
            match text[start..end].rfind(' ') {
                Some(space) if space > 0 => end = start + space,
                _ if count == 0 && first < rest => {
                    let word = text[start..].find(' ').map_or(text.len(), |at| start + at);
                    if word > end && word <= start + fits(&text[start..], rest) {
                        end = start;
                    }
                }
                _ => {}
            }
        }
        row(start..end);
        count += 1;
        start = end;
        while text.as_bytes().get(start) == Some(&b' ') {
            start += 1;
        }
    }
    if count == 0 {
        row(0..0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{CodePalette, test_font};

    #[test]
    fn the_gold_is_none_of_the_colour_codes() {
        let distance = |colour: [f32; 4]| {
            ((colour[0] - GOLD.r).powi(2)
                + (colour[1] - GOLD.g).powi(2)
                + (colour[2] - GOLD.b).powi(2))
            .sqrt()
        };
        for index in 0..=9 {
            for palette in [CodePalette::Game, CodePalette::Legible] {
                let code = palette.colour(index);
                assert!(distance(code) > 0.3, "^{index} {palette:?}: {code:?}");
            }
        }
        // Nor the SJK UI's accent gold, which its tag and the menus' marks use.
        let accent = crate::menu::sjk::color::GOLD;
        assert!(distance([accent.r, accent.g, accent.b, 1.0]) > 0.05);
    }

    #[test]
    fn a_message_is_shown_without_its_colour_codes() {
        assert_eq!(message_text("hello ^1there ^2^3you"), "hello there you");
        assert_eq!(message_text("bad \u{221E}x"), "bad x");
        assert_eq!(message_text("100% sure"), "100% sure");
    }

    /// Rows of `text` with test glyphs 8 wide at size 12.
    fn rows(text: &str, first: usize, rest: usize) -> Vec<&str> {
        let font = test_font();
        let measure = Measure::new(&font, TextStyle::NEUTRAL);
        flow(
            text,
            first as f32 * 8.0,
            rest as f32 * 8.0,
            |text, width| measure.fitting(text, width, 12.0, TextFace::Regular),
        )
        .into_iter()
        .map(|range| &text[range])
        .collect()
    }

    #[test]
    fn a_short_message_stays_on_the_names_row() {
        assert_eq!(rows("gg all", 20, 40), ["gg all"]);
        assert_eq!(rows("", 20, 40), [""]);
    }

    #[test]
    fn a_long_message_wraps_at_spaces_like_a_chat_line() {
        assert_eq!(
            rows("alpha bravo charlie delta echo foxtrot", 12, 20),
            ["alpha bravo", "charlie delta echo", "foxtrot"]
        );
    }

    #[test]
    fn a_first_word_too_wide_for_the_rest_of_the_row_starts_the_next() {
        assert_eq!(rows("unbelievable yes", 6, 20), ["", "unbelievable yes"]);
        // Wider than any row: broken where the name's row ends.
        let long = "x".repeat(30);
        assert_eq!(rows(&long, 6, 20), ["xxxxxx", &"x".repeat(20)[..], "xxxx"]);
    }

    #[test]
    fn the_measure_follows_the_text_style() {
        let font = test_font();
        let neutral = Measure::new(&font, TextStyle::NEUTRAL);
        assert_eq!(neutral.width("^2Sol", 12.0, TextFace::Regular), 24.0);
        let larger = Measure::new(
            &font,
            TextStyle {
                scale: 1.5,
                tracking: 0.0,
            },
        );
        assert_eq!(larger.width("Sol", 12.0, TextFace::Regular), 36.0);
        // A code is never split.
        assert_eq!(neutral.fitting("^1ab", 8.0, 12.0, TextFace::Regular), 3);
    }
}
