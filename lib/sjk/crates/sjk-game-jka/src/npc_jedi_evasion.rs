//! A Jedi NPC getting out of the way (`codemp/game/NPC_AI_Jedi.c:1940-3624`): the flips,
//! cartwheels and wall moves an evasion may turn into (`Jedi_CheckFlipEvasions`), how long
//! a parry is kept (`Jedi_ReCalcParryTime`), who reacts at once (`Jedi_QuickReactions`)
//! and whose saber is too busy to parry (`Jedi_SaberBusy`), the defence against an enemy
//! who swings, throws his saber or casts lightning (`Jedi_EvasionSaber`), and the Force
//! dodge of an instant-hit shot (`Jedi_DodgeEvasion`, which in MP is `w_force.c:5490-5620`
//! and serves players as well). The block itself is [`crate::npc_jedi_block`].
//!
//! `Jedi_Flee` is commented out in MP (`:3617-3623`); [`NpcWorld::jedi_flee`] keeps its
//! answer. The `d_JediAI` debug prints are left out (the cvar is 0), and
//! `g_saberRealisticCombat` is taken as 0, its default.

use crate::npc_spawn::{NpcActor, NpcHost};
use crate::npc_world::NpcWorld;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_LEGS};
use sjk_protocol::UserCommand;

/// `evasionType_t` (`w_saber.h:68-81`): what a Jedi did about a blow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(i32)]
pub enum EvasionType {
    /// `EVASION_NONE`.
    #[default]
    None = 0,
    Parry,
    DuckParry,
    JumpParry,
    Dodge,
    Jump,
    Duck,
    /// `EVASION_FJUMP`: a Force jump.
    FJump,
    Cartwheel,
    Other,
}

/// `class_t`s the evasions read.
pub(crate) const CLASS_DESANN: i32 = 6;
pub(crate) const CLASS_JEDI: i32 = 18;
pub(crate) const CLASS_TAVION: i32 = 47;
pub(crate) const CLASS_BOBAFETT: i32 = 52;
/// `RANK_*` (`ai.h:52-62`).
pub(crate) const RANK_CIVILIAN: i32 = 0;
pub(crate) const RANK_CREWMAN: i32 = 1;
pub(crate) const RANK_ENSIGN: i32 = 2;
pub(crate) const RANK_LT_JG: i32 = 3;
pub(crate) const RANK_LT: i32 = 4;
pub(crate) const RANK_LT_COMM: i32 = 5;
pub(crate) const RANK_COMMANDER: i32 = 6;
/// `SCF_NO_ACROBATICS`.
pub(crate) const SCF_NO_ACROBATICS: u32 = 0x0080_0000;
/// Force powers (`forcePowers_t`).
pub(crate) const FP_SPEED: usize = 2;
pub(crate) const FP_GRIP: usize = 6;
pub(crate) const FP_LIGHTNING: usize = 7;
pub(crate) const FP_RAGE: usize = 8;
pub(crate) const FP_DRAIN: usize = 13;
pub(crate) const FP_SEE: usize = 14;
pub(crate) const FP_SABER_DEFENSE: usize = 16;
/// `WP_SABER`; `WEAPON_FIRING`.
pub(crate) const WP_SABER: i32 = 3;
pub(crate) const WEAPON_FIRING: u8 = 3;
/// `ENTITYNUM_NONE`, `ENTITYNUM_WORLD`.
pub(crate) const ENTITYNUM_NONE: u16 = 1_023;
pub(crate) const ENTITYNUM_WORLD: u16 = 1_022;
/// `JUMP_VELOCITY`; `forceJumpStrength[FORCE_LEVEL_2]`.
pub(crate) const JUMP_VELOCITY: f32 = 225.0;
const FORCE_JUMP_STRENGTH_2: f32 = 420.0;
/// `EV_JUMP`, `EV_GENERAL_SOUND`; `CHAN_BODY`.
const EV_JUMP: u32 = 16;
const EV_GENERAL_SOUND: u32 = 76;
pub(crate) const CHAN_BODY: u32 = 6;
/// `saberBlocked` values (`saberBlockType_t`).
pub(crate) const BLOCKED_NONE: u32 = 0;
pub(crate) const BLOCKED_PARRY_BROKEN: u32 = 2;
pub(crate) const BLOCKED_ATK_BOUNCE: u32 = 3;
pub(crate) const BLOCKED_UPPER_RIGHT: u32 = 4;
pub(crate) const BLOCKED_UPPER_LEFT: u32 = 5;
pub(crate) const BLOCKED_LOWER_RIGHT: u32 = 6;
pub(crate) const BLOCKED_LOWER_LEFT: u32 = 7;
pub(crate) const BLOCKED_TOP: u32 = 8;
/// `SEF_LOCK_WON`, `SES_RETURNING`.
const SEF_LOCK_WON: u32 = 0x100;
const SES_RETURNING: i32 = 1;
/// `CONTENTS_SOLID`, `CONTENTS_MONSTERCLIP`, `CONTENTS_BOTCLIP`.
const CONTENTS_SOLID: u32 = 0x1;
const CONTENTS_MONSTERCLIP: u32 = 0x20;
const CONTENTS_BOTCLIP: u32 = 0x40;
/// `HANDEXTEND_NONE`, `HANDEXTEND_DODGE`; `PW_SPEEDBURST`.
const HANDEXTEND_NONE: u32 = 0;
const HANDEXTEND_DODGE: u32 = 7;
const PW_SPEEDBURST: usize = 8;
/// Player-state wire fields the evasions write: `weaponTime`, `fd.saberAnimLevel`,
/// `fd.forceJumpZStart`, `saberBlocked`, `forceHandExtend`, `forceDodgeAnim`.
pub(crate) const PS_WEAPON_TIME: usize = 10;
const PS_SABER_ANIM_LEVEL: usize = 23;
pub(crate) const PS_FORCE_JUMP_Z_START: usize = 74;
pub(crate) const PS_SABER_BLOCKED: usize = 77;
const PS_FORCE_HAND_EXTEND: usize = 80;
const PS_FORCE_DODGE_ANIM: usize = 89;
/// `SFL_NO_WALL_RUNS`, `SFL_NO_WALL_FLIPS`.
const SFL_NO_WALL_RUNS: u32 = 1 << 13;
const SFL_NO_WALL_FLIPS: u32 = 1 << 14;
/// The animations (`animNumber_t`).
pub(crate) const BOTH_DODGE_FL: u16 = 1_175;
pub(crate) const BOTH_DODGE_FR: u16 = 1_176;
pub(crate) const BOTH_DODGE_BL: u16 = 1_177;
pub(crate) const BOTH_DODGE_BR: u16 = 1_178;
pub(crate) const BOTH_DODGE_L: u16 = 1_179;
pub(crate) const BOTH_DODGE_R: u16 = 1_180;
const BOTH_ARIAL_LEFT: u16 = 1_201;
const BOTH_ARIAL_RIGHT: u16 = 1_202;
const BOTH_CARTWHEEL_LEFT: u16 = 1_203;
const BOTH_CARTWHEEL_RIGHT: u16 = 1_204;
const BOTH_WALL_RUN_RIGHT: u16 = 1_211;
const BOTH_WALL_RUN_RIGHT_FLIP: u16 = 1_212;
const BOTH_WALL_RUN_LEFT: u16 = 1_214;
const BOTH_WALL_RUN_LEFT_FLIP: u16 = 1_215;
const BOTH_WALL_FLIP_RIGHT: u16 = 1_217;
const BOTH_WALL_FLIP_LEFT: u16 = 1_218;

