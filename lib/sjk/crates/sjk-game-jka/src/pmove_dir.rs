//! `PM_SetMovementDir` (`codemp/game/bg_pmove.c:1635-1664`): the eight-sector
//! movement direction `PM_WalkMove` and `PM_AirMove` publish so cgame can
//! rotate the legs (and a quarter of the torso) for strafing.

use crate::UserCommand;
use crate::pmove::MovementState;

/// Update `ps->movementDir` from the command's forward/right input.
///
/// Sectors run counter-clockwise from forward (0) through left (2), back (4)
/// and right (6). Without directional input a pure sideways sector relaxes to
/// its forward diagonal so the player does not stop "too crooked".
pub(crate) fn set_movement_direction(state: &mut MovementState, command: &UserCommand) {
    let forward = command.forward_move.signum();
    let right = command.right_move.signum();
    state.movement_direction = match (right, forward) {
        (0, 0) => match state.movement_direction {
            2 => 1,
            6 => 7,
            current => current,
        },
        (0, 1) => 0,
        (-1, 1) => 1,
        (-1, 0) => 2,
        (-1, _) => 3,
        (0, _) => 4,
        (_, -1) => 5,
        (_, 0) => 6,
        (_, _) => 7,
    };
}
