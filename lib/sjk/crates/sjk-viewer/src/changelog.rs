//! The changelog page: every SJK release with its changes and their credits,
//! read from `CHANGELOG.md` (built in, parsed once; see `changelog_data.rs`).
//!
//! Opened by the main menu's Changelog entry or the `changelog` console
//! command. Like the `debug_panel` test list it lives in the
//! console and is drawn in place of it, so it opens over the menus and in a
//! match; opened with the console closed, it closes the console again with
//! itself. Releases are listed newest first on the left; the selected one's
//! introduction and changes, each followed by its credit, are wrapped on a
//! scrolling pane on the right. With `ui_menuStyle classic` the page takes the
//! classic+ look of the command browser's pop-up (`changelog_classic.rs`), with
//! the SJK UI its own (`changelog_sjk.rs`).

use crate::menu::art::ArtSet;
use crate::menu_widgets::{BACK_TOKEN, MenuCanvas};
use crate::text::{TextFace, UiFont, visible_text_width_style};
use sjk_ui::{FontWeight, InputEvent, Rect, TextAlign, UiEventKind};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

#[path = "changelog_classic.rs"]
mod classic;
#[path = "changelog_data.rs"]
mod data;
#[path = "changelog_sjk.rs"]
mod sjk;

/// Console command that toggles the page; keys bound to it toggle it too.
pub(crate) const COMMAND: &str = "changelog";
/// Help text for completion and `cmdlist`.
pub(crate) const HELP: &str = "Show every SJK release's changes and credits";

/// Row `slot` on screen answers to `ROW_BASE + slot`.
const ROW_BASE: u16 = 1_000;
/// Most release rows drawn at once.
const ROW_LIMIT: usize = 100;
/// Wheel target over the release pane.
const PANE_TOKEN: u16 = 906;
/// Draggable thumb beside the pane's lines.
const PANE_BAR_TOKEN: u16 = 910;
/// Pane lines one wheel notch scrolls.
const WHEEL_LINES: usize = 3;
/// The notes' chips answer to `CHIP_BASE + n`, in reading order (the SJK UI's look).
const CHIP_BASE: u16 = 3_000;

/// Releases the what's-new card pages through at most, newest first.
pub(crate) const DIGEST_LIMIT: usize = 5;

/// A release as the what's-new card (`whats_new_card.rs`) shows it.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Digest {
    /// "2026.1011.3 (Alpha)", or "Unreleased".
    pub(crate) title: String,
    /// dd/mm/yyyy; empty for "Unreleased".
    pub(crate) date: String,
    /// The release's first introduction paragraph, which stands in when it has no recap.
    pub(crate) intro: String,
    /// The recap: a heading and a sentence each.
    pub(crate) lines: Vec<(String, String)>,
    /// Who made the release's changes, and the clients they follow ("after OpenJK"),
    /// each by how many changes they credit, most first.
    pub(crate) people: Vec<String>,
    pub(crate) after: Vec<String>,
}

/// What an update brought: the recaps of the releases after `seen` up to
/// `installed` (newest first, at most [`DIGEST_LIMIT`]). With no `seen`, or one
/// the file does not list, only `installed`'s. Empty when `installed` is not a
/// release the changelog lists (a local build).
pub(crate) fn digests(installed: &str, seen: &str) -> Vec<Digest> {
    data::parse(data::EMBEDDED)
        .map(|releases| digests_of(&releases, installed, seen))
        .unwrap_or_default()
}

/// The newest release's recap, "Unreleased" included: the `whatsnew` command's preview.
pub(crate) fn newest_digest() -> Option<Digest> {
    let releases = data::parse(data::EMBEDDED).ok()?;
    releases.first().map(digest)
}

/// The newest dated release's recap: what the what's-new card shows after an update.
/// "Unreleased" has none until the release is cut.
#[cfg(test)]
pub(crate) fn newest_release_digest() -> Option<Digest> {
    let releases = data::parse(data::EMBEDDED).ok()?;
    releases
        .iter()
        .find(|release| release.version().is_some())
        .map(digest)
}

fn digests_of(releases: &[data::Release], installed: &str, seen: &str) -> Vec<Digest> {
    let Some(newest) = releases.iter().position(|r| r.version() == Some(installed)) else {
        return Vec::new();
    };
    let first_seen = releases
        .iter()
        .position(|r| !seen.is_empty() && r.version() == Some(seen))
        .filter(|at| *at > newest);
    let end = first_seen.unwrap_or(newest + 1).min(newest + DIGEST_LIMIT);
    releases[newest..end].iter().map(digest).collect()
}

fn digest(release: &data::Release) -> Digest {
    let (people, after) = credited(release.changes.iter().map(|change| change.credit.as_str()));
    Digest {
        title: release.title.clone(),
        date: release.date.clone(),
        intro: release.intro.first().cloned().unwrap_or_default(),
        lines: release
            .recap
            .iter()
            .map(|line| (line.heading.clone(), line.text.clone()))
            .collect(),
        people,
        after,
    }
}

