//! The SJK UI's Settings screen (`docs/sjk-ui.md`, Settings): the categories
//! of settings down one rail, opened from the main page's Settings, in one of
//! two modes ([`SettingsMode`]): Simple, the default, with the settings that
//! matter most in a few categories, or Advanced, with every setting; First
//! setup at start (and the `firstsetup` command) shows its category as a
//! pop-up instead ([`crate::settings::SettingsMenu::append_sjk_popup`]). The rows are the classic+ panels' groups
//! ([`crate::settings::SettingsMenu`] draws them in [`crate::settings::Rail`]'s
//! frame), and Key bindings the classic+ list of every binding
//! ([`crate::keybind_editor::KeybindEditor`] draws it in the same frame).

use super::super::classic::layout::{Entry, Page, Panel, Span};
use super::super::classic::panel::Frame as PanelFrame;
use super::super::{ClientMenu, ClientPhase, MenuAction};
use super::TextTarget;
use crate::console::ViewerConsole;
use crate::player_menu::ReturnTarget;
use crate::settings::mode::SettingsMode;
use crate::settings::{Advanced, Group, SettingsMenu, SettingsResult, Simple};

/// What a category of the rail shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Shows {
    /// A panel of settings rows.
    Rows(Panel),
    /// The key bindings: every action with its keys.
    Keys,
    /// The quick wheel's pages and their choices, edited
    /// ([`crate::settings::WheelMode::Category`]).
    Wheel,
}

/// One category: its name, its settings icon, what it shows, which modes'
/// rails it is on and, for one whose rows are regrouped from a tab, the
/// classic+ panel that tab is.
#[derive(Clone, Copy, Debug)]
struct Category {
    label: &'static str,
    icon: &'static str,
    shows: Shows,
    on: On,
    classic: Option<Panel>,
}

/// Which modes' rails a category is on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum On {
    Simple,
    Advanced,
    Both,
}

impl On {
    const fn has(self, mode: SettingsMode) -> bool {
        matches!(
            (self, mode),
            (Self::Both, _)
                | (Self::Simple, SettingsMode::Simple)
                | (Self::Advanced, SettingsMode::Advanced)
        )
    }
}

const fn tab(caption: &'static str) -> Shows {
    Shows::Rows(Panel::Settings {
        caption,
        span: Span::ALL,
    })
}

const fn group(group: Group) -> Shows {
    Shows::Rows(Panel::Group(group))
}

/// A category of Advanced's rail.
const fn advanced(label: &'static str, icon: &'static str, shows: Shows) -> Category {
    Category {
        label,
        icon,
        shows,
        on: On::Advanced,
        classic: None,
    }
}

/// A category of Simple's rail.
const fn simple(label: &'static str, icon: &'static str, simple: Simple) -> Category {
    Category {
        label,
        icon,
        shows: group(Group::Simple(simple)),
        on: On::Simple,
        classic: None,
    }
}

/// Every category: Advanced's rail, top to bottom, then Simple's own (Key
/// bindings is on both).
/// A category of Advanced's rail regrouping classic rows (`regrouped`), whose
/// classic+ panel is `classic`.
const fn regrouped(
    label: &'static str,
    icon: &'static str,
    regrouped: Advanced,
    classic: Panel,
) -> Category {
    Category {
        classic: Some(classic),
        ..advanced(label, icon, group(Group::Advanced(regrouped)))
    }
}

const CATEGORIES: [Category; 20] = [
    advanced("First setup", "first_setup", group(Group::Quick)),
    Category {
        classic: Some(Panel::Settings {
            caption: "VIDEO",
            span: Span::ALL,
        }),
        ..advanced("Display", "video", group(Group::Display))
    },
    advanced("Graphics", "graphics", group(Group::Graphics)),
    regrouped(
        "Sound",
        "sound",
        Advanced::Sound,
        Panel::Settings {
            caption: "AUDIO",
            span: Span::ALL,
        },
    ),
    advanced("Controls", "mouse_joystick", tab("CONTROLS")),
    Category {
        on: On::Both,
        ..advanced("Key bindings", "key_bindings", Shows::Keys)
    },
    regrouped(
        "Gameplay",
        "game_options",
        Advanced::Gameplay,
        Panel::Group(Group::GameOptions),
    ),
    regrouped(
        "Interface",
        "interface",
        Advanced::Interface,
        Panel::Group(Group::Interface),
    ),
    // With the scoreboard's rows under their own heading.
    regrouped("HUD", "hud", Advanced::Hud, Panel::Group(Group::Hud)),
    regrouped("Chat", "text", Advanced::Chat, Panel::Group(Group::Hud)),
    // The wheel's own icon, a ring of discs, from the wheel's second board.
    advanced(
        "Quick wheel",
        crate::quick_wheel::catalog::WHEEL_ICON,
        Shows::Wheel,
    ),
    regrouped(
        "Mods",
        "options",
        Advanced::Mods,
        Panel::Group(Group::GameOptions),
    ),
    Category {
        classic: Some(Panel::Settings {
            caption: "NETWORK",
            span: Span::ALL,
        }),
        ..advanced("Network", "network", group(Group::Network))
    },
    simple("Display", "video", Simple::Display),
    simple("Graphics", "graphics", Simple::Graphics),
    simple("Sound", "sound", Simple::Sound),
    simple("Controls", "mouse_joystick", Simple::Controls),
    simple("Gameplay", "game_options", Simple::Gameplay),
    simple("HUD", "hud", Simple::Hud),
    simple("General", "interface", Simple::General),
];

