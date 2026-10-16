//! The wall acrobatics of `codemp`'s `Pmove`, from the unmodified reference
//! (`codemp/game/bg_pmove.c`):
//!
//! - from the ground, a jump pressed while strafing beside a wall: a run along it
//!   (`BOTH_WALL_RUN_LEFT`/`RIGHT`, pushing forward too) or a flip off it
//!   (`BOTH_WALL_FLIP_LEFT`/`RIGHT`); pulling back, a back flip (`BOTH_FLIP_BACK1`)
//!   — `PM_CheckJump`, `bg_pmove.c:2144-2315`;
//! - in the air, jump pressed again: the flip off a wall being run along
//!   (`BOTH_WALL_RUN_LEFT_FLIP`/`RIGHT_FLIP`, `:2319-2387`), the flip off the top of a
//!   run up a wall (`BOTH_FORCEWALLRUNFLIP_END`, `:2389-2436`), the run up a wall
//!   pushed at just after a jump (`BOTH_FORCEWALLRUNFLIP_START` at jump level 3,
//!   `BOTH_WALL_FLIP_BACK1` at level 2, `:2488-2569`), and the grab of a wall pushed at
//!   (`BOTH_FORCEWALLREBOUND_*`, `PM_GrabWallForJump`, `:2570-2648`, `:1745-1750`);
//! - every command while one lasts (`PmoveSingle`, `:10597-10618`): the alternative flip
//!   onto a ledge at the top of a run up a wall (`BOTH_FORCEWALLRUNFLIP_ALT`,
//!   `PM_AdjustAnglesForWallRunUpFlipAlt`), holding a grabbed wall and kicking off it
//!   (`PM_AdjustAngleForWallJump`, `:1576-1732`), running up a wall
//!   (`PM_AdjustAngleForWallRunUp`, `:1441-1570`) and along one
//!   (`PM_AdjustAngleForWallRun`, `:1325-1431`); with the slow fall of a wall run
//!   (`PM_DoSlowFall`, `:336-344`) and the slide rule that keeps a runner from being
//!   pushed up a wall's slope (`PM_GroundSlideOkay`, `bg_slidemove.c:575-596`).
//!
//! Held against the wall scripts of `tools/prediction-pmove/onfoot.c`, which run the
//! reference's own `Pmove` beside the walls of its room.

use super::{Bounds, MoveContext, MovementCollision, MovementState, Predictor, vector_normalize};
use super::{
    ENTITY_NUMBER_WORLD, FORCE_LEVITATION_BIT, JUMP_VELOCITY, MAX_CLIENTS, PLAYER_CONTENT_MASK,
};
use super::{PMF_JUMP_HELD, PMF_STUCK_TO_WALL, PMF_TIME_KNOCKBACK};
use crate::pmove_anim::{
    SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_FLAG_RESTART, SETANIM_LEGS,
    set_animation,
};
use glam::Vec3;
use sjk_protocol::{ENTITY_NUMBER_NONE, UserCommand};

pub(super) const BOTH_FORCELONGLEAP_START: u16 = 869;
pub(super) const BOTH_FORCELONGLEAP_ATTACK: u16 = 870;
pub(super) const BOTH_FORCELONGLEAP_LAND: u16 = 871;
pub(super) const BOTH_FORCEWALLRUNFLIP_START: u16 = 872;
pub(super) const BOTH_FORCEWALLRUNFLIP_END: u16 = 873;
pub(super) const BOTH_FORCEWALLRUNFLIP_ALT: u16 = 874;
pub(super) const BOTH_FORCEWALLREBOUND_FORWARD: u16 = 875;
pub(super) const BOTH_FORCEWALLREBOUND_LEFT: u16 = 876;
pub(super) const BOTH_FORCEWALLREBOUND_BACK: u16 = 877;
pub(super) const BOTH_FORCEWALLREBOUND_RIGHT: u16 = 878;
pub(super) const BOTH_FORCEWALLHOLD_FORWARD: u16 = 879;
pub(super) const BOTH_FORCEWALLHOLD_LEFT: u16 = 880;
pub(super) const BOTH_FORCEWALLHOLD_BACK: u16 = 881;
pub(super) const BOTH_FORCEWALLHOLD_RIGHT: u16 = 882;
pub(super) const BOTH_FORCEWALLRELEASE_FORWARD: u16 = 883;
pub(super) const BOTH_JUMP1: u16 = 1_138;
pub(super) const BOTH_INAIR1: u16 = 1_139;
/// The Force jump's flips forward, left and right (`PM_SetForceJumpFlip`).
const BOTH_FLIP_F: u16 = 1_163;
const BOTH_FLIP_L: u16 = 1_165;
const BOTH_FLIP_R: u16 = 1_166;
pub(super) const BOTH_FORCEJUMP1: u16 = 1_151;
pub(super) const BOTH_FLIP_BACK1: u16 = 1_206;
pub(super) const BOTH_FLIP_BACK2: u16 = 1_207;
pub(super) const BOTH_FLIP_BACK3: u16 = 1_208;
pub(super) const BOTH_WALL_RUN_RIGHT: u16 = 1_211;
pub(super) const BOTH_WALL_RUN_RIGHT_FLIP: u16 = 1_212;
pub(super) const BOTH_WALL_RUN_RIGHT_STOP: u16 = 1_213;
pub(super) const BOTH_WALL_RUN_LEFT: u16 = 1_214;
pub(super) const BOTH_WALL_RUN_LEFT_FLIP: u16 = 1_215;
pub(super) const BOTH_WALL_RUN_LEFT_STOP: u16 = 1_216;
pub(super) const BOTH_WALL_FLIP_RIGHT: u16 = 1_217;
pub(super) const BOTH_WALL_FLIP_LEFT: u16 = 1_218;
pub(super) const BOTH_WALL_FLIP_BACK1: u16 = 1_247;

/// `forceJumpStrength[FORCE_LEVEL_2]`, `[FORCE_LEVEL_3]` (`bg_pmove.c:185-191`).
const STRENGTH_2: f32 = 590.0;
const STRENGTH_3: f32 = 840.0;
/// `BG_ForceWallJumpStrength` (`bg_pmove.c:1571-1575`): `forceJumpStrength[3] / 2.5f`.
const WALL_JUMP_STRENGTH: f32 = STRENGTH_3 / 2.5;
/// `forceJumpHeightMax[FORCE_LEVEL_3]` (`bg_pmove.c:1737-1743`).
const JUMP_HEIGHT_MAX_3: f32 = 418.0;
/// `JUMP_OFF_WALL_SPEED` (`bg_pmove.c:1569`).
const JUMP_OFF_WALL_SPEED: f32 = 200.0;
/// `MASK_SOLID`, `CONTENTS_SOLID`, `CONTENTS_BODY` (`bg_public.h:1225`, `surfaceflags.h`).
const MASK_SOLID: u32 = 0x1 | 0x1000;
const CONTENTS_SOLID: u32 = 0x1;
const CONTENTS_BODY: u32 = 0x100;
/// `EV_JUMP`.
const EV_JUMP: u16 = 16;
/// `BUTTON_ATTACK`, `WP_SABER`.
const BUTTON_ATTACK: u16 = 1;
const WP_SABER: u8 = 3;
/// `SFL_NO_WALL_RUNS`, `SFL_NO_WALL_FLIPS`, `SFL_NO_WALL_GRAB`, `SFL_NO_FLIPS`
/// (`bg_public.h:1579-1583`).
const SFL_NO_WALL_RUNS: u32 = 1 << 13;
const SFL_NO_WALL_FLIPS: u32 = 1 << 14;
const SFL_NO_WALL_GRAB: u32 = 1 << 15;
const SFL_NO_FLIPS: u32 = 1 << 17;
/// The box of the wall traces that do not use the player's (`-15 -15 0`, `15 15 24`).
const WALL_BOX: ([f32; 3], [f32; 3]) = ([-15.0, -15.0, 0.0], [15.0, 15.0, 24.0]);

