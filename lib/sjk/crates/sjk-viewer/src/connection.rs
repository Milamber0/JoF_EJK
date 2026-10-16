//! Legacy multiplayer connection composition for the viewer.

use crate::log::{self, ConnectTimeline, TimelinePhase};
use sjk_client::{ClientSession, CompatProfile, JoinPhase, LegacyTeamChoice, enter_play};
use sjk_network::{LegacyUserInfo, query_server_info, resolve_server};
use sjk_protocol::{GameState, InfoString};
use sjk_vfs::Pk3Fingerprint;
use std::error::Error;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

#[path = "local_game.rs"]
mod local_game;
#[path = "local_server.rs"]
pub(crate) mod local_server;
#[path = "connection_task.rs"]
mod task;
pub(crate) use task::{JoinPoll, JoinTask, JoinedSession};

impl crate::GpuState {
    /// End a failed live session and retain its reason on the connection screen.
    pub(crate) fn session_disconnected(&mut self, reason: String) {
        if let Some(menu) = &mut self.client_menu {
            menu.stop_local_server();
        }
        self.live_session = None;
        self.leave_session();
        self.gameplay_input.clear();
        self.game_menu = false;
        self.release_pointer();
        crate::session_transition::show_disconnect(
            reason,
            self.console.as_mut(),
            self.client_menu.as_mut(),
        );
    }

    pub(crate) fn begin_address_join(&mut self, address: String) {
        let address = match sjk_client::LegacyServerAddress::parse(&address) {
            Ok(address) => address.into_string(),
            Err(error) => {
                self.connection_error(format!("Bad server address: {error}"));
                return;
            }
        };
        self.leave_session();
        self.last_connect_address = Some(address.clone());
        if let Some(menu) = &mut self.client_menu {
            menu.release_local_server_unless(&address);
            menu.set_last_address(&address);
            menu.state_connecting(address.clone());
            menu.begin_join(&address);
        }
        if let Some(console) = &mut self.console {
            console.set_cvar("cl_reconnectArgs", &address);
            console.close_for_connection();
        }
        let result = self
            .console
            .as_ref()
            .ok_or_else(|| "client console is unavailable".into())
            .and_then(|console: &crate::console::ViewerConsole| console.userinfo());
        match result {
            Ok(userinfo) => {
                self.load_event_gap.start(Instant::now());
                self.join_task = Some(JoinTask::start_address(
                    address,
                    self.game_data.clone(),
                    userinfo,
                    self.console
                        .as_ref()
                        .and_then(|c| c.integer_cvar("cl_allowDownload"))
                        .unwrap_or(1)
                        != 0,
                    self.console.as_ref().map(|c| c.guid_policy()),
                ));
            }
            Err(error) => {
                if let Some(menu) = &mut self.client_menu {
                    menu.join_failed(error.to_string());
                }
            }
        }
    }

    pub(crate) fn apply_client_menu_action(&mut self, action: crate::menu::MenuAction) {
        match action {
            crate::menu::MenuAction::None => {}
            crate::menu::MenuAction::Connect(address) => self.begin_address_join(address),
            crate::menu::MenuAction::CancelJoin => {
                if self.is_menu_world {
                    self.leave_session();
                } else {
                    self.disconnect_to_menu();
                }
                if let Some(console) = &mut self.console {
                    console.download_status(None);
                }
            }
            crate::menu::MenuAction::HostGame(settings) => self.start_local_game(settings),
            crate::menu::MenuAction::Quit => self.quit_requested = true,
            crate::menu::MenuAction::ReturnToGameMenu => {
                self.game_menu = true;
                self.game_menu_page = crate::ingame_menu::Page::Main;
                // The SJK UI comes back on the entry that opened the screen.
                self.game_menu_row = self.in_game_menu.return_row();
                self.in_game_menu.focus = crate::ingame_menu::sjk_focus::Focus::List;
            }
        }
    }

