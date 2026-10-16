//! The `ui_menuStyle` setting: which layout the main menu uses.
//!
//! `sjk` is the SJK UI ([`super::sjk`]), SJK's own menus drawn over the live
//! map, and the default; `classic` follows the original Jedi Academy
//! multiplayer menus ([`super::classic`]). A profile saved with the earlier
//! default `classic` moves to `sjk` once (`ui_menuStyleDefaultVersion`, in the
//! console's start); a style chosen after that stays. The in-game menu follows
//! the same setting ([`crate::ingame_menu`]). Screens without an SJK UI
//! version yet use their classic one.

use super::ClientMenu;
use super::art::{self, ArtSet};
use crate::console::ViewerConsole;

/// Archived cvar naming the menu style.
pub(crate) const CVAR: &str = "ui_menuStyle";

/// Layout family of the main menu.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum MenuStyle {
    /// The SJK UI: SJK's own menus over the live map, in its own type. SJK's
    /// default.
    #[default]
    Sjk,
    /// Close to the retail menus in layout and flow, for players who know
    /// where things were in the original game.
    Classic,
}

impl MenuStyle {
    /// Values the settings screens offer, in [`MenuStyle`] order.
    pub(crate) const NAMES: [&'static str; 2] = ["sjk", "classic"];
    /// The `ui_menuStyle` value of the default style.
    pub(crate) const DEFAULT_NAME: &'static str = Self::NAMES[0];
    /// The default `ui_menuStyle` before the SJK UI became it: a saved value
    /// moves from it once.
    pub(crate) const OLD_DEFAULT_NAME: &'static str = Self::NAMES[1];

    /// Read the cvar value: `classic` (any case) or `1` selects the classic
    /// style; anything else, including a missing or mistyped value and the
    /// retired `modern` (or `0`), the default SJK UI, so a typo never leaves
    /// the player without a menu.
    pub(crate) fn from_cvar(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some(text) if text.eq_ignore_ascii_case("classic") || text == "1" => Self::Classic,
            _ => Self::Sjk,
        }
    }

    /// The map the menus stand on from the start: the SJK UI's main page is
    /// drawn over mp/duel6's temple courtyard; the classic style keeps mp/ffa3,
    /// whose camera routes, gate and player stage the menu backdrop has
    /// (`menu_backdrop`).
    pub(crate) fn boot_map(self) -> &'static str {
        match self {
            Self::Sjk => "maps/mp/duel6.bsp",
            Self::Classic => "maps/mp/ffa3.bsp",
        }
    }
}

impl ClientMenu {
    /// Apply the player's `ui_menuStyle`. A change puts the main menu back on
    /// its first page and entry, since selections do not carry across
    /// layouts.
    pub(crate) fn set_menu_style(&mut self, style: MenuStyle, console: &ViewerConsole) {
        if style != self.menu_style {
            let sjk_settings = self.sjk_settings_on_show();
            let first_setup = self.first_setup_on_show();
            self.menu_style = style;
            self.classic.reset();
            self.home.reset();
            // An open option panel stays as it is: the classic style and the
            // SJK UI share the panels, but for the SJK UI's own Settings,
            // which hands over to the classic panel.
            if sjk_settings {
                self.sjk_settings_to_classic(console);
            }
            // First setup picked to the SJK UI goes on as its pop-up.
            if style == MenuStyle::Sjk && first_setup {
                self.open_sjk_first_setup(console, crate::player_menu::ReturnTarget::MainMenu);
            }
        }
    }

    /// Whether First setup is on show over the main menu, in either style: the
    /// classic Setup panel's group, or the SJK UI's pop-up or category.
    fn first_setup_on_show(&self) -> bool {
        use super::classic::layout::Entry;
        if self.settings_return != crate::player_menu::ReturnTarget::MainMenu {
            return false;
        }
        match self.classic_panel {
            Some(panel) => panel.entry == Entry::FirstSetup,
            None => {
                *self.state.phase() == super::ClientPhase::Settings
                    && self.settings.on_first_setup()
            }
        }
    }

    /// The retail artwork the classic pages can draw this frame; the player
    /// screen follows the style and gets the same pieces. The SJK UI's player
    /// screen stands the model on the menu map's stage; opened from a game,
    /// where there is none, it shows the model in a live preview of its own.
    pub(crate) fn set_menu_art(&mut self, art: ArtSet) {
        self.art = art;
        let sjk = self.menu_style == MenuStyle::Sjk;
        self.player.set_style(!sjk, art);
        self.player.set_sjk(sjk);
    }
}

impl crate::GpuState {
    /// Apply `ui_menuStyle` to the main and in-game menus. The player's
    /// retail menu artwork, which the classic pages (the SJK UI borrows those
    /// it has no version of) and the classic console's classic+ command
    /// browser draw, is decoded (once, on a worker) and uploaded when ready;
    /// the menus and the console's browser are told which pieces they can draw.
    pub(crate) fn sync_menu_style(&mut self) {
        let Some(console) = &self.console else {
            return;
        };
        let style = MenuStyle::from_cvar(console.text_value(CVAR));
        // SJK's emblem is on every style's main page.
        crate::menu::emblem::request();
        self.ui_shapes.install_emblem(&self.device, &self.queue);
        self.ui_shapes.install_medals(&self.device, &self.queue);
        self.ui_shapes.install_cards(&self.device, &self.queue);
        if let Some(vfs) = &self.vfs {
            art::request(vfs);
        }
        self.ui_shapes.install_menu_art(&self.device, &self.queue);
        let art = self.ui_shapes.menu_art();
        if let Some(menu) = &mut self.client_menu {
            menu.set_menu_style(style, console);
            menu.set_menu_art(art);
        }
        self.in_game_menu.set_style(style, art);
        let sjk = style == MenuStyle::Sjk;
        // The report and note dialog is the SJK UI's card with its menus.
        self.text_dialog.set_look(
            match style {
                MenuStyle::Sjk => crate::text_dialog::Look::Sjk,
                MenuStyle::Classic => crate::text_dialog::Look::Classic,
            },
            art,
        );
        // So is the new medal pop-up, and the classic+ one with the classic menus.
        self.medal_popup.set_style(style, art);
        if let Some(console) = &mut self.console {
            // What's new, Update, Identity and Credits have the SJK UI's own
            // look with its menus, and their classic+ one with the classic.
            console.set_browser_art(art);
            console.set_page_art(art);
            console.set_sjk_pages(sjk);
        }
        // The first start's First setup waits for the style, so it opens in the right one.
        if let (Some(menu), Some(console)) = (&mut self.client_menu, &mut self.console) {
            menu.offer_first_setup(console);
        }
        self.sync_classic_loading();
    }

