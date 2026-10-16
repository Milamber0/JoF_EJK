//! Scripts on this server: the ICARUS interpreter (`sjk_icarus`) and the game's side of
//! it (`sjk_game_jka::script_*`) meeting the server's pool, triggers, targets and
//! players.
//!
//! As a level spawns, every entity a script can reach is made a script entity, in map
//! order: each `func_static` that runs scripts (an `ET_MOVER` every client is sent; the
//! others are `bridge_path_movers`'), each `target_scriptrunner`, and each
//! `trigger_multiple` a script names or that names a script — the trigger itself stays
//! the triggers module's, and reads where a script
//! moved it and whether a script switched it off from here. Reference tags are filed,
//! each such entity gets its sequencer (`ICARUS_InitEnt`) and runs its spawn script, and
//! the world's spawn script gets a runner. A player gets a sequencer at every spawn
//! (`ClientSpawn`), so a runner can run its script on whoever set it off.
//!
//! Every frame the players' scripts run, then each script entity moves (`G_RunMover`) or
//! thinks (`G_RunThink`) and runs its scripts, and what changed is published. A name
//! fired anywhere (`G_UseTargets`) reaches the script entities of that name after the
//! server's own; a script's `use` reaches the server's own entities through the same
//! `fire_targets`.
//!
//! What scripts cannot reach here, with the outcome: an entity of another class that
//! has a `script_targetname` (a door, a counter, a hurt brush, an NPC) is reported once
//! per level and is no script target (`affect` fails as for an unknown name); `G_Find`
//! by name (`kill`, `remove`, `SET_ENEMY`) sees the script entities and the players
//! only; ROFFs are not played (the reference does nothing without the file); a scripted
//! mover's turn is not pushed through players (only its travel is).

use super::*;
use sjk_game_jka::ref_tags::{Filed, RefTags};
use sjk_game_jka::script_entity::{EntityKind, NUM_TIDS, ScriptEntity, valid_for_scripts};
use sjk_game_jka::script_runner;
use sjk_icarus::{Icarus, IcarusConfig};
use std::fmt;

#[path = "bridge_icarus_world.rs"]
mod world;

/// Who runs a script: a player by client number, or a script entity by its place in
/// the level's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum ScriptOwner {
    Client(u16),
    Entity(u32),
}

impl fmt::Display for ScriptOwner {
    /// The number the debug lines print (`%d` of the entity): a player's client number, a
    /// script entity's place past the clients.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let number = match *self {
            Self::Client(client) => u32::from(client),
            Self::Entity(index) => index + 32,
        };
        fmt::Display::fmt(&number, f)
    }
}

/// What a script entity is on this server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SlotKind {
    /// A `func_static`: its own pool entity.
    FuncStatic,
    /// A `target_scriptrunner`, the world's spawn-script runner or a helper a set
    /// spawned: nothing any client sees.
    Unseen,
    /// A `trigger_multiple` of the triggers module, by its index there.
    Multiple(usize),
}

/// One script entity of the level.
#[derive(Debug)]
pub(super) struct ScriptSlot {
    pub(super) entity: ScriptEntity<ScriptOwner>,
    pub(super) kind: SlotKind,
    /// Its pool entity, where clients see it.
    pub(super) pool: Option<EntityId>,
    /// Its brush model, where it has one.
    pub(super) model: usize,
    /// What its pool entity was last given, to publish only what changed.
    published: Option<Published>,
}

/// The fields a script entity's pool entity carries.
#[derive(Clone, Debug, PartialEq)]
struct Published {
    pos: sjk_game_jka::script_mover::Trajectory,
    apos: sjk_game_jka::script_mover::Trajectory,
    loop_sound: u16,
    eflags: u32,
    shown: bool,
    scale: i32,
    frame: i32,
    angles: [f32; 3],
}

/// A name fired while the interpreter was busy: the script entities of that name are
/// used once it is free (`G_UseTargets2`'s part for them).
#[derive(Clone, Debug)]
struct PendingUse {
    name: String,
    /// The entity that fired it (`ent`), which does not use itself.
    from: Option<ScriptOwner>,
    activator: Option<ScriptOwner>,
}

