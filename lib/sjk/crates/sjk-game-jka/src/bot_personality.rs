//! A bot's personality (`ai_util.c:BotUtilizePersonality`): the `.jkb` file its
//! definition names — its skills, whether it chats and what, how it likes the weapons,
//! whom it is fond of, and its Force configuration.
//!
//! The reference reads the file with its own text scanners (`GetValueGroup`,
//! `GetPairedValue`), whose quirks decide what a file means, so they are ported byte for
//! byte:
//! - A group is found only where its name starts a line and the character two past the
//!   name is `{`, which is what a Windows line end (`\r\n`) gives. A file with Unix line
//!   ends therefore has no groups.
//! - A value runs to the end of its line, a `\r` included.
//! - `//` comments in a group are overwritten with slashes as keys are looked up.
//!
//! The reference's buffers are zero-filled, so a byte past the file reads as zero here.

use crate::userinfo::atoi;

/// The reference reads files shorter than this (`131072`).
pub const MAX_PERSONALITY_BYTES: usize = 131_072;
/// `MAX_CHAT_BUFFER_SIZE`.
pub const MAX_CHAT_BUFFER_SIZE: usize = 8192;
/// `MAX_LOVED_ONES`.
pub const MAX_LOVED_ONES: usize = 4;
/// `MAX_FORCE_INFO_SIZE`.
const MAX_FORCE_INFO_SIZE: usize = 2048;
/// `MAX_ATTACHMENT_NAME`.
const MAX_ATTACHMENT_NAME: usize = 64;
/// `WP_NUM_WEAPONS`.
pub const WEAPONS: usize = 19;

/// `botskills_t`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BotSkills {
    /// Milliseconds to react, before the skill divides it.
    pub reflex: i32,
    /// Degrees the aim may be off, before the skill divides it.
    pub accuracy: f32,
    pub turnspeed: f32,
    pub turnspeed_combat: f32,
    pub maxturn: f32,
    pub perfectaim: i32,
}

/// Someone a bot is fond of (`botattachment_t`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BotAttachment {
    pub name: Vec<u8>,
    pub level: i32,
}

/// What a bot's personality gives it (the parts of `bot_state_t` it fills).
#[derive(Clone, Debug, PartialEq)]
pub struct BotPersonality {
    pub skills: BotSkills,
    /// `canChat`.
    pub can_chat: i32,
    pub chat_frequency: i32,
    /// `loved_death_thresh` (`hatelevel`).
    pub hate_level: i32,
    /// `isCamper`.
    pub camper: i32,
    pub saber_specialist: i32,
    /// The Force configuration `WP_InitForcePowers` takes in place of the userinfo's.
    pub force_info: Vec<u8>,
    /// `botWeaponWeights`, by weapon.
    pub weapon_weights: [f32; WEAPONS],
    /// `loved`, `lovednum` of them.
    pub loved: Vec<BotAttachment>,
    /// `gBotChatBuffer`: the chat groups' text, when the bot chats.
    pub chat: Vec<u8>,
}

impl BotPersonality {
    /// A bot before its personality (`BotAISetupClient`: the state cleared, the default
    /// weapon weights set).
    pub fn cleared() -> Self {
        let mut weapon_weights = [0.0; WEAPONS];
        for (weapon, weight) in [
            (1, 1.0),
            (3, 10.0),
            (4, 11.0),
            (5, 12.0),
            (6, 13.0),
            (7, 14.0),
            (8, 15.0),
            (9, 16.0),
            (10, 17.0),
            (11, 18.0),
            (12, 14.0),
            (13, 0.0),
            (14, 0.0),
            (2, 1.0),
        ] {
            weapon_weights[weapon] = weight;
        }
        Self {
            skills: BotSkills::default(),
            can_chat: 0,
            chat_frequency: 0,
            hate_level: 0,
            camper: 0,
            saber_specialist: 0,
            force_info: Vec::new(),
            weapon_weights,
            loved: Vec::new(),
            chat: Vec::new(),
        }
    }
}

/// A NUL-terminated view: bytes past the end read as zero.
fn at(buf: &[u8], index: isize) -> u8 {
    usize::try_from(index)
        .ok()
        .and_then(|index| buf.get(index))
        .copied()
        .unwrap_or(0)
}

