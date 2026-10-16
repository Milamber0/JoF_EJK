//! Classic profile pages (`ui_menuStyle classic`): the retail `player`,
//! `player2` and `saber` menus of the main menu, and their in-game windows
//! `ingame_player`, `ingame_player2` and `ingame_saber`, laid out from the
//! retail `ui/jamp` item rectangles on the 640x480 menu canvas.
//!
//! They edit the same drafts as the SJK UI's Character and write them the
//! same way (immediately), so the two styles cannot disagree. What differs
//! is the presentation and the flow: the profile page's head grid, Custom
//! leading to character creation, and Apply leading on to lightsaber
//! creation, as retail had it. Entry geometry and focus order are in
//! [`layout`], drawing in [`view`], pointer routing in [`pointer`].
//!
//! Retail drew a live 3D model on character creation and a spinning saber on
//! lightsaber creation; so does SJK ([`crate::menu_stage::preview`]), with the
//! model's portrait and a drawn blade standing in until a frame is drawn.
//!
//! SJK adds two pages retail's main menu lacked: the Force page (retail's
//! in-game `ingame_playerforce`, on both frames, editing the same draft as
//! the SJK UI's Force tab), and JoF EJK's cosmetics window for hats and capes.
//! The profile page reaches both and sums up the Force profile.

mod cosmetics_page;
pub(super) mod force_page;
pub(super) mod layout;
mod pointer;
mod saber_rgb;
mod view;

use super::controller::wrap;
use super::saber::SaberStyle;
use super::*;
use crate::menu::art::ArtSet;
use crate::menu::classic::layout::Page as MainPage;
pub(super) use layout::GRID_COLUMNS;
use layout::Item;
use sjk_client::{CosmeticSlot, ForceSide};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// The three retail profile screens.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum ClassicPage {
    /// `player` / `ingame_player`: name, team colour and the head grid.
    #[default]
    Player,
    /// `player2` / `ingame_player2`: species, skin tint and parts.
    Character,
    /// `saber` / `ingame_saber`: saber type, hilts and blade colours.
    Saber,
    /// `ingame_playerforce`: side, powers and their levels.
    Force,
    /// JoF EJK's `ingame_cosmetics`: hats and capes.
    Cosmetics,
}

/// Where the pages are drawn: full screen from the main menu, or as the
/// retail in-game window over a match.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Frame {
    Full,
    InGame,
}

/// Retail blade colour swatches, left to right, as `color1` indices.
pub(super) const BLADE_SWATCHES: [u8; 6] = [4, 3, 1, 5, 2, 0];

/// Classic page, focus and list scroll state of the player screen.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ClassicState {
    pub(super) page: ClassicPage,
    /// Focused entry: an index into the page's [`layout::items`].
    pub(super) focus: usize,
    /// Which part list character creation shows: 0 head, 1 torso, 2 legs.
    pub(super) part_axis: usize,
    /// First visible column of the part and tint lists.
    pub(super) part_scroll: usize,
    pub(super) tint_scroll: usize,
    /// First visible row of each hilt list.
    pub(super) hilt_scroll: [usize; 2],
    /// Retail artwork that can be drawn this frame.
    pub(super) art: ArtSet,
}

impl PlayerMenu {
    /// Apply `ui_menuStyle` to this screen.
    pub(crate) fn set_style(&mut self, classic: bool, art: ArtSet) {
        self.classic.art = art;
        if classic != self.classic_style {
            self.classic_style = classic;
            self.selected = 0;
            self.name_editing = false;
            if classic {
                self.show_classic(ClassicPage::Player);
            }
        }
    }

    /// Whether the classic pages are in use.
    pub(crate) fn is_classic(&self) -> bool {
        self.classic_style
    }

    /// Draw the screen in the SJK UI's view (`sjk`), or not.
    pub(crate) fn set_sjk(&mut self, sjk: bool) {
        self.sjk = sjk;
    }

