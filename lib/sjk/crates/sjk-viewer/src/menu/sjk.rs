//! The SJK UI (`ui_menuStyle sjk`): SJK's own menus, drawn over the live map in
//! SJK's own type, as SJK's site looks (`site/assets/style.css`). It keeps the
//! feeling of retail's menus (gold for what you choose, holo blue line-work, the
//! turning ring round the emblem) and drops their window frames and boxes:
//! screens are laid out on alignment, thin lines and dark fades at the screen's
//! edges. Design: `docs/sjk-ui.md`.
//!
//! The UI is built screen by screen and becomes the default once done. So far
//! it has its main page ([`home`]), Settings ([`settings`]), Character
//! (`player_menu::sjk_view`), Servers ([`browser`]), Create game and its map
//! list (`menu::create_game_view`, in every menu style) and the loading screen
//! ([`loading`]), drawn with the controls of its [`kit`]; every other screen
//! opens in its classic version.
//!
//! Text is set in two families ([`TextFamily`]): Rajdhani for navigation,
//! titles and numbers, Exo 2 for the rest; both are bundled vector fonts
//! (`crate::text::DISPLAY`, `crate::text::BODY`). Layouts are authored in pixels
//! of a 1080-line screen and scale with the window's height
//! ([`crate::ui_scale::height_scale`]).

pub(crate) mod browser;
pub(crate) mod chat_dock;
pub(crate) mod home;
pub(crate) mod hover;
pub(crate) mod kit;
pub(crate) mod loading;
pub(crate) mod recent;
pub(crate) mod settings;

use super::{ClientMenu, MenuAction};
use crate::console::ViewerConsole;
use crate::game_font::SjkFonts;
use crate::menu_widgets::{MenuCanvas, TextFamily};
use crate::player_menu::ReturnTarget;
use crate::text::{TextStyle, TextVertex, UiFont};
use sjk_ui::{Color, DrawCommand, FontWeight, Gradient, Rect, TextAlign};

/// The main page's servers: the ones joined last with what the server list
/// says of them now, or the suggested JoF server while there are none.
fn home_servers<'a>(
    recent: &'a recent::RecentServers,
    entries: &'a [crate::server_browser::ServerEntry],
    now: u64,
) -> impl Iterator<Item = home::ServerItem<'a>> {
    let find = move |address: &str| {
        let parsed = address.trim().parse::<std::net::SocketAddr>().ok()?;
        entries.iter().find(|entry| entry.address == parsed)
    };
    let suggested = recent.servers().is_empty().then(|| {
        let live = find(home::JOF_SERVER);
        home::ServerItem {
            name: live.map_or("JoF", |entry| entry.name.as_str()),
            map: live.map_or("", |entry| entry.map.as_str()),
            live: live.map(|entry| (entry.players, entry.capacity, entry.ping_millis)),
            played: None,
        }
    });
    let joined = recent.servers().iter().map(move |server| {
        let live = find(&server.address);
        let stored = |value: &'a String, fallback: &'a str| {
            if value.is_empty() {
                fallback
            } else {
                value.as_str()
            }
        };
        home::ServerItem {
            name: live.map_or_else(
                || stored(&server.name, &server.address),
                |entry| entry.name.as_str(),
            ),
            map: live.map_or_else(|| stored(&server.map, ""), |entry| entry.map.as_str()),
            live: live.map(|entry| (entry.players, entry.capacity, entry.ping_millis)),
            played: Some(recent::ago(server.played, now)),
        }
    });
    suggested.into_iter().chain(joined)
}

/// Where an SJK UI screen's text goes this frame.
pub(crate) enum TextTarget<'a> {
    /// The UI's own families, in the player's menu text style.
    Families(SjkFonts<'a>, TextStyle),
    /// Inter, while the families are not loaded (or failed to load).
    Inter(&'a mut Vec<TextVertex>, &'a UiFont),
}

