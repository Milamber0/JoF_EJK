//! The movers a map places (OpenJK `codemp/game/g_mover.c`): the doors, lifts and
//! platforms that slide between two places and carry or crush whoever is in the way.
//!
//! Ported here is the binary mover — at rest, opening, open, closing — that both a
//! `func_door` and a `func_plat` are, with the trigger each spawns for itself and the
//! trajectory every client lerps it along. The one struct covers both because the
//! reference itself does: `SP_func_plat` ends with `ent->parent = ent; // so it can be
//! treated as a door`, and a plat runs the same `Use_BinaryMover`, `Reached_BinaryMover`,
//! `ReturnToPos1` and `Blocked_Door` a door does. What differs — where the second place
//! is, the box of the trigger, and what a touch means — is [`MoverKind`].
//!
//! A mover is one of the few entities a client draws and predicts nothing of: the server
//! owns where it is, so what this module writes on the wire is what a player sees.

use sjk_entity::Entity;

/// `moverState_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoverState {
    /// At rest where the map put it.
    Pos1,
    /// At rest where it opens to.
    Pos2,
    /// On its way open.
    OneToTwo,
    /// On its way back.
    TwoToOne,
}

impl MoverState {
    /// What the transcripts print (`ent->moverState`).
    pub fn number(self) -> u32 {
        match self {
            Self::Pos1 => 0,
            Self::Pos2 => 1,
            Self::OneToTwo => 2,
            Self::TwoToOne => 3,
        }
    }
}

/// `ET_MOVER`.
pub const ET_MOVER: u32 = 6;
/// `SVF_USE_CURRENT_ORIGIN`, which every mover carries.
pub const SVF_USE_CURRENT_ORIGIN: u32 = 0x0080;
/// `SVF_PLAYER_USABLE`, which the use key looks for.
pub const SVF_PLAYER_USABLE: u32 = 0x0010;
/// `CONTENTS_SOLID`: a door is solid, which is the whole point of it.
pub const CONTENTS_SOLID: u32 = 1;
/// `FRAMETIME`.
pub const FRAMETIME: i32 = 100;
/// `TR_STATIONARY`, `TR_LINEAR_STOP` and `TR_NONLINEAR_STOP`, the three a mover uses.
pub const TR_STATIONARY: u32 = 0;
pub const TR_LINEAR_STOP: u32 = 3;
pub const TR_NONLINEAR_STOP: u32 = 4;
/// `MOVER_TOGGLE` (8): it waits to be used again instead of returning by itself.
pub(crate) const MOVER_TOGGLE: u32 = 8;
/// `MOVER_LOCKED`, `MOVER_PLAYER_USE` and `MOVER_INACTIVE` of `SP_func_door`.
const MOVER_START_OPEN: u32 = 1;
const MOVER_PLAYER_USE: u32 = 64;
/// `MOVER_FORCE_ACTIVATE` and `MOVER_LOCKED`: pushed or pulled open, and locked until used.
const MOVER_FORCE_ACTIVATE: u32 = 2;
pub(crate) const MOVER_LOCKED: u32 = 16;
const MOVER_INACTIVE: u32 = 128;

/// What a mover is waiting to do (`ent->think`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waiting {
    /// Nothing.
    None,
    /// `Think_SpawnNewDoorTrigger`, a frame after the map spawned it.
    SpawnTrigger,
    /// `Reached_BinaryMover`: it arrives.
    Reached,
    /// `ReturnToPos1`: it has waited long enough and goes back.
    Return,
    /// `Think_MatchTeam`: a door something else opens sets its team at rest, a frame
    /// after the map spawned it.
    MatchTeam,
    /// `Use_BinaryMover_Go`: used, it waits out its `delay` before it moves.
    Go,
}

impl Waiting {
    /// The think as the transcripts print it.
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::SpawnTrigger => "spawntrigger",
            Self::Reached => "reached",
            Self::Return => "return",
            Self::MatchTeam => "matchteam",
            Self::Go => "go",
        }
    }
}

/// Which classname a binary mover was spawned from: the two differ in where the second
/// place is, in the box of the trigger each spawns, and in what touching one means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoverKind {
    /// `func_door`: it slides along the map's `angle`, and its trigger reaches 120 units
    /// past it so walking up to one opens it.
    Door,
    /// `func_plat`: it drops to its low position at spawn and rises to where the map
    /// drew it, with a thin trigger in the middle of that low position.
    Plat,
    /// `func_button`: it slides a short way in when it is used or shot, waits, and comes
    /// back. It has no trigger of its own — the use key or a bullet reaches it.
    Button,
}

