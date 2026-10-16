//! The seeker drone's AI (`codemp/game/NPC_AI_Seeker.c`), which `NPC_BehaviorSet_Seeker`
//! (`NPC.c:1007-1022`) runs: its behaviour (`NPC_BSSeeker_Default`, `539-597`: its owner
//! gone, it goes; it never takes on the first client or another seeker), its pain
//! (`NPC_Seeker_Pain`, `53-66`), its hover (`Seeker_MaintainHeight`, `69-163`), its strafe
//! about itself or to its enemy's side (`Seeker_Strafe`, `166-266`), its hunt (`269-312`),
//! its shot (`Seeker_Fire`, `315-343`) and its thirty of them (`Seeker_Ranged`, `346-374`),
//! its fight (`Seeker_Attack`, `377-406`), the look for an enemy (`Seeker_FindEnemy`,
//! `409-466`) and the orbit about its owner (`Seeker_FollowOwner`, `469-536`).
//!
//! Boba Fett flies on this AI once his jets are lit ([`crate::npc_boba`]): he hovers higher and
//! harder, strafes farther and without the hiss, fires none of the seeker's shots but decides
//! his own (`Boba_FireDecide`), and circles his enemy rather than an owner.
//!
//! A seeker has an owner when a player's seeker item made it (`ItemUse_Seeker`,
//! `g_items.c:1133-1161`, siege with `d_siegeSeekerNPC`): [`NpcRoster::use_seeker`].

use crate::means_of_death::{MOD_BLASTER, MOD_FALLING, MOD_TELEFRAG, MOD_UNKNOWN};
use crate::npc_creature::CHAN_AUTO;
use crate::npc_jedi_patrol::{distance_horizontal_squared, normalized};
use crate::npc_machine::{
    ENTITYNUM_NONE, ES_OWNER, MASK_SOLID, MachineBolt, SCF_CHASE_ENEMIES, WP_BLASTER, damped,
};
use crate::npc_roster::{CommandPlace, NpcFiles, NpcRoster};
use crate::npc_senses::{Spot, spot};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use sjk_protocol::UserCommand;

/// `VELOCITY_DECAY`, `MIN_DISTANCE_SQR`, `SEEKER_STRAFE_VEL`, `SEEKER_STRAFE_DIS`,
/// `SEEKER_UPWARD_PUSH`, `SEEKER_FORWARD_BASE_SPEED`, `SEEKER_FORWARD_MULTIPLIER`,
/// `SEEKER_SEEK_RADIUS`.
const VELOCITY_DECAY: f32 = 0.7;
const MIN_DISTANCE_SQR: f32 = 80.0 * 80.0;
const STRAFE_VEL: f32 = 100.0;
const STRAFE_DIS: f32 = 200.0;
const UPWARD_PUSH: f32 = 32.0;
const FORWARD_BASE_SPEED: i32 = 10;
const FORWARD_MULTIPLIER: i32 = 2;
const SEEK_RADIUS: f32 = 1_024.0;
/// `NPCAI_CUSTOM_GRAVITY`; `DAMAGE_NO_PROTECTION`.
const NPCAI_CUSTOM_GRAVITY: u32 = 0x20_0000;
const DAMAGE_NO_PROTECTION: u32 = 0x8;
/// `CLASS_SEEKER`; `NPCTEAM_ENEMY`, `NPCTEAM_PLAYER`, `NPCTEAM_NEUTRAL`.
const CLASS_SEEKER: i32 = 41;
const NPCTEAM_ENEMY: i32 = 1;
const NPCTEAM_PLAYER: i32 = 2;
const NPCTEAM_NEUTRAL: i32 = 3;
/// `TEAM_RED`, `TEAM_BLUE`.
const TEAM_RED: i32 = 1;
const TEAM_BLUE: i32 = 2;
/// Its hiss.
const HISS: &[u8] = b"sound/chars/seeker/misc/hiss";

