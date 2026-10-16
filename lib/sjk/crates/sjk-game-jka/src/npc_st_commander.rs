//! A stormtrooper squad's decisions (`ST_Commander`, `NPC_AI_Stormtrooper.c:1742-2430`),
//! made once a frame by the first of its members to think: who runs to which combat
//! point, who scouts after an enemy lost or out of reach, who covers, ducks or stands
//! (`ST_GetCPFlags`, `ST_TrackEnemy`, `ST_ApproachEnemy`, `ST_HuntEnemy`,
//! `ST_TransferMoveGoal`, `ST_TransferTimers`), and a squad that has not seen its enemy for
//! three minutes broken up.
//!
//! The reference makes each member the thinking NPC in turn (`SetNPCGlobals`, its command
//! cleared and put back after): here each member is named by its index, and what the
//! commander decides for it never touches the thinking NPC's command.
//!
//! Without navigation data a broken-up squad's members go back to their default state
//! (their waypoint is `WAYPOINT_NONE`, `NPC_BSSearchStart` is not reached).
//!
//! Held to `tools/game-oracle/npcst.c` (`game-npcst.txt`).

use crate::npc_combat_points::{CPF_DUCK, PointSearch, cp};
use crate::npc_groups::RANK_ENSIGN;
use crate::npc_senses::{AEL_DANGER, distance_squared};
use crate::npc_spawn::NpcHost;
use crate::npc_st::{
    CLASS_IMPERIAL, LSTATE_NONE, LSTATE_UNDERFIRE, SCF_CHASE_ENEMIES, SCF_USE_CP_NEAREST, speech,
    squad,
};
use crate::npc_world::NpcWorld;

/// `MIN_ROCKET_DIST_SQUARED`; `Q3_INFINITE`.
const MIN_ROCKET_DIST_SQUARED: f32 = 16_384.0;
const Q3_INFINITE: i32 = 16_777_216;
/// `WP_NONE`, `WP_SABER`, `WP_ROCKET_LAUNCHER`.
const WP_NONE: u8 = 0;
const WP_SABER: i32 = 3;
const WP_ROCKET_LAUNCHER: u8 = 11;
/// `BS_DEFAULT`.
const BS_DEFAULT: i32 = 0;
/// `NPCAI_BLOCKED`.
const NPCAI_BLOCKED: u32 = crate::npc_nav::NPCAI_BLOCKED;

/// The order `ST_Commander` gives up what a member wants of a combat point when none has
/// it all (`NPC_AI_Stormtrooper.c:2259-2330`): each flag dropped, some with another taken
/// on instead.
const GIVE_UP: [(i32, i32); 15] = [
    (cp::INVESTIGATE, 0),
    (cp::SQUAD, 0),
    (cp::DUCK, 0),
    (cp::NEAREST, 0),
    (cp::FLANK, 0),
    (cp::SAFE, 0),
    (cp::CLOSEST, cp::APPROACH_ENEMY),
    (cp::APPROACH_ENEMY, 0),
    (cp::COVER, cp::DUCK),
    (cp::CLEAR, 0),
    (cp::AVOID_ENEMY, 0),
    (cp::RETREAT, 0),
    (cp::FLEE, cp::COVER | cp::AVOID_ENEMY),
    (cp::AVOID, 0),
    (0, 0),
];

/// What `ST_Commander` wants for one member: the combat point flags, a point already
/// chosen, the squad state to take, and how far to keep off.
#[derive(Clone, Copy, Debug)]
struct Orders {
    flags: i32,
    point: i32,
    state: i32,
    avoid: f32,
}

