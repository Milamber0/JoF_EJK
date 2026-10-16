//! `showplayerid`: the console list of client numbers JoF EJK's `showPlayerId` prints.
//!
//! One line per connected player: the client number, a coloured team letter and the name,
//! with ` (bot)` after bots, then a count. Commands such as `whois`, `ignore` or a server's
//! private message take that number.

use sjk_protocol::GameState;

/// The console command name.
pub(crate) const COMMAND: &str = "showplayerid";
/// Its console help line.
pub(crate) const HELP: &str =
    "List each player's client number, team and name, as JoF EJK's showPlayerId";

/// `CS_PLAYERS`: the first player configstring.
const PLAYERS: usize = 1_131;
/// `MAX_CLIENTS`.
const MAX_CLIENTS: usize = 32;

/// The console lines for every player in `game_state`.
pub(crate) fn lines(game_state: &GameState) -> Vec<String> {
    let mut lines = Vec::new();
    for client in 0..MAX_CLIENTS {
        let Some(info) = game_state.config_string(PLAYERS + client) else {
            continue;
        };
        let Some(player) = Player::parse(info) else {
            continue;
        };
        let bot = if player.bot { " (bot)" } else { "" };
        lines.push(format!(
            "{client:2} {}   ^7{}^7{bot}",
            team_letter(player.team),
            player.name
        ));
    }
    let count = lines.len();
    lines.push(format!("Listed {count:2} clients"));
    lines
}

/// JoF EJK's coloured team letters: free, red, blue, spectator.
fn team_letter(team: u8) -> &'static str {
    match team {
        0 => "^3F",
        1 => "^1R",
        2 => "^4B",
        _ => "^3S",
    }
}

struct Player {
    name: String,
    team: u8,
    bot: bool,
}

impl Player {
    /// Read a player configstring; an empty one is a free slot.
    fn parse(info: &[u8]) -> Option<Self> {
        let body = info.strip_prefix(b"\\").unwrap_or(info);
        let mut name = None;
        let mut team = 3;
        let mut bot = false;
        let mut fields = body.split(|byte| *byte == b'\\');
        while let (Some(key), Some(value)) = (fields.next(), fields.next()) {
            match key {
                b"n" if !value.is_empty() => name = Some(value),
                b"t" => {
                    team = std::str::from_utf8(value)
                        .ok()
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(3);
                }
                // Only bots carry a skill level in their player configstring.
                b"skill" => bot = true,
                _ => {}
            }
        }
        Some(Self {
            name: sjk_client::decode_legacy(name?).into_owned(),
            team,
            bot,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(players: &[(usize, &str)]) -> GameState {
        let mut game = GameState::empty_local(0);
        for (client, info) in players {
            game.replace_config_string(PLAYERS + client, info.as_bytes().to_vec())
                .unwrap();
        }
        game
    }

    #[test]
    fn players_are_listed_by_client_number_with_team_letters() {
        let game = game(&[
            (0, "n\\^1Rosh\\t\\1"),
            (3, "n\\Kyle\\t\\2\\skill\\4"),
            (12, "n\\Watcher\\t\\3"),
        ]);
        assert_eq!(
            lines(&game),
            [
                " 0 ^1R   ^7^1Rosh^7",
                " 3 ^4B   ^7Kyle^7 (bot)",
                "12 ^3S   ^7Watcher^7",
                "Listed  3 clients",
            ]
        );
    }

    #[test]
    fn an_empty_server_lists_no_clients() {
        assert_eq!(lines(&game(&[])), ["Listed  0 clients"]);
    }
}
