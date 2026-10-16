//! Borrowed CS_PLAYERS fields; malformed name bytes cannot hide model/team fields.

/// A current clientinfo view, borrowed directly from the authoritative configstring.
#[derive(Clone, Copy)]
pub struct LegacyClientInfo<'a>(&'a [u8]);

impl<'a> LegacyClientInfo<'a> {
    /// Inspect one configstring without allocating or interpreting the wire codec.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }

    /// Stock case-insensitive Info_ValueForKey, with trailing separators tolerated.
    pub fn bytes(self, key: &str) -> Option<&'a [u8]> {
        let mut fields = self
            .0
            .strip_prefix(b"\\")
            .unwrap_or(self.0)
            .split(|b| *b == b'\\');
        while let (Some(k), Some(v)) = (fields.next(), fields.next()) {
            if k.eq_ignore_ascii_case(key.as_bytes()) {
                return Some(v);
            }
        }
        None
    }

    /// Decode only the requested field, not unrelated player-name bytes.
    pub fn text(self, key: &str) -> Option<&'a str> {
        std::str::from_utf8(self.bytes(key)?).ok()
    }

    /// Integer metadata such as t, hc, skill, w/l, tt/tl and dt.
    pub fn integer(self, key: &str) -> Option<i32> {
        self.text(key)?.parse().ok()
    }

    /// The player's name (`n`, else `name`) with its colour codes, for display.
    ///
    /// Names are legacy bytes: a byte above 0x7F is the Latin-1 character of that value
    /// ([`crate::decode_legacy`]), as the server keeps it (`ClientCleanName` drops only
    /// control bytes and a few unused Windows-1252 ones). `None` when the name is
    /// missing or empty.
    pub fn name(self) -> Option<std::borrow::Cow<'a, str>> {
        let name = self.bytes("n").or_else(|| self.bytes("name"))?;
        (!name.is_empty()).then(|| crate::decode_legacy(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_latin1_name_is_read_from_bytes_that_are_not_utf8() {
        let info = LegacyClientInfo::new(b"n\\K\xe9k\xe9\\t\\1\\model\\kyle\\ds\\f");
        assert_eq!(info.name().as_deref(), Some("Kéké"));
        assert_eq!(info.bytes("ds"), Some(&b"f"[..]));
        let clan = LegacyClientInfo::new(b"\\n\\{JoF}^1emiah{I}\\t\\0");
        assert_eq!(clan.name().as_deref(), Some("{JoF}^1emiah{I}"));
        assert_eq!(LegacyClientInfo::new(b"n\\\\t\\0").name(), None);
        assert_eq!(LegacyClientInfo::new(b"").name(), None);
    }
}