/// The categories on mode `mode`'s rail, top to bottom: Simple's keeps Key
/// bindings after Controls.
const fn rail_of(mode: SettingsMode) -> &'static [usize] {
    match mode {
        SettingsMode::Simple => &SIMPLE_RAIL,
        SettingsMode::Advanced => &ADVANCED_RAIL,
    }
}
const ADVANCED_RAIL: [usize; 13] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
const SIMPLE_RAIL: [usize; 8] = [13, 14, 15, 16, KEYS, 17, 18, 19];
const _: () = {
    let mut index = 0;
    while index < ADVANCED_RAIL.len() {
        assert!(
            CATEGORIES[ADVANCED_RAIL[index]]
                .on
                .has(SettingsMode::Advanced)
        );
        index += 1;
    }
    let mut index = 0;
    while index < SIMPLE_RAIL.len() {
        assert!(CATEGORIES[SIMPLE_RAIL[index]].on.has(SettingsMode::Simple));
        index += 1;
    }
};

/// The rail's names and icons, as the view takes them, for each mode.
const fn names<const N: usize>(rail: [usize; N]) -> [(&'static str, &'static str); N] {
    let mut names = [("", ""); N];
    let mut index = 0;
    while index < N {
        names[index] = (CATEGORIES[rail[index]].label, CATEGORIES[rail[index]].icon);
        index += 1;
    }
    names
}
const ADVANCED_NAMES: [(&str, &str); ADVANCED_RAIL.len()] = names(ADVANCED_RAIL);
const SIMPLE_NAMES: [(&str, &str); SIMPLE_RAIL.len()] = names(SIMPLE_RAIL);

/// The category First setup is.
pub(crate) const FIRST_SETUP: usize = 0;
/// The category gathering the renderer rows (`Group::Graphics`).
pub(crate) const GRAPHICS: usize = 2;
const _: () = assert!(matches!(
    CATEGORIES[GRAPHICS].shows,
    Shows::Rows(Panel::Group(Group::Graphics))
));
/// The category of the key bindings.
pub(crate) const KEYS: usize = 5;
const _: () = assert!(matches!(CATEGORIES[KEYS].shows, Shows::Keys));
/// The category the quick wheel's pages are.
pub(crate) const QUICK_WHEEL: usize = 10;
const _: () = assert!(matches!(CATEGORIES[QUICK_WHEEL].shows, Shows::Wheel));
/// Simple's Graphics: the renderer first.
pub(crate) const SIMPLE_GRAPHICS: usize = 14;
const _: () = assert!(matches!(
    CATEGORIES[SIMPLE_GRAPHICS].shows,
    Shows::Rows(Panel::Group(Group::Simple(Simple::Graphics)))
));
/// The category the screen opens on first in a run: the mode's first.
fn opening(mode: SettingsMode) -> usize {
    rail_of(mode)[0]
}

/// Chrome slots past the rail's: the top bar's Simple and Advanced segments,
/// and the key that switches between them (F2), as the rows' and key
/// bindings' screens report them ([`SettingsResult::Classic`]).
pub(crate) const MODE_SLOT: usize = 30;
pub(crate) const MODE_TOGGLE_SLOT: usize = 29;
const _: () = assert!(CATEGORIES.len() < MODE_TOGGLE_SLOT);

/// Whether the screen is open, on which category and in which mode.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SettingsPage {
    /// The settings phase shows this screen (not a classic+ panel).
    open: bool,
    /// The category on show, or last shown: the screen opens on it again.
    category: usize,
    /// The mode shown: the player's ([`SettingsMode::of`]), or the other one
    /// for a visit to a category only it has (the renderer's rows from the
    /// classic Graphics page, First setup's All settings).
    mode: SettingsMode,
}

impl Default for SettingsPage {
    fn default() -> Self {
        Self {
            open: false,
            category: opening(SettingsMode::Simple),
            mode: SettingsMode::Simple,
        }
    }
}

impl SettingsPage {
    /// The category on show, or the one the screen opens on next.
    #[cfg(test)]
    pub(crate) fn category(&self) -> usize {
        self.category
    }

    /// The category on show's place on its mode's rail.
    fn rail_index(&self) -> Option<usize> {
        rail_of(self.mode)
            .iter()
            .position(|index| *index == self.category)
    }
}

/// The category of mode `mode` that matches category `index`: itself when the
/// mode's rail has it, else the one of the same name, else the mode's first.
fn in_mode(index: usize, mode: SettingsMode) -> usize {
    let rail = rail_of(mode);
    if rail.contains(&index) {
        return index;
    }
    let label = CATEGORIES.get(index).map_or("", |category| category.label);
    rail.iter()
        .copied()
        .find(|other| CATEGORIES[*other].label == label)
        .unwrap_or(rail[0])
}

