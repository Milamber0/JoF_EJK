//! Mover teams and using a mover, as `g_mover.c` has them: `G_FindTeams` chains the
//! movers that share a `team` key, `MatchTeam` moves every part of a team at once, and
//! `Use_BinaryMover` (a trigger, a relay, a button, a shot) opens, holds, closes or turns
//! back the whole team through its master.
//!
//! A team lives in the movers themselves: the master lists its parts (itself first, then
//! the others in `teamchain`'s order), and every part names its master. A mover without a
//! team is a team of one.
//!
//! Held to `tools/game-oracle/doorteam.c` (`game-doorteam.txt`).

use crate::movers::{
    BMS_START, Door, FRAMETIME, MOVER_CRUSHER, MOVER_LOCKED, MOVER_TOGGLE, MoverState,
    TR_LINEAR_STOP, TR_NONLINEAR_STOP, TR_STATIONARY, Waiting, origin_at, reached, set_state,
};

/// Something a mover is kept in: the mover itself, or the mover with the entity numbers
/// a server or a replay keeps beside it.
pub trait DoorSlot {
    /// The mover.
    fn door(&self) -> &Door;
    /// The mover, to change.
    fn door_mut(&mut self) -> &mut Door;
}

impl DoorSlot for Door {
    fn door(&self) -> &Door {
        self
    }
    fn door_mut(&mut self) -> &mut Door {
        self
    }
}

impl<T> DoorSlot for (T, Door) {
    fn door(&self) -> &Door {
        &self.1
    }
    fn door_mut(&mut self) -> &mut Door {
        &mut self.1
    }
}

impl<T, U> DoorSlot for (T, Door, U) {
    fn door(&self) -> &Door {
        &self.1
    }
    fn door_mut(&mut self) -> &mut Door {
        &mut self.1
    }
}

/// `G_FindTeams` (`g_main.c:83-128`) over the movers, in entity order: the first of each
/// `team` is its master, every later one a slave (`FL_TEAMSLAVE`) chained in behind it,
/// and a slave's `targetname` moves to the master so that only the master is ever used
/// by name.
pub fn find_teams(doors: &mut [impl DoorSlot]) {
    for master in 0..doors.len() {
        let door = doors[master].door();
        if door.team.is_empty() || door.team_slave {
            continue;
        }
        let team = door.team.clone();
        doors[master].door_mut().team_master = Some(master);
        doors[master].door_mut().team_parts = vec![master];
        for other in master + 1..doors.len() {
            let part = doors[other].door();
            if part.team.is_empty() || part.team_slave || part.team != team {
                continue;
            }
            // `e2->teamchain = e->teamchain; e->teamchain = e2;`: each new part goes
            // straight behind the master.
            doors[master].door_mut().team_parts.insert(1, other);
            let part = doors[other].door_mut();
            part.team_master = Some(master);
            part.team_slave = true;
            if !part.targetname.is_empty() {
                let name = std::mem::take(&mut part.targetname);
                doors[master].door_mut().targetname = name;
            }
        }
    }
}

/// The parts of the team `leader` leads: `teamchain` from it, which is the leader alone
/// for a mover with no team.
fn part(doors: &[impl DoorSlot], leader: usize, at: usize) -> Option<usize> {
    let parts = &doors[leader].door().team_parts;
    if parts.is_empty() {
        (at == 0).then_some(leader)
    } else {
        parts.get(at).copied()
    }
}

/// `MatchTeam` (`g_mover.c:630-636`): every part set going the same way at the same
/// time, each over its own duration.
pub fn match_team(doors: &mut [impl DoorSlot], leader: usize, state: MoverState, time: i32) {
    let mut at = 0;
    while let Some(index) = part(doors, leader, at) {
        set_state(doors[index].door_mut(), state, time);
        at += 1;
    }
}

/// `UnLockDoors` (`:858-874`): the whole team unlocked, and — unless it toggles — its
/// name taken away, so it can never be used by name again.
fn unlock(doors: &mut [impl DoorSlot], leader: usize) {
    let mut at = 0;
    while let Some(index) = part(doors, leader, at) {
        let door = doors[index].door_mut();
        if door.spawnflags & MOVER_TOGGLE == 0 {
            door.targetname.clear();
        }
        door.spawnflags &= !MOVER_LOCKED;
        at += 1;
    }
}

/// `Use_BinaryMover` (`:895-932`) by `activator` (a client, or `None` for the world):
/// a slave hands the use to its master; an inactive mover ignores it; a locked one
/// unlocks; the rest remember who used them and, after any `delay`, go. Returns what the
/// mover fires: its `target` setting off from rest, its `target2` used open.
pub fn use_mover(
    doors: &mut [impl DoorSlot],
    index: usize,
    level_time: i32,
    activator: Option<usize>,
) -> Option<String> {
    let door = doors[index].door();
    // `ent->use` is gone once a door that never closes has opened.
    if door.spent {
        return None;
    }
    if door.team_slave
        && let Some(master) = door.team_master
    {
        return use_mover(doors, master, level_time, activator);
    }
    if door.inactive {
        return None;
    }
    if door.spawnflags & MOVER_LOCKED != 0 {
        unlock(doors, index);
        return None;
    }
    doors[index].door_mut().activator = activator;
    let door = doors[index].door();
    if door.delay != 0 {
        let door = doors[index].door_mut();
        door.waiting = Waiting::Go;
        door.next_think = level_time + door.delay;
        return None;
    }
    go(doors, index, level_time)
}

