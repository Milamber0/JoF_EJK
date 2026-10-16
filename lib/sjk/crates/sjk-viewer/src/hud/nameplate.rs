//! MMO-style nameplates over players (and, optionally, NPCs).
//!
//! Far away a player shows only a small, dim name. Closer, the name rises and a
//! framed plate fades in beneath it with health, shield and Force bars, and the
//! weapon the player holds beside it, haloed in the saber stance's colour.
//! Names come from the chat roster and are drawn in the classic HUD font with
//! their colour codes; the layout maths is in [`super::nameplate_math`]. What the
//! server does not send is estimated, once per snapshot: Force in
//! [`super::force_estimate`], health and shield in [`super::vitals_estimate`],
//! each as a range whose uncertainty the bar shows as a grey haze. The plain
//! TaystJK names stay in [`super::identification`] (`cg_drawPlayerNames`); a
//! nameplate replaces them.
use super::estimate::Range;
use super::force_estimate::{self, Calibration, Estimator};
use super::force_streams;
use super::identification::{Camera, info_number, unoccluded};
use super::nameplate_math::{self as math, Rows, Stack};
use super::vitals_estimate;
use crate::{TextVertex, UiFont, chat::ChatOverlay, console::ViewerConsole};
use glam::Vec3;
use sjk_bsp::{Aabb, Bsp, TraceScratch};
use sjk_client::TeamInfoTable;
use sjk_game_jka::force_powers::{FP_DRAIN, FP_LIGHTNING};
use sjk_protocol::{GameState, Snapshot};
use sjk_shell::{CvarDefinition, CvarFlags, CvarRegistry};
use sjk_ui::{
    Color, DrawCommand, DrawList, FontWeight, Gradient, Rect, TextAlign, TextId, TextOverflow,
    TextureId,
};

/// Text ids below this are player slots; above it, `NPC_TEXT + class_t`.
const NPC_TEXT: u32 = 1024;
/// Text id of the "?" over a bar the estimate cannot fill.
const UNKNOWN_TEXT: u32 = 1000;
/// Text id of the mode's name, shown for a moment when the mode changes.
const MODE_TEXT: u32 = 1001;
/// How long the mode's name shows, and the last part of that over which it fades.
const MODE_SHOWN: i64 = 1_500;
const MODE_FADE: i64 = 400;
/// How long the player last aimed at keeps their bars in [`Mode::Target`].
const FOCUS_LINGER: i64 = 3_000;
/// `cg_nameplateBars` 3: bars only on the player aimed at and the duel opponent.
const BARS_TARGET: i64 = 3;

/// Console command that cycles the nameplate modes, or sets one (`nameplates target`).
pub(crate) const COMMAND: &str = "nameplates";
/// Help text for completion and `cmdlist`.
pub(crate) const HELP: &str =
    "Cycle the nameplates: off, names only, bars on your target, bars on everyone (or name one)";

/// The nameplate modes the `nameplates` command (V by default) cycles through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// No nameplates (`cg_nameplate 0`).
    Off,
    /// Names only (`cg_nameplateBars 0`).
    Names,
    /// Names, and bars on the player aimed at and the duel opponent (`cg_nameplateBars 3`).
    Target,
    /// Names and bars on everyone (`cg_nameplateBars 2`).
    All,
}

impl Mode {
    /// The mode `cg_nameplate` and `cg_nameplateBars` make; bars on allies only (1),
    /// which the cycle skips, counts as between names and target.
    pub(crate) fn of(enabled: bool, bars: i64) -> Self {
        match (enabled, bars) {
            (false, _) => Self::Off,
            (true, 2) => Self::All,
            (true, BARS_TARGET) => Self::Target,
            (true, 1) => Self::Names,
            _ => Self::Names,
        }
    }

    /// The next mode V gives: off, names, target, everyone, and round again.
    pub(crate) fn next(self) -> Self {
        match self {
            Self::Off => Self::Names,
            Self::Names => Self::Target,
            Self::Target => Self::All,
            Self::All => Self::Off,
        }
    }

    /// `cg_nameplate` and, when on, `cg_nameplateBars` for this mode.
    pub(crate) fn settings(self) -> (bool, Option<i64>) {
        match self {
            Self::Off => (false, None),
            Self::Names => (true, Some(0)),
            Self::Target => (true, Some(BARS_TARGET)),
            Self::All => (true, Some(2)),
        }
    }

    /// A mode named by the `nameplates` command's word.
    pub(crate) fn named(word: &str) -> Option<Self> {
        match word.to_ascii_lowercase().as_str() {
            "off" | "0" => Some(Self::Off),
            "names" | "name" | "1" => Some(Self::Names),
            "target" | "2" => Some(Self::Target),
            "all" | "everyone" | "3" => Some(Self::All),
            _ => None,
        }
    }

    /// What the screen says when the mode is chosen.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Off => "Nameplates: off",
            Self::Names => "Nameplates: names only",
            Self::Target => "Nameplates: names, bars on your target and duel opponent",
            Self::All => "Nameplates: names and bars on everyone",
        }
    }
}
/// A health or shield range this wide (shares of a full bar) is too unsure to show:
/// the bar dims and a "?" stands over it until something narrows it.
const UNSURE_WIDTH: f32 = 0.6;
/// A shield whose high bound is under this share is known to be empty.
const EMPTY_SHARE: f32 = 0.005;
/// `ET_PLAYER` and `ET_NPC` entity types, `EF_DEAD`, and `PW_FORCE_BOON`.
const ET_PLAYER: u8 = 1;
const ET_NPC: u8 = 13;
const EF_DEAD: u32 = 2;
const PW_FORCE_BOON: u32 = 14;
/// `GT_JEDIMASTER` and the first team game type.
const GT_JEDIMASTER: i32 = 2;
/// `WP_SABER`.
const WP_SABER: u8 = 3;
const GT_TEAM: i32 = 6;
/// Tags drawn at most: every client slot, plus a bounded number of NPCs.
const MAX_PLAYER_TAGS: usize = 32;
const MAX_NPC_TAGS: usize = 16;
const MAX_TAGS: usize = MAX_PLAYER_TAGS + MAX_NPC_TAGS;
/// Entity numbers are ten bits on the wire.
const ENTITY_SLOTS: usize = 1024;
/// A plate unseen this long fades in again instead of resuming.
const STALE_MILLIS: i64 = 250;
/// Opacity of a plate behind a wall when `cg_nameplateWalls` is on.
const WALL_OPACITY: f32 = 0.35;
/// Allowed overshoot of the screen (in half-screens) before a plate is dropped.
const SCREEN_MARGIN: f32 = 1.15;
/// Smallest size, as a share of full, a plate at the end of its range shrinks to.
const MIN_SCALE: f32 = 0.6;
/// Opacity of a name too far for a plate.
const FAR_NAME_OPACITY: f32 = 0.7;
/// Full health and armour, for teammates (`tinfo` sends points, not a share).
const FULL_POINTS: f32 = 100.0;
/// Power icons shown over a name at most.
const MAX_ICONS: usize = 4;
/// Force powers whose icon shows while they are on, dark side first, then the
/// light side and neutral ones, as `forcePowers_t` indices. Push, pull, jump and
/// the saber powers are left out: they last a moment and would only flicker.
const ICON_POWERS: [u8; 11] = [7, 6, 13, 8, 9, 10, 2, 0, 12, 5, 14];
/// Frame colour of players who follow no team.
const NEUTRAL_ACCENT: Color = Color::new(0.78, 0.82, 0.9, 0.9);
/// The contour of a player who shares their bars (`cg_nameplateShare`, or
/// `cg_nameplateEstimate`, which shares them), the verified badge's gold.
const SHARED_CONTOUR: Color = Color::new(1.0, 0.82, 0.25, 1.0);
/// Frame colour of NPC plates.
const NPC_ACCENT: Color = Color::new(0.95, 0.8, 0.35, 0.9);
/// The grey haze over a bar's uncertain stretch, at its thickest.
const HAZE: Color = Color::new(0.8, 0.82, 0.86, 0.6);
/// A range narrower than this share of the bar is drawn sharp.
const HAZE_MIN_WIDTH: f32 = 0.02;
/// Backdrop of the power and weapon icons.
const ICON_BACKDROP: Color = Color::new(0.03, 0.04, 0.06, 0.55);
/// Halo opacity of a holstered saber, as a share of a lit one's.
const HOLSTERED_HALO: f32 = 0.4;
/// `WP_NONE`.
const WP_NONE: u8 = 0;
/// Milliseconds between reads of which players are verified.
const VERIFIED_EVERY: i64 = 1_000;
/// Milliseconds between reads of what the other players share: their values come
/// about twice a second.
const SHARED_EVERY: i64 = 100;

/// What one slot's player shares (`PROTOCOL.md`, "Vitals"), their claim matching the
/// name the game shows there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SharedSlot {
    /// They see everyone's estimated bars.
    pub(crate) estimates: bool,
    /// Their values, while they have some (alive in a match).
    pub(crate) vitals: Option<sjk_identity::Vitals>,
}

