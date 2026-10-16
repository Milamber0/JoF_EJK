//! The SJK chat page's GIF picker (`docs/hub-chat.md`, "The GIF picker"): GIF beside
//! Send opens it over the messages, three tabs of GIFs as a grid of animated tiles,
//! [`COLUMNS`] by [`ROWS`]:
//!
//! - Favourites, kept on this PC ([`chat_gifs::favourites`]);
//! - Search, GIPHY's search for the words typed, or its trending GIFs for none
//!   ([`chat_gifs::search`]), asked once the typing stops, with the player's own GIPHY
//!   key: without one, the key card says how to get it, free, and takes it pasted;
//! - Seen in chat, the GIFs of the chat's messages, newest first (muted senders' left
//!   out), which needs no key and asks GIPHY nothing but the GIFs.
//!
//! Picking a tile writes its link in the message (in place of a GIF link already there)
//! and gives the field the keyboard back: Enter sends it. The star on a tile, or F on
//! it, makes it a favourite or not. The tiles show GIPHY's preview file, smaller than a
//! message's GIF ([`chat_gifs::show_preview`]).

use super::view::{LIST_WIDTH, LIST_X};
use super::*;
use crate::chat_gifs::{self, GifId, Shown, favourites, link, search};
use crate::menu::sjk::{Frame, color, kit, text};
use crate::menu_widgets::TextFamily;
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};

/// The tabs (one token each), the search field, the tiles on show and their stars.
pub(super) const TAB_BASE: u16 = 1_500;
pub(super) const SEARCH_TOKEN: u16 = 1_504;
pub(super) const TILE_BASE: u16 = 1_510;
pub(super) const STAR_BASE: u16 = 1_530;
/// The key card's field, Open GIPHY, Save key and Not now; Get a GIPHY key (or Change
/// GIPHY key) on the Search tab, which opens the card.
pub(super) const KEY_FIELD_TOKEN: u16 = 1_545;
pub(super) const KEY_OPEN_TOKEN: u16 = 1_546;
pub(super) const KEY_SAVE_TOKEN: u16 = 1_547;
pub(super) const KEY_LATER_TOKEN: u16 = 1_548;
pub(super) const KEY_ASK_TOKEN: u16 = 1_549;
/// The grid.
pub(super) const COLUMNS: usize = 5;
pub(super) const ROWS: usize = 3;
pub(super) const TILES: usize = COLUMNS * ROWS;
/// The most GIFs Seen in chat lists.
const SEEN_MOST: usize = 60;

/// Where the picker is (frame pixels): its top row (tabs, search), the grid's top, a
/// tile's height, the gap between tiles, and its line at the bottom.
const PICKER_TOP: f32 = 226.0;
const GRID_TOP: f32 = 280.0;
const TILE_HEIGHT: f32 = 168.0;
const TILE_GAP: f32 = 14.0;
const FOOT_Y: f32 = GRID_TOP + ROWS as f32 * (TILE_HEIGHT + TILE_GAP) + 2.0;
const SEARCH_WIDTH: f32 = 620.0;
/// A tile's star: its side and its inset from the tile's top right corner.
const STAR: f32 = 26.0;
const STAR_INSET: f32 = 8.0;
/// The key card (frame pixels), in the middle of the page.
const CARD_X: f32 = 420.0;
const CARD_Y: f32 = 220.0;
const CARD_WIDTH: f32 = 1_080.0;
const CARD_HEIGHT: f32 = 600.0;
const CARD_PAD: f32 = 56.0;

/// The key card's steps: what to do on GIPHY's site, one line each.
const KEY_STEPS: [&str; 3] = [
    "Open GIPHY's developer site, then log in or sign up: it's free.",
    "Press Create an API Key, choose API, and give it any name, such as SJK chat.",
    "Copy the key GIPHY shows, paste it below with Ctrl+V, and press Save key.",
];

/// The picker's tabs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Tab {
    Favourites,
    Search,
    Seen,
}

impl Tab {
    pub(super) const ALL: [Self; 3] = [Self::Favourites, Self::Search, Self::Seen];

    fn label(self) -> &'static str {
        match self {
            Self::Favourites => "Favourites",
            Self::Search => "Search",
            Self::Seen => "Seen in chat",
        }
    }

    pub(super) fn index(self) -> usize {
        Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0)
    }
}

