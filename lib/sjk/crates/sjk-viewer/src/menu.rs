//! Native main-menu and server-browser input/presentation on the shared UI path.

pub(crate) mod art;
mod controller;
mod pointer;

pub(crate) mod browser_filters;
pub(crate) mod browser_table;
pub(crate) mod browser_view;
pub(crate) mod classic;
pub(crate) mod create_game;
pub(crate) mod create_game_catalog;
mod create_game_pointer;
mod create_game_view;
mod destination;
pub(crate) mod emblem;
pub(crate) mod first_setup;
mod hosting;
pub(crate) mod levelshot;
pub(crate) mod main_view;
mod map_picker;
mod map_picker_view;
pub(crate) mod sjk;
pub(crate) mod style;

use super::{TextVertex, UiFont};
use crate::client_state::{ClientPhase, ClientState};
use crate::console::ViewerConsole;
use crate::keybind_editor::{EditorResult, KeybindEditor};
use crate::menu_backdrop::{Sample, Shot, Stage};
use crate::menu_widgets::MenuCanvas;
use crate::player_menu::{PlayerMenu, PlayerMenuResult, ReturnTarget};
use crate::server_browser::{RefreshPoll, ServerBrowser, SortColumn};
use crate::settings::{SettingsMenu, SettingsResult};
use destination::MainDestination;
use sjk_ui::{AbstractAction, DrawList, InputEvent};
use style::MenuStyle;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// Hand the loaded map and mounted assets to the menu layer.
pub(crate) fn attach_world(
    menu: &mut Option<ClientMenu>,
    vfs: std::sync::Arc<sjk_vfs::VirtualFileSystem>,
    bsp: &sjk_bsp::Bsp,
    console: Option<&ViewerConsole>,
) {
    if let Some(menu) = menu {
        menu.attach_catalogue(vfs);
        menu.backdrop = crate::menu_backdrop::Backdrop::from_bsp(bsp);
        if let Some(console) = console {
            menu.player.prime(console);
        }
    }
}

/// Feed decoded menu images to the UI renderer while their screen is up: the
/// player screen's model icons, the key bindings' pictures, Create game's map
/// preview, the HUD picker's preview (drawn from `vfs` and `shaders`, the
/// files the HUD reads).
pub(crate) fn upload_menu_images(
    menu: &mut Option<ClientMenu>,
    renderer: &mut crate::ui_renderer::ShapeRenderer,
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    vfs: Option<&std::sync::Arc<sjk_vfs::VirtualFileSystem>>,
    shaders: &sjk_shader::ShaderCatalog,
) {
    let Some(menu) = menu else { return };
    menu.follow_renderer(renderer.id());
    match menu.state.phase() {
        ClientPhase::Player => menu.player.upload_icons(renderer, queue),
        ClientPhase::Keybinds => menu.keybinds.upload_icons(renderer, queue),
        ClientPhase::Settings => menu.settings.service_hud_picker(vfs, shaders, |image| {
            renderer.upload_hud_preview(device, queue, image);
        }),
        ClientPhase::CreateGame => {
            menu.create_game
                .service_levelshots(|image| renderer.upload_levelshot(device, queue, image));
        }
        // The SJK UI's browser shows the chosen server's map.
        ClientPhase::Browser if menu.menu_style == MenuStyle::Sjk => {
            let map = menu
                .browser
                .visible_entry(menu.browser.selected())
                .map(|entry| classic::loading::levelshot_key(&entry.map))
                .unwrap_or_default();
            if !map.is_empty() {
                menu.create_game.service_levelshot_for(&map, |image| {
                    renderer.upload_levelshot(device, queue, image);
                });
            }
        }
        ClientPhase::Connecting(_) | ClientPhase::ConnectionError => {
            let map = menu.loading.map().to_owned();
            if !map.is_empty() {
                menu.create_game.service_levelshot_for(&map, |image| {
                    renderer.upload_levelshot(device, queue, image);
                });
            }
        }
        _ => {}
    }
}

/// Side effects requested by menu input and executed by the application.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum MenuAction {
    None,
    Connect(String),
    CancelJoin,
    /// Start a local server with these settings and join it.
    HostGame(crate::connection::local_server::HostSettings),
    Quit,
    ReturnToGameMenu,
}

/// Persistent menu selection, browser data, and high-level client phase.
pub(crate) struct ClientMenu {
    state: ClientState,
    browser: ServerBrowser,
    browser_focus: u16,
    /// Last clicked browser row and when, for double-click joining.
    browser_last_click: Option<(u16, std::time::Instant)>,
    /// Layout of the main menu (`ui_menuStyle`).
    menu_style: MenuStyle,
    /// Page and entry of the classic main menu.
    classic: classic::ClassicMain,
    /// The SJK UI's main page: its page of the ring, chosen entry and motion.
    home: sjk::home::Home,
    /// What the main page's docked SJK chat shows.
    chat_dock: sjk::chat_dock::DockCache,
    /// The servers joined last, for the SJK UI's main page.
    recent: sjk::recent::RecentServers,
    /// The SJK UI's Settings: whether it is open and on which category.
    sjk_settings: sjk::settings::SettingsPage,
    /// The SJK UI's server browser: which item of its left column has the
    /// keyboard, if any.
    sjk_browser: sjk::browser::BrowserPage,
    /// The SJK UI's loading screen: the levelshot's fade and the furthest
    /// step the join has shown.
    sjk_loading: sjk::loading::LoadingPage,
    /// The classic option panel on show in the settings or key-binding
    /// phase, if any.
    classic_panel: Option<classic::ClassicPanel>,
    /// The classic panel the renderer settings were opened from, reopened when
    /// they close.
    renderer_panel: Option<classic::ClassicPanel>,
    /// Retail menu artwork the classic style can draw this frame.
    art: art::ArtSet,
    /// The classic connect and loading screens' state.
    loading: classic::loading::ClassicLoading,
    /// The world is not drawn under the menu this frame (classic style).
    world_hidden: bool,
    /// The Profile screen's Holocrons tab is open over the menu: the backdrop camera
    /// parks on its shot ([`Shot::Holocrons`]).
    holocron_shot: bool,
    /// The retail background drawn under the screens the classic pages open,
    /// while the world is hidden.
    classic_backdrop: MenuCanvas,
    /// The dark of the backdrop tour's fades ([`Self::world_fade`]).
    world_fade: MenuCanvas,
    settings: SettingsMenu,
    /// Where the settings screen (and the key-bindings editor it hosts)
    /// returns when closed: the main menu, or the game menu that opened it.
    settings_return: ReturnTarget,
    /// Where the server browser returns when closed without joining.
    browser_return: ReturnTarget,
    keybinds: KeybindEditor,
    /// The search typed on the settings tab on show and how many entries of the
    /// other tab match it (see `sync_cross_search`).
    cross_search: (String, usize),
    player: PlayerMenu,
    /// The Create game screen and the server it runs.
    create_game: create_game::CreateGameMenu,
    ui: MenuCanvas,
    filter_editing: bool,
    /// The classic browser's SERVER INFO pop-up is open.
    classic_info: bool,
    password_target: Option<String>,
    password: String,
    address_editing: bool,
    /// What the address prompt is for ([`controller::AddressPurpose`]).
    address_purpose: controller::AddressPurpose,
    /// The answer of the host name lookup of a favourite being added.
    favourite_lookup: Option<std::sync::mpsc::Receiver<Result<std::net::SocketAddr, String>>>,
    /// Passwords of locked servers remembered on this PC ([`crate::server_passwords`]).
    server_passwords: crate::server_passwords::ServerPasswords,
    /// Whether the open password prompt remembers what is typed (on unless switched off).
    remember_password: bool,
    address_input: String,
    address_error: String,
    backdrop: Option<crate::menu_backdrop::Backdrop>,
    /// Map of the server being joined, as the browser row advertised it.
    destination_map: Option<String>,
    /// First setup was offered in this run (`offer_first_setup`).
    first_setup_offered: bool,
    /// The renderer ([`crate::ui_renderer::ShapeRenderer::id`]) the menu's
    /// images were uploaded into; another one (a world change) has none of them.
    uploaded_into: Option<u64>,
    /// The Collection shows Shaders, Toys or Nameplates over this menu: the camera goes
    /// to the player's shot and the model stands on the stage, or shows in a preview
    /// over a match ([`Self::set_collection_stage`]).
    collection_stage: bool,
    /// The model the `modelviewer` page has chosen: it stands on the stage in place of
    /// the player's own ([`Self::follow_model_viewer`]).
    viewer_model: Option<String>,
    /// The rigid model (`.md3` or `.glm`) the page has chosen: the stage shows it in place
    /// of the player ([`crate::menu_stage`]).
    viewer_prop: Option<String>,
}

