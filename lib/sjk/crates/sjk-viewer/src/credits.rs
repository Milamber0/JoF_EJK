//! The credits page: who makes SJK, from `assets/credits.txt` and everyone's
//! work from `assets/credits_history.txt` (both built in, parsed once; see
//! `credits_data.rs`). Opened by the main menu's Credits entry, the in-game SJK
//! menu or the `credits` console command. Like the changelog it lives in the
//! console and is drawn in its place, so it opens over the menus and in a match,
//! and closes the console with itself when it opened it.
//!
//! The page is meant to shine: golden god rays turn slowly down from above the
//! top of the screen and sparks rise through them, SJK's emblem breathes in front
//! of a turning sunburst (both after SJK's site) above a title with a passing
//! glint. Everyone with a history has a panel of their own: their name large,
//! their role, what their history counts, and two folds, closed whenever the page
//! opens: their highlights, and all their work, newest first by day, each feature
//! or pull request unfolding into its commits. Unfolded rows fade in one after the
//! other. A pull request's number and a commit open on GitHub when clicked, as do
//! GitHub handles and the cards' links. The rest (fonts, references) sit on small
//! cards. A card's medals (`medal:` in credits.txt) show on it, picture and name.
//! With the classic menus the palette is retail's gold and blue and the text is
//! drawn in the menus' retail font; with the SJK UI the page has that UI's look
//! (`credits_sjk.rs`), its sun on the left. Sections come from the file, so a
//! later "Supporters" section needs no code.

use crate::medals::Medal;
use crate::menu::art::motion;
use crate::menu::emblem::{self, EmblemLayer};
use crate::menu_widgets::{BACK_TOKEN, MenuCanvas};
use crate::text::{TextFace, TextVertex, UiFont, visible_text_width_style};
use sjk_ui::{Color, DrawCommand, FontWeight, Gradient, InputEvent, Rect, TextAlign, UiEventKind};
use std::collections::BTreeSet;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

#[path = "credits_data.rs"]
mod data;
#[path = "credits_sjk.rs"]
mod sjk;

/// Console command that toggles the page.
pub(crate) const COMMAND: &str = "credits";
/// Help text for completion and `cmdlist`.
pub(crate) const HELP: &str = "Show who makes SJK";

/// Wheel target over the page.
const PAGE_TOKEN: u16 = 906;
/// The footer's Expand all / Collapse all.
const ALL_TOKEN: u16 = 907;
/// The scrollbar's track.
const SCROLLBAR_TOKEN: u16 = 908;
/// First section of the SJK UI's rail; section `i` is `SECTION_BASE + i`.
const SECTION_BASE: u16 = 920;
/// Sections the rail can answer for (tokens up to `URL_BASE` stay free).
const SECTION_TOKENS: u16 = 64;
/// First address token; address `i` of `Panel::urls` is `URL_BASE + i`.
const URL_BASE: u16 = 10_000;
/// First fold token; fold `i` of `Panel::actions` is `FOLD_BASE + i`.
const FOLD_BASE: u16 = 40_000;
/// Pointer areas a frame gives the rows, under the canvas's limit with the
/// page's own (scroll area, scrollbar, buttons).
const ROW_AREAS: usize = 84;
/// Pixels (at 1080 lines) one wheel notch or arrow key scrolls.
const STEP: f32 = 110.0;
/// The closing line, as in CREDITS.md.
const NOTICE: &str = "Star Wars, Jedi Knight and Jedi Academy are trademarks of their respective owners. SJK is a fan project, not affiliated with or endorsed by Lucasfilm, Disney, Raven Software or Activision.";
/// Sparks rising through the backdrop.
const SPARKS: usize = 64;
/// The site's god-ray gold (`#e8b84a`) and sunburst gold (`#ffcf70`).
const RAY_GOLD: Color = Color::new(0.910, 0.722, 0.290, 1.0);
const SUN_GOLD: Color = Color::new(1.0, 0.812, 0.439, 1.0);

/// The page's colours: retail's, as the classic menus have them.
#[derive(Clone, Copy)]
struct Palette {
    accent: Color,
    /// The glint and the brightest edges.
    shine: Color,
    title: Color,
    text: Color,
    muted: Color,
    /// Backdrop gradient, top and bottom.
    deep: [Color; 2],
    card: Color,
}

impl Palette {
    /// Retail gold (`1 .682 0`), title blue (`.549 .854 1`) and list lilac.
    fn classic() -> Self {
        Self {
            accent: Color::new(1.0, 0.682, 0.0, 1.0),
            shine: Color::new(1.0, 0.95, 0.75, 1.0),
            title: Color::new(0.549, 0.854, 1.0, 1.0),
            text: Color::new(0.75, 0.75, 1.0, 1.0),
            muted: Color::new(0.615, 0.615, 0.956, 0.9),
            deep: [
                Color::new(0.010, 0.020, 0.090, 0.97),
                Color::new(0.002, 0.004, 0.025, 0.99),
            ],
            card: Color::new(0.04, 0.05, 0.20, 0.62),
        }
    }
}

fn alpha(color: Color, a: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * a)
}

/// Something the reader opens and closes; `u16` numbers a card in
/// `Panel::people` and a piece of its work.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Fold {
    /// A card's highlights (its `did:` lines).
    Highlights(u16),
    /// A card's whole history.
    Work(u16),
    /// One piece of work's commits.
    Commits(u16, u16),
}

/// What a click on a row does.
#[derive(Clone, Copy, Debug)]
enum Action {
    Toggle(Fold),
    Open(u16),
}

/// One laid-out piece of the scrolling content, at a scroll of zero.
enum Piece {
    Heading {
        y: f32,
        section: usize,
    },
    /// The panel of someone with a history, as tall as everything unfolded in
    /// it; `head` is the height above its folds.
    Person {
        rect: Rect,
        person: u16,
        head: f32,
        role: Vec<String>,
        handle: Option<(u16, f32)>,
        links: Vec<(u16, f32)>,
    },
    /// A fold's bar, with the token that toggles it.
    Bar {
        rect: Rect,
        fold: Fold,
        token: u16,
    },
    /// A highlight's wrapped line; `first` starts the highlight.
    Highlight {
        rect: Rect,
        text: String,
        first: bool,
    },
    /// The date over the work merged that day.
    Day {
        rect: Rect,
        person: u16,
        work: u16,
    },
    /// A feature or pull request: its wrapped title, the token of the row and
    /// that of its pull request's number with the number's width.
    Work {
        rect: Rect,
        person: u16,
        work: u16,
        lines: Vec<String>,
        token: u16,
        reference: Option<(u16, f32)>,
    },
    /// One commit of an unfolded piece of work.
    Commit {
        rect: Rect,
        person: u16,
        work: u16,
        commit: u16,
        lines: Vec<String>,
        token: u16,
    },
    /// A card without a history, in a row of cards.
    Card {
        rect: Rect,
        section: usize,
        card: usize,
        /// The role wrapped to the card (the SJK UI's; the other looks draw
        /// it on one line).
        role: Vec<String>,
        /// The contributions wrapped to the card: text, and whether it starts one.
        lines: Vec<(String, bool)>,
        handle: Option<(u16, f32)>,
        links: Vec<(u16, f32)>,
    },
}

impl Piece {
    /// The window rows the piece covers at a scroll of zero.
    fn span(&self) -> (f32, f32) {
        match self {
            Self::Heading { y, .. } => (*y, y + 40.0),
            Self::Person { rect, .. }
            | Self::Bar { rect, .. }
            | Self::Highlight { rect, .. }
            | Self::Day { rect, .. }
            | Self::Work { rect, .. }
            | Self::Commit { rect, .. }
            | Self::Card { rect, .. } => (rect.y, rect.bottom()),
        }
    }
}

/// Sizes at a scale of 1 (1080 lines).
const NAME: f32 = 30.0;
const HANDLE: f32 = 13.0;
const ROLE: f32 = 15.0;
const BODY: f32 = 15.0;
const BODY_LINE: f32 = 21.0;
const PAD: f32 = 24.0;
const GAP: f32 = 24.0;
const BULLET: f32 = 18.0;
/// A person's panel: name (the first section's larger), role, padding.
const HERO_NAME: [f32; 2] = [78.0, 58.0];
const HERO_ROLE: f32 = 19.0;
const HERO_PAD: f32 = 40.0;
const HERO_WIDTH: f32 = 1_180.0;
/// Fold bars, work titles and commits.
const BAR: f32 = 46.0;
const WORK: f32 = 16.0;
const WORK_LINE: f32 = 23.0;
const COMMIT: f32 = 14.0;
const COMMIT_LINE: f32 = 20.0;
/// Room left of a work title for its fold box, and right of it for the
/// pull request's number and the commit count.
const BOX: f32 = 34.0;
const WORK_TAIL: f32 = 200.0;
/// Room left of a commit's subject for its hash.
const HASH: f32 = 84.0;

pub(crate) struct Panel {
    open: bool,
    owns_console: bool,
    sections: Vec<data::Section>,
    error: Option<String>,
    /// Section and card of each card with a history, by person number.
    people: Vec<(usize, usize)>,
    folds: BTreeSet<Fold>,
    /// Pixels scrolled, where scrolling is heading, and the most there is.
    scroll: f32,
    target: f32,
    max_scroll: f32,
    /// Window height at the last frame, for paging.
    page: f32,
    pieces: Vec<Piece>,
    /// When each piece starts to fade in (unfolded rows arrive in turn).
    born: Vec<f64>,
    /// The addresses the laid-out pieces open, indexed by token - `URL_BASE`.
    urls: Vec<String>,
    /// The folds the laid-out rows toggle, indexed by token - `FOLD_BASE`.
    actions: Vec<Action>,
    /// What `pieces` was laid out for: the look, then the viewport and text
    /// size (bits), or the SJK UI's text style and fonts; `None` after a fold
    /// or the look changed.
    laid_out_for: Option<[u32; 4]>,
    /// Each section's heading, in the laid-out pieces' units.
    heading_ys: Vec<f32>,
    /// Where a heading sits when scrolled to (pieces' units) and window
    /// pixels per piece unit, from the last frame.
    anchor: [f32; 2],
    /// The fold just opened (all of them for Expand all) and when, so its rows
    /// fade in.
    reveal: Option<(Option<Fold>, f64)>,
    /// Height of the laid-out content, and where its closing notice sits.
    content: f32,
    notice_y: f32,
    /// The SJK UI's look (`credits_sjk.rs`), else the classic one.
    sjk: bool,
    opened_at: f64,
    /// The clock at the last frame, for smooth scrolling.
    last_frame: f64,
    ui: MenuCanvas,
}