/// `AngleVectors(angles, fwd, right, NULL)`.
pub(crate) fn forward_right(angles: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let (forward, right) = crate::pmove::flight::flight_axes(angles);
    (forward.to_array(), right.to_array())
}

/// `DotProduct`.
pub(crate) fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

/// `VectorMA(start, scale, dir, out)`.
pub(crate) fn along(start: [f32; 3], scale: f32, dir: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| start[axis] + scale * dir[axis])
}

/// `VectorSubtract(left, right, out)`.
pub(crate) fn minus(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] - right[axis])
}

/// `BG_InRoll` (`bg_panimate.c:808-830`).
pub(crate) fn in_roll(npc: &NpcActor) -> bool {
    crate::pmove_roll_anim::in_roll(npc.player.leg_animation()) && npc.player.legs_timer() > 0
}

/// `PM_InKnockDown` on the NPC's player state.
pub(crate) fn knocked_down(npc: &NpcActor) -> bool {
    crate::pmove_hand_extend::in_knockdown(npc.player.leg_animation(), npc.player.legs_timer())
}

/// Raging, or recovering from it: `fd.forceRageRecoveryTime > level.time` or rage active.
fn raging(npc: &NpcActor, level_time: i32) -> bool {
    npc.player.force_rage_recovery_time() > level_time
        || npc.player.force_powers_active() & (1 << FP_RAGE) != 0
}

