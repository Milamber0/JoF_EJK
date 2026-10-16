//! Galak's mech in a fight (`NPC_BSGM_Attack`, `codemp/game/NPC_AI_GalakMech.c:614-1249`):
//! its laser sweep (the warm-up, then a trace from its `renderInfo` muzzle that burns what
//! it meets, a looping sound carried by an entity of its own), its swing at an enemy right
//! in front of it, its switch between the repeater's rapid fire and its lobbed alternate
//! by the enemy's distance, the lob's arc (`WP_LobFire`, `g_weapon.c:2165-2308`), whether it
//! sees and can shoot its enemy, its taunts, its move and its shot. The code the reference
//! compiles out (`#if 0`, `if (0)`: the gloat, the smack, the shield's zap) is left out.
//!
//! The mech's `renderInfo.muzzlePoint` and `muzzleDir` are never written in multiplayer:
//! its laser traces from the world's origin, nowhere.

use crate::npc_galak::GmAttack;
use crate::npc_senses::{Body, distance_squared};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
use sjk_protocol::UserCommand;

/// `MELEE_DIST_SQUARED`, `MIN_LOB_DIST_SQUARED`, `MAX_LOB_DIST_SQUARED`,
/// `REPEATER_ALT_SIZE`, `GENERATOR_HEALTH`.
const MELEE_DIST_SQUARED: f32 = 6_400.0;
const MIN_LOB_DIST_SQUARED: f32 = 65_536.0;
const MAX_LOB_DIST_SQUARED: f32 = 200_704.0;
const REPEATER_ALT_SIZE: f32 = 3.0;
const GENERATOR_HEALTH: i32 = 25;
/// `WP_NONE`, `WP_REPEATER`, `WP_SABER`, `WP_TURRET`.
const WP_NONE: i32 = 0;
pub(crate) const WP_REPEATER: i32 = 8;
const WP_SABER: i32 = 3;
const WP_TURRET: i32 = 18;
/// `BOTH_ATTACK1`, `BOTH_ATTACK2`.
const BOTH_ATTACK1: u16 = 113;
const BOTH_ATTACK2: u16 = 114;
/// `SCF_ALT_FIRE`, `SCF_CHASE_ENEMIES`, `SCF_DONT_FIRE`, `SCF_FIRE_WEAPON`.
const SCF_ALT_FIRE: u32 = 0x40;
const SCF_CHASE_ENEMIES: u32 = 0x400;
const SCF_DONT_FIRE: u32 = 0x4_000;
const SCF_FIRE_WEAPON: u32 = 0x4_0000;
/// `EV_ANGER1`, `EV_CHASE1`, `EV_COVER1`, `EV_ESCAPING1`.
const EV_ANGER1: i32 = 116;
const EV_CHASE1: i32 = 133;
const EV_COVER1: i32 = 136;
const EV_ESCAPING1: i32 = 149;
/// `MASK_SHOT | CONTENTS_LIGHTSABER`; `MASK_SHOT`.
const LOB_CLIP: u32 = crate::npc_aim::MASK_SHOT | 0x4_0000;
const MASK_SHOT: u32 = crate::npc_aim::MASK_SHOT;
/// `ENTITYNUM_WORLD`, `ENTITYNUM_NONE`.
const ENTITY_WORLD: u16 = 1_022;
const ENTITY_NONE: u16 = 1_023;
/// `MOD_UNKNOWN`; `CHAN_AUTO`.
const MOD_UNKNOWN: u32 = 0;
const CHAN_AUTO: u32 = 0;
/// `s.loopSound`; `GT_TEAM`.
const ES_LOOP_SOUND: usize = 55;
const GT_TEAM: i32 = 6;
/// `Q3_INFINITE`, the lob's best miss until one is found.
const Q3_INFINITE: f32 = 16_777_216.0;

/// Whether `enemy` is a placed turret (`WP_TURRET`, `classname` `PAS`), which the mech
/// crushes and lobs at from anywhere: an NPC's enemies here are clients, and no client is.
fn pas_enemy(_enemy: &Body) -> bool {
    false
}

