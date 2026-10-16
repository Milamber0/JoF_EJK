//! Single platform adapter for console key names.
//!
//! Stock JA names a key by what the active layout prints on it: SDL2 and the
//! retail Windows input both turn the scan code into the layout's unshifted
//! character (`sdl_input.cpp` `IN_TranslateSDLToJKKey`, `MapVirtualKey`), so
//! `bind w` is the key labelled W on AZERTY too. Keys without a character
//! (arrows, F-keys, keypad, modifiers) keep fixed names, and the digit row
//! stays `0`-`9` on every layout, as SDL2 keeps it for French layouts.
use winit::event::{KeyEvent, MouseButton};
use winit::keyboard::{Key, KeyCode, PhysicalKey};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

/// Physical sources with stock bind names.
pub(crate) enum Source {
    Key(KeyCode),
    Mouse(MouseButton),
    Wheel(bool),
}

/// A bind name for one key press, kept inline so input stays allocation-free.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum KeyName {
    /// A stock name such as `SPACE`, `KP_ENTER` or a US-position fallback.
    Fixed(&'static str),
    /// The layout's lowercase character for the key, UTF-8 encoded.
    Character { utf8: [u8; 4], len: u8 },
}

impl KeyName {
    fn character(character: char) -> Self {
        let mut utf8 = [0; 4];
        let len = character.encode_utf8(&mut utf8).len() as u8;
        Self::Character { utf8, len }
    }

    /// The name as `bind` accepts it and the config stores it.
    pub(crate) fn as_str(&self) -> &str {
        match self {
            Self::Fixed(name) => name,
            Self::Character { utf8, len } => {
                std::str::from_utf8(&utf8[..usize::from(*len)]).unwrap_or_default()
            }
        }
    }
}

/// Bind name of a keyboard event under the active layout.
pub(crate) fn key_name(event: &KeyEvent) -> Option<KeyName> {
    let PhysicalKey::Code(code) = event.physical_key else {
        return None;
    };
    layout_name(code, &event.key_without_modifiers())
}

/// Name `code`, given what the layout puts on it without modifiers.
///
/// Character keys take that character; dead keys report theirs too, so `^` on
/// AZERTY is always `^`. Letter positions whose character is outside the Latin
/// scripts keep their US letter, like Windows virtual keys on Cyrillic or Greek
/// layouts, so the default `w`/`a`/`s`/`d` binds still reach a key there.
pub(crate) fn layout_name(code: KeyCode, unshifted: &Key) -> Option<KeyName> {
    let fixed = || name(Source::Key(code)).map(KeyName::Fixed);
    let Some(letter_position) = character_position(code) else {
        return fixed();
    };
    single_character(unshifted)
        .and_then(|character| character_name(character, letter_position))
        .or_else(fixed)
}

fn single_character(key: &Key) -> Option<char> {
    match key {
        Key::Character(text) => {
            let mut characters = text.chars();
            match (characters.next(), characters.next()) {
                (Some(character), None) => Some(character),
                _ => None,
            }
        }
        Key::Dead(character) => *character,
        _ => None,
    }
}

/// Whether `code` is a key the layout assigns a character to, and if so
/// whether it is one of the 26 letter positions. The digit row is not: its
/// keys keep their digit names on every layout.
fn character_position(code: KeyCode) -> Option<bool> {
    match code {
        KeyCode::KeyA
        | KeyCode::KeyB
        | KeyCode::KeyC
        | KeyCode::KeyD
        | KeyCode::KeyE
        | KeyCode::KeyF
        | KeyCode::KeyG
        | KeyCode::KeyH
        | KeyCode::KeyI
        | KeyCode::KeyJ
        | KeyCode::KeyK
        | KeyCode::KeyL
        | KeyCode::KeyM
        | KeyCode::KeyN
        | KeyCode::KeyO
        | KeyCode::KeyP
        | KeyCode::KeyQ
        | KeyCode::KeyR
        | KeyCode::KeyS
        | KeyCode::KeyT
        | KeyCode::KeyU
        | KeyCode::KeyV
        | KeyCode::KeyW
        | KeyCode::KeyX
        | KeyCode::KeyY
        | KeyCode::KeyZ => Some(true),
        KeyCode::Backquote
        | KeyCode::Backslash
        | KeyCode::BracketLeft
        | KeyCode::BracketRight
        | KeyCode::Comma
        | KeyCode::Equal
        | KeyCode::IntlBackslash
        | KeyCode::IntlRo
        | KeyCode::IntlYen
        | KeyCode::Minus
        | KeyCode::Period
        | KeyCode::Quote
        | KeyCode::Semicolon
        | KeyCode::Slash => Some(false),
        _ => None,
    }
}

