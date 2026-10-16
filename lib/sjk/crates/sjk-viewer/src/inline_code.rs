//! Running text with inline code (`docs/sjk-ui.md`, "What's new card"): what a
//! `CHANGELOG.md` sentence puts in backticks (a command or a cvar, `` `whatsnew` ``) is
//! drawn as a chip, as messaging apps draw inline code. The what's-new card and the
//! changelog page's SJK UI look both lay their text out here: the words are wrapped by
//! their measured width, a chip is never split across lines, and a full stop or comma
//! written against a chip stays with it.

use crate::menu::sjk::{Frame, color, text};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use crate::sjk_chat_look::Measure;
use crate::text::TextFace;
use sjk_ui::{Color, DrawCommand, FontWeight, TextAlign};

/// How a text's runs and chips are drawn, in frame pixels.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Look {
    /// The text's size and line height.
    pub(crate) size: f32,
    pub(crate) line: f32,
    /// A chip's text size, the room either side of its text, its height and corners.
    pub(crate) chip_size: f32,
    pub(crate) chip_pad: f32,
    pub(crate) chip_height: f32,
    pub(crate) chip_radius: f32,
}

/// How the text and the chips' text measure: the body family's, and the family chips
/// are drawn in.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Measures<'a> {
    pub(crate) body: Measure<'a>,
    pub(crate) code: Measure<'a>,
}

impl Measures<'_> {
    /// A number that changes when the fonts or the text style change, to tell a layout
    /// made with other measures.
    pub(crate) fn key(&self, look: &Look) -> u32 {
        let body = self.body.width("Mm `", look.size, TextFace::Regular);
        let code = self.code.width("Mm `", look.chip_size, TextFace::Semibold);
        (body + code * 3.0).to_bits()
    }
}

/// A piece of a wrapped line: plain text, or a chip's text. `x` and `width` are frame
/// pixels from the text's left edge; a chip's width holds its padding.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Run {
    pub(crate) x: f32,
    pub(crate) width: f32,
    pub(crate) text: String,
    pub(crate) code: bool,
}

/// The room between a chip and a token glued to it, in frame pixels.
const GLUE: f32 = 3.0;

/// One wrapped line.
pub(crate) type Row = Vec<Run>;

/// `text` without its code marks, for a surface that draws no chips.
pub(crate) fn plain(text: &str) -> String {
    text.replace('`', "")
}

/// Whether `text` leaves a code span open.
pub(crate) fn unbalanced(text: &str) -> bool {
    text.matches('`').count() % 2 == 1
}

/// A word of a sentence, or a code span; `glued` when nothing parts it from the one
/// before (a full stop after a chip), so a line never breaks between them.
struct Token<'a> {
    text: &'a str,
    code: bool,
    glued: bool,
}

/// `sentence`'s words and code spans: what backticks enclose is code, kept whole.
fn tokens(sentence: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    // Whether a space came since the last token.
    let mut spaced = true;
    for (index, part) in sentence.split('`').enumerate() {
        if index % 2 == 1 {
            let code = part.trim();
            if !code.is_empty() {
                tokens.push(Token {
                    text: code,
                    code: true,
                    glued: !spaced,
                });
                spaced = false;
            }
            continue;
        }
        let mut rest = part;
        loop {
            let word = rest.trim_start();
            spaced |= word.len() != rest.len();
            if word.is_empty() {
                break;
            }
            let end = word.find(char::is_whitespace).unwrap_or(word.len());
            tokens.push(Token {
                text: &word[..end],
                code: false,
                glued: !spaced,
            });
            spaced = false;
            rest = &word[end..];
        }
    }
    tokens
}