/// `WP_LobFire`'s question: a lob from `start` to `target` for a box of `mins`, `maxs`.
struct Lob {
    start: [f32; 3],
    target: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    ignore: u16,
    enemy: u16,
    ideal_speed: f32,
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSGM_Attack` (`NPC_AI_GalakMech.c:614-1249`).
    pub(crate) fn bs_gm_attack(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        if self.actors[me].mind.fight.pain_debounce_time > level_time {
            // "Don't do anything if we're hurt"
            self.update_angles(me, true, true, command);
            return;
        }
        if !self.check_enemy_ext(me) || self.actors[me].mind.enemy.is_none() {
            self.actors[me].mind.enemy = None;
            self.gm_patrol(me, command);
            return;
        }
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        else {
            return;
        };
        let mut state = GmAttack {
            moving: true,
            enemy_distance: distance_squared(self.actors[me].current_origin, enemy.origin),
            ..GmAttack::default()
        };
        if self.actors[me].mind.creature.walker.lock_count != 0 {
            state.shoot = false;
            self.gm_laser(me);
        } else {
            self.gm_choose_attack(me, &enemy, &mut state);
        }
        self.gm_look(me, &enemy, &mut state);
        if state.enemy_los {
            state.face_enemy = true;
        } else {
            self.gm_chase(me, &mut state);
        }
        if state.enemy_cs {
            state.shoot = true;
        } else {
            self.gm_chase(me, &mut state);
        }
        self.gm_check_move_state(me, state.enemy_los, state.enemy_distance, command);
        self.gm_check_fire_state(me, &mut state);
        let npc = &self.actors[me];
        let lobbing =
            i32::from(npc.player.weapon()) == WP_REPEATER && npc.script_flags & SCF_ALT_FIRE != 0;
        if lobbing && state.shoot && npc.mind.timers.done("attackDelay", level_time) {
            self.gm_aim_lob(me, &enemy, &mut state);
        } else if state.face_enemy {
            self.face_enemy(me, true, command);
        }
        self.gm_move_and_shoot(me, &enemy, &mut state, command);
    }

    /// Its goal the enemy when it has none, and — chasing its enemy — on the move
    /// (`1017-1025`, `1033-1041`).
    fn gm_chase(&mut self, me: usize, state: &mut GmAttack) {
        let npc = &mut self.actors[me];
        if npc.mind.goal.is_none() {
            npc.mind.goal = npc.mind.enemy;
        }
        if npc.mind.goal == npc.mind.enemy {
            state.moving = true;
        }
    }

    /// The laser once started (`745-830`): charging until its beam's time, then the beam —
    /// the attack animation, its effect and hum, the entity the hum moves with — and while
    /// the animation lasts, the trace burning what it meets; then done.
    fn gm_laser(&mut self, me: usize) {
        let level_time = self.level_time;
        let muzzle = self.actors[me].mind.muzzle_point;
        // `renderInfo.muzzleDir`: never written in multiplayer.
        let muzzle_dir = [0.0; 3];
        if self.actors[me].mind.creature.walker.lock_count == 1 {
            if !self.actors[me].mind.timers.done("beamDelay", level_time) {
                return;
            }
            self.set_animation(
                me,
                SETANIM_BOTH,
                BOTH_ATTACK2,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            let torso = self.actors[me].player.torso_timer();
            let delay = torso + self.host.irand(1_000, 3_000);
            self.actors[me]
                .mind
                .timers
                .set("attackDelay", level_time, delay);
            self.actors[me].mind.creature.walker.lock_count = 2;
            let origin = self.actors[me].current_origin;
            self.play_effect_at(b"galak/trace_beam", origin, [0.0; 3]);
            let hum = self
                .host
                .sound_index(b"sound/weapons/galak/lasercutting.wav");
            self.actors[me]
                .state
                .set_raw_field(ES_LOOP_SOUND, u32::from(hum));
            if self.actors[me].mind.creature.walker.cover_target.is_none() {
                let target = self.host.spawn_entity();
                if let Some(target) = target {
                    self.actors[me].mind.creature.walker.cover_target = Some(target);
                    self.gm_place_hum(target, muzzle, hum);
                }
            }
            return;
        }
        if self.actors[me].player.torso_timer() <= 0 {
            // "attack done!"
            let npc = &mut self.actors[me];
            npc.mind.creature.walker.lock_count = 0;
            if let Some(target) = npc.mind.creature.walker.cover_target.take() {
                self.host.free(target);
            }
            let npc = &mut self.actors[me];
            npc.state.set_raw_field(ES_LOOP_SOUND, 0);
            let torso = npc.player.torso_timer();
            npc.mind.timers.set("attackDelay", level_time, torso);
            return;
        }
        let end: [f32; 3] = std::array::from_fn(|axis| muzzle[axis] + 1_024.0 * muzzle_dir[axis]);
        let number = self.actors[me].number;
        let trace = self.trace_bodies(muzzle, [-3.0; 3], [3.0; 3], end, number, MASK_SHOT);
        let hum = self
            .host
            .sound_index(b"sound/weapons/galak/lasercutting.wav");
        let cover = self.actors[me].mind.creature.walker.cover_target;
        if trace.all_solid || trace.start_solid {
            // "oops, in a wall"
            if let Some(target) = cover {
                self.gm_place_hum(target, muzzle, hum);
            }
            return;
        }
        if trace.fraction < 1.0 && self.takes_damage(trace.entity_number) {
            self.sound_at_location(
                trace.end_position,
                CHAN_AUTO,
                b"sound/weapons/galak/laserdamage.wav",
            );
            self.gm_burn(me, trace.entity_number, muzzle_dir, trace.end_position);
        }
        if let Some(target) = cover {
            self.gm_place_hum(target, trace.end_position, hum);
        }
        if self.host.irand(0, 5) == 0 {
            self.sound_at_location(
                trace.end_position,
                CHAN_AUTO,
                b"sound/weapons/galak/laserdamage.wav",
            );
        }
    }

    /// The laser's burn on what it met: `G_Damage(traceEnt, NPC, NPC, muzzleDir, endpos, 10,
    /// 0, MOD_UNKNOWN)`.
    fn gm_burn(&mut self, me: usize, target: u16, direction: [f32; 3], point: [f32; 3]) {
        self.creature_damage(me, target, Some(direction), Some(point), 10, 0, MOD_UNKNOWN);
    }

    /// The hum's entity at `origin` (`G_SetOrigin`), sent to everyone (`SVF_BROADCAST`),
    /// looping the laser's sound.
    fn gm_place_hum(&mut self, target: u16, origin: [f32; 3], hum: u16) {
        let mut state =
            sjk_protocol::EntityState::zero(target, &sjk_protocol::LEGACY_ENTITY_FIELDS);
        for axis in 0..3 {
            state.set_raw_field(crate::npc_spawn::es::POS_BASE[axis], origin[axis].to_bits());
            state.set_raw_field(crate::npc_spawn::es::ORIGIN[axis], origin[axis].to_bits());
        }
        state.set_raw_field(ES_LOOP_SOUND, u32::from(hum));
        self.host.publish(target, &state, ([0.0; 3], [0.0; 3]), 0);
    }

    /// Whether entity `number` takes damage (`takedamage`): a living client or any NPC, or
    /// what the host says has health.
    pub(crate) fn takes_damage(&self, number: u16) -> bool {
        if let Some(at) = self.actor_at(number) {
            return self.actors[at].takes_damage;
        }
        if self
            .host
            .players()
            .iter()
            .any(|player| player.number == number)
        {
            return true;
        }
        number < ENTITY_WORLD && self.host.damageable_health(number).is_some()
    }

    /// Not yet in a special attack (`831-911`): a swing at an enemy right in front, the
    /// laser (its antenna hit, now and then at lob range), or the rapid fire near and the
    /// lob far.
    fn gm_choose_attack(&mut self, me: usize, enemy: &Body, state: &mut GmAttack) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        let view = npc.player.view_angles();
        let origin = npc.current_origin;
        let in_front = crate::saber_block::in_front(enemy.origin, origin, view, 0.3);
        let enemy_humanoid = self
            .actor_at(enemy.number)
            .is_none_or(|at| self.actors[at].humanoid);
        let pas = pas_enemy(enemy);
        if state.enemy_distance < MELEE_DIST_SQUARED && in_front && enemy_humanoid {
            if self.actors[me].mind.timers.done("attackDelay", level_time) {
                self.set_animation(
                    me,
                    SETANIM_BOTH,
                    BOTH_ATTACK1,
                    SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                );
                let torso = self.actors[me].player.torso_timer();
                let delay = torso + self.host.irand(1_000, 3_000);
                let npc = &mut self.actors[me];
                npc.mind.timers.set("attackDelay", level_time, delay);
                npc.mind.timers.set("smackTime", level_time, 600);
                npc.mind.creature.walker.blocked_debounce_time = 0;
            }
            return;
        }
        let npc = &self.actors[me];
        let generator_down = npc.mind.creature.walker.location_damage
            [crate::npc_machine_parts::HL_GENERIC1 as usize]
            > GENERATOR_HEALTH;
        if npc.mind.creature.walker.lock_count == 0
            && generator_down
            && npc.mind.timers.done("attackDelay", level_time)
            && in_front
        {
            let skill = self.host.skill();
            let in_lob_range = self.host.irand(0, 10 * (2 - skill)) == 0
                && state.enemy_distance > MIN_LOB_DIST_SQUARED
                && state.enemy_distance < MAX_LOB_DIST_SQUARED;
            let timers = &self.actors[me].mind.timers;
            let neither = !timers.done("noLob", level_time) && !timers.done("noRapid", level_time);
            if (in_lob_range || neither) && enemy.weapon != WP_TURRET {
                state.shoot = false;
                self.gm_start_laser(me);
                return;
            }
        }
        let npc = &mut self.actors[me];
        let repeater = i32::from(npc.player.weapon()) == WP_REPEATER;
        if state.enemy_distance < MIN_LOB_DIST_SQUARED
            && (enemy.weapon != WP_TURRET || !pas)
            && npc.mind.timers.done("noRapid", level_time)
        {
            if repeater && npc.script_flags & SCF_ALT_FIRE != 0 {
                // "shooting an explosive, but enemy too close, switch to primary fire";
                // `NPC_ChangeWeapon` does nothing in multiplayer.
                npc.script_flags &= !SCF_ALT_FIRE;
                npc.mind.creature.walker.alt_fire = false;
            }
        } else if (state.enemy_distance > MAX_LOB_DIST_SQUARED
            || (enemy.weapon == WP_TURRET && pas))
            && npc.mind.timers.done("noLob", level_time)
            && repeater
            && npc.script_flags & SCF_ALT_FIRE == 0
        {
            npc.script_flags |= SCF_ALT_FIRE;
            npc.mind.creature.walker.alt_fire = true;
        }
    }

