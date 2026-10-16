//! Integrated command console over cvars, binds, and persistence.

use crate::{
    BindTable, CommandBuffer, CommandFileResolver, CommandRegistry, CvarRegistry, NoCommandFiles,
    ShellError, load_config, tokenize,
};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[path = "shell_builtins.rs"]
mod builtins;
#[path = "shell_completion.rs"]
mod completion;
#[path = "shell_cvar_commands.rs"]
mod cvar_commands;
#[path = "shell_dispatch.rs"]
mod dispatch;
#[path = "shell_file_log.rs"]
mod file_log;
#[path = "shell_schedule.rs"]
mod schedule;

pub use completion::CompletionKey;

const DEFAULT_LOG_CAPACITY: usize = 256;
/// Bytes of the `[HH:MM:SS] ` prefix of [`ConsoleLine::stamped_text`].
const STAMP_PREFIX_BYTES: usize = 11;
/// Echoed before typed input, as `Console_Key` prints `CONSOLE_PROMPT_CHAR`
/// straight before the line: `]cmd`.
pub(crate) const PROMPT: &str = "]";

/// Origin/severity of a console line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsoleLineKind {
    /// A command entered by the user.
    Input,
    /// Normal command output.
    Output,
    /// Parsing, validation, or handler failure.
    Error,
    /// Application-originated diagnostic text.
    Log,
}

/// Where a command listed by [`Shell::command_help`] is handled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandSource {
    /// A command of the shell itself (`bind`, `set`, `exec`, ...).
    Shell,
    /// A command the application registered.
    Application,
    /// A command forwarded to an external processor, such as the game server.
    External,
}

/// One bounded console scrollback entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsoleLine {
    /// Monotonic shell-clock timestamp for expiring notify text.
    pub written_millis: u64,
    /// Timestamped display copy, prepared once when the line is appended: the
    /// local time it was written as `[HH:MM:SS] `, then [`Self::text`].
    pub stamped_text: String,
    /// Origin/severity used by frontends for presentation.
    pub kind: ConsoleLineKind,
    /// Console-formatted text.
    pub text: String,
    /// Whether the line may show among the transient notify lines while the
    /// console is closed. Stock `CL_ConsolePrint` (`cl_console.cpp`) clears this
    /// for text starting with `*` or `[skipnotify]`, which cgame uses for chat.
    pub notify: bool,
}

impl ConsoleLine {
    /// The local time the line was written, `HH:MM:SS`, as EternalJK draws it
    /// in the console's left column.
    pub fn clock(&self) -> &str {
        self.stamped_text.get(1..9).unwrap_or("")
    }
}

/// Complete portable shell state used by a client frontend.
pub struct Shell {
    schedule: schedule::Schedule,
    /// Typed configuration registry.
    pub cvars: CvarRegistry,
    /// Application/mod command handlers.
    pub commands: CommandRegistry,
    /// Physical-input command bindings.
    pub binds: BindTable,
    config_path: Option<PathBuf>,
    config_load_failed: bool,
    config_saver: crate::config_saver::ConfigSaver,
    file_log: file_log::FileLog,
    /// Sees every line as it is kept ([`Shell::set_line_tap`]).
    line_tap: Option<fn(&str)>,
    lines: VecDeque<ConsoleLine>,
    log_capacity: usize,
    /// Lines ever appended, so frontends can name a line across scrollback trimming.
    lines_written: u64,
    external_command_help: BTreeMap<String, String>,
    command_buffer: CommandBuffer,
}

impl Shell {
    /// Compose shell state around application-defined cvars and default binds.
    pub fn new(cvars: CvarRegistry, binds: BindTable) -> Self {
        Self {
            schedule: schedule::Schedule::default(),
            cvars,
            commands: CommandRegistry::new(),
            binds,
            config_path: None,
            config_load_failed: false,
            config_saver: Default::default(),
            file_log: file_log::FileLog::default(),
            line_tap: None,
            lines: VecDeque::with_capacity(DEFAULT_LOG_CAPACITY),
            log_capacity: DEFAULT_LOG_CAPACITY,
            lines_written: 0,
            external_command_help: BTreeMap::new(),
            command_buffer: CommandBuffer::new(),
        }
    }

