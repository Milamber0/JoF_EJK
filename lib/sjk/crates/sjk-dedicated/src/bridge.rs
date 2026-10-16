//! The native server as the legacy endpoint sees it.
//!
//! The core's world and its entity budget know nothing of the adapter; the adapter's
//! roster knows nothing of the core. Either may be larger. A protocol-26 client number
//! meets a player only at the host boundary (`bridge_host.rs`); the game keeps its
//! players by place and identity (`players.rs`).
use crate::collision::{Void, WithPlayers, WorldCollision};
use crate::map::LoadedMap;
use crate::peer::{EFLAGS, Peer, TEAM_SPECTATOR};
use crate::visibility::Eye;
use sjk_game_jka::{
    AnimationLengthTable, PM_SPECTATOR,
    client_begin::{
        Begun, PlayerSession, SpawnPlace, TeamCommand, client_begin, client_respawn, team_command,
    },
    concussion::ConcussionTargets,
    crt_rand::CrtRand,
    damage::{
        Attacker, DAMAGE_NO_ARMOR, DAMAGE_NO_PROTECTION, DamageRequest, Damaged, HitLocation,
        MOD_FALLING, SplashTarget, Target, Wounds, damage_at, fall_damage, radius_damage,
    },
    demp2::{ShockTarget, Sphere, SphereRun, run_sphere},
    disruptor::{
        Blow, DisruptorTargets, EF_DISINTEGRATION, Shooter, StruckPlayer, disintegrated, fire_alt,
        fire_main, would_dodge,
    },
    entity_clip::BoxObstacle,
    entity_id::EntityId,
    entity_pool::EntityPool,
    event_entity::EventEntity,
    force_config::ForceServerSettings,
    give::give,
    items::Pickup,
    map::{SpawnKind, select_spawn_point, spectator_start},
    mines::{Charge, ChargeFrame, ChargeRun, Watched},
    player_death::{DeathRequest, Deaths, Rng, body_left_behind, corpse_hit, kill},
    pmove::{MovementCollision, MovementState, Predictor},
    ranks::{Contender, Standings, calculate_ranks_in, scoreboard_message_in},
    registries::{ModelTable, SoundTable, first_begin_sounds, init_game, init_game_strings},
    saber_block::{Blocked, Defender, Incoming, LookCandidate, Watcher, missile_block_check},
    saber_frame::SaberFrame,
    userinfo::{
        AcceptedUserinfo, ClientSession, accept_userinfo, connect_print, too_many_connections,
        validate_userinfo,
    },
    weapon_fire::{
        HomingTarget, HomingTargets, MOD_LASER_TRAP_BLOW, Missile, MissileRun, fire_weapon,
    },
};
use sjk_network::{
    LegacyGameHost, LegacyGameOutput, LegacyServerInfo, LegacySnapshotFrame, LegacyStatusPlayer,
};
use sjk_protocol::{EntityState, PlayerState, UserCommand};
use sjk_server::{CreateWorldError, Server, WorldId};

/// Where players are placed when they spawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpawnOrder {
    /// The reference's: one of the further half of the free points, drawn with the
    /// game's generator, away from where the player died.
    #[default]
    Random,
    /// The first free point in map order — reproducible; no stock server offers it.
    Map,
    /// The same, but starting at the map's Nth deathmatch point and wrapping: the probes
    /// use it to put a client beside a particular thing in the map. No stock server
    /// offers this either.
    MapFrom(usize),
}

/// What a target of the map is doing: a delay on its way, and whether something
/// deactivated it.
#[derive(Clone, Copy, Debug, Default)]
struct MapTarget {
    next_think: i32,
    inactive: bool,
}

/// What the operator configured; not yet a cvar system.
pub struct Identity {
    /// `sv_hostname` as browsers show it.
    pub hostname: Vec<u8>,
    /// Map name advertised to browsers; no map is loaded yet.
    pub mapname: Vec<u8>,
}

/// Owns the authoritative server and the game's players, which the legacy endpoint
/// reaches through [`LegacyGameHost`].
pub struct NativeGame {
    server: Server<(), Peer>,
    world: WorldId,
    identity: Identity,
    status_info: Vec<u8>,
    /// The players in the profile's order, by native identity; the peers themselves
    /// live in `server`'s world. No wire client number is stored (`players.rs`).
    players: crate::players::PlayerRoster,
    /// What a legacy client is told about the map, in index order.
    config_strings: Vec<(usize, Vec<u8>)>,
    /// What the game wants told to legacy clients, until the endpoint takes it.
    told: Vec<Told>,
    /// The game's cvars, as far as they exist.
    settings: ForceServerSettings,
    /// Everyone a moving player can run into, gathered before it moves; kept to be reused.
    obstacles: Vec<BoxObstacle>,
    /// Each player's legs by number, gathered with the obstacles, for a move's saber
    /// specials (`PM_BGEntForNum`); kept to be reused.
    body_legs: Vec<Option<u16>>,
    /// What a player's grip, lightning and drain need beside the world; kept to be reused.
    force_scratch: bridge_force::ForceScratch,
    /// Where a duellist's state waits while its opponent is reached; kept to be reused.
    duel_slot: PlayerState,
    /// The others as a player's look-target search sees them; kept to be reused.
    candidates: Vec<LookCandidate>,
    /// Everyone standing, for the paths of the missiles a saber may block; kept to be
    /// reused.
    everyone: Vec<BoxObstacle>,
    /// The missiles in flight as a saber's block check sees them; kept to be reused.
    incoming: Vec<Incoming>,
    /// The C library's generator the reference's saber jitters a bounced bolt with,
    /// seeded from the clock as the engine seeds it.
    rand: CrtRand,
    /// The game's entities beyond its players: so far the temp entities that carry an
    /// event a player's own entity could not.
    pool: EntityPool,
    /// The pool's entities as the snapshots cull them, worked out once a frame.
    pool_links: std::cell::RefCell<bridge_links::PoolLinks>,
    /// The server's clock at the last frame, for what happens between frames without one.
    last_frame_time: i32,
    /// Every missile in flight, by its entity number, run at every frame.
    missiles: Vec<(EntityId, Missile)>,
    /// What a shot fires, before the pool numbers it; kept to be reused.
    fired: Vec<Missile>,
    /// Exploded solid missiles (rockets) linked where they struck until the pool frees
    /// them, with their owners, whose traces pass them.
    husks: Vec<(BoxObstacle, u16, EntityId)>,
    /// The players as a homing rocket asks after its enemy, taken before the missiles
    /// run; kept to be reused.
    homing: Vec<Option<HomingTarget>>,
    /// Whom an explosion may reach; kept to be reused.
    splash_targets: Vec<SplashTarget>,
    /// The DEMP2's shock spheres, thinking every 50 ms.
    spheres: Vec<(EntityId, Sphere)>,
    /// The trip mines, proximity mines and det packs.
    charges: Vec<(EntityId, Charge)>,
    /// The items the map placed, dropped to the floor as the game started.
    items: Vec<(EntityId, Pickup)>,
    /// The hurt brushes the map placed, by entity number: the pits and the lava.
    triggers: Vec<(EntityId, sjk_game_jka::triggers::Hurt)>,
    /// The jump pads and teleporters it placed, with where each points.
    movers: Vec<(EntityId, sjk_game_jka::triggers::Moving, [f32; 3], [f32; 3])>,
    /// The `trigger_multiple` brushes it placed.
    multiples: Vec<sjk_game_jka::triggers::Multiple>,
    /// The doors it placed, by entity number: every client is sent them.
    doors: Vec<(EntityId, sjk_game_jka::movers::Door)>,
    /// Whether each team master holds its area portal open (`AdjustAreaPortalState`).
    door_portals: Vec<bool>,
    /// The breakable brushes the map placed, by the entity number each was given.
    breakables: Vec<(EntityId, sjk_game_jka::breakables::Breakable)>,
    /// Its targets: the name they are fired by, what they are, and what each is doing.
    targets: Vec<(String, sjk_game_jka::triggers::Target, MapTarget)>,
    /// `G_ModelIndex`'s table.
    models: ModelTable,
    /// The server's clock at the frame before the last (`level.previousTime`).
    previous_frame_time: i32,
    /// What every death shares: the game's generator (`Rand_Init`) and the death event's
    /// counter.
    deaths: Deaths,
    /// `gGAvoidDismember`: set while a lost lock's finishing blow lands.
    avoid_dismember: bool,
    /// The obstacles a saber lock's traces pass, reused.
    lock_obstacles: Vec<BoxObstacle>,
    /// Dead sabers spawned into a slot before their saber's: they run from the next frame.
    dead_sabers_waiting: Vec<(EntityId, Missile)>,
    /// The default saber's off sound, for a saber carrier's death.
    /// The game's sound table (`G_SoundIndex`), published as configstrings.
    sounds: SoundTable,
    /// Whether a begin has registered the Force loops and the saber spin yet.
    begun_once: bool,
    /// Where players spawn.
    spawn_order: SpawnOrder,
    /// The siege level being played (`bridge_siege.rs`), and what outlives it.
    siege: Option<Box<bridge_siege::SiegeState>>,
    siege_keep: bridge_siege::SiegeKeep,
    /// The `func_usable` entities the map placed.
    usable_entities: Vec<(EntityId, sjk_game_jka::use_key::Usable)>,
    /// `g_gametype`, which decides which of the map's spawn points are used.
    gametype: i32,
    /// `level.intermissionQueued`, `level.intermissiontime` and the two ready timers:
    /// everything `CheckExitRules` keeps between frames.
    match_end: sjk_game_jka::match_end::MatchEnd,
    /// The limits it reads — `fraglimit`, `timelimit` and the rest — which an operator
    /// sets once and this server then honours every frame.
    limits: sjk_game_jka::match_end::Limits,
    /// `level.startTime`: when the match began, which the time limit measures from.
    level_start_time: i32,
    /// `level.teamScores[TEAM_RED]` and `[TEAM_BLUE]`.
    team_scores: [i32; 2],
    /// `level.lastTeamLocationTime`: when the team overlay was last sent.
    last_team_info: i32,
    /// The flags of a capture-the-flag game (`teamgame`).
    flags: sjk_game_jka::ctf::Flags,
    /// The ready mask last written to every scoreboard, so it is written once per change.
    ready_mask: i32,
    /// `sv.serverId`: which world this is. Every map change gives clients a number they
    /// have not seen, so that packets for the old world are recognised and dropped.
    map_generation: i32,
    /// The maps this server plays, in order, and where in them it is. Empty means it
    /// plays the one map it was started with, for ever.
    rotation: Vec<String>,
    at_map: usize,
    /// `sv.restartedServerId`: the first serverId of this world, which a `map_restart`
    /// keeps while `map_generation` moves on.
    restarted_generation: i32,
    /// `sv.restartTime`: when a delayed `map_restart` runs, 0 for none.
    restart_at: i32,
    /// `killserver` stopped the server; a map starts it again.
    stopped: bool,
    /// The bot definitions and the spawn queue.
    bots: bridge_bots::Bots,
    /// Where the next map is read from. `None` on a server with no game data, which
    /// runs mapless and changes map without loading anything.
    game_data: Option<std::path::PathBuf>,
    /// Whether `CalculateRanks` is already running, so that the exit rules it ends with
    /// cannot rank again from inside themselves.
    ranking: bool,
    /// The exit rules' view of the players, kept between frames: `run_match_end` runs
    /// every frame, and a per-frame `Vec` is a per-frame allocation.
    contending: Vec<sjk_game_jka::match_end::Contender>,
    /// Player skeletons already loaded, by model.
    skeletons: std::collections::HashMap<
        String,
        std::sync::Arc<sjk_game_jka::server_skeleton::SkeletonModels>,
    >,
    /// The saber traces' reusable collision buffers.
    saber_work: bridge_saber_damage::SaberWork,
    /// `level.voteTime` and everything else the level keeps about a vote.
    vote: sjk_game_jka::vote::Vote,
    /// `g_allowVote` (already limited to what this server can carry out) and `g_voteDelay`.
    vote_rules: (i32, i32),
    /// Passed votes' commands, carried out at the start of the next frame as the
    /// reference's console buffer runs them between frames.
    vote_commands: Vec<Vec<u8>>,
    map: Option<LoadedMap>,
    /// Saber definitions set by the host over the map's game data's (a server without
    /// game data, a test).
    saber_definitions: Option<std::sync::Arc<sjk_game_jka::saber_definition::SaberParms>>,
    /// Jedi Master's saber and masters (`bridge_jedimaster`); the words a broadcast
    /// update is written into, kept to be reused.
    jedi_master: bridge_jedimaster::JediMaster,
    broadcast_scratch: Vec<u64>,
    /// Holocron FFA's holocrons (`bridge_holocron`), and a power duel's own state
    /// (`bridge_power_duel`).
    holocrons: Vec<sjk_game_jka::holocron::Holocron>,
    power_duel: bridge_power_duel::PowerDuel,
    /// The console variables (`cvar.cpp`), and the change counts last put into effect.
    cvars: crate::cvars::Cvars,
    cvars_seen: Vec<(Vec<u8>, i32)>,
    /// The registration's first look is done: changes are told and announced from now.
    cvars_primed: bool,
    cvars_generation: u64,
    /// The game's IP filter (`addip`, `g_banIPs`).
    ip_filter: sjk_game_jka::ip_filter::IpFilter,
    /// `level.logFile`: `g_log`, open while a level runs.
    log_file: Option<std::fs::File>,
    /// Server demo files, by wire client number.
    demo_files: Vec<Option<std::fs::File>>,
    /// The console's command buffer (`exec`, `vstr`, `wait`, typed lines).
    commands: crate::command_buffer::CommandBuffer,
    /// Where `exec` looks and the archived variables are written.
    config_files: crate::config_files::ConfigFiles,
    /// The level's NPCs and their spawners (`bridge_npcs`).
    npcs: bridge_npcs::Npcs,
    /// Its emplaced guns and who is at them (`bridge_emplaced`).
    mounted: bridge_use::bridge_emplaced::Mounted,
    /// Its effect runners and speakers, and the effect and soundset tables (`bridge_map_effects`).
    map_effects: bridge_map_effects::MapEffects,
    /// Its turrets (`bridge_map_turrets`).
    map_turrets: bridge_map_turrets::MapTurrets,
    /// Its logic entities, named entities and path movers (`bridge_map_logic`).
    stock: bridge_map_logic::StockEntities,
    /// The scripts (ICARUS) and the level's script entities.
    scripts: bridge_icarus::Scripts,
}

