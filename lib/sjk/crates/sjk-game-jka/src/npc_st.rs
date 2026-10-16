//! The stormtroopers' AI (`codemp/game/NPC_AI_Stormtrooper.c`) out of a fight: the
//! behaviour state (`NPC_BSST_Default`), patrolling and noticing the enemy by sight
//! (`NPC_BSST_Patrol`, `NPC_CheckPlayerTeamStealth`, `NPC_CheckEnemyStealth`), sleeping
//! (`NPC_BSST_Sleep`), investigating what it noticed (`NPC_BSST_Investigate`,
//! `NPC_ST_InvestigateEvent`, `ST_LookAround`), and what the squads say (`ST_Speech`, by the
//! group's or the trooper's own debounce) and do under fire (`ST_MarkToCover`,
//! `ST_StartFlee`, `NPC_StartFlee`). The fight is [`crate::npc_st_attack`], the squad's
//! decisions [`crate::npc_st_commander`].
//!
//! The same AI runs for every NPC `NPC_RunBehavior` gives it: the enemy team's armed
//! soldiers (stormtroopers, imperials, their officers and workers, swamptroopers,
//! trandoshans, reelos and the rest), and — through `NPC_BSDefault`'s fight — the player's
//! allies (rebels, Jan, Lando, prisoners).
//!
//! Alerts are checked as the reference checks them, and none is ever found (see
//! [`crate::npc_senses`]): what a trooper would investigate or wake to is ported, and
//! reached only when a script has set the state.
//!
//! Held to `tools/game-oracle/npcst.c` (`game-npcst.txt`).

use crate::npc_senses::{
    AEL_DANGER, AEL_DISCOVERED, AEL_MINOR, AlertKind, Body, Spot, distance_squared, in_fov, spot,
};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use sjk_protocol::UserCommand;

/// `SPEECH_*` (`NPC_AI_Stormtrooper.c:126-142`).
pub mod speech {
    pub const CHASE: i32 = 0;
    pub const CONFUSED: i32 = 1;
    pub const COVER: i32 = 2;
    pub const DETECTED: i32 = 3;
    pub const GIVEUP: i32 = 4;
    pub const LOOK: i32 = 5;
    pub const LOST: i32 = 6;
    pub const OUTFLANK: i32 = 7;
    pub const ESCAPING: i32 = 8;
    pub const SIGHT: i32 = 9;
    pub const SOUND: i32 = 10;
    pub const SUSPICIOUS: i32 = 11;
    pub const YELL: i32 = 12;
    pub const PUSHED: i32 = 13;
}
/// `LSTATE_NONE`, `LSTATE_UNDERFIRE`, `LSTATE_INVESTIGATE`.
pub(crate) const LSTATE_NONE: i32 = 0;
pub(crate) const LSTATE_UNDERFIRE: i32 = 1;
const LSTATE_INVESTIGATE: i32 = 2;
/// `SQUAD_*` (`ai.h:39-49`).
pub mod squad {
    pub const IDLE: i32 = 0;
    pub const STAND_AND_SHOOT: i32 = 1;
    pub const RETREAT: i32 = 2;
    pub const COVER: i32 = 3;
    pub const TRANSITION: i32 = 4;
    pub const POINT: i32 = 5;
    pub const SCOUT: i32 = 6;
}
/// The script flags the AI reads.
pub(crate) const SCF_CHASE_ENEMIES: u32 = 0x400;
pub(crate) const SCF_LOOK_FOR_ENEMIES: u32 = 0x800;
pub(crate) const SCF_IGNORE_ALERTS: u32 = 0x2000;
pub(crate) const SCF_FIRE_WEAPON: u32 = 0x4_0000;
pub(crate) const SCF_DONT_FLEE: u32 = 0x8000;
pub(crate) const SCF_DONT_FIRE: u32 = 0x4000;
pub(crate) const SCF_USE_CP_NEAREST: u32 = 0x10_0000;
const SCF_RUNNING: u32 = 0x20;
/// `BS_DEFAULT`, `BS_HUNT_AND_KILL`, `BS_INVESTIGATE`, `BS_FLEE`.
const BS_DEFAULT: i32 = 0;
const BS_HUNT_AND_KILL: i32 = 15;
const BS_INVESTIGATE: i32 = 13;
const BS_FLEE: i32 = 16;
/// `NPCTEAM_PLAYER`.
const NPCTEAM_PLAYER: i32 = 2;
/// Classes: `CLASS_ATST`, `CLASS_IMPERIAL`, `CLASS_IMPWORKER`, `CLASS_PROTOCOL`,
/// `CLASS_SWAMPTROOPER`.
const CLASS_ATST: i32 = 1;
pub(crate) const CLASS_IMPERIAL: i32 = 14;
const CLASS_IMPWORKER: i32 = 15;
const CLASS_PROTOCOL: i32 = 33;
const CLASS_SWAMPTROOPER: i32 = 46;
/// `RANK_LT`.
const RANK_LT: i32 = 4;
/// `BUTTON_WALKING`; `WP_NONE`, `WP_SABER`.
const BUTTON_WALKING: u16 = 16;
const WP_NONE: i32 = 0;
const WP_SABER: i32 = 3;
/// `BOTH_STAND4`; `SETANIM_BOTH`, `SETANIM_TORSO`; `SETANIM_FLAG_OVERRIDE|HOLD`.
const BOTH_STAND4: u16 = 922;
/// `ps.torsoTimer`, `ps.legsTimer`, `ps.weaponstate`; `s.angles[YAW]`.
const PS_TORSO_TIMER: usize = 20;
const PS_LEGS_TIMER: usize = 21;
const PS_WEAPON_STATE: usize = 33;
const PS_WEAPON: usize = 47;
/// `EF_DEAD`.
const EF_DEAD: u32 = 1;
const ES_ANGLES_YAW: usize = 9;
/// `CONTENTS_BODY`, `CONTENTS_BOTCLIP`.
const CONTENTS_BODY: u32 = 0x100;
const CONTENTS_BOTCLIP: u32 = 0x40;
/// `MAX_VIEW_DIST`, `MAX_VIEW_SPEED`, `DISTANCE_THRESHOLD`, `REALIZE_THRESHOLD`,
/// `CAUTIOUS_THRESHOLD` and the scales of the stealth rating (`NPC_AI_Stormtrooper.c:43-62`).
const MAX_VIEW_DIST: f32 = 1_024.0;
const MAX_VIEW_SPEED: f32 = 250.0;
const DISTANCE_THRESHOLD: f32 = 0.075;
const REALIZE_THRESHOLD: f32 = 0.6;
const CAUTIOUS_THRESHOLD: f64 = 0.6 * 0.75;
/// `ST_MIN_LIGHT_THRESHOLD`, `ST_MAX_LIGHT_THRESHOLD`.
const ST_MIN_LIGHT_THRESHOLD: i32 = 30;
const ST_MAX_LIGHT_THRESHOLD: i32 = 180;

