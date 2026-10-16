//! A Jedi NPC between fights (`codemp/game/NPC_AI_Jedi.c`): its pain (`NPC_Jedi_Pain`,
//! `5326-5423`), the danger and the player it wakes for (`Jedi_CheckDanger`,
//! `Jedi_CheckAmbushPlayer`, `5425-5528`), the ceiling ambush (`Jedi_Ambush`,
//! `Jedi_WaitingAmbush`, `5530-5553`), its patrol (`Jedi_Patrol`, `5560-5718`), its saber
//! called back (`Jedi_CanPullBackSaber`, `5720-5742`), following a leader
//! (`NPC_BSJedi_FollowLeader`, `5748-5804`), and the resisted push (`WP_ResistForcePush`,
//! `223-291`).

use crate::force_powers::{FP_GRIP, FP_PULL, FP_PUSH, FP_SABER_DEFENSE};
use crate::npc_senses::{AEL_DANGER, AEL_MINOR, Body, Spot, distance_squared, in_fov, spot};
use crate::npc_spawn::{ENTITYNUM_NONE, NpcHost};
use crate::npc_world::NpcWorld;
use crate::pmove_anim::{
    SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_LEGS, SETANIM_TORSO,
};
use sjk_protocol::UserCommand;

/// `playerState_t` wire fields: `weaponTime`, `saberEntityNum`, `saberBlocked`,
/// `saberInFlight`, `eFlags2`.
const PS_WEAPON_TIME: usize = 10;
const PS_SABER_ENTITY_NUM: usize = 31;
const PS_SABER_BLOCKED: usize = 77;
const PS_SABER_IN_FLIGHT: usize = 88;
const PS_EFLAGS2: usize = 103;
/// `EF2_HELD_BY_MONSTER`.
const EF2_HELD_BY_MONSTER: u32 = 1;
/// `PMF_TIME_KNOCKBACK`.
const PMF_TIME_KNOCKBACK: u16 = 64;
/// `powerups[]`: `PW_PULL`, `PW_DISINT_4`, `PW_CLOAKED`.
const PW_PULL: usize = 3;
const PW_DISINT_4: usize = 9;
const PW_CLOAKED: usize = 11;
/// `saberBlockedType_t`: `BLOCKED_NONE`, `BLOCKED_PARRY_BROKEN`.
const BLOCKED_NONE: u32 = 0;
const BLOCKED_PARRY_BROKEN: u32 = 2;
/// `FORCE_LEVEL_1`, `FORCE_LEVEL_3`.
const FORCE_LEVEL_1: i32 = 1;
const FORCE_LEVEL_3: i32 = 3;
/// `class_t`.
pub(crate) const CLASS_DESANN: i32 = 6;
pub(crate) const CLASS_LUKE: i32 = 22;
pub(crate) const CLASS_REBORN: i32 = 37;
pub(crate) const CLASS_SHADOWTROOPER: i32 = 43;
pub(crate) const CLASS_TAVION: i32 = 47;
pub(crate) const CLASS_BOBAFETT: i32 = 52;
/// `RANK_LT_JG`.
const RANK_LT_JG: i32 = 3;
/// `weapon_t`: `WP_SABER`.
const WP_SABER: i32 = 3;
/// `EV_ANGER1`, `EV_PUSHED1`, `EV_JDETECTED1`.
const EV_ANGER1: i32 = 116;
const EV_PUSHED1: i32 = 125;
const EV_JDETECTED1: i32 = 172;
/// Animations: `BOTH_CEILING_CLING`, `BOTH_CEILING_DROP`, `BOTH_RESISTPUSH`.
const BOTH_CEILING_CLING: u16 = 1_250;
const BOTH_CEILING_DROP: u16 = 1_251;
const BOTH_RESISTPUSH: u16 = 1_330;
/// `SCF_LOOK_FOR_ENEMIES`.
const SCF_LOOK_FOR_ENEMIES: u32 = 0x800;
/// `NPCAI_BLOCKED`.
const NPCAI_BLOCKED: u32 = 0x40;
/// `BUTTON_ATTACK`, `BUTTON_WALKING`.
const BUTTON_ATTACK: u16 = 1;
const BUTTON_WALKING: u16 = 16;
/// `CONTENTS_BODY`, `CONTENTS_BOTCLIP`.
const CONTENTS_BODY: u32 = 0x100;
const CONTENTS_BOTCLIP: u32 = 0x40;
/// `Q3_INFINITE`.
pub(crate) const Q3_INFINITE: i32 = 16_777_216;
/// `JSF_AMBUSH` (`w_saber.h:46`): the Jedi waits on the ceiling.
const JSF_AMBUSH: i32 = 16;
/// `timescale`, as a dedicated server runs: `WP_ResistForcePush` scales a sped-up
/// resister's pause by it.
const TIMESCALE: f32 = 1.0;

