//! The states only a script sets (`NPC_behavior.c`): asleep until an alert (`NPC_BSSleep`,
//! `:518-539`; the wake-up script is not run), a jump to the navigation goal along a
//! parabola (`NPC_BSJump`, `:749-939`), waiting to be out of client 0's sight to vanish
//! (`NPC_BSRemove`, `:941-958`), and flying through walls to the goal (`NPC_BSNoClip`,
//! `:1181-1213`). A host that runs scripts sets them as `Q3_SetBState` does
//! (`g_ICARUScb.c:2599-2713`): the behaviour state, `client->noclip` for `BS_NOCLIP`, the
//! jump's state for `BS_JUMP`.

use crate::npc_senses::{AEL_MINOR, subtract};
use crate::npc_spawn::{NpcHost, NpcThink};
use crate::npc_states::jump;
use crate::npc_world::NpcWorld;
use crate::player_angle_math::vector_angles;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_LEGS};
use sjk_protocol::UserCommand;

/// `BOTH_CROUCH1`, `BOTH_INAIR1`, `BOTH_LAND1` (`anims.h`).
const BOTH_CROUCH1: u16 = 1_004;
const BOTH_INAIR1: u16 = 1_139;
const BOTH_LAND1: u16 = 1_140;
/// `APEX_HEIGHT`; `MIN_ANGLE_ERROR`.
const APEX_HEIGHT: f32 = 200.0;
const MIN_ANGLE_ERROR: f32 = 0.01;
/// `FL_NO_KNOCKBACK`; `NPCAI_MOVING`.
const FL_NO_KNOCKBACK: u32 = 0x800;
const NPCAI_MOVING: u32 = 0x4;
/// `NPCAI_TOUCHED_GOAL`.
const NPCAI_TOUCHED_GOAL: u32 = 0x8;
/// `EF_NODRAW`, `ET_INVISIBLE`; `s.eType`, `s.eFlags`.
const EF_NODRAW: u32 = 0x100;
const ET_INVISIBLE: u32 = 12;
const ES_TYPE: usize = 8;
const ES_EFLAGS: usize = 19;
/// `FRAMETIME`.
const FRAMETIME: i32 = 100;
/// `ENTITYNUM_NONE`.
const ENTITYNUM_NONE: u16 = 1_023;

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `Q3_SetBState` (`g_ICARUScb.c:2599-2713`), a script's `SET_BEHAVIOR_STATE`: a search
    /// or a wander started from the NPC's waypoint (none found: nothing changes); the
    /// temporary state cleared; the state set (and made the default for `BS_DEFAULT`);
    /// `client->noclip` for `BS_NOCLIP` alone; a jump begun facing its goal.
    pub fn script_set_bstate(&mut self, me: usize, state: i32) {
        use crate::npc_behavior::bstate;
        if state == bstate::SEARCH || state == bstate::WANDER {
            let mut waypoint = self.actors[me].mind.tactics.waypoint;
            if waypoint == crate::npc_mind::WAYPOINT_NONE {
                waypoint = self.closest_waypoint_for(
                    crate::npc_navigator::NavHolder::Actor(me),
                    crate::npc_mind::WAYPOINT_NONE,
                );
                self.actors[me].mind.tactics.waypoint = waypoint;
                if waypoint == crate::npc_mind::WAYPOINT_NONE {
                    return;
                }
            }
            self.search_start(me, waypoint, state);
        }
        let npc = &mut self.actors[me];
        npc.mind.temp_behavior = bstate::DEFAULT;
        if npc.behavior_state == bstate::NOCLIP && state != bstate::NOCLIP {
            // "rise up out of the floor after noclipping" (`G_SetOrigin`).
            npc.current_origin[2] += 0.125;
            let origin = npc.current_origin;
            npc.player.set_origin(origin);
        }
        npc.behavior_state = state;
        if state == bstate::DEFAULT {
            npc.default_behavior = state;
        }
        npc.ai_flags &= !NPCAI_TOUCHED_GOAL;
        npc.mind.noclip = state == bstate::NOCLIP;
        if state == bstate::JUMP {
            npc.mind.states.jump_state = jump::FACING;
        }
    }

    /// `NPC_BSSleep` (`NPC_behavior.c:518-539`): an alert noticed would run its wake-up
    /// script (`BSET_AWAKE`); none runs, and it sleeps on.
    pub fn bs_sleep(&mut self, me: usize) {
        let _ = self.check_alerts(me, -1, false, AEL_MINOR);
    }

    /// `NPC_BSJump` (`NPC_behavior.c:749-939`): facing the goal it crouches, then leaps
    /// for the apex a hundred units above the higher end of the way, lands, and is done
    /// (the goal cleared).
    pub fn bs_jump(&mut self, me: usize, command: &mut UserCommand) {
        let Some(goal) = self.actors[me].mind.goal.and_then(|_| self.goal_origin(me)) else {
            return;
        };
        let state = self.actors[me].mind.states.jump_state;
        if state != jump::JUMPING && state != jump::LANDING {
            let angles = vector_angles(subtract(goal, self.actors[me].current_origin));
            let npc = &mut self.actors[me];
            let (pitch, yaw) = (
                crate::npc_droid::angle_normalize360(angles[0]),
                crate::npc_droid::angle_normalize360(angles[1]),
            );
            (npc.mind.desired_pitch, npc.mind.locked_desired_pitch) = (pitch, pitch);
            (npc.desired_yaw, npc.mind.locked_desired_yaw) = (yaw, yaw);
        }
        self.update_angles(me, true, true, command);
        let npc = &self.actors[me];
        let yaw_error =
            crate::npc_senses::angle_delta(npc.player.view_angles()[1], npc.desired_yaw);
        let legs_timer = npc.player.legs_timer();
        match state {
            jump::FACING => {
                if yaw_error < MIN_ANGLE_ERROR {
                    self.set_animation(
                        me,
                        SETANIM_LEGS,
                        BOTH_CROUCH1,
                        SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                    );
                    self.actors[me].mind.states.jump_state = jump::CROUCHING;
                }
            }
            jump::CROUCHING => {
                if legs_timer > 0 {
                    return;
                }
                self.leap(me, goal);
            }
            jump::JUMPING => {
                if self.actors[me].state.ground_entity_num() != ENTITYNUM_NONE {
                    self.actors[me].player.set_velocity([0.0; 3]);
                    self.set_animation(
                        me,
                        SETANIM_BOTH,
                        BOTH_LAND1,
                        SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                    );
                    self.actors[me].mind.states.jump_state = jump::LANDING;
                } else if legs_timer > 0 {
                    return;
                } else {
                    self.set_animation(me, SETANIM_BOTH, BOTH_INAIR1, SETANIM_FLAG_OVERRIDE);
                }
            }
            jump::LANDING => {
                if legs_timer > 0 {
                    return;
                }
                self.actors[me].mind.states.jump_state = jump::WAITING;
                // "task complete no matter what".
                self.clear_goal(me);
                let npc = &mut self.actors[me];
                npc.mind.tactics.goal_time = self.level_time;
                npc.ai_flags &= !NPCAI_MOVING;
                command.forward_move = 0;
                npc.flags &= !FL_NO_KNOCKBACK;
            }
            _ => self.actors[me].mind.states.jump_state = jump::FACING,
        }
    }

    /// `NPC_BSJump`'s leap (`NPC_behavior.c:787-874`): the apex on the way from the higher
    /// end to the lower, half the rise above the higher; the velocity to reach it under the
    /// NPC's gravity (none without the time to).
    fn leap(&mut self, me: usize, goal: [f32; 3]) {
        let origin = self.actors[me].current_origin;
        let (p1, p2) = if origin[2] < goal[2] {
            (goal, origin)
        } else {
            (origin, goal)
        };
        let mut dir = subtract(p2, p1);
        dir[2] = 0.0;
        let mut xy = crate::saber_clash::normalize(&mut dir);
        let apex_height = APEX_HEIGHT / 2.0;
        let rise = p1[2] - p2[2];
        let z = (f64::from(apex_height + rise).sqrt() - f64::from(apex_height).sqrt()) as f32;
        if xy > 0.0 {
            xy -= z;
            xy *= 0.5;
        }
        let mut apex: [f32; 3] = std::array::from_fn(|axis| p1[axis] + xy * dir[axis]);
        apex[2] += apex_height;
        let npc = &mut self.actors[me];
        npc.mind.fight.death_point = apex;
        let gravity = npc.player.gravity();
        let height = apex[2] - origin[2];
        let time = (f64::from(height) / (0.5 * f64::from(gravity))).sqrt() as f32;
        if time == 0.0 {
            return;
        }
        let mut velocity = subtract(apex, origin);
        velocity[2] = 0.0;
        let distance = crate::saber_clash::normalize(&mut velocity);
        let forward = distance / time;
        let mut velocity = velocity.map(|value| value * forward);
        velocity[2] = time * gravity as f32;
        npc.player.set_velocity(velocity);
        npc.flags |= FL_NO_KNOCKBACK;
        npc.mind.states.jump_state = jump::JUMPING;
    }

    /// `NPC_BSRemove` (`NPC_behavior.c:941-958`): out of client 0's potentially visible set
    /// it fires its `target3`, vanishes (undrawn, invisible, no contents, no health, no
    /// name) and is freed at the next think.
    pub fn bs_remove(&mut self, me: usize, command: &mut UserCommand) {
        self.update_angles(me, true, true, command);
        let client_zero = self.body(0).map_or([0.0; 3], |player| player.origin);
        if self
            .host
            .in_pvs(self.actors[me].current_origin, client_zero)
        {
            return;
        }
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        if let Some(target) = npc.target3.clone() {
            self.fired.push(target);
        }
        let npc = &mut self.actors[me];
        let flags = npc.state.raw_field(ES_EFLAGS).unwrap_or(0);
        npc.state.set_raw_field(ES_EFLAGS, flags | EF_NODRAW);
        npc.state.set_raw_field(ES_TYPE, ET_INVISIBLE);
        npc.contents = 0;
        npc.health = 0;
        npc.targetname = None;
        npc.think = NpcThink::Free(level_time + FRAMETIME);
    }

    /// `NPC_BSNoClip` (`NPC_behavior.c:1181-1213`): straight at the goal in every axis
    /// (the NPC flies, `client->noclip`), or — no goal — stopped.
    pub fn bs_noclip(&mut self, me: usize, command: &mut UserCommand) {
        if self.update_goal(me, command).is_some() {
            let goal = self.goal_origin(me).unwrap_or([0.0; 3]);
            let npc = &mut self.actors[me];
            let mut dir = subtract(goal, npc.current_origin);
            npc.desired_yaw = vector_angles(dir)[1];
            let (forward, right) = crate::pmove::flight::flight_axes(npc.mind.current_angles);
            let (forward, right) = (forward.to_array(), right.to_array());
            crate::saber_clash::normalize(&mut dir);
            let dot =
                |axis: [f32; 3]| (axis[0] * dir[0] + axis[1] * dir[1] + axis[2] * dir[2]) * 127.0;
            command.forward_move = f64::from(dot(forward)).floor() as i8;
            command.right_move = f64::from(dot(right)).floor() as i8;
            command.up_move = f64::from(dot([0.0, 0.0, 1.0])).floor() as i8;
        } else {
            self.actors[me].player.set_velocity([0.0; 3]);
        }
        self.update_angles(me, true, true, command);
    }
}
