//! The reference's spawn table (`spawns[]`, `g_spawn.c:494-686`, and the item list
//! `G_CallSpawn` searches first, `g_spawn.c:700-730`) as this server answers it: for every
//! classname a map can place, whether the native server spawns it, reads it as a place,
//! has nothing to do because the reference removes it, or does not spawn it yet.
//!
//! Nothing a map places is dropped silently: [`census`] counts every entity the native
//! server leaves unspawned — an unported classname the reference would run, or one the
//! reference has no spawn function for (`"%s doesn't have a spawn function"`) — so the
//! server can say so once per map, and the capability matrix can name each one.
//!
//! [`sound_set_order`] is `G_PrecacheSoundsets` (`g_spawn.c:1526-1546`, run by
//! `G_SpawnEntitiesFromString` after the last spawn): the order in which the map's
//! soundsets reach `CS_AMBIENT_SET`.

use sjk_entity::Entity;

/// How this server answers one classname of the reference's spawn table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    /// Spawned and run by the native server.
    Ported,
    /// A place other entities read (a teleporter's destination, an effect's aim, a bot's
    /// roam point): the reference spawns it and runs nothing of it.
    Point,
    /// Nothing to do: the reference removes it as it spawns (`info_null`, `func_group`,
    /// an unswitched `light`, `misc_model_static`, which clients build for themselves).
    Removed,
    /// The reference spawns and runs it; this server does not yet. Each is a row of the
    /// capability matrix.
    Unported,
}

use Support::{Point, Ported, Removed, Unported};

/// `spawns[]`, in the reference's order, with this server's answer for each.
const SPAWNS: &[(&str, Support)] = &[
    ("emplaced_gun", Ported),
    ("func_bobbing", Ported),
    ("func_breakable", Ported),
    ("func_button", Ported),
    ("func_door", Ported),
    ("func_glass", Ported),
    ("func_group", Removed),
    ("func_pendulum", Ported),
    ("func_plat", Ported),
    ("func_rotating", Ported),
    ("func_static", Ported),
    ("func_timer", Ported),
    ("func_train", Ported),
    ("func_usable", Ported),
    ("func_wall", Unported),
    ("fx_rain", Ported),
    ("fx_runner", Ported),
    ("fx_snow", Ported),
    ("fx_spacedust", Ported),
    ("fx_wind", Ported),
    ("gametype_item", Unported),
    ("info_camp", Point),
    ("info_jedimaster_start", Ported),
    ("info_notnull", Point),
    ("info_null", Removed),
    ("info_player_deathmatch", Ported),
    ("info_player_duel", Ported),
    ("info_player_duel1", Ported),
    ("info_player_duel2", Ported),
    ("info_player_intermission", Ported),
    ("info_player_intermission_blue", Ported),
    ("info_player_intermission_red", Ported),
    ("info_player_siegeteam1", Ported),
    ("info_player_siegeteam2", Ported),
    ("info_player_start", Ported),
    ("info_player_start_blue", Ported),
    ("info_player_start_red", Ported),
    ("info_siege_decomplete", Ported),
    ("info_siege_objective", Ported),
    ("info_siege_radaricon", Ported),
    ("item_botroam", Point),
    ("light", Removed),
    ("misc_ammo_floor_unit", Unported),
    ("misc_bsp", Unported),
    ("misc_cubemap", Removed),
    ("misc_faller", Unported),
    ("misc_G2model", Unported),
    ("misc_holocron", Ported),
    ("misc_maglock", Unported),
    ("misc_model", Unported),
    ("misc_model_ammo_power_converter", Unported),
    ("misc_model_breakable", Unported),
    ("misc_model_health_power_converter", Unported),
    ("misc_model_shield_power_converter", Unported),
    ("misc_model_static", Removed),
    ("misc_portal_camera", Ported),
    ("misc_portal_surface", Ported),
    ("misc_shield_floor_unit", Unported),
    ("misc_siege_item", Ported),
    ("misc_skyportal", Ported),
    ("misc_skyportal_orient", Removed),
    ("misc_teleporter_dest", Point),
    ("misc_turret", Ported),
    ("misc_turretG2", Ported),
    ("misc_weapon_shooter", Unported),
    ("misc_weather_zone", Removed),
    // Fighters are set aside at the spawner, which says so (`vehicle_roster`).
    ("npc_vehicle", Ported),
    ("path_corner", Point),
    ("point_combat", Ported),
    ("ref_tag", Point),
    ("ref_tag_huge", Point),
    ("shooter_blaster", Unported),
    ("target_activate", Ported),
    ("target_counter", Ported),
    ("target_deactivate", Ported),
    ("target_delay", Ported),
    ("target_escapetrig", Unported),
    ("target_give", Unported),
    ("target_interest", Unported),
    ("target_kill", Ported),
    ("target_laser", Unported),
    ("target_level_change", Unported),
    ("target_location", Ported),
    ("target_play_music", Ported),
    ("target_position", Point),
    ("target_print", Ported),
    ("target_push", Unported),
    ("target_random", Ported),
    ("target_relay", Ported),
    ("target_remove_powerups", Unported),
    ("target_score", Unported),
    ("target_screenshake", Unported),
    ("target_scriptrunner", Ported),
    ("target_siege_end", Ported),
    ("target_speaker", Ported),
    ("target_teleporter", Ported),
    ("team_CTF_blueplayer", Ported),
    ("team_CTF_bluespawn", Ported),
    ("team_CTF_redplayer", Ported),
    ("team_CTF_redspawn", Ported),
    ("terrain", Unported),
    ("trigger_always", Ported),
    ("trigger_asteroid_field", Unported),
    ("trigger_hurt", Ported),
    ("trigger_hyperspace", Ported),
    ("trigger_lightningstrike", Unported),
    ("trigger_multiple", Ported),
    ("trigger_once", Ported),
    ("trigger_push", Ported),
    ("trigger_shipboundary", Ported),
    ("trigger_space", Ported),
    ("trigger_teleport", Ported),
    ("waypoint", Ported),
    ("waypoint_navgoal", Ported),
    ("waypoint_navgoal_1", Ported),
    ("waypoint_navgoal_2", Ported),
    ("waypoint_navgoal_4", Ported),
    ("waypoint_navgoal_8", Ported),
    ("waypoint_small", Ported),
];

