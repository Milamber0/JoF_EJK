//! The movers that are not doors on this server (`func_train` and its `path_corner`s,
//! `func_bobbing`, `func_pendulum`, `func_rotating`, `func_static`): spawned from the lump
//! as entities every client draws, run every frame (`G_RunMover`), pushing, carrying,
//! turning and crushing the players in their way (`G_MoverPush`). The rules are
//! `sjk_game_jka::path_movers`; this is where they meet the pool, the map and the players.

use super::*;
use sjk_game_jka::path_movers::{Corner, Kind, PathMover, Pushee, link_corners, push};

/// `EV_PLAYDOORSOUND`, which a train's soundset sounds are sent as.
const EV_PLAYDOORSOUND: u32 = 71;
/// `ps.delta_angles[YAW]`: a player turned by a rotating mover is turned in its view too.
const PS_DELTA_YAW: usize = 11;

/// One mover of the level: its entity, its rules, the indices of its model2 and soundset,
/// and the first corner of a train's path.
#[derive(Debug)]
pub(super) struct Placed {
    id: EntityId,
    mover: PathMover,
    model2: u16,
    sound_set: u16,
    first_corner: Option<usize>,
}

/// The level's path movers and path corners.
#[derive(Debug, Default)]
pub(super) struct PathMovers {
    movers: Vec<Placed>,
    corners: Vec<Corner>,
    /// Whether a use of one of them (the reference's `Use_BinaryMover` on a mover that is not
    /// binary) was already told.
    told_use: bool,
}

impl PathMovers {
    /// The movers as obstacles to every trace, where their last frame left them. A turning
    /// mover is traced unturned (the obstacle carries no angles).
    pub(super) fn obstacles(&self) -> impl Iterator<Item = BoxObstacle> + '_ {
        self.movers.iter().map(|placed| BoxObstacle {
            entity: placed.id.legacy_number(),
            origin: placed.mover.origin,
            bounds: placed.mover.bounds,
            contents: sjk_game_jka::movers::CONTENTS_SOLID,
            model: Some(placed.mover.model),
        })
    }

    /// Lets go of the movers without freeing them: the pool they were in is gone.
    pub(super) fn forget_entities(&mut self) {
        self.movers.clear();
    }
}

impl NativeGame {
    /// Every path mover and corner the map places in this game type, in lump order.
    /// `named` (the list `G_Find` walks) gives each train its first corner.
    pub(super) fn spawn_path_movers(&mut self, entities: &[sjk_entity::Entity], level_time: i32) {
        let gravity = self.gravity();
        let Some(map) = self.map.take() else { return };
        let mut corners = Vec::new();
        let mut placed = Vec::new();
        for entity in entities.iter().skip(1) {
            if !sjk_game_jka::spawn_table::kept_by_reference(entity, self.gametype) {
                continue;
            }
            if entity
                .classname()
                .is_some_and(|name| name.eq_ignore_ascii_case("path_corner"))
            {
                match Corner::spawn(entity) {
                    Some(corner) => corners.push(corner),
                    None => println!(
                        "path_corner with no targetname at {:?}",
                        sjk_game_jka::fx_runner::vector(entity, "origin")
                    ),
                }
                continue;
            }
            // A `func_static` that runs scripts is the scripts' (`bridge_icarus`).
            if entity
                .classname()
                .is_some_and(|name| name.eq_ignore_ascii_case("func_static"))
                && sjk_game_jka::script_entity::runs_scripts(entity)
            {
                continue;
            }
            let model = entity
                .get("model")
                .and_then(|model| model.strip_prefix('*'))
                .and_then(|number| number.parse().ok());
            let bounds = model
                .and_then(|model| crate::map::brush_bounds(&map.bsp, model))
                .unwrap_or(([0.0; 3], [0.0; 3]));
            match PathMover::spawn(entity, bounds, gravity, level_time) {
                None => {}
                Some(Err(refused)) => println!(
                    "{} not spawned: {refused:?}",
                    entity.classname().unwrap_or_default()
                ),
                Some(Ok(mover)) => placed.push(mover),
            }
        }
        self.map = Some(map);
        for mover in placed {
            let (models, told) = (&mut self.models, &mut self.told);
            let model2 = if mover.model2.is_empty() {
                0
            } else {
                models.index(mover.model2.as_bytes(), &mut |index, value| {
                    told.push(Told::ConfigString {
                        index,
                        previous: Vec::new(),
                        value: value.to_vec(),
                    })
                })
            };
            let sound_set = self.map_effects.sound_set_index(&mover.sound_set);
            let first_corner = self.first_corner(&mover.target, &corners);
            let mut state = EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
            mover.project(&mut state, model2, sound_set);
            if let Some(id) = self.pool.spawn_entity(state, level_time) {
                self.pool.set_bounds(id, mover.bounds);
                self.pool.set_broadcast(id, mover.broadcast);
                self.stock.paths.movers.push(Placed {
                    id,
                    mover,
                    model2,
                    sound_set,
                    first_corner,
                });
            }
        }
        self.stock.paths.corners = corners;
    }

