use crate::{BindError, CommandBufferError, CommandError, ConfigError, CvarError};
use std::fmt::{Display, Formatter};

/// Failure while operating the integrated shell.
#[derive(Debug)]
pub enum ShellError {
    /// Command parsing or handler failure.
    Command(CommandError),
    /// Cvar lookup, parsing, or mutation failure.
    Cvar(CvarError),
    /// Binding creation or resolution failure.
    Bind(BindError),
    /// Configuration persistence failure.
    Config(ConfigError),
    /// Neither a built-in, handler, nor cvar matched the input.
    UnknownCommand(String),
    /// `toggle` targeted a text cvar.
    NotToggleable(String),
    /// A built-in command received an invalid argument shape.
    Usage(&'static str),
    /// Persistence was requested before selecting a config path.
    NoConfigPath,
    /// A previous load failed; saving partial state would destroy unread settings.
    ConfigLoadFailed,
    /// Application-local interception rejected a buffered command.
    Application(String),
    /// The application file resolver failed.
    ScriptResolver(String),
    /// An `exec` script was absent from every configured source.
    ScriptNotFound(String),
    /// Command buffering exceeded its explicit safety bound.
    CommandBuffer(CommandBufferError),
}

impl Display for ShellError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Command(error) => Display::fmt(error, formatter),
            Self::Cvar(error) => Display::fmt(error, formatter),
            Self::Bind(error) => Display::fmt(error, formatter),
            Self::Config(error) => Display::fmt(error, formatter),
            Self::UnknownCommand(name) => write!(formatter, "unknown command {name:?}"),
            Self::NotToggleable(name) => write!(formatter, "cvar {name:?} is not numeric"),
            Self::Usage(usage) => write!(formatter, "usage: {usage}"),
            Self::NoConfigPath => formatter.write_str("no user config path is configured"),
            Self::ConfigLoadFailed => formatter.write_str(
                "config was not fully loaded; fix the reported error and reload before saving",
            ),
            Self::Application(message) | Self::ScriptResolver(message) => {
                formatter.write_str(message)
            }
            Self::ScriptNotFound(path) => write!(formatter, "couldn't exec {path}"),
            Self::CommandBuffer(error) => Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for ShellError {}

impl From<CommandError> for ShellError {
    fn from(value: CommandError) -> Self {
        Self::Command(value)
    }
}

impl From<CvarError> for ShellError {
    fn from(value: CvarError) -> Self {
        Self::Cvar(value)
    }
}

impl From<BindError> for ShellError {
    fn from(value: BindError) -> Self {
        Self::Bind(value)
    }
}

impl From<ConfigError> for ShellError {
    fn from(value: ConfigError) -> Self {
        Self::Config(value)
    }
}

impl From<CommandBufferError> for ShellError {
    fn from(value: CommandBufferError) -> Self {
        Self::CommandBuffer(value)
    }
}
