//! An NPC's think (`codemp/game/NPC.c`): `NPC_Think` every frame (`NPC.c:1757-1905`), the
//! behaviour state every tenth of a second — `NPC_ExecuteBState` (`NPC.c:1517-1700`) with
//! what follows the behaviour: the look target, the weapon's readiness, the torso's rest,
//! the held attack, the script flags, the command saved and the facing kept — and between
//! behaviour thinks the last command again. Every command goes through `ClientThink` as a
//! player's does ([`crate::npc_client_think`]). And the angles an NPC turns to
//! (`NPC_UpdateAngles`, `NPC_utils.c:204-340`).
//!
//! A dead NPC's think is `DeadThink` ([`crate::npc_dead`]); vehicles (step 10) are not
//! spawned; ICARUS (scripts, `NPC_ApplyRoff`) is not run, so no script sets a behaviour, a
//! goal or a delayed behaviour.
//!
//! Held to `tools/game-oracle/npcthink.c` (`game-npcthink.txt`).

use crate::npc_behavior::{Traits, cultist_destroyer, run_behavior};
use crate::npc_spawn::{FRAMETIME, NpcHost, NpcThink};
use crate::npc_world::NpcWorld;
use sjk_protocol::UserCommand;

/// `BUTTON_ATTACK`, `BUTTON_ALT_ATTACK`, `BUTTON_USE`, `BUTTON_WALKING`.
const BUTTON_ATTACK: u16 = 1;
const BUTTON_USE: u16 = 32;
const BUTTON_WALKING: u16 = 16;
const BUTTON_ALT_ATTACK: u16 = 128;
/// The script flags the command reads (`SCF_CROUCHED`, `SCF_WALKING`, `SCF_LEAN_RIGHT`,
/// `SCF_LEAN_LEFT`, `SCF_RUNNING`, `SCF_ALT_FIRE`).
const SCF_CROUCHED: u32 = 0x1;
const SCF_WALKING: u32 = 0x2;
const SCF_LEAN_RIGHT: u32 = 0x8;
const SCF_LEAN_LEFT: u32 = 0x10;
const SCF_RUNNING: u32 = 0x20;
const SCF_ALT_FIRE: u32 = 0x40;
/// `NPCAI_LOST`.
const NPCAI_LOST: u32 = 0x2000;
/// `FL_DONT_SHOOT`.
const FL_DONT_SHOOT: u32 = 0x4_0000;
/// `NPCTEAM_ENEMY`.
const NPCTEAM_ENEMY: i32 = 1;
/// `WEAPON_READY`, `WEAPON_IDLE`.
const WEAPON_READY: u32 = 0;
const WEAPON_IDLE: u32 = 6;
/// `WP_SABER`, `WP_BRYAR_PISTOL`, `WP_EMPLACED_GUN`.
const WP_SABER: u8 = 3;
const WP_BRYAR_PISTOL: u8 = 4;
const WP_EMPLACED_GUN: i32 = 17;
/// `TORSO_WEAPONREADY1`, `TORSO_WEAPONREADY3`, `TORSO_WEAPONIDLE3`.
const TORSO_WEAPONREADY1: u16 = 1_400;
const TORSO_WEAPONREADY3: u16 = 1_402;
const TORSO_WEAPONIDLE3: u16 = 1_406;
/// `RANK_LT_JG`.
const RANK_LT_JG: i32 = 3;
/// `CLASS_VEHICLE`.
const CLASS_VEHICLE: i32 = 53;
/// `MIN_ANGLE_ERROR` (`b_local.h:51`).
const MIN_ANGLE_ERROR: f32 = 0.01;
/// `ps.weaponstate`, `ps.saberLockTime`, `ps.saberLockEnemy`, `ps.eFlags2`,
/// `ps.fd.forcePowersActive`; `s.torsoAnim`.
const PS_WEAPON_STATE: usize = 33;
const PS_SABER_LOCK_TIME: usize = 107;
const PS_SABER_LOCK_ENEMY: usize = 110;
const PS_EFLAGS2: usize = 103;
const ES_TORSO_ANIM: usize = 17;