/// Backdrop shot each client phase is presented over. A connect stays on
/// the browser shot, behind the loading screen.
fn shot_for(phase: &ClientPhase, player: &PlayerMenu, holocrons: bool) -> Shot {
    // The Profile screen's Holocrons tab (a console page over the main menu) asks for
    // its own shot whatever the phase.
    if holocrons {
        return Shot::Holocrons;
    }
    match phase {
        ClientPhase::Browser
        | ClientPhase::Connecting(_)
        | ClientPhase::ConnectionError
        | ClientPhase::CreateGame => Shot::Browser,
        ClientPhase::Settings | ClientPhase::Keybinds => Shot::Settings,
        ClientPhase::Player => player.shot(),
        _ => Shot::Main,
    }
}

/// Whether `phase` is a connect in progress.
fn connecting(phase: &ClientPhase) -> bool {
    matches!(phase, ClientPhase::Connecting(_))
}

impl ClientMenu {
    pub(crate) fn new(open_main_menu: bool, master: String) -> Self {
        Self {
            state: ClientState::new(open_main_menu),
            browser: ServerBrowser::new(master),
            browser_focus: 1_000,
            browser_last_click: None,
            menu_style: MenuStyle::default(),
            classic: classic::ClassicMain::new(),
            home: sjk::home::Home::default(),
            chat_dock: sjk::chat_dock::DockCache::default(),
            // Tests never read or write the player's own list.
            recent: if cfg!(test) {
                sjk::recent::RecentServers::default()
            } else {
                sjk::recent::RecentServers::load()
            },
            sjk_settings: sjk::settings::SettingsPage::default(),
            sjk_browser: sjk::browser::BrowserPage::default(),
            sjk_loading: sjk::loading::LoadingPage::default(),
            classic_panel: None,
            renderer_panel: None,
            art: art::ArtSet::default(),
            loading: classic::loading::ClassicLoading::default(),
            world_hidden: false,
            holocron_shot: false,
            classic_backdrop: MenuCanvas::new(),
            world_fade: MenuCanvas::new(),
            settings: SettingsMenu::new(),
            settings_return: ReturnTarget::MainMenu,
            browser_return: ReturnTarget::MainMenu,
            keybinds: KeybindEditor::new(),
            cross_search: (String::new(), 0),
            player: PlayerMenu::new(),
            create_game: create_game::CreateGameMenu::new(),
            ui: MenuCanvas::new(),
            filter_editing: false,
            classic_info: false,
            password_target: None,
            destination_map: None,
            first_setup_offered: false,
            password: String::with_capacity(64),
            address_editing: false,
            address_purpose: controller::AddressPurpose::Join,
            favourite_lookup: None,
            server_passwords: crate::server_passwords::ServerPasswords::load(),
            remember_password: true,
            address_input: String::with_capacity(256),
            address_error: String::with_capacity(96),
            backdrop: None,
            uploaded_into: None,
            collection_stage: false,
            viewer_model: None,
            viewer_prop: None,
        }
    }

    /// Upload the menu's images again when `renderer` is not the one they
    /// went into: every world has a renderer of its own, while the menu moves
    /// between worlds, so the icons of a cache that thinks itself filled would
    /// be drawn from empty cells (black squares) or from cells the other world
    /// filled with something else.
    fn follow_renderer(&mut self, renderer: u64) {
        if self.uploaded_into == Some(renderer) {
            return;
        }
        self.uploaded_into = Some(renderer);
        self.player.forget_uploads();
        self.keybinds.forget_uploads();
        self.settings.forget_hud_preview_upload();
        self.create_game.forget_levelshot_upload();
    }

    /// Advance the live-map backdrop toward the current screen's shot and
    /// return the camera for this frame, once a map is loaded.
    pub(crate) fn drive_backdrop(&mut self, millis: u64) -> Option<Sample> {
        let shot = self.shot();
        self.backdrop
            .as_mut()
            .map(|backdrop| backdrop.drive(shot, millis))
    }

    /// Whether the joined server's map is still loading behind this menu.
    /// Whether a join is in progress (connecting or loading the map).
    pub(crate) fn is_connecting(&self) -> bool {
        connecting(self.state.phase())
    }

