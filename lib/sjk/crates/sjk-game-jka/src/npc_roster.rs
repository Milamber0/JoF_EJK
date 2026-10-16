//! The NPCs of a level and the spawners that make them, run as `G_RunFrame` runs their
//! thinks: every spawner and NPC whose time has come, in entity-number order —
//! `NPC_Spawn_Go` for a spawner, `NPC_Begin` for an NPC placed a frame ago, `G_FreeEntity`
//! for one that gave up. Spawners are also used by name (`G_UseTargets` → `NPC_Spawn`) and
//! made by the `npc spawn` command (`Cmd_NPC_f`, `NPC_Spawn_f`, `NPC_SpawnType`).
//!
//! The roster is the core's own record: a list with no fixed capacity. Entity numbers are
//! the [`NpcHost`]'s to give and to refuse. What is not spawned here says so through
//! [`NpcHost::print`]: vehicles (`NPC_Vehicle`, the NPC plan's step 10) and shy spawners
//! (`NPC_ShySpawn` waits for client 0 to look away).
//!
//! A begun NPC thinks (`NPC_Think`, [`crate::npc_think`]) whenever its think is due: every
//! frame, its behaviour every tenth of a second.
//!
//! Held to `tools/game-oracle/npcspawn.c` (`game-npcspawn.txt`).

use crate::npc_begin::{Begun, begin, register};
use crate::npc_parms::NpcParms;
use crate::npc_senses::AlertEvents;
use crate::npc_spawn::{NoNpc, NpcActor, NpcHost, NpcThink, spawn_do};
use crate::npc_spawners::{NpcSpawner, Registration, choose};
use crate::npc_world::NpcWorld;
use crate::saber_definition::SaberParms;

/// `SHY`: a spawner that waits until nobody sees its spot.
const SHY: i32 = 2048;

/// The level's NPC spawners, each by its entity number, and its NPCs.
#[derive(Clone, Debug)]
pub struct NpcRoster {
    /// The spawners standing, in the order they were spawned.
    pub spawners: Vec<(u16, NpcSpawner)>,
    /// The NPCs, in the order they were spawned.
    pub actors: Vec<NpcActor>,
    /// The vehicles the map placed and this server does not spawn: walkers and fighters,
    /// whose movement is a later step's ([`crate::vehicle_roster`]).
    pub vehicles: Vec<NpcSpawner>,
    /// The level's vehicle definitions (`g_vehicleInfo`), where the level has vehicle files.
    pub vehicle_table: Option<crate::vehicle_parms::VehicleTable>,
    /// The level's alerts (`level.alertEvents`), which NPCs notice.
    pub alerts: AlertEvents,
    /// No `NPC_Think` at all: for a transcript whose driver stood it in (npcspawn.c). The
    /// begin's own first think runs.
    pub skip_thinks: bool,
    /// `showBBoxes` (`npc showbounds`).
    pub show_bounds: bool,
    /// `WP_SaberPositionUpdate` and `WP_SaberStartMissileBlockCheck` run for every NPC at its
    /// turn, and its saber entity thinks ([`crate::npc_saber`]). A replay of a driver that
    /// stood them in for NPCs (all but npcblade.c's and npcjedi.c's) turns it off: then a
    /// saber carrier keeps no saber, and only an NPC without one has its eyes kept.
    pub saber_upkeep: bool,
    /// No NPC's eyes move from where it began: for a transcript whose driver stood
    /// `WP_SaberPositionUpdate` in for every NPC (vehride.c), which is where they are kept.
    pub eyes_frozen: bool,
    /// `WP_ForcePowersUpdate` runs for every NPC at its turn ([`crate::npc_force_update`]).
    /// A replay of a driver that stood it in for NPCs (all but npcjedi.c's) turns it off.
    pub force_upkeep: bool,
    /// What the level keeps for the NPCs' tactics: squads, combat points, the last move.
    pub level: crate::npc_groups::NpcLevel,
    /// The timers freed NPCs left, by the entity number they had: the reference keeps its
    /// timers by entity index (`g_timers`, `g_timer.c`) and clears them only at a death
    /// (`TIMER_Clear2`), so the next NPC given the number starts with them.
    pub left_timers: Vec<(u16, crate::npc_mind::NpcTimers)>,
    /// Scratch the thinks reuse frame after frame: the actors in entity-number order, the
    /// bodies a move meets, and a temp entity's state.
    scratch: Scratch,
    /// What the vehicles' thinks left for the players' side ([`Self::take_vehicle_outcomes`]).
    pub(crate) vehicle_outcomes: Vec<crate::vehicle_drive::VehicleOutcome>,
    /// The level's space-ship triggers and the points they name
    /// ([`crate::vehicle_triggers`]).
    pub ship_triggers: crate::vehicle_triggers::ShipTriggers,
}

