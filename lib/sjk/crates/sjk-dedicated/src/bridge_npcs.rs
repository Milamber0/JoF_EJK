//! The level's NPCs on this server (`NPC_spawn.c`): the map's spawners spawned with the
//! level, their NPCs placed and begun as the frames come, spawners used by name, and the
//! `npc spawn` command. The rules are `sjk_game_jka::npc_roster` and the modules it
//! drives; this is where they meet the server's entity numbers, tables, map and players.
//!
//! An NPC is the core's own actor, kept in [`Npcs::roster`] — a list with no fixed
//! capacity. Protocol 26 sees each as the `ET_NPC` entity of its pool slot: the wire
//! state the spawn and the begin wrote, linked by its box, and solid for a client's
//! prediction as `SV_LinkEntity` packs a body's box (`sv_world.cpp:263-286`). The pool is
//! protocol 26's numbers; when it has none left, the spawn says so and makes nothing.
//!
//! A begun NPC thinks every frame (`NPC_Think`, [`sjk_game_jka::npc_think`]): its
//! command runs through the players' movement against this map and whoever stands in it,
//! and its `ET_NPC` entity is converted from its player state after every move.

use super::*;
use sjk_game_jka::npc_roster::{CommandPlace, Fired, NpcCommand, NpcFiles, NpcRoster, npc_command};

/// `CS_ITEMS`' length: one character per row of the item list.
const ITEM_COUNT: usize = 51;
/// `MASK_SOLID`: what the `npc spawn` placement traces against.
const MASK_SOLID: u32 = 0x1 | 0x1000;
/// `NPCTEAM_ENEMY`, `NPCTEAM_PLAYER`: a player's team for the NPCs (`ClientSpawn`,
/// `g_client.c:3794-3811`).
const NPCTEAM_ENEMY: i32 = 1;
const NPCTEAM_PLAYER: i32 = 2;

/// The NPCs of the level, and what their thinks read of the players and the models. Their
/// precaches register effects in the level's table (`MapEffects::effects`).
#[derive(Default)]
pub(super) struct Npcs {
    /// The spawners and the NPCs.
    pub(super) roster: NpcRoster,
    /// The players as the NPCs' senses read them, gathered for each run of the roster (the
    /// buffer kept from frame to frame).
    sights: Vec<sjk_game_jka::npc_senses::Body>,
    /// The skeletons NPC models animate with, found as they spawn.
    skeletons: Vec<(Vec<u8>, host::Skeleton)>,
    /// A blast's blows on NPCs, waiting for the map it set aside (`combat`): the shooter,
    /// the NPC, the blow, and whether it counts for the shooter's accuracy.
    deferred_blows: Vec<(usize, u16, DamageRequest, bool)>,
    /// Each NPC's server-side model, which blades meet (`bodies`).
    bodies: bodies::NpcBodies,
    /// The riders' states while the roster's vehicle code has them
    /// ([`riding::riders::RiderScratch`]), kept for the next time.
    rider_scratch: Vec<riding::riders::RiderScratch>,
    /// The NPCs' Force update's scratch (`host::force`).
    force_touch: host::force::ForceTouch,
    /// Every reference function an NPC reached that this server does not run, by NPC
    /// type, each told once ([`host::NpcOutcome::Unported`]).
    pub(super) unported: Vec<(Vec<u8>, String)>,
}

impl Npcs {
    /// Names, once per NPC type, a reference function its think, pain or death reached and
    /// this server does not run.
    pub(super) fn tell_unported_call(&mut self, number: u16, name: String) {
        let npc_type = self
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == number)
            .map(|npc| npc.npc_type.clone())
            .unwrap_or_default();
        if !self
            .unported
            .iter()
            .any(|(known, told)| *known == npc_type && *told == name)
        {
            eprintln!(
                "npc {number} ({}): the reference's {name} is not ported yet",
                String::from_utf8_lossy(&npc_type)
            );
            self.unported.push((npc_type, name));
        }
    }
}