/// Whether a saber the NPC holds (`saber[n].model[0]`) forbids a move by `flag`, the
/// first saber's say taken before the second's (`NPC_AI_Jedi.c:2062-2083`).
fn sabers_forbid(npc: &NpcActor, flag: u32) -> bool {
    i32::from(npc.player.weapon()) == WP_SABER
        && npc
            .definition
            .sabers
            .iter()
            .any(|saber| !saber.model.is_empty() && saber.flags & flag != 0)
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// The acrobatics' leave (`NPC_AI_Jedi.c:2811-2815`, `2838-2840`): no
    /// `SCF_NO_ACROBATICS`, the rage's recovery over (`<`, not `<=`) and no rage.
    pub(crate) fn jedi_may_leap(&self, me: usize) -> bool {
        let npc = &self.actors[me];
        npc.script_flags & SCF_NO_ACROBATICS == 0
            && npc.player.force_rage_recovery_time() < self.level_time
            && npc.player.force_powers_active() & (1 << FP_RAGE) == 0
    }

    /// `ps.saberBlocked = block`.
    pub(crate) fn jedi_set_blocked(&mut self, me: usize, block: u32) {
        self.actors[me]
            .player
            .set_raw_field(PS_SABER_BLOCKED, block);
    }

    /// `ps.velocity[2] = speed` and, for most, `fd.forceJumpZStart` where it stands (no
    /// fall damage landing at the same height).
    fn jedi_lift(&mut self, me: usize, speed: f32, z_start: bool) {
        let npc = &mut self.actors[me];
        let mut velocity = npc.player.velocity();
        velocity[2] = speed;
        npc.player.set_velocity(velocity);
        if z_start {
            npc.player
                .set_raw_field(PS_FORCE_JUMP_Z_START, npc.current_origin[2].to_bits());
        }
    }

    /// `NPC_SetAnim(self, parts, anim, SETANIM_FLAG_OVERRIDE|SETANIM_FLAG_HOLD)` on the legs
    /// alone while the weapon is busy, else on both.
    fn jedi_acrobatic_animation(&mut self, me: usize, animation: u16) {
        let parts = if self.actors[me].player.weapon_time() == 0 {
            SETANIM_BOTH
        } else {
            SETANIM_LEGS
        };
        self.set_animation(
            me,
            parts,
            animation,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        );
    }

    /// A leap's sound: Boba Fett's jets (`EV_JUMP`), anyone else's Force jump
    /// (`G_SoundOnEnt(self, CHAN_BODY, "sound/weapons/force/jump.wav")`).
    fn jedi_leap_sound(&mut self, me: usize) {
        if self.actors[me].definition.client_class == CLASS_BOBAFETT {
            self.add_event(me, EV_JUMP, 0);
        } else {
            let sound = self.host.sound_index(b"sound/weapons/force/jump.wav");
            let npc = &self.actors[me];
            let mut event =
                crate::knockdown::entity_sound(npc.current_origin, npc.number, CHAN_BODY);
            event.parameter = u32::from(sound);
            self.host.raise(event);
        }
    }

    /// `G_Sound(self, CHAN_BODY, G_SoundIndex(name))` (`g_utils.c:1349-1378`): a sound
    /// event where it stands, the channel in `saberEntityNum` (below the tracked channels).
    pub(crate) fn jedi_body_sound(&mut self, me: usize, name: &[u8]) {
        const ES_SABER_ENTITY: usize = 37;
        let sound = self.host.sound_index(name);
        let mut event = crate::event_entity::EventEntity {
            event: EV_GENERAL_SOUND,
            parameter: u32::from(sound),
            origin: self.actors[me].current_origin,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        event.extra[0] = (ES_SABER_ENTITY, CHAN_BODY);
        self.host.raise(event);
    }

    /// `Jedi_DodgeEvasion` (`w_force.c:5490-5620`): with `g_forceDodge` 1, a Jedi with
    /// Force sight on at level 3 dodges a shot on the ground and with its hands free; with
    /// 2, a Jedi using no power dodges by Force speed, more likely at a higher level. The
    /// dodge is a hand pose (`HANDEXTEND_DODGE`) held 300 ms with a speed burst. `shooter`
    /// and `trace` are unused, as in the reference.
    pub fn jedi_dodge_evasion(
        &mut self,
        me: usize,
        _shooter: Option<u16>,
        _trace: Option<&crate::pmove::MovementTrace>,
        hit_loc: i32,
    ) -> bool {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        if npc.health <= 0 {
            return false;
        }
        let dodge = self.jedi_force_dodge();
        let active = npc.player.force_powers_active();
        if dodge == 0 || (dodge != 2 && active & (1 << FP_SEE) == 0) {
            return false;
        }
        if npc.player.ground_entity_num() == ENTITYNUM_NONE
            || npc.player.weapon_time() > 0
            || u32::from(npc.player.force_hand_extend()) != HANDEXTEND_NONE
        {
            return false;
        }
        if dodge == 2 && (active != 0 || !self.force_power_usable(me, FP_SPEED)) {
            return false;
        }
        let levels = self.actors[me].force_levels;
        if dodge == 2 {
            if self.host.irand(1, 7) > levels[FP_SPEED] {
                return false;
            }
        } else if levels[FP_SEE] < 3 {
            return false;
        }
        let Some(animation) = dodge_animation(hit_loc) else {
            return false;
        };
        let npc = &mut self.actors[me];
        npc.player
            .set_raw_field(PS_FORCE_HAND_EXTEND, HANDEXTEND_DODGE);
        npc.player
            .set_raw_field(PS_FORCE_DODGE_ANIM, u32::from(animation));
        npc.mind.knockdown.hand_extend_time = level_time + 300;
        npc.player.powerups[PW_SPEEDBURST] = (level_time + 100) as u32;
        if dodge == 2 {
            self.force_speed(me, 500);
        } else {
            self.jedi_body_sound(me, b"sound/weapons/force/speed.wav");
        }
        true
    }

    /// `Jedi_CheckFlipEvasions` (`NPC_AI_Jedi.c:1952-2276`): off a wall it runs on, a flip
    /// away from a blow on that side; else, for Jedi of crewman or lieutenant rank and above
    /// but Desann, half the time a cartwheel away from the blow if there is room, or off a
    /// close wall a flip the other way, or a run along it. `z_diff` is unused, as in the
    /// reference.
    pub fn jedi_check_flip_evasions(
        &mut self,
        me: usize,
        right_dot: f32,
        _z_diff: f32,
    ) -> EvasionType {
        let npc = &self.actors[me];
        if npc.script_flags & SCF_NO_ACROBATICS != 0 || raging(npc, self.level_time) {
            return EvasionType::None;
        }
        let legs = npc.player.leg_animation();
        if legs == BOTH_WALL_RUN_LEFT || legs == BOTH_WALL_RUN_RIGHT {
            return self.jedi_flip_off_wall_run(me, legs, right_dot);
        }
        let rank = npc.definition.rank;
        if npc.definition.client_class != CLASS_DESANN
            && (rank == RANK_CREWMAN || rank >= RANK_LT)
            && self.host.irand(0, 1) != 0
            && !in_roll(&self.actors[me])
            && !knocked_down(&self.actors[me])
            && !crate::saber_rules::special_attack(self.actors[me].player.torso_animation())
        {
            return self.jedi_sideways_evasion(me, right_dot);
        }
        EvasionType::None
    }

    /// Running on a wall on the blow's side, away from it — not in the run's first or last
    /// 400 ms (`NPC_AI_Jedi.c:1972-2025`).
    fn jedi_flip_off_wall_run(&mut self, me: usize, legs: u16, right_dot: f32) -> EvasionType {
        let npc = &self.actors[me];
        let (_, right) = forward_right([0.0, npc.player.view_angles()[1], 0.0]);
        let length = npc
            .movement
            .animation_lengths()
            .and_then(|lengths| lengths.length_ms(legs))
            .unwrap_or(0) as f32;
        let timer = npc.player.legs_timer();
        let mid_run = length - timer as f32 > 400.0 && timer > 400;
        let (animation, push) = if legs == BOTH_WALL_RUN_LEFT && right_dot < 0.0 {
            (BOTH_WALL_RUN_LEFT_FLIP, 150.0)
        } else if legs == BOTH_WALL_RUN_RIGHT && right_dot > 0.0 {
            (BOTH_WALL_RUN_RIGHT_FLIP, -150.0)
        } else {
            return EvasionType::None;
        };
        if !mid_run {
            return EvasionType::None;
        }
        let npc = &mut self.actors[me];
        let mut velocity = npc.player.velocity();
        velocity[0] *= 0.5;
        velocity[1] *= 0.5;
        npc.player.set_velocity(along(velocity, push, right));
        self.jedi_acrobatic_animation(me, animation);
        self.add_event(me, EV_JUMP, 0);
        EvasionType::Other
    }

    /// A cartwheel or an arial away from the blow, 128 units clear to that side; off a wall
    /// there (not a do-not-enter brush) that faces it, or a body, the wall moves
    /// (`NPC_AI_Jedi.c:2027-2272`).
    fn jedi_sideways_evasion(&mut self, me: usize, right_dot: f32) -> EvasionType {
        let npc = &self.actors[me];
        let allow_cartwheels = !sabers_forbid(npc, crate::saber_info::SFL_NO_CARTWHEELS);
        let saber_move = npc.player.saber_move();
        let parts = if crate::saber_rules::in_attack(saber_move)
            || crate::saber_rules::in_start(saber_move)
        {
            SETANIM_LEGS
        } else {
            SETANIM_BOTH
        };
        let (animation, check_dist, speed) = if right_dot >= 0.0 {
            (
                if self.host.irand(0, 1) != 0 {
                    BOTH_ARIAL_LEFT
                } else {
                    BOTH_CARTWHEEL_LEFT
                },
                -128.0,
                -200.0,
            )
        } else {
            (
                if self.host.irand(0, 1) != 0 {
                    BOTH_ARIAL_RIGHT
                } else {
                    BOTH_CARTWHEEL_RIGHT
                },
                128.0,
                200.0,
            )
        };
        let trace = self.jedi_side_trace(me, check_dist);
        if trace.fraction >= 1.0 && allow_cartwheels {
            self.jedi_cartwheel(me, parts, animation, speed);
            return EvasionType::Cartwheel;
        }
        if self.jedi_hit_botclip(me, check_dist, &trace) {
            return EvasionType::None;
        }
        let npc = &self.actors[me];
        let (_, right) = forward_right([0.0, npc.player.view_angles()[1], 0.0]);
        let mut ideal_normal = minus(
            npc.current_origin,
            along(npc.current_origin, check_dist, right),
        );
        crate::saber_clash::normalize(&mut ideal_normal);
        // `traceEnt->s.solid != SOLID_BMODEL`: a client's body is no brush model; any other
        // entity is taken for one (movers, doors, breakables).
        let body =
            trace.entity_number < ENTITYNUM_WORLD && self.body(trace.entity_number).is_some();
        if !body && dot(trace.plane_normal, ideal_normal) <= 0.7 {
            return EvasionType::None;
        }
        self.jedi_off_the_wall(me, right_dot, check_dist, trace.fraction)
    }

    /// `trap->Trace` of the NPC's lower box (`mins` at its feet, 24 high) `dist` units to
    /// its right, through solids and both clips.
    fn jedi_side_trace(&mut self, me: usize, dist: f32) -> crate::pmove::MovementTrace {
        self.jedi_side_trace_masked(
            me,
            dist,
            CONTENTS_SOLID | CONTENTS_MONSTERCLIP | CONTENTS_BOTCLIP,
        )
    }

    fn jedi_side_trace_masked(
        &mut self,
        me: usize,
        dist: f32,
        mask: u32,
    ) -> crate::pmove::MovementTrace {
        let npc = &self.actors[me];
        let mins = [npc.mins[0], npc.mins[1], 0.0];
        let maxs = [npc.maxs[0], npc.maxs[1], 24.0];
        let (_, right) = forward_right([0.0, npc.player.view_angles()[1], 0.0]);
        let (origin, number) = (npc.current_origin, npc.number);
        self.trace_bodies(origin, mins, maxs, along(origin, dist, right), number, mask)
    }

    /// `trace.contents & CONTENTS_BOTCLIP` (`NPC_AI_Jedi.c:2131`): the trace here returns no
    /// contents, so the same trace against the bot clip alone says whether one stopped it
    /// first.
    fn jedi_hit_botclip(
        &mut self,
        me: usize,
        dist: f32,
        trace: &crate::pmove::MovementTrace,
    ) -> bool {
        if trace.fraction >= 1.0 {
            return false;
        }
        let clip = self.jedi_side_trace_masked(me, dist, CONTENTS_BOTCLIP);
        clip.fraction < 1.0 && clip.fraction <= trace.fraction
    }

    /// The cartwheel (`NPC_AI_Jedi.c:2104-2129`): no attack until it is over, thrown to the
    /// side at `speed` and 200 up, no Force-flip animation.
    fn jedi_cartwheel(&mut self, me: usize, parts: u8, animation: u16, speed: f32) {
        self.set_animation(
            me,
            parts,
            animation,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        );
        let npc = &mut self.actors[me];
        let legs_timer = npc.player.legs_timer();
        npc.player.set_raw_field(PS_WEAPON_TIME, legs_timer as u32);
        let (_, right) = forward_right([0.0, npc.player.view_angles()[1], 0.0]);
        npc.player.set_velocity(right.map(|axis| axis * speed));
        npc.force.jump_charge = 0.0;
        self.jedi_lift(me, 200.0, true);
        self.jedi_leap_sound(me);
    }

    /// Off a wall at `check_dist` (hit at `fraction`) — or a body — not running forward at
    /// 200 or more: a flip off the near wall the other way if that side is clear, else a
    /// run along whichever wall is within 32 units (`NPC_AI_Jedi.c:2144-2268`).
    fn jedi_off_the_wall(
        &mut self,
        me: usize,
        right_dot: f32,
        mut check_dist: f32,
        fraction: f32,
    ) -> EvasionType {
        let npc = &self.actors[me];
        let (forward, _) = forward_right([0.0, npc.player.view_angles()[1], 0.0]);
        let forward_speed = dot(npc.player.velocity(), forward);
        let mut best = 0.0;
        if forward_speed < 200.0 {
            if fraction * check_dist <= 32.0 {
                best = check_dist;
                check_dist *= -1.0;
                let trace = self.jedi_side_trace(me, check_dist);
                if trace.fraction >= 1.0 {
                    if !sabers_forbid(&self.actors[me], SFL_NO_WALL_FLIPS) {
                        self.jedi_wall_flip(me, right_dot);
                        return EvasionType::Other;
                    }
                } else {
                    if forward_speed < 0.0 {
                        return EvasionType::None;
                    }
                    if trace.fraction * check_dist <= 32.0 && trace.fraction * check_dist < best {
                        best = check_dist;
                    }
                }
            } else {
                check_dist *= -1.0;
                let trace = self.jedi_side_trace(me, check_dist);
                if trace.fraction * check_dist > 32.0 {
                    return EvasionType::None;
                }
                best = check_dist;
            }
        }
        if best != 0.0 && !sabers_forbid(&self.actors[me], SFL_NO_WALL_RUNS) {
            let animation = if best > 0.0 {
                BOTH_WALL_RUN_RIGHT
            } else {
                BOTH_WALL_RUN_LEFT
            };
            self.jedi_lift(me, FORCE_JUMP_STRENGTH_2 / 2.25, false);
            self.jedi_wall_leap(me, animation);
            return EvasionType::Other;
        }
        EvasionType::None
    }

    /// The flip off the wall away from the blow (`NPC_AI_Jedi.c:2162-2194`).
    fn jedi_wall_flip(&mut self, me: usize, right_dot: f32) {
        let npc = &mut self.actors[me];
        let (_, right) = forward_right([0.0, npc.player.view_angles()[1], 0.0]);
        let (animation, push) = if right_dot > 0.0 {
            (BOTH_WALL_FLIP_LEFT, 150.0)
        } else {
            (BOTH_WALL_FLIP_RIGHT, -150.0)
        };
        let velocity = npc.player.velocity();
        npc.player
            .set_velocity(along([0.0, 0.0, velocity[2]], push, right));
        self.jedi_lift(me, FORCE_JUMP_STRENGTH_2 / 2.25, false);
        self.jedi_wall_leap(me, animation);
    }

    /// A wall move's animation, its jump's start height and its sound.
    fn jedi_wall_leap(&mut self, me: usize, animation: u16) {
        self.jedi_acrobatic_animation(me, animation);
        let npc = &mut self.actors[me];
        npc.player
            .set_raw_field(PS_FORCE_JUMP_Z_START, npc.current_origin[2].to_bits());
        self.jedi_leap_sound(me);
    }

    /// `Jedi_ReCalcParryTime` (`NPC_AI_Jedi.c:2278-2410`) for an NPC: how long its choice of
    /// parry stands — at random up to 150 ms on hard (Tavion never waits there, nor on
    /// medium); else a dodge's or cartwheel's animation, a thrown saber's 50–150 ms, or the
    /// skill's time scaled by rank, with extra for ducks, jumps and acrobatics.
    pub fn jedi_recalc_parry_time(&mut self, me: usize, evasion: EvasionType) -> i32 {
        let skill = self.host.skill();
        let npc = &self.actors[me];
        let class = npc.definition.client_class;
        if skill == 2 || (skill == 1 && class == CLASS_TAVION) {
            return if class == CLASS_TAVION {
                0
            } else {
                self.host.irand(0, 150)
            };
        }
        if matches!(evasion, EvasionType::Dodge | EvasionType::Cartwheel) {
            return npc.player.torso_timer();
        }
        if npc.player.saber_in_flight() {
            return self.host.irand(1, 3) * 50;
        }
        let base = match skill {
            0 => 200,
            1 => 100,
            _ => 50,
        };
        let half = |time: i32| (time as f32 / 2.0).ceil() as i32;
        let rank = npc.definition.rank;
        let base = if class == CLASS_TAVION {
            half(base)
        } else if rank >= RANK_LT_JG {
            if self.host.irand(0, 2) == 0 {
                half(base)
            } else {
                base
            }
        } else if rank == RANK_CIVILIAN {
            base * self.host.irand(1, 3)
        } else if rank == RANK_CREWMAN {
            if matches!(
                evasion,
                EvasionType::Parry | EvasionType::DuckParry | EvasionType::JumpParry
            ) {
                base * self.host.irand(1, 2)
            } else {
                base
            }
        } else {
            base * self.host.irand(1, 2)
        };
        base + match evasion {
            EvasionType::Duck
            | EvasionType::DuckParry
            | EvasionType::Other
            | EvasionType::FJump => 100,
            EvasionType::Jump | EvasionType::JumpParry => 50,
            _ => 0,
        }
    }

    /// `Jedi_QuickReactions` (`NPC_AI_Jedi.c:2412-2422`): the Jedi commander, Tavion, and
    /// saber defence above level 1 on hard or above 2 on medium parry whenever they like.
    /// The reference reads the rank of the NPC thinking (`NPCS.NPCInfo`), which its one
    /// caller's `self` is.
    pub fn jedi_quick_reactions(&self, me: usize) -> bool {
        let npc = &self.actors[me];
        let (class, defense, skill) = (
            npc.definition.client_class,
            npc.force_levels[FP_SABER_DEFENSE],
            self.host.skill(),
        );
        (class == CLASS_JEDI && npc.definition.rank == RANK_COMMANDER)
            || class == CLASS_TAVION
            || (defense > 1 && skill > 1)
            || (defense > 2 && skill > 0)
    }

    /// `Jedi_SaberBusy` (`NPC_AI_Jedi.c:2424-2439`): with more than 300 ms of its torso
    /// animation left, a strong attack, a spin, a special, a broken parry, a flip or a roll
    /// keeps the saber from a parrying position.
    pub fn jedi_saber_busy(&self, me: usize) -> bool {
        let state = &self.actors[me].player;
        let (saber_move, torso) = (state.saber_move(), state.torso_animation());
        let strong = state.raw_field(PS_SABER_ANIM_LEVEL).unwrap_or(0) == 3;
        state.torso_timer() > 300
            && ((crate::saber_rules::in_attack(saber_move) && strong)
                || crate::saber_rules::spinning(torso)
                || crate::saber_rules::special_attack(torso)
                || crate::saber_rules::in_broken_parry(saber_move)
                || crate::saber_rules::flipping(torso)
                || crate::npc_pain::rolling(torso))
    }

    /// `Jedi_Flee`: commented out in MP (`NPC_AI_Jedi.c:3617-3623`), where it would answer
    /// no.
    pub fn jedi_flee(&mut self, _me: usize) -> bool {
        false
    }

    /// `Jedi_EvasionSaber` (`NPC_AI_Jedi.c:3325-3611`): against an enemy client not in a
    /// saber lock (nor one the NPC just won), the parry of a swing under way; then, more
    /// likely the more he attacks, if he comes at or faces the NPC, a defence — a Force
    /// push, a parry, or a strafe, jump or flip.
    pub fn jedi_evasion_saber(
        &mut self,
        me: usize,
        enemy_movedir: [f32; 3],
        enemy_dist: f32,
        enemy_dir: [f32; 3],
        command: &mut UserCommand,
    ) {
        let level_time = self.level_time;
        let Some(enemy) = self.actors[me].mind.enemy else {
            return;
        };
        let Some(foe) = self.jedi_enemy_combat(enemy) else {
            return;
        };
        let npc = &self.actors[me];
        if (foe.weapon == WP_SABER && foe.saber_lock_time > level_time)
            || (npc.saber.event_flags & SEF_LOCK_WON != 0 && foe.pain_debounce_time > level_time)
        {
            return;
        }
        if foe.saber_in_flight && !npc.mind.timers.done("taunting", level_time) {
            self.actors[me]
                .mind
                .timers
                .set("taunting", level_time, -level_time);
            if !self.actors[me].player.saber_in_flight() {
                self.activate_saber(me);
            }
        }
        if self.actors[me].mind.timers.done("parryTime", level_time) {
            let blocked = u32::from(self.actors[me].player.saber_blocked());
            if blocked != BLOCKED_ATK_BOUNCE && blocked != BLOCKED_PARRY_BROKEN {
                self.jedi_set_blocked(me, BLOCKED_NONE);
            }
        }
        let swinging = foe.weapon_time != 0 && foe.weapon_state == WEAPON_FIRING;
        if swinging
            && !self.actors[me].player.saber_in_flight()
            && self.jedi_saber_block(me, 0, 0, command)
        {
            return;
        }
        let mut threat = EnemyThreat {
            attacking: swinging,
            lightning: false,
            throwing: false,
            chance: if swinging { 90 } else { 30 },
        };
        if foe.force_powers_active & (1 << FP_LIGHTNING) != 0 {
            threat = EnemyThreat {
                attacking: true,
                lightning: true,
                chance: 50,
                ..threat
            };
        }
        if foe.saber_in_flight
            && foe.saber_entity_num != ENTITYNUM_NONE
            && foe.saber_entity_state != SES_RETURNING
        {
            threat.attacking = true;
            threat.throwing = true;
        }
        if self.host.irand(0, 100) >= threat.chance {
            return;
        }
        let mut to_me = minus(self.actors[me].current_origin, foe.origin);
        crate::saber_clash::normalize(&mut to_me);
        let still = enemy_movedir == [0.0; 3];
        let facing = if still || threat.lightning || threat.throwing {
            dot(forward_right(foe.view_angles).0, to_me)
        } else {
            dot(enemy_movedir, to_me)
        };
        if self.host.rng().flrand(0.25, 1.0) >= facing {
            return;
        }
        let Some(defense) =
            self.jedi_choose_defense(me, &threat, &foe, still, enemy_dist, enemy_dir)
        else {
            return;
        };
        self.jedi_defend(me, defense, &threat, enemy_dist, command);
        let timers = &mut self.actors[me].mind.timers;
        timers.set("walking", level_time, -level_time);
        timers.set("taunting", level_time, -level_time);
    }

    /// Which defence (`whichDefense`, `NPC_AI_Jedi.c:3421-3505`): 0–3 a push, 4–12 a parry
    /// (a strafe or jump with the saber thrown), anything else a strafe or jump; `None` when
    /// the NPC lets it be.
    fn jedi_choose_defense(
        &mut self,
        me: usize,
        threat: &EnemyThreat,
        foe: &crate::npc_jedi_glue::EnemyCombat,
        still: bool,
        enemy_dist: f32,
        enemy_dir: [f32; 3],
    ) -> Option<i32> {
        let npc = &self.actors[me];
        let aggression = npc.definition.stats.aggression;
        let mut defense = 0;
        if npc.player.weapon_time() != 0
            || npc.player.saber_in_flight()
            || npc.definition.client_class == CLASS_BOBAFETT
        {
            if self.host.irand(0, 10) < aggression {
                return None;
            }
            defense = 100;
        } else {
            if threat.lightning {
                defense = 100;
            } else if threat.throwing {
                defense = self.jedi_thrown_saber_defense(me, foe.saber_entity_num);
            }
            if defense != 0 {
                // Already chosen.
            } else if enemy_dist > 80.0 || !threat.attacking {
                if still || self.host.irand(0, 10) < aggression {
                    return None;
                }
                defense = 100;
            } else {
                let (forward, _) = forward_right(self.actors[me].player.view_angles());
                defense = if dot(enemy_dir, forward) < 0.5 {
                    self.host.irand(5, 16)
                } else if enemy_dist < 56.0 {
                    self.host.irand(aggression, 12)
                } else {
                    self.host.irand(2, 16)
                };
            }
        }
        if (4..=12).contains(&defense) && self.actors[me].player.saber_in_flight() {
            defense = 100;
        }
        Some(defense)
    }

    /// A thrown saber at `saber` heading for the NPC (`NPC_AI_Jedi.c:3451-3478`): within
    /// 100 a push or parry, within 200 maybe a push; a quarter of the time more aggression.
    fn jedi_thrown_saber_defense(&mut self, me: usize, saber: u16) -> i32 {
        let motion = self.jedi_entity_motion(saber);
        let mut to_me = minus(self.actors[me].current_origin, motion.origin);
        let dist = crate::saber_clash::normalize(&mut to_me);
        let mut heading = motion.delta;
        crate::saber_clash::normalize(&mut heading);
        if self.host.irand(0, 3) == 0 {
            self.jedi_aggression(me, 1);
        }
        if dot(heading, to_me) <= 0.5 {
            0
        } else if dist < 100.0 {
            self.host.irand(3, 6)
        } else if dist < 200.0 {
            self.host.irand(0, 8)
        } else {
            0
        }
    }

    /// The defence chosen (`NPC_AI_Jedi.c:3515-3605`).
    fn jedi_defend(
        &mut self,
        me: usize,
        defense: i32,
        threat: &EnemyThreat,
        enemy_dist: f32,
        command: &mut UserCommand,
    ) {
        let level_time = self.level_time;
        let rank = self.actors[me].definition.rank;
        match defense {
            0..=3 => {
                if (rank == RANK_ENSIGN || rank > RANK_LT_JG)
                    && self.actors[me].mind.timers.done("parryTime", level_time)
                {
                    self.force_throw(me, false);
                }
            }
            4..=12 => {
                self.jedi_saber_block(me, 0, 0, command);
            }
            _ => {
                if self.host.irand(0, 5) == 0
                    || !self.jedi_strafe(me, 300, 1_000, 0, 1_000, false, command)
                {
                    self.jedi_evade_otherwise(me, threat, enemy_dist, command);
                } else if self.jedi_may_leap(me)
                    && (rank == RANK_CREWMAN || rank > RANK_LT_JG)
                    && !knocked_down(&self.actors[me])
                    && self.host.irand(0, 5) == 0
                {
                    let charge = if self.actors[me].definition.client_class == CLASS_BOBAFETT {
                        280.0
                    } else {
                        320.0
                    };
                    self.actors[me].force.jump_charge = charge;
                    let debounce = self.host.irand(2_000, 5_000);
                    self.actors[me]
                        .mind
                        .timers
                        .set("jumpChaseDebounce", level_time, debounce);
                }
            }
        }
    }

    /// No strafe (`NPC_AI_Jedi.c:3542-3589`): against lightning, a thrown saber or a close
    /// enemy, a push, or a Force jump over or away from him with a low block, or a parry.
    fn jedi_evade_otherwise(
        &mut self,
        me: usize,
        threat: &EnemyThreat,
        enemy_dist: f32,
        command: &mut UserCommand,
    ) {
        let level_time = self.level_time;
        if !(threat.lightning || threat.throwing || enemy_dist < 80.0) {
            return;
        }
        let npc = &self.actors[me];
        let (rank, aggression) = (npc.definition.rank, npc.definition.stats.aggression);
        let leap = threat.lightning
            || (self.host.irand(0, 2) == 0
                && aggression < 4
                && self.actors[me].mind.timers.done("parryTime", level_time));
        if !leap {
            if threat.attacking {
                self.jedi_saber_block(me, 0, 0, command);
            }
            return;
        }
        if (rank == RANK_ENSIGN || rank > RANK_LT_JG)
            && !threat.lightning
            && self.host.irand(0, 2) != 0
        {
            self.force_throw(me, false);
        } else if (rank == RANK_CREWMAN || rank > RANK_LT_JG)
            && self.jedi_may_leap(me)
            && !knocked_down(&self.actors[me])
        {
            self.actors[me].force.jump_charge = 480.0;
            let debounce = self.host.irand(2_000, 5_000);
            self.actors[me]
                .mind
                .timers
                .set("jumpChaseDebounce", level_time, debounce);
            command.forward_move = if self.host.irand(0, 2) != 0 {
                127
            } else {
                -127
            };
            self.actors[me].mind.move_dir = [0.0; 3];
            let block = if self.host.irand(0, 1) != 0 {
                BLOCKED_LOWER_RIGHT
            } else {
                BLOCKED_LOWER_LEFT
            };
            self.jedi_set_blocked(me, block);
        }
    }
}

/// What the enemy is doing to the NPC (`Jedi_EvasionSaber`'s locals).
#[derive(Clone, Copy, Debug)]
struct EnemyThreat {
    /// `enemy_attacking`, `shooting_lightning`, `throwing_saber`.
    attacking: bool,
    lightning: bool,
    throwing: bool,
    /// `evasionChance`, in hundredths.
    chance: i32,
}

/// `Jedi_DodgeEvasion`'s dodge for where the shot would land (`hitLocation_t`): none for
/// the legs, feet or nowhere.
fn dodge_animation(hit_loc: i32) -> Option<u16> {
    match hit_loc {
        // `HL_BACK_RT`; `HL_BACK`, `HL_CHEST`, `HL_WAIST`; `HL_HEAD`.
        6 | 8 | 11 | 5 | 16 => Some(BOTH_DODGE_FL),
        // `HL_CHEST_RT`, `HL_BACK_LT`, `HL_CHEST_LT`.
        9 | 7 | 10 => Some(BOTH_DODGE_FR),
        // `HL_ARM_RT`, `HL_HAND_RT`.
        12 | 14 => Some(BOTH_DODGE_L),
        // `HL_ARM_LT`, `HL_HAND_LT`.
        13 | 15 => Some(BOTH_DODGE_R),
        _ => None,
    }
}