    pub(crate) fn disconnect_to_menu(&mut self) {
        self.leave_session();
        if let Some(menu) = &mut self.client_menu {
            menu.stop_local_server();
        }
        self.game_menu = false;
        if let Some(console) = &mut self.console {
            console.close_for_connection();
        }
        if let Some(menu) = &mut self.client_menu {
            menu.return_to_main_menu();
        }
    }

    pub(crate) fn reconnect_last(&mut self) {
        let saved = self
            .console
            .as_ref()
            .and_then(|c| c.text_cvar("cl_reconnectargs").ok())
            .filter(|s| !s.is_empty())
            .or(self.last_connect_address.as_deref());
        match crate::connection_commands::reconnect_target(saved) {
            Ok(address) => self.begin_address_join(address),
            Err(error) => self.connection_error(error.to_owned()),
        }
    }

    pub(crate) fn leave_session(&mut self) {
        self.end_free_camera();
        self.ghost_leave();
        self.resident.disconnect();
        self.world_load_map.clear();
        self.net_timing.end_session();
        self.portal.aim(None, None);
        self.join_task = None;
        self.world_load_task = None;
        self.world_install_task = None;
        self.world_load_state = crate::session_transition::LoadStateMachine::new();
        self.pending_map_reload = false;
        if let Some(session) = &mut self.live_session {
            let _ = session.disconnect();
        }
        self.live_session = None;
        self.demo_session = None;
        self.server_clock.reset_connection(Instant::now());
        if let Some(console) = &mut self.console {
            console.download_status(None);
            console.clear_server_info();
        }
    }

    fn connection_error(&mut self, error: String) {
        log::progress(format_args!("Connection failed: {error}"));
        if let Some(console) = &mut self.console {
            console.push_log(format!("^1Connection failed: {error}"));
            console.close_for_connection();
        }
        if let Some(menu) = &mut self.client_menu {
            menu.stop_local_server();
            menu.join_failed(error);
        }
    }

    pub(crate) fn poll_client_shell(&mut self) {
        self.poll_resident_connection();
        if let Some(menu) = &mut self.client_menu {
            if let Some(console) = &mut self.console {
                menu.configure_browser(console);
                menu.settle_force_page(console);
            }
            menu.poll();
        }
        self.join_local_game();
        if self
            .live_session
            .as_ref()
            .is_some_and(ClientSession::needs_download)
        {
            self.retain_world_for_connection();
        }
        if self
            .resident
            .session
            .as_ref()
            .is_some_and(ClientSession::needs_download)
        {
            let session = self.resident.session.take().expect("checked above");
            self.world_load_task = None;
            self.world_install_task = None;
            self.pending_map_reload = false;
            self.join_task = Some(JoinTask::resume(session));
            if self.is_menu_world
                && let Some(menu) = &mut self.client_menu
            {
                menu.state_connecting("Downloading server content".into());
            }
        }
        let Some(result) = self.join_task.as_ref().map(JoinTask::poll) else {
            return;
        };
        match result {
            JoinPoll::Pending => {}
            JoinPoll::Prepared(game) => {
                if let Some(menu) = &mut self.client_menu {
                    let local = menu.hosting_local();
                    menu.loading_mut()
                        .set_game(&game, local, &self.localization.strings);
                }
                self.prepare_gate_game(&game);
            }
            JoinPoll::Map(map) => {
                if let Some(menu) = &mut self.client_menu {
                    menu.loading_mut().set_map(&map);
                }
                self.world_load_map = format!("maps/{map}.bsp");
            }
            JoinPoll::Phase(phase) => {
                if let Some(menu) = &mut self.client_menu {
                    menu.loading_mut().set_phase(phase);
                }
            }
            JoinPoll::Progress(text) => {
                let text = self.console.as_mut().map_or_else(
                    || text.clone(),
                    |console| console.download_status(Some(&text)),
                );
                if self.is_menu_world
                    && let Some(menu) = &mut self.client_menu
                {
                    menu.state_connecting(text);
                }
            }
            JoinPoll::Joined(joined) => {
                let session = joined.session;
                if let Some(console) = &mut self.console {
                    console.download_status(None);
                    console.set_server_info(&session);
                    if !joined.forcepowers.is_empty() {
                        console.note_server_forcepowers(&joined.forcepowers);
                    }
                }
                // A fresh join or completed map download establishes a new time anchor.
                // Input may be due before another receive, so discard the old clock.
                let now = Instant::now();
                self.server_clock.reset_connection(now);
                self.local_prediction.clear_pending();
                self.resident.session = Some(*session);
                self.resident.after_sequence = None;
                // Arm the silence watch from the join, not from the first
                // snapshot: a session that never receives one is exactly the
                // case worth reporting.
                self.net_timing.begin_session(now);
                self.connect_timeline = Some(joined.timeline);
                self.pending_map_reload = !self.attach_prepared_session();
                self.game_menu = false;
                self.auto_opened_team_menu = false;
                // The menu stays in its connect phase: the map load that
                // follows names the map, and the install enters the game.
                // Leaving the phase here, even for a frame, would tear down
                // the gate's preview of that map and pull the glide back.
                self.join_task = None;
            }
            JoinPoll::Failed(error) => {
                if let Some(console) = &mut self.console {
                    console.download_status(None);
                }
                self.leave_session();
                self.connection_error(error);
            }
        }
    }
}