impl NpcRoster {
    /// `ItemUse_Seeker`'s NPC half (`g_items.c:1135-1154`): a `remote` spawned where `npc
    /// spawn` would put it for the player `owner` (`NPC_SpawnType`), owned by it and on the
    /// NPC team of its side — the players', the enemy's, or neutral. Returns what the spawn
    /// fired, and the seeker's number.
    pub fn use_seeker(
        &mut self,
        owner: u16,
        owner_team: i32,
        place: CommandPlace,
        level_time: i32,
        files: NpcFiles<'_>,
        host: &mut impl NpcHost,
    ) -> (crate::npc_roster::Fired, Option<u16>) {
        let before = self.actors.len();
        let fired = self.spawn_command(b"remote", b"", false, place, level_time, files, host);
        let Some(remote) = self.actors.get_mut(before) else {
            return (fired, None);
        };
        // `NPC_SpawnType(ent, "remote", NULL, qfalse)`: no name.
        remote.targetname = None;
        remote.state.set_raw_field(ES_OWNER, u32::from(owner));
        remote.mind.creature.activator = Some(owner);
        remote.player_team = match owner_team {
            TEAM_BLUE => NPCTEAM_PLAYER,
            TEAM_RED => NPCTEAM_ENEMY,
            _ => NPCTEAM_NEUTRAL,
        };
        (fired, Some(remote.number))
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `r.ownerNum`: an owned seeker's owner (`s.owner`, which `ItemUse_Seeker` and
    /// `NPC_Begin` set with it).
    fn seeker_owner(&self, me: usize) -> Option<u16> {
        let owner = self.actors[me]
            .state
            .raw_field(ES_OWNER)
            .unwrap_or(u32::from(ENTITYNUM_NONE)) as u16;
        (owner < ENTITYNUM_NONE).then_some(owner)
    }

    /// `NPC_BSSeeker_Default` (`539-597`).
    pub fn bs_seeker_default(&mut self, me: usize, command: &mut UserCommand) {
        if self.seeker_owner(me).is_some() {
            // "OJKFIXME: clientnum 0": the reference asks after the first client, whoever
            // owns the seeker.
            if self.body(0).is_none_or(|owner| owner.health <= 0) {
                self.machine_self_damage(
                    me,
                    None,
                    None,
                    10_000,
                    DAMAGE_NO_PROTECTION,
                    MOD_TELEFRAG,
                );
                return;
            }
        }
        if self.actors[me].mind.creature.machine.random == 0.0 {
            self.seeker_new_place(me);
        }
        let boba = self.is_boba(me);
        if let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
            .filter(|enemy| enemy.health != 0)
        {
            // "hacked to never take the player as an enemy, even if the player shoots at it"
            if !boba && (enemy.number == 0 || enemy.class == CLASS_SEEKER) {
                self.actors[me].mind.enemy = None;
            } else {
                self.seeker_attack(me, command);
                if boba {
                    self.boba_fire_decide(me, command);
                }
                return;
            }
        }
        self.seeker_follow_owner(me, command);
    }

    /// Whether the NPC at `me` is Boba Fett (`CLASS_BOBAFETT`), flying on this AI.
    fn is_boba(&self, me: usize) -> bool {
        self.actors[me].definition.client_class == crate::npc_boba::CLASS_BOBAFETT
    }

    /// `ent->random`: where about its owner it orbits, a draw up to "roughly 2 pi".
    fn seeker_new_place(&mut self, me: usize) {
        let random = self.host.rng().flrand(0.0, 1.0) * 6.3;
        self.actors[me].mind.creature.machine.random = random;
    }

    /// `NPC_Seeker_Pain` (`53-66`): one that falls is broken (the reference's seekers never
    /// fall: `NPC_Begin` gives them their own gravity); a strafe, then the pain.
    pub(crate) fn seeker_pain(
        &mut self,
        me: usize,
        attacker: Option<u16>,
        damage: i32,
        means: u32,
    ) {
        if self.actors[me].ai_flags & NPCAI_CUSTOM_GRAVITY == 0 {
            self.machine_self_damage(me, None, Some(([0.0; 3], [0.0; 3])), 999, 0, MOD_FALLING);
        }
        self.seeker_strafe(me);
        self.npc_pain(me, attacker, damage, means);
    }

    /// `Seeker_MaintainHeight` (`69-163`): its angles; now and then a hover to a little below
    /// its enemy's eyes, or to its goal's height; its drift damped.
    fn seeker_maintain_height(&mut self, me: usize, command: &mut UserCommand) {
        self.update_angles(me, true, true, command);
        let level_time = self.level_time;
        let origin = self.actors[me].current_origin;
        let mut velocity = self.actors[me].player.velocity();
        if let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        {
            if self.actors[me].mind.timers.done("heightChange", level_time) {
                let delay = self.host.irand(1_000, 3_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("heightChange", level_time, delay);
                let above = self
                    .host
                    .rng()
                    .flrand(enemy.maxs[2] / 2.0, enemy.maxs[2] + 8.0);
                let mut dif = (enemy.origin[2] + above) - origin[2];
                // Boba Fett, not flaming, ten times as far.
                let boba = self.is_boba(me);
                let factor: f32 =
                    if boba && self.actors[me].mind.timers.done("flameTime", level_time) {
                        10.0
                    } else {
                        1.0
                    };
                if f64::from(dif).abs() > f64::from(2.0 * factor) {
                    if f64::from(dif).abs() > f64::from(24.0 * factor) {
                        dif = if dif < 0.0 {
                            -24.0 * factor
                        } else {
                            24.0 * factor
                        };
                    }
                    velocity[2] = (velocity[2] + dif) / 2.0;
                }
                if boba {
                    velocity[2] *= self.host.rng().flrand(0.85, 3.0);
                }
            }
        } else if let Some(goal) = self.goal_origin(me) {
            // No `lastGoalEntity` is kept: the goal alone.
            if (goal[2] - origin[2]).abs() > 24.0 {
                command.up_move = if command.up_move < 0 { -4 } else { 4 };
            } else {
                velocity[2] = damped(velocity[2], VELOCITY_DECAY, 2.0);
            }
        }
        velocity[0] = damped(velocity[0], VELOCITY_DECAY, 1.0);
        velocity[1] = damped(velocity[1], VELOCITY_DECAY, 1.0);
        self.actors[me].player.set_velocity(velocity);
    }

    /// `Seeker_Strafe` (`166-266`): mostly to the side of its enemy's view, a little ahead
    /// or behind; else (three times in ten, or with no enemy) a plain strafe about itself.
    fn seeker_strafe(&mut self, me: usize) {
        let plain = self.host.rng().flrand(0.0, 1.0) > 0.7;
        let enemy = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy));
        let npc = &self.actors[me];
        let (origin, number, level_time) = (npc.current_origin, npc.number, self.level_time);
        let Some(enemy) = enemy.filter(|_| !plain) else {
            let right = crate::pmove::flight::flight_axes(npc.mind.eye_angles)
                .1
                .to_array();
            let side = if self.level.crt.next() & 1 != 0 {
                -1.0
            } else {
                1.0
            };
            let end: [f32; 3] =
                std::array::from_fn(|axis| origin[axis] + STRAFE_DIS * side * right[axis]);
            let trace = self.trace_bodies(origin, [0.0; 3], [0.0; 3], end, number, MASK_SOLID);
            if trace.fraction > 0.9 {
                // Boba Fett's jets: three times the strafe, four times the lift, no hiss.
                let (vel, up) = if self.is_boba(me) {
                    (STRAFE_VEL * 3.0, UPWARD_PUSH * 4.0)
                } else {
                    (STRAFE_VEL, UPWARD_PUSH)
                };
                if !self.is_boba(me) {
                    self.creature_sound(number, CHAN_AUTO, HISS);
                }
                let velocity = self.actors[me].player.velocity();
                let mut velocity: [f32; 3] =
                    std::array::from_fn(|axis| velocity[axis] + vel * side * right[axis]);
                velocity[2] += up;
                self.actors[me].player.set_velocity(velocity);
                self.actors[me].mind.stand_time =
                    ((level_time + 1_000) as f32 + self.host.rng().flrand(0.0, 1.0) * 500.0) as i32;
            }
            return;
        };
        let (dir, right) = crate::pmove::flight::flight_axes(enemy.eye_angles);
        let (dir, right) = (dir.to_array(), right.to_array());
        let side = if self.level.crt.next() & 1 != 0 {
            -1.0
        } else {
            1.0
        };
        let reach = if self.is_boba(me) {
            STRAFE_DIS * 2.0
        } else {
            STRAFE_DIS
        };
        let mut end: [f32; 3] =
            std::array::from_fn(|axis| enemy.origin[axis] + reach * side * right[axis]);
        let ahead = self.host.rng().flrand(-1.0, 1.0) * 25.0;
        end = std::array::from_fn(|axis| end[axis] + ahead * dir[axis]);
        let trace = self.trace_bodies(origin, [0.0; 3], [0.0; 3], end, number, MASK_SOLID);
        if trace.fraction > 0.9 {
            let mut way = crate::npc_senses::subtract(trace.end_position, origin);
            way[2] = (f64::from(way[2]) * 0.25) as f32;
            let (way, distance) = normalized(way);
            let velocity = self.actors[me].player.velocity();
            let mut velocity: [f32; 3] =
                std::array::from_fn(|axis| velocity[axis] + distance * way[axis]);
            let up = if self.is_boba(me) {
                UPWARD_PUSH * 4.0
            } else {
                UPWARD_PUSH
            };
            if !self.is_boba(me) {
                self.creature_sound(number, CHAN_AUTO, HISS);
            }
            velocity[2] += up;
            self.actors[me].player.set_velocity(velocity);
            self.actors[me].mind.stand_time =
                ((level_time + 2_500) as f32 + self.host.rng().flrand(0.0, 1.0) * 500.0) as i32;
        }
    }

