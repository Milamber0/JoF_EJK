//! The classic connect and loading screens.
//!
//! Before the server's gamestate arrives this is retail's connect screen
//! (`ui/jamp/connect.menu` and `UI_DrawConnectScreen`, `ui_main.c`): the
//! `menu/art/unknownmap_mp` background, "Connecting to <server>" and the
//! connection state. Once the gamestate is known it is cgame's information
//! screen (`CG_DrawInformation` and `CG_LoadBar`, `cg_info.c`): the map's
//! levelshot over the whole screen, the loading line, the server and rules
//! lines and the LED bar along the bottom. Both are laid out on the 640x480
//! canvas; the backgrounds cover a wide window the way cgame's levelshot
//! does, cropping top and bottom.

use super::layout::{CANVAS, Placement};
use super::view::art;
use crate::log::TimelinePhase;
use crate::menu::art::{ArtPiece, ArtSet};
use crate::menu::levelshot::Preview;
use crate::menu_widgets::MenuCanvas;
use crate::ui_renderer::LEVELSHOT_TEXTURE;
use sjk_protocol::{GameState, InfoString};
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};
use std::collections::HashMap;

/// `CS_MESSAGE`: the map's long name from its worldspawn.
pub(crate) const CS_MESSAGE: usize = 3;
/// `CS_MOTD`: the server's `g_motd`.
pub(crate) const CS_MOTD: usize = 4;
/// `iPropHeight`: the information lines' pitch on the canvas.
const LINE: f32 = 18.0;
/// `CG_LoadBar`'s tick count.
pub(crate) const TICKS: u8 = 9;
const WHITE: Color = Color::new(1.0, 1.0, 1.0, 1.0);
const SHADOW: Color = Color::new(0.0, 0.0, 0.0, 0.85);

/// How far the connection has got, as the connect screen words it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Stage {
    /// `CA_CONNECTING`: no challenge yet.
    #[default]
    Connecting,
    /// `CA_CHALLENGING`: challenged, waiting for the connect answer.
    Challenging,
    /// `CA_CONNECTED`: waiting for the gamestate.
    Connected,
    /// The gamestate is in: the information screen shows.
    Loading,
}

/// How far the world being loaded is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorldStage {
    /// The map and its content are read and parsed.
    Parsing,
    /// The world is built for the GPU.
    Building,
    /// Built; waiting for the session.
    Ready,
}

/// The loading line's subject and lit ticks for a load at `stage`, with
/// `joined` once the session is in: one tick for the gamestate, then three
/// per world stage, the last when the game can start. Retail advanced its
/// bar through cgame's registration steps instead (`cg.loadLCARSStage`).
pub(crate) fn progress(map: &str, stage: Option<WorldStage>, joined: bool) -> (String, u8) {
    let (subject, ticks) = match stage {
        None => (String::new(), 1),
        Some(WorldStage::Parsing) => (map.to_owned(), 3),
        Some(WorldStage::Building) => ("graphics".to_owned(), 5),
        Some(WorldStage::Ready) => (String::new(), 7),
    };
    (subject, if joined { ticks + 1 } else { ticks })
}

/// One information line and the extra space retail leaves above it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InfoLine {
    pub(crate) text: String,
    pub(crate) gap: f32,
}

/// What the classic loading screen shows for the join or map load in
/// progress.
#[derive(Debug, Default)]
pub(crate) struct ClassicLoading {
    /// The address being joined, as typed or picked (`cstate.servername`).
    server: String,
    /// The client hosts the server it joins (retail's `localhost`).
    local: bool,
    /// `mp/ffa3`: the levelshot key, once the map is known.
    map: String,
    stage: Stage,
    /// `CG_DrawInformation`'s lines from the gamestate.
    info: Vec<InfoLine>,
    /// What is being loaded, for "Loading... %s"; empty shows "Awaiting
    /// snapshot...".
    loading: String,
    /// Lit ticks of the bar, `0..=TICKS`.
    ticks: u8,
    /// How far the session's own world is (not a preview of the map built
    /// before the session named its content), for the SJK UI's steps.
    world: Option<WorldStage>,
    /// The session is in hand: joined, or a map change on a server.
    joined: bool,
    /// The load is a map change or map load rather than a join.
    map_change: bool,
    /// This load named its map: a map change to the "next map" leaves the
    /// last one in `map`, which the classic screen keeps showing.
    named: bool,
    /// Counts the joins and map loads, so a screen can tell a new one.
    generation: u32,
    /// What the SJK UI's loading screen shows of the gamestate.
    facts: crate::menu::sjk::loading::Facts,
    /// A world shot's made-up progress, which the frame's own does not
    /// replace.
    #[cfg(test)]
    pub(crate) held: bool,
}