/// A binary mover as `SP_func_door` or `SP_func_plat` over `InitMover` leaves it.
#[derive(Clone, Debug, PartialEq)]
pub struct Door {
    /// Which classname it came from, and so which rules it runs by.
    pub kind: MoverKind,
    /// The inline model it is made of.
    pub model: usize,
    /// `r.mins`/`r.maxs` from that model.
    pub bounds: ([f32; 3], [f32; 3]),
    /// Where it rests, and where it opens to.
    pub pos1: [f32; 3],
    pub pos2: [f32; 3],
    /// The direction it travels, as `G_SetMovedir` reads the map's `angle`.
    pub movedir: [f32; 3],
    pub speed: f32,
    /// `wait` in milliseconds, as `SP_func_door` leaves it; below zero it stays open.
    pub wait: i32,
    /// `dmg`: what it does to whoever it cannot push out of the way.
    pub damage: i32,
    pub spawnflags: u32,
    pub contents: u32,
    pub svflags: u32,
    /// What it is called, and what it fires when it opens.
    pub targetname: String,
    pub target: String,
    pub state: MoverState,
    /// `s.pos.trDuration`: how long the travel takes.
    pub duration: i32,
    /// `s.pos.trTime` and `s.pos.trBase`/`trDelta`: the trajectory a client lerps.
    pub started: i32,
    pub base: [f32; 3],
    pub delta: [f32; 3],
    pub trajectory: u32,
    /// `s.time`, which a client reads to know when the mover last set off.
    pub time: i32,
    /// `nextthink` and `think`.
    pub next_think: i32,
    pub waiting: Waiting,
    /// `FL_INACTIVE`.
    pub inactive: bool,
    /// The map's `health` key (`G_SpawnInt( "health", "0", ... )`): a door with any is
    /// shot open instead of walked into.
    pub health: i32,
    /// The map's `team` key: the movers that share one move as one.
    pub team: String,
    /// `teammaster`: the index of the team's master among the movers, once
    /// [`find_teams`](crate::mover_team::find_teams) has chained them (`None` without a
    /// team).
    pub team_master: Option<usize>,
    /// `FL_TEAMSLAVE`: a part its master runs and is used through.
    pub team_slave: bool,
    /// A master's parts, itself first, in `teamchain` order (empty for any other mover).
    pub team_parts: Vec<usize>,
    /// `delay` in milliseconds: how long a use waits before the mover sets off. The
    /// spawn reads the key as a whole number of seconds (`F_INT`).
    pub delay: i32,
    /// Whether `SP_func_door` gave it `Think_SpawnNewDoorTrigger` rather than
    /// `Think_MatchTeam`, decided at spawn, before a team hands it another's name.
    pub own_trigger: bool,
    /// `ent->use` cleared: a door that stays open for good cannot be used again.
    pub spent: bool,
    /// What it fires when it is used open (`target2`), when it has opened
    /// (`opentarget`) and when it has closed (`closetarget`).
    pub target2: String,
    pub opentarget: String,
    pub closetarget: String,
    /// `activator`: the client whose use set it going, if a client's (the world, or the
    /// mover itself, otherwise).
    pub activator: Option<usize>,
    /// `soundSet`: the soundset its start, travel and stop sounds come from; empty for a
    /// silent mover (`G_PlayDoorSound` and `G_PlayDoorLoopSound` do nothing for it).
    pub sound_set: String,
    /// `s.loopSound` = `BMS_MID` with `loopIsSoundset`: its travel sound loops.
    pub looping: bool,
    /// The `EV_PLAYDOORSOUND`s raised on it (`BMS_START`, `BMS_END`) and not yet sent:
    /// the server takes them as it publishes the mover.
    pub sounds: Vec<u32>,
}

/// `BMS_START`, `BMS_END` (`g_mover.c:55-57`): a mover's start and stop sounds.
pub const BMS_START: u32 = 0;
pub const BMS_END: u32 = 2;

/// `G_PlayDoorSound` (`g_mover.c:88-98`): `sound` raised on a mover with a soundset.
pub fn play_sound(door: &mut Door, sound: u32) {
    if !door.sound_set.is_empty() {
        door.sounds.push(sound);
    }
}

/// `G_PlayDoorLoopSound` (`g_mover.c:65-82`): a mover with a soundset loops its travel
/// sound.
pub fn play_loop(door: &mut Door) {
    if !door.sound_set.is_empty() {
        door.looping = true;
    }
}

/// `G_SetMovedir` (`g_utils.c:687-701`): the map's `angle` as a direction, with its two
/// special values for straight up and straight down.
pub fn movedir(angles: [f32; 3]) -> [f32; 3] {
    if angles == [0.0, -1.0, 0.0] {
        return [0.0, 0.0, 1.0];
    }
    if angles == [0.0, -2.0, 0.0] {
        return [0.0, 0.0, -1.0];
    }
    let (yaw_sin, yaw_cos) = angles[1].to_radians().sin_cos();
    let (pitch_sin, pitch_cos) = (-angles[0]).to_radians().sin_cos();
    [pitch_cos * yaw_cos, pitch_cos * yaw_sin, pitch_sin]
}

