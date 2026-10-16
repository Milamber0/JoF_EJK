//! A client enters the game, and changes teams: `ClientBegin` (OpenJK
//! `codemp/game/g_client.c`) and `Cmd_Team_f`/`SetTeam`/`BroadcastTeamChange`
//! (`g_cmds.c`) for a human player, composing the Force initialisation, the saber style
//! and the spawn state of the neighbouring modules. Held against
//! `tools/game-oracle/begin.c` and `spawn.c`.
//!
//! Team games' `PickTeam` and balance, following other players, the duel queue and
//! `g_maxGameClients` are not here: outside plain FFA a team command is refused as the
//! reference words it, or reported as not supported.

use crate::client_spawn::{PERS_SPAWN_COUNT, SaberKit, SaberStyle, SpawnRequest, client_spawn};
use crate::event_entity::EventEntity;
use crate::force_config::{ForceInitialisation, ForceServerSettings, initialise_force_powers};
use crate::pmove_anim::AnimationLengths;
use crate::userinfo::AcceptedUserinfo;
use sjk_protocol::{PlayerState, info_value};

const TEAM_FREE: i32 = 0;
const TEAM_SPECTATOR: i32 = 3;
const GT_DUEL: i32 = 3;
const GT_POWER_DUEL: i32 = 4;
const GT_TEAM: i32 = 6;
const GT_SIEGE: i32 = 7;
/// `TEAM_RED` and `TEAM_BLUE`.
pub const TEAM_RED: i32 = 1;
pub const TEAM_BLUE: i32 = 2;

/// What a team game's sides hold, which `PickTeam` weighs: how many are on each, and
/// what each has scored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sides {
    pub red_players: i32,
    pub blue_players: i32,
    pub red_score: i32,
    pub blue_score: i32,
    /// `level.numNonSpectatorClients`: in a duel, two of them fill the game.
    pub non_spectators: i32,
    /// A power duel with three in play, or this client's duel team full
    /// (`G_PowerDuelCheckFail`): it may only spectate.
    pub power_duel_full: bool,
}

/// `PickTeam` (`g_client.c:1341-1358`): the side with fewer players; on an equal count,
/// the side with the lower score; and blue when even those match.
pub fn pick_team(sides: Sides) -> i32 {
    if sides.blue_players > sides.red_players {
        return TEAM_RED;
    }
    if sides.red_players > sides.blue_players {
        return TEAM_BLUE;
    }
    if sides.blue_score > sides.red_score {
        TEAM_RED
    } else {
        TEAM_BLUE
    }
}

/// What the game remembers of a client between spawns (`clientSession_t` and a little
/// of `clientPersistant_t`), as far as beginning and changing teams need it.
#[derive(Clone, Debug, Default)]
pub struct PlayerSession {
    /// `sess.sessionTeam`.
    pub team: i32,
    /// `sess.setForce`: the player has been told its Force rank before.
    pub told_force: bool,
    /// `sess.selectedFP`.
    pub selected_power: u32,
    /// The saber style and the session's memory of it.
    pub saber_style: SaberStyle,
    /// The sabers named in the userinfo are the ones held: `ClientSpawn`'s check found
    /// nothing to set. Cleared when it did, with the kit they make in [`Self::saber_kit`].
    pub sabers_set: bool,
    /// The sabers' kit, for the stance a spawn that set them gives.
    pub saber_kit: SaberKit,
    /// `switchTeamTime`: no team command is taken before this server time.
    pub switch_team_time: i32,
    /// `fd` as the last begin's `WP_InitForcePowers` left it: a respawn spawns with it.
    pub force: Option<ForceInitialisation>,
    /// `sess.spectatorNum`: a spectator's place in the tournament's line, the longest
    /// waiting highest ([`crate::tournament::add_to_queue`]).
    pub spectator_num: i32,
    /// `sess.wins`, `sess.losses`: the duels this client has won and lost.
    pub wins: i32,
    pub losses: i32,
    /// `sess.duelTeam`: in a power duel, the lone or the pair (`power_duel::DUELTEAM_*`).
    pub duel_team: i32,
    /// `sess.spectatorState`: [`SPECTATOR_NOT`], [`SPECTATOR_FREE`],
    /// [`SPECTATOR_FOLLOW`] or [`SPECTATOR_SCOREBOARD`].
    pub spectator_state: i32,
    /// `sess.spectatorClient`: whom a following spectator follows; -1 and -2 are
    /// `follow1` and `follow2`, the first and second playing clients.
    pub spectator_client: i32,
    /// A bot's Force configuration (`botstates[n]->forceinfo`, from its personality) and
    /// its skill, which `WP_InitForcePowers` takes instead of the userinfo's; `None` for a
    /// player.
    pub bot_force: Option<(Vec<u8>, f32)>,
    /// `sess.teamLeader`: leads its side in a team game (`SetLeader`, `CheckTeamLeader`).
    pub team_leader: bool,
    /// `sess.siegeDesiredTeam`: the side a siege player waits to play on (0 for none).
    pub siege_desired_team: i32,
    /// `sess.siegeClass`: the class a siege player picked, by name — `none` until it
    /// picks one, as the session's own write and read leave it.
    pub siege_class: String,
    /// The Force levels of the siege class the player plays (`client->siegeClass`'s),
    /// which `WP_InitForcePowers` takes instead of the userinfo's; `None` without one.
    pub siege_force: Option<[u8; crate::force_config::FORCE_POWERS]>,
}