/// What the tiles were made from, so they are made again only when it changes.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Made {
    tab: Tab,
    favourites: u64,
    search: u64,
    words: String,
    pages: usize,
    chat: u64,
    muted: usize,
    key: bool,
}

/// The key card on show: what was typed or pasted into its field, and what it says of
/// it.
pub(super) struct KeyCard {
    pub(super) typed: String,
    pub(super) notice: Option<&'static str>,
}

/// The picker's state, kept by the page.
pub(super) struct Picker {
    pub(super) open: bool,
    pub(super) tab: Tab,
    /// The search field's words, and when they last changed while not searched yet.
    pub(super) typed: String,
    typed_at: Option<Instant>,
    /// The words searched ([`search::tidy`]).
    pub(super) searched: String,
    /// Pages of the search asked for: more as the grid scrolls to its end.
    pages: usize,
    /// The first row on show.
    pub(super) row: usize,
    pub(super) searcher: search::Searcher,
    /// The tab's GIFs, and what they were made from.
    pub(super) ids: Vec<GifId>,
    made: Option<Made>,
    /// The search's state, for the line under the grid.
    results: search::Results,
    /// What the last pick could not do, until the next.
    pub(super) notice: Option<&'static str>,
    /// The key card, while it is on show.
    pub(super) key_card: Option<KeyCard>,
    /// Not now was chosen: the card opens again only from Get a GIPHY key.
    key_card_put_off: bool,
}

impl Picker {
    pub(super) fn new() -> Self {
        Self {
            open: false,
            tab: Tab::Search,
            typed: String::new(),
            typed_at: None,
            searched: String::new(),
            pages: 1,
            row: 0,
            searcher: search::Searcher::default(),
            ids: Vec::with_capacity(search::MOST),
            made: None,
            results: search::Results::default(),
            notice: None,
            key_card: None,
            key_card_put_off: false,
        }
    }

    /// Open on Favourites when there are some, else Search; returns the control the
    /// keyboard goes to.
    pub(super) fn open(&mut self) -> u16 {
        self.open = true;
        self.notice = None;
        self.row = 0;
        self.tab = if favourites::list().is_empty() {
            Tab::Search
        } else {
            Tab::Favourites
        };
        self.made = None;
        self.first_control()
    }

    pub(super) fn close(&mut self) {
        self.open = false;
        self.key_card = None;
    }

    /// Show the key card when Search is on show without a key and the player has not
    /// put it off; whether it opened.
    pub(super) fn key_card_due(&mut self, has_key: bool) -> bool {
        let due = self.open
            && self.tab == Tab::Search
            && !has_key
            && !self.key_card_put_off
            && self.key_card.is_none();
        if due {
            self.open_key_card("");
        }
        due
    }

    /// Show the key card, its field holding `current` (the key saved, if any).
    pub(super) fn open_key_card(&mut self, current: &str) {
        self.key_card = Some(KeyCard {
            typed: current.trim().to_owned(),
            notice: None,
        });
    }

    /// Not now: the card goes, until Get a GIPHY key opens it again.
    pub(super) fn put_off_key_card(&mut self) {
        self.key_card = None;
        self.key_card_put_off = true;
    }

    /// Save key: the key typed, trimmed, and the card goes; `None` (the card saying
    /// why) when it does not look like a GIPHY key.
    pub(super) fn save_key(&mut self) -> Option<String> {
        let card = self.key_card.as_mut()?;
        let typed = card.typed.trim();
        if typed.is_empty() {
            card.notice = Some("Paste your key first: copy it on GIPHY's page, then Ctrl+V here.");
            return None;
        }
        if !search::looks_like_key(typed) {
            card.notice = Some(
                "That doesn't look like a GIPHY key: copy the whole key, letters and digits only.",
            );
            return None;
        }
        let key = typed.to_owned();
        self.key_card = None;
        self.made = None;
        Some(key)
    }

    /// A key typed, pasted, copied or taken back in the key card's field.
    pub(super) fn edit_key(
        &mut self,
        key: winit::keyboard::KeyCode,
        text: Option<&str>,
        shift: bool,
        control: bool,
    ) {
        if let Some(card) = self.key_card.as_mut()
            && crate::text_entry::edit(
                &mut card.typed,
                key,
                text,
                shift,
                control,
                search::KEY_MAX,
                true,
            )
        {
            card.notice = None;
        }
    }

