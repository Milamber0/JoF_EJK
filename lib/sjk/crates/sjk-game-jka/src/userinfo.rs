//! What the game makes of a player's userinfo: whether it is accepted, the name the
//! player keeps, and the `CS_PLAYERS` string every client is told.
//!
//! Ported from OpenJK `codemp/game/g_client.c` (`G_ValidateUserinfo`,
//! `ClientCleanName`, `ClientUserinfoChanged`) and held against the reference's whole
//! game module (`tools/game-oracle/userinfo.c`). Everything is bytes: a name may hold
//! any byte the cleaning lets through.

use sjk_protocol::{info_pairs, info_value};

/// `MAX_NETNAME`, terminator included: a kept name has at most 35 bytes.
pub const MAX_NETNAME: usize = 36;
/// `MAX_INFO_STRING`, terminator included.
const MAX_INFO_STRING: usize = 1_024;
/// `MAX_QPATH`, terminator included: what a model name is cut to.
const MAX_QPATH: usize = 64;
/// `DEFAULT_SABER`: what an unknown or missing first saber becomes.
const DEFAULT_SABER: &[u8] = b"Kyle";

/// `g_userinfoValidate`'s default: every field rule, size, slashes and control
/// characters; extended ASCII is allowed ("impossible to type, may want to disable").
pub const USERINFO_VALIDATE_DEFAULT: u32 = 25_165_823;

/// `userinfoFields` (`g_client.c:1964-1986`): key, least and most occurrences. A rule's
/// bit in `g_userinfoValidate` is its index here.
const FIELDS: [(&str, u32, u32); 21] = [
    ("cl_guid", 0, 0), // "not allowed, q3fill protection"
    ("cl_punkbuster", 0, 0),
    ("ip", 0, 1), // the engine adds this
    ("name", 1, 1),
    ("rate", 1, 1),
    ("snaps", 1, 1),
    ("model", 1, 1),
    ("forcepowers", 1, 1),
    ("color1", 1, 1),
    ("color2", 1, 1),
    ("handicap", 1, 1),
    ("sex", 0, 1),
    ("cg_predictItems", 1, 1),
    ("saber1", 1, 1),
    ("saber2", 1, 1),
    ("char_color_red", 1, 1),
    ("char_color_green", 1, 1),
    ("char_color_blue", 1, 1),
    ("teamtask", 0, 1),
    ("password", 0, 1),
    ("teamoverlay", 0, 1),
];
/// `userinfoValidateExtra`: the rules after the fields.
const EXTRA_RULES: [&str; 4] = [
    "Size",
    "# of slashes",
    "Extended ascii",
    "Control characters",
];
/// Every rule's name, a bit of `g_userinfoValidate` each in order: the fields' own
/// (`fieldClean`), then the four checks of the whole string.
pub fn validation_rule_names() -> impl Iterator<Item = &'static str> {
    FIELDS.iter().map(|(name, ..)| *name).chain(EXTRA_RULES)
}
const VALIDATE_SIZE: u32 = 1 << FIELDS.len();
const VALIDATE_SLASHES: u32 = 1 << (FIELDS.len() + 1);
const VALIDATE_EXTENDED_ASCII: u32 = 1 << (FIELDS.len() + 2);
const VALIDATE_CONTROL_CHARACTERS: u32 = 1 << (FIELDS.len() + 3);

