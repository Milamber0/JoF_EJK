//! Reading a map off disk: file access belongs to the process, not to the game.
use sjk_bsp::{Bsp, TraceScratch};
use sjk_entity::parse_entity_lump;
use sjk_game_jka::map::{SpawnPoint, spawn_points};
use sjk_game_jka::worldspawn::{World, WorldSettings, spawn_world};
use sjk_vfs::VirtualFileSystem;
use std::{cell::RefCell, error::Error, path::Path};

/// A siege objective's chain as the map places it: `None` for the reference's endless
/// chain, which stops the walk.
pub type SiegeChain = Option<(i32, Option<ChainEnd>)>;

/// The entity at the end of a siege objective's chain: its place in the entity lump, its
/// inline model (a brush) and the centre of its box as the game links it (a brush's
/// model at its origin; a linked `info_siege_objective`'s origin; an unlinked one's —
/// a relay's, a counter's — nothing, so the world's origin).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChainEnd {
    pub index: usize,
    pub model: Option<usize>,
    pub centre: [f32; 3],
}

/// What the server keeps of a loaded map.
pub struct LoadedMap {
    /// What players collide with.
    pub bsp: Bsp,
    /// Its areas, and which portals between them the doors hold open.
    pub areas: crate::visibility::Areas,
    /// Trace storage sized for this map, shared by every player's movement.
    pub scratch: RefCell<TraceScratch>,
    /// Where players can be placed, in the order of the map's entity lump.
    pub spawn_points: Vec<SpawnPoint>,
    /// The items the map places, in the same order.
    pub items: Vec<sjk_game_jka::items::Placed>,
    /// Where it places the Jedi Master's saber (`info_jedimaster_start`), and its
    /// holocrons with their powers (`misc_holocron`).
    pub jedi_master_starts: Vec<[f32; 3]>,
    pub holocrons: Vec<([f32; 3], usize)>,
    /// The hurt brushes the map places (pits, lava, crushers), in the same order.
    pub hurt_triggers: Vec<sjk_game_jka::triggers::Placed>,
    /// The jump pads and teleporters it places, each with where it points and how that
    /// place faces (`target_position`).
    pub movers: Vec<(sjk_game_jka::triggers::Mover, bool, [f32; 3], [f32; 3])>,
    /// The `trigger_multiple` brushes it places, with the bounds of the models they are.
    pub multiples: Vec<sjk_game_jka::triggers::Multiple>,
    /// The targets it places, by the name things fire them with.
    pub targets: Vec<(String, sjk_game_jka::triggers::Target)>,
    /// A siege map's `.siege` file and the game's class and team files, `None` for a map
    /// that has no `.siege` file.
    pub siege: Option<sjk_game_jka::siege_map::SiegeFiles>,
    /// Each siege objective's chain (`CalculateSiegeGoals`): its side and, where something
    /// sets it off, the chain's end as the map places it.
    pub siege_chains: Vec<SiegeChain>,
    /// The `func_usable` brushes it places, which a player's use key fires.
    pub usables: Vec<sjk_game_jka::use_key::Usable>,
    /// The breakable brushes it places, with the bounds of the brush models they are.
    pub breakables: Vec<sjk_game_jka::breakables::Breakable>,
    /// The binary movers it places — `func_door` and `func_plat` — with the bounds of
    /// the brush models they are.
    pub doors: Vec<sjk_game_jka::movers::Door>,
    /// The `target_location`s it places, which name where a teammate is speaking from.
    pub locations: Vec<sjk_game_jka::chat::Location>,
    /// What the map's worldspawn published, in index order, empty strings dropped.
    pub config_strings: Vec<(usize, Vec<u8>)>,
    /// What else it decided: gravity and warmup.
    pub world: World,
    /// The game's files, for what the server loads after the map: the skeletons of the
    /// player models its clients choose.
    pub files: VirtualFileSystem,
    /// The saber definitions (`WP_SaberLoadParms`), read as the game starts.
    pub sabers: std::sync::Arc<sjk_game_jka::saber_definition::SaberParms>,
    /// The NPC definitions (`NPC_LoadParms`), read as the game starts.
    pub npc_parms: std::sync::Arc<sjk_game_jka::npc_parms::NpcParms>,
    /// Unbounded native definitions, with verified single-part MD3 models enabled.
    pub npc_parms_native: std::sync::Arc<sjk_game_jka::npc_parms::NpcParms>,
    /// The vehicle and vehicle weapon definitions (`BG_VehicleLoadParms`), read as the game
    /// starts.
    pub vehicle_files: std::sync::Arc<sjk_game_jka::vehicle_parms::VehicleFiles>,
    /// The map's `NPC_*`, `point_combat` and `waypoint*` entities, in the lump's order, for
    /// the game to spawn.
    pub npc_entities: Vec<sjk_entity::Entity>,
    /// The whole entity lump in its order, for what the game spawns from it as the level
    /// begins in its game type (`bridge_map_effects`: effect runners, speakers, the
    /// soundsets, and the census of what this server leaves unspawned).
    pub entities: Vec<sjk_entity::Entity>,
    /// `sv_mapChecksum`: the `.bsp`'s checksum (`CM_LoadMap`, `Com_BlockChecksum`).
    pub checksum: i32,
    /// The map's navigation file (`maps/<name>.nav`), if the game data has one.
    pub nav_file: Option<Vec<u8>>,
    /// How long each animation of the humanoid skeleton lasts, which movement holds
    /// animations by; `None` if the game data has no `animation.cfg` for it.
    pub animations: Option<std::sync::Arc<dyn sjk_game_jka::AnimationLengths>>,
    /// The pak the map's `.bsp` was read from, as `FS_ReferencedPakChecksums` and
    /// `FS_ReferencedPakNames` report it: its checksum and `<game directory>/<name>`.
    /// A client missing it downloads it. `None` for a map not read from a pak.
    pub pak: Option<ReferencedPak>,
}

