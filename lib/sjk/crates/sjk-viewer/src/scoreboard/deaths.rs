//! Session-local death counts for jaPRO scoreboard modes 2 and 3.
use super::*;

/// Fixed counters, consuming the existing obituary feed exactly once.
pub(super) struct Deaths {
    counts: [i32; 64],
    consumed: u64,
    time: i32,
    mode: i64,
    gametype: i32,
}

impl Default for Deaths {
    fn default() -> Self {
        Self {
            counts: [0; 64],
            consumed: 0,
            time: 0,
            mode: 1,
            gametype: 0,
        }
    }
}

impl Scoreboard {
    /// Sample the cvar and accept new obituary counters without rebuilding the name cache.
    pub(crate) fn observe_deaths(
        &mut self,
        tracker: &sjk_client::ObituaryTracker,
        snapshot: &sjk_protocol::Snapshot,
        game: &GameState,
        console: Option<&crate::console::ViewerConsole>,
    ) {
        let d = &mut self.deaths;
        if snapshot.server_time < d.time || tracker.decoded() < d.consumed {
            d.counts.fill(0);
            d.consumed = 0;
        }
        d.time = snapshot.server_time;
        d.mode = console
            .and_then(|c| c.integer_cvar("cg_scoredeaths"))
            .unwrap_or(1);
        d.gametype = game
            .config_string(0)
            .and_then(|b| sjk_client::LegacyClientInfo::new(b).integer("g_gametype"))
            .unwrap_or(0);
        let mut killer = None;
        // Oldest first, so the newest kill of the viewing player wins.
        for offset in (0..(tracker.decoded() - d.consumed).min(8) as usize).rev() {
            let Some(event) = tracker.feed().newest(offset) else {
                continue;
            };
            if let Some(count) = d.counts.get_mut(usize::from(event.target)) {
                *count = count.saturating_add(1);
            }
            // `cg.killerName`: set when another player kills you.
            if event.local_was_killed && event.attacker < 32 && event.attacker != event.target {
                killer = Some(event.attacker);
            }
        }
        d.consumed = tracker.decoded();
        if let Some(client) = killer {
            self.killer = Some(client);
            self.killer_name.clear();
            self.killer_name
                .push_str(&client_identity(game, client as u8).0);
        }
    }
}

impl Deaths {
    /// Without a JA+ plugin handshake, mode 1 is deliberately hidden (stock rule).
    pub(super) fn count(&self, client: u8) -> Option<i32> {
        (matches!(self.mode, 2 | 3) && !matches!(self.gametype, 3 | 7))
            .then(|| self.counts.get(usize::from(client)).copied())
            .flatten()
    }
}
