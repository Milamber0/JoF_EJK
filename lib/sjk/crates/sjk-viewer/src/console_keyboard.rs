//! Console keyboard capture and editing.
use super::*;
use crate::input::dead_key::{TypingField, keep_caret};
use std::ops::Range;
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

/// Does `text` (the characters this key press produced) appear in a
/// `cl_consoleKeys` list? Entries are literal characters or `0x` hex
/// codepoints (`cl_main.cpp:2826`).
pub(crate) fn is_console_key(list: &str, text: &str) -> bool {
    let mut characters = text.chars();
    let (Some(typed), None) = (characters.next(), characters.next()) else {
        return false;
    };
    list.split_whitespace().any(|entry| {
        match entry
            .strip_prefix("0x")
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            .and_then(char::from_u32)
        {
            Some(code) => code == typed,
            None => {
                let mut entry = entry.chars();
                (entry.next(), entry.next()) == (Some(typed), None)
            }
        }
    })
}

/// Does the layout put `^` on this key without modifiers (German QWERTZ and
/// others, as a dead key or not)? `SDL_GetKeyFromScancode(...) == SDLK_CARET`.
pub(crate) fn types_caret(unshifted: &winit::keyboard::Key) -> bool {
    match unshifted {
        winit::keyboard::Key::Character(text) => text.as_str() == "^",
        winit::keyboard::Key::Dead(character) => *character == Some('^'),
        _ => false,
    }
}

/// What a pressed key does to the open console before any editing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OpenConsoleKey {
    /// Close the console.
    Close,
    /// Neither close it nor type: a held console key.
    Swallow,
    /// Leave the key to the console's own handling.
    Edit,
}

/// EternalJK turns a console key into `A_CONSOLE` before any text is made
/// (`IN_TranslateSDLToJKKey` and `IN_IsConsoleKey`, `shared/sdl/sdl_input.cpp`), and
/// `CL_KeyDownEvent` toggles the console on it, so a console key closes the open
/// console and never types its character; its repeats do nothing. A key bound to
/// `toggleconsole` closes it only when it prints nothing: a printable one types.
pub(crate) fn open_console_key(
    console_key: bool,
    non_printing_toggle: bool,
    repeat: bool,
) -> OpenConsoleKey {
    match (console_key || non_printing_toggle, repeat) {
        (true, false) => OpenConsoleKey::Close,
        (true, true) => OpenConsoleKey::Swallow,
        (false, _) => OpenConsoleKey::Edit,
    }
}

impl ViewerConsole {
    /// Stock console keys: Shift+Escape (`cl_keys.cpp:1318`), the physical key
    /// under Escape with `cl_consoleUseScanCode` (EternalJK's default, so layouts
    /// where it types `0`, as Hungarian, open the console too), or else a
    /// `cl_consoleKeys` character. Layouts where the key types `^` keep that
    /// character for colour codes and open the console with Shift.
    pub(super) fn configured_console_key(&self, event: &KeyEvent, key: KeyCode) -> bool {
        if key == KeyCode::Escape {
            return self.shift;
        }
        let native = self.integer_cvar("cl_consoleusescancode").unwrap_or(1) != 0;
        if native
            && key == KeyCode::Backquote
            && super::client_options::native_console(
                self.integer_cvar("cl_consoleshiftrequirement").unwrap_or(0),
                types_caret(&event.key_without_modifiers()),
                self.shift,
                self.open,
            )
        {
            return true;
        }
        let text = event.text.as_deref();
        let list = self
            .shell
            .cvars
            .get("cl_consoleKeys")
            .and_then(|cvar| match &cvar.value {
                CvarValue::Text(value) => Some(value.as_str()),
                _ => None,
            })
            .unwrap_or("");
        !native && text.is_some_and(|text| is_console_key(list, text))
    }

    fn toggles_console(&self, event: &KeyEvent, key: KeyCode) -> bool {
        self.configured_console_key(event, key) || self.bound_to_toggle(event, key)
    }

