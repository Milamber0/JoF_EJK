//! The classic server browser: retail's `ui/jamp/joinserver.menu` laid out
//! on the 640x480 canvas over the same browser model as the SJK UI's.
//!
//! Every control answers to the browser's pointer tokens (rows, sort
//! headers, scrollbar, refresh, favourite, filter, join, back, the filter
//! toggles and the password and address prompts), so the keyboard, pointer
//! and wheel handling in [`crate::menu`] serve both styles. Retail's items
//! map onto SJK's browser as follows:
//!
//! - GET NEW LIST and REFRESH LIST fetch the master list again (retail's
//!   `RefreshServers` behind both);
//! - the source selector switches between every server and the favourites;
//! - retail's mod filter row is SJK's text filter over names and maps;
//! - TYPE cycles the game-type filter, VIEW EMPTY / VIEW FULL / VIEW LOCKED
//!   toggle the archived `ui_browserShow*` cvars (VIEW LOCKED takes the place
//!   of retail's data-rate selector);
//! - SERVER INFO opens a pop-up of the selected server's status;
//! - CONNECT IP stands where retail's NEW FAVORITE was and opens SJK's
//!   direct connect;
//! - PASSWORD and FIND PLAYER are shown dimmed: SJK asks for the password
//!   when a locked server is joined and has no player search yet.

use super::layout::{CANVAS, Placement};
use super::view::{self, DISABLED, FOCUS, GOLD, HINT};
use crate::menu::art::{ArtPiece, ArtSet};
use crate::menu::browser_table::{HEADER_TOKEN, ROW_TOKEN, SCROLLBAR_TOKEN, TABLE_TOKEN};
use crate::menu::browser_view::{
    ADDRESS_TOKEN, BACK_TOKEN, FAVOURITE_TOKEN, FILTER_TOKEN, JOIN_TOKEN, REFRESH_TOKEN,
};
use crate::menu_widgets::{MenuCanvas, TAB_BASE};
use crate::server_browser::{DetailsState, ServerBrowser, SortColumn, gametype_name};
use sjk_client::CompatProfile;
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};

/// SERVER INFO: opens the status pop-up.
pub(crate) const INFO_TOKEN: u16 = 17;
/// The pop-up's DONE button and its backdrop.
pub(crate) const INFO_CLOSE_TOKEN: u16 = 18;
/// EXIT: retail's quit page.
pub(crate) const EXIT_TOKEN: u16 = 19;
/// REFRESH LIST, the second refresh button.
pub(crate) const REFRESH_LIST_TOKEN: u16 = 21;
/// The dimmed PASSWORD and FIND PLAYER buttons, for their descriptions.
const PASSWORD_TOKEN: u16 = 23;
const FIND_PLAYER_TOKEN: u16 = 24;
/// The filter toggles of [`crate::menu::browser_filters`]: empty, full,
/// locked, valid info, game type.
const FILTER_BASE: u16 = crate::menu::browser_filters::BASE;

/// Retail option colour of the selectors (`forecolor .615 .615 .956`).
const SELECTOR: Color = Color::new(0.615, 0.615, 0.956, 1.0);
/// Retail page-title colour (`forecolor .549 .854 1`).
const TITLE: Color = Color::new(0.549, 0.854, 1.0, 1.0);
/// Retail box fill and border (`backcolor 0 0 .6 .5`, `bordercolor 0 0 .6 1`).
const BOX_FILL: Color = Color::new(0.0, 0.0, 0.6, 0.5);
const BOX_BORDER: Color = Color::new(0.0, 0.0, 0.6, 1.0);
/// Retail listbox fill (`backcolor .25 .25 .8 .25`) and border.
const LIST_FILL: Color = Color::new(0.25, 0.25, 0.8, 0.25);
const COLUMN_BORDER: Color = Color::new(0.2, 0.2, 0.5, 0.5);
/// A hovered sort header's column border (`bordercolor .79 .64 .22 1`).
const COLUMN_HOVER: Color = Color::new(0.79, 0.64, 0.22, 1.0);
/// The sorted column's fill (`backcolor 0.1 0.1 0.5 0.5`).
const SORTED_FILL: Color = Color::new(0.1, 0.1, 0.5, 0.5);
/// The alternating row bands (`horizontalseparators`).
const BANDS: [Color; 2] = [
    Color::new(0.1, 0.1, 0.3, 0.5),
    Color::new(0.0, 0.0, 0.2, 0.5),
];
/// The selected row (`outlinecolor 1 1 1 .25`).
const SELECTED: Color = Color::new(1.0, 1.0, 1.0, 0.25);
/// The refresh line under the list (`forecolor .79 .64 .22 .7`).
const REFRESH_LINE: Color = Color::new(0.79, 0.64, 0.22, 0.7);

/// The listbox: `rect 10 112 620 264`, rows `elementheight 26`.
pub(crate) const LIST: [f32; 4] = [10.0, 112.0, 620.0, 264.0];
pub(crate) const ROW_HEIGHT: f32 = 26.0;
/// Rows the listbox shows.
pub(crate) const VISIBLE_ROWS: usize = 10;
/// Retail `SCROLLBAR_SIZE`: the bar along the list's right edge.
const SCROLLBAR_WIDTH: f32 = 16.0;

/// One list column: its sort header and its listbox column.
struct Column {
    label: &'static str,
    hint: &'static str,
    sort: SortColumn,
    /// The header button (`rect`).
    header: [f32; 4],
    /// The column frame under it (`serverColumn` and the others).
    frame: [f32; 4],
    /// Text start and width inside a row (`columns`, from the list's x).
    text: [f32; 2],
    align: TextAlign,
}