impl<'a> TextTarget<'a> {
    /// How the body family's text measures as this target will draw it, in the
    /// player's menu text style, for runs laid out one after another on a line.
    pub(crate) fn body_measure(&self) -> crate::sjk_chat_look::Measure<'a> {
        match *self {
            Self::Families(ref fonts, style) => {
                crate::sjk_chat_look::Measure::new(fonts.body.1, style)
            }
            Self::Inter(_, font) => crate::sjk_chat_look::Measure::new(font, font.style()),
        }
    }

    /// How chips' text (`inline_code`) measures as this target draws it, and the body
    /// family's text beside it.
    pub(crate) fn inline_measures(&self) -> crate::inline_code::Measures<'a> {
        let body = self.body_measure();
        let code = match *self {
            Self::Families(ref fonts, style) => fonts
                .code
                .as_ref()
                .map(|code| crate::sjk_chat_look::Measure::new(code.1, style)),
            Self::Inter(..) => None,
        };
        crate::inline_code::Measures {
            body,
            code: code.unwrap_or(body),
        }
    }

    /// Append `canvas`'s text runs, each in its family.
    pub(crate) fn append(self, canvas: &MenuCanvas, viewport: [f32; 2]) {
        match self {
            Self::Families(fonts, style) => canvas.append_text_families(fonts, viewport, style),
            Self::Inter(vertices, font) => canvas.append_text(vertices, font, viewport),
        }
    }
}

impl ClientMenu {
    /// Draw the SJK UI's screen on show ([`ClientMenu::sjk_screen`]): its main
    /// page, with the player read from `console`, its Settings, Character,
    /// Servers, Create game or loading screen.
    pub(crate) fn append_sjk_screen(
        &mut self,
        target: TextTarget<'_>,
        console: Option<&ViewerConsole>,
        viewport: [f32; 2],
    ) {
        match self.state.phase() {
            super::ClientPhase::Settings => self.append_sjk_settings(target, viewport),
            super::ClientPhase::Keybinds => self.append_sjk_keys(target, viewport),
            super::ClientPhase::Browser => self.append_sjk_browser(target, viewport),
            super::ClientPhase::Connecting(_) | super::ClientPhase::ConnectionError => {
                self.append_sjk_loading(target, viewport);
            }
            super::ClientPhase::CreateGame => {
                let reveal = self.screen_reveal();
                self.create_game.append(target, viewport, reveal);
            }
            super::ClientPhase::Player => {
                let reveal = self.screen_reveal();
                if let Some(console) = console {
                    self.player.follow_blade_skins(console);
                }
                self.player.append_sjk(target, viewport, reveal);
            }
            _ => self.append_sjk_home(target, console, viewport),
        }
    }

    /// Draw the SJK UI's main page, with the player read from `console` and the
    /// servers from the recent list and the server list.
    pub(crate) fn append_sjk_home(
        &mut self,
        target: TextTarget<'_>,
        console: Option<&ViewerConsole>,
        viewport: [f32; 2],
    ) {
        let reveal = self.screen_reveal();
        let name = console
            .and_then(|console| console.text_value("name"))
            .unwrap_or("Padawan");
        let model = console
            .and_then(|console| console.text_value("model"))
            .and_then(|model| model.split('/').next())
            .filter(|model| !model.is_empty())
            .unwrap_or("kyle");
        let update = crate::update::available_version();
        let now = recent::now();
        let entries = self.browser.entries();
        let blank = home::ServerItem {
            name: "",
            map: "",
            live: None,
            played: None,
        };
        let mut servers = [blank; recent::MAX];
        let mut count = 0;
        for (slot, item) in servers
            .iter_mut()
            .zip(home_servers(&self.recent, entries, now))
        {
            *slot = item;
            count += 1;
        }
        let chat_on = console.is_some_and(|console| console.bool_cvar("cl_sjkChat") != Some(false));
        if chat_on {
            self.chat_dock.refresh();
        }
        let mut lines = [chat_dock::BLANK; chat_dock::LINES];
        // The sender card's picture, asked before the profile card's lock is taken.
        self.home
            .place_sender_card(crate::player_identity::avatar_version);
        let measure = target.body_measure();
        let chat = chat_on.then(|| home::ChatDock {
            measure: Some(measure),
            ..self.chat_dock.view(&mut lines)
        });
        crate::profile_card::with(|summary| {
            let view = home::HomeView {
                name,
                model,
                blade_name: console.map_or("blue", blade_name),
                servers: &servers[..count],
                version: env!("SJK_BUILD_VERSION"),
                update: update.as_deref(),
                seconds: super::art::motion::seconds(),
                chat,
                summary,
                donation: crate::donation_goal::card(console),
            };
            home::build(&mut self.ui, viewport, &mut self.home, &view, reveal);
        });
        target.append(&self.ui, viewport);
    }

