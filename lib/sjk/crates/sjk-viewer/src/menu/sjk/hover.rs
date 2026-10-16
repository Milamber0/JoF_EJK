//! The SJK UI's hover card: a small card beside the thing the pointer rests on, saying
//! what it is and how it is obtained (`docs/sjk-ui.md`, "Hover cards"). It comes after
//! the pointer has rested a moment ([`DELAY`]) and fades in; moving to another thing
//! starts again, and a key hides it until the pointer moves. It never takes input.
//!
//! The card lies beside its thing, on the right when there is room, else on the left,
//! kept inside the frame. Text draws over every shape, so the card covers the page's
//! text under it ([`MenuCanvas::cover`]); a screen draws it last.

use super::{Frame, color, text, wrap};
use crate::menu_widgets::{MenuCanvas, MenuToken, TextFamily};
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};
use std::time::{Duration, Instant};

/// How long the pointer rests on a thing before its card comes.
pub(crate) const DELAY: Duration = Duration::from_millis(300);
/// How long the card takes to fade in.
const FADE: f32 = 0.12;
/// The card's width, its padding and the gap to its thing (frame pixels).
const WIDTH: f32 = 400.0;
const PAD: f32 = 20.0;
const GAP: f32 = 14.0;
/// The frame's margin the card keeps.
const MARGIN: f32 = 16.0;
/// The title's and the lines' sizes and steps.
const TITLE: f32 = 23.0;
const TITLE_STEP: f32 = 32.0;
const BODY: f32 = 16.0;
const LINE_STEP: f32 = 22.0;
/// Space between two lines of the card.
const LINE_GAP: f32 = 6.0;
/// Characters a body line holds at the plain text size.
const LINE_CHARS: f32 = 54.0;

/// One line of a card: its words and their colour.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Line {
    pub(crate) words: String,
    pub(crate) colour: Color,
}

impl Line {
    /// The card's main words: how the thing is obtained.
    pub(crate) fn how(words: impl Into<String>) -> Self {
        Self {
            words: words.into(),
            colour: color::TEXT,
        }
    }

    /// A quieter line: where the player stands with it.
    pub(crate) fn status(words: impl Into<String>) -> Self {
        Self {
            words: words.into(),
            colour: color::MUTED,
        }
    }

    /// A line in gold: the player holds it.
    pub(crate) fn held(words: impl Into<String>) -> Self {
        Self {
            words: words.into(),
            colour: color::GOLD_BRIGHT,
        }
    }
}

/// What a card says: a title in the thing's colour, then its lines.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Card {
    pub(crate) title: String,
    pub(crate) colour: Color,
    pub(crate) lines: Vec<Line>,
}

/// Which thing the pointer rests on and since when.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Hover {
    token: Option<MenuToken>,
    since: Instant,
    /// A key was pressed: no card until the pointer moves.
    hidden: bool,
    /// A world shot's thing, whose card shows in full whatever the pointer does.
    #[cfg(test)]
    pinned: Option<MenuToken>,
}

impl Default for Hover {
    fn default() -> Self {
        Self {
            token: None,
            since: Instant::now(),
            hidden: false,
            #[cfg(test)]
            pinned: None,
        }
    }
}

impl Hover {
    /// Follow the token the pointer rests on this frame.
    pub(crate) fn follow(&mut self, hovered: Option<MenuToken>, now: Instant) {
        if hovered != self.token {
            self.token = hovered;
            self.since = now;
        }
    }

    /// The pointer moved: cards may show again.
    pub(crate) fn moved(&mut self) {
        self.hidden = false;
    }

    /// A key was pressed: hide the card until the pointer moves.
    pub(crate) fn keyed(&mut self) {
        self.hidden = true;
    }

    /// Show `token`'s card in full whatever the pointer does (`None` lets go), for a
    /// world shot.
    #[cfg(test)]
    pub(crate) fn pin(&mut self, token: Option<MenuToken>) {
        self.pinned = token;
    }

    /// The thing whose card shows at `now` and how far it has faded in (0 to 1).
    pub(crate) fn showing(&self, now: Instant) -> Option<(MenuToken, f32)> {
        #[cfg(test)]
        if let Some(token) = self.pinned {
            return Some((token, 1.0));
        }
        if self.hidden {
            return None;
        }
        let token = self.token?;
        let rested = now.checked_duration_since(self.since)?;
        let into = rested.checked_sub(DELAY)?.as_secs_f32();
        Some((token, (into / FADE).min(1.0)))
    }
}

/// The lines of `card` as drawn, each with its colour, at the player's text size.
fn laid(card: &Card) -> Vec<(&str, Color, bool)> {
    let scale = crate::text::style::current().scale.max(0.5);
    let chars = (LINE_CHARS / scale).floor().max(12.0) as usize;
    let mut out = Vec::new();
    for (index, line) in card.lines.iter().enumerate() {
        for (part, words) in wrap(&line.words, chars).enumerate() {
            // A new line of the card, not a wrapped one, starts after a gap.
            out.push((words, line.colour, part == 0 && index > 0));
        }
    }
    out
}

/// The card's height for its laid lines (frame pixels).
fn height(laid: &[(&str, Color, bool)]) -> f32 {
    let gaps = laid.iter().filter(|(_, _, gap)| *gap).count() as f32;
    PAD + TITLE_STEP + laid.len() as f32 * LINE_STEP + gaps * LINE_GAP + PAD - 6.0
}

