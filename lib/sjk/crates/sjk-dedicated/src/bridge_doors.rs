//! The map's doors, lifts and buttons on this server (`g_mover.c`): spawned as the
//! entities every client draws, chained into teams (`G_FindTeams`), opened by their own
//! triggers, by name (`G_UseTargets`) or by the use key, run every frame, and pushing
//! whoever is in their way. The rules themselves are `sjk_game_jka::movers` and
//! `sjk_game_jka::mover_team`.

use super::*;
use sjk_game_jka::mover_team;
use sjk_game_jka::movers::{self, MoverKind, MoverState};

/// `s.loopSound`, `s.loopIsSoundset`.
const ES_LOOP_SOUND: usize = 55;
const ES_LOOP_IS_SOUNDSET: usize = 70;

impl NativeGame {
    /// `SP_func_door` for every door the map placed: an `ET_MOVER` every client is sent,
    /// standing where the map put it until somebody comes near. Then `G_FindTeams`.
    pub(super) fn spawn_doors(&mut self) {
        let Some(mut map) = self.map.take() else {
            return;
        };
        // A map begins with every portal closed.
        map.areas.close_all();
        for door in &map.doors {
            let mut state = EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
            state.set_raw_field(ES_ENTITY_TYPE, movers::ET_MOVER);
            sjk_game_jka::triggers::set_brush_model(&mut state, door.model);
            for axis in 0..3 {
                state.set_raw_field(ES_POS_BASE[axis], door.base[axis].to_bits());
                state.set_raw_field(ES_POS_DELTA[axis], door.delta[axis].to_bits());
            }
            state.set_raw_field(ES_POS_DURATION, door.duration as u32);
            state.set_raw_field(ES_TIME, door.time as u32);
            if let Some(number) = self.pool.spawn_entity(state, 0) {
                self.pool.set_bounds(number, door.bounds);
                self.doors.push((number, door.clone()));
            }
        }
        mover_team::find_teams(&mut self.doors);
        self.door_portals = vec![false; self.doors.len()];
        self.map = Some(map);
    }

    /// `AdjustAreaPortalState` for a door team: its portal is open while its master is
    /// away from rest (`Use_BinaryMover_Go` opens it leaving `pos1`, `Reached_BinaryMover`
    /// closes it back there), between the areas the master stands in at `pos1`.
    fn sync_portal(&mut self, master: usize) {
        let door = &self.doors[master].1;
        let open = door.state != MoverState::Pos1;
        if self
            .door_portals
            .get(master)
            .is_none_or(|&held| held == open)
        {
            return;
        }
        let (at, bounds) = (door.pos1, door.bounds);
        self.door_portals[master] = open;
        self.adjust_portal(at, bounds, open);
    }

    /// `SV_AdjustAreaPortalState`: one more (or one fewer) open portal between the two
    /// areas a brush at `at` stands in (`r.areanum`, `r.areanum2` of its link).
    pub(super) fn adjust_portal(&mut self, at: [f32; 3], bounds: ([f32; 3], [f32; 3]), open: bool) {
        let Some(map) = self.map.as_mut() else { return };
        let low = std::array::from_fn(|axis| at[axis] + bounds.0[axis] - 1.0);
        let high = std::array::from_fn(|axis| at[axis] + bounds.1[axis] + 1.0);
        let (first, second) = crate::visibility::ClusterLink::new(&map.bsp, low, high).areas();
        map.areas.adjust(first, second, open);
    }

