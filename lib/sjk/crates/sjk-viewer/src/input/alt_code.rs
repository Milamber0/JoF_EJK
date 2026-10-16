//! Windows Alt codes: hold Alt, type a decimal number on the numeric keypad and
//! release Alt to type the character with that number.
//!
//! Windows composes the code in `TranslateMessage` and posts the character as a
//! `WM_CHAR` after the Alt release. winit 0.30 drops a `WM_CHAR` that no key press
//! is waiting for (`platform_impl/windows/keyboard.rs`: "The message is probably
//! IME"), so the client composes the code itself, with the same rules:
//!
//! - a number that starts with 0 is a Windows-1252 (ANSI) code: `Alt+0248` is `ø`;
//! - any other number is a code page 437 (US OEM) code: `Alt+21` is `§`,
//!   `Alt+155` is `¢`;
//! - the number counts modulo 256, and a code naming a control character types
//!   nothing.
//!
//! Only plain Alt composes. AltGr (reported as [`NamedKey::AltGraph`]) and Ctrl+Alt
//! do not, and another key pressed during the code cancels it. A code is composed
//! only while a text field (console, chat, a menu) has the keyboard; in play the
//! keypad keeps its bindings. The keypad digits of a code reach no field, so they
//! neither type nor walk the console history.

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey, SmolStr};

/// What to do with one key event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Step {
    /// Route the key as usual.
    Pass,
    /// A keypad digit of a code: no field sees it.
    Withhold,
    /// Route the key (the Alt release), then type this character.
    Type(char),
}

/// An Alt code in progress.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct AltCode {
    /// The Alt key that opened the code, while it is held.
    alt: Option<KeyCode>,
    /// The number so far, modulo 256, and whether it began with 0; `None`
    /// before the first digit.
    number: Option<(u8, bool)>,
    /// Ctrl is held: Ctrl+Alt is a shortcut, not a code.
    control: bool,
}

impl AltCode {
    /// Follow the window's Ctrl state.
    pub(crate) fn set_control(&mut self, held: bool) {
        self.control = held;
    }

    /// Forget a code in progress, as when the window loses focus.
    pub(crate) fn reset(&mut self) {
        *self = Self {
            control: self.control,
            ..Self::default()
        };
    }

    /// Decide what happens to `event`; `typing` says whether a text field has
    /// the keyboard.
    pub(crate) fn key(&mut self, event: &KeyEvent, typing: bool) -> Step {
        self.step(
            event.physical_key,
            &event.logical_key,
            event.state == ElementState::Pressed,
            event.repeat,
            typing,
        )
    }

    fn step(
        &mut self,
        physical: PhysicalKey,
        logical: &Key,
        pressed: bool,
        repeat: bool,
        typing: bool,
    ) -> Step {
        let PhysicalKey::Code(key) = physical else {
            return self.interrupt(pressed);
        };
        if matches!(key, KeyCode::AltLeft | KeyCode::AltRight) {
            if repeat {
                return Step::Pass;
            }
            if pressed {
                let plain = matches!(logical, Key::Named(NamedKey::Alt));
                self.alt = (typing && plain && !self.control).then_some(key);
                self.number = None;
                return Step::Pass;
            }
            if self.alt != Some(key) {
                return Step::Pass;
            }
            self.alt = None;
            return match self.number.take().and_then(character) {
                Some(character) if typing => Step::Type(character),
                _ => Step::Pass,
            };
        }
        if matches!(key, KeyCode::ShiftLeft | KeyCode::ShiftRight) {
            return Step::Pass;
        }
        let Some(digit) = keypad_digit(key).filter(|_| self.alt.is_some()) else {
            return self.interrupt(pressed);
        };
        if pressed && !repeat {
            let (number, ansi) = self.number.unwrap_or((0, digit == 0));
            self.number = Some((number.wrapping_mul(10).wrapping_add(digit), ansi));
        }
        Step::Withhold
    }

    /// A key other than Alt, Shift or a keypad digit pressed during a code
    /// cancels it.
    fn interrupt(&mut self, pressed: bool) -> Step {
        if pressed {
            self.alt = None;
            self.number = None;
        }
        Step::Pass
    }
}