/// The five columns in retail order. The sort tokens follow the pointer
/// handling's header order (name, map, players, ping, type).
const COLUMNS: [Column; 5] = [
    Column {
        label: "SERVER NAME",
        hint: "Sort by server name.",
        sort: SortColumn::Name,
        header: [10.0, 88.0, 265.0, 26.0],
        frame: [10.0, 112.0, 265.0, 264.0],
        text: [2.0, 258.0],
        align: TextAlign::Start,
    },
    Column {
        label: "MAP NAME",
        hint: "Sort by map name.",
        sort: SortColumn::Map,
        header: [275.0, 88.0, 125.0, 26.0],
        frame: [275.0, 112.0, 125.0, 264.0],
        text: [270.0, 104.0],
        align: TextAlign::Start,
    },
    Column {
        label: "PLYRS",
        hint: "Sort by number of players.",
        sort: SortColumn::Players,
        header: [400.0, 88.0, 60.0, 26.0],
        frame: [400.0, 112.0, 60.0, 264.0],
        text: [393.0, 54.0],
        align: TextAlign::Start,
    },
    Column {
        label: "TYPE",
        hint: "Sort by game type.",
        sort: SortColumn::Gametype,
        header: [460.0, 88.0, 100.0, 26.0],
        frame: [460.0, 112.0, 100.0, 264.0],
        text: [453.0, 96.0],
        align: TextAlign::Start,
    },
    Column {
        label: "PING",
        hint: "Sort by ping time.",
        sort: SortColumn::Ping,
        header: [560.0, 88.0, 52.0, 26.0],
        frame: [560.0, 112.0, 52.0, 264.0],
        text: [553.0, 47.0],
        align: TextAlign::Start,
    },
];

/// The header token the pointer handling sorts `column` by.
fn sort_token(column: SortColumn) -> u16 {
    HEADER_TOKEN
        + match column {
            SortColumn::Name => 0,
            SortColumn::Map => 1,
            SortColumn::Players => 2,
            SortColumn::Ping => 3,
            SortColumn::Gametype => 4,
        }
}

/// A button of the screen: label, description, canvas rectangle, token,
/// text size and the glow retail shows while it is hovered.
struct Button {
    label: &'static str,
    hint: &'static str,
    rect: [f32; 4],
    token: u16,
    size: f32,
    glow: Glow,
    align: TextAlign,
}

/// Retail's two hover glows.
#[derive(Clone, Copy)]
enum Glow {
    /// `button_glow2` (`menu_blendbox_extended`) at its own rectangle.
    Band([f32; 4]),
    /// `button_glow` (`menu_buttonback`) at its own rectangle.
    Back([f32; 4]),
}

/// GET NEW LIST and REFRESH LIST, top left.
const TOP_BUTTONS: [Button; 2] = [
    Button {
        label: "GET NEW LIST",
        hint: "Get updated Server List.",
        rect: [15.0, 26.0, 180.0, 26.0],
        token: REFRESH_TOKEN,
        size: 15.0,
        glow: Glow::Band([10.0, 27.0, 220.0, 26.0]),
        align: TextAlign::Start,
    },
    Button {
        label: "REFRESH LIST",
        hint: "Refresh Server List.",
        rect: [15.0, 54.0, 180.0, 26.0],
        token: REFRESH_LIST_TOKEN,
        size: 15.0,
        glow: Glow::Band([10.0, 55.0, 220.0, 26.0]),
        align: TextAlign::Start,
    },
];

/// The row of secondary buttons on `secondary_background`.
fn secondary_buttons(favourite: bool) -> [Button; 5] {
    let button = |label, hint, x: f32, token| Button {
        label,
        hint,
        rect: [x, 402.0, 120.0, 20.0],
        token,
        size: 11.0,
        glow: Glow::Back([x - 3.0, 400.0, 150.0, 20.0]),
        align: TextAlign::Center,
    };
    [
        button(
            "PASSWORD",
            "SJK asks for the password when you join a locked server.",
            10.0,
            PASSWORD_TOKEN,
        ),
        button(
            "CONNECT IP",
            "Connect to a server by its address.",
            135.0,
            ADDRESS_TOKEN,
        ),
        if favourite {
            button(
                "DEL. FAVORITE",
                "Delete selected server from favorites.",
                260.0,
                FAVOURITE_TOKEN,
            )
        } else {
            button(
                "ADD FAVORITE",
                "Add selected server to favorites.",
                260.0,
                FAVOURITE_TOKEN,
            )
        },
        button(
            "SERVER INFO",
            "Display server information.",
            385.0,
            INFO_TOKEN,
        ),
        button(
            "FIND PLAYER",
            "Searching for players is not available in SJK yet.",
            510.0,
            FIND_PLAYER_TOKEN,
        ),
    ]
}

/// BACK, EXIT and JOIN along the bottom; EXIT only on the main menu.
fn bottom_buttons(from_game: bool) -> impl Iterator<Item = Button> {
    let button = |label, hint, rect: [f32; 4], token, glow| Button {
        label,
        hint,
        rect,
        token,
        size: 17.0,
        glow: Glow::Back(glow),
        align: TextAlign::Center,
    };
    [
        Some(button(
            "BACK",
            "Back up one menu.",
            [59.0, 444.0, 130.0, 24.0],
            BACK_TOKEN,
            [30.0, 441.0, 190.0, 30.0],
        )),
        (!from_game).then(|| {
            button(
                "EXIT",
                "Leave Jedi Academy Multiplayer game.",
                [255.0, 444.0, 130.0, 24.0],
                EXIT_TOKEN,
                [235.0, 441.0, 190.0, 30.0],
            )
        }),
        Some(button(
            "JOIN",
            "Join chosen server.",
            [440.0, 444.0, 160.0, 24.0],
            JOIN_TOKEN,
            [425.0, 441.0, 190.0, 30.0],
        )),
    ]
    .into_iter()
    .flatten()
}