    /// A key bound to `toggleconsole` (Escape never counts: it has its own meaning).
    fn bound_to_toggle(&self, event: &KeyEvent, key: KeyCode) -> bool {
        if key == KeyCode::Escape {
            return false;
        }
        let Some(key_name) = crate::input::keys::key_name(event) else {
            return false;
        };
        self.shell
            .binds
            .commands_for_event(key_name.as_str(), true)
            .is_ok_and(|commands| {
                commands
                    .iter()
                    .any(|command| command.eq_ignore_ascii_case("toggleconsole"))
            })
    }
    /// Consume console input before gameplay input gets a chance to observe it.
    pub(crate) fn handle_key(
        &mut self,
        event: &KeyEvent,
        session: Option<&mut ClientSession>,
    ) -> bool {
        let PhysicalKey::Code(key) = event.physical_key else {
            return self.open;
        };
        if !self.open {
            // A held console key does not reopen the console it just closed.
            if event.state == ElementState::Pressed
                && !event.repeat
                && self.toggles_console(event, key)
            {
                // `CL_KeyDownEvent`: Ctrl opens the console full screen, Shift a
                // quarter of it (classic console).
                self.open_height =
                    super::classic::open_height(key == KeyCode::Escape, self.control, self.shift);
                self.set_open(true);
                return true;
            }
            let Some(key_name) = crate::input::keys::key_name(event) else {
                return false;
            };
            let commands = match self
                .shell
                .binds
                .commands_for_event(key_name.as_str(), event.state == ElementState::Pressed)
            {
                Ok(commands) => commands,
                Err(error) => {
                    self.shell.push_log(format!("^1Bind error: {error}"));
                    return true;
                }
            };
            let mut consumed = false;
            for command in commands {
                if command.eq_ignore_ascii_case("toggleconsole") {
                    self.execute_bound_command(&command);
                    consumed = true;
                } else if command.eq_ignore_ascii_case(super::debug_panel::COMMAND) {
                    self.toggle_debug_panel();
                    consumed = true;
                }
            }
            return consumed;
        }

        if event.state != ElementState::Pressed {
            return true;
        }
        let printable = matches!(
            event.logical_key,
            winit::keyboard::Key::Character(_) | winit::keyboard::Key::Dead(_)
        );
        match open_console_key(
            self.configured_console_key(event, key),
            !printable && self.bound_to_toggle(event, key),
            event.repeat,
        ) {
            OpenConsoleKey::Close => {
                self.set_open(false);
                return true;
            }
            OpenConsoleKey::Swallow => return true,
            OpenConsoleKey::Edit => {}
        }
        if self.asset_browser_key(event)
            || self.model_viewer_key(event)
            || self.config_import_key(event)
            || self.credits_key(event)
            || self.changelog_key(event)
            || self.update_panel_key(event)
            || self.identity_panel_key(event)
            || self.profile_panel_key(event)
            || self.staff_panel_key(event)
            || self.collection_panel_key(event)
            || self.holocrons_panel_key(event)
            || self.sjk_chat_panel_key(event)
            || self.debug_panel_key(event)
        {
            return true;
        }
        if self.browser.is_open() {
            let action = self.browser.handle_key(event, self.shift, self.control);
            self.browser_action(action);
            return true;
        }
        if self.classic_key(event, key) {
            return true;
        }
        // Keys the console acts on itself end a pending composition when the platform
        // reported text for them (see `input::dead_key`); caret keys leave it pending.
        if matches!(
            key,
            KeyCode::F3
                | KeyCode::Escape
                | KeyCode::Enter
                | KeyCode::NumpadEnter
                | KeyCode::Tab
                | KeyCode::ArrowUp
                | KeyCode::ArrowDown
        ) {
            self.dead_key.other_key(event.text.as_deref());
        }
        match key {
            KeyCode::F3 if !event.repeat => self.browser.open(&self.shell),
            KeyCode::Escape => self.set_open(false),
            KeyCode::Enter | KeyCode::NumpadEnter => self.submit(session),
            KeyCode::Tab => self.complete_command(CompletionKey::Tab),
            KeyCode::ArrowUp if !event.repeat => self.navigate_history(-1),
            KeyCode::ArrowDown if !event.repeat => self.navigate_history(1),
            // Caret, deletion and clipboard keys: see `console_editing.rs`.
            _ if self.edit_key(event, key) => self.dead_key.other_key(event.text.as_deref()),
            _ if !event.repeat => {
                let mut dead = self.dead_key;
                dead.type_key(
                    &mut PromptLine {
                        text: &mut self.input,
                        edit: &mut self.edit,
                        overstrike: self.overstrike,
                    },
                    &event.logical_key,
                    event.text.as_deref(),
                );
                self.dead_key = dead;
            }
            _ => {}
        }
        true
    }
}

