//! The saber specials that take the view and the keys from the player, from the
//! unmodified reference (`codemp/game/bg_pmove.c`):
//!
//! - `PM_CmdForSaberMoves` (`:9477-9637`, called from `PmoveSingle` at `:10482`): the
//!   pair's jump attack (`BOTH_JUMPATTACK6`), the staff's jump attacks and butterflies
//!   (`BOTH_BUTTERFLY_*`) script their keys and their two hops, and hold the view while
//!   airborne; the staff's back flip attack (`BOTH_JUMPATTACK7`) pushes backwards and hops;
//!   the staff's and the pair's spin attacks hold the view and the keys throughout;
//! - `PmoveSingle`'s own lock (`:10620-10625`), after the wall moves and before
//!   `PM_UpdateViewAngles`: the strong DFA, the fast lunge and the three back attacks hold
//!   the view whatever the mouse does.
//!
//! A held view is `PM_SetPMViewAngle` (`:1311-1322`): `delta_angles` soaks up the
//! command's turn. Held against `tools/game-oracle/saberangles.c`, which turns the view
//! through every one of these moves.

use super::{MAX_CLIENTS, Predictor};
use crate::saber_move_data::movement::{
    LS_A_BACK, LS_A_BACK_CR, LS_A_BACKFLIP_ATK, LS_A_BACKSTAB, LS_A_JUMP_T__B_, LS_A_LUNGE,
    LS_BUTTERFLY_LEFT, LS_BUTTERFLY_RIGHT, LS_JUMPATTACK_DUAL, LS_JUMPATTACK_STAFF_LEFT,
    LS_JUMPATTACK_STAFF_RIGHT, LS_SPINATTACK, LS_SPINATTACK_DUAL,
};
use sjk_protocol::{ENTITY_NUMBER_NONE, UserCommand};

pub(super) const BOTH_JUMPATTACK6: u16 = 861;
pub(super) const BOTH_JUMPATTACK7: u16 = 862;
pub(super) const BOTH_BUTTERFLY_LEFT: u16 = 1_209;
pub(super) const BOTH_BUTTERFLY_RIGHT: u16 = 1_210;
pub(super) const BOTH_BUTTERFLY_FR1: u16 = 1_258;
pub(super) const BOTH_BUTTERFLY_FL1: u16 = 1_259;
/// `EV_JUMP`.
const EV_JUMP: u16 = 16;

/// The pair's or the staff's jump attack or butterfly, its legs playing its own animation
/// (`bg_pmove.c:9480-9484`).
fn jump_attack(legs: u16, saber_move: u32) -> bool {
    let wanted = match legs {
        BOTH_JUMPATTACK6 => LS_JUMPATTACK_DUAL,
        BOTH_BUTTERFLY_FL1 => LS_JUMPATTACK_STAFF_LEFT,
        BOTH_BUTTERFLY_FR1 => LS_JUMPATTACK_STAFF_RIGHT,
        BOTH_BUTTERFLY_RIGHT => LS_BUTTERFLY_RIGHT,
        BOTH_BUTTERFLY_LEFT => LS_BUTTERFLY_LEFT,
        _ => return false,
    };
    saber_move == u32::from(wanted)
}

/// `bg_pmove.c:10620-10625`: the moves whose view `PmoveSingle` holds before
/// `PM_UpdateViewAngles` — the DFA, the lunge and the back attacks.
pub(super) fn move_holds_view(saber_move: u32) -> bool {
    [
        LS_A_JUMP_T__B_,
        LS_A_LUNGE,
        LS_A_BACK_CR,
        LS_A_BACK,
        LS_A_BACKSTAB,
    ]
    .into_iter()
    .any(|held| saber_move == u32::from(held))
}

impl Predictor {
    /// `PM_AnimLength` (`bg_panimate.c:1601-1607`); nothing for a predictor without an
    /// animation table.
    fn saber_animation_length(&self, animation: u16) -> i32 {
        self.animation_lengths
            .as_deref()
            .and_then(|lengths| lengths.length_ms(animation))
            .unwrap_or(0)
    }

    /// `PM_SetPMViewAngle(ps, ps->viewangles, ucmd)` with the view `PmoveSingle` has not
    /// updated yet: `view` is the last command's.
    fn hold_saber_view(&mut self, command: &UserCommand, view: [f32; 3]) {
        self.state.view_angles = view;
        crate::pmove_input_freeze::set_view_angle(&mut self.state, command);
    }