fn character_name(character: char, letter_position: bool) -> Option<KeyName> {
    let mut lower = character.to_lowercase();
    let character = match (lower.next(), lower.next()) {
        (Some(lower), None) => lower,
        _ => character,
    };
    if character == ';' {
        return Some(KeyName::Fixed("SEMICOLON"));
    }
    if character.is_ascii_graphic() {
        return Some(KeyName::character(character));
    }
    if character.is_control() || character.is_whitespace() {
        return None;
    }
    // Latin-1 Supplement and Latin Extended-A/B hold the accented letters of
    // Latin-script layouts (é, ß, ı, ś); letters of other scripts keep the US letter.
    if letter_position && character > '\u{24F}' {
        return None;
    }
    Some(KeyName::character(character))
}

/// Names that held keys were pressed under, by physical key.
///
/// A release resolves to the name of its press, so a layout switch while a
/// key is down cannot leave a `+button` held. Fixed capacity: a press beyond
/// it reuses the last slot.
#[derive(Clone, Copy)]
pub(crate) struct HeldKeys {
    slots: [Option<(KeyCode, KeyName)>; 16],
}

impl Default for HeldKeys {
    fn default() -> Self {
        Self { slots: [None; 16] }
    }
}

impl HeldKeys {
    /// Remember the name `code` was pressed under.
    pub(crate) fn press(&mut self, code: KeyCode, name: KeyName) {
        let index = self
            .slots
            .iter()
            .position(|slot| slot.is_some_and(|(held, _)| held == code))
            .or_else(|| self.slots.iter().position(Option::is_none))
            .unwrap_or(self.slots.len() - 1);
        self.slots[index] = Some((code, name));
    }

    /// The name the held `code` was pressed under.
    pub(crate) fn name(&self, code: KeyCode) -> Option<KeyName> {
        self.slots
            .iter()
            .flatten()
            .find(|(held, _)| *held == code)
            .map(|(_, name)| *name)
    }

    /// Forget `code` and return the name it was pressed under.
    pub(crate) fn release(&mut self, code: KeyCode) -> Option<KeyName> {
        self.slots
            .iter_mut()
            .find(|slot| slot.is_some_and(|(held, _)| held == code))?
            .take()
            .map(|(_, name)| name)
    }
}

