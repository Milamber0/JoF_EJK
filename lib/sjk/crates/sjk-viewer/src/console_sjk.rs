//! The SJK UI's console, the deck (`con_style sjk`, what `auto` gives with the
//! SJK UI's menus; see `docs/client.md`, Console styles): the classic console's
//! grid, rows, keys, selection and notify lines ([`super`]) drawn in the SJK
//! UI's colours (`menu::sjk::color`) and type, without the `console` shader. A
//! full-width navy panel; a header with the console's name lit gold, the
//! version, the date and the time; a band for the input with the kit's gold
//! bar; key hints; a lit rail (gold fading into a holo line) along the bottom
//! edge.
//!
//! The scrollback and input are JetBrains Mono on the console's grid, its rows
//! a fifth further apart than the classic console's and their colour codes in
//! the SJK UI's legible palette; labels are in the UI's families (Rajdhani
//! and Exo 2), drawn on the console's layer ([`crate::console_backdrop`]) so
//! nothing shows through, or in the console font until the families load.

use super::{Grid, Ink, Layout, Painter, Prompt, ViewerConsole, cell_count, cell_width};
use crate::console::console_options::Options;
use crate::console_backdrop::{ConsoleFrame, Shade, SolidQuad};
use crate::menu::sjk::{BODY_CENTRE, DISPLAY_CENTRE, color};
use crate::text::{CodePalette, TextFace, UiFont, append_bounded, visible_text_width_style};
use sjk_ui::{Color, Rect, TextOverflow};
use std::fmt::Write as _;
use std::time::{Duration, Instant};

/// Row pitch as a multiple of the cell height: rows a fifth apart.
const PITCH: f32 = 1.2;
/// How long "Copied" shows after Ctrl+C.
const COPIED_FOR: Duration = Duration::from_millis(1_600);
/// The prompt's `›` (Windows-1252 0x9B).
const CHEVRON: u8 = 0x9b;

/// The SJK UI's text colours for the rows.
pub(super) const INK: Ink = Ink {
    text: rgba(color::TEXT, 1.0),
    error: [1.0, 0.56, 0.5, 1.0],
    stamp: rgba(color::QUIET, 1.0),
    highlight: rgba(color::GOLD, 0.34),
    palette: CodePalette::Legible,
};

/// The deck's input row: a gold `›` and the input two cells after it.
pub(super) const PROMPT: Prompt = Prompt {
    clock: false,
    mark: CHEVRON,
    mark_column: 0,
    mark_color: rgba(color::GOLD_BRIGHT, 1.0),
    input_column: 2,
    text_color: rgba(color::TEXT, 1.0),
    gold_caret: true,
};

/// `color` as a layer colour at `alpha`.
const fn rgba(color: Color, alpha: f32) -> [f32; 4] {
    [color.r, color.g, color.b, alpha]
}

/// The SJK UI's families for the deck's labels, or the console's own font
/// while they are not loaded.
#[derive(Clone, Copy)]
pub(crate) struct Labels<'a> {
    pub(crate) display: &'a UiFont,
    pub(crate) body: &'a UiFont,
    /// The families are loaded: labels go to the frame's family text runs.
    pub(crate) families: bool,
}

impl<'a> Labels<'a> {
    /// Labels in `font` alone (the families are not loaded).
    pub(crate) fn single(font: &'a UiFont) -> Self {
        Self {
            display: font,
            body: font,
            families: false,
        }
    }
}

/// Which family a label is in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Family {
    /// Rajdhani: names, the clock, key caps.
    Display,
    /// Exo 2: everything else.
    Body,
}

/// What the deck keeps between frames.
#[derive(Default)]
pub(crate) struct Chrome {
    /// The time Ctrl+C last copied, and how many characters.
    copied: Option<(Instant, usize)>,
    /// Where the command browser's hint was drawn: a click opens it.
    browser_hit: Option<Rect>,
    /// Formatted labels, kept to reuse their storage.
    scratch: String,
}

impl Chrome {
    /// Ctrl+C copied `characters` characters.
    pub(crate) fn note_copy(&mut self, characters: usize) {
        self.copied = Some((Instant::now(), characters));
    }

    /// Whether `position` is on the command browser's hint.
    pub(crate) fn opens_browser(&self, position: sjk_ui::Vec2) -> bool {
        self.browser_hit.is_some_and(|rect| rect.contains(position))
    }
}

