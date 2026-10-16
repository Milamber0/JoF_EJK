//! Client command catalogue and bounded handoff to the existing viewer services.
use super::*;

impl ViewerConsole {
    /// Coalesce `scl` until the ordinary viewer service handoff.
    pub(crate) fn request_siege_class(&mut self) {
        self.client_commands.siege_class = true;
    }
}
use std::collections::VecDeque;
use std::sync::mpsc::Receiver;

#[path = "console_hud_commands.rs"]
mod hud_commands;
#[path = "console_identity.rs"]
mod identity;
#[path = "console_info.rs"]
mod info;
#[path = "console_queries.rs"]
mod queries;
#[path = "console_remaps.rs"]
mod remaps;
#[path = "console_restarts.rs"]
mod restarts;

pub(super) use identity::color_chat;

/// Completion/help inventory for commands owned by viewer services.
pub(super) const COMMANDS: &[(&str, &str)] = &[
    (
        "remapShader",
        "Replace a local shader: remapShader <old> <new>",
    ),
    ("listRemaps", "List server and local shader replacements"),
    (
        "clearRemaps",
        "Clear server and local shader replacements until new ones arrive",
    ),
    (
        "blockRemaps",
        "Ignore server shader remaps on a map, the loaded one by default: blockRemaps [map]",
    ),
    (
        "unblockRemaps",
        "Allow server shader remaps on a blocked map again: unblockRemaps [map]",
    ),
    ("speedometer", "Configure supported speedometer flags"),
    ("strafehelper", "Configure supported airborne CGAZ flags"),
    ("play", "Play local sound files"),
    ("music", "Start intro and repeating music"),
    ("stopmusic", "Stop background music"),
    ("soundstop", "Stop sounds and music"),
    ("soundlist", "List registered sound assets"),
    ("soundinfo", "Describe audio output"),
    (
        "s_dynamic",
        "Select dynamic music state (untimed transitions)",
    ),
    ("messagemode", "Compose global chat"),
    ("messagemode2", "Compose team chat"),
    (
        "messagemode3",
        "Compose crosshair-client chat when a target is available",
    ),
    (
        "messagemode4",
        "Compose last-attacker chat when tracking is available",
    ),
    (
        "messagemode5",
        "Compose SJK chat, which every SJK player shares through the SJK hub",
    ),
    (crate::am_color::COMMAND, crate::am_color::HELP),
    (
        "reply",
        "Whisper to the last player who whispered to you: reply <message>, or bare to open the chat on it",
    ),
    (crate::player_ids::COMMAND, crate::player_ids::HELP),
    ("toggleconsole", "Toggle the console"),
    (super::asset_browser::COMMAND, super::asset_browser::HELP),
    (
        "consolebrowser",
        "Search commands and cvars and edit cvar values (F3 in the console)",
    ),
    (super::debug_panel::COMMAND, super::debug_panel::HELP),
    (super::model_viewer::COMMAND, super::model_viewer::HELP),
    (
        crate::medal_popup::rehearsal::COMMAND,
        crate::medal_popup::rehearsal::HELP,
    ),
    (
        crate::holocrons::rehearsal::COMMAND,
        crate::holocrons::rehearsal::HELP,
    ),
    (
        crate::cards::rehearsal::COMMAND,
        crate::cards::rehearsal::HELP,
    ),
    (
        crate::unlock_toast::UPDATE_COMMAND,
        crate::unlock_toast::UPDATE_HELP,
    ),
    (
        crate::quick_wheel::OPEN_COMMAND,
        crate::quick_wheel::OPEN_HELP,
    ),
    (
        crate::quick_wheel::RUN_COMMAND,
        crate::quick_wheel::RUN_HELP,
    ),
    (
        crate::quick_wheel::GIF_OPEN_COMMAND,
        crate::quick_wheel::GIF_OPEN_HELP,
    ),
    (
        crate::quick_wheel::GIF_RUN_COMMAND,
        crate::quick_wheel::GIF_RUN_HELP,
    ),
    (crate::weather::COMMAND, crate::weather::HELP),
    (super::changelog::COMMAND, super::changelog::HELP),
    (super::credits::COMMAND, super::credits::HELP),
    (
        crate::console::PROFILE_COMMAND,
        crate::console::PROFILE_HELP,
    ),
    (
        crate::console::COLLECTION_COMMAND,
        crate::console::COLLECTION_HELP,
    ),
    (
        crate::console::ACHIEVEMENTS_COMMAND,
        crate::console::ACHIEVEMENTS_HELP,
    ),
    (crate::console::STAFF_COMMAND, crate::console::STAFF_HELP),
    (
        crate::console::UNLOCKABLES_COMMAND,
        crate::console::UNLOCKABLES_HELP,
    ),
    (
        crate::console::HOLOCRONS_COMMAND,
        crate::console::HOLOCRONS_HELP,
    ),
    (
        crate::console::SJK_CHAT_COMMAND,
        crate::console::SJK_CHAT_HELP,
    ),
    (
        crate::console::SJK_SAY_COMMAND,
        crate::console::SJK_SAY_HELP,
    ),
    (
        crate::console::SJK_GIF_COMMAND,
        crate::console::SJK_GIF_HELP,
    ),
    (crate::emotes::COMMAND, crate::emotes::HELP),
    (
        crate::saber_skin_command::COMMAND,
        crate::saber_skin_command::HELP,
    ),
    (super::update_panel::COMMAND, super::update_panel::HELP),
    (
        crate::identity_command::COMMAND,
        crate::identity_command::HELP,
    ),
    (crate::avatar_command::COMMAND, crate::avatar_command::HELP),
    (crate::hud::nameplate::COMMAND, crate::hud::nameplate::HELP),
    ("togglemenu", "Toggle the in-game menu"),
    (
        crate::menu::first_setup::COMMAND,
        crate::menu::first_setup::HELP,
    ),
    (
        crate::graphics_quality::COMMAND,
        crate::graphics_quality::HELP,
    ),
    (
        crate::graphics_quality::RENDERER_ROW,
        crate::graphics_quality::RENDERER_HELP,
    ),
    ("cmd", "Forward arguments as a reliable server command"),
    ("clientinfo", "Print client state and userinfo"),
    ("userinfo", "Print userinfo"),
    ("model", "Set player model and optional skin"),
    ("forcepowers", "Set the player force profile"),
    ("configstrings", "Print non-empty indexed configstrings"),
    ("listEmojis", "List the chat emojis (cg_chatBoxEmojis)"),
    ("mods", "List the client mods and whether each is on"),
    ("showip", "List local interface addresses"),
    ("fs_openedList", "Print mounted package names"),
    ("fs_referencedList", "Print pure-proof package references"),
    ("modelist", "List video modes"),
    ("minimize", "Minimize the window"),
    ("ping", "Query server info and round-trip time"),
    ("serverstatus", "Query server status"),
    ("globalservers", "Query master server addresses"),
    ("localservers", "Discover LAN servers"),
    ("addFavorite", "Add an address to browser favorites"),
    ("rcon", "Execute a remote console command"),
    ("afk", "Toggle the AFK name prefix"),
    ("freecam", "Toggle detached local flight: freecam [on|off]"),
    (
        "peek",
        "Watch a player briefly: peek [id|name] [seconds], peek off",
    ),
    (
        "fakenoclip",
        "Fly locally while the server sees you standing still (toggle)",
    ),
    (crate::ghost_replay::COMMAND, crate::ghost_replay::HELP),
    (crate::mimic::COMMAND, crate::mimic::HELP),
    ("colorname", "Apply selected name colors"),
    ("colorstring", "Configure outgoing chat colors"),
    ("vid_restart", "Reapply video settings (device is retained)"),
    (crate::fps_help::COMMAND, crate::fps_help::HELP),
    (crate::whats_new_card::COMMAND, crate::whats_new_card::HELP),
    (crate::gameplay_fps::COMMAND, crate::gameplay_fps::HELP),
    (
        "snd_restart",
        "Recreate audio output and reload sound assets",
    ),
    ("in_restart", "Reset held input and pointer capture"),
];