/// `sentence` wrapped into rows `width` wide (frame pixels) as `measures` draw it: at
/// spaces, never inside a code span or between glued tokens; a group wider than a row
/// stands alone. Neighbouring words share one run. Always at least one row.
pub(crate) fn flow(sentence: &str, width: f32, measures: &Measures<'_>, look: &Look) -> Vec<Row> {
    let space = measures.body.width(" ", look.size, TextFace::Regular);
    let token_width = |token: &Token<'_>| {
        if token.code {
            measures
                .code
                .width(token.text, look.chip_size, TextFace::Semibold)
                + look.chip_pad * 2.0
        } else {
            measures
                .body
                .width(token.text, look.size, TextFace::Regular)
        }
    };
    let tokens = tokens(sentence);
    let mut rows: Vec<Row> = vec![Vec::new()];
    let mut x = 0.0;
    let mut start = 0;
    while start < tokens.len() {
        // The group: this token and those glued after it.
        let end = tokens[start + 1..]
            .iter()
            .position(|token| !token.glued)
            .map_or(tokens.len(), |at| start + 1 + at);
        let group = &tokens[start..end];
        // A chip and the punctuation glued to it keep a hair apart.
        let glue = |index: usize| {
            if index > 0 && (group[index].code || group[index - 1].code) {
                GLUE
            } else {
                0.0
            }
        };
        let group_width: f32 = group
            .iter()
            .enumerate()
            .map(|(index, token)| glue(index) + token_width(token))
            .sum();
        let mut gap = if x > 0.0 { space } else { 0.0 };
        if x > 0.0 && x + gap + group_width > width {
            rows.push(Vec::new());
            x = 0.0;
            gap = 0.0;
        }
        let row = rows.last_mut().expect("one row at least");
        for (index, token) in group.iter().enumerate() {
            let token_gap = if index == 0 { gap } else { glue(index) };
            let token_width = token_width(token);
            match row.last_mut() {
                Some(run) if !run.code && !token.code => {
                    if token_gap > 0.0 {
                        run.text.push(' ');
                    }
                    run.text.push_str(token.text);
                    run.width += token_gap + token_width;
                }
                _ => row.push(Run {
                    x: x + token_gap,
                    width: token_width,
                    text: token.text.to_owned(),
                    code: token.code,
                }),
            }
            x += token_gap + token_width;
        }
        start = end;
    }
    rows
}

/// Where and how [`draw_rows`] draws.
pub(crate) struct Place {
    /// The text's left edge and the first line's top, in frame pixels.
    pub(crate) x: f32,
    pub(crate) y: f32,
    /// The plain text's colour.
    pub(crate) colour: Color,
    /// The family chips' text is drawn in.
    pub(crate) code_family: TextFamily,
}

/// Draw wrapped `rows`, plain text in `place.colour` and code in chips. With `chip_token`,
/// each chip answers to the next token from it (in reading order) and lights while
/// hovered.
pub(crate) fn draw_rows(
    ui: &mut MenuCanvas,
    frame: &Frame,
    rows: &[Row],
    look: &Look,
    place: &Place,
    mut chip_token: Option<&mut u16>,
) {
    let s = frame.s;
    for (index, row) in rows.iter().enumerate() {
        let line_y = place.y + index as f32 * look.line;
        for run in row {
            if !run.code {
                // A little room past the measured end, so no letter is clipped.
                text(
                    ui,
                    TextFamily::Body,
                    format_args!("{}", run.text),
                    frame.rect(place.x + run.x, line_y, run.width + 8.0, look.line),
                    look.size * s,
                    place.colour,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                continue;
            }
            let chip = frame.rect(
                place.x + run.x,
                line_y + (look.line - look.chip_height) * 0.5,
                run.width,
                look.chip_height,
            );
            let token = chip_token.as_deref_mut().map(|next| {
                let token = *next;
                *next += 1;
                token
            });
            let hovered = token.is_some_and(|token| ui.token_hovered(token));
            if let Some(token) = token {
                ui.hit_region(token, chip);
            }
            let _ = ui.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: chip,
                radius: look.chip_radius * s,
                color: color::alpha(color::HOLO, if hovered { 0.24 } else { 0.12 }),
            });
            let _ = ui.draw_list_mut().push(DrawCommand::Border {
                rect: chip,
                radius: look.chip_radius * s,
                width: s.max(1.0),
                color: if hovered {
                    color::GOLD_BRIGHT
                } else {
                    color::alpha(color::HOLO, 0.4)
                },
            });
            text(
                ui,
                place.code_family,
                format_args!("{}", run.text),
                frame.rect(place.x + run.x, line_y, run.width, look.line),
                look.chip_size * s,
                color::TEXT,
                FontWeight::Semibold,
                TextAlign::Center,
            );
        }
    }
}