    /// Select the explicit application-owned persistence file.
    pub fn set_config_path(&mut self, path: impl Into<PathBuf>) {
        self.config_path = Some(path.into());
    }

    /// Return the selected persistence file, if any.
    pub fn config_path(&self) -> Option<&Path> {
        self.config_path.as_deref()
    }

    /// Restore archived cvars and the complete bind table.
    pub fn load(&mut self) -> Result<(), ShellError> {
        let path = self
            .config_path
            .as_deref()
            .ok_or(ShellError::NoConfigPath)?;
        let result = load_config(path, &mut self.cvars, &mut self.binds);
        self.config_load_failed = result.is_err();
        result?;
        // The file now holds exactly this state: nothing to save yet.
        self.config_saver.mark_saved(self.config_revision());
        Ok(())
    }

    fn config_revision(&self) -> crate::config_saver::Revision {
        (self.cvars.archive_revision(), self.binds.revision())
    }

    /// Whether archived cvars or binds changed since the config was loaded or saved.
    pub fn config_dirty(&self) -> bool {
        self.config_saver.is_dirty(self.config_revision())
    }

    /// Persist archived cvars and the complete bind table now, on this thread,
    /// after any background save still running. For exits and for code about to
    /// read the file back.
    pub fn save(&mut self) -> Result<(), ShellError> {
        // A failed earlier write is moot once this one lands.
        let _ = self.config_saver.wait_idle();
        if self.config_load_failed {
            return Err(ShellError::ConfigLoadFailed);
        }
        let path = self
            .config_path
            .as_deref()
            .ok_or(ShellError::NoConfigPath)?;
        let current = self.config_revision();
        crate::config::write_config_text(
            path,
            &crate::config::config_text(&self.cvars, &self.binds)?,
        )?;
        self.config_saver.mark_saved(current);
        Ok(())
    }

    /// Save only if something saved changed, now, on this thread: [`Self::save`]
    /// when [`Self::config_dirty`].
    pub fn save_if_dirty(&mut self) -> Result<(), ShellError> {
        if self.config_dirty() {
            self.save()
        } else {
            self.config_saver.wait_idle()?;
            Ok(())
        }
    }

    /// Once per frame: save archived cvars and binds once they changed and then
    /// held still for [`crate::CONFIG_SAVE_DELAY`]. The text is built here and
    /// written on a background thread; an unchanged frame costs two comparisons.
    /// Returns an error an earlier background write met, or why this one could not
    /// be queued (each reported once).
    pub fn autosave(&mut self, now: Instant) -> Result<(), ShellError> {
        if let Some(error) = self.config_saver.take_error() {
            return Err(error.into());
        }
        let current = self.config_revision();
        if !self.config_saver.due(current, now) {
            return Ok(());
        }
        let Some(path) = self.config_path.clone() else {
            return Ok(());
        };
        if self.config_load_failed {
            // Once per change, as each save used to report it.
            self.config_saver.mark_saved(current);
            return Err(ShellError::ConfigLoadFailed);
        }
        let contents = crate::config::config_text(&self.cvars, &self.binds)?;
        self.config_saver.queue(current, path, contents)?;
        Ok(())
    }

    /// Iterate bounded scrollback from oldest to newest.
    pub fn lines(&self) -> impl DoubleEndedIterator<Item = &ConsoleLine> {
        self.lines.iter()
    }

    /// Number of lines ever appended to scrollback. Lines are numbered from zero in
    /// the order they were appended, so the newest line in [`Self::lines`] is number
    /// `lines_written() - 1`. Numbers are never reused, even after trimming or
    /// clearing, so a frontend can keep one (for a text selection, say) and later
    /// find the line again or learn that it is gone.
    pub fn lines_written(&self) -> u64 {
        self.lines_written
    }