    /// `Seeker_Hunt(visible, advance)` (`269-312`).
    fn seeker_hunt(&mut self, me: usize, visible: bool, advance: bool, command: &mut UserCommand) {
        self.face_enemy(me, true, command);
        if self.actors[me].mind.stand_time < self.level_time && visible {
            self.seeker_strafe(me);
            return;
        }
        if !advance {
            return;
        }
        if !visible {
            if let Some(forward) = self.machine_seek_unseen(me, 24, command) {
                let speed = (FORWARD_BASE_SPEED + FORWARD_MULTIPLIER * self.host.skill()) as f32;
                self.machine_push(me, forward, speed);
            }
            return;
        }
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        else {
            return;
        };
        let speed = (FORWARD_BASE_SPEED + FORWARD_MULTIPLIER * self.host.skill()) as f32;
        self.machine_advance(me, enemy.origin, speed);
    }

    /// `Seeker_Fire` (`315-343`): a blaster bolt at its enemy's head from a little ahead of
    /// itself, its owner's (if it has one) to answer for.
    fn seeker_fire(&mut self, me: usize) {
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        else {
            return;
        };
        let origin = self.actors[me].current_origin;
        let (dir, _) = normalized(crate::npc_senses::subtract(
            spot(&enemy, Spot::Head),
            origin,
        ));
        let muzzle: [f32; 3] = std::array::from_fn(|axis| origin[axis] + 15.0 * dir[axis]);
        let owner = self.seeker_owner(me).unwrap_or(self.actors[me].number);
        self.machine_missile(
            me,
            muzzle,
            dir,
            MachineBolt {
                weapon: WP_BLASTER,
                damage: 5,
                means: MOD_BLASTER,
                speed: 1_000.0,
            },
            owner,
        );
        self.play_effect_at(b"blaster/muzzle_flash", origin, dir);
    }

