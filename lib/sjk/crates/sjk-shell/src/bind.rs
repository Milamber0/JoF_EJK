//! Physical-input to console-command bindings.

use crate::CommandError;
use crate::command::split_commands;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

/// Case-insensitive stable key binding table.
#[derive(Clone, Debug, Default)]
pub struct BindTable {
    bindings: BTreeMap<String, Binding>,
    /// Stamp of the bindings ([`Self::revision`]); not part of equality.
    revision: u64,
}

impl PartialEq for BindTable {
    fn eq(&self, other: &Self) -> bool {
        self.bindings == other.bindings
    }
}

impl Eq for BindTable {}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Binding {
    key: String,
    command: String,
}

impl BindTable {
    /// Construct an empty binding table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind a normalized physical key name, or clear it with an empty script.
    pub fn bind(&mut self, key: &str, command: impl Into<String>) -> Result<(), BindError> {
        let normalized = normalize_key(key)?;
        let command = command.into();
        if command.is_empty() {
            if self.bindings.remove(&normalized).is_some() {
                self.revision = crate::revision::next();
            }
            return Ok(());
        }
        let binding = Binding {
            key: crate::key_names::canonical_key(key).unwrap().to_owned(),
            command,
        };
        // A script that rebinds a key to what it already runs changes nothing saved.
        if self.bindings.get(&normalized) != Some(&binding) {
            self.bindings.insert(normalized, binding);
            self.revision = crate::revision::next();
        }
        Ok(())
    }

    /// Remove a binding case-insensitively.
    pub fn unbind(&mut self, key: &str) -> bool {
        let removed = normalize_key(key).is_ok_and(|key| self.bindings.remove(&key).is_some());
        if removed {
            self.revision = crate::revision::next();
        }
        removed
    }

    /// Remove every binding.
    pub fn clear(&mut self) {
        self.revision = crate::revision::next();
        self.bindings.clear();
    }

    /// A stamp of the bindings: it changes with every bind, unbind or clear, so
    /// an equal stamp means the same bindings. Stamps are unique across tables;
    /// every new table starts empty at 0.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Return a key's command script case-insensitively.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.bindings
            .get(&normalize_key(key).ok()?)
            .map(|binding| binding.command.as_str())
    }

    /// Iterate bindings in deterministic normalized-key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.bindings
            .values()
            .map(|binding| (binding.key.as_str(), binding.command.as_str()))
    }

    /// Resolve press/release commands using Quake-style `+button` semantics.
    pub fn commands_for_event(&self, key: &str, pressed: bool) -> Result<Vec<String>, BindError> {
        let Some(command) = self.get(key) else {
            return Ok(Vec::new());
        };
        let commands = split_commands(command).map_err(BindError::Command)?;
        if pressed {
            return Ok(commands);
        }
        Ok(commands
            .into_iter()
            .filter_map(|command| {
                let command = command.trim();
                command.strip_prefix('+').map(|rest| format!("-{rest}"))
            })
            .collect())
    }
}

fn normalize_key(key: &str) -> Result<String, BindError> {
    crate::key_names::canonical_key(key)
        .map(str::to_lowercase)
        .ok_or_else(|| BindError::InvalidKey(key.to_owned()))
}

/// Failure while creating or resolving a key binding.
#[derive(Debug)]
pub enum BindError {
    /// The key name is empty or contains unsupported characters.
    InvalidKey(String),
    /// A binding was given no command.
    EmptyCommand,
    /// A bound command script could not be parsed.
    Command(CommandError),
}

impl Display for BindError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidKey(key) => write!(formatter, "invalid bind key {key:?}"),
            Self::EmptyCommand => formatter.write_str("bind command cannot be empty"),
            Self::Command(error) => Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for BindError {}

#[cfg(test)]
mod tests {
    use super::BindTable;

    #[test]
    fn layout_characters_bind_and_resolve() {
        let mut binds = BindTable::new();
        binds.bind("é", "+attack").unwrap();
        binds.bind("^", "+use").unwrap();
        binds.bind("<", "+speed").unwrap();
        binds.bind("Ù", "+back").unwrap();
        assert_eq!(binds.get("é"), Some("+attack"));
        assert_eq!(binds.get("É"), Some("+attack"));
        assert_eq!(binds.get("^"), Some("+use"));
        assert_eq!(binds.get("<"), Some("+speed"));
        assert_eq!(binds.get("ù"), Some("+back"));
    }

    #[test]
    fn saved_names_keep_their_meaning() {
        let mut binds = BindTable::new();
        binds.bind("W", "+forward").unwrap();
        binds.bind("SEMICOLON", "+left").unwrap();
        assert_eq!(binds.get("w"), Some("+forward"));
        assert_eq!(binds.get(";"), Some("+left"));
        assert!(binds.bind("éé", "+attack").is_err());
        assert!(binds.bind("\u{7f}", "+attack").is_err());
    }
}
