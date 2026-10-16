//! The game's text parser (`codemp/qcommon/q_shared.c`): `COM_Compress`, `COM_ParseExt`,
//! `SkipBracedSection`, `SkipRestOfLine` and the number readings of `COM_ParseInt` and
//! `COM_ParseFloat`, byte for byte, quirks included. The definition files the game reads
//! (sabers first) are parsed with these, and what a file means depends on them: a
//! keyword's value missing from its line is read from the next one, or skips it.
//!
//! The text is treated as ending at its first NUL, as the C string it is in the game.

/// `MAX_TOKEN_CHARS`: a token keeps at most one byte less than this.
const MAX_TOKEN_CHARS: usize = 1024;

/// `COM_Compress`: comments dropped, each run of line breaks and blanks before a token
/// made one line break (or one space), quoted strings kept whole.
pub fn compress(text: &[u8]) -> Vec<u8> {
    let text = until_nul(text);
    let at = |index: usize| text.get(index).copied().unwrap_or(0);
    let mut out = Vec::with_capacity(text.len());
    let (mut newline, mut whitespace) = (false, false);
    let mut index = 0;
    while index < text.len() {
        let c = text[index];
        if c == b'/' && at(index + 1) == b'/' {
            while index < text.len() && text[index] != b'\n' {
                index += 1;
            }
        } else if c == b'/' && at(index + 1) == b'*' {
            while index < text.len() && !(text[index] == b'*' && at(index + 1) == b'/') {
                index += 1;
            }
            if index < text.len() {
                index += 2;
            }
        } else if c == b'\n' || c == b'\r' {
            newline = true;
            index += 1;
        } else if c == b' ' || c == b'\t' {
            whitespace = true;
            index += 1;
        } else {
            // A pending line break counts as the blank too.
            if newline {
                out.push(b'\n');
                newline = false;
                whitespace = false;
            }
            if whitespace {
                out.push(b' ');
                whitespace = false;
            }
            if c == b'"' {
                out.push(c);
                index += 1;
                while index < text.len() && text[index] != b'"' {
                    out.push(text[index]);
                    index += 1;
                }
                if index < text.len() {
                    out.push(b'"');
                    index += 1;
                }
            } else {
                out.push(c);
                index += 1;
            }
        }
    }
    out
}

/// The text up to its first NUL.
pub fn until_nul(text: &[u8]) -> &[u8] {
    &text[..text
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(text.len())]
}

/// A read position in a text, as the game's `const char *` is: `None` once a read ran off
/// the end (the pointer the game sets to `NULL`).
#[derive(Clone, Copy, Debug)]
pub struct TextParser<'a> {
    text: &'a [u8],
    at: Option<usize>,
}

impl<'a> TextParser<'a> {
    /// A parser at the start of `text` (read up to its first NUL).
    pub fn new(text: &'a [u8]) -> Self {
        Self {
            text: until_nul(text),
            at: Some(0),
        }
    }

    /// Whether the position is still in the text (`p != NULL`).
    pub fn is_live(&self) -> bool {
        self.at.is_some()
    }

    /// Back to the start, as the game sets `p` to the buffer again.
    pub fn restart(&mut self) {
        self.at = Some(0);
    }

    fn byte(&self, index: usize) -> u8 {
        self.text.get(index).copied().unwrap_or(0)
    }

