//! Viewer presentation and input adapter for the portable client shell.

use super::{TextVertex, UiFont};
use crate::keybind_editor;
#[path = "asset_pack_browser.rs"]
pub(crate) mod asset_browser;
#[path = "console_browser.rs"]
pub(crate) mod browser;
#[path = "changelog.rs"]
pub(crate) mod changelog;
#[path = "console_chat_log.rs"]
mod chat_log;
#[path = "console_client_options.rs"]
mod client_options;
#[path = "clipboard.rs"]
pub(crate) mod clipboard;
#[path = "console_asset_browser.rs"]
mod console_asset_browser;
#[path = "console_changelog.rs"]
mod console_changelog;
#[path = "console_connect.rs"]
mod console_connect;
#[path = "console_credits.rs"]
mod console_credits;
#[path = "console_cvars.rs"]
mod console_cvars;
pub(crate) use console_cvars::DEFAULT_MAX_PACKETS;
#[path = "console_collection_page.rs"]
mod collection_page;
#[path = "collection_panel.rs"]
pub(crate) mod collection_panel;
#[path = "config_import_panel.rs"]
pub(crate) mod config_import_panel;
#[path = "console_debug_panel.rs"]
mod console_debug_panel;
#[path = "console_identity_page.rs"]
mod console_identity_page;
#[path = "console_import.rs"]
mod console_import;
#[path = "console_model_viewer.rs"]
mod console_model_viewer;
#[path = "console_sjk_pages.rs"]
mod console_sjk_pages;
#[path = "console_update.rs"]
mod console_update;
#[path = "credits.rs"]
pub(crate) mod credits;
#[path = "debug_panel.rs"]
pub(crate) mod debug_panel;
#[path = "demo_director.rs"]
pub(crate) mod director;
#[path = "console_holocrons_page.rs"]
mod holocrons_page;
#[path = "holocrons_panel.rs"]
pub(crate) mod holocrons_panel;
#[path = "identity_panel.rs"]
pub(crate) mod identity_panel;
#[path = "model_viewer.rs"]
pub(crate) mod model_viewer;
#[path = "model_viewer_scan.rs"]
pub(crate) mod model_viewer_scan;
#[path = "console_profile_page.rs"]
mod profile_page;
#[path = "console_sjk_chat_page.rs"]
mod sjk_chat_page;
#[path = "sjk_chat_panel.rs"]
pub(crate) mod sjk_chat_panel;
#[path = "console_staff_page.rs"]
mod staff_page;
#[path = "staff_panel.rs"]
pub(crate) mod staff_panel;
pub(crate) use collection_page::{
    ACHIEVEMENTS_COMMAND, ACHIEVEMENTS_HELP, COLLECTION_COMMAND, COLLECTION_HELP,
    UNLOCKABLES_COMMAND, UNLOCKABLES_HELP,
};
pub(crate) use holocrons_page::{HOLOCRONS_COMMAND, HOLOCRONS_HELP};
pub(crate) use sjk_chat_page::{
    SJK_CHAT_COMMAND, SJK_CHAT_HELP, SJK_GIF_COMMAND, SJK_GIF_HELP, SJK_SAY_COMMAND, SJK_SAY_HELP,
};
pub(crate) use staff_page::{STAFF_COMMAND, STAFF_HELP};
#[path = "profile_panel.rs"]
pub(crate) mod profile_panel;
pub(crate) use profile_page::{PROFILE_COMMAND, PROFILE_HELP};
#[path = "console_qcommon.rs"]
mod qcommon;
#[path = "update_panel.rs"]
pub(crate) mod update_panel;
#[path = "console_window_options.rs"]
mod window_options;
pub(crate) use qcommon::wait_control;
#[path = "console_ui_options.rs"]
mod ui_options;
pub(crate) use console_connect::Action as ConnectionAction;
#[path = "console_demo.rs"]
mod console_demo;
pub(crate) use console_demo::Action as DemoAction;
#[path = "console_client.rs"]
mod console_client;
#[path = "console_command.rs"]
mod console_command;
#[cfg(test)]
#[path = "console_config_save_tests.rs"]
mod console_config_save_tests;
#[path = "console_forward.rs"]
mod console_forward;
#[path = "console_keyboard.rs"]
mod console_keyboard;
#[path = "console_edit_view.rs"]
pub(crate) mod edit_view;
#[path = "console_editing.rs"]
mod editing;
#[path = "console_line_edit.rs"]
pub(crate) mod line_edit;
#[path = "console_selection.rs"]
mod selection;
pub(crate) use selection::{floor_boundary, push_uncoloured};
#[path = "console_socket.rs"]
mod socket;

#[path = "console_classic.rs"]
pub(crate) mod classic;
#[path = "console_options.rs"]
pub(crate) mod console_options;
#[path = "console_pointer.rs"]
mod console_pointer;
#[path = "console_profile.rs"]
mod console_profile;
#[path = "console_scripting.rs"]
mod console_scripting;
#[path = "console_session.rs"]
mod console_session;
#[path = "console_tell.rs"]
mod console_tell;
#[path = "console_view.rs"]
mod console_view;
use console_cvars::register_daily_cvars;
use console_forward::{ForwardAction, forward_payload};
use console_view::ConsolePresentation;

