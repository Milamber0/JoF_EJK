//! Create game: the player picks a mode, a map, bots and limits, and the
//! client starts its own `sjk-server` on this machine and joins it — the
//! stock "Create game" flow on SJK's native server. The choices are kept in
//! archived `ui_host*` cvars; the server process is owned here, so it lives
//! exactly as long as the hosted game (see [`crate::connection::local_server`]).

use super::create_game_catalog::{Catalogue, MODES, ScoreLimit, mode_index, pick_bots};
use super::levelshot::Levelshots;
use super::map_picker::{MapPicker, PickerResult};
use crate::connection::local_server::{HostSettings, LocalServer, ServerPoll};
use crate::console::ViewerConsole;
use crate::menu_widgets::MenuCanvas;
use std::net::SocketAddr;
use std::sync::Arc;
use winit::keyboard::KeyCode;

/// The screen's rows, top to bottom.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Row {
    Mode,
    Map,
    Bots,
    BotSkill,
    ScoreLimit,
    TimeLimit,
    Hostname,
    Lan,
    Start,
}

/// Every row in screen order; a row's token is its index.
pub(crate) const ROWS: [Row; 9] = [
    Row::Mode,
    Row::Map,
    Row::Bots,
    Row::BotSkill,
    Row::ScoreLimit,
    Row::TimeLimit,
    Row::Hostname,
    Row::Lan,
    Row::Start,
];

/// The top bar's way back (its Esc key cap): to the main menu, or from the
/// map list to the screen. The value other screens' way back has.
pub(crate) const BACK_TOKEN: u16 = 900;

/// Most bots one hosted match takes.
pub(crate) const MAX_BOTS: u32 = 20;
/// Longest server name.
const HOSTNAME_BYTES: usize = 48;
/// Name a new server gets.
const DEFAULT_HOSTNAME: &str = "SJK local game";

/// The archived cvars that remember the last choices: (name, default, help).
pub(crate) const INTEGER_CVARS: [(&str, i64, &str); 6] = [
    ("ui_hostBots", 5, "Create game: bots added to the match"),
    ("ui_hostBotSkill", 3, "Create game: bot skill, 1 to 5"),
    (
        "ui_hostFraglimit",
        20,
        "Create game: frag limit, 0 for none",
    ),
    (
        "ui_hostCapturelimit",
        8,
        "Create game: capture limit, 0 for none",
    ),
    (
        "ui_hostTimelimit",
        0,
        "Create game: time limit in minutes, 0 for none",
    ),
    (
        "ui_hostLan",
        0,
        "Create game: 1 lets LAN players join (binds every interface)",
    ),
];
/// Text cvars that remember the last choices: (name, default, help).
pub(crate) const TEXT_CVARS: [(&str, &str, &str); 3] = [
    ("ui_hostMode", "ffa", "Create game: sjk-server game type"),
    ("ui_hostMap", "mp/ffa3", "Create game: map"),
    ("ui_hostName", DEFAULT_HOSTNAME, "Create game: server name"),
];

/// The player's current choices.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Draft {
    pub(crate) mode: usize,
    pub(crate) map: String,
    pub(crate) bots: u32,
    pub(crate) bot_skill: u8,
    pub(crate) fraglimit: u32,
    pub(crate) capturelimit: u32,
    pub(crate) timelimit: u32,
    pub(crate) hostname: String,
    pub(crate) allow_lan: bool,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            mode: 0,
            map: "mp/ffa3".to_owned(),
            bots: 5,
            bot_skill: 3,
            fraglimit: 20,
            capturelimit: 8,
            timelimit: 0,
            hostname: DEFAULT_HOSTNAME.to_owned(),
            allow_lan: false,
        }
    }
}