/// Queue and worker replies exist only for explicitly requested commands.
#[derive(Default)]
pub(super) struct Commands {
    /// Coalesced server request; never forwarded back to the server.
    siege_class: bool,
    last_demo: Option<String>,
    /// Bounded main-thread service requests.
    pub pending: VecDeque<Vec<String>>,
    /// Bounded worker reply channels.
    pub jobs: Vec<Receiver<Vec<String>>>,
    /// Consecutive frames beyond the connected-server timeout.
    pub timeout_frames: u8,
    /// Name-edit cooldown shared with the userinfo callback.
    pub name_clock: identity::NameClock,
}

/// Register command metadata and only the settings consumed by these services.
pub(super) fn register(shell: &mut Shell, commands: &Commands) -> Result<(), Box<dyn Error>> {
    for &(name, help) in COMMANDS.iter().chain(crate::cosmetics::command::COMMANDS) {
        if !shell.commands.contains(name) && shell.cvars.get(name).is_none() {
            shell.commands.register(name, help, |_| {
                Err(sjk_shell::CommandError::Handler(
                    "Viewer command dispatcher required".into(),
                ))
            })?;
        }
    }
    queries::register(&mut shell.cvars)?;
    restarts::register(&mut shell.cvars)?;
    identity::register(&mut shell.cvars, &commands.name_clock)?;
    for (name, value, flags, help) in [
        (
            "cg_chatBeep",
            true,
            CvarFlags::ARCHIVE,
            "Play global chat notifications",
        ),
        (
            "cg_teamChatBeep",
            true,
            CvarFlags::ARCHIVE,
            "Play team chat notifications",
        ),
        (
            "s_allowDynamicMusic",
            true,
            CvarFlags::ARCHIVE,
            "Enable dynamic music selection",
        ),
        (
            "s_initsound",
            true,
            CvarFlags::ARCHIVE,
            "Open audio output on startup/restart",
        ),
        (
            "s_show",
            false,
            CvarFlags::NONE,
            "Print started sounds (diagnostic)",
        ),
        (
            "cg_duelMusic",
            true,
            CvarFlags::ARCHIVE,
            "Play private duel music",
        ),
        (
            crate::ghost_replay::SHOWCASE_CVAR,
            false,
            CvarFlags::ARCHIVE,
            "Experimental: the ghost and mimic showcase commands",
        ),
    ] {
        shell
            .cvars
            .register(CvarDefinition::new(name, value, flags, help))?;
    }
    shell.cvars.register(CvarDefinition::new(
        "s_separation",
        0.5,
        CvarFlags::ARCHIVE,
        "Stereo separation",
    ))?;
    Ok(())
}

