//! The what's-new card (`docs/sjk-ui.md`, "What's new card"): the first time the player
//! opens a game that was updated, an SJK pop-up card over the darkened screen tells what
//! the update brought, in a few short lines (the release's recap in `CHANGELOG.md`,
//! `docs/sjk.md` "Changelog"). Close (gold, Enter) puts it away for good; Full changelog
//! (C) opens the changelog page. A player who skipped releases pages through the recaps
//! of those releases, newest first, up to `changelog::DIGEST_LIMIT` (Left and Right, or
//! the arrows beside the page dots).
//!
//! Each recap line is its heading, with an icon ([`HEADING_ICONS`]), over its sentence,
//! and a command or cvar the sentence marks in backticks (`` `whatsnew` ``) is drawn in a
//! chip (`inline_code`). Clicking a chip opens Settings on the setting it names, or types
//! it on the console's input line. Under the recap, who made the release's changes and
//! the clients they follow, from the changes' credits.
//!
//! `cl_whatsNewSeen` holds the version whose recap the player closed. The card shows on
//! the main menu or over the game menu, never in the middle of play. A new installation
//! (no marker and First setup still to do) writes the marker without showing the card;
//! a local build, which no release lists, shows nothing. The `whatsnew` command shows the
//! newest recap, the unreleased one included, to preview it. The card is drawn in the SJK
//! UI's look in every menu style, and the menus under it are not drawn and take no input,
//! as under the other cards.

use crate::console::changelog::Digest;
use crate::inline_code::{self, Look, Measures, Place, Row};
use crate::menu::sjk::{Frame, TextTarget, color, key_hint, key_hint_width, kit, text};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use sjk_ui::{Color, DrawCommand, FontWeight, InputEvent, TextAlign, UiEventKind};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// The version whose recap the player closed.
pub(crate) const SEEN_CVAR: &str = "cl_whatsNewSeen";
/// Console command that shows the newest recap again.
pub(crate) const COMMAND: &str = "whatsnew";
/// Help text for completion and `cmdlist`.
pub(crate) const HELP: &str = "Show what the latest SJK update brought";

/// Close, Full changelog, the page arrows, and the chips (`CHIP_BASE + n`, in reading
/// order).
const CLOSE_TOKEN: u16 = 1;
const CHANGELOG_TOKEN: u16 = 2;
const PREVIOUS_TOKEN: u16 = 3;
const NEXT_TOKEN: u16 = 4;
const CHIP_BASE: u16 = 100;

/// The card's left edge, width and the room above and below it; it is as tall as its
/// tallest page and sits in the middle of the frame.
const CARD_X: f32 = 500.0;
const CARD_WIDTH: f32 = 920.0;
const MARGIN: f32 = 44.0;
const TEXT_X: f32 = CARD_X + MARGIN;
const TEXT_WIDTH: f32 = CARD_WIDTH - MARGIN * 2.0;
const FRAME_HEIGHT: f32 = 1080.0;
/// The headline block above the recap.
const HEAD_HEIGHT: f32 = 134.0;
/// A recap line: its icon, then its heading, over its sentence, which starts under the
/// heading; the room under it.
const ICON: f32 = 26.0;
const INDENT: f32 = ICON + 12.0;
const HEADING_HEIGHT: f32 = 30.0;
const HEADING_SIZE: f32 = 21.0;
const ROW_GAP: f32 = 16.0;
/// The sentences and their chips.
const LOOK: Look = Look {
    size: 18.0,
    line: 28.0,
    chip_size: 17.0,
    chip_pad: 7.0,
    chip_height: 25.0,
    chip_radius: 6.0,
};
/// The line naming who made the release, under the recap.
const CREDIT_LOOK: Look = Look {
    size: 16.0,
    line: 24.0,
    ..LOOK
};
const CREDIT_GAP: f32 = 6.0;
/// Introduction lines shown for a release without a recap.
const INTRO_LINES: usize = 6;
/// How tall a page's recap may grow before it points to the changelog.
const BODY_MAX: f32 = 640.0;
/// The buttons along the card's foot, this far under the tallest page.
const BUTTONS_GAP: f32 = 26.0;
const BUTTON_HEIGHT: f32 = 46.0;
const BUTTON_GAP: f32 = 14.0;
const CLOSE_WIDTH: f32 = 150.0;
const CHANGELOG_WIDTH: f32 = 230.0;
/// The page arrows and dots, at the foot's left.
const ARROW_WIDTH: f32 = 46.0;
const DOT: f32 = 9.0;
const DOT_STEP: f32 = 20.0;
/// The keys' line, as on the other screens.
const KEYS_Y: f32 = 992.0;
const KEYS_RIGHT: f32 = 1824.0;