/// `SP_func_door` (`g_mover.c:1414-1505`) with `InitMover` and `InitMoverTrData`
/// (`:966-1029`, `:943-964`): four hundred a second unless the map says otherwise, two
/// seconds open, a lip of eight taken off its own travel, two points of damage — and the
/// second place worked out from the model's own size along the direction it faces.
pub fn spawn_door(entity: &Entity, bounds: ([f32; 3], [f32; 3])) -> Option<Door> {
    if entity.get("classname") != Some("func_door") {
        return None;
    }
    let model = entity
        .get("model")
        .and_then(|name| name.strip_prefix('*'))
        .and_then(|index| index.parse().ok())?;
    let number = |key: &str, default: f32| {
        entity
            .get(key)
            .and_then(|text| text.trim().parse::<f32>().ok())
            .unwrap_or(default)
    };
    let origin = entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]);
    let angles = entity
        .vector("angles")
        .ok()
        .flatten()
        .unwrap_or_else(|| [0.0, number("angle", 0.0), 0.0]);
    let direction = movedir(angles);
    // `distance = DotProduct(|movedir|, size) - lip`, and the second place that far along.
    let size: [f32; 3] = std::array::from_fn(|axis| bounds.1[axis] - bounds.0[axis]);
    let lip = number("lip", 8.0);
    let distance = (0..3)
        .map(|axis| direction[axis].abs() * size[axis])
        .sum::<f32>()
        - lip;
    let opened: [f32; 3] = std::array::from_fn(|axis| origin[axis] + distance * direction[axis]);
    // START_OPEN swaps the two.
    let spawnflags = number("spawnflags", 0.0) as u32;
    let (pos1, pos2) = if spawnflags & MOVER_START_OPEN != 0 {
        (opened, origin)
    } else {
        (origin, opened)
    };
    let speed = match number("speed", 0.0) {
        0.0 => 400.0,
        speed => speed,
    };
    let wait = match number("wait", 0.0) {
        0.0 => 2.0,
        wait => wait,
    };
    let damage = number("dmg", 2.0).max(0.0) as i32;
    // `InitMoverTrData`: the travel's own length decides how long it takes.
    let travel: [f32; 3] = std::array::from_fn(|axis| pos2[axis] - pos1[axis]);
    let length = (travel[0] * travel[0] + travel[1] * travel[1] + travel[2] * travel[2]).sqrt();
    let duration = ((length * 1_000.0 / speed) as i32).max(1);
    let mut svflags = SVF_USE_CURRENT_ORIGIN;
    if spawnflags & MOVER_PLAYER_USE != 0 {
        svflags |= SVF_PLAYER_USABLE;
    }
    Some(Door {
        kind: MoverKind::Door,
        model,
        bounds,
        pos1,
        pos2,
        movedir: direction,
        speed,
        wait: (wait * 1_000.0) as i32,
        damage,
        spawnflags,
        contents: CONTENTS_SOLID,
        svflags,
        targetname: entity.get("targetname").unwrap_or_default().to_owned(),
        target: entity.get("target").unwrap_or_default().to_owned(),
        state: MoverState::Pos1,
        duration,
        started: 0,
        base: pos1,
        // `InitMoverTrData` leaves it still, with the travel scaled by the speed.
        delta: std::array::from_fn(|axis| travel[axis] * speed),
        trajectory: TR_STATIONARY,
        time: 0,
        next_think: FRAMETIME,
        waiting: if door_opens_by_trigger(entity, spawnflags) {
            Waiting::SpawnTrigger
        } else {
            Waiting::MatchTeam
        },
        inactive: spawnflags & MOVER_INACTIVE != 0,
        health: number("health", 0.0) as i32,
        team: entity.get("team").unwrap_or_default().to_owned(),
        team_master: None,
        team_slave: false,
        team_parts: Vec::new(),
        delay: crate::userinfo::atoi(entity.get("delay").unwrap_or_default().as_bytes()) * 1_000,
        own_trigger: door_opens_by_trigger(entity, spawnflags),
        spent: false,
        target2: entity.get("target2").unwrap_or_default().to_owned(),
        opentarget: entity.get("opentarget").unwrap_or_default().to_owned(),
        closetarget: entity.get("closetarget").unwrap_or_default().to_owned(),
        activator: None,
        sound_set: entity.get("soundSet").unwrap_or_default().to_owned(),
        looping: false,
        sounds: Vec::new(),
    })
}

