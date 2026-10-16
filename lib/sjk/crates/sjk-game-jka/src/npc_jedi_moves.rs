//! The Jedi AI's temper and footwork (`codemp/game/NPC_AI_Jedi.c:868-1413`, `1855-1913`):
//! its aggression and how a new enemy changes it (`Jedi_Aggression`,
//! `NPC_Jedi_RateNewEnemy`), rage, battle taunts, the checks that keep it from walking
//! into walls or off ledges (`Jedi_ClearPathToSpot`, `NPC_MoveDirClear`), the moves toward
//! and away from its enemy (`Jedi_Move`, `Jedi_Hunt`, `Jedi_Retreat`, `Jedi_Advance`), the
//! saber style it fights in, and strafing.
//!
//! `Jedi_Track` and `Jedi_FaceEntity` are commented out in the reference
//! (`NPC_AI_Jedi.c:1286-1302`, `1914-1936`, and their one call, `5202-5206`): not ported.

use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use sjk_protocol::UserCommand;

/// `NPCTEAM_PLAYER`.
const NPCTEAM_PLAYER: i32 = 2;
/// `CLASS_DESANN`, `CLASS_JEDI`, `CLASS_TAVION`.
pub(crate) const CLASS_DESANN: i32 = 6;
pub(crate) const CLASS_JEDI: i32 = 18;
pub(crate) const CLASS_TAVION: i32 = 47;
/// `rank_t` (`ai.h:52-62`).
pub(crate) mod rank {
    pub const CIVILIAN: i32 = 0;
    pub const CREWMAN: i32 = 1;
    pub const ENSIGN: i32 = 2;
    pub const LT_JG: i32 = 3;
    pub const LT: i32 = 4;
    pub const COMMANDER: i32 = 6;
    pub const CAPTAIN: i32 = 7;
}
/// The Jedi's voice events (`bg_public.h:1012-1046`): `EV_TAUNT1`..`3`, `EV_JCHASE1`..`3`.
pub(crate) const EV_TAUNT1: i32 = 175;
pub(crate) const EV_TAUNT3: i32 = 177;
pub(crate) const EV_JCHASE1: i32 = 178;
pub(crate) const EV_JCHASE3: i32 = 180;
/// `WP_SABER`, `WP_BLASTER`.
const WP_SABER: i32 = 3;
const WP_BLASTER: i32 = 5;
/// `SCF_CHASE_ENEMIES`.
const SCF_CHASE_ENEMIES: u32 = 0x400;
/// `STEPSIZE`.
const STEPSIZE: f32 = 18.0;
/// `CONTENTS_BOTCLIP`.
const CONTENTS_BOTCLIP: u32 = 0x40;
/// `ENTITYNUM_NONE`.
const ENTITYNUM_NONE: u16 = crate::npc_spawn::ENTITYNUM_NONE;
/// `ps.weaponTime`, `ps.fd.saberAnimLevel`; `BUTTON_ATTACK`, `BUTTON_ALT_ATTACK`.
const PS_WEAPON_TIME: usize = 10;
const PS_SABER_ANIM_LEVEL: usize = 23;
const BUTTON_ATTACK: u16 = 1;
const BUTTON_ALT_ATTACK: u16 = 128;
/// `FORCE_LEVEL_1`, `FORCE_LEVEL_3`, `FORCE_LEVEL_4`, `FORCE_LEVEL_5`.
const FORCE_LEVEL_1: i32 = 1;
const FORCE_LEVEL_3: i32 = 3;
const FORCE_LEVEL_4: i32 = 4;
const FORCE_LEVEL_5: i32 = 5;

