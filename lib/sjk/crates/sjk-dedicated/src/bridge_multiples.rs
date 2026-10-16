//! The map's `trigger_multiple` brushes and the targets they fire (`g_trigger.c`
//! `Touch_Multi`, `multi_trigger`, `trigger_cleared_fire`; `g_utils.c` `G_UseTargets`):
//! touched by players, fired by name, and run when a delay or a clearing is due. The
//! rules are `sjk_game_jka::triggers`; a trigger a script moves or switches off is
//! read through `bridge_icarus`, and a name reaches the scripted entities there too.

use super::*;

impl NativeGame {
    /// The `trigger_multiple` brushes and the targets the map placed, as
    /// `SP_trigger_multiple` and `g_target.c`'s spawns leave them.
    pub(super) fn spawn_multiples(&mut self) {
        let Some(map) = self.map.take() else { return };
        self.multiples = map.multiples.clone();
        self.targets = map
            .targets
            .iter()
            .map(|(name, target)| (name.clone(), target.clone(), MapTarget::default()))
            .collect();
        self.map = Some(map);
    }

    /// `G_TouchTriggers` for the `trigger_multiple` brushes: the gates, the pose a used
    /// one puts the player in, and the targets it fires.
    pub(super) fn touch_multiples(&mut self, client: usize, level_time: i32) {
        let spectating = self.peer_mut(client).is_none_or(|peer| !peer.playing());
        if self
            .peer_mut(client)
            .is_none_or(|peer| !sjk_game_jka::triggers::may_touch(peer.health, spectating))
        {
            return;
        }
        for index in 0..self.multiples.len() {
            let trigger = self.multiples[index].clone();
            // `Touch_Multi`'s `FL_INACTIVE` as a script set it, and its team gate.
            if self.scripts.multiple_switched_off(index) {
                continue;
            }
            // Where a script moved the brush to (`r.currentOrigin`): its model is traced there.
            let moved = self.scripts.multiple_offset(index);
            let Some(peer) = self.peer_mut(client) else {
                return;
            };
            let (origin, bounds) = (peer.state.origin(), peer.movement.box_bounds());
            let who = sjk_game_jka::triggers::Toucher {
                view_angles: peer.state.view_angles(),
                buttons: peer.last_command.buttons,
                health: peer.health,
                spectating,
                weapon_time: peer.state.raw_field(10).unwrap_or(0) as i32,
                hand_extend: peer.state.raw_field(80).unwrap_or(0) as u8,
                torso_anim: peer.state.raw_field(15).unwrap_or(0) as u16,
                team: peer.session.team,
            };
            let contact = {
                let Some(map) = self.map.as_ref() else { return };
                let low: [f32; 3] = std::array::from_fn(|axis| origin[axis] + bounds.0[axis]);
                let high: [f32; 3] = std::array::from_fn(|axis| origin[axis] + bounds.1[axis]);
                let middle: [f32; 3] =
                    std::array::from_fn(|axis| (low[axis] + high[axis]) / 2.0 - moved[axis]);
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
            let mut hooks = self.siege_touch_hooks(client, index, level_time);
            let touched = sjk_game_jka::triggers::touch_multiple_as_with(
                &mut self.multiples[index],
                &who,
                sjk_game_jka::triggers::Activator::Player,
                [1.0, 0.0, 0.0],
                level_time,
                &mut hooks,
            );
            let touched = if touched == sjk_game_jka::triggers::Touched2::Uses {
                let Some(peer) = self.peer_mut(client) else {
                    return;
                };
                match sjk_game_jka::triggers::using_pose(who.torso_anim) {
                    Some(pose) => peer.movement.set_animation_parts(
                        sjk_game_jka::pmove_anim::SETANIM_TORSO,
                        pose,
                        sjk_game_jka::pmove_anim::SETANIM_FLAG_OVERRIDE
                            | sjk_game_jka::pmove_anim::SETANIM_FLAG_HOLD,
                    ),
                    None => peer
                        .movement
                        .set_torso_timer(sjk_game_jka::triggers::USING_AGAIN),
                }
                let timer = peer.movement.state().torso_timer;
                peer.movement.set_weapon_time(timer);
                peer.movement.write_player_state(&mut peer.state);
                sjk_game_jka::triggers::pressed_as_with(
                    &mut self.multiples[index],
                    sjk_game_jka::triggers::Activator::Player,
                    level_time,
                    &mut hooks,
                )
            } else {
                touched
            };
            self.siege_touch_done(client, hooks, level_time);
            if touched == sjk_game_jka::triggers::Touched2::Fires {
                self.fire_trigger(index, client, level_time);
            }
        }
    }

    /// `multi_trigger_run`: the trigger's targets and its own noise.
    pub(super) fn fire_trigger(&mut self, index: usize, client: usize, level_time: i32) {
        // `Q_flrand(-1, 1)` only where `multi_trigger_run` draws it.
        let spread = if sjk_game_jka::triggers::draws_spread(&self.multiples[index], level_time) {
            self.deaths.rng.flrand(-1.0, 1.0)
        } else {
            0.0
        };
        self.multiples[index].activator = Some(client);
        let fired = sjk_game_jka::triggers::fire_multiple(
            &mut self.multiples[index],
            level_time,
            spread,
            true,
        );
        // A siege zone's taker: its side's target first (`G_UseTargets2(ent, activator, …)`).
        if let Some(taken) = &fired.taken {
            self.fire_targets(taken, client, level_time);
        }
        if let Some(sound) = fired.sound {
            let origin = self
                .peer_mut(client)
                .map(|peer| peer.state.origin())
                .or_else(|| {
                    self.npcs
                        .roster
                        .actors
                        .iter()
                        .find(|npc| usize::from(npc.number) == client)
                        .map(|npc| npc.current_origin)
                })
                .unwrap_or_default();
            let mut event = sjk_game_jka::knockdown::entity_sound(
                origin,
                client as u16,
                sjk_game_jka::triggers::CHAN_AUTO,
            );
            event.parameter = u32::from(sound);
            let _ = self.pool.spawn_temporary(event.state(), level_time, None);
        }
        self.fire_targets(&fired.target, client, level_time);
    }

    /// `G_UseTargets`: everything of that name, in the map's order.
    pub(super) fn fire_targets(&mut self, name: &str, client: usize, level_time: i32) {
        if name.is_empty() {
            return;
        }
        let matching: Vec<usize> = self
            .targets
            .iter()
            .enumerate()
            .filter(|(_, (targetname, _, _))| targetname == name)
            .map(|(index, _)| index)
            .collect();
        for index in matching {
            let (_, target, state) = self.targets[index].clone();
            // `GlobalUse`: nothing deactivated is used (`FL_INACTIVE`).
            if state.inactive {
                continue;
            }
            match target {
                sjk_game_jka::triggers::Target::Print { message, .. } => {
                    self.told
                        .push(Told::Everyone(format!("cp \"{message}\"").into_bytes()));
                }
                // Speakers are entities of their own (`use_map_effects`, below).
                sjk_game_jka::triggers::Target::Speaker { .. } => {}
                sjk_game_jka::triggers::Target::Relay { target, .. } => {
                    self.fire_targets(&target, client, level_time)
                }
                sjk_game_jka::triggers::Target::Delay {
                    wait,
                    random,
                    spawnflags,
                    ..
                } => {
                    let state = self.targets[index].2;
                    if spawnflags & 1 != 0 && state.next_think > level_time {
                        continue;
                    }
                    let spread = self.deaths.rng.flrand(-1.0, 1.0);
                    self.targets[index].2.next_think =
                        level_time + ((wait + random * spread) * 1_000.0) as i32;
                }
                sjk_game_jka::triggers::Target::SetActive { target, active } => {
                    for (name, _, state) in self.targets.iter_mut() {
                        if *name == target {
                            state.inactive = !active;
                        }
                    }
                    for trigger in self.multiples.iter_mut() {
                        if trigger.targetname == target {
                            trigger.inactive = !active;
                        }
                    }
                    self.set_map_logic_active(&target, active);
                    self.set_scripted_active(&target, active);
                }
            }
        }
        self.use_doors(name, client, level_time);
        self.use_map_effects(name, client, level_time);
        self.use_map_turrets(name);
        self.use_stock_entities(name, client, level_time);
        self.use_npc_spawners(name, level_time);
        self.use_siege_entities(name, client, level_time);
        self.use_scripted(name, client, level_time);
    }

    /// `G_RunThink` for the triggers and targets counting something down.
    pub(super) fn run_multiples(&mut self, level_time: i32) {
        for index in 0..self.multiples.len() {
            let trigger = &mut self.multiples[index];
            if trigger.next_fire == 0 || trigger.next_fire > level_time {
                continue;
            }
            trigger.next_fire = 0;
            if trigger.clearing {
                // `trigger_cleared_fire`: nobody touches it any more.
                let rng = &mut self.deaths.rng;
                let target2 =
                    sjk_game_jka::triggers::cleared(trigger, level_time, || rng.flrand(-1.0, 1.0));
                let activator = trigger.activator.unwrap_or(usize::MAX);
                self.fire_targets(&target2, activator, level_time);
            } else if trigger.pending {
                self.fire_trigger(index, 0, level_time);
            }
        }
        for index in 0..self.targets.len() {
            let (_, target, state) = &mut self.targets[index];
            if state.next_think == 0 || state.next_think > level_time {
                continue;
            }
            state.next_think = 0;
            if let sjk_game_jka::triggers::Target::Delay { target, .. } = target.clone() {
                self.fire_targets(&target, 0, level_time);
            }
        }
    }
}