/// The people and the followed clients `credits` name, each by how many credits name
/// it, most first (the first named first on a tie). A credit is "<name>[, after
/// <client>][; <name>...]" (`CHANGELOG.md`'s format); other notes in it are left out.
fn credited<'a>(credits: impl Iterator<Item = &'a str>) -> (Vec<String>, Vec<String>) {
    let mut people: Vec<(String, usize)> = Vec::new();
    let mut after: Vec<(String, usize)> = Vec::new();
    let count = |list: &mut Vec<(String, usize)>, name: &str| {
        if name.is_empty() {
            return;
        }
        match list.iter_mut().find(|(known, _)| known == name) {
            Some((_, times)) => *times += 1,
            None => list.push((name.to_owned(), 1)),
        }
    };
    for credit in credits {
        for part in credit.split(';') {
            let mut pieces = part.split(',').map(str::trim);
            count(&mut people, pieces.next().unwrap_or_default());
            for piece in pieces {
                if let Some(client) = piece.strip_prefix("after ") {
                    count(&mut after, client.trim());
                }
            }
        }
    }
    let ranked = |mut list: Vec<(String, usize)>| {
        // A stable sort keeps the first named first on a tie.
        list.sort_by_key(|(_, times)| std::cmp::Reverse(*times));
        list.into_iter().map(|(name, _)| name).collect()
    };
    (ranked(people), ranked(after))
}