/// `BG_InReboundJump` (`bg_panimate.c:168-180`).
pub(super) fn rebound_jump(animation: u16) -> bool {
    (BOTH_FORCEWALLREBOUND_FORWARD..=BOTH_FORCEWALLREBOUND_RIGHT).contains(&animation)
}

/// `BG_InReboundHold` (`bg_panimate.c:182-194`).
pub(super) fn rebound_hold(animation: u16) -> bool {
    (BOTH_FORCEWALLHOLD_FORWARD..=BOTH_FORCEWALLHOLD_RIGHT).contains(&animation)
}

/// `BG_InBackFlip` (`bg_panimate.c:210-222`).
fn back_flip(animation: u16) -> bool {
    matches!(
        animation,
        BOTH_FLIP_BACK1 | BOTH_FLIP_BACK2 | BOTH_FLIP_BACK3
    )
}

/// `PM_DoSlowFall` (`bg_pmove.c:336-344`): running along a wall, not at the end of it,
/// a player falls at half gravity and has no air control. `PmoveSingle` asks once, as
/// the command starts (`:10213`).
pub(super) fn slow_fall(state: &MovementState) -> bool {
    matches!(state.legs_anim, BOTH_WALL_RUN_RIGHT | BOTH_WALL_RUN_LEFT) && state.legs_timer > 500
}

/// `pm->ps->gravity *= 0.5` on an integer gravity (`bg_pmove.c:10726-10730`).
pub(super) fn halved_gravity(gravity: f32) -> f32 {
    (f64::from(gravity as i32) * 0.5) as i32 as f32
}

/// `PM_GroundSlideOkay` (`bg_slidemove.c:575-596`): a plane facing up at all is not
/// slid along while rising in a wall run, a run up a wall, a long leap or a rebound —
/// the move treats it as a vertical wall instead, so the runner is never pushed up it.
pub(super) fn ground_slide_okay(legs: u16, vertical_velocity: f32, normal_z: f32) -> bool {
    !(normal_z > 0.0
        && vertical_velocity > 0.0
        && (matches!(
            legs,
            BOTH_WALL_RUN_RIGHT
                | BOTH_WALL_RUN_LEFT
                | BOTH_WALL_RUN_RIGHT_STOP
                | BOTH_WALL_RUN_LEFT_STOP
                | BOTH_FORCEWALLRUNFLIP_START
                | BOTH_FORCELONGLEAP_START
                | BOTH_FORCELONGLEAP_ATTACK
                | BOTH_FORCELONGLEAP_LAND
        ) || rebound_jump(legs)))
}

/// What the held sabers allow (`BG_MySaber`'s `saberFlags`, `bg_pmove.c:1795-1808,
/// 2095-2139`); only a saber in hand has a say.
#[derive(Clone, Copy, Debug)]
pub(super) struct JumpRules {
    pub(super) flips: bool,
    wall_runs: bool,
    wall_flips: bool,
    wall_grabs: bool,
}

impl JumpRules {
    pub(super) fn of(state: &MovementState, context: &MoveContext) -> Self {
        let forbids = |flag| state.weapon == WP_SABER && context.sabers.any_flag(flag);
        Self {
            flips: !forbids(SFL_NO_FLIPS),
            wall_runs: !forbids(SFL_NO_WALL_RUNS),
            wall_flips: !forbids(SFL_NO_WALL_FLIPS),
            wall_grabs: !forbids(SFL_NO_WALL_GRAB),
        }
    }
}

/// `AngleVectors` of `(0, yaw, 0)`: the flat forward and right axes.
fn yaw_axes(yaw: f32) -> (Vec3, Vec3) {
    super::flight::flight_axes([0.0, yaw, 0.0])
}

/// Where `PM_AdjustAngleForWallJump` looks for the wall of a rebound (`legsAnim`) from
/// the view's yaw (`checkDir`), and the turn from the wall's normal that faces the player
/// the way its pose expects (`yawAdjust`); `None` outside the rebounds and their holds.
fn rebound_side(legs: u16, view_yaw: f32) -> Option<(Vec3, f32)> {
    let (forward, right) = yaw_axes(view_yaw);
    match legs {
        BOTH_FORCEWALLREBOUND_RIGHT | BOTH_FORCEWALLHOLD_RIGHT => Some((right, -90.0)),
        BOTH_FORCEWALLREBOUND_LEFT | BOTH_FORCEWALLHOLD_LEFT => Some((right * -1.0, 90.0)),
        BOTH_FORCEWALLREBOUND_FORWARD | BOTH_FORCEWALLHOLD_FORWARD => Some((forward, 180.0)),
        BOTH_FORCEWALLREBOUND_BACK | BOTH_FORCEWALLHOLD_BACK => Some((forward * -1.0, 0.0)),
        _ => None,
    }
}

/// The wall check of `PM_AdjustAngleForWallJump`: 128 units to the rebound's side, a box
/// as wide as the player and 24 units tall from its origin; the normal of an upright wall
/// (`fabs(normal[2]) <= 0.2`) struck there.
fn rebound_wall(
    origin: [f32; 3],
    direction: Vec3,
    horizontal: ([f32; 3], [f32; 3]),
    collision: &impl MovementCollision,
) -> Option<[f32; 3]> {
    let minimums = [horizontal.0[0], horizontal.0[1], 0.0];
    let maximums = [horizontal.1[0], horizontal.1[1], 24.0];
    let end = Vec3::from_array(origin) + direction * 128.0;
    let trace = collision.trace(
        origin,
        minimums,
        maximums,
        end.to_array(),
        PLAYER_CONTENT_MASK,
    );
    (trace.fraction < 1.0 && trace.plane_normal[2].abs() <= 0.2).then_some(trace.plane_normal)
}

/// A wall rebound or its hold (`BG_InReboundJump || BG_InReboundHold`): the animations
/// during which `PM_AdjustAngleForWallJump` holds a grabbed wall.
pub fn in_wall_rebound(legs: u16) -> bool {
    rebound_jump(legs) || rebound_hold(legs)
}

