//! Chat drafts use the console's UTF-8 caret, selection and word editing rules.

use super::Channel;
use crate::console::line_edit::{LineEdit, Motion};
use crate::input::dead_key::{DeadKey, TypingField, keep_caret};
use sjk_client::{CHAT_INPUT_BYTES, ChatTarget, chat_input_fits};
use std::ops::Range;
use winit::keyboard::{Key, KeyCode};

pub(super) struct Editor {
    pub(super) text: String,
    pub(super) edit: LineEdit,
    pub(super) channel: Channel,
    pub(super) recipient: Option<ChatTarget>,
    pub(super) layout: super::editing::DraftLayout,
    /// Dead key shown at the caret until its composition arrives.
    pub(super) dead: DeadKey,
}

impl Editor {
    pub(super) fn new(channel: Channel) -> Self {
        Self {
            text: String::with_capacity(CHAT_INPUT_BYTES),
            edit: LineEdit::default(),
            channel,
            recipient: None,
            layout: super::editing::DraftLayout::default(),
            dead: DeadKey::default(),
        }
    }

    /// Type a pressed character key, showing a dead key until it composes.
    pub(super) fn type_key(&mut self, logical: &Key, text: Option<&str>) {
        let mut dead = self.dead;
        dead.type_key(self, logical, text);
        self.dead = dead;
    }

    /// Insert typed or pasted text. The draft's length counts the bytes it is
    /// sent with, escapes included (`%` takes three bytes, `"` two), as EternalJK's
    /// chat field does, so a full field is never cut short when it is sent.
    pub(super) fn insert(&mut self, value: &str) {
        self.edit
            .insert_fitting(&mut self.text, value, chat_input_fits);
    }

    pub(super) fn key(&mut self, key: KeyCode, control: bool, shift: bool) -> bool {
        let left = if control {
            Motion::WordLeft
        } else {
            Motion::Left
        };
        let right = if control {
            Motion::WordRight
        } else {
            Motion::Right
        };
        match key {
            KeyCode::ArrowLeft => self.edit.motion(&self.text, left, shift),
            KeyCode::ArrowRight => self.edit.motion(&self.text, right, shift),
            KeyCode::Home => self.edit.motion(&self.text, Motion::Home, shift),
            KeyCode::End => self.edit.motion(&self.text, Motion::End, shift),
            KeyCode::Backspace => self.edit.delete(&mut self.text, left),
            KeyCode::Delete => self.edit.delete(&mut self.text, right),
            _ => return false,
        }
        true
    }
}

impl TypingField for Editor {
    fn line(&self) -> &str {
        &self.text
    }

    fn caret(&self) -> usize {
        self.edit.cursor(&self.text)
    }

    fn insert(&mut self, text: &str) {
        Editor::insert(self, text);
    }

    fn remove(&mut self, range: Range<usize>) {
        let caret = keep_caret(self.edit.cursor(&self.text), &range);
        self.text.replace_range(range, "");
        self.edit.place(&self.text, caret, false);
    }
}

#[cfg(test)]
mod length_tests {
    use super::*;

    #[test]
    fn the_field_stops_where_the_sent_message_would_be_cut() {
        let mut editor = Editor::new(Channel::Global);
        editor.insert(&"%".repeat(CHAT_INPUT_BYTES));
        assert_eq!(editor.text, "%".repeat(CHAT_INPUT_BYTES / 3));

        let mut editor = Editor::new(Channel::Global);
        editor.insert(&"a".repeat(CHAT_INPUT_BYTES - 2));
        editor.insert("%");
        assert_eq!(editor.text.len(), CHAT_INPUT_BYTES - 2);
        editor.insert("\"");
        assert!(editor.text.ends_with('"'));
        editor.insert("a");
        assert_eq!(editor.text.len(), CHAT_INPUT_BYTES - 1);

        let sent = sjk_client::chat_command(sjk_client::ChatDestination::Global, &editor.text)
            .expect("global chat");
        assert!(sent.ends_with("a''\""), "{sent}");
    }
}
