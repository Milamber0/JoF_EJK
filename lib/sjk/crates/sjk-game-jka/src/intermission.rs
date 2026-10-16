//! BaseJKA intermission presentation policy.
//!
//! `codemp/cgame/cg_view.c:1699-1705` copies `playerState_t::origin` and
//! `viewangles` directly into the refdef during `PM_INTERMISSION`; notably it
//! does not add view height. `codemp/game/bg_pmove.c:10701-10703` returns before
//! movement for the same state. User commands are NOT filtered client-side:
//! the stock `CL_CreateCmd` sends raw input, and
//! `codemp/game/g_active.c:838-843` needs the raw `BUTTON_ATTACK` edge to mark
//! the client ready to exit intermission.

use sjk_protocol::PlayerState;

/// `pmtype_t::PM_INTERMISSION` from `codemp/game/bg_public.h:434`.
pub const PM_INTERMISSION: u8 = 7;

/// Authoritative camera supplied by the server for an intermission client.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IntermissionView {
    pub origin: [f32; 3],
    pub angles: [f32; 3],
}

impl IntermissionView {
    /// Extract the exact view used by `CG_CalcViewValues` at
    /// `codemp/cgame/cg_view.c:1699-1705`.
    pub fn from_player_state(player: &PlayerState) -> Option<Self> {
        (player.movement_type() == PM_INTERMISSION).then(|| Self {
            origin: player.origin(),
            angles: player.view_angles(),
        })
    }
}

/// Whether ordinary user movement must be suppressed for this pm type.
pub const fn suppresses_movement(movement_type: u8) -> bool {
    // PM_FREEZE, PM_INTERMISSION, PM_SPINTERMISSION: bg_pmove.c:10697-10703.
    matches!(movement_type, 6..=8)
}