    /// How many servers the main page's column lists.
    fn sjk_home_servers(&self) -> usize {
        self.recent.servers().len().max(1)
    }

    /// The address of server `index` of the main page's column.
    fn sjk_home_address(&self, index: usize) -> Option<String> {
        match self.recent.servers() {
            [] => (index == 0).then(|| home::JOF_SERVER.to_owned()),
            servers => servers.get(index).map(|server| server.address.clone()),
        }
    }

    /// Choose entry `index` of the main page's ring, as Down would (world shots).
    #[cfg(test)]
    pub(crate) fn sjk_home_entry_for_shot(&mut self, index: usize) {
        for _ in 0..index {
            let _ = self
                .home
                .key(winit::keyboard::KeyCode::ArrowDown, self.sjk_home_servers());
        }
    }

    /// Made-up SJK chat messages (sender, text, verified) and who is online in the main
    /// page's dock, and `draft` typed in its field (world shots).
    #[cfg(test)]
    pub(crate) fn sjk_home_chat_for_shot(
        &mut self,
        lines: &[(&str, &str, bool)],
        online: u32,
        draft: Option<&str>,
    ) {
        self.chat_dock.for_shot(lines, online);
        self.home.chat_for_shot(draft);
    }

    /// Whether the main page's last frame ran out of room on its canvas (world shots).
    #[cfg(test)]
    pub(crate) fn sjk_home_overflowed(&self) -> bool {
        self.ui.overflowed()
    }

    /// A key on the SJK UI's main page.
    pub(super) fn sjk_home_key(
        &mut self,
        key: winit::keyboard::KeyCode,
        console: &mut ViewerConsole,
    ) -> MenuAction {
        match self.home.key(key, self.sjk_home_servers()) {
            Some(action) => self.sjk_home_act(action, console),
            None => MenuAction::None,
        }
    }

    /// The pointer over (or clicking) `token` on the SJK UI's main page.
    pub(super) fn sjk_home_pointer(
        &mut self,
        token: u16,
        activate: bool,
        console: &mut ViewerConsole,
    ) -> MenuAction {
        match self.home.pointer(token, activate, self.sjk_home_servers()) {
            Some(action) => self.sjk_home_act(action, console),
            None => MenuAction::None,
        }
    }

    /// Carry out an action of the main page.
    fn sjk_home_act(&mut self, action: home::Action, console: &mut ViewerConsole) -> MenuAction {
        match action {
            home::Action::Join(index) => match self.sjk_home_address(index) {
                Some(address) => self.join_address(address),
                None => MenuAction::None,
            },
            home::Action::Open(super::destination::MainDestination::Settings { .. }) => {
                self.open_sjk_settings_resumed(console, ReturnTarget::MainMenu);
                MenuAction::None
            }
            home::Action::Open(destination) => self.open_main_destination(destination, console),
            home::Action::Quit => MenuAction::Quit,
            home::Action::Donate => {
                crate::donation_goal::open();
                MenuAction::None
            }
            home::Action::SendChat => {
                let text = self.home.take_draft();
                self.chat_dock
                    .send(text, console.bool_cvar("cl_sjkChat") != Some(false));
                MenuAction::None
            }
            home::Action::OpenChat => {
                console.open_sjk_chat_panel();
                MenuAction::None
            }
            home::Action::Mute => {
                // Local only: the list on this PC (`player_mutes.rs`).
                if let Some((key_id, name)) = self.home.take_mute() {
                    crate::player_mutes::set_muted(Some(&key_id), &name, true);
                }
                MenuAction::None
            }
        }
    }

