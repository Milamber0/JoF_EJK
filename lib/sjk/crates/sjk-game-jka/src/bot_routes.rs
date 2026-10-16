//! The bots' routes (OpenJK `codemp/game/ai_wpnav.c`): the waypoint trail a level's
//! route file holds (`botroutes/<map>.wnt`, `LoadPathData`), read when a level starts
//! with `bot_enable` (`G_InitBots` → `LoadPath_ThisLevel`), and what the game works out
//! from it at once: the items worth going for (`CalculateWeightGoals`), the siege
//! objectives (`CalculateSiegeGoals`) and the jump points (`CalculateJumpRoutes`).
//!
//! The bots find their way along these waypoints, never botlib's areas.

use crate::items::{Item, Kind};
use crate::text_parse::atof;
use crate::userinfo::atoi;

/// `MAX_WPARRAY_SIZE`: waypoints past this many are dropped as the file is read.
pub const MAX_WAYPOINTS: usize = 4096;
/// `MAX_NEIGHBOR_SIZE`: a waypoint's neighbours past this many are read and dropped.
pub const MAX_NEIGHBOURS: usize = 32;
/// The largest route file the game reads (its buffer, 512 KiB, less one).
pub const MAX_ROUTE_BYTES: usize = 524_288;

/// `WPFLAG_JUMP`: jump on reaching the waypoint.
pub const WPFLAG_JUMP: i32 = 0x10;
/// `WPFLAG_GOALPOINT`: somewhere worth going, by its weight.
pub const WPFLAG_GOALPOINT: i32 = 0x1_0000;
/// `WPFLAG_RED_FLAG`: the red flag's waypoint.
pub const WPFLAG_RED_FLAG: i32 = 0x2_0000;
/// `WPFLAG_BLUE_FLAG`: the blue flag's waypoint.
pub const WPFLAG_BLUE_FLAG: i32 = 0x4_0000;
/// `WPFLAG_SIEGE_REBELOBJ`: near a rebel (team 2) objective.
pub const WPFLAG_SIEGE_REBELOBJ: i32 = 0x8_0000;
/// `WPFLAG_SIEGE_IMPERIALOBJ`: near an imperial (team 1) objective.
pub const WPFLAG_SIEGE_IMPERIALOBJ: i32 = 0x10_0000;

/// `ENTITYNUM_NONE`: a waypoint tied to no entity.
const NO_ENTITY: i32 = 1023;
/// What the reader makes of any Force-jump level a neighbour names (the file's number is
/// ignored: `FJSR`).
const FORCE_JUMP: i32 = 999;

/// `botGlobalNavWeaponWeights`: how much a weapon lying on the map is worth to a bot.
const WEAPON_GOAL_WEIGHTS: [f32; 16] = [
    0.0, 0.0, 0.0, 0.0, 0.0, 3.0, 5.0, 4.0, 6.0, 7.0, 8.0, 9.0, 3.0, 3.0, 3.0, 0.0,
];

/// A waypoint's link to another (`wpneighbor_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Neighbour {
    /// The other waypoint's number, as the file names it.
    pub num: i32,
    /// 999 where the way there is a Force jump, else 0.
    pub force_jump_to: i32,
}

/// A waypoint (`wpobject_t`); its number is its place in [`BotRoutes::waypoints`].
#[derive(Clone, Debug, PartialEq)]
pub struct Waypoint {
    pub origin: [f32; 3],
    /// `WPFLAG_*`.
    pub flags: i32,
    /// What getting here is worth (a goal point's).
    pub weight: f32,
    /// The entity the waypoint serves (an item, an objective), or 1023.
    pub associated_entity: i32,
    /// The distance to the next waypoint on the trail, as the file has it.
    pub disttonext: f32,
    /// 999 where reaching this jump point takes a Force jump, else 0.
    pub force_jump_to: i32,
    pub neighbours: Vec<Neighbour>,
}

/// What `LoadPathData` made of the level's route file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteLoad {
    /// Read (1).
    Loaded,
    /// Too large to read (0).
    TooLong,
    /// Not there (2).
    NotFound,
}