fn join_with_storage(
    address: &str,
    game_data: &Path,
    userinfo: &LegacyUserInfo,
    storage: Option<Box<dyn sjk_client::download::DownloadStorage>>,
    guid: Option<crate::client_guid::Policy>,
    prepared: mpsc::SyncSender<GameState>,
    phases: mpsc::Sender<TimelinePhase>,
    map_known: impl FnOnce(String),
) -> Result<JoinedSession, Box<dyn Error>> {
    let server = resolve_server(address)?;
    let profile = query_server_info(server, Duration::from_millis(650))
        .map(|info| {
            if let Some(map) = info.get("mapname") {
                map_known(map.to_owned());
            }
            CompatProfile::from_server_info(&info)
        })
        .unwrap_or(CompatProfile::BaseJka);
    join_socket(
        server, profile, game_data, userinfo, storage, guid, prepared, phases,
    )
}

/// Stamp this client's `ja_guid` for `server` into the userinfo it connects
/// with. Computed per server, as `cl_guidServerUniq` has it by default.
fn with_server_guid(
    userinfo: &LegacyUserInfo,
    server: SocketAddr,
    policy: Option<&crate::client_guid::Policy>,
) -> LegacyUserInfo {
    let mut userinfo = userinfo.clone();
    if let Some(policy) = policy {
        userinfo.guid = match policy.identity(server) {
            Ok(guid) => guid,
            Err(error) => {
                log::progress(format_args!("could not load client identity: {error}"));
                None
            }
        };
        return userinfo;
    }
    if userinfo.guid.is_none() {
        match crate::platform::user_config_file()
            .and_then(|config| crate::client_guid::ja_guid(&config, server))
        {
            Ok(guid) => userinfo.guid = Some(guid),
            Err(error) => log::progress(format_args!("could not load client identity: {error}")),
        }
    }
    userinfo
}