/// The deck's grid in `viewport` at `con_scale` `scale`: classic cells, rows
/// [`PITCH`] apart, between the deck's margins.
pub(super) fn grid(viewport: [f32; 2], scale: f32) -> Grid {
    let width = cell_width(viewport, scale);
    let height = width * 2.0;
    let s = ui_scale(viewport);
    let (left, right) = (deck::MARGIN * s, (deck::MARGIN + GUTTER) * s);
    let left = left.round();
    Grid {
        width,
        height,
        columns: (((viewport[0] - left - right.round()) / width) as usize).max(1),
        left,
        pitch: (height * PITCH).round(),
    }
}

/// Room kept right of the rows for the scroll bar, in 1080-line pixels.
const GUTTER: f32 = 14.0;

/// The layout scale of the deck's chrome: 1 at 1080 lines, the console's
/// 0.75 floor below.
fn ui_scale(viewport: [f32; 2]) -> f32 {
    crate::ui_scale::height_scale(viewport[1]).max(0.75)
}

/// The text caret at `x` in the cell whose top is `y`: a thin gold bar, or a
/// translucent gold block in overstrike mode.
pub(super) fn caret(grid: &Grid, x: f32, y: f32, overstrike: bool) -> SolidQuad {
    if overstrike {
        SolidQuad {
            rect: [x, y, grid.width, grid.height],
            color: rgba(color::GOLD, 0.55),
        }
    } else {
        SolidQuad {
            rect: [
                x - (grid.width * 0.12).round(),
                y + grid.height * 0.08,
                (grid.width * 0.25).round().max(2.0),
                grid.height * 0.84,
            ],
            color: rgba(color::GOLD_BRIGHT, 1.0),
        }
    }
}

/// Chrome drawn into one frame: shapes, and labels in the families.
struct Chalk<'f, 'a> {
    frame: &'f mut ConsoleFrame,
    labels: Labels<'a>,
    viewport: [f32; 2],
    /// Layout scale.
    s: f32,
}

impl Chalk<'_, '_> {
    fn quad(&mut self, rect: [f32; 4], color: [f32; 4]) {
        self.frame.quads.push(SolidQuad { rect, color });
    }

    fn shade(&mut self, shade: Shade) {
        self.frame.shades.push(shade);
    }

    /// A hairline (`thickness` 1080-line pixels, at least one pixel) across
    /// `rect`'s top edge.
    fn rule(&mut self, x: f32, y: f32, width: f32, color: [f32; 4]) {
        let height = self.s.round().max(1.0);
        self.quad([x, y.round(), width, height], color);
    }

    /// A rectangle's outline, one pixel or more.
    fn outline(&mut self, [x, y, width, height]: [f32; 4], color: [f32; 4]) {
        let line = self.s.round().max(1.0);
        self.quad([x, y, width, line], color);
        self.quad([x, y + height - line, width, line], color);
        self.quad([x, y + line, line, height - line * 2.0], color);
        self.quad(
            [x + width - line, y + line, line, height - line * 2.0],
            color,
        );
    }

    /// `text` in `family` with its letters centred on `middle`, from `x` (or
    /// ending at `x` when `end`), at most `room` wide (cut with an ellipsis);
    /// returns its width.
    #[allow(clippy::too_many_arguments)]
    fn label(
        &mut self,
        family: Family,
        text: &str,
        x: f32,
        middle: f32,
        size: f32,
        color: [f32; 4],
        end: bool,
        room: f32,
    ) -> f32 {
        let Labels {
            display,
            body,
            families,
        } = self.labels;
        let (font, centre, face, vertices) = match (families, family) {
            (true, Family::Display) => (
                display,
                DISPLAY_CENTRE,
                TextFace::Semibold,
                &mut self.frame.display_text,
            ),
            (true, Family::Body) => (
                body,
                BODY_CENTRE,
                TextFace::Regular,
                &mut self.frame.body_text,
            ),
            (false, _) => (display, 0.5, TextFace::Regular, &mut self.frame.text),
        };
        if room <= 0.0 {
            return 0.0;
        }
        let scale = size / font.height.max(1.0);
        let width = visible_text_width_style(font, text, scale, face, 0.0).min(room);
        let left = if end { x - width } else { x };
        let top = middle - centre * size;
        // A pixel of slack so a text that just fits is not cut.
        let rect = Rect::new(left, top, width + 1.0, size * 1.3);
        append_bounded(
            vertices,
            font,
            text,
            [left, top],
            rect,
            Rect::new(0.0, 0.0, self.viewport[0], self.viewport[1]),
            scale,
            self.viewport,
            face,
            color,
            0.0,
            TextOverflow::Ellipsis,
            CodePalette::Legible,
        );
        width
    }

