use std::error::Error;
use std::fmt;

/// A Quake 3-style `\key\value` information string.
///
/// Entries remain ordered because duplicate keys exist in malformed real-world
/// packets and the original engine returns the first matching entry.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InfoString {
    entries: Vec<(String, String)>,
}

impl InfoString {
    pub fn parse(input: &str) -> Result<Self, InfoStringError> {
        if input.contains('\0') {
            return Err(InfoStringError::ContainsNul);
        }

        let body = input.strip_prefix('\\').unwrap_or(input);
        if body.is_empty() {
            return Ok(Self::default());
        }

        let mut fields: Vec<&str> = body.split('\\').collect();
        // A separator after the last value (TaystJK clientinfo strings carry
        // one) leaves a dangling empty key, which `Info_ValueForKey` never
        // reaches; a separator after a key still means an empty value.
        if !fields.len().is_multiple_of(2) && fields.last() == Some(&"") {
            fields.pop();
        }
        if !fields.len().is_multiple_of(2) {
            return Err(InfoStringError::MissingValue {
                key: fields.last().copied().unwrap_or_default().to_owned(),
            });
        }

        let mut entries = Vec::with_capacity(fields.len() / 2);
        for pair in fields.chunks_exact(2) {
            if pair[0].is_empty() {
                return Err(InfoStringError::EmptyKey);
            }
            entries.push((pair[0].to_owned(), pair[1].to_owned()));
        }

        Ok(Self { entries })
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    }

    pub fn get_i32(&self, key: &str) -> Option<i32> {
        self.get(key)?.parse().ok()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InfoStringError {
    ContainsNul,
    EmptyKey,
    MissingValue { key: String },
}

impl fmt::Display for InfoStringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContainsNul => formatter.write_str("info string contains a NUL byte"),
            Self::EmptyKey => formatter.write_str("info string contains an empty key"),
            Self::MissingValue { key } => write!(formatter, "info key {key:?} has no value"),
        }
    }
}

impl Error for InfoStringError {}

/// Writing a key back into a raw info string (`Info_SetValueForKey`, `q_shared.c`): the
/// pairs are `\key\value` and a key written twice is the first one, so setting one
/// replaces it in place and adding one puts it at the end. Used wherever the game
/// rewrites a player's own string — a siege class forcing a model, a team change
/// rewriting the colours — which `InfoString` itself cannot do, being a reader.
pub fn set_value(info: &str, key: &str, value: &str) -> String {
    let mut rebuilt = String::with_capacity(info.len() + key.len() + value.len() + 2);
    let mut replaced = false;
    let mut fields = info.split('\\').filter(|field| !field.is_empty());
    while let Some(name) = fields.next() {
        let existing = fields.next().unwrap_or("");
        rebuilt.push('\\');
        rebuilt.push_str(name);
        rebuilt.push('\\');
        if name.eq_ignore_ascii_case(key) && !replaced {
            rebuilt.push_str(value);
            replaced = true;
        } else {
            rebuilt.push_str(existing);
        }
    }
    if !replaced {
        rebuilt.push('\\');
        rebuilt.push_str(key);
        rebuilt.push('\\');
        rebuilt.push_str(value);
    }
    rebuilt
}
