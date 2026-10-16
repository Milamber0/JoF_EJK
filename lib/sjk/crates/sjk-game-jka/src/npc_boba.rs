//! Boba Fett's own part of the Jedi AI (`CLASS_BOBAFETT`, `codemp/game/NPC_AI_Jedi.c`): his
//! weapon changes (`Boba_ChangeWeapon`, `213-221`), the jet pack (`Boba_FlyStart`,
//! `Boba_FlyStop`, `365-404` — flying, he runs the seeker's AI, [`crate::npc_seeker`]), the
//! flamethrower (`Boba_FireFlameThrower`, `Boba_StartFlameThrower`, `Boba_DoFlameThrower`,
//! `411-498`) and the choice of when and at what he fires (`Boba_FireDecide`, `500-802`),
//! which `Jedi_Attack` and the seeker's fight call.
//!
//! In multiplayer `NPC_ChangeWeapon` does nothing (`NPC_combat.c:860-889`): Boba Fett keeps
//! the weapon his NPC file gave him (the blaster), and every change asked for sounds
//! (`EV_GENERAL_SOUND`) — again each time, the weapon never becoming the one asked for. Nor
//! does multiplayer set `renderInfo.handLBolt` and `handRBolt` for him: the flamethrower
//! reads bolt 0 of his instance, `lower_lumbar`. Only his death stops his flight
//! (`g_combat.c:2344-2345`).
//!
//! Held to `tools/game-oracle/npcsniper.c` (`game-npcsoldier-boba.txt`).

use crate::npc_senses::{Spot, distance_squared, spot, subtract};
use crate::npc_sniper::{CHAN_WEAPON, SCF_ALT_FIRE, angle_vectors};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use crate::player_angle_math::{normalize, vector_angles};
use sjk_protocol::UserCommand;

/// `CLASS_BOBAFETT`.
pub(crate) const CLASS_BOBAFETT: i32 = 52;
/// `weapon_t`s he reads.
const WP_NONE: u8 = 0;
const WP_STUN_BATON: u8 = 1;
const WP_SABER: i32 = 3;
const WP_BLASTER: u8 = 5;
const WP_DISRUPTOR: u8 = 6;
const WP_REPEATER: u8 = 8;
const WP_FLECHETTE: u8 = 10;
const WP_ROCKET_LAUNCHER: u8 = 11;
const WP_THERMAL: u8 = 12;
const WP_TRIP_MINE: u8 = 13;
const WP_DET_PACK: u8 = 14;
const WP_EMPLACED_GUN: i32 = 17;
/// `MIN_ROCKET_DIST_SQUARED`.
const MIN_ROCKET_DIST_SQUARED: f32 = 16_384.0;
/// `SCF_FIRE_WEAPON`.
const SCF_FIRE_WEAPON: u32 = 0x4_0000;
/// `BUTTON_ATTACK`, `BUTTON_ALT_ATTACK`.
const BUTTON_ATTACK: u16 = 1;
const BUTTON_ALT_ATTACK: u16 = 128;
/// `EV_GENERAL_SOUND`; `CHAN_ITEM`.
const EV_GENERAL_SOUND: u32 = 76;
const CHAN_ITEM: u32 = 5;
/// `EF2_FLYING`; `NPCAI_CUSTOM_GRAVITY`; `Q3_INFINITE`.
pub(crate) const EF2_FLYING: u32 = 1 << 4;
const NPCAI_CUSTOM_GRAVITY: u32 = 0x20_0000;
const Q3_INFINITE: i32 = 16_777_216;
/// `ps.gravity`, `ps.weaponTime`, `ps.torsoTimer`, `ps.eFlags2`, `ps.groundEntityNum`.
const PS_GRAVITY: usize = 46;
const PS_WEAPON_TIME: usize = 10;
const PS_TORSO_TIMER: usize = 20;
const PS_EFLAGS2: usize = 103;
/// `ENTITYNUM_NONE`, `ENTITYNUM_WORLD`.
const ENTITYNUM_NONE: u16 = 1_023;
const ENTITYNUM_WORLD: u16 = 1_022;
/// `BOTH_FORCELIGHTNING_HOLD`.
const BOTH_FORCELIGHTNING_HOLD: u16 = 1_337;
/// `DAMAGE_NO_ARMOR`, `DAMAGE_NO_KNOCKBACK`, `DAMAGE_IGNORE_TEAM`; `MOD_LAVA`.
const DAMAGE_NO_ARMOR: u32 = 0x2;
const DAMAGE_NO_KNOCKBACK: u32 = 0x4;
const DAMAGE_IGNORE_TEAM: u32 = 0x100;
const MOD_LAVA: u32 = crate::means_of_death::MOD_LAVA;
/// How long a burst of flame lasts.
const FLAME_TIME: i32 = 4_000;

