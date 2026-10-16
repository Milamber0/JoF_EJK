//! A siege round (OpenJK `codemp/game/g_saga.c`): the objectives' status a client reads
//! (`CS_SIEGE_OBJECTIVES`), the round's state (`CS_SIEGE_STATE`) from "waiting for
//! players" through the countdown to the round running and ending, each side's clock,
//! objectives completed and taken back, the round's winner and its points, and the
//! second round with the sides switched and the time to beat (`g_siegeTeamSwitch`).
//!
//! [`SiegeRound`] is `g_saga.c`'s globals; the functions of this module are the
//! reference's functions of the same names. Everything they do to the rest of the game
//! goes through [`SiegeWorld`] — the configstrings, the broadcasts, the scores, the
//! targets used, the level's exit and the players respawned — which also keeps the round,
//! so that a target a rule uses may come back into it. The map's file is
//! [`SiegeMapInfo`].

use crate::siege_class::{
    CS_SIEGE_OBJECTIVES, CS_SIEGE_STATE, CS_SIEGE_TIMEOVERRIDE, CS_SIEGE_WINTEAM, SIEGETEAM_TEAM1,
    SIEGETEAM_TEAM2,
};
use crate::siege_map::SiegeMapInfo;

/// `SIEGE_ROUND_BEGIN_TIME` (`bg_saga.h:34`): "delay 5 secs after players are in game."
pub const SIEGE_ROUND_BEGIN_TIME: i32 = 5_000;
/// `SIEGE_POINTS_OBJECTIVECOMPLETED`, `SIEGE_POINTS_FINALOBJECTIVECOMPLETED`,
/// `SIEGE_POINTS_TEAMWONROUND` (`bg_saga.h:30-32`).
pub const SIEGE_POINTS_OBJECTIVECOMPLETED: i32 = 20;
pub const SIEGE_POINTS_FINALOBJECTIVECOMPLETED: i32 = 30;
pub const SIEGE_POINTS_TEAMWONROUND: i32 = 10;
/// `EV_SIEGE_ROUNDOVER`, `EV_SIEGE_OBJECTIVECOMPLETE`, `EV_SIEGESPEC`.
pub const EV_SIEGE_ROUNDOVER: u32 = 101;
pub const EV_SIEGE_OBJECTIVECOMPLETE: u32 = 102;
pub const EV_SIEGESPEC: u32 = 191;
/// `Q3_INFINITE`: `gSiegeBeginTime` before anything set it.
pub const Q3_INFINITE: i32 = 16_777_216;
/// `ENTITYNUM_NONE`: nobody.
pub const ENTITYNUM_NONE: i32 = 1_023;
/// `TEAM_SPECTATOR`.
const TEAM_SPECTATOR: i32 = 3;

/// `siegePers_t`: what the engine keeps across the map restart between the two rounds
/// (`SiegePersSet`/`SiegePersGet`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SiegePersistent {
    /// The second round is running: the switched sides must beat `last_time`.
    pub beating_time: bool,
    /// The side that won the first round.
    pub last_team: i32,
    /// How long it took, in milliseconds.
    pub last_time: i32,
}

/// One player as the siege rules see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SiegePlayer {
    /// Its place (client number).
    pub client: i32,
    /// `pers.connected == CON_CONNECTED`.
    pub connected: bool,
    /// `sess.sessionTeam`.
    pub team: i32,
    /// `sess.siegeDesiredTeam`.
    pub desired_team: i32,
    /// `ps.pm_flags & PMF_FOLLOW`.
    pub following: bool,
}

/// Who used an entity (`use(self, other, activator)`): `other` passed the use on,
/// `activator` began it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SiegeUser {
    /// The entity that passed the use on.
    pub other: i32,
    /// The entity that began it, if any.
    pub activator: Option<i32>,
    /// Whether that entity is a client.
    pub activator_is_client: bool,
}