    /// One key cap centred on `middle` from `x`; returns the x after it.
    fn cap(&mut self, key: &str, x: f32, middle: f32) -> f32 {
        let s = self.s;
        let size = 15.0 * s;
        let height = (22.0 * s).round();
        let text = self.measure(Family::Display, key, size);
        let width = (text + 14.0 * s).max(height).round();
        let top = (middle - height * 0.5).round();
        self.quad([x, top, width, height], rgba(color::HOLO, 0.08));
        self.outline([x, top, width, height], rgba(color::HOLO, 0.42));
        self.label(
            Family::Display,
            key,
            x + (width - text) * 0.5,
            middle,
            size,
            rgba(color::TEXT, 1.0),
            false,
            width,
        );
        x + width
    }

    /// Key caps and what they do, from `x` on the line centred on `middle`;
    /// nothing past `limit`. Returns the x after it, and its rectangle.
    fn hint(
        &mut self,
        keys: &[&str],
        action: &str,
        x: f32,
        middle: f32,
        limit: f32,
    ) -> (f32, Rect) {
        let s = self.s;
        let size = 15.0 * s;
        let width = self.hint_width(keys, action);
        if x + width > limit {
            return (x, Rect::new(x, middle, 0.0, 0.0));
        }
        let mut pen = x;
        for key in keys {
            pen = self.cap(key, pen, middle) + 4.0 * s;
        }
        pen += 4.0 * s;
        pen += self.label(
            Family::Body,
            action,
            pen,
            middle,
            size,
            rgba(color::MUTED, 1.0),
            false,
            f32::MAX,
        );
        let height = 24.0 * s;
        (
            pen + 22.0 * s,
            Rect::new(x, middle - height * 0.5, pen - x, height),
        )
    }

    /// The width [`Self::hint`] takes, its gap after it included.
    fn hint_width(&self, keys: &[&str], action: &str) -> f32 {
        let s = self.s;
        let caps: f32 = keys
            .iter()
            .map(|key| {
                (self.measure(Family::Display, key, 15.0 * s) + 14.0 * s)
                    .max((22.0 * s).round())
                    .round()
                    + 4.0 * s
            })
            .sum();
        caps + 4.0 * s + self.measure(Family::Body, action, 15.0 * s) + 22.0 * s
    }

    /// The width of `text` in `family` at `size`.
    fn measure(&self, family: Family, text: &str, size: f32) -> f32 {
        let (font, face) = match (self.labels.families, family) {
            (true, Family::Display) => (self.labels.display, TextFace::Semibold),
            (true, Family::Body) => (self.labels.body, TextFace::Regular),
            (false, _) => (self.labels.display, TextFace::Regular),
        };
        visible_text_width_style(font, text, size / font.height.max(1.0), face, 0.0)
    }

    /// The console's keys as hints from `x`, as many as fit before `limit`;
    /// returns where the command browser's hint was drawn.
    fn console_hints(&mut self, x: f32, middle: f32, limit: f32) -> Option<Rect> {
        let mut pen = x;
        let mut browser_hit = None;
        let hints: [(&[&str], &str); 5] = [
            (&["Tab"], "Complete"),
            (&["Up", "Down"], "History"),
            (&["PgUp", "PgDn"], "Scroll"),
            (&["F3"], "Commands and cvars"),
            (&["Ctrl", "C"], "Copy"),
        ];
        for (keys, action) in hints {
            let (next, rect) = self.hint(keys, action, pen, middle, limit);
            if keys == ["F3"] && next > pen {
                browser_hit = Some(rect);
            }
            pen = next;
        }
        browser_hit
    }
}