    /// A key while the main page's chat field is being typed in: every key goes to
    /// the field.
    pub(super) fn sjk_home_typing(
        &mut self,
        event: &winit::event::KeyEvent,
        console: &mut ViewerConsole,
    ) -> MenuAction {
        let winit::keyboard::PhysicalKey::Code(key) = event.physical_key else {
            return MenuAction::None;
        };
        let (shift, control) = (console.shift_held(), console.control_held());
        match self
            .home
            .typing_key(key, event.text.as_deref(), shift, control)
        {
            Some(action) => self.sjk_home_act(action, console),
            None => MenuAction::None,
        }
    }

    /// A join just reached the game: the server goes to the top of the recent
    /// list, with its name and map as the server list (or the loading screen)
    /// knows them. A game this client hosts is not a server to come back to.
    pub(super) fn remember_joined_server(&mut self) {
        let super::ClientPhase::Connecting(address) = self.state.phase() else {
            return;
        };
        if self.hosting_local() {
            return;
        }
        let address = address.clone();
        let entry = address
            .parse::<std::net::SocketAddr>()
            .ok()
            .and_then(|parsed| {
                self.browser
                    .entries()
                    .iter()
                    .find(|entry| entry.address == parsed)
            });
        let name = entry.map_or("", |entry| entry.name.as_str()).to_owned();
        let map = entry
            .map(|entry| entry.map.as_str())
            .filter(|map| !map.is_empty())
            .unwrap_or(self.loading.map())
            .to_owned();
        self.recent.record(&address, &name, &map, recent::now());
    }

    /// Join `address` from the main page: through the password prompt when the
    /// server list says the server has one.
    fn join_address(&mut self, address: String) -> MenuAction {
        let entry = address
            .parse::<std::net::SocketAddr>()
            .ok()
            .and_then(|parsed| {
                self.browser
                    .entries()
                    .iter()
                    .find(|entry| entry.address == parsed)
            });
        self.destination_map = entry.map(|entry| entry.map.clone());
        if entry.is_some_and(|entry| entry.password) {
            // The browser draws the password prompt.
            self.open_browser();
            self.open_password_prompt(address);
            return MenuAction::None;
        }
        self.state.connecting(address.clone());
        MenuAction::Connect(address)
    }
}

/// The SJK UI's colours, as on SJK's site. They are display values, like all
/// 2D colours (`crate::ui_target`).
pub(crate) mod color {
    use sjk_ui::Color;

    /// The ground: deep space navy, never pure black.
    pub(crate) const SPACE: Color = Color::new(0.024, 0.039, 0.078, 1.0);
    /// Line-work: rules, ticks, outlines.
    pub(crate) const HOLO: Color = Color::new(0.659, 0.812, 1.0, 1.0);
    /// What you choose: the selection, the one main action of a screen.
    pub(crate) const GOLD: Color = Color::new(0.91, 0.722, 0.29, 1.0);
    /// Gold lit: the chosen item's text and marks.
    pub(crate) const GOLD_BRIGHT: Color = Color::new(1.0, 0.851, 0.478, 1.0);
    /// Text.
    pub(crate) const TEXT: Color = Color::new(0.886, 0.914, 0.965, 1.0);
    /// Secondary text.
    pub(crate) const MUTED: Color = Color::new(0.663, 0.71, 0.796, 1.0);
    /// Quieter still: an item that ends the session.
    pub(crate) const QUIET: Color = Color::new(0.53, 0.573, 0.675, 1.0);
    /// Only for leaving: Quit.
    pub(crate) const EMBER: Color = Color::new(1.0, 0.478, 0.239, 1.0);

    /// `color` at `alpha`.
    pub(crate) const fn alpha(color: Color, alpha: f32) -> Color {
        Color::new(color.r, color.g, color.b, alpha)
    }
}

/// A vertical fade over `rect` from `top` to `bottom`.
pub(crate) fn fade(canvas: &mut MenuCanvas, rect: Rect, top: Color, bottom: Color) {
    let _ = canvas.draw_list_mut().push(DrawCommand::GradientRect {
        rect,
        radius: 0.0,
        gradient: Gradient {
            start: top,
            end: bottom,
            vertical: true,
        },
    });
}

