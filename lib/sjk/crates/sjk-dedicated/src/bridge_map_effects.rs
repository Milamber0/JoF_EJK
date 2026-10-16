//! The map's effect runners and speakers on this server, and the level's effect and
//! soundset tables (`g_misc.c` `fx_runner`, `g_target.c` `target_speaker`,
//! `G_PrecacheSoundsets`). The rules are `sjk_game_jka::fx_runner`,
//! `sjk_game_jka::target_speaker` and `sjk_game_jka::spawn_table`; this is where they meet
//! the server's pool, tables, players and log.
//!
//! As the level begins every runner and speaker the game type keeps is spawned from the
//! map's lump as an entity of the pool — linked with its box, so a client's snapshot
//! carries it where the client can see it (a global speaker everywhere, a soundset
//! speaker nowhere: clients build that one themselves). Their names are registered where
//! protocol 26 reads them: effects at `CS_EFFECTS`, noises at `CS_SOUNDS`, soundsets at
//! `CS_AMBIENT_SET` in `G_PrecacheSoundsets`' order, which also gives the doors and
//! breakable brushes with a `soundSet` key their `s.soundSetIndex`. The runners think
//! every frame they are due; a name used (`G_UseTargets`) reaches them and the speakers.
//! Whatever the map places that this server does not spawn is logged once per map and
//! game type, by classname.

use super::*;
use sjk_game_jka::fx_runner::{FxRunner, MOD_UNKNOWN, Splash, Thought};
use sjk_game_jka::registries::{EffectTable, SoundSetTable};
use sjk_game_jka::spawn_table::{census, kept_by_reference, sound_set_order};
use sjk_game_jka::target_speaker::{EV_GENERAL_SOUND, Speaker, SpeakerKind, SpeakerUse};

/// `s.event`, `s.eventParm`, `s.soundSetIndex`.
const ES_EVENT: usize = 28;
const ES_EVENT_PARM: usize = 42;
const ES_SOUND_SET: usize = 78;
/// `ps.externalEvent`, `ps.externalEventParm`.
const PS_EXTERNAL_EVENT: usize = 56;
const PS_EXTERNAL_EVENT_PARM: usize = 64;
/// `EV_BMODEL_SOUND`, and `G_AddEvent`'s sequence bits.
const EV_BMODEL_SOUND: u32 = 73;
const EVENT_BITS: u32 = 0x300;
const EVENT_BIT1: u32 = 0x100;

/// The level's effect runners and speakers, and the tables their names are registered in.
#[derive(Debug, Default)]
pub(super) struct MapEffects {
    /// `G_EffectIndex`'s table (`CS_EFFECTS`), which the NPCs' precaches share.
    pub(super) effects: EffectTable,
    /// `G_BoneIndex`'s table (`CS_G2BONES`): the bones NPCs turn for the clients.
    pub(super) bones: sjk_game_jka::registries::BoneTable,
    /// `G_SoundSetIndex`'s table (`CS_AMBIENT_SET`).
    sound_sets: SoundSetTable,
    /// The runners, each with its entity and the indices of its effect and soundset.
    runners: Vec<(EntityId, FxRunner, u16, u16)>,
    /// The speakers, each with its entity and the index of its noise or soundset.
    speakers: Vec<(EntityId, Speaker, u16)>,
    /// Every standing entity with a name, and its origin: what `G_Find` finds a
    /// runner's aim among.
    places: Vec<(String, [f32; 3])>,
    /// The map and game type whose unspawned entities were last reported.
    reported: Option<(Vec<u8>, i32)>,
    /// The runner whose `target2` is being fired: `G_UseTargets2` does not use the entity
    /// that fires ("Entity used itself."), which keeps a one-shot naming itself from
    /// using itself for ever.
    firing: Option<usize>,
}

impl MapEffects {
    /// `G_SoundSetIndex` of a soundset the level registered (0 for one it did not).
    pub(super) fn sound_set(&self, name: &str) -> u16 {
        self.sound_sets.find(name.as_bytes())
    }

    /// Lets go of the runners and speakers without freeing them: the pool they were in
    /// is gone (a level spawned again into a new one).
    pub(super) fn forget_entities(&mut self) {
        self.runners.clear();
        self.speakers.clear();
    }

