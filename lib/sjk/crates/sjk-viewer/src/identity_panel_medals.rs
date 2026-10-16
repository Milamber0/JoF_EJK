//! The player's own medals on the Identity page (`medals.rs`, `docs/identity.md`
//! "Medals"): each one's whole picture, its name (with how often a repeatable one was
//! given), what it is for, when it was given and the SJK team's note, beside the page's
//! column; or a line saying how medals come when there are none. The SJK UI puts them
//! in a column right of its own; the classic+ box names them in a line of its status
//! with their small medallions.

use super::*;
use crate::medals::Award;
use crate::menu::sjk::{Frame, color, kit, text, wrap};
use crate::menu_widgets::TextFamily;
use sjk_ui::DrawCommand;

/// What the page says when the player holds no medal yet.
pub(super) const NONE_YET: &str =
    "No medals yet. The SJK team gives medals for testing, contributing and more.";

/// The SJK UI's medal column, right of the page's (reference units).
const SJK_X: f32 = 1_460.0;
const SJK_WIDTH: f32 = 364.0;
const SJK_ART: f32 = 104.0;
/// The SJK UI's lines: the name's, then the smaller ones'.
const SJK_NAME_LINE: f32 = 32.0;
const SJK_LINE: f32 = 24.0;

/// One medal with its description and note broken into lines.
struct Laid<'a> {
    award: &'a Award,
    description: Vec<String>,
    note: Vec<String>,
    given: String,
}

/// `medals` with their texts broken into the lines `fits` accepts; a note's lines leave
/// room for its quotes.
fn lay_out<'a>(medals: &'a [Award], fits: &dyn Fn(&str) -> bool) -> Vec<Laid<'a>> {
    medals
        .iter()
        .map(|award| Laid {
            award,
            description: wrap_fitting(award.medal.description(), fits),
            note: wrap_fitting(&award.note, &|line: &str| fits(&format!("\"{line}\""))),
            given: award.given(),
        })
        .collect()
}

/// `text` broken at spaces into lines `fits` accepts; a word too long for a line has
/// one to itself.
fn wrap_fitting(text: &str, fits: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let longer = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if line.is_empty() || fits(&longer) {
            line = longer;
        } else {
            lines.push(std::mem::replace(&mut line, word.to_owned()));
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// A test of whether a line fits `chars` characters.
fn chars_fit(chars: usize) -> impl Fn(&str) -> bool {
    move |line: &str| line.chars().count() <= chars
}

impl Laid<'_> {
    /// The lines under the name: the description, the date, the note.
    fn lines(&self) -> usize {
        self.description.len() + usize::from(!self.given.is_empty()) + self.note.len()
    }
}

/// The classic+ box's line about the player's medals, `None` before the hub answered.
pub(super) fn classic_line(medals: Option<&[Award]>) -> Option<String> {
    let medals = medals?;
    if medals.is_empty() {
        return Some(NONE_YET.to_owned());
    }
    let names: Vec<String> = medals.iter().map(Award::label).collect();
    Some(format!("Medals: {}", names.join(", ")))
}

impl Panel {
    /// The SJK UI's Medals column, right of the page's, down to `bottom`.
    pub(super) fn sjk_medals(&mut self, frame: &Frame, medals: &[Award], top: f32, bottom: f32) {
        let s = frame.s;
        kit::heading(&mut self.ui, frame, SJK_X, top + 20.0, SJK_WIDTH, "Medals");
        let mut y = top + 48.0;
        if medals.is_empty() {
            for part in wrap(NONE_YET, 40) {
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{part}"),
                    frame.rect(SJK_X, y, SJK_WIDTH, 26.0),
                    17.0 * s,
                    color::MUTED,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                y += 26.0;
            }
            return;
        }
        let text_x = SJK_X + SJK_ART + 14.0;
        let text_width = SJK_WIDTH - SJK_ART - 14.0;
        // Exo 2 at 16 is about 8 pixels a character.
        for medal in lay_out(medals, &chars_fit((text_width / 8.0) as usize)) {
            let height = (SJK_NAME_LINE + medal.lines() as f32 * SJK_LINE).max(SJK_ART);
            if y + height > bottom {
                break;
            }
            let _ = self.ui.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: frame.rect(SJK_X, y, SJK_ART, SJK_ART),
                texture: medal.award.medal.art(),
                color: Color::new(1.0, 1.0, 1.0, 1.0),
            });
            let mut line_y = y;
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}", medal.award.label()),
                frame.rect(text_x, line_y, text_width, SJK_NAME_LINE),
                24.0 * s,
                color::GOLD_BRIGHT,
                FontWeight::Semibold,
                TextAlign::Start,
            );
            line_y += SJK_NAME_LINE;
            let lines = medal
                .description
                .iter()
                .map(|part| (part.as_str(), color::MUTED))
                .chain((!medal.given.is_empty()).then_some((medal.given.as_str(), color::QUIET)));
            for (part, colour) in lines {
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{part}"),
                    frame.rect(text_x, line_y, text_width, SJK_LINE),
                    16.0 * s,
                    colour,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                line_y += SJK_LINE;
            }
            let last = medal.note.len().saturating_sub(1);
            for (index, part) in medal.note.iter().enumerate() {
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!(
                        "{}{part}{}",
                        if index == 0 { "\"" } else { "" },
                        if index == last { "\"" } else { "" }
                    ),
                    frame.rect(text_x, line_y, text_width, SJK_LINE),
                    16.0 * s,
                    color::TEXT,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                line_y += SJK_LINE;
            }
            y += height + 22.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::medals::Medal;

    fn award(medal: Medal, count: u32, note: &str) -> Award {
        Award {
            medal,
            count,
            awarded: 1_791_336_225,
            note: note.to_owned(),
        }
    }

    #[test]
    fn the_classic_line_names_the_medals_or_says_how_they_come() {
        assert_eq!(classic_line(None), None);
        assert_eq!(classic_line(Some(&[])).as_deref(), Some(NONE_YET));
        let medals = [
            award(Medal::EarlyTester, 1, ""),
            award(Medal::BugHunter, 3, ""),
        ];
        assert_eq!(
            classic_line(Some(&medals)).as_deref(),
            Some("Medals: Early Tester, Bug Hunter x3")
        );
    }

    #[test]
    fn a_medal_lays_out_its_description_date_and_note() {
        let medals = [award(
            Medal::BugHunter,
            2,
            "Found the fog that followed the camera floor and the flickering door",
        )];
        let laid = lay_out(&medals, &chars_fit(30));
        assert_eq!(laid[0].description, ["Found bugs that got fixed."]);
        assert_eq!(laid[0].given, "Given 07/10/2026");
        assert!(laid[0].note.len() >= 2);
        assert!(laid[0].note.iter().all(|line| line.chars().count() <= 28));
        assert_eq!(
            wrap_fitting("a b c", &chars_fit(3)),
            ["a b", "c"],
            "greedy, at spaces"
        );
        assert_eq!(wrap_fitting("unbreakable", &chars_fit(3)), ["unbreakable"]);
        assert_eq!(laid[0].lines(), 1 + 1 + laid[0].note.len());
    }
}
