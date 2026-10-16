//! Server-browser model: the discovered rows, the filtered/sorted view over
//! them, and the selection plus scroll window the screen navigates.

mod details;
mod discover;

mod fetch;
pub(crate) mod filters;

#[cfg(test)]
pub(crate) use details::DetailPlayer;
#[cfg(test)]
pub(crate) use details::DetailsView;
pub(crate) use details::{DetailsState, Fact};
pub(crate) use discover::gametype_name;

use sjk_client::CompatProfile;
use std::collections::BTreeSet;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

const MASTER_TIMEOUT: Duration = Duration::from_secs(2);
/// How long the whole batch of `getinfo` queries waits for stragglers.
const INFO_TIMEOUT: Duration = Duration::from_millis(900);
/// How old a fetched list may be before opening the browser fetches again.
const FRESH_FOR: Duration = Duration::from_secs(60);
/// The most servers the browser asks and lists: one per row token
/// (`menu::browser_table::ROW_TOKENS`). Stock keeps 2,048 (`MAX_GLOBAL_SERVERS`); 256
/// cut off the end of a master's list, whose servers never showed.
const MAX_BROWSER_SERVERS: usize = crate::menu::browser_table::ROW_TOKENS as usize;

/// One fully queried row, including a preformatted allocation-free render line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ServerEntry {
    pub(crate) address: SocketAddr,
    pub(crate) name: String,
    pub(crate) map: String,
    pub(crate) players: u16,
    pub(crate) capacity: u16,
    pub(crate) ping_millis: u32,
    pub(crate) gametype: String,
    pub(crate) profile: CompatProfile,
    pub(crate) password: bool,
    pub(crate) display: String,
    /// Numeric mode retained for exact filtering rather than comparing display labels.
    pub(crate) mode: Option<i32>,
    /// Reported bot count, used by stock's empty-server filter.
    pub(crate) bots: i32,
    /// Stock printable-info and nonblank-hostname validation result.
    pub(crate) valid_info: bool,
}

/// Stable sort columns exposed by the retained browser header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SortColumn {
    Name,
    Map,
    Players,
    Ping,
    Gametype,
}

impl SortColumn {
    /// Direction the first click on a header sorts by: fullest servers and
    /// lowest pings first, text columns alphabetical.
    fn natural_descending(self) -> bool {
        matches!(self, SortColumn::Players)
    }
}

/// Result of polling a background refresh operation.
pub(crate) enum RefreshPoll {
    Idle,
    Pending,
    Complete,
    Failed(String),
}

/// Browser data and its at-most-one background master query.
pub(crate) struct ServerBrowser {
    master: String,
    entries: Vec<ServerEntry>,
    selected: usize,
    /// First row of the visible window; moves independently of the selection.
    scroll: usize,
    /// Rows the last laid-out table showed, so wheel / page / drag input
    /// knows the window size without re-measuring the screen.
    page: usize,
    /// When the current rows arrived, if a fetch has completed.
    fetched_at: Option<Instant>,
    /// The rows on screen are from before the fetch in flight: they stay up
    /// until its first row lands (or it ends with none).
    replace_on_first_row: bool,
    refresh_started: Instant,
    refresh: Option<Receiver<fetch::Fetched>>,
    visible: Vec<usize>,
    filter: String,
    sort: SortColumn,
    descending: bool,
    favorites: BTreeSet<SocketAddr>,
    favorites_path: Option<PathBuf>,
    /// The FAVOURITES tab: only starred servers are visible.
    favorites_only: bool,
    details: details::ServerDetails,
    filters: filters::Filters,
}