/// The prompt shown over the screen, if any.
#[derive(Clone, Copy)]
pub(crate) enum Prompt<'a> {
    None,
    /// The password of a locked server being joined, `length` characters
    /// typed so far.
    Password {
        length: usize,
    },
    /// Direct connect: the typed address and the last parse error.
    Address {
        input: &'a str,
        error: &'a str,
    },
}

/// What the screen shows beside the browser model.
pub(crate) struct Screen<'a> {
    /// The status line (fetch progress or the result).
    pub(crate) status: &'a str,
    pub(crate) art: ArtSet,
    pub(crate) reveal: f32,
    /// The filter row takes typed text.
    pub(crate) filter_editing: bool,
    /// The SERVER INFO pop-up is open.
    pub(crate) info_open: bool,
    /// Opened from the game menu: no EXIT button.
    pub(crate) from_game: bool,
    pub(crate) prompt: Prompt<'a>,
}

/// Build the screen into `canvas`. The caller finishes the canvas with the
/// keyboard focus token.
pub(crate) fn build(
    canvas: &mut MenuCanvas,
    viewport: [f32; 2],
    browser: &mut ServerBrowser,
    screen: &Screen<'_>,
) {
    let place = Placement::new(viewport);
    canvas.begin_transparent(viewport);
    canvas.push_opacity(screen.reveal);
    backdrop(canvas, viewport, &place, screen.art);
    framed_box(canvas, place.rect([240.0, 24.0, 384.0, 60.0]), place.scale);
    framed_box(canvas, place.rect([5.0, 398.0, 630.0, 22.0]), place.scale);
    title(canvas, &place, screen.art);

    let mut hint: Option<&'static str> = None;
    for button in &TOP_BUTTONS {
        button_item(canvas, &place, screen.art, button, true, &mut hint);
    }
    selectors(canvas, &place, screen, browser, &mut hint);
    list(canvas, &place, screen.art, browser, &mut hint);
    let favourite = browser
        .visible_entry(browser.selected())
        .is_some_and(|entry| browser.is_favorite(entry.address));
    for button in &secondary_buttons(favourite) {
        let enabled = !matches!(button.token, PASSWORD_TOKEN | FIND_PLAYER_TOKEN);
        button_item(canvas, &place, screen.art, button, enabled, &mut hint);
    }
    for button in bottom_buttons(screen.from_game) {
        button_item(canvas, &place, screen.art, &button, true, &mut hint);
    }
    // The refresh line under the list (`UI_SERVERREFRESHDATE`).
    let s = place.scale;
    canvas.text_aligned(
        screen.status,
        place.rect([10.0, 378.0, 400.0, 16.0]),
        11.0 * s,
        REFRESH_LINE,
        FontWeight::Regular,
        0.3 * s,
        TextAlign::Start,
    );
    if let Some(hint) = hint {
        canvas.text_aligned(
            hint,
            place.centered([CANVAS[0] * 0.5, 432.0], 600.0, 16.0),
            12.0 * s,
            HINT,
            FontWeight::Regular,
            0.3 * s,
            TextAlign::Center,
        );
    }
    if screen.info_open {
        info_popup(canvas, viewport, &place, browser.details());
    }
    match screen.prompt {
        Prompt::None => {}
        Prompt::Password { length } => password_prompt(canvas, viewport, &place, length),
        Prompt::Address { input, error } => {
            address_prompt(canvas, viewport, &place, input, error);
        }
    }
    canvas.pop_opacity();
}

/// Retail's backdrop, opaque: `main_centerblue`, the glyph columns and
/// `main_background`, or SJK's dark ink where the art is missing.
fn backdrop(canvas: &mut MenuCanvas, viewport: [f32; 2], place: &Placement, art: ArtSet) {
    let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
        rect: Rect::new(0.0, 0.0, viewport[0], viewport[1]),
        color: view::ink(1.0),
    });
    for (piece, rect) in [
        (ArtPiece::CenterBlue, [156.0, 154.0, 320.0, 240.0]),
        (ArtPiece::SideLeft, [0.0, 0.0, 160.0, 480.0]),
        (ArtPiece::SideRight, [480.0, 0.0, 160.0, 480.0]),
        (ArtPiece::Background, [0.0, 0.0, 640.0, 480.0]),
    ] {
        if art.has(piece) {
            view::art(canvas, piece, place.rect(rect));
        }
    }
}

/// A retail `WINDOW_STYLE_FILLED` box with its one-unit border.
fn framed_box(canvas: &mut MenuCanvas, rect: Rect, scale: f32) {
    filled(canvas, rect, BOX_FILL);
    border(canvas, rect, scale, BOX_BORDER);
}

fn filled(canvas: &mut MenuCanvas, rect: Rect, color: Color) {
    let _ = canvas
        .draw_list_mut()
        .push(DrawCommand::SolidRect { rect, color });
}