impl Default for NpcRoster {
    /// An empty level's roster, every NPC's upkeep on.
    fn default() -> Self {
        Self {
            spawners: Vec::new(),
            actors: Vec::new(),
            vehicles: Vec::new(),
            vehicle_table: None,
            alerts: AlertEvents::default(),
            skip_thinks: false,
            show_bounds: false,
            saber_upkeep: true,
            eyes_frozen: false,
            vehicle_outcomes: Vec::new(),
            ship_triggers: Default::default(),
            force_upkeep: true,
            left_timers: Vec::new(),
            level: crate::npc_groups::NpcLevel::default(),
            scratch: Scratch::default(),
        }
    }
}

/// What a frame's entry in entity-number order runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Due {
    /// A spawner's spawn or its freeing.
    Spawner,
    /// An NPC's upkeep and think.
    Npc,
    /// An NPC's saber entity's think (`SaberUpdateSelf`).
    Saber,
    /// A ship boundary's think (`shipboundary_think`).
    Boundary,
    /// A limb's run (`G_RunItem`, `LimbThink`: [`crate::npc_dismember`]).
    Limb,
}

/// What the thinks reuse, so that a frame allocates nothing once warmed up.
#[derive(Clone, Debug)]
struct Scratch {
    due: Vec<(u16, Due)>,
    order: Vec<usize>,
    bodies: Vec<crate::entity_clip::BoxObstacle>,
    overflow: sjk_protocol::EntityState,
    spare_state: Option<sjk_protocol::PlayerState>,
    impact_bodies: Vec<crate::pmove::vehicle_impact::ImpactBody>,
    /// The clients' legs a move reads (`PM_BGEntForNum(n)->s.legsAnim`), by number.
    body_legs: Vec<(u16, u16)>,
}

impl Default for Scratch {
    fn default() -> Self {
        Self {
            due: Vec::new(),
            order: Vec::new(),
            bodies: Vec::new(),
            overflow: crate::npc_client_think::blank_entity(),
            spare_state: None,
            impact_bodies: Vec::new(),
            body_legs: Vec::new(),
        }
    }
}

/// Where the `npc spawn` command puts its NPC, as the caller worked it out
/// ([`crate::npc_spawn::command_place`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CommandPlace {
    /// The spawner's origin.
    pub origin: [f32; 3],
    /// The spawner's yaw: the player's own.
    pub yaw: f32,
}

/// The names a run of the roster fired, in order (`G_UseTargets`): a spawner's `target`
/// as its last NPC spawns, an NPC's `target3` as it gives up. The caller uses each as it
/// uses any name — the map's targets and the roster's spawners alike
/// ([`NpcRoster::use_targets`]) — once the roster's run is over; the reference uses them
/// at once, in the middle of its pass over the entities.
pub type Fired = Vec<Vec<u8>>;

/// The data a spawn reads: the NPC files and the sabers.
#[derive(Clone, Copy)]
pub struct NpcFiles<'a> {
    /// The NPC files (`NPC_LoadParms`).
    pub parms: &'a NpcParms,
    /// The saber files (`WP_SaberLoadParms`).
    pub sabers: &'a SaberParms,
}