/// What the console does after the page handled an event.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PanelAction {
    None,
    Close,
    /// A chip was clicked: its text (`ViewerConsole::follow_chip`).
    Chip(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LineKind {
    Intro,
    /// The first line of a change, drawn with a bullet.
    Change,
    /// A change's later lines.
    More,
    Credit,
    Gap,
}

struct Line {
    kind: LineKind,
    text: String,
    /// The line laid out with its chips, in the SJK UI's look (`changelog_sjk.rs`);
    /// empty in the classic+ look, which draws `text`.
    row: crate::inline_code::Row,
}

pub(crate) struct Panel {
    open: bool,
    /// The page opened the console, so closing the page closes it too.
    owns_console: bool,
    releases: Vec<data::Release>,
    /// A parse error of the built-in file, shown instead of the list.
    error: Option<String>,
    selected: usize,
    /// First release row on screen, and how many fit.
    first: usize,
    rows: usize,
    /// The selected release wrapped for the pane, and what it was wrapped for:
    /// release, pane width and text size (bits).
    lines: Vec<Line>,
    wrapped_for: Option<(usize, u32, u32)>,
    /// First pane line on screen, and the last value that still fills the pane.
    scroll: usize,
    max_scroll: usize,
    /// Pane lines the last frame showed, for Page Up / Page Down.
    page_lines: usize,
    /// The retail menu art the classic+ look can draw.
    art: ArtSet,
    /// The SJK UI's look (`changelog_sjk.rs`), else the classic+ one.
    sjk: bool,
    /// The chips drawn this frame, by token from [`CHIP_BASE`].
    chips: Vec<String>,
    ui: MenuCanvas,
}

impl Default for Panel {
    fn default() -> Self {
        Self::new()
    }
}

impl Panel {
    pub(crate) fn new() -> Self {
        let (releases, error) = match data::parse(data::EMBEDDED) {
            Ok(releases) => (releases, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        Self {
            open: false,
            owns_console: false,
            releases,
            error,
            selected: 0,
            first: 0,
            rows: 1,
            lines: Vec::new(),
            wrapped_for: None,
            scroll: 0,
            max_scroll: 0,
            page_lines: 1,
            art: ArtSet::default(),
            sjk: false,
            chips: Vec::new(),
            ui: MenuCanvas::with_text_capacity(256),
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// The retail `art` the classic+ look can draw.
    pub(crate) fn set_art(&mut self, art: ArtSet) {
        self.art = art;
    }

    /// Whether the classic+ look is drawn, so its text can use the retail font.
    pub(crate) fn is_classic(&self) -> bool {
        !self.sjk
    }

    /// Draw the SJK UI's look (`sjk`), in its families, or not.
    pub(crate) fn set_sjk(&mut self, sjk: bool) {
        self.sjk = sjk;
    }

    /// Whether the SJK UI's look is drawn.
    pub(crate) fn is_sjk(&self) -> bool {
        self.sjk
    }

    /// Show the page; `owns_console` when the console was closed before it.
    pub(crate) fn open(&mut self, owns_console: bool) {
        self.open = true;
        self.owns_console = owns_console;
    }

    /// Hide the page; returns whether it had opened the console.
    pub(crate) fn close(&mut self) -> bool {
        let owned = self.open && self.owns_console;
        self.open = false;
        self.owns_console = false;
        owned
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    pub(crate) fn handle_key(&mut self, event: &KeyEvent) -> PanelAction {
        if event.state != ElementState::Pressed {
            return PanelAction::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return PanelAction::None;
        };
        match key {
            KeyCode::Escape => return PanelAction::Close,
            KeyCode::ArrowUp => self.select(self.selected.saturating_sub(1)),
            KeyCode::ArrowDown => self.select(self.selected + 1),
            KeyCode::Home => self.select(0),
            KeyCode::End => self.select(usize::MAX),
            KeyCode::PageUp => self.scroll_pane(-(self.page_lines.max(1) as isize)),
            KeyCode::PageDown => self.scroll_pane(self.page_lines.max(1) as isize),
            _ => {}
        }
        PanelAction::None
    }

    pub(crate) fn handle_pointer(&mut self, event: InputEvent) -> PanelAction {
        let Some(event) = self.ui.pointer(event) else {
            return PanelAction::None;
        };
        if event.kind == UiEventKind::Wheel {
            let direction = event.delta.map_or(0, |delta| -delta.y.signum() as isize);
            if matches!(event.token, Some(PANE_TOKEN | PANE_BAR_TOKEN)) {
                self.scroll_pane(direction * WHEEL_LINES as isize);
            } else {
                let last_first = self.releases.len().saturating_sub(self.rows);
                self.first = self.first.saturating_add_signed(direction).min(last_first);
            }
            return PanelAction::None;
        }
        if event.kind == UiEventKind::Drag && event.token == Some(PANE_BAR_TOKEN) {
            if let (Some(point), Some(track)) = (event.position, self.ui.rect_for(PANE_BAR_TOKEN)) {
                let ratio = ((point.y - track.y) / track.height).clamp(0.0, 1.0);
                self.scroll = (ratio * self.max_scroll as f32).round() as usize;
            }
            return PanelAction::None;
        }
        if event.kind != UiEventKind::Activate {
            return PanelAction::None;
        }
        match event.token {
            Some(BACK_TOKEN) => PanelAction::Close,
            Some(token) if (ROW_BASE..ROW_BASE + ROW_LIMIT as u16).contains(&token) => {
                self.select(self.first + usize::from(token - ROW_BASE));
                PanelAction::None
            }
            Some(token) if token >= CHIP_BASE => self
                .chips
                .get(usize::from(token - CHIP_BASE))
                .map_or(PanelAction::None, |chip| PanelAction::Chip(chip.clone())),
            _ => PanelAction::None,
        }
    }

    /// Select release `index` (clamped), from the top of its pane.
    pub(crate) fn select(&mut self, index: usize) {
        let index = index.min(self.releases.len().saturating_sub(1));
        if index != self.selected {
            self.selected = index;
            self.scroll = 0;
        }
        let rows = self.rows.max(1);
        self.first = self
            .first
            .min(self.selected)
            .max((self.selected + 1).saturating_sub(rows));
    }

    fn scroll_pane(&mut self, lines: isize) {
        self.scroll = self
            .scroll
            .saturating_add_signed(lines)
            .min(self.max_scroll);
    }

    /// Wrap the selected release for a pane `width` wide, in body text of
    /// `size` and `spacing` with changes indented by `indent` (pixels).
    fn wrap(&mut self, font: &UiFont, width: f32, size: f32, spacing: f32, indent: f32) {
        let key = (self.selected, width.to_bits(), size.to_bits());
        if self.wrapped_for == Some(key) {
            return;
        }
        self.wrapped_for = Some(key);
        self.lines.clear();
        let Some(release) = self.releases.get(self.selected) else {
            return;
        };
        let style = font.style();
        let measure = |text: &str, size: f32, spacing: f32, face: TextFace| {
            let placement = style.place(Rect::new(0.0, 0.0, 0.0, size), size, spacing);
            visible_text_width_style(
                font,
                text,
                placement.size / font.height.max(1.0),
                face,
                placement.letter_spacing,
            )
        };
        let body = |text: &str| measure(text, size, spacing, TextFace::Regular);
        for paragraph in &release.intro {
            for line in wrap_words(paragraph, width, body) {
                self.lines.push(Line {
                    kind: LineKind::Intro,
                    text: line.to_owned(),
                    row: Vec::new(),
                });
            }
            self.lines.push(Line {
                kind: LineKind::Gap,
                text: String::new(),
                row: Vec::new(),
            });
        }
        for change in &release.changes {
            let text = crate::inline_code::plain(&change.text);
            for (index, line) in wrap_words(&text, width - indent, body)
                .into_iter()
                .enumerate()
            {
                self.lines.push(Line {
                    kind: if index == 0 {
                        LineKind::Change
                    } else {
                        LineKind::More
                    },
                    text: line.to_owned(),
                    row: Vec::new(),
                });
            }
            self.lines.push(Line {
                kind: LineKind::Credit,
                text: change.credit.to_ascii_uppercase(),
                row: Vec::new(),
            });
            self.lines.push(Line {
                kind: LineKind::Gap,
                text: String::new(),
                row: Vec::new(),
            });
        }
        self.lines.pop();
    }

    /// Set the last scroll that still fills a pane `available` high, the lines
    /// being `height` high, and keep the scroll within it.
    fn fit_scroll(&mut self, available: f32, height: impl Fn(LineKind) -> f32) {
        let mut total = 0.0;
        self.max_scroll = self.lines.len();
        for (index, line) in self.lines.iter().enumerate().rev() {
            total += height(line.kind);
            if total > available {
                break;
            }
            self.max_scroll = index;
        }
        self.scroll = self.scroll.min(self.max_scroll);
    }
}

/// Split `text` at spaces into lines no wider than `width`; a word wider than
/// the line stays whole (the renderer ends it in an ellipsis).
pub(crate) fn wrap_words(text: &str, width: f32, measure: impl Fn(&str) -> f32) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut end = 0;
    for (index, _) in text.match_indices(' ').chain([(text.len(), "")]) {
        if end > start && measure(&text[start..index]) > width {
            lines.push(&text[start..end]);
            start = end + 1;
        }
        end = index;
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_cover_the_releases_since_the_last_seen_one() {
        let text = "## Unreleased\n\n> Next: soon\n\n- x _(Sol)_\n\n## 3.0 (Alpha) | 03/10/2026\n\nThird.\n\n> Three: third\n\n- x _(Sol)_\n\n## 2.0 | 02/10/2026\n\n> Two: second\n\n- x _(Sol)_\n\n## 1.0 | 01/10/2026\n\nFirst.\n\n- x _(Sol)_\n";
        let releases = data::parse(text).unwrap();
        let titles = |installed, seen| -> Vec<String> {
            digests_of(&releases, installed, seen)
                .into_iter()
                .map(|digest| digest.title)
                .collect()
        };
        assert_eq!(titles("3.0", "2.0"), ["3.0 (Alpha)"]);
        assert_eq!(titles("3.0", "1.0"), ["3.0 (Alpha)", "2.0"]);
        // No marker, an unknown one or a newer one: only the installed release.
        assert_eq!(titles("3.0", ""), ["3.0 (Alpha)"]);
        assert_eq!(titles("3.0", "0.5"), ["3.0 (Alpha)"]);
        assert_eq!(titles("2.0", "3.0"), ["2.0"]);
        // A local build, or "Unreleased", is not a release.
        assert!(titles("0.1.0-dev", "1.0").is_empty());
        assert!(titles("Unreleased", "").is_empty());
        let (people, after) = credited(
            [
                "Sol",
                "Creyon, after OpenJK",
                "Creyon, after JoF EJK; Sol",
                "Sol, fonts traced from a pack",
                "Lumaya, after OpenJK",
            ]
            .into_iter(),
        );
        assert_eq!(people, ["Sol", "Creyon", "Lumaya"]);
        assert_eq!(after, ["OpenJK", "JoF EJK"]);
        let third = &digests_of(&releases, "3.0", "2.0")[0];
        assert_eq!(third.intro, "Third.");
        assert_eq!(third.lines, [("Three".to_owned(), "third".to_owned())]);
        // A release without a recap still has its introduction.
        let first = &digests_of(&releases, "1.0", "")[0];
        assert!(first.lines.is_empty());
        assert_eq!(first.intro, "First.");
    }

    #[test]
    fn words_wrap_at_the_width() {
        let measure = |text: &str| text.len() as f32;
        assert_eq!(
            wrap_words("the quick brown fox jumps", 10.0, measure),
            ["the quick", "brown fox", "jumps"]
        );
        assert_eq!(wrap_words("short", 10.0, measure), ["short"]);
        assert_eq!(
            wrap_words("unbreakableword x", 5.0, measure),
            ["unbreakableword", "x"]
        );
        assert!(wrap_words("", 10.0, measure).is_empty());
    }

    #[test]
    fn selection_scroll_and_close_stay_in_range() {
        let mut panel = Panel::new();
        assert!(panel.error.is_none(), "{:?}", panel.error);
        panel.rows = 2;
        panel.select(usize::MAX);
        assert_eq!(panel.selected, panel.releases.len() - 1);
        assert_eq!(panel.first, panel.releases.len() - 2);
        panel.max_scroll = 4;
        panel.scroll_pane(10);
        assert_eq!(panel.scroll, 4);
        panel.select(0);
        assert_eq!((panel.selected, panel.first, panel.scroll), (0, 0, 0));
        assert!(!panel.close());
        panel.open(true);
        assert!(panel.close());
    }
}
