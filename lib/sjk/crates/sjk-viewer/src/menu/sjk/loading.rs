//! The SJK UI's loading screen (`docs/sjk-ui.md`, Loading): what a join shows
//! from the moment it starts until the game is on screen, and what a failed
//! one says.
//!
//! - Before the destination map is known, the menu's map stays on show (its
//!   camera keeps touring) under a compact block at the bottom left: who is
//!   being joined, the address and how far the join has got, as a thin gold
//!   line that fills by step and the step in words.
//! - Once the map is known, its levelshot fades in over the whole screen,
//!   cropped to the window and darkened towards the bottom, with the map's
//!   name large at the bottom left, the server and its rules under it and a
//!   gold load bar along the bottom.
//! - A failed join keeps the same layout and says what went wrong.
//!
//! It is the classic loading screen's state ([`ClassicLoading`]) drawn another
//! way. Positions are pixels of the SJK UI's 16:9 frame ([`Frame`]).

use super::{Frame, TextTarget, color, fade, fade_across, key_hint, key_hint_width, text, wrap};
use crate::menu::classic::loading::{
    CS_MESSAGE, CS_MOTD, ClassicLoading, MASTERY, Stage, WorldStage, config_text, server_info,
};
use crate::menu::levelshot::{Preview, screen_fit};
use crate::menu::{ClientMenu, ClientPhase};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use crate::server_browser::ServerEntry;
use crate::text::Plain;
use sjk_protocol::{GameState, InfoString};
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};
use std::fmt::{self, Write as _};

/// Seconds the levelshot takes to fade in.
const FADE: f64 = 0.45;
/// Where every line starts and the keys end.
const LEFT: f32 = 96.0;
const RIGHT: f32 = 1824.0;
/// The bottom row's top: the step in words on the left, the keys on the right.
const ROW_Y: f32 = 992.0;
/// The load bar along the bottom once the map is known: its top and thickness.
const BAR_Y: f32 = 1040.0;
const BAR_HEIGHT: f32 = 3.0;
/// The block's step line before the map is known: its top and length.
const LINE_Y: f32 = 962.0;
const LINE_WIDTH: f32 = 440.0;
/// The lowest line of the destination's facts ends here.
const FACTS_BOTTOM: f32 = 950.0;
/// The pointer token of the way out (the Esc key), as the classic screen's.
const BACK_TOKEN: u16 = 0;
/// Characters a line of the message of the day and of an error holds.
const MOTD_LINE: usize = 120;
const REASON_LINE: usize = 100;

/// One step of a join, in the order they come.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Step {
    /// No answer yet (`CA_CONNECTING`).
    Asking,
    /// Challenged, waiting for the connect answer.
    Connecting,
    /// Connected, waiting for the gamestate.
    GameState,
    /// The gamestate is in; waiting for the first snapshot and the spawn.
    Joining,
    /// The session's map is read.
    Map,
    /// The world is built for the GPU.
    Graphics,
    /// Built: the game comes on screen.
    Entering,
}

impl Step {
    const COUNT: u8 = 7;

    /// The step a load at `stage` shows: the connection's until the session is
    /// in hand (`joined`), then its world's.
    pub(crate) fn of(stage: Stage, world: Option<WorldStage>, joined: bool) -> Self {
        if joined {
            return match world {
                None | Some(WorldStage::Parsing) => Self::Map,
                Some(WorldStage::Building) => Self::Graphics,
                Some(WorldStage::Ready) => Self::Entering,
            };
        }
        match stage {
            Stage::Connecting => Self::Asking,
            Stage::Challenging => Self::Connecting,
            Stage::Connected => Self::GameState,
            Stage::Loading => Self::Joining,
        }
    }

    /// How much of the line or bar is gold at this step: the step itself
    /// counts, so the last fills it.
    pub(crate) fn fraction(self) -> f32 {
        f32::from(self as u8 + 1) / f32::from(Self::COUNT)
    }
}

/// The step in words: `map` is the destination without `mp/`; a download in
/// progress speaks for itself.
struct StepWords<'a> {
    step: Step,
    map: &'a str,
    download: Option<&'a str>,
}

impl fmt::Display for StepWords<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(download) = self.download {
            return formatter.write_str(download);
        }
        match self.step {
            Step::Asking => formatter.write_str("Asking the server"),
            Step::Connecting => formatter.write_str("Connecting"),
            Step::GameState => formatter.write_str("Receiving the game state"),
            Step::Joining => formatter.write_str("Joining the game"),
            Step::Map if self.map.is_empty() => formatter.write_str("Loading the map"),
            Step::Map => write!(formatter, "Loading {}", self.map),
            Step::Graphics => formatter.write_str("Preparing the graphics"),
            Step::Entering => formatter.write_str("Entering the game"),
        }
    }
}

/// What the gamestate says of the server and its game, as the screen shows
/// it; empty until the gamestate is in.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Facts {
    /// `sv_hostname`, with its colour codes.
    pub(crate) hostname: String,
    /// The map's own name from its worldspawn (`CS_MESSAGE`).
    pub(crate) title: String,
    /// The game type and its limits: "Free for all, 30 frags, 20 minutes".
    pub(crate) rules: String,
    /// The mod and how the server plays: "JA+ Mod v2.6 · Saber only".
    pub(crate) setup: String,
    /// The message of the day.
    pub(crate) motd: String,
}

