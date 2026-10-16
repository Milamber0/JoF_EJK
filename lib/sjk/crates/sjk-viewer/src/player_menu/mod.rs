//! The player screen: its state and pages, drawn as the classic profile pages
//! ([`classic`]) or the SJK UI's Character ([`sjk_view`]), there with the
//! `model` cvar's character standing on the backdrop's stage. Character and saber
//! changes write their cvars the moment they are made, so the stage model
//! swaps instantly and there is nothing to apply or revert. The Force page
//! is the exception: it edits a draft that only its Apply action writes
//! (see `force`).

mod blade_skins;
mod classic;
mod controller;
mod cosmetics;
mod force;
pub(crate) mod force_icons;
pub(crate) mod force_pose;
mod force_templates;
mod grid;
pub(crate) mod icons;
mod model_icons;
mod numeric;
mod part_icons;
mod pointer;
mod rows;
mod saber;
mod sjk_creator;
mod sjk_outfit;
mod sjk_view;
mod team_filter;

use crate::console::ViewerConsole;
use crate::menu_widgets::MenuCanvas;
pub(crate) use rows::saber_color;
pub(crate) use saber::StageSabers;
use sjk_client::{LegacyAssetCatalog, LegacyAssetCatalogLoader};
use sjk_ui::DrawList;
use sjk_vfs::VirtualFileSystem;
use std::sync::Arc;
use team_filter::TeamSkin;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ProfilePage {
    #[default]
    Character,
    Saber,
    Force,
}

impl ProfilePage {
    const ALL: [Self; 3] = [Self::Character, Self::Saber, Self::Force];

    fn index(self) -> usize {
        Self::ALL.iter().position(|page| *page == self).unwrap_or(0)
    }
}

/// Screen to restore when the player selector closes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ReturnTarget {
    #[default]
    MainMenu,
    InGame,
}

/// Result of one keyboard or pointer event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlayerMenuResult {
    None,
    Back(ReturnTarget),
    /// Leave for a page of the classic main menu (its navigation row and
    /// Exit lead there).
    ClassicPage(crate::menu::classic::layout::Page),
}

/// Where the profile shows the live model ([`crate::menu_stage::preview`])
/// instead of on the menu map's stage: the classic pages, and the SJK UI's
/// pages opened from a game (where there is no stage). Its area and the
/// animation it plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ModelPreview {
    pub(crate) area: PreviewArea,
    pub(crate) stance: &'static str,
    /// The model holds the saber draft's sabers, lit, in their style's
    /// stance (`stance` is then unused).
    pub(crate) sabers: bool,
    /// A gesture played instead of either stance, then a wait, and again: on the SJK
    /// UI's Force page, the power under the pointer's ([`force_pose::gesture`]).
    pub(crate) pose: Option<&'static [force_pose::Move]>,
    /// A Force shell the model wears: on the SJK UI's Force page, the power under the
    /// pointer's ([`force_pose::shell`]).
    pub(crate) shell: Option<force_pose::Shell>,
    /// What the model gives off: on the SJK UI's Force page, the power under the
    /// pointer's beam, puffs, spray or trail ([`force_pose::effect`]).
    pub(crate) effect: Option<force_pose::Effect>,
    /// Only the sabers are drawn, laid on their side and turning as
    /// retail's lightsaber creation spun its hilt.
    pub(crate) showcase: bool,
    /// How much room the camera leaves round the body: 1 frames it as retail
    /// did, more keeps a raised blade in the picture.
    pub(crate) room: f32,
    /// The camera's angle round the model in degrees, held still; `None`
    /// turns round it as retail's preview did.
    pub(crate) angle: Option<f32>,
}

/// A model preview's area: a rectangle of retail's 640x480 canvas (the
/// classic pages) or of the SJK UI's 16:9 frame of 1080-line pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PreviewArea {
    Classic([f32; 4]),
    Sjk([f32; 4]),
}

