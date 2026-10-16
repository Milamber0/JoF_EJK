//! A Jedi NPC meeting a blow (`codemp/game/NPC_AI_Jedi.c:2441-3317`): where an enemy's
//! blade will cross it (`Jedi_SaberBlock`, with `ShortestLineSegBewteen2LineSegs`,
//! `g_utils.c:2107-2283`, and `G_FindClosestPointOnLineSegment`, `q_math.c:692-770`), and
//! what it does about a blow or a missile at a point (`Jedi_SaberBlockGo`): a parry in the
//! quadrant the blow comes from, a duck, a jump or Force jump, a dodge, or an acrobatic
//! evasion ([`NpcWorld::jedi_check_flip_evasions`]).
//!
//! `Jedi_SaberBlockGo` is also `WP_SaberStartMissileBlockCheck`'s, for a missile or thrown
//! saber (`incoming`) at an NPC outside its think; there the reference's `self ==
//! NPCS.NPC` is false (`ClearNPCGlobals` runs after every think, `g_main.c:3350`), and
//! within `Jedi_SaberBlock`, where `incoming` is always `NULL`, it is true — so a jump
//! is the command's `upmove` exactly when nothing is incoming.

use crate::npc_jedi_evasion::{
    BLOCKED_LOWER_LEFT, BLOCKED_LOWER_RIGHT, BLOCKED_TOP, BLOCKED_UPPER_LEFT, BLOCKED_UPPER_RIGHT,
    BOTH_DODGE_BL, BOTH_DODGE_BR, BOTH_DODGE_FL, BOTH_DODGE_FR, BOTH_DODGE_L, BOTH_DODGE_R,
    CLASS_BOBAFETT, CLASS_DESANN, CLASS_TAVION, ENTITYNUM_NONE, EvasionType, FP_DRAIN, FP_GRIP,
    FP_SABER_DEFENSE, JUMP_VELOCITY, PS_FORCE_JUMP_Z_START, PS_WEAPON_TIME, RANK_CREWMAN,
    RANK_LT_COMM, RANK_LT_JG, WP_SABER, along, dot, forward_right, in_roll, knocked_down, minus,
};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
use sjk_protocol::UserCommand;

/// `PMF_DUCKED`, `PMF_TIME_KNOCKBACK`.
const PMF_DUCKED: u16 = 1;
const PMF_TIME_KNOCKBACK: u16 = 64;
/// `EV_JUMP`.
const EV_JUMP: u32 = 16;
/// `CONTENTS_BODY`.
const CONTENTS_BODY: u32 = 0x100;
/// `Q3_INFINITE`, as `ShortestLineSegBewteen2LineSegs`' float distance.
const Q3_INFINITE: f32 = 16_777_216.0;
/// `PI_DIV_180`, a float (`q_math.h:85`).
const PI_DIV_180: f32 = 0.017_453_292;

/// Where a blow comes from, as `Jedi_SaberBlockGo` sorts it.
#[derive(Clone, Copy, Debug)]
struct Blow {
    /// `rightdot`: how far right of the eyes (horizontally, unnormalised); `zdiff`: how far
    /// above them.
    right_dot: f32,
    z_diff: f32,
    /// `!incoming && fabs(hitdir[2]) < 0.25f`: a level swing.
    level_swing: bool,
    /// A missile or thrown saber (`incoming`), and whether it is a saber.
    incoming: bool,
    incoming_saber: bool,
    /// `groundEntityNum != ENTITYNUM_NONE`.
    on_ground: bool,
    /// `doDodge`: the NPC may dodge instead of blocking.
    do_dodge: bool,
}

/// What `Jedi_SaberBlockGo` settles on: the evasion, a dodge animation, and the chance
/// against a duck (`duckChance`, 0 for none).
#[derive(Clone, Copy, Debug, Default)]
struct BlockChoice {
    evasion: EvasionType,
    dodge: Option<u16>,
    duck_chance: i32,
}

/// The high blocks on one side (`NPC_AI_Jedi.c:2540-2640`): the strafe a roll starts and
/// the one it stops, the forward and back dodges, and the block.
struct Side {
    strafe: &'static str,
    other_strafe: &'static str,
    forward_dodge: u16,
    back_dodge: u16,
    block: u32,
}

