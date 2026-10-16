//! Bots' chat (OpenJK `codemp/game/ai_util.c`'s `BotDoChat`, `ai_main.c`'s
//! `BotLovedOneDied`, `BotDeathNotify` and `BotReplyGreetings`): a line picked from a
//! section of the bot's chat groups, names filled in, said after a delay it types for;
//! the chat and hatred the death of a loved one stirs; answers to a greeting.
//!
//! The reference's quirks are kept: the frequency draw happens even for a chat that
//! always goes out; the walk to the chosen line skips the character after each line
//! break it passes; a `%s` or `%a` with nobody to name leaves what the chat buffer held
//! there before.

use crate::bot_personality::value_group;
use crate::bot_senses::{BotSenses, SensingBot, SensingRules, on_same_team, pass_loved_one_check};
use crate::bot_think::BotMind;
use crate::player_death::Rng;

/// `MAX_CHAT_LINE_SIZE`: the longest line said, and the chat buffer's size.
pub const MAX_CHAT_LINE_SIZE: usize = 128;
const GT_DUEL: i32 = 3;
const GT_POWERDUEL: i32 = 4;
const GT_TEAM: i32 = 6;

/// What a chat reads of the game: the level's time, whether the language is English
/// (`se_language` 0), and the clients' names.
pub struct ChatGame<'a> {
    pub level_time: i32,
    pub english: bool,
    pub senses: &'a dyn BotSenses,
}

fn at(buf: &[u8], index: usize) -> u8 {
    buf.get(index).copied().unwrap_or(0)
}

/// `BotDoChat`: whether a line of `section` was chosen to be said — the bot chats, has
/// nothing waiting, speaks English, passes its frequency (unless `always`) and has the
/// section. The line goes to `currentChat` with `%s` and `%a` the chat's objects' names;
/// it is said after 45 ms a character and 1.3 to 1.5 seconds more (`doChat` 2 for a
/// greeting).
pub fn do_chat(
    mind: &mut BotMind,
    section: &[u8],
    always: bool,
    game: &ChatGame,
    rng: &mut Rng,
) -> bool {
    if mind.can_chat == 0 || mind.do_chat != 0 || !game.english {
        return false;
    }
    if rng.irand(1, 10) > mind.chat_frequency && !always {
        return false;
    }
    mind.chat_team = 0;
    let Some(group) = value_group(&mind.chat_buffer, section) else {
        return false;
    };
    // From the third character on, without carriage returns and tabs.
    let group: Vec<u8> = group
        .iter()
        .skip(2)
        .copied()
        .take_while(|&byte| byte != 0)
        .filter(|&byte| byte != 13 && byte != 9)
        .collect();
    let lines = group.iter().filter(|&&byte| byte == b'\n').count() as i32;
    if lines == 0 {
        return false;
    }
    let wanted = rng.irand(0, lines + 1).clamp(1, lines);
    let (mut line, mut place) = (1, 0);
    while line != wanted {
        if at(&group, place) != 0 && at(&group, place) == b'\n' {
            place += 1;
            line += 1;
        }
        if line == wanted {
            break;
        }
        place += 1;
    }
    let chosen: Vec<u8> = group
        .get(place..)
        .unwrap_or_default()
        .iter()
        .copied()
        .take_while(|&byte| byte != b'\n')
        .collect();
    if chosen.len() > MAX_CHAT_LINE_SIZE {
        return false;
    }
    if mind.current_chat.len() < MAX_CHAT_LINE_SIZE {
        mind.current_chat.resize(MAX_CHAT_LINE_SIZE, 0);
    }
    let put = |chat: &mut Vec<u8>, index: usize, byte: u8| {
        if index >= chat.len() {
            chat.resize(index + 1, 0);
        }
        chat[index] = byte;
    };
    let (mut read, mut write) = (0, 0);
    while at(&chosen, read) != 0 {
        if at(&chosen, read) == b'%' && at(&chosen, read + 1) != b'%' {
            read += 1;
            let object = match at(&chosen, read) {
                b's' => mind.chat_object,
                b'a' => mind.chat_alt_object,
                _ => None,
            };
            if let Some(name) = object
                .and_then(|object| game.senses.client(object))
                .map(|client| client.netname.clone())
            {
                for &byte in &name {
                    put(&mut mind.current_chat, write, byte);
                    write += 1;
                }
                // The loop's own step follows.
                write = write.wrapping_sub(1);
            }
        } else {
            put(&mut mind.current_chat, write, at(&chosen, read));
        }
        write = write.wrapping_add(1);
        read += 1;
    }
    put(&mut mind.current_chat, write, 0);
    mind.do_chat = if section == b"GeneralGreetings" { 2 } else { 1 };
    let said = mind
        .current_chat
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(mind.current_chat.len());
    mind.chat_time_stored = (said as u64 * 45).wrapping_add(rng.irand(1300, 1500) as u64) as f32;
    mind.chat_time = game.level_time as f32 + mind.chat_time_stored;
    true
}