    /// `s.soundSetIndex` for an entity with `name` as its `soundSet` (registered by
    /// `G_PrecacheSoundsets`), 0 for none.
    pub(super) fn sound_set_index(&self, name: &str) -> u16 {
        if name.is_empty() {
            0
        } else {
            self.sound_sets.find(name.as_bytes())
        }
    }

    /// How many runners and speakers the level spawned.
    pub(super) fn spawned_counts(&self) -> (usize, usize) {
        (self.runners.len(), self.speakers.len())
    }
}

impl NativeGame {
    /// `SP_fx_runner` and `SP_target_speaker` for every one the map places in this game
    /// type, then `G_PrecacheSoundsets`; what the map places and this server does not spawn
    /// is reported. Run again (a game type changed) it spawns them afresh; the tables keep
    /// what they hold, as the configstrings do.
    pub(super) fn spawn_map_effects(&mut self) {
        let level_time = self.last_frame_time;
        for (id, ..) in std::mem::take(&mut self.map_effects.runners) {
            self.pool.free(id, level_time);
        }
        for (id, ..) in std::mem::take(&mut self.map_effects.speakers) {
            self.pool.free(id, level_time);
        }
        self.map_effects.places.clear();
        let Some(map) = self.map.take() else { return };
        self.spawn_map_effects_from(&map.entities, level_time);
        self.map = Some(map);
    }

