//! `MSG_ReadStringLine` and `Cmd_TokenizeString` for out-of-band request lines.

/// `MAX_STRING_CHARS - 1`: the longest line the reference reads from a datagram.
const MAX_LINE: usize = 1023;

/// The first line of an out-of-band datagram, normalized as the reference reads it.
///
/// Reading stops at a NUL, a newline or 1,023 bytes, and every `%` becomes `.` so
/// request text can never act as a format string. Held by the caller so that
/// parsing a datagram allocates nothing.
pub struct LegacyOobLine {
    bytes: [u8; MAX_LINE],
    length: usize,
}

impl Default for LegacyOobLine {
    fn default() -> Self {
        Self {
            bytes: [0; MAX_LINE],
            length: 0,
        }
    }
}

impl LegacyOobLine {
    /// Replace the contents with the first line of `parts` read back to back.
    pub fn read(&mut self, parts: &[&[u8]]) {
        self.length = 0;
        for &byte in parts.iter().flat_map(|part| part.iter()).take(MAX_LINE) {
            if byte == 0 || byte == b'\n' {
                break;
            }
            self.bytes[self.length] = if byte == b'%' { b'.' } else { byte };
            self.length += 1;
        }
    }

    /// The normalized line.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
}

/// Arguments of a command line, split as the reference's tokenizer splits them.
///
/// Bytes up to and including a space separate arguments, `//` ends the line, a
/// `/* */` block is skipped (an unclosed one ends the line), and a quoted argument
/// runs to the next quote or the end without any escape. A quote also ends an
/// unquoted argument. Bytes above 127 are ordinary argument bytes.
pub struct LegacyTokens<'a>(&'a [u8]);

impl<'a> LegacyTokens<'a> {
    /// Tokenize a line already normalized by [`LegacyOobLine`].
    pub fn new(line: &'a [u8]) -> Self {
        Self(line)
    }

    /// Everything not yet consumed, exactly as it appears in the line.
    pub fn rest(&self) -> &'a [u8] {
        self.0
    }
}

impl<'a> Iterator for LegacyTokens<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        let mut bytes = self.0;
        loop {
            let skip = bytes
                .iter()
                .position(|&byte| byte > b' ')
                .unwrap_or(bytes.len());
            bytes = &bytes[skip..];
            if bytes.is_empty() || bytes.starts_with(b"//") {
                self.0 = &[];
                return None;
            }
            let Some(comment) = bytes.strip_prefix(b"/*") else {
                break;
            };
            let Some(end) = comment.windows(2).position(|pair| pair == b"*/") else {
                self.0 = &[];
                return None;
            };
            bytes = &comment[end + 2..];
        }
        if let Some(quoted) = bytes.strip_prefix(b"\"") {
            let end = quoted.iter().position(|&byte| byte == b'"');
            self.0 = end.map_or(&[][..], |end| &quoted[end + 1..]);
            return Some(&quoted[..end.unwrap_or(quoted.len())]);
        }
        let end = (0..bytes.len())
            .find(|&i| {
                bytes[i] <= b' '
                    || bytes[i] == b'"'
                    || bytes[i..].starts_with(b"//")
                    || bytes[i..].starts_with(b"/*")
            })
            .unwrap_or(bytes.len());
        self.0 = &bytes[end..];
        Some(&bytes[..end])
    }
}