/// The serverId a world starts on. `SV_SpawnServer` uses `com_frameTime`, which is only
/// ever "a number this client has not seen"; a server that counts its worlds gives the
/// same guarantee and is reproducible, which the fixtures want.
const SERVER_ID: i32 = 1;
/// `FP_SABER_DEFENSE` in `fd.forcePowerLevel`.
const FP_SABER_DEFENSE: usize = 16;
/// The missile's trajectory fields a block check reads.
const ES_POS_DELTA: [usize; 3] = [6, 7, 10];
const ES_POS_TYPE: usize = 23;
/// A mover's own wire fields: its type, where its trajectory starts and how long it is.
const ES_ENTITY_TYPE: usize = 8;
const ES_POS_BASE: [usize; 3] = [2, 1, 4];
const ES_POS_TIME: usize = 0;
const ES_POS_DURATION: usize = 20;
/// `s.time`, which a mover stamps when it sets off.
const ES_TIME: usize = 65;
/// `s.eFlags`.
const ES_EFLAGS: usize = 19;
/// `s.health` and `s.maxhealth`, which a `showhealth` brush puts a bar on a HUD with.
const ES_HEALTH: usize = 69;
const ES_MAX_HEALTH: usize = 73;
/// `statIndex_t`'s `STAT_HEALTH`, `STAT_HOLDABLE_ITEMS`, `STAT_WEAPONS`, `STAT_ARMOR` and
/// `STAT_MAX_HEALTH`, and `ps.weapon`.
const STAT_HEALTH: usize = 0;
const STAT_HOLDABLE_ITEMS: usize = 2;
const STAT_WEAPONS: usize = 4;
const STAT_ARMOR: usize = 5;
const STAT_MAX_HEALTH: usize = 8;
const PS_WEAPON: usize = 47;
const ES_WEAPON: usize = 14;

/// `r.absmin`, `r.absmax`: a linked box grown by a unit.
fn grown(obstacle: &BoxObstacle) -> ([f32; 3], [f32; 3]) {
    (
        std::array::from_fn(|axis| obstacle.origin[axis] + obstacle.bounds.0[axis] - 1.0),
        std::array::from_fn(|axis| obstacle.origin[axis] + obstacle.bounds.1[axis] + 1.0),
    )
}

impl NativeGame {
    /// One resident world whose `peer_capacity` is the core's own budget.
    pub fn new(
        identity: Identity,
        map: Option<LoadedMap>,
        peer_capacity: usize,
        wire_clients: usize,
    ) -> Result<Self, CreateWorldError> {
        Self::with_cvars(
            identity,
            map,
            peer_capacity,
            wire_clients,
            crate::cvars::Cvars::server(),
        )
    }

