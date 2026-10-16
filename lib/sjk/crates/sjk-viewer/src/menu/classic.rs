//! Classic menu style (`ui_menuStyle classic`): screens laid out close to
//! the retail Jedi Academy multiplayer menus so long-time players find
//! things where they expect them. Not a port of the `.menu` scripts: pages
//! and entries are declared in [`layout`], drawn with the shared menu
//! widgets in [`view`], and lead to the same screens as the SJK UI through
//! [`MainDestination`].
//!
//! Implemented: the main menu with its Play (multiplayer) and quit pages,
//! the Setup and Controls option panels ([`panel`]) on the main menu and as
//! the in-game pop-ups, SJK's renderer page in the same layout (its groups
//! open from Setup's RENDERER), the server browser ([`browser`]) and the
//! in-game menu ([`crate::ingame_menu`]). The screens these pages open that
//! have no classic version (Create game, the SJK Settings and Key bindings when
//! no classic+ panel fits) are the SJK UI's; the follow-up plan is kept in
//! `docs/client.md`.

pub(crate) mod browser;
pub(crate) mod layout;
pub(crate) mod loading;
mod pages;
pub(crate) mod panel;
pub(crate) mod view;

use super::{ClientMenu, MenuAction};
use crate::console::ViewerConsole;
use crate::menu::destination::MainDestination;
use crate::player_menu::ReturnTarget;
use crate::settings::SettingsMenu;
use layout::{Entry, Outcome, Page, Panel, Slot};
use panel::{Frame, PanelFrame};
use sjk_ui::AbstractAction;

/// The classic option panel on show: the page whose list it belongs to,
/// the open group, and where it is drawn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ClassicPanel {
    pub(super) page: Page,
    pub(super) entry: Entry,
    pub(super) frame: Frame,
}

/// Page and focused entry of the classic main menu.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ClassicMain {
    page: Page,
    selection: usize,
}

impl ClassicMain {
    pub(crate) const fn new() -> Self {
        Self {
            page: Page::Main,
            selection: 0,
        }
    }

    /// Back to the opening page, focus on its first entry.
    pub(crate) fn reset(&mut self) {
        *self = Self::new();
    }

    pub(crate) fn page(&self) -> Page {
        self.page
    }

    pub(crate) fn selection(&self) -> usize {
        self.selection
    }

    /// The page's entries in focus order.
    pub(crate) fn slots(&self) -> &'static [Slot] {
        self.page.slots()
    }

    /// Focus entry `index` of the current page; out-of-range indices (a
    /// pointer token of another screen) are ignored.
    pub(crate) fn select(&mut self, index: usize) {
        if index < self.slots().len() {
            self.selection = index;
        }
    }

    /// Step focus forward or back in the page's entry order, wrapping.
    fn step(&mut self, forward: bool) {
        let count = self.slots().len();
        self.selection = if forward {
            (self.selection + 1) % count
        } else {
            self.selection.checked_sub(1).unwrap_or(count - 1)
        };
    }

    /// Show `page` with its initial entry focused.
    pub(crate) fn show(&mut self, page: Page) {
        self.page = page;
        self.selection = page.initial_selection();
    }

    /// What activating the focused entry does.
    fn outcome(&self) -> Option<Outcome> {
        Some(self.focused()?.outcome())
    }

    /// The focused entry.
    fn focused(&self) -> Option<Entry> {
        Some(self.slots().get(self.selection)?.entry)
    }
}

impl ClientMenu {
    /// Move focus on the classic main menu. Focus follows the page's entry
    /// order, as retail Up/Down/Tab did.
    pub(super) fn navigate_classic(&mut self, action: AbstractAction) {
        let count = self.classic.slots().len();
        match self
            .ui
            .action(action)
            .map(usize::from)
            .filter(|row| *row < count)
        {
            Some(row) => self.classic.select(row),
            None => self.classic.step(action != AbstractAction::Previous),
        }
    }

    /// Activate the focused classic entry: change page, or open the screen
    /// it leads to.
    pub(super) fn activate_classic(&mut self, console: &mut ViewerConsole) -> MenuAction {
        // Setup and Controls open on their first group, as the retail pages
        // show Video and Movement beside the list; a group opens its panel.
        if let Some(entry) = self.classic.focused() {
            let panel = match entry.outcome() {
                Outcome::Page(page) => page.opening_panel().map(|first| (page, first)),
                _ => entry.panel().map(|_| (self.classic.page(), entry)),
            };
            if let Some((page, entry)) = panel {
                self.open_classic_panel(console, page, entry, Frame::Main, ReturnTarget::MainMenu);
                return MenuAction::None;
            }
        }
        match self.classic.outcome() {
            Some(Outcome::Page(page)) => {
                self.classic.show(page);
                MenuAction::None
            }
            Some(Outcome::Open(destination)) => self.open_main_destination(destination, console),
            Some(Outcome::Settings(caption)) => {
                let tab = SettingsMenu::tab_index(caption).unwrap_or(0);
                self.open_main_destination(MainDestination::Settings { tab }, console)
            }
            Some(Outcome::Keybinds(category)) => self.open_main_destination(
                MainDestination::Keybinds {
                    category: category as usize,
                },
                console,
            ),
            Some(Outcome::Unavailable) | None => MenuAction::None,
        }
    }

