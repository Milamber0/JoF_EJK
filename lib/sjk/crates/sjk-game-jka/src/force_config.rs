//! A player's Force configuration — the `forcepowers` userinfo key, "rank-side-powers" —
//! made legal for a server: `BG_LegalizedForcePowers` (OpenJK `codemp/game/bg_misc.c`),
//! held against the compiled function (`tools/game-oracle/forcepowers.c`).

/// `NUM_FORCE_POWERS`.
pub const FORCE_POWERS: usize = 18;
const FP_HEAL: usize = 0;
const FP_LEVITATION: usize = 1;
const FP_PUSH: usize = 3;
const FP_PULL: usize = 4;
const FP_GRIP: usize = 6;
const FP_LIGHTNING: usize = 7;
const FP_RAGE: usize = 8;
const FP_PROTECT: usize = 9;
const FP_ABSORB: usize = 10;
const FP_DRAIN: usize = 13;
/// `GT_SIEGE`: a siege bot's powers are its class's.
const GT_SIEGE: i32 = 7;
const FP_TEAM_HEAL: usize = 11;
const FP_TEAM_FORCE: usize = 12;
const FP_SABER_OFFENSE: usize = 15;
const FP_SABER_DEFENSE: usize = 16;
const FP_SABERTHROW: usize = 17;
/// `FORCE_LIGHTSIDE`.
pub const FORCE_LIGHT_SIDE: i32 = 1;
/// `FORCE_DARKSIDE`.
pub const FORCE_DARK_SIDE: i32 = 2;
/// `GT_TEAM`: the first game type with teams.
const GT_TEAM: i32 = 6;
/// `DEFAULT_FORCEPOWERS_LEN`: what a configuration is cut to before it is read.
const CONFIGURATION_BYTES: usize = 22;

/// `forceMasteryPoints`: the points each rank may spend.
const MASTERY_POINTS: [i32; 8] = [0, 5, 10, 20, 30, 50, 75, 100];
/// `bgForcePowerCost`: what each level of each power costs on top of the one below.
const COST: [[i32; 4]; FORCE_POWERS] = [
    [0, 2, 4, 6],
    [0, 0, 2, 6],
    [0, 2, 4, 6],
    [0, 1, 3, 6],
    [0, 1, 3, 6],
    [0, 4, 6, 8],
    [0, 1, 3, 6],
    [0, 2, 5, 8],
    [0, 4, 6, 8],
    [0, 2, 5, 8],
    [0, 1, 3, 6],
    [0, 1, 3, 6],
    [0, 1, 3, 6],
    [0, 2, 4, 6],
    [0, 2, 5, 8],
    [0, 1, 5, 8],
    [0, 1, 5, 8],
    [0, 4, 6, 8],
];
/// `forcePowerDarkLight`: the side a power belongs to, 0 for either.
const SIDE: [i32; FORCE_POWERS] = [1, 0, 0, 0, 0, 1, 2, 2, 2, 1, 1, 1, 2, 2, 0, 0, 0, 0];

/// The server's rules for a Force configuration.
#[derive(Clone, Copy, Debug)]
pub struct ForceRules {
    /// `g_maxForceRank`, 0 to 7.
    pub max_rank: usize,
    /// Saber attack and defence cost nothing at their first level, and are never
    /// taken below it (`HasSetSaberOnly`: a server whose only weapon is the saber).
    pub free_saber: bool,
    /// The side a Force-aligned team imposes, or 0.
    pub team_side: i32,
    /// `g_gametype`: the two team powers exist from `GT_TEAM` up.
    pub gametype: i32,
    /// `g_forcePowerDisable`, one bit per power.
    pub disabled: u32,
}

/// A legal configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ForceConfiguration {
    /// The rank it is written with: always the server's.
    pub rank: usize,
    /// Light or dark.
    pub side: i32,
    /// Each power's level, 0 to 3.
    pub levels: [u8; FORCE_POWERS],
    /// Whether the player's own configuration was legal as it stood. The reference
    /// warns the player when it was not.
    pub was_valid: bool,
}

impl ForceConfiguration {
    /// "rank-side-powers", as the reference writes it back.
    pub fn text(&self) -> String {
        let levels: String = self
            .levels
            .iter()
            .map(|&level| char::from(b'0' + level))
            .collect();
        format!("{}-{}-{levels}", self.rank, self.side)
    }
}

/// `atoi` on a run of bytes that holds at most a few digits.
fn number(text: &[u8]) -> i32 {
    let digits = text.iter().take_while(|byte| byte.is_ascii_digit()).take(9);
    digits.fold(0, |value, byte| value * 10 + i32::from(byte - b'0'))
}

