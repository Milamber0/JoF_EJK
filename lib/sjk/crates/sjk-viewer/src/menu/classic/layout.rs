//! Pages, entries and geometry of the classic main menu, taken from the
//! retail multiplayer menus (`ui/jamp/main.menu`, `multiplayer.menu`,
//! `controls.menu`, `setup.menu`, `quit.menu`): which entries each page has,
//! in which order, where they sit on the original 640x480 menu canvas, and
//! where each one leads. The page tables themselves are in [`super::pages`].
//!
//! Positions are the retail item rectangles reduced to a text centre; labels
//! are the retail words. Hints and titles are SJK's own text.

use crate::keybind_editor::Category;
use crate::menu::destination::MainDestination;
use crate::settings::Group;
use sjk_ui::{Rect, TextAlign};

/// Size of the canvas the retail menus are authored on.
pub(crate) const CANVAS: [f32; 2] = [640.0, 480.0];
/// Vertical centre of the description line under the entries (retail
/// `descY` 424, plus half a line).
pub(crate) const HINT_Y: f32 = 432.0;
/// Area of the retail game logo at the top of every page.
pub(crate) const LOGO: [f32; 4] = [107.0, 8.0, 428.0, 112.0];

/// One screen of the classic main menu.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Page {
    /// The opening menu: Play, Profile, Settings, SJK and Exit.
    Main,
    /// Retail "multiplayer" menu behind Play: solo, join or create a game.
    Play,
    /// Settings' KEY BINDINGS tab (retail's "controls" menu): every binding
    /// in one list, its categories down the left.
    Controls,
    /// Settings' OPTIONS tab (retail's "setup" menu): first setup, and the
    /// graphics, sound and gameplay options.
    Setup,
    /// SJK's own page: the changelog, the credits and updates.
    Sjk,
    /// Setup's GRAPHICS, a classic+ page in the retail setup layout (its
    /// groups down the left, the panel beside): video, the renderer's image,
    /// lighting and shadows, and the weather.
    Graphics,
    /// Setup's GAMEPLAY, laid out as [`Page::Graphics`]: mouse, game options,
    /// interface, HUD, scoreboard and network.
    Gameplay,
    /// Retail quit confirmation behind Exit and Escape.
    Quit,
}

/// One selectable entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Entry {
    Play,
    Profile,
    /// SJK: retail's Controls and Setup as one Settings, with two tabs.
    Settings,
    /// SJK: the SJK page (changelog, credits, update).
    Sjk,
    Exit,
    /// SJK: the changelog page.
    Changelog,
    /// SJK: the credits page.
    Credits,
    /// SJK: the update page.
    Update,
    /// SJK: the identity page (name, bio, the SJK hub).
    Identity,
    /// SJK: the Profile page.
    SjkProfile,
    SoloGame,
    JoinServer,
    CreateServer,
    PlayDemo,
    Rules,
    Movement,
    Interaction,
    Weapons,
    /// Retail's Force Powers 1 and 2 pages, one group in classic+.
    ForcePowers,
    MouseJoystick,
    OtherControls,
    /// Retail's Video and More Video, one group in classic+.
    Video,
    Sound,
    GameOptions,
    /// SJK: the menus' and console's look.
    Interface,
    Hud,
    /// SJK: the scoreboard (the HUD+ settings regrouped).
    Scoreboard,
    /// SJK: the settings worth choosing on a first start.
    FirstSetup,
    Network,
    /// SJK: Setup's two sub-pages.
    Graphics,
    Gameplay,
    /// The renderer settings' groups, on the graphics page.
    RenderImage,
    RenderLighting,
    RenderShadows,
    /// SJK: rain, fog and clouds (the renderer settings' WEATHER tab).
    Weather,
    /// A sub-page's Back, to the setup options.
    SetupBack,
    Back,
    No,
    Yes,
}

