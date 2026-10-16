//! Chat text rules (`PROTOCOL.md` in Sol-Vulpes/SJK-hub, "Chat"), word for word with
//! the hub's `src/chat.rs`: what a player may say to every SJK player. A message is one
//! line of the bio's alphabet (`crate::bio::allowed`) and colour codes, so it shows the
//! same in every SJK font and hides, reorders or piles up nothing. The service checks a
//! message before sending it; a message read from a hub is shown through
//! [`for_display`], which drops what the rules would refuse. A sender's name, which the
//! hub keeps as worn, is shown through [`name_for_display`], whose alphabet adds the
//! marks players put around clan tags ([`name_allowed`]).

/// Longest message, in characters.
pub const TEXT_MAX: usize = 150;
/// Longest name shown, in characters: the hub's limit for the name a message carries.
pub const NAME_MAX: usize = 64;
/// Windows-1252's characters in 0x80..=0x9F, which a Jedi Academy name can hold.
const WINDOWS_1252_HIGH: &str = "€‚ƒ„…†‡ˆ‰Š‹ŒŽ‘’“”•–—˜™š›œžŸ";
/// Longest run of one character.
const RUN_MAX: usize = 8;

/// Which rule a message breaks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChatError {
    /// It holds a line break.
    Lines,
    /// It is empty or over [`TEXT_MAX`] characters once tidied.
    Length,
    /// It holds a character outside the alphabet, or a `^` that starts no colour code.
    Characters,
    /// It repeats one character more than 8 times in a row.
    Noise,
}

impl ChatError {
    /// The hub's error code for the rule.
    pub fn code(self) -> &'static str {
        match self {
            Self::Lines => "chat_lines",
            Self::Length => "chat_length",
            Self::Characters => "chat_characters",
            Self::Noise => "chat_noise",
        }
    }

    /// What the player is told.
    pub fn message(self) -> &'static str {
        match self {
            Self::Lines => "a message is one line",
            Self::Length => "a message is 1 to 150 characters",
            Self::Characters => {
                "a message keeps to letters, digits, common punctuation and colour codes"
            }
            Self::Noise => "a message repeats no character more than 8 times in a row",
        }
    }
}

impl std::fmt::Display for ChatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

/// The message ready to send or keep (runs of spaces and tabs made one space, the ends
/// trimmed), or the first rule it breaks.
pub fn check(raw: &str) -> Result<String, ChatError> {
    if raw.contains(['\n', '\r']) {
        return Err(ChatError::Lines);
    }
    let text = raw
        .split([' ', '\t'])
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let count = text.chars().count();
    if count == 0 || count > TEXT_MAX {
        return Err(ChatError::Length);
    }
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '^' => {
                if !chars.next().is_some_and(|next| next.is_ascii_digit()) {
                    return Err(ChatError::Characters);
                }
            }
            c if crate::bio::allowed(c) => {}
            _ => return Err(ChatError::Characters),
        }
    }
    if !crate::emoji::marks_in_place(&text) {
        return Err(ChatError::Characters);
    }
    let mut run = (None, 0);
    for c in text.chars() {
        run = if run.0 == Some(c) {
            (Some(c), run.1 + 1)
        } else {
            (Some(c), 1)
        };
        if run.1 > RUN_MAX {
            return Err(ChatError::Noise);
        }
    }
    Ok(text)
}

/// `raw` as a page may show it whatever a hub sent: characters the rules refuse
/// dropped (a `^` kept only before a digit, line breaks made spaces), tidied, runs cut
/// to 8 and the whole to [`TEXT_MAX`] characters. A message that passes [`check`]
/// comes back unchanged.
pub fn for_display(raw: &str) -> String {
    shown(raw, crate::bio::allowed, TEXT_MAX)
}

/// Whether a sender's name may show `c`: the message alphabet, and every other
/// character a Jedi Academy name can draw, which is all of Windows-1252 that prints
/// (SJK's fonts draw each of them): ASCII's other symbols (`{ } \` and `` ` ``),
/// Latin-1's (`« » ¤ § · ×`) and the typographic ones (`• † ™ …`), the marks players
/// put around clan tags. The soft hyphen, which prints nothing, stays out.
pub fn name_allowed(c: char) -> bool {
    crate::bio::allowed(c)
        || (matches!(c, '!'..='~' | '\u{A1}'..='\u{FF}') && c != '\u{AD}')
        || WINDOWS_1252_HIGH.contains(c)
}