impl Default for Panel {
    fn default() -> Self {
        Self::new()
    }
}

impl Panel {
    pub(crate) fn new() -> Self {
        let (sections, error) = match data::parse_all(data::EMBEDDED, data::HISTORY) {
            Ok(sections) => (sections, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        let people = sections
            .iter()
            .enumerate()
            .flat_map(|(index, section)| {
                section
                    .cards
                    .iter()
                    .enumerate()
                    .filter(|(_, card)| !card.work.is_empty())
                    .map(move |(card, _)| (index, card))
            })
            .collect();
        Self {
            open: false,
            owns_console: false,
            sections,
            error,
            people,
            folds: BTreeSet::new(),
            scroll: 0.0,
            target: 0.0,
            max_scroll: 0.0,
            page: 600.0,
            pieces: Vec::new(),
            born: Vec::new(),
            urls: Vec::new(),
            actions: Vec::new(),
            laid_out_for: None,
            heading_ys: Vec::new(),
            anchor: [0.0, 1.0],
            reveal: None,
            content: 0.0,
            notice_y: 0.0,
            sjk: false,
            opened_at: 0.0,
            last_frame: 0.0,
            ui: MenuCanvas::with_capacities(512, 384, 2_048),
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Show the page from the top, everything folded; `owns_console` when the
    /// console was closed.
    pub(crate) fn open(&mut self, owns_console: bool) {
        self.open = true;
        self.owns_console = owns_console;
        self.scroll = 0.0;
        self.target = 0.0;
        self.folds.clear();
        self.laid_out_for = None;
        self.reveal = None;
        self.opened_at = motion::seconds();
        emblem::request();
    }

    /// Hide the page; returns whether it had opened the console.
    pub(crate) fn close(&mut self) -> bool {
        let owned = self.open && self.owns_console;
        self.open = false;
        self.owns_console = false;
        owned
    }

    /// Whether the classic palette is drawn, so its text uses the retail font.
    pub(crate) fn is_classic(&self) -> bool {
        !self.sjk
    }

    /// Draw the SJK UI's look (`sjk`), in its families, or not.
    pub(crate) fn set_sjk(&mut self, sjk: bool) {
        if self.sjk != sjk {
            self.laid_out_for = None;
        }
        self.sjk = sjk;
    }

    /// Whether the SJK UI's look is drawn.
    pub(crate) fn is_sjk(&self) -> bool {
        self.sjk
    }

    /// Skip the opening and unfolding animations, for snapshots.
    #[cfg(test)]
    pub(crate) fn settle(&mut self) {
        self.opened_at = f64::MIN / 2.0;
        self.reveal = None;
        self.laid_out_for = None;
    }

    /// Scroll to `pixels`, for snapshots (clamped by the next frame).
    #[cfg(test)]
    pub(crate) fn scroll_to(&mut self, pixels: f32) {
        self.scroll = pixels;
        self.target = pixels;
    }

    /// Scroll on by `pixels` from where the page is, for snapshots.
    #[cfg(test)]
    pub(crate) fn scroll_on(&mut self, pixels: f32) {
        self.scroll_to(self.target + pixels);
    }

    /// Open `person`'s folds and the commits of their newest work that has
    /// several, for snapshots.
    #[cfg(test)]
    pub(crate) fn unfold(&mut self, person: u16) {
        let (section, card) = self.people[usize::from(person)];
        let work = &self.sections[section].cards[card].work;
        let several = work
            .iter()
            .position(|piece| !piece.is_single())
            .unwrap_or(0);
        self.folds.extend([
            Fold::Highlights(person),
            Fold::Work(person),
            Fold::Commits(person, several as u16),
        ]);
        self.laid_out_for = None;
    }

    /// Scroll to `person`'s panel as last laid out, for snapshots.
    #[cfg(test)]
    pub(crate) fn scroll_to_person(&mut self, person: u16) {
        let top = self.pieces.iter().find_map(|piece| match piece {
            Piece::Person {
                rect, person: p, ..
            } if *p == person => Some(rect.y),
            _ => None,
        });
        let [above, scale] = self.anchor;
        self.scroll_to((top.unwrap_or(0.0) - above) * scale);
    }

    /// Whether the last frame overflowed the canvas, and how many pointer
    /// areas it registered, for tests.
    #[cfg(test)]
    pub(crate) fn canvas_use(&self) -> (bool, usize) {
        (self.ui.overflowed(), self.ui.widget_count())
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    /// A key event; `shift` is held. Returns true when it closes the page.
    pub(crate) fn handle_key(&mut self, event: &KeyEvent, shift: bool) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return false;
        };
        self.key(key, shift)
    }

    /// A pressed key; returns true when it closes the page.
    fn key(&mut self, key: KeyCode, shift: bool) -> bool {
        let step = STEP * self.page / 1080.0;
        match key {
            KeyCode::Escape | KeyCode::Enter | KeyCode::NumpadEnter => return true,
            KeyCode::ArrowUp => self.scroll_by(-step),
            KeyCode::ArrowDown => self.scroll_by(step),
            KeyCode::PageUp => self.scroll_by(-self.page * 0.8),
            KeyCode::PageDown | KeyCode::Space => self.scroll_by(self.page * 0.8),
            KeyCode::Home => self.target = 0.0,
            KeyCode::End => self.target = self.max_scroll,
            KeyCode::Tab => self.step_section(!shift),
            KeyCode::BracketRight => self.step_section(true),
            KeyCode::BracketLeft => self.step_section(false),
            KeyCode::KeyE => self.toggle_all(),
            _ => {}
        }
        false
    }

    /// Where the page scrolls to show section `index` at its top, in window
    /// pixels, as last laid out.
    fn section_scroll(&self, index: usize) -> f32 {
        let [above, scale] = self.anchor;
        self.heading_ys
            .get(index)
            .map_or(0.0, |y| ((y - above) * scale).clamp(0.0, self.max_scroll))
    }

    /// The section in view: the last one whose heading has reached the upper
    /// third of the page.
    fn current_section(&self) -> usize {
        let here = self.scroll + self.page * 0.35;
        (0..self.heading_ys.len())
            .rev()
            .find(|&index| self.section_scroll(index) <= here)
            .unwrap_or(0)
    }

    /// Scroll to the next section (`forward`) or the one before, round to the
    /// first or the last past the ends.
    fn step_section(&mut self, forward: bool) {
        let count = self.heading_ys.len();
        if count == 0 {
            return;
        }
        let here = self.target;
        self.target = if forward {
            (0..count)
                .map(|index| self.section_scroll(index))
                .find(|&at| at > here + 1.0)
                .unwrap_or(0.0)
        } else {
            (0..count)
                .rev()
                .map(|index| self.section_scroll(index))
                .find(|&at| at < here - 1.0)
                .unwrap_or_else(|| self.section_scroll(count - 1))
        };
    }

    /// Glide the scroll towards where the wheel, keys or scrollbar sent it,
    /// for the frame drawn now; returns the clock.
    fn glide(&mut self) -> f64 {
        let now = motion::seconds();
        let dt = (now - self.last_frame).clamp(0.0, 0.1) as f32;
        self.last_frame = now;
        self.target = self.target.clamp(0.0, self.max_scroll);
        self.scroll += (self.target - self.scroll) * (1.0 - (-dt * 14.0).exp());
        if (self.target - self.scroll).abs() < 0.5 {
            self.scroll = self.target;
        }
        self.scroll = self.scroll.clamp(0.0, self.max_scroll);
        now
    }

    /// A pointer event; returns true when it closes the page.
    pub(crate) fn handle_pointer(&mut self, event: InputEvent) -> bool {
        let Some(event) = self.ui.pointer(event) else {
            return false;
        };
        match event.kind {
            UiEventKind::Wheel => {
                let direction = event.delta.map_or(0.0, |delta| -delta.y.signum());
                self.scroll_by(direction * STEP * self.page / 1080.0);
                false
            }
            UiEventKind::Press | UiEventKind::Drag if event.token == Some(SCROLLBAR_TOKEN) => {
                if let (Some(point), Some(track)) =
                    (event.position, self.ui.rect_for(SCROLLBAR_TOKEN))
                {
                    let ratio = ((point.y - track.y) / track.height).clamp(0.0, 1.0);
                    self.target = ratio * self.max_scroll;
                    self.scroll = self.target;
                }
                false
            }
            UiEventKind::Activate => {
                let Some(token) = event.token else {
                    return false;
                };
                if token == BACK_TOKEN {
                    return true;
                }
                if token == ALL_TOKEN {
                    self.toggle_all();
                } else if let Some(section) = token
                    .checked_sub(SECTION_BASE)
                    .filter(|section| *section < SECTION_TOKENS)
                {
                    self.target = self.section_scroll(usize::from(section));
                } else if let Some(action) = token
                    .checked_sub(FOLD_BASE)
                    .and_then(|index| self.actions.get(usize::from(index)).copied())
                {
                    match action {
                        Action::Toggle(fold) => self.toggle(fold),
                        Action::Open(url) => self.open_url(url),
                    }
                } else if let Some(url) = token.checked_sub(URL_BASE) {
                    self.open_url(url);
                }
                false
            }
            _ => false,
        }
    }

    fn open_url(&self, index: u16) {
        if let Some(url) = self.urls.get(usize::from(index)) {
            crate::update::open_page(url);
        }
    }

    fn toggle(&mut self, fold: Fold) {
        if !self.folds.remove(&fold) {
            self.folds.insert(fold);
            self.reveal = Some((Some(fold), motion::seconds()));
        }
        self.laid_out_for = None;
    }

    /// Open every fold, or close them all when any is open.
    fn toggle_all(&mut self) {
        if self.folds.is_empty() {
            for (person, &(section, card)) in self.people.iter().enumerate() {
                let person = person as u16;
                let card = &self.sections[section].cards[card];
                if !card.did.is_empty() {
                    self.folds.insert(Fold::Highlights(person));
                }
                self.folds.insert(Fold::Work(person));
                for (work, piece) in card.work.iter().enumerate() {
                    if !piece.is_single() {
                        self.folds.insert(Fold::Commits(person, work as u16));
                    }
                }
            }
            self.reveal = Some((None, motion::seconds()));
        } else {
            self.folds.clear();
        }
        self.laid_out_for = None;
    }

    fn scroll_by(&mut self, pixels: f32) {
        self.target = (self.target + pixels).clamp(0.0, self.max_scroll);
    }

    /// Lay the sections out for `viewport`: headings, a panel for each person
    /// with a history and their unfolded rows, then rows of cards for the rest,
    /// centred in the column, each card as tall as its row's tallest.
    fn layout(&mut self, font: &UiFont, viewport: [f32; 2]) {
        let s = crate::ui_scale::height_scale(viewport[1]);
        let style = font.style();
        let key = [
            0,
            viewport[0].to_bits(),
            viewport[1].to_bits(),
            style.scale.to_bits(),
        ];
        if self.laid_out_for == Some(key) {
            return;
        }
        self.laid_out_for = Some(key);
        self.pieces.clear();
        self.born.clear();
        self.urls.clear();
        self.actions.clear();
        self.heading_ys.clear();
        let measure = |text: &str, size: f32| {
            let size = size * s;
            let placement = style.place(Rect::new(0.0, 0.0, 0.0, size), size, 0.2 * s);
            visible_text_width_style(
                font,
                text,
                placement.size / font.height.max(1.0),
                TextFace::Regular,
                placement.letter_spacing,
            )
        };
        let mut lay = Layout {
            pieces: &mut self.pieces,
            born: &mut self.born,
            urls: &mut self.urls,
            actions: &mut self.actions,
            folds: &self.folds,
            reveal: self.reveal,
            revealing: None,
        };
        let margin = (viewport[0] * 0.07).max(40.0 * s);
        let column = viewport[0] - margin * 2.0;
        let gap = GAP * s;
        let columns = ((column + gap) / (420.0 * s + gap)).floor().clamp(1.0, 3.0) as usize;
        let hero_width = column.min(HERO_WIDTH * s);
        let hero_x = (viewport[0] - hero_width) * 0.5;
        let mut y = header_height(s);
        let mut person = 0_u16;
        for (section_index, section) in self.sections.iter().enumerate() {
            self.heading_ys.push(y);
            lay.push(Piece::Heading {
                y,
                section: section_index,
            });
            y += 56.0 * s;
            for card in section.cards.iter().filter(|card| !card.work.is_empty()) {
                let featured = section_index == 0;
                y = lay.person(card, person, featured, hero_x, hero_width, y, s, &measure);
                y += gap * 1.5;
                person += 1;
            }
            let plain: Vec<(usize, &data::Card)> = section
                .cards
                .iter()
                .enumerate()
                .filter(|(_, card)| card.work.is_empty())
                .collect();
            for row in plain.chunks(columns) {
                let count = row.len();
                let width = ((column - gap * (columns - 1) as f32) / columns as f32).min(620.0 * s);
                let row_width = width * count as f32 + gap * (count - 1) as f32;
                let left = (viewport[0] - row_width) * 0.5;
                let text_width = width - PAD * 2.0 * s - BULLET * s;
                let mut wrapped = Vec::with_capacity(count);
                let mut tallest: f32 = 0.0;
                for (_, card) in row {
                    // The handle is drawn smaller and wider spaced than the body.
                    let handle = card.github_url().map(|url| {
                        let width = link_width(&measure, &card.github, HANDLE, s);
                        (lay.url(url), width)
                    });
                    let links: Vec<_> = card
                        .links
                        .iter()
                        .map(|entry| {
                            (
                                lay.url(entry.url.clone()),
                                link_width(&measure, &entry.label, BODY, s),
                            )
                        })
                        .collect();
                    let mut lines = Vec::new();
                    for did in &card.did {
                        for (index, line) in wrap(did, text_width, |t| measure(t, BODY))
                            .into_iter()
                            .enumerate()
                        {
                            lines.push((line, index == 0));
                        }
                    }
                    tallest = tallest.max(card_height(
                        lines.len(),
                        card.did.len(),
                        links.len(),
                        !card.medals.is_empty(),
                        s,
                    ));
                    wrapped.push((lines, handle, links));
                }
                for (slot, (lines, handle, links)) in wrapped.into_iter().enumerate() {
                    lay.push(Piece::Card {
                        rect: Rect::new(left + slot as f32 * (width + gap), y, width, tallest),
                        section: section_index,
                        card: row[slot].0,
                        role: Vec::new(),
                        lines,
                        handle,
                        links,
                    });
                }
                y += tallest + gap;
            }
            y += 20.0 * s;
        }
        self.notice_y = y;
        self.content = y + 70.0 * s;
    }

    /// Draw the page over the whole frame. Text other overlays appended earlier
    /// this frame is dropped rather than shown through, as for the changelog.
    pub(crate) fn append(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        vertices.clear();
        let s = crate::ui_scale::height_scale(viewport[1]);
        self.page = viewport[1];
        self.layout(font, viewport);
        let footer = 64.0 * s;
        let view_height = viewport[1] - footer;
        self.max_scroll = (self.content - view_height).max(0.0);
        // A section scrolled to keeps a little room above its heading.
        self.anchor = [20.0 * s, 1.0];
        let now = self.glide();

        self.ui.begin_transparent(viewport);
        let palette = Palette::classic();
        let since = (now - self.opened_at).max(0.0) as f32;
        backdrop(&mut self.ui, viewport, palette, now as f32);

        let view = Rect::new(0.0, 0.0, viewport[0], view_height);
        self.ui.scroll_region(PAGE_TOKEN, view);
        let _ = self.ui.draw_list_mut().push(DrawCommand::PushClip(view));
        let top = -self.scroll;
        self.header(viewport, top, palette, now, s);
        if let Some(error) = &self.error {
            self.ui.text_aligned(
                error,
                Rect::new(0.0, top + header_height(s), viewport[0], 24.0 * s),
                16.0 * s,
                self.ui.theme().critical,
                FontWeight::Regular,
                0.2 * s,
                TextAlign::Center,
            );
        }
        let mut frame = Frame {
            top,
            view_height,
            palette,
            since,
            now,
            s,
            areas: ROW_AREAS,
        };
        for index in 0..self.pieces.len() {
            let (start, end) = self.pieces[index].span();
            // Pieces lift in by up to 18 pixels as they arrive.
            if top + start > view_height || top + end + 20.0 * s < 0.0 {
                continue;
            }
            self.piece(index, viewport, &mut frame);
        }
        self.ui.text_aligned(
            NOTICE,
            Rect::new(0.0, top + self.notice_y, viewport[0], 18.0 * s),
            11.0 * s,
            alpha(palette.muted, 0.8),
            FontWeight::Regular,
            0.3 * s,
            TextAlign::Center,
        );
        let _ = self.ui.draw_list_mut().push(DrawCommand::PopClip);

        self.scrollbar(viewport, view_height, palette, s);
        self.footer(viewport, footer, palette, s);
        self.ui.finish(BACK_TOKEN);
        self.ui.append_text(vertices, font, viewport);
    }

    /// The emblem breathing in front of two turning sunbursts, the title with
    /// its glint, and the line under it.
    fn header(&mut self, viewport: [f32; 2], top: f32, palette: Palette, now: f64, s: f32) {
        let side = 120.0 * s;
        let emblem = Rect::new((viewport[0] - side) * 0.5, top + 36.0 * s, side, side);
        let center = [emblem.x + side * 0.5, emblem.y + side * 0.5];
        let swell = 0.5 + 0.5 * (now as f32 * 1.4).sin();
        let turn = (now % 3_600.0) as f32;
        emblem::rays(
            &mut self.ui,
            EmblemLayer::Sunburst,
            center,
            side * 1.9,
            turn * 0.05,
            alpha(SUN_GOLD, 0.30 + 0.08 * swell),
        );
        emblem::rays(
            &mut self.ui,
            EmblemLayer::Sunburst,
            center,
            side * 1.35,
            -turn * 0.08 + 0.2,
            alpha(SUN_GOLD, 0.16 + 0.06 * swell),
        );
        emblem::draw(&mut self.ui, emblem, now);

        let title = Rect::new(0.0, emblem.bottom() + 14.0 * s, viewport[0], 84.0 * s);
        glint(&mut self.ui, title, palette, now, 4.5, 260.0 * s);
        let shimmer = 0.5 + 0.5 * (now as f32 * 0.9).sin();
        let title_color = mix(palette.title, palette.shine, shimmer * 0.35);
        self.ui.text_aligned(
            "CREDITS",
            title,
            76.0 * s,
            title_color,
            FontWeight::Semibold,
            14.0 * s,
            TextAlign::Center,
        );
        self.ui.text_aligned(
            "SOL JK   /   THE PEOPLE WHO MAKE IT",
            Rect::new(0.0, title.bottom() + 6.0 * s, viewport[0], 20.0 * s),
            13.0 * s,
            palette.accent,
            FontWeight::Semibold,
            3.6 * s,
            TextAlign::Center,
        );
    }

    /// One piece, faded and lifted in as the page opens or its fold unfolds.
    fn piece(&mut self, index: usize, viewport: [f32; 2], frame: &mut Frame) {
        let Frame {
            top,
            view_height,
            palette,
            since,
            now,
            s,
            ..
        } = *frame;
        // The first pieces arrive 0.08 s after each other as the page opens;
        // unfolded rows from the moment they were born.
        let arrival = ((since - index.min(14) as f32 * 0.08) / 0.45).clamp(0.0, 1.0);
        let unfold = ((now - self.born[index]) as f32 / 0.3).clamp(0.0, 1.0);
        let ease = (1.0 - (1.0 - arrival).powi(3)) * (1.0 - (1.0 - unfold).powi(2));
        let lift = (1.0 - ease) * 18.0 * s;
        // Clickable only where the page shows it, while there is room.
        let visible = |area: Rect| area.y >= 0.0 && area.bottom() <= view_height;
        let pulse = 0.5 + 0.5 * (now as f32 * 1.1 - index as f32 * 0.7).sin();
        let _ = self.ui.draw_list_mut().push(DrawCommand::PushOpacity(ease));
        match &self.pieces[index] {
            Piece::Heading { y, section } => {
                let y = top + y + lift;
                let title = &self.sections[*section].title;
                let line_width = 200.0 * s;
                let middle = viewport[0] * 0.5;
                let line_y = y + 15.0 * s;
                horizontal(
                    &mut self.ui,
                    Rect::new(middle - 180.0 * s - line_width, line_y, line_width, 1.5 * s),
                    alpha(palette.accent, 0.0),
                    alpha(palette.accent, 0.8),
                );
                horizontal(
                    &mut self.ui,
                    Rect::new(middle + 180.0 * s, line_y, line_width, 1.5 * s),
                    alpha(palette.accent, 0.8),
                    alpha(palette.accent, 0.0),
                );
                self.ui.text_fmt_aligned(
                    format_args!("{}", crate::menu::classic::view::Caps(title)),
                    Rect::new(middle - 175.0 * s, y + 4.0 * s, 350.0 * s, 24.0 * s),
                    17.0 * s,
                    palette.accent,
                    FontWeight::Semibold,
                    5.0 * s,
                    TextAlign::Center,
                );
            }
            Piece::Person {
                rect,
                person,
                head,
                role,
                handle,
                links,
            } => {
                let rect = Rect::new(rect.x, top + rect.y + lift, rect.width, rect.height);
                let (section, card) = self.people[usize::from(*person)];
                let featured = section == 0;
                let card = &self.sections[section].cards[card];
                let canvas = &mut self.ui;
                glass(canvas, rect, palette, pulse, 18.0 * s, s);
                // A light bar down the left edge, brightest at the name.
                let bar = Rect::new(rect.x + 1.5 * s, rect.y + 22.0 * s, 4.0 * s, *head);
                let _ = canvas.draw_list_mut().push(DrawCommand::GradientRect {
                    rect: bar,
                    radius: 2.0 * s,
                    gradient: Gradient {
                        start: alpha(palette.shine, 0.55 + 0.35 * pulse),
                        end: alpha(palette.accent, 0.0),
                        vertical: true,
                    },
                });
                let x = rect.x + HERO_PAD * s;
                let width = rect.width - HERO_PAD * 2.0 * s;
                let name_size = HERO_NAME[usize::from(!featured)] * s;
                let name = Rect::new(x, rect.y + 26.0 * s, width, name_size * 1.15);
                canvas.text(
                    &card.name,
                    name,
                    name_size,
                    palette.title,
                    FontWeight::Semibold,
                    2.0 * s,
                );
                if let Some((token, handle_width)) = *handle {
                    let area = Rect::new(x, rect.y + 30.0 * s, width, 20.0 * s);
                    link_text(
                        canvas,
                        &card.github,
                        area,
                        HANDLE * 1.15 * s,
                        TextAlign::End,
                        token,
                        handle_width,
                        palette,
                        s,
                        visible,
                        &mut frame.areas,
                    );
                }
                let mut y = name.bottom() + 6.0 * s;
                for line in role {
                    canvas.text(
                        line,
                        Rect::new(x, y, width, HERO_ROLE * 1.35 * s),
                        HERO_ROLE * s,
                        palette.accent,
                        FontWeight::Regular,
                        0.4 * s,
                    );
                    y += HERO_ROLE * 1.4 * s;
                }
                y += 14.0 * s;
                let tally = card.tally();
                let mut chip_x = x;
                // Changes count only when some are not pull requests.
                let changes = if tally.pulls == tally.work {
                    0
                } else {
                    tally.work
                };
                for (count, label) in [
                    (changes, "CHANGES"),
                    (tally.pulls, "PULL REQUESTS"),
                    (tally.commits, "COMMITS"),
                ] {
                    if count == 0 {
                        continue;
                    }
                    chip_x += chip(canvas, chip_x, y, count, label, palette, s) + 10.0 * s;
                }
                for &(medal, count) in &card.medals {
                    chip_x += medal_chip(canvas, chip_x, y, medal, count, palette, s) + 10.0 * s;
                }
                if !links.is_empty() {
                    y += 30.0 * s + 14.0 * s;
                    let mut link_x = x;
                    for (&(token, label_width), entry) in links.iter().zip(&card.links) {
                        let area = Rect::new(link_x, y, label_width + BULLET * s, BODY_LINE * s);
                        let hovered = canvas.token_hovered(token);
                        let color = if hovered {
                            palette.shine
                        } else {
                            palette.muted
                        };
                        canvas.text(">", area, BODY * s, color, FontWeight::Semibold, 0.2 * s);
                        link_text(
                            canvas,
                            &entry.label,
                            Rect::new(area.x + BULLET * s, area.y, label_width, area.height),
                            BODY * s,
                            TextAlign::Start,
                            token,
                            label_width,
                            palette,
                            s,
                            visible,
                            &mut frame.areas,
                        );
                        link_x += label_width + BULLET * s + 28.0 * s;
                    }
                }
            }
            Piece::Bar { rect, fold, token } => {
                let rect = Rect::new(rect.x, top + rect.y + lift, rect.width, rect.height);
                let open = self.folds.contains(fold);
                let hovered = self.ui.token_hovered(*token);
                let canvas = &mut self.ui;
                let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect,
                    radius: 10.0 * s,
                    color: alpha(
                        palette.accent,
                        if hovered {
                            0.16
                        } else if open {
                            0.10
                        } else {
                            0.06
                        },
                    ),
                });
                let _ = canvas.draw_list_mut().push(DrawCommand::Border {
                    rect,
                    radius: 10.0 * s,
                    width: s.max(1.0),
                    color: alpha(palette.accent, if hovered { 0.7 } else { 0.28 }),
                });
                let side = 20.0 * s;
                fold_box(
                    canvas,
                    Rect::new(
                        rect.x + 14.0 * s,
                        rect.y + (rect.height - side) * 0.5,
                        side,
                        side,
                    ),
                    open,
                    hovered,
                    palette,
                    s,
                );
                let (Fold::Highlights(person) | Fold::Work(person) | Fold::Commits(person, _)) =
                    *fold;
                let (section, card) = self.people[usize::from(person)];
                let card = &self.sections[section].cards[card];
                let tally = card.tally();
                let (label, count) = match fold {
                    Fold::Highlights(_) => ("HIGHLIGHTS", card.did.len()),
                    _ if tally.pulls == tally.work => ("ALL PULL REQUESTS", tally.work),
                    _ => ("ALL WORK", tally.work),
                };
                let text_rect = Rect::new(
                    rect.x + 50.0 * s,
                    rect.y + (rect.height - 20.0 * s) * 0.5,
                    rect.width - 70.0 * s,
                    20.0 * s,
                );
                canvas.text(
                    label,
                    text_rect,
                    14.0 * s,
                    if hovered {
                        palette.shine
                    } else {
                        palette.title
                    },
                    FontWeight::Semibold,
                    3.0 * s,
                );
                canvas.text_fmt_aligned(
                    format_args!("{}   {count}", if open { "CLOSE" } else { "OPEN" }),
                    text_rect,
                    13.0 * s,
                    if hovered {
                        palette.shine
                    } else {
                        palette.muted
                    },
                    FontWeight::Semibold,
                    2.0 * s,
                    TextAlign::End,
                );
                if visible(rect) && frame.areas > 0 {
                    frame.areas -= 1;
                    canvas.hit_region(*token, rect);
                }
            }
            Piece::Highlight { rect, text, first } => {
                let rect = Rect::new(rect.x, top + rect.y + lift, rect.width, rect.height);
                let canvas = &mut self.ui;
                if *first {
                    let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                        rect: Rect::new(rect.x + 2.0 * s, rect.y + 9.0 * s, 6.0 * s, 6.0 * s),
                        radius: 3.0 * s,
                        color: palette.accent,
                    });
                }
                canvas.text(
                    text,
                    Rect::new(
                        rect.x + BULLET * s,
                        rect.y,
                        rect.width - BULLET * s,
                        rect.height,
                    ),
                    (BODY + 1.0) * s,
                    palette.text,
                    FontWeight::Regular,
                    0.2 * s,
                );
            }
            Piece::Day { rect, person, work } => {
                let rect = Rect::new(rect.x, top + rect.y + lift, rect.width, rect.height);
                let (section, card) = self.people[usize::from(*person)];
                let date = &self.sections[section].cards[card].work[usize::from(*work)].date;
                let canvas = &mut self.ui;
                let y = rect.y + rect.height - 14.0 * s;
                let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: Rect::new(rect.x + 7.0 * s, y - 1.0 * s, 8.0 * s, 8.0 * s),
                    radius: 4.0 * s,
                    color: palette.accent,
                });
                canvas.text(
                    date,
                    Rect::new(rect.x + 26.0 * s, y - 6.0 * s, 160.0 * s, 18.0 * s),
                    13.0 * s,
                    palette.accent,
                    FontWeight::Semibold,
                    2.0 * s,
                );
                horizontal(
                    canvas,
                    Rect::new(
                        rect.x + 150.0 * s,
                        y + 3.0 * s,
                        rect.width - 150.0 * s,
                        s.max(1.0),
                    ),
                    alpha(palette.accent, 0.45),
                    alpha(palette.accent, 0.0),
                );
            }
            Piece::Work {
                rect,
                person,
                work,
                lines,
                token,
                reference,
            } => {
                let rect = Rect::new(rect.x, top + rect.y + lift, rect.width, rect.height);
                let (section, card) = self.people[usize::from(*person)];
                let piece = &self.sections[section].cards[card].work[usize::from(*work)];
                let fold = Fold::Commits(*person, *work);
                let open = self.folds.contains(&fold);
                let canvas = &mut self.ui;
                let hovered = canvas.token_hovered(*token);
                // The day's thread runs down the left of its rows.
                let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
                    rect: Rect::new(rect.x + 10.5 * s, rect.y, s.max(1.0), rect.height),
                    color: alpha(palette.accent, 0.3),
                });
                if hovered || open {
                    let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                        rect: Rect::new(
                            rect.x + 24.0 * s,
                            rect.y,
                            rect.width - 24.0 * s,
                            rect.height,
                        ),
                        radius: 8.0 * s,
                        color: alpha(palette.accent, if hovered { 0.12 } else { 0.06 }),
                    });
                }
                let side = 16.0 * s;
                let box_rect = Rect::new(rect.x + 3.0 * s, rect.y + 5.0 * s, side, side);
                if piece.is_single() {
                    let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                        rect: Rect::new(
                            box_rect.x + 4.5 * s,
                            box_rect.y + 4.5 * s,
                            7.0 * s,
                            7.0 * s,
                        ),
                        radius: 3.5 * s,
                        color: alpha(palette.accent, 0.8),
                    });
                } else {
                    fold_box(canvas, box_rect, open, hovered, palette, s);
                }
                let x = rect.x + BOX * s;
                let mut y = rect.y + 4.0 * s;
                let title_width = rect.width - BOX * s - WORK_TAIL * s;
                for line in lines {
                    canvas.text(
                        line,
                        Rect::new(x, y, title_width, WORK_LINE * s),
                        WORK * s,
                        if hovered { palette.shine } else { palette.text },
                        FontWeight::Regular,
                        0.2 * s,
                    );
                    y += WORK_LINE * s;
                }
                // Right: the commit count (or the commit's hash), then the number.
                let tail = Rect::new(
                    rect.right() - WORK_TAIL * s,
                    rect.y + 6.0 * s,
                    WORK_TAIL * s - 12.0 * s,
                    18.0 * s,
                );
                if piece.is_single() {
                    canvas.text_aligned(
                        &piece.commits[0].hash,
                        tail,
                        12.0 * s,
                        palette.muted,
                        FontWeight::Semibold,
                        1.0 * s,
                        TextAlign::End,
                    );
                } else {
                    let count = piece.commits.len();
                    canvas.text_fmt_aligned(
                        format_args!("{count} COMMIT{}", if count == 1 { "" } else { "S" }),
                        tail,
                        11.0 * s,
                        palette.muted,
                        FontWeight::Semibold,
                        1.6 * s,
                        TextAlign::End,
                    );
                }
                if visible(rect) && frame.areas > 0 {
                    frame.areas -= 1;
                    canvas.hit_region(*token, rect);
                }
                if let Some((url_token, width)) = *reference {
                    let chip_rect = Rect::new(
                        tail.x - 8.0 * s,
                        rect.y + 3.0 * s,
                        width + 16.0 * s,
                        22.0 * s,
                    );
                    let hovered = canvas.token_hovered(url_token);
                    let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                        rect: chip_rect,
                        radius: 11.0 * s,
                        color: alpha(palette.accent, if hovered { 0.35 } else { 0.14 }),
                    });
                    canvas.text_aligned(
                        &piece.reference,
                        Rect::new(
                            chip_rect.x,
                            chip_rect.y + 3.0 * s,
                            chip_rect.width,
                            16.0 * s,
                        ),
                        12.0 * s,
                        if hovered {
                            palette.shine
                        } else {
                            palette.accent
                        },
                        FontWeight::Semibold,
                        0.6 * s,
                        TextAlign::Center,
                    );
                    if visible(chip_rect) && frame.areas > 0 {
                        frame.areas -= 1;
                        canvas.hit_region(url_token, chip_rect);
                    }
                }
            }
            Piece::Commit {
                rect,
                person,
                work,
                commit,
                lines,
                token,
            } => {
                let rect = Rect::new(rect.x, top + rect.y + lift, rect.width, rect.height);
                let (section, card) = self.people[usize::from(*person)];
                let piece = &self.sections[section].cards[card].work[usize::from(*work)];
                let entry = &piece.commits[usize::from(*commit)];
                let canvas = &mut self.ui;
                let hovered = canvas.token_hovered(*token);
                let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
                    rect: Rect::new(rect.x + 10.5 * s, rect.y, s.max(1.0), rect.height),
                    color: alpha(palette.accent, 0.3),
                });
                // A branch line from the work's box down its commits.
                let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
                    rect: Rect::new(rect.x + 44.0 * s, rect.y, s.max(1.0), rect.height),
                    color: alpha(palette.accent, 0.35),
                });
                let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: Rect::new(rect.x + 41.5 * s, rect.y + 8.0 * s, 6.0 * s, 6.0 * s),
                    radius: 3.0 * s,
                    color: if hovered {
                        palette.shine
                    } else {
                        palette.accent
                    },
                });
                if hovered {
                    let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                        rect: Rect::new(
                            rect.x + 54.0 * s,
                            rect.y,
                            rect.width - 54.0 * s,
                            rect.height,
                        ),
                        radius: 6.0 * s,
                        color: alpha(palette.accent, 0.10),
                    });
                }
                let x = rect.x + 60.0 * s;
                canvas.text(
                    &entry.hash,
                    Rect::new(x, rect.y + 2.0 * s, HASH * s, COMMIT_LINE * s),
                    (COMMIT - 1.0) * s,
                    if hovered {
                        palette.shine
                    } else {
                        palette.accent
                    },
                    FontWeight::Semibold,
                    0.8 * s,
                );
                let by_width = if entry.by.is_empty() { 0.0 } else { 120.0 * s };
                let mut y = rect.y + 1.0 * s;
                for line in lines {
                    canvas.text(
                        line,
                        Rect::new(
                            x + HASH * s,
                            y,
                            rect.width - 60.0 * s - HASH * s - by_width,
                            COMMIT_LINE * s,
                        ),
                        COMMIT * s,
                        if hovered {
                            palette.text
                        } else {
                            alpha(palette.text, 0.78)
                        },
                        FontWeight::Regular,
                        0.2 * s,
                    );
                    y += COMMIT_LINE * s;
                }
                if !entry.by.is_empty() {
                    canvas.text_fmt_aligned(
                        format_args!("BY {}", crate::menu::classic::view::Caps(&entry.by)),
                        Rect::new(
                            rect.right() - by_width - 12.0 * s,
                            rect.y + 3.0 * s,
                            by_width,
                            16.0 * s,
                        ),
                        11.0 * s,
                        palette.muted,
                        FontWeight::Semibold,
                        1.4 * s,
                        TextAlign::End,
                    );
                }
                if visible(rect) && frame.areas > 0 {
                    frame.areas -= 1;
                    canvas.hit_region(*token, rect);
                }
            }
            Piece::Card {
                rect,
                section,
                card,
                lines,
                handle,
                links,
                ..
            } => {
                let rect = Rect::new(rect.x, top + rect.y + lift, rect.width, rect.height);
                let person = &self.sections[*section].cards[*card];
                let canvas = &mut self.ui;
                glass(canvas, rect, palette, pulse, 14.0 * s, s);
                let x = rect.x + PAD * s;
                let width = rect.width - PAD * 2.0 * s;
                let mut y = rect.y + 18.0 * s;
                let name_size = NAME * 0.8;
                canvas.text(
                    &person.name,
                    Rect::new(x, y, width, name_size * 1.25 * s),
                    name_size * s,
                    palette.title,
                    FontWeight::Semibold,
                    0.4 * s,
                );
                if let Some((token, handle_width)) = *handle {
                    link_text(
                        canvas,
                        &person.github,
                        Rect::new(x, y + 8.0 * s, width, 18.0 * s),
                        HANDLE * s,
                        TextAlign::End,
                        token,
                        handle_width,
                        palette,
                        s,
                        visible,
                        &mut frame.areas,
                    );
                }
                y += name_size * 1.25 * s + 4.0 * s;
                canvas.text(
                    &person.role,
                    Rect::new(x, y, width, 20.0 * s),
                    ROLE * s,
                    palette.muted,
                    FontWeight::Regular,
                    0.3 * s,
                );
                y += 30.0 * s;
                if !person.medals.is_empty() {
                    let mut chip_x = x;
                    for &(medal, count) in &person.medals {
                        chip_x += medal_chip(canvas, chip_x, y - 2.0 * s, medal, count, palette, s)
                            + 10.0 * s;
                    }
                    y += MEDAL_ROW * s;
                }
                if !lines.is_empty() {
                    let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
                        rect: Rect::new(x, y - 8.0 * s, width, s.max(1.0)),
                        color: alpha(palette.accent, 0.25),
                    });
                }
                for (line, first) in lines {
                    if *first {
                        y += 4.0 * s;
                        let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                            rect: Rect::new(x + 2.0 * s, y + 8.0 * s, 6.0 * s, 6.0 * s),
                            radius: 3.0 * s,
                            color: palette.accent,
                        });
                    }
                    canvas.text(
                        line,
                        Rect::new(x + BULLET * s, y, width - BULLET * s, BODY_LINE * s),
                        BODY * s,
                        palette.text,
                        FontWeight::Regular,
                        0.2 * s,
                    );
                    y += BODY_LINE * s;
                }
                if !links.is_empty() {
                    y += LINK_GAP * s;
                }
                for (&(token, label_width), entry) in links.iter().zip(&person.links) {
                    let hovered = canvas.token_hovered(token);
                    let color = if hovered {
                        palette.shine
                    } else {
                        palette.accent
                    };
                    canvas.text(
                        ">",
                        Rect::new(x + 1.0 * s, y, BULLET * s, BODY_LINE * s),
                        BODY * s,
                        color,
                        FontWeight::Semibold,
                        0.2 * s,
                    );
                    link_text(
                        canvas,
                        &entry.label,
                        Rect::new(x + BULLET * s, y, width - BULLET * s, BODY_LINE * s),
                        BODY * s,
                        TextAlign::Start,
                        token,
                        label_width,
                        palette,
                        s,
                        visible,
                        &mut frame.areas,
                    );
                    y += BODY_LINE * s;
                }
            }
        }
        let _ = self.ui.draw_list_mut().push(DrawCommand::PopOpacity);
    }

    /// A thin scrollbar at the right edge, dragged or clicked to jump.
    fn scrollbar(&mut self, viewport: [f32; 2], view_height: f32, palette: Palette, s: f32) {
        if self.max_scroll <= 0.0 {
            return;
        }
        let track = Rect::new(
            viewport[0] - 16.0 * s,
            24.0 * s,
            6.0 * s,
            view_height - 48.0 * s,
        );
        let active = self.ui.token_hovered(SCROLLBAR_TOKEN);
        let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: track,
            radius: track.width * 0.5,
            color: alpha(palette.accent, 0.10),
        });
        let shown = view_height / (self.max_scroll + view_height);
        let thumb_height = (track.height * shown).max(36.0 * s);
        let thumb_y = track.y + (track.height - thumb_height) * (self.scroll / self.max_scroll);
        let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: Rect::new(track.x, thumb_y, track.width, thumb_height),
            radius: track.width * 0.5,
            color: alpha(palette.accent, if active { 0.95 } else { 0.55 }),
        });
        // A wider grip than the bar drawn, so it is easy to catch.
        self.ui.scroll_region(
            SCROLLBAR_TOKEN,
            Rect::new(
                track.x - 8.0 * s,
                track.y,
                track.width + 16.0 * s,
                track.height,
            ),
        );
    }

    /// Expand all / Collapse all and the close cap, and a scroll cue, over a
    /// dark strip.
    fn footer(&mut self, viewport: [f32; 2], height: f32, palette: Palette, s: f32) {
        let strip = Rect::new(0.0, viewport[1] - height, viewport[0], height);
        let _ = self.ui.draw_list_mut().push(DrawCommand::GradientRect {
            rect: strip,
            radius: 0.0,
            gradient: Gradient {
                start: alpha(palette.deep[1], 0.0),
                end: palette.deep[1],
                vertical: true,
            },
        });
        let close = Rect::new(
            viewport[0] * 0.5 + 8.0 * s,
            strip.y + 14.0 * s,
            200.0 * s,
            36.0 * s,
        );
        let all = Rect::new(
            close.x - 16.0 * s - 240.0 * s,
            close.y,
            240.0 * s,
            close.height,
        );
        let label = if self.folds.is_empty() {
            "EXPAND ALL"
        } else {
            "COLLAPSE ALL"
        };
        pill_button(&mut self.ui, ALL_TOKEN, label, all, palette, s);
        pill_button(&mut self.ui, BACK_TOKEN, "CLOSE", close, palette, s);
        if self.max_scroll > 0.0 && self.scroll < self.max_scroll - 1.0 {
            self.ui.text_aligned(
                "SCROLL FOR MORE",
                Rect::new(
                    close.right() + 24.0 * s,
                    close.y + 10.0 * s,
                    260.0 * s,
                    16.0 * s,
                ),
                11.0 * s,
                palette.muted,
                FontWeight::Semibold,
                2.0 * s,
                TextAlign::Start,
            );
        }
    }
}

