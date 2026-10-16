//! Declarative settings catalog shared by the settings screen.
//!

//! Every row here is backed by a consumer: a cvar that nothing reads yet is
//! not offered. Player identity
//! (name, model, sabers) lives in the Player menu, not here.

pub(super) const TABS: [&str; 9] = [
    "VIDEO",
    "AUDIO",
    "HUD",
    "CONTROLS",
    "GAME",
    "NETWORK",
    "HUD+",
    "TEXT",
    FIRST_SETUP_CAPTION,
];
/// The first-start tab's caption, also the classic Setup group's name.
pub(crate) const FIRST_SETUP_CAPTION: &str = "FIRST SETUP";
/// The first-start tab (`settings/quick.rs`); last, so the other tabs keep their numbers.
pub(super) const QUICK_TAB: usize = 8;
/// The tab the renderer settings ([`RENDERER_TABS`]) open from.
pub(super) const RENDERER_TAB: usize = 0;
/// Tabs of the renderer settings, SJK's own rendering cvars, which the
/// classic Graphics page's panels and the SJK UI's Graphics show.
pub(super) const RENDERER_TABS: [&str; 4] = ["IMAGE", "LIGHTING", "SHADOWS", "WEATHER"];

#[derive(Clone, Copy)]
pub(super) enum ValueKind {
    /// On/off; an integer cvar reads nonzero as on and is written 0 or 1.
    Bool,
    Integer {
        min: i64,
        max: i64,
        step: i64,
    },
    Float {
        min: f64,
        max: f64,
        step: f64,
    },
    Choice(&'static [&'static str]),
    Text,
    /// `r_resolution`: steps within the aspect group, Enter opens the list.
    Resolution,
    /// `r_fullscreen` with `r_exclusiveFullscreen`, as named modes.
    DisplayMode,
    /// The HUD in use (`cg_hudStyle`, `cg_hudFiles`, `cg_hudPack`): steps
    /// through the HUDs, Enter opens the HUD picker with its previews.
    HudPicker,
    /// The quick wheel's pages (`wheel.json`, not a cvar): Enter opens their
    /// editor ([`super::wheel_editor`]).
    WheelPages,
    /// The graphics quality level ([`crate::graphics_quality`], not a cvar):
    /// steps or lists the levels, each setting the renderer's costly cvars.
    Quality,
    /// The SJK identity's key (not a cvar): Enter opens the Identity page, where the
    /// key's id and file stay hidden until shown.
    IdentityPage,
    /// Import from another client (not a cvar): Enter opens the Import page with
    /// the configs found, or the file dialog over it ([`crate::config_import`]).
    ImportPage,
    /// Open the installed PK3 pack browser.
    AssetBrowser,
    /// `cl_gameplayFps`: Left/Right step through the presets (125, 142, 250,
    /// 333); Enter opens the card that explains it and takes any value
    /// ([`crate::gameplay_fps`]).
    GameplayFps,
}

/// The "SJK identity key" row's name in place of a cvar: the command that opens the
/// same page.
pub(crate) const IDENTITY_ROW: &str = "identity";

/// The "Import from a client" row's name in place of a cvar: the command that
/// opens the same page.
pub(crate) const IMPORT_ROW: &str = "firstsetup import";

/// The renderer's two choices ([`crate::graphics_quality`]), named "Vanilla"
/// and "SJK (alpha)" on their segments ([`super::help::choice_name`]).
pub(super) const RENDERERS: [&str; 2] = [
    crate::graphics_quality::VANILLA,
    crate::graphics_quality::SJK,
];

/// The Gameplay FPS row, shared by Video and First setup.
pub(super) const GAMEPLAY_FPS_ROW: Setting = Setting {
    label: "Gameplay FPS",
    cvar: crate::gameplay_fps::CVAR,
    kind: ValueKind::GameplayFps,
};

#[derive(Clone, Copy)]
pub(super) struct Setting {
    pub(super) label: &'static str,
    pub(super) cvar: &'static str,
    pub(super) kind: ValueKind,
}