/// What the whole squad is doing this frame.
#[derive(Clone, Copy, Debug)]
struct SquadView {
    group: usize,
    enemy: crate::npc_senses::Body,
    runner: bool,
    lost: bool,
    protected: bool,
    officer: bool,
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `ST_Commander` (`NPC_AI_Stormtrooper.c:1742-2430`) for the group of the NPC at `me`.
    pub fn st_commander(&mut self, me: usize) {
        let level_time = self.level_time;
        let Some(group) = self.actors[me].mind.tactics.group else {
            return;
        };
        self.level.groups[group].processed = true;
        let Some(enemy) = self.level.groups[group]
            .enemy
            .and_then(|enemy| self.body(enemy))
        else {
            return;
        };
        let slot = &self.level.groups[group];
        if slot.last_seen_enemy_time < level_time - 180_000 {
            self.dissolve(me, group);
            return;
        }
        let runner = slot.num_state[squad::SCOUT as usize] > 0
            || slot.num_state[squad::TRANSITION as usize] > 0
            || slot.num_state[squad::RETREAT as usize] > 0;
        if slot.last_seen_enemy_time > level_time - 32_000
            && slot.last_seen_enemy_time < level_time - 30_000
        {
            let commander = slot
                .commander
                .and_then(|commander| self.actor_at(commander));
            let speaker = if commander.is_some() && self.host.irand(0, 1) == 0 {
                commander.unwrap_or(me)
            } else {
                me
            };
            self.st_speech(speaker, speech::ESCAPING, 0.0);
            self.actors[me].mind.blocked_speech_until = level_time + 3_000;
        }
        let slot = &self.level.groups[group];
        let officer = slot.commander.is_some_and(|commander| {
            self.actor_at(commander)
                .is_some_and(|at| self.actors[at].definition.rank >= RANK_ENSIGN)
        });
        let mut view = SquadView {
            group,
            enemy,
            runner,
            lost: slot.last_seen_enemy_time < level_time - 10_000,
            protected: slot.last_clear_shot_time < level_time - 5_000,
            officer,
        };
        let last = self.level.groups[group].len();
        for index in 0..last {
            let Some(number) = self.level.groups[group]
                .members()
                .get(index)
                .map(|member| member.number)
            else {
                break;
            };
            let Some(member) = self.actor_at(number) else {
                continue;
            };
            if self.actors[member].mind.enemy.is_none() {
                continue;
            }
            self.command_member(member, index, &mut view);
        }
    }

    /// `ST_Commander`'s squad broken up (`NPC_AI_Stormtrooper.c:1787-1822`): the enemy
    /// unseen for three minutes, its waypoint looked up; the members that may chase forget
    /// it and search from the enemy's waypoint where they have a route to it, else from
    /// their own — or, with no waypoint, go back to their default state.
    fn dissolve(&mut self, me: usize, group: usize) {
        use crate::npc_mind::WAYPOINT_NONE;
        self.st_speech(me, speech::LOST, 0.0);
        let enemy = self.level.groups[group]
            .enemy
            .and_then(|number| self.nav_holder_of(number));
        if let Some(holder) = enemy {
            let waypoint = self.closest_waypoint_for(holder, WAYPOINT_NONE);
            self.set_nav_waypoint(holder, waypoint);
        }
        for index in 0..self.level.groups[group].len() {
            let number = self.level.groups[group].members()[index].number;
            let Some(member) = self.actor_at(number) else {
                continue;
            };
            if self.actors[member].script_flags & SCF_CHASE_ENEMIES == 0 {
                continue;
            }
            self.clear_enemy(member);
            let enemy_waypoint = enemy.map_or(WAYPOINT_NONE, |holder| {
                self.nav_entity(holder).state.waypoint
            });
            let waypoint = self.closest_waypoint_for(
                crate::npc_navigator::NavHolder::Actor(member),
                enemy_waypoint,
            );
            self.actors[member].mind.tactics.waypoint = waypoint;
            if waypoint == WAYPOINT_NONE {
                self.actors[member].behavior_state = BS_DEFAULT;
            } else if enemy_waypoint == WAYPOINT_NONE
                || self
                    .level
                    .navigator
                    .graph
                    .path_cost(waypoint, enemy_waypoint)
                    >= crate::npc_navigator::Q3_INFINITE
            {
                self.search_start(member, waypoint, crate::npc_nav_route::BS_SEARCH);
            } else {
                self.search_start(member, enemy_waypoint, crate::npc_nav_route::BS_SEARCH);
            }
        }
        self.level.groups[group].enemy = None;
    }