/// The weapons whose splash makes him keep his distance from where his shot would land.
fn explosive(weapon: u8, alternate: bool) -> bool {
    matches!(
        weapon,
        WP_ROCKET_LAUNCHER | WP_FLECHETTE | WP_THERMAL | WP_TRIP_MINE | WP_DET_PACK
    ) || (weapon == WP_REPEATER && alternate)
}

/// `BG_GiveMeVectorFromMatrix(matrix, NEGATIVE_Y)`.
fn matrix_negative_y(matrix: &[[f32; 4]; 3]) -> [f32; 3] {
    [-matrix[0][1], -matrix[1][1], -matrix[2][1]]
}

/// What `Boba_FireDecide` senses of its enemy this think.
#[derive(Clone, Copy, Debug, Default)]
struct BobaSense {
    enemy_los: bool,
    enemy_cs: bool,
    enemy_in_fov: bool,
    hit_ally: bool,
    shoot: bool,
    enemy_dist: f32,
    impact: [f32; 3],
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `Boba_ChangeWeapon(wp)` (`213-221`): unless his entity already shows it, the change
    /// asked for (which does nothing) and its sound.
    pub fn boba_change_weapon(&mut self, me: usize, weapon: u8) {
        if self.actors[me]
            .state
            .raw_field(crate::npc_spawn::es::WEAPON)
            .unwrap_or(0)
            == u32::from(weapon)
        {
            return;
        }
        let sound = self.host.sound_index(b"sound/weapons/change.wav");
        self.add_event(me, EV_GENERAL_SOUND, u32::from(sound));
    }

    /// `Boba_FlyStart` (`365-385`): with his jets recharged, no gravity and flight, his jet
    /// pack's time, the take-off and the hover's loop; the seeker's shots unlimited.
    pub fn boba_fly_start(&mut self, me: usize) {
        let level_time = self.level_time;
        if !self.actors[me].mind.timers.done("jetRecharge", level_time) {
            return;
        }
        let npc = &mut self.actors[me];
        npc.player.set_raw_field(PS_GRAVITY, 0);
        npc.ai_flags |= NPCAI_CUSTOM_GRAVITY;
        let flags = npc.player.raw_field(PS_EFLAGS2).unwrap_or(0) | EF2_FLYING;
        npc.player.set_raw_field(PS_EFLAGS2, flags);
        let burn = self.host.irand(3_000, 10_000);
        self.actors[me].mind.jet_pack_time = level_time + burn;
        self.sound_on_channel(me, CHAN_ITEM, b"sound/boba/jeton.wav");
        self.machine_loop_sound(me, b"sound/boba/jethover.wav");
        self.actors[me].count = Q3_INFINITE;
    }

    /// `Boba_FlyStop` (`387-404`): gravity again, no flight, no jet pack or hover's loop, no
    /// seeker's shots; the jets to recharge and the next chase jump held off.
    pub fn boba_fly_stop(&mut self, me: usize) {
        let level_time = self.level_time;
        let gravity = self.host.gravity() as i32;
        let npc = &mut self.actors[me];
        npc.player.set_raw_field(PS_GRAVITY, gravity as u32);
        npc.ai_flags &= !NPCAI_CUSTOM_GRAVITY;
        let flags = npc.player.raw_field(PS_EFLAGS2).unwrap_or(0) & !EF2_FLYING;
        npc.player.set_raw_field(PS_EFLAGS2, flags);
        npc.mind.jet_pack_time = 0;
        npc.state
            .set_raw_field(crate::npc_machine::ES_LOOP_SOUND, 0);
        npc.count = 0;
        let recharge = self.host.irand(1_000, 5_000);
        self.actors[me]
            .mind
            .timers
            .set("jetRecharge", level_time, recharge);
        let chase = self.host.irand(500, 2_000);
        self.actors[me]
            .mind
            .timers
            .set("jumpChaseDebounce", level_time, chase);
    }