/// `G_ValidateUserinfo`: `Err` carries the reason, which the game appends to
/// "Failed userinfo validation: " when it drops the client. `rules` is
/// `g_userinfoValidate`.
pub fn validate_userinfo(userinfo: &[u8], rules: u32) -> Result<(), String> {
    let fail = |reason: &str| Err(reason.to_owned());
    if rules & VALIDATE_SIZE != 0 {
        if userinfo.is_empty() {
            return fail("Userinfo too short");
        } else if userinfo.len() >= MAX_INFO_STRING {
            return fail("Userinfo too long");
        }
    }
    if rules & VALIDATE_SLASHES != 0 {
        if userinfo.first() != Some(&b'\\') {
            return fail("Missing leading slash");
        }
        // "no trailing slashes allowed, engine will append ip\\ip:port"
        if userinfo.last() == Some(&b'\\') {
            return fail("Trailing slash");
        }
        if userinfo.iter().filter(|&&byte| byte == b'\\').count() % 2 == 1 {
            return fail("Bad number of slashes");
        }
    }
    if rules & VALIDATE_EXTENDED_ASCII != 0 && userinfo.iter().any(|&byte| byte >= 0x80) {
        return fail("Extended ASCII characters found");
    }
    if rules & VALIDATE_CONTROL_CHARACTERS != 0
        && userinfo.iter().any(|byte| b"\n\r;\"".contains(byte))
    {
        return fail("Invalid characters found");
    }
    let mut counts = [0_u32; FIELDS.len()];
    for (key, _) in info_pairs(userinfo) {
        for (count, (field, ..)) in counts.iter_mut().zip(FIELDS) {
            *count += u32::from(key.eq_ignore_ascii_case(field.as_bytes()));
        }
    }
    for (index, (&count, (field, least, most))) in counts.iter().zip(FIELDS).enumerate() {
        if rules & (1 << index) == 0 {
            continue;
        }
        if least != 0 && count == 0 {
            return Err(format!("{field} field not found"));
        } else if count > most {
            return Err(format!("Too many {field} fields ({count}/{most})"));
        }
    }
    Ok(())
}

/// `ClientCleanName`: leading spaces dropped, at most three spaces and two at-signs in
/// a row, control and unprintable bytes removed, cut to 35 bytes; a name with nothing
/// visible in it becomes "Padawan".
pub fn clean_name(name: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(MAX_NETNAME);
    let (mut visible, mut spaces, mut ats) = (0_i32, 0, 0);
    for &byte in name
        .iter()
        .skip_while(|&&byte| byte == b' ')
        .take_while(|&&byte| byte != 0)
    {
        if out.len() >= MAX_NETNAME - 1 {
            break;
        }
        if byte == b' ' {
            if spaces > 2 {
                continue;
            }
            spaces += 1;
        } else if byte == b'@' {
            ats += 1;
            if ats > 2 {
                // The third in a row takes the two before it along: "@@@" prefixes a
                // string reference, which a name must not be able to spell.
                out.truncate(out.len().saturating_sub(2));
                ats = 0;
                continue;
            }
        } else if byte < 0x20 || matches!(byte, 0x81 | 0x8D | 0x8F | 0x90 | 0x9D | 0xA0 | 0xAD) {
            continue;
        } else if out.last() == Some(&b'^') {
            if byte.is_ascii_digit() {
                // A colour code: the caret counted as visible a moment ago was not.
                visible -= 1;
            } else {
                (spaces, ats) = (0, 0);
                visible += 1;
            }
        } else {
            (spaces, ats) = (0, 0);
            visible += 1;
        }
        out.push(byte);
    }
    if out.is_empty() || visible == 0 {
        return b"Padawan".to_vec();
    }
    out
}

/// Which saber definitions exist. Names are validated against them; movement and
/// combat will read more from them later.
pub trait SaberCatalog {
    /// Whether a player may use the saber called `name` (any case) in multiplayer
    /// (`WP_SaberValidForPlayerInMP`), and if so whether it takes both hands
    /// (`SFL_TWO_HANDED`). `None` for an unknown name and for a campaign-only saber.
    fn player_saber(&self, name: &[u8]) -> Option<bool>;
}

/// No saber definitions at all: every name falls back to the default saber.
pub struct NoSabers;
impl SaberCatalog for NoSabers {
    fn player_saber(&self, _: &[u8]) -> Option<bool> {
        None
    }
}