    /// The control the keyboard goes to on this tab: the search field, else the first
    /// tile, else the tabs.
    pub(super) fn first_control(&self) -> u16 {
        match self.tab {
            Tab::Search => SEARCH_TOKEN,
            _ if self.ids.is_empty() && self.made.is_some() => TAB_BASE + self.tab.index() as u16,
            _ => TILE_BASE,
        }
    }

    pub(super) fn choose_tab(&mut self, tab: Tab) {
        if self.tab != tab {
            self.tab = tab;
            self.row = 0;
            self.made = None;
        }
    }

    /// A key typed into the search field.
    pub(super) fn type_key(
        &mut self,
        key: winit::keyboard::KeyCode,
        text: Option<&str>,
        shift: bool,
        control: bool,
    ) {
        let before = self.typed.len();
        crate::sjk_chat_field::key_into(&mut self.typed, key, text, shift, control);
        if self.typed.chars().count() > search::QUERY_MAX {
            self.typed = self.typed.chars().take(search::QUERY_MAX).collect();
        }
        if self.typed.len() != before {
            self.typed_at = Some(Instant::now());
        }
    }

    /// Ctrl+A and what a selection does to the key about to edit the search field
    /// ([`crate::text_entry::before_edit`]); whether the key is spent.
    pub(super) fn before_edit(
        &mut self,
        key: winit::keyboard::KeyCode,
        text: Option<&str>,
        shift: bool,
        control: bool,
    ) -> bool {
        let before = self.typed.len();
        let spent = crate::text_entry::before_edit(&mut self.typed, key, text, shift, control);
        if self.typed.len() != before {
            self.typed_at = Some(Instant::now());
        }
        spent
    }

    pub(super) fn backspace(&mut self) {
        if self.typed.pop().is_some() {
            self.typed_at = Some(Instant::now());
        }
    }

    /// Search the words typed now, not waiting for the typing to stop.
    pub(super) fn search_now(&mut self) {
        self.typed_at = None;
        let words = search::tidy(&self.typed);
        if words != self.searched {
            self.searched = words;
            self.pages = 1;
            self.row = 0;
        }
    }

    /// Once a frame: search the words once the typing stopped ([`search::DEBOUNCE`]),
    /// take GIPHY's answers and make the tiles again when what they show changed.
    fn refresh(&mut self, inputs: &Inputs<'_>, key: Option<&str>) {
        if self
            .typed_at
            .is_some_and(|at| at.elapsed() >= search::DEBOUNCE)
        {
            self.search_now();
        }
        self.searcher.poll();
        let made = Made {
            tab: self.tab,
            favourites: favourites::revision(),
            search: self.searcher.revision(),
            words: self.searched.clone(),
            pages: self.pages,
            chat: inputs.chat.map_or(0, |chat| chat.revision),
            muted: inputs.muted.len(),
            key: key.is_some(),
        };
        if self.made.as_ref() == Some(&made) {
            return;
        }
        self.made = Some(made);
        self.ids.clear();
        self.results = search::Results::default();
        match self.tab {
            Tab::Favourites => self.ids.extend(favourites::list()),
            Tab::Search => {
                self.results = self.searcher.results(key, &self.searched, self.pages);
                self.ids.extend(self.results.ids.iter().cloned());
            }
            Tab::Seen => self.ids.extend(seen(inputs)),
        }
        self.row = self.row.min(self.last_row());
    }

    /// The last first row: the grid's end on show.
    fn last_row(&self) -> usize {
        self.ids.len().div_ceil(COLUMNS).saturating_sub(ROWS)
    }

    /// Scroll by `rows` (down when positive), asking for the search's next page when
    /// its end comes on show.
    pub(super) fn scroll(&mut self, rows: isize) {
        self.row = self.row.saturating_add_signed(rows).min(self.last_row());
        self.more();
    }

    /// The search's next page, when the grid's last row is on show and there is more.
    fn more(&mut self) {
        let end_on_show = (self.row + ROWS) * COLUMNS >= self.ids.len();
        let more = self
            .results
            .total
            .is_some_and(|total| self.ids.len() < total)
            && self.pages * search::PAGE_SIZE < search::MOST
            && self.ids.len() >= self.pages * search::PAGE_SIZE;
        if self.tab == Tab::Search && end_on_show && more && !self.results.loading {
            self.pages += 1;
        }
    }