impl ClassicLoading {
    /// Start over for a join of `server`; `local` when the client hosts it.
    pub(crate) fn begin(&mut self, server: &str, local: bool) {
        self.server.clear();
        self.server.push_str(server);
        self.local = local;
        self.map.clear();
        self.stage = Stage::Connecting;
        self.info.clear();
        self.loading.clear();
        self.ticks = 0;
        self.world = None;
        self.joined = false;
        self.map_change = false;
        self.named = false;
        self.generation = self.generation.wrapping_add(1);
        self.facts = Default::default();
    }

    /// A server map change or map load: the gamestate is the session's, so
    /// the information screen shows from the start.
    pub(crate) fn begin_map_load(&mut self, map: &str) {
        self.stage = Stage::Loading;
        self.named = false;
        self.set_map(map);
        self.ticks = 0;
        self.world = None;
        self.map_change = true;
        self.generation = self.generation.wrapping_add(1);
    }

    /// The map, in any of `mp/ffa3`, `maps/mp/ffa3.bsp` or `next map` form;
    /// anything that does not name a map leaves it unknown.
    pub(crate) fn set_map(&mut self, map: &str) {
        let key = levelshot_key(map);
        if !key.is_empty() {
            self.map.clear();
            self.map.push_str(&key);
            self.named = true;
        }
    }

    /// The map this join or map load named (`mp/ffa3`); empty until it has
    /// named one, where [`Self::map`] may still hold the last load's.
    pub(crate) fn named_map(&self) -> &str {
        if self.named { &self.map } else { "" }
    }

    /// The levelshot this screen wants, if the map is known.
    pub(crate) fn map(&self) -> &str {
        &self.map
    }

    /// The join worker reached `phase`.
    pub(crate) fn set_phase(&mut self, phase: TimelinePhase) {
        let stage = match phase {
            TimelinePhase::Challenge => Stage::Challenging,
            TimelinePhase::Connected => Stage::Connected,
            TimelinePhase::Gamestate | TimelinePhase::FirstSnapshot => Stage::Loading,
            _ => return,
        };
        if stage as u8 > self.stage as u8 {
            self.stage = stage;
        }
    }

    /// The gamestate arrived: build the information lines from it. `local`
    /// leaves out the server lines, as retail does while `sv_running`.
    pub(crate) fn set_game(
        &mut self,
        game: &GameState,
        local: bool,
        strings: &HashMap<String, String>,
    ) {
        self.stage = Stage::Loading;
        self.info = information(game, local, strings);
        self.facts = crate::menu::sjk::loading::Facts::from_game(game);
        if let Some(map) = server_info(game).and_then(|info| info.get("mapname").map(str::to_owned))
        {
            self.set_map(&map);
        }
    }

    /// The loading line's subject and the bar's lit ticks this frame.
    pub(crate) fn set_progress(&mut self, loading: &str, ticks: u8) {
        #[cfg(test)]
        if self.held {
            return;
        }
        if self.loading != loading {
            self.loading.clear();
            self.loading.push_str(loading);
        }
        self.ticks = ticks.min(TICKS);
    }

    /// How far the session's own world is this frame (`None` before it
    /// starts, or while only a preview of the map loads) and whether the
    /// session is in hand.
    pub(crate) fn set_world(&mut self, world: Option<WorldStage>, joined: bool) {
        #[cfg(test)]
        if self.held {
            return;
        }
        self.world = world;
        self.joined = joined;
    }

    /// The address being joined, as typed or picked.
    pub(crate) fn server(&self) -> &str {
        &self.server
    }

    /// Whether the client hosts the server it joins.
    pub(crate) fn is_local(&self) -> bool {
        self.local || self.server.eq_ignore_ascii_case("localhost")
    }

    /// How far the connection has got.
    pub(crate) fn stage(&self) -> Stage {
        self.stage
    }

    /// How far the session's own world is ([`Self::set_world`]).
    pub(crate) fn world(&self) -> Option<WorldStage> {
        self.world
    }

