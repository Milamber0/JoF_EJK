//! Encoding for player-authored text the client sends to a server.
//!
//! JKA is a pre-Unicode engine: names, chat and console commands travel as byte
//! strings that every client draws through a 256-glyph codepage font. The retail
//! client is a Win32 ANSI program, so typed text reaches it as `WM_CHAR` bytes of the
//! Windows code page (Windows-1252 on Western systems) and leaves for the server as
//! those bytes. EternalJK also sends one byte per character
//! (`ConvertUTF32ToExpectedCharset`, `shared/sdl/sdl_input.cpp`). Sending a Rust
//! string's UTF-8 instead made `ø` arrive as `Ã¸` on those clients.
//!
//! Russian players use Windows-1251 the same way: EternalJK maps typed `А`..`я` to
//! the bytes 0xC0..=0xFF of that code page, and its players with `se_language
//! russian` draw those bytes with a Cyrillic font (`charsgrid_med_cyr`,
//! `CVU_LanguageModified` in `cg_cvar.c`). The bytes alone cannot say which code
//! page they are in, so [`LegacyCharset`] decides: the player's choice, or by
//! default a guess from the text ([`looks_cyrillic`]).
//!
//! Incoming text that is not Cyrillic is decoded as Latin-1
//! (`sjk_client::decode_legacy`), so a byte in 0x80..=0x9F reaches the client as the
//! C1 character of the same value, and the renderer draws the font glyph of that
//! byte. Those characters go back as the same byte, which keeps a name or line
//! copied from the game byte-exact.

use std::borrow::Cow;
use std::sync::atomic::{AtomicU8, Ordering};

/// Characters Windows-1252 assigns to bytes 0x80..=0x9F. The five bytes it leaves
/// undefined hold the C1 character of the same value, as `MultiByteToWideChar` maps
/// them; only characters above U+00FF are looked up here.
const WINDOWS_1252_HIGH: [char; 32] = [
    '\u{20ac}', '\u{81}', '\u{201a}', '\u{192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{2c6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8d}', '\u{17d}', '\u{8f}',
    '\u{90}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{2dc}', '\u{2122}', '\u{161}', '\u{203a}', '\u{153}', '\u{9d}', '\u{17e}', '\u{178}',
];

/// Characters Windows-1251 assigns to bytes 0x80..=0xBF (`MultiByteToWideChar` with
/// code page 1251; the undefined 0x98 holds its C1 character). Bytes 0xC0..=0xFF are
/// `А`..`я`, U+0410..=U+044F in order.
const WINDOWS_1251_HIGH: [char; 64] = [
    '\u{402}', '\u{403}', '\u{201a}', '\u{453}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{20ac}', '\u{2030}', '\u{409}', '\u{2039}', '\u{40a}', '\u{40c}', '\u{40b}', '\u{40f}',
    '\u{452}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{98}', '\u{2122}', '\u{459}', '\u{203a}', '\u{45a}', '\u{45c}', '\u{45b}', '\u{45f}',
    '\u{a0}', '\u{40e}', '\u{45e}', '\u{408}', '\u{a4}', '\u{490}', '\u{a6}', '\u{a7}', '\u{401}',
    '\u{a9}', '\u{404}', '\u{ab}', '\u{ac}', '\u{ad}', '\u{ae}', '\u{407}', '\u{b0}', '\u{b1}',
    '\u{406}', '\u{456}', '\u{491}', '\u{b5}', '\u{b6}', '\u{b7}', '\u{451}', '\u{2116}',
    '\u{454}', '\u{bb}', '\u{458}', '\u{405}', '\u{455}', '\u{457}',
];

/// The single-byte code page legacy text that is not UTF-8 is read and written in.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LegacyCharset {
    /// Windows-1252, unless the bytes read as Russian ([`looks_cyrillic`]); Cyrillic
    /// leaves in Windows-1251 when it reads back as Russian, else as UTF-8.
    #[default]
    Auto,
    /// Always Windows-1252 (Latin-1): SJK neither reads nor writes Windows-1251.
    Western,
    /// Always Windows-1251, as an EternalJK player with `se_language russian` reads it.
    Cyrillic,
}

impl LegacyCharset {
    const fn code(self) -> u8 {
        match self {
            Self::Auto => 0,
            Self::Western => 1,
            Self::Cyrillic => 2,
        }
    }

    const fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Western,
            2 => Self::Cyrillic,
            _ => Self::Auto,
        }
    }
}

