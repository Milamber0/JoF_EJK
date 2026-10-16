//! Server-only boarding requests from multiplayer `PM_GroundTrace` and
//! `PmoveSingle`. Client prediction never guesses that boarding succeeded.

use crate::pmove::MovementState;
use crate::vehicle_fields::kind;

/// The landing branch's rider checks (`bg_pmove.c:4203-4227`). Vehicle ownership,
/// type and team are checked by the authoritative roster before calling `Board`.
pub fn landing_candidate(state: &MovementState, ground: u16) -> Option<u16> {
    (state.client_num < 32
        && state.vehicle_entity_num == 0
        && (32..1022).contains(&ground)
        && state.zoom_mode == 0
        && !crate::saber_rules::in_special(state.saber_move)
        && state.force_hand_extend == 0
        && state.weapon_time <= 0)
        .then_some(ground)
}

/// Vehicle-side conditions for a normal landing, or for standing on an empty
/// suspended vehicle (`bg_pmove.c:10880-10904`). The latter also permits ships.
pub fn may_board(
    vehicle_kind: i32,
    occupied: bool,
    suspended: bool,
    landed: bool,
    gametype: i32,
    allied_team: i32,
    rider_team: i32,
) -> bool {
    !occupied
        && if landed {
            !matches!(vehicle_kind, kind::FIGHTER | kind::WALKER)
                && (gametype < 6 || allied_team == 0 || allied_team == rider_team)
        } else {
            suspended
        }
}