/// A level's waypoints and what the game marks on them.
#[derive(Clone, Debug, PartialEq)]
pub struct BotRoutes {
    pub load: RouteLoad,
    /// `gLevelFlags` (`LEVELFLAG_*`), from the file's `levelflags` line.
    pub level_flags: i32,
    pub waypoints: Vec<Waypoint>,
    /// `flagRed`: the last waypoint flagged as the red flag's.
    pub red_flag: Option<usize>,
    /// `flagBlue`: the last waypoint flagged as the blue flag's (and not the red's).
    pub blue_flag: Option<usize>,
    /// `eFlagRed`, `eFlagBlue`: the first red and blue flag entities
    /// ([`BotRoutes::find_flag_entities`]).
    pub flag_entities: (Option<i32>, Option<i32>),
    /// `flagRed`, `flagBlue` as play moves them: the flags' points ([`BotRoutes::red_flag`]
    /// and [`BotRoutes::blue_flag`] keep the originals), or the points nearest a dropped
    /// flag (`GetNewFlagPoint`).
    pub current_flags: (Option<usize>, Option<usize>),
}

/// An entity `CalculateWeightGoals` weighs: its number, classname, item and where it is
/// (`s.pos.trBase`).
#[derive(Clone, Copy, Debug)]
pub struct GoalEntity<'a> {
    pub number: i32,
    pub classname: &'a [u8],
    pub item: Option<&'a Item>,
    pub origin: [f32; 3],
}

/// An `info_siege_objective` in entity order, as `CalculateSiegeGoals` finds it.
#[derive(Clone, Copy, Debug)]
pub enum SiegeGoal {
    /// The objective's side and the entity at the end of the chain of what targets it
    /// (`GetObjectThatTargets`): its number and the centre of its bounds. `None` where
    /// nothing targets the objective.
    Objective {
        side: i32,
        target: Option<(i32, [f32; 3])>,
    },
    /// The chain ran 2,048 links: the reference stops looking at objectives there.
    Endless,
}

/// `G_SpawnGEntityFromSpawnVars`' first checks: whether a map entity is left standing in
/// `gametype` — not with `notsingle` in single player, `notteam` in the team games or
/// `notfree` in the others, nor with a `gametype` key that does not name this one
/// (`strstr`, so a part of a word counts).
pub fn spawns_in(entity: &sjk_entity::Entity, gametype: i32) -> bool {
    const NAMES: [&str; 10] = [
        "ffa",
        "holocron",
        "jedimaster",
        "duel",
        "powerduel",
        "single",
        "team",
        "siege",
        "ctf",
        "cty",
    ];
    let set = |key: &str| {
        entity
            .get(key)
            .is_some_and(|value| crate::userinfo::atoi(value.as_bytes()) != 0)
    };
    if gametype == 5 && set("notsingle") {
        return false;
    }
    if if gametype >= 6 {
        set("notteam")
    } else {
        set("notfree")
    } {
        return false;
    }
    match (
        entity.get("gametype"),
        usize::try_from(gametype)
            .ok()
            .and_then(|gametype| NAMES.get(gametype)),
    ) {
        (Some(value), Some(name)) => value.contains(name),
        _ => true,
    }
}

/// An `info_siege_objective` of the map as `CalculateSiegeGoals` walks it, before the
/// game has numbered anything: the map entity (its index in the entity lump) at the end
/// of the chain of what targets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiegeChain {
    /// The objective's side, and the chain's end (`None` where nothing targets it).
    Objective { side: i32, end: Option<usize> },
    /// The chain ran 2,048 links: the reference stops looking at objectives there.
    Endless,
}