use sjk_client::{ClientSession, ForceRankUpdate, compat_console_commands};
use sjk_shell::{CompletionKey, CvarDefinition, CvarFlags, CvarRegistry, CvarValue, Shell};
use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

const INPUT_LIMIT: usize = 512;

/// Toggleable console state owned by the viewer frontend.
pub(crate) struct ViewerConsole {
    shell: Shell,
    /// The profile had no config when the console started: a player's first start.
    new_profile: bool,
    open: bool,
    /// Shift held, for the Shift+Escape console toggle.
    shift: bool,
    /// Bind names that held keys were pressed under.
    held_keys: crate::input::keys::HeldKeys,
    /// Held keys whose bind repeats push or pull (`cl_forceRepeat`).
    force_repeat: crate::input::force_repeat::ForceRepeat,
    input: String,
    history: Vec<String>,
    history_index: Option<usize>,
    scroll_offset: usize,
    server_status: Arc<RwLock<String>>,
    /// The console socket external apps connect to (`cl_consoleSocket`).
    socket: socket::State,
    presentation: ConsolePresentation,
    /// Command and cvar browser drawn in place of the console while open.
    browser: browser::Browser,
    asset_browser: asset_browser::Browser,
    /// Personal test list of Sol's build, drawn in place of the console while open.
    debug_panel: debug_panel::Panel,
    /// The `modelviewer` page, drawn in place of the console while open.
    model_viewer: model_viewer::Panel,
    changelog: changelog::Panel,
    credits: credits::Panel,
    /// The Update page, drawn in place of the console while open.
    update_panel: update_panel::Panel,
    /// The Identity page, drawn in place of the console while open.
    identity_panel: identity_panel::Panel,
    profile_panel: profile_panel::Panel,
    staff_panel: staff_panel::Panel,
    collection_panel: collection_panel::Panel,
    /// The Holocrons page, the Profile screen's Holocrons tab in the SJK UI.
    holocrons_panel: holocrons_panel::Panel,
    /// Where the Profile screen returns while one of its tabs shows here
    /// ([`crate::profile_hub`]).
    profile_hub_return: crate::player_menu::ReturnTarget,
    /// The Collection's Holocrons (the Holocrons page) was shown last: the game menu's
    /// Collection opens on it again.
    holocrons_last: bool,
    /// The Illuminate holocron is lit: the viewer's state, mirrored each frame for the
    /// Collection's Toys tab (`console_collection_page.rs`).
    illuminate_lit: bool,
    /// The Toys tab asked to light the holocron (`true`) or put it out, not yet taken.
    pending_illuminate: Option<bool>,
    /// A chip naming a setting was clicked (`console_changelog.rs`): the cvar to search
    /// Settings for, not yet taken.
    pending_settings_search: Option<String>,
    sjk_chat_panel: sjk_chat_panel::Panel,
    /// The Import page (a dropped `.cfg`), drawn in place of the console while open.
    config_import: config_import_panel::Panel,
    userinfo_dirty: Arc<AtomicBool>,
    /// `forcepowers` changed (console, config or menu) and the Force-profile
    /// exchange has not yet been told: see `note_forcepowers_change`.
    forcepowers_changed: Arc<AtomicBool>,
    show_timedelta: crate::net_timing::CvarSetting,
    time_nudge: crate::presentation_clock::CvarSetting,
    smooth_clients: console_cvars::IntegerSetting,
    draw_fog: console_cvars::IntegerSetting,
    remaps: console_cvars::IntegerSetting,
    /// Changes of `cg_remapsBlockedMaps`, for [`crate::remap_blocked_maps`].
    remap_blocked_maps: console_cvars::RevisionSetting,
    packet_dup: console_cvars::IntegerSetting,
    max_packets: console_cvars::IntegerSetting,
    /// `gameplay_fps`, and the server's `pmove_fixed`/`pmove_msec` that override it.
    gameplay_fps: console_cvars::IntegerSetting,
    /// The FPS cap Auto follows, as the frame pacer last read it
    /// ([`Self::set_frame_cap`]).
    frame_cap: u32,
    /// The server's known command cap (JoF's 8 ms), from its serverinfo
    /// ([`crate::server_caps`]).
    server_cap: Option<&'static crate::server_caps::ServerCap>,
    /// `gameplay_fps_nocap`: 1 lifts the known caps.
    caps_lifted: console_cvars::IntegerSetting,
    /// Whether the server takes the inputs at that step ([`crate::input_health`]).
    input_health: crate::input_health::InputHealth,
    pmove_fixed: console_cvars::IntegerSetting,
    pmove_msec: console_cvars::IntegerSetting,
    pub(crate) geometry_controls: crate::shared_geometry::environment::Cvars,
    pub(crate) post_color: crate::frame_target::aa::color::Settings,
    /// Seeded scene-light control shared with graphics contexts and map-install workers.
    pub(crate) dynamic_light_settings: crate::dynamic_lights::Settings,
    /// Live soft-particle policy survives background world installation.
    pub(crate) soft_particles: crate::particle_draw::settings::Settings,
    /// Optional dust-mote intensity shared with graphics contexts.
    pub(crate) dust_motes: crate::dust_motes::Settings,
    /// Weather on/off and density, shared with graphics contexts.
    pub(crate) weather: crate::weather::Settings,
    /// Eye adaptation and base exposure shared with graphics contexts.
    pub(crate) exposure: crate::frame_target::aa::exposure::Settings,
    /// The quick wheel's pages (`wheel.json` beside the configuration), read by
    /// the wheel when it opens and changed by Settings.
    pub(crate) wheel_pages: crate::quick_wheel::pages::WheelPages,
    /// Optional ambient correction for supported main-view world surfaces.
    pub(crate) ssao: crate::world_materials::ssao::settings::Settings,
    pub(crate) director: director::Director,
    pending_connection: Option<console_connect::Action>,
    pending_demo: Option<console_demo::Action>,
    pending_screenshot: Option<crate::screenshot::Request>,
    /// `viewpos` was typed; the application answers with its camera pose.
    pending_viewpos: bool,
    /// `mark <note>` was typed: the pose is also kept, with the note, in [`Self::marks`].
    pending_mark: Option<String>,
    /// Where marks are appended: `marks.txt` beside the configuration.
    marks: std::path::PathBuf,
    /// The last line `viewpos` or `mark` answered, for Ctrl+C on an empty prompt.
    copied: String,
    pending_quit: bool,
    pending_input: Vec<String>,
    pending_chat: std::collections::VecDeque<String>,

