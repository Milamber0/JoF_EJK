//! The server browser as the SJK UI draws it (`docs/sjk-ui.md`, Servers): the
//! servers in a sortable list in the middle, where they come from and which of
//! them show down the left, the chosen server on the right (its map's picture,
//! its numbers and who plays there) and the search at the top. It is the
//! browser's own state ([`ServerBrowser`]) and pointer tokens drawn another
//! way, so favourites, filters, sorting, joining and the password and address
//! prompts work as in the other styles; the keyboard reaches the left column
//! too ([`ClientMenu::sjk_browser_key`]).
//!
//! Positions are pixels of the SJK UI's 16:9 frame ([`Frame`]).

use super::{
    Frame, SearchPill, TextTarget, color, key_hint, key_hint_width, kit, text, top_bar, wrap,
};
use crate::console::ViewerConsole;
use crate::menu::browser_filters::BASE as FILTER_BASE;
use crate::menu::browser_table::TABLE_TOKEN;
use crate::menu::browser_view::{
    ADD_FAVOURITE_TOKEN, ADDRESS_TOKEN, BACK_TOKEN, FAVOURITE_TOKEN, FILTER_TOKEN, HEADER_TOKEN,
    JOIN_TOKEN, REFRESH_TOKEN, ROW_TOKEN, SCROLLBAR_TOKEN,
};
use crate::menu::levelshot::{Preview, cover_uv};
use crate::menu::{ClientMenu, MenuAction};
use crate::menu_widgets::{MenuCanvas, TAB_BASE, TextFamily};
use crate::player_menu::ReturnTarget;
use crate::server_browser::{
    DetailsState, Fact, ServerBrowser, ServerEntry, SortColumn, gametype_name,
};
use crate::text::Plain;
use sjk_client::CompatProfile;
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};
use winit::keyboard::KeyCode;

/// The left column: its line, the sources' first top, step and height.
const SIDE_X: f32 = 96.0;
const SIDE_WIDTH: f32 = 330.0;
const SOURCES_TOP: f32 = 190.0;
const SOURCE_STEP: f32 = 60.0;
const SOURCE_HEIGHT: f32 = 52.0;
/// The "Show" sub-heading's line, then the filters' first top and step.
const SHOW_Y: f32 = 352.0;
const FILTERS_TOP: f32 = 374.0;
const FILTER_STEP: f32 = 56.0;
const FILTER_HEIGHT: f32 = 52.0;
/// The column's three actions at its foot: refresh, join by address, add a
/// favourite by its address.
const ACTIONS_TOP: f32 = 786.0;
const ACTION_HEIGHT: f32 = 44.0;
/// The list: its column, the header line, the rows and how many show.
const LIST_X: f32 = 470.0;
const LIST_WIDTH: f32 = 850.0;
const HEADER_TOP: f32 = 176.0;
const HEADER_HEIGHT: f32 = 34.0;
const ROWS_TOP: f32 = 216.0;
const ROW: f32 = 52.0;
const VISIBLE: usize = 14;
/// A favourite's gold dot, before the name.
const MARK_X: f32 = 488.0;
/// The chosen server's column, its map's picture (2:1) and the lines under it.
const DETAIL_X: f32 = 1384.0;
const DETAIL_WIDTH: f32 = 440.0;
const PICTURE: [f32; 4] = [DETAIL_X, 176.0, DETAIL_WIDTH, 220.0];
const NAME_TOP: f32 = 410.0;
const STATS_TOP: f32 = 492.0;
const ADDRESS_Y: f32 = 604.0;
const BUTTONS_TOP: f32 = 664.0;
const PLAYERS_Y: f32 = 748.0;
const PLAYER_LINE: f32 = 28.0;
pub(crate) const PLAYER_LINES: usize = 6;
/// The details panel's player list, which the wheel scrolls.
pub(crate) const PLAYERS_TOKEN: u16 = 27;
/// Rows of players one wheel notch moves.
pub(crate) const PLAYER_WHEEL_ROWS: i32 = 2;
/// The keys' and the status's line at the bottom.
const KEYS_Y: f32 = 992.0;
/// The password and address prompts' card.
const CARD: [f32; 4] = [640.0, 352.0, 640.0, 330.0];

/// The password prompt's tokens: the backdrop, the field, Cancel and Join; then its
/// Remember on this PC switch.
const PASSWORD_TOKENS: [u16; 4] = [33, 30, 31, 32];
const REMEMBER_TOKEN: u16 = 34;
/// The address prompt's: the backdrop, the field, Cancel and Connect.
const ADDRESS_TOKENS: [u16; 4] = [40, 41, 42, 43];

/// One column of the list: where it starts, how wide it is, its header, the
/// sort its header drives and how its cells align.
struct Column {
    x: f32,
    width: f32,
    label: &'static str,
    sort: SortColumn,
    align: TextAlign,
}

const NAME: Column = Column {
    x: 506.0,
    width: 340.0,
    label: "Server",
    sort: SortColumn::Name,
    align: TextAlign::Start,
};
const MODE: Column = Column {
    x: 862.0,
    width: 150.0,
    label: "Mode",
    sort: SortColumn::Gametype,
    align: TextAlign::Start,
};
const MAP: Column = Column {
    x: 1022.0,
    width: 124.0,
    label: "Map",
    sort: SortColumn::Map,
    align: TextAlign::Start,
};
const PLAYERS: Column = Column {
    x: 1150.0,
    width: 72.0,
    label: "Players",
    sort: SortColumn::Players,
    align: TextAlign::End,
};
const PING: Column = Column {
    x: 1232.0,
    width: 76.0,
    label: "Ping",
    sort: SortColumn::Ping,
    align: TextAlign::End,
};
const COLUMNS: [&Column; 5] = [&NAME, &MODE, &MAP, &PLAYERS, &PING];

/// The browser's header token for `sort`, as the shared pointer reads it
/// (`pointer::SORT_COLUMNS`).
fn header_token(sort: SortColumn) -> u16 {
    HEADER_TOKEN
        + match sort {
            SortColumn::Name => 0,
            SortColumn::Map => 1,
            SortColumn::Players => 2,
            SortColumn::Ping => 3,
            SortColumn::Gametype => 4,
        }
}

/// The left column's items, top to bottom: where the servers come from, then
/// which of them show.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Side {
    All,
    Favourites,
    Empty,
    Full,
    Locked,
    Mode,
}

const SIDE: [Side; 6] = [
    Side::All,
    Side::Favourites,
    Side::Empty,
    Side::Full,
    Side::Locked,
    Side::Mode,
];

impl Side {
    /// The browser's token for it: the tabs' and the filter strip's.
    fn token(self) -> u16 {
        match self {
            Self::All => TAB_BASE,
            Self::Favourites => TAB_BASE + 1,
            Self::Empty => FILTER_BASE,
            Self::Full => FILTER_BASE + 1,
            Self::Locked => FILTER_BASE + 2,
            Self::Mode => FILTER_BASE + 4,
        }
    }

    /// The item answering to `token`, as its place in [`SIDE`].
    fn of_token(token: u16) -> Option<usize> {
        SIDE.iter().position(|side| side.token() == token)
    }
}

/// What has the keyboard on the SJK UI's browser besides the list.
#[derive(Debug, Default)]
pub(crate) struct BrowserPage {
    /// The left column's item with the keyboard; none while the list has it.
    side: Option<usize>,
}

/// A prompt over the browser.
enum Dialog<'a> {
    /// The password of `server` (its name, or address), `length` typed, and
    /// whether it is to be remembered on this PC.
    Password {
        server: String,
        length: usize,
        remember: bool,
    },
    /// An address being typed, with what was wrong with the last one, to join it
    /// or (`favourite`) to keep it as a favourite.
    Address {
        input: &'a str,
        error: &'a str,
        favourite: bool,
    },
}

impl ClientMenu {
    /// Draw the SJK UI's server browser, its text to `target`.
    pub(crate) fn append_sjk_browser(&mut self, target: TextTarget<'_>, viewport: [f32; 2]) {
        self.build_sjk_browser(viewport);
        target.append(&self.ui, viewport);
    }