    /// `ST_Commander`'s orders for one member (`NPC_AI_Stormtrooper.c:1887-2428`).
    fn command_member(&mut self, member: usize, index: usize, view: &mut SquadView) {
        let level_time = self.level_time;
        let timers = &self.actors[member].mind.timers;
        if !timers.done("flee", level_time) {
            return;
        }
        // "running to pick up a gun, don't do other logic" (`1928-1934`).
        if self.npc(member).weapon == 0 && self.running_for_item(member) {
            return;
        }
        if !view.officer {
            let alert = self.check_alerts(member, -1, false, AEL_DANGER);
            if self.check_for_danger(member, alert) {
                self.st_speech(member, speech::COVER, 0.0);
                return;
            }
        }
        if self.actors[member].script_flags & SCF_CHASE_ENEMIES == 0 {
            return;
        }
        let mut orders = Orders {
            flags: 0,
            point: -1,
            state: squad::IDLE,
            avoid: 0.0,
        };
        if self.actors[member].mind.tactics.squad_state != squad::RETREAT {
            if self.actors[member].player.weapon() == WP_NONE {
                self.hide_unarmed(member, view);
                return;
            }
            self.threat_orders(member, view, &mut orders);
        }
        if orders.flags == 0 && !self.tactic_orders(member, index, view, &mut orders) {
            return;
        }
        self.actors[member].mind.fight.local_state = LSTATE_NONE;
        if self.actors[member].script_flags & SCF_USE_CP_NEAREST != 0 {
            orders.flags =
                (orders.flags & !(cp::FLANK | cp::APPROACH_ENEMY | cp::CLOSEST)) | cp::NEAREST;
        }
        if orders.flags != 0 {
            self.assign_point(member, view, orders);
        }
    }

    /// An unarmed member keeps hiding, and flees again when its hiding is over or the enemy
    /// comes near and sees it (`NPC_AI_Stormtrooper.c:1942-1953`).
    fn hide_unarmed(&mut self, member: usize, view: &SquadView) {
        // "not running after a pickup" (`1957`).
        if self.running_for_item(member) {
            return;
        }
        let level_time = self.level_time;
        let npc = &self.actors[member];
        let near = distance_squared(view.enemy.origin, npc.current_origin) < 65_536.0;
        let own_enemy = npc.mind.enemy.and_then(|enemy| self.body(enemy));
        let hiding_over = npc.mind.timers.done("hideTime", level_time);
        if hiding_over || (near && own_enemy.is_some_and(|enemy| self.clear_los4(member, &enemy))) {
            let enemy = own_enemy.map(|enemy| (enemy.number, enemy.origin));
            self.start_flee(
                member,
                enemy.map(|(number, _)| number),
                enemy.map_or([0.0; 3], |(_, origin)| origin),
                AEL_DANGER + 1,
                5_000,
                10_000,
            );
        }
    }

    /// The enemy-driven reasons to run (`NPC_AI_Stormtrooper.c:1954-2034`): out of sight
    /// of it, under fire, or too near a saber or its own rocket's blast.
    fn threat_orders(&mut self, member: usize, view: &SquadView, orders: &mut Orders) {
        let level_time = self.level_time;
        let npc = &self.actors[member];
        let origin = npc.current_origin;
        let near = distance_squared(view.enemy.origin, origin) < 65_536.0;
        let lit = view.enemy.weapon == WP_SABER && !view.enemy.saber_holstered;
        let timers = &npc.mind.timers;
        if timers.done("roamTime", level_time)
            && timers.done("hideTime", level_time)
            && npc.health > 10
            && !self.host.in_pvs(view.enemy.origin, origin)
        {
            orders.flags |= cp::CLEAR | cp::COVER;
        } else if npc.mind.fight.local_state == LSTATE_UNDERFIRE {
            if view.enemy.weapon == WP_SABER {
                if near {
                    orders.flags |= cp::AVOID_ENEMY | cp::COVER | cp::AVOID | cp::RETREAT;
                    if !view.officer {
                        orders.state = squad::RETREAT;
                    }
                    orders.avoid = 256.0;
                }
            } else {
                orders.flags |= cp::COVER;
            }
            if npc.health <= 10 && !view.officer {
                orders.flags |= cp::FLEE | cp::AVOID | cp::RETREAT;
                orders.state = squad::RETREAT;
            }
        } else if self.host.in_pvs(origin, view.enemy.origin) {
            let transition = npc.mind.tactics.squad_state == squad::TRANSITION;
            if npc.player.weapon() == WP_ROCKET_LAUNCHER
                && distance_squared(view.enemy.origin, origin) < MIN_ROCKET_DIST_SQUARED
                && !transition
            {
                orders.flags |= cp::AVOID_ENEMY | cp::CLEAR | cp::AVOID;
                orders.avoid = 256.0;
            } else if lit && near && timers.done("hideTime", level_time) && !transition {
                orders.flags |= cp::AVOID_ENEMY | cp::CLEAR | cp::AVOID;
                orders.avoid = 256.0;
            }
        }
    }