    client_commands: console_client::Commands,
    /// The client mods and their switches (`docs/mods.md`).
    pub(crate) mods: crate::mods::Mods,
    /// The profile of the server the client is on, for its commands' completion.
    server_profile: Option<sjk_client::CompatProfile>,
    script_vfs: Option<Arc<sjk_vfs::VirtualFileSystem>>,
    config_directory: PathBuf,
    force_profile: sjk_client::ForceProfileNegotiator,
    /// Sends the `saber` command when the saber changes on a JA+ or jaPRO-lineage server.
    saber_change: sjk_client::SaberChangeNotifier,
    movement_policy_log: Option<[i32; 4]>,
    window_options: window_options::Options,
    chat_log: chat_log::ChatLog,
    qcommon: qcommon::Settings,
    /// Ctrl held, for word motion and clipboard shortcuts.
    control: bool,
    /// Caret and selection of `input`.
    edit: line_edit::LineEdit,
    /// Dead key shown at the input caret until its composition arrives.
    dead_key: crate::input::dead_key::DeadKey,
    /// Scrollback selection and the pointer gesture editing it or the caret.
    selection: selection::Selection,
    /// Rows and clocks of the classic console.
    classic: classic::State,
    /// Explicit height override chosen with Ctrl (full) or Shift (a quarter).
    /// `None` follows live `con_height` in both console styles.
    open_height: Option<f32>,
    /// Insert toggled overstrike: typing replaces the character after the caret
    /// (classic console).
    overstrike: bool,
}

impl ViewerConsole {
    /// Cached `cg_smoothClients` for live and demo snapshot translation.
    pub(crate) fn smooth_clients(&self) -> bool {
        self.smooth_clients.enabled()
    }

    /// Cached `cl_showtimedelta` state for the allocation-free runtime read.
    pub(crate) fn show_timedelta(&self) -> bool {
        self.show_timedelta.enabled()
    }

    /// Cached, clamped `cl_timenudge` for the live presentation clock.
    pub(crate) fn time_nudge_millis(&self) -> i64 {
        self.time_nudge.millis()
    }

    /// Cached `cl_packetdup`, clamped like the stock client (0 to 5).
    pub(crate) fn packet_dup(&self) -> usize {
        self.packet_dup
            .value()
            .clamp(0, sjk_client::command_history::MAX_PACKET_DUP as i64) as usize
    }

    /// Cached `cl_maxpackets`; [`sjk_client::command_rate::PacketPacer`] clamps it.
    pub(crate) fn max_packets(&self) -> i64 {
        self.max_packets.value()
    }

    /// Cached `gameplay_fps`: the frame rate whose movement the player wants, or
    /// 0 (Auto) to follow the FPS cap.
    pub(crate) fn gameplay_fps(&self) -> i64 {
        self.gameplay_fps.value()
    }

    /// The server's `pmove_msec` while it fixes the movement step (`pmove_fixed`).
    pub(crate) fn pmove_fixed_msec(&self) -> Option<i32> {
        self.pmove_fixed
            .enabled()
            .then(|| self.pmove_msec.value().clamp(1, 50) as i32)
    }

