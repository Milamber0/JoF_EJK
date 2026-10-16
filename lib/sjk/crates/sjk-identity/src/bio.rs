//! The rules for a profile's bio (`PROTOCOL.md`, "Profile fields"), shared word for
//! word with the hub: what a player may write about themselves, and how a bio is
//! tidied before it is sent or shown.
//!
//! A bio is plain text other players read, so it keeps to what SJK's fonts draw and
//! nothing that hides, reorders or piles up: letters of the Latin and Cyrillic
//! alphabets, Japanese's kana and kanji (the Chinese characters too) and Korean's
//! Hangul, emoji ([`crate::emoji`]), digits, a space, a little punctuation (Japanese's
//! and Chinese's own marks included), Quake colour codes (`^` and a digit) and at most [`LINES_MAX`] lines. Everything else is refused, including
//! emoji, invisible and direction-changing characters, combining marks, private-use
//! and unassigned code points. The page filters what is typed with [`allowed`]; the
//! hub refuses a bio that breaks a rule; a bio read from a hub is shown through
//! [`for_display`], which drops what the rules would refuse, so a hub that kept an
//! older or foreign bio cannot put anything else on screen.

/// Longest bio, in characters, after [`tidy`].
pub const BIO_MAX: usize = 500;
/// Most lines a bio has (blank ones included).
pub const LINES_MAX: usize = 6;
/// Longest run of one character (`!!!!!!!!!`, `aaaaaaaaa` is noise).
pub const RUN_MAX: usize = 8;
/// The punctuation a bio may use besides letters, digits and spaces: ASCII's, the
/// marks Japanese is typed with (`、。「」『』・`, full-width `！？（）～`) and Chinese's
/// (full-width `，：；`, `《》`, curly quotes), and the euro sign.
pub const PUNCTUATION: &str = ".,!?'\"-:;()[]&/+#@%*_=~<>|$、。「」『』・！？（）～，：；《》“”‘’€";

/// Why a bio was refused; the hub answers each with its `error` code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BioError {
    /// Longer than [`BIO_MAX`] characters (`bio_length`).
    Length,
    /// A character outside [`allowed`], or a `^` that is not a colour code
    /// (`bio_characters`).
    Character,
    /// More than [`LINES_MAX`] lines (`bio_lines`).
    Lines,
    /// A run of one character longer than [`RUN_MAX`] (`bio_noise`).
    Noise,
}

impl BioError {
    /// The hub's `error` code for it.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Length => "bio_length",
            Self::Character => "bio_characters",
            Self::Lines => "bio_lines",
            Self::Noise => "bio_noise",
        }
    }

    /// What the player is told.
    pub const fn message(self) -> &'static str {
        match self {
            Self::Length => "a bio is at most 500 characters",
            Self::Character => {
                "a bio uses letters, digits, spaces, emoji and simple punctuation (no symbols)"
            }
            Self::Lines => "a bio is at most 6 lines",
            Self::Noise => "a bio repeats no character more than 8 times in a row",
        }
    }
}

impl std::fmt::Display for BioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

/// Whether `c` is a letter of the alphabets SJK's fonts draw: Latin (with its
/// accented and extended letters, Vietnamese's included), Cyrillic, Japanese
/// (hiragana, katakana with its prolonged-sound mark, kanji and the `々` repeat mark;
/// the kanji are the CJK ideographs, which Chinese shares) and Korean (the Hangul
/// syllables and the compatibility jamo typed alone, as in `ㅋㅋ`; not the conjoining
/// jamo, which pile up into syllables).
pub fn letter(c: char) -> bool {
    if c.is_ascii_alphabetic() {
        return true;
    }
    let code = u32::from(c);
    let latin = matches!(code, 0x00C0..=0x024F | 0x1E00..=0x1EFF) && c != '×' && c != '÷';
    let cyrillic = matches!(code, 0x0400..=0x04FF);
    let japanese = matches!(
        code,
        0x3005 | 0x3041..=0x30FF | 0x31F0..=0x31FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
    );
    let korean = matches!(code, 0xAC00..=0xD7A3 | 0x3131..=0x318E);
    (latin || cyrillic || japanese || korean) && c.is_alphabetic()
}

/// Whether a bio may hold `c` (newlines aside, which [`tidy`] counts as lines; `^`
/// aside, which must start a colour code).
pub fn allowed(c: char) -> bool {
    letter(c)
        || c.is_ascii_digit()
        || c == ' '
        || PUNCTUATION.contains(c)
        || crate::emoji::pictograph(c)
        || crate::emoji::mark(c)
}

