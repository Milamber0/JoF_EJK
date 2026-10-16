//! What each setting does, in the player's words, for the classic+ option
//! panels' detail box (see `docs/classic-plus.md`), as retail's `descText`
//! said what each `setup.menu` item did. Keyed by cvar; every row of the
//! catalogue has one. When a setting takes effect comes from its label's
//! "(restart)", "(reload)" or "(next map)", which the classic+ rows show as a mark.

/// Lines the detail box gives a description, and characters per line.
pub(super) const HELP_LINES: usize = 2;
pub(super) const HELP_LINE_CHARS: usize = 68;

/// When a changed setting takes effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Timing {
    Now,
    Restart,
    /// After a graphics reload (`graphics_reload.rs`), which the reload card offers.
    Reload,
    NextMap,
}

impl Timing {
    /// The detail box's words for it.
    pub(super) fn note(self) -> Option<&'static str> {
        match self {
            Self::Now => None,
            Self::Restart => Some("applies after a restart"),
            Self::Reload => Some("applies after a graphics reload"),
            Self::NextMap => Some("applies on the next map"),
        }
    }
}

/// `label` without its timing note, and the timing it gave.
pub(super) fn timing(label: &str) -> (&str, Timing) {
    for (suffix, timing) in [
        (" (restart)", Timing::Restart),
        (" (reload)", Timing::Reload),
        (" (next map)", Timing::NextMap),
    ] {
        if let Some(stem) = label.strip_suffix(suffix) {
            return (stem, timing);
        }
    }
    (label, Timing::Now)
}

/// What a classic+ row calls a setting: its label without the timing note
/// and without a trailing note in brackets ("Dynamic glow (0 off, 2 sabers)"
/// is "Dynamic glow"), which the retail label column has no room for; the
/// detail box shows the whole label and explains the values.
pub(super) fn row_label(label: &str) -> (&str, Timing) {
    let (label, timing) = timing(label);
    let short = match label.find(" (") {
        Some(at) if label.ends_with(')') && at > 0 => &label[..at],
        _ => label,
    };
    (short, timing)
}

/// Classic row names of settings whose label is longer than the label
/// column; the detail box still shows the whole label.
const SHORT_LABELS: &[(&str, &str)] = &[
    ("cg_drawFps", "FPS readout"),
    ("cg_forceModel", "Everyone as my model"),
    ("cg_thirdPersonCameraDamp", "Camera damping"),
    ("cg_thirdPersonTargetDamp", "Target damping"),
    ("cg_errorDecay", "Error smoothing"),
    (crate::ghost_replay::SHOWCASE_CVAR, "Experimental showcase"),
    (crate::frame_target::aa::exposure::MIN_EV, "Max darken, EV"),
    (
        crate::frame_target::aa::exposure::MAX_EV,
        "Max brighten, EV",
    ),
    ("r_modelPixelLight", "Pixel model lighting"),
    ("r_ambientFillOcclusion", "Ambient fill shading"),
];

/// What a classic+ row calls setting `cvar` labelled `label`: its short name
/// if it has one, else [`row_label`].
pub(super) fn classic_label<'a>(cvar: &str, label: &'a str) -> (&'a str, Timing) {
    let (row, timing) = row_label(label);
    let short = SHORT_LABELS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(cvar))
        .map_or(row, |(_, short)| *short);
    (short, timing)
}