/// Label size class: the main page's big buttons, the smaller ones of the
/// navigation row and centre lists, and the option lists of Controls and
/// Setup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Size {
    Large,
    Medium,
    List,
}

impl Size {
    /// Label height on the 640x480 canvas.
    pub(crate) fn text(self) -> f32 {
        match self {
            Self::Large => 22.0,
            Self::Medium => 17.0,
            Self::List => 14.0,
        }
    }
}

/// One entry placed on a page.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Slot {
    pub(crate) entry: Entry,
    pub(crate) label: &'static str,
    /// The description line shown while the entry has focus; for an entry
    /// SJK cannot open yet, the note saying so.
    pub(crate) hint: &'static str,
    /// Centre of the target on the 640x480 canvas.
    pub(crate) center: [f32; 2],
    /// Width of the pointer target and focus glow on the canvas: the
    /// retail item width.
    pub(crate) width: f32,
    /// Height of the pointer target and focus glow on the canvas.
    pub(crate) height: f32,
    pub(crate) size: Size,
    /// Label alignment inside the target: centred buttons, or the option
    /// lists' labels set against their right edge as retail does.
    pub(crate) align: TextAlign,
}

impl Slot {
    /// Pointer target and focus glow on the 640x480 canvas.
    pub(crate) fn target(&self) -> [f32; 4] {
        [
            self.center[0] - self.width * 0.5,
            self.center[1] - self.height * 0.5,
            self.width,
            self.height,
        ]
    }

    /// Whether activating the entry does something; the others are drawn
    /// dimmed with their note as the hint.
    pub(crate) fn enabled(&self) -> bool {
        self.entry.outcome() != Outcome::Unavailable
    }
}

impl Page {
    /// The page's entries in focus order.
    pub(crate) fn slots(self) -> &'static [Slot] {
        super::pages::slots(self)
    }

    /// Entry focused when the page opens: the first entry of the page's own
    /// list, as retail sets focus; the quit page starts on No.
    pub(crate) fn initial_selection(self) -> usize {
        let entry = match self {
            Self::Main => Entry::Play,
            Self::Play => Entry::SoloGame,
            Self::Controls => Entry::Movement,
            Self::Setup => Entry::FirstSetup,
            Self::Sjk => Entry::Changelog,
            Self::Graphics => Entry::Video,
            Self::Gameplay => Entry::MouseJoystick,
            Self::Quit => Entry::No,
        };
        self.index_of(entry).unwrap_or(0)
    }

    /// Position of `entry` on this page.
    pub(crate) fn index_of(self, entry: Entry) -> Option<usize> {
        self.slots().iter().position(|slot| slot.entry == entry)
    }

    /// Page heading and its vertical centre on the canvas.
    pub(crate) fn title(self) -> (&'static str, f32) {
        match self {
            Self::Main => ("MULTIPLAYER", 132.0),
            Self::Play => ("START PLAYING", 172.0),
            Self::Controls => ("KEY BINDINGS", 172.0),
            Self::Setup => ("OPTIONS", 172.0),
            Self::Sjk => ("SOL JK", 172.0),
            Self::Graphics => ("GRAPHICS", 172.0),
            Self::Gameplay => ("GAMEPLAY", 172.0),
            Self::Quit => ("QUIT", 172.0),
        }
    }

    /// Where Escape leads: the main page asks to quit, as retail does;
    /// Setup's sub-pages return to Setup, every other page to the main page.
    pub(crate) fn escape(self) -> Page {
        match self {
            Self::Main => Self::Quit,
            Self::Graphics | Self::Gameplay => Self::Setup,
            _ => Self::Main,
        }
    }

    /// One of Setup's sub-pages, whose Back and Escape return to Setup.
    pub(crate) fn is_setup_child(self) -> bool {
        matches!(self, Self::Graphics | Self::Gameplay)
    }
}