impl Facts {
    /// Read `game`'s server info, system info, map name and message of the
    /// day, as `CG_DrawInformation` does.
    pub(crate) fn from_game(game: &GameState) -> Self {
        let info = server_info(game);
        let system = config_text(game, 1).and_then(|text| InfoString::parse(text).ok());
        let value = |key: &str| info.as_ref().and_then(|info| info.get(key)).unwrap_or("");
        let number = |key: &str| value(key).trim().parse::<i32>().unwrap_or(0);
        let gametype = number("g_gametype");
        let mut rules = String::from(mode_words(gametype));
        let mut limit = |value: i32, one: &str, many: &str| {
            if value > 0 {
                if !rules.is_empty() {
                    rules.push_str(", ");
                }
                let unit = if value == 1 { one } else { many };
                let _ = write!(rules, "{value} {unit}");
            }
        };
        if gametype != 7 && gametype < 8 {
            limit(number("fraglimit"), "frag", "frags");
            if gametype == 3 || gametype == 4 {
                limit(number("duel_fraglimit"), "duel win", "duel wins");
            }
        }
        if gametype >= 8 {
            limit(number("capturelimit"), "capture", "captures");
        }
        if gametype != 7 {
            limit(number("timelimit"), "minute", "minutes");
        }
        let mut setup = Vec::with_capacity(5);
        let mod_name = clean(value("gamename"));
        if !mod_name.is_empty() && !mod_name.eq_ignore_ascii_case("basejka") {
            setup.push(mod_name);
        }
        if gametype != 7 {
            let no_force = number("g_forcePowerDisable") != 0;
            if !no_force {
                let rank = number("g_maxForceRank");
                let rank = if (1..8).contains(&rank) { rank } else { 7 };
                setup.push(format!("Force mastery: {}", MASTERY[rank as usize]));
            }
            let weapons = if gametype == 3 || gametype == 4 {
                number("g_duelWeaponDisable")
            } else {
                number("g_weaponDisable")
            };
            if gametype != 2 && weapons != 0 {
                setup.push("Saber only".to_owned());
            }
            if no_force {
                setup.push("No Force powers".to_owned());
            }
        }
        if gametype >= 6 && number("g_forceBasedTeams") != 0 {
            setup.push("Force-based teams".to_owned());
        }
        if system
            .as_ref()
            .and_then(|system| system.get("sv_cheats"))
            .is_some_and(|cheats| cheats.starts_with('1'))
        {
            setup.push("Cheats on".to_owned());
        }
        Self {
            hostname: printable(value("sv_hostname")),
            title: clean(config_text(game, CS_MESSAGE).unwrap_or("")),
            rules,
            setup: setup.join(" \u{b7} "),
            motd: clean(config_text(game, CS_MOTD).unwrap_or("")),
        }
    }
}

/// A game type in words.
fn mode_words(gametype: i32) -> &'static str {
    match gametype {
        0 => "Free for all",
        1 => "Holocron",
        2 => "Jedi Master",
        3 => "Duel",
        4 => "Power duel",
        5 => "Cooperative",
        6 => "Team free for all",
        7 => "Siege",
        8 => "Capture the flag",
        9 => "Capture the ysalamiri",
        _ => "",
    }
}

/// `text` without control characters, trimmed; its colour codes kept.
fn printable(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_control())
        .collect::<String>()
        .trim()
        .to_owned()
}

/// `text` without its colour codes or control characters, trimmed.
fn clean(text: &str) -> String {
    printable(&Plain(text).to_string())
}

/// `text` with its first letter in capitals, for an error the network layer
/// words in lower case.
struct Capital<'a>(&'a str);

impl fmt::Display for Capital<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut characters = self.0.chars();
        match characters.next() {
            Some(first) => {
                write!(formatter, "{}", first.to_uppercase())?;
                formatter.write_str(characters.as_str())
            }
            None => Ok(()),
        }
    }
}

/// The screen's memory across frames: which join it shows, the furthest step
/// that join has reached and the levelshot's fade.
#[derive(Debug, Default)]
pub(crate) struct LoadingPage {
    /// The join or map load shown ([`ClassicLoading::generation`]).
    generation: u32,
    /// The furthest step shown for it, so the line never runs back (the
    /// menu world's preview of a map gives way to the session's own build).
    furthest: Option<Step>,
    /// The map whose levelshot is fading in, and since when (menu clock).
    shown: String,
    since: Option<f64>,
}

impl LoadingPage {
    /// The step to show for join `generation` now at `step`: never one before
    /// the furthest it has shown. A new join starts over.
    fn advance(&mut self, generation: u32, step: Step) -> Step {
        if self.generation != generation {
            self.generation = generation;
            self.furthest = None;
            self.shown.clear();
            self.since = None;
        }
        let step = self.furthest.map_or(step, |furthest| furthest.max(step));
        self.furthest = Some(step);
        step
    }