fn border(canvas: &mut MenuCanvas, rect: Rect, scale: f32, color: Color) {
    let line = scale.max(1.0);
    for edge in [
        Rect::new(rect.x, rect.y, rect.width, line),
        Rect::new(rect.x, rect.bottom() - line, rect.width, line),
        Rect::new(rect.x, rect.y, line, rect.height),
        Rect::new(rect.right() - line, rect.y, line, rect.height),
    ] {
        filled(canvas, edge, color);
    }
}

/// `join_title`: the page title over its `menu_blendbox` band.
fn title(canvas: &mut MenuCanvas, place: &Placement, art: ArtSet) {
    let band = place.rect([50.0, 4.0, 540.0, 16.0]);
    if art.has(ArtPiece::BlendBox) {
        view::art(canvas, ArtPiece::BlendBox, band);
    } else {
        view::soft_band(canvas, band, 0.16);
    }
    canvas.text_aligned(
        "JOIN A GAME IN PROGRESS",
        place.rect([50.0, 4.0, 540.0, 16.0]),
        11.5 * place.scale,
        TITLE,
        FontWeight::Semibold,
        3.0 * place.scale,
        TextAlign::Center,
    );
}

/// A hover glow at canvas rectangle `glow`.
fn draw_glow(canvas: &mut MenuCanvas, place: &Placement, art: ArtSet, glow: Glow) {
    match glow {
        Glow::Band(rect) => {
            let rect = place.rect(rect);
            if art.has(ArtPiece::BlendBox) {
                view::art(canvas, ArtPiece::BlendBox, rect);
            } else {
                view::soft_band(canvas, rect, 0.22);
            }
        }
        Glow::Back(rect) => view::glow(canvas, place.rect(rect), place.scale, art),
    }
}

/// One button: its glow while hovered, the label (gold, white while
/// hovered, grey when dimmed) and its pointer target.
fn button_item(
    canvas: &mut MenuCanvas,
    place: &Placement,
    art: ArtSet,
    button: &Button,
    enabled: bool,
    hint: &mut Option<&'static str>,
) {
    let target = place.rect(button.rect);
    let hovered = canvas.token_hovered(button.token);
    if hovered {
        *hint = Some(button.hint);
        if enabled {
            draw_glow(canvas, place, art, button.glow);
        }
    }
    let color = match (enabled, hovered) {
        (false, _) => DISABLED,
        (true, true) => FOCUS,
        (true, false) => GOLD,
    };
    let [x, y, width, height] = button.rect;
    let line = button.size * 1.25;
    let inset = if button.align == TextAlign::Start {
        6.0
    } else {
        0.0
    };
    canvas.text_aligned(
        button.label,
        place.rect([x + inset, y + (height - line) * 0.5, width - inset, line]),
        button.size * place.scale,
        color,
        FontWeight::Semibold,
        1.2 * place.scale,
        button.align,
    );
    canvas.hit_region(button.token, target);
}