/// A horizontal fade over `rect` from `left` to `right`.
pub(crate) fn fade_across(canvas: &mut MenuCanvas, rect: Rect, left: Color, right: Color) {
    let _ = canvas.draw_list_mut().push(DrawCommand::GradientRect {
        rect,
        radius: 0.0,
        gradient: Gradient {
            start: left,
            end: right,
            vertical: false,
        },
    });
}

/// Where a run's letters sit in its line box, as a share of its size down from
/// the line's top: the middle of its capitals, leaning a quarter of the way to
/// the middle of its lower case, so mixed text looks centred. Measured from
/// the bundled fonts (`letters_centre_on_the_middle_of_their_rectangle`):
/// Rajdhani's capitals centre at 0.476 and its x-height at 0.529, Exo 2's at
/// 0.542 and 0.629.
pub(crate) const DISPLAY_CENTRE: f32 = 0.489;
pub(crate) const BODY_CENTRE: f32 = 0.563;
/// The code family's (JetBrains Mono, set in the console's cell): measured as
/// the others.
pub(crate) const CODE_CENTRE: f32 = 0.511;

/// Text in `family`, its letters centred on `rect`'s middle line (the
/// renderer otherwise sets a run's line box from its rectangle's top, which
/// left text high in its buttons and caps): the canvas's text, its family set
/// for the run.
#[allow(clippy::too_many_arguments)]
pub(crate) fn text(
    canvas: &mut MenuCanvas,
    family: TextFamily,
    value: std::fmt::Arguments<'_>,
    rect: Rect,
    size: f32,
    color: Color,
    weight: FontWeight,
    align: TextAlign,
) {
    let centre = match family {
        TextFamily::Display => DISPLAY_CENTRE,
        TextFamily::Code => CODE_CENTRE,
        TextFamily::Body => BODY_CENTRE,
    };
    let top = rect.y + rect.height * 0.5 - centre * size;
    // Tall enough for descenders, which the rectangle also clips.
    let height = (size * 1.3).max(rect.bottom() - top);
    canvas.set_family(family);
    canvas.text_fmt_aligned(
        value,
        Rect::new(rect.x, top, rect.width, height),
        size,
        color,
        weight,
        0.0,
        align,
    );
    canvas.set_family(TextFamily::Body);
}

/// One key cap and what it does, from `x` (window pixels) on the line whose
/// top is `y`; returns the x after it. `s` is the layout scale.
pub(crate) fn key_hint(
    canvas: &mut MenuCanvas,
    keys: &[&str],
    action: &str,
    x: f32,
    y: f32,
    s: f32,
) -> f32 {
    let height = 24.0 * s;
    let mut x = x;
    for key in keys {
        // Rajdhani Bold at 15 is about 8 pixels a character.
        let width = (18.0 + 8.2 * key.len() as f32) * s;
        let cap = Rect::new(x, y, width, height);
        let _ = canvas.draw_list_mut().push(DrawCommand::Border {
            rect: cap,
            radius: 5.0 * s,
            width: s.max(1.0),
            color: color::alpha(color::HOLO, 0.45),
        });
        text(
            canvas,
            TextFamily::Display,
            format_args!("{key}"),
            cap,
            17.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Center,
        );
        x += width + 6.0 * s;
    }
    let action_width = (12.0 + 7.6 * action.len() as f32) * s;
    text(
        canvas,
        TextFamily::Body,
        format_args!("{action}"),
        Rect::new(x + 4.0 * s, y, action_width, height),
        15.0 * s,
        color::MUTED,
        FontWeight::Regular,
        TextAlign::Start,
    );
    x + 4.0 * s + action_width
}

/// Width [`key_hint`] takes for `keys` and `action`, to right-align a row.
pub(crate) fn key_hint_width(keys: &[&str], action: &str, s: f32) -> f32 {
    let caps: f32 = keys
        .iter()
        .map(|key| (18.0 + 8.2 * key.len() as f32 + 6.0) * s)
        .sum();
    caps + 4.0 * s + (12.0 + 7.6 * action.len() as f32) * s
}