/// `BG_LegalizedForcePowers`: read `configuration`, keep what the rules allow, and when
/// the points do not suffice cut powers back — the lowest first, "because the higher
/// powers are probably more important", sparing jump and a free saber's first level.
pub fn legalize_force_powers(configuration: &[u8], rules: ForceRules) -> ForceConfiguration {
    // `WP_InitForcePowers` copies the key into a buffer of this size first.
    let configuration = &configuration[..configuration.len().min(CONFIGURATION_BYTES)];
    let configuration = &configuration[..configuration
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(configuration.len())];
    let mut was_valid = true;
    let mut parts = configuration.splitn(3, |&byte| byte == b'-');
    let (_, side, powers) = (
        parts.next(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    );
    let mut side = number(side);
    if side != FORCE_LIGHT_SIDE && side != FORCE_DARK_SIDE {
        // "Not a valid side. You will be dark. Because I said so."
        (side, was_valid) = (FORCE_DARK_SIDE, false);
    }
    if rules.team_side != 0 {
        // A Force-aligned team decides; the player keeps what survives the filter.
        side = rules.team_side;
    }
    let mut levels = [0_i32; FORCE_POWERS];
    for (level, &byte) in levels.iter_mut().zip(
        powers
            .iter()
            .take_while(|byte| (b'0'..=b'3').contains(byte)),
    ) {
        *level = i32::from(byte - b'0');
    }
    for (power, level) in levels.iter_mut().enumerate() {
        if SIDE[power] != 0 && SIDE[power] != side || rules.disabled & (1 << power) != 0 {
            *level = 0;
        }
    }
    if rules.gametype < GT_TEAM {
        (levels[FP_TEAM_HEAL], levels[FP_TEAM_FORCE]) = (0, 0);
    }
    let allowed = MASTERY_POINTS[rules.max_rank.min(MASTERY_POINTS.len() - 1)];
    // Jump's first level is free for everyone, the saber's for a free saber.
    let free_first_level = |power: usize| {
        power == FP_LEVITATION
            || rules.free_saber && matches!(power, FP_SABER_OFFENSE | FP_SABER_DEFENSE)
    };
    let mut used: i32 = (0..FORCE_POWERS)
        .map(|power| {
            (1..=levels[power] as usize)
                .filter(|&level| !(level == 1 && free_first_level(power)))
                .map(|level| COST[power][level])
                .sum::<i32>()
        })
        .sum();
    if used > allowed {
        was_valid = false;
        let least_defence = i32::from(rules.free_saber);
        let mut cycle = 2;
        for _ in 0..=FORCE_POWERS {
            if used <= allowed {
                break;
            }
            for power in 0..FORCE_POWERS {
                if used <= allowed {
                    break;
                }
                if levels[power] == 0 || levels[power] >= cycle {
                    continue;
                }
                // Saber attack gives way last: throw goes first, then defence.
                let drained = if power == FP_SABER_OFFENSE
                    && (levels[FP_SABER_DEFENSE] > least_defence || levels[FP_SABERTHROW] > 0)
                {
                    if levels[FP_SABERTHROW] != 0 {
                        FP_SABERTHROW
                    } else {
                        FP_SABER_DEFENSE
                    }
                } else {
                    power
                };
                // A free first level is never taken.
                while levels[drained] > 0
                    && used > allowed
                    && (levels[drained] > 1 || !free_first_level(drained))
                {
                    used -= COST[drained][levels[drained] as usize];
                    levels[drained] -= 1;
                }
            }
            cycle += 1;
        }
        if used > allowed {
            // "Still? Fine then.. we will kill all of your powers, except the freebies."
            for (power, level) in levels.iter_mut().enumerate() {
                *level = i32::from(free_first_level(power));
            }
        }
    }
    if rules.free_saber {
        levels[FP_SABER_OFFENSE] = levels[FP_SABER_OFFENSE].max(1);
        levels[FP_SABER_DEFENSE] = levels[FP_SABER_DEFENSE].max(1);
    }
    levels[FP_LEVITATION] = levels[FP_LEVITATION].max(1);
    // A disabled jump is capped at its first level; a disabled saber power is raised
    // to its third: "it's the way things work for the case of all powers disabled".
    if rules.disabled & (1 << FP_LEVITATION) != 0 {
        levels[FP_LEVITATION] = 1;
    }
    for power in [FP_SABER_OFFENSE, FP_SABER_DEFENSE] {
        if rules.disabled & (1 << power) != 0 {
            levels[power] = 3;
        }
    }
    if levels[FP_SABER_OFFENSE] < 1 {
        (levels[FP_SABER_DEFENSE], levels[FP_SABERTHROW]) = (0, 0);
    }
    ForceConfiguration {
        rank: rules.max_rank,
        side,
        levels: levels.map(|level| level.clamp(0, 3) as u8),
        was_valid,
    }
}

/// The server settings `WP_InitForcePowers` reads.
#[derive(Clone, Copy, Debug)]
pub struct ForceServerSettings {
    /// `g_gametype`.
    pub gametype: i32,
    /// `g_maxForceRank` as set; anything outside 1 to 7 means 7.
    pub max_rank: i32,
    /// `g_forcePowerDisable`.
    pub disabled: u32,
    /// `g_forceBasedTeams`: red is dark, blue is light.
    pub force_based_teams: bool,
    /// `g_teamAutoJoin`: players are not sent to the spectators to set themselves up.
    pub team_auto_join: bool,
    /// `g_weaponDisable`, or `g_duelWeaponDisable` in the duel game types.
    pub weapons_disabled: u32,
    /// `sv_cheats`, which the game reads for its `CMD_CHEAT` commands.
    pub cheats: bool,
}

const GT_HOLOCRON: i32 = 1;
const GT_JEDI_MASTER: i32 = 2;
const GT_DUEL: i32 = 3;
const GT_POWER_DUEL: i32 = 4;
const TEAM_RED: i32 = 1;
const TEAM_BLUE: i32 = 2;
const TEAM_SPECTATOR: i32 = 3;
/// `WP_NUM_WEAPONS`; the saber is weapon 3 and "none" weapon 0.
const WEAPONS: u32 = 19;
/// `DEFAULT_FORCEPOWERS`.
/// `DEFAULT_FORCEPOWERS`.
pub const DEFAULT_FORCE_POWERS: &[u8] = b"5-1-000000000000000000";
const DEFAULT_CONFIGURATION: &[u8] = DEFAULT_FORCE_POWERS;

impl ForceServerSettings {
    /// `HasSetSaberOnly` (`w_saber.c`): every weapon but the saber is disabled, which
    /// makes saber attack and defence free. Never in Jedi Master.
    pub fn saber_only(&self) -> bool {
        self.gametype != GT_JEDI_MASTER
            && (1..WEAPONS)
                .filter(|&weapon| weapon != 3)
                .all(|weapon| self.weapons_disabled & (1 << weapon) != 0)
    }

    /// The disable mask for the game type: duels have their own cvar.
    pub fn uses_duel_weapons(gametype: i32) -> bool {
        matches!(gametype, GT_DUEL | GT_POWER_DUEL)
    }
}

/// What `WP_InitForcePowers` decides for a human player (`w_force.c:158-420`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForceInitialisation {
    /// `fd.forcePowerLevel`.
    pub levels: [u8; FORCE_POWERS],
    /// `fd.forcePowersKnown`.
    pub known: u32,
    /// `fd.forcePowerSelected`.
    pub selected: i32,
    /// `fd.forceSide`.
    pub side: i32,
    /// The player is sent to the spectators "so they can set their powerups up without
    /// being bothered": `sess.sessionTeam` becomes `TEAM_SPECTATOR`.
    pub to_spectators: bool,
    /// Server commands for this player, in order: the invalid-string print, `spc` (open
    /// the profile menu), `nfr <rank> <show menu> <team>`.
    pub commands: Vec<Vec<u8>>,
}

