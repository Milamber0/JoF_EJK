//! The per-frame side of holocrons (`holocrons.rs`): twice a second, with the identity's
//! turn (`identity_frame.rs`), work out whether the player is actively playing and tell the
//! identity service, which says so with every claim. The hub counts the time toward the
//! next holocron only while it does ([`crate::holocrons::activity`]).
//!
//! It costs a few comparisons and an atomic read, and sends the service a message only when
//! the answer changes.

use super::*;
use crate::holocrons::activity::{self, Play};

impl GpuState {
    /// Tell the identity service whether the player is actively playing now: in a live
    /// game of someone else's, in their own view, mid-match, out of the menus, and with
    /// input in the last two minutes.
    pub(crate) fn update_holocron_activity(&mut self) {
        let play = self
            .live_session
            .as_ref()
            .map_or_else(Play::default, |session| {
                Play::from_state(
                    !session.is_local(),
                    &session.latest_snapshot().player,
                    session.game_state().client_num,
                    self.menu_has_the_player(),
                )
            });
        let active = play.active(activity::since_input(Instant::now()));
        let _ = player_identity::set_active(active);
    }

    /// A menu, the console, a dialog or a pop-up has the player's attention rather than
    /// the game.
    fn menu_has_the_player(&self) -> bool {
        self.game_menu
            || self
                .console
                .as_ref()
                .is_some_and(console::ViewerConsole::is_open)
            || self.text_dialog.is_open()
            || self.medal_popup.is_open()
            || self.holocron_popup.is_open()
            || self
                .client_menu
                .as_ref()
                .is_some_and(|menu| menu.is_visible())
    }
}