/// The bots whose minds these work on, by client.
pub type Minds = [Option<BotMind>];

/// `BotLovedOneDied`: bot `slot` hears that `loved` (whom it loves at `level`) died by
/// the hand of `lastHurt`. Not in a duel, not over a mere liking outside team games, not
/// against a teammate, not over a suicide or its own blow, not without
/// `bot_attachments`. A loved one killing a loved one is lamented; otherwise the killer
/// is hated more (said aloud at the height of it), or — unless it hates its current
/// enemy nearly to the height — becomes the one it hates, lamented.
pub fn loved_one_died(
    minds: &mut Minds,
    slot: usize,
    loved: usize,
    level: i32,
    game: &ChatGame,
    rules: &SensingRules,
    rng: &mut Rng,
) {
    let Some(killer) = minds[loved].as_ref().and_then(|loved| loved.last_hurt) else {
        return;
    };
    if game.senses.client(killer).is_none() || killer == loved as i32 {
        return;
    }
    if rules.gametype == GT_DUEL || rules.gametype == GT_POWERDUEL {
        return;
    }
    let same_team = |one: usize, other: i32| {
        game.senses
            .client(one as i32)
            .zip(game.senses.client(other))
            .is_some_and(|(one, other)| on_same_team(one, other, rules.gametype))
    };
    if rules.gametype < GT_TEAM {
        if level < 2 {
            return;
        }
    } else if same_team(slot, killer) {
        return;
    }
    if slot as i32 == killer || !rules.attachments {
        return;
    }
    let Some(mind) = minds[slot].as_mut() else {
        return;
    };
    let bot = SensingBot {
        client: slot as i32,
        duel_in_progress: false,
        duel_index: 0,
        is_jedi_master: false,
    };
    if !pass_loved_one_check(&mind.loved, &bot, game.senses, killer, rules) {
        mind.chat_object = Some(killer);
        mind.chat_alt_object = Some(loved as i32);
        do_chat(mind, b"LovedOneKilledLovedOne", false, game, rng);
        return;
    }
    if mind.revenge_enemy == Some(killer) {
        if mind.revenge_hate_level < mind.loved_death_thresh {
            mind.revenge_hate_level += 1;
            if mind.revenge_hate_level == mind.loved_death_thresh {
                mind.chat_object = Some(killer);
                mind.chat_alt_object = None;
                do_chat(mind, b"Hatred", true, game, rng);
            }
        }
    } else if mind.revenge_hate_level < mind.loved_death_thresh - 1 {
        mind.chat_object = Some(loved as i32);
        mind.chat_alt_object = Some(killer);
        do_chat(mind, b"BelovedKilled", false, game, rng);
        mind.revenge_hate_level = 0;
        mind.revenge_enemy = Some(killer);
    }
}

/// `BotDeathNotify`: every bot that loves `dead` by name hears of its death.
pub fn death_notify(
    minds: &mut Minds,
    dead: usize,
    game: &ChatGame,
    rules: &SensingRules,
    rng: &mut Rng,
) {
    let Some(name) = game
        .senses
        .client(dead as i32)
        .map(|client| client.netname.clone())
    else {
        return;
    };
    for slot in 0..minds.len() {
        let level = minds[slot].as_ref().and_then(|mind| {
            mind.loved
                .iter()
                .find(|love| love.name == name)
                .map(|love| love.level)
        });
        if let Some(level) = level {
            loved_one_died(minds, slot, dead, level, game, rules, rng);
        }
    }
}

/// `BotReplyGreetings`: the bots that chat answer `greeter`'s greeting, at most four.
pub fn reply_greetings(minds: &mut Minds, greeter: usize, game: &ChatGame, rng: &mut Rng) {
    let mut answered = 0;
    for slot in 0..minds.len() {
        if slot != greeter
            && let Some(mind) = minds[slot].as_mut().filter(|mind| mind.can_chat != 0)
        {
            mind.chat_object = Some(greeter as i32);
            mind.chat_alt_object = None;
            if do_chat(mind, b"ResponseGreetings", false, game, rng) {
                answered += 1;
            }
        }
        if answered > 3 {
            return;
        }
    }
}