/// From the right; from the left.
const FROM_RIGHT: Side = Side {
    strafe: "strafeLeft",
    other_strafe: "strafeRight",
    forward_dodge: BOTH_DODGE_FL,
    back_dodge: BOTH_DODGE_BL,
    block: BLOCKED_UPPER_RIGHT,
};
const FROM_LEFT: Side = Side {
    strafe: "strafeRight",
    other_strafe: "strafeLeft",
    forward_dodge: BOTH_DODGE_FR,
    back_dodge: BOTH_DODGE_BR,
    block: BLOCKED_UPPER_LEFT,
};

/// `WP_MissileBlockForBlock` (`w_saber.c:9110-9131`): a quadrant's projectile block.
fn missile_block_for_block(block: u32) -> u32 {
    if (BLOCKED_UPPER_RIGHT..=BLOCKED_TOP).contains(&block) {
        block + 5
    } else {
        block
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `TIMER_Start(self, name, Q_irand(500, 1500))`: the draw is made whether or not the
    /// timer starts.
    fn jedi_start_timer(&mut self, me: usize, name: &'static str) {
        let duration = self.host.irand(500, 1_500);
        self.actors[me]
            .mind
            .timers
            .start(name, self.level_time, duration);
    }

    /// `TIMER_Set(self, name, duration)`.
    fn jedi_set_timer(&mut self, me: usize, name: &'static str, duration: i32) {
        self.actors[me]
            .mind
            .timers
            .set(name, self.level_time, duration);
    }

    /// `Jedi_SaberBlockGo` (`NPC_AI_Jedi.c:2454-3084`): the evasion for a blow at `hit_loc`
    /// moving along `hit_dir` — or at the missile `incoming` — that misses by `dist`. The
    /// grip and drain stop, the taunt ends, and the saber defence waits out the parry's
    /// time.
    pub fn jedi_saber_block_go(
        &mut self,
        me: usize,
        command: &mut UserCommand,
        hit_loc: Option<[f32; 3]>,
        hit_dir: Option<[f32; 3]>,
        incoming: Option<u16>,
        dist: f32,
    ) -> EvasionType {
        let (hit_loc, hit_dir, incoming_saber, mut saber_busy) = match incoming {
            None => {
                let busy = if self.actors[me].player.saber_in_flight() {
                    true
                } else if self.jedi_quick_reactions(me) {
                    false
                } else {
                    self.jedi_saber_busy(me)
                };
                (
                    hit_loc.unwrap_or_default(),
                    hit_dir.unwrap_or_default(),
                    false,
                    busy,
                )
            }
            Some(number) => {
                let motion = self.jedi_entity_motion(number);
                (
                    motion.origin,
                    normalized(motion.delta),
                    motion.weapon == WP_SABER,
                    false,
                )
            }
        };
        if self.actors[me].definition.client_class == CLASS_BOBAFETT {
            saber_busy = true;
        }
        let npc = &self.actors[me];
        let eye = npc.mind.eye_point;
        let mut diff = minus(hit_loc, eye);
        diff[2] = 0.0;
        let (_, right) = forward_right([0.0, npc.player.view_angles()[1], 0.0]);
        let on_ground = npc.player.ground_entity_num() != ENTITYNUM_NONE;
        let do_dodge = self.jedi_block_may_dodge(me, command, dist, saber_busy);
        let blow = Blow {
            right_dot: dot(right, diff),
            z_diff: hit_loc[2] - eye[2],
            level_swing: incoming.is_none() && f64::from(hit_dir[2]).abs() < 0.25,
            incoming: incoming.is_some(),
            incoming_saber,
            on_ground,
            do_dodge,
        };
        let mut choice = BlockChoice::default();
        let z_diff = blow.z_diff;
        if z_diff >= -5.0 {
            self.jedi_block_high(me, &blow, saber_busy, &mut choice);
        } else if z_diff > -22.0 {
            self.jedi_block_middle(me, &blow, saber_busy, &mut choice);
        } else if saber_busy || (z_diff < -36.0 && (z_diff < -44.0 || self.host.irand(0, 2) == 0)) {
            self.jedi_block_jump(me, command, &blow, saber_busy, &mut choice);
        } else {
            self.jedi_block_low(me, command, &blow, saber_busy, &mut choice);
        }
        self.jedi_block_finish(me, choice, blow.incoming)
    }

    /// `doDodge` (`NPC_AI_Jedi.c:2503-2528`): a blow that will miss by more than 16 (two in
    /// three, or any while the saber is busy), or the saber out of hand or off, and an
    /// acrobat or fencer and above standing, not ducking, rolling, down, nor — saber in
    /// hand — attacking.
    fn jedi_block_may_dodge(
        &mut self,
        me: usize,
        command: &UserCommand,
        dist: f32,
        saber_busy: bool,
    ) -> bool {
        let npc = &self.actors[me];
        let in_flight = npc.player.saber_in_flight();
        let boba = npc.definition.client_class == CLASS_BOBAFETT;
        let exposed = (dist > 16.0 && (self.host.irand(0, 2) != 0 || saber_busy))
            || in_flight
            || crate::npc_saber::sabers_off(&self.actors[me])
            || boba;
        let npc = &self.actors[me];
        let rank = npc.definition.rank;
        if !exposed || !(rank == RANK_CREWMAN || rank >= RANK_LT_JG) {
            return false;
        }
        let state = &npc.player;
        let (saber_move, torso) = (state.saber_move(), state.torso_animation());
        state.ground_entity_num() != ENTITYNUM_NONE
            && state.movement_flags() & PMF_DUCKED == 0
            && command.up_move >= 0
            && npc.mind.timers.done("duck", self.level_time)
            && !in_roll(npc)
            && !knocked_down(npc)
            && (in_flight
                || boba
                || (!crate::saber_rules::in_attack(saber_move)
                    && !crate::saber_rules::in_start(saber_move)
                    && !crate::saber_rules::spinning(torso)
                    && !crate::saber_rules::special_attack(torso)))
    }

    /// At or above the eyes less 5 (`NPC_AI_Jedi.c:2541-2665`): a parry to the side or
    /// above, perhaps with a duck, or a dodge; a busy saber ducks instead.
    fn jedi_block_high(
        &mut self,
        me: usize,
        blow: &Blow,
        saber_busy: bool,
        choice: &mut BlockChoice,
    ) {
        let (right_dot, z_diff) = (blow.right_dot, blow.z_diff);
        if !(blow.incoming || !saber_busy) {
            if blow.on_ground {
                self.jedi_start_timer(me, "duck");
                choice.evasion = EvasionType::Duck;
            }
            return;
        }
        if right_dot > 12.0 || (right_dot > 3.0 && z_diff < 5.0) || blow.level_swing {
            self.jedi_block_high_side(me, blow, &FROM_RIGHT, choice);
        } else if right_dot < -12.0 || (right_dot < -3.0 && z_diff < 5.0) || blow.level_swing {
            self.jedi_block_high_side(me, blow, &FROM_LEFT, choice);
        } else {
            self.jedi_set_blocked(me, BLOCKED_TOP);
            choice.evasion = EvasionType::Parry;
            if blow.on_ground {
                choice.duck_chance = 4;
            }
        }
    }

    /// A high blow from `side`: Boba Fett rolls a third of the time; a dodger dodges
    /// forward or back; anyone else parries, ducking under a blow above the eyes.
    fn jedi_block_high_side(
        &mut self,
        me: usize,
        blow: &Blow,
        side: &Side,
        choice: &mut BlockChoice,
    ) {
        if blow.do_dodge {
            if self.actors[me].definition.client_class == CLASS_BOBAFETT
                && self.host.irand(0, 2) == 0
            {
                self.jedi_start_timer(me, "duck");
                self.jedi_start_timer(me, side.strafe);
                self.jedi_set_timer(me, side.other_strafe, 0);
                choice.evasion = EvasionType::Duck;
            } else {
                choice.dodge = Some(if self.host.irand(0, 1) != 0 {
                    side.forward_dodge
                } else {
                    side.back_dodge
                });
            }
            return;
        }
        self.jedi_set_blocked(me, side.block);
        choice.evasion = EvasionType::Parry;
        if blow.on_ground {
            if blow.z_diff > 5.0 {
                self.jedi_start_timer(me, "duck");
                choice.evasion = EvasionType::DuckParry;
            } else {
                choice.duck_chance = 6;
            }
        }
    }

    /// Between the eyes less 5 and less 22 (`NPC_AI_Jedi.c:2669-2771`): a duck on the
    /// ground, with a parry to the side or above, or a sideways dodge. Boba Fett's roll
    /// starts a left strafe on either side, as in the reference.
    fn jedi_block_middle(
        &mut self,
        me: usize,
        blow: &Blow,
        saber_busy: bool,
        choice: &mut BlockChoice,
    ) {
        if blow.on_ground {
            self.jedi_start_timer(me, "duck");
            choice.evasion = EvasionType::Duck;
        }
        if !(blow.incoming || !saber_busy) {
            return;
        }
        let (right_dot, z_diff) = (blow.right_dot, blow.z_diff);
        let (dodge, block) = if right_dot > 8.0 || (right_dot > 3.0 && z_diff < -11.0) {
            (Some(BOTH_DODGE_L), BLOCKED_UPPER_RIGHT)
        } else if right_dot < -8.0 || (right_dot < -3.0 && z_diff < -11.0) {
            (Some(BOTH_DODGE_R), BLOCKED_UPPER_LEFT)
        } else {
            (None, BLOCKED_TOP)
        };
        match dodge {
            Some(animation) if blow.do_dodge => {
                if self.actors[me].definition.client_class == CLASS_BOBAFETT
                    && self.host.irand(0, 2) == 0
                {
                    self.jedi_start_timer(me, "strafeLeft");
                    self.jedi_set_timer(me, "strafeRight", 0);
                } else {
                    choice.dodge = Some(animation);
                }
            }
            _ => {
                self.jedi_set_blocked(me, block);
                choice.evasion = if choice.evasion == EvasionType::Duck {
                    EvasionType::DuckParry
                } else {
                    EvasionType::Parry
                };
            }
        }
    }

    /// Low, with the saber busy or well below (`NPC_AI_Jedi.c:2772-2949`): in the air, legs
    /// up with a low block; on the ground a Force jump, a jump (Boba Fett rolls instead),
    /// Tavion's butterfly — then, whatever was chosen, an acrobatic evasion if one is
    /// possible, else a low block.
    fn jedi_block_jump(
        &mut self,
        me: usize,
        command: &mut UserCommand,
        blow: &Blow,
        mut saber_busy: bool,
        choice: &mut BlockChoice,
    ) {
        let right_dot = blow.right_dot;
        if !blow.on_ground {
            self.jedi_start_timer(me, "duck");
            choice.evasion = EvasionType::Duck;
            if blow.incoming || !saber_busy {
                self.jedi_set_blocked(
                    me,
                    if right_dot >= 0.0 {
                        BLOCKED_LOWER_RIGHT
                    } else {
                        BLOCKED_LOWER_LEFT
                    },
                );
                choice.evasion = EvasionType::DuckParry;
            }
            return;
        }
        if self.jedi_block_superjump(me, command) {
            if self.jedi_block_force_jump(me) {
                choice.evasion = EvasionType::FJump;
            }
        } else {
            if self.jedi_may_leap(me) {
                if self.actors[me].definition.client_class == CLASS_BOBAFETT
                    && self.host.irand(0, 1) == 0
                {
                    let (strafe, other) = if right_dot > 0.0 {
                        ("strafeLeft", "strafeRight")
                    } else {
                        ("strafeRight", "strafeLeft")
                    };
                    self.jedi_start_timer(me, strafe);
                    self.jedi_set_timer(me, other, 0);
                    self.jedi_set_timer(me, "walking", 0);
                } else {
                    self.jedi_block_hop(me, command, blow.incoming);
                }
                choice.evasion = EvasionType::Jump;
            }
            if self.actors[me].definition.client_class == CLASS_TAVION
                && !blow.incoming
                && self.host.irand(0, 2) == 0
                && self.jedi_butterfly(me, command)
            {
                choice.evasion = EvasionType::Cartwheel;
                saber_busy = true;
            }
        }
        // `(evasionType = Jedi_CheckFlipEvasions(...)) != EVASION_NONE` (`:2910`): the
        // assignment replaces whatever jump was chosen, even with none.
        choice.evasion = self.jedi_check_flip_evasions(me, right_dot, blow.z_diff);
        if choice.evasion != EvasionType::None {
            return;
        }
        if blow.incoming || !saber_busy {
            self.jedi_set_blocked(
                me,
                if right_dot >= 0.0 {
                    BLOCKED_LOWER_RIGHT
                } else {
                    BLOCKED_LOWER_LEFT
                },
            );
            choice.evasion = match choice.evasion {
                EvasionType::Jump => EvasionType::JumpParry,
                EvasionType::None => EvasionType::Parry,
                other => other,
            };
        }
    }

    /// Tavion's butterfly on the ground (`NPC_AI_Jedi.c:2868-2907`), unless she is
    /// attacking, rolling, down or in a special: whether she did it.
    fn jedi_butterfly(&mut self, me: usize, command: &mut UserCommand) -> bool {
        let npc = &self.actors[me];
        let state = &npc.player;
        let (saber_move, torso) = (state.saber_move(), state.torso_animation());
        let free = state.ground_entity_num() < ENTITYNUM_NONE
            && !crate::saber_rules::in_attack(saber_move)
            && !crate::saber_rules::in_start(saber_move)
            && !in_roll(npc)
            && !knocked_down(npc)
            && !crate::saber_rules::special_attack(torso);
        if !free {
            return false;
        }
        let animation = if self.host.irand(0, 1) != 0 {
            BOTH_BUTTERFLY_LEFT
        } else {
            BOTH_BUTTERFLY_RIGHT
        };
        self.set_animation(
            me,
            SETANIM_BOTH,
            animation,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        );
        let npc = &mut self.actors[me];
        let mut velocity = npc.player.velocity();
        velocity[2] = 225.0;
        npc.player.set_velocity(velocity);
        npc.player
            .set_raw_field(PS_FORCE_JUMP_Z_START, npc.current_origin[2].to_bits());
        if npc.definition.client_class == CLASS_BOBAFETT {
            self.add_event(me, EV_JUMP, 0);
        } else {
            self.jedi_body_sound(me, b"sound/weapons/force/jump.wav");
        }
        command.up_move = 0;
        true
    }

    /// Below the eyes less 22, not jumping (`NPC_AI_Jedi.c:2950-3014`): a low block, and at
    /// a thrown saber a Force jump or a jump as well.
    fn jedi_block_low(
        &mut self,
        me: usize,
        command: &mut UserCommand,
        blow: &Blow,
        saber_busy: bool,
        choice: &mut BlockChoice,
    ) {
        if !(blow.incoming || !saber_busy) {
            return;
        }
        self.jedi_set_blocked(
            me,
            if blow.right_dot >= 0.0 {
                BLOCKED_LOWER_RIGHT
            } else {
                BLOCKED_LOWER_LEFT
            },
        );
        choice.evasion = EvasionType::Parry;
        if !blow.incoming_saber {
            return;
        }
        if self.jedi_block_superjump(me, command) {
            if self.jedi_block_force_jump(me) {
                choice.evasion = EvasionType::FJump;
            }
        } else if self.jedi_may_leap(me) {
            self.jedi_block_hop(me, command, blow.incoming);
            choice.evasion = EvasionType::JumpParry;
        }
    }

    /// Whether an acrobat, or above a lieutenant, would Force jump (`NPC_AI_Jedi.c:2806-2807`):
    /// one time in eleven, or a third of the time while moving.
    fn jedi_block_superjump(&mut self, me: usize, command: &UserCommand) -> bool {
        let rank = self.actors[me].definition.rank;
        (rank == RANK_CREWMAN || rank > RANK_LT_JG)
            && (self.host.irand(0, 10) == 0
                || (self.host.irand(0, 2) == 0
                    && (command.forward_move != 0 || command.right_move != 0)))
    }

    /// The Force jump charged, if it may leap and is not down (`NPC_AI_Jedi.c:2811-2823`).
    fn jedi_block_force_jump(&mut self, me: usize) -> bool {
        if !self.jedi_may_leap(me) || knocked_down(&self.actors[me]) {
            return false;
        }
        self.actors[me].force.jump_charge = 320.0;
        true
    }

    /// A jump: the command's up move from the think, else straight up at `JUMP_VELOCITY`.
    fn jedi_block_hop(&mut self, me: usize, command: &mut UserCommand, incoming: bool) {
        if !incoming {
            command.up_move = 127;
        } else {
            let npc = &mut self.actors[me];
            let mut velocity = npc.player.velocity();
            velocity[2] = JUMP_VELOCITY;
            npc.player.set_velocity(velocity);
        }
    }

    /// The evasion carried out (`NPC_AI_Jedi.c:3016-3083`): no more taunt, grip or drain;
    /// a dodge's animation holding the weapon and the move; or the duck's chance, and a
    /// missile's block; then the saber defence's debounce.
    fn jedi_block_finish(
        &mut self,
        me: usize,
        mut choice: BlockChoice,
        incoming: bool,
    ) -> EvasionType {
        if choice.evasion == EvasionType::None {
            return EvasionType::None;
        }
        let level_time = self.level_time;
        self.jedi_set_timer(me, "taunting", 0);
        self.jedi_set_timer(me, "gripping", -level_time);
        self.force_power_stop(me, FP_GRIP);
        self.jedi_set_timer(me, "draining", -level_time);
        self.force_power_stop(me, FP_DRAIN);
        if let Some(animation) = choice.dodge {
            choice.evasion = EvasionType::Dodge;
            self.set_animation(
                me,
                SETANIM_BOTH,
                animation,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            let state = &mut self.actors[me].player;
            let torso = state.torso_timer();
            state.set_raw_field(PS_WEAPON_TIME, torso as u32);
            state.set_movement_time(torso as i16);
            state.set_movement_flags(state.movement_flags() | PMF_TIME_KNOCKBACK);
        } else {
            if choice.duck_chance != 0 && self.host.irand(0, choice.duck_chance) == 0 {
                self.jedi_start_timer(me, "duck");
                choice.evasion = if choice.evasion == EvasionType::Parry {
                    EvasionType::DuckParry
                } else {
                    EvasionType::Duck
                };
            }
            if incoming {
                let block = u32::from(self.actors[me].player.saber_blocked());
                self.jedi_set_blocked(me, missile_block_for_block(block));
            }
        }
        let parry = self.jedi_recalc_parry_time(me, choice.evasion);
        let debounce = &mut self.actors[me].force.debounce[FP_SABER_DEFENSE];
        if *debounce < level_time + parry {
            *debounce = level_time + parry;
        }
        choice.evasion
    }

    /// `Jedi_SaberBlock(saberNum, bladeNum)` (`NPC_AI_Jedi.c:3088-3317`) for the thinking
    /// NPC: once its parry may be rethought, where its living enemy's blade `blade` of
    /// saber `saber` will cross its axis — the blade's sweep extrapolated 200 units, traced
    /// against bodies, or estimated — and the block or dodge for it, held a while by rank.
    /// `false` when it did nothing.
    pub fn jedi_saber_block(
        &mut self,
        me: usize,
        saber: usize,
        blade: usize,
        command: &mut UserCommand,
    ) -> bool {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        if !npc.mind.timers.done("parryReCalcTime", level_time)
            || npc.force.debounce[FP_SABER_DEFENSE] > level_time
        {
            return false;
        }
        let Some(enemy) = npc.mind.enemy else {
            return false;
        };
        let Some(foe) = self.jedi_enemy_combat(enemy) else {
            return false;
        };
        if foe.health <= 0 {
            return false;
        }
        let edge = self.jedi_enemy_blade(enemy, saber, blade);
        let tip_old = along(edge.muzzle_point_old, edge.length, edge.muzzle_dir_old);
        let tip = along(edge.muzzle_point, edge.length, edge.muzzle_dir);
        let npc = &self.actors[me];
        let (origin, link) = (npc.current_origin, npc.link);
        let top = [origin[0], origin[1], link.1[2]];
        let bottom = [origin[0], origin[1], link.0[2]];
        let mut segments = Segments::new(foe.muzzle_point, tip, bottom, top, false);
        let mut dist = segments.shortest();
        let (saber_point, axis_point) = (segments.close1, segments.close2);
        if dist > npc.maxs[0] * 5.0 {
            self.jedi_set_timer(me, "parryTime", -1);
            return false;
        }
        let point_dist = length(minus(saber_point, foe.muzzle_point));
        let share = if edge.length <= 0.0 {
            0.5
        } else {
            point_dist / edge.length
        };
        let base_dir = minus(foe.muzzle_point, foe.muzzle_point_old).map(|axis| axis * share);
        let dir = along(base_dir, 1.0 - share, minus(tip, tip_old));
        let mut hit_loc = along(saber_point, 200.0, dir);
        let trace = self.trace_bodies(
            saber_point,
            [-4.0; 3],
            [4.0; 3],
            hit_loc,
            enemy,
            CONTENTS_BODY,
        );
        if trace.all_solid || trace.start_solid || trace.fraction >= 1.0 {
            let mut to_me = minus(axis_point, saber_point);
            dist = crate::saber_clash::normalize(&mut to_me);
            if dot(dir, to_me) < 0.2 {
                self.jedi_set_timer(me, "parryTime", -1);
                return false;
            }
            // `ShortestLineSegBewteen2LineSegs(saberPoint, hitloc, bottom, top, saberHitPoint,
            // hitloc)`: the second closest point is written over the first segment's end.
            let mut segments = Segments::new(saber_point, hit_loc, bottom, top, true);
            segments.shortest();
            hit_loc = segments.close2;
        } else {
            hit_loc = trace.end_position;
        }
        let evasion = self.jedi_saber_block_go(me, command, Some(hit_loc), Some(dir), None, dist);
        if evasion != EvasionType::Dodge {
            self.jedi_hold_parry(me, evasion);
        } else {
            let npc = &self.actors[me];
            let mut dodge_time = npc.player.torso_timer();
            if npc.definition.rank > RANK_LT_COMM && npc.definition.client_class != CLASS_DESANN {
                dodge_time -= 200;
            }
            self.jedi_set_timer(me, "parryReCalcTime", dodge_time);
            self.jedi_set_timer(me, "parryTime", dodge_time);
        }
        true
    }

    /// A block made (`NPC_AI_Jedi.c:3273-3305`): the saber on, the parry kept for a random
    /// share of its time, and held — Tavion a half to one and a half times it, fencers and
    /// above for it, others once or twice it.
    fn jedi_hold_parry(&mut self, me: usize, evasion: EvasionType) {
        if !self.actors[me].player.saber_in_flight() {
            self.activate_saber(me);
        }
        let parry = self.jedi_recalc_parry_time(me, evasion);
        let keep = self.host.irand(0, parry);
        self.jedi_set_timer(me, "parryReCalcTime", keep);
        if !self.actors[me]
            .mind
            .timers
            .done("parryTime", self.level_time)
        {
            return;
        }
        let npc = &self.actors[me];
        let hold = if npc.definition.client_class == CLASS_TAVION {
            self.host.irand(parry / 2, (f64::from(parry) * 1.5) as i32)
        } else if npc.definition.rank >= RANK_LT_JG {
            parry
        } else {
            self.host.irand(1, 2) * parry
        };
        self.jedi_set_timer(me, "parryTime", hold);
    }
}

/// `BOTH_BUTTERFLY_LEFT`, `BOTH_BUTTERFLY_RIGHT`.
const BOTH_BUTTERFLY_LEFT: u16 = 1_209;
const BOTH_BUTTERFLY_RIGHT: u16 = 1_210;

/// `VectorLength` (`(float)sqrt` of the float sum: the same as `sqrtf`).
fn length(vector: [f32; 3]) -> f32 {
    dot(vector, vector).sqrt()
}

/// `VectorNormalize2`: the unit vector, or zero for a zero length.
fn normalized(vector: [f32; 3]) -> [f32; 3] {
    let mut out = vector;
    if crate::saber_clash::normalize(&mut out) == 0.0 {
        [0.0; 3]
    } else {
        out
    }
}

/// `DotProductNormalize` (`q_math.c:1384-1392`).
fn dot_normalized(left: [f32; 3], right: [f32; 3]) -> f32 {
    dot(normalized(left), normalized(right))
}

/// `G_FindClosestPointOnLineSegment` (`q_math.c:692-770`): the point of the segment
/// `start`–`end` nearest `from`, by the reference's angle construction.
pub(crate) fn closest_point_on_segment(start: [f32; 3], end: [f32; 3], from: [f32; 3]) -> [f32; 3] {
    let (start_to_from, start_to_end) = (minus(from, start), minus(end, start));
    let cosine = dot_normalized(start_to_from, start_to_end);
    if cosine <= 0.0 {
        return start;
    }
    if cosine == 1.0 {
        return if dot(start_to_from, start_to_from) < dot(start_to_end, start_to_end) {
            from
        } else {
            end
        };
    }
    let (end_to_from, mut end_to_start) = (minus(from, end), minus(start, end));
    let cosine = dot_normalized(end_to_from, end_to_start);
    if cosine <= 0.0 {
        return end;
    }
    if cosine == 1.0 {
        return if dot(end_to_from, end_to_from) < dot(end_to_start, end_to_start) {
            from
        } else {
            end
        };
    }
    let theta = 90.0 * (1.0 - cosine);
    let along_end = (theta * PI_DIV_180).cos() * length(end_to_from);
    crate::saber_clash::normalize(&mut end_to_start);
    along(end, along_end, end_to_start)
}

/// `ShortestLineSegBewteen2LineSegs`' arguments: two segments and the closest point on
/// each. `end1_is_close2` is the reference's aliased call (`close_pnt2` the same array as
/// `end1`): each write of the second closest point then moves the first segment's end.
struct Segments {
    start1: [f32; 3],
    end1: [f32; 3],
    start2: [f32; 3],
    end2: [f32; 3],
    close1: [f32; 3],
    close2: [f32; 3],
    end1_is_close2: bool,
}

impl Segments {
    fn new(
        start1: [f32; 3],
        end1: [f32; 3],
        start2: [f32; 3],
        end2: [f32; 3],
        end1_is_close2: bool,
    ) -> Self {
        Self {
            start1,
            end1,
            start2,
            end2,
            close1: [0.0; 3],
            close2: end1,
            end1_is_close2,
        }
    }

    /// `end1`, as the aliased call reads it.
    fn end1(&self) -> [f32; 3] {
        if self.end1_is_close2 {
            self.close2
        } else {
            self.end1
        }
    }

    /// Both closest points, if `distance` beats `current`.
    fn keep(&mut self, current: &mut f32, distance: f32, one: [f32; 3], two: [f32; 3]) {
        if distance < *current {
            self.close1 = one;
            self.close2 = two;
            *current = distance;
        }
    }

    /// `ShortestLineSegBewteen2LineSegs` (`g_utils.c:2107-2283`): the distance between the
    /// segments, the closest points set — the lines' closest points if both fall within the
    /// segments, else the nearest of the endpoint pairs and the endpoints' projections.
    fn shortest(&mut self) -> f32 {
        let start_dif = minus(self.start2, self.start1);
        let v1 = minus(self.end1(), self.start1);
        let v2 = minus(self.end2, self.start2);
        let (v1v1, v2v2, v1v2) = (dot(v1, v1), dot(v2, v2), dot(v1, v2));
        let denom = (v1v2 * v1v2) - (v1v1 * v2v2);
        let mut current = Q3_INFINITE;
        if denom.abs() > 0.001 {
            let s = -((v2v2 * dot(v1, start_dif)) - (v1v2 * dot(v2, start_dif))) / denom;
            let t = ((v1v1 * dot(v2, start_dif)) - (v1v2 * dot(v1, start_dif))) / denom;
            // A NaN passes every test, as in the reference.
            let done = !(s < 0.0 || s > 1.0 || t < 0.0 || t > 1.0);
            let (s, t) = (clamp_unit(s), clamp_unit(t));
            self.close1 = along(self.start1, s, v1);
            self.close2 = along(self.start2, t, v2);
            current = length(minus(self.close2, self.close1));
            if done {
                return current;
            }
        }
        let (start1, start2, end2) = (self.start1, self.start2, self.end2);
        self.keep(&mut current, length(minus(start2, start1)), start1, start2);
        self.keep(&mut current, length(minus(end2, start1)), start1, end2);
        let end1 = self.end1();
        self.keep(&mut current, length(minus(start2, end1)), end1, start2);
        let end1 = self.end1();
        self.keep(&mut current, length(minus(end2, end1)), end1, end2);
        let point = closest_point_on_segment(start2, end2, start1);
        self.keep(&mut current, length(minus(point, start1)), start1, point);
        let end1 = self.end1();
        let point = closest_point_on_segment(start2, end2, end1);
        self.keep(&mut current, length(minus(point, end1)), end1, point);
        let end1 = self.end1();
        let point = closest_point_on_segment(start1, end1, start2);
        self.keep(&mut current, length(minus(point, start2)), point, start2);
        let end1 = self.end1();
        let point = closest_point_on_segment(start1, end1, end2);
        self.keep(&mut current, length(minus(point, end2)), point, end2);
        current
    }
}

/// `s`, `t` held to the segment (`if (s < 0) s = 0; if (s > 1) s = 1;`).
fn clamp_unit(value: f32) -> f32 {
    if value < 0.0 {
        0.0
    } else if value > 1.0 {
        1.0
    } else {
        value
    }
}