/// Register the nameplate settings.
pub(super) fn register(cvars: &mut CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    cvars.register(CvarDefinition::new(
        "cg_nameplate",
        true,
        CvarFlags::ARCHIVE,
        "MMO-style nameplates over players; they replace cg_drawPlayerNames",
    ))?;
    cvars.register(CvarDefinition::new(
        "cg_nameplateRange",
        3000_i64,
        CvarFlags::ARCHIVE,
        "Distance in units out to which nameplates show",
    ))?;
    cvars.register(CvarDefinition::new(
        "cg_nameplateNear",
        1000_i64,
        CvarFlags::ARCHIVE,
        "Distance in units inside which a nameplate shows its bars",
    ))?;
    cvars.register(CvarDefinition::new(
        "cg_nameplateScale",
        0.5_f64,
        CvarFlags::ARCHIVE,
        "Nameplate text size",
    ))?;
    cvars.register(CvarDefinition::new(
        "cg_nameplateBars",
        2_i64,
        CvarFlags::ARCHIVE,
        "Nameplate bars: 0 none, 1 allies only, 2 everyone, 3 the player you aim at and your duel opponent",
    ))?;
    cvars.register(CvarDefinition::new(
        "cg_nameplateForce",
        true,
        CvarFlags::ARCHIVE,
        "Nameplate Force bar, estimated from the player's powers",
    ))?;
    cvars.register(CvarDefinition::new(
        "cg_nameplateShare",
        false,
        CvarFlags::ARCHIVE,
        "Share your health, shield and Force with the other SJK players of your server; on whenever your nameplates show bars",
    ))?;
    cvars.register(CvarDefinition::new(
        "cg_nameplateEstimate",
        false,
        CvarFlags::ARCHIVE,
        "Estimated bars on players who do not share, while the hub has yours; turning it on shares your own, and the others are told",
    ))?;
    cvars.register(CvarDefinition::new(
        "cg_nameplateWeapon",
        true,
        CvarFlags::ARCHIVE,
        "Icon of the weapon a player holds beside the nameplate, a saber's haloed in its stance's colour",
    ))?;
    cvars.register(CvarDefinition::new(
        "cg_nameplateIcons",
        true,
        CvarFlags::ARCHIVE,
        "Icons of the Force powers a player has on, over the nameplate when close",
    ))?;
    cvars.register(CvarDefinition::new(
        "cg_nameplateWalls",
        false,
        CvarFlags::ARCHIVE,
        "Show nameplates dimmed through walls",
    ))?;
    cvars.register(CvarDefinition::new(
        "cg_nameplateNpcs",
        false,
        CvarFlags::ARCHIVE,
        "Nameplates on NPCs: their class and health",
    ))?;
    cvars.register(CvarDefinition::new(
        "cg_nameplateSelf",
        false,
        CvarFlags::ARCHIVE,
        "Your own nameplate over your head in third person, with your real bars",
    ))?;
    cvars.register(CvarDefinition::new(
        "cg_nameplateDebug",
        false,
        CvarFlags::NONE,
        "Log what the server sends about other players every two seconds",
    ))?;
    Ok(())
}

/// Seeing bars shares the player's own (Sol's rule, 15/10/2026): nameplates with bars
/// (`cg_nameplate` and `cg_nameplateBars` not 0) or `cg_nameplateEstimate` turn
/// `cg_nameplateShare` back on whenever it is off, so the settings say what is sent.
pub(crate) fn hold_sharing(console: &mut ViewerConsole) {
    let bars = console.bool_cvar("cg_nameplate") != Some(false)
        && console.integer_cvar("cg_nameplatebars").unwrap_or(2) != 0;
    if (bars || console.bool_cvar("cg_nameplateestimate") == Some(true))
        && console.bool_cvar("cg_nameplateshare") != Some(true)
    {
        console.set_cvar("cg_nameplateShare", "1");
    }
}

/// Settings, sampled once per frame.
#[derive(Clone, Copy)]
struct Settings {
    enabled: bool,
    range: f32,
    near: f32,
    scale: f32,
    bars: i64,
    force: bool,
    /// `cg_nameplateShare`: the player shares their own values (held on while the
    /// plates show bars, [`hold_sharing`]).
    share: bool,
    /// `cg_nameplateEstimate`: estimated bars on players who do not share, which
    /// makes the player share whatever `share` says.
    estimate: bool,
    weapon: bool,
    walls: bool,
    npcs: bool,
    icons: bool,
    friends: bool,
    /// The local player's own plate, in third person.
    own: bool,
    debug: bool,
    /// The local player's drain and lightning levels from its Force profile.
    drain_level: Option<u8>,
    lightning_level: Option<u8>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            range: 3000.0,
            near: 1000.0,
            scale: 0.5,
            bars: 2,
            force: true,
            share: false,
            estimate: false,
            weapon: true,
            walls: false,
            npcs: false,
            icons: true,
            friends: true,
            own: false,
            debug: false,
            drain_level: None,
            lightning_level: None,
        }
    }
}

/// One visible plate, collected in `update` and drawn in `append`.
#[derive(Clone, Copy)]
struct Entry {
    /// Player slot, or NPC entity number.
    number: u16,
    /// `class_t` for an NPC, zero for a player.
    npc_class: u8,
    /// Screen position of the point above the head.
    point: [f32; 2],
    distance: f32,
    /// Size multiplier from distance.
    scale: f32,
    /// Smoothed opacity, distance fade included.
    alpha: f32,
    /// How much of the plate shows, 0 (far: name only) to 1.
    detail: f32,
    /// How close the player is, 0 (far) to 1: the same ramp, whether or not there are bars.
    proximity: f32,
    /// Force powers to show icons for (`forcePowers_t` indices), `power_count` of them.
    powers: [u8; MAX_ICONS],
    power_count: u8,
    /// Shares of a full bar.
    health: Option<Range>,
    shield: Option<Range>,
    force: Option<Range>,
    /// The weapon held (`WP_NONE` for no icon), the saber style (`fireflag`, which
    /// carries `fd.saberAnimLevel`) and whether the blade is put away.
    weapon: u8,
    style: u8,
    holstered: bool,
    accent: Color,
    names_allowed: bool,
    /// The SJK hub's operator vouches for this player: the gold badge after the name.
    verified: bool,
    /// They share their bars, of their own will or because they see everyone's: a
    /// gold contour.
    shared: bool,
}

impl Entry {
    fn rows(&self) -> Rows {
        Rows {
            health: self.health.is_some(),
            shield: self.shield.is_some(),
            force: self.force.is_some(),
        }
    }
}

/// Smoothed opacity of one entity's plate.
#[derive(Clone, Copy, Default)]
struct Fade {
    alpha: f32,
    seen: i64,
}

/// Fixed-capacity plate state: at most 32 clients and 16 NPCs, no owned name strings.
pub(crate) struct State {
    /// Shapes and text IDs submitted through the normal HUD renderer.
    pub(crate) list: DrawList,
    settings: Settings,
    entries: Vec<Entry>,
    fades: Box<[Fade; ENTITY_SLOTS]>,
    last_update: i64,
    last_debug: i64,
    force: Estimator,
    streams: force_streams::Tracker,
    calibration: Calibration,
    /// Until when drain holds the local player's own refill back (calibration).
    own_hold: i32,
    /// Where the Force regeneration pace came from, for the debug log.
    regen_source: &'static str,
    vitals: vitals_estimate::Estimator,
    /// The health, shield and Force colours of the HUD in use, which the bars take.
    colors: BarColors,
    /// Slots the hub's operator vouches for, as bits, and when they were last read.
    verified: u32,
    verified_read: Option<i64>,
    /// What each slot's player shares (`PROTOCOL.md`, "Vitals"), and when it was last
    /// read.
    shared: [Option<SharedSlot>; MAX_PLAYER_TAGS],
    shared_read: Option<i64>,
    /// The hub has the player's own sharing: without it, no shared or estimated bars.
    sharing_live: bool,
    /// The server time of the shared values each slot's estimates were last
    /// corrected with: each value corrects them once.
    anchored: [Option<i32>; MAX_PLAYER_TAGS],
    /// Where the local player stands, while their own plate may show (third person).
    own_origin: Option<Vec3>,
    /// The player the crosshair was last on, and when.
    focus: Option<(u16, i64)>,
    /// The mode just chosen and when, to show its name for a moment.
    announced: Option<(Mode, i64)>,
    /// Each slot's team logo top this frame ([`super::team_logo`]): the plate stands
    /// above it.
    logo_tops: [Option<f32>; MAX_PLAYER_TAGS],
}

impl Default for State {
    fn default() -> Self {
        Self {
            list: DrawList::new(MAX_TAGS * 40 + 8),
            settings: Settings::default(),
            entries: Vec::with_capacity(MAX_TAGS),
            fades: Box::new([Fade::default(); ENTITY_SLOTS]),
            last_update: 0,
            last_debug: 0,
            force: Estimator::default(),
            streams: force_streams::Tracker::default(),
            calibration: Calibration::default(),
            own_hold: i32::MIN,
            regen_source: "default",
            vitals: vitals_estimate::Estimator::default(),
            colors: BarColors::default(),
            verified: 0,
            verified_read: None,
            shared: [None; MAX_PLAYER_TAGS],
            shared_read: None,
            sharing_live: false,
            anchored: [None; MAX_PLAYER_TAGS],
            own_origin: None,
            focus: None,
            announced: None,
            logo_tops: [None; MAX_PLAYER_TAGS],
        }
    }
}

impl State {
    /// Sample settings once, outside text emission.
    pub(crate) fn sample(&mut self, console: Option<&ViewerConsole>) {
        let flag =
            |name: &str, default: bool| console.and_then(|c| c.bool_cvar(name)).unwrap_or(default);
        let number = |name: &str, default: f32| {
            console
                .and_then(|c| c.integer_cvar(name))
                .map_or(default, |value| value as f32)
        };
        let profile = console.and_then(ViewerConsole::own_force_allocation);
        self.settings = Settings {
            enabled: flag("cg_nameplate", true),
            range: number("cg_nameplaterange", 3000.0).clamp(500.0, 10_000.0),
            near: number("cg_nameplatenear", 1000.0).clamp(0.0, 10_000.0),
            scale: crate::cgame_options::scalar(console, "cg_nameplatescale", 0.5).clamp(0.1, 3.0),
            bars: console
                .and_then(|c| c.integer_cvar("cg_nameplatebars"))
                .unwrap_or(2),
            force: flag("cg_nameplateforce", true),
            share: flag("cg_nameplateshare", false),
            estimate: flag("cg_nameplateestimate", false),
            weapon: flag("cg_nameplateweapon", true),
            walls: flag("cg_nameplatewalls", false),
            npcs: flag("cg_nameplatenpcs", false),
            icons: flag("cg_nameplateicons", true),
            friends: flag("cg_drawfriend", true),
            own: flag("cg_nameplateself", false),
            debug: flag("cg_nameplatedebug", false),
            drain_level: profile.as_ref().map(|profile| profile.levels[FP_DRAIN]),
            lightning_level: profile.as_ref().map(|profile| profile.levels[FP_LIGHTNING]),
        };
    }