/// `SP_func_plat` (`g_mover.c:1610-1651`) with `InitMover`/`InitMoverTrData`: a lift that
/// is **drawn at the top and spawned at the bottom** — the map's brush is `pos2`, and the
/// game drops the mover by its own height (less a lip of eight) to `pos1` before the
/// first frame. Two hundred a second unless the map says otherwise, two points of damage,
/// and a `wait` the reference overwrites with a flat second whatever the key said.
///
/// `VectorClear(ent->s.angles)` first: a plat ignores the map's `angle` entirely and
/// only ever travels straight up and down, so its `movedir` stays zero.
pub fn spawn_plat(entity: &Entity, bounds: ([f32; 3], [f32; 3])) -> Option<Door> {
    if entity.get("classname") != Some("func_plat") {
        return None;
    }
    let model = entity
        .get("model")
        .and_then(|name| name.strip_prefix('*'))
        .and_then(|index| index.parse().ok())?;
    let number = |key: &str| {
        entity
            .get(key)
            .and_then(|text| text.trim().parse::<f32>().ok())
    };
    let origin = entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]);
    // `G_SpawnFloat("speed", "200")`, and `InitMoverTrData`'s own hundred for a zero.
    let speed = match number("speed").unwrap_or(200.0) {
        0.0 => 100.0,
        speed => speed,
    };
    // `G_SpawnInt("dmg", "2")`, which — unlike a door's — the reference does not clamp.
    let damage = number("dmg").unwrap_or(2.0) as i32;
    let lip = number("lip").unwrap_or(8.0);
    // `G_SpawnFloat("height", "0", &height)` answers whether the key was there at all;
    // without one the plat travels its own height, less the lip.
    let height = number("height").unwrap_or_else(|| (bounds.1[2] - bounds.0[2]) - lip);
    // pos2 is where the map drew it; pos1 is that far below, and where it starts.
    let pos2 = origin;
    let pos1 = [origin[0], origin[1], origin[2] - height];
    let spawnflags = number("spawnflags").unwrap_or(0.0) as u32;
    let travel: [f32; 3] = std::array::from_fn(|axis| pos2[axis] - pos1[axis]);
    let length = (travel[0] * travel[0] + travel[1] * travel[1] + travel[2] * travel[2]).sqrt();
    let duration = ((length * 1_000.0 / speed) as i32).max(1);
    let mut svflags = SVF_USE_CURRENT_ORIGIN;
    if spawnflags & MOVER_PLAYER_USE != 0 {
        svflags |= SVF_PLAYER_USABLE;
    }
    Some(Door {
        kind: MoverKind::Plat,
        model,
        bounds,
        pos1,
        pos2,
        movedir: [0.0; 3],
        speed,
        // `ent->wait = 1000;`, straight over whatever `G_SpawnFloat("wait", "1")` read.
        wait: 1_000,
        damage,
        spawnflags,
        contents: CONTENTS_SOLID,
        svflags,
        targetname: entity.get("targetname").unwrap_or_default().to_owned(),
        target: entity.get("target").unwrap_or_default().to_owned(),
        state: MoverState::Pos1,
        duration,
        started: 0,
        base: pos1,
        delta: std::array::from_fn(|axis| travel[axis] * speed),
        trajectory: TR_STATIONARY,
        time: 0,
        // A plat spawns its trigger at once and has nothing to think about after.
        next_think: 0,
        waiting: Waiting::None,
        inactive: spawnflags & MOVER_INACTIVE != 0,
        health: number("health").unwrap_or(0.0) as i32,
        team: entity.get("team").unwrap_or_default().to_owned(),
        team_master: None,
        team_slave: false,
        team_parts: Vec::new(),
        // `SP_func_door` alone turns the key's seconds into milliseconds.
        delay: crate::userinfo::atoi(entity.get("delay").unwrap_or_default().as_bytes()),
        own_trigger: entity.get("targetname").is_none_or(str::is_empty),
        spent: false,
        target2: entity.get("target2").unwrap_or_default().to_owned(),
        opentarget: entity.get("opentarget").unwrap_or_default().to_owned(),
        closetarget: entity.get("closetarget").unwrap_or_default().to_owned(),
        activator: None,
        sound_set: entity.get("soundSet").unwrap_or_default().to_owned(),
        looping: false,
        sounds: Vec::new(),
    })
}

