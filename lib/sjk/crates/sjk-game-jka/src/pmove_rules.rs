//! System/server info adapters for the shared on-foot predictor.

use crate::pmove::MovementConfig;
use sjk_protocol::{GameState, InfoString};

/// `CS_LEGACY_FIXES` (`bg_public.h:123`): which animation fixes the server runs.
pub const CS_LEGACY_FIXES: usize = 36;

impl MovementConfig {
    /// Server side: the configstring that tells clients which legacy fixes this
    /// simulation runs, as `UpdateLegacyFixesConfigstring` writes it (`g_cvar.c:45`).
    /// Clients predict animations from it, so it has to be what the server simulates.
    pub fn legacy_fixes_config_string(&self) -> (usize, Vec<u8>) {
        (CS_LEGACY_FIXES, self.legacy_fixes.to_string().into_bytes())
    }

    /// Read CS_SYSTEMINFO movement cvars and CS_SERVERINFO g_stepSlideFix.
    /// cg_predict.c:1059-1068,1247; cg_servercmds.c:138; cg_xcvar.h:157-159.
    pub fn from_game_state(game: &GameState) -> Self {
        let mut config = Self::default();
        config.refresh_game_state(game);
        config
    }

    /// Refresh advertised policy, retaining already-loaded saber restrictions/scales.
    pub fn refresh_game_state(&mut self, game: &GameState) {
        let integer = |index, key, default| {
            game.config_string(index)
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
                .and_then(|text| InfoString::parse(text).ok())
                .and_then(|info| info.get_i32(key))
                .unwrap_or(default)
        };
        self.fixed_millis =
            (integer(1, "pmove_fixed", 0) != 0).then(|| integer(1, "pmove_msec", 8).clamp(8, 33));
        self.snap_velocity = integer(1, "pmove_float", 0) == 0;
        // atoi(Info_ValueForKey(...)) is zero when the server omits this key.
        self.step_slide_fix = integer(0, "g_stepSlideFix", 0) != 0;
        // cg_servercmds.c:140; cg_predict.c:1248.
        self.no_spectator_move = integer(0, "g_noSpecMove", 0) != 0;
        // `CS_LEGACY_FIXES`, configstring 36, a decimal bit mask (`g_cvar.c:45`); a server
        // that predates it publishes nothing and runs none of the fixes
        // (`bg_pmove.c:5168-5170`, `strtoul` of an empty string).
        self.legacy_fixes = game
            .config_string(CS_LEGACY_FIXES)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .map(|text| text.trim_start())
            .map(|text| {
                &text[..text
                    .find(|digit: char| !digit.is_ascii_digit())
                    .unwrap_or(text.len())]
            })
            .and_then(|digits| digits.parse().ok())
            .unwrap_or(0);
        // `cgs.debugMelee` (`cg_servercmds.c:137`), read by the dialect.
        self.debug_melee = crate::pmove_debug_melee::DebugMelee::from_game_state(game);
        self.grapple = crate::pmove::grapple::GrappleRules::from_game_state(game);
        let no_rolls = self.roll_rules.saber_forbids_rolls;
        self.roll_rules = crate::RollRules::from_game_state(game);
        self.roll_rules.saber_forbids_rolls = no_rolls;
        self.ja_plus = crate::pmove_japlus::JaPlusRules::from_game_state(game);
    }

    /// CG_PredictPlayerState's fixed-step timestamp ceiling (:1239-1241).
    /// This is prediction-only: the original user command remains untouched.
    pub fn command_time(&self, time: i32) -> i32 {
        self.fixed_millis.map_or(time, |msec| {
            let msec = i64::from(msec.clamp(8, 33));
            (((i64::from(time) + msec - 1) / msec) * msec) as i32
        })
    }
}
