//! The operator's console lines the engine's own commands leave to this server: the
//! world commands `map`, `devmap` and `map_restart` (`sv_ccmds.cpp`), which rebuild the
//! level, the console variables (`cvar.cpp`, [`crate::cvars`]), and the game's console
//! commands (`g_svcmds.c:ConsoleCommand`), in `Cmd_ExecuteString`'s order.
//!
//! The roster commands — `status`, the kicks, `svsay`, `svtell`, `serverinfo`,
//! `dumpuser` — and the remote console itself are the legacy endpoint's
//! (`sjk_network::execute_legacy_console`); they reach this server only as drops and
//! server commands.

use super::NativeGame;
use sjk_network::LegacyTokens;

/// `MAX_SAY_TEXT`.
const SAY_TEXT: usize = 150;

impl NativeGame {
    /// A console line no engine command claimed. Returns whether this server knows it;
    /// one it does not is silent, as on a reference dedicated server (cvars are not
    /// served yet, so a cvar's name is one of those).
    pub(super) fn console_line(
        &mut self,
        line: &[u8],
        server_time: i32,
        bots: &mut dyn sjk_network::LegacyBotSlots,
        print: &mut dyn FnMut(&[u8]),
    ) -> bool {
        let words: Vec<&[u8]> = LegacyTokens::new(line).collect();
        let Some(&name) = words.first() else {
            return false;
        };
        let is = |command: &[u8]| name.eq_ignore_ascii_case(command);
        let argument = words.get(1).copied().unwrap_or_default();

        if is(b"map") || is(b"devmap") {
            self.console_map(argument, is(b"devmap"), server_time, print);
        } else if self.stopped && (is(b"map_restart") || is(b"say")) {
            // `SV_MapRestart_f` and the game's commands need a running server.
            if is(b"map_restart") {
                print(b"Server is not running.\n");
            }
            return is(b"map_restart");
        } else if is(b"map_restart") {
            // `SV_MapRestart_f`: `atoi` of the delay, five seconds without one.
            let delay = if words.len() > 1 {
                sjk_game_jka::userinfo::atoi(argument)
            } else {
                5
            };
            // A game type waiting to be taken needs the level built anew, not replayed.
            if delay == 0
                && self.restart_at == 0
                && self
                    .cvars
                    .var(b"g_gametype")
                    .is_some_and(|var| var.latched.is_some())
            {
                print(b"variable change -- restarting.\n");
                let mapname = self.identity.mapname.clone();
                self.console_map(&mapname, self.settings.cheats, server_time, print);
            } else {
                self.map_restart(delay, server_time);
            }
        } else if self.commands.command(
            &words,
            &self.cvars,
            &mut |name| self.config_files.read(name),
            print,
        ) {
        } else if self.cvar_line(line, print) {
        } else if !self.stopped && self.bot_command(&words, server_time, bots, print).is_some() {
        } else if !self.stopped && self.game_console_command(&words, server_time, print) {
        } else if is(b"say") {
            self.console_say(&words[1..]);
        } else {
            return false;
        }
        true
    }

    /// `SV_Map_f`: a map that does not load leaves the server where it is, and `devmap`
    /// turns the cheats on where `map` turns them off.
    fn console_map(
        &mut self,
        name: &[u8],
        cheats: bool,
        server_time: i32,
        print: &mut dyn FnMut(&[u8]),
    ) {
        if name.contains(&b'\\') {
            print(b"Can't have mapnames with a \\\n");
            return;
        }
        let name = String::from_utf8_lossy(name).into_owned();
        let loaded = match &self.game_data {
            Some(game_data) => match crate::map::load(game_data, &name) {
                Ok(map) => Some(map),
                Err(error) => {
                    println!("map {name} does not load: {error}");
                    print(format!("Can't find map maps/{name}.bsp\n").as_bytes());
                    return;
                }
            },
            // A server started without game data plays its maps by name alone.
            None => None,
        };
        self.change_map(&name, loaded, server_time);
        self.set_cvar("sv_cheats", if cheats { "1" } else { "0" });
    }

    /// `Svcmd_Say_f`: the words at most 149 bytes, newlines made spaces, printed to
    /// everyone as coming from the server.
    fn console_say(&mut self, words: &[&[u8]]) {
        if words.is_empty() {
            return;
        }
        let mut text = words.join(&b' ');
        text.truncate(SAY_TEXT - 1);
        for byte in &mut text {
            if matches!(*byte, b'\n' | b'\r') {
                *byte = b' ';
            }
        }
        self.told.push(super::Told::Everyone(
            [&b"print \"server: "[..], &text, b"\n\""].concat(),
        ));
    }

    /// `Cbuf_AddText`: text for the console, run when the process next drains it —
    /// a typed line, or the command line's `+` commands.
    pub fn queue_console(&mut self, text: &[u8], print: &mut dyn FnMut(&[u8])) {
        self.commands.add_text(text, print);
    }

    /// The next line of this frame's pass over the command buffer (`Cbuf_Execute`), to
    /// be run through the whole console; `None` ends the pass.
    pub fn next_console_line(
        &mut self,
        pass: &mut crate::command_buffer::CommandPass,
    ) -> Option<Vec<u8>> {
        self.commands.next_line(pass)
    }

    /// Where `exec` looks and where archived variables are written.
    /// `G_InitGame` opens the game log there, as the first level starts.
    pub fn set_config_files(&mut self, files: crate::config_files::ConfigFiles) {
        self.config_files = files;
        self.register_stock_rules();
        self.open_log();
        self.start_warmup();
        // `G_InitBots`.
        self.load_bots();
    }

    /// `Com_WriteConfiguration`: the archived variables written out whenever one of them
    /// changed; a failure is told on the console.
    pub fn write_config(&mut self, print: &mut dyn FnMut(&[u8])) {
        if self.cvars.modified_flags & crate::cvars::CVAR_ARCHIVE == 0 {
            return;
        }
        self.cvars.modified_flags &= !crate::cvars::CVAR_ARCHIVE;
        let lines = self.cvars.archive_lines(print);
        if self.config_files.write_archive(&lines).is_err() {
            print(format!("Couldn't write {}.\n", crate::config_files::ARCHIVE_FILE).as_bytes());
        }
    }

    /// The ban file (`sv_banFile` in the home directory), if there is one.
    pub(super) fn read_ban_file(&self) -> Option<Vec<u8>> {
        let name = String::from_utf8_lossy(self.cvars.string(b"sv_banFile")).into_owned();
        (!name.is_empty())
            .then(|| self.config_files.read_home(&name))
            .flatten()
    }

    /// `SV_WriteBans`' file; nothing without a name or a home directory.
    pub(super) fn write_ban_file(&self, text: &[u8]) {
        let name = String::from_utf8_lossy(self.cvars.string(b"sv_banFile")).into_owned();
        if !name.is_empty() && self.config_files.write_home(&name, text).is_err() {
            println!("couldn't write {name}");
        }
    }
}