    /// Lay the browser out on `self.ui`.
    fn build_sjk_browser(&mut self, viewport: [f32; 2]) {
        let reveal = self.screen_reveal();
        let frame = Frame::new(viewport);
        self.browser.set_page(VISIBLE);
        let dialog = if self.address_editing {
            Some(Dialog::Address {
                input: &self.address_input,
                error: &self.address_error,
                favourite: self.address_purpose
                    == super::super::controller::AddressPurpose::Favourite,
            })
        } else {
            self.password_target.as_deref().map(|address| {
                let server = self
                    .browser
                    .entries()
                    .iter()
                    .find(|entry| entry.address.to_string() == address)
                    .map_or_else(
                        || address.to_owned(),
                        |entry| Plain(&entry.name).to_string(),
                    );
                Dialog::Password {
                    server,
                    length: self.password.chars().count(),
                    remember: self.remember_password,
                }
            })
        };
        let ui = &mut self.ui;
        ui.begin_transparent(viewport);
        ui.push_opacity(reveal);
        crate::settings::sjk_view::backdrop(ui, viewport);
        let from_game = self.browser_return == ReturnTarget::InGame;
        let found = (!self.browser.filter_text().is_empty()).then(|| self.browser.visible_len());
        top_bar(
            ui,
            &frame,
            if from_game { "Game menu" } else { "Main menu" },
            BACK_TOKEN,
            "Servers",
            dialog.is_none().then_some(SearchPill {
                query: self.browser.filter_text(),
                active: self.filter_editing,
                prompt: "Find a server or map",
                found,
                token: FILTER_TOKEN,
            }),
        );
        let focus = Focus::of(
            dialog.as_ref(),
            self.filter_editing,
            self.sjk_browser.side,
            self.browser.filter_text(),
            self.browser.favorites_only(),
        );
        match &dialog {
            Some(dialog) => draw_dialog(ui, &frame, viewport, dialog),
            None => {
                draw_side(ui, &frame, &self.browser, self.sjk_browser.side);
                draw_list(ui, &frame, &self.browser);
                if let Some(entry) = self.browser.visible_entry(self.browser.selected()) {
                    let map = crate::menu::classic::loading::levelshot_key(&entry.map);
                    let picture = Picture {
                        preview: self.create_game.levelshot_preview(&map),
                        size: self.create_game.levelshot_size(&map),
                    };
                    draw_detail(ui, &frame, &self.browser, entry, picture);
                }
                draw_status(ui, &frame, &status_line(&self.browser, self.state.status()));
            }
        }
        draw_keys(ui, &frame, &focus);
        ui.pop_opacity();
        ui.finish(self.browser_focus);
    }

    /// A key on the SJK UI's browser, before the shared browser keys: the
    /// search while it is typed, the left column while it has the keyboard,
    /// Left into it and Escape clearing a search first. `None` leaves the key
    /// to the shared keys (`ClientMenu::browser_key`).
    pub(crate) fn sjk_browser_key(
        &mut self,
        key: KeyCode,
        console: &mut ViewerConsole,
    ) -> Option<MenuAction> {
        if self.filter_editing {
            match key {
                KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::ArrowDown | KeyCode::Tab => {
                    self.filter_editing = false;
                }
                KeyCode::Escape => {
                    self.browser.clear_filter();
                    self.filter_editing = false;
                }
                KeyCode::Backspace => self.browser.pop_filter(),
                _ => {}
            }
            return Some(MenuAction::None);
        }
        if let Some(index) = self.sjk_browser.side {
            let side = SIDE[index];
            match key {
                KeyCode::ArrowUp | KeyCode::KeyW => {
                    self.sjk_browser.side = Some((index + SIDE.len() - 1) % SIDE.len());
                }
                KeyCode::ArrowDown | KeyCode::KeyS => {
                    self.sjk_browser.side = Some((index + 1) % SIDE.len());
                }
                KeyCode::ArrowLeft if side == Side::Mode => self.step_browser_mode(-1, console),
                KeyCode::ArrowRight if side == Side::Mode => self.step_browser_mode(1, console),
                KeyCode::ArrowRight | KeyCode::Escape => self.sjk_browser.side = None,
                KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                    self.take_side(side, console);
                }
                _ => return None,
            }
            return Some(MenuAction::None);
        }
        match key {
            KeyCode::ArrowLeft => {
                self.sjk_browser.side = Some(usize::from(self.browser.favorites_only()));
                Some(MenuAction::None)
            }
            KeyCode::Escape if !self.browser.filter_text().is_empty() => {
                self.browser.clear_filter();
                Some(MenuAction::None)
            }
            _ => None,
        }
    }

    /// A click on `token` (at `position`) on the SJK UI's browser, before the
    /// shared pointer: a left-column item takes the keyboard (anything else
    /// hands it back to the list), and the game type steps back when clicked
    /// on its left half. `None` leaves the click to the shared pointer.
    pub(crate) fn sjk_browser_pointer(
        &mut self,
        token: u16,
        position: Option<sjk_ui::Vec2>,
        console: &mut ViewerConsole,
    ) -> Option<MenuAction> {
        self.sjk_browser.side = Side::of_token(token);
        if token != Side::Mode.token() {
            return None;
        }
        let back = position
            .zip(self.ui.rect_for(token))
            .is_some_and(|(point, rect)| point.x < rect.x + rect.width * 0.5);
        self.step_browser_mode(if back { -1 } else { 1 }, console);
        Some(MenuAction::None)
    }

    /// Enter on a left-column item: show its servers, flip its filter or step
    /// the game type.
    fn take_side(&mut self, side: Side, console: &mut ViewerConsole) {
        match side {
            Side::All | Side::Favourites => {
                self.browser.set_favorites_only(side == Side::Favourites);
                self.browser_focus = ROW_TOKEN + self.browser.selected() as u16;
            }
            Side::Mode => self.step_browser_mode(1, console),
            Side::Empty | Side::Full | Side::Locked => {
                let _ = self.activate_filter(side.token(), console);
            }
        }
    }

    /// A fresh browser screen: the list has the keyboard.
    pub(crate) fn reset_sjk_browser(&mut self) {
        self.sjk_browser = BrowserPage::default();
    }
}

#[cfg(test)]
impl ClientMenu {
    /// Open the browser on made-up servers for the world shots: the first
    /// (sorted) a favourite whose status has answered, a few locked or
    /// running a mod; nothing goes to the network or is saved.
    pub(crate) fn browser_for_shot(&mut self) {
        use crate::server_browser::{DetailPlayer, DetailsView};
        let ja_plus = || CompatProfile::JaPlus { version: None };
        let rows = [
            (
                "135.125.145.49:29070",
                "^4JoF ^7FFA & duels",
                "mp/ffa3",
                18,
                32,
                24,
                0,
                ja_plus(),
                false,
            ),
            (
                "192.0.2.10:29070",
                "^1Sith ^7Academy",
                "mp/duel6",
                2,
                16,
                61,
                3,
                CompatProfile::TaystJk,
                false,
            ),
            (
                "192.0.2.11:29070",
                "Yavin Saber Club",
                "mp/ffa5",
                9,
                24,
                48,
                0,
                CompatProfile::BaseJka,
                false,
            ),
            (
                "192.0.2.12:29070",
                "^3CTF ^7Night",
                "mp/ctf1",
                12,
                20,
                88,
                8,
                CompatProfile::BaseJka,
                false,
            ),
            (
                "192.0.2.13:29070",
                "Hoth Siege 24/7",
                "mp/siege_hoth",
                14,
                32,
                105,
                7,
                CompatProfile::BaseJka,
                false,
            ),
            (
                "192.0.2.14:29070",
                "Power Duel Pit",
                "mp/duel1",
                3,
                8,
                132,
                4,
                ja_plus(),
                false,
            ),
            (
                "192.0.2.15:29070",
                "Clan training",
                "mp/ffa1",
                4,
                12,
                39,
                6,
                ja_plus(),
                true,
            ),
            (
                "192.0.2.16:29070",
                "Quiet meditation",
                "mp/duel2",
                0,
                10,
                210,
                3,
                CompatProfile::BaseJka,
                false,
            ),
            (
                "192.0.2.17:29070",
                "^5Holocron ^7Hunt",
                "mp/ffa2",
                5,
                16,
                74,
                1,
                CompatProfile::BaseJka,
                false,
            ),
            (
                "192.0.2.18:29070",
                "Bespin rooftops",
                "mp/ffa4",
                7,
                24,
                142,
                6,
                CompatProfile::BaseJka,
                false,
            ),
            (
                "192.0.2.19:29070",
                "Far east duels",
                "mp/duel5",
                6,
                16,
                290,
                3,
                CompatProfile::TaystJk,
                false,
            ),
            (
                "192.0.2.20:29070",
                "Jedi Master arena",
                "mp/ffa1",
                1,
                12,
                66,
                2,
                CompatProfile::BaseJka,
                true,
            ),
        ];
        let entries = rows
            .into_iter()
            .map(
                |(address, name, map, players, capacity, ping, mode, profile, password)| {
                    ServerEntry::for_test(
                        address, name, map, players, capacity, ping, mode, profile, password,
                    )
                },
            )
            .collect::<Vec<_>>();
        let favourite = entries[0].address;
        self.browser.list_for_test(entries, &[favourite]);
        let names = [
            ("^1Darth ^7Sol", 41),
            ("^5Kyle", 37),
            ("Padawan", 30),
            ("^3Rosh", 22),
            ("^2Tavion", 19),
            ("^6Luke", 15),
            ("Jan", 11),
        ];
        let mut players: Vec<DetailPlayer> = names
            .into_iter()
            .map(|(name, score)| DetailPlayer {
                score,
                ping: 30 + score,
                name: name.to_owned(),
            })
            .collect();
        players.extend((0..11).map(|index| DetailPlayer {
            score: 9 - index,
            ping: 60,
            name: format!("Player {}", index + 1),
        }));
        let fact = |label, value: &str| Fact {
            label,
            value: value.to_owned(),
        };
        self.browser.answer_for_test(DetailsView {
            address: favourite,
            hostname: "^4JoF ^7FFA & duels".to_owned(),
            map: "mp/ffa3".to_owned(),
            facts: vec![
                fact("Mod", "JA+ Mod v2.6"),
                fact("Gametype", "FFA"),
                fact("Frag limit", "30"),
                fact("Time limit", "20"),
            ],
            players,
            bots: 0,
        });
        self.open_browser();
    }