/// The selectors in the upper-right box: source, filter and game type on
/// the left, the VIEW toggles on the right.
fn selectors(
    canvas: &mut MenuCanvas,
    place: &Placement,
    screen: &Screen<'_>,
    browser: &ServerBrowser,
    hint: &mut Option<&'static str>,
) {
    let filters = browser.filters();
    let favourites = browser.favorites_only();
    // Clicking the source selector picks the other source.
    let source_token = TAB_BASE + u16::from(!favourites);
    let yes_no = |value: bool| if value { "YES" } else { "NO" };
    let rows: [(u16, &'static str, [f32; 4], [f32; 4]); 6] = [
        (
            source_token,
            "Choose source of servers.",
            [250.0, 26.0, 180.0, 18.0],
            [242.0, 24.0, 200.0, 20.0],
        ),
        (
            FILTER_TOKEN,
            "Type to filter servers by name or map; Escape ends the filter.",
            [250.0, 44.0, 180.0, 18.0],
            [243.0, 42.0, 200.0, 20.0],
        ),
        (
            FILTER_BASE + 4,
            "Set filter for specific game types.",
            [250.0, 62.0, 180.0, 18.0],
            [242.0, 60.0, 200.0, 20.0],
        ),
        (
            FILTER_BASE,
            "Include empty servers in list.",
            [430.0, 26.0, 180.0, 18.0],
            [422.0, 24.0, 200.0, 20.0],
        ),
        (
            FILTER_BASE + 1,
            "Include full servers in list.",
            [430.0, 44.0, 180.0, 18.0],
            [422.0, 42.0, 200.0, 20.0],
        ),
        (
            FILTER_BASE + 2,
            "Include password-protected servers in list.",
            [430.0, 62.0, 180.0, 18.0],
            [422.0, 60.0, 200.0, 20.0],
        ),
    ];
    let s = place.scale;
    for (index, (token, description, rect, glow)) in rows.into_iter().enumerate() {
        let hovered = canvas.token_hovered(token);
        let editing = token == FILTER_TOKEN && screen.filter_editing;
        if hovered || editing {
            draw_glow(canvas, place, screen.art, Glow::Band(glow));
        }
        if hovered {
            *hint = Some(description);
        }
        let color = if hovered || editing { FOCUS } else { SELECTOR };
        let [x, y, width, height] = rect;
        let text = place.rect([x, y + (height - 14.0) * 0.5, width, 14.0]);
        let size = 11.5 * s;
        let spacing = 0.6 * s;
        match index {
            0 => canvas.text_fmt_aligned(
                format_args!(
                    "SOURCE: {}",
                    if favourites { "FAVORITES" } else { "INTERNET" }
                ),
                text,
                size,
                color,
                FontWeight::Semibold,
                spacing,
                TextAlign::Start,
            ),
            1 => {
                let filter = browser.filter_text();
                canvas.text_fmt_aligned(
                    format_args!(
                        "FILTER: {}{}",
                        if filter.is_empty() && !editing {
                            "ALL"
                        } else {
                            filter
                        },
                        if editing { "_" } else { "" }
                    ),
                    text,
                    size,
                    color,
                    FontWeight::Semibold,
                    spacing,
                    TextAlign::Start,
                );
            }
            2 => canvas.text_fmt_aligned(
                format_args!(
                    "TYPE: {}",
                    if filters.mode < 0 {
                        "ALL"
                    } else {
                        gametype_name(Some(filters.mode))
                    }
                ),
                text,
                size,
                color,
                FontWeight::Semibold,
                spacing,
                TextAlign::Start,
            ),
            3 => canvas.text_fmt_aligned(
                format_args!("VIEW EMPTY: {}", yes_no(filters.empty)),
                text,
                size,
                color,
                FontWeight::Semibold,
                spacing,
                TextAlign::Start,
            ),
            4 => canvas.text_fmt_aligned(
                format_args!("VIEW FULL: {}", yes_no(filters.full)),
                text,
                size,
                color,
                FontWeight::Semibold,
                spacing,
                TextAlign::Start,
            ),
            _ => canvas.text_fmt_aligned(
                format_args!("VIEW LOCKED: {}", yes_no(filters.password)),
                text,
                size,
                color,
                FontWeight::Semibold,
                spacing,
                TextAlign::Start,
            ),
        }
        canvas.hit_region(token, place.rect(rect));
    }
}

/// Window rectangle of visible list row `index`.
pub(crate) fn row_rect(place: &Placement, index: usize) -> Rect {
    let [x, y, _, _] = LIST;
    place.rect([
        x,
        y + index as f32 * ROW_HEIGHT,
        LIST[2] - SCROLLBAR_WIDTH,
        ROW_HEIGHT,
    ])
}

/// The scrollbar along the list's right edge (canvas units).
pub(crate) fn scrollbar_track() -> [f32; 4] {
    let [x, y, width, height] = LIST;
    [x + width - SCROLLBAR_WIDTH, y, SCROLLBAR_WIDTH, height]
}

/// The scrollbar thumb inside `track` for a window of `visible` rows from
/// `first` over `total` rows: at least one bar wide, as retail's thumb.
pub(crate) fn thumb(track: [f32; 4], first: usize, visible: usize, total: usize) -> [f32; 4] {
    let [x, y, width, height] = track;
    let length = (height * visible as f32 / total.max(visible).max(1) as f32).max(width);
    let travel = height - length;
    let span = total.saturating_sub(visible).max(1) as f32;
    let offset = travel * (first as f32 / span).clamp(0.0, 1.0);
    [x, y + offset, width, length]
}

/// Sort headers, column frames, row bands and the server rows.
fn list(
    canvas: &mut MenuCanvas,
    place: &Placement,
    art: ArtSet,
    browser: &mut ServerBrowser,
    hint: &mut Option<&'static str>,
) {
    let s = place.scale;
    let (sort, descending) = browser.sort_state();
    // Header buttons and their column frames.
    for column in &COLUMNS {
        let token = sort_token(column.sort);
        let hovered = canvas.token_hovered(token);
        let active = column.sort == sort;
        if hovered {
            *hint = Some(column.hint);
            let [x, y, width, height] = column.header;
            draw_glow(
                canvas,
                place,
                art,
                Glow::Band([x - 3.0, y + 2.0, width + 3.0, height - 2.0]),
            );
        }
        let [x, y, width, height] = column.header;
        canvas.text_fmt_aligned(
            format_args!(
                "{}{}",
                column.label,
                match (active, descending) {
                    (false, _) => "",
                    (true, false) => " ^",
                    (true, true) => " v",
                }
            ),
            place.rect([x + 4.0, y + (height - 16.0) * 0.5, width - 4.0, 16.0]),
            12.5 * s,
            if active || hovered { FOCUS } else { GOLD },
            FontWeight::Semibold,
            0.8 * s,
            TextAlign::Start,
        );
        canvas.hit_region(token, place.rect(column.header));
    }
    let list_rect = place.rect(LIST);
    filled(canvas, list_rect, LIST_FILL);
    for band in 0..VISIBLE_ROWS {
        filled(
            canvas,
            place.rect([10.0, 116.0 + band as f32 * ROW_HEIGHT, 604.0, ROW_HEIGHT]),
            BANDS[band % 2],
        );
    }
    for column in &COLUMNS {
        let frame = place.rect(column.frame);
        if column.sort == sort {
            filled(canvas, frame, SORTED_FILL);
        }
        let hovered = canvas.token_hovered(sort_token(column.sort));
        border(
            canvas,
            frame,
            s,
            if hovered { COLUMN_HOVER } else { COLUMN_BORDER },
        );
    }
    browser.set_page(VISIBLE_ROWS);
    // The wheel target first, so rows registered after it keep their clicks.
    canvas.scroll_region(TABLE_TOKEN, list_rect);
    let first = browser.scroll();
    let count = browser
        .visible_len()
        .saturating_sub(first)
        .min(VISIBLE_ROWS);
    for visible in 0..count {
        let row = first + visible;
        let Some(entry) = browser.visible_entry(row) else {
            continue;
        };
        let rect = row_rect(place, visible);
        let token = ROW_TOKEN + row as u16;
        if row == browser.selected() {
            filled(canvas, rect, SELECTED);
        } else if canvas.token_hovered(token) {
            filled(canvas, rect, Color::new(1.0, 1.0, 1.0, 0.08));
        }
        let cell = |column: &Column| {
            let [x, width] = column.text;
            place.rect([
                LIST[0] + x,
                LIST[1] + visible as f32 * ROW_HEIGHT + 5.0,
                width,
                16.0,
            ])
        };
        let size = 12.0 * s;
        if browser.is_favorite(entry.address) {
            let [x, _, _, _] = LIST;
            canvas.text_aligned(
                "*",
                place.rect([
                    x - 7.0,
                    LIST[1] + visible as f32 * ROW_HEIGHT + 5.0,
                    8.0,
                    16.0,
                ]),
                size,
                GOLD,
                FontWeight::Semibold,
                0.0,
                TextAlign::Center,
            );
        }
        canvas.text_aligned(
            &entry.name,
            cell(&COLUMNS[0]),
            size,
            FOCUS,
            FontWeight::Regular,
            0.2 * s,
            COLUMNS[0].align,
        );
        canvas.text_aligned(
            &entry.map,
            cell(&COLUMNS[1]),
            size,
            FOCUS,
            FontWeight::Regular,
            0.2 * s,
            COLUMNS[1].align,
        );
        canvas.text_fmt_aligned(
            format_args!("{} ({})", entry.players, entry.capacity),
            cell(&COLUMNS[2]),
            size,
            FOCUS,
            FontWeight::Regular,
            0.2 * s,
            COLUMNS[2].align,
        );
        canvas.text_fmt_aligned(
            format_args!(
                "{}{}{}",
                entry.gametype,
                profile_suffix(&entry.profile),
                if entry.password { " *P" } else { "" }
            ),
            cell(&COLUMNS[3]),
            size,
            FOCUS,
            FontWeight::Regular,
            0.2 * s,
            COLUMNS[3].align,
        );
        canvas.text_fmt_aligned(
            format_args!("{}", entry.ping_millis),
            cell(&COLUMNS[4]),
            size,
            FOCUS,
            FontWeight::Regular,
            0.2 * s,
            COLUMNS[4].align,
        );
        canvas.hit_region(token, rect);
    }
    if count == 0 {
        canvas.text_aligned(
            if browser.favorites_only() {
                "NO FAVORITES YET.  ADD FAVORITE KEEPS THE SELECTED SERVER HERE."
            } else if browser.is_refreshing() {
                "WAITING FOR THE FIRST SERVER TO ANSWER..."
            } else {
                "NO SERVERS MATCH THESE FILTERS."
            },
            place.rect([LIST[0] + 4.0, LIST[1] + 5.0, 590.0, 16.0]),
            12.0 * s,
            SELECTOR,
            FontWeight::Semibold,
            0.6 * s,
            TextAlign::Start,
        );
    }
    let mut list_border = COLUMN_BORDER;
    if canvas.token_hovered(TABLE_TOKEN)
        || (0..count).any(|visible| canvas.token_hovered(ROW_TOKEN + (first + visible) as u16))
    {
        list_border = COLUMN_HOVER;
    }
    border(canvas, list_rect, s, list_border);
    if browser.visible_len() > VISIBLE_ROWS {
        let track = scrollbar_track();
        let rect = place.rect(track);
        filled(canvas, rect, Color::new(0.0, 0.0, 0.2, 0.7));
        border(canvas, rect, s, COLUMN_BORDER);
        let active = canvas.token_hovered(SCROLLBAR_TOKEN);
        let thumb_rect = place.rect(thumb(track, first, VISIBLE_ROWS, browser.visible_len()));
        filled(
            canvas,
            thumb_rect,
            if active { FOCUS } else { view::gold(0.75) },
        );
        // Registered as a scroll target, so a drag along it moves the window.
        canvas.scroll_region(SCROLLBAR_TOKEN, rect);
    }
}

/// The mod a server runs, after its game type, where SJK knows it.
fn profile_suffix(profile: &CompatProfile) -> &'static str {
    match profile {
        CompatProfile::BaseJka => "",
        CompatProfile::JaPlus { .. } => " JA+",
        CompatProfile::TaystJk => " JAPRO",
        CompatProfile::Unknown(_) => " MOD",
    }
}