impl ClientMenu {
    /// Open the SJK UI's Settings on category `index`, returning to `target`
    /// when it closes. Key bindings open the classic+ key bindings.
    pub(crate) fn open_sjk_settings(
        &mut self,
        console: &ViewerConsole,
        index: usize,
        target: ReturnTarget,
    ) {
        let index = index.min(CATEGORIES.len() - 1);
        // The mode on show while the screen is open, else the player's, unless
        // the category is only the other's.
        let chosen = if self.sjk_settings.open {
            self.sjk_settings.mode
        } else {
            SettingsMode::of(console)
        };
        let mode = if CATEGORIES[index].on.has(chosen) {
            chosen
        } else {
            chosen.other()
        };
        let label = CATEGORIES[index].label;
        let panel = match CATEGORIES[index].shows {
            Shows::Rows(panel) => panel,
            // The editor shows over Interface's rows, which hold its row.
            Shows::Wheel => Panel::Group(Group::Interface),
            Shows::Keys => {
                self.settings.leave_classic();
                self.keybinds.open_classic(console, 0, Span::ALL);
                self.keybinds.set_elsewhere(0);
                // Escape leaves the screen, as it does from Settings' rows.
                self.settings_return = target;
                self.renderer_panel = None;
                self.classic_panel = None;
                self.sjk_settings = SettingsPage {
                    open: true,
                    category: index,
                    mode,
                };
                self.state.open_keybinds();
                return;
            }
        };
        self.keybinds.leave_classic();
        match panel {
            Panel::Settings { caption, span } => {
                let tab = SettingsMenu::tab_index(caption).unwrap_or(0);
                self.settings
                    .open_classic(console, tab, span, PanelFrame::Main);
            }
            Panel::Group(group) => {
                self.settings
                    .open_classic_group(console, group, PanelFrame::Main)
            }
            Panel::Renderer { tab } => {
                self.settings
                    .open_classic_renderer(console, tab, PanelFrame::Main);
            }
            Panel::Keybinds { .. } => return,
        }
        self.settings.set_elsewhere(0);
        if CATEGORIES[index].shows == Shows::Wheel {
            self.settings.select_wheel_row();
            self.settings
                .open_wheel_editor(console, crate::settings::WheelMode::Category);
        }
        self.settings_return = target;
        self.renderer_panel = None;
        self.classic_panel = None;
        self.sjk_settings = SettingsPage {
            open: true,
            category: index,
            mode,
        };
        self.settings.set_screen(Some(mode), label);
        self.state.open_settings();
    }

    /// Open the SJK UI's Settings where the player left it: the category last
    /// shown, or its match in the player's mode ([`in_mode`]).
    pub(crate) fn open_sjk_settings_resumed(
        &mut self,
        console: &ViewerConsole,
        target: ReturnTarget,
    ) {
        let category = in_mode(self.sjk_settings.category, SettingsMode::of(console));
        self.open_sjk_settings(console, category, target);
    }

    /// Show mode `mode` (kept as the player's), on the category of it that
    /// matches the one on show.
    fn switch_sjk_settings_mode(&mut self, console: &mut ViewerConsole, mode: SettingsMode) {
        if mode != SettingsMode::of(console) {
            mode.store(console);
        }
        self.sjk_settings.mode = mode;
        let target = self.settings_return;
        let category = in_mode(self.sjk_settings.category, mode);
        self.open_sjk_settings(console, category, target);
    }

    /// What a chrome slot past the rail's asks (the mode's segments, F2), as
    /// the rows and the key bindings report it; `None` for a slot of the rail.
    fn sjk_mode_slot(&self, slot: usize) -> Option<SettingsMode> {
        match slot {
            MODE_TOGGLE_SLOT => Some(self.sjk_settings.mode.other()),
            MODE_SLOT.. => SettingsMode::ALL.get(slot - MODE_SLOT).copied(),
            _ => None,
        }
    }

    /// Open the SJK UI's Settings on the category holding the rows of the
    /// tabbed catalogue's tab `tab` (a classic Setup entry's), returning to
    /// `target`.
    pub(crate) fn open_sjk_settings_on_tab(
        &mut self,
        console: &ViewerConsole,
        tab: usize,
        target: ReturnTarget,
    ) {
        let category =
            SettingsMenu::tab_caption(tab).map_or(opening(SettingsMode::Advanced), category_of_tab);
        self.open_sjk_settings(console, category, target);
    }

    /// Open the SJK UI's Settings on Graphics, which gathers the renderer
    /// rows, returning to `target`.
    pub(crate) fn open_sjk_renderer(&mut self, console: &ViewerConsole, target: ReturnTarget) {
        self.open_sjk_settings(console, GRAPHICS, target);
    }

    /// Open the SJK UI's Key bindings, returning to `target`.
    pub(crate) fn open_sjk_keys(&mut self, console: &ViewerConsole, target: ReturnTarget) {
        self.open_sjk_settings(console, KEYS, target);
    }

    /// Open First setup as the SJK UI's pop-up over the map, returning to
    /// `target` when it closes; its All settings opens the whole screen on
    /// First setup.
    pub(crate) fn open_sjk_first_setup(&mut self, console: &ViewerConsole, target: ReturnTarget) {
        self.open_sjk_settings(console, FIRST_SETUP, target);
        self.settings.open_first_setup_popup(console);
    }

