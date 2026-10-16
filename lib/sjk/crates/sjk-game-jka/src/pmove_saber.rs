//! The saber's part of `PM_Weapon`: `PM_WeaponLightsaber` is [`crate::pmove_lightsaber`]
//! (the moves the controls ask for [`crate::pmove_saber_attack`], a move's pose
//! [`crate::pmove_saber_move`]); here are the quadrant tables they share and the saber's
//! entry for a client's own prediction.

use crate::pmove::MovementState;

use crate::saber_move_data::movement::*;

/// `transitionMove` (`bg_saber.c:387-397`): the move between one quadrant and the next.
pub(crate) const TRANSITION_MOVE: [[u16; 8]; 8] = [
    [0, 76, 77, 78, 79, 80, 81, 0],
    [82, 0, 83, 84, 85, 86, 87, 0],
    [88, 89, 0, 90, 91, 92, 93, 0],
    [94, 95, 96, 0, 97, 98, 99, 0],
    [100, 101, 102, 103, 0, 104, 105, 0],
    [106, 107, 108, 109, 110, 0, 111, 0],
    [112, 113, 114, 115, 116, 117, 0, 0],
    [112, 76, 77, 78, 79, 80, 81, 0],
];

pub(crate) const TRANSITION_ANGLE: [[u16; 8]; 8] = [
    [0, 45, 90, 135, 180, 215, 270, 45],
    [45, 0, 45, 90, 135, 180, 215, 90],
    [90, 45, 0, 45, 90, 135, 180, 135],
    [135, 90, 45, 0, 45, 90, 135, 180],
    [180, 135, 90, 45, 0, 45, 90, 135],
    [215, 180, 135, 90, 45, 0, 45, 90],
    [270, 215, 180, 135, 90, 45, 0, 45],
    [45, 90, 135, 180, 135, 90, 45, 0],
];

/// `saberMoveTransitionAngle` above: how far round a chain from one quadrant to the next
/// turns.
/// Admit all stock styles to early-branch dispatch and explicit deferral accounting —
/// style 0 included: a saber carrier without saber attack (a holocron game hands out no
/// powers) still runs `PM_WeaponLightsaber`, which counts its weapon time down.
pub(crate) fn can_predict(state: &MovementState, lengths: bool) -> bool {
    state.weapon == 3 && state.saber_anim_level <= 7 && lengths
}

/// Whether a move belongs to the ordinary single-saber subset predicted here.
pub fn move_is_predicted(movement: u32) -> bool {
    matches!(movement as u16, LS_NONE..=LS_A_T2B | LS_S_TL2BR..=LS_T1_BL__L)
}
