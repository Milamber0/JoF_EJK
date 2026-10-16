//! How a level gets its navigation (`codemp/game/g_nav.c`, `g_main.c`): the map's `.nav`
//! file read if it was made for this build of the map (`Nav_Load`); else its waypoint
//! entities placed as nodes (`SP_waypoint`, `SP_waypoint_small`: out of solid, their
//! clear radius measured) and, 400 ms into the level, connected by their targets
//! (`NAV_CheckCalcPaths`, `NAV_CalculatePaths`, `HardConnect`, `CheckBlockedEdges`) and
//! ranked (`CalculatePaths`). Either way every combat point is given its nearest waypoint
//! (`CP_FindCombatPointWaypoints`). The navigation goals scripts name
//! (`SP_waypoint_navgoal*`) are kept as tags.
//!
//! Some stock maps ship `.nav` files made for other maps.
//!
//! Held to `tools/game-oracle/npcnav.c` (`game-npcnav.txt`, `game-npcnavload.txt`).

use crate::npc_mind::WAYPOINT_NONE;
use crate::npc_navigator::{
    CONTENTS_BOTCLIP, CONTENTS_MONSTERCLIP, MASK_SOLID, MAX_STORED_WAYPOINTS, NF_CLEAR_PATH,
    NavGoalTag, NavHolder, StoredWaypoint,
};
use crate::npc_spawn::{ENTITYNUM_NONE, NpcHost};
use crate::npc_world::NpcWorld;
use crate::text_parse::atof;

/// `START_TIME_NAV_CALC` (`FRAMETIME*4`).
pub(crate) const START_TIME_NAV_CALC: i32 = 400;
/// `DEFAULT_MINS_2`, `DEFAULT_MAXS_2`, `CROUCH_MAXS_2`, `STEPSIZE`.
const DEFAULT_MINS_2: f32 = -24.0;
const DEFAULT_MAXS_2: f32 = 40.0;
const CROUCH_MAXS_2: f32 = 16.0;
const STEPSIZE: f32 = 18.0;
/// `MASK_DEADSOLID`: what a waypoint is tested against for being in solid.
const MASK_DEADSOLID: u32 = 0x1 | 0x10 | 0x1000;
/// `MAX_RADIUS_CHECK`, `YAW_ITERATIONS`.
const MAX_RADIUS_CHECK: u32 = 1_024;
const YAW_ITERATIONS: u32 = 16;
/// `NAVGOAL_USE_RADIUS`.
const NAVGOAL_USE_RADIUS: i32 = 16_384;
/// `wpMins`, `wpMaxs` (`navigator.cpp:59-60`): the box an edge is traced with.
const WP_MINS: [f32; 3] = [-16.0, -16.0, -24.0 + STEPSIZE];
const WP_MAXS: [f32; 3] = [16.0, 16.0, 32.0];
/// `NAV_FindClosestWaypointForPoint2`'s marker box.
const MARKER_MINS: [f32; 3] = [-16.0, -16.0, -6.0];
const MARKER_MAXS: [f32; 3] = [16.0, 16.0, 32.0];

/// What an entity in an edge's way is to the navigator (`GVM_NAV_EntIs*`, and
/// `NAV_TestBestNode`'s brush of monster or bot clip that a script will remove).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavObstacle {
    /// A door, and whether it opens for an NPC (`G_EntIsUnlockedDoor`).
    Door { unlocked: bool },
    /// Something breakable (`G_EntIsBreakable`).
    Breakable,
    /// A usable that removes itself (`G_EntIsRemovableUsable`).
    RemovableUsable,
    /// A named brush of monster or bot clip.
    ClipBrush,
    /// Anything else.
    Other,
}

/// `vtos` (`g_utils.c:662-675`).
fn vtos(v: [f32; 3]) -> String {
    format!("({} {} {})", v[0] as i32, v[1] as i32, v[2] as i32)
}

/// A map's `F_VECTOR` field: `sscanf("%f %f %f")`, what is not read left at zero.
fn vector(value: &str) -> [f32; 3] {
    let mut out = [0.0; 3];
    for (slot, word) in out.iter_mut().zip(value.split_ascii_whitespace()) {
        *slot = atof(word.as_bytes());
    }
    out
}