/// The bio as stored and shown: line ends unified, each line's spaces and tabs
/// collapsed to one space and trimmed, blank lines at either end dropped and runs of
/// blank lines kept as one. The rules are checked on this form.
pub fn tidy(raw: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in raw.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        let line = line
            .split([' ', '\t'])
            .filter(|word| !word.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if line.is_empty() && lines.last().is_none_or(String::is_empty) {
            continue;
        }
        lines.push(line);
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines.join("\n")
}

/// The bio ready to send or store ([`tidy`]), or the first rule it breaks.
pub fn check(raw: &str) -> Result<String, BioError> {
    let bio = tidy(raw);
    if bio.chars().count() > BIO_MAX {
        return Err(BioError::Length);
    }
    let mut chars = bio.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\n' => {}
            '^' => {
                if !chars.next().is_some_and(|next| next.is_ascii_digit()) {
                    return Err(BioError::Character);
                }
            }
            c if allowed(c) => {}
            _ => return Err(BioError::Character),
        }
    }
    if !crate::emoji::marks_in_place(&bio) {
        return Err(BioError::Character);
    }
    if bio.split('\n').count() > LINES_MAX {
        return Err(BioError::Lines);
    }
    if longest_run(&bio) > RUN_MAX {
        return Err(BioError::Noise);
    }
    Ok(bio)
}

/// The longest run of one character in `text`.
fn longest_run(text: &str) -> usize {
    let mut longest = 0;
    let mut run = (None, 0);
    for c in text.chars() {
        run = if run.0 == Some(c) {
            (Some(c), run.1 + 1)
        } else {
            (Some(c), 1)
        };
        longest = longest.max(run.1);
    }
    longest
}

