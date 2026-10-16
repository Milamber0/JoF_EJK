//! The classic Setup page's own groups. Retail's `setup.menu` and the settings
//! screen's tabs cut the settings differently, so classic+ regroups them by
//! what they are about (`docs/classic-plus.md`): gameplay options, the
//! menus and console, the HUD, the scoreboard. Each group lists its rows'
//! cvars; the settings come from the catalogue, so a group shows the same
//! rows the tabs do.

use super::catalog::*;
use std::sync::OnceLock;

/// A classic Setup group gathering rows of several settings tabs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Group {
    /// Gameplay: pickups, models, trails, remaps, camera and prediction.
    GameOptions,
    /// The menus' style, colours and fonts, and the console's look.
    Interface,
    /// The status HUD, crosshair and the readouts over the game.
    Hud,
    /// The scoreboard's style and columns.
    Scoreboard,
    /// The first-start settings ([`super::quick`]); gathers rows the other groups
    /// and tabs already hold, so it is not one of [`Group::ALL`].
    Quick,
    /// The renderer and its quality, then the renderer's four tabs (image,
    /// lighting, shadows, weather), as one group under their names, for the SJK
    /// UI's Graphics; the classic pages show each tab on its own, so it is not
    /// one of [`Group::ALL`].
    Graphics,
    /// The SJK UI's Display (Advanced): Video's rows but the renderer's, which
    /// Graphics leads with, under sub-headings. Not one of [`Group::ALL`]: the
    /// classic pages show the Video tab.
    Display,
    /// The SJK UI's Network (Advanced): the Network tab's rows under
    /// sub-headings (SJK's services, the connection), the SJK chat's left to
    /// Chat. Not one of [`Group::ALL`]: the classic pages show the tab.
    Network,
    /// Advanced's own regroupings of the classic groups and tabs, SJK UI only.
    Advanced(Advanced),
    /// Simple mode's categories ([`super::mode`]): the settings that matter
    /// most, a few to a category. SJK UI only.
    Simple(Simple),
}

/// Simple mode's categories, in the rail's order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Simple {
    Display,
    Graphics,
    Sound,
    Controls,
    Gameplay,
    Hud,
    General,
}

/// Advanced's categories that regroup the classic groups' rows by subject
/// (`docs/sjk-ui.md`, Settings): every sound in Sound, the chats in Chat, the
/// packs, mods and tools in Mods, so Gameplay, Interface and HUD keep what is
/// theirs (HUD with the scoreboard's rows).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Advanced {
    Sound,
    Gameplay,
    Interface,
    Hud,
    Chat,
    Mods,
}

impl Advanced {
    pub(crate) const ALL: [Self; 6] = [
        Self::Sound,
        Self::Gameplay,
        Self::Interface,
        Self::Hud,
        Self::Chat,
        Self::Mods,
    ];
}

/// The rows Chat gathers: the in-game chat box's (HUD's in the classic
/// groups, the code page of other clients' text Interface's) and the SJK chat's
/// (Network's).
const CHAT_ROWS: &[&str] = &[
    "cg_drawChat",
    crate::chat::emoji::CVAR,
    crate::chat::reply::REMEMBER_CVAR,
    "cg_chatBoxLetterSpacing",
    crate::text::charset::CVAR,
    "cl_sjkChat",
    crate::chat_gifs::CVAR,
    crate::chat_gifs::search::KEY_CVAR,
];

/// The rows Mods gathers from the classic Game options.
const MODS_ROWS: &[&str] = &[
    "assetbrowser",
    "mod_japlus",
    "mod_jof",
    crate::brush_view::CVAR,
    crate::entity_view::CVAR,
    crate::ghost_replay::SHOWCASE_CVAR,
];

/// The sounds Sound gathers from Network and Interface after the Sound tab's.
const NOTIFICATION_SOUNDS: &[&str] = &[
    crate::sjk_chat_frame::SOUND_CVAR,
    crate::world_ping::SOUND_CVAR,
    crate::quick_wheel::SOUNDS_CVAR,
];

/// The quick wheel's rows of the classic Interface, which Advanced's Quick
/// wheel category and Sound hold.
const WHEEL_ROWS: &[&str] = &[
    crate::quick_wheel::pages::FILE,
    crate::quick_wheel::SOUNDS_CVAR,
];