    /// The scrollback line numbered `number` (see [`Self::lines_written`]), if it
    /// is still kept.
    pub fn line(&self, number: u64) -> Option<&ConsoleLine> {
        let first = self.lines_written - self.lines.len() as u64;
        usize::try_from(number.checked_sub(first)?)
            .ok()
            .and_then(|index| self.lines.get(index))
    }

    /// Remove every scrollback line.
    pub fn clear_lines(&mut self) {
        self.lines.clear();
    }

    /// Append application-originated text to bounded scrollback.
    pub fn push_log(&mut self, text: impl Into<String>) {
        self.push_line(ConsoleLineKind::Log, text.into());
    }

    /// Hand every line the scrollback keeps to `tap` as well, as it is kept (a
    /// frontend's crash trail); `cl_noprint` silences it too.
    pub fn set_line_tap(&mut self, tap: fn(&str)) {
        self.line_tap = Some(tap);
    }

    /// Append application-originated text to scrollback only, never to the
    /// notify lines: the stock `*` print prefix cgame uses to echo chat.
    pub fn push_log_quiet(&mut self, text: impl Into<String>) {
        self.push_line_with(ConsoleLineKind::Log, text.into(), false);
    }

    /// Replace completion/help metadata for commands forwarded to an external
    /// command processor. These names remain unknown to local dispatch.
    pub fn replace_external_command_help(
        &mut self,
        commands: impl IntoIterator<Item = (String, String)>,
    ) {
        self.external_command_help.clear();
        self.external_command_help.extend(
            commands
                .into_iter()
                .map(|(name, description)| (name.to_ascii_lowercase(), description)),
        );
    }

    /// Every command `cmdlist` lists, with its description and where it runs, in
    /// no particular order.
    pub fn command_help(&self) -> impl Iterator<Item = (&str, &str, CommandSource)> {
        builtin_commands()
            .map(|(name, description)| (name, description, CommandSource::Shell))
            .chain(
                self.commands
                    .iter()
                    .map(|(name, description)| (name, description, CommandSource::Application)),
            )
            .chain(
                self.external_command_help
                    .iter()
                    .map(|(name, description)| {
                        (name.as_str(), description.as_str(), CommandSource::External)
                    }),
            )
    }

    /// Return whether the first token is handled by this shell rather than an
    /// external command processor.
    pub fn recognizes_local_command(&self, name: &str) -> bool {
        builtin_commands().any(|(builtin, _)| builtin.eq_ignore_ascii_case(name))
            || self.commands.contains(name)
            || self.cvars.get(name).is_some()
    }

    /// Return the unique case-insensitive command-name completion, including
    /// external commands advertised by the active application. Ambiguous
    /// prefixes return `None`; the scan does not allocate.
    pub fn unique_command_completion(&self, prefix: &str) -> Option<&str> {
        let mut found = None;
        for name in builtin_commands()
            .map(|(name, _)| name)
            .chain(self.commands.iter().map(|(name, _)| name))
            .chain(self.cvars.iter().map(|cvar| cvar.name.as_str()))
            .chain(self.external_command_help.keys().map(String::as_str))
            .filter(|name| ascii_starts_with_ignore_case(name, prefix))
        {
            if found.is_some_and(|current: &str| !current.eq_ignore_ascii_case(name)) {
                return None;
            }
            found = Some(name);
        }
        found
    }

    /// Queue one possibly-semicolon-separated line behind pending commands.
    pub fn queue_line(&mut self, input: &str) -> Result<(), ShellError> {
        let input = input.trim();
        if input.is_empty() {
            return Ok(());
        }
        self.push_line(ConsoleLineKind::Input, format!("{PROMPT}{input}"));
        self.command_buffer.append(input)?;
        Ok(())
    }

    /// Append command text without echoing it as typed console input.
    ///
    /// Frontends use this for physical-key bindings and generated scripts.
    pub fn queue_script(&mut self, input: &str) -> Result<(), ShellError> {
        self.command_buffer.append(input)?;
        Ok(())
    }