/// What the reference's `G_CallSpawn` does with `entity`, as this server answers it:
/// `None` when the reference has no spawn function for its classname (it prints so and
/// frees the entity). The item list comes first and compares exactly (`strcmp`); the
/// table compares without case (`Q_stricmp`). Every `npc_*` but the vehicle spawns
/// through the NPC roster. A `light` with a name switches light styles, which is not
/// ported; one without is removed. An `fx_runner` without an `fxFile` is refused.
pub fn support(entity: &Entity) -> Option<Support> {
    let classname = entity.classname()?;
    if crate::items::find(classname).is_some() || classname.eq_ignore_ascii_case("worldspawn") {
        return Some(Ported);
    }
    let found = SPAWNS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(classname))
        .map(|(_, support)| *support);
    let lower = classname.to_ascii_lowercase();
    let found =
        found.or_else(|| (lower.starts_with("npc_") && reference_npc(&lower)).then_some(Ported))?;
    Some(match lower.as_str() {
        "light"
            if entity
                .get("targetname")
                .is_some_and(|name| !name.is_empty()) =>
        {
            Unported
        }
        "fx_runner" if entity.get("fxFile").is_none_or(str::is_empty) => Removed,
        // `SP_func_rotating` with health is `SP_func_breakable`'s, which does not turn.
        "func_rotating"
            if entity
                .get("health")
                .is_some_and(|health| crate::userinfo::atoi(health.as_bytes()) != 0) =>
        {
            Unported
        }
        // `SP_func_train` frees a train without a target.
        "func_train" if entity.get("target").is_none_or(str::is_empty) => Removed,
        _ => found,
    })
}

/// Whether the reference's table has an `npc_*` spawn by this lowercase name (the NPC
/// spawners, `g_spawn.c:562-625`).
fn reference_npc(lower: &str) -> bool {
    const NPCS: &[&str] = &[
        "alora",
        "bartender",
        "bespincop",
        "colombian_emplacedgunner",
        "colombian_rebel",
        "colombian_soldier",
        "cultist",
        "cultist_commando",
        "cultist_destroyer",
        "cultist_saber",
        "cultist_saber_powers",
        "desann",
        "droid_atst",
        "droid_gonk",
        "droid_interrogator",
        "droid_mark1",
        "droid_mark2",
        "droid_mouse",
        "droid_probe",
        "droid_protocol",
        "droid_r2d2",
        "droid_r5d2",
        "droid_remote",
        "droid_seeker",
        "droid_sentry",
        "galak",
        "gran",
        "human_merc",
        "imperial",
        "impworker",
        "jan",
        "jawa",
        "jedi",
        "kyle",
        "lando",
        "luke",
        "manuel_vergara_rmg",
        "minemonster",
        "monmothma",
        "monster_claw",
        "monster_fish",
        "monster_flier2",
        "monster_glider",
        "monster_howler",
        "monster_lizard",
        "monster_murjj",
        "monster_rancor",
        "monster_swamp",
        "monster_wampa",
        "morgankatarn",
        "noghri",
        "prisoner",
        "rebel",
        "reborn",
        "reborn_new",
        "reelo",
        "rodian",
        "shadowtrooper",
        "snowtrooper",
        "spawner",
        "stormtrooper",
        "stormtrooperofficer",
        "swamptrooper",
        "tavion",
        "tavion_new",
        "tie_pilot",
        "trandoshan",
        "tusken",
        "ugnaught",
        "weequay",
    ];
    lower
        .strip_prefix("npc_")
        .is_some_and(|name| NPCS.contains(&name))
}