/// What every piece of a frame shares, and the pointer areas rows have left.
struct Frame {
    top: f32,
    view_height: f32,
    palette: Palette,
    since: f32,
    now: f64,
    s: f32,
    areas: usize,
}

/// The pieces being laid out and what they will open.
struct Layout<'a> {
    pieces: &'a mut Vec<Piece>,
    born: &'a mut Vec<f64>,
    urls: &'a mut Vec<String>,
    actions: &'a mut Vec<Action>,
    folds: &'a BTreeSet<Fold>,
    /// The fold just opened (`None` inside: all of them) and when.
    reveal: Option<(Option<Fold>, f64)>,
    /// While laying out the rows of the fold just opened: how many so far.
    revealing: Option<usize>,
}

impl Layout<'_> {
    /// Add a piece, born now or, inside the fold just opened, in turn after
    /// the rows before it.
    fn push(&mut self, piece: Piece) {
        let born = match (self.revealing.as_mut(), self.reveal) {
            (Some(count), Some((_, at))) => {
                *count += 1;
                at + (*count).min(40) as f64 * 0.012
            }
            _ => f64::MIN / 2.0,
        };
        self.pieces.push(piece);
        self.born.push(born);
    }

    fn url(&mut self, url: String) -> u16 {
        let token = URL_BASE.saturating_add(self.urls.len() as u16);
        self.urls.push(url);
        token
    }

    fn action(&mut self, action: Action) -> u16 {
        let token = FOLD_BASE.saturating_add(self.actions.len() as u16);
        self.actions.push(action);
        token
    }

    /// Start laying out `fold`'s rows; returns whether it is open. Rows of
    /// the fold just opened (any, for Expand all) fade in after each other.
    fn enter(&mut self, fold: Fold) -> bool {
        let open = self.folds.contains(&fold);
        let revealed = self
            .reveal
            .is_some_and(|(which, _)| which.is_none_or(|which| which == fold));
        if open && revealed && self.revealing.is_none() {
            self.revealing = Some(0);
        }
        open
    }

    /// Done with `fold`'s rows.
    fn leave(&mut self, fold: Fold) {
        if self.reveal.is_some_and(|(which, _)| which == Some(fold)) {
            self.revealing = None;
        }
    }

    /// A fold's bar at `y`; returns the row below it.
    fn bar(&mut self, fold: Fold, x: f32, width: f32, y: f32, s: f32) -> f32 {
        let token = self.action(Action::Toggle(fold));
        self.push(Piece::Bar {
            rect: Rect::new(x, y, width, BAR * s),
            fold,
            token,
        });
        y + BAR * s + 8.0 * s
    }

    /// Lay out the panel of `card`, person number `person`, from `y`; returns
    /// its bottom.
    #[allow(clippy::too_many_arguments)]
    fn person(
        &mut self,
        card: &data::Card,
        person: u16,
        featured: bool,
        x: f32,
        width: f32,
        y: f32,
        s: f32,
        measure: &impl Fn(&str, f32) -> f32,
    ) -> f32 {
        let panel = self.pieces.len();
        let inner_x = x + HERO_PAD * s;
        let inner = width - HERO_PAD * 2.0 * s;
        let handle = card.github_url().map(|url| {
            let width = link_width(measure, &card.github, HANDLE * 1.15, s);
            (self.url(url), width)
        });
        let links: Vec<_> = card
            .links
            .iter()
            .map(|entry| {
                (
                    self.url(entry.url.clone()),
                    link_width(measure, &entry.label, BODY, s),
                )
            })
            .collect();
        let role = wrap(&card.role, inner, |text| measure(text, HERO_ROLE));
        let name_size = HERO_NAME[usize::from(!featured)] * s;
        let mut cy = y + 26.0 * s + name_size * 1.15 + 6.0 * s;
        cy += role.len() as f32 * HERO_ROLE * 1.4 * s + 14.0 * s + 30.0 * s;
        if !links.is_empty() {
            cy += 14.0 * s + BODY_LINE * s;
        }
        cy += 24.0 * s;
        let head = cy - y - 44.0 * s;
        self.push(Piece::Person {
            rect: Rect::new(x, y, width, 0.0),
            person,
            head,
            role,
            handle,
            links,
        });

        if !card.did.is_empty() {
            let fold = Fold::Highlights(person);
            cy = self.bar(fold, inner_x, inner, cy, s);
            if self.enter(fold) {
                cy += 6.0 * s;
                let text_width = inner - BULLET * s - 16.0 * s;
                for did in &card.did {
                    for (index, line) in wrap(did, text_width, |text| measure(text, BODY + 1.0))
                        .into_iter()
                        .enumerate()
                    {
                        if index == 0 {
                            cy += 5.0 * s;
                        }
                        self.push(Piece::Highlight {
                            rect: Rect::new(
                                inner_x + 16.0 * s,
                                cy,
                                inner - 16.0 * s,
                                BODY_LINE * s,
                            ),
                            text: line,
                            first: index == 0,
                        });
                        cy += (BODY_LINE + 1.0) * s;
                    }
                }
                cy += 14.0 * s;
            }
            self.leave(fold);
        }

        let fold = Fold::Work(person);
        cy = self.bar(fold, inner_x, inner, cy, s);
        if self.enter(fold) {
            let row_x = inner_x + 8.0 * s;
            let row_width = inner - 8.0 * s;
            let title_width = row_width - BOX * s - WORK_TAIL * s;
            let subject_width = row_width - 60.0 * s - HASH * s;
            let mut day = "";
            for (index, piece) in card.work.iter().enumerate() {
                let work = index as u16;
                if piece.date != day {
                    day = &piece.date;
                    self.push(Piece::Day {
                        rect: Rect::new(row_x, cy, row_width, 34.0 * s),
                        person,
                        work,
                    });
                    cy += 34.0 * s + 4.0 * s;
                }
                let single = piece.is_single();
                let action = if single {
                    Action::Open(self.url(piece.commits[0].url()) - URL_BASE)
                } else {
                    Action::Toggle(Fold::Commits(person, work))
                };
                let token = self.action(action);
                let reference = piece.url().map(|url| {
                    let width =
                        measure(&piece.reference, 12.0) + piece.reference.len() as f32 * 0.6 * s;
                    (self.url(url), width)
                });
                let lines = wrap(&piece.title, title_width, |text| measure(text, WORK));
                let height = lines.len().max(1) as f32 * WORK_LINE * s + 8.0 * s;
                self.push(Piece::Work {
                    rect: Rect::new(row_x, cy, row_width, height),
                    person,
                    work,
                    lines,
                    token,
                    reference,
                });
                cy += height + 2.0 * s;
                if single {
                    continue;
                }
                let fold = Fold::Commits(person, work);
                if self.enter(fold) {
                    for (index, commit) in piece.commits.iter().enumerate() {
                        let lines = wrap(
                            &commit.subject,
                            subject_width - if commit.by.is_empty() { 0.0 } else { 120.0 * s },
                            |text| measure(text, COMMIT),
                        );
                        let height = lines.len().max(1) as f32 * COMMIT_LINE * s + 4.0 * s;
                        let token = self.url(commit.url());
                        self.push(Piece::Commit {
                            rect: Rect::new(row_x, cy, row_width, height),
                            person,
                            work,
                            commit: index as u16,
                            lines,
                            token,
                        });
                        cy += height;
                    }
                    cy += 8.0 * s;
                }
                self.leave(fold);
            }
            cy += 10.0 * s;
        }
        self.leave(fold);
        cy += 22.0 * s;
        if let Piece::Person { rect, .. } = &mut self.pieces[panel] {
            rect.height = cy - y;
        }
        cy
    }
}