impl std::fmt::Debug for Npcs {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Npcs")
            .field("roster", &self.roster)
            .finish_non_exhaustive()
    }
}

impl NativeGame {
    /// `G_SpawnEntitiesFromString` for the map's `NPC_*` entities, as the level begins: the
    /// roster started afresh, every spawner spawned and precached, and the items the
    /// precaches registered added to `CS_ITEMS` (`SaveRegisteredItems` runs once, after
    /// the map's entities).
    pub(super) fn spawn_npcs(&mut self) {
        self.npcs = Npcs::default();
        let Some(map) = self.map.take() else { return };
        // `SV_SpawnServer`'s `sv_mapChecksum` (`sv_init.cpp:600`), which `Nav_Load` checks.
        self.cvars
            .set(b"sv_mapChecksum", map.checksum.to_string().as_bytes());
        let (entities, parms, sabers) = (
            &map.npc_entities,
            self.level_npc_parms(&map),
            map.sabers.clone(),
        );
        let capacity = self.vehicle_capacity();
        let native_sand_creatures = !self.stock_rules();
        let allow = self.integer_cvar(b"g_allowNPC", 1) != 0;
        let (gametype, level_time) = (self.gametype, self.last_frame_time);
        let mut items = Vec::new();
        let source = self.npc_nav_source();
        let route_file = (source != sjk_game_jka::npc_nav_sources::NavSource::Stock)
            .then(|| self.route_file(&map))
            .flatten();
        self.gather_obstacles(usize::MAX);
        let mut host = self.npc_host(Some(&map), Some(&mut items));
        let mut roster = NpcRoster::default();
        roster.level.native_sand_creatures = native_sand_creatures;
        // `BG_VehicleLoadParms` at `G_InitGame`: the level's vehicle table, empty.
        roster.vehicle_table = Some(sjk_game_jka::vehicle_parms::VehicleTable::new(
            map.vehicle_files.clone(),
            capacity,
        ));
        // `Nav_Load` before the entities spawn (`g_main.c:325`).
        let loaded = roster.load_navigation(map.nav_file.as_deref(), map.checksum);
        roster.spawn_map(
            entities,
            gametype,
            allow,
            level_time,
            NpcFiles {
                parms: &parms,
                sabers: &sabers,
            },
            &mut host,
        );
        let nodes = roster.level.navigator.graph.len();
        let made = nav_source::make_navigation(
            source,
            &map,
            route_file.as_deref(),
            &mut roster,
            level_time,
            &mut host,
        );
        drop(host);
        match (loaded, map.nav_file.is_some()) {
            (true, _) => eprintln!("navigation: {nodes} nodes read from the map's .nav file"),
            (false, true) => eprintln!(
                "navigation: the map's .nav file is not for this build of it; {nodes} waypoints to connect"
            ),
            (false, false) => eprintln!("navigation: no .nav file; {nodes} waypoints to connect"),
        }
        if let Some(made) = made {
            eprintln!("{}", made.describe());
        }
        self.npcs.roster = roster;
        self.map = Some(map);
        let mut present = self
            .config_strings
            .iter()
            .find(|(index, _)| *index == CS_ITEMS)
            .map_or_else(|| vec![b'0'; ITEM_COUNT], |(_, value)| value.clone());
        let before = present.clone();
        for item in items {
            if let Some(slot) = present.get_mut(item) {
                *slot = b'1';
            }
        }
        if present != before {
            self.publish_config_string(CS_ITEMS, &present);
        }
        if !self.npcs.roster.vehicles.is_empty() {
            println!(
                "{} NPC walkers and fighters placed and not spawned: their movement is a later vehicle slice",
                self.npcs.roster.vehicles.len()
            );
        }
    }