    /// `Boba_Flying` (`406-409`).
    pub fn boba_flying(&self, me: usize) -> bool {
        self.actors[me].player.raw_field(PS_EFLAGS2).unwrap_or(0) & EF2_FLYING != 0
    }

    /// `G2API_GetBoltMatrix` of his `renderInfo` hand bolt (never set in multiplayer: bolt
    /// 0) at his `r.currentAngles` and `r.currentOrigin`, now.
    fn boba_hand_matrix(&mut self, me: usize, index: i32) -> [[f32; 4]; 3] {
        let npc = &self.actors[me];
        let bolt = npc.mind.creature.bolts.name(index);
        let (angles, origin, level_time) =
            (npc.mind.current_angles, npc.current_origin, self.level_time);
        self.host
            .npc_bolt_matrix(npc, index, bolt, angles, origin, level_time)
    }

    /// `Boba_FireFlameThrower` (`411-436`): a four-unit box traced 128 units from his left
    /// hand along it; what it meets that takes damage burns, 20 to 30.
    fn boba_fire_flamethrower(&mut self, me: usize) {
        let damage = self.host.irand(20, 30);
        let index = self.actors[me].mind.creature.render.hand_l;
        let matrix = self.boba_hand_matrix(me, index);
        let start = crate::npc_machine::matrix_origin(&matrix);
        let dir = matrix_negative_y(&matrix);
        let end: [f32; 3] = std::array::from_fn(|axis| start[axis] + 128.0 * dir[axis]);
        let number = self.actors[me].number;
        let trace = self.trace_bodies(
            start,
            [-4.0; 3],
            [4.0; 3],
            end,
            number,
            crate::npc_aim::MASK_SHOT,
        );
        let hit = trace.entity_number;
        if hit >= ENTITYNUM_WORLD {
            return;
        }
        let flags = DAMAGE_NO_ARMOR | DAMAGE_NO_KNOCKBACK | DAMAGE_IGNORE_TEAM;
        if self.body(hit).is_some() {
            if self
                .actor_at(hit)
                .is_none_or(|at| self.actors[at].takes_damage)
            {
                self.creature_damage(
                    me,
                    hit,
                    Some(dir),
                    Some(trace.end_position),
                    damage,
                    flags,
                    MOD_LAVA,
                );
            }
        } else if self.host.damageable_health(hit).is_some() {
            let npc = &self.actors[me];
            let attacker = crate::damage::Attacker {
                npc: true,
                client: number,
                max_health: npc.player.stats[crate::npc_begin::STAT_MAX_HEALTH] as i32,
                team: npc.session_team,
                saber_knockback: [0.0; 4],
            };
            let request = crate::damage::DamageRequest {
                level_time: self.level_time,
                attacker: Some(attacker),
                direction: Some(dir),
                point: Some(trace.end_position),
                damage,
                flags,
                means: MOD_LAVA,
            };
            self.host.saber_blow_on_entity(hit, request);
        }
    }

    /// `Boba_StartFlameThrower` (`438-488`): four seconds of flame — his torso held, his next
    /// attack and his walk put off — its roar, and its effect from his right hand.
    fn boba_start_flamethrower(&mut self, me: usize) {
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        npc.player.set_raw_field(PS_TORSO_TIMER, FLAME_TIME as u32);
        npc.mind
            .timers
            .set("nextAttackDelay", level_time, FLAME_TIME);
        npc.mind.timers.set("walking", level_time, 0);
        npc.mind.timers.set("flameTime", level_time, FLAME_TIME);
        self.sound_on_channel(me, CHAN_WEAPON, b"sound/effects/combustfire.mp3");
        let index = self.actors[me].mind.creature.render.hand_r;
        let matrix = self.boba_hand_matrix(me, index);
        let origin = crate::npc_machine::matrix_origin(&matrix);
        self.play_effect_at(b"boba/fthrw", origin, matrix_negative_y(&matrix));
    }

    /// `Boba_DoFlameThrower` (`490-498`): his torso in the lightning's hold; a new burst of
    /// flame when the last is out; the flame fired.
    fn boba_do_flamethrower(&mut self, me: usize) {
        use crate::pmove_anim::{SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_TORSO};
        self.set_animation(
            me,
            SETANIM_TORSO,
            BOTH_FORCELIGHTNING_HOLD,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        );
        let level_time = self.level_time;
        let timers = &self.actors[me].mind.timers;
        if timers.done("nextAttackDelay", level_time) && timers.done("flameTime", level_time) {
            self.boba_start_flamethrower(me);
        }
        self.boba_fire_flamethrower(me);
    }