/// A pop-up box over the dimmed screen, with its title band; returns the
/// window rectangle of its body. `backdrop` is the token the dimmed screen
/// answers to, so clicks outside the box do not reach the screen under it.
fn popup(
    canvas: &mut MenuCanvas,
    viewport: [f32; 2],
    place: &Placement,
    rect: [f32; 4],
    caption: &str,
    backdrop: u16,
) -> [f32; 4] {
    let screen = Rect::new(0.0, 0.0, viewport[0], viewport[1]);
    filled(canvas, screen, view::ink(0.6));
    canvas.hit_region(backdrop, screen);
    let window = place.rect(rect);
    filled(canvas, window, Color::new(0.0, 0.0, 0.12, 0.96));
    border(canvas, window, place.scale, BOX_BORDER);
    let [x, y, width, height] = rect;
    let band = place.rect([x + 10.0, y + 6.0, width - 20.0, 16.0]);
    view::soft_band(canvas, band, 0.16);
    canvas.text_aligned(
        caption,
        band,
        11.5 * place.scale,
        TITLE,
        FontWeight::Semibold,
        3.0 * place.scale,
        TextAlign::Center,
    );
    [x + 16.0, y + 30.0, width - 32.0, height - 40.0]
}

/// A gold pop-up button, white while hovered.
fn popup_button(
    canvas: &mut MenuCanvas,
    place: &Placement,
    label: &str,
    rect: [f32; 4],
    token: u16,
) {
    let target = place.rect(rect);
    let hovered = canvas.token_hovered(token);
    if hovered {
        view::glow(canvas, target, place.scale, ArtSet::default());
    }
    let [x, y, width, height] = rect;
    canvas.text_aligned(
        label,
        place.rect([x, y + (height - 18.0) * 0.5, width, 18.0]),
        14.0 * place.scale,
        if hovered { FOCUS } else { GOLD },
        FontWeight::Semibold,
        1.2 * place.scale,
        TextAlign::Center,
    );
    canvas.hit_region(token, target);
}

