//! The map's turrets on this server: `misc_turretG2` (siege_hoth's hangar guns, siege_desert's,
//! the community maps' turbolasers) and `misc_turret` (siege_hoth's two-piece guns). The rules
//! are `sjk_game_jka::map_turret_g2` and `sjk_game_jka::map_turret`, held to the reference's
//! transcripts (`tools/game-oracle/turrets.c`); this is where they meet the server's pool,
//! tables, players, NPCs, missiles and map.
//!
//! As the level begins every turret the game type keeps is spawned from the map's lump as
//! an entity of the pool (a `misc_turret` as two: its base and its top), its wire state the
//! game's whole — model indices, the Ghoul2 flag, the bone angles the clients turn its gun
//! by — so a stock client draws it as it draws a stock server's. Each thinks when its time
//! comes; it sees and shoots the players and NPCs (and a `misc_turretG2` the breakable
//! brushes), its bolts are missiles of the pool that the frame's missile pass flies, and
//! it is struck by missiles, splash and `G_Damage`'s other blows like any thing that takes
//! damage. A `misc_turretG2` fires from its muzzle bolt on the server's own Ghoul2 instance
//! of its model, turned by its pitch bone and its yaw. A name used reaches it (on/off).

use super::*;
use bridge_map_turret_host::{Blast, ServerTurretHost, TurretWork};
use sjk_game_jka::map_turret::{MiscTurret, Part};
use sjk_game_jka::map_turret_g2::TurretG2;
use sjk_game_jka::map_turret_world::{BlowAttacker, FL_NOTARGET, ObjectBlow, Sighted};

#[path = "bridge_map_turret_host.rs"]
mod bridge_map_turret_host;

/// `GT_TEAM`, `GT_SIEGE`.
const GT_TEAM: i32 = 6;
const GT_SIEGE: i32 = 7;
/// `ENTITYNUM_WORLD`.
const ENTITYNUM_WORLD: u16 = 1_022;

/// The level's turrets and what their calls left to do.
#[derive(Default)]
pub(super) struct MapTurrets {
    g2: Vec<(EntityId, TurretG2)>,
    /// A `misc_turret`'s base, its top and itself.
    misc: Vec<(EntityId, EntityId, MiscTurret)>,
    work: TurretWork,
    /// The level as the turrets read it, kept between frames.
    sighted: Vec<Sighted>,
    /// The blows dealt while the map is set aside for a blast's lines ([`NativeGame::flush_npc_blows`]).
    deferred: Vec<(u16, DamageRequest)>,
}

impl MapTurrets {
    /// Lets go of the turrets without freeing them: the pool they were in is gone.
    pub(super) fn forget_entities(&mut self) {
        self.g2.clear();
        self.misc.clear();
        self.work.instances.clear();
    }

    /// Whether entity `number` is one of the turrets (either half of a `misc_turret`).
    pub(super) fn owns(&self, number: u16) -> bool {
        self.g2.iter().any(|(id, _)| id.legacy_number() == number)
            || self.misc.iter().any(|(base, top, _)| {
                base.legacy_number() == number || top.legacy_number() == number
            })
    }

    /// The turrets' boxes for a trace of entity `pass`: not its own, nor what it owns — a
    /// `misc_turret`'s halves own each other (`SV_ClipMoveToEntities`).
    pub(super) fn add_obstacles(&self, obstacles: &mut Vec<BoxObstacle>, pass: usize) {
        let skipped =
            |number: u16, owner: u16| usize::from(number) == pass || usize::from(owner) == pass;
        for (id, turret) in self.g2.iter().filter(|(_, turret)| !turret.freed) {
            if !skipped(id.legacy_number(), ENTITYNUM_NONE) {
                obstacles.push(BoxObstacle {
                    entity: id.legacy_number(),
                    origin: turret.origin,
                    bounds: (turret.mins, turret.maxs),
                    contents: turret.contents,
                    model: None,
                });
            }
        }
        for (base, top, turret) in &self.misc {
            for (id, half) in [(base, &turret.base), (top, &turret.top)] {
                if !skipped(id.legacy_number(), half.owner) {
                    obstacles.push(BoxObstacle {
                        entity: id.legacy_number(),
                        origin: half.origin,
                        bounds: (half.mins, half.maxs),
                        contents: half.contents,
                        model: None,
                    });
                }
            }
        }
    }