/// `CM_ModelBounds` for one of the map's inline models: its box "spread by a pixel" on
/// every side (`CMod_LoadSubmodels`, `cm_load.cpp:154-156`), which is what
/// `trap->SetBrushModel` gives an entity as `r.mins`/`r.maxs`. Every rule that reads a
/// brush entity's size (a door's travel, a lift's rest, a trigger's reach) reads these.
pub fn brush_bounds(bsp: &Bsp, model: usize) -> Option<([f32; 3], [f32; 3])> {
    let model = bsp.render().models().get(model)?;
    Some((
        model.minimums.map(|value| value - 1.0),
        model.maximums.map(|value| value + 1.0),
    ))
}

/// Mount `<game_data>/base/*.pk3` in the reference's search order and load
/// `maps/<name>.bsp`, as `SV_SpawnServer` finds a map.
pub fn load(game_data: &Path, name: &str) -> Result<LoadedMap, Box<dyn Error>> {
    let mut files = VirtualFileSystem::new();
    files.mount_pk3_directory_with_warnings(game_data.join("base"), |archive, error| {
        eprintln!("skipping {}: {error}", archive.display());
    })?;
    load_from(files, name)
}

/// `maps/<name>.bsp` and what goes with it, out of `files` however they were mounted.
pub fn load_from(files: VirtualFileSystem, name: &str) -> Result<LoadedMap, Box<dyn Error>> {
    let path = format!("maps/{name}.bsp");
    let asset = files
        .read(&path)?
        .ok_or_else(|| format!("{path} is in none of the mounted archives"))?;
    let pak = referenced_pak(Path::new(&*asset.source.mount_name));
    let bsp = Bsp::parse(&asset.bytes)?;
    let entities = parse_entity_lump(bsp.entities())?;
    let spawn_points = spawn_points(&entities);
    let items = sjk_game_jka::items::placed(&entities);
    let jedi_master_starts = sjk_game_jka::jedi_master::starts(&entities);
    let holocrons = sjk_game_jka::holocron::placed(&entities);
    let hurt_triggers = sjk_game_jka::triggers::placed(&entities);
    // The triggers that move a player, with the positions they point at; one that points
    // nowhere the map knows is dropped, as the reference frees it.
    let mut movers = Vec::new();
    for (classname, teleports) in [("trigger_push", false), ("trigger_teleport", true)] {
        for mover in sjk_game_jka::triggers::movers(&entities, classname) {
            if let Some((origin, angles)) =
                sjk_game_jka::triggers::target_of(&entities, &mover.target)
            {
                movers.push((mover, teleports, origin, angles));
            }
        }
    }
    if spawn_points.is_empty() {
        return Err(format!("{path} has no spawn point").into());
    }
    // The first entity is the world. A later write to the same index replaces an
    // earlier one, and an empty string clears it, as a configstring table behaves.
    let first = entities
        .first()
        .ok_or_else(|| format!("{path} has no entities"))?;
    let mut table = std::collections::BTreeMap::new();
    // No cvars exist yet: deathmatch, no message of the day, no warmup, time zero.
    let settings = WorldSettings {
        gametype: 0,
        start_time: 0,
        motd: b"",
        do_warmup: false,
        restarted: false,
    };
    let world = spawn_world(first, settings, |index, value| {
        if value.is_empty() {
            table.remove(&index);
        } else {
            table.insert(index, value.to_vec());
        }
    })
    .map_err(|error| format!("{path}: {error:?}"))?;
    let scratch = RefCell::new(bsp.trace_scratch());
    // `BG_ParseAnimationFile` for the skeleton every player model shares.
    let animations = files
        .read("models/players/_humanoid/animation.cfg")?
        .and_then(|asset| {
            let config = sjk_model::AnimationConfig::parse(&asset.bytes).ok()?;
            let table: std::sync::Arc<dyn sjk_game_jka::AnimationLengths> = std::sync::Arc::new(
                sjk_game_jka::AnimationLengthTable::from_animation_config(&config),
            );
            Some(table)
        });
    if animations.is_none() {
        eprintln!("no humanoid animation.cfg: players will hold no animations");
    }
    // The triggers that fire a map's targets, and the targets themselves.
    let mut multiples = Vec::new();
    for entity in &entities {
        let model = entity
            .get("model")
            .and_then(|name| name.strip_prefix('*'))
            .and_then(|index| index.parse::<usize>().ok());
        let bounds = model.and_then(|index| brush_bounds(&bsp, index));
        if let Some(bounds) = bounds
            && let Some(trigger) = sjk_game_jka::triggers::spawn_multiple(entity, bounds, 0)
        {
            multiples.push(trigger);
        }
    }
    // The doors, lifts and platforms: brush entities that slide between two places.
    // `func_plat` is the same binary mover a `func_door` is, drawn at the top of its own
    // travel and spawned at the bottom of it.
    let mut doors = Vec::new();
    for entity in &entities {
        let model = entity
            .get("model")
            .and_then(|name| name.strip_prefix('*'))
            .and_then(|index| index.parse::<usize>().ok());
        let bounds = model.and_then(|index| brush_bounds(&bsp, index));
        let Some(bounds) = bounds else { continue };
        if let Some(door) = sjk_game_jka::movers::spawn_door(entity, bounds) {
            doors.push(door);
        } else if let Some(plat) = sjk_game_jka::movers::spawn_plat(entity, bounds) {
            doors.push(plat);
        } else if let Some(button) = sjk_game_jka::movers::spawn_button(entity, bounds) {
            doors.push(button);
        }
    }
    // A siege map: its `.siege` file, and every class and team file the game ships, in
    // the listing's order (`BG_SiegeLoadClasses`, `BG_SiegeLoadTeams`). The chains the
    // bots follow are read as a siege game (`GT_SIEGE`) spawns them.
    let mut siege = None;
    let mut siege_chains = Vec::new();
    if let Ok(Some(asset)) = files.read(&format!("maps/{name}.siege")) {
        let read_all = |directory: &str, extension: &str| -> Vec<String> {
            files
                .list_files(directory, extension)
                .into_iter()
                .filter_map(|file| {
                    files
                        .read(&format!("{directory}/{file}"))
                        .ok()
                        .flatten()
                        .map(|asset| String::from_utf8_lossy(&asset.bytes).into_owned())
                })
                .collect()
        };
        let (classes, teams) = (
            read_all("ext_data/Siege/Classes", ".scl"),
            read_all("ext_data/Siege/Teams", ".team"),
        );
        let registry = sjk_game_jka::siege_class::SiegeRegistry::load(
            classes.iter().map(String::as_str),
            teams.iter().map(String::as_str),
        );
        siege = Some(sjk_game_jka::siege_map::SiegeFiles {
            text: asset.bytes.to_vec(),
            registry: std::sync::Arc::new(registry),
        });
        siege_chains = sjk_game_jka::bot_routes::siege_chains(&entities, 7)
            .into_iter()
            .map(|chain| match chain {
                sjk_game_jka::bot_routes::SiegeChain::Objective { side, end } => Some((
                    side,
                    end.map(|index| chain_end(&bsp, &entities[index], index)),
                )),
                sjk_game_jka::bot_routes::SiegeChain::Endless => None,
            })
            .collect();
    }
    // The brushes a player's use key fires.
    let mut usables = Vec::new();
    for entity in &entities {
        let model = entity
            .get("model")
            .and_then(|name| name.strip_prefix('*'))
            .and_then(|index| index.parse::<usize>().ok());
        let bounds = model.and_then(|index| brush_bounds(&bsp, index));
        if let Some(bounds) = bounds
            && let Some(usable) = sjk_game_jka::use_key::spawn_usable(entity, bounds)
        {
            usables.push(usable);
        }
    }
    // The brushes it puts there to be broken: crates, glass, stone.
    let mut breakables = Vec::new();
    for entity in &entities {
        let model = entity
            .get("model")
            .and_then(|name| name.strip_prefix('*'))
            .and_then(|index| index.parse::<usize>().ok());
        let bounds = model.and_then(|index| brush_bounds(&bsp, index));
        if let Some(bounds) = bounds
            && let Some(brush) = sjk_game_jka::breakables::spawn(entity, bounds)
        {
            breakables.push(brush);
        }
    }
    let locations = entities
        .iter()
        .filter_map(sjk_game_jka::chat::spawn_location)
        .collect();
    // The speakers are entities of their own, which the game spawns from the lump with
    // their sounds registered where every other sound is (`bridge_map_effects`).
    let mut targets = sjk_game_jka::triggers::map_targets(&entities, &mut |_| 0);
    targets.retain(|(_, target)| !matches!(target, sjk_game_jka::triggers::Target::Speaker { .. }));
    let sabers = std::sync::Arc::new(saber_parms(&files));
    let (npc_parms, npc_parms_native) = npc_parms(&files);
    let (npc_parms, npc_parms_native) = (
        std::sync::Arc::new(npc_parms),
        std::sync::Arc::new(npc_parms_native),
    );
    let vehicle_files =
        std::sync::Arc::new(sjk_game_jka::vehicle_parms::VehicleFiles::from_listing(
            |directory, extension| files.list_files(directory, extension),
            |path| {
                files
                    .read(path)
                    .ok()
                    .flatten()
                    .map(|asset| asset.bytes.to_vec())
            },
        ));
    let for_npcs = |name: &str| {
        let lower = name.to_ascii_lowercase();
        lower.starts_with("npc_") || lower == "point_combat" || lower.starts_with("waypoint")
    };
    let npc_entities = entities
        .iter()
        .filter(|entity| entity.classname().is_some_and(for_npcs))
        .cloned()
        .collect();
    let checksum = sjk_vfs::file_checksum(&asset.bytes);
    let nav_file = files
        .read(&format!("maps/{name}.nav"))?
        .map(|asset| asset.bytes.to_vec());
    Ok(LoadedMap {
        areas: crate::visibility::Areas::new(&bsp),
        bsp,
        scratch,
        spawn_points,
        items,
        jedi_master_starts,
        holocrons,
        hurt_triggers,
        movers,
        multiples,
        targets,
        doors,
        breakables,
        siege,
        siege_chains,
        usables,
        locations,
        config_strings: table.into_iter().collect(),
        world,
        animations,
        sabers,
        npc_parms,
        npc_parms_native,
        vehicle_files,
        npc_entities,
        checksum,
        nav_file,
        files,
        pak,
        entities,
    })
}

