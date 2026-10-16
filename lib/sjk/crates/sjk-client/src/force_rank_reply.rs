//! Client reply to the server's `nfr` Force-rank notice.
//!
//! `WP_InitForcePowers` (`codemp/game/w_force.c:368-392`) parks a client in
//! spectator mode and sends `spc` + `nfr <rank> 1 <team>` whenever the
//! `forcepowers` userinfo had to be legalized or the session had never set its
//! powers. Stock cgame opens the Force menu; closing it runs
//! `UI_UpdateClientForcePowers` (`codemp/ui/ui_force.c:192-216`), which
//! rewrites the `forcepowers` cvar and issues `forcechanged ["TEAM"]`. The
//! server re-initializes the powers immediately for spectators and, when a
//! team argument is present, hands it to `Cmd_Team_f`
//! (`codemp/game/g_cmds.c:1276-1313`). This module derives the legalizer
//! inputs from `CS_SERVERINFO` exactly as the server reads its own cvars and
//! produces that reply without a legacy menu.

use crate::force_profile::{ForceLegalizeRules, ForceSide, legalize_force_powers};
use crate::team_commands::LegacyTeamChoice;
use sjk_protocol::{GameState, InfoString};

const CS_SERVERINFO: usize = 0;
/// `gametype_t` values from `codemp/game/bg_public.h:235-244`.
const GT_JEDIMASTER: i32 = 2;
const GT_DUEL: i32 = 3;
const GT_POWERDUEL: i32 = 4;
/// `team_t` values from `codemp/game/bg_public.h`.
const TEAM_RED: i32 = 1;
const TEAM_BLUE: i32 = 2;
/// `weapon_t` values from `codemp/game/bg_weapons.h:30-60`.
const WP_NONE: u32 = 0;
const WP_SABER: u32 = 3;
const WP_NUM_WEAPONS: u32 = 19;
/// `FORCE_MASTERY_JEDI_MASTER` / `NUM_FORCE_MASTERY_LEVELS`
/// (`codemp/game/bg_public.h:456-457`).
const FORCE_MASTERY_JEDI_MASTER: i32 = 7;
const NUM_FORCE_MASTERY_LEVELS: i32 = 8;

/// Mirror `HasSetSaberOnly` (`codemp/game/w_saber.c:9459-9489`).
fn has_set_saber_only(gametype: i32, weapon_disable: u32, duel_weapon_disable: u32) -> bool {
    if gametype == GT_JEDIMASTER {
        return false;
    }
    let disabled = if matches!(gametype, GT_DUEL | GT_POWERDUEL) {
        duel_weapon_disable
    } else {
        weapon_disable
    };
    (0..WP_NUM_WEAPONS)
        .filter(|weapon| !matches!(*weapon, WP_NONE | WP_SABER))
        .all(|weapon| disabled & (1 << weapon) != 0)
}

/// Build the legalizer inputs `WP_InitForcePowers` uses (`w_force.c:240-251`)
/// from the server's advertised cvars.
///
/// `rank` is the first `nfr` argument, which the server already clamped to
/// `FORCE_MASTERY_JEDI_MASTER` when `g_maxForceRank` was out of range
/// (`w_force.c:164-167`); the same clamp is applied here for safety. `team` is
/// the third argument and only matters when `g_forceBasedTeams` is active.
pub fn force_rules_from_serverinfo(
    game_state: &GameState,
    rank: i32,
    team: i32,
) -> ForceLegalizeRules {
    let info = game_state
        .config_string(CS_SERVERINFO)
        .and_then(|raw| std::str::from_utf8(raw).ok())
        .and_then(|text| InfoString::parse(text).ok());
    let integer = |key: &str| {
        info.as_ref()
            .and_then(|info| info.get_i32(key))
            .unwrap_or(0)
    };
    let mask = |key: &str| integer(key).unsigned_abs();
    let gametype = integer("g_gametype");
    let rank = if rank <= 0 || rank >= NUM_FORCE_MASTERY_LEVELS {
        FORCE_MASTERY_JEDI_MASTER
    } else {
        rank
    };
    let team_side = if integer("g_forceBasedTeams") != 0 {
        match team {
            TEAM_RED => Some(ForceSide::Dark),
            TEAM_BLUE => Some(ForceSide::Light),
            _ => None,
        }
    } else {
        None
    };
    ForceLegalizeRules {
        max_rank: u8::try_from(rank).unwrap_or(7),
        free_saber: has_set_saber_only(
            gametype,
            mask("g_weaponDisable"),
            mask("g_duelWeaponDisable"),
        ),
        team_side,
        gametype,
        disabled_mask: mask("g_forcePowerDisable"),
    }
}

/// The server's Force rules as `CS_SERVERINFO` advertises them, with its
/// `g_maxForceRank` as the rank ceiling (`WP_InitForcePowers` puts an out of
/// range one back to Jedi Master, `w_force.c:164-167`).
pub fn server_force_rules(game_state: &GameState) -> ForceLegalizeRules {
    let max_rank = game_state
        .config_string(CS_SERVERINFO)
        .and_then(|raw| std::str::from_utf8(raw).ok())
        .and_then(|text| InfoString::parse(text).ok())
        .and_then(|info| info.get_i32("g_maxForceRank"))
        .unwrap_or(0);
    force_rules_from_serverinfo(game_state, max_rank, 0)
}

/// The userinfo value and reliable command answering one `nfr` notice.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForceRankReply {
    /// Legalized `forcepowers` value; must reach the server before `command`.
    pub forcepowers: String,
    /// Whether `forcepowers` differs from the value the client had been sending.
    pub changed: bool,
    /// Byte-exact `forcechanged` command, with `UI_TeamName`'s upper-case team
    /// argument (`codemp/ui/ui_main.c:530-538`) when play should resume.
    pub command: Vec<u8>,
}

/// Legalize `current` under `rules` and pick the `forcechanged` form.
///
/// `rejoin` is the team the player asked for before the server parked them;
/// `None` mirrors the stock "SPECTATOR" path (`ui_main.c:6345-6358`), which only
/// refreshes the powers and leaves the player in spectator mode.
pub fn force_rank_reply(
    current: &str,
    rules: ForceLegalizeRules,
    rejoin: Option<LegacyTeamChoice>,
) -> ForceRankReply {
    let forcepowers = legalize_force_powers(current, rules).allocation.encode();
    let team = match rejoin {
        None | Some(LegacyTeamChoice::Spectator) => None,
        Some(LegacyTeamChoice::Free | LegacyTeamChoice::Auto) => Some("FREE"),
        Some(LegacyTeamChoice::Red) => Some("RED"),
        Some(LegacyTeamChoice::Blue) => Some("BLUE"),
    };
    let command = match team {
        Some(team) => format!("forcechanged \"{team}\"").into_bytes(),
        None => b"forcechanged".to_vec(),
    };
    ForceRankReply {
        changed: forcepowers != current,
        forcepowers,
        command,
    }
}