    /// Show group `entry` of `page` in a classic option panel drawn in
    /// `frame`, returning to `target` when closed. False when the entry has
    /// no group SJK can show.
    pub(crate) fn open_classic_panel(
        &mut self,
        console: &ViewerConsole,
        page: Page,
        entry: Entry,
        frame: Frame,
        target: ReturnTarget,
    ) -> bool {
        self.leave_sjk_settings();
        match entry.panel() {
            Some(Panel::Settings { caption, span }) => {
                let Some(tab) = SettingsMenu::tab_index(caption) else {
                    return false;
                };
                self.keybinds.leave_classic();
                self.settings.open_classic(console, tab, span, frame);
                self.state.open_settings();
            }
            Some(Panel::Keybinds { category, span }) => {
                self.settings.leave_classic();
                self.keybinds.open_classic(console, category as usize, span);
                self.state.open_keybinds();
            }
            Some(Panel::Renderer { tab }) => {
                self.keybinds.leave_classic();
                self.settings.open_classic_renderer(console, tab, frame);
                self.state.open_settings();
            }
            Some(Panel::Group(group)) => {
                self.keybinds.leave_classic();
                self.settings.open_classic_group(console, group, frame);
                self.state.open_settings();
            }
            None => return false,
        }
        self.settings_return = target;
        self.renderer_panel = None;
        self.classic_panel = Some(ClassicPanel { page, entry, frame });
        true
    }

    /// The search typed on the Settings tab `page` is, and so the other tab's
    /// share of it: the panels name how many entries of the other tab match, and a
    /// tab switch carries the search along. Recomputed only when the text changes.
    pub(super) fn sync_cross_search(&mut self) {
        let Some(panel) = self.classic_panel else {
            return;
        };
        let options = match panel.page.settings_tab() {
            Some(1) => true,
            Some(_) => false,
            None => return,
        };
        let text = if options {
            self.settings.search_text()
        } else {
            self.keybinds.search_text()
        };
        if text.trim() != self.cross_search.0 {
            let text = text.trim().to_owned();
            let count = if options {
                self.keybinds.count_matches(&text)
            } else {
                SettingsMenu::count_matches(&text)
            };
            self.cross_search = (text, count);
        }
        let count = self.cross_search.1;
        if options {
            self.settings.set_elsewhere(count);
        } else {
            self.keybinds.set_elsewhere(count);
        }
    }

    /// The search typed on the Settings tab `page` is, to carry to the other tab.
    fn search_to_carry(&self, page: Page) -> Option<String> {
        let text = match page.settings_tab()? {
            1 => self.settings.search_text(),
            _ => self.keybinds.search_text(),
        };
        (!text.trim().is_empty()).then(|| text.to_owned())
    }

    /// Type `text` into the search of the Settings tab `page` opened.
    fn carry_search(&mut self, console: &ViewerConsole, page: Page, text: &str) {
        match page.settings_tab() {
            Some(1) => self.settings.carry_search(console, text),
            Some(_) => self.keybinds.carry_search(text),
            None => {}
        }
    }

    /// Show `panel` again, as it was before a screen it opened.
    pub(super) fn reopen_classic_panel(&mut self, console: &ViewerConsole, panel: ClassicPanel) {
        let target = self.settings_return;
        self.open_classic_panel(console, panel.page, panel.entry, panel.frame, target);
    }

    /// The in-game bar's Setup or Controls: the pop-up on its first group.
    pub(crate) fn open_classic_panel_from_game(&mut self, console: &ViewerConsole, page: Page) {
        if let Some(entry) = page.opening_panel() {
            self.open_classic_panel(console, page, entry, Frame::InGame, ReturnTarget::InGame);
        }
    }

    /// The classic panel to draw this frame, if one is open.
    pub(super) fn classic_panel_frame(&self) -> Option<PanelFrame> {
        let panel = self.classic_panel?;
        // The one list of bindings marks the category being looked at.
        let active = match panel.page {
            Page::Controls => self
                .keybinds
                .classic_category()
                .and_then(Entry::of_category)
                .unwrap_or(panel.entry),
            _ => panel.entry,
        };
        Some(PanelFrame {
            frame: panel.frame,
            page: panel.page,
            active,
            art: self.art,
        })
    }