    /// [`Self::new`] over console variables the command line and the startup configs
    /// already set, and the server's registrations already made
    /// ([`crate::cvars::Cvars::register_server`]).
    pub fn with_cvars(
        identity: Identity,
        map: Option<LoadedMap>,
        peer_capacity: usize,
        wire_clients: usize,
        cvars: crate::cvars::Cvars,
    ) -> Result<Self, CreateWorldError> {
        let mut server = Server::new(1);
        let world = server.create_world((), peer_capacity)?;
        let mut status_info = b"\\sv_hostname\\".to_vec();
        status_info.extend_from_slice(&identity.hostname);
        status_info.extend_from_slice(b"\\mapname\\");
        status_info.extend_from_slice(&identity.mapname);
        // The `CVAR_SERVERINFO` cvars a legacy cgame reads from `CS_SERVERINFO`
        // (`CG_ParseServerinfo`), at the reference's defaults. `sv_maxclients` is the
        // protocol-26 roster — the cgame sizes its client arrays from it — not the
        // core's capacity, which the browser's `getinfo` reports.
        status_info.extend_from_slice(format!("\\sv_maxclients\\{wire_clients}").as_bytes());
        for (key, value) in SERVERINFO_DEFAULTS {
            status_info.extend_from_slice(format!("\\{key}\\{value}").as_bytes());
        }
        let (config_strings, sounds) =
            world_config_strings(&status_info, map.as_ref(), GAMETYPE_FFA, SERVER_ID);
        let mut game = Self {
            server,
            world,
            identity,
            status_info,
            players: crate::players::PlayerRoster::new(wire_clients),
            config_strings,
            told: Vec::new(),
            settings: SETTINGS,
            obstacles: Vec::with_capacity(wire_clients),
            body_legs: Vec::with_capacity(wire_clients),
            force_scratch: bridge_force::ForceScratch::default(),
            duel_slot: PlayerState::zero(),
            candidates: Vec::with_capacity(wire_clients),
            everyone: Vec::with_capacity(wire_clients),
            incoming: Vec::new(),
            rand: CrtRand::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(1, |since| since.as_secs() as u32),
            ),
            pool: EntityPool::new(0),
            pool_links: Default::default(),
            last_frame_time: 0,
            missiles: Vec::new(),
            fired: Vec::new(),
            husks: Vec::new(),
            homing: Vec::with_capacity(wire_clients),
            splash_targets: Vec::with_capacity(wire_clients),
            spheres: Vec::new(),
            charges: Vec::new(),
            items: Vec::new(),
            triggers: Vec::new(),
            movers: Vec::new(),
            multiples: Vec::new(),
            targets: Vec::new(),
            doors: Vec::new(),
            door_portals: Vec::new(),
            breakables: Vec::new(),
            models: ModelTable::default(),
            previous_frame_time: 0,
            deaths: Deaths {
                rng: Rng(0x89ab_cdef),
                next_death_event: 0,
            },
            avoid_dismember: false,
            lock_obstacles: Vec::with_capacity(wire_clients),
            dead_sabers_waiting: Vec::new(),
            sounds,
            begun_once: false,
            spawn_order: SpawnOrder::default(),
            siege: None,
            siege_keep: Default::default(),
            usable_entities: Vec::new(),
            gametype: GAMETYPE_FFA,
            match_end: Default::default(),
            limits: Default::default(),
            level_start_time: 0,
            team_scores: [0; 2],
            last_team_info: 0,
            flags: Default::default(),
            ready_mask: 0,
            ranking: false,
            contending: Vec::with_capacity(wire_clients),
            map_generation: SERVER_ID,
            restarted_generation: SERVER_ID,
            restart_at: 0,
            stopped: false,
            bots: Default::default(),
            rotation: Vec::new(),
            at_map: 0,
            game_data: None,
            skeletons: Default::default(),
            saber_work: Default::default(),
            vote: Default::default(),
            vote_rules: (
                bridge_votes::SUPPORTED_VOTES,
                sjk_game_jka::vote::DEFAULT_VOTE_DELAY,
            ),
            vote_commands: Vec::new(),
            map,
            saber_definitions: None,
            jedi_master: Default::default(),
            broadcast_scratch: Vec::new(),
            holocrons: Vec::new(),
            power_duel: Default::default(),
            cvars,
            cvars_seen: Vec::new(),
            cvars_primed: false,
            cvars_generation: 0,
            commands: Default::default(),
            config_files: Default::default(),
            ip_filter: Default::default(),
            log_file: None,
            demo_files: Vec::new(),
            npcs: Default::default(),
            mounted: Default::default(),
            map_effects: Default::default(),
            map_turrets: Default::default(),
            stock: Default::default(),
            scripts: Default::default(),
        };
        game.pool = EntityPool::with_budget(0, game.entity_budget());
        game.spawn_items();
        game.spawn_triggers();
        game.spawn_movers();
        game.spawn_multiples();
        game.spawn_doors();
        game.spawn_breakables();
        game.init_siege(0);
        game.spawn_usables();
        game.spawn_emplaced();
        game.spawn_map_effects();
        game.spawn_map_turrets();
        game.spawn_stock_entities();
        game.spawn_npcs();
        game.spawn_scripts();
        // What the process already knows, over the registrations' defaults; cheats are
        // off until a `devmap`, and this server runs twenty frames a second unless told
        // otherwise.
        let hostname = String::from_utf8_lossy(&game.identity.hostname).into_owned();
        let mapname = String::from_utf8_lossy(&game.identity.mapname).into_owned();
        for (name, value) in [
            ("sv_hostname", hostname.as_str()),
            ("mapname", &mapname),
            ("sv_maxclients", &wire_clients.to_string()),
            ("sv_cheats", "0"),
        ] {
            game.cvars.set(name.as_bytes(), value.as_bytes());
        }
        if game
            .cvars
            .var(b"sv_fps")
            .is_some_and(|var| var.string == var.reset)
        {
            game.cvars.set(b"sv_fps", b"20");
        }
        game.apply_cvars();
        game.process_ip_bans();
        Ok(game)
    }

    /// `SP_trigger_hurt` for every hurt brush the map placed: the inline model's bounds
    /// (`trap->SetBrushModel`) and a trigger nobody is told about.
    fn spawn_triggers(&mut self) {
        let Some(map) = self.map.take() else { return };
        for placed in &map.hurt_triggers {
            let Some(bounds) = crate::map::brush_bounds(&map.bsp, placed.model) else {
                continue;
            };
            let trigger = sjk_game_jka::triggers::spawn_hurt(placed, bounds);
            // The entity exists so that its number is the map's; no client ever sees it.
            if let Some(number) = self.pool.spawn_hidden(0) {
                self.triggers.push((number, trigger));
            }
        }
        self.map = Some(map);
    }

    /// `G_TouchTriggers` for a living player: the hurt brushes it stands in take their
    /// health at their own rate, or begin the fall to death a pit's brush is.
    fn touch_triggers(&mut self, client: usize, level_time: i32) {
        if self
            .peer_mut(client)
            .is_none_or(|peer| !sjk_game_jka::triggers::may_touch(peer.health, !peer.playing()))
        {
            return;
        }
        for index in 0..self.triggers.len() {
            let (number, trigger) = &self.triggers[index];
            let (number, linked) = (number.legacy_number(), trigger.linked);
            if !linked {
                continue;
            }
            let Some(peer) = self.peer_mut(client) else {
                return;
            };
            let (origin, bounds) = (peer.state.origin(), peer.movement.box_bounds());
            let (health, take_damage) = (peer.health, peer.health > 0);
            let (_, trigger) = &self.triggers[index];
            if !sjk_game_jka::triggers::near(origin, trigger) {
                continue;
            }
            // `trap->EntityContact` against the brush model: the player's box, where the
            // move left it, traced against that model's own brushes.
            let contact = {
                let Some(map) = self.map.as_ref() else { return };
                let low: [f32; 3] = std::array::from_fn(|axis| origin[axis] + bounds.0[axis]);
                let high: [f32; 3] = std::array::from_fn(|axis| origin[axis] + bounds.1[axis]);
                let middle: [f32; 3] = std::array::from_fn(|axis| (low[axis] + high[axis]) / 2.0);
                let half: [f32; 3] = std::array::from_fn(|axis| (high[axis] - low[axis]) / 2.0);
                let Ok(box_bounds) = sjk_bsp::Aabb::new(half.map(|value| -value), half) else {
                    continue;
                };
                map.bsp
                    .trace_model_box(trigger.model, middle, middle, box_bounds, u32::MAX)
                    .start_solid
            };
            if !contact {
                continue;
            }
            let (_, trigger) = &mut self.triggers[index];
            let mut trigger = trigger.clone();
            let Some(peer) = self.peer_mut(client) else {
                return;
            };
            let touched = sjk_game_jka::triggers::hurt_touch(
                &mut trigger,
                &mut peer.state,
                health,
                take_damage,
                level_time,
            );
            peer.movement = peer.movement.reseeded(&peer.state);
            self.triggers[index].1 = trigger;
            match touched {
                sjk_game_jka::triggers::Touched::Nothing => {}
                sjk_game_jka::triggers::Touched::Hurt {
                    damage,
                    flags,
                    means,
                } => {
                    // `G_Damage(other, self, self, ...)`: the brush itself, which is no
                    // client, is the attacker.
                    let brush = Attacker {
                        npc: false,
                        client: number,
                        max_health: 100,
                        team: 0,
                        saber_knockback: [0.0; 4],
                    };
                    let request = DamageRequest {
                        level_time,
                        attacker: Some(brush),
                        direction: None,
                        point: None,
                        damage,
                        flags,
                        means,
                    };
                    let _ = self.hurt(client, request);
                }
                sjk_game_jka::triggers::Touched::Fade { sound, channel } => {
                    let origin = self
                        .peer_mut(client)
                        .map(|peer| peer.state.origin())
                        .unwrap_or_default();
                    let (sounds, told) = (&mut self.sounds, &mut self.told);
                    let index = sounds.index(sound.as_bytes(), &mut |index, value| {
                        told.push(Told::ConfigString {
                            index,
                            previous: Vec::new(),
                            value: value.to_vec(),
                        })
                    });
                    let mut event =
                        sjk_game_jka::knockdown::entity_sound(origin, client as u16, channel);
                    event.parameter = u32::from(index);
                    let _ = self.pool.spawn_temporary(event.state(), level_time, None);
                }
                sjk_game_jka::triggers::Touched::Respawn => {}
            }
        }
    }

    /// `SP_trigger_push` and `SP_trigger_teleport` for the map's own: brush entities
    /// every client is sent, because every client predicts a jump pad's throw itself.
    fn spawn_movers(&mut self) {
        let Some(map) = self.map.take() else { return };
        for (placed, teleports, destination, angles) in &map.movers {
            let Some(bounds) = crate::map::brush_bounds(&map.bsp, placed.model) else {
                continue;
            };
            let mut mover = if *teleports {
                sjk_game_jka::triggers::spawn_teleport(placed, bounds)
            } else {
                sjk_game_jka::triggers::spawn_push(placed, bounds, 0)
            };
            // `AimAtTarget` a frame on; a pad that cannot throw at its target is freed,
            // as the reference frees it.
            if mover.aim_at != 0 {
                let Some(velocity) = sjk_game_jka::triggers::aim_at_target(
                    &mover,
                    *destination,
                    sjk_game_jka::triggers::GRAVITY,
                ) else {
                    continue;
                };
                mover.velocity = velocity;
                mover.aim_at = 0;
            }
            let mut state = EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
            state.set_raw_field(sjk_game_jka::triggers::ES_TYPE, mover.kind);
            sjk_game_jka::triggers::set_brush_model(&mut state, mover.model);
            for axis in 0..3 {
                state.set_raw_field(
                    sjk_game_jka::triggers::ES_ORIGIN2[axis],
                    mover.velocity[axis].to_bits(),
                );
            }
            if let Some(number) = self.pool.spawn_entity(state, 0) {
                self.movers.push((number, mover, *destination, *angles));
            }
        }
        self.map = Some(map);
    }

    /// The breakable brushes the map placed, each an `ET_MOVER` standing where the map
    /// drew it — which is all a client needs to draw the brush and to watch it go.
    fn spawn_breakables(&mut self) {
        let Some(map) = self.map.take() else { return };
        for brush in &map.breakables {
            let mut state = EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
            state.set_raw_field(ES_ENTITY_TYPE, sjk_game_jka::breakables::ET_MOVER);
            sjk_game_jka::triggers::set_brush_model(&mut state, brush.model);
            // `G_ScaleNetHealth`: a `showhealth` brush carries what a HUD draws a bar from.
            if brush.max_health > 0 {
                state.set_raw_field(ES_HEALTH, brush.health.max(0) as u32);
                state.set_raw_field(ES_MAX_HEALTH, brush.max_health as u32);
            }
            if let Some(number) = self.pool.spawn_entity(state, 0) {
                self.pool.set_bounds(number, brush.bounds);
                self.breakables.push((number, brush.clone()));
            }
        }
        self.map = Some(map);
    }

    /// A shot that landed on a breakable brush (`G_Damage` for a non-client entity, which
    /// is the one thing on this server that is neither a player nor a missile). Returns
    /// whether it was one of ours at all, so the caller can fall back to a player.
    fn hurt_brush(
        &mut self,
        number: u16,
        damage: i32,
        means: u32,
        owner: u16,
        level_time: i32,
    ) -> bool {
        if self.hurt_glass(number, damage, owner, level_time) {
            return true;
        }
        let Some(index) = self
            .breakables
            .iter()
            .position(|(ours, _)| ours.legacy_number() == number)
        else {
            return false;
        };
        let from = self
            .peer_mut(usize::from(owner))
            .map(|peer| peer.state.origin());
        let hit = sjk_game_jka::breakables::hurt(
            &mut self.breakables[index].1,
            damage,
            means,
            false,
            from,
            level_time,
        );
        match hit {
            sjk_game_jka::breakables::Hit::Ignored => {}
            sjk_game_jka::breakables::Hit::Hurt { paintarget, .. } => {
                self.publish_breakable(index);
                if let Some(target) = paintarget {
                    self.fire_targets(&target, usize::from(owner), level_time);
                }
            }
            // The delay is the map's own; without one it comes apart in the same frame.
            sjk_game_jka::breakables::Hit::Broken { at } if at > level_time => {
                self.publish_breakable(index)
            }
            sjk_game_jka::breakables::Hit::Broken { .. } => {
                self.break_brush(index, from, level_time)
            }
        }
        true
    }

    /// `funcBBrushDieGo` on the server: the brush leaves the world, fires what it targets,
    /// throws its fireball and its debris as the two event entities a client draws them
    /// from, hurts whoever is close, and is freed.
    fn break_brush(&mut self, index: usize, from: Option<[f32; 3]>, level_time: i32) {
        let (id, number) = (
            self.breakables[index].0,
            self.breakables[index].0.legacy_number(),
        );
        // `Q_flrand(0.0f, 1.0f)`, which decides how many chunks there are.
        let drawn = self.deaths.rng.flrand(0.0, 1.0);
        let broken = sjk_game_jka::breakables::break_apart(
            &mut self.breakables[index].1,
            from,
            number,
            drawn,
            level_time,
        );
        // Worth saying out loud: it is the one thing on this server that dies without
        // being a player, and an operator watching a log wants to see it happen.
        println!(
            "breakable {number} broken at {level_time} ({} chunks, splash {:?})",
            broken.debris.chunks,
            broken.splash.map(|(damage, radius, _)| (damage, radius))
        );
        if let Some(target) = &broken.target {
            self.fire_targets(target, 0, level_time);
        }
        if let Some(explosion) = &broken.explosion {
            self.raise_event(
                sjk_game_jka::breakables::explosion_fields(explosion),
                level_time,
            );
        }
        self.raise_event(
            sjk_game_jka::breakables::debris_fields(&broken.debris),
            level_time,
        );
        if let Some((damage, radius, at)) = broken.splash {
            self.splash_from_brush(number, damage, radius, at, level_time);
        }
        // The portal it stood in opens (`funcBBrushDieGo`, `g_mover.c:2514`).
        let bounds = self.breakables[index].1.bounds;
        self.adjust_portal([0.0; 3], bounds, true);
        self.pool.free(id, level_time);
        self.breakables.remove(index);
    }

    /// One event entity, built from the wire fields the reference crams its debris and
    /// its explosions into.
    fn raise_event(&mut self, fields: Vec<(usize, u32)>, level_time: i32) {
        let mut state = EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
        for (index, bits) in fields {
            state.set_raw_field(index, bits);
        }
        let _ = self.pool.spawn_temporary(state, level_time, None);
    }

    /// `G_RadiusDamage` from a brush that went off: everybody within its radius, hurt by
    /// how close they were, with the brush itself as the attacker.
    fn splash_from_brush(
        &mut self,
        number: u16,
        damage: i32,
        radius: i32,
        at: [f32; 3],
        level_time: i32,
    ) {
        for client in 0..self.players.places() {
            let Some(peer) = self.peer_mut(client) else {
                continue;
            };
            if !peer.playing() || peer.health <= 0 {
                continue;
            }
            let origin = peer.state.origin();
            let away: f32 = (0..3)
                .map(|axis| (origin[axis] - at[axis]).powi(2))
                .sum::<f32>()
                .sqrt();
            if away >= radius as f32 {
                continue;
            }
            // `G_RadiusDamage`'s own falloff: full at the middle, nothing at the edge.
            let points = (damage as f32 * (1.0 - away / radius as f32)) as i32;
            if points <= 0 {
                continue;
            }
            let attacker = Attacker {
                npc: false,
                client: number,
                max_health: 100,
                team: 0,
                saber_knockback: [0.0; 4],
            };
            let request = DamageRequest {
                level_time,
                attacker: Some(attacker),
                direction: None,
                point: Some(at),
                damage: points,
                flags: 0,
                means: 0,
            };
            let _ = self.hurt(client, request);
        }
    }

    /// What a client is told of a brush that is still standing: the health bar, if the
    /// map asked for one.
    fn publish_breakable(&mut self, index: usize) {
        let (number, brush) = &self.breakables[index];
        let (number, health, max_health) = (*number, brush.health.max(0), brush.max_health);
        if max_health <= 0 {
            return;
        }
        let Some(slot) = self.pool.state_mut(number) else {
            return;
        };
        slot.set_raw_field(ES_HEALTH, health as u32);
    }

    /// `G_TouchTriggers` for the triggers that move a player: the jump pad's throw (the
    /// movement's own, which the client predicts too), a pusher's push, and a
    /// teleporter's jump to where it points.
    fn touch_movers(&mut self, client: usize, level_time: i32) {
        let spectating = self.peer_mut(client).is_none_or(|peer| !peer.playing());
        if self
            .peer_mut(client)
            .is_none_or(|peer| !sjk_game_jka::triggers::may_touch_as_spectator(peer.health))
        {
            return;
        }
        let mut on_a_pad = false;
        for index in 0..self.movers.len() {
            let (number, mover, destination, angles) = self.movers[index].clone();
            // A spectator touches teleporters and nothing else here.
            if !mover.touches
                || (spectating && mover.kind != sjk_game_jka::triggers::ET_TELEPORT_TRIGGER)
            {
                continue;
            }
            let Some(peer) = self.peer_mut(client) else {
                return;
            };
            let (origin, bounds) = (peer.state.origin(), peer.movement.box_bounds());
            let movement_type = peer.state.movement_type();
            let contact = {
                let Some(map) = self.map.as_ref() else { return };
                let low: [f32; 3] = std::array::from_fn(|axis| origin[axis] + bounds.0[axis]);
                let high: [f32; 3] = std::array::from_fn(|axis| origin[axis] + bounds.1[axis]);
                let middle: [f32; 3] = std::array::from_fn(|axis| (low[axis] + high[axis]) / 2.0);
                let half: [f32; 3] = std::array::from_fn(|axis| (high[axis] - low[axis]) / 2.0);
                let Ok(box_bounds) = sjk_bsp::Aabb::new(half.map(|value| -value), half) else {
                    continue;
                };
                map.bsp
                    .trace_model_box(mover.model, middle, middle, box_bounds, u32::MAX)
                    .start_solid
            };
            if !contact {
                continue;
            }
            if mover.kind == sjk_game_jka::triggers::ET_TELEPORT_TRIGGER {
                if !sjk_game_jka::triggers::may_teleport(&mover, movement_type, spectating) {
                    continue;
                }
                self.teleport_client(client, origin, destination, angles, spectating, level_time);
                continue;
            }
            if sjk_game_jka::triggers::is_jump_pad(&mover) {
                let Some(peer) = self.peer_mut(client) else {
                    return;
                };
                let pad = sjk_game_jka::triggers::JumpPad {
                    number: number.legacy_number(),
                    velocity: mover.velocity,
                };
                if sjk_game_jka::triggers::touch_jump_pad(&mut peer.state, pad, movement_type, 0) {
                    on_a_pad = true;
                    peer.movement = peer.movement.reseeded(&peer.state);
                }
                continue;
            }
            let Some(peer) = self.peer_mut(client) else {
                return;
            };
            let state = peer.state.clone();
            let mover = &mut self.movers[index].1;
            if let Some(velocity) =
                sjk_game_jka::triggers::pushed(mover, &state, movement_type, level_time)
            {
                let Some(peer) = self.peer_mut(client) else {
                    return;
                };
                peer.state.set_velocity(velocity);
                peer.movement = peer.movement.reseeded(&peer.state);
            }
        }
        // `G_TouchTriggers`' tail: a pad is remembered for one move only.
        if !on_a_pad
            && let Some(peer) = self.peer_mut(client)
            && peer
                .state
                .raw_field(sjk_game_jka::triggers::PS_JUMPPAD_ENT)
                .unwrap_or(0)
                != 0
        {
            peer.state
                .set_raw_field(sjk_game_jka::triggers::PS_JUMPPAD_ENT, 0);
            peer.movement = peer.movement.reseeded(&peer.state);
        }
    }

    /// `ClientThink_real`'s end of a fall to death (`g_active.c:2761-2787`): three seconds
    /// after a pit's brush began it, a player still alive is killed outright and its
    /// scream stopped.
    fn run_fall_to_death(&mut self, client: usize, level_time: i32) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        if !sjk_game_jka::triggers::fade_over(&peer.state, peer.health, level_time) {
            return;
        }
        // The player itself, unless somebody pushed, held or knocked it in recently
        // (`otherKiller`) and is still there.
        let origin = peer.state.origin();
        let credited = peer
            .wounds
            .other_killer
            .credited(level_time)
            .map(usize::from);
        let killer = credited
            .filter(|&killer| killer < self.players.places() && self.peer(killer).is_some())
            .unwrap_or(client);
        let Some(by) = self.peer(killer) else { return };
        let attacker = Attacker {
            npc: false,
            client: killer as u16,
            max_health: by.state.max_health(),
            team: by.session.team,
            saber_knockback: [0.0; 4],
        };
        let request = DamageRequest {
            level_time,
            attacker: Some(attacker),
            direction: None,
            point: Some(origin),
            damage: 9_999,
            flags: sjk_game_jka::triggers::DAMAGE_NO_PROTECTION,
            means: MOD_FALLING,
        };
        let _ = self.hurt(client, request);
        let _ = self.pool.spawn_temporary(
            sjk_game_jka::triggers::scream_muted(client as u16).state(),
            level_time,
            None,
        );
    }

    /// `G_TouchTriggers` for a living player: every item it is near and may take is
    /// picked up — what it gives, the pickup event (predicted, as every client here asks
    /// for), and the item out of the world until it comes again.
    fn touch_items(&mut self, client: usize, level_time: i32) {
        if self
            .peer_mut(client)
            .is_none_or(|peer| peer.health <= 0 || !peer.playing())
        {
            return;
        }
        for index in 0..self.items.len() {
            let (number, pickup) = &self.items[index];
            let (number, item, origin, contents, dropped) = (
                number.legacy_number(),
                pickup.item,
                pickup.origin,
                pickup.contents,
                pickup.dropped.map(|dropped| dropped.count),
            );
            if contents == 0 {
                continue;
            }
            let Some(peer) = self.peer_mut(client) else {
                return;
            };
            if !sjk_game_jka::items::touches(peer.state.origin(), origin) {
                continue;
            }
            // A flag (`Pickup_Team`); one returned or captured may have changed the items.
            if sjk_game_jka::items::ITEMS[item].kind == sjk_game_jka::items::Kind::Team {
                if self.touch_flag(client, index, level_time) {
                    break;
                }
                continue;
            }
            if sjk_game_jka::dropped_items::refuses(
                &mut self.items[index].1,
                client as u16,
                level_time,
            ) {
                continue;
            }
            let Some(peer) = self.peer_mut(client) else {
                return;
            };
            if !sjk_game_jka::items::can_be_grabbed(item, &peer.state, dropped.is_some()) {
                continue;
            }
            let taken = sjk_game_jka::items::pick_up(
                item,
                &mut peer.state,
                &mut peer.health,
                dropped.unwrap_or(0),
                dropped.is_some(),
            );
            if taken.predicted && peer.accepted.predict_item_pickup {
                sjk_game_jka::items::add_predictable_event(
                    &mut peer.state,
                    sjk_game_jka::items::EV_ITEM_PICKUP,
                    u32::from(number),
                );
            } else {
                let bits = (peer.state.raw_field(EFLAGS_EVENT).unwrap_or(0) & 0x300)
                    .wrapping_add(0x100)
                    & 0x300;
                peer.state
                    .set_raw_field(EFLAGS_EVENT, sjk_game_jka::items::EV_ITEM_PICKUP | bits);
                peer.state
                    .set_raw_field(EFLAGS_EVENT_PARM, u32::from(number));
                peer.entity.event_raised(level_time);
            }
            peer.movement = peer.movement.reseeded(&peer.state);
            if dropped.is_some() {
                let (_, pickup) = &mut self.items[index];
                sjk_game_jka::dropped_items::taken(pickup, taken.respawn, level_time);
            } else {
                let spread = self.deaths.rng.flrand(-1.0, 1.0);
                let (_, pickup) = &mut self.items[index];
                let _ = sjk_game_jka::items::taken(pickup, taken.respawn, level_time, spread);
            }
            let _ = number;
            self.publish_item(index);
        }
    }

    /// A spectator where the map starts them, looking the way its author chose, at
    /// the speed `SpectatorThink` gives one (g_active.c:705-706).
    fn spectator(&self, client: usize) -> Predictor {
        let start = self
            .map
            .as_ref()
            .and_then(|map| spectator_start(&map.spawn_points));
        let angles = start.map_or([0.0; 3], |start| start.angles);
        // A command's angles are relative to these; none has arrived yet.
        let to_short = |degrees: f32| ((degrees * 65_536.0 / 360.0) as i32) & 0xffff;
        Predictor::from_state(
            MovementState {
                client_num: client as u16,
                movement_type: PM_SPECTATOR,
                team: TEAM_SPECTATOR,
                health: 100,
                speed: 400.0,
                base_speed: 400.0,
                gravity: self.gravity(),
                standing_height: 40.0,
                crouching_height: 16.0,
                ground_entity_number: 1_023,
                origin: start.map_or([0.0; 3], |start| start.origin),
                view_angles: angles,
                delta_angles: angles.map(to_short),
                ..MovementState::default()
            },
            authoritative(),
        )
    }

    /// `G_MissileImpact`'s splash (`G_RadiusDamage` from where the missile stands, the one
    /// struck spared): the accuracy counted once if the direct hit did not count.
    fn splash(&mut self, missile: &Missile, spared: Option<u16>, counted: bool, level_time: i32) {
        self.splash_as(
            missile,
            missile.current,
            spared,
            counted,
            missile.splash_method_of_death,
            level_time,
        );
    }

    /// [`Self::splash`] from `origin` under another means of death (`laserTrapExplode`'s
    /// blow, from where the grenade struck before the hit event snapped it).
    fn splash_as(
        &mut self,
        missile: &Missile,
        origin: [f32; 3],
        spared: Option<u16>,
        counted: bool,
        means: u32,
        level_time: i32,
    ) {
        // `ent->parent`, and "say my pilot did it" of a vehicle's (`G_RadiusDamage`).
        let credited = self.npcs.roster.splash_credit(missile.splash_attacker());
        let (owner, Some(attacker)) = (
            usize::from(credited),
            self.attacker_for(credited)
                .filter(|_| missile.splash_damage != 0),
        ) else {
            return;
        };
        self.gather_splash_targets();
        let targets = std::mem::take(&mut self.splash_targets);
        // The map is set aside while the explosion's lines are traced through it and
        // the players hurt: `CanDamage` sees the world alone.
        let map = self.map.take();
        let mut hurt = |number: u16, request: DamageRequest| {
            self.strike(owner, usize::from(number), request, false).1
        };
        let hit_client = match &map {
            Some(map) => {
                let world = WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                };
                radius_damage(
                    origin,
                    Some(attacker),
                    missile.splash_damage as f32,
                    missile.splash_radius,
                    spared,
                    means,
                    level_time,
                    &targets,
                    &world,
                    &mut hurt,
                )
            }
            None => radius_damage(
                origin,
                Some(attacker),
                missile.splash_damage as f32,
                missile.splash_radius,
                spared,
                means,
                level_time,
                &targets,
                &Void,
                &mut hurt,
            ),
        };
        self.map = map;
        self.flush_npc_blows();
        self.splash_targets = targets;
        if hit_client
            && !counted
            && let Some(shooter) = self.peer_mut(owner)
        {
            shooter.accuracy.0 += 1;
        }
    }

    /// The map this server is running, which a map change rewrites.
    pub fn mapname(&self) -> &[u8] {
        &self.identity.mapname
    }

    /// How the match is ending, or whether it is: the process reads it to decide what to
    /// log, and the tests to see the rule reach the wire.
    pub fn match_end(&self) -> sjk_game_jka::match_end::MatchEnd {
        self.match_end
    }

    /// The game's settings, for the process to set from its options before anyone connects.
    pub fn settings_mut(&mut self) -> &mut ForceServerSettings {
        &mut self.settings
    }

    /// One configstring set and told, which several siege things need.
    fn publish_config_string(&mut self, index: usize, value: &[u8]) {
        let previous = self
            .config_strings
            .iter()
            .find(|(known, _)| *known == index)
            .map_or_else(Vec::new, |(_, value)| value.clone());
        match self
            .config_strings
            .iter_mut()
            .find(|(known, _)| *known == index)
        {
            Some(slot) => slot.1 = value.to_vec(),
            // In index order: the gamestate interleaves the players' strings by it.
            None => self.config_strings.insert(
                self.config_strings
                    .partition_point(|(known, _)| *known < index),
                (index, value.to_vec()),
            ),
        }
        self.told.push(Told::ConfigString {
            index,
            previous,
            value: value.to_vec(),
        });
    }

    /// `CheckExitRules` on this server (`sjk_game_jka::match_end`): whether a limit has
    /// been hit, the second before the scoreboard, the scoreboard itself, and the exit.
    ///
    /// The reference runs it from `G_RunFrame` *and* from `CalculateRanks`
    /// (`g_main.c:1193`), so a score or a team change ends a match between frames rather
    /// than at the next one; this server calls it from both places for the same reason.
    fn run_match_end(&mut self, server_time: i32) {
        use sjk_game_jka::match_end::ExitStep;
        if self.ranking {
            return;
        }
        let mut contenders = std::mem::take(&mut self.contending);
        self.end_contenders(&mut contenders);
        let step = sjk_game_jka::match_end::check_exit_rules(
            &self.limits,
            &contenders,
            self.team_scores,
            server_time,
            self.level_start_time,
            &mut self.match_end,
        );
        contenders.clear();
        self.contending = contenders;
        // A duel's round has ended, two seconds into its intermission.
        if std::mem::take(&mut self.match_end.duel_round_due) {
            self.duel_round_ends(server_time);
        }
        match step {
            ExitStep::Playing => {}
            ExitStep::Logged(ending) => {
                self.log_exit(ending.reason);
                // `LogExit` tells every client at once, a second before the scoreboard,
                // so that they can stop the sounds the intermission would cut off.
                self.publish_config_string(sjk_game_jka::match_end::CS_INTERMISSION, b"1");
                for print in self.end_prints(ending.announce) {
                    self.told.push(Told::Everyone(print));
                }
            }
            ExitStep::BeginIntermission => {
                self.begin_intermission(server_time);
            }
            ExitStep::Ready { mask } => {
                // What each client is actually being sent while the scoreboard is up,
                // once a second, for anyone chasing an intermission that looks wrong on
                // a client. It was built to settle exactly that and it settled it: the
                // probe's first failure was its own `viewpos` timing, not the server.
                if std::env::var_os("SJK_INTERMISSION_TRACE").is_some() && server_time % 1000 < 50 {
                    for client in 0..self.players.places() {
                        if let Some(peer) = self.peer(client) {
                            println!(
                                "intermission trace {server_time} client {client}: origin {:?} angles {:?} pm_type {} cmdtime {} ready {}",
                                peer.state.origin(),
                                peer.state.view_angles(),
                                peer.state.movement_type(),
                                peer.state.command_time(),
                                peer.ready_to_exit
                            );
                        }
                    }
                }
                if mask != self.ready_mask {
                    self.ready_mask = mask;
                    for client in 0..self.players.places() {
                        if let Some(peer) = self.peer_mut(client) {
                            peer.state.stats[sjk_game_jka::match_end::STAT_CLIENTS_READY] =
                                mask as u32;
                        }
                    }
                }
            }
            ExitStep::ExitLevel { .. } => self.exit_level(server_time),
        }
    }

    pub fn set_spawn_order(&mut self, order: SpawnOrder) {
        self.spawn_order = order;
    }

    /// `ClientBegin`: the Force is read, the player may be sent to the spectators, it
    /// spawns, and the spawn's own think drops it to the floor. `entering` is the command
    /// a client enters the world with; a team change thinks with the state's own time.
    fn begin(&mut self, client: usize, server_time: i32, entering: Option<&UserCommand>) {
        if self.power_duel_begin(client, server_time) {
            return;
        }
        if !std::mem::replace(&mut self.begun_once, true) {
            let (sounds, told) = (&mut self.sounds, &mut self.told);
            first_begin_sounds(sounds, &mut |index, value| {
                told.push(Told::ConfigString {
                    index,
                    previous: Vec::new(),
                    value: value.to_vec(),
                })
            });
        }
        // `ClientSpawn`'s saber check, whose stance the spawn below settles; a change runs
        // `ClientUserinfoChanged` again, whose advice follows the Force's commands.
        let advice = if self.spawn_sabers(client) {
            self.peer(client)
                .and_then(|peer| self.snaps_advice(&peer.userinfo))
        } else {
            None
        };
        self.siege_begins_spectating(client, server_time);
        self.gather_obstacles(client);
        let aim = entering
            .copied()
            .or_else(|| self.peer(client).map(|peer| peer.last_command))
            .unwrap_or_default();
        let siege_place = self.siege_spawn_place(client, &aim, server_time);
        let Self {
            server,
            world,
            players,
            map,
            settings,
            obstacles,
            pool,
            deaths,
            spawn_order,
            gametype,
            rand,
            ..
        } = self;
        let settings = *settings;
        let peer = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle));
        let Some(peer) = peer else { return };
        // `ClientBegin`: connected from here on, entered now.
        if !std::mem::replace(&mut peer.begun, true) {
            peer.enter_time = server_time;
        }
        let empty = AnimationLengthTable::new([]);
        let lengths = map.as_ref().and_then(|map| map.animations.clone());
        // A team change's spawn thinks with the last command received (`ClientThink`
        // with no command re-reads `pers.cmd`): stale, it moves nothing.
        if let Some(entering) = entering {
            peer.last_command = *entering;
        }
        let command = peer.last_command;
        // The player's saber entity (`WP_SaberInitBladeData`): allocated once, on the
        // server alone, never freed; the player's state names it from then on, and a
        // saber carrier whose state names none is one whose saber is out of its hand.
        if peer.saber_entity.is_none() {
            peer.saber_entity = pool.spawn_hidden(server_time);
        }
        init_saber_entity(peer, pool, server_time);
        // The Force may send the player to the spectators, which start elsewhere: the
        // place is chosen for the team the begin ends on.
        let mut trial = peer.session.clone();
        let lengths_ref: &dyn sjk_game_jka::AnimationLengths =
            lengths.as_deref().map_or(&empty, |lengths| lengths);
        // (The trial's place is not drawn: only the team it ends on matters.)
        let trial_place = SpawnPlace {
            origin: [0.0; 3],
            angles: [0.0; 3],
            level_time: server_time,
            command_angles: command.angles,
        };
        let _ = client_begin(
            client as u16,
            &mut trial,
            &peer.userinfo,
            &peer.accepted,
            settings,
            SPAWN_INVULNERABILITY,
            trial_place,
            None,
            lengths_ref,
        );
        let playing = trial.team != i32::from(TEAM_SPECTATOR);
        // `ClientBegin` clears the state before `ClientSpawn`: a join avoids the origin.
        let place = siege_place
            .as_ref()
            .map(|(place, _)| *place)
            .unwrap_or_else(|| {
                spawn_place(
                    *spawn_order,
                    *gametype,
                    &mut deaths.rng,
                    map.as_ref(),
                    obstacles,
                    playing,
                    [0.0; 3],
                    &command,
                    server_time,
                    Some((trial.team, true, rand)),
                    trial.duel_team,
                )
            });
        let mut begun = client_begin(
            client as u16,
            &mut peer.session,
            &peer.userinfo,
            &peer.accepted,
            settings,
            SPAWN_INVULNERABILITY,
            place,
            Some(&peer.state),
            lengths_ref,
        );
        begun.commands.extend(advice);

        self.spawned(client, begun, place, command, server_time);
        self.siege_point_fired(siege_place, client, server_time);
        self.log(&sjk_game_jka::game_log::client_begin(client));
    }

    /// `ClientRespawn` outside power duels and siege: the body is left behind
    /// (`MaintainBodyQueue`), then `ClientSpawn` again with the Force the begin decided.
    /// `level_time` is the last frame's, as `level.time` is.
    fn respawn(&mut self, client: usize, command: UserCommand, level_time: i32, server_time: i32) {
        if self.power_duel_respawn(client, server_time) {
            return;
        }
        if self.siege.is_some() {
            self.siege_client_respawn(client, level_time);
            return;
        }
        self.leave_body(client, level_time);
        self.respawn_spawn(client, command, level_time, server_time);
    }

    /// `ClientSpawn` again, the body already left: the Force the begin decided.
    fn respawn_spawn(
        &mut self,
        client: usize,
        command: UserCommand,
        level_time: i32,
        server_time: i32,
    ) {
        if self.spawn_sabers(client)
            && let Some(advice) = self
                .peer(client)
                .and_then(|peer| self.snaps_advice(&peer.userinfo))
        {
            self.told.push(Told::One(client, advice));
        }
        self.gather_obstacles(client);
        let siege_place = self.siege_spawn_place(client, &command, level_time);
        let Self {
            server,
            world,
            players,
            map,
            settings,
            obstacles,
            deaths,
            spawn_order,
            gametype,
            rand,
            ..
        } = self;
        let settings = *settings;
        let peer = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle));
        let Some(peer) = peer else { return };
        let empty = AnimationLengthTable::new([]);
        let lengths = map.as_ref().and_then(|map| map.animations.clone());
        let lengths_ref: &dyn sjk_game_jka::AnimationLengths =
            lengths.as_deref().map_or(&empty, |lengths| lengths);
        // A respawn avoids where the player died.
        let place = siege_place
            .as_ref()
            .map(|(place, _)| *place)
            .unwrap_or_else(|| {
                spawn_place(
                    *spawn_order,
                    *gametype,
                    &mut deaths.rng,
                    map.as_ref(),
                    obstacles,
                    peer.playing(),
                    peer.state.origin(),
                    &command,
                    level_time,
                    Some((peer.session.team, false, rand)),
                    peer.session.duel_team,
                )
            });
        let Some(begun) = client_respawn(
            client as u16,
            &mut peer.session,
            &peer.accepted,
            settings,
            SPAWN_INVULNERABILITY,
            place,
            &peer.state,
            lengths_ref,
        ) else {
            return;
        };
        self.spawned(client, begun, place, command, server_time);
        self.siege_point_fired(siege_place, client, server_time);
    }

    /// `MaintainBodyQueue`: the corpse's entity goes into the body queue and everyone is
    /// told to move the ragdoll over — or, where no body is made, to reset the corpse.
    fn leave_body(&mut self, client: usize, level_time: i32) {
        let Self {
            server,
            world,
            players,
            pool,
            told,
            ..
        } = self;
        let peer = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle));
        let Some(peer) = peer else { return };
        Self::leave_body_of(peer, pool, told, client, level_time);
    }

    fn leave_body_of(
        peer: &Peer,
        pool: &mut EntityPool,
        told: &mut Vec<Told>,
        client: usize,
        level_time: i32,
    ) {
        match body_left_behind(&peer.state, peer.entity.state(), level_time)
            .filter(|_| !peer.no_corpse)
        {
            Some(body) => {
                let number = pool.spawn_body(body.state.clone(), level_time);
                told.push(Told::Everyone(
                    body.command(client as u16, number.legacy_number()),
                ));
            }
            None => told.push(Told::Everyone(format!("rcg {client}").into_bytes())),
        }
    }

    /// `player_die`: the corpse, its events, the killer's score, the ranks and the
    /// scoreboard for the dead. `request` says who killed, how, and where the blow landed
    /// (a suicide: `DeathRequest::suicide`).
    fn die(&mut self, client: usize, mut request: DeathRequest) {
        let server_time = request.level_time;
        // Nobody dies at the intermission (a respawn's telefrag there decides nothing).
        if self.match_end.intermission_time != 0 {
            return;
        }
        if !self.power_duel_death_begins(server_time) {
            return;
        }
        self.eject_on_death(client, server_time);
        // A carrier's item drops (or goes home) at once; a hack ends.
        self.siege_carrier_gone(client, server_time);
        self.siege_hack_ends(client);
        if let Some(killer) = request
            .attacker
            .filter(|killer| usize::from(*killer) != client)
            && let Some(killer) = self.peer_mut(usize::from(killer))
        {
            request.killer_last_kill_time = killer.mortality.last_kill_time;
        }
        self.log_kill(client, request.attacker, request.means);
        request.gametype = self.gametype;
        request.duel_opponent = self.duel_opponent(client);
        request.jedi_master = self.jedi_masters(
            request
                .attacker
                .filter(|killer| usize::from(*killer) != client),
        );
        if let Some(peer) = self.peer_mut(client) {
            (request.command_weapon, request.in_space) = (
                peer.last_command.weapon,
                sjk_game_jka::vehicle_triggers::in_space(peer.in_space),
            );
        }
        let Self {
            server,
            world,
            players,
            map,
            pool,
            deaths,
            limits,
            ..
        } = self;
        let peer = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle));
        let Some(peer) = peer else { return };
        let empty = AnimationLengthTable::new([]);
        let lengths = map.as_ref().and_then(|map| map.animations.clone());
        let lengths_ref: &dyn sjk_game_jka::AnimationLengths =
            lengths.as_deref().map_or(&empty, |lengths| lengths);
        let score = peer.state.persistent[PERS_SCORE];
        let died = kill(
            &mut peer.state,
            peer.entity.state_mut(),
            &mut peer.mortality,
            deaths,
            request,
            lengths_ref,
        );
        // `AddScore` is held off during the warmup (`g_combat.c:469`), the dead's own too.
        peer.state.persistent[PERS_SCORE] = if limits.warmup_time != 0 {
            score
        } else {
            peer.state.persistent[PERS_SCORE]
        };
        peer.entity.event_raised(server_time);
        peer.health = died.entity_health;
        peer.movement = peer.movement.reseeded(&peer.state);
        peer.corpse = Some((died.contents, died.top));
        // `player_die` links the corpse at once: `CONTENTS_CORPSE`, nothing a client's
        // prediction clips against, in a box down to its top.
        let (bottom, top) = peer.movement.box_bounds();
        peer.entity
            .linked((bottom, [top[0], top[1], died.top]), false);
        for event in &died.events[..died.tossed_from] {
            let _ = pool.spawn_temporary(event.state(), server_time, None);
        }
        // `Team_FragBonuses` (with the flags it carried), a flag a suicide sends home,
        // then `TossClientItems`' event and items (a flag's status `FLAG_DROPPED`).
        let tossed_events = died.events[died.tossed_from..].to_vec();
        self.flag_death(
            client,
            request.attacker,
            died.carried_flags,
            died.returned_flag,
            server_time,
        );
        for event in &tossed_events {
            let _ = self.pool.spawn_temporary(event.state(), server_time, None);
        }
        for dropped in died.dropped {
            let item = dropped.item;
            if let Some(number) = self.pool.spawn_entity(dropped.state.clone(), server_time) {
                self.pool.set_bounds(number, dropped.bounds);
                self.items.push((number, dropped));
                self.flag_launched(item, server_time);
            }
        }
        self.dismember_on_death(client, &request, died.death_animation);
        // `AddScore` in a team game also scores the scorer's team: the killer's, or — for
        // a suicide or a death by the world — the dead's, a point less.
        let (scorer, points) = died.killer_score.map_or((client, -1), |score| {
            (usize::from(score.killer), score.points)
        });
        let scoring = self.scoring();
        if scoring && let Some(team) = self.peer_mut(scorer).map(|peer| peer.session.team) {
            self.add_team_score(team, points);
        }
        // `AddScore` for the killer (`PERS_EXCELLENT_COUNT` for a quick second kill), which
        // recalculates the ranks; `Cmd_Score_f` shows the dead its scoreboard.
        if let Some(score) = died.killer_score
            && usize::from(score.killer) != client
            && let Some(killer) = self.peer_mut(usize::from(score.killer))
        {
            killer.state.persistent[PERS_SCORE] = (killer.state.persistent[PERS_SCORE] as i32
                + if scoring { score.points } else { 0 })
                as u32;
            if score.excellent {
                killer.state.persistent[PERS_EXCELLENT_COUNT] =
                    killer.state.persistent[PERS_EXCELLENT_COUNT].wrapping_add(1);
            }
            if score.gauntlet {
                killer.state.persistent[PERS_GAUNTLET_FRAG_COUNT] =
                    killer.state.persistent[PERS_GAUNTLET_FRAG_COUNT].wrapping_add(1);
            }
            if score.kill {
                killer.mortality.last_kill_time = server_time;
            }
        }
        self.jedi_master_died(client, died.master_point, died.saber_lost);
        // "Give them back a point since they didn't really die" (`g_combat.c:2691-2699`).
        let team_change = request.means == sjk_game_jka::means_of_death::MOD_TEAM_CHANGE;
        if team_change && let Some(peer) = self.peer_mut(client) {
            peer.state.persistent[PERS_SCORE] = peer.state.persistent[PERS_SCORE].wrapping_add(1);
        }
        self.calculate_ranks();
        if died.show_scoreboard && !team_change {
            let message = self.scoreboard(server_time);
            self.told.push(Told::One(client, message));
        }
        self.scores_to_followers(client, server_time);
        // The NPCs' half: an NPC killer's point and victory, `G_DeathAlert`.
        self.npc_player_death(
            client,
            request.attacker,
            request.means,
            died.saber_lost.is_some(),
        );
        self.power_duel_death(client);
    }

    /// `AddScore`'s team half (`g_combat.c:476-477`): only `GT_TEAM` scores teams by kills.
    fn add_team_score(&mut self, team: i32, points: i32) {
        if self.gametype == GAMETYPE_TEAM && (1..=2).contains(&team) {
            self.team_scores[(team - 1) as usize] += points;
        }
    }

    /// What `CalculateRanks` ranks by: the game type and the team scores.
    fn standings(&self) -> Standings {
        Standings {
            gametype: self.gametype,
            team_scores: self.team_scores,
        }
    }

    /// `DeathmatchScoreboardMessage` at `server_time`, everyone in rank order.
    fn scoreboard(&self, server_time: i32) -> Vec<u8> {
        let contenders = self.contenders();
        let sorted = calculate_ranks_in(&contenders, self.standings()).sorted;
        scoreboard_message_in(&contenders, &sorted, server_time, self.team_scores)
    }

    /// `CheckTeamStatus` (`g_team.c:1264-1305`): once a second in a team game, every
    /// player on a team is sent the team overlay (`tinfo`).
    fn check_team_status(&mut self, server_time: i32) {
        if self.gametype < sjk_game_jka::match_end::GT_TEAM {
            return;
        }
        let Some(now) = sjk_game_jka::team_info::due(self.last_team_info, server_time) else {
            return;
        };
        self.last_team_info = now;
        let mut everyone = Vec::new();
        for client in 0..self.players.places() {
            let location = self.location_number(client);
            let Some(peer) = self.peer(client).filter(|peer| peer.begun) else {
                continue;
            };
            everyone.push(sjk_game_jka::team_info::OverlayEntry {
                client: client as u16,
                team: peer.session.team,
                location,
                health: peer.state.stats[0] as i32,
                armor: peer.state.stats[5] as i32,
                weapon: u32::from(peer.state.weapon()),
                powerups: sjk_game_jka::player_entity::powerup_bits(&peer.state),
            });
        }
        for player in &everyone {
            if let Some(message) = sjk_game_jka::team_info::message(player.team, &everyone) {
                self.told
                    .push(Told::One(usize::from(player.client), message));
            }
        }
    }

    /// `WP_FireDisruptor` by `client` at `level_time`: the beams, the hits and the
    /// disintegration, through the map and the players.
    /// `WP_DEMP2_AltFire` by `client`: the sphere where the instant trace ends, a hidden
    /// pool entity thinking from the next frame.
    fn fire_demp2_sphere(&mut self, client: usize, level_time: i32) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let state = peer.state.clone();
        let (muzzle, forward) =
            sjk_game_jka::weapon_fire::muzzle_point(&state, peer.entity.state());
        let mut targets = InstantTargets {
            game: self,
            shooter: client,
            level_time,
        };
        let sphere = sjk_game_jka::demp2::fire_alt(
            &state,
            muzzle,
            forward,
            level_time,
            &mut |skip, start, end, mask| {
                DisruptorTargets::trace(&mut targets, skip, start, end, mask)
            },
        );
        if let Some(number) = self.pool.spawn_hidden(level_time) {
            self.spheres.push((number, sphere));
        }
    }

    /// The shock spheres' thinks (`DEMP2_AltDetonate`, `DEMP2_AltRadiusDamage`) at
    /// `server_time`, after the missiles: the effect, the shockwave's damage on every
    /// player it reaches for the first time (the owner spared), the freeing.
    fn run_spheres(&mut self, server_time: i32) {
        let mut spheres = std::mem::take(&mut self.spheres);
        spheres.retain_mut(|(number, sphere)| {
            let owner = usize::from(sphere.owner);
            let Some(shooter) = self.peer_mut(owner) else {
                return false;
            };
            let attacker = Attacker {
                npc: false,
                client: sphere.owner,
                max_health: shooter.state.max_health(),
                team: shooter.session.team,
                saber_knockback: [0.0; 4],
            };
            let mut targets = Vec::with_capacity(self.players.places());
            if let Some(world) = self.server.world(self.world) {
                for (other, handle) in self.players.holders().enumerate() {
                    let peer = handle
                        .and_then(|handle| world.entity(handle))
                        .filter(|peer| {
                            peer.begun
                                && peer.playing()
                                && peer.corpse.is_none_or(|(contents, _)| contents != 0)
                        });
                    if let Some(peer) = peer {
                        let (bottom, mut top) = peer.movement.box_bounds();
                        if let Some((_, corpse_top)) = peer.corpse {
                            top[2] = corpse_top;
                        }
                        let origin = peer.state.origin();
                        let bounds = (
                            std::array::from_fn(|axis| origin[axis] + bottom[axis] - 1.0),
                            std::array::from_fn(|axis| origin[axis] + top[axis] + 1.0),
                        );
                        targets.push(ShockTarget {
                            number: other as u16,
                            bounds,
                            origin,
                        });
                    }
                }
            }
            let mut hurt = |target: u16, request: DamageRequest| {
                let _ = self.strike(owner, usize::from(target), request, false);
            };
            match run_sphere(sphere, server_time, attacker, &targets, &mut hurt) {
                SphereRun::Waiting | SphereRun::Expanded | SphereRun::Done => true,
                SphereRun::Detonated(effect) => {
                    let _ = self.pool.spawn_temporary(effect.state(), server_time, None);
                    true
                }
                SphereRun::Freed => {
                    self.pool.free(*number, server_time);
                    false
                }
            }
        });
        self.spheres = spheres;
    }

    fn fire_disruptor(&mut self, client: usize, alternate: bool, level_time: i32) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let state = peer.state.clone();
        let base: [f32; 3] = std::array::from_fn(|axis| {
            f32::from_bits(peer.entity.state().raw_field([2, 1, 4][axis]).unwrap_or(0))
        });
        let attacker = Attacker {
            npc: false,
            client: client as u16,
            max_health: state.max_health(),
            team: peer.session.team,
            saber_knockback: [0.0; 4],
        };
        let shooter = Shooter {
            client: client as u16,
            state: &state,
            muzzle: Shooter::muzzle_from(&state, base),
            attacker,
        };
        let mut targets = InstantTargets {
            game: self,
            shooter: client,
            level_time,
        };
        if alternate {
            fire_alt(&shooter, level_time, &mut targets);
        } else {
            fire_main(&shooter, level_time, &mut targets);
        }
    }

    /// `WP_PlaceLaserTrap` or `WP_DropDetPack` by `client`: the oldest of ten freed (det
    /// packs spared with cheats on), the charge thrown — a det pack marking its owner as
    /// having one planted — or every det pack of the owner's primed to blow.
    fn fire_charge(&mut self, client: usize, alternate: bool, level_time: i32) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let det_pack = peer.state.weapon() == WP_DET_PACK;
        if let Some(index) =
            sjk_game_jka::mines::oldest(&self.charges, client as u16, det_pack, level_time)
            && !(det_pack && self.settings.cheats)
        {
            let (number, _) = self.charges.remove(index);
            self.free_with_model(number, level_time);
        }
        let Self {
            sounds, told, pool, ..
        } = self;
        let mut frame = ChargeFrame {
            sounds: &mut |name| {
                sounds.index(name, &mut |index, value| {
                    told.push(Told::ConfigString {
                        index,
                        previous: Vec::new(),
                        value: value.to_vec(),
                    })
                })
            },
            raise: &mut |event| {
                let _ = pool.spawn_temporary(event.state(), level_time, None);
            },
        };
        if det_pack && alternate {
            let mut rng = self.deaths.rng;
            let charges = &mut self.charges;
            if let Some(peer) = self
                .players
                .at(client)
                .and_then(|handle| self.server.world_mut(self.world)?.entity_mut(handle))
                && sjk_game_jka::mines::blow_det_packs(
                    &mut peer.state,
                    charges,
                    level_time,
                    &mut rng,
                    &mut frame,
                )
            {
                peer.movement = peer.movement.reseeded(&peer.state);
            }
            self.deaths.rng = rng;
            return;
        }
        let Some(peer) = self
            .players
            .at(client)
            .and_then(|handle| self.server.world_mut(self.world)?.entity_mut(handle))
        else {
            return;
        };
        let (muzzle, forward) =
            sjk_game_jka::weapon_fire::muzzle_point(&peer.state, peer.entity.state());
        let mut charge = if det_pack {
            let model = self.models.index(
                b"models/weapons2/detpack/det_pack_proj.glm",
                &mut |index, value| {
                    told.push(Told::ConfigString {
                        index,
                        previous: Vec::new(),
                        value: value.to_vec(),
                    })
                },
            );
            sjk_game_jka::mines::drop_det_pack(
                &peer.state,
                muzzle,
                forward,
                model,
                level_time,
                &mut self.rand,
            )
        } else {
            let model = self.models.index(
                b"models/weapons2/laser_trap/laser_trap_w.glm",
                &mut |index, value| {
                    told.push(Told::ConfigString {
                        index,
                        previous: Vec::new(),
                        value: value.to_vec(),
                    })
                },
            );
            sjk_game_jka::mines::place_laser_trap(
                &peer.state,
                muzzle,
                forward,
                alternate,
                model,
                level_time,
                &mut self.rand,
            )
        };
        let Some(number) = pool.spawn_entity(charge.missile.state.clone(), level_time) else {
            return;
        };
        pool.set_bounds(number, charge.missile.bounds);
        charge.number = number.legacy_number();
        self.charges.push((number, charge));
        if det_pack {
            peer.state.set_raw_field(PS_HAS_DETPACK_PLANTED, 1);
            peer.movement = peer.movement.reseeded(&peer.state);
        }
    }

    /// `G_FreeEntity` on an entity with a Ghoul2 model (a charge, a dead saber): its slot,
    /// and its model named to every client (`G_KillG2Queue`, sent as `kg2`).
    fn free_with_model(&mut self, id: EntityId, level_time: i32) {
        self.pool.free(id, level_time);
        self.told.push(Told::Everyone(
            format!("kg2 {}", id.legacy_number()).into_bytes(),
        ));
    }

    /// The charges this frame (after the missiles and the spheres): each moved, stuck,
    /// armed, watching or blown — the blast's splash on the players and the other
    /// charges, nothing counted for the accuracy, then its effect — and its wire state
    /// published; freed ones leave the pool.
    fn run_charges(&mut self, server_time: i32) {
        let previous_time = self.previous_frame_time;
        let mut charges = std::mem::take(&mut self.charges);
        let mut blown = Vec::new();
        for index in 0..charges.len() {
            let (number, charge) = &mut charges[index];
            let (owner, wire) = (usize::from(charge.missile.owner), number.legacy_number());
            self.gather_obstacles(owner);
            let mut own: Vec<BoxObstacle> = self
                .obstacles
                .iter()
                .copied()
                .filter(|obstacle| obstacle.entity != wire)
                .collect();
            own.extend(
                charges
                    .iter()
                    .enumerate()
                    .filter(|(other, (other_number, other_charge))| {
                        *other != index
                            && (usize::from(other_charge.missile.owner) != owner
                                || other_charge.owner_not_shared)
                            && !self
                                .obstacles
                                .iter()
                                .any(|obstacle| obstacle.entity == other_number.legacy_number())
                    })
                    .map(|(_, (other_number, other_charge))| BoxObstacle {
                        entity: other_number.legacy_number(),
                        origin: other_charge.missile.current,
                        bounds: other_charge.missile.bounds,
                        contents: other_charge.missile.contents,
                        model: None,
                    }),
            );
            let (number, charge) = &mut charges[index];
            self.gather_obstacles(usize::MAX);
            let everyone: Vec<BoxObstacle> = self
                .obstacles
                .iter()
                .copied()
                .filter(|obstacle| obstacle.entity < 32)
                .collect();
            let mut watched = Vec::new();
            if let Some(world) = self.server.world(self.world) {
                for (other, handle) in self.players.holders().enumerate() {
                    if let Some(peer) = handle
                        .and_then(|handle| world.entity(handle))
                        .filter(|peer| peer.begun && peer.playing() && peer.health > 0)
                    {
                        watched.push(Watched {
                            number: other as u16,
                            origin: peer.state.origin(),
                        });
                    }
                }
            }
            let Self {
                sounds,
                told,
                pool,
                map,
                ..
            } = self;
            let mut frame = ChargeFrame {
                sounds: &mut |name| {
                    sounds.index(name, &mut |index, value| {
                        told.push(Told::ConfigString {
                            index,
                            previous: Vec::new(),
                            value: value.to_vec(),
                        })
                    })
                },
                raise: &mut |event| {
                    let _ = pool.spawn_temporary(event.state(), server_time, None);
                },
            };
            let run = match map {
                Some(map) => sjk_game_jka::mines::run_charge(
                    charge,
                    server_time,
                    previous_time,
                    &WithPlayers {
                        world: WorldCollision {
                            bsp: &map.bsp,
                            scratch: &map.scratch,
                        },
                        players: &own,
                    },
                    &WithPlayers {
                        world: WorldCollision {
                            bsp: &map.bsp,
                            scratch: &map.scratch,
                        },
                        players: &everyone,
                    },
                    &watched,
                    &mut self.deaths.rng,
                    &mut frame,
                ),
                None => sjk_game_jka::mines::run_charge(
                    charge,
                    server_time,
                    previous_time,
                    &WithPlayers {
                        world: Void,
                        players: &own,
                    },
                    &WithPlayers {
                        world: Void,
                        players: &everyone,
                    },
                    &watched,
                    &mut self.deaths.rng,
                    &mut frame,
                ),
            };
            match run {
                ChargeRun::Quiet | ChargeRun::Moved => {}
                ChargeRun::Blown { origin, effect } => blown.push((index, origin, effect)),
                ChargeRun::Freed => {
                    self.free_with_model(*number, server_time);
                    continue;
                }
            }
            self.pool.set_state(*number, &charge.state(wire));
        }
        for (index, origin, effect) in blown {
            let (id, charge) = charges[index].clone();
            let number = id.legacy_number();
            let owner = usize::from(charge.missile.owner);
            let Some(shooter) = self.peer_mut(owner) else {
                continue;
            };
            let attacker = Attacker {
                npc: false,
                client: charge.missile.owner,
                max_health: shooter.state.max_health(),
                team: shooter.session.team,
                saber_knockback: [0.0; 4],
            };
            let (request, radius) =
                sjk_game_jka::mines::blast_request(&charge, server_time, attacker);
            self.gather_splash_targets();
            let mut targets = std::mem::take(&mut self.splash_targets);
            targets.extend(charges.iter().filter(|(other, _)| *other != id).map(
                |(other, charge)| SplashTarget {
                    number: other.legacy_number(),
                    bounds: charge.linked_bounds(),
                    origin: charge.missile.current,
                    takes_damage: charge.takes_damage,
                },
            ));
            let mut rng = self.deaths.rng;
            // The map is set aside while the blast's lines are traced through it.
            let map = self.map.take();
            let mut hurt = |target: u16, request: DamageRequest| {
                if usize::from(target) < self.players.places() || self.npcs.roster.is_npc(target) {
                    return self
                        .strike(owner, usize::from(target), request, false)
                        .0
                        .died
                        .is_some();
                }
                if let Some((_, other)) = charges
                    .iter_mut()
                    .find(|(other, _)| other.legacy_number() == target)
                {
                    sjk_game_jka::mines::hurt_charge(
                        other,
                        request.damage,
                        request.attacker.is_some(),
                        server_time,
                        &mut rng,
                    );
                }
                false
            };
            match &map {
                Some(map) => {
                    let _ = radius_damage(
                        origin,
                        Some(attacker),
                        request.damage as f32,
                        radius,
                        Some(number),
                        request.means,
                        server_time,
                        &targets,
                        &WorldCollision {
                            bsp: &map.bsp,
                            scratch: &map.scratch,
                        },
                        &mut hurt,
                    );
                }
                None => {
                    let _ = radius_damage(
                        origin,
                        Some(attacker),
                        request.damage as f32,
                        radius,
                        Some(number),
                        request.means,
                        server_time,
                        &targets,
                        &Void,
                        &mut hurt,
                    );
                }
            }
            self.map = map;
            self.deaths.rng = rng;
            self.flush_npc_blows();
            self.splash_targets = targets;
            self.splash_targets.clear();
            let _ = self.pool.spawn_temporary(effect.state(), server_time, None);
            for (other, charge) in charges.iter() {
                self.pool
                    .set_state(*other, &charge.state(other.legacy_number()));
            }
        }
        self.charges = charges
            .into_iter()
            .filter(|(number, _)| self.pool.state_mut(*number).is_some())
            .collect();
    }

    /// `WP_FireMelee` or `WP_FireStunBaton` by `client`: the hand's reach, the punch or
    /// the zap.
    fn fire_hand(&mut self, client: usize, level_time: i32) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let state = peer.state.clone();
        let (forward, right) = sjk_game_jka::pmove::flight::flight_axes(state.view_angles());
        let attacker = Attacker {
            npc: false,
            client: client as u16,
            max_health: state.max_health(),
            team: peer.session.team,
            saber_knockback: [0.0; 4],
        };
        let mut rng = self.deaths.rng;
        let mut targets = InstantTargets {
            game: self,
            shooter: client,
            level_time,
        };
        if state.weapon() == WP_STUN_BATON {
            sjk_game_jka::melee::fire_stun_baton(
                &state,
                forward.to_array(),
                right.to_array(),
                attacker,
                level_time,
                &mut rng,
                &mut targets,
            );
        } else {
            sjk_game_jka::melee::fire_melee(
                &state,
                forward.to_array(),
                right.to_array(),
                attacker,
                level_time,
                &mut rng,
                &mut targets,
            );
        }
        self.deaths.rng = rng;
    }

    /// `WP_FireConcussionAlt` by `client`: the shooter shoved, the beam, the knockdowns.
    fn fire_concussion_alt(&mut self, client: usize, level_time: i32) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let mut state = peer.state.clone();
        let (muzzle, forward) =
            sjk_game_jka::weapon_fire::muzzle_point(&state, peer.entity.state());
        let attacker = Attacker {
            npc: false,
            client: client as u16,
            max_health: state.max_health(),
            team: peer.session.team,
            saber_knockback: [0.0; 4],
        };
        let mut targets = InstantTargets {
            game: self,
            shooter: client,
            level_time,
        };
        sjk_game_jka::concussion::fire_alt(
            &mut state,
            muzzle,
            forward,
            attacker,
            level_time,
            &mut targets,
        );
        // The shooter's own shove is in the wire state: the movement restarts from it.
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        for field in [6, 7, 8, 16, 41] {
            peer.state
                .set_raw_field(field, state.raw_field(field).unwrap_or(0));
        }
        peer.movement = peer.movement.reseeded(&peer.state);
    }

    /// `G_Damage` on a player: the damage, the movement restarted from the knockback, the
    /// shield flash, and the death when the blow kills. Returns what it came to, for the
    /// attacker's counters.
    fn hurt(&mut self, target: usize, request: DamageRequest) -> Damaged {
        self.hurt_at(target, request, None)
    }

    /// [`Self::hurt`], placed where the caller says (see [`damage_at`]).
    fn hurt_at(
        &mut self,
        target: usize,
        request: DamageRequest,
        location: Option<HitLocation>,
    ) -> Damaged {
        // A siege player waiting for its wave cannot be hurt (`takedamage = qfalse`).
        if self.duel_refuses(target, &request)
            || self
                .peer(target)
                .is_some_and(|peer| peer.temp_spectate != 0)
        {
            return Damaged::default();
        }
        let level_time = request.level_time;
        // `G_LocationBasedDamageModifier`: a player whose model was struck this frame —
        // by a blade or a missile — is placed by the struck surface, whatever hurts it.
        let location = match (location, request.point) {
            (None, Some(point)) => self.surface_location(target, request.flags, point, level_time),
            (location, _) => location,
        };
        // The game's generator, copied out while the victim is borrowed and put back.
        let mut rng = self.deaths.rng;
        let spared_by_master =
            self.jedi_master_spares(request.attacker.map(|attacker| attacker.client), target);
        // Aboard a ship that shelters it, or in noclip (`client->noclip`, `g_combat.c`):
        // only a DEMP2's shock, which comes first, lands.
        let sheltered = self.peer(target).is_some_and(|victim| {
            victim.noclip || self.npcs.roster.shelters(&victim.state, request.flags)
        });
        let Some(victim) = self.peer_mut(target) else {
            return Damaged::default();
        };
        let was_dead = victim.health <= 0;
        let (bottom, top) = victim.movement.box_bounds();
        let origin = victim.state.origin();
        let bounds = (
            std::array::from_fn(|axis| origin[axis] + bottom[axis] - 1.0),
            std::array::from_fn(|axis| origin[axis] + top[axis] + 1.0),
        );
        let team = victim.session.team;
        let mut target_view = Target {
            client: target as u16,
            state: &mut victim.state,
            health: &mut victim.health,
            origin,
            bounds,
            invulnerable_until: &mut victim.invulnerable_until,
            team,
            wounds: &mut victim.wounds,
            force: Some(&mut victim.force),
            spared_by_master,
        };
        let damaged = if sheltered {
            sjk_game_jka::damage::shock(&mut target_view, &request, &mut rng);
            Damaged::default()
        } else {
            damage_at(&mut target_view, request, location, &mut rng)
        };
        // The knockback is in the wire state: the movement restarts from it.
        victim.movement = victim.movement.reseeded(&victim.state);
        self.deaths.rng = rng;
        for event in &damaged.events {
            let _ = self.pool.spawn_temporary(event.state(), level_time, None);
        }
        if damaged.landed
            && let Some(attacker) = request.attacker
        {
            self.blow_landed(usize::from(attacker.client), target, level_time);
        }
        if damaged.died.is_some()
            && was_dead
            && let Some(corpse) = self.peer_mut(target)
        {
            // `body_die`: a corpse hurt again is not killed again; gibbed, it disintegrates.
            if corpse_hit(
                &mut corpse.state,
                &mut corpse.health,
                corpse.mortality.respawn_time,
                level_time,
            ) {
                corpse.corpse = corpse.corpse.map(|(_, top)| (0, top));
                corpse.movement = corpse.movement.reseeded(&corpse.state);
            }
            return damaged;
        }
        if let Some(death) = damaged.died.map(|death| sjk_game_jka::damage::Death {
            attacker: death
                .attacker
                .map(|attacker| self.mounted.credited(attacker)),
            ..death
        }) && let Some(victim) = self.peer_mut(target)
        {
            let request = DeathRequest {
                npc_attacker: request.attacker.is_some_and(|attacker| attacker.npc),
                level_time,
                client: target as u16,
                origin: victim.state.origin(),
                saber_off_sounds: victim.saber_off_sounds(),
                health: victim.health,
                attacker: death.attacker,
                means: death.means,
                damage: death.take,
                point: death.point,
                bounds,
                killer_last_kill_time: 0,
                gametype: 0,
                command_weapon: 0,
                avoid_dismember: self.avoid_dismember,
                duel_opponent: None,
                jedi_master: None,
                in_space: false,
            };
            self.die(target, request);
        }
        damaged
    }

    /// One server frame of the game (`G_RunFrame`, as far as players' entities go):
    /// events shown long enough are cleared, and every player is converted once more.
    /// Call it before the endpoint's own frame, which builds the snapshots.
    pub fn run_frame(&mut self, server_time: i32) {
        // `SV_Frame`: a delayed `map_restart` whose time has come runs first; the frame
        // that follows is the one `SV_MapRestart_f` runs after everyone connected again.
        self.run_due_restart(server_time);
        // `SV_BotFrame` before the frame: the queued bots whose time has come begin, and
        // every bot thinks.
        self.check_bot_spawn(self.last_frame_time);
        self.bot_frame(server_time);
        self.previous_frame_time = self.last_frame_time;
        self.last_frame_time = server_time;
        // `level.startTime`, which the time limit is measured from: the first frame.
        if self.level_start_time == 0 {
            self.level_start_time = server_time;
        }
        self.pool.run_frame(server_time);
        // At the frame's very start, before `level.time` moves (`g_main.c:2935`).
        self.siege_respawn_wave(self.previous_frame_time);
        // `CheckTournament` before `CheckExitRules` (`g_main.c:3390-3394`).
        self.check_tournament(server_time);
        self.run_match_end(server_time);
        self.run_votes(server_time);
        // `G_RunFrame`'s pass over the entities in number order: the players first (the
        // saber's fields, the look target), the missiles after them.
        for client in 0..self.players.places() {
            self.client_frame(client, server_time);
            self.look_around(client, server_time);
            self.run_bot_client(client, server_time);
            self.siege_ex_data(client, server_time);
        }
        self.run_siege_hacks();
        self.update_saber_entities(server_time);
        self.saber_thinks(server_time);
        self.run_emplaced(server_time);
        self.run_missiles(server_time);
        self.run_spheres(server_time);
        self.run_charges(server_time);
        self.run_items(server_time);
        self.run_jedi_master(server_time);
        self.run_holocrons(server_time);
        self.run_siege_items(server_time);
        self.run_multiples(server_time);
        self.run_map_effects(server_time);
        self.run_stock_entities(server_time);
        self.run_doors(server_time);
        self.run_map_turrets(server_time);
        self.run_scripts(server_time);

        self.run_npcs(server_time);

        self.siege_timers(server_time);
        self.end_client_frames(server_time);
        self.check_team_status(server_time);
    }

    /// `WP_SaberPositionUpdate` and `WP_SaberStartMissileBlockCheck` for a playing client
    /// (`g_main.c:3314-3318`): the saber's fields, the look target — the nearest visible
    /// foe, through the map and the others — and, with a saber in hand, the nearest bolt
    /// coming in, which raises the saber a frame before it lands.
    fn look_around(&mut self, client: usize, server_time: i32) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        if !peer.playing() {
            return;
        }
        if peer.saber.update(
            &mut peer.state,
            peer.health,
            server_time,
            &peer.sabers.hands[0],
        ) {
            peer.movement = peer.movement.reseeded(&peer.state);
        }
        self.pose_saber(client, server_time);
        self.throw_frame(client, server_time);
        self.swing_saber(client, server_time);
        self.gather_obstacles(client);
        self.candidates.clear();
        self.everyone.clear();
        for obstacle in &self.obstacles {
            // A rocket or its husk is in the way of the sight lines, never a target.
            self.everyone.push(*obstacle);
            let Some(other) = self
                .players
                .at(usize::from(obstacle.entity))
                .and_then(|handle| self.server.world(self.world)?.entity(handle))
            else {
                continue;
            };
            let bounds = grown(obstacle);
            self.candidates.push(LookCandidate {
                number: obstacle.entity,
                origin: other.state.origin(),
                bounds,
                health: other.health,
                team: other.session.team,
                spectator: !other.playing(),
            });
        }
        self.incoming.clear();
        self.incoming.extend(
            self.missiles
                .iter()
                .filter(|(_, missile)| missile.linked)
                .map(|(number, missile)| Incoming {
                    number: number.legacy_number(),
                    owner: missile.owner,
                    origin: missile.current,
                    bounds: missile.bounds,
                    delta: std::array::from_fn(|axis| {
                        f32::from_bits(missile.state.raw_field(ES_POS_DELTA[axis]).unwrap_or(0))
                    }),
                    stationary: missile.state.raw_field(ES_POS_TYPE).unwrap_or(0) == 0,
                    weapon: missile.state.raw_field(ES_WEAPON).unwrap_or(0) as u8,
                    explodes: false,
                    clip_mask: missile.clip_mask,
                }),
        );
        let Self {
            server,
            world,
            players,
            map,
            obstacles,
            candidates,
            everyone,
            incoming,
            ..
        } = self;
        let peer = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle));
        let Some(peer) = peer else { return };
        let (bottom, top) = peer.movement.box_bounds();
        let own = BoxObstacle {
            entity: client as u16,
            origin: peer.state.origin(),
            bounds: (bottom, top),
            contents: CONTENTS_BODY,
            model: None,
        };
        everyone.push(own);
        let mut watcher = Watcher {
            client: client as u16,
            state: &mut peer.state,
            health: peer.health,
            team: peer.session.team,
            buttons: peer.last_command.buttons,
            top: grown(&own).1[2],
            actively_blocks: peer.sabers.actively_blocks(),
        };
        match map {
            Some(map) => {
                let world = WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                };
                let paths = WithPlayers {
                    world: WorldCollision {
                        bsp: &map.bsp,
                        scratch: &map.scratch,
                    },
                    players: everyone,
                };
                missile_block_check(
                    &mut watcher,
                    candidates,
                    incoming,
                    &WithPlayers {
                        world,
                        players: obstacles,
                    },
                    &paths,
                );
            }
            None => missile_block_check(
                &mut watcher,
                candidates,
                incoming,
                &WithPlayers {
                    world: Void,
                    players: obstacles,
                },
                &WithPlayers {
                    world: Void,
                    players: everyone,
                },
            ),
        }
        // The saber raised in the wire state: the next command's movement takes it up.
        let raised = peer.state.saber_blocked();
        peer.movement.set_saber_blocked(raised);
    }

    /// `G_RunMissile` for every missile in flight, through the map and the players but
    /// the one that fired it. One that strikes a player with a lit saber may be blocked
    /// (`G_MissileImpact`'s `WP_SaberCanBlock`): the flash, the bolt bounced back into
    /// flight as the defender's or killed on the blade. One that hits the world or a
    /// player becomes its impact event and leaves the flight; a player hit takes the
    /// damage.
    fn run_missiles(&mut self, server_time: i32) {
        let previous_time = self.previous_frame_time;
        self.gather_homing_targets();
        let mut missiles = std::mem::take(&mut self.missiles);
        missiles.retain_mut(|(number, missile)| {
            let run = self.run_one_missile(missile, server_time, previous_time);
            match run {
                MissileRun::Blown { effect } => {
                    self.pool.set_state(*number, &missile.state);
                    // `laserTrapExplode`: the splash sparing nobody, counted for nothing,
                    // then the burst's effect.
                    self.splash_as(
                        missile,
                        effect.origin,
                        None,
                        true,
                        MOD_LASER_TRAP_BLOW,
                        server_time,
                    );
                    let _ = self.pool.spawn_temporary(effect.state(), server_time, None);
                    true
                }
                MissileRun::HitBlown { struck, effect } => {
                    // A grenade on a player: the hit counted (`LogAccuracyHit`), the blow's
                    // splash sparing nobody and its effect, then the hit event and the
                    // grenade's own splash as for any hit.
                    let target = usize::from(struck);
                    let owner = usize::from(missile.owner);
                    let counted = self
                        .peer_mut(target)
                        .is_some_and(|victim| victim.health > 0)
                        && owner != target;
                    if counted && let Some(shooter) = self.peer_mut(owner) {
                        shooter.accuracy.0 += 1;
                    }
                    self.splash_as(
                        missile,
                        effect.origin,
                        None,
                        true,
                        MOD_LASER_TRAP_BLOW,
                        server_time,
                    );
                    let _ = self.pool.spawn_temporary(effect.state(), server_time, None);
                    self.pool.set_state(*number, &missile.state);
                    self.pool.free_after_event(*number, server_time);
                    self.splash(missile, Some(struck), counted, server_time);
                    false
                }
                MissileRun::Flying | MissileRun::Bounced => {
                    // What the run did to the wire state reaches the clients: a bounce's
                    // new flight and its event, a homing rocket's turn, a detonator's
                    // settling (which restarts its trajectory every frame) and its events.
                    self.pool.set_state(*number, &missile.state);
                    true
                }
                MissileRun::Blocked { struck, block } => {
                    let _ = self
                        .pool
                        .spawn_temporary(block.flash.state(), server_time, None);
                    if let Some(defender) = self.peer_mut(usize::from(struck)) {
                        let raised = defender.state.saber_blocked();
                        defender.movement.set_saber_blocked(raised);
                    }
                    let bounced = block.outcome == Blocked::Bounced;
                    self.pool.set_state(*number, &missile.state);
                    if !bounced {
                        self.pool.free_after_event(*number, server_time);
                    }
                    bounced
                }
                MissileRun::Missed
                | MissileRun::Hit(_)
                | MissileRun::HitEntity(_)
                | MissileRun::Burst => {
                    let struck = if let MissileRun::Hit(other) = run {
                        Some(other)
                    } else {
                        None
                    };
                    // A bolt that struck something that is not a player — a breakable
                    // brush, a turret, an emplaced gun — damages it, and nothing is counted
                    // for the accuracy.
                    if let MissileRun::HitEntity(other) = run {
                        self.hurt_thing(other, missile, server_time);
                    }
                    let counted = struck.is_some_and(|other| {
                        self.missile_hit(usize::from(other), missile, server_time)
                    });
                    self.pool.set_state(*number, &missile.state);
                    self.pool.free_after_event(*number, server_time);
                    self.splash(
                        missile,
                        struck.or(if let MissileRun::HitEntity(other) = run {
                            Some(other)
                        } else {
                            None
                        }),
                        counted,
                        server_time,
                    );
                    if missile.contents != 0 {
                        self.husks.push((
                            BoxObstacle {
                                entity: number.legacy_number(),
                                origin: missile.current,
                                bounds: missile.bounds,
                                contents: missile.contents,
                                model: None,
                            },
                            missile.owner,
                            *number,
                        ));
                    }
                    false
                }
                MissileRun::Freed => {
                    // A dead saber's model goes with it (`G_KillG2Queue`).
                    if missile.state.raw_field(54).unwrap_or(0) != 0 {
                        self.free_with_model(*number, server_time);
                    } else {
                        self.pool.free(*number, server_time);
                    }
                    false
                }
            }
        });
        self.missiles = missiles;
        let waiting = std::mem::take(&mut self.dead_sabers_waiting);
        self.missiles.extend(waiting);
    }

    /// `g_gravity` as worldspawn set it, read the way a cvar's value is; 800 without a map.
    fn gravity(&self) -> f32 {
        let text = self
            .map
            .as_ref()
            .map_or(&b"800"[..], |map| &map.world.gravity);
        std::str::from_utf8(text)
            .ok()
            .and_then(|text| text.trim().parse().ok())
            .unwrap_or(0.0)
    }

    /// Player `client`, when it is one in the world; `None` for any other number (a dead
    /// saber's owner is its saber entity).
    fn peer_mut(&mut self, client: usize) -> Option<&mut Peer> {
        let handle = self.players.at(client)?;
        self.server.world_mut(self.world)?.entity_mut(handle)
    }

    fn peer(&self, client: usize) -> Option<&Peer> {
        let handle = self.players.at(client)?;
        self.server.world(self.world)?.entity(handle)
    }

    fn peers(&self) -> usize {
        self.server.world(self.world).map_or(0, |world| world.len())
    }
}