/// The scripts of the server: the interpreter, which lives as long as the server (its
/// variables outlast a level, as in the reference), and the level's script entities.
#[derive(Debug)]
pub(super) struct Scripts {
    /// Lent out while a script call runs.
    icarus: Option<Box<Icarus<ScriptOwner>>>,
    pub(super) slots: Vec<Option<ScriptSlot>>,
    /// Each client's script entity, by client number.
    pub(super) clients: Vec<Option<ScriptEntity<ScriptOwner>>>,
    pub(super) tags: RefTags,
    /// For each trigger of the triggers module, its script entity if it has one.
    multiple_slots: Vec<Option<u32>>,
    pending: Vec<PendingUse>,
    /// `numNewICARUSEnts`.
    new_names: i32,
    /// The engine's own generator (`Q_flrand` in the engine), for `random()`.
    random: u32,
    /// What was already reported unsupported this level, by kind.
    reported: std::collections::HashSet<String>,
}

impl Default for Scripts {
    fn default() -> Self {
        Self {
            icarus: Some(Box::new(Icarus::new(IcarusConfig {
                task_slots: NUM_TIDS,
                ..IcarusConfig::default()
            }))),
            slots: Vec::new(),
            clients: Vec::new(),
            tags: RefTags::default(),
            multiple_slots: Vec::new(),
            pending: Vec::new(),
            new_names: 0,
            random: 0x1234_5678,
            reported: Default::default(),
        }
    }
}

impl Scripts {
    /// Where a script moved trigger `index` of the triggers module (its offset from where
    /// the map drew it); zero for one no script moved.
    pub(super) fn multiple_offset(&self, index: usize) -> [f32; 3] {
        self.multiple_slot(index)
            .map_or([0.0; 3], |slot| slot.entity.motion.origin)
    }

    /// Whether a script switched trigger `index` off (`FL_INACTIVE`) or freed it.
    pub(super) fn multiple_switched_off(&self, index: usize) -> bool {
        match self.multiple_slots.get(index).copied().flatten() {
            Some(slot) => self
                .slots
                .get(slot as usize)
                .and_then(Option::as_ref)
                .is_none_or(|slot| {
                    slot.entity.flags & sjk_game_jka::script_entity::FL_INACTIVE != 0
                }),
            None => false,
        }
    }

    fn multiple_slot(&self, index: usize) -> Option<&ScriptSlot> {
        let slot = self.multiple_slots.get(index).copied().flatten()?;
        self.slots.get(slot as usize).and_then(Option::as_ref)
    }

    /// The solid script brushes, where their last frame left them: what a player runs
    /// into (the `func_static`s).
    pub(super) fn brushes(&self) -> impl Iterator<Item = BoxObstacle> + '_ {
        self.slots
            .iter()
            .flatten()
            .filter(|slot| slot.kind == SlotKind::FuncStatic && slot.entity.contents != 0)
            .map(|slot| BoxObstacle {
                entity: slot.pool.map(|id| id.legacy_number()).unwrap_or(u16::MAX),
                origin: slot.entity.motion.origin,
                bounds: (slot.entity.mins, slot.entity.maxs),
                contents: slot.entity.contents,
                model: Some(slot.model),
            })
    }

    /// A message once a level for something scripts ask that this server does not do.
    pub(super) fn report_once(&mut self, key: &str, message: impl FnOnce() -> String) {
        if self.reported.insert(key.to_owned()) {
            eprintln!("scripts: {}", message());
        }
    }

    /// The engine's `Q_flrand(min, max)`: its own generator, apart from the game's.
    fn random(&mut self, min: f32, max: f32) -> f32 {
        // The engine's `Q_rand` (a linear congruential step), scaled as `Q_flrand` does.
        self.random = self.random.wrapping_mul(69_069).wrapping_add(1);
        let unit = (self.random >> 8) as f32 / (1u32 << 24) as f32;
        min + unit * (max - min)
    }
}