impl ViewerConsole {
    /// EternalJK's console keys (`Console_Key`): Page Up/Down scroll two
    /// rows (ten with Ctrl), Ctrl+Home/End jump to the top or bottom, keypad 8/2
    /// (without Num Lock) and Ctrl+P/N walk the history, Ctrl+L clears the
    /// scrollback and Insert toggles overstrike. `false` leaves the key to the
    /// shared handling.
    fn classic_key(&mut self, event: &KeyEvent, key: KeyCode) -> bool {
        let control = self.control;
        let control_letter = |letter: &str, code: &str| {
            event.text.as_deref() == Some(code)
                || (control
                    && matches!(&event.logical_key, winit::keyboard::Key::Character(text)
                        if text.eq_ignore_ascii_case(letter)))
        };
        match key {
            KeyCode::PageUp => self.scroll_rows(super::classic::page_rows(control) as isize),
            KeyCode::PageDown => {
                self.scroll_rows(-(super::classic::page_rows(control) as isize));
            }
            KeyCode::Home if control => self.scroll_offset = usize::MAX,
            KeyCode::End if control => self.scroll_offset = 0,
            KeyCode::Numpad8 if event.text.is_none() && !event.repeat => {
                self.navigate_history(-1);
            }
            KeyCode::Numpad2 if event.text.is_none() && !event.repeat => {
                self.navigate_history(1);
            }
            KeyCode::Insert if !control && !self.shift => self.overstrike = !self.overstrike,
            _ if control_letter("p", "\u{10}") => self.navigate_history(-1),
            _ if control_letter("n", "\u{e}") => self.navigate_history(1),
            _ if control_letter("l", "\u{c}") => {
                // `Console_Key`: Ctrl+L runs `clear`.
                self.shell.clear_lines();
                self.scroll_offset = 0;
                self.selection.clear();
            }
            _ => return false,
        }
        true
    }

    /// Scroll the classic scrollback back by `rows` (forward when negative); the
    /// view clamps it to the rows there are.
    pub(super) fn scroll_rows(&mut self, rows: isize) {
        self.scroll_offset = self.scroll_offset.saturating_add_signed(rows);
    }
}

/// The console input line as a field dead-key composition types into.
struct PromptLine<'a> {
    text: &'a mut String,
    edit: &'a mut super::line_edit::LineEdit,
    /// Typing replaces the character after the caret (Insert, classic console).
    overstrike: bool,
}

impl TypingField for PromptLine<'_> {
    fn line(&self) -> &str {
        self.text
    }

    fn caret(&self) -> usize {
        self.edit.cursor(self.text)
    }

    fn insert(&mut self, text: &str) {
        if self.overstrike {
            self.edit.overwrite(self.text, text, INPUT_LIMIT);
        } else {
            self.edit.insert(self.text, text, INPUT_LIMIT);
        }
    }

    fn remove(&mut self, range: Range<usize>) {
        let caret = keep_caret(self.edit.cursor(self.text), &range);
        self.text.replace_range(range, "");
        self.edit.place(self.text, caret, false);
    }
}

#[cfg(test)]
mod dead_key_tests {
    use super::*;
    use crate::input::dead_key::DeadKey;
    use winit::keyboard::{Key, NamedKey, SmolStr};

    /// Type one press into `input` the way the console's typing path does.
    fn press(
        input: &mut String,
        edit: &mut super::super::line_edit::LineEdit,
        dead: &mut DeadKey,
        logical: Key,
        text: Option<&str>,
    ) {
        dead.type_key(
            &mut PromptLine {
                text: input,
                edit,
                overstrike: false,
            },
            &logical,
            text,
        );
    }

    fn character(text: &str) -> Key {
        Key::Character(SmolStr::new(text))
    }

    #[test]
    fn prompt_line_types_a_dead_key_colour_code_exactly() {
        let mut input = String::from("say ");
        let mut edit = super::super::line_edit::LineEdit::default();
        edit.to_end(&input);
        let mut dead = DeadKey::default();
        press(&mut input, &mut edit, &mut dead, Key::Dead(Some('^')), None);
        assert_eq!(input, "say ^");
        press(
            &mut input,
            &mut edit,
            &mut dead,
            Key::Named(NamedKey::Shift),
            None,
        );
        // Windows reports the uncombined pair as the digit key's text.
        press(&mut input, &mut edit, &mut dead, character("1"), Some("^1"));
        assert_eq!(input, "say ^1");
        assert_eq!(edit.cursor(&input), input.len());
    }

