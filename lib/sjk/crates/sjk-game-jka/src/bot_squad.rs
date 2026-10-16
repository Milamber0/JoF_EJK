//! Bots' squads (OpenJK `codemp/game/ai_main.c`): a squad leader's orders to the bots
//! that follow it (`CommanderBotAI`: `CommanderBotCTFAI`, `CommanderBotSiegeAI`,
//! `CommanderBotTeamplayAI`), a bot finding a leader (`BotScanForLeader`,
//! `GetLoveLevel`) and a squad's regrouping (`BotDoTeamplayAI`).
//!
//! A commander changes other bots' states, so these work on every bot's mind at once,
//! by client (`None` for a slot without a bot).

use crate::bot_ctf::{
    CTFSTATE_ATTACKER, CTFSTATE_DEFENDER, CTFSTATE_GETFLAGHOME, CTFSTATE_GUARDCARRIER,
    CTFSTATE_RETRIEVAL,
};
use crate::bot_think::BotMind;
use crate::player_death::Rng;

/// `teamplayState`.
pub const TEAMPLAYSTATE_FOLLOWING: i32 = 1;
pub const TEAMPLAYSTATE_ASSISTING: i32 = 2;
pub const TEAMPLAYSTATE_REGROUP: i32 = 3;
const TEAM_RED: i32 = 1;
const GT_DUEL: i32 = 3;
const GT_POWERDUEL: i32 = 4;
const GT_SINGLE_PLAYER: i32 = 5;
const GT_TEAM: i32 = 6;
const GT_SIEGE: i32 = 7;
const GT_CTF: i32 = 8;
const GT_CTY: i32 = 9;

/// A client as squads read it.
#[derive(Clone, Debug, Default)]
pub struct SquadClient {
    pub team: i32,
    pub duel_team: i32,
    /// `SVF_BOT`.
    pub bot: bool,
    pub health: i32,
    pub red_flag: bool,
    pub blue_flag: bool,
    pub netname: Vec<u8>,
}

/// The game around the squads.
#[derive(Clone, Copy, Debug)]
pub struct SquadGame<'a> {
    pub gametype: i32,
    pub level_time: i32,
    /// `bot_attachments`.
    pub attachments: bool,
    /// The clients by number.
    pub clients: &'a [Option<SquadClient>],
}

impl SquadGame<'_> {
    fn client(&self, number: usize) -> Option<&SquadClient> {
        self.clients.get(number).and_then(Option::as_ref)
    }

    /// `OnSameTeam` of two clients.
    pub(crate) fn same_team(&self, one: usize, other: usize) -> bool {
        let (Some(one), Some(other)) = (self.client(one), self.client(other)) else {
            return false;
        };
        match self.gametype {
            GT_POWERDUEL => one.duel_team == other.duel_team,
            GT_SINGLE_PLAYER => one.bot == other.bot,
            gametype if gametype < GT_TEAM => false,
            _ => one.team == other.team,
        }
    }
}

/// `GetLoveLevel`: how much `mind` loves the bot in slot `loved` (by its name); none in
/// a duel, 1 for any without `bot_attachments`.
pub fn love_level(mind: &BotMind, loved: usize, game: &SquadGame) -> i32 {
    if game.gametype == GT_DUEL || game.gametype == GT_POWERDUEL {
        return 0;
    }
    let Some(client) = game.client(loved) else {
        return 0;
    };
    if mind.loved.is_empty() {
        return 0;
    }
    if !game.attachments {
        return 1;
    }
    mind.loved
        .iter()
        .find(|love| love.name == client.netname)
        .map_or(0, |love| love.level)
}

/// `CommanderBotAI`: a squad leader's orders by game type.
pub fn commander(minds: &mut [Option<BotMind>], me: usize, game: &SquadGame, rng: &mut Rng) {
    match game.gametype {
        GT_CTF | GT_CTY => commander_ctf(minds, me, game),
        GT_SIEGE => commander_siege(minds, me, game),
        GT_TEAM => commander_teamplay(minds, me, game, rng),
        _ => {}
    }
}

