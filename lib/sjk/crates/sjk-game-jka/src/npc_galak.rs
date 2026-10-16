//! Galak's mech's AI (`codemp/game/NPC_AI_GalakMech.c`, `CLASS_GALAKMECH`): its behaviour
//! (`NPC_BSGM_Default`, `1251-1317`: its shield back once its armour is gone), its patrol,
//! the move toward its goal and the hold (`GM_Move`, `GM_HoldPosition`, `382-452`), the
//! state its fight keeps (`GM_CheckMoveState`, `GM_CheckFireState`, `460-576`), its laser's
//! warm-up (`NPC_GM_StartLaser`, `578-593`), its pain — taunts while it is strong, the
//! plain pain once it is not (`NPC_GM_Pain`, `256-374`) — and its dying body's explosions
//! (`GM_Dying`, `151-247`, from `CorpsePhysics`). The fight itself is
//! [`crate::npc_galak_attack`]. `NPC_GalakMech_Init` is the begin's
//! ([`crate::npc_begin`]); `GM_StartGloat` is reached only from code compiled out
//! (`#if 0`, `623-685`).
//!
//! The retail `galak_mech.npc` names `CLASS_GALAK_MECH`, which the class table lacks: the
//! retail mech is class -1 and never runs this AI. A modder's definition naming
//! `CLASS_GALAKMECH` does.

use crate::npc_machine_parts::{TURN_OFF, TURN_ON};
use crate::npc_senses::{Spot, distance_squared, spot};
use crate::npc_spawn::{FL_SHIELDED, FRAMETIME, NpcHost, NpcThink};
use crate::npc_world::NpcWorld;
use sjk_protocol::UserCommand;

/// `GALAK_SHIELD_HEALTH`; the shield's box (`shieldMins`, `shieldMaxs`).
const GALAK_SHIELD_HEALTH: u32 = 500;
const SHIELD_MINS: [f32; 3] = [-60.0, -60.0, -24.0];
const SHIELD_MAXS: [f32; 3] = [60.0, 60.0, 80.0];
/// `SCF_FIRE_WEAPON`; `BUTTON_WALKING`.
const SCF_FIRE_WEAPON: u32 = 0x4_0000;
const BUTTON_WALKING: u16 = 16;
/// `STAT_ARMOR`; `ps.standheight`, `ps.crouchheight`, `ps.electrifyTime`.
const STAT_ARMOR: usize = 5;
const PS_STANDHEIGHT: usize = 35;
const PS_CROUCHHEIGHT: usize = 36;
const PS_ELECTRIFY_TIME: usize = 73;
/// `s.time`: nothing sets it on an NPC, so its dying is over at once.
const ES_TIME: usize = 65;
/// `NIF_COLLISION`.
const NIF_COLLISION: i32 = 0x4;
/// `EV_PUSHED1`, `EV_DETECTED1`.
const EV_PUSHED1: i32 = 125;
const EV_DETECTED1: i32 = 141;
/// `MOD_REPEATER`, `MOD_REPEATER_ALT`.
const MOD_REPEATER: u32 = 12;
const MOD_REPEATER_ALT: u32 = 13;
/// `SCF_ALT_FIRE`.
const SCF_ALT_FIRE: u32 = 0x40;

/// `GM_Dying`'s surfaces of each arm, in the order it loses them (`159-198`): the surface,
/// whether an explosion goes off at its bolt, and whether a small one.
const RIGHT_ARM_PARTS: [(&str, bool, bool); 2] =
    [("r_hand", true, true), ("r_arm_middle", false, false)];