/// For presentation: the yaw a player in a wall rebound or its hold (`legs`) faces while
/// the wall is there, as `PM_AdjustAngleForWallJump` turns a stock server's player
/// (`ps->viewangles[YAW] = vectoyaw(trace.plane.normal) + yawAdjust`).
///
/// A JA+ server leaves the view free while the wall is held
/// ([`crate::pmove_debug_melee::DebugMelee::free_wall_look`]), so a model drawn from
/// the view would turn away from the wall it holds; drawing it at this yaw keeps it on
/// the wall. Nothing in the move changes: the view, the hold and the kick off the wall
/// (straight back from the side the view picks, `checkDir`) are the server's.
/// `None` outside a rebound or without an upright wall within reach of the standing
/// player's box (`-15 -15`, `15 15`).
pub fn wall_hold_yaw(
    legs: u16,
    view_yaw: f32,
    origin: [f32; 3],
    collision: &impl MovementCollision,
) -> Option<f32> {
    let (direction, yaw_adjust) = rebound_side(legs, view_yaw)?;
    let horizontal = ([-15.0, -15.0, 0.0], [15.0, 15.0, 0.0]);
    rebound_wall(origin, direction, horizontal, collision)
        .map(|normal| crate::npc_nav::vector_to_yaw(normal) + yaw_adjust)
}

/// A wall a runner may run on (`MAX_WALL_RUN_Z_NORMAL`, 0.4): upright or overhanging
/// no further than that.
fn runnable(normal_z: f32) -> bool {
    (0.0..=0.4).contains(&normal_z)
}

/// `PM_SetPMViewAngle(ps, ps->viewangles, ucmd)` (`bg_pmove.c:1311-1322`): the view
/// stays as it is, whatever the command turns — `delta_angles` absorbs the difference.
fn hold_view(state: &mut MovementState, command: &UserCommand) {
    crate::pmove_input_freeze::set_view_angle(state, command);
    state.wall_view_locked = true;
}

/// Turn to face `yaw` and hold the view there, as each wall move does:
/// `ps->viewangles[YAW] = ...; PM_SetPMViewAngle(...); ucmd->angles[YAW] = ANGLE2SHORT(...)
/// - ps->delta_angles[YAW]`.
fn face(state: &mut MovementState, command: &mut UserCommand, yaw: f32) {
    state.view_angles[1] = yaw;
    hold_view(state, command);
    command.angles[1] = crate::npc_think::angle_to_short(yaw).wrapping_sub(state.delta_angles[1]);
}

impl Predictor {
    /// `PM_SetAnim` for the wall moves: a predictor without an animation table has no
    /// timers to run them by, and leaves the animations alone.
    fn wall_animation(&mut self, parts: u8, animation: u16, flags: u8) {
        if let Some(lengths) = self.animation_lengths.clone() {
            set_animation(&mut self.state, parts, animation, flags, &*lengths);
        }
    }

    /// `PM_AnimLength` (`bg_panimate.c:1601-1607`); `BG_AnimLength` reads the same table.
    fn animation_length(&self, animation: u16) -> f32 {
        self.animation_lengths
            .as_deref()
            .and_then(|lengths| lengths.length_ms(animation))
            .unwrap_or(0) as f32
    }

    /// The legs alone while a weapon is in use, else the whole body.
    fn jump_parts(&self) -> u8 {
        if self.state.weapon_time != 0 {
            SETANIM_LEGS
        } else {
            SETANIM_BOTH
        }
    }

    /// `pm->mins`, `pm->maxs` before this slice's `PM_CheckDuck`: the last slice's box —
    /// except on a server's first slice of a command, whose `pmove_t` `ClientThink_real`
    /// has just cleared (`g_active.c:2754`; an NPC's is its entity's, `:3007-3010`).
    fn box_before_duck(&self) -> ([f32; 3], [f32; 3]) {
        if self.cleared_box {
            ([0.0; 3], [0.0; 3])
        } else {
            self.box_bounds
        }
    }

    /// A trace struck something other than a brush model (`PM_BGEntForNum(...)->s.solid
    /// != SOLID_BMODEL`). The game's entity table is not in the move: a body the context
    /// knows counts as such, anything else below the world is taken for a brush model.
    fn struck_non_brush(entity: u16, context: &MoveContext) -> bool {
        entity < ENTITY_NUMBER_WORLD && (context.bodies)(entity).is_some()
    }

    /// Whether any of `PmoveSingle`'s per-frame wall handlers has anything to do.
    pub(super) fn wall_moves_live(&self) -> bool {
        let state = &self.state;
        let rebounding = |animation| rebound_jump(animation) || rebound_hold(animation);
        state.legs_anim == BOTH_FORCEWALLRUNFLIP_ALT && state.legs_timer > 0
            || rebounding(state.legs_anim) && rebounding(state.torso_anim)
            || state.movement_flags & PMF_STUCK_TO_WALL != 0
            || state.legs_anim == BOTH_FORCEWALLRUNFLIP_START
            || slow_fall(state)
    }

    /// `PmoveSingle` before `PM_UpdateViewAngles` (`bg_pmove.c:10597-10618`), with the view
    /// the last command left: the alternative flip's locked view, then
    /// `PM_AdjustAngleForWallJump`, `PM_AdjustAngleForWallRunUp` and
    /// `PM_AdjustAngleForWallRun`, each with `doMove`.
    pub(super) fn adjust_for_wall_moves(
        &mut self,
        command: &mut UserCommand,
        collision: &impl MovementCollision,
    ) {
        if self.state.legs_anim == BOTH_FORCEWALLRUNFLIP_ALT && self.state.legs_timer > 0 {
            let (forward, _) = yaw_axes(self.state.view_angles[1]);
            if self.state.ground_entity_number == ENTITY_NUMBER_NONE {
                let rising = self.state.velocity[2];
                self.state.velocity = (forward * 100.0).to_array();
                self.state.velocity[2] = rising;
            }
            (command.forward_move, command.right_move, command.up_move) = (0, 0, 0);
            // `PM_AdjustAnglesForWallRunUpFlipAlt` (`bg_pmove.c:1433-1439`).
            hold_view(&mut self.state, command);
        }
        self.hold_wall(command, collision);
        self.run_up_wall(command, collision);
        self.run_along_wall(command, collision);
    }

