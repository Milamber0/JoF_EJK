//! Typed configuration-variable registry.

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::ops::{BitOr, BitOrAssign};

#[path = "cvar_aliases.rs"]
mod aliases;

/// Behavioral flags attached to a configuration variable.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CvarFlags(u8);

impl CvarFlags {
    /// No persistence or compatibility behavior.
    pub const NONE: Self = Self(0);
    /// Persist the value in the user configuration file.
    pub const ARCHIVE: Self = Self(1 << 0);
    /// Include the value when a compatibility adapter builds userinfo.
    pub const USER_INFO: Self = Self(1 << 1);
    /// Reject user-originated changes after registration.
    pub const READ_ONLY: Self = Self(1 << 2);
    /// Include this variable in serverinfo projections.
    pub const SERVER_INFO: Self = Self(1 << 3);
    /// Created by a user command/config, removable by unset/restart.
    pub const USER_CREATED: Self = Self(1 << 4);
    /// Omit an archived value from generated configuration when it equals its default.
    pub const OMIT_DEFAULT: Self = Self(1 << 5);

    /// Returns whether every bit in `other` is present.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for CvarFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for CvarFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// Strongly typed cvar value.
#[derive(Clone, Debug, PartialEq)]
pub enum CvarValue {
    /// Boolean value, serialized as `0` or `1`.
    Bool(bool),
    /// Signed integer value.
    Integer(i64),
    /// Finite floating-point value.
    Float(f64),
    /// Arbitrary text value.
    Text(String),
}

impl CvarValue {
    /// Parse text using this value's type as the schema.
    pub fn parse_like(&self, text: &str) -> Result<Self, CvarError> {
        match self {
            Self::Bool(_) => match text.to_ascii_lowercase().as_str() {
                "1" | "true" | "on" | "yes" => Ok(Self::Bool(true)),
                "0" | "false" | "off" | "no" => Ok(Self::Bool(false)),
                _ => Err(CvarError::InvalidValue(text.to_owned())),
            },
            Self::Integer(_) => text
                .parse()
                .map(Self::Integer)
                .map_err(|_| CvarError::InvalidValue(text.to_owned())),
            Self::Float(_) => text
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .map(Self::Float)
                .ok_or_else(|| CvarError::InvalidValue(text.to_owned())),
            Self::Text(_) => Ok(Self::Text(text.to_owned())),
        }
    }

    /// Stable console/config representation.
    pub fn as_text(&self) -> String {
        match self {
            Self::Bool(value) => if *value { "1" } else { "0" }.to_owned(),
            Self::Integer(value) => value.to_string(),
            Self::Float(value) => {
                // A value that came from an `f32` (a default such as 0.9_f32) is
                // printed as that `f32`'s shortest form, "0.9", not its widened
                // binary value 0.8999999761581421.
                let narrow = *value as f32;
                let mut text = if f64::from(narrow) == *value {
                    narrow.to_string()
                } else {
                    value.to_string()
                };
                if !text.contains(['.', 'e', 'E']) {
                    text.push_str(".0");
                }
                text
            }
            Self::Text(value) => value.clone(),
        }
    }

    fn same_kind(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Bool(_), Self::Bool(_))
                | (Self::Integer(_), Self::Integer(_))
                | (Self::Float(_), Self::Float(_))
                | (Self::Text(_), Self::Text(_))
        )
    }
}

impl From<bool> for CvarValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i64> for CvarValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<f64> for CvarValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<&str> for CvarValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for CvarValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

/// Static registration data for a cvar.
#[derive(Clone, Debug, PartialEq)]
pub struct CvarDefinition {
    /// Display spelling of the case-insensitive cvar name.
    pub name: String,
    /// Initial value and type schema.
    pub default: CvarValue,
    /// Persistence and compatibility behavior.
    pub flags: CvarFlags,
    /// Human-readable purpose shown by shell UIs.
    pub description: String,
}

impl CvarDefinition {
    /// Construct registration data for a typed cvar.
    pub fn new(
        name: impl Into<String>,
        default: impl Into<CvarValue>,
        flags: CvarFlags,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            default: default.into(),
            flags,
            description: description.into(),
        }
    }
}

/// A registered cvar and its current/default values.
#[derive(Clone, Debug, PartialEq)]
pub struct Cvar {
    /// Display spelling of the case-insensitive cvar name.
    pub name: String,
    /// Current value.
    pub value: CvarValue,
    /// Registered default and type schema.
    pub default: CvarValue,
    /// Persistence and compatibility behavior.
    pub flags: CvarFlags,
    /// Human-readable purpose shown by shell UIs.
    pub description: String,
}

