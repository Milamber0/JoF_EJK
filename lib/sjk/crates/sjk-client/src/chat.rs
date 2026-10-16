//! Chat presentation and addressing at the protocol-26 compatibility boundary.
//!
//! OpenJK `codemp/game/g_cmds.c:G_SayTo` appends the sender slot to `chat`
//! and `tchat`; it is metadata, not part of the quoted message. `Cmd_Tell_f`
//! accepts `tell <slot> <message>`. No transport or wire encoding lives here.

use crate::{GameState, ServerEvent, ServerEventKind};

const CS_PLAYERS: usize = 1_131;
// codemp/qcommon/q_shared.h:890, protocol-26's absolute player-slot limit.
const MAX_CLIENTS: usize = 32;
/// Conservative compose budget, in UTF-8 bytes, below codemp's MAX_SAY_TEXT.
pub const CHAT_INPUT_BYTES: usize = 149;

/// Stock reliable-command destination; private messages address a numeric slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChatDestination {
    /// Everyone the server permits to receive global chat.
    Global,
    /// The sender's team, subject to the server's game-mode policy.
    Team,
    /// One currently connected player.
    Player(u16),
}

/// A roster observation, invalidated on a name change or observed slot reuse.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChatTarget {
    slot: u16,
    generation: u64,
}

impl ChatTarget {
    /// Server-side client slot; callers must validate with the current roster.
    pub const fn slot(self) -> u16 {
        self.slot
    }
}

#[derive(Default)]
struct Player {
    // Cache wire spelling so lossy legacy-name decoding occurs only on identity changes.
    source_name: Vec<u8>,
    raw_name: String,
    name: String,
    generation: u64,
}

/// Bounded, presentation-only roster for chat actions. Refreshing an unchanged
/// gamestate borrows configstrings and does not allocate.
pub struct ChatRoster {
    players: [Player; MAX_CLIENTS],
}

impl Default for ChatRoster {
    fn default() -> Self {
        Self {
            players: std::array::from_fn(|_| Player::default()),
        }
    }
}

impl ChatRoster {
    /// Observe compact `CS_PLAYERS` name keys, including TaystJK's trailing `\`.
    pub fn update(&mut self, game: &GameState) {
        for (slot, player) in self.players.iter_mut().enumerate() {
            let name = game
                .config_string(CS_PLAYERS + slot)
                .and_then(player_name)
                .unwrap_or(b"");
            if player.source_name != name {
                player.source_name.clear();
                player.source_name.extend_from_slice(name);
                player.generation = player.generation.wrapping_add(1);
                player.raw_name.clear();
                // Latin-1, not UTF-8: see `legacy_text::decode_legacy`.
                player
                    .raw_name
                    .push_str(&crate::legacy_text::decode_legacy(name));
                player.name = chat_name_key(&player.raw_name);
            }
        }
    }

    /// Resolve server-provided metadata. Unattributed messages stay unattributed;
    /// displayed names are never guessed into private-message destinations.
    pub fn target(&self, sender: Option<u16>) -> Option<ChatTarget> {
        let slot = sender?;
        let player = self.players.get(usize::from(slot))?;
        (!player.name.is_empty()).then_some(ChatTarget {
            slot,
            generation: player.generation,
        })
    }

    /// Current name with its colour escapes, for display only; identity and
    /// destinations keep using [`ChatRoster::name`].
    pub fn display_name(&self, target: ChatTarget) -> Option<&str> {
        let player = self.players.get(usize::from(target.slot))?;
        (player.generation == target.generation && !player.raw_name.is_empty())
            .then_some(player.raw_name.as_str())
    }

    /// Current plain name, only while this exact roster observation is valid.
    pub fn name(&self, target: ChatTarget) -> Option<&str> {
        let player = self.players.get(usize::from(target.slot))?;
        (player.generation == target.generation && !player.name.is_empty())
            .then_some(player.name.as_str())
    }

    /// Resolve a typed name to one connected player's slot; see [`crate::lookup_player`].
    pub fn lookup(&self, query: &str) -> crate::PlayerLookup {
        crate::lookup_player(
            self.players.iter().enumerate().map(|(slot, player)| {
                (slot as u16, player.name.as_str(), player.raw_name.as_str())
            }),
            query,
        )
    }
}

