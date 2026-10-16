//! A saber carrier standing still on uneven ground: `PM_AdjustStandAnimForSlope` with
//! `PM_FootSlopeTrace` (OpenJK `codemp/game/bg_pmove.c:4679-5095`). Each foot's bolt is
//! read from the mover's posed model, a small box is dropped from the bottom of its box
//! under each foot, and the difference in height between where they land picks one of the
//! five slope poses of the stance — stepped toward one pose each 100 ms
//! (`SLOPE_RECALC_INT`).
//!
//! The model is the host's: [`crate::pmove::MoveContext::foot_bolts`] answers the feet where
//! the mover's server-side model puts them, or nothing where it has none (`pm->ghoul2`
//! unset, a non-humanoid, a client predicting without the model), and then the stance is
//! left to `PM_LegsSlopeBackTransition` as before.

use crate::pmove::{MovementCollision, MovementState, PLAYER_CONTENT_MASK};
use crate::pmove_anim::continue_legs;
use sjk_protocol::UserCommand;

/// `LEGS_LEFTUP1` .. `LEGS_S5_RUP1`: the first pose of each group of five (`anims.h`).
const LEGS_LEFTUP1: u16 = 1_422;
const LEGS_RIGHTUP1: u16 = 1_427;
const LEGS_S1_LUP1: u16 = 1_432;
const LEGS_S1_RUP1: u16 = 1_437;
const LEGS_S3_LUP1: u16 = 1_442;
const LEGS_S3_RUP1: u16 = 1_447;
const LEGS_S4_LUP1: u16 = 1_452;
const LEGS_S4_RUP1: u16 = 1_457;
const LEGS_S5_LUP1: u16 = 1_462;
const LEGS_S5_RUP1: u16 = 1_467;
const BOTH_STAND1: u16 = 915;
const BOTH_STAND2: u16 = 917;
const BOTH_STAND3: u16 = 920;
const BOTH_STAND4: u16 = 922;
const BOTH_STAND5: u16 = 923;
const BOTH_SABERFAST_STANCE: u16 = 850;
const BOTH_SABERSLOW_STANCE: u16 = 851;
const BOTH_CROUCH1: u16 = 1_004;
const BOTH_CROUCH1IDLE: u16 = 1_005;
const TORSO_WEAPONREADY1: u16 = 1_400;
const TORSO_WEAPONREADY2: u16 = 1_401;
const TORSO_WEAPONREADY3: u16 = 1_402;
const TORSO_WEAPONREADY10: u16 = 1_404;
/// `SLOPE_RECALC_INT`.
const SLOPE_RECALC_INT: i32 = 100;

/// Whether `animation` is in the five poses from `first`.
fn in_group(animation: u16, first: u16) -> bool {
    (first..first + 5).contains(&animation)
}

/// `PM_FootSlopeTrace` (`bg_pmove.c:4679-4730`): how much higher the left foot lands than
/// the right, the feet `feet` gave, each dropped as a 6x6x1 box from one unit above the
/// bottom of the mover's box (`bottom`) down forty units.
fn foot_slope(feet: ([f32; 3], [f32; 3]), bottom: f32, collision: &dyn MovementCollision) -> f32 {
    const INTERVAL: f32 = 4.0;
    let land = |foot: [f32; 3]| {
        let start = [foot[0], foot[1], bottom + 1.0];
        let end = [start[0], start[1], start[2] - INTERVAL * 10.0];
        collision
            .trace(
                start,
                [-3.0, -3.0, 0.0],
                [3.0, 3.0, 1.0],
                end,
                PLAYER_CONTENT_MASK,
            )
            .end_position[2]
    };
    land(feet.0) - land(feet.1)
}