impl ServerBrowser {
    pub(crate) fn new(master: String) -> Self {
        let favorites_path = crate::platform::user_config_file()
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.join("favorites.json")));
        let favorites = favorites_path
            .as_deref()
            .map(load_favorites)
            .unwrap_or_default();
        Self {
            master,
            entries: Vec::new(),
            selected: 0,
            scroll: 0,
            page: 1,
            fetched_at: None,
            replace_on_first_row: false,
            refresh_started: Instant::now(),
            refresh: None,
            visible: Vec::with_capacity(MAX_BROWSER_SERVERS),
            filter: String::with_capacity(64),
            sort: SortColumn::Ping,
            descending: false,
            favorites,
            favorites_path,
            favorites_only: false,
            details: details::ServerDetails::new(),
            filters: filters::Filters::default(),
        }
    }

    /// Drive the selected row's status query; call once per frame while the
    /// browser is open.
    pub(crate) fn tick_details(&mut self) {
        let selected = self.visible_entry(self.selected).map(|entry| entry.address);
        self.details.tick(selected);
    }

    /// What the details panel shows for the selected row.
    pub(crate) fn details(&self) -> DetailsState<'_> {
        self.details.state()
    }

    /// The first of the selected server's players the details panel shows.
    pub(crate) fn player_scroll(&self) -> usize {
        self.details.player_scroll()
    }

    /// Wheel the selected server's players `by` rows, `page` in view.
    pub(crate) fn scroll_players(&mut self, by: i32, page: usize) {
        self.details.scroll_players(by, page);
    }

    pub(crate) fn entries(&self) -> &[ServerEntry] {
        &self.entries
    }

    pub(crate) fn visible_len(&self) -> usize {
        self.visible.len()
    }
    pub(crate) fn visible_entry(&self, row: usize) -> Option<&ServerEntry> {
        self.visible
            .get(row)
            .and_then(|index| self.entries.get(*index))
    }
    /// The typed filter text alone.
    pub(crate) fn filter_text(&self) -> &str {
        &self.filter
    }
    /// Presentation-only sort indicator for the retained table header.
    pub(crate) fn sort_state(&self) -> (SortColumn, bool) {
        (self.sort, self.descending)
    }
    /// Whether the persisted favourites set contains this row.
    pub(crate) fn is_favorite(&self, address: SocketAddr) -> bool {
        self.favorites.contains(&address)
    }
    /// Whether only starred servers are listed (the FAVOURITES tab).
    pub(crate) fn favorites_only(&self) -> bool {
        self.favorites_only
    }
    /// Switch between every server and the starred ones; the selection
    /// stays on its row where it survives the switch and clamps otherwise.
    pub(crate) fn set_favorites_only(&mut self, favorites_only: bool) {
        if self.favorites_only == favorites_only {
            return;
        }
        self.favorites_only = favorites_only;
        let selected = self.visible_entry(self.selected).map(|entry| entry.address);
        self.rebuild_visible();
        self.selected = selected
            .and_then(|address| self.row_of(address))
            .unwrap_or(0)
            .min(self.visible.len().saturating_sub(1));
        self.reveal_selection();
    }

    /// Type `text` into the search (the tests and world shots; the field itself goes through
    /// [`Self::edit_filter`]).
    #[cfg(test)]
    pub(crate) fn push_filter(&mut self, text: &str) {
        self.filter.push_str(text);
        self.rebuild_visible();
    }
    /// A key in the search field: pasted, copied, deleted back or typed ([`crate::text_entry`]).
    /// Whether the search text changed or the key was the field's own (copy), so the menu
    /// does not read it again as a command.
    pub(crate) fn edit_filter(
        &mut self,
        key: winit::keyboard::KeyCode,
        text: Option<&str>,
        shift: bool,
        control: bool,
    ) -> bool {
        let copy = crate::text_entry::is_copy(key, text, control);
        let changed =
            crate::text_entry::edit(&mut self.filter, key, text, shift, control, 64, true);
        if changed {
            self.rebuild_visible();
        }
        changed || copy
    }
    pub(crate) fn pop_filter(&mut self) {
        self.filter.pop();
        self.rebuild_visible();
    }
    /// Forget the typed filter: every server shows again.
    pub(crate) fn clear_filter(&mut self) {
        if self.filter.is_empty() {
            return;
        }
        self.filter.clear();
        self.rebuild_visible();
    }

    /// How many of the listed servers are favourites.
    pub(crate) fn favorites_listed(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| self.favorites.contains(&entry.address))
            .count()
    }

    /// Sort by `column`: the first click uses the column's natural direction,
    /// clicking the active column again flips it. The selected server stays
    /// selected and is scrolled into view.
    pub(crate) fn sort_by(&mut self, column: SortColumn) {
        if self.sort == column {
            self.descending = !self.descending;
        } else {
            self.sort = column;
            self.descending = column.natural_descending();
        }
        let selected = self.visible_entry(self.selected).map(|entry| entry.address);
        self.rebuild_visible();
        if let Some(row) = selected.and_then(|address| self.row_of(address)) {
            self.selected = row;
        }
        self.reveal_selection();
    }

    fn row_of(&self, address: SocketAddr) -> Option<usize> {
        self.visible
            .iter()
            .position(|index| self.entries[*index].address == address)
    }

    pub(crate) fn toggle_selected_favorite(&mut self) {
        let Some(address) = self.visible_entry(self.selected).map(|entry| entry.address) else {
            return;
        };
        if !self.favorites.remove(&address) {
            self.favorites.insert(address);
        }
        if let Some(path) = &self.favorites_path {
            let _ = save_favorites(path, &self.favorites);
        }
        self.rebuild_visible();
    }

    /// Add a console-selected address to the same persistent favorites set as the UI.
    pub(crate) fn add_favorite(&mut self, address: SocketAddr) -> Result<(), String> {
        let mut favorites = self.favorites.clone();
        if !favorites.insert(address) {
            return Err("Favorite already exists".into());
        }
        let path = self
            .favorites_path
            .as_ref()
            .ok_or("Favorites storage unavailable")?;
        save_favorites(path, &favorites).map_err(|error| error.to_string())?;
        self.favorites = favorites;
        self.rebuild_visible();
        Ok(())
    }

    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    pub(crate) fn set_selection(&mut self, selected: usize) {
        if selected < self.visible.len() {
            self.selected = selected;
        }
    }

    /// First visible row of the table window.
    pub(crate) fn scroll(&self) -> usize {
        self.scroll
    }

    /// Rows the table window shows.
    pub(crate) fn page(&self) -> usize {
        self.page
    }

    /// Record how many rows the table shows; called by the layout each frame
    /// so navigation can page and clamp correctly.
    pub(crate) fn set_page(&mut self, rows: usize) {
        self.page = rows.max(1);
        self.scroll = self.scroll.min(self.max_scroll());
    }

    fn max_scroll(&self) -> usize {
        self.visible.len().saturating_sub(self.page)
    }

    /// Scroll the window by `rows` (negative = up) without moving the selection.
    pub(crate) fn scroll_by(&mut self, rows: i32) {
        self.scroll_to(self.scroll.saturating_add_signed(rows as isize));
    }

    /// Scroll so `first` is the top visible row, clamped to the list.
    pub(crate) fn scroll_to(&mut self, first: usize) {
        self.scroll = first.min(self.max_scroll());
    }

    /// Move the selection by whole pages, keeping it in view.
    pub(crate) fn page_selection(&mut self, pages: i32) {
        if self.visible.is_empty() {
            return;
        }
        let step = (self.page as i32 * pages) as isize;
        self.selected = self
            .selected
            .saturating_add_signed(step)
            .min(self.visible.len() - 1);
        self.reveal_selection();
    }

    /// Jump the selection to the first (`false`) or last (`true`) row.
    pub(crate) fn select_end(&mut self, last: bool) {
        self.selected = if last {
            self.visible.len().saturating_sub(1)
        } else {
            0
        };
        self.reveal_selection();
    }

    /// Scroll just far enough that the selected row is inside the window.
    pub(crate) fn reveal_selection(&mut self) {
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + self.page {
            self.scroll = self.selected + 1 - self.page;
        }
        self.scroll = self.scroll.min(self.max_scroll());
    }

    pub(crate) fn move_selection(&mut self, delta: i32) {
        if self.visible.is_empty() {
            self.selected = 0;
            return;
        }
        self.selected = if delta < 0 {
            self.selected
                .checked_sub(1)
                .unwrap_or(self.visible.len() - 1)
        } else {
            (self.selected + 1) % self.visible.len()
        };
        self.reveal_selection();
    }

    fn rebuild_visible(&mut self) {
        self.visible.clear();
        let needle = self.filter.to_ascii_lowercase();
        self.visible.extend(
            self.entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| {
                    (!self.favorites_only || self.favorites.contains(&entry.address))
                        && self.filters.accepts(entry)
                        && (needle.is_empty()
                            || crate::text::Plain(&entry.name)
                                .to_string()
                                .to_ascii_lowercase()
                                .contains(&needle)
                            || entry.map.to_ascii_lowercase().contains(&needle))
                })
                .map(|(index, _)| index),
        );
        let entries = &self.entries;
        let favorites = &self.favorites;
        let column = self.sort;
        let descending = self.descending;
        self.visible.sort_by(|left, right| {
            let left = &entries[*left];
            let right = &entries[*right];
            let favorite = favorites
                .contains(&right.address)
                .cmp(&favorites.contains(&left.address));
            favorite.then_with(|| {
                let order = match column {
                    SortColumn::Name => left
                        .name
                        .to_ascii_lowercase()
                        .cmp(&right.name.to_ascii_lowercase()),
                    SortColumn::Map => left.map.cmp(&right.map),
                    SortColumn::Players => left.players.cmp(&right.players),
                    SortColumn::Ping => left.ping_millis.cmp(&right.ping_millis),
                    SortColumn::Gametype => left.gametype.cmp(&right.gametype),
                };
                if descending { order.reverse() } else { order }
            })
        });
        self.selected = self.selected.min(self.visible.len().saturating_sub(1));
        self.scroll = self.scroll.min(self.max_scroll());
    }
}