/// `SP_func_button` (`g_mover.c:1695-1760`) over `InitMover`: forty a second unless the
/// map says otherwise, a second before it comes back, a lip of **four** rather than a
/// door's eight, and it takes damage — which is how a shot presses a button — when the
/// map gave it health.
pub fn spawn_button(entity: &Entity, bounds: ([f32; 3], [f32; 3])) -> Option<Door> {
    if entity.get("classname") != Some("func_button") {
        return None;
    }
    let model = entity
        .get("model")
        .and_then(|name| name.strip_prefix('*'))
        .and_then(|index| index.parse().ok())?;
    let number = |key: &str, default: f32| {
        entity
            .get(key)
            .and_then(|text| text.trim().parse::<f32>().ok())
            .unwrap_or(default)
    };
    let origin = entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]);
    let angles = entity
        .vector("angles")
        .ok()
        .flatten()
        .unwrap_or_else(|| [0.0, number("angle", 0.0), 0.0]);
    let direction = movedir(angles);
    let size: [f32; 3] = std::array::from_fn(|axis| bounds.1[axis] - bounds.0[axis]);
    // A button's lip is four, where a door's is eight.
    let lip = number("lip", 4.0);
    let distance = (0..3)
        .map(|axis| direction[axis].abs() * size[axis])
        .sum::<f32>()
        - lip;
    let pos2: [f32; 3] = std::array::from_fn(|axis| origin[axis] + distance * direction[axis]);
    let speed = match number("speed", 0.0) {
        0.0 => 40.0,
        speed => speed,
    };
    let wait = match number("wait", 0.0) {
        0.0 => 1.0,
        wait => wait,
    };
    let travel: [f32; 3] = std::array::from_fn(|axis| pos2[axis] - origin[axis]);
    let length = (travel[0] * travel[0] + travel[1] * travel[1] + travel[2] * travel[2]).sqrt();
    let duration = ((length * 1_000.0 / speed) as i32).max(1);
    let spawnflags = number("spawnflags", 0.0) as u32;
    Some(Door {
        kind: MoverKind::Button,
        model,
        bounds,
        pos1: origin,
        pos2,
        movedir: direction,
        speed,
        wait: (wait * 1_000.0) as i32,
        damage: number("dmg", 2.0).max(0.0) as i32,
        spawnflags,
        contents: CONTENTS_SOLID,
        // A button is always reachable by the use key, whatever its spawnflags say.
        svflags: SVF_USE_CURRENT_ORIGIN | SVF_PLAYER_USABLE,
        targetname: entity.get("targetname").unwrap_or_default().to_owned(),
        target: entity.get("target").unwrap_or_default().to_owned(),
        state: MoverState::Pos1,
        duration,
        started: 0,
        base: origin,
        delta: std::array::from_fn(|axis| travel[axis] * speed),
        trajectory: TR_STATIONARY,
        time: 0,
        next_think: 0,
        waiting: Waiting::None,
        inactive: spawnflags & MOVER_INACTIVE != 0,
        health: number("health", 0.0) as i32,
        team: entity.get("team").unwrap_or_default().to_owned(),
        team_master: None,
        team_slave: false,
        team_parts: Vec::new(),
        // `SP_func_door` alone turns the key's seconds into milliseconds.
        delay: crate::userinfo::atoi(entity.get("delay").unwrap_or_default().as_bytes()),
        own_trigger: false,
        spent: false,
        target2: entity.get("target2").unwrap_or_default().to_owned(),
        opentarget: entity.get("opentarget").unwrap_or_default().to_owned(),
        closetarget: entity.get("closetarget").unwrap_or_default().to_owned(),
        activator: None,
        sound_set: entity.get("soundSet").unwrap_or_default().to_owned(),
        looping: false,
        sounds: Vec::new(),
    })
}

/// `SP_func_door`'s choice of think (`g_mover.c:1480-1502`): `Think_SpawnNewDoorTrigger`
/// when the door is locked, or when nothing else opens it — no `targetname`, no `health`,
/// neither `PLAYER_USE` nor `FORCE_ACTIVATE`. Any of those makes it `Think_MatchTeam`, a
/// door only its user, a shot or the Force opens.
fn door_opens_by_trigger(entity: &Entity, spawnflags: u32) -> bool {
    let named = entity
        .get("targetname")
        .is_some_and(|name| !name.is_empty());
    let health = entity
        .get("health")
        .is_some_and(|text| crate::userinfo::atoi(text.as_bytes()) != 0);
    let opened_otherwise =
        named || health || spawnflags & (MOVER_PLAYER_USE | MOVER_FORCE_ACTIVATE) != 0;
    spawnflags & MOVER_LOCKED != 0 || !opened_otherwise
}

/// Whether the mover has a trigger of its own: a door as [`Door::own_trigger`] records
/// `SP_func_door`'s choice — and only a team's master, since a slave's think never runs —
/// and a plat unless the map named it (`if (!ent->targetname) SpawnPlatTrigger(ent)`,
/// `:1648-1650`). A button is reached by the use key or a shot, never by a trigger.
pub fn spawns_own_trigger(mover: &Door) -> bool {
    mover.own_trigger && !mover.team_slave
}

/// The box the mover watches for company — [`door_trigger_bounds`] or
/// [`plat_trigger_bounds`], by kind.
pub fn trigger_bounds(mover: &Door) -> ([f32; 3], [f32; 3]) {
    match mover.kind {
        MoverKind::Door | MoverKind::Button => door_trigger_bounds(mover),
        MoverKind::Plat => plat_trigger_bounds(mover),
    }
}