    /// `ent->nextTrain = G_Find(NULL, targetname, ent->target)`: the first standing entity
    /// called `target`, which is a corner's place in `corners` when it is a `path_corner`.
    fn first_corner(&self, target: &str, corners: &[Corner]) -> Option<usize> {
        let first = self
            .stock
            .named
            .iter()
            .find(|named| named.name.eq_ignore_ascii_case(target))?;
        if first.classname != "path_corner" {
            return None;
        }
        corners
            .iter()
            .position(|corner| corner.targetname.eq_ignore_ascii_case(target))
    }

    /// `G_RunMover` for every path mover at `level_time`, then its think.
    pub(super) fn run_path_movers(&mut self, level_time: i32) {
        let previous = self.previous_frame_time;
        for index in 0..self.stock.paths.movers.len() {
            let mover = &self.stock.paths.movers[index].mover;
            if mover.travels() {
                let (shift, turn) = mover.travel(level_time);
                let pushed = shift == [0.0; 3] && turn == [0.0; 3]
                    || self.push_path_mover(index, shift, turn, level_time);
                let placed = &mut self.stock.paths.movers[index];
                if placed
                    .mover
                    .moved(shift, turn, pushed, level_time, previous)
                    && placed.mover.kind == Kind::Train
                {
                    let fired = placed
                        .mover
                        .reached_train(&self.stock.paths.corners, level_time);
                    self.publish_path_mover(index);
                    if let Some(target) = fired {
                        self.fire_targets(&target, usize::MAX, level_time);
                    }
                }
            }
            if self.stock.paths.movers[index].mover.due(level_time) {
                let placed = &mut self.stock.paths.movers[index];
                if let Some(first) = placed.first_corner {
                    link_corners(&mut self.stock.paths.corners, first);
                }
                let fired =
                    placed
                        .mover
                        .think(&self.stock.paths.corners, placed.first_corner, level_time);
                if let Some(target) = fired {
                    self.fire_targets(&target, usize::MAX, level_time);
                }
            }
            self.publish_path_mover(index);
        }
    }

