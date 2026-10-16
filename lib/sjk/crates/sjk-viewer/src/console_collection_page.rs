//! Console side of the Collection page (see `collection_panel.rs`): opening it on a
//! tab, setting `cg_saberSkin` when it equips or unequips a shader and asking the viewer
//! to light the Illuminate holocron or put it out (the lit state is the viewer's,
//! mirrored here for the page: [`ViewerConsole::set_illuminate_lit`]), and routing keys
//! and pointer events to it, as for the Staff page. It is drawn in the SJK UI's look
//! whatever the menu style (`console_sjk_pages.rs`).

use super::collection_panel::{Backstage, Inputs, PanelAction, StageWish};
use super::*;
use crate::player_menu::ReturnTarget;
use crate::profile_hub::Tab;
use crate::unlockables::{Holdings, SABER_SKIN_CVAR};
use sjk_ui::InputEvent;

/// The `collection` command.
pub(crate) const COLLECTION_COMMAND: &str = "collection";
pub(crate) const COLLECTION_HELP: &str =
    "Open your SJK collection: medals, achievements, shaders, toys and nameplates";
/// The `achievements` command.
pub(crate) const ACHIEVEMENTS_COMMAND: &str = "achievements";
pub(crate) const ACHIEVEMENTS_HELP: &str = "Open the Collection on your achievements";
/// The `unlockables` command.
pub(crate) const UNLOCKABLES_COMMAND: &str = "unlockables";
pub(crate) const UNLOCKABLES_HELP: &str =
    "Open the Collection on its shaders: saber looks you own or can unlock";

impl ViewerConsole {
    /// `collection`, `achievements` or `unlockables` in the classic menus: show the page
    /// on its own on `tab` (`None`: the tab shown last), or close it when it shows it.
    pub(crate) fn toggle_collection_panel(&mut self, tab: Option<Tab>) {
        let tab = tab.unwrap_or(self.collection_panel.tab());
        if self.open && self.collection_panel.is_open() && self.collection_panel.tab() == tab {
            self.close_collection_panel();
            return;
        }
        let owns_console = !self.open;
        self.show_collection_panel(tab, owns_console, false, ReturnTarget::MainMenu);
    }

    /// Show the page on its own on the tab shown last (the classic menus).
    pub(crate) fn open_collection_panel(&mut self) {
        let tab = self.collection_panel.tab();
        let owns_console = !self.open;
        self.show_collection_panel(tab, owns_console, false, ReturnTarget::MainMenu);
    }

    /// The Collection's tab on show, or shown last (Medals the first time).
    pub(crate) fn collection_tab(&self) -> Tab {
        if self.holocrons_last {
            Tab::Holocrons
        } else {
            self.collection_panel.tab()
        }
    }

    /// Show the page on `tab`, as the SJK UI's Collection screen returning to `back`
    /// (`hub`) or on its own; closing it closes the console too when `owns_console`.
    pub(super) fn show_collection_panel(
        &mut self,
        tab: Tab,
        owns_console: bool,
        hub: bool,
        back: ReturnTarget,
    ) {
        if !self.open {
            self.set_open(true);
        }
        self.browser.close();
        self.debug_panel.close();
        self.changelog.close();
        self.update_panel.close();
        self.config_import.close();
        self.credits.close();
        self.identity_panel.close();
        self.profile_panel.close();
        self.staff_panel.close();
        self.sjk_chat_panel.close();
        self.holocrons_panel.close();
        self.dead_key.settle();
        self.collection_panel.open(tab, owns_console, hub, back);
    }

    fn close_collection_panel(&mut self) {
        if self.collection_panel.close() {
            self.set_open(false);
        }
    }

    fn collection_panel_action(&mut self, action: PanelAction) {
        match action {
            PanelAction::None => {}
            PanelAction::Close => self.close_collection_panel(),
            PanelAction::Wear(id) => crate::unlockables::wear(self, id),
            PanelAction::SkinOption { skin, option, on } => {
                let setting = self
                    .text_cvar(crate::saber_skin_options::CVAR)
                    .unwrap_or_default()
                    .to_owned();
                let _ = self.set_cvar(
                    crate::saber_skin_options::CVAR,
                    &crate::saber_skin_options::with(&setting, skin, &option, on),
                );
            }
            PanelAction::RevealCards => {
                let _ = crate::player_identity::reveal_cards();
            }
            PanelAction::ShaderView(grid) => {
                self.set_cvar(
                    super::collection_panel::SHADER_VIEW_CVAR,
                    if grid { "1" } else { "0" },
                );
            }
            PanelAction::Illuminate(on) => {
                self.pending_illuminate = Some(on);
                // The page shows it at once; the viewer applies it next frame.
                self.illuminate_lit = on;
            }
            PanelAction::Hub(tab) => {
                // A tab of another page: the Holocrons page.
                let back = self.collection_panel.back();
                if self.collection_panel.is_hub() {
                    self.open_profile_hub_page(tab, back);
                } else if tab == Tab::Holocrons {
                    let owns_console = self.collection_panel.close();
                    self.show_holocrons_panel(owns_console);
                    self.holocrons_last = true;
                }
            }
        }
    }