/// The icon a recap heading wears: the first entry with a word that starts a word of
/// the heading (any case), else [`HEADING_ICON`]. Settings and quick wheel icons
/// (`settings_icons::texture`). A heading written for a new theme falls back until an
/// entry names it.
const HEADING_ICONS: &[(&[&str], &str)] = &[
    (
        &["server", "browser", "download", "network", "master"],
        "network",
    ),
    (&["scoreboard"], "scoreboard"),
    (&["hud", "frame rate", "clock", "kill feed"], "hud"),
    (&["saber", "shader", "blade", "mythical"], "saber_style"),
    (&["clan", "nameplate", "name"], "nameplates"),
    (
        &["holocron", "collection", "cosmetic", "hat", "cape", "toy"],
        "cosmetics",
    ),
    (
        &[
            "card",
            "profile",
            "identity",
            "medal",
            "team",
            "staff",
            "achievement",
        ],
        "identity",
    ),
    (&["update", "changelog", "release"], "update"),
    (&["weather", "rain", "snow", "fog", "cloud"], "weather"),
    (
        &["graphic", "render", "light", "shadow", "video", "effect"],
        "graphics",
    ),
    (&["sound", "audio", "music", "voice"], "sound"),
    (&["gameplay", "fps", "physics"], "gameplay"),
    (&["movement", "jetpack", "grapple", "strafe"], "movement"),
    (&["camera"], "camera"),
    (&["chat", "text", "font"], "text"),
    (&["setup", "import"], "import"),
    (&["showcase", "tool", "demo", "replay", "record"], "record"),
    (&["menu", "interface", "settings", "ui"], "interface"),
    (&["key", "bind", "control"], "key_bindings"),
    (&["fix", "crash", "stutter", "bug"], "options"),
];
/// The icon of a heading no entry names.
const HEADING_ICON: &str = "changelog";

/// The icon heading `heading` wears ([`HEADING_ICONS`]).
fn heading_icon(heading: &str) -> &'static str {
    let heading = heading.to_ascii_lowercase();
    let starts_word = |word: &str| {
        heading.match_indices(word).any(|(at, _)| {
            heading[..at]
                .chars()
                .next_back()
                .is_none_or(|before| !before.is_ascii_alphanumeric())
        })
    };
    HEADING_ICONS
        .iter()
        .find(|(words, _)| words.iter().any(|word| starts_word(word)))
        .map_or(HEADING_ICON, |(_, icon)| icon)
}

/// What the player chose on the card.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Choice {
    Close,
    Changelog,
    /// A chip: its text (`ViewerConsole::follow_chip`); the card closes.
    Chip(String),
}

/// One release on the card.
#[derive(Debug, Eq, PartialEq)]
struct Page {
    /// "2026.1011.3 (Alpha)", or "Unreleased"; its date, empty for "Unreleased".
    title: String,
    date: String,
    blocks: Vec<Block>,
    /// "By Sol and Creyon, after JoF EJK", or empty.
    credit: String,
}

/// One block of a page as offered, before it is wrapped to the card's width.
#[derive(Debug, Eq, PartialEq)]
enum Block {
    /// A recap line: its heading and its sentence, code in backticks.
    Line(String, String),
    /// A release's introduction, standing in for a recap it lacks.
    Intro(String),
}

/// One block of a page, wrapped.
#[derive(Debug, PartialEq)]
enum Item {
    /// A recap line: its heading's icon, its heading and the sentence's rows.
    Line(&'static str, String, Vec<Row>),
    /// An introduction's rows, in muted text.
    Intro(Vec<Row>),
    /// What did not fit.
    More,
}

impl Item {
    fn height(&self) -> f32 {
        match self {
            Self::Line(_, _, rows) => HEADING_HEIGHT + rows.len() as f32 * LOOK.line + ROW_GAP,
            Self::Intro(rows) => rows.len() as f32 * LOOK.line + ROW_GAP,
            Self::More => LOOK.line + ROW_GAP,
        }
    }
}

/// A page wrapped: its blocks, its credit's rows and its chips, in reading order.
#[derive(Debug, Default, PartialEq)]
struct Laid {
    items: Vec<Item>,
    credit: Vec<Row>,
    chips: Vec<String>,
}

impl Laid {
    fn height(&self) -> f32 {
        let credit = if self.credit.is_empty() {
            0.0
        } else {
            CREDIT_GAP + self.credit.len() as f32 * CREDIT_LOOK.line
        };
        self.items.iter().map(Item::height).sum::<f32>() + credit
    }
}

pub(crate) struct Card {
    /// The marker was read this run.
    checked: bool,
    /// The version the card is about, written to the marker on Close.
    version: String,
    kicker: &'static str,
    pages: Vec<Page>,
    /// The page on show.
    page: usize,
    /// The pages wrapped, and the measures they were wrapped with ([`Card::lay_out`]).
    laid: Vec<Laid>,
    laid_for: Option<u32>,
    /// Shown this frame.
    open: bool,
    ui: MenuCanvas,
}

impl Default for Card {
    fn default() -> Self {
        Self {
            checked: false,
            version: String::new(),
            kicker: "SJK updated",
            pages: Vec::new(),
            page: 0,
            laid: Vec::new(),
            laid_for: None,
            open: false,
            ui: MenuCanvas::with_capacities(96, 200, 96),
        }
    }
}

impl Card {
    /// Whether the marker was read this run; reading it sets this.
    pub(crate) fn take_check(&mut self) -> bool {
        !std::mem::replace(&mut self.checked, true)
    }