    /// The turrets as a blast reaches them (`G_RadiusDamage`): their linked boxes.
    pub(super) fn add_splash_targets(&self, targets: &mut Vec<SplashTarget>) {
        targets.extend(self.things().map(|thing| SplashTarget {
            number: thing.number,
            bounds: thing.bounds,
            origin: thing.origin,
            takes_damage: thing.takes_damage,
        }));
    }

    /// Every turret (every half) as the level reads it.
    fn things(&self) -> impl Iterator<Item = Sighted> + '_ {
        let thing = |number: u16,
                     origin: [f32; 3],
                     (mins, maxs): ([f32; 3], [f32; 3]),
                     takes_damage: bool,
                     health: i32,
                     flags: u32,
                     team_no_damage: i32| Sighted {
            number,
            client: false,
            takes_damage,
            health,
            no_target: flags & FL_NOTARGET != 0,
            breakable: false,
            session_team: 0,
            temp_spectate_until: 0,
            team_no_damage,
            origin,
            eye: origin,
            bounds: (
                std::array::from_fn(|axis| origin[axis] + mins[axis] - 1.0),
                std::array::from_fn(|axis| origin[axis] + maxs[axis] + 1.0),
            ),
            top: maxs[2],
            velocity: [0.0; 3],
            atst: false,
            walker: false,
        };
        let g2 = self
            .g2
            .iter()
            .filter(|(_, turret)| !turret.freed)
            .map(move |(id, turret)| {
                thing(
                    id.legacy_number(),
                    turret.origin,
                    (turret.mins, turret.maxs),
                    turret.takes_damage,
                    turret.health,
                    turret.flags,
                    turret.team_no_damage,
                )
            });
        let misc = self.misc.iter().flat_map(move |(base, top, turret)| {
            [
                thing(
                    base.legacy_number(),
                    turret.base.origin,
                    (turret.base.mins, turret.base.maxs),
                    turret.base.takes_damage,
                    turret.base.health,
                    turret.flags,
                    turret.base.team_no_damage,
                ),
                thing(
                    top.legacy_number(),
                    turret.top.origin,
                    (turret.top.mins, turret.top.maxs),
                    turret.top.takes_damage,
                    turret.top.health,
                    0,
                    turret.top.team_no_damage,
                ),
            ]
        });
        g2.chain(misc)
    }
}

/// `ENTITYNUM_NONE`: a `misc_turretG2` has no owner.
const ENTITYNUM_NONE: u16 = 1_023;