    /// Whether the SJK UI's view is in use.
    pub(crate) fn is_sjk(&self) -> bool {
        self.sjk && !self.classic_style
    }

    /// The live model the page on show wants: character creation's, walking
    /// in place as retail's did (`BOTH_WALK1`), the cosmetics window's,
    /// standing as JoF's does (`BOTH_STAND1`), and lightsaber creation's
    /// sabers alone, lit and turning, as retail spun the hilt.
    pub(crate) fn model_preview(&self) -> Option<ModelPreview> {
        if self.is_sjk() {
            return self.sjk_model_preview();
        }
        if !self.classic_style {
            return None;
        }
        let page = self.classic.page;
        let (stance, sabers) = match page {
            ClassicPage::Character => ("BOTH_WALK1", false),
            ClassicPage::Cosmetics => ("BOTH_STAND1", false),
            ClassicPage::Saber => ("BOTH_STAND2", true),
            _ => return None,
        };
        Some(ModelPreview {
            area: super::PreviewArea::Classic(layout::preview_rect(page, self.frame())?),
            stance,
            sabers,
            pose: None,
            shell: None,
            effect: None,
            showcase: sabers,
            room: 1.0,
            angle: None,
        })
    }

    /// Whether the renderer has a preview frame to show.
    pub(crate) fn set_preview_ready(&mut self, ready: bool) {
        self.preview_ready = ready;
    }

    pub(super) fn frame(&self) -> Frame {
        match self.return_target {
            ReturnTarget::MainMenu => Frame::Full,
            ReturnTarget::InGame => Frame::InGame,
        }
    }