    /// Whether the session is in hand.
    pub(crate) fn joined(&self) -> bool {
        self.joined
    }

    /// Whether this is a map change or map load rather than a join.
    pub(crate) fn is_map_change(&self) -> bool {
        self.map_change
    }

    /// Which join or map load this is; it changes with every new one.
    pub(crate) fn generation(&self) -> u32 {
        self.generation
    }

    /// What the gamestate says of the server and its game, once it is in.
    pub(crate) fn facts(&self) -> &crate::menu::sjk::loading::Facts {
        &self.facts
    }

    /// Fill in a world shot's or test's made-up progress: the stage, the
    /// session's world and whether the session is in hand, held against
    /// the frame's own from then on.
    #[cfg(test)]
    pub(crate) fn hold_for_test(&mut self, stage: Stage, world: Option<WorldStage>, joined: bool) {
        self.stage = stage;
        self.world = world;
        self.joined = joined;
        self.held = true;
    }
}

/// `mp/ffa3` for `maps/mp/ffa3.bsp`, `mp/ffa3` or `MP/FFA3`; empty for text
/// that names no map, such as `next map`.
pub(crate) fn levelshot_key(map: &str) -> String {
    let trimmed = map.trim();
    let lower = trimmed.to_ascii_lowercase();
    let stem = lower.strip_prefix("maps/").unwrap_or(&lower);
    let stem = stem.strip_suffix(".bsp").unwrap_or(stem);
    if stem.is_empty() || stem.contains(' ') {
        return String::new();
    }
    stem.to_owned()
}

/// `CS_SERVERINFO`, parsed.
pub(crate) fn server_info(game: &GameState) -> Option<InfoString> {
    config_text(game, 0).and_then(|text| InfoString::parse(text).ok())
}

/// Config string `index` as text.
pub(crate) fn config_text(game: &GameState, index: usize) -> Option<&str> {
    game.config_string(index)
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
}

/// Text without `^n` colour codes, as `Q_CleanAsciiStr` leaves it readable.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^'
            && chars
                .peek()
                .is_some_and(|next| next.is_ascii_alphanumeric())
        {
            chars.next();
            continue;
        }
        if c.is_ascii() && !c.is_ascii_control() {
            out.push(c);
        }
    }
    out
}

/// `BG_GetGametypeString`.
fn gametype_string(gametype: i32) -> &'static str {
    match gametype {
        0 => "Free For All",
        1 => "Holocron",
        2 => "Jedi Master",
        3 => "Duel",
        4 => "Power Duel",
        5 => "Cooperative",
        6 => "Team Deathmatch",
        7 => "Siege",
        8 => "Capture The Flag",
        9 => "Capture The Ysalimiri",
        _ => "Unknown Gametype",
    }
}