    /// The shown map's picture is in the levelshot texture (or there is none).
    pub(crate) fn browser_picture_settled(&self) -> bool {
        self.browser
            .visible_entry(self.browser.selected())
            .is_none_or(|entry| {
                let map = crate::menu::classic::loading::levelshot_key(&entry.map);
                self.create_game.levelshot_preview(&map) != Preview::Loading
            })
    }

    /// Type `search` into the browser's search and give the left column's
    /// `side` item the keyboard.
    pub(crate) fn browser_search_for_shot(&mut self, search: &str, side: usize) {
        self.browser.clear_filter();
        self.browser.push_filter(search);
        self.sjk_browser.side = Some(side);
    }

    /// Open the password prompt for the selected server.
    pub(crate) fn browser_password_for_shot(&mut self, typed: &str) {
        self.sjk_browser.side = None;
        self.password_target = self
            .browser
            .visible_entry(self.browser.selected())
            .map(|entry| entry.address.to_string());
        self.password = typed.to_owned();
    }
}

/// What has the keyboard, for the keys at the bottom.
enum Focus {
    Password,
    Address { favourite: bool },
    Search,
    Side(Side),
    List { searched: bool, favourites: bool },
}

impl Focus {
    fn of(
        dialog: Option<&Dialog<'_>>,
        searching: bool,
        side: Option<usize>,
        search: &str,
        favourites: bool,
    ) -> Self {
        match (dialog, side) {
            (Some(Dialog::Password { .. }), _) => Self::Password,
            (Some(Dialog::Address { favourite, .. }), _) => Self::Address {
                favourite: *favourite,
            },
            (None, _) if searching => Self::Search,
            (None, Some(index)) => Self::Side(SIDE[index]),
            (None, None) => Self::List {
                searched: !search.is_empty(),
                favourites,
            },
        }
    }
}

fn push(canvas: &mut MenuCanvas, command: DrawCommand) {
    let _ = canvas.draw_list_mut().push(command);
}