    /// Entries of the page on show, in focus order.
    pub(super) fn classic_items(&self) -> &'static [Item] {
        layout::items(
            self.classic.page,
            self.frame(),
            self.saber.style() == SaberStyle::Dual,
            self.force.allocation().side == ForceSide::Dark,
        )
    }

    pub(super) fn classic_focused(&self) -> Option<Item> {
        self.classic_items().get(self.classic.focus).copied()
    }

    /// Show `page` with focus on its first entry after the navigation row.
    pub(super) fn show_classic(&mut self, page: ClassicPage) {
        self.numeric = None;
        self.name_editing = false;
        self.search_editing = false;
        self.force_templates.editing = false;
        if page == ClassicPage::Force {
            self.force_templates.list.ensure(self.icon_vfs.as_ref());
            self.force_templates.note = None;
        }
        self.classic.page = page;
        let items = self.classic_items();
        self.classic.focus = items.iter().position(|item| !item.is_nav()).unwrap_or(0);
        if page == ClassicPage::Character {
            self.enter_character_creation();
        }
    }

    /// Character creation edits a species model; coming from an ordinary
    /// character, the first species is put on, as retail's Custom did.
    fn enter_character_creation(&mut self) {
        if matches!(self.choice, Some(Choice::Species(_))) {
            return;
        }
        let Some(first) = self
            .catalog()
            .filter(|catalog| !catalog.species.is_empty())
            .map(|catalog| catalog.characters.len())
        else {
            return;
        };
        self.select_choice(first);
        self.classic_dirty = true;
    }

    /// The species being edited on character creation.
    pub(super) fn current_species(&self) -> Option<usize> {
        match self.choice {
            Some(Choice::Species(index)) => Some(index),
            _ => None,
        }
    }

    fn cycle_species(&mut self, direction: isize) {
        let Some((characters, count)) = self
            .catalog()
            .map(|catalog| (catalog.characters.len(), catalog.species.len()))
        else {
            return;
        };
        if count == 0 {
            return;
        }
        let next = match self.current_species() {
            Some(index) => wrap(index, direction, count),
            None => 0,
        };
        self.select_choice(characters + next);
    }

    /// Choose the Force side and keep focus on its card (the side column's
    /// powers change under it).
    fn choose_side(&mut self, side: ForceSide) {
        self.force.set_side(side);
        let card = match side {
            ForceSide::Light => Item::SideLight,
            ForceSide::Dark => Item::SideDark,
        };
        if let Some(index) = self.classic_items().iter().position(|item| *item == card) {
            self.classic.focus = index;
        }
    }

    /// Step the focused entry left or right and write the change.
    pub(super) fn classic_adjust(&mut self, console: &mut ViewerConsole, direction: isize) {
        let Some(item) = self.classic_focused() else {
            return;
        };
        match item {
            Item::SideLight | Item::SideDark => {
                self.choose_side(if direction < 0 {
                    ForceSide::Light
                } else {
                    ForceSide::Dark
                });
                return;
            }
            // Browsing the templates loads each into the draft.
            Item::Templates => {
                self.step_template(direction);
                return;
            }
            Item::TemplateName | Item::TemplateSave => return,
            // The Force page edits a draft; only Apply Powers writes it.
            Item::Power(index) => {
                self.force.step(usize::from(index), direction > 0);
                return;
            }
            // Left and right walk the button row.
            Item::ForceReset | Item::ForceDiscard | Item::ForceApply => {
                self.classic.focus = self
                    .classic
                    .focus
                    .saturating_add_signed(direction)
                    .min(self.classic_items().len() - 1);
                if !matches!(
                    self.classic_focused(),
                    Some(Item::ForceReset | Item::ForceDiscard | Item::ForceApply)
                ) {
                    self.classic.focus = self
                        .classic
                        .focus
                        .saturating_add_signed(-direction)
                        .min(self.classic_items().len() - 1);
                }
                return;
            }
            Item::Hats => {
                self.cosmetics.move_cursor(CosmeticSlot::Hat, direction);
                return;
            }
            Item::Capes => {
                self.cosmetics.move_cursor(CosmeticSlot::Cape, direction);
                return;
            }
            Item::CosmeticsShow => {
                self.cosmetics.cycle_visibility(console, direction);
                return;
            }
            Item::Team => self.cycle_team(direction),
            Item::Models => {
                self.cycle_model(direction);
            }
            Item::Species => self.cycle_species(direction),
            Item::Tints => self.cycle_variant(3, direction),
            Item::PartHead | Item::PartTorso | Item::PartLegs => {
                self.classic.part_axis = wrap(self.classic.part_axis, direction, 3);
                self.classic.part_scroll = 0;
                return;
            }
            Item::Parts => self.cycle_variant(self.classic.part_axis, direction),
            Item::Single | Item::Dual | Item::Staff => {
                let order = [SaberStyle::Single, SaberStyle::Dual, SaberStyle::Staff];
                let index = order
                    .iter()
                    .position(|style| *style == self.saber.style())
                    .unwrap_or(0);
                let style = order[wrap(index, direction, order.len())];
                self.set_saber_style(console, style);
                return;
            }
            Item::Hilts | Item::Hilts2 => {
                let catalog = catalog_of(&self.loader);
                let row = if item == Item::Hilts {
                    super::rows::SaberRow::Hilt
                } else {
                    super::rows::SaberRow::SecondHilt
                };
                self.saber.adjust(row, direction, catalog);
                self.saber.apply(console);
                return;
            }
            Item::Channel(index) => {
                let catalog = catalog_of(&self.loader);
                self.saber.adjust(saber_rgb::row(index), direction, catalog);
                self.saber.apply(console);
                return;
            }
            Item::Blades | Item::Blades2 => {
                let second = item == Item::Blades2;
                let current = BLADE_SWATCHES
                    .iter()
                    .position(|index| *index == self.saber.color(second))
                    .unwrap_or(0);
                let next = BLADE_SWATCHES[wrap(current, direction, BLADE_SWATCHES.len())];
                self.saber.select_color(second, next);
                self.saber.apply(console);
                return;
            }
            _ => return,
        }
        self.grid_follow = true;
        self.apply(console);
    }

    /// Enter or a click on the focused entry.
    pub(super) fn classic_activate(&mut self, console: &mut ViewerConsole) -> PlayerMenuResult {
        let Some(item) = self.classic_focused() else {
            return PlayerMenuResult::None;
        };
        let frame = self.frame();
        match item {
            Item::NavPlay => PlayerMenuResult::ClassicPage(MainPage::Play),
            Item::NavControls => PlayerMenuResult::ClassicPage(MainPage::Setup),
            Item::NavSetup => PlayerMenuResult::ClassicPage(MainPage::Sjk),
            Item::NavProfile => {
                if self.classic.page != ClassicPage::Player {
                    self.show_classic(ClassicPage::Player);
                }
                PlayerMenuResult::None
            }
            Item::Exit => PlayerMenuResult::ClassicPage(MainPage::Quit),
            Item::Name => {
                self.name_before_edit.clone_from(&self.draft.name);
                self.search_editing = false;
                self.name_editing = true;
                PlayerMenuResult::None
            }
            Item::Search => {
                self.begin_search();
                PlayerMenuResult::None
            }
            Item::Custom => {
                self.show_classic(ClassicPage::Character);
                self.write_if_dirty(console);
                PlayerMenuResult::None
            }
            Item::SaberButton => {
                self.show_classic(ClassicPage::Saber);
                PlayerMenuResult::None
            }
            Item::ForceButton => {
                self.show_classic(ClassicPage::Force);
                PlayerMenuResult::None
            }
            Item::CosmeticsButton => {
                self.cosmetics.open(console, self.icon_vfs.as_ref());
                self.show_classic(ClassicPage::Cosmetics);
                PlayerMenuResult::None
            }
            Item::SideLight => {
                self.choose_side(ForceSide::Light);
                PlayerMenuResult::None
            }
            Item::SideDark => {
                self.choose_side(ForceSide::Dark);
                PlayerMenuResult::None
            }
            // A click raises a power a level, as retail's did.
            Item::Power(index) => {
                self.force.step(usize::from(index), true);
                PlayerMenuResult::None
            }
            Item::ForceReset => {
                self.force.reset();
                PlayerMenuResult::None
            }
            Item::Templates => {
                if self.template_row().is_none() {
                    self.step_template(1);
                }
                PlayerMenuResult::None
            }
            Item::TemplateName => {
                self.begin_template_name();
                PlayerMenuResult::None
            }
            Item::TemplateSave => {
                self.save_template();
                PlayerMenuResult::None
            }
            Item::ForceDiscard => {
                self.force.discard();
                PlayerMenuResult::None
            }
            // `applyjoin`: write the powers and go back to the profile.
            Item::ForceApply => {
                self.force.apply(console);
                self.show_classic(ClassicPage::Player);
                PlayerMenuResult::None
            }
            Item::Hats | Item::Capes => {
                let slot = if item == Item::Hats {
                    CosmeticSlot::Hat
                } else {
                    CosmeticSlot::Cape
                };
                let row = self.cosmetics.cursor[slot.index()];
                self.cosmetics.toggle(console, slot, row);
                PlayerMenuResult::None
            }
            Item::CosmeticsShow => {
                self.cosmetics.cycle_visibility(console, 1);
                PlayerMenuResult::None
            }
            Item::CosmeticsClear => {
                self.cosmetics.clear(console);
                PlayerMenuResult::None
            }
            Item::PartHead | Item::PartTorso | Item::PartLegs => {
                self.classic.part_axis = item.part_axis().unwrap_or(0);
                self.classic.part_scroll = 0;
                PlayerMenuResult::None
            }
            Item::Single => self.style_result(console, SaberStyle::Single),
            Item::Dual => self.style_result(console, SaberStyle::Dual),
            Item::Staff => self.style_result(console, SaberStyle::Staff),
            Item::Back => {
                self.show_classic(ClassicPage::Player);
                PlayerMenuResult::None
            }
            // Changes are already written but a Force draft; Apply writes
            // that too and moves on as retail's did.
            Item::Apply => match (frame, self.classic.page) {
                (_, ClassicPage::Cosmetics) => {
                    self.show_classic(ClassicPage::Player);
                    PlayerMenuResult::None
                }
                (Frame::Full, ClassicPage::Player | ClassicPage::Character) => {
                    self.force.apply(console);
                    self.show_classic(ClassicPage::Saber);
                    PlayerMenuResult::None
                }
                (Frame::Full, _) => PlayerMenuResult::None,
                (Frame::InGame, _) => {
                    self.force.apply(console);
                    PlayerMenuResult::Back(ReturnTarget::InGame)
                }
            },
            Item::ApplyMain => {
                self.force.apply(console);
                PlayerMenuResult::Back(ReturnTarget::MainMenu)
            }
            _ => {
                self.classic_adjust(console, 1);
                PlayerMenuResult::None
            }
        }
    }

    fn style_result(&mut self, console: &mut ViewerConsole, style: SaberStyle) -> PlayerMenuResult {
        self.set_saber_style(console, style);
        PlayerMenuResult::None
    }

    fn set_saber_style(&mut self, console: &mut ViewerConsole, style: SaberStyle) {
        let catalog = catalog_of(&self.loader);
        self.saber.set_style(style, catalog);
        self.saber.apply(console);
        self.classic.hilt_scroll = [0; 2];
        // The second-saber entries come and go with Dual.
        let count = self.classic_items().len();
        self.classic.focus = self.classic.focus.min(count.saturating_sub(1));
    }

    /// Write the character draft if entering a page changed it.
    fn write_if_dirty(&mut self, console: &mut ViewerConsole) {
        if std::mem::take(&mut self.classic_dirty) {
            self.apply(console);
        }
    }

    /// Escape: back to the profile page from the others (a Force draft is
    /// kept until the screen closes); from the profile page, back to where
    /// the screen was opened.
    fn classic_escape(&mut self) -> PlayerMenuResult {
        if self.classic.page == ClassicPage::Player {
            return PlayerMenuResult::Back(self.return_target);
        }
        self.show_classic(ClassicPage::Player);
        PlayerMenuResult::None
    }

    /// Keyboard on the classic pages: Up/Down/Tab move focus in entry order,
    /// Left/Right step the focused list or chooser, Enter activates.
    pub(super) fn classic_key(
        &mut self,
        event: &KeyEvent,
        console: &mut ViewerConsole,
    ) -> PlayerMenuResult {
        if event.state != ElementState::Pressed {
            return PlayerMenuResult::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return PlayerMenuResult::None;
        };
        if self.name_editing {
            return self.edit_name(event, key, console);
        }
        if self.search_editing {
            self.edit_search(event, key);
            return PlayerMenuResult::None;
        }
        if self.force_templates.editing {
            return self.edit_template_name(event, key);
        }
        self.write_if_dirty(console);
        let count = self.classic_items().len().max(1);
        match key {
            KeyCode::Escape if !event.repeat => return self.classic_escape(),
            KeyCode::ArrowUp | KeyCode::KeyW => {
                self.classic.focus = self.classic.focus.checked_sub(1).unwrap_or(count - 1);
            }
            KeyCode::ArrowDown | KeyCode::KeyS | KeyCode::Tab => {
                self.classic.focus = (self.classic.focus + 1) % count;
            }
            KeyCode::ArrowLeft | KeyCode::KeyA => self.classic_adjust(console, -1),
            KeyCode::ArrowRight | KeyCode::KeyD => self.classic_adjust(console, 1),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space if !event.repeat => {
                return self.classic_activate(console);
            }
            _ => {}
        }
        PlayerMenuResult::None
    }
}

