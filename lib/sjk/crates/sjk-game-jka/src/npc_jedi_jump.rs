//! A Jedi NPC's jumps (`codemp/game/NPC_AI_Jedi.c:4441-4886`, `5010-5121`): the throw at
//! a spot whose arc it traces (`Jedi_Jump`), the chase jump at its enemy or a goal
//! (`Jedi_TryJump`), facing the goal while it flies (`Jedi_Jumping`), and the jumps it
//! calls off because they would land nowhere safe (`Jedi_CheckJumps`).

use crate::npc_jedi_combat::{
    ENTITYNUM_NONE, PS_FORCE_POWERS_ACTIVE, PS_WEAPON_TIME, SCF_NO_ACROBATICS, class, rank,
};
use crate::npc_senses::{Spot, distance_squared, spot};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use crate::pmove::MovementTrace;
use crate::saber_clash::normalize;
use sjk_protocol::UserCommand;

/// `TR_GRAVITY`; `CONTENTS_BOTCLIP`; `ENTITYNUM_WORLD`; `JUMP_VELOCITY`.
const TR_GRAVITY: u8 = 6;
const CONTENTS_BOTCLIP: u32 = 0x40;
const ENTITYNUM_WORLD: u16 = 1_022;
const JUMP_VELOCITY: f32 = 225.0;
/// `FP_LEVITATION`; the player-state field `fd.forceJumpZStart`.
const FP_LEVITATION: u32 = 1;
const PS_FORCE_JUMP_Z_START: usize = 74;
/// `CHAN_ITEM`, `CHAN_BODY`.
const CHAN_ITEM: u32 = 5;
const CHAN_BODY: u32 = 6;
/// The chase jump's animations: `BOTH_FORCEJUMP1`, `BOTH_FLIP_F`.
const BOTH_FORCEJUMP1: u16 = 1_151;
const BOTH_FLIP_F: u16 = 1_163;
/// `Jedi_Jump`'s slices of the arc, and the tries at a faster or slower throw.
const TIME_STEP: i32 = 500;
const MAX_HITS: i32 = 7;

