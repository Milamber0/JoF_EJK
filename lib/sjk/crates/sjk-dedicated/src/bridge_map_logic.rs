//! The map's logic entities on this server (`func_timer`, `target_random`,
//! `target_counter`, `trigger_always`, `target_kill`, `target_teleporter`,
//! `target_play_music`) and what they share with every other entity a map names: the list
//! `G_Find` walks. The rules are `sjk_game_jka::map_logic`; this is where their deeds meet
//! the server's names, players and configstrings.
//!
//! A logic entity has no entity of the pool: nothing of it is ever sent to a client. Its
//! identity as an activator is [`LOGIC_NUMBERS`] and up, which no player slot reaches.

use super::*;
use sjk_game_jka::map_logic::{Deed, Logic};

/// Where the logic entities' identities start: above every possible client slot, so a
/// logic entity is never mistaken for a player (`activator->client`).
const LOGIC_NUMBERS: usize = 1 << 20;

/// Which of this server's lists a named entity is in, and where: what `target_random`'s
/// `GlobalUse` of one entity reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Held {
    Logic(usize),
    Runner(usize),
    Speaker(usize),
    Door(usize),
    /// Kept by the reference, and never used alone here.
    Other,
}

/// A kept entity with a name, in the lump's order: what `G_Find` walks (the reference
/// walks entity numbers, which follow the lump but for the slots freed at spawn and taken
/// again; this server does not reproduce that reuse).
#[derive(Clone, Debug)]
pub(super) struct Named {
    pub(super) name: String,
    pub(super) classname: String,
    /// `s.origin` and `s.angles`: where a teleporter sends to.
    pub(super) origin: [f32; 3],
    pub(super) angles: [f32; 3],
    pub(super) held: Held,
}

/// The level's logic entities and every named entity (`bridge_map_logic`).
#[derive(Debug, Default)]
pub(super) struct StockEntities {
    pub(super) logic: Vec<Logic>,
    pub(super) named: Vec<Named>,
    /// The logic entity whose targets are being used: `G_UseTargets2` never uses the entity
    /// that fires ("Entity used itself.").
    firing: Option<usize>,
    /// Classnames a `target_random` picked and this server cannot use alone, told once.
    told_unusable: Vec<String>,
    /// The trains, bobbing, swinging, rotating and static brushes (`bridge_path_movers`).
    pub(super) paths: super::bridge_path_movers::PathMovers,
    /// The sky portal, portal surfaces and glass (`bridge_map_scenery`).
    pub(super) scenery: bridge_map_scenery::Scenery,
    /// Portable shields deployed during this level.
    pub(super) shields: bridge_holdables::shields::Shields,
}

impl StockEntities {
    /// The path movers and the panes of glass as obstacles to every trace.
    pub(super) fn obstacles(&self) -> impl Iterator<Item = BoxObstacle> + '_ {
        self.paths
            .obstacles()
            .chain(self.scenery.obstacles())
            .chain(self.shields.obstacles())
    }
}

impl NativeGame {
    /// The level's logic entities and path movers, from the map's lump: run after the
    /// runners, speakers and doors are spawned (`spawn_level_entities`).
    pub(super) fn spawn_stock_entities(&mut self) {
        self.stock.paths.forget_entities();
        self.stock.shields = Default::default();
        // The lump once more, at a level's start only.
        let Some(entities) = self.map.as_ref().map(|map| map.entities.clone()) else {
            return;
        };
        let level_time = self.last_frame_time;
        self.spawn_map_logic(&entities, level_time);
        self.spawn_path_movers(&entities, level_time);
        self.spawn_scenery(&entities, level_time);
    }

    /// `G_RunFrame` for the logic entities and path movers.
    pub(super) fn run_stock_entities(&mut self, level_time: i32) {
        self.run_map_logic(level_time);
        self.run_path_movers(level_time);
        self.run_scenery(level_time);
        self.run_shields(level_time);
    }

    /// `G_UseTargets` reaching the logic entities and path movers called `name`.
    pub(super) fn use_stock_entities(&mut self, name: &str, client: usize, level_time: i32) {
        self.use_map_logic(name, client, level_time);
        self.use_path_movers(name, client, level_time);
        self.use_glass(name, client, level_time);
    }