/// Whether the reference leaves `entity` standing after the level's spawn in
/// `gametype`: it passes `G_SpawnGEntityFromSpawnVars`' game-type checks, has a spawn
/// function, and that function does not remove it. What `G_Find` can find afterwards,
/// and what `G_PrecacheSoundsets` walks.
pub fn kept_by_reference(entity: &Entity, gametype: i32) -> bool {
    crate::bot_routes::spawns_in(entity, gametype)
        && matches!(support(entity), Some(Ported | Point | Unported))
}

/// The entities of a map the native server leaves unspawned, by classname: those it has
/// not ported and those the reference has no spawn function for, each with how many the
/// map places. In the order the classnames first appear.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Census {
    /// Classnames the reference spawns and this server does not yet.
    pub unported: Vec<(String, usize)>,
    /// Classnames the reference has no spawn function for either.
    pub unknown: Vec<(String, usize)>,
}

impl Census {
    /// Whether the map places nothing the server leaves unspawned.
    pub fn is_empty(&self) -> bool {
        self.unported.is_empty() && self.unknown.is_empty()
    }

    /// One line per kind for the server's log, or none for a map with nothing left out.
    pub fn report(&self, map: &str) -> Vec<String> {
        let list = |entries: &[(String, usize)]| {
            entries
                .iter()
                .map(|(name, count)| format!("{name} x{count}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut lines = Vec::new();
        if !self.unported.is_empty() {
            lines.push(format!(
                "{map}: entities not spawned (not ported yet): {}",
                list(&self.unported)
            ));
        }
        if !self.unknown.is_empty() {
            lines.push(format!(
                "{map}: entities with no spawn function (the reference drops them too): {}",
                list(&self.unknown)
            ));
        }
        lines
    }
}

/// Every entity of `entities` (the map's lump, in order) that `gametype` keeps and this
/// server leaves unspawned, counted by classname.
pub fn census(entities: &[Entity], gametype: i32) -> Census {
    let mut census = Census::default();
    for entity in entities.iter().skip(1) {
        if !crate::bot_routes::spawns_in(entity, gametype) {
            continue;
        }
        let list = match support(entity) {
            Some(Unported) => &mut census.unported,
            None => &mut census.unknown,
            Some(_) => continue,
        };
        let name = entity.classname().unwrap_or("(no classname)");
        match list.iter_mut().find(|(known, _)| known == name) {
            Some((_, count)) => *count += 1,
            None => list.push((name.to_owned(), 1)),
        }
    }
    census
}

/// The soundsets of a map in the order they reach `CS_AMBIENT_SET` in `gametype`: first
/// each soundset `target_speaker`'s, as its spawn registers it (`SP_target_speaker`,
/// `g_target.c:329-336`), then every standing entity's `soundSet` in entity order
/// (`G_PrecacheSoundsets`), each name once. The worldspawn's is `CS_GLOBAL_AMBIENT_SET`'s,
/// not one of these.
pub fn sound_set_order(entities: &[Entity], gametype: i32) -> Vec<&str> {
    let placed = || {
        entities
            .iter()
            .skip(1)
            .filter(move |entity| kept_by_reference(entity, gametype))
    };
    let speakers = placed()
        .filter(|entity| {
            entity
                .classname()
                .is_some_and(|name| name.eq_ignore_ascii_case("target_speaker"))
        })
        .filter_map(|entity| entity.get("soundSet"));
    let mut order: Vec<&str> = Vec::new();
    for name in speakers.chain(placed().filter_map(|entity| entity.get("soundSet"))) {
        if !name.is_empty() && !order.contains(&name) {
            order.push(name);
        }
    }
    order
}