    /// Open Settings with `query` searched, returning to `target` when it closes (a chip
    /// naming a setting, on the what's-new card or the changelog page): the SJK UI's
    /// Settings on the category last shown, or the classic Setup panel under the
    /// classic style.
    pub(crate) fn open_settings_search(
        &mut self,
        console: &ViewerConsole,
        query: &str,
        target: ReturnTarget,
    ) {
        let classic = self.menu_style != super::super::MenuStyle::Sjk
            && Page::Setup.opening_panel().is_some_and(|entry| {
                let frame = match target {
                    ReturnTarget::MainMenu => PanelFrame::Main,
                    ReturnTarget::InGame => PanelFrame::InGame,
                };
                self.open_classic_panel(console, Page::Setup, entry, frame, target)
            });
        if !classic {
            self.open_sjk_settings_resumed(console, target);
        }
        self.settings.carry_search(console, query);
    }

    /// Open the SJK UI's Settings over a match, from its in-game menu, on the
    /// category last shown; it returns to the in-game menu.
    pub(crate) fn open_sjk_settings_from_game(&mut self, console: &ViewerConsole) {
        self.open_sjk_settings_resumed(console, ReturnTarget::InGame);
    }

    /// The way back the screen's top bar names: the game menu over a match,
    /// else the main menu.
    fn sjk_settings_back(&self) -> &'static str {
        if self.settings_return == ReturnTarget::InGame {
            "Game menu"
        } else {
            "Main menu"
        }
    }

    /// Whether the SJK UI's Settings is the screen on show: a category of
    /// settings rows, or Key bindings.
    pub(crate) fn sjk_settings_on_show(&self) -> bool {
        self.sjk_settings.open
            && self.classic_panel.is_none()
            && match self.state.phase() {
                ClientPhase::Settings => self.settings.has_panel_rows(),
                ClientPhase::Keybinds => self.keybinds.has_list(),
                _ => false,
            }
    }

    /// The screen is left (to the main menu or the game): it is no longer open.
    pub(in crate::menu) fn close_sjk_settings(&mut self) {
        self.sjk_settings.open = false;
        self.settings.set_screen(None, "");
    }

    /// Another settings screen opened: this one is not the one on show.
    pub(in crate::menu) fn leave_sjk_settings(&mut self) {
        self.sjk_settings.open = false;
    }

    /// What the key bindings asked of the screen around them, as
    /// [`Self::sjk_settings_result`] reads the settings rows'.
    pub(in crate::menu) fn sjk_keys_result(
        &mut self,
        result: &crate::keybind_editor::EditorResult,
        console: &mut ViewerConsole,
    ) -> Option<MenuAction> {
        use crate::keybind_editor::EditorResult;
        if !self.sjk_settings_on_show() {
            return None;
        }
        let target = self.settings_return;
        let rail = rail_of(self.sjk_settings.mode);
        match *result {
            EditorResult::Classic(slot) if self.sjk_mode_slot(slot).is_some() => {
                if let Some(mode) = self.sjk_mode_slot(slot) {
                    self.switch_sjk_settings_mode(console, mode);
                }
                Some(MenuAction::None)
            }
            EditorResult::Classic(slot) if slot < rail.len() => {
                let index = rail[slot];
                if index != self.sjk_settings.category
                    || !self.keybinds.search_text().trim().is_empty()
                {
                    self.open_sjk_settings(console, index, target);
                }
                Some(MenuAction::None)
            }
            EditorResult::Classic(_) => Some(MenuAction::None),
            EditorResult::ClassicCycle(direction) => {
                let next = next_category(&self.sjk_settings, direction);
                self.open_sjk_settings(console, next, target);
                Some(MenuAction::None)
            }
            _ => None,
        }
    }

    /// The rail of the mode on show, `searched` lighting none of it.
    fn sjk_rail(&self, searched: bool) -> crate::settings::Rail<'static> {
        crate::settings::Rail {
            categories: match self.sjk_settings.mode {
                SettingsMode::Simple => &SIMPLE_NAMES,
                SettingsMode::Advanced => &ADVANCED_NAMES,
            },
            current: if searched {
                None
            } else {
                self.sjk_settings.rail_index()
            },
            back: self.sjk_settings_back(),
            mode: self.sjk_settings.mode,
        }
    }

    /// Draw the SJK UI's Key bindings.
    pub(crate) fn append_sjk_keys(&mut self, target: TextTarget<'_>, viewport: [f32; 2]) {
        let reveal = self.screen_reveal();
        let searched = !self.keybinds.search_text().trim().is_empty();
        let rail = self.sjk_rail(searched);
        self.keybinds.append_sjk(target, viewport, reveal, &rail);
    }

    /// What the settings rows asked of the screen around them: a category of
    /// the rail (`Classic(index)`), or the next or previous one (Tab). `None`
    /// for results the screen leaves to the settings phase.
    pub(in crate::menu) fn sjk_settings_result(
        &mut self,
        result: &SettingsResult,
        console: &mut ViewerConsole,
    ) -> Option<MenuAction> {
        if !self.sjk_settings_on_show() {
            return None;
        }
        let target = self.settings_return;
        let rail = rail_of(self.sjk_settings.mode);
        match *result {
            SettingsResult::Classic(slot) if self.sjk_mode_slot(slot).is_some() => {
                if let Some(mode) = self.sjk_mode_slot(slot) {
                    self.switch_sjk_settings_mode(console, mode);
                }
                Some(MenuAction::None)
            }
            SettingsResult::Classic(slot) if slot < rail.len() => {
                let index = rail[slot];
                if index != self.sjk_settings.category || self.settings.searching_results() {
                    self.open_sjk_settings(console, index, target);
                }
                Some(MenuAction::None)
            }
            SettingsResult::Classic(_) => Some(MenuAction::None),
            SettingsResult::ClassicCycle(direction) => {
                let next = next_category(&self.sjk_settings, direction);
                self.open_sjk_settings(console, next, target);
                Some(MenuAction::None)
            }
            // The pop-up's All settings: its rows on the whole screen, among
            // every setting (Advanced, for the visit).
            SettingsResult::AllSettings => {
                self.open_sjk_settings(console, FIRST_SETUP, target);
                Some(MenuAction::None)
            }
            // Interface's "Quick wheel pages" row: the Quick wheel category.
            SettingsResult::OpenWheelPages => {
                self.open_sjk_settings(console, QUICK_WHEEL, target);
                Some(MenuAction::None)
            }
            _ => None,
        }
    }

    /// Draw the SJK UI's Settings, First setup's pop-up, or the resolution
    /// list's card over either.
    pub(crate) fn append_sjk_settings(&mut self, target: TextTarget<'_>, viewport: [f32; 2]) {
        let reveal = self.screen_reveal();
        if self.settings.resolutions_open() {
            self.settings
                .append_resolutions_sjk(target, viewport, reveal);
            return;
        }
        if self.settings.popup() {
            self.settings.append_sjk_popup(target, viewport, reveal);
            return;
        }
        let rail = self.sjk_rail(self.settings.searching_results());
        self.settings.append_sjk(target, viewport, reveal, &rail);
    }
}