pub(crate) const RESOLUTIONS: &[&str] = &[
    "1280x720",
    "1600x900",
    "1920x1080",
    "2560x1440",
    "3840x2160",
];
pub(super) const VIDEO: &[Setting] = &[
    Setting {
        label: "Renderer (reload)",
        cvar: crate::graphics_quality::RENDERER_ROW,
        kind: ValueKind::Choice(&RENDERERS),
    },
    Setting {
        label: "SJK quality (reload)",
        cvar: crate::graphics_quality::ROW_NAME,
        kind: ValueKind::Quality,
    },
    Setting {
        label: "Resolution",
        cvar: "r_resolution",
        kind: ValueKind::Resolution,
    },
    Setting {
        label: "Display mode",
        cvar: "r_fullscreen",
        kind: ValueKind::DisplayMode,
    },
    Setting {
        label: "Vertical sync",
        cvar: "r_vsync",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "FPS cap (AUTO, 0 = off)",
        cvar: "com_maxfps",
        kind: ValueKind::Integer {
            min: -1,
            max: 2000,
            step: 25,
        },
    },
    GAMEPLAY_FPS_ROW,
    Setting {
        label: "Ignore server caps",
        cvar: crate::server_caps::OVERRIDE_CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Detect refresh rate",
        cvar: crate::runtime_settings::MONITOR_CAP_CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Field of view",
        cvar: "cg_fov",
        kind: ValueKind::Float {
            min: 70.0,
            max: 130.0,
            step: 5.0,
        },
    },
    Setting {
        label: "Impact marks",
        cvar: "cg_marks",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Player shadows",
        cvar: "cg_shadows",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Brightness",
        cvar: "r_gamma",
        kind: ValueKind::Float {
            min: 0.5,
            max: 3.0,
            step: 0.1,
        },
    },
];
pub(super) const AUDIO: &[Setting] = &[
    Setting {
        label: "Effects volume",
        cvar: "s_volume",
        kind: ValueKind::Float {
            min: 0.0,
            max: 1.0,
            step: 0.05,
        },
    },
    Setting {
        label: "Music volume",
        cvar: "s_musicVolume",
        kind: ValueKind::Float {
            min: 0.0,
            max: 1.0,
            step: 0.05,
        },
    },
    Setting {
        label: "Doppler",
        cvar: "s_doppler",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Footsteps",
        cvar: "cg_footsteps",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Footprints and dust",
        cvar: crate::footprints::PRINTS_CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Jump and landing steps",
        cvar: crate::footprints::JUMP_CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Ground by texture name",
        cvar: crate::footprints::GUESS_CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Mute in background",
        cvar: "snd_mute_losefocus",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Emote volume",
        cvar: crate::emotes::VOLUME_CVAR,
        kind: ValueKind::Float {
            min: 0.0,
            max: 3.0,
            step: 0.1,
        },
    },
    Setting {
        label: "Unlock sound",
        cvar: crate::unlock_toast::SOUND_CVAR,
        kind: ValueKind::Bool,
    },
];
pub(super) const HUD_OPTIONS: &[Setting] = &[
    Setting {
        label: "Crosshair size",
        cvar: "cg_crosshairSize",
        kind: ValueKind::Float {
            min: 0.0,
            max: 96.0,
            step: 4.0,
        },
    },
    Setting {
        label: "Speedometer",
        cvar: "cg_speedometer",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Strafe lines",
        cvar: crate::hud::movement::LINES_ROW,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Scoreboard style",
        cvar: crate::scoreboard::style::CVAR,
        kind: ValueKind::Choice(&crate::scoreboard::style::ScoreboardStyle::NAMES),
    },
    Setting {
        label: "Compact SJK scoreboard",
        cvar: crate::scoreboard::style::COMPACT_CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Scoreboard frame rate",
        cvar: crate::scoreboard::style::FPS_CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Scoreboard client IDs",
        cvar: "cg_showClientIDs",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Clan emblems",
        cvar: crate::jof_tag::CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Scoreboard head icons",
        cvar: "cg_drawScoreboardIcons",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Small scoreboard rows",
        cvar: "cg_smallScoreboard",
        kind: ValueKind::Bool,
    },
];