/// What a choice row's value is called where the value itself is a number or
/// a code: each `(cvar, value, name)`. A value not listed is shown as it is
/// written ([`super::classic_view::choice_label`]).
const CHOICE_NAMES: &[(&str, &str, &str)] = &[
    (
        crate::graphics_quality::RENDERER_ROW,
        crate::graphics_quality::VANILLA,
        "Vanilla",
    ),
    (
        crate::graphics_quality::RENDERER_ROW,
        crate::graphics_quality::SJK,
        "SJK (alpha)",
    ),
    (crate::text::charset::CVAR, "auto", "Auto"),
    (crate::text::charset::CVAR, "western", "Western"),
    (crate::text::charset::CVAR, "cyrillic", "Cyrillic"),
    (crate::input::idrive::CVAR, "0", "Off"),
    (crate::input::idrive::CVAR, "1", "All keys"),
    (crate::input::idrive::CVAR, "2", "Jump, crouch"),
    (crate::effect_aux::saber_rain_steam::CVAR, "0", "Off"),
    (crate::effect_aux::saber_rain_steam::CVAR, "1", "Everyone"),
    (crate::effect_aux::saber_rain_steam::CVAR, "2", "Only mine"),
    ("cg_remaps", "0", "Never"),
    ("cg_remaps", "1", "Not players"),
    ("cg_remaps", "2", "Always"),
    (crate::brush_view::CVAR, "0", "Off"),
    (crate::brush_view::CVAR, "1", "Gameplay"),
    (crate::brush_view::CVAR, "2", "All"),
    (crate::entity_view::CVAR, "0", "Off"),
    (crate::entity_view::CVAR, "1", "Common"),
    (crate::entity_view::CVAR, "2", "All"),
    ("r_DynamicGlow", "0", "Off"),
    ("r_DynamicGlow", "1", "On"),
    ("r_DynamicGlow", "2", "Sabers"),
    ("r_dynamicGlowStyle", "0", "Original"),
    ("r_dynamicGlowStyle", "1", "Vulkan"),
    ("r_volumetrics", "0", "Off"),
    ("r_volumetrics", "1", "Low"),
    ("r_volumetrics", "2", "Medium"),
    ("r_volumetrics", "3", "High"),
    (crate::weather::QUALITY_CVAR, "0", "Low"),
    (crate::weather::QUALITY_CVAR, "1", "Medium"),
    (crate::weather::QUALITY_CVAR, "2", "High"),
    (crate::weather::QUALITY_CVAR, "3", "Ultra"),
    (crate::weather::FORCE_CVAR, "0", "The map's"),
    (crate::weather::FORCE_CVAR, "1", "Drizzle"),
    (crate::weather::FORCE_CVAR, "2", "Rain"),
    (crate::weather::FORCE_CVAR, "3", "Storm"),
    (crate::weather::FORCE_CVAR, "4", "Snow"),
    (crate::weather::FOG_CVAR, "0", "Never"),
    (crate::weather::FOG_CVAR, "1", "Map's fog"),
    (crate::weather::FOG_CVAR, "2", "Always"),
];

/// What choice `value` of setting `cvar` is called, when it has a name of its own.
pub(super) fn choice_name(cvar: &str, value: &str) -> Option<&'static str> {
    CHOICE_NAMES
        .iter()
        .find(|(name, choice, _)| {
            name.eq_ignore_ascii_case(cvar) && choice.eq_ignore_ascii_case(value.trim())
        })
        .map(|(_, _, shown)| *shown)
}

/// What the detail column says, in gold, while the SJK renderer is on and a
/// renderer setting is focused ([`super::renderer_row`]).
pub(super) const ALPHA_WARNING: &str = "The SJK renderer is in alpha: some maps can show glitches \
    (lighting, shadows, missing surfaces) or run slower. Report them from the SJK menu's Report a \
    bug; Vanilla is the safe choice.";

/// The description of setting `cvar`.
pub(super) fn help(cvar: &str) -> Option<&'static str> {
    HELP.iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(cvar))
        .map(|(_, text)| *text)
}

/// `text` cut into at most [`HELP_LINES`] lines of [`HELP_LINE_CHARS`]
/// characters at spaces; the last line takes what is left.
pub(super) fn lines(text: &str) -> [&str; HELP_LINES] {
    let mut out = [""; HELP_LINES];
    let mut rest = text.trim();
    for (index, line) in out.iter_mut().enumerate() {
        if rest.chars().count() <= HELP_LINE_CHARS || index + 1 == HELP_LINES {
            *line = rest;
            break;
        }
        let limit = rest
            .char_indices()
            .nth(HELP_LINE_CHARS)
            .map_or(rest.len(), |(at, _)| at);
        let cut = rest[..limit].rfind(' ').unwrap_or(limit);
        *line = rest[..cut].trim_end();
        rest = rest[cut..].trim_start();
    }
    out
}

