//! The `noclip` cheat for players: `Cmd_Noclip_f` (OpenJK `codemp/game/g_cmds.c`), which
//! toggles `client->noclip`, and what `ClientThink_real` (`g_active.c`) makes of the flag.
//!
//! The command is `CMD_CHEAT | CMD_ALIVE | CMD_NOINTERMISSION`: the server checks those
//! gates before it toggles. While the flag is on, every think puts the player in
//! `PM_NOCLIP` — ahead of a disintegration, of death and of a grip — and `Pmove` flies it
//! through everything (`PM_NoclipMove`, [`crate::pmove`]). The client predicts the same
//! move from the `pm_type` the snapshot carries. The flag also keeps the player out of
//! `G_TouchTriggers`, out of `P_WorldEffects`' drowning and burning
//! ([`crate::world_effects`]) and out of `G_Damage`. `ClientSpawn` clears the whole
//! client, so a respawn or a team change ends it.

/// `PM_NORMAL`, `PM_NOCLIP`, `PM_DEAD` (`bg_public.h`).
const PM_NORMAL: u8 = 0;
const PM_NOCLIP: u8 = 3;
const PM_DEAD: u8 = 5;

/// `Cmd_Noclip_f`: the flag toggled, and the line the player is printed
/// (`print "noclip ON\n"` or `print "noclip OFF\n"`).
pub fn toggle(noclip: &mut bool) -> &'static [u8] {
    *noclip = !*noclip;
    if *noclip {
        b"print \"noclip ON\n\""
    } else {
        b"print \"noclip OFF\n\""
    }
}

/// `ClientThink_real`'s movement type (`g_active.c`, the `client->noclip` chain) as far
/// as noclip decides it, for a player in the world whose type is `current`: `PM_NOCLIP`
/// while the flag is on; once it is off, the type the chain then picks for a player
/// noclip left in `PM_NOCLIP` — still `PM_NOCLIP` when `disintegrated`
/// (`EF_DISINTEGRATION`), `PM_DEAD` at no health, else `PM_NORMAL` (a grip's own type
/// is [`crate::force_dark::gripped_movement_type`]'s to restore). Returns the new type
/// when it changes; every other type is left to the rules that own it.
pub fn movement_type(noclip: bool, current: u8, health: i32, disintegrated: bool) -> Option<u8> {
    let wanted = if noclip {
        PM_NOCLIP
    } else if current != PM_NOCLIP || disintegrated {
        return None;
    } else if health <= 0 {
        PM_DEAD
    } else {
        PM_NORMAL
    };
    (wanted != current).then_some(wanted)
}