/// Menu snapshots (`menu_snapshot`) show a classic page without a running
/// client: the page by name, the Force side, and every Force icon counted as
/// uploaded. Returns the Force icons' atlas cells and the files to draw in them.
#[cfg(test)]
impl PlayerMenu {
    pub(crate) fn snapshot_page(
        &mut self,
        page: &str,
        dark: bool,
    ) -> Vec<crate::player_menu::icons::IconRequest> {
        let page = match page {
            "force" => ClassicPage::Force,
            "saber" => ClassicPage::Saber,
            "character" => ClassicPage::Character,
            "cosmetics" => ClassicPage::Cosmetics,
            _ => ClassicPage::Player,
        };
        self.show_classic(page);
        if page == ClassicPage::Force {
            self.choose_side(if dark {
                ForceSide::Dark
            } else {
                ForceSide::Light
            });
        }
        let requests = super::force_icons::requests();
        for (texture, _) in &requests {
            self.force_icons.mark_ready(*texture);
        }
        requests
    }

    /// Pointer widgets the last frame registered.
    pub(crate) fn snapshot_widgets(&self) -> usize {
        self.canvas.widget_count()
    }

    /// Focus the Force page's row of power `index`, as a pointer hover would.
    pub(crate) fn snapshot_focus_power(&mut self, index: u8) {
        if let Some(at) = self
            .classic_items()
            .iter()
            .position(|item| *item == Item::Power(index))
        {
            self.classic.focus = at;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::layout::{Item, items};
    use super::*;

    #[test]
    fn swatches_are_the_six_stock_colours_once() {
        let mut sorted = BLADE_SWATCHES;
        sorted.sort_unstable();
        assert_eq!(sorted, [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn pages_lead_on_as_retail() {
        let mut menu = PlayerMenu::new();
        menu.return_target = ReturnTarget::MainMenu;
        menu.classic_style = true;
        menu.show_classic(ClassicPage::Player);
        assert_eq!(menu.classic_focused(), Some(Item::Name));
        let full = items(ClassicPage::Player, Frame::Full, false, false);
        assert!(full.contains(&Item::Custom) && full.contains(&Item::Apply));
        assert!(full.contains(&Item::NavPlay) && full.contains(&Item::Exit));
        let in_game = items(ClassicPage::Player, Frame::InGame, false, false);
        assert!(
            !in_game
                .iter()
                .any(|item| item.is_nav() || *item == Item::Exit)
        );
        assert!(in_game.contains(&Item::SaberButton));
        assert!(full.contains(&Item::ForceButton) && in_game.contains(&Item::ForceButton));
        assert!(full.contains(&Item::CosmeticsButton) && in_game.contains(&Item::CosmeticsButton));
        assert_eq!(
            menu.classic_escape(),
            PlayerMenuResult::Back(ReturnTarget::MainMenu)
        );
        for page in [
            ClassicPage::Saber,
            ClassicPage::Force,
            ClassicPage::Cosmetics,
        ] {
            menu.show_classic(page);
            assert_eq!(menu.classic_escape(), PlayerMenuResult::None);
            assert_eq!(menu.classic.page, ClassicPage::Player);
        }
    }

    #[test]
    fn the_force_page_follows_the_side_chosen() {
        let mut menu = PlayerMenu::new();
        menu.return_target = ReturnTarget::InGame;
        menu.classic_style = true;
        menu.show_classic(ClassicPage::Force);
        assert_eq!(menu.classic_focused(), Some(Item::SideLight));
        assert!(menu.classic_items().contains(&Item::Power(0))); // Heal
        menu.choose_side(ForceSide::Dark);
        assert_eq!(menu.classic_focused(), Some(Item::SideDark));
        assert!(!menu.classic_items().contains(&Item::Power(0)));
        assert!(menu.classic_items().contains(&Item::Power(6))); // Grip
    }

    #[test]
    fn second_saber_entries_follow_dual() {
        let single = items(ClassicPage::Saber, Frame::Full, false, false);
        let dual = items(ClassicPage::Saber, Frame::Full, true, false);
        assert!(!single.contains(&Item::Hilts2) && !single.contains(&Item::Blades2));
        assert!(dual.contains(&Item::Hilts2) && dual.contains(&Item::Blades2));
    }
}
