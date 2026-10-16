//! `G_TouchTriggers` for an NPC on this server ([`sjk_game_jka::npc_triggers`]): the map's
//! hurt brushes, jump pads, pushers and teleporters, `trigger_multiple`s and the doors' and
//! plats' own triggers, touched by an NPC's box where its move left it — as the players'
//! are (`bridge.rs`, `bridge_doors.rs`), with the reference's NPC rules.
//!
//! The roster tells of each NPC's touch as its think ends ([`NpcOutcome::TouchTriggers`]);
//! they are carried out in the NPCs' order once the roster's run is over, against where the
//! NPC then stands (another NPC's later think may have shoved it; the reference touches at
//! once). The trigger kinds are taken in the players' order (hurt, movers,
//! `trigger_multiple`, doors), not in the entity-number order of `EntitiesInBox`.

use super::super::*;
use sjk_game_jka::mover_team;
use sjk_game_jka::movers::{self, MoverKind, MoverState};
use sjk_game_jka::npc_spawn::NpcActor;
use sjk_game_jka::triggers::{self as rules, Activator, Touched, Touched2};

/// `Q3_INFINITE`: a pit's blow on an NPC (`hurt_touch`, `g_trigger.c:1397-1403`).
const Q3_INFINITE: i32 = 16_777_216;

/// `trap->EntityContact`: the box `mins`..`maxs` about `origin` against inline model
/// `model`'s own brushes.
fn contact(
    map: &crate::map::LoadedMap,
    model: usize,
    origin: [f32; 3],
    (mins, maxs): ([f32; 3], [f32; 3]),
) -> bool {
    let low: [f32; 3] = std::array::from_fn(|axis| origin[axis] + mins[axis]);
    let high: [f32; 3] = std::array::from_fn(|axis| origin[axis] + maxs[axis]);
    let middle: [f32; 3] = std::array::from_fn(|axis| (low[axis] + high[axis]) / 2.0);
    let half: [f32; 3] = std::array::from_fn(|axis| (high[axis] - low[axis]) / 2.0);
    let Ok(bounds) = sjk_bsp::Aabb::new(half.map(|value| -value), half) else {
        return false;
    };
    map.bsp
        .trace_model_box(model, middle, middle, bounds, u32::MAX)
        .start_solid
}

impl NativeGame {
    /// The NPC numbered `number`, while it is still one.
    fn npc_actor(&mut self, number: u16) -> Option<&mut NpcActor> {
        self.npcs
            .roster
            .actors
            .iter_mut()
            .find(|npc| npc.number == number)
    }

    /// Where NPC `number` stands and its box, while it may touch triggers.
    fn npc_box(&mut self, number: u16) -> Option<([f32; 3], ([f32; 3], [f32; 3]))> {
        let npc = self.npc_actor(number)?;
        sjk_game_jka::npc_triggers::touches_triggers(npc)
            .then(|| (npc.player.origin(), (npc.mins, npc.maxs)))
    }

    /// `G_TouchTriggers` for NPC `number` at `level_time`.
    pub(in super::super) fn npc_touch_triggers(&mut self, number: u16, level_time: i32) {
        self.npc_touch_hurts(number, level_time);
        self.npc_touch_movers(number, level_time);
        self.npc_touch_multiples(number, level_time);
        self.npc_touch_doors(number, level_time);
    }