/// The file of `firstsetup import <path>` as typed: the tokenizer reads a backslash
/// inside quotes as an escape, which would eat a quoted Windows path's separators
/// (`"C:\Users\..."`, as Explorer's Copy as path gives it). One pair of surrounding
/// quotes is dropped and the rest kept as it is.
fn raw_import_path(command: &str, tokens: &[String]) -> Option<String> {
    if !(tokens.len() > 2
        && tokens[0].eq_ignore_ascii_case(crate::menu::first_setup::COMMAND)
        && tokens[1].eq_ignore_ascii_case(crate::menu::first_setup::IMPORT))
    {
        return None;
    }
    let (_, rest) = command.trim_start().split_once(char::is_whitespace)?;
    let (_, raw) = rest.trim_start().split_once(char::is_whitespace)?;
    let raw = raw.trim();
    let raw = raw
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(raw);
    Some(raw.to_owned())
}

impl Commands {
    /// Retain a recognized service command, preserving rcon's and
    /// `firstsetup import`'s raw argument text.
    pub fn queue(
        &mut self,
        command: &str,
        tokens: &[String],
    ) -> Option<Result<Vec<String>, String>> {
        if !COMMANDS
            .iter()
            .chain(crate::cosmetics::command::COMMANDS)
            .any(|(name, _)| name.eq_ignore_ascii_case(&tokens[0]))
        {
            return None;
        }
        if self.pending.len() == 64 {
            return Some(Err("Client command queue is full".into()));
        }
        if tokens[0].eq_ignore_ascii_case("rcon") {
            let raw = command
                .split_once(char::is_whitespace)
                .map_or("", |(_, args)| args);
            self.pending
                .push_back(vec![tokens[0].clone(), raw.to_owned()]);
        } else if let Some(path) = raw_import_path(command, tokens) {
            self.pending
                .push_back(vec![tokens[0].clone(), tokens[1].clone(), path]);
        } else {
            self.pending.push_back(tokens.to_vec());
        }
        Some(Ok(Vec::new()))
    }
}

impl ViewerConsole {
    /// Last successfully opened console demo, retained after playback stops.
    pub(crate) fn last_console_demo(&self) -> Option<&str> {
        self.client_commands.last_demo.as_deref()
    }

    /// Remember a demo after its header and gamestate have loaded successfully.
    pub(crate) fn remember_console_demo(&mut self, name: &str) {
        self.client_commands.last_demo = Some(name.to_owned());
    }
}

