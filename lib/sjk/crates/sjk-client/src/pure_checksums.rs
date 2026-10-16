//! The `cp` reply a pure server requires before it lets a client play.
//!
//! `SV_SendClientGameState` clears `gotCP` for every gamestate
//! (`codemp/server/sv_client.cpp:504-505`), and until the client answers with
//! `cp`, `SV_UserMove` returns before it can enter the client into the world
//! (`:1394-1400`). The return is silent: snapshots keep arriving, nothing the
//! player does reaches the game, and the server never says why. Stock answers
//! it after *every* gamestate, not just the first
//! (`CL_SendPureChecksums`, `codemp/client/cl_main.cpp:1154-1160,1382`).

use crate::GameState;
use sjk_protocol::InfoString;

/// Whether this server demands pure clients (`sv_pure` in the systeminfo).
pub fn legacy_server_is_pure(game_state: &GameState) -> bool {
    let Some(system_info) = game_state.config_string(1) else {
        return false;
    };
    let Ok(system_info) = std::str::from_utf8(system_info) else {
        return false;
    };
    let Ok(system_info) = InfoString::parse(system_info) else {
        return false;
    };
    system_info.get_i32("sv_pure").is_some_and(|pure| pure != 0)
}

/// The `cp` reliable command for a pure server, or `None` when the server does
/// not run pure.
///
/// `pure_checksum` is the client's checksum of the assets it loaded, seeded
/// with the gamestate's checksum feed. The shape mirrors
/// `FS_ReferencedPakPureChecksums` for a single referenced pak, which is what
/// SJK mounts; a server that references its own paks will not match it (see
/// `KNOWN_ISSUES.md`).
pub fn legacy_pure_checksum_command(game_state: &GameState, pure_checksum: i32) -> Option<Vec<u8>> {
    if !legacy_server_is_pure(game_state) {
        return None;
    }
    let proof = game_state.checksum_feed ^ pure_checksum ^ 1;
    Some(format!("cp {pure_checksum} {pure_checksum} @ {pure_checksum} {proof}").into_bytes())
}