/// The `CG_DrawInformation` lines for `game`, with retail's English
/// `MP_INGAME` strings where `strings` lacks them.
pub(crate) fn information(
    game: &GameState,
    local: bool,
    strings: &HashMap<String, String>,
) -> Vec<InfoLine> {
    let text = |key: &str, english: &str| -> String {
        strings
            .get(key)
            .map_or_else(|| english.to_owned(), Clone::clone)
    };
    let info = server_info(game);
    let system = config_text(game, 1).and_then(|text| InfoString::parse(text).ok());
    let value = |key: &str| info.as_ref().and_then(|info| info.get(key)).unwrap_or("");
    let number = |key: &str| value(key).trim().parse::<i32>().unwrap_or(0);
    let mut lines = Vec::with_capacity(16);
    let mut gap = 0.0;
    let push = |lines: &mut Vec<InfoLine>, text: String, gap: &mut f32| {
        if !text.is_empty() {
            lines.push(InfoLine { text, gap: *gap });
            *gap = 0.0;
        }
    };
    if !local {
        push(&mut lines, plain(value("sv_hostname")), &mut gap);
        if system
            .as_ref()
            .and_then(|system| system.get("sv_pure"))
            .is_some_and(|pure| pure.starts_with('1'))
        {
            push(&mut lines, text("PURE_SERVER", "Pure Server"), &mut gap);
        }
        push(
            &mut lines,
            plain(config_text(game, CS_MOTD).unwrap_or("")),
            &mut gap,
        );
        let gamename = value("gamename");
        if !gamename.eq_ignore_ascii_case("japro") {
            push(&mut lines, plain(gamename), &mut gap);
        }
        // Some extra space after the hostname and message of the day.
        gap = 10.0;
    }
    push(
        &mut lines,
        plain(config_text(game, CS_MESSAGE).unwrap_or("")),
        &mut gap,
    );
    if system
        .as_ref()
        .and_then(|system| system.get("sv_cheats"))
        .is_some_and(|cheats| cheats.starts_with('1'))
    {
        push(
            &mut lines,
            text("CHEATSAREENABLED", "CHEATS ARE ENABLED"),
            &mut gap,
        );
    }
    let gametype = number("g_gametype");
    push(&mut lines, gametype_string(gametype).to_owned(), &mut gap);
    let limit = |lines: &mut Vec<InfoLine>, gap: &mut f32, key: &str, label: String| {
        let value = number(key);
        if value != 0 {
            push(lines, format!("{label} {value}"), gap);
        }
    };
    if gametype != 7 {
        limit(
            &mut lines,
            &mut gap,
            "timelimit",
            text("TIMELIMIT", "Time Limit:"),
        );
        if gametype < 8 {
            limit(
                &mut lines,
                &mut gap,
                "fraglimit",
                text("FRAGLIMIT", "Kill Limit:"),
            );
            if gametype == 3 || gametype == 4 {
                limit(
                    &mut lines,
                    &mut gap,
                    "duel_fraglimit",
                    text("WINLIMIT", "Duel Win Limit:"),
                );
            }
        }
    }
    if gametype >= 8 {
        limit(
            &mut lines,
            &mut gap,
            "capturelimit",
            text("CAPTURELIMIT", "Capture Limit:"),
        );
    }
    if gametype >= 6 && number("g_forceBasedTeams") != 0 {
        push(
            &mut lines,
            text("FORCEBASEDTEAMS", "Force-Based Teams Enabled"),
            &mut gap,
        );
    }
    if gametype != 7 {
        let no_force = number("g_forcePowerDisable") != 0;
        let rank = number("g_maxForceRank");
        if !no_force {
            let level = if (1..8).contains(&rank) { rank } else { 7 };
            let label = text("MAXFORCERANK", "Force Mastery Level:");
            let name = text(&format!("MASTERY{level}"), MASTERY[level as usize]);
            push(&mut lines, format!("{label} {name}"), &mut gap);
        }
        let weapons = if gametype == 3 || gametype == 4 {
            number("g_duelWeaponDisable")
        } else {
            number("g_weaponDisable")
        };
        if gametype != 2 && weapons != 0 {
            push(&mut lines, text("SABERONLYSET", "Saber Only"), &mut gap);
        }
        if no_force {
            push(&mut lines, text("NOFPSET", "No Force Powers"), &mut gap);
        }
    }
    // The rules follow one blank line.
    gap += LINE;
    let rules: &[(&str, &str)] = match gametype {
        0 => &[(
            "RULES_FFA_1",
            "Rules:  Defeat your enemies to score points!",
        )],
        1 => &[("RULES_HOLO_1", ""), ("RULES_HOLO_2", "")],
        2 => &[("RULES_JEDI_1", ""), ("RULES_JEDI_2", "")],
        3 => &[("RULES_DUEL_1", ""), ("RULES_DUEL_2", "")],
        4 => &[("RULES_POWERDUEL_1", ""), ("RULES_POWERDUEL_2", "")],
        6 => &[("RULES_TEAM_1", ""), ("RULES_TEAM_2", "")],
        8 => &[("RULES_CTF_1", ""), ("RULES_CTF_2", "")],
        9 => &[("RULES_CTY_1", ""), ("RULES_CTY_2", "")],
        _ => &[],
    };
    for (key, english) in rules {
        push(&mut lines, text(key, english), &mut gap);
    }
    lines
}

/// Retail's English `MP_INGAME` force mastery names (`forceMasteryLevels`).
pub(crate) const MASTERY: [&str; 8] = [
    "Uninitiated",
    "Initiate",
    "Padawan",
    "Jedi",
    "Jedi Adept",
    "Jedi Guardian",
    "Jedi Knight",
    "Jedi Master",
];