impl Draft {
    /// The draft the cvars remember.
    pub(crate) fn load(console: &ViewerConsole) -> Self {
        let defaults = Self::default();
        let integer = |name: &str, fallback: u32| {
            console
                .integer_cvar(name)
                .map_or(fallback, |value| value.clamp(0, 999) as u32)
        };
        let text = |name: &str| {
            console
                .text_value(name)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        };
        Self {
            mode: text("ui_hostMode").and_then(mode_index).unwrap_or(0),
            map: text("ui_hostMap").map_or(defaults.map, str::to_owned),
            bots: integer("ui_hostBots", defaults.bots).min(MAX_BOTS),
            bot_skill: integer("ui_hostBotSkill", 3).clamp(1, 5) as u8,
            fraglimit: integer("ui_hostFraglimit", defaults.fraglimit),
            capturelimit: integer("ui_hostCapturelimit", defaults.capturelimit),
            timelimit: integer("ui_hostTimelimit", defaults.timelimit),
            hostname: text("ui_hostName").map_or(defaults.hostname, str::to_owned),
            allow_lan: integer("ui_hostLan", 0) != 0,
        }
    }

    /// Write the draft back to its cvars.
    pub(crate) fn save(&self, console: &mut ViewerConsole) {
        console.set_cvar("ui_hostMode", MODES[self.mode].server);
        console.set_cvar("ui_hostMap", &self.map);
        for (name, value) in [
            ("ui_hostBots", self.bots),
            ("ui_hostBotSkill", u32::from(self.bot_skill)),
            ("ui_hostFraglimit", self.fraglimit),
            ("ui_hostCapturelimit", self.capturelimit),
            ("ui_hostTimelimit", self.timelimit),
            ("ui_hostLan", u32::from(self.allow_lan)),
        ] {
            console.set_cvar(name, &value.to_string());
        }
        console.set_cvar("ui_hostName", &self.hostname);
    }

    /// The server settings this draft asks for, bots drawn from `roster`.
    pub(crate) fn host_settings(&self, roster: &[String], seed: u64) -> HostSettings {
        let mode = MODES[self.mode];
        let hostname = self.hostname.trim();
        HostSettings {
            map: self.map.clone(),
            gametype: mode.server,
            hostname: if hostname.is_empty() {
                DEFAULT_HOSTNAME.to_owned()
            } else {
                hostname.to_owned()
            },
            bots: pick_bots(roster, self.bots as usize, seed),
            bot_skill: self.bot_skill,
            fraglimit: if mode.score == ScoreLimit::Frags {
                self.fraglimit
            } else {
                0
            },
            timelimit: self.timelimit,
            capturelimit: if mode.score == ScoreLimit::Captures {
                self.capturelimit
            } else {
                0
            },
            allow_lan: self.allow_lan,
            cheats: false,
        }
    }

    /// Change `row` by `step` notches; the map follows a mode change when
    /// the new mode cannot be played on it.
    pub(crate) fn adjust(&mut self, row: Row, step: isize, catalogue: &Catalogue) {
        let bump = |value: u32, by: u32, max: u32| {
            (value as i64 + step as i64 * i64::from(by)).clamp(0, i64::from(max)) as u32
        };
        match row {
            Row::Mode => {
                self.mode = (self.mode as isize + step).rem_euclid(MODES.len() as isize) as usize;
                if catalogue.map(self.mode, &self.map).is_none() {
                    if let Some(first) = catalogue.maps_for(self.mode).next() {
                        self.map = first.name.clone();
                    }
                }
            }
            Row::Map => {
                if let Some(next) = catalogue.step_map(self.mode, &self.map, step) {
                    self.map = next.name.clone();
                }
            }
            Row::Bots => self.bots = bump(self.bots, 1, MAX_BOTS),
            Row::BotSkill => self.bot_skill = bump(u32::from(self.bot_skill), 1, 5).max(1) as u8,
            Row::ScoreLimit => match MODES[self.mode].score {
                ScoreLimit::Frags => self.fraglimit = bump(self.fraglimit, 5, 200),
                ScoreLimit::Captures => self.capturelimit = bump(self.capturelimit, 1, 50),
                ScoreLimit::None => {}
            },
            Row::TimeLimit => self.timelimit = bump(self.timelimit, 5, 120),
            Row::Lan => self.allow_lan = !self.allow_lan,
            Row::Hostname | Row::Start => {}
        }
    }
}

/// What a key or click on the screen asks of the menu.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum CreateGameResult {
    None,
    /// Leave toward the main menu.
    Back,
    /// Start a server with these settings.
    Start(HostSettings),
}

