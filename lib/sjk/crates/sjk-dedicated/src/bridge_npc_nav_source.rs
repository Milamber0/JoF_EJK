//! `g_npcNav`, this server's own option (not the reference's): navigation for NPCs on a
//! level that has none of its own. Server-side only, so stock clients see NPCs that hunt,
//! flank and take cover on maps that never shipped NPC navigation.
//!
//! - `0` (the default): the map's own `.nav` file or waypoints, exactly as the reference.
//! - `1`: the bots' route file (`botroutes/<map>.wnt`) made into the NPCs' graph.
//! - `2`: the route file, else a graph sampled from the map's collision.
//!
//! The rules are `sjk_game_jka::npc_nav_sources`; here the server reads the option and the
//! route file, sweeps its map (without movers, whose links the game flags) and seeds the
//! floor from the map's spawn points and items. It is read as a level begins: a change
//! takes effect with the next map or `map_restart`.

use super::*;
use sjk_game_jka::npc_nav_sources::{
    CollisionSweeps, NavSource, WALK_MASK, bot_route_walkways, collision_walkways,
};
use sjk_game_jka::npc_roster::NpcRoster;
use sjk_game_jka::npc_spawn::NpcHost;

/// The option's name.
pub(super) const NPC_NAV: &[u8] = b"g_npcNav";

/// What the level was given: where from, its nodes and links, the combat points at
/// cover, and how long the making took.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct MadeNavigation {
    pub(super) source: NavSource,
    pub(super) from_routes: bool,
    pub(super) nodes: usize,
    pub(super) links: usize,
    pub(super) cover_points: usize,
    pub(super) millis: f64,
}

impl NativeGame {
    /// `g_npcNav` registered (archived: an operator's choice is remembered) and read.
    pub(super) fn npc_nav_source(&mut self) -> NavSource {
        let about: &[u8] = b"SJK: NPC navigation for maps without their own: 0 stock, 1 bot routes, 2 bot routes or map collision";
        self.cvars
            .get(NPC_NAV, b"0", crate::cvars::CVAR_ARCHIVE, Some(about));
        NavSource::from_setting(self.integer_cvar(NPC_NAV, 0))
    }

    /// The level's bot route file, from the operator's files or the map's archives.
    pub(super) fn route_file(&mut self, map: &LoadedMap) -> Option<Vec<u8>> {
        let path = format!(
            "botroutes/{}.wnt",
            String::from_utf8_lossy(&self.identity.mapname)
        );
        self.config_files.read(&path).or_else(|| {
            map.files
                .read(&path)
                .ok()
                .flatten()
                .map(|asset| asset.bytes.to_vec())
        })
    }
}

/// The navigation `source` gives a level that has none of its own (after its entities
/// spawned): the route file's walkways, else — for [`NavSource::Collision`] — the floor
/// flooded from the map's spawn points and items; adopted by the roster, with combat
/// points at cover. `None` where the source is stock, the level has its own, or nothing
/// could be made.
pub(super) fn make_navigation(
    source: NavSource,
    map: &LoadedMap,
    route_file: Option<&[u8]>,
    roster: &mut NpcRoster,
    level_time: i32,
    host: &mut impl NpcHost,
) -> Option<MadeNavigation> {
    if source == NavSource::Stock {
        return None;
    }
    let started = std::time::Instant::now();
    let collision = WorldCollision {
        bsp: &map.bsp,
        scratch: &map.scratch,
    };
    let mut world = CollisionSweeps {
        collision: &collision,
        mask: WALK_MASK,
    };
    if roster.has_own_navigation(&mut world) {
        return None;
    }
    let routes = route_file
        .map(|file| sjk_game_jka::bot_routes::BotRoutes::read(b"", Some(file), &mut |_| {}));
    let mut ways = routes
        .as_ref()
        .map(|routes| bot_route_walkways(routes, &mut world))
        .filter(|ways| !ways.links.is_empty());
    let from_routes = ways.is_some();
    if ways.is_none() && source == NavSource::Collision {
        let seeds: Vec<[f32; 3]> = map
            .spawn_points
            .iter()
            .map(|point| point.origin)
            .chain(map.items.iter().map(|item| item.origin))
            .collect();
        ways = Some(collision_walkways(&mut world, &seeds)).filter(|ways| !ways.links.is_empty());
    }
    let ways = ways?;
    let cover_points = roster.adopt_navigation(&ways, &mut world, level_time, host);
    let millis = started.elapsed().as_secs_f64() * 1_000.0;
    Some(MadeNavigation {
        source,
        from_routes,
        nodes: ways.points.len(),
        links: ways.links.len(),
        cover_points,
        millis,
    })
}

impl MadeNavigation {
    /// The line the server logs.
    pub(super) fn describe(&self) -> String {
        let from = if self.from_routes {
            "the bots' route file"
        } else {
            "the map's collision"
        };
        format!(
            "navigation: g_npcNav {} made {} nodes and {} links from {from}, and {} combat points at cover, in {:.0} ms",
            match self.source {
                NavSource::Stock => 0,
                NavSource::BotRoutes => 1,
                NavSource::Collision => 2,
            },
            self.nodes,
            self.links,
            self.cover_points,
            self.millis
        )
    }
}
