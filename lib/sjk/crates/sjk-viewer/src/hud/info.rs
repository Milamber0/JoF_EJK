//! Match-information projection into retained HUD storage.

use super::*;
use sjk_client::{CrosshairName, ObituaryTracker, WarmupText};
use std::fmt::Write as _;

impl HudOverlay {
    /// `local_client` is the viewed player's client number, whose kills and
    /// deaths the kill feed marks.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update_match_information(
        &mut self,
        game: &GameState,
        server_time: i32,
        obituaries: &ObituaryTracker,
        local_client: u16,
        lagometer: &sjk_client::LagometerSamples,
        crosshair: Option<CrosshairName>,
        interrupted: bool,
    ) {
        self.kill_feed
            .observe(obituaries, game, server_time, local_client);
        self.crosshair_name.clear();
        self.crosshair_alpha = 0.0;
        self.crosshair_teammate = false;
        if let Some(target) = crosshair {
            family::name(
                &mut self.crosshair_name,
                &client_name_and_team(game, target.client_num).0,
                self.family.name_colors,
            );
            self.crosshair_alpha = target.alpha;
            self.crosshair_teammate = target.teammate;
        }
        self.match_timer.clear();
        family::timer(&mut self.match_timer, game, server_time, self.family);
        self.warmup_text.clear();
        match sjk_client::legacy_warmup_text(game, server_time) {
            WarmupText::Hidden => {}
            WarmupText::WaitingForPlayers => {
                self.warmup_text.push_str("WAITING FOR PLAYERS");
            }
            WarmupText::StartsIn(seconds) => {
                let _ = write!(self.warmup_text, "STARTS IN: {seconds}");
            }
        }
        self.interrupted = interrupted;
        self.lagometer.clone_from(lagometer);
    }
}