impl PreviewArea {
    /// The area in window pixels for a window of `viewport`.
    pub(crate) fn window_rect(self, viewport: [f32; 2]) -> sjk_ui::Rect {
        match self {
            Self::Classic(rect) => {
                crate::menu::classic::layout::Placement::new(viewport).rect(rect)
            }
            Self::Sjk([x, y, width, height]) => {
                crate::menu::sjk::Frame::new(viewport).rect(x, y, width, height)
            }
        }
    }
}

/// Which catalogue entry the `model` cvar currently names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Choice {
    Character(usize),
    Species(usize),
}

/// The userinfo values this screen owns (sabers and Force live in their
/// own drafts).
#[derive(Clone, Debug, Eq, PartialEq)]
struct Draft {
    name: String,
    model: String,
    rgb: [u8; 3],
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            name: String::with_capacity(32),
            model: "kyle/default".to_owned(),
            rgb: [255; 3],
        }
    }
}

/// Draw commands the screen has room for: the Saber page's blade choice draws
/// up to six live swatches, a hundred and more shapes each with every effect.
const DRAWS: usize = 1_280;

/// Fixed-storage controller and retained draw list for the player screen.
pub(crate) struct PlayerMenu {
    canvas: MenuCanvas,
    loader: Option<LegacyAssetCatalogLoader>,
    /// Where the model icons come from once the catalogue is known.
    icon_vfs: Option<Arc<VirtualFileSystem>>,
    /// The character grid's icons, loaded as tiles are drawn.
    icons: model_icons::ModelIcons,
    /// The Force page's power icons and side emblems.
    force_icons: icons::IconLoader,
    /// Character creation's part icons and tint base, for the species edited.
    part_icons: part_icons::PartIcons,
    /// Skin set the grid lists (retail's Team Color chooser).
    team: TeamSkin,
    /// Catalogue indices (characters first, then species) the grid shows,
    /// rebuilt when the team, the search or the catalogue changes.
    tiles: Vec<usize>,
    /// Words a listed model's name must contain (SJK's model search).
    search: String,
    /// The search is being typed.
    search_editing: bool,
    /// Catalogue entry of the grid tile under the pointer, for the classic
    /// description line.
    hovered_entry: Option<usize>,
    /// First visible tile row of the model grid.
    grid_scroll: usize,
    /// Largest `grid_scroll` the last frame's layout allowed.
    grid_max_scroll: usize,
    /// Scroll the grid to the current model on the next frame (set when the
    /// choice changes without the pointer, so wheel browsing is not undone).
    grid_follow: bool,
    draft: Draft,
    /// The `model` and team colour the cvars hold: the SJK UI's Character page shows
    /// the draft's model but wears this one until Apply ([`Self::apply_look`]).
    applied_model: String,
    applied_rgb: [u8; 3],
    choice: Option<Choice>,
    /// Selected head, torso, legs and skin colour of the current species.
    variants: [usize; 4],
    /// Keyboard/pointer selection: a row index on the current page.
    selected: usize,
    name_editing: bool,
    numeric: Option<crate::menu_widgets::numeric::NumericEdit>,
    name_before_edit: String,
    return_target: ReturnTarget,
    resolved_catalogue: bool,
    page: ProfilePage,
    saber: saber::SaberMenu,
    force: force::ForceMenu,
    /// The classic Force page's templates (retail's `forcecfg`).
    force_templates: force_templates::TemplateState,
    /// JoF EJK's hats and capes (the classic cosmetics window).
    cosmetics: cosmetics::CosmeticsMenu,
    /// `ui_menuStyle classic`: the retail profile pages.
    classic_style: bool,
    /// `ui_menuStyle sjk`: the SJK UI's view of the screen (`sjk_view`).
    sjk: bool,
    /// The SJK UI's hilt lists: the first saber's and the second's.
    sjk_hilts: [sjk_view::HiltList; 2],
    /// The SJK UI Saber page's blade: the stock one or a blade skin owned.
    blade_choice: blade_skins::BladeChoice,
    /// The first blade choice the Saber page's row showed last frame (its arrows and
    /// wheel browse from it).
    blade_first: usize,
    /// The SJK UI Character page's turn of the model (dragged with the pointer).
    turn: sjk_outfit::Turn,
    /// The SJK UI Character page's creator: the category on show and its grid.
    creator: sjk_creator::Creator,
    /// How many of `tiles`, from the first, are the retail game's.
    retail_tiles: usize,
    /// How many tiles after the retail ones are high quality (rig) models.
    rig_tiles: usize,
    /// The Force level last bought and when (menu clock seconds), for the
    /// ring the SJK UI sends out from it.
    sjk_burst: Option<(usize, u8, f64)>,
    classic: classic::ClassicState,
    /// The character draft changed on entering a classic page and is not
    /// written yet.
    classic_dirty: bool,
    /// The model preview has a frame to show (set by the renderer).
    preview_ready: bool,
    /// Shown as the Profile screen's Character, Saber and Force tabs
    /// ([`crate::profile_hub`]): the SJK UI's view draws the screen's row of tabs in
    /// place of its own three.
    hub: bool,
    /// The Force page showed last frame ([`Self::settle_force_page`]).
    force_shown: bool,
}