/// The left column: the sources down a lit rail, the filters under "Show",
/// the two actions at its foot. `focus` is the item with the keyboard.
fn draw_side(ui: &mut MenuCanvas, frame: &Frame, browser: &ServerBrowser, focus: Option<usize>) {
    let s = frame.s;
    let current = usize::from(browser.favorites_only());
    kit::rail(
        ui,
        frame,
        SIDE_X,
        SOURCES_TOP - 20.0,
        SOURCES_TOP + 2.0 * SOURCE_STEP + 12.0,
        Some(SOURCES_TOP + current as f32 * SOURCE_STEP + SOURCE_HEIGHT * 0.5),
    );
    let counts = [browser.entries().len(), browser.favorites_listed()];
    for (index, (label, count)) in ["All servers", "Favourites"]
        .into_iter()
        .zip(counts)
        .enumerate()
    {
        let top = SOURCES_TOP + index as f32 * SOURCE_STEP;
        let token = SIDE[index].token();
        let hovered = ui.token_hovered(token);
        if focus == Some(index) {
            kit::band(
                ui,
                frame,
                [SIDE_X + 14.0, top, SIDE_WIDTH - 14.0, SOURCE_HEIGHT],
            );
        }
        text(
            ui,
            TextFamily::Display,
            format_args!("{label}"),
            frame.rect(SIDE_X + 34.0, top, SIDE_WIDTH - 110.0, SOURCE_HEIGHT),
            26.0 * s,
            match (index == current, hovered) {
                (true, _) => color::GOLD_BRIGHT,
                (false, true) => color::TEXT,
                (false, false) => color::MUTED,
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        text(
            ui,
            TextFamily::Display,
            format_args!("{count}"),
            frame.rect(SIDE_X + SIDE_WIDTH - 80.0, top, 66.0, SOURCE_HEIGHT),
            20.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::End,
        );
        ui.hit_region(token, frame.rect(SIDE_X, top, SIDE_WIDTH, SOURCE_HEIGHT));
    }
    kit::heading(ui, frame, SIDE_X, SHOW_Y, SIDE_WIDTH, "Show");
    let filters = browser.filters();
    let rows = [
        (Side::Empty, "Empty servers", filters.empty),
        (Side::Full, "Full servers", filters.full),
        (Side::Locked, "Locked servers", filters.password),
        (Side::Mode, "Game type", false),
    ];
    for (row, (side, label, on)) in rows.into_iter().enumerate() {
        let index = row + 2;
        let top = FILTERS_TOP + row as f32 * FILTER_STEP;
        let middle = top + FILTER_HEIGHT * 0.5;
        let focused = focus == Some(index);
        if focused {
            kit::band(ui, frame, [SIDE_X, top, SIDE_WIDTH, FILTER_HEIGHT]);
        }
        text(
            ui,
            TextFamily::Body,
            format_args!("{label}"),
            frame.rect(SIDE_X + 22.0, top, 150.0, FILTER_HEIGHT),
            19.0 * s,
            if focused {
                Color::new(1.0, 1.0, 1.0, 1.0)
            } else {
                color::alpha(color::TEXT, 0.88)
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        if side == Side::Mode {
            let cycler = [SIDE_X + 136.0, middle - 19.0, SIDE_WIDTH - 148.0, 38.0];
            let mode = if filters.mode < 0 {
                "All"
            } else {
                gametype_name(Some(filters.mode))
            };
            kit::cycler(ui, frame, cycler, format_args!("{mode}"), None, focused);
            let [x, y, width, height] = cycler;
            ui.hit_region(side.token(), frame.rect(x, y, width, height));
        } else {
            kit::switch(ui, frame, SIDE_X + SIDE_WIDTH - 12.0, middle, on, focused);
            ui.hit_region(
                side.token(),
                frame.rect(SIDE_X, top, SIDE_WIDTH, FILTER_HEIGHT),
            );
        }
    }
    let refreshing = browser.is_refreshing();
    kit::button(
        ui,
        frame,
        [SIDE_X, ACTIONS_TOP, SIDE_WIDTH, ACTION_HEIGHT],
        if refreshing {
            "Refreshing..."
        } else {
            "Refresh the list"
        },
        false,
        !refreshing,
        false,
        REFRESH_TOKEN,
    );
    kit::button(
        ui,
        frame,
        [
            SIDE_X,
            ACTIONS_TOP + ACTION_HEIGHT + 12.0,
            SIDE_WIDTH,
            ACTION_HEIGHT,
        ],
        "Join by address",
        false,
        true,
        false,
        ADDRESS_TOKEN,
    );
    kit::button(
        ui,
        frame,
        [
            SIDE_X,
            ACTIONS_TOP + 2.0 * (ACTION_HEIGHT + 12.0),
            SIDE_WIDTH,
            ACTION_HEIGHT,
        ],
        "Add a favourite",
        false,
        true,
        false,
        ADD_FAVOURITE_TOKEN,
    );
}

/// The list: its sortable header, the servers on show and its scrollbar, or
/// a line saying why it is empty.
fn draw_list(ui: &mut MenuCanvas, frame: &Frame, browser: &ServerBrowser) {
    let s = frame.s;
    let (sort, descending) = browser.sort_state();
    for column in COLUMNS {
        let token = header_token(column.sort);
        let active = column.sort == sort;
        let hovered = ui.token_hovered(token);
        let colour = match (active, hovered) {
            (true, _) => color::GOLD_BRIGHT,
            (false, true) => color::TEXT,
            (false, false) => color::MUTED,
        };
        let middle = HEADER_TOP + HEADER_HEIGHT * 0.5;
        text(
            ui,
            TextFamily::Display,
            format_args!("{}", column.label),
            frame.rect(column.x, HEADER_TOP, column.width, HEADER_HEIGHT),
            18.0 * s,
            colour,
            FontWeight::Regular,
            column.align,
        );
        if active {
            // Rajdhani at 18 sets these labels at about 6.2 pixels a character.
            let label = 6.2 * column.label.len() as f32;
            let x = match column.align {
                TextAlign::End => column.x + column.width - label - 11.0,
                _ => column.x + label + 11.0,
            };
            kit::sort_mark(ui, frame, x, middle, descending, colour);
        }
        ui.hit_region(
            token,
            frame.rect(
                column.x - 6.0,
                HEADER_TOP,
                column.width + 12.0,
                HEADER_HEIGHT,
            ),
        );
    }
    push(
        ui,
        DrawCommand::SolidRect {
            rect: frame.rect(LIST_X, HEADER_TOP + HEADER_HEIGHT, LIST_WIDTH, 1.0),
            color: color::alpha(color::HOLO, 0.22),
        },
    );
    ui.scroll_region(
        TABLE_TOKEN,
        frame.rect(LIST_X, ROWS_TOP, LIST_WIDTH, VISIBLE as f32 * ROW),
    );
    if browser.visible_len() == 0 {
        text(
            ui,
            TextFamily::Body,
            format_args!("{}", EmptyNote(browser)),
            frame.rect(LIST_X + 22.0, ROWS_TOP + 8.0, LIST_WIDTH - 44.0, 36.0),
            19.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }
    let first = browser.scroll();
    for slot in 0..VISIBLE {
        let row = first + slot;
        let Some(entry) = browser.visible_entry(row) else {
            break;
        };
        let top = ROWS_TOP + slot as f32 * ROW;
        draw_row(
            ui,
            frame,
            entry,
            top,
            row,
            row == browser.selected(),
            browser.is_favorite(entry.address),
        );
    }
    if browser.visible_len() > VISIBLE {
        ui.scrollbar(
            SCROLLBAR_TOKEN,
            frame.rect(
                LIST_X + LIST_WIDTH + 14.0,
                ROWS_TOP,
                4.0,
                VISIBLE as f32 * ROW,
            ),
            first,
            VISIBLE,
            browser.visible_len(),
        );
    }
}

/// Why the list shows no server.
struct EmptyNote<'a>(&'a ServerBrowser);

impl std::fmt::Display for EmptyNote<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let browser = self.0;
        let search = browser.filter_text();
        if !search.is_empty() {
            write!(
                formatter,
                "No server or map matches \u{201c}{search}\u{201d}."
            )
        } else if browser.favorites_only() && browser.favorites_listed() == 0 {
            formatter.write_str("No favourites answered. F on a server keeps it here.")
        } else if browser.entries().is_empty() && browser.is_refreshing() {
            formatter.write_str("Asking the master server for servers...")
        } else if browser.entries().is_empty() {
            formatter.write_str("No server answered. R asks the master server again.")
        } else {
            formatter.write_str("The choices under Show hide every server.")
        }
    }
}

/// Server `entry`, row `row` of the list, on the line whose top is `top`.
fn draw_row(
    ui: &mut MenuCanvas,
    frame: &Frame,
    entry: &ServerEntry,
    top: f32,
    row: usize,
    selected: bool,
    favourite: bool,
) {
    let s = frame.s;
    let token = ROW_TOKEN + row as u16;
    let middle = top + ROW * 0.5;
    if selected {
        kit::band(ui, frame, [LIST_X, top, LIST_WIDTH, ROW]);
    } else if ui.token_hovered(token) {
        push(
            ui,
            DrawCommand::RoundedRect {
                rect: frame.rect(LIST_X, top, LIST_WIDTH, ROW),
                radius: 10.0 * s,
                color: color::alpha(color::HOLO, 0.05),
            },
        );
    }
    if favourite {
        kit::changed_dot(ui, frame, MARK_X, middle);
    }
    let humans = i32::from(entry.players) - entry.bots.max(0);
    let lock = if entry.password { 26.0 } else { 0.0 };
    // The name in its colours; an empty server's faded, its colours too.
    text(
        ui,
        TextFamily::Body,
        format_args!("{}", entry.name),
        frame.rect(NAME.x, top, NAME.width - lock, ROW),
        19.0 * s,
        match (selected, humans > 0) {
            (true, _) => Color::new(1.0, 1.0, 1.0, 1.0),
            (false, true) => color::alpha(color::TEXT, 0.92),
            (false, false) => color::alpha(color::TEXT, 0.5),
        },
        FontWeight::Regular,
        TextAlign::Start,
    );
    if entry.password {
        padlock(
            ui,
            frame,
            NAME.x + NAME.width - 10.0,
            middle,
            color::GOLD_BRIGHT,
        );
    }
    let quiet = if selected { color::TEXT } else { color::MUTED };
    text(
        ui,
        TextFamily::Display,
        format_args!("{}", entry.gametype),
        frame.rect(MODE.x, top, MODE.width, ROW),
        19.0 * s,
        quiet,
        FontWeight::Regular,
        TextAlign::Start,
    );
    if let Some(tag) = profile_tag(&entry.profile) {
        // Rajdhani at 19 is about 8.4 pixels a character.
        let x = MODE.x + 8.4 * entry.gametype.len() as f32 + 10.0;
        let width = 14.0 + 7.0 * tag.len() as f32;
        if x + width <= MODE.x + MODE.width {
            push(
                ui,
                DrawCommand::Border {
                    rect: frame.rect(x, middle - 11.0, width, 22.0),
                    radius: 11.0 * s,
                    width: s.max(1.0),
                    color: color::alpha(color::HOLO, 0.4),
                },
            );
            text(
                ui,
                TextFamily::Display,
                format_args!("{tag}"),
                frame.rect(x, middle - 11.0, width, 22.0),
                14.0 * s,
                color::HOLO,
                FontWeight::Semibold,
                TextAlign::Center,
            );
        }
    }
    text(
        ui,
        TextFamily::Body,
        format_args!("{}", short_map(&entry.map)),
        frame.rect(MAP.x, top, MAP.width, ROW),
        17.0 * s,
        quiet,
        FontWeight::Regular,
        TextAlign::Start,
    );
    text(
        ui,
        TextFamily::Display,
        format_args!("{}/{}", entry.players, entry.capacity),
        frame.rect(PLAYERS.x, top, PLAYERS.width, ROW),
        20.0 * s,
        if humans > 0 {
            color::TEXT
        } else {
            color::QUIET
        },
        FontWeight::Regular,
        PLAYERS.align,
    );
    signal(ui, frame, PING.x + 2.0, middle, entry.ping_millis, selected);
    text(
        ui,
        TextFamily::Display,
        format_args!("{}", entry.ping_millis),
        frame.rect(PING.x + 30.0, top, PING.width - 30.0, ROW),
        19.0 * s,
        quiet,
        FontWeight::Regular,
        PING.align,
    );
    ui.hit_region(token, frame.rect(LIST_X, top, LIST_WIDTH, ROW));
}

/// A server's mod as a small tag after its mode: none for base Jedi Academy.
fn profile_tag(profile: &CompatProfile) -> Option<&'static str> {
    match profile {
        CompatProfile::BaseJka => None,
        CompatProfile::JaPlus { .. } => Some("JA+"),
        CompatProfile::TaystJk => Some("JAPRO"),
        CompatProfile::Unknown(_) => Some("Mod"),
    }
}

/// A server's mod named in full, for the chosen server's numbers.
fn profile_name(profile: &CompatProfile) -> &'static str {
    match profile {
        CompatProfile::BaseJka => "Base",
        CompatProfile::JaPlus { .. } => "JA+",
        CompatProfile::TaystJk => "JAPRO",
        CompatProfile::Unknown(_) => "Other",
    }
}

/// A map's name without the `mp/` every multiplayer map starts with.
fn short_map(map: &str) -> &str {
    map.strip_prefix("mp/")
        .or_else(|| map.strip_prefix("MP/"))
        .unwrap_or(map)
}

/// How many of four bars a ping lights: four up to 60 ms, none past 400.
pub(crate) fn signal_bars(ping: u32) -> usize {
    match ping {
        0..=60 => 4,
        61..=110 => 3,
        111..=180 => 2,
        181..=400 => 1,
        _ => 0,
    }
}

/// Four rising bars from `x`, their feet below `y`, as many lit as the ping
/// is good.
pub(crate) fn signal(
    ui: &mut MenuCanvas,
    frame: &Frame,
    x: f32,
    y: f32,
    ping: u32,
    selected: bool,
) {
    let lit = signal_bars(ping);
    for bar in 0..4 {
        let height = 6.0 + bar as f32 * 4.0;
        let rect = frame.rect(x + bar as f32 * 7.0, y + 9.0 - height, 4.0, height);
        push(
            ui,
            DrawCommand::RoundedRect {
                rect,
                radius: 1.5 * frame.s,
                color: match (bar < lit, selected) {
                    (true, true) => color::GOLD_BRIGHT,
                    (true, false) => color::HOLO,
                    (false, _) => color::alpha(color::HOLO, 0.2),
                },
            },
        );
    }
}

/// A small padlock centred on (`x`, `y`): a half-ring shackle over its body.
fn padlock(ui: &mut MenuCanvas, frame: &Frame, x: f32, y: f32, colour: Color) {
    let s = frame.s;
    push(
        ui,
        DrawCommand::Arc {
            center: frame.point(x, y - 2.0),
            radius: 4.5 * s,
            width: 1.8 * s,
            start: std::f32::consts::PI,
            sweep: std::f32::consts::PI,
            color: colour,
            knockout: None,
        },
    );
    push(
        ui,
        DrawCommand::RoundedRect {
            rect: frame.rect(x - 6.5, y - 2.0, 13.0, 10.0),
            radius: 2.0 * s,
            color: colour,
        },
    );
}