fn player_name(config: &[u8]) -> Option<&[u8]> {
    let info = crate::LegacyClientInfo::new(config);
    info.bytes("n").or_else(|| info.bytes("name"))
}

/// Plain modern-chat text: remove Quake colour escapes and the stock chat
/// separator (0x19), retaining Unicode and ordinary punctuation.
pub fn chat_plain_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^' && chars.peek().is_some_and(char::is_ascii_digit) {
            chars.next();
        } else {
            push_chat_char(&mut output, c);
        }
    }
    output
}

/// A player's name as the roster's key (friends, `tell <name>`, private
/// messages): plain text whose Windows-1252 bytes 0x80..=0x9F (decoded as C1
/// controls) become the typographic characters they stand for (`’`, `€`, `…`), so
/// the name matches what a player types, and with every remaining control (a
/// vertical tab in `{JoF}\vToxiee`) removed. The displayed name keeps them all.
pub fn chat_name_key(value: &str) -> String {
    chat_plain_text(value)
        .chars()
        .map(|c| match u8::try_from(u32::from(c)) {
            Ok(byte @ 0x80..=0x9f) => sjk_protocol::windows_1252_char(byte),
            _ => c,
        })
        .filter(|c| !c.is_control())
        .collect()
}

/// Display chat text: drop the stock 0x19 separators but keep `^n` colour
/// escapes, which the text renderer applies per glyph.
pub fn chat_display_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for c in value.chars() {
        push_chat_char(&mut output, c);
    }
    output
}

/// Line breaks and tabs become spaces and the 0x19 separator is dropped. Other
/// control characters stay: a byte in 0x80..=0x9F decodes to the C1 control of
/// that value and is a Windows-1252 symbol (`€`, `’`, `…`), and a C0 control in
/// a name draws `.`, as it does in retail and EternalJK.
fn push_chat_char(output: &mut String, c: char) {
    match c {
        '\n' | '\r' | '\t' => output.push(' '),
        '\u{19}' | '\0' => {}
        _ => output.push(c),
    }
}

/// Chars of `value` with `^n` colour escapes skipped, as `(byte index, char)`.
fn uncoloured(value: &str) -> impl Iterator<Item = (usize, char)> + '_ {
    let mut skip = false;
    value.char_indices().filter(move |(index, c)| {
        if skip {
            skip = false;
            return false;
        }
        if *c == '^'
            && value[index + 1..]
                .chars()
                .next()
                .is_some_and(|n| n.is_ascii_digit())
        {
            skip = true;
            return false;
        }
        true
    })
}

/// Byte offset just past a prefix equal to `prefix` once colour codes are
/// ignored on both sides, so a server that colours the prefix differently from
/// the configstring name still splits.
fn skip_prefix(text: &str, prefix: &str) -> Option<usize> {
    let mut wanted = uncoloured(prefix).map(|(_, c)| c);
    let mut cursor = 0;
    for (index, c) in uncoloured(text) {
        let Some(expected) = wanted.next() else {
            // Stop right after the last matched character: a colour code
            // between the prefix and the body belongs to the body.
            return Some(cursor);
        };
        if c != expected {
            return None;
        }
        cursor = index + c.len_utf8();
    }
    wanted.next().is_none().then_some(cursor)
}

/// Separate only a known stock name prefix. Unfamiliar mod formatting is

/// retained in full. The prefix does not establish sender identity.
pub fn chat_body<'a>(text: &'a str, name: &str) -> (&'a str, bool) {
    for (open, close, private) in [("", ": ", false), ("(", "): ", false), ("[", "]: ", true)] {
        if let Some(at) = skip_prefix(text, open)
            .and_then(|at| Some(at + skip_prefix(&text[at..], name)?))
            .and_then(|at| Some(at + skip_prefix(&text[at..], close)?))
        {
            return (&text[at..], private);
        }
    }
    (text, false)
}

/// What EternalJK sends for `%`, which the engine turns into `.` on the way
/// (`MSG_ReadString`): `°/.`, the degree sign going out as byte 0xB0.
const PERCENT_ESCAPE: &str = "\u{b0}/.";
/// What EternalJK sends for `"`, which would end the quoted command.
const QUOTE_ESCAPE: &str = "''";