    /// The GIF of the tile with `token`, if one is on show there.
    pub(super) fn tile(&self, token: u16) -> Option<&GifId> {
        let index = usize::from(token.checked_sub(TILE_BASE)?);
        (index < TILES)
            .then(|| self.ids.get(self.row * COLUMNS + index))
            .flatten()
    }

    /// The tile the keyboard goes to from tile `token` moved by `dx` columns and `dy`
    /// rows, scrolling the grid; `None` past its top (the keyboard leaves it).
    pub(super) fn step(&mut self, token: u16, dx: isize, dy: isize) -> Option<u16> {
        let at = self.row * COLUMNS + usize::from(token - TILE_BASE);
        let to = at as isize + dx + dy * COLUMNS as isize;
        if to < 0 {
            return None;
        }
        let to = (to as usize).min(self.ids.len().saturating_sub(1));
        let row = to / COLUMNS;
        if row < self.row {
            self.row = row;
        } else if row >= self.row + ROWS {
            self.row = row + 1 - ROWS;
        }
        self.more();
        Some(TILE_BASE + (to - self.row * COLUMNS) as u16)
    }
}

/// The GIFs of the chat's messages, newest first, each once; muted senders' left out,
/// as their GIFs are never fetched.
fn seen(inputs: &Inputs<'_>) -> Vec<GifId> {
    let mut ids: Vec<GifId> = Vec::new();
    let Some(chat) = inputs.chat else {
        return ids;
    };
    for message in chat.messages.iter().rev() {
        if ids.len() == SEEN_MOST {
            break;
        }
        if inputs.muted.contains(&message.key_id) {
            continue;
        }
        if let Some((id, _)) = link::find(&message.text)
            && !ids.contains(&id)
        {
            ids.push(id);
        }
    }
    ids
}

/// `draft` with `id`'s link in place of the GIF links it has (or after its text), or
/// `None` when the link does not fit the message's length.
pub(super) fn with_gif(draft: &str, id: &GifId) -> Option<String> {
    let mut words: Vec<&str> = draft
        .split(' ')
        .filter(|word| !word.is_empty() && link::parse(word).is_none())
        .collect();
    let url = link::page_url(id);
    words.push(&url);
    let text = words.join(" ");
    (text.chars().count() <= DRAFT_MAX).then_some(text)
}