/// The pak a map's `.bsp` was read from, as clients are told of it (`pack->referenced`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferencedPak {
    /// The checksum of its entries (`FS_LoadZipFile`).
    pub checksum: i32,
    /// `<directory>/<name without .pk3>`, as `sv_referencedPakNames` lists it.
    pub name: String,
    /// Where it is on disk, for a client that downloads it.
    pub path: std::path::PathBuf,
}

/// A `.bsp`'s pak, or `None` for a map not read from one.
fn referenced_pak(archive: &Path) -> Option<ReferencedPak> {
    if !archive
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pk3"))
    {
        return None;
    }
    let checksum = sjk_vfs::Pk3Fingerprint::open(archive).ok()?.checksum();
    let name = archive.file_stem()?.to_string_lossy();
    let directory = archive.parent()?.file_name()?.to_string_lossy();
    Some(ReferencedPak {
        checksum,
        name: format!("{directory}/{name}"),
        path: archive.to_path_buf(),
    })
}

/// `WP_SaberLoadParms`: every `ext_data/sabers/*.sab` the game's listing holds, in its
/// order. Definitions that outgrow the game's buffer are refused whole (the reference
/// stops the map), leaving every player the default saber.
fn saber_parms(files: &VirtualFileSystem) -> sjk_game_jka::saber_definition::SaberParms {
    use sjk_game_jka::saber_definition::{SaberParms, listed_files};
    let names = files.list_files("ext_data/sabers", ".sab");
    let mut texts = Vec::new();
    for name in listed_files(names.iter().map(String::as_str)) {
        match files.read(&format!("ext_data/sabers/{name}")) {
            Ok(Some(asset)) => texts.push((name, asset.bytes)),
            _ => eprintln!("WP_SaberLoadParms: error reading file: {name}"),
        }
    }
    SaberParms::load(texts.iter().map(|(name, text)| (*name, text.as_slice()))).unwrap_or_else(
        |error| {
            eprintln!("WP_SaberLoadParms: {error}");
            SaberParms::default()
        },
    )
}