#[cfg(test)]
impl ServerEntry {
    /// A row as a server answering `getinfo` would make it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn for_test(
        address: &str,
        name: &str,
        map: &str,
        players: u16,
        capacity: u16,
        ping_millis: u32,
        mode: i32,
        profile: CompatProfile,
        password: bool,
    ) -> Self {
        Self {
            address: address.parse().expect("a test address"),
            name: name.to_owned(),
            map: map.to_owned(),
            players,
            capacity,
            ping_millis,
            gametype: gametype_name(Some(mode)).to_owned(),
            profile,
            password,
            display: String::new(),
            mode: Some(mode),
            bots: 0,
            valid_info: true,
        }
    }
}

#[cfg(test)]
impl ServerBrowser {
    /// List `entries` as if the master server had just answered, with
    /// `favourites` starred; favourites are not saved from then on.
    pub(crate) fn list_for_test(&mut self, entries: Vec<ServerEntry>, favourites: &[SocketAddr]) {
        self.favorites_path = None;
        self.favorites = favourites.iter().copied().collect();
        self.entries = entries;
        self.fetched_at = Some(Instant::now());
        self.rebuild_visible();
    }

    /// Keep favourites in `path` from now on, as a profile does.
    pub(crate) fn keep_favourites_for_test(&mut self, path: std::path::PathBuf) {
        self.favorites_path = Some(path);
    }

    /// Show `view` as the answer of the selected server's status query.
    pub(crate) fn answer_for_test(&mut self, view: DetailsView) {
        self.details.answer_for_test(view);
    }
}

fn load_favorites(path: &std::path::Path) -> BTreeSet<SocketAddr> {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
        .into_iter()
        .flatten()
        .filter_map(|value| value.parse().ok())
        .collect()
}

fn save_favorites(path: &std::path::Path, favorites: &BTreeSet<SocketAddr>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let values = favorites
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec_pretty(&values).map_err(std::io::Error::other)?;
    fs::write(path, bytes)
}
