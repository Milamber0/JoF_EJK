//! Byte-exact `Info_SetValueForKey` / `Info_RemoveKey` from OpenJK `q_shared.c`.

/// `MAX_INFO_STRING`, terminator included: contents never reach this length.
const CAPACITY: usize = 1024;

/// A stock `\key\value` string a legacy server writes, kept as raw bytes.
///
/// Distinct from `sjk_protocol::InfoString`, which parses received UTF-8 text.
/// This one reproduces how the reference *mutates* a string, which clients and
/// master servers can observe: new keys are prepended, only the first exact-case
/// duplicate is replaced, and a pair that does not fit or contains `\`, `;` or `"`
/// is silently left out.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LegacyInfoString {
    bytes: Vec<u8>,
}

impl LegacyInfoString {
    /// An empty string with its full capacity reserved once.
    pub fn new() -> Self {
        Self {
            bytes: Vec::with_capacity(CAPACITY),
        }
    }

    /// Copy as `Q_strncpyz` does: up to the first NUL, cut to fit the buffer.
    pub fn from_truncated(source: &[u8]) -> Self {
        let mut result = Self::new();
        result.bytes.extend_from_slice(c_string(source));
        result.bytes.truncate(CAPACITY - 1);
        result
    }

    /// First value whose key matches without regard to case (`Info_ValueForKey`).
    pub fn value(&self, key: &[u8]) -> Option<&[u8]> {
        sjk_protocol::info_value(&self.bytes, key)
    }

    /// Forget everything; the capacity stays reserved.
    pub fn clear(&mut self) {
        self.bytes.clear();
    }

    /// The current contents, without a terminator.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Replace or add a pair; returns whether the value is now present.
    ///
    /// An empty value only removes the key. The old value is removed even when
    /// the new one turns out not to fit, as in the reference.
    pub fn set(&mut self, key: &[u8], value: &[u8]) -> bool {
        let (key, value) = (c_string(key), c_string(value));
        if key.iter().chain(value).any(|byte| b"\\;\"".contains(byte)) {
            return false;
        }
        self.remove(key);
        if value.is_empty() || 2 + key.len() + value.len() + self.bytes.len() >= CAPACITY {
            return false;
        }
        let pair = [b"\\", key, b"\\", value].concat();
        self.bytes.splice(0..0, pair);
        true
    }

    /// Remove the first pair whose key matches exactly, case included.
    pub fn remove(&mut self, key: &[u8]) {
        if key.contains(&b'\\') {
            return;
        }
        let mut cursor = 0;
        loop {
            let start = cursor;
            if self.bytes.get(cursor) == Some(&b'\\') {
                cursor += 1;
            }
            // A key without a value ends the scan, as the reference returns there.
            let Some(length) = self.bytes[cursor..].iter().position(|&byte| byte == b'\\') else {
                return;
            };
            let found = &self.bytes[cursor..cursor + length] == key;
            cursor += length + 1;
            cursor += self.bytes[cursor..]
                .iter()
                .position(|&byte| byte == b'\\')
                .unwrap_or(self.bytes.len() - cursor);
            if found {
                self.bytes.drain(start..cursor);
                return;
            }
            if cursor == self.bytes.len() {
                return;
            }
        }
    }
}

/// Bytes up to the first NUL, as every reference string function sees them.
pub(super) fn c_string(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len())]
}