    /// `G_RunThink` for the level's spawners and NPCs at `level_time`, one at a time in
    /// entity order ([`NpcRoster::pending`]), and what each fired. What an NPC's turn did
    /// beyond the roster — its Force blows, saber blows, knock-outs, lost locks and blasts
    /// on the players ([`host::NpcOutcome`]), the names it used, its vehicle's outcomes — is
    /// dealt as its turn ends, before the next one runs: `G_Damage` on a player inside an
    /// NPC's think is over before `G_RunFrame` reaches the next entity, so the next NPC
    /// sees the player hurt or dead, as in the reference.
    pub(super) fn run_npcs(&mut self, level_time: i32) {
        if self
            .npcs
            .roster
            .next_due()
            .is_none_or(|due| due > level_time)
        {
            return;
        }
        let _ = self.with_roster(|roster, _, host| roster.frame_begin(level_time, host));
        while let Some(_number) = self.npcs.roster.pending() {
            let fired =
                self.with_roster(|roster, files, host| roster.run_pending(level_time, files, host));
            let Some(fired) = fired else { break };
            self.fire_from_npcs(fired, level_time);
            self.vehicle_outcomes(level_time);
        }
        self.tell_unported();
    }

    /// Names, once per NPC, the reference function its behaviour reached that no step has
    /// ported yet (its class AI: `NPC_BSJedi_Default` and the rest), for which it stands.
    fn tell_unported(&mut self) {
        for npc in &mut self.npcs.roster.actors {
            if npc.mind.unported.is_some() && npc.mind.unported != npc.mind.unported_told {
                npc.mind.unported_told = npc.mind.unported;
                let name = npc.mind.unported.unwrap_or_default();
                eprintln!(
                    "npc {} ({}): the reference's {name} is not ported yet; it stands",
                    npc.number,
                    String::from_utf8_lossy(&npc.npc_type)
                );
            }
        }
    }

    /// `G_UseTargets` reaching the roster's spawners called `name`.
    pub(super) fn use_npc_spawners(&mut self, name: &str, level_time: i32) {
        if !self.npcs.roster.answers_to(name.as_bytes()) {
            return;
        }
        let fired = self
            .with_roster(|roster, files, host| {
                roster.use_targets(name.as_bytes(), level_time, files, host)
            })
            .unwrap_or_default();
        self.fire_from_npcs(fired, level_time);
    }

    /// `npc ...` from a client (`Cmd_NPC_f`, `CMD_CHEAT|CMD_ALIVE`: `g_cmds.c:3417`,
    /// `3466-3480`). Whether the command was this one.
    pub(super) fn npc_client_command(&mut self, client: usize, text: &[u8]) -> bool {
        let mut words = text
            .split(|byte| byte.is_ascii_whitespace())
            .filter(|word| !word.is_empty());
        if !words
            .next()
            .is_some_and(|word| word.eq_ignore_ascii_case(b"npc"))
        {
            return false;
        }
        if !self.settings.cheats {
            self.told
                .push(Told::One(client, b"print \"@@@NOCHEATS\n\"".to_vec()));
            return true;
        }
        let Some(peer) = self.peer(client) else {
            return true;
        };
        if peer.health <= 0 || !peer.playing() {
            self.told
                .push(Told::One(client, b"print \"@@@MUSTBEALIVE\n\"".to_vec()));
            return true;
        }
        let (origin, view) = (peer.state.origin(), peer.state.view_angles());
        let words: Vec<&[u8]> = words.collect();
        match npc_command(&words) {
            NpcCommand::Help => print!("{}", sjk_game_jka::npc_roster::NPC_COMMAND_HELP),
            NpcCommand::Kill { name, team } => {
                let (level_time, name, team) = (self.last_frame_time, name.to_vec(), team.to_vec());
                let fired = self
                    .with_roster(|roster, _, host| {
                        roster.kill_command(&name, &team, level_time, host)
                    })
                    .unwrap_or_default();
                self.fire_from_npcs(fired, level_time);
            }
            NpcCommand::Score(name) => {
                let name = name.to_vec();
                let _ = self.with_roster(|roster, _, host| roster.score_command(&name, host));
            }
            NpcCommand::ShowBounds => self.npcs.roster.toggle_bounds(),
            NpcCommand::Nothing => {}
            NpcCommand::Spawn {
                npc_type,
                targetname,
                vehicle,
            } => {
                let level_time = self.last_frame_time;
                let Some((npc_type, vehicle)) = self.npc_spawn_name(npc_type, vehicle, client)
                else {
                    return true;
                };
                let targetname = targetname.to_vec();
                let place = self.command_place(origin, view);
                let fired = self
                    .with_roster(|roster, files, host| {
                        roster.spawn_command(
                            &npc_type,
                            &targetname,
                            vehicle,
                            place,
                            level_time,
                            files,
                            host,
                        )
                    })
                    .unwrap_or_default();
                self.fire_from_npcs(fired, level_time);
            }
        }
        true
    }

