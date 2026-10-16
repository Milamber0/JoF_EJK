//! The classic main menu's page tables: every page's entries in retail item
//! order (which keyboard focus follows), with their retail positions on the
//! 640x480 canvas. Types and geometry are in [`super::layout`].

use super::layout::{Entry, Page, Size, Slot};
use sjk_ui::TextAlign;

/// A centred button `width` canvas units wide and 30 high.
const fn button(
    entry: Entry,
    label: &'static str,
    hint: &'static str,
    center: [f32; 2],
    width: f32,
    size: Size,
) -> Slot {
    Slot {
        entry,
        label,
        hint,
        center,
        width,
        height: 30.0,
        size,
        align: TextAlign::Center,
    }
}

/// One row of the Controls and Setup option lists: retail rect `80 y 170
/// 24`, its label set against the right edge.
const fn list_row(entry: Entry, label: &'static str, hint: &'static str, y: f32) -> Slot {
    Slot {
        entry,
        label,
        hint,
        center: [165.0, y + 12.0],
        width: 170.0,
        height: 24.0,
        size: Size::List,
        align: TextAlign::End,
    }
}

/// One entry of the start-playing centre list: retail rect `225 y 190 36`.
const fn centre_row(entry: Entry, label: &'static str, hint: &'static str, y: f32) -> Slot {
    Slot {
        entry,
        label,
        hint,
        center: [320.0, y + 18.0],
        width: 190.0,
        height: 34.0,
        size: Size::Medium,
        align: TextAlign::Center,
    }
}

const PLAY_HINT: &str = "Solo game, join a server or start your own";
const PROFILE_HINT: &str = "Name, model, saber and Force";
const SETTINGS_HINT: &str = "Key bindings and every option, with search";
const SJK_HINT: &str = "Changelog, credits and updates";
const EXIT_HINT: &str = "Leave the game";
const CREDITS_HINT: &str = "The people who make SJK";
const CHANGELOG_HINT: &str = "What changed in each SJK release, and who made it";
const UPDATE_HINT: &str = "Check for a newer SJK release and install it";
const IDENTITY_HINT: &str = "Your SJK identity: your key and the SJK hub";
const SJK_PROFILE_HINT: &str = "Your SJK profile: medals, bio, record and achievements";
const BACK_HINT: &str = "Return to the main menu";

/// Retail `main.menu`: two columns either side of the centre window, Exit
/// below. SJK: Settings takes retail's Controls place and gathers Setup too;
/// SJK (changelog, credits, update) takes Setup's.
const MAIN: [Slot; 5] = [
    button(
        Entry::Play,
        "PLAY",
        PLAY_HINT,
        [101.0, 224.0],
        190.0,
        Size::Large,
    ),
    button(
        Entry::Profile,
        "PROFILE",
        PROFILE_HINT,
        [101.0, 322.0],
        190.0,
        Size::Large,
    ),
    button(
        Entry::Settings,
        "SETTINGS",
        SETTINGS_HINT,
        [521.0, 224.0],
        190.0,
        Size::Large,
    ),
    button(
        Entry::Sjk,
        "SJK",
        SJK_HINT,
        [521.0, 322.0],
        190.0,
        Size::Large,
    ),
    button(
        Entry::Exit,
        "EXIT",
        EXIT_HINT,
        [320.0, 456.0],
        190.0,
        Size::Large,
    ),
];

/// The navigation row every retail sub-menu repeats along its top (retail
/// rects `7 126`, `170 126`, `340 126`, `502 126`, 130 by 24).
const fn nav_row() -> [Slot; 4] {
    [
        button(
            Entry::Play,
            "PLAY",
            PLAY_HINT,
            [72.0, 138.0],
            130.0,
            Size::Medium,
        ),
        button(
            Entry::Profile,
            "PROFILE",
            PROFILE_HINT,
            [235.0, 138.0],
            130.0,
            Size::Medium,
        ),
        button(
            Entry::Settings,
            "SETTINGS",
            SETTINGS_HINT,
            [405.0, 138.0],
            130.0,
            Size::Medium,
        ),
        button(
            Entry::Sjk,
            "SJK",
            SJK_HINT,
            [567.0, 138.0],
            130.0,
            Size::Medium,
        ),
    ]
}