    /// Put `digests` (newest first) on the card for the player's `version`, a page each,
    /// or take the card away with none. `preview` is a recap shown on request, not an
    /// update.
    pub(crate) fn offer(&mut self, version: &str, digests: &[Digest], preview: bool) {
        self.put_away();
        self.version = version.to_owned();
        self.kicker = if preview { "Preview" } else { "SJK updated" };
        for digest in digests {
            let blocks = if digest.lines.is_empty() {
                (!digest.intro.trim().is_empty())
                    .then(|| Block::Intro(digest.intro.clone()))
                    .into_iter()
                    .collect()
            } else {
                digest
                    .lines
                    .iter()
                    .map(|(heading, sentence)| Block::Line(heading.clone(), sentence.clone()))
                    .collect()
            };
            self.pages.push(Page {
                title: digest.title.clone(),
                date: digest.date.clone(),
                blocks,
                credit: credit_line(&digest.people, &digest.after),
            });
        }
    }

    /// Whether the card has something to show.
    pub(crate) fn wants(&self) -> bool {
        !self.pages.is_empty()
    }

    /// The card is answered: it puts the recap away.
    fn put_away(&mut self) {
        self.pages.clear();
        self.laid.clear();
        self.laid_for = None;
        self.page = 0;
    }

    pub(crate) fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Show the page `step` pages on (back when negative), kept within the pages.
    pub(crate) fn turn(&mut self, step: isize) {
        let last = self.pages.len().saturating_sub(1);
        self.page = self.page.saturating_add_signed(step).min(last);
    }

    /// Enter, Space, Escape or Backspace close the card, C opens the changelog, Left and
    /// Right turn its pages; every other key is swallowed while the card shows.
    pub(crate) fn handle_key(&mut self, event: &KeyEvent) -> Option<Choice> {
        if event.state != ElementState::Pressed {
            return None;
        }
        let PhysicalKey::Code(code) = event.physical_key else {
            return None;
        };
        // Pages turn while an arrow is held; the other keys act once.
        if event.repeat && !matches!(code, KeyCode::ArrowLeft | KeyCode::ArrowRight) {
            return None;
        }
        self.key(code)
    }

    fn key(&mut self, code: KeyCode) -> Option<Choice> {
        let choice = match code {
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => Choice::Close,
            KeyCode::Escape | KeyCode::Backspace => Choice::Close,
            KeyCode::KeyC => Choice::Changelog,
            KeyCode::ArrowLeft => {
                self.turn(-1);
                return None;
            }
            KeyCode::ArrowRight => {
                self.turn(1);
                return None;
            }
            _ => return None,
        };
        self.put_away();
        Some(choice)
    }