pub(super) const HUD: &[Setting] = &[
    Setting {
        label: "HUD",
        cvar: "cg_drawHud",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "HUD scale",
        cvar: "cg_hudScale",
        kind: ValueKind::Float {
            min: 0.5,
            max: 1.5,
            step: 0.05,
        },
    },
    Setting {
        label: "Status (health / armour / force)",
        cvar: "cg_drawStatus",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Weapon bar",
        cvar: "cg_drawWeapon",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Crosshair (0 off)",
        cvar: "cg_crosshair",
        kind: ValueKind::Integer {
            min: 0,
            max: 10,
            step: 1,
        },
    },
    Setting {
        label: "Crosshair names",
        cvar: "cg_drawCrosshairNames",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Damage hitmarker",
        cvar: "cg_hitMarker",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Overhead names",
        cvar: "cg_drawPlayerNames",
        kind: ValueKind::Integer {
            min: 0,
            max: 2,
            step: 1,
        },
    },
    Setting {
        label: "Name size",
        cvar: "cg_drawPlayerNamesScale",
        kind: ValueKind::Float {
            min: 0.2,
            max: 1.5,
            step: 0.1,
        },
    },
    Setting {
        label: "Team logos",
        cvar: "cg_drawFriend",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Player card",
        cvar: "cg_playerCard",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Card delay (s)",
        cvar: "cg_playerCardDelay",
        kind: ValueKind::Float {
            min: 0.5,
            max: 5.0,
            step: 0.5,
        },
    },
    Setting {
        label: "Card outline",
        cvar: "cg_playerCardOutline",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Team outlines",
        cvar: crate::hud::team_outline::CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Nameplates",
        cvar: "cg_nameplate",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Nameplate range",
        cvar: "cg_nameplateRange",
        kind: ValueKind::Integer {
            min: 500,
            max: 10000,
            step: 250,
        },
    },
    Setting {
        label: "Bars distance",
        cvar: "cg_nameplateNear",
        kind: ValueKind::Integer {
            min: 0,
            max: 5000,
            step: 100,
        },
    },
    Setting {
        label: "Nameplate size",
        cvar: "cg_nameplateScale",
        kind: ValueKind::Float {
            min: 0.2,
            max: 1.5,
            step: 0.1,
        },
    },
    Setting {
        label: "Nameplate bars",
        cvar: "cg_nameplateBars",
        kind: ValueKind::Integer {
            min: 0,
            max: 3,
            step: 1,
        },
    },
    Setting {
        label: "Power icons",
        cvar: "cg_nameplateIcons",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Force bar (estimated)",
        cvar: "cg_nameplateForce",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Share my bars",
        cvar: "cg_nameplateShare",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Bars on everyone",
        cvar: "cg_nameplateEstimate",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Weapon icon",
        cvar: "cg_nameplateWeapon",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "My own nameplate",
        cvar: "cg_nameplateSelf",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Plates through walls",
        cvar: "cg_nameplateWalls",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "NPC nameplates",
        cvar: "cg_nameplateNpcs",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Others' world pings",
        cvar: crate::world_ping::SHOW_CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "World ping sound",
        cvar: crate::world_ping::SOUND_CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "FPS / frame-time readout",
        cvar: "cg_drawFps",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Match timer",
        cvar: "cg_drawTimer",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Upper right clock",
        cvar: "cg_drawTimerUpperRight",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Version and date",
        cvar: crate::version_overlay::CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Lagometer",
        cvar: "cg_lagometer",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Lagometer scale",
        cvar: "cg_lagometerScale",
        kind: ValueKind::Float {
            min: 0.25,
            max: 4.0,
            step: 0.25,
        },
    },
    Setting {
        label: "Kill feed",
        cvar: "cg_killfeed",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Chat",
        cvar: "cg_drawChat",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Chat emojis",
        cvar: crate::chat::emoji::CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Remember chat channel",
        cvar: crate::chat::reply::REMEMBER_CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Chat letter spacing",
        cvar: "cg_chatBoxLetterSpacing",
        kind: ValueKind::Float {
            min: -2.0,
            max: 8.0,
            step: 0.5,
        },
    },
    Setting {
        label: "HUD look (Enter: pick)",
        cvar: crate::menu_hud::STYLE_CVAR,
        kind: ValueKind::HudPicker,
    },
    Setting {
        label: "Game HUD files",
        cvar: crate::menu_hud::FILES_CVAR,
        kind: ValueKind::Text,
    },
    Setting {
        label: "Classic HUD font",
        cvar: "cg_classicHudFont",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Classic game fonts",
        cvar: crate::game_font::CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Ground HUD (third person)",
        cvar: crate::ground_hud::CVAR,
        kind: ValueKind::Bool,
    },
];
pub(super) const CONTROLS: &[Setting] = &[
    Setting {
        label: "Mouse sensitivity",
        cvar: "sensitivity",
        kind: ValueKind::Float {
            min: 0.1,
            max: 20.0,
            step: 0.25,
        },
    },
    Setting {
        label: "Invert mouse",
        cvar: "m_invert",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Always run",
        cvar: "cl_run",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Last key wins",
        cvar: crate::input::idrive::CVAR,
        kind: ValueKind::Choice(&["0", "1", "2"]),
    },
    Setting {
        label: "Last key wins delay (ms)",
        cvar: crate::input::idrive::DELAY_CVAR,
        kind: ValueKind::Integer {
            min: 0,
            max: crate::input::idrive::MAX_DELAY_MILLIS as i64,
            step: 5,
        },
    },
    Setting {
        label: "Hold push/pull repeats",
        cvar: crate::input::force_repeat::CVAR,
        kind: ValueKind::Bool,
    },
];
pub(super) const GAME: &[Setting] = &[
    Setting {
        label: "Asset browser",
        cvar: "assetbrowser",
        kind: ValueKind::AssetBrowser,
    },
    Setting {
        label: "Simple pickup icons",
        cvar: "cg_simpleItems",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Show everyone as my model",
        cvar: "cg_forceModel",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Saber trail",
        cvar: "cg_saberTrail",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Saber rain steam",
        cvar: crate::effect_aux::saber_rain_steam::CVAR,
        kind: ValueKind::Choice(&["0", "1", "2"]),
    },
    Setting {
        label: "Force Speed trail",
        cvar: "cg_speedTrail",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Shield hit sphere",
        cvar: "cg_shieldSphere",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Shield hit brightness",
        cvar: "cg_shieldBrightness",
        kind: ValueKind::Integer {
            min: 1,
            max: 12,
            step: 1,
        },
    },
    Setting {
        label: "Force Seeing aura",
        cvar: "cg_auraShell",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Protect+Absorb combo",
        cvar: "cg_spProtAbsColor",
        kind: ValueKind::Bool,
    },
    Setting {
        // SJK defaults to 2 (EternalJK); 1 excludes player-texture configstring remaps.
        label: "Shader remaps",
        cvar: "cg_remaps",
        kind: ValueKind::Choice(&["0", "1", "2"]),
    },
    Setting {
        label: "Camera style",
        cvar: crate::camera::STYLE_CVAR,
        kind: ValueKind::Choice(&crate::camera::Style::NAMES),
    },
    Setting {
        label: "First-person weapon",
        cvar: "cg_drawGun",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Third-person camera damping",
        cvar: "cg_thirdPersonCameraDamp",
        kind: ValueKind::Float {
            min: 0.0,
            max: 1.0,
            step: 0.05,
        },
    },
    Setting {
        label: "Third-person target damping",
        cvar: "cg_thirdPersonTargetDamp",
        kind: ValueKind::Float {
            min: 0.0,
            max: 1.0,
            step: 0.05,
        },
    },
    Setting {
        label: "Prediction error smoothing (ms)",
        cvar: "cg_errorDecay",
        kind: ValueKind::Float {
            min: 0.0,
            max: 500.0,
            step: 25.0,
        },
    },
    Setting {
        label: "Rarity effects",
        cvar: crate::rarity_fx::CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Menu style",
        cvar: crate::menu::style::CVAR,
        kind: ValueKind::Choice(&crate::menu::style::MenuStyle::NAMES),
    },
    Setting {
        label: "Quick wheel pages",
        cvar: crate::quick_wheel::pages::FILE,
        kind: ValueKind::WheelPages,
    },
    Setting {
        label: "Quick wheel sounds",
        cvar: crate::quick_wheel::SOUNDS_CVAR,
        kind: ValueKind::Bool,
    },
    // The client mods (`docs/mods.md`): on, each loading on its servers only.
    Setting {
        label: "JA+ tools",
        cvar: "mod_japlus",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "JoF tools",
        cvar: "mod_jof",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Invisible brushes",
        cvar: crate::brush_view::CVAR,
        kind: ValueKind::Choice(&["0", "1", "2"]),
    },
    Setting {
        label: "Map entities",
        cvar: crate::entity_view::CVAR,
        kind: ValueKind::Choice(&["0", "1", "2"]),
    },
    // Experimental features, off by default.
    Setting {
        label: "Experimental: showcase tools (ghost, mimic)",
        cvar: crate::ghost_replay::SHOWCASE_CVAR,
        kind: ValueKind::Bool,
    },
];
pub(super) const NETWORK: &[Setting] = &[
    Setting {
        label: "Master server",
        cvar: "cl_master",
        kind: ValueKind::Text,
    },
    Setting {
        label: "Second master server",
        cvar: "cl_master2",
        kind: ValueKind::Text,
    },
    Setting {
        label: "Check for updates",
        cvar: "cl_autoUpdate",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "SJK identity",
        cvar: "cl_identity",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Share my model list",
        cvar: crate::installed_models::CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "SJK hub",
        cvar: "cl_hubUrl",
        kind: ValueKind::Text,
    },
    Setting {
        label: "SJK chat",
        cvar: "cl_sjkChat",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "SJK chat sound",
        cvar: crate::sjk_chat_frame::SOUND_CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "SJK chat GIFs",
        cvar: crate::chat_gifs::CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "GIPHY key",
        cvar: crate::chat_gifs::search::KEY_CVAR,
        kind: ValueKind::Text,
    },
    Setting {
        label: "Send crash reports",
        cvar: crate::crash_report::CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Share settings usage",
        cvar: crate::settings_usage::CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "SJK identity key",
        cvar: IDENTITY_ROW,
        kind: ValueKind::IdentityPage,
    },
    Setting {
        label: "Rate (bytes/s)",
        cvar: "rate",
        kind: ValueKind::Integer {
            min: 1000,
            max: 100000,
            step: 1000,
        },
    },
    Setting {
        label: "Snapshots per second",
        cvar: "snaps",
        kind: ValueKind::Integer {
            min: 10,
            max: 125,
            step: 5,
        },
    },
];
/// Text size and spacing, and the console's style and feed. Menu rows keep their layout.
pub(super) const TEXT: &[Setting] = &[
    Setting {
        label: "Console style",
        cvar: crate::console::console_options::STYLE_CVAR,
        kind: ValueKind::Choice(&crate::console::console_options::ConsoleStyle::NAMES),
    },
    Setting {
        label: "Menu text size",
        cvar: crate::text::style::SCALE_CVAR,
        kind: ValueKind::Float {
            min: 0.8,
            max: 1.2,
            step: 0.05,
        },
    },
    Setting {
        label: "Letter spacing (menus, console)",
        cvar: crate::text::style::TRACKING_CVAR,
        kind: ValueKind::Float {
            min: -0.05,
            max: 0.15,
            step: 0.01,
        },
    },
    Setting {
        label: "Console text size",
        cvar: "con_scale",
        kind: ValueKind::Float {
            min: 0.5,
            max: 2.0,
            step: 0.05,
        },
    },
    Setting {
        label: "Console feed (top left)",
        cvar: crate::console::console_options::DRAW_NOTIFY_CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Other clients' text",
        cvar: crate::text::charset::CVAR,
        kind: ValueKind::Choice(&crate::text::charset::NAMES),
    },
];

