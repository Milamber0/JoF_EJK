//! Daily-use cvar definitions kept separate from console interaction logic.

use sjk_shell::{CvarDefinition, CvarFlags, CvarRegistry};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// `cl_maxpackets` default: one packet per user command (125 a second), as
/// JoF EJK. Stock's 30 and EternalJK's 63 batch two to four commands a packet.
pub(crate) const DEFAULT_MAX_PACKETS: i64 = 125;

/// `rate` default: 90000 bytes a second, so the 40 snapshots of a busy server are
/// not delayed by the client's own rate; servers cap it with `sv_maxRate`. Stock's
/// 25000 left about 600 bytes a snapshot at 40 a second.
pub(crate) const DEFAULT_RATE: i64 = 90_000;
/// The stock default every profile saved, moved once to [`DEFAULT_RATE`].
pub(crate) const OLD_DEFAULT_RATE: i64 = 25_000;
/// Marks that move as done.
pub(crate) const RATE_VERSION_CVAR: &str = "rateDefaultVersion";

/// Change-callback cache for integer values; frame reads never look up a name.
pub(super) struct IntegerSetting(Arc<AtomicI64>);

impl IntegerSetting {
    /// Bind before config loading so archived values apply.
    pub(super) fn bind(
        cvars: &mut CvarRegistry,
        name: &str,
        default: i64,
    ) -> Result<Self, sjk_shell::CvarError> {
        let value = Arc::new(AtomicI64::new(default));
        let changed = Arc::clone(&value);
        cvars.on_change(name, move |change| {
            if let sjk_shell::CvarValue::Integer(value) = change.current {
                changed.store(value, Ordering::Relaxed);
            }
        })?;
        Ok(Self(value))
    }

    /// Read the nonzero state without allocation or locking.
    pub(super) fn enabled(&self) -> bool {
        self.value() != 0
    }

    /// Read the integer value without allocation or locking.
    pub(super) fn value(&self) -> i64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Change counter for a cvar that frame code reads again only after it changes.
pub(super) struct RevisionSetting(Arc<AtomicU64>);

impl RevisionSetting {
    /// Bind before config loading so an archived value counts as a change.
    pub(super) fn bind(cvars: &mut CvarRegistry, name: &str) -> Result<Self, sjk_shell::CvarError> {
        let revision = Arc::new(AtomicU64::new(0));
        let changed = Arc::clone(&revision);
        cvars.on_change(name, move |_| {
            changed.fetch_add(1, Ordering::Relaxed);
        })?;
        Ok(Self(revision))
    }