    pub(crate) fn handle_pointer(&mut self, event: InputEvent) -> Option<Choice> {
        let event = self.ui.pointer(event)?;
        if event.kind != UiEventKind::Activate {
            return None;
        }
        let choice = match event.token? {
            CLOSE_TOKEN => Choice::Close,
            CHANGELOG_TOKEN => Choice::Changelog,
            PREVIOUS_TOKEN => {
                self.turn(-1);
                return None;
            }
            NEXT_TOKEN => {
                self.turn(1);
                return None;
            }
            token if token >= CHIP_BASE => {
                let chip = self
                    .laid
                    .get(self.page)?
                    .chips
                    .get(usize::from(token - CHIP_BASE))?;
                Choice::Chip(chip.clone())
            }
            _ => return None,
        };
        self.put_away();
        Some(choice)
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    /// Wrap every page to the card's width as `measures` draw text, once for each
    /// measure (the menu text style changes it); a page whose recap would grow taller
    /// than [`BODY_MAX`] stops with a pointer to the changelog.
    fn lay_out(&mut self, measures: &Measures<'_>) {
        let key = measures.key(&LOOK);
        if self.laid_for == Some(key) {
            return;
        }
        self.laid_for = Some(key);
        self.laid = self
            .pages
            .iter()
            .map(|page| lay_out_page(page, measures))
            .collect();
    }

    /// Draw the card over the whole frame.
    pub(crate) fn append(&mut self, target: TextTarget<'_>, viewport: [f32; 2]) {
        self.lay_out(&target.inline_measures());
        let frame = Frame::new(viewport);
        let s = frame.s;
        let body = self.laid.iter().map(Laid::height).fold(0.0, f32::max);
        let height = HEAD_HEIGHT + body + BUTTONS_GAP + BUTTON_HEIGHT + MARGIN;
        let top = ((FRAME_HEIGHT - height) / 2.0).max(24.0);
        let (Some(page), Some(laid)) = (self.pages.get(self.page), self.laid.get(self.page)) else {
            return;
        };
        let headline = if page.title == "Unreleased" {
            "What's coming next".to_owned()
        } else {
            format!("What's new in {}", page.title)
        };
        let ui = &mut self.ui;
        ui.begin_transparent(viewport);
        kit::scrim(ui, viewport);
        kit::card(ui, &frame, [CARD_X, top, CARD_WIDTH, height]);
        text(
            ui,
            TextFamily::Display,
            format_args!("{}", self.kicker),
            frame.rect(TEXT_X, top + 30.0, TEXT_WIDTH, 30.0),
            22.0 * s,
            color::GOLD_BRIGHT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let when = if self.pages.len() > 1 {
            format!(
                "Release {} of {}   /   {}",
                self.page + 1,
                self.pages.len(),
                page.date
            )
        } else {
            page.date.clone()
        };
        text(
            ui,
            TextFamily::Display,
            format_args!("{}", when.trim_end_matches(&[' ', '/'][..])),
            frame.rect(TEXT_X, top + 30.0, TEXT_WIDTH, 30.0),
            19.0 * s,
            color::MUTED,
            FontWeight::Semibold,
            TextAlign::End,
        );
        text(
            ui,
            TextFamily::Display,
            format_args!("{headline}"),
            frame.rect(TEXT_X, top + 66.0, TEXT_WIDTH, 48.0),
            36.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let mut y = top + HEAD_HEIGHT;
        let mut chip_token = CHIP_BASE;
        for item in &laid.items {
            match item {
                Item::Line(icon, heading, rows) => {
                    if let Some(texture) = crate::settings_icons::texture(icon) {
                        let _ = ui.draw_list_mut().push(DrawCommand::TexturedQuad {
                            rect: frame.rect(TEXT_X, y + (HEADING_HEIGHT - ICON) * 0.5, ICON, ICON),
                            texture,
                            color: Color::new(1.0, 1.0, 1.0, 0.9),
                        });
                    }
                    text(
                        ui,
                        TextFamily::Display,
                        format_args!("{heading}"),
                        frame.rect(TEXT_X + INDENT, y, TEXT_WIDTH - INDENT, HEADING_HEIGHT),
                        HEADING_SIZE * s,
                        color::GOLD_BRIGHT,
                        FontWeight::Semibold,
                        TextAlign::Start,
                    );
                    inline_code::draw_rows(
                        ui,
                        &frame,
                        rows,
                        &LOOK,
                        &Place {
                            x: TEXT_X + INDENT,
                            y: y + HEADING_HEIGHT,
                            colour: color::TEXT,
                            code_family: TextFamily::Code,
                        },
                        Some(&mut chip_token),
                    );
                }
                Item::Intro(rows) => inline_code::draw_rows(
                    ui,
                    &frame,
                    rows,
                    &LOOK,
                    &Place {
                        x: TEXT_X,
                        y,
                        colour: color::MUTED,
                        code_family: TextFamily::Code,
                    },
                    Some(&mut chip_token),
                ),
                Item::More => {
                    text(
                        ui,
                        TextFamily::Body,
                        format_args!("More in the full changelog."),
                        frame.rect(TEXT_X, y, TEXT_WIDTH, LOOK.line),
                        LOOK.size * s,
                        color::MUTED,
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                }
            }
            y += item.height();
        }
        if !laid.credit.is_empty() {
            inline_code::draw_rows(
                ui,
                &frame,
                &laid.credit,
                &CREDIT_LOOK,
                &Place {
                    x: TEXT_X,
                    y: y + CREDIT_GAP,
                    colour: color::alpha(color::GOLD, 0.85),
                    code_family: TextFamily::Code,
                },
                None,
            );
        }
        let buttons_y = top + HEAD_HEIGHT + body + BUTTONS_GAP;
        let right = CARD_X + CARD_WIDTH - MARGIN;
        kit::button(
            ui,
            &frame,
            [right - CLOSE_WIDTH, buttons_y, CLOSE_WIDTH, BUTTON_HEIGHT],
            "Close",
            true,
            true,
            false,
            CLOSE_TOKEN,
        );
        kit::button(
            ui,
            &frame,
            [
                right - CLOSE_WIDTH - BUTTON_GAP - CHANGELOG_WIDTH,
                buttons_y,
                CHANGELOG_WIDTH,
                BUTTON_HEIGHT,
            ],
            "Full changelog",
            false,
            true,
            false,
            CHANGELOG_TOKEN,
        );
        let pages = self.pages.len();
        if pages > 1 {
            pager(ui, &frame, buttons_y, self.page, pages);
        }
        let [right, keys_y] = frame.point(KEYS_RIGHT, KEYS_Y);
        let mut hints: Vec<(&[&str], &str)> = Vec::with_capacity(3);
        if pages > 1 {
            hints.push((&["Left", "Right"], "release"));
        }
        hints.push((&["C"], "full changelog"));
        hints.push((&["Enter"], "close"));
        let gap = 18.0 * s;
        let width: f32 = hints
            .iter()
            .map(|(keys, action)| key_hint_width(keys, action, s))
            .sum::<f32>()
            + gap * (hints.len() - 1) as f32;
        let mut x = right - width;
        for (keys, action) in hints {
            x = key_hint(ui, keys, action, x, keys_y, s) + gap;
        }
        ui.finish(CLOSE_TOKEN);
        target.append(&self.ui, viewport);
    }
}

/// The page arrows and a dot for each of the `pages`, the one on show (`page`) lit, at
/// the foot's left on the buttons' line `y`.
fn pager(ui: &mut MenuCanvas, frame: &Frame, y: f32, page: usize, pages: usize) {
    kit::button(
        ui,
        frame,
        [TEXT_X, y, ARROW_WIDTH, BUTTON_HEIGHT],
        "<",
        false,
        page > 0,
        false,
        PREVIOUS_TOKEN,
    );
    let dots_x = TEXT_X + ARROW_WIDTH + 18.0;
    for index in 0..pages {
        let lit = index == page;
        let dot = frame.rect(
            dots_x + index as f32 * DOT_STEP,
            y + (BUTTON_HEIGHT - DOT) * 0.5,
            DOT,
            DOT,
        );
        let _ = ui.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: dot,
            radius: dot.width * 0.5,
            color: if lit {
                color::GOLD_BRIGHT
            } else {
                color::alpha(color::HOLO, 0.35)
            },
        });
    }
    let next_x = dots_x + (pages - 1) as f32 * DOT_STEP + DOT + 18.0;
    kit::button(
        ui,
        frame,
        [next_x, y, ARROW_WIDTH, BUTTON_HEIGHT],
        ">",
        false,
        page + 1 < pages,
        false,
        NEXT_TOKEN,
    );
}

/// `page` wrapped to the card's width as `measures` draw it.
fn lay_out_page(page: &Page, measures: &Measures<'_>) -> Laid {
    let credit = if page.credit.is_empty() {
        Vec::new()
    } else {
        inline_code::flow(&page.credit, TEXT_WIDTH, measures, &CREDIT_LOOK)
    };
    let mut laid = Laid {
        credit,
        ..Laid::default()
    };
    let room = BODY_MAX - Item::More.height() - (laid.height());
    let mut height = 0.0;
    for block in &page.blocks {
        let item = match block {
            Block::Line(heading, sentence) => Item::Line(
                heading_icon(heading),
                heading.clone(),
                inline_code::flow(sentence, TEXT_WIDTH - INDENT, measures, &LOOK),
            ),
            Block::Intro(intro) => Item::Intro(intro_rows(intro, measures)),
        };
        if height + item.height() > room {
            laid.items.push(Item::More);
            break;
        }
        height += item.height();
        laid.items.push(item);
    }
    for item in &laid.items {
        if let Item::Line(_, _, rows) | Item::Intro(rows) = item {
            laid.chips
                .extend(inline_code::chips(rows).map(str::to_owned));
        }
    }
    laid
}

/// An introduction's rows: [`INTRO_LINES`] at most, the last ending in "..." when it was
/// cut short.
fn intro_rows(intro: &str, measures: &Measures<'_>) -> Vec<Row> {
    let ellipsis = measures
        .body
        .width("...", LOOK.size, crate::text::TextFace::Regular);
    let mut rows = inline_code::flow(intro, TEXT_WIDTH - ellipsis, measures, &LOOK);
    if rows.len() > INTRO_LINES {
        rows.truncate(INTRO_LINES);
        if let Some(run) = rows.last_mut().and_then(|row| row.last_mut()) {
            if run.code {
                // A chip ends the row: the dots come after it.
                let x = run.x + run.width;
                rows.last_mut().expect("checked").push(inline_code::Run {
                    x,
                    width: ellipsis,
                    text: "...".to_owned(),
                    code: false,
                });
            } else {
                run.text.push_str("...");
                run.width += ellipsis;
            }
        }
    }
    rows
}

/// "By Sol and Creyon, after JoF EJK and OpenJK": who made a release's changes and the
/// clients they follow; empty when nobody is credited.
fn credit_line(people: &[String], after: &[String]) -> String {
    if people.is_empty() {
        return String::new();
    }
    let mut line = format!("By {}", and_list(people));
    if !after.is_empty() {
        line.push_str(", after ");
        line.push_str(&and_list(after));
    }
    line
}

/// "a", "a and b", "a, b and c".
fn and_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [first @ .., last] => format!("{} and {last}", first.join(", ")),
    }
}

impl crate::GpuState {
    /// Read the marker once and put the recap of an update on the card; a new
    /// installation, and a build no release lists, show nothing. Then say whether the
    /// card shows this frame: the player on the main menu or the game menu with nothing
    /// else over it. `covered` is another card or the console over the frame.
    pub(crate) fn prepare_whats_new_card(&mut self, covered: bool) -> bool {
        if self.whats_new_card.take_check() {
            self.read_whats_new_marker();
        }
        let menu_visible = self
            .client_menu
            .as_ref()
            .is_some_and(crate::menu::ClientMenu::is_visible);
        let main_menu = self
            .client_menu
            .as_ref()
            .is_some_and(|menu| menu.is_visible() && menu.on_main_menu());
        let place = main_menu || (self.game_menu && !menu_visible);
        let console_open = self
            .console
            .as_ref()
            .is_some_and(crate::console::ViewerConsole::is_open);
        let away = self
            .console
            .as_ref()
            .is_some_and(crate::console::ViewerConsole::window_away);
        let open = self.whats_new_card.wants()
            && place
            && !covered
            && !console_open
            && !away
            && !self.text_dialog.is_open()
            && self.world_load_task.is_none()
            && self.world_install_task.is_none()
            && !self.pending_map_reload;
        self.whats_new_card.set_open(open);
        open
    }