    /// How far `map`'s levelshot has faded in at `now`, once it is `ready`.
    fn fade(&mut self, map: &str, ready: bool, now: f64) -> f32 {
        if !ready || map.is_empty() {
            return 0.0;
        }
        let since = match self.since {
            Some(since) if self.shown == map => since,
            _ => {
                self.shown.clear();
                self.shown.push_str(map);
                self.since = Some(now);
                now
            }
        };
        ((now - since) / FADE).clamp(0.0, 1.0) as f32
    }

    /// Whether join `generation`'s levelshot of `map` covers the screen at
    /// `now`.
    fn covers(&self, generation: u32, map: &str, now: f64) -> bool {
        self.generation == generation
            && !map.is_empty()
            && self.shown == map
            && self.since.is_some_and(|since| now - since >= FADE)
    }
}

/// What the screen says of the game being joined, where the bottom row's step
/// would be.
enum Status<'a> {
    Step(StepWords<'a>),
    Failed(&'a str),
}

/// The destination's rules: the gamestate's, or the server list's before it.
enum Rules<'a> {
    None,
    Game(&'a str),
    Listed {
        mode: &'static str,
        players: u16,
        capacity: u16,
    },
}

/// Everything the screen shows this frame.
struct View<'a> {
    /// The small line over the name: "Joining", "Could not join"...
    kicker: &'static str,
    /// The server's name, with its colour codes.
    name: &'a str,
    /// The address, under the name while the map is unknown.
    address: Option<&'a str>,
    /// The map without `mp/`; empty while unknown.
    map: &'a str,
    title: &'a str,
    rules: Rules<'a>,
    setup: &'a str,
    motd: &'a str,
    status: Status<'a>,
    /// How much of the line or bar is gold.
    progress: f32,
    /// What Escape does.
    back: &'static str,
    /// The levelshot's opacity and texture coordinates, once it is in.
    picture: Option<(f32, Rect, [[f32; 2]; 4])>,
    /// The world is left out: the navy ground instead.
    ground: bool,
    /// The menu clock, for the activity mark.
    seconds: f64,
}

/// A map's name without the `mp/` every multiplayer map starts with.
fn short_map(map: &str) -> &str {
    map.strip_prefix("mp/").unwrap_or(map)
}

impl ClientMenu {
    /// Whether the SJK UI's loading screen is on show.
    pub(crate) fn sjk_loading_on_show(&self) -> bool {
        self.menu_style == crate::menu::style::MenuStyle::Sjk && self.is_loading_screen()
    }

    /// Whether the SJK UI's loading screen leaves the world out: always on a
    /// server's world (a map change has nothing of the menu map to show, and
    /// the joined world waits for the game), and on the menu's map once the
    /// destination's levelshot covers the screen.
    pub(crate) fn sjk_loading_hides_world(&self, menu_world: bool) -> bool {
        let map = self.loading.named_map();
        !menu_world
            || (self.create_game.levelshot_preview(map) == Preview::Image
                && self.sjk_loading.covers(
                    self.loading.generation(),
                    map,
                    crate::menu::art::motion::seconds(),
                ))
    }

    /// Draw the SJK UI's loading screen, its text to `target`.
    pub(crate) fn append_sjk_loading(&mut self, target: TextTarget<'_>, viewport: [f32; 2]) {
        self.build_sjk_loading(viewport, crate::menu::art::motion::seconds());
        target.append(&self.ui, viewport);
    }

    /// Lay the loading screen out on `self.ui` at menu clock `now`.
    fn build_sjk_loading(&mut self, viewport: [f32; 2], now: f64) {
        let local = self.hosting_local() || self.loading.is_local();
        let failed = matches!(self.state.phase(), ClientPhase::ConnectionError);
        let loading = &self.loading;
        let step = self.sjk_loading.advance(
            loading.generation(),
            Step::of(loading.stage(), loading.world(), loading.joined()),
        );
        let levelshot = loading.named_map();
        let ready = self.create_game.levelshot_preview(levelshot) == Preview::Image;
        let opacity = self.sjk_loading.fade(levelshot, ready, now);
        let picture = ready.then(|| {
            let size = self.create_game.levelshot_size(levelshot);
            let window = Rect::new(0.0, 0.0, viewport[0], viewport[1]);
            let (rect, uv) = screen_fit(size.unwrap_or([4, 3]), window);
            (opacity, rect, uv)
        });
        let address = loading.server();
        let entry = address
            .parse::<std::net::SocketAddr>()
            .ok()
            .and_then(|parsed| {
                self.browser
                    .entries()
                    .iter()
                    .find(|entry| entry.address == parsed)
            });
        let facts = loading.facts();
        let map = short_map(levelshot);
        let status = if failed {
            let reason = self.state.status();
            Status::Failed(
                reason
                    .strip_prefix("Connection failed: ")
                    .unwrap_or(reason)
                    .trim(),
            )
        } else {
            let download = match self.state.phase() {
                ClientPhase::Connecting(text) if text.starts_with("Downloading") => {
                    Some(text.as_str())
                }
                _ => None,
            };
            Status::Step(StepWords {
                step,
                map,
                download,
            })
        };
        let view = View {
            kicker: kicker(failed, local, loading),
            name: server_name(facts, entry, &self.recent, address, local),
            address: (!local && !address.is_empty()).then_some(address),
            map,
            title: &facts.title,
            rules: rules(facts, entry),
            setup: &facts.setup,
            motd: &facts.motd,
            status,
            progress: step.fraction(),
            back: if failed {
                "back to servers"
            } else if loading.is_map_change() && !local {
                "leave the server"
            } else {
                "cancel"
            },
            picture,
            ground: self.world_hidden,
            seconds: now,
        };
        draw(&mut self.ui, viewport, &view);
    }
}