    /// Whether it sees its enemy and can shoot it (`913-1009`): seen, its aim bettered and
    /// its shot checked (a lob at an enemy within 256 would hurt itself); unseen but in its
    /// potentially visible set, a word now and then and its shot checked too.
    fn gm_look(&mut self, me: usize, enemy: &Body, state: &mut GmAttack) {
        let level_time = self.level_time;
        if self.clear_los4(me, enemy) {
            self.actors[me].mind.tactics.enemy_last_seen_time = level_time;
            state.enemy_los = true;
            let npc = &self.actors[me];
            let weapon = i32::from(npc.player.weapon());
            if weapon == WP_NONE {
                state.enemy_cs = false;
                self.aim_adjust(me, -1);
            } else if weapon == WP_REPEATER
                && npc.script_flags & SCF_ALT_FIRE != 0
                && state.enemy_distance < MIN_LOB_DIST_SQUARED
            {
                state.enemy_cs = false;
                // "us!"
                state.hit_ally = true;
            } else {
                let (hit, impact) = self.shot_entity(me, enemy);
                state.impact = impact;
                if self.shot_would_do(me, enemy, hit) {
                    state.enemy_cs = true;
                    self.aim_adjust(me, 2);
                    self.actors[me].mind.tactics.enemy_last_seen_location = enemy.origin;
                } else {
                    self.aim_adjust(me, 1);
                    let hit_body = self.body(hit);
                    if hit_body.is_some_and(|body| body.player_team == self.actors[me].player_team)
                    {
                        state.hit_ally = true;
                    }
                }
            }
            return;
        }
        let origin = self.actors[me].current_origin;
        if !self.host.in_pvs(enemy.origin, origin) {
            return;
        }
        if self.actors[me].mind.timers.done("talkDebounce", level_time)
            && self.host.irand(0, 10) == 0
        {
            let said = self.actors[me].mind.tactics.enemy_check_debounce_time;
            if said < 8 {
                let speech = match said {
                    0..=2 => EV_CHASE1 + said,
                    3..=5 => EV_COVER1 + said - 3,
                    _ => EV_ESCAPING1 + said - 6,
                };
                self.actors[me].mind.tactics.enemy_check_debounce_time += 1;
                let debounce = self.host.irand(3_000, 5_000);
                self.add_voice(me, speech, debounce);
                let delay = self.host.irand(5_000, 7_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("talkDebounce", level_time, delay);
            }
        }
        self.actors[me].mind.tactics.enemy_last_seen_time = level_time;
        let (hit, impact) = self.shot_entity(me, enemy);
        state.impact = impact;
        if self.shot_would_do(me, enemy, hit) {
            state.enemy_cs = true;
        } else {
            state.face_enemy = true;
            self.aim_adjust(me, -1);
        }
    }

    /// Whether a shot that would hit entity `hit` is worth it: the enemy itself, someone of
    /// the team it fights, or anything that takes damage (glass, a breakable).
    fn shot_would_do(&self, me: usize, enemy: &Body, hit: u16) -> bool {
        hit == enemy.number
            || self
                .body(hit)
                .is_some_and(|body| body.player_team == self.actors[me].enemy_team)
            || self.takes_damage(hit)
    }

    /// The lob aimed (`1049-1095`): its arc to the enemy, jittered by its aim; none clear
    /// and a straight shot at hand, the rapid fire back; else faced along the arc.
    fn gm_aim_lob(&mut self, me: usize, enemy: &Body, state: &mut GmAttack) {
        let level_time = self.level_time;
        let muzzle = self.weapon_spot(me);
        let mut target = enemy.origin;
        let aim = self.actors[me].mind.current_aim;
        for axis in &mut target {
            let jitter = self.host.rng().flrand(-5.0, 5.0);
            let spread = self.host.rng().flrand(-1.0, 1.0) * (6 - aim) as f32 * 2.0;
            *axis += jitter + spread;
        }
        let number = self.actors[me].number;
        let lob = Lob {
            start: muzzle,
            target,
            mins: [-REPEATER_ALT_SIZE; 3],
            maxs: [REPEATER_ALT_SIZE; 3],
            ignore: number,
            enemy: enemy.number,
            ideal_speed: 1_500.0,
        };
        let (clear, velocity) = self.lob_fire(me, &lob);
        if velocity == [0.0; 3] || (!clear && state.enemy_los && state.enemy_cs) {
            if state.enemy_los
                && state.enemy_cs
                && self.actors[me].mind.timers.done("noRapid", level_time)
            {
                let npc = &mut self.actors[me];
                npc.script_flags &= !SCF_ALT_FIRE;
                npc.mind.creature.walker.alt_fire = false;
                let delay = self.host.irand(500, 1_000);
                self.actors[me].mind.timers.set("noLob", level_time, delay);
            } else {
                state.shoot = false;
            }
            return;
        }
        let angles = crate::player_angle_math::vector_angles(velocity);
        let npc = &mut self.actors[me];
        npc.desired_yaw = crate::npc_droid::angle_normalize360(angles[1]);
        npc.mind.desired_pitch = crate::npc_droid::angle_normalize360(angles[0]);
        let mut direction = velocity;
        let length = crate::saber_clash::normalize(&mut direction);
        npc.mind.creature.walker.hidden_dir = direction;
        npc.mind.creature.walker.hidden_dist = length;
    }

    /// The move and the shot (`1101-1248`): no move while it must stand, nor at an enemy it
    /// does not chase; the move toward its goal; its facing; the shot unless told not to or
    /// its enemy duels a Jedi; a turret crushed; and its taunts as its enemy weakens.
    fn gm_move_and_shoot(
        &mut self,
        me: usize,
        enemy: &Body,
        state: &mut GmAttack,
        command: &mut UserCommand,
    ) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        if !npc.mind.timers.done("standTime", level_time) {
            state.moving = false;
        }
        if npc.script_flags & SCF_CHASE_ENEMIES == 0 && npc.mind.goal == npc.mind.enemy {
            state.moving = false;
        }
        if state.moving && self.actors[me].mind.creature.walker.lock_count == 0 {
            state.moving = if self.actors[me].mind.goal.is_some() {
                self.gm_move(me, command)
            } else {
                false
            };
        }
        if !self.actors[me].mind.timers.done("flee", level_time) {
            state.face_enemy = false;
        }
        if !state.face_enemy {
            let npc = &mut self.actors[me];
            if !state.moving {
                npc.mind.tactics.last_path_angles = npc.player.view_angles();
            } else {
                npc.desired_yaw = npc.mind.tactics.last_path_angles[1];
                npc.mind.desired_pitch = 0.0;
                state.shoot = false;
            }
        }
        self.update_angles(me, true, true, command);
        if self.actors[me].script_flags & SCF_DONT_FIRE != 0 {
            state.shoot = false;
        }
        let enemy_enemy = enemy.enemy.and_then(|other| self.body(other));
        if enemy.weapon == WP_SABER && enemy_enemy.is_some_and(|other| other.weapon == WP_SABER) {
            state.shoot = false;
        }
        if state.shoot
            && self.actors[me].mind.timers.done("attackDelay", level_time)
            && self.actors[me].script_flags & SCF_FIRE_WEAPON == 0
        {
            self.weapon_think(me, command);
        }
        if enemy.weapon == WP_TURRET && pas_enemy(enemy) {
            // "crush turrets": their boxes overlapping.
            let npc = &self.actors[me];
            let (low, high) = npc.link;
            let (enemy_low, enemy_high) = (
                std::array::from_fn::<f32, 3, _>(|axis| enemy.origin[axis] + enemy.mins[axis]),
                std::array::from_fn::<f32, 3, _>(|axis| enemy.origin[axis] + enemy.maxs[axis]),
            );
            if (0..3).all(|axis| low[axis] <= enemy_high[axis] && high[axis] >= enemy_low[axis]) {
                let origin = npc.current_origin;
                self.creature_damage(
                    me,
                    enemy.number,
                    None,
                    Some(origin),
                    100,
                    crate::npc_creature::DAMAGE_NO_KNOCKBACK,
                    crate::means_of_death::MOD_CRUSH,
                );
            }
        }
        // The shield's zap at a touching enemy is compiled out (`if (0)`).
        self.gm_taunt(me, enemy);
    }