/// The event that types `character`: a press of the released Alt key carrying
/// the character as its text, so every text field takes it like a typed key.
pub(crate) fn typed_event(release: &KeyEvent, character: char) -> KeyEvent {
    let text = SmolStr::new(character.encode_utf8(&mut [0; 4]));
    let mut event = release.clone();
    event.state = ElementState::Pressed;
    event.repeat = false;
    event.logical_key = Key::Character(text.clone());
    event.text = Some(text);
    event
}

fn keypad_digit(key: KeyCode) -> Option<u8> {
    Some(match key {
        KeyCode::Numpad0 => 0,
        KeyCode::Numpad1 => 1,
        KeyCode::Numpad2 => 2,
        KeyCode::Numpad3 => 3,
        KeyCode::Numpad4 => 4,
        KeyCode::Numpad5 => 5,
        KeyCode::Numpad6 => 6,
        KeyCode::Numpad7 => 7,
        KeyCode::Numpad8 => 8,
        KeyCode::Numpad9 => 9,
        _ => return None,
    })
}

/// The character of a finished code, or `None` for a control character.
fn character((number, ansi): (u8, bool)) -> Option<char> {
    let character = match (ansi, number) {
        (_, 0x20..=0x7e) => char::from(number),
        (true, 0x80..=0x9f) => WINDOWS_1252[usize::from(number - 0x80)],
        (true, _) => char::from(number),
        (false, 0x01..=0x1f) => CP437_LOW[usize::from(number - 1)],
        (false, 0x7f) => '\u{2302}',
        (false, 0x80..) => CP437_HIGH[usize::from(number - 0x80)],
        (false, 0) => return None,
    };
    (!character.is_control()).then_some(character)
}

/// Windows-1252 0x80..=0x9F. The five undefined slots keep their C1 control,
/// as `MultiByteToWideChar` does.
const WINDOWS_1252: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}',
    '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
];

/// Code page 437 0x01..=0x1F, as glyphs, the way Windows types them.
const CP437_LOW: [char; 31] = [
    '☺', '☻', '♥', '♦', '♣', '♠', '•', '◘', '○', '◙', '♂', '♀', '♪', '♫', '☼', '►', '◄', '↕', '‼',
    '¶', '§', '▬', '↨', '↑', '↓', '→', '←', '∟', '↔', '▲', '▼',
];

/// Code page 437 0x80..=0xFF.
const CP437_HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', //
    'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', //
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', //
    '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐', //
    '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧', //
    '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀', //
    'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', //
    '≡', '±', '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}', //
];

#[cfg(test)]
mod tests {
    use super::*;

    const ALT: Key = Key::Named(NamedKey::Alt);

    fn code(key: KeyCode) -> PhysicalKey {
        PhysicalKey::Code(key)
    }

    fn digit(n: char) -> KeyCode {
        [
            KeyCode::Numpad0,
            KeyCode::Numpad1,
            KeyCode::Numpad2,
            KeyCode::Numpad3,
            KeyCode::Numpad4,
            KeyCode::Numpad5,
            KeyCode::Numpad6,
            KeyCode::Numpad7,
            KeyCode::Numpad8,
            KeyCode::Numpad9,
        ][n.to_digit(10).expect("digit") as usize]
    }

    /// Hold Alt, press and release each digit of `digits`, release Alt; the
    /// steps of the digits and the step of the release.
    fn type_code(alt: &mut AltCode, digits: &str, typing: bool) -> (Vec<Step>, Step) {
        let none = Key::Unidentified(winit::keyboard::NativeKey::Unidentified);
        assert_eq!(
            alt.step(code(KeyCode::AltLeft), &ALT, true, false, typing),
            Step::Pass
        );
        let mut steps = Vec::new();
        for n in digits.chars() {
            steps.push(alt.step(code(digit(n)), &none, true, false, typing));
            steps.push(alt.step(code(digit(n)), &none, false, false, typing));
        }
        let release = alt.step(code(KeyCode::AltLeft), &ALT, false, false, typing);
        (steps, release)
    }

    fn typed(digits: &str) -> Step {
        type_code(&mut AltCode::default(), digits, true).1
    }

    #[test]
    fn leading_zero_is_windows_1252() {
        assert_eq!(typed("0248"), Step::Type('ø'));
        assert_eq!(typed("0167"), Step::Type('§'));
        assert_eq!(typed("0128"), Step::Type('€'));
        assert_eq!(typed("0153"), Step::Type('™'));
        assert_eq!(typed("065"), Step::Type('A'));
    }