impl NpcRoster {
    /// `G_SpawnEntitiesFromString` for the map's NPC entities, in the lump's order, at
    /// `level_time`: each left out in a game type it is not for, its healing sound
    /// registered (`G_CallSpawn`), its class's choice and precache made around
    /// `SP_NPC_spawner` (`allow`: `g_allowNPC`), and its NPC precached; the map's combat
    /// points, waypoints and navigation goals with them. Navigation is set going as
    /// `G_InitGame` ends: call [`Self::load_navigation`] first for a map with a `.nav` file.
    pub fn spawn_map(
        &mut self,
        entities: &[sjk_entity::Entity],
        gametype: i32,
        allow: bool,
        level_time: i32,
        files: NpcFiles<'_>,
        host: &mut impl NpcHost,
    ) {
        let named = crate::vehicle_triggers::named_points(entities);
        for entity in entities {
            let Some(classname) = entity.classname() else {
                continue;
            };
            if crate::bot_routes::spawns_in(entity, gametype)
                && self.place_ship_entity(entity, &named, level_time, host)
            {
                continue;
            }
            if crate::bot_routes::spawns_in(entity, gametype)
                && self.spawn_navigation(entity, classname, host)
            {
                continue;
            }
            if classname.eq_ignore_ascii_case("point_combat")
                && crate::bot_routes::spawns_in(entity, gametype)
            {
                // `SP_point_combat`: the level's, its entity freed at once.
                if let Some(point) =
                    crate::npc_combat_points::point_combat(entity, &self.level.combat_points)
                {
                    self.level.combat_points.push(point);
                }
                continue;
            }
            if !classname
                .get(..4)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("npc_"))
                || !crate::bot_routes::spawns_in(entity, gametype)
            {
                continue;
            }
            let mut irand = |low, high| host.irand(low, high);
            let npc_type = entity
                .get("npc_type")
                .map(|name| crate::npc_parms::new_string(name.as_bytes()));
            let spawnflags = entity
                .get("spawnflags")
                .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()));
            let Some(choice) = choose(classname, npc_type.as_deref(), spawnflags, &mut irand)
            else {
                continue;
            };
            if let Some(sound) = entity.get("healingsound").filter(|sound| !sound.is_empty()) {
                host.sound_index(sound.as_bytes());
            }
            register(choice.before, host);
            let spawner = NpcSpawner::from_entity(entity, &choice, allow, level_time);
            if let Some(spawner) = spawner {
                self.place(spawner, level_time, files, host);
            }
            register(choice.after, host);
        }
        // The end of `G_InitGame`'s navigation: the paths' time set, or the combat points
        // given their waypoints.
        self.finish_navigation(level_time, host);
    }

    /// A map spawner kept, precached; or a vehicle or a shy spawner told of and set aside.
    fn place(
        &mut self,
        spawner: NpcSpawner,
        level_time: i32,
        files: NpcFiles<'_>,
        host: &mut impl NpcHost,
    ) {
        if spawner.vehicle {
            self.place_vehicle(spawner, level_time, files, host);
            return;
        }
        let precached = crate::npc_precache::spawner_precache(
            files.parms,
            spawner.npc_type.as_deref(),
            spawner.sound_flags,
            spawner.spawnflags,
        );
        for model in &precached.models {
            host.model_index(model);
        }
        for sound in &precached.sounds {
            host.sound_index(sound);
        }
        for weapon in &precached.weapons {
            if let Some(item) = Registration::Weapon(*weapon).item() {
                host.register_item(item);
            }
        }
        let Some(number) = host.spawn_hidden() else {
            host.print("^1ERROR: no entity left for an NPC spawner\n");
            return;
        };
        self.spawners.push((number, spawner));
    }

    /// `G_RunFrame` for the roster at `level_time`: the alerts older than 200 ms gone
    /// (`ClearPlayerAlertEvents`), every NPC's upkeep, and `G_RunThink` for every spawner and
    /// NPC due, in entity-number order. Returns what they fired beyond the roster's own
    /// spawners.
    pub fn run_frame(
        &mut self,
        level_time: i32,
        files: NpcFiles<'_>,
        host: &mut impl NpcHost,
    ) -> Fired {
        self.frame_begin(level_time, host);
        let mut fired = Fired::new();
        while self.pending().is_some() {
            fired.append(&mut self.run_pending(level_time, files, host));
        }
        fired
    }

    /// The start of `G_RunFrame` for the roster: the alerts older than 200 ms gone, every
    /// NPC's upkeep, and the spawners and NPCs whose think is due gathered, to be run in
    /// entity-number order ([`Self::pending`], [`Self::run_pending`]) — between the other
    /// entities the caller runs in that order.
    pub fn frame_begin(&mut self, level_time: i32, host: &mut impl NpcHost) {
        // The paths calculated when their time has come (`NAV_CheckCalcPaths`),
        // `AI_UpdateGroups`, the nodes checked last frame and every entity's stale waypoint
        // forgotten (`g_main.c:3022-3057`).
        let mut fired = Fired::new();
        {
            let mut world = self.world(level_time, host, &mut fired);
            world.check_calc_paths();
            world.update_groups();
            world.forget_waypoints();
        }
        self.alerts.clear_old(level_time);
        // The players' turns come before any NPC's: each finds its waypoint
        // (`NAV_FindPlayerWaypoint`, `g_main.c:3321-3326`).
        self.world(level_time, host, &mut fired)
            .find_player_waypoints();
        let due_at = |spawner: &NpcSpawner| {
            spawner
                .spawn_at
                .or(spawner.free_at)
                .is_some_and(|at| at <= level_time)
        };
        let due = &mut self.scratch.due;
        due.clear();
        due.extend(
            self.spawners
                .iter()
                .filter(|(_, spawner)| due_at(spawner))
                .map(|(number, _)| (*number, Due::Spawner)),
        );
        // Every NPC is met in its turn (`G_RunFrame` gives each its render info), thinking or
        // not; and every saber entity, whose think is decided at its turn.
        due.extend(self.actors.iter().map(|npc| (npc.number, Due::Npc)));
        if self.saber_upkeep {
            due.extend(
                self.actors
                    .iter()
                    .filter_map(|npc| npc.saber_entity)
                    .map(|number| (number, Due::Saber)),
            );
        }
        due.extend(self.level.limbs.iter().map(|limb| (limb.number, Due::Limb)));
        due.extend(
            self.ship_triggers
                .triggers
                .iter()
                .filter(|trigger| {
                    matches!(
                        trigger.kind,
                        crate::vehicle_triggers::ShipTriggerKind::Boundary { .. }
                    )
                })
                .map(|trigger| (trigger.number, Due::Boundary)),
        );
        due.sort_unstable_by_key(|(number, _)| *number);
        // Taken from the end: the list is kept reversed.
        due.reverse();
    }

    /// The entity number of the next spawner or NPC due this frame, if any is left.
    pub fn pending(&self) -> Option<u16> {
        self.scratch.due.last().map(|(number, _)| *number)
    }

    /// `G_RunThink` for the next spawner or NPC due this frame ([`Self::pending`]): a spawn,
    /// a begin, a think or a freeing — if its think is still due (another's think may have
    /// put it off). Returns what it fired.
    pub fn run_pending(
        &mut self,
        level_time: i32,
        files: NpcFiles<'_>,
        host: &mut impl NpcHost,
    ) -> Fired {
        let mut fired = Fired::new();
        let Some((number, due)) = self.scratch.due.pop() else {
            return fired;
        };
        match due {
            Due::Spawner => {
                let Some(at) = self.spawners.iter().position(|(known, _)| *known == number) else {
                    return fired;
                };
                if self.spawners[at].1.free_at.is_some() {
                    self.spawners.remove(at);
                    host.free(number);
                } else {
                    self.spawners[at].1.spawn_at = None;
                    self.spawn(at, level_time, files, host, &mut fired);
                }
            }
            Due::Npc => {
                let Some(at) = self.actors.iter().position(|npc| npc.number == number) else {
                    return fired;
                };
                // Its event cleared once shown long enough and its powerups run out, at its own
                // turn (`g_main.c:3080-3100`, `3333-3341`): another's earlier turn this frame may
                // have given it a new event.
                crate::npc_client_think::frame_upkeep(&mut self.actors[at], level_time);
                // `WP_ForcePowersUpdate`, `WP_SaberPositionUpdate` and
                // `WP_SaberStartMissileBlockCheck` (`g_main.c:3342-3346`), then `G_RunThink`.
                if self.force_upkeep {
                    self.world(level_time, host, &mut fired).force_update(at);
                }
                if self.saber_upkeep {
                    crate::npc_saber::upkeep(&mut self.actors[at], level_time);
                } else if !self.eyes_frozen {
                    update_render_info(&mut self.actors[at]);
                }
                let look_target = self.look_target(at, host);
                // A droid's `G_G2NPCAngles` in place of the spine (`w_saber.c:1034-1047`): by
                // class, its own skeleton set up as it spawned. Part of `WP_SaberPositionUpdate`,
                // so not in the replays whose drivers leave that out.
                if self.saber_upkeep
                    && crate::npc_droid_head::turns_head(self.actors[at].definition.client_class)
                {
                    let origin = self.actors[at].player.origin();
                    let seen = host
                        .players()
                        .iter()
                        .any(|player| host.in_pvs(player.origin, origin));
                    self.world(level_time, host, &mut fired)
                        .droid_head_angles(at, seen);
                }
                let blades = host.pose_npc(&self.actors[at], look_target, level_time);
                if self.saber_upkeep {
                    let mut world = self.world(level_time, host, &mut fired);
                    world.saber_update(at, &blades);
                    world.look_target_update(at);
                }
                // `WP_SaberPositionUpdate`'s finalUpdate (`w_saber.c:9107`): a clash
                // above may have changed the lock pose since the blades were read.
                host.update_npc_anims(&self.actors[at], level_time);
                if thinks_at(&self.actors[at]).is_some_and(|at| at <= level_time) {
                    self.think(at, level_time, files, host, &mut fired);
                }
            }
            Due::Limb => {
                if let Some(at) = self
                    .level
                    .limbs
                    .iter()
                    .position(|limb| limb.number == number)
                {
                    self.world(level_time, host, &mut fired).limb_run(at);
                }
            }
            Due::Boundary => self
                .world(level_time, host, &mut fired)
                .boundary_think(number),
            Due::Saber => {
                // `G_RunFrame` skips a `neverFree` entity that is not linked (`g_main.c:3117`);
                // `G_RunThink` a think not yet due.
                let Some(at) = self
                    .actors
                    .iter()
                    .position(|npc| npc.saber_entity == Some(number))
                else {
                    return fired;
                };
                let entity = self.actors[at].saber.entity;
                // Out of the hand, the flight's own think ([`crate::npc_saber_throw`]).
                if self.actors[at].saber.flight.think != crate::saber_throw::SaberThink::InHand {
                    self.world(level_time, host, &mut fired)
                        .saber_flight_think(at);
                } else if entity.linked && entity.think_at > 0 && entity.think_at <= level_time {
                    self.world(level_time, host, &mut fired)
                        .saber_entity_think(at);
                }
            }
        }
        fired
    }

    /// Where the entity the NPC at `at` looks at stands (`ps.hasLookTarget`, `ps.lookTarget`:
    /// `g_entities[lookTarget].r.currentOrigin`, `w_saber.c:943-948`): an NPC or a player.
    fn look_target(&self, at: usize, host: &impl NpcHost) -> Option<[f32; 3]> {
        const PS_HAS_LOOK_TARGET: usize = 76;
        const PS_LOOK_TARGET: usize = 66;
        let state = &self.actors[at].player;
        if state.raw_field(PS_HAS_LOOK_TARGET).unwrap_or(0) == 0 {
            return None;
        }
        let target = state.raw_field(PS_LOOK_TARGET).unwrap_or(0) as u16;
        self.actors
            .iter()
            .find(|npc| npc.number == target)
            .map(|npc| npc.current_origin)
            .or_else(|| {
                host.players()
                    .iter()
                    .find(|body| body.number == target)
                    .map(|body| body.origin)
            })
    }

    /// An NPC's think: its begin, its think (alive or dead), its body's removal, or its
    /// freeing.
    fn think(
        &mut self,
        at: usize,
        level_time: i32,
        files: NpcFiles<'_>,
        host: &mut impl NpcHost,
        fired: &mut Fired,
    ) {
        match self.actors[at].think {
            NpcThink::Free(_) => self.remove(at, host),
            NpcThink::Begin(_) => {
                let mut npc = self.actors.remove(at);
                let others: Vec<&NpcActor> = self.actors.iter().collect();
                let mut telefrags = Vec::new();
                let begun = begin(&mut npc, &others, level_time, host, &mut telefrags);
                self.actors.insert(at, npc);
                if !telefrags.is_empty() {
                    self.telefrag(at, &telefrags, level_time, host, fired);
                }
                match begun {
                    Begun::GaveUp(Some(target3)) => fired.push(target3),
                    // The begin's own think: an empty command, which drops the NPC to the
                    // floor and sets its animations going (`NPC_spawn.c:1172-1183`).
                    Begun::Begun => {
                        self.world(level_time, host, fired)
                            .client_think(at, sjk_protocol::UserCommand::default());
                        let npc = &self.actors[at];
                        host.publish(npc.number, &npc.state, npc.bounds(), npc.contents);
                        // A vehicle's droid unit last (`NPC_spawn.c:1201-1266`).
                        self.spawn_droid_unit(at, level_time, files, host, fired);
                    }
                    _ => {}
                }
            }
            NpcThink::Think(_) if self.skip_thinks => {}
            NpcThink::Think(_) | NpcThink::RemoveBody(_) => {
                let removing = matches!(self.actors[at].think, NpcThink::RemoveBody(_));
                let thought = {
                    let mut world = self.world(level_time, host, fired);
                    if removing {
                        world.remove_body(at)
                    } else {
                        world.think(at)
                    }
                };
                if thought == crate::npc_dead::DeadThought::Gone {
                    self.remove(at, host);
                } else {
                    let npc = &self.actors[at];
                    host.publish(npc.number, &npc.state, npc.bounds(), npc.contents);
                }
            }
        }
    }

    /// `G_KillBox`'s blows on the NPCs where the NPC at `at` began (`g_utils.c:1192-1194`):
    /// `G_Damage` for 100000, `DAMAGE_NO_PROTECTION`, `MOD_TELEFRAG`, the new NPC the
    /// attacker — a body lying there among them.
    fn telefrag(
        &mut self,
        at: usize,
        victims: &[u16],
        level_time: i32,
        host: &mut impl NpcHost,
        fired: &mut Fired,
    ) {
        let npc = &self.actors[at];
        let attacker = crate::damage::Attacker {
            npc: true,
            client: npc.number,
            max_health: npc.player.stats[crate::npc_begin::STAT_MAX_HEALTH] as i32,
            team: npc.session_team,
            saber_knockback: [0.0; 4],
        };
        let request = crate::damage::DamageRequest {
            level_time,
            attacker: Some(attacker),
            direction: None,
            point: None,
            damage: 100_000,
            flags: crate::damage::DAMAGE_NO_PROTECTION,
            means: crate::means_of_death::MOD_TELEFRAG,
        };
        let mut world = self.world(level_time, host, fired);
        for &victim in victims {
            let Some(target) = world.actor_at(victim) else {
                continue;
            };
            world.host.noting_damage(victim, attacker.client, &request);
            let damaged = world.damage(
                target,
                crate::npc_damage::NpcBlow {
                    request,
                    spared_by_master: false,
                    surface: None,
                },
            );
            if damaged.attacker_hits != 0 {
                let persistent = &mut world.actors[at].player.persistent;
                persistent[1] = (persistent[1] as i32 + damaged.attacker_hits) as u32;
                persistent[7] = damaged.attackee_armor.unwrap_or(0);
            }
        }
    }

    /// `G_FreeEntity` on the NPC at `at` (`g_utils.c`): itself — its model named for the
    /// clients to drop (`G_KillG2Queue`) where it has one — then its saber entity, whose
    /// `neverFree` the NPC's freeing clears (`NPC_RemoveBody`'s own try at it only unlinks
    /// it) and whose `s.modelGhoul2` `WP_SaberInitBladeData` set: named too.
    pub(crate) fn remove(&mut self, at: usize, host: &mut impl NpcHost) {
        let mut npc = self.actors.remove(at);
        if !npc.mind.timers.is_empty() {
            self.left_timers.retain(|(number, _)| *number != npc.number);
            self.left_timers
                .push((npc.number, std::mem::take(&mut npc.mind.timers)));
        }
        if npc
            .state
            .raw_field(crate::npc_spawn::es::MODEL_GHOUL2)
            .unwrap_or(0)
            != 0
        {
            host.free_model(npc.number);
        } else {
            host.free(npc.number);
        }
        if let Some(saber) = npc.saber_entity {
            host.free_model(saber);
        }
    }

    /// The level as the thinks see it, the actors in entity-number order.
    pub(crate) fn world<'a, H: NpcHost>(
        &'a mut self,
        level_time: i32,
        host: &'a mut H,
        fired: &'a mut Fired,
    ) -> NpcWorld<'a, H> {
        let Scratch {
            order,
            bodies,
            overflow,
            spare_state,
            impact_bodies,
            body_legs,
            ..
        } = &mut self.scratch;
        // The order stands until an NPC comes or goes.
        let actors = &self.actors;
        if order.len() != actors.len()
            || order
                .windows(2)
                .any(|pair| actors[pair[0]].number >= actors[pair[1]].number)
        {
            order.clear();
            order.extend(0..actors.len());
            order.sort_unstable_by_key(|&at| actors[at].number);
        }
        NpcWorld {
            actors: &mut self.actors,
            order,
            alerts: &mut self.alerts,
            host,
            level_time,
            bodies,
            body_legs,
            impact_bodies,
            ship_triggers: &mut self.ship_triggers,
            overflow,
            fired,
            level: &mut self.level,
            rider: None,
            passengers: Vec::new(),
            vehicle_outcomes: &mut self.vehicle_outcomes,
            spare_state,
            vehicle_weapons: self
                .vehicle_table
                .as_ref()
                .map_or(&[], |table| table.weapons()),
        }
    }

    /// `NPC_Spawn_Do` for the spawner at `at`; the spawner freed when it is spent. A map's
    /// vehicle spawner spawns through `G_VehicleSpawn` ([`crate::vehicle_spawn::after_spawn`]).
    pub(crate) fn spawn(
        &mut self,
        at: usize,
        level_time: i32,
        files: NpcFiles<'_>,
        host: &mut impl NpcHost,
        fired: &mut Fired,
    ) {
        let vehicle_spawn = self.spawners[at].1.vehicle && self.spawners[at].1.free_at.is_none();
        if vehicle_spawn && self.spawners[at].1.count == 0 {
            self.spawners[at].1.count = 1;
        }
        let spawned = spawn_do(
            &mut self.spawners[at].1,
            files.parms,
            files.sabers,
            self.vehicle_table.as_mut(),
            level_time,
            host,
        );
        if let Ok(mut npc) = spawned.npc {
            if vehicle_spawn {
                crate::vehicle_spawn::after_spawn(&mut npc, level_time);
            }
            if let Some(at) = self
                .left_timers
                .iter()
                .position(|(number, _)| *number == npc.number)
            {
                npc.mind.timers = self.left_timers.swap_remove(at).1;
            }
            self.actors.push(npc);
        } else if let Err(NoNpc::OutOfEntities) = spawned.npc {
            return;
        }
        if spawned.spawner_freed {
            let (number, _) = self.spawners.remove(at);
            host.free(number);
        }
        if let Some(target) = spawned.fired {
            fired.push(target);
        }
    }

    /// `G_UseTargets2` for `name` as it reaches the roster: its spawners of that name used
    /// (`NPC_Spawn`) — spawning now, or after their delay. Returns what their spawns fired,
    /// for the caller to use as it used `name`.
    pub fn use_targets(
        &mut self,
        name: &[u8],
        level_time: i32,
        files: NpcFiles<'_>,
        host: &mut impl NpcHost,
    ) -> Fired {
        let mut fired = Fired::new();
        if name.is_empty() {
            return fired;
        }
        let mut numbers: Vec<u16> = self
            .spawners
            .iter()
            .filter(|(_, spawner)| spawner.usable && spawner.targetname.as_deref() == Some(name))
            .map(|(number, _)| *number)
            .collect();
        numbers.sort_unstable();
        for number in numbers {
            let Some(at) = self.spawners.iter().position(|(known, _)| *known == number) else {
                continue;
            };
            let spawner = &mut self.spawners[at].1;
            if spawner.spawnflags & SHY != 0 {
                host.print(
                    "a shy NPC spawner waits for NPC senses (the NPC plan's step 3): not spawned\n",
                );
                continue;
            }
            if spawner.delay != 0 {
                spawner.spawn_at = Some(level_time + spawner.delay);
            } else {
                self.spawn(at, level_time, files, host, &mut fired);
            }
        }
        fired
    }

    /// `NPC_SpawnType` (`NPC_spawn.c:3953-4099`) once the player's place is known: the
    /// spawner of one NPC made, the class precache for the names that have one, and the
    /// NPC spawned at once. The spawner goes with its NPC; one its spawn left standing
    /// frees itself a frame on.
    pub fn spawn_command(
        &mut self,
        npc_type: &[u8],
        targetname: &[u8],
        vehicle: bool,
        place: CommandPlace,
        level_time: i32,
        files: NpcFiles<'_>,
        host: &mut impl NpcHost,
    ) -> Fired {
        let mut fired = Fired::new();
        let Some(number) = host.spawn_hidden() else {
            host.print("^1NPC_Spawn Error: Out of entities!\n");
            return fired;
        };
        let mut spawner =
            NpcSpawner::for_command(npc_type, Some(targetname), place.origin, place.yaw, vehicle);
        // `think = G_FreeEntity` a frame on, for a spawner its spawn does not free.
        spawner.free_at = Some(level_time + crate::npc_spawn::FRAMETIME);
        if npc_type.is_empty() {
            host.print("^1Error, expected one of:\n^7 NPC spawn [NPC type (from ext_data/NPCs)]\n NPC spawn vehicle [VEH type (from ext_data/vehicles)]\n");
            self.spawners.push((number, spawner));
            return fired;
        }
        register(command_precache(npc_type), host);
        self.spawners.push((number, spawner));
        let at = self.spawners.len() - 1;
        self.spawn(at, level_time, files, host, &mut fired);
        fired
    }

    /// When the next spawner or NPC think is due, if any is waiting: a frame with nothing
    /// due has nothing to run.
    pub fn next_due(&self) -> Option<i32> {
        let spawners = self
            .spawners
            .iter()
            .filter_map(|(_, spawner)| spawner.spawn_at.or(spawner.free_at));
        spawners
            .chain(self.actors.iter().filter_map(thinks_at))
            .min()
    }

    /// Whether a spawner of the roster answers to `name` (`G_UseTargets` would use it).
    pub fn answers_to(&self, name: &[u8]) -> bool {
        self.spawners
            .iter()
            .any(|(_, spawner)| spawner.usable && spawner.targetname.as_deref() == Some(name))
    }

    /// The NPCs that have begun: standing, shown and solid.
    pub fn begun(&self) -> impl Iterator<Item = &NpcActor> {
        self.actors.iter().filter(|npc| npc.begun())
    }
}