/// `CommanderBotCTFAI`: the squad (the bots following it, then itself) split by turns
/// between attack and defence — defenders guarding the carrier by turns while the team
/// has the enemy's flag, attackers retrieving by turns while the enemy has its own —
/// never a bot taking the flag home, unless it is alone to retrieve.
fn commander_ctf(minds: &mut [Option<BotMind>], me: usize, game: &SquadGame) {
    let team = game.client(me).map_or(0, |client| client.team);
    let (my_flag_red, enemy_flag_red) = (team == TEAM_RED, team != TEAM_RED);
    let (mut we_have_enemy_flag, mut enemy_has_our_flag, mut on_my_team, mut attackers) =
        (false, false, 0, 0);
    for (number, client) in game.clients.iter().enumerate() {
        let Some(client) = client else { continue };
        let same_team = game.same_team(me, number);
        if (if enemy_flag_red {
            client.red_flag
        } else {
            client.blue_flag
        }) && same_team
        {
            we_have_enemy_flag = true;
        } else if (if my_flag_red {
            client.red_flag
        } else {
            client.blue_flag
        }) && !same_team
        {
            enemy_has_our_flag = true;
        }
        if same_team {
            on_my_team += 1;
        }
        match minds.get(number).and_then(Option::as_ref) {
            Some(mind)
                if mind.ctf_state != CTFSTATE_ATTACKER && mind.ctf_state != CTFSTATE_RETRIEVAL => {}
            _ => attackers += 1,
        }
    }
    let mut squad: Vec<usize> = (0..minds.len())
        .filter(|&number| {
            number != me
                && game.client(number).is_some()
                && minds[number]
                    .as_ref()
                    .is_some_and(|mind| mind.squad_leader == Some(me as i32))
        })
        .collect();
    squad.push(me);
    let (mut defend_next, mut guard_next) = (false, false);
    let mut attack_next = enemy_has_our_flag && !we_have_enemy_flag;
    for member in squad {
        let Some(mind) = minds[member].as_mut() else {
            continue;
        };
        if mind.ctf_state != CTFSTATE_GETFLAGHOME {
            if defend_next {
                mind.ctf_state = if we_have_enemy_flag && guard_next {
                    CTFSTATE_GUARDCARRIER
                } else {
                    CTFSTATE_DEFENDER
                };
                if we_have_enemy_flag {
                    guard_next = !guard_next;
                }
                defend_next = false;
            } else {
                mind.ctf_state = if enemy_has_our_flag && !attack_next {
                    CTFSTATE_RETRIEVAL
                } else {
                    CTFSTATE_ATTACKER
                };
                if enemy_has_our_flag {
                    attack_next = !attack_next;
                }
                defend_next = true;
            }
        } else if (on_my_team < 2 || attackers == 0) && enemy_has_our_flag {
            mind.ctf_state = CTFSTATE_RETRIEVAL;
        }
    }
}

/// `CommanderBotSiegeAI`: up to half the team told to take its own role, those already
/// ordered counting.
fn commander_siege(minds: &mut [Option<BotMind>], me: usize, game: &SquadGame) {
    let (mut squad, mut commanded, mut teammates) = (Vec::new(), 0, 0);
    for number in 0..game.clients.len() {
        if game.client(number).is_none() || !game.same_team(me, number) {
            continue;
        }
        if let Some(mind) = minds.get(number).and_then(Option::as_ref)
            && mind.is_squad_leader == 0
        {
            if mind.state_forced == 0 {
                squad.push(number);
            } else {
                commanded += 1;
            }
        }
        teammates += 1;
    }
    let role = minds[me].as_ref().map_or(0, |mind| mind.siege_state);
    for member in squad {
        if commanded > teammates / 2 {
            break;
        }
        if let Some(mind) = minds[member].as_mut() {
            mind.state_forced = role;
            mind.siege_state = role;
            commanded += 1;
        }
    }
}

