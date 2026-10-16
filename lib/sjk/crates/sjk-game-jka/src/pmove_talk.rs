//! The talk balloon: `BUTTON_TALK` in a command becomes `EF_TALK` on the player.
//!
//! A client sends `BUTTON_TALK` while a key catcher (console, menu or chat field) is
//! open (`CL_CmdButtons`, `codemp/client/cl_input.cpp`). `PmoveSingle`
//! (`codemp/game/bg_pmove.c`) sets or clears `EF_TALK` from it on every command, right
//! after the walking-button clear, and cgame floats `gfx/mp/chat_icon` over every
//! player with the flag (`CG_PlayerSprites`, `codemp/cgame/cg_players.c`).

/// `BUTTON_TALK` (`codemp/qcommon/q_shared.h`): "displays talk balloon and disables
/// actions".
pub const BUTTON_TALK: u16 = 2;
/// `EF_TALK` (`codemp/game/bg_public.h`): "draw a talk balloon".
pub const EF_TALK: u32 = 1 << 13;

/// `CL_CmdButtons`: a command made while a key catcher is open also carries
/// `BUTTON_TALK`.
pub fn command_buttons(buttons: u16, key_catcher: bool) -> u16 {
    if key_catcher {
        buttons | BUTTON_TALK
    } else {
        buttons
    }
}

/// `PmoveSingle`'s "set the talk balloon flag": `EF_TALK` follows `BUTTON_TALK`.
pub fn set_talk_flag(entity_flags: &mut u32, buttons: u16) {
    if buttons & BUTTON_TALK != 0 {
        *entity_flags |= EF_TALK;
    } else {
        *entity_flags &= !EF_TALK;
    }
}