    /// `Boba_FireDecide` (`500-802`): in the air after a jump, now and then his jets; the
    /// weapon for his enemy and his health; the flamethrower close in front of him; and — his
    /// attacks due — a shot at his enemy when it would strike it, or at where it was lately
    /// seen, now and then a homing rocket.
    pub fn boba_fire_decide(&mut self, me: usize, command: &mut UserCommand) {
        let npc = &self.actors[me];
        let jumped = npc.movement.state().force_jump_start_height != 0.0;
        if npc.player.ground_entity_num() == ENTITYNUM_NONE
            && jumped
            && !crate::saber_rules::flipping(npc.player.leg_animation())
            && self.host.irand(0, 10) == 0
        {
            self.boba_fly_start(me);
        }
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        else {
            return;
        };
        self.boba_choose_weapon(me, &enemy);
        let npc = &self.actors[me];
        let mut sense = BobaSense {
            enemy_dist: distance_squared(npc.current_origin, enemy.origin),
            ..BobaSense::default()
        };
        let (to_enemy, _) =
            crate::npc_jedi_patrol::normalized(subtract(enemy.origin, npc.current_origin));
        let (forward, _, _) = angle_vectors(npc.player.view_angles());
        let dot = to_enemy[0] * forward[0] + to_enemy[1] * forward[1] + to_enemy[2] * forward[2];
        sense.enemy_in_fov = dot > 0.5 || sense.enemy_dist * (1.0 - dot) < 10_000.0;
        let level_time = self.level_time;
        let weapon = npc.player.weapon();
        let alternate = npc.script_flags & SCF_ALT_FIRE != 0;
        if (sense.enemy_dist < 128.0 * 128.0 && sense.enemy_in_fov)
            || !npc.mind.timers.done("flameTime", level_time)
        {
            self.boba_do_flamethrower(me);
            self.actors[me].mind.tactics.enemy_last_seen_time = level_time;
            command.buttons &= !(BUTTON_ATTACK | BUTTON_ALT_ATTACK);
        } else if sense.enemy_dist < MIN_ROCKET_DIST_SQUARED {
            if (weapon == WP_FLECHETTE || weapon == WP_REPEATER) && alternate {
                self.actors[me].script_flags &= !SCF_ALT_FIRE;
            }
        } else if sense.enemy_dist > 65_536.0 && weapon == WP_DISRUPTOR && !alternate {
            self.actors[me].script_flags |= SCF_ALT_FIRE;
            self.update_angles(me, true, true, command);
            return;
        }
        let timers = &self.actors[me].mind.timers;
        if timers.done("nextAttackDelay", level_time) && timers.done("flameTime", level_time) {
            self.boba_sense(me, &enemy, &mut sense);
            self.boba_fire_on_last_seen(me, &mut sense);
            self.boba_shoot(me, sense, command);
        }
    }

    /// `Boba_FireDecide`'s weapon (`521-548`): the rocket launcher against a saber, else the
    /// blaster — on its alternate fire in bursts once he is down to half his health.
    fn boba_choose_weapon(&mut self, me: usize, enemy: &crate::npc_senses::Body) {
        if enemy.weapon == WP_SABER {
            self.actors[me].script_flags &= !SCF_ALT_FIRE;
            self.boba_change_weapon(me, WP_ROCKET_LAUNCHER);
            return;
        }
        let npc = &self.actors[me];
        if (npc.health as f32) < npc.max_health as f32 * 0.5 {
            self.actors[me].script_flags |= SCF_ALT_FIRE;
            self.boba_change_weapon(me, WP_BLASTER);
            let spacing = self.host.irand(300, 750);
            let fight = &mut self.actors[me].mind.fight;
            (
                fight.burst_min,
                fight.burst_mean,
                fight.burst_max,
                fight.burst_spacing,
            ) = (3, 12, 20, spacing);
        } else {
            self.actors[me].script_flags &= !SCF_ALT_FIRE;
            self.boba_change_weapon(me, WP_BLASTER);
        }
    }