/// `Use_BinaryMover_Go` (`:740-855`): at rest the team sets off fifty milliseconds on
/// and the mover fires its `target`; open, its return is set and it fires its `target2`;
/// on its way, the team turns back, keeping as much of the way as it has already come.
pub fn go(doors: &mut [impl DoorSlot], index: usize, level_time: i32) -> Option<String> {
    let door = doors[index].door();
    match door.state {
        MoverState::Pos1 => {
            match_team(doors, index, MoverState::OneToTwo, level_time + 50);
            let door = doors[index].door_mut();
            crate::movers::play_loop(door);
            crate::movers::play_sound(door, BMS_START);
            door.time = level_time;
            Some(door.target.clone())
        }
        MoverState::Pos2 => {
            let door = doors[index].door_mut();
            door.waiting = Waiting::Return;
            door.next_think = level_time
                + if door.spawnflags & MOVER_TOGGLE != 0 {
                    FRAMETIME
                } else {
                    door.wait
                };
            (!door.target2.is_empty()).then(|| door.target2.clone())
        }
        MoverState::TwoToOne => {
            let started = level_time - left_to_go(door, door.pos1, level_time);
            match_team(doors, index, MoverState::OneToTwo, started);
            crate::movers::play_sound(doors[index].door_mut(), BMS_START);
            None
        }
        MoverState::OneToTwo => {
            let started = level_time - left_to_go(door, door.pos2, level_time);
            match_team(doors, index, MoverState::TwoToOne, started);
            crate::movers::play_sound(doors[index].door_mut(), BMS_START);
            None
        }
    }
}

/// `total - partial` of `Use_BinaryMover_Go`'s reversal: how much of the way back is
/// still to come, from how far the mover is from `from` (the end it is heading away
/// from after the turn) and the arc its eased trajectory follows.
fn left_to_go(door: &Door, from: [f32; 3], level_time: i32) -> i32 {
    let (total, partial) = if door.trajectory == TR_NONLINEAR_STOP {
        let total = door.duration - 50;
        let current = origin_at(door, level_time);
        let length = |v: [f32; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        let mut partial =
            length(std::array::from_fn(|axis| current[axis] - from[axis])) / length(door.delta);
        partial /= door.duration as f32;
        partial /= 0.001;
        partial = (f64::from(partial).acos()) as f32;
        partial *= 57.295_78_f32;
        partial = (90.0 - partial) / 90.0 * door.duration as f32;
        (
            total,
            (f64::from(total) - f64::from(partial).floor()) as i32,
        )
    } else {
        (door.duration, level_time - door.started)
    };
    total - partial.min(total)
}

/// `ReturnToPos1` (`:640-652`): the wait is over and the team goes back.
pub fn return_to_pos1(doors: &mut [impl DoorSlot], index: usize, level_time: i32) {
    let door = doors[index].door_mut();
    door.waiting = Waiting::None;
    door.next_think = 0;
    door.time = level_time;
    match_team(doors, index, MoverState::TwoToOne, level_time);
    let door = doors[index].door_mut();
    crate::movers::play_loop(door);
    crate::movers::play_sound(door, BMS_START);
}

/// What one frame did to a team: whether any part changed, and what it fired — a
/// delayed use's targets, and each part's `opentarget` or `closetarget` as it arrived —
/// each with the client that set the mover going, if one did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ran {
    pub changed: bool,
    pub fired: Vec<(String, Option<usize>)>,
}

/// `G_RunMover` with `G_MoverTeam` (`:445-523`) for the mover at `index`: a slave does
/// nothing (its master runs it); a master that is going somewhere lets every part whose
/// travel is spent arrive, and then its own think runs if it has come due. A slave's own
/// think never runs.
pub fn run(doors: &mut [impl DoorSlot], index: usize, level_time: i32) -> Ran {
    let mut ran = Ran::default();
    if doors[index].door().team_slave {
        return ran;
    }
    if doors[index].door().trajectory != TR_STATIONARY {
        let mut at = 0;
        while let Some(part) = part(doors, index, at) {
            let door = doors[part].door_mut();
            let stops = door.trajectory == TR_LINEAR_STOP || door.trajectory == TR_NONLINEAR_STOP;
            if stops && level_time >= door.started + door.duration {
                if let Some(target) = reached(door, level_time) {
                    ran.fired.push((target, door.activator));
                }
                ran.changed = true;
            }
            at += 1;
        }
    }
    // `G_RunThink`: `think` stays as it was; only `nextthink` is spent.
    let door = doors[index].door_mut();
    if door.next_think > 0 && door.next_think <= level_time {
        door.next_think = 0;
        ran.changed = true;
        match door.waiting {
            Waiting::SpawnTrigger | Waiting::MatchTeam => {
                let state = door.state;
                match_team(doors, index, state, level_time);
            }
            Waiting::Return => return_to_pos1(doors, index, level_time),
            Waiting::Go => {
                if let Some(target) = go(doors, index, level_time) {
                    ran.fired.push((target, doors[index].door().activator));
                }
            }
            Waiting::Reached | Waiting::None => {}
        }
    }
    ran
}