    /// `hurt_touch` on an NPC: health taken at the brush's rate (`G_Damage` through the
    /// roster, the brush the attacker), or — a pit's `dmg -1` brush — its fall begun and the
    /// NPC killed outright by itself (`MOD_FALLING`), where a player would scream.
    fn npc_touch_hurts(&mut self, number: u16, level_time: i32) {
        for index in 0..self.triggers.len() {
            let Some((origin, bounds)) = self.npc_box(number) else {
                return;
            };
            let (brush, trigger) = &self.triggers[index];
            let brush = brush.legacy_number();
            if !trigger.linked
                || !rules::near(origin, trigger)
                || !self
                    .map
                    .as_ref()
                    .is_some_and(|map| contact(map, trigger.model, origin, bounds))
            {
                continue;
            }
            let mut trigger = self.triggers[index].1.clone();
            let Some(npc) = self.npc_actor(number) else {
                return;
            };
            let (health, takes_damage) = (npc.health, npc.takes_damage);
            let touched = rules::hurt_touch(
                &mut trigger,
                &mut npc.player,
                health,
                takes_damage,
                level_time,
            );
            self.triggers[index].1 = trigger;
            let request = match touched {
                Touched::Hurt {
                    damage,
                    flags,
                    means,
                } => {
                    let attacker = Attacker {
                        npc: false,
                        client: brush,
                        max_health: 100,
                        team: 0,
                        saber_knockback: [0.0; 4],
                    };
                    DamageRequest {
                        level_time,
                        attacker: Some(attacker),
                        direction: None,
                        point: None,
                        damage,
                        flags,
                        means,
                    }
                }
                Touched::Fade { .. } => {
                    let attacker = self.attacker_for(number);
                    DamageRequest {
                        level_time,
                        attacker,
                        direction: Some([0.0, 1.0, 0.0]),
                        point: Some(origin),
                        damage: Q3_INFINITE,
                        flags: 0,
                        means: sjk_game_jka::means_of_death::MOD_FALLING,
                    }
                }
                Touched::Nothing | Touched::Respawn => continue,
            };
            let _ = self.strike(usize::MAX, usize::from(number), request, false);
        }
    }

    /// `trigger_teleporter_touch`, `trigger_push_touch` (the jump pad's throw and the
    /// pusher's push) on an NPC, and `G_TouchTriggers`' tail: a pad is remembered for one
    /// move only.
    fn npc_touch_movers(&mut self, number: u16, level_time: i32) {
        let mut on_a_pad = false;
        for index in 0..self.movers.len() {
            let Some((origin, bounds)) = self.npc_box(number) else {
                return;
            };
            let (pad, mover, destination, angles) = self.movers[index].clone();
            if !mover.touches
                || !self
                    .map
                    .as_ref()
                    .is_some_and(|map| contact(map, mover.model, origin, bounds))
            {
                continue;
            }
            let Some(npc) = self.npc_actor(number) else {
                return;
            };
            let movement_type = npc.player.movement_type();
            if mover.kind == rules::ET_TELEPORT_TRIGGER {
                if rules::may_teleport(&mover, movement_type, false) {
                    self.npc_teleport(number, destination, angles, level_time);
                }
                continue;
            }
            if rules::is_jump_pad(&mover) {
                let pad = rules::JumpPad {
                    number: pad.legacy_number(),
                    velocity: mover.velocity,
                };
                on_a_pad |= rules::touch_jump_pad(&mut npc.player, pad, movement_type, 0);
                continue;
            }
            let state = npc.player.clone();
            if let Some(velocity) =
                rules::pushed(&mut self.movers[index].1, &state, movement_type, level_time)
                && let Some(npc) = self.npc_actor(number)
            {
                npc.player.set_velocity(velocity);
            }
        }
        if !on_a_pad && let Some(npc) = self.npc_actor(number) {
            npc.player.set_raw_field(rules::PS_JUMPPAD_ENT, 0);
        }
    }

    /// `TeleportPlayer` on NPC `number`: the flashes where it left and where it arrives,
    /// its jump ([`sjk_game_jka::npc_triggers::teleport`]), and `G_KillBox` — every other
    /// client where it arrives telefragged by it.
    fn npc_teleport(
        &mut self,
        number: u16,
        destination: [f32; 3],
        angles: [f32; 3],
        level_time: i32,
    ) {
        let Some(npc) = self.npc_actor(number) else {
            return;
        };
        let left = npc.player.origin();
        let teleported = sjk_game_jka::npc_triggers::teleport(npc, destination, angles);
        let (mins, maxs, arrived) = (npc.mins, npc.maxs, teleported.origin);
        if teleported.flashes {
            let _ = self.pool.spawn_temporary(
                EventEntity::teleport_out(left, number).state(),
                level_time,
                None,
            );
            let _ = self.pool.spawn_temporary(
                EventEntity::teleport_in(destination, number).state(),
                level_time,
                None,
            );
        }
        if !teleported.kill_box {
            return;
        }
        let (low, high): ([f32; 3], [f32; 3]) = (
            std::array::from_fn(|axis| arrived[axis] + mins[axis]),
            std::array::from_fn(|axis| arrived[axis] + maxs[axis]),
        );
        let meets = |(absmin, absmax): ([f32; 3], [f32; 3])| {
            (0..3).all(|axis| absmin[axis] <= high[axis] && absmax[axis] >= low[axis])
        };
        let mut victims: Vec<u16> = (0..self.players.places())
            .filter(|client| {
                self.peer(*client).is_some_and(|peer| {
                    peer.begun
                        && peer.playing()
                        && meets(super::super::bridge_force::linked_box(peer))
                })
            })
            .map(|client| client as u16)
            .collect();
        victims.extend(
            self.npcs
                .roster
                .actors
                .iter()
                .filter(|other| other.number != number && other.begun() && meets(other.link))
                .map(|other| other.number),
        );
        victims.sort_unstable();
        let attacker = self.attacker_for(number);
        for victim in victims {
            let request = DamageRequest {
                level_time,
                attacker,
                direction: None,
                point: None,
                damage: 100_000,
                flags: DAMAGE_NO_PROTECTION,
                means: MOD_TELEFRAG,
            };
            let _ = self.strike(usize::from(number), usize::from(victim), request, false);
        }
    }