/// `SpawnPlatTrigger` (`:1561-1593`): a thin trigger in the middle of the plat's **low**
/// position — the plat's own box there, pulled in by thirty-three on both horizontal
/// axes and reaching eight above its top, so that stepping onto a lowered plat calls it
/// up. A plat too narrow to pull in that far gets a one-unit sliver down its middle
/// instead; the reference's own comment says an elevator car needs the trigger to run
/// through the whole low position, not just sit on top of it.
pub fn plat_trigger_bounds(plat: &Door) -> ([f32; 3], [f32; 3]) {
    let mut low: [f32; 3] = std::array::from_fn(|axis| plat.pos1[axis] + plat.bounds.0[axis]);
    let mut high: [f32; 3] = std::array::from_fn(|axis| plat.pos1[axis] + plat.bounds.1[axis]);
    for axis in 0..2 {
        low[axis] += 33.0;
        high[axis] -= 33.0;
    }
    high[2] += 8.0;
    for axis in 0..2 {
        if high[axis] <= low[axis] {
            low[axis] = plat.pos1[axis] + (plat.bounds.0[axis] + plat.bounds.1[axis]) * 0.5;
            high[axis] = low[axis] + 1.0;
        }
    }
    (low, high)
}

/// `Think_SpawnNewDoorTrigger` (`:1202-1245`) for a door with no team: its box at rest,
/// a unit out and grown by a hundred and twenty along its thinnest axis, which is why
/// standing anywhere near a door holds it open. A team's is
/// [`team_trigger_bounds`](crate::mover_team::team_trigger_bounds).
pub fn door_trigger_bounds(door: &Door) -> ([f32; 3], [f32; 3]) {
    let (low, high, _) =
        crate::mover_team::team_trigger_bounds(std::slice::from_ref(&alone(door)), 0);
    (low, high)
}

/// The mover as a team of one, for the single-mover forms below.
fn alone(door: &Door) -> Door {
    Door {
        team_master: None,
        team_slave: false,
        team_parts: Vec::new(),
        ..door.clone()
    }
}

/// `SetMoverState` (`:565-619`): where the mover is, and the trajectory every client
/// draws it along — still at either end, and a curve (`TR_NONLINEAR_STOP`, which eases
/// in and out) between them.
pub fn set_state(door: &mut Door, state: MoverState, time: i32) {
    door.state = state;
    door.started = time;
    if door.duration <= 0 {
        door.duration = 1;
    }
    let (from, to) = match state {
        MoverState::Pos1 | MoverState::TwoToOne => (door.pos1, door.pos2),
        MoverState::Pos2 | MoverState::OneToTwo => (door.pos2, door.pos1),
    };
    match state {
        // At either end `SetMoverState` leaves `trDelta` exactly as the last travel set
        // it: `TR_STATIONARY` never reads it, but it stays on the wire.
        MoverState::Pos1 => {
            door.base = door.pos1;
            door.trajectory = TR_STATIONARY;
        }
        MoverState::Pos2 => {
            door.base = door.pos2;
            door.trajectory = TR_STATIONARY;
        }
        MoverState::OneToTwo | MoverState::TwoToOne => {
            let (base, target) = if state == MoverState::OneToTwo {
                (door.pos1, door.pos2)
            } else {
                (door.pos2, door.pos1)
            };
            door.base = base;
            let factor = 1_000.0 / door.duration as f32;
            door.delta = std::array::from_fn(|axis| (target[axis] - base[axis]) * factor);
            door.trajectory = TR_NONLINEAR_STOP;
        }
    }
    let _ = (from, to);
}

/// `Use_BinaryMover` (`:895-932`) for a mover with no team: see
/// [`use_mover`](crate::mover_team::use_mover). Returns what it fires if it sets off.
pub fn used(door: &mut Door, level_time: i32) -> Option<String> {
    crate::mover_team::use_mover(std::slice::from_mut(door), 0, level_time, None)
}

/// A living player is standing in the mover's own trigger: [`touch_door_trigger`] or
/// [`touch_plat_center`], by kind. Returns what the mover fires if it starts moving.
pub fn touched(mover: &mut Door, level_time: i32) -> Option<String> {
    match mover.kind {
        MoverKind::Door | MoverKind::Button => touch_door_trigger(mover, level_time),
        MoverKind::Plat => touch_plat_center(mover, level_time),
    }
}

/// `Touch_PlatCenterTrigger` (`:1541-1549`): a plat sitting at the bottom is called up,
/// and one that is anywhere else is left alone — the trigger has nothing to say about a
/// plat already at the top, which is [`touch_plat`]'s business.
pub fn touch_plat_center(plat: &mut Door, level_time: i32) -> Option<String> {
    // `Use_BinaryMover`'s own gate, which `Touch_PlatCenterTrigger` goes through.
    if plat.inactive {
        return None;
    }
    (plat.state == MoverState::Pos1)
        .then(|| used(plat, level_time))
        .flatten()
}