    /// Whether nameplates replace the plain overhead names.
    pub(crate) fn enabled(&self) -> bool {
        self.settings.enabled
    }

    /// Take the bars' colours from the HUD in use (its health, armour and Force
    /// meters); `None` for one it names none for (the game-data HUD draws
    /// pictures), which gets the retail colour.
    pub(crate) fn set_hud_colors(
        &mut self,
        health: Option<Color>,
        shield: Option<Color>,
        force: Option<Color>,
    ) {
        self.colors = BarColors {
            health: health.unwrap_or(math::HEALTH_COLOR),
            shield: shield.unwrap_or(math::SHIELD_COLOR),
            force: force.unwrap_or(math::FORCE_COLOR),
        };
    }

    /// Read which slots are verified (`read` asks the identity service) at most once a
    /// second, so the names it compares are not rebuilt every frame.
    pub(crate) fn refresh_verified(&mut self, now: i64, read: impl FnOnce() -> u32) {
        if self
            .verified_read
            .is_some_and(|last| (0..VERIFIED_EVERY).contains(&(now - last)))
        {
            return;
        }
        self.verified_read = Some(now);
        self.verified = read();
    }

    /// Read what the other SJK players share and whether the hub has the player's own
    /// sharing (`read` asks the identity service) at most every [`SHARED_EVERY`] ms,
    /// and correct the estimates with each new value.
    pub(crate) fn refresh_shared(
        &mut self,
        now: i64,
        read: impl FnOnce() -> ([Option<SharedSlot>; MAX_PLAYER_TAGS], bool),
    ) {
        if self
            .shared_read
            .is_some_and(|last| (0..SHARED_EVERY).contains(&(now - last)))
        {
            return;
        }
        self.shared_read = Some(now);
        (self.shared, self.sharing_live) = read();
        for (slot, shared) in self.shared.iter().enumerate() {
            let Some(values) = shared.and_then(|shared| shared.vitals) else {
                continue;
            };
            if self.anchored[slot] == Some(values.t) {
                continue;
            }
            self.anchored[slot] = Some(values.t);
            let slot = slot as u16;
            self.vitals
                .anchor(slot, values.t, values.health as f32, values.armor as f32);
            let pool = values.force as f32 * 100.0 / values.max_force.max(1) as f32;
            self.force.anchor(slot, values.t, pool);
        }
    }

    /// What the player shares this frame: their own health, shield and Force while
    /// alive in a match, with `cg_nameplateShare`, or whenever they may see others'
    /// bars (plates with bars, `cg_nameplateEstimate`) whatever it says.
    pub(crate) fn sharing(&self, snapshot: Option<&Snapshot>) -> sjk_identity::Sharing {
        let settings = &self.settings;
        let share = settings.share || settings.estimate || (settings.enabled && settings.bars != 0);
        let vitals = snapshot
            .filter(|snapshot| {
                share && snapshot.player.health() > 0 && !snapshot.player.is_spectator()
            })
            .map(|snapshot| sjk_identity::Vitals {
                t: snapshot.server_time,
                health: snapshot.player.health(),
                armor: snapshot.player.armor(),
                force: i32::from(snapshot.player.force_power()),
                max_health: snapshot.player.max_health().max(1),
                // The protocol does not carry `fd.forcePowerMax`; a full pool is 100.
                max_force: 100,
            });
        sjk_identity::Sharing {
            share,
            estimates: settings.estimate,
            vitals,
        }
    }

    /// The player the crosshair is on at `now`, if any, for [`Mode::Target`]: they keep
    /// their bars for a few seconds after the crosshair leaves them.
    pub(crate) fn set_focus(&mut self, target: Option<u16>, now: i64) {
        if let Some(client) = target {
            self.focus = Some((client, now));
        }
    }

    /// Show `mode`'s name on screen for a moment.
    pub(crate) fn announce(&mut self, mode: Mode) {
        self.announced = Some((mode, self.last_update));
    }

    /// Where the local player stands, for their own plate; `None` hides it (first
    /// person, where it would sit inside the camera).
    pub(crate) fn set_own_origin(&mut self, origin: Option<[f32; 3]>) {
        self.own_origin = origin.map(Vec3::from_array);
    }

    /// Feed one accepted snapshot to the estimates. Every snapshot is observed
    /// once, in order, whether or not plates are shown, so no event is missed.
    pub(crate) fn observe_snapshot(
        &mut self,
        snapshot: &Snapshot,
        game: &GameState,
        bsp: &Bsp,
        scratch: &mut TraceScratch,
    ) {
        let mode = info_number(game.config_string(0), "g_gametype");
        // The events first (drain names its victims), then the shots, which the
        // health and the Force both take.
        self.vitals.read(snapshot, game);
        self.observe_streams(snapshot, game, mode, bsp, scratch);
        self.vitals.apply(snapshot, game, mode, &self.streams);
        self.observe_force(snapshot, game, mode);
    }

    /// Rebuild this snapshot's drain, lightning and grip shots from where every
    /// living player stands and looks ([`force_streams`]); `bsp` gives the walls that
    /// stop them.
    fn observe_streams(
        &mut self,
        snapshot: &Snapshot,
        game: &GameState,
        mode: i32,
        bsp: &Bsp,
        scratch: &mut TraceScratch,
    ) {
        use force_streams::Body;
        let mut bodies = [Body::default(); 33];
        let mut count = 0;
        for entity in &snapshot.entities {
            let number = entity.number();
            if entity.entity_type() != ET_PLAYER
                || number >= 32
                || entity.e_flags() & EF_DEAD != 0
                || count == bodies.len()
            {
                continue;
            }
            let (mins, maxs) = Body::unpack_box(entity.solid());
            bodies[count] = Body {
                number,
                origin: entity.trajectory_base(),
                view: entity.angular_trajectory_base(),
                mins,
                maxs,
                active: entity.force_powers_active(),
                weapon: entity.weapon(),
                team: info_number(game.config_string(1131 + usize::from(number)), "t"),
                duelling: entity.bolt1(),
                electrified: entity.emplaced_owner(),
                drain_level: None,
                lightning_level: None,
                estimated: true,
            };
            count += 1;
        }
        let player = &snapshot.player;
        if player.health() > 0 && !player.is_spectator() && count < bodies.len() {
            let (mins, maxs) = Body::standing_box();
            bodies[count] = Body {
                number: player.client_num(),
                origin: player.origin(),
                view: player.view_angles(),
                mins,
                maxs,
                active: player.force_powers_active(),
                weapon: player.weapon(),
                team: i32::from(player.team()),
                duelling: player.duel_in_progress(),
                electrified: player.electrify_time(),
                drain_level: self.settings.drain_level,
                lightning_level: self.settings.lightning_level,
                estimated: false,
            };
            count += 1;
        }
        let rules = force_streams::Rules {
            teams: mode >= GT_TEAM,
        };
        let own = (player.health() > 0 && !player.is_spectator()).then(|| force_streams::Own {
            number: player.client_num(),
            pool: i32::from(player.force_power()),
            health: player.health(),
            max_health: player.max_health(),
        });
        self.streams.observe(
            snapshot.server_time,
            &bodies[..count],
            rules,
            self.vitals.drained(),
            own,
            force_streams::world_sight(bsp, scratch),
        );
        if let Some(run) = self.streams.take_finished() {
            log_drain(&run, game);
        }
    }