    /// `COM_ParseExt`: the next token, empty at the end. Without `allow_line_breaks`, a
    /// line break before the next token answers empty and leaves the position at that
    /// token, past the break.
    pub fn parse_ext(&mut self, allow_line_breaks: bool) -> &'a [u8] {
        let Some(mut at) = self.at else { return b"" };
        let mut has_newlines = false;
        let c = loop {
            // `SkipWhitespace`: every byte up to the space, unsigned.
            loop {
                match self.byte(at) {
                    0 => {
                        self.at = None;
                        return b"";
                    }
                    byte if byte <= b' ' => {
                        has_newlines |= byte == b'\n';
                        at += 1;
                    }
                    _ => break,
                }
            }
            if has_newlines && !allow_line_breaks {
                self.at = Some(at);
                return b"";
            }
            let c = self.byte(at);
            if c == b'/' && self.byte(at + 1) == b'/' {
                at += 2;
                while !matches!(self.byte(at), 0 | b'\n') {
                    at += 1;
                }
            } else if c == b'/' && self.byte(at + 1) == b'*' {
                at += 2;
                while self.byte(at) != 0 && !(self.byte(at) == b'*' && self.byte(at + 1) == b'/') {
                    at += 1;
                }
                if self.byte(at) != 0 {
                    at += 2;
                }
            } else {
                break c;
            }
        };
        if c == b'"' {
            let start = at + 1;
            let mut end = start;
            while !matches!(self.byte(end), 0 | b'"') {
                end += 1;
            }
            // Past the closing quote, or past the end's NUL, as `*data++` leaves it.
            self.at = Some(end + 1);
            return &self.text[start..end.min(start + MAX_TOKEN_CHARS - 1)];
        }
        // A word runs while its bytes, read signed, are above the space: a byte from 128
        // up ends it (though one may start it).
        let start = at;
        loop {
            at += 1;
            if (self.byte(at) as i8) <= 32 {
                break;
            }
        }
        self.at = Some(at);
        &self.text[start..at.min(start + MAX_TOKEN_CHARS - 1)]
    }

    /// `SkipBracedSection`: tokens read until the braces opened (`depth` of them already)
    /// close; with no brace open, a single token. Whether they closed.
    pub fn skip_braced_section(&mut self, mut depth: i32) -> bool {
        loop {
            match self.parse_ext(true) {
                b"{" => depth += 1,
                b"}" => depth -= 1,
                _ => {}
            }
            if depth == 0 || !self.is_live() {
                return depth == 0;
            }
        }
    }

    /// `SkipRestOfLine`: past the next line break. Nothing at the end of the text.
    pub fn skip_rest_of_line(&mut self) {
        let Some(mut at) = self.at else { return };
        if self.byte(at) == 0 {
            return;
        }
        loop {
            let c = self.byte(at);
            at += 1;
            if c == 0 || c == b'\n' {
                break;
            }
        }
        self.at = Some(at);
    }

    /// `COM_ParseString`: the next token on this line, which it never fails to give
    /// (empty past the line's end).
    pub fn parse_string(&mut self) -> &'a [u8] {
        self.parse_ext(false)
    }

    /// `COM_ParseInt`: the next token on this line read by `atoi`; `None` without one.
    pub fn parse_int(&mut self) -> Option<i32> {
        let token = self.parse_ext(false);
        (!token.is_empty()).then(|| crate::userinfo::atoi(token))
    }

    /// `COM_ParseFloat`: the next token on this line read by `atof`; `None` without one.
    pub fn parse_float(&mut self) -> Option<f32> {
        let token = self.parse_ext(false);
        (!token.is_empty()).then(|| atof(token))
    }

    /// `BG_ParseLiteral`: whether the next token is `literal` (any case).
    pub fn parse_literal(&mut self, literal: &[u8]) -> bool {
        let token = self.parse_ext(true);
        !token.is_empty() && token.eq_ignore_ascii_case(literal)
    }
}

/// `atof`: the longest leading decimal number (sign, digits, point, exponent), rounded
/// to a double and then to a float as the game stores it; zero without one. Hexadecimal
/// numbers and `inf`/`nan` are not read.
pub fn atof(text: &[u8]) -> f32 {
    let text = text.trim_ascii_start();
    let digits_from = |from: usize| {
        text[from.min(text.len())..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count()
    };
    let mut end = usize::from(matches!(text.first(), Some(b'+' | b'-')));
    let whole = digits_from(end);
    end += whole;
    let mut fraction = 0;
    if text.get(end) == Some(&b'.') {
        fraction = digits_from(end + 1);
        end += 1 + fraction;
    }
    if whole + fraction == 0 {
        return 0.0;
    }
    if matches!(text.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(text.get(end + 1), Some(b'+' | b'-')));
        let exponent = digits_from(end + 1 + sign);
        if exponent > 0 {
            end += 1 + sign + exponent;
        }
    }
    std::str::from_utf8(&text[..end])
        .ok()
        .and_then(|number| number.parse::<f64>().ok())
        .map_or(0.0, |value| value as f32)
}