impl PlayerMenu {
    pub(crate) fn new() -> Self {
        Self {
            canvas: MenuCanvas::with_draw_capacity(DRAWS),
            loader: None,
            icon_vfs: None,
            icons: model_icons::ModelIcons::new(),
            force_icons: icons::IconLoader::new(),
            part_icons: part_icons::PartIcons::new(),
            team: TeamSkin::default(),
            tiles: Vec::with_capacity(icons::MAX_ICONS),
            search: String::with_capacity(32),
            search_editing: false,
            hovered_entry: None,
            grid_scroll: 0,
            grid_max_scroll: 0,
            grid_follow: true,
            draft: Draft::default(),
            applied_model: "kyle/default".to_owned(),
            applied_rgb: [255; 3],
            choice: None,
            variants: [0; 4],
            selected: 0,
            name_editing: false,
            numeric: None,
            name_before_edit: String::with_capacity(32),
            return_target: ReturnTarget::MainMenu,
            resolved_catalogue: false,
            page: ProfilePage::Character,
            saber: saber::SaberMenu::new(),
            force: force::ForceMenu::new(),
            force_templates: force_templates::TemplateState::default(),
            cosmetics: cosmetics::CosmeticsMenu::new(),
            classic_style: false,
            sjk: false,
            sjk_hilts: Default::default(),
            blade_choice: blade_skins::BladeChoice::new(),
            blade_first: 0,
            turn: sjk_outfit::Turn::default(),
            creator: sjk_creator::Creator::default(),
            retail_tiles: 0,
            rig_tiles: 0,
            sjk_burst: None,
            classic: classic::ClassicState::default(),
            classic_dirty: false,
            preview_ready: false,
            hub: false,
            force_shown: false,
        }
    }

    /// Show the screen as the Profile screen's first tabs (`true`), or on its own;
    /// [`Self::open`] puts it back on its own.
    pub(crate) fn set_hub(&mut self, hub: bool) {
        self.hub = hub;
    }

    /// Whether the screen is the Profile screen's first tabs.
    pub(crate) fn is_hub(&self) -> bool {
        self.hub
    }

    /// The page on show: 0 Character, 1 Saber, 2 Force.
    pub(crate) fn page_index(&self) -> usize {
        self.page.index()
    }

    /// Show page `index` (Character, Saber, Force), its first row chosen.
    pub(crate) fn show_page(&mut self, index: usize) {
        self.set_page(ProfilePage::ALL[index.min(2)]);
    }

    /// Whether the page on show is the Force page, the classic one or the
    /// SJK UI's.
    fn force_page_on_show(&self) -> bool {
        if self.classic_style {
            self.classic.page == classic::ClassicPage::Force
        } else {
            self.page == ProfilePage::Force
        }
    }

