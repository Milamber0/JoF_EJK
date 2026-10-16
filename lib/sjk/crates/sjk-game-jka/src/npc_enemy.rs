//! Whom an NPC fights (`codemp/game/NPC_combat.c`, `NPC_utils.c`): a valid enemy
//! (`NPC_ValidEnemy`), how far it may be (`NPC_EnemyTooFar`), the choice of one
//! (`NPC_PickEnemy`, `NPC_CheckEnemy`), and taking one — `G_SetEnemy` with its anger, its
//! first aim, the alert to its team and its hesitation (`G_AngerAlert`, `G_AlertTeam`,
//! `G_AttackDelay`) — and dropping one (`G_ClearEnemy`), with the look target that follows
//! it (`NPC_SetLookTarget`, `NPC_CheckLookTarget`).
//!
//! What belongs to the combat and the classes of the NPC plan's later steps is not here:
//! `NPC_Jedi_RateNewEnemy` (a saber carrier's rating of its new enemy) and the hidden
//! players' `hiddenDist` (set by scripts only) are left out, and said so where they would
//! be.
//!
//! Held to `tools/game-oracle/npcthink.c` (`game-npcthink.txt`).

use crate::npc_senses::{
    Body, CHECK_360, CHECK_FOV, CHECK_VISRANGE, Sight, Visibility, check_visibility,
    distance_squared, in_fov, in_visrange, subtract,
};
use crate::npc_spawn::{ENTITYNUM_NONE, FL_NOTARGET, NpcHost};
use crate::npc_world::NpcWorld;

/// `npcteam_t`.
pub const NPCTEAM_FREE: i32 = 0;
pub const NPCTEAM_ENEMY: i32 = 1;
pub const NPCTEAM_PLAYER: i32 = 2;
pub const NPCTEAM_NEUTRAL: i32 = 3;
/// `team_t`: `TEAM_RED`, `TEAM_BLUE`, `TEAM_SPECTATOR`.
const TEAM_RED: i32 = 1;
const TEAM_BLUE: i32 = 2;
const TEAM_SPECTATOR: i32 = 3;
/// `GT_TEAM`: the first team game.
const GT_TEAM: i32 = 6;
/// `bState_t`: the states the choice of an enemy reads.
const BS_PATROL: i32 = 12;
const BS_INVESTIGATE: i32 = 13;
const BS_STAND_AND_SHOOT: i32 = 14;
const BS_HUNT_AND_KILL: i32 = 15;
/// `class_t`s the rules name.
const CLASS_IMPWORKER: i32 = 15;
const CLASS_STORMTROOPER: i32 = 44;
const CLASS_RANCOR: i32 = 54;
const CLASS_WAMPA: i32 = 55;
/// `RANK_CREWMAN`.
const RANK_CREWMAN: i32 = 1;
/// `weapon_t`s the rules name.
const WP_NONE: i32 = 0;
const WP_SABER: i32 = 3;
const WP_BLASTER: i32 = 5;
const WP_DISRUPTOR: i32 = 6;
const WP_BOWCASTER: i32 = 7;
const WP_REPEATER: i32 = 8;
const WP_THERMAL: i32 = 12;
/// `SCF_ALT_FIRE`, `SCF_NO_GROUPS`, `SCF_IGNORE_ALERTS`, `SCF_LOOK_FOR_ENEMIES`,
/// `SCF_NO_COMBAT_TALK`, `SCF_NO_ALERT_TALK`.
const SCF_ALT_FIRE: u32 = 0x40;
const SCF_NO_COMBAT_TALK: u32 = 0x200;
const SCF_LOOK_FOR_ENEMIES: u32 = 0x800;
const SCF_IGNORE_ALERTS: u32 = 0x2000;
const SCF_NO_GROUPS: u32 = 0x2_0000;
const SCF_NO_ALERT_TALK: u32 = 0x200_0000;
/// `entity_event_t`: the voices the rules name.
pub const EV_ANGER1: i32 = 116;
const EV_ANGER3: i32 = 118;
const EV_VICTORY3: i32 = 121;
const EV_CHASE1: i32 = 133;
const EV_GIVEUP1: i32 = 152;
const EV_SUSPICIOUS5: i32 = 168;
/// `EV_GENERAL_SOUND`; `PM_DEAD`; `EF2_HELD_BY_MONSTER`.
const EV_GENERAL_SOUND: u32 = 76;
const PM_DEAD: u8 = 5;
const EF2_HELD_BY_MONSTER: u32 = 1;
/// `ps.eFlags2`.
const PS_EFLAGS2: usize = 103;
/// `ANGER_ALERT_RADIUS`, `ANGER_ALERT_SOUND_RADIUS` (`NPC_combat.c:66-67`).
const ANGER_ALERT_RADIUS: f32 = 512.0;
const ANGER_ALERT_SOUND_RADIUS: f32 = 256.0;
/// `Q3_INFINITE`.
const Q3_INFINITE: f32 = 16_777_216.0;

