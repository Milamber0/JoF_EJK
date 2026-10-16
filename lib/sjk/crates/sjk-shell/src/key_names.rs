//! Case-insensitive Quake console key names and pre-stock viewer aliases.
//! OpenJK codemp/client/cl_keys.cpp:51-381,870-920.

/// Canonical named keys; printable ASCII keys additionally name themselves.
pub const KEY_NAMES: &[&str] = &[
    "SEMICOLON",
    "UPARROW",
    "DOWNARROW",
    "LEFTARROW",
    "RIGHTARROW",
    "CTRL",
    "SHIFT",
    "ALT",
    "ENTER",
    "ESCAPE",
    "SPACE",
    "BACKSPACE",
    "TAB",
    "CAPSLOCK",
    "SCROLLLOCK",
    "PAUSE",
    "INS",
    "DEL",
    "PGDN",
    "PGUP",
    "HOME",
    "END",
    "CONSOLE",
    "EURO",
    "CIRCUMFLEX",
    "SHIFT_ENTER",
    "SHIFT_SPACE",
    "SHIFT_KP_ENTER",
    "F1",
    "F2",
    "F3",
    "F4",
    "F5",
    "F6",
    "F7",
    "F8",
    "F9",
    "F10",
    "F11",
    "F12",
    "MOUSE1",
    "MOUSE2",
    "MOUSE3",
    "MOUSE4",
    "MOUSE5",
    "MWHEELUP",
    "MWHEELDOWN",
    "KP_NUMLOCK",
    "KP_ENTER",
    "KP_PLUS",
    "KP_MINUS",
    "KP_STAR",
    "KP_SLASH",
    "KP_EQUALS",
    "KP_DEL",
    "KP_INS",
    "KP_END",
    "KP_DOWNARROW",
    "KP_PGDN",
    "KP_LEFTARROW",
    "KP_5",
    "KP_RIGHTARROW",
    "KP_HOME",
    "KP_UPARROW",
    "KP_PGUP",
    "JOY0",
    "JOY1",
    "JOY2",
    "JOY3",
    "JOY4",
    "JOY5",
    "JOY6",
    "JOY7",
    "JOY8",
    "JOY9",
    "JOY10",
    "JOY11",
    "JOY12",
    "JOY13",
    "JOY14",
    "JOY15",
    "JOY16",
    "JOY17",
    "JOY18",
    "JOY19",
    "JOY20",
    "JOY21",
    "JOY22",
    "JOY23",
    "JOY24",
    "JOY25",
    "JOY26",
    "JOY27",
    "JOY28",
    "JOY29",
    "JOY30",
    "JOY31",
    "AUX0",
    "AUX1",
    "AUX2",
    "AUX3",
    "AUX4",
    "AUX5",
    "AUX6",
    "AUX7",
    "AUX8",
    "AUX9",
    "AUX10",
    "AUX11",
    "AUX12",
    "AUX13",
    "AUX14",
    "AUX15",
    "AUX16",
    "AUX17",
    "AUX18",
    "AUX19",
    "AUX20",
    "AUX21",
    "AUX22",
    "AUX23",
    "AUX24",
    "AUX25",
    "AUX26",
    "AUX27",
    "AUX28",
    "AUX29",
    "AUX30",
    "AUX31",
];

const ALIASES: &[(&str, &str)] = &[
    ("KeyA", "a"),
    ("KeyB", "b"),
    ("KeyC", "c"),
    ("KeyD", "d"),
    ("KeyE", "e"),
    ("KeyF", "f"),
    ("KeyG", "g"),
    ("KeyH", "h"),
    ("KeyI", "i"),
    ("KeyJ", "j"),
    ("KeyK", "k"),
    ("KeyL", "l"),
    ("KeyM", "m"),
    ("KeyN", "n"),
    ("KeyO", "o"),
    ("KeyP", "p"),
    ("KeyQ", "q"),
    ("KeyR", "r"),
    ("KeyS", "s"),
    ("KeyT", "t"),
    ("KeyU", "u"),
    ("KeyV", "v"),
    ("KeyW", "w"),
    ("KeyX", "x"),
    ("KeyY", "y"),
    ("KeyZ", "z"),
    ("Digit0", "0"),
    ("Digit1", "1"),
    ("Digit2", "2"),
    ("Digit3", "3"),
    ("Digit4", "4"),
    ("Digit5", "5"),
    ("Digit6", "6"),
    ("Digit7", "7"),
    ("Digit8", "8"),
    ("Digit9", "9"),
    ("ControlLeft", "CTRL"),
    ("ControlRight", "CTRL"),
    ("ShiftLeft", "SHIFT"),
    ("ShiftRight", "SHIFT"),
    ("AltLeft", "ALT"),
    ("AltRight", "ALT"),
    ("ArrowUp", "UPARROW"),
    ("ArrowDown", "DOWNARROW"),
    ("ArrowLeft", "LEFTARROW"),
    ("ArrowRight", "RIGHTARROW"),
    ("Insert", "INS"),
    ("Delete", "DEL"),
    ("PageUp", "PGUP"),
    ("PageDown", "PGDN"),
    ("Backquote", "`"),
    ("BracketLeft", "["),
    ("BracketRight", "]"),
    ("Backslash", "\\"),
    ("IntlBackslash", "\\"),
    ("Comma", ","),
    ("Period", "."),
    ("Slash", "/"),
    ("Minus", "-"),
    ("Equal", "="),
    ("Quote", "'"),
    ("Semicolon", "SEMICOLON"),
    ("NumLock", "KP_NUMLOCK"),
    ("NumpadEnter", "KP_ENTER"),
    ("NumpadAdd", "KP_PLUS"),
    ("NumpadSubtract", "KP_MINUS"),
    ("NumpadMultiply", "KP_STAR"),
    ("NumpadDivide", "KP_SLASH"),
    ("NumpadEqual", "KP_EQUALS"),
    ("NumpadDecimal", "KP_DEL"),
    ("Numpad0", "KP_INS"),
    ("Numpad1", "KP_END"),
    ("Numpad2", "KP_DOWNARROW"),
    ("Numpad3", "KP_PGDN"),
    ("Numpad4", "KP_LEFTARROW"),
    ("Numpad5", "KP_5"),
    ("Numpad6", "KP_RIGHTARROW"),
    ("Numpad7", "KP_HOME"),
    ("Numpad8", "KP_UPARROW"),
    ("Numpad9", "KP_PGUP"),
    ("WheelUp", "MWHEELUP"),
    ("WheelDown", "MWHEELDOWN"),
];