/// Everything a siege rule does to the rest of the game, and where the round it plays is
/// kept: a rule reaches the round through [`SiegeWorld::round`] every time, because what
/// it does to the world may come back into the round (a target it uses completes another
/// objective).
pub trait SiegeWorld {
    /// The round being played.
    fn round(&mut self) -> &mut SiegeRound;
    /// `trap->SetConfigstring`.
    fn set_config_string(&mut self, index: usize, value: &[u8]);
    /// The players, in client order (`g_entities[0..MAX_CLIENTS]` that are in use).
    fn players(&self) -> Vec<SiegePlayer>;
    /// A broadcast temp entity at the origin (`G_TempEntity` + `SVF_BROADCAST`) with its
    /// `eventParm`, `weapon` and `trickedentindex`.
    fn broadcast(&mut self, event: u32, parm: i32, weapon: i32, tricked: i32);
    /// `AddScore(client, …, points)`.
    fn add_score(&mut self, client: i32, points: i32);
    /// `G_UseTargets2(activator, activator, name)` from the entity `activator` — a client,
    /// or another entity's number.
    fn use_targets(&mut self, activator: i32, name: &str);
    /// `LogExit(reason)`: the level ends into the intermission.
    fn log_exit(&mut self, reason: &str);
    /// `SiegeRespawn`: the player is spawned again, moved first to the side it wants
    /// (`SetTeamQuick`) when that is not the side it is on.
    fn siege_respawn(&mut self, client: i32);
    /// `trap->SiegePersSet`.
    fn set_persistent(&mut self, persistent: SiegePersistent);
}

/// `g_saga.c`'s globals: the round of the map being played.
#[derive(Clone, Debug, PartialEq)]
pub struct SiegeRound {
    /// The map's file (`siege_valid` is its presence).
    pub map: SiegeMapInfo,
    /// `gObjectiveCfgStr`: `CS_SIEGE_OBJECTIVES` as it stands.
    pub objectives: Vec<u8>,
    /// `imperial_goals_completed`, `rebel_goals_completed`.
    pub completed: [i32; 2],
    /// `imperial_goals_required`, `rebel_goals_required`.
    pub required: [i32; 2],
    /// `imperial_time_limit`, `rebel_time_limit`: zeroed once a side's clock has run out.
    pub time_limit: [i32; 2],
    /// `gImperialCountdown`, `gRebelCountdown`: when each side's time is up.
    pub countdown: [i32; 2],
    /// `gSiegeRoundBegun`, `gSiegeRoundEnded`, `gSiegeRoundWinningTeam`.
    pub begun: bool,
    pub ended: bool,
    pub winner: i32,
    /// `gSiegeBeginTime`.
    pub begin_time: i32,
    /// `g_siegePersistant`.
    pub persistent: SiegePersistent,
    /// `g_siegeTeamSwitch`.
    pub team_switch: bool,
    /// `g_siegeRespawnCheck`: when the next respawn wave is due.
    pub respawn_check: i32,
}

impl SiegeRound {
    /// `InitSiegeMode` (`g_saga.c:118-398`): the round of a map that has just started,
    /// with what the engine kept from the round before (`persistent`) — only read when
    /// the teams switch. Sets `CS_SIEGE_WINTEAM`, `CS_SIEGE_TIMEOVERRIDE` and
    /// `CS_SIEGE_OBJECTIVES` through `set_config_string`.
    pub fn init(
        map: SiegeMapInfo,
        persistent: SiegePersistent,
        team_switch: bool,
        level_time: i32,
        set_config_string: &mut dyn FnMut(usize, &[u8]),
    ) -> Self {
        set_config_string(CS_SIEGE_WINTEAM, b"0");
        let persistent = if team_switch {
            persistent
        } else {
            SiegePersistent::default()
        };
        if team_switch && persistent.beating_time {
            set_config_string(
                CS_SIEGE_TIMEOVERRIDE,
                persistent.last_time.to_string().as_bytes(),
            );
        } else {
            set_config_string(CS_SIEGE_TIMEOVERRIDE, b"0");
        }
        let mut countdown = [0; 2];
        for (index, limit) in map.time_limit.iter().enumerate() {
            if *limit != 0 {
                countdown[index] = level_time
                    + if team_switch && persistent.beating_time {
                        persistent.last_time
                    } else {
                        *limit
                    };
            }
        }
        let objectives = map.objectives_config();
        set_config_string(CS_SIEGE_OBJECTIVES, &objectives);
        Self {
            objectives,
            completed: [0; 2],
            required: map.required,
            time_limit: map.time_limit,
            countdown,
            begun: false,
            ended: false,
            winner: 0,
            begin_time: Q3_INFINITE,
            persistent,
            team_switch,
            respawn_check: 0,
            map,
        }
    }

