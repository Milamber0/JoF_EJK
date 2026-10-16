//! Snapshot and prediction projection into retained HUD strings.

use super::*;

impl HudOverlay {
    pub(crate) fn update(
        &mut self,
        session: &ClientSession,
        time_ms: u64,
        predicted: Option<&MovementState>,
        console: Option<&ViewerConsole>,
    ) -> ClientHudData {
        let player = &session.latest_snapshot().player;
        self.guides.update(player, predicted, console);
        self.speed
            .update(predicted.map_or(player.velocity(), |p| p.velocity), console);
        self.update_votes(session.game_state(), player.team(), time_ms as i32, console);
        let values = legacy_predicted_hud_data(player, predicted);
        self.update_values(player, values, time_ms)
    }

    pub(crate) fn update_player(&mut self, player: &PlayerState, time_ms: u64) -> ClientHudData {
        let values = legacy_hud_data(player);
        self.update_values(player, values, time_ms)
    }

    pub(crate) fn update_demo_votes(
        &mut self,
        game_state: &GameState,
        player: &PlayerState,
        time_ms: u64,
        console: Option<&ViewerConsole>,
    ) {
        self.guides.update(player, None, console);
        self.update_votes(game_state, player.team(), time_ms as i32, console);
        self.speed.update(player.velocity(), console);
    }

    fn update_values(
        &mut self,
        player: &PlayerState,
        values: ClientHudData,
        time_ms: u64,
    ) -> ClientHudData {
        let maximum = player.max_health().max(1) as f32;
        self.icons.weapon(values.weapon);
        // Health and armour run to twice the maximum (125 at spawn, a large shield on a
        // full one): the meters draw what lies over it as a second, deeper band.
        let targets = [
            (values.health as f32 / maximum).clamp(0.0, 2.0),
            (values.armor as f32 / maximum).clamp(0.0, 2.0),
            f32::from(values.force) / 100.0,
        ];
        let ammo_target = radial::ammo_ratio(
            values.weapon,
            values.ammo,
            player.entity_flags() & (1 << 20) != 0,
        );
        if (self.ammo_ratio.target() - ammo_target).abs() > f32::EPSILON {
            self.ammo_ratio.retarget(
                ammo_target,
                time_ms,
                self.theme.motion.normal,
                Easing::SmoothStep,
            );
        }
        for (tween, target) in self.ratios.iter_mut().zip(targets) {
            if (tween.target() - target).abs() > f32::EPSILON {
                tween.retarget(
                    target,
                    time_ms,
                    self.theme.motion.normal,
                    Easing::SmoothStep,
                );
            }
        }
        if self.values == Some(values) {
            return values;
        }
        if self
            .values
            .is_none_or(|previous| previous.weapon != values.weapon)
        {
            self.weapon_shown_ms = Some(time_ms);
        }
        self.format_values(values);
        values
    }

    /// Rewrite every retained value string from `values`.
    pub(super) fn format_values(&mut self, values: ClientHudData) {
        use std::fmt::Write as _;
        self.health.clear();
        self.armor.clear();
        self.force.clear();
        self.weapon.clear();
        self.ammo.clear();
        self.health_value.clear();
        self.armor_value.clear();
        self.force_value.clear();
        self.weapon_value.clear();
        self.ammo_value.clear();
        self.style_value.clear();
        let _ = write!(self.health, "^1HEALTH ^7{}", values.health);
        let _ = write!(self.armor, "^5ARMOR ^7{}", values.armor);
        let _ = write!(self.force, "^5FORCE ^7{}", values.force);
        let _ = write!(self.health_value, "{}", values.health);
        let _ = write!(self.armor_value, "{}", values.armor);
        let _ = write!(self.force_value, "{}", values.force);
        self.weapon_value
            .push_str(crate::ingame_menu::weapon_name(values.weapon));
        let _ = write!(
            self.weapon,
            "^3WEAPON ^7{}",
            crate::ingame_menu::weapon_name(values.weapon)
        );
        match (values.saber_style, values.ammo) {
            (Some(style), _) => {
                let style = sjk_client::legacy_saber_style_name(style);
                let _ = write!(self.ammo, "^5STYLE ^7{style}");
                self.style_value.push_str(style);
            }
            (None, Some(ammo)) => {
                let _ = write!(self.ammo, "^3AMMO ^7{ammo}");
                let _ = write!(self.ammo_value, "{ammo}");
            }
            (None, None) => {
                self.ammo.push_str("^3AMMO ^7--");
                self.ammo_value.push_str("--");
            }
        }
        self.values = Some(values);
    }
}