impl crate::GpuState {
    /// `sjkemote <id>`: play an emote the SJK players on this server see; alone it
    /// lists the installed ones.
    fn emote_command(&mut self, args: &[String]) -> Result<Vec<String>, String> {
        let Some(id) = args.first().map(|id| id.to_ascii_lowercase()) else {
            if crate::emotes::lock().catalogue.is_none()
                && let Some(vfs) = self.vfs.as_deref()
            {
                crate::emotes::lock().catalogue = Some(crate::emotes::load(vfs));
            }
            let state = crate::emotes::lock();
            return Ok(crate::emotes::listing(
                state.catalogue.as_deref().unwrap_or_default(),
            ));
        };
        if !crate::emotes::valid_id(&id) {
            return Err("An emote id is 1 to 32 of a to z, 0 to 9 and _".to_owned());
        }
        let Some(session) = self.live_session.as_ref() else {
            return Err("Join a server to emote".to_owned());
        };
        // The player's own emote starts here at once; the hub's echo of it is not
        // started again (`update_emotes`).
        let own = u8::try_from(session.latest_snapshot().player.client_num()).ok();
        if id == crate::emotes::STOP {
            if let Some(slot) = own {
                crate::emotes::lock().active.stop(slot);
            }
            // The others stop it too; without the identity there is nobody to tell.
            let _ = crate::player_identity::emote(id, None);
            return Ok(Vec::new());
        }
        if id == crate::emotes::SYNC {
            let Some(slot) = own else {
                return Err("Join a server to emote".to_owned());
            };
            return self.emote_sync(slot, args.get(1).map(String::as_str));
        }
        let mut state = crate::emotes::lock();
        if state.catalogue.is_none()
            && let Some(vfs) = self.vfs.as_deref()
        {
            state.catalogue = Some(crate::emotes::load(vfs));
        }
        let def = state
            .catalogue
            .as_ref()
            .and_then(|catalogue| catalogue.iter().find(|def| def.id == id))
            .cloned();
        if let Some(slot) = own {
            let origin = self
                .live_world
                .entity(sjk_runtime::EntityId::new(u64::from(slot) + 1))
                .map(|entity| entity.current().transform.translation)
                .unwrap_or_default();
            state
                .active
                .start(slot, &id, def.as_ref(), origin, std::time::Instant::now());
        }
        drop(state);
        if def.is_some_and(|def| def.local) {
            return Ok(Vec::new());
        }
        if !crate::player_identity::emote(id, None) {
            return Err(
                "Played here only: other SJK players see emotes with the SJK identity (cl_identity 1)"
                    .to_owned(),
            );
        }
        Ok(Vec::new())
    }

    /// `sjkemote sync [player]`: the player in `own` joins, in step, the emote of
    /// `target` (a slot number or part of a name), else of the player under the
    /// crosshair, else of the nearest one within [`crate::emotes::SYNC_RANGE`].
    fn emote_sync(&mut self, own: u8, target: Option<&str>) -> Result<Vec<String>, String> {
        let Some(session) = self.live_session.as_ref() else {
            return Err("Join a server to emote".to_owned());
        };
        let game_state = session.game_state();
        let now_ms = session.latest_snapshot().server_time;
        let position = |slot: u8| {
            self.live_world
                .entity(sjk_runtime::EntityId::new(u64::from(slot) + 1))
                .map(|entity| entity.current().transform.translation)
        };
        let here = position(own).unwrap_or_default();
        let mut state = crate::emotes::lock();
        let emoting: Vec<(u8, String)> = state
            .active
            .playing()
            .filter(|(slot, _)| *slot != own)
            .map(|(slot, active)| (slot, active.emote.clone()))
            .collect();
        let shown = |slot: u8| {
            crate::player_identity::shown_name(game_state, usize::from(slot)).unwrap_or_default()
        };
        let leader = match target {
            Some(target) => {
                let wanted = target.parse::<u8>().ok();
                let words = crate::emotes::plain_name(target);
                emoting
                    .iter()
                    .find(|(slot, _)| {
                        wanted == Some(*slot)
                            || (wanted.is_none()
                                && crate::emotes::plain_name(&shown(*slot)).contains(&words))
                    })
                    .ok_or_else(|| format!("{target} is not playing an emote"))?
            }
            None => {
                let aimed = self
                    .crosshair_scan
                    .chat_client(now_ms)
                    .and_then(|slot| u8::try_from(slot).ok());
                let distance = |slot: u8| {
                    position(slot).map_or(f32::INFINITY, |at| {
                        at.iter()
                            .zip(here)
                            .map(|(a, b)| (a - b) * (a - b))
                            .sum::<f32>()
                            .sqrt()
                    })
                };
                emoting
                    .iter()
                    .find(|(slot, _)| Some(*slot) == aimed)
                    .or_else(|| {
                        emoting
                            .iter()
                            .filter(|(slot, _)| distance(*slot) <= crate::emotes::SYNC_RANGE)
                            .min_by(|a, b| distance(a.0).total_cmp(&distance(b.0)))
                    })
                    .ok_or_else(|| "Nobody near you is playing an emote".to_owned())?
            }
        };
        let (leader, emote) = leader.clone();
        if state.catalogue.is_none()
            && let Some(vfs) = self.vfs.as_deref()
        {
            state.catalogue = Some(crate::emotes::load(vfs));
        }
        let def = state
            .catalogue
            .as_ref()
            .and_then(|catalogue| catalogue.iter().find(|def| def.id == emote))
            .cloned();
        state.active.sync(
            own,
            leader,
            &emote,
            def.as_ref(),
            here,
            std::time::Instant::now(),
        );
        drop(state);
        let name = crate::emotes::plain_name_shown(&shown(leader));
        if def.as_ref().is_some_and(|def| def.local) {
            return Ok(vec![format!("In step with {name} (played here only)")]);
        }
        if !crate::player_identity::emote(emote, Some(leader)) {
            return Err(
                "Played here only: other SJK players see emotes with the SJK identity (cl_identity 1)"
                    .to_owned(),
            );
        }
        Ok(vec![format!("In step with {name}")])
    }