    /// Whether a screen of the SJK UI's own is on show (its main page, its
    /// Settings, Character, Servers, Create game or its loading screen), which
    /// sits over the live map where the classic pages cover it and draws in
    /// the UI's families. Create game has no classic+ version, so it is the
    /// SJK UI's in every menu style. The loading screen covers the map itself
    /// when it has to ([`Self::sjk_loading_hides_world`]).
    pub(crate) fn sjk_screen(&self) -> bool {
        // Its Settings and Key bindings, whatever the style: the screens a
        // classic page has no classic+ panel for open them too.
        let settings = match self.state.phase() {
            ClientPhase::Settings => {
                self.sjk_settings_on_show() && !self.settings.hud_picker_open()
            }
            ClientPhase::Keybinds => self.sjk_settings_on_show(),
            _ => false,
        };
        settings
            || matches!(self.state.phase(), ClientPhase::CreateGame)
            || self.menu_style == MenuStyle::Sjk
                && match self.state.phase() {
                    ClientPhase::MainMenu => true,
                    ClientPhase::Player => self.player.is_sjk(),
                    ClientPhase::Browser
                    | ClientPhase::Connecting(_)
                    | ClientPhase::ConnectionError => true,
                    _ => false,
                }
    }

    /// Whether the connect or loading screen covers the screen: a connect
    /// in progress or its failure.
    pub(crate) fn is_loading_screen(&self) -> bool {
        matches!(
            self.state.phase(),
            ClientPhase::Connecting(_) | ClientPhase::ConnectionError
        )
    }

    /// Tell the menu whether the Holocrons tab is open over it, to park the camera on its
    /// shot.
    pub(crate) fn set_holocron_shot(&mut self, on: bool) {
        self.holocron_shot = on;
    }

    /// Tell the menu whether the world is drawn under it this frame.
    pub(crate) fn set_world_hidden(&mut self, hidden: bool) {
        self.world_hidden = hidden;
    }

    /// The map the classic loading screen shows (`mp/ffa3`), if known.
    pub(crate) fn loading_map(&self) -> &str {
        self.loading.map()
    }

    /// The classic loading screen's state (the SJK UI's loading screen reads
    /// it too), for the join to report into.
    pub(crate) fn loading_mut(&mut self) -> &mut classic::loading::ClassicLoading {
        &mut self.loading
    }

    /// Build the dark the backdrop's camera tour fades through between its
    /// shots and cuts through to a screen's shot, over the world `viewport`
    /// shows ([`Self::world_fade`]); false while the world is clear.
    pub(crate) fn prepare_world_fade(&mut self, viewport: [f32; 2]) -> bool {
        let darkness = self
            .backdrop
            .as_ref()
            .map_or(0.0, crate::menu_backdrop::Backdrop::darkness);
        if darkness <= 0.0 {
            return false;
        }
        self.world_fade.begin_transparent(viewport);
        // The SJK UI's deep navy rather than black, as its fades are.
        let _ = self
            .world_fade
            .draw_list_mut()
            .push(sjk_ui::DrawCommand::SolidRect {
                rect: sjk_ui::Rect::new(0.0, 0.0, viewport[0], viewport[1]),
                color: sjk::color::alpha(sjk::color::SPACE, darkness),
            });
        true
    }

    /// The fade [`Self::prepare_world_fade`] built this frame.
    pub(crate) fn world_fade(&self) -> &DrawList {
        self.world_fade.draw_list()
    }

    /// The retail background drawn under a screen the classic pages opened
    /// while the world is hidden; built in [`Self::append_overlay`].
    pub(crate) fn backdrop_draw_list(&self) -> Option<&DrawList> {
        self.backdrop_draw_list_wanted()
            .then(|| self.classic_backdrop.draw_list())
    }

    fn backdrop_draw_list_wanted(&self) -> bool {
        let opened_screen = matches!(
            self.state.phase(),
            ClientPhase::Browser
                | ClientPhase::Settings
                | ClientPhase::Keybinds
                | ClientPhase::Player
                | ClientPhase::CreateGame
        );
        self.world_hidden && opened_screen
    }

    /// The map the browser row of the server being joined advertised
    /// (`mp/duel1`), if the join came from a row.
    pub(crate) fn destination_map(&self) -> Option<&str> {
        self.destination_map
            .as_deref()
            .filter(|map| !map.is_empty())
    }

    /// The backdrop shot behind this frame: the current screen's, or the player's
    /// while the Collection's stage is wanted.
    fn shot(&self) -> Shot {
        if self.collection_stage {
            Shot::Player
        } else {
            shot_for(self.state.phase(), &self.player, self.holocron_shot)
        }
    }

    /// Whether the Collection wants the player's model (on Shaders, Toys and
    /// Nameplates). When it starts wanting it and the player screen is not open, the
    /// model and sabers are read from the cvars, so the stage shows the player's own.
    pub(crate) fn set_collection_stage(&mut self, wanted: bool, console: &ViewerConsole) {
        if wanted && !self.collection_stage && !matches!(self.state.phase(), ClientPhase::Player) {
            self.player.read_for_stage(console);
        }
        self.collection_stage = wanted;
    }

    /// Follow the `modelviewer` page: show the player model it has chosen in place of the
    /// player's own, or note the rigid one for the stage to show instead.
    pub(crate) fn follow_model_viewer(&mut self, console: &ViewerConsole) {
        let choice = console.model_viewer_choice();
        let player = choice.filter(|entry| !entry.kind.is_prop());
        let prop = choice.filter(|entry| entry.kind.is_prop());
        if self.viewer_model.as_deref() != player.map(|entry| entry.name.as_str()) {
            self.viewer_model = player.map(|entry| entry.name.clone());
        }
        if self.viewer_prop.as_deref() != prop.map(|entry| entry.name.as_str()) {
            self.viewer_prop = prop.map(|entry| entry.name.clone());
        }
    }

    /// The rigid model the model viewer has chosen, if it is one.
    pub(crate) fn viewer_prop(&self) -> Option<&str> {
        self.viewer_prop.as_deref()
    }

    /// The model the stage shows: the model viewer's choice, else the player's own.
    fn shown_model(&self) -> &str {
        self.viewer_model
            .as_deref()
            .unwrap_or_else(|| self.player.stage_model())
    }

    /// The model the Collection shows in its own preview where it has no stage (over a
    /// match, or in the classic menus), with the stage it may stand on.
    pub(crate) fn collection_preview(
        &self,
    ) -> Option<(Option<Stage>, &str, crate::player_menu::ModelPreview)> {
        if !self.collection_stage {
            return None;
        }
        let preview = crate::player_menu::ModelPreview {
            area: crate::player_menu::PreviewArea::Sjk(
                crate::console::collection_panel::MODEL_AREA,
            ),
            stance: "BOTH_STAND2",
            sabers: true,
            pose: None,
            shell: None,
            effect: None,
            showcase: false,
            room: 1.35,
            angle: Some(-28.0),
        };
        let stage = self.backdrop.as_ref().and_then(|backdrop| backdrop.stage());
        Some((stage, self.shown_model(), preview))
    }

