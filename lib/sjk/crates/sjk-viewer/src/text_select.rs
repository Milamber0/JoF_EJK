//! Mouse text selection over lines a [`MenuCanvas`] page draws: a press on a
//! line puts the selection's start there, dragging moves its end across the
//! lines, a double click selects the word under the pointer, and
//! [`TextSelect::selected`] gives the text to copy (Ctrl+C), without colour
//! codes. The command browser's detail column uses it (`docs/client.md`,
//! Console styles).
//!
//! Each frame the page registers its selectable lines in drawing order
//! ([`TextSelect::line`]), with the measure the text is drawn with; the
//! selection's highlight is drawn there, under the text. The caret stops are
//! kept until the next frame, so pointer events resolve at once against what
//! is on screen. The selection holds line numbers and byte offsets: it stays
//! while the page draws the same content (its key, [`TextSelect::begin`]) and
//! is dropped when the content changes.

use crate::console::edit_view::walk;
use crate::console::line_edit::token_at;
use crate::menu_widgets::MenuCanvas;
use sjk_ui::{Color, InputEvent, PointerButton, Rect, Vec2};
use std::ops::Range;
use std::time::{Duration, Instant};

/// A second press this soon and this close to the first is a double click.
const DOUBLE_CLICK: Duration = Duration::from_millis(500);
const DOUBLE_CLICK_DISTANCE: f32 = 6.0;

/// How a line joins the next one in copied text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Joiner {
    /// The next line goes on: a paragraph wrapped onto several lines.
    Space,
    /// The next line is another piece of text.
    Newline,
}

/// A place in the registered lines: a line and a byte offset in its text.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Place {
    line: usize,
    byte: usize,
}

/// One registered line.
#[derive(Clone, Debug)]
struct Line {
    /// Where the pointer reaches it: its row across the selectable area.
    band: Rect,
    /// Its text in [`TextSelect::text`] and its stops in [`TextSelect::stops`].
    text: Range<usize>,
    stops: Range<usize>,
    joiner: Joiner,
}

/// The selection over a page's lines and the gesture editing it.
#[derive(Default)]
pub(crate) struct TextSelect {
    lines: Vec<Line>,
    /// Caret stops of every line: x on screen and byte offset.
    stops: Vec<(f32, usize)>,
    text: String,
    anchor: Option<Place>,
    head: Option<Place>,
    dragging: bool,
    last_press: Option<(Instant, Vec2)>,
    /// What the page draws: the selection belongs to this content.
    key: u64,
}

impl TextSelect {
    /// Start a frame drawing the content `key` names; a different key drops
    /// the selection.
    pub(crate) fn begin(&mut self, key: u64) {
        if key != self.key {
            self.key = key;
            self.clear();
        }
        self.lines.clear();
        self.stops.clear();
        self.text.clear();
    }

    /// Drop the selection and any gesture.
    pub(crate) fn clear(&mut self) {
        self.anchor = None;
        self.head = None;
        self.dragging = false;
    }

