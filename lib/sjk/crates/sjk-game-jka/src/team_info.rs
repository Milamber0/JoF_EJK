//! The team overlay (`CheckTeamStatus`, `TeamplayInfoMessage`, `g_team.c:1184-1305`):
//! once a second in a game with teams, every connected client on the red or blue team is
//! sent `tinfo` — for each player of its team, in client order and at most
//! `TEAM_MAXOVERLAY`, its number, location, health, armour, weapon and powerups.
//!
//! Not yet: a spectator following a player is sent the followed player's team (there is
//! no following), siege's temporary spectators.

/// `TEAM_LOCATION_UPDATE_TIME`: how often the overlay is sent.
pub const TEAM_INFO_INTERVAL: i32 = 1_000;
/// `TEAM_MAXOVERLAY`: the most players an overlay names.
const TEAM_MAXOVERLAY: usize = 32;
const TEAM_RED: i32 = 1;
const TEAM_BLUE: i32 = 2;

/// A player as the overlay shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayEntry {
    pub client: u16,
    /// `sess.sessionTeam`.
    pub team: i32,
    /// `pers.teamState.location`: its `CS_LOCATIONS` number, 0 for none.
    pub location: u32,
    /// `ps.stats[STAT_HEALTH]`, `ps.stats[STAT_ARMOR]`.
    pub health: i32,
    pub armor: i32,
    /// `ps.weapon`, `s.powerups`.
    pub weapon: u32,
    pub powerups: u32,
}

/// When the overlay is next due (`level.lastTeamLocationTime`): `Some` new time when
/// `level_time` is more than a second past the last.
pub fn due(last: i32, level_time: i32) -> Option<i32> {
    (level_time - last > TEAM_INFO_INTERVAL).then_some(level_time)
}

/// `TeamplayInfoMessage` for a client on `team`, from `everyone` in the game in client
/// order: `None` for a client on no team.
pub fn message(team: i32, everyone: &[OverlayEntry]) -> Option<Vec<u8>> {
    if team != TEAM_RED && team != TEAM_BLUE {
        return None;
    }
    let mut entries = String::new();
    let mut count = 0;
    for player in everyone
        .iter()
        .filter(|player| player.team == team)
        .take(TEAM_MAXOVERLAY)
    {
        let entry = format!(
            " {} {} {} {} {} {}",
            player.client,
            player.location,
            player.health.max(0),
            player.armor.max(0),
            player.weapon,
            player.powerups
        );
        // `string[8192]`.
        if entries.len() + entry.len() >= 8_192 {
            break;
        }
        entries.push_str(&entry);
        count += 1;
    }
    Some(format!("tinfo {count} {entries}").into_bytes())
}