    /// Once a frame: `visible` says whether the screen shows. Leaving the Force
    /// page applies it ([`force::ForceMenu::leave`]), however it was left: another
    /// page, Back, Escape, another Profile tab or the screen closed for the game.
    pub(crate) fn settle_force_page(&mut self, visible: bool, console: &mut ViewerConsole) {
        if !visible {
            // A model chosen and not applied is dropped with the screen.
            self.discard_look();
        }
        let shown = visible && self.force_page_on_show();
        match (self.force_shown, shown) {
            (false, true) => self.force.enter(),
            (true, false) => self.force.leave(console),
            _ => {}
        }
        self.force_shown = shown;
    }

    /// Read the blade skins the player owns and wears for the Saber page's blade
    /// choice, twice a second while it draws.
    pub(crate) fn follow_blade_skins(&mut self, console: &ViewerConsole) {
        if self.is_sjk() && self.page == ProfilePage::Saber {
            self.blade_choice.follow(console);
        }
    }

    /// Whether a field takes the keys: the name, a search (the model's or the
    /// hilts') or a number typed.
    pub(crate) fn typing(&self) -> bool {
        self.name_editing || self.search_editing || self.numeric.is_some()
    }

    /// The text of the field that has the keys, for Ctrl+C.
    pub(crate) fn field_text(&self) -> Option<&str> {
        if self.name_editing {
            return Some(&self.draft.name);
        }
        if self.search_editing {
            return Some(match self.page {
                ProfilePage::Saber => self.saber.search(),
                _ => &self.search,
            });
        }
        None
    }

    pub(crate) fn draw_list(&self) -> &DrawList {
        self.canvas.draw_list()
    }

    /// Whether the last frame ran out of room on the canvas.
    #[cfg(test)]
    pub(crate) fn overflowed(&self) -> bool {
        self.canvas.overflowed()
    }

    /// The `model` cvar value currently shown, for the live stage model.
    pub(crate) fn stage_model(&self) -> &str {
        &self.draft.model
    }

    /// The sabers the stage model holds: the saber draft as it stands,
    /// thrown out to the backdrop's saber shot while the screen is `open`
    /// on its Saber tab (the tab stays selected after the screen closes;
    /// the saber must not).
    pub(crate) fn stage_sabers(&self, open: bool) -> StageSabers<'_> {
        self.saber
            .stage_sabers(open && self.page == ProfilePage::Saber)
    }

    /// Where the screen returns when closed.
    pub(crate) fn return_target(&self) -> ReturnTarget {
        self.return_target
    }

    /// Backdrop shot behind the current tab. The classic pages cover the
    /// screen with retail art, so the backdrop camera stays where it is.
    pub(crate) fn shot(&self) -> crate::menu_backdrop::Shot {
        if self.classic_style {
            return crate::menu_backdrop::Shot::Main;
        }
        match self.page {
            ProfilePage::Saber => crate::menu_backdrop::Shot::Saber,
            _ => crate::menu_backdrop::Shot::Player,
        }
    }

    /// Selected row, for the shared highlight shader.
    pub(crate) fn visual_selection(&self) -> usize {
        self.selected
    }

    /// Move a few decoded model and Force icons into the UI atlas, and ask
    /// for the model icons this frame's tiles lacked.
    pub(crate) fn upload_icons(
        &mut self,
        renderer: &crate::ui_renderer::ShapeRenderer,
        queue: &crate::frame_queue::FrameQueue,
    ) {
        self.icons.end_frame(renderer, queue, 32);
        self.force_icons.upload_batch(renderer, queue, 32);
        self.part_icons.upload(renderer, queue);
    }

    /// Upload every icon again, into an atlas that holds none of them
    /// (another world's, see [`crate::ui_renderer::ShapeRenderer::id`]).
    /// The Force and part icons are decoded again on the next poll.
    pub(crate) fn forget_uploads(&mut self) {
        self.icons.forget_uploads();
        self.force_icons = icons::IconLoader::new();
        self.part_icons.forget_uploads();
    }

    /// Start decoding the Force icons once the VFS is known, and serve the
    /// model icons once the catalogue is too.
    fn request_icons_if_ready(&mut self) {
        if let Some(vfs) = self
            .icon_vfs
            .as_ref()
            .filter(|_| self.force_icons.is_idle())
        {
            self.force_icons
                .request_paths(Arc::clone(vfs), force_icons::requests());
        }
        let catalog = self
            .loader
            .as_ref()
            .and_then(LegacyAssetCatalogLoader::catalog);
        if let (Some(vfs), Some(catalog)) = (&self.icon_vfs, catalog) {
            self.icons.attach(vfs, catalog);
        }
    }

    fn catalog(&self) -> Option<&LegacyAssetCatalog> {
        catalog_of(&self.loader)
    }
}