fn join_socket(
    server: SocketAddr,
    profile: CompatProfile,
    game_data: &Path,
    userinfo: &LegacyUserInfo,
    storage: Option<Box<dyn sjk_client::download::DownloadStorage>>,
    guid: Option<crate::client_guid::Policy>,
    prepared: mpsc::SyncSender<GameState>,
    phases: mpsc::Sender<TimelinePhase>,
) -> Result<JoinedSession, Box<dyn Error>> {
    let assets = Pk3Fingerprint::open(game_data.join("base/assets3.pk3"))?;
    log::progress(format_args!("connecting to {server}"));
    let userinfo = &with_server_guid(userinfo, server, guid.as_ref());
    let mut timeline = ConnectTimeline::new();
    let mut session = ClientSession::join_with_downloads(
        server,
        userinfo,
        profile,
        Duration::from_secs(5),
        move |game_state| {
            let _ = prepared.try_send(game_state.clone());
            base_jka_pure_command(game_state, &assets)
        },
        |phase| {
            let phase = match phase {
                JoinPhase::Challenge => TimelinePhase::Challenge,
                JoinPhase::Connected => TimelinePhase::Connected,
                JoinPhase::Gamestate => TimelinePhase::Gamestate,
                JoinPhase::FirstSnapshot => TimelinePhase::FirstSnapshot,
                JoinPhase::UnknownReply(command) => {
                    log::progress(format_args!(
                        "connect: unrecognized OOB reply command={:?}",
                        String::from_utf8_lossy(command),
                    ));
                    return;
                }
            };
            // Logged as it happens: a join that stalls leaves its last phase
            // in the log instead of only "connecting to".
            log::progress(format_args!("join: {phase:?}"));
            timeline.mark(phase);
            let _ = phases.send(phase);
        },
        storage,
    )?;
    // Servers place a new client in spectator state; the stock join UI's
    // Force-menu accept enters the team without a bounce.
    let mut userinfo = userinfo.clone();
    log_pure_requirements(session.game_state());
    match enter_play(&mut session, &mut userinfo, LegacyTeamChoice::Free) {
        Ok(outcome) => {
            if !outcome.active {
                // Not a failure: the server has us, it just has not spawned
                // us yet. A JA+ or JAPro server can hold a fresh client in
                // spectator until it picks a team, and a mod's game module
                // need not answer the force profile with `nfr`. A stock
                // client sits there connected with the join menu open, which
                // is what the spectator reflex in `present_live_snapshot`
                // does; hanging up instead locked the client out of servers it was
                // already connected to.
                log::progress(format_args!(
                    "join: not spawned yet (force profile confirmed={}, spectating={}); \
                     entering as spectator",
                    outcome.confirmed, outcome.spectating,
                ));
            }
            Ok(JoinedSession {
                session: Box::new(session),
                timeline,
                forcepowers: outcome.forcepowers,
            })
        }
        Err(error) => {
            let _ = session.disconnect();
            Err(error.into())
        }
    }
}

/// Report what a pure server demands of us, so a refused join can be read
/// against it. `sv_referencedPakNames` is the list our `cp` has to answer.
fn log_pure_requirements(game_state: &GameState) {
    let Some(system_info) = game_state
        .config_string(1)
        .and_then(|raw| std::str::from_utf8(raw).ok())
        .and_then(|text| InfoString::parse(text).ok())
    else {
        return;
    };
    let paks = system_info.get("sv_referencedPakNames").unwrap_or("<none>");
    log::progress(format_args!(
        "join: sv_pure={} referenced paks: {}",
        system_info.get("sv_pure").unwrap_or("<absent>"),
        paks.split(' ').take(12).collect::<Vec<_>>().join(" "),
    ));
}

fn base_jka_pure_command(game_state: &GameState, assets: &Pk3Fingerprint) -> Option<Vec<u8>> {
    // Answer every pure server, whatever mod it runs. A client that stays
    // silent is never entered into the world (`sv_client.cpp:1394-1400`), and
    // the JKA ecosystem expects any client to be able to join a JA+ or JAPro
    // server, so keying this on the server running stock JKA locked the client out
    // of most of it.
    let checksum = assets.pure_checksum(game_state.checksum_feed);
    let command = sjk_client::legacy_pure_checksum_command(game_state, checksum)?;
    log::progress(format_args!(
        "join: pure reply {:?} (checksum feed {})",
        String::from_utf8_lossy(&command),
        game_state.checksum_feed,
    ));
    Some(command)
}