    /// `Boba_FireDecide`'s sense of its enemy (`597-661`): in sight — a clear shot at its
    /// enemy in front of him, or one of its side, or glass or something breakable — or at
    /// least in his potentially visible set.
    fn boba_sense(&mut self, me: usize, enemy: &crate::npc_senses::Body, sense: &mut BobaSense) {
        let level_time = self.level_time;
        if self.clear_los4(me, enemy) {
            self.actors[me].mind.tactics.enemy_last_seen_time = level_time;
            sense.enemy_los = true;
            let npc = &self.actors[me];
            let weapon = npc.player.weapon();
            let alternate = npc.script_flags & SCF_ALT_FIRE != 0;
            if weapon == WP_NONE {
                sense.enemy_cs = false;
            } else if (weapon == WP_ROCKET_LAUNCHER || (weapon == WP_FLECHETTE && alternate))
                && sense.enemy_dist < MIN_ROCKET_DIST_SQUARED
            {
                sense.hit_ally = true;
            } else if sense.enemy_in_fov {
                let (hit, impact) = self.shot_entity(me, enemy);
                sense.impact = impact;
                if self.boba_shot_counts(me, hit, enemy.number) {
                    sense.enemy_cs = true;
                    self.actors[me].mind.tactics.enemy_last_seen_location = enemy.origin;
                } else {
                    let own = self.actors[me].player_team;
                    sense.hit_ally = self.body(hit).is_some_and(|body| body.player_team == own);
                }
            }
        } else if self
            .host
            .in_pvs(enemy.origin, self.actors[me].current_origin)
        {
            self.actors[me].mind.tactics.enemy_last_seen_time = level_time;
        }
        sense.shoot = self.actors[me].player.weapon() != WP_NONE && sense.enemy_cs;
    }

    /// A shot striking entity `hit` will do (`619-621`): his enemy, its side, or glass or
    /// something breakable enough.
    fn boba_shot_counts(&self, me: usize, hit: u16, enemy: u16) -> bool {
        if hit == enemy {
            return true;
        }
        let struck = self.body(hit);
        if struck.is_some_and(|body| body.player_team == self.actors[me].enemy_team) {
            return true;
        }
        let glass = self.host.glass(hit);
        let (takes_damage, health) = match struck {
            Some(body) => (
                self.actor_at(hit)
                    .is_none_or(|at| self.actors[at].takes_damage),
                body.health,
            ),
            None => {
                let health = self.host.damageable_health(hit);
                (health.is_some() || glass, health.unwrap_or(0))
            }
        };
        let emplaced = self.actors[me]
            .state
            .raw_field(crate::npc_spawn::es::WEAPON)
            .unwrap_or(0) as i32
            == WP_EMPLACED_GUN;
        takes_damage && (glass || health < 40 || emplaced)
    }

    /// `Boba_FireDecide`'s shot at where its enemy was lately seen (`663-765`): with no clear
    /// shot, nobody of his own in the way and its enemy in front, one time in eleven — when
    /// the shot would land neither too near himself nor, its enemy long unseen, too far from
    /// where it was.
    fn boba_fire_on_last_seen(&mut self, me: usize, sense: &mut BobaSense) {
        let level_time = self.level_time;
        let tactics = self.actors[me].mind.tactics;
        if sense.enemy_cs
            || sense.hit_ally
            || !sense.enemy_in_fov
            || tactics.enemy_last_seen_time <= 0
            || level_time - tactics.enemy_last_seen_time >= 10_000
        {
            return;
        }
        if self.host.irand(0, 10) != 0 {
            return;
        }
        let muzzle = spot(&self.npc(me), Spot::Head);
        if sense.impact == [0.0; 3] {
            let npc = &self.actors[me];
            let (forward, _, _) = angle_vectors(npc.player.view_angles());
            let end: [f32; 3] = std::array::from_fn(|axis| muzzle[axis] + 8_192.0 * forward[axis]);
            let number = npc.number;
            sense.impact = self
                .trace_bodies(
                    muzzle,
                    [0.0; 3],
                    [0.0; 3],
                    end,
                    number,
                    crate::npc_aim::MASK_SHOT,
                )
                .end_position;
        }
        let npc = &self.actors[me];
        let splash = explosive(
            npc.state
                .raw_field(crate::npc_spawn::es::WEAPON)
                .unwrap_or(0) as u8,
            npc.script_flags & SCF_ALT_FIRE != 0,
        );
        if distance_squared(sense.impact, muzzle) < if splash { 65_536.0 } else { 16_384.0 } {
            return;
        }
        let group_unseen = npc.mind.tactics.group.is_some_and(|group| {
            level_time - self.level.groups[group].last_seen_enemy_time > 5_000
        });
        if (level_time - tactics.enemy_last_seen_time > 5_000 || group_unseen)
            && distance_squared(sense.impact, tactics.enemy_last_seen_location)
                > if splash { 262_144.0 } else { 65_536.0 }
        {
            return;
        }
        let mut direction = subtract(tactics.enemy_last_seen_location, muzzle);
        normalize(&mut direction);
        let angles = vector_angles(direction);
        let npc = &mut self.actors[me];
        npc.desired_yaw = angles[1];
        npc.mind.desired_pitch = angles[0];
        sense.shoot = true;
    }