/// What a driver that stood a saber carrier's `WP_SaberPositionUpdate` in leaves
/// ([`NpcRoster::saber_upkeep`] off): an NPC without a saber runs the reference's own
/// update — [`crate::npc_saber::upkeep`]: its drawn style and its eyes — and a saber
/// carrier's eyes stay where it began.
fn update_render_info(npc: &mut NpcActor) {
    if npc.saber_entity.is_none() {
        crate::npc_saber::upkeep(npc, 0);
    }
}

/// When an NPC's think is due, if it has one waiting.
fn thinks_at(npc: &NpcActor) -> Option<i32> {
    match npc.think {
        NpcThink::Begin(at)
        | NpcThink::Free(at)
        | NpcThink::Think(at)
        | NpcThink::RemoveBody(at) => Some(at),
    }
}

/// The class precaches `NPC_SpawnType` runs by name (`NPC_spawn.c:4024-4094`), some by a
/// case-sensitive prefix.
fn command_precache(npc_type: &[u8]) -> &'static [Registration] {
    use crate::npc_spawners::class_precache as list;
    let is = |name: &str| npc_type.eq_ignore_ascii_case(name.as_bytes());
    let starts = |prefix: &str| npc_type.starts_with(prefix.as_bytes());
    if is("gonk") {
        list("gonk")
    } else if is("mouse") {
        list("mouse")
    } else if starts("r2d2") {
        list("r2d2")
    } else if is("atst") {
        list("atst")
    } else if starts("r5d2") {
        list("r5d2")
    } else if is("mark1") {
        list("mark1")
    } else if is("mark2") {
        list("mark2")
    } else if is("interrogator") {
        list("interrogator")
    } else if is("probe") {
        list("probe")
    } else if is("seeker") {
        list("seeker")
    } else if is("remote") {
        list("remote")
    } else if starts("shadowtrooper") {
        list("shadowtrooper")
    } else if is("minemonster") {
        list("minemonster")
    } else if is("sentry") {
        list("sentry")
    } else if is("protocol") {
        list("protocol")
    } else if is("galak_mech") {
        list("galak_mech")
    } else if is("wampa") {
        list("wampa")
    } else {
        // `howler`'s precache registers nothing.
        &[]
    }
}