    /// Drop every collected plate and drawn shape.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.list.clear();
    }

    /// Smooth one entity's opacity towards `target`.
    fn fade(&mut self, number: u16, target: f32, now: i64, step: f32) -> f32 {
        let fade = &mut self.fades[usize::from(number) % ENTITY_SLOTS];
        if now - fade.seen > STALE_MILLIS {
            fade.alpha = 0.0;
        }
        fade.seen = now;
        fade.alpha += (target - fade.alpha).clamp(-step, step);
        fade.alpha
    }

    /// This frame's team logo tops, by client slot.
    pub(crate) fn set_logo_tops(&mut self, tops: [Option<f32>; MAX_PLAYER_TAGS]) {
        self.logo_tops = tops;
    }

    /// Collect visible players using fixed scratch and the actual rendered camera.
    pub(crate) fn update(
        &mut self,
        snapshot: &Snapshot,
        game: &GameState,
        world: &sjk_runtime::World,
        team_info: &TeamInfoTable,
        now: i64,
        camera: Camera,
        bsp: &Bsp,
        scratch: &mut TraceScratch,
        hidden: bool,
    ) {
        self.clear();
        let step = (now - self.last_update).clamp(0, 100) as f32 / math::FADE_MILLIS;
        self.last_update = now;
        let settings = self.settings;
        if !settings.enabled {
            return;
        }
        let mode = info_number(game.config_string(0), "g_gametype");
        let local = snapshot.player.client_num();
        if settings.debug && now - self.last_debug >= 2_000 {
            self.last_debug = now;
            self.log_players(snapshot, game, team_info);
        }
        if hidden {
            return;
        }
        let standing = Standing {
            mode,
            restrictions: info_number(game.config_string(0), "restricts"),
            local_team: snapshot.player.team() as i32,
            local_duel: info_number(game.config_string(1131 + usize::from(local)), "ds"),
            master: snapshot
                .entities
                .iter()
                .any(|e| e.number() < 32 && e.is_jedi_master()),
            local_master: snapshot.player.is_jedi_master(),
        };
        // `cg_nameplateBars 3`: the player aimed at lately and the duel opponent.
        let focus = self
            .focus
            .filter(|(_, seen)| (0..=FOCUS_LINGER).contains(&(now - seen)))
            .map(|(client, _)| client);
        let duel_opponent = snapshot
            .player
            .duel_in_progress()
            .then(|| snapshot.player.duel_index());
        let mut npcs = 0;
        for entity in &snapshot.entities {
            let number = entity.number();
            let player = entity.entity_type() == ET_PLAYER && number < 32 && number != local;
            let npc = !player
                && settings.npcs
                && entity.entity_type() == ET_NPC
                && usize::from(number) < ENTITY_SLOTS
                && super::npc_class::name(entity.npc_class()).is_some()
                && entity.health() > 0
                && npcs < MAX_NPC_TAGS;
            if !(player || npc)
                || entity.e_flags() & EF_DEAD != 0
                || entity.client_bitflag(local)
                || entity.powerups() & (1 << 11) != 0
            {
                continue;
            }
            let (accent, names_allowed, ally) = if player {
                let Some(who) =
                    identify(game, &settings, &standing, number, entity.is_jedi_master())
                else {
                    continue;
                };
                (who.accent, who.names_allowed, who.ally)
            } else {
                (NPC_ACCENT, true, false)
            };
            let presented = |id: u16| {
                world
                    .entity(sjk_runtime::EntityId::new(u64::from(id) + 1))
                    .map(|e| e.sample(now).translation)
            };
            // A pilot hidden in his vehicle (EF_NODRAW) has no presented body:
            // his plate hangs over the vehicle instead.
            let vehicle = match entity.vehicle_entity_num() {
                0 => None,
                id => snapshot
                    .entities
                    .binary_search_by_key(&id, |state| state.number())
                    .ok()
                    .and_then(|at| {
                        presented(id).map(|origin| (origin, snapshot.entities[at].solid()))
                    }),
            };
            let Some(placed) = math::anchor(
                presented(number).map(|origin| (origin, entity.solid())),
                vehicle.filter(|_| player),
            ) else {
                continue;
            };
            let origin = Vec3::from_array(placed.origin);
            let distance = origin.distance(camera.eye);
            if distance >= settings.range {
                continue;
            }
            let anchor = origin + Vec3::Z * (placed.head + math::HEAD_CLEARANCE);
            let Some(point) = camera.project_within(anchor, SCREEN_MARGIN) else {
                continue;
            };
            let trace = bsp.trace_box_with(
                scratch,
                camera.eye.to_array(),
                origin.to_array(),
                Aabb::new([0.0; 3], [0.0; 3]).unwrap(),
                1 | 0x0200_0000,
            );
            let visibility = if unoccluded(trace.fraction, trace.start_solid, trace.all_solid) {
                1.0
            } else if settings.walls {
                WALL_OPACITY
            } else {
                0.0
            };
            let alpha = self.fade(
                number,
                math::distance_fade(distance, settings.range) * visibility,
                now,
                step,
            );
            if alpha < 0.02 {
                continue;
            }
            let bars = names_allowed
                && match settings.bars {
                    2 => true,
                    1 => ally,
                    BARS_TARGET => {
                        player && (focus == Some(number) || duel_opponent == Some(number))
                    }
                    _ => false,
                };
            // Estimates show on players who share (corrected by what they share) and,
            // with `cg_nameplateEstimate`, on everyone; either only while the hub has
            // the player's own sharing.
            let estimated = player
                && self.sharing_live
                && (settings.estimate
                    || self
                        .shared
                        .get(usize::from(number))
                        .copied()
                        .flatten()
                        .is_some_and(|shared| shared.vitals.is_some()));
            let (health, shield) = if bars {
                let predicted = estimated.then_some(&self.vitals);
                bar_values(entity, number, ally && player, team_info, predicted)
            } else {
                (None, None)
            };
            let force = (bars && estimated && settings.force)
                .then(|| self.force.ratio(number))
                .flatten();
            let rows = Rows {
                health: health.is_some(),
                shield: shield.is_some(),
                force: force.is_some(),
            };
            // Names only means names: the icons come with the bars.
            let (powers, power_count) = if settings.icons && player && bars {
                icon_powers(entity.force_powers_active())
            } else {
                ([0; MAX_ICONS], 0)
            };
            if npc {
                npcs += 1;
            }
            self.entries.push(Entry {
                number,
                npc_class: if npc { entity.npc_class() } else { 0 },
                point,
                distance,
                scale: math::distance_scale(distance, settings.range, MIN_SCALE),
                alpha,
                detail: if rows.any() {
                    math::detail(distance, settings.near)
                } else {
                    0.0
                },
                proximity: math::detail(distance, settings.near),
                powers,
                power_count,
                health,
                shield,
                force,
                weapon: if player && settings.weapon && bars {
                    entity.weapon()
                } else {
                    WP_NONE
                },
                style: entity.fire_flag(),
                holstered: entity.saber_holstered() != 0,
                accent,
                names_allowed,
                verified: player && self.verified & (1 << number) != 0,
                shared: player
                    && self
                        .shared
                        .get(usize::from(number))
                        .is_some_and(Option::is_some),
            });
            if self.entries.len() == MAX_TAGS {
                break;
            }
        }
        self.hidden_pilots(
            snapshot, game, world, &standing, now, camera, bsp, scratch, step,
        );
        if settings.own {
            self.own_plate(snapshot, mode, now, camera, step);
        }
        // Far plates first, so a near plate covers a far one.
        self.entries
            .sort_unstable_by(|a, b| b.distance.total_cmp(&a.distance));
    }

    /// The plates of pilots sealed in a vehicle (`hideRider`). `Ghost` gives such a
    /// pilot `SVF_NOCLIENT`, so no snapshot carries his entity; stock cgame finds him
    /// through the vehicle's `entityState_t::owner` (`CG_DrawCrosshair`'s target
    /// name), and the plate hangs over the vehicle. Only the name and team are
    /// known of him: no bars, icons or weapon.
    #[allow(clippy::too_many_arguments)]
    fn hidden_pilots(
        &mut self,
        snapshot: &Snapshot,
        game: &GameState,
        world: &sjk_runtime::World,
        standing: &Standing,
        now: i64,
        camera: Camera,
        bsp: &Bsp,
        scratch: &mut TraceScratch,
        step: f32,
    ) {
        let settings = self.settings;
        let local = snapshot.player.client_num();
        for vehicle in &snapshot.entities {
            let Some(number) = math::hidden_pilot(
                vehicle.entity_type(),
                vehicle.npc_class(),
                vehicle.owner(),
                local,
                |pilot| {
                    snapshot
                        .entities
                        .binary_search_by_key(&pilot, |state| state.number())
                        .is_ok()
                },
            ) else {
                continue;
            };
            if self.entries.len() >= MAX_TAGS {
                break;
            }
            let Some(who) = identify(game, &settings, standing, number, false) else {
                continue;
            };
            let Some(placed) = world
                .entity(sjk_runtime::EntityId::new(u64::from(vehicle.number()) + 1))
                .and_then(|presented| {
                    math::anchor(
                        None,
                        Some((presented.sample(now).translation, vehicle.solid())),
                    )
                })
            else {
                continue;
            };
            let origin = Vec3::from_array(placed.origin);
            let distance = origin.distance(camera.eye);
            if distance >= settings.range {
                continue;
            }
            let Some(point) = camera.project_within(
                origin + Vec3::Z * (placed.head + math::HEAD_CLEARANCE),
                SCREEN_MARGIN,
            ) else {
                continue;
            };
            let trace = bsp.trace_box_with(
                scratch,
                camera.eye.to_array(),
                origin.to_array(),
                Aabb::new([0.0; 3], [0.0; 3]).unwrap(),
                1 | 0x0200_0000,
            );
            let visibility = if unoccluded(trace.fraction, trace.start_solid, trace.all_solid) {
                1.0
            } else if settings.walls {
                WALL_OPACITY
            } else {
                0.0
            };
            let alpha = self.fade(
                number,
                math::distance_fade(distance, settings.range) * visibility,
                now,
                step,
            );
            if alpha < 0.02 {
                continue;
            }
            self.entries.push(Entry {
                number,
                npc_class: 0,
                point,
                distance,
                scale: math::distance_scale(distance, settings.range, MIN_SCALE),
                alpha,
                detail: 0.0,
                proximity: math::detail(distance, settings.near),
                powers: [0; MAX_ICONS],
                power_count: 0,
                health: None,
                shield: None,
                force: None,
                weapon: WP_NONE,
                style: 0,
                holstered: false,
                accent: who.accent,
                names_allowed: who.names_allowed,
                verified: self.verified & (1 << number) != 0,
                shared: false,
            });
        }
    }

    /// `cg_nameplateSelf`: the local player's own plate over their head in third
    /// person, with the real health, shield and Force the server sends them.
    fn own_plate(&mut self, snapshot: &Snapshot, mode: i32, now: i64, camera: Camera, step: f32) {
        let settings = self.settings;
        let player = &snapshot.player;
        let Some(origin) = self.own_origin else {
            return;
        };
        if player.health() <= 0 || player.is_spectator() || self.entries.len() >= MAX_TAGS {
            return;
        }
        let local = player.client_num();
        let distance = origin.distance(camera.eye);
        if distance >= settings.range {
            return;
        }
        // `CROUCH_VIEWHEIGHT` is 12, the standing one 36: the box top drops to 16.
        let head = if player.view_height() < 24 {
            16.0
        } else {
            math::head_height(0)
        };
        let Some(point) = camera.project_within(
            origin + Vec3::Z * (head + math::HEAD_CLEARANCE),
            SCREEN_MARGIN,
        ) else {
            return;
        };
        let alpha = self.fade(
            local,
            math::distance_fade(distance, settings.range),
            now,
            step,
        );
        if alpha < 0.02 {
            return;
        }
        let bars = settings.bars != 0;
        let full = player.max_health().max(1) as f32;
        let exact = |value: i32, full: f32| Range::exact(value.max(0) as f32).share_over(full);
        let shield = Some(exact(player.armor(), full));
        let entry = Entry {
            number: local,
            npc_class: 0,
            point,
            distance,
            scale: math::distance_scale(distance, settings.range, MIN_SCALE),
            alpha,
            detail: if bars {
                math::detail(distance, settings.near)
            } else {
                0.0
            },
            proximity: math::detail(distance, settings.near),
            powers: [0; MAX_ICONS],
            power_count: 0,
            health: bars.then(|| exact(player.health(), full)),
            shield: shield.filter(|_| bars),
            force: (bars && settings.force).then(|| {
                exact(
                    i32::from(player.force_power()),
                    sjk_game_jka::force_powers::FORCE_POWER_MAX as f32,
                )
            }),
            weapon: if settings.weapon {
                player.weapon()
            } else {
                WP_NONE
            },
            style: player.saber_style(),
            holstered: player.saber_holstered() != 0,
            accent: team_accent(mode, player.team() as i32),
            names_allowed: true,
            verified: self.verified & (1 << local) != 0,
            shared: self.sharing_live,
        };
        let (powers, power_count) = if settings.icons {
            icon_powers(player.force_powers_active())
        } else {
            ([0; MAX_ICONS], 0)
        };
        self.entries.push(Entry {
            powers,
            power_count,
            ..entry
        });
    }

    /// `cg_nameplateDebug`: what the server sends about each other player, to
    /// settle whether enemy health reaches the client.
    fn log_players(&self, snapshot: &Snapshot, game: &GameState, team_info: &TeamInfoTable) {
        let force_keys: Vec<String> = game
            .config_string(0)
            .map(|info| {
                String::from_utf8_lossy(info)
                    .trim_start_matches('\\')
                    .split('\\')
                    .collect::<Vec<_>>()
                    .chunks(2)
                    .filter(|pair| {
                        let key = pair[0].to_ascii_lowercase();
                        key.contains("force") || key.contains("regen")
                    })
                    .map(|pair| pair.join("="))
                    .collect()
            })
            .unwrap_or_default();
        eprintln!(
            "nameplate: regen pace {:.0} ms/point ({}; measured {:?}), server info force keys {force_keys:?}, pain events {:?}",
            self.force.regen_millis(),
            self.regen_source,
            self.calibration.millis_per_point().map(f32::round),
            self.vitals.pain_report(),
        );
        let local = snapshot.player.client_num();
        eprintln!(
            "nameplate: own Force actual {} estimated {}",
            snapshot.player.force_power(),
            shown(self.force.ratio(local).map(|r| r.map(|v| v * 100.0))),
        );
        for entity in &snapshot.entities {
            let number = entity.number();
            if entity.entity_type() != ET_PLAYER || number >= 32 || number == local {
                continue;
            }
            let team = team_info
                .entries()
                .iter()
                .find(|row| u16::from(row.client_num) == number)
                .map(|row| (row.health, row.armor));
            eprintln!(
                "nameplate: client {number} health {} max {} hp~{} armor~{} powers {:#x} fp~{} weapon {} style {} tinfo {team:?}",
                entity.health(),
                entity.max_health(),
                shown(self.vitals.health(number)),
                shown(self.vitals.armor(number)),
                entity.force_powers_active(),
                shown(self.force.ratio(number).map(|r| r.map(|v| v * 100.0))),
                entity.weapon(),
                entity.fire_flag(),
            );
        }
    }

    /// Feed every player's Force use to the estimator, shown or not, and
    /// measure the server's regeneration pace from the local player's own pool.
    ///
    /// The local player is tracked too, but never shown: its estimate is
    /// compared with its real pool in the `cg_nameplateDebug` log.
    fn observe_force(&mut self, snapshot: &Snapshot, game: &GameState, mode: i32) {
        let time = snapshot.server_time;
        let player = &snapshot.player;
        let boon = player.powerup_active(PW_FORCE_BOON as usize, time);
        let master = mode == GT_JEDIMASTER && player.is_jedi_master();
        // Draining, and being drained, hold the refill back for a while.
        if time < self.own_hold.saturating_sub(1_000) {
            self.own_hold = i32::MIN;
        }
        if player.force_powers_active() & (1 << FP_DRAIN) != 0 {
            self.own_hold = self.own_hold.max(time + 500);
        }
        if self.vitals.drained() & (1 << player.client_num()) != 0 {
            self.own_hold = self.own_hold.max(time + 800);
        }
        let special = player.weapon() == WP_SABER
            && sjk_game_jka::saber_rules::in_special(player.saber_move());
        let refilling = player.force_powers_active() & !(1 << FP_DRAIN) == 0
            && !player.saber_in_flight()
            && !special
            && !boon
            && !master;
        self.calibration.observe(
            time,
            i32::from(player.force_power()),
            refilling && time >= self.own_hold,
        );
        let info = game
            .config_string(0)
            .and_then(|b| sjk_client::LegacyClientInfo::new(b).integer("g_forceRegenTime"))
            .map(|millis| millis as f32);
        let measured = self.calibration.millis_per_point();
        self.regen_source = if measured.is_some() {
            "measured"
        } else if info.is_some() {
            "serverinfo"
        } else {
            "default"
        };
        self.force
            .set_regen_millis(measured.or(info), measured.is_some());
        for entity in &snapshot.entities {
            let number = entity.number();
            if entity.entity_type() != ET_PLAYER || number >= 32 {
                continue;
            }
            let stream = self.streams.effect(number);
            let facts = self.vitals.force_facts(number);
            // An absorb's hit sound while somebody else pushes or pulls, with no shot
            // of drain or lightning absorbed: the throw gave Force back.
            let absorbed_throw = facts.absorbed
                && stream.force[1] >= 0.0
                && snapshot.entities.iter().any(|other| {
                    other.entity_type() == ET_PLAYER
                        && other.number() != number
                        && self.force.throwing(other.torso_animation())
                });
            // A saber knocked away flies too, but its own entity says it was not thrown.
            let disarmed = entity.saber_in_flight()
                && snapshot
                    .entities
                    .binary_search_by_key(&entity.event_sound_channel(), |e| e.number())
                    .ok()
                    .is_some_and(|index| !snapshot.entities[index].saber_in_flight());
            let saber = entity.weapon() == WP_SABER;
            let regen_multiplier = if entity.powerups() & (1 << PW_FORCE_BOON) != 0 {
                6.0
            } else if mode == GT_JEDIMASTER && entity.is_jedi_master() {
                4.0
            } else {
                1.0
            };
            self.force.observe(
                number,
                force_estimate::Observation {
                    time,
                    active: entity.force_powers_active(),
                    torso_animation: entity.torso_animation(),
                    saber_in_flight: entity.saber_in_flight(),
                    saber_special: saber
                        && sjk_game_jka::saber_rules::in_special(entity.saber_move()),
                    regen_multiplier,
                    dead: entity.e_flags() & EF_DEAD != 0,
                    stream,
                    rising: entity.trajectory_delta()[2],
                    legs_animation: entity.leg_animation(),
                    saber_move: saber.then_some(entity.saber_move() as u16),
                    disarmed,
                    jedi_master: mode == GT_JEDIMASTER && entity.is_jedi_master(),
                    facts,
                    absorbed_throw,
                },
            );
        }
    }

    /// Text id of an entry's label.
    fn text_id(entry: &Entry) -> TextId {
        if entry.npc_class == 0 {
            TextId(u32::from(entry.number))
        } else {
            TextId(NPC_TEXT + u32::from(entry.npc_class))
        }
    }

    /// Lay the collected plates out as shapes and text commands.
    fn build<'a>(
        &mut self,
        label: &dyn Fn(TextId) -> &'a str,
        icons: &[Option<TextureId>],
        weapons: &super::icons::Icons,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        self.list.clear();
        let unit = crate::ui_scale::height_scale(viewport[1]);
        for index in 0..self.entries.len() {
            let entry = self.entries[index];
            let u = unit * entry.scale;
            let size = 36.0 * self.settings.scale * u;
            let id = Self::text_id(&entry);
            let rows = if entry.names_allowed {
                entry.rows()
            } else {
                Rows::default()
            };
            let [x, y] = entry.point;
            // Over a team logo (drawn by `super::team_logo` at the same anchor).
            let logo_top = if entry.npc_class == 0 {
                self.logo_tops
                    .get(usize::from(entry.number))
                    .copied()
                    .flatten()
            } else {
                None
            };
            let _ = self.list.push(DrawCommand::PushOpacity(entry.alpha));
            let bottom = logo_top.map_or(y, |top| top.min(y)) - 2.0 * u;
            let stack_height = Stack::height(rows, u);
            // The name rises as the plate fades in beneath it.
            let line = size * 1.15;
            let top = bottom - stack_height * entry.detail - line;
            if entry.names_allowed && !label(id).is_empty() {
                let width = (320.0 * unit).min(viewport[0]);
                let left = (x - width * 0.5).clamp(0.0, (viewport[0] - width).max(0.0));
                let opacity = FAR_NAME_OPACITY + (1.0 - FAR_NAME_OPACITY) * entry.proximity;
                let tint = if entry.npc_class == 0 {
                    Color::new(1.0, 1.0, 1.0, opacity)
                } else {
                    Color::new(1.0, 0.95, 0.75, opacity)
                };
                let _ = self.list.push(DrawCommand::Text {
                    rect: Rect::new(left, top, width, line),
                    text: id,
                    size,
                    color: tint,
                    align: TextAlign::Center,
                    overflow: TextOverflow::Ellipsis,
                    weight: FontWeight::Regular,
                    letter_spacing: 0.0,
                });
                if entry.verified {
                    let scale = size / font.height.max(1.0);
                    let text = crate::text::visible_text_width(font, label(id), scale).min(width);
                    let side = size * 0.95;
                    // Level with the name's capitals: the text hangs from the top of
                    // its line by the font's own baseline, not from the line's middle.
                    let badge = Rect::new(
                        (left + (width + text) * 0.5 + size * 0.15).min(viewport[0] - side),
                        top + font.capital_middle(scale) - side * 0.5,
                        side,
                        side,
                    );
                    let _ = self.list.push(DrawCommand::TexturedQuad {
                        rect: badge,
                        texture: crate::ui_renderer::VERIFIED_TEXTURE,
                        color: Color::new(1.0, 1.0, 1.0, opacity),
                    });
                }
            }
            if entry.power_count > 0 && entry.proximity > 0.01 {
                self.power_row(&entry, icons, [x, top], size, u, viewport);
            }
            // The weapon sits left of the plate, or of where it would be beside the name.
            let mut weapon_centre = top + line * 0.5;
            if rows.any() && entry.detail > 0.01 {
                let stack = Stack::new(x, bottom, rows, u, viewport);
                weapon_centre = stack.frame.y + stack.frame.height * 0.5;
                let _ = self.list.push(DrawCommand::PushOpacity(entry.detail));
                self.plate(&entry, &stack, u);
                let _ = self.list.push(DrawCommand::PopOpacity);
            }
            if entry.weapon != WP_NONE && entry.proximity > 0.01 {
                self.weapon_icon(&entry, weapons, [x, weapon_centre], u, viewport);
            }
            let _ = self.list.push(DrawCommand::PopOpacity);
        }
        self.mode_label(label, font, unit, viewport);
    }

    /// The name of the mode just chosen, near the top of the screen for a moment.
    fn mode_label<'a>(
        &mut self,
        label: &dyn Fn(TextId) -> &'a str,
        font: &UiFont,
        unit: f32,
        viewport: [f32; 2],
    ) {
        let Some((_, at)) = self.announced else {
            return;
        };
        let age = self.last_update - at;
        if !(0..MODE_SHOWN).contains(&age) {
            return;
        }
        let alpha = ((MODE_SHOWN - age) as f32 / MODE_FADE as f32).min(1.0);
        let size = 28.0 * unit;
        let text = crate::text::visible_text_width(
            font,
            label(TextId(MODE_TEXT)),
            size / font.height.max(1.0),
        );
        let width = (text + size * 1.6).min(viewport[0]);
        let rect = Rect::new(
            (viewport[0] - width) * 0.5,
            viewport[1] * 0.18,
            width,
            size * 1.6,
        );
        let _ = self.list.push(DrawCommand::PushOpacity(alpha));
        let _ = self.list.push(DrawCommand::RoundedRect {
            rect,
            radius: rect.height * 0.5,
            color: Color::new(0.03, 0.04, 0.06, 0.7),
        });
        let _ = self.list.push(DrawCommand::Text {
            rect: Rect::new(rect.x, rect.y + size * 0.2, rect.width, size * 1.2),
            text: TextId(MODE_TEXT),
            size,
            color: Color::new(1.0, 1.0, 1.0, 1.0),
            align: TextAlign::Center,
            overflow: TextOverflow::Ellipsis,
            weight: FontWeight::Semibold,
            letter_spacing: 0.0,
        });
        let _ = self.list.push(DrawCommand::PopOpacity);
    }

    /// The held weapon's icon, centred vertically on `centre_y` left of the plate,
    /// in a round backdrop; a saber's is ringed and haloed in its stance's colour
    /// (the Radial HUD's), dimmed while the blade is put away.
    fn weapon_icon(
        &mut self,
        entry: &Entry,
        weapons: &super::icons::Icons,
        [x, centre_y]: [f32; 2],
        u: f32,
        viewport: [f32; 2],
    ) {
        let Some(texture) = weapons.weapon_select(entry.weapon, false, entry.style) else {
            return;
        };
        let side = 22.0 * u;
        let width = Stack::WIDTH * u;
        let plate_left = (x - width * 0.5).clamp(0.0, (viewport[0] - width).max(0.0));
        let rect = Rect::new(
            (plate_left - 4.0 * u - side).max(0.0),
            centre_y - side * 0.5,
            side,
            side,
        );
        let radius = side * 0.5;
        let halo = (entry.weapon == WP_SABER && entry.style != 0).then(|| {
            let strength = if entry.holstered { HOLSTERED_HALO } else { 1.0 };
            (super::radial::saber_style_color(entry.style), strength)
        });
        let _ = self.list.push(DrawCommand::PushOpacity(entry.proximity));
        if let Some((color, strength)) = halo {
            let grow = 3.0 * u;
            let _ = self.list.push(DrawCommand::RoundedRect {
                rect: Rect::new(
                    rect.x - grow,
                    rect.y - grow,
                    side + grow * 2.0,
                    side + grow * 2.0,
                ),
                radius: radius + grow,
                color: Color::new(color.r, color.g, color.b, 0.3 * strength),
            });
        }
        let _ = self.list.push(DrawCommand::RoundedRect {
            rect,
            radius,
            color: ICON_BACKDROP,
        });
        if let Some((color, strength)) = halo {
            let _ = self.list.push(DrawCommand::Border {
                rect,
                radius,
                width: (1.6 * u).max(1.0),
                color: Color::new(color.r, color.g, color.b, 0.95 * strength),
            });
        }
        let inset = 3.5 * u;
        let _ = self.list.push(DrawCommand::TexturedQuad {
            rect: Rect::new(
                rect.x + inset,
                rect.y + inset,
                side - inset * 2.0,
                side - inset * 2.0,
            ),
            texture,
            color: Color::new(1.0, 1.0, 1.0, 1.0),
        });
        let _ = self.list.push(DrawCommand::PopOpacity);
    }

    /// The row of power icons centred over the name, whose top edge is `[x, top]`.
    fn power_row(
        &mut self,
        entry: &Entry,
        icons: &[Option<TextureId>],
        [x, top]: [f32; 2],
        size: f32,
        u: f32,
        viewport: [f32; 2],
    ) {
        let side = size;
        let gap = 2.0 * u;
        let shown = entry.powers[..usize::from(entry.power_count)]
            .iter()
            .filter(|power| icons.get(usize::from(**power)).copied().flatten().is_some())
            .count();
        if shown == 0 {
            return;
        }
        let width = shown as f32 * side + (shown - 1) as f32 * gap;
        let mut left = (x - width * 0.5).clamp(0.0, (viewport[0] - width).max(0.0));
        let y = top - gap - side;
        let _ = self.list.push(DrawCommand::PushOpacity(entry.proximity));
        for power in &entry.powers[..usize::from(entry.power_count)] {
            let Some(texture) = icons.get(usize::from(*power)).copied().flatten() else {
                continue;
            };
            let cell = Rect::new(left, y, side, side);
            let _ = self.list.push(DrawCommand::RoundedRect {
                rect: cell,
                radius: 3.0 * u,
                color: ICON_BACKDROP,
            });
            let inset = u.max(1.0);
            let _ = self.list.push(DrawCommand::TexturedQuad {
                rect: Rect::new(
                    left + inset,
                    y + inset,
                    side - inset * 2.0,
                    side - inset * 2.0,
                ),
                texture,
                color: Color::new(1.0, 1.0, 1.0, 1.0),
            });
            left += side + gap;
        }
        let _ = self.list.push(DrawCommand::PopOpacity);
    }

    /// The framed plate and its bars.
    fn plate(&mut self, entry: &Entry, stack: &Stack, u: f32) {
        let _ = self.list.push(DrawCommand::RoundedRect {
            rect: stack.frame,
            radius: 4.0 * u,
            color: Color::new(0.03, 0.04, 0.06, 0.62),
        });
        if entry.shared {
            // Gold for sharing; a team's colour stays as a thin line inside it.
            let _ = self.list.push(DrawCommand::Border {
                rect: stack.frame,
                radius: 4.0 * u,
                width: (1.6 * u).max(1.0),
                color: SHARED_CONTOUR,
            });
            if entry.accent != NEUTRAL_ACCENT {
                let inset = (1.6 * u).max(1.0);
                let _ = self.list.push(DrawCommand::Border {
                    rect: Rect::new(
                        stack.frame.x + inset,
                        stack.frame.y + inset,
                        stack.frame.width - inset * 2.0,
                        stack.frame.height - inset * 2.0,
                    ),
                    radius: (4.0 * u - inset).max(0.0),
                    width: (0.8 * u).max(1.0),
                    color: entry.accent,
                });
            }
        } else {
            let _ = self.list.push(DrawCommand::Border {
                rect: stack.frame,
                radius: 4.0 * u,
                width: (1.2 * u).max(1.0),
                color: entry.accent,
            });
        }
        let colors = self.colors;
        for (bar, range, color, meter) in [
            (stack.shield, entry.shield, colors.shield, Meter::Shield),
            (stack.health, entry.health, colors.health, Meter::Health),
            (stack.force, entry.force, colors.force, Meter::Force),
        ] {
            if let (Some(rect), Some(range)) = (bar, range) {
                self.meter(rect, range, color, meter, u);
            }
        }
    }

    /// One bar: its track, the fill to the guess with the haze over the uncertain
    /// stretch, and over a full bar (overheal, overshield) a second layer in a
    /// deeper shade from the left. An empty shield is a broken grey bar; health or
    /// shield too unsure to show dims under a yellow "?".
    fn meter(&mut self, rect: Rect, range: Range, color: Color, meter: Meter, u: f32) {
        let radius = rect.height * 0.5;
        let _ = self.list.push(DrawCommand::RoundedRect {
            rect,
            radius,
            color: Color::new(0.0, 0.0, 0.0, 0.6),
        });
        let outline = match meter {
            Meter::Force => math::lighter(color, 0.4, 0.75),
            _ => Color::new(1.0, 1.0, 1.0, 0.3),
        };
        if meter == Meter::Shield && range.high < EMPTY_SHARE {
            for (start, end) in math::broken_dashes() {
                let _ = self.list.push(DrawCommand::SolidRect {
                    rect: Rect::new(
                        rect.x + rect.width * start,
                        rect.y + rect.height * 0.2,
                        rect.width * (end - start),
                        rect.height * 0.6,
                    ),
                    color: math::EMPTY_SHIELD,
                });
            }
            let _ = self.list.push(DrawCommand::Border {
                rect,
                radius,
                width: u.max(1.0),
                color: Color::new(1.0, 1.0, 1.0, 0.15),
            });
            return;
        }
        let unsure = meter != Meter::Force && range.width() >= UNSURE_WIDTH;
        let _ = self
            .list
            .push(DrawCommand::PushOpacity(if unsure { 0.35 } else { 1.0 }));
        let layers = [
            (range.clamp(0.0, 1.0), color),
            (
                range.map(|share| (share - 1.0).clamp(0.0, 1.0)),
                math::saturated(color),
            ),
        ];
        for (index, (layer, shade)) in layers.into_iter().enumerate() {
            if index == 1 && range.high <= 1.0 {
                break;
            }
            // The second layer is an inner band, so the full bar shows round it.
            let band = if index == 0 {
                rect
            } else {
                math::overflow_band(rect)
            };
            if layer.best > 0.01 {
                let _ = self.list.push(DrawCommand::RoundedRect {
                    rect: Rect::new(
                        band.x,
                        band.y,
                        (band.width * layer.best).max(band.height),
                        band.height,
                    ),
                    radius: band.height * 0.5,
                    color: shade,
                });
            }
            self.haze(band, layer);
        }
        let _ = self.list.push(DrawCommand::PopOpacity);
        let _ = self.list.push(DrawCommand::Border {
            rect,
            radius,
            width: u.max(1.0),
            color: outline,
        });
        if unsure {
            // Sized to its bar, so the marks of two unsure bars do not meet.
            let size = rect.height * 1.3 + 2.5 * u;
            let _ = self.list.push(DrawCommand::Text {
                rect: Rect::new(
                    rect.x,
                    rect.y + (rect.height - size * 1.15) * 0.5,
                    rect.width,
                    size * 1.15,
                ),
                text: TextId(UNKNOWN_TEXT),
                size,
                color: math::UNKNOWN_MARK,
                align: TextAlign::Center,
                overflow: TextOverflow::Ellipsis,
                weight: FontWeight::Semibold,
                letter_spacing: 0.0,
            });
        }
    }

    /// The grey haze over the uncertain stretch of a bar: thickest at the guess,
    /// fading out towards either bound, so the bar's edge looks as blurred as the
    /// estimate is loose.
    fn haze(&mut self, rect: Rect, range: Range) {
        if range.width() < HAZE_MIN_WIDTH {
            return;
        }
        let at = |share: f32| rect.x + rect.width * share;
        let clear = Color::new(HAZE.r, HAZE.g, HAZE.b, 0.0);
        let _ = self.list.push(DrawCommand::PushClip(rect));
        for (from, to, start, end) in [
            (range.low, range.best, clear, HAZE),
            (range.best, range.high, HAZE, clear),
        ] {
            let width = at(to) - at(from);
            if width < 0.5 {
                continue;
            }
            let _ = self.list.push(DrawCommand::GradientRect {
                rect: Rect::new(at(from), rect.y, width, rect.height),
                radius: 0.0,
                gradient: Gradient {
                    start,
                    end,
                    vertical: false,
                },
            });
        }
        let _ = self.list.push(DrawCommand::PopClip);
    }

    /// Build the plates from the existing roster text at submission, never
    /// allocating another name store.
    pub(crate) fn append(
        &mut self,
        chat: &ChatOverlay,
        icons: &[Option<TextureId>],
        weapons: &super::icons::Icons,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        let mode = self.announced.map(|(mode, _)| mode);
        let text = |id: TextId| {
            if id.0 == MODE_TEXT {
                mode.map_or("", Mode::label)
            } else {
                label(chat, id)
            }
        };
        self.build(&text, icons, weapons, font, viewport);
        crate::ui_renderer::append_text_commands(
            &self.list,
            text,
            vertices,
            font,
            viewport,
            crate::text::TextStyle::NEUTRAL,
        );
    }
}