    /// Read the change count without allocation or locking.
    pub(super) fn value(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

pub(super) fn register_daily_cvars(cvars: &mut CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    crate::frame_target::aa::register(cvars)?;
    crate::frame_target::scale::register(cvars)?;
    crate::world_materials::filtering::register(cvars)?;
    crate::world_materials::material_maps::register(cvars)?;
    crate::scene_views::register_floor_reflections(cvars)?;
    crate::assets::search_paths::register(cvars)?;
    super::client_options::register(cvars)?;
    super::ui_options::register(cvars)?;
    crate::hud::icons::register(cvars)?;
    crate::text::style::register(cvars)?;
    crate::text::charset::register(cvars)?;
    crate::menu_hud::register(cvars)?;
    let archive = CvarFlags::ARCHIVE;
    let definitions = [
        CvarDefinition::new(
            "cg_remaps",
            2_i64,
            archive,
            "Shader remaps: 0 off, 1 skip player textures, 2 all (default, as EternalJK)",
        ),
        CvarDefinition::new(
            crate::fps_help::CVAR,
            true,
            archive,
            "Offer Vanilla graphics when a map's SJK lighting keeps the FPS low (help_fps shows it)",
        ),
        CvarDefinition::new(
            crate::graphics_quality::RESTORE_CVAR,
            "",
            archive,
            "The SJK renderer settings put aside while the renderer is Vanilla, restored when the SJK renderer is turned on again",
        ),
        CvarDefinition::new(
            crate::remap_blocked_maps::CVAR,
            "",
            archive,
            "Maps whose server shader remaps are ignored, as with cg_remaps 0: mp/ffa4 mp/duel6",
        ),
        CvarDefinition::new(
            "cg_forceEnemyModel",
            "none",
            archive,
            "Enemy model with cg_forceModel",
        ),
        CvarDefinition::new(
            "cg_forceAllyModel",
            "none",
            archive,
            "Ally model with cg_forceModel",
        ),
        CvarDefinition::new(
            "cg_forceOwnSaber",
            "none",
            CvarFlags::NONE,
            "Local presentation hilt override(s)",
        ),
        CvarDefinition::new(
            "cg_scorePlums",
            1_i64,
            archive,
            "Server-authored floating numbers",
        ),
        CvarDefinition::new(
            "cg_killMessage",
            1_i64,
            archive,
            "Personal kill announcement mode",
        ),
        CvarDefinition::new(
            "cg_duelSounds",
            1_i64,
            archive,
            "Duel: 0 off, 1 both, 2 sound, 3 text",
        ),
        CvarDefinition::new(
            "cg_scoreDeaths",
            1_i64,
            archive,
            "Deaths: 1 plugin, 2/3 observed",
        ),
        CvarDefinition::new(
            "cg_hitsounds",
            0_i64,
            archive,
            "Hit cues 1-4; saber variants 5-6",
        ),
        CvarDefinition::new(
            "cg_oldPainSounds",
            false,
            archive,
            "Local pain from health loss",
        ),
        CvarDefinition::new(
            "cg_killSounds",
            2_i64,
            archive,
            "Kill cue: 0 off, 1 normal, 2 midair",
        ),
        CvarDefinition::new(
            "cg_jumpSounds",
            0_i64,
            archive,
            "Jump voices: 0 off, 1 all, 2 others, 3 self",
        ),
        CvarDefinition::new(
            "cg_rollSounds",
            1_i64,
            archive,
            "Roll voices: 0 off, 1 all, 2 others, 3 self",
        ),
        CvarDefinition::new(
            "cl_allowDownload",
            1_i64,
            archive,
            "Allow verified UDP pak downloads",
        ),
        CvarDefinition::new("developer", 0_i64, CvarFlags::NONE, "Debug server commands"),
        CvarDefinition::new(
            "pmove_fixed",
            0_i64,
            CvarFlags::READ_ONLY,
            "Server fixed-step movement",
        ),
        CvarDefinition::new(
            "pmove_msec",
            8_i64,
            CvarFlags::READ_ONLY,
            "Server movement step (8..33 ms)",
        ),
        CvarDefinition::new(
            "pmove_float",
            0_i64,
            CvarFlags::READ_ONLY,
            "Server unsnapped velocity",
        ),
        CvarDefinition::new(
            "g_stepSlideFix",
            0_i64,
            CvarFlags::READ_ONLY,
            "Server step-slide correction",
        ),
        CvarDefinition::new("r_resolution", "1280x720", archive, "Window resolution"),
        // Stock r_fullscreen: fullscreen on/off (Alt+Enter toggles it). Which
        // kind of fullscreen is SJK's choice; see settings/display.rs.
        CvarDefinition::new("r_fullscreen", false, archive, "Fullscreen (0/1)"),
        CvarDefinition::new(
            crate::settings::EXCLUSIVE_CVAR,
            false,
            archive,
            "Fullscreen kind: 0 borderless at the desktop size, 1 exclusive video mode at r_resolution",
        ),
        CvarDefinition::new(
            "r_swapInterval",
            false,
            archive,
            "Synchronize presentation (0/1)",
        ),
        CvarDefinition::new(
            crate::menu::style::CVAR,
            crate::menu::style::MenuStyle::DEFAULT_NAME,
            archive,
            "Menu layout: sjk (the SJK UI) or classic (after the original Jedi Academy menus)",
        ),
        CvarDefinition::new(
            "r_gamma",
            1.0_f64,
            archive,
            "Display gamma (0.5–3.0, neutral 1); applies immediately, including HUD",
        ),
        CvarDefinition::new("cg_fov", 90.0_f64, archive, "Horizontal field of view"),
        CvarDefinition::new(
            "cg_fovAspectAdjust",
            true,
            archive,
            "Widen the field of view for widescreen displays",
        ),
        CvarDefinition::new(
            "cl_showTimeDelta",
            // Default off: this was raised to 1 during the step-74 netcode investigation and
            // never lowered, so every connected session spammed the owner's console with a
            // `nettiming:` line every five seconds. Set it to 1 to bring the windows back.
            0_i64,
            CvarFlags::NONE,
            "Log aggregate snapshot and prediction timing every five seconds while connected",
        ),
        CvarDefinition::new(
            "cg_smoothClients",
            0_i64,
            archive,
            "Extrapolate remote players and NPCs from snapshot velocity",
        ),
        CvarDefinition::new(
            "cg_errorDecay",
            100.0_f64,
            archive,
            "Milliseconds over which a prediction miss is smoothed out of the view; 0 snaps",
        ),
        CvarDefinition::new(
            "cg_hudScale",
            0.7_f64,
            archive,
            "Size multiplier for the HUD (0.25 to 2)",
        ),
        CvarDefinition::new(
            "cg_thirdPersonRange",
            80.0,
            archive,
            "Third-person camera distance behind the player",
        ),
        CvarDefinition::new(
            "cg_thirdPersonVertOffset",
            16.0,
            archive,
            "Third-person camera height above the player",
        ),
        CvarDefinition::new(
            "cg_thirdPersonHorzOffset",
            0.0,
            archive,
            "Third-person camera sideways offset",
        ),
        CvarDefinition::new(
            "cg_thirdPersonAngle",
            0.0,
            archive,
            "Third-person camera yaw offset in degrees",
        ),
        CvarDefinition::new(
            "cg_thirdPersonPitchOffset",
            0.0,
            archive,
            "Third-person camera pitch offset in degrees",
        ),
        CvarDefinition::new(
            "cg_thirdPersonCameraDamp",
            0.3_f64,
            archive,
            "Third-person camera easing per cg_cameraFPS frame (per 50 ms below 15); 1 or more snaps to the ideal position",
        ),
        CvarDefinition::new(
            "cg_thirdPersonTargetDamp",
            0.5_f64,
            archive,
            "Third-person look-target easing per cg_cameraFPS frame (per 50 ms below 15); 1 or more snaps",
        ),
        CvarDefinition::new(
            "cg_cameraFPS",
            // A float: the camera reads it with `float_cvar`, so an integer value
            // was never seen and `cg_cameraFPS 0` did nothing.
            125.0_f64,
            archive,
            "Third-person camera easing as EternalJK: damping per frame at this rate, \
             independent of the real frame rate; below 15 uses the original per-50 ms easing",
        ),
        CvarDefinition::new(
            crate::camera::STYLE_CVAR,
            crate::camera::Style::DEFAULT_NAME,
            archive,
            "Third-person camera: ejk stays locked behind you with no damping, as JoF EJK \
             with its strafe helper on; sjk eases after you with the damping cvars",
        ),
        CvarDefinition::new(
            "com_maxfps",
            -1_i64,
            archive | CvarFlags::OMIT_DEFAULT,
            "Maximum rendered frames per second; -1 (AUTO) is 125, or the monitor's refresh \
             rate with com_maxfpsMonitor 1; 0 is uncapped",
        ),
        CvarDefinition::new("cg_drawFPS", false, archive, "Display FPS and frame time"),
        CvarDefinition::new(
            "cg_classicHudFont",
            false,
            archive,
            "Use the retail bitmap font for the in-game status HUD only",
        ),
        CvarDefinition::new(
            crate::game_font::CVAR,
            true,
            archive,
            "Draw text with the game's own fonts where retail did (menus, chat, HUD text, scoreboard, console), else Inter",
        ),
        CvarDefinition::new(
            crate::brush_view::CVAR,
            0_i64,
            archive,
            "Invisible brushes (clip, trigger, ladder...) and movers (door, lift...) drawn see-through: 0 off, 1 gameplay, 2 all",
        ),
        CvarDefinition::new(
            crate::brush_view::TYPES_CVAR,
            "",
            archive,
            "Brush kinds r_showBrushes shows instead of its set: clip playerclip npcclip shotclip              solid trigger teleport push hurt ladder nodrop slick cushion door lift button train endless weather botclip caulk nodraw breakable travel",
        ),
        CvarDefinition::new(
            crate::entity_view::CVAR,
            0_i64,
            archive,
            "Map entities (lights, spawns, origins, items...) marked through walls, pickable with a world note: 0 off, 1 common, 2 all",
        ),
        CvarDefinition::new(
            crate::entity_view::TYPES_CVAR,
            "",
            archive,
            "Entity kinds r_showEntities shows instead of its set: light spawn origin item target sound effect misc other",
        ),
        CvarDefinition::new(
            crate::ground_hud::CVAR,
            false,
            archive,
            "Third-person ground HUD: health, shield, Force and stance around the feet",
        ),
        // Retail names and defaults: OpenJK codemp client/snd_dma.cpp S_Init
        // (s_volume "0.5", s_musicvolume "0.25", s_volumeVoice "1.0",
        // s_khz "44"); the retail sound menu labels s_volume "Effects Volume".
        CvarDefinition::new("s_volume", 0.5_f64, archive, "Effects volume"),
        CvarDefinition::new("s_musicVolume", 0.25_f64, archive, "Music volume"),
        CvarDefinition::new(
            "s_volumeVoice",
            1.0_f64,
            archive,
            "Saved voice gain; mixer bus integration pending",
        ),
        CvarDefinition::new(
            "s_khz",
            44_i64,
            archive,
            "Retail sample-rate preset (11/22/44); SJK always mixes at 44.1 kHz",
        ),
        CvarDefinition::new("s_doppler", true, archive, "Legacy looping-sound Doppler"),
        CvarDefinition::new("cg_footsteps", true, archive, "Play footstep sounds"),
        CvarDefinition::new(
            crate::footprints::PRINTS_CVAR,
            true,
            archive,
            "Steps on sand, snow, mud and dirt kick up dust and leave footprints",
        ),
        CvarDefinition::new(
            crate::footprints::JUMP_CVAR,
            true,
            archive,
            "Jumps and forward landings step with both feet (SJK)",
        ),
        CvarDefinition::new(
            crate::footprints::GUESS_CVAR,
            true,
            archive,
            "Ground a map gives no material takes it from its texture name (SJK)",
        ),
        CvarDefinition::new(
            crate::emotes::VOLUME_CVAR,
            crate::emotes::DEFAULT_VOLUME,
            archive,
            "Loudness of emote sounds, times the effects volume (SJK)",
        ),
        CvarDefinition::new(
            crate::unlock_toast::SOUND_CVAR,
            true,
            archive,
            "Play the secret-area sound with the unlock cards",
        ),
        // Stock defaults and units: `sensitivity` 5 scaling `m_yaw`/`m_pitch`
        // degrees per mouse count (`cl_main.cpp:2774,2802-2803`, applied in
        // `cl_input.cpp:1161-1180`). Matching them is what lets a player carry
        // a sensitivity value over from JKA and keep their aim.
        CvarDefinition::new("sensitivity", 5.0_f64, archive, "Mouse sensitivity"),
        CvarDefinition::new(
            "m_yaw",
            0.022_f64,
            archive,
            "Degrees turned per horizontal mouse count",
        ),
        CvarDefinition::new(
            "m_pitch",
            0.022_f64,
            archive,
            "Degrees turned per vertical mouse count",
        ),
        CvarDefinition::new("m_invert", false, archive, "Invert mouse pitch"),
        CvarDefinition::new("in_raw", true, archive, "Saved raw-input preference"),
        CvarDefinition::new(
            "cl_run",
            true,
            archive,
            "Run by default; the walk key walks",
        ),
        // JoF EJK's `cl_idrive` (`cl_input.cpp`, default 0) and SJK's
        // `cl_idriveDelay` ([`crate::input::idrive`]).
        CvarDefinition::new(
            crate::input::idrive::CVAR,
            0_i64,
            archive,
            "Last-pressed movement key wins: 0 off, 1 all directions, 2 jump/crouch only",
        ),
        // A held push or pull key, as JoF EJK's key repeats do ([`crate::input::force_repeat`]).
        CvarDefinition::new(
            crate::input::force_repeat::CVAR,
            true,
            archive,
            "Holding a push or pull key uses it again as soon as its cooldown is over",
        ),
        CvarDefinition::new(
            crate::input::idrive::DELAY_CVAR,
            0_i64,
            archive,
            "cl_idrive: milliseconds a reversal stays neutral before the new key wins (0-1000)",
        ),
        // Retail defaults: `codemp/cgame/cg_xcvar.h` cg_marks 1, cg_shadows 1,
        // cg_drawGun 1.
        CvarDefinition::new(
            "cg_saberContact",
            true,
            archive,
            "Saber contact marks and sparks",
        ),
        CvarDefinition::new("cg_marks", true, archive, "Leave impact marks on the world"),
        CvarDefinition::new("cg_shadows", true, archive, "Draw player shadows"),
        // EternalJK's `cg_dismember` (`cg_xcvar.h`) is off by default; SJK shows
        // every cut limb.
        CvarDefinition::new(
            crate::dismember::CVAR,
            2_i64,
            archive,
            "Show cut-off limbs: 0 none, 1 no heads or waists, 2 all",
        ),
        CvarDefinition::new(
            crate::dismember::SERVER_CVAR,
            0_i64,
            archive,
            "Chance (0-100) that games you host cut off limbs",
        ),
        CvarDefinition::new("cg_drawGun", true, archive, "Draw the first-person weapon"),
        CvarDefinition::new(
            "cg_debugMissiles",
            0_i64,
            CvarFlags::NONE,
            "Log missile trails once a second (diagnostics)",
        ),
        CvarDefinition::new(
            "fx_debug",
            0_i64,
            CvarFlags::NONE,
            "Log each effect as it plays, with its shaders and sizes (diagnostics)",
        ),
        CvarDefinition::new(
            crate::debug_collectibles::CVAR,
            false,
            archive,
            "SJK staff: show every collectible as held, on this PC only, to try them (the hub and other players see what you really hold)",
        ),
        CvarDefinition::new(
            "cg_drawCrosshair",
            1_i64,
            archive,
            "Crosshair picture 0-10: 0 hides it, 1-8 gfx/2d/crosshairb-i, 9 a, 10 j, a white dot of cg_crosshairSize pixels",
        ),
        // Stock defaults from `codemp/cgame/cg_xcvar.h:60,72,102`.
        CvarDefinition::new(
            "cg_drawCrosshairNames",
            true,
            archive,
            "Display the name under the crosshair",
        ),
        CvarDefinition::new("cg_drawTimer", true, archive, "Display elapsed match time"),
        CvarDefinition::new(
            "cg_drawTimerUpperRight",
            false,
            archive,
            "Show the match time in the upper right under the frame rate, as JoF EJK does, not in the HUD layout",
        ),
        CvarDefinition::new(
            crate::version_overlay::CVAR,
            true,
            archive,
            "Draw SJK's version, build date and commit at the top of the screen",
        ),
        // JoF EJK's `flipkick` run (`cg_xcvar.h`), counted in user commands.
        CvarDefinition::new(
            crate::input::flip_kick::DURATION_CVAR,
            50_i64,
            archive,
            "flipkick: user commands the run of jump taps lasts",
        ),
        CvarDefinition::new(
            crate::input::flip_kick::FIRST_JUMP_CVAR,
            0_i64,
            archive,
            "flipkick: user commands the first jump stays held",
        ),
        CvarDefinition::new(
            crate::input::flip_kick::SECOND_JUMP_CVAR,
            0_i64,
            archive,
            "flipkick: user command at which the second jump starts",
        ),
        CvarDefinition::new(
            "cg_lagometer",
            false,
            archive,
            "Display frame interpolation and snapshot latency samples",
        ),
        CvarDefinition::new(
            "cg_saberTrail",
            1_i64,
            archive,
            "Saber motion trails; mode 2 currently uses the normal trail",
        ),
        CvarDefinition::new(
            crate::effect_aux::saber_rain_steam::CVAR,
            1_i64,
            archive,
            "Saber steam in rain: 0 off, 1 everyone, 2 only me",
        ),
        // Stock default "1" (`codemp/cgame/cg_xcvar.h`).
        CvarDefinition::new(
            "cg_speedTrail",
            1_i64,
            archive,
            "Draw Force Speed afterimages behind players",
        ),
        CvarDefinition::new(
            "cg_auraShell",
            1_i64,
            archive,
            "Display the Force Seeing aura shell on other players",
        ),
        // JoF EJK's `cg_spprotabscolor` (cg_xcvar.h), same default.
        CvarDefinition::new(
            "cg_spProtAbsColor",
            true,
            archive,
            "Protect and Absorb together show one cyan shell, as in single player",
        ),
        CvarDefinition::new(
            crate::unlockables::SABER_SKIN_CVAR,
            "",
            archive,
            "The blade skin your sabers wear, by unlock id (saber_sun: the Sun blade), shown once your SJK profile holds it; empty for the stock blade",
        ),
        CvarDefinition::new(
            crate::saber_skin_options::CVAR,
            "",
            archive,
            "The blade skins' parts you switched off, skin.option words (saber_sun.haze: the Sun blade without its heat haze); every SJK player sees your blade so",
        ),
        CvarDefinition::new(
            crate::quick_wheel::SOUNDS_CVAR,
            true,
            archive,
            "Quick wheel sounds: changing page, moving to another choice and running one (0 silent)",
        ),
        CvarDefinition::new(
            "cg_shieldSphere",
            0_i64,
            archive,
            "Show a shield hit as multiplayer's sphere (1) instead of a shell on the body (0)",
        ),
        CvarDefinition::new(
            "cg_hitMarker",
            false,
            archive,
            "Show the red damage-direction indicator near the crosshair (0 off, 1 on)",
        ),
        CvarDefinition::new(
            "cg_shieldBrightness",
            2_i64,
            archive,
            "How bright a shield hit shows on the body, 1 (stock) to 12, default 2; ignored by the sphere",
        ),
        CvarDefinition::new(
            "cg_shieldBrightnessDefaultVersion",
            0_i64,
            archive,
            "Internal migration marker for the two-pass shield flash default",
        ),
        CvarDefinition::new(
            crate::whats_new_card::SEEN_CVAR,
            "",
            archive,
            "The SJK version whose update recap (the what's-new card) you closed; empty shows the current one once",
        ),
        CvarDefinition::new(
            crate::donation_goal::CVAR,
            true,
            archive,
            "Show SJK's donation goal on the SJK UI's main page, read from the SJK hub, with Donate opening SJK's Ko-fi page (0 hides it and asks the hub nothing)",
        ),
        CvarDefinition::new(
            "ui_hideFirstSetup",
            false,
            archive,
            "Don't open First setup when the client starts (the firstsetup command still opens it)",
        ),
        CvarDefinition::new(
            "cl_autoUpdate",
            true,
            archive,
            "Look for a newer SJK release at start and every 30 minutes while running (the Update page installs it)",
        ),
        CvarDefinition::new(
            "cl_identity",
            true,
            archive,
            "Keep an identity key and tell the SJK hub which game server you are on, so other SJK players see your badge (0 sends nothing)",
        ),
        CvarDefinition::new(
            crate::crash_report::CVAR,
            true,
            archive,
            "Send a report to the SJK team when SJK crashes (build, system, graphics card, map, server, the error and the last console lines without chat), at the next start through the SJK hub (0 keeps it on this PC only)",
        ),
        CvarDefinition::new(
            crate::settings_usage::CVAR,
            true,
            archive,
            "Tell the SJK team anonymously which settings you change and what you search for in Settings (no key, name or server is sent; 0 sends nothing)",
        ),
        CvarDefinition::new(
            crate::settings::mode::CVAR,
            crate::settings::mode::SettingsMode::Simple.name(),
            archive,
            "Settings shows the settings that matter most (simple) or every setting (advanced)",
        ),
        CvarDefinition::new(
            "cl_hubUrl",
            crate::player_identity::DEFAULT_HUB_URL,
            archive,
            "Address of the SJK hub (https://...); empty means no hub",
        ),
        CvarDefinition::new(
            "cl_sjkChat",
            true,
            archive,
            "Show the SJK chat, which every SJK player shares through the SJK hub, and read it (0 hides it and stops reading)",
        ),
        CvarDefinition::new(
            crate::installed_models::CVAR,
            true,
            archive,
            "Tell the SJK hub which player models you have, so other SJK players' cards say whether they see the model you wear (0 makes the hub forget your list)",
        ),
        CvarDefinition::new(
            crate::world_ping::SHOW_CVAR,
            true,
            archive,
            "Show other SJK players' world pings (+ping): your team's in team games, everyone's otherwise; 0 hides them",
        ),
        CvarDefinition::new(
            crate::world_ping::SOUND_CVAR,
            true,
            archive,
            "Play a short sound when a world ping appears, yours included",
        ),
        CvarDefinition::new(
            crate::sjk_chat_frame::SOUND_CVAR,
            true,
            archive,
            "Play a sound when an SJK chat message arrives, yours included (cg_chatSounds 0 silences it too)",
        ),
        CvarDefinition::new(
            crate::chat_gifs::CVAR,
            true,
            archive,
            "Show the GIF of a GIPHY link in SJK chat, fetched from GIPHY by this PC, which GIPHY sees (0 keeps links as text and fetches nothing)",
        ),
        CvarDefinition::new(
            crate::chat_gifs::search::KEY_CVAR,
            "",
            archive,
            "Your own GIPHY API key for the SJK chat's GIF search (empty uses SJK's, shared by every SJK player)",
        ),
        CvarDefinition::new(
            "cl_updateAs",
            "",
            CvarFlags::NONE,
            "Pretend to be this release version when checking for updates (testing a local build)",
        ),
        // JoF EJK's: 1 shows your own movement from the server, 2 also its angles.
        CvarDefinition::new(
            "cg_noPredict",
            0_i64,
            CvarFlags::NONE,
            "Skip movement prediction: 1 shows the server's position, 2 its angles too",
        ),
        CvarDefinition::new(
            "cg_freeCamera",
            0_i64,
            CvarFlags::NONE,
            "Detached camera flight with a stationary server body; use /freecam",
        ),
        CvarDefinition::new(
            // JoF EJK fake noclip: fly locally with a stationary server body.
            "cg_fakeNoclip",
            0_i64,
            CvarFlags::NONE,
            "Fly client-side while the server sees you standing still (use /fakenoclip)",
        ),
        // JoF EJK's: 0 hides all hats and capes, 1 shows everyone's, 2 only yours.
        CvarDefinition::new(
            crate::cosmetics::VISIBILITY_CVAR,
            1_i64,
            archive,
            "Draw worn hats and capes: 0 none, 1 everyone, 2 only yours",
        ),
        // Clamped 15..1000 like JoF EJK and EternalJK (`CL_ReadyToSendPacket`).
        CvarDefinition::new(
            "cl_maxpackets",
            DEFAULT_MAX_PACKETS,
            archive,
            "Most move packets sent a second (15 to 1000); user commands are made at your gameplay FPS (gameplay_fps) and wait for the next packet",
        ),
        // SJK's: the servers known to cap commands keep their cap unless lifted
        // (`server_caps.rs`).
        CvarDefinition::new(
            crate::server_caps::OVERRIDE_CVAR,
            0_i64,
            archive,
            "1 ignores the servers SJK knows to cap commands (JoF's at 125 a second): gameplay_fps applies as set there too",
        ),
        // SJK's: the frame rate whose movement the player wants, apart from the
        // frames drawn, or 0 to follow the FPS cap (`command_rate`, `gameplay_fps.rs`).
        CvarDefinition::new(
            crate::gameplay_fps::CVAR,
            sjk_client::command_rate::AUTO_GAMEPLAY_FPS,
            archive,
            "Gameplay FPS: 0 (Auto) moves you at your FPS cap; 125, 142, 250, 333 or 20 to 1000 keep that movement whatever FPS you draw; a pmove_fixed server's step wins; help_gameplay_fps explains it",
        ),
        // Stock default "1", clamped 0..5 (`codemp/client/cl_input.cpp:1539`).
        CvarDefinition::new(
            "cl_packetdup",
            1_i64,
            archive,
            "Repeat the previous N packets' usercmds in every packet (0 to 5) so lost packets drop no move",
        ),
        CvarDefinition::new(
            "cl_timeNudge",
            0_i64,
            archive,
            "Live presentation latency in milliseconds (-30 to 30); positive adds buffer",
        ),
        CvarDefinition::new("cg_drawHud", true, archive, "Display the in-game HUD"),
        CvarDefinition::new("cg_draw2D", true, archive, "Display 2D game overlays"),
        CvarDefinition::new(
            "cg_simpleItems",
            false,
            archive,
            "Draw pickup icons instead of models",
        ),
        CvarDefinition::new(
            "cg_forceModel",
            false,
            archive,
            "Use your model for other players",
        ),
        CvarDefinition::new(
            "cg_crosshairSize",
            24.0_f64,
            archive,
            "Crosshair size in 640x480 units",
        ),
        CvarDefinition::new(
            "cg_speedometer",
            0_i64,
            archive,
            "Speed bits: 1 on, 256 kph, 512 mph, 32768 XYZ",
        ),
        CvarDefinition::new(
            "cl_consoleKeys",
            // `cl_main.cpp:2826` ships "~ ` 0x7e 0x60 0xb2"; 0xb0 (the shifted
            // German key) is added so shift+that key opens the console there.
            "~ ` 0x7e 0x60 0xb2 0xb0",
            archive,
            "Characters that toggle the console",
        ),
        CvarDefinition::new(
            "cg_drawStatus",
            true,
            archive,
            "Display health, armor, and force status",
        ),
        CvarDefinition::new(
            "cg_drawWeapon",
            true,
            archive,
            "Display current weapon and ammunition",
        ),
        CvarDefinition::new(
            "cg_drawScores",
            true,
            archive,
            "Display the bound scoreboard",
        ),
        CvarDefinition::new(
            crate::scoreboard::style::CVAR,
            crate::scoreboard::style::ScoreboardStyle::DEFAULT_NAME,
            archive,
            "Scoreboard layout: auto (sjk with the SJK UI's menus, else classic), sjk, or classic (after the retail scoreboard)",
        ),
        CvarDefinition::new(
            crate::scoreboard::style::COMPACT_CVAR,
            true,
            archive,
            "SJK scoreboard: thin rows, so every player fits in one column, as wide as the names and centred",
        ),
        CvarDefinition::new(
            crate::scoreboard::style::FPS_CVAR,
            true,
            archive,
            "SJK scoreboard: show the frame rate on the header line, left of the clock",
        ),
        CvarDefinition::new(
            "cg_smallScoreboard",
            false,
            archive,
            "Classic scoreboard: always use the small rows",
        ),
        CvarDefinition::new(
            "cg_showClientIDs",
            true,
            archive,
            "Classic scoreboard: show each player's client ID",
        ),
        CvarDefinition::new(
            crate::jof_tag::CVAR,
            true,
            archive,
            "Show a known clan's emblem left of names carrying its tag (JoF, JoFA)",
        ),
        CvarDefinition::new(
            "cg_drawScoreboardIcons",
            true,
            archive,
            "Classic scoreboard: show each player's head icon",
        ),
        CvarDefinition::new(
            "cg_drawScoreboardPlayerCount",
            1_i64,
            archive,
            "Classic scoreboard header: 0 off, 1 host name and counts, 2 counts",
        ),
        CvarDefinition::new(
            "cg_drawChat",
            true,
            archive,
            "Display incoming chat and prints",
        ),
        CvarDefinition::new(
            "cl_sensitivityScaleVersion",
            0_i64,
            archive,
            "Internal migration marker for the move to stock sensitivity units",
        ),
        CvarDefinition::new(
            RATE_VERSION_CVAR,
            0_i64,
            archive,
            "Internal migration marker for the 90000 rate default",
        ),
        CvarDefinition::new(
            "com_maxfpsDefaultVersion",
            0_i64,
            archive,
            "Internal migration marker for the AUTO com_maxfps default",
        ),
        CvarDefinition::new(
            "cl_bindDefaultsVersion",
            0_i64,
            archive,
            "Internal stock-bind migration version",
        ),
        CvarDefinition::new(
            "cl_consoleKeyDefaultVersion",
            0_i64,
            archive,
            "Internal migration marker for the scan-code console key default",
        ),
        CvarDefinition::new(
            "cg_scoreboardStyleDefaultVersion",
            0_i64,
            archive,
            "Internal migration marker for the auto scoreboard style default",
        ),
        CvarDefinition::new(
            "cl_wheelBindVersion",
            0_i64,
            archive,
            "Internal migration marker for Q's quick wheel bind opening the last page",
        ),
        CvarDefinition::new(
            "ui_menuStyleDefaultVersion",
            0_i64,
            archive,
            "Internal migration marker for the SJK UI menu style default",
        ),
        CvarDefinition::new(
            "cg_cameraStyleDefaultVersion",
            0_i64,
            archive,
            "Internal migration marker for the ejk camera style default",
        ),
        CvarDefinition::new(
            "cg_killfeedDefaultVersion",
            0_i64,
            archive,
            "Internal migration marker for the kill feed being on by default",
        ),
    ];
    for definition in definitions {
        cvars.register(definition)?;
    }
    cvars.register_alias("r_vsync", "r_swapInterval")?;
    crate::cgame_options::register(cvars)
}
