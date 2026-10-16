//! Local player actions. Only composing and submitting a whisper sends a command.

use super::*;

pub(super) const MENU_BACK: u16 = 109;
pub(super) const WHISPER: u16 = 110;
pub(super) const IGNORE: u16 = 111;
pub(super) const FRIEND: u16 = 112;
pub(super) const COPY_NAME: u16 = 113;
pub(super) const ACTIONS: [u16; 4] = [WHISPER, IGNORE, FRIEND, COPY_NAME];

impl ChatOverlay {
    pub(super) fn player_action(&mut self, token: u16) {
        let Some(menu) = self.player_menu.take() else {
            return;
        };
        let Some(name) = self.roster.name(menu.target) else {
            self.notice = "Player is no longer available.";
            return;
        };
        match token {
            WHISPER => {
                if let Some(input) = &mut self.input {
                    input.channel = Channel::Whisper;
                    input.recipient = Some(menu.target);
                    self.notice = "";
                }
            }
            IGNORE => {
                let ignored = if let Some(index) = self.muted.iter().position(|t| *t == menu.target)
                {
                    self.muted.remove(index);
                    false
                } else {
                    self.muted.push(menu.target);
                    true
                };
                for line in &mut self.lines {
                    if line.sender == Some(menu.target) {
                        line.muted = ignored;
                    }
                }
                self.notice = "";
            }
            FRIEND => {
                self.notice = match self.friends.toggle(name) {
                    Ok(_) => "",
                    Err(message) => message,
                };
            }
            COPY_NAME => {
                self.notice = if crate::console::clipboard::copy(
                    self.roster.display_name(menu.target).unwrap_or(name),
                ) {
                    ""
                } else {
                    "Clipboard unavailable."
                };
            }
            _ => {}
        }
    }
}
