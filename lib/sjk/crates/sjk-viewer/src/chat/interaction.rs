//! Keyboard and pointer actions for the floating conversation layer.

use super::player_actions::{ACTIONS, MENU_BACK};
use super::*;
use sjk_client::{ChatDestination, chat_command};
use sjk_ui::{InputEvent, PointerButton, UiEventKind};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, KeyCode, PhysicalKey};

pub(super) const GLOBAL: u16 = 100;
pub(super) const TEAM: u16 = 101;
pub(super) const LATEST: u16 = 102;
pub(super) const SJK: u16 = 103;

/// Keys the composer acts on itself rather than typing their text.
fn handled_key(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::Escape
            | KeyCode::Enter
            | KeyCode::NumpadEnter
            | KeyCode::Tab
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::ArrowUp
            | KeyCode::ArrowDown
            | KeyCode::ArrowLeft
            | KeyCode::ArrowRight
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::Backspace
            | KeyCode::Delete
    )
}

impl ChatOverlay {
    pub(crate) fn handle_key(&mut self, event: &KeyEvent) -> ChatInputResult {
        if event.state != ElementState::Pressed {
            return ChatInputResult::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return ChatInputResult::None;
        };
        if self.edit_shortcut(event) {
            if let Some(input) = &mut self.input {
                input.dead.other_key(event.text.as_deref());
            }
            return ChatInputResult::None;
        }
        self.edit_key(key, &event.logical_key, event.text.as_deref())
    }

    pub(super) fn edit_key(
        &mut self,
        key: KeyCode,
        logical: &Key,
        text: Option<&str>,
    ) -> ChatInputResult {
        let Some(input) = &mut self.input else {
            return ChatInputResult::None;
        };
        // Keys the composer acts on itself end a pending composition when the
        // platform reported text for them (see `input::dead_key`).
        if handled_key(key) {
            input.dead.other_key(text);
        }
        if let Some(menu) = &mut self.player_menu {
            match key {
                KeyCode::Escape => {
                    self.player_menu = None;
                    return ChatInputResult::None;
                }
                KeyCode::ArrowUp | KeyCode::ArrowDown | KeyCode::Tab => {
                    let previous = key == KeyCode::ArrowUp
                        || (key == KeyCode::Tab && self.modifiers.shift_key());
                    let index = menu
                        .selected
                        .and_then(|selected| ACTIONS.iter().position(|token| *token == selected));
                    let next = match index {
                        Some(index) if previous => (index + ACTIONS.len() - 1) % ACTIONS.len(),
                        Some(index) => (index + 1) % ACTIONS.len(),
                        None if previous => ACTIONS.len() - 1,
                        None => 0,
                    };
                    menu.selected = Some(ACTIONS[next]);
                    return ChatInputResult::None;
                }
                KeyCode::Enter | KeyCode::NumpadEnter => {
                    if let Some(token) = menu.selected {
                        self.activate(token);
                    }
                    return ChatInputResult::None;
                }
                _ => self.player_menu = None,
            }
        }
        match key {
            KeyCode::Escape => {
                self.input = None;
                self.scroll = 0;
                self.unread = 0;
            }
            KeyCode::Enter | KeyCode::NumpadEnter => return self.submit(),
            KeyCode::PageUp | KeyCode::ArrowUp => self.scroll_by(3),
            KeyCode::PageDown | KeyCode::ArrowDown => self.scroll_by(-3),
            KeyCode::Tab => {
                let channel = self.input.as_ref().expect("active input").channel;
                match channel {
                    Channel::Global => self.activate(TEAM),
                    Channel::Team => self.activate(SJK),
                    // The last whisperer comes after SJK (`chat/reply.rs`).
                    Channel::Sjk if self.reply_target().is_ok() => {
                        let target = self.reply_target().ok();
                        let input = self.input.as_mut().expect("active input");
                        input.channel = Channel::Whisper;
                        input.recipient = target;
                        self.notice = "";
                    }
                    Channel::Sjk | Channel::Whisper => self.activate(GLOBAL),
                }
            }
            _ => {
                let input = self.input.as_mut().expect("active input");
                if !input.key(
                    key,
                    self.modifiers.control_key(),
                    self.modifiers.shift_key(),
                ) && (!self.modifiers.control_key() || self.modifiers.alt_key())
                {
                    input.type_key(logical, text);
                }
            }
        }
        ChatInputResult::None
    }