/// The players as a homing rocket asks after its enemy, taken before the missiles ran.
struct HomingPeers<'a> {
    targets: &'a [Option<HomingTarget>],
}

impl HomingTargets for HomingPeers<'_> {
    fn target(&self, number: u16) -> Option<HomingTarget> {
        self.targets.get(usize::from(number)).copied().flatten()
    }
}

/// The game as an instant-hit shot traces and strikes it.
struct InstantTargets<'a> {
    game: &'a mut NativeGame,
    shooter: usize,
    level_time: i32,
}

impl sjk_game_jka::melee::MeleeTargets for InstantTargets<'_> {
    fn trace(
        &mut self,
        skip: u16,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> sjk_game_jka::pmove::MovementTrace {
        self.game.gather_obstacles(usize::from(skip));
        let NativeGame { map, obstacles, .. } = &*self.game;
        match map {
            Some(map) => WithPlayers {
                world: WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                },
                players: obstacles,
            }
            .trace(start, mins, maxs, end, mask),
            None => WithPlayers {
                world: Void,
                players: obstacles,
            }
            .trace(start, mins, maxs, end, mask),
        }
    }
    fn struck(&self, number: u16) -> Option<sjk_game_jka::melee::Struck> {
        let Some(peer) = self
            .game
            .players
            .at(usize::from(number))
            .and_then(|handle| self.game.server.world(self.game.world)?.entity(handle))
        else {
            return self.game.npc_struck(number);
        };
        (peer.begun && peer.playing()).then(|| sjk_game_jka::melee::Struck {
            origin: peer.state.origin(),
            player: true,
            duelling: peer
                .state
                .duel_in_progress()
                .then(|| peer.state.duel_index()),
        })
    }
    fn hurt(&mut self, number: u16, request: DamageRequest) {
        let _ = self
            .game
            .strike(self.shooter, usize::from(number), request, false);
    }
    fn sound(&mut self, name: &[u8]) -> u16 {
        let (sounds, told) = (&mut self.game.sounds, &mut self.game.told);
        sounds.index(name, &mut |index, value| {
            told.push(Told::ConfigString {
                index,
                previous: Vec::new(),
                value: value.to_vec(),
            })
        })
    }
    fn raise(&mut self, event: EventEntity) {
        let _ = self
            .game
            .pool
            .spawn_temporary(event.state(), self.level_time, None);
    }
    fn electrify(&mut self, number: u16, until: i32) {
        let Some(victim) = self.game.peer_mut(usize::from(number)) else {
            return self.game.electrify_npc(number, until);
        };
        victim
            .state
            .set_raw_field(sjk_game_jka::melee::ELECTRIFY_TIME, until as u32);
        victim.movement = victim.movement.reseeded(&victim.state);
    }
}