    /// Return whether a command or `wait` delay remains buffered.
    pub fn has_buffered_commands(&self) -> bool {
        !self.command_buffer.is_idle() || !self.schedule.pending.is_empty()
    }

    /// Execute one frame of buffered commands with application interception.
    ///
    /// The interceptor runs before built-in dispatch. Returning `Some` marks
    /// the command as application-owned; `None` continues local shell dispatch.
    /// Errors are logged and do not abort later commands, matching cfg behavior.
    pub fn execute_buffered_frame_with<R, F>(
        &mut self,
        resolver: &mut R,
        mut intercept: F,
    ) -> Result<Vec<String>, ShellError>
    where
        R: CommandFileResolver,
        F: FnMut(&str, &[String], bool) -> Option<Result<Vec<String>, String>>,
    {
        self.resume_scheduled()?;
        if self.command_buffer.is_idle() {
            return Ok(Vec::new());
        }
        let mut output = Vec::new();
        let mut first_error = None;
        loop {
            // OpenJK checks `cmd_wait` at the head of Cbuf_Execute's loop and
            // decrements it before returning (`cmd.cpp:189-195`). Thus the
            // frame which executed `wait` consumes the first wait count.
            if self.command_buffer.consume_wait() {
                break;
            }
            let Some(queued) = self.command_buffer.pop_front() else {
                break;
            };
            let result = match tokenize(&queued.text) {
                Ok(tokens) => {
                    let Some((name, arguments)) = tokens.split_first() else {
                        continue;
                    };
                    let local = self.recognizes_local_command(name);
                    if let Some(result) = intercept(&queued.text, &tokens, local) {
                        result.map_err(ShellError::Application)
                    } else {
                        self.execute_buffered_one(name, arguments, queued.exec_depth, resolver)
                    }
                }
                Err(error) => Err(error.into()),
            };
            match result {
                Ok(lines) => {
                    for line in lines {
                        self.push_line(ConsoleLineKind::Output, line.clone());
                        output.push(line);
                    }
                }
                Err(error) => {
                    self.push_line(ConsoleLineKind::Error, error.to_string());
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        first_error.map_or(Ok(output), Err)
    }

    /// Execute one buffered frame without an application interceptor.
    pub fn execute_buffered_frame<R: CommandFileResolver>(
        &mut self,
        resolver: &mut R,
    ) -> Result<Vec<String>, ShellError> {
        self.execute_buffered_frame_with(resolver, |_, _, _| None)
    }

    /// Execute one possibly-semicolon-separated console line immediately.
    pub fn execute_line(&mut self, input: &str) -> Result<Vec<String>, ShellError> {
        self.queue_line(input)?;
        self.execute_buffered_frame(&mut NoCommandFiles)
    }

    fn push_line(&mut self, kind: ConsoleLineKind, text: String) {
        self.push_line_with(kind, text, true);
    }

    fn push_line_with(&mut self, kind: ConsoleLineKind, text: String, notify: bool) {
        // OpenJK cl_console.cpp:610-612: suppress console printing at its sink.
        if self
            .cvars
            .get("cl_noprint")
            .is_some_and(|v| v.value.as_text() != "0")
        {
            return;
        }
        if self.lines.len() == self.log_capacity {
            self.lines.pop_front();
        }
        if let Some(tap) = self.line_tap {
            tap(&text);
        }
        let written_millis = self.command_clock_millis();
        // Local time, as `Com_RealTime` stamps console text; frontends that draw
        // the stamp on its own read it back from bytes 1..9.
        let mut stamped_text = String::with_capacity(text.len() + STAMP_PREFIX_BYTES);
        stamped_text.push('[');
        crate::local_time::LocalTime::now().push_clock(&mut stamped_text);
        stamped_text.push_str("] ");
        stamped_text.push_str(&text);
        let mode = self
            .cvars
            .get("logfile")
            .and_then(|cvar| match cvar.value {
                crate::CvarValue::Integer(value) => Some(value),
                _ => None,
            })
            .unwrap_or(0);
        let timestamps = !self
            .cvars
            .get("com_timestamps")
            .is_some_and(|cvar| cvar.value == crate::CvarValue::Integer(0));
        self.file_log
            .write(mode, if timestamps { &stamped_text } else { &text });
        self.lines.push_back(ConsoleLine {
            kind,
            notify,
            text,
            written_millis,
            stamped_text,
        });
        self.lines_written += 1;
    }
}

fn builtin_commands() -> impl Iterator<Item = (&'static str, &'static str)> {
    [
        ("bind", "Set or inspect a key binding"),
        ("bindlist", "List key bindings"),
        ("clear", "Clear console scrollback"),
        ("cmdlist", "List commands"),
        ("cvarlist", "List configuration variables"),
        ("echo", "Print text"),
        ("print", "Describe a cvar"),
        ("path", "List filesystem search roots"),
        ("dir", "List a virtual directory"),
        ("fdir", "List files matching a wildcard"),
        ("touchFile", "Read a file through the search path"),
        ("which", "Locate the winning file source"),
        ("condump", "Write console scrollback to a text file"),
        ("exec", "Execute a configuration script"),
        ("reset", "Restore a cvar default"),
        ("set", "Set or create a session cvar"),
        ("seta", "Set or create an archived cvar"),
        ("toggle", "Toggle a boolean/numeric cvar"),
        ("unbind", "Remove a key binding"),
        ("unbindall", "Remove all key bindings"),
        ("vstr", "Execute a cvar value as commands"),
        ("wait", "Pause buffered commands for frames"),
        ("writeconfig", "Persist archived cvars and binds"),
        (
            "write",
            "Persist archived cvars and binds (writeconfig alias)",
        ),
    ]
    .into_iter()
    .chain(cvar_commands::COMMANDS.iter().copied())
    .chain(schedule::COMMANDS.iter().copied())
}

fn list_matches(name: &str, arguments: &[String]) -> bool {
    arguments.first().is_none_or(|pattern| {
        name.to_ascii_lowercase()
            .contains(&pattern.to_ascii_lowercase())
    })
}

fn default_cfg_extension(requested: &str) -> String {
    if Path::new(requested).extension().is_some() {
        requested.to_owned()
    } else {
        format!("{requested}.cfg")
    }
}

fn ascii_starts_with_ignore_case(value: &str, prefix: &str) -> bool {
    value.len() >= prefix.len()
        && value
            .bytes()
            .zip(prefix.bytes())
            .all(|(left, right)| left.eq_ignore_ascii_case(&right))
}

#[cfg(test)]
mod notify_tests {
    use super::*;

    #[test]
    fn quiet_log_lines_skip_notify_but_stay_in_scrollback() {
        let mut shell = Shell::new(CvarRegistry::new(), BindTable::new());
        shell.push_log("print");
        shell.push_log_quiet("chat");
        let lines: Vec<_> = shell
            .lines()
            .map(|line| (line.text.as_str(), line.notify))
            .collect();
        assert_eq!(lines, [("print", true), ("chat", false)]);
    }
}

#[cfg(test)]
mod key_display_tests {
    use super::*;

    #[test]
    fn bind_output_shows_uppercase_names_but_stores_the_canonical_one() {
        let mut shell = Shell::new(CvarRegistry::new(), BindTable::new());
        assert_eq!(
            shell.execute_line("bind W +forward").unwrap(),
            ["W = \"+forward\""]
        );
        assert_eq!(shell.execute_line("bind w").unwrap(), ["W = \"+forward\""]);
        let list = shell.execute_line("bindlist").unwrap();
        assert!(list.iter().any(|line| line.starts_with("W ")), "{list:?}");
        // Configs keep the saved spelling.
        assert_eq!(shell.binds.iter().collect::<Vec<_>>(), [("w", "+forward")]);
        assert_eq!(shell.execute_line("unbind w").unwrap(), ["W unbound"]);
    }
}