impl Simple {
    pub(crate) const ALL: [Self; 7] = [
        Self::Display,
        Self::Graphics,
        Self::Sound,
        Self::Controls,
        Self::Gameplay,
        Self::Hud,
        Self::General,
    ];
}

/// The rows Graphics leads with, before the renderer's tabs: the renderer, its
/// quality and the two effects the original game had (impact marks, player
/// shadows).
const GRAPHICS_LEAD: &[&str] = &[
    crate::graphics_quality::RENDERER_ROW,
    crate::graphics_quality::ROW_NAME,
    "cg_marks",
    "cg_shadows",
];

/// The renderer tabs [`Group::Graphics`] gathers after [`GRAPHICS_LEAD`], each
/// under its heading.
fn graphics_tabs() -> [(&'static str, &'static [Setting]); 4] {
    [
        ("Image", RENDER_IMAGE),
        ("Lighting", RENDER_LIGHTING),
        ("Shadows", RENDER_SHADOWS),
        ("Weather", RENDER_WEATHER),
    ]
}

/// The heading over [`GRAPHICS_LEAD`].
const GRAPHICS_LEAD_HEADING: &str = "Quality";

/// The catalogue's row of setting `cvar`, on any tab, the renderer's included.
fn find(cvar: &str) -> Option<Setting> {
    (0..TABS.len())
        .flat_map(super::settings)
        .chain(
            [
                RENDER_IMAGE,
                RENDER_LIGHTING,
                RENDER_SHADOWS,
                RENDER_WEATHER,
            ]
            .into_iter()
            .flatten(),
        )
        .find(|setting| setting.cvar.eq_ignore_ascii_case(cvar))
        .copied()
}

impl Group {
    /// The classic Setup page's groups.
    #[cfg(test)]
    pub(crate) const ALL: [Self; 4] = [
        Self::GameOptions,
        Self::Interface,
        Self::Hud,
        Self::Scoreboard,
    ];

    /// How many groups there are, Simple's and Advanced's included.
    const COUNT: usize = 8 + Simple::ALL.len() + Advanced::ALL.len();

    /// The group's name, as the one tab of the tabbed screen showing it.
    const CAPTIONS: [&'static str; Self::COUNT] = [
        "GAME OPTIONS",
        "INTERFACE",
        "HUD",
        "SCOREBOARD",
        super::catalog::FIRST_SETUP_CAPTION,
        "GRAPHICS",
        "DISPLAY",
        "NETWORK",
        "DISPLAY",
        "GRAPHICS",
        "SOUND",
        "CONTROLS",
        "GAMEPLAY",
        "HUD",
        "GENERAL",
        "SOUND",
        "GAMEPLAY",
        "INTERFACE",
        "HUD",
        "CHAT",
        "MODS",
    ];

    fn index(self) -> usize {
        match self {
            Self::GameOptions => 0,
            Self::Interface => 1,
            Self::Hud => 2,
            Self::Scoreboard => 3,
            Self::Quick => 4,
            Self::Graphics => 5,
            Self::Display => 6,
            Self::Network => 7,
            Self::Simple(simple) => 8 + simple as usize,
            Self::Advanced(advanced) => 8 + Simple::ALL.len() + advanced as usize,
        }
    }

