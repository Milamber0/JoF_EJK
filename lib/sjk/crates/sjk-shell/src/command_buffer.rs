//! Bounded Quake-style command buffering for scripts and key bindings.

use crate::{CommandError, split_commands};
use std::collections::VecDeque;
use std::fmt::{Display, Formatter};

/// Maximum nested `exec` depth accepted by the portable shell.
pub const MAX_EXEC_DEPTH: u8 = 16;
/// Maximum aggregate command text retained by the portable shell.
pub const MAX_COMMAND_BUFFER_BYTES: usize = 1_048_576;

const INITIAL_COMMAND_CAPACITY: usize = 256;

/// Application-provided source of console script files.
///
/// The shell deliberately owns no filesystem or VFS policy. Frontends can
/// search a writable config directory, package mounts, or another source.
pub trait CommandFileResolver {
    /// Application-owned filesystem operations; headless shells fail explicitly.
    fn file_command(
        &mut self,
        _name: &str,
        _args: &[String],
        _dump: &str,
    ) -> Result<Vec<String>, String> {
        Err("No filesystem command provider is attached".to_owned())
    }
    /// Return UTF-8 script text for `path`, or `None` when it does not exist.
    fn read_command_file(&mut self, path: &str) -> Result<Option<String>, String>;
}

/// Resolver used when immediate callers have no file-search policy.
#[derive(Default)]
pub struct NoCommandFiles;

impl CommandFileResolver for NoCommandFiles {
    fn read_command_file(&mut self, _path: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
}

#[derive(Debug)]
pub(crate) struct QueuedCommand {
    pub(crate) text: String,
    pub(crate) exec_depth: u8,
}

/// Front-insert/back-append command queue evaluated once per rendered frame.
///
/// OpenJK's `Cbuf_InsertText` inserts nested script commands immediately after
/// the current command (`codemp/qcommon/cmd.cpp:111-138`). `wait` leaves the
/// remainder queued for later frames (`cmd.cpp:176-254`).
pub struct CommandBuffer {
    commands: VecDeque<QueuedCommand>,
    retained_bytes: usize,
    wait_frames: u32,
}

impl Default for CommandBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandBuffer {
    /// Construct an empty queue with reusable command-node storage.
    pub fn new() -> Self {
        Self {
            commands: VecDeque::with_capacity(INITIAL_COMMAND_CAPACITY),
            retained_bytes: 0,
            wait_frames: 0,
        }
    }

    /// Append console text behind commands already waiting to execute.
    pub fn append(&mut self, text: &str) -> Result<(), CommandBufferError> {
        self.append_at_depth(text, 0)
    }

    /// Return whether no commands or waits remain.
    pub fn is_idle(&self) -> bool {
        self.commands.is_empty() && self.wait_frames == 0
    }

    pub(crate) fn append_at_depth(
        &mut self,
        text: &str,
        exec_depth: u8,
    ) -> Result<(), CommandBufferError> {
        let commands = script_commands(text)?;
        self.ensure_capacity(&commands)?;
        for text in commands {
            self.retained_bytes += text.len() + 1;
            self.commands.push_back(QueuedCommand { text, exec_depth });
        }
        Ok(())
    }

    pub(crate) fn insert_after_current(
        &mut self,
        text: &str,
        exec_depth: u8,
    ) -> Result<(), CommandBufferError> {
        if exec_depth > MAX_EXEC_DEPTH {
            return Err(CommandBufferError::ExecDepth(MAX_EXEC_DEPTH));
        }
        let commands = script_commands(text)?;
        self.ensure_capacity(&commands)?;
        for text in commands.into_iter().rev() {
            self.retained_bytes += text.len() + 1;
            self.commands.push_front(QueuedCommand { text, exec_depth });
        }
        Ok(())
    }

    pub(crate) fn pop_front(&mut self) -> Option<QueuedCommand> {
        let command = self.commands.pop_front()?;
        self.retained_bytes = self.retained_bytes.saturating_sub(command.text.len() + 1);
        Some(command)
    }

    pub(crate) fn append_queued(
        &mut self,
        command: QueuedCommand,
    ) -> Result<(), CommandBufferError> {
        if self.retained_bytes + command.text.len() + 1 > MAX_COMMAND_BUFFER_BYTES {
            return Err(CommandBufferError::Overflow(MAX_COMMAND_BUFFER_BYTES));
        }
        self.retained_bytes += command.text.len() + 1;
        self.commands.push_back(command);
        Ok(())
    }

    pub(crate) fn set_wait(&mut self, frames: u32) {
        self.wait_frames = frames;
    }

    pub(crate) fn consume_wait(&mut self) -> bool {
        if self.wait_frames == 0 {
            return false;
        }
        self.wait_frames -= 1;
        true
    }

    fn ensure_capacity(&self, commands: &[String]) -> Result<(), CommandBufferError> {
        let added = commands
            .iter()
            .map(|command| command.len() + 1)
            .sum::<usize>();
        if self.retained_bytes.saturating_add(added) > MAX_COMMAND_BUFFER_BYTES {
            return Err(CommandBufferError::Overflow(MAX_COMMAND_BUFFER_BYTES));
        }
        Ok(())
    }
}

fn script_commands(text: &str) -> Result<Vec<String>, CommandBufferError> {
    let mut commands = Vec::new();
    for line in text.lines() {
        let line = strip_line_comment(line).trim();
        if line.is_empty() {
            continue;
        }
        commands.extend(split_commands(line)?);
    }
    Ok(commands)
}

fn strip_line_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut quoted = false;
    let mut escaped = false;
    let mut index = 0;
    while index + 1 < bytes.len() {
        match bytes[index] {
            b'\\' if quoted => escaped = !escaped,
            b'"' if !escaped => quoted = !quoted,
            b'/' if !quoted && bytes[index + 1] == b'/' => return &line[..index],
            _ => escaped = false,
        }
        index += 1;
    }
    line
}

/// Failure while adding command text to the bounded queue.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandBufferError {
    /// A command contains an unterminated quoted string.
    Parse(CommandError),
    /// Aggregate buffered text exceeded the explicit safety limit.
    Overflow(usize),
    /// Nested `exec` exceeded the explicit safety depth.
    ExecDepth(u8),
}

impl Display for CommandBufferError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => Display::fmt(error, formatter),
            Self::Overflow(limit) => write!(formatter, "command buffer exceeds {limit} bytes"),
            Self::ExecDepth(limit) => write!(formatter, "exec recursion exceeds depth {limit}"),
        }
    }
}

impl std::error::Error for CommandBufferError {}

impl From<CommandError> for CommandBufferError {
    fn from(value: CommandError) -> Self {
        Self::Parse(value)
    }
}