/// Create game screen state and the server it started.
pub(crate) struct CreateGameMenu {
    pub(super) ui: MenuCanvas,
    pub(super) selected: usize,
    pub(super) draft: Draft,
    pub(super) catalogue: Catalogue,
    vfs: Option<Arc<sjk_vfs::VirtualFileSystem>>,
    catalogue_loaded: bool,
    /// The server name being typed, while the name row is edited.
    pub(super) editing: Option<String>,
    /// One status line under the rows (starting, failures).
    pub(super) status: String,
    server: Option<LocalServer>,
    /// The server answered: its address waits to be joined.
    join_pending: Option<SocketAddr>,
    /// The answering server's address was handed out for joining.
    joined: bool,
    /// The map list opened from the Map row.
    pub(super) picker: MapPicker,
    /// Map previews (the form's small one and the list's large one).
    pub(super) levelshots: Levelshots,
}

impl CreateGameMenu {
    pub(crate) fn new() -> Self {
        Self {
            ui: MenuCanvas::new(),
            selected: 0,
            draft: Draft::default(),
            catalogue: Catalogue::default(),
            vfs: None,
            catalogue_loaded: false,
            editing: None,
            status: String::with_capacity(160),
            server: None,
            join_pending: None,
            joined: false,
            picker: MapPicker::new(),
            levelshots: Levelshots::new(),
        }
    }

    /// The mounted game data the maps and bots are read from.
    pub(crate) fn attach_vfs(&mut self, vfs: Arc<sjk_vfs::VirtualFileSystem>) {
        self.levelshots.attach_vfs(Arc::clone(&vfs));
        self.vfs = Some(vfs);
        self.catalogue_loaded = false;
    }

    /// Open the screen on the remembered choices.
    pub(crate) fn open(&mut self, console: &ViewerConsole) {
        if !self.catalogue_loaded {
            if let Some(vfs) = &self.vfs {
                self.catalogue = Catalogue::from_vfs(vfs);
                self.catalogue_loaded = true;
            }
        }
        self.draft = Draft::load(console);
        // A remembered map that is gone, or not played in this mode: the
        // mode's first map instead.
        if self
            .catalogue
            .map(self.draft.mode, &self.draft.map)
            .is_none()
        {
            if let Some(first) = self.catalogue.maps_for(self.draft.mode).next() {
                self.draft.map = first.name.clone();
            }
        }
        self.selected = ROWS.len() - 1;
        self.editing = None;
        self.picker.close();
        if self.server.is_none() {
            self.status.clear();
        }
    }

    /// The screen's retained draw list.
    /// Whether a field takes the keys: the server name or the map list's search.
    pub(crate) fn typing(&self) -> bool {
        self.editing.is_some() || self.picker.is_open()
    }