/// One line in the log for each drain of the local player: what its pool and health
/// measured, and whom it was put on, so a report says what the plates were told.
fn log_drain(run: &force_streams::Run, game: &GameState) {
    use std::fmt::Write;
    let mut victims = String::new();
    for (slot, taken) in run.taken.iter().enumerate() {
        if *taken > 0.0 {
            let name = game
                .config_string(1131 + slot)
                .and_then(|info| sjk_client::LegacyClientInfo::new(info).text("n"))
                .unwrap_or("?");
            let _ = write!(
                victims,
                "{}{} {:.0}",
                if victims.is_empty() { "" } else { ", " },
                crate::text::Plain(name),
                taken
            );
        }
    }
    let healed = run.healed.map_or_else(
        || "health at its maximum".to_owned(),
        |healed| format!("healed {healed:.0}"),
    );
    let [reach, event, front] = run.found;
    crate::log::progress(format_args!(
        "nameplate drain: {:.2} s, {:.0} shots paid ({:.0} Force), {healed}; taken: {} \
         (found in reach {reach}, by the drained event {event}, in front {front}); \
         pace {:.0} ms a shot",
        run.millis as f32 / 1_000.0,
        run.shots,
        run.shots * 5.0,
        if victims.is_empty() {
            "nothing"
        } else {
            &victims
        },
        run.pace,
    ));
}