    fn submit(&mut self) -> ChatInputResult {
        let input = self.input.as_ref().expect("active input");
        if input.channel == Channel::Sjk {
            let text = input.text.trim().to_owned();
            self.input = None;
            self.scroll = 0;
            self.unread = 0;
            return if text.is_empty() {
                ChatInputResult::None
            } else {
                self.sent_on(Channel::Sjk, None);
                ChatInputResult::Sjk(text)
            };
        }
        let destination = match input.channel {
            Channel::Sjk => unreachable!("sent to the hub above"),
            Channel::Global => ChatDestination::Global,
            Channel::Team => ChatDestination::Team,
            Channel::Whisper => {
                let Some(target) = input
                    .recipient
                    .filter(|target| self.roster.name(*target).is_some())
                else {
                    self.notice = "Player left or changed identity. Choose a name again.";
                    return ChatInputResult::None;
                };
                ChatDestination::Player(target.slot())
            }
        };
        let result = chat_command(destination, &input.text);
        if result.is_some() {
            let (channel, recipient) = (input.channel, input.recipient);
            self.sent_on(channel, recipient);
        }
        self.input = None;
        self.scroll = 0;
        self.unread = 0;
        result.map_or(ChatInputResult::None, ChatInputResult::Submit)
    }

    pub(crate) fn pointer(&mut self, event: InputEvent) {
        if !self.is_typing() {
            return;
        }
        if matches!(
            event,
            InputEvent::PointerMove(_) | InputEvent::PointerLeave | InputEvent::PointerPress { .. }
        ) && let Some(menu) = &mut self.player_menu
        {
            menu.selected = None;
        }
        // The profile card lies over the feed and may reach the composer: a press on
        // it is the card's.
        let on_card = matches!(event, InputEvent::PointerPress { position, .. }
            if self.card.is_some()
                && self.ui.rect_for(card::CARD).is_some_and(|rect| rect.contains(position)));
        if !on_card && self.edit_pointer(event) {
            return;
        }
        if let InputEvent::PointerPress {
            position,
            button: PointerButton::Primary,
        } = event
        {
            // Match the canvas's reverse paint order: popovers cover the
            // composer, which covers the feed. Otherwise release activates a
            // different widget than the one recorded here and is discarded.
            self.pressed_action = [card::CARD_MUTE, card::CARD]
                .into_iter()
                .chain(ACTIONS.into_iter().rev())
                .chain([MENU_BACK, LATEST, SJK, TEAM, GLOBAL])
                .chain(0..MAX_VISIBLE as u16)
                .find(|token| {
                    self.ui
                        .rect_for(*token)
                        .is_some_and(|rect| rect.contains(position))
                })
                .map(|token| (token, self.action_target(token)));
        }
        if let InputEvent::PointerWheel { delta, .. } = event {
            self.player_menu = None;
            if delta.y != 0.0 {
                self.scroll_by(if delta.y > 0.0 { 1 } else { -1 });
            }
            return;
        }
        let result = self.ui.pointer(event);
        if let Some(result) = result
            && result.kind == UiEventKind::Activate
            && let Some(token) = result.token
        {
            let target = self.action_target(token);
            // A new message can rebuild the name widgets between press and
            // release. Never reinterpret the old press as a different sender.
            if self.pressed_action.take() != Some((token, target)) {
                return;
            }
            if usize::from(token) < self.visible_targets.len() {
                if let Some(target) = self.visible_targets[usize::from(token)]
                    .filter(|target| self.roster.name(*target).is_some())
                {
                    self.player_menu = Some(PlayerMenu {
                        target,
                        anchor_y: self.ui.rect_for(token).map_or(0.0, |rect| rect.y),
                        selected: None,
                    });
                }
            } else {
                self.activate(token);
            }
        }
        if let InputEvent::PointerPress {
            position,
            button: PointerButton::Primary,
        } = event
            && self.player_menu.is_some()
            && !self
                .ui
                .rect_for(MENU_BACK)
                .is_some_and(|r| r.contains(position))
        {
            self.player_menu = None;
        }
    }