/// What `Cmd_NPC_f` (`NPC_spawn.c:4264-4324`) makes of the words after `npc`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NpcCommand<'a> {
    /// No word: the list of commands, on the server's console ([`NPC_COMMAND_HELP`]).
    Help,
    /// `spawn [vehicle] <type> [targetname]` (`NPC_Spawn_f`): an empty type is the
    /// spawn's own error.
    Spawn {
        npc_type: &'a [u8],
        targetname: &'a [u8],
        vehicle: bool,
    },
    /// `kill <targetname|all>`, `kill team <team|nonally>` (`NPC_Kill_f`): `name` is the
    /// word after `kill`, `team` the one after `team`.
    Kill { name: &'a [u8], team: &'a [u8] },
    /// `showbounds`: the bounding boxes' debug drawing toggled.
    ShowBounds,
    /// `score [targetname]`: the NPCs' kills, one's or everyone's.
    Score(&'a [u8]),
    /// Any other word: nothing.
    Nothing,
}

/// `Cmd_NPC_f`'s list of commands (`Com_Printf`, on the server's console).
pub const NPC_COMMAND_HELP: &str = "Valid NPC commands are:\n spawn [NPC type (from NPCs.cfg)]\n kill [NPC targetname] or [all(kills all NPCs)] or 'team [teamname]'\n showbounds (draws exact bounding boxes of NPCs)\n score [NPC targetname] (prints number of kills per NPC)\n";

/// The words after `npc`, as `Cmd_NPC_f` and `NPC_Spawn_f` read them (`Q_stricmp`).
pub fn npc_command<'a>(words: &[&'a [u8]]) -> NpcCommand<'a> {
    let word = |at: usize| words.get(at).copied().unwrap_or_default();
    let Some(&command) = words.first() else {
        return NpcCommand::Help;
    };
    if command.eq_ignore_ascii_case(b"spawn") {
        return if word(1).eq_ignore_ascii_case(b"vehicle") {
            NpcCommand::Spawn {
                npc_type: word(2),
                targetname: word(3),
                vehicle: true,
            }
        } else {
            NpcCommand::Spawn {
                npc_type: word(1),
                targetname: word(2),
                vehicle: false,
            }
        };
    }
    if command.eq_ignore_ascii_case(b"kill") {
        return NpcCommand::Kill {
            name: word(1),
            team: word(2),
        };
    }
    if command.eq_ignore_ascii_case(b"showbounds") {
        return NpcCommand::ShowBounds;
    }
    if command.eq_ignore_ascii_case(b"score") {
        return NpcCommand::Score(word(1));
    }
    NpcCommand::Nothing
}