/// The Force powers of `active` that get an icon, in display order, at most
/// [`MAX_ICONS`] of them.
fn icon_powers(active: u32) -> ([u8; MAX_ICONS], u8) {
    let mut powers = [0; MAX_ICONS];
    let mut count = 0;
    for power in ICON_POWERS {
        if active & (1 << power) != 0 && usize::from(count) < MAX_ICONS {
            powers[usize::from(count)] = power;
            count += 1;
        }
    }
    (powers, count)
}

/// What the viewer's own standing decides about every other player's plate.
struct Standing {
    mode: i32,
    restrictions: i32,
    local_team: i32,
    local_duel: i32,
    master: bool,
    local_master: bool,
}

/// How a player's plate is drawn for the viewer.
struct Identity {
    accent: Color,
    names_allowed: bool,
    ally: bool,
}

/// Player `number`'s plate identity from his `CS_PLAYERS` string; `None` where
/// he has none, is a spectator, or the server's restrictions hide him.
fn identify(
    game: &GameState,
    settings: &Settings,
    standing: &Standing,
    number: u16,
    jedi_master: bool,
) -> Option<Identity> {
    let info = game.config_string(1131 + usize::from(number));
    let team = info_number(info, "t");
    if info.is_none() || team == 3 {
        return None;
    }
    // An ally with a team logo keeps his plate where a server's restrictions hide
    // names, as stock keeps the ally marker.
    let logo = settings.friends
        && super::team_logo::ally_logo(
            standing.mode,
            standing.local_team,
            team,
            standing.local_duel,
            info_number(info, "ds"),
            standing.master,
            standing.local_master,
            jedi_master,
        )
        .is_some();
    if !logo && standing.restrictions & 64 != 0 {
        return None;
    }
    Some(Identity {
        accent: team_accent(standing.mode, team),
        names_allowed: standing.restrictions & 64 == 0,
        ally: standing.mode >= GT_TEAM && (team == 1 || team == 2) && team == standing.local_team,
    })
}