    fn action_target(&self, token: u16) -> Option<ChatTarget> {
        if ACTIONS.contains(&token) || token == MENU_BACK {
            self.player_menu.map(|menu| menu.target)
        } else {
            self.visible_targets
                .get(usize::from(token))
                .copied()
                .flatten()
        }
    }

    fn scroll_by(&mut self, delta: isize) {
        self.scroll = self
            .scroll
            .saturating_add_signed(delta)
            .min(self.lines.len().saturating_sub(1));
        if self.scroll == 0 {
            self.unread = 0;
        }
        for line in &mut self.lines {
            line.y = None;
        }
    }

    pub(super) fn activate(&mut self, token: u16) {
        match token {
            GLOBAL | TEAM | SJK => {
                if let Some(input) = &mut self.input {
                    input.channel = match token {
                        GLOBAL => Channel::Global,
                        TEAM => Channel::Team,
                        _ => Channel::Sjk,
                    };
                    input.recipient = None;
                    self.notice = "";
                }
                self.player_menu = None;
            }
            LATEST => {
                self.scroll = 0;
                self.unread = 0;
                self.player_menu = None;
            }
            token if ACTIONS.contains(&token) => self.player_action(token),
            card::CARD_MUTE => self.toggle_card_mute(),
            _ => {}
        }
    }
}

impl crate::GpuState {
    /// Refresh recipient identity at submission time and use the existing reliable
    /// channel. No pointer action sends traffic or enters the gameplay bind path.
    pub(crate) fn chat_key(&mut self, event: &KeyEvent) {
        if let Some(session) = self
            .resident
            .session
            .as_ref()
            .or(self.live_session.as_ref())
        {
            self.chat.update_roster(session.game_state());
        }
        match self.chat.handle_key(event) {
            ChatInputResult::Submit(command) => {
                let command = self.console.as_ref().map_or_else(
                    || command.clone(),
                    |console| console.color_chat_command(&command),
                );
                self.send_chat_command(&command);
            }
            ChatInputResult::Sjk(text) => self.send_sjk_chat(text),
            ChatInputResult::None => {}
        }
        self.sync_cursor_policy();
    }

    /// Hand a message to the SJK chat, never to the game server; say why when it
    /// cannot go.
    pub(crate) fn send_sjk_chat(&mut self, text: String) {
        let off = self
            .console
            .as_ref()
            .is_some_and(|console| console.bool_cvar("cl_sjkChat") == Some(false));
        let why = if off {
            Some("^3SJK chat is off (cl_sjkChat 1 turns it on)")
        } else if !crate::player_identity::chat(text) {
            Some("^3SJK chat needs the SJK identity (cl_identity 1)")
        } else {
            None
        };
        if let Some(why) = why {
            self.chat.receive(
                sjk_client::ServerEventKind::Chat,
                why.to_owned(),
                None,
                Instant::now(),
            );
        }
    }

    /// Composer and console messages share the real server's reliable channel.
    pub(crate) fn send_chat_command(&mut self, command: &str) {
        if let Some(session) = self.communication_session_mut()
            && let Err(error) = session.send_reliable_command(command.as_bytes())
        {
            eprintln!("failed to send chat: {error}");
        }
    }
}

