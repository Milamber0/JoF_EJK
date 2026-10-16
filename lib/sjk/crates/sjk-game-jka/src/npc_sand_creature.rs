//! Native server extension inspired by SP `AI_SandCreature.cpp`.
//!
//! Multiplayer has no sand-creature class/AI. This uses ordinary MP entity flags,
//! effects, animation IDs and damage; it never transmits SP held-victim flags.
//! The ambush inflicts a normal multiplayer death instead of retaining a swallowed
//! player across respawns. Stock-rules servers leave this extension disabled.

use crate::npc_creature::{CHAN_VOICE, DAMAGE_NO_ARMOR, DAMAGE_NO_KNOCKBACK, MOD_MELEE};
use crate::npc_senses::{Body, EF_NODRAW, distance_squared};
use crate::npc_spawn::{ENTITYNUM_WORLD, FL_NOTARGET, NpcActor, NpcHost, es};
use crate::npc_world::NpcWorld;
use crate::pmove_anim::{
    SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_FLAG_RESTART, SETANIM_LEGS,
};
use sjk_protocol::UserCommand;

const BODY: u32 = 0x100;
const BURIED_MASK: u32 = 0x21;
const NPC_MASK: u32 = BURIED_MASK | BODY | 0x1000;
const PS_EFLAGS: usize = 17;
const PS_LOOP_SOUND: usize = 75;
const ATTACK1: u16 = 113;
const ATTACK2: u16 = 114;
const BREACH: u16 = 1103;
const ATTACK_DISTANCE_SQUARED: f32 = 128.0;

/// Per-creature native state. No heap storage or references to a victim survive death.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SandCreature {
    target: Option<u16>,
    goal: [f32; 3],
    sensed_at: i32,
    breach_until: i32,
    attack_until: i32,
    next_breach: i32,
    next_miss: i32,
    next_voice: i32,
    previous_health: i32,
}

/// Model identity rather than an invented multiplayer `class_t` value.
pub fn is_sand_creature(npc: &NpcActor) -> bool {
    npc.definition
        .player_model
        .eq_ignore_ascii_case(b"sand_creature")
}

fn vibration(origin: [f32; 3], body: &Body) -> f32 {
    body.velocity.iter().map(|value| value * value).sum::<f32>()
        - distance_squared(origin, body.origin)
}