// Renderer settings. "(reload)" marks cvars the renderer reads only when its graphics
// start (their registrations print a notice on change and the graphics reload card
// offers to apply them, `graphics_reload.rs`); "(next map)" those read
// when a map loads. The rest apply immediately. Ranges follow each consumer's clamp.

pub(super) const RENDER_IMAGE: &[Setting] = &[
    Setting {
        label: "HDR scene (reload)",
        cvar: "r_sceneHdr",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "HDR exposure",
        cvar: "r_hdrExposure",
        kind: ValueKind::Float {
            min: 0.25,
            max: 4.0,
            step: 0.05,
        },
    },
    Setting {
        label: "Eye adaptation",
        cvar: crate::frame_target::aa::exposure::ENABLED,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Adaptation: max darken, EV (HDR)",
        cvar: crate::frame_target::aa::exposure::MIN_EV,
        kind: ValueKind::Float {
            min: -2.0,
            max: 0.0,
            step: 0.25,
        },
    },
    Setting {
        label: "Adaptation: max brighten, EV",
        cvar: crate::frame_target::aa::exposure::MAX_EV,
        kind: ValueKind::Float {
            min: 0.0,
            max: 2.0,
            step: 0.25,
        },
    },
    Setting {
        label: "Filmic tone curve",
        cvar: "r_toneCurve",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Bloom",
        cvar: "r_sceneBloom",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Dynamic glow",
        cvar: "r_DynamicGlow",
        kind: ValueKind::Choice(&["0", "1", "2"]),
    },
    Setting {
        label: "Glow style",
        cvar: "r_dynamicGlowStyle",
        kind: ValueKind::Choice(&["0", "1"]),
    },
    Setting {
        label: "FXAA (reload)",
        cvar: "r_fxaa",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Supersampling, 1 off (reload)",
        cvar: "r_superSample",
        kind: ValueKind::Integer {
            min: 1,
            max: 3,
            step: 1,
        },
    },
    Setting {
        label: "Soft particles",
        cvar: "r_softParticles",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Ambient occlusion",
        cvar: "r_ssao",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Sunbeam dust (0 off)",
        cvar: crate::dust_motes::CVAR,
        kind: ValueKind::Float {
            min: 0.0,
            max: 1.0,
            step: 0.1,
        },
    },
    Setting {
        label: "Per-pixel model lighting",
        cvar: "r_modelPixelLight",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Reflection probes (reload)",
        cvar: "r_cubeMapping",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Floor mirrors",
        cvar: "r_floorReflections",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Normal maps (reload)",
        cvar: "r_normalMapping",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Specular maps (reload)",
        cvar: "r_specularMapping",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Parallax mapping (reload)",
        cvar: "r_parallaxMapping",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Parallax depth (0 flat)",
        cvar: "r_parallaxStrength",
        kind: ValueKind::Float {
            min: 0.0,
            max: 1.0,
            step: 0.05,
        },
    },
    Setting {
        label: "Emission maps (reload)",
        cvar: "r_emissiveMaps",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Emission strength (0 off)",
        cvar: "r_emissionStrength",
        kind: ValueKind::Float {
            min: 0.0,
            max: 4.0,
            step: 0.25,
        },
    },
    Setting {
        label: "Emission glow halo",
        cvar: "r_emissiveGlow",
        kind: ValueKind::Bool,
    },
];