    /// The tactics when nothing drives the member to run (`NPC_AI_Stormtrooper.c:2036-2210`).
    /// Whether the member's orders go on to a combat point (`false`: it is running already
    /// and done with).
    fn tactic_orders(
        &mut self,
        member: usize,
        index: usize,
        view: &mut SquadView,
        orders: &mut Orders,
    ) -> bool {
        let level_time = self.level_time;
        let tactics = self.actors[member].mind.tactics;
        let running_self = matches!(
            tactics.squad_state,
            squad::SCOUT | squad::TRANSITION | squad::RETREAT
        );
        let off_point = |world: &Self| {
            tactics.combat_point >= 0
                && distance_squared(
                    world.actors[member].current_origin,
                    world.level.combat_points[tactics.combat_point as usize].origin,
                ) > 64.0 * 64.0
        };
        if view.runner && tactics.combat_point != -1 {
            if running_self {
                if self.actors[member].ai_flags & NPCAI_BLOCKED != 0 {
                    let blocking = tactics.blocking_ent_num;
                    let found = self.level.groups[view.group]
                        .members()
                        .iter()
                        .find(|m| i32::from(m.number) == blocking)
                        .map(|m| m.number);
                    if let Some(other) = found.and_then(|number| self.actor_at(number)) {
                        self.transfer_move_goal(member, other);
                    }
                }
                return false;
            }
            if self.actors[member].mind.timers.done("verifyCP", level_time) && off_point(self) {
                orders.point = tactics.combat_point;
                orders.flags |= self.cp_flags(member);
            } else {
                let timers = &mut self.actors[member].mind.timers;
                timers.set("duck", level_time, -1);
                timers.set("attackDelay", level_time, -1);
            }
            return true;
        }
        if tactics.combat_point != -1
            && !running_self
            && self.actors[member].mind.timers.done("verifyCP", level_time)
            && off_point(self)
        {
            orders.point = tactics.combat_point;
            orders.flags |= self.cp_flags(member);
        }
        if view.lost {
            if self.level.groups[view.group].num_state[squad::SCOUT as usize] <= 0 {
                self.store_movement_speech(member, speech::CHASE, 0.0);
            }
            let spot = self.level.groups[view.group].enemy_last_seen_pos;
            self.track_enemy(member, spot);
            self.update_squad_state(Some(view.group), member, squad::SCOUT);
            view.runner = true;
        } else if view.protected {
            let count = self.level.groups[view.group].len() as i32;
            if self.actors[member].mind.timers.done("roamTime", level_time)
                && self.host.irand(0, count) == 0
            {
                orders.flags |= self.approach_enemy(member);
                self.update_squad_state(Some(view.group), member, squad::SCOUT);
            }
        } else {
            self.engaged_orders(member, index, view, orders);
        }
        true
    }