impl Panel {
    /// The picker over the messages' place: the tabs, the search field on Search, the
    /// tiles on show, and the line under them.
    pub(super) fn gif_picker(&mut self, frame: &Frame, inputs: &Inputs<'_>) {
        let s = frame.s;
        let key = search::key(inputs.giphy_key);
        self.gifs.refresh(inputs, key);
        let tab = self.gifs.tab;
        let labels = Tab::ALL.map(Tab::label);
        let on_tabs = (TAB_BASE..TAB_BASE + Tab::ALL.len() as u16).contains(&self.focus);
        kit::segments(
            &mut self.ui,
            frame,
            LIST_X + LIST_WIDTH,
            PICKER_TOP + 20.0,
            &labels,
            tab.index(),
            on_tabs,
            TAB_BASE,
        );
        self.order.push(TAB_BASE + tab.index() as u16);
        if tab == Tab::Search {
            let searching = self.focus == SEARCH_TOKEN;
            let found = (!self.gifs.searched.is_empty())
                .then_some(self.gifs.results.total)
                .flatten();
            kit::search(
                &mut self.ui,
                frame,
                [LIST_X, PICKER_TOP, SEARCH_WIDTH, 40.0],
                &self.gifs.typed,
                searching,
                "Search GIPHY (nothing typed: trending)",
                found,
                SEARCH_TOKEN,
            );
            self.order.push(SEARCH_TOKEN);
        } else {
            let line = match tab {
                Tab::Favourites => {
                    "Your favourite GIFs, kept on this PC. F or the star removes one."
                }
                _ => "GIFs posted in SJK chat lately, newest first. F or the star keeps one.",
            };
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{line}"),
                frame.rect(LIST_X, PICKER_TOP + 8.0, SEARCH_WIDTH + 40.0, 24.0),
                16.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        let width = (LIST_WIDTH - (COLUMNS - 1) as f32 * TILE_GAP) / COLUMNS as f32;
        for index in 0..TILES {
            let Some(id) = self.gifs.ids.get(self.gifs.row * COLUMNS + index).cloned() else {
                break;
            };
            let column = (index % COLUMNS) as f32;
            let row = (index / COLUMNS) as f32;
            let rect = [
                LIST_X + column * (width + TILE_GAP),
                GRID_TOP + row * (TILE_HEIGHT + TILE_GAP),
                width,
                TILE_HEIGHT,
            ];
            self.gif_tile(frame, &id, rect, index as u16);
        }
        self.picker_empty(frame);
        self.picker_foot(frame);
    }

    /// One tile: the GIF fitted in it, a band when the keyboard or the pointer is on
    /// it, and its star.
    fn gif_tile(&mut self, frame: &Frame, id: &GifId, rect: [f32; 4], index: u16) {
        let s = frame.s;
        let token = TILE_BASE + index;
        let star_token = STAR_BASE + index;
        let [x, y, width, height] = rect;
        let area = frame.rect(x, y, width, height);
        let lit = self.focus == token
            || self.ui.token_hovered(token)
            || self.ui.token_hovered(star_token);
        let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: area,
            radius: 8.0 * s,
            color: color::alpha(color::HOLO, if lit { 0.16 } else { 0.05 }),
        });
        let inner = Rect::new(
            area.x + 4.0 * s,
            area.y + 4.0 * s,
            area.width - 8.0 * s,
            area.height - 8.0 * s,
        );
        match chat_gifs::show_preview(id) {
            Shown::Ready { texture, size } => {
                let [w, h] = contain(size, [inner.width, inner.height]);
                let _ = self.ui.draw_list_mut().push(DrawCommand::TexturedQuad {
                    rect: Rect::new(
                        inner.x + (inner.width - w) * 0.5,
                        inner.y + (inner.height - h) * 0.5,
                        w,
                        h,
                    ),
                    texture,
                    color: Color::new(1.0, 1.0, 1.0, 1.0),
                });
            }
            shown => text(
                &mut self.ui,
                TextFamily::Body,
                format_args!(
                    "{}",
                    if shown == Shown::Loading {
                        "Loading GIF"
                    } else {
                        "GIF unavailable"
                    }
                ),
                inner,
                14.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Center,
            ),
        }
        if self.focus == token {
            // The keyboard's tile: a gold edge, as the kit's focused controls have.
            let edge = 2.0 * s;
            for bar in [
                Rect::new(area.x, area.y, area.width, edge),
                Rect::new(area.x, area.y + area.height - edge, area.width, edge),
                Rect::new(area.x, area.y, edge, area.height),
                Rect::new(area.x + area.width - edge, area.y, edge, area.height),
            ] {
                let _ = self.ui.draw_list_mut().push(DrawCommand::SolidRect {
                    rect: bar,
                    color: color::GOLD,
                });
            }
        }
        self.ui.hit_region(token, area);
        self.order.push(token);
        let favourite = favourites::contains(id);
        let star = frame.rect(x + width - STAR - STAR_INSET, y + STAR_INSET, STAR, STAR);
        if favourite || lit {
            let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: star,
                radius: star.width * 0.5,
                color: color::alpha(color::SPACE, 0.7),
            });
            let mark = Rect::new(
                star.x + star.width * 0.18,
                star.y + star.height * 0.16,
                star.width * 0.64,
                star.height * 0.64,
            );
            let colour = match (favourite, self.ui.token_hovered(star_token)) {
                (true, _) => color::GOLD_BRIGHT,
                (false, true) => color::TEXT,
                (false, false) => color::alpha(color::TEXT, 0.55),
            };
            crate::chat::star(&mut self.ui, mark, colour);
        }
        // Over the tile's target, so a click on the star is the star's.
        self.ui.hit_region(star_token, star);
    }

    /// What the grid says when it has no tile: why, or what to do.
    fn picker_empty(&mut self, frame: &Frame) {
        if !self.gifs.ids.is_empty() {
            return;
        }
        // Without a key, or with one GIPHY refused: the way to the key card.
        let ask = match self.gifs.results.failure {
            Some(search::Failure::NoKey) if self.gifs.tab == Tab::Search => Some("Get a GIPHY key"),
            Some(search::Failure::BadKey) if self.gifs.tab == Tab::Search => {
                Some("Change GIPHY key")
            }
            _ => None,
        };
        if let Some(label) = ask {
            kit::button(
                &mut self.ui,
                frame,
                [LIST_X, GRID_TOP + 64.0, 280.0, 46.0],
                label,
                true,
                true,
                self.focus == KEY_ASK_TOKEN,
                KEY_ASK_TOKEN,
            );
            self.order.push(KEY_ASK_TOKEN);
        }
        let picker = &self.gifs;
        let line = match picker.tab {
            Tab::Favourites => {
                "No favourite GIFs yet: star one in Search or Seen in chat, or choose a message with a GIF and Add to favourites.".to_owned()
            }
            Tab::Seen => "No GIFs in SJK chat yet.".to_owned(),
            Tab::Search => match &picker.results.failure {
                Some(failure) => failure.message(),
                None if picker.results.loading => "Asking GIPHY...".to_owned(),
                None if picker.searched.is_empty() => "Nothing trending on GIPHY.".to_owned(),
                None => format!("GIPHY found nothing for \"{}\".", picker.searched),
            },
        };
        let colour = if picker.results.failure.is_some() {
            color::EMBER
        } else {
            color::QUIET
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{line}"),
            frame.rect(LIST_X, GRID_TOP + 20.0, LIST_WIDTH, 24.0),
            17.0 * frame.s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    /// The line under the grid: which tiles are on show, or what the last pick could
    /// not do, and GIPHY's mark.
    fn picker_foot(&mut self, frame: &Frame) {
        let s = frame.s;
        let picker = &self.gifs;
        let count = picker.ids.len();
        let (line, colour) = if let Some(notice) = picker.notice {
            (notice.to_owned(), color::EMBER)
        } else if let Some(failure) = picker.results.failure.as_ref().filter(|_| count > 0) {
            (failure.message(), color::EMBER)
        } else if count > TILES {
            let first = picker.row * COLUMNS + 1;
            let last = ((picker.row + ROWS) * COLUMNS).min(count);
            let more = if picker.results.loading {
                ", more coming"
            } else {
                ""
            };
            (
                format!("{first} to {last} of {count}{more}: Page Up and Page Down scroll"),
                color::QUIET,
            )
        } else {
            (String::new(), color::QUIET)
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{line}"),
            frame.rect(LIST_X, FOOT_Y, LIST_WIDTH - 220.0, 20.0),
            14.0 * s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
        // GIPHY's terms ask for its mark wherever its API is used.
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Powered by GIPHY"),
            frame.rect(LIST_X + LIST_WIDTH - 220.0, FOOT_Y, 220.0, 20.0),
            14.0 * s,
            color::MUTED,
            FontWeight::Semibold,
            TextAlign::End,
        );
    }
}

impl Panel {
    /// The key card, in the page's place while it is on show: why search needs the
    /// player's own key, the steps on GIPHY's site with a button to it, the field the key
    /// is pasted into, Save key and Not now.
    pub(super) fn key_card(&mut self, frame: &Frame) {
        let s = frame.s;
        let Some(card) = self.gifs.key_card.as_ref() else {
            return;
        };
        let typed = card.typed.clone();
        let notice = card.notice;
        kit::card(
            &mut self.ui,
            frame,
            [CARD_X, CARD_Y, CARD_WIDTH, CARD_HEIGHT],
        );
        let x = CARD_X + CARD_PAD;
        let width = CARD_WIDTH - 2.0 * CARD_PAD;
        let mut y = CARD_Y + 44.0;
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("Search GIFs with your own GIPHY key"),
            frame.rect(x, y, width, 40.0),
            32.0 * s,
            color::TEXT,
            FontWeight::Regular,
            TextAlign::Start,
        );
        y += 52.0;
        for line in [
            "GIPHY's search needs a key, and each player uses their own. It's free and takes",
            "about two minutes. Favourites and Seen in chat work without one.",
        ] {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{line}"),
                frame.rect(x, y, width, 24.0),
                18.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
            y += 26.0;
        }
        y += 24.0;
        for (step, line) in KEY_STEPS.iter().enumerate() {
            let mark = frame.rect(x, y, 34.0, 34.0);
            let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: mark,
                radius: mark.width * 0.5,
                color: color::alpha(color::GOLD, 0.18),
            });
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}", step + 1),
                frame.rect(x, y + 4.0, 34.0, 26.0),
                20.0 * s,
                color::GOLD_BRIGHT,
                FontWeight::Regular,
                TextAlign::Center,
            );
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{line}"),
                frame.rect(x + 52.0, y + 5.0, width - 52.0, 24.0),
                18.0 * s,
                color::TEXT,
                FontWeight::Regular,
                TextAlign::Start,
            );
            y += 46.0;
            if step == 0 {
                kit::button(
                    &mut self.ui,
                    frame,
                    [x + 52.0, y - 4.0, 380.0, 44.0],
                    "Open developers.giphy.com",
                    false,
                    true,
                    self.focus == KEY_OPEN_TOKEN,
                    KEY_OPEN_TOKEN,
                );
                self.order.push(KEY_OPEN_TOKEN);
                y += 58.0;
            }
        }
        y += 12.0;
        let focused = self.focus == KEY_FIELD_TOKEN;
        let caret = focused && (self.epoch.elapsed().as_millis() / 500).is_multiple_of(2);
        let shown = if typed.is_empty() && !focused {
            "Paste your GIPHY key here"
        } else {
            typed.as_str()
        };
        let field = [x, y, width, 48.0];
        kit::field(
            &mut self.ui,
            frame,
            field,
            format_args!("{shown}{}", if caret { "|" } else { "" }),
            focused,
            false,
        );
        self.ui.hit_region(
            KEY_FIELD_TOKEN,
            frame.rect(field[0], field[1], field[2], field[3]),
        );
        self.order.push(KEY_FIELD_TOKEN);
        y += 60.0;
        let (line, colour) = match notice {
            Some(notice) => (notice, color::EMBER),
            None => (
                "Kept on this PC only, in Settings > Network > GIPHY key. It allows 100 searches an hour.",
                color::QUIET,
            ),
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{line}"),
            frame.rect(x, y, width, 22.0),
            15.0 * s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
        let buttons_y = CARD_Y + CARD_HEIGHT - 44.0 - 40.0;
        kit::button(
            &mut self.ui,
            frame,
            [x + width - 200.0, buttons_y, 200.0, 44.0],
            "Save key",
            true,
            !typed.trim().is_empty(),
            self.focus == KEY_SAVE_TOKEN,
            KEY_SAVE_TOKEN,
        );
        kit::button(
            &mut self.ui,
            frame,
            [x + width - 200.0 - 16.0 - 160.0, buttons_y, 160.0, 44.0],
            "Not now",
            false,
            true,
            self.focus == KEY_LATER_TOKEN,
            KEY_LATER_TOKEN,
        );
        self.order.push(KEY_SAVE_TOKEN);
        self.order.push(KEY_LATER_TOKEN);
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Powered by GIPHY"),
            frame.rect(x, buttons_y + 12.0, 300.0, 20.0),
            14.0 * s,
            color::MUTED,
            FontWeight::Semibold,
            TextAlign::Start,
        );
    }
}