    /// Where team `team`'s objectives start in the status string, `None` for a team that
    /// has none there.
    fn team_status(&self, team: i32) -> Option<usize> {
        let tag: &[u8] = match team {
            1 => b"t1",
            2 => b"t2",
            _ => return None,
        };
        self.objectives.windows(2).position(|pair| pair == tag)
    }

    /// The status character of `team`'s objective `objective`, when there is one.
    fn status_at(&self, team: i32, objective: i32) -> Option<usize> {
        let mut at = self.team_status(team)?;
        let mut on = 0;
        while at < self.objectives.len() && self.objectives[at] != b'|' {
            if self.objectives[at] == b'-' {
                on += 1;
            }
            if on == objective {
                return Some(at + 1);
            }
            at += 1;
        }
        None
    }

    /// `G_SiegeGetCompletionStatus` (`g_saga.c:458-506`).
    pub fn completion_status(&self, team: i32, objective: i32) -> bool {
        self.status_at(team, objective)
            .is_some_and(|at| self.objectives.get(at) == Some(&b'1'))
    }

    /// Whether the level's exit restarts this map for the second round
    /// (`ExitLevel`, `g_main.c:1456-1466`) rather than moving on.
    pub fn restarts_for_second_round(&self) -> bool {
        self.team_switch && self.persistent.beating_time
    }

    /// The index of team `team`'s side in the two-element arrays.
    fn side_index(team: i32) -> usize {
        if team == i32::from(SIEGETEAM_TEAM1) {
            0
        } else {
            1
        }
    }
}

/// `G_SiegeSetObjectiveComplete` (`g_saga.c:400-455`): the objective's status made `1` (or
/// back to `0`), and the configstring sent again.
pub fn set_objective_complete(world: &mut dyn SiegeWorld, team: i32, objective: i32, failed: bool) {
    let round = world.round();
    if round.team_status(team).is_none() {
        return;
    }
    if let Some(at) = round.status_at(team, objective) {
        if at < round.objectives.len() {
            round.objectives[at] = if failed { b'0' } else { b'1' };
        } else {
            // The status was the string's last character: the terminator is written.
            round.objectives.push(if failed { b'0' } else { b'1' });
        }
    }
    let objectives = round.objectives.clone();
    world.set_config_string(CS_SIEGE_OBJECTIVES, &objectives);
}

/// `SiegeCheckTimers` (`g_saga.c:931-1028`), every frame.
pub fn check_timers(world: &mut dyn SiegeWorld, level_time: i32, intermission: bool) {
    if intermission || world.round().ended {
        return;
    }
    let (mut team1, mut team2) = (0, 0);
    if !world.round().begun {
        for player in world.players().iter().filter(|player| player.connected) {
            if player.desired_team == i32::from(SIEGETEAM_TEAM1) {
                team1 += 1;
            } else if player.desired_team == i32::from(SIEGETEAM_TEAM2) {
                team2 += 1;
            }
        }
        let round = world.round();
        let beating = round.team_switch && round.persistent.beating_time;
        for index in 0..2 {
            round.countdown[index] = level_time
                + if beating {
                    round.persistent.last_time
                } else {
                    round.time_limit[index]
                };
        }
    }
    for (index, winner) in [(0, SIEGETEAM_TEAM2), (1, SIEGETEAM_TEAM1)] {
        let round = world.round();
        if round.time_limit[index] != 0 && round.countdown[index] < level_time {
            round_complete(world, i32::from(winner), ENTITYNUM_NONE, level_time);
            world.round().time_limit[index] = 0;
            return;
        }
    }
    let round = world.round();
    if !round.begun {
        if team1 == 0 || team2 == 0 {
            round.begin_time = level_time + SIEGE_ROUND_BEGIN_TIME;
            world.set_config_string(CS_SIEGE_STATE, b"1");
        } else if round.begin_time < level_time {
            round.begun = true;
            // `SiegeBeginRound(i)`: the loop's own counter, `MAX_CLIENTS`, is the entity the
            // begin target is fired from.
            begin_round(world, 32, level_time);
        } else if round.begin_time > level_time + SIEGE_ROUND_BEGIN_TIME {
            round.begin_time = level_time + SIEGE_ROUND_BEGIN_TIME;
        } else {
            let value = format!("2|{}", round.begin_time - SIEGE_ROUND_BEGIN_TIME);
            world.set_config_string(CS_SIEGE_STATE, value.as_bytes());
        }
    }
}