/// Immutable change notification delivered after a successful mutation.
#[derive(Clone, Debug, PartialEq)]
pub struct CvarChange {
    /// Name of the changed cvar.
    pub name: String,
    /// Value before the mutation.
    pub previous: CvarValue,
    /// Value after the mutation.
    pub current: CvarValue,
}

type ChangeCallback = Box<dyn Fn(&CvarChange) + Send + Sync>;

struct CvarEntry {
    cvar: Cvar,
    callbacks: Vec<ChangeCallback>,
}

/// Case-insensitive typed cvar registry.
#[derive(Default)]
pub struct CvarRegistry {
    entries: BTreeMap<String, CvarEntry>,
    aliases: BTreeMap<String, String>,
    /// Stamp of the archived state ([`Self::archive_revision`]).
    revision: u64,
}

impl CvarRegistry {
    /// OpenJK cvar.cpp:1337-1377: only user-created variables may be removed.
    pub fn unset(&mut self, name: &str) -> Result<(), CvarError> {
        if let Some(cvar) = self.get(name) {
            if !cvar.flags.contains(CvarFlags::USER_CREATED) {
                return Err(CvarError::ReadOnly(name.to_owned()));
            }
        }
        let key = self.canonical_key(&name.to_ascii_lowercase()).to_owned();
        if self
            .entries
            .remove(&key)
            .is_some_and(|entry| entry.cvar.flags.contains(CvarFlags::ARCHIVE))
        {
            self.archive_changed();
        }
        self.aliases.retain(|_, target| target != &key);
        Ok(())
    }

    /// OpenJK cvar.cpp:1390-1411: discard user variables, reset writable defaults.
    pub fn restart(&mut self, only_user_created: bool) -> Result<(), CvarError> {
        self.archive_changed();
        self.entries
            .retain(|_, entry| !entry.cvar.flags.contains(CvarFlags::USER_CREATED));
        self.aliases
            .retain(|_, target| self.entries.contains_key(target));
        if !only_user_created {
            for entry in self.entries.values_mut() {
                if !entry.cvar.flags.contains(CvarFlags::READ_ONLY) {
                    set_entry(entry, entry.cvar.default.clone(), false)?;
                }
            }
        }
        Ok(())
    }
    /// Construct an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// A stamp of what the config file holds of this registry: it changes whenever
    /// an archived cvar is added, removed or changes value or flags, and never for
    /// the other cvars. Stamps are unique across registries, so an equal stamp
    /// means the same archived state.
    pub fn archive_revision(&self) -> u64 {
        self.revision
    }

    fn archive_changed(&mut self) {
        self.revision = crate::revision::next();
    }

    /// Set an entry and restamp the registry when an archived value changed.
    fn set_tracked(
        &mut self,
        name: &str,
        value: impl FnOnce(&Cvar) -> Result<CvarValue, CvarError>,
        restoring: bool,
    ) -> Result<bool, CvarError> {
        let entry = self.entry_mut(name)?;
        let value = value(&entry.cvar)?;
        let changed = set_entry(entry, value, restoring)?;
        if changed && entry.cvar.flags.contains(CvarFlags::ARCHIVE) {
            self.archive_changed();
        }
        Ok(changed)
    }

    /// Register a cvar, rejecting duplicate or invalid names.
    pub fn register(&mut self, definition: CvarDefinition) -> Result<(), CvarError> {
        let key = normalize_name(&definition.name)?;
        if self.entries.contains_key(&key) || self.aliases.contains_key(&key) {
            return Err(CvarError::AlreadyRegistered(definition.name));
        }
        if definition.flags.contains(CvarFlags::ARCHIVE) {
            self.archive_changed();
        }
        self.entries.insert(
            key,
            CvarEntry {
                cvar: Cvar {
                    name: definition.name,
                    value: definition.default.clone(),
                    default: definition.default,
                    flags: definition.flags,
                    description: definition.description,
                },
                callbacks: Vec::new(),
            },
        );
        Ok(())
    }