#[cfg(test)]
mod dead_key_tests {
    use super::*;
    use winit::keyboard::{NamedKey, SmolStr};

    fn press(chat: &mut ChatOverlay, key: KeyCode, logical: Key, text: Option<&str>) {
        chat.edit_key(key, &logical, text);
    }

    fn typed(chat: &mut ChatOverlay, key: KeyCode, text: &str) {
        press(chat, key, Key::Character(SmolStr::new(text)), Some(text));
    }

    fn dead_caret(chat: &mut ChatOverlay) {
        press(chat, KeyCode::BracketLeft, Key::Dead(Some('^')), None);
    }

    fn draft(chat: &ChatOverlay) -> &str {
        &chat.input.as_ref().expect("composer open").text
    }

    fn composer() -> ChatOverlay {
        let mut chat = ChatOverlay::new();
        chat.open(false);
        chat
    }

    #[test]
    fn azerty_dead_caret_and_digit_type_a_colour_code() {
        let mut chat = composer();
        dead_caret(&mut chat);
        assert_eq!(draft(&chat), "^");
        press(
            &mut chat,
            KeyCode::ShiftLeft,
            Key::Named(NamedKey::Shift),
            None,
        );
        typed(&mut chat, KeyCode::Digit1, "^1");
        typed(&mut chat, KeyCode::KeyH, "h");
        assert_eq!(draft(&chat), "^1h");
    }

    #[test]
    fn dead_caret_and_a_vowel_type_the_circumflex_letter() {
        let mut chat = composer();
        typed(&mut chat, KeyCode::KeyF, "f");
        dead_caret(&mut chat);
        typed(&mut chat, KeyCode::KeyE, "ê");
        typed(&mut chat, KeyCode::KeyT, "t");
        typed(&mut chat, KeyCode::KeyE, "e");
        assert_eq!(draft(&chat), "fête");
    }

    #[test]
    fn dead_caret_then_space_types_the_caret_alone() {
        let mut chat = composer();
        dead_caret(&mut chat);
        typed(&mut chat, KeyCode::Space, "^");
        assert_eq!(draft(&chat), "^");
    }

    #[test]
    fn dead_caret_is_replaced_at_the_caret_inside_the_draft() {
        let mut chat = composer();
        typed(&mut chat, KeyCode::KeyA, "a");
        typed(&mut chat, KeyCode::KeyB, "b");
        press(
            &mut chat,
            KeyCode::ArrowLeft,
            Key::Named(NamedKey::ArrowLeft),
            None,
        );
        dead_caret(&mut chat);
        assert_eq!(draft(&chat), "a^b");
        typed(&mut chat, KeyCode::Digit2, "^2");
        assert_eq!(draft(&chat), "a^2b");
        let input = chat.input.as_ref().unwrap();
        assert_eq!(input.edit.cursor(&input.text), 3);
    }

    #[test]
    fn backspace_after_a_dead_caret_erases_only_the_caret() {
        let mut chat = composer();
        typed(&mut chat, KeyCode::KeyA, "a");
        dead_caret(&mut chat);
        press(
            &mut chat,
            KeyCode::Backspace,
            Key::Named(NamedKey::Backspace),
            Some("^\u{8}"),
        );
        assert_eq!(draft(&chat), "a");
        typed(&mut chat, KeyCode::Digit1, "1");
        assert_eq!(draft(&chat), "a1");
    }

    #[test]
    fn enter_after_a_dead_caret_sends_it() {
        let mut chat = composer();
        typed(&mut chat, KeyCode::KeyA, "a");
        dead_caret(&mut chat);
        let result = chat.edit_key(KeyCode::Enter, &Key::Named(NamedKey::Enter), Some("^\r"));
        assert!(matches!(result, ChatInputResult::Submit(command) if command == "say \"a^\""));
    }
}