const HELP: &[(&str, &str)] = &[
    // VIDEO
    (
        crate::graphics_quality::RENDERER_ROW,
        "Vanilla: the original game's look and frame rate. SJK: SJK's own renderer (sun, shadows, reflections), in alpha.",
    ),
    (
        crate::graphics_quality::ROW_NAME,
        "How much the SJK renderer draws: Performance for the most FPS, Ultra for the best look. Picking one turns the SJK renderer on.",
    ),
    (
        "r_resolution",
        "The window's size, or the screen mode in exclusive fullscreen. Enter opens the list of sizes.",
    ),
    (
        "r_fullscreen",
        "Windowed, borderless (fills the desktop) or exclusive fullscreen (sets the monitor's mode).",
    ),
    (
        "r_vsync",
        "Waits for the monitor's refresh before showing a frame: no tearing, a little more input delay.",
    ),
    (
        "com_maxfps",
        "Most frames drawn a second; your movement is Gameplay FPS. AUTO is 125, or the monitor's rate with Detect refresh rate; 0 is off.",
    ),
    (
        crate::server_caps::OVERRIDE_CVAR,
        "On, Gameplay FPS applies on servers SJK knows to cap commands (JoF's: 125) too. Inputs may stop going through.",
    ),
    (
        crate::gameplay_fps::CVAR,
        "Auto moves you at your FPS cap; 125, 142, 250 or 333 keep that movement at any FPS. Enter explains it and takes any value.",
    ),
    (
        crate::runtime_settings::MONITOR_CAP_CVAR,
        "AUTO on the FPS cap follows the monitor's refresh rate. Off, AUTO caps at 125 and the monitor is not read.",
    ),
    (
        "cg_fov",
        "How wide you see, in degrees across the screen: higher shows more at the sides.",
    ),
    (
        "cg_marks",
        "Scorch and blaster marks left on walls and floors by shots and explosions.",
    ),
    ("cg_shadows", "Players' shadows on the ground under them."),
    (
        "cg_drawGun",
        "Shows your weapon in first person; off hides it for a clear view.",
    ),
    (
        "cg_drawFps",
        "Shows frames per second and frame time in a corner of the screen.",
    ),
    (
        "r_gamma",
        "Brightness curve of the whole display, HUD included. 1 is neutral; higher lifts dark areas.",
    ),
    // AUDIO
    ("s_volume", "Loudness of every sound but the music."),
    ("s_musicVolume", "Loudness of the music."),
    (
        "s_doppler",
        "Moving looping sounds, such as passing ships, change pitch as they come and go.",
    ),
    ("cg_footsteps", "Footstep sounds, yours and other players'."),
    (
        crate::footprints::PRINTS_CVAR,
        "Steps on sand, snow, mud and dirt kick up dust and leave footprints for ten seconds, as in the original game.",
    ),
    (
        crate::footprints::JUMP_CVAR,
        "A jump and a forward landing step with both feet: the ground's sound, dust and prints. Not in the original game.",
    ),
    (
        crate::footprints::GUESS_CVAR,
        "Untagged ground (much original sand and snow) sounds and marks as its texture name says. Off: stone, as originally.",
    ),
    (
        "snd_mute_losefocus",
        "Silences the game while its window is alt-tabbed out or minimised.",
    ),
    (
        crate::emotes::VOLUME_CVAR,
        "Loudness of emote sounds: 1 as recorded, 2 twice as loud. Yours plays in both ears; others' follow their player.",
    ),
    (
        crate::unlock_toast::SOUND_CVAR,
        "Plays the secret-area chime with each unlock card: an achievement, or a new medal or holocron in a match.",
    ),
    // HUD
    (
        "cg_drawHud",
        "Shows the HUD; off hides it, for screenshots and recordings.",
    ),
    ("cg_hudScale", "Size of the HUD: higher is larger."),
    ("cg_drawStatus", "Health, armor and Force on the HUD."),
    (
        "cg_drawWeapon",
        "The weapon bar shown while you change weapons.",
    ),
    (
        "cg_crosshair",
        "Crosshair picture: 0 hides it, 1 to 9 pick a picture, 10 is a white dot.",
    ),
    (
        "cg_drawCrosshairNames",
        "Shows the name of the player under your crosshair.",
    ),
    (
        "cg_drawPlayerNames",
        "Plain names above players: 0 off, 1 names, 2 with a health strip. Nameplates replace it.",
    ),
    (
        "cg_drawPlayerNamesScale",
        "Size of the plain names above players.",
    ),
    (
        "cg_playerCard",
        "Look at a player without moving and a card shows their model, sabers and SJK profile.",
    ),
    (
        "cg_playerCardDelay",
        "Seconds you must keep looking at a player, steady, before the card shows.",
    ),
    (
        "cg_playerCardOutline",
        "Outline the player whose card is up, in the card's colour.",
    ),
    (
        crate::hud::team_outline::CVAR,
        "In team games, a red outline around enemies and a green one around teammates, only where you can see them.",
    ),
    (
        "cg_drawFriend",
        "Your team's logo over teammates' heads (everyone's while spectating), your partner's in power duel.",
    ),
    (
        "cg_nameplate",
        "MMO-style nameplates: a name far away, with bars and a frame up close.",
    ),
    (
        "cg_nameplateRange",
        "How far away, in game units, a nameplate still shows. It fades near the end.",
    ),
    (
        "cg_nameplateNear",
        "Inside this distance the bars fade in under the name; beyond it just the name.",
    ),
    ("cg_nameplateScale", "Size of the nameplate text."),
    (
        "cg_nameplateBars",
        "Bars under the name: 0 none, 1 allies, 2 everyone, 3 your target and duel opponent. V cycles modes.",
    ),
    (
        "cg_nameplateForce",
        "Force bar under the name, estimated from the powers the player uses. The grey haze shows how unsure the estimate is.",
    ),
    (
        "cg_nameplateShare",
        "Your real health, shield and Force for the other SJK players, through the SJK hub. Always on while your nameplates show bars.",
    ),
    (
        "cg_nameplateEstimate",
        "Estimated bars on players who don't share, while the hub has yours. Your player card tells others you see theirs.",
    ),
    (
        "cg_nameplateSelf",
        "Your own nameplate over your head in third person, with your real bars.",
    ),
    (
        "cg_nameplateWeapon",
        "The weapon a player holds, left of the nameplate. A saber is ringed in its stance's colour.",
    ),
    (
        "cg_nameplateIcons",
        "Icons of the Force powers a player has on, over the name when close.",
    ),
    (
        "cg_nameplateWalls",
        "Shows nameplates of players behind walls, dimmed. Off hides them.",
    ),
    (
        "cg_nameplateNpcs",
        "Nameplates on NPCs with their class and health, such as Stormtrooper.",
    ),
    (
        crate::world_ping::SHOW_CVAR,
        "Shows other SJK players' pings (+ping, middle mouse): your team's in team games, everyone's otherwise. Off hides them.",
    ),
    (
        crate::world_ping::SOUND_CVAR,
        "A short sound when a world ping appears, yours or another player's.",
    ),
    ("cg_drawTimer", "Shows the match time."),
    (
        "cg_drawTimerUpperRight",
        "Puts the match time in the upper right under the frame rate, as JoF EJK does, instead of in the HUD layout.",
    ),
    (
        crate::version_overlay::CVAR,
        "Shows SJK's version, build date and commit at the top, so screenshots say which build.",
    ),
    (
        "cg_lagometer",
        "A small graph of interpolation and snapshot delay, for spotting network trouble.",
    ),
    (
        "cg_lagometerScale",
        "Size of the lagometer: 1 normal, 2 double. Keeps its bottom-right corner fixed.",
    ),
    (
        "cg_killfeed",
        "The last kills at the top right: killer, weapon icon, victim; a skull for suicides and falls.",
    ),
    ("cg_drawChat", "Shows chat messages over the game."),
    (
        crate::chat::emoji::CVAR,
        "Shows emoji pictures in place of names like :poop: in new chat messages.",
    ),
    (
        crate::chat::reply::REMEMBER_CVAR,
        "The chat key opens on your last channel: All, Team, SJK or a whisper. Off, it opens on All.",
    ),
    (
        "cg_chatBoxLetterSpacing",
        "Adds space between the letters of chat messages; below zero packs them closer.",
    ),
    (
        crate::menu_hud::STYLE_CVAR,
        "Which HUD shows health, armor, Force and ammo: the game's, an installed HUD pack's, or SJK's.",
    ),
    (
        crate::menu_hud::FILES_CVAR,
        "The game HUD's menu list (ui/jahud.txt); 1 is the text-only HUD, 3 and 4 EternalJK's HUDs.",
    ),
    (
        "cg_classicHudFont",
        "Draws the HUD's numbers and text in the original game's HUD font.",
    ),
    (
        crate::game_font::CVAR,
        "Draws the menus and console in the original game's fonts instead of SJK's.",
    ),
    (
        crate::ground_hud::CVAR,
        "In third person, shows health, armor and Force on the ground around your character.",
    ),
    // CONTROLS
    (
        "sensitivity",
        "How far the view turns for a given mouse movement.",
    ),
    (
        "m_invert",
        "Moving the mouse forward looks down instead of up.",
    ),
    ("cl_run", "Run by default; the Walk key walks."),
    (
        crate::input::idrive::CVAR,
        "Opposite movement keys move toward the one pressed last instead of cancelling: all of them, or jump and crouch only.",
    ),
    (
        crate::input::idrive::DELAY_CVAR,
        "Stand still this long after pressing the opposite key before it takes over; some servers punish instant flips.",
    ),
    (
        crate::input::force_repeat::CVAR,
        "Keep a push or pull key held to use the power again the moment its cooldown ends, as in JoF EJK.",
    ),
    // GAME
    (
        "cg_simpleItems",
        "Shows pickups as flat icons instead of 3D models, easier to spot from afar.",
    ),
    (
        "cg_forceModel",
        "Draws every player with your own model and skin.",
    ),
    (
        "cg_saberTrail",
        "The light trail behind a swinging saber blade.",
    ),
    (
        crate::effect_aux::saber_rain_steam::CVAR,
        "Steam and sizzle in rain on exposed blades: everyone's, or only your own.",
    ),
    (
        "cg_speedTrail",
        "Afterimages behind players using Force Speed.",
    ),
    (
        "cg_auraShell",
        "The glowing shell Force Sight shows around other players.",
    ),
    (
        "cg_spProtAbsColor",
        "Protect and Absorb at once show one cyan shell like single player, not green plus blue.",
    ),
    (
        "cg_shieldSphere",
        "Off: a shield hit flashes on the body, like single player. On: multiplayer's sphere around the player.",
    ),
    (
        "cg_hitMarker",
        "Red direction indicator when you take damage. Off by default; camera kick and sounds stay the same.",
    ),
    (
        "cg_shieldBrightness",
        "How bright the shield flash on the body is: 1 is the stock look, 2 the default, 12 the strongest. Not used by the sphere.",
    ),
    (
        "cg_remaps",
        "Lets the server swap textures: never, all but player models, or always. Maps' own always apply.",
    ),
    (
        crate::camera::STYLE_CVAR,
        "EJK: the third-person camera stays locked behind you, as in JoF EJK. SJK: it trails you a little as you move and turn.",
    ),
    (
        "cg_thirdPersonCameraDamp",
        "How loosely the third-person camera follows you: lower is smoother, 1 sticks to you.",
    ),
    (
        "cg_thirdPersonTargetDamp",
        "How loosely the third-person camera turns where you aim: lower is smoother, 1 is instant.",
    ),
    (
        "cg_errorDecay",
        "Smooths the view over this many milliseconds when the server corrects you; 0 snaps.",
    ),
    (
        "mod_japlus",
        "On JA+ servers: the japlus. commands (admin teleports, auto login, plugin options) and JA+ command completion.",
    ),
    (
        "mod_jof",
        "On JoF servers: jof.commands and completion of their own commands (gun admin commands, jetpack, emotes).",
    ),
    (
        crate::ghost_replay::SHOWCASE_CVAR,
        "Experimental, for filming: ghost replays a run you recorded, mimic copies a player facing you. Any key stops them.",
    ),
    (
        crate::brush_view::CVAR,
        "Shows invisible brushes (clip, trigger...) and movers (doors, lifts) see-through, as Radiant does: those of gameplay, or all.",
    ),
    (
        crate::entity_view::CVAR,
        "Marks the map's entities (lights, spawns, origins, items...) through walls with their names. A world note can pick them.",
    ),
    (
        crate::rarity_fx::CVAR,
        "Animates rarity: Rare frames breathe, Legendary ones catch a sheen, Mythical ones a running spark. Off keeps the tier frames still.",
    ),
    (
        crate::menu::style::CVAR,
        "The SJK UI, SJK's own menus over the live map; or classic menus after the original game's.",
    ),
    (
        crate::quick_wheel::pages::FILE,
        "The quick wheel's pages (+wheel) and the choices on each, kept in wheel.json. Enter edits them.",
    ),
    (
        crate::quick_wheel::SOUNDS_CVAR,
        "The game's menu sounds as the quick wheel changes page, moves to another choice and runs one.",
    ),
    // NETWORK
    (
        "cl_master",
        "The master server the server browser asks for its list of servers.",
    ),
    (
        "cl_master2",
        "A second master server, asked at the same time; servers either one knows are listed. Empty asks only the first.",
    ),
    (
        "cl_autoUpdate",
        "Looks for a newer SJK release at start and every 30 minutes while SJK runs, and says so with a card. Install it from the Update page.",
    ),
    (
        "ui_hideFirstSetup",
        "Ticked, First setup no longer opens when SJK starts. Settings and the firstsetup command still open it.",
    ),
    (
        "cl_identity",
        "Makes an identity key and tells the SJK hub where you play and if you are active (badge, holocrons). Off sends nothing.",
    ),
    (
        crate::installed_models::CVAR,
        "Tells the SJK hub which player models you have, so SJK players' cards say if they see yours. Off makes it forget.",
    ),
    (
        "cl_hubUrl",
        "The SJK hub's address (https://...). Empty means no hub: nothing is sent.",
    ),
    (
        crate::crash_report::CVAR,
        "After a crash, the next start sends the SJK team a report: system, map, the error, recent console lines (no chat). Off sends none.",
    ),
    (
        crate::settings_usage::CVAR,
        "Tells the SJK team anonymously what you change and search for here, to make Settings clearer. No key or name sent.",
    ),
    (
        "cl_sjkChat",
        "The chat every SJK player shares through the SJK hub, in menus and games. Off hides it.",
    ),
    (
        crate::sjk_chat_frame::SOUND_CVAR,
        "A sound for each new SJK chat message, yours included, in menus and games. Chat sounds off (cg_chatSounds 0) silences it too.",
    ),
    (
        crate::chat_gifs::CVAR,
        "GIPHY links in SJK chat show their GIF. Your PC loads it from GIPHY, which sees your address. Off keeps links as text.",
    ),
    (
        crate::chat_gifs::search::KEY_CVAR,
        "Your own GIPHY key, which GIF search needs: free at developers.giphy.com. The GIF picker's Search shows how.",
    ),
    (
        super::catalog::IDENTITY_ROW,
        "Your SJK key: switch it on, back it up, copy its id. Enter opens it; the key stays hidden until Show.",
    ),
    (
        super::catalog::IMPORT_ROW,
        "Bring name, model, FOV, FPS and keys from Base, EJK, TaystJK or any .cfg. Enter shows the configs found.",
    ),
    (
        "assetbrowser",
        "Enable or disable installed PK3 packs for the next client start. Retail base packs stay enabled; files stay in place.",
    ),
    (
        "rate",
        "Most data per second the server may send you; raise it on a fast connection.",
    ),
    (
        "snaps",
        "World updates per second asked of the server; it sends at most its own rate.",
    ),
    // HUD+
    ("cg_crosshairSize", "Size of the crosshair."),
    ("cg_speedometer", "Shows how fast you move."),
    (
        crate::hud::movement::LINES_ROW,
        "JoF EJK's strafe helper: a line to the best heading for each strafe key. Keep the crosshair on the green one to gain speed.",
    ),
    (
        crate::scoreboard::style::CVAR,
        "Auto matches the menus: SJK's own with the SJK UI, else the classic one after the original game's.",
    ),
    (
        crate::scoreboard::style::COMPACT_CVAR,
        "Thinner rows on SJK's scoreboard, so every player fits in one column, in a board as wide as its names, centred.",
    ),
    (
        crate::scoreboard::style::FPS_CVAR,
        "Shows your frame rate on SJK's scoreboard, left of the match clock.",
    ),
    (
        "cg_showClientIDs",
        "Shows each player's client number on the scoreboard.",
    ),
    (
        crate::jof_tag::CVAR,
        "Shows a clan's emblem (JoF, JoFA) left of a tagged player's name in the chat, on the scoreboard and on player cards.",
    ),
    (
        "cg_drawScoreboardIcons",
        "Shows each player's head icon on the scoreboard.",
    ),
    (
        "cg_smallScoreboard",
        "Smaller scoreboard rows, so more players fit.",
    ),
    // TEXT
    (
        crate::console::console_options::STYLE_CVAR,
        "The console's look: the SJK UI's with its menus, or classic, after EternalJK's.",
    ),
    (crate::text::style::SCALE_CVAR, "Size of the menu text."),
    (
        crate::text::style::TRACKING_CVAR,
        "Extra space between letters in the menus and the console.",
    ),
    ("con_scale", "Size of the console text."),
    (
        crate::console::console_options::DRAW_NOTIFY_CVAR,
        "Shows the newest console lines at the top left while playing. Chat keeps its own box.",
    ),
    (
        crate::text::charset::CVAR,
        "Chat and names of EternalJK and retail players: Auto spots Russian, Cyrillic always reads it as Russian, Western never.",
    ),
    // IMAGE
    (
        "r_sceneHdr",
        "Renders the scene in high dynamic range: brighter lights, smoother bright areas.",
    ),
    (
        "r_hdrExposure",
        "How bright the scene is drawn; eye adaptation works around this value.",
    ),
    (
        crate::frame_target::aa::exposure::ENABLED,
        "Brightness follows what you look at, as eyes adapt between dark rooms and bright sky.",
    ),
    (
        crate::frame_target::aa::exposure::MIN_EV,
        "How far eye adaptation may darken a bright view, in exposure steps (HDR only).",
    ),
    (
        crate::frame_target::aa::exposure::MAX_EV,
        "How far eye adaptation may brighten a dark view, in exposure steps.",
    ),
    (
        "r_toneCurve",
        "A filmic curve that softens the brightest parts of the picture.",
    ),
    (
        "r_sceneBloom",
        "Bright lights bleed a soft glow around them. The HUD and menus never bloom.",
    ),
    (
        "r_DynamicGlow",
        "The glow around sabers and glowing surfaces: on everything that glows, or on sabers only.",
    ),
    (
        "r_dynamicGlowStyle",
        "How the glow is blurred: like the original renderer, or like EternalJK's Vulkan one.",
    ),
    ("r_fxaa", "Smooths jagged edges with a light filter."),
    (
        "r_superSample",
        "Draws the scene 2 or 3 times larger each way and shrinks it: very sharp, very costly.",
    ),
    (
        "r_softParticles",
        "Smoke and dust fade where they meet walls instead of cutting into them.",
    ),
    (
        "r_ssao",
        "Darkens corners and creases of the map's walls and floors a little.",
    ),
    (
        crate::dust_motes::CVAR,
        "Dust drifting in sunbeams; needs light shafts. 0 is off.",
    ),
    (
        crate::weather::CVAR,
        "The rain, snow and mist a map asks for. It stays under open sky: roofs keep it out.",
    ),
    (
        crate::weather::DENSITY_CVAR,
        "How many weather particles: 1 as many as the original game, 2 twice as many.",
    ),
    (
        crate::weather::QUALITY_CVAR,
        "Medium adds splashes, 3D fog and wet ground; High distant rain and water down walls; \
         Ultra puddles, finer fog and clouds.",
    ),
    (
        crate::weather::FORCE_CVAR,
        "Weather on every map with sky: the map's own, or drizzle, rain, a storm or snow everywhere.",
    ),
    (
        crate::weather::FOG_CVAR,
        "Fog lying on the ground outdoors: never, where the map has fog, or on every map \
         with sky. Works with rain and forced weather.",
    ),
    (
        crate::weather::CLOUDS_CVAR,
        "Drifting 3D clouds over open sky, lit by the map's sun; darker and thicker in a storm.",
    ),
    (
        "r_modelPixelLight",
        "Lights models at every pixel rather than once each, so they match the light around them.",
    ),
    (
        "r_cubeMapping",
        "Captures each map's surroundings so metal and shiny surfaces reflect their room.",
    ),
    (
        "r_floorReflections",
        "Mirror-like reflections on polished floors.",
    ),
    (
        "r_normalMapping",
        "Surface relief from a material pack's normal maps: bricks and stone catch the light.",
    ),
    (
        "r_specularMapping",
        "Shine from a material pack's specular maps: metal and wet stone glint.",
    ),
    (
        "r_parallaxMapping",
        "Gives stone, tiles and sand real-looking depth from their height maps.",
    ),
    (
        "r_parallaxStrength",
        "How deep parallax looks: 0 flat, 1 the full depth of the maps; 0.1 by default.",
    ),
    (
        "r_emissiveMaps",
        "Emission maps, which make lamps, screens and signs glow on their own.",
    ),
    (
        "r_emissionStrength",
        "Brightness of emission maps: 1 as made, 0 off.",
    ),
    (
        "r_emissiveGlow",
        "A glow halo around emission-mapped surfaces; needs dynamic glow.",
    ),
    // LIGHTING
    (
        "r_dayNight",
        "Lights maps with SJK's sun and sky as well as their own baked light.",
    ),
    (
        "r_liveLighting",
        "How much light is computed live: 0 keeps the map's baked bounce light, 2 computes it all.",
    ),
    (
        "r_dayHour",
        "The hour of the sun, 0 to 24, when the sun and sky are on.",
    ),
    (
        "r_dayMinutes",
        "Real minutes a whole day lasts; 0 holds the hour still.",
    ),
    (
        "r_dayBrightness",
        "Brightness of the live lighting, relative to the sun.",
    ),
    (
        "r_ambientFill",
        "A faint light in very dark places so they stay readable; 0 is off.",
    ),
    (
        "r_ambientFillOcclusion",
        "How much that fill darkens in corners: 1 as designed, 0 not at all.",
    ),
    (
        "r_indirectBoost",
        "Brightness of bounced and sky light; direct light stays as it is. 1 as designed.",
    ),
    (
        crate::world_materials::material_maps::lights::CVAR,
        "Lets emission-mapped surfaces light their surroundings in live lighting; 0 is off.",
    ),
    (
        "r_volumetrics",
        "Light shafts through the air; each level up is finer and costs more.",
    ),
    (
        "r_volumetricClarity",
        "1 keeps sunlit air clear and shows only the shafts; 0 adds an even haze.",
    ),
    // SHADOWS
    (
        "r_worldSunShadows",
        "The sun casts shadows from walls and buildings.",
    ),
    (
        "r_actorSunShadows",
        "Players and NPCs cast sun shadows too.",
    ),
    (
        "r_sunShadowResolution",
        "Detail of the sun's shadows: higher is sharper and takes more video memory.",
    ),
    (
        "r_sunShadowDistance",
        "How far from you sun shadows stay sharp; a coarser map covers the rest of the level.",
    ),
    (
        "r_sunShadowNear",
        "Range of the finest shadows, close around you.",
    ),
    (
        "r_sunShadowTaps",
        "Samples taken along a shadow's edge: more is smoother and more costly.",
    ),
    (
        "r_sunShadowGapClose",
        "Fills light leaking through slits narrower than this many units; 0 is off.",
    ),
    (
        "r_contactShadows",
        "Small shadows where things touch the ground, toward the sun. Still being tuned.",
    ),
];