/// The width of link text at `size`: [`link_text`] draws it semibold and
/// wider spaced than `measure`'s regular text.
fn link_width(measure: &impl Fn(&str, f32) -> f32, text: &str, size: f32, s: f32) -> f32 {
    measure(text, size) * 1.06 + text.len() as f32 * 0.8 * s
}

/// `text` wrapped to `width`, as owned lines.
fn wrap(text: &str, width: f32, measure: impl Fn(&str) -> f32) -> Vec<String> {
    crate::console::changelog::wrap_words(text, width, measure)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// A glass card: a soft glow round it that pulses with `pulse`, the fill, a
/// sheen on its upper half, the edge and a bright cap shining from the middle
/// of its top.
fn glass(canvas: &mut MenuCanvas, rect: Rect, palette: Palette, pulse: f32, radius: f32, s: f32) {
    let draw = canvas.draw_list_mut();
    for ring in 1..=3 {
        let grow = ring as f32 * 3.0 * s;
        let _ = draw.push(DrawCommand::RoundedRect {
            rect: Rect::new(
                rect.x - grow,
                rect.y - grow,
                rect.width + grow * 2.0,
                rect.height + grow * 2.0,
            ),
            radius: radius + grow,
            color: alpha(palette.accent, (0.03 + 0.05 * pulse) / ring as f32),
        });
    }
    let _ = draw.push(DrawCommand::RoundedRect {
        rect,
        radius,
        color: palette.card,
    });
    let _ = draw.push(DrawCommand::GradientRect {
        rect: Rect::new(rect.x, rect.y, rect.width, rect.height.min(240.0 * s) * 0.5),
        radius,
        gradient: Gradient {
            start: Color::new(1.0, 1.0, 1.0, 0.06),
            end: Color::new(1.0, 1.0, 1.0, 0.0),
            vertical: true,
        },
    });
    let _ = draw.push(DrawCommand::Border {
        rect,
        radius,
        width: (1.5 * s).max(1.0),
        color: alpha(palette.accent, 0.25 + 0.45 * pulse),
    });
    let cap = Rect::new(
        rect.x + 18.0 * s,
        rect.y,
        (rect.width - 36.0 * s) * 0.5,
        2.0 * s,
    );
    horizontal(
        canvas,
        cap,
        alpha(palette.shine, 0.0),
        alpha(palette.shine, 0.7 * pulse + 0.2),
    );
    horizontal(
        canvas,
        Rect::new(cap.right(), cap.y, cap.width, cap.height),
        alpha(palette.shine, 0.7 * pulse + 0.2),
        alpha(palette.shine, 0.0),
    );
}

/// A band of light crossing `area` every `period` seconds (text drawn later
/// shows above it).
fn glint(canvas: &mut MenuCanvas, area: Rect, palette: Palette, now: f64, period: f64, width: f32) {
    let phase = ((now % period) / period) as f32;
    let x = area.x - width + phase * (area.width + width * 2.0);
    let _ = canvas.draw_list_mut().push(DrawCommand::PushClip(area));
    let band = Rect::new(x, area.y, width * 0.5, area.height);
    horizontal(
        canvas,
        band,
        alpha(palette.shine, 0.0),
        alpha(palette.shine, 0.2),
    );
    horizontal(
        canvas,
        Rect::new(band.right(), area.y, width * 0.5, area.height),
        alpha(palette.shine, 0.2),
        alpha(palette.shine, 0.0),
    );
    let _ = canvas.draw_list_mut().push(DrawCommand::PopClip);
}

/// A fold's box: a plus when closed, a minus when open.
fn fold_box(
    canvas: &mut MenuCanvas,
    rect: Rect,
    open: bool,
    hovered: bool,
    palette: Palette,
    s: f32,
) {
    let radius = 4.0 * s;
    let draw = canvas.draw_list_mut();
    let _ = draw.push(DrawCommand::RoundedRect {
        rect,
        radius,
        color: alpha(
            palette.accent,
            if open {
                0.9
            } else if hovered {
                0.3
            } else {
                0.12
            },
        ),
    });
    let _ = draw.push(DrawCommand::Border {
        rect,
        radius,
        width: s.max(1.0),
        color: alpha(palette.accent, 0.9),
    });
    let mark = if open { palette.deep[1] } else { palette.shine };
    let thick = (2.0 * s).max(1.0);
    let length = rect.width * 0.5;
    let middle = [rect.x + rect.width * 0.5, rect.y + rect.height * 0.5];
    let _ = draw.push(DrawCommand::SolidRect {
        rect: Rect::new(
            middle[0] - length * 0.5,
            middle[1] - thick * 0.5,
            length,
            thick,
        ),
        color: mark,
    });
    if !open {
        let _ = draw.push(DrawCommand::SolidRect {
            rect: Rect::new(
                middle[0] - thick * 0.5,
                middle[1] - length * 0.5,
                thick,
                length,
            ),
            color: mark,
        });
    }
}

/// A rounded count chip: the number bold, then its label.
fn chip(
    canvas: &mut MenuCanvas,
    x: f32,
    y: f32,
    count: usize,
    label: &str,
    palette: Palette,
    s: f32,
) -> f32 {
    // "1 COMMIT", "2 COMMITS".
    let label = if count == 1 {
        label.strip_suffix('S').unwrap_or(label)
    } else {
        label
    };
    let digits = count.to_string().len() as f32;
    let width = (digits * 11.0 + label.len() as f32 * 9.4 + 34.0) * s;
    let rect = Rect::new(x, y, width, 30.0 * s);
    let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
        rect,
        radius: 15.0 * s,
        color: alpha(palette.accent, 0.12),
    });
    let _ = canvas.draw_list_mut().push(DrawCommand::Border {
        rect,
        radius: 15.0 * s,
        width: s.max(1.0),
        color: alpha(palette.accent, 0.4),
    });
    let number = Rect::new(
        x + 14.0 * s,
        y + 6.0 * s,
        digits * 11.0 * s + 4.0 * s,
        18.0 * s,
    );
    canvas.text_fmt_aligned(
        format_args!("{count}"),
        number,
        16.0 * s,
        palette.shine,
        FontWeight::Semibold,
        0.4 * s,
        TextAlign::Start,
    );
    canvas.text(
        label,
        Rect::new(number.right() + 6.0 * s, y + 8.0 * s, width, 16.0 * s),
        11.0 * s,
        palette.accent,
        FontWeight::Semibold,
        2.2 * s,
    );
    width
}

