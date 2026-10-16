//! Local-only zoom deadline history at acknowledged usercmd boundaries.
use super::{MovementConfig, MovementState, PlayerState, Predictor};

#[derive(Clone, Debug)]
/// Bounded local deadlines, never copied from speculative presentation previews.
pub(super) struct ZoomHistory {
    entries: [Option<(i32, i32)>; 128],
    next: usize,
}

impl Default for ZoomHistory {
    fn default() -> Self {
        Self {
            entries: [None; 128],
            next: 0,
        }
    }
}

impl ZoomHistory {
    /// Retain a committed command endpoint for reconciliation.
    pub(super) fn record(&mut self, state: &MovementState) {
        self.entries[self.next] = Some((state.command_time, state.zoom_lock_time));
        self.next = (self.next + 1) % self.entries.len();
    }
}

impl Predictor {
    /// Seed network fields from the server, retaining only local zoom deadlines at/before ACK.
    /// Pending/future commands must replay their own edges, never borrow a future deadline.
    pub fn reseed_player_state(&self, player: &PlayerState, config: MovementConfig) -> Self {
        let mut seed = Self::from_player_state(player, config);
        self.restore_history(&mut seed);
        seed
    }

    fn restore_history(&self, seed: &mut Self) {
        if seed.state.client_num != self.state.client_num
            || (seed.state.entity_flags ^ self.state.entity_flags) & crate::EF_TELEPORT_BIT != 0
            || seed.state.health <= 0
        {
            return;
        }
        seed.zoom_history = self.zoom_history.clone();
        for entry in &mut seed.zoom_history.entries {
            if entry.is_some_and(|(time, _)| time > seed.state.command_time) {
                *entry = None;
            }
        }
        seed.state.zoom_lock_time = seed
            .zoom_history
            .entries
            .iter()
            .flatten()
            .max_by_key(|(time, _)| *time)
            .map_or(0, |(_, deadline)| *deadline);
    }
}
