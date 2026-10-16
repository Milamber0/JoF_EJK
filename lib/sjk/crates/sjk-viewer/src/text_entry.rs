//! Typing, pasting and copying in a one-line field of the menus: the server address and
//! password prompts, the browser's search. The caret is at the end of the text.
//!
//! Every text field takes Ctrl+V (and Shift+Insert) to paste, Ctrl+C (and Ctrl+Insert) to
//! copy and a held Backspace to delete on, as the console and the SJK chat do (Sol's rule,
//! 12/10/2026, `AGENTS.md`). The menu lets key repeats through for a field that is open
//! ([`crate::menu::ClientMenu::handle_key`]); this module is what such a field calls.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use winit::event::KeyEvent;
use winit::keyboard::{Key, KeyCode, SmolStr};

/// Ctrl+C on Windows types this character.
const CONTROL_C: &str = "\u{3}";
/// Ctrl+A on Windows types this character.
const CONTROL_A: &str = "\u{1}";

/// Whether a field's whole text is selected (Ctrl+A), and which text: its address and
/// length, so a selection never outlives a change of field or of text. A one-line field
/// has no caret movement, so a selection is all or nothing: Backspace or Delete clears the
/// field, typed text or a paste replaces it, Ctrl+C copies it, any other key drops it.
static SELECTED: AtomicBool = AtomicBool::new(false);
static SELECTED_AT: AtomicUsize = AtomicUsize::new(0);
static SELECTED_LEN: AtomicUsize = AtomicUsize::new(0);

/// Whether a pressed `key` that typed `text` selects all: Ctrl+A. Like
/// [`crate::sjk_chat_field::is_paste`], Windows may hand the letter without Ctrl, or the
/// Ctrl character itself.
pub(crate) fn is_select_all(key: KeyCode, text: Option<&str>, control: bool) -> bool {
    text == Some(CONTROL_A)
        || (control && text.is_some_and(|text| text.eq_ignore_ascii_case("a")))
        || (control && key == KeyCode::KeyA && text.is_none())
}

/// Select all of `field`'s text (nothing to select in an empty field).
pub(crate) fn select_all(field: &str) {
    if field.is_empty() {
        deselect();
        return;
    }
    SELECTED_AT.store(field.as_ptr() as usize, Ordering::Relaxed);
    SELECTED_LEN.store(field.len(), Ordering::Relaxed);
    SELECTED.store(true, Ordering::Relaxed);
}

/// Drop the selection.
pub(crate) fn deselect() {
    SELECTED.store(false, Ordering::Relaxed);
}

/// Whether all of `field`'s text is selected.
pub(crate) fn is_selected(field: &str) -> bool {
    SELECTED.load(Ordering::Relaxed)
        && SELECTED_AT.load(Ordering::Relaxed) == field.as_ptr() as usize
        && SELECTED_LEN.load(Ordering::Relaxed) == field.len()
}

/// Whether some field's text is selected: the field being typed in draws it highlighted.
pub(crate) fn selection_shown() -> bool {
    SELECTED.load(Ordering::Relaxed)
}

/// Whether `text` types characters (not Ctrl+letter, not a control character).
fn types(text: Option<&str>, control: bool) -> bool {
    !control && text.is_some_and(|text| !text.is_empty() && text.chars().all(|c| !c.is_control()))
}

/// What the selection does to the key about to edit `field`: Ctrl+A selects all of it;
/// with all of it selected Backspace and Delete clear it, and typed text or a paste
/// replaces it (the field is emptied and the key goes on typing); Ctrl+C keeps the selection
/// for the caller to copy; any other key drops it. Whether the key is spent.
pub(crate) fn before_edit(
    field: &mut String,
    key: KeyCode,
    text: Option<&str>,
    shift: bool,
    control: bool,
) -> bool {
    if is_select_all(key, text, control) {
        select_all(field);
        return true;
    }
    if !is_selected(field) || is_copy(key, text, control) {
        return false;
    }
    let erases = matches!(key, KeyCode::Backspace | KeyCode::Delete);
    if erases || types(text, control) || crate::sjk_chat_field::is_paste(key, text, shift, control)
    {
        field.clear();
    }
    deselect();
    erases
}

/// Whether a pressed `key` that typed `text` copies: Ctrl+C, or Ctrl+Insert. Like
/// [`crate::sjk_chat_field::is_paste`], Windows may hand the letter without Ctrl, or the
/// Ctrl character itself.
pub(crate) fn is_copy(key: KeyCode, text: Option<&str>, control: bool) -> bool {
    text == Some(CONTROL_C)
        || (control && text.is_some_and(|text| text.eq_ignore_ascii_case("c")))
        || (control && key == KeyCode::KeyC && text.is_none())
        || (control && key == KeyCode::Insert)
}

/// The event a paste types in place of `event` (the Ctrl+V press): the clipboard's first
/// line, trimmed and without control characters, as the text of the key. Handlers that read
/// `event.text` take it like typing. `None` when the clipboard has no text to type.
pub(crate) fn pasted_event(event: &KeyEvent, clipboard: &str) -> Option<KeyEvent> {
    let line = clipboard.lines().next().unwrap_or_default().trim();
    let text: String = line.chars().filter(|c| !c.is_control()).collect();
    if text.is_empty() {
        return None;
    }
    let mut typed = event.clone();
    typed.repeat = false;
    typed.logical_key = Key::Character(SmolStr::new(&text));
    typed.text = Some(SmolStr::new(&text));
    Some(typed)
}