/// `WP_InitForcePowers` for a human player outside siege. `team` is `sess.sessionTeam`,
/// `told_before` is `sess.setForce`, `selected_before` is `sess.selectedFP`. The caller
/// A bot's side of `WP_InitForcePowers`: its personality's configuration and its skill.
#[derive(Clone, Copy, Debug)]
pub struct BotForce<'a> {
    pub configuration: &'a [u8],
    pub skill: f32,
}

/// sets `sess.setForce` afterwards, whatever happened.
pub fn initialise_force_powers(
    configuration: &[u8],
    settings: ForceServerSettings,
    team: i32,
    told_before: bool,
    selected_before: u32,
    bot: Option<BotForce<'_>>,
) -> ForceInitialisation {
    let mut commands = Vec::new();
    // "if server has no max rank, default to max (50)"
    let max_rank = if (1..=7).contains(&settings.max_rank) {
        settings.max_rank as usize
    } else {
        7
    };
    let configuration = &configuration[..configuration.len().min(CONFIGURATION_BYTES)];
    let configuration = if configuration.len() == CONFIGURATION_BYTES {
        configuration
    } else {
        commands.push(b"print \"^1Invalid forcepowers string, setting default\n\"".to_vec());
        DEFAULT_CONFIGURATION
    };
    // "if it's a bot just copy the info directly from its personality".
    let configuration = bot.map_or(configuration, |bot| {
        &bot.configuration[..bot.configuration.len().min(CONFIGURATION_BYTES)]
    });
    let team_side = match (settings.force_based_teams, team) {
        (true, TEAM_RED) => FORCE_DARK_SIDE,
        (true, TEAM_BLUE) => FORCE_LIGHT_SIDE,
        _ => 0,
    };
    let rules = ForceRules {
        max_rank,
        free_saber: settings.saber_only(),
        team_side,
        gametype: settings.gametype,
        disabled: settings.disabled,
    };
    let mut legal = legalize_force_powers(configuration, rules);
    // "hmm..I'm going to cheat here": a bot's push and pull at the top, a light-side
    // bot's absorb too, and at skill 4 or more its side's powers, over the legal levels.
    if let Some(bot) = bot
        && settings.gametype != GT_SIEGE
    {
        let skilled = bot.skill >= 4.0;
        let side: &[usize] = match (legal.side, skilled) {
            (FORCE_LIGHT_SIDE, true) => &[FP_ABSORB, FP_HEAL, FP_PROTECT],
            (FORCE_LIGHT_SIDE, false) => &[FP_ABSORB],
            (FORCE_DARK_SIDE, true) => &[FP_GRIP, FP_LIGHTNING, FP_RAGE, FP_DRAIN],
            _ => &[],
        };
        for &power in side.iter().chain(&[FP_PUSH, FP_PULL]) {
            legal.levels[power] = 3;
        }
    }
    let known = (0..FORCE_POWERS)
        .filter(|&power| legal.levels[power] != 0)
        .fold(0, |known, power| known | 1 << power);
    // The reference means "the last power known that is not jump or a saber skill" and
    // computes the last power that is none of those, known or not: always sight.
    let last_selectable = 14;
    let selected = if known & selected_before != 0 {
        selected_before as i32
    } else {
        last_selectable
    };
    let selected = if (0..FORCE_POWERS as i32).contains(&selected) && known & (1 << selected) != 0 {
        selected
    } else {
        last_selectable
    };
    // The player is told its rank; the first time, or whenever its configuration was not
    // legal, with the menu — except where the game type hands out the powers itself.
    let mut to_spectators = false;
    let mut team = team;
    let with_menu = (!legal.was_valid || !told_before)
        && !matches!(settings.gametype, GT_HOLOCRON | GT_JEDI_MASTER);
    // A bot is never sent away to set its powers up.
    if with_menu && !settings.team_auto_join && bot.is_none() {
        (to_spectators, team) = (true, TEAM_SPECTATOR);
        commands.push(b"spc".to_vec());
    }
    commands.push(format!("nfr {max_rank} {} {team}", u8::from(with_menu)).into_bytes());
    ForceInitialisation {
        levels: legal.levels,
        known,
        selected,
        side: legal.side,
        to_spectators,
        commands,
    }
}

