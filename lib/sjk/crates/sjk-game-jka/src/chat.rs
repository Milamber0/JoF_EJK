//! Chat: `say`, `say_team`, `tell` and `gc`, and the locations a team message names.
//!
//! [`chat_command`] is `Cmd_Say_f`, `Cmd_SayTeam_f`, `Cmd_Tell_f` and `Cmd_GameCommand_f`
//! (`codemp/game/g_cmds.c:1652-1830`) with `G_Say` and `G_SayTo` (`:1523-1645`): who
//! is told, with which of the four commands a client's cgame reads (`chat`, `tchat`,
//! `lchat`, `ltchat`), how each mode dresses the speaker's name, and the location a
//! teammate's message carries. [`spawn_location`] is `SP_target_location`
//! (`g_target.c:585`) and [`location_message`] `Team_GetLocationMsg` (`g_team.c:1000`).
//!
//! The rules read a description of the clients and write nothing; what they say comes
//! back addressed, in the reference's order.

use crate::client_view::{ClientView, Connection, client_number_from_string};
use crate::match_end::{GT_SIEGE, GT_TEAM, TEAM_SPECTATOR};

/// `MAX_SAY_TEXT` (`g_local.h`): a message is cut to one byte less than this.
pub const MAX_SAY_TEXT: usize = 150;
/// `Q_COLOR_ESCAPE`.
const COLOUR: u8 = b'^';
/// `EC`, the escape the name decorations are wrapped in, which the cgame strips.
const EC: u8 = 0x19;
/// The `name` and `location` buffers of `G_Say`, 64 bytes with their terminator.
const SHORT_BUFFER: usize = 63;
/// `MAX_STRING_CHARS`, which `ConcatArgs` stays below.
const MAX_STRING_CHARS: usize = 1024;
/// `gc_orders` (`g_cmds.c:1794`): what `gc <player> <order>` says.
pub const ORDERS: [&str; 7] = [
    "hold your position",
    "hold this position",
    "come here",
    "cover me",
    "guard location",
    "search and destroy",
    "report",
];

/// A `target_location`: a name for the part of the map around a point.
#[derive(Clone, Debug, PartialEq)]
pub struct Location {
    /// Where it stands.
    pub origin: [f32; 3],
    /// What it is called.
    pub message: Vec<u8>,
    /// The colour its name is drawn in, 0 for none; clamped to 0..7.
    pub count: i32,
}

/// `SP_target_location`: a location, or `None` for one with a `targetname` (which the
/// reference turns into a `target_position`, a place other entities aim at) or with no
/// message (which it frees).
///
/// The reference keeps at most `MAX_LOCATIONS` (64), because each is also a configstring
/// its team overlay indexes. Chat names a location by its text, so this keeps every one;
/// the 64 limit belongs to the legacy configstring range and is applied there.
pub fn spawn_location(entity: &sjk_entity::Entity) -> Option<Location> {
    if entity.classname() != Some("target_location")
        || entity
            .get("targetname")
            .is_some_and(|name| !name.is_empty())
    {
        return None;
    }
    let message = entity.get("message")?;
    Some(Location {
        origin: entity.vector("origin").ok().flatten().unwrap_or_default(),
        message: message.as_bytes().to_vec(),
        count: crate::userinfo::atoi(entity.get("count").unwrap_or_default().as_bytes())
            .clamp(0, 7),
    })
}

/// `Team_GetLocationMsg`: the nearest location `in_pvs` says can be seen from `origin`,
/// its name coloured when it has a colour — or `None` when none can be. Ties go to the
/// later location, as `len > bestlen` lets them.
pub fn location_message(
    locations: &[Location],
    origin: [f32; 3],
    in_pvs: impl Fn([f32; 3]) -> bool,
) -> Option<Vec<u8>> {
    let best = &locations[nearest_location(locations, origin, in_pvs)?];
    let mut message = if best.count != 0 {
        [&[COLOUR, b'0' + best.count as u8][..], &best.message, b"^7"].concat()
    } else {
        best.message.clone()
    };
    message.truncate(SHORT_BUFFER);
    Some(message)
}

/// `Team_GetLocation` (`g_team.c:959-990`): which of `locations` is nearest to `origin`
/// among those `in_pvs` says can be seen, the later one on a tie.
pub fn nearest_location(
    locations: &[Location],
    origin: [f32; 3],
    in_pvs: impl Fn([f32; 3]) -> bool,
) -> Option<usize> {
    let mut best = None;
    let mut best_length = 3.0 * 8192.0 * 8192.0_f32;
    for (index, location) in locations.iter().enumerate() {
        let length: f32 = (0..3)
            .map(|axis| {
                (origin[axis] - location.origin[axis]) * (origin[axis] - location.origin[axis])
            })
            .sum();
        if length > best_length || !in_pvs(location.origin) {
            continue;
        }
        best_length = length;
        best = Some(index);
    }
    best
}

/// A location's number in `CS_LOCATIONS` (`cs_index`), which the team overlay sends; 0
/// for none, or one past the legacy range.
pub fn location_number(index: Option<usize>) -> u32 {
    index
        .filter(|index| *index < LEGACY_LOCATIONS - 1)
        .map_or(0, |index| index as u32 + 1)
}