/// Health and shield shares of `entity` (up to 2: a second bar's worth over the
/// maximum): a teammate's from the team overlay (points out of a full 100), anyone's
/// from the entity state when the server sends health there, else the `predicted`
/// estimate.
fn bar_values(
    entity: &sjk_protocol::EntityState,
    number: u16,
    teammate: bool,
    team_info: &TeamInfoTable,
    predicted: Option<&vitals_estimate::Estimator>,
) -> (Option<Range>, Option<Range>) {
    if teammate
        && let Some(row) = team_info
            .entries()
            .iter()
            .find(|row| u16::from(row.client_num) == number)
    {
        let share = |points: i32| Range::exact(points as f32).share_over(FULL_POINTS);
        return (Some(share(row.health)), Some(share(row.armor)));
    }
    let maximum = entity.max_health() as f32;
    if maximum > 0.0 {
        return (
            Some(Range::exact(entity.health() as f32).share(maximum)),
            None,
        );
    }
    let Some(vitals) = predicted else {
        return (None, None);
    };
    let full = vitals.full();
    (
        vitals.health(number).map(|range| range.share_over(full)),
        vitals.armor(number).map(|range| range.share_over(full)),
    )
}

/// A range in points for the debug log.
fn shown(range: Option<Range>) -> String {
    range.map_or_else(
        || "-".to_owned(),
        |range| format!("{:.0}[{:.0}..{:.0}]", range.best, range.low, range.high),
    )
}

/// Frame colour for a player on `team` in game type `mode`: red or blue in
/// team games, neutral otherwise.
fn team_accent(mode: i32, team: i32) -> Color {
    match (mode >= GT_TEAM, team) {
        (true, 1) => Color::new(1.0, 0.3, 0.25, 0.95),
        (true, 2) => Color::new(0.3, 0.6, 1.0, 0.95),
        _ => NEUTRAL_ACCENT,
    }
}

/// The bars' colours.
#[derive(Clone, Copy, Debug)]
struct BarColors {
    health: Color,
    shield: Color,
    force: Color,
}

impl Default for BarColors {
    fn default() -> Self {
        Self {
            health: math::HEALTH_COLOR,
            shield: math::SHIELD_COLOR,
            force: math::FORCE_COLOR,
        }
    }
}

/// Which bar [`State::meter`] draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Meter {
    Shield,
    Health,
    Force,
}

/// Text of a plate: a roster name, an NPC class name, or the "?" over a bar.
fn label(chat: &ChatOverlay, id: TextId) -> &str {
    if id.0 == UNKNOWN_TEXT {
        return "?";
    }
    match id.0.checked_sub(NPC_TEXT) {
        None => chat.player_label(id.0 as u16),
        Some(class) => super::npc_class::name(class as u8).unwrap_or(""),
    }
}