/// `WP_InitForcePowers` on a siege server (`w_force.c:203-217` and `:360-366`): a player
/// with a class knows exactly the class's powers — nothing selected, no side — and is told
/// only to open the class menu the first time (`scl`); one without a class is initialised
/// from its userinfo as anywhere else, but is never sent to the spectators nor told its
/// rank — the class menu instead, the first time.
pub fn siege_force_powers(
    generic: ForceInitialisation,
    class_levels: Option<&[u8; FORCE_POWERS]>,
    told_before: bool,
) -> ForceInitialisation {
    let menu = (!told_before).then(|| b"scl".to_vec());
    match class_levels {
        Some(levels) => {
            let known = (0..FORCE_POWERS)
                .filter(|&power| levels[power] != 0)
                .fold(0, |known, power| known | 1 << power);
            ForceInitialisation {
                levels: *levels,
                known,
                selected: -1,
                side: 0,
                to_spectators: false,
                commands: menu.into_iter().collect(),
            }
        }
        None => {
            let mut commands: Vec<Vec<u8>> = generic
                .commands
                .into_iter()
                .filter(|command| command.starts_with(b"print"))
                .collect();
            commands.extend(menu);
            ForceInitialisation {
                to_spectators: false,
                commands,
                ..generic
            }
        }
    }
}