/// Where the top bar's way back starts and its middle line, in frame pixels.
const BAR_X: f32 = 96.0;
const BAR_Y: f32 = 87.0;
/// The search pill at the top bar's right end.
pub(crate) const SEARCH_PILL: [f32; 4] = [1404.0, 64.0, 420.0, 46.0];

/// A screen's search pill: the typed `query` (with its cursor while
/// `active`) or `prompt`, how many were `found` while a search is typed; it
/// answers to `token`.
pub(crate) struct SearchPill<'a> {
    pub(crate) query: &'a str,
    pub(crate) active: bool,
    pub(crate) prompt: &'a str,
    pub(crate) found: Option<usize>,
    pub(crate) token: u16,
}

/// A screen's top bar: the way back (an Esc key cap and `back`, answering to
/// `back_token`), the screen's `title`, and its search pill when it has one.
pub(crate) fn top_bar(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    back: &str,
    back_token: u16,
    title: &str,
    search: Option<SearchPill<'_>>,
) {
    let s = frame.s;
    let [x, y] = frame.point(BAR_X, BAR_Y - 12.0);
    let end = key_hint(canvas, &["Esc"], back, x, y, s);
    canvas.hit_region(back_token, Rect::new(x, y, end - x, 24.0 * s));
    let title_x = (end - frame.origin[0]) / s + 22.0;
    text(
        canvas,
        TextFamily::Display,
        format_args!("{title}"),
        frame.rect(title_x, BAR_Y - 30.0, 600.0, 60.0),
        48.0 * s,
        color::TEXT,
        FontWeight::Semibold,
        TextAlign::Start,
    );
    if let Some(search) = search {
        kit::search(
            canvas,
            frame,
            SEARCH_PILL,
            search.query,
            search.active,
            search.prompt,
            search.found,
            search.token,
        );
    }
}

/// The layout scale of `viewport`: 1 at 1080 lines.
pub(crate) fn scale(viewport: [f32; 2]) -> f32 {
    crate::ui_scale::height_scale(viewport[1])
}

/// A screen's 16:9 frame of 1080-line pixels in the window: its scale (window
/// pixels per frame pixel) and where its corner lies. A wider window shows more
/// map at the frame's sides; a narrower one scales the frame down to its width.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Frame {
    pub(crate) s: f32,
    pub(crate) origin: [f32; 2],
}

impl Frame {
    pub(crate) fn new(viewport: [f32; 2]) -> Self {
        let s = scale(viewport).min(viewport[0] / 1920.0);
        Self {
            s,
            origin: [
                (viewport[0] - 1920.0 * s) * 0.5,
                (viewport[1] - 1080.0 * s) * 0.5,
            ],
        }
    }

    /// Window point of frame point (`x`, `y`).
    pub(crate) fn point(&self, x: f32, y: f32) -> [f32; 2] {
        [self.origin[0] + x * self.s, self.origin[1] + y * self.s]
    }

    /// Window rectangle of a frame rectangle.
    pub(crate) fn rect(&self, x: f32, y: f32, width: f32, height: f32) -> Rect {
        let [x, y] = self.point(x, y);
        Rect::new(x, y, width * self.s, height * self.s)
    }

    /// The same frame moved `dx`, `dy` frame pixels, to draw a laid-out part
    /// of one screen at another place (Settings' rows in First setup's card).
    pub(crate) fn shifted(&self, dx: f32, dy: f32) -> Self {
        Self {
            s: self.s,
            origin: self.point(dx, dy),
        }
    }
}

/// `text` cut into lines of at most `chars` characters at spaces (a word longer
/// than a line stands alone), for running text the renderer would otherwise
/// cut mid-word.
pub(crate) fn wrap(text: &str, chars: usize) -> impl Iterator<Item = &str> {
    let mut rest = text.trim();
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        if rest.chars().count() <= chars {
            return Some(std::mem::take(&mut rest));
        }
        let limit = rest
            .char_indices()
            .nth(chars)
            .map_or(rest.len(), |(at, _)| at);
        let cut = rest[..limit]
            .rfind(' ')
            .filter(|cut| *cut > 0)
            .or_else(|| rest.find(' '))
            .unwrap_or(rest.len());
        let line = rest[..cut].trim_end();
        rest = rest[cut..].trim_start();
        Some(line)
    })
}