/// The chips of `rows`, in the order [`draw_rows`] gives them tokens.
pub(crate) fn chips(rows: &[Row]) -> impl Iterator<Item = &str> {
    rows.iter()
        .flatten()
        .filter(|run| run.code)
        .map(|run| run.text.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{TextStyle, test_font};

    const LOOK: Look = Look {
        size: 18.0,
        line: 28.0,
        chip_size: 17.0,
        chip_pad: 7.0,
        chip_height: 25.0,
        chip_radius: 6.0,
    };

    #[test]
    fn code_in_backticks_becomes_chips_that_are_never_cut() {
        // Test glyphs advance 8 at size 12: 12 a character at 18, 11.33 at 17.
        let font = test_font();
        let measure = Measure::new(&font, TextStyle::NEUTRAL);
        let measures = Measures {
            body: measure,
            code: measure,
        };
        let rows = flow(
            "Type `whatsnew`, or `cl_allowDownload 1`.",
            800.0,
            &measures,
            &LOOK,
        );
        assert_eq!(rows.len(), 1);
        let texts: Vec<(&str, bool)> = rows[0]
            .iter()
            .map(|run| (run.text.as_str(), run.code))
            .collect();
        assert_eq!(
            texts,
            [
                ("Type", false),
                ("whatsnew", true),
                (", or", false),
                ("cl_allowDownload 1", true),
                (".", false),
            ]
        );
        let chip = &rows[0][1];
        let letters = 8.0 * 8.0 * LOOK.chip_size / 12.0;
        assert!(
            (chip.width - letters - LOOK.chip_pad * 2.0).abs() < 0.01,
            "{chip:?}"
        );
        // The comma is glued to the chip and shares a run with the next word; the next
        // chip is a space after it.
        assert_eq!(rows[0][2].x, chip.x + chip.width + GLUE);
        assert_eq!(rows[0][3].x, rows[0][2].x + rows[0][2].width + 12.0);
        assert_eq!(
            chips(&rows).collect::<Vec<_>>(),
            ["whatsnew", "cl_allowDownload 1"]
        );

        // A chip that does not fit the rest of a row moves to the next one whole, with
        // the full stop glued to it.
        let narrow = flow("aaaa `bbbb cccc`.", 150.0, &measures, &LOOK);
        assert_eq!(narrow.len(), 2, "{narrow:?}");
        assert_eq!(narrow[0][0].text, "aaaa");
        assert_eq!(narrow[1][0].text, "bbbb cccc");
        assert!(narrow[1][0].code);
        assert_eq!(narrow[1][1].text, ".");
    }

    #[test]
    fn words_wrap_at_the_width_and_an_empty_text_is_one_row() {
        let font = test_font();
        let measure = Measure::new(&font, TextStyle::NEUTRAL);
        let measures = Measures {
            body: measure,
            code: measure,
        };
        // Words of 48 with spaces of 12: 14 fit 832.
        let rows = flow(&"word ".repeat(30), 832.0, &measures, &LOOK);
        assert_eq!(rows.iter().map(Vec::len).collect::<Vec<_>>(), [1, 1, 1]);
        assert!(
            rows.iter()
                .all(|row| row[0].x == 0.0 && row[0].width <= 832.0)
        );
        assert_eq!(flow("", 100.0, &measures, &LOOK), [Vec::<Run>::new()]);
    }

    #[test]
    fn plain_text_and_open_spans() {
        assert_eq!(plain("a `b` c"), "a b c");
        assert!(unbalanced("a `b"));
        assert!(!unbalanced("a `b` c"));
    }
}