impl ConcussionTargets for InstantTargets<'_> {
    fn trace(
        &mut self,
        skip: u16,
        start: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> sjk_game_jka::pmove::MovementTrace {
        self.game.gather_obstacles(usize::from(skip));
        let NativeGame { map, obstacles, .. } = &*self.game;
        match map {
            Some(map) => WithPlayers {
                world: WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                },
                players: obstacles,
            }
            .trace(start, [-1.0; 3], [1.0; 3], end, mask),
            None => WithPlayers {
                world: Void,
                players: obstacles,
            }
            .trace(start, [-1.0; 3], [1.0; 3], end, mask),
        }
    }
    fn player(&self, number: u16) -> Option<sjk_game_jka::concussion::StruckPlayer> {
        let peer = self
            .game
            .players
            .at(usize::from(number))
            .and_then(|handle| self.game.server.world(self.game.world)?.entity(handle))?;
        if !peer.begun || !peer.playing() {
            return None;
        }
        let team = self
            .game
            .players
            .at(self.shooter)
            .and_then(|handle| self.game.server.world(self.game.world)?.entity(handle))
            .map_or(0, |shooter| shooter.session.team);
        let counts = peer.health > 0
            && usize::from(number) != self.shooter
            && !(team == peer.session.team && team != 0);
        Some(sjk_game_jka::concussion::StruckPlayer {
            counts,
            no_knockback: peer.wounds.no_knockback,
            dodges: would_dodge(&peer.state, peer.health > 0),
        })
    }
    fn count_hit(&mut self) {
        if let Some(shooter) = self.game.peer_mut(self.shooter) {
            shooter.accuracy.0 += 1;
        }
    }
    fn hurt(&mut self, number: u16, request: DamageRequest) {
        let _ = self
            .game
            .strike(self.shooter, usize::from(number), request, false);
    }
    fn shove(
        &mut self,
        number: u16,
        no_knockback: bool,
        forward: [f32; 3],
        shooter: u16,
        shooter_origin: [f32; 3],
    ) {
        let level_time = self.level_time;
        let Some(victim) = self.game.peer_mut(usize::from(number)) else {
            return;
        };
        if sjk_game_jka::knockdown::concussion_shove(
            &mut victim.state,
            &mut victim.knockdown,
            &mut victim.wounds.other_killer,
            victim.health,
            no_knockback,
            forward,
            shooter,
            shooter_origin,
            level_time,
        ) {
            victim.movement = victim.movement.reseeded(&victim.state);
        }
    }
    fn raise(&mut self, event: EventEntity) {
        let _ = self
            .game
            .pool
            .spawn_temporary(event.state(), self.level_time, None);
    }
}