/// The small line over the server's or map's name.
fn kicker(failed: bool, local: bool, loading: &ClassicLoading) -> &'static str {
    match (failed, local) {
        (true, _) if loading.joined() => "Connection lost",
        (true, _) => "Could not join",
        (false, true) => "Starting your game",
        (false, false) if loading.is_map_change() => "Next map",
        (false, false) => "Joining",
    }
}

/// The server's name with its colour codes: the gamestate's, else the server
/// list's or the recent servers', else its address.
fn server_name<'a>(
    facts: &'a Facts,
    entry: Option<&'a ServerEntry>,
    recent: &'a super::recent::RecentServers,
    address: &'a str,
    local: bool,
) -> &'a str {
    if !facts.hostname.is_empty() {
        return &facts.hostname;
    }
    if local {
        return "Your game";
    }
    if let Some(entry) = entry.filter(|entry| !entry.name.trim().is_empty()) {
        return &entry.name;
    }
    recent
        .servers()
        .iter()
        .find(|server| {
            !server.name.is_empty() && super::recent::same_address(&server.address, address)
        })
        .map_or(address, |server| server.name.as_str())
}

/// The destination's rules: the gamestate's, or the server list's game type
/// and players before it is in.
fn rules<'a>(facts: &'a Facts, entry: Option<&'a ServerEntry>) -> Rules<'a> {
    if !facts.rules.is_empty() {
        return Rules::Game(&facts.rules);
    }
    match entry {
        Some(entry) => Rules::Listed {
            mode: mode_words(entry.mode.unwrap_or(-1)),
            players: entry.players,
            capacity: entry.capacity,
        },
        None => Rules::None,
    }
}

fn push(canvas: &mut MenuCanvas, command: DrawCommand) {
    let _ = canvas.draw_list_mut().push(command);
}

/// The whole screen: the ground, the levelshot, the shade, the block or the
/// destination, the bottom row.
fn draw(ui: &mut MenuCanvas, viewport: [f32; 2], view: &View<'_>) {
    let frame = Frame::new(viewport);
    let window = Rect::new(0.0, 0.0, viewport[0], viewport[1]);
    ui.begin_transparent(viewport);
    if view.ground {
        push(
            ui,
            DrawCommand::SolidRect {
                rect: window,
                color: color::SPACE,
            },
        );
    }
    if let Some((opacity, rect, uv)) = view.picture.filter(|(opacity, ..)| *opacity > 0.0) {
        ui.push_opacity(opacity);
        // A picture wider than the window leaves bands: navy, not the map.
        if rect != window {
            push(
                ui,
                DrawCommand::SolidRect {
                    rect: window,
                    color: color::SPACE,
                },
            );
        }
        push(
            ui,
            DrawCommand::TexturedQuadUv {
                rect,
                texture: crate::ui_renderer::LEVELSHOT_TEXTURE,
                color: Color::new(1.0, 1.0, 1.0, 1.0),
                uv,
            },
        );
        ui.pop_opacity();
    }
    if view.map.is_empty() {
        shade_corner(ui, viewport);
        draw_block(ui, &frame, view);
    } else {
        shade_picture(ui, viewport);
        draw_destination(ui, &frame, view);
    }
    draw_row(ui, &frame, view);
    ui.finish(BACK_TOKEN);
}

/// The dark the block sits in: the bottom of the screen, deepest at the left.
fn shade_corner(ui: &mut MenuCanvas, viewport: [f32; 2]) {
    let [width, height] = viewport;
    let space = |alpha| color::alpha(color::SPACE, alpha);
    fade(
        ui,
        Rect::new(0.0, height * 0.45, width, height * 0.55),
        space(0.0),
        space(0.85),
    );
    fade_across(
        ui,
        Rect::new(0.0, height * 0.45, width * 0.6, height * 0.55),
        space(0.55),
        space(0.0),
    );
}

/// The levelshot darkened so text stays legible over a bright one: a little
/// all over, more towards the bottom and its left, a touch at the top.
fn shade_picture(ui: &mut MenuCanvas, viewport: [f32; 2]) {
    let [width, height] = viewport;
    let space = |alpha| color::alpha(color::SPACE, alpha);
    push(
        ui,
        DrawCommand::SolidRect {
            rect: Rect::new(0.0, 0.0, width, height),
            color: space(0.18),
        },
    );
    fade(
        ui,
        Rect::new(0.0, 0.0, width, height * 0.16),
        space(0.45),
        space(0.0),
    );
    fade(
        ui,
        Rect::new(0.0, height * 0.38, width, height * 0.62),
        space(0.0),
        space(0.92),
    );
    fade_across(
        ui,
        Rect::new(0.0, height * 0.5, width * 0.62, height * 0.5),
        space(0.5),
        space(0.0),
    );
}

