//! The Create game map list: every installed map the chosen mode is played
//! on, filtered live by what the player types (a case-insensitive substring
//! of the long name or the `mp/…` name), walked with the arrows, pages,
//! Home/End or the wheel, and picked with Enter or a click.

use super::create_game_catalog::{Catalogue, MapEntry};
use winit::keyboard::KeyCode;

/// Longest filter the list takes.
const FILTER_BYTES: usize = 32;
/// Rows the wheel scrolls per notch.
const WHEEL_ROWS: usize = 3;

/// What a key on the open list asks for.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PickerResult {
    /// Still choosing.
    None,
    /// Close the list; `Some` picks that map (`mp/ffa3`).
    Close(Option<String>),
}

/// Map list state: open flag, filter, the matching maps and the view.
pub(crate) struct MapPicker {
    open: bool,
    mode: usize,
    filter: String,
    /// Indices into [`Catalogue::maps`] of the maps shown, in list order.
    matches: Vec<usize>,
    /// Highlighted position in `matches`.
    selected: usize,
    /// First visible position in `matches`.
    first: usize,
    /// Rows the view fits; set by the view, used for paging and scrolling.
    page: usize,
}

impl MapPicker {
    pub(crate) fn new() -> Self {
        Self {
            open: false,
            mode: 0,
            filter: String::with_capacity(FILTER_BYTES),
            matches: Vec::with_capacity(256),
            selected: 0,
            first: 0,
            page: 12,
        }
    }

    /// Whether the list is up.
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Open on mode `mode`'s maps with no filter, `current` highlighted and
    /// scrolled into the middle of the view.
    pub(crate) fn open(&mut self, catalogue: &Catalogue, mode: usize, current: &str) {
        self.open = true;
        self.mode = mode;
        self.filter.clear();
        self.refilter(catalogue, Some(current));
        self.first = self.selected.saturating_sub(self.page / 2);
        self.clamp_scroll();
    }

    /// Close without picking.
    pub(crate) fn close(&mut self) {
        self.open = false;
    }

    /// What has been typed.
    pub(crate) fn filter(&self) -> &str {
        &self.filter
    }

    /// Maps the mode offers in all, before filtering.
    pub(crate) fn total(&self, catalogue: &Catalogue) -> usize {
        catalogue.maps_for(self.mode).count()
    }

    /// How many maps match the filter.
    pub(crate) fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Highlighted position among the matches.
    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    /// First visible position among the matches.
    pub(crate) fn first(&self) -> usize {
        self.first
    }

    /// Rows the view fits.
    pub(crate) fn page(&self) -> usize {
        self.page
    }

    /// The view reports how many rows fit.
    pub(crate) fn set_page(&mut self, rows: usize) {
        self.page = rows.max(1);
        self.clamp_scroll();
        self.reveal_selection();
    }