const LEFT_ARM_PARTS: [(&str, bool, bool); 4] = [
    ("l_hand", true, false),
    ("l_arm_wrist", false, false),
    ("l_arm_middle", false, false),
    ("l_arm_augment", false, false),
];
/// The bolts added as those surfaces go.
const RIGHT_ARM_BOLTS: [&str; 2] = ["*flasha", "*r_arm_elbow"];
const LEFT_ARM_BOLTS: [&str; 4] = [
    "*flashc",
    "*l_arm_cap_l_hand",
    "*l_arm_cap_l_hand",
    "*l_arm_elbow",
];

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSGM_Default` (`NPC_AI_GalakMech.c:1251-1317`): a scripted shot; its armour gone,
    /// its shield back (where the shield's box has room); then the patrol or the fight.
    pub fn bs_gm_default(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        if self.actors[me].script_flags & SCF_FIRE_WEAPON != 0 {
            self.weapon_think(me, command);
        }
        let npc = &self.actors[me];
        // "start regenerating the armor" is compiled out (`if (0)`).
        if npc.player.stats[STAT_ARMOR] as i32 <= 0
            && npc.mind.creature.walker.investigate_debounce_time < level_time
        {
            let (origin, number, mask) = (npc.current_origin, npc.number, npc.clip_mask);
            let trace = self.trace_bodies(origin, SHIELD_MINS, SHIELD_MAXS, origin, number, mask);
            if !trace.start_solid {
                let npc = &mut self.actors[me];
                (npc.mins, npc.maxs) = (SHIELD_MINS, SHIELD_MAXS);
                npc.player
                    .set_raw_field(PS_CROUCHHEIGHT, SHIELD_MAXS[2] as i32 as u32);
                npc.player
                    .set_raw_field(PS_STANDHEIGHT, SHIELD_MAXS[2] as i32 as u32);
                npc.player.stats[STAT_ARMOR] = GALAK_SHIELD_HEALTH;
                npc.mind.creature.walker.investigate_debounce_time = 0;
                npc.flags |= FL_SHIELDED;
                self.machine_surface(me, "torso_shield", TURN_ON);
            }
        }
        if self.actors[me].mind.enemy.is_none() {
            self.gm_patrol(me, command);
        } else {
            self.bs_gm_attack(me, command);
        }
    }

    /// `NPC_BSGM_Patrol` (`436-452`): an enemy of its team noticed is faced; else to its
    /// goal walking.
    pub(crate) fn gm_patrol(&mut self, me: usize, command: &mut UserCommand) {
        if self.check_player_team_stealth(me) {
            self.update_angles(me, true, true, command);
            return;
        }
        if self.update_goal(me, command).is_some() {
            command.buttons |= BUTTON_WALKING;
            self.move_to_goal(me, true, command);
        }
        self.update_angles(me, true, true, command);
    }

    /// `GM_HoldPosition` (`382-389`): its combat point given up, and its goal (no script
    /// waits on it).
    fn gm_hold_position(&mut self, me: usize) {
        let point = self.actors[me].mind.tactics.combat_point;
        self.free_combat_point(me, point, true);
        self.actors[me].mind.goal = None;
    }

    /// `GM_Move` (`396-428`): straight at its goal; bumping into its enemy, or failing to
    /// move, it holds where it is. Whether it moved.
    pub(crate) fn gm_move(&mut self, me: usize, command: &mut UserCommand) -> bool {
        self.actors[me].mind.combat_move = true;
        let moved = self.move_to_goal(me, true, command);
        let info = self.level.nav;
        if info.flags & NIF_COLLISION != 0
            && info.blocker.is_some()
            && info.blocker == self.actors[me].mind.enemy
        {
            self.gm_hold_position(me);
        }
        if !moved {
            self.gm_hold_position(me);
        }
        moved
    }

    /// `GM_CheckMoveState` (`460-480`): a goal that is not its enemy reached — or given up
    /// once its enemy is near and in sight — and a short wait before it attacks.
    pub(crate) fn gm_check_move_state(
        &mut self,
        me: usize,
        enemy_los: bool,
        enemy_distance: f32,
        command: &mut UserCommand,
    ) {
        let npc = &self.actors[me];
        let goal = npc.mind.goal;
        if goal.is_none() || goal == npc.mind.enemy {
            return;
        }
        let Some(target) = self.goal_origin(me) else {
            return;
        };
        let npc = &self.actors[me];
        let reached =
            crate::npc_nav::hit_nav_goal(npc.current_origin, npc.mins, npc.maxs, target, 16, false);
        if reached || (enemy_los && enemy_distance <= 10_000.0) {
            self.reached_goal(me, command);
            let delay = self.host.irand(250, 500);
            let level_time = self.level_time;
            self.actors[me]
                .mind
                .timers
                .set("attackDelay", level_time, delay);
        }
    }

    /// `GM_CheckFireState` (`488-576`): no clear shot and standing still, now and then a
    /// shot at where its enemy was last seen — unless it would land too near itself, or (the
    /// enemy unseen five seconds) too far from that place.
    pub(crate) fn gm_check_fire_state(&mut self, me: usize, state: &mut GmAttack) {
        if state.enemy_cs {
            return;
        }
        if self.actors[me].player.velocity() != [0.0; 3] {
            return;
        }
        let level_time = self.level_time;
        let seen = self.actors[me].mind.tactics.enemy_last_seen_time;
        if state.hit_ally || seen <= 0 || level_time - seen >= 10_000 || self.host.irand(0, 10) != 0
        {
            return;
        }
        let body = self.npc(me);
        let muzzle = spot(&body, Spot::Head);
        if state.impact == [0.0; 3] {
            // "never checked ShotEntity this frame, so must do a trace..."
            let forward = Self::forward_of(self.actors[me].player.view_angles());
            let end = std::array::from_fn(|axis| muzzle[axis] + 8_192.0 * forward[axis]);
            state.impact = self
                .trace_bodies(
                    muzzle,
                    [0.0; 3],
                    [0.0; 3],
                    end,
                    body.number,
                    crate::npc_aim::MASK_SHOT,
                )
                .end_position;
        }
        let npc = &self.actors[me];
        let lobbing = body.weapon == crate::npc_galak_attack::WP_REPEATER
            && npc.script_flags & SCF_ALT_FIRE != 0;
        let too_close =
            distance_squared(state.impact, muzzle) < if lobbing { 65_536.0 } else { 16_384.0 };
        let last_seen = npc.mind.tactics.enemy_last_seen_location;
        let too_far = !too_close
            && level_time - seen > 5_000
            && distance_squared(state.impact, last_seen)
                > if lobbing { 262_144.0 } else { 65_536.0 };
        if too_close || too_far {
            return;
        }
        let mut direction = crate::npc_senses::subtract(last_seen, muzzle);
        crate::player_angle_math::normalize(&mut direction);
        let angles = crate::player_angle_math::vector_angles(direction);
        let npc = &mut self.actors[me];
        npc.desired_yaw = angles[1];
        npc.mind.desired_pitch = angles[0];
        state.shoot = true;
        state.face_enemy = false;
    }

    /// `NPC_GM_StartLaser` (`578-593`): the laser's warm-up — its beam and the next attack
    /// timed from the torso's animation, the charge's effect and sound.
    pub(crate) fn gm_start_laser(&mut self, me: usize) {
        if self.actors[me].mind.creature.walker.lock_count != 0 {
            return;
        }
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        let torso = npc.player.torso_timer();
        npc.mind.timers.set("beamDelay", level_time, torso);
        npc.mind
            .timers
            .set("attackDelay", level_time, torso + 3_000);
        npc.mind.creature.walker.lock_count = 1;
        let origin = npc.current_origin;
        self.play_effect_at(b"galak/beam_warmup", origin, [0.0; 3]);
        self.sound_on_entity(me, b"sound/weapons/galak/lasercharge.wav");
    }

    /// `NPC_GM_Pain` (`256-374`): unless in a laser sweep or another special move, a taunt
    /// (four at most, while it is strong) or the plain pain; then — if the blow was its own
    /// lob sent back — a change between its lob and its rapid fire.
    pub(crate) fn gm_pain(&mut self, me: usize, attacker: Option<u16>, damage: i32, means: u32) {
        // "hitLoc = 1": never the antenna here (that is `NPC_Pain`'s to read).
        const HIT_LOCATION: i32 = 1;
        let level_time = self.level_time;
        let npc = &self.actors[me];
        if npc.mind.creature.walker.lock_count == 0 && npc.player.torso_timer() <= 0 {
            if npc.count < 4
                && npc.health > 100
                && HIT_LOCATION != crate::npc_machine_parts::HL_GENERIC1
            {
                if npc.mind.creature.walker.delay < level_time {
                    let speech = match npc.count {
                        1 => EV_PUSHED1 + 1,
                        2 => EV_PUSHED1 + 2,
                        3 => EV_DETECTED1,
                        _ => EV_PUSHED1,
                    };
                    let npc = &mut self.actors[me];
                    npc.count += 1;
                    npc.mind.blocked_speech_until = 0;
                    let debounce = self.host.irand(3_000, 5_000);
                    self.add_voice(me, speech, debounce);
                    let delay = self.host.irand(5_000, 7_000);
                    self.actors[me].mind.creature.walker.delay = level_time + delay;
                }
            } else {
                self.npc_pain(me, attacker, damage, means);
            }
        }
        // "He force-pushed my own lobfires back at me": a player keeps no `lastEnemy`.
        let sent_back = attacker
            .and_then(|number| self.actor_at(number))
            .is_some_and(|at| self.actors[at].mind.last_enemy == Some(self.actors[me].number));
        if !sent_back {
            return;
        }
        if means == MOD_REPEATER_ALT && self.host.irand(0, 2) == 0 {
            if self.actors[me].mind.timers.done("noRapid", level_time) {
                let npc = &mut self.actors[me];
                npc.script_flags &= !SCF_ALT_FIRE;
                npc.mind.creature.walker.alt_fire = false;
                let delay = self.host.irand(2_000, 6_000);
                self.actors[me].mind.timers.set("noLob", level_time, delay);
            } else {
                let delay = self.host.irand(1_000, 2_000);
                self.actors[me].mind.timers.set("noLob", level_time, delay);
            }
        } else if means == MOD_REPEATER && self.host.irand(0, 5) == 0 {
            if self.actors[me].mind.timers.done("noLob", level_time) {
                let npc = &mut self.actors[me];
                npc.script_flags |= SCF_ALT_FIRE;
                npc.mind.creature.walker.alt_fire = true;
                let delay = self.host.irand(2_000, 6_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("noRapid", level_time, delay);
            } else {
                let delay = self.host.irand(1_000, 2_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("noRapid", level_time, delay);
            }
        }
    }

    /// `GM_CreateExplosion` (`119-143`): a small or a medium explosion at the bolt, along its
    /// back.
    fn gm_create_explosion(&mut self, me: usize, bolt: i32, small: bool) {
        if bolt < 0 {
            return;
        }
        let (origin, back) = crate::npc_machine_parts::origin_and_back(self.bolt_matrix(me, bolt));
        self.play_effect_at(
            if small {
                b"env/small_explode2"
            } else {
                b"env/med_explode2"
            },
            origin,
            back,
        );
    }

    /// `GM_Dying` (`151-247`), from `CorpsePhysics`: four seconds from its `s.time` of
    /// sparks and explosions about its body — its arms' parts going — then one last huge
    /// explosion, and it is freed a frame on. Nothing sets an NPC's `s.time`: the four
    /// seconds are long over when it dies.
    pub(crate) fn gm_dying(&mut self, me: usize) {
        let level_time = self.level_time;
        let since = self.actors[me].state.raw_field(ES_TIME).unwrap_or(0) as i32;
        if level_time - since >= 4_000 {
            let origin = self.actors[me].current_origin;
            self.play_effect_at(b"galak/explode", origin, [0.0; 3]);
            self.actors[me].think = NpcThink::Free(level_time + FRAMETIME);
            return;
        }
        self.actors[me]
            .player
            .set_raw_field(PS_ELECTRIFY_TIME, (level_time + 1_000) as u32);
        if !self.actors[me]
            .mind
            .timers
            .done("dyingExplosion", level_time)
        {
            return;
        }
        match self.host.irand(1, 14) {
            1 => self.gm_lose_part(me, &RIGHT_ARM_PARTS, &RIGHT_ARM_BOLTS),
            2 => self.gm_lose_part(me, &LEFT_ARM_PARTS, &LEFT_ARM_BOLTS),
            3 | 4 => self.gm_explode_at(me, "*hip_fr", false),
            5 | 6 => self.gm_explode_at(me, "*shldr_l", false),
            7 | 8 => self.gm_explode_at(me, "*uchest_r", false),
            9 | 10 => {
                let head = self.actors[me].mind.creature.render.head;
                self.gm_create_explosion(me, head, false);
            }
            11 => self.gm_explode_at(me, "*l_leg_knee", true),
            12 => self.gm_explode_at(me, "*r_leg_knee", true),
            13 => self.gm_explode_at(me, "*l_leg_foot", true),
            _ => self.gm_explode_at(me, "*r_leg_foot", true),
        }
        let delay = self.host.irand(300, 1_100);
        self.actors[me]
            .mind
            .timers
            .set("dyingExplosion", level_time, delay);
    }

    /// An explosion at the bolt `name`.
    fn gm_explode_at(&mut self, me: usize, name: &'static str, small: bool) {
        let bolt = self.add_bolt(me, name);
        self.gm_create_explosion(me, bolt, small);
    }

    /// The first of an arm's surfaces still shown goes, its bolt added — the hand's with its
    /// explosion.
    fn gm_lose_part(
        &mut self,
        me: usize,
        parts: &[(&'static str, bool, bool)],
        bolts: &[&'static str],
    ) {
        for (index, (surface, exploding, small)) in parts.iter().enumerate() {
            if self.machine_surface_status(me, surface) != 0 {
                continue;
            }
            let bolt = self.add_bolt(me, bolts[index]);
            if *exploding {
                self.gm_create_explosion(me, bolt, *small);
            }
            self.machine_surface(me, surface, TURN_OFF);
            return;
        }
    }
}

/// `NPC_BSGM_Attack`'s file-wide state for one think (`enemyLOS4`, `enemyCS4`, `hitAlly4`,
/// `faceEnemy4`, `move4`, `shoot4`, `enemyDist4`, `impactPos4`).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GmAttack {
    pub enemy_los: bool,
    pub enemy_cs: bool,
    pub hit_ally: bool,
    pub face_enemy: bool,
    pub moving: bool,
    pub shoot: bool,
    pub enemy_distance: f32,
    pub impact: [f32; 3],
}