/// What a chase jump goes for (`Jedi_TryJump`'s `goal`): the enemy, or a spot a goal
/// entity marks (`Jedi_Combat`'s temporary goal at the blocked destination).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JumpGoal {
    /// `goal->s.number`.
    pub number: u16,
    /// `goal->r.currentOrigin`.
    pub origin: [f32; 3],
    /// For a client, whether it stands on something (`ps.groundEntityNum`); `None` for an
    /// entity that is no client.
    pub client_on_ground: Option<bool>,
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `Jedi_Jump` (`NPC_AI_Jedi.c:4445-4689`): a throw at `dest`, its arc traced in half
    /// second slices by the NPC's box (a bot-clip brush stops it on the way up); a throw
    /// blocked short of `dest` is tried again up to six times, slower and then faster.
    /// Whatever the tries found, the last throw becomes the NPC's velocity (the reference
    /// overwrites its closest miss). Always true.
    pub fn jedi_jump(&mut self, me: usize, dest: [f32; 3], goal_ent: u16) -> bool {
        let gravity = self.actors[me].player.gravity();
        let mut shot_speed: f32 = 300.0;
        let mut hit_count = 0;
        let mut shot_vel = [0.0_f32; 3];
        while hit_count < MAX_HITS {
            let origin = self.actors[me].current_origin;
            let mut target_dir = crate::npc_senses::subtract(dest, origin);
            let target_dist = normalize(&mut target_dir);
            shot_vel = target_dir.map(|axis| axis * shot_speed);
            let travel_time = target_dist / shot_speed;
            // `travelTime * 0.5 * gravity`: a double product added to the float.
            shot_vel[2] =
                (f64::from(shot_vel[2]) + f64::from(travel_time) * 0.5 * f64::from(gravity)) as f32;
            if !self.jedi_arc_blocked(me, shot_vel, travel_time * 1000.0, dest, goal_ent) {
                break;
            }
            hit_count += 1;
            shot_speed = (300 + (hit_count - 2) * 100) as f32;
            if hit_count >= 2 {
                shot_speed += 100.0;
            }
        }
        self.actors[me].player.set_velocity(shot_vel);
        true
    }

    /// One trace of a jump's arc from `from` to `to` by the NPC's box: on the way up bot-clip
    /// brushes stop it too.
    fn jedi_arc_trace(&mut self, me: usize, from: [f32; 3], to: [f32; 3]) -> MovementTrace {
        let npc = &self.actors[me];
        let mask = if to[2] < from[2] {
            npc.clip_mask
        } else {
            npc.clip_mask | CONTENTS_BOTCLIP
        };
        let (mins, maxs, number) = (npc.mins, npc.maxs, npc.number);
        self.trace_bodies(from, mins, maxs, to, number, mask)
    }

    /// Whether a throw at `shot_vel` is blocked before it lands (`NPC_AI_Jedi.c:4495-4577`):
    /// it hits something other than the goal away from `dest`, or ends over a drop of more
    /// than 128 units. `travel_time` is in milliseconds, a float as the reference keeps it.
    fn jedi_arc_blocked(
        &mut self,
        me: usize,
        shot_vel: [f32; 3],
        travel_time: f32,
        dest: [f32; 3],
        goal_ent: u16,
    ) -> bool {
        let level_time = self.level_time;
        let base = self.actors[me].current_origin;
        let last_slice = f64::from(travel_time).floor();
        let mut last = base;
        let mut elapsed = TIME_STEP;
        while f64::from(elapsed) < last_slice + f64::from(TIME_STEP) {
            if elapsed as f32 > travel_time {
                elapsed = last_slice as i32;
            }
            let test = crate::trajectory::legacy_evaluate_trajectory(
                base,
                shot_vel,
                TR_GRAVITY,
                level_time,
                0,
                level_time + elapsed,
            );
            let trace = self.jedi_arc_trace(me, last, test);
            if trace.all_solid || trace.start_solid {
                return true;
            }
            if trace.fraction < 1.0 {
                // A bot-clip brush is told apart by `trace.contents`, which the traces do not
                // report: it counts as any other surface.
                if trace.entity_number == goal_ent {
                    return false;
                }
                return !(f64::from(trace.plane_normal[2]) > 0.7
                    && distance_squared(trace.end_position, dest) < 4096.0);
            }
            if f64::from(elapsed) == last_slice {
                let mut bottom = trace.end_position;
                bottom[2] -= 128.0;
                let npc = &self.actors[me];
                let (mins, maxs, number, mask) = (npc.mins, npc.maxs, npc.number, npc.clip_mask);
                let floor = self.trace_bodies(trace.end_position, mins, maxs, bottom, number, mask);
                return floor.fraction >= 1.0;
            }
            last = test;
            elapsed += TIME_STEP;
        }
        false
    }

    /// The goal entity `number` as a chase jump reads it: a client's place and footing, or
    /// another entity's place; `None` for none.
    fn jedi_jump_goal(&self, number: u16) -> Option<JumpGoal> {
        if let Some(client) = self.client_view(number) {
            return Some(JumpGoal {
                number,
                origin: client.origin,
                client_on_ground: Some(client.ground_entity != ENTITYNUM_NONE),
            });
        }
        let (origin, _, _) = self.host.entity_box(number)?;
        Some(JumpGoal {
            number,
            origin,
            client_on_ground: None,
        })
    }

    /// `Jedi_TryJump` (`NPC_AI_Jedi.c:4691-4837`) at the entity `goal`.
    pub fn jedi_try_jump(
        &mut self,
        me: usize,
        goal: Option<u16>,
        command: &mut UserCommand,
    ) -> bool {
        match goal.and_then(|number| self.jedi_jump_goal(number)) {
            Some(goal) => self.jedi_try_jump_to(me, goal, command),
            None => false,
        }
    }

    /// `Jedi_TryJump`: allowed acrobatics and not debounced, with the goal on its feet (or no
    /// client) and the NPC neither knocked down nor rolling, a goal within 550 units and not
    /// 400 below — hurt and above a drop, it walks off; near and level, it hops; else it
    /// throws itself at the goal (beside the enemy, where there is floor), flipping unless it
    /// is of low rank, with the jump's sound. Any of these debounces the next jump two to
    /// five seconds and runs it forward.
    pub fn jedi_try_jump_to(
        &mut self,
        me: usize,
        goal: JumpGoal,
        command: &mut UserCommand,
    ) -> bool {
        let npc = &self.actors[me];
        if npc.script_flags & SCF_NO_ACROBATICS != 0
            || !self.jedi_timer_done(me, "jumpChaseDebounce")
            || goal.client_on_ground == Some(false)
        {
            return false;
        }
        let (legs, legs_timer) = (npc.player.leg_animation(), npc.player.legs_timer());
        if crate::pmove_hand_extend::in_knockdown(legs, legs_timer)
            || (crate::pmove_roll_anim::in_roll(legs) && legs_timer > 0)
        {
            return false;
        }
        let mut goal_diff = crate::npc_senses::subtract(goal.origin, npc.current_origin);
        let goal_z_diff = goal_diff[2];
        goal_diff[2] = 0.0;
        let goal_xy_dist = normalize(&mut goal_diff);
        if goal_xy_dist >= 550.0 || goal_z_diff <= -400.0 {
            return false;
        }
        let health = npc.health;
        let debounce =
            if health < 150 && ((health < 30 && goal_z_diff < 0.0) || goal_z_diff < -128.0) {
                true
            } else if goal_z_diff < 32.0 && goal_xy_dist < 200.0 {
                command.up_move = 127;
                true
            } else if goal_z_diff > 0.0 || goal_xy_dist > 128.0 {
                let dest = if self.actors[me].mind.enemy == Some(goal.number) {
                    self.jedi_landing_beside(me, goal)
                } else {
                    goal.origin
                };
                self.jedi_jump(me, dest, goal.number) && self.jedi_launch_chase(me)
            } else {
                false
            };
        if !debounce {
            return false;
        }
        let hold = self.host.irand(2000, 5000);
        self.jedi_timer_set(me, "jumpChaseDebounce", hold);
        command.forward_move = 127;
        self.actors[me].mind.move_dir = [0.0; 3];
        let level_time = self.level_time;
        self.jedi_timer_set(me, "duck", -level_time);
        true
    }

    /// Where beside the enemy a chase jump lands (`NPC_AI_Jedi.c:4737-4774`): up to ten
    /// steps of a quarter more than its box to either side — each from the last, as the
    /// reference adds them up — until one has floor within 128 units below; else the enemy's
    /// own place.
    fn jedi_landing_beside(&mut self, me: usize, goal: JumpGoal) -> [f32; 3] {
        let (enemy_mins, enemy_maxs) = self
            .client_view(goal.number)
            .map(|enemy| (enemy.mins, enemy.maxs))
            .unwrap_or_default();
        let mut dest = goal.origin;
        for _ in 0..10 {
            for axis in 0..2 {
                let side = if self.host.irand(0, 1) != 0 {
                    enemy_maxs[axis]
                } else {
                    enemy_mins[axis]
                };
                dest[axis] = (f64::from(dest[axis]) + f64::from(side) * 1.25) as f32;
            }
            let mut bottom = dest;
            bottom[2] -= 128.0;
            let npc = &self.actors[me];
            let (mins, maxs, mask) = (npc.mins, npc.maxs, npc.clip_mask);
            if self
                .trace_bodies(dest, mins, maxs, bottom, goal.number, mask)
                .fraction
                < 1.0
            {
                return dest;
            }
        }
        goal.origin
    }

    /// A chase jump thrown (`NPC_AI_Jedi.c:4788-4819`): the jump or flip held, the height it
    /// began at, the weapon kept still through it, levitation on, the jump's sound (Boba
    /// Fett's jet pack), and two to three seconds of chasing. Always true.
    fn jedi_launch_chase(&mut self, me: usize) -> bool {
        use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
        let npc = &self.actors[me];
        let (npc_class, npc_rank) = (npc.definition.client_class, npc.definition.rank);
        let anim = if npc_class == class::BOBAFETT
            || (npc_rank != rank::CREWMAN && npc_rank <= rank::LT_JG)
        {
            BOTH_FORCEJUMP1
        } else {
            BOTH_FLIP_F
        };
        self.set_animation(
            me,
            SETANIM_BOTH,
            anim,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        );
        let npc = &mut self.actors[me];
        npc.player
            .set_raw_field(PS_FORCE_JUMP_Z_START, npc.current_origin[2].to_bits());
        npc.player
            .set_raw_field(PS_WEAPON_TIME, npc.player.torso_timer() as u32);
        let active = npc.player.force_powers_active() | (1 << FP_LEVITATION);
        npc.player.set_raw_field(PS_FORCE_POWERS_ACTIVE, active);
        if npc_class == class::BOBAFETT {
            self.jedi_sound_on_entity(me, CHAN_ITEM, b"sound/boba/jeton.wav");
            let burn = self.host.irand(1000, 3000);
            self.actors[me].mind.jet_pack_time = self.level_time + burn;
        } else {
            self.jedi_sound_on_entity(me, CHAN_BODY, b"sound/weapons/force/jump.wav");
        }
        let chase = self.host.irand(2000, 3000);
        self.jedi_timer_set(me, "forceJumpChasing", chase);
        true
    }

    /// `G_SoundOnEnt` (`g_utils.c`): the sound's temp entity at the NPC, naming it and the
    /// channel.
    pub(crate) fn jedi_sound_on_entity(&mut self, me: usize, channel: u32, name: &[u8]) {
        let sound = self.host.sound_index(name);
        let npc = &self.actors[me];
        let mut event = crate::knockdown::entity_sound(npc.current_origin, npc.number, channel);
        event.parameter = u32::from(sound);
        self.host.raise(event);
    }

    /// `Jedi_Jumping` (`NPC_AI_Jedi.c:4839-4886`): while a chase jump lasts the NPC faces
    /// its goal's leaning head in the air; landed, the chase is over.
    pub fn jedi_jumping(
        &mut self,
        me: usize,
        goal: Option<u16>,
        command: &mut UserCommand,
    ) -> bool {
        let Some(goal) = goal else { return false };
        if self.jedi_timer_done(me, "forceJumpChasing") {
            return false;
        }
        if self.actors[me].player.ground_entity_num() != ENTITYNUM_NONE {
            self.jedi_timer_set(me, "forceJumpChasing", 0);
            return false;
        }
        // `NPC_FaceEntity`: an entity with no body is not faced.
        if let Some(body) = self.body(goal) {
            self.face_position(me, spot(&body, Spot::HeadLean), true, command);
        }
        true
    }

    /// `Jedi_CheckJumps` (`NPC_AI_Jedi.c:5010-5121`): a jump the NPC is about to make — a
    /// charged Force jump or a plain one — traced along its arc for four seconds; one that
    /// starts in a solid, hits a bot-clip brush, drops more than 400 units, lands on glass or
    /// finds no floor 128 units below its end is called off. None at all without acrobatics.
    pub fn jedi_check_jumps(&mut self, me: usize, command: &mut UserCommand) {
        if self.actors[me].script_flags & SCF_NO_ACROBATICS != 0 {
            self.jedi_unsafe_jump(me, command);
            return;
        }
        let jump_vel = if self.actors[me].force.jump_charge != 0.0 {
            self.velocity_for_force_jump(me, command).0
        } else if command.up_move > 0 {
            let mut velocity = self.actors[me].player.velocity();
            velocity[2] = JUMP_VELOCITY;
            velocity
        } else {
            return;
        };
        if jump_vel[0] == 0.0 && jump_vel[1] == 0.0 {
            return;
        }
        if !self.jedi_jump_lands(me, jump_vel) {
            self.jedi_unsafe_jump(me, command);
        }
    }

    /// Whether a jump at `jump_vel` lands safely (`NPC_AI_Jedi.c:5053-5115`).
    fn jedi_jump_lands(&mut self, me: usize, jump_vel: [f32; 3]) -> bool {
        let level_time = self.level_time;
        let origin = self.actors[me].current_origin;
        let mut last = origin;
        let mut end = [0.0_f32; 3];
        for elapsed in (500..=4000).step_by(500) {
            let test = crate::trajectory::legacy_evaluate_trajectory(
                origin,
                jump_vel,
                TR_GRAVITY,
                level_time,
                0,
                level_time + elapsed,
            );
            let trace = self.jedi_arc_trace(me, last, test);
            if trace.all_solid || trace.start_solid {
                return false;
            }
            end = trace.end_position;
            if trace.fraction < 1.0 {
                // A bot-clip brush (`trace.contents`) is not told apart: see `jedi_arc_blocked`.
                break;
            }
            last = test;
        }
        let mut bottom = end;
        if bottom[2] > origin[2] {
            bottom[2] = origin[2];
        } else if origin[2] - bottom[2] > 400.0 {
            return false;
        }
        bottom[2] -= 128.0;
        let npc = &self.actors[me];
        let (mins, maxs, number, mask) = (npc.mins, npc.maxs, npc.number, npc.clip_mask);
        let floor = self.trace_bodies(end, mins, maxs, bottom, number, mask);
        if floor.all_solid || floor.start_solid || floor.fraction < 1.0 {
            return !(floor.entity_number < ENTITYNUM_WORLD
                && self.host.glass(floor.entity_number));
        }
        false
    }

    /// `jump_unsafe`: no charged jump, no jump at all.
    fn jedi_unsafe_jump(&mut self, me: usize, command: &mut UserCommand) {
        self.actors[me].force.jump_charge = 0.0;
        command.up_move = 0;
    }
}