/// `Touch_Plat` (`:1523-1532`): the plat's own touch, which a player reaches by standing
/// on it or walking into it (`PM_GroundTrace`/`PM_SlideMove` add it to the move's touched
/// entities, and `ClientImpacts` calls this). A living player on a raised plat pushes its
/// return a second further out, which is what keeps a lift up while somebody rides it.
/// A corpse does not.
pub fn touch_plat(plat: &mut Door, health: i32, level_time: i32) {
    if health <= 0 {
        return;
    }
    if plat.state == MoverState::Pos2 {
        plat.next_think = level_time + 1_000;
    }
}

/// `Touch_DoorTrigger` (`:1112-1190`) for a living player at a door with no team: see
/// [`touch_trigger`](crate::mover_team::touch_trigger).
pub fn touch_door_trigger(door: &mut Door, level_time: i32) -> Option<String> {
    crate::mover_team::touch_trigger(std::slice::from_mut(door), 0, level_time, None)
}

/// `Reached_BinaryMover` (`:664-730`) for one part: opening, it waits its `wait` and
/// then goes back (or stays for good below zero, and can never be used again); closing,
/// it is simply at rest, its think left as it was. Returns what it fires: `opentarget`
/// or `closetarget`.
pub fn reached(door: &mut Door, level_time: i32) -> Option<String> {
    // The travel sound stops whatever state it arrives in.
    door.looping = false;
    let fired = match door.state {
        MoverState::OneToTwo => &door.opentarget,
        MoverState::TwoToOne => &door.closetarget,
        MoverState::Pos1 | MoverState::Pos2 => return None,
    }
    .clone();
    match door.state {
        MoverState::OneToTwo => {
            set_state(door, MoverState::Pos2, level_time);
            play_sound(door, BMS_END);
            if door.wait < 0 {
                door.waiting = Waiting::None;
                door.next_think = 0;
                door.spent = true;
            } else {
                door.waiting = Waiting::Return;
                door.next_think = if door.spawnflags & MOVER_TOGGLE != 0 {
                    -1
                } else {
                    level_time + door.wait
                };
            }
        }
        MoverState::TwoToOne => {
            set_state(door, MoverState::Pos1, level_time);
            play_sound(door, BMS_END);
        }
        _ => {}
    }
    (!fired.is_empty()).then_some(fired)
}

/// `ReturnToPos1` (`:640-652`) for a mover with no team: see
/// [`return_to_pos1`](crate::mover_team::return_to_pos1).
pub fn return_to_rest(door: &mut Door, level_time: i32) {
    crate::mover_team::return_to_pos1(std::slice::from_mut(door), 0, level_time);
}

/// `G_RunMover` (`:509-523`) for a mover with no team: see
/// [`run`](crate::mover_team::run). Returns whether anything happened, so the caller
/// can publish its state.
pub fn run(door: &mut Door, level_time: i32) -> bool {
    crate::mover_team::run(std::slice::from_mut(door), 0, level_time).changed
}

/// Where the mover stands at `level_time`, as `BG_EvaluateTrajectory` puts it
/// (`bg_misc.c:2255-2269`): still at either end, and along the way
///
/// ```text
/// deltaTime = trDuration * 0.001 * cos( 90° - 90° * (atTime - trTime) / trDuration )
/// ```
///
/// which is the slow-down at the end the reference's own comment names — a door eases
/// into its stop instead of arriving at speed.
pub fn origin_at(door: &Door, level_time: i32) -> [f32; 3] {
    if door.trajectory == TR_STATIONARY {
        return door.base;
    }
    let elapsed = (level_time - door.started).min(door.duration);
    if elapsed > door.duration || elapsed <= 0 {
        return door.base;
    }
    let degrees = 90.0 - (90.0 * elapsed as f32) / door.duration as f32;
    let moved = door.duration as f32 * 0.001 * degrees.to_radians().cos();
    std::array::from_fn(|axis| door.base[axis] + door.delta[axis] * moved)
}

/// `MOD_CRUSH`, which a mover kills with.
pub const MOD_CRUSH: u32 = 36;
/// `MOVER_CRUSHER` (64 on a door's own spawnflags in the reference's mover set): it does
/// not reverse for whoever is in the way.
pub(crate) const MOVER_CRUSHER: u32 = 256;

/// One entity the mover is carrying or shoving along.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shoved {
    /// Whose it is.
    pub client: u16,
    /// Where the mover puts it.
    pub to: [f32; 3],
    /// Whether it was standing on the mover (its ground is kept; otherwise it is in the
    /// air as far as the move is concerned).
    pub carried: bool,
}