impl ViewerConsole {
    /// Draw the deck's background and chrome for a console whose bottom edge is
    /// `lines` pixels down, and say where its rows and input row go.
    pub(super) fn sjk_chrome(
        &mut self,
        frame: &mut ConsoleFrame,
        painter: &Painter<'_>,
        labels: Labels<'_>,
        lines: f32,
        options: Options,
    ) -> Layout {
        let viewport = painter.viewport;
        // Below full height the panel follows `con_opacity`, as the classic
        // console's background does.
        let opacity = if lines >= viewport[1] {
            1.0
        } else {
            options.opacity
        };
        let mut chalk = Chalk {
            frame,
            labels,
            viewport,
            s: ui_scale(viewport),
        };
        let mut chrome = std::mem::take(&mut self.classic.sjk);
        chrome.browser_hit = None;
        let facts = Facts {
            clock: &self.classic.clock,
            corner: &self.classic.corner,
            copied: chrome
                .copied
                .filter(|(at, _)| at.elapsed() < COPIED_FOR)
                .map(|(_, characters)| characters),
        };
        let layout = deck::draw(&mut chalk, &mut chrome, painter, lines, opacity, &facts);
        self.classic.sjk = chrome;
        layout
    }

    /// The bottom row's place while scrolled back: a gold line across the rows
    /// with how many newer rows there are and how to reach them.
    pub(super) fn sjk_scrolled_back(
        &mut self,
        frame: &mut ConsoleFrame,
        painter: &Painter<'_>,
        labels: Labels<'_>,
        y: f32,
    ) {
        let grid = painter.grid;
        let mut chalk = Chalk {
            frame,
            labels,
            viewport: painter.viewport,
            s: ui_scale(painter.viewport),
        };
        let s = chalk.s;
        let middle = y + grid.pitch * 0.5;
        let right = grid.x(grid.columns);
        let mut scratch = std::mem::take(&mut self.classic.sjk.scratch);
        scratch.clear();
        let rows = self.scroll_offset;
        let _ = write!(
            scratch,
            "{rows} newer row{} below   Ctrl+End",
            if rows == 1 { "" } else { "s" }
        );
        let width = chalk.measure(Family::Body, &scratch, 14.0 * s);
        let label_x = right - width - 12.0 * s;
        chalk.shade(Shade::horizontal(
            [
                grid.left,
                middle.round(),
                label_x - 12.0 * s - grid.left,
                s.round().max(1.0),
            ],
            rgba(color::GOLD, 0.0),
            rgba(color::GOLD, 0.6),
        ));
        chalk.label(
            Family::Body,
            &scratch,
            label_x,
            middle,
            14.0 * s,
            rgba(color::GOLD_BRIGHT, 1.0),
            false,
            f32::MAX,
        );
        self.classic.sjk.scratch = scratch;
    }

    /// A thin scroll bar right of the rows when there are more than fit: the
    /// part shown, holo, gold while scrolled back.
    pub(super) fn sjk_scrollbar(
        &mut self,
        frame: &mut ConsoleFrame,
        painter: &Painter<'_>,
        layout: &Layout,
        rows_bottom: f32,
        total: usize,
    ) {
        let grid = painter.grid;
        let s = ui_scale(painter.viewport);
        let track = rows_bottom - layout.rows_top;
        let shown = (track / grid.pitch).floor().max(1.0) as usize;
        if total <= shown || track <= 0.0 {
            return;
        }
        let x = (grid.x(grid.columns) + 6.0 * s).round();
        let width = (3.0 * s).round().max(2.0);
        frame.quads.push(SolidQuad {
            rect: [x, layout.rows_top, width, track],
            color: rgba(color::HOLO, 0.1),
        });
        let length = (track * shown as f32 / total as f32).max(18.0 * s);
        let newest = total.saturating_sub(1 + self.scroll_offset);
        // How far down the bar's bottom reaches: the newest row shown.
        let end = (newest + 1) as f32 / total as f32;
        let bottom = layout.rows_top + (track * end).max(length);
        frame.quads.push(SolidQuad {
            rect: [x, bottom - length, width, length],
            color: if self.scroll_offset > 0 {
                rgba(color::GOLD, 0.9)
            } else {
                rgba(color::HOLO, 0.5)
            },
        });
    }

    /// The rest of a unique command or cvar name being typed, after the input
    /// in a quiet colour, as Tab would complete it; only with the caret at the
    /// end of an input that fits.
    pub(super) fn sjk_ghost(
        &mut self,
        frame: &mut ConsoleFrame,
        painter: &Painter<'_>,
        y: f32,
        prompt: Prompt,
    ) {
        if self.input.is_empty()
            || self.edit.selection(&self.input).is_some()
            || self.edit.cursor(&self.input) != self.input.len()
        {
            return;
        }
        let Some(rest) = ghost(&self.input, self.shell.completion_hint(&self.input)) else {
            return;
        };
        let column = prompt.input_column + cell_count(&self.input);
        if column + rest.len() >= painter.grid.columns {
            return;
        }
        // The caret's thin bar sits on the cell's left edge, over the ghost.
        painter.raw(frame, rest, column, y, rgba(color::MUTED, 0.55));
    }
}