    /// Where `npc spawn` puts its NPC for a player at `origin` looking along `view`: point
    /// traces through the world alone (`MASK_SOLID`, `NPC_spawn.c:3990-3997`).
    fn command_place(&self, origin: [f32; 3], view: [f32; 3]) -> CommandPlace {
        let mut trace = |start, end| match self.map.as_ref() {
            Some(map) => WorldCollision {
                bsp: &map.bsp,
                scratch: &map.scratch,
            }
            .trace(start, [0.0; 3], [0.0; 3], end, MASK_SOLID),
            None => Void.trace(start, [0.0; 3], [0.0; 3], end, MASK_SOLID),
        };
        let (origin, yaw) = sjk_game_jka::npc_spawn::command_place(origin, view, &mut trace);
        CommandPlace { origin, yaw }
    }

    /// The begun, solid NPCs as bodies the players' moves run into.
    pub(super) fn npc_bodies(npcs: &Npcs) -> impl Iterator<Item = BoxObstacle> + '_ {
        npcs.roster
            .begun()
            .filter(|npc| npc.contents != 0)
            .map(|npc| npc.body())
    }

    /// The begun NPCs as a player's move meets them: their bodies among the obstacles, and
    /// their legs by entity number (`PM_BGEntForNum`: a saber's specials read them, and
    /// whoever stands on one is bounced off its head).
    pub(super) fn add_npc_bodies(
        npcs: &Npcs,
        obstacles: &mut Vec<BoxObstacle>,
        legs: &mut Vec<Option<u16>>,
        skip: usize,
    ) {
        // Not the one whose trace it is (an NPC's own missile passes it), nor the vehicle
        // it drives, which it owns (`SV_ClipMoveToEntities`).
        let owned = |body: &BoxObstacle| {
            npcs.roster.actors.iter().any(|npc| {
                npc.number == body.entity
                    && usize::from(sjk_game_jka::vehicle_board::owner_of(npc)) == skip
            })
        };
        obstacles.extend(
            Self::npc_bodies(npcs).filter(|body| usize::from(body.entity) != skip && !owned(body)),
        );
        for npc in npcs.roster.begun() {
            let at = usize::from(npc.number);
            if legs.len() <= at {
                legs.resize(at + 1, None);
            }
            legs[at] = Some(npc.player.leg_animation());
        }
    }

    /// Runs `run` on the roster with the level's NPC files and a host over this game;
    /// then the telefrags its begins asked for, and what it did to the players and the
    /// game ([`host::NpcOutcome`]). `None` without a map.
    pub(super) fn with_roster<R>(
        &mut self,
        run: impl FnOnce(&mut NpcRoster, NpcFiles<'_>, &mut ServerHost<'_>) -> R,
    ) -> Option<R> {
        let map = self.map.take()?;
        let (parms, sabers) = (self.level_npc_parms(&map), map.sabers.clone());
        self.gather_obstacles(usize::MAX);
        let mut roster = std::mem::take(&mut self.npcs.roster);
        let mut host = self.npc_host(Some(&map), None);
        let result = run(
            &mut roster,
            NpcFiles {
                parms: &parms,
                sabers: &sabers,
            },
            &mut host,
        );
        let telefrags = std::mem::take(&mut host.telefrags);
        let outcomes = std::mem::take(&mut host.outcomes);
        drop(host);
        self.npcs.roster = roster;
        self.map = Some(map);
        for victim in telefrags {
            let request = DamageRequest {
                level_time: self.last_frame_time,
                attacker: None,
                direction: None,
                point: None,
                damage: 100_000,
                flags: DAMAGE_NO_PROTECTION,
                means: MOD_TELEFRAG,
            };
            let _ = self.hurt(victim, request);
        }
        self.apply_npc_outcomes(outcomes);
        Some(result)
    }

    /// The names the roster fired, each used as any name is (`G_UseTargets`).
    pub(super) fn fire_from_npcs(&mut self, fired: Fired, level_time: i32) {
        for name in fired {
            self.fire_targets(&String::from_utf8_lossy(&name), usize::MAX, level_time);
        }
    }

    /// The host the roster runs against: this game's pool, tables, map and players.
    fn npc_host<'a>(
        &'a mut self,
        map: Option<&'a LoadedMap>,
        items: Option<&'a mut Vec<usize>>,
    ) -> ServerHost<'a> {
        let client_zero = self
            .peer(0)
            .filter(|peer| peer.begun)
            .map_or(([0.0; 3], 0), |peer| {
                let origin = [11, 12, 13]
                    .map(|index| f32::from_bits(peer.entity.state().raw_field(index).unwrap_or(0)));
                let team = if self.gametype == GAMETYPE_SIEGE && peer.session.team == 1 {
                    NPCTEAM_ENEMY
                } else {
                    NPCTEAM_PLAYER
                };
                (origin, team)
            });
        let skill = self.integer_cvar(b"g_npcspskill", 0);
        let fighter_alt_control = self.integer_cvar(b"bg_fighterAltControl", 0) != 0;
        let debug_saber_locks = self.debug_saber_locks();
        let gravity = self.gravity();
        let clients = self.gather_sights();
        let jedi_master = self.jedi_masters(None).and_then(|masters| masters.master);
        let intermission = self.match_end.intermission_time != 0;
        let ghoul2_time = if self.previous_frame_time == 0 {
            self.last_frame_time
        } else {
            self.previous_frame_time
        };
        let since_last_frame = self.last_frame_time - self.previous_frame_time;
        let limbs = self.limb_rules();
        let Self {
            pool,
            sounds,
            models,
            told,
            deaths,
            obstacles,
            npcs,
            gametype,
            last_frame_time,
            missiles,
            server,
            world,
            players,
            doors,
            multiples,
            breakables,
            usable_entities,
            map_effects,
            skeletons: models_cache,
            rand,
            items: pickups,
            ..
        } = self;
        let Npcs {
            sights,
            skeletons,
            bodies,
            force_touch,
            ..
        } = npcs;
        let (effects, bones) = (&mut map_effects.effects, &mut map_effects.bones);
        ServerHost {
            pool,
            sounds,
            models,
            effects,
            bones,
            told,
            deaths,
            missiles,
            server,
            world: *world,
            roster: players,
            outcomes: Vec::new(),
            jedi_master,
            // This server has no warm-up.
            warmup: false,
            intermission,
            map,
            solids: obstacles,
            players: sights,
            clients,
            skeletons,
            bodies,
            models_cache,
            ghoul2_time,
            level_time: *last_frame_time,
            gravity,
            skill,
            gametype: *gametype,
            client_zero,
            items,
            telefrags: Vec::new(),
            doors,
            multiples,
            breakables,
            usables: usable_entities,
            crt: rand,
            pickups,
            since_last_frame,
            force_touch,
            fighter_alt_control,
            debug_saber_locks,
            limbs,
        }
    }

    /// Player `client` as the NPCs' senses read it, if it is in the game.
    pub(super) fn player_sight(&mut self, client: usize) -> Option<sjk_game_jka::npc_senses::Body> {
        self.gather_sights();
        self.npcs
            .sights
            .iter()
            .find(|body| usize::from(body.number) == client)
            .copied()
    }

    /// The begun players as the NPCs' senses read them (`g_entities[0..MAX_CLIENTS]`), into
    /// the kept buffer; returns which clients are in use, a bit each.
    fn gather_sights(&mut self) -> u64 {
        let siege = self.gametype == GAMETYPE_SIEGE;
        let Self {
            server,
            world,
            players,
            npcs,
            ..
        } = self;
        npcs.sights.clear();
        let Some(world) = server.world(*world) else {
            return 0;
        };
        let mut clients = 0_u64;
        for (client, handle) in players.holders().enumerate().take(64) {
            let Some(peer) = handle.and_then(|handle| world.entity(handle)) else {
                continue;
            };
            clients |= 1 << client;
            if !peer.begun {
                continue;
            }
            let (mins, maxs) = peer.movement.box_bounds();
            // `ClientSpawn`'s team for the NPCs: the player's own, but siege's red.
            let player_team = if siege && peer.session.team == 1 {
                NPCTEAM_ENEMY
            } else {
                NPCTEAM_PLAYER
            };
            npcs.sights.push(sjk_game_jka::npc_senses::Body {
                number: client as u16,
                npc: false,
                origin: peer.state.origin(),
                mins,
                maxs,
                view_height: peer.state.view_height(),
                view_angles: peer.state.view_angles(),
                // `UpdateClientRenderinfo`, before the frame's thinks: the eyes over it.
                eye_point: {
                    let mut eyes = peer.state.origin();
                    eyes[2] += peer.state.view_height() as f32;
                    eyes
                },
                eye_angles: peer.state.view_angles(),
                health: peer.health,
                flags: 0,
                entity_flags: peer.entity.state().raw_field(19).unwrap_or(0),
                player_team,
                enemy_team: 0,
                session_team: peer.session.team,
                class: 0,
                weapon: i32::from(peer.state.weapon()),
                enemy: peer.npc_enemy,
                spectating: false,
                surrendering: false,
                velocity: peer.state.velocity(),
                ducked: peer.state.movement_flags() & 1 != 0,
                saber_holstered: peer.state.raw_field(81).unwrap_or(0) != 0,
                saber_in_flight: peer.state.raw_field(88).unwrap_or(0) != 0,
            });
        }
        clients
    }

    /// A console variable's integer value (`atoi`), or `default` where it is not
    /// registered.
    pub(super) fn integer_cvar(&self, name: &[u8], default: i32) -> i32 {
        self.cvars
            .var(name)
            .map_or(default, |var| sjk_game_jka::userinfo::atoi(&var.string))
    }
}

#[path = "bridge_dismember.rs"]
mod dismember;
#[path = "bridge_npc_host.rs"]
mod host;
use host::ServerHost;
#[path = "bridge_npc_blades.rs"]
mod blades;
#[path = "bridge_npc_bodies.rs"]
mod bodies;
#[path = "bridge_npc_combat.rs"]
mod combat;
#[path = "bridge_npc_force_frame.rs"]
pub(super) mod force_frame;
#[path = "bridge_npc_nav_source.rs"]
mod nav_source;
#[path = "bridge_npc_items.rs"]
mod npc_items;
#[path = "bridge_npc_triggers.rs"]
mod triggers;
#[path = "bridge_vehicle_bodies.rs"]
mod vehicle_bodies;

#[path = "bridge_vehicles.rs"]
mod bridge_vehicles;
#[path = "bridge_riding.rs"]
mod riding;

#[path = "bridge_npc_names.rs"]
mod bridge_npc_names;