/// The entity's `s.angles`: `angles`, or `angle` as a yaw, whichever the map gave last.
fn angles(entity: &sjk_entity::Entity) -> [f32; 3] {
    entity
        .fields()
        .iter()
        .rev()
        .find_map(|(key, value)| {
            if key.eq_ignore_ascii_case("angles") {
                Some(vector(value))
            } else if key.eq_ignore_ascii_case("angle") {
                Some([0.0, atof(value.as_bytes()), 0.0])
            } else {
                None
            }
        })
        .unwrap_or([0.0; 3])
}

/// `G_CheckInSolid` (`g_utils.c:1941-1977`) for a box at `origin` with the entity's
/// `clipmask` (`mask`): its bottom swept down by its own depth. Where it stops short and
/// `fix`, the box is put where it stopped (`origin` changed) and tested again without
/// fixing.
fn check_in_solid(
    host: &mut impl NpcHost,
    origin: &mut [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    mask: u32,
    fix: bool,
) -> bool {
    let end = [origin[0], origin[1], origin[2] + mins[2]];
    let flat = [mins[0], mins[1], 0.0];
    let trace = host.trace(*origin, flat, maxs, end, ENTITYNUM_NONE, mask, &[]);
    if trace.all_solid || trace.start_solid {
        return true;
    }
    if trace.fraction < 1.0 {
        if !fix {
            return true;
        }
        *origin = [
            trace.end_position[0],
            trace.end_position[1],
            trace.end_position[2] - mins[2],
        ];
        return check_in_solid(host, origin, mins, maxs, mask, false);
    }
    false
}

/// `waypoint_getRadius` (`g_nav.c:1242-1289`): the shortest of 16 flat sweeps out from
/// `origin`, each no longer than the shortest so far (1025 at first).
pub(crate) fn waypoint_radius(host: &mut impl NpcHost, origin: [f32; 3]) -> u32 {
    let (mins, maxs) = (
        [-15.0, -15.0, DEFAULT_MINS_2 + STEPSIZE],
        [15.0, 15.0, DEFAULT_MAXS_2],
    );
    let mut least = MAX_RADIUS_CHECK + 1;
    for step in 0..YAW_ITERATIONS {
        let yaw = (360.0_f32 / YAW_ITERATIONS as f32) * step as f32;
        let forward = crate::pmove::flight::flight_axes([0.0, yaw, 0.0])
            .0
            .to_array();
        let reach = least as f32;
        let end: [f32; 3] = std::array::from_fn(|axis| origin[axis] + reach * forward[axis]);
        let trace = host.trace(
            origin,
            mins,
            maxs,
            end,
            ENTITYNUM_NONE,
            0x1 | CONTENTS_MONSTERCLIP | CONTENTS_BOTCLIP,
            &[],
        );
        let distance = (least as f32 * trace.fraction) as u32;
        least = least.min(distance);
    }
    least
}

impl crate::npc_roster::NpcRoster {
    /// `Nav_Load` (`g_main.c:325`), before the map's entities spawn: its `.nav` file
    /// (`file`, if the map has one) against the map's checksum (`sv_mapChecksum`). Whether
    /// it loaded; if not, the waypoint entities make the graph.
    pub fn load_navigation(&mut self, file: Option<&[u8]>, checksum: i32) -> bool {
        self.level.navigator = crate::npc_navigator::Navigator::default();
        self.level.navigator.load(file, checksum)
    }

    /// The spawn of a waypoint or navigation goal entity (`classname`), if it is one.
    pub(crate) fn spawn_navigation(
        &mut self,
        entity: &sjk_entity::Entity,
        classname: &str,
        host: &mut impl NpcHost,
    ) -> bool {
        let lower = classname.to_ascii_lowercase();
        match lower.as_str() {
            "waypoint" => self.spawn_waypoint(entity, false, host),
            "waypoint_small" => self.spawn_waypoint(entity, true, host),
            "waypoint_navgoal" | "waypoint_navgoal_8" | "waypoint_navgoal_4"
            | "waypoint_navgoal_2" | "waypoint_navgoal_1" => {
                let size = lower
                    .strip_prefix("waypoint_navgoal_")
                    .map_or(16.0, |size| atof(size.as_bytes()));
                self.spawn_nav_goal(entity, size, host);
            }
            _ => return false,
        }
        true
    }

    /// `SP_waypoint`, `SP_waypoint_small` (`g_nav.c:1297-1377`): where the paths are to be
    /// calculated, a node where the waypoint stands (raised out of the floor it touches;
    /// crouch-high if standing-high is in solid; refused if that is too), with its clear
    /// radius (a small one's 2), kept with its names for connecting.
    fn spawn_waypoint(
        &mut self,
        entity: &sjk_entity::Entity,
        small: bool,
        host: &mut impl NpcHost,
    ) {
        if !self.level.navigator.calculating {
            return;
        }
        let width = if small { 2.0 } else { 15.0 };
        let (mins, mut maxs) = (
            [-width, -width, DEFAULT_MINS_2],
            [width, width, DEFAULT_MAXS_2],
        );
        let mut origin = entity.get("origin").map_or([0.0; 3], vector);
        let spawnflags = entity
            .get("spawnflags")
            .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()));
        let targetname = entity
            .get("targetname")
            .map(|name| crate::npc_parms::new_string(name.as_bytes()));
        if spawnflags & 1 == 0
            && check_in_solid(host, &mut origin, mins, maxs, MASK_DEADSOLID, true)
        {
            maxs[2] = CROUCH_MAXS_2;
            if check_in_solid(host, &mut origin, mins, maxs, MASK_DEADSOLID, true) {
                let name = targetname.as_deref().map_or_else(
                    || "(null)".to_owned(),
                    |name| String::from_utf8_lossy(name).into_owned(),
                );
                let kind = if small { "Waypoint_small" } else { "Waypoint" };
                host.print(&format!(
                    "^1ERROR: {kind} {name} at {} in solid!\n",
                    vtos(origin)
                ));
                return;
            }
        }
        let radius = if small {
            2
        } else {
            waypoint_radius(host, origin) as i32
        };
        let node = self
            .level
            .navigator
            .graph
            .add_node(origin, spawnflags, radius);
        // `NAV_StoreWaypoint`: past 512 the node stays, unconnectable.
        if self.level.navigator.stored.len() < MAX_STORED_WAYPOINTS {
            let key = |key: &str| {
                entity
                    .get(key)
                    .map(|name| crate::npc_parms::new_string(name.as_bytes()))
                    .unwrap_or_default()
            };
            let targets = [
                key("target"),
                key("target2"),
                key("target3"),
                key("target4"),
            ];
            self.level.navigator.stored.push(StoredWaypoint {
                targetname: targetname.unwrap_or_default(),
                targets,
                node,
            });
        }
    }

    /// `SP_waypoint_navgoal` and its sized kin (`g_nav.c:1392-1535`): a tag scripts send
    /// NPCs to (`TAG_Add`, `RTF_NAVGOAL`), its radius the map's (reached by distance) or
    /// 12, or the size's for a sized one; told of when it stands in solid.
    fn spawn_nav_goal(&mut self, entity: &sjk_entity::Entity, size: f32, host: &mut impl NpcHost) {
        let sized = size != 16.0;
        let radius = entity
            .get("radius")
            .map_or(0.0, |value| atof(value.as_bytes()));
        let radius = if sized {
            size as i32
        } else if radius != 0.0 {
            (radius as i32) | NAVGOAL_USE_RADIUS
        } else {
            12
        };
        let (mins, maxs) = ([-size, -size, -24.0], [size, size, 32.0]);
        let mut origin = entity.get("origin").map_or([0.0; 3], vector);
        let spawnflags = entity
            .get("spawnflags")
            .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()));
        let name = entity
            .get("targetname")
            .map(|name| crate::npc_parms::new_string(name.as_bytes()))
            .unwrap_or_default();
        let shown = String::from_utf8_lossy(&name).into_owned();
        // The test is made at `r.currentOrigin`, before `s.origin` is raised, with the
        // entity's own `clipmask` — never set on a navigation goal, so 0: a real map's
        // navigation goal is never found in solid.
        let mut at = origin;
        if spawnflags & 1 == 0 && check_in_solid(host, &mut at, mins, maxs, 0, false) {
            let kind = if sized {
                format!("Waypoint_navgoal_{}", size as i32)
            } else {
                "Waypoint_navgoal".to_owned()
            };
            host.print(&format!(
                "^1ERROR: {kind} {shown} at {} in solid!\n",
                vtos(origin)
            ));
        }
        origin[2] += 0.125;
        // `TAG_Add`: a name already a tag is refused; a nameless one too.
        let lowered = name.to_ascii_lowercase();
        if self
            .level
            .navigator
            .tags
            .iter()
            .any(|tag| tag.name == lowered)
        {
            host.print(&format!("^1Duplicate tag name \"{shown}\"\n"));
            return;
        }
        if name.is_empty() {
            host.print(&format!(
                "^1ERROR: Nameless ref_tag found at ({} {} {})\n",
                origin[0] as i32, origin[1] as i32, origin[2] as i32
            ));
            return;
        }
        self.level.navigator.tags.push(NavGoalTag {
            name: lowered,
            origin,
            angles: angles(entity),
            radius,
        });
    }

    /// The end of `G_InitGame`'s navigation (`g_main.c:377-397`): the paths calculated 400
    /// ms on where no file was loaded; else the combat points given their waypoints now.
    pub(crate) fn finish_navigation(&mut self, level_time: i32, host: &mut impl NpcHost) {
        if self.level.navigator.calculating {
            self.level.navigator.calc_at = level_time + START_TIME_NAV_CALC;
            return;
        }
        self.level.navigator.calc_at = 0;
        let mut fired = Vec::new();
        self.world(level_time, host, &mut fired)
            .find_combat_point_waypoints();
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NAV_CheckCalcPaths` (`g_main.c:2862-2893`): once the level has run 400 ms, the
    /// failed edges cleared, the stored waypoints connected, the blocked edges checked,
    /// the paths ranked and the combat points given their waypoints. (The reference then
    /// saves the file, which is not done here.)
    pub fn check_calc_paths(&mut self) {
        let calc_at = self.level.navigator.calc_at;
        if calc_at == 0 || calc_at >= self.level_time {
            return;
        }
        self.calc_paths();
    }

    /// `NAV_CheckCalcPaths`' work, whenever it is due: the failed edges cleared, the stored
    /// waypoints connected, the blocked edges checked, the paths ranked and the combat
    /// points given their waypoints.
    pub(crate) fn calc_paths(&mut self) {
        self.level.navigator.clear_all_failed_edges();
        self.connect_stored_waypoints();
        self.check_blocked_edges();
        self.level.navigator.graph.paths_calculated = false;
        self.level.navigator.graph.calculate_paths();
        self.find_combat_point_waypoints();
        self.level.navigator.calc_at = 0;
    }

    /// `NAV_CalculatePaths`' connecting (`g_nav.c:1756-1845`): each stored waypoint
    /// joined to the first stored waypoint named by each of its four targets
    /// (`HardConnect`: blocked where the world is in the way).
    fn connect_stored_waypoints(&mut self) {
        let stored = std::mem::take(&mut self.level.navigator.stored);
        for waypoint in &stored {
            for target in &waypoint.targets {
                if target.is_empty() {
                    continue;
                }
                let Some(other) = stored.iter().find(|other| {
                    !other.targetname.is_empty() && other.targetname.eq_ignore_ascii_case(target)
                }) else {
                    continue;
                };
                let graph = &self.level.navigator.graph;
                let (start, end) = (
                    graph.nodes()[waypoint.node as usize].position,
                    graph.nodes()[other.node as usize].position,
                );
                let trace = self.trace_bodies(
                    start,
                    WP_MINS,
                    WP_MAXS,
                    end,
                    ENTITYNUM_NONE,
                    MASK_SOLID | CONTENTS_BOTCLIP | CONTENTS_MONSTERCLIP,
                );
                let blocked = trace.fraction != 1.0 || trace.start_solid || trace.all_solid;
                self.level
                    .navigator
                    .graph
                    .hard_connect(waypoint.node, other.node, blocked);
            }
        }
        self.level.navigator.stored = stored;
    }

    /// `CheckBlockedEdges` (`navigator.cpp:998-1066`): an edge the world blocked when made,
    /// traced again; failed if what is in its way is a locked door, a breakable or a
    /// removable usable.
    fn check_blocked_edges(&mut self) {
        let count = self.level.navigator.graph.len();
        for first in 0..count {
            for index in 0..self.level.navigator.graph.nodes()[first].edges.len() {
                let graph = &self.level.navigator.graph;
                let edge = graph.nodes()[first].edges[index];
                if edge.flags & sjk_nav::EDGE_BLOCKED == 0 {
                    continue;
                }
                let (start, end) = (
                    graph.nodes()[first].position,
                    graph.nodes()[edge.node as usize].position,
                );
                let trace = self.trace_bodies(
                    start,
                    WP_MINS,
                    WP_MAXS,
                    end,
                    ENTITYNUM_NONE,
                    MASK_SOLID | CONTENTS_MONSTERCLIP | CONTENTS_BOTCLIP,
                );
                if trace.entity_number >= crate::npc_spawn::ENTITYNUM_WORLD
                    || !(trace.fraction < 1.0 || trace.start_solid || trace.all_solid)
                {
                    continue;
                }
                let failed = matches!(
                    self.host.nav_obstacle(trace.entity_number),
                    NavObstacle::Door { unlocked: false }
                        | NavObstacle::Breakable
                        | NavObstacle::RemovableUsable
                );
                if failed {
                    let time = self.level_time;
                    self.level.navigator.add_failed_edge(
                        i32::from(ENTITYNUM_NONE),
                        first as i32,
                        edge.node,
                        time,
                    );
                }
            }
        }
    }

    /// `CP_FindCombatPointWaypoints` (`NPC_combat.c:2510-2524`): each combat point's
    /// nearest waypoint it can walk to, found with a marker entity put there for the
    /// moment (`NAV_FindClosestWaypointForPoint2`). (A debug build tells of a point
    /// without one; a release server does not.)
    pub fn find_combat_point_waypoints(&mut self) {
        for index in 0..self.level.combat_points.len() {
            let origin = self.level.combat_points[index].origin;
            self.level.combat_points[index].waypoint = self.closest_waypoint_for_point(origin);
        }
    }

    /// `NAV_FindClosestWaypointForPoint2` (`g_nav.c:406-431`).
    pub fn closest_waypoint_for_point(&mut self, origin: [f32; 3]) -> i32 {
        let Some(number) = self.host.spawn_hidden() else {
            return WAYPOINT_NONE;
        };
        let marker = NavHolder::Marker {
            number,
            origin,
            mins: MARKER_MINS,
            maxs: MARKER_MAXS,
        };
        let waypoint = self.nearest_node(marker, WAYPOINT_NONE, NF_CLEAR_PATH, WAYPOINT_NONE);
        self.host.free(number);
        waypoint
    }
}

/// `G_EntIsDoor` and `G_EntIsUnlockedDoor` (`g_mover.c:1256-1379`) for the mover at
/// `at` of `doors` (each with its entity number): a `func_door` opens for an NPC when its
/// team's master is named and an active `trigger_multiple` targets that name, or — not
/// named — when it is active, has no health and needs no use, Force or key. (A trigger's
/// `target2` and a door's own trigger turned off by a script are not kept here.)
pub fn door_obstacle<K>(
    doors: &[(K, crate::movers::Door)],
    at: usize,
    multiples: &[crate::triggers::Multiple],
) -> NavObstacle {
    const MOVER_FORCE_ACTIVATE: u32 = 2;
    const MOVER_PLAYER_USE: u32 = 64;
    let Some((_, door)) = doors.get(at) else {
        return NavObstacle::Other;
    };
    if door.kind != crate::movers::MoverKind::Door {
        return NavObstacle::Other;
    }
    let mut master = door;
    while master.team_slave {
        match master.team_master.and_then(|index| doors.get(index)) {
            Some((_, next)) if !std::ptr::eq(next, master) => master = next,
            _ => break,
        }
    }
    if !master.targetname.is_empty() {
        let unlocked = multiples.iter().any(|trigger| {
            !trigger.inactive && trigger.target.eq_ignore_ascii_case(&master.targetname)
        });
        return NavObstacle::Door { unlocked };
    }
    let unlocked = !master.inactive
        && master.health == 0
        && master.spawnflags
            & (MOVER_PLAYER_USE | MOVER_FORCE_ACTIVATE | crate::movers::MOVER_LOCKED)
            == 0;
    NavObstacle::Door { unlocked }
}

/// `CalcTeamDoorCenter` (`g_mover.c:541-558`) for the `func_door` at `at` of `doors`: the
/// middle of its box, then halfway to each part after it on its team's chain in turn.
pub fn door_center<K>(doors: &[(K, crate::movers::Door)], at: usize) -> Option<[f32; 3]> {
    let (_, door) = doors.get(at)?;
    if door.kind != crate::movers::MoverKind::Door {
        return None;
    }
    let middle = |bounds: ([f32; 3], [f32; 3])| {
        std::array::from_fn::<f32, 3, _>(|axis| (bounds.0[axis] + bounds.1[axis]) * 0.5)
    };
    let mut center = middle(door.bounds);
    let chain = &doors.get(door.team_master.unwrap_or(at))?.1.team_parts;
    let after = chain
        .iter()
        .position(|&part| part == at)
        .map_or(&[][..], |position| &chain[position + 1..]);
    for &part in after {
        let Some((_, slave)) = doors.get(part) else {
            continue;
        };
        let other = middle(slave.bounds);
        center = std::array::from_fn(|axis| (center[axis] + other[axis]) * 0.5);
    }
    Some(center)
}