impl NativeGame {
    /// The level's scripts, as `G_InitGame` and `G_SpawnEntitiesFromString` leave them:
    /// the interpreter shut down and started again (its variables kept), the reference
    /// tags filed, the script entities spawned in map order with their sequencers and
    /// spawn scripts, and the world's spawn script given a runner.
    pub(super) fn spawn_scripts(&mut self) {
        // `ICARUS_Shutdown` then `ICARUS_Init`: the last level's sequencers go.
        self.with_scripts(|world, icarus| {
            icarus.shutdown(&mut sjk_game_jka::script_host::ScriptHost::new(world))
        });
        if let Some(icarus) = self.scripts.icarus.as_mut() {
            icarus.init();
        }
        for slot in std::mem::take(&mut self.scripts.slots)
            .into_iter()
            .flatten()
        {
            if let Some(id) = slot.pool {
                self.pool.free(id, self.last_frame_time);
            }
        }
        self.scripts.multiple_slots = vec![None; self.multiples.len()];
        self.scripts.pending.clear();
        self.scripts.reported.clear();
        self.scripts.tags.clear();
        for client in self.scripts.clients.iter_mut() {
            *client = None;
        }
        let Some(map) = self.map.take() else { return };
        let entities = map.entities.clone();
        // `SV_SetBrushModel`: an inline model's bounds and `CM_ModelContents`.
        let bsp_bounds = |model: usize| {
            crate::map::brush_bounds(&map.bsp, model)
                .map(|(mins, maxs)| (mins, maxs, map.bsp.model_contents(model)))
        };
        let mut spawned = Vec::new();
        for entity in entities.iter().skip(1) {
            if !sjk_game_jka::bot_routes::spawns_in(entity, self.gametype) {
                continue;
            }
            if let Some(slot) = self.script_slot_of(entity, &bsp_bounds) {
                spawned.push(self.scripts.slots.len() as u32);
                self.scripts.slots.push(Some(slot));
            } else if entity.get("script_targetname").is_some()
                || sjk_game_jka::script_entity::BSET_KEYS
                    .iter()
                    .any(|(key, _)| entity.get(key).is_some())
            {
                let class = entity.classname().unwrap_or_default().to_owned();
                let name = entity
                    .get("script_targetname")
                    .or_else(|| entity.get("targetname"))
                    .unwrap_or_default()
                    .to_owned();
                self.scripts.report_once(&format!("class {class}"), || {
                    format!(
                        "{class} '{name}' has scripts; this server does not script that class yet"
                    )
                });
            }
        }
        self.file_reference_tags(&entities);
        let spawnscript = entities
            .first()
            .and_then(|world| world.get("spawnscript"))
            .map(str::to_owned);
        self.map = Some(map);
        for index in spawned {
            self.publish_slot(index);
        }
        self.with_scripts(|world, icarus| {
            for index in 0..world.game.scripts.slots.len() as u32 {
                script_runner::spawned(world, icarus, ScriptOwner::Entity(index));
            }
            if let Some(script) = spawnscript.as_deref() {
                script_runner::start_world_script(world, icarus, script);
            }
        });
    }

    /// The script entity a map entity is, if it is one.
    fn script_slot_of(
        &mut self,
        entity: &sjk_entity::Entity,
        bounds_of: &dyn Fn(usize) -> Option<([f32; 3], [f32; 3], u32)>,
    ) -> Option<ScriptSlot> {
        let class = entity.classname().unwrap_or_default().to_ascii_lowercase();
        let model = entity
            .get("model")
            .and_then(|name| name.strip_prefix('*'))
            .and_then(|index| index.parse::<usize>().ok());
        let slot = |entity: ScriptEntity<ScriptOwner>, kind, pool, model| ScriptSlot {
            entity,
            kind,
            pool,
            model,
            published: None,
        };
        match class.as_str() {
            "func_static" if sjk_game_jka::script_entity::runs_scripts(entity) => {
                let model = model?;
                let (mins, maxs, contents) = bounds_of(model)?;
                let ent = script_runner::spawn_func_static(entity, contents, mins, maxs);
                let mut state = EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
                state.set_raw_field(ES_ENTITY_TYPE, sjk_game_jka::movers::ET_MOVER);
                sjk_game_jka::triggers::set_brush_model(&mut state, model);
                let id = self.pool.spawn_entity(state, self.last_frame_time)?;
                self.pool.set_bounds(id, (mins, maxs));
                Some(slot(ent, SlotKind::FuncStatic, Some(id), model))
            }
            "target_scriptrunner" => Some(slot(
                script_runner::spawn_scriptrunner(entity),
                SlotKind::Unseen,
                None,
                0,
            )),
            "trigger_multiple" | "trigger_once" => {
                let model = model?;
                let index = self
                    .multiples
                    .iter()
                    .position(|trigger| trigger.model == model)?;
                let mut ent = ScriptEntity::from_map(entity, EntityKind::Other);
                if !valid_for_scripts(&mut ent) {
                    return None;
                }
                let trigger = &self.multiples[index];
                (ent.contents, ent.svflags, ent.mins, ent.maxs) = (
                    trigger.contents,
                    trigger.svflags,
                    trigger.bounds.0,
                    trigger.bounds.1,
                );
                ent.motion = sjk_game_jka::script_mover::ScriptMover::standing([0.0; 3], [0.0; 3]);
                ent.uses = sjk_game_jka::script_entity::Uses::Other;
                self.scripts.multiple_slots[index] = Some(self.scripts.slots.len() as u32);
                Some(slot(ent, SlotKind::Multiple(index), None, model))
            }
            _ => None,
        }
    }