/// `CalculateSiegeGoals`' walk over the map's entities (`GetObjectThatTargets`: the
/// first entity, in the map's order, whose `target` is the one's `targetname`, any
/// case), from each `info_siege_objective` back to what sets it off.
///
/// The reference walks the game's entities: the map's in order, less those the game
/// frees as they spawn in `gametype` ([`spawns_in`]). One freed for an unknown class is
/// not left out here; a chain running through one is not expected on a playable map.
pub fn siege_chains(entities: &[sjk_entity::Entity], gametype: i32) -> Vec<SiegeChain> {
    let spawned = |entity: &sjk_entity::Entity| spawns_in(entity, gametype);
    let targeting = |index: usize| {
        let name = entities[index].get("targetname")?;
        entities.iter().position(|entity| {
            spawned(entity)
                && entity
                    .get("target")
                    .is_some_and(|target| target.eq_ignore_ascii_case(name))
        })
    };
    let mut chains = Vec::new();
    for (index, entity) in entities.iter().enumerate() {
        if entity.classname() != Some("info_siege_objective") || !spawned(entity) {
            continue;
        }
        let (mut end, mut next, mut links) = (index, targeting(index), 0);
        while let Some(found) = next
            && links < 2048
        {
            end = found;
            next = targeting(found);
            links += 1;
        }
        if links >= 2048 {
            chains.push(SiegeChain::Endless);
            break;
        }
        let side = entity
            .get("side")
            .map_or(0, |side| crate::userinfo::atoi(side.as_bytes()));
        chains.push(SiegeChain::Objective {
            side,
            end: (end != index).then_some(end),
        });
    }
    chains
}

/// What the goals ask of the world: the engine's PVS and a clear box trace
/// (`MASK_SOLID`, missing everything, not starting or staying in a solid).
pub trait RouteWorld {
    /// `trap->InPVS`.
    fn in_pvs(&mut self, from: [f32; 3], to: [f32; 3]) -> bool;
    /// `OrgVisibleBox`: the box `mins`..`maxs` sweeps from `from` to `to`, `ignore`
    /// passed through, without touching a solid.
    fn clear_box(
        &mut self,
        from: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        to: [f32; 3],
        ignore: i32,
    ) -> bool;
}

/// `VectorLength(a - b)` in floats.
fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// The reader's walk through the file. Where the reference would read past the end of
/// the file (a line cut short), the record is dropped and reading ends.
struct Reader<'a> {
    text: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn byte(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }
    /// The bytes up to (not including) the first of `stops`, the reader left on it.
    fn until(&mut self, stops: &[u8]) -> Option<&[u8]> {
        let from = self.at;
        let length = self
            .text
            .get(from..)?
            .iter()
            .position(|byte| stops.contains(byte))?;
        self.at += length;
        Some(&self.text[from..from + length])
    }
    fn skip(&mut self, count: usize) {
        self.at += count;
    }
    /// One line of the trail: `index flags weight (x y z) { n n-f ... } disttonext`.
    fn waypoint(&mut self) -> Option<Waypoint> {
        self.until(b" ")?; // the index, which the table's order replaces
        self.skip(1);
        let flags = atoi(self.until(b" ")?);
        self.skip(1);
        let weight = atof(self.until(b" ")?);
        self.skip(2);
        let x = atof(self.until(b" ")?);
        self.skip(1);
        let y = atof(self.until(b" ")?);
        self.skip(1);
        let z = atof(self.until(b")")?);
        self.skip(4);
        let mut neighbours = Vec::new();
        while self.byte()? != b'}' {
            let num = atoi(self.until(b" -")?);
            let mut force_jump_to = 0;
            if self.byte() == Some(b'-') {
                self.skip(1);
                self.until(b" ")?;
                force_jump_to = FORCE_JUMP;
            }
            if neighbours.len() < MAX_NEIGHBOURS {
                neighbours.push(Neighbour { num, force_jump_to });
            }
            self.skip(1);
        }
        self.skip(2);
        let disttonext = atof(self.until(b"\n")?);
        self.skip(1);
        Some(Waypoint {
            origin: [x, y, z],
            flags,
            weight,
            associated_entity: NO_ENTITY,
            disttonext,
            force_jump_to: 0,
            neighbours,
        })
    }
}

impl Default for BotRoutes {
    /// No route read yet.
    fn default() -> Self {
        Self::empty(RouteLoad::NotFound)
    }
}

impl BotRoutes {
    /// No route: what a level without a file, or before one is read, has.
    pub fn empty(load: RouteLoad) -> Self {
        Self {
            load,
            level_flags: 0,
            waypoints: Vec::new(),
            red_flag: None,
            blue_flag: None,
            flag_entities: (None, None),
            current_flags: (None, None),
        }
    }