    /// Feed the classic loading screen (and the SJK UI's, which shares its
    /// state) this frame's progress: the join's destination world (the
    /// portal) from the menu, or the map change's load on a server world.
    fn sync_classic_loading(&mut self) {
        use super::classic::loading::{WorldStage, progress};
        let Some(menu) = &self.client_menu else {
            return;
        };
        if !menu.is_loading_screen() {
            return;
        }
        let stage = if self.is_menu_world {
            self.portal.stage()
        } else if self.world_install_task.is_some() {
            Some(WorldStage::Building)
        } else if self.world_load_task.is_some() {
            Some(WorldStage::Parsing)
        } else {
            None
        };
        let joined = self.resident.session.is_some() || self.live_session.is_some();
        let map = menu.loading_map().to_owned();
        let (subject, ticks) = progress(&map, stage, joined);
        // Before the session names its content, the menu world builds a
        // preview of the browser's map, which is built again for the session.
        let session_world = !self.is_menu_world || self.portal.for_session();
        if let Some(menu) = &mut self.client_menu {
            let loading = menu.loading_mut();
            loading.set_progress(&subject, ticks);
            loading.set_world(stage.filter(|_| session_world), joined);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_needs_an_explicit_value() {
        assert_eq!(MenuStyle::from_cvar(None), MenuStyle::Sjk);
        assert_eq!(MenuStyle::from_cvar(Some("")), MenuStyle::Sjk);
        // The retired modern style is the SJK UI.
        assert_eq!(MenuStyle::from_cvar(Some("modern")), MenuStyle::Sjk);
        assert_eq!(MenuStyle::from_cvar(Some(" Modern ")), MenuStyle::Sjk);
        assert_eq!(MenuStyle::from_cvar(Some("0")), MenuStyle::Sjk);
        assert_eq!(MenuStyle::from_cvar(Some("modrn")), MenuStyle::Sjk);
        assert_eq!(MenuStyle::from_cvar(Some("clasic")), MenuStyle::Sjk);
        assert_eq!(MenuStyle::from_cvar(Some("classic")), MenuStyle::Classic);
        assert_eq!(MenuStyle::from_cvar(Some(" Classic ")), MenuStyle::Classic);
        assert_eq!(MenuStyle::from_cvar(Some("1")), MenuStyle::Classic);
    }

    /// A new profile starts on the SJK UI; a profile saved with the old
    /// default `classic` moves to it once, and a style chosen after that
    /// stays start after start. A saved `modern`, a retired style, is reset.
    #[test]
    fn a_saved_old_default_moves_to_the_sjk_ui_once() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.cfg");
        let console = ViewerConsole::new(path.clone()).unwrap();
        assert_eq!(console.text_value(CVAR), Some("sjk"));
        drop(console);
        // Every profile saved the old default `classic`.
        std::fs::write(&path, "seta ui_menuStyle \"classic\"\n").unwrap();
        let mut console = ViewerConsole::new(path.clone()).unwrap();
        assert_eq!(console.text_value(CVAR), Some("sjk"));
        // Classic chosen after the move stays.
        assert!(console.set_cvar(CVAR, "classic"));
        drop(console);
        let console = ViewerConsole::new(path.clone()).unwrap();
        assert_eq!(console.text_value(CVAR), Some("classic"));
        assert_eq!(
            MenuStyle::from_cvar(console.text_value(CVAR)),
            MenuStyle::Classic
        );
        drop(console);
        // The modern style is retired: a saved one starts on the SJK UI.
        std::fs::write(
            &path,
            "seta ui_menuStyle \"modern\"\nseta ui_menuStyleDefaultVersion \"1\"\n",
        )
        .unwrap();
        let console = ViewerConsole::new(path).unwrap();
        assert_eq!(console.text_value(CVAR), Some("sjk"));
    }

    #[test]
    fn offered_names_parse_in_order() {
        let parsed = MenuStyle::NAMES.map(|name| MenuStyle::from_cvar(Some(name)));
        assert_eq!(parsed, [MenuStyle::Sjk, MenuStyle::Classic]);
        assert_eq!(MenuStyle::from_cvar(Some(" SJK ")), MenuStyle::Sjk);
        assert_eq!(
            MenuStyle::from_cvar(Some(MenuStyle::DEFAULT_NAME)),
            MenuStyle::default()
        );
        assert_eq!(MenuStyle::default(), MenuStyle::Sjk);
        assert_eq!(
            MenuStyle::from_cvar(Some(MenuStyle::OLD_DEFAULT_NAME)),
            MenuStyle::Classic
        );
    }
}