/// The part of `hint` (the unique name completing the input's last word) still
/// to type, if the last word is a start of it.
fn ghost<'h>(input: &str, hint: Option<&'h str>) -> Option<&'h str> {
    let hint = hint?;
    let word = input.rsplit([' ', ';']).next()?;
    let word = word.strip_prefix(['/', '\\']).unwrap_or(word);
    if word.is_empty() || word.len() >= hint.len() || !hint.is_char_boundary(word.len()) {
        return None;
    }
    hint[..word.len()]
        .eq_ignore_ascii_case(word)
        .then(|| &hint[word.len()..])
}

/// What the chrome shows besides the rows.
struct Facts<'a> {
    /// `HH:MM:SS`, and the corner's `Day dd/mm/yyyy HH:MM:SS`.
    clock: &'a [u8; 8],
    corner: &'a str,
    /// Characters Ctrl+C copied, while "Copied" shows.
    copied: Option<usize>,
}

impl Facts<'_> {
    /// `HH:MM`.
    fn time(&self) -> &str {
        std::str::from_utf8(&self.clock[..5]).unwrap_or("")
    }

    /// `Day dd/mm/yyyy`.
    fn date(&self) -> &str {
        self.corner
            .trim_end()
            .rsplit_once(' ')
            .map_or(self.corner, |(date, _)| date)
    }
}

/// "Copied N characters" in gold bright from `x` on `middle`, while it shows.
fn copied(chalk: &mut Chalk<'_, '_>, chrome: &mut Chrome, facts: &Facts<'_>, x: f32, middle: f32) {
    let Some(characters) = facts.copied else {
        return;
    };
    let s = chalk.s;
    chrome.scratch.clear();
    let _ = write!(
        chrome.scratch,
        "Copied {characters} character{}",
        if characters == 1 { "" } else { "s" }
    );
    let text = std::mem::take(&mut chrome.scratch);
    chalk.label(
        Family::Body,
        &text,
        x,
        middle,
        15.0 * s,
        rgba(color::GOLD_BRIGHT, 1.0),
        false,
        f32::MAX,
    );
    chrome.scratch = text;
}

/// The deck's opacity at full `con_opacity`: the world shows
/// faintly through, as under the SJK UI's menus.
const PANEL: f32 = 0.97;

/// The navy of the panel at `alpha`.
const fn space(alpha: f32) -> [f32; 4] {
    rgba(color::SPACE, alpha)
}

/// A lighter navy for a panel's top.
const fn space_lit(alpha: f32) -> [f32; 4] {
    [0.047, 0.078, 0.145, alpha]
}

/// Deck: the full-width panel.
mod deck {
    use super::*;

    /// Left margin of the header, the rows and the input, in 1080-line pixels.
    pub(super) const MARGIN: f32 = 28.0;
    const HEADER: f32 = 46.0;
    const FOOTER: f32 = 36.0;
    const RAIL: f32 = 2.0;