    /// `Seeker_Ranged(visible, advance)` (`346-374`): a shot now and then while it has any
    /// left; out of them, it breaks itself.
    fn seeker_ranged(
        &mut self,
        me: usize,
        visible: bool,
        advance: bool,
        command: &mut UserCommand,
    ) {
        let level_time = self.level_time;
        if self.is_boba(me) {
            // Boba Fett fires as he decides (`Boba_FireDecide`), not the seeker's shots.
        } else if self.actors[me].count > 0 {
            if self.actors[me].mind.timers.done("attackDelay", level_time) {
                let delay = self.host.irand(250, 2_500);
                self.actors[me]
                    .mind
                    .timers
                    .set("attackDelay", level_time, delay);
                self.seeker_fire(me);
                self.actors[me].count -= 1;
            }
        } else {
            let number = self.actors[me].number;
            self.machine_self_damage(me, Some(number), None, 999, 0, MOD_UNKNOWN);
        }
        if self.actors[me].script_flags & SCF_CHASE_ENEMIES != 0 {
            self.seeker_hunt(me, visible, advance, command);
        }
    }

    /// `Seeker_Attack` (`377-406`).
    fn seeker_attack(&mut self, me: usize, command: &mut UserCommand) {
        self.seeker_maintain_height(me, command);
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        else {
            return;
        };
        let distance = distance_horizontal_squared(self.actors[me].current_origin, enemy.origin);
        let visible = self.clear_los4(me, &enemy);
        let advance = if self.is_boba(me) {
            distance > 200.0 * 200.0
        } else {
            distance > MIN_DISTANCE_SQR
        };
        if !visible && self.actors[me].script_flags & SCF_CHASE_ENEMIES != 0 {
            self.seeker_hunt(me, visible, advance, command);
            return;
        }
        self.seeker_ranged(me, visible, advance, command);
    }