    #[test]
    fn prompt_line_composes_a_circumflex_letter() {
        let mut input = String::from("t");
        let mut edit = super::super::line_edit::LineEdit::default();
        edit.to_end(&input);
        let mut dead = DeadKey::default();
        press(&mut input, &mut edit, &mut dead, Key::Dead(Some('^')), None);
        assert_eq!(input, "t^");
        press(&mut input, &mut edit, &mut dead, character("ê"), Some("ê"));
        press(&mut input, &mut edit, &mut dead, character("t"), Some("t"));
        press(&mut input, &mut edit, &mut dead, character("e"), Some("e"));
        assert_eq!(input, "tête");
    }

    #[test]
    fn prompt_line_replaces_a_dead_key_shown_inside_the_line() {
        let mut input = String::from("ab");
        let mut edit = super::super::line_edit::LineEdit::default();
        edit.place(&input, 1, false);
        let mut dead = DeadKey::default();
        press(&mut input, &mut edit, &mut dead, Key::Dead(Some('^')), None);
        assert_eq!(input, "a^b");
        press(&mut input, &mut edit, &mut dead, character("2"), Some("^2"));
        assert_eq!((input.as_str(), edit.cursor(&input)), ("a^2b", 3));
    }

    #[test]
    fn prompt_line_keeps_its_limit_for_a_dead_key() {
        let mut input = "x".repeat(INPUT_LIMIT);
        let mut edit = super::super::line_edit::LineEdit::default();
        edit.to_end(&input);
        let mut dead = DeadKey::default();
        press(&mut input, &mut edit, &mut dead, Key::Dead(Some('^')), None);
        assert_eq!(input.len(), INPUT_LIMIT);
        assert_eq!(dead, DeadKey::default());
    }
}

#[cfg(test)]
mod open_console_tests {
    use super::{OpenConsoleKey, is_console_key, open_console_key};

    #[test]
    fn the_console_key_closes_the_open_console_without_typing() {
        // `~` or `²` in cl_consoleKeys: EternalJK's A_CONSOLE, never a character.
        assert_eq!(open_console_key(true, false, false), OpenConsoleKey::Close);
        assert_eq!(open_console_key(true, true, false), OpenConsoleKey::Close);
    }

    #[test]
    fn a_held_console_key_neither_closes_nor_types() {
        assert_eq!(open_console_key(true, false, true), OpenConsoleKey::Swallow);
    }

    #[test]
    fn a_non_printing_toggle_binding_closes_and_printable_text_types() {
        assert_eq!(open_console_key(false, true, false), OpenConsoleKey::Close);
        assert_eq!(open_console_key(false, false, false), OpenConsoleKey::Edit);
        assert_eq!(open_console_key(false, false, true), OpenConsoleKey::Edit);
    }

    #[test]
    fn console_key_list_entries_are_characters_or_hex_codepoints() {
        let list = "~ ` 0x7e 0xb2";
        for typed in ["~", "`", "²"] {
            assert!(is_console_key(list, typed), "{typed}");
        }
        assert!(!is_console_key(list, "^"));
        assert!(!is_console_key(list, "~~"));
    }

    #[test]
    fn the_key_under_escape_opens_the_console_unless_it_types_a_caret() {
        use super::super::client_options::native_console;
        use super::types_caret;
        use winit::keyboard::Key;
        // Hungarian `0`, US `` ` ``, French `²`, Nordic `§`: no Shift needed.
        for unshifted in ["0", "`", "²", "§"] {
            let caret = types_caret(&Key::Character(unshifted.into()));
            assert!(native_console(0, caret, false, false), "{unshifted}");
        }
        // German QWERTZ: `^` types, Shift+`^` opens.
        for unshifted in [Key::Character("^".into()), Key::Dead(Some('^'))] {
            assert!(types_caret(&unshifted));
            assert!(!native_console(0, true, false, false));
            assert!(!native_console(1, true, false, true));
            assert!(native_console(0, true, true, false));
        }
    }
}