    /// The text of the field that has the keys, for Ctrl+C.
    pub(crate) fn field_text(&self) -> Option<&str> {
        if self.picker.is_open() {
            return Some(self.picker.filter());
        }
        self.editing.as_deref()
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    /// The selected row, for the backdrop highlight.
    pub(crate) fn visual_selection(&self) -> usize {
        if self.picker.is_open() {
            return self.picker.selected().saturating_sub(self.picker.first());
        }
        self.selected
    }

    /// Ask for the shown map's preview and hand a freshly decoded one to
    /// `upload` (the UI renderer's levelshot texture); once per frame.
    pub(crate) fn service_levelshots(
        &mut self,
        upload: impl FnMut(&super::levelshot::LevelshotImage),
    ) {
        self.levelshots
            .want(shown_map(&self.picker, &self.catalogue, &self.draft));
        self.levelshots.service(upload);
    }

    /// Upload the wanted levelshot again, into another world's texture.
    pub(crate) fn forget_levelshot_upload(&mut self) {
        self.levelshots.forget_upload();
    }

    /// Ask for `map`'s levelshot for another screen (the classic loading
    /// screen) through the same cache, which owns the renderer's one preview
    /// texture, and hand a freshly decoded one to `upload`.
    pub(crate) fn service_levelshot_for(
        &mut self,
        map: &str,
        upload: impl FnMut(&super::levelshot::LevelshotImage),
    ) {
        self.levelshots.want(map);
        self.levelshots.service(upload);
    }

    /// What the preview slot shows for `map`.
    pub(crate) fn levelshot_preview(&self, map: &str) -> super::levelshot::Preview {
        self.levelshots.preview(map)
    }

    /// The pixel size of `map`'s levelshot while the preview texture holds it.
    pub(crate) fn levelshot_size(&self, map: &str) -> Option<[u32; 2]> {
        self.levelshots.size(map)
    }

    /// Whether a server this screen started is running.
    pub(crate) fn hosting(&self) -> bool {
        self.server.is_some()
    }

    /// Whether the server is started but not yet answering.
    pub(crate) fn starting(&self) -> bool {
        self.server.is_some() && self.join_pending.is_none() && !self.joined
    }

    /// Handle one key on the screen; `text` is what the key typed.
    pub(crate) fn key(
        &mut self,
        key: KeyCode,
        text: Option<&str>,
        console: &mut ViewerConsole,
    ) -> CreateGameResult {
        if self.picker.is_open() {
            if let PickerResult::Close(picked) = self.picker.key(key, text, &self.catalogue) {
                self.map_picked(picked, console);
            }
            return CreateGameResult::None;
        }
        if let Some(buffer) = &mut self.editing {
            match key {
                KeyCode::Escape => self.editing = None,
                KeyCode::Enter | KeyCode::NumpadEnter => {
                    self.draft.hostname = self.editing.take().unwrap_or_default().trim().to_owned();
                    self.draft.save(console);
                }
                KeyCode::Backspace => {
                    buffer.pop();
                }
                _ => {
                    if let Some(text) = text {
                        let room = HOSTNAME_BYTES.saturating_sub(buffer.len());
                        buffer.extend(
                            text.chars()
                                .filter(|c| !c.is_control() && *c != '"')
                                .take(room),
                        );
                    }
                }
            }
            return CreateGameResult::None;
        }
        match key {
            KeyCode::ArrowUp | KeyCode::KeyW => self.move_selection(-1),
            KeyCode::ArrowDown | KeyCode::KeyS | KeyCode::Tab => self.move_selection(1),
            KeyCode::ArrowLeft | KeyCode::KeyA => self.adjust(-1, console),
            KeyCode::ArrowRight | KeyCode::KeyD => self.adjust(1, console),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                return self.activate(console);
            }
            KeyCode::Escape => return CreateGameResult::Back,
            _ => {}
        }
        CreateGameResult::None
    }

    pub(super) fn move_selection(&mut self, step: isize) {
        self.selected = (self.selected as isize + step).rem_euclid(ROWS.len() as isize) as usize;
    }

    /// Step the selected row and remember the result.
    pub(super) fn adjust(&mut self, step: isize, console: &mut ViewerConsole) {
        if self.hosting() {
            return;
        }
        self.draft
            .adjust(ROWS[self.selected], step, &self.catalogue);
        self.draft.save(console);
    }

    /// Enter on the selected row: start, edit the name, or step forward.
    pub(super) fn activate(&mut self, console: &mut ViewerConsole) -> CreateGameResult {
        match ROWS[self.selected] {
            Row::Start => self.start_request(),
            Row::Hostname if !self.hosting() => {
                self.editing = Some(self.draft.hostname.clone());
                CreateGameResult::None
            }
            Row::Map if !self.hosting() => {
                self.open_map_picker();
                CreateGameResult::None
            }
            _ => {
                self.adjust(1, console);
                CreateGameResult::None
            }
        }
    }

    /// Open the map list on the chosen map.
    pub(super) fn open_map_picker(&mut self) {
        self.picker
            .open(&self.catalogue, self.draft.mode, &self.draft.map);
    }

    /// The map list closed; `picked` becomes the map.
    pub(super) fn map_picked(&mut self, picked: Option<String>, console: &mut ViewerConsole) {
        if let Some(map) = picked {
            self.draft.map = map;
            self.draft.save(console);
        }
    }