    /// A squad that sees and shoots at its enemy (`NPC_AI_Stormtrooper.c:2113-2208`): off
    /// to a combat point if not on one; on one, the nearest member may flank or take point,
    /// the farthest move in, the rest now and then move; one standing may duck.
    fn engaged_orders(
        &mut self,
        member: usize,
        index: usize,
        view: &SquadView,
        orders: &mut Orders,
    ) {
        let level_time = self.level_time;
        let slot = &self.level.groups[view.group];
        let (morale, count) = (slot.morale, slot.len() as i32);
        let spare = morale - count;
        if self.actors[member].mind.tactics.combat_point == -1 {
            orders.flags |= self.cp_flags(member);
        } else if self.actors[member].mind.timers.done("roamTime", level_time) {
            if index == 0 {
                if spare > 0 && self.host.irand(0, 4) == 0 {
                    orders.flags |= cp::CLEAR | cp::COVER | cp::FLANK | cp::APPROACH_ENEMY;
                } else if spare < 0 {
                    orders.flags |= self.cp_flags(member);
                } else {
                    let roam = self.host.irand(2_000, 5_000);
                    let stick = self.host.irand(2_000, 5_000);
                    let duck = self.host.irand(3_000, 4_000);
                    let timers = &mut self.actors[member].mind.timers;
                    timers.set("roamTime", level_time, roam);
                    timers.set("stick", level_time, stick);
                    timers.set("duck", level_time, duck);
                    self.update_squad_state(Some(view.group), member, squad::POINT);
                }
            } else if index as i32 == count - 1 {
                if spare < 0 {
                    let roam = self.host.irand(2_000, 5_000);
                    let stick = self.host.irand(2_000, 5_000);
                    self.actors[member]
                        .mind
                        .timers
                        .set("roamTime", level_time, roam);
                    self.actors[member]
                        .mind
                        .timers
                        .set("stick", level_time, stick);
                } else if spare > 0 {
                    orders.flags |= self.approach_enemy(member);
                    self.update_squad_state(Some(view.group), member, squad::SCOUT);
                } else {
                    orders.flags |= self.cp_flags(member);
                }
            } else if spare < 0 || self.host.irand(0, 4) == 0 {
                orders.flags |= self.cp_flags(member);
            } else {
                let stick = self.host.irand(2_000, 4_000);
                let roam = self.host.irand(2_000, 4_000);
                self.actors[member]
                    .mind
                    .timers
                    .set("stick", level_time, stick);
                self.actors[member]
                    .mind
                    .timers
                    .set("roamTime", level_time, roam);
            }
        }
        if orders.flags != 0 {
            return;
        }
        let npc = &self.actors[member];
        let point = npc.mind.tactics.combat_point;
        let may_duck = point == -1
            || self
                .level
                .combat_points
                .get(point as usize)
                .is_some_and(|spot| spot.flags & CPF_DUCK != 0);
        if npc.mind.timers.done("duck", level_time)
            && npc.mind.timers.done("stand", level_time)
            && may_duck
            && self.host.irand(0, 3) == 0
        {
            let duck = self.host.irand(1_000, 3_000);
            self.actors[member]
                .mind
                .timers
                .set("duck", level_time, duck);
        }
    }

    /// `ST_Commander`'s combat point for a member that wants one (`NPC_AI_Stormtrooper.c:
    /// 2217-2427`): found by its flags, given up one by one until any will do, then set as
    /// its goal with the squad state and what it will say as it moves; a scout that finds
    /// none goes straight at the enemy.
    fn assign_point(&mut self, member: usize, view: &mut SquadView, mut orders: Orders) {
        let level_time = self.level_time;
        if view.enemy.weapon == WP_SABER && !view.enemy.saber_holstered {
            orders.flags |= cp::AVOID_ENEMY;
            orders.avoid = 256.0;
        }
        let origin = self.actors[member].current_origin;
        let search = |flags: i32, ignore: i32| PointSearch {
            position: origin,
            enemy: view.enemy.origin,
            flags: flags | cp::HAS_ROUTE,
            avoid_distance: orders.avoid,
            ignore,
        };
        let mut point = orders.point;
        if point == -1 {
            let failed = self.actors[member].mind.tactics.last_failed_combat_point;
            point = self.find_combat_point(member, search(orders.flags, failed));
        }
        while point == -1 && orders.flags != cp::ANY {
            let (drop, take) = GIVE_UP
                .iter()
                .copied()
                .find(|(flag, _)| *flag == 0 || orders.flags & flag != 0)
                .unwrap_or((0, 0));
            orders.flags = if drop == 0 {
                cp::ANY
            } else {
                (orders.flags & !drop) | take
            };
            point = self.find_combat_point(member, search(orders.flags, -1));
        }
        if point == -1 {
            if self.actors[member].mind.tactics.squad_state == squad::SCOUT {
                self.hunt_enemy(member);
                self.update_squad_state(Some(view.group), member, squad::SCOUT);
            }
            return;
        }
        view.runner = true;
        let verify = self.host.irand(1_000, 3_000);
        let timers = &mut self.actors[member].mind.timers;
        timers.set("roamTime", level_time, Q3_INFINITE);
        timers.set("verifyCP", level_time, verify);
        self.set_combat_point(member, point);
        let spot = self.level.combat_points[point as usize].origin;
        self.set_move_goal(member, spot, 8, true, point, None);
        let state = if orders.state != squad::IDLE {
            orders.state
        } else if orders.flags & cp::FLEE != 0 {
            squad::RETREAT
        } else {
            squad::TRANSITION
        };
        self.update_squad_state(Some(view.group), member, state);
        let count = self.level.groups[view.group].len();
        if orders.flags & cp::FLANK != 0 {
            if count > 1 {
                self.store_movement_speech(member, speech::OUTFLANK, -1.0);
            }
        } else if count > 1 {
            let mut dot = 1.0_f32;
            if self.host.irand(0, 3) == 0 {
                let held = self.actors[member].mind.tactics.combat_point;
                let target = self
                    .level
                    .combat_points
                    .get(held as usize)
                    .map_or([0.0; 3], |spot| spot.origin);
                let mut to_me = crate::npc_senses::subtract(origin, view.enemy.origin);
                let mut to_point = crate::npc_senses::subtract(target, view.enemy.origin);
                crate::player_angle_math::normalize(&mut to_me);
                crate::player_angle_math::normalize(&mut to_point);
                dot = to_me[0] * to_point[0] + to_me[1] * to_point[1] + to_me[2] * to_point[2];
            }
            if f64::from(dot) < 0.4 {
                self.store_movement_speech(member, speech::OUTFLANK, -1.0);
            } else if self.host.irand(0, 10) == 0 {
                self.store_movement_speech(member, speech::YELL, 0.2);
            }
        }
    }