#[cfg(test)]
mod tests {
    use super::super::catalog::*;
    use super::*;

    fn every_row() -> impl Iterator<Item = &'static Setting> {
        let general = (0..TABS.len()).flat_map(super::super::settings);
        let renderer = [
            RENDER_IMAGE,
            RENDER_LIGHTING,
            RENDER_SHADOWS,
            RENDER_WEATHER,
        ]
        .into_iter()
        .flatten();
        general.chain(renderer)
    }

    #[test]
    fn every_setting_says_what_it_does_in_two_lines() {
        for setting in every_row() {
            let text =
                help(setting.cvar).unwrap_or_else(|| panic!("{} has no description", setting.cvar));
            let lines = lines(text);
            assert!(
                lines
                    .iter()
                    .all(|line| line.chars().count() <= HELP_LINE_CHARS),
                "{}: {lines:?}",
                setting.cvar
            );
            assert!(
                text.is_ascii(),
                "{}: the menu font draws bytes",
                setting.cvar
            );
        }
        // And nothing describes a setting no row shows.
        for (cvar, _) in HELP {
            assert!(
                every_row().any(|setting| setting.cvar.eq_ignore_ascii_case(cvar)),
                "{cvar} is described but not on any page"
            );
        }
    }

    /// Longest row label, in characters, that fits the in-game pop-up's
    /// label column.
    const ROW_LABEL_CHARS: usize = 23;

    #[test]
    fn every_row_label_fits_the_label_column() {
        for setting in every_row() {
            let (label, _) = classic_label(setting.cvar, setting.label);
            assert!(
                label.chars().count() <= ROW_LABEL_CHARS,
                "{}: {label}",
                setting.cvar
            );
        }
        for (cvar, _) in SHORT_LABELS {
            assert!(
                every_row().any(|setting| setting.cvar.eq_ignore_ascii_case(cvar)),
                "{cvar} has a short label but no row"
            );
        }
        // A short name keeps the timing of the full label.
        assert_eq!(
            classic_label("cg_errorDecay", "Prediction error smoothing (restart)"),
            ("Error smoothing", Timing::Restart)
        );
    }

    #[test]
    fn lines_break_at_spaces() {
        let text = "Brightness curve of the whole display, HUD included. 1 is neutral; higher lifts dark areas.";
        let [first, second] = lines(text);
        assert!(first.len() <= HELP_LINE_CHARS && !first.ends_with(' '));
        assert_eq!(format!("{first} {second}"), text);
        assert_eq!(lines("Short."), ["Short.", ""]);
    }

    #[test]
    fn labels_lose_their_timing_note() {
        assert_eq!(timing("HDR scene (reload)"), ("HDR scene", Timing::Reload));
        assert_eq!(
            timing("Live lighting 0-2 (next map)"),
            ("Live lighting 0-2", Timing::NextMap)
        );
        assert_eq!(timing("Bloom"), ("Bloom", Timing::Now));
        assert_eq!(
            timing("Dynamic glow (0 off, 2 sabers)"),
            ("Dynamic glow (0 off, 2 sabers)", Timing::Now)
        );
        // Rows also drop a note in brackets.
        assert_eq!(
            row_label("Dynamic glow (0 off, 2 sabers)"),
            ("Dynamic glow", Timing::Now)
        );
        assert_eq!(
            row_label("Light shafts (reload)"),
            ("Light shafts", Timing::Reload)
        );
        assert_eq!(
            row_label("Adaptation: max darken, EV (HDR)"),
            ("Adaptation: max darken, EV", Timing::Now)
        );
        assert_eq!(row_label("FPS cap (AUTO, 0 = off)").0, "FPS cap");
        assert_eq!(
            row_label("Supersampling, 1 off (reload)").0,
            "Supersampling, 1 off"
        );
    }
}
