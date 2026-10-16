//! Clipboard and pointer editing. No action submits the draft or enters binds.

use super::*;
use crate::console::{
    clipboard,
    line_edit::{Motion, token_at},
};
use sjk_ui::{InputEvent, PointerButton, Rect};
use winit::{
    event::KeyEvent,
    keyboard::{Key, KeyCode, ModifiersState, PhysicalKey},
};

/// Cached glyph positions from the last drawn draft; bounded by the compose budget.
pub(super) struct DraftLayout {
    pub(super) rect: Rect,
    pub(super) stops: [(usize, f32); sjk_client::CHAT_INPUT_BYTES + 1],
    pub(super) len: usize,
    dragging: bool,
    last_click: Option<(Instant, usize)>,
}

impl Default for DraftLayout {
    fn default() -> Self {
        Self {
            rect: Rect::default(),
            stops: [(0, 0.0); sjk_client::CHAT_INPUT_BYTES + 1],
            len: 0,
            dragging: false,
            last_click: None,
        }
    }
}

impl DraftLayout {
    fn byte_at(&self, x: f32) -> usize {
        let stops = &self.stops[..self.len];
        for pair in stops.windows(2) {
            if x < (pair[0].1 + pair[1].1) * 0.5 {
                return pair[0].0;
            }
        }
        stops.last().map_or(0, |stop| stop.0)
    }
}

impl ChatOverlay {
    pub(crate) fn set_modifiers(&mut self, modifiers: ModifiersState) {
        self.modifiers = modifiers;
    }

    pub(super) fn edit_shortcut(&mut self, event: &KeyEvent) -> bool {
        let Some(input) = &mut self.input else {
            return false;
        };
        let control = self.modifiers.control_key() && !self.modifiers.alt_key();
        let letter = match event.text.as_deref() {
            Some("\u{1}") => Some('a'),
            Some("\u{3}") => Some('c'),
            Some("\u{16}") => Some('v'),
            Some("\u{18}") => Some('x'),
            _ if control => match &event.logical_key {
                Key::Character(text) => text.chars().next().map(|c| c.to_ascii_lowercase()),
                _ => None,
            },
            _ => None,
        };
        let letter = if event.physical_key == PhysicalKey::Code(KeyCode::Insert) {
            if self.modifiers.shift_key() {
                Some('v')
            } else if control {
                Some('c')
            } else {
                letter
            }
        } else {
            letter
        };
        match letter {
            Some('a') => input.edit.select_all(&input.text),
            Some('c') => {
                let text = input
                    .edit
                    .selection(&input.text)
                    .map_or(input.text.as_str(), |range| &input.text[range]);
                clipboard::copy(text);
            }
            Some('x') => {
                if let Some(range) = input.edit.selection(&input.text)
                    && clipboard::copy(&input.text[range])
                {
                    input.edit.delete(&mut input.text, Motion::Left);
                }
            }
            Some('v') => {
                if let Some(text) = clipboard::paste() {
                    input.insert(&text);
                }
            }
            _ => return false,
        }
        self.player_menu = None;
        true
    }

    pub(super) fn edit_pointer(&mut self, event: InputEvent) -> bool {
        let Some(input) = &mut self.input else {
            return false;
        };
        match event {
            InputEvent::PointerPress {
                position,
                button: PointerButton::Primary,
            } if input.layout.rect.contains(position) && input.layout.len > 0 => {
                let at = input.layout.byte_at(position.x);
                let now = Instant::now();
                let double = input.layout.last_click.is_some_and(|(last, byte)| {
                    byte == at && now.duration_since(last).as_millis() < 350
                });
                if double {
                    input.edit.select(&input.text, token_at(&input.text, at));
                } else {
                    input
                        .edit
                        .place(&input.text, at, self.modifiers.shift_key());
                }
                input.layout.last_click = (!double).then_some((now, at));
                input.layout.dragging = !double;
                self.player_menu = None;
                self.pressed_action = None;
                true
            }
            InputEvent::PointerMove(position) if input.layout.dragging => {
                input
                    .edit
                    .place(&input.text, input.layout.byte_at(position.x), true);
                true
            }
            InputEvent::PointerRelease {
                button: PointerButton::Primary,
                ..
            } if input.layout.dragging => {
                input.layout.dragging = false;
                true
            }
            InputEvent::PointerLeave => {
                input.layout.dragging = false;
                false
            }
            _ => false,
        }
    }
}