    /// `Touch_Multi` for an NPC ([`rules::touch_multiple_as`]): the trigger's NPC gates,
    /// then its firing with the NPC as the activator.
    fn npc_touch_multiples(&mut self, number: u16, level_time: i32) {
        for index in 0..self.multiples.len() {
            let Some((origin, bounds)) = self.npc_box(number) else {
                return;
            };
            if !self
                .map
                .as_ref()
                .is_some_and(|map| contact(map, self.multiples[index].model, origin, bounds))
            {
                continue;
            }
            let Some(npc) = self
                .npcs
                .roster
                .actors
                .iter_mut()
                .find(|npc| npc.number == number)
            else {
                return;
            };
            let who = sjk_game_jka::npc_triggers::toucher(npc);
            let activator = Activator::Npc {
                script_targetname: npc.targetname.as_deref(),
            };
            let mut touched = rules::touch_multiple_as(
                &mut self.multiples[index],
                &who,
                activator,
                [1.0, 0.0, 0.0],
                level_time,
            );
            if touched == Touched2::Uses {
                // The pose a USE_BUTTON trigger puts its user in (`g_trigger.c:547-559`).
                sjk_game_jka::npc_triggers::use_pose(npc, who.torso_anim);
                let activator = Activator::Npc {
                    script_targetname: npc.targetname.as_deref(),
                };
                touched = rules::pressed_as(&mut self.multiples[index], activator, level_time);
            }
            if touched == Touched2::Fires {
                self.fire_trigger(index, usize::from(number), level_time);
            }
        }
    }

    /// `Touch_DoorTrigger` and `Touch_PlatCenterTrigger` for an NPC in a mover's own
    /// trigger: a door opens for any NPC but a vehicle, a plat at the bottom is used.
    fn npc_touch_doors(&mut self, number: u16, level_time: i32) {
        let opens = self
            .npc_actor(number)
            .is_some_and(|npc| sjk_game_jka::npc_triggers::opens_doors(npc));
        for index in 0..self.doors.len() {
            let Some((origin, (mins, maxs))) = self.npc_box(number) else {
                return;
            };
            let door = &self.doors[index].1;
            if !movers::spawns_own_trigger(door) {
                continue;
            }
            let (low, high) = match door.kind {
                MoverKind::Plat => movers::plat_trigger_bounds(door),
                MoverKind::Door | MoverKind::Button => {
                    let (low, high, _) = mover_team::team_trigger_bounds(&self.doors, index);
                    (low, high)
                }
            };
            if !(0..3).all(|axis| {
                origin[axis] + mins[axis] < high[axis] && origin[axis] + maxs[axis] > low[axis]
            }) {
                continue;
            }
            let fired = match self.doors[index].1.kind {
                MoverKind::Plat => (self.doors[index].1.state == MoverState::Pos1)
                    .then(|| {
                        mover_team::use_mover(
                            &mut self.doors,
                            index,
                            level_time,
                            Some(usize::from(number)),
                        )
                    })
                    .flatten(),
                MoverKind::Door | MoverKind::Button if opens => mover_team::touch_trigger(
                    &mut self.doors,
                    index,
                    level_time,
                    Some(usize::from(number)),
                ),
                MoverKind::Door | MoverKind::Button => continue,
            };
            self.publish_team(index);
            if let Some(target) = fired {
                self.fire_targets(&target, usize::from(number), level_time);
            }
        }
    }
}