/// The category holding the rows of the tabbed catalogue's tab `caption`:
/// the one showing that tab, or the classic Setup group gathering its rows.
fn category_of_tab(caption: &str) -> usize {
    let group = match caption {
        "GAME" => Some(Group::GameOptions),
        "TEXT" => Some(Group::Interface),
        // The scoreboard's rows are HUD's.
        "HUD" | "HUD+" => Some(Group::Hud),
        crate::settings::FIRST_SETUP_CAPTION => Some(Group::Quick),
        _ => None,
    };
    ADVANCED_RAIL
        .iter()
        .copied()
        .find(|index| {
            let category = &CATEGORIES[*index];
            match (category.classic, category.shows, group) {
                (Some(Panel::Settings { caption: shown, .. }), _, None) => shown == caption,
                (Some(Panel::Group(shown)), _, Some(wanted)) => shown == wanted,
                (_, Shows::Rows(Panel::Group(shown)), Some(wanted)) => shown == wanted,
                (_, Shows::Rows(Panel::Settings { caption: shown, .. }), None) => shown == caption,
                _ => false,
            }
        })
        .unwrap_or(opening(SettingsMode::Advanced))
}

#[cfg(test)]
impl ClientMenu {
    /// The SJK UI's Key bindings, the action of `command` focused and awaiting
    /// its second key when `capture` (world shots).
    pub(crate) fn sjk_keys_for_shot(
        &mut self,
        console: &ViewerConsole,
        command: &str,
        capture: bool,
    ) {
        let keys = CATEGORIES
            .iter()
            .position(|category| category.shows == Shows::Keys)
            .unwrap_or(0);
        self.open_sjk_settings(console, keys, ReturnTarget::MainMenu);
        self.keybinds.focus_for_shot(command, capture);
    }

    /// Settings > Quick wheel (opened if it is not), after `keys`, each typing
    /// its text if it has one (world shots).
    pub(crate) fn sjk_wheel_for_shot(
        &mut self,
        console: &mut ViewerConsole,
        keys: &[(winit::keyboard::KeyCode, Option<&str>)],
    ) {
        if !self.settings.wheel_editor_open() {
            self.open_sjk_settings(console, QUICK_WHEEL, ReturnTarget::MainMenu);
        }
        self.settings.wheel_keys_for_shot(keys, console);
    }

    /// The quick wheel's editor opened from Interface's row of the classic+
    /// settings (world shots).
    pub(crate) fn wheel_overlay_for_shot(&mut self, console: &ViewerConsole) {
        self.open_classic_panel(
            console,
            Page::Gameplay,
            Entry::Interface,
            PanelFrame::Main,
            ReturnTarget::MainMenu,
        );
        self.settings
            .open_wheel_editor(console, crate::settings::WheelMode::Overlay);
    }

    /// The SJK UI's Settings on category `category`, the row of `cvar` focused,
    /// its list open (`list`) or a `search` typed (menu snapshots).
    pub(crate) fn sjk_settings_for_snapshot(
        &mut self,
        console: &ViewerConsole,
        category: usize,
        cvar: Option<&str>,
        list: bool,
        search: Option<&str>,
    ) {
        self.menu_style = super::super::MenuStyle::Sjk;
        self.open_sjk_settings(console, category, ReturnTarget::MainMenu);
        if let Some(cvar) = cvar {
            self.settings.select_cvar(cvar);
        }
        if list {
            self.settings.dropdown_for_snapshot(console);
        }
        if let Some(text) = search {
            self.settings.search_for_snapshot(console, text);
        }
    }
}

/// The category `direction` steps from `page`'s along its mode's rail, wrapping.
fn next_category(page: &SettingsPage, direction: i32) -> usize {
    let rail = rail_of(page.mode);
    let at = page.rail_index().unwrap_or(0) as i32;
    rail[(at + direction.signum()).rem_euclid(rail.len() as i32) as usize]
}