/// `SiegeBeginRound` (`g_saga.c:885-929`): with `preround_state` 0 everyone on a side — and
/// every spectator waiting for one — is respawned; the map's `roundbegin_target` is fired
/// from entity `activator`; the round is running.
pub fn begin_round(world: &mut dyn SiegeWorld, activator: i32, level_time: i32) {
    if world.round().map.preround_state == 0 {
        for player in world.players() {
            let respawn = (player.team != TEAM_SPECTATOR && !player.following)
                || (player.team == TEAM_SPECTATOR
                    && (player.desired_team == 1 || player.desired_team == 2));
            if respawn {
                world.siege_respawn(player.client);
            }
        }
    }
    if let Some(target) = world.round().map.value("roundbegin_target")
        && !target.is_empty()
    {
        world.use_targets(activator, &target);
    }
    world.set_config_string(CS_SIEGE_STATE, format!("0|{level_time}").as_bytes());
}

/// `SiegeObjectiveCompleted` (`g_saga.c:1030-1073`): the status set, the side's count raised
/// unless the objective does not count, and the round won when that was the last one
/// needed (or a final one) — else the completion broadcast and its points.
pub fn objective_completed(
    world: &mut dyn SiegeWorld,
    team: i32,
    objective: i32,
    final_flag: i32,
    client: i32,
    level_time: i32,
) {
    if world.round().ended {
        return;
    }
    set_objective_complete(world, team, objective, false);
    let side = SiegeRound::side_index(team);
    let round = world.round();
    if final_flag != -1 {
        round.completed[side] += 1;
    }
    if final_flag == 1 || round.completed[side] >= round.required[side] {
        round_complete(world, team, client, level_time);
    } else {
        // `BroadcastObjectiveCompletion`: points for a taker on the objective's side.
        if client != ENTITYNUM_NONE
            && world
                .players()
                .iter()
                .any(|player| player.client == client && player.team == team)
        {
            world.add_score(client, SIEGE_POINTS_OBJECTIVECOMPLETED);
        }
        world.broadcast(EV_SIEGE_OBJECTIVECOMPLETE, team, client, objective);
    }
}

/// `siegeTriggerUse` (`g_saga.c:1075-1145`) for an objective entity of `side` and number
/// `objective`. An objective not yet on the radar only goes onto it: returns `true` when it
/// did that (the caller sets `EF_RADAROBJECT`). `own_target` is the entity's own `target`
/// key. `user` is who used it: only a client activator is credited, and the targets are
/// used by the activator when it is a client, else by `other` (`UseSiegeTarget`), and not
/// at all without an activator.
pub fn trigger_use(
    world: &mut dyn SiegeWorld,
    side: i32,
    objective: i32,
    on_radar: bool,
    own_target: Option<&str>,
    user: SiegeUser,
    level_time: i32,
) -> bool {
    if !on_radar {
        return true;
    }
    let credited = if user.activator_is_client {
        user.activator.unwrap_or(ENTITYNUM_NONE)
    } else {
        ENTITYNUM_NONE
    };
    let Ok(Some(info)) = world.round().map.side(side).objective(objective) else {
        return false;
    };
    let by = user.activator.map(|activator| {
        if user.activator_is_client {
            activator
        } else {
            user.other
        }
    });
    if let (Some(target), Some(by)) = (&info.target, by) {
        world.use_targets(by, target);
    }
    if let (Some(target), Some(by)) = (own_target.filter(|target| !target.is_empty()), by) {
        world.use_targets(by, target);
    }
    objective_completed(
        world,
        side,
        objective,
        info.final_flag,
        credited,
        level_time,
    );
    false
}