/// What `G_MoverPush` made of a mover's travel (`:178-270`, `:275-420`).
#[derive(Clone, Debug, PartialEq)]
pub enum Push {
    /// Everybody in the way went along with it.
    Moved(Vec<Shoved>),
    /// Somebody could not be moved out of the way; the mover is blocked by them.
    Blocked(u16),
}

/// `G_MoverPush` with `G_TryPushingEntity` for a mover that slides (no rotation, which
/// is `func_rotating`'s): everything whose box the mover's new place would take is moved
/// by the same amount, and one that cannot go anywhere blocks it. `free` answers whether
/// that client's box may stand somewhere, as `G_TestEntityPosition` does — the client's
/// own number is handed in because its trace passes itself.
///
/// A mover whose model is its box (the whole-game oracle's): the box overlap is the whole
/// test. [`push_through`] is the same for a real brush.
pub fn push(
    door: &Door,
    from: [f32; 3],
    to: [f32; 3],
    candidates: &[(u16, [f32; 3], ([f32; 3], [f32; 3]), u16)],
    free: &mut dyn FnMut(u16, [f32; 3], ([f32; 3], [f32; 3])) -> bool,
    mover_number: u16,
) -> Push {
    push_through(
        door,
        from,
        to,
        candidates,
        free,
        &mut |_, _, _| true,
        mover_number,
    )
}

/// [`push`] for a mover made of the map's brushes. A client not riding the mover is in its
/// way only if its box overlaps the mover's box at its new place *and* its box, where it
/// stands, is inside the mover's brush there — `G_MoverPush`'s `G_TestEntityPosition`
/// (`g_mover.c`), which `inside` answers. The box alone is the model's spread by a pixel
/// (`CM_ModelBounds`), so a player flush against a door's face overlaps it without being
/// in its way.
pub fn push_through(
    door: &Door,
    from: [f32; 3],
    to: [f32; 3],
    candidates: &[(u16, [f32; 3], ([f32; 3], [f32; 3]), u16)],
    free: &mut dyn FnMut(u16, [f32; 3], ([f32; 3], [f32; 3])) -> bool,
    inside: &mut dyn FnMut(u16, [f32; 3], ([f32; 3], [f32; 3])) -> bool,
    mover_number: u16,
) -> Push {
    push_box_through(
        door.bounds,
        from,
        to,
        candidates,
        free,
        inside,
        mover_number,
    )
}

/// [`push_through`] for any brush mover by its box alone (`r.mins`, `r.maxs` at origin
/// zero): a scripted `func_static` pushes as a door does.
pub fn push_box_through(
    mover_bounds: ([f32; 3], [f32; 3]),
    from: [f32; 3],
    to: [f32; 3],
    candidates: &[(u16, [f32; 3], ([f32; 3], [f32; 3]), u16)],
    free: &mut dyn FnMut(u16, [f32; 3], ([f32; 3], [f32; 3])) -> bool,
    inside: &mut dyn FnMut(u16, [f32; 3], ([f32; 3], [f32; 3])) -> bool,
    mover_number: u16,
) -> Push {
    let move_by: [f32; 3] = std::array::from_fn(|axis| to[axis] - from[axis]);
    let mut shoved = Vec::new();
    for (client, origin, bounds, ground) in candidates {
        // The mover's box where it is going (`r.absmin`/`r.absmax` about `currentOrigin`,
        // which is `to`: a brush model's bounds are where the map drew it, at origin
        // zero), against this one where it stands. Not about `pos1`: a START_OPEN door or
        // a plat rests away from where it was drawn.
        let low: [f32; 3] = std::array::from_fn(|axis| mover_bounds.0[axis] + to[axis]);
        let high: [f32; 3] = std::array::from_fn(|axis| mover_bounds.1[axis] + to[axis]);
        let standing_on = *ground == mover_number;
        let in_the_way = (0..3).all(|axis| {
            origin[axis] + bounds.0[axis] < high[axis] && origin[axis] + bounds.1[axis] > low[axis]
        });
        if !standing_on && (!in_the_way || !inside(*client, *origin, *bounds)) {
            continue;
        }
        let moved: [f32; 3] = std::array::from_fn(|axis| origin[axis] + move_by[axis]);
        if !free(*client, moved, *bounds) {
            return Push::Blocked(*client);
        }
        shoved.push(Shoved {
            client: *client,
            to: moved,
            carried: standing_on,
        });
    }
    Push::Moved(shoved)
}

/// `Blocked_Door` (`:1048-1065`) for a mover with no team, blocked in the frame from
/// `previous_time` to `level_time`: see [`blocked`](crate::mover_team::blocked). Returns
/// the damage to deal, if any.
pub fn blocked(door: &mut Door, level_time: i32, previous_time: i32) -> Option<i32> {
    crate::mover_team::blocked(
        std::slice::from_mut(door),
        0,
        level_time,
        previous_time,
        None,
    )
}
