//! The per-frame hook of muting (`muted_players.rs`): read the mute list once, carry
//! out a mute the chat's profile card asked for, and keep the muted slots of the
//! server being played in step with the list, the hub's claims and the server's
//! players. Each frame costs a few comparisons; the slots are worked out again only
//! when one of those changed. Their looks (blade skin, holocron) go at once
//! (`looks.rs`); the models, sabers and sounds follow in
//! [`GpuState::refresh_muted_models`].

use super::*;

impl GpuState {
    /// Follow the mute list into the chat and the muted slots.
    pub(crate) fn update_muted_players(&mut self) {
        if let Some(console) = &self.console {
            player_mutes::load(console.config_directory());
        }
        if let Some(request) = self.chat.take_mute_request() {
            player_mutes::set_muted(request.key_id.as_deref(), &request.name, request.muted);
        }
        let Some(session) = self.live_session.as_ref() else {
            if self.muted_players.clear() {
                player_mutes::set_roster(std::iter::empty(), None);
            }
            self.chat.set_muted_slots(0);
            self.looks.set_muted(0);
            return;
        };
        let own = session.game_state().client_num;
        let roster = self.chat.roster_targets();
        let chat = &self.chat;
        self.muted_players.refresh(
            player_mutes::revision(),
            player_identity::revision(),
            roster,
            own,
            || {
                player_mutes::set_roster(
                    (0..muted_players::SLOTS as u16).map(|slot| chat.player_label(slot)),
                    usize::try_from(own).ok(),
                );
                player_mutes::slots()
            },
        );
        let muted = self.muted_players.slots().muted;
        self.chat.set_muted_slots(muted);
        // Their look too: the stock blade and its sounds, no holocron (`looks.rs`).
        self.looks.set_muted(muted);
    }

    /// Draw the muted players as the stand-in, or as themselves again, when the muted
    /// slots changed, and tell the sound which players to silence.
    pub(crate) fn refresh_muted_models(&mut self, audio: &mut Option<GameAudio>) {
        let muted = self.muted_players.slots().muted;
        if let Some(audio) = audio {
            audio.set_muted_clients(muted);
        }
        let changed = muted ^ self.muted_players.applied;
        if changed == 0 || self.live_session.is_none() {
            return;
        }
        self.muted_players.applied = muted;
        for client in 0..muted_players::SLOTS as u16 {
            if changed & (1 << client) != 0
                && let Err(error) = self.apply_clientinfo(client)
            {
                log::progress(format_args!("mute client {client}: {error}"));
            }
        }
    }
}