/// A sender's name as a page may show it whatever a hub sent: as [`for_display`]
/// shows a message, with [`name_allowed`]'s alphabet and cut to [`NAME_MAX`]
/// characters.
pub fn name_for_display(raw: &str) -> String {
    shown(raw, name_allowed, NAME_MAX)
}

/// `raw` with only `allowed` characters and colour codes, tidied, runs cut to 8 and the
/// whole to `max` characters.
fn shown(raw: &str, allowed: fn(char) -> bool, max: usize) -> String {
    let mut kept = String::with_capacity(raw.len().min(max * 4));
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\n' | '\r' | '\t' | '\u{A0}' => kept.push(' '),
            '^' => {
                if let Some(&next) = chars.peek()
                    && next.is_ascii_digit()
                {
                    kept.push('^');
                    kept.push(next);
                    let _ = chars.next();
                }
            }
            c if allowed(c) => kept.push(c),
            _ => {}
        }
    }
    let kept = crate::emoji::without_stray_marks(&kept);
    let mut out = String::with_capacity(kept.len());
    let mut run = (None, 0);
    let mut count = 0;
    for c in kept
        .split(' ')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
    {
        run = if run.0 == Some(c) {
            (Some(c), run.1 + 1)
        } else {
            (Some(c), 1)
        };
        if run.1 > RUN_MAX {
            continue;
        }
        if count >= max {
            break;
        }
        out.push(c);
        count += 1;
    }
    // A colour code cut in half at the end is dropped with its `^`.
    if out.ends_with('^') {
        out.pop();
    }
    out.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_keeps_only_what_the_rules_allow() {
        assert_eq!(
            for_display("hi\u{202E}there \u{1F600}!"),
            "hithere \u{1F600}!"
        );
        assert_eq!(
            for_display("a\u{200D}b \u{1F44D}\u{1F3FD}"),
            "ab \u{1F44D}\u{1F3FD}"
        );
        assert_eq!(for_display("a\nb\r\nc"), "a b c");
        assert_eq!(for_display("^1red ^x no ^"), "^1red x no");
        assert_eq!(for_display(&"!".repeat(30)), "!".repeat(RUN_MAX));
        assert_eq!(for_display(&"ab".repeat(400)).chars().count(), TEXT_MAX);
        // A colour code cut in half at the length limit goes with its `^`.
        let cut = format!("{}c", "ab".repeat(74));
        assert_eq!(for_display(&format!("{cut}^5x")), cut);
        for good in ["gg wp", "^2gg ^7:poop:", "Привет"] {
            assert_eq!(for_display(good), good);
        }
        // Whatever comes in, what goes out passes the rules (or is empty).
        for raw in [
            "x\u{0301}\u{0301}y",
            "^^^^^^^^^1",
            &"a b\n".repeat(80),
            &"é".repeat(600),
            "\u{FEFF}\u{2066}abc\u{2069}",
            &format!("{}^", "ab".repeat(75)),
        ] {
            let shown = for_display(raw);
            assert!(
                shown.is_empty() || check(&shown) == Ok(shown.clone()),
                "{raw:?} -> {shown:?}"
            );
        }
    }

    #[test]
    fn names_keep_the_marks_around_clan_tags() {
        for name in [
            "{JoF}^1Sol",
            "\\o/ `Fox`",
            "«Ð» ¤§·× ^5Kyle",
            "•JoF• Sol™ † …",
            "[JoF]|=<Sol>=|~*",
        ] {
            assert_eq!(name_for_display(name), name);
        }
        // A message keeps its own alphabet.
        assert_eq!(for_display("{JoF} Sol"), "JoF Sol");
        assert_eq!(check("{JoF}"), Err(ChatError::Characters));
    }

    #[test]
    fn names_drop_what_prints_nothing_or_reorders() {
        assert_eq!(name_for_display("a\u{AD}b\u{200B}c"), "abc");
        assert_eq!(
            name_for_display("x\u{202E}y\u{0301} \u{1F600}"),
            "xy \u{1F600}"
        );
        assert_eq!(name_for_display("Sol\u{A0}\u{A0}Fox"), "Sol Fox");
        assert_eq!(name_for_display("a\u{7}\u{85}b"), "ab");
        assert_eq!(name_for_display("^1red ^x ^"), "^1red x");
        assert_eq!(name_for_display(&"=".repeat(20)), "=".repeat(RUN_MAX));
        assert_eq!(
            name_for_display(&"ab".repeat(100)).chars().count(),
            NAME_MAX
        );
        // Every character comes out only where the alphabet allows it; an emoji mark
        // between letters is out of place and dropped.
        for c in (0..0x3000).filter_map(char::from_u32).filter(|&c| c != '^') {
            let shown = name_for_display(&format!("a{c}b"));
            assert!(
                shown == format!("a{c}b") && name_allowed(c)
                    || ["ab", "a b"].contains(&shown.as_str())
                        && (!name_allowed(c) || crate::emoji::mark(c)),
                "{c:?} -> {shown:?}"
            );
        }
    }

    #[test]
    fn spaces_are_tidied_and_the_ends_trimmed() {
        assert_eq!(check("  hello \t  there ").as_deref(), Ok("hello there"));
    }

    #[test]
    fn a_message_is_one_line_of_1_to_150_characters() {
        assert_eq!(check("a\nb"), Err(ChatError::Lines));
        assert_eq!(check("a\rb"), Err(ChatError::Lines));
        assert_eq!(check(" \t "), Err(ChatError::Length));
        assert_eq!(check(""), Err(ChatError::Length));
        let longest = "ab ".repeat(50);
        assert_eq!(check(&longest).map(|text| text.chars().count()), Ok(149));
        assert!(check(&"ab".repeat(75)).is_ok());
        assert_eq!(
            check(&format!("{}c", "ab".repeat(75))),
            Err(ChatError::Length)
        );
        // Characters are counted, not bytes.
        assert!(check(&"é".repeat(150)).is_err_and(|e| e == ChatError::Noise));
        assert!(check(&"éa".repeat(75)).is_ok());
    }

    #[test]
    fn the_bio_alphabet_and_colour_codes_only() {
        assert_eq!(
            check("^2gg wp :poop: #>:D ^7(nice)").as_deref(),
            Ok("^2gg wp :poop: #>:D ^7(nice)")
        );
        assert_eq!(check("Привет, Ça va?").as_deref(), Ok("Привет, Ça va?"));
        assert_eq!(
            check("日本語のチャット、OK！").as_deref(),
            Ok("日本語のチャット、OK！")
        );
        assert_eq!(check("hi \u{1F600}").as_deref(), Ok("hi \u{1F600}"));
        assert_eq!(
            check("안녕 你好 👨\u{200D}👩\u{200D}👧").as_deref(),
            Ok("안녕 你好 👨\u{200D}👩\u{200D}👧")
        );
        assert_eq!(check("a\u{200D}b"), Err(ChatError::Characters));
        assert_eq!(check("a^"), Err(ChatError::Characters));
        assert_eq!(check("^xa"), Err(ChatError::Characters));
        assert_eq!(check("a\u{200B}b"), Err(ChatError::Characters));
        assert_eq!(check("a\u{202E}b"), Err(ChatError::Characters));
        assert_eq!(check("x\u{0301}"), Err(ChatError::Characters));
        assert_eq!(check("a\u{7}b"), Err(ChatError::Characters));
    }

    #[test]
    fn long_runs_of_one_character_are_noise() {
        assert!(check("noooooooo").is_ok());
        assert_eq!(check("nooooooooo"), Err(ChatError::Noise));
        assert_eq!(check("!!!!!!!!!"), Err(ChatError::Noise));
    }

    #[test]
    fn every_error_has_its_code() {
        let codes: Vec<&str> = [
            ChatError::Lines,
            ChatError::Length,
            ChatError::Characters,
            ChatError::Noise,
        ]
        .iter()
        .map(|error| error.code())
        .collect();
        assert_eq!(
            codes,
            ["chat_lines", "chat_length", "chat_characters", "chat_noise"]
        );
    }
}