    /// `Boba_FireDecide`'s firing (`767-800`): a rocket in the air cancelled without a clear
    /// shot, else his next attack put off; otherwise, due, the weapon's think — and one rocket
    /// in four homing.
    fn boba_shoot(&mut self, me: usize, sense: BobaSense, command: &mut UserCommand) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        let rocket = npc
            .state
            .raw_field(crate::npc_spawn::es::WEAPON)
            .unwrap_or(0)
            == u32::from(WP_ROCKET_LAUNCHER);
        if npc.player.raw_field(PS_WEAPON_TIME).unwrap_or(0) as i32 > 0 {
            if rocket {
                if !sense.enemy_los || !sense.enemy_cs {
                    self.actors[me].player.set_raw_field(PS_WEAPON_TIME, 0);
                } else {
                    let delay = self.host.irand(500, 1_000);
                    self.actors[me]
                        .mind
                        .timers
                        .set("nextAttackDelay", level_time, delay);
                }
            }
            return;
        }
        if !sense.shoot
            || !self.actors[me]
                .mind
                .timers
                .done("nextAttackDelay", level_time)
        {
            return;
        }
        if self.actors[me].script_flags & SCF_FIRE_WEAPON == 0 {
            self.weapon_think(me, command);
        }
        if rocket && command.buttons & BUTTON_ATTACK != 0 && self.host.irand(0, 3) == 0 {
            command.buttons &= !BUTTON_ATTACK;
            command.buttons |= BUTTON_ALT_ATTACK;
            let time = self.host.irand(500, 1_500);
            self.actors[me]
                .player
                .set_raw_field(PS_WEAPON_TIME, time as u32);
        }
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `Jedi_FaceEnemy`'s lead (`NPC_AI_Jedi.c:3766-3786`): Boba Fett, not flaming, below
    /// half his health and with a gun that fires a bolt, aims where his enemy's eyes will be
    /// when a shot gets there (`WP_SpeedOfMissileForWeapon`: 500 for every weapon), give or
    /// take. `None` when he does not lead.
    pub(crate) fn boba_lead(
        &mut self,
        me: usize,
        enemy: u16,
        eyes: [f32; 3],
        enemy_eyes: [f32; 3],
    ) -> Option<[f32; 3]> {
        let npc = &self.actors[me];
        if npc.definition.client_class != CLASS_BOBAFETT
            || !npc.mind.timers.done("flameTime", self.level_time)
        {
            return None;
        }
        let weapon = npc
            .state
            .raw_field(crate::npc_spawn::es::WEAPON)
            .unwrap_or(0) as u8;
        let alternate = npc.script_flags & SCF_ALT_FIRE != 0;
        let unled = [
            WP_NONE,
            WP_DISRUPTOR,
            WP_THERMAL,
            WP_TRIP_MINE,
            WP_DET_PACK,
            WP_STUN_BATON,
        ]
        .contains(&weapon)
            || (weapon == WP_ROCKET_LAUNCHER && alternate);
        if unled || (npc.health as f32) >= npc.max_health as f32 * 0.5 {
            return None;
        }
        let velocity = self
            .jedi_client(enemy)
            .map_or([0.0; 3], |client| client.velocity);
        let apart = subtract(enemy_eyes, eyes);
        let distance = (f64::from(apart[0] * apart[0] + apart[1] * apart[1] + apart[2] * apart[2])
            .sqrt() as f32)
            / 500.0;
        let scale = distance * self.host.rng().flrand(0.95, 1.25);
        Some(std::array::from_fn(|axis| {
            enemy_eyes[axis] + scale * velocity[axis]
        }))
    }
}