impl<H: NpcHost> NpcWorld<'_, H> {
    pub(crate) fn bs_sand_creature(&mut self, me: usize, command: &mut UserCommand) {
        let mut memory = self.actors[me].mind.creature.sand;
        let (time, origin, number) = (
            self.level_time,
            self.actors[me].current_origin,
            self.actors[me].number,
        );
        command.buttons = 0;
        command.forward_move = 0;
        command.right_move = 0;
        command.up_move = 0;
        self.actors[me].player.set_raw_field(PS_LOOP_SOUND, 0);
        self.actors[me].mind.enemy = None;
        self.actors[me].mind.combat_move = false;

        // Damage while exposed extends the visible reaction; no victim is held.
        if memory.previous_health > self.actors[me].health && self.actors[me].health > 0 {
            let duration = self.sand_animation(me, ATTACK1);
            memory.attack_until = time + duration + self.host.irand(500, 2000);
            memory.target = None;
        }
        memory.previous_health = self.actors[me].health;
        if time >= memory.attack_until {
            self.sand_hunt(me, command, &mut memory);
        }
        let attacking = time < memory.attack_until;
        let breaching = time < memory.breach_until;
        let visible = attacking || breaching || self.actors[me].health <= 0;
        let npc = &mut self.actors[me];
        // Only the travelling breach is solid, as in SP. An ambush can emerge
        // underneath its target without first being blocked by that target's hull.
        npc.contents = if breaching { BODY } else { 0 };
        npc.clip_mask = if breaching { NPC_MASK } else { BURIED_MASK };
        let flags = npc.player.raw_field(PS_EFLAGS).unwrap_or(0);
        let flags = if visible {
            flags & !EF_NODRAW
        } else {
            flags | EF_NODRAW
        };
        npc.player.set_raw_field(PS_EFLAGS, flags);
        npc.state.set_raw_field(es::EFLAGS, flags);
        if attacking {
            let remaining = npc.player.legs_timer();
            if remaining > 3700 || (1600..1900).contains(&remaining) {
                self.play_effect_at(
                    b"env/sand_spray",
                    [origin[0], origin[1], origin[2] - 40.0],
                    [0.0, 0.0, 1.0],
                );
            }
        }
        if time >= memory.next_voice && !breaching {
            let voices: [&[u8]; 3] = [
                b"sound/chars/sand_creature/voice1.mp3",
                b"sound/chars/sand_creature/voice2.mp3",
                b"sound/chars/sand_creature/voice3.mp3",
            ];
            let voice = self.host.irand(0, 2) as usize;
            self.creature_sound(number, CHAN_VOICE, voices[voice]);
            memory.next_voice = time + self.host.irand(3000, 10000);
        }
        self.update_angles(me, true, true, command);
        self.actors[me].mind.creature.sand = memory;
    }

    fn sand_grounded_target(&self, me: usize, body: &Body) -> bool {
        if body.number == self.actors[me].number
            || body.health <= 0
            || body.spectating
            || body.session_team == 3
            || body.flags & FL_NOTARGET != 0
            || body.entity_flags & EF_NODRAW != 0
        {
            return false;
        }
        if let Some(at) = self.actor_at(body.number) {
            !is_sand_creature(&self.actors[at])
                && self.actors[at].vehicle.is_none()
                && self.actors[at].player.ground_entity_num() == ENTITYNUM_WORLD
                && self.actors[at].player.raw_field(103).unwrap_or(0) & 1 == 0
        } else {
            self.host.client_ground(body.number) == Some(ENTITYNUM_WORLD)
        }
    }

    fn sand_hunt(&mut self, me: usize, command: &mut UserCommand, memory: &mut SandCreature) {
        let (time, origin) = (self.level_time, self.actors[me].current_origin);
        let radius = self.actors[me].definition.stats.earshot;
        let mut best = None;
        let mut best_score = 0.0;
        // One linear scan; do not look up each NPC again by entity number.
        let players = self.host.players().iter().copied().map(|body| {
            let grounded = self.host.client_ground(body.number) == Some(ENTITYNUM_WORLD);
            (body, grounded)
        });
        let npcs = self.order.iter().map(|&at| {
            let npc = &self.actors[at];
            let grounded = !is_sand_creature(npc)
                && npc.vehicle.is_none()
                && npc.player.ground_entity_num() == ENTITYNUM_WORLD
                && npc.player.raw_field(103).unwrap_or(0) & 1 == 0;
            (self.npc(at), grounded)
        });
        for (body, grounded) in players.chain(npcs) {
            if grounded
                && body.number != self.actors[me].number
                && body.health > 0
                && !body.spectating
                && body.session_team != 3
                && body.flags & FL_NOTARGET == 0
                && body.entity_flags & EF_NODRAW == 0
                && (0..3).all(|axis| (body.origin[axis] - origin[axis]).abs() <= radius)
            {
                let score = vibration(origin, &body);
                if score > best_score {
                    best_score = score;
                    best = Some(body);
                }
            }
        }
        let tracked = memory
            .target
            .and_then(|target| self.body(target))
            .filter(|body| self.sand_grounded_target(me, body));
        let sensed = best.or_else(|| {
            tracked.filter(|body| {
                body.velocity.iter().any(|value| *value != 0.0)
                    && vibration(origin, body) > -37500.0
            })
        });
        if let Some(body) = sensed {
            memory.target = Some(body.number);
            memory.goal = body.origin;
            memory.sensed_at = time;
        } else if tracked.is_none() || time - memory.sensed_at > 10000 {
            memory.target = None;
        }
        let Some(target) = memory.target.and_then(|target| self.body(target)) else {
            return;
        };
        let distance = distance_squared(origin, target.origin);
        if self.sand_grounded_target(me, &target)
            && distance < ATTACK_DISTANCE_SQUARED
            && time >= memory.breach_until
        {
            self.sand_attack(me, target.number, false, memory);
            return;
        }
        if time - memory.sensed_at > 3000 {
            return;
        }
        if time >= memory.breach_until
            && time >= memory.next_miss
            && (10000.0..250000.0).contains(&distance)
            && self.host.irand(0, 10) == 0
        {
            self.sand_attack(me, target.number, true, memory);
            return;
        }
        if distance_squared(origin, memory.goal) < ATTACK_DISTANCE_SQUARED {
            return;
        }
        self.face_position(me, memory.goal, false, command);
        let mut direction = crate::npc_senses::subtract(memory.goal, origin);
        self.ucmd_move_for_dir(me, command, &mut direction);
        self.actors[me].mind.dist_to_goal = distance_squared(origin, memory.goal).sqrt();
        if time >= memory.breach_until && time >= memory.next_breach && self.host.irand(0, 10) == 0
        {
            let npc = &self.actors[me];
            let trace = self.trace_bodies(origin, npc.mins, npc.maxs, origin, npc.number, NPC_MASK);
            if !trace.start_solid && !trace.all_solid {
                let duration = self.sand_animation(me, BREACH);
                memory.breach_until = time + duration;
                memory.next_breach = memory.breach_until + self.host.irand(0, 10000);
            }
        }
        let effect: &[u8] = if time < memory.breach_until {
            b"env/sand_move_breach"
        } else {
            b"env/sand_move"
        };
        let floor = origin[2] + self.actors[me].mins[2] + 2.0;
        self.play_effect_at(effect, [origin[0], origin[1], floor], [0.0, 0.0, 1.0]);
        let sound = self
            .host
            .sound_index(b"sound/chars/sand_creature/slither.wav");
        self.actors[me]
            .player
            .set_raw_field(PS_LOOP_SOUND, u32::from(sound));
    }

    fn sand_animation(&mut self, me: usize, animation: u16) -> i32 {
        self.set_animation(
            me,
            SETANIM_LEGS,
            animation,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD | SETANIM_FLAG_RESTART,
        );
        self.actors[me].player.legs_timer().max(1)
    }

    fn sand_attack(&mut self, me: usize, target: u16, miss: bool, memory: &mut SandCreature) {
        let animation = if self.host.irand(0, 1) == 0 {
            ATTACK1
        } else {
            ATTACK2
        };
        memory.attack_until = self.level_time + self.sand_animation(me, animation);
        memory.next_miss = memory.attack_until + self.host.irand(3000, 10000);
        if !miss {
            // MP clients have no sand-grab state. Resolve the catch with the normal
            // death path, preserving god mode, team rules and respawn ownership.
            self.creature_damage(
                me,
                target,
                None,
                None,
                1000,
                DAMAGE_NO_ARMOR | DAMAGE_NO_KNOCKBACK,
                MOD_MELEE,
            );
            memory.target = None;
        } else if let Some(body) = self.body(target) {
            let mut direction =
                crate::npc_senses::subtract(body.origin, self.actors[me].current_origin);
            direction[2] = direction[2].max(30.0);
            let distance = crate::saber_clash::normalize(&mut direction);
            if distance < 200.0 {
                self.creature_throw(
                    target,
                    direction,
                    ((200.0 - distance) * 0.4 + 20.0).min(45.0),
                );
            }
        }
    }
}