pub(super) const RENDER_LIGHTING: &[Setting] = &[
    Setting {
        label: "Sun and sky (reload)",
        cvar: "r_dayNight",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Live lighting 0-2 (next map)",
        cvar: "r_liveLighting",
        kind: ValueKind::Integer {
            min: 0,
            max: 2,
            step: 1,
        },
    },
    Setting {
        label: "Time of day (hour)",
        cvar: "r_dayHour",
        kind: ValueKind::Float {
            min: 0.0,
            max: 24.0,
            step: 0.5,
        },
    },
    Setting {
        label: "Day length, min (0 holds)",
        cvar: "r_dayMinutes",
        kind: ValueKind::Float {
            min: 0.0,
            max: 1440.0,
            step: 5.0,
        },
    },
    Setting {
        label: "Sunlight brightness",
        cvar: "r_dayBrightness",
        kind: ValueKind::Float {
            min: 0.1,
            max: 10.0,
            step: 0.1,
        },
    },
    Setting {
        label: "Ambient fill",
        cvar: "r_ambientFill",
        kind: ValueKind::Float {
            min: 0.0,
            max: 0.2,
            step: 0.005,
        },
    },
    Setting {
        label: "Ambient fill corner shading",
        cvar: "r_ambientFillOcclusion",
        kind: ValueKind::Float {
            min: 0.0,
            max: 1.0,
            step: 0.05,
        },
    },
    Setting {
        label: "Indirect light boost",
        cvar: "r_indirectBoost",
        kind: ValueKind::Float {
            min: 0.0,
            max: 4.0,
            step: 0.1,
        },
    },
    Setting {
        label: "Emission-map lights (next map)",
        cvar: "r_emissiveLights",
        kind: ValueKind::Float {
            min: 0.0,
            max: 4.0,
            step: 0.25,
        },
    },
    Setting {
        label: "Light shafts (reload)",
        cvar: "r_volumetrics",
        kind: ValueKind::Choice(&["0", "1", "2", "3"]),
    },
    Setting {
        label: "Light shaft clarity",
        cvar: "r_volumetricClarity",
        kind: ValueKind::Float {
            min: 0.0,
            max: 1.0,
            step: 0.05,
        },
    },
];