/// `DistanceHorizontalSquared`.
pub(crate) fn distance_horizontal_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (x, y) = (a[0] - b[0], a[1] - b[1]);
    x * x + y * y
}

/// `VectorNormalize` (`q_math.c`): the unit vector, and the length it had (a zero vector
/// stays as it is).
pub(crate) fn normalized(vector: [f32; 3]) -> ([f32; 3], f32) {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if length == 0.0 {
        return (vector, 0.0);
    }
    let inverse = 1.0 / length;
    (vector.map(|axis| axis * inverse), length)
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `Jedi_WaitingAmbush` (`NPC_AI_Jedi.c:5545-5552`): an ambusher still clinging to the
    /// ceiling.
    pub fn jedi_waiting_ambush(&self, me: usize) -> bool {
        let npc = &self.actors[me];
        npc.spawnflags & JSF_AMBUSH != 0 && npc.mind.noclip
    }

    /// `Jedi_CanPullBackSaber` (`NPC_AI_Jedi.c:5720-5742`): not while its parry is broken,
    /// and — but for the masters — not in pain.
    pub fn jedi_can_pull_back_saber(&self, me: usize) -> bool {
        let npc = &self.actors[me];
        if npc.player.raw_field(PS_SABER_BLOCKED).unwrap_or(0) == BLOCKED_PARRY_BROKEN
            && !npc.mind.timers.done("parryTime", self.level_time)
        {
            return false;
        }
        let class = npc.definition.client_class;
        if matches!(
            class,
            CLASS_SHADOWTROOPER | CLASS_TAVION | CLASS_LUKE | CLASS_DESANN
        ) || npc.npc_type.eq_ignore_ascii_case(b"yoda")
        {
            return true;
        }
        npc.mind.fight.pain_debounce_time <= self.level_time
    }

    /// `NPC_Jedi_Pain` (`NPC_AI_Jedi.c:5326-5423`): a saber's blow makes it back off (no
    /// parry for a while, maybe another style, maybe less aggression), anything else angers
    /// it; its grip lets go, the pain runs (`NPC_Pain`), a push is voiced, and an ambusher
    /// drops from the ceiling. `means` is `gPainMOD`, which `NPC_Pain` reads.
    pub fn jedi_pain(&mut self, me: usize, attacker: Option<u16>, damage: i32, means: u32) {
        let level_time = self.level_time;
        // The world (no attacker) has weapon 0.
        let weapon = attacker
            .and_then(|number| self.body(number))
            .map_or(0, |body| body.weapon);
        if weapon == WP_SABER {
            self.actors[me].mind.timers.set("parryTime", level_time, -1);
            let skill = self.host.skill();
            let npc = &mut self.actors[me];
            let step = if npc.definition.client_class == CLASS_DESANN
                || npc.npc_type.eq_ignore_ascii_case(b"yoda")
            {
                50
            } else if npc.definition.rank >= RANK_LT_JG {
                100
            } else {
                200
            };
            npc.force.debounce[FP_SABER_DEFENSE] = level_time + (3 - skill) * step;
            if self.host.irand(0, 3) == 0 {
                let level = self.host.irand(FORCE_LEVEL_1, FORCE_LEVEL_3);
                self.jedi_adjust_saber_anim_level(me, level);
            }
            if self.host.irand(0, 1) == 0 {
                self.jedi_aggression(me, -1);
            }
        } else {
            self.jedi_aggression(me, 1);
        }
        self.actors[me].mind.tactics.enemy_check_debounce_time = 0;
        self.force_power_stop(me, FP_GRIP);
        self.npc_pain(me, attacker, damage, means);
        if damage == 0 && self.actors[me].health > 0 {
            let event = self.host.irand(EV_PUSHED1, EV_PUSHED1 + 2);
            self.add_voice(me, event, 2_000);
        }
        if self.jedi_waiting_ambush(me) {
            self.actors[me].mind.noclip = false;
        }
        let drop = SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD;
        if self.actors[me].player.leg_animation() == BOTH_CEILING_CLING {
            self.set_animation(me, SETANIM_LEGS, BOTH_CEILING_DROP, drop);
        }
        if self.actors[me].player.torso_animation() == BOTH_CEILING_CLING {
            self.set_animation(me, SETANIM_TORSO, BOTH_CEILING_DROP, drop);
        }
    }

    /// `Jedi_CheckDanger` (`NPC_AI_Jedi.c:5425-5444`): a dangerous alert of its own (or its
    /// own side's) making sets its maker as the enemy — the reference's test, inverted as it
    /// is. Whether it did.
    pub fn jedi_check_danger(&mut self, me: usize) -> bool {
        // The reference reads `level.alertEvents[-1]` when nothing is noticed; here nothing
        // noticed is no danger.
        let Some(alert) = self
            .check_alerts(me, -1, false, AEL_MINOR)
            .map(|at| self.alerts.events()[at])
        else {
            return false;
        };
        if alert.level < AEL_DANGER {
            return false;
        }
        let Some(owner) = alert.owner.and_then(|owner| self.body(owner)) else {
            return false;
        };
        let npc = &self.actors[me];
        if owner.number != npc.number && owner.player_team != npc.player_team {
            return false;
        }
        self.set_enemy(me, owner.number);
        self.wake_to_enemy(me);
        true
    }

    /// `Jedi_CheckAmbushPlayer` (`NPC_AI_Jedi.c:5446-5528`): a valid player below it and
    /// near (in view unless very near, in sight) — or, uncloaked, anyone looking at it —
    /// made the enemy. Whether one was.
    pub fn jedi_check_ambush_player(&mut self, me: usize) -> bool {
        for at in 0..self.host.players().len() {
            let player = self.host.players()[at];
            if !self.valid_for(me, &player) {
                continue;
            }
            let cloaked = self.actors[me].player.powerups[PW_CLOAKED] != 0;
            if (cloaked || !self.someone_looking_at(me)) && !self.ambush_sees(me, &player, cloaked)
            {
                continue;
            }
            self.set_enemy(me, player.number);
            self.wake_to_enemy(me);
            return true;
        }
        false
    }

    /// The body of `Jedi_CheckAmbushPlayer`'s test for one player (`5470-5516`): in the
    /// same room (which turns an uncloaked ambusher's eyes to entity 0), below it by up to
    /// 512, within 64 — or within 384 and its view — and in clear sight.
    fn ambush_sees(&mut self, me: usize, player: &Body, cloaked: bool) -> bool {
        let origin = self.actors[me].current_origin;
        if !self.host.in_pvs(player.origin, origin) {
            return false;
        }
        if !cloaked {
            crate::npc_enemy::set_look(&mut self.actors[me], 0, 0);
        }
        let z_diff = origin[2] - player.origin[2];
        if z_diff <= 0.0 || z_diff > 512.0 {
            return false;
        }
        let distance = distance_horizontal_squared(player.origin, origin);
        if distance > 4_096.0 {
            if distance > 147_456.0 {
                return false;
            }
            let hfov = if cloaked { 30 } else { 45 };
            if !in_fov(player, &self.npc(me), hfov, 90) {
                return false;
            }
        }
        self.clear_los4(me, player)
    }

    /// What `Jedi_CheckDanger` and `Jedi_CheckAmbushPlayer` do on setting the enemy: seen
    /// now, and an attack delayed.
    fn wake_to_enemy(&mut self, me: usize) {
        let level_time = self.level_time;
        self.actors[me].mind.tactics.enemy_last_seen_time = level_time;
        let delay = self.host.irand(500, 2_500);
        self.actors[me]
            .mind
            .timers
            .set("attackDelay", level_time, delay);
    }

    /// `Jedi_Ambush` (`NPC_AI_Jedi.c:5530-5543`): the drop from the ceiling, the saber lit
    /// (Boba Fett has none), uncloaked, with an angry shout.
    pub fn jedi_ambush(&mut self, me: usize) {
        self.actors[me].mind.noclip = false;
        self.set_animation(
            me,
            SETANIM_BOTH,
            BOTH_CEILING_DROP,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        );
        let npc = &mut self.actors[me];
        let torso_timer = npc.player.torso_timer();
        npc.player.set_raw_field(PS_WEAPON_TIME, torso_timer as u32);
        if npc.definition.client_class != CLASS_BOBAFETT {
            self.activate_saber(me);
        }
        self.jedi_decloak(me);
        let event = self.host.irand(EV_ANGER1, EV_ANGER1 + 2);
        self.add_voice(me, event, 1_000);
    }

    /// `Jedi_Patrol` (`NPC_AI_Jedi.c:5560-5718`): an ambusher on the ceiling watches for the
    /// player or danger; a Jedi looking for enemies takes one near (or whose thrown saber
    /// comes at it), else toys with the player it sees in stages; then it walks to its
    /// goal.
    pub fn jedi_patrol(&mut self, me: usize, command: &mut UserCommand) {
        self.actors[me]
            .player
            .set_raw_field(PS_SABER_BLOCKED, BLOCKED_NONE);
        let looks = self.actors[me].script_flags & SCF_LOOK_FOR_ENEMIES != 0;
        if self.jedi_waiting_ambush(me) {
            self.set_animation(
                me,
                SETANIM_BOTH,
                BOTH_CEILING_CLING,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            if looks && (self.jedi_check_ambush_player(me) || self.jedi_check_danger(me)) {
                self.jedi_ambush(me);
                self.update_angles(me, true, true, command);
                return;
            }
        } else if looks {
            self.patrol_look(me, command);
        }
        // `finish:`
        if self.update_goal(me, command).is_some() {
            command.buttons |= BUTTON_WALKING;
            self.move_to_goal(me, true, command);
        }
        self.update_angles(me, true, true, command);
        if self.actors[me].mind.enemy.is_some() {
            let delay = self.host.irand(3_000, 10_000);
            self.actors[me].mind.tactics.enemy_check_debounce_time = self.level_time + delay;
        }
    }

    /// `Jedi_Patrol`'s look for enemies (`5561-5700`): every client of the enemy team in the
    /// same room, in entity-number order — a player always weighed, anyone else only if
    /// nearer than the best so far.
    fn patrol_look(&mut self, me: usize, command: &mut UserCommand) {
        let mut best: Option<(Body, f32)> = None;
        let mut best_distance = Q3_INFINITE as f32;
        let mut at = 0;
        while let Some(enemy) = self.nth_client(at) {
            at += 1;
            let npc = &self.actors[me];
            if !self.valid_for(me, &enemy)
                || enemy.player_team != npc.enemy_team
                || !self.host.in_pvs(npc.current_origin, enemy.origin)
            {
                continue;
            }
            let distance = distance_squared(npc.current_origin, enemy.origin);
            if enemy.npc && distance >= best_distance {
                continue;
            }
            if self.patrol_threat(me, &enemy, distance) {
                self.set_enemy(me, enemy.number);
                self.actors[me].definition.stats.aggression = 3;
                break;
            }
            best_distance = distance;
            best = Some((enemy, distance));
        }
        if self.actors[me].mind.enemy.is_some() {
            return;
        }
        match best {
            None => self.jedi_aggression_erosion(me, -1),
            Some((enemy, distance)) => self.consider(me, &enemy, distance, command),
        }
    }

    /// Whether `Jedi_Patrol` takes `enemy` at once (`5575-5608`): within 220, or it has
    /// watched long with its saber lit, or its thrown blade heads at the Jedi from within
    /// 200.
    fn patrol_threat(&mut self, me: usize, enemy: &Body, distance: f32) -> bool {
        let npc = &self.actors[me];
        let lit = npc.player.saber_holstered() == 0;
        if distance < (220 * 220) as f32 || npc.mind.tactics.investigate_count >= 3 && lit {
            return true;
        }
        if !enemy.saber_in_flight || enemy.saber_holstered {
            return false;
        }
        let Some((saber_origin, saber_delta)) = self.thrown_saber(enemy.number) else {
            return false;
        };
        let (to_me, saber_distance) = normalized(crate::npc_senses::subtract(
            self.actors[me].current_origin,
            saber_origin,
        ));
        let (heading, _) = normalized(saber_delta);
        let dot = heading[0] * to_me[0] + heading[1] * to_me[1] + heading[2] * to_me[2];
        dot > 0.5 && saber_distance < 200.0
    }

    /// `Jedi_Patrol`'s "have one to consider" (`5617-5698`): another NPC in clear sight is
    /// attacked; the player (entity 0) is watched, noticed with a voice, faced, and at last
    /// met with a lit saber as he comes nearer.
    fn consider(&mut self, me: usize, enemy: &Body, distance: f32, command: &mut UserCommand) {
        let level_time = self.level_time;
        if !self.clear_los4(me, enemy) {
            if self.actors[me].mind.timers.done("watchTime", level_time) {
                self.clear_look(me);
            }
            return;
        }
        // `best_enemy->s.number`: only entity 0 is toyed with.
        if enemy.number != 0 {
            self.set_enemy(me, enemy.number);
            self.actors[me].definition.stats.aggression = 3;
            return;
        }
        if self.actors[me].definition.client_class == CLASS_BOBAFETT {
            return;
        }
        if self.actors[me].mind.timers.done("watchTime", level_time) {
            if self.actors[me].mind.timers.get("watchTime").unwrap_or(-1) == -1 {
                let watch = self.host.irand(3_000, 5_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("watchTime", level_time, watch);
                // `goto finish`.
                return;
            }
            if self.actors[me].mind.tactics.investigate_count == 0 {
                let event = self.host.irand(EV_JDETECTED1, EV_JDETECTED1 + 2);
                self.add_voice(me, event, 3_000);
            }
            self.actors[me].mind.tactics.investigate_count += 1;
            let watch = self.host.irand(4_000, 10_000);
            self.actors[me]
                .mind
                .timers
                .set("watchTime", level_time, watch);
        }
        let investigated = self.actors[me].mind.tactics.investigate_count;
        if distance < (440 * 440) as f32 || investigated >= 2 {
            self.face_entity(me, enemy, true, command);
            if distance < (330 * 330) as f32
                && self.actors[me]
                    .player
                    .raw_field(PS_SABER_IN_FLIGHT)
                    .unwrap_or(0)
                    == 0
            {
                self.activate_saber(me);
            }
        } else if distance < (550 * 550) as f32 || investigated == 1 {
            if self.actors[me].mind.timers.done("watchTime", level_time) {
                self.face_entity(me, enemy, true, command);
            }
        } else {
            crate::npc_enemy::set_look(&mut self.actors[me], enemy.number, 0);
        }
    }

    /// `NPC_BSJedi_FollowLeader` (`NPC_AI_Jedi.c:5748-5804`): a dropped saber fetched first
    /// if it fights; a goal it cannot walk straight to jumped to; a blocked move's end
    /// jumped to when far above or below; else the common follow.
    pub fn bs_jedi_follow_leader(&mut self, me: usize, command: &mut UserCommand) {
        self.actors[me]
            .player
            .set_raw_field(PS_SABER_BLOCKED, BLOCKED_NONE);
        if self.actors[me].mind.enemy.is_none() {
            self.jedi_aggression_erosion(me, -1);
        }
        if self.fetch_saber(me, command) {
            return;
        }
        if let Some(goal) = self.actors[me].mind.goal {
            if self.jedi_jumping(me, Some(goal), command) {
                return;
            }
            let clip = (self.actors[me].clip_mask & !CONTENTS_BODY) | CONTENTS_BOTCLIP;
            if let Some(origin) = self.entity_origin(me, goal)
                && !self.check_ahead(me, origin, clip).0
                && self.goal_in_sight(me, goal)
                && self.face_entity_number(me, goal, command)
                && self.jedi_try_jump(me, Some(goal), command)
            {
                return;
            }
            let npc = &self.actors[me];
            let destination = npc.mind.tactics.blocked_dest;
            if npc.ai_flags & NPCAI_BLOCKED != 0
                && f64::from(destination[2] - npc.current_origin[2]).abs() > 64.0
            {
                self.actors[me]
                    .mind
                    .timers
                    .set("jumpChaseDebounce", self.level_time, -1);
                if self.jedi_try_jump_to_point(me, destination, command) {
                    return;
                }
            }
        }
        if self.level.states_stood_in() {
            self.host.stub(self.actors[me].number, "NPC_BSFollowLeader");
        } else {
            self.bs_follow_leader(me, command);
        }
    }

    /// `NPC_BSJedi_FollowLeader`'s dropped saber (`5757-5790`): lying still and callable,
    /// it becomes the goal and the attack button calls it; while the enemy lives, the Jedi
    /// goes (or jumps) for it at once. Whether that took the frame.
    fn fetch_saber(&mut self, me: usize, command: &mut UserCommand) -> bool {
        let npc = &self.actors[me];
        if npc.player.raw_field(PS_SABER_IN_FLIGHT).unwrap_or(0) == 0 {
            return false;
        }
        let saber = npc.player.raw_field(PS_SABER_ENTITY_NUM).unwrap_or(0);
        if saber == 0
            || saber >= u32::from(ENTITYNUM_NONE)
            || !self.npc_saber_stationary(me)
            || !self.jedi_can_pull_back_saber(me)
        {
            return false;
        }
        let saber = saber as u16;
        let npc = &mut self.actors[me];
        npc.player.set_raw_field(PS_SABER_BLOCKED, BLOCKED_NONE);
        npc.mind.goal = Some(saber);
        command.buttons |= BUTTON_ATTACK;
        if !self.enemy_lives(me) {
            return false;
        }
        if !self.move_to_goal(me, true, command) {
            self.face_entity_number(me, saber, command);
            self.jedi_try_jump(me, Some(saber), command);
        }
        self.update_angles(me, true, true, command);
        true
    }

    /// `NPC->enemy && NPC->enemy->health > 0`.
    pub(crate) fn enemy_lives(&self, me: usize) -> bool {
        self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
            .is_some_and(|enemy| enemy.health > 0)
    }

    /// `NPC_ClearLOS4(ent)` for any entity: a client's body, else its origin.
    fn goal_in_sight(&mut self, me: usize, number: u16) -> bool {
        if let Some(body) = self.body(number) {
            return self.clear_los4(me, &body);
        }
        let Some(origin) = self.entity_origin(me, number) else {
            return false;
        };
        // `G_ClearLOS3`: a clientless entity's spots are its origin.
        let eyes = spot(&self.npc(me), Spot::HeadLean);
        crate::npc_senses::clear_los(&mut self.senses(), eyes, origin)
    }

    /// `ent->r.currentOrigin` of whatever the NPC may be after: a client, its own goal
    /// entity, or another linked entity (a dropped saber) as the host knows it.
    pub(crate) fn entity_origin(&self, me: usize, number: u16) -> Option<[f32; 3]> {
        if let Some(body) = self.body(number) {
            return Some(body.origin);
        }
        let npc = &self.actors[me];
        if npc.goal == Some(number) {
            return Some(npc.mind.tactics.temp_goal.origin);
        }
        self.host.entity_box(number).map(|(origin, _, _)| origin)
    }

    /// `NPC_FaceEntity` (`NPC_utils.c:1571-1580`): the body's leaning head faced. Whether it
    /// now faces it.
    pub fn face_entity(
        &mut self,
        me: usize,
        target: &Body,
        do_pitch: bool,
        command: &mut UserCommand,
    ) -> bool {
        self.face_position(me, spot(target, Spot::HeadLean), do_pitch, command)
    }

    /// `NPC_FaceEntity(ent, qtrue)` for any entity: `CalcEntitySpot(SPOT_HEAD_LEAN)` is a
    /// clientless entity's origin.
    pub(crate) fn face_entity_number(
        &mut self,
        me: usize,
        number: u16,
        command: &mut UserCommand,
    ) -> bool {
        if let Some(body) = self.body(number) {
            return self.face_entity(me, &body, true, command);
        }
        match self.entity_origin(me, number) {
            Some(origin) => self.face_position(me, origin, true, command),
            None => false,
        }
    }

    /// `NPC_ClearLookTarget` (`NPC_utils.c:1626-1640`): unless a monster holds it.
    pub(crate) fn clear_look(&mut self, me: usize) {
        let npc = &mut self.actors[me];
        if npc.player.raw_field(PS_EFLAGS2).unwrap_or(0) & EF2_HELD_BY_MONSTER == 0 {
            npc.mind.clear_look_target();
        }
    }

    /// The `at`th client in entity-number order — the players, then the NPCs — as the
    /// reference's loops over `g_entities` meet them.
    fn nth_client(&self, at: usize) -> Option<Body> {
        let players = self.host.players();
        match players.get(at) {
            Some(player) => Some(*player),
            None => self.order.get(at - players.len()).map(|&npc| self.npc(npc)),
        }
    }

    /// `WP_ResistForcePush` (`NPC_AI_Jedi.c:223-291`): the push resisted — the whole body
    /// braced on the ground (and held still a second), the torso alone when running
    /// (masters) or in the air, rolling, flipping or down — with the push effect on its
    /// hand and a voice.
    pub fn wp_resist_force_push(&mut self, me: usize, pusher: u16, no_penalty: bool) {
        if self.actors[me].health <= 0 || self.body(pusher).is_none() {
            return;
        }
        let npc = &self.actors[me];
        let velocity = npc.player.velocity();
        let master = npc.definition.client_class == CLASS_DESANN
            || npc.npc_type.eq_ignore_ascii_case(b"yoda")
            || npc.definition.client_class == CLASS_LUKE;
        let running_resist = master
            && (velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2]
                > 10_000.0
                || npc.force_levels[FP_PUSH] >= FORCE_LEVEL_3
                || npc.force_levels[FP_PULL] >= FORCE_LEVEL_3);
        let legs = npc.player.leg_animation();
        let braced = !running_resist
            && npc.player.ground_entity_num() != ENTITYNUM_NONE
            && !crate::saber_rules::spinning(legs)
            && !crate::saber_rules::flipping(legs)
            && !crate::npc_pain::rolling(legs)
            && !crate::pmove_hand_extend::in_knockdown(legs, npc.player.legs_timer())
            && !crate::npc_pain::crouching(legs);
        let parts = if braced { SETANIM_BOTH } else { SETANIM_TORSO };
        self.set_animation(
            me,
            parts,
            BOTH_RESISTPUSH,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        );
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        if !no_penalty {
            let sped = npc.player.force_powers_active() & (1 << crate::force_powers::FP_SPEED) != 0;
            // `floor(weaponTime * tFVal)`: an int times a float, floored as a double.
            let scaled = |time: i32| {
                if sped {
                    f64::from(time as f32 * TIMESCALE).floor() as i32
                } else {
                    time
                }
            };
            if running_resist {
                npc.player.set_raw_field(PS_WEAPON_TIME, scaled(600) as u32);
            } else {
                npc.player.set_velocity([0.0; 3]);
                let weapon_time = scaled(1_000);
                npc.player.set_raw_field(PS_WEAPON_TIME, weapon_time as u32);
                npc.player.set_movement_time(weapon_time as i16);
                npc.player
                    .set_movement_flags(npc.player.movement_flags() | PMF_TIME_KNOCKBACK);
            }
        }
        npc.player.powerups[PW_DISINT_4] = (level_time + npc.player.torso_timer() + 500) as u32;
        npc.player.powerups[PW_PULL] = 0;
        self.jedi_play_blocked_push_sound(me);
    }
}