/// A medal's chip, as wide as it needs: its medallion, then its name (with how
/// often it was given, past once) in the count chips' lettering.
fn medal_chip(
    canvas: &mut MenuCanvas,
    x: f32,
    y: f32,
    medal: Medal,
    count: u32,
    palette: Palette,
    s: f32,
) -> f32 {
    let times = if count > 1 {
        3 + count.checked_ilog10().unwrap_or(0) as usize
    } else {
        0
    };
    // The label's capitals, spaced as the count chips', are about 8.2 a letter.
    let letters = (medal.name().len() + times) as f32;
    let width = (38.0 + letters * 8.2 + 20.0) * s;
    let rect = Rect::new(x, y, width, 30.0 * s);
    let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
        rect,
        radius: 15.0 * s,
        color: alpha(palette.accent, 0.12),
    });
    let _ = canvas.draw_list_mut().push(DrawCommand::Border {
        rect,
        radius: 15.0 * s,
        width: s.max(1.0),
        color: alpha(palette.accent, 0.4),
    });
    let _ = canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
        rect: Rect::new(x + 5.0 * s, y + 2.0 * s, 26.0 * s, 26.0 * s),
        texture: medal.icon(),
        color: Color::new(1.0, 1.0, 1.0, 1.0),
    });
    let label = Rect::new(x + 38.0 * s, y + 8.0 * s, width, 16.0 * s);
    let caps = crate::menu::classic::view::Caps(medal.name());
    let mut write = |args: std::fmt::Arguments<'_>| {
        canvas.text_fmt_aligned(
            args,
            label,
            11.0 * s,
            palette.shine,
            FontWeight::Semibold,
            2.2 * s,
            TextAlign::Start,
        );
    };
    if count > 1 {
        write(format_args!("{caps} X{count}"));
    } else {
        write(format_args!("{caps}"));
    }
    width
}