/// `spectatorState_t`.
pub const SPECTATOR_NOT: i32 = 0;
pub const SPECTATOR_FREE: i32 = 1;
pub const SPECTATOR_FOLLOW: i32 = 2;
pub const SPECTATOR_SCOREBOARD: i32 = 3;

impl PlayerSession {
    /// The end of `Cmd_Team_f`: another team command is refused for five seconds, but
    /// "only if team change really happend" — a player bounced straight back to the
    /// spectators by its Force configuration may try again at once.
    pub fn team_command_done(&mut self, team_before: i32, level_time: i32) {
        if self.team != team_before {
            self.switch_team_time = level_time + 5_000;
        }
    }
}

/// Where and when a client begins.
#[derive(Clone, Copy, Debug)]
pub struct SpawnPlace {
    /// The spawn point's origin, already lifted the nine units the reference adds.
    pub origin: [f32; 3],
    /// Pitch, yaw, roll in degrees.
    pub angles: [f32; 3],
    /// `level.time`.
    pub level_time: i32,
    /// The angles of the client's last command.
    pub command_angles: [i32; 3],
}

/// What `ClientBegin` leaves behind.
pub struct Begun {
    /// The player's state, before the spawn's own think.
    pub state: PlayerState,
    /// Server commands for this client, in order (`spc`, `nfr`, a print).
    pub commands: Vec<Vec<u8>>,
    /// Everyone is told the player entered: `print "<name>^7 @@@PLENTER\n"`. Not for
    /// spectators, and not in a duel.
    pub entered: Option<Vec<u8>>,
    /// The event entities the begin makes, in order: the server's Force rules, told to
    /// everyone; and, for a player, the flash where it appears.
    pub events: Vec<EventEntity>,
}

/// `ClientBegin`: the Force is read from the userinfo, which may send the player to
/// the spectators; the sabers are set on the first spawn; the player spawns. `previous`
/// is the state before, of which the teleport bit, the counters and the saber's entity
/// survive.
#[allow(clippy::too_many_arguments)]
pub fn client_begin(
    client: u16,
    session: &mut PlayerSession,
    userinfo: &[u8],
    accepted: &AcceptedUserinfo,
    settings: ForceServerSettings,
    spawn_invulnerability: i32,
    place: SpawnPlace,
    previous: Option<&PlayerState>,
    lengths: &dyn AnimationLengths,
) -> Begun {
    // The state is cleared first, the style with it.
    (session.saber_style.level, session.saber_style.draw_level) = (0, 0);
    let configuration = info_value(userinfo, b"forcepowers").unwrap_or_default();
    let force = initialise_force_powers(
        configuration,
        settings,
        session.team,
        session.told_force,
        session.selected_power,
        session
            .bot_force
            .as_ref()
            .map(|(configuration, skill)| crate::force_config::BotForce {
                configuration,
                skill: *skill,
            }),
    );
    let force = if settings.gametype == GT_SIEGE {
        crate::force_config::siege_force_powers(
            force,
            session.siege_force.as_ref(),
            session.told_force,
        )
    } else {
        force
    };
    session.saber_style.force_initialised();
    session.told_force = true;
    if force.to_spectators {
        session.team = TEAM_SPECTATOR;
    }
    let attack = i32::from(force.levels[15]);
    sabers_spawned(session, attack);
    // `ClientBegin` clears the state; the spawn count and the entity flags survive.
    let mut persistant = [0; 16];
    if let Some(previous) = previous {
        persistant[PERS_SPAWN_COUNT] = previous.persistent[PERS_SPAWN_COUNT] as i32;
    }
    let request = SpawnRequest {
        client,
        team: session.team,
        level_time: place.level_time,
        origin: place.origin,
        angles: place.angles,
        command_angles: place.command_angles,
        max_health: accepted.max_health,
        custom_rgba: accepted.custom_rgba,
        force: &force,
        saber_style: session.saber_style,
        settings,
        spawn_invulnerability,
        previous_entity_flags: previous
            .and_then(|previous| previous.raw_field(17))
            .unwrap_or(0),
        persistant,
        event_sequence: 0,
        saber_entity: previous
            .and_then(|previous| previous.raw_field(31))
            .unwrap_or(0),
    };
    let state = client_spawn(&request, lengths);
    let playing = session.team != TEAM_SPECTATOR;
    let mut events =
        EventEntity::force_rules(settings.saber_only(), settings.disabled != 0).to_vec();
    if playing {
        events.push(EventEntity::teleport_in(place.origin, client));
    }
    let announced = playing && (settings.gametype != GT_DUEL || settings.gametype == GT_POWER_DUEL);
    let entered =
        announced.then(|| [b"print \"", accepted.name.as_slice(), b"^7 @@@PLENTER\n\""].concat());
    let mut force = force;
    let commands = std::mem::take(&mut force.commands);
    session.force = Some(force);
    Begun {
        state,
        commands,
        entered,
        events,
    }
}