    pub(super) fn draw(
        chalk: &mut Chalk<'_, '_>,
        chrome: &mut Chrome,
        painter: &Painter<'_>,
        lines: f32,
        opacity: f32,
        facts: &Facts<'_>,
    ) -> Layout {
        let s = chalk.s;
        let grid = painter.grid;
        let [width, _] = chalk.viewport;
        let margin = grid.left;
        chalk.shade(Shade::vertical(
            [0.0, 0.0, width, lines],
            space_lit(PANEL * opacity),
            space(PANEL * opacity),
        ));

        // Header: the name lit gold, the version, then the date and time.
        let header = (HEADER * s).round();
        let middle = header * 0.5;
        let name = chalk.label(
            Family::Display,
            "Console",
            margin,
            middle,
            25.0 * s,
            rgba(color::TEXT, 1.0),
            false,
            f32::MAX,
        );
        chalk.quad(
            [margin, header - (2.0 * s).round(), name, (2.0 * s).round()],
            rgba(color::GOLD, 1.0),
        );
        let version = chalk.label(
            Family::Body,
            crate::menu::main_view::VERSION_LINE,
            margin + name + 18.0 * s,
            middle,
            15.0 * s,
            rgba(color::MUTED, 1.0),
            false,
            f32::MAX,
        );
        copied(
            chalk,
            chrome,
            facts,
            margin + name + version + 40.0 * s,
            middle,
        );
        let right = width - margin;
        let time = chalk.label(
            Family::Display,
            facts.time(),
            right,
            middle,
            25.0 * s,
            rgba(color::TEXT, 1.0),
            true,
            f32::MAX,
        );
        chalk.label(
            Family::Body,
            facts.date(),
            right - time - 14.0 * s,
            middle,
            15.0 * s,
            rgba(color::MUTED, 1.0),
            true,
            f32::MAX,
        );
        chalk.rule(0.0, header, width, rgba(color::HOLO, 0.16));

        // Bottom: the lit rail, the keys, the input band.
        let rail = (RAIL * s).round().max(1.0);
        chalk.rule(0.0, lines - 1.0, width, rgba(color::HOLO, 0.3));
        chalk.shade(Shade::horizontal(
            [0.0, lines - rail, width * 0.45, rail],
            rgba(color::GOLD_BRIGHT, 1.0),
            rgba(color::GOLD, 0.0),
        ));
        let footer = (FOOTER * s).round();
        let keys_middle = lines - rail - footer * 0.5;
        chrome.browser_hit = chalk.console_hints(margin, keys_middle, right);

        let band = (grid.pitch + 16.0 * s).round();
        let band_y = lines - rail - footer - band;
        chalk.quad([0.0, band_y, width, band], rgba(color::HOLO, 0.07));
        chalk.rule(0.0, band_y, width, rgba(color::HOLO, 0.12));
        chalk.rule(0.0, band_y + band, width, rgba(color::HOLO, 0.12));
        chalk.quad(
            [0.0, band_y + band * 0.2, (4.0 * s).round(), band * 0.6],
            rgba(color::GOLD_BRIGHT, 1.0),
        );
        let input_y = (band_y + (band - grid.pitch) * 0.5).round();
        Layout {
            rows_y: band_y - (8.0 * s).round() - grid.pitch,
            rows_top: header + (6.0 * s).round(),
            partial: false,
            input_y,
            prompt: Rect::new(0.0, band_y, width, band),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghost_is_the_rest_of_the_last_word() {
        assert_eq!(ghost("con_st", Some("con_style")), Some("yle"));
        assert_eq!(ghost("bind x; CON_ST", Some("con_style")), Some("yle"));
        assert_eq!(ghost("/con_st", Some("con_style")), Some("yle"));
        assert_eq!(ghost("con_style", Some("con_style")), None);
        assert_eq!(ghost("cl_", Some("con_style")), None);
        assert_eq!(ghost("con_st ", Some("con_style")), None);
        assert_eq!(ghost("con_st", None), None);
    }

    #[test]
    fn rows_are_a_fifth_apart_and_cells_sit_in_their_middle() {
        let grid = grid([1920.0, 1080.0], 1.0);
        assert_eq!((grid.width, grid.height, grid.pitch), (8.0, 16.0, 19.0));
        assert_eq!(grid.inset(), 2.0);
        // The rows and the scroll bar stay inside the screen.
        assert!(grid.x(grid.columns) + GUTTER <= 1920.0);
        let uhd = super::grid([3840.0, 2160.0], 1.0);
        assert_eq!((uhd.width, uhd.pitch), (16.0, 38.0));
    }

    #[test]
    fn the_caret_is_a_thin_bar_or_a_block() {
        let grid = grid([1920.0, 1080.0], 1.0);
        let bar = caret(&grid, 100.0, 50.0, false);
        assert!(bar.rect[2] >= 2.0 && bar.rect[2] < grid.width);
        let block = caret(&grid, 100.0, 50.0, true);
        assert_eq!(block.rect, [100.0, 50.0, grid.width, grid.height]);
    }

    #[test]
    fn the_date_and_time_come_from_the_classic_clocks() {
        let clock = *b"22:52:10";
        let facts = Facts {
            clock: &clock,
            corner: "Wed 07/10/2026 22:52:10",
            copied: None,
        };
        assert_eq!((facts.time(), facts.date()), ("22:52", "Wed 07/10/2026"));
    }
}
