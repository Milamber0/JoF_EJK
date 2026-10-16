//! Player-authored text that predates Unicode, in both directions.
//!
//! JKA is a pre-Unicode engine: names, chat and client info are byte strings rendered through
//! a codepage font, so a byte above 0x7F means the Latin-1 (or, for Russian players,
//! Windows-1251) character of that value, not part of a UTF-8 sequence. Treating those
//! bytes as UTF-8 and replacing what does not decode turns a name like `Marañ` into `Maraï¿½` on screen, which is what the owner saw in the
//! kill announcement; validating strictly instead loses the name entirely, which is what the
//! crosshair used to do. Text the client sends goes the other way through
//! [`sjk_protocol::encode_legacy_text`], so legacy clients read it too.

use std::borrow::Cow;

/// Decode player-authored bytes for display.
///
/// Valid UTF-8 is returned borrowed and unchanged, which covers ASCII and any client that
/// genuinely sends UTF-8. Anything else is one character per byte, as JKA's own font gives
/// it: Latin-1, or Windows-1251 when the text reads as Russian
/// ([`sjk_protocol::decode_legacy_text`]). Pure ASCII is identical under every reading, so
/// the common case never allocates and never changes.
pub fn decode_legacy(bytes: &[u8]) -> Cow<'_, str> {
    sjk_protocol::decode_legacy_text(bytes)
}

/// The bytes a reliable command leaves with.
///
/// A command built as UTF-8 text is encoded for legacy clients
/// ([`sjk_protocol::encode_legacy_text`]); bytes that are not UTF-8 are already
/// legacy bytes and pass unchanged.
pub(crate) fn legacy_command(command: &[u8]) -> Cow<'_, [u8]> {
    match std::str::from_utf8(command) {
        Ok(text) => sjk_protocol::encode_legacy_text(text),
        Err(_) => Cow::Borrowed(command),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_chat_leaves_in_windows_1252() {
        assert_eq!(
            &*legacy_command("say \"gg ø\"".as_bytes()),
            b"say \"gg \xf8\""
        );
        assert!(matches!(legacy_command(b"score"), Cow::Borrowed(_)));
    }

    #[test]
    fn legacy_bytes_pass_unchanged() {
        assert_eq!(&*legacy_command(b"say \"\xf8\""), b"say \"\xf8\"");
    }

    #[test]
    fn decoded_names_go_back_byte_exact() {
        // A name read from the game, colour codes and the 0x99 glyph included.
        let wire = b"^1J\x99\xf8^7";
        let decoded = decode_legacy(wire);
        assert_eq!(&*legacy_command(decoded.as_bytes()), wire);
    }
}