    /// What the marker says about this run: nothing to show, a new installation to mark
    /// as up to date, or an update to recap.
    fn read_whats_new_marker(&mut self) {
        let installed = crate::build_info::VERSION;
        let Some(console) = self.console.as_mut() else {
            return;
        };
        let seen = console.text_cvar(SEEN_CVAR).unwrap_or_default().to_owned();
        if seen == installed {
            return;
        }
        // A player who never saw First setup installed just now: nothing was updated.
        if seen.is_empty() && console.bool_cvar(crate::settings::quick::HIDE_CVAR) != Some(true) {
            console.set_cvar(SEEN_CVAR, installed);
            return;
        }
        let digests = crate::console::changelog::digests(installed, &seen);
        self.whats_new_card.offer(installed, &digests, false);
    }

    /// `whatsnew`: put the newest recap on the card, the unreleased one included.
    pub(crate) fn preview_whats_new(&mut self) {
        let digests: Vec<Digest> = crate::console::changelog::newest_digest()
            .into_iter()
            .collect();
        self.whats_new_card.offer("", &digests, true);
    }

    /// Draw the card. It darkens everything, so every font batch appended before it is
    /// dropped (text draws above all shapes).
    pub(crate) fn append_whats_new_card(&mut self, viewport: [f32; 2]) {
        self.text_vertices.clear();
        self.classic_text_vertices.clear();
        self.game_fonts.clear_text();
        let target = crate::ingame_menu::sjk_view::text_target(
            &mut self.game_fonts,
            &mut self.text_vertices,
            &self.ui_font,
        );
        self.whats_new_card.append(target, viewport);
    }