/// Construct one quoted stock command. Controls become spaces; `%` and `"` are
/// escaped as EternalJK's chat sends them (`Message_Key`, `cl_keys.cpp:896-930`
/// at JoF EternalJK bd5e202), so EternalJK clients and SJK show them again
/// ([`chat_unescape`]). The byte budget counts the bytes the text leaves with
/// ([`sjk_protocol::encode_legacy_text`]: one per Latin-1 or Russian letter, UTF-8
/// otherwise), always ends at a character boundary and never splits an escape.
pub fn chat_command(destination: ChatDestination, text: &str) -> Option<String> {
    if matches!(destination, ChatDestination::Player(slot) if usize::from(slot) >= MAX_CLIENTS) {
        return None;
    }
    let mut payload = String::with_capacity(CHAT_INPUT_BYTES);
    let mut buffer = [0; 4];
    for c in text.chars() {
        let before = payload.len();
        payload.push_str(chat_piece(c, &mut buffer));
        if sent_bytes(&payload) > CHAT_INPUT_BYTES {
            payload.truncate(before);
            break;
        }
    }
    let payload = payload.trim();
    if payload.is_empty() {
        return None;
    }
    Some(match destination {
        ChatDestination::Global => format!("say \"{payload}\""),
        ChatDestination::Team => format!("say_team \"{payload}\""),
        ChatDestination::Player(slot) => format!("tell {slot} \"{payload}\""),
    })
}

/// What `c` becomes in a chat command: its escape, a space for a control, or
/// itself.
fn chat_piece(c: char, buffer: &mut [u8; 4]) -> &str {
    match c {
        '%' => PERCENT_ESCAPE,
        '"' => QUOTE_ESCAPE,
        c if c.is_control() => " ",
        c => c.encode_utf8(buffer),
    }
}

/// Bytes an escaped chat payload leaves with.
fn sent_bytes(payload: &str) -> usize {
    sjk_protocol::encode_legacy_text(payload).len()
}

/// Whether a draft fits the [`CHAT_INPUT_BYTES`] budget once escaped and encoded
/// as it is sent, so the chat field stops where the sent message would be cut, as
/// EternalJK's field leaves room for its escapes (`cl_keys.cpp:645-650`). The whole
/// line counts, since it picks the encoding: Russian takes a byte a letter when the
/// line leaves in Windows-1251, two in UTF-8, and a kanji three.
pub fn chat_input_fits(text: &str) -> bool {
    let mut payload = String::with_capacity(text.len() + 8);
    let mut buffer = [0; 4];
    for c in text.chars() {
        payload.push_str(chat_piece(c, &mut buffer));
    }
    sent_bytes(&payload) <= CHAT_INPUT_BYTES
}