// Key_WriteBindings (cl_keys.cpp:1110-1115) writes a bare backslash inside
// quotes. Only reinterpret that key token; other script string escapes remain intact.
pub(crate) fn normalize_stock_backslash(input: &str) -> std::borrow::Cow<'_, str> {
    let trimmed = input.trim_start();
    let Some(end) = trimmed.find(char::is_whitespace) else {
        return input.into();
    };
    let command = &trimmed[..end];
    let rest = trimmed[end..].trim_start();
    if (command.eq_ignore_ascii_case("bind") || command.eq_ignore_ascii_case("unbind"))
        && rest.as_bytes().starts_with(&[b'"', b'\\', b'"'])
        && rest.as_bytes().get(3).is_none_or(u8::is_ascii_whitespace)
    {
        return format!("{command} Backslash{}", &rest[3..]).into();
    }
    input.into()
}

/// A key name as shown to players: ASCII letters in uppercase, as retail's
/// controls menu shows them (`ui_shared.c` `BindingFromName` upper-cases
/// with `Q_strupr`). Only ASCII is changed, as `Q_strupr` does, so layout
/// names such as `é` keep their character. Configs and `bind` arguments keep
/// the canonical spelling; matching is case-insensitive either way.
pub fn display_key(key: &str) -> std::borrow::Cow<'_, str> {
    if key.bytes().any(|byte| byte.is_ascii_lowercase()) {
        key.to_ascii_uppercase().into()
    } else {
        key.into()
    }
}

/// Append [`display_key`] of `key` to `output` without allocating.
pub fn push_display_key(output: &mut String, key: &str) {
    output.extend(key.chars().map(|character| character.to_ascii_uppercase()));
}

/// Resolve a stock name or old viewer alias to the spelling saved in configs.
pub fn canonical_key(key: &str) -> Option<&str> {
    for (alias, name) in [
        ("SHIFT_ENTER", "ENTER"),
        ("SHIFT_SPACE", "SPACE"),
        ("SHIFT_KP_ENTER", "KP_ENTER"),
    ] {
        if key.eq_ignore_ascii_case(alias) {
            return Some(name);
        }
    }
    if key == ";" {
        return Some("SEMICOLON");
    }
    if key.len() == 1 && key.as_bytes()[0].is_ascii_graphic() {
        let byte = key.as_bytes()[0];
        if byte.is_ascii_uppercase() {
            const LETTERS: [&str; 26] = [
                "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p",
                "q", "r", "s", "t", "u", "v", "w", "x", "y", "z",
            ];
            return Some(LETTERS[(byte - b'A') as usize]);
        }
        return Some(key);
    }
    // Keys a layout labels outside ASCII (é, ù, ², ß) are named by that
    // character, as stock names its Latin-1 keys (cl_keys.cpp:1003-1015).
    let mut characters = key.chars();
    if let (Some(character), None) = (characters.next(), characters.next())
        && !character.is_ascii()
        && !character.is_control()
        && !character.is_whitespace()
    {
        return Some(key);
    }
    KEY_NAMES
        .iter()
        .copied()
        .find(|name| name.eq_ignore_ascii_case(key))
        .or_else(|| {
            ALIASES
                .iter()
                .find(|(alias, _)| alias.eq_ignore_ascii_case(key))
                .map(|(_, name)| *name)
        })
}

#[cfg(test)]
mod display_tests {
    use super::*;

    #[test]
    fn display_names_are_ascii_uppercase() {
        assert_eq!(display_key("w"), "W");
        assert_eq!(display_key("MOUSE1"), "MOUSE1");
        assert_eq!(display_key("kp_enter"), "KP_ENTER");
        assert_eq!(display_key("é"), "é");
        assert_eq!(display_key(";"), ";");
        let mut output = String::from("a / ");
        push_display_key(&mut output, "mwheelup");
        assert_eq!(output, "a / MWHEELUP");
    }

    #[test]
    fn display_names_still_resolve_to_the_saved_spelling() {
        for key in ["w", "space", "MOUSE1", "kp_enter"] {
            assert_eq!(canonical_key(&display_key(key)), canonical_key(key));
        }
    }
}