/// `MAX_LOCATIONS`: the `CS_LOCATIONS` range a legacy client's team overlay indexes.
pub const LEGACY_LOCATIONS: usize = 64;

/// `G_LinkLocations` (`g_spawn.c:1549`): each location after `CS_LOCATIONS`, numbered
/// from one (`CS_LOCATIONS` itself is "unknown", which
/// [`crate::registries::init_game_strings`] sets) — as far as the legacy range reaches.
/// The reference would write a 64th into `CS_PARTICLES`; this stops at 63. Chat is not
/// bound by the range at all.
pub fn link_locations(locations: &[Location], set: &mut impl FnMut(usize, &[u8])) {
    let cs_locations = crate::registries::CS_LOCATIONS;
    for (number, location) in locations.iter().take(LEGACY_LOCATIONS - 1).enumerate() {
        set(cs_locations + number + 1, &location.message);
    }
}

/// `ConcatArgs(start)`: the arguments from `start` on, joined by single spaces, stopping
/// before one that would not fit below `MAX_STRING_CHARS`.
pub fn concat_args(arguments: &[&[u8]], start: usize) -> Vec<u8> {
    let mut line = Vec::new();
    let count = arguments.len();
    for (index, argument) in arguments.iter().enumerate().skip(start) {
        if line.len() + argument.len() >= MAX_STRING_CHARS - 1 {
            break;
        }
        line.extend_from_slice(argument);
        if index != count - 1 {
            line.push(b' ');
        }
    }
    line
}

/// How a message is addressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    All,
    Team,
    Tell,
}

/// What a chat command said: server commands, each for one client, in the order they
/// are sent; the game log's lines (`G_LogPrintf`); and the echo of a message to
/// everyone a dedicated server's console shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Spoken {
    /// `(client, command)` pairs.
    pub told: Vec<(usize, Vec<u8>)>,
    /// The game log's lines, without their line ends.
    pub log: Vec<Vec<u8>>,
    /// What the server prints besides.
    pub console: Vec<Vec<u8>>,
}

/// The game state a message is read against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChatSettings {
    /// `level.gametype`.
    pub gametype: i32,
    /// `level.time`, which a siege player's `tempSpectate` is compared with.
    pub now: i32,
}

/// A chat command from `speaker`: `arguments` is the whole tokenized command, its name
/// first. `location` is the speaker's [`location_message`], which a team message and a
/// team `tell` carry. `None` when the command is not a chat command.
pub fn chat_command(
    settings: &ChatSettings,
    clients: &[ClientView],
    speaker: usize,
    arguments: &[&[u8]],
    location: Option<&[u8]>,
) -> Option<Spoken> {
    let command = *arguments.first()?;
    let is = |name: &str| command.eq_ignore_ascii_case(name.as_bytes());
    if !(is("say") || is("say_team") || is("tell") || is("gc")) {
        return None;
    }
    let mut spoken = Spoken::default();
    let Some(me) = clients.iter().find(|view| view.client == speaker) else {
        return Some(spoken);
    };
    let chat = Chat {
        settings,
        clients,
        me,
        location,
    };
    if is("say") || is("say_team") {
        if arguments.len() < 2 {
            return Some(spoken);
        }
        let mode = if is("say_team") && settings.gametype >= GT_TEAM {
            Mode::Team
        } else {
            Mode::All
        };
        chat.say(None, mode, &cut(concat_args(arguments, 1)), &mut spoken);
    } else if is("tell") {
        if arguments.len() < 3 {
            spoken.told.push((
                speaker,
                b"print \"Usage: tell <player id> <message>\n\"".to_vec(),
            ));
            return Some(spoken);
        }
        let Some(target) = chat.find(arguments[1], &mut spoken) else {
            return Some(spoken);
        };
        chat.tell(target, &cut(concat_args(arguments, 2)), &mut spoken);
    } else {
        if arguments.len() != 3 {
            spoken.told.push((
                speaker,
                format!(
                    "print \"Usage: gc <player id> <order 0-{}>\n\"",
                    ORDERS.len() - 1
                )
                .into_bytes(),
            ));
            return Some(spoken);
        }
        // `unsigned int order = atoi(...)`: a negative order is a huge one, printed back
        // as the signed number it was.
        let order = crate::userinfo::atoi(arguments[2]);
        let Some(text) = usize::try_from(order)
            .ok()
            .and_then(|order| ORDERS.get(order))
        else {
            spoken.told.push((
                speaker,
                format!("print \"Bad order: {order}\n\"").into_bytes(),
            ));
            return Some(spoken);
        };
        let Some(target) = chat.find(arguments[1], &mut spoken) else {
            return Some(spoken);
        };
        chat.tell(target, text.as_bytes(), &mut spoken);
    }
    Some(spoken)
}

/// `Cmd_Say_f`'s cut: a message of `MAX_SAY_TEXT` bytes or more loses its end.
fn cut(mut text: Vec<u8>) -> Vec<u8> {
    text.truncate(MAX_SAY_TEXT - 1);
    text
}