/// What the player's first blade (`color1`) is called, for the player's line
/// ("blue saber").
pub(crate) fn blade_name(console: &crate::console::ViewerConsole) -> &'static str {
    let index = console
        .text_value("color1")
        .map(str::trim)
        .and_then(|value| value.get(..1))
        .and_then(|digit| digit.parse::<u8>().ok())
        .unwrap_or(4);
    match sjk_client::SaberColor::from_index(index) {
        Ok(sjk_client::SaberColor::Red) => "red",
        Ok(sjk_client::SaberColor::Orange) => "orange",
        Ok(sjk_client::SaberColor::Yellow) => "yellow",
        Ok(sjk_client::SaberColor::Green) => "green",
        Ok(sjk_client::SaberColor::Purple) => "purple",
        Ok(sjk_client::SaberColor::Rgb) => "custom",
        _ => "blue",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_hints_measure_what_they_draw() {
        let mut canvas = MenuCanvas::new();
        canvas.begin_transparent([1920.0, 1080.0]);
        let end = key_hint(&mut canvas, &["Left", "Right"], "choose", 100.0, 900.0, 1.0);
        assert!((end - 100.0 - key_hint_width(&["Left", "Right"], "choose", 1.0)).abs() < 1e-3);
    }

    #[test]
    fn letters_centre_on_the_middle_of_their_rectangle() {
        for (family, centre) in [
            (&crate::text::DISPLAY, DISPLAY_CENTRE),
            (&crate::text::BODY, BODY_CENTRE),
        ] {
            let loaded = crate::text::load_family(family, 1.0, None).expect("a bundled family");
            let font = &loaded.font;
            let middle = |byte| {
                let glyph = font.glyph(crate::text::TextFace::Regular, byte);
                (glyph.offset_y + glyph.height * 0.5) / font.height
            };
            let measured = middle(b'H') * 0.75 + middle(b'x') * 0.25;
            assert!(
                (measured - centre).abs() < 0.005,
                "{measured} against {centre}"
            );
        }
        let code = crate::text::console_font::load()
            .expect("the console font")
            .font;
        let middle = |byte| {
            let glyph = code.glyph(crate::text::TextFace::Regular, byte);
            (glyph.offset_y + glyph.height * 0.5) / code.height
        };
        let measured = middle(b'H') * 0.75 + middle(b'x') * 0.25;
        assert!(
            (measured - CODE_CENTRE).abs() < 0.005,
            "{measured} against {CODE_CENTRE}"
        );
        // A run's line box is placed so that point lands on the middle.
        let mut canvas = MenuCanvas::new();
        canvas.begin_transparent([1920.0, 1080.0]);
        text(
            &mut canvas,
            TextFamily::Display,
            format_args!("Play"),
            Rect::new(0.0, 100.0, 200.0, 40.0),
            20.0,
            color::TEXT,
            FontWeight::Regular,
            TextAlign::Start,
        );
        let placed = canvas
            .draw_list()
            .commands()
            .iter()
            .find_map(|command| match command {
                DrawCommand::Text { rect, .. } => Some(*rect),
                _ => None,
            });
        let placed = placed.expect("the run");
        assert!((placed.y + DISPLAY_CENTRE * 20.0 - 120.0).abs() < 1e-3);
        assert!(placed.bottom() >= 140.0);
    }

    #[test]
    fn the_blade_follows_the_profile_saber() {
        let directory = tempfile::tempdir().unwrap();
        let mut console =
            crate::console::ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        console.set_cvar("color1", "1");
        assert_eq!(blade_name(&console), "orange");
        // A hat's suffix on the colour does not change it.
        console.set_cvar("color1", "3tophat");
        assert_eq!(blade_name(&console), "green");
        console.set_cvar("color1", "6");
        assert_eq!(blade_name(&console), "custom");
    }
}