/// What activating an entry does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    /// Show another classic page.
    Page(Page),
    /// Leave the main menu for a screen it opens.
    Open(MainDestination),
    /// Open the settings screen on the tab with this caption.
    Settings(&'static str),
    /// Open the key-binding editor on this category.
    Keybinds(Category),
    /// A retail screen SJK has no equivalent for yet: nothing happens.
    Unavailable,
}

impl Entry {
    /// What activating this entry does.
    pub(crate) fn outcome(self) -> Outcome {
        match self {
            Self::Play => Outcome::Page(Page::Play),
            Self::Settings => Outcome::Page(Page::Setup),
            Self::Sjk => Outcome::Page(Page::Sjk),
            Self::Exit => Outcome::Page(Page::Quit),
            Self::Changelog => Outcome::Open(MainDestination::Changelog),
            Self::Credits => Outcome::Open(MainDestination::Credits),
            Self::Update => Outcome::Open(MainDestination::Update),
            Self::Identity => Outcome::Open(MainDestination::Identity),
            Self::SjkProfile => Outcome::Open(MainDestination::Profile),
            Self::Back | Self::No => Outcome::Page(Page::Main),
            Self::SetupBack => Outcome::Page(Page::Setup),
            Self::Profile => Outcome::Open(MainDestination::Player),
            Self::JoinServer => Outcome::Open(MainDestination::Browser),
            // Retail's Solo Game is a local match with bots, which is what
            // Create game hosts.
            Self::SoloGame | Self::CreateServer => Outcome::Open(MainDestination::CreateGame),
            Self::Yes => Outcome::Open(MainDestination::Quit),
            Self::Movement => Outcome::Keybinds(Category::Movement),
            Self::Interaction => Outcome::Keybinds(Category::Interaction),
            Self::Weapons => Outcome::Keybinds(Category::Weapons),
            Self::ForcePowers => Outcome::Keybinds(Category::Force),
            Self::OtherControls => Outcome::Keybinds(Category::Other),
            Self::MouseJoystick => Outcome::Settings("CONTROLS"),
            Self::Video => Outcome::Settings("VIDEO"),
            Self::Sound => Outcome::Settings("AUDIO"),
            Self::GameOptions => Outcome::Settings("GAME"),
            Self::Interface => Outcome::Settings("TEXT"),
            Self::Hud => Outcome::Settings("HUD"),
            Self::Scoreboard => Outcome::Settings("HUD+"),
            Self::FirstSetup => Outcome::Settings(crate::settings::FIRST_SETUP_CAPTION),
            Self::Network => Outcome::Settings("NETWORK"),
            Self::Graphics => Outcome::Page(Page::Graphics),
            Self::Gameplay => Outcome::Page(Page::Gameplay),
            // Each opens its panel ([`Entry::panel`]); the renderer settings otherwise.
            Self::RenderImage | Self::RenderLighting | Self::RenderShadows | Self::Weather => {
                Outcome::Open(MainDestination::Renderer)
            }
            Self::PlayDemo | Self::Rules => Outcome::Unavailable,
        }
    }
}

/// Rows of an option group, as offsets into its settings tab or key-binding
/// category: `start..end`, with `end` clamped to the group's length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Span {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

impl Span {
    /// Every row of the group.
    pub(crate) const ALL: Self = Self {
        start: 0,
        end: usize::MAX,
    };

    /// The absolute rows of this span inside a group of `len` rows that
    /// starts at row `base`.
    pub(crate) fn within(self, base: usize, len: usize) -> std::ops::Range<usize> {
        let start = self.start.min(len);
        base + start..base + self.end.clamp(start, len)
    }
}

/// The option group a Setup or Controls entry shows in the classic option
/// panel, as retail's `setup.menu` and `controls.menu` show a group of items
/// beside their list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Panel {
    /// Rows of the settings tab with this caption.
    Settings { caption: &'static str, span: Span },
    /// Rows of a key-binding category.
    Keybinds { category: Category, span: Span },
    /// Every row of renderer settings tab `tab` (IMAGE, LIGHTING, SHADOWS,
    /// WEATHER).
    Renderer { tab: usize },
    /// A classic Setup group gathering rows of several settings tabs.
    Group(Group),
}