    /// Look up a cvar case-insensitively.
    pub fn get(&self, name: &str) -> Option<&Cvar> {
        // Runtime consumers use normalized names; do not allocate on every frame read.
        if !name.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return self
                .entries
                .get(self.canonical_key(name))
                .map(|entry| &entry.cvar);
        }
        self.entries
            .get(self.canonical_key(&name.to_ascii_lowercase()))
            .map(|entry| &entry.cvar)
    }

    /// Iterate cvars in deterministic normalized-name order.
    pub fn iter(&self) -> impl Iterator<Item = &Cvar> {
        self.entries.values().map(|entry| &entry.cvar)
    }

    /// Parse and set a cvar according to its registered type.
    pub fn set_text(&mut self, name: &str, value: &str) -> Result<bool, CvarError> {
        self.set_tracked(name, |cvar| cvar.default.parse_like(value), false)
    }

    /// Set an already-typed value, rejecting a different value kind.
    pub fn set_value(&mut self, name: &str, value: CvarValue) -> Result<bool, CvarError> {
        self.set_tracked(name, |_| Ok(value), false)
    }

    /// Add behavioral flags to an existing cvar.
    pub fn add_flags(&mut self, name: &str, flags: CvarFlags) -> Result<(), CvarError> {
        let cvar = &mut self.entry_mut(name)?.cvar;
        let before = cvar.flags;
        cvar.flags |= flags;
        // Archiving or omitting a default changes what the config file holds.
        if cvar.flags != before {
            self.archive_changed();
        }
        Ok(())
    }

    /// Apply persisted state, including read-only startup values.
    pub fn restore_text(&mut self, name: &str, value: &str) -> Result<bool, CvarError> {
        self.set_tracked(name, |cvar| cvar.default.parse_like(value), true)
    }

    /// Restore the registered default value.
    pub fn reset(&mut self, name: &str) -> Result<bool, CvarError> {
        self.set_tracked(name, |cvar| Ok(cvar.default.clone()), false)
    }

    /// Register a callback invoked synchronously after each effective change.
    pub fn on_change(
        &mut self,
        name: &str,
        callback: impl Fn(&CvarChange) + Send + Sync + 'static,
    ) -> Result<(), CvarError> {
        self.entry_mut(name)?.callbacks.push(Box::new(callback));
        Ok(())
    }
}

fn set_entry(entry: &mut CvarEntry, value: CvarValue, restoring: bool) -> Result<bool, CvarError> {
    if !entry.cvar.default.same_kind(&value) {
        return Err(CvarError::WrongType(entry.cvar.name.clone()));
    }
    if !restoring && entry.cvar.flags.contains(CvarFlags::READ_ONLY) {
        return Err(CvarError::ReadOnly(entry.cvar.name.clone()));
    }
    if entry.cvar.value == value {
        return Ok(false);
    }
    let change = CvarChange {
        name: entry.cvar.name.clone(),
        previous: std::mem::replace(&mut entry.cvar.value, value.clone()),
        current: value,
    };
    for callback in &entry.callbacks {
        callback(&change);
    }
    Ok(true)
}

fn normalize_name(name: &str) -> Result<String, CvarError> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
    {
        return Err(CvarError::InvalidName(name.to_owned()));
    }
    Ok(name.to_ascii_lowercase())
}

/// Failure while registering, parsing, or mutating a cvar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CvarError {
    /// The name contains unsupported characters or is empty.
    InvalidName(String),
    /// The normalized name is already registered.
    AlreadyRegistered(String),
    /// The requested name is not registered.
    Unknown(String),
    /// Text could not be parsed as the registered type.
    InvalidValue(String),
    /// A typed mutation used a different value kind.
    WrongType(String),
    /// User mutation was attempted on a read-only cvar.
    ReadOnly(String),
}

impl Display for CvarError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName(name) => write!(formatter, "invalid cvar name {name:?}"),
            Self::AlreadyRegistered(name) => {
                write!(formatter, "cvar {name:?} is already registered")
            }
            Self::Unknown(name) => write!(formatter, "unknown cvar {name:?}"),
            Self::InvalidValue(value) => write!(formatter, "invalid cvar value {value:?}"),
            Self::WrongType(name) => write!(formatter, "wrong value type for cvar {name:?}"),
            Self::ReadOnly(name) => write!(formatter, "cvar {name:?} is read-only"),
        }
    }
}

impl std::error::Error for CvarError {}

#[cfg(test)]
mod float_text_tests {
    use super::CvarValue;

    #[test]
    fn floats_from_f32_print_their_shortest_form() {
        assert_eq!(CvarValue::Float(f64::from(0.9_f32)).as_text(), "0.9");
        assert_eq!(CvarValue::Float(f64::from(0.75_f32)).as_text(), "0.75");
        assert_eq!(CvarValue::Float(3.0).as_text(), "3.0");
        // A value no f32 holds keeps its full form.
        assert_eq!(CvarValue::Float(0.1).as_text(), "0.1");
        assert_eq!(CvarValue::Float(1.0 / 3.0).as_text(), "0.3333333333333333");
    }
}