impl DisruptorTargets for InstantTargets<'_> {
    fn trace(
        &mut self,
        skip: u16,
        start: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> sjk_game_jka::pmove::MovementTrace {
        self.game.gather_obstacles(usize::from(skip));
        let NativeGame { map, obstacles, .. } = &*self.game;
        match map {
            Some(map) => WithPlayers {
                world: WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                },
                players: obstacles,
            }
            .trace(start, [0.0; 3], [0.0; 3], end, mask),
            None => WithPlayers {
                world: Void,
                players: obstacles,
            }
            .trace(start, [0.0; 3], [0.0; 3], end, mask),
        }
    }
    fn player(&self, number: u16) -> Option<StruckPlayer> {
        let peer = self
            .game
            .players
            .at(usize::from(number))
            .and_then(|handle| self.game.server.world(self.game.world)?.entity(handle))?;
        if !peer.begun || !peer.playing() {
            return None;
        }
        let saber_defense = peer
            .session
            .force
            .as_ref()
            .map_or(0, |force| force.levels[FP_SABER_DEFENSE]);
        Some(StruckPlayer {
            duelling_another: sjk_game_jka::duel::elsewhere(&peer.state, self.shooter as u16),
            saber_defense,
            dodges: would_dodge(&peer.state, peer.health > 0),
        })
    }
    fn can_block(&mut self, number: u16, point: [f32; 3]) -> bool {
        let level_time = self.level_time;
        let Some(defender) = self.game.peer_mut(usize::from(number)) else {
            return false;
        };
        let defense = defender
            .session
            .force
            .as_ref()
            .map_or(0, |force| force.levels[FP_SABER_DEFENSE]);
        let mut view = Defender {
            client: number,
            state: &mut defender.state,
            saber_blocking: defender.movement.state().saber_blocking,
            buttons: defender.last_command.buttons,
            forward_move: defender.last_command.forward_move,
            defense,
            block_time: &mut defender.block_time,
        };
        let blocked = view.can_block(point, level_time);
        let raised = defender.state.saber_blocked();
        defender.movement.set_saber_blocked(raised);
        blocked
    }
    fn hurt(&mut self, number: u16, request: DamageRequest) -> Blow {
        let (damaged, _) = self
            .game
            .strike(self.shooter, usize::from(number), request, true);
        Blow {
            killed: damaged.died.is_some(),
        }
    }
    fn disintegrate(&mut self, number: u16, point: [f32; 3], poses: (u32, u32)) {
        let Some(victim) = self.game.peer_mut(usize::from(number)) else {
            return;
        };
        disintegrated(&mut victim.state, point, poses);
        // `r.contents = 0`: nothing runs into the body.
        victim.corpse = victim.corpse.map(|(_, top)| (0, top));
        victim.movement = victim.movement.reseeded(&victim.state);
    }
    fn poses(&self, number: u16) -> (u32, u32) {
        let state = self
            .game
            .players
            .at(usize::from(number))
            .and_then(|handle| self.game.server.world(self.game.world)?.entity(handle))
            .map(|peer| peer.state.clone());
        state.map_or((0, 0), |state| {
            (
                state.raw_field(13).unwrap_or(0),
                state.raw_field(15).unwrap_or(0),
            )
        })
    }
    fn raise(&mut self, event: EventEntity) {
        let _ = self
            .game
            .pool
            .spawn_temporary(event.state(), self.level_time, None);
    }
}