/// Where a card of `size` lies beside `anchor` (both frame pixels): on its right when
/// there is room, else on its left, kept inside the frame.
pub(crate) fn place(anchor: [f32; 4], size: [f32; 2]) -> [f32; 2] {
    let [x, y, width, _] = anchor;
    let right = x + width + GAP;
    let left = x - GAP - size[0];
    let x = if right + size[0] <= 1_920.0 - MARGIN {
        right
    } else if left >= MARGIN {
        left
    } else {
        (1_920.0 - MARGIN - size[0]).max(MARGIN)
    };
    let y = y.clamp(MARGIN, (1_080.0 - MARGIN - size[1]).max(MARGIN));
    [x, y]
}

/// Draw `card` beside `anchor` (window pixels, the thing's pointer area), faded in to
/// `alpha`.
pub(crate) fn draw(canvas: &mut MenuCanvas, frame: &Frame, anchor: Rect, card: &Card, alpha: f32) {
    let s = frame.s;
    let anchor = [
        (anchor.x - frame.origin[0]) / s,
        (anchor.y - frame.origin[1]) / s,
        anchor.width / s,
        anchor.height / s,
    ];
    let laid = laid(card);
    let size = [WIDTH, height(&laid)];
    let [x, y] = place(anchor, size);
    canvas.cover(frame.rect(x, y, size[0], size[1]));
    canvas.push_opacity(alpha);
    let push = |canvas: &mut MenuCanvas, command| {
        let _ = canvas.draw_list_mut().push(command);
    };
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect: frame.rect(x + 3.0, y + 8.0, size[0], size[1]),
            radius: 12.0 * s,
            color: color::alpha(color::SPACE, 0.55),
        },
    );
    push(
        canvas,
        DrawCommand::RoundedRect {
            rect: frame.rect(x, y, size[0], size[1]),
            radius: 12.0 * s,
            color: Color::new(0.04, 0.06, 0.12, 0.97),
        },
    );
    push(
        canvas,
        DrawCommand::Border {
            rect: frame.rect(x, y, size[0], size[1]),
            radius: 12.0 * s,
            width: 1.5 * s,
            color: Color::new(card.colour.r, card.colour.g, card.colour.b, 0.55),
        },
    );
    text(
        canvas,
        TextFamily::Display,
        format_args!("{}", card.title),
        frame.rect(x + PAD, y + PAD - 4.0, WIDTH - PAD * 2.0, TITLE_STEP),
        TITLE * s,
        card.colour,
        FontWeight::Semibold,
        TextAlign::Start,
    );
    let mut line_y = y + PAD + TITLE_STEP;
    for (words, colour, gap) in laid {
        if gap {
            line_y += LINE_GAP;
        }
        text(
            canvas,
            TextFamily::Body,
            format_args!("{words}"),
            frame.rect(x + PAD, line_y, WIDTH - PAD * 2.0, LINE_STEP),
            BODY * s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
        line_y += LINE_STEP;
    }
    canvas.pop_opacity();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_waits_then_fades_in_and_a_key_hides_it() {
        let t0 = Instant::now();
        let mut hover = Hover::default();
        hover.follow(Some(7), t0);
        assert_eq!(hover.showing(t0), None);
        assert_eq!(hover.showing(t0 + DELAY / 2), None);
        let (token, alpha) = hover.showing(t0 + DELAY).unwrap();
        assert_eq!((token, alpha), (7, 0.0));
        assert_eq!(hover.showing(t0 + DELAY * 3).unwrap().1, 1.0);
        // The same thing keeps its time; another starts again.
        hover.follow(Some(7), t0 + DELAY * 3);
        assert!(hover.showing(t0 + DELAY * 3).is_some());
        hover.follow(Some(8), t0 + DELAY * 3);
        assert_eq!(hover.showing(t0 + DELAY * 3), None);
        hover.keyed();
        assert_eq!(hover.showing(t0 + DELAY * 10), None);
        hover.moved();
        assert!(hover.showing(t0 + DELAY * 10).is_some());
        hover.follow(None, t0 + DELAY * 10);
        assert_eq!(hover.showing(t0 + DELAY * 20), None);
    }

    #[test]
    fn the_card_lies_beside_its_thing_inside_the_frame() {
        // Room on the right.
        assert_eq!(
            place([100.0, 300.0, 200.0, 80.0], [440.0, 150.0]),
            [314.0, 300.0]
        );
        // None on the right: on the left.
        assert_eq!(
            place([1_500.0, 300.0, 200.0, 80.0], [440.0, 150.0]),
            [1_046.0, 300.0]
        );
        // Low on the screen: raised to stay inside.
        assert_eq!(
            place([100.0, 1_000.0, 200.0, 60.0], [440.0, 150.0])[1],
            1_080.0 - 16.0 - 150.0
        );
    }

    #[test]
    fn long_lines_wrap_and_new_lines_start_after_a_gap() {
        let card = Card {
            title: "Rare card".to_owned(),
            colour: color::HOLO,
            lines: vec![
                Line::how(
                    "Opening a holocron gives one sealed card; this kind comes in about one holocron in fifty.",
                ),
                Line::status("You hold 2."),
            ],
        };
        let laid = laid(&card);
        assert!(laid.len() >= 3, "{laid:?}");
        assert!(!laid[0].2 && !laid[1].2, "a wrapped line has no gap");
        assert!(laid.last().unwrap().2, "a new line has one");
        assert!(height(&laid) > PAD * 2.0 + TITLE_STEP + 2.0 * LINE_STEP);
    }
}