/// The size a GIF of `size` is drawn at to fit `room` whole, keeping its shape.
fn contain(size: [u32; 2], room: [f32; 2]) -> [f32; 2] {
    let [w, h] = [size[0].max(1) as f32, size[1].max(1) as f32];
    let scale = (room[0] / w).min(room[1] / h);
    [w * scale, h * scale]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: usize) -> GifId {
        GifId::new(&format!("Pick{n:04}xy")).unwrap()
    }

    #[test]
    fn a_picked_gif_takes_the_place_of_the_drafts_gif_link() {
        let first = id(1);
        let second = id(2);
        assert_eq!(
            with_gif("", &first).unwrap(),
            "https://giphy.com/gifs/Pick0001xy"
        );
        let draft = with_gif("ha ", &first).unwrap();
        assert_eq!(draft, "ha https://giphy.com/gifs/Pick0001xy");
        assert_eq!(
            with_gif(&draft, &second).unwrap(),
            "ha https://giphy.com/gifs/Pick0002xy"
        );
        // No room: nothing changes.
        assert_eq!(with_gif(&"a".repeat(DRAFT_MAX - 10), &first), None);
    }

    #[test]
    fn a_gif_fits_its_tile_whole() {
        assert_eq!(contain([200, 100], [100.0, 100.0]), [100.0, 50.0]);
        assert_eq!(contain([100, 200], [100.0, 100.0]), [50.0, 100.0]);
        assert_eq!(contain([0, 0], [10.0, 10.0]), [10.0, 10.0]);
    }

    #[test]
    fn the_keyboard_walks_the_grid_and_scrolls_it() {
        let mut picker = Picker::new();
        picker.ids = (0..40).map(id).collect();
        assert_eq!(picker.step(TILE_BASE, 1, 0), Some(TILE_BASE + 1));
        assert_eq!(picker.step(TILE_BASE, 0, -1), None, "past the top");
        // Down from the last row scrolls by one.
        let last_row = TILE_BASE + (TILES - 1) as u16;
        assert_eq!(picker.step(last_row, 0, 1), Some(last_row));
        assert_eq!(picker.row, 1);
        assert_eq!(picker.tile(TILE_BASE), Some(&id(COLUMNS)));
        // Past the end: the last GIF.
        picker.row = 0;
        let to = picker.step(TILE_BASE, 0, 20).unwrap();
        assert_eq!(picker.tile(to), Some(&id(39)));
        picker.scroll(-100);
        assert_eq!(picker.row, 0);
        picker.scroll(100);
        assert_eq!(picker.row, 40_usize.div_ceil(COLUMNS) - ROWS);
    }

    #[test]
    fn ctrl_a_then_typing_replaces_the_words() {
        use winit::keyboard::KeyCode;
        let mut picker = Picker::new();
        picker.typed = "cats".to_owned();
        assert!(picker.before_edit(KeyCode::KeyA, Some("a"), false, true));
        assert!(!picker.before_edit(KeyCode::KeyD, Some("d"), false, false));
        assert_eq!(picker.typed, "", "the selection is replaced");
        assert!(picker.typed_at.is_some(), "a search follows");
        crate::text_entry::deselect();
    }

    #[test]
    fn search_without_a_key_shows_the_key_card_until_put_off() {
        let mut picker = Picker::new();
        assert!(!picker.key_card_due(false), "closed");
        let _ = picker.open();
        picker.choose_tab(Tab::Search);
        assert!(!picker.key_card_due(true), "a key: no card");
        assert!(picker.key_card_due(false));
        assert!(!picker.key_card_due(false), "already on show");
        picker.put_off_key_card();
        assert!(picker.key_card.is_none());
        assert!(!picker.key_card_due(false), "put off");
        // Get a GIPHY key opens it again, the key saved in its field.
        picker.open_key_card(" saved ");
        assert_eq!(picker.key_card.as_ref().unwrap().typed, "saved");
    }

    #[test]
    fn save_key_takes_only_what_looks_like_a_key() {
        use winit::keyboard::KeyCode;
        let mut picker = Picker::new();
        picker.open_key_card("");
        assert_eq!(picker.save_key(), None);
        assert!(picker.key_card.as_ref().unwrap().notice.is_some());
        picker.edit_key(KeyCode::KeyA, Some("not a key!"), false, false);
        assert!(
            picker.key_card.as_ref().unwrap().notice.is_none(),
            "typing clears it"
        );
        assert_eq!(picker.save_key(), None);
        picker.open_key_card("");
        let key = "0123456789abcdefABCDEF0123456789";
        picker.edit_key(KeyCode::KeyA, Some(key), false, false);
        assert_eq!(picker.save_key().as_deref(), Some(key));
        assert!(picker.key_card.is_none(), "saved: the card goes");
    }

    #[test]
    fn typing_searches_once_it_stops() {
        let mut picker = Picker::new();
        picker.type_key(winit::keyboard::KeyCode::KeyA, Some("Cats  "), false, false);
        assert_eq!(picker.searched, "", "not yet");
        picker.typed_at = Some(Instant::now() - search::DEBOUNCE);
        let chat = super::super::tests::chat(0);
        let inputs = super::super::tests::inputs(&chat, &[], false);
        picker.refresh(&inputs, Some("k"));
        assert_eq!(picker.searched, "cats");
        assert_eq!(picker.searcher.pending_for_test(), Some(("cats", 0)));
    }
}
