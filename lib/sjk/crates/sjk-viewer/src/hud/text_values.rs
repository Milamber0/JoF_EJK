//! Retained HUD text resolution, independent of layout.
use super::*;

impl HudOverlay {
    /// Resolve a widget text ID without formatting or allocating.
    pub(crate) fn resolve_text(&self, id: TextId) -> &str {
        match id.0 {
            0 => &self.health,
            1 => &self.armor,
            2 => &self.force,
            3 => &self.weapon,
            4 => &self.ammo,
            6 => &self.health_value,
            8 => &self.armor_value,
            10 => &self.force_value,
            12 => &self.weapon_value,
            14 => &self.ammo_value,
            15 => &self.style_value,
            200 => &self.vote_heading,
            201 => &self.vote_text,
            202 => &self.vote_keys,
            203 => &self.team_vote_heading,
            204 => &self.team_vote_text,
            310 => &self.crosshair_name,
            311 => &self.match_timer,
            312 => &self.warmup_text,
            313 => "CONNECTION INTERRUPTED",
            314 => &self.speed.text,
            315 => &self.score_text,
            316 => &self.snapshot_text,
            317 => &self.targeting.position,
            318 => &self.enemy_info.name,
            319 => &self.enemy_info.detail,
            value @ vehicle::TEXT_FIRST..=vehicle::TEXT_LAST => self.vehicle.text(value),
            value @ 400..=415 => self.icons.text((value - 400) as usize),
            value @ kill_feed::TEXT_FIRST..=kill_feed::TEXT_LAST => self.kill_feed.text(value),
            _ => selection::text(id),
        }
    }
}

/// The retail font the cgame drew a HUD text with, for `ui_gameFont`; `None`
/// keeps the HUD's own font (Inter, or `arialnb` with `cg_classicHudFont`).
///
/// OpenJK `codemp` `cg_draw.c`: `CG_DrawCrosshairNames`, `CG_DrawWarmup`,
/// `CG_DrawTimer` and `CG_DrawEnemyInfo` paint with `FONT_MEDIUM`; the Force and
/// inventory selection names use `UI_SMALLFONT` (`FONT_SMALL`); vote,
/// snapshot and `CG_DrawDisconnect` draw console characters
/// (`CG_DrawSmallString`, `CG_DrawStringExt`, `CG_DrawBigString`). Retail has no
/// kill feed (obituaries are console prints, `CG_Obituary`): SJK's
/// ([`super::kill_feed`]) stays on the HUD font it is measured in.
pub(super) fn retail_font(id: TextId) -> Option<RetailFont> {
    match id.0 {
        310..=312 | 318 | 319 => Some(RetailFont::Medium),
        1000..=1022 | 1100..=1111 => Some(RetailFont::Small),
        200..=204 | 313 | 316 => Some(RetailFont::Console),
        _ => None,
    }
}

#[cfg(test)]
mod retail_font_tests {
    use super::*;

    #[test]
    fn crosshair_name_uses_the_medium_font_and_status_values_their_own() {
        assert_eq!(retail_font(TextId(310)), Some(RetailFont::Medium));
        assert_eq!(retail_font(TextId(1003)), Some(RetailFont::Small));
        assert_eq!(retail_font(TextId(201)), Some(RetailFont::Console));
        // Health, ammo, the selector headings and the kill feed stay on the HUD font.
        for id in [
            0,
            6,
            14,
            100,
            1200,
            1201,
            kill_feed::TEXT_FIRST,
            kill_feed::TEXT_LAST,
        ] {
            assert_eq!(retail_font(TextId(id)), None);
        }
    }
}