/// `NPC_ValidEnemy` (`NPC_utils.c:1113-1210`) for `me`: alive, targetable, playing, not on
/// its team, and on the team it hates — or mad at anyone of another class, a rampaging
/// monster, or a stray attacking its friends. `team_of` gives the team of the target's own
/// enemy.
pub fn valid_enemy(
    me: &Body,
    target: &Body,
    gametype: i32,
    team_of: impl Fn(u16) -> Option<i32>,
) -> bool {
    if target.number == me.number || target.health <= 0 || target.flags & FL_NOTARGET != 0 {
        return false;
    }
    if target.session_team == TEAM_SPECTATOR || target.spectating {
        return false;
    }
    let team = if target.npc {
        target.player_team
    } else if gametype < GT_TEAM {
        NPCTEAM_PLAYER
    } else {
        match target.session_team {
            TEAM_BLUE => NPCTEAM_PLAYER,
            TEAM_RED => NPCTEAM_ENEMY,
            _ => NPCTEAM_NEUTRAL,
        }
    };
    if target.player_team == me.player_team {
        return false;
    }
    let stray = team == NPCTEAM_FREE
        && target.enemy_team == NPCTEAM_FREE
        && target.enemy.and_then(team_of).is_some_and(|their| {
            their == me.player_team || (their != NPCTEAM_ENEMY && me.player_team == NPCTEAM_PLAYER)
        });
    team == me.enemy_team
        || (me.enemy_team == NPCTEAM_FREE && target.class != me.class)
        || (target.class == CLASS_WAMPA && target.enemy.is_some())
        || (target.class == CLASS_RANCOR && target.enemy.is_some())
        || stray
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_MaxDistSquaredForWeapon` (`NPC_combat.c:1299-1350`): how far the NPC fights.
    pub fn max_distance_squared(&self, me: usize) -> f32 {
        let npc = &self.actors[me];
        let shoot = npc.definition.stats.shoot_distance;
        if shoot > 0.0 {
            return shoot * shoot;
        }
        match self.npc(me).weapon {
            WP_DISRUPTOR if npc.script_flags & SCF_ALT_FIRE != 0 => 4_096.0 * 4_096.0,
            WP_SABER => {
                let length = npc.definition.sabers[0].blades[0].length_max;
                if length != 0.0 {
                    // A float and a double: squared in double, returned as a float.
                    let reach = f64::from(length) + f64::from(npc.maxs[0]) * 1.5;
                    (reach * reach) as f32
                } else {
                    48.0 * 48.0
                }
            }
            _ => 1_024.0 * 1_024.0,
        }
    }

    /// `NPC_EnemyTooFar` (`NPC_combat.c:1427-1450`): a saber carrier only has to reach its
    /// enemy; anyone else is too far beyond its weapon's reach. A `distance` of zero is
    /// measured here.
    pub fn enemy_too_far(&self, me: usize, enemy: &Body, distance: f32, to_shoot: bool) -> bool {
        let npc = &self.actors[me];
        if !to_shoot && i32::from(npc.player.weapon()) == WP_SABER {
            return false;
        }
        let distance = if distance == 0.0 {
            distance_squared(npc.current_origin, enemy.origin)
        } else {
            distance
        };
        distance > self.max_distance_squared(me)
    }

    /// The NPC's sight (`stats.hfov`, `vfov`, `visrange`).
    pub fn sight(&self, me: usize) -> Sight {
        let stats = &self.actors[me].definition.stats;
        Sight {
            hfov: stats.hfov,
            vfov: stats.vfov,
            visrange: stats.visrange,
        }
    }

    /// `NPC_ValidEnemy` for the NPC at `me`.
    pub fn valid_for(&self, me: usize, target: &Body) -> bool {
        let gametype = self.host.gametype();
        valid_enemy(&self.npc(me), target, gametype, |number| {
            self.body(number).map(|body| body.player_team)
        })
    }

    /// `NPC_PickEnemy(closestTo, enemyTeam, checkVis, qfalse, qtrue)` (`NPC_combat.c:1469-1760`)
    /// as `NPC_CheckEnemy` asks it: the closest living, valid enemy on `enemy_team` in the
    /// potentially visible set, in sight and in view if `check_vis` — the view left out for
    /// an NPC already fighting (`BS_STAND_AND_SHOOT`, `BS_HUNT_AND_KILL`). The forms that
    /// look for the player first or choose at random (`rand()`) are for the class AI of
    /// later steps.
    pub fn pick_enemy(
        &mut self,
        me: usize,
        closest_to: usize,
        enemy_team: i32,
        check_vis: bool,
    ) -> Option<u16> {
        if enemy_team == NPCTEAM_NEUTRAL {
            return None;
        }
        let behavior = self.actors[me].behavior_state;
        let (mut checks, mut least) = (CHECK_360 | CHECK_FOV | CHECK_VISRANGE, Visibility::Fov);
        if behavior == BS_STAND_AND_SHOOT || behavior == BS_HUNT_AND_KILL {
            checks &= !CHECK_FOV;
            least = Visibility::Full360;
        }
        let watchful = behavior == BS_INVESTIGATE || behavior == BS_PATROL;
        let has_enemy = self.actors[me].mind.enemy.is_some();
        let last_enemy = self.actors[me].mind.last_enemy;
        let closest_origin = self.actors[closest_to].current_origin;
        let me_body = self.npc(me);
        let sight = self.sight(me);
        let (mut best, mut best_distance) = (None, Q3_INFINITE);
        let players = self.host.players().len();
        for index in 0..players + self.order.len() {
            let candidate = if index < players {
                self.host.players()[index]
            } else {
                self.npc(self.order[index - players])
            };
            if candidate.number == me_body.number
                || candidate.flags & FL_NOTARGET != 0
                || candidate.entity_flags & crate::npc_senses::EF_NODRAW != 0
            {
                continue;
            }
            if candidate.health <= 0 || !self.valid_for(me, &candidate) {
                continue;
            }
            // Player allies turned on the player turn on the player alone.
            if me_body.player_team == NPCTEAM_PLAYER
                && enemy_team == NPCTEAM_PLAYER
                && candidate.number >= 32
            {
                continue;
            }
            if Some(candidate.number) == last_enemy
                || !self.host.in_pvs(candidate.origin, me_body.origin)
            {
                continue;
            }
            if watchful
                && !has_enemy
                && (!in_visrange(&candidate, &me_body, sight.visrange)
                    || self.visibility(me, &candidate, CHECK_360 | CHECK_FOV | CHECK_VISRANGE)
                        != Visibility::Fov)
            {
                continue;
            }
            let distance = distance_squared(closest_origin, candidate.origin);
            if distance < best_distance
                && !self.enemy_too_far(me, &candidate, distance, false)
                && (!check_vis || self.visibility(me, &candidate, checks) == least)
            {
                best = Some(candidate.number);
                best_distance = distance;
            }
        }
        best
    }

    /// `NPC_CheckVisibility` of `target` by the NPC at `me`.
    pub fn visibility(&mut self, me: usize, target: &Body, flags: u32) -> Visibility {
        let (me_body, sight) = (self.npc(me), self.sight(me));
        check_visibility(&mut self.senses(), target, &me_body, sight, flags)
    }

    /// `NPC_CheckEnemy` (`NPC_combat.c:1861-2067`): the enemy dropped when it is gone, too
    /// far or dead, and — if asked to find one — a new one taken on the NPC's enemy team.
    /// Returns the new enemy it found.
    pub fn check_enemy(
        &mut self,
        me: usize,
        find_new: bool,
        too_far_ok: bool,
        set_enemy: bool,
    ) -> Option<u16> {
        let mut force_find = false;
        let mut new_enemy = None;
        if let Some(enemy) = self.actors[me].mind.enemy
            && self.body(enemy).is_none()
            && set_enemy
        {
            self.clear_enemy(me);
        }
        if let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|number| self.body(number))
        {
            if self.enemy_too_far(me, &enemy, 0.0, false) {
                if find_new {
                    force_find = true;
                } else if !too_far_ok && set_enemy {
                    self.clear_enemy(me);
                }
            }
            // Out of the potentially visible set the enemy is kept: its `hiddenDist`, which
            // would lose it, is only ever set by scripts.
        }
        if let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|number| self.body(number))
            && (enemy.health <= 0 || enemy.flags & FL_NOTARGET != 0)
            && set_enemy
        {
            self.clear_enemy(me);
        }
        // Nobody to defend (`defendEnt`) until the steps that set one: the closest enemy is
        // the closest to the NPC itself.
        let enemy_health = self.actors[me]
            .mind
            .enemy
            .and_then(|number| self.body(number))
            .map(|enemy| enemy.health);
        if self.actors[me].mind.enemy.is_none()
            || enemy_health.is_some_and(|health| health <= 0)
            || force_find
        {
            if !find_new {
                if set_enemy {
                    self.actors[me].mind.last_enemy = self.actors[me].mind.enemy;
                    self.clear_enemy(me);
                }
                return None;
            }
            let mut found = false;
            let enemy_team = self.actors[me].enemy_team;
            if enemy_team != NPCTEAM_NEUTRAL {
                new_enemy = self.pick_enemy(me, me, enemy_team, true);
                if let Some(enemy) = new_enemy {
                    found = true;
                    if set_enemy {
                        self.set_enemy(me, enemy);
                    }
                }
            }
            if !force_find {
                if !found && set_enemy {
                    self.actors[me].mind.last_enemy = self.actors[me].mind.enemy;
                    self.clear_enemy(me);
                }
                self.actors[me].mind.cant_hit_enemy_counter = 0;
            }
        }
        if let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|number| self.body(number))
            && enemy.player_team != 0
            && self.actors[me].player_team != enemy.player_team
        {
            self.actors[me].enemy_team = enemy.player_team;
        }
        new_enemy
    }

    /// `G_ClearEnemy` (`NPC_combat.c:39-58`): no enemy, nor a look at it, nor a goal of it.
    pub fn clear_enemy(&mut self, me: usize) {
        self.check_look_target(me);
        let npc = &mut self.actors[me];
        if let Some(enemy) = npc.mind.enemy {
            if npc.mind.look_target == enemy {
                clear_look(npc);
            }
            if npc.mind.goal == Some(enemy) {
                npc.mind.goal = None;
            }
        }
        npc.mind.enemy = None;
    }

    /// `NPC_CheckLookTarget` (`NPC_utils.c:1668-1700`): whether the NPC still looks at
    /// someone — not at a target gone, past its time, or a body not its enemy while it has
    /// one.
    pub fn check_look_target(&mut self, me: usize) -> bool {
        let target = self.actors[me].mind.look_target;
        if target >= crate::pmove::ENTITY_NUMBER_WORLD {
            return false;
        }
        let exists = self.host.in_use(target) || self.actor_at(target).is_some();
        let has_client = self.body(target).is_some();
        let npc = &mut self.actors[me];
        if !exists {
            clear_look(npc);
        } else if npc.mind.look_target_clear_time != 0
            && npc.mind.look_target_clear_time < self.level_time
        {
            clear_look(npc);
        } else if has_client && npc.mind.enemy.is_some_and(|enemy| enemy != target) {
            clear_look(npc);
        } else {
            return true;
        }
        false
    }

    /// `G_SetEnemy` (`NPC_combat.c:373-592`) for the NPC at `me`: `enemy` taken — with, for
    /// a first enemy, its anger voiced, its first aim, its team alerted and its attack
    /// delayed.
    pub fn set_enemy(&mut self, me: usize, enemy: u16) {
        let Some(target) = self.body(enemy) else {
            return;
        };
        if target.flags & FL_NOTARGET != 0 || self.actors[me].mind.confusion_time > self.level_time
        {
            return;
        }
        let level_time = self.level_time;
        if target.player_team == self.actors[me].player_team
            && self.actors[me].mind.charmed_time > level_time
        {
            return;
        }
        if i32::from(self.actors[me].player.weapon()) == WP_SABER {
            // `NPC_Jedi_RateNewEnemy`: the Jedi's own judgement — stood in for (a printed stub)
            // by the drivers that stand the class AI in.
            if self.level.jedi_ai_stood_in() {
                self.host
                    .stub(self.actors[me].number, "NPC_Jedi_RateNewEnemy");
            } else {
                self.jedi_rate_new_enemy(me, enemy);
            }
        }
        if self.actors[me].mind.enemy.is_some() {
            // Another enemy: taken quietly.
            self.clear_enemy(me);
            self.actors[me].mind.enemy = Some(enemy);
            return;
        }
        if self.actors[me].health > 0 {
            force_saber_on(&mut self.actors[me], self.host);
        }
        self.clear_enemy(me);
        self.actors[me].mind.enemy = Some(enemy);
        if self.actors[me].player_team == NPCTEAM_PLAYER && enemy < 32 {
            self.actors[me].enemy_team = NPCTEAM_PLAYER;
        }
        // No anger script (`G_ActivateBehavior(BSET_ANGER)`): the anger is voiced.
        if self.actors[me].player_team != target.player_team && !self.team_has_enemy(me) {
            let event = self.host.irand(EV_ANGER1, EV_ANGER3);
            self.add_voice(me, event, 2_000);
        }
        self.first_aim(me);
        let npc = &self.actors[me];
        // Gripped, it cannot call for help.
        if !npc.npc_type.eq_ignore_ascii_case(b"desperado")
            && !npc.npc_type.eq_ignore_ascii_case(b"paladin")
            && npc.force.grip_being_gripped < self.level_time as f32
        {
            self.anger_alert(me);
        }
        self.attack_delay(me, &target);
    }

    /// `G_TeamEnemy` (`NPC_combat.c:89-137`): whether anyone living on the NPC's team has
    /// an enemy off it.
    fn team_has_enemy(&self, me: usize) -> bool {
        let npc = &self.actors[me];
        if npc.player_team == NPCTEAM_FREE || npc.script_flags & SCF_NO_GROUPS != 0 {
            return false;
        }
        let mut found = false;
        self.each_body(|body| {
            if found
                || body.number == 0
                || body.number == npc.number
                || body.health <= 0
                || body.player_team != npc.player_team
            {
                return;
            }
            if let Some(enemy) = body.enemy {
                found = self
                    .body(enemy)
                    .is_none_or(|their| their.player_team != npc.player_team);
            }
        });
        found
    }

    /// `G_AddVoiceEvent` (`NPC_sounds.c:45-86`): the NPC says `event` as an event of its
    /// own (`G_SpeechEvent`), and not again for `debounce` milliseconds (5 s for 0).
    pub fn add_voice(&mut self, me: usize, event: i32, debounce: i32) {
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        if npc.player.movement_type() >= PM_DEAD || npc.mind.blocked_speech_until > level_time {
            return;
        }
        let combat = (EV_ANGER1..=EV_VICTORY3).contains(&event)
            || (EV_CHASE1..=EV_SUSPICIOUS5).contains(&event);
        if npc.script_flags & SCF_NO_COMBAT_TALK != 0 && combat {
            return;
        }
        if npc.script_flags & SCF_NO_ALERT_TALK != 0
            && (EV_GIVEUP1..=EV_SUSPICIOUS5).contains(&event)
        {
            return;
        }
        crate::player_entity::add_event(&mut npc.player, event as u32, 0);
        npc.mind.event_time = level_time;
        npc.mind.blocked_speech_until = level_time + if debounce == 0 { 5_000 } else { debounce };
    }

    /// `G_SetEnemy`'s first aim for a gun carrier (`NPC_combat.c:461-487`) and `G_AimSet`
    /// (`NPC_combat.c:3091-3104`): worse than its best at first, and more so on easy.
    fn first_aim(&mut self, me: usize) {
        let weapon = self.npc(me).weapon;
        if !matches!(weapon, WP_BLASTER | WP_REPEATER | WP_THERMAL | WP_BOWCASTER) {
            return;
        }
        let skill = self.host.skill();
        let npc = &self.actors[me];
        let aim = npc.definition.stats.aim;
        let aim = if npc.player_team == NPCTEAM_PLAYER {
            self.host.irand(aim - 5 * skill, aim - skill)
        } else {
            let (least, most) = match npc.definition.client_class {
                CLASS_IMPWORKER => (15, 30),
                CLASS_STORMTROOPER if npc.definition.rank <= RANK_CREWMAN => (5, 15),
                _ => (3, 12),
            };
            self.host
                .irand(aim - most * (3 - skill), aim - least * (3 - skill))
        };
        self.actors[me].mind.current_aim = aim;
        let debounce = 500 + (3 - skill) * 100;
        let time = self.host.irand(debounce, debounce + 1_000);
        let level_time = self.level_time;
        self.actors[me]
            .mind
            .timers
            .set("aimDebounce", level_time, time);
    }

    /// `G_AngerAlert` (`NPC_combat.c:69-82`): the NPC's teammates within 512 units, not yet
    /// angry, that can hear it (256 units) or see it, take its enemy too.
    fn anger_alert(&mut self, me: usize) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        if npc.script_flags & SCF_NO_GROUPS != 0
            || !npc.mind.timers.done("interrogating", level_time)
        {
            return;
        }
        let Some(attacker) = npc.mind.enemy.and_then(|enemy| self.body(enemy)) else {
            return;
        };
        let victim = self.npc(me);
        self.alert_team(
            &victim,
            Some(me),
            &attacker,
            ANGER_ALERT_RADIUS,
            ANGER_ALERT_SOUND_RADIUS,
        );
    }

    /// `G_AlertTeam(victim, attacker, radius, soundDist)` (`g_combat.c:1786-1873`): the NPCs
    /// on the victim's team (`victim_at`: the victim itself, when it is an NPC) within
    /// `radius` of it, alive, listening and not yet angry, that can hear it (`sound`
    /// units) or see it, take `attacker` for their enemy.
    pub(crate) fn alert_team(
        &mut self,
        victim: &Body,
        victim_at: Option<usize>,
        attacker: &Body,
        radius: f32,
        sound_radius: f32,
    ) {
        let sound = sound_radius * sound_radius;
        // `EntitiesInBox` in entity order: the NPCs are the only ones with a `gNPC_t`.
        for index in 0..self.order.len() {
            let check = self.order[index];
            let body = self.npc(check);
            let linked = (0..3).all(|axis| {
                body.origin[axis] + body.mins[axis] - 1.0 <= victim.origin[axis] + radius
                    && body.origin[axis] + body.maxs[axis] + 1.0 >= victim.origin[axis] - radius
            });
            let flags = self.actors[check].script_flags;
            if !linked
                || flags & SCF_IGNORE_ALERTS != 0
                || flags & SCF_LOOK_FOR_ENEMIES == 0
                || flags & SCF_NO_GROUPS != 0
            {
                continue;
            }
            if Some(check) == victim_at
                || body.number == attacker.number
                || body.player_team != victim.player_team
                || body.health <= 0
                || body.enemy.is_some()
            {
                continue;
            }
            let distance = distance_squared(body.origin, victim.origin);
            if distance > 16_384.0 && !self.host.in_pvs(victim.origin, body.origin) {
                continue;
            }
            if sound_radius <= 0.0 || distance > sound {
                let sight = self.sight(check);
                let eyes = crate::npc_senses::spot(&body, crate::npc_senses::Spot::HeadLean);
                if !in_fov(victim, &body, sight.hfov, sight.vfov)
                    || !crate::npc_senses::clear_los(&mut self.senses(), eyes, victim.origin)
                {
                    continue;
                }
            }
            self.set_enemy(check, attacker.number);
        }
    }

    /// `G_AttackDelay` (`NPC_combat.c:139-334`): how long before the NPC fires and moves —
    /// longer the farther it faces from its enemy, by class, weapon and difficulty.
    fn attack_delay(&mut self, me: usize, enemy: &Body) {
        let skill = self.host.skill();
        let level_time = self.level_time;
        let npc = &self.actors[me];
        let mut direction = subtract(npc.mind.eye_point, enemy.origin);
        crate::player_angle_math::normalize(&mut direction);
        let (forward, _) = crate::pmove::flight::flight_axes(npc.mind.eye_angles);
        let forward = forward.to_array();
        let mut delay = (4 - skill) * 500;
        if npc.player_team == NPCTEAM_PLAYER {
            delay = 2_000 - delay;
        }
        // `floor((DotProduct(fwd, dir) + 1.0f) * 2000.0f)`, the product a float's.
        let facing =
            direction[0] * forward[0] + direction[1] * forward[1] + direction[2] * forward[2];
        delay += ((facing + 1.0) * 2_000.0).floor() as i32;
        let (class, rank, flags) = (
            npc.definition.client_class,
            npc.definition.rank,
            npc.script_flags,
        );
        match class_delay(class, rank) {
            None => return,
            Some((sign, low, high)) if high != 0 => delay += sign * self.host.irand(low, high),
            Some(_) => {}
        }
        let weapon = self.npc(me).weapon;
        match weapon {
            WP_NONE | WP_SABER | WP_DISRUPTOR | WP_THERMAL | 1 | 17 | 18 => return,
            WP_BLASTER if flags & SCF_ALT_FIRE != 0 => delay += self.host.irand(0, 500),
            WP_BLASTER => delay -= self.host.irand(0, 500),
            WP_BOWCASTER => delay += self.host.irand(0, 500),
            WP_REPEATER if flags & SCF_ALT_FIRE == 0 => delay += self.host.irand(0, 500),
            // WP_FLECHETTE, WP_ROCKET_LAUNCHER (`NPC_combat.c:259-264`).
            10 | 11 => delay += self.host.irand(500, 1_500),
            _ => {}
        }
        if self.actors[me].player_team == NPCTEAM_PLAYER && delay > 2_000 {
            delay = 2_000;
        }
        let most = 4_000 + (2 - skill) * 3_000;
        delay = delay.min(most);
        self.actors[me]
            .mind
            .timers
            .set("attackDelay", level_time, delay);
        let roam = if delay > 4_000 {
            4_000 - self.host.irand(500, 1_500)
        } else {
            delay - self.host.irand(500, 1_500)
        };
        self.actors[me]
            .mind
            .timers
            .set("roamTime", level_time, roam);
    }
}

/// `G_AttackDelay`'s class part (`NPC_combat.c:167-234`): `None` for the classes that
/// never wait; else the sign and range of the random change (a range of zero for none).
fn class_delay(class: i32, rank: i32) -> Option<(i32, i32, i32)> {
    const RANK_LT: i32 = 4;
    Some(match class {
        // CLASS_IMPERIAL: they give orders and hang back.
        14 => (1, 500, 1_500),
        CLASS_STORMTROOPER if rank >= RANK_LT => (-1, 500, 1_500),
        CLASS_STORMTROOPER => (-1, 0, 1_000),
        // CLASS_SWAMPTROOPER.
        46 => (-1, 1_000, 2_000),
        CLASS_IMPWORKER => (1, 1_000, 2_500),
        // CLASS_TRANDOSHAN, and CLASS_JAN, CLASS_LANDO, CLASS_PRISONER, CLASS_REBEL.
        48 | 17 | 20 | 31 | 36 => (-1, 500, 1_500),
        // CLASS_GALAKMECH, CLASS_ATST.
        25 | 1 => (-1, 1_000, 2_000),
        // CLASS_REELO, CLASS_UGNAUGHT, CLASS_JAWA, CLASS_MINEMONSTER, CLASS_MURJJ, the
        // droids that shoot, the remote and the seeker.
        38 | 49 | 50 | 26 | 30 | 16 | 32 | 23 | 24 | 42 | 39 | 41 => return None,
        _ => (1, 0, 0),
    })
}

/// `NPC_ClearLookTarget` for an NPC not held by a monster.
fn clear_look(npc: &mut crate::npc_spawn::NpcActor) {
    if npc.player.raw_field(PS_EFLAGS2).unwrap_or(0) & EF2_HELD_BY_MONSTER == 0 {
        npc.mind.clear_look_target();
    }
}

/// `NPC_SetLookTarget` for an NPC not held by a monster.
pub fn set_look(npc: &mut crate::npc_spawn::NpcActor, number: u16, clear_time: i32) {
    if npc.player.raw_field(PS_EFLAGS2).unwrap_or(0) & EF2_HELD_BY_MONSTER == 0 {
        npc.mind.set_look_target(number, clear_time);
    }
}

/// `G_ForceSaberOn` (`NPC_combat.c:336-362`): a saber carrier's blades lit, with their
/// sounds.
fn force_saber_on(npc: &mut crate::npc_spawn::NpcActor, host: &mut impl NpcHost) {
    const PS_SABER_IN_FLIGHT: usize = 88;
    const PS_SABER_HOLSTERED: usize = 81;
    let player = &mut npc.player;
    if player.raw_field(PS_SABER_IN_FLIGHT).unwrap_or(0) != 0
        || player.raw_field(PS_SABER_HOLSTERED).unwrap_or(0) == 0
        || i32::from(player.weapon()) != WP_SABER
    {
        return;
    }
    player.set_raw_field(PS_SABER_HOLSTERED, 0);
    for saber in &npc.definition.sabers {
        if saber.sound_on != 0 {
            // `G_Sound(ent, CHAN_AUTO, soundOn)`: a sound event where it stands.
            host.raise(crate::event_entity::EventEntity {
                event: EV_GENERAL_SOUND,
                parameter: u32::from(saber.sound_on),
                origin: npc.current_origin,
                client: None,
                broadcast: false,
                extra: [(0, 0); 12],
            });
        }
    }
}

/// `ENTITYNUM_NONE` as a look target: nobody.
pub const NOBODY: u16 = ENTITYNUM_NONE;