/// Before the map is known: who is being joined, its address, the step line.
fn draw_block(ui: &mut MenuCanvas, frame: &Frame, view: &View<'_>) {
    let s = frame.s;
    let failed = matches!(view.status, Status::Failed(_));
    text(
        ui,
        TextFamily::Display,
        format_args!("{}", view.kicker),
        frame.rect(LEFT, 812.0, 900.0, 30.0),
        24.0 * s,
        if failed { color::TEXT } else { color::MUTED },
        FontWeight::Regular,
        TextAlign::Start,
    );
    text(
        ui,
        TextFamily::Display,
        format_args!("{}", view.name),
        frame.rect(LEFT, 842.0, 1400.0, 66.0),
        56.0 * s,
        color::TEXT,
        FontWeight::Semibold,
        TextAlign::Start,
    );
    if let Some(address) = view.address {
        text(
            ui,
            TextFamily::Body,
            format_args!("{address}"),
            frame.rect(LEFT + 2.0, 912.0, 700.0, 28.0),
            18.0 * s,
            color::HOLO,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }
    if !failed {
        line(ui, frame, [LEFT, LINE_Y, LINE_WIDTH], view.progress);
    }
}

/// Once the map is known: the map's name large, its own name, the server, its
/// rules, setup and message of the day, stacked up from the bottom row.
fn draw_destination(ui: &mut MenuCanvas, frame: &Frame, view: &View<'_>) {
    let s = frame.s;
    let mut y = FACTS_BOTTOM;
    let body = |ui: &mut MenuCanvas, y: f32, value: fmt::Arguments<'_>, colour: Color| {
        text(
            ui,
            TextFamily::Body,
            value,
            frame.rect(LEFT + 2.0, y, 1500.0, 28.0),
            18.0 * s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
    };
    // The message of the day takes up to two lines.
    y -= 28.0 * wrap(view.motd, MOTD_LINE).take(2).count() as f32;
    for (index, part) in wrap(view.motd, MOTD_LINE).take(2).enumerate() {
        body(
            ui,
            y + index as f32 * 28.0,
            format_args!("{part}"),
            color::alpha(color::TEXT, 0.88),
        );
    }
    if !view.setup.is_empty() {
        y -= 28.0;
        body(ui, y, format_args!("{}", view.setup), color::MUTED);
    }
    match view.rules {
        Rules::None => {}
        Rules::Game(rules) => {
            y -= 28.0;
            body(ui, y, format_args!("{rules}"), color::MUTED);
        }
        Rules::Listed {
            mode,
            players,
            capacity,
        } => {
            y -= 28.0;
            let comma = if mode.is_empty() { "" } else { ", " };
            body(
                ui,
                y,
                format_args!("{mode}{comma}{players} of {capacity} playing"),
                color::MUTED,
            );
        }
    }
    y -= 12.0 + 46.0;
    text(
        ui,
        TextFamily::Display,
        format_args!("{}", view.name),
        frame.rect(LEFT, y, 1500.0, 46.0),
        36.0 * s,
        color::TEXT,
        FontWeight::Semibold,
        TextAlign::Start,
    );
    // The map's own name large ("Tatooine FFA"), its file name ("ffa3") small
    // under it; the file name large when the map names itself nothing.
    let (large, small) = if view.title.is_empty() {
        (view.map, "")
    } else {
        (view.title, view.map)
    };
    if !small.is_empty() {
        y -= 34.0;
        text(
            ui,
            TextFamily::Display,
            format_args!("{small}"),
            frame.rect(LEFT + 2.0, y, 1200.0, 34.0),
            26.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }
    y -= 104.0;
    // Rajdhani SemiBold sets about half its size a character: a long name
    // steps down so it stays on the line.
    let size = if large.chars().count() > 18 {
        96.0
    } else {
        124.0
    };
    text(
        ui,
        TextFamily::Display,
        format_args!("{large}"),
        frame.rect(LEFT - 4.0, y, 1700.0, 104.0),
        size * s,
        color::TEXT,
        FontWeight::Semibold,
        TextAlign::Start,
    );
    y -= 32.0;
    text(
        ui,
        TextFamily::Display,
        format_args!("{}", view.kicker),
        frame.rect(LEFT, y, 900.0, 32.0),
        26.0 * s,
        if matches!(view.status, Status::Failed(_)) {
            color::TEXT
        } else {
            color::MUTED
        },
        FontWeight::Regular,
        TextAlign::Start,
    );
    if matches!(view.status, Status::Step(_)) {
        line(ui, frame, [LEFT, BAR_Y, RIGHT - LEFT], view.progress);
    }
}

/// A thin line from (`x`, `y`), `width` long: holo, gold up to `progress`.
fn line(ui: &mut MenuCanvas, frame: &Frame, at: [f32; 3], progress: f32) {
    let [x, y, width] = at;
    push(
        ui,
        DrawCommand::SolidRect {
            rect: frame.rect(x, y, width, BAR_HEIGHT),
            color: color::alpha(color::HOLO, 0.22),
        },
    );
    let lit = width * progress.clamp(0.0, 1.0);
    if lit > 0.0 {
        push(
            ui,
            DrawCommand::RoundedRect {
                rect: frame.rect(x, y, lit, BAR_HEIGHT),
                radius: BAR_HEIGHT * 0.5 * frame.s,
                color: color::GOLD,
            },
        );
    }
}

/// The bottom row: the turning mark and the step in words (or what went
/// wrong) on the left, the way out on the right.
fn draw_row(ui: &mut MenuCanvas, frame: &Frame, view: &View<'_>) {
    let s = frame.s;
    match &view.status {
        Status::Step(words) => {
            activity(ui, frame, LEFT + 10.0, ROW_Y + 12.0, view.seconds);
            text(
                ui,
                TextFamily::Body,
                format_args!("{words}"),
                frame.rect(LEFT + 32.0, ROW_Y, 1100.0, 24.0),
                18.0 * s,
                color::TEXT,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        Status::Failed(reason) => {
            // Up to two lines, the last on the row.
            let lines = wrap(reason, REASON_LINE).take(2).count().max(1);
            let top = ROW_Y - 32.0 * (lines - 1) as f32;
            for (index, part) in wrap(reason, REASON_LINE).take(2).enumerate() {
                text(
                    ui,
                    TextFamily::Body,
                    format_args!("{}", Capital(part)),
                    frame.rect(LEFT + 2.0, top + index as f32 * 32.0, 1300.0, 24.0),
                    22.0 * s,
                    color::TEXT,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
            }
        }
    }
    let width = key_hint_width(&["Esc"], view.back, s);
    let [right, y] = frame.point(RIGHT, ROW_Y);
    let x = right - width;
    let end = key_hint(ui, &["Esc"], view.back, x, y, s);
    ui.hit_region(BACK_TOKEN, Rect::new(x, y, end - x, 24.0 * s));
}

/// The activity mark centred on (`x`, `y`): a faint ring with a gold arc
/// slowly turning round it (the report card's while it sends, too).
pub(crate) fn activity(ui: &mut MenuCanvas, frame: &Frame, x: f32, y: f32, seconds: f64) {
    let s = frame.s;
    let center = frame.point(x, y);
    push(
        ui,
        DrawCommand::Arc {
            center,
            radius: 8.0 * s,
            width: 2.0 * s,
            start: 0.0,
            sweep: std::f32::consts::TAU,
            color: color::alpha(color::HOLO, 0.22),
            knockout: None,
        },
    );
    // A turn every three seconds.
    let start = (seconds * std::f64::consts::TAU / 3.0).rem_euclid(std::f64::consts::TAU) as f32;
    push(
        ui,
        DrawCommand::Arc {
            center,
            radius: 8.0 * s,
            width: 2.2 * s,
            start,
            sweep: std::f32::consts::FRAC_PI_2 * 1.2,
            color: color::GOLD_BRIGHT,
            knockout: None,
        },
    );
}

// The block and the destination's facts end above the bottom row, which ends
// above the bar, inside the frame.
const _: () = assert!(LINE_Y + BAR_HEIGHT < ROW_Y - 20.0);
const _: () = assert!(FACTS_BOTTOM < ROW_Y - 20.0);
const _: () = assert!(ROW_Y + 24.0 < BAR_Y && BAR_Y + BAR_HEIGHT < 1080.0);

#[cfg(test)]
impl ClientMenu {
    /// Show the loading screen for the world shots and tests: a join of the
    /// JoF server listed in the browser (its name and players known), at
    /// `stage`, with mp/ffa3 known as the map and the gamestate in when
    /// `map`, its world at `world` once `joined`; `error` fails it. The
    /// levelshot's fade is over.
    pub(crate) fn loading_for_shot(
        &mut self,
        stage: Stage,
        map: bool,
        world: Option<WorldStage>,
        joined: bool,
        error: Option<&str>,
    ) {
        const ADDRESS: &str = "135.125.145.49:29070";
        self.browser.list_for_test(
            vec![ServerEntry::for_test(
                ADDRESS,
                "^4JoF ^7FFA & duels",
                "mp/ffa3",
                18,
                32,
                24,
                0,
                sjk_client::CompatProfile::JaPlus { version: None },
                false,
            )],
            &[],
        );
        self.state_connecting(ADDRESS.to_owned());
        self.begin_join(ADDRESS);
        if map {
            self.loading
                .set_game(&test_game(), false, &std::collections::HashMap::new());
        }
        self.loading.hold_for_test(stage, world, joined);
        if let Some(error) = error {
            self.join_failed(error);
        }
        self.past_loading_fade();
    }

    /// Move the join on show to a server map change whose map is not named
    /// yet, the session in hand, as a server's change of map starts.
    pub(crate) fn map_change_for_shot(&mut self) {
        self.state_loading("next map");
        self.loading
            .hold_for_test(Stage::Loading, Some(WorldStage::Parsing), true);
        self.past_loading_fade();
    }

    /// Put the levelshot's fade behind the screen on show.
    fn past_loading_fade(&mut self) {
        self.sjk_loading
            .advance(self.loading.generation(), Step::Asking);
        self.sjk_loading.shown = self.loading.named_map().to_owned();
        self.sjk_loading.since = Some(-1.0e9);
    }

    /// The loading screen's levelshot is in the levelshot texture (or there is
    /// none, or no map is known).
    pub(crate) fn loading_picture_settled(&self) -> bool {
        let map = self.loading.named_map();
        map.is_empty() || self.create_game.levelshot_preview(map) != Preview::Loading
    }
}

/// A gamestate as the JoF server's might be: JA+ on mp/ffa3, free for all
/// to 30 frags or 20 minutes, with a message of the day.
#[cfg(test)]
fn test_game() -> GameState {
    let mut game = GameState::empty_local(0);
    let mut set = |index: usize, text: &str| {
        game.replace_config_string(index, text.as_bytes().to_vec())
            .expect("a valid config string");
    };
    set(
        0,
        "\\sv_hostname\\^4JoF ^7FFA & duels\\mapname\\mp/ffa3\\g_gametype\\0\\fraglimit\\30\\timelimit\\20\\gamename\\JA+ Mod v2.6\\g_maxForceRank\\7",
    );
    set(1, "\\sv_pure\\1");
    set(CS_MESSAGE, "Tatooine FFA");
    set(
        CS_MOTD,
        "^3Welcome to JoF!^7 Duels on the bridge, be kind, and have fun.",
    );
    game
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::ViewerConsole;

    const VIEWPORTS: [[f32; 2]; 4] = [
        [1_920.0, 1_080.0],
        [3_840.0, 2_160.0],
        [2_560.0, 1_080.0],
        [1_280.0, 1_024.0],
    ];

    fn menu() -> (ClientMenu, ViewerConsole, tempfile::TempDir) {
        let directory = tempfile::tempdir().expect("a profile");
        let console = ViewerConsole::new(directory.path().join("config.cfg")).expect("a console");
        let mut menu = ClientMenu::new(true, String::new());
        menu.menu_style = crate::menu::style::MenuStyle::Sjk;
        (menu, console, directory)
    }

    #[test]
    fn steps_follow_the_join_and_fill_the_line() {
        use Stage::{Challenging, Connected, Connecting, Loading};
        assert_eq!(Step::of(Connecting, None, false), Step::Asking);
        assert_eq!(Step::of(Challenging, None, false), Step::Connecting);
        assert_eq!(Step::of(Connected, None, false), Step::GameState);
        // A preview of the map built before the session counts for nothing.
        assert_eq!(
            Step::of(Loading, Some(WorldStage::Ready), false),
            Step::Joining
        );
        assert_eq!(Step::of(Loading, None, true), Step::Map);
        assert_eq!(
            Step::of(Loading, Some(WorldStage::Building), true),
            Step::Graphics
        );
        assert_eq!(
            Step::of(Loading, Some(WorldStage::Ready), true),
            Step::Entering
        );
        assert!(Step::Asking.fraction() > 0.0);
        assert!(Step::Map.fraction() > Step::Joining.fraction());
        assert_eq!(Step::Entering.fraction(), 1.0);
        let words = |step, download| {
            StepWords {
                step,
                map: "ffa3",
                download,
            }
            .to_string()
        };
        assert_eq!(words(Step::Map, None), "Loading ffa3");
        assert_eq!(
            words(Step::Asking, Some("Downloading jof.pk3: 12 / 900 KiB")),
            "Downloading jof.pk3: 12 / 900 KiB"
        );
    }

    #[test]
    fn the_line_never_runs_back_within_a_join() {
        let mut page = LoadingPage::default();
        assert_eq!(page.advance(1, Step::Graphics), Step::Graphics);
        // The session's own build starts over after a preview.
        assert_eq!(page.advance(1, Step::Map), Step::Graphics);
        assert_eq!(page.advance(1, Step::Entering), Step::Entering);
        // The next join starts from its own first step.
        assert_eq!(page.advance(2, Step::Asking), Step::Asking);
    }

    #[test]
    fn the_levelshot_fades_in_once_it_is_ready() {
        let mut page = LoadingPage::default();
        page.advance(1, Step::Asking);
        assert_eq!(page.fade("mp/ffa3", false, 10.0), 0.0);
        assert!(!page.covers(1, "mp/ffa3", 10.0));
        assert_eq!(page.fade("mp/ffa3", true, 10.0), 0.0);
        let half = page.fade("mp/ffa3", true, 10.0 + FADE * 0.5);
        assert!((half - 0.5).abs() < 1e-4);
        assert!(!page.covers(1, "mp/ffa3", 10.0 + FADE * 0.5));
        assert_eq!(page.fade("mp/ffa3", true, 11.0), 1.0);
        assert!(page.covers(1, "mp/ffa3", 11.0));
        // Another map, or a new join of the same map, fades in again.
        assert_eq!(page.fade("mp/duel6", true, 12.0), 0.0);
        assert!(!page.covers(2, "mp/duel6", 20.0));
    }

    #[test]
    fn facts_read_the_gamestate_in_words() {
        let facts = Facts::from_game(&test_game());
        assert_eq!(facts.hostname, "^4JoF ^7FFA & duels");
        assert_eq!(facts.title, "Tatooine FFA");
        assert_eq!(facts.rules, "Free for all, 30 frags, 20 minutes");
        assert_eq!(
            facts.setup,
            "JA+ Mod v2.6 \u{b7} Force mastery: Jedi Master"
        );
        assert_eq!(
            facts.motd,
            "Welcome to JoF! Duels on the bridge, be kind, and have fun."
        );
        let mut duel = GameState::empty_local(0);
        duel.replace_config_string(
            0,
            b"\\g_gametype\\3\\fraglimit\\1\\duel_fraglimit\\5\\timelimit\\0\\g_duelWeaponDisable\\1\\gamename\\basejka\\g_forcePowerDisable\\1".to_vec(),
        )
        .expect("a valid config string");
        let facts = Facts::from_game(&duel);
        assert_eq!(facts.rules, "Duel, 1 frag, 5 duel wins");
        assert_eq!(facts.setup, "Saber only \u{b7} No Force powers");
        assert!(facts.hostname.is_empty() && facts.motd.is_empty());
        let mut siege = GameState::empty_local(0);
        siege
            .replace_config_string(0, b"\\g_gametype\\7\\timelimit\\30".to_vec())
            .expect("a valid config string");
        assert_eq!(Facts::from_game(&siege).rules, "Siege");
        assert_eq!(Capital("server is full").to_string(), "Server is full");
    }

    #[test]
    fn the_names_come_from_the_gamestate_then_the_list() {
        let (mut menu, _console, _profile) = menu();
        menu.loading_for_shot(Stage::Challenging, false, None, false, None);
        assert!(menu.sjk_screen() && menu.sjk_loading_on_show());
        let facts = Facts::default();
        let entry = menu.browser.entries().first();
        let address = "135.125.145.49:29070";
        assert_eq!(
            server_name(&facts, entry, &menu.recent, address, false),
            "^4JoF ^7FFA & duels"
        );
        assert_eq!(
            server_name(&facts, None, &menu.recent, address, false),
            address
        );
        assert_eq!(
            server_name(&facts, None, &menu.recent, "localhost", true),
            "Your game"
        );
        let game = Facts::from_game(&test_game());
        assert!(matches!(rules(&game, entry), Rules::Game(_)));
        assert!(matches!(
            rules(&facts, entry),
            Rules::Listed {
                mode: "Free for all",
                players: 18,
                capacity: 32
            }
        ));
        assert_eq!(kicker(false, false, &menu.loading), "Joining");
        assert_eq!(kicker(true, false, &menu.loading), "Could not join");
        assert_eq!(kicker(false, true, &menu.loading), "Starting your game");
    }

    /// A made-up join: its stage, whether the map and gamestate are in, the
    /// session's world, whether the session is in hand and its error.
    type Join = (Stage, bool, Option<WorldStage>, bool, Option<&'static str>);

    #[test]
    fn every_state_fits_the_canvas_with_its_way_out() {
        let states: [Join; 5] = [
            (Stage::Connecting, false, None, false, None),
            (Stage::Loading, true, Some(WorldStage::Building), true, None),
            (Stage::Loading, true, None, false, Some("server is full")),
            (
                Stage::Challenging,
                false,
                None,
                false,
                Some(
                    "timed out waiting for the server to answer the challenge after five tries, which usually means the server is down or a firewall drops the reply",
                ),
            ),
            (Stage::Loading, true, Some(WorldStage::Ready), true, None),
        ];
        for (stage, map, world, joined, error) in states {
            for viewport in VIEWPORTS {
                let (mut menu, _console, _profile) = menu();
                menu.loading_for_shot(stage, map, world, joined, error);
                menu.build_sjk_loading(viewport, 3.0);
                assert!(
                    menu.ui.rect_for(BACK_TOKEN).is_some(),
                    "{stage:?} {viewport:?}"
                );
                assert!(!menu.ui.overflowed(), "{stage:?} {viewport:?}");
            }
        }
    }

    #[test]
    fn only_the_key_cap_leaves() {
        let (mut menu, _console, _profile) = menu();
        menu.loading_for_shot(Stage::Connected, false, None, false, None);
        menu.build_sjk_loading([1920.0, 1080.0], 0.0);
        // The classic screen's whole-screen target is the Esc key cap here,
        // bottom right, so a stray click does not end the join.
        let cap = menu.ui.rect_for(BACK_TOKEN).expect("the way out");
        assert!(cap.width < 300.0 && cap.x > 1400.0 && cap.y > 950.0);
    }

    #[test]
    fn a_change_to_the_next_map_has_no_map_yet() {
        let (mut menu, _console, _profile) = menu();
        menu.loading_for_shot(Stage::Loading, true, None, true, None);
        assert_eq!(menu.loading.named_map(), "mp/ffa3");
        menu.map_change_for_shot();
        // The classic screen keeps the last map; this one waits for the new.
        assert_eq!(menu.loading.map(), "mp/ffa3");
        assert_eq!(menu.loading.named_map(), "");
        assert_eq!(kicker(false, false, &menu.loading), "Next map");
        menu.build_sjk_loading([1920.0, 1080.0], 1.0);
        assert!(!menu.ui.overflowed());
        // The gamestate names it.
        menu.loading.set_map("maps/mp/duel6.bsp");
        assert_eq!(menu.loading.named_map(), "mp/duel6");
    }

    #[test]
    fn the_world_shows_until_the_levelshot_covers_it() {
        let (mut menu, _console, _profile) = menu();
        menu.loading_for_shot(Stage::Loading, true, None, false, None);
        // The menu's map stays while no levelshot is in; a server's world
        // never shows.
        assert!(!menu.sjk_loading_hides_world(true));
        assert!(menu.sjk_loading_hides_world(false));
    }
}