    /// `Seeker_FindEnemy` (`409-466`): the nearest living client in sight, of another team and
    /// not neutral, among those in the box of `SEEKER_SEEK_RADIUS` about the world's origin
    /// (the reference's box is not moved to the seeker).
    fn seeker_find_enemy(&mut self, me: usize) {
        let npc = self.npc(me);
        let (mins, maxs) = ([-SEEK_RADIUS; 3], [SEEK_RADIUS; 3]);
        let mut candidates = Vec::new();
        self.each_body(|body| {
            let absmin: [f32; 3] =
                std::array::from_fn(|axis| body.origin[axis] + body.mins[axis] - 1.0);
            let absmax: [f32; 3] =
                std::array::from_fn(|axis| body.origin[axis] + body.maxs[axis] + 1.0);
            if (0..3).all(|axis| absmin[axis] <= maxs[axis] && absmax[axis] >= mins[axis]) {
                candidates.push(body);
            }
        });
        let mut best = None;
        let mut best_distance = SEEK_RADIUS * SEEK_RADIUS + 1.0;
        for body in candidates {
            if body.number == npc.number
                || body.health <= 0
                || body.player_team == npc.player_team
                || body.player_team == NPCTEAM_NEUTRAL
            {
                continue;
            }
            if !self.clear_los4(me, &body) {
                continue;
            }
            let distance = distance_horizontal_squared(npc.origin, body.origin);
            if distance <= best_distance {
                best_distance = distance;
                best = Some(body.number);
            }
        }
        if best.is_some() {
            self.seeker_new_place(me);
            self.actors[me].mind.enemy = best;
        }
    }

    /// `Seeker_FollowOwner` (`469-536`): close to its owner it orbits it, else it goes back to
    /// it hissing; and twice a second a look for an enemy.
    fn seeker_follow_owner(&mut self, me: usize, command: &mut UserCommand) {
        self.seeker_maintain_height(me, command);
        let boba = self.is_boba(me);
        // Boba Fett circles his enemy.
        let owner_number = if boba {
            self.actors[me].mind.enemy
        } else {
            Some(self.actors[me].state.raw_field(ES_OWNER).unwrap_or(0) as u16)
        };
        let Some(owner) = owner_number
            .and_then(|number| self.body(number))
            .filter(|owner| owner.number != self.actors[me].number)
        else {
            return;
        };
        let level_time = self.level_time;
        let origin = self.actors[me].current_origin;
        let near = if boba && self.actors[me].mind.timers.done("flameTime", level_time) {
            200.0 * 200.0
        } else {
            MIN_DISTANCE_SQR
        };
        if distance_horizontal_squared(origin, owner.origin) < near {
            // "generally circle the player closely till we take an enemy"
            let turn =
                f64::from(level_time as f32 * 0.001 + self.actors[me].mind.creature.machine.random);
            let (radius, height) = if !boba {
                (56.0, owner.origin[2] + 40.0)
            } else if self.actors[me].mind.jet_pack_time < level_time {
                (250.0, origin[2] - 64.0)
            } else {
                (250.0, owner.origin[2] + 200.0)
            };
            let point = [
                (f64::from(owner.origin[0]) + turn.cos() * radius) as f32,
                (f64::from(owner.origin[1]) + turn.sin() * radius) as f32,
                height,
            ];
            let way = crate::npc_senses::subtract(point, origin);
            let velocity = self.actors[me].player.velocity();
            self.actors[me]
                .player
                .set_velocity(std::array::from_fn(|axis| velocity[axis] + 0.8 * way[axis]));
        } else {
            if !boba && self.actors[me].mind.timers.done("seekerhiss", level_time) {
                let delay = (1_000.0 + self.host.rng().flrand(0.0, 1.0) * 1_000.0) as i32;
                self.actors[me]
                    .mind
                    .timers
                    .set("seekerhiss", level_time, delay);
                let number = self.actors[me].number;
                self.creature_sound(number, CHAN_AUTO, HISS);
            }
            // "Hey come back!"
            self.actors[me].mind.goal = Some(owner.number);
            self.actors[me].mind.tactics.goal_radius = 32;
            self.move_to_goal(me, true, command);
        }
        if self.actors[me].mind.tactics.enemy_check_debounce_time < level_time {
            // "check twice a second to find a new enemy"
            self.seeker_find_enemy(me);
            self.actors[me].mind.tactics.enemy_check_debounce_time = level_time + 500;
        }
        self.update_angles(me, true, true, command);
    }
}