    /// Act on the player's choice: remember the version, then open the changelog, or
    /// follow a chip (Settings on the setting it names, or the console with it typed).
    pub(crate) fn whats_new_card_choice(&mut self, choice: Option<Choice>) {
        let Some(choice) = choice else {
            return;
        };
        let version = self.whats_new_card.version.clone();
        if let Some(console) = self.console.as_mut() {
            if !version.is_empty() {
                console.set_cvar(SEEN_CVAR, &version);
            }
            match &choice {
                Choice::Close => {}
                Choice::Changelog => console.open_changelog(),
                Choice::Chip(text) => console.follow_chip(text),
            }
        }
        self.sync_cursor_policy();
    }

    /// Open Settings on the setting a chip named (`ViewerConsole::follow_chip`), once;
    /// it returns to the game menu in a match, else to the main menu.
    pub(crate) fn open_chip_settings(&mut self) {
        let Some(query) = self
            .console
            .as_mut()
            .and_then(crate::console::ViewerConsole::take_settings_search)
        else {
            return;
        };
        let target = if self.live_session.is_some() {
            crate::player_menu::ReturnTarget::InGame
        } else {
            crate::player_menu::ReturnTarget::MainMenu
        };
        if let (Some(menu), Some(console)) = (&mut self.client_menu, &self.console) {
            menu.open_settings_search(console, &query, target);
            self.game_menu = false;
        }
        self.sync_cursor_policy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sjk_chat_look::Measure;
    use crate::text::{TextStyle, test_font};

    fn digest(title: &str, lines: &[(&str, &str)]) -> Digest {
        Digest {
            title: title.to_owned(),
            date: "12/10/2026".to_owned(),
            intro: "A release introduction that is not a recap.".to_owned(),
            lines: lines
                .iter()
                .map(|(heading, sentence)| ((*heading).to_owned(), (*sentence).to_owned()))
                .collect(),
            people: vec!["Sol".to_owned(), "Creyon".to_owned()],
            after: vec!["JoF EJK".to_owned()],
        }
    }

    /// Lay `card` out with test glyphs, 12 frame pixels a character at the text size.
    fn laid_out(card: &mut Card) {
        let font = test_font();
        let measure = Measure::new(&font, TextStyle::NEUTRAL);
        card.lay_out(&Measures {
            body: measure,
            code: measure,
        });
    }

    #[test]
    fn a_recap_is_shown_until_it_is_closed() {
        let mut card = Card::default();
        assert!(!card.wants());
        card.offer(
            "2.0",
            &[digest("2.0", &[("Servers", "Downloads work.")])],
            false,
        );
        assert!(card.wants());
        assert_eq!(card.pages[0].title, "2.0");
        assert_eq!(card.key(KeyCode::KeyA), None);
        assert!(card.wants(), "other keys are swallowed");
        assert_eq!(card.key(KeyCode::Enter), Some(Choice::Close));
        assert!(!card.wants());
        card.offer("2.0", &[digest("2.0", &[("Servers", "x")])], false);
        assert_eq!(card.key(KeyCode::KeyC), Some(Choice::Changelog));
        card.offer("2.0", &[digest("2.0", &[("Servers", "x")])], false);
        assert_eq!(card.key(KeyCode::Escape), Some(Choice::Close));
        card.offer("2.0", &[], false);
        assert!(!card.wants());
    }

    #[test]
    fn the_marker_is_read_once_a_run() {
        let mut card = Card::default();
        assert!(card.take_check());
        assert!(!card.take_check());
    }

    #[test]
    fn skipped_releases_are_pages_and_a_missing_recap_falls_back_to_the_introduction() {
        let mut card = Card::default();
        card.offer(
            "3.0",
            &[
                digest("3.0", &[("Cards", "Two cards.")]),
                digest("2.0", &[]),
                digest("1.0", &[("Fixes", "One.")]),
            ],
            false,
        );
        laid_out(&mut card);
        assert_eq!(card.pages.len(), 3);
        assert!(matches!(card.laid[0].items[0], Item::Line(..)));
        assert!(matches!(card.laid[1].items[0], Item::Intro(_)));
        // Left and Right turn the pages within them, and do not close the card.
        assert_eq!(card.key(KeyCode::ArrowLeft), None);
        assert_eq!(card.page, 0);
        assert_eq!(card.key(KeyCode::ArrowRight), None);
        assert_eq!(card.key(KeyCode::ArrowRight), None);
        assert_eq!(card.key(KeyCode::ArrowRight), None);
        assert_eq!(card.page, 2);
        assert!(card.wants());
        assert_eq!(card.key(KeyCode::Enter), Some(Choice::Close));
        assert_eq!(card.page, 0, "a new offer starts on its first page");

        // A long introduction is cut, with an ellipsis.
        let long = Digest {
            intro: "word ".repeat(200),
            ..digest("1.0", &[])
        };
        card.offer("1.0", &[long], false);
        laid_out(&mut card);
        let Some(Item::Intro(rows)) = card.laid[0].items.first() else {
            panic!("{:?}", card.laid);
        };
        assert_eq!(rows.len(), INTRO_LINES);
        let last = rows[INTRO_LINES - 1].last().unwrap();
        assert!(last.text.ends_with("..."), "{rows:?}");
        assert!(last.x + last.width <= TEXT_WIDTH);
    }

    #[test]
    fn a_heading_sits_with_its_icon_over_its_sentence() {
        let mut card = Card::default();
        let sentence = "word ".repeat(30);
        card.offer("2.0", &[digest("2.0", &[("Servers", &sentence)])], false);
        laid_out(&mut card);
        let Some(Item::Line(icon, heading, rows)) = card.laid[0].items.first() else {
            panic!("{:?}", card.laid);
        };
        assert_eq!((*icon, heading.as_str()), ("network", "Servers"));
        // 150 characters of 12 pixels in rows 794 wide (the heading's indent off).
        assert_eq!(rows.len(), 3, "{rows:?}");
        assert!(rows.iter().all(|row| row[0].width <= TEXT_WIDTH - INDENT));
    }

    #[test]
    fn headings_find_their_icons_and_every_icon_exists() {
        assert_eq!(heading_icon("Browser"), "network");
        assert_eq!(heading_icon("Saber shaders"), "saber_style");
        assert_eq!(heading_icon("Small fixes"), "options");
        assert_eq!(heading_icon("Gameplay FPS"), "gameplay");
        assert_eq!(heading_icon("HUD"), "hud");
        // A word inside another is no match ("ui" in "build").
        assert_eq!(heading_icon("Builds"), HEADING_ICON);
        for (_, icon) in HEADING_ICONS {
            assert!(crate::settings_icons::texture(icon).is_some(), "{icon}");
        }
        assert!(crate::settings_icons::texture(HEADING_ICON).is_some());
    }

    #[test]
    fn chips_answer_in_reading_order_and_close_the_card() {
        let mut card = Card::default();
        card.offer(
            "2.0",
            &[digest(
                "2.0",
                &[
                    ("Updates", "Type `whatsnew`."),
                    ("HUD", "See `cg_drawFPS`."),
                ],
            )],
            false,
        );
        laid_out(&mut card);
        assert_eq!(card.laid[0].chips, ["whatsnew", "cg_drawFPS"]);
    }

    #[test]
    fn the_credit_names_people_then_the_clients_they_follow() {
        assert_eq!(credit_line(&["Sol".to_owned()], &[]), "By Sol");
        assert_eq!(
            credit_line(
                &["Sol".to_owned(), "Creyon".to_owned(), "Lumaya".to_owned()],
                &["JoF EJK".to_owned(), "OpenJK".to_owned()]
            ),
            "By Sol, Creyon and Lumaya, after JoF EJK and OpenJK"
        );
        assert_eq!(credit_line(&[], &["OpenJK".to_owned()]), "");
    }

    #[test]
    fn a_long_recap_stops_with_a_pointer_to_the_changelog() {
        let long = "word ".repeat(60);
        let lines: Vec<(&str, &str)> = (0..8).map(|_| ("Heading", long.as_str())).collect();
        let mut card = Card::default();
        card.offer("3.0", &[digest("3.0", &lines)], false);
        laid_out(&mut card);
        assert_eq!(card.laid[0].items.last(), Some(&Item::More));
        assert!(
            card.laid[0].height() <= BODY_MAX,
            "{}",
            card.laid[0].height()
        );
    }

    #[test]
    fn the_layout_is_kept_until_the_measure_changes() {
        let font = test_font();
        let neutral = Measure::new(&font, TextStyle::NEUTRAL);
        let mut card = Card::default();
        card.offer(
            "2.0",
            &[digest("2.0", &[("Servers", "Downloads work.")])],
            false,
        );
        let measures = Measures {
            body: neutral,
            code: neutral,
        };
        card.lay_out(&measures);
        let first = card.laid_for;
        card.laid.clear();
        card.lay_out(&measures);
        assert!(card.laid.is_empty(), "laid out once for a measure");
        let larger = Measure::new(
            &font,
            TextStyle {
                scale: 1.25,
                ..TextStyle::NEUTRAL
            },
        );
        card.lay_out(&Measures {
            body: larger,
            code: larger,
        });
        assert_ne!(card.laid_for, first);
        assert!(!card.laid.is_empty());
    }

    #[test]
    fn the_real_changelog_recaps_fit_the_card() {
        // "Unreleased", when main has one, gets its recap when the release is cut.
        let newest = crate::console::changelog::newest_release_digest().unwrap();
        assert!(!newest.lines.is_empty(), "{} has no recap", newest.title);
        let mut card = Card::default();
        card.offer("", &[newest], true);
        laid_out(&mut card);
        assert!(card.wants());
        assert!(!card.laid[0].items.contains(&Item::More), "{:?}", card.laid);
        assert!(card.laid[0].height() < FRAME_HEIGHT - HEAD_HEIGHT - 200.0);
    }
}