    /// The Start row: the settings to launch, unless a server already runs
    /// or there is nothing to play on.
    fn start_request(&mut self) -> CreateGameResult {
        if self.hosting() {
            return CreateGameResult::None;
        }
        if self
            .catalogue
            .map(self.draft.mode, &self.draft.map)
            .is_none()
        {
            self.status.clear();
            self.status.push_str("No installed map plays this mode.");
            return CreateGameResult::None;
        }
        let mut seed = [0; 8];
        let seed =
            getrandom::fill(&mut seed).map_or(0x9E37_79B9_7F4A_7C15, |()| u64::from_le_bytes(seed));
        CreateGameResult::Start(self.draft.host_settings(self.catalogue.bots(), seed))
    }

    /// Take ownership of the server just started.
    pub(crate) fn server_started(&mut self, server: LocalServer) {
        crate::log::progress(format_args!(
            "local server starting; log {}",
            server.log_path().display()
        ));
        self.status.clear();
        self.status.push_str("Starting the server...");
        self.server = Some(server);
        self.join_pending = None;
        self.joined = false;
    }

    /// The server could not be started or failed while starting.
    pub(crate) fn server_failed(&mut self, reason: &str) {
        crate::log::progress(format_args!("local server failed: {reason}"));
        self.stop_server();
        self.status.clear();
        self.status.push_str(reason);
    }

    /// Follow a starting server; once it answers its address is queued for
    /// [`Self::take_join`]. Returns a failure to show.
    pub(crate) fn poll_server(&mut self) -> Option<String> {
        if !self.starting() {
            return None;
        }
        match self.server.as_mut()?.poll() {
            ServerPoll::Starting => None,
            ServerPoll::Ready(address) => {
                crate::log::progress(format_args!("local server answering at {address}"));
                self.join_pending = Some(address);
                None
            }
            ServerPoll::Failed(reason) => {
                self.server_failed(&reason);
                Some(reason)
            }
        }
    }

    /// The address to join, once, when the server has started answering.
    pub(crate) fn take_join(&mut self) -> Option<SocketAddr> {
        let address = self.join_pending.take()?;
        self.joined = true;
        Some(address)
    }

    /// Stop the hosted server, if any; returns whether one ran.
    pub(crate) fn stop_server(&mut self) -> bool {
        self.join_pending = None;
        self.joined = false;
        match self.server.take() {
            Some(server) => {
                server.stop();
                self.status.clear();
                true
            }
            None => false,
        }
    }

    /// Stop the hosted server unless `address` is where it listens: joining
    /// any other server ends the hosted game.
    pub(crate) fn release_unless(&mut self, address: &str) {
        let ours = self.server.as_ref().and_then(LocalServer::address);
        let same = ours.is_some_and(|ours| {
            address
                .parse::<SocketAddr>()
                .is_ok_and(|joined| joined == ours)
        });
        if !same {
            self.stop_server();
        }
    }
}

#[cfg(test)]
impl CreateGameMenu {
    /// Choose row `selected`, and open the map list with `filter` typed when
    /// it is given (world shots and tests).
    pub(crate) fn for_shot(&mut self, selected: usize, filter: Option<&str>) {
        self.selected = selected.min(ROWS.len() - 1);
        self.editing = None;
        self.picker.close();
        if let Some(filter) = filter {
            self.open_map_picker();
            for c in filter.chars() {
                let mut typed = [0; 4];
                let _ = self.picker.key(
                    KeyCode::KeyA,
                    Some(c.encode_utf8(&mut typed)),
                    &self.catalogue,
                );
            }
        }
    }

    /// Whether the shown map's levelshot has decoded (or is known missing).
    pub(crate) fn picture_settled(&self) -> bool {
        self.levelshots
            .preview(shown_map(&self.picker, &self.catalogue, &self.draft))
            != super::levelshot::Preview::Loading
    }

    /// Whether the last frame ran out of room on the screen's canvas.
    pub(crate) fn overflowed(&self) -> bool {
        self.ui.overflowed()
    }
}

/// The map whose preview shows: the list's highlight while it is open,
/// else the chosen map.
fn shown_map<'a>(picker: &MapPicker, catalogue: &'a Catalogue, draft: &'a Draft) -> &'a str {
    if picker.is_open() {
        return picker
            .highlighted(catalogue)
            .map_or("", |entry| entry.name.as_str());
    }
    &draft.map
}
