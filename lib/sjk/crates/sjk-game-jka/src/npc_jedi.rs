//! A Jedi NPC's top-level AI (`codemp/game/NPC_AI_Jedi.c`): its behaviour
//! (`NPC_BSJedi_Default`, `6258-6335`), its fight (`Jedi_Attack`, `5812-6128`), the special
//! moves that hold it (`Jedi_InSpecialMove`, `6131-6256`), its voices (`142-186`) and the
//! shadowtrooper's cloak (`804-861`). The fight's parts — evasion, moves, combat, jumps —
//! are the neighbouring modules'.

use crate::force_powers::{FP_HEAL, FP_PUSH, FP_RAGE, FP_SABER_DEFENSE, FP_SPEED};
use crate::npc_jedi_patrol::{
    CLASS_BOBAFETT, CLASS_DESANN, CLASS_REBORN, CLASS_SHADOWTROOPER, CLASS_TAVION, Q3_INFINITE,
    distance_horizontal_squared,
};
use crate::npc_senses::distance_squared;
use crate::npc_spawn::{ENTITYNUM_NONE, FL_NOTARGET, NpcHost};
use crate::npc_world::NpcWorld;
use sjk_protocol::UserCommand;

/// `playerState_t` wire fields: `saberAnimLevel`, `saberEntityNum`, `forcePowersKnown`,
/// `saberBlocked`, `forcePowersActive`, `saberInFlight`, `saberLockTime`.
const PS_SABER_ANIM_LEVEL: usize = 23;
const PS_SABER_ENTITY_NUM: usize = 31;
const PS_FORCE_KNOWN: usize = 51;
const PS_SABER_BLOCKED: usize = 77;
const PS_FORCE_ACTIVE: usize = 82;
const PS_SABER_IN_FLIGHT: usize = 88;
const PS_SABER_LOCK_TIME: usize = 107;
/// `entityState_t.loopSound`.
const ES_LOOP_SOUND: usize = 55;
/// `STAT_MAX_HEALTH`.
const STAT_MAX_HEALTH: usize = 8;
/// `powerups[PW_CLOAKED]`.
const PW_CLOAKED: usize = 11;
/// `saberBlockedType_t`: `BLOCKED_NONE`, `BLOCKED_PARRY_BROKEN`.
const BLOCKED_NONE: u32 = 0;
const BLOCKED_PARRY_BROKEN: u32 = 2;
/// `FORCE_LEVEL_2`, `FORCE_LEVEL_3`.
const FORCE_LEVEL_2: i32 = 2;
const FORCE_LEVEL_3: i32 = 3;
/// `rank_t`: `RANK_CREWMAN`, `RANK_LT`, `RANK_CAPTAIN`.
const RANK_CREWMAN: i32 = 1;
const RANK_LT: i32 = 4;
const RANK_CAPTAIN: i32 = 7;
/// `npcteam_t`: `NPCTEAM_FREE`, `NPCTEAM_ENEMY`, `NPCTEAM_PLAYER`.
const NPCTEAM_FREE: i32 = 0;
const NPCTEAM_ENEMY: i32 = 1;
const NPCTEAM_PLAYER: i32 = 2;
/// `bState_t`: `BS_DEFAULT`, `BS_HUNT_AND_KILL`.
const BS_DEFAULT: i32 = 0;
const BS_HUNT_AND_KILL: i32 = 15;
/// `weapon_t`: `WP_MELEE`, `WP_SABER`.
const WP_MELEE: u32 = 2;
const WP_SABER: i32 = 3;
const WP_DISRUPTOR: u8 = 6;
/// Voice events: `EV_ANGER1`… — each the first of three.
const EV_VICTORY1: i32 = 119;
const EV_CONFUSE1: i32 = 122;
const EV_COMBAT1: i32 = 169;
const EV_TAUNT1: i32 = 175;
const EV_DEFLECT1: i32 = 184;
const EV_GLOAT1: i32 = 187;
const EV_PUSHFAIL: i32 = 190;
/// Animations that hold the AI (`Jedi_InSpecialMove`).
const BOTH_KYLE_PA_1: u16 = 1_284;
const BOTH_PLAYER_PA_1: u16 = 1_285;
const BOTH_KYLE_PA_2: u16 = 1_286;
const BOTH_PLAYER_PA_2: u16 = 1_287;
const BOTH_KYLE_PA_3: u16 = 1_289;
const BOTH_PLAYER_PA_3: u16 = 1_290;
const BOTH_TAVION_SWORDPOWER: u16 = 1_278;
const BOTH_FORCE_DRAIN_GRAB_START: u16 = 1_359;
const BOTH_FORCE_DRAIN_GRAB_HOLD: u16 = 1_360;
const BOTH_FORCE_DRAIN_GRAB_END: u16 = 1_361;
const BOTH_FORCE_DRAIN_GRABBED: u16 = 1_362;
/// `scriptFlags`: `SCF_ALT_FIRE`, `SCF_CHASE_ENEMIES`, `SCF_DONT_FIRE`,
/// `SCF_NO_ACROBATICS`.
const SCF_ALT_FIRE: u32 = 0x40;
const SCF_CHASE_ENEMIES: u32 = 0x400;
const SCF_DONT_FIRE: u32 = 0x4000;
const SCF_NO_ACROBATICS: u32 = 0x80_0000;
/// `BUTTON_ATTACK`, `BUTTON_WALKING`, `BUTTON_ALT_ATTACK`, `BUTTON_FORCE_DRAIN`.
const BUTTON_ATTACK: u16 = 1;
const BUTTON_WALKING: u16 = 16;
const BUTTON_ALT_ATTACK: u16 = 128;
const BUTTON_FORCE_DRAIN: u16 = 2_048;

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSJedi_Default` (`NPC_AI_Jedi.c:6258-6335`): held by a special move, or cloaked
    /// as it may be; without an enemy it patrols, with one it drops from its ambush,
    /// becomes a destroyer's everlasting rage, and fights — looking now and then for a
    /// better enemy while idle or over a dead one.
    pub fn bs_jedi_default(&mut self, me: usize, command: &mut UserCommand) {
        if self.jedi_in_special_move(me, command) {
            return;
        }
        self.jedi_check_cloak(me);
        let Some(enemy) = self.actors[me].mind.enemy else {
            if self.actors[me].definition.client_class == CLASS_BOBAFETT {
                self.bs_st_patrol(me, command);
            } else {
                self.jedi_patrol(me, command);
            }
            return;
        };
        if self.jedi_waiting_ambush(me) {
            self.jedi_ambush(me);
        }
        if self.jedi_cultist_destroyer(me) && self.actors[me].mind.charmed_time == 0 {
            let sound = self
                .host
                .sound_index(b"sound/movers/objects/green_beam_lp2.wav");
            let npc = &mut self.actors[me];
            npc.mind.charmed_time = Q3_INFINITE;
            let active = npc.player.force_powers_active() | 1 << FP_RAGE;
            npc.player.set_raw_field(PS_FORCE_ACTIVE, active);
            npc.force.duration[FP_RAGE] = Q3_INFINITE;
            npc.state.set_raw_field(ES_LOOP_SOUND, u32::from(sound));
        }
        if self.actors[me].definition.client_class == CLASS_BOBAFETT && self.boba_snipes(me, enemy)
        {
            self.actors[me].script_flags |= SCF_ALT_FIRE;
            self.boba_change_weapon(me, WP_DISRUPTOR);
            self.bs_sniper_default(me, command);
            return;
        }
        self.jedi_attack(me, command);
        self.look_for_better_enemy(me, command);
    }

    /// Boba Fett, unhurt, snipes an enemy over 800 away that is not after him
    /// (`NPC_AI_Jedi.c:6308-6316`).
    fn boba_snipes(&self, me: usize, enemy: u16) -> bool {
        let npc = &self.actors[me];
        let Some(target) = self.body(enemy) else {
            return false;
        };
        target.enemy != Some(npc.number)
            && npc.health == npc.max_health
            && distance_squared(npc.current_origin, target.origin) > (800 * 800) as f32
    }

    /// `NPC_BSJedi_Default`'s second look (`6319-6333`): not pressing a button or using a
    /// power — or standing over a dead enemy — and due, it looks for another enemy as if it
    /// had none, and takes a different one.
    fn look_for_better_enemy(&mut self, me: usize, command: &UserCommand) {
        let npc = &self.actors[me];
        let idle = command.buttons == 0 && npc.player.force_powers_active() == 0;
        let enemy_dead = npc
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
            .is_some_and(|enemy| enemy.health <= 0);
        if !(idle || enemy_dead) || npc.mind.tactics.enemy_check_debounce_time >= self.level_time {
            return;
        }
        let saved = npc.mind.enemy;
        let find_new = npc.mind.confusion_time < self.level_time;
        self.actors[me].mind.enemy = None;
        let found = self.check_enemy(me, find_new, false, false);
        self.actors[me].mind.enemy = saved;
        if let Some(found) = found
            && Some(found) != saved
        {
            self.actors[me].mind.last_enemy = saved;
            self.set_enemy(me, found);
        }
        let delay = self.host.irand(1_000, 3_000);
        self.actors[me].mind.tactics.enemy_check_debounce_time = self.level_time + delay;
    }

    /// `WP_Explode` for the NPC at `me` (`g_weapon.c:347-395`): no more damage to it, its
    /// loop quiet, `G_RadiusDamage` of `damage` within `radius` from where it stands — its
    /// owner's (`s.owner`), else its own — through the host, its `target` used, and itself
    /// freed 50 ms on.
    fn wp_explode(&mut self, me: usize, damage: i32, radius: f32) {
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        npc.takes_damage = false;
        npc.state.set_raw_field(ES_LOOP_SOUND, 0);
        let owner = npc
            .state
            .raw_field(crate::npc_spawn::es::OWNER)
            .unwrap_or(0) as u16;
        let (number, at) = (npc.number, npc.current_origin);
        npc.think = crate::npc_spawn::NpcThink::Free(level_time + 50);
        let target = npc.target.clone();
        let attacker = match self
            .actor_at(owner)
            .filter(|_| owner != 0 && owner != ENTITYNUM_NONE)
        {
            Some(at) => crate::npc_saber_throw::npc_attacker(&self.actors[at]),
            None => crate::npc_saber_throw::npc_attacker(&self.actors[me]),
        };
        let (hits, armor) = self.host.explode(number, at, damage, radius, attacker);
        if hits != 0
            && let Some(at) = self.actor_at(attacker.client)
        {
            let persistent = &mut self.actors[at].player.persistent;
            persistent[1] = (persistent[1] as i32 + hits) as u32;
            persistent[7] = armor.unwrap_or(0);
        }
        if let Some(target) = target {
            self.fired.push(target);
        }
    }

    /// `Jedi_CultistDestroyer` (`NPC_AI_Jedi.c:188-200`): the Reborn "cultist_destroyer"
    /// with its bare hands.
    fn jedi_cultist_destroyer(&self, me: usize) -> bool {
        let npc = &self.actors[me];
        npc.definition.client_class == CLASS_REBORN
            && npc
                .state
                .raw_field(crate::npc_spawn::es::WEAPON)
                .unwrap_or(0)
                == WP_MELEE
            && npc.npc_type.eq_ignore_ascii_case(b"cultist_destroyer")
    }

    /// `Jedi_InSpecialMove` (`NPC_AI_Jedi.c:6131-6256`): a move that holds the AI — a
    /// scripted grapple, the drain's grab (holding the drain button while `draining`),
    /// Tavion's sword power (healing), or a cultist destroyer about to blow up. Whether one
    /// held it.
    pub fn jedi_in_special_move(&mut self, me: usize, command: &mut UserCommand) -> bool {
        let torso = self.actors[me].player.torso_animation();
        if matches!(
            torso,
            BOTH_KYLE_PA_1
                | BOTH_KYLE_PA_2
                | BOTH_KYLE_PA_3
                | BOTH_PLAYER_PA_1
                | BOTH_PLAYER_PA_2
                | BOTH_PLAYER_PA_3
                | BOTH_FORCE_DRAIN_GRAB_END
                | BOTH_FORCE_DRAIN_GRABBED
        ) {
            self.update_angles(me, true, true, command);
            return true;
        }
        if matches!(
            torso,
            BOTH_FORCE_DRAIN_GRAB_START | BOTH_FORCE_DRAIN_GRAB_HOLD
        ) {
            if !self.actors[me]
                .mind
                .timers
                .done("draining", self.level_time)
            {
                command.buttons |= BUTTON_FORCE_DRAIN;
            }
            self.update_angles(me, true, true, command);
            return true;
        }
        if torso == BOTH_TAVION_SWORDPOWER {
            let heal = self.host.irand(1, 2);
            let npc = &mut self.actors[me];
            npc.health += heal;
            let max = npc.player.stats[STAT_MAX_HEALTH] as i32;
            if npc.health > max {
                npc.health = max;
            }
            self.update_angles(me, true, true, command);
            return true;
        }
        if self.jedi_cultist_destroyer(me) && !self.actors[me].takes_damage {
            if self.actors[me].mind.use_debounce_time <= self.level_time {
                // `splashDamage` 200, `splashRadius` 512, then `WP_Explode`.
                self.actors[me].player_team = NPCTEAM_FREE;
                self.wp_explode(me, 200, 512.0);
                return true;
            }
            if self.actors[me].mind.enemy.is_some() {
                self.face_enemy(me, false, command);
            }
            return true;
        }
        false
    }

    /// `Jedi_Attack` (`NPC_AI_Jedi.c:5812-6128`): in pain it only turns; in a saber lock it
    /// pushes; a dropped saber is fetched; a dead enemy is gloated over; otherwise it keeps
    /// or finds an enemy and fights (`Jedi_Combat`), its command then trimmed by its orders
    /// and state, a war cry now and then, and Force speed matched.
    pub fn jedi_attack(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        if self.actors[me].mind.fight.pain_debounce_time > level_time {
            if self.host.irand(0, 1) != 0 {
                self.jedi_face_enemy(me, true, command);
            }
            self.update_angles(me, true, true, command);
            return;
        }
        if self.actors[me]
            .player
            .raw_field(PS_SABER_LOCK_TIME)
            .unwrap_or(0) as i32
            > level_time
        {
            self.saber_lock_push(me, command);
            self.update_angles(me, true, true, command);
            return;
        }
        if self.fetch_dropped_saber(me, command) || self.gloat(me, command) {
            return;
        }
        // The `PAS` sentry out of ammo is no client, and no enemy here ever is one.
        self.check_enemy(me, true, true, true);
        if self.actors[me].mind.enemy.is_none() {
            self.actors[me]
                .player
                .set_raw_field(PS_SABER_BLOCKED, BLOCKED_NONE);
            if self.actors[me].mind.temp_behavior == BS_HUNT_AND_KILL {
                self.actors[me].mind.temp_behavior = BS_DEFAULT;
                self.update_angles(me, true, true, command);
                return;
            }
            self.jedi_patrol(me, command);
            return;
        }
        self.actors[me].mind.combat_move = true;
        self.jedi_combat(me, command);
        self.trim_command(me, command);
        self.war_cry(me, command);
        self.match_speed(me);
    }

    /// `Jedi_Attack` in a saber lock (`5827-5880`): a strong pusher, the lock nearly over,
    /// sometimes pushes out of it; otherwise it presses attack as often as its skill
    /// allows (capped below Desann's and Tavion's).
    fn saber_lock_push(&mut self, me: usize, command: &mut UserCommand) {
        let npc = &self.actors[me];
        let lock_time = npc.player.raw_field(PS_SABER_LOCK_TIME).unwrap_or(0) as i32;
        if npc.force_levels[FP_PUSH] > FORCE_LEVEL_2
            && lock_time < self.level_time + 5_000
            && self.host.irand(0, 10) == 0
        {
            self.force_throw(me, false);
            return;
        }
        let npc = &self.actors[me];
        let skill = self.host.skill();
        let chance = if npc.definition.client_class == CLASS_DESANN
            || npc.npc_type.eq_ignore_ascii_case(b"yoda")
        {
            if skill != 0 { 4.0 } else { 3.0 }
        } else if npc.definition.client_class == CLASS_TAVION {
            2.0 + skill as f32
        } else {
            let max_chance = RANK_LT as f32 / 2.0 + 3.0;
            let rank = npc.definition.rank as f32;
            let chance = if skill == 0 {
                rank / 2.0
            } else {
                rank / 2.0 + 1.0
            };
            chance.min(max_chance)
        };
        if self.host.rng().flrand(-4.0, chance) >= 0.0 {
            command.buttons |= BUTTON_ATTACK;
        }
    }

    /// `Jedi_Attack`'s lost saber (`5882-5911`): with its saber gone from the hand, the
    /// stored saber entity becomes the goal and the attack button calls it; while the
    /// enemy lives, it goes for it — still evading a saber enemy. Whether that took the
    /// frame.
    fn fetch_dropped_saber(&mut self, me: usize, command: &mut UserCommand) -> bool {
        let npc = &self.actors[me];
        if npc.player.raw_field(PS_SABER_IN_FLIGHT).unwrap_or(0) == 0
            || npc.player.raw_field(PS_SABER_ENTITY_NUM).unwrap_or(0) != 0
        {
            return false;
        }
        let Some(stored) = npc.saber_entity else {
            return false;
        };
        let npc = &mut self.actors[me];
        npc.player.set_raw_field(PS_SABER_BLOCKED, BLOCKED_NONE);
        npc.mind.goal = Some(stored);
        command.buttons |= BUTTON_ATTACK;
        if !self.enemy_lives(me) {
            return false;
        }
        self.jedi_move(me, Some(stored), false, command);
        self.update_angles(me, true, true, command);
        let enemy_weapon = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
            .map_or(0, |enemy| enemy.weapon);
        if enemy_weapon == WP_SABER {
            let info = self.jedi_set_enemy_info(me, 300);
            self.jedi_evasion_saber(me, info.movedir, info.dist, info.dir, command);
        }
        true
    }

    /// `Jedi_Attack` over an enemy it killed (`5914-6014`), for all but the good guys:
    /// Boba Fett walks up and gloats; a Jedi lowers its guard, lets its aggression ebb,
    /// voices its victory once its saber is off, and walks up (healing when there). Whether
    /// that took the frame.
    fn gloat(&mut self, me: usize, command: &mut UserCommand) -> bool {
        let npc = &self.actors[me];
        let Some(enemy) = npc.mind.enemy.and_then(|enemy| self.body(enemy)) else {
            return false;
        };
        if enemy.health > 0 || enemy.enemy != Some(npc.number) || npc.player_team == NPCTEAM_PLAYER
        {
            return false;
        }
        self.actors[me].mind.tactics.enemy_check_debounce_time = 0;
        if self.actors[me].definition.client_class == CLASS_BOBAFETT {
            self.boba_gloat(me, &enemy, command);
            return true;
        }
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        if !npc.mind.timers.done("parryTime", level_time) {
            npc.mind.timers.set("parryTime", level_time, -1);
            npc.force.debounce[FP_SABER_DEFENSE] = level_time + 500;
        }
        npc.player.set_raw_field(PS_SABER_BLOCKED, BLOCKED_NONE);
        let in_flight = |npc: &crate::npc_spawn::NpcActor| {
            npc.player.raw_field(PS_SABER_IN_FLIGHT).unwrap_or(0) != 0
        };
        if npc.player.saber_holstered() == 0 && in_flight(npc) {
            self.jedi_aggression_erosion(me, -3);
            let npc = &self.actors[me];
            if crate::npc_saber::sabers_off(npc) && !in_flight(npc) {
                self.voice_victory(me);
            }
            self.actors[me]
                .mind
                .timers
                .set("gloatTime", level_time, 10_000);
        }
        let npc = &self.actors[me];
        if npc.player.saber_holstered() != 0
            && !in_flight(npc)
            && npc.mind.timers.done("gloatTime", level_time)
        {
            return false;
        }
        if self.walk_to_victim(me, &enemy, command) {
            let npc = &self.actors[me];
            let known = npc.player.raw_field(PS_FORCE_KNOWN).unwrap_or(0);
            let active = npc.player.force_powers_active();
            if npc.health < npc.max_health
                && known & 1 << FP_HEAL != 0
                && active & 1 << FP_HEAL == 0
            {
                self.force_heal(me);
            }
        }
        self.jedi_face_enemy(me, true, command);
        self.update_angles(me, true, true, command);
        true
    }

    /// Boba Fett's gloat (`5920-5953`): ten seconds of walking up to the body, then one
    /// victory line.
    fn boba_gloat(
        &mut self,
        me: usize,
        enemy: &crate::npc_senses::Body,
        command: &mut UserCommand,
    ) {
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        if npc.mind.walk_debounce_time < level_time && npc.mind.walk_debounce_time >= 0 {
            npc.mind.timers.set("gloatTime", level_time, 10_000);
            npc.mind.walk_debounce_time = -1;
        }
        if !self.actors[me].mind.timers.done("gloatTime", level_time) {
            if self.walk_to_victim(me, enemy, command) {
                self.actors[me].mind.timers.set("gloatTime", level_time, 0);
            }
        } else if self.actors[me].mind.walk_debounce_time == -1 {
            self.actors[me].mind.walk_debounce_time = -2;
            self.voice_victory(me);
        }
        self.jedi_face_enemy(me, true, command);
        self.update_angles(me, true, true, command);
    }

    /// Walking up to a dead enemy while more than 64 away (and chasing is allowed). Whether
    /// it is already there.
    fn walk_to_victim(
        &mut self,
        me: usize,
        enemy: &crate::npc_senses::Body,
        command: &mut UserCommand,
    ) -> bool {
        let npc = &self.actors[me];
        if distance_horizontal_squared(npc.mind.eye_point, enemy.origin) > 4_096.0
            && npc.script_flags & SCF_CHASE_ENEMIES != 0
        {
            self.actors[me].mind.goal = Some(enemy.number);
            self.jedi_move(me, Some(enemy.number), false, command);
            command.buttons |= BUTTON_WALKING;
            return false;
        }
        true
    }

    /// The victory line, heard over the team's Jedi for three seconds; it looks level and
    /// has no goal.
    fn voice_victory(&mut self, me: usize) {
        let event = self.host.irand(EV_VICTORY1, EV_VICTORY1 + 2);
        self.add_voice(me, event, 3_000);
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        if let Some(slot) = usize::try_from(npc.player_team)
            .ok()
            .and_then(|team| self.level.jedi_speech_debounce.get_mut(team))
        {
            *slot = level_time + 3_000;
        }
        npc.mind.desired_pitch = 0.0;
        npc.mind.goal = None;
    }

    /// `Jedi_Attack`'s trims after the fight's command (`6044-6104`): no moving but where
    /// it may chase (or while it heals slowly), none in the air, crouching while `duck`
    /// runs, no attack while its parry is broken, forbidden, healing or under water, no
    /// acrobatics where forbidden, and its style eased down.
    fn trim_command(&mut self, me: usize, command: &mut UserCommand) {
        let npc = &self.actors[me];
        let active = npc.player.force_powers_active();
        let healing_below =
            |level: i32| active & 1 << FP_HEAL != 0 && npc.force_levels[FP_HEAL] < level;
        if npc.script_flags & SCF_CHASE_ENEMIES == 0 || healing_below(FORCE_LEVEL_2) {
            command.forward_move = 0;
            command.right_move = 0;
            if command.up_move > 0 {
                command.up_move = 0;
            }
            self.clear_force_jump_charge(me);
            self.actors[me].mind.move_dir = [0.0; 3];
        }
        if self.actors[me].player.ground_entity_num() == ENTITYNUM_NONE {
            command.forward_move = 0;
            command.right_move = 0;
            self.actors[me].mind.move_dir = [0.0; 3];
        }
        let npc = &self.actors[me];
        if !npc.mind.timers.done("duck", self.level_time) {
            command.up_move = -127;
        }
        let boba = npc.definition.client_class == CLASS_BOBAFETT;
        if !boba
            && (crate::saber_rules::in_broken_parry(npc.player.saber_move())
                || npc.player.raw_field(PS_SABER_BLOCKED).unwrap_or(0) == BLOCKED_PARRY_BROKEN)
        {
            command.buttons &= !BUTTON_ATTACK;
        }
        let in_flight = npc.player.raw_field(PS_SABER_IN_FLIGHT).unwrap_or(0) != 0;
        let active = npc.player.force_powers_active();
        let healing = active & 1 << FP_HEAL != 0 && npc.force_levels[FP_HEAL] < FORCE_LEVEL_3;
        if npc.script_flags & SCF_DONT_FIRE != 0 || healing || !in_flight && self.saber_in_water(me)
        {
            command.buttons &= !(BUTTON_ATTACK | BUTTON_ALT_ATTACK);
        }
        if self.actors[me].script_flags & SCF_NO_ACROBATICS != 0 {
            command.up_move = 0;
            self.clear_force_jump_charge(me);
        }
        if !boba {
            self.jedi_check_decrease_saber_anim_level(me, command);
        }
    }

    /// `Jedi_Attack`'s war cry (`6106-6114`): an enemy Jedi swinging, the likelier the
    /// stronger its style and the more it is hurt.
    fn war_cry(&mut self, me: usize, command: &UserCommand) {
        let npc = &self.actors[me];
        if command.buttons & BUTTON_ATTACK == 0 || npc.player_team != NPCTEAM_ENEMY {
            return;
        }
        let (style, max_health, health) = (
            npc.player.raw_field(PS_SABER_ANIM_LEVEL).unwrap_or(0) as i32,
            npc.max_health,
            npc.health,
        );
        if self.host.irand(0, style) > 0
            && self.host.irand(0, max_health + 10) > health
            && self.host.irand(0, 3) == 0
        {
            let event = self.host.irand(EV_COMBAT1, EV_COMBAT1 + 2);
            self.add_voice(me, event, 1_000);
        }
    }

    /// `Jedi_Attack`'s Force speed (`6116-6146`): Tavion — and on harder skills Desann and
    /// the higher ranks — answer a player's speed with their own, the likelier the
    /// harder the skill.
    fn match_speed(&mut self, me: usize) {
        let npc = &self.actors[me];
        let class = npc.definition.client_class;
        if class == CLASS_BOBAFETT {
            return;
        }
        let skill = self.host.skill();
        let rank = npc.definition.rank;
        let answers = class == CLASS_TAVION
            || skill != 0
                && (class == CLASS_DESANN || rank >= self.host.irand(RANK_CREWMAN, RANK_CAPTAIN));
        if !answers {
            return;
        }
        let npc = &self.actors[me];
        let Some(enemy) = npc.mind.enemy.and_then(|enemy| self.body(enemy)) else {
            return;
        };
        let own_speed = npc.player.force_powers_active() & 1 << FP_SPEED != 0;
        if enemy.npc
            || own_speed
            || self.client_force_powers_active(enemy.number) & 1 << FP_SPEED == 0
        {
            return;
        }
        let chance = match skill {
            0 => 9,
            1 => 3,
            2 => 1,
            _ => 0,
        };
        if self.host.irand(0, chance) == 0 {
            self.force_speed(me, 0);
        }
    }

    /// `Jedi_PlayBlockedPushSound` (`NPC_AI_Jedi.c:142-153`): a living NPC's failed-push
    /// voice, three seconds apart.
    pub fn jedi_play_blocked_push_sound(&mut self, me: usize) {
        if self.speech_ready(me) {
            self.add_voice(me, EV_PUSHFAIL, 3_000);
            self.actors[me].mind.blocked_speech_until = self.level_time + 3_000;
        }
    }

    /// `Jedi_PlayDeflectSound` (`NPC_AI_Jedi.c:155-166`): a living NPC's deflection voice,
    /// three seconds apart.
    pub fn jedi_play_deflect_sound(&mut self, me: usize) {
        if self.speech_ready(me) {
            let event = self.host.irand(EV_DEFLECT1, EV_DEFLECT1 + 2);
            self.add_voice(me, event, 3_000);
            self.actors[me].mind.blocked_speech_until = self.level_time + 3_000;
        }
    }

    /// Alive and past `blockedSpeechDebounceTime`.
    fn speech_ready(&self, me: usize) -> bool {
        let npc = &self.actors[me];
        npc.health > 0 && npc.mind.blocked_speech_until < self.level_time
    }

    /// `NPC_Jedi_PlayConfusionSound` (`NPC_AI_Jedi.c:168-186`): Tavion and Desann are
    /// confused; the others taunt or gloat.
    pub fn jedi_play_confusion_sound(&mut self, me: usize) {
        if self.actors[me].health <= 0 {
            return;
        }
        let class = self.actors[me].definition.client_class;
        let first = if class == CLASS_TAVION || class == CLASS_DESANN {
            EV_CONFUSE1
        } else if self.host.irand(0, 1) != 0 {
            EV_TAUNT1
        } else {
            EV_GLOAT1
        };
        let event = self.host.irand(first, first + 2);
        self.add_voice(me, event, 2_000);
    }

    /// `Jedi_Cloak` (`NPC_AI_Jedi.c:804-821`).
    pub fn jedi_cloak(&mut self, me: usize) {
        crate::npc_begin::cloak(&mut self.actors[me], &mut *self.host);
    }

    /// `Jedi_Decloak` (`NPC_AI_Jedi.c:823-838`): targetable again, and uncloaked with a
    /// sound if it was cloaked.
    pub fn jedi_decloak(&mut self, me: usize) {
        let npc = &mut self.actors[me];
        npc.flags &= !FL_NOTARGET;
        if npc.player.powerups[PW_CLOAKED] != 0 {
            npc.player.powerups[PW_CLOAKED] = 0;
            crate::npc_begin::item_sound(
                npc,
                &mut *self.host,
                b"sound/chars/shadowtrooper/decloak.wav",
            );
        }
    }

    /// `Jedi_CheckCloak` (`NPC_AI_Jedi.c:840-861`): a shadowtrooper is cloaked while its
    /// saber is off and in hand and it is alive and not in pain.
    pub fn jedi_check_cloak(&mut self, me: usize) {
        let npc = &self.actors[me];
        if npc.definition.client_class != CLASS_SHADOWTROOPER {
            return;
        }
        let in_flight = npc.player.raw_field(PS_SABER_IN_FLIGHT).unwrap_or(0) != 0;
        let pain = npc.mind.fight.pain_debounce_time;
        if npc.player.saber_holstered() == 0
            || npc.health <= 0
            || in_flight
            || pain > self.level_time
        {
            self.jedi_decloak(me);
        } else if npc.health > 0 && !in_flight && pain < self.level_time {
            self.jedi_cloak(me);
        }
    }
}