    /// Its taunts as its enemy weakens (`1228-1248`): below 100, 75 and 50 health while its
    /// enemy is in pain, each once.
    fn gm_taunt(&mut self, me: usize, enemy: &Body) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        let said = npc.mind.creature.walker.movement_speech;
        if said >= 3
            || npc.mind.blocked_speech_until > level_time
            || enemy.health <= 0
            || self.pain_debounce_of(enemy.number) <= level_time
        {
            return;
        }
        let (event, next) = match said {
            2 if enemy.health < 50 => (EV_ANGER1 + 1, 3),
            1 if enemy.health < 75 => (EV_ANGER1, 2),
            0 if enemy.health < 100 => (EV_ANGER1 + 2, 1),
            _ => return,
        };
        let debounce = self.host.irand(2_000, 4_000);
        self.add_voice(me, event, debounce);
        self.actors[me].mind.creature.walker.movement_speech = next;
    }

    /// `painDebounceTime` of client `number`: an NPC's own, a player's the host's.
    fn pain_debounce_of(&mut self, number: u16) -> i32 {
        match self.actor_at(number) {
            Some(at) => self.actors[at].mind.fight.pain_debounce_time,
            // A player's is never set in multiplayer.
            None => 0,
        }
    }

    /// `WP_LobFire` (`g_weapon.c:2165-2308`, `tracePath` and `mustHit` on, the speeds 300 to
    /// 1100 unused): the velocity that lobs a box from `start` onto `target` under gravity —
    /// the ideal speed first, then others by a hundred at a time — its path traced in half
    /// seconds; whether a clear one was found, and the velocity (the closest miss's when
    /// none was).
    fn lob_fire(&mut self, me: usize, lob: &Lob) -> (bool, [f32; 3]) {
        const SPEED_INC: f32 = 100.0;
        const TIME_STEP: i32 = 500;
        const MAX_HITS: i32 = 7;
        let level_time = self.level_time;
        let gravity = self.host.gravity();
        let ideal = if lob.ideal_speed < SPEED_INC {
            SPEED_INC
        } else {
            lob.ideal_speed
        };
        let skip = ((ideal - SPEED_INC) / SPEED_INC) as i32;
        let mut speed = ideal;
        let mut best = Q3_INFINITE;
        // `mustHit`: the fallback is only a miss the path found.
        let mut fail_case = [0.0; 3];
        let mut shot = [0.0; 3];
        let mut hits = 0;
        while hits < MAX_HITS {
            let mut direction = crate::npc_senses::subtract(lob.target, lob.start);
            let distance = crate::saber_clash::normalize(&mut direction);
            shot = direction.map(|axis| axis * speed);
            let mut travel = distance / speed;
            shot[2] = (f64::from(shot[2]) + f64::from(travel) * 0.5 * f64::from(gravity)) as f32;
            let mut blocked = false;
            travel *= 1_000.0;
            let mut last = lob.start;
            let floor = f64::from(travel).floor();
            let mut elapsed = TIME_STEP;
            while f64::from(elapsed) < floor + f64::from(TIME_STEP) {
                if elapsed as f32 > travel {
                    elapsed = floor as i32;
                }
                let test = crate::trajectory::legacy_evaluate_trajectory(
                    lob.start,
                    shot,
                    6,
                    level_time,
                    0,
                    level_time + elapsed,
                );
                let trace = self.trace_bodies(last, lob.mins, lob.maxs, test, lob.ignore, LOB_CLIP);
                if trace.all_solid || trace.start_solid {
                    blocked = true;
                    break;
                }
                if trace.fraction < 1.0 {
                    if trace.entity_number == lob.enemy
                        || (trace.plane_normal[2] > 0.7
                            && distance_squared(trace.end_position, lob.target) < 4_096.0)
                    {
                        // "hit the enemy, that's perfect!" — or "close enough!"
                        break;
                    }
                    let miss = distance_squared(trace.end_position, lob.target);
                    if miss < best {
                        best = miss;
                        fail_case = shot;
                    }
                    blocked = true;
                    if trace.entity_number < ENTITY_WORLD
                        && trace.entity_number != ENTITY_NONE
                        && self.takes_damage(trace.entity_number)
                        && !self.lob_same_team(me, trace.entity_number)
                    {
                        // "hit something breakable, so that's okay"
                        fail_case = shot;
                    }
                    break;
                }
                if f64::from(elapsed) == floor {
                    // "reached end, all clear"
                    break;
                }
                last = test;
                elapsed += TIME_STEP;
            }
            if !blocked {
                break;
            }
            hits += 1;
            speed = ideal + (hits - skip) as f32 * SPEED_INC;
            if hits >= skip {
                speed += SPEED_INC;
            }
        }
        if hits >= MAX_HITS {
            (false, fail_case)
        } else {
            (true, shot)
        }
    }

    /// `OnSameTeam(self, other)` for the lob's miss: never outside the team games; else the
    /// same session team.
    fn lob_same_team(&self, me: usize, other: u16) -> bool {
        if self.host.gametype() < GT_TEAM {
            return false;
        }
        let team = self.actors[me].session_team;
        self.body(other)
            .is_some_and(|body| body.session_team == team && team != 0)
    }
}
