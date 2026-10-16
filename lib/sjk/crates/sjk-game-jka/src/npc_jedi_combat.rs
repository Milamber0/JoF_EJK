//! A Jedi NPC's fight (`codemp/game/NPC_AI_Jedi.c:3631-3819`, `4225-4439`, `5123-5312`):
//! where its enemy will be (`Jedi_SetEnemyInfo`), facing it (`Jedi_FaceEnemy`), the enemy
//! in its cone (`Jedi_FindEnemyInCone`), the fight's frame (`Jedi_Combat`), deciding to
//! attack (`Jedi_AttackDecide`) and idling between attacks (`Jedi_CombatIdle`).
//!
//! The timers and the enemy's special moves are [`crate::npc_jedi_timers`]; the jumps
//! [`crate::npc_jedi_jump`].

use crate::npc_senses::{Body, Spot, spot};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use crate::player_angle_math::{angle_mod, vector_angles};
use crate::saber_clash::normalize;
use sjk_protocol::UserCommand;

/// `BUTTON_ATTACK`, `BUTTON_ALT_ATTACK`.
pub(crate) const BUTTON_ATTACK: u16 = 1;
pub(crate) const BUTTON_ALT_ATTACK: u16 = 128;
/// `WP_SABER`.
pub(crate) const WP_SABER: u8 = 3;
/// `FP_GRIP`, `FP_RAGE`; `FORCE_LEVEL_1`, `FORCE_LEVEL_2`.
pub(crate) const FP_GRIP: usize = 6;
pub(crate) const FP_RAGE: usize = 8;
const FORCE_LEVEL_1: i32 = 1;
const FORCE_LEVEL_2: i32 = 2;
/// `class_t` values the fight reads (`teams.h`).
pub(crate) mod class {
    pub const DESANN: i32 = 6;
    pub const JEDI: i32 = 18;
    pub const LUKE: i32 = 22;
    pub const REBORN: i32 = 37;
    pub const SHADOWTROOPER: i32 = 43;
    pub const TAVION: i32 = 47;
    pub const BOBAFETT: i32 = 52;
}
/// `rank_t`: `RANK_CREWMAN`, `RANK_LT_JG`, `RANK_LT`, `RANK_COMMANDER`.
pub(crate) mod rank {
    pub const CREWMAN: i32 = 1;
    pub const LT_JG: i32 = 3;
    pub const LT: i32 = 4;
    pub const COMMANDER: i32 = 6;
}
/// `NPCTEAM_PLAYER`.
const NPCTEAM_PLAYER: i32 = 2;
/// `SCF_DONT_FIRE`, `SCF_NO_ACROBATICS`; `NPCAI_BLOCKED`; `FL_GODMODE`.
pub(crate) const SCF_DONT_FIRE: u32 = 0x4000;
pub(crate) const SCF_NO_ACROBATICS: u32 = 0x80_0000;
const NPCAI_BLOCKED: u32 = 0x40;
const FL_GODMODE: u32 = 0x10;
/// `ENTITYNUM_NONE`.
pub(crate) const ENTITYNUM_NONE: u16 = 1_023;
/// `BLOCKED_NONE`, `BLOCKED_PARRY_BROKEN`, `BLOCKED_ATK_BOUNCE`.
const BLOCKED_NONE: u32 = 0;
const BLOCKED_PARRY_BROKEN: u32 = 2;
const BLOCKED_ATK_BOUNCE: u32 = 3;
/// `HANDEXTEND_NONE`, `HANDEXTEND_JEDITAUNT`.
const HANDEXTEND_NONE: u32 = 0;
const HANDEXTEND_JEDITAUNT: u32 = 16;
/// `EV_JLOST1` (`bg_public.h`: `EV_ANGER1` plus 65).
const EV_JLOST1: i32 = crate::npc_enemy::EV_ANGER1 + 65;
/// Animations the fight reads.
const BOTH_A2_STABBACK1: u16 = 854;
const BOTH_ATTACK_BACK: u16 = 855;
const BOTH_CROUCHATTACKBACK1: u16 = 860;
const BOTH_FORCE_RAGE: u16 = 1_350;
/// Player-state fields read or written without an accessor: `weaponTime`,
/// `fd.saberAnimLevel`, `saberMove`, `saberBlocked`, `forceHandExtend`, `saberHolstered`,
/// `saberInFlight`.
pub(crate) const PS_WEAPON_TIME: usize = 10;
pub(crate) const PS_SABER_ANIM_LEVEL: usize = 23;
pub(crate) const PS_SABER_BLOCKED: usize = 77;
pub(crate) const PS_FORCE_HAND_EXTEND: usize = 80;
pub(crate) const PS_SABER_HOLSTERED: usize = 81;
pub(crate) const PS_SABER_IN_FLIGHT: usize = 88;
/// `fd.forcePowersActive`.
pub(crate) const PS_FORCE_POWERS_ACTIVE: usize = 82;