/// `G_SetSaber` and `WP_SetSaber` for both hands: the names `pers.saber1` and
/// `pers.saber2` end up with. A usable saber keeps the name as the player typed it; an
/// unknown or campaign-only one becomes the default saber; the first saber cannot be
/// removed; a two-handed saber in either hand leaves no second.
pub fn saber_names(
    saber1: &[u8],
    saber2: &[u8],
    catalog: &impl SaberCatalog,
) -> (Vec<u8>, Vec<u8>) {
    let removes =
        |name: &[u8]| name.eq_ignore_ascii_case(b"none") || name.eq_ignore_ascii_case(b"remove");
    // `truncSaberName`: names are cut to a path's length before they are looked up.
    let resolve = |name: &[u8]| {
        let name = &name[..name.len().min(MAX_QPATH - 1)];
        match catalog.player_saber(name) {
            Some(two_handed) => (name.to_vec(), two_handed),
            None => (
                DEFAULT_SABER.to_vec(),
                catalog.player_saber(DEFAULT_SABER).unwrap_or(false),
            ),
        }
    };
    let (first, first_two_handed) = resolve(if removes(saber1) {
        DEFAULT_SABER
    } else {
        saber1
    });
    if removes(saber2) || first_two_handed {
        return (first, b"none".to_vec());
    }
    let (second, second_two_handed) = resolve(saber2);
    (
        first,
        if second_two_handed {
            b"none".to_vec()
        } else {
            second
        },
    )
}

/// `CompareIPs` (`g_client.c`): two addresses are one host if they agree up to a port.
fn same_host(one: &[u8], other: &[u8]) -> bool {
    let host = |address: &'_ [u8]| -> Vec<u8> {
        address
            .iter()
            .copied()
            .take_while(|&byte| byte != b':' && byte != 0)
            .collect()
    };
    host(one) == host(other)
}

/// `ClientConnect`'s address limit: whether more than `limit` (`g_maxConnPerIP`) of the
/// other connected clients share `address`'s host. The game answers such a connect
/// with "Too many connections from the same IP".
pub fn too_many_connections<'a>(
    address: &[u8],
    others: impl Iterator<Item = &'a [u8]>,
    limit: usize,
) -> bool {
    others.filter(|other| same_host(address, other)).count() > limit
}

/// What `ClientUserinfoChanged` knows about a client besides its userinfo.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClientSession<'a> {
    /// `sess.sessionTeam`.
    pub team: i32,
    /// A bot's `skill` key is passed on.
    pub bot: bool,
    /// Duel and power duel: `sess.wins` and `sess.losses`.
    pub duel_record: Option<(i32, i32)>,
    /// Power duel: `sess.duelTeam`.
    pub duel_team: Option<i32>,
    /// Team games: `sess.teamLeader`.
    pub team_leader: Option<i32>,
    /// Siege: the class name and `sess.siegeDesiredTeam`.
    pub siege: Option<(&'a [u8], i32)>,
    /// Siege: the model a class forces instead of the userinfo's (`forcedModel`).
    pub forced_model: Option<&'a [u8]>,
    /// Siege: a class's `maxhealth` (100 for one that names none), which replaces the
    /// handicap (`g_client.c:2275-2282`).
    pub class_max_health: Option<i32>,
}

/// What `ClientUserinfoChanged` takes from an accepted userinfo.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedUserinfo {
    /// `pers.netname`.
    pub name: Vec<u8>,
    /// `pers.maxHealth`, from `handicap`: 1 to 100.
    pub max_health: i32,
    /// `ps.customRGBA`: the character's tint, never too dark to see, alpha 255.
    pub custom_rgba: [u8; 4],
    /// `pers.predictItemPickup`.
    pub predict_item_pickup: bool,
    /// `pers.saber1` and `pers.saber2`.
    pub sabers: (Vec<u8>, Vec<u8>),
    /// The `CS_PLAYERS` string: "a subset of the userinfo keys so other clients can
    /// print scoreboards, display models, and play custom sounds".
    pub client_info: Vec<u8>,
}