    /// `PM_CmdForSaberMoves` (`bg_pmove.c:9477-9637`). `view` is the view the last command
    /// left, which the reference still has in `ps->viewangles` here. Returns whether it held
    /// the view, which `PM_UpdateViewAngles` must then recompute.
    pub(super) fn command_for_saber_moves(
        &mut self,
        command: &mut UserCommand,
        view: [f32; 3],
    ) -> bool {
        let legs = self.state.legs_anim;
        let saber_move = self.state.saber_move;
        if jump_attack(legs, saber_move) {
            (command.forward_move, command.right_move, command.up_move) = (0, 0, 0);
            if legs == BOTH_JUMPATTACK6 {
                self.pair_jump_attack(command);
            } else {
                self.staff_jump_attack(command);
            }
            // "can only turn when your feet hit the ground"
            if self.state.ground_entity_number == ENTITY_NUMBER_NONE {
                self.hold_saber_view(command, view);
                return true;
            }
        } else if saber_move == u32::from(LS_A_BACKFLIP_ATK) && legs == BOTH_JUMPATTACK7 {
            let length = self.saber_animation_length(BOTH_JUMPATTACK7);
            let timer = self.state.legs_timer;
            if timer > 800
                && length - timer >= 400
                && self.state.ground_entity_number != ENTITY_NUMBER_NONE
            {
                // Pushed backwards some, and a hop.
                let (back, _) = super::flight::flight_axes([0.0, view[1] + 180.0, 0.0]);
                self.state.velocity = (back * 100.0).to_array();
                self.state.velocity[2] = 300.0;
                self.state.force_jump_start_height = self.state.origin[2];
                self.add_event(EV_JUMP, 0);
                command.up_move = 0;
            }
            (command.forward_move, command.right_move, command.up_move) = (0, 0, 0);
        } else if saber_move == u32::from(LS_SPINATTACK)
            || saber_move == u32::from(LS_SPINATTACK_DUAL)
        {
            (command.forward_move, command.right_move, command.up_move) = (0, 0, 0);
            self.hold_saber_view(command, view);
            return true;
        }
        false
    }

    /// The pair's jump attack (`bg_pmove.c:9492-9523`): forward in the middle of its
    /// animation, and a hop at either of its two jumps while still on something other
    /// than a client.
    fn pair_jump_attack(&mut self, command: &mut UserCommand) {
        let length = self.saber_animation_length(BOTH_JUMPATTACK6);
        let timer = self.state.legs_timer;
        if timer >= 100 && length - timer >= 250 {
            command.forward_move = 127;
        }
        let jumps =
            timer >= 900 && length - timer >= 950 || timer >= 1_600 && length - timer >= 400;
        let ground = self.state.ground_entity_number;
        if jumps && ground != ENTITY_NUMBER_NONE && ground >= MAX_CLIENTS {
            self.state.velocity[2] = 250.0;
            self.state.force_jump_start_height = self.state.origin[2];
            self.add_event(EV_JUMP, 0);
        }
    }

    /// The staff's jump attacks and butterflies (`bg_pmove.c:9524-9583`): a butterfly
    /// strafes until its last 450 ms, a jump attack pushes forward in its middle; a hop
    /// from the ground in the window its animation times.
    fn staff_jump_attack(&mut self, command: &mut UserCommand) {
        let legs = self.state.legs_anim;
        let length = self.saber_animation_length(legs);
        let (window_start, window_end) = if legs == BOTH_BUTTERFLY_LEFT {
            (1_200.0, 1_400.0)
        } else {
            (1_700.0, 1_800.0)
        };
        let timer = self.state.legs_timer;
        if legs == BOTH_BUTTERFLY_RIGHT || legs == BOTH_BUTTERFLY_LEFT {
            if timer > 450 {
                command.right_move = if legs == BOTH_BUTTERFLY_LEFT {
                    -127
                } else {
                    127
                };
            }
        } else if timer >= 100 && length - timer >= 250 {
            command.forward_move = 127;
        }
        let timer = timer as f32;
        if timer >= window_start
            && timer < window_end
            && self.state.ground_entity_number != ENTITY_NUMBER_NONE
        {
            self.state.velocity[2] = if legs == BOTH_BUTTERFLY_LEFT {
                350.0
            } else {
                250.0
            };
            self.state.force_jump_start_height = self.state.origin[2];
            self.add_event(EV_JUMP, 0);
        }
    }
}