/// Shadowed white text centred on canvas row `y`, as retail's
/// `UI_DROPSHADOW` / `ITEM_TEXTSTYLE_SHADOWED` lines.
fn line(canvas: &mut MenuCanvas, place: &Placement, text: &str, y: f32, size: f32) {
    let s = place.scale;
    let rect = place.rect([20.0, y, CANVAS[0] - 40.0, size * 1.3]);
    let mut shadow = rect;
    shadow.x += 1.5 * s;
    shadow.y += 1.5 * s;
    canvas.text_aligned(
        text,
        shadow,
        size * s,
        SHADOW,
        FontWeight::Regular,
        0.0,
        TextAlign::Center,
    );
    canvas.text_aligned(
        text,
        rect,
        size * s,
        WHITE,
        FontWeight::Regular,
        0.0,
        TextAlign::Center,
    );
}

/// Window rectangle covering `viewport` with a 4:3 image: full width, the
/// excess height cropped equally top and bottom (cgame's levelshot), or full
/// height with side bars on a narrow window.
fn cover(viewport: [f32; 2]) -> Rect {
    let [width, height] = viewport;
    let image_height = width * CANVAS[1] / CANVAS[0];
    if image_height >= height {
        Rect::new(0.0, (height - image_height) * 0.5, width, image_height)
    } else {
        let image_width = height * CANVAS[0] / CANVAS[1];
        Rect::new((width - image_width) * 0.5, 0.0, image_width, height)
    }
}

/// Build the connect or loading screen into `canvas`. The whole screen is
/// the cancel target (token 0), as the notice it replaces was.
pub(crate) fn build(
    canvas: &mut MenuCanvas,
    viewport: [f32; 2],
    loading: &ClassicLoading,
    art_set: ArtSet,
    levelshot: Preview,
    error: Option<&str>,
) {
    let place = Placement::new(viewport);
    canvas.begin_transparent(viewport);
    {
        let draw = canvas.draw_list_mut();
        let _ = draw.push(DrawCommand::SolidRect {
            rect: Rect::new(0.0, 0.0, viewport[0], viewport[1]),
            color: Color::new(0.0, 0.0, 0.0, 1.0),
        });
    }
    let information = loading.stage == Stage::Loading && error.is_none();
    let backdrop = cover(viewport);
    if information && levelshot == Preview::Image {
        let _ = canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
            rect: backdrop,
            texture: LEVELSHOT_TEXTURE,
            color: WHITE,
        });
    } else if art_set.has(ArtPiece::UnknownMap) {
        art(canvas, ArtPiece::UnknownMap, backdrop);
    }
    if let Some(error) = error {
        connect_lines(canvas, &place, loading);
        line(canvas, &place, error, 258.0, 15.0);
        line(canvas, &place, "Press Escape to return.", 306.0, 13.0);
    } else if information {
        information_lines(canvas, &place, loading);
        load_bar(canvas, &place, loading.ticks, art_set);
    } else {
        connect_lines(canvas, &place, loading);
    }
    canvas.hit_region(0, Rect::new(0.0, 0.0, viewport[0], viewport[1]));
    canvas.finish(0);
}

/// `UI_DrawConnectScreen` before the gamestate: rows at 130 + 48 and + 80.
fn connect_lines(canvas: &mut MenuCanvas, place: &Placement, loading: &ClassicLoading) {
    const Y: f32 = 130.0;
    if !loading.map.is_empty() && loading.stage == Stage::Loading {
        line(
            canvas,
            place,
            &format!("Loading... {}", loading.map),
            Y - 8.0,
            16.0,
        );
    }
    let local = loading.local || loading.server.eq_ignore_ascii_case("localhost");
    let heading = if local {
        "Starting up...".to_owned()
    } else {
        format!("Connecting to {}", loading.server)
    };
    line(canvas, place, &heading, Y + 48.0 - 8.0, 16.0);
    let status = match loading.stage {
        Stage::Connecting => "Awaiting connection...",
        Stage::Challenging => "Awaiting challenge...",
        Stage::Connected | Stage::Loading => "Awaiting gamestate...",
    };
    if !local {
        line(canvas, place, status, Y + 80.0 - 8.0, 16.0);
    }
}

