//! Replying to the last whisper and opening the composer on the last channel used.
//!
//! `reply <message>` whispers to the last player who whispered to us, as JoF
//! EternalJK's `reply` does (`CG_Say_f`, `lastWhispererId` set in
//! `CG_ChatBox_AddString` from a line holding `]: ^6`); a bare `reply`, a key bind,
//! opens the composer on that whisper instead. Tab in the composer reaches the
//! same whisper after the SJK channel.
//!
//! With `cg_chatRememberChannel 1` (the default) the chat key (`messagemode`) opens
//! the composer on the channel of the last message sent from it: All, Team, SJK or
//! a whisper to a player still in the game. The team, crosshair and SJK keys keep
//! opening on their own channel.

use super::*;

/// `cg_chatRememberChannel`: the chat key opens on the last channel a message went to.
pub(crate) const REMEMBER_CVAR: &str = "cg_chatRememberChannel";

/// Why `reply` has nobody to whisper to.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum NoReply {
    /// Nobody whispered to us since SJK started.
    Nobody,
    /// The last whisperer left or renamed.
    Left,
}

impl NoReply {
    /// The line the console prints.
    pub(crate) fn line(&self) -> &'static str {
        match self {
            Self::Nobody => "^3reply: nobody has whispered to you yet",
            Self::Left => "^3reply: the player who whispered to you has left or renamed",
        }
    }
}

impl ChatOverlay {
    /// The player to whom `reply` whispers, while they are still in the game.
    pub(crate) fn reply_target(&self) -> Result<ChatTarget, NoReply> {
        let target = self.last_whisperer.ok_or(NoReply::Nobody)?;
        self.roster
            .name(target)
            .map(|_| target)
            .ok_or(NoReply::Left)
    }

    /// Who sent a line the server did not attribute when it is a whisper, `[name]: `
    /// with the name of a player in the game (colour codes aside), as JoF EternalJK reads
    /// it. Only `reply` uses it: the line itself stays unattributed and unclickable.
    pub(super) fn whisper_sender(&self, display: &str) -> Option<ChatTarget> {
        let rest = display.strip_prefix('[')?;
        let name = sjk_client::chat_name_key(&rest[..rest.find("]: ")?]);
        (0..32_u16)
            .filter_map(|slot| self.roster.target(Some(slot)))
            .find(|target| self.roster.name(*target) == Some(name.as_str()))
    }

    /// Note a received line's sender as the one to reply to when it is a whisper from
    /// someone else (the server echoes our own whispers back to us).
    pub(super) fn note_whisper(&mut self, private: bool, sender: Option<ChatTarget>) {
        if let Some(sender) =
            sender.filter(|sender| private && Some(sender.slot()) != self.own_slot)
        {
            self.last_whisperer = Some(sender);
        }
    }

    /// Open the composer on a whisper to the last whisperer.
    pub(crate) fn open_reply(&mut self) -> Result<(), NoReply> {
        let target = self.reply_target()?;
        self.open(false);
        let input = self.input.as_mut().expect("opened composer");
        input.channel = Channel::Whisper;
        input.recipient = Some(target);
        Ok(())
    }

    /// Open the composer for the chat key: on the last channel used while
    /// `cg_chatRememberChannel` is on, else on All.
    pub(crate) fn open_chat(&mut self) {
        let (channel, recipient) = self.last_channel;
        self.open(false);
        if !self.options.remember {
            return;
        }
        let recipient = recipient.filter(|target| self.roster.name(*target).is_some());
        let input = self.input.as_mut().expect("opened composer");
        match channel {
            Channel::Whisper if recipient.is_none() => {}
            channel => {
                input.channel = channel;
                input.recipient = recipient;
            }
        }
    }