impl Entry {
    /// The option group this entry shows in the classic panel; `None` for
    /// entries that are not option groups, or that SJK cannot show yet.
    ///
    /// Retail split video and the Force binds over two pages each because a
    /// page held few items; classic+ panels scroll and explain the focused
    /// item, so SJK shows each as one group, and regroups the GAME, HUD,
    /// HUD+ and TEXT tabs by subject ([`Group`]).
    pub(crate) fn panel(self) -> Option<Panel> {
        let settings = |caption| Panel::Settings {
            caption,
            span: Span::ALL,
        };
        let keybinds = |category| Panel::Keybinds {
            category,
            span: Span::ALL,
        };
        Some(match self {
            Self::Video => settings("VIDEO"),
            Self::Sound => settings("AUDIO"),
            Self::GameOptions => Panel::Group(Group::GameOptions),
            Self::Interface => Panel::Group(Group::Interface),
            Self::Hud => Panel::Group(Group::Hud),
            Self::Scoreboard => Panel::Group(Group::Scoreboard),
            Self::FirstSetup => Panel::Group(Group::Quick),
            Self::Network => settings("NETWORK"),
            Self::MouseJoystick => settings("CONTROLS"),
            Self::Movement => keybinds(Category::Movement),
            Self::Interaction => keybinds(Category::Interaction),
            Self::Weapons => keybinds(Category::Weapons),
            Self::ForcePowers => keybinds(Category::Force),
            Self::OtherControls => keybinds(Category::Other),
            Self::RenderImage => Panel::Renderer { tab: 0 },
            Self::RenderLighting => Panel::Renderer { tab: 1 },
            Self::RenderShadows => Panel::Renderer { tab: 2 },
            Self::Weather => Panel::Renderer { tab: 3 },
            _ => return None,
        })
    }
}

impl Entry {
    /// The settings icon (`settings_icons::ICONS`) beside this entry in the
    /// Settings group lists; `None` for entries drawn without one.
    pub(crate) fn icon(self) -> Option<&'static str> {
        Some(match self {
            Self::Movement => "movement",
            Self::Interaction => "interaction",
            Self::Weapons => "weapons",
            Self::ForcePowers => "force_powers",
            Self::OtherControls => "other_controls",
            Self::FirstSetup => "first_setup",
            Self::Graphics => "graphics",
            Self::Sound => "sound",
            Self::Gameplay => "gameplay",
            Self::Video => "video",
            Self::RenderImage => "image",
            Self::RenderLighting => "lighting",
            Self::RenderShadows => "shadows",
            Self::Weather => "weather",
            Self::MouseJoystick => "mouse_joystick",
            Self::GameOptions => "game_options",
            Self::Interface => "interface",
            Self::Hud => "hud",
            Self::Scoreboard => "scoreboard",
            Self::Network => "network",
            _ => return None,
        })
    }

    /// The KEY BINDINGS group of key-binding category `category`.
    pub(crate) fn of_category(category: usize) -> Option<Self> {
        [
            Self::Movement,
            Self::Interaction,
            Self::Weapons,
            Self::ForcePowers,
            Self::OtherControls,
        ]
        .get(category)
        .copied()
    }
}

impl Page {
    /// The Settings tab a panel page belongs to: KEY BINDINGS (0) or OPTIONS
    /// (1, Setup's sub-pages too); `None` outside Settings.
    pub(crate) fn settings_tab(self) -> Option<usize> {
        match self {
            Self::Controls => Some(0),
            Self::Setup | Self::Graphics | Self::Gameplay => Some(1),
            _ => None,
        }
    }