/// `ANGLE2SHORT`: `(int)(x * 65536 / 360) & 65535`, in floats.
pub(crate) fn angle_to_short(angle: f32) -> i32 {
    ((angle * 65_536.0 / 360.0) as i32) & 65_535
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_Think` for the begun NPC at `me`, whose think is due: whether its body is gone.
    pub fn think(&mut self, me: usize) -> crate::npc_dead::DeadThought {
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        npc.think = NpcThink::Think(level_time + FRAMETIME);
        let old_move_dir = npc.mind.move_dir;
        if npc.definition.entity_class != CLASS_VEHICLE {
            npc.mind.move_dir = [0.0; 3];
        }
        if npc.health <= 0 {
            if self.level.native_sand_creatures && crate::npc_sand_creature::is_sand_creature(npc) {
                let flags = npc.player.raw_field(17).unwrap_or(0) & !crate::npc_senses::EF_NODRAW;
                npc.player.set_raw_field(17, flags);
                npc.player.set_raw_field(75, 0);
                npc.state.set_raw_field(19, flags);
                npc.mind.creature.sand = Default::default();
            }
            let thought = self.dead_think(me);
            let npc = &mut self.actors[me];
            npc.player.set_origin(npc.current_origin);
            return thought;
        }

        // Neither `d_npcfreeze` nor an ICARUS freeze.
        npc.think = NpcThink::Think(level_time + FRAMETIME / 2);
        // A vehicle has no behaviour of its own (`NPC.c:1839-1856`, `1880-1885`).
        if crate::vehicle_think::before_think(npc) {
            return crate::npc_dead::DeadThought::Lies;
        }
        // "droid in a vehicle?" (`NPC.c:1857-1860`): its chatter.
        if npc.vehicle.is_none()
            && npc
                .state
                .raw_field(crate::vehicle_rider::field::ES_VEHICLE)
                .unwrap_or(0)
                != 0
        {
            self.droid_sounds(me);
        }
        let npc = &mut self.actors[me];
        // "NPCs sitting in Vehicles do NOTHING" (`s.m_iVehicleNum`, which a vehicle's entity
        // still shows from its last think after its pilot got off).
        if npc.mind.next_bstate_think <= level_time
            && npc
                .state
                .raw_field(crate::vehicle_rider::field::ES_VEHICLE)
                .unwrap_or(0)
                == 0
        {
            let fast = npc.player.weapon() == WP_SABER
                && self.host.skill() >= 2
                && npc.definition.rank > RANK_LT_JG;
            npc.mind.next_bstate_think = level_time + if fast { FRAMETIME / 2 } else { FRAMETIME };
            if npc.vehicle.is_none() {
                self.execute_bstate(me);
            }
        } else {
            npc.mind.move_dir = old_move_dir;
            npc.mind.last_command.server_time = level_time - 50;
            // `NPC_UpdateAngles` sets the angles the last command then replaces: only its
            // locked angles stay.
            let mut discarded = UserCommand::default();
            self.update_angles(me, true, true, &mut discarded);
            let command = self.actors[me].mind.last_command;
            self.client_think(me, command);
        }
        let npc = &mut self.actors[me];
        npc.player.set_origin(npc.current_origin);
        crate::npc_dead::DeadThought::Lies
    }

    /// `NPC_ExecuteBState` (`NPC.c:1517-1700`).
    fn execute_bstate(&mut self, me: usize) {
        let level_time = self.level_time;
        let mut command = UserCommand::default();
        self.handle_ai_flags(me);
        let npc = &mut self.actors[me];
        npc.mind.combat_move = false;
        let state = if npc.mind.temp_behavior != 0 {
            npc.mind.temp_behavior
        } else {
            if npc.behavior_state == 0 {
                npc.behavior_state = npc.default_behavior;
            }
            npc.behavior_state
        };
        let weapon = npc.player.weapon();
        let traits = Traits {
            class: npc.definition.client_class,
            weapon,
            team: npc.player_team,
            flags2: npc.player.raw_field(PS_EFLAGS2).unwrap_or(0),
            script_flags: npc.script_flags,
            charmed: npc.mind.charmed_time > level_time,
            cultist_destroyer: cultist_destroyer(
                npc.definition.client_class,
                weapon,
                &npc.npc_type,
            ),
            has_enemy: npc.mind.enemy.is_some(),
        };
        let behavior = run_behavior(traits, state);
        self.run(me, behavior, &mut command);
        self.after_behavior(me, &mut command);
        command.server_time = level_time - 50;
        let npc = &mut self.actors[me];
        npc.mind.last_command = command;
        if npc.mind.attack_hold_time == 0 {
            npc.mind.last_command.buttons &= !(BUTTON_ATTACK | BUTTON_ALT_ATTACK);
        }
        // `NPC_CheckAttackScript`: no attack script. `NPC_KeepCurrentFacing`.
        let view = npc.player.view_angles();
        let delta = npc.player.delta_angles();
        for axis in [1, 0] {
            if command.angles[axis] == 0 {
                command.angles[axis] = angle_to_short(view[axis]).wrapping_sub(delta[axis]);
            }
        }
        self.client_think(me, command);
    }

    /// `NPC_HandleAIFlags` (`NPC.c:675-770`): a lost navigation dropped (and the enemy
    /// with it when it was the goal), a victory voiced, a friendly-fire count faded.
    fn handle_ai_flags(&mut self, me: usize) {
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        if npc.ai_flags & NPCAI_LOST != 0 {
            npc.ai_flags &= !NPCAI_LOST;
            if npc.mind.goal.is_some() && npc.mind.goal == npc.mind.enemy {
                self.lost_enemy_decide_chase(me);
            }
        }
        let npc = &mut self.actors[me];
        if npc.mind.greeting_debounce_time != 0 && npc.mind.greeting_debounce_time < level_time {
            // `G_AddVoiceEvent(NPC, Q_irand(EV_ANGER1 + 3, ...), Q_irand(2000, 4000))`: the
            // arguments evaluated right to left, as GCC builds the reference.
            let debounce = self.host.irand(2_000, 4_000);
            let event = self.host.irand(
                crate::npc_enemy::EV_ANGER1 + 3,
                crate::npc_enemy::EV_ANGER1 + 5,
            );
            self.add_voice(me, event, debounce);
            self.actors[me].mind.greeting_debounce_time = 0;
        }
        let npc = &mut self.actors[me];
        if npc.mind.ffire_count > 0 && npc.mind.ffire_fade_debounce < level_time {
            npc.mind.ffire_count -= 1;
            npc.mind.ffire_fade_debounce = level_time + 3_000;
        }
    }

    /// What `NPC_ExecuteBState` does after the behaviour (`NPC.c:1580-1662`): an enemy
    /// gone dropped, the look target kept, the attack held back from someone not to be
    /// shot, the weapon raised in a fight and lowered out of one, the torso's rest, the held
    /// attack and the script flags.
    fn after_behavior(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        if let Some(enemy) = self.actors[me].mind.enemy
            && !self.host.in_use(enemy)
            && self.actor_at(enemy).is_none()
        {
            self.clear_enemy(me);
        }
        let lock_time = self.actors[me]
            .player
            .raw_field(PS_SABER_LOCK_TIME)
            .unwrap_or(0);
        let lock_enemy = self.actors[me]
            .player
            .raw_field(PS_SABER_LOCK_ENEMY)
            .unwrap_or(0) as u16;
        if lock_time != 0 && lock_enemy != crate::npc_spawn::ENTITYNUM_NONE {
            crate::npc_enemy::set_look(&mut self.actors[me], lock_enemy, level_time + 1_000);
        } else if !self.check_look_target(me)
            && let Some(enemy) = self.actors[me].mind.enemy
        {
            crate::npc_enemy::set_look(&mut self.actors[me], enemy, 0);
        }
        let enemy = self.actors[me].mind.enemy.map(|number| self.body(number));
        let npc = &mut self.actors[me];
        let weapon_state = npc.player.raw_field(PS_WEAPON_STATE).unwrap_or(0);
        if let Some(body) = enemy {
            // An enemy with no client (the reference allows one) has no flags to read here.
            if let Some(enemy) = body
                && (enemy.flags & FL_DONT_SHOOT != 0
                    || (npc.player_team != NPCTEAM_ENEMY && enemy.npc && enemy.surrendering))
            {
                command.buttons &= !(BUTTON_ATTACK | BUTTON_ALT_ATTACK);
            }
            if weapon_state == WEAPON_IDLE {
                npc.player.set_raw_field(PS_WEAPON_STATE, WEAPON_READY);
            }
        } else if weapon_state == WEAPON_READY {
            npc.player.set_raw_field(PS_WEAPON_STATE, WEAPON_IDLE);
        }
        let torso = npc.state.raw_field(ES_TORSO_ANIM).unwrap_or(0) as u16;
        if command.buttons & BUTTON_ATTACK == 0 && npc.mind.attack_debounce_time > level_time {
            // Just shot: the gun held up a while.
            match npc.player.weapon() {
                WP_SABER => {
                    self.set_animation(me, crate::pmove_anim::SETANIM_TORSO, TORSO_WEAPONREADY1, 0)
                }
                WP_BRYAR_PISTOL => {
                    self.set_animation(me, crate::pmove_anim::SETANIM_TORSO, TORSO_WEAPONREADY3, 0)
                }
                _ => {}
            }
        } else if self.actors[me].mind.enemy.is_none()
            && (torso == TORSO_WEAPONREADY1 || torso == TORSO_WEAPONREADY3)
        {
            // Ready for nothing: the weapon rests on the shoulder.
            self.set_animation(me, crate::pmove_anim::SETANIM_TORSO, TORSO_WEAPONIDLE3, 0);
        }
        self.check_attack_hold(me, command);
        self.apply_script_flags(me, command);
    }

    /// `NPC_CheckAttackHold` (`NPC.c:788-851`): an attack held on while its time runs,
    /// within the weapon's reach.
    fn check_attack_hold(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|number| self.body(number))
        else {
            self.actors[me].mind.attack_hold_time = 0;
            return;
        };
        let reach = self.max_distance_squared(me);
        let npc = &mut self.actors[me];
        let distance = crate::npc_senses::distance_squared(enemy.origin, npc.current_origin);
        if distance > reach {
            npc.mind.attack_hold_time = 0;
        } else if npc.mind.attack_hold_time != 0 && npc.mind.attack_hold_time > level_time {
            command.buttons |= BUTTON_ATTACK;
        } else if npc.mind.attack_hold != 0 && command.buttons & BUTTON_ATTACK != 0 {
            npc.mind.attack_hold_time = level_time + npc.mind.attack_hold;
        } else {
            npc.mind.attack_hold_time = 0;
        }
    }

    /// `NPC_ApplyScriptFlags` (`NPC.c:620-672`): crouched, walking, running, leaning and
    /// the alternate fire, as the scripts set them.
    fn apply_script_flags(&mut self, me: usize, command: &mut UserCommand) {
        let npc = &self.actors[me];
        let flags = npc.script_flags;
        let charmed_moving = npc.mind.charmed_time > self.level_time
            && (command.forward_move != 0 || command.right_move != 0);
        if flags & SCF_CROUCHED != 0 && !charmed_moving {
            command.up_move = -127;
        }
        if flags & SCF_RUNNING != 0 {
            command.buttons &= !BUTTON_WALKING;
        } else if flags & SCF_WALKING != 0 && !charmed_moving {
            command.buttons |= BUTTON_WALKING;
        }
        if flags & (SCF_LEAN_RIGHT | SCF_LEAN_LEFT) != 0 {
            command.buttons |= BUTTON_USE;
            command.right_move = if flags & SCF_LEAN_RIGHT != 0 {
                127
            } else {
                -127
            };
            command.forward_move = 0;
            command.up_move = 0;
        }
        if flags & SCF_ALT_FIRE != 0 && command.buttons & BUTTON_ATTACK != 0 {
            command.buttons |= BUTTON_ALT_ATTACK;
        }
    }

    /// `NPC_UpdateAngles` (`NPC_utils.c:204-340`): the command's angles turned from the
    /// view toward the desired angles, by at most `(60 + yawSpeed * 3) / 20` degrees a
    /// think. Returns whether the NPC faces them exactly.
    pub fn update_angles(
        &mut self,
        me: usize,
        pitch: bool,
        yaw: bool,
        command: &mut UserCommand,
    ) -> bool {
        let level_time = self.level_time;
        let weapon = self.npc(me).weapon;
        let npc = &mut self.actors[me];
        let mind = &mut npc.mind;
        let (mut target_pitch, mut target_yaw) = (0.0, 0.0);
        if mind.enemy.is_none() && level_time < mind.aim_time {
            if pitch {
                target_pitch = mind.locked_desired_pitch;
            }
            if yaw {
                target_yaw = mind.locked_desired_yaw;
            }
        } else {
            if pitch {
                target_pitch = mind.desired_pitch;
                mind.locked_desired_pitch = mind.desired_pitch;
            }
            if yaw {
                target_yaw = npc.desired_yaw;
                mind.locked_desired_yaw = npc.desired_yaw;
            }
        }
        // A saber carrier's Force speed divides by the time scale, which is 1.
        let speed = if weapon == WP_EMPLACED_GUN {
            20.0
        } else {
            npc.definition.stats.yaw_speed
        };
        let view = npc.player.view_angles();
        let delta = npc.player.delta_angles();
        let mut exact = true;
        let mut turn = |from: f32, to: f32| -> f32 {
            let error = crate::npc_senses::angle_delta(from, to);
            if error.abs() <= MIN_ANGLE_ERROR || error == 0.0 {
                return error;
            }
            exact = false;
            // `60.0 + yawSpeed * 3` in double, kept as a float; times `50.0f / 1000.0f`.
            let decay = ((60.0 + f64::from(speed * 3.0)) as f32) * (50.0_f32 / 1_000.0);
            if error < 0.0 {
                let error = error + decay;
                if error > 0.0 { 0.0 } else { error }
            } else {
                let error = error - decay;
                if error < 0.0 { 0.0 } else { error }
            }
        };
        if yaw {
            let error = turn(view[1], target_yaw);
            command.angles[1] = angle_to_short(target_yaw + error).wrapping_sub(delta[1]);
        }
        if pitch {
            let error = turn(view[0], target_pitch);
            command.angles[0] = angle_to_short(target_pitch + error).wrapping_sub(delta[0]);
        }
        command.angles[2] = angle_to_short(view[2]).wrapping_sub(delta[2]);
        exact
    }
}