/// The player's [`LegacyCharset`], read by every decode and encode.
static CHARSET: AtomicU8 = AtomicU8::new(0);

/// Set the code page legacy text is read and written in (the client's `cl_charset`).
pub fn set_legacy_charset(charset: LegacyCharset) {
    CHARSET.store(charset.code(), Ordering::Relaxed);
}

/// The code page legacy text is read and written in.
pub fn legacy_charset() -> LegacyCharset {
    LegacyCharset::from_code(CHARSET.load(Ordering::Relaxed))
}

/// Decode text a server sent: UTF-8 when the bytes are UTF-8 (SJK and some mods send
/// it), else one character per byte: Windows-1251 when the text reads as Russian (see
/// [`LegacyCharset`]), else Latin-1, as the rest of the client reads incoming text
/// (`sjk_client::decode_legacy`). A name such as `jof§entinel` (byte 0xA7) then reads
/// as its glyph instead of being refused or replaced by a question mark.
pub fn decode_legacy_text(bytes: &[u8]) -> Cow<'_, str> {
    decode_with(bytes, legacy_charset())
}

fn decode_with(bytes: &[u8], charset: LegacyCharset) -> Cow<'_, str> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) if reads_cyrillic(bytes, charset) => {
            Cow::Owned(bytes.iter().map(|&byte| windows_1251_char(byte)).collect())
        }
        Err(_) => Cow::Owned(bytes.iter().map(|&byte| char::from(byte)).collect()),
    }
}

fn reads_cyrillic(bytes: &[u8], charset: LegacyCharset) -> bool {
    match charset {
        LegacyCharset::Auto => looks_cyrillic(bytes),
        LegacyCharset::Western => false,
        LegacyCharset::Cyrillic => true,
    }
}

/// Whether single-byte `bytes` read as Russian in Windows-1251 rather than as Latin-1.
///
/// Both code pages put letters in 0xC0..=0xFF, Cyrillic's lower case in 0xE0..=0xFF.
/// A Russian word is made of those bytes only, while a word of a Latin-script
/// language mixes accented letters with ASCII ones (`été`, `Größe`). So the text
/// reads as Russian when one of its words (a run of letter bytes) is made of high
/// letters only and is two of them with a lower-case one (`да`), or four or more
/// (`ПРИВЕТ`). Decorated names stay Latin-1: `ÐÄRK` mixes in ASCII, `ÆØ` is two
/// capitals.
pub fn looks_cyrillic(bytes: &[u8]) -> bool {
    let high = |byte: u8| matches!(byte, 0xC0..=0xFF | 0xA8 | 0xB8);
    bytes
        .split(|&byte| !(byte.is_ascii_alphabetic() || high(byte)))
        .any(|word| {
            word.iter().all(|&byte| high(byte))
                && (word.len() >= 4 || (word.len() >= 2 && word.iter().any(|&byte| byte >= 0xE0)))
        })
}

/// Encode text for the wire as legacy clients read it.
///
/// Text whose every character has a Windows-1252 byte is sent as those bytes: ASCII
/// and U+0080..=U+00FF as the byte of the same value, and the 27 typographic
/// characters Windows-1252 adds (`€`, `™`, `œ`, curly quotes, ...) as their bytes
/// in 0x80..=0x9F. Russian text whose every character has a Windows-1251 byte is
/// sent as those, as EternalJK sends it. Any other text (CJK, emoji, `♥`, Cyrillic
/// mixed with `é`) is sent as UTF-8 unchanged, which SJK clients still decode.
///
/// Under [`LegacyCharset::Auto`] a single-byte encoding is used only when
/// [`decode_legacy_text`] reads it back as the same text, so SJK players always read
/// each other exactly: a Cyrillic line too short to read as Russian (`ДА`), or a
/// Latin-1 one that would (`àé`), is sent as UTF-8. ASCII, the common case, is
/// returned borrowed without allocating.
pub fn encode_legacy_text(text: &str) -> Cow<'_, [u8]> {
    encode_with(text, legacy_charset())
}

fn encode_with(text: &str, charset: LegacyCharset) -> Cow<'_, [u8]> {
    if text.is_ascii() {
        return Cow::Borrowed(text.as_bytes());
    }
    let western = (charset != LegacyCharset::Cyrillic)
        .then(|| single_byte(text, windows_1252_byte))
        .flatten()
        .filter(|bytes| charset == LegacyCharset::Western || !looks_cyrillic(bytes));
    let cyrillic = || {
        (charset != LegacyCharset::Western)
            .then(|| single_byte(text, windows_1251_byte))
            .flatten()
            .filter(|bytes| charset == LegacyCharset::Cyrillic || looks_cyrillic(bytes))
    };
    match western.or_else(cyrillic) {
        Some(bytes) => Cow::Owned(bytes),
        None => Cow::Borrowed(text.as_bytes()),
    }
}