    /// The FPS cap in use (`com_maxfps`, AUTO resolved; 0 is uncapped), which the
    /// gameplay FPS's Auto follows. Set every frame of a game.
    pub(crate) fn set_frame_cap(&mut self, cap: u32) {
        self.frame_cap = cap;
    }

    pub(crate) fn frame_cap(&self) -> u32 {
        self.frame_cap
    }

    /// Milliseconds between user commands ([`sjk_client::command_rate::command_step`]).
    pub(crate) fn command_step(&self) -> i32 {
        sjk_client::command_rate::command_step(
            self.gameplay_fps(),
            self.pmove_fixed_msec(),
            self.frame_cap,
            self.server_step_floor(),
        )
    }

    /// The server's known command cap ([`crate::server_caps`]), lifted or not.
    pub(crate) fn server_cap(&self) -> Option<&'static crate::server_caps::ServerCap> {
        self.server_cap
    }

    /// Whether `gameplay_fps_nocap` lifts the known caps.
    pub(crate) fn server_caps_lifted(&self) -> bool {
        self.caps_lifted.enabled()
    }

    /// The shortest step this server takes, when it is known to cap commands and
    /// the player has not lifted the caps.
    pub(crate) fn server_step_floor(&self) -> Option<i32> {
        self.server_cap
            .filter(|_| !self.server_caps_lifted())
            .map(|cap| cap.step)
    }

    /// One predicted snapshot's verdict at the step in use: true once when the
    /// server has not been taking the inputs (open the gameplay FPS card).
    pub(crate) fn observe_input_health(
        &mut self,
        sample: &crate::local_prediction::PredictionSample,
        now: std::time::Instant,
    ) -> bool {
        let step = self.command_step();
        self.input_health.observe(sample, step, now)
    }

    pub(crate) fn config_directory(&self) -> &std::path::Path {
        &self.config_directory
    }

    /// The master servers the browser asks, apart by spaces: `cl_master` then
    /// `cl_master2` (either may be empty).
    pub(crate) fn master_server(&self) -> Result<String, Box<dyn Error>> {
        let first = self.text_cvar("cl_master")?;
        let second = self.text_value("cl_master2").unwrap_or("");
        Ok(format!("{first} {second}").trim().to_owned())
    }

    pub(crate) fn text_value(&self, name: &str) -> Option<&str> {
        match self.cvar(name) {
            Some(CvarValue::Text(value)) => Some(value),
            _ => None,
        }
    }

    /// Whether the overlay currently owns keyboard input.
    /// Track Shift for the stock Shift+Escape toggle (`cl_keys.cpp:1318`).
    pub(crate) fn set_shift(&mut self, held: bool) {
        self.shift = held;
    }

    /// Whether Shift is held (`ModifiersChanged`).
    pub(crate) fn shift_held(&self) -> bool {
        self.shift
    }

    /// Whether Ctrl is held (`ModifiersChanged`).
    pub(crate) fn control_held(&self) -> bool {
        self.control
    }

    /// Whether Shift or Ctrl is held (`ModifiersChanged`), for keys that move
    /// an item rather than the focus.
    pub(crate) fn modifier_held(&self) -> bool {
        self.shift || self.control
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// The command and cvar browser is open and covers the whole frame; overlays
    /// under it should not build their text.
    pub(crate) fn covers_frame(&self) -> bool {
        self.open
            && (self.browser.is_open()
                || self.asset_browser.open
                || self.debug_panel.is_open()
                || self.model_viewer.is_open()
                || self.changelog.is_open()
                || self.credits.is_open()
                || self.update_panel.is_open()
                || self.identity_panel.is_open()
                || self.profile_panel.is_open()
                || self.staff_panel.is_open()
                || self.collection_panel.is_open()
                || self.holocrons_panel.is_open()
                || self.sjk_chat_panel.is_open()
                || self.config_import.is_open())
    }

    /// Add an application diagnostic to the visible bounded scrollback.
    pub(crate) fn push_log(&mut self, text: impl Into<String>) {
        self.shell.push_log(text);
    }

    /// Queue the script bound to a key event. A release runs under the name
    /// its press had, so `+button` binds release even if the layout changed.
    pub(crate) fn queue_bound_key(
        &mut self,
        code: KeyCode,
        name: Option<crate::input::keys::KeyName>,
        pressed: bool,
    ) {
        let name = if pressed {
            if let Some(name) = name {
                self.held_keys.press(code, name);
            }
            name
        } else {
            self.held_keys.release(code).or(name)
        };
        if let Some(name) = name {
            self.queue_bound_script(name.as_str(), pressed);
        }
    }

    /// A key went up, wherever the release was routed: a held push or pull key stops
    /// repeating even when a menu or the chat took the release.
    pub(crate) fn force_repeat_key_up(
        &mut self,
        code: KeyCode,
        name: Option<crate::input::keys::KeyName>,
    ) {
        if let Some(name) = self.held_keys.name(code).or(name) {
            self.force_repeat.release(name.as_str());
        }
    }

    /// A mouse button bound under `key` went up, wherever the release was routed.
    pub(crate) fn force_repeat_button_up(&mut self, key: &str) {
        self.force_repeat.release(key);
    }

    /// Every key was let go of: the window lost the keyboard.
    pub(crate) fn clear_force_repeat(&mut self) {
        self.force_repeat.clear();
    }

    /// The `generic_cmd` a held push or pull key repeats in the next user command.
    pub(crate) fn force_repeat_command(&self) -> Option<u8> {
        self.force_repeat.command()
    }

    /// Add text to the scrollback without showing it among the notify lines.
    pub(crate) fn push_log_quiet(&mut self, text: impl Into<String>) {
        self.shell.push_log_quiet(text);
    }

    /// Queue `text` to run as if typed, on the next command frame.
    pub(crate) fn queue_command(&mut self, text: &str) -> Result<(), String> {
        self.shell
            .queue_script(text)
            .map_err(|error| error.to_string())
    }

    /// Queue a bound script through the same frame-buffered path as cfg text.
    pub(crate) fn queue_bound_script(&mut self, key: &str, pressed: bool) {
        let time = self.input_millis();
        let id = key
            .bytes()
            .fold(0_u64, |hash, byte| hash.wrapping_mul(31) + u64::from(byte));
        if !pressed {
            self.force_repeat.release(key);
        }
        let repeat = pressed && self.bool_cvar(crate::input::force_repeat::CVAR) != Some(false);
        match self.shell.binds.commands_for_event(key, pressed) {
            Ok(commands) => {
                for command in commands {
                    if repeat && let Some(value) = crate::input::force_repeat::repeated(&command) {
                        self.force_repeat.press(key, value);
                    }
                    let command = if command.starts_with(['+', '-']) {
                        format!("{command} {id} {time}")
                    } else {
                        command
                    };
                    if let Err(error) = self.shell.queue_script(&command) {
                        self.shell.push_log(format!("^1Bind error: {error}"));
                    }
                }
            }
            Err(error) => self.shell.push_log(format!("^1Bind error: {error}")),
        }
    }

    pub(crate) fn cvar(&self, name: &str) -> Option<&CvarValue> {
        self.shell.cvars.get(name).map(|cvar| &cvar.value)
    }

    /// The value a cvar starts with in a fresh profile.
    pub(crate) fn cvar_default(&self, name: &str) -> Option<&CvarValue> {
        self.shell.cvars.get(name).map(|cvar| &cvar.default)
    }

    pub(crate) fn bool_cvar(&self, name: &str) -> Option<bool> {
        match self.cvar(name) {
            Some(CvarValue::Bool(value)) => Some(*value),
            _ => None,
        }
    }

    pub(crate) fn integer_cvar(&self, name: &str) -> Option<i64> {
        match self.cvar(name) {
            Some(CvarValue::Integer(value)) => Some(*value),
            _ => None,
        }
    }

    /// `cg_fkDuration`, `cg_fkFirstJumpDuration` and `cg_fkSecondJumpDelay`.
    pub(crate) fn flip_kick_timing(&self) -> crate::input::flip_kick::Timing {
        use crate::input::flip_kick::*;
        let count = |name, default| {
            self.integer_cvar(name)
                .map_or(default, |value| value.clamp(0, i64::from(u32::MAX)) as u32)
        };
        // The cvars are EJK's frames; the run counts user commands.
        let step = self.command_step();
        Timing {
            duration: commands(count(DURATION_CVAR, 50), step),
            first_jump: commands(count(FIRST_JUMP_CVAR, 0), step),
            second_jump_delay: commands(count(SECOND_JUMP_CVAR, 0), step),
        }
    }

    pub(crate) fn float_cvar(&self, name: &str) -> Option<f64> {
        match self.cvar(name) {
            Some(CvarValue::Float(value)) => Some(*value),
            _ => None,
        }
    }

    /// Set a cvar from text, handing a refusal to the caller instead of the log.
    pub(crate) fn try_set_cvar(&mut self, name: &str, value: &str) -> Result<(), String> {
        self.apply_cvar(name, value)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn set_cvar(&mut self, name: &str, value: &str) -> bool {
        match self.apply_cvar(name, value) {
            Ok(()) => true,
            Err(error) => {
                self.shell.push_log(format!("^1{error}"));
                false
            }
        }
    }

    /// Set several cvars from text and save the profile once if any changed;
    /// a value a cvar refuses is logged and the others are still set.
    pub(crate) fn set_cvars<'a>(&mut self, values: impl IntoIterator<Item = (&'a str, &'a str)>) {
        let mut changed = false;
        for (name, value) in values {
            match self.shell.cvars.set_text(name, value) {
                Ok(set) => changed |= set,
                Err(error) => self.shell.push_log(format!("^1{error}")),
            }
        }
        if changed {
            self.persist();
        }
    }

    /// Set a cvar from text and persist a change; the error is returned, not logged.
    fn apply_cvar(&mut self, name: &str, value: &str) -> Result<(), sjk_shell::CvarError> {
        if self.shell.cvars.set_text(name, value)? {
            if name.eq_ignore_ascii_case("r_resolution") {
                self.sync_custom_resolution(value);
            }
            self.persist();
        }
        Ok(())
    }

    /// Open the console with the command and cvar browser in front of it.
    pub(crate) fn open_browser(&mut self) {
        if !self.open {
            self.set_open(true);
        }
        self.browser.open(&self.shell);
    }

    /// Carry out what the browser asked for after a key or pointer event.
    fn browser_action(&mut self, action: browser::BrowserAction) {
        use browser::BrowserAction;
        match action {
            BrowserAction::None => {}
            BrowserAction::Close => self.browser.close(),
            BrowserAction::Insert(name) => {
                self.browser.close();
                self.input = format!("{name} ");
                self.rebuild_prompt();
            }
            BrowserAction::Set { name, value } => {
                let result = self.apply_cvar(&name, &value);
                self.browser.refresh(&self.shell);
                match result {
                    Ok(()) => self
                        .browser
                        .set_status(format!("{name} = \"{value}\""), false),
                    Err(error) => self.browser.set_status(error.to_string(), true),
                }
            }
            BrowserAction::Reset(name) => {
                let result = self.shell.cvars.reset(&name);
                if let Ok(true) = result {
                    if name.eq_ignore_ascii_case("r_resolution")
                        && let Some(value) = self.shell.cvars.get(&name).map(|c| c.value.as_text())
                    {
                        self.sync_custom_resolution(&value);
                    }
                    self.persist();
                }
                self.browser.refresh(&self.shell);
                match result {
                    Ok(_) => self
                        .browser
                        .set_status(format!("{name} restored to its default"), false),
                    Err(error) => self.browser.set_status(error.to_string(), true),
                }
            }
        }
    }

    pub(crate) fn keys_for_command(&self, command: &str) -> Vec<String> {
        self.shell
            .binds
            .iter()
            .filter(|(_, bound)| bound.eq_ignore_ascii_case(command))
            .map(|(key, _)| key.to_owned())
            .collect()
    }

    /// Whether a stock key has an explicit script, before UI fallback handling.
    pub(crate) fn has_key_binding(&self, key: &str) -> bool {
        self.shell
            .binds
            .iter()
            .any(|(bound, _)| bound.eq_ignore_ascii_case(key))
    }

    /// Format all physical keys bound to `command` into retained storage, as
    /// players see key names ([`sjk_shell::key_names::display_key`]).
    pub(crate) fn write_keys_for_command(&self, command: &str, output: &mut String) {
        output.clear();
        for (key, bound) in self.shell.binds.iter() {
            if !bound.eq_ignore_ascii_case(command) {
                continue;
            }
            if !output.is_empty() {
                output.push_str(" / ");
            }
            sjk_shell::key_names::push_display_key(output, key);
        }
        if output.is_empty() {
            output.push_str("unbound");
        }
    }

    /// Replace one displayed binding without removing the other slot. Locked
    /// keys ([`keybind_editor::is_locked_key`]) are never bound or replaced
    /// here; the `bind` command is unrestricted.
    pub(crate) fn rebind_action(&mut self, command: &str, slot: usize, key: &str) {
        let Some(key) = sjk_shell::key_names::canonical_key(key) else {
            self.shell.push_log(format!("^1Unsupported key: {key}"));
            return;
        };
        if slot > 1 || keybind_editor::is_locked_key(key) {
            return;
        }
        let keys = self.keys_for_command(command);
        if keys.iter().any(|old| old.eq_ignore_ascii_case(key)) {
            return;
        }
        if keys
            .get(slot)
            .is_some_and(|old| keybind_editor::is_locked_key(old))
        {
            return;
        }
        if let Some(old) = keys.get(slot) {
            self.shell.binds.unbind(old);
        }
        if let Err(error) = self.shell.binds.bind(key, command) {
            self.shell.push_log(format!("^1Bind error: {error}"));
        }
        self.persist();
    }

    /// Clear only the selected displayed binding, unless it is a locked key
    /// ([`keybind_editor::is_locked_key`]).
    pub(crate) fn clear_action(&mut self, command: &str, slot: usize) {
        if let Some(key) = self.keys_for_command(command).get(slot)
            && !keybind_editor::is_locked_key(key)
        {
            self.shell.binds.unbind(key);
        }
        self.persist();
    }

    pub(crate) fn reset_default_binds(&mut self) {
        self.shell.binds = keybind_editor::default_bindings();
        self.persist();
    }

    /// Append console text vertices without allocating additional line strings.
    pub(crate) fn append_overlay(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        _scale: f32,
    ) {
        // Overlay text draws above every overlay's shapes, so the browser replaces the
        // console's drawing rather than covering it.
        if self.append_credits(vertices, font, viewport)
            || self.append_changelog(vertices, font, viewport)
            || self.append_identity_panel(vertices, font, viewport)
        {
            return;
        }
        if self.open && self.browser.is_open() {
            self.browser.append(vertices, font, viewport);
            return;
        }
        // The console itself is drawn on its own layer by `append_classic`.
        self.presentation.clear(viewport);
    }

    /// The command browser follows the console style: the classic console's is
    /// drawn classic+, with the retail menu `art` it can use, the SJK UI's
    /// console's in its look.
    pub(crate) fn set_browser_art(&mut self, art: crate::menu::art::ArtSet) {
        let look = match self.console_style() {
            console_options::ConsoleStyle::Classic => browser::Look::Classic,
            console_options::ConsoleStyle::Sjk => browser::Look::Sjk,
        };
        self.browser.set_look(look, art);
    }

    /// The retail menu `art` the changelog and Identity pages' classic+ looks
    /// can use (the SJK UI's looks are chosen with [`Self::set_sjk_pages`]).
    pub(crate) fn set_page_art(&mut self, art: crate::menu::art::ArtSet) {
        self.changelog.set_art(art);
        self.identity_panel.set_art(art);
    }

    /// Fill the console for a world shot: open it on made-up scrollback (a
    /// start, a server's status, chat, an error) with `typed` in the input.
    #[cfg(test)]
    pub(crate) fn console_for_shot(&mut self, typed: &str) {
        self.shell.clear_lines();
        for line in [
            "^5SJK console ready. ^7Type cmdlist for commands.",
            "Loading maps/mp/duel6.bsp",
            "map: mp/duel6, 2 surfaces sorted, 12 lights",
            "]connect 135.125.145.49:29070",
            "Connecting to 135.125.145.49:29070...",
            "^7Joined ^5J^7o^5F ^7| ^3JA+ Duel ^7| ^2EU",
            "]status",
            "map: mp/ffa3",
            "num score ping name            lastmsg address               qport rate",
            "--- ----- ---- --------------- ------- --------------------- ----- -----",
            "  0    42   12 ^1S^7ol             0 loopback              21345 90000",
            "  1    37   48 ^5Kyle^7            0 82.64.11.201:29071     4410 25000",
            "  2    29   61 ^3Bishop^7          0 51.75.0.3:29070       11023 25000",
            "  3    12   35 ^2Creyon^7          0 90.112.43.7:29070      7012 25000",
            "^5Kyle^7: gg, nice duel",
            "^1S^7ol^7: ^3one more? ^7best of three",
            "]cg_fov 110",
            "]con_style",
            "\"con_style\" is:\"auto^7\" default:\"auto^7\"",
            "Console style: auto (the SJK UI's with its menus, else classic), sjk or classic (after EternalJK)",
            "^1Unknown command \"saberthrow\"",
            "Kyle was cut in half by Sol's saber",
            "^3Bishop^7 entered the game",
        ] {
            self.shell.push_log(line);
        }
        self.set_open(true);
        self.type_text(typed);
    }

    /// Drag over the command browser's detail text, for a world shot.
    #[cfg(test)]
    pub(crate) fn browser_drag_for_shot(&mut self, from: sjk_ui::Vec2, to: sjk_ui::Vec2) {
        self.browser.drag_for_shot(from, to);
    }

    /// Open the command browser on a search, for the menu snapshots.
    #[cfg(test)]
    pub(crate) fn open_browser_on(&mut self, filter: &str) {
        self.set_open(true);
        self.open_browser();
        self.browser.search_for_snapshot(filter);
    }

    /// Whether the open full-frame page (the browser or the changelog) is the
    /// classic+ one, drawn in the menus' font.
    pub(crate) fn classic_browser_open(&self) -> bool {
        if self.credits.is_open() {
            return self.open && self.credits.is_classic();
        }
        if self.changelog.is_open() {
            return self.open && self.changelog.is_classic();
        }
        if self.identity_panel.is_open() {
            return self.open && self.identity_panel.is_classic();
        }
        self.open && self.browser.is_open() && self.browser.is_classic()
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        if self.open && self.asset_browser.open {
            return self.asset_browser.draw_list();
        }
        if let Some(draw_list) = self.model_viewer_draw_list() {
            return draw_list;
        }
        if let Some(draw_list) = self.config_import_draw_list() {
            return draw_list;
        }
        if let Some(draw_list) = self.credits_draw_list() {
            return draw_list;
        }
        if let Some(draw_list) = self.changelog_draw_list() {
            return draw_list;
        }
        if let Some(draw_list) = self.update_panel_draw_list() {
            return draw_list;
        }
        if let Some(draw_list) = self.identity_panel_draw_list() {
            return draw_list;
        }
        if let Some(draw_list) = self.profile_panel_draw_list() {
            return draw_list;
        }
        if let Some(draw_list) = self.staff_panel_draw_list() {
            return draw_list;
        }
        if let Some(draw_list) = self.collection_panel_draw_list() {
            return draw_list;
        }
        if let Some(draw_list) = self.holocrons_panel_draw_list() {
            return draw_list;
        }
        if let Some(draw_list) = self.sjk_chat_panel_draw_list() {
            return draw_list;
        }
        if let Some(draw_list) = self.debug_panel_draw_list() {
            return draw_list;
        }
        if self.open && self.browser.is_open() {
            return self.browser.draw_list();
        }
        self.presentation.draw_list()
    }

    fn execute_bound_command(&mut self, command: &str) {
        if command.eq_ignore_ascii_case("toggleconsole") {
            self.set_open(!self.open);
        } else {
            let _ = self.shell.execute_line(command);
            self.persist();
        }
    }

    fn submit(&mut self, session: Option<&mut ClientSession>) {
        // `Console_Key`: one leading `/` or `\` is optional and stripped before the
        // line is completed and run.
        if let Some(line) = self.input.trim_start().strip_prefix(['/', '\\']) {
            self.input = line.to_owned();
        }
        if self.integer_cvar("cl_allowentercompletion").unwrap_or(1) != 0 {
            self.complete_command(CompletionKey::Enter);
        }
        let command = self.input.trim().to_owned();
        self.input.clear();
        self.history_index = None;
        self.rebuild_prompt();
        if command.is_empty() {
            return;
        }
        if self.history.last() != Some(&command) {
            self.history.push(command.clone());
            if self.history.len() > classic::HISTORY {
                self.history.remove(0);
            }
        }
        self.execute_console_line(&command, session);
    }

    /// Change the console catcher and its existing animated presentation state.
    pub(crate) fn set_open(&mut self, open: bool) {
        self.selection.clear();
        self.dead_key.settle();
        if self.bool_cvar("con_autoclear").unwrap_or(true) {
            self.input.clear();
            self.rebuild_prompt();
        }
        self.open = open;
        self.history_index = None;
        if !open {
            self.asset_browser.open = false;
            self.browser.close();
            self.debug_panel.close();
            self.model_viewer.close();
            self.changelog.close();
            self.credits.close();
            self.update_panel.close();
            self.identity_panel.close();
            self.profile_panel.close();
            self.staff_panel.close();
            self.collection_panel.close();
            self.holocrons_panel.close();
            self.sjk_chat_panel.close();
            self.config_import.close();
        }
    }

    fn navigate_history(&mut self, direction: i32) {
        if self.history.is_empty() {
            return;
        }
        let current = self.history_index.unwrap_or(self.history.len());
        let next = if direction < 0 {
            current.saturating_sub(1)
        } else {
            (current + 1).min(self.history.len())
        };
        self.history_index = (next < self.history.len()).then_some(next);
        self.input.clear();
        if let Some(index) = self.history_index {
            self.input.push_str(&self.history[index]);
        }
        self.rebuild_prompt();
    }

    fn complete_command(&mut self, key: CompletionKey) {
        if let Some(line) = self.shell.complete_line(&self.input, key) {
            self.input = line;
            self.rebuild_prompt();
        }
    }

    /// The input line was replaced as a whole: put the caret at its end. A dead key
    /// shown on the old line is no longer pending.
    fn rebuild_prompt(&mut self) {
        self.dead_key.settle();
        self.edit.to_end(&self.input);
    }

    /// Let the config file catch up with a change. Cheap: nothing is written here
    /// unless an archived cvar or a bind changed and then held still for
    /// [`sjk_shell::CONFIG_SAVE_DELAY`], and then on a background thread
    /// ([`sjk_shell::Shell::autosave`]). Also run once per frame.
    pub(crate) fn persist(&mut self) {
        if let Err(error) = self.shell.autosave(std::time::Instant::now()) {
            self.shell
                .push_log(format!("^1Could not save config: {error}"));
        }
    }

    /// Write pending config changes now, on this thread, so a test can read
    /// config.cfg back (and a hosted renderer saves before the engine unloads it).
    /// The client's own read-backs (`exec config.cfg`, `writeconfig`, exit) go
    /// through the shell's synchronous save.
    pub(crate) fn flush_config(&mut self) {
        if let Err(error) = self.shell.save_if_dirty() {
            self.shell
                .push_log(format!("^1Could not save config: {error}"));
        }
    }

    /// Borrow a text cvar without allocating a display string.
    pub(crate) fn text_cvar(&self, name: &str) -> Result<&str, Box<dyn Error>> {
        match self.shell.cvars.get(name).map(|cvar| &cvar.value) {
            Some(CvarValue::Text(value)) => Ok(value),
            _ => Err(invalid_cvar(name, "must be text").into()),
        }
    }
}

impl Drop for ViewerConsole {
    /// Every exit path drops the console: write what the debounce still holds,
    /// after any background write, before the process ends.
    fn drop(&mut self) {
        let _ = self.shell.save();
    }
}

fn invalid_cvar(name: &str, requirement: &str) -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        format!("cvar {name:?} {requirement}"),
    )
}

impl ViewerConsole {
    pub(crate) fn remap_mode(&self) -> i64 {
        self.remaps.value()
    }

    /// How often `cg_remapsBlockedMaps` has changed, read without a name lookup.
    pub(crate) fn remap_blocked_maps_revision(&self) -> u64 {
        self.remap_blocked_maps.value()
    }
}
