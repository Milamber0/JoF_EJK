//! Bots as the game defines them (`g_bot.c`): the definitions read from
//! `botfiles/bots.txt` and `scripts/*.bot` (`G_LoadBots`, `G_ParseInfos`), `addbot`'s
//! arguments (`Svcmd_AddBot_f`) and the userinfo a new bot joins with (`G_AddBot`).
//!
//! Where a bot's slot comes from, how it connects and what it does once in the game are
//! the server's; this is the data and the rules for them.

use crate::text_parse::{TextParser, atof};
use sjk_protocol::{info_set_value, info_value};

/// `MAX_BOTS`: how many definitions the reference keeps.
pub const MAX_BOTS: usize = 1024;
/// `MAX_BOTS_TEXT`: a definitions file this long or longer is not read.
pub const MAX_BOTS_TEXT: usize = 8192;
/// The definitions file read unless `g_botsFile` names another.
pub const BOTS_FILE: &str = "botfiles/bots.txt";
/// `MAX_TOKEN_CHARS`: `Q_strncpyz` of a key into its buffer.
const TOKEN_BYTES: usize = 1024;

/// `G_ParseInfos`: each `{ key value ... }` block of `text` as an info string, at most
/// `room` of them. A value missing at the end of its line is `<NULL>`. The reference's
/// complaints go to `print`.
pub fn parse_infos(text: &[u8], room: usize, print: &mut dyn FnMut(&[u8])) -> Vec<Vec<u8>> {
    let mut parser = TextParser::new(text);
    let mut infos = Vec::new();
    loop {
        let token = parser.parse_ext(true);
        if token.is_empty() {
            break;
        }
        if token != b"{" {
            print(b"Missing { in info file\n");
            break;
        }
        if infos.len() == room {
            print(b"Max infos exceeded\n");
            break;
        }
        let mut info = Vec::new();
        loop {
            let token = parser.parse_ext(true);
            if token.is_empty() {
                print(b"Unexpected end of info file\n");
                break;
            }
            if token == b"}" {
                break;
            }
            let key = token[..token.len().min(TOKEN_BYTES - 1)].to_vec();
            let value = parser.parse_ext(false);
            let value: &[u8] = if value.is_empty() { b"<NULL>" } else { value };
            info = info_set_value(&info, &key, value);
        }
        infos.push(info);
    }
    infos
}

/// `level.bots`: every definition read, in order.
#[derive(Clone, Debug, Default)]
pub struct BotRoster {
    infos: Vec<Vec<u8>>,
}

impl BotRoster {
    /// `G_LoadBotsFromFile`: one file's definitions added, up to [`MAX_BOTS`] in all. A
    /// file of [`MAX_BOTS_TEXT`] bytes or more is refused.
    pub fn load(&mut self, name: &str, text: Option<&[u8]>, print: &mut dyn FnMut(&[u8])) {
        let Some(text) = text else {
            print(format!("^1file not found: {name}\n").as_bytes());
            return;
        };
        if text.len() >= MAX_BOTS_TEXT {
            print(
                format!(
                    "^1file too large: {name} is {}, max allowed is {MAX_BOTS_TEXT}\n",
                    text.len()
                )
                .as_bytes(),
            );
            return;
        }
        let room = MAX_BOTS - self.infos.len();
        let found = parse_infos(text, room, print);
        self.infos.extend(found);
    }

    /// How many definitions there are.
    pub fn len(&self) -> usize {
        self.infos.len()
    }
    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.infos.is_empty()
    }
    /// Every definition, in order.
    pub fn infos(&self) -> impl Iterator<Item = &[u8]> {
        self.infos.iter().map(Vec::as_slice)
    }

    /// `G_GetBotInfoByName`: the first definition whose `name` matches, any case.
    pub fn by_name(&self, name: &[u8]) -> Option<&[u8]> {
        self.infos().find(|info| {
            info_value(info, b"name")
                .unwrap_or_default()
                .eq_ignore_ascii_case(name)
        })
    }
}

/// `addbot`'s arguments (`Svcmd_AddBot_f`).
#[derive(Clone, Debug, PartialEq)]
pub struct AddBot {
    pub name: Vec<u8>,
    /// 1 to 5; 4 when not given.
    pub skill: f32,
    /// Empty for the game's choice.
    pub team: Vec<u8>,
    /// Milliseconds before the bot enters (the spawn queue); 0 enters at once.
    pub delay: i32,
    /// A name to play under instead of the definition's.
    pub altname: Vec<u8>,
}

impl AddBot {
    /// The arguments after the command's name, `None` without a bot's name (the usage).
    pub fn parse(arguments: &[&[u8]]) -> Option<Self> {
        let get = |index: usize| arguments.get(index).copied().unwrap_or_default();
        let name = get(0);
        if name.is_empty() {
            return None;
        }
        let skill = if get(1).is_empty() { 4.0 } else { atof(get(1)) };
        let delay = if get(3).is_empty() {
            0
        } else {
            crate::userinfo::atoi(get(3))
        };
        Some(Self {
            name: name.to_vec(),
            skill,
            team: get(2).to_vec(),
            delay,
            altname: get(4).to_vec(),
        })
    }
}

/// `addbot`'s usage line.
pub const ADDBOT_USAGE: &[u8] =
    b"Usage: Addbot <botname> [skill 1-5] [team] [msec delay] [altname]\n";

/// `G_AddBot`'s userinfo for a bot defined by `botinfo`, of `skill`, on `team` (already
/// chosen), under `altname` if one is given: the definition's values, and the
/// reference's defaults where it has none.
pub fn bot_userinfo(botinfo: &[u8], skill: f32, team: &[u8], altname: &[u8]) -> Vec<u8> {
    let value = |key: &[u8]| info_value(botinfo, key).unwrap_or_default();
    let mut userinfo = Vec::new();
    let mut set = |key: &[u8], value: &[u8]| userinfo = info_set_value(&userinfo, key, value);
    let mut name = value(b"funname");
    if name.is_empty() {
        name = value(b"name");
    }
    if !altname.is_empty() {
        name = altname;
    }
    set(b"name", name);
    set(b"rate", b"25000");
    set(b"snaps", b"20");
    set(b"ip", b"localhost");
    set(b"skill", format!("{skill:.2}").as_bytes());
    let handicap: &[u8] = if (1.0..2.0).contains(&skill) {
        b"50"
    } else if (2.0..3.0).contains(&skill) {
        b"70"
    } else if (3.0..4.0).contains(&skill) {
        b"90"
    } else {
        b"100"
    };
    set(b"handicap", handicap);
    let or = |found: &'_ [u8], default: &'static [u8]| -> Vec<u8> {
        if found.is_empty() {
            default.to_vec()
        } else {
            found.to_vec()
        }
    };
    set(b"model", &or(value(b"model"), b"kyle/default"));
    let mut sex = value(b"sex");
    if sex.is_empty() {
        sex = value(b"gender");
    }
    set(b"sex", &or(sex, b"male"));
    for (key, default) in [
        (&b"color1"[..], &b"4"[..]),
        (b"color2", b"4"),
        (b"saber1", b"Kyle"),
        (b"saber2", b"none"),
        (b"forcepowers", b"5-1-000000000000000000"),
        (b"cg_predictItems", b"1"),
        (b"char_color_red", b"255"),
        (b"char_color_green", b"255"),
        (b"char_color_blue", b"255"),
        (b"teamtask", b"0"),
        (b"personality", b"botfiles/default.jkb"),
    ] {
        set(key, &or(value(key), default));
    }
    set(b"team", team);
    userinfo
}
