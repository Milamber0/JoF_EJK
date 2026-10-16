//! Editing keys of the console input line and its clipboard shortcuts.
//!
//! Arrows, Home and End move the caret (Ctrl+arrows by word, Shift selects);
//! Backspace and Delete remove a character, or a word with Ctrl. Ctrl+A selects
//! the line, Ctrl+X cuts the selection, Ctrl+V and Shift+Insert paste. Ctrl+C (or
//! Ctrl+Insert) copies selected scrollback text, else the selected input, else, as
//! before, the whole line or the last `viewpos`/`mark` answer. Up and Down stay
//! history, and Enter still submits the whole line wherever the caret is.

use super::line_edit::{Motion, token_at};
use super::selection::{PromptPointer, copy_range};
use super::*;
use winit::keyboard::Key;

/// A clipboard shortcut of the console.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Shortcut {
    SelectAll,
    Copy,
    Cut,
    Paste,
}

impl ViewerConsole {
    /// Track Ctrl for word motion and clipboard shortcuts.
    pub(crate) fn set_control(&mut self, held: bool) {
        self.control = held;
    }

    /// Handle an editing or clipboard key of the open console; `false` leaves the
    /// key to the caller (history, completion, typing).
    pub(super) fn edit_key(&mut self, event: &KeyEvent, key: KeyCode) -> bool {
        let (word, extend) = (self.control, self.shift);
        let motion = |plain, by_word| if word { by_word } else { plain };
        match key {
            KeyCode::ArrowLeft => self.move_caret(motion(Motion::Left, Motion::WordLeft), extend),
            KeyCode::ArrowRight => {
                self.move_caret(motion(Motion::Right, Motion::WordRight), extend);
            }
            KeyCode::Home => self.move_caret(Motion::Home, extend),
            KeyCode::End => self.move_caret(Motion::End, extend),
            KeyCode::Backspace => self.erase(motion(Motion::Left, Motion::WordLeft)),
            KeyCode::Delete => self.erase(motion(Motion::Right, Motion::WordRight)),
            KeyCode::Insert if extend => self.shortcut(Shortcut::Paste),
            KeyCode::Insert if word => self.shortcut(Shortcut::Copy),
            _ => match self.shortcut_of(event) {
                Some(shortcut) => self.shortcut(shortcut),
                None => return false,
            },
        }
        true
    }

    /// Ctrl+letter arrives as its control character where the platform reports one,
    /// which follows the keyboard layout; otherwise the held Ctrl and the layout's
    /// letter are used.
    fn shortcut_of(&self, event: &KeyEvent) -> Option<Shortcut> {
        let letter = match event.text.as_deref() {
            Some("\u{1}") => 'a',
            Some("\u{3}") => 'c',
            Some("\u{16}") => 'v',
            Some("\u{18}") => 'x',
            _ if self.control => match &event.logical_key {
                Key::Character(text) => text.chars().next()?.to_ascii_lowercase(),
                _ => return None,
            },
            _ => return None,
        };
        Some(match letter {
            'a' => Shortcut::SelectAll,
            'c' => Shortcut::Copy,
            'v' => Shortcut::Paste,
            'x' => Shortcut::Cut,
            _ => return None,
        })
    }

    fn shortcut(&mut self, shortcut: Shortcut) {
        match shortcut {
            Shortcut::SelectAll => {
                self.selection.clear();
                self.edit.select_all(&self.input);
            }
            Shortcut::Copy => {
                let characters = match self.copy_output() {
                    Some(characters) => characters,
                    None => {
                        let text = match self.edit.selection(&self.input) {
                            Some(range) => &self.input[range],
                            None if self.input.is_empty() => &self.copied,
                            None => &self.input,
                        };
                        super::clipboard::copy(text);
                        text.chars().count()
                    }
                };
                if characters != 0 {
                    self.classic.sjk.note_copy(characters);
                }
            }
            Shortcut::Cut => {
                if let Some(range) = self.edit.selection(&self.input) {
                    super::clipboard::copy(&self.input[range]);
                    self.erase(Motion::Left);
                }
            }
            Shortcut::Paste => {
                if let Some(text) = super::clipboard::paste() {
                    self.type_text(&text);
                }
            }
        }
    }

    fn move_caret(&mut self, motion: Motion, extend: bool) {
        if extend {
            self.selection.clear();
        }
        self.edit.motion(&self.input, motion, extend);
    }

    /// Delete the input selection, or up to `motion` from the caret.
    fn erase(&mut self, motion: Motion) {
        self.edit.delete(&mut self.input, motion);
    }

    /// Insert typed or pasted text at the caret, replacing the input selection:
    /// no control characters, up to the input limit.
    pub(super) fn type_text(&mut self, text: &str) {
        self.edit.insert(&mut self.input, text, INPUT_LIMIT);
    }

    /// Copy selected scrollback text, as the console shows it but without colour
    /// codes; how many characters, or `None` if none is selected.
    fn copy_output(&mut self) -> Option<usize> {
        let (start, end) = self.selection.range()?;
        // The console draws stamps beside the text, so its marks count bytes
        // of the text alone.
        let lines = self.shell.lines().map(|line| line.text.as_str());
        let first = self.shell.lines_written() - self.shell.lines().count() as u64;
        let mut text = String::new();
        copy_range(lines, first, start, end, &mut text);
        super::clipboard::copy(&text);
        Some(text.chars().count())
    }

    /// Apply what the last frame resolved of a pointer gesture on the input line.
    pub(super) fn apply_prompt_pointer(&mut self) {
        match self.selection.take_prompt() {
            Some(PromptPointer::Place { byte, extend }) => {
                self.edit.place(&self.input, byte, extend);
            }
            Some(PromptPointer::Token(byte)) => {
                let token = token_at(&self.input, byte);
                self.edit.select(&self.input, token);
            }
            Some(PromptPointer::Deselect) => self.edit.deselect(),
            None => {}
        }
    }
}