    /// `SP_reference_tag` and `ref_link` for every tag of the level, in map order: a tag
    /// aimed at a target takes its angles from where the target stands.
    fn file_reference_tags(&mut self, entities: &[sjk_entity::Entity]) {
        for entity in entities {
            if !entity
                .classname()
                .is_some_and(|class| class.eq_ignore_ascii_case("ref_tag"))
            {
                continue;
            }
            let Some(tag) = sjk_game_jka::ref_tags::placed(entity) else {
                let origin = entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]);
                eprintln!(
                    "ERROR: Nameless ref_tag found at ({} {} {})",
                    origin[0] as i32, origin[1] as i32, origin[2] as i32
                );
                continue;
            };
            let mut angles = tag.angles;
            if let Some(target) = tag.target.as_deref() {
                let aim = entities.iter().find(|other| {
                    other
                        .get("targetname")
                        .is_some_and(|name| name.eq_ignore_ascii_case(target))
                });
                match aim.and_then(|other| other.vector("origin").ok().flatten()) {
                    Some(at) => angles = sjk_game_jka::ref_tags::aimed_angles(&tag, at),
                    None => eprintln!(
                        "ERROR: ref_tag ({}) unable to find target ({target})",
                        tag.targetname
                    ),
                }
            }
            if self.scripts.tags.add(
                &tag.targetname,
                tag.ownername.as_deref(),
                tag.origin,
                angles,
                0,
                0,
            ) == Filed::Duplicate
            {
                eprintln!("Duplicate tag name \"{}\"", tag.targetname);
            }
        }
    }

    /// Runs `call` with the interpreter and the server as its world, then uses what was
    /// fired meanwhile and publishes what changed. Nothing if the interpreter is busy
    /// (a call from within a script, which its own caller finishes).
    pub(super) fn with_scripts(
        &mut self,
        call: impl FnOnce(&mut world::ServerWorld<'_>, &mut Icarus<ScriptOwner>),
    ) {
        let Some(mut icarus) = self.scripts.icarus.take() else {
            return;
        };
        {
            let mut world = world::ServerWorld { game: self };
            call(&mut world, &mut icarus);
            world.use_pending(&mut icarus);
        }
        self.scripts.icarus = Some(icarus);
        for index in 0..self.scripts.slots.len() as u32 {
            self.publish_slot(index);
        }
    }

    /// `G_RunFrame` for the scripts: every player's (`ICARUS_MaintainTaskManager` before
    /// its move), then each script entity's move or think and its own, in the level's
    /// order.
    pub(super) fn run_scripts(&mut self, _level_time: i32) {
        self.with_scripts(|world, icarus| {
            for client in 0..world.game.scripts.clients.len() {
                if world.game.scripts.clients[client].is_some() {
                    script_runner::maintain(world, icarus, ScriptOwner::Client(client as u16));
                }
            }
            let mut index = 0;
            while index < world.game.scripts.slots.len() as u32 {
                if world.game.scripts.slots[index as usize].is_some() {
                    script_runner::run_entity(world, icarus, ScriptOwner::Entity(index));
                }
                index += 1;
            }
        });
    }

    /// `G_UseTargets`' part for the script entities called `name`, fired by player
    /// `client` (`usize::MAX` for none).
    pub(super) fn use_scripted(&mut self, name: &str, client: usize, _level_time: i32) {
        if name.is_empty()
            || !self.scripts.slots.iter().flatten().any(|slot| {
                slot.entity
                    .targetname
                    .as_deref()
                    .is_some_and(|own| own.eq_ignore_ascii_case(name))
            })
        {
            return;
        }
        let activator = (client != usize::MAX).then(|| ScriptOwner::Client(client as u16));
        self.scripts.pending.push(PendingUse {
            name: name.to_owned(),
            from: None,
            activator,
        });
        self.with_scripts(|_, _| {});
    }

    /// `target_activate` / `target_deactivate` (`G_SetActiveState`) reaching the script
    /// entities called `name`.
    pub(super) fn set_scripted_active(&mut self, name: &str, active: bool) {
        for slot in self.scripts.slots.iter_mut().flatten() {
            if slot
                .entity
                .targetname
                .as_deref()
                .is_some_and(|own| own.eq_ignore_ascii_case(name))
            {
                slot.entity.flags = if active {
                    slot.entity.flags & !sjk_game_jka::script_entity::FL_INACTIVE
                } else {
                    slot.entity.flags | sjk_game_jka::script_entity::FL_INACTIVE
                };
            }
        }
    }

    /// `ClientSpawn`'s `ICARUS_FreeEnt` and `ICARUS_InitEnt` (`g_client.c:3840-3841`): the
    /// player's scripts start afresh.
    pub(super) fn scripts_client_spawned(&mut self, client: usize) {
        if self.scripts.clients.len() <= client {
            self.scripts.clients.resize_with(client + 1, || None);
        }
        let origin = self
            .peer(client)
            .map_or([0.0; 3], |peer| peer.state.origin());
        let kept = self.scripts.clients[client].take();
        let mut ent = ScriptEntity::new("player", EntityKind::Client, origin, [0.0; 3]);
        if let Some(kept) = kept {
            (ent.targetname, ent.script_targetname) = (kept.targetname, kept.script_targetname);
        }
        self.scripts.clients[client] = Some(ent);
        self.with_scripts(|world, icarus| {
            let owner = ScriptOwner::Client(client as u16);
            script_runner::free_scripts(world, icarus, owner);
            script_runner::init_entity(world, icarus, owner);
        });
    }

    /// `ClientDisconnect`'s `G_FreeEntity`: the player's scripts go.
    pub(super) fn scripts_client_left(&mut self, client: usize) {
        self.with_scripts(|world, icarus| {
            script_runner::free_scripts(world, icarus, ScriptOwner::Client(client as u16))
        });
        if let Some(slot) = self.scripts.clients.get_mut(client) {
            *slot = None;
        }
    }

    /// `G_MoverPush` for a scripted brush travelling from `from` to `to`
    /// (`bridge_doors`' push, the brush's box and model): the players it takes with it
    /// are moved; the one it cannot move is answered, and the move is held.
    pub(super) fn push_scripted_brush(
        &mut self,
        bounds: ([f32; 3], [f32; 3]),
        model: usize,
        number: u16,
        from: [f32; 3],
        to: [f32; 3],
    ) -> Option<u16> {
        let candidates: Vec<(u16, [f32; 3], ([f32; 3], [f32; 3]), u16)> =
            (0..self.players.places())
                .filter_map(|client| {
                    let peer = self.peer_mut(client)?;
                    (peer.playing() && peer.health > 0).then(|| {
                        (
                            client as u16,
                            peer.state.origin(),
                            peer.movement.box_bounds(),
                            peer.state.ground_entity_num(),
                        )
                    })
                })
                .collect();
        if candidates.is_empty() {
            return None;
        }
        let pushed = match self.map.as_ref() {
            Some(map) => {
                let world = WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                };
                let mut free = |_client: u16, origin: [f32; 3], bounds: ([f32; 3], [f32; 3])| {
                    !world
                        .trace(origin, bounds.0, bounds.1, origin, 0x1)
                        .start_solid
                };
                let mut inside = |_client: u16, origin: [f32; 3], bounds: ([f32; 3], [f32; 3])| {
                    sjk_bsp::Aabb::new(bounds.0, bounds.1).is_ok_and(|bounds| {
                        map.bsp
                            .trace_transformed_model(
                                model,
                                to,
                                None,
                                origin,
                                origin,
                                bounds,
                                u32::MAX,
                            )
                            .start_solid
                    })
                };
                sjk_game_jka::movers::push_box_through(
                    bounds,
                    from,
                    to,
                    &candidates,
                    &mut free,
                    &mut inside,
                    number,
                )
            }
            None => sjk_game_jka::movers::push_box_through(
                bounds,
                from,
                to,
                &candidates,
                &mut |_, _, _| true,
                &mut |_, _, _| true,
                number,
            ),
        };
        match pushed {
            sjk_game_jka::movers::Push::Moved(shoved) => {
                for sjk_game_jka::movers::Shoved { client, to, .. } in shoved {
                    let Some(peer) = self.peer_mut(usize::from(client)) else {
                        continue;
                    };
                    peer.state.set_origin(to);
                    peer.movement = peer.movement.reseeded(&peer.state);
                }
                None
            }
            sjk_game_jka::movers::Push::Blocked(client) => Some(client),
        }
    }

    /// A script entity's pool entity, given what changed: its trajectories, its travel
    /// loop and soundset, its visibility, scale and shader frame.
    fn publish_slot(&mut self, index: u32) {
        const ES_APOS_BASE: [usize; 3] = [5, 3, 33];
        const ES_APOS_DELTA: [usize; 3] = [48, 44, 49];
        const ES_APOS_TYPE: usize = 15;
        const ES_APOS_TIME: usize = 34;
        const ES_APOS_DURATION: usize = 89;
        const ES_ANGLES: [usize; 3] = [25, 9, 24];
        const ES_LOOP_SOUND: usize = 55;
        const ES_LOOP_IS_SOUNDSET: usize = 70;
        const ES_SOUND_SET: usize = 78;
        const ES_MODEL_SCALE: usize = 76;
        const ES_FRAME: usize = 83;
        let Some(Some(slot)) = self.scripts.slots.get(index as usize) else {
            return;
        };
        let Some(id) = slot.pool else { return };
        let ent = &slot.entity;
        let now = Published {
            pos: ent.motion.pos,
            apos: ent.motion.apos,
            loop_sound: ent.loop_sound,
            eflags: ent.eflags,
            shown: ent.svflags & sjk_game_jka::script_entity::SVF_NOCLIENT == 0,
            scale: ent.model_scale,
            frame: ent.frame,
            angles: ent.spawn_angles,
        };
        if slot.published.as_ref() == Some(&now) {
            return;
        }
        let sound_set = (!ent.motion.sound_set.is_empty())
            .then(|| self.map_effects.sound_set(&ent.motion.sound_set));
        self.pool.show(id, now.shown);
        if let Some(state) = self.pool.state_mut(id) {
            for (trajectory, kind, time, duration, base, delta) in [
                (
                    &now.pos,
                    ES_POS_TYPE,
                    ES_POS_TIME,
                    ES_POS_DURATION,
                    ES_POS_BASE,
                    ES_POS_DELTA,
                ),
                (
                    &now.apos,
                    ES_APOS_TYPE,
                    ES_APOS_TIME,
                    ES_APOS_DURATION,
                    ES_APOS_BASE,
                    ES_APOS_DELTA,
                ),
            ] {
                state.set_raw_field(kind, trajectory.kind);
                state.set_raw_field(time, trajectory.time as u32);
                state.set_raw_field(duration, trajectory.duration as u32);
                for axis in 0..3 {
                    state.set_raw_field(base[axis], trajectory.base[axis].to_bits());
                    state.set_raw_field(delta[axis], trajectory.delta[axis].to_bits());
                }
            }
            for axis in 0..3 {
                state.set_raw_field(ES_ANGLES[axis], now.angles[axis].to_bits());
            }
            // `G_PlayDoorLoopSound`: the soundset's travel loop.
            state.set_raw_field(ES_LOOP_SOUND, u32::from(now.loop_sound));
            state.set_raw_field(
                ES_LOOP_IS_SOUNDSET,
                u32::from(now.loop_sound != 0 && sound_set.is_some()),
            );
            if let Some(set) = sound_set {
                state.set_raw_field(ES_SOUND_SET, u32::from(set));
            }
            state.set_raw_field(ES_EFLAGS, now.eflags);
            state.set_raw_field(ES_MODEL_SCALE, now.scale as u32);
            state.set_raw_field(ES_FRAME, now.frame as u32);
        }
        if let Some(Some(slot)) = self.scripts.slots.get_mut(index as usize) {
            slot.published = Some(now);
        }
    }
}