/// `CG_DrawInformation`: the loading line at 96, then the server and rules
/// lines from 148 at `LINE` pitch.
fn information_lines(canvas: &mut MenuCanvas, place: &Placement, loading: &ClassicLoading) {
    let heading = if loading.loading.is_empty() {
        "Awaiting snapshot...".to_owned()
    } else {
        format!("Loading... {}", loading.loading)
    };
    line(canvas, place, &heading, 128.0 - 32.0 - 8.0, 15.0);
    let mut y = 180.0 - 32.0;
    for info in &loading.info {
        y += info.gap;
        if y > 420.0 {
            break;
        }
        line(canvas, place, &info.text, y - 8.0, 13.0);
        y += LINE;
    }
}

/// `CG_LoadBar`: the surround, the fill with a cap at either end.
fn load_bar(canvas: &mut MenuCanvas, place: &Placement, ticks: u8, art_set: ArtSet) {
    const TICK_WIDTH: f32 = 40.0;
    const TICK_HEIGHT: f32 = 8.0;
    const PAD_X: f32 = 20.0;
    const PAD_Y: f32 = 12.0;
    const CAP: f32 = 8.0;
    let bar_width = f32::from(TICKS) * TICK_WIDTH + PAD_X * 2.0 + CAP * 2.0;
    let bar_left = (CANVAS[0] - bar_width) / 2.0;
    let bar_height = TICK_HEIGHT + PAD_Y * 2.0;
    let bar_top = CANVAS[1] - bar_height;
    let tick_left = bar_left + PAD_X + CAP;
    let tick_top = bar_top + PAD_Y;
    let fill = f32::from(ticks.min(TICKS)) * TICK_WIDTH;
    let pieces = [
        (
            ArtPiece::LoadFrame,
            [bar_left, bar_top, bar_width, bar_height],
        ),
        (
            ArtPiece::LoadCapLeft,
            [tick_left - CAP, tick_top, CAP, TICK_HEIGHT],
        ),
        (ArtPiece::LoadTick, [tick_left, tick_top, fill, TICK_HEIGHT]),
        (
            ArtPiece::LoadCap,
            [tick_left + fill, tick_top, CAP, TICK_HEIGHT],
        ),
    ];
    for (piece, rect) in pieces {
        if rect[2] > 0.0 && art_set.has(piece) {
            art(canvas, piece, place.rect(rect));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levelshot_keys_name_maps_only() {
        assert_eq!(levelshot_key("maps/MP/Siege_Desert.bsp"), "mp/siege_desert");
        assert_eq!(levelshot_key("mp/ffa3"), "mp/ffa3");
        assert_eq!(levelshot_key("next map"), "");
        assert_eq!(levelshot_key(""), "");
    }

    #[test]
    fn colour_codes_and_controls_are_removed() {
        assert_eq!(plain("^1JoF^7 Clan\n"), "JoF Clan");
        assert_eq!(plain("100^%"), "100^%");
    }

    #[test]
    fn stages_only_move_forward() {
        let mut loading = ClassicLoading::default();
        loading.begin("135.125.145.49:29070", false);
        loading.set_phase(TimelinePhase::Connected);
        loading.set_phase(TimelinePhase::Challenge);
        assert_eq!(loading.stage, Stage::Connected);
        loading.set_phase(TimelinePhase::Gamestate);
        assert_eq!(loading.stage, Stage::Loading);
    }

    #[test]
    fn information_follows_cg_draw_information() {
        let mut game = GameState::empty_local(0);
        let set = |game: &mut GameState, index: usize, text: &str| {
            game.replace_config_string(index, text.as_bytes().to_vec())
                .expect("valid config string");
        };
        set(
            &mut game,
            0,
            "\\sv_hostname\\^1JoF ^7Siege\\mapname\\mp/siege_desert\\g_gametype\\0\\timelimit\\20\\fraglimit\\50\\g_maxForceRank\\5",
        );
        set(&mut game, 1, "\\sv_pure\\1");
        set(&mut game, CS_MESSAGE, "Desert");
        let lines = information(&game, false, &HashMap::new());
        let texts: Vec<&str> = lines.iter().map(|line| line.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "JoF Siege",
                "Pure Server",
                "Desert",
                "Free For All",
                "Time Limit: 20",
                "Kill Limit: 50",
                "Force Mastery Level: Jedi Guardian",
                "Rules:  Defeat your enemies to score points!",
            ]
        );
        assert_eq!(lines[2].gap, 10.0);
        assert_eq!(lines[7].gap, LINE);
        let local = information(&game, true, &HashMap::new());
        assert_eq!(local[0].text, "Desert");
    }
}