/// `Touch_DoorTrigger` (`:1112-1190`) for a living player in the trigger of the team
/// `master` leads: a locked door does not even try, and one already opening is left
/// alone; anything else is used (which holds an open door open, and turns a closing
/// one back).
pub fn touch_trigger(
    doors: &mut [impl DoorSlot],
    master: usize,
    level_time: i32,
    toucher: Option<usize>,
) -> Option<String> {
    let door = doors[master].door();
    if door.spawnflags & MOVER_LOCKED != 0 || door.state == MoverState::OneToTwo {
        return None;
    }
    use_mover(doors, master, level_time, toucher)
}

/// `Think_SpawnNewDoorTrigger` (`:1202-1245`): the box around every part of the team as
/// each stood at rest (`r.absmin`/`r.absmax`, a unit out), grown by a hundred and twenty
/// both ways along its thinnest axis, which is returned too (the trigger's `count`).
pub fn team_trigger_bounds(doors: &[impl DoorSlot], master: usize) -> ([f32; 3], [f32; 3], usize) {
    let mut low = [f32::MAX; 3];
    let mut high = [f32::MIN; 3];
    let mut at = 0;
    while let Some(index) = part(doors, master, at) {
        let door = doors[index].door();
        for axis in 0..3 {
            low[axis] = low[axis].min(door.pos1[axis] + door.bounds.0[axis] - 1.0);
            high[axis] = high[axis].max(door.pos1[axis] + door.bounds.1[axis] + 1.0);
        }
        at += 1;
    }
    let mut best = 0;
    for axis in 1..3 {
        if high[axis] - low[axis] < high[best] - low[best] {
            best = axis;
        }
    }
    high[best] += 120.0;
    low[best] -= 120.0;
    (low, high, best)
}

/// `Touch_DoorTriggerSpectator` (`g_mover.c:1074-1104`): where a spectator in a door
/// trigger (`low`/`high`, grown along `axis`) that is neither open nor opening is put — past
/// the door on the far side from where it stands, twenty-five units beyond the door's own
/// span — or `None` outside that span. The caller moves it there only if a player's box
/// fits (`TeleportPlayer`, keeping its angles and speed).
pub fn spectator_passage(
    low: [f32; 3],
    high: [f32; 3],
    axis: usize,
    origin: [f32; 3],
) -> Option<[f32; 3]> {
    // The trigger's `r.absmin`/`r.absmax` are a unit out; the door lies 100 in from them.
    let door_min = (low[axis] - 1.0) + 100.0;
    let door_max = (high[axis] + 1.0) - 100.0;
    if origin[axis] < door_min || origin[axis] > door_max {
        return None;
    }
    let mut to = origin;
    to[axis] = if (origin[axis] - door_max).abs() < (origin[axis] - door_min).abs() {
        door_min - 25.0
    } else {
        door_max + 25.0
    };
    Some(to)
}

/// `Blocked_Door` (`:1048-1065`) after `G_MoverTeam` found the team `master` leads
/// blocked: every part is put back where it was a frame ago (its trajectory's clock
/// pushed on by the frame), whoever was in the way takes the master's damage, and — unless
/// it crushes — the team is used, which turns it back. A locked team is used all the same
/// (which unlocks it) and locked again after (`LockDoors`).
pub fn blocked(
    doors: &mut [impl DoorSlot],
    master: usize,
    level_time: i32,
    previous_time: i32,
    blocker: Option<usize>,
) -> Option<i32> {
    let mut at = 0;
    while let Some(index) = part(doors, master, at) {
        doors[index].door_mut().started += level_time - previous_time;
        at += 1;
    }
    let door = doors[master].door();
    let damage = (door.damage > 0).then_some(door.damage);
    if door.spawnflags & MOVER_CRUSHER != 0 {
        return damage;
    }
    let relock = door.spawnflags & MOVER_LOCKED != 0;
    // Only a mover on its way is blocked, and turning back fires nothing.
    let _ = use_mover(doors, master, level_time, blocker);
    if relock {
        let mut at = 0;
        while let Some(index) = part(doors, master, at) {
            doors[index].door_mut().spawnflags |= MOVER_LOCKED;
            at += 1;
        }
    }
    damage
}