/// `atoi`: leading whitespace, a sign, digits; anything else ends it.
pub fn atoi(text: &[u8]) -> i32 {
    let text = &text[text
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(text.len())..];
    let (negative, digits) = match text.first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let value = digits
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .fold(0_i64, |value, byte| {
            (value * 10 + i64::from(byte - b'0')).min(i64::from(u32::MAX))
        });
    (if negative { -value } else { value }).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// The name, health, tint, sabers and client-info string of `ClientUserinfoChanged`
/// for a userinfo that passed [`validate_userinfo`]. Team skins, siege classes' forced
/// models and the rename throttle are not here yet.
pub fn accept_userinfo(
    userinfo: &[u8],
    session: ClientSession<'_>,
    catalog: &impl SaberCatalog,
    restrict_dark_tints: bool,
) -> AcceptedUserinfo {
    let value = |key: &str| info_value(userinfo, key.as_bytes()).unwrap_or_default();
    let name = clean_name(value("name"));
    let model = session.forced_model.unwrap_or(value("model"));
    let model = &model[..model.len().min(MAX_QPATH - 1)];
    let tint = |key: &str| atoi(value(key)).clamp(0, 255) as u8;
    let mut custom_rgba = [
        tint("char_color_red"),
        tint("char_color_green"),
        tint("char_color_blue"),
        255,
    ];
    // "Prevent skins being too dark"
    if restrict_dark_tints
        && custom_rgba[..3]
            .iter()
            .map(|&part| u32::from(part))
            .sum::<u32>()
            < 100
    {
        custom_rgba = [255; 4];
    }
    // "if < 1 or > the class's maximum, 100" (`g_client.c:2284-2286`).
    let max_health = match session.class_max_health {
        Some(maximum) if maximum >= 1 => maximum,
        Some(_) => 100,
        None => atoi(value("handicap")).clamp(1, 100),
    };
    let sabers = saber_names(value("saber1"), value("saber2"), catalog);
    // A colour is passed on as the text the client sent, cut to fifteen bytes.
    let colour = |key: &str| &value(key)[..value(key).len().min(15)];
    let mut info = Vec::with_capacity(160);
    let mut pair = |key: &str, text: &[u8]| {
        info.extend_from_slice(key.as_bytes());
        info.push(b'\\');
        info.extend_from_slice(text);
        info.push(b'\\');
    };
    pair("n", &name);
    pair("t", session.team.to_string().as_bytes());
    pair("model", model);
    pair(
        "ds",
        if value("sex").eq_ignore_ascii_case(b"female") {
            b"f"
        } else {
            b"m"
        },
    );
    pair("st", &sabers.0);
    pair("st2", &sabers.1);
    pair("c1", colour("color1"));
    pair("c2", colour("color2"));
    pair("hc", max_health.to_string().as_bytes());
    if session.bot {
        pair("skill", value("skill"));
    }
    if let Some((wins, losses)) = session.duel_record {
        pair("w", wins.to_string().as_bytes());
        pair("l", losses.to_string().as_bytes());
    }
    if let Some(team) = session.duel_team {
        pair("dt", team.to_string().as_bytes());
    }
    if let Some(leader) = session.team_leader {
        pair("tl", leader.to_string().as_bytes());
    }
    if let Some((class, desired_team)) = session.siege {
        pair("siegeclass", class);
        pair("sdt", desired_team.to_string().as_bytes());
    }
    AcceptedUserinfo {
        name,
        max_health,
        custom_rgba,
        predict_item_pickup: atoi(value("cg_predictItems")) != 0,
        sabers,
        client_info: info,
    }
}

/// `ClientConnect`'s announcement to everyone, a server command: the name, a colour
/// reset, and a string reference each client shows in its own language.
pub fn connect_print(name: &[u8]) -> Vec<u8> {
    [b"print \"", name, b"^7 @@@PLCONNECT\n\""].concat()
}
