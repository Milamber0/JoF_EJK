//! Canonical aliases share storage, callbacks, flags and config serialization.
use super::*;

impl CvarRegistry {
    /// Add a compatibility spelling for an existing registered variable.
    /// Aliases are flattened and never appear as duplicate entries in `iter`.
    pub fn register_alias(&mut self, alias: &str, target: &str) -> Result<(), CvarError> {
        let alias = normalize_name(alias)?;
        let target = normalize_name(target)?;
        let target = self.canonical_key(&target).to_owned();
        if !self.entries.contains_key(&target) {
            return Err(CvarError::Unknown(target));
        }
        if self.entries.contains_key(&alias) || self.aliases.contains_key(&alias) {
            return Err(CvarError::AlreadyRegistered(alias));
        }
        self.aliases.insert(alias, target);
        Ok(())
    }

    /// Resolve a normalized key without allocating on frame reads.
    pub(super) fn canonical_key<'a>(&'a self, normalized: &'a str) -> &'a str {
        self.aliases
            .get(normalized)
            .map_or(normalized, String::as_str)
    }

    /// Resolve an arbitrary spelling for an infrequent mutation.
    pub(super) fn entry_mut(&mut self, name: &str) -> Result<&mut CvarEntry, CvarError> {
        let normalized = name.to_ascii_lowercase();
        let key = self.aliases.get(&normalized).unwrap_or(&normalized);
        self.entries
            .get_mut(key)
            .ok_or_else(|| CvarError::Unknown(name.to_owned()))
    }
}