/// An owned [`LegacyGameOutput`], kept until the endpoint asks.
enum Told {
    Everyone(Vec<u8>),
    One(usize, Vec<u8>),
    PlayerString {
        client: usize,
        previous: Vec<u8>,
        value: Vec<u8>,
    },
    /// Any other configstring the game set: `previous` is what the stored table held.
    ConfigString {
        index: usize,
        previous: Vec<u8>,
        value: Vec<u8>,
    },
    Drop {
        client: usize,
        reason: Vec<u8>,
    },
    /// The world is a new one; the endpoint gives every client a new gamestate.
    MapChanged {
        server_time: i32,
    },
    /// The level is being played again on this world (`map_restart`).
    MapRestarted,
}

/// `gametype_t`, as far as this server goes so far: free-for-all, Jedi Master, the duels,
/// team deathmatch, siege and capture the flag.
pub const GAMETYPE_FFA: i32 = 0;
pub const GAMETYPE_HOLOCRON: i32 = sjk_game_jka::holocron::GT_HOLOCRON;
pub const GAMETYPE_JEDIMASTER: i32 = sjk_game_jka::jedi_master::GT_JEDIMASTER;
pub const GAMETYPE_DUEL: i32 = 3;
pub const GAMETYPE_POWERDUEL: i32 = 4;
pub const GAMETYPE_TEAM: i32 = 6;
pub const GAMETYPE_SIEGE: i32 = 7;
pub const GAMETYPE_CTF: i32 = 8;
pub const GAMETYPE_CTY: i32 = sjk_game_jka::ctf::GT_CTY;
/// The reference's cvar defaults: every Force rank, nothing disabled, no auto join.
const SETTINGS: ForceServerSettings = ForceServerSettings {
    gametype: GAMETYPE_FFA,
    max_rank: 7,
    disabled: 0,
    force_based_teams: false,
    team_auto_join: false,
    weapons_disabled: 0,
    cheats: false,
};
/// `g_spawnInvulnerability`'s default, in milliseconds.
const SPAWN_INVULNERABILITY: i32 = 3_000;
/// `g_forceRespawn`'s default: a dead player is respawned after this many seconds.
const FORCE_RESPAWN_SECONDS: i32 = 60;
/// `BUTTON_ATTACK`, `BUTTON_USE_HOLDABLE`: either respawns a dead player.
const BUTTON_ATTACK: u16 = 1;
const BUTTON_USE_HOLDABLE: u16 = 4;
/// `g_speed`'s default.
const PLAYER_SPEED: f32 = 250.0;
/// Wire field `saberEntityNum`.
const SABER_ENTITY: usize = 31;
/// `persistant[PERS_SCORE]`, `PERS_HITS`, `PERS_RANK`, `PERS_ATTACKEE_ARMOR`,
/// `PERS_EXCELLENT_COUNT`.
const PERS_SCORE: usize = 0;
const PERS_HITS: usize = 1;
const PERS_RANK: usize = 2;
const PERS_ATTACKEE_ARMOR: usize = 7;
const PERS_EXCELLENT_COUNT: usize = 10;
const PERS_GAUNTLET_FRAG_COUNT: usize = 13;
/// `ps.hasDetPackPlanted`.
const PS_HAS_DETPACK_PLANTED: usize = 87;
/// `CS_ITEMS`; the player's external event and its parameter; an entity's event.
const CS_ITEMS: usize = 27;
const EFLAGS_EVENT: usize = 56;
const EFLAGS_EVENT_PARM: usize = 64;
const ES_EVENT: usize = 28;
/// `modelindex` in `msg.cpp`'s entity table: the player model a client draws.
const ES_MODELINDEX: usize = 46;
const ES_EVENT_PARM: usize = 42;
/// `PM_DEAD`, `PM_NOCLIP`.
const PM_DEAD: u8 = 5;
const PM_NOCLIP: u8 = 3;
/// `WP_DISRUPTOR`, `WP_DEMP2`, `WP_ROCKET_LAUNCHER`.
const WP_DISRUPTOR: u8 = 6;
const WP_DEMP2: u8 = 9;
const WP_ROCKET_LAUNCHER: u8 = 11;
const WP_CONCUSSION: u8 = 15;
const WP_STUN_BATON: u8 = 1;
const WP_MELEE: u8 = 2;
const WP_TRIP_MINE: u8 = 13;
const WP_DET_PACK: u8 = 14;
/// `EV_GENERAL_SOUND`.
const EV_GENERAL_SOUND: u32 = 76;
/// `MOD_TELEFRAG`.
const MOD_TELEFRAG: u32 = 37;
/// `EF_INVULNERABLE`: spawn protection.
const EF_INVULNERABLE: u32 = 1 << 27;
/// `EF_BODYPUSH`: a player a Force push struck, shown for 600 ms.
const EF_BODYPUSH: u32 = 1 << 19;
/// A standing player's box (`playerMins`, `playerMaxs`).
const PLAYER_BOX: ([f32; 3], [f32; 3]) = ([-15.0, -15.0, -24.0], [15.0, 15.0, 40.0]);
/// `CONTENTS_BODY`: what a living player is made of.
const CONTENTS_BODY: u32 = 0x100;