    /// The cvars of the group's rows, in order: [`Self::cvars`], or for one of
    /// Advanced's, the classic groups' rows it takes.
    fn cvar_list(self) -> Vec<&'static str> {
        let without = |group: Self, left: &[&str]| -> Vec<&'static str> {
            group
                .cvars()
                .iter()
                .copied()
                .filter(|cvar| !left.contains(cvar))
                .collect()
        };
        match self {
            Self::Advanced(Advanced::Gameplay) => without(Self::GameOptions, MODS_ROWS),
            Self::Advanced(Advanced::Interface) => without(Self::Interface, WHEEL_ROWS)
                .into_iter()
                .filter(|cvar| !CHAT_ROWS.contains(cvar))
                .collect(),
            Self::Advanced(Advanced::Hud) => {
                let mut cvars: Vec<&str> = without(Self::Hud, CHAT_ROWS)
                    .into_iter()
                    .filter(|cvar| !NOTIFICATION_SOUNDS.contains(cvar))
                    .collect();
                cvars.extend_from_slice(Self::Scoreboard.cvars());
                cvars
            }
            Self::Advanced(Advanced::Sound) => {
                let mut cvars: Vec<&str> = AUDIO.iter().map(|setting| setting.cvar).collect();
                cvars.extend_from_slice(NOTIFICATION_SOUNDS);
                cvars
            }
            _ => self.cvars().to_vec(),
        }
    }

    /// The tab list of a screen showing the group: its name alone.
    pub(super) fn tabs(self) -> &'static [&'static str] {
        std::slice::from_ref(&Self::CAPTIONS[self.index()])
    }

    /// The cvars of the group's rows, in order.
    fn cvars(self) -> &'static [&'static str] {
        match self {
            Self::GameOptions => &[
                "assetbrowser",
                "cg_simpleItems",
                "cg_forceModel",
                "cg_saberTrail",
                crate::effect_aux::saber_rain_steam::CVAR,
                "cg_speedTrail",
                "cg_auraShell",
                "cg_spProtAbsColor",
                "cg_shieldSphere",
                "cg_shieldBrightness",
                "cg_remaps",
                crate::camera::STYLE_CVAR,
                "cg_drawGun",
                "cg_thirdPersonCameraDamp",
                "cg_thirdPersonTargetDamp",
                "cg_errorDecay",
                "mod_japlus",
                "mod_jof",
                crate::brush_view::CVAR,
                crate::entity_view::CVAR,
                crate::ghost_replay::SHOWCASE_CVAR,
            ],
            Self::Interface => &[
                crate::menu::style::CVAR,
                crate::rarity_fx::CVAR,
                crate::game_font::CVAR,
                crate::text::style::SCALE_CVAR,
                crate::text::style::TRACKING_CVAR,
                crate::console::console_options::STYLE_CVAR,
                "con_scale",
                crate::console::console_options::DRAW_NOTIFY_CVAR,
                crate::text::charset::CVAR,
                crate::quick_wheel::pages::FILE,
                crate::quick_wheel::SOUNDS_CVAR,
            ],
            Self::Hud => &[
                "cg_drawHud",
                crate::menu_hud::STYLE_CVAR,
                crate::menu_hud::FILES_CVAR,
                "cg_hudScale",
                "cg_classicHudFont",
                "cg_drawStatus",
                "cg_drawWeapon",
                "cg_crosshair",
                "cg_crosshairSize",
                "cg_hitMarker",
                "cg_drawCrosshairNames",
                "cg_drawPlayerNames",
                "cg_drawPlayerNamesScale",
                "cg_drawFriend",
                "cg_playerCard",
                "cg_playerCardDelay",
                "cg_playerCardOutline",
                crate::hud::team_outline::CVAR,
                "cg_nameplate",
                "cg_nameplateRange",
                "cg_nameplateNear",
                "cg_nameplateScale",
                "cg_nameplateBars",
                "cg_nameplateForce",
                "cg_nameplateShare",
                "cg_nameplateEstimate",
                "cg_nameplateWeapon",
                "cg_nameplateSelf",
                "cg_nameplateIcons",
                "cg_nameplateWalls",
                "cg_nameplateNpcs",
                crate::world_ping::SHOW_CVAR,
                crate::world_ping::SOUND_CVAR,
                "cg_drawFps",
                "cg_drawTimer",
                "cg_drawTimerUpperRight",
                crate::version_overlay::CVAR,
                "cg_speedometer",
                crate::hud::movement::LINES_ROW,
                "cg_lagometer",
                "cg_lagometerScale",
                "cg_killfeed",
                crate::ground_hud::CVAR,
                "cg_drawChat",
                crate::chat::emoji::CVAR,
                crate::chat::reply::REMEMBER_CVAR,
                "cg_chatBoxLetterSpacing",
            ],
            Self::Scoreboard => &[
                crate::scoreboard::style::CVAR,
                crate::scoreboard::style::COMPACT_CVAR,
                crate::scoreboard::style::FPS_CVAR,
                "cg_showClientIDs",
                crate::jof_tag::CVAR,
                "cg_drawScoreboardIcons",
                "cg_smallScoreboard",
            ],
            Self::Display => &[
                "r_resolution",
                "r_fullscreen",
                "r_vsync",
                "com_maxfps",
                crate::gameplay_fps::CVAR,
                crate::server_caps::OVERRIDE_CVAR,
                crate::runtime_settings::MONITOR_CAP_CVAR,
                "cg_fov",
                "r_gamma",
            ],
            Self::Network => &[
                "cl_autoUpdate",
                "cl_identity",
                super::catalog::IDENTITY_ROW,
                crate::installed_models::CVAR,
                "cl_hubUrl",
                crate::crash_report::CVAR,
                crate::settings_usage::CVAR,
                "rate",
                "snaps",
                "cl_master",
                "cl_master2",
            ],
            Self::Simple(Simple::Display) => &[
                "r_resolution",
                "r_fullscreen",
                "r_vsync",
                "com_maxfps",
                "cg_fov",
                "r_gamma",
            ],
            Self::Simple(Simple::Graphics) => &[
                crate::graphics_quality::RENDERER_ROW,
                crate::graphics_quality::ROW_NAME,
                crate::weather::CVAR,
                "r_DynamicGlow",
                "cg_marks",
                "cg_shadows",
            ],
            Self::Simple(Simple::Sound) => &["s_volume", "s_musicVolume", "snd_mute_losefocus"],
            Self::Simple(Simple::Controls) => &["sensitivity", "m_invert", "cl_run"],
            Self::Simple(Simple::Gameplay) => &[
                crate::camera::STYLE_CVAR,
                crate::gameplay_fps::CVAR,
                "cg_drawGun",
                "cg_saberTrail",
                "cg_forceModel",
                "cg_auraShell",
            ],
            Self::Simple(Simple::Hud) => &[
                crate::menu_hud::STYLE_CVAR,
                "cg_hudScale",
                "cg_crosshair",
                "cg_crosshairSize",
                "cg_hitMarker",
                "cg_nameplate",
                "cg_drawFriend",
                crate::hud::team_outline::CVAR,
                "cg_drawFps",
                "cg_speedometer",
                "cg_killfeed",
                "cg_drawChat",
            ],
            Self::Simple(Simple::General) => &[
                crate::menu::style::CVAR,
                crate::text::style::SCALE_CVAR,
                super::catalog::IMPORT_ROW,
                "cl_autoUpdate",
                "cl_sjkChat",
                crate::crash_report::CVAR,
                crate::settings_usage::CVAR,
                super::catalog::IDENTITY_ROW,
            ],
            Self::Advanced(Advanced::Chat) => CHAT_ROWS,
            Self::Advanced(Advanced::Mods) => MODS_ROWS,
            // Their rows are taken from the classic groups ([`Self::cvar_list`]).
            Self::Advanced(_) | Self::Quick | Self::Graphics => &[],
        }
    }

    /// The sub-headings that divide the group's rows: each heading stands
    /// before the row of its cvar.
    fn headings(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::GameOptions => &[
                ("assetbrowser", "Asset packs"),
                ("cg_simpleItems", "Items and models"),
                ("cg_saberTrail", "Effects"),
                (crate::camera::STYLE_CVAR, "Camera and prediction"),
                (crate::brush_view::CVAR, "Map tools"),
            ],
            Self::Interface => &[
                (crate::menu::style::CVAR, "Menus"),
                (crate::console::console_options::STYLE_CVAR, "Console"),
                (crate::quick_wheel::pages::FILE, "Quick wheel"),
            ],
            Self::Hud => &[
                ("cg_drawHud", "Layout"),
                ("cg_crosshair", "Crosshair"),
                ("cg_drawCrosshairNames", "Names and cards"),
                ("cg_nameplate", "Nameplates"),
                (crate::world_ping::SHOW_CVAR, "World pings"),
                ("cg_drawFps", "Readouts"),
                ("cg_drawChat", "Chat"),
            ],
            Self::Display => &[
                ("r_resolution", "Screen"),
                ("com_maxfps", "Frame rate"),
                ("cg_fov", "View"),
            ],
            Self::Network => &[("cl_autoUpdate", "SJK online"), ("rate", "Connection")],
            Self::Advanced(Advanced::Sound) => &[
                ("s_volume", "Game"),
                (crate::emotes::VOLUME_CVAR, "Emotes"),
                (crate::unlock_toast::SOUND_CVAR, "Notifications"),
            ],
            Self::Advanced(Advanced::Gameplay) => &[
                ("cg_simpleItems", "Items and models"),
                ("cg_saberTrail", "Effects"),
                (crate::camera::STYLE_CVAR, "Camera and prediction"),
            ],
            Self::Advanced(Advanced::Interface) => &[
                (crate::menu::style::CVAR, "Menus"),
                (crate::console::console_options::STYLE_CVAR, "Console"),
            ],
            Self::Advanced(Advanced::Hud) => &[
                ("cg_drawHud", "Layout"),
                ("cg_crosshair", "Crosshair"),
                ("cg_drawCrosshairNames", "Names and cards"),
                ("cg_nameplate", "Nameplates"),
                (crate::world_ping::SHOW_CVAR, "World pings"),
                ("cg_drawFps", "Readouts"),
                (crate::scoreboard::style::CVAR, "Scoreboard"),
            ],
            Self::Advanced(Advanced::Chat) => {
                &[("cg_drawChat", "Chat box"), ("cl_sjkChat", "SJK chat")]
            }
            Self::Advanced(Advanced::Mods) => &[
                ("assetbrowser", "Packs"),
                ("mod_japlus", "Server mods"),
                (crate::brush_view::CVAR, "Map tools"),
                (crate::ghost_replay::SHOWCASE_CVAR, "Experimental"),
            ],
            Self::Simple(Simple::Graphics) => &[
                (crate::graphics_quality::RENDERER_ROW, "Renderer"),
                (crate::weather::CVAR, "Effects"),
            ],
            Self::Simple(Simple::Hud) => &[
                (crate::menu_hud::STYLE_CVAR, "HUD"),
                ("cg_crosshair", "Crosshair"),
                ("cg_nameplate", "Over the game"),
            ],
            Self::Simple(Simple::General) => &[
                (crate::menu::style::CVAR, "Menus"),
                (super::catalog::IMPORT_ROW, "From another client"),
                ("cl_autoUpdate", "SJK online"),
            ],
            // Its headings are its tabs' names ([`graphics_tabs`]).
            Self::Scoreboard | Self::Graphics | Self::Simple(_) => &[],
            Self::Quick => &[
                (super::catalog::IMPORT_ROW, "From another client"),
                (crate::graphics_quality::RENDERER_ROW, "Graphics"),
                (crate::menu::style::CVAR, "Styles"),
                ("r_resolution", "Display"),
                ("sensitivity", "Aim"),
                ("s_volume", "Sound"),
                (crate::menu_hud::STYLE_CVAR, "HUD"),
                ("cg_nameplate", "Nameplates"),
                ("cg_auraShell", "Force shells"),
                ("cl_autoUpdate", "SJK"),
            ],
        }
    }

    /// The lines a classic panel shows for the group: its rows, under their
    /// sub-headings.
    pub(super) fn lines(self) -> Vec<super::Line> {
        if self == Self::Graphics {
            let mut lines = Vec::with_capacity(self.rows().len() + 5);
            lines.push(super::Line::Heading(GRAPHICS_LEAD_HEADING));
            lines.extend((0..GRAPHICS_LEAD.len()).map(super::Line::Row));
            let mut row = GRAPHICS_LEAD.len();
            for (heading, tab) in graphics_tabs() {
                lines.push(super::Line::Heading(heading));
                lines.extend((row..row + tab.len()).map(super::Line::Row));
                row += tab.len();
            }
            return lines;
        }
        let rows = self.rows();
        let mut lines = Vec::with_capacity(rows.len() + self.headings().len());
        for (row, setting) in rows.iter().enumerate() {
            if let Some((_, heading)) = self
                .headings()
                .iter()
                .find(|(cvar, _)| cvar.eq_ignore_ascii_case(setting.cvar))
            {
                lines.push(super::Line::Heading(heading));
            }
            lines.push(super::Line::Row(row));
        }
        lines
    }

    /// The group's settings, looked up in the catalogue once.
    pub(super) fn rows(self) -> &'static [Setting] {
        if self == Self::Quick {
            return super::quick::rows();
        }
        static ROWS: [OnceLock<Vec<Setting>>; Group::COUNT] =
            [const { OnceLock::new() }; Group::COUNT];
        ROWS[self.index()].get_or_init(|| {
            if self == Self::Graphics {
                return GRAPHICS_LEAD
                    .iter()
                    .filter_map(|cvar| find(cvar))
                    .chain(
                        graphics_tabs()
                            .iter()
                            .flat_map(|(_, tab)| tab.iter().copied()),
                    )
                    .collect();
            }
            self.cvar_list()
                .iter()
                .filter_map(|cvar| find(cvar))
                .collect()
        })
    }

    /// Every group of rows the SJK UI's Settings shows, Simple's and the
    /// Advanced ones that are not one of [`Group::ALL`].
    #[cfg(test)]
    pub(super) fn sjk_only() -> impl Iterator<Item = Self> {
        [Self::Graphics, Self::Display, Self::Network]
            .into_iter()
            .chain(Simple::ALL.map(Self::Simple))
            .chain(Advanced::ALL.map(Self::Advanced))
    }

    /// The groups of rows Advanced's rail shows, First setup's aside.
    #[cfg(test)]
    pub(super) fn advanced_rail() -> [Self; 9] {
        [
            Self::Display,
            Self::Graphics,
            Self::Advanced(Advanced::Sound),
            Self::Advanced(Advanced::Gameplay),
            Self::Advanced(Advanced::Interface),
            Self::Advanced(Advanced::Hud),
            Self::Advanced(Advanced::Chat),
            Self::Advanced(Advanced::Mods),
            Self::Network,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_cvar_is_a_setting() {
        for group in Group::ALL
            .into_iter()
            .chain(Group::sjk_only())
            .filter(|group| *group != Group::Graphics)
        {
            assert_eq!(group.rows().len(), group.cvar_list().len(), "{group:?}");
        }
    }

    #[test]
    fn display_and_network_hold_their_tabs_rows_but_the_renderers() {
        let tab = |caption: &str| {
            super::super::settings(TABS.iter().position(|tab| *tab == caption).unwrap())
        };
        let shown = |group: Group| -> Vec<&str> {
            group.rows().iter().map(|setting| setting.cvar).collect()
        };
        let display = shown(Group::Display);
        for setting in tab("VIDEO") {
            let in_graphics = GRAPHICS_LEAD.contains(&setting.cvar);
            assert_eq!(
                display.contains(&setting.cvar),
                !in_graphics,
                "{}",
                setting.cvar
            );
        }
        // Network: the tab's rows, but the SJK chat's (Chat) and its sound (Sound).
        let network = shown(Group::Network);
        let mut expected: Vec<&str> = tab("NETWORK")
            .iter()
            .map(|setting| setting.cvar)
            .filter(|cvar| !CHAT_ROWS.contains(cvar) && !NOTIFICATION_SOUNDS.contains(cvar))
            .collect();
        let mut found = network.clone();
        expected.sort_unstable();
        found.sort_unstable();
        assert_eq!(found, expected, "Network holds the tab's rows, once each");
    }

    /// Every setting is on exactly one of Advanced's categories (First setup's
    /// rows are its others'): the groups of rows, the Controls tab, and the
    /// quick wheel's pages, which its own category edits.
    #[test]
    fn advanced_shows_every_setting_once() {
        let mut shown: Vec<&str> = Group::advanced_rail()
            .iter()
            .flat_map(|group| group.rows())
            .map(|setting| setting.cvar)
            .collect();
        let controls =
            super::super::settings(TABS.iter().position(|tab| *tab == "CONTROLS").unwrap());
        shown.extend(controls.iter().map(|setting| setting.cvar));
        shown.push(crate::quick_wheel::pages::FILE);
        let every = (0..TABS.len())
            .filter(|tab| *tab != QUICK_TAB)
            .flat_map(super::super::settings)
            .chain(
                [
                    RENDER_IMAGE,
                    RENDER_LIGHTING,
                    RENDER_SHADOWS,
                    RENDER_WEATHER,
                ]
                .into_iter()
                .flatten(),
            );
        for setting in every {
            let count = shown.iter().filter(|cvar| **cvar == setting.cvar).count();
            assert_eq!(count, 1, "{} is shown {count} times", setting.cvar);
        }
        // Only First setup's own rows are First setup's alone.
        for setting in Group::Quick.rows() {
            let own = [
                super::super::catalog::IMPORT_ROW,
                super::super::quick::HIDE_CVAR,
            ];
            assert!(
                own.contains(&setting.cvar) || shown.contains(&setting.cvar),
                "{} is only in First setup",
                setting.cvar
            );
        }
    }

    #[test]
    fn simple_mode_keeps_its_categories_short() {
        for simple in Simple::ALL {
            let rows = Group::Simple(simple).rows();
            assert!(
                (3..=12).contains(&rows.len()),
                "{simple:?}: {} rows",
                rows.len()
            );
        }
        // The renderer first: the choice that matters most.
        assert_eq!(
            Group::Simple(Simple::Graphics).rows()[0].cvar,
            crate::graphics_quality::RENDERER_ROW
        );
    }

    #[test]
    fn every_heading_names_a_row_of_its_group_and_the_first_row_is_under_one() {
        for group in Group::ALL
            .into_iter()
            .chain([Group::Quick])
            .chain(Group::sjk_only().filter(|group| *group != Group::Graphics))
        {
            let rows = group.rows();
            for (cvar, heading) in group.headings() {
                assert!(
                    rows.iter()
                        .any(|setting| setting.cvar.eq_ignore_ascii_case(cvar)),
                    "{group:?}: {heading} names {cvar}, not one of its rows"
                );
            }
            let lines = group.lines();
            assert_eq!(
                lines.len(),
                rows.len() + group.headings().len(),
                "{group:?}"
            );
            if !group.headings().is_empty() {
                assert!(
                    matches!(lines[0], super::super::Line::Heading(_)),
                    "{group:?}"
                );
            }
        }
    }

    #[test]
    fn graphics_leads_with_the_renderer_then_holds_every_renderer_row_once_under_its_tabs_name() {
        let rows = Group::Graphics.rows();
        let renderer: Vec<&str> = GRAPHICS_LEAD
            .iter()
            .copied()
            .chain(
                graphics_tabs()
                    .iter()
                    .flat_map(|(_, tab)| tab.iter().map(|setting| setting.cvar)),
            )
            .collect();
        assert_eq!(
            rows.iter().map(|setting| setting.cvar).collect::<Vec<_>>(),
            renderer
        );
        let lines = Group::Graphics.lines();
        let headings: Vec<&str> = lines
            .iter()
            .filter_map(|line| match line {
                super::super::Line::Heading(heading) => Some(*heading),
                super::super::Line::Row(_) => None,
            })
            .collect();
        assert_eq!(
            headings,
            ["Quality", "Image", "Lighting", "Shadows", "Weather"]
        );
        assert_eq!(lines.len(), rows.len() + 5);
        // Every row once, in order.
        let shown: Vec<usize> = lines
            .iter()
            .filter_map(|line| match line {
                super::super::Line::Row(row) => Some(*row),
                super::super::Line::Heading(_) => None,
            })
            .collect();
        assert_eq!(shown, (0..rows.len()).collect::<Vec<_>>());
    }

    #[test]
    fn the_groups_hold_the_game_hud_and_text_tabs_once_each() {
        let grouped: Vec<&str> = Group::ALL
            .iter()
            .flat_map(|group| group.rows())
            .map(|setting| setting.cvar)
            .collect();
        for caption in ["GAME", "HUD", "HUD+", "TEXT"] {
            let tab = TABS.iter().position(|tab| *tab == caption).unwrap();
            for setting in super::super::settings(tab) {
                let count = grouped.iter().filter(|cvar| **cvar == setting.cvar).count();
                assert_eq!(count, 1, "{} ({caption})", setting.cvar);
            }
        }
    }
}