/// Load strict stock definitions plus an unbounded native set. The native profile
/// enables existing, single-frame MD3 bodies without rewriting their NPC class or
/// pretending that they have another droid's Ghoul2 skeleton.
fn npc_parms(
    files: &VirtualFileSystem,
) -> (
    sjk_game_jka::npc_parms::NpcParms,
    sjk_game_jka::npc_parms::NpcParms,
) {
    use sjk_game_jka::npc_parms::NpcParms;
    use sjk_game_jka::saber_definition::listed_files;
    let names = files.list_files("ext_data/npcs", ".npc");
    let mut texts = Vec::new();
    for name in listed_files(names.iter().map(String::as_str)) {
        match files.read(&format!("ext_data/npcs/{name}")) {
            Ok(Some(asset)) => texts.push((name, asset.bytes)),
            _ => eprintln!("NPC_LoadParms: error reading file: {name}"),
        }
    }
    let stock = NpcParms::load(texts.iter().map(|(name, text)| (*name, text.as_slice())))
        .unwrap_or_else(|error| {
            eprintln!("NPC_LoadParms: {error}");
            NpcParms::default()
        });
    let whole = NpcParms::load_unbounded(texts.iter().map(|(name, text)| (*name, text.as_slice())));
    let rigid = sjk_game_jka::npc_rigid::discover(
        &whole,
        |path| files.contains(path).unwrap_or(false),
        |path| {
            files
                .read(path)
                .ok()
                .flatten()
                .and_then(|asset| sjk_model::Md3::parse(&asset.bytes).ok())
                .is_some_and(|model| {
                    model.frames.len() == 1 && model.tags.iter().all(Vec::is_empty)
                })
        },
    );
    if !rigid.is_empty() {
        let names: Vec<String> = rigid
            .iter()
            .map(|model| {
                format!(
                    "{} as {}",
                    String::from_utf8_lossy(&model.npc),
                    String::from_utf8_lossy(&model.path)
                )
            })
            .collect();
        eprintln!("native rigid NPC models: {}", names.join(", "));
    }
    (stock, whole.with_rigid_models(rigid))
}

/// Where a siege chain's end is, as `CalculateSiegeGoals` reads its box.
fn chain_end(bsp: &Bsp, entity: &sjk_entity::Entity, index: usize) -> ChainEnd {
    let origin = entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]);
    let model = entity
        .get("model")
        .and_then(|name| name.strip_prefix('*'))
        .and_then(|number| number.parse::<usize>().ok());
    let centre = match model.and_then(|model| bsp.render().models().get(model)) {
        Some(bounds) => std::array::from_fn(|axis| {
            origin[axis] + (bounds.minimums[axis] + bounds.maximums[axis]) / 2.0
        }),
        None if entity.classname() == Some("info_siege_objective") => origin,
        None => [0.0; 3],
    };
    ChainEnd {
        index,
        model,
        centre,
    }
}