/// Show EternalJK's chat escapes as what was typed: `°/.` as `%` and `''` as
/// `"`, scanning left to right as `CG_ChatBox_AddString` does
/// (`cg_draw.c:9897-9916` at JoF EternalJK bd5e202). Text without them is
/// returned borrowed.
pub fn chat_unescape(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains(PERCENT_ESCAPE) && !text.contains(QUOTE_ESCAPE) {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix(PERCENT_ESCAPE) {
            output.push('%');
            rest = after;
        } else if let Some(after) = rest.strip_prefix(QUOTE_ESCAPE) {
            output.push('"');
            rest = after;
        } else {
            output.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    std::borrow::Cow::Owned(output)
}

/// Interpret the already-tokenized stock chat command (`G_SayTo`).
pub(crate) fn server_chat_event(arguments: &[Vec<u8>]) -> Option<ServerEvent> {
    let command = arguments.first()?.as_slice();
    let kind = match command {
        b"chat" | b"lchat" => ServerEventKind::Chat,
        b"tchat" | b"ltchat" => ServerEventKind::TeamChat,
        _ => return None,
    };
    let localized = matches!(command, b"lchat" | b"ltchat");
    let sender_index = if localized { 5 } else { 2 };
    let sender = arguments
        .get(sender_index)
        .and_then(|arg| std::str::from_utf8(arg).ok())
        .and_then(|arg| arg.parse::<u16>().ok())
        .filter(|slot| usize::from(*slot) < MAX_CLIENTS);
    let text = if localized {
        // CG_Chat_f: location-bearing chat uses separate name/location/colour/body args.
        let [name, location, color, body] = [1, 2, 3, 4].map(|i| {
            arguments
                .get(i)
                .map(|arg| crate::legacy_text::decode_legacy(arg))
        });
        let location = location?;
        // Use the client's existing localization marker for CG_Chat_f's @key.
        let marker = if location.starts_with('@') { "@@@" } else { "" };
        let location = location.strip_prefix('@').unwrap_or(&location);
        format!("{}^7<{marker}{location}> ^{}{}", name?, color?, body?)
    } else {
        crate::legacy_text::decode_legacy(arguments.get(1)?).into_owned()
    };
    Some(ServerEvent { kind, text, sender })
}

#[cfg(test)]
mod display_text_tests {
    use super::{chat_display_text, chat_plain_text};

    #[test]
    fn chat_keeps_windows_1252_symbols_and_name_controls() {
        // Bytes 0x92, 0x91, 0x80 and 0x85 from another client, decoded as Latin-1.
        let received = "^6a\u{92}\u{91}\u{80}\u{85}×";
        assert_eq!(chat_display_text(received), received);
        assert_eq!(chat_plain_text(received), "a\u{92}\u{91}\u{80}\u{85}×");
        // A vertical tab in a name stays (drawn as `.`); the 0x19 separator and
        // line breaks do not.
        assert_eq!(
            chat_display_text("{JoF}\u{b}Toxiee\u{19}: hi\nthere"),
            "{JoF}\u{b}Toxiee: hi there"
        );
    }

    #[test]
    fn roster_keys_have_no_control_characters() {
        use super::chat_name_key;
        // ’ € … sent as bytes 0x92 0x80 0x85 match the characters a player types.
        assert_eq!(chat_name_key("^1a\u{92}b\u{80}\u{85}"), "a’b€…");
        // A vertical tab is dropped; an undefined 0x81 too.
        assert_eq!(
            chat_name_key("{JoF}\u{b}Toxiee\u{b}{C}.ak\u{81}"),
            "{JoF}Toxiee{C}.ak"
        );
    }
}

#[cfg(test)]
mod escape_tests {
    use super::*;

    #[test]
    fn percent_and_quotes_go_out_as_eternaljk_sends_them() {
        assert_eq!(
            chat_command(ChatDestination::Global, "100% \"done\"").as_deref(),
            Some("say \"100\u{b0}/. ''done''\"")
        );
        assert_eq!(
            chat_command(ChatDestination::Team, "50%").as_deref(),
            Some("say_team \"50\u{b0}/.\"")
        );
        assert_eq!(
            chat_command(ChatDestination::Player(3), "a\tb").as_deref(),
            Some("tell 3 \"a b\"")
        );
        // The degree sign leaves as the single byte EternalJK sends.
        let command = chat_command(ChatDestination::Global, "%").unwrap();
        assert_eq!(
            &*crate::legacy_text::legacy_command(command.as_bytes()),
            b"say \"\xb0/.\""
        );
    }

    #[test]
    fn the_input_fits_what_is_sent() {
        let full = |text: &str| {
            assert!(chat_input_fits(text), "{text}");
            let command = chat_command(ChatDestination::Global, text).unwrap();
            let sent = crate::legacy_text::legacy_command(command.as_bytes());
            assert_eq!(sent.len(), "say \"\"".len() + CHAT_INPUT_BYTES, "{text}");
            assert!(!chat_input_fits(&format!("{text}a")), "{text}");
        };
        full(&"a".repeat(CHAT_INPUT_BYTES));
        // `é` and Russian leave a byte a letter, as EternalJK sends them.
        full(&format!("{}caf\u{e9}", "caf\u{e9} ".repeat(29)));
        full(&"\u{436}".repeat(CHAT_INPUT_BYTES));
        // `%` leaves as `°/.`, three bytes; a kanji as three bytes of UTF-8.
        assert!(chat_input_fits(&"%".repeat(CHAT_INPUT_BYTES / 3)));
        assert!(!chat_input_fits(&"%".repeat(CHAT_INPUT_BYTES / 3 + 1)));
        assert!(chat_input_fits(&"\u{65e5}".repeat(CHAT_INPUT_BYTES / 3)));
        assert!(!chat_input_fits(
            &"\u{65e5}".repeat(CHAT_INPUT_BYTES / 3 + 1)
        ));
    }

    #[test]
    fn the_budget_never_splits_an_escape_or_a_character() {
        let text = format!("{}%", "a".repeat(CHAT_INPUT_BYTES - 2));
        let command = chat_command(ChatDestination::Global, &text).unwrap();
        assert!(!command.contains('\u{b0}'), "{command}");
        assert_eq!(command.len(), "say \"\"".len() + CHAT_INPUT_BYTES - 2);
        // Kanji are cut whole: 49 of them (147 bytes), not 49 and two thirds.
        let text = "\u{65e5}".repeat(60);
        let command = chat_command(ChatDestination::Global, &text).unwrap();
        assert_eq!(command, format!("say \"{}\"", "\u{65e5}".repeat(49)));
    }

    /// What arrives back at a client when `text` is typed in SJK's chat: the
    /// command's bytes as the server reads them (`MSG_ReadString` turns every `%`
    /// into `.`), the quoted message taken out, and the client's decoding and
    /// unescaping.
    fn round_trip(text: &str) -> String {
        let command = chat_command(ChatDestination::Global, text).unwrap();
        let bytes: Vec<u8> = crate::legacy_text::legacy_command(command.as_bytes())
            .iter()
            .map(|&byte| if byte == b'%' { b'.' } else { byte })
            .collect();
        let quoted = &bytes[b"say \"".len()..bytes.len() - 1];
        assert!(!quoted.contains(&b'"'), "a quote would end the message");
        // A byte in 0x80..=0x9F decodes as the C1 character of that value, which
        // the renderer draws as its Windows-1252 glyph (`€`, `’`, ...).
        chat_unescape(&crate::legacy_text::decode_legacy(quoted))
            .chars()
            .map(|c| match u8::try_from(u32::from(c)) {
                Ok(byte @ 0x80..=0x9f) => sjk_protocol::windows_1252_char(byte),
                _ => c,
            })
            .collect()
    }

    #[test]
    fn every_typed_character_comes_back() {
        // Printable ASCII, Latin-1 and the characters Windows-1252 adds.
        let mut typed: Vec<char> = (0x20_u8..0x7f).map(char::from).collect();
        typed.extend((0xa0_u32..=0xff).filter_map(char::from_u32));
        typed.extend("€‚ƒ„…†‡ˆ‰Š‹ŒŽ‘’“”•–—˜™š›œžŸ".chars());
        for c in typed {
            let text = format!("a{c}b");
            assert_eq!(round_trip(&text), text, "{c:?} (U+{:04X})", u32::from(c));
        }
        let mixed = "100% \"x\" ^1red ;// \\ ~`@#$&*()[]{}<>|=+-_!?.,:é€ü";
        assert_eq!(round_trip(mixed), mixed);
    }

    #[test]
    fn escapes_show_as_what_was_typed() {
        assert_eq!(chat_unescape("100\u{b0}/. ''done''"), "100% \"done\"");
        assert_eq!(chat_unescape("^1a\u{b0}/.^7b"), "^1a%^7b");
        // Left to right, as EternalJK: three apostrophes are a quote and one.
        assert_eq!(chat_unescape("\u{27}\u{27}\u{27}"), "\"\u{27}");
        assert_eq!(chat_unescape("\u{b0}/"), "\u{b0}/");
        assert!(matches!(
            chat_unescape("plain"),
            std::borrow::Cow::Borrowed("plain")
        ));
        // What SJK sends shows as what was typed.
        let sent = chat_command(ChatDestination::Global, "it's 100% \"fine\"").unwrap();
        let body = sent
            .strip_prefix("say \"")
            .unwrap()
            .strip_suffix('"')
            .unwrap();
        assert_eq!(chat_unescape(body), "it's 100% \"fine\"");
    }
}
