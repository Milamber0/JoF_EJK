//! How a match ends: the exit rules, the intermission, and the exit itself.
//!
//! `codemp/game/g_main.c` decides this in two functions that run one after the other
//! every frame. [`check_exit_rules`] is `CheckExitRules` (`:1915`) — what ends a match
//! and what everyone is told about it. [`check_intermission_exit`] is
//! `CheckIntermissionExit` (`:1644`) — how long the scoreboard stays up once it has.
//! Between them sit `LogExit` (`:1572`), which queues the intermission, and
//! `BeginIntermission` (`:1338`), which moves everyone to the intermission point.
//!
//! The rules read scores and write only [`MatchEnd`], so a caller can run them on any
//! shape of world: what a legacy client is later shown of an intermission is the
//! adapter's business, not this module's.

use sjk_protocol::PlayerState;

/// `CS_INTERMISSION` (`codemp/game/bg_public.h:110`): set to `"1"` the moment a limit is
/// hit, a second before the intermission itself, so that clients can stop their sounds.
pub const CS_INTERMISSION: usize = 22;
/// `STAT_CLIENTS_READY` (`bg_public.h:596`): the bit mask of clients asking to leave,
/// which every client's scoreboard draws from.
pub const STAT_CLIENTS_READY: usize = 7;
/// `INTERMISSION_DELAY_TIME` (`g_local.h:58`): the pause between the limit being hit and
/// the intermission starting, so the last frag can be seen.
pub const INTERMISSION_DELAY_TIME: i32 = 1000;
/// "never exit in less than five seconds" (`g_main.c:1845`).
pub const INTERMISSION_FLOOR: i32 = 5000;
/// The wait after the first player asks to leave (`g_main.c:1875`).
pub const READY_TIMEOUT: i32 = 10_000;
/// A duel intermission: the round's end two seconds in, the next round after four.
pub const DUEL_ROUND_DELAY: i32 = 2_000;
pub const DUEL_NEXT_ROUND_DELAY: i32 = 4_000;
/// Only the first sixteen clients get a bit in the ready mask (`g_main.c:1667`).
pub const READY_MASK_CLIENTS: usize = 16;

/// `gametype_t` (`codemp/game/bg_public.h`), the entries these rules branch on.
pub const GT_FFA: i32 = 0;
/// `GT_DUEL`.
pub const GT_DUEL: i32 = 3;
/// `GT_POWERDUEL`.
pub const GT_POWERDUEL: i32 = 4;
/// `GT_TEAM`: at and above this a match is scored by team.
pub const GT_TEAM: i32 = 6;
/// `GT_SIEGE`: neither the tie gate, the time limit nor the kill limit applies to it.
pub const GT_SIEGE: i32 = 7;
/// `GT_CTF`: at and above this the capture limit applies.
pub const GT_CTF: i32 = 8;

/// `TEAM_FREE` (`bg_public.h` `team_t`).
pub const TEAM_FREE: i32 = 0;
/// `TEAM_RED`.
pub const TEAM_RED: i32 = 1;
/// `TEAM_BLUE`.
pub const TEAM_BLUE: i32 = 2;
/// `TEAM_SPECTATOR`.
pub const TEAM_SPECTATOR: i32 = 3;

/// The level's own end state: `level.intermissionQueued`, `level.intermissiontime`,
/// `level.readyToExit` and `level.exitTime` (`g_local.h`), which is all
/// `CheckExitRules` and `CheckIntermissionExit` keep between frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MatchEnd {
    /// The millisecond `LogExit` ran; zero once the intermission has begun.
    pub queued: i32,
    /// The millisecond the intermission began; zero while the match is being played.
    pub intermission_time: i32,
    /// Whether anybody has asked to leave and somebody has not.
    pub ready_to_exit: bool,
    /// The millisecond the first player asked to leave.
    pub exit_time: i32,
    /// `gDuelExit`: this intermission ends the tournament (`DuelLimitHit` at its start),
    /// so it waits for its players as any other does.
    pub duel_exit: bool,
    /// `gDidDuelStuff`: the round's end has been dealt with (two seconds in, once).
    /// Both reset with the level, as the module's globals do at a restart.
    pub did_duel_stuff: bool,
    /// Set by [`check_intermission_exit`] in the frame the round's end is due: the
    /// caller sends the loser to the back of the line, brings the next one in, and clears
    /// this.
    pub duel_round_due: bool,
    /// `g_endPDuel`: a power duel's round was decided by a death, to be ended at the next
    /// check of the rules.
    pub end_power_duel: bool,
}

impl MatchEnd {
    /// Whether the match is over — queued or at the scoreboard. Commands marked
    /// `CMD_NOINTERMISSION` are refused for exactly this span (`g_cmds.c:3460`).
    pub const fn ending(&self) -> bool {
        self.queued != 0 || self.intermission_time != 0
    }
}