/// Text that opens an address: shines and is underlined under the pointer,
/// and takes clicks while shown and while the frame has areas left.
#[allow(clippy::too_many_arguments)]
fn link_text(
    canvas: &mut MenuCanvas,
    text: &str,
    area: Rect,
    size: f32,
    align: TextAlign,
    token: u16,
    text_width: f32,
    palette: Palette,
    s: f32,
    visible: impl Fn(Rect) -> bool,
    areas: &mut usize,
) {
    let hovered = canvas.token_hovered(token);
    // Room past the measured width, so rounding never cuts the text short.
    let room = Rect::new(
        area.x,
        area.y,
        area.width.max(text_width + 12.0 * s),
        area.height,
    );
    let room = match align {
        TextAlign::End => Rect::new(area.right() - room.width, area.y, room.width, area.height),
        _ => room,
    };
    canvas.text_aligned(
        text,
        room,
        size,
        if hovered {
            palette.shine
        } else {
            palette.accent
        },
        FontWeight::Semibold,
        1.0 * s,
        align,
    );
    let left = match align {
        TextAlign::End => area.right() - text_width,
        _ => area.x,
    };
    let target = Rect::new(
        left - 4.0 * s,
        area.y - 4.0 * s,
        text_width + 8.0 * s,
        area.height + 8.0 * s,
    );
    if hovered {
        let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
            rect: Rect::new(left, target.bottom() - 4.0 * s, text_width, s.max(1.0)),
            color: alpha(palette.shine, 0.8),
        });
    }
    if visible(target) && *areas > 0 {
        *areas -= 1;
        canvas.hit_region(token, target);
    }
}