    /// `NPC_ST_StoreMovementSpeech` (`NPC_AI_Stormtrooper.c:352-356`).
    fn store_movement_speech(&mut self, at: usize, kind: i32, chance: f32) {
        let tactics = &mut self.actors[at].mind.tactics;
        tactics.movement_speech = kind;
        tactics.movement_speech_chance = chance;
    }

    /// `ST_GetCPFlags` (`NPC_AI_Stormtrooper.c:1667-1740`): what a member wants of a combat
    /// point by its squad's morale — an imperial commander hangs back and gives orders.
    fn cp_flags(&mut self, member: usize) -> i32 {
        let mut flags = 0;
        if let Some(group) = self.actors[member].mind.tactics.group {
            let slot = &self.level.groups[group];
            let (morale, count, commander) = (slot.morale, slot.len() as i32, slot.commander);
            let npc = &self.actors[member];
            if commander == Some(npc.number) && npc.definition.client_class == CLASS_IMPERIAL {
                if count > 1 && self.host.irand(-3, count) > 1 {
                    let kind = if self.host.irand(0, 1) != 0 {
                        speech::CHASE
                    } else {
                        speech::YELL
                    };
                    self.st_speech(member, kind, 0.5);
                }
                flags = cp::CLEAR | cp::COVER | cp::AVOID | cp::SAFE | cp::RETREAT;
            } else if morale < 0 {
                flags = cp::COVER | cp::AVOID | cp::SAFE | cp::RETREAT;
            } else if morale >= count {
                let boost = morale - count;
                flags = if boost > 20 {
                    cp::CLEAR | cp::FLANK | cp::APPROACH_ENEMY
                } else if boost > 15 {
                    cp::CLEAR | cp::CLOSEST | cp::APPROACH_ENEMY
                } else if boost > 10 {
                    cp::CLEAR | cp::APPROACH_ENEMY
                } else {
                    0
                };
            }
            // Morale below the squad's size drops by a positive amount, which none of the
            // reference's (negative) thresholds meets: no flags.
        }
        if flags == 0 {
            flags = match self.host.irand(0, 3) {
                0 => cp::CLEAR | cp::COVER | cp::NEAREST,
                1 => cp::CLEAR | cp::COVER | cp::APPROACH_ENEMY,
                2 => cp::CLEAR | cp::COVER | cp::CLOSEST | cp::APPROACH_ENEMY,
                _ => cp::CLEAR | cp::COVER | cp::FLANK | cp::APPROACH_ENEMY,
            };
        }
        if self.actors[member].script_flags & SCF_USE_CP_NEAREST != 0 {
            flags = (flags & !(cp::FLANK | cp::APPROACH_ENEMY | cp::CLOSEST)) | cp::NEAREST;
        }
        flags
    }