/// One playing client, as the exit rules see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Contender {
    /// Its client number, which is its bit in the ready mask.
    pub client: usize,
    /// `sess.sessionTeam`.
    pub team: i32,
    /// `ps.persistant[PERS_SCORE]`.
    pub score: i32,
    /// `pers.connected == CON_CONNECTED`.
    pub connected: bool,
    /// `r.svFlags & SVF_BOT`: bots are not waited for at an intermission.
    pub bot: bool,
    /// `client->readyToExit`.
    pub ready: bool,
    /// `iAmALoser`: a power duellist dead this round and waiting in line, still counted
    /// as playing (`CalculateRanks`).
    pub loser: bool,
}

/// The server's limits, read fresh every frame as the reference reads its cvars.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    /// `level.gametype`.
    pub gametype: i32,
    /// `fraglimit`, zero for none.
    pub fraglimit: i32,
    /// `capturelimit`, zero for none.
    pub capturelimit: i32,
    /// `timelimit` in minutes, zero for none — a float, as the cvar is.
    pub timelimit: f32,
    /// `level.warmupTime`: a time limit does not run during warmup.
    pub warmup_time: i32,
    /// `d_noIntermissionWait`: leave the scoreboard the moment the floor is past.
    pub no_intermission_wait: bool,
    /// `duel_fraglimit`: the duels a player must win to end the tournament, 0 for none.
    pub duel_fraglimit: i32,
}

impl Default for Limits {
    fn default() -> Self {
        // The reference's own cvar defaults (`g_xcvar.h`): twenty frags, eight captures,
        // no time limit.
        Self {
            gametype: GT_FFA,
            fraglimit: 20,
            capturelimit: 8,
            timelimit: 0.0,
            warmup_time: 0,
            no_intermission_wait: false,
            duel_fraglimit: 10,
        }
    }
}

/// What everyone is told when a match ends: the reason `LogExit` writes to the log, and
/// the line the game prints to every client just before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ending {
    /// The `LogExit` string: `"Timelimit hit."`, `"Kill limit hit."`,
    /// `"Capturelimit hit."` — or empty, which duels with a limit of one produce.
    pub reason: &'static str,
    /// Who or what hit the limit, for the print that goes out first.
    pub announce: Announce,
}

/// The print `CheckExitRules` sends to every client before `LogExit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Announce {
    /// `print "@@@TIMELIMIT_HIT.\n"`.
    TimeLimit,
    /// `print "Red @@@HIT_THE_KILL_LIMIT.\n"` and the blue twin, for a team's score.
    TeamKillLimit(i32),
    /// `print "<name>^7 @@@HIT_THE_KILL_LIMIT.\n"` for the client that reached it.
    ClientKillLimit(usize),
    /// The two prints a capture limit sends: the team's name, then the limit.
    CaptureLimit(i32),
    /// A duel whose limit is one prints nothing (`printLimit` stays false).
    Silent,
}

/// What [`check_exit_rules`] did with this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitStep {
    /// The match goes on.
    Playing,
    /// A limit was hit: `LogExit` has queued the intermission and set `CS_INTERMISSION`.
    Logged(Ending),
    /// `INTERMISSION_DELAY_TIME` is up: move everyone to the intermission point.
    BeginIntermission,
    /// The scoreboard's mask changed, or is being written for the first time.
    Ready { mask: i32 },
    /// `ExitLevel`: the level is over.
    ExitLevel { mask: i32 },
}

/// `ScoreIsTied` (`g_main.c:1888`): fewer than two playing clients is never a tie; a
/// team game compares the two team scores; anything else compares the top two players.
///
/// `contenders` must be sorted as `level.sortedClients` is — by score, best first.
pub fn score_is_tied(limits: &Limits, contenders: &[Contender], team_scores: [i32; 2]) -> bool {
    let playing = contenders
        .iter()
        .filter(|c| c.team != TEAM_SPECTATOR)
        .count();
    if playing < 2 {
        return false;
    }
    if limits.gametype >= GT_TEAM {
        return team_scores[0] == team_scores[1];
    }
    let mut scores = contenders
        .iter()
        .filter(|c| c.team != TEAM_SPECTATOR)
        .map(|c| c.score);
    scores.next() == scores.next()
}