    /// The page of Settings tab `tab` (see [`Self::settings_tab`]).
    pub(crate) fn of_settings_tab(tab: usize) -> Self {
        if tab == 0 {
            Self::Controls
        } else {
            Self::Setup
        }
    }

    /// The group a panel page shows when it opens, as retail's `onOpen`
    /// shows Video and Movement; `None` for pages without a panel.
    pub(crate) fn opening_panel(self) -> Option<Entry> {
        match self {
            Self::Setup => Some(Entry::FirstSetup),
            Self::Controls => Some(Entry::Movement),
            Self::Graphics => Some(Entry::Video),
            Self::Gameplay => Some(Entry::MouseJoystick),
            _ => None,
        }
    }
}

/// The 640x480 canvas fitted into the window: scaled to its height (or
/// width, on a portrait window) and centred, so wide screens keep the
/// retail proportions instead of stretching them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placement {
    pub(crate) origin: [f32; 2],
    pub(crate) scale: f32,
}

impl Placement {
    pub(crate) fn new(viewport: [f32; 2]) -> Self {
        let scale = (viewport[0] / CANVAS[0]).min(viewport[1] / CANVAS[1]);
        Self {
            origin: [
                (viewport[0] - CANVAS[0] * scale) * 0.5,
                (viewport[1] - CANVAS[1] * scale) * 0.5,
            ],
            scale,
        }
    }

    /// Window rectangle of a canvas rectangle `[x, y, width, height]`.
    pub(crate) fn rect(&self, [x, y, width, height]: [f32; 4]) -> Rect {
        Rect::new(
            self.origin[0] + x * self.scale,
            self.origin[1] + y * self.scale,
            width * self.scale,
            height * self.scale,
        )
    }