/// The chosen server's map picture: what the levelshot cache has of it.
struct Picture {
    preview: Preview,
    size: Option<[u32; 2]>,
}

/// The chosen server: its map's picture, its name, its numbers, Join and the
/// favourite, then who is playing.
fn draw_detail(
    ui: &mut MenuCanvas,
    frame: &Frame,
    browser: &ServerBrowser,
    entry: &ServerEntry,
    picture: Picture,
) {
    let s = frame.s;
    let [x, y, width, height] = PICTURE;
    let rect = frame.rect(x, y, width, height);
    push(
        ui,
        DrawCommand::SolidRect {
            rect,
            color: color::alpha(color::SPACE, 0.7),
        },
    );
    if picture.preview == Preview::Image {
        push(
            ui,
            DrawCommand::TexturedQuadUv {
                rect,
                texture: crate::ui_renderer::LEVELSHOT_TEXTURE,
                color: Color::new(1.0, 1.0, 1.0, 1.0),
                uv: cover_uv(picture.size.unwrap_or([4, 3]), width / height),
            },
        );
    } else if picture.preview == Preview::Missing {
        text(
            ui,
            TextFamily::Body,
            format_args!("No picture of this map"),
            frame.rect(x, y, width, height - 40.0),
            16.0 * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::Center,
        );
    }
    super::fade(
        ui,
        frame.rect(x, y + height * 0.5, width, height * 0.5),
        color::alpha(color::SPACE, 0.0),
        color::alpha(color::SPACE, 0.85),
    );
    push(
        ui,
        DrawCommand::Border {
            rect,
            radius: 0.0,
            width: s.max(1.0),
            color: color::alpha(color::HOLO, 0.25),
        },
    );
    text(
        ui,
        TextFamily::Display,
        format_args!("{}", short_map(&entry.map)),
        frame.rect(x + 18.0, y + height - 46.0, width - 36.0, 36.0),
        24.0 * s,
        color::TEXT,
        FontWeight::Semibold,
        TextAlign::Start,
    );

    // The name in its colours, the second line going on in the colour the
    // first ended in.
    let mut carried = "";
    for (line, part) in wrap(&entry.name, 30).take(2).enumerate() {
        text(
            ui,
            TextFamily::Display,
            format_args!("{carried}{part}"),
            frame.rect(DETAIL_X, NAME_TOP + line as f32 * 36.0, DETAIL_WIDTH, 36.0),
            30.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        carried = crate::text::last_colour(part).unwrap_or(carried);
    }

    let view = match browser.details() {
        DetailsState::Ready(view) if view.address == entry.address => Some(view),
        _ => None,
    };
    let bots = view.map_or_else(
        || usize::try_from(entry.bots).unwrap_or(0),
        |view| view.bots,
    );
    let players_label = match bots {
        0 => "Players".to_owned(),
        1 => "Players, 1 bot".to_owned(),
        bots => format!("Players, {bots} bots"),
    };
    let mod_name = view
        .and_then(|view| view.facts.iter().find(|fact| fact.label == "Mod"))
        .map_or(profile_name(&entry.profile), |fact| fact.value.as_str());
    let stats = [
        ("Mode", entry.gametype.clone()),
        (
            players_label.as_str(),
            format!("{} of {}", entry.players, entry.capacity),
        ),
        ("Ping", format!("{} ms", entry.ping_millis)),
        ("Mod", mod_name.to_owned()),
    ];
    for (index, (label, value)) in stats.iter().enumerate() {
        let left = DETAIL_X + (index % 2) as f32 * 220.0;
        let top = STATS_TOP + (index / 2) as f32 * 56.0;
        text(
            ui,
            TextFamily::Body,
            format_args!("{label}"),
            frame.rect(left, top, 210.0, 20.0),
            14.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        text(
            ui,
            TextFamily::Display,
            format_args!("{value}"),
            frame.rect(left, top + 20.0, 210.0, 30.0),
            22.0 * s,
            color::TEXT,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }
    text(
        ui,
        TextFamily::Body,
        format_args!("{}", entry.address),
        frame.rect(DETAIL_X, ADDRESS_Y, 260.0, 24.0),
        16.0 * s,
        color::HOLO,
        FontWeight::Regular,
        TextAlign::Start,
    );
    if entry.password {
        text(
            ui,
            TextFamily::Body,
            format_args!("Needs a password"),
            frame.rect(DETAIL_X + 220.0, ADDRESS_Y, DETAIL_WIDTH - 220.0, 24.0),
            16.0 * s,
            color::GOLD_BRIGHT,
            FontWeight::Regular,
            TextAlign::End,
        );
    }
    if let Some(limits) = view.and_then(|view| limits(&view.facts)) {
        text(
            ui,
            TextFamily::Body,
            format_args!("{limits}"),
            frame.rect(DETAIL_X, ADDRESS_Y + 28.0, DETAIL_WIDTH, 24.0),
            16.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }
    kit::button(
        ui,
        frame,
        [DETAIL_X, BUTTONS_TOP, 200.0, 46.0],
        "Join",
        true,
        true,
        false,
        JOIN_TOKEN,
    );
    kit::button(
        ui,
        frame,
        [DETAIL_X + 214.0, BUTTONS_TOP, DETAIL_WIDTH - 214.0, 46.0],
        if browser.is_favorite(entry.address) {
            "Remove favourite"
        } else {
            "Add to favourites"
        },
        false,
        true,
        false,
        FAVOURITE_TOKEN,
    );

    kit::heading(ui, frame, DETAIL_X, PLAYERS_Y, DETAIL_WIDTH, "Playing now");
    let mut line_y = PLAYERS_Y + 22.0;
    let note = |ui: &mut MenuCanvas, message: &str| {
        text(
            ui,
            TextFamily::Body,
            format_args!("{message}"),
            frame.rect(DETAIL_X, PLAYERS_Y + 22.0, DETAIL_WIDTH, PLAYER_LINE),
            16.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
    };
    match browser.details() {
        DetailsState::Ready(view) if view.address == entry.address => {
            if view.players.is_empty() {
                note(ui, "Nobody is playing.");
            }
            // The wheel scrolls the list (`PLAYERS_TOKEN`); a thin bar on the right
            // shows where the rows in view are.
            let scrolls = view.players.len() > PLAYER_LINES;
            let first = browser
                .player_scroll()
                .min(view.players.len().saturating_sub(PLAYER_LINES));
            let bar = if scrolls { 14.0 } else { 0.0 };
            ui.scroll_region(
                PLAYERS_TOKEN,
                frame.rect(
                    DETAIL_X,
                    line_y,
                    DETAIL_WIDTH,
                    PLAYER_LINES as f32 * PLAYER_LINE,
                ),
            );
            for player in view.players.iter().skip(first).take(PLAYER_LINES) {
                let bot = player.ping == 0;
                text(
                    ui,
                    TextFamily::Body,
                    format_args!("{}", player.name),
                    frame.rect(DETAIL_X + 12.0, line_y, DETAIL_WIDTH - 120.0, PLAYER_LINE),
                    17.0 * s,
                    if bot {
                        color::alpha(color::TEXT, 0.5)
                    } else {
                        color::TEXT
                    },
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                text(
                    ui,
                    TextFamily::Display,
                    format_args!("{}", player.score),
                    frame.rect(
                        DETAIL_X + DETAIL_WIDTH - 100.0 - bar,
                        line_y,
                        100.0,
                        PLAYER_LINE,
                    ),
                    18.0 * s,
                    color::MUTED,
                    FontWeight::Regular,
                    TextAlign::End,
                );
                line_y += PLAYER_LINE;
            }
            if scrolls {
                let track = PLAYER_LINES as f32 * PLAYER_LINE;
                let total = view.players.len() as f32;
                let top = PLAYERS_Y + 22.0;
                push(
                    ui,
                    DrawCommand::SolidRect {
                        rect: frame.rect(DETAIL_X + DETAIL_WIDTH - 4.0, top, 3.0, track),
                        color: color::alpha(color::HOLO, 0.15),
                    },
                );
                push(
                    ui,
                    DrawCommand::SolidRect {
                        rect: frame.rect(
                            DETAIL_X + DETAIL_WIDTH - 4.0,
                            top + track * first as f32 / total,
                            3.0,
                            track * PLAYER_LINES as f32 / total,
                        ),
                        color: color::alpha(color::HOLO, 0.7),
                    },
                );
            }
        }
        DetailsState::Failed(_) => note(ui, "The server did not say who is playing."),
        _ => note(ui, "Asking the server who is playing..."),
    }
}

/// A server's limits in words ("20 frags, 15 minutes"), from its status.
fn limits(facts: &[Fact]) -> Option<String> {
    let words = [
        ("Frag limit", "frags"),
        ("Duel limit", "duel wins"),
        ("Capture limit", "captures"),
        ("Time limit", "minutes"),
    ];
    let parts: Vec<String> = words
        .iter()
        .filter_map(|(label, unit)| {
            let value = facts.iter().find(|fact| fact.label == *label)?;
            let value = value.value.trim();
            (value != "0" && !value.is_empty()).then(|| format!("{value} {unit}"))
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// The line at the bottom left: how the list stands.
fn status_line(browser: &ServerBrowser, status: &str) -> String {
    let count = browser.entries().len();
    let refreshing = browser.is_refreshing();
    match (count, refreshing) {
        (0, true) => "Asking the master server...".to_owned(),
        (count, true) => format!("{count} servers so far, more answering..."),
        (0, false) if status.starts_with("Refresh failed") => status.to_owned(),
        (0, false) => "No server has answered yet.".to_owned(),
        (1, false) => "1 server answered".to_owned(),
        (count, false) => format!("{count} servers answered"),
    }
}

fn draw_status(ui: &mut MenuCanvas, frame: &Frame, status: &str) {
    text(
        ui,
        TextFamily::Body,
        format_args!("{status}"),
        frame.rect(SIDE_X, KEYS_Y, 640.0, 24.0),
        16.0 * frame.s,
        color::MUTED,
        FontWeight::Regular,
        TextAlign::Start,
    );
}

/// The keys of what has the keyboard, right-aligned at the bottom.
fn draw_keys(ui: &mut MenuCanvas, frame: &Frame, focus: &Focus) {
    let s = frame.s;
    let mut keys: Vec<(&[&str], &str)> = Vec::with_capacity(5);
    match focus {
        Focus::Password => keys.extend([(&["Enter"][..], "join"), (&["Esc"][..], "cancel")]),
        Focus::Address { favourite } => keys.extend([
            (&["Enter"][..], if *favourite { "add" } else { "connect" }),
            (&["Esc"][..], "cancel"),
        ]),
        Focus::Search => keys.extend([(&["Enter"][..], "servers"), (&["Esc"][..], "clear")]),
        Focus::Side(side) => {
            keys.push(match side {
                Side::Mode => (&["Left", "Right"][..], "game type"),
                Side::All | Side::Favourites => (&["Enter"][..], "show"),
                _ => (&["Enter"][..], "switch"),
            });
            keys.push((&["Up", "Down"][..], "choose"));
            keys.push((&["Esc"][..], "servers"));
        }
        Focus::List {
            searched,
            favourites,
        } => {
            keys.extend([(&["Enter"][..], "join"), (&["F"][..], "favourite")]);
            keys.push((&["A"][..], "add by address"));
            if !searched {
                keys.push((
                    &["Tab"][..],
                    if *favourites {
                        "all servers"
                    } else {
                        "favourites"
                    },
                ));
            }
            keys.push((&["Left"][..], "filters"));
            keys.push(if *searched {
                (&["Esc"][..], "clear the search")
            } else {
                (&["R"][..], "refresh")
            });
        }
    }
    let gap = 30.0 * s;
    let width: f32 = keys
        .iter()
        .map(|(caps, action)| key_hint_width(caps, action, s))
        .sum::<f32>()
        + gap * keys.len().saturating_sub(1) as f32;
    let [right, y] = frame.point(1824.0, KEYS_Y);
    let mut x = right - width;
    for (caps, action) in keys {
        x = key_hint(ui, caps, action, x, y, s) + gap;
    }
}

/// A prompt over the browser, which leaves out the list beneath it (text
/// draws over every shape, so the list's would show through the card).
fn draw_dialog(ui: &mut MenuCanvas, frame: &Frame, viewport: [f32; 2], dialog: &Dialog<'_>) {
    let s = frame.s;
    let [backdrop, field_token, cancel, submit] = match dialog {
        Dialog::Password { .. } => PASSWORD_TOKENS,
        Dialog::Address { .. } => ADDRESS_TOKENS,
    };
    ui.hit_region(backdrop, Rect::new(0.0, 0.0, viewport[0], viewport[1]));
    // The password's card is taller, for its Remember on this PC line.
    let card = match dialog {
        Dialog::Password { .. } => [CARD[0], CARD[1] - 26.0, CARD[2], CARD[3] + 52.0],
        Dialog::Address { .. } => CARD,
    };
    let [x, y, width, height] = card;
    kit::card(ui, frame, card);
    let (title, body, value, error, action, ready) = match dialog {
        Dialog::Password { server, length, .. } => (
            "Password",
            format!("{server} needs a password to join."),
            String::new(),
            "",
            "Join",
            *length > 0,
        ),
        Dialog::Address {
            input,
            error,
            favourite: false,
        } => (
            "Join by address",
            "Type the server's address, with its port if it is not 29070.".to_owned(),
            (*input).to_owned(),
            *error,
            "Connect",
            !input.trim().is_empty(),
        ),
        Dialog::Address {
            input,
            error,
            favourite: true,
        } => (
            "Add a favourite",
            "Type the server's address, with its port if it is not 29070. It is kept and asked for whether a master server lists it or not.".to_owned(),
            (*input).to_owned(),
            *error,
            "Add",
            !input.trim().is_empty(),
        ),
    };
    text(
        ui,
        TextFamily::Display,
        format_args!("{title}"),
        frame.rect(x + 40.0, y + 28.0, width - 80.0, 52.0),
        36.0 * s,
        color::TEXT,
        FontWeight::Semibold,
        TextAlign::Start,
    );
    for (line, part) in wrap(&body, 58).take(2).enumerate() {
        text(
            ui,
            TextFamily::Body,
            format_args!("{part}"),
            frame.rect(x + 40.0, y + 88.0 + line as f32 * 26.0, width - 80.0, 26.0),
            17.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }
    let field = [x + 40.0, y + 158.0, width - 80.0, 50.0];
    let [fx, fy, fw, fh] = field;
    match dialog {
        // The password as dots, as many as fit, then the cursor.
        Dialog::Password { length, .. } => {
            let fit = ((fw - 60.0) / 16.0) as usize;
            let dots = (*length).min(fit);
            kit::field(ui, frame, field, format_args!(""), true, false);
            for dot in 0..dots {
                push(
                    ui,
                    DrawCommand::RoundedRect {
                        rect: frame.rect(
                            fx + 20.0 + dot as f32 * 16.0,
                            fy + fh * 0.5 - 4.5,
                            9.0,
                            9.0,
                        ),
                        radius: 4.5 * s,
                        color: color::TEXT,
                    },
                );
            }
            text(
                ui,
                TextFamily::Body,
                format_args!("_"),
                frame.rect(fx + 18.0 + dots as f32 * 16.0, fy, 20.0, fh),
                17.0 * s,
                color::TEXT,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        Dialog::Address { .. } => {
            kit::field(ui, frame, field, format_args!("{value}_"), true, false);
        }
    }
    ui.hit_region(field_token, frame.rect(fx, fy, fw, fh));
    if !error.is_empty() {
        text(
            ui,
            TextFamily::Body,
            format_args!("{error}"),
            frame.rect(x + 40.0, y + 214.0, width - 80.0, 26.0),
            16.0 * s,
            color::GOLD_BRIGHT,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }
    if let Dialog::Password { remember, .. } = dialog {
        // Remember on this PC: the switch at its right, the whole line its target.
        let line = y + 250.0;
        text(
            ui,
            TextFamily::Body,
            format_args!("Remember on this PC"),
            frame.rect(x + 40.0, line - 12.0, 300.0, 24.0),
            17.0 * s,
            if *remember { color::TEXT } else { color::MUTED },
            FontWeight::Regular,
            TextAlign::Start,
        );
        let hovered = ui.token_hovered(REMEMBER_TOKEN);
        kit::switch(ui, frame, x + 40.0 + 300.0, line, *remember, hovered);
        ui.hit_region(
            REMEMBER_TOKEN,
            frame.rect(x + 34.0, line - 18.0, 312.0, 36.0),
        );
    }
    let buttons = y + height - 40.0 - 46.0;
    kit::button(
        ui,
        frame,
        [
            x + width - 40.0 - 180.0 - 14.0 - 150.0,
            buttons,
            150.0,
            46.0,
        ],
        "Cancel",
        false,
        true,
        false,
        cancel,
    );
    kit::button(
        ui,
        frame,
        [x + width - 40.0 - 180.0, buttons, 180.0, 46.0],
        action,
        true,
        ready,
        false,
        submit,
    );
}

// The left column, the list with its scrollbar and the chosen server's
// column side by side inside the frame's margins.
const _: () = assert!(SIDE_X + SIDE_WIDTH < LIST_X);
const _: () = assert!(LIST_X < MARK_X - 8.0 && MARK_X + 8.0 < NAME.x);
const _: () = assert!(NAME.x + NAME.width < MODE.x && MODE.x + MODE.width < MAP.x);
const _: () = assert!(MAP.x + MAP.width < PLAYERS.x && PLAYERS.x + PLAYERS.width < PING.x);
const _: () = assert!(PING.x + PING.width <= LIST_X + LIST_WIDTH);
const _: () = assert!(LIST_X + LIST_WIDTH + 18.0 < DETAIL_X);
const _: () = assert!(DETAIL_X + DETAIL_WIDTH <= 1824.0);
// Top to bottom: the search clears the header; the rows, the column's actions
// and the players end above the keys; the chosen server's lines in order.
const _: () = assert!(super::SEARCH_PILL[1] + super::SEARCH_PILL[3] < HEADER_TOP);
const _: () = assert!(HEADER_TOP + HEADER_HEIGHT < ROWS_TOP);
const _: () = assert!(ROWS_TOP + VISIBLE as f32 * ROW < KEYS_Y - 20.0);
const _: () = assert!(FILTERS_TOP + 4.0 * FILTER_STEP < ACTIONS_TOP);
const _: () = assert!(ACTIONS_TOP + 3.0 * ACTION_HEIGHT + 24.0 < KEYS_Y - 20.0);
const _: () = assert!(PICTURE[1] + PICTURE[3] < NAME_TOP && NAME_TOP + 72.0 < STATS_TOP);
const _: () = assert!(STATS_TOP + 112.0 <= ADDRESS_Y && ADDRESS_Y + 52.0 < BUTTONS_TOP);
const _: () = assert!(BUTTONS_TOP + 46.0 < PLAYERS_Y - 20.0);
const _: () = assert!(PLAYERS_Y + 22.0 + PLAYER_LINES as f32 * PLAYER_LINE < KEYS_Y - 20.0);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::ClientPhase;
    use crate::server_browser::{DetailPlayer, DetailsView};

    fn entry(address: &str, name: &str, map: &str, players: u16, ping: u32) -> ServerEntry {
        ServerEntry::for_test(
            address,
            name,
            map,
            players,
            32,
            ping,
            0,
            CompatProfile::BaseJka,
            false,
        )
    }

    /// A menu on the SJK UI's browser listing three servers.
    fn browsing() -> (ClientMenu, ViewerConsole, tempfile::TempDir) {
        let directory = tempfile::tempdir().expect("a profile");
        let console = ViewerConsole::new(directory.path().join("config.cfg")).expect("a console");
        let mut menu = ClientMenu::new(true, String::new());
        menu.menu_style = crate::menu::style::MenuStyle::Sjk;
        menu.browser.list_for_test(
            vec![
                entry("192.0.2.1:29070", "^1J^7o^1F", "mp/ffa3", 12, 40),
                entry("192.0.2.2:29070", "Quiet duels", "mp/duel6", 0, 90),
                entry("192.0.2.3:29070", "Far away", "mp/ffa5", 3, 250),
            ],
            &[],
        );
        menu.open_browser();
        (menu, console, directory)
    }

    #[test]
    fn the_browser_tokens_are_all_different() {
        // The add-favourite button once shared the server table's token, so a click
        // on the table's empty rows opened its prompt.
        let tokens = [
            BACK_TOKEN,
            FILTER_TOKEN,
            JOIN_TOKEN,
            FAVOURITE_TOKEN,
            REFRESH_TOKEN,
            ADDRESS_TOKEN,
            ADD_FAVOURITE_TOKEN,
            TABLE_TOKEN,
            SCROLLBAR_TOKEN,
        ];
        for (at, token) in tokens.iter().enumerate() {
            assert!(!tokens[at + 1..].contains(token), "token {token}");
        }
    }

    #[test]
    fn header_tokens_answer_to_the_shared_sort_columns() {
        for column in COLUMNS {
            let index = usize::from(header_token(column.sort) - HEADER_TOKEN);
            assert_eq!(crate::menu::pointer::SORT_COLUMNS[index], column.sort);
        }
    }

    #[test]
    fn the_left_column_answers_to_the_tabs_and_the_filter_strip() {
        assert_eq!(Side::of_token(TAB_BASE), Some(0));
        assert_eq!(Side::of_token(TAB_BASE + 1), Some(1));
        // The filter strip's order: empty, full, locked, valid info, mode.
        assert_eq!(Side::Empty.token(), FILTER_BASE);
        assert_eq!(Side::Locked.token(), FILTER_BASE + 2);
        assert_eq!(Side::Mode.token(), FILTER_BASE + 4);
        assert_eq!(Side::of_token(FILTER_BASE + 3), None);
        assert_eq!(Side::of_token(JOIN_TOKEN), None);
    }

    #[test]
    fn pings_light_fewer_bars_as_they_grow() {
        assert_eq!(signal_bars(25), 4);
        assert_eq!(signal_bars(90), 3);
        assert_eq!(signal_bars(150), 2);
        assert_eq!(signal_bars(300), 1);
        assert_eq!(signal_bars(999), 0);
        assert_eq!(short_map("mp/ffa3"), "ffa3");
        assert_eq!(short_map("academy1"), "academy1");
    }

    #[test]
    fn wrapped_names_go_on_in_their_colour() {
        // A wrapped name's second line goes on in the colour the first ended in.
        assert_eq!(crate::text::last_colour("^1Red ^4Blue x"), Some("^4"));
        assert_eq!(crate::text::last_colour("plain"), None);
        assert_eq!(crate::text::last_colour("end^"), None);
        // The search and the prompt read names without them.
        assert_eq!(Plain("^1J^7o^1F").to_string(), "JoF");
        assert_eq!(Plain("100^% ^^7x").to_string(), "100^% ^x");
    }

    #[test]
    fn limits_read_in_words() {
        let fact = |label, value: &str| Fact {
            label,
            value: value.to_owned(),
        };
        let facts = [
            fact("Version", "JAmp: v1.0.1.0"),
            fact("Frag limit", "20"),
            fact("Capture limit", "0"),
            fact("Time limit", "15"),
        ];
        assert_eq!(limits(&facts).as_deref(), Some("20 frags, 15 minutes"));
        assert_eq!(limits(&facts[..1]), None);
    }

    /// Sol's request (11/10/2026): a favourite added by its address is kept, the
    /// favourites show, the prompt closes; the same address again, or a bad one, is
    /// refused in the prompt; the prompt says Add, not Connect.
    #[test]
    fn a_favourite_is_added_by_its_address() {
        let (mut menu, _console, profile) = browsing();
        let stored = profile.path().join("favorites.json");
        menu.browser.keep_favourites_for_test(stored.clone());
        menu.open_favourite_entry();
        menu.build_sjk_browser([1920.0, 1080.0]);
        let texts: Vec<String> = menu
            .ui
            .draw_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, .. } => Some(menu.ui.stored_text(*text).to_owned()),
                _ => None,
            })
            .collect();
        assert!(
            texts.iter().any(|text| text == "Add a favourite"),
            "{texts:?}"
        );
        assert!(texts.iter().any(|text| text == "Add"), "{texts:?}");
        assert!(!texts.iter().any(|text| text == "Connect"), "{texts:?}");
        menu.address_input = "not an address::".to_owned();
        assert_eq!(menu.submit_address(), MenuAction::None);
        assert!(menu.address_editing && !menu.address_error.is_empty());
        let private: std::net::SocketAddr = "198.51.100.7:29071".parse().unwrap();
        menu.address_input = "198.51.100.7:29071".to_owned();
        assert_eq!(menu.submit_address(), MenuAction::None);
        assert!(!menu.address_editing, "{}", menu.address_error);
        assert!(menu.browser.is_favorite(private));
        assert!(menu.browser.favorites_only());
        assert!(
            std::fs::read_to_string(&stored)
                .unwrap()
                .contains("198.51.100.7:29071")
        );
        // Again: refused, and the prompt stays.
        menu.open_favourite_entry();
        menu.address_input = "198.51.100.7:29071".to_owned();
        let _ = menu.submit_address();
        assert!(menu.address_editing);
        assert_eq!(menu.address_error, "That server is already a favourite");
        // Joining by address still connects.
        menu.open_address_entry();
        menu.address_input = "192.0.2.9:29070".to_owned();
        assert!(matches!(menu.submit_address(), MenuAction::Connect(_)));
    }

    /// Sol's request (11/10/2026): a locked server's password can be remembered on
    /// this PC; the prompt then opens with it typed, and switching Remember off
    /// forgets it when the player joins.
    #[test]
    fn a_locked_servers_password_is_remembered_on_this_pc() {
        let (mut menu, mut console, profile) = browsing();
        let kept = profile.path().join("server-passwords.json");
        menu.server_passwords =
            crate::server_passwords::ServerPasswords::load_from(Some(kept.clone()));
        let temple = "192.0.2.1:29070";
        menu.open_password_prompt(temple.to_owned());
        assert!(menu.password.is_empty() && menu.remember_password);
        menu.build_sjk_browser([1920.0, 1080.0]);
        let switch = menu
            .ui
            .rect_for(REMEMBER_TOKEN)
            .expect("Remember on this PC");
        let join = menu.ui.rect_for(PASSWORD_TOKENS[3]).expect("Join");
        assert!(switch.bottom() <= join.y, "{switch:?} {join:?}");
        menu.password = "temple".to_owned();
        assert!(matches!(
            menu.handle_browser_pointer(
                PASSWORD_TOKENS[3],
                sjk_ui::UiEventKind::Activate,
                None,
                None,
                &mut console
            ),
            MenuAction::Connect(_)
        ));
        assert_eq!(console.text_value("password"), Some("temple"));
        let reloaded = crate::server_passwords::ServerPasswords::load_from(Some(kept.clone()));
        assert_eq!(reloaded.get(temple), Some("temple"));
        // Next time: typed already. Switched off and joined: forgotten.
        menu.open_password_prompt(temple.to_owned());
        assert_eq!(menu.password, "temple");
        let _ = menu.handle_browser_pointer(
            REMEMBER_TOKEN,
            sjk_ui::UiEventKind::Activate,
            None,
            None,
            &mut console,
        );
        assert!(!menu.remember_password);
        let _ = menu.handle_browser_pointer(
            PASSWORD_TOKENS[3],
            sjk_ui::UiEventKind::Activate,
            None,
            None,
            &mut console,
        );
        let reloaded = crate::server_passwords::ServerPasswords::load_from(Some(kept));
        assert_eq!(reloaded.get(temple), None);
    }

    #[test]
    fn the_screen_draws_every_server_on_show_and_its_controls() {
        let (mut menu, _console, _profile) = browsing();
        assert!(matches!(menu.state.phase(), ClientPhase::Browser));
        assert!(menu.sjk_screen());
        menu.build_sjk_browser([1920.0, 1080.0]);
        let tokens = |menu: &ClientMenu, token| menu.ui.rect_for(token).is_some();
        for row in 0..3 {
            assert!(tokens(&menu, ROW_TOKEN + row), "row {row}");
        }
        assert!(!tokens(&menu, ROW_TOKEN + 3));
        for token in [
            BACK_TOKEN,
            FILTER_TOKEN,
            JOIN_TOKEN,
            FAVOURITE_TOKEN,
            REFRESH_TOKEN,
            ADDRESS_TOKEN,
            ADD_FAVOURITE_TOKEN,
            TABLE_TOKEN,
        ]
        .into_iter()
        .chain(SIDE.map(Side::token))
        .chain(COLUMNS.map(|column| header_token(column.sort)))
        {
            assert!(tokens(&menu, token), "token {token}");
        }
        // Three servers fit: no scrollbar.
        assert!(!tokens(&menu, SCROLLBAR_TOKEN));
        assert_eq!(menu.browser.page(), VISIBLE);
        // Every text run fitted the canvas.
        assert!(!menu.ui.overflowed());
    }

    #[test]
    fn the_wheel_scrolls_a_long_player_list() {
        let (mut menu, mut console, _profile) = browsing();
        let players = (0..10)
            .map(|index| DetailPlayer {
                score: index,
                ping: 50,
                name: format!("Player {index}"),
            })
            .collect();
        menu.browser.answer_for_test(DetailsView {
            address: "192.0.2.1:29070".parse().unwrap(),
            hostname: "JoF".to_owned(),
            map: "mp/ffa3".to_owned(),
            facts: Vec::new(),
            players,
            bots: 0,
        });
        let shown = |menu: &mut ClientMenu| {
            menu.build_sjk_browser([1920.0, 1080.0]);
            (0..10)
                .filter(|index| {
                    let name = format!("Player {index}");
                    menu.ui.text_runs().any(|text| text == name)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(shown(&mut menu), [0, 1, 2, 3, 4, 5]);
        assert!(menu.ui.rect_for(PLAYERS_TOKEN).is_some());
        let wheel = |menu: &mut ClientMenu, console: &mut ViewerConsole, y: f32| {
            menu.handle_browser_pointer(
                PLAYERS_TOKEN,
                sjk_ui::UiEventKind::Wheel,
                None,
                Some(sjk_ui::Vec2 { x: 0.0, y }),
                console,
            );
        };
        // Down two rows a notch, no further than the last row at the bottom.
        wheel(&mut menu, &mut console, -1.0);
        assert_eq!(shown(&mut menu), [2, 3, 4, 5, 6, 7]);
        wheel(&mut menu, &mut console, -1.0);
        wheel(&mut menu, &mut console, -1.0);
        assert_eq!(shown(&mut menu), [4, 5, 6, 7, 8, 9]);
        // The server table stays where it was.
        assert_eq!(menu.browser.selected(), 0);
        wheel(&mut menu, &mut console, 1.0);
        assert_eq!(shown(&mut menu), [2, 3, 4, 5, 6, 7]);
        assert!(!menu.ui.overflowed());
    }

    #[test]
    fn a_full_list_scrolls_and_still_fits_the_canvas() {
        let (mut menu, _console, _profile) = browsing();
        let many = (0..40)
            .map(|index| {
                let mut row = entry(
                    &format!("192.0.2.{}:29070", index + 10),
                    "^5A rather long server name ^7| ^3with colours",
                    "mp/siege_desert",
                    7,
                    60 + index,
                );
                row.profile = CompatProfile::TaystJk;
                row.password = index % 3 == 0;
                row
            })
            .collect();
        menu.browser.list_for_test(many, &[]);
        menu.build_sjk_browser([3840.0, 2160.0]);
        assert!(menu.ui.rect_for(SCROLLBAR_TOKEN).is_some());
        assert!(menu.ui.rect_for(ROW_TOKEN + VISIBLE as u16 - 1).is_some());
        assert!(menu.ui.rect_for(ROW_TOKEN + VISIBLE as u16).is_none());
        assert!(!menu.ui.overflowed());
    }

    #[test]
    fn the_keyboard_reaches_the_left_column_and_comes_back() {
        let (mut menu, mut console, _profile) = browsing();
        // Left: the source on show.
        assert_eq!(
            menu.sjk_browser_key(KeyCode::ArrowLeft, &mut console),
            Some(MenuAction::None)
        );
        assert_eq!(menu.sjk_browser.side, Some(0));
        // Down to Favourites, Enter shows them.
        menu.sjk_browser_key(KeyCode::ArrowDown, &mut console);
        menu.sjk_browser_key(KeyCode::Enter, &mut console);
        assert!(menu.browser.favorites_only());
        // Up from the first item wraps to the game type, which Left and Right
        // step.
        menu.sjk_browser_key(KeyCode::ArrowUp, &mut console);
        menu.sjk_browser_key(KeyCode::ArrowUp, &mut console);
        assert_eq!(menu.sjk_browser.side, Some(5));
        menu.sjk_browser_key(KeyCode::ArrowLeft, &mut console);
        assert_eq!(menu.browser.filters().mode, 9);
        menu.sjk_browser_key(KeyCode::ArrowRight, &mut console);
        menu.sjk_browser_key(KeyCode::ArrowRight, &mut console);
        assert_eq!(menu.browser.filters().mode, 0);
        // Escape hands the keyboard back to the list; its keys are shared.
        menu.sjk_browser_key(KeyCode::Escape, &mut console);
        assert_eq!(menu.sjk_browser.side, None);
        assert_eq!(menu.sjk_browser_key(KeyCode::ArrowDown, &mut console), None);
    }

    #[test]
    fn a_switch_flips_its_filter_from_the_keyboard() {
        let (mut menu, mut console, _profile) = browsing();
        menu.sjk_browser.side = Some(2);
        assert!(menu.browser.filters().empty);
        menu.sjk_browser_key(KeyCode::Enter, &mut console);
        assert!(!menu.browser.filters().empty);
        assert_eq!(console.integer_cvar("ui_browserShowEmpty"), Some(0));
        // The empty server is hidden.
        assert_eq!(menu.browser.visible_len(), 2);
    }

    #[test]
    fn the_search_takes_enter_and_escape_clears_it() {
        let (mut menu, mut console, _profile) = browsing();
        menu.filter_editing = true;
        // Names match without their colour codes.
        menu.browser.push_filter("jof");
        assert_eq!(menu.browser.visible_len(), 1);
        // Enter ends the typing and keeps the search; it does not join.
        assert_eq!(
            menu.sjk_browser_key(KeyCode::Enter, &mut console),
            Some(MenuAction::None)
        );
        assert!(!menu.filter_editing);
        assert_eq!(menu.browser.filter_text(), "jof");
        // Escape then clears the search before it leaves the screen.
        assert_eq!(
            menu.sjk_browser_key(KeyCode::Escape, &mut console),
            Some(MenuAction::None)
        );
        assert_eq!(menu.browser.filter_text(), "");
        assert_eq!(menu.browser.visible_len(), 3);
        assert_eq!(menu.sjk_browser_key(KeyCode::Escape, &mut console), None);
    }

    #[test]
    fn a_prompt_leaves_the_list_out() {
        let (mut menu, _console, _profile) = browsing();
        menu.password_target = Some("192.0.2.1:29070".to_owned());
        menu.build_sjk_browser([1920.0, 1080.0]);
        for token in PASSWORD_TOKENS {
            assert!(menu.ui.rect_for(token).is_some(), "token {token}");
        }
        assert!(menu.ui.rect_for(ROW_TOKEN).is_none());
        assert!(menu.ui.rect_for(FILTER_TOKEN).is_none());
    }
}
