//! JoF EternalJK's `/fakenoclip` (`cg_consolecmds.c:2943`, `cg_predict.c:1239-1255`): fly
//! around the map locally while the server keeps a motionless player.
//!
//! The predictor runs a noclip move on the real input and is never pulled back to the
//! snapshot; [`LocalPrediction::command_for_server`] sends the server a still player with
//! the talk balloon up. Ending it, or the state it needs ending (death, spectating, a
//! vehicle), snaps back to the server's position. While it lasts every map area is drawn:
//! the snapshot's area mask is keyed to where the server thinks the player stands.

use super::*;

impl GpuState {
    /// Follow `cg_fakeNoclip`, switching it off when the player can no longer fly
    /// (`CG_PredictPlayerState`).
    pub(crate) fn sync_fake_noclip(&mut self, player: &sjk_protocol::PlayerState) {
        let free_camera = self.free_camera_active();
        let wanted = free_camera
            || self
                .console
                .as_ref()
                .and_then(|console| console.integer_cvar("cg_fakeNoclip"))
                .unwrap_or(0)
                != 0;
        let allowed = sjk_game_jka::prediction_policy::fake_noclip_allowed(
            player,
            !local_prediction::predicts_local_view(player.movement_flags()),
        );
        if wanted
            && !allowed
            && let Some(console) = &mut self.console
        {
            console.set_cvar("cg_fakeNoclip", "0");
            console.set_cvar("cg_freeCamera", "0");
        }
        self.local_prediction.set_fake_noclip(wanted && allowed);
        self.local_prediction
            .set_free_camera(free_camera && allowed, player.weapon());
        self.detached_camera = free_camera && allowed;
    }

    /// `/fakenoclip`: toggle, only while alive and on foot.
    pub(crate) fn fake_noclip_command(&mut self) -> Result<Vec<String>, String> {
        let player = self
            .live_session
            .as_ref()
            .map(|session| session.latest_snapshot().player.clone())
            .ok_or("fakenoclip: not in a game")?;
        let console = self.console.as_mut().ok_or("Console unavailable")?;
        console.set_cvar("cg_freeCamera", "0");
        if console.integer_cvar("cg_fakeNoclip").unwrap_or(0) != 0 {
            console.set_cvar("cg_fakeNoclip", "0");
            self.sync_fake_noclip(&player);
            return Ok(vec![
                "^1fakenoclip OFF^7 - snapping back to your real position.".into(),
            ]);
        }
        if !sjk_game_jka::prediction_policy::fake_noclip_allowed(
            &player,
            !local_prediction::predicts_local_view(player.movement_flags()),
        ) {
            return Err("fakenoclip: only available while alive and on foot.".into());
        }
        console.set_cvar("cg_fakeNoclip", "1");
        self.sync_fake_noclip(&player);
        Ok(vec![
            "^2fakenoclip ON^7 - flying locally; the server sees you standing still. \
             Type ^5/fakenoclip^7 again to snap back."
                .into(),
        ])
    }
}