    /// The player model that stands on the backdrop's stage: only while a
    /// route with a stage is active (flying to, parked on, or flying back
    /// from the Player screen, or behind the Collection's Shaders, Toys and
    /// Nameplates).
    pub(crate) fn stage_model(&self) -> Option<(Stage, &str)> {
        if self.collection_stage && !self.player.is_classic() {
            let stage = self.backdrop.as_ref()?.stage()?;
            return Some((stage, self.shown_model()));
        }
        // The classic profile pages draw the model's portrait over retail
        // art; nothing stands on the stage behind them. Opened from a game,
        // the screen shows the model in its own preview: the menu map's stage
        // is not the match's.
        if self.player.is_classic() || self.player.return_target() == ReturnTarget::InGame {
            return None;
        }
        let stage = self.backdrop.as_ref()?.stage()?;
        Some((stage, self.player.stage_model()))
    }

    /// The model the classic profile's preview shows, with the stage it
    /// may stand on: only while the profile is on screen and its page shows
    /// one.
    pub(crate) fn preview_model(
        &self,
    ) -> Option<(Option<Stage>, &str, crate::player_menu::ModelPreview)> {
        if !self.is_visible() || !matches!(self.state.phase(), ClientPhase::Player) {
            return None;
        }
        let preview = self.player.model_preview()?;
        let stage = self.backdrop.as_ref().and_then(|backdrop| backdrop.stage());
        Some((stage, self.player.stage_model(), preview))
    }

    /// Whether the renderer has a preview frame for the profile to show.
    pub(crate) fn set_preview_ready(&mut self, ready: bool) {
        self.player.set_preview_ready(ready);
    }