    /// `LoadPath_ThisLevel`'s last walk: the first entity in use of each flag's
    /// classname, among `entities` (number, classname) in entity order.
    pub fn find_flag_entities<'a>(&mut self, entities: impl IntoIterator<Item = (i32, &'a [u8])>) {
        let (red, blue) = &mut self.flag_entities;
        for (number, classname) in entities {
            if red.is_none() && classname == b"team_CTF_redflag" {
                *red = Some(number);
            } else if blue.is_none() && classname == b"team_CTF_blueflag" {
                *blue = Some(number);
            }
            if red.is_some() && blue.is_some() {
                break;
            }
        }
    }

    /// `LoadPathData(map)`'s reading of `file` (`None` where there is none), with the
    /// reference's complaints printed. The goals are [`BotRoutes::calculate_goals`]'s.
    pub fn read(map: &[u8], file: Option<&[u8]>, print: &mut dyn FnMut(&[u8])) -> Self {
        let Some(text) = file else {
            print(&[b"^3Bot route data not found for ", map, b"\n"].concat());
            return Self::empty(RouteLoad::NotFound);
        };
        if text.len() >= MAX_ROUTE_BYTES {
            print(b"^1Route file exceeds maximum length\n");
            return Self::empty(RouteLoad::TooLong);
        }
        let mut routes = Self::empty(RouteLoad::Loaded);
        let mut reader = Reader { text, at: 0 };
        if text.first() == Some(&b'l') {
            if reader.until(b" ").is_some() {
                reader.skip(1);
            }
            // `readLFlags` holds 63 characters.
            routes.level_flags = reader
                .until(b"\n")
                .map_or(0, |flags| atoi(&flags[..flags.len().min(63)]));
            reader.skip(1);
        }
        while reader.at < text.len() {
            let Some(waypoint) = reader.waypoint() else {
                break;
            };
            // `CreateNewWP_FromObject`: full tables take no more.
            if routes.waypoints.len() >= MAX_WAYPOINTS {
                continue;
            }
            let number = routes.waypoints.len();
            if waypoint.flags & WPFLAG_RED_FLAG != 0 {
                routes.red_flag = Some(number);
            } else if waypoint.flags & WPFLAG_BLUE_FLAG != 0 {
                routes.blue_flag = Some(number);
            }
            routes.waypoints.push(waypoint);
        }
        routes.current_flags = (routes.red_flag, routes.blue_flag);
        routes
    }

    /// The rest of `LoadPathData` once a file was read: the siege objectives in a siege
    /// game, the item goals (after clearing the file's with `bot_wp_clearweight`), the
    /// jump points.
    pub fn calculate_goals(
        &mut self,
        siege: Option<&[SiegeGoal]>,
        entities: &[GoalEntity],
        clear_weights: bool,
        world: &mut dyn RouteWorld,
    ) {
        if self.load != RouteLoad::Loaded {
            return;
        }
        if let Some(objectives) = siege {
            self.calculate_siege_goals(objectives, world);
        }
        self.calculate_weight_goals(entities, clear_weights, world);
        self.calculate_jump_routes();
    }

    /// `GetNearestVisibleWP`: the nearest waypoint under 800 units in the PVS a thin box
    /// reaches from `origin`.
    pub fn nearest_visible(
        &self,
        origin: [f32; 3],
        ignore: i32,
        world: &mut dyn RouteWorld,
    ) -> Option<usize> {
        let (mut best, mut best_distance) = (None, 800.0);
        for (index, waypoint) in self.waypoints.iter().enumerate() {
            let length = distance(origin, waypoint.origin);
            if length < best_distance
                && world.in_pvs(origin, waypoint.origin)
                && world.clear_box(
                    origin,
                    [-15.0, -15.0, -1.0],
                    [15.0, 15.0, 1.0],
                    waypoint.origin,
                    ignore,
                )
            {
                (best, best_distance) = (Some(index), length);
            }
        }
        best
    }

    /// `GetNearestVisibleWPToItem`: the nearest waypoint under 64 units and within 15 of
    /// the item's height that a flat box reaches.
    fn nearest_to_item(
        &self,
        origin: [f32; 3],
        ignore: i32,
        world: &mut dyn RouteWorld,
    ) -> Option<usize> {
        let (mut best, mut best_distance) = (None, 64.0);
        for (index, waypoint) in self.waypoints.iter().enumerate() {
            if !(waypoint.origin[2] - 15.0 < origin[2] && waypoint.origin[2] + 15.0 > origin[2]) {
                continue;
            }
            let length = distance(origin, waypoint.origin);
            if length < best_distance
                && world.in_pvs(origin, waypoint.origin)
                && world.clear_box(
                    origin,
                    [-15.0, -15.0, 0.0],
                    [15.0, 15.0, 0.0],
                    waypoint.origin,
                    ignore,
                )
            {
                (best, best_distance) = (Some(index), length);
            }
        }
        best
    }

    /// `CalculateSiegeGoals`: the waypoint nearest what each objective's chain ends at is
    /// flagged with the objective's side and tied to that entity.
    fn calculate_siege_goals(&mut self, objectives: &[SiegeGoal], world: &mut dyn RouteWorld) {
        for objective in objectives {
            let SiegeGoal::Objective { side, target } = *objective else {
                break;
            };
            let Some((number, centre)) = target else {
                continue;
            };
            if let Some(index) = self.nearest_visible(centre, number, world) {
                let waypoint = &mut self.waypoints[index];
                waypoint.flags |= if side == 1 {
                    WPFLAG_SIEGE_IMPERIALOBJ
                } else {
                    WPFLAG_SIEGE_REBELOBJ
                };
                waypoint.associated_entity = number;
            }
        }
    }

    /// `CalculateWeightGoals`: each item a bot wants makes the waypoint nearest it a goal
    /// with the item's weight (a later entity's weight replacing an earlier one's).
    fn calculate_weight_goals(
        &mut self,
        entities: &[GoalEntity],
        clear_weights: bool,
        world: &mut dyn RouteWorld,
    ) {
        if clear_weights {
            for waypoint in &mut self.waypoints {
                waypoint.weight = 0.0;
                waypoint.flags &= !WPFLAG_GOALPOINT;
            }
        }
        for entity in entities {
            let weight = goal_weight(entity);
            if weight == 0.0 {
                continue;
            }
            if let Some(index) = self.nearest_to_item(entity.origin, entity.number, world) {
                let waypoint = &mut self.waypoints[index];
                waypoint.weight = weight;
                waypoint.flags |= WPFLAG_GOALPOINT;
                waypoint.associated_entity = entity.number;
            }
        }
    }

    /// `CalculateJumpRoutes`: a jump point more than 128 units above a neighbour on the
    /// trail takes a Force jump. The first waypoint has no one before it here (the
    /// reference reads the slot before its table).
    fn calculate_jump_routes(&mut self) {
        for index in 0..self.waypoints.len() {
            if self.waypoints[index].flags & WPFLAG_JUMP == 0 {
                continue;
            }
            let height = self.waypoints[index].origin[2];
            let below = |other: Option<&Waypoint>| {
                other
                    .filter(|other| other.origin[2] + 16.0 < height)
                    .map_or(0.0, |other| height - other.origin[2])
            };
            let before = below(
                index
                    .checked_sub(1)
                    .and_then(|previous| self.waypoints.get(previous)),
            );
            let after = below(self.waypoints.get(index + 1));
            let rise = if before > after { before } else { after };
            self.waypoints[index].force_jump_to = if rise > 128.0 { FORCE_JUMP } else { 0 };
        }
    }
}

/// What `CalculateWeightGoals` makes of an entity: the named items' weights (by exact
/// classname), a weapon's by `botGlobalNavWeaponWeights`, ammo 3, anything else nothing.
fn goal_weight(entity: &GoalEntity) -> f32 {
    match entity.classname {
        b"item_seeker" | b"item_shield" | b"item_medpac" | b"item_sentry_gun"
        | b"item_ysalimari" => 2.0,
        b"item_force_enlighten_dark" | b"item_force_enlighten_light" | b"item_force_boon" => 5.0,
        classname => match entity.item {
            Some(item) if classname.windows(7).any(|window| window == b"weapon_") => {
                usize::try_from(item.tag)
                    .ok()
                    .and_then(|tag| WEAPON_GOAL_WEIGHTS.get(tag))
                    .copied()
                    .unwrap_or(0.0)
            }
            Some(item) if item.kind == Kind::Ammo => 3.0,
            _ => 0.0,
        },
    }
}