/// `VectorMA`.
fn along(start: [f32; 3], scale: f32, direction: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| start[axis] + scale * direction[axis])
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `jediSpeechDebounceTime[playerTeam]` of the NPC at `me`'s team: when a Jedi of its
    /// team may next speak.
    pub(crate) fn jedi_speech_debounce(&self, me: usize) -> i32 {
        usize::try_from(self.actors[me].player_team)
            .ok()
            .and_then(|team| self.level.jedi_speech_debounce.get(team))
            .copied()
            .unwrap_or(0)
    }

    /// Sets `jediSpeechDebounceTime[playerTeam]` and the NPC's own
    /// `blockedSpeechDebounceTime` to `until`, as the Jedi AI's chained assignment does.
    pub(crate) fn jedi_hush(&mut self, me: usize, until: i32) {
        if let Some(slot) = usize::try_from(self.actors[me].player_team)
            .ok()
            .and_then(|team| self.level.jedi_speech_debounce.get_mut(team))
        {
            *slot = until;
        }
        self.actors[me].mind.blocked_speech_until = until;
    }

    /// `Jedi_Aggression` (`NPC_AI_Jedi.c:868-903`): the aggression changed by `change`,
    /// kept within 1..7 for the player's side, 5..20 for Desann, 3..10 for the rest.
    pub fn jedi_aggression(&mut self, me: usize, change: i32) {
        let npc = &mut self.actors[me];
        let (upper, lower) = if npc.player_team == NPCTEAM_PLAYER {
            (7, 1)
        } else if npc.definition.client_class == CLASS_DESANN {
            (20, 5)
        } else {
            (10, 3)
        };
        let stats = &mut npc.definition.stats;
        stats.aggression += change;
        if stats.aggression > upper {
            stats.aggression = upper;
        } else if stats.aggression < lower {
            stats.aggression = lower;
        }
    }

    /// `Jedi_AggressionErosion` (`NPC_AI_Jedi.c:905-917`): unalerted and alone, the
    /// aggression wears away by `amount` every 2 to 5 seconds; below 4 (6 for Desann) the
    /// saber goes off.
    pub fn jedi_aggression_erosion(&mut self, me: usize, amount: i32) {
        let level_time = self.level_time;
        if self.actors[me].mind.timers.done("roamTime", level_time) {
            let roam = self.host.irand(2_000, 5_000);
            self.actors[me]
                .mind
                .timers
                .set("roamTime", level_time, roam);
            self.jedi_aggression(me, amount);
        }
        let npc = &self.actors[me];
        let aggression = npc.definition.stats.aggression;
        if aggression < 4 || (aggression < 6 && npc.definition.client_class == CLASS_DESANN) {
            self.deactivate_saber(me, false);
        }
    }

    /// `NPC_Jedi_RateNewEnemy` (`NPC_AI_Jedi.c:919-955`): the aggression averaged with
    /// what the new enemy's weapon and the NPC's health call for, and no taunt for 4 to 7
    /// seconds.
    pub fn jedi_rate_new_enemy(&mut self, me: usize, enemy: u16) {
        let level_time = self.level_time;
        let Some(foe) = self.body(enemy) else { return };
        let npc = &self.actors[me];
        let health = npc.health as f32;
        let (health_aggression, weapon_aggression) = match foe.weapon {
            WP_SABER => (health / 200.0 * 6.0, 7.0),
            WP_BLASTER
                if crate::npc_senses::distance_squared(npc.current_origin, foe.origin)
                    < 65_536.0 =>
            {
                (health / 200.0 * 8.0, 8.0)
            }
            WP_BLASTER => (8.0 - (health / 200.0 * 8.0), 2.0),
            _ => (health / 200.0 * 8.0, 6.0),
        };
        let aggression = npc.definition.stats.aggression;
        // `ceil` of a float sum over 3.0f, taken as a double.
        let new_aggression =
            f64::from((health_aggression + weapon_aggression + aggression as f32) / 3.0).ceil()
                as i32;
        self.jedi_aggression(me, new_aggression - aggression);
        let chatter = self.host.irand(4_000, 7_000);
        self.actors[me]
            .mind
            .timers
            .set("chatter", level_time, chatter);
    }

    /// `Jedi_Rage` (`NPC_AI_Jedi.c:957-969`): aggression to about 10, the calm timers run
    /// out, and Force rage.
    pub fn jedi_rage(&mut self, me: usize) {
        let level_time = self.level_time;
        let aggression = self.actors[me].definition.stats.aggression;
        let change = 10 - aggression + self.host.irand(-2, 2);
        self.jedi_aggression(me, change);
        let timers = &mut self.actors[me].mind.timers;
        for name in [
            "roamTime",
            "chatter",
            "walking",
            "taunting",
            "jumpChaseDebounce",
            "movenone",
            "movecenter",
            "noturn",
        ] {
            timers.set(name, level_time, 0);
        }
        self.force_rage(me);
    }

    /// `Jedi_RageStop` (`NPC_AI_Jedi.c:971-979`): calm down and back off.
    pub fn jedi_rage_stop(&mut self, me: usize) {
        let level_time = self.level_time;
        self.actors[me].mind.timers.set("roamTime", level_time, 0);
        let change = self.host.irand(-5, 0);
        self.jedi_aggression(me, change);
    }

    /// `Jedi_BattleTaunt` (`NPC_AI_Jedi.c:985-1018`): now and then, while nobody of its
    /// team spoke lately, a taunt — a Jedi training against a Jedi only when it is the
    /// trainer. Whether it spoke.
    pub fn jedi_battle_taunt(&mut self, me: usize) -> bool {
        let level_time = self.level_time;
        if !self.actors[me].mind.timers.done("chatter", level_time)
            || self.host.irand(0, 3) != 0
            || self.actors[me].mind.blocked_speech_until >= level_time
            || self.jedi_speech_debounce(me) >= level_time
        {
            return false;
        }
        let npc = &self.actors[me];
        let enemy_is_jedi = npc
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
            .is_some_and(|enemy| enemy.class == CLASS_JEDI);
        let event = if npc.player_team == NPCTEAM_PLAYER && enemy_is_jedi {
            // "a jedi fighting a jedi - training": only the trainer taunts.
            (npc.definition.client_class == CLASS_JEDI && npc.definition.rank == rank::COMMANDER)
                .then_some(EV_TAUNT1)
        } else {
            Some(self.host.irand(EV_TAUNT1, EV_TAUNT3))
        };
        let Some(event) = event else { return false };
        self.add_voice(me, event, 3_000);
        self.jedi_hush(me, level_time + 6_000);
        let chatter = self.host.irand(5_000, 10_000);
        self.actors[me]
            .mind
            .timers
            .set("chatter", level_time, chatter);
        true
    }

    /// `Jedi_ClearPathToSpot` (`NPC_AI_Jedi.c:1025-1081`): a straight box trace to `dest`
    /// (only `impact_ent` may stop it), and floor every body width along it — no drop of
    /// more than a step going up, of 64 going down or level.
    pub fn jedi_clear_path_to_spot(&mut self, me: usize, dest: [f32; 3], impact_ent: u16) -> bool {
        let npc = &self.actors[me];
        let (origin, maxs, number, clip) =
            (npc.current_origin, npc.maxs, npc.number, npc.clip_mask);
        let mins = [npc.mins[0], npc.mins[1], npc.mins[2] + STEPSIZE];
        let trace = self.trace_bodies(origin, mins, maxs, dest, number, clip);
        if trace.all_solid || trace.start_solid {
            return false;
        }
        if trace.fraction < 1.0 {
            return impact_ent != ENTITYNUM_NONE && trace.entity_number == impact_ent;
        }
        let mut dir = crate::npc_senses::subtract(dest, origin);
        let dist = crate::saber_clash::normalize(&mut dir);
        let drop = if dest[2] > origin[2] { STEPSIZE } else { 64.0 };
        let step = maxs[0] * 2.0;
        let mut i = step;
        while i < dist {
            let start = along(origin, i, dir);
            let end = [start[0], start[1], start[2] - drop];
            let trace = self.trace_bodies(start, mins, maxs, end, number, clip);
            if !(trace.fraction < 1.0 || trace.all_solid || trace.start_solid) {
                // "no floor here! (or a long drop?)"
                return false;
            }
            i += step;
        }
        true
    }

    /// `NPC_MoveDirClear` (`NPC_AI_Jedi.c:1083-1202`): whether a move of `forward` and
    /// `right` from where it stands, facing its view's yaw, runs into nothing close (its
    /// enemy or goal aside) and off no ledge deeper than four steps (deeper by as much as
    /// its goal lies below). With `reset`, a blocked move is stopped and a move off a ledge
    /// turned about.
    pub fn npc_move_dir_clear(
        &mut self,
        me: usize,
        forward: i32,
        right: i32,
        reset: bool,
        command: &mut UserCommand,
    ) -> bool {
        let mut bottom_max = -STEPSIZE * 4.0 - 1.0;
        if forward == 0 && right == 0 {
            return true;
        }
        let npc = &self.actors[me];
        if command.up_move > 0 || npc.force.jump_charge != 0.0 {
            return true;
        }
        if npc.player.ground_entity_num() == ENTITYNUM_NONE {
            return true;
        }
        let (origin, maxs, number, clip) =
            (npc.current_origin, npc.maxs, npc.number, npc.clip_mask);
        let mins = [npc.mins[0], npc.mins[1], npc.mins[2] + STEPSIZE];
        let (enemy, goal) = (npc.mind.enemy, npc.mind.goal);
        let (forward_axis, right_axis) =
            crate::pmove::flight::flight_axes([0.0, npc.player.view_angles()[1], 0.0]);
        let (forward_axis, right_axis) = (forward_axis.to_array(), right_axis.to_array());
        let test = along(
            along(origin, forward as f32 / 2.0, forward_axis),
            right as f32 / 2.0,
            right_axis,
        );
        let mut trace =
            self.trace_bodies(origin, mins, maxs, test, number, clip | CONTENTS_BOTCLIP);
        if trace.all_solid || trace.start_solid {
            if reset {
                trace.fraction = 1.0;
            }
            trace.end_position = test;
        }
        if f64::from(trace.fraction) < 0.6 {
            // "okay to bump into enemy or goal"
            if (enemy.is_some() && Some(trace.entity_number) == enemy)
                || (goal.is_some() && Some(trace.entity_number) == goal)
            {
                return true;
            }
            if reset {
                command.forward_move = 0;
                command.right_move = 0;
                self.actors[me].mind.move_dir = [0.0; 3];
            }
            return false;
        }
        if let Some(goal_origin) = goal.and_then(|_| self.goal_origin(me))
            && goal_origin[2] < origin[2]
        {
            bottom_max += goal_origin[2] - origin[2];
        }
        let start = trace.end_position;
        let end = [start[0], start[1], start[2] + bottom_max];
        let trace = self.trace_bodies(start, mins, maxs, end, number, clip);
        if trace.all_solid || trace.start_solid || f64::from(trace.fraction) < 1.0 {
            return true;
        }
        if reset {
            // `ucmd.forwardmove *= -1.0`: a signed char through a double.
            command.forward_move = (f64::from(command.forward_move) * -1.0) as i32 as i8;
            command.right_move = (f64::from(command.right_move) * -1.0) as i32 as i8;
            let dir = &mut self.actors[me].mind.move_dir;
            *dir = dir.map(|value| value * -1.0);
        }
        false
    }

    /// `Jedi_HoldPosition` (`NPC_AI_Jedi.c:1204-1216`): no goal.
    pub fn jedi_hold_position(&mut self, me: usize) {
        self.actors[me].mind.goal = None;
    }

    /// `Jedi_Move` (`NPC_AI_Jedi.c:1223-1255`): a combat move toward `goal` — turned about
    /// to retreat — holding its position where it ran into its enemy or the move failed.
    pub fn jedi_move(
        &mut self,
        me: usize,
        goal: Option<u16>,
        retreat: bool,
        command: &mut UserCommand,
    ) {
        let npc = &mut self.actors[me];
        npc.mind.combat_move = true;
        npc.mind.goal = goal;
        let moved = self.move_to_goal(me, true, command);
        if retreat {
            command.forward_move = command.forward_move.wrapping_neg();
            command.right_move = command.right_move.wrapping_neg();
            let dir = &mut self.actors[me].mind.move_dir;
            *dir = dir.map(|value| value * -1.0);
        }
        let info = self.level.nav;
        if info.flags & crate::npc_nav::NIF_COLLISION != 0
            && info.blocker == self.actors[me].mind.enemy
        {
            self.jedi_hold_position(me);
        }
        if !moved {
            self.jedi_hold_position(me);
        }
    }

    /// `Jedi_Hunt` (`NPC_AI_Jedi.c:1257-1284`): at all willing to fight, after its enemy —
    /// or, not allowed to chase, just facing. Whether it did.
    pub fn jedi_hunt(&mut self, me: usize, command: &mut UserCommand) -> bool {
        let npc = &mut self.actors[me];
        if npc.definition.stats.aggression <= 1 {
            return false;
        }
        npc.mind.combat_move = true;
        if npc.script_flags & SCF_CHASE_ENEMIES == 0 {
            self.update_angles(me, true, true, command);
            return true;
        }
        if npc.mind.goal.is_none() {
            npc.mind.goal = npc.mind.enemy;
        }
        if self.move_to_goal(me, false, command) {
            self.update_angles(me, true, true, command);
            return true;
        }
        false
    }

    /// `Jedi_Retreat` (`NPC_AI_Jedi.c:1304-1314`): back away from its enemy, unless held.
    pub fn jedi_retreat(&mut self, me: usize, command: &mut UserCommand) {
        if !self.actors[me]
            .mind
            .timers
            .done("noRetreat", self.level_time)
        {
            return;
        }
        let enemy = self.actors[me].mind.enemy;
        self.jedi_move(me, enemy, true, command);
    }

    /// `Jedi_Advance` (`NPC_AI_Jedi.c:1316-1329`): the saber on if in hand, and at its
    /// enemy.
    pub fn jedi_advance(&mut self, me: usize, command: &mut UserCommand) {
        if !self.actors[me].player.saber_in_flight() {
            self.activate_saber(me);
        }
        let enemy = self.actors[me].mind.enemy;
        self.jedi_move(me, enemy, false, command);
    }

    /// `Jedi_AdjustSaberAnimLevel` (`NPC_AI_Jedi.c:1331-1398`): Tavion and Desann keep
    /// their own styles, the enemy's grunts and fencers the fast one, acrobats and
    /// Force-users the medium one; the rest take `level`, within 1 and their offense.
    pub fn jedi_adjust_saber_anim_level(&mut self, me: usize, level: i32) {
        let npc = &mut self.actors[me];
        let (class, rank) = (npc.definition.client_class, npc.definition.rank);
        let style = if class == CLASS_TAVION {
            FORCE_LEVEL_5
        } else if class == CLASS_DESANN {
            FORCE_LEVEL_4
        } else if npc.player_team == crate::npc_enemy::NPCTEAM_ENEMY
            && (rank == rank::CIVILIAN || rank == rank::LT_JG)
        {
            FORCE_LEVEL_1
        } else if npc.player_team == crate::npc_enemy::NPCTEAM_ENEMY
            && (rank == rank::CREWMAN || rank == rank::ENSIGN)
        {
            2
        } else {
            let offense = npc.force_levels[crate::force_powers::FP_SABER_OFFENSE];
            if level > offense {
                offense
            } else if level < FORCE_LEVEL_1 {
                FORCE_LEVEL_1
            } else {
                level
            }
        };
        npc.player.set_raw_field(PS_SABER_ANIM_LEVEL, style as u32);
    }

    /// `Jedi_CheckDecreaseSaberAnimLevel` (`NPC_AI_Jedi.c:1400-1416`): not attacking (no
    /// weapon time, no attack in `command`, `NPCS.ucmd`), now and then a random style;
    /// attacking, the next change put off.
    pub fn jedi_check_decrease_saber_anim_level(&mut self, me: usize, command: &UserCommand) {
        let level_time = self.level_time;
        let busy = self.actors[me]
            .player
            .raw_field(PS_WEAPON_TIME)
            .unwrap_or(0)
            != 0;
        if !busy && command.buttons & (BUTTON_ATTACK | BUTTON_ALT_ATTACK) == 0 {
            if self.actors[me]
                .mind
                .timers
                .done("saberLevelDebounce", level_time)
                && self.host.irand(0, 10) == 0
            {
                let level = self.host.irand(FORCE_LEVEL_1, FORCE_LEVEL_3);
                self.jedi_adjust_saber_anim_level(me, level);
                let debounce = self.host.irand(3_000, 10_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("saberLevelDebounce", level_time, debounce);
            }
        } else {
            let debounce = self.host.irand(1_000, 5_000);
            self.actors[me]
                .mind
                .timers
                .set("saberLevelDebounce", level_time, debounce);
        }
    }

    /// `Jedi_Strafe` (`NPC_AI_Jedi.c:1855-1913`): not already strafing, a strafe to a side
    /// picked at random (the other if that one is not clear) for `min`..`max`, the next
    /// one not for `next_min`..`next_max` after it; slow when `walking`. Whether it
    /// strafed.
    pub fn jedi_strafe(
        &mut self,
        me: usize,
        min: i32,
        max: i32,
        next_min: i32,
        next_max: i32,
        walking: bool,
        command: &mut UserCommand,
    ) -> bool {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        if crate::npc_behavior::cultist_destroyer(
            npc.definition.client_class,
            npc.player.weapon(),
            &npc.npc_type,
        ) {
            return false;
        }
        let pressing = npc.saber.event_flags & crate::saber_clash::sef::LOCK_WON != 0
            && npc
                .mind
                .enemy
                .and_then(|enemy| self.jedi_foe(enemy))
                .is_some_and(|foe| foe.pain_debounce_time > level_time);
        if pressing {
            return false;
        }
        let timers = &self.actors[me].mind.timers;
        if !(timers.done("strafeLeft", level_time) && timers.done("strafeRight", level_time)) {
            return false;
        }
        let strafe_time = self.host.irand(min, max);
        let forward = i32::from(command.forward_move);
        let sides = if self.host.irand(0, 1) != 0 {
            [(-127, "strafeLeft"), (127, "strafeRight")]
        } else {
            [(127, "strafeRight"), (-127, "strafeLeft")]
        };
        let mut strafed = false;
        for (right, timer) in sides {
            if self.npc_move_dir_clear(me, forward, right, false, command) {
                self.actors[me]
                    .mind
                    .timers
                    .set(timer, level_time, strafe_time);
                strafed = true;
                break;
            }
        }
        if !strafed {
            return false;
        }
        let next = self.host.irand(next_min, next_max);
        let timers = &mut self.actors[me].mind.timers;
        timers.set("noStrafe", level_time, strafe_time + next);
        if walking {
            timers.set("walking", level_time, strafe_time);
        }
        true
    }
}