/// `text` in a single-byte code page, if each of its characters has a `byte` there.
fn single_byte(text: &str, byte: fn(char) -> Option<u8>) -> Option<Vec<u8>> {
    text.chars().map(byte).collect()
}

/// The Windows-1252 byte of `character`, if it has one.
///
/// The renderer uses it too: JKA's fonts hold one glyph per byte, so the character
/// a player typed (`€`) and the byte another client sent (0x80) select one glyph.
pub fn windows_1252_byte(character: char) -> Option<u8> {
    u8::try_from(u32::from(character)).ok().or_else(|| {
        WINDOWS_1252_HIGH
            .iter()
            .position(|&mapped| mapped == character)
            .map(|index| 0x80 + index as u8)
    })
}

/// The character Windows-1252 assigns to `byte`: the byte's own value, except the
/// 27 typographic characters in 0x80..=0x9F. The inverse of [`windows_1252_byte`].
pub fn windows_1252_char(byte: u8) -> char {
    match byte {
        0x80..=0x9f => WINDOWS_1252_HIGH[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

/// The Windows-1251 byte of `character`, if it has one: ASCII as itself, `А`..`я` as
/// 0xC0..=0xFF, and the rest of the code page from its table.
pub fn windows_1251_byte(character: char) -> Option<u8> {
    match character {
        '\0'..='\u{7f}' => Some(character as u8),
        'А'..='я' => Some((u32::from(character) - 0x410 + 0xC0) as u8),
        _ => WINDOWS_1251_HIGH
            .iter()
            .position(|&mapped| mapped == character)
            .map(|index| 0x80 + index as u8),
    }
}

/// The character Windows-1251 assigns to `byte`. The inverse of [`windows_1251_byte`].
pub fn windows_1251_char(byte: u8) -> char {
    match byte {
        0x00..=0x7f => char::from(byte),
        0x80..=0xbf => WINDOWS_1251_HIGH[usize::from(byte - 0x80)],
        _ => char::from_u32(u32::from(byte) - 0xC0 + 0x410).unwrap_or('?'),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_legacy_text(text: &str) -> Cow<'_, [u8]> {
        encode_with(text, LegacyCharset::Auto)
    }

    #[test]
    fn ascii_is_borrowed_unchanged() {
        let encoded = encode_legacy_text("say \"^1hello\"");
        assert!(matches!(encoded, Cow::Borrowed(_)));
        assert_eq!(&*encoded, b"say \"^1hello\"");
    }

    #[test]
    fn latin1_letters_are_single_bytes() {
        assert_eq!(&*encode_legacy_text("ø"), [0xf8]);
        assert_eq!(&*encode_legacy_text("say \"j^6ø^7f §·¤\""), {
            let mut expected = b"say \"j^6".to_vec();
            expected.extend_from_slice(&[0xf8]);
            expected.extend_from_slice(b"^7f ");
            expected.extend_from_slice(&[0xa7, 0xb7, 0xa4]);
            expected.push(b'"');
            expected
        });
    }

    #[test]
    fn windows_1252_typography_uses_its_high_bytes() {
        assert_eq!(&*encode_legacy_text("€"), [0x80]);
        assert_eq!(&*encode_legacy_text("™"), [0x99]);
        assert_eq!(&*encode_legacy_text("“œ”"), [0x93, 0x9c, 0x94]);
        assert_eq!(&*encode_legacy_text("Ÿ"), [0x9f]);
    }

    #[test]
    fn c1_characters_return_to_their_own_byte() {
        // What `decode_legacy` makes of the bytes 0x80..=0x9F, undefined slots included.
        for byte in 0x80..=0x9f_u8 {
            let text = char::from(byte).to_string();
            assert_eq!(&*encode_legacy_text(&text), [byte]);
        }
    }

    #[test]
    fn russian_leaves_in_windows_1251_as_eternaljk_sends_it() {
        // `ConvertUTF32ToExpectedCharset`: U+0410 is 192, U+044F is 255.
        assert_eq!(
            &*encode_legacy_text("say \"привет\""),
            b"say \"\xef\xf0\xe8\xe2\xe5\xf2\""
        );
        assert_eq!(&*encode_legacy_text("Абв я"), b"\xc0\xe1\xe2 \xff");
        assert_eq!(&*encode_legacy_text("Ёлка №1"), b"\xa8\xeb\xea\xe0 \xb91");
    }

    #[test]
    fn text_outside_one_code_page_stays_utf8() {
        // `ДА` is too short to read back as Russian; `àé` would read back as `аé`.
        for text in ["ø ♥", "say \"日本\"", "😀", "привет é", "ДА", "àé"] {
            let encoded = encode_legacy_text(text);
            assert!(matches!(encoded, Cow::Borrowed(_)), "{text}");
            assert_eq!(&*encoded, text.as_bytes());
        }
    }

    #[test]
    fn every_line_auto_encodes_reads_back_the_same() {
        for text in [
            "привет",
            "gg wp ребята",
            "да",
            "ДА",
            "ПРИВЕТ",
            "été",
            "Größe",
            "ÐÄRK",
            "ÆØ",
            "àé",
            "jof§entinel",
            "日本語",
            "Ёж",
            "«ура»",
        ] {
            let encoded = encode_legacy_text(text);
            assert_eq!(decode_with(&encoded, LegacyCharset::Auto), text);
        }
    }

    #[test]
    fn the_player_can_force_either_code_page() {
        assert_eq!(&*encode_with("ДА", LegacyCharset::Cyrillic), b"\xc4\xc0");
        assert_eq!(&*encode_with("é", LegacyCharset::Cyrillic), "é".as_bytes());
        assert_eq!(
            &*encode_with("привет", LegacyCharset::Western),
            "привет".as_bytes()
        );
        assert_eq!(&*encode_with("àé", LegacyCharset::Western), b"\xe0\xe9");
        assert_eq!(decode_with(b"\xc4\xc0", LegacyCharset::Cyrillic), "ДА");
        assert_eq!(decode_with(b"\xef\xf0", LegacyCharset::Western), "ïð");
    }

    #[test]
    fn russian_bytes_from_eternaljk_read_as_cyrillic() {
        let decoded = decode_with(
            b"^2Vasya^7: \xef\xf0\xe8\xe2\xe5\xf2 all",
            LegacyCharset::Auto,
        );
        assert_eq!(decoded, "^2Vasya^7: привет all");
        assert_eq!(
            decode_with(b"\xc4\xc0\xc0\xc0", LegacyCharset::Auto),
            "ДААА"
        );
    }

    #[test]
    fn latin_text_stays_latin_1() {
        for text in ["été", "Größe", "ÐÄRK", "ÆØ", "jof§entinel", "Marañón", "ÀÉ"] {
            let bytes: Vec<u8> = text.chars().map(|c| c as u8).collect();
            assert!(!looks_cyrillic(&bytes), "{text}");
            assert_eq!(decode_with(&bytes, LegacyCharset::Auto), text);
        }
    }

    #[test]
    fn every_table_entry_encodes_to_its_byte() {
        for (index, &character) in WINDOWS_1252_HIGH.iter().enumerate() {
            let text = character.to_string();
            assert_eq!(&*encode_legacy_text(&text), [0x80 + index as u8]);
        }
    }
}

#[cfg(test)]
mod windows_1252_tests {
    use super::{windows_1251_byte, windows_1251_char, windows_1252_byte, windows_1252_char};

    #[test]
    fn every_byte_round_trips_through_its_character() {
        for byte in 0..=u8::MAX {
            assert_eq!(windows_1252_byte(windows_1252_char(byte)), Some(byte));
            assert_eq!(windows_1251_byte(windows_1251_char(byte)), Some(byte));
        }
    }

    #[test]
    fn typographic_characters_have_their_bytes() {
        for (character, byte) in [
            ('€', 0x80),
            ('’', 0x92),
            ('‘', 0x91),
            ('™', 0x99),
            ('—', 0x97),
        ] {
            assert_eq!(windows_1252_byte(character), Some(byte));
            assert_eq!(windows_1252_char(byte), character);
        }
        assert_eq!(windows_1252_byte('♥'), None);
    }

    #[test]
    fn windows_1251_matches_its_code_page() {
        for (character, byte) in [
            ('А', 0xc0),
            ('я', 0xff),
            ('Ё', 0xa8),
            ('ё', 0xb8),
            ('№', 0xb9),
        ] {
            assert_eq!(windows_1251_byte(character), Some(byte));
            assert_eq!(windows_1251_char(byte), character);
        }
        assert_eq!(windows_1251_byte('é'), None);
    }
}