    /// Open stock targeted chat using authoritative damage and retained crosshair state.
    pub(crate) fn targeted_chat(&mut self, attacker: bool) {
        if let Some(session) = &self.live_session {
            let snapshot = session.latest_snapshot();
            let slot = if !attacker {
                self.crosshair_scan.chat_client(snapshot.server_time)
            } else {
                self.damage_feedback
                    .has_attacker()
                    .then(|| u16::try_from(snapshot.player.persistent[6]).ok())
                    .flatten()
            };
            self.chat.update_roster(
                self.resident
                    .session
                    .as_ref()
                    .unwrap_or(session)
                    .game_state(),
            );
            if self.chat.whisper_to(slot) {
                self.gameplay_input.release_keys();
                if let Some(console) = &mut self.console {
                    console.set_open(false);
                }
                self.sync_cursor_policy();
            }
        }
    }
    /// `reply [message]` (JoF EternalJK's): whisper `message` to the last player who
    /// whispered to us, or open the composer on that whisper (`chat/reply.rs`).
    fn reply_command(&mut self, args: &[String]) -> Result<Vec<String>, String> {
        let Some(session) = &self.live_session else {
            return Err("reply: not in a game".into());
        };
        self.chat.update_roster(
            self.resident
                .session
                .as_ref()
                .unwrap_or(session)
                .game_state(),
        );
        if args.is_empty() {
            self.chat
                .open_reply()
                .map_err(|why| why.line().to_owned())?;
            self.gameplay_input.release_keys();
            if let Some(console) = &mut self.console {
                console.set_open(false);
            }
            self.sync_cursor_policy();
            return Ok(Vec::new());
        }
        let target = self
            .chat
            .reply_target()
            .map_err(|why| why.line().to_owned())?;
        let Some(command) = sjk_client::chat_command(
            sjk_client::ChatDestination::Player(target.slot()),
            &args.join(" "),
        ) else {
            return Ok(Vec::new());
        };
        let command = self.console.as_ref().map_or_else(
            || command.clone(),
            |console| console.color_chat_command(&command),
        );
        self.send_chat_command(&command);
        Ok(Vec::new())
    }