    /// Whether the Illuminate holocron is lit, as the viewer last said.
    fn illuminate_lit(&self) -> bool {
        self.illuminate_lit
    }

    /// The viewer says whether the holocron is lit, once a frame, for the Toys tab.
    pub(crate) fn set_illuminate_lit(&mut self, lit: bool) {
        self.illuminate_lit = lit;
    }

    /// The Toys tab asked to light the holocron (`true`) or put it out, once.
    pub(crate) fn take_illuminate_request(&mut self) -> Option<bool> {
        self.pending_illuminate.take()
    }

    /// Give a pressed key to the open page; false when the page is closed.
    pub(super) fn collection_panel_key(&mut self, event: &KeyEvent) -> bool {
        if !self.collection_panel.is_open() {
            return false;
        }
        let illuminate = self.illuminate_lit();
        let action = self
            .collection_panel
            .handle_key(event, self.shift, self.control, illuminate);
        self.collection_panel_action(action);
        true
    }

    /// Give a pointer event to the open page; false when the page is closed.
    pub(super) fn collection_panel_pointer(&mut self, event: InputEvent) -> bool {
        if !self.collection_panel.is_open() {
            return false;
        }
        let illuminate = self.illuminate_lit();
        let action = self.collection_panel.handle_pointer(event, illuminate);
        self.collection_panel_action(action);
        true
    }

    /// What the page wants the stage to show this frame, while it shows.
    pub(crate) fn collection_stage_wish(&self) -> Option<StageWish> {
        if let Some(wish) = self.model_viewer_wish() {
            return Some(wish);
        }
        (self.open && !self.config_import.is_open())
            .then(|| self.collection_panel.stage_wish())
            .flatten()
    }

    /// Where the model stands behind the page this frame.
    pub(crate) fn set_collection_backstage(&mut self, backstage: Backstage) {
        self.model_viewer.set_backstage(backstage);
        self.collection_panel.set_backstage(backstage);
    }

    /// The stock blade's colour, from `color1`, for its swatch.
    fn stock_blade(&self) -> sjk_ui::Color {
        let index = self
            .text_value("color1")
            .map(str::trim)
            .and_then(|value| value.get(..1))
            .and_then(|digit| digit.parse::<u8>().ok())
            .unwrap_or(4);
        crate::player_menu::saber_color(index, [255, 255, 255])
    }