/// `PM_AdjustStandAnimForSlope` (`bg_pmove.c:4798-5095`) for a mover standing at rest
/// whose feet `feet` read from its model (`None`: no model, nothing done). `minimum_z` is
/// the bottom of its box (`pm->mins[2]`). Returns whether it took a slope pose; the legs
/// then continue in it.
pub(crate) fn adjust_stand_for_slope(
    state: &mut MovementState,
    command: &UserCommand,
    minimum_z: f32,
    feet: Option<([f32; 3], [f32; 3])>,
    collision: &dyn MovementCollision,
) -> bool {
    let Some(feet) = feet else { return false };
    let diff = foot_slope(feet, state.origin[2] + minimum_z, collision);
    let interval = 4.0_f32;
    // Step 4: the pose for the difference, 1 to 5, left or right foot up.
    let steps = [5.0, 4.0, 3.0, 2.0, 1.0];
    let dest = if let Some(at) = steps.iter().position(|step| diff >= interval * step) {
        LEGS_LEFTUP1 + 4 - at as u16
    } else if let Some(at) = steps.iter().position(|step| diff <= interval * -step) {
        LEGS_RIGHTUP1 + 4 - at as u16
    } else {
        return false;
    };
    let mut legs = state.legs_anim;
    // Adjusted for the stance the legs are in.
    let group = |first: u16| first + (dest - LEGS_LEFTUP1);
    let mut dest = match legs {
        BOTH_STAND1 => group(LEGS_S1_LUP1),
        _ if in_group(legs, LEGS_S1_LUP1) || in_group(legs, LEGS_S1_RUP1) => group(LEGS_S1_LUP1),
        BOTH_STAND2
        | BOTH_SABERFAST_STANCE
        | BOTH_SABERSLOW_STANCE
        | BOTH_CROUCH1IDLE
        | BOTH_CROUCH1 => dest,
        _ if in_group(legs, LEGS_LEFTUP1) || in_group(legs, LEGS_RIGHTUP1) => dest,
        BOTH_STAND3 => group(LEGS_S3_LUP1),
        _ if in_group(legs, LEGS_S3_LUP1) || in_group(legs, LEGS_S3_RUP1) => group(LEGS_S3_LUP1),
        BOTH_STAND4 => group(LEGS_S4_LUP1),
        _ if in_group(legs, LEGS_S4_LUP1) || in_group(legs, LEGS_S4_RUP1) => group(LEGS_S4_LUP1),
        BOTH_STAND5 => group(LEGS_S5_LUP1),
        _ if in_group(legs, LEGS_S5_LUP1) || in_group(legs, LEGS_S5_RUP1) => group(LEGS_S5_LUP1),
        _ => return false,
    };
    let time = command.server_time;
    let left_up = [
        LEGS_LEFTUP1,
        LEGS_S1_LUP1,
        LEGS_S3_LUP1,
        LEGS_S4_LUP1,
        LEGS_S5_LUP1,
    ]
    .iter()
    .any(|first| in_group(legs, *first));
    let right_up = [
        LEGS_RIGHTUP1,
        LEGS_S1_RUP1,
        LEGS_S3_RUP1,
        LEGS_S4_RUP1,
        LEGS_S5_RUP1,
    ]
    .iter()
    .any(|first| in_group(legs, *first));
    if left_up || right_up {
        // Steps 5 and 6: one pose at a time toward the one wanted, not at once.
        if dest > legs && state.slope_recalc_time < time {
            legs += 1;
            state.slope_recalc_time = time + SLOPE_RECALC_INT;
        } else if dest < legs && state.slope_recalc_time < time {
            legs -= 1;
            state.slope_recalc_time = time + SLOPE_RECALC_INT;
        } else {
            legs = dest;
        }
        dest = legs;
    } else {
        // From a stand: the stance's first pose on the side that is up.
        let (left, right) = match legs {
            BOTH_STAND1 | TORSO_WEAPONREADY1 | TORSO_WEAPONREADY2 | TORSO_WEAPONREADY3
            | TORSO_WEAPONREADY10 => (LEGS_S1_LUP1, LEGS_S1_RUP1),
            BOTH_STAND2 | BOTH_SABERFAST_STANCE | BOTH_SABERSLOW_STANCE | BOTH_CROUCH1IDLE => {
                (LEGS_LEFTUP1, LEGS_RIGHTUP1)
            }
            BOTH_STAND3 => (LEGS_S3_LUP1, LEGS_S3_RUP1),
            BOTH_STAND4 => (LEGS_S4_LUP1, LEGS_S4_RUP1),
            BOTH_STAND5 => (LEGS_S5_LUP1, LEGS_S5_RUP1),
            _ => return false,
        };
        dest = if in_group(dest, left) {
            left
        } else if in_group(dest, right) {
            right
        } else {
            return false;
        };
        state.slope_recalc_time = time + SLOPE_RECALC_INT;
    }
    // Step 7.
    continue_legs(state, dest);
    true
}