    /// Consume explicit service requests and nonblocking query replies each frame.
    pub(crate) fn run_client_commands(&mut self, audio: &mut Option<crate::GameAudio>) {
        while let Some(command) = self
            .console
            .as_mut()
            .and_then(|c| c.pending_chat.pop_front())
        {
            self.send_chat_command(&command);
        }
        if self.live_presentation_ready()
            && self
                .console
                .as_mut()
                .is_some_and(|c| std::mem::take(&mut c.client_commands.siege_class))
        {
            self.open_siege_classes();
        }
        if let (Some(audio), Some(console)) = (audio.as_mut(), self.console.as_mut()) {
            audio.print_sound_starts(console);
        }
        if let (Some(session), Some(console)) = (
            self.resident
                .session
                .as_mut()
                .or(self.live_session.as_mut()),
            &mut self.console,
        ) {
            while let Some(text) = session.pop_server_print() {
                console.push_log(text);
            }
            let limit = console.float_cvar("cl_timeout").unwrap_or(200.0).max(0.0);
            let timed_out = session
                .packet_silence()
                .is_some_and(|age| age.as_secs_f64() > limit);
            console.client_commands.timeout_frames = if timed_out {
                console.client_commands.timeout_frames.saturating_add(1)
            } else {
                0
            };
            if console.client_commands.timeout_frames > 5 {
                self.session_disconnected("Server connection timed out".into());
            }
        }
        loop {
            let command = self
                .console
                .as_mut()
                .and_then(|console| console.client_commands.pending.pop_front());
            let Some(tokens) = command else { break };
            let result = self.client_command(&tokens, audio);
            if let Some(console) = &mut self.console {
                match result {
                    Ok(lines) => {
                        for line in lines {
                            console.push_log(line);
                        }
                    }
                    Err(error) => console.push_log(format!("^1{error}")),
                }
            }
        }
        if let Some(console) = &mut self.console {
            let mut index = 0;
            while index < console.client_commands.jobs.len() {
                match console.client_commands.jobs[index].try_recv() {
                    Ok(lines) => {
                        console.client_commands.jobs.swap_remove(index);
                        for line in lines {
                            console.push_log(line);
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => index += 1,
                    Err(_) => {
                        console.client_commands.jobs.swap_remove(index);
                        console.push_log("^1Query worker stopped");
                    }
                }
            }
        }
    }

    fn client_command(
        &mut self,
        tokens: &[String],
        audio: &mut Option<crate::GameAudio>,
    ) -> Result<Vec<String>, String> {
        if let Some(result) = self.run_mod_command(tokens) {
            return result;
        }
        let name = tokens[0].to_ascii_lowercase();
        let args = &tokens[1..];
        match name.as_str() {
            "remapshader" | "listremaps" | "clearremaps" => {
                return self.remap_command(&name, args);
            }
            "blockremaps" | "unblockremaps" => {
                return self.remap_block_command(name == "blockremaps", args);
            }
            "speedometer" | "strafehelper" => {
                return hud_commands::execute(
                    self.console.as_mut().ok_or("Console unavailable")?,
                    &name,
                    args,
                );
            }
            "play" | "music" | "stopmusic" | "soundstop" | "soundlist" | "soundinfo"
            | "s_dynamic" => {
                let Some(audio) = audio else {
                    if name == "soundinfo" {
                        return Ok(vec!["Audio device closed/unavailable".into()]);
                    }
                    return Err("Audio is not initialized".into());
                };
                audio.sync_gains(self.console.as_ref());
                return audio.sound_command(&name, args, self.vfs.as_deref());
            }
            "messagemode" | "messagemode2" => {
                self.apply_input_action(Some(crate::input::InputAction::MessageMode(
                    name == "messagemode2",
                )));
            }
            "messagemode3" | "messagemode4" => {
                self.targeted_chat(name == "messagemode4");
            }
            "messagemode5" => {
                self.apply_input_action(Some(crate::input::InputAction::SjkMessageMode));
            }
            crate::am_color::COMMAND => {
                let console = self.console.as_mut().ok_or("Console unavailable")?;
                return crate::am_color::run(console, args);
            }
            "reply" => return self.reply_command(args),
            crate::player_ids::COMMAND => {
                let session = self
                    .resident
                    .session
                    .as_ref()
                    .or(self.live_session.as_ref())
                    .ok_or("showplayerid: not in a game")?;
                return Ok(crate::player_ids::lines(session.game_state()));
            }
            "toggleconsole" => {
                if let Some(console) = &mut self.console {
                    console.set_open(!console.is_open());
                }
                self.sync_cursor_policy();
            }
            "consolebrowser" => {
                if let Some(console) = &mut self.console {
                    console.open_browser();
                }
                self.sync_cursor_policy();
            }
            super::asset_browser::COMMAND => {
                if let Some(console) = &mut self.console {
                    console
                        .open_asset_browser(&self.game_data)
                        .map_err(|error| error.to_string())?;
                }
                self.sync_cursor_policy();
            }
            super::debug_panel::COMMAND => {
                if let Some(console) = &mut self.console {
                    console.toggle_debug_panel();
                }
                self.sync_cursor_policy();
            }
            super::model_viewer::COMMAND => {
                if let Some(console) = &mut self.console {
                    console.toggle_model_viewer();
                }
                self.sync_cursor_policy();
            }
            super::changelog::COMMAND => {
                if let Some(console) = &mut self.console {
                    console.toggle_changelog();
                }
                self.sync_cursor_policy();
            }
            super::credits::COMMAND => {
                if let Some(console) = &mut self.console {
                    console.toggle_credits();
                }
                self.sync_cursor_policy();
            }
            super::update_panel::COMMAND => {
                if let Some(console) = &mut self.console {
                    console.toggle_update_panel();
                }
                self.sync_cursor_policy();
            }
            // The SJK UI shows them as the Profile and Collection screens' tabs.
            crate::console::PROFILE_COMMAND if self.in_game_menu.is_sjk() => {
                self.toggle_profile_hub(crate::profile_hub::Tab::Profile);
            }
            crate::console::PROFILE_COMMAND => {
                if let Some(console) = &mut self.console {
                    console.toggle_profile_panel();
                }
                self.sync_cursor_policy();
            }
            crate::console::COLLECTION_COMMAND if self.in_game_menu.is_sjk() => {
                let tab = self
                    .console
                    .as_ref()
                    .map_or(crate::profile_hub::Tab::Medals, |console| {
                        console.collection_tab()
                    });
                self.toggle_profile_hub(tab);
            }
            crate::console::COLLECTION_COMMAND => {
                if let Some(console) = &mut self.console {
                    console.toggle_collection_panel(None);
                }
                self.sync_cursor_policy();
            }
            crate::emotes::COMMAND => return self.emote_command(args),
            crate::medal_popup::rehearsal::COMMAND => return self.debug_medal_command(args),
            crate::holocrons::rehearsal::COMMAND => return self.debug_holocron_command(args),
            crate::cards::rehearsal::COMMAND => return self.debug_open_command(args),
            crate::unlock_toast::UPDATE_COMMAND => return self.debug_update_command(args),
            crate::saber_skin_command::COMMAND => return self.saber_skin_command(args),
            crate::console::SJK_CHAT_COMMAND => {
                if let Some(console) = &mut self.console {
                    console.toggle_sjk_chat_panel();
                }
                self.sync_cursor_policy();
            }
            crate::console::STAFF_COMMAND => {
                if let Some(console) = &mut self.console {
                    console.toggle_staff_panel();
                }
                self.sync_cursor_policy();
            }
            crate::console::UNLOCKABLES_COMMAND if self.in_game_menu.is_sjk() => {
                self.toggle_profile_hub(crate::profile_hub::Tab::Shaders);
            }
            crate::console::UNLOCKABLES_COMMAND => {
                if let Some(console) = &mut self.console {
                    console.toggle_collection_panel(Some(crate::profile_hub::Tab::Shaders));
                }
                self.sync_cursor_policy();
            }
            crate::console::HOLOCRONS_COMMAND if self.in_game_menu.is_sjk() => {
                self.toggle_profile_hub(crate::profile_hub::Tab::Holocrons);
            }
            crate::console::HOLOCRONS_COMMAND => {
                if let Some(console) = &mut self.console {
                    console.toggle_holocrons_panel();
                }
                self.sync_cursor_policy();
            }
            crate::console::ACHIEVEMENTS_COMMAND if self.in_game_menu.is_sjk() => {
                self.toggle_profile_hub(crate::profile_hub::Tab::Achievements);
            }
            crate::console::ACHIEVEMENTS_COMMAND => {
                if let Some(console) = &mut self.console {
                    console.toggle_collection_panel(Some(crate::profile_hub::Tab::Achievements));
                }
                self.sync_cursor_policy();
            }
            crate::identity_command::COMMAND if args.is_empty() => {
                if let Some(console) = &mut self.console {
                    console.toggle_identity_panel();
                }
                self.sync_cursor_policy();
            }
            crate::identity_command::COMMAND => return self.identity_command(args),
            crate::avatar_command::COMMAND => return self.avatar_command(args),
            crate::weather::COMMAND => return self.weather_command(args),
            crate::quick_wheel::OPEN_COMMAND => return self.open_quick_wheel(args),
            crate::quick_wheel::RUN_COMMAND | crate::quick_wheel::GIF_RUN_COMMAND => {
                return self.release_quick_wheel();
            }
            crate::quick_wheel::GIF_OPEN_COMMAND => return self.open_gif_wheel(),
            crate::console::SJK_SAY_COMMAND => {
                let console = self.console.as_mut().ok_or("Console unavailable")?;
                return console.sjk_say_command(args);
            }
            crate::console::SJK_GIF_COMMAND => {
                let console = self.console.as_mut().ok_or("Console unavailable")?;
                return console.sjk_gif_command(args);
            }
            crate::hud::nameplate::COMMAND => return self.nameplate_command(args),
            crate::graphics_quality::COMMAND => {
                let console = self.console.as_mut().ok_or("Console unavailable")?;
                return crate::graphics_quality::command(console, args);
            }
            crate::graphics_quality::RENDERER_ROW => {
                let console = self.console.as_mut().ok_or("Console unavailable")?;
                return crate::graphics_quality::renderer_command(console, args);
            }
            crate::menu::first_setup::COMMAND
                if args.first().is_some_and(|arg| {
                    arg.eq_ignore_ascii_case(crate::menu::first_setup::IMPORT)
                }) =>
            {
                // A path with spaces may come unquoted: its words are joined again.
                let path = args[1..].join(" ");
                if let Some(console) = &mut self.console {
                    console.open_config_import(
                        (!path.is_empty()).then(|| std::path::Path::new(&path)),
                    );
                }
                self.sync_cursor_policy();
            }
            crate::menu::first_setup::COMMAND => {
                if let Some(console) = &mut self.console {
                    console.set_open(false);
                }
                let in_game = self.live_session.is_some();
                if let (Some(menu), Some(console)) = (&mut self.client_menu, &self.console) {
                    let target = if in_game {
                        crate::player_menu::ReturnTarget::InGame
                    } else {
                        crate::player_menu::ReturnTarget::MainMenu
                    };
                    menu.open_first_setup(console, target);
                    self.game_menu = false;
                }
                self.sync_cursor_policy();
            }
            "togglemenu" => {
                if self.game_menu {
                    self.game_menu = false;
                    self.sync_cursor_policy();
                } else if self.live_session.is_some() {
                    self.release_pointer();
                    self.game_menu = true;
                    self.game_menu_page = crate::ingame_menu::Page::Main;
                    self.game_menu_row = 0;
                } else {
                    return Err("No game session; the client menu is already available".into());
                }
            }
            "minimize" => {
                self.window
                    .as_ref()
                    .ok_or("No window to minimize")?
                    .set_minimized(true);
            }
            "fakenoclip" => return self.fake_noclip_command(),
            crate::ghost_replay::COMMAND | crate::mimic::COMMAND if !self.showcase_enabled() => {
                return Err(crate::ghost_replay::showcase_off(&name));
            }
            crate::ghost_replay::COMMAND => return self.ghost_command(args),
            crate::mimic::COMMAND => return self.mimic_command(args),
            crate::fps_help::COMMAND => return self.fps_help_command(),
            crate::whats_new_card::COMMAND => {
                self.preview_whats_new();
                return Ok(Vec::new());
            }
            crate::gameplay_fps::COMMAND => return self.gameplay_fps_command(args),
            "freecam" => return self.free_camera_command(args),
            "peek" => return self.peek_command(args),
            "cosmetics" => return self.cosmetics_command(args),
            "vid_restart" | "snd_restart" | "in_restart" | "modelist" => {
                return self.restart_command(&name, audio);
            }
            "ping" | "serverstatus" | "globalservers" | "localservers" | "rcon" | "showip" => {
                let server = self.live_session.as_ref().map(|session| session.server());
                let console = self.console.as_mut().ok_or("Console unavailable")?;
                queries::start(console, &name, args, server)?;
            }
            "addfavorite" => {
                let address = match args {
                    [address] => queries::address(address, 29070)?,
                    [] => self
                        .live_session
                        .as_ref()
                        .ok_or("Not connected to a server.")?
                        .server(),
                    _ => return Err("usage: addFavorite [address]".into()),
                };
                let menu = self
                    .client_menu
                    .as_mut()
                    .ok_or("Server browser unavailable")?;
                menu.add_favorite(address)?;
                return Ok(vec![format!("Added favorite {address}")]);
            }
            _ => {
                let console = self.console.as_mut().ok_or("Console unavailable")?;
                return console.info_command(&name, args, self.live_session.as_mut());
            }
        }
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn import_path(command: &str) -> Option<String> {
        raw_import_path(command, &sjk_shell::tokenize(command).unwrap())
    }

    #[test]
    fn an_import_path_keeps_its_backslashes_quoted_or_not() {
        let quoted = r#"firstsetup import "C:\Users\turin\Downloads\turin.cfg""#;
        assert_eq!(
            import_path(quoted).as_deref(),
            Some(r"C:\Users\turin\Downloads\turin.cfg")
        );
        let spaced = r"firstsetup IMPORT  C:\My Games\jampconfig.cfg ";
        assert_eq!(
            import_path(spaced).as_deref(),
            Some(r"C:\My Games\jampconfig.cfg")
        );
        assert_eq!(import_path("firstsetup import"), None);
        assert_eq!(import_path("firstsetup"), None);
    }
}