/// `CheckExitRules` (`g_main.c:1915`) for one frame, up to but not including the duel
/// and power-duel branches, which belong with those gametypes.
///
/// `contenders` is sorted best-first (as `level.sortedClients` is), `team_scores` is
/// `[red, blue]`, and `state` is the level's own end state, which this updates in place.
/// Once the intermission has begun this forwards to [`check_intermission_exit`], exactly
/// as the reference's first branch does, so a caller only has to call this one.
pub fn check_exit_rules(
    limits: &Limits,
    contenders: &[Contender],
    team_scores: [i32; 2],
    level_time: i32,
    start_time: i32,
    state: &mut MatchEnd,
) -> ExitStep {
    // At the intermission, nothing else is decided: the level waits for its clients.
    if state.intermission_time != 0 {
        return check_intermission_exit(limits, contenders, level_time, state);
    }
    // The second between the limit and the scoreboard, so that the last frag is seen.
    if state.queued != 0 {
        if level_time - state.queued >= INTERMISSION_DELAY_TIME {
            state.queued = 0;
            state.intermission_time = level_time;
            return ExitStep::BeginIntermission;
        }
        return ExitStep::Playing;
    }
    // Sudden death: a tie is never ended by a limit. Siege has rounds of its own, and a
    // duel with a time limit plays its tie out against the clock.
    if limits.gametype != GT_SIEGE
        && score_is_tied(limits, contenders, team_scores)
        && (limits.gametype != GT_DUEL || limits.timelimit == 0.0)
        && limits.gametype != GT_POWERDUEL
    {
        return ExitStep::Playing;
    }
    if limits.gametype != GT_SIEGE && limits.timelimit > 0.0 && limits.warmup_time == 0 {
        // The comparison is the reference's: a float of minutes times sixty thousand,
        // against an integer of milliseconds.
        if (level_time - start_time) as f32 >= limits.timelimit * 60_000.0 {
            return log_exit(
                state,
                level_time,
                Ending {
                    reason: "Timelimit hit.",
                    announce: Announce::TimeLimit,
                },
            );
        }
    }
    // A power duel of three plays until a death decides it; its limits wait for fewer.
    let playing = contenders
        .iter()
        .filter(|c| c.connected && (c.team != TEAM_SPECTATOR || c.loser))
        .count();
    if limits.gametype == GT_POWERDUEL && playing >= 3 {
        if std::mem::take(&mut state.end_power_duel) {
            return log_exit(
                state,
                level_time,
                Ending {
                    reason: "Powerduel ended.",
                    announce: Announce::Silent,
                },
            );
        }
        return ExitStep::Playing;
    }
    // Below two playing clients, no limit ends a match.
    if contenders
        .iter()
        .filter(|c| c.team != TEAM_SPECTATOR)
        .count()
        < 2
    {
        return ExitStep::Playing;
    }
    // A duel whose limit is one has no kill limit at all, and says nothing when it ends.
    let duel = limits.gametype == GT_DUEL || limits.gametype == GT_POWERDUEL;
    let kill_limit = if duel && limits.fraglimit <= 1 {
        ""
    } else {
        "Kill limit hit."
    };
    if limits.gametype < GT_SIEGE && limits.fraglimit != 0 {
        for (team, score) in [(TEAM_RED, team_scores[0]), (TEAM_BLUE, team_scores[1])] {
            if score >= limits.fraglimit {
                return log_exit(
                    state,
                    level_time,
                    Ending {
                        reason: kill_limit,
                        announce: Announce::TeamKillLimit(team),
                    },
                );
            }
        }
        // The reference walks the client slots, so where two players cross the limit in
        // the same frame it is the lower client number that is named — not the higher
        // score. `contenders` is sorted best-first, so the slot order is found rather
        // than assumed, and without a second list.
        let first = contenders
            .iter()
            .filter(|c| c.connected && c.team == TEAM_FREE && c.score >= limits.fraglimit)
            .min_by_key(|c| c.client);
        if let Some(contender) = first {
            let announce = if kill_limit.is_empty() {
                Announce::Silent
            } else {
                Announce::ClientKillLimit(contender.client)
            };
            return log_exit(
                state,
                level_time,
                Ending {
                    reason: kill_limit,
                    announce,
                },
            );
        }
    }
    if limits.gametype >= GT_CTF && limits.capturelimit != 0 {
        for (team, score) in [(TEAM_RED, team_scores[0]), (TEAM_BLUE, team_scores[1])] {
            if score >= limits.capturelimit {
                return log_exit(
                    state,
                    level_time,
                    Ending {
                        reason: "Capturelimit hit.",
                        announce: Announce::CaptureLimit(team),
                    },
                );
            }
        }
    }
    ExitStep::Playing
}

/// `LogExit` (`g_main.c:1572`): the intermission is queued and every client is told
/// through `CS_INTERMISSION` at once, a second before the scoreboard appears.
fn log_exit(state: &mut MatchEnd, level_time: i32, ending: Ending) -> ExitStep {
    state.queued = level_time;
    ExitStep::Logged(ending)
}