pub(super) const RENDER_SHADOWS: &[Setting] = &[
    Setting {
        label: "World sun shadows (reload)",
        cvar: "r_worldSunShadows",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Character sun shadows (reload)",
        cvar: "r_actorSunShadows",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Shadow resolution (reload)",
        cvar: "r_sunShadowResolution",
        kind: ValueKind::Integer {
            min: 512,
            max: 4096,
            step: 512,
        },
    },
    Setting {
        label: "Sharp shadow distance (reload)",
        cvar: "r_sunShadowDistance",
        kind: ValueKind::Integer {
            min: 128,
            max: 4096,
            step: 128,
        },
    },
    Setting {
        label: "Close shadow distance (reload)",
        cvar: "r_sunShadowNear",
        kind: ValueKind::Integer {
            min: 64,
            max: 1024,
            step: 64,
        },
    },
    Setting {
        label: "Shadow filter taps (reload)",
        cvar: "r_sunShadowTaps",
        kind: ValueKind::Integer {
            min: 4,
            max: 32,
            step: 4,
        },
    },
    Setting {
        label: "Close shadow slits (units)",
        cvar: "r_sunShadowGapClose",
        kind: ValueKind::Float {
            min: 0.0,
            max: 64.0,
            step: 1.0,
        },
    },
    Setting {
        label: "Contact shadows",
        cvar: "r_contactShadows",
        kind: ValueKind::Bool,
    },
];

/// The weather: rain, snow and fog (`weather.rs`) and the clouds.
pub(super) const RENDER_WEATHER: &[Setting] = &[
    Setting {
        label: "Weather (rain, snow, mist)",
        cvar: crate::weather::CVAR,
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Weather density (1 original)",
        cvar: crate::weather::DENSITY_CVAR,
        kind: ValueKind::Float {
            min: 0.25,
            max: 4.0,
            step: 0.25,
        },
    },
    Setting {
        label: "Weather quality",
        cvar: crate::weather::QUALITY_CVAR,
        kind: ValueKind::Choice(&["0", "1", "2", "3"]),
    },
    Setting {
        label: "Force weather",
        cvar: crate::weather::FORCE_CVAR,
        kind: ValueKind::Choice(&["0", "1", "2", "3", "4"]),
    },
    Setting {
        label: "Ground fog",
        cvar: crate::weather::FOG_CVAR,
        kind: ValueKind::Choice(&["0", "1", "2"]),
    },
    Setting {
        label: "Volumetric clouds",
        cvar: crate::weather::CLOUDS_CVAR,
        kind: ValueKind::Bool,
    },
];