    #[test]
    fn other_numbers_are_code_page_437() {
        assert_eq!(typed("21"), Step::Type('§'));
        assert_eq!(typed("3"), Step::Type('♥'));
        assert_eq!(typed("130"), Step::Type('é'));
        assert_eq!(typed("155"), Step::Type('¢'));
        assert_eq!(typed("250"), Step::Type('·'));
        assert_eq!(typed("65"), Step::Type('A'));
        assert_eq!(typed("127"), Step::Type('⌂'));
    }

    #[test]
    fn numbers_count_modulo_256() {
        assert_eq!(typed("321"), Step::Type('A'));
        assert_eq!(typed("0504"), Step::Type('ø'));
    }

    #[test]
    fn control_characters_type_nothing() {
        for digits in ["0", "00", "256", "09", "010", "013", "0127", "0129", "0157"] {
            assert_eq!(typed(digits), Step::Pass, "Alt+{digits}");
        }
    }

    #[test]
    fn digits_are_withheld_and_alt_alone_types_nothing() {
        let (steps, release) = type_code(&mut AltCode::default(), "0248", true);
        assert!(steps.iter().all(|step| *step == Step::Withhold));
        assert_eq!(release, Step::Type('ø'));
        assert_eq!(typed(""), Step::Pass);
    }

    #[test]
    fn play_keeps_the_keypad() {
        let (steps, release) = type_code(&mut AltCode::default(), "0248", false);
        assert!(steps.iter().all(|step| *step == Step::Pass));
        assert_eq!(release, Step::Pass);
    }

    #[test]
    fn altgr_and_ctrl_alt_do_not_compose() {
        let mut alt = AltCode::default();
        let none = Key::Unidentified(winit::keyboard::NativeKey::Unidentified);
        let altgr = Key::Named(NamedKey::AltGraph);
        alt.step(code(KeyCode::AltRight), &altgr, true, false, true);
        assert_eq!(
            alt.step(code(KeyCode::Numpad6), &none, true, false, true),
            Step::Pass
        );
        alt.step(code(KeyCode::AltRight), &altgr, false, false, true);

        alt.set_control(true);
        let (steps, release) = type_code(&mut alt, "65", true);
        assert!(steps.iter().all(|step| *step == Step::Pass));
        assert_eq!(release, Step::Pass);
    }

    #[test]
    fn another_key_cancels_the_code() {
        let mut alt = AltCode::default();
        let none = Key::Unidentified(winit::keyboard::NativeKey::Unidentified);
        let a = Key::Character(SmolStr::new("a"));
        alt.step(code(KeyCode::AltLeft), &ALT, true, false, true);
        alt.step(code(KeyCode::Numpad6), &none, true, false, true);
        assert_eq!(
            alt.step(code(KeyCode::KeyA), &a, true, false, true),
            Step::Pass
        );
        assert_eq!(
            alt.step(code(KeyCode::Numpad5), &none, true, false, true),
            Step::Pass
        );
        assert_eq!(
            alt.step(code(KeyCode::AltLeft), &ALT, false, false, true),
            Step::Pass
        );
    }

    #[test]
    fn shift_and_repeats_leave_the_code_alone() {
        let mut alt = AltCode::default();
        let none = Key::Unidentified(winit::keyboard::NativeKey::Unidentified);
        let shift = Key::Named(NamedKey::Shift);
        alt.step(code(KeyCode::AltLeft), &ALT, true, false, true);
        alt.step(code(KeyCode::Numpad6), &none, true, false, true);
        alt.step(code(KeyCode::Numpad6), &none, true, true, true);
        alt.step(code(KeyCode::AltLeft), &ALT, true, true, true);
        alt.step(code(KeyCode::ShiftLeft), &shift, true, false, true);
        alt.step(code(KeyCode::Numpad5), &none, true, false, true);
        assert_eq!(
            alt.step(code(KeyCode::AltLeft), &ALT, false, false, true),
            Step::Type('A')
        );
    }

    #[test]
    fn focus_loss_forgets_the_code() {
        let mut alt = AltCode::default();
        let none = Key::Unidentified(winit::keyboard::NativeKey::Unidentified);
        alt.step(code(KeyCode::AltLeft), &ALT, true, false, true);
        alt.step(code(KeyCode::Numpad6), &none, true, false, true);
        alt.reset();
        assert_eq!(
            alt.step(code(KeyCode::AltLeft), &ALT, false, false, true),
            Step::Pass
        );
    }
}