impl NativeGame {
    /// `SP_misc_turretG2` and `SP_misc_turret` for every one the map places in this game
    /// type, in its order; run again (a game type changed) it spawns them afresh.
    pub(super) fn spawn_map_turrets(&mut self) {
        let level_time = self.last_frame_time;
        for (id, _) in std::mem::take(&mut self.map_turrets.g2) {
            self.pool.free(id, level_time);
        }
        for (base, top, _) in std::mem::take(&mut self.map_turrets.misc) {
            self.pool.free(base, level_time);
            self.pool.free(top, level_time);
        }
        self.map_turrets.work.instances.clear();
        let Some(map) = self.map.take() else { return };
        let gametype = self.gametype;
        let turrets = map
            .entities
            .iter()
            .skip(1)
            .filter(|entity| sjk_game_jka::bot_routes::spawns_in(entity, gametype));
        for entity in turrets {
            match entity.classname().map(str::to_ascii_lowercase).as_deref() {
                Some("misc_turretg2") => {
                    let Some(id) = self.pool.spawn_entity(
                        EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS),
                        level_time,
                    ) else {
                        continue;
                    };
                    let turret = self.with_turret_host(Some(&map), usize::MAX, |host| {
                        sjk_game_jka::map_turret_g2::spawn(
                            entity,
                            id.legacy_number(),
                            level_time,
                            gametype,
                            host,
                        )
                    });
                    self.pool.set_state(id, &turret.state);
                    self.pool.set_bounds(id, (turret.mins, turret.maxs));
                    self.map_turrets.g2.push((id, turret));
                }
                Some("misc_turret") => {
                    let Some(base) = self.pool.spawn_entity(
                        EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS),
                        level_time,
                    ) else {
                        continue;
                    };
                    let Some(top) = self.pool.spawn_entity(
                        EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS),
                        level_time,
                    ) else {
                        continue;
                    };
                    let turret = self.with_turret_host(Some(&map), usize::MAX, |host| {
                        sjk_game_jka::map_turret::spawn(
                            entity,
                            base.legacy_number(),
                            top.legacy_number(),
                            level_time,
                            host,
                        )
                    });
                    for (id, half) in [(base, &turret.base), (top, &turret.top)] {
                        self.pool.set_state(id, &half.state);
                        self.pool.set_bounds(id, (half.mins, half.maxs));
                    }
                    self.map_turrets.misc.push((base, top, turret));
                }
                _ => {}
            }
        }
        self.map = Some(map);
        self.register_turret_items();
    }

    /// The items the turrets registered (`RegisterItem`: the blaster's, the emplaced
    /// gun's), added to `CS_ITEMS` so that the clients load what draws their bolts.
    fn register_turret_items(&mut self) {
        let items = std::mem::take(&mut self.map_turrets.work.items);
        let Some(mut present) = self
            .config_strings
            .iter()
            .find(|(index, _)| *index == CS_ITEMS)
            .map(|(_, value)| value.clone())
        else {
            return;
        };
        let before = present.clone();
        for item in items {
            if let Some(slot) = present.get_mut(item) {
                *slot = b'1';
            }
        }
        if present != before {
            self.publish_config_string(CS_ITEMS, &present);
        }
    }

    /// `f(host)` with the server as a turret asks for it, the traces those of entity
    /// `pass`; then the configstrings it registered told.
    fn with_turret_host<R>(
        &mut self,
        map: Option<&LoadedMap>,
        pass: usize,
        f: impl FnOnce(&mut ServerTurretHost<'_>) -> R,
    ) -> R {
        if pass != usize::MAX {
            self.gather_obstacles(pass);
        } else {
            self.obstacles.clear();
        }
        let mut registered = Vec::new();
        let clock = if self.previous_frame_time == 0 {
            self.last_frame_time
        } else {
            self.previous_frame_time
        };
        let Self {
            deaths,
            models,
            sounds,
            map_effects,
            obstacles,
            pool,
            missiles,
            map_turrets,
            last_frame_time,
            ..
        } = self;
        let mut host = ServerTurretHost {
            rng: &mut deaths.rng,
            models,
            sounds,
            effects: &mut map_effects.effects,
            bones: &mut map_effects.bones,
            registered: &mut registered,
            map,
            obstacles,
            pool,
            missiles,
            work: &mut map_turrets.work,
            level_time: *last_frame_time,
            clock,
        };
        let result = f(&mut host);
        for (index, value) in registered {
            self.publish_config_string(index, &value);
        }
        result
    }

    /// Everything the turrets regard, in entity order, into the kept list: the players in
    /// the game, the NPCs, the breakable brushes and the turrets.
    fn gather_turret_sights(&mut self) {
        let mut sighted = std::mem::take(&mut self.map_turrets.sighted);
        sighted.clear();
        let Self {
            server,
            world,
            players,
            npcs,
            breakables,
            map_turrets,
            ..
        } = self;
        let box_of = |origin: [f32; 3], (mins, maxs): ([f32; 3], [f32; 3])| {
            (
                std::array::from_fn(|axis| origin[axis] + mins[axis] - 1.0),
                std::array::from_fn(|axis| origin[axis] + maxs[axis] + 1.0),
            )
        };
        if let Some(world) = server.world(*world) {
            for (number, handle) in players.holders().enumerate() {
                let Some(peer) = handle
                    .and_then(|handle| world.entity(handle))
                    .filter(|peer| peer.begun && peer.body_active())
                else {
                    continue;
                };
                let (origin, bounds) = (peer.state.origin(), peer.movement.box_bounds());
                // `UpdateClientRenderinfo`'s eye: its origin at its view height.
                let eye = [
                    origin[0],
                    origin[1],
                    origin[2] + peer.state.view_height() as f32,
                ];
                sighted.push(Sighted {
                    number: number as u16,
                    client: true,
                    takes_damage: true,
                    health: peer.health,
                    no_target: false,
                    breakable: false,
                    session_team: peer.session.team,
                    temp_spectate_until: 0,
                    team_no_damage: 0,
                    origin,
                    eye,
                    bounds: box_of(origin, bounds),
                    top: bounds.1[2],
                    velocity: peer.state.velocity(),
                    atst: false,
                    walker: false,
                });
            }
        }
        for npc in npcs.roster.begun() {
            let walker = npc.vehicle.as_deref().is_some_and(|vehicle| {
                vehicle.info.kind == sjk_game_jka::vehicle_fields::kind::WALKER
            });
            sighted.push(Sighted {
                number: npc.number,
                client: true,
                takes_damage: npc.takes_damage,
                health: npc.health,
                no_target: npc.flags & FL_NOTARGET != 0,
                breakable: false,
                session_team: npc.session_team,
                temp_spectate_until: 0,
                team_no_damage: 0,
                origin: npc.current_origin,
                eye: npc.mind.eye_point,
                bounds: npc.link,
                top: npc.maxs[2],
                velocity: npc.player.velocity(),
                atst: npc.npc_type.eq_ignore_ascii_case(b"atst_vehicle"),
                walker,
            });
        }
        for (id, brush) in breakables.iter().filter(|(_, brush)| brush.contents != 0) {
            let bounds = sjk_game_jka::breakables::linked_box(brush);
            sighted.push(Sighted {
                number: id.legacy_number(),
                client: false,
                takes_damage: brush.takes_damage,
                health: brush.health,
                no_target: false,
                breakable: true,
                session_team: 0,
                temp_spectate_until: 0,
                team_no_damage: 0,
                origin: [0.0; 3],
                eye: [0.0; 3],
                bounds,
                top: brush.bounds.1[2],
                velocity: [0.0; 3],
                atst: false,
                walker: false,
            });
        }
        sighted.extend(map_turrets.things());
        sighted.sort_by_key(|target| target.number);
        self.map_turrets.sighted = sighted;
    }

    /// `G_RunThink` for the turrets due at `level_time`: each thinks with the level as it
    /// stands, its wire state is published, and what it asked for is carried out.
    pub(super) fn run_map_turrets(&mut self, level_time: i32) {
        let map = self.map.take();
        for index in 0..self.map_turrets.g2.len() {
            let (id, turret) = &mut self.map_turrets.g2[index];
            if turret.freed || turret.next_think <= 0 || turret.next_think > level_time {
                continue;
            }
            turret.next_think = 0;
            let number = id.legacy_number();
            self.gather_turret_sights();
            let targets = std::mem::take(&mut self.map_turrets.sighted);
            // Out of its list while it thinks: the level it reads was gathered whole.
            let (id, mut turret) = self.map_turrets.g2.remove(index);
            self.with_turret_host(map.as_ref(), usize::from(number), |host| {
                sjk_game_jka::map_turret_g2::think(&mut turret, number, &targets, level_time, host)
            });
            self.map_turrets.g2.insert(index, (id, turret));
            self.map_turrets.sighted = targets;
            self.publish_turret(number);
        }
        for index in 0..self.map_turrets.misc.len() {
            let (base, _, turret) = &mut self.map_turrets.misc[index];
            if turret.next_think <= 0 || turret.next_think > level_time {
                continue;
            }
            turret.next_think = 0;
            if !turret.thinking {
                continue;
            }
            let number = base.legacy_number();
            self.gather_turret_sights();
            let targets = std::mem::take(&mut self.map_turrets.sighted);
            let (base, top, mut turret) = self.map_turrets.misc.remove(index);
            self.with_turret_host(map.as_ref(), usize::from(number), |host| {
                sjk_game_jka::map_turret::think(&mut turret, number, &targets, level_time, host)
            });
            self.map_turrets.misc.insert(index, (base, top, turret));
            self.map_turrets.sighted = targets;
            self.publish_turret(number);
        }
        self.map = map;
        self.carry_out_turret_work(level_time);
    }

    /// A turret's wire state and box into the pool (both halves of a `misc_turret`); a
    /// turret `ObjectDie` freed leaves it.
    fn publish_turret(&mut self, number: u16) {
        let level_time = self.last_frame_time;
        if let Some(index) = self
            .map_turrets
            .g2
            .iter()
            .position(|(id, _)| id.legacy_number() == number)
        {
            let (id, turret) = &self.map_turrets.g2[index];
            let id = *id;
            if turret.freed {
                self.pool.free(id, level_time);
                self.map_turrets.g2.remove(index);
                return;
            }
            self.pool.set_state(id, &turret.state);
        }
        if let Some((base, top, turret)) =
            self.map_turrets.misc.iter().find(|(base, top, _)| {
                base.legacy_number() == number || top.legacy_number() == number
            })
        {
            self.pool.set_state(*base, &turret.base.state);
            self.pool.set_state(*top, &turret.top.state);
        }
    }

    /// What the turrets' calls asked for: the splashes, the names used, the models the
    /// clients are to drop.
    fn carry_out_turret_work(&mut self, level_time: i32) {
        while !self.map_turrets.work.blasts.is_empty() || !self.map_turrets.work.uses.is_empty() {
            for blast in std::mem::take(&mut self.map_turrets.work.blasts) {
                self.turret_blast(blast, level_time);
            }
            for (name, activator) in std::mem::take(&mut self.map_turrets.work.uses) {
                self.fire_targets(&name, activator.map_or(usize::MAX, usize::from), level_time);
            }
        }
        for number in std::mem::take(&mut self.map_turrets.work.killed) {
            self.told
                .push(Told::Everyone(format!("kg2 {number}").into_bytes()));
        }
    }

    /// `G_RadiusDamage` a turret asked for: everyone within the radius, the attacker spared
    /// where it asked, the blows credited to the attacker (a death by nobody is the world's).
    fn turret_blast(&mut self, blast: Blast, level_time: i32) {
        let attacker_number = blast.attacker.unwrap_or(ENTITYNUM_WORLD);
        let attacker = self.attacker_for(attacker_number).unwrap_or(Attacker {
            npc: false,
            client: attacker_number,
            max_health: 100,
            team: 0,
            saber_knockback: [0.0; 4],
        });
        self.gather_splash_targets();
        let targets = std::mem::take(&mut self.splash_targets);
        let map = self.map.take();
        let mut hurt = |target: u16, request: DamageRequest| {
            self.strike(
                usize::from(attacker_number),
                usize::from(target),
                request,
                false,
            )
            .1
        };
        match &map {
            Some(map) => {
                let world = WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                };
                radius_damage(
                    blast.origin,
                    Some(attacker),
                    blast.damage,
                    blast.radius,
                    blast.ignore,
                    blast.means,
                    level_time,
                    &targets,
                    &world,
                    &mut hurt,
                )
            }
            None => radius_damage(
                blast.origin,
                Some(attacker),
                blast.damage,
                blast.radius,
                blast.ignore,
                blast.means,
                level_time,
                &targets,
                &Void,
                &mut hurt,
            ),
        };
        self.map = map;
        self.flush_npc_blows();
        self.splash_targets = targets;
    }

    /// `G_UseTargets` reaching the turrets called `name`: each switched on or off.
    pub(super) fn use_map_turrets(&mut self, name: &str) {
        for (_, turret) in self
            .map_turrets
            .g2
            .iter_mut()
            .filter(|(_, turret)| turret.targetname.eq_ignore_ascii_case(name))
        {
            sjk_game_jka::map_turret_g2::used(turret);
        }
        for (_, _, turret) in self
            .map_turrets
            .misc
            .iter_mut()
            .filter(|(_, _, turret)| turret.targetname.eq_ignore_ascii_case(name))
        {
            sjk_game_jka::map_turret::used(turret);
        }
        let numbers: Vec<u16> = self
            .map_turrets
            .g2
            .iter()
            .filter(|(_, turret)| turret.targetname.eq_ignore_ascii_case(name))
            .map(|(id, _)| id.legacy_number())
            .collect();
        for number in numbers {
            self.publish_turret(number);
        }
    }
}

#[path = "bridge_map_turret_damage.rs"]
mod damage;