/// SERVER INFO: the selected server's published settings and players.
fn info_popup(
    canvas: &mut MenuCanvas,
    viewport: [f32; 2],
    place: &Placement,
    details: DetailsState<'_>,
) {
    let [x, y, width, _] = popup(
        canvas,
        viewport,
        place,
        [70.0, 60.0, 500.0, 340.0],
        "SERVER INFO",
        INFO_CLOSE_TOKEN,
    );
    let s = place.scale;
    let line = |canvas: &mut MenuCanvas, row: f32, text: &str, color: Color| {
        canvas.text_aligned(
            text,
            place.rect([x, y + row * 16.0, width, 16.0]),
            11.5 * s,
            color,
            FontWeight::Regular,
            0.3 * s,
            TextAlign::Start,
        );
    };
    match details {
        DetailsState::Nothing => line(canvas, 0.0, "NO SERVER SELECTED.", SELECTOR),
        DetailsState::Querying => line(canvas, 0.0, "ASKING THE SERVER...", SELECTOR),
        DetailsState::Failed(error) => {
            line(canvas, 0.0, "NO ANSWER TO THE STATUS QUERY.", SELECTOR);
            line(canvas, 1.0, error, DISABLED);
        }
        DetailsState::Ready(view) => {
            line(canvas, 0.0, &view.hostname, FOCUS);
            canvas.text_fmt_aligned(
                format_args!("{}    {}", view.address, view.map),
                place.rect([x, y + 16.0, width, 16.0]),
                11.0 * s,
                SELECTOR,
                FontWeight::Regular,
                0.3 * s,
                TextAlign::Start,
            );
            // Settings down the left half, players down the right.
            let half = (width - 16.0) * 0.5;
            let mut row = 2.5;
            for fact in view.facts.iter().take(14) {
                canvas.text_aligned(
                    fact.label,
                    place.rect([x, y + row * 16.0, half * 0.5, 16.0]),
                    10.5 * s,
                    SELECTOR,
                    FontWeight::Regular,
                    0.2 * s,
                    TextAlign::Start,
                );
                canvas.text_aligned(
                    &fact.value,
                    place.rect([x + half * 0.5, y + row * 16.0, half * 0.5, 16.0]),
                    10.5 * s,
                    FOCUS,
                    FontWeight::Regular,
                    0.2 * s,
                    TextAlign::Start,
                );
                row += 1.0;
            }
            let px = x + half + 16.0;
            canvas.text_fmt_aligned(
                format_args!("PLAYERS {}", view.players.len()),
                place.rect([px, y + 2.5 * 16.0, half, 16.0]),
                10.5 * s,
                GOLD,
                FontWeight::Semibold,
                0.6 * s,
                TextAlign::Start,
            );
            for (index, player) in view.players.iter().take(14).enumerate() {
                let top = y + (3.5 + index as f32) * 16.0;
                canvas.text_aligned(
                    &player.name,
                    place.rect([px, top, half - 70.0, 16.0]),
                    10.5 * s,
                    FOCUS,
                    FontWeight::Regular,
                    0.2 * s,
                    TextAlign::Start,
                );
                canvas.text_fmt_aligned(
                    format_args!("{}  {}", player.score, player.ping),
                    place.rect([px + half - 70.0, top, 70.0, 16.0]),
                    10.5 * s,
                    SELECTOR,
                    FontWeight::Regular,
                    0.2 * s,
                    TextAlign::End,
                );
            }
        }
    }
    popup_button(
        canvas,
        place,
        "DONE",
        [270.0, 370.0, 100.0, 22.0],
        INFO_CLOSE_TOKEN,
    );
}

/// A text field box: the value (or a placeholder) and the caret.
fn field(canvas: &mut MenuCanvas, place: &Placement, rect: [f32; 4], text: &str, token: u16) {
    let window = place.rect(rect);
    filled(canvas, window, Color::new(0.0, 0.0, 0.3, 0.8));
    border(canvas, window, place.scale, COLUMN_HOVER);
    let [x, y, width, height] = rect;
    canvas.text_fmt_aligned(
        format_args!("{text}_"),
        place.rect([x + 6.0, y + (height - 16.0) * 0.5, width - 12.0, 16.0]),
        12.5 * place.scale,
        FOCUS,
        FontWeight::Regular,
        0.4 * place.scale,
        TextAlign::Start,
    );
    canvas.hit_region(token, window);
}