/// `raw` as a page may show it whatever a hub sent: characters the rules refuse
/// dropped (a `^` kept only before a digit), [`tidy`], cut to [`LINES_MAX`] lines,
/// runs cut to [`RUN_MAX`] and the whole to [`BIO_MAX`] characters. A bio that
/// passes [`check`] comes back unchanged.
pub fn for_display(raw: &str) -> String {
    let mut kept = String::with_capacity(raw.len().min(BIO_MAX * 4));
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\n' | '\r' | '\t' => kept.push(c),
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
    let tidied = tidy(&crate::emoji::without_stray_marks(&kept));
    let mut out = String::with_capacity(tidied.len());
    let mut run = (None, 0);
    let mut count = 0;
    for (index, line) in tidied.split('\n').take(LINES_MAX).enumerate() {
        if index > 0 {
            out.push('\n');
            count += 1;
            run = (None, 0);
        }
        for c in line.chars() {
            run = if run.0 == Some(c) {
                (Some(c), run.1 + 1)
            } else {
                (Some(c), 1)
            };
            if run.1 > RUN_MAX {
                continue;
            }
            if count >= BIO_MAX {
                break;
            }
            out.push(c);
            count += 1;
        }
    }
    // A colour code cut in half at the end is dropped with its `^`.
    if out.ends_with('^') {
        out.pop();
    }
    tidy(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_bios_pass_as_they_are() {
        for bio in [
            "",
            "Saber duelist from Lyon. Mostly on JoF, say hi!",
            "Jedi Knight since 2003 (yes, really) - main: staff, yellow.",
            "Ça va? Привет! Tiếng Việt, Łódź, Ñandú.",
            "^1Red^7 and white",
            "line one\nline two\n\nline four",
            "discord: sol#1234 | site: https://example.org/~sol?x=1&y=2",
        ] {
            assert_eq!(check(bio), Ok(bio.to_owned()), "{bio:?}");
            assert_eq!(for_display(bio), bio, "{bio:?}");
        }
    }

    #[test]
    fn spaces_and_blank_lines_are_tidied() {
        assert_eq!(tidy("  a \t b  "), "a b");
        assert_eq!(tidy("\n\na\r\n\r\n\r\nb\n\n"), "a\n\nb");
        assert_eq!(tidy("a\rb"), "a\nb");
        assert_eq!(check("  hello   there \n"), Ok("hello there".to_owned()));
    }

    #[test]
    fn weird_characters_are_refused() {
        for bio in [
            "stray joiner a\u{200D}b",
            "stray selector x\u{FE0F}",
            "zero\u{200B}width",
            "right\u{202E}to left",
            "isolate\u{2066}x",
            "soft\u{00AD}hyphen",
            "zalgo a\u{0301}\u{0302}\u{0303}",
            "nbsp\u{00A0}here",
            "ideographic\u{3000}space",
            "private \u{E000}",
            "bell \u{7}",
            "nul \u{0}",
            "arabic \u{0627}",
            "thai \u{0E01}",
            "greek \u{03B1}",
            "fullwidth \u{FF21}",
            "math \u{1D400}",
            "times \u{00D7}",
            "backslash \\",
            "braces {}",
            "caret ^ alone",
            "caret ^a letter",
            "trailing caret ^",
            "superscript \u{00B2}",
            "BOM \u{FEFF}",
        ] {
            assert_eq!(check(bio), Err(BioError::Character), "{bio:?}");
        }
    }

    #[test]
    fn length_lines_and_noise_are_bounded() {
        let long = "word ".repeat(100) + "end";
        assert_eq!(check(&long), Err(BioError::Length));
        assert!(check(&"ab".repeat(BIO_MAX / 2)).is_ok());
        assert_eq!(check(&"ab".repeat(BIO_MAX / 2 + 1)), Err(BioError::Length));
        assert!(check("1\n2\n3\n4\n5\n6").is_ok());
        assert_eq!(check("1\n2\n3\n4\n5\n6\n7"), Err(BioError::Lines));
        // Blank runs count once.
        assert!(check("1\n\n\n\n\n2\n3\n4\n5").is_ok());
        assert!(check("yes!!!!!!!!").is_ok());
        assert_eq!(check("yes!!!!!!!!!"), Err(BioError::Noise));
        assert_eq!(check("aaaaaaaaaaaa"), Err(BioError::Noise));
    }

    #[test]
    fn display_keeps_only_what_the_rules_allow() {
        assert_eq!(
            for_display("hi\u{202E}there \u{1F600}!"),
            "hithere \u{1F600}!"
        );
        assert_eq!(
            for_display("x\u{FE0F} \u{2764}\u{FE0F}"),
            "x \u{2764}\u{FE0F}"
        );
        assert_eq!(for_display("a\u{200B}b\u{0301}c"), "abc");
        assert_eq!(for_display("^1red ^x no ^"), "^1red x no");
        assert_eq!(for_display("1\n2\n3\n4\n5\n6\n7\n8"), "1\n2\n3\n4\n5\n6");
        assert_eq!(for_display(&"!".repeat(30)), "!".repeat(RUN_MAX));
        assert_eq!(for_display(&"ab".repeat(400)).chars().count(), BIO_MAX);
        assert_eq!(for_display("  \u{0}\u{7} "), "");
        // Whatever comes in, what goes out passes the rules.
        for raw in [
            "x\u{0301}\u{0301}\u{0301}y",
            "^^^^^^^^^1",
            &"a\n".repeat(50),
            &"é".repeat(600),
            "\u{FEFF}\u{2066}abc\u{2069}",
        ] {
            let shown = for_display(raw);
            assert_eq!(check(&shown), Ok(shown.clone()), "{raw:?} -> {shown:?}");
        }
    }

    #[test]
    fn letters_are_latin_cyrillic_and_japanese_only() {
        assert!(letter('a') && letter('Z') && letter('é') && letter('ß') && letter('Ж'));
        assert!(letter('ẞ') && letter('ơ'));
        for c in ['あ', 'カ', 'ー', '日', '漢', '々', 'ㇰ'] {
            assert!(letter(c), "{c}");
        }
        for c in ['한', '국', 'ㅋ', 'ㅎ'] {
            assert!(letter(c), "{c}");
        }
        // The middle dot, the voicing marks and conjoining jamo are no letters; Greek is
        // not drawn.
        assert!(!letter('・') && !letter('゛') && !letter('\u{1100}') && !letter('α'));
        assert!(
            allowed('，') && allowed('《') && allowed('😀') && allowed('\u{200D}') && allowed('€')
        );
        assert_eq!(
            check("你好，我们说中文。"),
            Ok("你好，我们说中文。".to_owned())
        );
        assert_eq!(
            check("안녕하세요 ㅋㅋㅋ 👍🏽"),
            Ok("안녕하세요 ㅋㅋㅋ 👍🏽".to_owned())
        );
        assert!(allowed('。') && allowed('、') && allowed('！') && allowed('「'));
        assert_eq!(
            check("こんにちは、日本語のチャットです。"),
            Ok("こんにちは、日本語のチャットです。".to_owned())
        );
        assert!(!letter('×') && !letter('÷') && !letter('😀'));
        assert!(!letter('1') && !letter(' '));
    }

    #[test]
    fn errors_have_the_hubs_codes() {
        assert_eq!(BioError::Length.code(), "bio_length");
        assert_eq!(BioError::Character.code(), "bio_characters");
        assert_eq!(BioError::Lines.code(), "bio_lines");
        assert_eq!(BioError::Noise.code(), "bio_noise");
    }
}