/// A footer button: an outlined pill that fills under the pointer.
fn pill_button(
    canvas: &mut MenuCanvas,
    token: u16,
    label: &str,
    rect: Rect,
    palette: Palette,
    s: f32,
) {
    let hovered = canvas.token_hovered(token);
    let radius = rect.height * 0.5;
    if hovered {
        let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
            rect,
            radius,
            color: alpha(palette.accent, 0.18),
        });
    }
    let _ = canvas.draw_list_mut().push(DrawCommand::Border {
        rect,
        radius,
        width: (1.5 * s).max(1.0),
        color: alpha(palette.accent, if hovered { 1.0 } else { 0.55 }),
    });
    canvas.text_aligned(
        label,
        Rect::new(rect.x, rect.y + 8.0 * s, rect.width, 20.0 * s),
        15.0 * s,
        if hovered {
            palette.shine
        } else {
            palette.accent
        },
        FontWeight::Semibold,
        3.0 * s,
        TextAlign::Center,
    );
    canvas.hit_region(token, rect);
}

/// Height above the first section: emblem, title and the line under it.
fn header_height(s: f32) -> f32 {
    300.0 * s
}

/// Space above a card's links.
const LINK_GAP: f32 = 10.0;

/// Room a card's row of medal chips takes under its role.
const MEDAL_ROW: f32 = 38.0;