/// `CommanderBotTeamplayAI`: one squad leader at most; the first free squad member sent
/// to help the teammate worst off (under 50 health), the others brought back to follow;
/// every 45 to 65 seconds, at random, the squad told to regroup.
fn commander_teamplay(minds: &mut [Option<BotMind>], me: usize, game: &SquadGame, rng: &mut Rng) {
    let (mut squad, mut found_leader) = (Vec::new(), false);
    let (mut in_danger, mut worst_health) = (None, 50);
    for number in 0..game.clients.len() {
        let Some(client) = game.client(number) else {
            continue;
        };
        if !game.same_team(me, number) {
            continue;
        }
        if let Some(mind) = minds.get_mut(number).and_then(Option::as_mut) {
            if found_leader && mind.is_squad_leader != 0 {
                mind.is_squad_leader = 0;
            }
            if mind.is_squad_leader == 0 {
                squad.push(number);
            } else {
                found_leader = true;
            }
        }
        if client.health < worst_health {
            in_danger = Some(number);
            worst_health = client.health;
        }
    }
    let mut helped = false;
    for member in squad {
        if minds[member]
            .as_ref()
            .is_none_or(|mind| mind.state_forced != 0)
        {
            continue;
        }
        let mind = minds[member].as_mut().unwrap();
        if let Some(needy) = in_danger.filter(|_| !helped) {
            mind.teamplay_state = TEAMPLAYSTATE_ASSISTING;
            mind.squad_leader = Some(needy as i32);
            helped = true;
        } else if (in_danger.is_none() || helped) && mind.teamplay_state == TEAMPLAYSTATE_ASSISTING
        {
            mind.teamplay_state = TEAMPLAYSTATE_FOLLOWING;
            mind.squad_leader = Some(me as i32);
        }
        let regroup_due = minds[me]
            .as_ref()
            .is_some_and(|leader| leader.squad_regroup_interval < game.level_time);
        if regroup_due && rng.irand(1, 10) < 5 {
            if let Some(mind) = minds[member].as_mut()
                && mind.teamplay_state == TEAMPLAYSTATE_FOLLOWING
            {
                mind.teamplay_state = TEAMPLAYSTATE_REGROUP;
            }
            let interval = game.level_time + rng.irand(45_000, 65_000);
            if let Some(leader) = minds[me].as_mut() {
                leader.is_squad_leader = 0;
                leader.squad_cannot_lead = game.level_time + 500;
                leader.squad_regroup_interval = interval;
            }
        }
    }
}

/// `BotScanForLeader`: a bot that leads nobody follows the first bot leading a squad on
/// its team — or, outside the team games, one it loves more than a little.
pub fn scan_for_leader(minds: &mut [Option<BotMind>], me: usize, game: &SquadGame) {
    if minds[me]
        .as_ref()
        .is_none_or(|mind| mind.is_squad_leader != 0)
    {
        return;
    }
    for number in 0..game.clients.len() {
        if number == me
            || game.client(number).is_none()
            || !minds[number]
                .as_ref()
                .is_some_and(|mind| mind.is_squad_leader != 0)
        {
            continue;
        }
        let loves = minds[me]
            .as_ref()
            .map_or(0, |mind| love_level(mind, number, game));
        if game.same_team(me, number) || (loves > 1 && game.gametype < GT_TEAM) {
            minds[me].as_mut().unwrap().squad_leader = Some(number as i32);
            break;
        }
    }
}

/// `BotDoTeamplayAI`: a forced role taken; told to regroup, it drops its leader and its
/// own lead.
pub fn do_teamplay(mind: &mut BotMind) {
    if mind.state_forced != 0 {
        mind.teamplay_state = mind.state_forced;
    }
    if mind.teamplay_state == TEAMPLAYSTATE_REGROUP {
        mind.squad_leader = None;
        mind.is_squad_leader = 0;
    }
}