    /// `PM_AdjustAngleForWallJump` (`bg_pmove.c:1576-1732`): a grabbed wall is held,
    /// facing it and pulled to it, while the rebound's animation has more than 100 ms to
    /// run; then the player kicks off it — 200 units a second away, 336 up, half a second
    /// without control, Force spent as a Force jump's. Under `g_debugMelee` a player
    /// holding jump keeps a wall until letting go (`:1621-1641`, from level 2 on JA+); on
    /// JA+ the held wall does not turn the view, which follows the mouse
    /// ([`crate::pmove_debug_melee`]).
    fn hold_wall(&mut self, command: &mut UserCommand, collision: &impl MovementCollision) {
        let rebounding = |animation| rebound_jump(animation) || rebound_hold(animation);
        let state = &self.state;
        if !(rebounding(state.legs_anim) && rebounding(state.torso_anim)
            || state.movement_flags & PMF_STUCK_TO_WALL != 0)
        {
            self.state.movement_flags &= !PMF_STUCK_TO_WALL;
            return;
        }
        let Some((direction, yaw_adjust)) =
            rebound_side(self.state.legs_anim, self.state.view_angles[1])
        else {
            self.state.movement_flags &= !PMF_STUCK_TO_WALL;
            return;
        };
        let debug_melee = self.config.debug_melee;
        if debug_melee.holds_walls() && command.up_move > 0 {
            if rebound_hold(self.state.legs_anim) {
                // Keep holding.
                self.state.legs_timer = self.state.legs_timer.max(150);
            } else if self.state.legs_timer <= 300 {
                // The rebound reached its hold: `BOTH_FORCEWALLRELEASE_FORWARD` plus the
                // rebound's offset from `BOTH_FORCEWALLHOLD_FORWARD` is its hold pose.
                self.state.saber_holstered = 2;
                let hold = i32::from(BOTH_FORCEWALLRELEASE_FORWARD)
                    + (i32::from(self.state.legs_anim) - i32::from(BOTH_FORCEWALLHOLD_FORWARD));
                self.wall_animation(
                    SETANIM_BOTH,
                    hold as u16,
                    SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                );
                self.state.legs_timer = 150;
                self.state.torso_timer = 150;
            }
        }
        let wall = rebound_wall(
            self.state.origin,
            direction,
            self.box_before_duck(),
            collision,
        );
        if let Some(normal) = wall.filter(|_| self.state.legs_timer > 100) {
            command.up_move = command.up_move.max(0);
            if !debug_melee.free_wall_look {
                face(
                    &mut self.state,
                    command,
                    crate::npc_nav::vector_to_yaw(normal) + yaw_adjust,
                );
            }
            self.state.velocity = (Vec3::from_array(normal) * -128.0).to_array();
            command.up_move = 0;
            self.state.movement_flags |= PMF_STUCK_TO_WALL;
            return;
        }
        if self.state.movement_flags & PMF_STUCK_TO_WALL != 0 {
            // Push off.
            self.state.movement_flags &= !PMF_STUCK_TO_WALL;
            self.state.velocity = (direction * -JUMP_OFF_WALL_SPEED).to_array();
            self.state.velocity[2] = WALL_JUMP_STRENGTH;
            self.state.movement_flags |= PMF_JUMP_HELD;
            self.state.force_jump_sound = true;
            if self.state.origin[2] < self.state.force_jump_start_height {
                self.state.force_jump_start_height = self.state.origin[2];
            }
            super::force_jump::drain_levitation(&mut self.state);
            self.state.movement_flags |= PMF_TIME_KNOCKBACK;
            self.state.movement_time = 500;
            (command.forward_move, command.right_move, command.up_move) = (0, 0, 127);
            if rebound_hold(self.state.legs_anim) {
                let release = BOTH_FORCEWALLRELEASE_FORWARD
                    + (self.state.legs_anim - BOTH_FORCEWALLHOLD_FORWARD);
                self.wall_animation(
                    SETANIM_BOTH,
                    release,
                    SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                );
            } else {
                self.wall_animation(
                    SETANIM_LEGS,
                    BOTH_FORCEJUMP1,
                    SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD | SETANIM_FLAG_RESTART,
                );
            }
        }
        self.state.movement_flags &= !PMF_STUCK_TO_WALL;
    }

    /// `PM_AdjustAngleForWallRunUp` (`bg_pmove.c:1441-1566`): running up a wall. Room at
    /// the top with a floor to land on, the player flips onto it (`BOTH_FORCEWALLRUNFLIP_ALT`);
    /// a wall still ahead, pushing forward, no ceiling within 64 units, it keeps climbing at
    /// 300 a second, facing the wall and pulled to it; otherwise it flips off backwards.
    fn run_up_wall(&mut self, command: &mut UserCommand, collision: &impl MovementCollision) {
        if self.state.legs_anim != BOTH_FORCEWALLRUNFLIP_START {
            return;
        }
        const DISTANCE: f32 = 128.0;
        let (minimums, maximums) = WALL_BOX;
        let (forward, _) = yaw_axes(self.state.view_angles[1]);
        let origin = Vec3::from_array(self.state.origin);
        let trace = collision.trace(
            self.state.origin,
            minimums,
            maximums,
            (origin + forward * DISTANCE).to_array(),
            PLAYER_CONTENT_MASK,
        );
        if trace.fraction > 0.5 {
            // Some room: is there a floor right there?
            let (box_minimums, box_maximums) = self.box_before_duck();
            let mut top = trace.end_position;
            top[2] += box_minimums[2] * -1.0 + 4.0;
            let mut bottom = top;
            bottom[2] -= 64.0;
            let floor =
                collision.trace(top, box_minimums, box_maximums, bottom, PLAYER_CONTENT_MASK);
            if !floor.all_solid
                && !floor.start_solid
                && floor.fraction < 1.0
                && floor.plane_normal[2] > 0.7
            {
                self.state.velocity = (forward * 100.0).to_array();
                self.state.velocity[2] += 400.0;
                self.wall_animation(
                    SETANIM_BOTH,
                    BOTH_FORCEWALLRUNFLIP_ALT,
                    SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                );
                self.state.movement_flags |= PMF_JUMP_HELD;
                self.add_event(EV_JUMP, 0);
                command.up_move = 0;
                return;
            }
        }
        if self.state.legs_timer > 0
            && command.forward_move > 0
            && trace.fraction < 1.0
            && runnable(trace.plane_normal[2])
        {
            let mut above = self.state.origin;
            above[2] += 64.0;
            let ceiling = collision.trace(
                self.state.origin,
                minimums,
                maximums,
                above,
                PLAYER_CONTENT_MASK,
            );
            // A ceiling (or anything else) within 64 units forces the flip off now.
            if ceiling.fraction >= 1.0 {
                command.forward_move = 127;
                command.up_move = command.up_move.max(0);
                face(
                    &mut self.state,
                    command,
                    crate::npc_nav::vector_to_yaw(trace.plane_normal) + 180.0,
                );
                self.state.velocity = (Vec3::from_array(trace.plane_normal)
                    * (-DISTANCE * trace.fraction))
                    .to_array();
                if self.state.legs_timer > 200 {
                    self.state.velocity[2] = 300.0;
                }
                command.forward_move = 0;
                return;
            }
        }
        // Failed: flip off backwards.
        self.state.velocity = (forward * -300.0).to_array();
        self.state.velocity[2] += 200.0;
        self.wall_animation(
            SETANIM_BOTH,
            BOTH_FORCEWALLRUNFLIP_END,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        );
        self.state.movement_flags |= PMF_JUMP_HELD;
        self.add_event(EV_JUMP, 0);
        command.up_move = 0;
    }