    /// Draw the page with its text to `target`.
    pub(super) fn append_collection_panel(
        &mut self,
        target: crate::menu::sjk::TextTarget<'_>,
        viewport: [f32; 2],
    ) {
        self.collection_panel.follow_skins();
        // Copied out so the page can borrow itself mutably while it draws.
        let setting = self
            .text_cvar(SABER_SKIN_CVAR)
            .unwrap_or_default()
            .to_owned();
        let name = self.text_value("name").unwrap_or_default().to_owned();
        let illuminate = self.illuminate_lit();
        let stock = self.stock_blade();
        let grid = self.integer_cvar(super::collection_panel::SHADER_VIEW_CVAR) == Some(1);
        let options = self
            .text_cvar(crate::saber_skin_options::CVAR)
            .unwrap_or_default()
            .to_owned();
        let enabled = self.bool_cvar("cl_identity") == Some(true);
        let snapshot =
            crate::player_identity::collection_snapshot(crate::debug_collectibles::wanted(self));
        let held = snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.me.as_ref())
            .map(|me| me.achievements.clone())
            .unwrap_or_default();
        let standings = crate::achievements::standings(&held);
        #[cfg(test)]
        if let Some(unlocks) = self.collection_panel.preview.take() {
            let profile = self.profile_preview_snapshot();
            let standings = self.profile_preview_standings().unwrap_or(standings);
            let inputs = Inputs {
                enabled: true,
                snapshot: profile.as_ref(),
                holdings: Holdings::Known(&unlocks),
                setting: &setting,
                standings: &standings,
                illuminate,
                name: &name,
                stock,
                grid,
                options: &options,
            };
            self.collection_panel.append_sjk(&inputs, target, viewport);
            self.collection_panel.preview = Some(unlocks);
            return;
        }
        let inputs = Inputs {
            enabled,
            snapshot: snapshot.as_ref(),
            holdings: Holdings::of(enabled, snapshot.as_ref()),
            setting: &setting,
            standings: &standings,
            illuminate,
            name: &name,
            stock,
            grid,
            options: &options,
        };
        self.collection_panel.append_sjk(&inputs, target, viewport);
    }

    /// Show `unlocks` (`None`: the live ones) whenever the page opens, its swatches drawn
    /// at `seconds`, for a world shot.
    #[cfg(test)]
    pub(crate) fn preview_collection(
        &mut self,
        unlocks: Option<Vec<sjk_identity::Unlock>>,
        seconds: f32,
    ) {
        self.collection_panel.preview = unlocks;
        self.collection_panel.shot_seconds = Some(seconds);
    }

    /// Open the page on its own on `tab` with `unlocks` in place of the own profile's,
    /// its swatches drawn at `seconds`, for a world shot.
    #[cfg(test)]
    pub(crate) fn preview_collection_alone(
        &mut self,
        tab: Tab,
        unlocks: Option<Vec<sjk_identity::Unlock>>,
        seconds: f32,
    ) {
        self.toggle_collection_panel(Some(tab));
        self.preview_collection(unlocks, seconds);
    }

    /// Show `cards` and `credits` on the Cards tab in place of the own profile's, for a
    /// world shot.
    #[cfg(test)]
    pub(crate) fn preview_cards(&mut self, cards: Vec<sjk_identity::CardStack>, credits: u64) {
        self.collection_panel.preview_cards = Some((cards, credits));
    }

    /// Choose the Cards tab's kind `index`, for a world shot.
    #[cfg(test)]
    pub(crate) fn collection_card_for_shot(&mut self, index: usize) {
        self.collection_panel.choose_card_for_shot(index);
    }

    /// Tilt the Cards tab's card `key` (0 up close) as though the pointer rested at `at`
    /// on it, for a world shot.
    #[cfg(test)]
    pub(crate) fn collection_tilt_for_shot(&mut self, key: usize, at: [f32; 2]) {
        self.collection_panel.tilt_card_for_shot(key, at);
    }

    /// Show the hover card of the Collection tab's thing `index`, for a world shot.
    #[cfg(test)]
    pub(crate) fn collection_hover_for_shot(&mut self, index: Option<usize>) {
        self.collection_panel.hover_for_shot(index);
    }

    /// Choose the Collection's shader row `row` (0 the stock blade), for a world shot.
    #[cfg(test)]
    pub(crate) fn collection_shader_for_shot(&mut self, row: usize) {
        self.collection_panel.choose_shader_for_shot(row);
    }

    /// Whether the Profile page's, the Collection page's or the Holocrons page's last
    /// frame ran out of room on its canvas, for the world shots.
    #[cfg(test)]
    pub(crate) fn profile_pages_overflowed(&self) -> bool {
        self.profile_panel.overflowed()
            || self.collection_panel.overflowed()
            || self.holocrons_panel.overflowed()
    }

    /// The page's draw list while it is shown.
    pub(super) fn collection_panel_draw_list(&self) -> Option<&sjk_ui::DrawList> {
        (self.open && self.collection_panel.is_open()).then(|| self.collection_panel.draw_list())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_opens_alone_and_its_actions_set_the_cvars_and_ask_for_the_holocron() {
        let directory = tempfile::tempdir().unwrap();
        let mut console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        console.open_profile_panel();
        console.toggle_collection_panel(Some(Tab::Shaders));
        assert!(console.collection_panel.is_open());
        assert!(!console.profile_panel.is_open());
        assert_eq!(console.collection_tab(), Tab::Shaders);
        assert!(console.sjk_page_open(), "drawn in the SJK UI's look");
        assert!(console.covers_frame());
        assert!(console.collection_stage_wish().is_some());
        console.collection_panel_action(PanelAction::Wear("saber_sun"));
        assert_eq!(console.text_cvar(SABER_SKIN_CVAR).unwrap(), "saber_sun");
        console.collection_panel_action(PanelAction::Wear(""));
        assert_eq!(console.text_cvar(SABER_SKIN_CVAR).unwrap(), "");
        // The holocron's state is the viewer's: the page asks, shows the answer at
        // once and the viewer takes the request once.
        console.set_illuminate_lit(false);
        assert_eq!(console.take_illuminate_request(), None);
        console.collection_panel_action(PanelAction::Illuminate(true));
        assert!(console.illuminate_lit());
        assert_eq!(console.take_illuminate_request(), Some(true));
        assert_eq!(console.take_illuminate_request(), None);
        console.collection_panel_action(PanelAction::Illuminate(false));
        assert!(!console.illuminate_lit());
        assert_eq!(console.take_illuminate_request(), Some(false));
        // It was opened with the console already open (the Profile page's).
        console.collection_panel_action(PanelAction::Close);
        assert!(!console.collection_panel.is_open());
        assert!(console.is_open());
        assert_eq!(console.collection_stage_wish(), None);
        // From a closed console: again on the tab shown last; then it closes the
        // console it took over.
        console.set_open(false);
        console.toggle_collection_panel(None);
        assert_eq!(console.collection_tab(), Tab::Shaders);
        assert!(console.is_open());
        console.toggle_collection_panel(Some(Tab::Shaders));
        assert!(!console.is_open(), "it closes the console it took over");
    }
}