    /// Window rectangle `width` canvas units wide and `height` high, centred
    /// on canvas point `center`.
    pub(crate) fn centered(&self, center: [f32; 2], width: f32, height: f32) -> Rect {
        self.rect([
            center[0] - width * 0.5,
            center[1] - height * 0.5,
            width,
            height,
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SettingsMenu;

    const PAGES: [Page; 8] = [
        Page::Main,
        Page::Play,
        Page::Controls,
        Page::Setup,
        Page::Graphics,
        Page::Gameplay,
        Page::Sjk,
        Page::Quit,
    ];

    fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
        a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
    }

    fn entries(page: Page) -> Vec<Entry> {
        page.slots().iter().map(|slot| slot.entry).collect()
    }

    #[test]
    fn pages_keep_the_retail_entry_order() {
        assert_eq!(
            entries(Page::Main),
            [
                Entry::Play,
                Entry::Profile,
                Entry::Settings,
                Entry::Sjk,
                Entry::Exit
            ]
        );
        assert_eq!(
            entries(Page::Play)[4..9],
            [
                Entry::SoloGame,
                Entry::JoinServer,
                Entry::CreateServer,
                Entry::PlayDemo,
                Entry::Rules
            ]
        );
        assert_eq!(
            entries(Page::Controls)[4..9],
            [
                Entry::Movement,
                Entry::Interaction,
                Entry::Weapons,
                Entry::ForcePowers,
                Entry::OtherControls
            ]
        );
        assert_eq!(
            entries(Page::Setup)[4..8],
            [
                Entry::FirstSetup,
                Entry::Graphics,
                Entry::Sound,
                Entry::Gameplay
            ]
        );
        assert_eq!(
            entries(Page::Graphics)[4..9],
            [
                Entry::Video,
                Entry::RenderImage,
                Entry::RenderLighting,
                Entry::RenderShadows,
                Entry::Weather
            ]
        );
        assert_eq!(
            entries(Page::Gameplay)[4..10],
            [
                Entry::MouseJoystick,
                Entry::GameOptions,
                Entry::Interface,
                Entry::Hud,
                Entry::Scoreboard,
                Entry::Network
            ]
        );
        assert_eq!(
            entries(Page::Sjk)[4..9],
            [
                Entry::Changelog,
                Entry::Credits,
                Entry::Update,
                Entry::SjkProfile,
                Entry::Identity
            ]
        );
        // Every sub-page's navigation row: Play, Profile, Settings, SJK.
        for page in [
            Page::Play,
            Page::Controls,
            Page::Setup,
            Page::Sjk,
            Page::Quit,
        ] {
            assert_eq!(
                entries(page)[..4],
                [Entry::Play, Entry::Profile, Entry::Settings, Entry::Sjk]
            );
        }
    }

    #[test]
    fn targets_fit_the_canvas_without_overlapping() {
        for page in PAGES {
            let slots = page.slots();
            for (index, slot) in slots.iter().enumerate() {
                let [x, y, width, height] = slot.target();
                assert!(x >= 0.0 && y >= 0.0, "{page:?} {:?}", slot.entry);
                assert!(x + width <= CANVAS[0] && y + height <= CANVAS[1]);
                for other in &slots[index + 1..] {
                    assert!(
                        !overlaps(slot.target(), other.target()),
                        "{page:?}: {:?} overlaps {:?}",
                        slot.entry,
                        other.entry
                    );
                }
            }
        }
    }

    #[test]
    fn pages_open_on_an_enabled_entry() {
        for page in PAGES {
            assert!(page.slots()[page.initial_selection()].enabled(), "{page:?}");
        }
        assert_eq!(
            Page::Quit.slots()[Page::Quit.initial_selection()].entry,
            Entry::No
        );
    }

    #[test]
    fn only_yes_quits() {
        for page in PAGES {
            for slot in page.slots() {
                let quits = slot.entry.outcome() == Outcome::Open(MainDestination::Quit);
                assert_eq!(quits, slot.entry == Entry::Yes, "{:?}", slot.entry);
            }
        }
        assert_eq!(Page::Main.escape(), Page::Quit);
        for page in [
            Page::Play,
            Page::Controls,
            Page::Setup,
            Page::Sjk,
            Page::Quit,
        ] {
            assert_eq!(page.escape(), Page::Main);
        }
        for page in [Page::Graphics, Page::Gameplay] {
            assert_eq!(page.escape(), Page::Setup);
            assert!(page.is_setup_child());
        }
    }

    #[test]
    fn settings_entries_name_real_tabs() {
        for page in PAGES {
            for slot in page.slots() {
                if let Outcome::Settings(caption) = slot.entry.outcome() {
                    assert!(
                        SettingsMenu::tab_index(caption).is_some(),
                        "{:?} names missing tab {caption}",
                        slot.entry
                    );
                }
            }
        }
    }

    #[test]
    fn unavailable_entries_say_so() {
        for page in PAGES {
            for slot in page.slots() {
                assert!(!slot.hint.is_empty(), "{:?}", slot.entry);
                assert_eq!(
                    !slot.enabled(),
                    slot.hint.starts_with("Not in SJK yet"),
                    "{:?}",
                    slot.entry
                );
            }
        }
    }

    #[test]
    fn main_page_reaches_every_page_and_screen() {
        let reachable: Vec<_> = PAGES
            .iter()
            .flat_map(|page| page.slots())
            .filter_map(|slot| match slot.entry.outcome() {
                Outcome::Open(destination) => Some(destination),
                _ => None,
            })
            .collect();
        for destination in [
            MainDestination::Browser,
            MainDestination::CreateGame,
            MainDestination::Player,
        ] {
            assert!(reachable.contains(&destination), "{destination:?}");
        }
        let pages: Vec<_> = Page::Main
            .slots()
            .iter()
            .filter_map(|slot| match slot.entry.outcome() {
                Outcome::Page(page) => Some(page),
                _ => None,
            })
            .collect();
        for page in [Page::Play, Page::Setup, Page::Sjk, Page::Quit] {
            assert!(pages.contains(&page), "{page:?}");
        }
        for destination in [
            MainDestination::Changelog,
            MainDestination::Credits,
            MainDestination::Update,
            MainDestination::Identity,
            MainDestination::Profile,
        ] {
            assert!(reachable.contains(&destination), "{destination:?}");
        }
    }

    #[test]
    fn every_group_has_a_panel_or_says_why_not() {
        for page in [Page::Setup, Page::Controls, Page::Graphics, Page::Gameplay] {
            assert!(
                page.opening_panel().and_then(Entry::panel).is_some(),
                "{page:?}"
            );
            for slot in page.slots().iter().filter(|slot| slot.size == Size::List) {
                // A group shows a panel; GRAPHICS and GAMEPLAY open their pages.
                let opens = slot.entry.panel().is_some()
                    || matches!(
                        slot.entry.outcome(),
                        Outcome::Page(Page::Graphics | Page::Gameplay)
                    );
                assert_eq!(opens, slot.enabled(), "{:?}", slot.entry);
                if let Some(Panel::Settings { caption, .. }) = slot.entry.panel() {
                    assert!(SettingsMenu::tab_index(caption).is_some(), "{caption}");
                }
            }
        }
        for page in [Page::Main, Page::Play, Page::Sjk, Page::Quit] {
            assert_eq!(page.opening_panel(), None);
        }
    }

    #[test]
    fn every_settings_group_has_its_own_icon() {
        let mut seen = Vec::new();
        for page in [Page::Setup, Page::Controls, Page::Graphics, Page::Gameplay] {
            for slot in page.slots().iter().filter(|slot| slot.size == Size::List) {
                let icon = slot.entry.icon().expect("a group icon");
                assert!(crate::settings_icons::texture(icon).is_some(), "{icon}");
                assert!(!seen.contains(&icon), "{icon} is used twice");
                seen.push(icon);
            }
        }
    }

    #[test]
    fn merged_groups_show_whole_tabs() {
        // Retail's Video / More Video and Force Powers 1 / 2 pairs are one
        // group each.
        assert_eq!(
            Entry::Video.panel(),
            Some(Panel::Settings {
                caption: "VIDEO",
                span: Span::ALL
            })
        );
        assert_eq!(
            Entry::ForcePowers.panel(),
            Some(Panel::Keybinds {
                category: Category::Force,
                span: Span::ALL
            })
        );
        let groups: Vec<_> = [Page::Setup, Page::Gameplay]
            .into_iter()
            .flat_map(entries)
            .filter_map(|entry| match entry.panel() {
                Some(Panel::Group(group)) => Some(group),
                _ => None,
            })
            .collect();
        let expected: Vec<_> = std::iter::once(Group::Quick).chain(Group::ALL).collect();
        assert_eq!(groups, expected);
    }

    #[test]
    fn spans_clamp_to_their_group() {
        assert_eq!(Span::ALL.within(10, 4), 10..14);
        assert_eq!(Span { start: 2, end: 9 }.within(10, 4), 12..14);
        assert_eq!(Span { start: 6, end: 9 }.within(10, 4), 14..14);
    }

    #[test]
    fn canvas_is_fitted_and_centred() {
        let wide = Placement::new([1920.0, 1080.0]);
        assert_eq!(wide.scale, 2.25);
        assert_eq!(wide.origin, [240.0, 0.0]);
        let tall = Placement::new([1280.0, 1024.0]);
        assert_eq!(tall.scale, 2.0);
        assert_eq!(tall.origin, [0.0, 32.0]);
        let rect = wide.rect([0.0, 0.0, 640.0, 480.0]);
        assert_eq!(
            (rect.x, rect.y, rect.width, rect.height),
            (240.0, 0.0, 1440.0, 1080.0)
        );
        let centered = wide.centered([320.0, 240.0], 40.0, 20.0);
        assert_eq!(
            (centered.x, centered.y),
            (240.0 + 300.0 * 2.25, 230.0 * 2.25)
        );
    }
}
