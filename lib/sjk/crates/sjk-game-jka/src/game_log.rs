//! The game log (`G_LogPrintf`, `g_main.c:1506`): what `g_log` records of a match, a
//! line an event, each stamped with the level's minutes and seconds; a dedicated server
//! prints the same lines, unstamped, on its console.
//!
//! These are the lines' texts; the server decides when each is written.

/// `modNames` (`g_combat.c:769`): a means of death's name. The table stops before
/// `MOD_TEAM_CHANGE`, whose slot is empty (`(null)` as glibc prints it).
const MEANS: [&str; 42] = [
    "MOD_UNKNOWN",
    "MOD_STUN_BATON",
    "MOD_MELEE",
    "MOD_SABER",
    "MOD_BRYAR_PISTOL",
    "MOD_BRYAR_PISTOL_ALT",
    "MOD_BLASTER",
    "MOD_TURBLAST",
    "MOD_DISRUPTOR",
    "MOD_DISRUPTOR_SPLASH",
    "MOD_DISRUPTOR_SNIPER",
    "MOD_BOWCASTER",
    "MOD_REPEATER",
    "MOD_REPEATER_ALT",
    "MOD_REPEATER_ALT_SPLASH",
    "MOD_DEMP2",
    "MOD_DEMP2_ALT",
    "MOD_FLECHETTE",
    "MOD_FLECHETTE_ALT_SPLASH",
    "MOD_ROCKET",
    "MOD_ROCKET_SPLASH",
    "MOD_ROCKET_HOMING",
    "MOD_ROCKET_HOMING_SPLASH",
    "MOD_THERMAL",
    "MOD_THERMAL_SPLASH",
    "MOD_TRIP_MINE_SPLASH",
    "MOD_TIMED_MINE_SPLASH",
    "MOD_DET_PACK_SPLASH",
    "MOD_VEHICLE",
    "MOD_CONC",
    "MOD_CONC_ALT",
    "MOD_FORCE_DARK",
    "MOD_SENTRY",
    "MOD_WATER",
    "MOD_SLIME",
    "MOD_LAVA",
    "MOD_CRUSH",
    "MOD_TELEFRAG",
    "MOD_FALLING",
    "MOD_SUICIDE",
    "MOD_TARGET_LASER",
    "MOD_TRIGGER_HURT",
];
/// `MOD_MAX`.
const MEANS_COUNT: u32 = 43;
/// `ENTITYNUM_WORLD`.
const WORLD: i32 = 1022;
/// `MAX_CLIENTS`.
const MAX_CLIENTS: i32 = 32;

/// The stamp before a line in the file: the level's `mins:seconds` (`%i:%02i `).
pub fn stamp(level_time: i32, start_time: i32) -> String {
    let seconds = (level_time - start_time) / 1000;
    format!("{}:{:02} ", seconds / 60, seconds % 60)
}

/// `TeamName`.
pub fn team_name(team: i32) -> &'static str {
    match team {
        1 => "RED",
        2 => "BLUE",
        3 => "SPECTATOR",
        _ => "FREE",
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Who a line is about: slot, address (`sess.IP`), `pers.guid` and name.
#[derive(Clone, Copy, Debug)]
pub struct Who<'a> {
    pub client: usize,
    pub address: &'a [u8],
    pub guid: &'a [u8],
    pub name: &'a [u8],
}

impl Who<'_> {
    fn head(&self) -> String {
        format!(
            "{} [{}] ({}) \"{}^7\"",
            self.client,
            text(self.address),
            text(self.guid),
            text(self.name)
        )
    }
}

/// The two lines `G_InitGame` starts a level's log with.
pub fn init_game(server_info: &[u8]) -> [String; 2] {
    [
        "------------------------------------------------------------\n".to_owned(),
        format!("InitGame: {}\n", text(server_info)),
    ]
}
/// `G_ShutdownGame`'s line.
pub fn shutdown_game() -> String {
    "ShutdownGame:\n------------------------------------------------------------\n".to_owned()
}
/// `ClientConnect`'s line.
pub fn client_connect(who: Who) -> String {
    format!("ClientConnect: {}\n", who.head())
}
/// `ClientBegin`'s line.
pub fn client_begin(client: usize) -> String {
    format!("ClientBegin: {client}\n")
}
/// `SetTeam`'s line.
pub fn change_team(who: Who, from: i32, to: i32) -> String {
    format!(
        "ChangeTeam: {} {} -> {}\n",
        who.head(),
        team_name(from),
        team_name(to)
    )
}
/// `ClientUserinfoChanged`'s line for a new name.
pub fn client_rename(who: Who, new_name: &[u8]) -> String {
    format!("ClientRename: {} -> \"{}^7\"\n", who.head(), text(new_name))
}
/// `ClientUserinfoChanged`'s line with `g_logClientInfo`: the player's new string, or
/// that nothing changed.
pub fn userinfo_changed(client: usize, client_info: Option<&[u8]>) -> String {
    match client_info {
        Some(info) => format!("ClientUserinfoChanged: {client} {}\n", text(info)),
        None => format!("ClientUserinfoChanged: {client} <no change>\n"),
    }
}
/// `ClientDisconnect`'s line.
pub fn client_disconnect(who: Who) -> String {
    format!("ClientDisconnect: {}\n", who.head())
}
/// `G_Say`'s line: `say` for everyone, `sayteam` for a team.
pub fn say(team: bool, name: &[u8], message: &[u8]) -> String {
    format!(
        "{}: {}: {}\n",
        if team { "sayteam" } else { "say" },
        text(name),
        text(message)
    )
}
/// `Cmd_Tell_f`'s and `Cmd_GameCommand_f`'s line.
pub fn tell(from: &[u8], to: &[u8], message: &[u8]) -> String {
    format!("tell: {} to {}: {}\n", text(from), text(to), text(message))
}
/// `player_die`'s line: the killer (a client, or `<world>` for anything else) and the
/// victim by number, the means by number and name.
pub fn kill(
    attacker: Option<(i32, &[u8])>,
    victim: usize,
    victim_name: &[u8],
    means: u32,
) -> String {
    let (killer, killer_name) = match attacker {
        Some((number, name)) if (0..MAX_CLIENTS).contains(&number) => (number, text(name)),
        _ => (WORLD, "<world>".to_owned()),
    };
    let obit = match means {
        means if (means as usize) < MEANS.len() => MEANS[means as usize],
        means if means < MEANS_COUNT => "(null)",
        _ => "<bad obituary>",
    };
    format!(
        "Kill: {killer} {victim} {means}: {killer_name} killed {} by {obit}\n",
        text(victim_name)
    )
}
/// `LogExit`'s first line.
pub fn exit(reason: &str) -> String {
    format!("Exit: {reason}\n")
}
/// `LogExit`'s team scores (team games only).
pub fn team_scores(red: i32, blue: i32) -> String {
    format!("red:{red}  blue:{blue}\n")
}
/// `LogExit`'s line for a player in the game, its team named in a team game.
pub fn score(
    team: Option<i32>,
    score: i32,
    ping: i32,
    guid: &[u8],
    client: usize,
    name: &[u8],
) -> String {
    let body = format!(
        "score: {score}  ping: {}  client: [{}] {client} \"{}^7\"\n",
        ping.min(999),
        text(guid),
        text(name)
    );
    match team {
        Some(team) => format!("({}) {body}", team_name(team)),
        None => body,
    }
}