    /// `PM_AdjustAngleForWallRun` (`bg_pmove.c:1325-1431`): running along a wall, until
    /// the animation's last half second — facing along the wall, 250 a second pushing
    /// forward (175 not, 100 pulling back), pulled towards it; a wall ahead it cannot run
    /// on, or none beside it any more, ends the run (`BOTH_WALL_RUN_*_STOP`).
    fn run_along_wall(&mut self, command: &mut UserCommand, collision: &impl MovementCollision) {
        if !slow_fall(&self.state) {
            return;
        }
        let (minimums, maximums) = WALL_BOX;
        let (forward, right) = yaw_axes(self.state.view_angles[1]);
        let running_right = self.state.legs_anim == BOTH_WALL_RUN_RIGHT;
        let (distance, yaw_adjust) = if running_right {
            (128.0, -90.0)
        } else {
            (-128.0, 90.0)
        };
        let origin = Vec3::from_array(self.state.origin);
        let trace = collision.trace(
            self.state.origin,
            minimums,
            maximums,
            (origin + right * distance).to_array(),
            PLAYER_CONTENT_MASK,
        );
        let mut fraction = trace.fraction;
        if fraction < 1.0 && runnable(trace.plane_normal[2]) {
            let (along, _) =
                yaw_axes(crate::npc_nav::vector_to_yaw(trace.plane_normal) + yaw_adjust);
            let ahead = collision.trace(
                self.state.origin,
                minimums,
                maximums,
                (origin + along * 32.0).to_array(),
                PLAYER_CONTENT_MASK,
            );
            if ahead.fraction < 1.0 && Vec3::from_array(ahead.plane_normal).dot(along) <= -0.999 {
                // A wall ahead it cannot run on: kicked off the wall below.
                fraction = 1.0;
            }
        }
        if fraction < 1.0 && runnable(trace.plane_normal[2]) {
            command.right_move = if running_right { 127 } else { -127 };
            command.up_move = command.up_move.max(0);
            face(
                &mut self.state,
                command,
                crate::npc_nav::vector_to_yaw(trace.plane_normal) + yaw_adjust,
            );
            let rising = self.state.velocity[2];
            if self.state.legs_timer > 500 {
                let speed = match command.forward_move {
                    forward_move if forward_move < 0 => 100.0,
                    forward_move if forward_move > 0 => 250.0,
                    _ => 175.0,
                };
                self.state.velocity = (forward * speed).to_array();
            }
            self.state.velocity[2] = rising;
            self.state.velocity =
                (Vec3::from_array(self.state.velocity) + right * distance).to_array();
            command.forward_move = 0;
        } else {
            let stop = if running_right {
                BOTH_WALL_RUN_RIGHT_STOP
            } else {
                BOTH_WALL_RUN_LEFT_STOP
            };
            self.wall_animation(
                SETANIM_BOTH,
                stop,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
        }
    }

    /// `PM_CheckJump`'s special jumps from the ground (`bg_pmove.c:2144-2315`): above jump
    /// level 1, strafing with a wall within 16 units on that side, a run along it (pushing
    /// forward too) or a flip off it (not); pulling back without attacking, a back flip.
    /// None of them costs Force up front: each starts a Force jump, whose upkeep pays.
    pub(super) fn ground_special_jump(
        &mut self,
        command: &mut UserCommand,
        bounds: Bounds,
        collision: &impl MovementCollision,
        rules: JumpRules,
    ) {
        let level = self.state.levitation_level;
        let chosen = match (command.right_move, command.forward_move) {
            (right, forward) if right > 0 && level > 1 && forward > 0 => rules
                .wall_runs
                .then_some((BOTH_WALL_RUN_RIGHT, STRENGTH_2 / 2.0)),
            (right, 0) if right > 0 && level > 1 => rules
                .wall_flips
                .then_some((BOTH_WALL_FLIP_RIGHT, STRENGTH_2 / 2.25)),
            (right, _) if right > 0 && level > 1 => None,
            (right, forward) if right < 0 && level > 1 && forward > 0 => rules
                .wall_runs
                .then_some((BOTH_WALL_RUN_LEFT, STRENGTH_2 / 2.0)),
            (right, 0) if right < 0 && level > 1 => rules
                .wall_flips
                .then_some((BOTH_WALL_FLIP_LEFT, STRENGTH_2 / 2.25)),
            (right, _) if right < 0 && level > 1 => None,
            (_, forward) if forward < 0 && command.buttons & BUTTON_ATTACK == 0 => {
                rules.flips.then_some((BOTH_FLIP_BACK1, JUMP_VELOCITY))
            }
            _ => None,
        };
        let Some((animation, push)) = chosen else {
            return;
        };
        // "give them an extra shove"
        let vertical_push = push + 128.0;
        let minimums = [bounds.minimums[0], bounds.minimums[1], 0.0];
        let maximums = [bounds.maximums[0], bounds.maximums[1], 24.0];
        let (forward, right) = yaw_axes(self.state.view_angles[1]);
        let origin = Vec3::from_array(self.state.origin);
        let target = match animation {
            BOTH_WALL_FLIP_LEFT | BOTH_WALL_RUN_LEFT => Some(origin + right * -16.0),
            BOTH_WALL_FLIP_RIGHT | BOTH_WALL_RUN_RIGHT => Some(origin + right * 16.0),
            _ => None,
        };
        let mut wall_normal = Vec3::ZERO;
        if let Some(target) = target {
            // A JA+ server with flip kick lets a player serve as the wall
            // (`bg_pmove.c:2655-2665` in EternalJK).
            let mask = if self.config.ja_plus.flip_kick() {
                PLAYER_CONTENT_MASK
            } else {
                MASK_SOLID
            };
            let trace = collision.trace(
                self.state.origin,
                minimums,
                maximums,
                target.to_array(),
                mask,
            );
            wall_normal = vector_normalize(Vec3::from_array(trace.plane_normal));
            let ideal = vector_normalize(origin - target);
            // A wall there, or a client.
            if !(trace.fraction < 1.0
                && (trace.entity_number < MAX_CLIENTS || f64::from(wall_normal.dot(ideal)) > 0.7))
            {
                return;
            }
        }
        // Wall runs only on (nearly) upright walls.
        if matches!(animation, BOTH_WALL_RUN_LEFT | BOTH_WALL_RUN_RIGHT) && !runnable(wall_normal.z)
        {
            return;
        }
        let away = match animation {
            BOTH_WALL_FLIP_LEFT => Some(right * 150.0),
            BOTH_WALL_FLIP_RIGHT => Some(right * -150.0),
            BOTH_FLIP_BACK1 => Some(forward * -150.0),
            _ => None,
        };
        if let Some(away) = away {
            let rising = self.state.velocity[2];
            self.state.velocity = (Vec3::new(0.0, 0.0, rising) + away).to_array();
        }
        self.state.velocity[2] = vertical_push;
        self.state.force_powers_active |= FORCE_LEVITATION_BIT;
        let parts = self.jump_parts();
        self.wall_animation(parts, animation, SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD);
        // So a landing at the same height does no damage.
        self.set_force_jump_start(self.state.origin[2]);
        self.state.movement_flags |= PMF_JUMP_HELD;
        command.up_move = 0;
        self.state.force_jump_sound = true;
    }

    /// `PM_CheckJump`'s special jumps in the air (`bg_pmove.c:2316-2653`), jump pressed
    /// again: off a wall being run along, off the top of a run up a wall, up a wall
    /// pushed at just after a jump, and the grab of a wall pushed at.
    pub(super) fn air_special_jump(
        &mut self,
        command: &mut UserCommand,
        bounds: Bounds,
        collision: &impl MovementCollision,
        context: &MoveContext,
        rules: JumpRules,
    ) {
        let legs = self.state.legs_anim;
        let level = self.state.levitation_level;
        if matches!(legs, BOTH_WALL_RUN_LEFT | BOTH_WALL_RUN_RIGHT) {
            self.flip_off_wall_run(command, bounds, collision);
        } else if legs == BOTH_FORCEWALLRUNFLIP_START {
            self.flip_off_wall_top(command, bounds, collision);
        } else if self.config.ja_plus.flip_kick()
            && command.forward_move > 0
            && level > 1
            && self.state.velocity[2] > 200.0
            && self.ground_distance(bounds, collision) <= 80.0
            && !crate::pmove_roll_anim::special_jump(legs)
            && self.flip_off_player(command, bounds, collision, context)
        {
            // A JA+ server's flip kick: rising off a jump, pushing forward, a flip back
            // off a player ahead (`bg_pmove.c:2919-2990`). Unlike EternalJK's branch, a
            // JA+ server still runs up a wall when no player is there.
        } else if command.forward_move > 0
            && self.state.force_rage_recovery_time < command.server_time
            && level > 1
            && self.walkable_ground_distance(bounds, collision) <= 80.0
            && (matches!(legs, BOTH_JUMP1 | BOTH_INAIR1)
                || self.config.ja_plus.wall_runs_from_force_flips()
                    && matches!(legs, BOTH_FLIP_F | BOTH_FLIP_L | BOTH_FLIP_R))
        {
            if rules.wall_runs {
                self.start_wall_run_up(command, bounds, collision, context);
            }
        } else if (!crate::pmove_roll_anim::special_jump(legs)
            || rebound_jump(legs)
            || back_flip(legs))
            && self.state.velocity[2] > -1_200.0
            && self.state.movement_flags & PMF_JUMP_HELD == 0
            && (command.forward_move != 0 || command.right_move != 0)
            && level > 2
            && crate::pmove_saber_attack::can_levitate_now(
                &self.state,
                command.server_time,
                self.gametype,
            )
            && self.state.origin[2] - self.state.force_jump_start_height
                < JUMP_HEIGHT_MAX_3 - WALL_JUMP_STRENGTH / 2.0
            && rules.wall_grabs
        {
            self.grab_wall(command, bounds, collision, context);
        }
    }

    /// Off a wall being run along, neither at its start nor its end
    /// (`bg_pmove.c:2319-2387`): away from it at half speed plus 150.
    fn flip_off_wall_run(
        &mut self,
        command: &mut UserCommand,
        bounds: Bounds,
        collision: &impl MovementCollision,
    ) {
        let legs = self.state.legs_anim;
        // `mins[0]` twice, as the reference has it.
        let minimums = [bounds.minimums[0], bounds.minimums[0], 0.0];
        let maximums = [bounds.maximums[0], bounds.maximums[0], 24.0];
        let (_, right) = yaw_axes(self.state.view_angles[1]);
        if self.state.legs_timer <= 400
            || self.state.legs_timer as f32 >= self.animation_length(legs) - 400.0
        {
            return;
        }
        let (side, flip, away) = if legs == BOTH_WALL_RUN_LEFT {
            (-16.0, BOTH_WALL_RUN_LEFT_FLIP, 150.0)
        } else {
            (16.0, BOTH_WALL_RUN_RIGHT_FLIP, -150.0)
        };
        let target = (Vec3::from_array(self.state.origin) + right * side).to_array();
        let trace = collision.trace(
            self.state.origin,
            minimums,
            maximums,
            target,
            CONTENTS_SOLID | CONTENTS_BODY,
        );
        if trace.fraction < 1.0 {
            self.state.velocity[0] *= 0.5;
            self.state.velocity[1] *= 0.5;
            self.state.velocity = (Vec3::from_array(self.state.velocity) + right * away).to_array();
            let parts = self.jump_parts();
            self.wall_animation(parts, flip, SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD);
            command.up_move = 0;
        }
    }

    /// Off the top of a run up a wall, past its first 400 ms (`bg_pmove.c:2389-2436`):
    /// backwards at 300 plus half the speed it had, 200 up.
    fn flip_off_wall_top(
        &mut self,
        command: &mut UserCommand,
        bounds: Bounds,
        collision: &impl MovementCollision,
    ) {
        let minimums = [bounds.minimums[0], bounds.minimums[0], 0.0];
        let maximums = [bounds.maximums[0], bounds.maximums[0], 24.0];
        let (forward, _) = yaw_axes(self.state.view_angles[1]);
        if self.state.legs_timer as f32
            >= self.animation_length(BOTH_FORCEWALLRUNFLIP_START) - 400.0
        {
            return;
        }
        let target = (Vec3::from_array(self.state.origin) + forward * 16.0).to_array();
        let trace = collision.trace(
            self.state.origin,
            minimums,
            maximums,
            target,
            CONTENTS_SOLID | CONTENTS_BODY,
        );
        if trace.fraction < 1.0 {
            self.state.velocity[0] *= 0.5;
            self.state.velocity[1] *= 0.5;
            self.state.velocity =
                (Vec3::from_array(self.state.velocity) + forward * -300.0).to_array();
            self.state.velocity[2] += 200.0;
            let parts = self.jump_parts();
            self.wall_animation(
                parts,
                BOTH_FORCEWALLRUNFLIP_END,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            command.up_move = 0;
            self.add_event(EV_JUMP, 0);
        }
    }

    /// A JA+ flip kick's flip back off a player (or NPC) within 32 units ahead
    /// (`bg_pmove.c:2925-2989` in EternalJK): 150 back, 128 more up, the end of the
    /// flip cut by 600 ms. Whether there was one to flip off.
    fn flip_off_player(
        &mut self,
        command: &mut UserCommand,
        bounds: Bounds,
        collision: &impl MovementCollision,
        context: &MoveContext,
    ) -> bool {
        let (forward, _) = yaw_axes(self.state.view_angles[1]);
        let target = (Vec3::from_array(self.state.origin) + forward * 32.0).to_array();
        let trace = collision.trace(
            self.state.origin,
            bounds.minimums,
            bounds.maximums,
            target,
            PLAYER_CONTENT_MASK,
        );
        if !(trace.fraction < 1.0
            && (trace.entity_number < MAX_CLIENTS || (context.npcs)(trace.entity_number)))
        {
            return false;
        }
        self.state.velocity[0] = 0.0;
        self.state.velocity[1] = 0.0;
        self.state.velocity = (Vec3::from_array(self.state.velocity) + forward * -150.0).to_array();
        self.state.velocity[2] += 128.0;
        let parts = self.jump_parts();
        self.wall_animation(
            parts,
            BOTH_WALL_FLIP_BACK1,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        );
        self.state.legs_timer -= 600;
        self.set_force_jump_start(self.state.origin[2]);
        command.up_move = 0;
        self.state.force_jump_sound = true;
        super::force_jump::drain_levitation(&mut self.state);
        true
    }

    /// `PM_GroundDistance` (`bg_saber.c:1936-1950`): how far below the player, box and
    /// all, the solid ground is.
    pub(super) fn ground_distance(
        &self,
        bounds: Bounds,
        collision: &impl MovementCollision,
    ) -> f32 {
        let mut down = self.state.origin;
        down[2] -= 4_096.0;
        let trace = collision.trace(
            self.state.origin,
            bounds.minimums,
            bounds.maximums,
            down,
            MASK_SOLID,
        );
        (Vec3::from_array(self.state.origin) - Vec3::from_array(trace.end_position)).length()
    }

    /// `PM_WalkableGroundDistance` (`bg_saber.c:1952-1972`): how far below the player
    /// ground it could stand on is, 4096 if none.
    fn walkable_ground_distance(&self, bounds: Bounds, collision: &impl MovementCollision) -> f32 {
        let mut down = self.state.origin;
        down[2] -= 4_096.0;
        let trace = collision.trace(
            self.state.origin,
            bounds.minimums,
            bounds.maximums,
            down,
            MASK_SOLID,
        );
        if trace.plane_normal[2] < super::MIN_WALK_NORMAL {
            return 4_096.0;
        }
        (Vec3::from_array(self.state.origin) - Vec3::from_array(trace.end_position)).length()
    }

    /// Up a wall pushed at within 32 units (`bg_pmove.c:2488-2569`): at jump level 3 a
    /// run up it (`BOTH_FORCEWALLRUNFLIP_START`, 420 up), at level 2 a flip off it
    /// backwards (`BOTH_WALL_FLIP_BACK1`, 150 back and up).
    fn start_wall_run_up(
        &mut self,
        command: &mut UserCommand,
        bounds: Bounds,
        collision: &impl MovementCollision,
        context: &MoveContext,
    ) {
        let (animation, parts) = if self.state.levitation_level > 2 {
            (BOTH_FORCEWALLRUNFLIP_START, SETANIM_BOTH)
        } else {
            (BOTH_WALL_FLIP_BACK1, self.jump_parts())
        };
        let minimums = [bounds.minimums[0], bounds.minimums[1], 0.0];
        let maximums = [bounds.maximums[0], bounds.maximums[1], 24.0];
        let (forward, _) = yaw_axes(self.state.view_angles[1]);
        let origin = Vec3::from_array(self.state.origin);
        let target = origin + forward * 32.0;
        let trace = collision.trace(
            self.state.origin,
            minimums,
            maximums,
            target.to_array(),
            MASK_SOLID,
        );
        let ideal = vector_normalize(origin - target);
        let facing = f64::from(Vec3::from_array(trace.plane_normal).dot(ideal)) > 0.7;
        if !(trace.fraction < 1.0
            && (Self::struck_non_brush(trace.entity_number, context) || facing))
        {
            return;
        }
        self.state.velocity[0] = 0.0;
        self.state.velocity[1] = 0.0;
        if animation == BOTH_FORCEWALLRUNFLIP_START {
            self.state.velocity[2] = STRENGTH_3 / 2.0;
        } else {
            self.state.velocity =
                (Vec3::from_array(self.state.velocity) + forward * -150.0).to_array();
            self.state.velocity[2] += 150.0;
        }
        self.wall_animation(parts, animation, SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD);
        self.set_force_jump_start(self.state.origin[2]);
        command.up_move = 0;
        self.state.force_jump_sound = true;
        super::force_jump::drain_levitation(&mut self.state);
        (command.right_move, command.forward_move) = (0, 0);
    }

    /// The grab of an upright wall within 8 units in the direction pushed, not moving
    /// away from it (`bg_pmove.c:2570-2648`, `PM_GrabWallForJump` `:1745-1750`).
    fn grab_wall(
        &mut self,
        command: &mut UserCommand,
        bounds: Bounds,
        collision: &impl MovementCollision,
        context: &MoveContext,
    ) {
        let minimums = [bounds.minimums[0], bounds.minimums[1], 0.0];
        let maximums = [bounds.maximums[0], bounds.maximums[1], 24.0];
        let (forward, right) = yaw_axes(self.state.view_angles[1]);
        let (animation, direction) = if command.right_move > 0 {
            (BOTH_FORCEWALLREBOUND_RIGHT, right)
        } else if command.right_move < 0 {
            (BOTH_FORCEWALLREBOUND_LEFT, right * -1.0)
        } else if command.forward_move > 0 {
            (BOTH_FORCEWALLREBOUND_FORWARD, forward)
        } else {
            (BOTH_FORCEWALLREBOUND_BACK, forward * -1.0)
        };
        let origin = Vec3::from_array(self.state.origin);
        let target = origin + direction * 8.0;
        let trace = collision.trace(
            self.state.origin,
            minimums,
            maximums,
            target.to_array(),
            CONTENTS_SOLID,
        );
        let ideal = vector_normalize(origin - target);
        let normal = Vec3::from_array(trace.plane_normal);
        let facing = f64::from(normal.dot(ideal)) > 0.7;
        if trace.fraction < 1.0
            && normal.z.abs() <= 0.2
            && (Self::struck_non_brush(trace.entity_number, context) || facing)
            && Vec3::from_array(self.state.velocity).dot(normal) < 1.0
        {
            self.wall_animation(
                SETANIM_BOTH,
                animation,
                SETANIM_FLAG_RESTART | SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            self.add_event(EV_JUMP, 0);
            self.state.movement_flags |= PMF_STUCK_TO_WALL;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pmove::MovementTrace;

    /// One solid half-space: the points whose distance along `normal` from the plane
    /// through `normal * distance` is negative.
    struct Wall {
        normal: [f32; 3],
        distance: f32,
    }

    impl MovementCollision for Wall {
        fn trace(
            &self,
            start: [f32; 3],
            minimums: [f32; 3],
            maximums: [f32; 3],
            end: [f32; 3],
            _content_mask: u32,
        ) -> MovementTrace {
            let normal = Vec3::from_array(self.normal);
            // The box's corner deepest into the wall.
            let corner = Vec3::from_array(std::array::from_fn(|axis| {
                if self.normal[axis] < 0.0 {
                    maximums[axis]
                } else {
                    minimums[axis]
                }
            }));
            let depth =
                |point: [f32; 3]| normal.dot(Vec3::from_array(point) + corner) - self.distance;
            let (from, to) = (depth(start), depth(end));
            if to >= 0.0 || from < 0.0 {
                return MovementTrace::miss(end);
            }
            let fraction = from / (from - to);
            let position = Vec3::from_array(start).lerp(Vec3::from_array(end), fraction);
            MovementTrace {
                fraction,
                end_position: position.to_array(),
                plane_normal: self.normal,
                ..MovementTrace::miss(end)
            }
        }
    }

    /// A wall 40 units east of the origin, facing west.
    const EAST: Wall = Wall {
        normal: [-1.0, 0.0, 0.0],
        distance: -40.0,
    };

    #[test]
    fn wall_runs_and_the_ledge_flip_report_their_held_view() {
        for step in [8, 7, 4, 3] {
            for (animation, yaw, forward) in [
                (BOTH_WALL_RUN_RIGHT, 90.0, 127),
                (BOTH_WALL_RUN_LEFT, 270.0, 127),
                (BOTH_FORCEWALLRUNFLIP_START, 0.0, 127),
                (BOTH_FORCEWALLRUNFLIP_ALT, 0.0, 0),
            ] {
                let mut predictor = Predictor::from_state(
                    MovementState {
                        health: 100,
                        gravity: 800.0,
                        speed: 250.0,
                        ground_entity_number: ENTITY_NUMBER_NONE,
                        legs_anim: animation,
                        torso_anim: animation,
                        legs_timer: 600,
                        torso_timer: 600,
                        view_angles: [12.0, yaw, 0.0],
                        ..MovementState::default()
                    },
                    Default::default(),
                );
                predictor.predict_command(
                    UserCommand {
                        server_time: step,
                        angles: [0, 4096, 0],
                        forward_move: forward,
                        ..Default::default()
                    },
                    &EAST,
                );
                assert!(predictor.state.wall_view_locked, "{animation} at {step} ms");
                assert!((predictor.state.view_angles[1].rem_euclid(360.0) - yaw).abs() < 0.006);
            }
        }
    }

    #[test]
    fn wall_rebound_locks_the_view_until_release_but_japlus_leaves_it_free() {
        for step in [8, 7, 4, 3] {
            for free_wall_look in [false, true] {
                let mut predictor = Predictor::from_state(
                    MovementState {
                        health: 100,
                        gravity: 800.0,
                        speed: 250.0,
                        ground_entity_number: ENTITY_NUMBER_NONE,
                        legs_anim: BOTH_FORCEWALLREBOUND_FORWARD,
                        torso_anim: BOTH_FORCEWALLREBOUND_FORWARD,
                        legs_timer: 600,
                        torso_timer: 600,
                        view_angles: [12.0, 0.0, 0.0],
                        movement_flags: PMF_STUCK_TO_WALL,
                        ..MovementState::default()
                    },
                    super::super::MovementConfig {
                        debug_melee: crate::pmove_debug_melee::DebugMelee {
                            free_wall_look,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                );
                let command = UserCommand {
                    server_time: step,
                    angles: [0, 4096, 0],
                    ..Default::default()
                };
                predictor.predict_command(command, &EAST);
                assert_eq!(predictor.state.wall_view_locked, !free_wall_look);
                assert_ne!(predictor.state.movement_flags & PMF_STUCK_TO_WALL, 0);
                if !free_wall_look {
                    assert_eq!(predictor.state.view_angles[1], 0.0);
                    assert!((predictor.state.view_angles[0] - 12.0).abs() < 0.006);
                } else {
                    assert_eq!(predictor.state.view_angles[1], 22.5);
                }
                // No new slice: keep the last slice's lock for presentation.
                predictor.predict_command(command, &EAST);
                assert_eq!(predictor.state.wall_view_locked, !free_wall_look);
                predictor.state.legs_timer = 100;
                predictor.predict_command(
                    UserCommand {
                        server_time: step * 2,
                        ..command
                    },
                    &EAST,
                );
                assert!(!predictor.state.wall_view_locked);
                assert_eq!(predictor.state.movement_flags & PMF_STUCK_TO_WALL, 0);
            }
        }
    }

    fn yaw(legs: u16, view_yaw: f32, wall: &Wall) -> Option<f32> {
        wall_hold_yaw(legs, view_yaw, [0.0; 3], wall).map(|yaw| yaw.rem_euclid(360.0))
    }

    #[test]
    fn a_held_wall_faces_the_player_as_its_pose_needs() {
        // Grabbed ahead: facing the wall (east). Beside: the wall on that side.
        // Behind: facing away from it.
        assert_eq!(yaw(BOTH_FORCEWALLREBOUND_FORWARD, 0.0, &EAST), Some(0.0));
        assert_eq!(yaw(BOTH_FORCEWALLHOLD_FORWARD, 0.0, &EAST), Some(0.0));
        assert_eq!(yaw(BOTH_FORCEWALLREBOUND_RIGHT, 90.0, &EAST), Some(90.0));
        assert_eq!(yaw(BOTH_FORCEWALLHOLD_LEFT, 270.0, &EAST), Some(270.0));
        assert_eq!(yaw(BOTH_FORCEWALLREBOUND_BACK, 180.0, &EAST), Some(180.0));
    }

    #[test]
    fn turning_the_view_does_not_turn_the_held_facing() {
        // JA+ leaves the view free on the wall: the facing follows the wall alone, while
        // the wall is still within the check's reach of the turned view.
        for view in [-60.0, -30.0, 0.0, 25.0, 60.0] {
            assert_eq!(
                yaw(BOTH_FORCEWALLREBOUND_FORWARD, view, &EAST),
                Some(0.0),
                "view {view}"
            );
        }
        let slanted = Wall {
            normal: [-0.6, -0.8, 0.0],
            distance: -30.0,
        };
        let facing = crate::npc_nav::vector_to_yaw(slanted.normal) + 180.0;
        for view in [20.0, 53.0, 80.0] {
            let held = yaw(BOTH_FORCEWALLHOLD_FORWARD, view, &slanted).unwrap();
            assert!(
                (held - facing.rem_euclid(360.0)).abs() < 1e-3,
                "view {view}"
            );
        }
    }

    #[test]
    fn no_facing_without_a_rebound_or_a_wall_in_reach() {
        assert_eq!(yaw(BOTH_INAIR1, 0.0, &EAST), None);
        assert_eq!(yaw(BOTH_FORCEWALLRELEASE_FORWARD, 0.0, &EAST), None);
        // Turned away from the wall: the check, along the view, finds nothing.
        assert_eq!(yaw(BOTH_FORCEWALLREBOUND_FORWARD, 180.0, &EAST), None);
        // Out of reach: 128 units plus the box.
        let far = Wall {
            normal: [-1.0, 0.0, 0.0],
            distance: -200.0,
        };
        assert_eq!(yaw(BOTH_FORCEWALLREBOUND_FORWARD, 0.0, &far), None);
        // Not upright: a slope (|normal z| > 0.2) is no wall to hold.
        let slope = Wall {
            normal: [-0.9, 0.0, 0.436],
            distance: -40.0,
        };
        assert_eq!(yaw(BOTH_FORCEWALLREBOUND_FORWARD, 0.0, &slope), None);
    }
}
