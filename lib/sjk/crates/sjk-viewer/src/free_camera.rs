//! Detached local flight using the existing JoF EJK fake-noclip predictor and
//! frozen server command policy. The body remains at its authoritative entity.
use crate::{GpuState, camera, local_prediction};

impl GpuState {
    /// Whether this live session has a detached free camera.
    pub(crate) fn free_camera_active(&self) -> bool {
        self.live_session.is_some()
            && self
                .console
                .as_ref()
                .and_then(|console| console.integer_cvar("cg_freecamera"))
                .unwrap_or(0)
                != 0
    }

    /// Clear local flight at every session exit, and allow cancellation from menus.
    pub(crate) fn end_free_camera(&mut self) {
        if let Some(console) = &mut self.console {
            console.set_cvar("cg_freeCamera", "0");
            console.set_cvar("cg_fakeNoclip", "0");
        }
        self.local_prediction.set_free_camera(false, 0);
        self.local_prediction.set_fake_noclip(false);
        self.detached_camera = false;
        self.third_person_camera = camera::State::default();
    }

    /// `/freecam [on|off]`: toggle or explicitly set local camera flight.
    pub(crate) fn free_camera_command(&mut self, args: &[String]) -> Result<Vec<String>, String> {
        let current = self.free_camera_active();
        let on = match args {
            [] => !current,
            [value] if value.eq_ignore_ascii_case("on") || value == "1" => true,
            [value] if value.eq_ignore_ascii_case("off") || value == "0" => false,
            _ => return Err("Usage: freecam [on|off]".into()),
        };
        if !on {
            self.end_free_camera();
            return Ok(vec!["freecam OFF: returning to your player.".into()]);
        }
        let player = self
            .live_session
            .as_ref()
            .ok_or("freecam: not in a game")?
            .latest_snapshot()
            .player
            .clone();
        if on
            && !sjk_game_jka::prediction_policy::fake_noclip_allowed(
                &player,
                !local_prediction::predicts_local_view(player.movement_flags()),
            )
        {
            return Err("freecam: only available while alive and on foot".into());
        }
        let console = self.console.as_mut().ok_or("Console unavailable")?;
        console.set_cvar("cg_freeCamera", if on { "1" } else { "0" });
        console.set_cvar("cg_fakeNoclip", "0");
        self.sync_fake_noclip(&player);
        self.detached_camera = on;
        self.third_person_camera = camera::State::default();
        Ok(vec![if on {
            "freecam ON: movement flies the camera; attack speeds up flight. Your body stays on the server. /freecam off returns.".into()
        } else {
            "freecam OFF: returning to your player.".into()
        }])
    }
}

/// Keep the local model out of the main view while flight starts inside its head.
/// The ordinary third-person render flag preserves the body in portal views.
pub(crate) fn inside_body(camera: glam::Vec3, feet: glam::Vec3, height: f32) -> bool {
    camera.distance_squared(feet + glam::Vec3::Z * height) < 32.0 * 32.0
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_body_returns_when_the_camera_leaves_the_head() {
        let feet = glam::Vec3::new(10., 20., 30.);
        let eye = feet + glam::Vec3::Z * 26.;
        assert!(super::inside_body(eye, feet, 26.));
        assert!(!super::inside_body(eye + glam::Vec3::X * 40., feet, 26.));
    }
}