/// The events each speech is said with (`ST_Speech`'s switch): the first and the last.
fn speech_events(kind: i32) -> Option<(i32, i32)> {
    Some(match kind {
        speech::CHASE => (133, 135),
        speech::CONFUSED => (122, 124),
        speech::COVER => (136, 140),
        speech::DETECTED => (141, 145),
        speech::GIVEUP => (152, 155),
        speech::LOOK => (156, 157),
        speech::LOST => (146, 146),
        speech::OUTFLANK => (147, 148),
        speech::ESCAPING => (149, 151),
        speech::SIGHT => (158, 160),
        speech::SOUND => (161, 163),
        speech::SUSPICIOUS => (164, 168),
        speech::YELL => (116, 118),
        speech::PUSHED => (125, 127),
        _ => return None,
    })
}

/// `NPC_GetHFOVPercentage` and `NPC_GetVFOVPercentage` (`NPC_senses.c:846-884`): how near
/// the middle of the field of view `point` is, 0 outside it.
pub(crate) fn fov_percentage(
    point: [f32; 3],
    from: [f32; 3],
    facing: f32,
    fov: f32,
    axis: usize,
) -> f32 {
    let angles = crate::player_angle_math::vector_angles(crate::npc_senses::subtract(point, from));
    let delta = crate::npc_senses::angle_delta(facing, angles[axis]).abs();
    if delta > fov {
        0.0
    } else {
        (fov - delta) / fov
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSST_Default` (`NPC_AI_Stormtrooper.c:2764-2779`).
    pub fn bs_st_default(&mut self, me: usize, command: &mut UserCommand) {
        if self.actors[me].script_flags & SCF_FIRE_WEAPON != 0 {
            self.weapon_think(me, command);
        }
        if self.actors[me].mind.enemy.is_none() {
            self.bs_st_patrol(me, command);
            return;
        }
        if self.npc(me).weapon == WP_NONE {
            // `NPC_CheckGetNewWeapon` ([`crate::npc_weapon_pickup`]).
            self.check_get_new_weapon_or_stub(me);
        }
        self.bs_st_attack(me, command);
    }

    /// `ST_AggressionAdjust` (`NPC_AI_Stormtrooper.c:77-106`): 1..7 for the player's side,
    /// 3..10 for the rest.
    pub fn aggression_adjust(&mut self, at: usize, change: i32) {
        let npc = &mut self.actors[at];
        let (low, high) = if npc.player_team == NPCTEAM_PLAYER {
            (1, 7)
        } else {
            (3, 10)
        };
        let stats = &mut npc.definition.stats;
        stats.aggression += change;
        if stats.aggression > high {
            stats.aggression = high;
        } else if stats.aggression < low {
            stats.aggression = low;
        }
    }

    /// `ST_Speech` (`NPC_AI_Stormtrooper.c:144-236`): a line said — unless it fails its
    /// chance, or the group (or the trooper, or its team) spoke too lately; a negative
    /// chance always speaks.
    pub fn st_speech(&mut self, at: usize, kind: i32, fail_chance: f32) {
        let level_time = self.level_time;
        if self.host.rng().flrand(0.0, 1.0) < fail_chance {
            return;
        }
        let group = self.actors[at].mind.tactics.group;
        let team = self.actors[at].player_team;
        let team_debounce = usize::try_from(team)
            .ok()
            .and_then(|team| self.level.group_speech.get(team).copied())
            .unwrap_or(0);
        if fail_chance >= 0.0 {
            match group {
                Some(group) if self.level.groups[group].speech_debounce_time > level_time => return,
                Some(_) => {}
                None if !self.actors[at].mind.timers.done("chatter", level_time)
                    || team_debounce > level_time =>
                {
                    return;
                }
                None => {}
            }
        }
        let wait = self.host.irand(2_000, 4_000);
        match group {
            Some(group) => self.level.groups[group].speech_debounce_time = level_time + wait,
            None => self.actors[at].mind.timers.set("chatter", level_time, wait),
        }
        let wait = self.host.irand(2_000, 4_000);
        if let Some(slot) = usize::try_from(team)
            .ok()
            .and_then(|team| self.level.group_speech.get_mut(team))
        {
            *slot = level_time + wait;
        }
        if self.actors[at].mind.blocked_speech_until > level_time {
            return;
        }
        if let Some((first, last)) = speech_events(kind) {
            let event = if first == last {
                first
            } else {
                self.host.irand(first, last)
            };
            self.add_voice(at, event, 2_000);
        }
        self.actors[at].mind.blocked_speech_until = level_time + 2_000;
    }

    /// `ST_MarkToCover` (`NPC_AI_Stormtrooper.c:238-252`).
    pub fn mark_to_cover(&mut self, at: usize) {
        let level_time = self.level_time;
        self.actors[at].mind.fight.local_state = LSTATE_UNDERFIRE;
        let delay = self.host.irand(500, 2_500);
        self.actors[at]
            .mind
            .timers
            .set("attackDelay", level_time, delay);
        self.aggression_adjust(at, -3);
        if self.actors[at]
            .mind
            .tactics
            .group
            .is_some_and(|group| self.level.groups[group].len() > 1)
        {
            self.st_speech(at, speech::COVER, 0.0);
        }
    }

    /// `ST_StartFlee` (`NPC_AI_Stormtrooper.c:254-265`): `G_StartFlee`, and a word of it in
    /// a squad.
    pub fn st_start_flee(
        &mut self,
        at: usize,
        enemy: Option<u16>,
        danger: [f32; 3],
        level: u32,
        least: i32,
        most: i32,
    ) {
        self.start_flee(at, enemy, danger, level, least, most);
        if self.actors[at]
            .mind
            .tactics
            .group
            .is_some_and(|group| self.level.groups[group].len() > 1)
        {
            self.st_speech(at, speech::COVER, 0.0);
        }
    }

    /// `NPC_StartFlee` (`NPC_behavior.c:1581-1655`): off to a combat point away from the
    /// danger — out of its sight if in great danger, unarmed or alone and hurt — or, unarmed
    /// with none, running (`BS_FLEE`); an armed NPC with no point stays.
    pub fn start_flee(
        &mut self,
        at: usize,
        enemy: Option<u16>,
        danger: [f32; 3],
        level: u32,
        least: i32,
        most: i32,
    ) {
        use crate::npc_combat_points::{PointSearch, cp};
        if let Some(enemy) = enemy {
            self.set_enemy(at, enemy);
        }
        let npc = &self.actors[at];
        let origin = npc.current_origin;
        let alone = npc
            .mind
            .tactics
            .group
            .is_none_or(|group| self.level.groups[group].len() <= 1);
        let unarmed = self.npc(at).weapon == WP_NONE;
        let search = |flags| PointSearch {
            position: origin,
            enemy: danger,
            flags,
            avoid_distance: 128.0,
            ignore: -1,
        };
        let mut point = -1;
        if level > AEL_DANGER || unarmed || (alone && npc.health <= 10) {
            point = self.find_combat_point(
                at,
                search(cp::COVER | cp::AVOID | cp::HAS_ROUTE | cp::NO_PVS),
            );
        }
        for flags in [
            cp::COVER | cp::AVOID | cp::HAS_ROUTE,
            cp::COVER | cp::HAS_ROUTE,
            cp::HAS_ROUTE,
        ] {
            if point != -1 {
                break;
            }
            point = self.find_combat_point(at, search(flags));
        }
        if point != -1 {
            self.set_combat_point(at, point);
            let spot = self.level.combat_points[point as usize].origin;
            self.set_move_goal(at, spot, 8, true, point, None);
            let npc = &mut self.actors[at];
            npc.behavior_state = BS_HUNT_AND_KILL;
            npc.mind.temp_behavior = BS_DEFAULT;
        } else if !unarmed {
            return;
        } else {
            self.actors[at].mind.temp_behavior = BS_FLEE;
            self.set_move_goal(at, danger, 0, true, -1, None);
            self.actors[at].mind.tactics.investigate_goal = danger;
        }
        let level_time = self.level_time;
        let delay = self.host.irand(500, 2_500);
        self.actors[at]
            .mind
            .timers
            .set("attackDelay", level_time, delay);
        self.actors[at].mind.tactics.squad_state = squad::RETREAT;
        let flee = self.host.irand(least, most);
        self.actors[at].mind.timers.set("flee", level_time, flee);
        let panic = self.host.irand(1_000, 4_000);
        self.actors[at].mind.timers.set("panic", level_time, panic);
        if self.actors[at].definition.client_class != CLASS_PROTOCOL {
            self.actors[at].mind.timers.set("duck", level_time, 0);
        }
    }

    /// `NPC_CheckAlertEvents` (`NPC_senses.c:554-557`) for the NPC at `me`: the alert it
    /// notices, if any — `ignore` an index as the reference passes it (an alert's ID).
    pub fn check_alerts(
        &mut self,
        me: usize,
        ignore: i32,
        needs_owner: bool,
        min_level: u32,
    ) -> Option<usize> {
        let body = self.npc(me);
        let sight = self.sight(me);
        let hear = self.actors[me].definition.stats.earshot;
        let alive = self.body(0).is_some_and(|player| player.health > 0);
        let Self { alerts, host, .. } = self;
        let mut senses = crate::npc_world::HostSenses(&mut **host);
        alerts.check(
            &mut senses,
            &body,
            sight,
            hear,
            usize::try_from(ignore).ok(),
            needs_owner,
            min_level,
            alive,
        )
    }

    /// `G_CheckForDanger` (`NPC_senses.c:559-588`): a dangerous alert not of the NPC's own
    /// side makes it flee (unless it may not). Whether it does.
    pub fn check_for_danger(&mut self, me: usize, alert: Option<usize>) -> bool {
        let Some(alert) = alert.map(|at| self.alerts.events()[at]) else {
            return false;
        };
        if alert.level < AEL_DANGER {
            return false;
        }
        let owner = alert.owner.and_then(|owner| self.body(owner));
        let npc = &self.actors[me];
        if owner
            .is_some_and(|owner| owner.number == npc.number || owner.player_team == npc.player_team)
        {
            return false;
        }
        if npc.script_flags & SCF_DONT_FLEE != 0 {
            return false;
        }
        self.start_flee(me, alert.owner, alert.position, alert.level, 3_000, 6_000);
        true
    }

    /// `NPC_BSST_Sleep` (`NPC_AI_Stormtrooper.c:456-500`): woken only by what it hears.
    pub fn bs_st_sleep(&mut self, me: usize) {
        let Some(alert) = self.check_alerts(me, -1, false, AEL_MINOR) else {
            return;
        };
        let event = self.alerts.events()[alert];
        if event.level == AEL_DISCOVERED && self.actors[me].script_flags & SCF_LOOK_FOR_ENEMIES != 0
        {
            let origin = self.actors[me].current_origin;
            let mut best: Option<(u16, f32)> = None;
            for index in 0..self.host.players().len() {
                let player = self.host.players()[index];
                if player.health <= 0 || player.entity_flags & EF_DEAD != 0 {
                    continue;
                }
                if !crate::npc_senses::clear_los(&mut self.senses(), origin, player.origin) {
                    continue;
                }
                let distance = distance_squared(origin, player.origin).sqrt();
                if distance < best.map_or(16_384.0, |(_, best)| best) {
                    best = Some((player.number, distance));
                }
            }
            if let Some((player, _)) = best {
                self.set_enemy(me, player);
                return;
            }
        }
        self.sleep_shuffle(me);
    }

    /// `NPC_ST_SleepShuffle` (`NPC_AI_Stormtrooper.c:419-454`).
    fn sleep_shuffle(&mut self, me: usize) {
        let level_time = self.level_time;
        let timers = &mut self.actors[me].mind.timers;
        if timers.done("shuffleTime", level_time) {
            timers.set("shuffleTime", level_time, 4_000);
            timers.set("sleepTime", level_time, 2_000);
            return;
        }
        if timers.done("sleepTime", level_time) {
            self.check_player_team_stealth(me);
            self.actors[me]
                .mind
                .timers
                .set("sleepTime", level_time, 2_000);
        }
    }

    /// `NPC_CheckPlayerTeamStealth` (`NPC_AI_Stormtrooper.c:725-754`): each valid enemy on
    /// the team it hates, in entity order, looked for until one is noticed.
    pub fn check_player_team_stealth(&mut self, me: usize) -> bool {
        let players = self.host.players().len();
        for index in 0..players + self.order.len() {
            let body = if index < players {
                self.host.players()[index]
            } else {
                self.npc(self.order[index - players])
            };
            if self.valid_for(me, &body)
                && body.player_team == self.actors[me].enemy_team
                && self.check_enemy_stealth(me, &body)
            {
                return true;
            }
        }
        false
    }

    /// `NPC_CheckEnemyStealth` (`NPC_AI_Stormtrooper.c:507-723`): whether `target` is
    /// noticed — at once when very close, else in view and sight by how far, how central,
    /// how lit and how fast it is; almost noticed makes the trooper wary and look.
    ///
    /// Multiplayer rates the turning at 5 (`turning_rating`, a constant), which alone puts
    /// the rating past the threshold: whatever else the rating adds (the water's or fog's
    /// bonus, crouching) cannot change what is decided, and is left out.
    fn check_enemy_stealth(&mut self, me: usize, target: &Body) -> bool {
        let level_time = self.level_time;
        if self.actors[me].mind.enemy.is_some() {
            return true;
        }
        if target.flags & crate::npc_spawn::FL_NOTARGET != 0 || target.health <= 0 {
            return false;
        }
        let min_distance: f32 =
            if target.weapon == WP_SABER && !target.saber_holstered && !target.saber_in_flight {
                100.0
            } else {
                40.0
            };
        let npc = &self.actors[me];
        let flags = npc.script_flags;
        let distance = distance_squared(target.origin, npc.current_origin);
        if !target.ducked
            && flags & SCF_LOOK_FOR_ENEMIES != 0
            && distance < min_distance * min_distance
        {
            self.notice(me, target.number, true, None);
            return true;
        }
        let max_view = npc.definition.stats.visrange.max(MAX_VIEW_DIST);
        if distance > max_view * max_view {
            return false;
        }
        let sight = self.sight(me);
        if !in_fov(target, &self.npc(me), sight.hfov, sight.vfov) || !self.clear_los4(me, target) {
            return false;
        }
        if target.class == CLASS_ATST {
            self.notice(me, target.number, false, None);
            return true;
        }
        let npc = &self.actors[me];
        let head = [
            target.origin[0],
            target.origin[1],
            target.origin[2] + target.maxs[2] - 4.0,
        ];
        let eyes = npc.mind.eye_angles;
        let horizontal = fov_percentage(head, npc.mind.eye_point, eyes[1], sight.hfov as f32, 1);
        let vertical = fov_percentage(head, npc.mind.eye_point, eyes[0], sight.vfov as f32, 0);
        let (horizontal, vertical) = (horizontal * (horizontal * horizontal), vertical * vertical);
        let target_distance = distance_squared(target.origin, npc.current_origin).sqrt();
        let [x, y, z] = target.velocity;
        let speed = ((x * x + y * y + z * z).sqrt() / MAX_VIEW_SPEED).min(1.0);
        let distance_rating = target_distance / max_view;
        let fov_perc = 1.0 - (horizontal + vertical) * 0.5;
        if distance_rating < DISTANCE_THRESHOLD {
            self.notice(me, target.number, false, None);
            return true;
        }
        if distance_rating > 1.0 {
            return false;
        }
        let mut rating =
            0.35 * (1.0 - distance_rating) + 0.40 * (1.0 - fov_perc) + (1.0 - 0.5) * 0.25;
        rating += speed * 0.25;
        rating += 5.0 * 0.25;
        let swamp = npc.definition.client_class == CLASS_SWAMPTROOPER;
        let realize = if swamp {
            CAUTIOUS_THRESHOLD as f32
        } else {
            REALIZE_THRESHOLD
        };
        let cautious = (CAUTIOUS_THRESHOLD as f32) * 0.75;
        if rating > realize && flags & SCF_LOOK_FOR_ENEMIES != 0 {
            self.notice(me, target.number, true, None);
            return true;
        }
        if rating <= cautious || flags & SCF_IGNORE_ALERTS != 0 {
            return false;
        }
        if self.actors[me]
            .mind
            .timers
            .done("enemyLastVisible", level_time)
        {
            let look = self.host.irand(4_500, 8_500);
            self.actors[me]
                .mind
                .timers
                .set("enemyLastVisible", level_time, look);
            self.st_speech(me, speech::SIGHT, 0.0);
            self.temp_look_target(me, target.number, look, look);
            return false;
        }
        let deadline = self.actors[me]
            .mind
            .timers
            .get("enemyLastVisible")
            .unwrap_or(-1);
        if deadline > level_time + 500 || flags & SCF_LOOK_FOR_ENEMIES == 0 {
            return false;
        }
        if self.actors[me].definition.rank < RANK_LT && self.host.irand(0, 2) == 0 {
            let interrogate = self.host.irand(2_000, 4_000);
            self.st_speech(me, speech::SUSPICIOUS, 0.0);
            self.actors[me]
                .mind
                .timers
                .set("interrogating", level_time, interrogate);
            self.notice(me, target.number, true, Some((interrogate, interrogate)));
        } else {
            self.notice(me, target.number, true, None);
            let stand = self.host.irand(500, 2_500);
            self.actors[me].mind.timers.set("stand", level_time, stand);
        }
        true
    }

    /// An enemy noticed (`NPC_CheckEnemyStealth`'s outcomes): taken, perhaps seen now
    /// (`enemyLastSeenTime`), and the attack delayed — 0.5–2.5 s, or as given with the
    /// stand timer too.
    fn notice(&mut self, me: usize, enemy: u16, seen: bool, fixed: Option<(i32, i32)>) {
        let level_time = self.level_time;
        self.set_enemy(me, enemy);
        if seen {
            self.actors[me].mind.tactics.enemy_last_seen_time = level_time;
        }
        let (delay, stand) = match fixed {
            Some(times) => times,
            None => (self.host.irand(500, 2_500), 0),
        };
        self.actors[me]
            .mind
            .timers
            .set("attackDelay", level_time, delay);
        if fixed.is_some() {
            self.actors[me].mind.timers.set("stand", level_time, stand);
        }
    }

    /// `NPC_BSST_Patrol` (`NPC_AI_Stormtrooper.c:1100-1198`).
    pub fn bs_st_patrol(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        self.get_group(me);
        let flags = self.actors[me].script_flags;
        if self.actors[me].mind.confusion_time < level_time
            && flags & SCF_LOOK_FOR_ENEMIES != 0
            && self.check_player_team_stealth(me)
        {
            self.update_angles(me, true, true, command);
            return;
        }
        if flags & SCF_IGNORE_ALERTS == 0
            && let Some(alert) = self.check_alerts(me, -1, false, AEL_MINOR)
            && self.investigate_event(me, alert, false)
        {
            self.update_angles(me, true, true, command);
            return;
        }
        let class = self.actors[me].definition.client_class;
        let imperial = class == CLASS_IMPERIAL || class == CLASS_IMPWORKER;
        if self.update_goal(me, command).is_some() {
            command.buttons |= BUTTON_WALKING;
            self.move_to_goal(me, true, command);
        } else if !imperial
            && self.actors[me]
                .mind
                .timers
                .done("enemyLastVisible", level_time)
        {
            if self.host.irand(0, 30) == 0 {
                let yaw =
                    f32::from_bits(self.actors[me].state.raw_field(ES_ANGLES_YAW).unwrap_or(0));
                let turn = self.host.irand(-90, 90);
                self.actors[me].desired_yaw = yaw + turn as f32;
            }
            if self.host.irand(0, 30) == 0 {
                let pitch = self.host.irand(-20, 20);
                self.actors[me].mind.desired_pitch = pitch as f32;
            }
        }
        self.update_angles(me, true, true, command);
        if imperial {
            self.imperial_stance(me, command);
        }
    }

    /// `NPC_BSST_Patrol`'s imperials (`NPC_AI_Stormtrooper.c:1157-1197`): their stand
    /// animation held, and their weapon put away.
    fn imperial_stance(&mut self, me: usize, command: &UserCommand) {
        use crate::pmove_anim::{
            SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_TORSO,
        };
        let npc = &self.actors[me];
        let (torso, legs) = (npc.player.torso_animation(), npc.player.leg_animation());
        let torso_timer = npc.player.raw_field(PS_TORSO_TIMER).unwrap_or(0) as i32;
        let legs_timer = npc.player.raw_field(PS_LEGS_TIMER).unwrap_or(0) as i32;
        let torso_free = torso_timer <= 0 || torso == BOTH_STAND4;
        if command.forward_move != 0 || command.right_move != 0 || command.up_move != 0 {
            if torso_free
                && command.buttons & BUTTON_WALKING != 0
                && npc.script_flags & SCF_RUNNING == 0
            {
                self.set_animation(
                    me,
                    SETANIM_TORSO,
                    BOTH_STAND4,
                    SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                );
                self.actors[me].player.set_raw_field(PS_TORSO_TIMER, 200);
            }
        } else if torso_free && (legs_timer <= 0 || legs == BOTH_STAND4) {
            self.set_animation(
                me,
                SETANIM_BOTH,
                BOTH_STAND4,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            self.actors[me].player.set_raw_field(PS_TORSO_TIMER, 200);
            self.actors[me].player.set_raw_field(PS_LEGS_TIMER, 200);
        }
        if self.actors[me].player.weapon() != 0 {
            let skill = self.host.skill();
            let npc = &mut self.actors[me];
            crate::npc_combat::change_weapon(npc, 0, skill);
            npc.player.set_raw_field(PS_WEAPON, 0);
            npc.player.set_raw_field(PS_WEAPON_STATE, 0);
        }
    }

    /// `NPC_ST_InvestigateEvent` (`NPC_AI_Stormtrooper.c:756-925`): an alert looked into —
    /// an enemy that gave itself away taken, anything else looked at or walked to.
    fn investigate_event(&mut self, me: usize, alert: usize, extra_suspicious: bool) -> bool {
        let level_time = self.level_time;
        let event = self.alerts.events()[alert];
        let flags = self.actors[me].script_flags;
        if self.actors[me].mind.confusion_time < level_time
            && event.level == AEL_DISCOVERED
            && flags & SCF_LOOK_FOR_ENEMIES != 0
        {
            self.actors[me].mind.last_alert_id = event.id;
            let owner = event.owner.and_then(|owner| self.body(owner));
            let Some(owner) = owner.filter(|owner| {
                owner.health > 0 && owner.player_team == self.actors[me].enemy_team
            }) else {
                return false;
            };
            self.notice(me, owner.number, true, None);
            if event.kind == AlertKind::Sound {
                let roam = self.host.irand(500, 2_500);
                self.actors[me]
                    .mind
                    .timers
                    .set("roamTime", level_time, roam);
            }
            return true;
        }
        if event.id == self.actors[me].mind.last_alert_id {
            return false;
        }
        self.actors[me].mind.last_alert_id = event.id;
        if event.kind == AlertKind::Sight
            && event.light
                < self
                    .host
                    .irand(ST_MIN_LIGHT_THRESHOLD, ST_MAX_LIGHT_THRESHOLD) as f32
        {
            return false;
        }
        let tactics = &mut self.actors[me].mind.tactics;
        tactics.investigate_goal = event.position;
        tactics.investigate_count =
            (tactics.investigate_count + if extra_suspicious { 2 } else { 1 }).min(4);
        if event.level > AEL_MINOR
            && tactics.investigate_count > 1
            && flags & SCF_CHASE_ENEMIES != 0
        {
            self.walk_to_investigate(me);
            let tactics = self.actors[me].mind.tactics;
            if tactics.investigate_debounce_time + tactics.pause_time > level_time {
                let commander = self.imperial_commander(me);
                if let Some(commander) = commander.filter(|_| self.host.irand(0, 3) == 0) {
                    self.st_speech(commander, speech::LOOK, 0.0);
                } else {
                    self.st_speech(me, speech::LOOK, 0.0);
                }
            } else {
                self.speak_of(me, event.kind);
            }
            let tactics = &mut self.actors[me].mind.tactics;
            tactics.investigate_debounce_time = tactics.investigate_count * 5_000;
            tactics.investigate_sound_debounce_time = level_time + 2_000;
            tactics.pause_time = level_time;
        } else {
            self.speak_of(me, event.kind);
            let tactics = &mut self.actors[me].mind.tactics;
            tactics.investigate_debounce_time = tactics.investigate_count * 1_000;
            tactics.investigate_sound_debounce_time = level_time + 1_000;
            tactics.pause_time = level_time;
            tactics.investigate_goal = event.position;
        }
        if event.level >= AEL_DANGER {
            let debounce = self.host.irand(500, 2_500);
            self.actors[me].mind.tactics.investigate_debounce_time = debounce;
        }
        self.actors[me].mind.temp_behavior = BS_INVESTIGATE;
        true
    }

    /// The group's imperial commander, if it has one (`NPC_ST_SayMovementSpeech`'s and the
    /// investigation's test).
    pub(crate) fn imperial_commander(&self, me: usize) -> Option<usize> {
        let group = self.actors[me].mind.tactics.group?;
        let commander = self.actor_at(self.level.groups[group].commander?)?;
        (self.actors[commander].definition.client_class == CLASS_IMPERIAL).then_some(commander)
    }

    /// The sight or the sound spoken of.
    fn speak_of(&mut self, me: usize, kind: AlertKind) {
        match kind {
            AlertKind::Sight => self.st_speech(me, speech::SIGHT, 0.0),
            AlertKind::Sound => self.st_speech(me, speech::SOUND, 0.0),
        }
    }

    /// `NPC_ST_InvestigateEvent`'s walk (`NPC_AI_Stormtrooper.c:826-862`): to the spot put
    /// on the ground, or to an investigation combat point by it.
    fn walk_to_investigate(&mut self, me: usize) {
        use crate::npc_combat_points::{PointSearch, cp};
        let npc = &self.actors[me];
        let (mins, maxs, number) = (npc.mins, npc.maxs, npc.number);
        let clip = (npc.clip_mask & !CONTENTS_BODY) | CONTENTS_BOTCLIP;
        let mut goal = npc.mind.tactics.investigate_goal;
        if self.expand_point_to_bbox(&mut goal, mins, maxs, number, clip) {
            self.actors[me].mind.tactics.investigate_goal = goal;
            let mut end = goal;
            end[2] -= 512.0;
            let trace = self.trace_bodies(
                goal,
                mins,
                maxs,
                end,
                crate::npc_spawn::ENTITYNUM_NONE,
                clip,
            );
            if trace.fraction < 1.0 {
                self.actors[me].mind.tactics.investigate_goal = trace.end_position;
                self.set_move_goal(me, trace.end_position, 16, true, -1, None);
                self.actors[me].mind.fight.local_state = LSTATE_INVESTIGATE;
            }
        } else {
            let search = PointSearch {
                position: goal,
                enemy: goal,
                flags: cp::INVESTIGATE | cp::HAS_ROUTE,
                avoid_distance: 0.0,
                ignore: -1,
            };
            let point = self.find_combat_point(me, search);
            if point != -1 {
                let spot = self.level.combat_points[point as usize].origin;
                self.set_move_goal(me, spot, 16, true, point, None);
                self.actors[me].mind.fight.local_state = LSTATE_INVESTIGATE;
            }
        }
    }

    /// `ST_LookAround` (`NPC_AI_Stormtrooper.c:970-1003`): the spot, straight above it, then
    /// 45° right, then left, through the investigation's time.
    fn look_around(&mut self, me: usize, command: &mut UserCommand) {
        let tactics = self.actors[me].mind.tactics;
        let perc = (self.level_time - tactics.pause_time) as f32
            / tactics.investigate_debounce_time as f32;
        let position = if perc < 0.25 {
            tactics.investigate_goal
        } else {
            let offset = if perc < 0.5 {
                0.0
            } else if perc < 0.75 {
                45.0
            } else {
                -45.0
            };
            self.offset_look(me, offset)
        };
        self.face_position(me, position, true, command);
    }

    /// `ST_OffsetLook` (`NPC_AI_Stormtrooper.c:945-963`).
    fn offset_look(&self, me: usize, offset: f32) -> [f32; 3] {
        let npc = &self.actors[me];
        let origin = npc.current_origin;
        let mut angles = crate::player_angle_math::vector_angles(crate::npc_senses::subtract(
            npc.mind.tactics.investigate_goal,
            origin,
        ));
        angles[1] += offset;
        let (forward, _) = crate::pmove::flight::flight_axes(angles);
        let forward = forward.to_array();
        let mut out: [f32; 3] = std::array::from_fn(|axis| origin[axis] + 64.0 * forward[axis]);
        out[2] = spot(&self.npc(me), Spot::Head)[2];
        out
    }

    /// `NPC_BSST_Investigate` (`NPC_AI_Stormtrooper.c:1010-1093`).
    pub fn bs_st_investigate(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        self.get_group(me);
        let flags = self.actors[me].script_flags;
        if flags & SCF_FIRE_WEAPON != 0 {
            self.weapon_think(me, command);
        }
        if self.actors[me].mind.confusion_time < level_time
            && flags & SCF_LOOK_FOR_ENEMIES != 0
            && self.check_player_team_stealth(me)
        {
            self.st_speech(me, speech::DETECTED, 0.0);
            self.actors[me].mind.temp_behavior = BS_DEFAULT;
            self.update_angles(me, true, true, command);
            return;
        }
        if flags & SCF_IGNORE_ALERTS == 0 {
            let last = self.actors[me].mind.last_alert_id;
            if let Some(alert) = self.check_alerts(me, last, false, AEL_MINOR) {
                if self.actors[me].mind.confusion_time < level_time
                    && self.check_for_danger(me, Some(alert))
                {
                    self.st_speech(me, speech::COVER, 0.0);
                    return;
                }
                if self.alerts.events()[alert].id != self.actors[me].mind.last_alert_id {
                    self.investigate_event(me, alert, true);
                }
            }
        }
        let tactics = self.actors[me].mind.tactics;
        if tactics.investigate_debounce_time + tactics.pause_time < level_time {
            self.actors[me].mind.temp_behavior = BS_DEFAULT;
            self.update_goal(me, command);
            self.update_angles(me, true, true, command);
            self.st_speech(me, speech::GIVEUP, 0.0);
            return;
        }
        if self.actors[me].mind.fight.local_state == LSTATE_INVESTIGATE
            && self.actors[me].mind.goal.is_some()
        {
            let npc = &self.actors[me];
            let goal = self.goal_origin(me).unwrap_or([0.0; 3]);
            if !crate::npc_nav::hit_nav_goal(
                npc.current_origin,
                npc.mins,
                npc.maxs,
                goal,
                32,
                self.flying(me),
            ) {
                command.buttons |= BUTTON_WALKING;
                if self.move_to_goal(me, true, command) {
                    let tactics = &mut self.actors[me].mind.tactics;
                    tactics.investigate_debounce_time = tactics.investigate_count * 5_000;
                    tactics.pause_time = level_time;
                    self.update_angles(me, true, true, command);
                    return;
                }
            }
            self.actors[me].mind.fight.local_state = LSTATE_NONE;
        }
        self.look_around(me, command);
    }
}