    /// [`Self::spawn_map_effects`] from a map's entity lump, in its order.
    pub(super) fn spawn_map_effects_from(
        &mut self,
        entities: &[sjk_entity::Entity],
        level_time: i32,
    ) {
        self.report_unspawned(entities);
        let mut registered = Vec::new();
        let mut register = |index: usize, value: &[u8]| registered.push((index, value.to_vec()));
        // The soundsets first: their order is the reference's whatever registers them.
        for name in sound_set_order(entities, self.gametype) {
            self.map_effects
                .sound_sets
                .index(name.as_bytes(), &mut register);
        }
        for entity in entities.iter().skip(1) {
            if !sjk_game_jka::bot_routes::spawns_in(entity, self.gametype) {
                continue;
            }
            if kept_by_reference(entity, self.gametype)
                && let Some(name) = entity.get("targetname")
            {
                let origin = entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]);
                self.map_effects.places.push((name.to_owned(), origin));
            }
            match entity.classname().map(str::to_ascii_lowercase).as_deref() {
                Some("fx_runner") => match FxRunner::spawn(entity, level_time) {
                    Ok(runner) => {
                        let effect = self
                            .map_effects
                            .effects
                            .index(runner.effect.as_bytes(), &mut register);
                        let set = self
                            .map_effects
                            .sound_sets
                            .find(runner.sound_set.as_bytes());
                        let mut state = EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
                        runner.project(&mut state, effect, set);
                        if let Some(id) = self.pool.spawn_entity(state, level_time) {
                            self.pool.set_bounds(id, FxRunner::bounds());
                            self.map_effects.runners.push((id, runner, effect, set));
                        }
                    }
                    Err(refused) => eprintln!("fx_runner not spawned: {refused:?}"),
                },
                // `SP_CreateSnow`, `Rain`, `Wind`, `SpaceDust`: names only, in the lump's order.
                Some("fx_snow" | "fx_rain" | "fx_wind" | "fx_spacedust") => {
                    for name in sjk_game_jka::map_scenery::weather_effects(entity) {
                        self.map_effects
                            .effects
                            .index(name.as_bytes(), &mut register);
                    }
                }
                Some("target_speaker") => match Speaker::spawn(entity) {
                    Ok(speaker) => {
                        let index = match speaker.kind {
                            SpeakerKind::Noise { .. } => {
                                self.sounds.index(speaker.sound().as_bytes(), &mut register)
                            }
                            SpeakerKind::SoundSet { .. } => {
                                self.map_effects.sound_sets.find(speaker.sound().as_bytes())
                            }
                        };
                        let mut state = EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
                        speaker.project(&mut state, index);
                        if let Some(id) = self.pool.spawn_entity(state, level_time) {
                            self.pool.set_broadcast(id, speaker.broadcast());
                            self.map_effects.speakers.push((id, speaker, index));
                        }
                    }
                    Err(refused) => eprintln!(
                        "target_speaker not spawned (the reference stops the map): {refused:?}"
                    ),
                },
                _ => {}
            }
        }
        self.stamp_sound_sets(entities);
        for (index, value) in registered {
            self.publish_config_string(index, &value);
        }
    }

    /// `G_PrecacheSoundsets`' `s.soundSetIndex` on the brushes this server spawns from
    /// other modules: the doors, lifts, buttons and breakable brushes whose map entity
    /// names a soundset.
    fn stamp_sound_sets(&mut self, entities: &[sjk_entity::Entity]) {
        let set_of = |model: usize| {
            let name = format!("*{model}");
            entities
                .iter()
                .find(|entity| entity.get("model") == Some(name.as_str()))
                .and_then(|entity| entity.get("soundSet"))
        };
        let brushes = self
            .doors
            .iter()
            .map(|(id, door)| (*id, door.model))
            .chain(self.breakables.iter().map(|(id, brush)| (*id, brush.model)));
        let stamps: Vec<(EntityId, u16)> = brushes
            .filter_map(|(id, model)| {
                Some((
                    id,
                    self.map_effects.sound_sets.find(set_of(model)?.as_bytes()),
                ))
            })
            .collect();
        for (id, index) in stamps {
            if let Some(state) = self.pool.state_mut(id) {
                state.set_raw_field(ES_SOUND_SET, u32::from(index));
            }
        }
    }

    /// The map's entities this server leaves unspawned in this game type, by classname,
    /// told once per map and game type: nothing a map places is dropped unsaid.
    fn report_unspawned(&mut self, entities: &[sjk_entity::Entity]) {
        let key = (self.identity.mapname.clone(), self.gametype);
        if self.map_effects.reported.as_ref() == Some(&key) {
            return;
        }
        let mapname = String::from_utf8_lossy(&key.0).into_owned();
        for line in census(entities, self.gametype).report(&mapname) {
            eprintln!("{line}");
        }
        self.map_effects.reported = Some(key);
    }

    /// `G_RunThink` for the runners due at `level_time`: a link or a think, then what it
    /// asked for.
    pub(super) fn run_map_effects(&mut self, level_time: i32) {
        for index in 0..self.map_effects.runners.len() {
            let MapEffects {
                runners, places, ..
            } = &mut self.map_effects;
            let (_, runner, ..) = &mut runners[index];
            if !runner.due(level_time) {
                continue;
            }
            let aim = |target: &str| {
                places
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case(target))
                    .map(|(_, at)| *at)
            };
            let (thought, missing) = runner.run(level_time, &mut self.deaths.rng, aim);
            if missing {
                eprintln!(
                    "fx_runner_link: target specified but not found: {}; assuming UP orientation",
                    runner.target
                );
            }
            self.carry_out_runner(index, thought, level_time);
        }
    }

    /// `G_UseTargets` reaching the runners and speakers called `name` (`Q_stricmp`), used
    /// by player `client` (`usize::MAX` for none).
    pub(super) fn use_map_effects(&mut self, name: &str, client: usize, level_time: i32) {
        for index in 0..self.map_effects.runners.len() {
            if self.map_effects.runners[index]
                .1
                .targetname
                .eq_ignore_ascii_case(name)
            {
                self.use_one_runner(index, client, level_time);
            }
        }
        for index in 0..self.map_effects.speakers.len() {
            if self.map_effects.speakers[index]
                .1
                .targetname
                .eq_ignore_ascii_case(name)
            {
                self.use_one_speaker(index, client, level_time);
            }
        }
    }

    /// `GlobalUse` of runner `index` (`fx_runner_use`), by player `client`.
    pub(super) fn use_one_runner(&mut self, index: usize, _client: usize, level_time: i32) {
        let Some((id, runner, ..)) = self.map_effects.runners.get_mut(index) else {
            return;
        };
        if !runner.usable || self.map_effects.firing == Some(index) {
            return;
        }
        let id = *id;
        let used = runner.use_runner(level_time, &mut self.deaths.rng);
        if let Some(sound) = used.sound {
            self.raise_on(id, EV_BMODEL_SOUND, sound, level_time);
        }
        self.carry_out_runner(index, used.thought, level_time);
    }

    /// `GlobalUse` of speaker `index` (`Use_Target_Speaker`), by player `client`.
    pub(super) fn use_one_speaker(&mut self, index: usize, client: usize, level_time: i32) {
        {
            let Some((id, speaker, noise)) = self.map_effects.speakers.get_mut(index) else {
                return;
            };
            if !speaker.usable() {
                return;
            }
            let (id, noise, used) = (*id, *noise, speaker.use_speaker());
            if let Some(state) = self.pool.state_mut(id) {
                speaker.project(state, noise);
            }
            match used {
                SpeakerUse::None => {}
                SpeakerUse::OnSelf { event } => {
                    self.raise_on(id, event, u32::from(noise), level_time)
                }
                // `G_AddEvent` on a player: its external event. Nothing else a use comes
                // from here carries one a client would hear.
                SpeakerUse::OnActivator => {
                    if let Some(peer) = self.peer_mut(client) {
                        let bits = (peer.state.raw_field(PS_EXTERNAL_EVENT).unwrap_or(0)
                            & EVENT_BITS)
                            .wrapping_add(EVENT_BIT1)
                            & EVENT_BITS;
                        peer.state
                            .set_raw_field(PS_EXTERNAL_EVENT, EV_GENERAL_SOUND | bits);
                        peer.state
                            .set_raw_field(PS_EXTERNAL_EVENT_PARM, u32::from(noise));
                    }
                }
            }
        }
    }

    /// A runner's think or use carried out: its entity's wire state, its burn, and its
    /// `target2` (fired with the runner as the activator: no player).
    fn carry_out_runner(&mut self, index: usize, thought: Thought, level_time: i32) {
        let (id, runner, effect, set) = &self.map_effects.runners[index];
        let (id, target2) = (*id, runner.target2.clone());
        if let Some(state) = self.pool.state_mut(id) {
            runner.project(state, *effect, *set);
        }
        if let Some(splash) = thought.damage {
            self.burn(id.legacy_number(), splash, level_time);
        }
        let outer = self.map_effects.firing.replace(index);
        for _ in 0..thought.fired {
            self.fire_targets(&target2, usize::MAX, level_time);
        }
        self.map_effects.firing = outer;
    }

    /// `G_AddEvent` on an entity of the pool: the event with the next sequence bits, shown
    /// for `EVENT_VALID_MSEC`.
    pub(super) fn raise_on(&mut self, id: EntityId, event: u32, parameter: u32, level_time: i32) {
        let Some(state) = self.pool.state_mut(id) else {
            return;
        };
        let bits = (state.raw_field(ES_EVENT).unwrap_or(0) & EVENT_BITS).wrapping_add(EVENT_BIT1)
            & EVENT_BITS;
        state.set_raw_field(ES_EVENT, event | bits);
        state.set_raw_field(ES_EVENT_PARM, parameter);
        self.pool.raise_event(id, level_time);
    }

    /// `G_RadiusDamage(origin, self, damage, radius, self, self, MOD_UNKNOWN)` from runner
    /// `number`: everyone within the radius, hurt by how close they are, the runner the
    /// attacker (a death by it is the world's).
    fn burn(&mut self, number: u16, splash: Splash, level_time: i32) {
        let attacker = Attacker {
            npc: false,
            client: number,
            max_health: 100,
            team: 0,
            saber_knockback: [0.0; 4],
        };
        self.gather_splash_targets();
        let targets = std::mem::take(&mut self.splash_targets);
        let map = self.map.take();
        let mut hurt = |target: u16, request: DamageRequest| {
            self.strike(usize::from(number), usize::from(target), request, false)
                .1
        };
        let (damage, radius) = (splash.damage as f32, splash.radius as f32);
        match &map {
            Some(map) => {
                let world = WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                };
                radius_damage(
                    splash.origin,
                    Some(attacker),
                    damage,
                    radius,
                    Some(number),
                    MOD_UNKNOWN,
                    level_time,
                    &targets,
                    &world,
                    &mut hurt,
                )
            }
            None => radius_damage(
                splash.origin,
                Some(attacker),
                damage,
                radius,
                Some(number),
                MOD_UNKNOWN,
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
}