/// `ClientSpawn`'s saber blocks: the stance the sabers bring when they were set on
/// this spawn, then the settled single-saber style.
fn sabers_spawned(session: &mut PlayerSession, attack: i32) {
    if !session.sabers_set {
        session
            .saber_style
            .sabers_changed(session.saber_kit, attack);
        session.sabers_set = true;
    }
    session.saber_style.settle(attack);
}

/// `ClientRespawn` outside power duels and siege, after the body is left behind:
/// `ClientSpawn` again with the Force the last begin gave, the persistants kept, and the
/// flash where the player reappears. A player that has not begun cannot respawn.
#[allow(clippy::too_many_arguments)]
pub fn client_respawn(
    client: u16,
    session: &mut PlayerSession,
    accepted: &AcceptedUserinfo,
    settings: ForceServerSettings,
    spawn_invulnerability: i32,
    place: SpawnPlace,
    previous: &PlayerState,
    lengths: &dyn AnimationLengths,
) -> Option<Begun> {
    let mut force = session.force.clone()?;
    // `WP_SpawnInitForcePowers` (`w_force.c:519-532`): a siege class's powers, again, at
    // every spawn — a class picked since the last begin is the one spawned with.
    if let Some(levels) = session.siege_force {
        force.levels = levels;
        force.known = (0..levels.len())
            .filter(|&power| levels[power] != 0)
            .fold(0, |known, power| known | 1 << power);
        session.force = Some(force.clone());
    }
    let attack = i32::from(force.levels[15]);
    sabers_spawned(session, attack);
    let request = SpawnRequest {
        client,
        team: session.team,
        level_time: place.level_time,
        origin: place.origin,
        angles: place.angles,
        command_angles: place.command_angles,
        max_health: accepted.max_health,
        custom_rgba: accepted.custom_rgba,
        force: &force,
        saber_style: session.saber_style,
        settings,
        spawn_invulnerability,
        previous_entity_flags: previous.raw_field(17).unwrap_or(0),
        persistant: previous.persistent.map(|value| value as i32),
        event_sequence: previous.raw_field(19).unwrap_or(0),
        saber_entity: previous.raw_field(31).unwrap_or(0),
    };
    let state = client_spawn(&request, lengths);
    Some(Begun {
        state,
        commands: Vec::new(),
        entered: None,
        events: vec![EventEntity::teleport_in(place.origin, client)],
    })
}

/// What a `team` command comes to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TeamCommand {
    /// Only this client is told something: its current team, or why not.
    Told(Vec<u8>),
    /// The client begins again on the session's (new) team; first everyone is told
    /// `announcement`, and the client's `CS_PLAYERS` string changes. A spectator who asks
    /// to spectate begins again too, unannounced.
    Changed {
        /// `cp "<name>^7 @@@JOINEDTHE…\n"`, as `BroadcastTeamChange` words it.
        announcement: Option<Vec<u8>>,
        /// It went to the spectators from a team: "they go to the end of the line for
        /// tournaments" ([`crate::tournament::add_to_queue`]).
        queued: bool,
    },
    /// Nothing happens: a player asked for the team it is playing on.
    Unchanged,
    /// A game type whose team rules are not ported.
    NotSupported,
}