    /// Close the classic panel, leaving the settings and key-binding
    /// screens in their tabbed form.
    pub(super) fn leave_classic_panel(&mut self) -> bool {
        self.settings.leave_classic();
        self.keybinds.leave_classic();
        self.classic_panel.take().is_some()
    }

    /// A button of the panel screen: another group, another page, or a
    /// screen the main page opens.
    pub(super) fn classic_panel_button(
        &mut self,
        index: usize,
        console: &mut ViewerConsole,
    ) -> MenuAction {
        let Some(panel) = self.classic_panel else {
            return MenuAction::None;
        };
        if let Some(tab) = panel::settings_tab_slot(index) {
            if panel.page.settings_tab() != Some(tab) || panel.page.is_setup_child() {
                let page = Page::of_settings_tab(tab);
                if let Some(entry) = page.opening_panel() {
                    let target = self.settings_return;
                    let carried = self.search_to_carry(panel.page);
                    self.open_classic_panel(console, page, entry, panel.frame, target);
                    if let Some(text) = carried {
                        self.carry_search(console, page, &text);
                    }
                }
            }
            return MenuAction::None;
        }
        let Some(slot) = panel.page.slots().get(index).filter(|slot| slot.enabled()) else {
            return MenuAction::None;
        };
        let target = self.settings_return;
        if slot.entry.panel().is_some() {
            self.open_classic_panel(console, panel.page, slot.entry, panel.frame, target);
            return MenuAction::None;
        }
        match slot.entry.outcome() {
            Outcome::Page(page) => {
                if let Some(entry) = page.opening_panel() {
                    self.open_classic_panel(console, page, entry, panel.frame, target);
                } else {
                    self.leave_classic_panel();
                    self.state.main_menu();
                    self.classic.show(page);
                }
                MenuAction::None
            }
            // The SJK UI's Settings on Graphics (the renderer rows), which
            // return to this panel, in game too.
            Outcome::Open(MainDestination::Renderer) => {
                self.open_renderer_from_panel(console, panel);
                MenuAction::None
            }
            Outcome::Open(destination) => {
                self.leave_classic_panel();
                self.open_main_destination(destination, console)
            }
            _ => MenuAction::None,
        }
    }

    /// The renderer settings from classic panel `panel`: the SJK UI's
    /// Settings on Graphics, which gathers the renderer rows; backing out
    /// shows `panel` again, in game too.
    pub(super) fn open_renderer_from_panel(
        &mut self,
        console: &ViewerConsole,
        panel: ClassicPanel,
    ) {
        let target = self.settings_return;
        self.leave_classic_panel();
        self.open_sjk_renderer(console, target);
        self.renderer_panel = Some(panel);
    }

    /// Tab on a panel: the next (1) or previous (-1) group of its list. On
    /// Settings the groups of both tabs follow each other, so the keyboard
    /// reaches the key bindings from the options and back.
    pub(super) fn classic_panel_cycle(&mut self, direction: i32, console: &ViewerConsole) {
        let Some(panel) = self.classic_panel else {
            return;
        };
        let pages: &[Page] = match panel.page {
            Page::Controls | Page::Setup => &[Page::Controls, Page::Setup],
            _ => std::slice::from_ref(&panel.page),
        };
        let groups: Vec<(Page, Entry)> = pages
            .iter()
            .flat_map(|page| page.slots().iter().map(move |slot| (*page, slot.entry)))
            .filter(|(_, entry)| entry.panel().is_some())
            .collect();
        let Some(current) = groups
            .iter()
            .position(|group| *group == (panel.page, panel.entry))
        else {
            return;
        };
        let (page, next) =
            groups[(current as i32 + direction).rem_euclid(groups.len() as i32) as usize];
        let target = self.settings_return;
        let carried = (page != panel.page)
            .then(|| self.search_to_carry(panel.page))
            .flatten();
        self.open_classic_panel(console, page, next, panel.frame, target);
        if let Some(text) = carried {
            self.carry_search(console, page, &text);
        }
    }