/// `strstr` from `from`, stopping at the first zero byte.
fn find(buf: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    let end = buf.iter().position(|&byte| byte == 0).unwrap_or(buf.len());
    if needle.is_empty() || from > end {
        return None;
    }
    buf[from..end]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|found| from + found)
}

/// `GetValueGroup`: the body of the group named `group`, between its braces.
pub fn value_group(buf: &[u8], group: &[u8]) -> Option<Vec<u8>> {
    let mut place = find(buf, 0, group)?;
    let mut start = (place + group.len() + 1) as isize;
    let mut letter = place as isize - 1;
    while at(buf, start + 1) != b'{' || at(buf, letter) != b'\n' {
        let second = find(buf, place + 1, group)?;
        start += (second - place) as isize;
        letter += (second - place) as isize;
        place = second;
    }
    while at(buf, start) != b'{' {
        start += 1;
    }
    start += 1;
    let (mut out, mut depth) = (Vec::new(), 0);
    while (at(buf, start) != b'}' || depth != 0) && (start as usize) < buf.len() {
        match at(buf, start) {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        out.push(at(buf, start));
        start += 1;
    }
    Some(out)
}

/// `GetPairedValue`: the value after `key` in `buf` (a group's body). Its `//` comments
/// are overwritten with slashes first, as the reference writes into its buffer.
pub fn paired_value(buf: &mut [u8], key: &[u8]) -> Option<Vec<u8>> {
    let mut i = 0;
    while i < buf.len() && buf[i] != 0 {
        if buf[i] == b'/' && at(buf, i as isize + 1) == b'/' {
            while i < buf.len() && buf[i] != b'\n' {
                buf[i] = b'/';
                i += 1;
            }
        }
        i += 1;
    }
    let buf = &*buf;
    let mut place = find(buf, 0, key)?;
    let mut start = (place + key.len()) as isize;
    let mut letter = place as isize - 1;
    let blank = |byte: u8| matches!(byte, 0 | b'\t' | b' ' | b'\n');
    loop {
        if (letter == 0 || blank(at(buf, letter))) && blank(at(buf, start)) {
            break;
        }
        let second = find(buf, place + 1, key)?;
        start += (second - place) as isize;
        letter += (second - place) as isize;
        place = second;
    }
    if at(buf, start) == 0 {
        return None;
    }
    while matches!(at(buf, start), b' ' | b'\t' | b'\n') {
        start += 1;
    }
    let mut out = Vec::new();
    while !matches!(at(buf, start), 0 | b'\n') {
        out.push(at(buf, start));
        start += 1;
    }
    Some(out)
}

/// `ParseEmotionalAttachments`: `name level` a line, at most [`MAX_LOVED_ONES`].
fn emotional_attachments(buf: &[u8]) -> Vec<BotAttachment> {
    let (mut loved, mut i) = (Vec::new(), 0_isize);
    while at(buf, i) != 0 && at(buf, i) != b'}' {
        while matches!(at(buf, i), b' ' | b'{' | b'\t' | 13 | b'\n') {
            i += 1;
        }
        if at(buf, i) == 0 || at(buf, i) == b'}' {
            break;
        }
        let mut name = Vec::new();
        while !matches!(at(buf, i), b'{' | b'\t' | 13 | b'\n') && (i as usize) < buf.len() {
            name.push(at(buf, i));
            i += 1;
        }
        name.truncate(MAX_ATTACHMENT_NAME - 1);
        while matches!(at(buf, i), b' ' | b'{' | b'\t' | 13 | b'\n') {
            i += 1;
        }
        let mut level = Vec::new();
        while !matches!(at(buf, i), b'{' | b'\t' | 13 | b'\n') && (i as usize) < buf.len() {
            level.push(at(buf, i));
            i += 1;
        }
        loved.push(BotAttachment {
            name,
            level: atoi(&level),
        });
        if loved.len() >= MAX_LOVED_ONES {
            return loved;
        }
        i += 1;
    }
    loved
}

/// `ReadChatGroups`: everything from the line after `BEGIN_CHAT_GROUPS` on, or `None`
/// (the bot then does not chat) where there is none or it is too long.
fn chat_groups(buf: &[u8], print: &mut dyn FnMut(&[u8])) -> Option<Vec<u8>> {
    let begin = find(buf, 0, b"BEGIN_CHAT_GROUPS")?;
    let end = buf.iter().position(|&byte| byte == 0).unwrap_or(buf.len());
    if end - begin >= MAX_CHAT_BUFFER_SIZE {
        print(b"^1Error: Personality chat section exceeds max size\n");
        return None;
    }
    let mut place = begin + 1;
    while at(buf, place as isize) != b'\n' {
        place += 1;
    }
    Some(buf[place.min(end)..end].to_vec())
}

/// `BotUtilizePersonality` over `file` (`None` where it cannot be opened), after
/// `BotAISetupClient`'s defaults; `duel` is a duel or power duel, where the saber weighs
/// 13 whatever the file says. The reference's complaints go to `print`.
pub fn utilize_personality(
    file: Option<&[u8]>,
    duel: bool,
    print: &mut dyn FnMut(&[u8]),
) -> BotPersonality {
    let mut bot = BotPersonality::cleared();
    let finish = |mut bot: BotPersonality| {
        if duel {
            bot.weapon_weights[3] = 13.0;
        }
        bot
    };
    let Some(file) = file else {
        print(b"^1Error: Specified personality not found\n");
        return finish(bot);
    };
    if file.len() >= MAX_PERSONALITY_BYTES {
        print(b"^1Personality file exceeds maximum length\n");
        return finish(bot);
    }
    let buf = file;
    let general = value_group(buf, b"GeneralBotInfo");
    if general.is_none() {
        print(b"^1Personality file contains no GeneralBotInfo group\n");
    }
    let mut group = general.clone().unwrap_or_default();
    let mut value = |key: &[u8]| general.as_ref().and(paired_value(&mut group, key));
    let number = |value: Option<Vec<u8>>, default: i32| value.map_or(default, |value| atoi(&value));
    let float = |value: Option<Vec<u8>>, default: f32| {
        value.map_or(default, |value| crate::text_parse::atof(&value))
    };
    bot.skills.reflex = number(value(b"reflex"), 100);
    bot.skills.accuracy = float(value(b"accuracy"), 10.0);
    bot.skills.turnspeed = float(value(b"turnspeed"), 0.01);
    bot.skills.turnspeed_combat = float(value(b"turnspeed_combat"), 0.05);
    bot.skills.maxturn = float(value(b"maxturn"), 360.0);
    bot.skills.perfectaim = number(value(b"perfectaim"), 0);
    bot.can_chat = number(value(b"chatability"), 0);
    bot.chat_frequency = number(value(b"chatfrequency"), 5);
    bot.hate_level = number(value(b"hatelevel"), 3);
    bot.camper = number(value(b"camper"), 0);
    bot.saber_specialist = number(value(b"saberspecialist"), 0);
    let mut force =
        value(b"forceinfo").unwrap_or_else(|| crate::force_config::DEFAULT_FORCE_POWERS.to_vec());
    force.truncate(MAX_FORCE_INFO_SIZE - 1);
    bot.force_info = force;
    if bot.can_chat != 0 {
        match chat_groups(buf, print) {
            Some(chat) => bot.chat = chat,
            None => bot.can_chat = 0,
        }
    }
    if let Some(mut weights) = value_group(buf, b"BotWeaponWeights") {
        for (key, weapon) in [
            (&b"WP_STUN_BATON"[..], 1),
            (b"WP_SABER", 3),
            (b"WP_BRYAR_PISTOL", 4),
            (b"WP_BLASTER", 5),
            (b"WP_DISRUPTOR", 6),
            (b"WP_BOWCASTER", 7),
            (b"WP_REPEATER", 8),
            (b"WP_DEMP2", 9),
            (b"WP_FLECHETTE", 10),
            (b"WP_ROCKET_LAUNCHER", 11),
            (b"WP_THERMAL", 12),
            (b"WP_TRIP_MINE", 13),
            (b"WP_DET_PACK", 14),
        ] {
            if let Some(found) = paired_value(&mut weights, key) {
                bot.weapon_weights[weapon] = atoi(&found) as f32;
                // The stun baton's weight is the fists' too.
                if weapon == 1 {
                    bot.weapon_weights[2] = bot.weapon_weights[1];
                }
            }
        }
    }
    if let Some(attachments) = value_group(buf, b"EmotionalAttachments") {
        bot.loved = emotional_attachments(&attachments);
    }
    finish(bot)
}