    /// The sabers in the stage model's hands.
    pub(crate) fn stage_sabers(&self) -> crate::player_menu::StageSabers<'_> {
        let open = matches!(self.state.phase(), ClientPhase::Player);
        self.player.stage_sabers(open)
    }

    /// Where the thrown saber floats on this map's saber shot.
    pub(crate) fn saber_focus(&self) -> Option<crate::menu_backdrop::Focus> {
        self.backdrop.as_ref()?.focus(Shot::Saber)
    }

    /// Opacity of the current screen: 1 while the backdrop camera is parked
    /// on its shot, fading in as the flight arrives, 0 while it is away.
    fn screen_reveal(&self) -> f32 {
        if self.opened_from_game() {
            // Over a live match the backdrop camera is not flying anywhere.
            return 1.0;
        }
        let shot = self.shot();
        self.backdrop
            .as_ref()
            .map_or(1.0, |backdrop| backdrop.reveal(shot))
    }

    /// Whether the settings screen wants the window's monitor facts.
    pub(crate) fn wants_monitor_modes(&self) -> bool {
        self.settings.wants_monitor_modes()
    }

    /// Hand the settings screen the window's monitor facts.
    pub(crate) fn set_monitor_modes(
        &mut self,
        modes: crate::settings::MonitorModes,
        console: &ViewerConsole,
    ) {
        self.settings.set_monitor_modes(modes, console);
    }

    /// Start the master-server fetch now, ahead of the browser being
    /// opened, so its rows are waiting when "Play" is pressed.
    pub(crate) fn prefetch_servers(&mut self) {
        self.browser.refresh();
    }

    /// Evidence hook: open the server browser the way "Play" would, minus
    /// the master-server refresh (evidence runs stay off the network).
    pub(crate) fn open_browser(&mut self) {
        self.browser_return = ReturnTarget::MainMenu;
        self.state.open_browser();
        self.reset_sjk_browser();
    }

    /// Open the server browser over a live match, returning to the game
    /// menu when closed; a join from it leaves the current server. The
    /// caller fetches rows with [`Self::refresh_servers_if_stale`].
    pub(crate) fn open_browser_from_game(&mut self, console: &ViewerConsole) {
        if let Ok(master) = console.master_server() {
            self.browser.set_master(master);
        }
        self.browser_return = ReturnTarget::InGame;
        self.state.open_browser();
        self.filter_editing = false;
        self.browser_focus = 1_000 + self.browser.selected() as u16;
        self.reset_sjk_browser();
    }

    /// Start a master-server fetch unless fresh rows are already on show.
    pub(crate) fn refresh_servers_if_stale(&mut self) {
        if self.browser.is_stale() {
            self.refresh();
        }
    }

    /// Close the browser toward wherever it was opened from.
    pub(super) fn close_browser(&mut self) -> MenuAction {
        self.classic_info = false;
        match self.browser_return {
            ReturnTarget::MainMenu => {
                self.state.main_menu();
                MenuAction::None
            }
            ReturnTarget::InGame => {
                self.state.entered_game();
                MenuAction::ReturnToGameMenu
            }
        }
    }

    /// Act on what the settings screen asked for.
    pub(super) fn settings_result(
        &mut self,
        result: SettingsResult,
        console: &mut ViewerConsole,
    ) -> MenuAction {
        if let Some(action) = self.sjk_settings_result(&result, console) {
            return action;
        }
        match result {
            SettingsResult::Back => {
                if let Some(panel) = self.renderer_panel.take() {
                    self.reopen_classic_panel(console, panel);
                    return MenuAction::None;
                }
                // Setup's sub-pages back out to Setup, as their Back does.
                if let Some(panel) = self
                    .classic_panel
                    .filter(|panel| panel.page.is_setup_child())
                {
                    let setup = panel.page.escape();
                    if let Some(entry) = setup.opening_panel() {
                        let target = self.settings_return;
                        self.open_classic_panel(console, setup, entry, panel.frame, target);
                        return MenuAction::None;
                    }
                }
                self.close_settings()
            }
            // The classic+ and tabbed settings show the quick wheel's editor on
            // its own (the SJK UI's has its category, `sjk_settings_result`).
            SettingsResult::OpenWheelPages => {
                self.settings
                    .open_wheel_editor(console, crate::settings::WheelMode::Overlay);
                MenuAction::None
            }
            // Network's "SJK identity key" row: the Identity page over the settings,
            // which show again when it closes.
            SettingsResult::OpenIdentity => {
                console.open_identity_panel();
                MenuAction::None
            }
            SettingsResult::OpenAssets => {
                console.execute_console_line("assetbrowser", None);
                MenuAction::None
            }
            // First setup's "Import from another client" row: the Import page
            // with the configs found (or the file dialog), over the settings.
            SettingsResult::OpenImport => {
                console.browse_config_import();
                MenuAction::None
            }
            // The Gameplay FPS row: its card, over the settings.
            SettingsResult::OpenGameplayFps => {
                console.open_gameplay_fps();
                MenuAction::None
            }
            SettingsResult::Classic(index) => self.classic_panel_button(index, console),
            SettingsResult::ClassicCycle(direction) => {
                self.classic_panel_cycle(direction, console);
                MenuAction::None
            }
            // Only the SJK UI's First setup pop-up offers it.
            SettingsResult::AllSettings | SettingsResult::None => MenuAction::None,
        }
    }

    /// Act on what the key-binding editor asked for.
    pub(super) fn keybinds_result(
        &mut self,
        result: EditorResult,
        console: &mut ViewerConsole,
    ) -> MenuAction {
        if let Some(action) = self.sjk_keys_result(&result, console) {
            return action;
        }
        match result {
            EditorResult::Back => {
                // What Settings recorded before Key bindings goes out as it closes.
                crate::settings_usage::flush(console);
                self.close_keybinds()
            }
            EditorResult::Classic(index) => self.classic_panel_button(index, console),
            EditorResult::ClassicCycle(direction) => {
                self.classic_panel_cycle(direction, console);
                MenuAction::None
            }
            EditorResult::None => MenuAction::None,
        }
    }

    /// Close the settings screen toward wherever it was opened from.
    pub(super) fn close_settings(&mut self) -> MenuAction {
        self.close_sjk_settings();
        let classic_panel = self.leave_classic_panel();
        match self.settings_return {
            ReturnTarget::MainMenu => {
                self.state.main_menu();
                // Escape on a retail Setup or Controls page closes it to the
                // main page.
                if classic_panel {
                    self.classic.show(classic::layout::Page::Main);
                }
                MenuAction::None
            }
            ReturnTarget::InGame => {
                self.state.entered_game();
                MenuAction::ReturnToGameMenu
            }
        }
    }

    /// Whether the screen on show was opened from the game menu, over a
    /// live match rather than the menu map.
    fn opened_from_game(&self) -> bool {
        match self.state.phase() {
            ClientPhase::Settings | ClientPhase::Keybinds => {
                self.settings_return == ReturnTarget::InGame
            }
            ClientPhase::Player => self.player.return_target() == ReturnTarget::InGame,
            ClientPhase::Browser => self.browser_return == ReturnTarget::InGame,
            _ => false,
        }
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.state.is_overlay_visible()
    }

    /// The main menu is up: not a settings, browser or connection screen.
    pub(crate) fn on_main_menu(&self) -> bool {
        matches!(self.state.phase(), ClientPhase::MainMenu)
    }

    /// The Renderer settings page is on show; eye adaptation holds still under it.
    pub(crate) fn renderer_settings_open(&self) -> bool {
        self.is_visible()
            && matches!(self.state.phase(), ClientPhase::Settings)
            && self.settings.renderer_open()
    }

    pub(crate) fn poll(&mut self) {
        if matches!(self.state.phase(), ClientPhase::Player) || !self.player.is_resolved() {
            self.player.poll();
        }
        if matches!(self.state.phase(), ClientPhase::Keybinds) {
            self.keybinds.poll_icons();
        }
        self.poll_local_server();
        self.poll_favourite_lookup();
        if !self.field_open() {
            // A selection ends with the field.
            crate::text_entry::deselect();
        }
        let browser_visible = matches!(self.state.phase(), ClientPhase::Browser);
        if browser_visible {
            self.browser.tick_details();
        }
        match self.browser.poll() {
            RefreshPoll::Idle | RefreshPoll::Pending => {}
            RefreshPoll::Complete if browser_visible && self.browser.entries().is_empty() => {
                self.state
                    .set_status("No responding protocol-26 servers found.");
            }
            RefreshPoll::Complete if browser_visible => self.state.set_status(format!(
                "{} responding servers. Enter joins; R refreshes.",
                self.browser.entries().len()
            )),
            RefreshPoll::Failed(error) if browser_visible => {
                self.state.set_status(format!("Refresh failed: {error}"));
            }
            RefreshPoll::Complete | RefreshPoll::Failed(_) => {}
        }
    }

    /// A key on the server browser, in every style (the SJK UI's handles some
    /// first, [`ClientMenu::sjk_browser_key`]).
    fn browser_key(&mut self, key: KeyCode) -> MenuAction {
        match key {
            KeyCode::ArrowUp | KeyCode::KeyW => {
                self.browser.move_selection(-1);
                self.browser_focus = 1_000 + self.browser.selected() as u16;
                MenuAction::None
            }
            KeyCode::ArrowDown | KeyCode::KeyS => {
                self.browser.move_selection(1);
                self.browser_focus = 1_000 + self.browser.selected() as u16;
                MenuAction::None
            }
            KeyCode::PageUp | KeyCode::PageDown => {
                self.browser
                    .page_selection(if key == KeyCode::PageUp { -1 } else { 1 });
                self.browser_focus = 1_000 + self.browser.selected() as u16;
                MenuAction::None
            }
            KeyCode::Home | KeyCode::End => {
                self.browser.select_end(key == KeyCode::End);
                self.browser_focus = 1_000 + self.browser.selected() as u16;
                MenuAction::None
            }
            KeyCode::Tab => {
                self.browser
                    .set_favorites_only(!self.browser.favorites_only());
                self.browser_focus = 1_000 + self.browser.selected() as u16;
                MenuAction::None
            }
            KeyCode::KeyC => {
                self.open_address_entry();
                MenuAction::None
            }
            KeyCode::KeyA if self.menu_style == MenuStyle::Sjk => {
                self.open_favourite_entry();
                MenuAction::None
            }
            KeyCode::KeyR => {
                self.refresh();
                MenuAction::None
            }
            KeyCode::KeyF => {
                self.browser.toggle_selected_favorite();
                MenuAction::None
            }
            KeyCode::Slash => {
                self.filter_editing = true;
                MenuAction::None
            }
            KeyCode::Backspace if self.filter_editing => {
                self.browser.pop_filter();
                MenuAction::None
            }
            KeyCode::Digit1 => {
                self.browser.sort_by(SortColumn::Name);
                MenuAction::None
            }
            KeyCode::Digit2 => {
                self.browser.sort_by(SortColumn::Map);
                MenuAction::None
            }
            KeyCode::Digit3 => {
                self.browser.sort_by(SortColumn::Players);
                MenuAction::None
            }
            KeyCode::Digit4 => {
                self.browser.sort_by(SortColumn::Ping);
                MenuAction::None
            }
            KeyCode::Digit5 => {
                self.browser.sort_by(SortColumn::Gametype);
                MenuAction::None
            }
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => self.activate_browser_focus(),
            KeyCode::Escape => {
                if self.classic_info {
                    self.classic_info = false;
                    MenuAction::None
                } else if self.filter_editing {
                    self.filter_editing = false;
                    MenuAction::None
                } else {
                    self.close_browser()
                }
            }
            _ => MenuAction::None,
        }
    }

    pub(crate) fn handle_key(
        &mut self,
        event: &KeyEvent,
        console: &mut ViewerConsole,
    ) -> MenuAction {
        self.key_event(event, console, false)
    }

    /// `handle_key`; `pasted` for the event a paste made of the clipboard's text, which is
    /// typed as it is and never read as another paste.
    fn key_event(
        &mut self,
        event: &KeyEvent,
        console: &mut ViewerConsole,
        pasted: bool,
    ) -> MenuAction {
        if !self.is_visible() || event.state != ElementState::Pressed {
            return MenuAction::None;
        }
        // Typing in the main page's chat field, held keys repeat (Backspace).
        if matches!(self.state.phase(), ClientPhase::MainMenu)
            && self.menu_style == MenuStyle::Sjk
            && self.home.is_typing()
        {
            return self.sjk_home_typing(event, console);
        }
        // A field that is open takes held keys (Backspace deletes on, a letter types on);
        // every other key acts once.
        let field_open = self.field_open();
        let (shift, control) = (console.shift_held(), console.control_held());
        if event.repeat && !(field_open && repeats_in_a_field(event, control)) {
            return MenuAction::None;
        }
        // The pages' own fields (settings, key bindings, character, create game) read
        // `event.text`: Ctrl+C copies the field, Ctrl+V types the clipboard's line.
        if field_open
            && !pasted
            && !self.address_editing
            && self.password_target.is_none()
            && !matches!(self.state.phase(), ClientPhase::Browser)
            && let PhysicalKey::Code(key) = event.physical_key
        {
            let text = event.text.as_deref();
            if let Some(field) = self.page_field_text() {
                if crate::text_entry::is_select_all(key, text, control) {
                    crate::text_entry::select_all(field);
                    return MenuAction::None;
                }
                if crate::text_entry::is_selected(field)
                    && !crate::text_entry::is_copy(key, text, control)
                {
                    // All of it is selected: Backspace or Delete clears it, typing or a
                    // paste replaces it; the field is erased with Backspaces, whatever
                    // it is, and the key goes on.
                    let chars = field.chars().count();
                    let erases = matches!(key, KeyCode::Backspace | KeyCode::Delete);
                    let replaces = erases
                        || crate::sjk_chat_field::is_paste(key, text, shift, control)
                        || (!control
                            && text.is_some_and(|text| {
                                !text.is_empty() && text.chars().all(|c| !c.is_control())
                            }));
                    crate::text_entry::deselect();
                    if replaces {
                        self.erase_field(event, console, chars);
                    }
                    if erases {
                        return MenuAction::None;
                    }
                }
            }
            if crate::text_entry::is_copy(key, text, control) {
                if let Some(field) = self.page_field_text()
                    && !field.is_empty()
                {
                    let _ = crate::console::clipboard::copy(field);
                }
                return MenuAction::None;
            }
            if crate::sjk_chat_field::is_paste(key, text, shift, control) {
                let typed = crate::console::clipboard::paste()
                    .and_then(|clipboard| crate::text_entry::pasted_event(event, &clipboard));
                return match typed {
                    Some(typed) => self.key_event(&typed, console, true),
                    None => MenuAction::None,
                };
            }
        }
        if self.address_editing {
            let PhysicalKey::Code(key) = event.physical_key else {
                return MenuAction::None;
            };
            match key {
                KeyCode::Escape => {
                    self.address_editing = false;
                    self.address_error.clear();
                }
                KeyCode::Enter | KeyCode::NumpadEnter => return self.submit_address(),
                _ => {
                    if crate::text_entry::edit(
                        &mut self.address_input,
                        key,
                        event.text.as_deref(),
                        shift,
                        control,
                        255,
                        true,
                    ) {
                        self.address_error.clear();
                    }
                }
            }
            return MenuAction::None;
        }
        if let Some(address) = self.password_target.clone() {
            let PhysicalKey::Code(key) = event.physical_key else {
                return MenuAction::None;
            };
            match key {
                KeyCode::Escape => {
                    self.password_target = None;
                    self.password.clear();
                }
                KeyCode::Enter | KeyCode::NumpadEnter if !self.password.is_empty() => {
                    console.set_cvar("password", &self.password);
                    self.server_passwords
                        .update(&address, &self.password, self.remember_password);
                    self.password_target = None;
                    self.state.connecting(address.clone());
                    return MenuAction::Connect(address);
                }
                _ => {
                    // A password is typed and pasted, never copied back out.
                    crate::text_entry::edit(
                        &mut self.password,
                        key,
                        event.text.as_deref(),
                        shift,
                        control,
                        128,
                        false,
                    );
                }
            }
            return MenuAction::None;
        }
        if matches!(self.state.phase(), ClientPhase::Browser) && self.filter_editing {
            if let PhysicalKey::Code(key) = event.physical_key
                && self
                    .browser
                    .edit_filter(key, event.text.as_deref(), shift, control)
            {
                return MenuAction::None;
            }
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return MenuAction::None;
        };
        match self.state.phase() {
            ClientPhase::MainMenu if self.menu_style == MenuStyle::Sjk => {
                self.sjk_home_key(key, console)
            }
            ClientPhase::MainMenu => match key {
                KeyCode::ArrowUp | KeyCode::KeyW => {
                    self.navigate_classic(AbstractAction::Previous);
                    MenuAction::None
                }
                KeyCode::ArrowDown | KeyCode::KeyS | KeyCode::Tab => {
                    self.navigate_classic(AbstractAction::Next);
                    MenuAction::None
                }
                KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                    self.activate_classic(console)
                }
                KeyCode::Escape => {
                    self.cancel_classic();
                    MenuAction::None
                }
                _ => MenuAction::None,
            },
            ClientPhase::Browser if self.menu_style == MenuStyle::Sjk => {
                match self.sjk_browser_key(key, console) {
                    Some(action) => action,
                    None => self.browser_key(key),
                }
            }
            ClientPhase::Browser => self.browser_key(key),
            ClientPhase::Settings => {
                let result = self.settings.handle_key(event, console);
                self.settings_result(result, console)
            }
            ClientPhase::Keybinds => {
                let result = self.keybinds.handle_key(event, console);
                self.keybinds_result(result, console)
            }
            ClientPhase::Player => {
                let result = self.player.handle_key(event, console);
                self.player_result(result, console)
            }
            ClientPhase::CreateGame => {
                let result = self.create_game.key(key, event.text.as_deref(), console);
                self.create_game_result(result)
            }
            ClientPhase::Connecting(_) if key == KeyCode::Escape => self.cancel_join(),
            ClientPhase::ConnectionError
                if matches!(
                    key,
                    KeyCode::Escape | KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space
                ) =>
            {
                self.state.open_browser();
                MenuAction::None
            }
            ClientPhase::Connecting(_) | ClientPhase::ConnectionError | ClientPhase::InGame => {
                MenuAction::None
            }
        }
    }

    /// Whether a field takes the keys: the address or password prompt, the browser's
    /// search, or a field of the page on show.
    fn field_open(&self) -> bool {
        self.address_editing
            || self.password_target.is_some()
            || match self.state.phase() {
                ClientPhase::Browser => self.filter_editing,
                ClientPhase::Settings => self.settings.typing(),
                ClientPhase::Keybinds => self.keybinds.typing(),
                ClientPhase::Player => self.player.typing(),
                ClientPhase::CreateGame => self.create_game.typing(),
                _ => false,
            }
    }

    /// Erase `chars` characters of the field that has the keys, as that many Backspaces.
    fn erase_field(&mut self, event: &KeyEvent, console: &mut ViewerConsole, chars: usize) {
        let mut backspace = event.clone();
        backspace.physical_key = PhysicalKey::Code(KeyCode::Backspace);
        backspace.logical_key = winit::keyboard::Key::Named(winit::keyboard::NamedKey::Backspace);
        backspace.text = None;
        backspace.repeat = false;
        for _ in 0..chars {
            let _ = self.key_event(&backspace, console, true);
        }
    }

    /// The text of the open field of the page on show, for Ctrl+C.
    fn page_field_text(&self) -> Option<&str> {
        match self.state.phase() {
            ClientPhase::Settings => self.settings.field_text(),
            ClientPhase::Keybinds => self.keybinds.field_text(),
            ClientPhase::Player => self.player.field_text(),
            ClientPhase::CreateGame => self.create_game.field_text(),
            _ => None,
        }
    }

    pub(crate) fn draw_list(&self) -> Option<&DrawList> {
        match self.state.phase() {
            ClientPhase::MainMenu
            | ClientPhase::Browser
            | ClientPhase::Connecting(_)
            | ClientPhase::ConnectionError => Some(self.ui.draw_list()),
            ClientPhase::Settings => Some(self.settings.draw_list()),
            ClientPhase::Keybinds => Some(self.keybinds.draw_list()),
            ClientPhase::Player => Some(self.player.draw_list()),
            ClientPhase::CreateGame => Some(self.create_game.draw_list()),
            ClientPhase::InGame => None,
        }
    }

    /// Selection metadata consumed by the shared backdrop/highlight shader.
    pub(crate) fn visual_selection(&self) -> (f32, f32) {
        match self.state.phase() {
            ClientPhase::MainMenu => (1.0, 0.0),
            ClientPhase::Browser => (2.0, self.browser.selected() as f32),
            ClientPhase::Settings => {
                let (selected, root) = self.settings.visual_selection();
                (if root { 3.0 } else { 4.0 }, selected as f32)
            }
            ClientPhase::Keybinds => (5.0, self.keybinds.visual_selection() as f32),
            ClientPhase::Player => (7.0, self.player.visual_selection() as f32),
            ClientPhase::CreateGame => (8.0, self.create_game.visual_selection() as f32),
            ClientPhase::Connecting(_) => (6.0, 0.0),
            ClientPhase::ConnectionError => (6.0, 0.0),
            ClientPhase::InGame => (0.0, 0.0),
        }
    }

    pub(crate) fn handle_mouse_binding(
        &mut self,
        button: winit::event::MouseButton,
        console: &mut ViewerConsole,
    ) -> bool {
        matches!(self.state.phase(), ClientPhase::Keybinds)
            && self.keybinds.capture_mouse(button, console)
    }

    /// A wheel notch while the key-binding form awaits a key binds it.
    pub(crate) fn handle_wheel_binding(&mut self, up: bool, console: &mut ViewerConsole) -> bool {
        matches!(self.state.phase(), ClientPhase::Keybinds)
            && self.keybinds.capture_wheel(up, console)
    }

    pub(crate) fn joined(&mut self) {
        self.remember_joined_server();
        self.state.entered_game();
    }

    pub(crate) fn state_connecting(&mut self, address: String) {
        // A join leaves the current server; a cancelled or failed one lands
        // in the browser, which then returns to the main menu.
        self.browser_return = ReturnTarget::MainMenu;
        self.state.connecting(address);
    }

    /// A join of `address` starts: the classic loading screen starts over,
    /// with the map the browser row advertised if there was one.
    pub(crate) fn begin_join(&mut self, address: &str) {
        let local = self.hosting_local();
        self.loading.begin(address, local);
        if let Some(map) = &self.destination_map {
            self.loading.set_map(map);
        }
    }

    /// Whether the client hosts the server it joins (Create game), which
    /// leaves the server lines off the loading screen as `sv_running` does.
    pub(crate) fn hosting_local(&self) -> bool {
        self.create_game.hosting()
    }

    /// Show map preparation using the existing cancellable connection notice.
    pub(crate) fn state_loading(&mut self, map: &str) {
        self.state_connecting(map.to_owned());
        self.loading.begin_map_load(map);
        self.state.set_status(format!("Loading {map}..."));
    }

    pub(crate) fn join_failed(&mut self, error: impl Into<String>) {
        self.state.connection_failed(error);
    }

    pub(crate) fn return_to_main_menu(&mut self) {
        self.state.main_menu();
        self.classic.reset();
        self.home.reset();
    }

    pub(crate) fn attach_catalogue(&mut self, vfs: std::sync::Arc<sjk_vfs::VirtualFileSystem>) {
        self.create_game.attach_vfs(std::sync::Arc::clone(&vfs));
        self.keybinds.attach_vfs(std::sync::Arc::clone(&vfs));
        self.player.attach_catalogue(vfs);
    }

    /// Leave or stay on the player screen after one of its events.
    pub(super) fn player_result(
        &mut self,
        result: PlayerMenuResult,
        console: &ViewerConsole,
    ) -> MenuAction {
        match result {
            PlayerMenuResult::None => MenuAction::None,
            PlayerMenuResult::Back(ReturnTarget::MainMenu) => {
                self.state.main_menu();
                MenuAction::None
            }
            PlayerMenuResult::Back(ReturnTarget::InGame) => {
                self.state.entered_game();
                MenuAction::ReturnToGameMenu
            }
            PlayerMenuResult::ClassicPage(page) => {
                self.state.main_menu();
                self.classic.show(page);
                // Settings opens on its first group, as from the main page.
                if let Some(entry) = page.opening_panel() {
                    self.open_classic_panel(
                        console,
                        page,
                        entry,
                        classic::panel::Frame::Main,
                        ReturnTarget::MainMenu,
                    );
                }
                MenuAction::None
            }
        }
    }

    pub(crate) fn open_player(&mut self, console: &ViewerConsole, target: ReturnTarget) {
        self.player.open(console, target);
        self.state.open_player();
    }

    /// Open the player screen as the Profile screen's first tabs
    /// ([`crate::profile_hub`]) in the SJK UI, else on its own.
    pub(crate) fn open_player_hub(&mut self, console: &ViewerConsole, target: ReturnTarget) {
        self.open_player(console, target);
        let sjk = self.player.is_sjk();
        self.player.set_hub(sjk);
    }

    /// Once a frame: apply the player screen's Force page when it was left.
    pub(crate) fn settle_force_page(&mut self, console: &mut ViewerConsole) {
        let visible = self.is_visible() && matches!(self.state.phase(), ClientPhase::Player);
        self.player.settle_force_page(visible, console);
    }

    /// Where the player screen returns when it closes.
    pub(crate) fn player_return_target(&self) -> ReturnTarget {
        self.player.return_target()
    }

    /// Whether the Profile screen's Character, Saber or Force tab shows.
    pub(crate) fn player_hub_shown(&self) -> bool {
        matches!(self.state.phase(), ClientPhase::Player) && self.player.is_hub()
    }

    /// The player screen's page on show as the Profile screen's tab (0 Character,
    /// 1 Saber, 2 Force), when it shows as one.
    pub(crate) fn player_hub_page(&self) -> Option<usize> {
        self.player_hub_shown().then(|| self.player.page_index())
    }

    /// Turn the player screen to page `index` (Character, Saber, Force).
    pub(crate) fn show_player_hub_page(&mut self, index: usize) {
        self.player.show_page(index);
    }

    /// Whether a field of the Profile screen's player pages takes the keys, so Tab
    /// and the brackets stay with it.
    pub(crate) fn player_hub_typing(&self) -> bool {
        self.player_hub_shown() && self.player.typing()
    }

    /// Close the player screen's tabs for another of the Profile screen's tabs, quietly
    /// (the screen behind it shows again under the console's page); returns where it
    /// had been opened from.
    pub(crate) fn leave_player_hub(&mut self) -> ReturnTarget {
        let target = self.player.return_target();
        self.player.set_hub(false);
        match target {
            ReturnTarget::MainMenu => self.state.main_menu(),
            ReturnTarget::InGame => self.state.entered_game(),
        }
        target
    }

    /// Whether the player screen's last frame ran out of room on its canvas (world
    /// shots).
    #[cfg(test)]
    pub(crate) fn player_overflowed(&self) -> bool {
        self.player.overflowed()
    }

    /// The player screen's creator on a species' (or the model's) category `index`
    /// (world shots).
    #[cfg(test)]
    pub(crate) fn creator_for_shot(&mut self, species: bool, index: usize) {
        self.player.creator_for_shot(species, index);
    }

    /// Select the settings row of `cvar`, scrolled into view (world shots).
    #[cfg(test)]
    pub(crate) fn select_setting_for_shot(&mut self, cvar: &str) {
        self.settings.select_cvar(cvar);
    }

    /// Open the resolution list from the Resolution row of the settings on
    /// show, as Enter there does (world shots).
    #[cfg(test)]
    pub(crate) fn resolutions_for_shot(&mut self, console: &ViewerConsole) {
        self.settings.resolutions_for_shot(console);
    }

    /// The renderer settings opened from the classic Graphics page's panel
    /// in `frame` (the main menu's or the in-game one), as its renderer entry
    /// does (world shots, tests).
    #[cfg(test)]
    pub(crate) fn classic_renderer_for_shot(
        &mut self,
        console: &ViewerConsole,
        frame: classic::panel::Frame,
    ) {
        let target = match frame {
            classic::panel::Frame::Main => ReturnTarget::MainMenu,
            classic::panel::Frame::InGame => ReturnTarget::InGame,
        };
        let page = classic::layout::Page::Graphics;
        let entry = classic::layout::Entry::RenderImage;
        self.open_classic_panel(console, page, entry, frame, target);
        if let Some(panel) = self.classic_panel {
            self.open_renderer_from_panel(console, panel);
        }
    }

    /// Show the player screen's page `index` with row `row` selected (world
    /// shots).
    #[cfg(test)]
    pub(crate) fn player_page_for_shot(&mut self, index: usize, row: usize) {
        self.player.show_page_for_shot(index, row);
    }

    #[cfg(test)]
    pub(crate) fn player_hover_level_for_shot(&mut self, power: usize, level: u8) {
        self.player.hover_level_for_shot(power, level);
    }

    #[cfg(test)]
    pub(crate) fn player_server_rules_for_shot(&mut self, rules: sjk_client::ForceLegalizeRules) {
        self.player.server_rules_for_shot(rules);
    }

    #[cfg(test)]
    pub(crate) fn player_dual_for_shot(&mut self) {
        self.player.dual_for_shot();
    }

    /// The Saber page's hilt search showing `search` (typed when `typing`) and its
    /// blade choice offering what `unlocks` own (world shots).
    #[cfg(test)]
    pub(crate) fn player_saber_for_shot(
        &mut self,
        console: &ViewerConsole,
        search: &str,
        typing: bool,
        unlocks: Vec<sjk_identity::Unlock>,
    ) {
        self.player.saber_for_shot(console, search, typing, unlocks);
    }
}

/// Whether a held `event` repeats in an open text field: Backspace, or a key that types
/// (not with Ctrl held: a held Ctrl+V pastes once).
fn repeats_in_a_field(event: &KeyEvent, control: bool) -> bool {
    matches!(event.physical_key, PhysicalKey::Code(KeyCode::Backspace))
        || (!control
            && event
                .text
                .as_deref()
                .is_some_and(|text| !text.is_empty() && text.chars().all(|c| !c.is_control())))
}