/// One message being sent: who says it and to whom it could go.
struct Chat<'a> {
    settings: &'a ChatSettings,
    clients: &'a [ClientView],
    me: &'a ClientView,
    location: Option<&'a [u8]>,
}

impl Chat<'_> {
    /// `ClientNumberFromString` without connecting clients, telling the speaker when
    /// nobody answers to the name.
    fn find(&self, name: &[u8], spoken: &mut Spoken) -> Option<&ClientView> {
        let found = client_number_from_string(self.clients, name, false);
        if found.is_none() {
            spoken.told.push((
                self.me.client,
                [
                    b"print \"User ".as_slice(),
                    name,
                    b" is not on the server\n\"",
                ]
                .concat(),
            ));
        }
        found
    }

    /// The tail of `Cmd_Tell_f` and `Cmd_GameCommand_f`: logged, told to the target, and
    /// echoed to the speaker unless it told itself or is a bot.
    fn tell(&self, target: &ClientView, text: &[u8], spoken: &mut Spoken) {
        spoken.log.push(
            [
                b"tell: ".as_slice(),
                &self.me.name,
                b" to ",
                &target.name,
                b": ",
                text,
            ]
            .concat(),
        );
        self.say(Some(target), Mode::Tell, text, spoken);
        if target.client != self.me.client && !self.me.bot {
            self.say(Some(self.me), Mode::Tell, text, spoken);
        }
    }

    /// `G_Say`.
    fn say(&self, target: Option<&ClientView>, mode: Mode, text: &[u8], spoken: &mut Spoken) {
        // `Q_strncpyz` into `MAX_SAY_TEXT`, then line breaks become spaces.
        let text: Vec<u8> = text
            .iter()
            .take(MAX_SAY_TEXT - 1)
            .map(|&byte| {
                if byte == b'\n' || byte == b'\r' {
                    b' '
                } else {
                    byte
                }
            })
            .collect();
        let name = &self.me.name;
        let (mut label, colour, location) = match mode {
            Mode::All => {
                spoken
                    .log
                    .push([b"say: ".as_slice(), name, b": ", &text].concat());
                ([name.as_slice(), b"^7", &[EC], b": "].concat(), b'2', None)
            }
            Mode::Team => {
                spoken
                    .log
                    .push([b"sayteam: ".as_slice(), name, b": ", &text].concat());
                (
                    [&[EC, b'('][..], name, b"^7", &[EC, b')', EC], b": "].concat(),
                    b'5',
                    self.location,
                )
            }
            Mode::Tell => {
                let teammate = target.is_some_and(|target| {
                    self.settings.gametype >= GT_TEAM && target.team == self.me.team
                });
                (
                    [&[EC, b'['][..], name, b"^7", &[EC, b']', EC], b": "].concat(),
                    b'6',
                    self.location.filter(|_| teammate),
                )
            }
        };
        label.truncate(SHORT_BUFFER);
        if let Some(target) = target {
            self.say_to(target, mode, colour, &label, &text, location, spoken);
            return;
        }
        // "echo the text to the console"
        spoken.console.push([label.as_slice(), &text].concat());
        for other in self.clients {
            self.say_to(other, mode, colour, &label, &text, location, spoken);
        }
    }

    /// `G_SayTo`.
    #[allow(clippy::too_many_arguments)]
    fn say_to(
        &self,
        other: &ClientView,
        mode: Mode,
        colour: u8,
        label: &[u8],
        text: &[u8],
        location: Option<&[u8]>,
        spoken: &mut Spoken,
    ) {
        if other.connection != Connection::Connected {
            return;
        }
        // `OnSameTeam` for two players: a team message only exists from `GT_TEAM` up,
        // where it is the session team that decides.
        if mode == Mode::Team && other.team != self.me.team {
            return;
        }
        let now = self.settings.now;
        let me_watching = self.me.temp_spectate >= now || self.me.team == TEAM_SPECTATOR;
        let other_playing = other.team != TEAM_SPECTATOR && other.temp_spectate < now;
        if self.settings.gametype == GT_SIEGE && me_watching && other_playing {
            // "siege temp spectators should not communicate to ingame players"
            return;
        }
        let number = self.me.client.to_string();
        let command = match location {
            Some(location) => {
                let name = if mode == Mode::Team {
                    b"ltchat".as_slice()
                } else {
                    b"lchat"
                };
                [
                    name,
                    b" \"",
                    label,
                    b"\" \"",
                    location,
                    b"\" \"",
                    &[colour],
                    b"\" \"",
                    text,
                    b"\" ",
                    number.as_bytes(),
                ]
                .concat()
            }
            None => {
                let name = if mode == Mode::Team {
                    b"tchat".as_slice()
                } else {
                    b"chat"
                };
                [
                    name,
                    b" \"",
                    label,
                    &[COLOUR, colour],
                    text,
                    b"\" ",
                    number.as_bytes(),
                ]
                .concat()
            }
        };
        spoken.told.push((other.client, command));
    }
}