/// `CheckIntermissionExit` (`g_main.c:1644`): the scoreboard's ready mask, the
/// five-second floor, and the two ways past it.
///
/// Note the order the reference has: the mask is written to every client's scoreboard
/// *before* the floor is tested, so the ticks appear while the floor still holds.
pub fn check_intermission_exit(
    limits: &Limits,
    contenders: &[Contender],
    level_time: i32,
    state: &mut MatchEnd,
) -> ExitStep {
    let mut ready = 0;
    let mut not_ready = 0;
    let mut mask = 0;
    for contender in contenders.iter().filter(|c| c.connected && !c.bot) {
        if contender.ready {
            ready += 1;
            if contender.client < READY_MASK_CLIENTS {
                mask |= 1 << contender.client;
            }
        } else {
            not_ready += 1;
        }
    }
    // A duel between rounds: two seconds in, the round's end; four seconds in, the next
    // round, whoever is ready — unless the tournament is over (`g_main.c:1673-1831`).
    let duel = limits.gametype == GT_DUEL || limits.gametype == GT_POWERDUEL;
    if duel && !state.did_duel_stuff && level_time > state.intermission_time + DUEL_ROUND_DELAY {
        state.did_duel_stuff = true;
        state.duel_round_due = true;
    }
    if duel && !state.duel_exit {
        if level_time > state.intermission_time + DUEL_NEXT_ROUND_DELAY {
            return exit_level(state, 0);
        }
        return ExitStep::Ready { mask: 0 };
    }
    if level_time < state.intermission_time + INTERMISSION_FLOOR {
        return ExitStep::Ready { mask };
    }
    if limits.no_intermission_wait {
        return exit_level(state, mask);
    }
    // Nobody wants to go: the timer is cleared, so a player who asks later starts a
    // fresh ten seconds.
    if ready == 0 {
        state.ready_to_exit = false;
        return ExitStep::Ready { mask };
    }
    if not_ready == 0 {
        return exit_level(state, mask);
    }
    if !state.ready_to_exit {
        state.ready_to_exit = true;
        state.exit_time = level_time;
    }
    if level_time < state.exit_time + READY_TIMEOUT {
        return ExitStep::Ready { mask };
    }
    exit_level(state, mask)
}

/// `ExitLevel` (`g_main.c:1434`) for the gametypes without a queue of their own: the
/// intermission ends, the engine is asked to run `vstr nextmap`, and the scores are
/// cleared so that the next level does not enter an intermission at once.
fn exit_level(state: &mut MatchEnd, mask: i32) -> ExitStep {
    state.intermission_time = 0;
    ExitStep::ExitLevel { mask }
}

/// The console command `ExitLevel` asks the engine to run (`g_main.c:1459-1464`).
pub const NEXT_MAP_COMMAND: &str = "vstr nextmap\n";
/// What it asks for instead when the level is being played again (duel between rounds,
/// siege with the teams switching).
pub const MAP_RESTART_COMMAND: &str = "map_restart 0\n";

/// `ClientIntermissionThink`'s latch (`g_active.c:838-843`): a player is ready once it
/// presses attack or the holdable-item button, and stays ready — "once a player says
/// ready, it should stick". The edge is what counts, not the button being down.
pub fn asked_to_leave(old_buttons: i32, buttons: i32) -> bool {
    const BUTTON_ATTACK: i32 = 1;
    const BUTTON_USE_HOLDABLE: i32 = 2;
    buttons & (BUTTON_ATTACK | BUTTON_USE_HOLDABLE) & (old_buttons ^ buttons) != 0
}

/// `MoveClientToIntermission` (`g_main.c:1240`): where a player stands while the
/// scoreboard is up, and what of it is switched off.
///
/// The entity's own side of it — `eType` back to `ET_GENERAL`, no model, no sound, no
/// contents — is the caller's, because this crate does not own the entity.
pub fn move_client_to_intermission(player: &mut PlayerState, origin: [f32; 3], angles: [f32; 3]) {
    /// `eFlags`, `eFlags2` and `rocketLockIndex` in `msg.cpp`'s player-state table.
    const PS_EFLAGS: usize = 17;
    const PS_EFLAGS2: usize = 90;
    const PS_ROCKET_LOCK_INDEX: usize = 24;
    const PS_ROCKET_LOCK_TIME: usize = 79;
    player.set_origin(origin);
    player.set_view_angles(angles);
    player.set_movement_type(crate::intermission::PM_INTERMISSION);
    // "clean up powerup info" (`g_main.c:1253-1254`).
    player.powerups.fill(0);
    player.set_raw_field(PS_EFLAGS, 0);
    player.set_raw_field(PS_EFLAGS2, 0);
    player.set_raw_field(
        PS_ROCKET_LOCK_INDEX,
        u32::from(sjk_protocol::ENTITY_NUMBER_NONE),
    );
    player.set_raw_field(PS_ROCKET_LOCK_TIME, 0);
}