    /// The timers `ST_TrackEnemy`, `ST_ApproachEnemy` and `ST_HuntEnemy` share: the attack
    /// delay (unless `None`), how long to stick, no standing, and how long to scout.
    fn scout_timers(&mut self, at: usize, attack: Option<(i32, i32)>, stick: (i32, i32)) {
        let level_time = self.level_time;
        if let Some((least, most)) = attack {
            let delay = self.host.irand(least, most);
            self.actors[at]
                .mind
                .timers
                .set("attackDelay", level_time, delay);
        }
        let stick = self.host.irand(stick.0, stick.1);
        self.actors[at].mind.timers.set("stick", level_time, stick);
        self.actors[at].mind.timers.set("stand", level_time, -1);
        let until = self.actors[at].mind.timers.get("stick").unwrap_or(-1) - level_time
            + self.host.irand(5_000, 10_000);
        self.actors[at]
            .mind
            .timers
            .set("scoutTime", level_time, until);
        let point = self.actors[at].mind.tactics.combat_point;
        self.free_combat_point(at, point, false);
    }

    /// `ST_TrackEnemy` (`NPC_AI_Stormtrooper.c:1632-1645`): after the enemy's last seen spot.
    fn track_enemy(&mut self, at: usize, spot: [f32; 3]) {
        self.scout_timers(at, Some((1_000, 2_000)), (500, 1_500));
        self.set_move_goal(at, spot, 16, false, -1, None);
    }

    /// `ST_ApproachEnemy` (`NPC_AI_Stormtrooper.c:1647-1658`): the flags of a point nearer.
    fn approach_enemy(&mut self, at: usize) -> i32 {
        self.scout_timers(at, Some((250, 500)), (1_000, 2_000));
        cp::CLEAR | cp::CLOSEST
    }

    /// `ST_HuntEnemy` (`NPC_AI_Stormtrooper.c:1660-1675`): straight at the enemy, if it
    /// may chase.
    fn hunt_enemy(&mut self, at: usize) {
        self.scout_timers(at, None, (250, 1_000));
        if self.actors[at].script_flags & SCF_CHASE_ENEMIES != 0 {
            self.actors[at].mind.goal = self.actors[at].mind.enemy;
        }
    }

    /// `ST_TransferTimers` (`NPC_AI_Stormtrooper.c:1677-1692`): this trooper's timers handed
    /// to `other` (its `scoutTime` read by the wrong name, as the reference reads it), and
    /// its own run out.
    fn transfer_timers(&mut self, me: usize, other: usize) {
        let level_time = self.level_time;
        for (to, from) in [
            ("attackDelay", "attackDelay"),
            ("duck", "duck"),
            ("stick", "stick"),
            ("scoutTime", "scout"),
            ("roamTime", "roamTime"),
            ("stand", "stand"),
        ] {
            let left = self.actors[me].mind.timers.get(from).unwrap_or(-1) - level_time;
            self.actors[other].mind.timers.set(to, level_time, left);
        }
        for name in [
            "attackDelay",
            "duck",
            "stick",
            "scoutTime",
            "roamTime",
            "stand",
        ] {
            self.actors[me].mind.timers.set(name, level_time, -1);
        }
    }

    /// `ST_TransferMoveGoal` (`NPC_AI_Stormtrooper.c:1694-1726`): a squadmate blocking the
    /// way given this trooper's combat point (or goal), squad state and timers; this one
    /// stands a second or three.
    pub fn transfer_move_goal(&mut self, me: usize, other: usize) {
        let level_time = self.level_time;
        let point = self.actors[me].mind.tactics.combat_point;
        if point != -1 {
            self.actors[me].mind.tactics.last_failed_combat_point = point;
            self.actors[other].mind.tactics.combat_point = point;
            self.actors[me].mind.tactics.combat_point = -1;
        } else if self.goal_is_temp(me) {
            let (goal, radius) = (
                self.actors[me].mind.tactics.temp_goal,
                self.actors[me].mind.tactics.goal_radius,
            );
            self.set_move_goal(other, goal.origin, radius, goal.nav_goal, -1, None);
        } else {
            self.actors[other].mind.goal = self.actors[me].mind.goal;
        }
        let group = self.actors[me].mind.tactics.group;
        let state = self.actors[me].mind.tactics.squad_state;
        self.update_squad_state(group, other, state);
        self.transfer_timers(me, other);
        self.update_squad_state(group, me, squad::STAND_AND_SHOOT);
        let stand = self.host.irand(1_000, 3_000);
        self.actors[me].mind.timers.set("stand", level_time, stand);
    }
}