    /// Register `text`, drawn from `x` with glyph advances `advance`, its
    /// pointer row `band` (its height the line's, its width what the pointer
    /// may cover); draw the selection's highlight on it in `highlight`. Call it
    /// before drawing the text.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn line(
        &mut self,
        canvas: &mut MenuCanvas,
        band: Rect,
        x: f32,
        text: &str,
        advance: impl Fn(crate::text::GlyphKey) -> f32,
        joiner: Joiner,
        highlight: Color,
    ) {
        let index = self.lines.len();
        let first_stop = self.stops.len();
        walk(text, advance, |byte, pen, _| {
            self.stops.push((x + pen, byte));
            false
        });
        let start = self.text.len();
        self.text.push_str(text);
        self.lines.push(Line {
            band,
            text: start..self.text.len(),
            stops: first_stop..self.stops.len(),
            joiner,
        });
        let Some((from, to)) = self.range() else {
            return;
        };
        if index < from.line || index > to.line {
            return;
        }
        let stops = &self.stops[first_stop..];
        let x_of = |byte: usize| {
            stops
                .iter()
                .find(|(_, at)| *at >= byte)
                .or(stops.last())
                .map_or(x, |(x, _)| *x)
        };
        let left = if index == from.line {
            x_of(from.byte)
        } else {
            x
        };
        let mut right = if index == to.line {
            x_of(to.byte)
        } else {
            x_of(text.len())
        };
        // A selection running on shows the line's end as a little room.
        if index < to.line {
            right += band.height * 0.25;
        }
        if right > left {
            canvas.accent_bar(
                Rect::new(left, band.y, right - left, band.height),
                highlight,
            );
        }
    }

    /// Handle a pointer event; `true` when it was the selection's (a press on
    /// a line, or the drag and release that follow it). A press elsewhere drops
    /// the selection and is left to the page.
    pub(crate) fn pointer(&mut self, event: InputEvent) -> bool {
        match event {
            InputEvent::PointerPress {
                position,
                button: PointerButton::Primary,
            } => {
                let Some(line) = self
                    .lines
                    .iter()
                    .position(|line| line.band.contains(position))
                else {
                    self.clear();
                    return false;
                };
                let now = Instant::now();
                let double = self.last_press.is_some_and(|(time, last)| {
                    now.duration_since(time) <= DOUBLE_CLICK
                        && (last.x - position.x).abs() <= DOUBLE_CLICK_DISTANCE
                        && (last.y - position.y).abs() <= DOUBLE_CLICK_DISTANCE
                });
                self.last_press = (!double).then_some((now, position));
                let at = self.place_in(line, position.x);
                if double {
                    let token = token_at(self.line_text(line), at.byte);
                    self.anchor = Some(Place {
                        line,
                        byte: token.start,
                    });
                    self.head = Some(Place {
                        line,
                        byte: token.end,
                    });
                    self.dragging = false;
                } else {
                    self.anchor = Some(at);
                    self.head = Some(at);
                    self.dragging = true;
                }
                true
            }
            InputEvent::PointerMove(position) if self.dragging => {
                self.head = self.place_at(position).or(self.head);
                true
            }
            InputEvent::PointerRelease {
                position,
                button: PointerButton::Primary,
            } if self.dragging => {
                self.head = self.place_at(position).or(self.head);
                self.dragging = false;
                true
            }
            _ => false,
        }
    }

    /// The selected text, lines joined as registered and colour codes left
    /// out; `None` when nothing is selected.
    pub(crate) fn selected(&self) -> Option<String> {
        let (from, to) = self.range()?;
        let mut out = String::new();
        for index in from.line..=to.line.min(self.lines.len().saturating_sub(1)) {
            let text = self.line_text(index);
            let start = if index == from.line {
                from.byte.min(text.len())
            } else {
                0
            };
            let end = if index == to.line {
                to.byte.min(text.len())
            } else {
                text.len()
            };
            let start = crate::console::floor_boundary(text, start);
            let end = crate::console::floor_boundary(text, end);
            if start < end {
                crate::console::push_uncoloured(&text[start..end], &mut out);
            }
            // No joiner before a last line the selection takes nothing of.
            if index < to.line && (index + 1 < to.line || to.byte > 0) {
                out.push(match self.lines[index].joiner {
                    Joiner::Space => ' ',
                    Joiner::Newline => '\n',
                });
            }
        }
        (!out.is_empty()).then_some(out)
    }

    /// The selection, start first, if it is not empty.
    fn range(&self) -> Option<(Place, Place)> {
        let (anchor, head) = (self.anchor?, self.head?);
        (anchor != head).then(|| (anchor.min(head), anchor.max(head)))
    }

    fn line_text(&self, line: usize) -> &str {
        self.lines
            .get(line)
            .map_or("", |line| &self.text[line.text.clone()])
    }

    /// The caret stop of `line` nearest `x`.
    fn place_in(&self, line: usize, x: f32) -> Place {
        let stops = &self.stops[self.lines[line].stops.clone()];
        let byte = stops
            .windows(2)
            .find(|pair| x < (pair[0].0 + pair[1].0) * 0.5)
            .map_or_else(|| stops.last().map_or(0, |stop| stop.1), |pair| pair[0].1);
        Place { line, byte }
    }

    /// Where a drag at `position` reaches: the line it is on, or above the
    /// lines their start, below them their end, between two the nearer.
    fn place_at(&self, position: Vec2) -> Option<Place> {
        let first = self.lines.first()?;
        let last = self.lines.len() - 1;
        if position.y < first.band.y {
            return Some(Place { line: 0, byte: 0 });
        }
        let line = self
            .lines
            .iter()
            .position(|line| position.y < line.band.bottom())
            .unwrap_or(last);
        if position.y >= self.lines[last].band.bottom() {
            return Some(Place {
                line: last,
                byte: self.line_text(last).len(),
            });
        }
        Some(self.place_in(line, position.x))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two lines of ten-pixel glyphs, the first wrapping into the second.
    fn page(select: &mut TextSelect, canvas: &mut MenuCanvas) {
        select.begin(1);
        let highlight = Color::new(1.0, 0.8, 0.3, 0.3);
        for (row, text) in ["say hello", "^1red world"].iter().enumerate() {
            select.line(
                canvas,
                Rect::new(0.0, row as f32 * 20.0, 400.0, 20.0),
                100.0,
                text,
                |_| 10.0,
                Joiner::Space,
                highlight,
            );
        }
    }

    fn press(x: f32, y: f32) -> InputEvent {
        InputEvent::PointerPress {
            position: Vec2::new(x, y),
            button: PointerButton::Primary,
        }
    }

    #[test]
    fn a_drag_selects_across_lines_without_colour_codes() {
        let mut select = TextSelect::default();
        let mut canvas = MenuCanvas::new();
        page(&mut select, &mut canvas);
        // From before "hello" to after "red".
        assert!(select.pointer(press(141.0, 5.0)));
        assert!(select.pointer(InputEvent::PointerMove(Vec2::new(131.0, 25.0))));
        assert!(select.pointer(InputEvent::PointerRelease {
            position: Vec2::new(131.0, 25.0),
            button: PointerButton::Primary,
        }));
        assert_eq!(select.selected().as_deref(), Some("hello red"));
        // Past the lines' end, everything after the start (a press away from
        // the last one, which would be a double click).
        select.pointer(press(101.0, 25.0));
        select.pointer(InputEvent::PointerMove(Vec2::new(0.0, 300.0)));
        assert_eq!(select.selected().as_deref(), Some("red world"));
        // Above the lines, everything before it.
        select.pointer(InputEvent::PointerMove(Vec2::new(0.0, -10.0)));
        assert_eq!(select.selected().as_deref(), Some("say hello"));
    }

    #[test]
    fn a_double_click_selects_a_word_and_new_content_drops_it() {
        let mut select = TextSelect::default();
        let mut canvas = MenuCanvas::new();
        page(&mut select, &mut canvas);
        select.pointer(press(165.0, 5.0));
        select.pointer(InputEvent::PointerRelease {
            position: Vec2::new(165.0, 5.0),
            button: PointerButton::Primary,
        });
        select.pointer(press(165.0, 5.0));
        assert_eq!(select.selected().as_deref(), Some("hello"));
        // The same content keeps it, another drops it.
        page(&mut select, &mut canvas);
        assert_eq!(select.selected().as_deref(), Some("hello"));
        select.begin(2);
        assert_eq!(select.selected(), None);
    }

    #[test]
    fn a_press_off_the_lines_is_the_pages_and_clears() {
        let mut select = TextSelect::default();
        let mut canvas = MenuCanvas::new();
        page(&mut select, &mut canvas);
        select.pointer(press(141.0, 5.0));
        select.pointer(InputEvent::PointerMove(Vec2::new(191.0, 5.0)));
        assert_eq!(select.selected().as_deref(), Some("hello"));
        assert!(!select.pointer(press(10.0, 200.0)));
        assert_eq!(select.selected(), None);
    }
}