/// The classic Setup page and entry whose panel is category `index`'s, if one
/// is (the classic pages have no Graphics of all four renderer tabs); Key
/// bindings is the Controls page's.
fn classic_place(index: usize) -> Option<(Page, Entry)> {
    let category = CATEGORIES.get(index)?;
    let panel = match category.shows {
        _ if category.classic.is_some() => category.classic?,
        Shows::Rows(panel) => panel,
        // Interface holds the row that opens the quick wheel's pages.
        Shows::Wheel => Panel::Group(Group::Interface),
        Shows::Keys => return Some((Page::Controls, Entry::Movement)),
    };
    [Page::Setup, Page::Graphics, Page::Gameplay]
        .into_iter()
        .find_map(|page| {
            let slot = page
                .slots()
                .iter()
                .find(|slot| slot.entry.panel() == Some(panel))?;
            Some((page, slot.entry))
        })
}

impl ClientMenu {
    /// The menu style changed to classic under the screen (its Menu style row
    /// is on Interface): the classic+ panel of the category on show takes over,
    /// Graphics' first renderer tab for Graphics.
    pub(in crate::menu) fn sjk_settings_to_classic(&mut self, console: &ViewerConsole) {
        let target = self.settings_return;
        let (page, entry) = classic_place(self.sjk_settings.category)
            .unwrap_or((Page::Graphics, Entry::RenderImage));
        self.open_classic_panel(console, page, entry, PanelFrame::Main, target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn console() -> (tempfile::TempDir, ViewerConsole) {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        (directory, console)
    }

    fn menu() -> ClientMenu {
        let mut menu = ClientMenu::new(true, String::new());
        menu.menu_style = super::super::super::MenuStyle::Sjk;
        menu
    }

    #[test]
    fn the_rail_shows_every_classic_setup_group_and_the_renderer_as_one() {
        // Every settings panel of the classic Setup pages has its category,
        // but the renderer's four, which Graphics gathers.
        for entry in [
            Entry::FirstSetup,
            Entry::Video,
            Entry::Sound,
            Entry::MouseJoystick,
            Entry::GameOptions,
            Entry::Interface,
            // Scoreboard's rows are HUD's, under their own heading.
            Entry::Hud,
            Entry::Network,
        ] {
            assert!(
                (0..CATEGORIES.len())
                    .any(|index| classic_place(index).map(|(_, found)| found) == Some(entry)),
                "{entry:?}"
            );
        }
        assert_eq!(CATEGORIES[FIRST_SETUP].label, "First setup");
        // Every category has its icon: a settings one, or the wheel's own.
        for (label, icon) in ADVANCED_NAMES.into_iter().chain(SIMPLE_NAMES) {
            assert!(
                crate::settings_icons::texture(icon).is_some(),
                "{label}: {icon}"
            );
        }
        assert_eq!(
            CATEGORIES[QUICK_WHEEL].icon,
            crate::quick_wheel::catalog::WHEEL_ICON
        );
    }

    #[test]
    fn tab_steps_along_the_modes_rail_and_wraps() {
        let page = |mode, category| SettingsPage {
            open: true,
            category,
            mode,
        };
        let advanced = |category| page(SettingsMode::Advanced, category);
        assert_eq!(next_category(&advanced(KEYS - 1), 1), KEYS);
        assert_eq!(next_category(&advanced(KEYS), 1), KEYS + 1);
        let last = ADVANCED_RAIL[ADVANCED_RAIL.len() - 1];
        assert_eq!(next_category(&advanced(last), 1), 0);
        assert_eq!(next_category(&advanced(0), -1), last);
        // Simple's rail goes through Key bindings, its own, and wraps.
        let simple = |category| page(SettingsMode::Simple, category);
        let controls = SIMPLE_RAIL[3];
        assert_eq!(CATEGORIES[controls].label, "Controls");
        assert_eq!(next_category(&simple(controls), 1), KEYS);
        assert_eq!(next_category(&simple(KEYS), 1), SIMPLE_RAIL[5]);
        assert_eq!(next_category(&simple(SIMPLE_RAIL[7]), 1), SIMPLE_RAIL[0]);
    }

    #[test]
    fn each_mode_maps_a_category_to_its_own_of_the_same_name() {
        // Advanced's Sound is Simple's Sound, and back.
        let sound = in_mode(3, SettingsMode::Simple);
        assert_eq!(CATEGORIES[sound].label, "Sound");
        assert!(SIMPLE_RAIL.contains(&sound));
        assert_eq!(in_mode(sound, SettingsMode::Advanced), 3);
        // Key bindings is on both rails; Scoreboard has no Simple match.
        assert_eq!(in_mode(KEYS, SettingsMode::Simple), KEYS);
        assert_eq!(in_mode(10, SettingsMode::Simple), SIMPLE_RAIL[0]);
        // Simple's General has no Advanced match: Advanced's first.
        assert_eq!(in_mode(SIMPLE_RAIL[7], SettingsMode::Advanced), 0);
    }

    #[test]
    fn a_new_profile_opens_simple_and_the_top_bar_switches_modes() {
        let (_directory, mut console) = console();
        let mut menu = menu();
        menu.open_sjk_settings_resumed(&console, ReturnTarget::MainMenu);
        assert_eq!(menu.sjk_settings.mode, SettingsMode::Simple);
        assert_eq!(menu.sjk_settings.category, SIMPLE_RAIL[0]);
        // Simple's Graphics, the rail's second, leads with the renderer.
        menu.settings_result(SettingsResult::Classic(1), &mut console);
        assert_eq!(menu.sjk_settings.category, SIMPLE_GRAPHICS);
        // Advanced's segment: kept, and Advanced's Graphics shows.
        menu.settings_result(SettingsResult::Classic(MODE_SLOT + 1), &mut console);
        assert_eq!(SettingsMode::of(&console), SettingsMode::Advanced);
        assert_eq!(menu.sjk_settings.mode, SettingsMode::Advanced);
        assert_eq!(menu.sjk_settings.category, GRAPHICS);
        assert!(menu.settings.renderer_open());
        // F2 (the toggle slot) goes back to Simple.
        menu.settings_result(SettingsResult::Classic(MODE_TOGGLE_SLOT), &mut console);
        assert_eq!(SettingsMode::of(&console), SettingsMode::Simple);
        assert_eq!(menu.sjk_settings.category, SIMPLE_GRAPHICS);
        // A category only Advanced has shows Advanced for the visit, and the
        // player's mode stays Simple.
        menu.open_sjk_settings(&console, QUICK_WHEEL, ReturnTarget::MainMenu);
        assert_eq!(menu.sjk_settings.mode, SettingsMode::Advanced);
        assert_eq!(SettingsMode::of(&console), SettingsMode::Simple);
        // Closed and opened again on Key bindings, the player's mode; the
        // toggle works there too.
        menu.settings_result(SettingsResult::Back, &mut console);
        menu.open_sjk_settings(&console, KEYS, ReturnTarget::MainMenu);
        assert_eq!(menu.sjk_settings.mode, SettingsMode::Simple);
        menu.keybinds_result(
            crate::keybind_editor::EditorResult::Classic(MODE_TOGGLE_SLOT),
            &mut console,
        );
        assert_eq!(SettingsMode::of(&console), SettingsMode::Advanced);
        assert_eq!(menu.sjk_settings.category, KEYS);
    }

    #[test]
    fn the_screen_opens_switches_category_and_closes_to_the_main_menu() {
        let (_directory, mut console) = console();
        let mut menu = menu();
        menu.open_sjk_settings(&console, 2, ReturnTarget::MainMenu);
        assert!(menu.sjk_settings_on_show());
        assert!(
            menu.settings.renderer_open(),
            "Graphics holds the renderer's rows"
        );
        // A category of the rail, then Tab to the next.
        let action = menu.settings_result(SettingsResult::Classic(3), &mut console);
        assert_eq!(action, MenuAction::None);
        assert_eq!(menu.sjk_settings.category, 3);
        menu.settings_result(SettingsResult::ClassicCycle(1), &mut console);
        assert_eq!(menu.sjk_settings.category, 4);
        // Back leaves for the main menu; the screen opens again where it was.
        menu.settings_result(SettingsResult::Back, &mut console);
        assert_eq!(*menu.state.phase(), ClientPhase::MainMenu);
        assert!(!menu.sjk_settings_on_show());
        menu.open_sjk_settings(&console, menu.sjk_settings.category, ReturnTarget::MainMenu);
        assert_eq!(menu.sjk_settings.category, 4);
    }

    #[test]
    fn opened_from_a_game_it_names_and_returns_to_the_game_menu() {
        let (_directory, mut console) = console();
        SettingsMode::Advanced.store(&mut console);
        let mut menu = menu();
        menu.open_sjk_settings(&console, 3, ReturnTarget::MainMenu);
        assert_eq!(menu.sjk_settings_back(), "Main menu");
        menu.settings_result(SettingsResult::Back, &mut console);
        // From the game, on the category last shown.
        menu.open_sjk_settings_from_game(&console);
        assert!(menu.sjk_settings_on_show() && menu.sjk_screen());
        assert_eq!(menu.sjk_settings.category, 3);
        assert_eq!(menu.sjk_settings_back(), "Game menu");
        let action = menu.settings_result(SettingsResult::Back, &mut console);
        assert_eq!(action, MenuAction::ReturnToGameMenu);
        assert!(!menu.sjk_settings_on_show());
    }

    #[test]
    fn a_style_change_on_the_screen_hands_over_to_the_same_group() {
        let (_directory, console) = console();
        let interface = CATEGORIES
            .iter()
            .position(|category| category.label == "Interface")
            .unwrap();
        // To classic: the classic+ panel of the same group.
        let mut classic = menu();
        classic.open_sjk_settings(&console, interface, ReturnTarget::MainMenu);
        classic.set_menu_style(super::super::super::MenuStyle::Classic, &console);
        assert!(!classic.sjk_settings_on_show());
        let panel = classic.classic_panel.expect("a classic panel");
        assert_eq!(
            (panel.page, panel.entry),
            (Page::Gameplay, Entry::Interface)
        );
    }

    #[test]
    fn key_bindings_are_a_category_of_the_screen_and_tab_leaves_them() {
        use crate::keybind_editor::EditorResult;
        let (_directory, mut console) = console();
        let mut menu = menu();
        menu.open_sjk_settings(&console, 7, ReturnTarget::MainMenu);
        let keys = CATEGORIES
            .iter()
            .position(|category| category.shows == Shows::Keys)
            .unwrap();
        menu.settings_result(SettingsResult::Classic(keys), &mut console);
        assert_eq!(*menu.state.phase(), ClientPhase::Keybinds);
        assert!(
            menu.sjk_settings_on_show(),
            "the same screen, on Key bindings"
        );
        assert!(menu.classic_panel.is_none());
        // Tab to the next category, the rail back to Key bindings, then Escape.
        menu.keybinds_result(EditorResult::ClassicCycle(1), &mut console);
        assert_eq!(*menu.state.phase(), ClientPhase::Settings);
        assert_eq!(menu.sjk_settings.category, keys + 1);
        menu.settings_result(SettingsResult::Classic(keys), &mut console);
        assert_eq!(*menu.state.phase(), ClientPhase::Keybinds);
        menu.keybinds_result(EditorResult::Back, &mut console);
        assert_eq!(*menu.state.phase(), ClientPhase::MainMenu);
        assert!(!menu.sjk_settings_on_show());
        // A style change there hands over to the classic Controls panel.
        menu.open_sjk_settings(&console, keys, ReturnTarget::MainMenu);
        menu.set_menu_style(super::super::super::MenuStyle::Classic, &console);
        let panel = menu.classic_panel.expect("a classic panel");
        assert_eq!(panel.page, Page::Controls);
    }

    /// The main menu's ways to settings without a classic+ panel open the SJK
    /// UI's Settings, under either style: a tab on the category holding its
    /// rows, the renderer on Graphics, the key bindings on Key bindings; each
    /// closes back to the main menu.
    #[test]
    fn the_destinations_open_the_sjk_settings_in_either_style() {
        use super::super::super::destination::MainDestination;
        use crate::keybind_editor::EditorResult;
        let (_directory, mut console) = console();
        for style in [
            super::super::super::MenuStyle::Sjk,
            super::super::super::MenuStyle::Classic,
        ] {
            let mut menu = menu();
            menu.menu_style = style;
            for (caption, label) in [
                ("VIDEO", "Display"),
                ("AUDIO", "Sound"),
                ("HUD", "HUD"),
                ("CONTROLS", "Controls"),
                ("GAME", "Gameplay"),
                ("NETWORK", "Network"),
                ("HUD+", "HUD"),
                ("TEXT", "Interface"),
                (crate::settings::FIRST_SETUP_CAPTION, "First setup"),
            ] {
                let tab = SettingsMenu::tab_index(caption).unwrap();
                menu.open_main_destination(MainDestination::Settings { tab }, &mut console);
                assert!(
                    menu.sjk_settings_on_show() && menu.sjk_screen(),
                    "{caption}"
                );
                assert_eq!(CATEGORIES[menu.sjk_settings.category].label, label);
                menu.settings_result(SettingsResult::Back, &mut console);
                assert_eq!(*menu.state.phase(), ClientPhase::MainMenu);
            }
            menu.open_main_destination(MainDestination::Renderer, &mut console);
            assert!(menu.sjk_settings_on_show() && menu.sjk_screen());
            assert_eq!(menu.sjk_settings.category, GRAPHICS);
            assert!(menu.settings.renderer_open());
            menu.settings_result(SettingsResult::Back, &mut console);
            assert_eq!(*menu.state.phase(), ClientPhase::MainMenu);
            menu.open_main_destination(MainDestination::Keybinds { category: 2 }, &mut console);
            assert_eq!(*menu.state.phase(), ClientPhase::Keybinds);
            assert!(menu.sjk_settings_on_show() && menu.sjk_screen());
            assert_eq!(menu.sjk_settings.category, KEYS);
            menu.keybinds_result(EditorResult::Back, &mut console);
            assert_eq!(*menu.state.phase(), ClientPhase::MainMenu);
        }
    }

    /// The resolution list is the SJK UI's card: the screen stays the SJK
    /// UI's while it is open, and closing it shows the rows again.
    #[test]
    fn the_resolution_list_stays_on_the_sjk_screen() {
        let (_directory, console) = console();
        let mut menu = menu();
        let display = CATEGORIES
            .iter()
            .position(|category| category.label == "Display")
            .unwrap();
        menu.open_sjk_settings(&console, display, ReturnTarget::MainMenu);
        menu.resolutions_for_shot(&console);
        assert!(menu.settings.resolutions_open());
        assert!(menu.sjk_settings_on_show() && menu.sjk_screen());
    }

    /// The classic Graphics page's renderer route opens the SJK UI's Settings
    /// on Graphics, and backing out shows the classic panel it came from
    /// again, in game too.
    #[test]
    fn the_classic_renderer_route_returns_to_its_panel() {
        let (_directory, mut console) = console();
        for (frame, target) in [
            (PanelFrame::Main, ReturnTarget::MainMenu),
            (PanelFrame::InGame, ReturnTarget::InGame),
        ] {
            let mut menu = menu();
            menu.menu_style = super::super::super::MenuStyle::Classic;
            menu.classic_renderer_for_shot(&console, frame);
            assert!(menu.sjk_settings_on_show() && menu.sjk_screen());
            assert_eq!(menu.sjk_settings.category, GRAPHICS);
            assert_eq!(menu.settings_return, target);
            menu.settings_result(SettingsResult::Back, &mut console);
            let panel = menu.classic_panel.expect("the classic panel again");
            assert_eq!(
                (panel.page, panel.entry),
                (Page::Graphics, Entry::RenderImage)
            );
            assert_eq!(panel.frame, frame);
            assert!(!menu.sjk_settings_on_show());
        }
    }
}