/// Where a Jedi's enemy is, and where it is headed (`Jedi_SetEnemyInfo`'s outputs).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EnemyInfo {
    /// Where the enemy will be `prediction` milliseconds on.
    pub dest: [f32; 3],
    /// From the NPC to there, normalised.
    pub dir: [f32; 3],
    /// How far — a client's from the tip of the NPC's first blade.
    pub dist: f32,
    /// Which way the enemy moves, normalised, and how fast.
    pub movedir: [f32; 3],
    pub movespeed: f32,
}

/// What the Jedi AI reads of another client (a player or an NPC): its linked place and box,
/// its motion and pose, its saber, its enemy.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClientView {
    /// Its entity number.
    pub number: u16,
    /// `r.currentOrigin`, `r.mins`, `r.maxs`, `r.currentAngles`.
    pub origin: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub current_angles: [f32; 3],
    /// `ps.velocity`, `ps.legsAnim`, `ps.groundEntityNum`.
    pub velocity: [f32; 3],
    pub legs_anim: u16,
    pub ground_entity: u16,
    /// `ps.saberMove`, `ps.saberLockTime`, `ps.weapon` (and `s.weapon`), `BG_SabersOff`.
    pub saber_move: u32,
    pub saber_lock_time: i32,
    pub weapon: u8,
    pub sabers_off: bool,
    /// `ps.fd.forcePowersActive`.
    pub force_powers_active: u32,
    /// `attackDebounceTime`, `enemy`, `health`.
    pub attack_debounce_time: i32,
    pub enemy: Option<u16>,
    pub health: i32,
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `TIMER_Done(NPC, name)`.
    pub(crate) fn jedi_timer_done(&self, me: usize, name: &str) -> bool {
        self.actors[me].mind.timers.done(name, self.level_time)
    }

    /// `TIMER_Set(NPC, name, duration)`.
    pub(crate) fn jedi_timer_set(&mut self, me: usize, name: &'static str, duration: i32) {
        let level_time = self.level_time;
        self.actors[me].mind.timers.set(name, level_time, duration);
    }

    /// Entity `number` as the Jedi AI reads a client: an NPC's own record, a player's
    /// through the host; `None` for anything that is no client.
    pub fn client_view(&self, number: u16) -> Option<ClientView> {
        self.jedi_client(number).map(|client| client.view())
    }

    /// `Jedi_FindEnemyInCone` (`NPC_AI_Jedi.c:3631-3706`): within 1024 units, a living
    /// client of the NPC's enemy team it might see, ahead of it by at least `min_dot` and
    /// in clear line of fire (`MASK_SHOT`). The reference never lowers its best distance
    /// (`dist = bestDist`), so the last such found wins; `fallback` when there is none.
    /// The bodies are met players first, then NPCs, each in entity order (`EntitiesInBox`'
    /// order is the area tree's, which decides only which of several wins).
    pub fn jedi_find_enemy_in_cone(
        &mut self,
        me: usize,
        fallback: Option<u16>,
        min_dot: f32,
    ) -> Option<u16> {
        let npc = self.npc(me);
        let forward = crate::pmove::flight::flight_axes(npc.view_angles)
            .0
            .to_array();
        let mut enemy = fallback;
        for player in 0..self.host.players().len() {
            let check = self.host.players()[player];
            if self.jedi_in_cone(me, &npc, &check, forward, min_dot) {
                enemy = Some(check.number);
            }
        }
        let order = self.order;
        for &at in order {
            let check = self.npc(at);
            if self.jedi_in_cone(me, &npc, &check, forward, min_dot) {
                enemy = Some(check.number);
            }
        }
        enemy
    }

    /// One body of `Jedi_FindEnemyInCone`'s box (`NPC_AI_Jedi.c:3657-3703`): another living
    /// client of the NPC's enemy team whose box meets 1024 units round it, in its PVS, ahead
    /// by `min_dot` and in a clear line of fire. (`dist < bestDist` is against the never
    /// lowered `Q3_INFINITE`: always true.)
    fn jedi_in_cone(
        &mut self,
        me: usize,
        npc: &Body,
        check: &Body,
        forward: [f32; 3],
        min_dot: f32,
    ) -> bool {
        let origin = npc.origin;
        // `EntitiesInBox` meets the linked box, a unit larger each way.
        let inside = (0..3).all(|axis| {
            check.origin[axis] + check.maxs[axis] + 1.0 >= origin[axis] - 1024.0
                && check.origin[axis] + check.mins[axis] - 1.0 <= origin[axis] + 1024.0
        });
        if check.number == npc.number
            || !inside
            || check.player_team != self.actors[me].enemy_team
            || check.health <= 0
        {
            return false;
        }
        if !self.host.in_pvs(check.origin, origin) {
            return false;
        }
        let mut dir = crate::npc_senses::subtract(check.origin, origin);
        normalize(&mut dir);
        if dir[0] * forward[0] + dir[1] * forward[1] + dir[2] * forward[2] < min_dot {
            return false;
        }
        let trace = self.trace_bodies(
            origin,
            [0.0; 3],
            [0.0; 3],
            check.origin,
            npc.number,
            crate::npc_aim::MASK_SHOT,
        );
        trace.fraction >= 1.0 || trace.entity_number == check.number
    }

    /// `Jedi_SetEnemyInfo` (`NPC_AI_Jedi.c:3708-3741`): where the enemy will be
    /// `prediction` milliseconds on and how far that is — a client's distance measured from
    /// the tip of the NPC's first blade (`lengthMax + maxs[0] * 1.5 + 16`). All zero without
    /// an enemy.
    pub fn jedi_set_enemy_info(&mut self, me: usize, prediction: i32) -> EnemyInfo {
        let mut info = EnemyInfo::default();
        let Some(enemy) = self.actors[me].mind.enemy else {
            return info;
        };
        let npc = &self.actors[me];
        let origin = npc.current_origin;
        match self.client_view(enemy) {
            None => {
                let Some((enemy_origin, mins, _)) = self.host.entity_box(enemy) else {
                    return info;
                };
                info.dest = enemy_origin;
                info.dest[2] += mins[2] + 24.0;
                info.dir = crate::npc_senses::subtract(info.dest, origin);
                info.dist = normalize(&mut info.dir);
            }
            Some(client) => {
                info.movedir = client.velocity;
                info.movespeed = normalize(&mut info.movedir);
                // `movespeed * 0.001 * prediction`: a double, as is `VectorMA`'s sum.
                let scale = f64::from(info.movespeed) * 0.001 * f64::from(prediction);
                info.dest = std::array::from_fn(|axis| {
                    (f64::from(client.origin[axis]) + scale * f64::from(info.movedir[axis])) as f32
                });
                info.dir = crate::npc_senses::subtract(info.dest, origin);
                let length = normalize(&mut info.dir);
                let reach = f64::from(npc.definition.sabers[0].blades[0].length_max)
                    + f64::from(npc.maxs[0]) * 1.5
                    + 16.0;
                info.dist = (f64::from(length) - reach) as f32;
            }
        }
        info
    }

    /// `Jedi_FaceEnemy` (`NPC_AI_Jedi.c:3744-3819`): the desired angles from its head to the
    /// enemy's — away from it in a backward attack — unchanged while it grips above level 1;
    /// a thrown saber tilts the pitch down ten degrees; Boba Fett, hurt, leads his enemy
    /// ([`Self::boba_lead`]). An enemy with no body (no client) is not faced.
    pub fn jedi_face_enemy(&mut self, me: usize, do_pitch: bool, _command: &mut UserCommand) {
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        else {
            return;
        };
        let npc = &mut self.actors[me];
        let active = npc.player.force_powers_active();
        if active & (1 << FP_GRIP) != 0 && npc.force_levels[FP_GRIP] > FORCE_LEVEL_1 {
            let view = npc.player.view_angles();
            npc.mind.desired_pitch = view[0];
            npc.desired_yaw = view[1];
            return;
        }
        let eyes = spot(&self.npc(me), Spot::Head);
        let mut enemy_eyes = spot(&enemy, Spot::Head);
        if let Some(lead) = self.boba_lead(me, enemy.number, eyes, enemy_eyes) {
            enemy_eyes = lead;
        }
        let npc = &mut self.actors[me];
        let in_flight = npc.player.raw_field(PS_SABER_IN_FLIGHT).unwrap_or(0) != 0;
        let legs = npc.player.leg_animation();
        let backward = !in_flight
            && [BOTH_A2_STABBACK1, BOTH_CROUCHATTACKBACK1, BOTH_ATTACK_BACK].contains(&legs);
        let angles = if backward {
            vector_angles(crate::npc_senses::subtract(eyes, enemy_eyes))
        } else {
            vector_angles(crate::npc_senses::subtract(enemy_eyes, eyes))
        };
        npc.desired_yaw = angle_mod(angles[1]);
        if do_pitch {
            npc.mind.desired_pitch = angle_mod(angles[0]);
            if in_flight {
                npc.mind.desired_pitch += 10.0;
            }
        }
    }

    /// Whether the NPC grips at level 2 or above (`Jedi_Combat`'s "gripping" tests).
    fn jedi_gripping_hard(&self, me: usize) -> bool {
        let npc = &self.actors[me];
        npc.player.force_powers_active() & (1 << FP_GRIP) != 0
            && npc.force_levels[FP_GRIP] >= FORCE_LEVEL_2
    }

    /// `Jedi_Combat`'s end of a parry (`NPC_AI_Jedi.c:5183-5190`, `5253-5260`): once the
    /// parry's time is up, a block not the NPC's own undoing is lowered.
    fn jedi_finish_parry(&mut self, me: usize) {
        if !self.jedi_timer_done(me, "parryTime") {
            return;
        }
        let npc = &mut self.actors[me];
        let blocked = npc.player.raw_field(PS_SABER_BLOCKED).unwrap_or(0);
        if blocked != BLOCKED_ATK_BOUNCE && blocked != BLOCKED_PARRY_BROKEN {
            npc.player.set_raw_field(PS_SABER_BLOCKED, BLOCKED_NONE);
        }
    }

    /// `Jedi_Combat` (`NPC_AI_Jedi.c:5123-5312`): where the enemy will be in 300 ms; in the
    /// middle of a jump, only the attack decision; without a clear path, a jump at the enemy
    /// or a hunt for it (or a jump to where the move was blocked); then the timers, the
    /// distance kept, the enemy's last seen place, facing it, the evasion of a saber, the
    /// attack or the idling, the enemy's special moves, safe jumps, and no strafing into
    /// walls or off ledges.
    pub fn jedi_combat(&mut self, me: usize, command: &mut UserCommand) {
        let info = self.jedi_set_enemy_info(me, 300);
        let Some(enemy) = self.actors[me].mind.enemy else {
            return;
        };
        if self.jedi_jumping(me, Some(enemy), command) {
            self.jedi_attack_decide(me, info.dist as i32, command);
            return;
        }
        let mut enemy_lost = false;
        if !self.jedi_gripping_hard(me) && !self.jedi_clear_path_to_spot(me, info.dest, enemy) {
            match self.jedi_combat_no_path(me, enemy, info.dist, command) {
                NoPath::Done => return,
                NoPath::Lost => enemy_lost = true,
            }
        }
        self.jedi_combat_timers_update(me, info.dist as i32, command);
        self.jedi_combat_distance(me, info.dist as i32, command);
        if !enemy_lost {
            let enemy_view = self.client_view(enemy);
            let npc_ground = self.actors[me].player.ground_entity_num();
            let origin = enemy_view
                .map(|view| view.origin)
                .or_else(|| self.host.entity_box(enemy).map(|(origin, _, _)| origin))
                .unwrap_or([0.0; 3]);
            let level_time = self.level_time;
            let tactics = &mut self.actors[me].mind.tactics;
            if enemy_view.is_none_or(|view| {
                view.ground_entity != ENTITYNUM_NONE && npc_ground != ENTITYNUM_NONE
            }) {
                tactics.enemy_last_seen_location = origin;
            }
            tactics.enemy_last_seen_time = level_time;
        }
        if self.jedi_timer_done(me, "noturn") {
            self.jedi_face_enemy(me, true, command);
        }
        self.update_angles(me, true, true, command);
        self.jedi_finish_parry(me);
        let enemy_weapon = self.body(enemy).map(|body| body.weapon).unwrap_or(0);
        if enemy_weapon == i32::from(WP_SABER) {
            self.jedi_evasion_saber(me, info.movedir, info.dist, info.dir, command);
        }
        self.jedi_timers_apply(me, command);
        let in_flight = self.actors[me]
            .player
            .raw_field(PS_SABER_IN_FLIGHT)
            .unwrap_or(0)
            != 0;
        if !in_flight && !self.jedi_gripping_hard(me) {
            if !self.jedi_attack_decide(me, info.dist as i32, command) {
                self.jedi_combat_idle(me, info.dist as i32, command);
            } else {
                let level_time = self.level_time;
                self.jedi_timer_set(me, "taunting", -level_time);
            }
        }
        if self.actors[me].definition.client_class == class::BOBAFETT {
            self.boba_fire_decide(me, command);
        }
        self.jedi_check_enemy_movement(me, info.dist, command);
        self.jedi_check_jumps(me, command);
        if !self.npc_move_dir_clear(
            me,
            i32::from(command.forward_move),
            i32::from(command.right_move),
            true,
            command,
        ) {
            if self.level.nav.flags & crate::npc_nav::NIF_MACRO_NAV == 0 {
                self.move_to_goal(me, false, command);
            }
            self.jedi_timer_set(me, "strafeLeft", 0);
            self.jedi_timer_set(me, "strafeRight", 0);
        }
    }

    /// `Jedi_Combat` without a clear path to the enemy (`NPC_AI_Jedi.c:5141-5224`): seen
    /// lately and faced, a jump at it; else, the parry over, a hunt for it (a word now and
    /// then when it is out of sight), or a jump to where the move was blocked.
    fn jedi_combat_no_path(
        &mut self,
        me: usize,
        enemy: u16,
        enemy_dist: f32,
        command: &mut UserCommand,
    ) -> NoPath {
        let seen = self
            .body(enemy)
            .is_some_and(|body| self.clear_los4(me, &body))
            || self.actors[me].mind.tactics.enemy_last_seen_time > self.level_time - 500;
        if seen
            && self.face_enemy(me, true, command)
            && self.jedi_try_jump(me, Some(enemy), command)
        {
            return NoPath::Done;
        }
        self.jedi_finish_parry(me);
        if self.jedi_hunt(me, command) && self.actors[me].ai_flags & NPCAI_BLOCKED == 0 {
            if enemy_dist < 384.0
                && self.host.irand(0, 10) == 0
                && self.actors[me].mind.blocked_speech_until < self.level_time
                && self.jedi_speech_debounce(me) < self.level_time
                && !self
                    .body(enemy)
                    .is_some_and(|body| self.clear_los4(me, &body))
            {
                let event = self.host.irand(EV_JLOST1, EV_JLOST1 + 2);
                self.add_voice(me, event, 3000);
                let until = self.level_time + 3000;
                self.jedi_hush(me, until);
            }
            return NoPath::Done;
        }
        if self.actors[me].ai_flags & NPCAI_BLOCKED != 0 {
            // `G_Spawn` a goal at the blocked destination, try to jump there, free it.
            let dest = self.actors[me].mind.tactics.blocked_dest;
            if let Some(number) = self.host.spawn_entity() {
                let jumped = self.jedi_try_jump_to(
                    me,
                    crate::npc_jedi_jump::JumpGoal {
                        number,
                        origin: dest,
                        client_on_ground: None,
                    },
                    command,
                );
                self.host.free(number);
                if jumped {
                    return NoPath::Done;
                }
            }
        }
        NoPath::Lost
    }

    /// `Jedi_CombatIdle` (`NPC_AI_Jedi.c:4225-4287`): not parrying, not throwing, not
    /// raging and 64 units or more away, an aggressive Jedi taunts now and then — far enough
    /// off, it may even put its saber away and taunt with its hand.
    pub fn jedi_combat_idle(&mut self, me: usize, enemy_dist: i32, _command: &mut UserCommand) {
        if !self.jedi_timer_done(me, "parryTime") {
            return;
        }
        let npc = &self.actors[me];
        if npc.player.raw_field(PS_SABER_IN_FLIGHT).unwrap_or(0) != 0 {
            return;
        }
        if npc.player.force_powers_active() & (1 << FP_RAGE) != 0
            || npc.player.force_rage_recovery_time() > self.level_time
        {
            return;
        }
        if enemy_dist < 64 {
            return;
        }
        let chance = if npc.definition.client_class == class::SHADOWTROOPER {
            10
        } else {
            20
        };
        if self.host.irand(2, chance) >= self.actors[me].definition.stats.aggression {
            return;
        }
        let npc = &self.actors[me];
        if !self.jedi_timer_done(me, "chatter")
            || npc.player.raw_field(PS_FORCE_HAND_EXTEND).unwrap_or(0) != HANDEXTEND_NONE
        {
            return;
        }
        let holstered = npc.player.raw_field(PS_SABER_HOLSTERED).unwrap_or(0) != 0;
        if enemy_dist > 200
            && npc.definition.client_class != class::BOBAFETT
            && !holstered
            && self.host.irand(0, 5) == 0
        {
            self.deactivate_saber(me, false);
            self.actors[me].definition.stats.aggression = 3;
            if self.actors[me].player_team != NPCTEAM_PLAYER && self.host.irand(0, 1) == 0 {
                let until = self.level_time + 5000;
                self.actors[me]
                    .player
                    .set_raw_field(PS_FORCE_HAND_EXTEND, HANDEXTEND_JEDITAUNT);
                self.set_force_hand_extend_time(me, until);
                let chatter = self.host.irand(5000, 10_000);
                self.jedi_timer_set(me, "chatter", chatter);
                self.jedi_timer_set(me, "taunting", 5500);
            } else {
                self.jedi_battle_taunt(me);
                let taunting = self.host.irand(5000, 10_000);
                self.jedi_timer_set(me, "taunting", taunting);
            }
        } else {
            self.jedi_battle_taunt(me);
        }
    }

    /// `Jedi_AttackDecide` (`NPC_AI_Jedi.c:4289-4439`): whether the NPC attacks — the
    /// cultist destroyer's blast; nothing while the enemy is locked and it is not; a won
    /// lock pressed by chance; Tavion, fencers and trainers following a parry at once; else,
    /// within 64 units, not parrying and allowed to, a swing (`WeaponThink`), a quarter of
    /// them sideways away from the enemy.
    pub fn jedi_attack_decide(
        &mut self,
        me: usize,
        enemy_dist: i32,
        command: &mut UserCommand,
    ) -> bool {
        let npc = &self.actors[me];
        if crate::npc_behavior::cultist_destroyer(
            npc.definition.client_class,
            npc.player.weapon(),
            &npc.npc_type,
        ) {
            return enemy_dist <= 32 && self.jedi_destroyer_blast(me);
        }
        let Some(enemy) = npc.mind.enemy else {
            return false;
        };
        let enemy_client = self.client_view(enemy);
        let level_time = self.level_time;
        let npc = &self.actors[me];
        if enemy_client
            .is_some_and(|view| view.weapon == WP_SABER && view.saber_lock_time > level_time)
            && npc.player.saber_lock_time() < level_time
        {
            return false;
        }
        if npc.saber.event_flags & crate::saber_clash::sef::LOCK_WON != 0
            && self.jedi_press_won_lock(me, command)
        {
            return true;
        }
        let npc = &self.actors[me];
        let (class, rank) = (npc.definition.client_class, npc.definition.rank);
        let follows_parry = class == class::TAVION
            || (class == class::REBORN && rank == rank::LT_JG)
            || (class == class::JEDI && rank == rank::COMMANDER);
        let saber_move = npc.player.saber_move();
        if follows_parry
            && (crate::saber_rules::in_parry(saber_move)
                || crate::saber_rules::in_knockaway(saber_move))
            && npc.player.raw_field(PS_SABER_BLOCKED).unwrap_or(0) != BLOCKED_PARRY_BROKEN
        {
            self.jedi_clear_to_swing(me);
            self.jedi_adjust_saber_anim_level(me, FORCE_LEVEL_1);
            self.weapon_think(me, command);
            return true;
        }
        if enemy_dist >= 64
            || !self.jedi_timer_done(me, "parryTime")
            || self.actors[me].script_flags & SCF_DONT_FIRE != 0
        {
            return false;
        }
        if command.buttons & (BUTTON_ATTACK | BUTTON_ALT_ATTACK) == 0 {
            self.weapon_think(me, command);
        }
        if command.buttons & BUTTON_ATTACK == 0 {
            return false;
        }
        if command.right_move == 0 && self.host.irand(0, 3) == 0 {
            // `dir2enemy` is taken from `r.currentAngles` as a point, as the reference has it.
            let npc = &self.actors[me];
            let right = crate::pmove::flight::flight_axes(npc.mind.current_angles)
                .1
                .to_array();
            let enemy_origin = self.body(enemy).map(|body| body.origin).unwrap_or([0.0; 3]);
            let to_enemy = crate::npc_senses::subtract(enemy_origin, npc.mind.current_angles);
            command.right_move =
                if right[0] * to_enemy[0] + right[1] * to_enemy[1] + right[2] * to_enemy[2] > 0.0 {
                    -127
                } else {
                    127
                };
            self.actors[me].mind.move_dir = [0.0; 3];
        }
        true
    }

    /// `weaponTime = shotTime = attackDebounceTime = 0`, no block raised: free to swing at
    /// once (`NPC_AI_Jedi.c:4345-4347`, `4360-4362`).
    fn jedi_clear_to_swing(&mut self, me: usize) {
        let npc = &mut self.actors[me];
        npc.player.set_raw_field(PS_WEAPON_TIME, 0);
        npc.mind.fight.shot_time = 0;
        npc.mind.attack_debounce_time = 0;
        npc.player.set_raw_field(PS_SABER_BLOCKED, BLOCKED_NONE);
    }

    /// A won saber lock pressed with an attack (`NPC_AI_Jedi.c:4321-4351`), by the NPC's
    /// class and rank against a roll of thirty.
    fn jedi_press_won_lock(&mut self, me: usize, command: &mut UserCommand) -> bool {
        let npc = &self.actors[me];
        let class = npc.definition.client_class;
        let chance = if class == class::DESANN
            || class == class::LUKE
            || npc.npc_type.eq_ignore_ascii_case(b"yoda")
        {
            20
        } else if class == class::TAVION {
            10
        } else if class == class::REBORN && npc.definition.rank == rank::LT_JG {
            5
        } else {
            npc.definition.rank
        };
        if self.host.irand(0, 30) >= chance {
            return false;
        }
        self.actors[me].saber.event_flags &= !crate::saber_clash::sef::LOCK_WON;
        let hold = self.host.irand(500, 2000);
        self.jedi_timer_set(me, "noRetreat", hold);
        self.jedi_clear_to_swing(me);
        self.weapon_think(me, command);
        true
    }

    /// The cultist destroyer's blast within 32 units (`NPC_AI_Jedi.c:4294-4309`): god mode,
    /// no damage taken, the rage pose held and rage on, pain and use held off for it.
    fn jedi_destroyer_blast(&mut self, me: usize) -> bool {
        use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
        let npc = &mut self.actors[me];
        npc.flags |= FL_GODMODE;
        npc.takes_damage = false;
        self.set_animation(
            me,
            SETANIM_BOTH,
            BOTH_FORCE_RAGE,
            SETANIM_FLAG_HOLD | SETANIM_FLAG_OVERRIDE,
        );
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        let active = npc.player.force_powers_active() | (1 << FP_RAGE);
        npc.player.set_raw_field(PS_FORCE_POWERS_ACTIVE, active);
        let until = level_time + npc.player.torso_timer();
        npc.mind.fight.pain_debounce_time = until;
        npc.mind.use_debounce_time = until;
        true
    }
}

/// What `Jedi_Combat` does when it cannot get straight at its enemy.
enum NoPath {
    /// It jumped or hunts: the frame is over.
    Done,
    /// It lost its enemy: the fight goes on without a last-seen update.
    Lost,
}
