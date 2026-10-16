//! BaseJKA team-menu commands emitted over the reliable-command channel.
//!
//! `Cmd_Team_f` forwards its sole argument to `SetTeam`
//! (`codemp/game/g_cmds.c:974-1028`). `SetTeam` accepts both long and short
//! forms (`g_cmds.c:659-680`). The retail MP menu sends `team free`, `team
//! red`, `team blue`, and `team s` (`assets1.pk3:ui/jamp/ingame_join.menu:
//! 104-108,142-146,180-184,289-294`), so these payloads intentionally match
//! those bytes and contain no newline or terminator.

/// A selection offered by the modern join/team screen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyTeamChoice {
    /// Join an FFA/duel game.
    Free,
    /// Let a team-mode server choose the smaller team.
    Auto,
    /// Join red.
    Red,
    /// Join blue.
    Blue,
    /// Enter free spectator mode.
    Spectator,
}

/// Return the byte-exact BaseJKA reliable command for a team choice.
pub const fn legacy_team_command(choice: LegacyTeamChoice) -> &'static [u8] {
    match choice {
        LegacyTeamChoice::Free | LegacyTeamChoice::Auto => b"team free",
        LegacyTeamChoice::Red => b"team red",
        LegacyTeamChoice::Blue => b"team blue",
        LegacyTeamChoice::Spectator => b"team s",
    }
}