/// A pressed `key` that typed `text` into `field`, up to `max` characters: pasted from the
/// clipboard, copied to it (when `copyable`: a password is not), Backspace taking the last
/// character back, or typed (control characters left out). Whether `field` changed.
pub(crate) fn edit(
    field: &mut String,
    key: KeyCode,
    text: Option<&str>,
    shift: bool,
    control: bool,
    max: usize,
    copyable: bool,
) -> bool {
    let before = field.len();
    if before_edit(field, key, text, shift, control) {
        return field.len() != before;
    }
    if key == KeyCode::Backspace {
        return field.pop().is_some();
    }
    if crate::sjk_chat_field::is_paste(key, text, shift, control) {
        return crate::console::clipboard::paste()
            .is_some_and(|clipboard| paste_into(field, &clipboard, max));
    }
    if is_copy(key, text, control) {
        if copyable && !field.is_empty() {
            let _ = crate::console::clipboard::copy(field);
        }
        return false;
    }
    text.is_some_and(|text| type_into(field, text, max))
}

/// Typed `text` added to `field`, control characters left out, up to `max` characters.
fn type_into(field: &mut String, text: &str, max: usize) -> bool {
    let before = field.len();
    let room = max.saturating_sub(field.chars().count());
    field.extend(text.chars().filter(|c| !c.is_control()).take(room));
    field.len() != before
}

/// Pasted `text` added to `field`: its first line, trimmed (an address copied with a line
/// break after it pastes clean), control characters left out, up to `max` characters.
fn paste_into(field: &mut String, text: &str, max: usize) -> bool {
    let line = text.lines().next().unwrap_or_default().trim();
    type_into(field, line, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(field: &str, text: &str, max: usize) -> String {
        let mut field = field.to_owned();
        type_into(&mut field, text, max);
        field
    }

    #[test]
    fn ctrl_c_and_ctrl_insert_copy() {
        assert!(is_copy(KeyCode::KeyC, Some("\u{3}"), true));
        assert!(is_copy(KeyCode::KeyC, Some("c"), true));
        assert!(is_copy(KeyCode::KeyC, None, true));
        assert!(is_copy(KeyCode::Insert, None, true));
        assert!(!is_copy(KeyCode::KeyC, Some("c"), false));
        assert!(!is_copy(KeyCode::KeyC, Some("k"), true));
    }

    #[test]
    fn ctrl_a_selects_all_and_the_next_key_replaces_or_clears_it() {
        assert!(is_select_all(KeyCode::KeyA, Some("\u{1}"), true));
        assert!(is_select_all(KeyCode::KeyA, Some("a"), true));
        assert!(is_select_all(KeyCode::KeyA, None, true));
        assert!(!is_select_all(KeyCode::KeyA, Some("a"), false));
        let mut field = "192.0.2.1:29070".to_owned();
        assert!(before_edit(
            &mut field,
            KeyCode::KeyA,
            Some("a"),
            false,
            true
        ));
        assert!(is_selected(&field));
        // Typing replaces it.
        assert!(!before_edit(
            &mut field,
            KeyCode::KeyX,
            Some("x"),
            false,
            false
        ));
        assert_eq!(field, "");
        // Backspace clears it and is spent.
        let mut field = "abc".to_owned();
        select_all(&field);
        assert!(before_edit(
            &mut field,
            KeyCode::Backspace,
            None,
            false,
            false
        ));
        assert_eq!(field, "");
        // Another key drops the selection and leaves the text.
        let mut field = "abc".to_owned();
        select_all(&field);
        assert!(!before_edit(
            &mut field,
            KeyCode::ArrowLeft,
            None,
            false,
            false
        ));
        assert_eq!(field, "abc");
        assert!(!is_selected(&field));
        // The selection belongs to its text: a change of text drops it, and copying keeps it.
        select_all(&field);
        assert!(!before_edit(
            &mut field,
            KeyCode::KeyC,
            Some("c"),
            false,
            true
        ));
        assert!(is_selected(&field));
        field.push('d');
        assert!(!is_selected(&field));
        // An empty field selects nothing.
        let mut empty = String::new();
        before_edit(&mut empty, KeyCode::KeyA, Some("a"), false, true);
        assert!(!selection_shown());
    }

    #[test]
    fn typing_stops_at_the_limit_and_drops_control_characters() {
        assert_eq!(typed("ab", "c\td", 4), "abcd");
        assert_eq!(typed("abcd", "e", 4), "abcd");
        assert_eq!(typed("", "\u{16}", 4), "");
    }

    #[test]
    fn a_paste_is_its_first_line_trimmed() {
        let mut field = "192.0.2.1".to_owned();
        assert!(paste_into(&mut field, "  :29070 \r\nsecond line", 255));
        assert_eq!(field, "192.0.2.1:29070");
        assert!(!paste_into(&mut field, "\n", 255));
    }

    #[test]
    fn backspace_takes_the_last_character_back() {
        let mut field = "ab".to_owned();
        assert!(edit(
            &mut field,
            KeyCode::Backspace,
            None,
            false,
            false,
            9,
            true
        ));
        assert_eq!(field, "a");
        assert!(edit(
            &mut field,
            KeyCode::Backspace,
            None,
            false,
            false,
            9,
            true
        ));
        assert!(!edit(
            &mut field,
            KeyCode::Backspace,
            None,
            false,
            false,
            9,
            true
        ));
    }
}