/// Fixed stock name of a mouse source, or of a key at its US-layout position.
///
/// Keyboard binds go through [`key_name`], which prefers the layout's
/// character; this table names the keys without one and is the fallback when
/// a layout reports none.
pub(crate) fn name(source: Source) -> Option<&'static str> {
    Some(match source {
        Source::Key(key) => match key {
            KeyCode::KeyA => "a",
            KeyCode::KeyB => "b",
            KeyCode::KeyC => "c",
            KeyCode::KeyD => "d",
            KeyCode::KeyE => "e",
            KeyCode::KeyF => "f",
            KeyCode::KeyG => "g",
            KeyCode::KeyH => "h",
            KeyCode::KeyI => "i",
            KeyCode::KeyJ => "j",
            KeyCode::KeyK => "k",
            KeyCode::KeyL => "l",
            KeyCode::KeyM => "m",
            KeyCode::KeyN => "n",
            KeyCode::KeyO => "o",
            KeyCode::KeyP => "p",
            KeyCode::KeyQ => "q",
            KeyCode::KeyR => "r",
            KeyCode::KeyS => "s",
            KeyCode::KeyT => "t",
            KeyCode::KeyU => "u",
            KeyCode::KeyV => "v",
            KeyCode::KeyW => "w",
            KeyCode::KeyX => "x",
            KeyCode::KeyY => "y",
            KeyCode::KeyZ => "z",
            KeyCode::Digit0 => "0",
            KeyCode::Digit1 => "1",
            KeyCode::Digit2 => "2",
            KeyCode::Digit3 => "3",
            KeyCode::Digit4 => "4",
            KeyCode::Digit5 => "5",
            KeyCode::Digit6 => "6",
            KeyCode::Digit7 => "7",
            KeyCode::Digit8 => "8",
            KeyCode::Digit9 => "9",
            KeyCode::ControlLeft => "CTRL",
            KeyCode::ControlRight => "CTRL",
            KeyCode::ShiftLeft => "SHIFT",
            KeyCode::ShiftRight => "SHIFT",
            KeyCode::AltLeft => "ALT",
            KeyCode::AltRight => "ALT",
            KeyCode::ArrowUp => "UPARROW",
            KeyCode::ArrowDown => "DOWNARROW",
            KeyCode::ArrowLeft => "LEFTARROW",
            KeyCode::ArrowRight => "RIGHTARROW",
            KeyCode::Insert => "INS",
            KeyCode::Delete => "DEL",
            KeyCode::PageUp => "PGUP",
            KeyCode::PageDown => "PGDN",
            KeyCode::Backquote => "`",
            KeyCode::BracketLeft => "[",
            KeyCode::BracketRight => "]",
            KeyCode::Backslash => "\\",
            KeyCode::IntlBackslash => "\\",
            KeyCode::Comma => ",",
            KeyCode::Period => ".",
            KeyCode::Slash => "/",
            KeyCode::Minus => "-",
            KeyCode::Equal => "=",
            KeyCode::Quote => "'",
            KeyCode::Semicolon => "SEMICOLON",
            KeyCode::NumLock => "KP_NUMLOCK",
            KeyCode::NumpadEnter => "KP_ENTER",
            KeyCode::NumpadAdd => "KP_PLUS",
            KeyCode::NumpadSubtract => "KP_MINUS",
            KeyCode::NumpadMultiply => "KP_STAR",
            KeyCode::NumpadDivide => "KP_SLASH",
            KeyCode::NumpadEqual => "KP_EQUALS",
            KeyCode::NumpadDecimal => "KP_DEL",
            KeyCode::Numpad0 => "KP_INS",
            KeyCode::Numpad1 => "KP_END",
            KeyCode::Numpad2 => "KP_DOWNARROW",
            KeyCode::Numpad3 => "KP_PGDN",
            KeyCode::Numpad4 => "KP_LEFTARROW",
            KeyCode::Numpad5 => "KP_5",
            KeyCode::Numpad6 => "KP_RIGHTARROW",
            KeyCode::Numpad7 => "KP_HOME",
            KeyCode::Numpad8 => "KP_UPARROW",
            KeyCode::Numpad9 => "KP_PGUP",
            KeyCode::Enter => "ENTER",
            KeyCode::Escape => "ESCAPE",
            KeyCode::Space => "SPACE",
            KeyCode::Backspace => "BACKSPACE",
            KeyCode::Tab => "TAB",
            KeyCode::CapsLock => "CAPSLOCK",
            KeyCode::Pause => "PAUSE",
            KeyCode::ScrollLock => "SCROLLLOCK",
            KeyCode::Home => "HOME",
            KeyCode::End => "END",
            KeyCode::F1 => "F1",
            KeyCode::F2 => "F2",
            KeyCode::F3 => "F3",
            KeyCode::F4 => "F4",
            KeyCode::F5 => "F5",
            KeyCode::F6 => "F6",
            KeyCode::F7 => "F7",
            KeyCode::F8 => "F8",
            KeyCode::F9 => "F9",
            KeyCode::F10 => "F10",
            KeyCode::F11 => "F11",
            KeyCode::F12 => "F12",
            _ => return None,
        },
        Source::Mouse(button) => match button {
            MouseButton::Left => "MOUSE1",
            MouseButton::Right => "MOUSE2",
            MouseButton::Middle => "MOUSE3",
            MouseButton::Back => "MOUSE4",
            MouseButton::Forward => "MOUSE5",
            _ => return None,
        },
        Source::Wheel(up) => {
            if up {
                "MWHEELUP"
            } else {
                "MWHEELDOWN"
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{HeldKeys, KeyName, layout_name};
    use winit::keyboard::{Key, KeyCode, NamedKey, NativeKey, SmolStr};

    fn named(code: KeyCode, key: Key) -> Option<String> {
        layout_name(code, &key).map(|name| name.as_str().to_owned())
    }

    fn typed(code: KeyCode, text: &str) -> Option<String> {
        named(code, Key::Character(SmolStr::new(text)))
    }

    #[test]
    fn letters_follow_the_layout() {
        // French AZERTY: the US W position is labelled Z and the US Z position W.
        assert_eq!(typed(KeyCode::KeyW, "z").as_deref(), Some("z"));
        assert_eq!(typed(KeyCode::KeyZ, "w").as_deref(), Some("w"));
        assert_eq!(typed(KeyCode::KeyQ, "a").as_deref(), Some("a"));
        assert_eq!(typed(KeyCode::KeyA, "q").as_deref(), Some("q"));
        assert_eq!(typed(KeyCode::KeyW, "W").as_deref(), Some("w"));
        // US layout: unchanged names.
        assert_eq!(typed(KeyCode::KeyW, "w").as_deref(), Some("w"));
    }

    #[test]
    fn punctuation_follows_the_layout() {
        assert_eq!(typed(KeyCode::IntlBackslash, "<").as_deref(), Some("<"));
        assert_eq!(typed(KeyCode::KeyM, ",").as_deref(), Some(","));
        assert_eq!(typed(KeyCode::Comma, ";").as_deref(), Some("SEMICOLON"));
        assert_eq!(typed(KeyCode::Slash, "!").as_deref(), Some("!"));
        assert_eq!(typed(KeyCode::Quote, "ù").as_deref(), Some("ù"));
        assert_eq!(typed(KeyCode::Backquote, "²").as_deref(), Some("²"));
        // German: ß and - sit on different keys and must not share a name.
        assert_eq!(typed(KeyCode::Minus, "ß").as_deref(), Some("ß"));
        assert_eq!(typed(KeyCode::Slash, "-").as_deref(), Some("-"));
        // UK: the ISO key types a backslash and the key beside Enter a hash.
        assert_eq!(typed(KeyCode::IntlBackslash, "\\").as_deref(), Some("\\"));
        assert_eq!(typed(KeyCode::Backslash, "#").as_deref(), Some("#"));
    }

    #[test]
    fn dead_keys_have_a_stable_name() {
        let dead = named(KeyCode::BracketLeft, Key::Dead(Some('^')));
        assert_eq!(dead.as_deref(), Some("^"));
        assert_eq!(typed(KeyCode::BracketLeft, "^").as_deref(), Some("^"));
        assert_eq!(
            named(KeyCode::BracketLeft, Key::Dead(None)).as_deref(),
            Some("[")
        );
    }

    #[test]
    fn digits_and_named_keys_stay_fixed() {
        // AZERTY types & and é on the digit row; binds keep their digits.
        assert_eq!(typed(KeyCode::Digit1, "&").as_deref(), Some("1"));
        assert_eq!(typed(KeyCode::Digit2, "é").as_deref(), Some("2"));
        assert_eq!(typed(KeyCode::Numpad1, "1").as_deref(), Some("KP_END"));
        let space = named(KeyCode::Space, Key::Named(NamedKey::Space));
        assert_eq!(space.as_deref(), Some("SPACE"));
        let enter = named(KeyCode::Enter, Key::Named(NamedKey::Enter));
        assert_eq!(enter.as_deref(), Some("ENTER"));
    }

    #[test]
    fn non_latin_letters_keep_the_us_letter() {
        // Russian: the US W position types ц; the period key types ю.
        assert_eq!(typed(KeyCode::KeyW, "ц").as_deref(), Some("w"));
        assert_eq!(typed(KeyCode::Period, "ю").as_deref(), Some("ю"));
        assert_eq!(typed(KeyCode::Slash, ".").as_deref(), Some("."));
        // Turkish Q: dotless i is a Latin letter and names its own key.
        assert_eq!(typed(KeyCode::KeyI, "ı").as_deref(), Some("ı"));
    }

    #[test]
    fn missing_characters_fall_back_to_the_us_position() {
        let unidentified = Key::Unidentified(NativeKey::Unidentified);
        assert_eq!(named(KeyCode::KeyW, unidentified).as_deref(), Some("w"));
        assert_eq!(typed(KeyCode::Quote, "ab").as_deref(), Some("'"));
        assert_eq!(typed(KeyCode::Quote, "\u{7f}").as_deref(), Some("'"));
    }

    #[test]
    fn releases_use_the_pressed_name() {
        let mut held = HeldKeys::default();
        let pressed = layout_name(KeyCode::KeyW, &Key::Character(SmolStr::new("z"))).unwrap();
        held.press(KeyCode::KeyW, pressed);
        assert_eq!(held.release(KeyCode::KeyW), Some(pressed));
        assert_eq!(held.release(KeyCode::KeyW), None);
        assert_eq!(
            pressed,
            KeyName::Character {
                utf8: *b"z\0\0\0",
                len: 1
            }
        );
    }

    #[test]
    fn a_full_table_still_records_new_presses() {
        let mut held = HeldKeys::default();
        let name = KeyName::Fixed("F1");
        for code in [KeyCode::F1; 16] {
            held.press(code, name);
        }
        let codes = [
            KeyCode::KeyA,
            KeyCode::KeyB,
            KeyCode::KeyC,
            KeyCode::KeyD,
            KeyCode::KeyE,
            KeyCode::KeyF,
            KeyCode::KeyG,
            KeyCode::KeyH,
            KeyCode::KeyI,
            KeyCode::KeyJ,
            KeyCode::KeyK,
            KeyCode::KeyL,
            KeyCode::KeyM,
            KeyCode::KeyN,
            KeyCode::KeyO,
            KeyCode::KeyP,
            KeyCode::KeyQ,
        ];
        for code in codes {
            held.press(code, name);
        }
        assert_eq!(held.release(KeyCode::KeyQ), Some(name));
    }
}