    /// `Touch_DoorTrigger` and `Touch_PlatCenterTrigger` for a player in a mover's own
    /// trigger: a door team's box around all its parts, a plat's thin box in its low
    /// position. Only the movers the reference gives a trigger have one. A spectator
    /// touches door triggers alone, and passes through a door that is shut.
    pub(super) fn touch_doors(&mut self, client: usize, level_time: i32) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let spectating = !peer.body_active();
        if !sjk_game_jka::triggers::may_touch_as_spectator(peer.health) {
            return;
        }
        for index in 0..self.doors.len() {
            let door = &self.doors[index].1;
            if !movers::spawns_own_trigger(door) || (spectating && door.kind == MoverKind::Plat) {
                continue;
            }
            let (low, high, axis) = match door.kind {
                MoverKind::Plat => {
                    let (low, high) = movers::plat_trigger_bounds(door);
                    (low, high, 0)
                }
                MoverKind::Door | MoverKind::Button => {
                    mover_team::team_trigger_bounds(&self.doors, index)
                }
            };
            let Some(peer) = self.peer_mut(client) else {
                return;
            };
            let (origin, bounds) = (peer.state.origin(), peer.movement.box_bounds());
            let contact = (0..3).all(|axis| {
                origin[axis] + bounds.0[axis] < high[axis]
                    && origin[axis] + bounds.1[axis] > low[axis]
            });
            if !contact {
                continue;
            }
            if spectating {
                if !matches!(
                    self.doors[index].1.state,
                    MoverState::OneToTwo | MoverState::Pos2
                ) && let Some(to) = mover_team::spectator_passage(low, high, axis, origin)
                {
                    self.pass_spectator(client, to);
                }
                continue;
            }
            let fired = match self.doors[index].1.kind {
                // `Touch_PlatCenterTrigger`: a plat at the bottom is used.
                MoverKind::Plat => (self.doors[index].1.state == MoverState::Pos1)
                    .then(|| {
                        mover_team::use_mover(&mut self.doors, index, level_time, Some(client))
                    })
                    .flatten(),
                MoverKind::Door | MoverKind::Button => {
                    mover_team::touch_trigger(&mut self.doors, index, level_time, Some(client))
                }
            };
            self.publish_team(index);
            if let Some(target) = fired {
                self.fire_targets(&target, client, level_time);
            }
        }
    }

    /// The end of `Touch_DoorTriggerSpectator`: if a player's box fits where the pass
    /// puts the spectator (`MASK_PLAYERSOLID`, nobody in the way), it is moved there with
    /// its angles and speed kept (`TeleportPlayer` with `doorangles`).
    fn pass_spectator(&mut self, client: usize, to: [f32; 3]) {
        const MASK_PLAYERSOLID: u32 = 0x1 | 0x100 | 0x10000;
        self.gather_obstacles(client);
        let fits = match self.map.as_ref() {
            Some(map) => {
                let world = WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                };
                let trace = crate::collision::WithPlayers {
                    world,
                    players: &self.obstacles,
                }
                .trace(
                    to,
                    [-15.0, -15.0, -24.0],
                    [15.0, 15.0, 40.0],
                    to,
                    MASK_PLAYERSOLID,
                );
                !trace.start_solid && !trace.all_solid && trace.fraction == 1.0
            }
            None => true,
        };
        if !fits {
            return;
        }
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let _ = sjk_game_jka::triggers::teleport_player(
            &mut peer.state,
            to,
            [10_000_000.0, 0.0, 0.0],
            true,
        );
        peer.movement = peer.movement.reseeded(&peer.state);
    }

    /// `G_UseTargets` reaching the movers called `name` (`Use_BinaryMover`): a slave
    /// hands the use to its master, a locked team unlocks, and the rest open, hold, close
    /// or turn back.
    pub(super) fn use_doors(&mut self, name: &str, client: usize, level_time: i32) {
        for index in 0..self.doors.len() {
            if self.doors[index].1.targetname != name {
                continue;
            }
            let activator = (client != usize::MAX).then_some(client);
            let fired = mover_team::use_mover(&mut self.doors, index, level_time, activator);
            self.publish_team(index);
            if let Some(target) = fired {
                self.fire_targets(&target, client, level_time);
            }
        }
    }

    /// `ClientImpacts` (`g_active.c:493-521`) for the movers this move touched: a
    /// `func_plat` a living player stood on or walked into keeps itself up for another
    /// second (`Touch_Plat`), which is what holds a lift at the top while somebody rides
    /// it.
    pub(super) fn touch_plats(&mut self, client: usize, level_time: i32) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let (health, touched) = (peer.health, peer.movement.touched());
        for index in 0..self.doors.len() {
            if !touched
                .entities()
                .contains(&self.doors[index].0.legacy_number())
                || self.doors[index].1.kind != MoverKind::Plat
            {
                continue;
            }
            let before = self.doors[index].1.next_think;
            movers::touch_plat(&mut self.doors[index].1, health, level_time);
            if self.doors[index].1.next_think != before {
                self.publish_door(index);
            }
        }
    }

    /// `G_RunMover` for every mover: what it has travelled since the last frame is
    /// pushed through whoever is in the way (`G_MoverPush`), then a team's master lets
    /// the parts that are there arrive and runs its think. What a part fires on arriving,
    /// or a delayed use on coming due, is fired for the client that set it going
    /// (`usize::MAX`, nobody, when the world did).
    pub(super) fn run_doors(&mut self, level_time: i32) {
        let previous = self.previous_frame_time;
        for index in 0..self.doors.len() {
            let (from, to) = {
                let (_, door) = &self.doors[index];
                (
                    movers::origin_at(door, previous),
                    movers::origin_at(door, level_time),
                )
            };
            if from != to {
                self.push_door(index, from, to, level_time);
            }
            let ran = mover_team::run(&mut self.doors, index, level_time);
            if ran.changed {
                self.publish_team(index);
            }
            for (target, activator) in ran.fired {
                self.fire_targets(&target, activator.unwrap_or(usize::MAX), level_time);
            }
        }
    }

    /// `G_MoverPush` with `G_TryPushingEntity`: the players a mover takes with it, and
    /// `Blocked_Door` on its team's master for one it cannot move — crushed for the
    /// master's own damage, and the team turned back.
    fn push_door(&mut self, index: usize, from: [f32; 3], to: [f32; 3], level_time: i32) {
        let (number, door) = (
            self.doors[index].0.legacy_number(),
            self.doors[index].1.clone(),
        );
        let candidates: Vec<(u16, [f32; 3], ([f32; 3], [f32; 3]), u16)> =
            (0..self.players.places())
                .filter_map(|client| {
                    let peer = self.peer_mut(client)?;
                    (peer.body_active() && peer.health > 0).then(|| {
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
            return;
        }
        let pushed = match self.map.as_ref() {
            Some(map) => {
                let world = WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                };
                // `G_TestEntityPosition`: may the box stand there at all?
                let mut free = |_client: u16, origin: [f32; 3], bounds: ([f32; 3], [f32; 3])| {
                    !world
                        .trace(origin, bounds.0, bounds.1, origin, 0x1)
                        .start_solid
                };
                // `G_TestEntityPosition` against the mover itself: is the box, where it
                // stands, inside the mover's brush at its new place?
                let mut inside = |_client: u16, origin: [f32; 3], bounds: ([f32; 3], [f32; 3])| {
                    sjk_bsp::Aabb::new(bounds.0, bounds.1).is_ok_and(|bounds| {
                        map.bsp
                            .trace_transformed_model(
                                door.model,
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
                movers::push_through(&door, from, to, &candidates, &mut free, &mut inside, number)
            }
            // A world with no map in it has nothing to be blocked by.
            None => movers::push(&door, from, to, &candidates, &mut |_, _, _| true, number),
        };
        match pushed {
            movers::Push::Moved(shoved) => {
                for movers::Shoved { client, to, .. } in shoved {
                    let Some(peer) = self.peer_mut(usize::from(client)) else {
                        continue;
                    };
                    peer.state.set_origin(to);
                    peer.movement = peer.movement.reseeded(&peer.state);
                }
            }
            movers::Push::Blocked(client) => {
                let master = door.team_master.unwrap_or(index);
                let damage = mover_team::blocked(
                    &mut self.doors,
                    master,
                    level_time,
                    self.previous_frame_time,
                    Some(usize::from(client)),
                );
                self.publish_team(master);
                if let Some(damage) = damage {
                    let attacker = Attacker {
                        npc: false,
                        client: self.doors[master].0.legacy_number(),
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
                        flags: 0,
                        means: movers::MOD_CRUSH,
                    };
                    let _ = self.hurt(usize::from(client), request);
                }
            }
        }
    }

    /// Every part of the team the mover at `index` belongs to, published, and its portal
    /// kept in step.
    pub(super) fn publish_team(&mut self, index: usize) {
        let master = self.doors[index].1.team_master.unwrap_or(index);
        self.sync_portal(master);
        for at in 0..self.doors[master].1.team_parts.len().max(1) {
            let part = self.doors[master]
                .1
                .team_parts
                .get(at)
                .copied()
                .unwrap_or(master);
            self.publish_door(part);
        }
    }

    /// The mover's wire state, which every client draws it by and hears it by: its
    /// trajectory, its travel loop (`G_PlayDoorLoopSound`), and the start and stop sounds
    /// it raised since (`G_PlayDoorSound`'s `EV_PLAYDOORSOUND`, from its soundset).
    pub(super) fn publish_door(&mut self, index: usize) {
        const EV_PLAYDOORSOUND: u32 = 71;
        let sounds = std::mem::take(&mut self.doors[index].1.sounds);
        let (number, door) = &self.doors[index];
        let (number, door) = (*number, door.clone());
        for sound in sounds {
            self.raise_on(number, EV_PLAYDOORSOUND, sound, self.last_frame_time);
        }
        let Some(slot) = self.pool.state_mut(number) else {
            return;
        };
        slot.set_raw_field(
            ES_LOOP_SOUND,
            if door.looping {
                sjk_game_jka::fx_runner::BMS_MID
            } else {
                0
            },
        );
        slot.set_raw_field(ES_LOOP_IS_SOUNDSET, u32::from(door.looping));
        slot.set_raw_field(ES_POS_TYPE, door.trajectory);
        slot.set_raw_field(ES_POS_TIME, door.started as u32);
        for axis in 0..3 {
            slot.set_raw_field(ES_POS_BASE[axis], door.base[axis].to_bits());
            slot.set_raw_field(ES_POS_DELTA[axis], door.delta[axis].to_bits());
        }
        slot.set_raw_field(ES_POS_DURATION, door.duration as u32);
        slot.set_raw_field(ES_TIME, door.time as u32);
    }
}