    /// Escape on the classic main menu: the opening page asks to quit, the
    /// others return to it.
    pub(super) fn cancel_classic(&mut self) {
        // Routed through the canvas for its "back" sound cue.
        let _ = self.ui.action(AbstractAction::Cancel);
        let page = self.classic.page().escape();
        self.classic.show(page);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keybind_editor::Category;
    use layout::Entry;

    fn focus(menu: &mut ClassicMain, entry: Entry) {
        let index = menu.page().index_of(entry).expect("entry on page");
        menu.select(index);
    }

    #[test]
    fn exit_asks_before_quitting() {
        let mut menu = ClassicMain::new();
        focus(&mut menu, Entry::Exit);
        assert_eq!(menu.outcome(), Some(Outcome::Page(Page::Quit)));
        menu.show(Page::Quit);
        // The quit page opens on No, so a second Enter does not quit.
        assert_eq!(menu.outcome(), Some(Outcome::Page(Page::Main)));
        focus(&mut menu, Entry::Yes);
        assert_eq!(menu.outcome(), Some(Outcome::Open(MainDestination::Quit)));
    }

    #[test]
    fn play_opens_the_multiplayer_page() {
        let mut menu = ClassicMain::new();
        assert_eq!(menu.outcome(), Some(Outcome::Page(Page::Play)));
        menu.show(Page::Play);
        // Retail focuses Solo Game, a local match with bots.
        assert_eq!(
            menu.outcome(),
            Some(Outcome::Open(MainDestination::CreateGame))
        );
        focus(&mut menu, Entry::JoinServer);
        assert_eq!(
            menu.outcome(),
            Some(Outcome::Open(MainDestination::Browser))
        );
        focus(&mut menu, Entry::PlayDemo);
        assert_eq!(menu.outcome(), Some(Outcome::Unavailable));
    }

    #[test]
    fn settings_opens_options_and_key_bindings_are_its_other_tab() {
        let mut menu = ClassicMain::new();
        focus(&mut menu, Entry::Settings);
        assert_eq!(menu.outcome(), Some(Outcome::Page(Page::Setup)));
        focus(&mut menu, Entry::Sjk);
        assert_eq!(menu.outcome(), Some(Outcome::Page(Page::Sjk)));
        assert_eq!(Page::Controls.settings_tab(), Some(0));
        assert_eq!(Page::Setup.settings_tab(), Some(1));
        assert_eq!(Page::Graphics.settings_tab(), Some(1));
        assert_eq!(Page::Gameplay.settings_tab(), Some(1));
        assert_eq!(Page::of_settings_tab(0), Page::Controls);
        menu.show(Page::Controls);
        assert_eq!(menu.outcome(), Some(Outcome::Keybinds(Category::Movement)));
        focus(&mut menu, Entry::ForcePowers);
        assert_eq!(menu.outcome(), Some(Outcome::Keybinds(Category::Force)));
        // The mouse options moved to OPTIONS, under GAMEPLAY.
        menu.show(Page::Gameplay);
        focus(&mut menu, Entry::MouseJoystick);
        assert_eq!(menu.outcome(), Some(Outcome::Settings("CONTROLS")));
    }

    #[test]
    fn setup_pages_open_settings_tabs() {
        let mut menu = ClassicMain::new();
        menu.show(Page::Setup);
        assert_eq!(
            menu.outcome(),
            Some(Outcome::Settings(crate::settings::FIRST_SETUP_CAPTION))
        );
        focus(&mut menu, Entry::Sound);
        assert_eq!(menu.outcome(), Some(Outcome::Settings("AUDIO")));
        // Retail's Mods and Defaults are gone: every OPTIONS group opens.
        assert!(Page::Setup.slots().iter().all(|slot| slot.enabled()));
    }

    #[test]
    fn the_graphics_page_holds_video_the_renderer_and_weather_and_returns_to_setup() {
        let mut menu = ClassicMain::new();
        menu.show(Page::Setup);
        focus(&mut menu, Entry::Graphics);
        assert_eq!(menu.outcome(), Some(Outcome::Page(Page::Graphics)));
        focus(&mut menu, Entry::Gameplay);
        assert_eq!(menu.outcome(), Some(Outcome::Page(Page::Gameplay)));
        assert_eq!(Page::Graphics.opening_panel(), Some(Entry::Video));
        let groups: Vec<_> = Page::Graphics
            .slots()
            .iter()
            .filter_map(|slot| slot.entry.panel())
            .skip(1)
            .collect();
        assert_eq!(
            groups,
            [0, 1, 2, 3].map(|tab| Panel::Renderer { tab }).to_vec(),
            "IMAGE, LIGHTING, SHADOWS and WEATHER, in the renderer tabs' order"
        );
        for page in [Page::Graphics, Page::Gameplay] {
            menu.show(page);
            focus(&mut menu, Entry::SetupBack);
            assert_eq!(menu.outcome(), Some(Outcome::Page(Page::Setup)));
            assert_eq!(page.escape(), Page::Setup);
        }
    }

    #[test]
    fn focus_wraps_and_ignores_foreign_tokens() {
        let mut menu = ClassicMain::new();
        menu.step(false);
        assert_eq!(menu.selection(), menu.slots().len() - 1);
        menu.step(true);
        assert_eq!(menu.selection(), 0);
        menu.select(500);
        assert_eq!(menu.selection(), 0);
        menu.show(Page::Quit);
        menu.reset();
        assert_eq!(menu, ClassicMain::new());
    }
}