/// `decompTriggerUse` (`g_saga.c:1245-1297`): a completed objective taken back — its status
/// cleared and, unless it did not count, its side's count lowered.
pub fn decomplete(world: &mut dyn SiegeWorld, side: i32, objective: i32) {
    let round = world.round();
    if round.ended || !round.completion_status(side, objective) {
        return;
    }
    set_objective_complete(world, side, objective, true);
    let round = world.round();
    let final_flag = round
        .map
        .side(side)
        .objective(objective)
        .ok()
        .flatten()
        .map_or(0, |info| info.final_flag);
    if final_flag != -1 {
        round.completed[SiegeRound::side_index(side)] -= 1;
    }
}

/// `SiegeRoundComplete` (`g_saga.c:612-712`): the round over — the broadcast, every winner's
/// points, `CS_SIEGE_STATE` ended, the side's `roundover_target` fired (or the level simply
/// exited), and with `g_siegeTeamSwitch` and a clock the time to beat kept.
pub fn round_complete(world: &mut dyn SiegeWorld, winner: i32, client: i32, level_time: i32) {
    let original = client;
    let players = world.players();
    let client = if client != ENTITYNUM_NONE
        && players
            .iter()
            .any(|player| player.client == client && player.team != winner)
    {
        ENTITYNUM_NONE
    } else {
        client
    };
    world.broadcast(EV_SIEGE_ROUNDOVER, winner, client, 0);
    // `AddSiegeWinningTeamPoints`.
    for player in players.iter().filter(|player| player.team == winner) {
        let points = if player.client == client {
            SIEGE_POINTS_TEAMWONROUND + SIEGE_POINTS_FINALOBJECTIVECOMPLETED
        } else {
            SIEGE_POINTS_TEAMWONROUND
        };
        world.add_score(player.client, points);
    }
    world.set_config_string(CS_SIEGE_STATE, format!("3|{level_time}").as_bytes());
    let round = world.round();
    round.begun = false;
    round.ended = true;
    round.winner = winner;
    let side = round.map.side(winner).clone();
    if side.group.is_some() {
        match side.value("roundover_target").ok().flatten() {
            None => {
                world.log_exit("Objectives completed");
                return;
            }
            Some(target) => {
                let from = if original == ENTITYNUM_NONE {
                    players
                        .first()
                        .map_or(ENTITYNUM_NONE, |player| player.client)
                } else {
                    original
                };
                world.use_targets(from, &target);
            }
        }
    }
    let round = world.round();
    if round.team_switch && (round.time_limit[0] != 0 || round.time_limit[1] != 0) {
        let time = if round.time_limit[0] != 0 {
            round.time_limit[0] - (round.countdown[0] - level_time)
        } else {
            round.time_limit[1] - (round.countdown[1] - level_time)
        };
        team_switch(world, winner, time.max(1));
    } else {
        round.persistent = SiegePersistent::default();
        world.set_persistent(SiegePersistent::default());
    }
}

/// `SiegeTeamSwitch` (`g_saga.c:596-610`): after the first round the winner and its time are
/// kept for the second; after the second the overall winner is published and the memory
/// cleared.
fn team_switch(world: &mut dyn SiegeWorld, winner: i32, time: i32) {
    let round = world.round();
    if round.persistent.beating_time {
        round.persistent = SiegePersistent::default();
        world.set_config_string(CS_SIEGE_WINTEAM, winner.to_string().as_bytes());
    } else {
        round.persistent = SiegePersistent {
            beating_time: true,
            last_team: winner,
            last_time: time,
        };
    }
    let persistent = world.round().persistent;
    world.set_persistent(persistent);
}

/// `ClientRespawn`'s siege branch (`g_client.c:1240-1270`) with `g_siegeRespawn` on: a
/// player not already waiting is made to wait — until at least `max(2 s × g_siegeRespawn,
/// 20 s)` from now — and comes back with the next wave.
pub fn respawn_wait(siege_respawn: i32, level_time: i32) -> i32 {
    level_time + (siege_respawn * 2_000).max(20_000)
}