/// `Cmd_Team_f` and `SetTeam` outside team games: `arguments` are the command's tokens
/// after `team`.
pub fn team_command(
    session: &mut PlayerSession,
    name: &[u8],
    arguments: &[&[u8]],
    gametype: i32,
    sides: Sides,
    level_time: i32,
) -> TeamCommand {
    let told = |text: &str| TeamCommand::Told(format!("print \"{text}\n\"").into_bytes());
    let [wanted] = arguments else {
        return told(match session.team {
            1 => "@@@PRINTREDTEAM",
            2 => "@@@PRINTBLUETEAM",
            TEAM_SPECTATOR => "@@@PRINTSPECTEAM",
            _ => "@@@PRINTFREETEAM",
        });
    };
    if session.switch_team_time > level_time {
        return told("@@@NOSWITCH");
    }
    if gametype == GT_DUEL && session.team == TEAM_FREE {
        return told("Cannot switch teams in Duel");
    }
    if gametype == GT_POWER_DUEL {
        return told("Cannot switch teams in Power Duel");
    }
    set_team(session, name, wanted, gametype, sides, level_time)
}

/// The team `SetTeam` reads a team game's word as (`g_cmds.c:652-701`): the spectators'
/// words, red and blue by name, and anything else `PickTeam`'s choice.
pub fn team_for_word(wanted: &[u8], sides: Sides) -> i32 {
    let is = |words: &[&[u8]]| words.iter().any(|word| wanted.eq_ignore_ascii_case(word));
    if is(&[
        b"scoreboard",
        b"score",
        b"follow1",
        b"follow2",
        b"spectator",
        b"s",
    ]) {
        TEAM_SPECTATOR
    } else if is(&[b"red", b"r"]) {
        TEAM_RED
    } else if is(&[b"blue", b"b"]) {
        TEAM_BLUE
    } else {
        pick_team(sides)
    }
}

/// `SetTeam(ent, wanted)` itself, as the game calls it without `Cmd_Team_f`'s checks — a
/// tournament bringing the next one in (`"f"`) or sending a loser away (`"s"`).
pub fn set_team(
    session: &mut PlayerSession,
    name: &[u8],
    wanted: &[u8],
    gametype: i32,
    sides: Sides,
    level_time: i32,
) -> TeamCommand {
    // The spectator's state the word asks for (`g_cmds.c:652-672`): the scoreboard is a
    // free spectator ("totally broken on client side"), `follow1`/`follow2` follow the
    // first and second playing clients; any team is not a spectator at all.
    let is = |words: &[&[u8]]| words.iter().any(|word| wanted.eq_ignore_ascii_case(word));
    let spectating = if is(&[b"scoreboard", b"score", b"spectator", b"s"]) {
        (SPECTATOR_FREE, 0)
    } else if is(&[b"follow1"]) {
        (SPECTATOR_FOLLOW, -1)
    } else if is(&[b"follow2"]) {
        (SPECTATOR_FOLLOW, -2)
    } else {
        (SPECTATOR_NOT, 0)
    };
    // `SetTeam`'s team-game branch (`g_cmds.c:673-701`): red and blue by name, and
    // anything else is `PickTeam`'s choice. (`g_teamForceBalance`, which would refuse a
    // side already two ahead, is off by default and is not ported.)
    if gametype >= GT_TEAM {
        let wanted_team = team_for_word(wanted, sides);
        let before = std::mem::replace(&mut session.team, wanted_team);
        if wanted_team == before {
            return TeamCommand::Unchanged;
        }
        (session.spectator_state, session.spectator_client) = spectating;
        session.team_command_done(before, level_time);
        return TeamCommand::Changed {
            announcement: None,
            queued: wanted_team == TEAM_SPECTATOR,
        };
    }
    let spectate = [
        &b"scoreboard"[..],
        b"score",
        b"follow1",
        b"follow2",
        b"spectator",
        b"s",
    ]
    .iter()
    .any(|word| wanted.eq_ignore_ascii_case(word));
    // "override decision if limiting the players": a duel's two are all it holds.
    let full = (gametype == GT_DUEL && sides.non_spectators >= 2)
        || (gametype == GT_POWER_DUEL && sides.power_duel_full);
    let team = if spectate || full {
        TEAM_SPECTATOR
    } else {
        TEAM_FREE
    };
    let before = std::mem::replace(&mut session.team, team);
    if team == before && team != TEAM_SPECTATOR {
        return TeamCommand::Unchanged;
    }
    (session.spectator_state, session.spectator_client) = spectating;
    // A player who leaves for the spectators dies first in the reference (its body
    // stays, it is sent the scores); death is not ported, so here it simply leaves.
    let words = if team == TEAM_SPECTATOR {
        "@@@JOINEDTHESPECTATORS"
    } else {
        "@@@JOINEDTHEBATTLE"
    };
    let announced = team != before;
    let queued = team == TEAM_SPECTATOR && before != TEAM_SPECTATOR;
    TeamCommand::Changed {
        announcement: announced
            .then(|| [b"cp \"", name, b"^7 ", words.as_bytes(), b"\n\""].concat()),
        queued,
    }
}