/// Borrow the loaded catalogue independently of the mutable menu canvas.
fn catalog_of(loader: &Option<LegacyAssetCatalogLoader>) -> Option<&LegacyAssetCatalog> {
    loader
        .as_ref()
        .and_then(LegacyAssetCatalogLoader::catalog)
        .map(Arc::as_ref)
}

#[cfg(test)]
impl PlayerMenu {
    /// Show page `index` (Character, Saber, Force) with row `row` selected
    /// (world shots).
    pub(crate) fn show_page_for_shot(&mut self, index: usize, row: usize) {
        self.show_page(index);
        self.selected = row;
    }

    /// Move the pointer onto level `level` of power `power` as the last frame
    /// laid it out, for the world shots.
    pub(crate) fn hover_level_for_shot(&mut self, power: usize, level: u8) {
        if let Some(rect) = self.canvas.rect_for(sjk_view::level_token(power, level)) {
            let _ = self
                .canvas
                .pointer(sjk_ui::InputEvent::PointerMove(sjk_ui::Vec2::new(
                    rect.x + rect.width * 0.5,
                    rect.y + rect.height * 0.5,
                )));
        }
    }

    /// Show the Force page as on a server with `rules`, for the world shots.
    #[cfg(test)]
    pub(crate) fn server_rules_for_shot(&mut self, rules: sjk_client::ForceLegalizeRules) {
        self.force.load_on_server("7-2-031330310000030333", rules);
    }

    /// Show the Saber page's hilt search as `search` (typed when `typing`) and
    /// offer the blade skins `unlocks` own, for the world shots.
    pub(crate) fn saber_for_shot(
        &mut self,
        console: &ViewerConsole,
        search: &str,
        typing: bool,
        unlocks: Vec<sjk_identity::Unlock>,
    ) {
        self.saber.set_search(search);
        self.search_editing = typing;
        self.blade_choice.preview = Some(unlocks);
        self.blade_choice.read(console);
    }

    /// Make the draft Dual (not written to the profile), for the world shots.
    pub(crate) fn dual_for_shot(&mut self) {
        self.saber
            .set_style(saber::SaberStyle::Dual, catalog_of(&self.loader));
    }

    /// Open the classic lightsaber creation page on `vfs`'s catalogue as
    /// `console` sets it up, for the menu snapshots: the catalogue is waited
    /// for (a minute at most).
    pub(crate) fn open_classic_saber_for_snapshot(
        &mut self,
        console: &ViewerConsole,
        vfs: Arc<sjk_vfs::VirtualFileSystem>,
        art: crate::menu::art::ArtSet,
        in_game: bool,
    ) {
        self.attach_catalogue(vfs);
        self.set_style(true, art);
        let target = if in_game {
            ReturnTarget::InGame
        } else {
            ReturnTarget::MainMenu
        };
        self.open(console, target);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while !self.is_resolved() && std::time::Instant::now() < deadline {
            self.poll();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        self.show_classic(classic::ClassicPage::Saber);
    }
}

impl crate::GpuState {
    pub(crate) fn open_player_menu_from_game(&mut self) {
        if let (Some(menu), Some(console)) = (&mut self.client_menu, &self.console) {
            menu.open_player(console, ReturnTarget::InGame);
            self.game_menu = false;
        }
    }
}