    /// Every logic entity the map places in this game type, and the list of named
    /// entities, from the lump in order. Run after the runners, speakers and doors are
    /// spawned, whose places in their own lists the named entities record.
    pub(super) fn spawn_map_logic(&mut self, entities: &[sjk_entity::Entity], level_time: i32) {
        self.stock.logic.clear();
        self.stock.named.clear();
        let (mut runners, mut speakers) = (0, 0);
        for entity in entities.iter().skip(1) {
            if !sjk_game_jka::spawn_table::kept_by_reference(entity, self.gametype) {
                continue;
            }
            let classname = entity.classname().unwrap_or_default().to_ascii_lowercase();
            let number = LOGIC_NUMBERS + self.stock.logic.len();
            let held = match Logic::spawn(entity, number, level_time) {
                Some(Ok(logic)) => {
                    self.stock.logic.push(logic);
                    Held::Logic(self.stock.logic.len() - 1)
                }
                Some(Err(refused)) => {
                    eprintln!("{classname} not spawned (the reference stops the map): {refused:?}");
                    continue;
                }
                None => match classname.as_str() {
                    "fx_runner" => {
                        runners += 1;
                        Held::Runner(runners - 1)
                    }
                    "target_speaker" => {
                        speakers += 1;
                        Held::Speaker(speakers - 1)
                    }
                    "func_door" | "func_plat" | "func_button" => {
                        let model = entity.get("model").unwrap_or_default();
                        self.doors
                            .iter()
                            .position(|(_, door)| format!("*{}", door.model) == model)
                            .map_or(Held::Other, Held::Door)
                    }
                    _ => Held::Other,
                },
            };
            let Some(name) = entity.get("targetname").filter(|name| !name.is_empty()) else {
                continue;
            };
            let origin = entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]);
            let angles = sjk_game_jka::fx_runner::spawn_angles(entity);
            self.stock.named.push(Named {
                name: name.to_owned(),
                classname,
                origin,
                angles,
                held,
            });
        }
        // The runners and speakers this server really spawned: a refused one holds no place.
        let (spawned_runners, spawned_speakers) = self.map_effects.spawned_counts();
        if runners != spawned_runners || speakers != spawned_speakers {
            eprintln!(
                "map logic: {runners} runners and {speakers} speakers named in the lump, {spawned_runners} and {spawned_speakers} spawned"
            );
        }
    }

    /// `G_RunThink` for the logic entities due at `level_time`.
    pub(super) fn run_map_logic(&mut self, level_time: i32) {
        for index in 0..self.stock.logic.len() {
            if !self.stock.logic[index].due(level_time) {
                continue;
            }
            let deeds = self.stock.logic[index].think();
            self.carry_out_logic(index, deeds, level_time);
        }
    }

    /// `G_UseTargets` reaching the logic entities called `name`, set off by `client`
    /// (`usize::MAX` for no client).
    pub(super) fn use_map_logic(&mut self, name: &str, client: usize, level_time: i32) {
        let activator = (client != usize::MAX).then_some(client);
        for index in 0..self.stock.logic.len() {
            if self.stock.firing == Some(index)
                || !self.stock.logic[index]
                    .targetname
                    .eq_ignore_ascii_case(name)
            {
                continue;
            }
            self.use_logic_entity(index, activator, level_time);
        }
    }

    /// `target_activate` and `target_deactivate` on the logic entities called `name`.
    pub(super) fn set_map_logic_active(&mut self, name: &str, active: bool) {
        for logic in self
            .stock
            .logic
            .iter_mut()
            .filter(|logic| logic.targetname.eq_ignore_ascii_case(name))
        {
            logic.inactive = !active;
        }
    }

    /// `GlobalUse` of logic entity `index` for `activator`.
    fn use_logic_entity(&mut self, index: usize, activator: Option<usize>, level_time: i32) {
        let logic = &self.stock.logic[index];
        let named = if logic.target.is_empty() {
            0
        } else {
            self.stock
                .named
                .iter()
                .filter(|named| {
                    named.name.eq_ignore_ascii_case(&logic.target)
                        && named.held != Held::Logic(index)
                })
                .count()
        };
        let deeds = self.stock.logic[index].use_logic(activator, named, &mut self.deaths.rng);
        self.carry_out_logic(index, deeds, level_time);
    }

    /// What logic entity `index`'s think or use asked for, in order.
    fn carry_out_logic(&mut self, index: usize, deeds: Vec<Deed>, level_time: i32) {
        for deed in deeds {
            match deed {
                Deed::Fire { name, activator } => {
                    let outer = self.stock.firing.replace(index);
                    self.fire_targets(&name, self.client_of(activator), level_time);
                    self.stock.firing = outer;
                }
                Deed::FireOne {
                    name,
                    pick,
                    activator,
                } => self.use_one_named(index, &name, pick, activator, level_time),
                Deed::Kill { activator } => {
                    let client = self.client_of(Some(activator));
                    if self.peer(client).is_some() {
                        let request = DamageRequest {
                            level_time,
                            attacker: None,
                            direction: None,
                            point: None,
                            damage: 100_000,
                            flags: DAMAGE_NO_PROTECTION,
                            means: MOD_TELEFRAG,
                        };
                        let _ = self.hurt(client, request);
                    }
                }
                Deed::Teleport {
                    activator,
                    destination,
                } => {
                    let client = self.client_of(Some(activator));
                    let Some(spectating) = self.peer(client).map(|peer| !peer.playing()) else {
                        continue;
                    };
                    // `G_PickTarget`: `rand() % num_choices` among everything so named.
                    let choices: Vec<([f32; 3], [f32; 3])> = self
                        .stock
                        .named
                        .iter()
                        .filter(|named| named.name.eq_ignore_ascii_case(&destination))
                        .map(|named| (named.origin, named.angles))
                        .collect();
                    if choices.is_empty() {
                        println!("Couldn't find teleporter destination");
                        continue;
                    }
                    let (destination, angles) = choices[self.rand.next() as usize % choices.len()];
                    let Some(from) = self.peer(client).map(|peer| peer.state.origin()) else {
                        continue;
                    };
                    self.teleport_client(client, from, destination, angles, spectating, level_time);
                }
                Deed::Music(music) => {
                    self.publish_config_string(sjk_game_jka::worldspawn::CS_MUSIC, music.as_bytes())
                }
                Deed::Rearm => self.stock.logic[index].rearm(level_time, &mut self.deaths.rng),
            }
        }
    }

    /// `target_random`'s `GlobalUse` of the `pick`-th entity called `name` (not counting
    /// the firing logic entity `index`), in `G_Find`'s order.
    fn use_one_named(
        &mut self,
        index: usize,
        name: &str,
        pick: usize,
        activator: Option<usize>,
        level_time: i32,
    ) {
        let chosen = self
            .stock
            .named
            .iter()
            .filter(|named| {
                named.name.eq_ignore_ascii_case(name) && named.held != Held::Logic(index)
            })
            .nth(pick.wrapping_sub(1))
            .cloned();
        let Some(chosen) = chosen else { return };
        let client = self.client_of(activator);
        match chosen.held {
            Held::Logic(other) => self.use_logic_entity(other, activator, level_time),
            Held::Runner(runner) => self.use_one_runner(runner, client, level_time),
            Held::Speaker(speaker) => self.use_one_speaker(speaker, client, level_time),
            Held::Door(door) => {
                let fired = sjk_game_jka::mover_team::use_mover(
                    &mut self.doors,
                    door,
                    level_time,
                    (client != usize::MAX).then_some(client),
                );
                self.publish_team(door);
                if let Some(target) = fired {
                    self.fire_targets(&target, client, level_time);
                }
            }
            Held::Other => {
                if !self.stock.told_unusable.contains(&chosen.classname) {
                    println!(
                        "target_random picked a {} ({name}), which this server does not use alone yet",
                        chosen.classname
                    );
                    self.stock.told_unusable.push(chosen.classname);
                }
            }
        }
    }

    /// A logic activator as the rest of the server takes it: a client slot, or
    /// `usize::MAX` for no client.
    fn client_of(&self, activator: Option<usize>) -> usize {
        activator
            .filter(|&who| who < self.players.places())
            .unwrap_or(usize::MAX)
    }
}

#[path = "bridge_map_scenery.rs"]
mod bridge_map_scenery;