    /// Remember the channel the composer just sent on.
    pub(super) fn sent_on(&mut self, channel: Channel, recipient: Option<ChatTarget>) {
        self.last_channel = (channel, recipient);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_protocol::GameState;
    use winit::keyboard::{Key, KeyCode, NamedKey};

    const CS_PLAYERS: usize = 1131;

    fn roster(own: i32, names: &[(usize, &str)]) -> GameState {
        let mut game = GameState::empty_local(own);
        for (slot, name) in names {
            game.replace_config_string(CS_PLAYERS + slot, format!("n\\{name}\\t\\0").into_bytes())
                .unwrap();
        }
        game
    }

    fn chat_with(game: &GameState) -> ChatOverlay {
        let mut chat = ChatOverlay::new();
        chat.update_roster(game);
        chat
    }

    fn whisper(chat: &mut ChatOverlay, from: u16, name: &str) {
        chat.receive(
            ServerEventKind::Chat,
            format!("[{name}^7]: ^6hello"),
            Some(from),
            Instant::now(),
        );
    }

    fn press(chat: &mut ChatOverlay, key: KeyCode) -> ChatInputResult {
        chat.edit_key(key, &Key::Named(NamedKey::Enter), None)
    }

    fn typed(chat: &mut ChatOverlay, text: &str) {
        for c in text.chars() {
            let text = c.to_string();
            chat.edit_key(
                KeyCode::KeyA,
                &Key::Character(text.as_str().into()),
                Some(&text),
            );
        }
    }

    #[test]
    fn reply_whispers_to_the_last_whisperer_but_never_to_ourselves() {
        let game = roster(0, &[(0, "Sol"), (3, "Bob"), (5, "Ann")]);
        let mut chat = chat_with(&game);
        assert_eq!(chat.reply_target(), Err(NoReply::Nobody));
        whisper(&mut chat, 3, "Bob");
        whisper(&mut chat, 0, "Sol");
        assert_eq!(chat.reply_target().map(ChatTarget::slot), Ok(3));
        whisper(&mut chat, 5, "Ann");
        assert_eq!(chat.reply_target().map(ChatTarget::slot), Ok(5));
        // Public chat does not change it.
        chat.receive(
            ServerEventKind::Chat,
            "Bob^7: ^2hi".to_owned(),
            Some(3),
            Instant::now(),
        );
        assert_eq!(chat.reply_target().map(ChatTarget::slot), Ok(5));
    }

    #[test]
    fn an_unattributed_whisper_is_read_from_its_name() {
        let game = roster(0, &[(0, "Sol"), (4, "^1Bob")]);
        let mut chat = chat_with(&game);
        chat.receive(
            ServerEventKind::Chat,
            "[^1Bob^7]: ^6psst".to_owned(),
            None,
            Instant::now(),
        );
        assert_eq!(chat.reply_target().map(ChatTarget::slot), Ok(4));
        assert!(
            chat.lines[0].sender.is_none(),
            "the line stays unattributed"
        );
        // Our own whisper echoed back is not one to reply to.
        chat.receive(
            ServerEventKind::Chat,
            "[Sol^7]: ^6ok".to_owned(),
            None,
            Instant::now(),
        );
        assert_eq!(chat.reply_target().map(ChatTarget::slot), Ok(4));
    }

    #[test]
    fn a_whisperer_who_left_cannot_be_replied_to() {
        let mut chat = chat_with(&roster(0, &[(0, "Sol"), (3, "Bob")]));
        whisper(&mut chat, 3, "Bob");
        chat.update_roster(&roster(0, &[(0, "Sol")]));
        assert_eq!(chat.reply_target(), Err(NoReply::Left));
        assert_eq!(chat.open_reply(), Err(NoReply::Left));
        assert!(!chat.is_typing());
    }

    #[test]
    fn the_reply_composer_sends_a_tell_to_the_whisperer() {
        let mut chat = chat_with(&roster(0, &[(0, "Sol"), (3, "Bob")]));
        whisper(&mut chat, 3, "Bob");
        chat.open_reply().unwrap();
        typed(&mut chat, "yes");
        let ChatInputResult::Submit(command) = press(&mut chat, KeyCode::Enter) else {
            panic!("a whisper goes to the game server");
        };
        assert!(command.starts_with("tell 3 "), "{command}");
    }

    #[test]
    fn tab_reaches_the_reply_after_the_sjk_channel() {
        let mut chat = chat_with(&roster(0, &[(0, "Sol"), (3, "Bob")]));
        chat.open(false);
        let channels = |chat: &mut ChatOverlay| {
            (0..4)
                .map(|_| {
                    press(chat, KeyCode::Tab);
                    chat.input.as_ref().unwrap().channel
                })
                .collect::<Vec<_>>()
        };
        // Nobody to reply to: All, Team, SJK in turn.
        assert_eq!(
            channels(&mut chat),
            [Channel::Team, Channel::Sjk, Channel::Global, Channel::Team]
        );
        whisper(&mut chat, 3, "Bob");
        assert_eq!(
            channels(&mut chat),
            [
                Channel::Sjk,
                Channel::Whisper,
                Channel::Global,
                Channel::Team
            ]
        );
    }

    #[test]
    fn the_chat_key_opens_on_the_last_channel_sent_on() {
        let mut chat = chat_with(&roster(0, &[(0, "Sol"), (3, "Bob")]));
        chat.options.remember = true;
        chat.open_sjk();
        typed(&mut chat, "hi");
        assert!(matches!(
            press(&mut chat, KeyCode::Enter),
            ChatInputResult::Sjk(_)
        ));
        chat.open_chat();
        assert_eq!(chat.input.as_ref().unwrap().channel, Channel::Sjk);
        // An empty draft sends nothing and changes nothing.
        press(&mut chat, KeyCode::Tab);
        press(&mut chat, KeyCode::Enter);
        chat.open_chat();
        assert_eq!(chat.input.as_ref().unwrap().channel, Channel::Sjk);
        press(&mut chat, KeyCode::Escape);

        // A whisper is remembered while its player stays.
        whisper(&mut chat, 3, "Bob");
        chat.open_reply().unwrap();
        typed(&mut chat, "ok");
        press(&mut chat, KeyCode::Enter);
        chat.open_chat();
        let input = chat.input.as_ref().unwrap();
        assert_eq!(input.channel, Channel::Whisper);
        assert_eq!(input.recipient.map(ChatTarget::slot), Some(3));
        press(&mut chat, KeyCode::Escape);
        chat.update_roster(&roster(0, &[(0, "Sol")]));
        chat.open_chat();
        assert_eq!(chat.input.as_ref().unwrap().channel, Channel::Global);
        press(&mut chat, KeyCode::Escape);

        // Off, the chat key always opens on All.
        chat.sent_on(Channel::Team, None);
        chat.options.remember = false;
        chat.open_chat();
        assert_eq!(chat.input.as_ref().unwrap().channel, Channel::Global);
    }
}