/// Back and Exit along the bottom of every sub-page (retail rects `59 444`
/// and `255 444`).
const fn back_exit() -> [Slot; 2] {
    [
        button(
            Entry::Back,
            "BACK",
            BACK_HINT,
            [124.0, 456.0],
            130.0,
            Size::Medium,
        ),
        button(
            Entry::Exit,
            "EXIT",
            EXIT_HINT,
            [320.0, 456.0],
            130.0,
            Size::Medium,
        ),
    ]
}

/// Retail `multiplayer.menu`: the start-playing list in the centre.
const PLAY: [Slot; 11] = {
    let [play, profile, settings, sjk] = nav_row();
    let [back, exit] = back_exit();
    [
        play,
        profile,
        settings,
        sjk,
        centre_row(
            Entry::SoloGame,
            "SOLO GAME",
            "A local match with bots, set up in Create game",
            191.0,
        ),
        centre_row(
            Entry::JoinServer,
            "JOIN SERVER",
            "Browse servers and join a game",
            226.0,
        ),
        centre_row(
            Entry::CreateServer,
            "CREATE SERVER",
            "Host a match with bots on this machine",
            261.0,
        ),
        centre_row(
            Entry::PlayDemo,
            "PLAY DEMO",
            "Not in SJK yet: use the demo console command",
            296.0,
        ),
        centre_row(
            Entry::Rules,
            "RULES",
            "Not in SJK yet: no rules pages",
            331.0,
        ),
        back,
        exit,
    ]
};

/// Settings' KEY BINDINGS tab (retail `controls.menu`): the binding
/// categories down the left, each a place in the one list of every binding.
/// Retail's two Force Powers pages are one category; the mouse options moved
/// to OPTIONS.
const CONTROLS: [Slot; 11] = {
    let [play, profile, settings, sjk] = nav_row();
    let [back, exit] = back_exit();
    [
        play,
        profile,
        settings,
        sjk,
        list_row(
            Entry::Movement,
            "MOVEMENT",
            "Moving, jumping, turning and looking",
            185.0,
        ),
        list_row(
            Entry::Interaction,
            "INTERACTION",
            "Attacks, saber, use and items",
            209.0,
        ),
        list_row(Entry::Weapons, "WEAPONS", "Every weapon", 233.0),
        list_row(
            Entry::ForcePowers,
            "FORCE POWERS",
            "Every Force power",
            257.0,
        ),
        list_row(
            Entry::OtherControls,
            "OTHER",
            "Chat, scores, votes, emotes and the console",
            281.0,
        ),
        back,
        exit,
    ]
};

/// Settings' OPTIONS tab (retail `setup.menu`): the option groups down the
/// left. Classic+ starts with the first-start settings, gathers the video and
/// renderer groups under GRAPHICS and everything about play and its screens
/// under GAMEPLAY, keeps retail's Sound, and leaves out retail's Mods and
/// Defaults, which SJK cannot offer (Backspace restores one default).
const SETUP: [Slot; 10] = {
    let [play, profile, settings, sjk] = nav_row();
    let [back, exit] = back_exit();
    [
        play,
        profile,
        settings,
        sjk,
        list_row(
            Entry::FirstSetup,
            "FIRST SETUP",
            "The settings worth choosing first: display, aim, sound, HUD, nameplates",
            185.0,
        ),
        list_row(
            Entry::Graphics,
            "GRAPHICS",
            "Video, the renderer's image, lighting and shadows, and the weather",
            209.0,
        ),
        list_row(
            Entry::Sound,
            "SOUND",
            "Effects and music volume, footsteps",
            233.0,
        ),
        list_row(
            Entry::Gameplay,
            "GAMEPLAY",
            "Mouse, game options, interface, HUD, scoreboard and network",
            257.0,
        ),
        back,
        exit,
    ]
};