    /// `G_MoverPush` for mover `index`: the players it carries, shoves or turns moved; those
    /// it crushes hurt (`MOD_CRUSH`, the mover the attacker). Returns whether it moved.
    fn push_path_mover(
        &mut self,
        index: usize,
        shift: [f32; 3],
        turn: [f32; 3],
        level_time: i32,
    ) -> bool {
        let number = self.stock.paths.movers[index].id.legacy_number();
        let mover = self.stock.paths.movers[index].mover.clone();
        let candidates: Vec<Pushee> = (0..self.players.places())
            .filter_map(|client| {
                let peer = self.peer(client)?;
                if !peer.playing() {
                    return None;
                }
                let (bottom, mut top) = peer.movement.box_bounds();
                if let Some((_, corpse_top)) = peer.corpse {
                    top[2] = corpse_top;
                }
                Some(Pushee {
                    number: client as u16,
                    origin: peer.state.origin(),
                    bounds: (bottom, top),
                    ground: peer.state.ground_entity_num(),
                    dead: peer.health < 1,
                })
            })
            .collect();
        if candidates.is_empty() {
            return true;
        }
        let at: [f32; 3] = std::array::from_fn(|axis| mover.origin[axis] + shift[axis]);
        let outcome = match self.map.as_ref() {
            Some(map) => {
                let world = WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                };
                let inside_mover = |origin: [f32; 3], bounds: ([f32; 3], [f32; 3])| {
                    sjk_bsp::Aabb::new(bounds.0, bounds.1).is_ok_and(|bounds| {
                        map.bsp
                            .trace_transformed_model(
                                mover.model,
                                at,
                                None,
                                origin,
                                origin,
                                bounds,
                                u32::MAX,
                            )
                            .start_solid
                    })
                };
                let mut inside = |check: &Pushee| inside_mover(check.origin, check.bounds);
                // `G_TestEntityPosition`: the world, and this mover where it now stands.
                let mut free = |_: u16, origin: [f32; 3], bounds: ([f32; 3], [f32; 3])| {
                    !world
                        .trace(origin, bounds.0, bounds.1, origin, 0x1)
                        .start_solid
                        && !inside_mover(origin, bounds)
                };
                push(
                    &mover,
                    number,
                    shift,
                    turn,
                    &candidates,
                    &mut inside,
                    &mut free,
                )
            }
            None => push(
                &mover,
                number,
                shift,
                turn,
                &candidates,
                &mut |_| false,
                &mut |_, _, _| true,
            ),
        };
        for pushed in &outcome.pushed {
            let Some(peer) = self.peer_mut(usize::from(pushed.number)) else {
                continue;
            };
            peer.state.set_origin(pushed.to);
            let yaw = peer.state.raw_field(PS_DELTA_YAW).unwrap_or(0) as i32;
            // (`s.groundEntityNum` of one pushed off is cleared; the player state's is its
            // next move's to change.)
            peer.state
                .set_raw_field(PS_DELTA_YAW, yaw.wrapping_add(pushed.yaw) as u32);
            peer.movement = peer.movement.reseeded(&peer.state);
        }
        for (victim, damage, flags) in outcome.crushed {
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
                point: None,
                damage,
                flags,
                means: sjk_game_jka::movers::MOD_CRUSH,
            };
            let _ = self.hurt(usize::from(victim), request);
        }
        outcome.blocked.is_none()
    }

    /// The mover's wire state and the sounds it raised since.
    fn publish_path_mover(&mut self, index: usize) {
        let level_time = self.last_frame_time;
        let placed = &mut self.stock.paths.movers[index];
        let sounds = std::mem::take(&mut placed.mover.sounds);
        let id = placed.id;
        if let Some(state) = self.pool.state_mut(id) {
            placed.mover.project(state, placed.model2, placed.sound_set);
        }
        for sound in sounds {
            self.raise_on(id, EV_PLAYDOORSOUND, sound, level_time);
        }
    }

    /// `G_UseTargets` reaching the path movers called `name`: a `func_static` switches its
    /// shader frame and fires its own targets (`func_static_use`); any other has the
    /// reference's `Use_BinaryMover`, which is not ported for them and is said so once.
    pub(super) fn use_path_movers(&mut self, name: &str, client: usize, level_time: i32) {
        for index in 0..self.stock.paths.movers.len() {
            let placed = &mut self.stock.paths.movers[index];
            if placed.mover.inactive || !placed.mover.targetname.eq_ignore_ascii_case(name) {
                continue;
            }
            if placed.mover.kind != Kind::Static {
                if !std::mem::replace(&mut self.stock.paths.told_use, true) {
                    println!(
                        "{name}: a used {:?} mover (Use_BinaryMover on a mover that is not binary) is not ported",
                        placed.mover.kind
                    );
                }
                continue;
            }
            placed.mover.use_static();
            let target = placed.mover.target.clone();
            self.publish_path_mover(index);
            self.fire_targets(&target, client, level_time);
        }
    }
}
