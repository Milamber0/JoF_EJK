//! The Force jump of `PM_CheckJump` (`bg_pmove.c:1812-2052`, `METROID_JUMP`): held, a
//! jump rises past an ordinary one's height on a curve of its level, costs Force while it
//! does, and shows it — a flip in the direction pushed, a leap from a run. Held against
//! the Jedi scripts of `tools/prediction-pmove/onfoot.c`, whose `wire` lines carry the
//! pool, the debounce and the active bit.

use super::{FORCE_LEVITATION_BIT, JUMP_VELOCITY, PMF_JUMP_HELD, Predictor};
use crate::pmove_anim::{
    SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_LEGS, set_animation,
};
use sjk_protocol::{ENTITY_NUMBER_NONE, UserCommand};

/// `forceJumpHeight` and `forceJumpStrength` by level (`bg_pmove.c:88-104`).
const HEIGHTS: [f32; 4] = [32.0, 96.0, 192.0, 384.0];
const STRENGTHS: [f32; 4] = [225.0, 420.0, 590.0, 840.0];
const BOTH_FORCEJUMP1: u16 = 1_151;
const BOTH_FORCEINAIR1: u16 = 1_152;
const BOTH_FORCEINAIRBACK1: u16 = 1_155;
const BOTH_FORCEINAIRLEFT1: u16 = 1_158;
const BOTH_FORCEINAIRRIGHT1: u16 = 1_161;
const BOTH_FORCELAND1: u16 = 1_153;
const BOTH_FORCEJUMPBACK1: u16 = 1_154;
const BOTH_FORCELANDBACK1: u16 = 1_156;
const BOTH_FORCEJUMPLEFT1: u16 = 1_157;
const BOTH_FORCELANDLEFT1: u16 = 1_159;
const BOTH_FORCEJUMPRIGHT1: u16 = 1_160;
const BOTH_FORCELANDRIGHT1: u16 = 1_162;
const BOTH_FLIP_F: u16 = 1_163;
const BOTH_FLIP_B: u16 = 1_164;
const BOTH_FLIP_L: u16 = 1_165;
const BOTH_FLIP_R: u16 = 1_166;

impl Predictor {
    /// `PM_CheckJump`'s `forceJumpFlip` (`bg_pmove.c:1839-1888`): the game's `ForceJump`
    /// launched a jump, whose pose plays now — a flip the way the command pushes (or the
    /// in-air pose where flips are not allowed), the legs alone while the weapon is busy.
    pub(super) fn force_jump_flip(&mut self, command: &UserCommand, allow_flips: bool) {
        let pushed = |forward: u16, back: u16, right: u16, left: u16| match (
            command.forward_move,
            command.right_move,
        ) {
            (forward_move, _) if forward_move > 0 => Some(forward),
            (forward_move, _) if forward_move < 0 => Some(back),
            (_, right_move) if right_move > 0 => Some(right),
            (_, right_move) if right_move < 0 => Some(left),
            _ => None,
        };
        let animation = if allow_flips {
            pushed(BOTH_FLIP_F, BOTH_FLIP_B, BOTH_FLIP_R, BOTH_FLIP_L)
        } else {
            pushed(
                BOTH_FORCEINAIR1,
                BOTH_FORCEINAIRBACK1,
                BOTH_FORCEINAIRRIGHT1,
                BOTH_FORCEINAIRLEFT1,
            )
        }
        .unwrap_or(BOTH_FORCEINAIR1);
        let parts = if self.state.weapon_time != 0 {
            SETANIM_LEGS
        } else {
            SETANIM_BOTH
        };
        self.set_animation_parts(parts, animation, SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD);
        self.state.force_jump_flip = false;
    }

    /// What every `PM_CheckJump` does before it looks at the jump key: a Force jump is
    /// over on the ground or below where it began, and one in progress pays every 300 ms
    /// (200 at level 1).
    pub(super) fn force_jump_upkeep(&mut self, command_time: i32) {
        let state = &mut self.state;
        if state.ground_entity_number != ENTITY_NUMBER_NONE
            || state.origin[2] < state.force_jump_start_height
        {
            state.force_powers_active &= !FORCE_LEVITATION_BIT;
        }
        if state.force_powers_active & FORCE_LEVITATION_BIT == 0
            || state.levitation_debounce >= command_time
        {
            return;
        }
        drain_levitation(state);
        state.levitation_debounce = command_time
            + if state.levitation_level >= 2 {
                300
            } else {
                200
            };
    }

