//! Console command parsing and handler dispatch.

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

/// Result lines produced by a command handler.
pub type CommandResult = Result<Vec<String>, CommandError>;
type CommandHandler = Box<dyn FnMut(&[String]) -> CommandResult + Send>;

struct RegisteredCommand {
    name: String,
    description: String,
    handler: CommandHandler,
}

/// Case-insensitive registry for application and mod commands.
#[derive(Default)]
pub struct CommandRegistry {
    commands: BTreeMap<String, RegisteredCommand>,
}

impl CommandRegistry {
    /// Construct an empty command registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a named handler and help description.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        handler: impl FnMut(&[String]) -> CommandResult + Send + 'static,
    ) -> Result<(), CommandError> {
        let name = name.into();
        let key = normalize_name(&name)?;
        if self.commands.contains_key(&key) {
            return Err(CommandError::AlreadyRegistered(name));
        }
        self.commands.insert(
            key,
            RegisteredCommand {
                name,
                description: description.into(),
                handler: Box::new(handler),
            },
        );
        Ok(())
    }

    /// Dispatch a command case-insensitively, or return `None` if unknown.
    pub fn dispatch(&mut self, name: &str, arguments: &[String]) -> Option<CommandResult> {
        self.commands
            .get_mut(&name.to_ascii_lowercase())
            .map(|command| (command.handler)(arguments))
    }

    /// Remove a handler; returns whether `name` was registered.
    pub fn unregister(&mut self, name: &str) -> bool {
        self.commands.remove(&name.to_ascii_lowercase()).is_some()
    }

    /// Return whether a handler is registered for `name`.
    pub fn contains(&self, name: &str) -> bool {
        self.commands.contains_key(&name.to_ascii_lowercase())
    }

    /// Iterate registered names and descriptions in deterministic order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.commands
            .values()
            .map(|command| (command.name.as_str(), command.description.as_str()))
    }
}

fn normalize_name(name: &str) -> Result<String, CommandError> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'+' | b'-'))
    {
        return Err(CommandError::InvalidName(name.to_owned()));
    }
    Ok(name.to_ascii_lowercase())
}

/// Split a command line into scripts at unquoted semicolons.
pub fn split_commands(input: &str) -> Result<Vec<String>, CommandError> {
    let input = crate::key_names::normalize_stock_backslash(input);
    let mut commands = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in input.chars() {
        if escaped {
            current.push(character);
            escaped = false;
        } else if character == '\\' && quoted {
            current.push(character);
            escaped = true;
        } else if character == '"' {
            current.push(character);
            quoted = !quoted;
        } else if character == ';' && !quoted {
            if !current.trim().is_empty() {
                commands.push(current.trim().to_owned());
            }
            current.clear();
        } else {
            current.push(character);
        }
    }
    if quoted || escaped {
        return Err(CommandError::UnterminatedQuote);
    }
    if !current.trim().is_empty() {
        commands.push(current.trim().to_owned());
    }
    Ok(commands)
}

/// Tokenize one console command with quoted strings and backslash escapes.
pub fn tokenize(input: &str) -> Result<Vec<String>, CommandError> {
    let input = crate::key_names::normalize_stock_backslash(input);
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    let mut started = false;
    for character in input.chars() {
        if escaped {
            current.push(match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escaped = false;
            started = true;
        } else if character == '\\' && quoted {
            escaped = true;
            started = true;
        } else if character == '"' {
            quoted = !quoted;
            started = true;
        } else if character.is_whitespace() && !quoted {
            if started {
                tokens.push(std::mem::take(&mut current));
                started = false;
            }
        } else {
            current.push(character);
            started = true;
        }
    }
    if quoted || escaped {
        return Err(CommandError::UnterminatedQuote);
    }
    if started {
        tokens.push(current);
    }
    Ok(tokens)
}

/// Failure while parsing, registering, or running a command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandError {
    /// The command name is empty or contains unsupported characters.
    InvalidName(String),
    /// A command with the normalized name is already registered.
    AlreadyRegistered(String),
    /// A quoted token or escape was not terminated.
    UnterminatedQuote,
    /// The handler rejected its arguments.
    InvalidArguments(String),
    /// The handler failed while performing its command.
    Handler(String),
}

impl Display for CommandError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName(name) => write!(formatter, "invalid command name {name:?}"),
            Self::AlreadyRegistered(name) => {
                write!(formatter, "command {name:?} is already registered")
            }
            Self::UnterminatedQuote => formatter.write_str("unterminated quoted string"),
            Self::InvalidArguments(message) | Self::Handler(message) => {
                formatter.write_str(message)
            }
        }
    }
}

impl std::error::Error for CommandError {}