/// A plate for the off-screen snapshots (`menu_snapshot.rs`), its bars as shares of
/// a full one: `[low, guess, high]`.
#[cfg(test)]
pub(crate) struct PreviewPlate {
    pub(crate) slot: u16,
    pub(crate) point: [f32; 2],
    pub(crate) distance: f32,
    pub(crate) health: Option<[f32; 3]>,
    pub(crate) shield: Option<[f32; 3]>,
    pub(crate) force: Option<[f32; 3]>,
    pub(crate) weapon: u8,
    pub(crate) style: u8,
    pub(crate) verified: bool,
    /// The frame's colour: red or blue team, else neutral.
    pub(crate) team: Option<bool>,
    /// The gold contour of a player who shares their bars.
    pub(crate) shared: bool,
}

#[cfg(test)]
impl State {
    /// Lay out `plates` as [`State::append`] does, naming slot `n` `names[n]`.
    pub(crate) fn preview(
        &mut self,
        plates: &[PreviewPlate],
        names: &[&str],
        weapons: &super::icons::Icons,
        font: &UiFont,
        vertices: &mut Vec<TextVertex>,
        viewport: [f32; 2],
    ) {
        let range = |share: [f32; 3]| Range::new(share[0], share[1], share[2]);
        self.entries.clear();
        for plate in plates {
            let proximity = math::detail(plate.distance, self.settings.near);
            self.entries.push(Entry {
                number: plate.slot,
                npc_class: 0,
                point: plate.point,
                distance: plate.distance,
                scale: math::distance_scale(plate.distance, self.settings.range, MIN_SCALE),
                alpha: 1.0,
                detail: proximity,
                proximity,
                powers: [0; MAX_ICONS],
                power_count: 0,
                health: plate.health.map(range),
                shield: plate.shield.map(range),
                force: plate.force.map(range),
                weapon: plate.weapon,
                style: plate.style,
                holstered: false,
                accent: match plate.team {
                    Some(red) => team_accent(GT_TEAM, if red { 1 } else { 2 }),
                    None => NEUTRAL_ACCENT,
                },
                names_allowed: true,
                verified: plate.verified,
                shared: plate.shared,
            });
        }
        let label = |id: TextId| {
            if id.0 == UNKNOWN_TEXT {
                "?"
            } else {
                names.get(id.0 as usize).copied().unwrap_or("")
            }
        };
        self.build(&label, &[], weapons, font, viewport);
        crate::ui_renderer::append_text_commands(
            &self.list,
            label,
            vertices,
            font,
            viewport,
            crate::text::TextStyle::NEUTRAL,
        );
    }
}

impl crate::GpuState {
    /// `nameplates [off|names|target|all]`: the next mode, or the one named; it is
    /// shown on screen for a moment.
    pub(crate) fn nameplate_command(&mut self, args: &[String]) -> Result<Vec<String>, String> {
        let console = self
            .console
            .as_mut()
            .ok_or_else(|| "no console".to_owned())?;
        let current = Mode::of(
            console.bool_cvar("cg_nameplate").unwrap_or(true),
            console.integer_cvar("cg_nameplateBars").unwrap_or(2),
        );
        let mode = match args.first() {
            None => current.next(),
            Some(word) => Mode::named(word)
                .ok_or_else(|| "usage: nameplates [off | names | target | all]".to_owned())?,
        };
        let (enabled, bars) = mode.settings();
        console.set_cvar("cg_nameplate", if enabled { "1" } else { "0" });
        if let Some(bars) = bars {
            console.set_cvar("cg_nameplateBars", &bars.to_string());
        }
        self.hud.nameplate.announce(mode);
        Ok(vec![mode.label().to_owned()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v_cycles_off_names_target_everyone_and_round() {
        let mut mode = Mode::Off;
        let mut seen = Vec::new();
        for _ in 0..4 {
            mode = mode.next();
            seen.push(mode);
        }
        assert_eq!(seen, [Mode::Names, Mode::Target, Mode::All, Mode::Off]);
        // Each mode's settings read back as that mode.
        for mode in seen {
            let (enabled, bars) = mode.settings();
            assert_eq!(Mode::of(enabled, bars.unwrap_or(2)), mode);
        }
        // Bars on allies only sits between names and target.
        assert_eq!(Mode::of(true, 1).next(), Mode::Target);
        assert_eq!(Mode::named("TARGET"), Some(Mode::Target));
        assert_eq!(Mode::named("everyone"), Some(Mode::All));
        assert_eq!(Mode::named("x"), None);
    }

    #[test]
    fn the_mode_name_shows_for_a_moment_then_goes() {
        let mut state = State {
            last_update: 10_000,
            ..State::default()
        };
        state.announce(Mode::Target);
        let font = crate::text::load_modern(1.0, None).unwrap().font;
        let label = |id: TextId| {
            if id.0 == MODE_TEXT {
                Mode::Target.label()
            } else {
                ""
            }
        };
        let texts = |state: &mut State| {
            state.list.clear();
            state.mode_label(&label, &font, 1.0, [1920.0, 1080.0]);
            state
                .list
                .commands()
                .iter()
                .filter(|command| matches!(command, DrawCommand::Text { .. }))
                .count()
        };
        assert_eq!(texts(&mut state), 1);
        state.last_update = 10_000 + MODE_SHOWN;
        assert_eq!(texts(&mut state), 0);
    }

    #[test]
    fn icons_list_the_continuous_powers_dark_side_first() {
        let active = (1 << 9) | (1 << 7) | (1 << 2);
        assert_eq!(icon_powers(active), ([7, 9, 2, 0], 3));
    }

    #[test]
    fn instant_powers_get_no_icon_and_the_row_is_capped() {
        // Jump, push, pull and the saber powers are skipped.
        let instant = (1 << 1) | (1 << 3) | (1 << 4) | (1 << 15) | (1 << 16) | (1 << 17);
        assert_eq!(icon_powers(instant).1, 0);
        let many = u32::MAX & !instant;
        let (powers, count) = icon_powers(many);
        assert_eq!(usize::from(count), MAX_ICONS);
        assert_eq!(powers, [7, 6, 13, 8]);
    }

    #[test]
    fn the_verified_badge_sits_level_with_the_capitals() {
        // The HUD font the plates use in game hangs its glyphs from the line top by
        // its own baseline, low in the line box.
        let font = crate::text::load_classic().expect("the HUD font").font;
        let plate = PreviewPlate {
            slot: 0,
            point: [400.0, 300.0],
            distance: 200.0,
            health: None,
            shield: None,
            force: None,
            weapon: WP_NONE,
            style: 0,
            verified: true,
            team: None,
            shared: false,
        };
        let mut state = State::default();
        let icons = super::super::icons::Icons::default();
        state.preview(
            &[plate],
            &["Hello"],
            &icons,
            &font,
            &mut Vec::new(),
            [800.0, 600.0],
        );
        let commands = state.list.commands();
        let (line, size) = commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::Text { rect, size, .. } => Some((*rect, *size)),
                _ => None,
            })
            .expect("the name");
        let badge = commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::TexturedQuad { rect, texture, .. }
                    if *texture == crate::ui_renderer::VERIFIED_TEXTURE =>
                {
                    Some(*rect)
                }
                _ => None,
            })
            .expect("the badge");
        let capital = font.glyph(crate::text::TextFace::Regular, b'H');
        let scale = size / font.height;
        let middle = line.y + (capital.offset_y + capital.height * 0.5) * scale;
        assert!(
            (badge.y + badge.height * 0.5 - middle).abs() < 0.01,
            "{badge:?} {middle}"
        );
        // After the name, not over it.
        let width = crate::text::visible_text_width(&font, "Hello", scale);
        assert!(badge.x >= line.x + (line.width + width) * 0.5);
    }

    /// Snapshots of the local player (0) at the origin looking along +x, draining
    /// when `draining`, and player 3 standing `ahead` units in front.
    mod drain {
        use super::*;
        use sjk_protocol::{EntityState, LEGACY_ENTITY_FIELDS, PlayerState};

        pub(super) fn snapshot(time: i32, draining: bool, ahead: f32, pool: u32) -> Snapshot {
            let mut player = PlayerState::zero();
            player.set_client_num(0);
            player.stats[0] = 100;
            player.stats[8] = 100;
            player.set_raw_field(18, pool);
            player.set_origin([0.0, 0.0, 24.0]);
            player.set_view_angles([0.0, 0.0, 0.0]);
            if draining {
                player.set_raw_field(82, 1 << FP_DRAIN);
            }
            let mut victim = EntityState::zero(3, &LEGACY_ENTITY_FIELDS);
            victim.set_raw_field(8, u32::from(ET_PLAYER));
            for (field, value) in [2, 1, 4].into_iter().zip([ahead, 0.0, 24.0]) {
                victim.set_raw_field(field, value.to_bits());
            }
            Snapshot {
                message_sequence: 0,
                reliable_acknowledge: 0,
                server_commands: Vec::new(),
                server_time: time,
                delta_from: None,
                flags: 0,
                area_mask: Vec::new(),
                player,
                vehicle_player: None,
                entities: vec![victim],
                consumed_bits: 0,
            }
        }
    }

    #[test]
    fn your_drain_empties_the_bar_of_the_player_in_front() {
        let game = GameState::empty_local(0);
        let bsp = Bsp::empty([-4_096.0; 3], [4_096.0; 3]);
        let mut scratch = TraceScratch::default();
        let mut state = State::default();
        state.settings.drain_level = Some(3);
        let mut time = 1_000;
        for _ in 0..10 {
            state.observe_snapshot(
                &drain::snapshot(time, false, 200.0, 100),
                &game,
                &bsp,
                &mut scratch,
            );
            time += 25;
        }
        assert_eq!(state.force.ratio(3).map(|r| r.best), Some(1.0));
        // Half a second of level 3 drain at full health: 10 shots paid (5 each, every
        // other snapshot), 4 taken each.
        let mut pool = 100;
        for step in 0..20 {
            if step % 2 == 0 {
                pool -= 5;
            }
            state.observe_snapshot(
                &drain::snapshot(time, true, 200.0, pool),
                &game,
                &bsp,
                &mut scratch,
            );
            time += 25;
        }
        let left = state.force.ratio(3).expect("tracked").best * 100.0;
        assert!((left - 60.0).abs() < 4.5, "{left}");
    }
}
