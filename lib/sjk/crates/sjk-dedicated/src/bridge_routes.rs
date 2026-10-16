//! The level's bot routes on this server (`LoadPath_ThisLevel`): the route file found
//! through the search path, the goals worked out against the map, kept for the bots'
//! thinking. See [`sjk_game_jka::bot_routes`].

use super::*;
use crate::cvars::{CVAR_CHEAT, CVAR_VM_CREATED};
use sjk_game_jka::bot_routes::{BotRoutes, GoalEntity, RouteWorld, SiegeGoal};

/// `MASK_SOLID`.
const MASK_SOLID: u32 = 0x1;

/// A siege objective's chain end as the bots find it: the number its route point is tied
/// to, its model (a brush) and its centre.
#[derive(Clone, Copy, Debug)]
pub(super) struct SiegeLink {
    pub(super) entity: i32,
    pub(super) model: Option<usize>,
    pub(super) centre: [f32; 3],
}

/// `BotAISetup`'s variables, as a release build of the reference registers them.
const BOT_AI_CVARS: [(&[u8], &[u8], u32); 11] = [
    (b"bot_forcepowers", b"1", CVAR_CHEAT),
    (b"bot_forgimmick", b"0", CVAR_CHEAT),
    (b"bot_honorableduelacceptance", b"0", CVAR_CHEAT),
    (b"bot_pvstype", b"1", CVAR_CHEAT),
    (b"bot_attachments", b"1", 0),
    (b"bot_camp", b"1", 0),
    (b"bot_wp_info", b"1", 0),
    (b"bot_wp_edit", b"0", CVAR_CHEAT),
    (b"bot_wp_clearweight", b"1", 0),
    (b"bot_wp_distconnect", b"1", 0),
    (b"bot_wp_visconnect", b"1", 0),
];

/// The map as the goals see it: its PVS and its brushes (the server's other traces
/// likewise leave brush entities out); without a map, everything is seen and clear.
struct ServerRoutes<'a> {
    map: Option<&'a LoadedMap>,
}

impl RouteWorld for ServerRoutes<'_> {
    fn in_pvs(&mut self, from: [f32; 3], to: [f32; 3]) -> bool {
        self.map
            .is_none_or(|map| Eye::new(&map.bsp, &map.areas, from).sees_point(&map.bsp, to))
    }

    fn clear_box(
        &mut self,
        from: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        to: [f32; 3],
        _ignore: i32,
    ) -> bool {
        let trace = match self.map {
            Some(map) => WorldCollision {
                bsp: &map.bsp,
                scratch: &map.scratch,
            }
            .trace(from, mins, maxs, to, MASK_SOLID),
            None => Void.trace(from, mins, maxs, to, MASK_SOLID),
        };
        trace.fraction == 1.0 && !trace.start_solid && !trace.all_solid
    }
}

impl NativeGame {
    /// `BotAISetup`'s registrations, the game's own.
    pub(super) fn register_bot_cvars(&mut self) {
        for (name, value, flags) in BOT_AI_CVARS {
            self.cvars.get(name, value, flags | CVAR_VM_CREATED, None);
        }
    }

    /// `LoadPath_ThisLevel`: `botroutes/<map>.wnt` read (the complaints on the console),
    /// then the siege objectives, the items' goals and the jump points, and the flags.
    ///
    /// The items weighed are those the level placed, where it placed them (the reference
    /// weighs them before they fall to the floor); an item the game type removes weighs
    /// nothing here, where the reference weighs it and frees it a frame later.
    pub(super) fn load_routes(&mut self) {
        let map_name = self.identity.mapname.clone();
        let path = format!("botroutes/{}.wnt", String::from_utf8_lossy(&map_name));
        let file = self.config_files.read(&path);
        let mut routes = BotRoutes::read(&map_name, file.as_deref(), &mut |text| {
            let _ = std::io::Write::write_all(&mut std::io::stdout(), text);
        });
        let placed = self.placed_items();
        let entities: Vec<GoalEntity> = placed
            .iter()
            .map(|&(number, item, origin)| {
                let item = &sjk_game_jka::items::ITEMS[item];
                GoalEntity {
                    number: i32::from(number),
                    classname: item.classname.as_bytes(),
                    item: Some(item),
                    origin,
                }
            })
            .collect();
        let goals = self.siege_goals();
        let siege = (self.gametype == GAMETYPE_SIEGE).then_some(&goals[..]);
        let clear_weights = self.cvars.integer(b"bot_wp_clearweight") != 0;
        routes.calculate_goals(
            siege,
            &entities,
            clear_weights,
            &mut ServerRoutes {
                map: self.map.as_ref(),
            },
        );
        routes.find_flag_entities(
            entities
                .iter()
                .map(|entity| (entity.number, entity.classname)),
        );
        self.bots.routes = routes;
    }

    /// `CalculateSiegeGoals`' objectives, their chains' ends linked to this server's
    /// entities: a breakable or a usable brush by its model and number; anything else
    /// (a trigger, a relay, a counter, an objective) by a number of its own below zero,
    /// which no trace reports. The links are kept for the bots' views.
    fn siege_goals(&mut self) -> Vec<SiegeGoal> {
        let Some(map) = &self.map else {
            return Vec::new();
        };
        let mut goals = Vec::new();
        let mut links = Vec::new();
        for chain in &map.siege_chains {
            let Some((side, end)) = *chain else {
                goals.push(SiegeGoal::Endless);
                break;
            };
            let target = end.map(|end| {
                let numbered = end.model.and_then(|model| {
                    let breakable = self
                        .breakables
                        .iter()
                        .find(|(_, brush)| brush.model == model)
                        .map(|(number, _)| number.legacy_number());
                    breakable.or_else(|| {
                        self.usable_entities
                            .iter()
                            .find(|(_, usable)| usable.model == model)
                            .map(|(number, _)| number.legacy_number())
                    })
                });
                let entity = numbered.map_or(-2 - end.index as i32, i32::from);
                links.push(SiegeLink {
                    entity,
                    model: end.model,
                    centre: end.centre,
                });
                (entity, end.centre)
            });
            goals.push(SiegeGoal::Objective { side, target });
        }
        self.bots.siege_links = links;
        goals
    }

    /// The items the level placed that stand in the game, in entity order: number, item
    /// and the origin the map gave it.
    fn placed_items(&self) -> Vec<(u16, usize, [f32; 3])> {
        let Some(map) = &self.map else {
            return Vec::new();
        };
        // The pickups were spawned in the map's order, skipping what the game type
        // removes and what could not stand.
        let mut standing = self.items.iter().peekable();
        let mut placed = Vec::new();
        for item in &map.items {
            if let Some((number, pickup)) = standing.peek()
                && pickup.item == item.item
            {
                placed.push((number.legacy_number(), item.item, item.origin));
                standing.next();
            }
        }
        placed
    }
}