/// Setup's GRAPHICS (classic+): `setup.menu`'s layout with retail's video
/// group (its two pages as one), the renderer settings' three groups and the
/// weather down the left; Back returns to Setup.
const GRAPHICS: [Slot; 11] = {
    let [play, profile, settings, sjk] = nav_row();
    let [back, exit] = back_exit();
    [
        play,
        profile,
        settings,
        sjk,
        list_row(
            Entry::Video,
            "VIDEO",
            "Resolution, display, frame rate, field of view and brightness",
            185.0,
        ),
        list_row(
            Entry::RenderImage,
            "IMAGE",
            "HDR, exposure, bloom, glow, edge smoothing, reflections and emission",
            209.0,
        ),
        list_row(
            Entry::RenderLighting,
            "LIGHTING",
            "Sun and sky, live lighting, fill light and light shafts",
            233.0,
        ),
        list_row(
            Entry::RenderShadows,
            "SHADOWS",
            "Sun shadows: on or off, detail, distance and edges",
            257.0,
        ),
        list_row(
            Entry::Weather,
            "WEATHER",
            "Rain, snow, fog and clouds: quality, amount, forced weather",
            281.0,
        ),
        Slot {
            entry: Entry::SetupBack,
            hint: "Return to the setup options",
            ..back
        },
        exit,
    ]
};

/// Setup's GAMEPLAY (classic+), laid out as [`GRAPHICS`]: the mouse options
/// brought over from Controls, and SJK's additions regrouped by subject (game
/// options, the menus and console, the HUD, the scoreboard), then the network.
const GAMEPLAY: [Slot; 12] = {
    let [play, profile, settings, sjk] = nav_row();
    let [back, exit] = back_exit();
    [
        play,
        profile,
        settings,
        sjk,
        list_row(
            Entry::MouseJoystick,
            "MOUSE",
            "Mouse sensitivity, inversion and always run",
            185.0,
        ),
        list_row(
            Entry::GameOptions,
            "GAME OPTIONS",
            "Pickups, models, saber and Force trails, camera",
            209.0,
        ),
        list_row(
            Entry::Interface,
            "INTERFACE",
            "Menu style, colours and fonts, the console's look",
            233.0,
        ),
        list_row(
            Entry::Hud,
            "HUD",
            "HUD style and scale, status, crosshair, readouts and chat",
            257.0,
        ),
        list_row(
            Entry::Scoreboard,
            "SCOREBOARD",
            "Scoreboard style, client numbers, head icons and row size",
            281.0,
        ),
        list_row(
            Entry::Network,
            "NETWORK",
            "Master server and connection rates",
            305.0,
        ),
        Slot {
            entry: Entry::SetupBack,
            hint: "Return to the setup options",
            ..back
        },
        exit,
    ]
};
/// SJK's page behind its button: the start-playing list's layout, holding
/// SJK's own screens.
const SJK: [Slot; 11] = {
    let [play, profile, settings, sjk] = nav_row();
    let [back, exit] = back_exit();
    [
        play,
        profile,
        settings,
        sjk,
        centre_row(Entry::Changelog, "CHANGELOG", CHANGELOG_HINT, 191.0),
        centre_row(Entry::Credits, "CREDITS", CREDITS_HINT, 226.0),
        centre_row(Entry::Update, "UPDATE", UPDATE_HINT, 261.0),
        centre_row(Entry::SjkProfile, "PROFILE", SJK_PROFILE_HINT, 296.0),
        centre_row(Entry::Identity, "IDENTITY", IDENTITY_HINT, 331.0),
        back,
        exit,
    ]
};

/// Retail `quit.menu`: No bottom left, Yes bottom right.
const QUIT: [Slot; 6] = {
    let [play, profile, settings, sjk] = nav_row();
    [
        play,
        profile,
        settings,
        sjk,
        button(
            Entry::No,
            "NO",
            BACK_HINT,
            [124.0, 456.0],
            130.0,
            Size::Medium,
        ),
        button(
            Entry::Yes,
            "YES",
            "Exit to the desktop",
            [519.0, 456.0],
            130.0,
            Size::Medium,
        ),
    ]
};

/// The entries of `page`, in focus order.
pub(super) fn slots(page: Page) -> &'static [Slot] {
    match page {
        Page::Main => &MAIN,
        Page::Play => &PLAY,
        Page::Controls => &CONTROLS,
        Page::Setup => &SETUP,
        Page::Sjk => &SJK,
        Page::Graphics => &GRAPHICS,
        Page::Gameplay => &GAMEPLAY,
        Page::Quit => &QUIT,
    }
}