/// The password of a locked server (retail `password_popmenu`). The
/// tokens are the SJK UI prompt's.
fn password_prompt(canvas: &mut MenuCanvas, viewport: [f32; 2], place: &Placement, length: usize) {
    let [x, y, width, _] = popup(
        canvas,
        viewport,
        place,
        [170.0, 160.0, 300.0, 130.0],
        "PASSWORD",
        33,
    );
    canvas.text_aligned(
        "This server is locked. Enter its password.",
        place.rect([x, y, width, 16.0]),
        11.0 * place.scale,
        SELECTOR,
        FontWeight::Regular,
        0.3 * place.scale,
        TextAlign::Start,
    );
    const STARS: &str = "********************************";
    let shown = &STARS[..length.min(STARS.len())];
    field(canvas, place, [x, y + 22.0, width, 22.0], shown, 30);
    popup_button(canvas, place, "CANCEL", [x, y + 56.0, 100.0, 22.0], 31);
    popup_button(
        canvas,
        place,
        "JOIN",
        [x + width - 100.0, y + 56.0, 100.0, 22.0],
        32,
    );
}

/// Direct connect by address, in SJK's own prompt's tokens.
fn address_prompt(
    canvas: &mut MenuCanvas,
    viewport: [f32; 2],
    place: &Placement,
    input: &str,
    error: &str,
) {
    let [x, y, width, _] = popup(
        canvas,
        viewport,
        place,
        [150.0, 150.0, 340.0, 150.0],
        "CONNECT IP",
        40,
    );
    canvas.text_aligned(
        "IPv4 or hostname, default port 29070.",
        place.rect([x, y, width, 16.0]),
        11.0 * place.scale,
        SELECTOR,
        FontWeight::Regular,
        0.3 * place.scale,
        TextAlign::Start,
    );
    field(canvas, place, [x, y + 22.0, width, 22.0], input, 41);
    canvas.text_aligned(
        error,
        place.rect([x, y + 48.0, width, 14.0]),
        10.5 * place.scale,
        Color::new(1.0, 0.36, 0.32, 1.0),
        FontWeight::Semibold,
        0.0,
        TextAlign::Start,
    );
    popup_button(canvas, place, "CANCEL", [x, y + 74.0, 100.0, 22.0], 42);
    popup_button(
        canvas,
        place,
        "CONNECT",
        [x + width - 100.0, y + 74.0, 100.0, 22.0],
        43,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_fill_the_listbox() {
        let place = Placement::new(CANVAS);
        let last = row_rect(&place, VISIBLE_ROWS - 1);
        assert!(last.bottom() <= LIST[1] + LIST[3] + 0.01);
        assert!(row_rect(&place, VISIBLE_ROWS).bottom() > LIST[1] + LIST[3]);
        // Rows stop short of the scrollbar.
        assert_eq!(last.right(), scrollbar_track()[0]);
    }

    #[test]
    fn columns_tile_the_header_line_inside_the_list() {
        let mut x = COLUMNS[0].header[0];
        for column in &COLUMNS {
            assert_eq!(column.header[0], x, "{}", column.label);
            assert_eq!(column.frame[0], column.header[0]);
            assert!(LIST[0] + column.text[0] >= column.frame[0] - 8.0);
            assert!(
                LIST[0] + column.text[0] + column.text[1]
                    <= column.frame[0] + column.frame[2] + 1.0,
                "{}",
                column.label
            );
            x += column.header[2];
        }
        assert!(x <= scrollbar_track()[0]);
    }

    #[test]
    fn sort_headers_answer_to_the_shared_sort_tokens() {
        // The pointer handling sorts header `token - HEADER_TOKEN` by
        // name, map, players, ping, game type.
        let expected = [
            SortColumn::Name,
            SortColumn::Map,
            SortColumn::Players,
            SortColumn::Ping,
            SortColumn::Gametype,
        ];
        for column in &COLUMNS {
            let index = usize::from(sort_token(column.sort) - HEADER_TOKEN);
            assert_eq!(expected[index], column.sort);
        }
    }

    #[test]
    fn thumb_spans_the_track_from_first_to_last_window() {
        let track = scrollbar_track();
        let top = thumb(track, 0, VISIBLE_ROWS, 40);
        let bottom = thumb(track, 30, VISIBLE_ROWS, 40);
        assert_eq!(top[1], track[1]);
        assert!((bottom[1] + bottom[3] - (track[1] + track[3])).abs() < 0.01);
        assert!((top[3] - track[3] * 0.25).abs() < 0.01);
        // A long list keeps a thumb at least one bar wide.
        assert!(thumb(track, 0, VISIBLE_ROWS, 10_000)[3] >= SCROLLBAR_WIDTH);
    }

    #[test]
    fn buttons_and_tokens_do_not_collide() {
        let mut tokens = vec![
            INFO_TOKEN,
            INFO_CLOSE_TOKEN,
            EXIT_TOKEN,
            REFRESH_LIST_TOKEN,
            PASSWORD_TOKEN,
            FIND_PLAYER_TOKEN,
            REFRESH_TOKEN,
            FAVOURITE_TOKEN,
            BACK_TOKEN,
            JOIN_TOKEN,
            SCROLLBAR_TOKEN,
            ADDRESS_TOKEN,
            TABLE_TOKEN,
            FILTER_TOKEN,
        ];
        tokens.sort_unstable();
        tokens.dedup();
        assert_eq!(tokens.len(), 14);
        // Labels are retail's, in capitals.
        for button in TOP_BUTTONS
            .iter()
            .chain(secondary_buttons(false).iter())
            .chain(bottom_buttons(false).collect::<Vec<_>>().iter())
        {
            assert_eq!(button.label, button.label.to_ascii_uppercase());
        }
        assert_eq!(bottom_buttons(true).count(), 2);
    }
}
