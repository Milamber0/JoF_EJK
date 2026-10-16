//! TaystJK client-info overrides; asset work remains in the normal refresh path.

use sjk_protocol::GameState;

/// Read the advertised team without creating a second client-info store.
pub(super) fn team(game: &GameState, client: u16) -> i32 {
    game.config_string(1131 + usize::from(client))
        .and_then(|config| sjk_client::LegacyClientInfo::new(config).integer("t"))
        .unwrap_or(0)
}

// TaystJK codemp/cgame/cg_players.c:2288-2297 changes only modelName,
// inside cg_forceModel, never the local client's model or selected skin.
/// Choose a remote model override while retaining the ordinary forced skin.
pub(super) fn model(
    forced: Option<&str>,
    overrides: &[String; 3],
    client: u16,
    local: (i32, i32),
    team: i32,
) -> Option<String> {
    let forced = forced?;
    if i32::from(client) == local.0 {
        return None;
    }
    let name = &overrides[usize::from(team == local.1 && local.1 != 0)];
    if name.is_empty() || name == "0" || name.eq_ignore_ascii_case("none") {
        return None;
    }
    let skin = forced.split_once('/').map_or("default", |(_, skin)| skin);
    Some(format!("{name}/{skin}"))
}

// TaystJK codemp/cgame/cg_players.c:2384-2420: one or two tokens; `none`
// leaves the corresponding advertised hilt alone, not a request to holster.
/// Replace only explicitly requested local hilt slots.
pub(super) fn sabers(names: &mut [Option<String>; 2], value: &str) {
    // CVU_ForceOwnSaber, cg_cvar.c:255-261 normalizes these to `none`.
    if value.len() > 64 || value == "0" {
        return;
    }
    for (slot, name) in names.iter_mut().zip(value.split_whitespace()) {
        if !name.eq_ignore_ascii_case("none") {
            *slot = Some(name.to_owned());
        }
    }
}
