//! BaseJKA entity colour (`customRGBA`) resolution for rendered actors.
//!
//! The server fills `playerState_t::customRGBA` from the `char_color_*`
//! userinfo keys (`codemp/game/g_client.c:2177-2198`) and copies it onto
//! body-queue corpses (`g_client.c:1135-1138`). cgame forwards it as the
//! render entity's `shaderRGBA`:
//!
//! - `CG_Player` prefers `clientInfo_t::colorOverride` when it is non-zero and
//!   otherwise uses `entityState_t::customRGBA`
//!   (`codemp/cgame/cg_players.c:9016-9030`). `colorOverride` is produced by
//!   `BG_ValidateSkinForTeam` for `jedi_*` custom-species models in team
//!   gametypes other than siege and jedi-vs-merc
//!   (`cg_players.c:481-488`, `codemp/game/bg_misc.c:2643-2660`): red team
//!   `(1, 0, 0)`, blue team `(0, 0, 1)`.
//! - `CG_General`, which renders `ET_BODY` corpses, copies `customRGBA`
//!   verbatim (`codemp/cgame/cg_ents.c`, `CG_General`).
//!
//! Only material stages whose colour generator reads the entity colour
//! (`entity`, `oneMinusEntity`, `lightingDiffuseEntity`) observe the result;
//! that mapping lives in the renderer.

use sjk_protocol::GameState;

const CS_SERVERINFO: usize = 0;
const CS_PLAYERS: usize = 1_131;
/// `GT_TEAM` / `GT_SIEGE` from `codemp/game/bg_public.h` `gametype_t`.
const GT_TEAM: i32 = 6;
const GT_SIEGE: i32 = 7;
/// `TEAM_RED` / `TEAM_BLUE` from `team_t`.
const TEAM_RED: i32 = 1;
const TEAM_BLUE: i32 = 2;

/// Per-gamestate inputs to the team colour override rule, resolved once per
/// snapshot so entity resolution stays a fixed-cost lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TeamColorPolicy {
    /// `cgs.gametype >= GT_TEAM && !cgs.jediVmerc && cgs.gametype != GT_SIEGE`.
    pub team_override_active: bool,
}

impl TeamColorPolicy {
    /// Read `g_gametype` and `g_jediVmerc` from `CS_SERVERINFO`.
    pub fn from_game_state(game_state: &GameState) -> Self {
        Self::from_server_info(game_state.config_string(CS_SERVERINFO).unwrap_or(&[]))
    }

    /// Evaluate the rule over a raw `CS_SERVERINFO` string.
    pub fn from_server_info(serverinfo: &[u8]) -> Self {
        let gametype = info_int(serverinfo, "g_gametype");
        let jedi_vs_merc = info_int(serverinfo, "g_jediVmerc") != 0;
        Self {
            team_override_active: gametype >= GT_TEAM && !jedi_vs_merc && gametype != GT_SIEGE,
        }
    }
}

/// Resolve the colour `CG_Player` hands to the renderer for a live player.
///
/// `custom_rgba` is the entity's (or the local player state's) `customRGBA`.
pub fn legacy_player_color(
    game_state: &GameState,
    policy: TeamColorPolicy,
    client_num: u16,
    custom_rgba: [u8; 4],
) -> [u8; 4] {
    if !policy.team_override_active {
        return custom_rgba;
    }
    let Some(config) = game_state.config_string(CS_PLAYERS + usize::from(client_num)) else {
        return custom_rgba;
    };
    player_color_from_config(config, custom_rgba)
}

/// `colorOverride` selection over one raw `CS_PLAYERS` string.
fn player_color_from_config(config: &[u8], custom_rgba: [u8; 4]) -> [u8; 4] {
    let is_custom_species = info_value(config, "model")
        .is_some_and(|model| model.len() > 5 && model[..5].eq_ignore_ascii_case(b"jedi_"));
    if !is_custom_species {
        return custom_rgba;
    }
    match info_int(config, "t") {
        TEAM_RED => [255, 0, 0, custom_rgba[3]],
        TEAM_BLUE => [0, 0, 255, custom_rgba[3]],
        _ => custom_rgba,
    }
}

/// Resolve the colour `CG_General` hands to the renderer for a corpse.
pub fn legacy_body_color(custom_rgba: [u8; 4]) -> [u8; 4] {
    custom_rgba
}

/// Allocation-free `Info_ValueForKey` over a raw configstring.
fn info_value<'a>(info: &'a [u8], key: &str) -> Option<&'a [u8]> {
    let mut fields = info
        .split(|byte| *byte == b'\\')
        .skip_while(|field| field.is_empty());
    while let (Some(candidate), Some(value)) = (fields.next(), fields.next()) {
        if candidate.eq_ignore_ascii_case(key.as_bytes()) {
            return Some(value);
        }
    }
    None
}

/// `atoi(Info_ValueForKey(...))` with a zero default.
fn info_int(info: &[u8], key: &str) -> i32 {
    info_value(info, key).map_or(0, crate::team_info::legacy_atoi)
}