/// A card's height for `lines` wrapped lines from `items` contributions,
/// `links` links and a row of medals when it has `medals`.
fn card_height(lines: usize, items: usize, links: usize, medals: bool, s: f32) -> f32 {
    let medals = if medals { MEDAL_ROW } else { 0.0 };
    let head = 18.0 + NAME * 1.25 + 4.0 + 30.0 + medals;
    let body = lines as f32 * BODY_LINE + items as f32 * 4.0;
    let links = if links == 0 {
        0.0
    } else {
        LINK_GAP + links as f32 * BODY_LINE
    };
    (head + body + links + 22.0) * s
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

fn horizontal(canvas: &mut MenuCanvas, rect: Rect, start: Color, end: Color) {
    let _ = canvas.draw_list_mut().push(DrawCommand::GradientRect {
        rect,
        radius: 0.0,
        gradient: Gradient {
            start,
            end,
            vertical: false,
        },
    });
}

/// The deep backdrop, golden god rays turning slowly down from above the top
/// of the screen (two sets turning against each other, so the shafts shimmer
/// as they cross, after the site), and sparks rising through them, all from
/// the clock so nothing is stored per frame.
fn backdrop(canvas: &mut MenuCanvas, viewport: [f32; 2], palette: Palette, now: f32) {
    let [width, height] = viewport;
    let _ = canvas.draw_list_mut().push(DrawCommand::GradientRect {
        rect: Rect::new(0.0, 0.0, width, height),
        radius: 0.0,
        gradient: Gradient {
            start: palette.deep[0],
            end: palette.deep[1],
            vertical: true,
        },
    });
    let source = [width * 0.5, -height * 0.1];
    let reach = width.max(height) * 1.05;
    let turn = now % 3_600.0;
    let breathe = 0.5 + 0.5 * (now * 0.37).sin();
    emblem::rays(
        canvas,
        EmblemLayer::Godrays,
        source,
        reach,
        turn * 0.021,
        alpha(RAY_GOLD, 0.42 + 0.10 * breathe),
    );
    emblem::rays(
        canvas,
        EmblemLayer::Godrays,
        source,
        reach * 0.9,
        -turn * 0.013 + 2.1,
        alpha(RAY_GOLD, 0.30 - 0.08 * breathe),
    );
    let s = crate::ui_scale::height_scale(height);
    sparks(
        canvas,
        Rect::new(0.0, 0.0, width, height),
        now,
        [palette.shine, palette.accent],
        (SPARKS, 0.55),
        s,
    );
}

/// `count` sparks rising through `area` (window pixels) at most `strength`
/// opaque, every third in `colors[0]` and the rest in `colors[1]`, from the
/// clock alone; `s` is the layout scale.
fn sparks(
    canvas: &mut MenuCanvas,
    area: Rect,
    now: f32,
    colors: [Color; 2],
    (count, strength): (usize, f32),
    s: f32,
) {
    for spark in 0..count {
        let seed = emblem::hash(spark as u32);
        let column = (seed & 0xffff) as f32 / 65_535.0;
        let speed = 18.0 + ((seed >> 16) & 0xff) as f32 / 255.0 * 46.0;
        let start = ((seed >> 24) & 0xff) as f32 / 255.0;
        let rise = (start + now * speed * s / area.height.max(1.0)).fract();
        let y = area.y + area.height * (1.0 - rise);
        let sway = (now * 0.6 + spark as f32).sin() * 14.0 * s;
        let size = (1.5 + (seed % 5) as f32 * 0.6) * s;
        let twinkle = 0.25 + 0.75 * (0.5 + 0.5 * (now * 2.3 + spark as f32 * 1.7).sin());
        // Sparks fade in at the bottom and out at the top.
        let fade = (rise * 4.0).min(1.0) * ((1.0 - rise) * 3.0).min(1.0);
        let color = colors[usize::from(spark % 3 != 0)];
        let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: Rect::new(area.x + column * area.width + sway, y, size, size),
            radius: size * 0.5,
            color: alpha(color, strength * twinkle * fade),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrolling_stays_in_range_and_close_reports_ownership() {
        let mut panel = Panel::new();
        assert!(panel.error.is_none(), "{:?}", panel.error);
        panel.max_scroll = 500.0;
        panel.scroll_by(10_000.0);
        assert_eq!(panel.target, 500.0);
        panel.scroll_by(-10_000.0);
        assert_eq!(panel.target, 0.0);
        assert!(!panel.close());
        panel.open(true);
        assert!(panel.close());
    }

    #[test]
    fn sparks_spread_across_the_width() {
        let columns: Vec<u32> = (0..SPARKS as u32)
            .map(|i| (emblem::hash(i) & 0xffff) / 6_554)
            .collect();
        for bucket in 0..10 {
            assert!(columns.contains(&bucket), "no spark in tenth {bucket}");
        }
    }

    #[test]
    fn everyone_with_a_history_has_a_panel_and_everything_starts_folded() {
        let mut panel = Panel::new();
        let names: Vec<_> = panel
            .people
            .iter()
            .map(|&(section, card)| panel.sections[section].cards[card].name.as_str())
            .collect();
        assert_eq!(names[0], "Sol");
        assert!(names.contains(&"Bishop") && names.contains(&"Creyon"));
        panel.open(false);
        assert!(panel.folds.is_empty());
        panel.toggle(Fold::Work(0));
        panel.toggle(Fold::Commits(0, 3));
        assert_eq!(panel.folds.len(), 2);
        panel.toggle(Fold::Commits(0, 3));
        assert_eq!(panel.folds.len(), 1);
        // Expand all opens every fold; pressed again, it closes them.
        panel.folds.clear();
        panel.toggle_all();
        let opened = panel.folds.len();
        let works: usize = panel
            .people
            .iter()
            .map(|&(section, card)| {
                let card = &panel.sections[section].cards[card];
                card.work.iter().filter(|w| !w.is_single()).count()
            })
            .sum();
        assert!(opened > works, "{opened} folds for {works} pieces");
        panel.toggle_all();
        assert!(panel.folds.is_empty());
        // Reopening the page folds everything again.
        panel.toggle(Fold::Highlights(1));
        panel.open(false);
        assert!(panel.folds.is_empty());
    }
}