    /// The `METROID_JUMP` block (`bg_pmove.c:1897-2052`): while a latched jump is held and
    /// rising, the level's height and strength curve replaces the vertical velocity before
    /// gravity; past an ordinary jump's height the Force jump proper begins, with its
    /// animation. Whether it ran (`PM_ForceJumpingUp`): then `PM_CheckJump` clears the
    /// command's jump and looks no further.
    pub(super) fn check_force_jump(&mut self, command: &UserCommand, allow_flips: bool) -> bool {
        if self.state.water_level >= 2 || self.state.gravity <= 0.0 {
            return false;
        }
        // `PM_ForceJumpingUp` (`bg_pmove.c:1235-1275`): not after a charged jump let go, not
        // in a special jump, a special saber move or its leg pose, and free to use the power.
        let state = &self.state;
        if state.force_powers_active & FORCE_LEVITATION_BIT == 0 && state.force_jump_charge != 0.0
            || crate::saber_rules::special_jump(state.legs_anim)
            || crate::saber_rules::in_special(state.saber_move)
            || crate::saber_rules::special_attack(state.legs_anim)
            || !crate::pmove_saber_attack::can_levitate_now(
                state,
                command.server_time,
                self.gametype,
            )
        {
            return false;
        }
        if self.state.movement_flags & PMF_JUMP_HELD == 0
            || self.state.velocity[2] <= 0.0
            || self.state.levitation_level == 0
            || self.state.ground_entity_number != ENTITY_NUMBER_NONE
        {
            return false;
        }
        let level = usize::from(self.state.levitation_level.min(3));
        let height = self.state.origin[2] - self.state.force_jump_start_height;
        let pushing = self.state.force_power > 0 && command.up_move >= 10;
        if (height <= HEIGHTS[0] || pushing)
            && height < HEIGHTS[level]
            && self.state.force_jump_start_height != 0.0
        {
            if height > HEIGHTS[0] {
                self.force_jump_animation(command, allow_flips);
            }
            self.state.velocity[2] = (HEIGHTS[level] - height) / HEIGHTS[level] * STRENGTHS[level]
                / 10.0
                + JUMP_VELOCITY;
            self.state.movement_flags |= PMF_JUMP_HELD;
        } else if self.state.velocity[2] > JUMP_VELOCITY {
            self.state.velocity[2] = JUMP_VELOCITY;
        }
        true
    }

    /// Past an ordinary jump's height: the Force jump starts — a flip in the direction
    /// pushed, else (above level 1) a leap along a run of more than 150 units a second —
    /// and, once its animation has played out, moves on to the matching landing pose.
    fn force_jump_animation(&mut self, command: &UserCommand, allow_flips: bool) {
        let Some(lengths) = self.animation_lengths.clone() else {
            if self.state.force_powers_active & FORCE_LEVITATION_BIT == 0 {
                self.state.force_powers_active |= FORCE_LEVITATION_BIT;
                self.state.force_jump_sound = true;
            }
            return;
        };
        let state = &mut self.state;
        let animation = if state.force_powers_active & FORCE_LEVITATION_BIT == 0 {
            // `bg_pmove.c:1909-1910`: the Force jump begins, and the game will sound it.
            state.force_powers_active |= FORCE_LEVITATION_BIT;
            state.force_jump_sound = true;
            let flipping = matches!(
                state.legs_anim,
                BOTH_FLIP_F | BOTH_FLIP_B | BOTH_FLIP_R | BOTH_FLIP_L
            );
            if (command.forward_move != 0 || command.right_move != 0) && !flipping && allow_flips {
                Some(match (command.forward_move, command.right_move) {
                    (forward, _) if forward > 0 => BOTH_FLIP_F,
                    (forward, _) if forward < 0 => BOTH_FLIP_B,
                    (_, right) if right > 0 => BOTH_FLIP_R,
                    _ => BOTH_FLIP_L,
                })
            } else if state.levitation_level > 1 {
                let (sin, cos) = state.view_angles[1].to_radians().sin_cos();
                let [vx, vy, _] = state.velocity;
                let (ahead, right) = (vx * cos + vy * sin, vx * sin - vy * cos);
                if right.abs() > ahead.abs() * 1.5 {
                    if right > 150.0 {
                        Some(BOTH_FORCEJUMPRIGHT1)
                    } else if right < -150.0 {
                        Some(BOTH_FORCEJUMPLEFT1)
                    } else {
                        None
                    }
                } else if ahead > 150.0 {
                    Some(BOTH_FORCEJUMP1)
                } else if ahead < -150.0 {
                    Some(BOTH_FORCEJUMPBACK1)
                } else {
                    None
                }
            } else {
                None
            }
        } else if state.legs_timer < 1 {
            match state.legs_anim {
                BOTH_FORCEJUMP1 => Some(BOTH_FORCELAND1),
                BOTH_FORCEJUMPBACK1 => Some(BOTH_FORCELANDBACK1),
                BOTH_FORCEJUMPLEFT1 => Some(BOTH_FORCELANDLEFT1),
                BOTH_FORCEJUMPRIGHT1 => Some(BOTH_FORCELANDRIGHT1),
                _ => None,
            }
        } else {
            None
        };
        if let Some(animation) = animation {
            // A weapon in use keeps the torso.
            let parts = if state.weapon_time != 0 {
                SETANIM_LEGS
            } else {
                SETANIM_BOTH
            };
            set_animation(
                state,
                parts,
                animation,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                &*lengths,
            );
        }
    }
}

/// `BG_ForcePowerDrain(ps, FP_LEVITATION, n)` (`bg_saber.c:55-110`): whatever `n`, the
/// Force jump's special case — more the faster the player rises, less the higher its jump
/// level — and never below an empty pool.
pub(super) fn drain_levitation(state: &mut super::MovementState) {
    let rising = state.velocity[2];
    let cost = [
        (250.0, 20),
        (200.0, 16),
        (150.0, 12),
        (100.0, 8),
        (50.0, 6),
        (0.0, 4),
    ]
    .into_iter()
    .find(|(above, _)| rising > *above)
    .map_or(0, |(_, cost)| cost);
    let cost = if state.levitation_level > 0 {
        cost / i32::from(state.levitation_level)
    } else {
        cost
    };
    state.force_power = (i32::from(state.force_power) - cost).max(0) as u8;
}