/// `CS_PLAYERS` (`bg_public.h`): one string per legacy client number.
const CS_PLAYERS: usize = 1_131;
/// `g_maxConnPerIP`'s default.
const CONNECTIONS_PER_ADDRESS: usize = 3;

#[path = "bridge_chat.rs"]
mod bridge_chat;
#[path = "bridge_cheats.rs"]
mod bridge_cheats;
#[path = "bridge_ctf.rs"]
mod bridge_ctf;
#[path = "bridge_duel.rs"]
mod bridge_duel;
#[path = "bridge_follow.rs"]
mod bridge_follow;
#[path = "bridge_force.rs"]
mod bridge_force;
#[path = "bridge_generic.rs"]
mod bridge_generic;
#[path = "bridge_holocron.rs"]
mod bridge_holocron;
#[path = "bridge_icarus.rs"]
mod bridge_icarus;
#[path = "bridge_jedimaster.rs"]
mod bridge_jedimaster;
#[path = "bridge_lock.rs"]
mod bridge_lock;
#[path = "bridge_maps.rs"]
mod bridge_maps;
#[path = "bridge_missile_models.rs"]
mod bridge_missile_models;
#[path = "bridge_multiples.rs"]
mod bridge_multiples;
#[path = "bridge_power_duel.rs"]
mod bridge_power_duel;
#[path = "bridge_restart.rs"]
mod bridge_restart;
#[path = "bridge_saber.rs"]
mod bridge_saber;
#[path = "bridge_saber_bounce.rs"]
mod bridge_saber_bounce;
#[path = "bridge_saber_damage.rs"]
mod bridge_saber_damage;
#[path = "bridge_spawn.rs"]
mod bridge_spawn;
#[path = "bridge_think.rs"]
mod bridge_think;
#[path = "bridge_tournament.rs"]
mod bridge_tournament;
#[path = "bridge_votes.rs"]
mod bridge_votes;
use bridge_maps::{authoritative, fixed, world_config_strings};
#[path = "bridge_console.rs"]
mod bridge_console;
#[path = "bridge_cvars.rs"]
mod bridge_cvars;
#[path = "bridge_log.rs"]
mod bridge_log;
#[path = "bridge_ranks.rs"]
mod bridge_ranks;
#[path = "bridge_svcmds.rs"]
mod bridge_svcmds;
#[path = "bridge_userinfo.rs"]
mod bridge_userinfo;
use bridge_userinfo::judge;
#[path = "bridge_demos.rs"]
mod bridge_demos;
#[path = "bridge_intermission.rs"]
mod bridge_intermission;
use bridge_cvars::{SERVERINFO_DEFAULTS, server_info_with};
use bridge_spawn::spawn_place;

#[path = "bridge_bots.rs"]
mod bridge_bots;
#[path = "bridge_commands.rs"]
mod bridge_commands;
#[path = "bridge_connect.rs"]
mod bridge_connect;
#[path = "bridge_host.rs"]
mod bridge_host;
#[path = "bridge_items.rs"]
mod bridge_items;

#[path = "bridge_throw.rs"]
mod bridge_throw;
use bridge_throw::init_saber_entity;
#[path = "bridge_client_spawn.rs"]
mod bridge_client_spawn;
#[path = "bridge_collision_gather.rs"]
mod bridge_collision_gather;
#[path = "bridge_doors.rs"]
mod bridge_doors;
#[path = "bridge_holdables.rs"]
mod bridge_holdables;
#[path = "bridge_links.rs"]
mod bridge_links;
#[path = "bridge_map_effects.rs"]
mod bridge_map_effects;
#[path = "bridge_map_logic.rs"]
mod bridge_map_logic;
#[path = "bridge_map_turrets.rs"]
mod bridge_map_turrets;
#[path = "bridge_npcs.rs"]
mod bridge_npcs;
#[path = "bridge_path_movers.rs"]
mod bridge_path_movers;
#[path = "bridge_sabers.rs"]
mod bridge_sabers;
#[path = "bridge_siege.rs"]
mod bridge_siege;
#[path = "bridge_siege_clients.rs"]
mod bridge_siege_clients;
#[path = "bridge_siege_items.rs"]
mod bridge_siege_items;
#[path = "bridge_siege_triggers.rs"]
mod bridge_siege_triggers;
#[path = "bridge_stock_rules.rs"]
mod bridge_stock_rules;
#[path = "bridge_teleport.rs"]
mod bridge_teleport;
#[path = "bridge_use.rs"]
mod bridge_use;
#[path = "bridge_world_effects.rs"]
mod bridge_world_effects;
pub(crate) use bridge_saber::PlayerSkeleton;
pub(crate) use bridge_saber_damage::SaberCut;
pub use bridge_votes::SUPPORTED_VOTES;

#[path = "bridge_continuation.rs"]
mod continuation;