    /// The match at list position `position`.
    pub(crate) fn entry<'a>(
        &self,
        catalogue: &'a Catalogue,
        position: usize,
    ) -> Option<&'a MapEntry> {
        self.matches
            .get(position)
            .and_then(|&index| catalogue.maps().get(index))
    }

    /// The highlighted map.
    pub(crate) fn highlighted<'a>(&self, catalogue: &'a Catalogue) -> Option<&'a MapEntry> {
        self.entry(catalogue, self.selected)
    }

    /// Handle one key; `text` is what it typed.
    pub(crate) fn key(
        &mut self,
        key: KeyCode,
        text: Option<&str>,
        catalogue: &Catalogue,
    ) -> PickerResult {
        match key {
            KeyCode::Escape => {
                self.open = false;
                return PickerResult::Close(None);
            }
            KeyCode::Enter | KeyCode::NumpadEnter => return self.pick(catalogue),
            KeyCode::ArrowUp => self.move_by(-1),
            KeyCode::ArrowDown => self.move_by(1),
            KeyCode::PageUp => self.move_by(-(self.page as isize)),
            KeyCode::PageDown => self.move_by(self.page as isize),
            KeyCode::Home => self.move_to(0),
            KeyCode::End => self.move_to(self.matches.len().saturating_sub(1)),
            KeyCode::Backspace => {
                if self.filter.pop().is_some() {
                    self.refilter_keeping(catalogue);
                }
            }
            _ => {
                let typed = text.unwrap_or_default().chars().filter(|c| !c.is_control());
                let before = self.filter.len();
                for c in typed {
                    if self.filter.len() + c.len_utf8() > FILTER_BYTES {
                        break;
                    }
                    self.filter.push(c);
                }
                if self.filter.len() != before {
                    self.refilter_keeping(catalogue);
                }
            }
        }
        PickerResult::None
    }

    /// Pick the highlighted map and close; nothing to pick keeps it open.
    pub(crate) fn pick(&mut self, catalogue: &Catalogue) -> PickerResult {
        match self.highlighted(catalogue) {
            Some(entry) => {
                self.open = false;
                PickerResult::Close(Some(entry.name.clone()))
            }
            None => PickerResult::None,
        }
    }

    /// Highlight list position `position` (the pointer rests on it).
    pub(crate) fn hover(&mut self, position: usize) {
        if position < self.matches.len() {
            self.selected = position;
        }
    }

    /// Scroll the view by wheel `notches` (positive = down the list); the
    /// highlight stays on its map unless it leaves the view.
    pub(crate) fn scroll(&mut self, notches: isize) {
        let first = self.first as isize + notches * WHEEL_ROWS as isize;
        self.first = first.max(0) as usize;
        self.clamp_scroll();
        let last = self.first + self.page - 1;
        self.selected = self
            .selected
            .clamp(self.first, last.min(self.matches.len().saturating_sub(1)));
    }

    fn move_by(&mut self, step: isize) {
        let last = self.matches.len().saturating_sub(1) as isize;
        self.move_to((self.selected as isize + step).clamp(0, last.max(0)) as usize);
    }

    fn move_to(&mut self, position: usize) {
        self.selected = position.min(self.matches.len().saturating_sub(1));
        self.reveal_selection();
    }

    fn reveal_selection(&mut self) {
        if self.selected < self.first {
            self.first = self.selected;
        } else if self.selected >= self.first + self.page {
            self.first = self.selected + 1 - self.page;
        }
        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        self.first = self.first.min(self.matches.len().saturating_sub(self.page));
    }

    /// Refilter, keeping the highlighted map when it still matches.
    fn refilter_keeping(&mut self, catalogue: &Catalogue) {
        let keep = self.highlighted(catalogue).map(|entry| entry.name.clone());
        self.refilter(catalogue, keep.as_deref());
        self.reveal_selection();
    }

    /// Rebuild the matches; highlight `keep` if listed, else the first.
    fn refilter(&mut self, catalogue: &Catalogue, keep: Option<&str>) {
        self.matches.clear();
        let mode = self.mode;
        let filter = &self.filter;
        self.matches.extend(
            catalogue
                .maps()
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.supports(mode) && matches_filter(entry, filter))
                .map(|(index, _)| index),
        );
        let maps = catalogue.maps();
        self.selected = keep
            .and_then(|name| {
                self.matches
                    .iter()
                    .position(|&index| maps[index].name.eq_ignore_ascii_case(name))
            })
            .unwrap_or(0);
        self.first = 0;
    }
}

/// Whether `entry`'s long name (without `^N` colour codes) or `mp/…` name
/// contains `filter`, ignoring ASCII case.
pub(crate) fn matches_filter(entry: &MapEntry, filter: &str) -> bool {
    filter.is_empty()
        || contains_ignore_case(&strip_colours(&entry.title), filter)
        || contains_ignore_case(&entry.name, filter)
}

/// `text` without Quake colour codes (`^` and the character after it).
fn strip_colours(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '^' {
            chars.next();
        } else {
            plain.push(c);
        }
    }
    plain
}

fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    let (haystack, needle) = (haystack.as_bytes(), needle.as_bytes());
    needle.len() <= haystack.len()
        && haystack
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
}
